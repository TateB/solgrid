/**
 * LSP Diagnostics Tests
 *
 * Tests textDocument/didOpen, didChange, and didClose interactions
 * and verifies publishDiagnostics notifications are correct.
 */

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import {
  initializeServer,
  notifyWatchedFilesChanged,
  openDocument,
  changeDocument,
  closeDocument,
  requestExecuteCommand,
  waitForDiagnostics,
  readFixture,
  fixtureUri,
  resetDocumentVersions,
  PublishDiagnosticsParams,
} from "./helpers";

function tempWorkspace(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "solgrid-diag-"));
}

function toUri(filePath: string): string {
  const resolvedPath = fs.existsSync(filePath)
    ? fs.realpathSync(filePath)
    : path.resolve(filePath);
  return pathToFileURL(resolvedPath).toString();
}

function normalizeUriPath(uri: string): string {
  const filePath = fileURLToPath(uri);
  return fs.existsSync(filePath) ? fs.realpathSync(filePath) : path.resolve(filePath);
}

describe("LSP Diagnostics", () => {
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

  it("publishes diagnostics when opening a file with issues", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    expect(result.diagnostics).toBeDefined();
    expect(result.diagnostics.length).toBeGreaterThan(0);
  });

  it("detects security/tx-origin in file with tx.origin usage", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    const txOriginDiags = result.diagnostics.filter(
      (d) => d.code === "security/tx-origin"
    );
    expect(txOriginDiags.length).toBeGreaterThan(0);
  });

  it("diagnostics have correct source field", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    for (const diag of result.diagnostics) {
      expect(diag.source).toBe("solgrid");
    }
  });

  it("diagnostics have valid severity", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    for (const diag of result.diagnostics) {
      expect(diag.severity).toBeDefined();
      // LSP severity: 1=Error, 2=Warning, 3=Info, 4=Hint
      expect(diag.severity).toBeGreaterThanOrEqual(1);
      expect(diag.severity).toBeLessThanOrEqual(4);
    }
  });

  it("diagnostics have valid ranges", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    for (const diag of result.diagnostics) {
      expect(diag.range.start.line).toBeLessThanOrEqual(diag.range.end.line);
      expect(diag.range.start.line).toBeGreaterThanOrEqual(0);
    }
  });

  it("diagnostics have rule ID as code", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    for (const diag of result.diagnostics) {
      expect(diag.code).toBeDefined();
      expect(typeof diag.code).toBe("string");
      // Rule IDs follow category/name pattern
      expect(diag.code as string).toMatch(/^[a-z-]+\/[a-z-]+$/);
    }
  });

  it("produces fewer diagnostics for a clean file", async () => {
    const cleanUri = fixtureUri("clean.sol");
    const cleanContent = readFixture("clean.sol");
    const issuesUri = fixtureUri("with_issues.sol");
    const issuesContent = readFixture("with_issues.sol");

    openDocument(client, issuesUri, issuesContent);
    const issuesResult = await waitForDiagnostics(client, issuesUri);

    openDocument(client, cleanUri, cleanContent);
    const cleanResult = await waitForDiagnostics(client, cleanUri);

    expect(cleanResult.diagnostics.length).toBeLessThan(
      issuesResult.diagnostics.length
    );
  });

  it("updates diagnostics on document change", async () => {
    const uri = "file:///tmp/test-change.sol";
    const initialContent = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public {
        require(tx.origin == msg.sender);
    }
}
`;

    openDocument(client, uri, initialContent);
    const initial = await waitForDiagnostics(client, uri);
    const hasTxOrigin = initial.diagnostics.some(
      (d) => d.code === "security/tx-origin"
    );
    expect(hasTxOrigin).toBe(true);

    // Fix the issue by removing tx.origin
    const fixedContent = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function good() public pure returns (uint256) {
        return 42;
    }
}
`;

    changeDocument(client, uri, fixedContent);
    const updated = await waitForDiagnostics(client, uri);
    const stillHasTxOrigin = updated.diagnostics.some(
      (d) => d.code === "security/tx-origin"
    );
    expect(stillHasTxOrigin).toBe(false);
  });

  it("re-publishes diagnostics from disk on document close", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const closedDiagsPromise = waitForDiagnostics(client, uri);
    closeDocument(client, uri);
    const result = await closedDiagsPromise;

    expect(result.diagnostics.length).toBeGreaterThan(0);
    expect(result.diagnostics.some((diagnostic) => diagnostic.code === "security/tx-origin")).toBe(
      true
    );
  });

  it("handles multiple open files independently", async () => {
    const uri1 = fixtureUri("with_issues.sol");
    const content1 = readFixture("with_issues.sol");
    const uri2 = fixtureUri("clean.sol");
    const content2 = readFixture("clean.sol");

    openDocument(client, uri1, content1);
    const result1 = await waitForDiagnostics(client, uri1);

    openDocument(client, uri2, content2);
    const result2 = await waitForDiagnostics(client, uri2);

    // File with issues should have more diagnostics
    expect(result1.diagnostics.length).toBeGreaterThan(0);
    // Both should have independent diagnostic sets
    expect(result1.uri).toBe(uri1);
    expect(result2.uri).toBe(uri2);
  });

  it("handles syntax errors without crashing", async () => {
    const uri = fixtureUri("syntax_error.sol");
    const content = readFixture("syntax_error.sol");

    openDocument(client, uri, content);

    // Should either publish diagnostics or an empty set, but not crash
    // Give it a shorter timeout since syntax errors may not produce diagnostics
    try {
      const result = await waitForDiagnostics(client, uri, 5000);
      expect(result).toBeDefined();
    } catch {
      // Timeout is acceptable — the server may not publish diagnostics
      // for files that fail to parse. The important thing is no crash.
    }

    // Server should still be responsive after syntax error
    expect(client.isRunning).toBe(true);
  });

  it("handles large files", async () => {
    const uri = fixtureUri("large.sol");
    const content = readFixture("large.sol");

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    expect(result).toBeDefined();
    expect(client.isRunning).toBe(true);
  });

  it("detects multiple rule categories in a single file", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    const categories = new Set(
      result.diagnostics.map((d) => (d.code as string).split("/")[0])
    );
    // with_issues.sol should trigger at least security rules
    expect(categories.has("security")).toBe(true);
  });

  it("attaches normalized finding metadata to lint diagnostics", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    const txOrigin = result.diagnostics.find(
      (diagnostic) => diagnostic.code === "security/tx-origin"
    );
    expect(txOrigin).toBeDefined();
    expect(txOrigin?.data).toMatchObject({
      id: "security/tx-origin",
      category: "security",
      kind: "detector",
      confidence: "high",
      suppressible: true,
    });
  });

  it("publishes compiler diagnostics for unresolved custom types and bases", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "Broken.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract Broken is MissingBase {
    MissingType private value;
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    expect(result.diagnostics.some((d) => d.code === "compiler/unresolved-base-contract")).toBe(
      true
    );
    expect(result.diagnostics.some((d) => d.code === "compiler/unresolved-type")).toBe(true);
  });

  it("publishes compiler diagnostics for unresolved events and custom errors", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "BrokenTargets.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract BrokenTargets {
    function fail() external {
        emit MissingEvent(1);
        revert MissingError(2);
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    expect(result.diagnostics.some((d) => d.code === "compiler/unresolved-event")).toBe(
      true
    );
    expect(result.diagnostics.some((d) => d.code === "compiler/unresolved-error")).toBe(
      true
    );
  });

  it("does not flag builtin revert errors as unresolved compiler diagnostics", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "BuiltinError.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract BuiltinError {
    function fail() external pure {
        revert Error("failed");
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    expect(result.diagnostics.some((d) => d.code === "compiler/unresolved-error")).toBe(
      false
    );
  });

  it("does not flag imported type aliases as unresolved compiler diagnostics", async () => {
    const dir = tempWorkspace();
    const depPath = path.join(dir, "Types.sol");
    const mainPath = path.join(dir, "Main.sol");
    const depUri = toUri(depPath);
    const mainUri = toUri(mainPath);
    const depSource = `pragma solidity ^0.8.0;

struct Point {
    uint256 x;
}
`;
    const mainSource = `pragma solidity ^0.8.0;
import {Point as Coord} from "./Types.sol";

contract Main {
    Coord private point;
}
`;

    fs.writeFileSync(depPath, depSource, "utf8");
    fs.writeFileSync(mainPath, mainSource, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, depUri, depSource);
    await waitForDiagnostics(client, depUri);

    openDocument(client, mainUri, mainSource);
    const result = await waitForDiagnostics(client, mainUri);

    expect(result.diagnostics.some((d) => d.code === "compiler/unresolved-type")).toBe(false);
  });

  it("publishes semantic detector diagnostics for unchecked low-level calls", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "UncheckedCall.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract UncheckedCall {
    function run(address target, bytes memory payload) external {
        target.call(payload);
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    const unchecked = result.diagnostics.find(
      (diagnostic) => diagnostic.code === "security/unchecked-low-level-call"
    );
    expect(unchecked).toBeDefined();
    expect(
      result.diagnostics.some(
        (diagnostic) => diagnostic.code === "security/low-level-calls"
      )
    ).toBe(false);
    expect(unchecked?.severity).toBe(2);
    expect(unchecked?.data).toMatchObject({
      id: "security/unchecked-low-level-call",
      category: "security",
      kind: "detector",
      confidence: "high",
      help_url:
        "https://github.com/TateB/solgrid/blob/main/docs/semantic-detectors.md#security-unchecked-low-level-call",
      suppressible: true,
      has_fix: false,
    });
  });

  it("publishes semantic detector diagnostics for delegatecall targets sourced from parameters", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "Delegatecall.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract Delegatecall {
    function run(address implementation, bytes memory payload) external {
        implementation.delegatecall(payload);
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    const delegatecall = result.diagnostics.find(
      (diagnostic) => diagnostic.code === "security/user-controlled-delegatecall"
    );
    expect(delegatecall).toBeDefined();
    expect(
      result.diagnostics.some(
        (diagnostic) => diagnostic.code === "security/low-level-calls"
      )
    ).toBe(false);
    expect(
      result.diagnostics.some(
        (diagnostic) => diagnostic.code === "security/unchecked-low-level-call"
      )
    ).toBe(true);
    expect(delegatecall?.severity).toBe(1);
    expect(delegatecall?.data).toMatchObject({
      id: "security/user-controlled-delegatecall",
      category: "security",
      kind: "detector",
      confidence: "high",
      help_url:
        "https://github.com/TateB/solgrid/blob/main/docs/semantic-detectors.md#security-user-controlled-delegatecall",
      suppressible: true,
      has_fix: false,
    });
  });

  it("publishes propagated delegatecall diagnostics when a parameter flows through a helper", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "DelegatecallWrapper.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract DelegatecallWrapper {
    function run(address implementation, bytes memory payload) external {
        _delegate(implementation, payload);
    }

    function _delegate(address target, bytes memory payload) internal {
        target.delegatecall(payload);
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    const propagated = result.diagnostics.find(
      (diagnostic) =>
        diagnostic.code === "security/user-controlled-delegatecall" &&
        diagnostic.message.includes("flows into delegatecall via `_delegate`")
    );
    expect(propagated).toBeDefined();
    expect(propagated?.severity).toBe(1);
    expect(propagated?.data).toMatchObject({
      id: "security/user-controlled-delegatecall",
      confidence: "medium",
      kind: "detector",
    });
  });

  it("publishes semantic detector diagnostics for parameter-driven ETH transfers", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "EthTransfer.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract EthTransfer {
    function pay(address recipient, uint256 amount) external payable {
        payable(recipient).call{value: amount}("");
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    const transfer = result.diagnostics.find(
      (diagnostic) => diagnostic.code === "security/user-controlled-eth-transfer"
    );
    expect(transfer).toBeDefined();
    expect(
      result.diagnostics.some(
        (diagnostic) => diagnostic.code === "security/arbitrary-send-eth"
      )
    ).toBe(false);
    expect(
      result.diagnostics.some(
        (diagnostic) => diagnostic.code === "security/low-level-calls"
      )
    ).toBe(false);
    expect(
      result.diagnostics.some(
        (diagnostic) => diagnostic.code === "security/unchecked-low-level-call"
      )
    ).toBe(true);
    expect(transfer?.severity).toBe(2);
    expect(transfer?.data).toMatchObject({
      id: "security/user-controlled-eth-transfer",
      category: "security",
      kind: "detector",
      confidence: "high",
      help_url:
        "https://github.com/TateB/solgrid/blob/main/docs/semantic-detectors.md#security-user-controlled-eth-transfer",
      suppressible: true,
      has_fix: false,
    });
  });

  it("publishes propagated ETH transfer diagnostics when a parameter flows through a helper", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "EthTransferWrapper.sol");
    const uri = toUri(filePath);
    const content = `pragma solidity ^0.8.0;

contract EthTransferWrapper {
    function pay(address recipient, uint256 amount) external payable {
        _pay(recipient, amount);
    }

    function _pay(address target, uint256 amount) internal {
        payable(target).call{value: amount}("");
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, uri, content);
    const result = await waitForDiagnostics(client, uri);

    const propagated = result.diagnostics.find(
      (diagnostic) =>
        diagnostic.code === "security/user-controlled-eth-transfer" &&
        diagnostic.message.includes("flows into an ETH transfer via `_pay`")
    );
    expect(propagated).toBeDefined();
    expect(propagated?.severity).toBe(2);
    expect(propagated?.data).toMatchObject({
      id: "security/user-controlled-eth-transfer",
      confidence: "medium",
      kind: "detector",
    });
  });

  it("reruns workspace analysis for closed files and clears stale diagnostics", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "Closed.sol");
    const uri = toUri(filePath);
    const initialContent = `pragma solidity ^0.8.0;

