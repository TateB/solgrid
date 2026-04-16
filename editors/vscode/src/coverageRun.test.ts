import { describe, expect, it, vi } from "vitest";

vi.mock("vscode", () => ({}));
import {
  availableCoverageRunSpecs,
  coverageRunSpec,
  preferredCoverageRunSpec,
  preferredCoverageWorkspaceFolder,
} from "./coverageRun";
import { DEFAULT_COVERAGE_CONFIG } from "./config";

describe("coverageRunSpec", () => {
  it("builds the built-in Foundry LCOV command", () => {
    expect(coverageRunSpec("foundry-lcov", DEFAULT_COVERAGE_CONFIG)).toEqual({
      kind: "foundry-lcov",
      label: "Foundry Coverage (LCOV)",
      command: "forge",
      args: ["coverage", "--report", "lcov"],
    });
  });

  it("builds the built-in Foundry Cobertura command", () => {
    expect(coverageRunSpec("foundry-cobertura", DEFAULT_COVERAGE_CONFIG)).toEqual({
      kind: "foundry-cobertura",
      label: "Foundry Coverage (Cobertura)",
      command: "forge",
      args: ["coverage", "--report", "cobertura"],
    });
  });

  it("builds the built-in Hardhat LCOV command", () => {
    expect(coverageRunSpec("hardhat-lcov", DEFAULT_COVERAGE_CONFIG)).toEqual({
      kind: "hardhat-lcov",
      label: "Hardhat Coverage (LCOV)",
      command: "npx",
      args: ["hardhat", "coverage"],
    });
  });

  it("derives the custom command from configuration", () => {
    const spec = coverageRunSpec("custom", {
      ...DEFAULT_COVERAGE_CONFIG,
      customCommand: ["pnpm", "run", "coverage"],
    });
    expect(spec).toEqual({
      kind: "custom",
      label: "Custom Coverage Command",
      command: "pnpm",
      args: ["run", "coverage"],
    });
  });

  it("rejects an empty custom command", () => {
    expect(coverageRunSpec("custom", DEFAULT_COVERAGE_CONFIG)).toBeUndefined();
  });
});

describe("availableCoverageRunSpecs", () => {
  it("returns both Foundry built-ins when Foundry is available", () => {
    expect(
      availableCoverageRunSpecs(
        { hasFoundry: true, hasHardhat: false, hasCustomCommand: false },
        DEFAULT_COVERAGE_CONFIG
      ).map((spec) => spec.kind)
    ).toEqual(["foundry-lcov", "foundry-cobertura"]);
  });

  it("returns the Hardhat provider when Hardhat is available", () => {
    expect(
      availableCoverageRunSpecs(
        { hasFoundry: false, hasHardhat: true, hasCustomCommand: false },
        DEFAULT_COVERAGE_CONFIG
      ).map((spec) => spec.kind)
    ).toEqual(["hardhat-lcov"]);
  });

  it("adds the custom command when configured", () => {
    expect(
      availableCoverageRunSpecs(
        { hasFoundry: true, hasHardhat: true, hasCustomCommand: true },
        {
          ...DEFAULT_COVERAGE_CONFIG,
          customCommand: ["pnpm", "run", "coverage"],
        }
      ).map((spec) => spec.kind)
    ).toEqual(["foundry-lcov", "foundry-cobertura", "hardhat-lcov", "custom"]);
  });
});

describe("preferredCoverageRunSpec", () => {
  it("prefers Foundry LCOV when available", () => {
    const specs = availableCoverageRunSpecs(
      { hasFoundry: true, hasHardhat: true, hasCustomCommand: true },
      {
        ...DEFAULT_COVERAGE_CONFIG,
        customCommand: ["pnpm", "run", "coverage"],
      }
    );
    expect(preferredCoverageRunSpec(specs)?.kind).toBe("foundry-lcov");
  });

  it("prefers Hardhat when Foundry is unavailable", () => {
    const specs = availableCoverageRunSpecs(
      { hasFoundry: false, hasHardhat: true, hasCustomCommand: true },
      {
        ...DEFAULT_COVERAGE_CONFIG,
        customCommand: ["pnpm", "run", "coverage"],
      }
    );
    expect(preferredCoverageRunSpec(specs)?.kind).toBe("hardhat-lcov");
  });

  it("falls back to the first available option otherwise", () => {
    const specs = availableCoverageRunSpecs(
      { hasFoundry: false, hasHardhat: false, hasCustomCommand: true },
      {
        ...DEFAULT_COVERAGE_CONFIG,
        customCommand: ["pnpm", "run", "coverage"],
      }
    );
    expect(preferredCoverageRunSpec(specs)?.kind).toBe("custom");
  });
});

describe("preferredCoverageWorkspaceFolder", () => {
  const folders = [
    { name: "alpha", uri: { fsPath: "/workspace/alpha" } },
    { name: "beta", uri: { fsPath: "/workspace/beta" } },
  ];

  it("prefers the active document workspace folder", () => {
    const selected = preferredCoverageWorkspaceFolder(
      folders,
      "/workspace/beta/contracts/Vault.sol",
      (filePath) =>
        filePath.startsWith("/workspace/beta") ? folders[1] : folders[0]
    );
    expect(selected?.name).toBe("beta");
  });

  it("falls back to the only folder when there is one", () => {
    const selected = preferredCoverageWorkspaceFolder(
      [folders[0]],
      undefined,
      () => undefined
    );
    expect(selected?.name).toBe("alpha");
  });

  it("returns undefined when multiple folders exist without an active match", () => {
    const selected = preferredCoverageWorkspaceFolder(
      folders,
      undefined,
      () => undefined
    );
    expect(selected).toBeUndefined();
  });
});
