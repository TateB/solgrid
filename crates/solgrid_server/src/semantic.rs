//! Semantic analysis helpers shared by completion and signature help.

use crate::builtins;
use crate::convert;
use crate::definition;
use crate::resolve::ImportResolver;
use crate::symbols::{self, SignatureData, SymbolDef, SymbolKind, SymbolTable, TypeSpec};
use solgrid_ast::selectors::SelectorContext;
use solgrid_linter::source_utils::{is_in_non_code_region, scan_source_regions};
use solgrid_parser::solar_ast::{
    self, ContractKind, Expr, ExprKind, FunctionKind, IndexKind, ItemKind, Stmt, StmtKind,
    Visibility,
};
use solgrid_parser::with_parsed_ast_sequential;
use std::collections::HashSet;
use std::path::Path;
use tower_lsp_server::ls_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensLegend,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParameterInlayHint {
    pub offset: usize,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectorInlayHint {
    pub offset: usize,
    pub label: String,
    pub tooltip: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SemanticTokenKind {
    Namespace,
    Class,
    Interface,
    Enum,
    Struct,
    Type,
    Event,
    Function,
    Modifier,
    Parameter,
    Variable,
    Property,
    EnumMember,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawSemanticToken {
    span: std::ops::Range<usize>,
    kind: SemanticTokenKind,
    modifiers: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SemanticTokenInfo {
    kind: SemanticTokenKind,
    readonly: bool,
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
    source: Option<String>,
    origin_key: Option<String>,
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
    fn is_namespace_alias(&self, name: &str) -> bool {
        self.table
            .imports
            .iter()
            .any(|import| match &import.symbols {
                symbols::ImportedSymbols::Plain(Some(alias))
                | symbols::ImportedSymbols::Glob(alias) => alias == name,
                symbols::ImportedSymbols::Plain(None) | symbols::ImportedSymbols::Named(_) => false,
            })
    }

    fn resolve_ident_info(&self, ident: &solar_ast::Ident) -> Option<SemanticTokenInfo> {
        let offset = solgrid_ast::span_to_range(ident.span).start;
        if let Some(def) = self.table.resolve(ident.as_str(), offset) {
            return semantic_token_info_for_symbol(def, Some(self.source));
        }

        if let Some(current_file) = self.current_file {
            if let Some(cross) = definition::resolve_cross_file_symbol(
                self.table,
                ident.as_str(),
                current_file,
                self.get_source,
                self.resolver,
            ) {
                return semantic_token_info_for_symbol(&cross.def, Some(&cross.source));
            }
        }

        self.is_namespace_alias(ident.as_str())
            .then_some(SemanticTokenInfo {
                kind: SemanticTokenKind::Namespace,
                readonly: false,
            })
    }

    fn resolve_member_info(
        &self,
        base: &Expr<'_>,
        member: &solar_ast::Ident,
    ) -> Option<SemanticTokenInfo> {
        if let ExprKind::Ident(namespace) = &base.kind {
            if let Some(current_file) = self.current_file {
                if let Some(cross) = definition::resolve_cross_file_member_symbol(
                    self.table,
                    namespace.as_str(),
                    member.as_str(),
                    current_file,
                    self.get_source,
                    self.resolver,
                ) {
                    return semantic_token_info_for_symbol(&cross.def, Some(&cross.source));
                }
            }
        }

        let defs = self.resolve_member_defs(base, member.as_str());
        (defs.len() == 1)
            .then(|| {
                semantic_token_info_for_symbol(
                    &defs[0].def,
                    defs[0].source.as_deref().or(Some(self.source)),
                )
            })
            .flatten()
    }

    fn resolve_path_info(
        &self,
        path: &solar_ast::AstPath<'_>,
        resolve_offset: usize,
    ) -> Option<SemanticTokenInfo> {
        if let Some(ident) = path.get_ident() {
            if let Some(def) = self.table.resolve(ident.as_str(), resolve_offset) {
                return semantic_token_info_for_symbol(def, Some(self.source));
            }
            if let Some(current_file) = self.current_file {
                if let Some(cross) = definition::resolve_cross_file_symbol(
                    self.table,
                    ident.as_str(),
                    current_file,
                    self.get_source,
                    self.resolver,
                ) {
                    return semantic_token_info_for_symbol(&cross.def, Some(&cross.source));
                }
            }
            return None;
        }

        let first = path.first();
        let last = path.last();
        if self.is_namespace_alias(first.as_str()) {
            if let Some(current_file) = self.current_file {
                if let Some(cross) = definition::resolve_cross_file_member_symbol(
                    self.table,
                    first.as_str(),
                    last.as_str(),
                    current_file,
                    self.get_source,
                    self.resolver,
                ) {
                    return semantic_token_info_for_symbol(&cross.def, Some(&cross.source));
                }
            }
        }

        let type_path = symbols::TypePath {
            segments: path
                .segments()
                .iter()
                .map(|segment| segment.as_str().to_string())
                .collect(),
        };
        let resolved = self.resolve_type_path(&type_path, resolve_offset);
        (resolved.len() == 1)
            .then(|| {
                semantic_token_info_for_symbol(
                    &resolved[0].def,
                    resolved[0].source.as_deref().or(Some(self.source)),
                )
            })
            .flatten()
    }

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
                            source: None,
                            origin_key: self
                                .current_file
                                .map(|path| path.to_string_lossy().to_string()),
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
                    source: container.source.clone(),
                    origin_key: container.origin_key.clone(),
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
                    source: Some(imported_source.clone()),
                    origin_key: Some(path.to_string_lossy().to_string()),
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
                                    source: container.source.clone(),
                                    origin_key: container.origin_key.clone(),
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
                    source: None,
                    origin_key: self
                        .current_file
                        .map(|path| path.to_string_lossy().to_string()),
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
                            source: Some(cross.source.clone()),
                            origin_key: Some(cross.resolved_path.to_string_lossy().to_string()),
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
                            source: container.source.clone(),
                            origin_key: container.origin_key.clone(),
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

const SEMANTIC_TOKEN_MODIFIER_DECLARATION: u32 = 1 << 0;
const SEMANTIC_TOKEN_MODIFIER_READONLY: u32 = 1 << 1;

pub(crate) fn semantic_token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::NAMESPACE,
            SemanticTokenType::CLASS,
            SemanticTokenType::INTERFACE,
            SemanticTokenType::ENUM,
            SemanticTokenType::STRUCT,
            SemanticTokenType::TYPE,
            SemanticTokenType::EVENT,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::MODIFIER,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::ENUM_MEMBER,
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::READONLY,
        ],
    }
}

pub(crate) fn semantic_tokens(
    source: &str,
    current_file: Option<&Path>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<SemanticToken> {
    encode_semantic_tokens(
        source,
        collect_raw_semantic_tokens(source, current_file, get_source, resolver),
    )
}

pub(crate) fn semantic_tokens_in_range(
    source: &str,
    range: std::ops::Range<usize>,
    current_file: Option<&Path>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<SemanticToken> {
    encode_semantic_tokens(
        source,
        collect_raw_semantic_tokens(source, current_file, get_source, resolver)
            .into_iter()
            .filter(|token| token.span.end > range.start && token.span.start < range.end)
            .collect(),
    )
}

fn collect_raw_semantic_tokens(
    source: &str,
    current_file: Option<&Path>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<RawSemanticToken> {
    let filename = current_file
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "buffer.sol".to_string());
    let Some(table) = symbols::build_symbol_table(source, &filename) else {
        return Vec::new();
    };
    let semantic = SemanticContext {
        table: &table,
        source,
        current_file,
        get_source,
        resolver,
    };

    let raw = with_parsed_ast_sequential(source, &filename, |source_unit| {
        let mut tokens = Vec::new();
        for item in source_unit.items.iter() {
            collect_item_semantic_tokens(item, &semantic, &mut tokens);
        }
        tokens
    })
    .unwrap_or_default();

    dedup_semantic_tokens(raw)
}

fn collect_item_semantic_tokens(
    item: &solar_ast::Item<'_>,
    semantic: &SemanticContext<'_>,
    tokens: &mut Vec<RawSemanticToken>,
) {
    match &item.kind {
        ItemKind::Pragma(_) => {}
        ItemKind::Import(import) => collect_import_semantic_tokens(import, semantic, tokens),
        ItemKind::Using(using_directive) => {
            match &using_directive.list {
                solar_ast::UsingList::Single(path) => {
                    collect_path_semantic_tokens(
                        path,
                        path.span().lo().0 as usize,
                        semantic,
                        tokens,
                    );
                }
                solar_ast::UsingList::Multiple(paths) => {
                    for (path, _) in paths.iter() {
                        collect_path_semantic_tokens(
                            path,
                            path.span().lo().0 as usize,
                            semantic,
                            tokens,
                        );
                    }
                }
            }
            if let Some(ty) = &using_directive.ty {
                collect_type_semantic_tokens(ty, semantic, tokens);
            }
        }
        ItemKind::Contract(contract) => {
            push_ident_semantic_token(
                contract.name,
                semantic_token_kind_for_contract(contract.kind),
                true,
                false,
                tokens,
            );
            if let Some(layout) = &contract.layout {
                collect_expr_semantic_tokens(layout.slot, semantic, tokens);
            }
            for base in contract.bases.iter() {
                collect_path_semantic_tokens(
                    &base.name,
                    base.name.span().lo().0 as usize,
                    semantic,
                    tokens,
                );
                for argument in base.arguments.exprs() {
                    collect_expr_semantic_tokens(argument, semantic, tokens);
                }
            }
            for body_item in contract.body.iter() {
                collect_item_semantic_tokens(body_item, semantic, tokens);
            }
        }
        ItemKind::Function(function) => {
            if let Some(name) = function.header.name {
                push_ident_semantic_token(
                    name,
                    semantic_token_kind_for_function(function.kind),
                    true,
                    false,
                    tokens,
                );
            }
            for parameter in function.header.parameters.iter() {
                collect_variable_declaration_tokens(
                    parameter,
                    SymbolKind::Parameter,
                    semantic,
                    tokens,
                );
            }
            for return_param in function.header.returns() {
                collect_variable_declaration_tokens(
                    return_param,
                    SymbolKind::ReturnParameter,
                    semantic,
                    tokens,
                );
            }
            for modifier in function.header.modifiers.iter() {
                collect_path_semantic_tokens(
                    &modifier.name,
                    modifier.name.span().lo().0 as usize,
                    semantic,
                    tokens,
                );
                for argument in modifier.arguments.exprs() {
                    collect_expr_semantic_tokens(argument, semantic, tokens);
                }
            }
            if let Some(override_) = &function.header.override_ {
                for path in override_.paths.iter() {
                    collect_path_semantic_tokens(
                        path,
                        path.span().lo().0 as usize,
                        semantic,
                        tokens,
                    );
                }
            }
            if let Some(body) = &function.body {
                for stmt in body.stmts.iter() {
                    collect_stmt_semantic_tokens(stmt, semantic, tokens);
                }
            }
        }
        ItemKind::Variable(variable) => {
            collect_variable_declaration_tokens(
                variable,
                SymbolKind::StateVariable,
                semantic,
                tokens,
            );
        }
        ItemKind::Struct(struct_) => {
            push_ident_semantic_token(struct_.name, SemanticTokenKind::Struct, true, false, tokens);
            for field in struct_.fields.iter() {
                collect_variable_declaration_tokens(
                    field,
                    SymbolKind::StructField,
                    semantic,
                    tokens,
                );
            }
        }
        ItemKind::Enum(enum_) => {
            push_ident_semantic_token(enum_.name, SemanticTokenKind::Enum, true, false, tokens);
            for variant in enum_.variants.iter() {
                push_ident_semantic_token(
                    *variant,
                    SemanticTokenKind::EnumMember,
                    true,
                    true,
                    tokens,
                );
            }
        }
        ItemKind::Udvt(udvt) => {
            push_ident_semantic_token(udvt.name, SemanticTokenKind::Type, true, false, tokens);
            collect_type_semantic_tokens(&udvt.ty, semantic, tokens);
        }
        ItemKind::Error(error) => {
            push_ident_semantic_token(error.name, SemanticTokenKind::Type, true, false, tokens);
            for parameter in error.parameters.iter() {
                collect_variable_declaration_tokens(
                    parameter,
                    SymbolKind::Parameter,
                    semantic,
                    tokens,
                );
            }
        }
        ItemKind::Event(event) => {
            push_ident_semantic_token(event.name, SemanticTokenKind::Event, true, false, tokens);
            for parameter in event.parameters.iter() {
                collect_variable_declaration_tokens(
                    parameter,
                    SymbolKind::Parameter,
                    semantic,
                    tokens,
                );
            }
        }
    }
}

fn collect_import_semantic_tokens(
    import: &solar_ast::ImportDirective<'_>,
    semantic: &SemanticContext<'_>,
    tokens: &mut Vec<RawSemanticToken>,
) {
    match &import.items {
        solar_ast::ImportItems::Plain(Some(alias)) | solar_ast::ImportItems::Glob(alias) => {
            push_ident_semantic_token(*alias, SemanticTokenKind::Namespace, true, false, tokens);
        }
        solar_ast::ImportItems::Aliases(items) => {
            for (original, alias) in items.iter() {
                let local_name = alias.unwrap_or(*original);
                let token_info = semantic
                    .current_file
                    .and_then(|current_file| {
                        definition::resolve_cross_file_symbol(
                            semantic.table,
                            local_name.as_str(),
                            current_file,
                            semantic.get_source,
                            semantic.resolver,
                        )
                    })
                    .and_then(|cross| {
                        semantic_token_info_for_symbol(&cross.def, Some(&cross.source))
                    })
                    .unwrap_or(SemanticTokenInfo {
                        kind: SemanticTokenKind::Type,
                        readonly: false,
                    });
                push_ident_semantic_token(
                    local_name,
                    token_info.kind,
                    true,
                    token_info.readonly,
                    tokens,
                );
            }
        }
        solar_ast::ImportItems::Plain(None) => {}
    }
}

fn collect_variable_declaration_tokens(
    variable: &solar_ast::VariableDefinition<'_>,
    kind: SymbolKind,
    semantic: &SemanticContext<'_>,
    tokens: &mut Vec<RawSemanticToken>,
) {
    collect_type_semantic_tokens(&variable.ty, semantic, tokens);
    if let Some(override_) = &variable.override_ {
        for path in override_.paths.iter() {
            collect_path_semantic_tokens(path, path.span().lo().0 as usize, semantic, tokens);
        }
    }
    if let Some(name) = variable.name {
        if let Some(token_kind) = semantic_token_kind_for_symbol(kind) {
            push_ident_semantic_token(
                name,
                token_kind,
                true,
                variable.mutability.is_some(),
                tokens,
            );
        }
    }
    if let Some(initializer) = &variable.initializer {
        collect_expr_semantic_tokens(initializer, semantic, tokens);
    }
}

fn collect_type_semantic_tokens(
    ty: &solar_ast::Type<'_>,
    semantic: &SemanticContext<'_>,
    tokens: &mut Vec<RawSemanticToken>,
) {
    match &ty.kind {
        solar_ast::TypeKind::Elementary(_) => {}
        solar_ast::TypeKind::Custom(path) => {
            collect_path_semantic_tokens(path, ty.span.lo().0 as usize, semantic, tokens);
        }
        solar_ast::TypeKind::Array(array) => {
            collect_type_semantic_tokens(&array.element, semantic, tokens);
            if let Some(size) = &array.size {
                collect_expr_semantic_tokens(size, semantic, tokens);
            }
        }
        solar_ast::TypeKind::Function(function_ty) => {
            for parameter in function_ty.parameters.iter() {
                collect_variable_declaration_tokens(
                    parameter,
                    SymbolKind::Parameter,
                    semantic,
                    tokens,
                );
            }
            for return_param in function_ty.returns() {
                collect_variable_declaration_tokens(
                    return_param,
                    SymbolKind::ReturnParameter,
                    semantic,
                    tokens,
                );
            }
        }
        solar_ast::TypeKind::Mapping(mapping) => {
            collect_type_semantic_tokens(&mapping.key, semantic, tokens);
            collect_type_semantic_tokens(&mapping.value, semantic, tokens);
        }
    }
}

fn collect_stmt_semantic_tokens(
    stmt: &Stmt<'_>,
    semantic: &SemanticContext<'_>,
    tokens: &mut Vec<RawSemanticToken>,
) {
    match &stmt.kind {
        StmtKind::Assembly(_) | StmtKind::Break | StmtKind::Continue | StmtKind::Placeholder => {}
        StmtKind::DeclSingle(variable) => {
            collect_variable_declaration_tokens(
                variable,
                SymbolKind::LocalVariable,
                semantic,
                tokens,
            );
        }
        StmtKind::DeclMulti(vars, expr) => {
            for variable in vars.iter() {
                if let solgrid_parser::solar_interface::SpannedOption::Some(variable) = variable {
                    collect_variable_declaration_tokens(
                        variable,
                        SymbolKind::LocalVariable,
                        semantic,
                        tokens,
                    );
                }
            }
            collect_expr_semantic_tokens(expr, semantic, tokens);
        }
        StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
            for stmt in block.stmts.iter() {
                collect_stmt_semantic_tokens(stmt, semantic, tokens);
            }
        }
        StmtKind::DoWhile(body, expr) | StmtKind::While(expr, body) => {
            collect_expr_semantic_tokens(expr, semantic, tokens);
            collect_stmt_semantic_tokens(body, semantic, tokens);
        }
        StmtKind::Emit(path, args) => {
            collect_path_semantic_tokens(path, path.span().lo().0 as usize, semantic, tokens);
            for arg in args.exprs() {
                collect_expr_semantic_tokens(arg, semantic, tokens);
            }
        }
        StmtKind::Revert(path, args) => {
            collect_path_semantic_tokens(path, path.span().lo().0 as usize, semantic, tokens);
            for arg in args.exprs() {
                collect_expr_semantic_tokens(arg, semantic, tokens);
            }
        }
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => {
            collect_expr_semantic_tokens(expr, semantic, tokens);
        }
        StmtKind::Return(None) => {}
        StmtKind::For {
            init,
            cond,
            next,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_semantic_tokens(init, semantic, tokens);
            }
            if let Some(cond) = cond {
                collect_expr_semantic_tokens(cond, semantic, tokens);
            }
            if let Some(next) = next {
                collect_expr_semantic_tokens(next, semantic, tokens);
            }
            collect_stmt_semantic_tokens(body, semantic, tokens);
        }
        StmtKind::If(cond, then_stmt, else_stmt) => {
            collect_expr_semantic_tokens(cond, semantic, tokens);
            collect_stmt_semantic_tokens(then_stmt, semantic, tokens);
            if let Some(else_stmt) = else_stmt {
                collect_stmt_semantic_tokens(else_stmt, semantic, tokens);
            }
        }
        StmtKind::Try(try_stmt) => {
            collect_expr_semantic_tokens(try_stmt.expr, semantic, tokens);
            for clause in try_stmt.clauses.iter() {
                for stmt in clause.block.stmts.iter() {
                    collect_stmt_semantic_tokens(stmt, semantic, tokens);
                }
            }
        }
    }
}

fn collect_expr_semantic_tokens(
    expr: &Expr<'_>,
    semantic: &SemanticContext<'_>,
    tokens: &mut Vec<RawSemanticToken>,
) {
    match &expr.kind {
        ExprKind::Array(exprs) => {
            for expr in exprs.iter() {
                collect_expr_semantic_tokens(expr, semantic, tokens);
            }
        }
        ExprKind::Assign(lhs, _, rhs) | ExprKind::Binary(lhs, _, rhs) => {
            collect_expr_semantic_tokens(lhs, semantic, tokens);
            collect_expr_semantic_tokens(rhs, semantic, tokens);
        }
        ExprKind::Call(callee, args) => {
            collect_expr_semantic_tokens(callee, semantic, tokens);
            for arg in args.exprs() {
                collect_expr_semantic_tokens(arg, semantic, tokens);
            }
        }
        ExprKind::CallOptions(callee, args) => {
            collect_expr_semantic_tokens(callee, semantic, tokens);
            for arg in args.iter() {
                collect_expr_semantic_tokens(arg.value, semantic, tokens);
            }
        }
        ExprKind::Delete(expr) | ExprKind::Unary(_, expr) => {
            collect_expr_semantic_tokens(expr, semantic, tokens);
        }
        ExprKind::Ident(ident) => {
            if let Some(info) = semantic.resolve_ident_info(ident) {
                push_ident_semantic_token(*ident, info.kind, false, info.readonly, tokens);
            }
        }
        ExprKind::Index(lhs, kind) => {
            collect_expr_semantic_tokens(lhs, semantic, tokens);
            match kind {
                IndexKind::Index(Some(expr)) => {
                    collect_expr_semantic_tokens(expr, semantic, tokens)
                }
                IndexKind::Range(start, end) => {
                    if let Some(start) = start {
                        collect_expr_semantic_tokens(start, semantic, tokens);
                    }
                    if let Some(end) = end {
                        collect_expr_semantic_tokens(end, semantic, tokens);
                    }
                }
                IndexKind::Index(None) => {}
            }
        }
        ExprKind::Lit(_, _) => {}
        ExprKind::Member(base, member) => {
            collect_expr_semantic_tokens(base, semantic, tokens);
            if let Some(info) = semantic.resolve_member_info(base, member) {
                push_ident_semantic_token(*member, info.kind, false, info.readonly, tokens);
            }
        }
        ExprKind::New(ty) | ExprKind::TypeCall(ty) | ExprKind::Type(ty) => {
            collect_type_semantic_tokens(ty, semantic, tokens);
        }
        ExprKind::Payable(args) => {
            for arg in args.exprs() {
                collect_expr_semantic_tokens(arg, semantic, tokens);
            }
        }
        ExprKind::Ternary(cond, if_true, if_false) => {
            collect_expr_semantic_tokens(cond, semantic, tokens);
            collect_expr_semantic_tokens(if_true, semantic, tokens);
            collect_expr_semantic_tokens(if_false, semantic, tokens);
        }
        ExprKind::Tuple(exprs) => {
            for expr in exprs.iter() {
                if let solgrid_parser::solar_interface::SpannedOption::Some(expr) = expr {
                    collect_expr_semantic_tokens(expr, semantic, tokens);
                }
            }
        }
    }
}

fn collect_path_semantic_tokens(
    path: &solar_ast::AstPath<'_>,
    resolve_offset: usize,
    semantic: &SemanticContext<'_>,
    tokens: &mut Vec<RawSemanticToken>,
) {
    if path.segments().len() >= 2 && semantic.is_namespace_alias(path.first().as_str()) {
        push_ident_semantic_token(
            *path.first(),
            SemanticTokenKind::Namespace,
            false,
            false,
            tokens,
        );
    }
    if let Some(info) = semantic.resolve_path_info(path, resolve_offset) {
        push_ident_semantic_token(*path.last(), info.kind, false, info.readonly, tokens);
    }
}

fn semantic_token_kind_for_contract(kind: ContractKind) -> SemanticTokenKind {
    match kind {
        ContractKind::Contract | ContractKind::AbstractContract | ContractKind::Library => {
            SemanticTokenKind::Class
        }
        ContractKind::Interface => SemanticTokenKind::Interface,
    }
}

fn semantic_token_kind_for_function(kind: FunctionKind) -> SemanticTokenKind {
    match kind {
        FunctionKind::Modifier => SemanticTokenKind::Modifier,
        FunctionKind::Constructor | FunctionKind::Function => SemanticTokenKind::Function,
        FunctionKind::Fallback | FunctionKind::Receive => SemanticTokenKind::Function,
    }
}

fn semantic_token_kind_for_symbol(kind: SymbolKind) -> Option<SemanticTokenKind> {
    Some(match kind {
        SymbolKind::Contract => SemanticTokenKind::Class,
        SymbolKind::Interface => SemanticTokenKind::Interface,
        SymbolKind::Library => SemanticTokenKind::Class,
        SymbolKind::Constructor | SymbolKind::Function => SemanticTokenKind::Function,
        SymbolKind::Modifier => SemanticTokenKind::Modifier,
        SymbolKind::Event => SemanticTokenKind::Event,
        SymbolKind::Error | SymbolKind::Udvt => SemanticTokenKind::Type,
        SymbolKind::Struct => SemanticTokenKind::Struct,
        SymbolKind::StructField | SymbolKind::StateVariable => SemanticTokenKind::Property,
        SymbolKind::Enum => SemanticTokenKind::Enum,
        SymbolKind::LocalVariable => SemanticTokenKind::Variable,
        SymbolKind::Parameter | SymbolKind::ReturnParameter => SemanticTokenKind::Parameter,
        SymbolKind::EnumVariant => SemanticTokenKind::EnumMember,
    })
}

fn semantic_token_info_for_symbol(
    def: &SymbolDef,
    source: Option<&str>,
) -> Option<SemanticTokenInfo> {
    Some(SemanticTokenInfo {
        kind: semantic_token_kind_for_symbol(def.kind)?,
        readonly: semantic_token_symbol_is_readonly(def, source),
    })
}

fn semantic_token_symbol_is_readonly(def: &SymbolDef, source: Option<&str>) -> bool {
    match def.kind {
        SymbolKind::EnumVariant => true,
        SymbolKind::StateVariable => source
            .and_then(|source| source.get(def.def_span.clone()))
            .is_some_and(|snippet| {
                snippet.contains("constant")
                    || snippet.contains("immutable")
                    || snippet.contains("Constant")
                    || snippet.contains("Immutable")
            }),
        _ => false,
    }
}

fn push_ident_semantic_token(
    ident: solar_ast::Ident,
    kind: SemanticTokenKind,
    declaration: bool,
    readonly: bool,
    tokens: &mut Vec<RawSemanticToken>,
) {
    let mut modifiers = 0;
    if declaration {
        modifiers |= SEMANTIC_TOKEN_MODIFIER_DECLARATION;
    }
    if readonly {
        modifiers |= SEMANTIC_TOKEN_MODIFIER_READONLY;
    }
    tokens.push(RawSemanticToken {
        span: solgrid_ast::span_to_range(ident.span),
        kind,
        modifiers,
    });
}

fn dedup_semantic_tokens(mut tokens: Vec<RawSemanticToken>) -> Vec<RawSemanticToken> {
    tokens.sort_by(|left, right| {
        left.span
            .start
            .cmp(&right.span.start)
            .then_with(|| left.span.end.cmp(&right.span.end))
            .then_with(|| right.modifiers.cmp(&left.modifiers))
    });

    let mut deduped: Vec<RawSemanticToken> = Vec::new();
    for token in tokens {
        if let Some(previous) = deduped.last() {
            if previous.span == token.span {
                continue;
            }
        }
        deduped.push(token);
    }
    deduped
}

fn encode_semantic_tokens(source: &str, tokens: Vec<RawSemanticToken>) -> Vec<SemanticToken> {
    let mut encoded = Vec::new();
    let mut previous_line = 0u32;
    let mut previous_start = 0u32;
    let mut first = true;

    for token in tokens {
        let start = convert::offset_to_position(source, token.span.start);
        let end = convert::offset_to_position(source, token.span.end);
        if start.line != end.line || start.character >= end.character {
            continue;
        }

        let delta_line = if first {
            start.line
        } else {
            start.line.saturating_sub(previous_line)
        };
        let delta_start = if first || delta_line > 0 {
            start.character
        } else {
            start.character.saturating_sub(previous_start)
        };
        encoded.push(SemanticToken {
            delta_line,
            delta_start,
            length: end.character - start.character,
            token_type: semantic_token_type_index(token.kind),
            token_modifiers_bitset: token.modifiers,
        });
        previous_line = start.line;
        previous_start = start.character;
        first = false;
    }

    encoded
}

fn semantic_token_type_index(kind: SemanticTokenKind) -> u32 {
    match kind {
        SemanticTokenKind::Namespace => 0,
        SemanticTokenKind::Class => 1,
        SemanticTokenKind::Interface => 2,
        SemanticTokenKind::Enum => 3,
        SemanticTokenKind::Struct => 4,
        SemanticTokenKind::Type => 5,
        SemanticTokenKind::Event => 6,
        SemanticTokenKind::Function => 7,
        SemanticTokenKind::Modifier => 8,
        SemanticTokenKind::Parameter => 9,
        SemanticTokenKind::Variable => 10,
        SemanticTokenKind::Property => 11,
        SemanticTokenKind::EnumMember => 12,
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

pub(crate) fn parameter_name_hints_in_range(
    source: &str,
    start_offset: usize,
    end_offset: usize,
    current_file: Option<&Path>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<ParameterInlayHint> {
    let Some(table) = symbols::build_symbol_table(source, "buffer.sol") else {
        return Vec::new();
    };
    let semantic = SemanticContext {
        table: &table,
        source,
        current_file,
        get_source,
        resolver,
    };

    with_parsed_ast_sequential(source, "buffer.sol", |source_unit| {
        let mut hints = Vec::new();
        let mut seen = HashSet::new();

        visit_source_unit_exprs(source_unit, &mut |expr| {
            let ExprKind::Call(callee, args) = &expr.kind else {
                return;
            };
            let expr_range = solgrid_ast::span_to_range(expr.span);
            if expr_range.end < start_offset || expr_range.start > end_offset {
                return;
            }

            let Some(parameter_names) = parameter_names_for_call(&semantic, callee, args.len())
            else {
                return;
            };

            for (index, argument) in args.exprs().enumerate() {
                let Some(parameter_name) = parameter_names.get(index) else {
                    continue;
                };
                let arg_range = solgrid_ast::span_to_range(argument.span);
                if arg_range.start < start_offset || arg_range.start > end_offset {
                    continue;
                }
                if argument_already_matches_parameter(argument, parameter_name) {
                    continue;
                }
                if seen.insert((arg_range.start, parameter_name.clone())) {
                    hints.push(ParameterInlayHint {
                        offset: arg_range.start,
                        label: format!("{parameter_name}:"),
                    });
                }
            }
        });

        hints.sort_by(|left, right| left.offset.cmp(&right.offset));
        hints
    })
    .unwrap_or_default()
}

pub(crate) fn selector_hints_in_range(
    source: &str,
    start_offset: usize,
    end_offset: usize,
    current_file: Option<&Path>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<SelectorInlayHint> {
    let current_file = current_file.unwrap_or_else(|| Path::new("buffer.sol"));

    with_parsed_ast_sequential(source, "buffer.sol", |source_unit| {
        struct SelectorHintCollector<'a, 'ast> {
            source: &'a str,
            start_offset: usize,
            end_offset: usize,
            contract_stack: Vec<String>,
            selectors: SelectorContext<'a>,
            seen: HashSet<(usize, String)>,
            hints: Vec<SelectorInlayHint>,
            _marker: std::marker::PhantomData<&'ast ()>,
        }

        impl<'a, 'ast> SelectorHintCollector<'a, 'ast> {
            fn visit_item(&mut self, item: &solar_ast::Item<'ast>) {
                match &item.kind {
                    ItemKind::Contract(contract) => {
                        if contract.kind == ContractKind::Interface {
                            if let Some(interface_id) = self
                                .selectors
                                .interface_id_info_for_items(contract.name.as_str(), contract.body)
                            {
                                let offset = declaration_hint_offset(
                                    self.source,
                                    solgrid_ast::span_to_range(item.span),
                                );
                                let label = format!("interface ID: {}", interface_id.hex);
                                if offset >= self.start_offset
                                    && offset <= self.end_offset
                                    && self.seen.insert((offset, label.clone()))
                                {
                                    self.hints.push(SelectorInlayHint {
                                        offset,
                                        label,
                                        tooltip: format!(
                                            "ERC-165 interface ID for `{}`",
                                            contract.name.as_str()
                                        ),
                                    });
                                }
                            }
                        }

                        self.contract_stack.push(contract.name.as_str().to_string());
                        for body_item in contract.body.iter() {
                            self.visit_item(body_item);
                        }
                        self.contract_stack.pop();
                    }
                    ItemKind::Function(function)
                        if is_selector_visible_function(function)
                            || (self.contract_stack.is_empty()
                                && function.kind == FunctionKind::Function) =>
                    {
                        if let Some(selector) = self.selectors.function_selector_info(
                            self.contract_stack.last().map(String::as_str),
                            function,
                        ) {
                            let offset = declaration_hint_offset(
                                self.source,
                                solgrid_ast::span_to_range(function.header.span),
                            );
                            let label = format!("selector: {}", selector.hex);
                            if offset >= self.start_offset
                                && offset <= self.end_offset
                                && self.seen.insert((offset, label.clone()))
                            {
                                self.hints.push(SelectorInlayHint {
                                    offset,
                                    label,
                                    tooltip: format!(
                                        "Function selector for `{}`",
                                        selector.signature
                                    ),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut collector = SelectorHintCollector {
            source,
            start_offset,
            end_offset,
            contract_stack: Vec::new(),
            selectors: SelectorContext::new(source, current_file, resolver, get_source),
            seen: HashSet::new(),
            hints: Vec::new(),
            _marker: std::marker::PhantomData,
        };

        for item in source_unit.items.iter() {
            collector.visit_item(item);
        }

        collector
            .hints
            .sort_by(|left, right| left.offset.cmp(&right.offset));
        collector.hints
    })
    .unwrap_or_default()
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

fn parameter_names_for_call(
    semantic: &SemanticContext<'_>,
    callee: &Expr<'_>,
    arg_count: usize,
) -> Option<Vec<String>> {
    let mut candidates = semantic
        .resolve_user_call_signatures(callee)
        .into_iter()
        .map(parameter_names_from_signature_data)
        .collect::<Vec<_>>();
    candidates.extend(
        semantic
            .resolve_builtin_signatures(callee)
            .into_iter()
            .map(parameter_names_from_resolved_signature),
    );
    consistent_parameter_names(candidates, arg_count)
}

fn parameter_names_from_signature_data(signature: SignatureData) -> Vec<Option<String>> {
    signature
        .parameters
        .into_iter()
        .map(|parameter| extract_parameter_name(&parameter.label))
        .collect()
}

fn parameter_names_from_resolved_signature(signature: ResolvedSignature) -> Vec<Option<String>> {
    signature
        .parameter_ranges
        .into_iter()
        .map(|(start, end)| extract_parameter_name(&signature.label[start as usize..end as usize]))
        .collect()
}

fn consistent_parameter_names(
    candidates: Vec<Vec<Option<String>>>,
    arg_count: usize,
) -> Option<Vec<String>> {
    let candidates = candidates
        .into_iter()
        .filter(|candidate| candidate.len() >= arg_count)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }

    let mut parameter_names = Vec::with_capacity(arg_count);
    for index in 0..arg_count {
        let mut agreed_name: Option<&str> = None;
        for candidate in &candidates {
            let name = candidate.get(index)?.as_deref()?;
            if let Some(existing) = agreed_name {
                if existing != name {
                    return None;
                }
            } else {
                agreed_name = Some(name);
            }
        }
        parameter_names.push(agreed_name?.to_string());
    }

    Some(parameter_names)
}

fn declaration_hint_offset(source: &str, span: std::ops::Range<usize>) -> usize {
    let mut offset = span.end;
    while offset > span.start {
        let ch = source[..offset].chars().next_back().unwrap_or_default();
        if ch.is_whitespace() {
            offset -= ch.len_utf8();
            continue;
        }
        if matches!(ch, '{' | ';') {
            offset -= ch.len_utf8();
        }
        break;
    }
    offset
}

fn is_selector_visible_function(function: &solar_ast::ItemFunction<'_>) -> bool {
    matches!(
        function.header.visibility(),
        Some(Visibility::Public | Visibility::External)
    ) && function.kind == FunctionKind::Function
}

fn extract_parameter_name(label: &str) -> Option<String> {
    let parts = label.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }

    let candidate = parts.last().copied()?;
    (is_hint_identifier(candidate) && !is_modifier_keyword(candidate))
        .then(|| candidate.to_string())
}

fn argument_already_matches_parameter(argument: &Expr<'_>, parameter_name: &str) -> bool {
    match &argument.peel_parens().kind {
        ExprKind::Ident(ident) => ident.as_str() == parameter_name,
        ExprKind::Member(_, member) => member.as_str() == parameter_name,
        _ => false,
    }
}

fn is_hint_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn is_modifier_keyword(value: &str) -> bool {
    matches!(
        value,
        "memory" | "storage" | "calldata" | "payable" | "indexed" | "virtual" | "override"
    )
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
            resolved.origin_key.clone(),
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
    use crate::resolve::ImportResolver;

    fn noop_source(_path: &Path) -> Option<String> {
        None
    }

    fn noop_resolver() -> ImportResolver {
        ImportResolver::new(None)
    }

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

    #[test]
    fn test_parameter_name_hints_for_positional_arguments() {
        let source = r#"contract Token {
    function transfer(address recipient, uint256 amount) public {}

    function run() public {
        transfer(address(0), 1);
    }
}"#;

        let hints = parameter_name_hints_in_range(
            source,
            source.find("transfer(address").unwrap(),
            source.len(),
            None,
            &noop_source,
            &noop_resolver(),
        );

        let labels = hints.into_iter().map(|hint| hint.label).collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec!["recipient:".to_string(), "amount:".to_string()]
        );
    }

    #[test]
    fn test_parameter_name_hints_skip_matching_identifier_names() {
        let source = r#"contract Token {
    function transfer(address recipient, uint256 amount) public {}

    function run(address recipient, uint256 amount) public {
        transfer(recipient, amount);
    }
}"#;

        let hints = parameter_name_hints_in_range(
            source,
            source.find("transfer(recipient").unwrap(),
            source.len(),
            None,
            &noop_source,
            &noop_resolver(),
        );

        assert!(hints.is_empty());
    }

    #[test]
    fn test_selector_hints_for_abi_declarations() {
        let source = r#"interface IRouter {
    function swap(address tokenIn, uint256 amountIn) external returns (uint256);
}

contract Router {
    function swap(address tokenIn, uint256 amountIn) public returns (uint256) {
        return amountIn;
    }

    function helper(uint256 amount) internal {}
}"#;

        let hints = selector_hints_in_range(
            source,
            0,
            source.len(),
            None,
            &noop_source,
            &noop_resolver(),
        );
        let labels = hints.into_iter().map(|hint| hint.label).collect::<Vec<_>>();

        assert_eq!(
            labels
                .iter()
                .filter(|label| label.starts_with("selector: "))
                .count(),
            2
        );
        assert!(labels
            .iter()
            .any(|label| label.starts_with("interface ID: ")));
        assert_eq!(labels.len(), 3);
    }
}
