/**
 * Language definition for Solidity.
 *
 * Registers .sol files to be parsed by the "solgrid" parser.
 */
export const languages = [
  {
    name: "Solidity",
    parsers: ["solgrid"],
    extensions: [".sol"],
    vscodeLanguageIds: ["solidity"],
  },
];
