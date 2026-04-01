import { describe, expect, it } from "vitest";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const scriptPath = join(packageDir, "scripts", "run-lsp-tests.mjs");

describe("run-lsp-tests.mjs", () => {
  it("treats --no-build passed after -- as a runner flag", () => {
    const tempDir = mkdtempSync(join(tmpdir(), "solgrid-vscode-"));
    const binaryName = process.platform === "win32" ? "solgrid.exe" : "solgrid";
    const fakeBinary = join(tempDir, binaryName);

    writeFileSync(fakeBinary, "");

    try {
      const result = spawnSync(
        process.execPath,
        [scriptPath, "--release", "--", "--no-build", "--help"],
        {
          cwd: packageDir,
          env: {
            ...process.env,
            SOLGRID_BIN: fakeBinary,
          },
          encoding: "utf8",
        }
      );

      expect(result.status).toBe(0);
      expect(result.stderr).not.toContain("Unknown option `--build`");
      expect(result.stdout).toContain("Running LSP integration tests with");
    } finally {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });
});
