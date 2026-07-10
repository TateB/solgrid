import { beforeEach, describe, expect, it, vi } from "vitest";

const vscodeHarness = vi.hoisted(() => {
  const disposable = () => ({ dispose: vi.fn() });
  const watcher = () => ({
    dispose: vi.fn(),
    onDidCreate: vi.fn(),
    onDidChange: vi.fn(),
    onDidDelete: vi.fn(),
  });
  const workspace = {
    workspaceFolders: undefined as
      | Array<{ uri: { fsPath: string } }>
      | undefined,
    createFileSystemWatcher: vi.fn(watcher),
    findFiles: vi.fn(),
    fs: { readFile: vi.fn() },
  };
  const module = {
    EventEmitter: class {
      readonly event = vi.fn();
      readonly fire = vi.fn();
    },
    OverviewRulerLane: { Right: 4 },
    ThemeColor: class {
      constructor(readonly id: string) {}
    },
    Uri: {
      file: (fsPath: string) => ({ scheme: "file", fsPath }),
      parse: (value: string) => ({ value }),
    },
    window: {
      createTextEditorDecorationType: vi.fn((_options: unknown) => disposable()),
      onDidChangeVisibleTextEditors: vi.fn(disposable),
      onDidChangeActiveTextEditor: vi.fn(disposable),
      visibleTextEditors: [],
    },
    workspace,
  };
  return { module, workspace };
});

vi.mock("vscode", () => vscodeHarness.module);
import {
  COVERAGE_REFRESHING_MESSAGE,
  COVERAGE_WAITING_MESSAGE,
  CoverageOverviewFeature,
  coverageFileAccessibilityLabel,
  coverageLineAccessibilityLabel,
  coverageLineHoverMessage,
  coverageOverviewPresentationMessage,
  normalizeCoverageConfig,
} from "./coverageOverview";
import { AsyncRefreshQueue } from "./asyncRefreshQueue";

beforeEach(() => {
  vscodeHarness.workspace.workspaceFolders = undefined;
  vscodeHarness.workspace.findFiles.mockReset();
  vscodeHarness.workspace.fs.readFile.mockReset();
  vscodeHarness.workspace.createFileSystemWatcher.mockClear();
  vscodeHarness.module.window.createTextEditorDecorationType.mockClear();
});

describe("normalizeCoverageConfig", () => {
  it("deduplicates artifact globs without changing custom argv semantics", () => {
    expect(
      normalizeCoverageConfig({
        enable: true,
        artifacts: [" **/lcov.info ", "**/lcov.info", ""],
        autoRefreshAfterRun: true,
        customCommand: [
          "tool",
          "--tag",
          "value",
          "--tag",
          "value",
          "",
          " spaced value ",
        ],
      })
    ).toEqual({
      enable: true,
      artifacts: ["**/lcov.info"],
      autoRefreshAfterRun: true,
      customCommand: [
        "tool",
        "--tag",
        "value",
        "--tag",
        "value",
        "",
        " spaced value ",
      ],
    });
  });
});

describe("coverage UI labels", () => {
  it("announces path, status, hits, and singular branch grammar", () => {
    const detail = {
      line: 12,
      status: "partial" as const,
      hits: 1,
      branchesFound: 1,
      branchesHit: 0,
    };

    expect(coverageLineAccessibilityLabel("/workspace/Vault.sol", detail)).toBe(
      "partial coverage; line 12; source /workspace/Vault.sol; 1 hit; 0 of 1 branch covered"
    );
    expect(coverageLineHoverMessage(detail)).toBe(
      "Coverage: line 12 is partially covered (1 hit; 0 of 1 branch covered)."
    );
  });

  it("announces file coverage and actionable line counts", () => {
    expect(
      coverageFileAccessibilityLabel({
        filePath: "/workspace/Vault.sol",
        displayPath: "Vault.sol",
        artifactPaths: ["/workspace/lcov.info"],
        linesFound: 1,
        linesHit: 0,
        branchesFound: 0,
        branchesHit: 0,
        actionableLines: [
          {
            line: 1,
            status: "uncovered",
            hits: 0,
            branchesFound: 0,
            branchesHit: 0,
          },
        ],
      })
    ).toBe(
      "Vault.sol; source /workspace/Vault.sol; 0 of 1 line covered; 0 of 0 branches covered; 1 actionable line"
    );
  });
});

