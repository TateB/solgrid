# Configuration

solgrid uses `solgrid.toml` for configuration, following the Foundry ecosystem convention. Config is discovered by walking up from the file being linted to the filesystem root.

## Full Example

```toml
# solgrid.toml

[lint]
# Rule selection preset: "all", "recommended" (default), or "security-only"
preset = "recommended"

# Enable/disable specific rules
# Values: "error", "warn", "info", "off"
[lint.rules]
"security/tx-origin" = "error"
"gas/cache-array-length" = "off"
"naming/private-vars-underscore" = "warn"

# Rule-specific configuration
[lint.settings]
"best-practices/code-complexity".threshold = 10
"best-practices/function-max-lines".max_lines = 60
"best-practices/max-states-count".max_count = 20
"security/compiler-version".allowed = [">=0.8.19", "<0.9.0"]
"naming/foundry-test-functions".pattern = "test(Fork)?(Fuzz)?(Fail)?_"
"style/max-line-length".limit = 120

[format]
line_length = 120
tab_width = 4
use_tabs = false
single_quote = false
bracket_spacing = false
number_underscore = "preserve"
uint_type = "uint256"
sort_imports = false
multiline_func_header = "attributes_first"

[global]
# Solidity version (auto-detected from pragma if omitted)
solidity_version = "0.8.24"
# File patterns to include
include = ["src/**/*.sol", "test/**/*.sol", "script/**/*.sol"]
# File patterns to exclude
exclude = ["lib/**", "node_modules/**", "out/**"]
# Respect .gitignore
respect_gitignore = true
# Number of threads (0 = auto)
threads = 0
# Cache directory
cache_dir = ".solgrid_cache"
```

## Lint Configuration

### Presets

| Preset | Description |
|--------|-------------|
| `recommended` | Default. Enables rules the solgrid team considers essential |
| `all` | Enable every rule |
| `security-only` | Only security category rules |

### Rule Severity Levels

Rules can be set to `"error"`, `"warn"`, `"info"`, or `"off"` in `[lint.rules]`.

### Rule-Specific Settings

Some rules accept additional configuration in `[lint.settings]`:

| Rule | Setting | Default | Description |
|------|---------|---------|-------------|
| `best-practices/code-complexity` | `threshold` | `10` | Max cyclomatic complexity |
| `best-practices/function-max-lines` | `max_lines` | `50` | Max function line count |
| `best-practices/max-states-count` | `max_count` | `15` | Max state variables per contract |
| `security/compiler-version` | `allowed` | `[">=0.8.19", "<0.9.0"]` | Allowed compiler versions |
| `naming/foundry-test-functions` | `pattern` | `"test(Fork)?(Fuzz)?(Fail)?_"` | Test function regex |
| `style/max-line-length` | `limit` | `120` | Max line length |

## Format Configuration

All formatter options live under `[format]`:

| Option | Type | Default | forge fmt | prettier |
|--------|------|---------|-----------|----------|
| `line_length` | integer | 120 | `line_length` | `printWidth` |
| `tab_width` | integer | 4 | `tab_width` | `tabWidth` |
| `use_tabs` | bool | false | -- | `useTabs` |
| `single_quote` | bool | false | `quote_style` | `singleQuote` |
| `bracket_spacing` | bool | false | `bracket_spacing` | `bracketSpacing` |
| `number_underscore` | `"thousands"` / `"remove"` / `"preserve"` | `"preserve"` | `number_underscore` | -- |
| `uint_type` | `"uint256"` / `"uint"` / `"preserve"` | `"uint256"` | `int_types` | -- |
| `override_spacing` | bool | true | `override_spacing` | -- |
| `wrap_comments` | bool | false | `wrap_comments` | -- |
| `sort_imports` | bool | false | `sort_imports` | -- |
| `multiline_func_header` | `"attributes_first"` / `"params_first"` / `"all"` | `"attributes_first"` | `multiline_func_header` | -- |
| `contract_body_spacing` | `"preserve"` / `"single"` / `"compact"` | `"preserve"` | `contract_new_lines`* | -- |
| `inheritance_brace_new_line` | bool | true | -- | -- |

### Formatter Directives

```solidity
// solgrid-fmt: off
// ... this code is not formatted ...
// solgrid-fmt: on

// Also supports forge fmt compatibility:
// forgefmt: disable-next-line
```

## Global Configuration

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `solidity_version` | string | auto-detected | Solidity version hint |
| `include` | string[] | `["src/**/*.sol", "test/**/*.sol", "script/**/*.sol"]` | File patterns to include |
| `exclude` | string[] | `["lib/**", "node_modules/**", "out/**"]` | File patterns to exclude |
| `respect_gitignore` | bool | `true` | Honor `.gitignore` patterns |
| `threads` | integer | `0` (auto) | Number of parallel threads |
| `cache_dir` | string | `".solgrid_cache"` | Cache directory path |

## Config Resolution Order

Configuration is resolved in this priority (highest first):

1. CLI flags (`--rule`, `--fix`, etc.)
2. Inline comments (`// solgrid-disable-next-line`)
3. `solgrid.toml` in the closest parent directory
4. `solgrid.toml` in the project root
5. `~/.config/solgrid/solgrid.toml` (global user config)
6. Built-in defaults

## Foundry.toml Compatibility

If no `solgrid.toml` is found, solgrid reads formatting options from `foundry.toml` under `[fmt]`. This provides zero-config adoption for Foundry projects.

## Solhint Migration

```bash
solgrid migrate --from solhint
```

Reads `.solhint.json` (or `.solhintrc`), maps rule names to solgrid equivalents, and writes a `solgrid.toml`.
