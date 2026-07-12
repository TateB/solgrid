import { beforeEach, describe, expect, it, vi } from "vitest";

const vscodeHarness = vi.hoisted(() => {
  const workspaceEditInsert = vi.fn();
  const workspaceEditReplace = vi.fn();
  const workspace = {
    applyEdit: vi.fn(async () => true),
    openTextDocument: vi.fn(async () => ({
      lineCount: 10,
      lineAt: (line: number) => ({
        text: line === 0 ? "  tx.origin;" : "  uint value;",
        range: {},
      }),
    })),
  };
  const languages = {
    getDiagnostics: vi.fn(() => [] as unknown[]),
  };

  class Position {
    constructor(
      readonly line: number,
      readonly character: number
    ) {}
  }

  class Range {
    readonly start: Position;
    readonly end: Position;

    constructor(start: Position, end: Position) {
      this.start = start;
      this.end = end;
    }

    isEqual(other: Range): boolean {
      return (
        this.start.line === other.start.line &&
        this.start.character === other.start.character &&
        this.end.line === other.end.line &&
        this.end.character === other.end.character
      );
    }
  }

  const module = {
    EventEmitter: class {
      readonly event = vi.fn();
      readonly fire = vi.fn();
    },
    Position,
    Range,
    Uri: {
      parse: (value: string) => ({ value }),
    },
    WorkspaceEdit: class {
      insert(...args: unknown[]): void {
        workspaceEditInsert(...args);
      }

      replace(...args: unknown[]): void {
        workspaceEditReplace(...args);
      }
    },
    languages,
    workspace,
  };

  return {
    languages,
    module,
    workspace,
    workspaceEditInsert,
    workspaceEditReplace,
  };
});

vi.mock("vscode", () => vscodeHarness.module);

import {
  SECURITY_ANALYSIS_PENDING_MESSAGE,
  SecurityOverviewProvider,
  applyGroupFixes,
  clearIgnoredBaselinesWithConfirmation,
  securityFindingAccessibilityLabel,
  securityGroupAccessibilityLabel,
  securityOverviewPresentationMessage,
  setSecurityCodeActionResolver,
  suppressGroupNextLine,
} from "./securityOverview";
import {
  buildOverviewTree,
  extractSecurityFindings,
  findingFingerprint,
  type SecurityFinding,
} from "./securityOverviewModel";

beforeEach(() => {
  setSecurityCodeActionResolver(undefined);
  vscodeHarness.languages.getDiagnostics.mockReset();
  vscodeHarness.workspace.applyEdit.mockClear();
  vscodeHarness.workspace.openTextDocument.mockClear();
  vscodeHarness.workspaceEditInsert.mockClear();
  vscodeHarness.workspaceEditReplace.mockClear();
});

