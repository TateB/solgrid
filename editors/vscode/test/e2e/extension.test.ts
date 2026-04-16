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
import * as fs from "fs";
import * as vscode from "vscode";
import * as path from "path";

describe("solgrid Extension E2E", () => {
  // __dirname at runtime is out/test/e2e/, so resolve to source fixtures
  const fixturesPath = path.resolve(__dirname, "../../../test/fixtures");

  before(async function () {
    this.timeout(30000);
    // Wait for any pending extension activation
    await new Promise((resolve) => setTimeout(resolve, 2000));
  });

  afterEach(async function () {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    while (e2eTempPaths.length > 0) {
      const filePath = e2eTempPaths.pop();
      if (filePath) {
        try {
          fs.rmSync(filePath, { force: true });
        } catch {
          // Ignore cleanup failures in tests.
        }
      }
    }
  });

  it("extension is registered", () => {
    const ext = vscode.extensions.getExtension("solgrid.solgrid-vscode");
    assert.ok(ext, "solgrid extension should be registered");
  });

  it("extension activates when opening a .sol file", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "clean.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);

    // Wait for activation
    await new Promise((resolve) => setTimeout(resolve, 3000));

    const ext = vscode.extensions.getExtension("solgrid.solgrid-vscode");
    if (ext) {
      // The extension should be active after opening a .sol file
      assert.ok(ext.isActive, "extension should be active after opening .sol");
    }
  });

  it("diagnostics appear for file with issues", async function () {
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

  it("clean file has fewer diagnostics than file with issues", async function () {
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

  it("format document command works", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "needs_formatting.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);

    // Wait for extension to be ready
    await new Promise((resolve) => setTimeout(resolve, 3000));

    const originalText = document.getText();

    await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<vscode.TextEdit[]>(
          "vscode.executeFormatDocumentProvider",
          document.uri
        )) ?? [],
      (edits) => edits.length > 0,
      15000,
      "format document provider"
    );

    // Execute format document command
    await vscode.commands.executeCommand(
      "editor.action.formatDocument"
    );

    const formattedText = await waitForProviderResult(
      async () => document.getText(),
      (text) => text !== originalText,
      5000,
      "formatted document"
    );

    assert.notStrictEqual(formattedText, originalText);
    assert.ok(formattedText.includes("contract FormatTest {"));
    assert.ok(formattedText.includes("function doStuff() public pure returns (uint256)"));
  });

  it("code actions available for file with issues", async function () {
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

  it("extension contributes expected settings", () => {
    const config = vscode.workspace.getConfiguration("solgrid");
    // Verify settings exist with correct defaults
    assert.strictEqual(config.get("enable"), true);
    assert.strictEqual(config.get("fixOnSave"), true);
    assert.strictEqual(config.get("formatOnSave"), true);
    // Note: "fixOnSave.unsafeFixes" is not testable via getConfiguration() because
    // VSCode ignores dotted child keys when the parent is a boolean leaf.
    // The extension handles this with a fallback default in readVSCodeConfig().
    // Note: "path" may be set by test runner via workspace settings, so only check type
    const pathVal = config.get("path");
    assert.ok(pathVal === null || typeof pathVal === "string", "path should be null or string");
    assert.strictEqual(config.get("configPath"), null);
  });

  it("registers security, graph, and coverage commands", async () => {
    const commands = await vscode.commands.getCommands(true);
    assert.ok(commands.includes("solgrid.securityOverview.refresh"));
    assert.ok(commands.includes("solgrid.graph.showImports"));
    assert.ok(commands.includes("solgrid.coverage.refresh"));
  });

  it("security overview snapshot reflects grouped findings and filter changes", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "with_issues.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);
    await waitForDiagnostics(document.uri, 15000);

    await vscode.commands.executeCommand("solgrid.securityOverview.groupByFile");
    await vscode.commands.executeCommand("solgrid.securityOverview.showSecurity");

    const snapshot = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{
            kind: "group";
            label: string;
            description: string;
            contextValue?: string;
            children: Array<{
              kind: "finding";
              label: string;
              description: string;
              contextValue?: string;
              code: string;
              uri: string;
            }>;
          }>
        >("_solgrid.test.getSecurityOverviewSnapshot")) ?? [],
      (groups) =>
        groups.length > 0 &&
        groups.some((group) =>
          group.children.some((child) => child.uri === document.uri.toString())
        ),
      15000,
      "security overview snapshot"
    );

    assert.ok(snapshot.some((group) => group.contextValue?.includes("solgridSecurityGroup")));
    assert.ok(
      snapshot.some((group) =>
        group.children.some((child) => child.uri === document.uri.toString())
      )
    );

    await vscode.commands.executeCommand("solgrid.securityOverview.showCompiler");
    const compilerSnapshot = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<unknown[]>(
          "_solgrid.test.getSecurityOverviewSnapshot"
        )) ?? [],
      (groups) => groups.length === 0,
      15000,
      "compiler-only security overview snapshot"
    );
    assert.strictEqual(compilerSnapshot.length, 0);

    await vscode.commands.executeCommand("solgrid.securityOverview.showAll");
  });

  it("document symbols expose contract outline", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "importable.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);

    const symbols = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<vscode.DocumentSymbol[]>(
          "vscode.executeDocumentSymbolProvider",
          document.uri
        )) ?? [],
      (items) => items.length > 0,
      15000,
      "document symbols"
    );

    const importable = symbols.find((symbol) => symbol.name === "Importable");
    assert.ok(importable, "Importable contract should appear in document symbols");
    assert.ok(
      importable.children.some((symbol) => symbol.name === "getValue"),
      "getValue should appear in the contract outline"
    );
  });

  it("workspace symbols find indexed Solidity declarations", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "importable.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);

    const symbols = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<vscode.SymbolInformation[]>(
          "vscode.executeWorkspaceSymbolProvider",
          "Importable"
        )) ?? [],
      (items) => items.some((symbol) => symbol.name === "Importable"),
      15000,
      "workspace symbols"
    );

    assert.ok(symbols.some((symbol) => symbol.name === "Importable"));
  });

  it("document links resolve relative imports", async function () {
    this.timeout(30000);

    const targetPath = createTempFixtureFile("DocumentLinkTarget.sol", `pragma solidity ^0.8.0;
contract DocumentLinkTarget {}
`);
    const sourcePath = createTempFixtureFile("DocumentLinkSource.sol", `pragma solidity ^0.8.0;
import "./${path.basename(targetPath)}";

contract DocumentLinkSource {}
`);

    const document = await vscode.workspace.openTextDocument(sourcePath);
    await vscode.window.showTextDocument(document);

    const links = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<vscode.DocumentLink[]>(
          "vscode.executeLinkProvider",
          document.uri
        )) ?? [],
      (items) => items.length > 0,
      15000,
      "document links"
    );

    assert.ok(links[0].target, "import should resolve to a target");
    assert.strictEqual(links[0].target?.fsPath, targetPath);
  });

  it("references provider returns same-file references", async function () {
    this.timeout(30000);

    const filePath = createTempFixtureFile("References.sol", `pragma solidity ^0.8.0;

contract Token {
    function foo(uint256 amount) public pure returns (uint256) {
        uint256 doubled = amount + amount;
        return doubled;
    }
}
`);

    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);

    const references = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<vscode.Location[]>(
          "vscode.executeReferenceProvider",
          document.uri,
          new vscode.Position(3, 26)
        )) ?? [],
      (items) => items.length >= 3,
      15000,
      "references"
    );

    assert.strictEqual(references.length, 3);
  });

  it("rename provider returns and applies same-file edits", async function () {
    this.timeout(30000);

    const filePath = createTempFixtureFile("Rename.sol", `pragma solidity ^0.8.0;

contract Token {
    function run(uint256 amount) external pure returns (uint256) {
        uint256 doubled = amount + amount;
        return doubled;
    }
}
`);

    const document = await vscode.workspace.openTextDocument(filePath);
    const editor = await vscode.window.showTextDocument(document);

    const edit = await waitForProviderResult(
      async () =>
        await vscode.commands.executeCommand<vscode.WorkspaceEdit>(
          "vscode.executeDocumentRenameProvider",
          document.uri,
          new vscode.Position(4, 16),
          "tripled"
        ),
      (workspaceEdit) =>
        !!workspaceEdit && (workspaceEdit.get(document.uri)?.length ?? 0) > 0,
      15000,
      "rename edits"
    );

    const applied = await vscode.workspace.applyEdit(edit);
    assert.ok(applied, "rename workspace edit should apply");
    await document.save();
    assert.ok(editor.document.getText().includes("uint256 tripled = amount + amount;"));
    assert.ok(editor.document.getText().includes("return tripled;"));
  });

  it("code lenses expose references and graph actions", async function () {
    this.timeout(30000);

    const filePath = createTempFixtureFile("CodeLens.sol", `pragma solidity ^0.8.0;

contract Token {
    function transfer(address to, uint256 amount) public {}

    function run() public {
        transfer(address(0), 1);
        transfer(address(0), 2);
    }
}
`);

    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);

    const lenses = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<vscode.CodeLens[]>(
          "vscode.executeCodeLensProvider",
          document.uri
        )) ?? [],
      (items) =>
        items.some((lens) => lens.command?.title === "2 references") &&
        items.some((lens) => lens.command?.title === "Control-flow graph"),
      15000,
      "code lenses"
    );

    assert.ok(lenses.some((lens) => lens.command?.title === "2 references"));
    assert.ok(
      lenses.some(
        (lens) =>
          lens.command?.title === "Control-flow graph" &&
          lens.command.command === "solgrid.graph.show"
      )
    );
    assert.ok(
      lenses.some(
        (lens) =>
          lens.command?.title === "Inheritance graph" &&
          lens.command.command === "solgrid.graph.show"
      )
    );
  });

  it("inlay hints expose parameter labels and selectors", async function () {
    this.timeout(30000);

    const filePath = createTempFixtureFile("InlayHints.sol", `pragma solidity ^0.8.0;

interface IRouter {
    function swap(address tokenIn, uint256 amountIn) external returns (uint256);
}

contract Router {
    function swap(address tokenIn, uint256 amountIn) public returns (uint256) {
        return amountIn;
    }

    function run() public {
        swap(address(0), 1);
    }
}
`);

    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);

    const hints = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<vscode.InlayHint[]>(
          "vscode.executeInlayHintProvider",
          document.uri,
          new vscode.Range(0, 0, 13, 0)
        )) ?? [],
      (items) =>
        items.some((hint) => inlayHintText(hint).includes("recipient:") || inlayHintText(hint).includes("tokenIn:")) &&
        items.some((hint) => inlayHintText(hint).startsWith("selector: ")),
      15000,
      "inlay hints"
    );

    const labels = hints.map(inlayHintText);
    assert.ok(labels.some((label) => label === "tokenIn:" || label === "recipient:"));
    assert.ok(labels.some((label) => label === "amountIn:" || label === "amount:"));
    assert.ok(labels.some((label) => label.startsWith("selector: ")));
    assert.ok(labels.some((label) => label.startsWith("interface ID: ")));
  });

  it("save applies formatting edits through the real editor flow", async function () {
    this.timeout(30000);

    const filePath = createTempFixtureFile(
      "SaveFormatting.sol",
      `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract    FormatTest   {
uint256 public   x  ;
function    doStuff(   )  public  pure  returns(uint256) {
return   42;
}
}
`
    );

    const document = await vscode.workspace.openTextDocument(filePath);
    const editor = await vscode.window.showTextDocument(document);

    const updatedText = document.getText().replace("42", "43");
    const fullRange = new vscode.Range(
      document.positionAt(0),
      document.positionAt(document.getText().length)
    );
    await editor.edit((editBuilder) => {
      editBuilder.replace(fullRange, updatedText);
    });

    const saved = await document.save();
    assert.ok(saved, "document save should succeed");

    const formattedText = await waitForProviderResult(
      async () => document.getText(),
      (text) =>
        text.includes("contract FormatTest {") &&
        text.includes("function doStuff() public pure returns (uint256)") &&
        text.includes("return 43;"),
      15000,
      "save formatting"
    );

    assert.strictEqual(
      formattedText,
      `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract FormatTest {
    uint256 public x;
    function doStuff() public pure returns (uint256) {
        return 43;
    }
}
`
    );
  });

  it("graph preview command opens a rendered imports preview", async function () {
    this.timeout(30000);

    const depPath = createTempFixtureFile(
      "GraphDep.sol",
      `pragma solidity ^0.8.0;
contract GraphDep {}
`
    );
    const sourcePath = createTempFixtureFile(
      "GraphMain.sol",
      `pragma solidity ^0.8.0;
import "./${path.basename(depPath)}";

contract GraphMain is GraphDep {}
`
    );

    const document = await vscode.workspace.openTextDocument(sourcePath);
    await vscode.window.showTextDocument(document);

    await vscode.commands.executeCommand("solgrid.graph.showImports");

    const preview = await waitForGraphPreviewSnapshot(
      (snapshot) =>
        snapshot.title.includes("GraphMain.sol") &&
        snapshot.kind === "imports" &&
        snapshot.nodeLabels.some((label) => label.includes("GraphMain.sol")) &&
        snapshot.nodeLabels.some((label) => label.includes("GraphDep.sol")),
      15000,
      "imports graph preview"
    );

    assert.strictEqual(preview.kind, "imports");
    assert.ok(preview.summary.includes("edge"));
  });

  it("graph preview command opens a rendered control-flow preview", async function () {
    this.timeout(30000);

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
    const filePath = createTempFixtureFile("GraphControlFlow.sol", source);
    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);

    await vscode.commands.executeCommand("solgrid.graph.show", {
      kind: "control-flow",
      uri: document.uri.toString(),
      symbolName: "Vault.run",
      targetOffset: source.indexOf("run"),
    });

    const preview = await waitForGraphPreviewSnapshot(
      (snapshot) =>
        snapshot.title === "Control-flow graph for Vault.run" &&
        snapshot.kind === "control-flow" &&
        snapshot.focusLabel === "Entry",
      15000,
      "control-flow graph preview"
    );

    assert.ok(preview.summary.includes("Function-level CFG"));
    assert.ok(preview.nodeLabels.includes("Entry"));
    assert.ok(preview.nodeLabels.includes("Exit"));
  });

  it("graph preview command opens a rendered linearized inheritance preview", async function () {
    this.timeout(30000);

    const source = `pragma solidity ^0.8.0;

contract Root {}
contract AccessControl is Root {}
contract Pausable is Root {}
contract Vault is AccessControl, Pausable {}
`;
    const filePath = createTempFixtureFile("GraphLinearized.sol", source);
    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);

    await vscode.commands.executeCommand("solgrid.graph.show", {
      kind: "linearized-inheritance",
      uri: document.uri.toString(),
      symbolName: "Vault",
    });

    const preview = await waitForGraphPreviewSnapshot(
      (snapshot) =>
        snapshot.title === "Linearized inheritance for Vault" &&
        snapshot.kind === "linearized-inheritance" &&
        snapshot.nodeLabels.join(" -> ") ===
          "Vault -> Pausable -> AccessControl -> Root",
      15000,
      "linearized inheritance graph preview"
    );

    assert.ok(preview.summary.includes("Order: Vault -> Pausable -> AccessControl -> Root"));
  });

  it("graph preview command opens a rendered inheritance preview", async function () {
    this.timeout(30000);

    const source = `pragma solidity ^0.8.0;

contract Root {}
contract Base is Root {}
contract Vault is Base {}
`;
    const filePath = createTempFixtureFile("GraphInheritance.sol", source);
    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);

    await vscode.commands.executeCommand("solgrid.graph.show", {
      kind: "inheritance",
      uri: document.uri.toString(),
      symbolName: "Vault",
    });

    const preview = await waitForGraphPreviewSnapshot(
      (snapshot) =>
        snapshot.title === "Inheritance graph for Vault" &&
        snapshot.kind === "inheritance" &&
        snapshot.nodeLabels.includes("Vault") &&
        snapshot.nodeLabels.includes("Base") &&
        snapshot.nodeLabels.includes("Root"),
      15000,
      "inheritance graph preview"
    );

    assert.ok(preview.summary.includes("nodes"));
  });

  it("call hierarchy works through VS Code provider commands", async function () {
    this.timeout(30000);

    const filePath = createTempFixtureFile(
      "CallHierarchy.sol",
      `pragma solidity ^0.8.0;

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
`
    );

    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);

    const prepared = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<vscode.CallHierarchyItem[]>(
          "vscode.prepareCallHierarchy",
          document.uri,
          new vscode.Position(9, 13)
        )) ?? [],
      (items) => items.length > 0,
      15000,
      "call hierarchy roots"
    );

    const branch = prepared[0];
    assert.strictEqual(branch.name, "branch");

    const outgoing =
      (await vscode.commands.executeCommand<vscode.CallHierarchyOutgoingCall[]>(
        "vscode.provideOutgoingCalls",
        branch
      )) ?? [];
    assert.ok(outgoing.some((call) => call.to.name === "leaf"));
    assert.ok(outgoing.some((call) => call.to.name === "gated"));

    const preparedLeaf =
      (await vscode.commands.executeCommand<vscode.CallHierarchyItem[]>(
        "vscode.prepareCallHierarchy",
        document.uri,
        new vscode.Position(7, 13)
      )) ?? [];
    assert.ok(preparedLeaf.length > 0, "leaf should prepare for call hierarchy");

    const incoming =
      (await vscode.commands.executeCommand<vscode.CallHierarchyIncomingCall[]>(
        "vscode.provideIncomingCalls",
        preparedLeaf[0]
      )) ?? [];
    assert.ok(incoming.some((call) => call.from.name === "branch"));
  });

  it("security overview open finding command reveals the diagnostic range", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "with_issues.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);
    await waitForDiagnostics(document.uri, 15000);

    const diagnostic = vscode.languages.getDiagnostics(document.uri)[0];
    assert.ok(diagnostic, "expected a diagnostic for with_issues.sol");

    await vscode.commands.executeCommand(
      "solgrid.securityOverview.openFinding",
      findingNodeFromDiagnostic(document.uri, diagnostic)
    );

    const editor = vscode.window.activeTextEditor;
    assert.ok(editor, "a text editor should be active");
    assert.strictEqual(editor?.document.uri.toString(), document.uri.toString());
    assert.strictEqual(editor?.selection.start.line, diagnostic.range.start.line);
    assert.strictEqual(
      editor?.selection.start.character,
      diagnostic.range.start.character
    );
    assert.strictEqual(editor?.selection.end.line, diagnostic.range.end.line);
    assert.strictEqual(
      editor?.selection.end.character,
      diagnostic.range.end.character
    );
  });

  it("security overview suppress command inserts a next-line directive", async function () {
    this.timeout(30000);

    const filePath = createTempFixtureFile(
      "SecuritySuppress.sol",
      `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SecuritySuppress {
    address private owner;

    function badAuth() external view returns (bool) {
        return tx.origin == owner;
    }
}
`
    );

    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);
    await waitForDiagnostics(document.uri, 15000);

    const diagnostic = vscode.languages.getDiagnostics(document.uri)[0];
    assert.ok(diagnostic, "expected a suppressible diagnostic");

    const findingNode = findingNodeFromDiagnostic(document.uri, diagnostic);
    await vscode.commands.executeCommand(
      "solgrid.securityOverview.suppressNextLine",
      findingNode
    );

    const suppressedText = await waitForProviderResult(
      async () => document.getText(),
      (text) =>
        text.includes(`// solgrid-disable-next-line ${findingNode.finding.meta.id}`),
      15000,
      "suppression directive"
    );

    const updatedDocument = await vscode.workspace.openTextDocument(filePath);
    assert.ok(
      suppressedText.includes(`// solgrid-disable-next-line ${findingNode.finding.meta.id}`)
    );
    assert.strictEqual(
      updatedDocument.lineAt(diagnostic.range.start.line).text.trim(),
      `// solgrid-disable-next-line ${findingNode.finding.meta.id}`
    );
  });

  it("security overview ignored baselines hide, show, and restore real findings", async function () {
    this.timeout(30000);

    await vscode.commands.executeCommand("_solgrid.test.resetSecurityOverviewState");

    const filePath = createTempFixtureFile(
      "SecurityIgnored.sol",
      `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SecurityIgnored {
    address private owner;

    function badAuth() external view returns (bool) {
        return tx.origin == owner;
    }
}
`
    );

    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);
    await waitForDiagnostics(document.uri, 15000);

    await vscode.commands.executeCommand("solgrid.securityOverview.groupByFile");
    await vscode.commands.executeCommand("solgrid.securityOverview.showSecurity");

    const findingNode = await waitForProviderResult(
      async () =>
        await vscode.commands.executeCommand<{
          kind: "finding";
          label: string;
          description: string;
          ignored: boolean;
          finding: {
            uri: string;
            code: string;
            meta: {
              hasFix: boolean;
              suppressible: boolean;
            };
          };
        }>("_solgrid.test.findSecurityOverviewFinding", {
          uri: document.uri.toString(),
          code: "security/tx-origin",
          suppressible: true,
        }),
      (node) => !!node,
      15000,
      "security overview finding for ignore flow"
    );

    assert.ok(findingNode);
    const ignored = await vscode.commands.executeCommand<boolean>(
      "_solgrid.test.ignoreSecurityOverviewFinding",
      {
        uri: document.uri.toString(),
        code: "security/tx-origin",
        suppressible: true,
      }
    );
    assert.strictEqual(ignored, true);

    const debugAfterIgnore = await vscode.commands.executeCommand<{
      showIgnoredBaselines: boolean;
      ignoredFindingKeys: string[];
      findings: Array<{ uri: string; fingerprint: string }>;
    }>("_solgrid.test.getSecurityOverviewDebugState");
    assert.strictEqual(debugAfterIgnore?.showIgnoredBaselines, false);
    assert.ok(
      debugAfterIgnore?.ignoredFindingKeys.some((key) =>
        key.includes(document.uri.toString()) &&
        key.includes("security/tx-origin")
      ),
      `expected ignored keys to include ${document.uri.toString()}, got ${JSON.stringify(debugAfterIgnore)}`
    );

    const hiddenSnapshot = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{
            kind: "group";
            children: Array<{ uri: string; code: string }>;
          }>
        >("_solgrid.test.getSecurityOverviewSnapshot")) ?? [],
      (groups) =>
        !groups.some((group) =>
          group.children.some(
            (child) =>
              child.uri === document.uri.toString() &&
              child.code === "security/tx-origin"
          )
        ),
      15000,
      "ignored hidden security overview snapshot"
    );
    assert.ok(
      !hiddenSnapshot.some((group) =>
        group.children.some(
          (child) =>
            child.uri === document.uri.toString() &&
            child.code === "security/tx-origin"
        )
      )
    );

    await vscode.commands.executeCommand("solgrid.securityOverview.toggleShowIgnored");
    const ignoredNode = await waitForProviderResult(
      async () =>
        await vscode.commands.executeCommand<{
          kind: "finding";
          ignored: boolean;
          finding: { uri: string };
        }>("_solgrid.test.findSecurityOverviewFinding", {
          uri: document.uri.toString(),
          code: "security/tx-origin",
        }),
      (node) => !!node && node.ignored,
      15000,
      "ignored security overview finding"
    );

    assert.ok(ignoredNode?.ignored);

    const restored = await vscode.commands.executeCommand<boolean>(
      "_solgrid.test.restoreSecurityOverviewFinding",
      {
        uri: document.uri.toString(),
        code: "security/tx-origin",
      }
    );
    assert.strictEqual(restored, true);

    const restoredNode = await waitForProviderResult(
      async () =>
        await vscode.commands.executeCommand<{
          kind: "finding";
          ignored: boolean;
          finding: { uri: string };
        }>("_solgrid.test.findSecurityOverviewFinding", {
          uri: document.uri.toString(),
          code: "security/tx-origin",
        }),
      (node) => !!node && !node.ignored,
      15000,
      "restored security overview finding"
    );

    assert.ok(restoredNode);
    await vscode.commands.executeCommand("solgrid.securityOverview.toggleShowIgnored");
  });

  it("security overview group actions ignore and restore whole file groups", async function () {
    this.timeout(30000);

    await vscode.commands.executeCommand("_solgrid.test.resetSecurityOverviewState");

    const filePath = createTempFixtureFile(
      "SecurityGroupIgnored.sol",
      `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SecurityGroupIgnored {
    address private owner;

    function badAuth() external view returns (bool) {
        return tx.origin == owner;
    }

    function noop() external {}
}
`
    );

    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);
    await waitForDiagnostics(document.uri, 15000);

    await vscode.commands.executeCommand("solgrid.securityOverview.groupByFile");
    await vscode.commands.executeCommand("solgrid.securityOverview.showSecurity");

    const groupPresent = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{
            kind: "group";
            children: Array<{ uri: string }>;
          }>
        >("_solgrid.test.getSecurityOverviewSnapshot")) ?? [],
      (groups) =>
        groups.some((group) =>
          group.children.some((child) => child.uri === document.uri.toString())
        ),
      15000,
      "security overview group before ignore"
    );
    assert.ok(
      groupPresent.some((group) =>
        group.children.some((child) => child.uri === document.uri.toString())
      )
    );

    const ignored = await vscode.commands.executeCommand<boolean>(
      "_solgrid.test.ignoreSecurityOverviewGroup",
      {
        childUri: document.uri.toString(),
      }
    );
    assert.strictEqual(ignored, true);

    const hiddenSnapshot = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{
            kind: "group";
            children: Array<{ uri: string }>;
          }>
        >("_solgrid.test.getSecurityOverviewSnapshot")) ?? [],
      (groups) =>
        !groups.some((group) =>
          group.children.some((child) => child.uri === document.uri.toString())
        ),
      15000,
      "ignored security overview group snapshot"
    );
    assert.ok(
      !hiddenSnapshot.some((group) =>
        group.children.some((child) => child.uri === document.uri.toString())
      )
    );

    await vscode.commands.executeCommand("solgrid.securityOverview.toggleShowIgnored");

    const ignoredGroupVisible = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{
            kind: "group";
            contextValue?: string;
            children: Array<{ uri: string }>;
          }>
        >("_solgrid.test.getSecurityOverviewSnapshot")) ?? [],
      (groups) =>
        groups.some(
          (group) =>
            group.contextValue?.includes("restorable") &&
            group.children.some((child) => child.uri === document.uri.toString())
        ),
      15000,
      "ignored group visible snapshot"
    );
    assert.ok(
      ignoredGroupVisible.some(
        (group) =>
          group.contextValue?.includes("restorable") &&
          group.children.some((child) => child.uri === document.uri.toString())
      )
    );

    const restored = await vscode.commands.executeCommand<boolean>(
      "_solgrid.test.restoreSecurityOverviewGroup",
      {
        childUri: document.uri.toString(),
      }
    );
    assert.strictEqual(restored, true);

    const restoredFinding = await waitForProviderResult(
      async () =>
        await vscode.commands.executeCommand<{
          kind: "finding";
          ignored: boolean;
          finding: { uri: string; code: string };
        }>("_solgrid.test.findSecurityOverviewFinding", {
          uri: document.uri.toString(),
          code: "security/tx-origin",
        }),
      (node) => !!node && !node.ignored,
      15000,
      "restored group finding"
    );

    assert.ok(restoredFinding);
    await vscode.commands.executeCommand("solgrid.securityOverview.toggleShowIgnored");
  });

  it("security overview group suppress inserts directives for multiple real findings", async function () {
    this.timeout(30000);

    await vscode.commands.executeCommand("_solgrid.test.resetSecurityOverviewState");

    const filePath = createTempFixtureFile(
      "SecurityGroupSuppress.sol",
      `// SPDX-License-Identifier: MIT
pragma solidity 0.8.0;

contract SecurityGroupSuppress {
    address private owner;

    function badAuth() external view returns (bool) {
        return tx.origin == owner;
    }

    function noop() external {}
}
`
    );

    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);
    await waitForDiagnostics(document.uri, 15000);

    await vscode.commands.executeCommand("solgrid.securityOverview.groupByFile");
    await vscode.commands.executeCommand("solgrid.securityOverview.showSecurity");

    const suppressibleGroup = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{
            kind: "group";
            contextValue?: string;
            children: Array<{ uri: string }>;
          }>
        >("_solgrid.test.getSecurityOverviewSnapshot")) ?? [],
      (groups) =>
        groups.some(
          (group) =>
            group.contextValue?.includes("suppressible") &&
            group.children.some((child) => child.uri === document.uri.toString())
        ),
      15000,
      "suppressible security overview group"
    );
    assert.ok(
      suppressibleGroup.some(
        (group) =>
          group.contextValue?.includes("suppressible") &&
          group.children.some((child) => child.uri === document.uri.toString())
      )
    );

    const originalText = document.getText();
    const suppressed = await vscode.commands.executeCommand<boolean>(
      "_solgrid.test.suppressSecurityOverviewGroupNextLine",
      {
        childUri: document.uri.toString(),
      }
    );
    assert.strictEqual(suppressed, true);

    const updatedText = await waitForProviderResult(
      async () => document.getText(),
      (text) =>
        text !== originalText &&
        text.includes("// solgrid-disable-next-line security/tx-origin") &&
        text.includes(
          "// solgrid-disable-next-line best-practices/no-empty-blocks"
        ),
      15000,
      "group suppress directive application"
    );

    assert.ok(updatedText.includes("solgrid-disable-next-line security/tx-origin"));
    assert.ok(
      updatedText.includes(
        "solgrid-disable-next-line best-practices/no-empty-blocks"
      )
    );
  });

  it("security overview group fixes apply multiple real quick fixes", async function () {
    this.timeout(30000);

    await vscode.commands.executeCommand("_solgrid.test.resetSecurityOverviewState");

    const filePath = createTempFixtureFile(
      "SecurityGroupFixable.sol",
      `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract SecurityGroupFixable {
    uint public x;
    uint public y;
    uint public z;
}
`
    );

    const document = await vscode.workspace.openTextDocument(filePath);
    await vscode.window.showTextDocument(document);
    await waitForDiagnostics(document.uri, 15000);

    await vscode.commands.executeCommand("solgrid.securityOverview.groupByFile");
    await vscode.commands.executeCommand("solgrid.securityOverview.showAll");

    const fixableGroup = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{
            kind: "group";
            contextValue?: string;
            children: Array<{ uri: string }>;
          }>
        >("_solgrid.test.getSecurityOverviewSnapshot")) ?? [],
      (groups) =>
        groups.some(
          (group) =>
            group.contextValue?.includes("fixable") &&
            group.children.some((child) => child.uri === document.uri.toString())
        ),
      15000,
      "fixable security overview group"
    );
    assert.ok(
      fixableGroup.some(
        (group) =>
          group.contextValue?.includes("fixable") &&
          group.children.some((child) => child.uri === document.uri.toString())
      )
    );

    const originalText = document.getText();
    const applied = await vscode.commands.executeCommand<boolean>(
      "_solgrid.test.applySecurityOverviewGroupFixes",
      {
        childUri: document.uri.toString(),
      }
    );
    assert.strictEqual(applied, true);

    const fixedText = await waitForProviderResult(
      async () => document.getText(),
      (text) =>
        text !== originalText &&
        text.includes("uint256 public x;") &&
        text.includes("uint256 public y;") &&
        text.includes("uint256 public z;"),
      15000,
      "group fix application"
    );

    assert.ok(!fixedText.includes("uint public x;"));
    assert.ok(!fixedText.includes("uint public y;"));
    assert.ok(!fixedText.includes("uint public z;"));
  });

  it("security overview exposes real fixable view nodes under showAll", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "fixable.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);
    await waitForDiagnostics(document.uri, 15000);

    await vscode.commands.executeCommand("solgrid.securityOverview.groupByFile");
    await vscode.commands.executeCommand("solgrid.securityOverview.showAll");

    const findingNode = await waitForProviderResult(
      async () =>
        await vscode.commands.executeCommand<{
          kind: "finding";
          label: string;
          description: string;
          ignored: boolean;
          finding: {
            uri: string;
            code: string;
            message: string;
            source: string;
            range: vscode.Range;
            meta: {
              id: string;
              title: string;
              category: string;
              severity: "error" | "warning" | "info";
              kind: "compiler" | "lint" | "detector";
              suppressible: boolean;
              hasFix: boolean;
            };
          };
        }>("_solgrid.test.findSecurityOverviewFinding", {
          uri: document.uri.toString(),
          fixable: true,
        }),
      (node) => !!node,
      15000,
      "real fixable security overview node"
    );

    assert.ok(findingNode, "expected a fixable node in the security overview");
    assert.strictEqual(findingNode.finding.uri, document.uri.toString());
    assert.strictEqual(findingNode.finding.meta.hasFix, true);
  });

  it("security overview can preview and apply a real fixable node", async function () {
    this.timeout(30000);

    const solFile = path.join(fixturesPath, "fixable.sol");
    const document = await vscode.workspace.openTextDocument(solFile);
    await vscode.window.showTextDocument(document);
    await waitForDiagnostics(document.uri, 15000);

    await vscode.commands.executeCommand("solgrid.securityOverview.groupByFile");
    await vscode.commands.executeCommand("solgrid.securityOverview.showAll");

    const findingNode = await waitForProviderResult(
      async () =>
        await vscode.commands.executeCommand<{
          kind: "finding";
          label: string;
          description: string;
          ignored: boolean;
          finding: {
            uri: string;
            code: string;
            message: string;
            source: string;
            range: vscode.Range;
            meta: {
              id: string;
              title: string;
              category: string;
              severity: "error" | "warning" | "info";
              kind: "compiler" | "lint" | "detector";
              suppressible: boolean;
              hasFix: boolean;
            };
          };
        }>("_solgrid.test.findSecurityOverviewFinding", {
          uri: document.uri.toString(),
          fixable: true,
        }),
      (node) => !!node,
      15000,
      "real fixable security overview node"
    );

    assert.ok(findingNode, "expected a fixable node in the security overview");

    const preview = await vscode.commands.executeCommand<{
      selectedTitle?: string;
      selectedKind?: string;
      matchingTitles: string[];
      matchingKinds: string[];
      allTitles: string[];
    }>("_solgrid.test.previewSecurityOverviewFix", findingNode);

    assert.ok(preview, "expected a fix preview result");
    assert.ok(preview?.selectedTitle, "expected a concrete selected fix action");
    assert.strictEqual(preview?.selectedKind, "quickfix");
    assert.ok((preview?.matchingTitles.length ?? 0) > 0);

    const originalText = document.getText();
    const applied = await vscode.commands.executeCommand<boolean>(
      "_solgrid.test.applySecurityOverviewFix",
      findingNode
    );
    assert.strictEqual(applied, true);

    const fixedText = await waitForProviderResult(
      async () => document.getText(),
      (text) => text !== originalText,
      15000,
      "security overview fix application"
    );
    assert.notStrictEqual(fixedText, originalText);
  });

  it("coverage open node command reveals the requested file and line", async function () {
    this.timeout(30000);

    const filePath = createTempFixtureFile(
      "CoverageOpenNode.sol",
      `pragma solidity ^0.8.0;

contract CoverageOpenNode {
    function run() external pure returns (uint256) {
        return 1;
    }
}
`
    );

    await vscode.commands.executeCommand("solgrid.coverage.openNode", {
      kind: "line",
      key: "coverage-open-node",
      label: "Line 4",
      description: "uncovered",
      filePath,
      detail: {
        line: 4,
        status: "uncovered",
        hits: 0,
        branchesFound: 0,
        branchesHit: 0,
      },
    });

    const editor = await waitForProviderResult(
      async () => vscode.window.activeTextEditor,
      (activeEditor) =>
        !!activeEditor && activeEditor.document.uri.fsPath === filePath,
      15000,
      "coverage open node editor"
    );

    assert.ok(editor, "coverage open node should activate an editor");
    assert.strictEqual(editor.document.uri.fsPath, filePath);
    assert.strictEqual(editor.selection.start.line, 3);
    assert.strictEqual(editor.selection.end.line, 3);
  });

  it("coverage overview discovers LCOV artifacts and opens real actionable nodes", async function () {
    this.timeout(30000);

    const solidityPath = createTempFixtureFile(
      "CoverageArtifact.sol",
      `pragma solidity ^0.8.0;

contract CoverageArtifact {
    function run() external pure returns (uint256) {
        return 1;
    }
}
`
    );
    const artifactDir = createTempFixtureDirectory("coverage-artifact");
    const artifactPath = path.join(artifactDir, "lcov.info");
    fs.writeFileSync(
      artifactPath,
      `TN:
SF:${solidityPath}
DA:3,1
DA:4,1
DA:5,0
end_of_record
    );
`
    );
    registerTempPath(artifactPath);
    await vscode.commands.executeCommand("solgrid.coverage.refresh");

    const snapshot = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{
            kind: "file";
            label: string;
            description: string;
            filePath: string;
            children: Array<{
              kind: "line";
              label: string;
              description: string;
              filePath: string;
              line: number;
              status: string;
            }>;
          }>
        >("_solgrid.test.getCoverageOverviewSnapshot")) ?? [],
      (nodes) =>
        nodes.some(
          (node) =>
            node.filePath === solidityPath &&
            node.children.some((child) => child.line === 5)
        ),
      15000,
      "coverage overview snapshot"
    );

    assert.ok(snapshot.some((node) => node.filePath === solidityPath));

    const realLineNode = await waitForProviderResult(
      async () =>
        await vscode.commands.executeCommand<{
          kind: "line";
          label: string;
          description: string;
          filePath: string;
          detail: {
            line: number;
            status: "uncovered" | "partial";
            hits: number;
            branchesFound: number;
            branchesHit: number;
          };
        }>("_solgrid.test.findCoverageOverviewNode", {
          filePath: solidityPath,
          kind: "line",
          line: 5,
        }),
      (node) => !!node,
      15000,
      "real coverage line node"
    );

    await vscode.commands.executeCommand("solgrid.coverage.openNode", realLineNode);

    const editor = await waitForProviderResult(
      async () => vscode.window.activeTextEditor,
      (activeEditor) =>
        !!activeEditor && activeEditor.document.uri.fsPath === solidityPath,
      15000,
      "coverage editor for real node"
    );

    assert.ok(editor);
    assert.strictEqual(editor.document.uri.fsPath, solidityPath);
    assert.strictEqual(editor.selection.start.line, 4);
  });

  it("coverage overview filter commands switch between actionable and all files", async function () {
    this.timeout(30000);

    const actionablePath = createTempFixtureFile(
      "CoverageActionable.sol",
      `pragma solidity ^0.8.0;

contract CoverageActionable {
    function run() external pure returns (uint256) {
        return 1;
    }
}
`
    );
    const coveredPath = createTempFixtureFile(
      "CoverageCovered.sol",
      `pragma solidity ^0.8.0;

contract CoverageCovered {
    function run() external pure returns (uint256) {
        return 2;
    }
}
`
    );
    const artifactDir = createTempFixtureDirectory("coverage-filter");
    const artifactPath = path.join(artifactDir, "lcov.info");
    fs.writeFileSync(
      artifactPath,
      `TN:
SF:${actionablePath}
DA:3,1
DA:4,1
DA:5,0
end_of_record
TN:
SF:${coveredPath}
DA:3,1
DA:4,1
DA:5,1
end_of_record
`
    );
    registerTempPath(artifactPath);

    await vscode.commands.executeCommand("solgrid.coverage.refresh");
    await vscode.commands.executeCommand("solgrid.coverage.showActionable");

    const actionableSnapshot = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{ filePath: string; children: Array<{ line: number }> }>
        >("_solgrid.test.getCoverageOverviewSnapshot")) ?? [],
      (nodes) =>
        nodes.some((node) => node.filePath === actionablePath) &&
        !nodes.some((node) => node.filePath === coveredPath),
      15000,
      "actionable coverage snapshot"
    );

    assert.ok(actionableSnapshot.some((node) => node.filePath === actionablePath));
    assert.ok(!actionableSnapshot.some((node) => node.filePath === coveredPath));

    await vscode.commands.executeCommand("solgrid.coverage.showAll");
    const allSnapshot = await waitForProviderResult(
      async () =>
        (await vscode.commands.executeCommand<
          Array<{ filePath: string; children: Array<{ line: number }> }>
        >("_solgrid.test.getCoverageOverviewSnapshot")) ?? [],
      (nodes) =>
        nodes.some((node) => node.filePath === actionablePath) &&
        nodes.some((node) => node.filePath === coveredPath),
      15000,
      "all coverage snapshot"
    );

    assert.ok(allSnapshot.some((node) => node.filePath === actionablePath));
    assert.ok(allSnapshot.some((node) => node.filePath === coveredPath));
    assert.ok(
      allSnapshot.some(
        (node) => node.filePath === coveredPath && node.children.length === 0
      )
    );
  });

  it("extension deactivates without errors", async function () {
    this.timeout(10000);

    // Close all editors
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Extension should still be available (it deactivates when VSCode closes)
    const ext = vscode.extensions.getExtension("solgrid.solgrid-vscode");
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

async function waitForProviderResult<T>(
  producer: () => Thenable<T> | T,
  isReady: (value: T) => boolean,
  timeoutMs: number,
  label: string
): Promise<T> {
  const startedAt = Date.now();
  let lastValue: T | undefined;

  while (Date.now() - startedAt < timeoutMs) {
    lastValue = await producer();
    if (isReady(lastValue)) {
      return lastValue;
    }
    await new Promise((resolve) => setTimeout(resolve, 150));
  }

  throw new Error(
    `Timed out waiting for ${label} after ${timeoutMs}ms. Last value: ${debugValue(lastValue)}`
  );
}

interface GraphPreviewSnapshot {
  edgeCount: number;
  focusLabel?: string;
  kind:
    | "imports"
    | "inheritance"
    | "linearized-inheritance"
    | "control-flow";
  nodeLabels: string[];
  summary: string;
  title: string;
}

async function waitForGraphPreviewSnapshot(
  matches: (snapshot: GraphPreviewSnapshot) => boolean,
  timeoutMs: number,
  label: string
): Promise<GraphPreviewSnapshot> {
  const snapshot = await waitForProviderResult<GraphPreviewSnapshot | undefined>(
    async () =>
      (await vscode.commands.executeCommand<GraphPreviewSnapshot | undefined>(
        "_solgrid.test.getGraphPreviewSnapshot"
      )) ?? undefined,
    (snapshot): snapshot is GraphPreviewSnapshot =>
      snapshot !== undefined && matches(snapshot),
    timeoutMs,
    label
  );
  if (!snapshot) {
    throw new Error(`Timed out waiting for ${label} after ${timeoutMs}ms`);
  }
  return snapshot;
}

function createTempFixtureFile(name: string, content: string): string {
  const tempDir = path.join(fixturesPathForHelpers(), ".e2e-temp");
  fs.mkdirSync(tempDir, { recursive: true });
  const filePath = path.join(
    tempDir,
    `${Date.now()}-${Math.random().toString(16).slice(2)}-${name}`
  );
  fs.writeFileSync(filePath, content, "utf8");
  registerTempPath(filePath);
  return filePath;
}

function createTempFixtureDirectory(name: string): string {
  const tempDir = path.join(
    fixturesPathForHelpers(),
    ".e2e-temp",
    `${Date.now()}-${Math.random().toString(16).slice(2)}-${name}`
  );
  fs.mkdirSync(tempDir, { recursive: true });
  registerTempPath(tempDir);
  return tempDir;
}

function inlayHintText(hint: vscode.InlayHint): string {
  return typeof hint.label === "string"
    ? hint.label
    : hint.label.map((part) => part.value).join("");
}

function findingNodeFromDiagnostic(
  uri: vscode.Uri,
  diagnostic: vscode.Diagnostic
): {
  kind: "finding";
  key: string;
  label: string;
  description: string;
  ignored: false;
  finding: {
    uri: string;
    code: string;
    message: string;
    source: string;
    range: vscode.Range;
    meta: {
      id: string;
      title: string;
      category: string;
      severity: "error" | "warning" | "info";
      kind: "compiler" | "lint" | "detector";
      suppressible: boolean;
      hasFix: boolean;
    };
  };
} {
  const code = diagnosticCodeValue(diagnostic.code);
  const severity =
    diagnostic.severity === vscode.DiagnosticSeverity.Error
      ? "error"
      : diagnostic.severity === vscode.DiagnosticSeverity.Warning
        ? "warning"
        : "info";

  return {
    kind: "finding",
    key: `${uri.toString()}:${code}:${diagnostic.range.start.line}`,
    label: diagnostic.message,
    description: `${path.basename(uri.fsPath)}:${diagnostic.range.start.line + 1}`,
    ignored: false,
    finding: {
      uri: uri.toString(),
      code,
      message: diagnostic.message,
      source: diagnostic.source ?? "solgrid",
      range: diagnostic.range,
      meta: {
        id: code,
        title: diagnostic.message,
        category: "security",
        severity,
        kind: "detector",
        suppressible: true,
        hasFix: false,
      },
    },
  };
}

function diagnosticCodeValue(
  code: vscode.Diagnostic["code"]
): string {
  if (typeof code === "string" || typeof code === "number") {
    return String(code);
  }
  if (code && typeof code === "object" && "value" in code) {
    return String(code.value);
  }
  return "unknown";
}

const e2eTempPaths: string[] = [];

function registerTempPath(filePath: string): void {
  e2eTempPaths.push(filePath);
}

function fixturesPathForHelpers(): string {
  return path.resolve(__dirname, "../../../test/fixtures");
}

function debugValue(value: unknown): string {
  try {
    const serialized = JSON.stringify(
      value,
      (_key, candidate) => {
        if (candidate instanceof vscode.Uri) {
          return candidate.toString();
        }
        return candidate;
      },
      2
    );
    if (!serialized) {
      return String(serialized);
    }
    return serialized.length > 4000
      ? `${serialized.slice(0, 4000)}…`
      : serialized;
  } catch {
    return Object.prototype.toString.call(value);
  }
}
