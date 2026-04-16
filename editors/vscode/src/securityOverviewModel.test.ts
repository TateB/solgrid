import { describe, expect, it } from "vitest";
import {
  buildOverviewTree,
  collectFixableGroupFindings,
  collectIgnorableGroupFindings,
  collectRestorableGroupFindings,
  collectSuppressibleGroupFindings,
  buildSuppressNextLineDirective,
  findingFingerprint,
  extractSecurityFindings,
  groupContextValue,
  pickPreferredCodeActionForFinding,
  summarizeOverview,
} from "./securityOverviewModel";

describe("extractSecurityFindings", () => {
  it("uses normalized metadata from diagnostic data", () => {
    const findings = extractSecurityFindings({
      uri: "file:///workspace/Test.sol",
      diagnostics: [
        {
          range: {
            start: { line: 1, character: 4 },
            end: { line: 1, character: 14 },
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
            help_url: "https://example.test/rule",
            suppressible: true,
            has_fix: false,
          },
        },
      ],
    });

    expect(findings).toHaveLength(1);
    expect(findings[0].meta).toMatchObject({
      id: "security/tx-origin",
      kind: "detector",
      confidence: "high",
      helpUrl: "https://example.test/rule",
      suppressible: true,
      hasFix: false,
    });
  });

  it("falls back to inferred compiler metadata when data is missing", () => {
    const findings = extractSecurityFindings({
      uri: "file:///workspace/Test.sol",
      diagnostics: [
        {
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 10 },
          },
          severity: 1,
          code: "compiler/unresolved-type",
          source: "solgrid",
          message: 'cannot resolve type "MissingType"',
        },
      ],
    });

    expect(findings).toHaveLength(1);
    expect(findings[0].meta).toMatchObject({
      id: "compiler/unresolved-type",
      category: "compiler",
      kind: "compiler",
      severity: "error",
      suppressible: false,
    });
  });
});

describe("buildOverviewTree", () => {
  const findings = extractSecurityFindings({
    uri: "file:///workspace/A.sol",
    diagnostics: [
      {
        range: {
          start: { line: 1, character: 0 },
          end: { line: 1, character: 12 },
        },
        severity: 1,
        code: "compiler/unresolved-type",
        source: "solgrid",
        message: 'cannot resolve type "MissingType"',
      },
      {
        range: {
          start: { line: 3, character: 0 },
          end: { line: 3, character: 10 },
        },
        severity: 2,
        code: "style/max-line-length",
        source: "solgrid",
        message: "line is too long",
      },
      {
        range: {
          start: { line: 5, character: 0 },
          end: { line: 5, character: 12 },
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
    ],
  });

  it("applies the security focus filter by defaulting out lint findings", () => {
    const groups = buildOverviewTree(findings, "severity", "security");

    expect(groups).toHaveLength(1);
    expect(groups[0].label).toBe("Error");
    expect(groups[0].children).toHaveLength(2);
    expect(groups[0].children.every((child) => child.finding.meta.kind !== "lint")).toBe(
      true
    );
  });

  it("groups by finding id with stable labels", () => {
    const groups = buildOverviewTree(findings, "finding", "all");

    expect(groups.map((group) => group.label)).toEqual([
      "compiler/unresolved-type",
      "security/tx-origin",
      "style/max-line-length",
    ]);
    expect(groups[1].children[0].description).toContain("A.sol:6");
  });
});

describe("summarizeOverview", () => {
  it("returns a clear empty-state message for security focus", () => {
    const summary = summarizeOverview([], "file", "security");
    expect(summary.count).toBe(0);
    expect(summary.description).toBe("Security focus • by file");
    expect(summary.message).toBe(
      "No compiler or detector findings in the current workspace."
    );
  });

  it("mentions hidden ignored baselines when nothing visible remains", () => {
    const findings = extractSecurityFindings({
      uri: "file:///workspace/Test.sol",
      diagnostics: [
        {
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 4 },
          },
          severity: 1,
          code: "security/tx-origin",
          source: "solgrid",
          message: "Avoid using tx.origin",
          data: {
            id: "security/tx-origin",
            title: "Avoid using tx.origin",
            category: "security",
            severity: "error",
            kind: "detector",
            suppressible: true,
            has_fix: false,
          },
        },
      ],
    });

    const ignored = new Set(findings.map((finding) => findingFingerprint(finding)));
    const summary = summarizeOverview(findings, "file", "security", ignored, false);

    expect(summary.count).toBe(0);
    expect(summary.message).toBe(
      "No visible findings in the current workspace. 1 ignored baseline is hidden."
    );
  });
});

