//! Rule: docs/natspec
//!
//! Enforce NatSpec presence, tag validation, and formatting.

use crate::context::LintContext;
use crate::rule::Rule;
use serde::Deserialize;
use solgrid_ast::natspec::{
    find_attached_natspec, render_triple_slash_block, NatSpecBlock, NatSpecStyle,
};
use solgrid_diagnostics::*;
use solgrid_parser::solar_ast::{
    ContractKind, FunctionKind, Item, ItemFunction, ItemKind, StateMutability, Visibility,
};
use solgrid_parser::with_parsed_ast_sequential;

static META: RuleMeta = RuleMeta {
    id: "docs/natspec",
    name: "natspec",
    category: RuleCategory::Docs,
    default_severity: Severity::Info,
    description: "NatSpec comments should use the configured style and include the required tags",
    fix_availability: FixAvailability::Available(FixSafety::Safe),
};

pub struct NatspecRule;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CommentStyleSetting {
    #[default]
    TripleSlash,
    Either,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ContinuationIndentSetting {
    #[default]
    Padded,
    None,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct NatspecTagSettings {
    title: TagRuleConfig,
    author: TagRuleConfig,
    notice: TagRuleConfig,
    dev: TagRuleConfig,
    param: TagRuleConfig,
    #[serde(rename = "return")]
    return_tag: TagRuleConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct TagRuleConfig {
    enabled: Option<bool>,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    skip_internal: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct NatspecSettings {
    comment_style: CommentStyleSetting,
    continuation_indent: ContinuationIndentSetting,
    tags: NatspecTagSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TagKind {
    Title,
    Author,
    Notice,
    Dev,
    Param,
    Return,
}

impl TagKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Author => "author",
            Self::Notice => "notice",
            Self::Dev => "dev",
            Self::Param => "param",
            Self::Return => "return",
        }
    }

    fn display(self) -> &'static str {
        match self {
            Self::Title => "@title",
            Self::Author => "@author",
            Self::Notice => "@notice",
            Self::Dev => "@dev",
            Self::Param => "@param",
            Self::Return => "@return",
        }
    }

    fn default_enabled(self) -> bool {
        matches!(self, Self::Notice | Self::Param | Self::Return)
    }
}

#[derive(Debug, Clone, Copy)]
enum SubjectKind {
    Contract(ContractKind),
    Function,
    Event,
    Variable,
}

#[derive(Debug, Clone)]
struct Subject {
    kind: SubjectKind,
    name: String,
    span: std::ops::Range<usize>,
    context: String,
    block: Option<NatSpecBlock>,
    params: Vec<Option<String>>,
    returns: Vec<Option<String>>,
    simple_getter: bool,
}

#[derive(Debug, Clone)]
struct ParsedTagLine {
    kind: String,
    arg: Option<String>,
}

impl Rule for NatspecRule {
    fn meta(&self) -> &RuleMeta {
        &META
    }

    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        let settings: NatspecSettings = ctx.config.rule_settings(META.id);
        let filename = ctx.path.to_string_lossy().to_string();

        with_parsed_ast_sequential(ctx.source, &filename, |source_unit| {
            let mut diagnostics = Vec::new();

            for item in source_unit.items.iter() {
                if let Some(subject) = contract_subject(ctx.source, item) {
                    diagnostics.extend(check_subject(ctx, &settings, &subject));
                }

                let ItemKind::Contract(contract) = &item.kind else {
                    continue;
                };

                for body_item in contract.body.iter() {
                    match &body_item.kind {
                        ItemKind::Function(func) if func.kind != FunctionKind::Modifier => {
                            let subject = function_subject(ctx.source, body_item, func);
                            diagnostics.extend(check_subject(ctx, &settings, &subject));
                        }
                        ItemKind::Event(_) => {
                            let subject = event_subject(ctx.source, body_item);
                            diagnostics.extend(check_subject(ctx, &settings, &subject));
                        }
                        ItemKind::Variable(var) => {
                            let subject = variable_subject(ctx.source, body_item, var.visibility);
                            diagnostics.extend(check_subject(ctx, &settings, &subject));
                        }
                        _ => {}
                    }
                }
            }

            diagnostics
        })
        .unwrap_or_default()
    }
}

