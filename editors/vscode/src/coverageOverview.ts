import * as vscode from "vscode";
import {
  buildCoverageTree,
  type CoverageFileSummary,
  type CoverageLineDetail,
  type CoverageOverviewFileNode,
  type CoverageOverviewFilterMode,
  type CoverageOverviewLineNode,
  type CoverageWorkspaceSummary,
  parseCoverageArtifact,
  shouldExpandCoverageFile,
  summarizeCoverageArtifacts,
  summarizeCoverageOverview,
} from "./coverageOverviewModel";
import { AsyncRefreshQueue } from "./asyncRefreshQueue";

export interface CoverageConfig {
  enable: boolean;
  artifacts: string[];
  autoRefreshAfterRun: boolean;
  customCommand: string[];
}

export type CoverageOverviewNode =
  | CoverageOverviewFileNode
  | CoverageOverviewLineNode;

export const COVERAGE_WAITING_MESSAGE = "Waiting to load coverage artifacts…";
export const COVERAGE_REFRESHING_MESSAGE = "Refreshing coverage artifacts…";

export function coverageOverviewPresentationMessage(
  summaryMessage: string | undefined,
  refreshInProgress: boolean,
  hasCompletedRefresh: boolean,
  refreshIssue?: string
): string | undefined {
  if (refreshInProgress) {
    return COVERAGE_REFRESHING_MESSAGE;
  }
  if (refreshIssue) {
    return refreshIssue;
  }
  return hasCompletedRefresh ? summaryMessage : COVERAGE_WAITING_MESSAGE;
}

export function coverageRefreshFailureMessage(
  error: unknown,
  hasStaleCoverage: boolean
): string {
  const suffix = hasStaleCoverage
    ? " Previously loaded coverage is still shown and may be stale."
    : " No new coverage data was applied.";
  return `Coverage refresh failed: ${errorDetail(error)}.${suffix}`;
}

export function normalizeCoverageConfig(config: CoverageConfig): CoverageConfig {
  return {
    enable: config.enable,
    artifacts: Array.from(
      new Set(config.artifacts.map((pattern) => pattern.trim()).filter(Boolean))
    ),
    autoRefreshAfterRun: config.autoRefreshAfterRun,
    // argv is ordered data. Repeated values can be meaningful to the command.
    customCommand: [...config.customCommand],
  };
}

const COVERAGE_EXCLUDE_GLOB = "{**/node_modules/**,**/target/**,**/.git/**,**/out/**}";

