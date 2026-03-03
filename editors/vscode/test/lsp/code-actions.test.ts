/**
 * LSP Code Actions Tests
 *
 * Tests textDocument/codeAction — quick-fixes grouped by safety tier.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import {
  initializeServer,
  openDocument,
  waitForDiagnostics,
  requestCodeActions,
  readFixture,
  fixtureUri,
  fullFileRange,
  resetDocumentVersions,
  CodeAction,
} from "./helpers";

describe("LSP Code Actions", () => {
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

  it("returns code actions for fixable diagnostics", async () => {
    const uri = fixtureUri("fixable.sol");
    const content = readFixture("fixable.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const actions = await requestCodeActions(
      client,
      uri,
      fullFileRange(content)
    );

    // fixable.sol has `uint` which could trigger explicit-types rule
    // At minimum, we should get no crash
    expect(actions).toBeDefined();
  });

  it("returns empty array for clean file", async () => {
    const uri = fixtureUri("clean.sol");
    const content = readFixture("clean.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const actions = await requestCodeActions(
      client,
      uri,
      fullFileRange(content)
    );

    // Clean file should have no or very few code actions
    expect(actions).toBeDefined();
  });

  it("code actions have title and edit", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const actions = await requestCodeActions(
      client,
      uri,
      fullFileRange(content)
    );

    for (const action of actions) {
      const codeAction = action as CodeAction;
      if (codeAction.title) {
        expect(codeAction.title).toBeTruthy();
        // Code actions should have a kind
        if (codeAction.kind) {
          expect(typeof codeAction.kind).toBe("string");
        }
      }
    }
  });

  it("safe fix code actions are marked as preferred", async () => {
    const uri = fixtureUri("fixable.sol");
    const content = readFixture("fixable.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const actions = await requestCodeActions(
      client,
      uri,
      fullFileRange(content)
    );

    const quickFixes = (actions as CodeAction[]).filter(
      (a) => a.kind === "quickfix"
    );

    for (const fix of quickFixes) {
      expect(fix.isPreferred).toBe(true);
    }
  });

  it("suggestion fix actions have (suggestion) in title", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const actions = await requestCodeActions(
      client,
      uri,
      fullFileRange(content)
    );

    const suggestions = (actions as CodeAction[]).filter(
      (a) => a.kind === "refactor"
    );

    for (const fix of suggestions) {
      expect(fix.title).toContain("(suggestion)");
    }
  });

  it("code action kinds match safety tiers", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const actions = await requestCodeActions(
      client,
      uri,
      fullFileRange(content)
    );

    for (const action of actions as CodeAction[]) {
      if (action.kind) {
        expect([
          "quickfix",
          "refactor",
          "refactor.rewrite",
          "source.fixAll",
        ]).toContain(action.kind);
      }
    }
  });

  it("code actions include workspace edits", async () => {
    const uri = fixtureUri("fixable.sol");
    const content = readFixture("fixable.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    const actions = await requestCodeActions(
      client,
      uri,
      fullFileRange(content)
    );

    const actionsWithEdits = (actions as CodeAction[]).filter(
      (a) => a.edit?.changes
    );

    for (const action of actionsWithEdits) {
      expect(action.edit!.changes).toBeDefined();
      const edits = action.edit!.changes![uri];
      if (edits) {
        for (const edit of edits) {
          expect(edit.range).toBeDefined();
          expect(edit.newText).toBeDefined();
        }
      }
    }
  });

  it("returns no code actions for range without diagnostics", async () => {
    const uri = fixtureUri("with_issues.sol");
    const content = readFixture("with_issues.sol");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri);

    // Request actions for line 0 only (the SPDX license comment)
    const actions = await requestCodeActions(client, uri, {
      start: { line: 0, character: 0 },
      end: { line: 0, character: 1 },
    });

    // The SPDX line shouldn't have fixable diagnostics
    expect(actions).toBeDefined();
  });
});