describe("buildSuppressNextLineDirective", () => {
  it("preserves line indentation when building a suppression comment", () => {
    const directive = buildSuppressNextLineDirective(
      "security/unchecked-low-level-call",
      "        target.call(payload);"
    );

    expect(directive).toBe(
      "        // solgrid-disable-next-line security/unchecked-low-level-call\n"
    );
  });
});

describe("pickPreferredCodeActionForFinding", () => {
  it("prefers the matching preferred fix action", () => {
    const [finding] = extractSecurityFindings({
      uri: "file:///workspace/Test.sol",
      diagnostics: [
        {
          range: {
            start: { line: 4, character: 8 },
            end: { line: 4, character: 12 },
          },
          severity: 2,
          code: "style/use-uint256",
          source: "solgrid",
          message: "use uint256",
          data: {
            id: "style/use-uint256",
            title: "Use uint256",
            category: "style",
            severity: "info",
            kind: "lint",
            suppressible: true,
            has_fix: true,
          },
        },
      ],
    });

    const chosen = pickPreferredCodeActionForFinding(finding, [
      {
        title: "Unrelated fix",
        diagnostics: [
          {
            code: "security/tx-origin",
            range: finding.range,
          },
        ],
      },
      {
        title: "Preferred fix",
        isPreferred: true,
        diagnostics: [
          {
            code: "style/use-uint256",
            range: finding.range,
          },
        ],
      },
      {
        title: "Fallback fix",
        diagnostics: [
          {
            code: "style/use-uint256",
            range: finding.range,
          },
        ],
      },
    ]);

    expect(chosen?.title).toBe("Preferred fix");
  });
});

describe("groupContextValue", () => {
  it("adds fixable and suppressible suffixes based on child findings", () => {
    const findings = extractSecurityFindings({
      uri: "file:///workspace/Test.sol",
      diagnostics: [
        {
          range: {
            start: { line: 1, character: 0 },
            end: { line: 1, character: 4 },
          },
          severity: 2,
          code: "style/use-uint256",
          source: "solgrid",
          message: "use uint256",
          data: {
            id: "style/use-uint256",
            title: "Use uint256",
            category: "style",
            severity: "info",
            kind: "lint",
            suppressible: true,
            has_fix: true,
          },
        },
      ],
    });

    expect(groupContextValue(findings)).toBe(
      "solgridSecurityGroup ignorable suppressible fixable"
    );
  });

  it("adds restore tokens for ignored child findings", () => {
    const findings = extractSecurityFindings({
      uri: "file:///workspace/Test.sol",
      diagnostics: [
        {
          range: {
            start: { line: 1, character: 0 },
            end: { line: 1, character: 4 },
          },
          severity: 2,
          code: "style/use-uint256",
          source: "solgrid",
          message: "use uint256",
          data: {
            id: "style/use-uint256",
            title: "Use uint256",
            category: "style",
            severity: "info",
            kind: "lint",
            suppressible: true,
            has_fix: true,
          },
        },
      ],
    });

    const ignored = new Set(findings.map((finding) => findingFingerprint(finding)));
    expect(groupContextValue(findings, ignored)).toBe(
      "solgridSecurityGroup restorable"
    );
  });
});

describe("collectSuppressibleGroupFindings", () => {
  it("deduplicates same-line suppressions by rule id", () => {
    const findings = extractSecurityFindings({
      uri: "file:///workspace/Test.sol",
      diagnostics: [
        {
          range: {
            start: { line: 2, character: 4 },
            end: { line: 2, character: 8 },
          },
          severity: 1,
          code: "security/tx-origin",
          source: "solgrid",
          message: "Avoid using tx.origin",
          data: {
            id: "security/tx-origin",
            title: "Avoid using tx.origin",
            category: "security",
            severity: "error",
            kind: "detector",
            suppressible: true,
            has_fix: false,
          },
        },
        {
          range: {
            start: { line: 2, character: 12 },
            end: { line: 2, character: 18 },
          },
          severity: 1,
          code: "security/tx-origin",
          source: "solgrid",
          message: "Avoid using tx.origin again",
          data: {
            id: "security/tx-origin",
            title: "Avoid using tx.origin",
            category: "security",
            severity: "error",
            kind: "detector",
            suppressible: true,
            has_fix: false,
          },
        },
      ],
    });

    expect(collectSuppressibleGroupFindings(findings)).toHaveLength(1);
  });
});

