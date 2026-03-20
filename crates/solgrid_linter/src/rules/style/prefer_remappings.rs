//! Rule: style/prefer-remappings
//!
//! Suggest using remapped import paths instead of relative imports.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::ItemKind;
use solgrid_parser::with_parsed_ast_sequential;
use std::path::{Component, Path, PathBuf};

static META: RuleMeta = RuleMeta {
    id: "style/prefer-remappings",
    name: "prefer-remappings",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "prefer remapped import paths over relative imports",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct PreferRemappingsRule;

impl Rule for PreferRemappingsRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        if ctx.remappings.is_empty() {
            return Vec::new();
        }

        let remappings = ctx.remappings;
        let file_dir = match ctx.path.parent() {
            Some(dir) => dir,
            None => return Vec::new(),
        };

        let filename = ctx
            .path
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "buffer.sol".to_string());

        let mut diagnostics = Vec::new();

        let _ = with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            for item in source_unit.items.iter() {
                if let ItemKind::Import(import) = &item.kind {
                    let import_path = import.path.value.as_str();

                    // Only check relative imports
                    if !import_path.starts_with('.') {
                        continue;
                    }

                    // Resolve the relative path
                    let resolved = normalize_path(&file_dir.join(import_path));

                    // Find best matching remapping (longest target)
                    if let Some(remapped) = find_remapped_path(&resolved, remappings) {
                        let item_range = solgrid_ast::span_to_range(item.span);
                        let item_source = &ctx.source[item_range.clone()];

                        // Find the path within the import statement
                        if let Some(path_offset) = find_path_in_import(item_source, import_path) {
                            let abs_start = item_range.start + path_offset;
                            let abs_end = abs_start + import_path.len();

                            let mut diag = Diagnostic::new(
                                META.id,
                                format!("use `{remapped}` instead of relative import"),
                                META.default_severity,
                                abs_start..abs_end,
                            );
                            diag = diag.with_fix(Fix::suggestion(
                                format!("Replace with `{remapped}`"),
                                vec![TextEdit::replace(abs_start..abs_end, remapped)],
                            ));
                            diagnostics.push(diag);
                        }
                    }
                }
            }
        });

        diagnostics
    }
}

/// Normalize a path by collapsing `.` and `..` components without filesystem access.
fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<Component> = Vec::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                // Only pop if the last component is a normal component
                if matches!(parts.last(), Some(Component::Normal(_))) {
                    parts.pop();
                } else {
                    parts.push(c);
                }
            }
            Component::CurDir => {}
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}

/// Find a remapped path for the given resolved absolute path.
/// Returns the remapped import string if a matching remapping is found.
fn find_remapped_path(resolved: &Path, remappings: &[(String, PathBuf)]) -> Option<String> {
    let mut best: Option<(&str, &Path, usize)> = None;

    for (prefix, target) in remappings {
        // Normalize the target for comparison
        let norm_target = normalize_path(target);

        if let Ok(rest) = resolved.strip_prefix(&norm_target) {
            let target_len = norm_target.as_os_str().len();
            let _ = rest;
            match best {
                None => best = Some((prefix, target, target_len)),
                Some((_, _, prev_len)) if target_len > prev_len => {
                    best = Some((prefix, target, target_len));
                }
                _ => {}
            }
        }
    }

    let (prefix, target, _) = best?;
    let norm_target = normalize_path(target);
    let rest = resolved.strip_prefix(&norm_target).ok()?;
    // Convert rest to forward-slash path for Solidity imports
    let rest_str = rest.to_string_lossy().replace('\\', "/");
    Some(format!("{prefix}{rest_str}"))
}

/// Find the byte offset of the import path string within an import statement.
fn find_path_in_import(import_text: &str, path: &str) -> Option<usize> {
    // Look for the path between quotes
    for quote in ['"', '\''] {
        if let Some(pos) = import_text.find(&format!("{quote}{path}{quote}")) {
            return Some(pos + 1); // skip the opening quote
        }
    }
    None
}
