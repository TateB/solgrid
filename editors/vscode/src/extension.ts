import { workspace, ExtensionContext, window } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";
import {
  SolgridConfig,
  getServerPath,
  getInitializationOptions,
  getSettings,
} from "./config";

let client: LanguageClient | undefined;

export async function activate(context: ExtensionContext): Promise<void> {
  const solgridConfig = readVSCodeConfig();

  if (!solgridConfig.enable) {
    return;
  }

  const serverPath = getServerPath(solgridConfig);

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ["server"],
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "solidity" }],
    synchronize: {
      configurationSection: "solgrid",
      fileEvents: workspace.createFileSystemWatcher("**/solgrid.toml"),
    },
    initializationOptions: getInitializationOptions(solgridConfig),
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

  // Watch for configuration changes
  context.subscriptions.push(
    workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("solgrid")) {
        const newConfig = readVSCodeConfig();
        client?.sendNotification("workspace/didChangeConfiguration", {
          settings: getSettings(newConfig),
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
