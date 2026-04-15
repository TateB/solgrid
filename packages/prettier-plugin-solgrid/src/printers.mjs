/**
 * Prettier printer for Solidity.
 *
 * Delegates all formatting to solgrid's Rust formatter via NAPI.
 * Maps Prettier's resolved options to solgrid's format config.
 */
import { loadBinding } from "./binding.mjs";

const napi = loadBinding();

export const printers = {
  "solgrid-ast": {
    print(path, options) {
      const node = path.getValue();
      const contractBodySpacing =
        options.solidityContractNewLines &&
        options.solidityContractBodySpacing === "preserve"
          ? "single"
          : options.solidityContractBodySpacing;

      const formatted = napi.format(node.source, {
        // Standard Prettier options -> solgrid equivalents
        printWidth: options.printWidth,
        tabWidth: options.tabWidth,
        useTabs: options.useTabs,
        singleQuote: options.singleQuote,
        bracketSpacing: options.bracketSpacing,
        // solgrid-specific options
        numberUnderscore: options.solidityNumberUnderscore,
        uintType: options.solidityUintType,
        overrideSpacing: options.solidityOverrideSpacing,
        wrapComments: options.solidityWrapComments,
        operatorLineBreak: options.solidityOperatorLineBreak,
        sortImports: options.soliditySortImports,
        multilineFuncHeader: options.solidityMultilineFuncHeader,
        contractBodySpacing,
        inheritanceBraceNewLine: options.solidityInheritanceBraceNewLine,
      });

      return formatted;
    },
  },
};
