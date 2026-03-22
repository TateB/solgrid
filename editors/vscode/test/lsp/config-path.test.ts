import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { TestLspClient } from "./client";
import {
  changeDocument,
  initializeServer,
  openDocument,
  waitForDiagnostics,
} from "./helpers";

describe("LSP configPath", () => {
  let client: TestLspClient;
  let tempDir: string;

  beforeEach(() => {
    client = new TestLspClient();
    client.start();
    tempDir = mkdtempSync(join(tmpdir(), "solgrid-lsp-config-"));
  });

  afterEach(() => {
    client.kill();
    rmSync(tempDir, { recursive: true, force: true });
  });

  it("applies configPath during initialize", async () => {
    const configPath = join(tempDir, "disabled.toml");
    writeFileSync(configPath, '[lint.rules]\n"security/tx-origin" = "off"\n');

    await initializeServer(client, `file://${tempDir}`, {
      configPath,
      fixOnSave: true,
      fixOnSaveUnsafe: false,
      formatOnSave: true,
    });

    const uri = `file://${join(tempDir, "Test.sol")}`;
    const source = `// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    function bad() public {
        require(tx.origin == msg.sender);
    }
}
`;

    openDocument(client, uri, source);
    const diagnostics = await waitForDiagnostics(client, uri);
    const ruleIds = diagnostics.diagnostics
      .map((diag) => diag.code)
      .filter((code): code is string => typeof code === "string");
    expect(ruleIds).not.toContain("security/tx-origin");
  });

  it("applies configPath updates from didChangeConfiguration", async () => {
    await initializeServer(client, `file://${tempDir}`);

    const uri = `file://${join(tempDir, "Test.sol")}`;
    const source = `// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;
contract Test {
    function bad() public {
        require(tx.origin == msg.sender);
    }
}
`;

    openDocument(client, uri, source);
    let diagnostics = await waitForDiagnostics(client, uri);
    let ruleIds = diagnostics.diagnostics
      .map((diag) => diag.code)
      .filter((code): code is string => typeof code === "string");
    expect(ruleIds).toContain("security/tx-origin");

    const configPath = join(tempDir, "disabled.toml");
    writeFileSync(configPath, '[lint.rules]\n"security/tx-origin" = "off"\n');

    client.notify("workspace/didChangeConfiguration", {
      settings: {
        fixOnSave: true,
        fixOnSaveUnsafe: false,
        formatOnSave: true,
        configPath,
      },
    });
    changeDocument(client, uri, source);

    diagnostics = await waitForDiagnostics(client, uri);
    ruleIds = diagnostics.diagnostics
      .map((diag) => diag.code)
      .filter((code): code is string => typeof code === "string");
    expect(ruleIds).not.toContain("security/tx-origin");
  });
});
