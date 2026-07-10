import { describe, expect, it } from "vitest";
import { LanguageServerLifecycle } from "./languageServerLifecycle";

describe("LanguageServerLifecycle", () => {
  it("does not expose commands for the initial Running event before start resolves", () => {
    const lifecycle = new LanguageServerLifecycle();

    expect(lifecycle.stateChanged("running")).toEqual({
      active: undefined,
      status: "none",
    });
    expect(lifecycle.initialStartSucceeded()).toEqual({
      active: true,
      status: "ready",
    });
  });

  it("preserves an initial project-index presentation when startup succeeds", () => {
    const lifecycle = new LanguageServerLifecycle();

    expect(lifecycle.recordProjectIndexStatus()).toBe(true);
    expect(lifecycle.initialStartSucceeded()).toEqual({
      active: true,
      status: "none",
    });
  });

  it("disables commands during a restart and restores them once Running", () => {
    const lifecycle = new LanguageServerLifecycle();
    lifecycle.initialStartSucceeded();
    lifecycle.recordProjectIndexStatus();

    expect(lifecycle.stateChanged("stopped")).toEqual({
      active: false,
      status: "unavailable",
    });
    expect(lifecycle.stateChanged("starting")).toEqual({
      active: false,
      status: "starting",
    });
    expect(lifecycle.recordProjectIndexStatus()).toBe(false);
    expect(lifecycle.stateChanged("running")).toEqual({
      active: true,
      status: "ready",
    });
    expect(lifecycle.recordProjectIndexStatus()).toBe(true);
  });

  it("keeps commands unavailable after an initial startup failure", () => {
    const lifecycle = new LanguageServerLifecycle();

    expect(lifecycle.initialStartFailed()).toEqual({
      active: false,
      status: "unavailable",
    });
    expect(lifecycle.recordProjectIndexStatus()).toBe(false);
  });
});
