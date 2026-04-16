import * as vscode from "vscode";
import {
  buildOverviewTree,
  collectFixableGroupFindings,
  collectIgnorableGroupFindings,
  collectRestorableGroupFindings,
  collectSuppressibleGroupFindings,
  buildSuppressNextLineDirective,
  extractSecurityFindings,
  findingFingerprint,
  groupContextValue,
  pickPreferredCodeActionForFinding,
  PublishDiagnosticsParamsLike,
  SecurityFinding,
  SecurityOverviewFilterMode,
  SecurityOverviewFindingNode,
  SecurityOverviewGroupMode,
  SecurityOverviewGroupNode,
  summarizeOverview,
} from "./securityOverviewModel";

export type SecurityOverviewNode =
  | SecurityOverviewGroupNode
  | SecurityOverviewFindingNode;

export class SecurityOverviewProvider
  implements vscode.TreeDataProvider<SecurityOverviewNode>
{
  static readonly ignoredFindingStorageKey =
    "solgrid.securityOverview.ignoredFindings";

  private readonly onDidChangeTreeDataEmitter =
    new vscode.EventEmitter<SecurityOverviewNode | undefined>();
  readonly onDidChangeTreeData = this.onDidChangeTreeDataEmitter.event;

  private readonly findingsByUri = new Map<string, SecurityFinding[]>();
  private readonly ignoredFindingKeys = new Set<string>();
  private groupMode: SecurityOverviewGroupMode = "file";
  private filterMode: SecurityOverviewFilterMode = "security";
  private showIgnoredBaselines = false;
  private view: vscode.TreeView<SecurityOverviewNode> | undefined;

  constructor(private readonly storage: vscode.Memento) {
    for (const key of readIgnoredFindingKeys(storage)) {
      this.ignoredFindingKeys.add(key);
    }
  }

  attachView(view: vscode.TreeView<SecurityOverviewNode>): void {
    this.view = view;
    this.updatePresentation();
  }

  updateFromDiagnostics(params: PublishDiagnosticsParamsLike): void {
    const findings = extractSecurityFindings(params);
    if (findings.length === 0) {
      this.findingsByUri.delete(params.uri);
    } else {
      this.findingsByUri.set(params.uri, findings);
    }
    this.refresh();
  }

  refresh(): void {
    this.updatePresentation();
    this.onDidChangeTreeDataEmitter.fire(undefined);
  }

  setGroupMode(mode: SecurityOverviewGroupMode): void {
    if (this.groupMode === mode) {
      return;
    }
    this.groupMode = mode;
    this.refresh();
  }

  setFilterMode(mode: SecurityOverviewFilterMode): void {
    if (this.filterMode === mode) {
      return;
    }
    this.filterMode = mode;
    this.refresh();
  }

  async ignoreFinding(node?: SecurityOverviewFindingNode): Promise<void> {
    if (!node || node.ignored) {
      return;
    }
    await this.ignoreFindings([node.finding]);
  }

  async restoreFinding(node?: SecurityOverviewFindingNode): Promise<void> {
    if (!node || !node.ignored) {
      return;
    }
    await this.restoreFindings([node.finding]);
  }

  async ignoreGroup(node?: SecurityOverviewGroupNode): Promise<void> {
    if (!node) {
      return;
    }
    await this.ignoreFindings(
      collectIgnorableGroupFindings(
        node.children.map((child) => child.finding),
        this.ignoredFindingKeys
      )
    );
  }

  async restoreGroup(node?: SecurityOverviewGroupNode): Promise<void> {
    if (!node) {
      return;
    }
    await this.restoreFindings(
      collectRestorableGroupFindings(
        node.children.map((child) => child.finding),
        this.ignoredFindingKeys
      )
    );
  }

  toggleShowIgnoredBaselines(): void {
    this.showIgnoredBaselines = !this.showIgnoredBaselines;
    this.refresh();
  }

  async clearIgnoredBaselines(): Promise<void> {
    if (this.ignoredFindingKeys.size === 0) {
      return;
    }
    this.ignoredFindingKeys.clear();
    await this.persistIgnoredFindingKeys();
    this.refresh();
  }

  getTreeItem(element: SecurityOverviewNode): vscode.TreeItem {
    if (element.kind === "group") {
      const item = new vscode.TreeItem(
        element.label,
        vscode.TreeItemCollapsibleState.Expanded
      );
      item.description = element.description;
      item.tooltip = `${element.label}\n${element.description}`;
      item.contextValue = groupContextValue(
        element.children.map((child) => child.finding),
        this.ignoredFindingKeys
      );
      item.iconPath = new vscode.ThemeIcon(groupIcon(element.key, this.groupMode));
      return item;
    }

    const item = new vscode.TreeItem(
      element.label,
      vscode.TreeItemCollapsibleState.None
    );
    item.description = element.ignored
      ? `${element.description} • ignored`
      : element.description;
    item.tooltip = findingTooltip(element.finding, element.ignored);
    item.contextValue = findingContextValue(element.finding, element.ignored);
    item.iconPath = new vscode.ThemeIcon(
      element.ignored ? "eye-closed" : severityIcon(element.finding.meta.severity)
    );
    item.command = {
      command: "solgrid.securityOverview.openFinding",
      title: "Open Finding",
      arguments: [element],
    };
    return item;
  }

  getChildren(
    element?: SecurityOverviewNode
  ): vscode.ProviderResult<SecurityOverviewNode[]> {
    if (!element) {
      return buildOverviewTree(
        this.currentFindings(),
        this.groupMode,
        this.filterMode,
        this.ignoredFindingKeys,
        this.showIgnoredBaselines
      );
    }
    if (element.kind === "group") {
      return element.children;
    }
    return [];
  }

  private currentFindings(): SecurityFinding[] {
    return Array.from(this.findingsByUri.values()).flat();
  }

  private updatePresentation(): void {
    if (!this.view) {
      return;
    }
    const summary = summarizeOverview(
      this.currentFindings(),
      this.groupMode,
      this.filterMode,
      this.ignoredFindingKeys,
      this.showIgnoredBaselines
    );
    this.view.description = summary.description;
    this.view.message = summary.message;
    this.view.badge =
      summary.count > 0
        ? {
            value: summary.count,
            tooltip: `${summary.count} findings`,
          }
        : undefined;
  }

  private async ignoreFindings(findings: readonly SecurityFinding[]): Promise<void> {
    let changed = false;
    for (const finding of findings) {
      changed =
        !this.ignoredFindingKeys.has(findingFingerprint(finding)) || changed;
      this.ignoredFindingKeys.add(findingFingerprint(finding));
    }
    if (!changed) {
      return;
    }
    await this.persistIgnoredFindingKeys();
    this.refresh();
  }

  private async restoreFindings(findings: readonly SecurityFinding[]): Promise<void> {
    let changed = false;
    for (const finding of findings) {
      changed =
        this.ignoredFindingKeys.delete(findingFingerprint(finding)) || changed;
    }
    if (!changed) {
      return;
    }
    await this.persistIgnoredFindingKeys();
    this.refresh();
  }

  private async persistIgnoredFindingKeys(): Promise<void> {
    await this.storage.update(
      SecurityOverviewProvider.ignoredFindingStorageKey,
      Array.from(this.ignoredFindingKeys).sort()
    );
  }
}