describe("coverage editor decoration cues", () => {
  it("keeps uncovered and partial lines distinguishable across themes", () => {
    const feature = new CoverageOverviewFeature();
    const [uncovered, partial] =
      vscodeHarness.module.window.createTextEditorDecorationType.mock.calls.map(
        ([options]) =>
          options as {
            borderStyle: string;
            borderWidth: string;
            gutterIconPath: { value: string };
            light: { gutterIconPath: { value: string } };
            dark: { gutterIconPath: { value: string } };
          }
      );
    const svg = (uri: { value: string }): string =>
      decodeURIComponent(uri.value);

    expect(uncovered.borderStyle).toBe("solid");
    expect(partial.borderStyle).toBe("dashed");
    expect(uncovered.borderWidth).toBe("0 0 0 3px");
    expect(partial.borderWidth).toBe("0 0 0 3px");
    expect(svg(uncovered.gutterIconPath)).toContain("M3 3l10 10");
    expect(svg(partial.gutterIconPath)).toContain("M2 2h12v12H2z");
    expect(svg(uncovered.gutterIconPath)).toContain("#888888");
    expect(svg(partial.gutterIconPath)).toContain("#888888");
    expect(svg(uncovered.light.gutterIconPath)).toContain("#a1260d");
    expect(svg(uncovered.dark.gutterIconPath)).toContain("#f48771");
    expect(svg(partial.light.gutterIconPath)).toContain("#7a6400");
    expect(svg(partial.dark.gutterIconPath)).toContain("#cca700");

    feature.dispose();
  });
});

describe("coverage loading presentation", () => {
  it("does not report a false no-artifact state before the first refresh", () => {
    expect(
      coverageOverviewPresentationMessage("No artifacts.", false, false)
    ).toBe(COVERAGE_WAITING_MESSAGE);
    expect(
      coverageOverviewPresentationMessage("No artifacts.", true, false)
    ).toBe(COVERAGE_REFRESHING_MESSAGE);
    expect(
      coverageOverviewPresentationMessage("No artifacts.", false, true)
    ).toBe("No artifacts.");
  });

  it("transitions the attached view through waiting, refreshing, and loaded", async () => {
    const feature = new CoverageOverviewFeature();
    const view: {
      description?: string;
      message?: string;
      badge?: { value: number; tooltip: string };
    } = {};
    feature.attachView(view as never);
    expect(view.message).toBe(COVERAGE_WAITING_MESSAGE);

    const refresh = feature.applyConfig({
      enable: true,
      artifacts: [],
      autoRefreshAfterRun: true,
      customCommand: [],
    });
    expect(view.message).toBe(COVERAGE_REFRESHING_MESSAGE);

    await refresh;
    expect(view.message).toBe(
      "No supported coverage artifacts found. Generate LCOV or Cobertura coverage and refresh."
    );
    feature.dispose();
  });

  it("shows discovery failures without claiming the first refresh completed", async () => {
    vscodeHarness.workspace.workspaceFolders = [
      { uri: { fsPath: "/workspace" } },
    ];
    vscodeHarness.workspace.findFiles.mockRejectedValue(
      new Error("filesystem scan unavailable")
    );
    const feature = new CoverageOverviewFeature();
    const view: { message?: string } = {};
    feature.attachView(view as never);

    await feature.applyConfig({
      enable: true,
      artifacts: ["**/lcov.info"],
      autoRefreshAfterRun: true,
      customCommand: [],
    });

    expect(view.message).toContain("Coverage refresh failed");
    expect(view.message).toContain("filesystem scan unavailable");
    expect(view.message).toContain("No new coverage data was applied");
    expect(view.message).not.toContain("No supported coverage artifacts");
    feature.dispose();
  });

  it("labels retained results as stale when a later discovery fails", async () => {
    const workspaceRoot = process.cwd();
    const artifactPath = `${workspaceRoot}/coverage/lcov.info`;
    const sourcePath = `${workspaceRoot}/test/fixtures/clean.sol`;
    vscodeHarness.workspace.workspaceFolders = [
      { uri: { fsPath: workspaceRoot } },
    ];
    vscodeHarness.workspace.findFiles
      .mockResolvedValueOnce([{ scheme: "file", fsPath: artifactPath }])
      .mockRejectedValueOnce(new Error("workspace search failed"));
    vscodeHarness.workspace.fs.readFile.mockResolvedValue(
      new TextEncoder().encode(
        `SF:${sourcePath}\nDA:1,0\nend_of_record\n`
      )
    );
    const feature = new CoverageOverviewFeature();
    const view: { message?: string } = {};
    feature.attachView(view as never);
    await feature.applyConfig({
      enable: true,
      artifacts: ["**/lcov.info"],
      autoRefreshAfterRun: true,
      customCommand: [],
    });

    await feature.refresh();

    expect(view.message).toContain(
      "Previously loaded coverage is still shown and may be stale"
    );
    feature.dispose();
  });

  it("lets a queued successful refresh replace an overlapping discovery failure", async () => {
    let rejectFirstDiscovery: ((error: Error) => void) | undefined;
    vscodeHarness.workspace.workspaceFolders = [
      { uri: { fsPath: "/workspace" } },
    ];
    vscodeHarness.workspace.findFiles
      .mockImplementationOnce(
        () =>
          new Promise<never>((_resolve, reject) => {
            rejectFirstDiscovery = reject;
          })
      )
      .mockResolvedValueOnce([]);
    const feature = new CoverageOverviewFeature();
    const view: { message?: string } = {};
    feature.attachView(view as never);

    const owner = feature.applyConfig({
      enable: true,
      artifacts: ["**/lcov.info"],
      autoRefreshAfterRun: true,
      customCommand: [],
    });
    const follower = feature.refresh();
    rejectFirstDiscovery?.(new Error("old discovery failed"));

    await Promise.all([owner, follower]);

    expect(view.message).toBe(
      "No supported coverage artifacts found. Generate LCOV or Cobertura coverage and refresh."
    );
    expect(view.message).not.toContain("old discovery failed");
    feature.dispose();
  });

  it("surfaces the artifact path and cause when an artifact cannot be read", async () => {
    vscodeHarness.workspace.workspaceFolders = [
      { uri: { fsPath: "/workspace" } },
    ];
    vscodeHarness.workspace.findFiles.mockResolvedValue([
      { scheme: "file", fsPath: "/workspace/coverage/lcov.info" },
    ]);
    vscodeHarness.workspace.fs.readFile.mockRejectedValue(
      new Error("permission denied")
    );
    const feature = new CoverageOverviewFeature();
    const view: { message?: string } = {};
    feature.attachView(view as never);

    await feature.applyConfig({
      enable: true,
      artifacts: ["**/lcov.info"],
      autoRefreshAfterRun: true,
      customCommand: [],
    });

    expect(view.message).toContain("Could not read or parse 1 coverage artifact");
    expect(view.message).toContain("/workspace/coverage/lcov.info");
    expect(view.message).toContain("permission denied");
    feature.dispose();
  });

  it("marks a partial refresh incomplete when one artifact is skipped", async () => {
    const workspaceRoot = process.cwd();
    const sourcePath = `${workspaceRoot}/test/fixtures/clean.sol`;
    vscodeHarness.workspace.workspaceFolders = [
      { uri: { fsPath: workspaceRoot } },
    ];
    vscodeHarness.workspace.findFiles.mockResolvedValue([
      { scheme: "file", fsPath: `${workspaceRoot}/coverage/good.lcov` },
      {
        scheme: "file",
        fsPath: `${workspaceRoot}/coverage/unreadable.lcov`,
      },
    ]);
    vscodeHarness.workspace.fs.readFile.mockImplementation(
      async (uri: { fsPath: string }) => {
        if (uri.fsPath.endsWith("unreadable.lcov")) {
          throw new Error("access denied");
        }
        return new TextEncoder().encode(
          `SF:${sourcePath}\nDA:1,0\nend_of_record\n`
        );
      }
    );
    const feature = new CoverageOverviewFeature();
    const view: { message?: string } = {};
    feature.attachView(view as never);

    await feature.applyConfig({
      enable: true,
      artifacts: ["**/*.lcov"],
      autoRefreshAfterRun: true,
      customCommand: [],
    });

    expect(view.message).toContain("Coverage results may be incomplete");
    expect(view.message).toContain("unreadable.lcov");
    const nodes = await feature.getChildren();
    expect(nodes).toHaveLength(1);
    if (!nodes) {
      throw new Error("Expected the successfully parsed coverage file node.");
    }
    expect(nodes[0]).toMatchObject({
      kind: "file",
      summary: { filePath: sourcePath },
    });
    feature.dispose();
  });
});

