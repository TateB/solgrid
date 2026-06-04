#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");
const { pack } = require("@vscode/vsce/out/package");

const extensionRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(extensionRoot, "../..");

function takeValue(args, index, flag) {
  const value = args[index + 1];
  if (!value || value.startsWith("-")) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

function parseArgs(args) {
  const options = {
    dependencies: false,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    switch (arg) {
      case "-o":
      case "--out":
        options.packagePath = takeValue(args, index, arg);
        index += 1;
        break;
      case "-t":
      case "--target":
        options.target = takeValue(args, index, arg);
        index += 1;
        break;
      case "--ignore-other-target-folders":
        options.ignoreOtherTargetFolders = true;
        break;
      default:
        throw new Error(`Unsupported argument: ${arg}`);
    }
  }

  return options;
}

function currentTarget() {
  return `${process.platform}-${process.arch}`;
}

function binaryNameForTarget(target) {
  return target.startsWith("win32-") ? "solgrid.exe" : "solgrid";
}

async function exists(filePath) {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

async function stageBundledBinary(options) {
  const target = options.target ?? currentTarget();
  const binDir = path.join(extensionRoot, "bin");
  const bundledPath = path.join(binDir, binaryNameForTarget(target));

  if (await exists(bundledPath)) {
    return async () => {};
  }

  const hostTarget = currentTarget();
  if (target !== hostTarget) {
    console.warn(
      `No bundled binary found for ${target}; cannot auto-stage from host target ${hostTarget}.`
    );
    return async () => {};
  }

  const sourcePath =
    process.env.SOLGRID_BIN ??
    path.join(repoRoot, "target", "release", binaryNameForTarget(hostTarget));

  if (!(await exists(sourcePath))) {
    throw new Error(
      `Missing release binary at ${sourcePath}. Run cargo build --release -p solgrid first, or set SOLGRID_BIN.`
    );
  }

  await fs.mkdir(binDir, { recursive: true });
  await fs.copyFile(sourcePath, bundledPath);
  if (!target.startsWith("win32-")) {
    await fs.chmod(bundledPath, 0o755);
  }
  console.log(`Staged bundled binary: ${bundledPath}`);

  return async () => {
    await fs.rm(bundledPath, { force: true });
    try {
      await fs.rmdir(binDir);
    } catch {}
  };
}

function formatBytes(bytes) {
  if (bytes < 1024) {
    return `${bytes}B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(2)}KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(2)}MB`;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const cleanupBundledBinary = await stageBundledBinary(options);
  try {
    const { files, packagePath } = await pack(options);
    const stats = await fs.stat(packagePath);
    console.log(
      `Packaged: ${path.resolve(packagePath)} (${files.length} files, ${formatBytes(
        stats.size
      )})`
    );
  } finally {
    await cleanupBundledBinary();
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
