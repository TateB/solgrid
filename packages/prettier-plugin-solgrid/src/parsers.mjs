/**
 * Prettier parser for Solidity.
 *
 * Uses solgrid's NAPI bindings to validate syntax. Returns an opaque
 * AST wrapper — actual formatting is handled entirely in Rust.
 */
import { loadBinding } from "./binding.mjs";

const napi = loadBinding();

export const parsers = {
  solgrid: {
    parse(text) {
      const valid = napi.parse(text);
      if (!valid) {
        throw new Error("Failed to parse Solidity source");
      }
      return { type: "solgrid-ast", source: text };
    },
    astFormat: "solgrid-ast",
    locStart: () => 0,
    locEnd: (node) => node.source.length,
  },
};