describe("AsyncRefreshQueue", () => {
  it("makes followers await a queued refresh after the in-flight run", async () => {
    let releaseFirst: (() => void) | undefined;
    let currentConfig = "old";
    const applied: string[] = [];
    const queue = new AsyncRefreshQueue(async () => {
      const config = currentConfig;
      if (config === "old") {
        await new Promise<void>((resolve) => {
          releaseFirst = resolve;
        });
      }
      applied.push(config);
    });

    const owner = queue.run();
    currentConfig = "new";
    const follower = queue.run();
    let followerResolved = false;
    void follower.then(() => {
      followerResolved = true;
    });

    releaseFirst?.();
    await Promise.all([owner, follower]);
    expect(followerResolved).toBe(true);
    expect(applied).toEqual(["old", "new"]);
  });

  it("drains queued work after a failed iteration and preserves per-batch outcomes", async () => {
    let releaseFirst: (() => void) | undefined;
    let currentConfig = "old";
    const attempted: string[] = [];
    const presentationOrder: string[] = [];
    let presentation = "initial";
    const queue = new AsyncRefreshQueue(async () => {
      const config = currentConfig;
      attempted.push(config);
      if (config === "old") {
        await new Promise<void>((resolve) => {
          releaseFirst = resolve;
        });
        throw new Error("old refresh failed");
      }
    });

    const owner = queue.run();
    currentConfig = "new";
    const follower = queue.run();
    const ownerOutcome = owner.then(
      () => "resolved",
      (error: unknown) => {
        presentation = "old failure";
        presentationOrder.push(presentation);
        return error instanceof Error ? error.message : String(error);
      }
    );
    const followerOutcome = follower.then(
      () => {
        presentation = "new success";
        presentationOrder.push(presentation);
        return "resolved";
      },
      (error: unknown) =>
        error instanceof Error ? error.message : String(error)
    );

    releaseFirst?.();

    await expect(ownerOutcome).resolves.toBe("old refresh failed");
    await expect(followerOutcome).resolves.toBe("resolved");
    expect(attempted).toEqual(["old", "new"]);
    expect(presentationOrder).toEqual(["old failure", "new success"]);
    expect(presentation).toBe("new success");
  });
});
