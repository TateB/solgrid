import { describe, expect, it } from "vitest";
import packageManifest from "../package.json";

interface CommandContribution {
  command: string;
  icon?: string;
  enablement?: string;
}

interface MenuContribution {
  command: string;
  when?: string;
  group?: string;
}

interface ViewContribution {
  id: string;
  when?: string;
}

const commands = packageManifest.contributes.commands as CommandContribution[];
const menus = packageManifest.contributes.menus as Record<
  string,
  MenuContribution[]
>;
const explorerViews = packageManifest.contributes.views
  .explorer as ViewContribution[];

describe("VS Code UI contributions", () => {
  it("gives every contributed tree action a codicon", () => {
    const treeCommandIds = new Set(
      [
        ...(menus["view/title"] ?? []),
        ...(menus["view/item/context"] ?? []),
      ].map((menu) => menu.command)
    );

    for (const command of commands.filter((entry) =>
      treeCommandIds.has(entry.command)
    )) {
      expect(command.icon, command.command).toMatch(/^\$\([a-z0-9-]+\)$/u);
    }
  });

  it("keeps title toolbars compact while exposing every filter and group mode", () => {
    const titleMenus = menus["view/title"] ?? [];
    expect(
      titleMenus
        .filter((menu) => menu.group?.startsWith("navigation"))
        .map((menu) => menu.command)
    ).toEqual([
      "solgrid.securityOverview.refresh",
      "solgrid.coverage.refresh",
      "solgrid.coverage.run",
    ]);

    expect(titleMenus.map((menu) => menu.command)).toEqual(
      expect.arrayContaining([
        "solgrid.securityOverview.showSecurity",
        "solgrid.securityOverview.showAll",
        "solgrid.securityOverview.showCompiler",
        "solgrid.securityOverview.showDetectors",
        "solgrid.securityOverview.groupByFile",
        "solgrid.securityOverview.groupBySeverity",
        "solgrid.securityOverview.groupByConfidence",
        "solgrid.securityOverview.groupByFinding",
        "solgrid.coverage.showActionable",
        "solgrid.coverage.showAll",
      ])
    );
  });

  it("offers row actions inline and in the normal context menu by intent", () => {
    const itemMenus = menus["view/item/context"] ?? [];
    const groupsFor = (command: string): string[] =>
      itemMenus
        .filter((menu) => menu.command === command)
        .map((menu) => menu.group ?? "");

    expect(groupsFor("solgrid.securityOverview.openHelp")).toEqual([
      "inline@1",
      "navigation@2",
    ]);
    expect(groupsFor("solgrid.securityOverview.applyFix")).toEqual([
      "inline@2",
      "1_fix@1",
    ]);
    expect(groupsFor("solgrid.securityOverview.suppressNextLine")).toEqual([
      "2_suppress@1",
    ]);
    expect(groupsFor("solgrid.securityOverview.ignoreFinding")).toEqual([
      "3_baseline@1",
    ]);
  });

  it("hides node-dependent commands from the Command Palette", () => {
    const hidden = new Map(
      (menus.commandPalette ?? []).map((menu) => [menu.command, menu.when])
    );
    for (const command of [
      "solgrid.securityOverview.openFinding",
      "solgrid.securityOverview.openHelp",
      "solgrid.securityOverview.suppressNextLine",
      "solgrid.securityOverview.applyFix",
      "solgrid.securityOverview.suppressGroupNextLine",
      "solgrid.securityOverview.applyGroupFixes",
      "solgrid.securityOverview.ignoreFinding",
      "solgrid.securityOverview.restoreFinding",
      "solgrid.securityOverview.ignoreGroup",
      "solgrid.securityOverview.restoreGroup",
    ]) {
      expect(hidden.get(command), command).toBe("false");
    }
  });

  it("disables every coverage command when coverage is disabled", () => {
    const coverageCommands = commands.filter((command) =>
      command.command.startsWith("solgrid.coverage.")
    );
    expect(coverageCommands.length).toBeGreaterThan(0);
    for (const command of coverageCommands) {
      expect(command.enablement, command.command).toBe(
        "config.solgrid.coverage.enable"
      );
    }
  });

  it("keeps the security view registered while the language server starts", () => {
    expect(
      explorerViews.find((view) => view.id === "solgridSecurityOverview")?.when
    ).toBe("config.solgrid.enable");

    const securityCommands = commands.filter((command) =>
      command.command.startsWith("solgrid.securityOverview.")
    );
    expect(securityCommands.length).toBeGreaterThan(0);
    for (const command of securityCommands) {
      expect(command.enablement, command.command).toBe(
        "config.solgrid.enable && solgrid.languageServerActive"
      );
    }
  });

  it("enables imports only for an active local Solidity editor", () => {
    expect(
      commands.find(
        (command) => command.command === "solgrid.graph.showImports"
      )?.enablement
    ).toBe(
      "config.solgrid.enable && solgrid.languageServerActive && editorLangId == solidity && resourceScheme == file"
    );
  });
});