describe("security overview accessibility labels", () => {
  const findings = extractSecurityFindings({
    uri: "file:///workspace/contracts/Vault.sol",
    diagnostics: [
      {
        range: {
          start: { line: 4, character: 8 },
          end: { line: 4, character: 17 },
        },
        severity: 1,
        code: "security/tx-origin",
        source: "solgrid",
        message: "Avoid using tx.origin",
        data: {
          id: "security/tx-origin",
          title: "Avoid using tx.origin for authorization",
          category: "security",
          severity: "error",
          kind: "detector",
          confidence: "high",
          suppressible: true,
          has_fix: false,
        },
      },
      {
        range: {
          start: { line: 8, character: 4 },
          end: { line: 8, character: 8 },
        },
        severity: 2,
        code: "style/use-uint256",
        source: "solgrid",
        message: "Use uint256",
        data: {
          id: "style/use-uint256",
          title: "Use uint256",
          category: "style",
          severity: "warning",
          kind: "lint",
          confidence: "medium",
          suppressible: true,
          has_fix: true,
        },
      },
    ],
  });

  it("includes severity, confidence, type, path, state, and fixability", () => {
    const [finding] = findings;
    if (!finding) {
      throw new Error("expected a security finding fixture");
    }
    const label = securityFindingAccessibilityLabel(finding, true);

    expect(label).toContain("error severity");
    expect(label).toContain("high confidence");
    expect(label).toContain("detector type");
    expect(label).toContain("location /workspace/contracts/Vault.sol:5:9");
    expect(label).toContain("ignored");
    expect(label).toContain("no automatic fix");
    expect(label).toContain("can be suppressed");
  });

  it("aggregates finding state and fixability for groups", () => {
    const [finding] = findings;
    if (!finding) {
      throw new Error("expected a security finding fixture");
    }
    const ignored = new Set([findingFingerprint(finding)]);
    const [group] = buildOverviewTree(findings, "file", "all", ignored, true);
    if (!group) {
      throw new Error("expected a security overview group");
    }
    const label = securityGroupAccessibilityLabel(group);

    expect(label).toContain("2 findings");
    expect(label).toContain("severity: 1 error finding");
    expect(label).toContain("1 warning finding");
    expect(label).toContain("confidence: 1 high finding");
    expect(label).toContain("1 medium finding");
    expect(label).toContain("type: 1 detector finding");
    expect(label).toContain("1 lint finding");
    expect(label).toContain("locations /workspace/contracts/Vault.sol");
    expect(label).toContain("1 ignored finding");
    expect(label).toContain("1 active automatic fix");
  });
});

describe("security overview analysis state", () => {
  it("keeps the empty state pending until analysis explicitly completes", () => {
    const provider = new SecurityOverviewProvider({
      get: (_key: string, defaultValue: unknown) => defaultValue,
      update: vi.fn(),
    } as never);
    const view: {
      description?: string;
      message?: string;
      badge?: { value: number; tooltip: string };
    } = {};

    provider.attachView(view as never);
    expect(view.message).toBe(SECURITY_ANALYSIS_PENDING_MESSAGE);

    provider.completeAnalysis();
    expect(view.message).toBe(
      "No compiler or detector findings in the current workspace."
    );

    provider.beginAnalysis();
    expect(view.message).toBe(SECURITY_ANALYSIS_PENDING_MESSAGE);

    provider.updateFromDiagnostics({
      uri: "file:///workspace/contracts/Vault.sol",
      diagnostics: [],
    });
    expect(view.message).toBe(SECURITY_ANALYSIS_PENDING_MESSAGE);

    provider.completeAnalysis();
    expect(view.message).toBe(
      "No compiler or detector findings in the current workspace."
    );

    provider.failAnalysis("Security analysis failed again.");
    provider.beginAnalysis();
    expect(view.message).toBe(SECURITY_ANALYSIS_PENDING_MESSAGE);
  });

  it("lets diagnostics complete analysis when no explicit run is active", () => {
    const provider = new SecurityOverviewProvider({
      get: (_key: string, defaultValue: unknown) => defaultValue,
      update: vi.fn(),
    } as never);
    const view: { message?: string } = {};
    provider.attachView(view as never);

    provider.updateFromDiagnostics({
      uri: "file:///workspace/contracts/Vault.sol",
      diagnostics: [],
    });

    expect(view.message).toBe(
      "No compiler or detector findings in the current workspace."
    );
  });

  it("shows a persistent failure instead of leaving analysis pending", () => {
    const provider = new SecurityOverviewProvider({
      get: (_key: string, defaultValue: unknown) => defaultValue,
      update: vi.fn(),
    } as never);
    const view: { message?: string } = {};
    provider.attachView(view as never);
    provider.beginAnalysis();

    provider.failAnalysis("Security analysis could not reach the language server.");

    expect(view.message).toBe(
      "Security analysis could not reach the language server."
    );

    provider.updateFromDiagnostics({
      uri: "file:///workspace/contracts/Vault.sol",
      diagnostics: [],
    });
    expect(view.message).toBe(
      "Security analysis could not reach the language server."
    );

    provider.completeAnalysis();
    expect(view.message).toBe(
      "No compiler or detector findings in the current workspace."
    );
  });

  it("lets pending analysis override only the premature summary message", () => {
    expect(
      securityOverviewPresentationMessage("No findings.", false)
    ).toBe(SECURITY_ANALYSIS_PENDING_MESSAGE);
    expect(securityOverviewPresentationMessage("No findings.", true)).toBe(
      "No findings."
    );
    expect(
      securityOverviewPresentationMessage(
        "No findings.",
        false,
        "Security analysis failed."
      )
    ).toBe("Security analysis failed.");
  });
});

