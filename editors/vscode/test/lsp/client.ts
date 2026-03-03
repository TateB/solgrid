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

// ---------------------------------------------------------------------------
// Binary resolution
// ---------------------------------------------------------------------------

/**
 * Resolve the solgrid binary path.
 *
 * Priority:
 * 1. SOLGRID_BIN environment variable
 * 2. ../../target/release/solgrid (relative to editors/vscode)
 * 3. ../../target/debug/solgrid (relative to editors/vscode)
 * 4. "solgrid" (on PATH)
 */
export function getSolgridBinaryPath(): string {
  if (process.env.SOLGRID_BIN) {
    return process.env.SOLGRID_BIN;
  }

  const root = path.resolve(__dirname, "../..");
  const releasePath = path.join(root, "../../target/release/solgrid");
  if (fs.existsSync(releasePath)) {
    return releasePath;
  }

  const debugPath = path.join(root, "../../target/debug/solgrid");
  if (fs.existsSync(debugPath)) {
    return debugPath;
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
  private buffer = "";
  private contentLength = -1;

  /**
   * Start the solgrid server process.
   */
  start(binaryPath?: string): void {
    const bin = binaryPath ?? getSolgridBinaryPath();
    this.process = spawn(bin, ["server"], {
      stdio: ["pipe", "pipe", "pipe"],
    });

    this.process.stdout!.on("data", (data: Buffer) => {
      this.onData(data.toString("utf-8"));
    });

    this.process.stderr!.on("data", (data: Buffer) => {
      // Log server stderr for debugging but don't fail
      const text = data.toString("utf-8").trim();
      if (text) {
        // Suppress noisy output; emit event for tests that care
        this.emit("stderr", text);
      }
    });

    this.process.on("exit", (code) => {
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
      this.send(message);
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
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.removeListener("notification", handler);
        reject(
          new Error(
            `Timeout waiting for notification "${method}" after ${timeoutMs}ms`
          )
        );
      }, timeoutMs);

      const handler = (msg: { method: string; params: unknown }) => {
        if (msg.method === method) {
          if (!filter || filter(msg.params)) {
            clearTimeout(timer);
            this.removeListener("notification", handler);
            resolve(msg.params);
          }
        }
      };

      this.on("notification", handler);
    });
  }

  /**
   * Send shutdown request followed by exit notification.
   */
  async shutdown(): Promise<void> {
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
    if (this.process) {
      this.process.kill("SIGTERM");
      this.process = null;
    }
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

  private send(message: JsonRpcMessage): void {
    const body = JSON.stringify(message);
    const header = `Content-Length: ${Buffer.byteLength(body, "utf-8")}\r\n\r\n`;
    this.process!.stdin!.write(header + body, "utf-8");
  }

  private onData(chunk: string): void {
    this.buffer += chunk;
    this.parseMessages();
  }

  private parseMessages(): void {
    while (true) {
      if (this.contentLength === -1) {
        // Look for Content-Length header
        const headerEnd = this.buffer.indexOf("\r\n\r\n");
        if (headerEnd === -1) return;

        const header = this.buffer.substring(0, headerEnd);
        const match = header.match(/Content-Length:\s*(\d+)/i);
        if (!match) {
          // Skip malformed header
          this.buffer = this.buffer.substring(headerEnd + 4);
          continue;
        }

        this.contentLength = parseInt(match[1], 10);
        this.buffer = this.buffer.substring(headerEnd + 4);
      }

      // Check if we have enough data for the body
      const bodyBytes = Buffer.byteLength(this.buffer, "utf-8");
      if (bodyBytes < this.contentLength) return;

      // Extract exactly contentLength bytes
      // Need to handle multi-byte correctly
      const buf = Buffer.from(this.buffer, "utf-8");
      const bodyStr = buf.subarray(0, this.contentLength).toString("utf-8");
      this.buffer = buf.subarray(this.contentLength).toString("utf-8");
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
      this.emit("notification", {
        method: message.method,
        params: message.params,
      });
    } else if (message.method && message.id !== undefined) {
      // Server-initiated request (e.g., workspace/configuration)
      // Respond with empty result
      this.send({
        jsonrpc: "2.0",
        id: message.id,
        result: null,
      });
    }
  }
}
