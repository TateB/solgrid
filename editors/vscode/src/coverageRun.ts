import * as vscode from "vscode";
import * as path from "node:path";
import { CoverageExtensionConfig } from "./config";

export type CoverageRunKind =
  | "foundry-lcov"
  | "foundry-cobertura"
  | "hardhat-lcov"
  | "custom";

export interface CoverageRunSpec {
  kind: CoverageRunKind;
  label: string;
  command: string;
  args: string[];
}

interface CoverageFolderLike {
  name: string;
  uri: { fsPath: string };
}

export interface CoverageProviderAvailability {
  hasFoundry: boolean;
  hasHardhat: boolean;
  hasCustomCommand: boolean;
}

export function coverageRunSpec(
  kind: CoverageRunKind,
  config: CoverageExtensionConfig
): CoverageRunSpec | undefined {
  switch (kind) {
    case "foundry-lcov":
      return {
        kind,
        label: "Foundry Coverage (LCOV)",
        command: "forge",
        args: ["coverage", "--report", "lcov"],
      };
    case "foundry-cobertura":
      return {
        kind,
        label: "Foundry Coverage (Cobertura)",
        command: "forge",
        args: ["coverage", "--report", "cobertura"],
      };
    case "hardhat-lcov":
      return {
        kind,
        label: "Hardhat Coverage (LCOV)",
        command: "npx",
        args: ["hardhat", "coverage"],
      };
    case "custom": {
      const [command, ...args] = config.customCommand;
      if (!command) {
        return undefined;
      }
      return {
        kind,
        label: "Custom Coverage Command",
        command,
        args,
      };
    }
  }
}

export function availableCoverageRunSpecs(
  availability: CoverageProviderAvailability,
  config: CoverageExtensionConfig
): CoverageRunSpec[] {
  const specs: CoverageRunSpec[] = [];
  if (availability.hasFoundry) {
    specs.push(coverageRunSpec("foundry-lcov", config)!);
    specs.push(coverageRunSpec("foundry-cobertura", config)!);
  }
  if (availability.hasHardhat) {
    specs.push(coverageRunSpec("hardhat-lcov", config)!);
  }
  if (availability.hasCustomCommand) {
    const custom = coverageRunSpec("custom", config);
    if (custom) {
      specs.push(custom);
    }
  }
  return specs;
}

export function preferredCoverageRunSpec(
  specs: readonly CoverageRunSpec[]
): CoverageRunSpec | undefined {
  return (
    specs.find((spec) => spec.kind === "foundry-lcov") ??
    specs.find((spec) => spec.kind === "hardhat-lcov") ??
    specs[0]
  );
}

export function preferredCoverageWorkspaceFolder<T extends CoverageFolderLike>(
  folders: readonly T[],
  activeDocumentPath: string | undefined,
  findWorkspaceFolder: (filePath: string) => T | undefined
): T | undefined {
  if (activeDocumentPath) {
    const activeFolder = findWorkspaceFolder(activeDocumentPath);
    if (activeFolder) {
      return activeFolder;
    }
  }
  if (folders.length === 1) {
    return folders[0];
  }
  return undefined;
}

export async function runCoverageCommand(
  kind: CoverageRunKind,
  config: CoverageExtensionConfig,
  refreshCoverage: () => Promise<void>
): Promise<void> {
  const spec = coverageRunSpec(kind, config);
  if (!spec) {
    void vscode.window.showWarningMessage(
      "Set solgrid.coverage.customCommand before running the custom coverage command."
    );
    return;
  }

  const folder = await pickCoverageWorkspaceFolder();
  if (!folder) {
    void vscode.window.showWarningMessage(
      "Open a workspace folder or focus a file inside one before running coverage."
    );
    return;
  }
  await runCoverageSpec(spec, folder, config, refreshCoverage);
}

export async function runPreferredCoverageCommand(
  config: CoverageExtensionConfig,
  refreshCoverage: () => Promise<void>
): Promise<void> {
  const folder = await pickCoverageWorkspaceFolder();
  if (!folder) {
    void vscode.window.showWarningMessage(
      "Open a workspace folder or focus a file inside one before running coverage."
    );
    return;
  }

  const specs = availableCoverageRunSpecs(
    await detectCoverageProviderAvailability(folder, config),
    config
  );
  if (specs.length === 0) {
    void vscode.window.showWarningMessage(
      "No supported coverage provider was detected for this workspace. Configure solgrid.coverage.customCommand or open a Foundry project."
    );
    return;
  }

  const selected =
    specs.length === 1
      ? specs[0]
      : await pickCoverageRunSpec(specs, preferredCoverageRunSpec(specs));
  if (!selected) {
    return;
  }

  await runCoverageSpec(selected, folder, config, refreshCoverage);
}

