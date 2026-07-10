import {
  commands,
  ExtensionContext,
  languages,
  Location,
  Position,
  Range,
  StatusBarAlignment,
  StatusBarItem,
  Uri,
  window,
  workspace,
} from "vscode";
import {
  type CodeAction as ProtocolCodeAction,
  type CodeActionParams,
  type Command as ProtocolCommand,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";
import {
  CoverageExtensionConfig,
  DEFAULT_COVERAGE_CONFIG,
  EditorSaveConfig,
  SolgridConfig,
  getServerPath,
  getInitializationOptions,
  getSettings,
} from "./config";
import {
  CoverageOverviewFeature,
  CoverageOverviewNode,
} from "./coverageOverview";
import { runCoverageCommand, runPreferredCoverageCommand } from "./coverageRun";
import {
  applyGroupFixes,
  applyFindingFix,
  applyFindingFixForTests,
  openFindingHelp,
  openSecurityFinding,
  previewFindingFix,
  SecurityOverviewNode,
  SecurityOverviewProvider,
  setSecurityCodeActionResolver,
  suppressGroupNextLine,
  suppressFindingNextLine,
} from "./securityOverview";
import { SecurityOverviewFindingNode } from "./securityOverviewModel";
import {
  activeImportsGraphArgs,
  getGraphPreviewSnapshot,
  showGraph,
} from "./graphPreview";

let client: LanguageClient | undefined;

interface ReferenceLensArgs {
  locations?: ReferenceLocationArg[];
  position?: {
    character: number;
    line: number;
  };
  uri?: string;
}

interface ReferenceLocationArg {
  range: {
    end: {
      character: number;
      line: number;
    };
    start: {
      character: number;
      line: number;
    };
  };
  uri: string;
}

interface ProjectIndexStatus {
  durationMs?: number | null;
  files?: number;
  state?: string;
}

export async function activate(context: ExtensionContext): Promise<void> {
  const solgridConfig = readVSCodeConfig();
  let coverageConfig = readCoverageConfig();
  const editorSaveConfig = readEditorSaveConfig();

  const coverageOverview = new CoverageOverviewFeature();
  const coverageOverviewView = window.createTreeView<CoverageOverviewNode>(
    "solgridCoverageOverview",
    {
      treeDataProvider: coverageOverview,
      showCollapseAll: true,
    }
  );
  coverageOverview.attachView(coverageOverviewView);
  applyCoverageConfig(coverageOverview, coverageConfig);
  context.subscriptions.push(
    coverageOverview,
    coverageOverviewView,
    commands.registerCommand("solgrid.coverage.refresh", () =>
      coverageOverview.refresh()
    ),
    commands.registerCommand("solgrid.coverage.run", () =>
      runPreferredCoverageCommand(coverageConfig, () =>
        coverageOverview.refresh()
      )
    ),
    commands.registerCommand("solgrid.coverage.runFoundryLcov", () =>
      runCoverageCommand("foundry-lcov", coverageConfig, () =>
        coverageOverview.refresh()
      )
    ),
    commands.registerCommand("solgrid.coverage.runHardhatLcov", () =>
      runCoverageCommand("hardhat-lcov", coverageConfig, () =>
        coverageOverview.refresh()
      )
    ),
    commands.registerCommand("solgrid.coverage.runCustom", () =>
      runCoverageCommand("custom", coverageConfig, () =>
        coverageOverview.refresh()
      )
    ),
    commands.registerCommand("solgrid.coverage.showActionable", () =>
      coverageOverview.setFilterMode("actionable")
    ),
    commands.registerCommand("solgrid.coverage.showAll", () =>
      coverageOverview.setFilterMode("all")
    ),
    commands.registerCommand("solgrid.coverage.openNode", (node) =>
      coverageOverview.openNode(node)
    ),
    commands.registerCommand("_solgrid.test.getCoverageOverviewSnapshot", async () =>
      snapshotCoverageOverview(coverageOverview)
    ),
    commands.registerCommand(
      "_solgrid.test.findCoverageOverviewNode",
      async (criteria) => findCoverageOverviewNode(coverageOverview, criteria)
    ),
    workspace.onDidChangeConfiguration((event) => {
      if (!event.affectsConfiguration("solgrid.coverage")) {
        return;
      }
      coverageConfig = readCoverageConfig();
      applyCoverageConfig(coverageOverview, coverageConfig);
    })
  );

  const activatedWithLanguageServer = solgridConfig.enable;
  await commands.executeCommand(
    "setContext",
    "solgrid.languageServerActive",
    activatedWithLanguageServer
  );
  let enableReloadPromptOpen = false;
  context.subscriptions.push(
    workspace.onDidChangeConfiguration(async (event) => {
      if (
        !event.affectsConfiguration("solgrid.enable") ||
        readVSCodeConfig().enable === activatedWithLanguageServer ||
        enableReloadPromptOpen
      ) {
        return;
      }
      enableReloadPromptOpen = true;
      try {
        const action = await window.showInformationMessage(
          activatedWithLanguageServer
            ? "Reload VS Code to stop the solgrid language server."
            : "Reload VS Code to start the solgrid language server.",
          "Reload Window"
        );
        if (action === "Reload Window") {
          await commands.executeCommand("workbench.action.reloadWindow");
        }
      } finally {
        enableReloadPromptOpen = false;
      }
    })
  );

  if (!solgridConfig.enable) {
    return;
  }

  const serverPath = getServerPath(solgridConfig, context.extensionPath);

  // Note: don't set `transport: TransportKind.stdio` — it adds `--stdio` to
  // the command args, which solgrid doesn't accept. Omitting transport defaults
  // to stdio communication without adding extra flags.
  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ["server"],
  };

  const fileWatchers = [
    workspace.createFileSystemWatcher("**/*.sol"),
    workspace.createFileSystemWatcher("**/solgrid.toml"),
    workspace.createFileSystemWatcher("**/foundry.toml"),
    workspace.createFileSystemWatcher("**/remappings.txt"),
  ];
  context.subscriptions.push(...fileWatchers);

  const securityOverview = new SecurityOverviewProvider(context.workspaceState);
  const securityOverviewView = window.createTreeView<SecurityOverviewNode>(
    "solgridSecurityOverview",
    {
      treeDataProvider: securityOverview,
      showCollapseAll: true,
    }
  );
  securityOverview.attachView(securityOverviewView);
  context.subscriptions.push(securityOverviewView);

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "solidity" }],
    synchronize: {
      configurationSection: "solgrid",
      fileEvents: fileWatchers,
    },
    initializationOptions: getInitializationOptions(solgridConfig, editorSaveConfig),
    middleware: {
      provideDocumentFormattingEdits: (document, options, token, next) =>
        next(
          document,
          options ?? fallbackFormattingOptions(document.uri),
          token
        ),
      provideDocumentRangeFormattingEdits: (
        document,
        range,
        options,
        token,
        next
      ) =>
        next(
          document,
          range,
          options ?? fallbackFormattingOptions(document.uri),
          token
        ),
      workspace: {
        configuration: async (params, token, next) => {
          const result = await next(params, token);
          return result;
        },
      },
    },
  };

  client = new LanguageClient(
    "solgrid",
    "solgrid Language Server",
    serverOptions,
    clientOptions
  );

  setSecurityCodeActionResolver(async (finding) => {
    if (!client) {
      return [];
    }
    const uri = Uri.parse(finding.uri);
    const range = new Range(
      new Position(finding.range.start.line, finding.range.start.character),
      new Position(finding.range.end.line, finding.range.end.character)
    );
    const diagnostic = languages.getDiagnostics(uri).find((candidate) => {
      const code =
        typeof candidate.code === "object" && candidate.code !== null
          ? candidate.code.value
          : candidate.code;
      return (
        candidate.source === finding.source &&
        String(code) === finding.code &&
        candidate.range.isEqual(range)
      );
    });
    if (!diagnostic) {
      return [];
    }

    const params: CodeActionParams = {
      textDocument: { uri: finding.uri },
      range: client.code2ProtocolConverter.asRange(range),
      context: {
        diagnostics: [client.code2ProtocolConverter.asDiagnostic(diagnostic)],
        only: ["quickfix"],
        triggerKind: 1,
      },
    };
    try {
      const protocolActions = await client.sendRequest<
        Array<ProtocolCodeAction | ProtocolCommand> | null
      >(
        "textDocument/codeAction",
        params
      );
      return protocolActions
        ? await client.protocol2CodeConverter.asCodeActionResult(protocolActions)
        : [];
    } catch {
      return [];
    }
  });

  client.outputChannel.appendLine(`Using solgrid binary: ${serverPath}`);

  const projectIndexStatus = window.createStatusBarItem(
    StatusBarAlignment.Left,
    100
  );
  projectIndexStatus.name = "solgrid workspace index";
  projectIndexStatus.text = "$(sync~spin) solgrid: starting";
  projectIndexStatus.tooltip = "Starting the solgrid language server.";
  projectIndexStatus.show();
  context.subscriptions.push(projectIndexStatus);

  client.onNotification("solgrid/projectIndexStatus", (status: ProjectIndexStatus) => {
    updateProjectIndexStatus(projectIndexStatus, status);
  });

  context.subscriptions.push(
    languages.onDidChangeDiagnostics((event) => {
      for (const uri of event.uris) {
        securityOverview.updateFromDiagnostics({
          uri: uri.toString(),
          diagnostics: languages.getDiagnostics(uri).map((diagnostic) => ({
            range: diagnostic.range,
            severity: diagnostic.severity,
            code:
              typeof diagnostic.code === "object" &&
              diagnostic.code !== null &&
              "value" in diagnostic.code
                ? diagnostic.code.value
                : diagnostic.code,
            source: diagnostic.source,
            message: diagnostic.message,
            data: (diagnostic as typeof diagnostic & { data?: unknown }).data,
          })),
        });
      }
    }),
    commands.registerCommand("solgrid.securityOverview.refresh", async () => {
      await rerunSecurityAnalysis(securityOverview);
    }),
    commands.registerCommand("solgrid.securityOverview.groupByFile", () =>
      securityOverview.setGroupMode("file")
    ),
    commands.registerCommand("solgrid.securityOverview.groupBySeverity", () =>
      securityOverview.setGroupMode("severity")
    ),
    commands.registerCommand("solgrid.securityOverview.groupByConfidence", () =>
      securityOverview.setGroupMode("confidence")
    ),
    commands.registerCommand("solgrid.securityOverview.groupByFinding", () =>
      securityOverview.setGroupMode("finding")
    ),
    commands.registerCommand("solgrid.securityOverview.showSecurity", () =>
      securityOverview.setFilterMode("security")
    ),
    commands.registerCommand("solgrid.securityOverview.showAll", () =>
      securityOverview.setFilterMode("all")
    ),
    commands.registerCommand("solgrid.securityOverview.showCompiler", () =>
      securityOverview.setFilterMode("compiler")
    ),
    commands.registerCommand("solgrid.securityOverview.showDetectors", () =>
      securityOverview.setFilterMode("detector")
    ),
    commands.registerCommand("solgrid.securityOverview.openFinding", openSecurityFinding),
    commands.registerCommand("solgrid.securityOverview.openHelp", openFindingHelp),
    commands.registerCommand("solgrid.securityOverview.applyFix", applyFindingFix),
    commands.registerCommand("solgrid.securityOverview.applyGroupFixes", applyGroupFixes),
    commands.registerCommand("solgrid.graph.show", (args) => showGraph(client, args)),
    commands.registerCommand("solgrid.showReferences", showReferences),
    commands.registerCommand("solgrid.graph.showImports", async () => {
      const args = activeImportsGraphArgs();
      if (args) {
        await showGraph(client, args);
      }
    }),
    commands.registerCommand("_solgrid.test.getGraphPreviewSnapshot", () =>
      getGraphPreviewSnapshot()
    ),
    commands.registerCommand("_solgrid.test.getSecurityOverviewSnapshot", async () =>
      snapshotSecurityOverview(securityOverview)
    ),
    commands.registerCommand(
      "_solgrid.test.findSecurityOverviewFinding",
      async (criteria) => findSecurityOverviewFinding(securityOverview, criteria)
    ),
    commands.registerCommand("_solgrid.test.resetSecurityOverviewState", () =>
      securityOverview.resetForTests()
    ),
    commands.registerCommand("_solgrid.test.getSecurityOverviewDebugState", () =>
      securityOverview.debugStateForTests()
    ),
    commands.registerCommand(
      "_solgrid.test.ignoreSecurityOverviewGroup",
      async (criteria) =>
        ignoreSecurityOverviewGroup(securityOverview, criteria)
    ),
    commands.registerCommand(
      "_solgrid.test.restoreSecurityOverviewGroup",
      async (criteria) =>
        restoreSecurityOverviewGroup(securityOverview, criteria)
    ),
    commands.registerCommand(
      "_solgrid.test.applySecurityOverviewGroupFixes",
      async (criteria) =>
        applySecurityOverviewGroupFixes(securityOverview, criteria)
    ),
    commands.registerCommand(
      "_solgrid.test.suppressSecurityOverviewGroupNextLine",
      async (criteria) =>
        suppressSecurityOverviewGroupNextLine(securityOverview, criteria)
    ),
    commands.registerCommand(
      "_solgrid.test.ignoreSecurityOverviewFinding",
      async (criteria) =>
        ignoreSecurityOverviewFinding(securityOverview, criteria)
    ),
    commands.registerCommand(
      "_solgrid.test.restoreSecurityOverviewFinding",
      async (criteria) =>
        restoreSecurityOverviewFinding(securityOverview, criteria)
    ),
    commands.registerCommand(
      "_solgrid.test.previewSecurityOverviewFix",
      previewFindingFix
    ),
    commands.registerCommand(
      "_solgrid.test.applySecurityOverviewFix",
      applyFindingFixForTests
    ),
    commands.registerCommand("solgrid.securityOverview.ignoreFinding", (node) =>
      securityOverview.ignoreFinding(node)
    ),
    commands.registerCommand("solgrid.securityOverview.restoreFinding", (node) =>
      securityOverview.restoreFinding(node)
    ),
    commands.registerCommand("solgrid.securityOverview.ignoreGroup", (node) =>
      securityOverview.ignoreGroup(node)
    ),
    commands.registerCommand("solgrid.securityOverview.restoreGroup", (node) =>
      securityOverview.restoreGroup(node)
    ),
    commands.registerCommand("solgrid.securityOverview.toggleShowIgnored", () =>
      securityOverview.toggleShowIgnoredBaselines()
    ),
    commands.registerCommand("solgrid.securityOverview.clearIgnoredBaselines", () =>
      securityOverview.clearIgnoredBaselines()
    ),
    commands.registerCommand(
      "solgrid.securityOverview.suppressNextLine",
      suppressFindingNextLine
    ),
    commands.registerCommand(
      "solgrid.securityOverview.suppressGroupNextLine",
      suppressGroupNextLine
    )
  );

  // Watch for configuration changes
  context.subscriptions.push(
    workspace.onDidChangeConfiguration((e) => {
      if (
        e.affectsConfiguration("solgrid") ||
        e.affectsConfiguration("editor.formatOnSave") ||
        e.affectsConfiguration("[solidity]")
      ) {
        const newConfig = readVSCodeConfig();
        const newEditorSaveConfig = readEditorSaveConfig();
        client?.sendNotification("workspace/didChangeConfiguration", {
          settings: getSettings(newConfig, newEditorSaveConfig),
        });
      }
    })
  );

  try {
    await client.start();
  } catch (error) {
    console.error(`[solgrid] Failed to start language server at "${serverPath}":`, error);
    window.showErrorMessage(
      `Failed to start solgrid language server: ${error}`
    );
  }
}

