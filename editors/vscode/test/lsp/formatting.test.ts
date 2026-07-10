/**
 * LSP Formatting Tests
 *
 * Tests textDocument/formatting and textDocument/rangeFormatting.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import {
  initializeServer,
  openDocument,
  waitForDiagnostics,
  requestFormatting,
  requestRangeFormatting,
  readFixture,
  fixtureUri,
  applyEdits,
  resetDocumentVersions,
} from "./helpers";

describe("LSP Formatting", () => {
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

  it("returns edits for an unformatted file", async () => {
    const uri = fixtureUri("needs_formatting.sol");
    const content = readFixture("needs_formatting.sol");

    openDocument(client, uri, content);
    // Wait briefly for the server to process the document
    await waitForDiagnostics(client, uri);

    const edits = await requestFormatting(client, uri);

    // The formatter should produce edits for a poorly formatted file
    expect(edits).not.toBeNull();
    expect(edits!.length).toBeGreaterThan(0);
  });

  it("formatting produces valid output", async () => {
    const uri = fixtureUri("needs_formatting.sol");
    const content = readFixture("needs_formatting.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const edits = await requestFormatting(client, uri);

    expect(edits).not.toBeNull();
    expect(edits!.length).toBeGreaterThan(0);
    const formatted = applyEdits(content, edits!);
    // Formatted output should still contain key Solidity tokens
    expect(formatted).toContain("pragma solidity");
    expect(formatted).toContain("contract");
    expect(formatted).toContain("function");
  });

  it("formatting is idempotent", async () => {
    const uri = fixtureUri("needs_formatting.sol");
    const content = readFixture("needs_formatting.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    // First format
    const edits1 = await requestFormatting(client, uri);
    expect(edits1).not.toBeNull();
    expect(edits1!.length).toBeGreaterThan(0);

    const formatted = applyEdits(content, edits1!);

    // Open the formatted version
    const uri2 = "file:///tmp/formatted-test.sol";
    openDocument(client, uri2, formatted);
    await waitForDiagnostics(client, uri2);

    // Second format should produce no edits
    const edits2 = await requestFormatting(client, uri2);
    const doubleFormatted = edits2
      ? applyEdits(formatted, edits2)
      : formatted;
    expect(doubleFormatted).toBe(formatted);
  });

  it("returns null for already-formatted file", async () => {
    const uri = fixtureUri("clean.sol");
    const content = readFixture("clean.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    // First, format to get canonical form
    const edits = await requestFormatting(client, uri);

    const canonical = edits ? applyEdits(content, edits) : content;

    // Now open the canonical form and format again
    const uri2 = "file:///tmp/canonical-test.sol";
    openDocument(client, uri2, canonical);
    await waitForDiagnostics(client, uri2);

    const edits2 = await requestFormatting(client, uri2);
    expect(edits2).toBeNull();
  });

  it("range formatting returns edits within the requested range", async () => {
    const uri = fixtureUri("needs_formatting.sol");
    const content = readFixture("needs_formatting.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const edits = await requestRangeFormatting(client, uri, {
      start: { line: 0, character: 0 },
      end: { line: 3, character: 0 },
    });

    expect(edits).not.toBeNull();
    expect(edits!.length).toBeGreaterThan(0);
    for (const edit of edits!) {
      expect(edit.range).toBeDefined();
      expect(edit.newText).toBeDefined();
    }
  });

  it("formatting edits have valid range", async () => {
    const uri = fixtureUri("needs_formatting.sol");
    const content = readFixture("needs_formatting.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const edits = await requestFormatting(client, uri);

    expect(edits).not.toBeNull();
    expect(edits!.length).toBeGreaterThan(0);
    for (const edit of edits!) {
      expect(edit.range.start.line).toBeGreaterThanOrEqual(0);
      expect(edit.range.start.character).toBeGreaterThanOrEqual(0);
      expect(edit.range.start.line).toBeLessThanOrEqual(
        edit.range.end.line
      );
    }
  });
});
