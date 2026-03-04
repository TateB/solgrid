set dotenv-load := false

# Show available recipes
default:
    @just --list

# --- Rust ---

# Check the workspace compiles
check:
    cargo check --workspace --all-targets

# Run all tests
test:
    cargo test --workspace

# Run doc tests
test-doc:
    cargo test --workspace --doc

# Check formatting
fmt:
    cargo fmt --all --check

# Fix formatting
fmt-fix:
    cargo fmt --all

# Run clippy
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run cargo-deny audit
deny:
    cargo deny check

# Run benchmarks (all, or a specific crate e.g. `just bench solgrid_formatter`)
bench crate="":
    #!/usr/bin/env bash
    if [ -z "{{crate}}" ]; then
        cargo bench --workspace
    else
        cargo bench -p "{{crate}}"
    fi

# Build release binary
build:
    cargo build --release -p solgrid

# Build debug binary
dev:
    cargo build -p solgrid

# --- WASM ---

# Build WASM bindings (target: web, nodejs, bundler)
wasm target="web":
    wasm-pack build crates/solgrid_wasm --target {{target}}

# --- Node / pnpm ---

# Install all Node.js dependencies
install:
    pnpm install

# --- VSCode extension ---

# Compile the VSCode extension
vscode-compile: install
    pnpm --filter solgrid-vscode run compile

# Run VSCode unit tests
vscode-test: install
    pnpm --filter solgrid-vscode run compile
    pnpm --filter solgrid-vscode test

# Run LSP integration tests (builds debug binary)
vscode-integration: dev install
    SOLGRID_BIN={{justfile_directory()}}/target/debug/solgrid pnpm --filter solgrid-vscode run test:integration

# Run VSCode e2e tests (builds debug binary)
vscode-e2e: dev install
    pnpm --filter solgrid-vscode run compile:tests
    SOLGRID_BIN={{justfile_directory()}}/target/debug/solgrid node editors/vscode/out/test/e2e/run.js

# Run all VSCode tests
vscode-test-all: dev install
    pnpm --filter solgrid-vscode run compile
    pnpm --filter solgrid-vscode test
    SOLGRID_BIN={{justfile_directory()}}/target/debug/solgrid pnpm --filter solgrid-vscode run test:integration

# Package VSIX (release build)
vscode-package: build install
    pnpm --filter solgrid-vscode run compile
    pnpm --filter solgrid-vscode run package

# --- Prettier plugin ---

# Build NAPI native addon (release)
prettier-build: install
    pnpm --filter prettier-plugin-solgrid run build:napi

# Build NAPI native addon (debug)
prettier-build-debug: install
    pnpm --filter prettier-plugin-solgrid run build:napi:debug

# Run Prettier plugin tests (builds addon first)
prettier-test: prettier-build
    pnpm --filter prettier-plugin-solgrid test

# --- Version ---

# Check/set version: `just version`, `just version set 0.2.0`, `just version write`
version action="" ver="":
    #!/usr/bin/env bash
    case "{{action}}" in
        ""|check) ./scripts/version.sh ;;
        write)    ./scripts/version.sh --write ;;
        set)
            if [ -z "{{ver}}" ]; then
                echo "Usage: just version set X.Y.Z" >&2; exit 1
            fi
            ./scripts/version.sh --set "{{ver}}" ;;
        *) echo "Usage: just version [check|write|set X.Y.Z]" >&2; exit 1 ;;
    esac

# --- CI ---

# Run all Rust CI checks
ci-rust: check test test-doc fmt clippy deny

# Run all CI checks locally
ci: ci-rust vscode-test-all prettier-test