fn contract_subject(source: &str, item: &Item<'_>) -> Option<Subject> {
    let ItemKind::Contract(contract) = &item.kind else {
        return None;
    };

    let context = match contract.kind {
        ContractKind::Contract | ContractKind::AbstractContract => "contract".to_string(),
        ContractKind::Library => "library".to_string(),
        ContractKind::Interface => "interface".to_string(),
    };

    let start = solgrid_ast::span_to_range(item.span).start;
    Some(Subject {
        kind: SubjectKind::Contract(contract.kind),
        name: contract.name.as_str().to_string(),
        span: solgrid_ast::item_name_range(item),
        context,
        block: find_attached_natspec(source, start),
        params: Vec::new(),
        returns: Vec::new(),
        simple_getter: false,
    })
}

fn function_subject(source: &str, item: &Item<'_>, func: &ItemFunction<'_>) -> Subject {
    let start = solgrid_ast::span_to_range(item.span).start;
    let name = func
        .header
        .name
        .map(|name| name.as_str().to_string())
        .unwrap_or_else(|| func.kind.to_str().to_string());
    let visibility = match func.header.visibility() {
        Some(Visibility::External) => "external",
        Some(Visibility::Public) => "public",
        Some(Visibility::Internal) => "internal",
        Some(Visibility::Private) => "private",
        None => "default",
    };
    let returns = return_names(func);
    let simple_getter = matches!(
        func.header.state_mutability(),
        StateMutability::View | StateMutability::Pure
    ) && func.header.parameters.is_empty()
        && returns.len() == 1;

    Subject {
        kind: SubjectKind::Function,
        name,
        span: solgrid_ast::item_name_range(item),
        context: format!("function:{visibility}"),
        block: find_attached_natspec(source, start),
        params: func
            .header
            .parameters
            .iter()
            .map(|param| param.name.map(|name| name.as_str().to_string()))
            .collect(),
        returns,
        simple_getter,
    }
}

fn event_subject(source: &str, item: &Item<'_>) -> Subject {
    let ItemKind::Event(event) = &item.kind else {
        unreachable!();
    };
    let start = solgrid_ast::span_to_range(item.span).start;
    Subject {
        kind: SubjectKind::Event,
        name: event.name.as_str().to_string(),
        span: solgrid_ast::item_name_range(item),
        context: "event".to_string(),
        block: find_attached_natspec(source, start),
        params: event
            .parameters
            .iter()
            .map(|param| param.name.map(|name| name.as_str().to_string()))
            .collect(),
        returns: Vec::new(),
        simple_getter: false,
    }
}

fn variable_subject(source: &str, item: &Item<'_>, visibility: Option<Visibility>) -> Subject {
    let ItemKind::Variable(variable) = &item.kind else {
        unreachable!();
    };
    let start = solgrid_ast::span_to_range(item.span).start;
    let context = match visibility {
        Some(Visibility::Public) => "variable:public",
        Some(Visibility::Private) => "variable:private",
        Some(Visibility::Internal) | None => "variable:internal",
        Some(Visibility::External) => "variable:internal",
    };

    Subject {
        kind: SubjectKind::Variable,
        name: variable
            .name
            .map(|name| name.as_str().to_string())
            .unwrap_or_else(|| "variable".to_string()),
        span: solgrid_ast::item_name_range(item),
        context: context.to_string(),
        block: find_attached_natspec(source, start),
        params: Vec::new(),
        returns: Vec::new(),
        simple_getter: false,
    }
}

