//! Rule: style/import-path-format
//!
//! Enforce consistent import path format (relative vs absolute).
//! Reports if a file mixes relative (starting with `.`) and absolute import paths.

use crate::context::LintContext;
use crate::rule::Rule;
use solgrid_diagnostics::*;

static META: RuleMeta = RuleMeta {
    id: "style/import-path-format",
    name: "import-path-format",
    category: RuleCategory::Style,
    default_severity: Severity::Info,
    description: "import paths should use a consistent format (all relative or all absolute)",
    fix_availability: FixAvailability::Available(FixSafety::Suggestion),
};

pub struct ImportPathFormatRule;

impl Rule for ImportPathFormatRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Collect all import paths with their locations
        let mut relative_imports: Vec<(usize, usize, String)> = Vec::new();
        let mut absolute_imports: Vec<(usize, usize, String)> = Vec::new();
        let mut offset = 0;

        for line in ctx.source.split('\n') {
            let trimmed = line.trim();
            if trimmed.starts_with("import ") {
                if let Some(path) = extract_import_path(trimmed) {
                    let entry = (offset, offset + line.len(), path.clone());
                    if path.starts_with('.') {
                        relative_imports.push(entry);
                    } else {
                        absolute_imports.push(entry);
                    }
                }
            }
            offset += line.len() + 1;
        }

        // Only flag if there's a mix — flag the minority style
        if !relative_imports.is_empty() && !absolute_imports.is_empty() {
            let (minority, majority_style) = if relative_imports.len() <= absolute_imports.len() {
                (&relative_imports, "absolute")
            } else {
                (&absolute_imports, "relative")
            };

            for (start, end, path) in minority {
                let fix = build_path_fix(ctx.source, *start, *end, path, majority_style);

                let mut diag = Diagnostic::new(
                    META.id,
                    format!(
                        "import path `{path}` should use {majority_style} format for consistency"
                    ),
                    META.default_severity,
                    *start..*end,
                );
                if let Some(fix) = fix {
                    diag = diag.with_fix(fix);
                }
                diagnostics.push(diag);
            }
        }

        diagnostics
    }
}

/// Build a fix that converts an import path to the target style.
fn build_path_fix(
    source: &str,
    line_start: usize,
    line_end: usize,
    path: &str,
    target_style: &str,
) -> Option<Fix> {
    let line_text = &source[line_start..line_end];

    // Find the path string within the line (between quotes)
    let path_offset = line_text.find(path)?;
    let abs_path_start = line_start + path_offset;
    let abs_path_end = abs_path_start + path.len();

    let new_path = match target_style {
        "absolute" => {
            if path.starts_with("../") {
                return None;
            }

            let mut p = path;
            let mut changed = false;
            while let Some(stripped) = p.strip_prefix("./") {
                p = stripped;
                changed = true;
            }

            if !changed || p.is_empty() {
                return None;
            }

            p.to_string()
        }
        "relative" => {
            if path.starts_with('@') || path.contains('/') {
                return None;
            }

            format!("./{path}")
        }
        _ => return None,
    };

    if new_path == path {
        return None;
    }

    Some(Fix::suggestion(
        format!("Convert to {target_style} import path"),
        vec![TextEdit::replace(abs_path_start..abs_path_end, new_path)],
    ))
}

/// Extract the import path from an import statement.
fn extract_import_path(line: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if let Some(start) = line.rfind(quote) {
            let before = &line[..start];
            if let Some(path_start) = before.rfind(quote) {
                let path = &line[path_start + 1..start];
                return Some(path.to_string());
            }
        }
    }
    None
}
