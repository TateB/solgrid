import { describe, expect, it } from "vitest";
import {
  actionableDecorationPlan,
  buildCoverageTree,
  parseCoberturaArtifact,
  parseCoverageArtifact,
  parseLcovArtifact,
  summarizeCoverageArtifacts,
  summarizeCoverageOverview,
} from "./coverageOverviewModel";

describe("parseLcovArtifact", () => {
  it("parses DA and BRDA entries into normalized file records", () => {
    const records = parseLcovArtifact(
      [
        "TN:",
        "SF:src/Vault.sol",
        "DA:10,0",
        "DA:11,4",
        "BRDA:11,0,0,4",
        "BRDA:11,0,1,0",
        "end_of_record",
      ].join("\n"),
      "/workspace/coverage/lcov.info",
      ["/workspace"]
    );

    expect(records).toHaveLength(1);
    expect(records[0]?.filePath).toBe("/workspace/src/Vault.sol");
    expect(records[0]?.lineHits.get(10)).toBe(0);
    expect(records[0]?.lineHits.get(11)).toBe(4);
    expect(records[0]?.branchHits.get(11)).toEqual({ found: 2, hit: 1 });
  });

  it("flushes the final record without a trailing end_of_record", () => {
    const records = parseLcovArtifact(
      ["SF:/workspace/src/Vault.sol", "DA:7,1"].join("\n"),
      "/workspace/lcov.info",
      ["/workspace"]
    );

    expect(records).toHaveLength(1);
    expect(records[0]?.filePath).toBe("/workspace/src/Vault.sol");
    expect(records[0]?.lineHits.get(7)).toBe(1);
  });
});

describe("parseCoberturaArtifact", () => {
  it("parses class and line elements into normalized file records", () => {
    const records = parseCoberturaArtifact(
      [
        '<?xml version="1.0" ?>',
        "<coverage>",
        '  <packages><package name="contracts"><classes>',
        '    <class name="Vault" filename="src/Vault.sol">',
        "      <lines>",
        '        <line number="10" hits="0" branch="false" />',
        '        <line number="11" hits="4" branch="true" condition-coverage="50% (1/2)" />',
        "      </lines>",
        "    </class>",
        "  </classes></package></packages>",
        "</coverage>",
      ].join("\n"),
      "/workspace/coverage/cobertura.xml",
      ["/workspace"]
    );

    expect(records).toHaveLength(1);
    expect(records[0]?.filePath).toBe("/workspace/src/Vault.sol");
    expect(records[0]?.lineHits.get(10)).toBe(0);
    expect(records[0]?.lineHits.get(11)).toBe(4);
    expect(records[0]?.branchHits.get(11)).toEqual({ found: 2, hit: 1 });
  });

  it("dispatches XML artifacts through the generic parser", () => {
    const records = parseCoverageArtifact(
      [
        "<coverage>",
        '  <packages><package name="contracts"><classes>',
        '    <class name="Vault" filename="/workspace/src/Vault.sol">',
        "      <lines>",
        '        <line number="8" hits="3" branch="true" condition-coverage="100% (2/2)" />',
        "      </lines>",
        "    </class>",
        "  </classes></package></packages>",
        "</coverage>",
      ].join("\n"),
      "/workspace/coverage/coverage.xml",
      ["/workspace"]
    );

    expect(records).toHaveLength(1);
    expect(records[0]?.branchHits.get(8)).toEqual({ found: 2, hit: 2 });
  });
});

