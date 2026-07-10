import { describe, expect, it } from "vitest";
import {
  actionableDecorationPlan,
  buildCoverageTree,
  parseCoberturaArtifact,
  parseCoverageArtifact,
  parseLcovArtifact,
  shouldExpandCoverageFile,
  summarizeCoverageArtifacts,
  summarizeCoverageOverview,
} from "./coverageOverviewModel";

const fixtureSourceExists = (candidate: string): boolean =>
  candidate.startsWith("/workspace/") && candidate.endsWith(".sol");

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
      ["/workspace"],
      fixtureSourceExists
    );

    expect(records).toHaveLength(1);
    expect(records[0]?.filePath).toBe("/workspace/src/Vault.sol");
    expect(records[0]?.lineHits.get(10)).toBe(0);
    expect(records[0]?.lineHits.get(11)).toBe(4);
    expect(records[0]?.branchHits.get(11)).toMatchObject({ found: 2, hit: 1 });
    expect(Array.from(records[0]?.branchHits.get(11)?.identities ?? [])).toEqual([
      ["0:0", 4],
      ["0:1", 0],
    ]);
  });

  it("flushes the final record without a trailing end_of_record", () => {
    const records = parseLcovArtifact(
      ["SF:/workspace/src/Vault.sol", "DA:7,1"].join("\n"),
      "/workspace/lcov.info",
      ["/workspace"],
      fixtureSourceExists
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
      ["/workspace"],
      fixtureSourceExists
    );

    expect(records).toHaveLength(1);
    expect(records[0]?.filePath).toBe("/workspace/src/Vault.sol");
    expect(records[0]?.lineHits.get(10)).toBe(0);
    expect(records[0]?.lineHits.get(11)).toBe(4);
    expect(records[0]?.branchHits.get(11)).toMatchObject({ found: 2, hit: 1 });
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
      ["/workspace"],
      fixtureSourceExists
    );

    expect(records).toHaveLength(1);
    expect(records[0]?.branchHits.get(8)).toMatchObject({ found: 2, hit: 2 });
  });

  it("resolves class filenames relative to Cobertura source roots", () => {
    const expected = "/workspace/packages/vault/src/Vault.sol";
    const records = parseCoberturaArtifact(
      [
        "<coverage>",
        "  <sources><source>../packages/vault</source></sources>",
        '  <packages><package name="contracts"><classes>',
        '    <class name="Vault" filename="src/Vault.sol">',
        '      <lines><line number="8" hits="1" /></lines>',
        "    </class>",
        "  </classes></package></packages>",
        "</coverage>",
      ].join("\n"),
      "/workspace/reports/coverage.xml",
      ["/workspace"],
      (candidate) => candidate === expected
    );

    expect(records[0]?.filePath).toBe(expected);
  });

  it("uses line-level branch counts for nested Cobertura conditions", () => {
    const records = parseCoberturaArtifact(
      [
        "<coverage>",
        '  <packages><package name="contracts"><classes>',
        '    <class name="Vault" filename="/workspace/src/Vault.sol">',
        '      <lines><line number="11" hits="1" branch="true" condition-coverage="50% (1/2)">',
        '        <conditions><condition number="0" type="jump" coverage="50%" /></conditions>',
        "      </line></lines>",
        "    </class>",
        "  </classes></package></packages>",
        "</coverage>",
      ].join("\n"),
      "/workspace/coverage/cobertura.xml",
      ["/workspace"],
      fixtureSourceExists
    );

    expect(records[0]?.branchHits.get(11)).toMatchObject({ found: 2, hit: 1 });
    const summary = summarizeCoverageArtifacts(records, ["/workspace"]);
    expect(summary.files[0]?.actionableLines).toEqual([
      expect.objectContaining({
        line: 11,
        status: "partial",
        branchesFound: 2,
        branchesHit: 1,
      }),
    ]);
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
        ["/workspace"],
        fixtureSourceExists
      ),
      ...parseLcovArtifact(
        [
          "SF:/workspace/src/Vault.sol",
          "DA:10,2",
          "DA:20,0",
          "end_of_record",
        ].join("\n"),
        "/workspace/coverage/integration.lcov",
        ["/workspace"],
        fixtureSourceExists
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

  it("unions branch identities across complementary LCOV artifacts", () => {
    const records = [
      ...parseLcovArtifact(
        [
          "SF:/workspace/src/Vault.sol",
          "DA:11,1",
          "BRDA:11,0,0,1",
          "BRDA:11,0,1,0",
          "end_of_record",
        ].join("\n"),
        "/workspace/coverage/unit.lcov",
        ["/workspace"],
        fixtureSourceExists
      ),
      ...parseLcovArtifact(
        [
          "SF:/workspace/src/Vault.sol",
          "DA:11,1",
          "BRDA:11,0,0,0",
          "BRDA:11,0,1,1",
          "end_of_record",
        ].join("\n"),
        "/workspace/coverage/integration.lcov",
        ["/workspace"],
        fixtureSourceExists
      ),
    ];

    const summary = summarizeCoverageArtifacts(records, ["/workspace"]);
    expect(summary.files[0]).toMatchObject({
      branchesFound: 2,
      branchesHit: 2,
      actionableLines: [],
    });
  });

  it("deduplicates mixed-format line hits and prefers LCOV branches per line", () => {
    const records = [
      ...parseLcovArtifact(
        [
          "SF:/workspace/src/Vault.sol",
          "DA:11,1",
          "BRDA:11,0,0,1",
          "BRDA:11,0,1,0",
          "end_of_record",
        ].join("\n"),
        "/workspace/coverage/lcov.info",
        ["/workspace"],
        fixtureSourceExists
      ),
      ...parseCoberturaArtifact(
        [
          "<coverage>",
          '  <packages><package name="contracts"><classes>',
          '    <class name="Vault" filename="/workspace/src/Vault.sol">',
          "      <lines>",
          '        <line number="11" hits="1" branch="true" condition-coverage="100% (2/2)" />',
          '        <line number="12" hits="1" branch="true" condition-coverage="50% (1/2)" />',
          "      </lines>",
          "    </class>",
          "  </classes></package></packages>",
          "</coverage>",
        ].join("\n"),
        "/workspace/coverage/cobertura.xml",
        ["/workspace"],
        fixtureSourceExists
      ),
    ];

    const summary = summarizeCoverageArtifacts(records, ["/workspace"]);
    expect(summary.files[0]).toMatchObject({
      branchesFound: 4,
      branchesHit: 2,
      actionableLines: [
        {
          line: 11,
          status: "partial",
          hits: 1,
          branchesFound: 2,
          branchesHit: 1,
        },
        {
          line: 12,
          status: "partial",
          hits: 1,
          branchesFound: 2,
          branchesHit: 1,
        },
      ],
    });
  });

  it("adds shortest unique root context when multi-root display paths collide", () => {
    const filePaths = [
      "/workspace/one/app/src/Vault.sol",
      "/workspace/two/app/src/Vault.sol",
      "/workspace/one/app/src/Token.sol",
    ];
    const summary = summarizeCoverageArtifacts(
      filePaths.map((filePath, index) => ({
        filePath,
        artifactPath: `/workspace/coverage/${index}.lcov`,
        format: "lcov" as const,
        lineHits: new Map([[1, 0]]),
        branchHits: new Map(),
      })),
      ["/workspace/two/app", "/workspace/one/app"]
    );
    const labelsByPath = Object.fromEntries(
      summary.files.map((file) => [file.filePath, file.displayPath])
    );

    expect(labelsByPath).toEqual({
      "/workspace/one/app/src/Token.sol": "src/Token.sol",
      "/workspace/one/app/src/Vault.sol": "one/app/src/Vault.sol",
      "/workspace/two/app/src/Vault.sol": "two/app/src/Vault.sol",
    });
    expect(buildCoverageTree(summary, "all").map((file) => file.label)).toEqual([
      "one/app/src/Vault.sol",
      "src/Token.sol",
      "two/app/src/Vault.sol",
    ]);
  });
});