export async function deactivate(): Promise<void> {
  setSecurityCodeActionResolver(undefined);
  await commands.executeCommand(
    "setContext",
    "solgrid.languageServerActive",
    false
  );
  if (client) {
    await client.stop();
    client = undefined;
  }
}

async function showReferences(args?: ReferenceLensArgs): Promise<void> {
  const activeEditor = window.activeTextEditor;
  const uri = typeof args?.uri === "string" ? Uri.parse(args.uri) : activeEditor?.document.uri;
  const position =
    args?.position &&
    Number.isInteger(args.position.line) &&
    Number.isInteger(args.position.character)
      ? new Position(args.position.line, args.position.character)
      : activeEditor?.selection.active;

  if (!uri || !position) {
    void window.showWarningMessage(
      "Open a Solidity file before requesting solgrid references."
    );
    return;
  }

  const locations =
    referenceLocationsFromArgs(args?.locations) ??
    ((await commands.executeCommand<Location[]>(
      "vscode.executeReferenceProvider",
      uri,
      position
    )) ??
      []);

  if (locations.length === 0) {
    void window.showInformationMessage("solgrid found no references for this symbol.");
    return;
  }

  await commands.executeCommand("editor.action.showReferences", uri, position, locations);
}

function referenceLocationsFromArgs(
  locations: ReferenceLocationArg[] | undefined
): Location[] | undefined {
  if (!Array.isArray(locations)) {
    return undefined;
  }

  return locations.flatMap((location) => {
    if (
      typeof location?.uri !== "string" ||
      !Number.isInteger(location.range?.start?.line) ||
      !Number.isInteger(location.range?.start?.character) ||
      !Number.isInteger(location.range?.end?.line) ||
      !Number.isInteger(location.range?.end?.character)
    ) {
      return [];
    }

    return [
      new Location(
        Uri.parse(location.uri),
        new Range(
          new Position(location.range.start.line, location.range.start.character),
          new Position(location.range.end.line, location.range.end.character)
        )
      ),
    ];
  });
}