export class CoverageOverviewFeature
  implements vscode.TreeDataProvider<CoverageOverviewNode>, vscode.Disposable
{
  private readonly onDidChangeTreeDataEmitter =
    new vscode.EventEmitter<CoverageOverviewNode | undefined>();
  readonly onDidChangeTreeData = this.onDidChangeTreeDataEmitter.event;

  private readonly uncoveredDecorationType =
    vscode.window.createTextEditorDecorationType({
      isWholeLine: true,
      overviewRulerLane: vscode.OverviewRulerLane.Right,
      backgroundColor: new vscode.ThemeColor("editorError.background"),
      overviewRulerColor: new vscode.ThemeColor("editorError.foreground"),
      borderColor: new vscode.ThemeColor("editorError.foreground"),
      borderStyle: "solid",
      borderWidth: "0 0 0 3px",
      gutterIconPath: coverageGutterIcon("uncovered", "#888888"),
      gutterIconSize: "contain",
      light: {
        gutterIconPath: coverageGutterIcon("uncovered", "#a1260d"),
      },
      dark: {
        gutterIconPath: coverageGutterIcon("uncovered", "#f48771"),
      },
    });
  private readonly partialDecorationType =
    vscode.window.createTextEditorDecorationType({
      isWholeLine: true,
      overviewRulerLane: vscode.OverviewRulerLane.Right,
      backgroundColor: new vscode.ThemeColor("editorWarning.background"),
      overviewRulerColor: new vscode.ThemeColor("editorWarning.foreground"),
      borderColor: new vscode.ThemeColor("editorWarning.foreground"),
      borderStyle: "dashed",
      borderWidth: "0 0 0 3px",
      gutterIconPath: coverageGutterIcon("partial", "#888888"),
      gutterIconSize: "contain",
      light: {
        gutterIconPath: coverageGutterIcon("partial", "#7a6400"),
      },
      dark: {
        gutterIconPath: coverageGutterIcon("partial", "#cca700"),
      },
    });
  private readonly disposables: vscode.Disposable[] = [];

  private config: CoverageConfig = {
    enable: true,
    artifacts: [],
    autoRefreshAfterRun: true,
    customCommand: [],
  };
  private summary: CoverageWorkspaceSummary | undefined;
  private view: vscode.TreeView<CoverageOverviewNode> | undefined;
  private filterMode: CoverageOverviewFilterMode = "actionable";
  private watchers: vscode.FileSystemWatcher[] = [];
  private refreshGeneration = 0;
  private refreshInProgress = false;
  private hasCompletedRefresh = false;
  private refreshIssue: string | undefined;
  private disposed = false;
  private readonly refreshQueue = new AsyncRefreshQueue(() =>
    this.performRefresh(this.refreshGeneration)
  );

  constructor() {
    this.disposables.push(
      this.uncoveredDecorationType,
      this.partialDecorationType,
      vscode.window.onDidChangeVisibleTextEditors(() =>
        this.refreshVisibleEditorDecorations()
      ),
      vscode.window.onDidChangeActiveTextEditor(() =>
        this.refreshVisibleEditorDecorations()
      )
    );
  }

  attachView(view: vscode.TreeView<CoverageOverviewNode>): void {
    this.view = view;
    this.updatePresentation();
  }

  async applyConfig(config: CoverageConfig): Promise<void> {
    if (this.disposed) {
      return;
    }
    this.refreshGeneration += 1;
    this.refreshInProgress = false;
    this.hasCompletedRefresh = false;
    this.refreshIssue = undefined;
    this.config = normalizeCoverageConfig(config);
    this.rebuildWatchers();
    if (!this.config.enable) {
      this.clearCoverage();
      return;
    }
    await this.refresh();
  }

  setFilterMode(filterMode: CoverageOverviewFilterMode): void {
    if (this.filterMode === filterMode) {
      return;
    }
    this.filterMode = filterMode;
    this.refreshTree();
  }

  async refresh(): Promise<void> {
    if (this.disposed) {
      return;
    }
    if (!this.config.enable) {
      this.clearCoverage();
      return;
    }

    const generation = this.refreshGeneration;
    this.refreshInProgress = true;
    this.refreshIssue = undefined;
    this.updatePresentation();
    let completed = false;
    try {
      await this.refreshQueue.run();
      completed = true;
    } catch (error) {
      if (!this.disposed && generation === this.refreshGeneration) {
        this.refreshIssue = coverageRefreshFailureMessage(
          error,
          this.summary !== undefined
        );
      }
    } finally {
      if (!this.disposed && generation === this.refreshGeneration) {
        this.refreshInProgress = false;
        if (completed) {
          this.hasCompletedRefresh = true;
        }
        this.updatePresentation();
      }
    }
  }

  refreshTreeData(): void {
    if (this.disposed) {
      return;
    }
    this.updatePresentation();
    this.onDidChangeTreeDataEmitter.fire(undefined);
  }

  getTreeItem(element: CoverageOverviewNode): vscode.TreeItem {
    if (element.kind === "file") {
      const item = new vscode.TreeItem(
        element.label,
        shouldExpandCoverageFile(element.children.length)
          ? vscode.TreeItemCollapsibleState.Expanded
          : vscode.TreeItemCollapsibleState.Collapsed
      );
      item.description = element.description;
      item.tooltip = fileTooltip(element.summary);
      item.iconPath = new vscode.ThemeIcon(
        element.summary.actionableLines.length > 0 ? "graph-line" : "pass"
      );
      item.accessibilityInformation = {
        label: coverageFileAccessibilityLabel(element.summary),
      };
      item.command = {
        command: "solgrid.coverage.openNode",
        title: "Open Coverage File",
        arguments: [element],
      };
      return item;
    }

    const item = new vscode.TreeItem(
      element.label,
      vscode.TreeItemCollapsibleState.None
    );
    item.description = element.description;
    item.tooltip = coverageLineTooltip(element.filePath, element.detail);
    item.iconPath = new vscode.ThemeIcon(
      element.detail.status === "uncovered" ? "error" : "warning"
    );
    item.accessibilityInformation = {
      label: coverageLineAccessibilityLabel(element.filePath, element.detail),
    };
    item.command = {
      command: "solgrid.coverage.openNode",
      title: "Open Coverage Line",
      arguments: [element],
    };
    return item;
  }

  getChildren(
    element?: CoverageOverviewNode
  ): vscode.ProviderResult<CoverageOverviewNode[]> {
    if (!element) {
      return buildCoverageTree(this.summary, this.filterMode);
    }
    if (element.kind === "file") {
      return element.children;
    }
    return [];
  }

  async openNode(node?: CoverageOverviewNode): Promise<void> {
    if (!node) {
      return;
    }

    const filePath = node.kind === "file" ? node.summary.filePath : node.filePath;
    const targetLine =
      node.kind === "file"
        ? node.summary.actionableLines[0]?.line
        : node.detail.line;
    const document = await vscode.workspace.openTextDocument(
      vscode.Uri.file(filePath)
    );
    const editor = await vscode.window.showTextDocument(document, {
      preview: false,
      preserveFocus: false,
    });
    if (!targetLine || targetLine < 1 || targetLine > document.lineCount) {
      return;
    }
    const line = document.lineAt(targetLine - 1);
    editor.selection = new vscode.Selection(line.range.start, line.range.end);
    editor.revealRange(line.range, vscode.TextEditorRevealType.InCenter);
  }

  dispose(): void {
    if (this.disposed) {
      return;
    }
    this.disposed = true;
    this.refreshGeneration += 1;
    for (const watcher of this.watchers) {
      watcher.dispose();
    }
    this.watchers = [];
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
  }

  private async performRefresh(generation: number): Promise<void> {
    const workspaceRoots =
      vscode.workspace.workspaceFolders?.map((folder) => folder.uri.fsPath) ?? [];
    if (workspaceRoots.length === 0 || this.config.artifacts.length === 0) {
      this.applyRefreshResult(generation, undefined);
      return;
    }

    const artifactUris = await discoverCoverageArtifacts(this.config.artifacts);
    if (artifactUris.length === 0) {
      this.applyRefreshResult(generation, undefined);
      return;
    }

    const decoder = new TextDecoder("utf-8");
    const records = [];
    const artifactFailures: string[] = [];
    for (const artifactUri of artifactUris) {
      try {
        const bytes = await vscode.workspace.fs.readFile(artifactUri);
        const content = decoder.decode(bytes);
        records.push(
          ...parseCoverageArtifact(content, artifactUri.fsPath, workspaceRoots)
        );
      } catch (error) {
        artifactFailures.push(
          `${artifactUri.fsPath} (${errorDetail(error)})`
        );
      }
    }

    if (artifactFailures.length === artifactUris.length) {
      throw new Error(coverageArtifactFailureDetail(artifactFailures, false));
    }

    const summary =
      records.length > 0
        ? summarizeCoverageArtifacts(records, workspaceRoots)
        : {
            artifactCount: artifactUris.length - artifactFailures.length,
            files: [],
          };
    this.applyRefreshResult(
      generation,
      summary,
      artifactFailures.length > 0
        ? coverageArtifactFailureDetail(artifactFailures, true)
        : undefined
    );
  }

  private applyRefreshResult(
    generation: number,
    summary: CoverageWorkspaceSummary | undefined,
    refreshIssue?: string
  ): void {
    if (
      this.disposed ||
      generation !== this.refreshGeneration ||
      !this.config.enable
    ) {
      return;
    }
    this.summary = summary;
    this.refreshIssue = refreshIssue;
    this.refreshTree();
  }

  private clearCoverage(): void {
    this.summary = undefined;
    this.refreshIssue = undefined;
    this.refreshTree();
  }

  private refreshTree(): void {
    this.updatePresentation();
    this.refreshVisibleEditorDecorations();
    this.onDidChangeTreeDataEmitter.fire(undefined);
  }

  private updatePresentation(): void {
    if (!this.view) {
      return;
    }
    const summary = summarizeCoverageOverview(this.summary, this.filterMode);
    this.view.description = summary.description;
    this.view.message = coverageOverviewPresentationMessage(
      summary.message,
      this.refreshInProgress,
      this.hasCompletedRefresh,
      this.refreshIssue
    );
    this.view.badge =
      summary.count > 0
        ? {
            value: summary.count,
            tooltip: `${summary.count} actionable coverage ${
              summary.count === 1 ? "line" : "lines"
            }`,
          }
        : undefined;
  }

  private refreshVisibleEditorDecorations(): void {
    const coverageByFile = new Map(
      (this.summary?.files ?? []).map((file) => [normalizePath(file.filePath), file])
    );
    for (const editor of vscode.window.visibleTextEditors) {
      if (editor.document.languageId !== "solidity" || editor.document.uri.scheme !== "file") {
        continue;
      }
      const summary = coverageByFile.get(normalizePath(editor.document.uri.fsPath));
      this.applyDecorations(editor, summary);
    }
  }

  private applyDecorations(
    editor: vscode.TextEditor,
    summary: CoverageFileSummary | undefined
  ): void {
    if (!summary) {
      editor.setDecorations(this.uncoveredDecorationType, []);
      editor.setDecorations(this.partialDecorationType, []);
      return;
    }

    const uncovered = summary.actionableLines.filter(
      (detail) => detail.status === "uncovered"
    );
    const partial = summary.actionableLines.filter(
      (detail) => detail.status === "partial"
    );
    editor.setDecorations(
      this.uncoveredDecorationType,
      uncovered
        .map((detail) => coverageDecoration(editor.document, detail))
        .filter(
          (option): option is vscode.DecorationOptions => option !== undefined
        )
    );
    editor.setDecorations(
      this.partialDecorationType,
      partial
        .map((detail) => coverageDecoration(editor.document, detail))
        .filter(
          (option): option is vscode.DecorationOptions => option !== undefined
        )
    );
  }

  private rebuildWatchers(): void {
    for (const watcher of this.watchers) {
      watcher.dispose();
    }
    this.watchers = [];

    if (!this.config.enable) {
      return;
    }

    for (const pattern of this.config.artifacts) {
      const watcher = vscode.workspace.createFileSystemWatcher(pattern);
      watcher.onDidCreate(() => void this.refresh());
      watcher.onDidChange(() => void this.refresh());
      watcher.onDidDelete(() => void this.refresh());
      this.watchers.push(watcher);
    }
  }
}

