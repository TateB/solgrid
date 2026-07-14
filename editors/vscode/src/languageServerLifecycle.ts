export type LanguageServerLifecyclePhase =
  | "starting"
  | "running"
  | "stopped";

export type LanguageServerStatusPresentation =
  | "none"
  | "starting"
  | "ready"
  | "unavailable";

export interface LanguageServerLifecycleUpdate {
  active: boolean | undefined;
  status: LanguageServerStatusPresentation;
}

const IGNORED_UPDATE: LanguageServerLifecycleUpdate = {
  active: undefined,
  status: "none",
};

export class LanguageServerLifecycle {
  private active = false;
  private initialStartSettled = false;
  private projectIndexStatusReceived = false;

  recordProjectIndexStatus(): boolean {
    if (this.initialStartSettled && !this.active) {
      return false;
    }
    this.projectIndexStatusReceived = true;
    return true;
  }

  initialStartSucceeded(): LanguageServerLifecycleUpdate {
    this.initialStartSettled = true;
    this.active = true;
    return {
      active: true,
      status: this.projectIndexStatusReceived ? "none" : "ready",
    };
  }

  initialStartFailed(): LanguageServerLifecycleUpdate {
    this.initialStartSettled = true;
    this.active = false;
    this.projectIndexStatusReceived = false;
    return { active: false, status: "unavailable" };
  }

  stateChanged(
    phase: LanguageServerLifecyclePhase
  ): LanguageServerLifecycleUpdate {
    if (!this.initialStartSettled) {
      return IGNORED_UPDATE;
    }

    switch (phase) {
      case "starting":
        this.active = false;
        this.projectIndexStatusReceived = false;
        return { active: false, status: "starting" };
      case "stopped":
        this.active = false;
        this.projectIndexStatusReceived = false;
        return { active: false, status: "unavailable" };
      case "running":
        this.active = true;
        return {
          active: true,
          status: this.projectIndexStatusReceived ? "none" : "ready",
        };
    }
  }
}
