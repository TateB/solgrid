import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const packageDir = path.resolve(scriptDir, "..");
const repoRoot = path.resolve(packageDir, "../..");
const binaryName = process.platform === "win32" ? "solgrid.exe" : "solgrid";

const rawArgs = process.argv.slice(2);
const separatorIndex = rawArgs.indexOf("--");
const args = separatorIndex === -1 ? rawArgs : rawArgs.slice(0, separatorIndex);
const vitestArgs = separatorIndex === -1 ? [] : rawArgs.slice(separatorIndex + 1);

const wantsRelease = args.includes("--release");
const wantsDebug = args.includes("--debug");
const noBuild = args.includes("--no-build");

if (wantsRelease && wantsDebug) {
  console.error("Choose only one of --debug or --release.");
  process.exit(1);
}

const mode = wantsRelease ? "release" : "debug";
const explicitBinary = process.env.SOLGRID_BIN;
const binaryPath =
  explicitBinary ?? path.join(repoRoot, "target", mode, binaryName);

if (!explicitBinary && !noBuild) {
  const cargoArgs =
    mode === "release"
      ? ["build", "--release", "-p", "solgrid", "--bin", "solgrid"]
      : ["build", "-p", "solgrid", "--bin", "solgrid"];
  runOrThrow("cargo", cargoArgs, repoRoot);
}

if (!fs.existsSync(binaryPath)) {
  console.error(`solgrid binary not found at ${binaryPath}`);
  process.exit(1);
}

console.log(`Running LSP integration tests with ${binaryPath}`);

const vitestResult = runOrThrow(
  process.execPath,
  [
    path.join(packageDir, "node_modules", "vitest", "vitest.mjs"),
    "run",
    "--project",
    "lsp-integration",
    ...vitestArgs,
  ],
  packageDir,
  {
    ...process.env,
    SOLGRID_BIN: binaryPath,
  }
);

process.exit(vitestResult.status ?? 1);

function runOrThrow(command, commandArgs, cwd, env = process.env) {
  const result = spawnSync(command, commandArgs, {
    cwd,
    env,
    stdio: "inherit",
  });

  if (result.error) {
    console.error(result.error.message);
    process.exit(1);
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }

  return result;
}
