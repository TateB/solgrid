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
"docs/natspec".comment_style = "triple_slash"
"docs/natspec".continuation_indent = "padded"
"style/category-headers".min_categories = 2
"style/imports-ordering".import_order = ["^forge-std/", "^@openzeppelin/"]

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
# File patterns to include. Omit this key to use the default src/test/script set.
# Set include = [] to disable implicit file discovery entirely.
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
| `recommended` | Default. Enables `security/*`, `best-practices/*`, and `naming/*` |
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
| `docs/natspec` | `comment_style` | `"triple_slash"` | Accept only `///` comments or allow `/** */` with `"either"` |
| `docs/natspec` | `continuation_indent` | `"padded"` | Continuation indent mode: `"padded"` or `"none"` |
| `docs/natspec` | `tags.<tag>.*` | tag-specific | Configure `enabled`, `include`, `exclude`, `skip_internal` for `title`, `author`, `notice`, `dev`, `param`, `return` |
| `docs/selector-tags` | none | n/a | No settings |
| `style/category-headers` | `min_categories` | `2` | Minimum distinct declaration categories before headers are enforced |
| `style/category-headers` | `initialization_functions` | built-in list | Function names treated as initialization sections |
| `style/imports-ordering` | `import_order` | default external/relative grouping | Regex-defined import groups |

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
| `include` | string[] | `["src/**/*.sol", "test/**/*.sol", "script/**/*.sol"]` | File patterns to include |
| `exclude` | string[] | `["lib/**", "node_modules/**", "out/**"]` | File patterns to exclude |
| `respect_gitignore` | bool | `true` | Honor `.gitignore` patterns |
| `threads` | integer | `0` (auto) | Number of parallel threads |
| `cache_dir` | string | `".solgrid_cache"` | Cache directory path |

If `global.include` is omitted, solgrid uses the default include set:
`["src/**/*.sol", "test/**/*.sol", "script/**/*.sol"]`.

If `global.include = []` is set explicitly, solgrid discovers no files unless
you pass file paths directly on the CLI.

## Config Resolution

Configuration is resolved per file. For explicit file paths, solgrid walks up from the file's parent directory to find the nearest `solgrid.toml`. For discovered files under a directory, solgrid uses the config resolved at the traversal root for file discovery and then resolves the nearest config again for each file before linting or formatting it.

Resolution order is:

1. `--config <path>` when provided
2. The nearest `solgrid.toml` discovered by walking upward from the file or traversal root
3. Built-in defaults

Lint suppressions such as `// solgrid-disable-next-line` are applied during analysis, not during config file loading.

## Foundry.toml Compatibility

If no `solgrid.toml` is found, solgrid reads formatting options from `foundry.toml` under `[fmt]`. This provides zero-config adoption for Foundry projects.

## Solhint Migration

```bash
solgrid migrate --from solhint
```

Reads `.solhint.json` (or `.solhintrc`), maps rule names to solgrid equivalents, and writes a `solgrid.toml`.
