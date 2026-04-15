# Prettier Plugin

The `prettier-plugin-solgrid` package lets teams already using Prettier adopt solgrid's formatter without changing their workflow. The plugin delegates all formatting to solgrid's Rust formatter via NAPI-RS bindings.

## Installation

```bash
npm install --save-dev prettier prettier-plugin-solgrid
```

## Usage

```bash
npx prettier --write "**/*.sol"
```

## Option Mapping

Standard Prettier options are automatically mapped to solgrid equivalents:

| Prettier Option | solgrid Option |
|-----------------|----------------|
| `printWidth` | `line_length` |
| `tabWidth` | `tab_width` |
| `useTabs` | `use_tabs` |
| `singleQuote` | `single_quote` |
| `bracketSpacing` | `bracket_spacing` |

## Solgrid-Specific Options

Additional options for Solidity formatting:

| Option | Default | Description |
|--------|---------|-------------|
| `solidityNumberUnderscore` | `"preserve"` | Number literal underscores: `"preserve"`, `"thousands"`, `"remove"` |
| `solidityUintType` | `"long"` | Uint representation: `"long"` (uint256), `"short"` (uint), `"preserve"` |
| `soliditySortImports` | `false` | Sort import statements alphabetically |
| `solidityMultilineFuncHeader` | `"attributes_first"` | Multiline function header style |
| `solidityOverrideSpacing` | `true` | Space in override specifiers |
| `solidityWrapComments` | `false` | Wrap comments to fit within printWidth |
| `solidityOperatorLineBreak` | `"leading"` | Multiline binary operator position: `"leading"` or `"trailing"` |
| `solidityContractBodySpacing` | `"preserve"` | Contract body spacing: `"preserve"`, `"single"`, or `"compact"` |
| `solidityInheritanceBraceNewLine` | `true` | Put the opening brace on a new line for wrapped inheritance lists |
| `solidityContractNewLines` | `false` | Deprecated alias. `true` maps to `solidityContractBodySpacing = "single"` |
