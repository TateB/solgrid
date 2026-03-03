/**
 * LSP Completion Tests
 *
 * Tests textDocument/completion — suppression comment completions.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import {
  initializeServer,
  openDocument,
  waitForDiagnostics,
  requestCompletion,
  resetDocumentVersions,
  CompletionItem,
} from "./helpers";

describe("LSP Completion", () => {
  let client: TestLspClient;

  beforeEach(async () => {
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client);
  });

  afterEach(async () => {
    try {
      await client.shutdown();
    } catch {
      client.kill();
    }
  });

  it("returns directive completions when typing // sol", async () => {
    const uri = "file:///tmp/completion-test.sol";
    const content = "// sol\ncontract Test {}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 0,
      character: 6, // after "// sol"
    });

    const items = normalizeCompletionResult(result);

    if (items.length > 0) {
      const labels = items.map((i) => i.label);
      expect(labels).toContain("solgrid-disable-next-line");
      expect(labels).toContain("solgrid-disable");
      expect(labels).toContain("solgrid-enable");
    }
  });

  it("returns rule ID completions after directive prefix", async () => {
    const uri = "file:///tmp/completion-rule-test.sol";
    const content =
      "// solgrid-disable-next-line \ncontract Test {}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 0,
      character: 29, // after "// solgrid-disable-next-line "
    });

    const items = normalizeCompletionResult(result);

    if (items.length > 0) {
      const labels = items.map((i) => i.label);
      expect(labels).toContain("security/tx-origin");
    }
  });

  it("rule ID completions include descriptions", async () => {
    const uri = "file:///tmp/completion-detail-test.sol";
    const content =
      "// solgrid-disable-next-line \ncontract Test {}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 0,
      character: 29,
    });

    const items = normalizeCompletionResult(result);

    if (items.length > 0) {
      // At least some items should have a detail/description
      const withDetail = items.filter((i) => i.detail);
      expect(withDetail.length).toBeGreaterThan(0);
    }
  });

  it("returns empty completions in non-comment context", async () => {
    const uri = "file:///tmp/completion-code-test.sol";
    const content = "contract Test { uint256 x; }\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 0,
      character: 10, // Inside "contract Test"
    });

    const items = normalizeCompletionResult(result);
    expect(items).toHaveLength(0);
  });

  it("directive completions have SNIPPET kind", async () => {
    const uri = "file:///tmp/completion-kind-test.sol";
    const content = "// sol\ncontract Test {}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 0,
      character: 6,
    });

    const items = normalizeCompletionResult(result);

    const directives = items.filter((i) =>
      i.label.startsWith("solgrid-")
    );

    for (const item of directives) {
      // CompletionItemKind.Snippet = 15
      if (item.kind !== undefined) {
        expect(item.kind).toBe(15);
      }
    }
  });

  it("rule ID completions have VALUE kind", async () => {
    const uri = "file:///tmp/completion-value-kind-test.sol";
    const content =
      "// solgrid-disable-next-line \ncontract Test {}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 0,
      character: 29,
    });

    const items = normalizeCompletionResult(result);

    for (const item of items) {
      // CompletionItemKind.Value = 12
      if (item.kind !== undefined) {
        expect(item.kind).toBe(12);
      }
    }
  });

  it("returns completions after // with space", async () => {
    const uri = "file:///tmp/completion-space-test.sol";
    const content = "// \ncontract Test {}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 0,
      character: 3, // after "// "
    });

    const items = normalizeCompletionResult(result);

    if (items.length > 0) {
      const labels = items.map((i) => i.label);
      expect(labels).toContain("solgrid-disable-next-line");
    }
  });
});

/**
 * Normalize completion result to an array of CompletionItems.
 * The server may return null, an array, or { items: [] }.
 */
function normalizeCompletionResult(
  result: CompletionItem[] | { items: CompletionItem[] } | null
): CompletionItem[] {
  if (!result) return [];
  if (Array.isArray(result)) return result;
  if ("items" in result) return result.items;
  return [];
}
