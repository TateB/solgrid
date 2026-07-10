/**
 * LSP test client — spawns the solgrid server and communicates via JSON-RPC.
 *
 * This client is editor-agnostic: it tests the raw LSP protocol that both
 * VSCode and Cursor (and any other LSP-compatible editor) rely on.
 */

import { ChildProcess, spawn } from "child_process";
import { EventEmitter } from "events";
import * as fs from "fs";
import * as path from "path";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface JsonRpcMessage {
  jsonrpc: "2.0";
  id?: number;
  method?: string;
  params?: unknown;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
}

interface NotificationMessage {
  method: string;
  params: unknown;
}

interface PendingNotification {
  method: string;
  filter?: (params: unknown) => boolean;
  resolve: (params: unknown) => void;
  reject: (reason: unknown) => void;
  timer: NodeJS.Timeout;
}

// ---------------------------------------------------------------------------
// Binary resolution
// ---------------------------------------------------------------------------

/**
 * Resolve the solgrid binary path.
 *
 * Priority:
 * 1. SOLGRID_BIN environment variable
 * 2. ../../target/debug/solgrid (relative to editors/vscode)
 * 3. ../../target/release/solgrid (relative to editors/vscode)
 * 4. "solgrid" (on PATH)
 */
export function getSolgridBinaryPath(): string {
  if (process.env.SOLGRID_BIN) {
    return process.env.SOLGRID_BIN;
  }

  const root = path.resolve(__dirname, "../..");
  const debugPath = path.join(root, "../../target/debug/solgrid");
  if (fs.existsSync(debugPath)) {
    return debugPath;
  }

  const releasePath = path.join(root, "../../target/release/solgrid");
  if (fs.existsSync(releasePath)) {
    return releasePath;
  }

  return "solgrid";
}

// ---------------------------------------------------------------------------
// LSP Client
// ---------------------------------------------------------------------------

export class TestLspClient extends EventEmitter {
  private process: ChildProcess | null = null;
  private nextId = 1;
  private pending = new Map<number, PendingRequest>();
  private buffer = Buffer.alloc(0);
  private contentLength = -1;
  private closing = false;
  private notificationBacklog: NotificationMessage[] = [];
  private notificationWaiters: PendingNotification[] = [];

