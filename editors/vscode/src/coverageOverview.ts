import * as vscode from "vscode";
import {
  actionableDecorationPlan,
  buildCoverageTree,
  CoverageFileSummary,
  CoverageOverviewFileNode,
  CoverageOverviewFilterMode,
  CoverageOverviewLineNode,
  CoverageWorkspaceSummary,
  parseCoverageArtifact,
  summarizeCoverageArtifacts,
  summarizeCoverageOverview,
} from "./coverageOverviewModel";

export interface CoverageConfig {
  enable: boolean;
  artifacts: string[];
  autoRefreshAfterRun: boolean;
  customCommand: string[];
}

export type CoverageOverviewNode =
  | CoverageOverviewFileNode
  | CoverageOverviewLineNode;

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
    });
  private readonly partialDecorationType =
    vscode.window.createTextEditorDecorationType({
      isWholeLine: true,
      overviewRulerLane: vscode.OverviewRulerLane.Right,
      backgroundColor: new vscode.ThemeColor("editorWarning.background"),
      overviewRulerColor: new vscode.ThemeColor("editorWarning.foreground"),
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
  private refreshPromise: Promise<void> | undefined;
  private refreshQueued = false;

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
    this.config = {
      enable: config.enable,
      artifacts: Array.from(
        new Set(config.artifacts.map((pattern) => pattern.trim()).filter(Boolean))
      ),
      autoRefreshAfterRun: config.autoRefreshAfterRun,
      customCommand: Array.from(
        new Set(config.customCommand.map((part) => part.trim()).filter(Boolean))
      ),
    };
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
    if (!this.config.enable) {
      this.clearCoverage();
      return;
    }

    if (this.refreshPromise) {
      this.refreshQueued = true;
      await this.refreshPromise;
      return;
    }

    this.refreshPromise = this.performRefresh();
    try {
      await this.refreshPromise;
    } finally {
      this.refreshPromise = undefined;
      if (this.refreshQueued) {
        this.refreshQueued = false;
        await this.refresh();
      }
    }
  }

  getTreeItem(element: CoverageOverviewNode): vscode.TreeItem {
    if (element.kind === "file") {
      const item = new vscode.TreeItem(
        element.label,
        vscode.TreeItemCollapsibleState.Expanded
      );
      item.description = element.description;
      item.tooltip = fileTooltip(element.summary);
      item.iconPath = new vscode.ThemeIcon(
        element.summary.actionableLines.length > 0 ? "graph-line" : "pass"
      );
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
    item.tooltip = `${element.label}\n${element.description}`;
    item.iconPath = new vscode.ThemeIcon(
      element.detail.status === "uncovered" ? "error" : "warning"
    );
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
    for (const watcher of this.watchers) {
      watcher.dispose();
    }
    this.watchers = [];
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
  }

  private async performRefresh(): Promise<void> {
    const workspaceRoots =
      vscode.workspace.workspaceFolders?.map((folder) => folder.uri.fsPath) ?? [];
    if (workspaceRoots.length === 0 || this.config.artifacts.length === 0) {
      this.summary = undefined;
      this.refreshTree();
      return;
    }

    const artifactUris = await discoverCoverageArtifacts(this.config.artifacts);
    if (artifactUris.length === 0) {
      this.summary = undefined;
      this.refreshTree();
      return;
    }

    const decoder = new TextDecoder("utf-8");
    const records = [];
    for (const artifactUri of artifactUris) {
      try {
        const bytes = await vscode.workspace.fs.readFile(artifactUri);
        const content = decoder.decode(bytes);
        records.push(
          ...parseCoverageArtifact(content, artifactUri.fsPath, workspaceRoots)
        );
      } catch {
        // Ignore unreadable coverage artifacts and continue with what we can load.
      }
    }

    this.summary =
      records.length > 0
        ? summarizeCoverageArtifacts(records, workspaceRoots)
        : {
            artifactCount: artifactUris.length,
            files: [],
          };
    this.refreshTree();
  }

  private clearCoverage(): void {
    this.summary = undefined;
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
    this.view.message = summary.message;
    this.view.badge =
      summary.count > 0
        ? {
            value: summary.count,
            tooltip: `${summary.count} actionable coverage lines`,
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

    const plan = actionableDecorationPlan(summary);
    editor.setDecorations(
      this.uncoveredDecorationType,
      plan.uncoveredLines
        .map((line) => lineRange(editor.document, line))
        .filter((range): range is vscode.Range => range !== undefined)
    );
    editor.setDecorations(
      this.partialDecorationType,
      plan.partialLines
        .map((line) => lineRange(editor.document, line))
        .filter((range): range is vscode.Range => range !== undefined)
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
    const matches = await vscode.workspace.findFiles(pattern, COVERAGE_EXCLUDE_GLOB);
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

function fileTooltip(summary: CoverageFileSummary): string {
  const artifactList = summary.artifactPaths.map((artifact) => `- ${artifact}`).join("\n");
  const header = `${summary.displayPath}\n${summary.linesHit}/${summary.linesFound} lines covered`;
  return artifactList ? `${header}\nArtifacts:\n${artifactList}` : header;
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
