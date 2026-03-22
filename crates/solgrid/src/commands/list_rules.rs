use solgrid_diagnostics::FixAvailability;
use solgrid_linter::LintEngine;

pub fn run() -> i32 {
    let engine = LintEngine::new();
    let registry = engine.registry();
    let mut metas: Vec<_> = registry.all_meta();
    metas.sort_by_key(|m| m.id);

    println!("{:<35} {:<15} {:<10} Fix", "Rule", "Category", "Severity");
    println!("{}", "-".repeat(75));

    for meta in metas {
        let fix = match meta.fix_availability {
            FixAvailability::None => "-",
            FixAvailability::Available(safety) => match safety {
                solgrid_diagnostics::FixSafety::Safe => "safe",
                solgrid_diagnostics::FixSafety::Suggestion => "suggestion",
                solgrid_diagnostics::FixSafety::Dangerous => "dangerous",
            },
        };

        println!(
            "{:<35} {:<15} {:<10} {}",
            meta.id,
            meta.category.as_str(),
            meta.default_severity,
            fix
        );
    }

    println!("\n{} rules available", registry.len());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiler_version_listed_severity_matches_runtime_default() {
        let engine = LintEngine::new();
        let rule = engine
            .registry()
            .get("security/compiler-version")
            .expect("compiler-version rule should exist");
        let meta = rule.meta();

        let severity = solgrid_config::Config::default()
            .lint
            .rule_severity(meta.id, meta.default_severity);

        assert_eq!(severity, Some(meta.default_severity));
    }
}
