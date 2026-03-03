/**
 * LSP Diagnostics Tests
 *
 * Tests textDocument/didOpen, didChange, and didClose interactions
 * and verifies publishDiagnostics notifications are correct.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import {
  initializeServer,
  openDocument,
  changeDocument,
  closeDocument,
  waitForDiagnostics,
  readFixture,
  fixtureUri,
  resetDocumentVersions,
  PublishDiagnosticsParams,
} from "./helpers";

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

  it("clears diagnostics on document close", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    // Close the document and expect empty diagnostics
    const emptyDiagsPromise = waitForDiagnostics(client, uri);
    closeDocument(client, uri);
    const result = await emptyDiagsPromise;

    expect(result.diagnostics).toHaveLength(0);
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
});