describe("collectFixableGroupFindings", () => {
  it("returns only findings with fixes in descending line order per file", () => {
    const findings = extractSecurityFindings({
      uri: "file:///workspace/Test.sol",
      diagnostics: [
        {
          range: {
            start: { line: 1, character: 0 },
            end: { line: 1, character: 4 },
          },
          severity: 2,
          code: "style/use-uint256",
          source: "solgrid",
          message: "use uint256",
          data: {
            id: "style/use-uint256",
            title: "Use uint256",
            category: "style",
            severity: "info",
            kind: "lint",
            suppressible: true,
            has_fix: true,
          },
        },
        {
          range: {
            start: { line: 4, character: 0 },
            end: { line: 4, character: 4 },
          },
          severity: 2,
          code: "style/use-uint256",
          source: "solgrid",
          message: "use uint256 again",
          data: {
            id: "style/use-uint256",
            title: "Use uint256",
            category: "style",
            severity: "info",
            kind: "lint",
            suppressible: true,
            has_fix: true,
          },
        },
      ],
    });

    const fixable = collectFixableGroupFindings(findings);
    expect(fixable.map((finding) => finding.range.start.line)).toEqual([4, 1]);
  });
});

describe("ignored baseline helpers", () => {
  const findings = extractSecurityFindings({
    uri: "file:///workspace/Test.sol",
    diagnostics: [
      {
        range: {
          start: { line: 1, character: 0 },
          end: { line: 1, character: 4 },
        },
        severity: 1,
        code: "security/tx-origin",
        source: "solgrid",
        message: "Avoid using tx.origin",
        data: {
          id: "security/tx-origin",
          title: "Avoid using tx.origin",
          category: "security",
          severity: "error",
          kind: "detector",
          suppressible: true,
          has_fix: false,
        },
      },
      {
        range: {
          start: { line: 4, character: 0 },
          end: { line: 4, character: 4 },
        },
        severity: 2,
        code: "style/use-uint256",
        source: "solgrid",
        message: "use uint256",
        data: {
          id: "style/use-uint256",
          title: "Use uint256",
          category: "style",
          severity: "info",
          kind: "lint",
          suppressible: true,
          has_fix: true,
        },
      },
    ],
  });

  it("filters ignored findings from the tree by default", () => {
    const ignored = new Set([findingFingerprint(findings[0])]);
    const groups = buildOverviewTree(findings, "file", "all", ignored, false);

    expect(groups).toHaveLength(1);
    expect(groups[0].children).toHaveLength(1);
    expect(groups[0].children[0].finding.code).toBe("style/use-uint256");
    expect(groups[0].children[0].ignored).toBe(false);
  });

  it("shows ignored findings when requested", () => {
    const ignored = new Set([findingFingerprint(findings[0])]);
    const groups = buildOverviewTree(findings, "file", "all", ignored, true);

    expect(groups[0].children).toHaveLength(2);
    expect(groups[0].children[0].ignored).toBe(true);
  });

  it("collects ignorable and restorable group findings separately", () => {
    const ignored = new Set([findingFingerprint(findings[0])]);

    expect(collectIgnorableGroupFindings(findings, ignored)).toHaveLength(1);
    expect(collectIgnorableGroupFindings(findings, ignored)[0].code).toBe(
      "style/use-uint256"
    );
    expect(collectRestorableGroupFindings(findings, ignored)).toHaveLength(1);
    expect(collectRestorableGroupFindings(findings, ignored)[0].code).toBe(
      "security/tx-origin"
    );
  });

  it("uses a stable fingerprint when the diagnostic message changes", () => {
    const variants = extractSecurityFindings({
      uri: "file:///workspace/Test.sol",
      diagnostics: [
        {
          range: {
            start: { line: 1, character: 0 },
            end: { line: 1, character: 9 },
          },
          severity: 1,
          code: "security/tx-origin",
          source: "solgrid",
          message: "Avoid using tx.origin",
          data: {
            id: "security/tx-origin",
            title: "Avoid using tx.origin",
            category: "security",
            severity: "error",
            kind: "detector",
            suppressible: true,
            has_fix: false,
          },
        },
        {
          range: {
            start: { line: 1, character: 0 },
            end: { line: 1, character: 9 },
          },
          severity: 1,
          code: "security/tx-origin",
          source: "solgrid",
          message: "Avoid using tx.origin for authorization",
          data: {
            id: "security/tx-origin",
            title: "Avoid using tx.origin",
            category: "security",
            severity: "error",
            kind: "detector",
            suppressible: true,
            has_fix: false,
          },
        },
      ],
    });

    expect(findingFingerprint(variants[0])).toBe(findingFingerprint(variants[1]));
  });
});