async function discoverCoverageArtifacts(
  patterns: readonly string[]
): Promise<vscode.Uri[]> {
  const artifacts = new Map<string, vscode.Uri>();
  for (const pattern of patterns) {
    let matches: vscode.Uri[];
    try {
      matches = await vscode.workspace.findFiles(
        pattern,
        COVERAGE_EXCLUDE_GLOB
      );
    } catch (error) {
      throw new Error(
        `Could not search for coverage artifacts matching "${pattern}": ${errorDetail(error)}`
      );
    }
    for (const uri of matches) {
      if (uri.scheme === "file") {
        artifacts.set(normalizePath(uri.fsPath), uri);
      }
    }
  }
  return Array.from(artifacts.values()).sort((left, right) =>
    left.fsPath.localeCompare(right.fsPath)
  );
}

function coverageArtifactFailureDetail(
  failures: readonly string[],
  partial: boolean
): string {
  const count = failures.length;
  const shown = failures.slice(0, 3).join("; ");
  const omitted = count > 3 ? `; and ${count - 3} more` : "";
  const noun = count === 1 ? "artifact" : "artifacts";
  return partial
    ? `Coverage results may be incomplete: could not read or parse ${count} ${noun}: ${shown}${omitted}.`
    : `Could not read or parse ${count} coverage ${noun}: ${shown}${omitted}`;
}

