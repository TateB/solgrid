/**
 * VSCode E2E Test Runner Entry Point
 *
 * This module is loaded by @vscode/test-electron inside the Extension Host.
 * It creates a Mocha instance (which registers BDD globals like describe/it),
 * discovers test files, and runs them.
 */

import * as path from "path";
import Mocha from "mocha";

export function run(): Promise<void> {
  const mocha = new Mocha({ ui: "bdd", color: true, timeout: 60000 });

  const testsRoot = path.resolve(__dirname);

  return new Promise((resolve, reject) => {
    // Add the e2e test file
    mocha.addFile(path.resolve(testsRoot, "extension.test.js"));

    try {
      mocha.run((failures) => {
        if (failures > 0) {
          reject(new Error(`${failures} tests failed.`));
        } else {
          resolve();
        }
      });
    } catch (err) {
      reject(err);
    }
  });
}
