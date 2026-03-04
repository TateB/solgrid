/**
 * Native binding loader.
 *
 * Resolves the platform-specific .node binary built by @napi-rs/cli.
 *
 * Resolution order:
 *   1. Local .node files in the package root (development / `pnpm build:napi`)
 *   2. Platform-specific @solgrid/napi-{platform}-{arch} package (npm install)
 */
import { createRequire } from "node:module";
import { join, dirname } from "node:path";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

let binding = null;

const PLATFORM_PACKAGES = {
  "darwin-arm64": "@solgrid/napi-darwin-arm64",
  "darwin-x64": "@solgrid/napi-darwin-x64",
  "linux-x64": "@solgrid/napi-linux-x64",
  "win32-x64": "@solgrid/napi-win32-x64",
};

export function loadBinding() {
  if (binding) return binding;

  // 1. Try local .node files (development builds)
  const localCandidates = [
    join(__dirname, "..", "solgrid_napi.node"),
    join(__dirname, "..", "index.node"),
  ];

  for (const candidate of localCandidates) {
    if (existsSync(candidate)) {
      binding = require(candidate);
      return binding;
    }
  }

  // 2. Try platform-specific npm package
  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORM_PACKAGES[key];

  if (!pkg) {
    throw new Error(
      `Unsupported platform: ${process.platform}-${process.arch}. ` +
        `Supported platforms: ${Object.keys(PLATFORM_PACKAGES).join(", ")}`,
    );
  }

  try {
    binding = require(`${pkg}/solgrid_napi.node`);
    return binding;
  } catch {
    throw new Error(
      `Could not load solgrid native binding for ${key}.\n` +
        `The platform package "${pkg}" is not installed.\n` +
        `Try reinstalling with: npm install prettier-plugin-solgrid\n` +
        `Or for local development: pnpm build:napi`,
    );
  }
}
