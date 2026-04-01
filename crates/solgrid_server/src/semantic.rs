//! Semantic analysis helpers shared by completion and signature help.

use crate::builtins;
use crate::definition;
use crate::resolve::ImportResolver;
use crate::symbols::{self, SignatureData, SymbolDef, SymbolKind, SymbolTable, TypeSpec};
use solgrid_linter::source_utils::{is_in_non_code_region, scan_source_regions};
use solgrid_parser::solar_ast::{self, Expr, ExprKind, IndexKind, ItemKind, Stmt, StmtKind};
use solgrid_parser::with_parsed_ast_sequential;
use std::collections::HashSet;
use std::path::Path;

const COMPLETION_MEMBER_PLACEHOLDER: &str = "__solgrid_member";
const SIGNATURE_ARG_PLACEHOLDER: &str = "0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedSignature {
    pub label: String,
    pub parameter_ranges: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallSignatureHelp {
    pub signatures: Vec<ResolvedSignature>,
    pub active_parameter: u32,
}

#[derive(Debug, Clone, Copy)]
struct MemberContext {
    dot_offset: usize,
    member_start: usize,
    stmt_start: usize,
    stmt_end: usize,
}

#[derive(Debug, Clone, Copy)]
struct CallContext {
    open_paren_offset: usize,
    active_parameter: u32,
    stmt_start: usize,
    stmt_end: usize,
}

#[derive(Debug, Clone)]
struct ResolvedDef {
    table: SymbolTable,
    def: SymbolDef,
}

#[derive(Clone, Copy)]
struct SemanticContext<'a> {
    table: &'a SymbolTable,
    source: &'a str,
    current_file: Option<&'a Path>,
    get_source: &'a dyn Fn(&Path) -> Option<String>,
    resolver: &'a ImportResolver,
}

impl<'a> SemanticContext<'a> {
    fn resolve_namespace_scope_symbols(&self, namespace: &str) -> Vec<SymbolDef> {
        let Some(current_file) = self.current_file else {
            return Vec::new();
        };

        let mut symbols = Vec::new();
        let mut seen = HashSet::new();
        for import in &self.table.imports {
            let matches_namespace = match &import.symbols {
                symbols::ImportedSymbols::Plain(Some(alias))
                | symbols::ImportedSymbols::Glob(alias) => alias == namespace,
                _ => false,
            };
            if !matches_namespace {
                continue;
            }

            let Some(path) = self.resolver.resolve(&import.path, current_file) else {
                continue;
            };
            let Some(imported_source) = (self.get_source)(&path) else {
                continue;
            };
            let filename = path.to_string_lossy().to_string();
            let Some(imported_table) = symbols::build_symbol_table(&imported_source, &filename)
            else {
                continue;
            };

            for symbol in imported_table.file_level_symbols() {
                if symbol.kind == SymbolKind::Constructor || symbol.name.is_empty() {
                    continue;
                }
                if seen.insert(symbol.name.clone()) {
                    symbols.push(symbol.clone());
                }
            }
        }

        symbols
    }

    fn resolve_container_targets_from_expr(&self, expr: &Expr<'_>) -> Vec<ResolvedDef> {
        let expr_offset = solgrid_ast::span_to_range(expr.span).start;
        let mut results = Vec::new();

        match &expr.kind {
            ExprKind::Ident(ident) => {
                for def in self.table.resolve_all(ident.as_str(), expr_offset) {
                    if is_container_kind(def.kind) {
                        results.push(ResolvedDef {
                            table: self.table.clone(),
                            def: def.clone(),
                        });
                    }
                    if let Some(type_info) = &def.type_info {
                        results.extend(self.resolve_type_spec(type_info));
                    }
                }
            }
            ExprKind::Member(base, member) => {
                for resolved in self.resolve_member_defs(base, member.as_str()) {
                    if is_container_kind(resolved.def.kind) {
                        results.push(resolved.clone());
                    }
                    if let Some(type_info) = &resolved.def.type_info {
                        results.extend(self.resolve_type_spec(type_info));
                    }
                }
            }
            ExprKind::Call(callee, _) => {
                let mut return_types = HashSet::new();
                for signature in self.resolve_user_call_signatures(callee) {
                    if let Some(first_return) = signature.first_return_type {
                        return_types.insert(first_return);
                    }
                }
                if return_types.len() == 1 {
                    if let Some(return_type) = return_types.into_iter().next() {
                        results.extend(self.resolve_type_spec(&return_type));
                    }
                }
            }
            ExprKind::Index(base, IndexKind::Index(_) | IndexKind::Range(_, _)) => {
                for value_type in self.infer_value_types(base) {
                    if let Some(indexed) = value_type.index_result() {
                        results.extend(self.resolve_type_spec(indexed));
                    }
                }
            }
            ExprKind::New(ty) => {
                let ty_spec = symbols::type_spec_from_ast(self.source, ty, None, expr_offset);
                results.extend(self.resolve_type_spec(&ty_spec));
            }
            _ => {}
        }

        dedup_resolved_defs(results)
    }

