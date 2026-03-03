# WASM Bindings

The `solgrid_wasm` crate provides WebAssembly bindings for browser and web playground use.

## API

| Function | Description |
|----------|-------------|
| `lint(source, config_json)` | Lint Solidity source, returns JSON diagnostics |
| `fix(source, config_json, include_unsafe)` | Lint and auto-fix, returns fixed source + remaining diagnostics |
| `format(source, config_json)` | Format Solidity source |
| `list_rules()` | List all available lint rules as JSON |
| `version()` | Return the solgrid version string |

## Building

```bash
# Install wasm-pack
cargo install wasm-pack

# Build for web
wasm-pack build crates/solgrid_wasm --target web

# Build for Node.js
wasm-pack build crates/solgrid_wasm --target nodejs
```

## Usage

```javascript
import init, { lint, format, list_rules } from 'solgrid_wasm';

await init();

const diagnostics = lint(soliditySource, '{}');
const formatted = format(soliditySource, '{}');
const rules = list_rules();
```
