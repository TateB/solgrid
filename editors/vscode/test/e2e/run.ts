/**
 * VSCode E2E Test Runner
 *
 * Downloads and launches a VSCode instance with the solgrid extension loaded,
 * then runs the e2e test suite inside it.
 *
 * This runner tests extension behavior that can only be verified inside a real
 * VSCode instance: activation, configuration reading, language client lifecycle.
 *
 * Note: Cursor uses the same Extension Host and LSP protocol as VSCode.
 * These e2e tests validate behavior that applies to both editors.
 */

import * as path from "path";
import * as fs from "fs";
import { runTests } from "@vscode/test-electron";

async function main() {
  // The path to the extension root (editors/vscode/)
  // __dirname at runtime is out/test/e2e/, so we need 3 levels up
  const extensionDevelopmentPath = path.resolve(__dirname, "../../../");

  // The path to the test entry point (compiled from index.ts)
  const extensionTestsPath = path.resolve(__dirname, "./index");

  // The workspace folder containing test fixtures (source, not compiled)
  const testWorkspace = path.resolve(__dirname, "../../../test/fixtures");

  const solgridBinary = getSolgridBinaryPath(extensionDevelopmentPath);

  // Write workspace settings so the extension host uses the intended binary
  // even when environment variables are not propagated through VS Code.
  const settingsDir = path.join(testWorkspace, ".vscode");
  let createdSettings = false;
  if (solgridBinary !== "solgrid") {
    fs.mkdirSync(settingsDir, { recursive: true });
    fs.writeFileSync(
      path.join(settingsDir, "settings.json"),
      JSON.stringify({ "solgrid.path": solgridBinary }, null, 2)
    );
    createdSettings = true;
    console.log(`Configured solgrid.path = ${solgridBinary}`);
  }

  try {
    await runTests({
      extensionDevelopmentPath,
      extensionTestsPath,
      launchArgs: [
        testWorkspace,
        "--disable-extensions", // Disable other extensions to isolate our tests
      ],
    });
  } catch (err) {
    console.error("Failed to run VSCode e2e tests:", err);
    process.exit(1);
  } finally {
    // Clean up workspace settings
    if (createdSettings) {
      try {
        fs.rmSync(settingsDir, { recursive: true });
      } catch {
        // Ignore cleanup errors
      }
    }
  }
}

function getSolgridBinaryPath(extensionDevelopmentPath: string): string {
  if (process.env.SOLGRID_BIN) {
    return process.env.SOLGRID_BIN;
  }

  const repoRoot = path.resolve(extensionDevelopmentPath, "../..");
  const binaryName = process.platform === "win32" ? "solgrid.exe" : "solgrid";
  const debugPath = path.join(repoRoot, "target", "debug", binaryName);
  if (fs.existsSync(debugPath)) {
    return debugPath;
  }

  const releasePath = path.join(repoRoot, "target", "release", binaryName);
  if (fs.existsSync(releasePath)) {
    return releasePath;
  }

  return "solgrid";
}

main();
