import { commands, ExtensionContext, window, workspace } from "vscode";
import {
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
  openFindingHelp,
  openSecurityFinding,
  SecurityOverviewNode,
  SecurityOverviewProvider,
  suppressGroupNextLine,
  suppressFindingNextLine,
} from "./securityOverview";
import { activeImportsGraphArgs, showGraph } from "./graphPreview";

let client: LanguageClient | undefined;

export async function activate(context: ExtensionContext): Promise<void> {
  const solgridConfig = readVSCodeConfig();
  let coverageConfig = readCoverageConfig();
  const editorSaveConfig = readEditorSaveConfig();

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

  const coverageOverview = new CoverageOverviewFeature();
  const coverageOverviewView = window.createTreeView<CoverageOverviewNode>(
    "solgridCoverageOverview",
    {
      treeDataProvider: coverageOverview,
      showCollapseAll: true,
    }
  );
  coverageOverview.attachView(coverageOverviewView);
  await coverageOverview.applyConfig(coverageConfig);
  context.subscriptions.push(coverageOverview, coverageOverviewView);

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "solidity" }],
    synchronize: {
      configurationSection: "solgrid",
      fileEvents: fileWatchers,
    },
    initializationOptions: getInitializationOptions(solgridConfig, editorSaveConfig),
    middleware: {
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

  client.onNotification("textDocument/publishDiagnostics", (params) => {
    securityOverview.updateFromDiagnostics(params);
  });

  client.outputChannel.appendLine(`Using solgrid binary: ${serverPath}`);

  context.subscriptions.push(
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
    commands.registerCommand("solgrid.graph.showImports", async () => {
      const args = activeImportsGraphArgs();
      if (args) {
        await showGraph(client, args);
      }
    }),
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
    commands.registerCommand("solgrid.coverage.runFoundryCobertura", () =>
      runCoverageCommand("foundry-cobertura", coverageConfig, () =>
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
        const newCoverageConfig = readCoverageConfig();
        const newEditorSaveConfig = readEditorSaveConfig();
        coverageConfig = newCoverageConfig;
        void coverageOverview.applyConfig(newCoverageConfig);
        client?.sendNotification("workspace/didChangeConfiguration", {
          settings: getSettings(newConfig, newEditorSaveConfig),
        });
      }
    })
  );

  // Register willSaveTextDocument for fix-on-save and format-on-save
  context.subscriptions.push(
    workspace.onWillSaveTextDocument((e) => {
      if (e.document.languageId !== "solidity") {
        return;
      }
      // The LSP server handles willSaveWaitUntil for fix-on-save + format-on-save
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
  if (client) {
    await client.stop();
    client = undefined;
  }
}

/**
 * Read solgrid settings from the VSCode configuration API.
 */
function readVSCodeConfig(): SolgridConfig {
  const config = workspace.getConfiguration("solgrid");
  return {
    enable: config.get<boolean>("enable", true),
    path: config.get<string | null>("path", null),
    fixOnSave: config.get<boolean>("fixOnSave", true),
    fixOnSaveUnsafe: config.get<boolean>("fixOnSave.unsafeFixes", false),
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