fn check_subject(
    ctx: &LintContext<'_>,
    settings: &NatspecSettings,
    subject: &Subject,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let block = subject.block.clone();
    let (stripped_lines, parsed_tags) = block
        .as_ref()
        .map(|block| {
            let lines = block.stripped_lines();
            let tags = parse_tag_lines(&lines);
            (lines, tags)
        })
        .unwrap_or_else(|| (Vec::new(), Vec::new()));

    if let Some(block) = &block {
        if let Some(diag) = formatting_diagnostic(ctx, settings, subject, block, &stripped_lines) {
            diagnostics.push(diag);
        }
    }

    let has_inheritdoc = parsed_tags
        .iter()
        .any(|tag| tag.kind.eq_ignore_ascii_case("inheritdoc"));

    let required_tags = required_tags(subject, settings);
    if !has_inheritdoc {
        let documented_tags: Vec<String> = parsed_tags.iter().map(|tag| tag.kind.clone()).collect();
        let missing: Vec<_> = required_tags
            .iter()
            .copied()
            .filter(|tag| {
                !documented_tags
                    .iter()
                    .any(|documented| documented == tag.as_str())
            })
            .collect();

        if !missing.is_empty() {
            diagnostics.push(Diagnostic::new(
                META.id,
                format!(
                    "{} `{}` is missing NatSpec tags: {}",
                    subject_label(subject.kind),
                    subject.name,
                    missing
                        .iter()
                        .map(|tag| tag.display())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                META.default_severity,
                subject.span.clone(),
            ));
        }

        diagnostics.extend(validate_params(
            subject,
            &parsed_tags,
            required_tags.contains(&TagKind::Param),
        ));
        diagnostics.extend(validate_returns(
            subject,
            &parsed_tags,
            required_tags.contains(&TagKind::Return),
        ));
    }

    if subject.simple_getter {
        let forbidden: Vec<_> = parsed_tags
            .iter()
            .filter(|tag| tag.kind == "param" || tag.kind == "return")
            .collect();
        if !forbidden.is_empty() {
            diagnostics.push(Diagnostic::new(
                META.id,
                format!(
                    "simple getter `{}` must not document {}",
                    subject.name,
                    forbidden
                        .iter()
                        .map(|tag| format!("@{}", tag.kind))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                META.default_severity,
                subject.span.clone(),
            ));
        }
    }

    diagnostics
}

fn formatting_diagnostic(
    _ctx: &LintContext<'_>,
    settings: &NatspecSettings,
    subject: &Subject,
    block: &NatSpecBlock,
    stripped_lines: &[String],
) -> Option<Diagnostic> {
    let formatted_lines = format_contents(stripped_lines, settings.continuation_indent);
    let style_invalid = block.style == NatSpecStyle::Block
        && settings.comment_style == CommentStyleSetting::TripleSlash;
    let format_invalid = formatted_lines != stripped_lines;

    if !style_invalid && !format_invalid {
        return None;
    }

    let replacement = render_triple_slash_block(&block.indent, &formatted_lines);
    let message = if style_invalid {
        "NatSpec should use triple-slash comments"
    } else {
        "NatSpec should use canonical continuation indentation and spacing"
    };

    Some(
        Diagnostic::new(
            META.id,
            format!(
                "{message} on {} `{}`",
                subject_label(subject.kind),
                subject.name
            ),
            META.default_severity,
            subject.span.clone(),
        )
        .with_fix(Fix::safe(
            "Rewrite NatSpec block",
            vec![TextEdit::replace(block.range.clone(), replacement)],
        )),
    )
}

fn format_contents(
    lines: &[String],
    continuation_indent: ContinuationIndentSetting,
) -> Vec<String> {
    let mut result = Vec::new();
    let mut cursor = 0;

    while cursor < lines.len() {
        if !is_tag_line(&lines[cursor]) {
            result.push(lines[cursor].clone());
            cursor += 1;
            continue;
        }

        let start = cursor;
        cursor += 1;
        while cursor < lines.len() && !is_tag_line(&lines[cursor]) {
            cursor += 1;
        }

        let group = &lines[start..cursor];
        let formatted = format_tag_group(group, continuation_indent);
        result.extend(formatted);
    }

    result
}

fn format_tag_group(
    group: &[String],
    continuation_indent: ContinuationIndentSetting,
) -> Vec<String> {
    let mut lines = group.to_vec();
    while matches!(lines.last(), Some(last) if last.trim().is_empty()) {
        lines.pop();
    }

    if lines.len() <= 1 {
        return lines;
    }

    if continuation_indent == ContinuationIndentSetting::None {
        let mut formatted = vec![lines[0].clone()];
        for line in lines.iter().skip(1) {
            if line.trim().is_empty() {
                formatted.push(String::new());
            } else {
                formatted.push(line.trim_start().to_string());
            }
        }
        return formatted;
    }

    let expected = expected_continuation_indent(&lines[0]);
    let has_internal_blank = lines[1..].iter().any(|line| line.trim().is_empty());
    let is_long = lines.len() > 4 || has_internal_blank;

    let mut formatted = vec![lines[0].clone()];
    if is_long {
        let uniform_padding = lines[1..]
            .iter()
            .filter(|line| !line.trim().is_empty())
            .all(|line| leading_spaces(line) >= expected);

        for line in lines.iter().skip(1) {
            if line.trim().is_empty() {
                formatted.push(String::new());
            } else if uniform_padding && expected > 0 {
                formatted.push(
                    line.chars()
                        .skip(expected.min(line.len()))
                        .collect::<String>()
                        .trim_start_matches(char::is_whitespace)
                        .to_string(),
                );
            } else {
                formatted.push(line.clone());
            }
        }
        if !matches!(formatted.last(), Some(last) if last.trim().is_empty()) {
            formatted.push(String::new());
        }
        return formatted;
    }

    for line in lines.iter().skip(1) {
        if line.trim().is_empty() {
            formatted.push(String::new());
            continue;
        }

        if leading_spaces(line) < expected {
            formatted.push(format!("{}{}", " ".repeat(expected), line.trim_start()));
        } else {
            formatted.push(line.clone());
        }
    }

    formatted
}

fn expected_continuation_indent(tag_line: &str) -> usize {
    let trimmed = tag_line.trim_start();
    let tokens: Vec<_> = trimmed.split_whitespace().collect();
    if tokens.is_empty() {
        return 0;
    }

    match tokens[0] {
        "@param" | "@return" if tokens.len() >= 3 => {
            let prefix = format!("{} {} ", tokens[0], tokens[1]);
            prefix.len()
        }
        tag => format!("{tag} ").len(),
    }
}

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

fn parse_tag_lines(lines: &[String]) -> Vec<ParsedTagLine> {
    lines
        .iter()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let tag = trimmed.strip_prefix('@')?;
            let mut parts = tag.split_whitespace();
            let kind = parts.next()?.to_ascii_lowercase();
            let arg = parts.next().map(ToString::to_string);
            Some(ParsedTagLine { kind, arg })
        })
        .collect()
}

