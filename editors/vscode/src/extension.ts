import { workspace, ExtensionContext, window } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";
import {
  EditorSaveConfig,
  SolgridConfig,
  getServerPath,
  getInitializationOptions,
  getSettings,
} from "./config";

let client: LanguageClient | undefined;

export async function activate(context: ExtensionContext): Promise<void> {
  const solgridConfig = readVSCodeConfig();
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

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "solidity" }],
    synchronize: {
      configurationSection: "solgrid",
      fileEvents: workspace.createFileSystemWatcher("**/solgrid.toml"),
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

  client.outputChannel.appendLine(`Using solgrid binary: ${serverPath}`);

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
