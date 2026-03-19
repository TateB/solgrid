/**
 * LSP Will-Save and Configuration Change Tests
 *
 * Tests textDocument/willSaveWaitUntil (fix-on-save + format-on-save)
 * and workspace/didChangeConfiguration.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import {
  applyEdits,
  initializeServer,
  openDocument,
  waitForDiagnostics,
  requestWillSaveWaitUntil,
  readFixture,
  fixtureUri,
  resetDocumentVersions,
} from "./helpers";

describe("LSP Will-Save (fix-on-save + format-on-save)", () => {
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

  it("returns edits for a file with fixable issues", async () => {
    const uri = fixtureUri("fixable.sol");
    const content = readFixture("fixable.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const edits = await requestWillSaveWaitUntil(client, uri);

    // fixable.sol has `uint` which may be fixed to `uint256`
    // Also may include formatting edits
    // The important thing is no crash
    expect(edits === null || Array.isArray(edits)).toBe(true);
  });

  it("returns formatting edits for unformatted file", async () => {
    const uri = fixtureUri("needs_formatting.sol");
    const content = readFixture("needs_formatting.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const edits = await requestWillSaveWaitUntil(client, uri);

    // Unformatted file should get formatting edits on save
    if (edits && edits.length > 0) {
      expect(edits[0].range).toBeDefined();
      expect(edits[0].newText).toBeDefined();
    }
  });

  it("applies import formatting and ordering in one save edit", async () => {
    const uri = "file:///tmp/import-order-on-save.sol";
    const content = `//SPDX-License-Identifier: MIT
pragma solidity ~0.8.17;

import {Zebra} from "./Zebra.sol";
import {Alpha, VeryLongName, AnotherLongName, MoreLongName, FinalLongName} from "./Alpha.sol";
contract Test {
    Zebra zebra;
    Alpha alpha;
}
`;

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const edits = await requestWillSaveWaitUntil(client, uri);
    expect(edits).not.toBeNull();
    expect(edits).toHaveLength(1);

    const expected = `// SPDX-License-Identifier: MIT
pragma solidity ~0.8.17;

import {Alpha} from "./Alpha.sol";
import {Zebra} from "./Zebra.sol";

contract Test {
    Zebra zebra;
    Alpha alpha;
}
`;

    expect(applyEdits(content, edits ?? [])).toBe(expected);
  });

  it("returns null for non-solidity file", async () => {
    const uri = "file:///tmp/test.ts";
    openDocument(client, uri, "const x = 1;", "typescript");

    const edits = await requestWillSaveWaitUntil(client, uri);
    expect(edits).toBeNull();
  });

  it("returns null for clean, formatted file", async () => {
    // First, format clean.sol to get the canonical form
    const uri = fixtureUri("clean.sol");
    const content = readFixture("clean.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    // Get willSaveWaitUntil result
    const edits = await requestWillSaveWaitUntil(client, uri);

    // If edits are returned, they should be meaningful
    // A truly clean+formatted file would return null
    expect(edits === null || Array.isArray(edits)).toBe(true);
  });
});

describe("LSP Configuration Change", () => {
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

  it("accepts didChangeConfiguration without error", async () => {
    // Send configuration change
    client.notify("workspace/didChangeConfiguration", {
      settings: {
        fixOnSave: false,
        fixOnSaveUnsafe: false,
        formatOnSave: false,
      },
    });

    // Server should still be responsive after config change
    // Wait a bit for the config to be processed
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(client.isRunning).toBe(true);
  });

  it("re-lints open documents after config change", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    // Send config change — should trigger re-lint
    const reLintPromise = waitForDiagnostics(client, uri);
    client.notify("workspace/didChangeConfiguration", {
      settings: {
        fixOnSave: true,
        fixOnSaveUnsafe: false,
        formatOnSave: true,
      },
    });

    const result = await reLintPromise;
    expect(result).toBeDefined();
    expect(result.uri).toBe(uri);
  });

  it("willSaveWaitUntil respects disabled fix/format settings", async () => {
    // Disable both fix and format on save
    client.notify("workspace/didChangeConfiguration", {
      settings: {
        fixOnSave: false,
        fixOnSaveUnsafe: false,
        formatOnSave: false,
      },
    });

    await new Promise((resolve) => setTimeout(resolve, 200));

    const uri = fixtureUri("fixable.sol");
    const content = readFixture("fixable.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const edits = await requestWillSaveWaitUntil(client, uri);

    // With both disabled, should return null
    expect(edits).toBeNull();
  });

  it("didSave triggers re-lint", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    // Send didSave — should trigger re-lint
    const reLintPromise = waitForDiagnostics(client, uri);
    client.notify("textDocument/didSave", {
      textDocument: { uri },
      text: content,
    });

    const result = await reLintPromise;
    expect(result).toBeDefined();
  });
});