contract Closed {
    function bad() external view returns (address) {
        return tx.origin;
    }
}
`;

    fs.writeFileSync(filePath, initialContent, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    const normalizedUri = normalizeUriPath(uri);
    const initialDiagnostics = client.waitForNotification(
      "textDocument/publishDiagnostics",
      (params) =>
        normalizeUriPath((params as PublishDiagnosticsParams).uri) === normalizedUri
    ) as Promise<PublishDiagnosticsParams>;
    const initialResult = await requestExecuteCommand<{
      filesAnalyzed: number;
      diagnosticsPublished: number;
      staleDiagnosticsCleared: number;
      openDocuments: number;
    }>(client, "solgrid.workspace.rerunSecurityAnalysis");
    const published = await initialDiagnostics;

    expect(initialResult).toMatchObject({
      filesAnalyzed: 1,
      openDocuments: 0,
    });
    expect(published).toBeDefined();
    expect(
      published?.diagnostics.some((diagnostic) => diagnostic.code === "security/tx-origin")
    ).toBe(true);

    fs.writeFileSync(
      filePath,
      `pragma solidity ^0.8.0;

contract Closed {
    function good() external pure returns (uint256) {
        return 1;
    }
}
`,
      "utf8"
    );

    const clearedDiagnostics = client.waitForNotification(
      "textDocument/publishDiagnostics",
      (params) =>
        normalizeUriPath((params as PublishDiagnosticsParams).uri) === normalizedUri
    ) as Promise<PublishDiagnosticsParams>;
    const rerunResult = await requestExecuteCommand<{
      filesAnalyzed: number;
      diagnosticsPublished: number;
      staleDiagnosticsCleared: number;
      openDocuments: number;
    }>(client, "solgrid.workspace.rerunSecurityAnalysis");
    const cleared = await clearedDiagnostics;

    expect(rerunResult).toMatchObject({
      filesAnalyzed: 1,
      openDocuments: 0,
    });
    expect(
      cleared.diagnostics.some((diagnostic) => diagnostic.code === "security/tx-origin")
    ).toBe(false);
  });

  it("automatically reruns analysis when solgrid.toml changes", async () => {
    const dir = tempWorkspace();
    const filePath = path.join(dir, "ConfigRefresh.sol");
    const configPath = path.join(dir, "solgrid.toml");
    const uri = toUri(filePath);
    const configUri = toUri(configPath);
    const content = `pragma solidity ^0.8.0;