function updateProjectIndexStatus(
  statusBar: StatusBarItem,
  status: ProjectIndexStatus
): void {
  const files = Number.isInteger(status.files) ? status.files ?? 0 : 0;
  const fileLabel = `${files} Solidity file${files === 1 ? "" : "s"}`;
  const duration =
    typeof status.durationMs === "number" && Number.isFinite(status.durationMs)
      ? ` in ${formatDuration(status.durationMs)}`
      : "";

  if (status.state === "building") {
    statusBar.text = "$(sync~spin) solgrid: indexing";
    statusBar.tooltip =
      files > 0
        ? `Rebuilding the solgrid workspace index from ${fileLabel}.`
        : "Building the solgrid workspace index.";
    statusBar.show();
    return;
  }

  statusBar.text = "$(check) solgrid";
  statusBar.tooltip =
    files > 0
      ? `Workspace index ready for ${fileLabel}${duration}.`
      : "Workspace index ready.";
  statusBar.show();
}

function formatDuration(milliseconds: number): string {
  if (milliseconds < 1000) {
    return `${Math.max(0, Math.round(milliseconds))}ms`;
  }

  return `${(milliseconds / 1000).toFixed(1)}s`;
}

/**
 * Read solgrid settings from the VSCode configuration API.
 */
function readVSCodeConfig(): SolgridConfig {
  const config = workspace.getConfiguration("solgrid");
  const unsafeFixes = config.inspect<boolean>("unsafeFixesOnSave");
  const explicitlyConfiguredUnsafeFixes =
    unsafeFixes?.workspaceFolderValue ??
    unsafeFixes?.workspaceValue ??
    unsafeFixes?.globalValue;
  return {
    enable: config.get<boolean>("enable", true),
    path: config.get<string | null>("path", null),
    fixOnSave: config.get<boolean>("fixOnSave", true),
    fixOnSaveUnsafe:
      explicitlyConfiguredUnsafeFixes ??
      config.get<boolean>("fixOnSave.unsafeFixes", false),
    formatOnSave: config.get<boolean>("formatOnSave", true),
    configPath: config.get<string | null>("configPath", null),
  };
}