export async function openSecurityFinding(
  node?: SecurityOverviewFindingNode
): Promise<void> {
  if (!node) {
    return;
  }
  const uri = vscode.Uri.parse(node.finding.uri);
  const document = await vscode.workspace.openTextDocument(uri);
  const editor = await vscode.window.showTextDocument(document, {
    preview: false,
    preserveFocus: false,
  });
  const range = new vscode.Range(
    new vscode.Position(
      node.finding.range.start.line,
      node.finding.range.start.character
    ),
    new vscode.Position(
      node.finding.range.end.line,
      node.finding.range.end.character
    )
  );
  editor.selection = new vscode.Selection(range.start, range.end);
  editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
}

export async function openFindingHelp(
  node?: SecurityOverviewFindingNode
): Promise<void> {
  if (!node?.finding.meta.helpUrl) {
    return;
  }
  await vscode.env.openExternal(vscode.Uri.parse(node.finding.meta.helpUrl));
}

export async function suppressFindingNextLine(
  node?: SecurityOverviewFindingNode
): Promise<void> {
  if (!node?.finding.meta.suppressible) {
    return;
  }

  await suppressFindings([node.finding], true);
}

export async function applyFindingFix(
  node?: SecurityOverviewFindingNode
): Promise<void> {
  if (!node?.finding.meta.hasFix) {
    return;
  }

  await applyFixes([node.finding], true);
}

export async function suppressGroupNextLine(
  node?: SecurityOverviewGroupNode
): Promise<void> {
  if (!node) {
    return;
  }
  await suppressFindings(
    collectSuppressibleGroupFindings(node.children.map((child) => child.finding)),
    false
  );
}

export async function applyGroupFixes(
  node?: SecurityOverviewGroupNode
): Promise<void> {
  if (!node) {
    return;
  }
  await applyFixes(
    collectFixableGroupFindings(node.children.map((child) => child.finding)),
    false
  );
}

function groupIcon(
  key: string,
  groupMode: SecurityOverviewGroupMode
): string {
  switch (groupMode) {
    case "file":
      return "file";
    case "severity":
      return severityIcon(key === "error" || key === "warning" ? key : "info");
    case "confidence":
      return "shield";
    case "finding":
      return "symbol-key";
  }
}

function severityIcon(severity: "error" | "warning" | "info"): string {
  switch (severity) {
    case "error":
      return "error";
    case "warning":
      return "warning";
    case "info":
      return "info";
  }
}

