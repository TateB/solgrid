import * as path from "path";
import { workspace, ExtensionContext, window } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: ExtensionContext): Promise<void> {
  const config = workspace.getConfiguration("solgrid");

  if (!config.get<boolean>("enable", true)) {
    return;
  }

  const serverPath = getServerPath(config);

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
    initializationOptions: getInitializationOptions(config),
    middleware: {
      // Send configuration changes to the server
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
        const newConfig = workspace.getConfiguration("solgrid");
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
 * Resolve the path to the solgrid binary.
 *
 * Priority:
 * 1. User-configured `solgrid.path`
 * 2. `solgrid` on PATH
 */
function getServerPath(
  config: ReturnType<typeof workspace.getConfiguration>
): string {
  const configuredPath = config.get<string | null>("path", null);
  if (configuredPath) {
    return configuredPath;
  }

  // Default: assume solgrid is on PATH
  return "solgrid";
}

/**
 * Get initialization options from VSCode configuration.
 */
function getInitializationOptions(
  config: ReturnType<typeof workspace.getConfiguration>
): Record<string, unknown> {
  return {
    fixOnSave: config.get<boolean>("fixOnSave", true),
    fixOnSaveUnsafe: config.get<boolean>("fixOnSave.unsafeFixes", false),
    formatOnSave: config.get<boolean>("formatOnSave", true),
    configPath: config.get<string | null>("configPath", null),
  };
}

/**
 * Get current settings to send to the server.
 */
function getSettings(
  config: ReturnType<typeof workspace.getConfiguration>
): Record<string, unknown> {
  return {
    fixOnSave: config.get<boolean>("fixOnSave", true),
    fixOnSaveUnsafe: config.get<boolean>("fixOnSave.unsafeFixes", false),
    formatOnSave: config.get<boolean>("formatOnSave", true),
  };
}
