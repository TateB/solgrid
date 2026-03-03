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
  solidityContractNewLines: {
    category: "Solidity",
    type: "boolean",
    default: false,
    description: "Add newlines at start/end of contract body.",
  },
};