function readCoverageConfig(): CoverageExtensionConfig {
  const config = workspace.getConfiguration("solgrid");
  return {
    enable: config.get<boolean>(
      "coverage.enable",
      DEFAULT_COVERAGE_CONFIG.enable
    ),
    artifacts: config.get<string[]>(
      "coverage.artifacts",
      DEFAULT_COVERAGE_CONFIG.artifacts
    ),
    autoRefreshAfterRun: config.get<boolean>(
      "coverage.autoRefreshAfterRun",
      DEFAULT_COVERAGE_CONFIG.autoRefreshAfterRun
    ),
    customCommand: config.get<string[]>(
      "coverage.customCommand",
      DEFAULT_COVERAGE_CONFIG.customCommand
    ),
  };
}

function applyCoverageConfig(
  coverageOverview: CoverageOverviewFeature,
  coverageConfig: CoverageExtensionConfig
): void {
  void coverageOverview.applyConfig(coverageConfig).catch((error) => {
    console.error("[solgrid] Failed to load coverage artifacts:", error);
  });
}

function readEditorSaveConfig(): EditorSaveConfig {
  const editorConfig = workspace.getConfiguration("editor");
  const solidityOverrides =
    workspace.getConfiguration().get<Record<string, unknown>>("[solidity]") ?? {};

  const defaultFormatter =
    typeof solidityOverrides["editor.defaultFormatter"] === "string"
      ? (solidityOverrides["editor.defaultFormatter"] as string)
      : editorConfig.get<string | null>("defaultFormatter", null);

  const formatOnSave =
    typeof solidityOverrides["editor.formatOnSave"] === "boolean"
      ? (solidityOverrides["editor.formatOnSave"] as boolean)
      : editorConfig.get<boolean>("formatOnSave", false);

  return {
    formatOnSave,
    defaultFormatter,
  };
}

