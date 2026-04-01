/**
 * LSP Signature Help Tests
 *
 * Tests textDocument/signatureHelp for user-defined callables and builtins.
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { TestLspClient } from "./client";
import {
  initializeServer,
  openDocument,
  closeDocument,
  waitForDiagnostics,
  requestSignatureHelp,
  resetDocumentVersions,
} from "./helpers";

describe("LSP Signature Help", () => {
  let client: TestLspClient;

  beforeEach(async () => {
    client = new TestLspClient();
    client.start();
    resetDocumentVersions();
    await initializeServer(client);
  });

  afterEach(async () => {
    try {
      await client.shutdown();
    } catch {
      client.kill();
    }
  });

  it("returns signature help for user-defined functions with active parameter", async () => {
    const uri = "file:///tmp/signature-help-function.sol";
    const content = [
      "pragma solidity ^0.8.0;",
      "",
      "contract Test {",
      "  function transfer(address recipient, uint256 amount, string memory note) public {}",
      "",
      "  function callTransfer(address recipient) public {",
      "    transfer(recipient, ",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const help = await requestSignatureHelp(client, uri, {
      line: 6,
      character: 24, // after "transfer(recipient, "
    });

    expect(help).toBeTruthy();
    expect(help!.signatures[0].label).toContain(
      "function transfer(address recipient, uint256 amount, string memory note) public"
    );
    expect(help!.activeParameter).toBe(1);
  });

  it("returns signature help for contract member calls", async () => {
    const uri = "file:///tmp/signature-help-member.sol";
    const content = [
      "pragma solidity ^0.8.0;",
      "",
      "contract SomethingA {",
      "  function update(uint256 count, address owner) public {}",
      "}",
      "",
      "contract SomethingB {",
      "  SomethingA public somethingA;",
      "",
      "  constructor() {",
      "    somethingA = new SomethingA();",
      "  }",
      "",
      "  function callUpdate() public {",
      "    somethingA.update(1, ",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, uri, content);
    await waitForDiagnostics(client, uri).catch(() => {});

    const help = await requestSignatureHelp(client, uri, {
      line: 14,
      character: 25, // after "somethingA.update(1, "
    });

    expect(help).toBeTruthy();
    expect(help!.signatures[0].label).toContain(
      "function update(uint256 count, address owner) public"
    );
    expect(help!.activeParameter).toBe(1);
  });

  it("returns signature help for constructors and builtins", async () => {
    const uri = "file:///tmp/signature-help-constructor.sol";
    const constructorContent = [
      "pragma solidity ^0.8.0;",
      "",
      "contract SomethingA {",
      "  constructor(uint256 count, address owner) {}",
      "}",
      "",
      "contract SomethingB {",
      "  function build(address owner) public {",
      "    SomethingA created = new SomethingA(1, ",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, uri, constructorContent);
    await waitForDiagnostics(client, uri).catch(() => {});

    const constructorHelp = await requestSignatureHelp(client, uri, {
      line: 8,
      character: 43, // after "new SomethingA(1, "
    });
    closeDocument(client, uri);

    const builtinUri = "file:///tmp/signature-help-builtin.sol";
    const builtinContent = [
      "pragma solidity ^0.8.0;",
      "",
      "contract SomethingB {",
      "  function guard() public {",
      "    require(true, ",
      "  }",
      "}",
      "",
    ].join("\n");

    openDocument(client, builtinUri, builtinContent);
    await waitForDiagnostics(client, builtinUri).catch(() => {});

    const builtinHelp = await requestSignatureHelp(client, builtinUri, {
      line: 4,
      character: 18, // after "require(true, "
    });

    expect(constructorHelp).toBeTruthy();
    expect(constructorHelp!.signatures[0].label).toContain(
      "constructor(uint256 count, address owner)"
    );
    expect(constructorHelp!.activeParameter).toBe(1);

    expect(builtinHelp).toBeTruthy();
    expect(builtinHelp!.signatures[0].label).toContain("require(");
    expect(builtinHelp!.activeParameter).toBe(1);
  });
});
