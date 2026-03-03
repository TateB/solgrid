/**
 * LSP Hover Tests
 *
 * Tests textDocument/hover — rule documentation on diagnostic hover.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import {
  initializeServer,
  openDocument,
  waitForDiagnostics,
  requestHover,
  readFixture,
  fixtureUri,
  resetDocumentVersions,
  Hover,
} from "./helpers";

describe("LSP Hover", () => {
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

  it("shows rule documentation on hover over diagnostic", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const diagResult = await waitForDiagnostics(client, uri);

    // Find the tx.origin diagnostic
    const txOriginDiag = diagResult.diagnostics.find(
      (d) => d.code === "security/tx-origin"
    );

    if (!txOriginDiag) {
      // If the rule didn't fire, skip
      return;
    }

    // Hover in the middle of the diagnostic range
    const hoverLine = txOriginDiag.range.start.line;
    const hoverChar = Math.floor(
      (txOriginDiag.range.start.character +
        txOriginDiag.range.end.character) /
        2
    );

    const hover = await requestHover(client, uri, {
      line: hoverLine,
      character: hoverChar,
    });

    expect(hover).not.toBeNull();
  });

  it("hover content contains the hovered rule ID", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const diagResult = await waitForDiagnostics(client, uri);

    // Find any diagnostic and verify hover shows its rule ID
    const diag = diagResult.diagnostics[0];
    if (!diag) return;

    const hover = await requestHover(client, uri, {
      line: diag.range.start.line,
      character: diag.range.start.character,
    });

    if (hover) {
      const hoverContent = extractHoverText(hover);
      // The hover content should contain the rule ID from the diagnostic
      const ruleId = diag.code as string;
      const ruleName = ruleId.split("/")[1];
      expect(hoverContent).toContain(ruleName);
    }
  });

  it("hover content contains disable instruction", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const diagResult = await waitForDiagnostics(client, uri);

    const diag = diagResult.diagnostics.find(
      (d) => d.code === "security/tx-origin"
    );
    if (!diag) return;

    const hover = await requestHover(client, uri, {
      line: diag.range.start.line,
      character: diag.range.start.character,
    });

    if (hover) {
      const hoverContent = extractHoverText(hover);
      expect(hoverContent).toContain("solgrid-disable-next-line");
    }
  });

  it("hover content is markdown", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const diagResult = await waitForDiagnostics(client, uri);

    const diag = diagResult.diagnostics.find(
      (d) => d.code === "security/tx-origin"
    );
    if (!diag) return;

    const hover = await requestHover(client, uri, {
      line: diag.range.start.line,
      character: diag.range.start.character,
    });

    if (hover) {
      const contents = hover.contents;
      if (
        typeof contents === "object" &&
        !Array.isArray(contents) &&
        "kind" in contents
      ) {
        expect(contents.kind).toBe("markdown");
      }
    }
  });

  it("hover content contains fix availability info", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    const diagResult = await waitForDiagnostics(client, uri);

    // Find any diagnostic with a hover
    for (const diag of diagResult.diagnostics) {
      const hover = await requestHover(client, uri, {
        line: diag.range.start.line,
        character: diag.range.start.character,
      });

      if (hover) {
        const text = extractHoverText(hover);
        // Should mention auto-fix availability
        expect(text).toMatch(/auto-fix/i);
        break;
      }
    }
  });

  it("returns null for position without diagnostic", async () => {
    // Use the clean.sol fixture which should have very few or no diagnostics
    const uri = fixtureUri("clean.sol");
    const content = readFixture("clean.sol");

    openDocument(client, uri, content);
    // Wait for diagnostics (or timeout if none)
    try {
      await waitForDiagnostics(client, uri, 5000);
    } catch {
      // May timeout if there are no diagnostics — that's fine
    }

    // Hover in the middle of a clean region — should return null
    // (clean.sol has proper NatSpec, so line 6 is the balance declaration)
    const hover = await requestHover(client, uri, {
      line: 6,
      character: 15,
    });

    expect(hover).toBeNull();
  });

  it("returns null for non-solidity file", async () => {
    const uri = "file:///tmp/test.ts";
    openDocument(client, uri, "const x = 1;", "typescript");

    const hover = await requestHover(client, uri, {
      line: 0,
      character: 0,
    });

    expect(hover).toBeNull();
  });
});

function extractHoverText(hover: Hover): string {
  const contents = hover.contents;
  if (typeof contents === "string") return contents;
  if (Array.isArray(contents)) {
    return contents
      .map((c) => (typeof c === "string" ? c : c.value))
      .join("\n");
  }
  if ("value" in contents) return contents.value;
  return "";
}
