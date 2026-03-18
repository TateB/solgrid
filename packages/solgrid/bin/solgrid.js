#!/usr/bin/env node

"use strict";

const { spawnSync } = require("child_process");
const { platform, arch, env, argv, exit } = require("process");

const PLATFORMS = {
  "darwin-arm64": "@solgrid/cli-darwin-arm64",
  "linux-arm64": "@solgrid/cli-linux-arm64",
  "linux-x64": "@solgrid/cli-linux-x64",
  "win32-x64": "@solgrid/cli-win32-x64",
};

function getBinaryPath() {
  // Allow override via environment variable
  if (env.SOLGRID_BINARY) {
    return env.SOLGRID_BINARY;
  }

  const key = `${platform}-${arch}`;
  const pkg = PLATFORMS[key];

  if (!pkg) {
    console.error(
      `solgrid: unsupported platform ${platform}-${arch}\n` +
        `Supported platforms: ${Object.keys(PLATFORMS).join(", ")}`
    );
    exit(1);
  }

  const ext = platform === "win32" ? ".exe" : "";
  const binaryName = `solgrid${ext}`;

  try {
    return require.resolve(`${pkg}/${binaryName}`);
  } catch {
    console.error(
      `solgrid: could not find the binary for ${platform}-${arch}\n\n` +
        `The platform-specific package "${pkg}" is not installed.\n` +
        `This can happen if your package manager was configured to skip optional dependencies.\n\n` +
        `To fix this, try:\n` +
        `  npm install ${pkg}\n\n` +
        `Or download the binary directly from:\n` +
        `  https://github.com/TateB/solgrid/releases`
    );
    exit(1);
  }
}

const binary = getBinaryPath();
const result = spawnSync(binary, argv.slice(2), {
  stdio: "inherit",
});

if (result.error) {
  console.error(`solgrid: failed to execute binary: ${result.error.message}`);
  exit(1);
}

exit(result.status ?? 1);