function fallbackFormattingOptions(uri: Uri): {
  tabSize: number;
  insertSpaces: boolean;
} {
  const editorConfig = workspace.getConfiguration("editor", uri);
  return {
    tabSize: editorConfig.get<number>("tabSize", 4),
    insertSpaces: editorConfig.get<boolean>("insertSpaces", true),
  };
}

async function rerunSecurityAnalysis(
  securityOverview: SecurityOverviewProvider
): Promise<void> {
  if (!client) {
    securityOverview.refresh();
    return;
  }

  try {
    await client.sendRequest("workspace/executeCommand", {
      command: "solgrid.workspace.rerunSecurityAnalysis",
      arguments: [],
    });
  } catch {
    const config = readVSCodeConfig();
    const editorSaveConfig = readEditorSaveConfig();
    await client.sendNotification("workspace/didChangeConfiguration", {
      settings: getSettings(config, editorSaveConfig),
    });
  }
  securityOverview.refresh();
}

interface SecurityOverviewFindingCriteria {
  uri?: string;
  code?: string;
  fixable?: boolean;
  suppressible?: boolean;
  labelIncludes?: string;
}

interface SecurityOverviewGroupCriteria {
  labelIncludes?: string;
  childUri?: string;
  childCode?: string;
}

interface SecurityOverviewFindingNodeLike {
  kind: "finding";
  label: string;
  description: string;
  ignored: boolean;
  finding: {
    uri: string;
    code: string;
    message: string;
    source: string;
    range: {
      start: { line: number; character: number };
      end: { line: number; character: number };
    };
    meta: {
      id: string;
      title: string;
      category: string;
      severity: "error" | "warning" | "info";
      kind: "compiler" | "lint" | "detector";
      hasFix: boolean;
      suppressible: boolean;
    };
  };
}

