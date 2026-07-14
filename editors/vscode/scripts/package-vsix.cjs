#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");
const { pack } = require("@vscode/vsce/out/package");
const { publish } = require("@vscode/vsce/out/publish");

const extensionRoot = path.resolve(__dirname, "..");
const repoRoot = path.resolve(extensionRoot, "../..");

const nativeTargetSpecs = new Map([
  ["linux-x64", { format: "elf", machine: 62 }],
  ["linux-arm64", { format: "elf", machine: 183 }],
  ["darwin-arm64", { format: "macho", machine: 0x0100000c }],
  ["win32-x64", { format: "pe", machine: 0x8664 }],
]);

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
      case "--publish":
        options.publish = true;
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
  requireNativeTargetSpec(target);
  return target.startsWith("win32-") ? "solgrid.exe" : "solgrid";
}

function requireNativeTargetSpec(target) {
  const spec = nativeTargetSpecs.get(target);
  if (!spec) {
    throw new Error(`Unsupported VS Code native target: ${target}`);
  }
  return spec;
}

async function readAt(handle, length, position) {
  const buffer = Buffer.alloc(length);
  const { bytesRead } = await handle.read(buffer, 0, length, position);
  return buffer.subarray(0, bytesRead);
}

async function validateNativeBinary(filePath, target) {
  const spec = requireNativeTargetSpec(target);
  const stats = await fs.stat(filePath);
  if (!stats.isFile()) {
    throw new Error(
      `Refusing to package ${filePath}: source is not a regular file.`
    );
  }
  if (!target.startsWith("win32-") && (stats.mode & 0o111) === 0) {
    throw new Error(
      `Refusing to package ${filePath}: source is not executable for ${target}.`
    );
  }

  const handle = await fs.open(filePath, "r");
  try {
    const header = await readAt(handle, 64, 0);
    let actualFormat;
    let actualMachine;

    if (
      header.length >= 20 &&
      header[0] === 0x7f &&
      header.subarray(1, 4).toString("ascii") === "ELF"
    ) {
      if (header[4] !== 2 || header[5] !== 1) {
        throw new Error("only little-endian 64-bit ELF binaries are supported");
      }
      actualFormat = "elf";
      actualMachine = header.readUInt16LE(18);
    } else if (
      header.length >= 8 &&
      header.readUInt32LE(0) === 0xfeedfacf
    ) {
      actualFormat = "macho";
      actualMachine = header.readUInt32LE(4);
    } else if (
      header.length >= 64 &&
      header[0] === 0x4d &&
      header[1] === 0x5a
    ) {
      const peOffset = header.readUInt32LE(0x3c);
      if (peOffset > stats.size - 6) {
        throw new Error("invalid PE header offset");
      }
      const peHeader = await readAt(handle, 6, peOffset);
      if (
        peHeader.length < 6 ||
        peHeader[0] !== 0x50 ||
        peHeader[1] !== 0x45 ||
        peHeader[2] !== 0 ||
        peHeader[3] !== 0
      ) {
        throw new Error("invalid PE signature");
      }
      actualFormat = "pe";
      actualMachine = peHeader.readUInt16LE(4);
    } else {
      throw new Error("unrecognized native executable format");
    }

    if (actualFormat !== spec.format || actualMachine !== spec.machine) {
      throw new Error(
        `binary format/architecture does not match ${target} (found ${actualFormat} machine 0x${actualMachine.toString(16)})`
      );
    }
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(`Refusing to package ${filePath}: ${reason}`);
  } finally {
    await handle.close();
  }
}

async function exists(filePath) {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}

async function directoryHasEntries(directory) {
  try {
    return (await fs.readdir(directory)).length > 0;
  } catch (error) {
    if (error && error.code === "ENOENT") {
      return false;
    }
    throw error;
  }
}

async function stageBundledBinary(options, context = {}) {
  const activeExtensionRoot = context.extensionRoot ?? extensionRoot;
  const activeRepoRoot = context.repoRoot ?? repoRoot;
  const hostTarget = context.hostTarget ?? currentTarget();
  const explicitBinary = context.solgridBin ?? process.env.SOLGRID_BIN;
  const binDir = path.join(activeExtensionRoot, "bin");
  if (!options.target) {
    if (await directoryHasEntries(binDir)) {
      throw new Error(
        "Refusing to create a generic VSIX with platform-specific files in editors/vscode/bin. Pass --target <platform-arch>, or empty that directory before packaging."
      );
    }
    return async () => {};
  }

  const target = options.target;
  const bundledName = binaryNameForTarget(target);
  for (const flatName of ["solgrid", "solgrid.exe"]) {
    if (await exists(path.join(binDir, flatName))) {
      throw new Error(
        `Refusing to package ${target} from unverified flat bin/${flatName}. Stage target-addressed input at bin/${target}/${bundledName}, or set SOLGRID_BIN.`
      );
    }
  }
  const bundledPath = path.join(binDir, bundledName);
  const targetAddressedPath = path.join(binDir, target, bundledName);
  const hostReleasePath =
    target === hostTarget
      ? path.join(activeRepoRoot, "target", "release", bundledName)
      : undefined;
  const sourcePath = explicitBinary
    ? explicitBinary
    : (await exists(targetAddressedPath))
      ? targetAddressedPath
      : hostReleasePath;

  if (!sourcePath || !(await exists(sourcePath))) {
    throw new Error(
      target === hostTarget
        ? `Missing target-addressed or release binary for ${target}. Run cargo build --release -p solgrid, stage bin/${target}/${bundledName}, or set SOLGRID_BIN.`
        : `Missing target-addressed binary for requested target ${target}. Stage bin/${target}/${bundledName}, or set SOLGRID_BIN to a ${target} binary.`
    );
  }

  await validateNativeBinary(sourcePath, target);
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
  if (options.publish && !options.target) {
    options.target = currentTarget();
  }
  const { publish: shouldPublish, ...packageOptions } = options;
  const cleanupBundledBinary = await stageBundledBinary(packageOptions);
  try {
    const { files, packagePath } = await pack(packageOptions);
    const stats = await fs.stat(packagePath);
    console.log(
      `Packaged: ${path.resolve(packagePath)} (${files.length} files, ${formatBytes(
        stats.size
      )})`
    );
    if (shouldPublish) {
      await publish({ packagePath: [packagePath] });
      console.log(`Published: ${path.resolve(packagePath)}`);
    }
  } finally {
    await cleanupBundledBinary();
  }
}

if (require.main === module) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}

module.exports = {
  binaryNameForTarget,
  stageBundledBinary,
  validateNativeBinary,
};