describe("summarizeCoverageArtifacts", () => {
  it("merges multiple artifacts and surfaces actionable lines", () => {
    const records = [
      ...parseLcovArtifact(
        [
          "SF:/workspace/src/Vault.sol",
          "DA:10,0",
          "DA:11,1",
          "BRDA:11,0,0,1",
          "BRDA:11,0,1,0",
          "end_of_record",
        ].join("\n"),
        "/workspace/coverage/lcov.info",
        ["/workspace"]
      ),
      ...parseLcovArtifact(
        [
          "SF:/workspace/src/Vault.sol",
          "DA:10,2",
          "DA:20,0",
          "end_of_record",
        ].join("\n"),
        "/workspace/coverage/integration.lcov",
        ["/workspace"]
      ),
    ];

    const summary = summarizeCoverageArtifacts(records, ["/workspace"]);
    expect(summary.artifactCount).toBe(2);
    expect(summary.files).toHaveLength(1);
    expect(summary.files[0]).toMatchObject({
      displayPath: "src/Vault.sol",
      linesFound: 3,
      linesHit: 2,
      branchesFound: 2,
      branchesHit: 1,
    });
    expect(summary.files[0]?.actionableLines).toEqual([
      {
        line: 11,
        status: "partial",
        hits: 1,
        branchesFound: 2,
        branchesHit: 1,
      },
      {
        line: 20,
        status: "uncovered",
        hits: 0,
        branchesFound: 0,
        branchesHit: 0,
      },
    ]);
  });
});

describe("buildCoverageTree", () => {
  it("filters to actionable files by default and keeps child line nodes", () => {
    const summary = summarizeCoverageArtifacts(
      [
        ...parseLcovArtifact(
          [
            "SF:/workspace/src/Vault.sol",
            "DA:5,0",
            "DA:6,1",
            "end_of_record",
          ].join("\n"),
          "/workspace/lcov.info",
          ["/workspace"]
        ),
        ...parseLcovArtifact(
          [
            "SF:/workspace/src/Token.sol",
            "DA:8,3",
            "end_of_record",
          ].join("\n"),
          "/workspace/lcov.info",
          ["/workspace"]
        ),
      ],
      ["/workspace"]
    );

    const actionableTree = buildCoverageTree(summary, "actionable");
    expect(actionableTree).toHaveLength(1);
    expect(actionableTree[0]?.label).toBe("src/Vault.sol");
    expect(actionableTree[0]?.children).toHaveLength(1);
    expect(actionableTree[0]?.children[0]).toMatchObject({
      label: "Line 5",
      description: "uncovered • 0 hits",
    });

    const allTree = buildCoverageTree(summary, "all");
    expect(allTree).toHaveLength(2);
  });
});

describe("summarizeCoverageOverview", () => {
  it("reports actionable counts and percentages", () => {
    const summary = summarizeCoverageArtifacts(
      parseLcovArtifact(
        [
          "SF:/workspace/src/Vault.sol",
          "DA:10,0",
          "DA:11,1",
          "end_of_record",
        ].join("\n"),
        "/workspace/lcov.info",
        ["/workspace"]
      ),
      ["/workspace"]
    );

    expect(summarizeCoverageOverview(summary, "actionable")).toEqual({
      count: 1,
      description: "actionable • 50.0% lines • 1 artifacts",
      message: undefined,
    });
  });

  it("returns a specific message when artifacts do not map to Solidity files", () => {
    expect(
      summarizeCoverageOverview(
        {
          artifactCount: 2,
          files: [],
        },
        "actionable"
      )
    ).toEqual({
      count: 0,
      description: "actionable • 2 artifacts",
      message:
        "Coverage artifacts were found, but none mapped to Solidity source files in this workspace.",
    });
  });

  it("mentions the supported artifact formats when nothing is loaded", () => {
    expect(summarizeCoverageOverview(undefined, "actionable")).toEqual({
      count: 0,
      description: "actionable • 0 artifacts",
      message:
        "No supported coverage artifacts found. Generate LCOV or Cobertura coverage and refresh.",
    });
  });
});

describe("actionableDecorationPlan", () => {
  it("splits uncovered and partial lines for editor decorations", () => {
    const summary = summarizeCoverageArtifacts(
      parseLcovArtifact(
        [
          "SF:/workspace/src/Vault.sol",
          "DA:10,0",
          "DA:11,2",
          "BRDA:11,0,0,2",
          "BRDA:11,0,1,0",
          "end_of_record",
        ].join("\n"),
        "/workspace/lcov.info",
        ["/workspace"]
      ),
      ["/workspace"]
    );

    expect(actionableDecorationPlan(summary.files[0]!)).toEqual({
      uncoveredLines: [10],
      partialLines: [11],
    });
  });
});