interface CoverageOverviewNodeCriteria {
  filePath?: string;
  kind?: "file" | "line";
  label?: string;
  line?: number;
}

async function snapshotSecurityOverview(
  securityOverview: SecurityOverviewProvider
): Promise<unknown[]> {
  const roots = await resolveProviderChildren(securityOverview.getChildren());
  return roots.map((node) => snapshotSecurityNode(securityOverview, node));
}

function snapshotSecurityNode(
  securityOverview: SecurityOverviewProvider,
  node: SecurityOverviewNode
): unknown {
  const item = securityOverview.getTreeItem(node);
  if (node.kind === "group") {
    return {
      kind: "group",
      label: node.label,
      description: node.description,
      contextValue: item.contextValue,
      children: node.children.map((child) =>
        snapshotSecurityNode(securityOverview, child)
      ),
    };
  }
  return {
    kind: "finding",
    label: node.label,
    description: node.description,
    contextValue: item.contextValue,
    code: node.finding.code,
    uri: node.finding.uri,
  };
}

async function findSecurityOverviewFinding(
  securityOverview: SecurityOverviewProvider,
  criteria: SecurityOverviewFindingCriteria = {}
): Promise<SecurityOverviewFindingNodeLike | undefined> {
  const node = await findSecurityOverviewFindingNode(securityOverview, criteria);
  return node;
}

