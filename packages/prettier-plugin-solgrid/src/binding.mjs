/**
 * Native binding loader.
 *
 * Resolves the platform-specific .node binary built by @napi-rs/cli.
 * Falls back through common locations used during development and CI.
 */
import { createRequire } from "node:module";
import { join, dirname } from "node:path";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

let binding = null;

export function loadBinding() {
  if (binding) return binding;

  // Candidate paths for the native .node binary
  const candidates = [
    // Built by `napi build` into the package root (default name)
    join(__dirname, "..", "index.node"),
    // Alternative name based on crate
    join(__dirname, "..", "solgrid_napi.node"),
    // Platform-specific name pattern used by napi-rs
    join(
      __dirname,
      "..",
      `solgrid_napi.${process.platform}-${process.arch}.node`,
    ),
  ];

  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      binding = require(candidate);
      return binding;
    }
  }

  throw new Error(
    "Could not find solgrid_napi native binding. " +
      "Run `pnpm build:napi` in the prettier-plugin-solgrid directory first.\n" +
      "Searched:\n" +
      candidates.map((c) => `  - ${c}`).join("\n"),
  );
}