    fn infer_value_types(&self, expr: &Expr<'_>) -> Vec<TypeSpec> {
        let expr_offset = solgrid_ast::span_to_range(expr.span).start;
        let mut results = Vec::new();

        match &expr.kind {
            ExprKind::Ident(ident) => {
                for def in self.table.resolve_all(ident.as_str(), expr_offset) {
                    if let Some(type_info) = &def.type_info {
                        results.push(type_info.clone());
                    }
                }
            }
            ExprKind::Member(base, member) => {
                for resolved in self.resolve_member_defs(base, member.as_str()) {
                    if let Some(type_info) = &resolved.def.type_info {
                        results.push(type_info.clone());
                    }
                }
            }
            ExprKind::Call(callee, _) => {
                for signature in self.resolve_user_call_signatures(callee) {
                    if let Some(first_return) = signature.first_return_type {
                        results.push(first_return);
                    }
                }
            }
            ExprKind::Index(base, IndexKind::Index(_) | IndexKind::Range(_, _)) => {
                for value_type in self.infer_value_types(base) {
                    if let Some(indexed) = value_type.index_result() {
                        results.push(indexed.clone());
                    }
                }
            }
            ExprKind::New(ty) => {
                results.push(symbols::type_spec_from_ast(
                    self.source,
                    ty,
                    None,
                    expr_offset,
                ));
            }
            _ => {}
        }

        dedup_types(results)
    }

    fn resolve_member_defs(&self, base: &Expr<'_>, member_name: &str) -> Vec<ResolvedDef> {
        let mut resolved = Vec::new();

        if let ExprKind::Ident(namespace) = &base.kind {
            resolved.extend(self.resolve_namespace_member_defs(namespace.as_str(), member_name));
        }

        for container in self.resolve_container_targets_from_expr(base) {
            for member in container
                .table
                .resolve_member_all(&container.def, member_name)
            {
                resolved.push(ResolvedDef {
                    table: container.table.clone(),
                    def: member.clone(),
                });
            }
        }

        dedup_resolved_defs(resolved)
    }

    fn resolve_namespace_member_defs(
        &self,
        namespace: &str,
        member_name: &str,
    ) -> Vec<ResolvedDef> {
        let Some(current_file) = self.current_file else {
            return Vec::new();
        };

        let mut resolved = Vec::new();
        for import in &self.table.imports {
            let matches_namespace = match &import.symbols {
                symbols::ImportedSymbols::Plain(Some(alias))
                | symbols::ImportedSymbols::Glob(alias) => alias == namespace,
                _ => false,
            };
            if !matches_namespace {
                continue;
            }

            let Some(path) = self.resolver.resolve(&import.path, current_file) else {
                continue;
            };
            let Some(imported_source) = (self.get_source)(&path) else {
                continue;
            };
            let filename = path.to_string_lossy().to_string();
            let Some(imported_table) = symbols::build_symbol_table(&imported_source, &filename)
            else {
                continue;
            };

            for def in imported_table.resolve_all(member_name, 0) {
                resolved.push(ResolvedDef {
                    table: imported_table.clone(),
                    def: def.clone(),
                });
            }
        }

        dedup_resolved_defs(resolved)
    }

