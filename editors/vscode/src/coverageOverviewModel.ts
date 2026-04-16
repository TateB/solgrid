import * as path from "node:path";

export type CoverageOverviewFilterMode = "actionable" | "all";
export type CoverageLineStatus = "uncovered" | "partial";

export interface CoverageArtifactRecord {
  filePath: string;
  artifactPath: string;
  lineHits: ReadonlyMap<number, number>;
  branchHits: ReadonlyMap<number, { found: number; hit: number }>;
}

export interface CoverageLineDetail {
  line: number;
  status: CoverageLineStatus;
  hits: number;
  branchesFound: number;
  branchesHit: number;
}

export interface CoverageFileSummary {
  filePath: string;
  displayPath: string;
  artifactPaths: string[];
  linesFound: number;
  linesHit: number;
  branchesFound: number;
  branchesHit: number;
  actionableLines: CoverageLineDetail[];
}

export interface CoverageWorkspaceSummary {
  artifactCount: number;
  files: CoverageFileSummary[];
}

export interface CoverageOverviewFileNode {
  kind: "file";
  key: string;
  label: string;
  description: string;
  summary: CoverageFileSummary;
  children: CoverageOverviewLineNode[];
}

export interface CoverageOverviewLineNode {
  kind: "line";
  key: string;
  label: string;
  description: string;
  filePath: string;
  detail: CoverageLineDetail;
}

export function parseCoverageArtifact(
  content: string,
  artifactPath: string,
  workspaceRoots: readonly string[]
): CoverageArtifactRecord[] {
  const extension = path.extname(artifactPath).toLowerCase();
  if (extension === ".xml") {
    return parseCoberturaArtifact(content, artifactPath, workspaceRoots);
  }
  return parseLcovArtifact(content, artifactPath, workspaceRoots);
}

export function parseLcovArtifact(
  content: string,
  artifactPath: string,
  workspaceRoots: readonly string[]
): CoverageArtifactRecord[] {
  const records: CoverageArtifactRecord[] = [];
  let current: {
    rawSourcePath: string;
    lineHits: Map<number, number>;
    branchHits: Map<number, { found: number; hit: number }>;
  } | null = null;

  const flush = (): void => {
    if (!current) {
      return;
    }
    const resolvedPath = resolveCoverageSourcePath(
      current.rawSourcePath,
      artifactPath,
      workspaceRoots
    );
    if (resolvedPath) {
      records.push({
        filePath: resolvedPath,
        artifactPath: normalizePath(artifactPath),
        lineHits: new Map(current.lineHits),
        branchHits: new Map(current.branchHits),
      });
    }
    current = null;
  };

  for (const rawLine of content.split(/\r?\n/u)) {
    if (rawLine.startsWith("SF:")) {
      flush();
      current = {
        rawSourcePath: rawLine.slice(3).trim(),
        lineHits: new Map(),
        branchHits: new Map(),
      };
      continue;
    }

    if (!current) {
      continue;
    }

    if (rawLine === "end_of_record") {
      flush();
      continue;
    }

    if (rawLine.startsWith("DA:")) {
      const [lineValue, hitValue] = rawLine.slice(3).split(",", 2);
      const line = Number.parseInt(lineValue ?? "", 10);
      const hits = Number.parseInt(hitValue ?? "", 10);
      if (Number.isInteger(line) && line > 0 && Number.isFinite(hits)) {
        current.lineHits.set(line, (current.lineHits.get(line) ?? 0) + hits);
      }
      continue;
    }

    if (rawLine.startsWith("BRDA:")) {
      const [lineValue, _block, _branch, takenValue] = rawLine
        .slice(5)
        .split(",", 4);
      const line = Number.parseInt(lineValue ?? "", 10);
      if (!Number.isInteger(line) || line <= 0) {
        continue;
      }
      const branch = current.branchHits.get(line) ?? { found: 0, hit: 0 };
      branch.found += 1;
      if (takenValue !== "-" && Number.parseInt(takenValue ?? "", 10) > 0) {
        branch.hit += 1;
      }
      current.branchHits.set(line, branch);
    }
  }

  flush();
  return records;
}