async function ignoreSecurityOverviewFinding(
  securityOverview: SecurityOverviewProvider,
  criteria: SecurityOverviewFindingCriteria = {}
): Promise<boolean> {
  const node = await findSecurityOverviewFindingNode(securityOverview, criteria);
  if (!node) {
    return false;
  }
  await securityOverview.ignoreFinding(node);
  return true;
}

async function restoreSecurityOverviewFinding(
  securityOverview: SecurityOverviewProvider,
  criteria: SecurityOverviewFindingCriteria = {}
): Promise<boolean> {
  const node = await findSecurityOverviewFindingNode(securityOverview, criteria);
  if (!node) {
    return false;
  }
  await securityOverview.restoreFinding(node);
  return true;
}

async function ignoreSecurityOverviewGroup(
  securityOverview: SecurityOverviewProvider,
  criteria: SecurityOverviewGroupCriteria = {}
): Promise<boolean> {
  const node = await findSecurityOverviewGroupNode(securityOverview, criteria);
  if (!node) {
    return false;
  }
  await securityOverview.ignoreGroup(node);
  return true;
}

async function restoreSecurityOverviewGroup(
  securityOverview: SecurityOverviewProvider,
  criteria: SecurityOverviewGroupCriteria = {}
): Promise<boolean> {
  const node = await findSecurityOverviewGroupNode(securityOverview, criteria);
  if (!node) {
    return false;
  }
  await securityOverview.restoreGroup(node);
  return true;
}

async function applySecurityOverviewGroupFixes(
  securityOverview: SecurityOverviewProvider,
  criteria: SecurityOverviewGroupCriteria = {}
): Promise<boolean> {
  const node = await findSecurityOverviewGroupNode(securityOverview, criteria);
  if (!node) {
    return false;
  }
  await applyGroupFixes(node);
  return true;
}

async function suppressSecurityOverviewGroupNextLine(
  securityOverview: SecurityOverviewProvider,
  criteria: SecurityOverviewGroupCriteria = {}
): Promise<boolean> {
  const node = await findSecurityOverviewGroupNode(securityOverview, criteria);
  if (!node) {
    return false;
  }
  await suppressGroupNextLine(node);
  return true;
}

async function findSecurityOverviewFindingNode(
  securityOverview: SecurityOverviewProvider,
  criteria: SecurityOverviewFindingCriteria = {}
): Promise<SecurityOverviewFindingNode | undefined> {
  const roots = await resolveProviderChildren(securityOverview.getChildren());
  for (const root of roots) {
    if (root.kind !== "group") {
      continue;
    }
    for (const child of root.children) {
      if (matchesSecurityFinding(child, criteria)) {
        return child;
      }
    }
  }
  return undefined;
}

async function findSecurityOverviewGroupNode(
  securityOverview: SecurityOverviewProvider,
  criteria: SecurityOverviewGroupCriteria = {}
): Promise<Extract<SecurityOverviewNode, { kind: "group" }> | undefined> {
  const roots = await resolveProviderChildren(securityOverview.getChildren());
  for (const root of roots) {
    if (root.kind !== "group") {
      continue;
    }
    if (matchesSecurityGroup(root, criteria)) {
      return root;
    }
  }
  return undefined;
}

