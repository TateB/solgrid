import { chmodSync, existsSync } from "node:fs";
import { join } from "node:path";

/**
 * Pure configuration helpers for the solgrid VSCode extension.
 *
 * These functions accept a plain SolgridConfig object (no VSCode API dependency)
 * so they can be unit-tested without mocking the VSCode workspace API.
 */

/** Resolved solgrid extension settings. */
export interface SolgridConfig {
  enable: boolean;
  path: string | null;
  fixOnSave: boolean;
  fixOnSaveUnsafe: boolean;
  formatOnSave: boolean;
  configPath: string | null;
}

export interface EditorSaveConfig {
  formatOnSave: boolean;
  defaultFormatter: string | null;
}

/** Default configuration values. */
export const DEFAULT_CONFIG: SolgridConfig = {
  enable: true,
  path: null,
  fixOnSave: true,
  fixOnSaveUnsafe: false,
  formatOnSave: true,
  configPath: null,
};

export const DEFAULT_EDITOR_SAVE_CONFIG: EditorSaveConfig = {
  formatOnSave: false,
  defaultFormatter: null,
};

/**
 * Resolve the path to the solgrid binary.
 *
 * Priority:
 * 1. User-configured `solgrid.path`
 * 2. `SOLGRID_BIN` environment variable
 * 3. Bundled binary inside the extension
 * 4. `"solgrid"` (assumes it's on PATH)
 */
export function getServerPath(
  config: SolgridConfig,
  extensionPath?: string
): string {
  if (config.path) {
    return config.path;
  }
  if (process.env.SOLGRID_BIN) {
    return process.env.SOLGRID_BIN;
  }
  if (extensionPath) {
    const binaryName = process.platform === "win32" ? "solgrid.exe" : "solgrid";
    const bundledPath = join(extensionPath, "bin", binaryName);
    if (existsSync(bundledPath)) {
      if (process.platform !== "win32") {
        try {
          chmodSync(bundledPath, 0o755);
        } catch {}
      }
      return bundledPath;
    }
  }
  return "solgrid";
}

/**
 * Build initialization options sent to the LSP server on startup.
 */
export function getInitializationOptions(
  config: SolgridConfig,
  editorConfig: EditorSaveConfig = DEFAULT_EDITOR_SAVE_CONFIG
): Record<string, unknown> {
  return {
    fixOnSave: config.fixOnSave,
    fixOnSaveUnsafe: config.fixOnSaveUnsafe,
    formatOnSave: getEffectiveServerFormatOnSave(config, editorConfig),
    configPath: config.configPath,
  };
}

/**
 * Build settings payload sent to the LSP server on configuration change.
 */
export function getSettings(
  config: SolgridConfig,
  editorConfig: EditorSaveConfig = DEFAULT_EDITOR_SAVE_CONFIG
): Record<string, unknown> {
  return {
    fixOnSave: config.fixOnSave,
    fixOnSaveUnsafe: config.fixOnSaveUnsafe,
    formatOnSave: getEffectiveServerFormatOnSave(config, editorConfig),
  };
}

export function getEffectiveServerFormatOnSave(
  config: SolgridConfig,
  editorConfig: EditorSaveConfig = DEFAULT_EDITOR_SAVE_CONFIG
): boolean {
  return !(
    config.formatOnSave &&
    editorConfig.formatOnSave &&
    editorConfig.defaultFormatter === "solgrid.solgrid-vscode"
  ) && config.formatOnSave;
}