export function parseCoberturaArtifact(
  content: string,
  artifactPath: string,
  workspaceRoots: readonly string[]
): CoverageArtifactRecord[] {
  const records = new Map<
    string,
    {
      artifactPath: string;
      lineHits: Map<number, number>;
      branchHits: Map<number, { found: number; hit: number }>;
    }
  >();

  for (const classMatch of content.matchAll(/<class\b([^>]*)>([\s\S]*?)<\/class>/giu)) {
    const rawFilename = xmlAttr(classMatch[1] ?? "", "filename");
    if (!rawFilename) {
      continue;
    }
    const filePath = resolveCoverageSourcePath(
      rawFilename,
      artifactPath,
      workspaceRoots
    );
    if (!filePath) {
      continue;
    }

    const record = records.get(filePath) ?? {
      artifactPath: normalizePath(artifactPath),
      lineHits: new Map<number, number>(),
      branchHits: new Map<number, { found: number; hit: number }>(),
    };

    for (const lineMatch of (classMatch[2] ?? "").matchAll(/<line\b([^>]*)\/?>/giu)) {
      const lineNumber = parseCoverageInt(xmlAttr(lineMatch[1] ?? "", "number"));
      const hits = parseCoverageInt(xmlAttr(lineMatch[1] ?? "", "hits"));
      if (!Number.isInteger(lineNumber) || lineNumber <= 0 || !Number.isFinite(hits)) {
        continue;
      }
      record.lineHits.set(lineNumber, (record.lineHits.get(lineNumber) ?? 0) + hits);

      if (!xmlBoolAttr(lineMatch[1] ?? "", "branch")) {
        continue;
      }
      const branch = record.branchHits.get(lineNumber) ?? { found: 0, hit: 0 };
      const coverage = xmlAttr(lineMatch[1] ?? "", "condition-coverage");
      const counts = coverage?.match(/\((\d+)\s*\/\s*(\d+)\)/u);
      if (counts) {
        branch.hit += Number.parseInt(counts[1] ?? "0", 10);
        branch.found += Number.parseInt(counts[2] ?? "0", 10);
      } else {
        branch.found += 1;
        if (hits > 0) {
          branch.hit += 1;
        }
      }
      record.branchHits.set(lineNumber, branch);
    }

    records.set(filePath, record);
  }

  return Array.from(records.entries()).map(([filePath, record]) => ({
    filePath,
    artifactPath: record.artifactPath,
    lineHits: record.lineHits,
    branchHits: record.branchHits,
  }));
}