  /**
   * Start the solgrid server process.
   */
  start(binaryPath?: string): void {
    const bin = binaryPath ?? getSolgridBinaryPath();
    this.closing = false;
    this.notificationBacklog = [];
    this.buffer = Buffer.alloc(0);
    this.contentLength = -1;
    const child = spawn(bin, ["server"], {
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.process = child;

    child.stdout!.on("data", (data: Buffer) => {
      this.onData(data);
    });

    child.stderr!.on("data", (data: Buffer) => {
      // Log server stderr for debugging but don't fail
      const text = data.toString("utf-8").trim();
      if (text) {
        // Suppress noisy output; emit event for tests that care
        this.emit("stderr", text);
      }
    });

    child.stdin!.on("error", (error: NodeJS.ErrnoException) => {
      if (!this.isExpectedShutdownError(error)) {
        this.emit("stderr", `LSP stdin error: ${error.message}`);
      }
    });

    child.on("error", (error: NodeJS.ErrnoException) => {
      if (!this.isExpectedShutdownError(error)) {
        this.emit("stderr", `LSP process error: ${error.message}`);
      }
      this.rejectPending(error);
    });

    child.once("close", (code, signal) => {
      if (this.process === child) {
        this.process = null;
      }
      this.rejectPending(
        new Error(
          `LSP server closed with ${
            signal ? `signal ${signal}` : `code ${code}`
          }`
        )
      );
      this.emit("exit", code);
    });
  }

  /**
   * Send a JSON-RPC request and wait for the response.
   */
  async request<T = unknown>(
    method: string,
    params?: unknown
  ): Promise<T> {
    const id = this.nextId++;
    const message: JsonRpcMessage = {
      jsonrpc: "2.0",
      id,
      method,
    };
    if (params !== undefined) {
      message.params = params;
    }

    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: resolve as (value: unknown) => void,
        reject,
      });
      if (!this.send(message)) {
        this.pending.delete(id);
        reject(new Error(`Cannot send LSP request "${method}": server is not running`));
      }
    });
  }

  /**
   * Send a JSON-RPC notification (no response expected).
   */
  notify(method: string, params?: unknown): void {
    const message: JsonRpcMessage = {
      jsonrpc: "2.0",
      method,
    };
    if (params !== undefined) {
      message.params = params;
    }
    this.send(message);
    if (method === "exit") {
      this.closing = true;
      this.process?.stdin?.end();
    }
  }

  /**
   * Wait for a server-initiated notification.
   * Optionally filter by a predicate on the params.
   */
  waitForNotification(
    method: string,
    filter?: (params: unknown) => boolean,
    timeoutMs = 15000
  ): Promise<unknown> {
    const bufferedMatches = this.notificationBacklog.filter(
      (message) =>
        message.method === method && this.matchesNotification(filter, message.params)
    );
    const latestBuffered = bufferedMatches.at(-1);
    if (latestBuffered) {
      this.notificationBacklog = this.notificationBacklog.filter(
        (message) =>
          message.method !== method ||
          !this.matchesNotification(filter, message.params)
      );
      return Promise.resolve(latestBuffered.params);
    }

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const index = this.notificationWaiters.indexOf(waiter);
        if (index >= 0) {
          this.notificationWaiters.splice(index, 1);
        }
        reject(
          new Error(
            `Timeout waiting for notification "${method}" after ${timeoutMs}ms`
          )
        );
      }, timeoutMs);

      const waiter: PendingNotification = {
        method,
        filter,
        resolve,
        reject,
        timer,
      };
      this.notificationWaiters.push(waiter);
    });
  }

  discardNotifications(
    method: string,
    filter?: (params: unknown) => boolean
  ): void {
    this.notificationBacklog = this.notificationBacklog.filter(
      (message) =>
        message.method !== method ||
        !this.matchesNotification(filter, message.params)
    );
  }

  waitForNotificationQuiescence(
    method: string,
    filter?: (params: unknown) => boolean,
    quietMs = 250,
    timeoutMs = 5000
  ): Promise<void> {
    return new Promise((resolve, reject) => {
      let quietTimer: NodeJS.Timeout;
      let settled = false;
      const cleanup = (): void => {
        clearTimeout(quietTimer);
        clearTimeout(timeoutTimer);
        this.removeListener("notification", handler);
        this.removeListener("exit", exitHandler);
      };
      const fail = (error: Error): void => {
        if (settled) {
          return;
        }
        settled = true;
        cleanup();
        reject(error);
      };
      const timeoutTimer = setTimeout(() => {
        fail(
          new Error(
            `Notifications for "${method}" did not settle within ${timeoutMs}ms`
          )
        );
      }, timeoutMs);
      const settle = (): void => {
        if (settled) {
          return;
        }
        settled = true;
        cleanup();
        resolve();
      };
      const resetQuietTimer = (): void => {
        clearTimeout(quietTimer);
        quietTimer = setTimeout(settle, quietMs);
      };
      const handler = (message: NotificationMessage): void => {
        if (
          message.method === method &&
          this.matchesNotification(filter, message.params)
        ) {
          resetQuietTimer();
        }
      };
      const exitHandler = (code: number | null): void => {
        fail(
          new Error(
            `LSP server exited with code ${code} before notifications settled`
          )
        );
      };

      this.on("notification", handler);
      this.on("exit", exitHandler);
    });
  }

  /**
   * Wait for a server-initiated request.
   */
  waitForRequest(
    method: string,
    filter?: (params: unknown) => boolean,
    timeoutMs = 15000
  ): Promise<unknown> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.removeListener("request", handler);
        reject(
          new Error(
            `Timeout waiting for request "${method}" after ${timeoutMs}ms`
          )
        );
      }, timeoutMs);

      const handler = (msg: { method: string; params: unknown }) => {
        if (msg.method === method) {
          if (!filter || filter(msg.params)) {
            clearTimeout(timer);
            this.removeListener("request", handler);
            resolve(msg.params);
          }
        }
      };

      this.on("request", handler);
    });
  }

  /**
   * Send shutdown request followed by exit notification.
   */
  async shutdown(): Promise<void> {
    this.closing = true;
    try {
      await this.request("shutdown", undefined);
    } catch {
      // Server may have already exited
    }
    this.notify("exit", undefined);
  }

  /**
   * Force-kill the server process.
   */
  kill(): void {
    this.closing = true;
    if (this.process) {
      this.process.kill("SIGTERM");
      this.process = null;
    }
    this.rejectPending(new Error("LSP server was terminated"));
  }

  /**
   * Check if the process is still running.
   */
  get isRunning(): boolean {
    return this.process !== null && this.process.exitCode === null;
  }

  // -------------------------------------------------------------------------
  // Internal
  // -------------------------------------------------------------------------

  private send(message: JsonRpcMessage): boolean {
    const body = JSON.stringify(message);
    const header = `Content-Length: ${Buffer.byteLength(body, "utf-8")}\r\n\r\n`;
    const stdin = this.process?.stdin;
    if (!stdin || stdin.destroyed || stdin.writableEnded) {
      return false;
    }
    try {
      stdin.write(header + body, "utf-8", (error) => {
        if (error && !this.isExpectedShutdownError(error)) {
          this.emit("stderr", `LSP write error: ${error.message}`);
        }
      });
      return true;
    } catch (error) {
      if (
        error instanceof Error &&
        !this.isExpectedShutdownError(error as NodeJS.ErrnoException)
      ) {
        this.emit("stderr", `LSP write error: ${error.message}`);
      }
      return false;
    }
  }

  feedServerBytesForTests(chunk: Buffer): void {
    this.onData(chunk);
  }

  private onData(chunk: Buffer): void {
    this.buffer = Buffer.concat([this.buffer, chunk]);
    this.parseMessages();
  }

  private parseMessages(): void {
    while (true) {
      if (this.contentLength === -1) {
        // Look for Content-Length header
        const headerEnd = this.buffer.indexOf("\r\n\r\n");
        if (headerEnd === -1) return;

        const header = this.buffer.subarray(0, headerEnd).toString("ascii");
        const match = header.match(/Content-Length:\s*(\d+)/i);
        if (!match) {
          // Skip malformed header
          this.buffer = this.buffer.subarray(headerEnd + 4);
          continue;
        }

        this.contentLength = parseInt(match[1], 10);
        this.buffer = this.buffer.subarray(headerEnd + 4);
      }

      // Check if we have enough data for the body
      if (this.buffer.length < this.contentLength) return;

      const bodyStr = this.buffer
        .subarray(0, this.contentLength)
        .toString("utf-8");
      this.buffer = this.buffer.subarray(this.contentLength);
      this.contentLength = -1;

      try {
        const message: JsonRpcMessage = JSON.parse(bodyStr);
        this.handleMessage(message);
      } catch {
        // Skip malformed JSON
      }
    }
  }

  private handleMessage(message: JsonRpcMessage): void {
    if (message.id !== undefined && message.id !== null && !message.method) {
      // Response to a request we sent
      const pending = this.pending.get(message.id);
      if (pending) {
        this.pending.delete(message.id);
        if (message.error) {
          pending.reject(
            new Error(
              `LSP error ${message.error.code}: ${message.error.message}`
            )
          );
        } else {
          pending.resolve(message.result);
        }
      }
    } else if (message.method && message.id === undefined) {
      // Server-initiated notification
      const notification = {
        method: message.method,
        params: message.params,
      };
      this.dispatchNotification(notification);
      this.emit("notification", notification);
    } else if (message.method && message.id !== undefined) {
      // Server-initiated request (e.g., workspace/configuration)
      this.emit("request", {
        method: message.method,
        params: message.params,
      });
      // Respond with empty result
      this.send({
        jsonrpc: "2.0",
        id: message.id,
        result: null,
      });
    }
  }

  private dispatchNotification(message: NotificationMessage): void {
    const waiterIndex = this.notificationWaiters.findIndex(
      (waiter) =>
        waiter.method === message.method &&
        this.matchesNotification(waiter.filter, message.params)
    );
    if (waiterIndex >= 0) {
      const [waiter] = this.notificationWaiters.splice(waiterIndex, 1);
      if (waiter) {
        clearTimeout(waiter.timer);
        waiter.resolve(message.params);
      }
      return;
    }

    this.notificationBacklog.push(message);
    if (this.notificationBacklog.length > 200) {
      this.notificationBacklog.shift();
    }
  }

  private matchesNotification(
    filter: ((params: unknown) => boolean) | undefined,
    params: unknown
  ): boolean {
    if (!filter) {
      return true;
    }
    try {
      return filter(params);
    } catch {
      return false;
    }
  }

  private rejectPending(error: Error): void {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
    for (const waiter of this.notificationWaiters) {
      clearTimeout(waiter.timer);
      waiter.reject(error);
    }
    this.notificationWaiters = [];
  }

  private isExpectedShutdownError(error: NodeJS.ErrnoException): boolean {
    return this.closing && (error.code === "EPIPE" || error.code === "ERR_STREAM_DESTROYED");
  }
}
