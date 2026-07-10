import * as path from "node:path";
import { existsSync } from "node:fs";

export type CoverageOverviewFilterMode = "actionable" | "all";
export type CoverageLineStatus = "uncovered" | "partial";
export type CoverageArtifactFormat = "lcov" | "cobertura";

export interface CoverageArtifactRecord {
  filePath: string;
  artifactPath: string;
  format: CoverageArtifactFormat;
  lineHits: ReadonlyMap<number, number>;
  branchHits: ReadonlyMap<number, CoverageBranchHits>;
}

export interface CoverageBranchHits {
  found: number;
  hit: number;
  identities: ReadonlyMap<string, number>;
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
  workspaceRoots: readonly string[],
  pathExists: (candidate: string) => boolean = existsSync
): CoverageArtifactRecord[] {
  const extension = path.extname(artifactPath).toLowerCase();
  if (extension === ".xml") {
    return parseCoberturaArtifact(
      content,
      artifactPath,
      workspaceRoots,
      pathExists
    );
  }
  return parseLcovArtifact(content, artifactPath, workspaceRoots, pathExists);
}

export function parseLcovArtifact(
  content: string,
  artifactPath: string,
  workspaceRoots: readonly string[],
  pathExists: (candidate: string) => boolean = existsSync
): CoverageArtifactRecord[] {
  const records: CoverageArtifactRecord[] = [];
  let current: {
    rawSourcePath: string;
    lineHits: Map<number, number>;
    branchHits: Map<number, CoverageBranchHits>;
  } | null = null;

  const flush = (): void => {
    if (!current) {
      return;
    }
    const resolvedPath = resolveCoverageSourcePath(
      current.rawSourcePath,
      artifactPath,
      workspaceRoots,
      pathExists
    );
    if (resolvedPath) {
      records.push({
        filePath: resolvedPath,
        artifactPath: normalizePath(artifactPath),
        format: "lcov",
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
      const [lineValue, blockValue, branchValue, takenValue] = rawLine
        .slice(5)
        .split(",", 4);
      const line = Number.parseInt(lineValue ?? "", 10);
      if (!Number.isInteger(line) || line <= 0) {
        continue;
      }
      const identities = new Map(
        current.branchHits.get(line)?.identities ?? []
      );
      const identity = `${blockValue ?? ""}:${branchValue ?? ""}`;
      const taken =
        takenValue === "-"
          ? 0
          : Math.max(0, Number.parseInt(takenValue ?? "0", 10) || 0);
      identities.set(identity, (identities.get(identity) ?? 0) + taken);
      current.branchHits.set(line, branchCoverage(identities));
    }
  }

  flush();
  return records;
}

export function parseCoberturaArtifact(
  content: string,
  artifactPath: string,
  workspaceRoots: readonly string[],
  pathExists: (candidate: string) => boolean = existsSync
): CoverageArtifactRecord[] {
  const sourceRoots = Array.from(
    content.matchAll(/<source\b[^>]*>([\s\S]*?)<\/source>/giu)
  )
    .map((match) => decodeXmlText((match[1] ?? "").trim()))
    .filter(Boolean);
  const records = new Map<
    string,
    {
      artifactPath: string;
      lineHits: Map<number, number>;
      branchHits: Map<number, CoverageBranchHits>;
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
      workspaceRoots,
      pathExists,
      sourceRoots
    );
    if (!filePath) {
      continue;
    }

    const record = records.get(filePath) ?? {
      artifactPath: normalizePath(artifactPath),
      lineHits: new Map<number, number>(),
      branchHits: new Map<number, CoverageBranchHits>(),
    };

    for (const lineMatch of (classMatch[2] ?? "").matchAll(
      /<line\b([^>]*?)(?:\/>|>([\s\S]*?)<\/line>)/giu
    )) {
      const lineNumber = parseCoverageInt(xmlAttr(lineMatch[1] ?? "", "number"));
      const hits = parseCoverageInt(xmlAttr(lineMatch[1] ?? "", "hits"));
      if (!Number.isInteger(lineNumber) || lineNumber <= 0 || !Number.isFinite(hits)) {
        continue;
      }
      record.lineHits.set(lineNumber, (record.lineHits.get(lineNumber) ?? 0) + hits);

      if (!xmlBoolAttr(lineMatch[1] ?? "", "branch")) {
        continue;
      }
      const identities = new Map(
        record.branchHits.get(lineNumber)?.identities ?? []
      );
      const conditions = Array.from(
        (lineMatch[2] ?? "").matchAll(/<condition\b([^>]*)\/?>/giu)
      );
      const coverage = xmlAttr(lineMatch[1] ?? "", "condition-coverage");
      const counts = coverage?.match(/\((\d+)\s*\/\s*(\d+)\)/u);
      if (counts) {
        // The line-level numerator/denominator is authoritative. Individual
        // condition elements often describe a two-outcome jump as one item.
        const hit = Number.parseInt(counts[1] ?? "0", 10);
        const found = Number.parseInt(counts[2] ?? "0", 10);
        for (let index = 0; index < found; index += 1) {
          const identity = `cobertura:${index}`;
          identities.set(
            identity,
            Math.max(identities.get(identity) ?? 0, index < hit ? 1 : 0)
          );
        }
      } else if (conditions.length > 0) {
        for (const [index, condition] of conditions.entries()) {
          const conditionNumber =
            xmlAttr(condition[1] ?? "", "number") ?? String(index);
          const percent = Number.parseFloat(
            xmlAttr(condition[1] ?? "", "coverage") ?? "0"
          );
          const outcomesHit = Math.max(
            0,
            Math.min(2, Math.round((percent / 100) * 2))
          );
          for (let outcome = 0; outcome < 2; outcome += 1) {
            const identity = `cobertura:${conditionNumber}:${outcome}`;
            identities.set(
              identity,
              Math.max(
                identities.get(identity) ?? 0,
                outcome < outcomesHit ? 1 : 0
              )
            );
          }
        }
      } else {
        identities.set(
          "cobertura:0",
          Math.max(identities.get("cobertura:0") ?? 0, hits > 0 ? 1 : 0)
        );
      }
      record.branchHits.set(lineNumber, branchCoverage(identities));
    }

    records.set(filePath, record);
  }

  return Array.from(records.entries()).map(([filePath, record]) => ({
    filePath,
    artifactPath: record.artifactPath,
    format: "cobertura" as const,
    lineHits: record.lineHits,
    branchHits: record.branchHits,
  }));
}

export function summarizeCoverageArtifacts(
  records: readonly CoverageArtifactRecord[],
  workspaceRoots: readonly string[]
): CoverageWorkspaceSummary {
  interface FormatBucket {
    lineHits: Map<number, number>;
    branchHits: Map<number, Map<string, number>>;
  }

  const files = new Map<
    string,
    {
      artifactPaths: Set<string>;
      formats: Map<CoverageArtifactFormat, FormatBucket>;
    }
  >();

  for (const record of records) {
    if (path.extname(record.filePath) !== ".sol") {
      continue;
    }
    const bucket = files.get(record.filePath) ?? {
      artifactPaths: new Set<string>(),
      formats: new Map<CoverageArtifactFormat, FormatBucket>(),
    };
    const formatBucket = bucket.formats.get(record.format) ?? {
      lineHits: new Map<number, number>(),
      branchHits: new Map<number, Map<string, number>>(),
    };
    bucket.artifactPaths.add(record.artifactPath);
    for (const [line, hits] of record.lineHits) {
      formatBucket.lineHits.set(
        line,
        (formatBucket.lineHits.get(line) ?? 0) + hits
      );
    }
    for (const [line, branchHits] of record.branchHits) {
      const identities =
        formatBucket.branchHits.get(line) ?? new Map<string, number>();
      for (const [identity, hits] of branchHits.identities) {
        identities.set(identity, Math.max(identities.get(identity) ?? 0, hits));
      }
      formatBucket.branchHits.set(line, identities);
    }
    bucket.formats.set(record.format, formatBucket);
    files.set(record.filePath, bucket);
  }

  const summaries = Array.from(files.entries())
    .map(([filePath, bucket]): CoverageFileSummary => {
      const lcov = bucket.formats.get("lcov");
      const cobertura = bucket.formats.get("cobertura");
      const lineNumbers = Array.from(
        new Set([
          ...(lcov?.lineHits.keys() ?? []),
          ...(cobertura?.lineHits.keys() ?? []),
        ])
      ).sort((left, right) => left - right);
      const lineHits = new Map(
        lineNumbers.map((line) => [
          line,
          // Runs from the same format are complementary and are summed above.
          // Across formats, use the larger total so duplicate reports do not
          // inflate the displayed execution count.
          Math.max(
            lcov?.lineHits.get(line) ?? 0,
            cobertura?.lineHits.get(line) ?? 0
          ),
        ])
      );
      const branchLineNumbers = new Set([
        ...(lcov?.branchHits.keys() ?? []),
        ...(cobertura?.branchHits.keys() ?? []),
      ]);
      const branchHits = new Map<number, Map<string, number>>();
      for (const line of branchLineNumbers) {
        // LCOV exposes stable block/branch identities. Prefer it only on lines
        // where it has branch data, retaining Cobertura-only branch lines.
        const identities =
          lcov?.branchHits.get(line) ?? cobertura?.branchHits.get(line);
        if (identities) {
          branchHits.set(line, identities);
        }
      }
      const actionableLines = lineNumbers
        .map((line): CoverageLineDetail | null => {
          const hits = lineHits.get(line) ?? 0;
          const branch = branchCoverage(
            branchHits.get(line) ?? new Map<string, number>()
          );
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
      const linesHit = lineNumbers.filter((line) => (lineHits.get(line) ?? 0) > 0).length;
      const branches = Array.from(branchHits.values()).map(branchCoverage);
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
  workspaceRoots: readonly string[],
  pathExists: (candidate: string) => boolean,
  sourceRoots: readonly string[] = []
): string | null {
  if (!rawSourcePath) {
    return null;
  }

  if (path.isAbsolute(rawSourcePath)) {
    return normalizePath(rawSourcePath);
  }

  const normalizedArtifact = normalizePath(artifactPath);
  const roots = workspaceRoots.map(normalizePath);
  const owningRoots = roots
    .filter((root) => isPathInside(normalizedArtifact, root))
    .sort((left, right) => right.length - left.length);
  const remainingRoots = roots.filter((root) => !owningRoots.includes(root));
  const artifactDirectory = path.dirname(normalizedArtifact);
  const sourceRootCandidates = sourceRoots.flatMap((sourceRoot) => {
    if (path.isAbsolute(sourceRoot)) {
      return [path.resolve(sourceRoot, rawSourcePath)];
    }
    return [
      path.resolve(artifactDirectory, sourceRoot, rawSourcePath),
      ...owningRoots.map((root) =>
        path.resolve(root, sourceRoot, rawSourcePath)
      ),
      ...remainingRoots.map((root) =>
        path.resolve(root, sourceRoot, rawSourcePath)
      ),
    ];
  });
  const artifactRelative = path.resolve(
    artifactDirectory,
    rawSourcePath
  );
  const rootRelative = [...owningRoots, ...remainingRoots].map((root) =>
    path.resolve(root, rawSourcePath)
  );
  const candidates = rawSourcePath.startsWith(".")
    ? [...sourceRootCandidates, artifactRelative, ...rootRelative]
    : [...sourceRootCandidates, ...rootRelative, artifactRelative];
  const uniqueCandidates = Array.from(new Set(candidates.map(normalizePath)));

  return uniqueCandidates.find(pathExists) ?? uniqueCandidates[0] ?? null;
}

function branchCoverage(
  identities: ReadonlyMap<string, number>
): CoverageBranchHits {
  return {
    found: identities.size,
    hit: Array.from(identities.values()).filter((hits) => hits > 0).length,
    identities: new Map(identities),
  };
}

function isPathInside(candidate: string, root: string): boolean {
  const relative = path.relative(root, candidate);
  return (
    relative === "" ||
    (!relative.startsWith("..") && !path.isAbsolute(relative))
  );
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
