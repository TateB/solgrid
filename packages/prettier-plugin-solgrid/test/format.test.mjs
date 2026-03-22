import { describe, it, expect } from "vitest";
import * as prettier from "prettier";
import * as plugin from "../src/index.mjs";

async function formatSol(source, options = {}) {
  return prettier.format(source, {
    parser: "solgrid",
    plugins: [plugin],
    ...options,
  });
}

describe("prettier-plugin-solgrid", () => {
  describe("basic formatting", () => {
    it("formats a simple contract", async () => {
      const source =
        "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\ncontract   Foo  {  }\n";
      const result = await formatSol(source);
      expect(result).toContain("contract Foo");
    });

    it("preserves pragma", async () => {
      const source =
        "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n";
      const result = await formatSol(source);
      expect(result).toContain("pragma solidity ^0.8.0;");
    });

    it("formats a contract with functions", async () => {
      const source = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
function foo() public pure returns (uint256) { return 1; }
}
`;
      const result = await formatSol(source);
      expect(result).toContain("function foo()");
      expect(result).toContain("return 1;");
    });
  });

  describe("option mapping", () => {
    it("respects tabWidth option", async () => {
      const source = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract T {
function f() public {}
}
`;
      const result = await formatSol(source, { tabWidth: 2 });
      // With tabWidth 2, indentation should use 2 spaces
      expect(result).toMatch(/^ {2}\S/m);
    });

    it("respects useTabs option", async () => {
      const source = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract T {
function f() public {}
}
`;
      const result = await formatSol(source, { useTabs: true });
      expect(result).toContain("\t");
    });

    it("maps contract body spacing and deprecated alias consistently", async () => {
      const source = `contract T {
uint256 public x;
uint256 public y;
}
`;
      const canonical = await formatSol(source, {
        solidityContractBodySpacing: "single",
      });
      const deprecatedAlias = await formatSol(source, {
        solidityContractNewLines: true,
      });

      expect(canonical).toBe(deprecatedAlias);
      expect(canonical).toContain("uint256 public x;\n\n  uint256 public y;");
    });

    it("respects inheritance brace placement option", async () => {
      const source =
        "contract OwnedResolver is Ownable, ABIResolver, AddrResolver, ContentHashResolver, DNSResolver, InterfaceResolver, NameResolver, PubkeyResolver, TextResolver, ExtendedResolver {}\n";
      const result = await formatSol(source, {
        solidityInheritanceBraceNewLine: false,
      });
      expect(result).not.toContain("\n{");
    });
  });

  describe("error handling", () => {
    it("throws on syntax errors", async () => {
      const source = "this is not solidity at all {{{";
      await expect(formatSol(source)).rejects.toThrow();
    });
  });

  describe("idempotency", () => {
    it("formatting is idempotent", async () => {
      const source = `// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    uint256 public value;
    function setValue(uint256 _value) external { value = _value; }
    function getValue() external view returns (uint256) { return value; }
}
`;
      const first = await formatSol(source);
      const second = await formatSol(first);
      expect(second).toBe(first);
    });
  });
});

describe("plugin exports", () => {
  it("exports languages", () => {
    expect(plugin.languages).toBeDefined();
    expect(plugin.languages).toHaveLength(1);
    expect(plugin.languages[0].name).toBe("Solidity");
    expect(plugin.languages[0].extensions).toContain(".sol");
  });

  it("exports parsers with solgrid parser", () => {
    expect(plugin.parsers).toBeDefined();
    expect(plugin.parsers.solgrid).toBeDefined();
    expect(plugin.parsers.solgrid.astFormat).toBe("solgrid-ast");
  });

  it("exports printers for solgrid-ast", () => {
    expect(plugin.printers).toBeDefined();
    expect(plugin.printers["solgrid-ast"]).toBeDefined();
    expect(plugin.printers["solgrid-ast"].print).toBeInstanceOf(Function);
  });

  it("exports solgrid-specific options", () => {
    expect(plugin.options).toBeDefined();
    expect(plugin.options.solidityNumberUnderscore).toBeDefined();
    expect(plugin.options.solidityUintType).toBeDefined();
    expect(plugin.options.soliditySortImports).toBeDefined();
    expect(plugin.options.solidityMultilineFuncHeader).toBeDefined();
    expect(plugin.options.solidityOverrideSpacing).toBeDefined();
    expect(plugin.options.solidityWrapComments).toBeDefined();
    expect(plugin.options.solidityContractBodySpacing).toBeDefined();
    expect(plugin.options.solidityInheritanceBraceNewLine).toBeDefined();
    expect(plugin.options.solidityContractNewLines).toBeDefined();
  });

  it("all options have descriptions", () => {
    for (const [name, opt] of Object.entries(plugin.options)) {
      expect(opt.description, `${name} missing description`).toBeTruthy();
    }
  });
});
