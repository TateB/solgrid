use criterion::{criterion_group, criterion_main, Criterion};
use solgrid_config::Config;
use solgrid_linter::LintEngine;

/// Benchmark the initialization cost: creating a LintEngine with all rules
/// registered and a default config. This measures the "startup" overhead
/// before any files are linted.
fn bench_engine_initialization(c: &mut Criterion) {
    c.bench_function("engine_initialization", |b| {
        b.iter(|| {
            let engine = LintEngine::new();
            let config = Config::default();
            // Return both to prevent optimization
            (engine.registry().rules().len(), config.lint.preset)
        })
    });
}

/// Benchmark config parsing from a TOML string (simulates reading solgrid.toml).
fn bench_config_parse(c: &mut Criterion) {
    let config_toml = r#"
[lint]
preset = "recommended"

[lint.rules]
"security/tx-origin" = "error"
"gas/custom-errors" = "warn"
"naming/const-name-snakecase" = "off"

[format]
line_length = 120
tab_width = 4
use_tabs = false
single_quote = false
bracket_spacing = false

[global]
exclude = ["lib/**", "node_modules/**"]
"#;

    c.bench_function("config_parse_toml", |b| {
        b.iter(|| {
            let config: Config = toml::from_str(config_toml).unwrap();
            config
        })
    });
}

/// Benchmark a minimal end-to-end: init engine + lint one small file.
/// This represents the minimum latency a user sees.
fn bench_minimal_e2e(c: &mut Criterion) {
    let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test { uint256 public x; }
"#;

    c.bench_function("minimal_e2e_single_file", |b| {
        b.iter(|| {
            let engine = LintEngine::new();
            let config = Config::default();
            engine.lint_source(source, std::path::Path::new("test.sol"), &config)
        })
    });
}

criterion_group!(
    benches,
    bench_engine_initialization,
    bench_config_parse,
    bench_minimal_e2e
);
criterion_main!(benches);
