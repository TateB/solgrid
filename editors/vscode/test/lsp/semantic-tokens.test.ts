import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { pathToFileURL } from "node:url";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { TestLspClient } from "./client";
import {
  changeDocument,
  decodeSemanticTokens,
  initializeServer,
  openDocument,
  requestSemanticTokens,
  requestSemanticTokensFullDelta,
  requestSemanticTokensRange,
  resetDocumentVersions,
  waitForDiagnostics,
} from "./helpers";

function tempWorkspace(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "solgrid-semantic-tokens-"));
}

function toUri(filePath: string): string {
  const resolvedPath = fs.existsSync(filePath)
    ? fs.realpathSync(filePath)
    : path.resolve(filePath);
  return pathToFileURL(resolvedPath).toString();
}

function tokenText(source: string, line: number, startChar: number, length: number): string {
  const lineText = source.split("\n")[line] ?? "";
  return Array.from(lineText)
    .slice(startChar, startChar + length)
    .join("");
}

describe("LSP Semantic Tokens", () => {
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

  it("returns semantic tokens for Solidity declarations and references", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "Token.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract Token {
    event Transfer(address indexed from, address indexed to, uint256 value);

    address private owner;

    modifier onlyOwner() {
        _;
    }

    function run(address recipient) external onlyOwner {
        emit Transfer(owner, recipient, 1);
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      const init = await initializeServer(client, toUri(dir));
      const legend = init.capabilities.semanticTokensProvider?.legend;
      expect(legend).toBeDefined();

      openDocument(client, uri, content);
      await waitForDiagnostics(client, uri).catch(() => {});

      const result = await requestSemanticTokens(client, uri);
      expect(result).toBeDefined();
      const tokens = decodeSemanticTokens(result!, legend!);
      const entries = tokens.map((token) => ({
        ...token,
        text: tokenText(content, token.line, token.startChar, token.length),
      }));

      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "Token",
          tokenType: "class",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "Transfer",
          tokenType: "event",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "owner",
          tokenType: "property",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "owner",
          tokenType: "property",
          tokenModifiers: [],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "onlyOwner",
          tokenType: "modifier",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "onlyOwner",
          tokenType: "modifier",
          tokenModifiers: [],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "recipient",
          tokenType: "parameter",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "recipient",
          tokenType: "parameter",
          tokenModifiers: [],
        })
      );
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("returns only semantic tokens that overlap the requested range", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "Token.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract Token {
    event Transfer(address indexed from, address indexed to, uint256 value);

    address private owner;

    modifier onlyOwner() {
        _;
    }

    function run(address recipient) external onlyOwner {
        emit Transfer(owner, recipient, 1);
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      const init = await initializeServer(client, toUri(dir));
      const legend = init.capabilities.semanticTokensProvider?.legend;
      expect(legend).toBeDefined();
      expect(init.capabilities.semanticTokensProvider?.range).toBeTruthy();

      openDocument(client, uri, content);
      await waitForDiagnostics(client, uri).catch(() => {});

      const lineText = content.split("\n")[12] ?? "";
      const result = await requestSemanticTokensRange(client, uri, {
        start: { line: 11, character: 0 },
        end: { line: 12, character: lineText.length },
      });
      expect(result).toBeDefined();
      const tokens = decodeSemanticTokens(result!, legend!);
      const entries = tokens.map((token) => ({
        ...token,
        text: tokenText(content, token.line, token.startChar, token.length),
      }));

      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "onlyOwner",
          tokenType: "modifier",
          tokenModifiers: [],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "Transfer",
          tokenType: "event",
          tokenModifiers: [],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "owner",
          tokenType: "property",
          tokenModifiers: [],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "recipient",
          tokenType: "parameter",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).not.toContainEqual(
        expect.objectContaining({
          text: "Token",
          tokenType: "class",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).not.toContainEqual(
        expect.objectContaining({
          text: "Transfer",
          tokenType: "event",
          tokenModifiers: ["declaration"],
        })
      );
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("supports semantic token delta requests for unchanged and changed documents", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "Delta.sol");
    const uri = toUri(filePath);
    const original = `pragma solidity ^0.8.0;

contract Delta {
    uint256 private counter;

    function run(uint256 amount) external {
        counter = amount;
    }
}
`;
    const updated = `pragma solidity ^0.8.0;

contract Delta {
    uint256 private counter;

    function run(uint256 amount) external {
        uint256 next = amount + 1;
        counter = next;
    }
}
`;

    fs.writeFileSync(filePath, original, "utf8");

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      const init = await initializeServer(client, toUri(dir));
      expect(init.capabilities.semanticTokensProvider?.full).toBeTruthy();

      openDocument(client, uri, original);
      await waitForDiagnostics(client, uri).catch(() => {});

      const full = await requestSemanticTokens(client, uri);
      expect(full?.resultId).toBeDefined();

      const unchanged = await requestSemanticTokensFullDelta(
        client,
        uri,
        full!.resultId!
      );
      expect(unchanged).toBeDefined();
      expect("edits" in unchanged!).toBe(true);
      if (unchanged && "edits" in unchanged) {
        expect(unchanged.resultId).toBe(full!.resultId);
        expect(unchanged.edits).toEqual([]);
      }

      changeDocument(client, uri, updated);
      await waitForDiagnostics(client, uri).catch(() => {});

      const changed = await requestSemanticTokensFullDelta(
        client,
        uri,
        full!.resultId!
      );
      expect(changed).toBeDefined();
      expect("data" in changed!).toBe(true);
      if (changed && "data" in changed) {
        expect(changed.resultId).not.toBe(full!.resultId);
        const legend = init.capabilities.semanticTokensProvider?.legend;
        expect(legend).toBeDefined();
        const entries = decodeSemanticTokens(changed, legend!).map((token) => ({
          ...token,
          text: tokenText(updated, token.line, token.startChar, token.length),
        }));
        expect(entries).toContainEqual(
          expect.objectContaining({
            text: "next",
            tokenType: "variable",
            tokenModifiers: ["declaration"],
          })
        );
      }
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("returns namespace and imported type tokens for namespace-qualified references", async () => {
    const dir = tempWorkspace();
    const depPath = path.join(dir, "Lib.sol");
    const mainPath = path.join(dir, "Main.sol");
    const depUri = toUri(depPath);
    const mainUri = toUri(mainPath);
    const depSource = `pragma solidity ^0.8.0;
contract Token {}
`;
    const mainSource = `pragma solidity ^0.8.0;
import * as Contracts from "./Lib.sol";

contract Main {
    Contracts.Token token;
}
`;

    fs.writeFileSync(depPath, depSource, "utf8");
    fs.writeFileSync(mainPath, mainSource, "utf8");

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      const init = await initializeServer(client, toUri(dir));
      const legend = init.capabilities.semanticTokensProvider?.legend;
      expect(legend).toBeDefined();

      openDocument(client, depUri, depSource);
      await waitForDiagnostics(client, depUri).catch(() => {});
      openDocument(client, mainUri, mainSource);
      await waitForDiagnostics(client, mainUri).catch(() => {});

      const result = await requestSemanticTokens(client, mainUri);
      expect(result).toBeDefined();
      const tokens = decodeSemanticTokens(result!, legend!);
      const entries = tokens.map((token) => ({
        ...token,
        text: tokenText(mainSource, token.line, token.startChar, token.length),
      }));

      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "Contracts",
          tokenType: "namespace",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "Contracts",
          tokenType: "namespace",
          tokenModifiers: [],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "Token",
          tokenType: "class",
          tokenModifiers: [],
        })
      );
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("preserves readonly modifiers and imported alias kinds", async () => {
    const dir = tempWorkspace();
    const depPath = path.join(dir, "Lib.sol");
    const mainPath = path.join(dir, "Main.sol");
    const depUri = toUri(depPath);
    const mainUri = toUri(mainPath);
    const depSource = `pragma solidity ^0.8.0;

contract Vault {}
error Unauthorized(address caller);
`;
    const mainSource = `pragma solidity ^0.8.0;
import { Vault as ImportedVault, Unauthorized as ImportedUnauthorized } from "./Lib.sol";

library Limits {
    uint256 internal constant MAX = 2;
}

contract Main {
    uint256 private constant LOCAL_MAX = 1;
    address private immutable owner;

    enum Mode {
        Idle,
        Running
    }

    ImportedVault vault;

    constructor() {
        owner = msg.sender;
    }

    function run() external view returns (uint256) {
        Mode mode = Mode.Running;
        if (msg.sender != owner) {
            revert ImportedUnauthorized(msg.sender);
        }
        if (mode == Mode.Running) {
            return Limits.MAX;
        }
        if (mode == Mode.Idle) {
            return LOCAL_MAX;
        }
        return 0;
    }
}
`;

    fs.writeFileSync(depPath, depSource, "utf8");
    fs.writeFileSync(mainPath, mainSource, "utf8");

    try {
      client.kill();
      client = new TestLspClient();
      client.start();
      resetDocumentVersions();
      const init = await initializeServer(client, toUri(dir));
      const legend = init.capabilities.semanticTokensProvider?.legend;
      expect(legend).toBeDefined();

      openDocument(client, depUri, depSource);
      await waitForDiagnostics(client, depUri).catch(() => {});
      openDocument(client, mainUri, mainSource);
      await waitForDiagnostics(client, mainUri).catch(() => {});

      const result = await requestSemanticTokens(client, mainUri);
      expect(result).toBeDefined();
      const tokens = decodeSemanticTokens(result!, legend!);
      const entries = tokens.map((token) => ({
        ...token,
        text: tokenText(mainSource, token.line, token.startChar, token.length),
      }));

      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "ImportedVault",
          tokenType: "class",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "ImportedUnauthorized",
          tokenType: "type",
          tokenModifiers: ["declaration"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "ImportedUnauthorized",
          tokenType: "type",
          tokenModifiers: [],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "LOCAL_MAX",
          tokenType: "property",
          tokenModifiers: ["declaration", "readonly"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "LOCAL_MAX",
          tokenType: "property",
          tokenModifiers: ["readonly"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "owner",
          tokenType: "property",
          tokenModifiers: ["declaration", "readonly"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "owner",
          tokenType: "property",
          tokenModifiers: ["readonly"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "Running",
          tokenType: "enumMember",
          tokenModifiers: ["declaration", "readonly"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "Running",
          tokenType: "enumMember",
          tokenModifiers: ["readonly"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "MAX",
          tokenType: "property",
          tokenModifiers: ["declaration", "readonly"],
        })
      );
      expect(entries).toContainEqual(
        expect.objectContaining({
          text: "MAX",
          tokenType: "property",
          tokenModifiers: ["readonly"],
        })
      );
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});
