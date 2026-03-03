import { describe, it, expect } from "vitest";
import * as prettier from "prettier";
import * as plugin from "../../src/index.mjs";
import { readFileSync, readdirSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIXTURES_DIR = join(__dirname, "fixtures");

async function formatSol(source, options = {}) {
  return prettier.format(source, {
    parser: "solgrid",
    plugins: [plugin],
    ...options,
  });
}

/**
 * Get all .sol fixture files from the fixtures directory.
 */
function getFixtureFiles() {
  return readdirSync(FIXTURES_DIR)
    .filter((f) => f.endsWith(".sol"))
    .sort();
}

describe("conformance test suite", () => {
  const fixtureFiles = getFixtureFiles();

  describe("formatting produces valid output", () => {
    for (const file of fixtureFiles) {
      it(`formats ${file} without error`, async () => {
        const source = readFileSync(join(FIXTURES_DIR, file), "utf-8");
        const result = await formatSol(source);
        // Should produce non-empty output
        expect(result.length).toBeGreaterThan(0);
        // Should contain at least the pragma and SPDX
        expect(result).toContain("SPDX-License-Identifier");
        expect(result).toContain("pragma solidity");
      });
    }
  });

  describe("idempotency — format(format(x)) === format(x)", () => {
    for (const file of fixtureFiles) {
      it(`${file} is idempotent`, async () => {
        const source = readFileSync(join(FIXTURES_DIR, file), "utf-8");
        const first = await formatSol(source);
        const second = await formatSol(first);
        expect(second).toBe(first);
      });
    }
  });

  describe("option variations", () => {
    const source = readFileSync(
      join(FIXTURES_DIR, "SimpleContract.sol"),
      "utf-8",
    );

    it("tabWidth: 2 produces 2-space indentation", async () => {
      const result = await formatSol(source, { tabWidth: 2 });
      // Lines inside the contract should use 2-space indent
      const indentedLines = result
        .split("\n")
        .filter((l) => l.match(/^ {2}\S/));
      expect(indentedLines.length).toBeGreaterThan(0);
    });

    it("tabWidth: 4 produces 4-space indentation (default)", async () => {
      const result = await formatSol(source, { tabWidth: 4 });
      // Lines inside the contract should use 4-space indent
      const indentedLines = result
        .split("\n")
        .filter((l) => l.match(/^ {4}\S/));
      expect(indentedLines.length).toBeGreaterThan(0);
    });

    it("useTabs: true produces tab indentation", async () => {
      const result = await formatSol(source, { useTabs: true });
      expect(result).toContain("\t");
    });

    it("useTabs: false produces space indentation", async () => {
      const result = await formatSol(source, { useTabs: false });
      // Should have indented lines (spaces, not tabs)
      const indentedLines = result
        .split("\n")
        .filter((l) => l.match(/^ +\S/));
      expect(indentedLines.length).toBeGreaterThan(0);
      // No tabs expected inside the contract body
      const contractBody = result.split("contract")[1] || "";
      // Indented lines should not start with tabs
      const tabLines = contractBody.split("\n").filter((l) => l.startsWith("\t"));
      expect(tabLines.length).toBe(0);
    });

    it("printWidth: 80 vs 120 may differ in line wrapping", async () => {
      const wide = await formatSol(source, { printWidth: 120 });
      const narrow = await formatSol(source, { printWidth: 80 });
      // Both should be valid and contain the contract
      expect(wide).toContain("contract SimpleContract");
      expect(narrow).toContain("contract SimpleContract");
    });
  });

  describe("comment preservation", () => {
    it("preserves single-line comments", async () => {
      const source = readFileSync(join(FIXTURES_DIR, "Comments.sol"), "utf-8");
      const result = await formatSol(source);
      expect(result).toContain("// Single-line comment");
      expect(result).toContain("// Trailing comment");
      expect(result).toContain("// save old value");
    });

    it("preserves multi-line comments", async () => {
      const source = readFileSync(join(FIXTURES_DIR, "Comments.sol"), "utf-8");
      const result = await formatSol(source);
      expect(result).toContain("Multi-line comment");
      expect(result).toContain("spanning multiple lines");
    });

    it("preserves NatSpec comments", async () => {
      const source = readFileSync(join(FIXTURES_DIR, "Comments.sol"), "utf-8");
      const result = await formatSol(source);
      expect(result).toContain("@notice NatSpec function documentation");
      expect(result).toContain("@param _value The new value to set");
      expect(result).toContain("@return The old value");
    });
  });

  describe("structural preservation", () => {
    it("preserves contract inheritance", async () => {
      const source = readFileSync(
        join(FIXTURES_DIR, "ComplexFunctions.sol"),
        "utf-8",
      );
      const result = await formatSol(source);
      expect(result).toContain("Ownable");
      expect(result).toContain("ReentrancyGuard");
    });

    it("preserves import statements", async () => {
      const source = readFileSync(
        join(FIXTURES_DIR, "Imports.sol"),
        "utf-8",
      );
      const result = await formatSol(source);
      expect(result).toContain("IERC20");
      expect(result).toContain("Ownable");
      expect(result).toContain("SafeERC20");
    });

    it("preserves interface and library", async () => {
      const source = readFileSync(
        join(FIXTURES_DIR, "Interface.sol"),
        "utf-8",
      );
      const result = await formatSol(source);
      expect(result).toContain("interface IVault");
      expect(result).toContain("library MathLib");
    });

    it("preserves control flow structures", async () => {
      const source = readFileSync(
        join(FIXTURES_DIR, "ControlFlow.sol"),
        "utf-8",
      );
      const result = await formatSol(source);
      expect(result).toContain("for (");
      expect(result).toContain("if (");
      expect(result).toContain("while (");
    });

    it("preserves events, errors, and structs", async () => {
      const source = readFileSync(join(FIXTURES_DIR, "Events.sol"), "utf-8");
      const result = await formatSol(source);
      expect(result).toContain("event Transfer");
      expect(result).toContain("error InsufficientBalance");
      expect(result).toContain("struct UserInfo");
      expect(result).toContain("enum Status");
    });
  });

  describe("cross-fixture consistency", () => {
    it("all fixtures format to end with a newline", async () => {
      for (const file of fixtureFiles) {
        const source = readFileSync(join(FIXTURES_DIR, file), "utf-8");
        const result = await formatSol(source);
        expect(result.endsWith("\n"), `${file} should end with newline`).toBe(
          true,
        );
      }
    });

    it("all fixtures produce stable output across repeated formatting", async () => {
      for (const file of fixtureFiles) {
        const source = readFileSync(join(FIXTURES_DIR, file), "utf-8");
        const first = await formatSol(source);
        const second = await formatSol(first);
        const third = await formatSol(second);
        expect(third, `${file} unstable after 3 rounds`).toBe(second);
      }
    });
  });
});
