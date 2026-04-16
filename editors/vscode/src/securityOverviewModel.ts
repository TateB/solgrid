import * as path from "node:path";
import { fileURLToPath } from "node:url";

export type FindingSeverity = "error" | "warning" | "info";
export type FindingKind = "compiler" | "lint" | "detector";
export type FindingConfidence = "high" | "medium" | "low";
export type SecurityOverviewGroupMode =
  | "file"
  | "severity"
  | "confidence"
  | "finding";
export type SecurityOverviewFilterMode =
  | "security"
  | "all"
  | "compiler"
  | "detector";

export interface PositionLike {
  line: number;
  character: number;
}

export interface RangeLike {
  start: PositionLike;
  end: PositionLike;
}

export interface DiagnosticLike {
  range: RangeLike;
  severity?: number;
  code?: string | number;
  source?: string;
  message: string;
  data?: unknown;
}

export interface PublishDiagnosticsParamsLike {
  uri: string;
  version?: number;
  diagnostics: DiagnosticLike[];
}

export interface CodeActionDiagnosticLike {
  code?:
    | string
    | number
    | {
        value: string | number;
        target?: unknown;
      };
  range?: RangeLike;
}

export interface CodeActionLike {
  title?: string;
  isPreferred?: boolean;
  diagnostics?: readonly CodeActionDiagnosticLike[];
}

export interface FindingMetaLike {
  id: string;
  title: string;
  category: string;
  severity: FindingSeverity;
  kind: FindingKind;
  confidence?: FindingConfidence;
  helpUrl?: string;
  suppressible: boolean;
  hasFix: boolean;
}

export interface SecurityFinding {
  uri: string;
  version?: number;
  code: string;
  message: string;
  source: string;
  range: RangeLike;
  meta: FindingMetaLike;
}

export interface SecurityOverviewFindingNode {
  kind: "finding";
  key: string;
  label: string;
  description: string;
  ignored: boolean;
  finding: SecurityFinding;
}

export interface SecurityOverviewGroupNode {
  kind: "group";
  key: string;
  label: string;
  description: string;
  children: SecurityOverviewFindingNode[];
}

export function extractSecurityFindings(
  params: PublishDiagnosticsParamsLike
): SecurityFinding[] {
  return params.diagnostics
    .filter((diagnostic) => diagnostic.source === "solgrid")
    .map((diagnostic) => {
      const code =
        typeof diagnostic.code === "string"
          ? diagnostic.code
          : typeof diagnostic.code === "number"
            ? String(diagnostic.code)
            : "unknown";

      return {
        uri: params.uri,
        version: params.version,
        code,
        message: diagnostic.message,
        source: diagnostic.source ?? "solgrid",
        range: diagnostic.range,
        meta: parseFindingMeta(diagnostic, code),
      };
    });
}

export function filterFindings(
  findings: readonly SecurityFinding[],
  filterMode: SecurityOverviewFilterMode
): SecurityFinding[] {
  switch (filterMode) {
    case "all":
      return [...findings];
    case "compiler":
      return findings.filter((finding) => finding.meta.kind === "compiler");
    case "detector":
      return findings.filter((finding) => finding.meta.kind === "detector");
    case "security":
      return findings.filter((finding) => finding.meta.kind !== "lint");
  }
}

export function buildOverviewTree(
  findings: readonly SecurityFinding[],
  groupMode: SecurityOverviewGroupMode,
  filterMode: SecurityOverviewFilterMode,
  ignoredFindingKeys: ReadonlySet<string> = EMPTY_FINDING_KEYS,
  showIgnoredBaselines = false
): SecurityOverviewGroupNode[] {
  const filtered = filterVisibleFindings(
    findings,
    filterMode,
    ignoredFindingKeys,
    showIgnoredBaselines
  ).sort(compareFindings);
  const groups = new Map<
    string,
    { label: string; description: string; findings: SecurityFinding[] }
  >();

  for (const finding of filtered) {
    const { key, label, description } = groupDescriptor(finding, groupMode);
    const bucket = groups.get(key);
    if (bucket) {
      bucket.findings.push(finding);
    } else {
      groups.set(key, {
        label,
        description,
        findings: [finding],
      });
    }
  }

  return Array.from(groups.entries())
    .sort((left, right) =>
      compareGroups(groupMode, left[1].findings[0], right[1].findings[0])
    )
    .map(([key, group]) => ({
      kind: "group",
      key,
      label: group.label,
      description:
        groupMode === "file"
          ? `${group.findings.length} findings • ${group.description}`
          : `${group.description} • ${group.findings.length} findings`,
      children: group.findings.map((finding) => ({
        kind: "finding",
        key: findingFingerprint(finding),
        label:
          finding.meta.kind === "compiler"
            ? finding.message
            : finding.meta.title || finding.message,
        description: `${fileLabel(finding.uri)}:${finding.range.start.line + 1} • ${finding.code}`,
        ignored: ignoredFindingKeys.has(findingFingerprint(finding)),
        finding,
      })),
    }));
}

