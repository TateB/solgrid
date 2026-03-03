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
import { runTests } from "@vscode/test-electron";

async function main() {
  try {
    // The path to the extension root (editors/vscode/)
    const extensionDevelopmentPath = path.resolve(__dirname, "../../");

    // The path to the test entry point (compiled from extension.test.ts)
    const extensionTestsPath = path.resolve(__dirname, "./extension.test");

    // The workspace folder containing test fixtures
    const testWorkspace = path.resolve(__dirname, "../fixtures");

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
  }
}

main();