describe("ignored baseline clearing", () => {
  it("leaves ignored baselines intact when confirmation is cancelled", async () => {
    const clearIgnoredBaselines = vi.fn(async () => undefined);

    const cleared = await clearIgnoredBaselinesWithConfirmation(
      { clearIgnoredBaselines },
      async () => false
    );

    expect(cleared).toBe(false);
    expect(clearIgnoredBaselines).not.toHaveBeenCalled();
  });

  it("clears ignored baselines after explicit confirmation", async () => {
    const clearIgnoredBaselines = vi.fn(async () => undefined);

    const cleared = await clearIgnoredBaselinesWithConfirmation(
      { clearIgnoredBaselines },
      async () => true
    );

    expect(cleared).toBe(true);
    expect(clearIgnoredBaselines).toHaveBeenCalledOnce();
  });
});

describe("security overview group actions", () => {
  function mixedActiveAndIgnoredGroup() {
    const findings = extractSecurityFindings({
      uri: "file:///workspace/contracts/Vault.sol",
      diagnostics: [
        {
          range: {
            start: { line: 0, character: 2 },
            end: { line: 0, character: 11 },
          },
          severity: 1,
          code: "security/active-finding",
          source: "solgrid",
          message: "Active finding",
          data: {
            id: "security/active-finding",
            title: "Active finding",
            category: "security",
            severity: "error",
            kind: "detector",
            suppressible: true,
            has_fix: true,
          },
        },
        {
          range: {
            start: { line: 1, character: 2 },
            end: { line: 1, character: 7 },
          },
          severity: 1,
          code: "security/ignored-finding",
          source: "solgrid",
          message: "Ignored finding",
          data: {
            id: "security/ignored-finding",
            title: "Ignored finding",
            category: "security",
            severity: "error",
            kind: "detector",
            suppressible: true,
            has_fix: true,
          },
        },
      ],
    });
    const ignoredFinding = findings.find(
      (finding) => finding.code === "security/ignored-finding"
    );
    if (!ignoredFinding) {
      throw new Error("expected ignored finding fixture");
    }
    const ignored = new Set([findingFingerprint(ignoredFinding)]);
    const [group] = buildOverviewTree(findings, "file", "all", ignored, true);
    if (!group) {
      throw new Error("expected mixed security group fixture");
    }
    return { findings, group };
  }

  it("does not suppress ignored children through the group action", async () => {
    const { findings, group } = mixedActiveAndIgnoredGroup();
    vscodeHarness.languages.getDiagnostics.mockReturnValue(
      findings.map((finding) => ({
        source: finding.source,
        code: finding.code,
        range: finding.range,
      }))
    );

    await suppressGroupNextLine(group);

    expect(vscodeHarness.workspaceEditInsert).toHaveBeenCalledOnce();
    const directive = vscodeHarness.workspaceEditInsert.mock.calls[0]?.[2];
    expect(directive).toContain("security/active-finding");
    expect(directive).not.toContain("security/ignored-finding");
    expect(vscodeHarness.workspace.applyEdit).toHaveBeenCalledOnce();
  });

  it("does not resolve or apply fixes for ignored children through the group action", async () => {
    const { group } = mixedActiveAndIgnoredGroup();
    const resolver = vi.fn(async (_finding: SecurityFinding) => []);
    setSecurityCodeActionResolver(resolver);

    await applyGroupFixes(group);

    expect(resolver).toHaveBeenCalledOnce();
    expect(resolver.mock.calls[0]?.[0].code).toBe("security/active-finding");
  });
});