export function summarizeOverview(
  findings: readonly SecurityFinding[],
  groupMode: SecurityOverviewGroupMode,
  filterMode: SecurityOverviewFilterMode,
  ignoredFindingKeys: ReadonlySet<string> = EMPTY_FINDING_KEYS,
  showIgnoredBaselines = false
): { count: number; description: string; message: string | undefined } {
  const filtered = filterFindings(findings, filterMode);
  const ignoredCount = filtered.filter((finding) =>
    ignoredFindingKeys.has(findingFingerprint(finding))
  ).length;
  const visible = filterVisibleFindings(
    findings,
    filterMode,
    ignoredFindingKeys,
    showIgnoredBaselines
  );
  const count = visible.length;
  const ignoredDescription =
    ignoredCount === 0
      ? null
      : showIgnoredBaselines
        ? `${ignoredCount} ignored shown`
        : `${ignoredCount} ignored hidden`;
  const description = [
    `${filterModeLabel(filterMode)} • by ${groupModeLabel(groupMode)}`,
    ignoredDescription,
  ]
    .filter((part): part is string => part !== null)
    .join(" • ");
  const message =
    count === 0
      ? noFindingsMessage(filterMode, ignoredCount, showIgnoredBaselines)
      : undefined;
  return { count, description, message };
}

export function groupContextValue(
  findings: readonly SecurityFinding[],
  ignoredFindingKeys: ReadonlySet<string> = EMPTY_FINDING_KEYS
): string {
  const active = findings.filter(
    (finding) => !ignoredFindingKeys.has(findingFingerprint(finding))
  );
  const ignored = findings.filter((finding) =>
    ignoredFindingKeys.has(findingFingerprint(finding))
  );
  const tokens = ["solgridSecurityGroup"];
  if (active.length > 0) {
    tokens.push("ignorable");
  }
  if (ignored.length > 0) {
    tokens.push("restorable");
  }
  if (active.some((finding) => finding.meta.suppressible)) {
    tokens.push("suppressible");
  }
  if (active.some((finding) => finding.meta.hasFix)) {
    tokens.push("fixable");
  }
  return tokens.join(" ");
}

export function collectSuppressibleGroupFindings(
  findings: readonly SecurityFinding[]
): SecurityFinding[] {
  const deduped = new Map<string, SecurityFinding>();
  for (const finding of findings) {
    if (!finding.meta.suppressible) {
      continue;
    }
    deduped.set(
      `${finding.uri}:${finding.range.start.line}:${finding.meta.id}`,
      finding
    );
  }
  return Array.from(deduped.values()).sort(compareFixableOrSuppressibleFindings);
}

export function collectFixableGroupFindings(
  findings: readonly SecurityFinding[]
): SecurityFinding[] {
  return findings
    .filter((finding) => finding.meta.hasFix)
    .sort(compareFixableOrSuppressibleFindings);
}

export function collectIgnorableGroupFindings(
  findings: readonly SecurityFinding[],
  ignoredFindingKeys: ReadonlySet<string>
): SecurityFinding[] {
  return findings
    .filter((finding) => !ignoredFindingKeys.has(findingFingerprint(finding)))
    .sort(compareFixableOrSuppressibleFindings);
}