    fn resolve_type_spec(&self, ty: &TypeSpec) -> Vec<ResolvedDef> {
        let Some(path) = ty.member_target() else {
            return Vec::new();
        };

        self.resolve_type_path(path, ty.resolve_offset())
    }

    fn resolve_type_path(
        &self,
        path: &symbols::TypePath,
        resolve_offset: usize,
    ) -> Vec<ResolvedDef> {
        if path.segments.is_empty() {
            return Vec::new();
        }

        if path.segments.len() >= 2 {
            let namespace_matches =
                self.resolve_namespace_member_defs(&path.segments[0], &path.segments[1]);
            if !namespace_matches.is_empty() {
                let mut current = namespace_matches
                    .into_iter()
                    .filter(|resolved| is_container_kind(resolved.def.kind))
                    .collect::<Vec<_>>();
                for segment in &path.segments[2..] {
                    current = current
                        .into_iter()
                        .flat_map(|container| {
                            container
                                .table
                                .resolve_member_all(&container.def, segment)
                                .iter()
                                .filter(|member| is_container_kind(member.kind))
                                .map(|member| ResolvedDef {
                                    table: container.table.clone(),
                                    def: (*member).clone(),
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect();
                }
                return dedup_resolved_defs(current);
            }
        }

        let mut current = Vec::new();
        for def in self.table.resolve_all(&path.segments[0], resolve_offset) {
            if is_container_kind(def.kind) {
                current.push(ResolvedDef {
                    table: self.table.clone(),
                    def: def.clone(),
                });
            }
        }

        if let Some(current_file) = self.current_file {
            if let Some(cross) = definition::resolve_cross_file_symbol(
                self.table,
                &path.segments[0],
                current_file,
                self.get_source,
                self.resolver,
            ) {
                for def in cross.table.resolve_all(&cross.def.name, 0) {
                    if is_container_kind(def.kind) {
                        current.push(ResolvedDef {
                            table: cross.table.clone(),
                            def: def.clone(),
                        });
                    }
                }
            }
        }

        for segment in &path.segments[1..] {
            current = current
                .into_iter()
                .flat_map(|container| {
                    container
                        .table
                        .resolve_member_all(&container.def, segment)
                        .iter()
                        .filter(|member| is_container_kind(member.kind))
                        .map(|member| ResolvedDef {
                            table: container.table.clone(),
                            def: (*member).clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
        }

        dedup_resolved_defs(current)
    }

    fn resolve_user_call_signatures(&self, callee: &Expr<'_>) -> Vec<SignatureData> {
        let expr_offset = solgrid_ast::span_to_range(callee.span).start;
        let mut signatures = Vec::new();

        match &callee.kind {
            ExprKind::Ident(ident) => {
                for def in self.table.resolve_all(ident.as_str(), expr_offset) {
                    if let Some(signature) = &def.signature {
                        signatures.push(signature.clone());
                    }
                }

                if signatures.is_empty() {
                    if let Some(current_file) = self.current_file {
                        if let Some(cross) = definition::resolve_cross_file_symbol(
                            self.table,
                            ident.as_str(),
                            current_file,
                            self.get_source,
                            self.resolver,
                        ) {
                            for def in cross.table.resolve_all(&cross.def.name, 0) {
                                if let Some(signature) = &def.signature {
                                    signatures.push(signature.clone());
                                }
                            }
                        }
                    }
                }
            }
            ExprKind::Member(base, member) => {
                for resolved in self.resolve_member_defs(base, member.as_str()) {
                    if let Some(signature) = &resolved.def.signature {
                        signatures.push(signature.clone());
                    }
                }
            }
            ExprKind::New(ty) => {
                let type_spec = symbols::type_spec_from_ast(self.source, ty, None, expr_offset);
                for container in self.resolve_type_spec(&type_spec) {
                    for constructor in container.table.constructors(&container.def) {
                        if let Some(signature) = &constructor.signature {
                            signatures.push(signature.clone());
                        }
                    }
                }
            }
            _ => {}
        }

        dedup_signature_data(signatures)
    }

    fn resolve_builtin_signatures(&self, callee: &Expr<'_>) -> Vec<ResolvedSignature> {
        match &callee.kind {
            ExprKind::Ident(ident) => builtins::lookup_solidity_global(ident.as_str())
                .into_iter()
                .filter_map(|def| parse_builtin_signature(def.signature))
                .collect(),
            ExprKind::Member(base, member) => {
                if let ExprKind::Ident(namespace) = &base.kind {
                    return builtins::lookup_solidity_member(namespace.as_str(), member.as_str())
                        .into_iter()
                        .filter_map(|def| parse_builtin_signature(def.signature))
                        .collect();
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

pub(crate) fn member_completion_symbols(
    source: &str,
    offset: usize,
    current_file: Option<&Path>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<SymbolDef> {
    let Some(context) = find_member_context(source, offset) else {
        return Vec::new();
    };

    let patched = patch_member_source(source, context);
    let Some(table) = symbols::build_symbol_table(&patched, "buffer.sol") else {
        return Vec::new();
    };
    let semantic = SemanticContext {
        table: &table,
        source: &patched,
        current_file,
        get_source,
        resolver,
    };

    with_parsed_ast_sequential(&patched, "buffer.sol", |source_unit| {
        let mut best_len = usize::MAX;
        let mut best_items = Vec::new();
        visit_source_unit_exprs(source_unit, &mut |expr| {
            if let ExprKind::Member(base, _) = &expr.kind {
                let base_range = solgrid_ast::span_to_range(base.span);
                let expr_range = solgrid_ast::span_to_range(expr.span);
                if base_range.end <= context.dot_offset && context.dot_offset < expr_range.end {
                    let len = expr_range.len();
                    if len < best_len {
                        best_len = len;
                        let mut seen = HashSet::new();
                        let mut items = Vec::new();
                        if let ExprKind::Ident(namespace) = &base.kind {
                            for symbol in
                                semantic.resolve_namespace_scope_symbols(namespace.as_str())
                            {
                                if seen.insert(symbol.name.clone()) {
                                    items.push(symbol);
                                }
                            }
                        }
                        for container in semantic.resolve_container_targets_from_expr(base) {
                            let Some(scope_id) = container.def.scope else {
                                continue;
                            };
                            for symbol in container.table.scope_symbols(scope_id) {
                                if symbol.kind == SymbolKind::Constructor || symbol.name.is_empty()
                                {
                                    continue;
                                }
                                if seen.insert(symbol.name.clone()) {
                                    items.push(symbol.clone());
                                }
                            }
                        }
                        best_items = items;
                    }
                }
            }
        });
        best_items
    })
    .unwrap_or_default()
}

pub(crate) fn signature_help_at_offset(
    source: &str,
    offset: usize,
    current_file: Option<&Path>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<CallSignatureHelp> {
    let context = find_active_call_context(source, offset)?;
    let patched = patch_call_source(source, offset, context);
    let table = symbols::build_symbol_table(&patched, "buffer.sol")?;
    let semantic = SemanticContext {
        table: &table,
        source: &patched,
        current_file,
        get_source,
        resolver,
    };

    with_parsed_ast_sequential(&patched, "buffer.sol", |source_unit| {
        let mut best_len = usize::MAX;
        let mut best_help = None;
        visit_source_unit_exprs(source_unit, &mut |expr| {
            if let ExprKind::Call(_, args) = &expr.kind {
                let args_range = solgrid_ast::span_to_range(args.span);
                if args_range.start == context.open_paren_offset {
                    let len = solgrid_ast::span_to_range(expr.span).len();
                    if len < best_len {
                        best_len = len;
                        let ExprKind::Call(callee, _) = &expr.kind else {
                            return;
                        };
                        let mut signatures: Vec<ResolvedSignature> = semantic
                            .resolve_user_call_signatures(callee)
                            .into_iter()
                            .map(signature_data_to_resolved)
                            .collect();
                        signatures.extend(semantic.resolve_builtin_signatures(callee));

                        if !signatures.is_empty() {
                            let max_parameter_index = signatures
                                .iter()
                                .filter_map(|signature| {
                                    signature.parameter_ranges.len().checked_sub(1)
                                })
                                .max()
                                .unwrap_or(0)
                                as u32;
                            best_help = Some(CallSignatureHelp {
                                signatures,
                                active_parameter: context.active_parameter.min(max_parameter_index),
                            });
                        }
                    }
                }
            }
        });
        best_help
    })
    .ok()
    .flatten()
}

fn signature_data_to_resolved(signature: SignatureData) -> ResolvedSignature {
    ResolvedSignature {
        label: signature.label,
        parameter_ranges: signature
            .parameters
            .into_iter()
            .map(|param| (param.start, param.end))
            .collect(),
    }
}

fn parse_builtin_signature(signature: &str) -> Option<ResolvedSignature> {
    let open = signature.find('(')?;
    let close = find_matching_paren(signature, open)?;
    let params_text = &signature[open + 1..close];
    let parameter_parts = split_top_level(params_text, ',');

    let mut search_start = open + 1;
    let mut parameter_ranges = Vec::new();
    for part in parameter_parts {
        let trimmed = normalize_builtin_parameter_part(&part);
        if trimmed.is_empty() {
            continue;
        }
        let found = signature[search_start..]
            .find(trimmed)
            .map(|relative| search_start + relative)
            .or_else(|| signature.find(trimmed));
        let Some(start) = found else {
            continue;
        };
        let end = start + trimmed.len();
        parameter_ranges.push((start as u32, end as u32));
        search_start = end;
    }

    Some(ResolvedSignature {
        label: signature.to_string(),
        parameter_ranges,
    })
}

fn split_top_level(text: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;

    for (index, ch) in text.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }

        if ch == delimiter && paren_depth == 0 && brace_depth == 0 {
            parts.push(text[start..index].to_string());
            start = index + ch.len_utf8();
        }
    }

    parts.push(text[start..].to_string());
    parts
}

fn normalize_builtin_parameter_part(part: &str) -> &str {
    part.trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '[' | ']'))
        .trim()
}

fn find_matching_paren(text: &str, open_index: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate().skip(open_index) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_member_context(source: &str, offset: usize) -> Option<MemberContext> {
    let regions = scan_source_regions(source);
    let bytes = source.as_bytes();
    if offset == 0 {
        return None;
    }

    let mut pos = offset.min(bytes.len());
    while pos > 0 && is_ident_char(bytes[pos - 1]) {
        pos -= 1;
    }
    let member_start = pos;

    if member_start == 0 || bytes[member_start - 1] != b'.' {
        return None;
    }
    let dot_offset = member_start - 1;
    if is_in_non_code_region(&regions, dot_offset) {
        return None;
    }

    let (stmt_start, stmt_end) = statement_bounds(source, dot_offset);
    Some(MemberContext {
        dot_offset,
        member_start,
        stmt_start,
        stmt_end,
    })
}

fn patch_member_source(source: &str, context: MemberContext) -> String {
    let stmt_prefix = format!(
        "{}{}",
        &source[context.stmt_start..context.member_start],
        COMPLETION_MEMBER_PLACEHOLDER
    );
    let replacement = format!(
        "{}{}",
        COMPLETION_MEMBER_PLACEHOLDER,
        synthesized_statement_suffix(
            &stmt_prefix,
            source.as_bytes().get(context.stmt_end).copied(),
        )
    );

    replace_range(source, context.member_start..context.stmt_end, &replacement)
}

fn find_active_call_context(source: &str, offset: usize) -> Option<CallContext> {
    let regions = scan_source_regions(source);
    let bytes = source.as_bytes();
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut active_parameter = 0u32;
    let mut index = offset.min(bytes.len());

    while index > 0 {
        index -= 1;
        if is_in_non_code_region(&regions, index) {
            continue;
        }

        match bytes[index] {
            b')' => paren_depth += 1,
            b'(' => {
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
                    if !looks_like_callable_paren(source, index) {
                        return None;
                    }
                    let (stmt_start, stmt_end) = statement_bounds(source, index);
                    return Some(CallContext {
                        open_paren_offset: index,
                        active_parameter,
                        stmt_start,
                        stmt_end,
                    });
                }
                paren_depth = paren_depth.saturating_sub(1);
            }
            b']' => bracket_depth += 1,
            b'[' => bracket_depth = bracket_depth.saturating_sub(1),
            b'}' => brace_depth += 1,
            b'{' => brace_depth = brace_depth.saturating_sub(1),
            b',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                active_parameter += 1;
            }
            _ => {}
        }
    }

    None
}

fn patch_call_source(source: &str, offset: usize, context: CallContext) -> String {
    let prefix = &source[context.stmt_start..offset];
    let needs_placeholder =
        last_significant_byte(prefix).is_none_or(|byte| matches!(byte, b'(' | b','));
    let stmt_prefix = if needs_placeholder {
        format!("{prefix}{SIGNATURE_ARG_PLACEHOLDER}")
    } else {
        prefix.to_string()
    };
    let replacement = synthesized_statement_suffix(
        &stmt_prefix,
        source.as_bytes().get(context.stmt_end).copied(),
    );
    let replacement = if needs_placeholder {
        format!("{SIGNATURE_ARG_PLACEHOLDER}{replacement}")
    } else {
        replacement
    };

    replace_range(source, offset..context.stmt_end, &replacement)
}

fn synthesized_statement_suffix(stmt_prefix: &str, next_byte: Option<u8>) -> String {
    let regions = scan_source_regions(stmt_prefix);
    let mut stack = Vec::new();

    for (index, byte) in stmt_prefix.as_bytes().iter().enumerate() {
        if is_in_non_code_region(&regions, index) {
            continue;
        }
        match byte {
            b'(' => stack.push(')'),
            b'[' => stack.push(']'),
            b'{' => stack.push('}'),
            b')' | b']' | b'}' => {
                stack.pop();
            }
            _ => {}
        }
    }

    let mut suffix: String = stack.into_iter().rev().collect();
    if !matches!(next_byte, Some(b';') | Some(b'}')) {
        suffix.push(';');
    }
    suffix
}

fn statement_bounds(source: &str, offset: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let mut start = offset.min(bytes.len());
    while start > 0 {
        match bytes[start - 1] {
            b';' | b'{' | b'}' | b'\n' => break,
            _ => start -= 1,
        }
    }

    let mut end = offset.min(bytes.len());
    while end < bytes.len() {
        match bytes[end] {
            b';' | b'\n' | b'}' => break,
            _ => end += 1,
        }
    }

    (start, end)
}

fn looks_like_callable_paren(source: &str, open_paren: usize) -> bool {
    let bytes = source.as_bytes();
    let mut index = open_paren;
    while index > 0 && bytes[index - 1].is_ascii_whitespace() {
        index -= 1;
    }
    if index == 0 {
        return false;
    }

    let last_byte = bytes[index - 1];
    if matches!(last_byte, b')' | b']' | b'}') {
        return true;
    }
    if !is_ident_char(last_byte) {
        return false;
    }

    let mut start = index - 1;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let word = &source[start..index];
    !matches!(
        word,
        "if" | "while"
            | "for"
            | "function"
            | "modifier"
            | "returns"
            | "event"
            | "error"
            | "contract"
            | "constructor"
            | "catch"
    )
}

fn last_significant_byte(text: &str) -> Option<u8> {
    text.as_bytes()
        .iter()
        .rev()
        .find(|byte| !byte.is_ascii_whitespace())
        .copied()
}

fn replace_range(source: &str, range: std::ops::Range<usize>, replacement: &str) -> String {
    let mut patched = String::with_capacity(source.len() + replacement.len());
    patched.push_str(&source[..range.start]);
    patched.push_str(replacement);
    patched.push_str(&source[range.end..]);
    patched
}

fn is_ident_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn is_container_kind(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Contract
            | SymbolKind::Interface
            | SymbolKind::Library
            | SymbolKind::Struct
            | SymbolKind::Enum
    )
}

fn dedup_types(types: Vec<TypeSpec>) -> Vec<TypeSpec> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for ty in types {
        if seen.insert(ty.clone()) {
            unique.push(ty);
        }
    }
    unique
}

fn dedup_signature_data(signatures: Vec<SignatureData>) -> Vec<SignatureData> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for signature in signatures {
        if seen.insert(signature.label.clone()) {
            unique.push(signature);
        }
    }
    unique
}

fn dedup_resolved_defs(defs: Vec<ResolvedDef>) -> Vec<ResolvedDef> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for resolved in defs {
        let key = (
            resolved.def.name.clone(),
            resolved.def.kind,
            resolved.def.name_span.start,
            resolved.def.name_span.end,
        );
        if seen.insert(key) {
            unique.push(resolved);
        }
    }
    unique
}

fn visit_source_unit_exprs(
    source_unit: &solar_ast::SourceUnit<'_>,
    visitor: &mut impl FnMut(&Expr<'_>),
) {
    for item in source_unit.items.iter() {
        visit_item_exprs(item, visitor);
    }
}

fn visit_item_exprs(item: &solar_ast::Item<'_>, visitor: &mut impl FnMut(&Expr<'_>)) {
    match &item.kind {
        ItemKind::Contract(contract) => {
            for body_item in contract.body.iter() {
                visit_item_exprs(body_item, visitor);
            }
        }
        ItemKind::Function(function) => {
            for modifier in function.header.modifiers.iter() {
                for arg in modifier.arguments.exprs() {
                    visit_expr_tree(arg, visitor);
                }
            }
            if let Some(body) = &function.body {
                for stmt in body.stmts.iter() {
                    visit_stmt_exprs(stmt, visitor);
                }
            }
        }
        ItemKind::Variable(var) => {
            if let Some(initializer) = &var.initializer {
                visit_expr_tree(initializer, visitor);
            }
        }
        _ => {}
    }
}

fn visit_stmt_exprs(stmt: &Stmt<'_>, visitor: &mut impl FnMut(&Expr<'_>)) {
    match &stmt.kind {
        StmtKind::Assembly(_) | StmtKind::Break | StmtKind::Continue | StmtKind::Placeholder => {}
        StmtKind::DeclSingle(var) => {
            if let Some(initializer) = &var.initializer {
                visit_expr_tree(initializer, visitor);
            }
        }
        StmtKind::DeclMulti(vars, expr) => {
            for var in vars.iter() {
                if let solgrid_parser::solar_interface::SpannedOption::Some(var) = var {
                    if let Some(initializer) = &var.initializer {
                        visit_expr_tree(initializer, visitor);
                    }
                }
            }
            visit_expr_tree(expr, visitor);
        }
        StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
            for stmt in block.stmts.iter() {
                visit_stmt_exprs(stmt, visitor);
            }
        }
        StmtKind::DoWhile(body, expr) | StmtKind::While(expr, body) => {
            visit_expr_tree(expr, visitor);
            visit_stmt_exprs(body, visitor);
        }
        StmtKind::Emit(_, args) | StmtKind::Revert(_, args) => {
            for arg in args.exprs() {
                visit_expr_tree(arg, visitor);
            }
        }
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => visit_expr_tree(expr, visitor),
        StmtKind::Return(None) => {}
        StmtKind::For {
            init,
            cond,
            next,
            body,
        } => {
            if let Some(init) = init {
                visit_stmt_exprs(init, visitor);
            }
            if let Some(cond) = cond {
                visit_expr_tree(cond, visitor);
            }
            if let Some(next) = next {
                visit_expr_tree(next, visitor);
            }
            visit_stmt_exprs(body, visitor);
        }
        StmtKind::If(cond, then_stmt, else_stmt) => {
            visit_expr_tree(cond, visitor);
            visit_stmt_exprs(then_stmt, visitor);
            if let Some(else_stmt) = else_stmt {
                visit_stmt_exprs(else_stmt, visitor);
            }
        }
        StmtKind::Try(try_stmt) => {
            visit_expr_tree(try_stmt.expr, visitor);
            for clause in try_stmt.clauses.iter() {
                for stmt in clause.block.stmts.iter() {
                    visit_stmt_exprs(stmt, visitor);
                }
            }
        }
    }
}

fn visit_expr_tree(expr: &Expr<'_>, visitor: &mut impl FnMut(&Expr<'_>)) {
    visitor(expr);

    match &expr.kind {
        ExprKind::Array(exprs) => {
            for expr in exprs.iter() {
                visit_expr_tree(expr, visitor);
            }
        }
        ExprKind::Assign(lhs, _, rhs) | ExprKind::Binary(lhs, _, rhs) => {
            visit_expr_tree(lhs, visitor);
            visit_expr_tree(rhs, visitor);
        }
        ExprKind::Call(lhs, args) => {
            visit_expr_tree(lhs, visitor);
            for arg in args.exprs() {
                visit_expr_tree(arg, visitor);
            }
        }
        ExprKind::CallOptions(lhs, args) => {
            visit_expr_tree(lhs, visitor);
            for arg in args.iter() {
                visit_expr_tree(arg.value, visitor);
            }
        }
        ExprKind::Delete(expr) | ExprKind::Unary(_, expr) => {
            visit_expr_tree(expr, visitor);
        }
        ExprKind::Index(lhs, kind) => {
            visit_expr_tree(lhs, visitor);
            match kind {
                IndexKind::Index(Some(expr)) => visit_expr_tree(expr, visitor),
                IndexKind::Range(start, end) => {
                    if let Some(start) = start {
                        visit_expr_tree(start, visitor);
                    }
                    if let Some(end) = end {
                        visit_expr_tree(end, visitor);
                    }
                }
                IndexKind::Index(None) => {}
            }
        }
        ExprKind::Member(expr, _) => visit_expr_tree(expr, visitor),
        ExprKind::Payable(args) => {
            for arg in args.exprs() {
                visit_expr_tree(arg, visitor);
            }
        }
        ExprKind::Ternary(cond, if_true, if_false) => {
            visit_expr_tree(cond, visitor);
            visit_expr_tree(if_true, visitor);
            visit_expr_tree(if_false, visitor);
        }
        ExprKind::Tuple(exprs) => {
            for expr in exprs.iter() {
                if let solgrid_parser::solar_interface::SpannedOption::Some(expr) = expr {
                    visit_expr_tree(expr, visitor);
                }
            }
        }
        ExprKind::Ident(_)
        | ExprKind::Lit(_, _)
        | ExprKind::New(_)
        | ExprKind::Type(_)
        | ExprKind::TypeCall(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_builtin_signature_with_nested_args() {
        let parsed = parse_builtin_signature(
            "abi.encodeCall(functionPointer, (arg1, arg2, ...)) returns (bytes memory)",
        )
        .unwrap();
        assert_eq!(parsed.parameter_ranges.len(), 2);
        assert!(parsed.label.contains("abi.encodeCall"));
    }

    #[test]
    fn test_parse_builtin_signature_with_optional_parameter() {
        let parsed =
            parse_builtin_signature("require(bool condition [, string memory message])").unwrap();
        assert_eq!(parsed.parameter_ranges.len(), 2);
        assert!(parsed.label.contains("require("));
    }

    #[test]
    fn test_find_member_context_partial_member() {
        let source = "factory.current().tok";
        let context = find_member_context(source, source.len()).unwrap();
        assert_eq!(context.dot_offset, "factory.current()".len());
    }

    #[test]
    fn test_find_active_call_context_nested_call() {
        let source = "foo(bar(, baz))";
        let context = find_active_call_context(source, "foo(bar(".len()).unwrap();
        assert_eq!(context.active_parameter, 0);
        assert_eq!(context.open_paren_offset, "foo(bar".len());
    }

    #[test]
    fn test_patch_call_source_adds_placeholder_and_closers() {
        let source = "foo(bar(";
        let patched = patch_call_source(
            source,
            source.len(),
            find_active_call_context(source, source.len()).unwrap(),
        );
        assert_eq!(patched, "foo(bar(0));");
    }
}
