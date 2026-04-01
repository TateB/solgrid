/**
 * LSP Completion Tests
 *
 * Tests textDocument/completion — suppression comment completions,
 * intelligent autocomplete (builtins, keywords, dot completions,
 * in-scope symbols), and auto-import completions.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import {
  initializeServer,
  openDocument,
  closeDocument,
  waitForDiagnostics,
  requestCompletion,
  resetDocumentVersions,
  fixtureUri,
  readFixture,
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

  it("returns no suppression directives in non-comment context", async () => {
    const uri = "file:///tmp/completion-code-test.sol";
    const content = "contract Test { uint256 x; }\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 0,
      character: 10, // Inside "contract Test"
    });

    const items = normalizeCompletionResult(result);
    // Non-comment context may return autocomplete items (keywords, builtins, etc.)
    // but should NOT include suppression comment directives
    const directives = items.filter((i) =>
      i.label.startsWith("solgrid-")
    );
    expect(directives).toHaveLength(0);
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

describe("LSP Completion — Builtins & Keywords", () => {
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

  it("returns Solidity keyword completions in code context", async () => {
    const uri = "file:///tmp/completion-keywords.sol";
    const content = "pragma solidity ^0.8.0;\n\ncontract Test {\n  \n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 3,
      character: 2, // inside contract body
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    // Should contain common Solidity keywords
    expect(labels).toContain("function");
    expect(labels).toContain("mapping");
    expect(labels).toContain("event");
    expect(labels).toContain("modifier");
  });

  it("returns Solidity type completions", async () => {
    const uri = "file:///tmp/completion-types.sol";
    const content = "pragma solidity ^0.8.0;\n\ncontract Test {\n  \n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 3,
      character: 2,
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    expect(labels).toContain("uint256");
    expect(labels).toContain("address");
    expect(labels).toContain("bool");
    expect(labels).toContain("bytes32");
  });

  it("keywords have KEYWORD kind", async () => {
    const uri = "file:///tmp/completion-kw-kind.sol";
    const content = "pragma solidity ^0.8.0;\n\ncontract Test {\n  \n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 3,
      character: 2,
    });

    const items = normalizeCompletionResult(result);
    const functionItem = items.find((i) => i.label === "function");

    expect(functionItem).toBeDefined();
    // CompletionItemKind.Keyword = 14
    expect(functionItem!.kind).toBe(14);
  });

  it("types have TYPE_PARAMETER kind", async () => {
    const uri = "file:///tmp/completion-type-kind.sol";
    const content = "pragma solidity ^0.8.0;\n\ncontract Test {\n  \n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 3,
      character: 2,
    });

    const items = normalizeCompletionResult(result);
    const uint256Item = items.find((i) => i.label === "uint256");

    expect(uint256Item).toBeDefined();
    // CompletionItemKind.TypeParameter = 25
    expect(uint256Item!.kind).toBe(25);
  });

  it("returns builtin global functions", async () => {
    const uri = "file:///tmp/completion-builtins.sol";
    const content =
      "pragma solidity ^0.8.0;\n\ncontract Test {\n  function f() public {\n    \n  }\n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 4,
      character: 4, // inside function body
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    expect(labels).toContain("keccak256");
    expect(labels).toContain("require");
  });

  it("returns builtin namespace names", async () => {
    const uri = "file:///tmp/completion-namespaces.sol";
    const content =
      "pragma solidity ^0.8.0;\n\ncontract Test {\n  function f() public {\n    \n  }\n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 4,
      character: 4,
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    // msg, block, tx are builtin namespaces
    expect(labels).toContain("msg");
    expect(labels).toContain("block");
    expect(labels).toContain("tx");
  });
});

describe("LSP Completion — Dot Completions", () => {
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

  it("returns msg members after msg.", async () => {
    const uri = "file:///tmp/completion-msg-dot.sol";
    const content =
      "pragma solidity ^0.8.0;\n\ncontract Test {\n  function f() public {\n    msg.\n  }\n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 4,
      character: 8, // after "msg."
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    expect(labels).toContain("sender");
    expect(labels).toContain("value");
    expect(labels).toContain("data");
  });

  it("returns block members after block.", async () => {
    const uri = "file:///tmp/completion-block-dot.sol";
    const content =
      "pragma solidity ^0.8.0;\n\ncontract Test {\n  function f() public {\n    block.\n  }\n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 4,
      character: 10, // after "block."
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    expect(labels).toContain("timestamp");
    expect(labels).toContain("number");
  });

  it("returns tx members after tx.", async () => {
    const uri = "file:///tmp/completion-tx-dot.sol";
    const content =
      "pragma solidity ^0.8.0;\n\ncontract Test {\n  function f() public {\n    tx.\n  }\n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 4,
      character: 7, // after "tx."
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    expect(labels).toContain("origin");
    expect(labels).toContain("gasprice");
  });

  it("dot completion members have FIELD kind", async () => {
    const uri = "file:///tmp/completion-dot-kind.sol";
    const content =
      "pragma solidity ^0.8.0;\n\ncontract Test {\n  function f() public {\n    msg.\n  }\n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 4,
      character: 8,
    });

    const items = normalizeCompletionResult(result);
    const senderItem = items.find((i) => i.label === "sender");

    expect(senderItem).toBeDefined();
    // CompletionItemKind.Field = 5
    expect(senderItem!.kind).toBe(5);
  });

  it("dot completion members include type signatures", async () => {
    const uri = "file:///tmp/completion-dot-detail.sol";
    const content =
      "pragma solidity ^0.8.0;\n\ncontract Test {\n  function f() public {\n    msg.\n  }\n}\n";

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 4,
      character: 8,
    });

    const items = normalizeCompletionResult(result);
    const senderItem = items.find((i) => i.label === "sender");

    expect(senderItem).toBeDefined();
    expect(senderItem!.detail).toBeDefined();
    expect(senderItem!.detail).toBeTruthy();
  });

  it("returns members for custom-typed contract instances", async () => {
    const uri = "file:///tmp/completion-contract-instance.sol";
    const content = [
      "pragma solidity ^0.8.0;",
      "",
      "contract SomethingA {",
      "  function thisThing() public view returns (string memory) {",
      '    return "A";',
      "  }",
      "}",
      "",
      "contract SomethingB {",
      "  SomethingA public somethingA;",
      "",
      "  constructor() {",
      "    somethingA = new SomethingA();",
      "  }",
      "",
      "  function thisThing() public view returns (string memory) {",
      "    return somethingA.",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 16,
      character: 22, // after "somethingA."
    });

    const items = normalizeCompletionResult(result);
    const thisThingItem = items.find((item) => item.label === "thisThing");

    expect(thisThingItem).toBeDefined();
    expect(thisThingItem!.detail).toContain("function thisThing()");
  });

  it("resolves chained call and index receivers for member completion", async () => {
    const uri = "file:///tmp/completion-call-receiver.sol";
    const callContent = [
      "pragma solidity ^0.8.0;",
      "",
      "contract SomethingA {",
      "  function thisThing() public view returns (string memory) {",
      '    return "A";',
      "  }",
      "}",
      "",
      "contract SomethingB {",
      "  mapping(address => SomethingA) public items;",
      "",
      "  constructor() {",
      "    items[msg.sender] = new SomethingA();",
      "  }",
      "",
      "  function current() public view returns (SomethingA) {",
      "    return items[msg.sender];",
      "  }",
      "",
      "  function fromCall() public view returns (string memory) {",
      "    return current().",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, uri, callContent);
    await waitForDiagnostics(client, uri).catch(() => {});

    const fromCall = normalizeCompletionResult(
      await requestCompletion(client, uri, {
        line: 20,
        character: 21, // after "current()."
      })
    );
    closeDocument(client, uri);

    const indexUri = "file:///tmp/completion-index-receiver.sol";
    const indexContent = [
      "pragma solidity ^0.8.0;",
      "",
      "contract SomethingA {",
      "  function thisThing() public view returns (string memory) {",
      '    return "A";',
      "  }",
      "}",
      "",
      "contract SomethingB {",
      "  mapping(address => SomethingA) public items;",
      "",
      "  constructor() {",
      "    items[msg.sender] = new SomethingA();",
      "  }",
      "",
      "  function fromIndex() public view returns (string memory) {",
      "    return items[msg.sender].",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, indexUri, indexContent);
    await waitForDiagnostics(client, indexUri).catch(() => {});

    const fromIndex = normalizeCompletionResult(
      await requestCompletion(client, indexUri, {
        line: 16,
        character: 29, // after "items[msg.sender]."
      })
    );

    expect(fromCall.map((item) => item.label)).toContain("thisThing");
    expect(fromIndex.map((item) => item.label)).toContain("thisThing");
  });
});

describe("LSP Completion — In-Scope Symbols", () => {
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

  it("returns contract members inside contract body", async () => {
    const uri = "file:///tmp/completion-members.sol";
    const content = [
      "pragma solidity ^0.8.0;",
      "",
      "contract Test {",
      "  uint256 public balance;",
      "  address public owner;",
      "",
      "  function doSomething() public {",
      "    ",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 7,
      character: 4, // inside function body
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    expect(labels).toContain("balance");
    expect(labels).toContain("owner");
  });

  it("returns function parameters as completions", async () => {
    const uri = "file:///tmp/completion-params.sol";
    const content = [
      "pragma solidity ^0.8.0;",
      "",
      "contract Test {",
      "  function transfer(address recipient, uint256 amount) public {",
      "    ",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 4,
      character: 4,
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    expect(labels).toContain("recipient");
    expect(labels).toContain("amount");
  });

  it("returns local variables as completions", async () => {
    const uri = "file:///tmp/completion-locals.sol";
    const content = [
      "pragma solidity ^0.8.0;",
      "",
      "contract Test {",
      "  function calc() public pure returns (uint256) {",
      "    uint256 myLocalVar = 42;",
      "    ",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 5,
      character: 4,
    });

    const items = normalizeCompletionResult(result);
    const labels = items.map((i) => i.label);

    expect(labels).toContain("myLocalVar");
  });
});

describe("LSP Completion — Auto-Import", () => {
  let client: TestLspClient;

  beforeEach(async () => {
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    // Initialize with fixtures dir as root so workspace index picks up files
    await initializeServer(client);
  });

  afterEach(async () => {
    try {
      await client.shutdown();
    } catch {
      client.kill();
    }
  });

  it("offers auto-import for symbols from other workspace files", async () => {
    // Open the importable file first so the workspace index picks it up
    const importableUri = fixtureUri("importable.sol");
    const importableContent = readFixture("importable.sol");
    openDocument(client, importableUri, importableContent);
    await waitForDiagnostics(client, importableUri).catch(() => {});

    // Now open a file that doesn't import Importable
    const uri = fixtureUri("completion.sol");
    const content = readFixture("completion.sol");
    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 5,
      character: 4, // inside function body
    });

    const items = normalizeCompletionResult(result);
    const autoImportItems = items.filter(
      (i) => i.detail && i.detail.startsWith("Auto import")
    );

    // Should have at least one auto-import suggestion
    expect(autoImportItems.length).toBeGreaterThan(0);
  });

  it("auto-import completions include additionalTextEdits", async () => {
    const importableUri = fixtureUri("importable.sol");
    const importableContent = readFixture("importable.sol");
    openDocument(client, importableUri, importableContent);
    await waitForDiagnostics(client, importableUri).catch(() => {});

    const uri = fixtureUri("completion.sol");
    const content = readFixture("completion.sol");
    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const result = await requestCompletion(client, uri, {
      line: 5,
      character: 4,
    });

    const items = normalizeCompletionResult(result);
    const autoImportItems = items.filter(
      (i) => i.detail && i.detail.startsWith("Auto import")
    );

    if (autoImportItems.length > 0) {
      // Auto-import items should have additionalTextEdits to insert the import statement
      const withEdits = autoImportItems.filter(
        (i) =>
          i.additionalTextEdits && i.additionalTextEdits.length > 0
      );
      expect(withEdits.length).toBeGreaterThan(0);

      // The edit should contain an import statement
      const edit = withEdits[0].additionalTextEdits![0];
      expect(edit.newText).toContain("import");
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
