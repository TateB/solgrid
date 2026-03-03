/**
 * LSP Server Lifecycle Tests
 *
 * Tests the initialize/initialized/shutdown/exit protocol and
 * verifies the server reports correct capabilities.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import { initializeServer, InitializeResult } from "./helpers";

describe("LSP Server Lifecycle", () => {
  let client: TestLspClient;

  beforeEach(() => {
    client = new TestLspClient();
    client.start();
  });

  afterEach(() => {
    client.kill();
  });

  it("responds to initialize with capabilities", async () => {
    const result = await initializeServer(client);
    expect(result).toBeDefined();
    expect(result.capabilities).toBeDefined();
  });

  it("reports server info with name and version", async () => {
    const result = await initializeServer(client);
    expect(result.serverInfo).toBeDefined();
    expect(result.serverInfo!.name).toBe("solgrid");
    expect(result.serverInfo!.version).toBeDefined();
  });

  it("declares full text document sync", async () => {
    const result = await initializeServer(client);
    const sync = result.capabilities.textDocumentSync;
    expect(sync).toBeDefined();
    // Full sync = 1 (TextDocumentSyncKind.Full)
    if (typeof sync === "object" && sync !== null) {
      expect(sync.change).toBe(1);
      expect(sync.openClose).toBe(true);
    }
  });

  it("declares willSaveWaitUntil support", async () => {
    const result = await initializeServer(client);
    const sync = result.capabilities.textDocumentSync;
    if (typeof sync === "object" && sync !== null) {
      expect(sync.willSaveWaitUntil).toBe(true);
    }
  });

  it("declares code action provider with expected kinds", async () => {
    const result = await initializeServer(client);
    expect(result.capabilities.codeActionProvider).toBeDefined();
    const provider = result.capabilities.codeActionProvider as {
      codeActionKinds?: string[];
    };
    if (provider.codeActionKinds) {
      expect(provider.codeActionKinds).toContain("quickfix");
      expect(provider.codeActionKinds).toContain("refactor");
      expect(provider.codeActionKinds).toContain("refactor.rewrite");
      expect(provider.codeActionKinds).toContain("source.fixAll");
    }
  });

  it("declares document formatting provider", async () => {
    const result = await initializeServer(client);
    expect(result.capabilities.documentFormattingProvider).toBeTruthy();
  });

  it("declares range formatting provider", async () => {
    const result = await initializeServer(client);
    expect(result.capabilities.documentRangeFormattingProvider).toBeTruthy();
  });

  it("declares hover provider", async () => {
    const result = await initializeServer(client);
    expect(result.capabilities.hoverProvider).toBeTruthy();
  });

  it("declares completion provider with trigger characters", async () => {
    const result = await initializeServer(client);
    const completion = result.capabilities.completionProvider;
    expect(completion).toBeDefined();
    expect(completion!.triggerCharacters).toBeDefined();
    expect(completion!.triggerCharacters).toContain("/");
    expect(completion!.triggerCharacters).toContain(" ");
  });

  it("handles shutdown request gracefully", async () => {
    await initializeServer(client);
    const result = await client.request("shutdown", undefined);
    // LSP spec: shutdown returns null
    expect(result).toBeNull();
  });

  it("exits cleanly after shutdown + exit", async () => {
    await initializeServer(client);

    // Send shutdown request
    const result = await client.request("shutdown", undefined);
    expect(result).toBeNull();

    // Send exit notification
    client.notify("exit", undefined);

    // tower-lsp-server v0.21+ closes transport ~1s after exit notification.
    // The server process should terminate on its own within 5s.
    const exited = await Promise.race([
      new Promise<boolean>((resolve) => {
        client.on("exit", () => resolve(true));
      }),
      new Promise<boolean>((resolve) => {
        setTimeout(() => resolve(false), 5000);
      }),
    ]);

    expect(exited).toBe(true);
  });
});