function errorDetail(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message.trim().replace(/[.!]+$/, "");
  }
  const detail = String(error).trim().replace(/[.!]+$/, "");
  return detail || "unknown error";
}

function fileTooltip(summary: CoverageFileSummary): string {
  const artifactList = summary.artifactPaths
    .map((artifact) => `- ${artifact}`)
    .join("\n");
  const header = [
    summary.displayPath,
    `Source: ${summary.filePath}`,
    `${summary.linesHit}/${summary.linesFound} ${
      summary.linesFound === 1 ? "line" : "lines"
    } covered`,
    `${summary.branchesHit}/${summary.branchesFound} ${
      summary.branchesFound === 1 ? "branch" : "branches"
    } covered`,
    `${summary.actionableLines.length} actionable ${
      summary.actionableLines.length === 1 ? "line" : "lines"
    }`,
  ].join("\n");
  return artifactList ? `${header}\nArtifacts:\n${artifactList}` : header;
}

export function coverageFileAccessibilityLabel(
  summary: CoverageFileSummary
): string {
  return [
    summary.displayPath,
    `source ${summary.filePath}`,
    `${summary.linesHit} of ${summary.linesFound} ${
      summary.linesFound === 1 ? "line" : "lines"
    } covered`,
    `${summary.branchesHit} of ${summary.branchesFound} ${
      summary.branchesFound === 1 ? "branch" : "branches"
    } covered`,
    `${summary.actionableLines.length} actionable ${
      summary.actionableLines.length === 1 ? "line" : "lines"
    }`,
  ].join("; ");
}