async function pickCoverageWorkspaceFolder(): Promise<vscode.WorkspaceFolder | undefined> {
  const folders = vscode.workspace.workspaceFolders ?? [];
  if (folders.length === 0) {
    return undefined;
  }

  const activeDocumentPath =
    vscode.window.activeTextEditor?.document.uri.scheme === "file"
      ? vscode.window.activeTextEditor.document.uri.fsPath
      : undefined;
  const preferred = preferredCoverageWorkspaceFolder(
    folders,
    activeDocumentPath,
    (filePath) => vscode.workspace.getWorkspaceFolder(vscode.Uri.file(filePath))
  );
  if (preferred) {
    return preferred;
  }

  if (folders.length === 1) {
    return folders[0];
  }

  return vscode.window.showWorkspaceFolderPick({
    placeHolder: "Select the workspace folder to run coverage in",
  });
}

async function pickCoverageRunSpec(
  specs: readonly CoverageRunSpec[],
  preferred: CoverageRunSpec | undefined
): Promise<CoverageRunSpec | undefined> {
  const items = specs.map((spec) => ({
    label: spec.label,
    description:
      spec === preferred ? "recommended" : `${spec.command} ${spec.args.join(" ")}`,
    spec,
  }));
  const selected = await vscode.window.showQuickPick(items, {
    placeHolder: "Select a coverage command to run",
  });
  return selected?.spec;
}

async function detectCoverageProviderAvailability(
  folder: vscode.WorkspaceFolder,
  config: CoverageExtensionConfig
): Promise<CoverageProviderAvailability> {
  return {
    hasFoundry: await workspaceFileExists(folder, "foundry.toml"),
    hasHardhat: await workspaceHasAnyFile(folder, [
      "hardhat.config.ts",
      "hardhat.config.js",
      "hardhat.config.mjs",
      "hardhat.config.cjs",
    ]),
    hasCustomCommand: config.customCommand.length > 0,
  };
}

async function workspaceHasAnyFile(
  folder: vscode.WorkspaceFolder,
  relativePaths: readonly string[]
): Promise<boolean> {
  for (const relativePath of relativePaths) {
    if (await workspaceFileExists(folder, relativePath)) {
      return true;
    }
  }
  return false;
}

async function workspaceFileExists(
  folder: vscode.WorkspaceFolder,
  relativePath: string
): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(
      vscode.Uri.file(path.join(folder.uri.fsPath, relativePath))
    );
    return true;
  } catch {
    return false;
  }
}

async function runCoverageSpec(
  spec: CoverageRunSpec,
  folder: vscode.WorkspaceFolder,
  config: CoverageExtensionConfig,
  refreshCoverage: () => Promise<void>
): Promise<void> {
  const task = new vscode.Task(
    {
      type: "solgrid-coverage",
      command: spec.command,
      args: spec.args,
      kind: spec.kind,
    },
    folder,
    spec.label,
    "solgrid",
    new vscode.ProcessExecution(spec.command, spec.args, {
      cwd: folder.uri.fsPath,
    }),
    []
  );
  task.presentationOptions = {
    reveal: vscode.TaskRevealKind.Always,
    panel: vscode.TaskPanelKind.Shared,
    clear: false,
    focus: false,
  };

  const execution = await vscode.tasks.executeTask(task);
  const pending = waitForTaskExecution(execution).then(async (exitCode) => {
    if (exitCode === 0 || exitCode === undefined) {
      if (config.autoRefreshAfterRun) {
        await refreshCoverage();
      }
      return;
    }

    void vscode.window.showWarningMessage(
      `Coverage command "${spec.label}" exited with code ${exitCode}.`
    );
  });
  void vscode.window.setStatusBarMessage(`solgrid: running ${spec.label}`, pending);
  await pending;
}

function waitForTaskExecution(
  execution: vscode.TaskExecution
): Promise<number | undefined> {
  return new Promise((resolve) => {
    let settled = false;
    const disposables: vscode.Disposable[] = [];

    const settle = (exitCode: number | undefined): void => {
      if (settled) {
        return;
      }
      settled = true;
      for (const disposable of disposables) {
        disposable.dispose();
      }
      resolve(exitCode);
    };

    disposables.push(
      vscode.tasks.onDidEndTaskProcess((event) => {
        if (event.execution === execution) {
          settle(event.exitCode);
        }
      }),
      vscode.tasks.onDidEndTask((event) => {
        if (event.execution === execution) {
          settle(undefined);
        }
      })
    );
  });
}
