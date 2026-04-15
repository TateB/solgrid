/**
 * Prettier option definitions for solgrid-specific formatting options.
 *
 * Standard Prettier options (printWidth, tabWidth, useTabs, singleQuote,
 * bracketSpacing) are automatically available and mapped in the printer.
 * These options cover solgrid-specific formatting behavior.
 */
export const options = {
  solidityNumberUnderscore: {
    category: "Solidity",
    type: "choice",
    default: "preserve",
    description: "How to handle underscores in number literals.",
    choices: [
      { value: "preserve", description: "Don't change" },
      { value: "thousands", description: "Add underscore separators" },
      { value: "remove", description: "Remove all underscores" },
    ],
  },
  solidityUintType: {
    category: "Solidity",
    type: "choice",
    default: "long",
    description: "Preferred uint type representation.",
    choices: [
      { value: "long", description: "Use uint256 (long form)" },
      { value: "short", description: "Use uint (short form)" },
      { value: "preserve", description: "Don't change" },
    ],
  },
  solidityOverrideSpacing: {
    category: "Solidity",
    type: "boolean",
    default: true,
    description: "Add space in override specifiers.",
  },
  solidityWrapComments: {
    category: "Solidity",
    type: "boolean",
    default: false,
    description: "Wrap comments to fit within printWidth.",
  },
  solidityOperatorLineBreak: {
    category: "Solidity",
    type: "choice",
    default: "leading",
    description: "Place multiline binary operators at the start or end of wrapped lines.",
    choices: [
      { value: "leading", description: "Put operators at the start of continued lines" },
      { value: "trailing", description: "Put operators at the end of continued lines" },
    ],
  },
  soliditySortImports: {
    category: "Solidity",
    type: "boolean",
    default: false,
    description: "Sort import statements alphabetically.",
  },
  solidityMultilineFuncHeader: {
    category: "Solidity",
    type: "choice",
    default: "attributes_first",
    description: "How to break long function headers across lines.",
    choices: [
      { value: "attributes_first", description: "Break at attributes first" },
      { value: "params_first", description: "Break at parameters first" },
      { value: "all", description: "Break everything" },
    ],
  },
  solidityContractBodySpacing: {
    category: "Solidity",
    type: "choice",
    default: "preserve",
    description: "Spacing between declarations inside contract bodies.",
    choices: [
      { value: "preserve", description: "Keep existing blank lines" },
      { value: "single", description: "Insert a single blank line" },
      { value: "compact", description: "Remove blank lines" },
    ],
  },
  solidityInheritanceBraceNewLine: {
    category: "Solidity",
    type: "boolean",
    default: true,
    description: "Put the opening brace on a new line for wrapped inheritance lists.",
  },
  solidityContractNewLines: {
    category: "Solidity",
    type: "boolean",
    default: false,
    description: "Deprecated alias for solidityContractBodySpacing = \"single\" when true.",
  },
};