export function summarizeCoverageArtifacts(
  records: readonly CoverageArtifactRecord[],
  workspaceRoots: readonly string[]
): CoverageWorkspaceSummary {
  const files = new Map<
    string,
    {
      artifactPaths: Set<string>;
      lineHits: Map<number, number>;
      branchHits: Map<number, { found: number; hit: number }>;
    }
  >();

  for (const record of records) {
    if (path.extname(record.filePath) !== ".sol") {
      continue;
    }
    const bucket = files.get(record.filePath) ?? {
      artifactPaths: new Set<string>(),
      lineHits: new Map<number, number>(),
      branchHits: new Map<number, { found: number; hit: number }>(),
    };
    bucket.artifactPaths.add(record.artifactPath);
    for (const [line, hits] of record.lineHits) {
      bucket.lineHits.set(line, (bucket.lineHits.get(line) ?? 0) + hits);
    }
    for (const [line, branchHits] of record.branchHits) {
      const branch = bucket.branchHits.get(line) ?? { found: 0, hit: 0 };
      branch.found += branchHits.found;
      branch.hit += branchHits.hit;
      bucket.branchHits.set(line, branch);
    }
    files.set(record.filePath, bucket);
  }

  const summaries = Array.from(files.entries())
    .map(([filePath, bucket]): CoverageFileSummary => {
      const lineNumbers = Array.from(bucket.lineHits.keys()).sort((left, right) => left - right);
      const actionableLines = lineNumbers
        .map((line): CoverageLineDetail | null => {
          const hits = bucket.lineHits.get(line) ?? 0;
          const branch = bucket.branchHits.get(line) ?? { found: 0, hit: 0 };
          if (hits <= 0) {
            return {
              line,
              status: "uncovered",
              hits,
              branchesFound: branch.found,
              branchesHit: branch.hit,
            };
          }
          if (branch.found > 0 && branch.hit < branch.found) {
            return {
              line,
              status: "partial",
              hits,
              branchesFound: branch.found,
              branchesHit: branch.hit,
            };
          }
          return null;
        })
        .filter((detail): detail is CoverageLineDetail => detail !== null);

      const linesFound = lineNumbers.length;
      const linesHit = lineNumbers.filter((line) => (bucket.lineHits.get(line) ?? 0) > 0).length;
      const branches = Array.from(bucket.branchHits.values());
      const branchesFound = branches.reduce((sum, branch) => sum + branch.found, 0);
      const branchesHit = branches.reduce((sum, branch) => sum + branch.hit, 0);

      return {
        filePath,
        displayPath: displayPathForFile(filePath, workspaceRoots),
        artifactPaths: Array.from(bucket.artifactPaths).sort(),
        linesFound,
        linesHit,
        branchesFound,
        branchesHit,
        actionableLines,
      };
    })
    .sort(compareCoverageFiles);

  return {
    artifactCount: new Set(records.map((record) => record.artifactPath)).size,
    files: summaries,
  };
}

export function buildCoverageTree(
  summary: CoverageWorkspaceSummary | undefined,
  filterMode: CoverageOverviewFilterMode
): CoverageOverviewFileNode[] {
  if (!summary) {
    return [];
  }

  return summary.files
    .filter((file) => filterMode === "all" || file.actionableLines.length > 0)
    .map((file) => ({
      kind: "file",
      key: file.filePath,
      label: file.displayPath,
      description: fileDescription(file),
      summary: file,
      children: file.actionableLines.map((detail) => ({
        kind: "line",
        key: `${file.filePath}:${detail.line}:${detail.status}`,
        label: `Line ${detail.line}`,
        description: lineDescription(detail),
        filePath: file.filePath,
        detail,
      })),
    }));
}

export function summarizeCoverageOverview(
  summary: CoverageWorkspaceSummary | undefined,
  filterMode: CoverageOverviewFilterMode
): { count: number; description: string; message: string | undefined } {
  if (!summary) {
    return {
      count: 0,
      description: `${filterModeLabel(filterMode)} • 0 artifacts`,
      message:
        "No supported coverage artifacts found. Generate LCOV or Cobertura coverage and refresh.",
    };
  }

  if (summary.files.length === 0) {
    return {
      count: 0,
      description: `${filterModeLabel(filterMode)} • ${summary.artifactCount} artifacts`,
      message:
        "Coverage artifacts were found, but none mapped to Solidity source files in this workspace.",
    };
  }

  const visibleFiles =
    filterMode === "all"
      ? summary.files
      : summary.files.filter((file) => file.actionableLines.length > 0);
  const actionableLines = visibleFiles.reduce(
    (sum, file) => sum + file.actionableLines.length,
    0
  );
  const totals = summarizeWorkspacePercentages(summary.files);
  return {
    count: actionableLines,
    description: `${filterModeLabel(filterMode)} • ${formatPercent(
      totals.linesHit,
      totals.linesFound
    )} lines • ${summary.artifactCount} artifacts`,
    message:
      visibleFiles.length === 0
        ? "Coverage is fully exercised for the loaded Solidity files."
        : undefined,
  };
}

export function actionableDecorationPlan(summary: CoverageFileSummary): {
  uncoveredLines: number[];
  partialLines: number[];
} {
  return {
    uncoveredLines: summary.actionableLines
      .filter((detail) => detail.status === "uncovered")
      .map((detail) => detail.line),
    partialLines: summary.actionableLines
      .filter((detail) => detail.status === "partial")
      .map((detail) => detail.line),
  };
}

