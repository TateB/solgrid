import { afterEach, beforeEach, describe, expect, it } from "vitest";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { fileURLToPath, pathToFileURL } from "url";
import { TestLspClient } from "./client";
import {
  closeDocument,
  CallHierarchyItem,
  CodeLens,
  DocumentSymbol,
  requestIncomingCalls,
  initializeServer,
  InlayHint,
  notifyWatchedFilesChanged,
  openDocument,
  requestOutgoingCalls,
  requestPrepareCallHierarchy,
  requestCodeLenses,
  requestDocumentLinks,
  requestDocumentSymbols,
  requestExecuteCommand,
  requestInlayHints,
  requestPrepareRename,
  requestRename,
  requestReferences,
  requestWorkspaceSymbols,
  resetDocumentVersions,
  waitForDiagnostics,
} from "./helpers";

function tempWorkspace(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "solgrid-nav-"));
}

function toUri(filePath: string): string {
  const resolvedPath = fs.existsSync(filePath)
    ? fs.realpathSync(filePath)
    : path.resolve(filePath);
  return pathToFileURL(resolvedPath).toString();
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function canonicalUri(uri: string): string {
  const filePath = fileURLToPath(uri);
  return pathToFileURL(fs.realpathSync(filePath)).toString();
}

function workspaceChangesForUri(
  edit: { changes?: Record<string, unknown[]> } | null | undefined,
  uri: string
): unknown[] | undefined {
  const changes = edit?.changes;
  if (!changes) {
    return undefined;
  }
  const direct = changes[uri];
  if (direct) {
    return direct;
  }
  const canonical = canonicalUri(uri);
  return Object.entries(changes).find(([key]) => canonicalUri(key) === canonical)?.[1];
}

function nestedDocumentSymbols(
  response: DocumentSymbol[] | unknown[] | null
): DocumentSymbol[] {
  return (response ?? []).filter(
    (entry): entry is DocumentSymbol =>
      typeof entry === "object" &&
      entry !== null &&
      "selectionRange" in entry
  );
}

function inlayHintLabel(hint: InlayHint): string {
  return typeof hint.label === "string"
    ? hint.label
    : hint.label.map((part) => part.value).join("");
}

async function waitForWorkspaceSymbol(
  client: TestLspClient,
  query: string,
  expectedName: string
) {
  for (let attempt = 0; attempt < 20; attempt++) {
    const result = await requestWorkspaceSymbols(client, query);
    if (result?.some((symbol) => symbol.name === expectedName)) {
      return result;
    }
    await sleep(100);
  }

  throw new Error(`Timed out waiting for workspace symbol ${expectedName}`);
}

describe("LSP Navigation", () => {
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

  it("returns same-file references with and without declarations", async () => {
    const uri = "file:///tmp/solgrid-references.sol";
    const source = `pragma solidity ^0.8.0;

contract Token {
    function foo(uint256 amount) public pure returns (uint256) {
        uint256 doubled = amount + amount;
        return doubled;
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const position = { line: 3, character: 26 };
    const withoutDeclaration = await requestReferences(client, uri, position, false);
    const withDeclaration = await requestReferences(client, uri, position, true);

    expect(withoutDeclaration).toHaveLength(2);
    expect(withDeclaration).toHaveLength(3);
  });

  it("returns cross-file references for aliased imports", async () => {
    const dir = tempWorkspace();
    const tokenPath = path.join(dir, "Token.sol");
    const mainPath = path.join(dir, "Main.sol");
    const tokenUri = toUri(tokenPath);
    const mainUri = toUri(mainPath);
    const tokenSource = `pragma solidity ^0.8.0;
contract Token {}
`;
    const mainSource = `pragma solidity ^0.8.0;
import {Token as T} from "./Token.sol";
contract Main is T {}
`;

    fs.writeFileSync(tokenPath, tokenSource);
    fs.writeFileSync(mainPath, mainSource);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      await initializeServer(client, toUri(dir));

      openDocument(client, tokenUri, tokenSource);
      openDocument(client, mainUri, mainSource);
      await waitForDiagnostics(client, tokenUri).catch(() => {});
      await waitForDiagnostics(client, mainUri).catch(() => {});

      const references = await requestReferences(
        client,
        tokenUri,
        { line: 1, character: 10 },
        true
      );

      expect(references).toHaveLength(4);
      expect(
        references?.filter(
          (location) => canonicalUri(location.uri) === canonicalUri(mainUri)
        ).length
      ).toBe(3);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("prepares and applies safe same-file rename edits", async () => {
    const uri = "file:///tmp/solgrid-rename.sol";
    const source = `pragma solidity ^0.8.0;

contract Token {
    function run(uint256 amount) external pure returns (uint256) {
        uint256 doubled = amount + amount;
        return doubled;
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const position = { line: 4, character: 16 };
    const prepare = await requestPrepareRename(client, uri, position);
    expect(prepare).toEqual({
      range: {
        start: { line: 4, character: 16 },
        end: { line: 4, character: 23 },
      },
      placeholder: "doubled",
    });

    const edit = await requestRename(client, uri, position, "tripled");
    expect(edit?.changes?.[uri]).toEqual([
      {
        range: {
          start: { line: 4, character: 16 },
          end: { line: 4, character: 23 },
        },
        newText: "tripled",
      },
      {
        range: {
          start: { line: 5, character: 15 },
          end: { line: 5, character: 22 },
        },
        newText: "tripled",
      },
    ]);
  });

  it("prepares and applies safe cross-file rename edits for aliased imports", async () => {
    const dir = tempWorkspace();
    const tokenPath = path.join(dir, "Token.sol");
    const mainPath = path.join(dir, "Main.sol");
    const tokenUri = toUri(tokenPath);
    const mainUri = toUri(mainPath);
    const tokenSource = `pragma solidity ^0.8.0;
contract Token {}
`;
    const mainSource = `pragma solidity ^0.8.0;
import {Token as T} from "./Token.sol";
contract Main is T {}
`;

    fs.writeFileSync(tokenPath, tokenSource);
    fs.writeFileSync(mainPath, mainSource);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      const init = await initializeServer(client, toUri(dir));

      expect(init.capabilities.renameProvider).toBeTruthy();

      openDocument(client, tokenUri, tokenSource);
      openDocument(client, mainUri, mainSource);
      await waitForDiagnostics(client, tokenUri).catch(() => {});
      await waitForDiagnostics(client, mainUri).catch(() => {});

      const prepare = await requestPrepareRename(client, tokenUri, {
        line: 1,
        character: 10,
      });
      expect(prepare).toEqual({
        range: {
          start: { line: 1, character: 9 },
          end: { line: 1, character: 14 },
        },
        placeholder: "Token",
      });

      const edit = await requestRename(client, tokenUri, { line: 1, character: 10 }, "Asset");
      expect(workspaceChangesForUri(edit, tokenUri)).toEqual([
        {
          range: {
            start: { line: 1, character: 9 },
            end: { line: 1, character: 14 },
          },
          newText: "Asset",
        },
      ]);
      expect(workspaceChangesForUri(edit, mainUri)).toEqual([
        {
          range: {
            start: { line: 1, character: 8 },
            end: { line: 1, character: 13 },
          },
          newText: "Asset",
        },
      ]);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("rejects rename from an alias usage site", async () => {
    const dir = tempWorkspace();
    const tokenPath = path.join(dir, "Token.sol");
    const mainPath = path.join(dir, "Main.sol");
    const tokenUri = toUri(tokenPath);
    const mainUri = toUri(mainPath);
    const tokenSource = `pragma solidity ^0.8.0;
contract Token {}
`;
    const mainSource = `pragma solidity ^0.8.0;
import {Token as T} from "./Token.sol";
contract Main is T {}
`;

    fs.writeFileSync(tokenPath, tokenSource);
    fs.writeFileSync(mainPath, mainSource);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      await initializeServer(client, toUri(dir));

      openDocument(client, tokenUri, tokenSource);
      openDocument(client, mainUri, mainSource);
      await waitForDiagnostics(client, tokenUri).catch(() => {});
      await waitForDiagnostics(client, mainUri).catch(() => {});

      expect(
        await requestPrepareRename(client, mainUri, { line: 2, character: 17 })
      ).toBeNull();
      expect(
        await requestRename(client, mainUri, { line: 2, character: 17 }, "Asset")
      ).toBeNull();
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("prepares and applies safe cross-file rename edits for unaliased imports", async () => {
    const dir = tempWorkspace();
    const tokenPath = path.join(dir, "Token.sol");
    const mainPath = path.join(dir, "Main.sol");
    const tokenUri = toUri(tokenPath);
    const mainUri = toUri(mainPath);
    const tokenSource = `pragma solidity ^0.8.0;
contract Token {}
`;
    const mainSource = `pragma solidity ^0.8.0;
import {Token} from "./Token.sol";
contract Main {
    Token token;
}
`;

    fs.writeFileSync(tokenPath, tokenSource);
    fs.writeFileSync(mainPath, mainSource);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      const init = await initializeServer(client, toUri(dir));

      expect(init.capabilities.renameProvider).toBeTruthy();

      openDocument(client, tokenUri, tokenSource);
      openDocument(client, mainUri, mainSource);
      await waitForDiagnostics(client, tokenUri).catch(() => {});
      await waitForDiagnostics(client, mainUri).catch(() => {});

      const prepare = await requestPrepareRename(client, tokenUri, {
        line: 1,
        character: 10,
      });
      expect(prepare).toEqual({
        range: {
          start: { line: 1, character: 9 },
          end: { line: 1, character: 14 },
        },
        placeholder: "Token",
      });

      const edit = await requestRename(client, tokenUri, { line: 1, character: 10 }, "Asset");
      expect(workspaceChangesForUri(edit, tokenUri)).toEqual([
        {
          range: {
            start: { line: 1, character: 9 },
            end: { line: 1, character: 14 },
          },
          newText: "Asset",
        },
      ]);
      expect(workspaceChangesForUri(edit, mainUri)).toEqual([
        {
          range: {
            start: { line: 1, character: 8 },
            end: { line: 1, character: 13 },
          },
          newText: "Asset",
        },
        {
          range: {
            start: { line: 3, character: 4 },
            end: { line: 3, character: 9 },
          },
          newText: "Asset",
        },
      ]);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("prepares and applies safe cross-file rename edits for namespace imports", async () => {
    const dir = tempWorkspace();
    const tokenPath = path.join(dir, "Token.sol");
    const mainPath = path.join(dir, "Main.sol");
    const tokenUri = toUri(tokenPath);
    const mainUri = toUri(mainPath);
    const tokenSource = `pragma solidity ^0.8.0;
contract Token {}
`;
    const mainSource = `pragma solidity ^0.8.0;
import * as Contracts from "./Token.sol";
contract Main {
    Contracts.Token token;
}
`;

    fs.writeFileSync(tokenPath, tokenSource);
    fs.writeFileSync(mainPath, mainSource);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      const init = await initializeServer(client, toUri(dir));

      expect(init.capabilities.renameProvider).toBeTruthy();

      openDocument(client, tokenUri, tokenSource);
      openDocument(client, mainUri, mainSource);
      await waitForDiagnostics(client, tokenUri).catch(() => {});
      await waitForDiagnostics(client, mainUri).catch(() => {});

      const prepare = await requestPrepareRename(client, tokenUri, {
        line: 1,
        character: 10,
      });
      expect(prepare).toEqual({
        range: {
          start: { line: 1, character: 9 },
          end: { line: 1, character: 14 },
        },
        placeholder: "Token",
      });

      const edit = await requestRename(client, tokenUri, { line: 1, character: 10 }, "Asset");
      expect(workspaceChangesForUri(edit, tokenUri)).toEqual([
        {
          range: {
            start: { line: 1, character: 9 },
            end: { line: 1, character: 14 },
          },
          newText: "Asset",
        },
      ]);
      expect(workspaceChangesForUri(edit, mainUri)).toEqual([
        {
          range: {
            start: { line: 3, character: 14 },
            end: { line: 3, character: 19 },
          },
          newText: "Asset",
        },
      ]);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("returns incoming and outgoing call hierarchy for same-file call sites", async () => {
    const uri = "file:///tmp/solgrid-call-hierarchy.sol";
    const source = `pragma solidity ^0.8.0;

contract Token {
    modifier gated() {
        _;
    }

    function leaf() internal {}

    function branch() internal gated {
        leaf();
        leaf();
    }

    function run() external {
        branch();
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const prepared = await requestPrepareCallHierarchy(client, uri, {
      line: 9,
      character: 13,
    });
    expect(prepared).toHaveLength(1);
    const branch = prepared?.[0] as CallHierarchyItem;
    expect(branch.name).toBe("branch");

    const outgoing = await requestOutgoingCalls(client, branch);
    expect(outgoing).toHaveLength(2);
    expect(outgoing?.map((call) => call.to.name)).toEqual(
      expect.arrayContaining(["gated", "leaf"])
    );
    const leafOutgoing = outgoing?.find((call) => call.to.name === "leaf");
    expect(leafOutgoing?.fromRanges).toHaveLength(2);

    const leafPrepared = await requestPrepareCallHierarchy(client, uri, {
      line: 7,
      character: 13,
    });
    const leaf = leafPrepared?.[0] as CallHierarchyItem;
    const incoming = await requestIncomingCalls(client, leaf);
    expect(incoming).toHaveLength(1);
    expect(incoming?.[0]?.from.name).toBe("branch");
    expect(incoming?.[0]?.fromRanges).toHaveLength(2);
  });

  it("does not prepare call hierarchy for ambiguous overloaded call targets", async () => {
    const uri = "file:///tmp/solgrid-call-hierarchy-overload.sol";
    const source = `pragma solidity ^0.8.0;

contract Token {
    function leaf(uint256 amount) internal {}
    function leaf(address account) internal {}

    function branch(uint256 amount) internal {
        leaf(amount);
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    expect(
      await requestPrepareCallHierarchy(client, uri, {
        line: 6,
        character: 8,
      })
    ).toBeNull();
  });

  it("returns hierarchical document symbols for contracts and members", async () => {
    const uri = "file:///tmp/solgrid-outline.sol";
    const source = `pragma solidity ^0.8.0;

contract Token {
    uint256 public supply;
    event Transfer(address indexed from, address indexed to, uint256 amount);

    function transfer(address to, uint256 amount) public {}
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const symbols = nestedDocumentSymbols(await requestDocumentSymbols(client, uri));
    expect(symbols).toHaveLength(1);
    expect(symbols[0].name).toBe("Token");
    expect(symbols[0].children?.map((child) => child.name)).toEqual(
      expect.arrayContaining(["supply", "Transfer", "transfer"])
    );
  });

  it("returns import-path document links", async () => {
    const dir = tempWorkspace();
    const depPath = path.join(dir, "Dep.sol");
    const mainPath = path.join(dir, "Main.sol");
    const depUri = toUri(depPath);
    const mainUri = toUri(mainPath);

    fs.writeFileSync(depPath, "pragma solidity ^0.8.0;\ncontract Dep {}\n");
    fs.writeFileSync(
      mainPath,
      `pragma solidity ^0.8.0;
import {Dep} from "./Dep.sol";
contract Main {}
`
    );

    try {
      openDocument(client, mainUri, fs.readFileSync(mainPath, "utf-8"));
      await waitForDiagnostics(client, mainUri).catch(() => {});

      const links = await requestDocumentLinks(client, mainUri);
      expect(links).toHaveLength(1);
      expect(canonicalUri(links?.[0].target ?? "")).toBe(canonicalUri(depUri));
    } finally {
      closeDocument(client, mainUri);
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("returns workspace symbols for exported top-level declarations", async () => {
    const dir = tempWorkspace();
    fs.writeFileSync(
      path.join(dir, "Token.sol"),
      "pragma solidity ^0.8.0;\ncontract Token {}\n"
    );
    fs.writeFileSync(
      path.join(dir, "Errors.sol"),
      "pragma solidity ^0.8.0;\nerror Unauthorized();\n"
    );

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      await initializeServer(client, toUri(dir));

      const symbols = await waitForWorkspaceSymbol(client, "Tok", "Token");
      expect(symbols.some((symbol) => symbol.name === "Token")).toBe(true);
      expect(symbols.some((symbol) => symbol.name === "Unauthorized")).toBe(
        false
      );

      const errors = await waitForWorkspaceSymbol(client, "Una", "Unauthorized");
      expect(errors.some((symbol) => symbol.name === "Unauthorized")).toBe(
        true
      );
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("returns reference-count code lenses", async () => {
    const uri = "file:///tmp/solgrid-codelens.sol";
    const source = `pragma solidity ^0.8.0;

contract Token {
    function transfer(address to, uint256 amount) public {}

    function run() public {
        transfer(address(0), 1);
        transfer(address(0), 2);
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const lenses = (await requestCodeLenses(client, uri)) ?? [];
    expect(
      lenses.some((lens: CodeLens) => lens.command?.title === "2 references")
    ).toBe(true);
    expect(
      lenses.some(
        (lens: CodeLens) =>
          lens.command?.title === "Inheritance graph" &&
          lens.command.command === "solgrid.graph.show"
      )
    ).toBe(true);
    expect(
      lenses.some(
        (lens: CodeLens) =>
          lens.command?.title === "Control-flow graph" &&
          lens.command.command === "solgrid.graph.show"
      )
    ).toBe(true);
    expect(
      lenses.some(
        (lens: CodeLens) =>
          lens.command?.title === "Linearized inheritance" &&
          lens.command.command === "solgrid.graph.show"
      )
    ).toBe(false);
  });

  it("builds an imports graph for the active file", async () => {
    const dir = tempWorkspace();
    const depPath = path.join(dir, "Dep.sol");
    const mainPath = path.join(dir, "Main.sol");
    const mainUri = toUri(mainPath);
    const mainSource = `pragma solidity ^0.8.0;
import {Dep} from "./Dep.sol";
contract Main is Dep {}
`;

    fs.writeFileSync(depPath, "pragma solidity ^0.8.0;\ncontract Dep {}\n");
    fs.writeFileSync(mainPath, mainSource);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      await initializeServer(client, toUri(dir));

      openDocument(client, mainUri, mainSource);
      await waitForDiagnostics(client, mainUri).catch(() => {});

      const graph = await requestExecuteCommand<{
        kind: string;
        title: string;
        nodes: Array<{ label: string }>;
        edges: Array<{ label?: string }>;
      }>(client, "solgrid.graph.imports", [{ uri: mainUri }]);

      expect(graph?.kind).toBe("imports");
      expect(graph?.title).toContain("Imports graph");
      expect(graph?.nodes.map((node) => node.label)).toEqual(
        expect.arrayContaining(["Main.sol", "Dep.sol"])
      );
      expect(graph?.edges.some((edge) => edge.label === "imports")).toBe(true);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("builds an inheritance graph for a contract lens target", async () => {
    const dir = tempWorkspace();
    const uri = toUri(path.join(dir, "Vault.sol"));
    const source = `pragma solidity ^0.8.0;

contract Ownable {}
contract Vault is Ownable {}
`;

    fs.writeFileSync(fileURLToPath(uri), source);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      await initializeServer(client, toUri(dir));

      openDocument(client, uri, source);
      await waitForDiagnostics(client, uri).catch(() => {});

      const graph = await requestExecuteCommand<{
        kind: string;
        title: string;
        nodes: Array<{ label: string }>;
        edges: Array<{ label?: string }>;
      }>(client, "solgrid.graph.inheritance", [
        { uri, symbolName: "Vault" },
      ]);

      expect(graph?.kind).toBe("inheritance");
      expect(graph?.title).toContain("Vault");
      expect(graph?.nodes.map((node) => node.label)).toEqual(
        expect.arrayContaining(["Vault", "Ownable"])
      );
      expect(graph?.edges.some((edge) => edge.label === "inherits")).toBe(true);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("builds a linearized inheritance graph for a contract lens target", async () => {
    const dir = tempWorkspace();
    const uri = toUri(path.join(dir, "Vault.sol"));
    const source = `pragma solidity ^0.8.0;

contract Root {}
contract AccessControl is Root {}
contract Pausable is Root {}
contract Vault is AccessControl, Pausable {}
`;

    fs.writeFileSync(fileURLToPath(uri), source);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      await initializeServer(client, toUri(dir));

      openDocument(client, uri, source);
      await waitForDiagnostics(client, uri).catch(() => {});

      const lenses = (await requestCodeLenses(client, uri)) ?? [];
      expect(
        lenses.some(
          (lens: CodeLens) =>
            lens.command?.title === "Linearized inheritance" &&
            lens.command.command === "solgrid.graph.show"
        )
      ).toBe(true);

      const graph = await requestExecuteCommand<{
        kind: string;
        title: string;
        nodes: Array<{ label: string }>;
        edges: Array<{ label?: string }>;
      }>(client, "solgrid.graph.linearizedInheritance", [
        { uri, symbolName: "Vault" },
      ]);

      expect(graph?.kind).toBe("linearized-inheritance");
      expect(graph?.title).toContain("Vault");
      expect(graph?.nodes.map((node) => node.label)).toEqual([
        "Vault",
        "Pausable",
        "AccessControl",
        "Root",
      ]);
      expect(graph?.edges.every((edge) => edge.label === "precedes")).toBe(true);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("builds a control-flow graph for a function lens target", async () => {
    const uri = "file:///tmp/solgrid-cfg.sol";
    const source = `pragma solidity ^0.8.0;

contract Vault {
    function run(uint256 amount) public returns (uint256) {
        if (amount == 0) {
            return 1;
        }

        return amount;
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const graph = await requestExecuteCommand<{
      kind: string;
      title: string;
      nodes: Array<{ label: string; kind?: string }>;
      edges: Array<{ label?: string; kind?: string }>;
    }>(client, "solgrid.graph.controlFlow", [
      {
        uri,
        symbolName: "Vault.run",
        targetOffset: source.indexOf("run"),
      },
    ]);

    expect(graph?.kind).toBe("control-flow");
    expect(graph?.title).toContain("Vault.run");
    expect(graph?.nodes.map((node) => node.label)).toEqual(
      expect.arrayContaining(["Entry", "Exit", "if amount == 0", "return"])
    );
    expect(graph?.nodes.some((node) => node.label === "Entry" && node.kind === "entry")).toBe(
      true
    );
    expect(
      graph?.nodes.some((node) => node.label === "if amount == 0" && node.kind === "branch")
    ).toBe(true);
    expect(
      graph?.nodes.some((node) => node.label === "return" && node.kind === "terminal-return")
    ).toBe(true);
    expect(graph?.edges.some((edge) => edge.label === "true")).toBe(true);
    expect(graph?.edges.some((edge) => edge.label === "false")).toBe(true);
    expect(graph?.edges.some((edge) => edge.label === "return")).toBe(true);
    expect(graph?.edges.some((edge) => edge.kind === "branch-true")).toBe(true);
    expect(graph?.edges.some((edge) => edge.kind === "return")).toBe(true);
  });

  it("expands same-file modifiers in control-flow graphs", async () => {
    const uri = "file:///tmp/solgrid-cfg-modifier.sol";
    const source = `pragma solidity ^0.8.0;

contract Vault {
    modifier onlyPositive(uint256 amount) {
        require(amount > 0);
        _;
    }

    function run(uint256 amount) public onlyPositive(amount) returns (uint256) {
        return amount;
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const graph = await requestExecuteCommand<{
      kind: string;
      title: string;
      nodes: Array<{ label: string; kind?: string }>;
      edges: Array<{ label?: string; kind?: string }>;
    }>(client, "solgrid.graph.controlFlow", [
      {
        uri,
        symbolName: "Vault.run",
        targetOffset: source.indexOf("run"),
      },
    ]);

    expect(graph?.nodes.map((node) => node.label)).toEqual(
      expect.arrayContaining(["modifier onlyPositive(amount)", "call require", "return"])
    );
    expect(
      graph?.nodes.some(
        (node) => node.label === "modifier onlyPositive(amount)" && node.kind === "modifier"
      )
    ).toBe(true);
    expect(
      graph?.nodes.some((node) => node.label === "call require" && node.kind === "call")
    ).toBe(true);
    expect(graph?.nodes.some((node) => node.label === "_")).toBe(false);
  });

  it("expands inherited modifiers from other files in control-flow graphs", async () => {
    const dir = tempWorkspace();
    const basePath = path.join(dir, "Base.sol");
    const mainPath = path.join(dir, "Main.sol");
    const mainUri = toUri(mainPath);
    const baseSource = `pragma solidity ^0.8.0;

contract Base {
    modifier onlyPositive(uint256 amount) {
        require(amount > 0);
        _;
    }
}
`;
    const mainSource = `pragma solidity ^0.8.0;
import "./Base.sol";

contract Main is Base {
    function run(uint256 amount) public onlyPositive(amount) returns (uint256) {
        return amount;
    }
}
`;

    fs.writeFileSync(basePath, baseSource);
    fs.writeFileSync(mainPath, mainSource);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      await initializeServer(client, toUri(dir));

      openDocument(client, mainUri, mainSource);
      await waitForDiagnostics(client, mainUri).catch(() => {});

      const graph = await requestExecuteCommand<{
        kind: string;
        title: string;
        nodes: Array<{ label: string; kind?: string }>;
        edges: Array<{ label?: string; kind?: string }>;
      }>(client, "solgrid.graph.controlFlow", [
        {
          uri: mainUri,
          symbolName: "Main.run",
          targetOffset: mainSource.indexOf("run"),
        },
      ]);

      expect(graph?.nodes.map((node) => node.label)).toEqual(
        expect.arrayContaining([
          "modifier onlyPositive(amount)",
          "call require",
          "return",
        ])
      );
      expect(
        graph?.nodes.some(
          (node) => node.label === "modifier onlyPositive(amount)" && node.kind === "modifier"
        )
      ).toBe(true);
      expect(
        graph?.nodes.some((node) => node.label === "call require" && node.kind === "call")
      ).toBe(true);
      expect(graph?.nodes.some((node) => node.label === "_")).toBe(false);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("expands inline assembly into Yul control-flow nodes", async () => {
    const uri = "file:///tmp/solgrid-cfg-assembly.sol";
    const source = `pragma solidity ^0.8.0;

contract Vault {
    function run(uint256 amount) public returns (uint256) {
        assembly {
            let result := add(amount, 1)
            if gt(result, 10) {
                mstore(0x00, result)
                leave
            }
        }

        return amount;
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const graph = await requestExecuteCommand<{
      kind: string;
      title: string;
      nodes: Array<{ label: string; kind?: string }>;
      edges: Array<{ label?: string; kind?: string }>;
    }>(client, "solgrid.graph.controlFlow", [
      {
        uri,
        symbolName: "Vault.run",
        targetOffset: source.indexOf("run"),
      },
    ]);

    expect(graph?.kind).toBe("control-flow");
    expect(
      graph?.nodes.some((node) => node.label === "assembly" && node.kind === "assembly")
    ).toBe(true);
    expect(
      graph?.nodes.some(
        (node) =>
          node.label === "let result := add(amount, 1)" && node.kind === "declaration"
      )
    ).toBe(true);
    expect(
      graph?.nodes.some(
        (node) => node.label === "if gt(result, 10)" && node.kind === "branch"
      )
    ).toBe(true);
    expect(
      graph?.nodes.some((node) => node.label === "call mstore" && node.kind === "call")
    ).toBe(true);
    expect(
      graph?.nodes.some((node) => node.label === "leave" && node.kind === "terminal-return")
    ).toBe(true);
    expect(
      graph?.edges.some((edge) => edge.label === "leave" && edge.kind === "return")
    ).toBe(true);
  });

  it("expands Yul function definitions and terminal builtins in control-flow graphs", async () => {
    const uri = "file:///tmp/solgrid-cfg-yul-functions.sol";
    const source = `pragma solidity ^0.8.0;

contract Vault {
    function run(uint256 amount) public returns (uint256) {
        assembly {
            function helper(value) -> result {
                if gt(value, 10) {
                    revert(0, 0)
                }
                result := add(value, 1)
            }

            let computed := helper(amount)
            return(0, 0)
        }
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const graph = await requestExecuteCommand<{
      kind: string;
      title: string;
      nodes: Array<{ label: string; kind?: string }>;
      edges: Array<{ label?: string; kind?: string }>;
    }>(client, "solgrid.graph.controlFlow", [
      {
        uri,
        symbolName: "Vault.run",
        targetOffset: source.indexOf("run"),
      },
    ]);

    expect(graph?.nodes.some((node) => node.label === "function helper")).toBe(true);
    expect(graph?.nodes.some((node) => node.label === "end helper")).toBe(true);
    expect(
      graph?.nodes.some((node) => node.label === "revert" && node.kind === "terminal-revert")
    ).toBe(true);
    expect(
      graph?.nodes.some((node) => node.label === "return" && node.kind === "terminal-return")
    ).toBe(true);
    expect(graph?.edges.some((edge) => edge.label === "calls")).toBe(true);
    expect(graph?.edges.some((edge) => edge.label === "revert" && edge.kind === "revert")).toBe(
      true
    );
    expect(graph?.edges.some((edge) => edge.label === "return" && edge.kind === "return")).toBe(
      true
    );
  });

  it("returns parameter-name inlay hints for positional call arguments", async () => {
    const uri = "file:///tmp/solgrid-inlay.sol";
    const source = `pragma solidity ^0.8.0;

contract Token {
    function transfer(address recipient, uint256 amount) public {}

    function run() public {
        transfer(address(0), 1);
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const hints =
      (await requestInlayHints(client, uri, {
        start: { line: 6, character: 0 },
        end: { line: 6, character: 32 },
      })) ?? [];
    const labels = hints.map((hint: InlayHint) =>
      typeof hint.label === "string"
        ? hint.label
        : hint.label.map((part) => part.value).join("")
    );

    expect(labels).toEqual(expect.arrayContaining(["recipient:", "amount:"]));
  });

  it("returns selector-oriented inlay hints for ABI declarations", async () => {
    const uri = "file:///tmp/solgrid-selector-hints.sol";
    const source = `pragma solidity ^0.8.0;

interface IRouter {
    function swap(address tokenIn, uint256 amountIn) external returns (uint256);
}

contract Router {
    function swap(address tokenIn, uint256 amountIn) public returns (uint256) {
        return amountIn;
    }

    function helper(uint256 amount) internal {}
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const hints =
      (await requestInlayHints(client, uri, {
        start: { line: 0, character: 0 },
        end: { line: 13, character: 0 },
      })) ?? [];
    const labels = hints.map((hint: InlayHint) =>
      typeof hint.label === "string"
        ? hint.label
        : hint.label.map((part) => part.value).join("")
    );

    expect(labels.filter((label) => label.startsWith("selector: "))).toHaveLength(2);
    expect(labels.some((label) => label.startsWith("interface ID: "))).toBe(true);
  });

  it("returns inheritance-origin inlay hints for overriding declarations", async () => {
    const uri = "file:///tmp/solgrid-inheritance-hints.sol";
    const source = `pragma solidity ^0.8.0;

interface IRouter {
    function swap(address tokenIn, uint256 amountIn) external returns (uint256);
}

abstract contract BaseRouter {
    function swap(address tokenIn, uint256 amountIn) public virtual returns (uint256);
}

contract Router is BaseRouter, IRouter {
    function swap(address tokenIn, uint256 amountIn) public override(BaseRouter, IRouter) returns (uint256) {
        return amountIn;
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const hints =
      (await requestInlayHints(client, uri, {
        start: { line: 0, character: 0 },
        end: { line: 16, character: 0 },
      })) ?? [];
    const labels = hints.map((hint: InlayHint) =>
      typeof hint.label === "string"
        ? hint.label
        : hint.label.map((part) => part.value).join("")
    );

    expect(labels).toContain("linearized: IRouter -> BaseRouter");
    expect(labels).toContain("overrides BaseRouter; implements IRouter");
  });

  it("returns detector-aware declaration inlay hints for metadata-backed findings", async () => {
    const uri = "file:///tmp/solgrid-detector-hints.sol";
    const source = `pragma solidity ^0.8.0;

contract Forwarder {
    function route(address target, bytes memory data) internal {
        target.call(data);
    }
}
`;

    openDocument(client, uri, source);
    await waitForDiagnostics(client, uri).catch(() => {});

    const hints =
      (await requestInlayHints(client, uri, {
        start: { line: 0, character: 0 },
        end: { line: 6, character: 0 },
      })) ?? [];
    const labels = hints.map(inlayHintLabel);

    expect(labels).toContain("security warning/high: Unchecked low-level call");
  });

  it("returns inherited-member hints for accessible base declarations", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "Vault.sol");
    const uri = toUri(filePath);
    const source = `pragma solidity ^0.8.0;

contract BaseVault {
    uint256 internal totalSupply;
    uint256 private secretSupply;
    event Transfer(address indexed from, address indexed to, uint256 amount);

    modifier onlyOwner() {
        _;
    }

    function pause() internal {}
}

contract Vault is BaseVault {}
`;

    fs.writeFileSync(filePath, source);

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      await initializeServer(client, toUri(dir));

      openDocument(client, uri, source);
      await waitForDiagnostics(client, uri).catch(() => {});

      const hints =
        (await requestInlayHints(client, uri, {
          start: { line: 0, character: 0 },
          end: { line: 16, character: 0 },
        })) ?? [];
      const labels = hints.map(inlayHintLabel);
      const inherited = labels.find((label) => label.startsWith("inherits members:"));

      expect(inherited).toBeDefined();
      expect(inherited).toContain("totalSupply");
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("refreshes workspace symbols after closed-file changes", async () => {
    const dir = tempWorkspace();
    const depPath = path.join(dir, "Dep.sol");
    const depUri = toUri(depPath);

    fs.writeFileSync(depPath, "pragma solidity ^0.8.0;\ncontract OldDep {}\n");

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      await initializeServer(client, toUri(dir));

      await waitForWorkspaceSymbol(client, "Old", "OldDep");

      fs.writeFileSync(depPath, "pragma solidity ^0.8.0;\ncontract NewDep {}\n");
      notifyWatchedFilesChanged(client, [{ uri: depUri, type: 2 }]);

      const symbols = await waitForWorkspaceSymbol(client, "New", "NewDep");
      expect(symbols.some((symbol) => symbol.name === "NewDep")).toBe(true);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});
