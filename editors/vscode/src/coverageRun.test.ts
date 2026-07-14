import { describe, expect, it, vi } from "vitest";

vi.mock("vscode", () => ({}));
import {
  availableCoverageRunSpecs,
  CoverageRunGuard,
  coverageRunExitOutcome,
  coverageRunSpec,
  preferredCoverageRunSpec,
  preferredCoverageWorkspaceFolder,
  TaskCompletionArbiter,
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

  it("builds the built-in Hardhat LCOV command", () => {
    expect(coverageRunSpec("hardhat-lcov", DEFAULT_COVERAGE_CONFIG)).toEqual({
      kind: "hardhat-lcov",
      label: "Hardhat Coverage (LCOV)",
      command: "npx",
      args: ["--no-install", "hardhat", "coverage"],
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
  it("returns the supported Foundry LCOV command when Foundry is available", () => {
    expect(
      availableCoverageRunSpecs(
        { hasFoundry: true, hasHardhat: false, hasCustomCommand: false },
        DEFAULT_COVERAGE_CONFIG
      ).map((spec) => spec.kind)
    ).toEqual(["foundry-lcov"]);
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
    ).toEqual(["foundry-lcov", "hardhat-lcov", "custom"]);
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

describe("coverageRunExitOutcome", () => {
  it("only treats an explicit zero exit code as success", () => {
    expect(coverageRunExitOutcome(0)).toBe("success");
    expect(coverageRunExitOutcome(1)).toBe("failed");
    expect(coverageRunExitOutcome(-1)).toBe("failed");
  });

  it("treats a task without a process exit as cancelled", () => {
    expect(coverageRunExitOutcome(undefined)).toBe("cancelled");
  });
});

describe("CoverageRunGuard", () => {
  it("rejects a concurrent run and reports the active label", () => {
    const guard = new CoverageRunGuard();
    const release = guard.acquire("Foundry Coverage (LCOV)");

    expect(release).toBeTypeOf("function");
    expect(guard.activeLabel).toBe("Foundry Coverage (LCOV)");
    expect(guard.acquire("Hardhat Coverage (LCOV)")).toBeUndefined();
  });

  it("releases on cleanup and makes release idempotent", () => {
    const guard = new CoverageRunGuard();
    const release = guard.acquire("Foundry Coverage (LCOV)");
    expect(release).toBeTypeOf("function");

    release?.();
    release?.();

    expect(guard.activeLabel).toBeUndefined();
    expect(guard.acquire("Hardhat Coverage (LCOV)")).toBeTypeOf("function");
  });
});

describe("TaskCompletionArbiter", () => {
  it("keeps a later non-zero process exit authoritative over task-end order", () => {
    vi.useFakeTimers();
    try {
      const settled: Array<number | undefined> = [];
      const arbiter = new TaskCompletionArbiter(
        (exitCode) => settled.push(exitCode),
        100
      );

      arbiter.taskEnded();
      arbiter.processEnded(7);
      vi.advanceTimersByTime(100);

      expect(settled).toEqual([7]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("uses task-end only as a delayed no-process fallback", () => {
    vi.useFakeTimers();
    try {
      const settled: Array<number | undefined> = [];
      const arbiter = new TaskCompletionArbiter(
        (exitCode) => settled.push(exitCode),
        100
      );

      arbiter.taskEnded();
      expect(settled).toEqual([]);
      vi.advanceTimersByTime(100);
      expect(settled).toEqual([undefined]);
    } finally {
      vi.useRealTimers();
    }
  });
});
