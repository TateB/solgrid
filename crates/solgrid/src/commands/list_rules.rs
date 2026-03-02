use solgrid_diagnostics::FixAvailability;
use solgrid_linter::LintEngine;

pub fn run() -> i32 {
    let engine = LintEngine::new();
    let registry = engine.registry();
    let mut metas: Vec<_> = registry.all_meta();
    metas.sort_by_key(|m| m.id);

    println!("{:<35} {:<15} {:<10} {}", "Rule", "Category", "Severity", "Fix");
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
