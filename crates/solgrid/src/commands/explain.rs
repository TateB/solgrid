use solgrid_diagnostics::FixAvailability;
use solgrid_linter::LintEngine;

pub fn run(rule_id: &str) -> i32 {
    let engine = LintEngine::new();
    let registry = engine.registry();

    if let Some(rule) = registry.get(rule_id) {
        let meta = rule.meta();
        println!("Rule: {}", meta.id);
        println!("Category: {}", meta.category);
        println!("Default severity: {}", meta.default_severity);
        println!("Description: {}", meta.description);
        println!(
            "Auto-fix: {}",
            match meta.fix_availability {
                FixAvailability::None => "not available".to_string(),
                FixAvailability::Available(safety) => format!("available ({safety})"),
            }
        );
        println!();
        println!("Configuration:");
        println!("  [lint.rules]");
        println!(
            "  \"{}\" = \"warn\"  # or \"error\", \"info\", \"off\"",
            meta.id
        );
        println!("  [lint.settings.\"{}\"]", meta.id);
        println!("  # rule-specific options vary by rule");
        0
    } else {
        eprintln!("Unknown rule: {rule_id}");
        eprintln!();
        eprintln!("Available rules:");
        for meta in registry.all_meta() {
            eprintln!("  {}", meta.id);
        }
        2
    }
}