export function collectRestorableGroupFindings(
  findings: readonly SecurityFinding[],
  ignoredFindingKeys: ReadonlySet<string>
): SecurityFinding[] {
  return findings
    .filter((finding) => ignoredFindingKeys.has(findingFingerprint(finding)))
    .sort(compareFixableOrSuppressibleFindings);
}

export function buildSuppressNextLineDirective(
  ruleId: string,
  lineText: string
): string {
  const indentation = lineText.match(/^\s*/)?.[0] ?? "";
  return `${indentation}// solgrid-disable-next-line ${ruleId}\n`;
}

export function pickPreferredCodeActionForFinding<T extends CodeActionLike>(
  finding: SecurityFinding,
  actions: readonly T[]
): T | undefined {
  const applicable = actions.filter((action) =>
    action.diagnostics?.some((diagnostic) =>
      diagnosticMatchesFinding(finding, diagnostic)
    )
  );
  return applicable.find((action) => action.isPreferred) ?? applicable[0];
}

function parseFindingMeta(
  diagnostic: DiagnosticLike,
  code: string
): FindingMetaLike {
  const data = isFindingMetaData(diagnostic.data) ? diagnostic.data : null;
  const inferredSeverity = severityFromDiagnostic(diagnostic.severity);
  const category = data?.category ?? inferCategory(code);
  const kind = data?.kind ?? inferKind(category);

  return {
    id: data?.id ?? code,
    title: data?.title ?? diagnostic.message,
    category,
    severity: data?.severity ?? inferredSeverity,
    kind,
    confidence: data?.confidence,
    helpUrl: data?.helpUrl ?? data?.help_url,
    suppressible: data?.suppressible ?? kind !== "compiler",
    hasFix: data?.hasFix ?? data?.has_fix ?? false,
  };
}

function isFindingMetaData(value: unknown): value is {
  id: string;
  title: string;
  category: string;
  severity: FindingSeverity;
  kind: FindingKind;
  confidence?: FindingConfidence;
  help_url?: string;
  helpUrl?: string;
  suppressible: boolean;
  has_fix?: boolean;
  hasFix?: boolean;
} {
  return typeof value === "object" && value !== null;
}

function inferCategory(code: string): string {
  const slash = code.indexOf("/");
  return slash === -1 ? "unknown" : code.slice(0, slash);
}

function inferKind(category: string): FindingKind {
  if (category === "compiler") {
    return "compiler";
  }
  if (
    category === "security" ||
    category === "best-practices" ||
    category === "docs"
  ) {
    return "detector";
  }
  return "lint";
}

function severityFromDiagnostic(severity?: number): FindingSeverity {
  switch (severity) {
    case 1:
      return "error";
    case 2:
      return "warning";
    default:
      return "info";
  }
}

function diagnosticMatchesFinding(
  finding: SecurityFinding,
  diagnostic: CodeActionDiagnosticLike
): boolean {
  const code =
    typeof diagnostic.code === "string"
      ? diagnostic.code
      : typeof diagnostic.code === "number"
        ? String(diagnostic.code)
        : typeof diagnostic.code === "object" &&
            diagnostic.code !== null &&
            ("value" in diagnostic.code)
          ? String(diagnostic.code.value)
        : undefined;

  return (
    code === finding.code &&
    rangesEqual(diagnostic.range, finding.range)
  );
}

function rangesEqual(left?: RangeLike, right?: RangeLike): boolean {
  if (!left || !right) {
    return false;
  }
  return (
    left.start.line === right.start.line &&
    left.start.character === right.start.character &&
    left.end.line === right.end.line &&
    left.end.character === right.end.character
  );
}

function groupDescriptor(
  finding: SecurityFinding,
  groupMode: SecurityOverviewGroupMode
): { key: string; label: string; description: string } {
  switch (groupMode) {
    case "file":
      return {
        key: finding.uri,
        label: fileLabel(finding.uri),
        description: filePathLabel(finding.uri),
      };
    case "severity":
      return {
        key: finding.meta.severity,
        label: titleCase(finding.meta.severity),
        description: `${titleCase(finding.meta.severity)} severity`,
      };
    case "confidence":
      return {
        key: finding.meta.confidence ?? "unknown",
        label: titleCase(finding.meta.confidence ?? "unknown"),
        description: `${titleCase(finding.meta.confidence ?? "unknown")} confidence`,
      };
    case "finding":
      return {
        key: finding.meta.id,
        label: finding.meta.id,
        description: finding.meta.title,
      };
  }
}