function fileDescription(summary: CoverageFileSummary): string {
  const actionable = summary.actionableLines.length;
  const uncovered = summary.actionableLines.filter(
    (detail) => detail.status === "uncovered"
  ).length;
  const partial = summary.actionableLines.filter(
    (detail) => detail.status === "partial"
  ).length;
  const parts = [
    `${formatPercent(summary.linesHit, summary.linesFound)} lines`,
    `${summary.linesHit}/${summary.linesFound}`,
  ];
  if (summary.branchesFound > 0) {
    parts.push(
      `${formatPercent(summary.branchesHit, summary.branchesFound)} branches`
    );
  }
  if (actionable > 0) {
    parts.push(`${uncovered} uncovered`);
    if (partial > 0) {
      parts.push(`${partial} partial`);
    }
  } else {
    parts.push("fully covered");
  }
  return parts.join(" • ");
}

function lineDescription(detail: CoverageLineDetail): string {
  if (detail.status === "uncovered") {
    return detail.branchesFound > 0
      ? `uncovered • 0 hits • ${detail.branchesHit}/${detail.branchesFound} branches`
      : "uncovered • 0 hits";
  }
  return `partial • ${detail.hits} hits • ${detail.branchesHit}/${detail.branchesFound} branches`;
}

function summarizeWorkspacePercentages(files: readonly CoverageFileSummary[]): {
  linesFound: number;
  linesHit: number;
} {
  return files.reduce(
    (acc, file) => {
      acc.linesFound += file.linesFound;
      acc.linesHit += file.linesHit;
      return acc;
    },
    { linesFound: 0, linesHit: 0 }
  );
}

function compareCoverageFiles(left: CoverageFileSummary, right: CoverageFileSummary): number {
  return (
    right.actionableLines.length - left.actionableLines.length ||
    left.linesHit / Math.max(left.linesFound, 1) - right.linesHit / Math.max(right.linesFound, 1) ||
    left.displayPath.localeCompare(right.displayPath)
  );
}

function resolveCoverageSourcePath(
  rawSourcePath: string,
  artifactPath: string,
  workspaceRoots: readonly string[]
): string | null {
  if (!rawSourcePath) {
    return null;
  }

  const candidates = path.isAbsolute(rawSourcePath)
    ? [rawSourcePath]
    : [
        ...workspaceRoots.map((root) => path.resolve(root, rawSourcePath)),
        path.resolve(path.dirname(artifactPath), rawSourcePath),
      ];

  return normalizePath(candidates[0]);
}

function displayPathForFile(filePath: string, workspaceRoots: readonly string[]): string {
  const normalized = normalizePath(filePath);
  for (const root of workspaceRoots.map(normalizePath)) {
    const relative = path.relative(root, normalized);
    if (relative && !relative.startsWith("..") && !path.isAbsolute(relative)) {
      return relative;
    }
  }
  return path.basename(normalized);
}

function normalizePath(value: string): string {
  return path.normalize(value);
}

function xmlAttr(source: string, name: string): string | undefined {
  const pattern = new RegExp(`${escapeRegExp(name)}\\s*=\\s*(['"])(.*?)\\1`, "iu");
  const match = source.match(pattern);
  if (!match) {
    return undefined;
  }
  return decodeXmlText(match[2] ?? "");
}

function xmlBoolAttr(source: string, name: string): boolean {
  const value = xmlAttr(source, name);
  return value?.toLowerCase() === "true";
}

function parseCoverageInt(value: string | undefined): number {
  if (!value) {
    return Number.NaN;
  }
  return Number.parseInt(value, 10);
}

function decodeXmlText(value: string): string {
  return value
    .replace(/&quot;/gu, "\"")
    .replace(/&apos;/gu, "'")
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&amp;/gu, "&");
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

function filterModeLabel(filterMode: CoverageOverviewFilterMode): string {
  return filterMode === "all" ? "all files" : "actionable";
}

function formatPercent(hit: number, found: number): string {
  if (found <= 0) {
    return "0.0%";
  }
  return `${((hit / found) * 100).toFixed(1)}%`;
}
