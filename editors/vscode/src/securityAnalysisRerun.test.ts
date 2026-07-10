import { describe, expect, it, vi } from "vitest";
import { requestSecurityAnalysisRerun } from "./securityAnalysisRerun";

describe("requestSecurityAnalysisRerun", () => {
  it("reports confirmed completion when the command succeeds", async () => {
    const fallback = vi.fn(async () => undefined);

    await expect(
      requestSecurityAnalysisRerun(async () => undefined, fallback)
    ).resolves.toEqual({ status: "complete" });
    expect(fallback).not.toHaveBeenCalled();
  });

  it("surfaces an unconfirmed fallback instead of remaining pending", async () => {
    const result = await requestSecurityAnalysisRerun(
      async () => {
        throw new Error("unsupported command");
      },
      async () => undefined
    );

    expect(result.status).toBe("unconfirmed");
    expect("message" in result ? result.message : "").toContain(
      "unsupported command"
    );
    expect("message" in result ? result.message : "").toContain(
      "results may still update"
    );
  });

  it("combines command and fallback failures into an explicit error", async () => {
    const result = await requestSecurityAnalysisRerun(
      async () => {
        throw new Error("command channel closed");
      },
      async () => {
        throw new Error("notification channel closed");
      }
    );

    expect(result).toEqual({
      status: "failed",
      message:
        "Security analysis failed: command channel closed. The compatibility refresh also failed: notification channel closed.",
    });
  });
});