export function coverageLineAccessibilityLabel(
  filePath: string,
  detail: CoverageLineDetail
): string {
  return [
    `${detail.status} coverage`,
    `line ${detail.line}`,
    `source ${filePath}`,
    `${detail.hits} ${detail.hits === 1 ? "hit" : "hits"}`,
    `${detail.branchesHit} of ${detail.branchesFound} ${
      detail.branchesFound === 1 ? "branch" : "branches"
    } covered`,
  ].join("; ");
}

export function coverageLineHoverMessage(detail: CoverageLineDetail): string {
  if (detail.status === "uncovered") {
    const branchDetail =
      detail.branchesFound > 0
        ? `; ${detail.branchesHit} of ${detail.branchesFound} ${
            detail.branchesFound === 1 ? "branch" : "branches"
          } covered`
        : "";
    return `Coverage: line ${detail.line} is uncovered (0 hits${branchDetail}).`;
  }
  return `Coverage: line ${detail.line} is partially covered (${detail.hits} ${
    detail.hits === 1 ? "hit" : "hits"
  }; ${detail.branchesHit} of ${detail.branchesFound} ${
    detail.branchesFound === 1 ? "branch" : "branches"
  } covered).`;
}

function coverageLineTooltip(
  filePath: string,
  detail: CoverageLineDetail
): string {
  return `${coverageLineHoverMessage(detail)}\nSource: ${filePath}`;
}

function coverageDecoration(
  document: vscode.TextDocument,
  detail: CoverageLineDetail
): vscode.DecorationOptions | undefined {
  const range = lineRange(document, detail.line);
  return range
    ? {
        range,
        hoverMessage: coverageLineHoverMessage(detail),
      }
    : undefined;
}

function coverageGutterIcon(
  status: "uncovered" | "partial",
  color: string
): vscode.Uri {
  const mark =
    status === "uncovered"
      ? `<path d="M3 3l10 10M13 3L3 13" fill="none" stroke="${color}" stroke-linecap="round" stroke-width="2.5"/>`
      : [
          `<path d="M2 2h12v12H2z" fill="none" stroke="${color}" stroke-width="2"/>`,
          `<path d="M2 14L14 2v12z" fill="${color}"/>`,
        ].join("");
  const svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"',
    ` viewBox="0 0 16 16">${mark}</svg>`,
  ].join("");
  return vscode.Uri.parse(`data:image/svg+xml,${encodeURIComponent(svg)}`);
}

function lineRange(
  document: vscode.TextDocument,
  lineNumber: number
): vscode.Range | undefined {
  if (lineNumber < 1 || lineNumber > document.lineCount) {
    return undefined;
  }
  return document.lineAt(lineNumber - 1).range;
}

function normalizePath(filePath: string): string {
  return vscode.Uri.file(filePath).fsPath;
}