function matchesSecurityFinding(
  node: SecurityOverviewFindingNodeLike,
  criteria: SecurityOverviewFindingCriteria
): boolean {
  if (criteria.uri && node.finding.uri !== criteria.uri) {
    return false;
  }
  if (criteria.code && node.finding.code !== criteria.code) {
    return false;
  }
  if (
    typeof criteria.fixable === "boolean" &&
    node.finding.meta.hasFix !== criteria.fixable
  ) {
    return false;
  }
  if (
    typeof criteria.suppressible === "boolean" &&
    node.finding.meta.suppressible !== criteria.suppressible
  ) {
    return false;
  }
  if (
    criteria.labelIncludes &&
    !node.label.toLowerCase().includes(criteria.labelIncludes.toLowerCase())
  ) {
    return false;
  }
  return true;
}

function matchesSecurityGroup(
  node: Extract<SecurityOverviewNode, { kind: "group" }>,
  criteria: SecurityOverviewGroupCriteria
): boolean {
  if (
    criteria.labelIncludes &&
    !node.label.toLowerCase().includes(criteria.labelIncludes.toLowerCase())
  ) {
    return false;
  }
  if (criteria.childUri || criteria.childCode) {
    return node.children.some(
      (child) =>
        (!criteria.childUri || child.finding.uri === criteria.childUri) &&
        (!criteria.childCode || child.finding.code === criteria.childCode)
    );
  }
  return true;
}

async function snapshotCoverageOverview(
  coverageOverview: CoverageOverviewFeature
): Promise<unknown[]> {
  const roots = await resolveProviderChildren(coverageOverview.getChildren());
  return roots.map((node) => snapshotCoverageNode(coverageOverview, node));
}

function snapshotCoverageNode(
  coverageOverview: CoverageOverviewFeature,
  node: CoverageOverviewNode
): unknown {
  const item = coverageOverview.getTreeItem(node);
  if (node.kind === "file") {
    return {
      kind: "file",
      label: node.label,
      description: node.description,
      contextValue: item.contextValue,
      filePath: node.summary.filePath,
      children: node.children.map((child) =>
        snapshotCoverageNode(coverageOverview, child)
      ),
    };
  }
  return {
    kind: "line",
    label: node.label,
    description: node.description,
    contextValue: item.contextValue,
    filePath: node.filePath,
    line: node.detail.line,
    status: node.detail.status,
  };
}

async function findCoverageOverviewNode(
  coverageOverview: CoverageOverviewFeature,
  criteria: CoverageOverviewNodeCriteria = {}
): Promise<CoverageOverviewNode | undefined> {
  const roots = await resolveProviderChildren(coverageOverview.getChildren());
  for (const root of roots) {
    if (matchesCoverageNode(root, criteria)) {
      return root;
    }
    if (root.kind === "file") {
      for (const child of root.children) {
        if (matchesCoverageNode(child, criteria)) {
          return child;
        }
      }
    }
  }
  return undefined;
}

function matchesCoverageNode(
  node: CoverageOverviewNode,
  criteria: CoverageOverviewNodeCriteria
): boolean {
  if (criteria.kind && node.kind !== criteria.kind) {
    return false;
  }
  if (criteria.label && node.label !== criteria.label) {
    return false;
  }
  if (criteria.filePath) {
    const nodeFilePath =
      node.kind === "file" ? node.summary.filePath : node.filePath;
    if (nodeFilePath !== criteria.filePath) {
      return false;
    }
  }
  if (typeof criteria.line === "number") {
    if (node.kind !== "line" || node.detail.line !== criteria.line) {
      return false;
    }
  }
  return true;
}

async function resolveProviderChildren<T>(
  value: T[] | undefined | null | Promise<T[] | undefined | null> | Thenable<T[] | undefined | null>
): Promise<T[]> {
  const resolved = await Promise.resolve(value);
  return resolved ?? [];
}