contract ConfigRefresh {
    function bad() external view returns (address) {
        return tx.origin;
    }
}
`;

    fs.writeFileSync(filePath, content, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, uri, content);
    const initial = await waitForDiagnostics(client, uri);
    expect(initial.diagnostics.some((diagnostic) => diagnostic.code === "security/tx-origin")).toBe(
      true
    );

    fs.writeFileSync(configPath, '[lint.rules]\n"security/tx-origin" = "off"\n', "utf8");

    const refreshedDiagnostics = waitForDiagnostics(client, uri);
    notifyWatchedFilesChanged(client, [{ uri: configUri, type: 2 }]);
    const refreshed = await refreshedDiagnostics;

    expect(
      refreshed.diagnostics.some((diagnostic) => diagnostic.code === "security/tx-origin")
    ).toBe(false);
  });

  it("automatically reruns analysis when remappings.txt changes", async () => {
    const dir = tempWorkspace();
    const depPath = path.join(dir, "src", "Dep.sol");
    const mainPath = path.join(dir, "Main.sol");
    const remappingsPath = path.join(dir, "remappings.txt");
    const mainUri = toUri(mainPath);
    const remappingsUri = toUri(remappingsPath);
    const depSource = `pragma solidity ^0.8.0;

contract Dep {}
`;
    const mainSource = `pragma solidity ^0.8.0;
import "@src/Dep.sol";

contract Main is Dep {}
`;

    fs.mkdirSync(path.dirname(depPath), { recursive: true });
    fs.writeFileSync(depPath, depSource, "utf8");
    fs.writeFileSync(mainPath, mainSource, "utf8");

    await client.shutdown().catch(() => {
      client.kill();
    });
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client, toUri(dir));

    openDocument(client, mainUri, mainSource);
    const initial = await waitForDiagnostics(client, mainUri);
    expect(
      initial.diagnostics.some((diagnostic) => diagnostic.code === "compiler/unresolved-import")
    ).toBe(true);

    fs.writeFileSync(remappingsPath, "@src/=src/\n", "utf8");

    const refreshedDiagnostics = waitForDiagnostics(client, mainUri);
    notifyWatchedFilesChanged(client, [{ uri: remappingsUri, type: 2 }]);
    const refreshed = await refreshedDiagnostics;

    expect(
      refreshed.diagnostics.some((diagnostic) => diagnostic.code === "compiler/unresolved-import")
    ).toBe(false);
    expect(
      refreshed.diagnostics.some(
        (diagnostic) => diagnostic.code === "compiler/unresolved-base-contract"
      )
    ).toBe(false);
  });

});