function findingTooltip(finding: SecurityFinding, ignored: boolean): string {
  const helpLine = finding.meta.helpUrl ? `\n${finding.meta.helpUrl}` : "";
  const ignoredLine = ignored ? "\nIgnored in the security overview" : "";
  return `${finding.message}\n${finding.code} • ${finding.meta.kind}${helpLine}${ignoredLine}`;
}

function findingContextValue(finding: SecurityFinding, ignored: boolean): string {
  const tokens = ["solgridSecurityFinding"];
  if (finding.meta.helpUrl) {
    tokens.push("help");
  }
  if (ignored) {
    tokens.push("ignored", "restorable");
    return tokens.join(" ");
  }
  tokens.push("ignorable");
  if (finding.meta.suppressible) {
    tokens.push("suppressible");
  }
  if (finding.meta.hasFix) {
    tokens.push("fixable");
  }
  return tokens.join(" ");
}

function isCodeAction(
  action: vscode.CodeAction | vscode.Command
): action is vscode.CodeAction {
  return "edit" in action || "diagnostics" in action || "kind" in action;
}

async function suppressFindings(
  findings: readonly SecurityFinding[],
  revealFirstFinding: boolean
): Promise<void> {
  if (findings.length === 0) {
    return;
  }

  const edit = new vscode.WorkspaceEdit();

  for (const finding of findings) {
    const uri = vscode.Uri.parse(finding.uri);
    const document = await vscode.workspace.openTextDocument(uri);
    const targetLine = finding.range.start.line;
    const directive = buildSuppressNextLineDirective(
      finding.meta.id,
      document.lineAt(targetLine).text
    );

    if (
      targetLine > 0 &&
      document.lineAt(targetLine - 1).text.trim() === directive.trim()
    ) {
      continue;
    }

    edit.insert(uri, new vscode.Position(targetLine, 0), directive);
  }

  const applied = await vscode.workspace.applyEdit(edit);
  if (!applied) {
    return;
  }

  if (revealFirstFinding) {
    const [firstFinding] = findings;
    if (firstFinding) {
      await revealSuppressionDirective(firstFinding);
    }
  }
}

async function revealSuppressionDirective(finding: SecurityFinding): Promise<void> {
  const uri = vscode.Uri.parse(finding.uri);
  const updatedDocument = await vscode.workspace.openTextDocument(uri);
  const editor = await vscode.window.showTextDocument(updatedDocument, {
    preview: false,
    preserveFocus: false,
  });
  const directiveLine = updatedDocument.lineAt(finding.range.start.line).range;
  editor.selection = new vscode.Selection(directiveLine.end, directiveLine.end);
  editor.revealRange(directiveLine, vscode.TextEditorRevealType.InCenter);
}

async function applyFixes(
  findings: readonly SecurityFinding[],
  revealFirstFinding: boolean
): Promise<void> {
  if (findings.length === 0) {
    return;
  }

  let revealed = false;
  for (const finding of findings) {
    const applied = await applyPreferredFixForFinding(
      finding,
      revealFirstFinding && !revealed
    );
    if (applied && revealFirstFinding && !revealed) {
      revealed = true;
    }
  }
}

async function applyPreferredFixForFinding(
  finding: SecurityFinding,
  reveal: boolean
): Promise<boolean> {
  const uri = vscode.Uri.parse(finding.uri);
  const document = await vscode.workspace.openTextDocument(uri);
  const range = new vscode.Range(
    new vscode.Position(
      finding.range.start.line,
      finding.range.start.character
    ),
    new vscode.Position(
      finding.range.end.line,
      finding.range.end.character
    )
  );

  const actions =
    (await vscode.commands.executeCommand<
      Array<vscode.CodeAction | vscode.Command>
    >("vscode.executeCodeActionProvider", uri, range, vscode.CodeActionKind.QuickFix)) ?? [];
  const matchingActions = actions.filter(isCodeAction);
  const action = pickPreferredCodeActionForFinding<vscode.CodeAction>(
    finding,
    matchingActions
  );
  if (!action) {
    return false;
  }

  if (action.edit) {
    const applied = await vscode.workspace.applyEdit(action.edit);
    if (!applied) {
      return false;
    }
  }
  if (action.command) {
    await vscode.commands.executeCommand(
      action.command.command,
      ...(action.command.arguments ?? [])
    );
  }

  if (reveal) {
    const editor = await vscode.window.showTextDocument(document, {
      preview: false,
      preserveFocus: false,
    });
    editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
  }

  return true;
}

function readIgnoredFindingKeys(storage: vscode.Memento): string[] {
  const persisted = storage.get<unknown[]>(
    SecurityOverviewProvider.ignoredFindingStorageKey,
    []
  );
  return persisted.filter((value): value is string => typeof value === "string");
}