fn validate_params(
    subject: &Subject,
    parsed_tags: &[ParsedTagLine],
    enabled: bool,
) -> Vec<Diagnostic> {
    if !enabled || !matches!(subject.kind, SubjectKind::Function | SubjectKind::Event) {
        return Vec::new();
    }

    let documented: Vec<_> = parsed_tags
        .iter()
        .filter(|tag| tag.kind == "param")
        .cloned()
        .collect();
    if documented.is_empty() {
        return Vec::new();
    }

    let actual_names: Vec<_> = subject.params.clone();
    let any_unnamed = actual_names.iter().any(|name| name.is_none());

    let mismatch = if any_unnamed {
        documented.len() != actual_names.len()
    } else {
        let documented_names: Vec<_> = documented.iter().map(|tag| tag.arg.clone()).collect();
        documented.len() != actual_names.len() || documented_names != actual_names
    };

    if mismatch {
        vec![Diagnostic::new(
            META.id,
            format!(
                "{} `{}` has mismatched @param documentation",
                subject_label(subject.kind),
                subject.name
            ),
            META.default_severity,
            subject.span.clone(),
        )]
    } else {
        Vec::new()
    }
}

fn validate_returns(
    subject: &Subject,
    parsed_tags: &[ParsedTagLine],
    enabled: bool,
) -> Vec<Diagnostic> {
    if !enabled || !matches!(subject.kind, SubjectKind::Function) {
        return Vec::new();
    }

    let documented: Vec<_> = parsed_tags
        .iter()
        .filter(|tag| tag.kind == "return")
        .cloned()
        .collect();
    if documented.is_empty() {
        return Vec::new();
    }

    let any_unnamed = subject.returns.iter().any(|name| name.is_none());
    let mismatch = if any_unnamed {
        documented.len() != subject.returns.len()
    } else {
        let documented_names: Vec<_> = documented.iter().map(|tag| tag.arg.clone()).collect();
        documented.len() != subject.returns.len() || documented_names != subject.returns
    };

    if mismatch {
        vec![Diagnostic::new(
            META.id,
            format!(
                "function `{}` has mismatched @return documentation",
                subject.name
            ),
            META.default_severity,
            subject.span.clone(),
        )]
    } else {
        Vec::new()
    }
}