function compareGroups(
  groupMode: SecurityOverviewGroupMode,
  left: SecurityFinding,
  right: SecurityFinding
): number {
  switch (groupMode) {
    case "file":
      return filePathLabel(left.uri).localeCompare(filePathLabel(right.uri));
    case "severity":
      return (
        severityRank(left.meta.severity) - severityRank(right.meta.severity)
      );
    case "confidence":
      return (
        confidenceRank(left.meta.confidence) -
        confidenceRank(right.meta.confidence)
      );
    case "finding":
      return left.meta.id.localeCompare(right.meta.id);
  }
}

function compareFindings(left: SecurityFinding, right: SecurityFinding): number {
  return (
    severityRank(left.meta.severity) - severityRank(right.meta.severity) ||
    filePathLabel(left.uri).localeCompare(filePathLabel(right.uri)) ||
    left.range.start.line - right.range.start.line ||
    left.range.start.character - right.range.start.character ||
    left.meta.id.localeCompare(right.meta.id)
  );
}

function severityRank(severity: FindingSeverity): number {
  switch (severity) {
    case "error":
      return 0;
    case "warning":
      return 1;
    case "info":
      return 2;
  }
}

function confidenceRank(confidence?: FindingConfidence): number {
  switch (confidence) {
    case "high":
      return 0;
    case "medium":
      return 1;
    case "low":
      return 2;
    default:
      return 3;
  }
}

function titleCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

export function findingFingerprint(finding: SecurityFinding): string {
  return `${finding.uri}:${finding.range.start.line}:${finding.range.start.character}:${finding.code}:${finding.message}`;
}

const EMPTY_FINDING_KEYS = new Set<string>();

function filterVisibleFindings(
  findings: readonly SecurityFinding[],
  filterMode: SecurityOverviewFilterMode,
  ignoredFindingKeys: ReadonlySet<string>,
  showIgnoredBaselines: boolean
): SecurityFinding[] {
  const filtered = filterFindings(findings, filterMode);
  if (showIgnoredBaselines || ignoredFindingKeys.size === 0) {
    return filtered;
  }
  return filtered.filter(
    (finding) => !ignoredFindingKeys.has(findingFingerprint(finding))
  );
}

function compareFixableOrSuppressibleFindings(
  left: SecurityFinding,
  right: SecurityFinding
): number {
  return (
    left.uri.localeCompare(right.uri) ||
    right.range.start.line - left.range.start.line ||
    right.range.start.character - left.range.start.character ||
    left.meta.id.localeCompare(right.meta.id)
  );
}

function fileLabel(uri: string): string {
  return path.basename(filePathLabel(uri));
}

function filePathLabel(uri: string): string {
  try {
    return fileURLToPath(uri);
  } catch {
    return uri;
  }
}

function groupModeLabel(mode: SecurityOverviewGroupMode): string {
  switch (mode) {
    case "file":
      return "file";
    case "severity":
      return "severity";
    case "confidence":
      return "confidence";
    case "finding":
      return "finding";
  }
}

function filterModeLabel(mode: SecurityOverviewFilterMode): string {
  switch (mode) {
    case "all":
      return "All findings";
    case "compiler":
      return "Compiler only";
    case "detector":
      return "Detectors only";
    case "security":
      return "Security focus";
  }
}

function noFindingsMessage(
  mode: SecurityOverviewFilterMode,
  ignoredCount: number,
  showIgnoredBaselines: boolean
): string {
  if (!showIgnoredBaselines && ignoredCount > 0) {
    return `No visible findings in the current workspace. ${ignoredCount} ignored baseline${ignoredCount === 1 ? "" : "s"} ${ignoredCount === 1 ? "is" : "are"} hidden.`;
  }
  switch (mode) {
    case "all":
      return "No solgrid findings in the current workspace.";
    case "compiler":
      return "No compiler diagnostics in the current workspace.";
    case "detector":
      return "No detector findings in the current workspace.";
    case "security":
      return "No compiler or detector findings in the current workspace.";
  }
}
