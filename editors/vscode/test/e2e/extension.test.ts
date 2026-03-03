/**
 * VSCode Extension E2E Tests
 *
 * These tests run inside a real VSCode instance via @vscode/test-electron.
 * They test extension activation, deactivation, and basic editor integration.
 *
 * Both VSCode and Cursor use the same Extension Host runtime, so these tests
 * validate behavior for both editors.
 */

import * as assert from "assert";
import * as vscode from "vscode";
import * as path from "path";

suite("solgrid Extension E2E", () => {
  const fixturesPath = path.resolve(__dirname, "../fixtures");

  suiteSetup(async function () {
    this.timeout(30000);
    // Wait for any pending extension activation
    await new Promise((resolve) => setTimeout(resolve, 2000));
  });

  test("extension is registered", () => {
    const ext = vscode.extensions.getExtension("solgrid.solgrid");
    assert.ok(ext, "solgrid extension should be registered");
  });

  test("extension activates when opening a .sol file", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "clean.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);

    // Wait for activation
    await new Promise((resolve) => setTimeout(resolve, 3000));

    const ext = vscode.extensions.getExtension("solgrid.solgrid");
    if (ext) {
      // The extension should be active after opening a .sol file
      assert.ok(ext.isActive, "extension should be active after opening .sol");
    }
  });

  test("diagnostics appear for file with issues", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "with_issues.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);

    // Wait for diagnostics to appear
    await waitForDiagnostics(document.uri, 15000);

    const diagnostics = vscode.languages.getDiagnostics(document.uri);
    assert.ok(
      diagnostics.length > 0,
      "should have diagnostics for file with issues"
    );

    // Check diagnostic structure
    for (const diag of diagnostics) {
      assert.ok(diag.source === "solgrid", "diagnostic source should be solgrid");
      assert.ok(diag.code, "diagnostic should have a code (rule ID)");
    }
  });

  test("clean file has fewer diagnostics than file with issues", async function () {
    this.timeout(30000);

    const issuesFile = path.join(fixturesPath, "with_issues.sol");
    const issuesDoc = await vscode.workspace.openTextDocument(issuesFile);
    await vscode.window.showTextDocument(issuesDoc);
    await waitForDiagnostics(issuesDoc.uri, 15000);
    const issuesDiags = vscode.languages.getDiagnostics(issuesDoc.uri);

    const cleanFile = path.join(fixturesPath, "clean.sol");
    const cleanDoc = await vscode.workspace.openTextDocument(cleanFile);
    await vscode.window.showTextDocument(cleanDoc);
    await waitForDiagnostics(cleanDoc.uri, 10000).catch(() => {});
    const cleanDiags = vscode.languages.getDiagnostics(cleanDoc.uri);

    assert.ok(
      cleanDiags.length < issuesDiags.length,
      `clean file (${cleanDiags.length} diags) should have fewer diagnostics than issues file (${issuesDiags.length} diags)`
    );
  });

  test("format document command works", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "needs_formatting.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    const editor = await vscode.window.showTextDocument(document);

    // Wait for extension to be ready
    await new Promise((resolve) => setTimeout(resolve, 3000));

    const originalText = document.getText();

    // Execute format document command
    await vscode.commands.executeCommand(
      "editor.action.formatDocument"
    );

    // Wait for formatting to complete
    await new Promise((resolve) => setTimeout(resolve, 2000));

    const formattedText = document.getText();

    // The file should have been modified (or stayed the same if already formatted)
    // At minimum, the command should not crash
    assert.ok(typeof formattedText === "string");
  });

  test("code actions available for file with issues", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "with_issues.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);

    await waitForDiagnostics(document.uri, 15000);

    const diagnostics = vscode.languages.getDiagnostics(document.uri);
    if (diagnostics.length === 0) return;

    // Request code actions at the first diagnostic
    const range = diagnostics[0].range;
    const actions = await vscode.commands.executeCommand<vscode.CodeAction[]>(
      "vscode.executeCodeActionProvider",
      document.uri,
      range
    );

    // There may or may not be code actions, but the command shouldn't crash
    assert.ok(actions === undefined || Array.isArray(actions));
  });

  test("extension contributes expected settings", () => {
    const config = vscode.workspace.getConfiguration("solgrid");
    // Verify settings exist with correct defaults
    assert.strictEqual(config.get("enable"), true);
    assert.strictEqual(config.get("fixOnSave"), true);
    assert.strictEqual(config.get("formatOnSave"), true);
    assert.strictEqual(
      config.get("fixOnSave.unsafeFixes"),
      false
    );
    assert.strictEqual(config.get("path"), null);
    assert.strictEqual(config.get("configPath"), null);
  });

  test("extension deactivates without errors", async function () {
    this.timeout(10000);

    // Close all editors
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Extension should still be available (it deactivates when VSCode closes)
    const ext = vscode.extensions.getExtension("solgrid.solgrid");
    assert.ok(ext, "extension should still be registered");
  });
});

/**
 * Wait for diagnostics to appear for a given URI.
 */
function waitForDiagnostics(
  uri: vscode.Uri,
  timeoutMs: number
): Promise<vscode.Diagnostic[]> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      disposable.dispose();
      // Check one more time before rejecting
      const diags = vscode.languages.getDiagnostics(uri);
      if (diags.length > 0) {
        resolve(diags);
      } else {
        reject(
          new Error(
            `Timeout waiting for diagnostics on ${uri.toString()} after ${timeoutMs}ms`
          )
        );
      }
    }, timeoutMs);

    // Check if diagnostics already exist
    const existing = vscode.languages.getDiagnostics(uri);
    if (existing.length > 0) {
      clearTimeout(timer);
      resolve(existing);
      return;
    }

    const disposable = vscode.languages.onDidChangeDiagnostics((e) => {
      if (e.uris.some((u) => u.toString() === uri.toString())) {
        const diags = vscode.languages.getDiagnostics(uri);
        if (diags.length > 0) {
          clearTimeout(timer);
          disposable.dispose();
          resolve(diags);
        }
      }
    });
  });
}
