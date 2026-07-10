import { describe, expect, it, vi } from "vitest";

vi.mock("vscode", () => ({}));
import { normalizeCoverageConfig } from "./coverageOverview";
import { AsyncRefreshQueue } from "./asyncRefreshQueue";

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
    await owner;
    expect(followerResolved).toBe(true);
    expect(applied).toEqual(["old", "new"]);
  });
});
