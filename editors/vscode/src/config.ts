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

/** Default configuration values. */
export const DEFAULT_CONFIG: SolgridConfig = {
  enable: true,
  path: null,
  fixOnSave: true,
  fixOnSaveUnsafe: false,
  formatOnSave: true,
  configPath: null,
};

/**
 * Resolve the path to the solgrid binary.
 *
 * Priority:
 * 1. User-configured `solgrid.path`
 * 2. `SOLGRID_BIN` environment variable
 * 3. `"solgrid"` (assumes it's on PATH)
 */
export function getServerPath(config: SolgridConfig): string {
  if (config.path) {
    return config.path;
  }
  if (process.env.SOLGRID_BIN) {
    return process.env.SOLGRID_BIN;
  }
  return "solgrid";
}

/**
 * Build initialization options sent to the LSP server on startup.
 */
export function getInitializationOptions(
  config: SolgridConfig
): Record<string, unknown> {
  return {
    fixOnSave: config.fixOnSave,
    fixOnSaveUnsafe: config.fixOnSaveUnsafe,
    formatOnSave: config.formatOnSave,
    configPath: config.configPath,
  };
}

/**
 * Build settings payload sent to the LSP server on configuration change.
 */
export function getSettings(config: SolgridConfig): Record<string, unknown> {
  return {
    fixOnSave: config.fixOnSave,
    fixOnSaveUnsafe: config.fixOnSaveUnsafe,
    formatOnSave: config.formatOnSave,
  };
}
