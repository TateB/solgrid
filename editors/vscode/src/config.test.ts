import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  DEFAULT_EDITOR_SAVE_CONFIG,
  SolgridConfig,
  DEFAULT_CONFIG,
  getEffectiveServerFormatOnSave,
  getServerPath,
  getInitializationOptions,
  getSettings,
} from "./config";

describe("DEFAULT_CONFIG", () => {
  it("has sensible defaults", () => {
    expect(DEFAULT_CONFIG.enable).toBe(true);
    expect(DEFAULT_CONFIG.path).toBeNull();
    expect(DEFAULT_CONFIG.fixOnSave).toBe(true);
    expect(DEFAULT_CONFIG.fixOnSaveUnsafe).toBe(false);
    expect(DEFAULT_CONFIG.formatOnSave).toBe(true);
    expect(DEFAULT_CONFIG.configPath).toBeNull();
  });
});

describe("getServerPath", () => {
  let savedEnv: string | undefined;
  let tempDirs: string[] = [];

  beforeEach(() => {
    savedEnv = process.env.SOLGRID_BIN;
    delete process.env.SOLGRID_BIN;
    tempDirs = [];
  });

  afterEach(() => {
    if (savedEnv !== undefined) {
      process.env.SOLGRID_BIN = savedEnv;
    } else {
      delete process.env.SOLGRID_BIN;
    }

    for (const dir of tempDirs) {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("returns configured path when set", () => {
    const config: SolgridConfig = {
      ...DEFAULT_CONFIG,
      path: "/usr/local/bin/solgrid",
    };
    expect(getServerPath(config)).toBe("/usr/local/bin/solgrid");
  });

  it("prefers configured path over SOLGRID_BIN", () => {
    process.env.SOLGRID_BIN = "/env/bin/solgrid";
    const config: SolgridConfig = {
      ...DEFAULT_CONFIG,
      path: "/usr/local/bin/solgrid",
    };
    expect(getServerPath(config)).toBe("/usr/local/bin/solgrid");
  });

  it("returns SOLGRID_BIN when path is null", () => {
    process.env.SOLGRID_BIN = "/env/bin/solgrid";
    const config: SolgridConfig = { ...DEFAULT_CONFIG, path: null };
    expect(getServerPath(config)).toBe("/env/bin/solgrid");
  });

  it("returns bundled binary when present", () => {
    const extensionPath = mkdtempSync(join(tmpdir(), "solgrid-vscode-"));
    const binaryName = process.platform === "win32" ? "solgrid.exe" : "solgrid";
    const binDir = join(extensionPath, "bin");
    tempDirs.push(extensionPath);

    mkdirSync(binDir, { recursive: true });
    writeFileSync(join(binDir, binaryName), "");

    const config: SolgridConfig = { ...DEFAULT_CONFIG, path: null };
    expect(getServerPath(config, extensionPath)).toBe(join(binDir, binaryName));
  });

  it("returns 'solgrid' when path is null, SOLGRID_BIN is unset, and no bundled binary exists", () => {
    const config: SolgridConfig = { ...DEFAULT_CONFIG, path: null };
    expect(getServerPath(config)).toBe("solgrid");
  });

  it("returns 'solgrid' when path is empty string, SOLGRID_BIN is unset, and no bundled binary exists", () => {
    const config: SolgridConfig = { ...DEFAULT_CONFIG, path: "" };
    expect(getServerPath(config)).toBe("solgrid");
  });
});

describe("getInitializationOptions", () => {
  it("maps all config fields with defaults", () => {
    const opts = getInitializationOptions(DEFAULT_CONFIG);
    expect(opts).toEqual({
      fixOnSave: true,
      fixOnSaveUnsafe: false,
      formatOnSave: true,
      configPath: null,
    });
  });

  it("maps custom config values", () => {
    const config: SolgridConfig = {
      ...DEFAULT_CONFIG,
      fixOnSave: false,
      fixOnSaveUnsafe: true,
      formatOnSave: false,
      configPath: "/path/to/solgrid.toml",
    };
    const opts = getInitializationOptions(config);
    expect(opts).toEqual({
      fixOnSave: false,
      fixOnSaveUnsafe: true,
      formatOnSave: false,
      configPath: "/path/to/solgrid.toml",
    });
  });

  it("disables server-side format-on-save when VS Code already formats with solgrid", () => {
    const opts = getInitializationOptions(DEFAULT_CONFIG, {
      formatOnSave: true,
      defaultFormatter: "solgrid.solgrid-vscode",
    });
    expect(opts).toEqual({
      fixOnSave: true,
      fixOnSaveUnsafe: false,
      formatOnSave: false,
      configPath: null,
    });
  });
});

describe("getSettings", () => {
  it("maps all settings fields with defaults", () => {
    const settings = getSettings(DEFAULT_CONFIG);
    expect(settings).toEqual({
      fixOnSave: true,
      fixOnSaveUnsafe: false,
      formatOnSave: true,
      configPath: null,
    });
  });

  it("includes configPath for runtime config updates", () => {
    const settings = getSettings(DEFAULT_CONFIG);
    expect(settings).toHaveProperty("configPath", null);
    expect(settings).not.toHaveProperty("path");
    expect(settings).not.toHaveProperty("enable");
  });

  it("maps custom settings values", () => {
    const config: SolgridConfig = {
      ...DEFAULT_CONFIG,
      fixOnSave: false,
      fixOnSaveUnsafe: true,
      formatOnSave: false,
    };
    const settings = getSettings(config);
    expect(settings).toEqual({
      fixOnSave: false,
      fixOnSaveUnsafe: true,
      formatOnSave: false,
      configPath: null,
    });
  });

  it("keeps server-side format-on-save when VS Code uses another formatter", () => {
    const settings = getSettings(DEFAULT_CONFIG, {
      formatOnSave: true,
      defaultFormatter: "esbenp.prettier-vscode",
    });
    expect(settings).toEqual({
      fixOnSave: true,
      fixOnSaveUnsafe: false,
      formatOnSave: true,
      configPath: null,
    });
  });
});

describe("getEffectiveServerFormatOnSave", () => {
  it("returns the solgrid setting when editor format on save is off", () => {
    expect(
      getEffectiveServerFormatOnSave(DEFAULT_CONFIG, DEFAULT_EDITOR_SAVE_CONFIG)
    ).toBe(true);
  });

  it("returns false when editor format-on-save already uses solgrid", () => {
    expect(
      getEffectiveServerFormatOnSave(DEFAULT_CONFIG, {
        formatOnSave: true,
        defaultFormatter: "solgrid.solgrid-vscode",
      })
    ).toBe(false);
  });

  it("returns false when solgrid format-on-save is disabled", () => {
    expect(
      getEffectiveServerFormatOnSave(
        { ...DEFAULT_CONFIG, formatOnSave: false },
        {
          formatOnSave: true,
          defaultFormatter: "solgrid.solgrid-vscode",
        }
      )
    ).toBe(false);
  });
});
