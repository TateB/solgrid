import { fileURLToPath } from "node:url";
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
  type PublishDiagnosticsParamsLike,
  type SecurityFinding,
  type SecurityOverviewFilterMode,
  type SecurityOverviewFindingNode,
  type SecurityOverviewGroupMode,
  type SecurityOverviewGroupNode,
  shouldExpandSecurityGroup,
  summarizeOverview,
} from "./securityOverviewModel";

type SecurityCodeActionResolver = (
  finding: SecurityFinding
) => Promise<Array<vscode.CodeAction | vscode.Command>>;

let securityCodeActionResolver: SecurityCodeActionResolver | undefined;

export function setSecurityCodeActionResolver(
  resolver: SecurityCodeActionResolver | undefined
): void {
  securityCodeActionResolver = resolver;
}

export type SecurityOverviewNode =
  | SecurityOverviewGroupNode
  | SecurityOverviewFindingNode;

type SecurityAnalysisState = "waiting" | "running" | "complete" | "error";

export const SECURITY_ANALYSIS_PENDING_MESSAGE =
  "Waiting for security analysis results…";

export function securityOverviewPresentationMessage(
  summaryMessage: string | undefined,
  analysisComplete: boolean,
  analysisError?: string
): string | undefined {
  if (analysisError) {
    return analysisError;
  }
  return analysisComplete ? summaryMessage : SECURITY_ANALYSIS_PENDING_MESSAGE;
}

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
  private analysisState: SecurityAnalysisState = "waiting";
  private analysisError: string | undefined;
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
    if (this.analysisState === "waiting") {
      this.analysisState = "complete";
      this.analysisError = undefined;
    }
    this.refresh();
  }

  beginAnalysis(): void {
    if (this.analysisState === "running") {
      return;
    }
    this.analysisState = "running";
    this.analysisError = undefined;
    this.refresh();
  }

  completeAnalysis(): void {
    if (this.analysisState === "complete") {
      return;
    }
    this.analysisState = "complete";
    this.analysisError = undefined;
    this.refresh();
  }

  failAnalysis(message: string): void {
    const normalized = message.trim() || "Security analysis failed.";
    if (
      this.analysisState === "error" &&
      this.analysisError === normalized
    ) {
      return;
    }
    this.analysisState = "error";
    this.analysisError = normalized;
    this.refresh();
  }

  refresh(): void {
    this.updatePresentation();
    this.onDidChangeTreeDataEmitter.fire(undefined);
  }

  refreshTreeData(): void {
    this.refresh();
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
    if (!node?.ignored) {
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

  async resetForTests(): Promise<void> {
    let changed = false;

    if (this.groupMode !== "file") {
      this.groupMode = "file";
      changed = true;
    }
    if (this.filterMode !== "security") {
      this.filterMode = "security";
      changed = true;
    }
    if (this.showIgnoredBaselines) {
      this.showIgnoredBaselines = false;
      changed = true;
    }
    if (this.analysisState !== "waiting") {
      this.analysisState = "waiting";
      this.analysisError = undefined;
      changed = true;
    }
    if (this.ignoredFindingKeys.size > 0) {
      this.ignoredFindingKeys.clear();
      await this.persistIgnoredFindingKeys();
      changed = true;
    }

    if (changed) {
      this.refresh();
    }
  }

  debugStateForTests(): {
    groupMode: SecurityOverviewGroupMode;
    filterMode: SecurityOverviewFilterMode;
    showIgnoredBaselines: boolean;
    ignoredFindingKeys: string[];
    findings: Array<{
      uri: string;
      code: string;
      message: string;
      fingerprint: string;
    }>;
  } {
    const findings = this.currentFindings().map((finding) => ({
      uri: finding.uri,
      code: finding.code,
      message: finding.message,
      fingerprint: findingFingerprint(finding),
    }));

    return {
      groupMode: this.groupMode,
      filterMode: this.filterMode,
      showIgnoredBaselines: this.showIgnoredBaselines,
      ignoredFindingKeys: Array.from(this.ignoredFindingKeys).sort(),
      findings,
    };
  }

  getTreeItem(element: SecurityOverviewNode): vscode.TreeItem {
    if (element.kind === "group") {
      const item = new vscode.TreeItem(
        element.label,
        shouldExpandSecurityGroup(element.children.length)
          ? vscode.TreeItemCollapsibleState.Expanded
          : vscode.TreeItemCollapsibleState.Collapsed
      );
      item.description = element.description;
      item.tooltip = groupTooltip(element);
      item.accessibilityInformation = {
        label: securityGroupAccessibilityLabel(element),
      };
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
    item.accessibilityInformation = {
      label: securityFindingAccessibilityLabel(
        element.finding,
        element.ignored
      ),
    };
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
    this.view.message = securityOverviewPresentationMessage(
      summary.message,
      this.analysisState === "complete",
      this.analysisError
    );
    this.view.badge =
      summary.count > 0
        ? {
            value: summary.count,
            tooltip: `${summary.count} ${
              summary.count === 1 ? "finding" : "findings"
            }`,
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

const CLEAR_IGNORED_BASELINES_ACTION = "Clear Ignored Baselines";

type IgnoredBaselineClearConfirmation = () => Promise<boolean>;

export async function clearIgnoredBaselinesWithConfirmation(
  provider: Pick<SecurityOverviewProvider, "clearIgnoredBaselines">,
  confirm: IgnoredBaselineClearConfirmation = confirmIgnoredBaselineClear
): Promise<boolean> {
  if (!(await confirm())) {
    return false;
  }
  await provider.clearIgnoredBaselines();
  return true;
}

async function confirmIgnoredBaselineClear(): Promise<boolean> {
  const selected = await vscode.window.showWarningMessage(
    "Clear all ignored security baselines?",
    {
      modal: true,
      detail: "Previously ignored findings will appear again. This cannot be undone.",
    },
    CLEAR_IGNORED_BASELINES_ACTION
  );
  return selected === CLEAR_IGNORED_BASELINES_ACTION;
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
    collectSuppressibleGroupFindings(
      node.children
        .filter((child) => !child.ignored)
        .map((child) => child.finding)
    ),
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
    collectFixableGroupFindings(
      node.children
        .filter((child) => !child.ignored)
        .map((child) => child.finding)
    ),
    false
  );
}

export interface SecurityFixPreview {
  selectedTitle?: string;
  selectedKind?: string;
  matchingTitles: string[];
  matchingKinds: string[];
  allTitles: string[];
}

export async function previewFindingFix(
  node?: SecurityOverviewFindingNode
): Promise<SecurityFixPreview | undefined> {
  if (!node?.finding.meta.hasFix) {
    return undefined;
  }

  const resolution = await resolvePreferredFixForFinding(node.finding);
  return {
    selectedTitle: resolution.selected?.title,
    selectedKind: resolution.selected?.kind?.value,
    matchingTitles: resolution.matchingActions.map((action) => action.title),
    matchingKinds: resolution.matchingActions.map(
      (action) => action.kind?.value ?? ""
    ),
    allTitles: resolution.actions
      .filter(isCodeAction)
      .map((action) => action.title),
  };
}

export async function applyFindingFixForTests(
  node?: SecurityOverviewFindingNode
): Promise<boolean> {
  if (!node?.finding.meta.hasFix) {
    return false;
  }
  return applyPreferredFixForFinding(node.finding, false);
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
  const lines = [
    finding.message,
    `Rule: ${finding.code}`,
    `Severity: ${titleCase(finding.meta.severity)}`,
    `Confidence: ${titleCase(finding.meta.confidence ?? "unknown")}`,
    `Type: ${titleCase(finding.meta.kind)}`,
    `Category: ${finding.meta.category}`,
    `Location: ${findingLocation(finding)}`,
    `State: ${ignored ? "Ignored in the security overview" : "Active"}`,
    `Automatic fix: ${finding.meta.hasFix ? "Available" : "Not available"}`,
    `Suppression: ${finding.meta.suppressible ? "Available" : "Not available"}`,
  ];
  if (finding.meta.helpUrl) {
    lines.push(`Documentation: ${finding.meta.helpUrl}`);
  }
  return lines.join("\n");
}

export function securityFindingAccessibilityLabel(
  finding: SecurityFinding,
  ignored: boolean
): string {
  return [
    finding.meta.title || finding.message,
    `${finding.meta.severity} severity`,
    `${finding.meta.confidence ?? "unknown"} confidence`,
    `${finding.meta.kind} type`,
    `${finding.meta.category} category`,
    `rule ${finding.code}`,
    `location ${findingLocation(finding)}`,
    ignored ? "ignored" : "active",
    finding.meta.hasFix ? "automatic fix available" : "no automatic fix",
    finding.meta.suppressible ? "can be suppressed" : "cannot be suppressed",
  ].join("; ");
}

export function securityGroupAccessibilityLabel(
  group: SecurityOverviewGroupNode
): string {
  const findings = group.children.map((child) => child.finding);
  const active = group.children.filter((child) => !child.ignored);
  const ignoredCount = group.children.length - active.length;
  const fixableCount = active.filter(
    (child) => child.finding.meta.hasFix
  ).length;
  return [
    `${group.label} group`,
    countLabel(group.children.length, "finding"),
    aggregateFindingProperty(
      findings.map((finding) => finding.meta.severity),
      "severity"
    ),
    aggregateFindingProperty(
      findings.map((finding) => finding.meta.confidence ?? "unknown"),
      "confidence"
    ),
    aggregateFindingProperty(
      findings.map((finding) => finding.meta.kind),
      "type"
    ),
    groupPathSummary(findings),
    countLabel(ignoredCount, "ignored finding"),
    countLabel(
      fixableCount,
      "active automatic fix",
      "active automatic fixes"
    ),
  ].join("; ");
}

function groupTooltip(group: SecurityOverviewGroupNode): string {
  return [
    group.label,
    group.description,
    securityGroupAccessibilityLabel(group),
  ].join("\n");
}

function aggregateFindingProperty(
  values: readonly string[],
  noun: string
): string {
  const counts = new Map<string, number>();
  for (const value of values) {
    counts.set(value, (counts.get(value) ?? 0) + 1);
  }
  const summary = Array.from(counts.entries())
    .map(([value, count]) =>
      countLabel(count, `${value} finding`, `${value} findings`)
    )
    .join(", ");
  return `${noun}: ${summary}`;
}

function groupPathSummary(findings: readonly SecurityFinding[]): string {
  const paths = Array.from(
    new Set(findings.map((finding) => findingPath(finding.uri)))
  );
  const visiblePaths = paths.slice(0, 3);
  const suffix =
    paths.length > visiblePaths.length
      ? `, and ${paths.length - visiblePaths.length} more`
      : "";
  return `locations ${visiblePaths.join(", ")}${suffix}`;
}

function findingLocation(finding: SecurityFinding): string {
  return `${findingPath(finding.uri)}:${finding.range.start.line + 1}:${
    finding.range.start.character + 1
  }`;
}

function findingPath(uri: string): string {
  try {
    return fileURLToPath(uri);
  } catch {
    return uri;
  }
}

function countLabel(
  count: number,
  singular: string,
  plural = `${singular}s`
): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

function titleCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
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
  const grouped = new Map<string, SecurityFinding[]>();
  let firstApplied:
    | { finding: SecurityFinding; directiveLine: number }
    | undefined;

  for (const finding of findings) {
    const key = `${finding.uri}:${finding.range.start.line}`;
    const group = grouped.get(key) ?? [];
    group.push(finding);
    grouped.set(key, group);
  }

  for (const sameLineFindings of grouped.values()) {
    const firstCandidate = sameLineFindings[0];
    if (!firstCandidate) {
      continue;
    }
    let uri: vscode.Uri;
    let document: vscode.TextDocument;
    try {
      uri = vscode.Uri.parse(firstCandidate.uri, true);
      document = await vscode.workspace.openTextDocument(uri);
    } catch {
      continue;
    }
    const currentFindings = sameLineFindings.filter((finding) =>
      isCurrentFinding(uri, finding)
    );
    const [first] = currentFindings;
    if (!first) {
      continue;
    }
    const targetLine = first.range.start.line;
    if (targetLine < 0 || targetLine >= document.lineCount) {
      continue;
    }
    const ruleIds = currentFindings.map((finding) => finding.meta.id);
    const directive = buildSuppressNextLineDirective(
      ruleIds,
      document.lineAt(targetLine).text
    );

    if (targetLine > 0) {
      const previousLine = document.lineAt(targetLine - 1);
      const existing = previousLine.text.match(
        /^\s*\/\/\s*solgrid-disable-next-line(?:\s+(.*?))?\s*$/u
      );
      if (existing) {
        const existingIds = (existing[1] ?? "")
          .split(",")
          .map((ruleId) => ruleId.trim())
          .filter(Boolean);
        if (existingIds.length === 0) {
          continue;
        }
        const combinedDirective = buildSuppressNextLineDirective(
          [...existingIds, ...ruleIds],
          document.lineAt(targetLine).text
        ).trimEnd();
        if (previousLine.text !== combinedDirective) {
          edit.replace(uri, previousLine.range, combinedDirective);
          firstApplied ??= { finding: first, directiveLine: targetLine - 1 };
        }
        continue;
      }
    }

    edit.insert(uri, new vscode.Position(targetLine, 0), directive);
    firstApplied ??= { finding: first, directiveLine: targetLine };
  }

  if (!firstApplied) {
    return;
  }

  const applied = await vscode.workspace.applyEdit(edit);
  if (!applied) {
    return;
  }

  if (revealFirstFinding) {
    try {
      await revealSuppressionDirective(
        firstApplied.finding,
        firstApplied.directiveLine
      );
    } catch {
      // The document can change again between applying and revealing the edit.
    }
  }
}

function isCurrentFinding(uri: vscode.Uri, finding: SecurityFinding): boolean {
  return vscode.languages.getDiagnostics(uri).some((diagnostic) => {
    const code =
      typeof diagnostic.code === "object" && diagnostic.code !== null
        ? diagnostic.code.value
        : diagnostic.code;
    return (
      diagnostic.source === finding.source &&
      String(code) === finding.code &&
      diagnostic.range.start.line === finding.range.start.line &&
      diagnostic.range.start.character === finding.range.start.character &&
      diagnostic.range.end.line === finding.range.end.line &&
      diagnostic.range.end.character === finding.range.end.character
    );
  });
}

async function revealSuppressionDirective(
  finding: SecurityFinding,
  directiveLineNumber: number
): Promise<void> {
  const uri = vscode.Uri.parse(finding.uri);
  const updatedDocument = await vscode.workspace.openTextDocument(uri);
  const editor = await vscode.window.showTextDocument(updatedDocument, {
    preview: false,
    preserveFocus: false,
  });
  if (
    directiveLineNumber < 0 ||
    directiveLineNumber >= updatedDocument.lineCount
  ) {
    return;
  }
  const directiveLine = updatedDocument.lineAt(directiveLineNumber).range;
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
  const resolution = await resolvePreferredFixForFinding(finding);
  const action = resolution.selected;
  if (!action) {
    return false;
  }

  return applyResolvedFixAction(finding, action, reveal);
}

async function resolvePreferredFixForFinding(
  finding: SecurityFinding
): Promise<{
  actions: Array<vscode.CodeAction | vscode.Command>;
  matchingActions: vscode.CodeAction[];
  selected: vscode.CodeAction | undefined;
}> {
  const uri = vscode.Uri.parse(finding.uri);
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

  const actions = securityCodeActionResolver
    ? await securityCodeActionResolver(finding)
    : ((await vscode.commands.executeCommand<
        Array<vscode.CodeAction | vscode.Command>
      >(
        "vscode.executeCodeActionProvider",
        uri,
        range,
        vscode.CodeActionKind.QuickFix.value
      )) ?? []);
  const codeActions = actions.filter(isCodeAction);
  const diagnosticMatch = pickPreferredCodeActionForFinding<vscode.CodeAction>(
    finding,
    codeActions
  );
  const matchingActions = diagnosticMatch
    ? codeActions.filter((action) =>
        action.diagnostics?.some((diagnostic) => {
          const diagnosticCode =
            typeof diagnostic.code === "object" && diagnostic.code !== null
              ? diagnostic.code.value
              : diagnostic.code;
          return (
            String(diagnosticCode) === finding.code &&
            diagnostic.range.isEqual(range)
          );
        })
      )
    : [];
  const selected = diagnosticMatch;

  return {
    actions,
    matchingActions,
    selected,
  };
}

async function applyResolvedFixAction(
  finding: SecurityFinding,
  action: vscode.CodeAction,
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