describe("coverage source path resolution", () => {
  it("uses the workspace root that owns the artifact in a multi-root workspace", () => {
    const records = parseLcovArtifact(
      ["SF:src/Vault.sol", "DA:7,1", "end_of_record"].join("\n"),
      "/workspace/beta/coverage/lcov.info",
      ["/workspace/alpha", "/workspace/beta"],
      fixtureSourceExists
    );

    expect(records[0]?.filePath).toBe("/workspace/beta/src/Vault.sol");
  });

  it("prefers an existing artifact-relative source path", () => {
    const expected = "/workspace/packages/vault/src/Vault.sol";
    const records = parseLcovArtifact(
      ["SF:../src/Vault.sol", "DA:7,1", "end_of_record"].join("\n"),
      "/workspace/packages/vault/coverage/lcov.info",
      ["/workspace"],
      (candidate) => candidate === expected
    );

    expect(records[0]?.filePath).toBe(expected);
  });

  it("rejects absolute source paths outside every workspace root", () => {
    const records = parseLcovArtifact(
      ["SF:/other-project/src/Vault.sol", "DA:7,1", "end_of_record"].join(
        "\n"
      ),
      "/workspace/coverage/lcov.info",
      ["/workspace"],
      () => true
    );

    expect(records).toEqual([]);
  });

  it("rejects nonexistent absolute source paths inside the workspace", () => {
    const records = parseLcovArtifact(
      ["SF:/workspace/src/Missing.sol", "DA:7,1", "end_of_record"].join(
        "\n"
      ),
      "/workspace/coverage/lcov.info",
      ["/workspace"],
      () => false
    );

    expect(records).toEqual([]);
  });

  it("does not fabricate a relative source path when no candidate exists", () => {
    const records = parseLcovArtifact(
      ["SF:src/Missing.sol", "DA:7,1", "end_of_record"].join("\n"),
      "/workspace/coverage/lcov.info",
      ["/workspace"],
      () => false
    );

    expect(records).toEqual([]);
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
          ["/workspace"],
          fixtureSourceExists
        ),
        ...parseLcovArtifact(
          [
            "SF:/workspace/src/Token.sol",
            "DA:8,3",
            "end_of_record",
          ].join("\n"),
          "/workspace/lcov.info",
          ["/workspace"],
          fixtureSourceExists
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

  it("uses singular grammar and omits zero-valued status counts", () => {
    const [file] = buildCoverageTree(
      {
        artifactCount: 1,
        files: [
          {
            filePath: "/workspace/src/Vault.sol",
            displayPath: "src/Vault.sol",
            artifactPaths: ["/workspace/lcov.info"],
            linesFound: 1,
            linesHit: 1,
            branchesFound: 1,
            branchesHit: 0,
            actionableLines: [
              {
                line: 7,
                status: "partial",
                hits: 1,
                branchesFound: 1,
                branchesHit: 0,
              },
            ],
          },
        ],
      },
      "all"
    );

    expect(file?.description).toBe(
      "100.0% line coverage • 1/1 line • 0.0% branch coverage • 1 partial line"
    );
    expect(file?.children[0]?.description).toBe(
      "partial • 1 hit • 0/1 branch"
    );
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
        ["/workspace"],
        fixtureSourceExists
      ),
      ["/workspace"]
    );

    expect(summarizeCoverageOverview(summary, "actionable")).toEqual({
      count: 1,
      description: "actionable • 50.0% line coverage • 1 artifact",
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

describe("coverage tree expansion", () => {
  it("collapses files only after the default child limit", () => {
    expect(shouldExpandCoverageFile(20)).toBe(true);
    expect(shouldExpandCoverageFile(21)).toBe(false);
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
        ["/workspace"],
        fixtureSourceExists
      ),
      ["/workspace"]
    );

    const [file] = summary.files;
    if (!file) {
      throw new Error("expected one coverage file summary");
    }

    expect(actionableDecorationPlan(file)).toEqual({
      uncoveredLines: [10],
      partialLines: [11],
    });
  });
});