fn required_tags(subject: &Subject, settings: &NatspecSettings) -> Vec<TagKind> {
    let mut tags = match subject.kind {
        SubjectKind::Contract(_) => vec![
            TagKind::Title,
            TagKind::Author,
            TagKind::Notice,
            TagKind::Dev,
        ],
        SubjectKind::Function => vec![
            TagKind::Notice,
            TagKind::Dev,
            TagKind::Param,
            TagKind::Return,
        ],
        SubjectKind::Event => vec![TagKind::Notice, TagKind::Dev, TagKind::Param],
        SubjectKind::Variable => vec![TagKind::Notice, TagKind::Dev],
    };

    tags.retain(|tag| tag_enabled(settings, *tag, &subject.context));

    if subject.params.is_empty() {
        tags.retain(|tag| *tag != TagKind::Param);
    }
    if subject.returns.is_empty() {
        tags.retain(|tag| *tag != TagKind::Return);
    }
    if subject.simple_getter {
        tags.retain(|tag| *tag != TagKind::Param && *tag != TagKind::Return);
    }

    tags
}

fn tag_enabled(settings: &NatspecSettings, tag: TagKind, context: &str) -> bool {
    let config = match tag {
        TagKind::Title => &settings.tags.title,
        TagKind::Author => &settings.tags.author,
        TagKind::Notice => &settings.tags.notice,
        TagKind::Dev => &settings.tags.dev,
        TagKind::Param => &settings.tags.param,
        TagKind::Return => &settings.tags.return_tag,
    };

    let mut enabled = config.enabled.unwrap_or_else(|| tag.default_enabled());
    if !enabled {
        return false;
    }

    if let Some(include) = &config.include {
        return include
            .iter()
            .any(|pattern| context_matches(pattern, context));
    }

    if let Some(exclude) = &config.exclude {
        enabled &= !exclude
            .iter()
            .any(|pattern| context_matches(pattern, context));
    } else if config.skip_internal.unwrap_or(false) {
        enabled &= !matches!(context, "function:internal" | "function:private");
    }

    enabled
}

fn context_matches(pattern: &str, context: &str) -> bool {
    pattern == context
        || pattern
            .strip_suffix(':')
            .map(|prefix| context.starts_with(prefix))
            .unwrap_or(false)
        || context
            .strip_prefix(pattern)
            .map(|rest| rest.starts_with(':'))
            .unwrap_or(false)
}

fn return_names(func: &ItemFunction<'_>) -> Vec<Option<String>> {
    match &func.header.returns {
        Some(returns) => returns
            .iter()
            .map(|param| param.name.map(|name| name.as_str().to_string()))
            .collect(),
        None => Vec::new(),
    }
}

fn is_tag_line(line: &str) -> bool {
    line.trim_start().starts_with('@')
}

fn subject_label(kind: SubjectKind) -> &'static str {
    match kind {
        SubjectKind::Contract(ContractKind::Library) => "library",
        SubjectKind::Contract(ContractKind::Interface) => "interface",
        SubjectKind::Contract(_) => "contract",
        SubjectKind::Function => "function",
        SubjectKind::Event => "event",
        SubjectKind::Variable => "state variable",
    }
}
