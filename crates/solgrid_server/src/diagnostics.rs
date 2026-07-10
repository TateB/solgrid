//! Diagnostics — real-time lint integration for the LSP server.

use crate::convert;
use crate::resolve::ImportResolver;
use solgrid_ast::resolve::ImportResolver as SharedImportResolver;
use solgrid_ast::symbols::{
    self, ImportedSymbols, SignatureData, SymbolDef, SymbolKind, SymbolTable, TypePath, TypeSpec,
};
use solgrid_config::Config;
use solgrid_diagnostics::{
    Confidence, FileResult, FindingKind, FindingMeta, RuleCategory, RuleMeta, Severity,
};
use solgrid_linter::suppression::{parse_suppressions, Suppressions};
use solgrid_linter::LintEngine;
use solgrid_parser::solar_ast::{self, Expr, ExprKind, IndexKind, ItemKind, Stmt, StmtKind, Type};
use solgrid_parser::solar_interface::SpannedOption;
use solgrid_parser::with_parsed_ast_sequential;
use solgrid_project::{
    resolve_cross_file_member_symbol, resolve_cross_file_symbol, NavBackend, ProjectIndex,
    ProjectSnapshot,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use tower_lsp_server::ls_types;

/// Run the linter on source text and return LSP diagnostics.
pub fn lint_to_lsp_diagnostics(
    engine: &LintEngine,
    source: &str,
    path: &Path,
    config: &Config,
) -> Vec<ls_types::Diagnostic> {
    let result = engine.lint_source(source, path, config);
    file_result_to_lsp_diagnostics_with_meta(source, &result, rule_meta_map(engine))
}

/// Run the linter with an explicit remapping set and return LSP diagnostics.
pub fn lint_to_lsp_diagnostics_with_remappings(
    engine: &LintEngine,
    source: &str,
    path: &Path,
    config: &Config,
    remappings: &[(String, PathBuf)],
) -> Vec<ls_types::Diagnostic> {
    let result = engine.lint_source_with_remappings(source, path, config, remappings);
    file_result_to_lsp_diagnostics_with_meta(source, &result, rule_meta_map(engine))
}

/// Convert a FileResult to LSP diagnostics.
pub fn file_result_to_lsp_diagnostics(
    source: &str,
    result: &FileResult,
) -> Vec<ls_types::Diagnostic> {
    result
        .diagnostics
        .iter()
        .map(|d| convert::diagnostic_to_lsp(source, d))
        .collect()
}

fn file_result_to_lsp_diagnostics_with_meta(
    source: &str,
    result: &FileResult,
    rule_meta: HashMap<&str, &RuleMeta>,
) -> Vec<ls_types::Diagnostic> {
    result
        .diagnostics
        .iter()
        .map(|diag| {
            let data = rule_meta
                .get(diag.rule_id.as_str())
                .and_then(|meta| serde_json::to_value(meta.finding_meta_for_diagnostic(diag)).ok());
            convert::diagnostic_to_lsp_with_data(source, diag, data)
        })
        .collect()
}

fn rule_meta_map(engine: &LintEngine) -> HashMap<&str, &RuleMeta> {
    engine
        .registry()
        .all_meta()
        .into_iter()
        .map(|meta| (meta.id, meta))
        .collect()
}

/// Produce diagnostics for import paths that cannot be resolved.
pub fn unresolved_import_diagnostics(
    source: &str,
    path: &Path,
    resolver: &ImportResolver,
) -> Vec<ls_types::Diagnostic> {
    let table = match symbols::build_symbol_table(source, &path.to_string_lossy()) {
        Some(t) => t,
        None => return Vec::new(),
    };

    table
        .imports
        .iter()
        .filter(|import| resolver.resolve(&import.path, path).is_none())
        .map(|import| {
            compiler_lsp_diagnostic(
                source,
                "compiler/unresolved-import",
                "Unresolved import",
                format!("cannot resolve import \"{}\"", import.path),
                import.path_span.clone(),
            )
        })
        .collect()
}

/// Produce compiler-style semantic diagnostics for unresolved references.
pub fn compiler_to_lsp_diagnostics<B: NavBackend>(
    project_index: &ProjectIndex<B>,
    source: &str,
    path: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
) -> Vec<ls_types::Diagnostic> {
    compiler_to_lsp_diagnostics_with_config(
        project_index,
        source,
        path,
        get_source,
        &Config::default(),
    )
}

/// Produce compiler-style semantic diagnostics using the active lint configuration.
///
/// Native semantic detectors share the same enablement, severity, and inline
/// suppression behavior as registry-backed lint rules. Compiler diagnostics are
/// intentionally unaffected by lint configuration.
pub fn compiler_to_lsp_diagnostics_with_config<B: NavBackend>(
    project_index: &ProjectIndex<B>,
    source: &str,
    path: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    config: &Config,
) -> Vec<ls_types::Diagnostic> {
    let mut diagnostics = unresolved_import_diagnostics(source, path, project_index.resolver());
    let Some(snapshot) = project_index.snapshot_for_source(path, source) else {
        return diagnostics;
    };

    let filename = snapshot.path.to_string_lossy().to_string();
    let mut context =
        CompilerDiagnosticContext::new(&snapshot, project_index.resolver(), get_source, config);

    let _ = with_parsed_ast_sequential(source, &filename, |source_unit| {
        for item in source_unit.items.iter() {
            context.visit_item(item);
        }
    });

    diagnostics.extend(context.finish());
    diagnostics.sort_by(|left, right| {
        left.range
            .start
            .line
            .cmp(&right.range.start.line)
            .then_with(|| left.range.start.character.cmp(&right.range.start.character))
            .then_with(|| left.message.cmp(&right.message))
    });
    diagnostics
}

/// Suppress lower-signal diagnostics when a more specific finding overlaps.
pub fn suppress_redundant_diagnostics(
    diagnostics: Vec<ls_types::Diagnostic>,
) -> Vec<ls_types::Diagnostic> {
    let mut suppressed = vec![false; diagnostics.len()];

    for (index, diagnostic) in diagnostics.iter().enumerate() {
        let Some(code) = diagnostic_code(diagnostic) else {
            continue;
        };
        let suppressed_codes = suppressed_rule_ids(code);
        if suppressed_codes.is_empty() {
            continue;
        }

        for (candidate_index, candidate) in diagnostics.iter().enumerate() {
            if index == candidate_index || suppressed[candidate_index] {
                continue;
            }
            let Some(candidate_code) = diagnostic_code(candidate) else {
                continue;
            };
            if suppressed_codes.contains(&candidate_code)
                && ranges_overlap(&diagnostic.range, &candidate.range)
            {
                suppressed[candidate_index] = true;
            }
        }
    }

    diagnostics
        .into_iter()
        .enumerate()
        .filter_map(|(index, diagnostic)| (!suppressed[index]).then_some(diagnostic))
        .collect()
}

const UNCHECKED_LOW_LEVEL_CALL_ID: &str = "security/unchecked-low-level-call";
const UNCHECKED_LOW_LEVEL_CALL_TITLE: &str = "Unchecked low-level call";
const USER_CONTROLLED_DELEGATECALL_ID: &str = "security/user-controlled-delegatecall";
const USER_CONTROLLED_DELEGATECALL_TITLE: &str = "User-controlled delegatecall target";
const USER_CONTROLLED_ETH_TRANSFER_ID: &str = "security/user-controlled-eth-transfer";
const USER_CONTROLLED_ETH_TRANSFER_TITLE: &str = "User-controlled ETH transfer target";

struct CompilerDiagnosticContext<'a> {
    snapshot: &'a ProjectSnapshot,
    resolver: &'a SharedImportResolver,
    get_source: &'a dyn Fn(&Path) -> Option<String>,
    diagnostics: Vec<ls_types::Diagnostic>,
    seen: HashSet<(String, usize, usize, String)>,
    config: &'a Config,
    suppressions: Suppressions,
    semantic_files: HashMap<PathBuf, FileSemanticInfo>,
    function_summaries: HashMap<CallableTargetKey, FunctionSinkSummary>,
    current_contracts: Vec<String>,
}

fn ast_path_to_type_path(path: &solar_ast::AstPath<'_>) -> TypePath {
    TypePath {
        segments: path
            .segments()
            .iter()
            .map(|segment| segment.as_str().to_string())
            .collect(),
    }
}

impl<'a> CompilerDiagnosticContext<'a> {
    fn new(
        snapshot: &'a ProjectSnapshot,
        resolver: &'a SharedImportResolver,
        get_source: &'a dyn Fn(&Path) -> Option<String>,
        config: &'a Config,
    ) -> Self {
        let (semantic_files, function_summaries) =
            build_function_sink_summaries(snapshot, resolver, get_source);
        Self {
            snapshot,
            resolver,
            get_source,
            diagnostics: Vec::new(),
            seen: HashSet::new(),
            config,
            suppressions: parse_suppressions(&snapshot.source),
            semantic_files,
            function_summaries,
            current_contracts: Vec::new(),
        }
    }

    fn finish(self) -> Vec<ls_types::Diagnostic> {
        self.diagnostics
    }

    fn push(&mut self, id: &str, title: &str, message: String, span: std::ops::Range<usize>) {
        if !self
            .seen
            .insert((id.to_string(), span.start, span.end, message.clone()))
        {
            return;
        }
        self.diagnostics.push(compiler_lsp_diagnostic(
            &self.snapshot.source,
            id,
            title,
            message,
            span,
        ));
    }

    fn push_detector(
        &mut self,
        id: &str,
        title: &str,
        message: String,
        span: std::ops::Range<usize>,
        default_severity: Severity,
        confidence: Confidence,
    ) {
        if !self.config.lint.is_rule_enabled(id, RuleCategory::Security) {
            return;
        }
        let Some(severity) = self.config.lint.rule_severity(id, default_severity) else {
            return;
        };
        let line = self.snapshot.source[..span.start.min(self.snapshot.source.len())]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        if self.suppressions.is_suppressed(id, line) {
            return;
        }
        if !self
            .seen
            .insert((id.to_string(), span.start, span.end, message.clone()))
        {
            return;
        }
        self.diagnostics.push(detector_lsp_diagnostic(
            &self.snapshot.source,
            id,
            title,
            message,
            span,
            severity,
            confidence,
        ));
    }

    fn visit_item(&mut self, item: &solar_ast::Item<'_>) {
        match &item.kind {
            ItemKind::Contract(contract) => {
                for base in contract.bases.iter() {
                    let base_name = base.name.to_string();
                    let base_span = solgrid_ast::span_to_range(base.name.span());
                    if !self.resolve_ast_path(&base.name, base_span.start) {
                        self.push(
                            "compiler/unresolved-base-contract",
                            "Unresolved base contract",
                            format!("cannot resolve base contract \"{base_name}\""),
                            base_span,
                        );
                    }
                    for argument in base.arguments.exprs() {
                        self.visit_expr(argument);
                    }
                }

                self.current_contracts
                    .push(contract.name.as_str().to_string());
                for body_item in contract.body.iter() {
                    self.visit_item(body_item);
                }
                self.current_contracts.pop();
            }
            ItemKind::Function(function) => {
                for parameter in function.header.parameters.iter() {
                    self.visit_variable_definition(parameter);
                }
                for return_param in function.header.returns() {
                    self.visit_variable_definition(return_param);
                }
                for modifier in function.header.modifiers.iter() {
                    let modifier_name = modifier.name.to_string();
                    let modifier_span = solgrid_ast::span_to_range(modifier.name.span());
                    if !self.resolve_modifier_path(&modifier.name, modifier_span.start) {
                        self.push(
                            "compiler/unresolved-modifier",
                            "Unresolved modifier",
                            format!(
                                "cannot resolve modifier or base constructor \"{modifier_name}\""
                            ),
                            modifier_span,
                        );
                    }
                    for argument in modifier.arguments.exprs() {
                        self.visit_expr(argument);
                    }
                }
                if let Some(override_) = &function.header.override_ {
                    for path in override_.paths.iter() {
                        let override_name = path.to_string();
                        let override_span = solgrid_ast::span_to_range(path.span());
                        if !self.resolve_ast_path(path, override_span.start) {
                            self.push(
                                "compiler/unresolved-override",
                                "Unresolved override target",
                                format!("cannot resolve override target \"{override_name}\""),
                                override_span,
                            );
                        }
                    }
                }
                if let Some(body) = &function.body {
                    for stmt in body.stmts.iter() {
                        self.visit_stmt(stmt);
                    }
                }
            }
            ItemKind::Variable(variable) => self.visit_variable_definition(variable),
            ItemKind::Struct(struct_) => {
                for field in struct_.fields.iter() {
                    self.visit_variable_definition(field);
                }
            }
            ItemKind::Udvt(udvt) => self.visit_type(&udvt.ty),
            ItemKind::Error(error) => {
                for parameter in error.parameters.iter() {
                    self.visit_variable_definition(parameter);
                }
            }
            ItemKind::Event(event) => {
                for parameter in event.parameters.iter() {
                    self.visit_variable_definition(parameter);
                }
            }
            ItemKind::Using(using_directive) => {
                match &using_directive.list {
                    solar_ast::UsingList::Single(path) => {
                        let path_name = path.to_string();
                        let path_span = solgrid_ast::span_to_range(path.span());
                        if !self.resolve_ast_path(path, path_span.start) {
                            self.push(
                                "compiler/unresolved-using-symbol",
                                "Unresolved using symbol",
                                format!("cannot resolve using symbol \"{path_name}\""),
                                path_span,
                            );
                        }
                    }
                    solar_ast::UsingList::Multiple(paths) => {
                        for (path, _) in paths.iter() {
                            let path_name = path.to_string();
                            let path_span = solgrid_ast::span_to_range(path.span());
                            if !self.resolve_ast_path(path, path_span.start) {
                                self.push(
                                    "compiler/unresolved-using-symbol",
                                    "Unresolved using symbol",
                                    format!("cannot resolve using symbol \"{path_name}\""),
                                    path_span,
                                );
                            }
                        }
                    }
                }

                if let Some(ty) = &using_directive.ty {
                    self.visit_type(ty);
                }
            }
            ItemKind::Pragma(_) | ItemKind::Import(_) | ItemKind::Enum(_) => {}
        }
    }

    fn visit_variable_definition(&mut self, variable: &solar_ast::VariableDefinition<'_>) {
        self.visit_type(&variable.ty);
        if let Some(override_) = &variable.override_ {
            for path in override_.paths.iter() {
                let override_name = path.to_string();
                let override_span = solgrid_ast::span_to_range(path.span());
                if !self.resolve_ast_path(path, override_span.start) {
                    self.push(
                        "compiler/unresolved-override",
                        "Unresolved override target",
                        format!("cannot resolve override target \"{override_name}\""),
                        override_span,
                    );
                }
            }
        }
        if let Some(initializer) = &variable.initializer {
            self.visit_expr(initializer);
        }
    }

    fn visit_type(&mut self, ty: &Type<'_>) {
        match &ty.kind {
            solar_ast::TypeKind::Elementary(_) => {}
            solar_ast::TypeKind::Custom(path) => {
                let type_name = path.to_string();
                let type_span = solgrid_ast::span_to_range(ty.span);
                if !self.resolve_ast_path(path, type_span.start) {
                    self.push(
                        "compiler/unresolved-type",
                        "Unresolved type",
                        format!("cannot resolve type \"{type_name}\""),
                        type_span,
                    );
                }
            }
            solar_ast::TypeKind::Array(array) => {
                self.visit_type(&array.element);
                if let Some(size) = &array.size {
                    self.visit_expr(size);
                }
            }
            solar_ast::TypeKind::Function(function_ty) => {
                for parameter in function_ty.parameters.iter() {
                    self.visit_variable_definition(parameter);
                }
                for return_param in function_ty.returns() {
                    self.visit_variable_definition(return_param);
                }
            }
            solar_ast::TypeKind::Mapping(mapping) => {
                self.visit_type(&mapping.key);
                self.visit_type(&mapping.value);
            }
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt<'_>) {
        match &stmt.kind {
            StmtKind::Assembly(_)
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Placeholder => {}
            StmtKind::DeclSingle(variable) => self.visit_variable_definition(variable),
            StmtKind::DeclMulti(variables, expr) => {
                for variable in variables.iter() {
                    if let SpannedOption::Some(variable) = variable {
                        self.visit_variable_definition(variable);
                    }
                }
                self.visit_expr(expr);
            }
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                for stmt in block.stmts.iter() {
                    self.visit_stmt(stmt);
                }
            }
            StmtKind::DoWhile(body, expr) | StmtKind::While(expr, body) => {
                self.visit_expr(expr);
                self.visit_stmt(body);
            }
            StmtKind::Emit(path, args) => {
                let event_name = path.to_string();
                let event_span = solgrid_ast::span_to_range(path.span());
                if !self.resolve_member_path(path, event_span.start, &[SymbolKind::Event]) {
                    self.push(
                        "compiler/unresolved-event",
                        "Unresolved event",
                        format!("cannot resolve event \"{event_name}\""),
                        event_span,
                    );
                }
                for argument in args.exprs() {
                    self.visit_expr(argument);
                }
            }
            StmtKind::Revert(path, args) => {
                let error_name = path.to_string();
                let error_span = solgrid_ast::span_to_range(path.span());
                if !is_builtin_error_path(path)
                    && !self.resolve_member_path(path, error_span.start, &[SymbolKind::Error])
                {
                    self.push(
                        "compiler/unresolved-error",
                        "Unresolved error",
                        format!("cannot resolve custom error \"{error_name}\""),
                        error_span,
                    );
                }
                for argument in args.exprs() {
                    self.visit_expr(argument);
                }
            }
            StmtKind::Expr(expr) => {
                self.visit_expression_statement(expr);
                self.visit_expr(expr);
            }
            StmtKind::Return(Some(expr)) => self.visit_expr(expr),
            StmtKind::Return(None) => {}
            StmtKind::For {
                init,
                cond,
                next,
                body,
            } => {
                if let Some(init) = init {
                    self.visit_stmt(init);
                }
                if let Some(cond) = cond {
                    self.visit_expr(cond);
                }
                if let Some(next) = next {
                    self.visit_expr(next);
                }
                self.visit_stmt(body);
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                self.visit_expr(cond);
                self.visit_stmt(then_stmt);
                if let Some(else_stmt) = else_stmt {
                    self.visit_stmt(else_stmt);
                }
            }
            StmtKind::Try(try_stmt) => {
                self.visit_expr(try_stmt.expr);
                for clause in try_stmt.clauses.iter() {
                    for stmt in clause.block.stmts.iter() {
                        self.visit_stmt(stmt);
                    }
                }
            }
        }
    }

    fn visit_expression_statement(&mut self, expr: &Expr<'_>) {
        let Some((method, span)) =
            unchecked_low_level_call_site(&self.snapshot.source, &self.snapshot.table, expr)
        else {
            return;
        };

        self.push_detector(
            UNCHECKED_LOW_LEVEL_CALL_ID,
            UNCHECKED_LOW_LEVEL_CALL_TITLE,
            format!("low-level `.{method}()` result is ignored; check the returned success value"),
            span,
            Severity::Warning,
            Confidence::High,
        );
    }

    fn visit_call_expression(&mut self, expr: &Expr<'_>) {
        let Some((target_name, span)) = self.user_controlled_delegatecall_site(expr) else {
            if let Some((target_name, method_label, span)) =
                self.user_controlled_eth_transfer_site(expr)
            {
                self.push_detector(
                    USER_CONTROLLED_ETH_TRANSFER_ID,
                    USER_CONTROLLED_ETH_TRANSFER_TITLE,
                    format!(
                        "ETH transfer via `{method_label}` targets `{target_name}`, which resolves to a function parameter; ensure the recipient is trusted or validated"
                    ),
                    span,
                    Severity::Warning,
                    Confidence::High,
                );
            }
            self.visit_interprocedural_sink_calls(expr);
            return;
        };

        self.push_detector(
            USER_CONTROLLED_DELEGATECALL_ID,
            USER_CONTROLLED_DELEGATECALL_TITLE,
            format!(
                "delegatecall target `{target_name}` resolves to a function parameter; avoid delegatecalling user-controlled addresses"
            ),
            span,
            Severity::Error,
            Confidence::High,
        );

        if let Some((target_name, method_label, span)) =
            self.user_controlled_eth_transfer_site(expr)
        {
            self.push_detector(
                USER_CONTROLLED_ETH_TRANSFER_ID,
                USER_CONTROLLED_ETH_TRANSFER_TITLE,
                format!(
                    "ETH transfer via `{method_label}` targets `{target_name}`, which resolves to a function parameter; ensure the recipient is trusted or validated"
                ),
                span,
                Severity::Warning,
                Confidence::High,
            );
        }

        self.visit_interprocedural_sink_calls(expr);
    }

    fn visit_expr(&mut self, expr: &Expr<'_>) {
        match &expr.kind {
            ExprKind::Array(exprs) => {
                for expr in exprs.iter() {
                    self.visit_expr(expr);
                }
            }
            ExprKind::Assign(lhs, _, rhs) | ExprKind::Binary(lhs, _, rhs) => {
                self.visit_expr(lhs);
                self.visit_expr(rhs);
            }
            ExprKind::Call(lhs, args) => {
                self.visit_call_expression(expr);
                self.visit_expr(lhs);
                for argument in args.exprs() {
                    self.visit_expr(argument);
                }
            }
            ExprKind::CallOptions(lhs, args) => {
                self.visit_expr(lhs);
                for argument in args.iter() {
                    self.visit_expr(argument.value);
                }
            }
            ExprKind::Delete(expr) | ExprKind::Unary(_, expr) => self.visit_expr(expr),
            ExprKind::Index(lhs, kind) => {
                self.visit_expr(lhs);
                match kind {
                    IndexKind::Index(Some(expr)) => self.visit_expr(expr),
                    IndexKind::Range(start, end) => {
                        if let Some(start) = start {
                            self.visit_expr(start);
                        }
                        if let Some(end) = end {
                            self.visit_expr(end);
                        }
                    }
                    IndexKind::Index(None) => {}
                }
            }
            ExprKind::Member(expr, _) => self.visit_expr(expr),
            ExprKind::New(ty) => self.visit_type(ty),
            ExprKind::Payable(args) => {
                for argument in args.exprs() {
                    self.visit_expr(argument);
                }
            }
            ExprKind::Ternary(cond, if_true, if_false) => {
                self.visit_expr(cond);
                self.visit_expr(if_true);
                self.visit_expr(if_false);
            }
            ExprKind::Tuple(exprs) => {
                for expr in exprs.iter() {
                    if let SpannedOption::Some(expr) = expr {
                        self.visit_expr(expr);
                    }
                }
            }
            ExprKind::Ident(_)
            | ExprKind::Lit(_, _)
            | ExprKind::Type(_)
            | ExprKind::TypeCall(_) => {}
        }
    }

    fn resolve_ast_path(&self, path: &solar_ast::AstPath<'_>, resolve_offset: usize) -> bool {
        let path = ast_path_to_type_path(path);
        self.resolve_path(&path, resolve_offset)
    }

    fn resolve_modifier_path(&self, path: &solar_ast::AstPath<'_>, resolve_offset: usize) -> bool {
        self.resolve_member_path(path, resolve_offset, &[SymbolKind::Modifier])
    }

    fn resolve_member_path(
        &self,
        path: &solar_ast::AstPath<'_>,
        resolve_offset: usize,
        accepted_kinds: &[SymbolKind],
    ) -> bool {
        let path = ast_path_to_type_path(path);
        self.resolve_path_with_kinds(&path, resolve_offset, accepted_kinds)
            || self.resolve_inherited_member_path(&path, accepted_kinds)
    }

    fn resolve_path_with_kinds(
        &self,
        path: &TypePath,
        resolve_offset: usize,
        accepted_kinds: &[SymbolKind],
    ) -> bool {
        if path.segments.is_empty() {
            return false;
        }

        if self.resolve_namespace_path_with_kinds(path, accepted_kinds) {
            return true;
        }

        if self
            .snapshot
            .table
            .resolve_all(&path.segments[0], resolve_offset)
            .into_iter()
            .any(|def| {
                self.resolve_member_chain_with_kinds(
                    &self.snapshot.table,
                    def,
                    &path.segments[1..],
                    accepted_kinds,
                )
            })
        {
            return true;
        }

        resolve_cross_file_symbol(
            &self.snapshot.table,
            &path.segments[0],
            &self.snapshot.path,
            self.get_source,
            self.resolver,
        )
        .is_some_and(|cross_file| {
            self.resolve_member_chain_with_kinds(
                &cross_file.table,
                &cross_file.def,
                &path.segments[1..],
                accepted_kinds,
            )
        })
    }

    fn resolve_inherited_member_path(
        &self,
        path: &TypePath,
        accepted_kinds: &[SymbolKind],
    ) -> bool {
        let [member_name] = path.segments.as_slice() else {
            return false;
        };
        let Some(contract_name) = self.current_contracts.last() else {
            return false;
        };
        let Some(file) = self.semantic_files.get(&self.snapshot.path) else {
            return false;
        };
        let context = SinkSummaryContext {
            semantic_files: &self.semantic_files,
            resolver: self.resolver,
            get_source: self.get_source,
        };
        let mut visited = HashSet::new();
        self.contract_hierarchy_declares_member(
            file,
            contract_name,
            member_name,
            accepted_kinds,
            &context,
            &mut visited,
        )
    }

    fn contract_hierarchy_declares_member(
        &self,
        file: &FileSemanticInfo,
        contract_name: &str,
        member_name: &str,
        accepted_kinds: &[SymbolKind],
        context: &SinkSummaryContext<'_>,
        visited: &mut HashSet<(PathBuf, String)>,
    ) -> bool {
        if !visited.insert((file.path.clone(), contract_name.to_string())) {
            return false;
        }

        if let Some(contract_def) = file.table.resolve(contract_name, 0) {
            if file
                .table
                .resolve_member_all(contract_def, member_name)
                .into_iter()
                .any(|def| accepted_kinds.contains(&def.kind))
            {
                return true;
            }
        }

        let Some(contract) = file.contracts.get(contract_name) else {
            return false;
        };
        for base in &contract.bases {
            let Some((base_path, base_name)) = resolve_contract_path_target(file, base, context)
            else {
                continue;
            };
            let Some(base_file) = self.semantic_files.get(&base_path) else {
                continue;
            };
            if self.contract_hierarchy_declares_member(
                base_file,
                &base_name,
                member_name,
                accepted_kinds,
                context,
                visited,
            ) {
                return true;
            }
        }

        false
    }

    fn resolve_path(&self, path: &TypePath, resolve_offset: usize) -> bool {
        if path.segments.is_empty() {
            return false;
        }

        if self.resolve_namespace_path(path) {
            return true;
        }

        if self
            .snapshot
            .table
            .resolve_all(&path.segments[0], resolve_offset)
            .into_iter()
            .any(|def| self.resolve_member_chain(&self.snapshot.table, def, &path.segments[1..]))
        {
            return true;
        }

        if let Some(cross_file) = resolve_cross_file_symbol(
            &self.snapshot.table,
            &path.segments[0],
            &self.snapshot.path,
            self.get_source,
            self.resolver,
        ) {
            return self.resolve_member_chain(
                &cross_file.table,
                &cross_file.def,
                &path.segments[1..],
            );
        }

        false
    }

    fn resolve_namespace_path(&self, path: &TypePath) -> bool {
        if path.segments.len() < 2 {
            return false;
        }

        let namespace = &path.segments[0];
        for import in &self.snapshot.table.imports {
            let matches_namespace = match &import.symbols {
                ImportedSymbols::Plain(Some(alias)) | ImportedSymbols::Glob(alias) => {
                    alias == namespace
                }
                ImportedSymbols::Plain(None) | ImportedSymbols::Named(_) => false,
            };
            if !matches_namespace {
                continue;
            }

            let Some(resolved) = self.resolver.resolve(&import.path, &self.snapshot.path) else {
                continue;
            };
            let Some(imported_source) = (self.get_source)(&resolved) else {
                continue;
            };
            let filename = resolved.to_string_lossy().to_string();
            let Some(imported_table) = symbols::build_symbol_table(&imported_source, &filename)
            else {
                continue;
            };
            let Some(root) = imported_table.resolve(&path.segments[1], 0) else {
                continue;
            };
            if self.resolve_member_chain(&imported_table, root, &path.segments[2..]) {
                return true;
            }
        }

        false
    }

    fn resolve_namespace_path_with_kinds(
        &self,
        path: &TypePath,
        accepted_kinds: &[SymbolKind],
    ) -> bool {
        if path.segments.len() < 2 {
            return false;
        }

        let namespace = &path.segments[0];
        for import in &self.snapshot.table.imports {
            let matches_namespace = match &import.symbols {
                ImportedSymbols::Plain(Some(alias)) | ImportedSymbols::Glob(alias) => {
                    alias == namespace
                }
                ImportedSymbols::Plain(None) | ImportedSymbols::Named(_) => false,
            };
            if !matches_namespace {
                continue;
            }

            let Some(resolved) = self.resolver.resolve(&import.path, &self.snapshot.path) else {
                continue;
            };
            let Some(imported_source) = (self.get_source)(&resolved) else {
                continue;
            };
            let filename = resolved.to_string_lossy().to_string();
            let Some(imported_table) = symbols::build_symbol_table(&imported_source, &filename)
            else {
                continue;
            };
            let Some(root) = imported_table.resolve(&path.segments[1], 0) else {
                continue;
            };
            if self.resolve_member_chain_with_kinds(
                &imported_table,
                root,
                &path.segments[2..],
                accepted_kinds,
            ) {
                return true;
            }
        }

        false
    }

    fn resolve_member_chain(
        &self,
        table: &SymbolTable,
        root: &SymbolDef,
        remaining: &[String],
    ) -> bool {
        let mut current = root;
        for segment in remaining {
            let Some(next) = table.resolve_member(current, segment) else {
                return false;
            };
            current = next;
        }
        true
    }

    fn resolve_member_chain_with_kinds(
        &self,
        table: &SymbolTable,
        root: &SymbolDef,
        remaining: &[String],
        accepted_kinds: &[SymbolKind],
    ) -> bool {
        let mut current = root;
        for segment in remaining {
            let Some(next) = table.resolve_member(current, segment) else {
                return false;
            };
            current = next;
        }
        accepted_kinds.contains(&current.kind)
    }

    fn visit_interprocedural_sink_calls(&mut self, expr: &Expr<'_>) {
        let Some(current_file) = self.semantic_files.get(&self.snapshot.path) else {
            return;
        };
        let Some((call_span, call_name, propagated)) = propagated_sink_summary(
            current_file,
            expr,
            &self.function_summaries,
            &self.semantic_files,
            self.resolver,
            self.get_source,
        ) else {
            return;
        };

        for (parameter_name, sink_kind) in propagated {
            match sink_kind {
                SinkKind::Delegatecall => self.push_detector(
                    USER_CONTROLLED_DELEGATECALL_ID,
                    USER_CONTROLLED_DELEGATECALL_TITLE,
                    format!(
                        "argument `{parameter_name}` flows into delegatecall via `{call_name}`; avoid delegatecalling user-controlled addresses"
                    ),
                    call_span.clone(),
                    Severity::Error,
                    Confidence::Medium,
                ),
                SinkKind::EthTransfer => self.push_detector(
                    USER_CONTROLLED_ETH_TRANSFER_ID,
                    USER_CONTROLLED_ETH_TRANSFER_TITLE,
                    format!(
                        "argument `{parameter_name}` flows into an ETH transfer via `{call_name}`; ensure the recipient is trusted or validated"
                    ),
                    call_span.clone(),
                    Severity::Warning,
                    Confidence::Medium,
                ),
            }
        }
    }

    fn user_controlled_delegatecall_site(
        &self,
        expr: &Expr<'_>,
    ) -> Option<(String, std::ops::Range<usize>)> {
        let ExprKind::Call(callee, _) = &expr.kind else {
            return None;
        };
        let callee = match &callee.kind {
            ExprKind::CallOptions(inner, _) => inner,
            _ => callee,
        };
        let ExprKind::Member(base, member) = &callee.kind else {
            return None;
        };
        if member.as_str() != "delegatecall" {
            return None;
        }
        if !is_low_level_address_receiver(&self.snapshot.source, &self.snapshot.table, base) {
            return None;
        }

        let (target_name, span) = delegatecall_target_identifier(base)?;
        let resolve_offset = span.start;
        let resolved = self.snapshot.table.resolve(&target_name, resolve_offset)?;
        if resolved.kind != SymbolKind::Parameter {
            return None;
        }

        Some((target_name, span))
    }

    fn user_controlled_eth_transfer_site(
        &self,
        expr: &Expr<'_>,
    ) -> Option<(String, &'static str, std::ops::Range<usize>)> {
        let ExprKind::Call(callee, args) = &expr.kind else {
            return None;
        };

        let (base, member, has_value_option) = match &callee.kind {
            ExprKind::Member(base, member) => (base, member, false),
            ExprKind::CallOptions(inner, options) => {
                let ExprKind::Member(base, member) = &inner.kind else {
                    return None;
                };
                (
                    base,
                    member,
                    call_options_contain_nonzero_named_arg(options, "value"),
                )
            }
            _ => return None,
        };

        let method_label = match member.as_str() {
            "send" if args.len() == 1 => ".send()",
            "transfer" if args.len() == 1 => ".transfer()",
            "call" if has_value_option => ".call{value: ...}()",
            _ => return None,
        };
        if !is_low_level_address_receiver(&self.snapshot.source, &self.snapshot.table, base) {
            return None;
        }

        let (target_name, resolve_span) = delegatecall_target_identifier(base)?;
        let resolved = self
            .snapshot
            .table
            .resolve(&target_name, resolve_span.start)?;
        if resolved.kind != SymbolKind::Parameter {
            return None;
        }

        Some((
            target_name,
            method_label,
            solgrid_ast::span_to_range(member.span),
        ))
    }
}

fn unchecked_low_level_call_site(
    source: &str,
    table: &SymbolTable,
    expr: &Expr<'_>,
) -> Option<(&'static str, std::ops::Range<usize>)> {
    let ExprKind::Call(callee, _) = &expr.kind else {
        return None;
    };
    let callee = match &callee.kind {
        ExprKind::CallOptions(inner, _) => inner,
        _ => callee,
    };
    let ExprKind::Member(base, member) = &callee.kind else {
        return None;
    };

    let method = match member.as_str() {
        "call" => "call",
        "delegatecall" => "delegatecall",
        "staticcall" => "staticcall",
        _ => return None,
    };
    if !is_low_level_address_receiver(source, table, base) {
        return None;
    }

    Some((method, solgrid_ast::span_to_range(member.span)))
}

fn delegatecall_target_identifier(expr: &Expr<'_>) -> Option<(String, std::ops::Range<usize>)> {
    match &expr.peel_parens().kind {
        ExprKind::Ident(ident) => Some((
            ident.as_str().to_string(),
            solgrid_ast::span_to_range(ident.span),
        )),
        ExprKind::Payable(args) => args.exprs().next().and_then(delegatecall_target_identifier),
        ExprKind::Call(callee, args)
            if matches!(callee.kind, ExprKind::Type(_)) && args.len() == 1 =>
        {
            args.exprs().next().and_then(delegatecall_target_identifier)
        }
        _ => None,
    }
}

fn call_options_contain_nonzero_named_arg(
    options: &solar_ast::NamedArgList<'_>,
    name: &str,
) -> bool {
    options
        .iter()
        .find(|arg| arg.name.as_str() == name)
        .is_some_and(|arg| !is_literal_zero(arg.value))
}

fn is_literal_zero(expr: &Expr<'_>) -> bool {
    matches!(
        &expr.peel_parens().kind,
        ExprKind::Lit(literal, _)
            if matches!(literal.kind, solar_ast::LitKind::Number(value) if value.is_zero())
    )
}

fn is_low_level_address_receiver(source: &str, table: &SymbolTable, expr: &Expr<'_>) -> bool {
    let types = infer_local_value_types(source, table, expr);
    !types.is_empty() && types.iter().all(is_address_type)
}

fn infer_local_value_types(source: &str, table: &SymbolTable, expr: &Expr<'_>) -> Vec<TypeSpec> {
    let expr = expr.peel_parens();
    let offset = solgrid_ast::span_to_range(expr.span).start;
    let mut types = match &expr.kind {
        ExprKind::Ident(ident) => table
            .resolve_all(ident.as_str(), offset)
            .into_iter()
            .filter_map(|def| def.type_info.clone())
            .collect(),
        ExprKind::Member(base, member) => {
            if matches!(
                (&base.peel_parens().kind, member.as_str()),
                (ExprKind::Ident(namespace), "sender") if namespace.as_str() == "msg"
            ) || matches!(
                (&base.peel_parens().kind, member.as_str()),
                (ExprKind::Ident(namespace), "origin") if namespace.as_str() == "tx"
            ) || matches!(
                (&base.peel_parens().kind, member.as_str()),
                (ExprKind::Ident(namespace), "coinbase") if namespace.as_str() == "block"
            ) {
                vec![TypeSpec::Elementary {
                    display: "address".to_string(),
                }]
            } else {
                infer_local_value_types(source, table, base)
                    .into_iter()
                    .filter_map(|base_type| {
                        let path = base_type.member_target()?;
                        let container =
                            table.resolve(path.segments.last()?, base_type.resolve_offset())?;
                        table
                            .resolve_member(container, member.as_str())?
                            .type_info
                            .clone()
                    })
                    .collect()
            }
        }
        ExprKind::Index(base, IndexKind::Index(_) | IndexKind::Range(_, _)) => {
            infer_local_value_types(source, table, base)
                .into_iter()
                .filter_map(|ty| ty.index_result().cloned())
                .collect()
        }
        ExprKind::Call(callee, args) => match &callee.peel_parens().kind {
            ExprKind::Type(ty) if args.len() == 1 => {
                vec![symbols::type_spec_from_ast(source, ty, None, offset)]
            }
            ExprKind::Ident(ident) => table
                .resolve_all(ident.as_str(), offset)
                .into_iter()
                .filter_map(|def| def.signature.as_ref()?.first_return_type.clone())
                .collect(),
            ExprKind::Member(base, member) => infer_local_value_types(source, table, base)
                .into_iter()
                .filter_map(|base_type| {
                    let path = base_type.member_target()?;
                    let container =
                        table.resolve(path.segments.last()?, base_type.resolve_offset())?;
                    table
                        .resolve_member_all(container, member.as_str())
                        .iter()
                        .find_map(|def| def.signature.as_ref()?.first_return_type.clone())
                })
                .collect(),
            _ => Vec::new(),
        },
        // `payable(value)` always yields an address-payable receiver.
        ExprKind::Payable(args) if args.len() == 1 => vec![TypeSpec::Elementary {
            display: "address payable".to_string(),
        }],
        ExprKind::Ternary(_, if_true, if_false) => {
            let mut types = infer_local_value_types(source, table, if_true);
            types.extend(infer_local_value_types(source, table, if_false));
            types
        }
        _ => Vec::new(),
    };
    types.sort_by(|left, right| left.display().cmp(right.display()));
    types.dedup();
    types
}

fn is_address_type(ty: &TypeSpec) -> bool {
    matches!(
        ty,
        TypeSpec::Elementary { display }
            if matches!(display.trim(), "address" | "address payable")
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum SinkKind {
    Delegatecall,
    EthTransfer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CallableTargetKey {
    path: PathBuf,
    offset: usize,
}

#[derive(Debug, Clone)]
struct CallableSignatureRef {
    target: CallableTargetKey,
    arg_count: usize,
}

#[derive(Debug, Clone, Default)]
struct ContractSemanticInfo {
    bases: Vec<TypePath>,
    callables: HashMap<String, Vec<CallableSignatureRef>>,
}

#[derive(Debug, Clone)]
struct FileSemanticInfo {
    path: PathBuf,
    source: String,
    table: SymbolTable,
    contracts: HashMap<String, ContractSemanticInfo>,
    callable_contracts: HashMap<usize, Option<String>>,
    callable_signatures: HashMap<usize, SignatureData>,
    callable_parameters: HashMap<usize, Vec<CallableParameter>>,
}

#[derive(Debug, Clone)]
struct CallableParameter {
    name: Option<String>,
    ty: TypeSpec,
}

#[derive(Debug, Clone)]
struct FunctionCallEdge {
    callees: Vec<CallableTargetKey>,
    argument_parameters: ArgumentParameterBindings,
}

#[derive(Debug, Clone)]
enum ArgumentParameterBindings {
    Positional(Vec<Option<String>>),
    Named(HashMap<String, Option<String>>),
}

#[derive(Debug, Clone, Default)]
struct FunctionSinkSummary {
    parameter_names: Vec<Option<String>>,
    delegatecall_parameters: HashSet<usize>,
    eth_transfer_parameters: HashSet<usize>,
    call_edges: Vec<FunctionCallEdge>,
}

#[derive(Debug, Clone)]
struct ResolvedExprDef {
    path: PathBuf,
    def: SymbolDef,
}

#[derive(Debug, Clone)]
struct ResolvedExprType {
    path: PathBuf,
    ty: TypeSpec,
}

#[derive(Clone, Copy)]
struct CrossFileMemberQuery<'a> {
    container_name: &'a str,
    member_name: &'a str,
}

type PropagatedSinkFinding = (std::ops::Range<usize>, String, Vec<(String, SinkKind)>);

struct SinkSummaryContext<'a> {
    semantic_files: &'a HashMap<PathBuf, FileSemanticInfo>,
    resolver: &'a SharedImportResolver,
    get_source: &'a dyn Fn(&Path) -> Option<String>,
}

fn build_function_sink_summaries(
    snapshot: &ProjectSnapshot,
    resolver: &SharedImportResolver,
    get_source: &dyn Fn(&Path) -> Option<String>,
) -> (
    HashMap<PathBuf, FileSemanticInfo>,
    HashMap<CallableTargetKey, FunctionSinkSummary>,
) {
    let mut semantic_files = HashMap::new();
    if !load_semantic_file(
        &snapshot.path,
        Some(&snapshot.source),
        resolver,
        get_source,
        &mut semantic_files,
    ) {
        return (HashMap::new(), HashMap::new());
    }

    let mut summaries = HashMap::<CallableTargetKey, FunctionSinkSummary>::new();
    let mut file_paths = semantic_files.keys().cloned().collect::<Vec<_>>();
    file_paths.sort();
    for path in file_paths {
        let Some(file) = semantic_files.get(&path) else {
            continue;
        };
        summarize_semantic_file_functions(
            file,
            &semantic_files,
            resolver,
            get_source,
            &mut summaries,
        );
    }

    loop {
        let previous = summaries.clone();
        let mut changed = false;

        for summary in summaries.values_mut() {
            for edge in &summary.call_edges {
                changed |= propagate_sink_indices_through_edge(
                    &mut summary.delegatecall_parameters,
                    SinkKind::Delegatecall,
                    edge,
                    &previous,
                    &summary.parameter_names,
                );
                changed |= propagate_sink_indices_through_edge(
                    &mut summary.eth_transfer_parameters,
                    SinkKind::EthTransfer,
                    edge,
                    &previous,
                    &summary.parameter_names,
                );
            }
        }

        if !changed {
            break;
        }
    }

    (semantic_files, summaries)
}

fn load_semantic_file(
    path: &Path,
    source_override: Option<&str>,
    resolver: &SharedImportResolver,
    get_source: &dyn Fn(&Path) -> Option<String>,
    semantic_files: &mut HashMap<PathBuf, FileSemanticInfo>,
) -> bool {
    let path = path.to_path_buf();
    if semantic_files.contains_key(&path) {
        return true;
    }

    let Some(source) = source_override
        .map(str::to_string)
        .or_else(|| get_source(&path))
    else {
        return false;
    };

    let filename = path.to_string_lossy().to_string();
    let Some(table) = symbols::build_symbol_table(&source, &filename) else {
        return false;
    };

    let Ok((contracts, callable_contracts, callable_signatures, callable_parameters)) =
        with_parsed_ast_sequential(&source, &filename, |source_unit| {
            let mut contracts = HashMap::<String, ContractSemanticInfo>::new();
            let mut callable_contracts = HashMap::<usize, Option<String>>::new();
            let mut callable_signatures = HashMap::<usize, SignatureData>::new();
            let mut callable_parameters = HashMap::<usize, Vec<CallableParameter>>::new();

            for item in source_unit.items.iter() {
                match &item.kind {
                    ItemKind::Contract(contract) => {
                        let bases = contract
                            .bases
                            .iter()
                            .map(|base| TypePath {
                                segments: base
                                    .name
                                    .segments()
                                    .iter()
                                    .map(|segment| segment.as_str().to_string())
                                    .collect(),
                            })
                            .collect::<Vec<_>>();
                        let contract_info = contracts
                            .entry(contract.name.as_str().to_string())
                            .or_default();
                        contract_info.bases = bases;
                        for body_item in contract.body.iter() {
                            let ItemKind::Function(function) = &body_item.kind else {
                                continue;
                            };
                            if !function.is_implemented() {
                                continue;
                            }
                            let Some(target_offset) = function_target_offset(function) else {
                                continue;
                            };
                            let Some(name) =
                                function.header.name.map(|ident| ident.as_str().to_string())
                            else {
                                continue;
                            };
                            if let Some(signature) =
                                lookup_callable_signature(&table, &name, target_offset)
                            {
                                callable_signatures.insert(target_offset, signature);
                            }
                            callable_parameters.insert(
                                target_offset,
                                callable_parameters_for_function(&source, function, target_offset),
                            );
                            let target = CallableTargetKey {
                                path: path.clone(),
                                offset: target_offset,
                            };
                            record_callable_signature(
                                &mut contract_info.callables,
                                &name,
                                target,
                                function.header.parameters.len(),
                            );
                            callable_contracts
                                .insert(target_offset, Some(contract.name.as_str().to_string()));
                        }
                    }
                    ItemKind::Function(function) => {
                        if !function.is_implemented() {
                            continue;
                        }
                        let Some(target_offset) = function_target_offset(function) else {
                            continue;
                        };
                        let Some(name) =
                            function.header.name.map(|ident| ident.as_str().to_string())
                        else {
                            continue;
                        };
                        if let Some(signature) =
                            lookup_callable_signature(&table, &name, target_offset)
                        {
                            callable_signatures.insert(target_offset, signature);
                        }
                        callable_parameters.insert(
                            target_offset,
                            callable_parameters_for_function(&source, function, target_offset),
                        );
                        callable_contracts.insert(target_offset, None);
                    }
                    _ => {}
                }
            }

            (
                contracts,
                callable_contracts,
                callable_signatures,
                callable_parameters,
            )
        })
    else {
        return false;
    };

    let import_paths = table
        .imports
        .iter()
        .filter_map(|import| resolver.resolve(&import.path, &path))
        .collect::<Vec<_>>();

    semantic_files.insert(
        path.clone(),
        FileSemanticInfo {
            path: path.clone(),
            source,
            table,
            contracts,
            callable_contracts,
            callable_signatures,
            callable_parameters,
        },
    );

    for import_path in import_paths {
        let _ = load_semantic_file(&import_path, None, resolver, get_source, semantic_files);
    }

    true
}

fn callable_parameters_for_function(
    source: &str,
    function: &solar_ast::ItemFunction<'_>,
    resolve_offset: usize,
) -> Vec<CallableParameter> {
    function
        .header
        .parameters
        .iter()
        .map(|parameter| CallableParameter {
            name: parameter.name.map(|name| name.as_str().to_string()),
            ty: symbols::type_spec_from_ast(
                source,
                &parameter.ty,
                parameter.data_location,
                resolve_offset,
            ),
        })
        .collect()
}

fn record_callable_signature(
    map: &mut HashMap<String, Vec<CallableSignatureRef>>,
    name: &str,
    target: CallableTargetKey,
    arg_count: usize,
) {
    map.entry(name.to_string())
        .or_default()
        .push(CallableSignatureRef { target, arg_count });
}

fn lookup_callable_signature(
    table: &SymbolTable,
    name: &str,
    target_offset: usize,
) -> Option<SignatureData> {
    table
        .resolve_all(name, target_offset)
        .into_iter()
        .find(|def| def.name_span.start == target_offset)
        .and_then(|def| def.signature.clone())
}

fn summarize_semantic_file_functions(
    file: &FileSemanticInfo,
    semantic_files: &HashMap<PathBuf, FileSemanticInfo>,
    resolver: &SharedImportResolver,
    get_source: &dyn Fn(&Path) -> Option<String>,
    summaries: &mut HashMap<CallableTargetKey, FunctionSinkSummary>,
) {
    let context = SinkSummaryContext {
        semantic_files,
        resolver,
        get_source,
    };
    let filename = file.path.to_string_lossy().to_string();
    let _ = with_parsed_ast_sequential(&file.source, &filename, |source_unit| {
        fn visit_item(
            file: &FileSemanticInfo,
            current_contract: Option<&str>,
            item: &solar_ast::Item<'_>,
            context: &SinkSummaryContext<'_>,
            summaries: &mut HashMap<CallableTargetKey, FunctionSinkSummary>,
        ) {
            match &item.kind {
                ItemKind::Contract(contract) => {
                    let contract_name = contract.name.as_str().to_string();
                    for body_item in contract.body.iter() {
                        visit_item(
                            file,
                            Some(contract_name.as_str()),
                            body_item,
                            context,
                            summaries,
                        );
                    }
                }
                ItemKind::Function(function) => {
                    if !function.is_implemented() {
                        return;
                    }
                    let Some(target_offset) = function_target_offset(function) else {
                        return;
                    };
                    summaries.insert(
                        CallableTargetKey {
                            path: file.path.clone(),
                            offset: target_offset,
                        },
                        summarize_function_sinks(file, current_contract, function, context),
                    );
                }
                _ => {}
            }
        }

        for item in source_unit.items.iter() {
            visit_item(file, None, item, &context, summaries);
        }
    });
}

fn summarize_function_sinks(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    function: &solar_ast::ItemFunction<'_>,
    context: &SinkSummaryContext<'_>,
) -> FunctionSinkSummary {
    let parameter_names = function
        .header
        .parameters
        .iter()
        .map(|parameter| parameter.name.map(|name| name.as_str().to_string()))
        .collect::<Vec<_>>();
    let mut summary = FunctionSinkSummary {
        parameter_names: parameter_names.clone(),
        ..FunctionSinkSummary::default()
    };
    let parameter_indexes = parameter_names
        .iter()
        .enumerate()
        .filter_map(|(index, name)| name.clone().map(|name| (name, index)))
        .collect::<HashMap<_, _>>();

    if let Some(body) = &function.body {
        collect_function_sink_stmts(
            file,
            current_contract,
            body.stmts,
            &parameter_indexes,
            context,
            &mut summary,
        );
    }

    summary
}

fn collect_function_sink_stmts(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    stmts: &[Stmt<'_>],
    parameter_indexes: &HashMap<String, usize>,
    context: &SinkSummaryContext<'_>,
    summary: &mut FunctionSinkSummary,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assembly(_)
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Placeholder
            | StmtKind::Return(None) => {}
            StmtKind::DeclSingle(variable) => {
                if let Some(initializer) = &variable.initializer {
                    collect_function_sink_expr(
                        file,
                        current_contract,
                        initializer,
                        parameter_indexes,
                        context,
                        summary,
                    );
                }
            }
            StmtKind::DeclMulti(_, expr) | StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => {
                collect_function_sink_expr(
                    file,
                    current_contract,
                    expr,
                    parameter_indexes,
                    context,
                    summary,
                );
            }
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                collect_function_sink_stmts(
                    file,
                    current_contract,
                    block.stmts,
                    parameter_indexes,
                    context,
                    summary,
                );
            }
            StmtKind::DoWhile(body, expr) | StmtKind::While(expr, body) => {
                collect_function_sink_expr(
                    file,
                    current_contract,
                    expr,
                    parameter_indexes,
                    context,
                    summary,
                );
                collect_function_sink_stmts(
                    file,
                    current_contract,
                    std::slice::from_ref(&**body),
                    parameter_indexes,
                    context,
                    summary,
                );
            }
            StmtKind::Emit(_, args) | StmtKind::Revert(_, args) => {
                for argument in args.exprs() {
                    collect_function_sink_expr(
                        file,
                        current_contract,
                        argument,
                        parameter_indexes,
                        context,
                        summary,
                    );
                }
            }
            StmtKind::For {
                init,
                cond,
                next,
                body,
            } => {
                if let Some(init) = init {
                    collect_function_sink_stmts(
                        file,
                        current_contract,
                        std::slice::from_ref(&**init),
                        parameter_indexes,
                        context,
                        summary,
                    );
                }
                if let Some(cond) = cond {
                    collect_function_sink_expr(
                        file,
                        current_contract,
                        cond,
                        parameter_indexes,
                        context,
                        summary,
                    );
                }
                if let Some(next) = next {
                    collect_function_sink_expr(
                        file,
                        current_contract,
                        next,
                        parameter_indexes,
                        context,
                        summary,
                    );
                }
                collect_function_sink_stmts(
                    file,
                    current_contract,
                    std::slice::from_ref(&**body),
                    parameter_indexes,
                    context,
                    summary,
                );
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                collect_function_sink_expr(
                    file,
                    current_contract,
                    cond,
                    parameter_indexes,
                    context,
                    summary,
                );
                collect_function_sink_stmts(
                    file,
                    current_contract,
                    std::slice::from_ref(&**then_stmt),
                    parameter_indexes,
                    context,
                    summary,
                );
                if let Some(else_stmt) = else_stmt {
                    collect_function_sink_stmts(
                        file,
                        current_contract,
                        std::slice::from_ref(&**else_stmt),
                        parameter_indexes,
                        context,
                        summary,
                    );
                }
            }
            StmtKind::Try(try_stmt) => {
                collect_function_sink_expr(
                    file,
                    current_contract,
                    try_stmt.expr,
                    parameter_indexes,
                    context,
                    summary,
                );
                for clause in try_stmt.clauses.iter() {
                    collect_function_sink_stmts(
                        file,
                        current_contract,
                        clause.block.stmts,
                        parameter_indexes,
                        context,
                        summary,
                    );
                }
            }
        }
    }
}

fn collect_function_sink_expr(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    expr: &Expr<'_>,
    parameter_indexes: &HashMap<String, usize>,
    context: &SinkSummaryContext<'_>,
    summary: &mut FunctionSinkSummary,
) {
    let expr = expr.peel_parens();
    match &expr.kind {
        ExprKind::Array(exprs) => {
            for expr in exprs.iter() {
                collect_function_sink_expr(
                    file,
                    current_contract,
                    expr,
                    parameter_indexes,
                    context,
                    summary,
                );
            }
        }
        ExprKind::Assign(lhs, _, rhs) | ExprKind::Binary(lhs, _, rhs) => {
            collect_function_sink_expr(
                file,
                current_contract,
                lhs,
                parameter_indexes,
                context,
                summary,
            );
            collect_function_sink_expr(
                file,
                current_contract,
                rhs,
                parameter_indexes,
                context,
                summary,
            );
        }
        ExprKind::Call(callee, args) => {
            if let Some((parameter_name, _)) =
                delegatecall_target_identifier_from_sink_expr(file, expr)
            {
                if let Some(index) = parameter_indexes.get(&parameter_name) {
                    summary.delegatecall_parameters.insert(*index);
                }
            }
            if let Some((parameter_name, _, _)) =
                eth_transfer_target_identifier_from_sink_expr(file, expr)
            {
                if let Some(index) = parameter_indexes.get(&parameter_name) {
                    summary.eth_transfer_parameters.insert(*index);
                }
            }
            let callees = resolved_callable_targets(file, current_contract, callee, args, context);
            if !callees.is_empty() {
                summary.call_edges.push(FunctionCallEdge {
                    callees,
                    argument_parameters: argument_parameter_bindings(args),
                });
            }
            collect_function_sink_expr(
                file,
                current_contract,
                callee,
                parameter_indexes,
                context,
                summary,
            );
            for argument in args.exprs() {
                collect_function_sink_expr(
                    file,
                    current_contract,
                    argument,
                    parameter_indexes,
                    context,
                    summary,
                );
            }
        }
        ExprKind::CallOptions(callee, options) => {
            collect_function_sink_expr(
                file,
                current_contract,
                callee,
                parameter_indexes,
                context,
                summary,
            );
            for argument in options.iter() {
                collect_function_sink_expr(
                    file,
                    current_contract,
                    argument.value,
                    parameter_indexes,
                    context,
                    summary,
                );
            }
        }
        ExprKind::Delete(expr) | ExprKind::Unary(_, expr) => {
            collect_function_sink_expr(
                file,
                current_contract,
                expr,
                parameter_indexes,
                context,
                summary,
            );
        }
        ExprKind::Index(lhs, kind) => {
            collect_function_sink_expr(
                file,
                current_contract,
                lhs,
                parameter_indexes,
                context,
                summary,
            );
            match kind {
                IndexKind::Index(Some(expr)) => {
                    collect_function_sink_expr(
                        file,
                        current_contract,
                        expr,
                        parameter_indexes,
                        context,
                        summary,
                    );
                }
                IndexKind::Range(start, end) => {
                    if let Some(start) = start {
                        collect_function_sink_expr(
                            file,
                            current_contract,
                            start,
                            parameter_indexes,
                            context,
                            summary,
                        );
                    }
                    if let Some(end) = end {
                        collect_function_sink_expr(
                            file,
                            current_contract,
                            end,
                            parameter_indexes,
                            context,
                            summary,
                        );
                    }
                }
                IndexKind::Index(None) => {}
            }
        }
        ExprKind::Member(expr, _) => {
            collect_function_sink_expr(
                file,
                current_contract,
                expr,
                parameter_indexes,
                context,
                summary,
            );
        }
        ExprKind::Payable(args) => {
            for argument in args.exprs() {
                collect_function_sink_expr(
                    file,
                    current_contract,
                    argument,
                    parameter_indexes,
                    context,
                    summary,
                );
            }
        }
        ExprKind::Ternary(cond, if_true, if_false) => {
            collect_function_sink_expr(
                file,
                current_contract,
                cond,
                parameter_indexes,
                context,
                summary,
            );
            collect_function_sink_expr(
                file,
                current_contract,
                if_true,
                parameter_indexes,
                context,
                summary,
            );
            collect_function_sink_expr(
                file,
                current_contract,
                if_false,
                parameter_indexes,
                context,
                summary,
            );
        }
        ExprKind::Tuple(exprs) => {
            for expr in exprs.iter() {
                if let SpannedOption::Some(expr) = expr {
                    collect_function_sink_expr(
                        file,
                        current_contract,
                        expr,
                        parameter_indexes,
                        context,
                        summary,
                    );
                }
            }
        }
        ExprKind::New(_ty) => {}
        ExprKind::Ident(_) | ExprKind::Lit(_, _) | ExprKind::Type(_) | ExprKind::TypeCall(_) => {}
    }
}

fn is_contract_container_symbol(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
    )
}

fn callable_signature_for_target(
    target: &CallableTargetKey,
    context: &SinkSummaryContext<'_>,
) -> Option<SignatureData> {
    context
        .semantic_files
        .get(&target.path)?
        .callable_signatures
        .get(&target.offset)
        .cloned()
}

fn dedup_contract_targets(targets: Vec<(PathBuf, String)>) -> Vec<(PathBuf, String)> {
    let mut unique = targets;
    unique.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    unique.dedup();
    unique
}

fn dedup_resolved_expr_defs(defs: Vec<ResolvedExprDef>) -> Vec<ResolvedExprDef> {
    let mut unique = defs;
    unique.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.def.name_span.start.cmp(&right.def.name_span.start))
            .then_with(|| left.def.name.cmp(&right.def.name))
    });
    unique.dedup_by(|left, right| {
        left.path == right.path
            && left.def.name_span.start == right.def.name_span.start
            && left.def.name == right.def.name
    });
    unique
}

fn dedup_resolved_expr_types(types: Vec<ResolvedExprType>) -> Vec<ResolvedExprType> {
    let mut unique = types;
    unique.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.ty.display().cmp(right.ty.display()))
    });
    unique.dedup_by(|left, right| left.path == right.path && left.ty == right.ty);
    unique
}

fn resolve_contract_targets_from_type_spec(
    origin_path: &Path,
    type_spec: &TypeSpec,
    context: &SinkSummaryContext<'_>,
) -> Vec<(PathBuf, String)> {
    let Some(type_path) = type_spec.member_target() else {
        return Vec::new();
    };
    let Some(file) = context.semantic_files.get(origin_path) else {
        return Vec::new();
    };
    resolve_contract_path_target(file, type_path, context)
        .into_iter()
        .collect()
}

fn resolve_member_defs_from_expr(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    base: &Expr<'_>,
    member_name: &str,
    context: &SinkSummaryContext<'_>,
) -> Vec<ResolvedExprDef> {
    let base = base.peel_parens();
    let mut defs = Vec::new();

    if let ExprKind::Ident(namespace) = &base.kind {
        let cached_source =
            |candidate: &Path| cached_source(candidate, context.semantic_files, context.get_source);
        if let Some(cross) = resolve_cross_file_member_symbol(
            &file.table,
            namespace.as_str(),
            member_name,
            &file.path,
            &cached_source,
            context.resolver,
        ) {
            defs.push(ResolvedExprDef {
                path: cross.resolved_path,
                def: cross.def,
            });
        }

        if namespace.as_str() == "super" {
            if let Some(contract_name) = current_contract {
                if let Some(contract) = file.contracts.get(contract_name) {
                    for base_contract in &contract.bases {
                        let Some((base_path, base_name)) =
                            resolve_contract_path_target(file, base_contract, context)
                        else {
                            continue;
                        };
                        let Some(base_file) = context.semantic_files.get(&base_path) else {
                            continue;
                        };
                        let Some(contract_def) = base_file.table.resolve(&base_name, 0) else {
                            continue;
                        };
                        for member_def in base_file
                            .table
                            .resolve_member_all(contract_def, member_name)
                        {
                            defs.push(ResolvedExprDef {
                                path: base_path.clone(),
                                def: member_def.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    for (path, contract_name) in
        resolve_contract_targets_from_expr(file, current_contract, base, context)
    {
        let Some(contract_file) = context.semantic_files.get(&path) else {
            continue;
        };
        let Some(contract_def) = contract_file.table.resolve(&contract_name, 0) else {
            continue;
        };
        for member_def in contract_file
            .table
            .resolve_member_all(contract_def, member_name)
        {
            defs.push(ResolvedExprDef {
                path: path.clone(),
                def: member_def.clone(),
            });
        }
    }

    dedup_resolved_expr_defs(defs)
}

fn infer_value_types_from_expr(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    expr: &Expr<'_>,
    context: &SinkSummaryContext<'_>,
) -> Vec<ResolvedExprType> {
    let expr = expr.peel_parens();
    let expr_offset = solgrid_ast::span_to_range(expr.span).start;
    let mut types = Vec::new();

    match &expr.kind {
        ExprKind::Ident(ident) => {
            for def in file.table.resolve_all(ident.as_str(), expr_offset) {
                if let Some(type_info) = &def.type_info {
                    types.push(ResolvedExprType {
                        path: file.path.clone(),
                        ty: type_info.clone(),
                    });
                }
            }

            let cached_source = |candidate: &Path| {
                cached_source(candidate, context.semantic_files, context.get_source)
            };
            if let Some(cross) = resolve_cross_file_symbol(
                &file.table,
                ident.as_str(),
                &file.path,
                &cached_source,
                context.resolver,
            ) {
                if let Some(type_info) = &cross.def.type_info {
                    types.push(ResolvedExprType {
                        path: cross.resolved_path,
                        ty: type_info.clone(),
                    });
                }
            }
        }
        ExprKind::Member(base, member) => {
            for resolved in resolve_member_defs_from_expr(
                file,
                current_contract,
                base,
                member.as_str(),
                context,
            ) {
                if let Some(type_info) = &resolved.def.type_info {
                    types.push(ResolvedExprType {
                        path: resolved.path.clone(),
                        ty: type_info.clone(),
                    });
                }
            }
        }
        ExprKind::Call(callee, args) => {
            if let ExprKind::Type(ty) = &callee.peel_parens().kind {
                types.push(ResolvedExprType {
                    path: file.path.clone(),
                    ty: symbols::type_spec_from_ast(&file.source, ty, None, expr_offset),
                });
            } else {
                for target in
                    resolved_callable_targets(file, current_contract, callee, args, context)
                {
                    if let Some(signature) = callable_signature_for_target(&target, context) {
                        if let Some(first_return_type) = signature.first_return_type {
                            types.push(ResolvedExprType {
                                path: target.path,
                                ty: first_return_type,
                            });
                        }
                    }
                }
            }
        }
        ExprKind::Payable(_) => {
            types.push(ResolvedExprType {
                path: file.path.clone(),
                ty: TypeSpec::Elementary {
                    display: "address payable".to_string(),
                },
            });
        }
        ExprKind::Index(base, IndexKind::Index(_) | IndexKind::Range(_, _)) => {
            for resolved in infer_value_types_from_expr(file, current_contract, base, context) {
                if let Some(indexed) = resolved.ty.index_result() {
                    types.push(ResolvedExprType {
                        path: resolved.path,
                        ty: indexed.clone(),
                    });
                }
            }
        }
        ExprKind::New(ty) => {
            types.push(ResolvedExprType {
                path: file.path.clone(),
                ty: symbols::type_spec_from_ast(&file.source, ty, None, expr_offset),
            });
        }
        _ => {}
    }

    dedup_resolved_expr_types(types)
}

fn resolve_contract_targets_from_expr(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    expr: &Expr<'_>,
    context: &SinkSummaryContext<'_>,
) -> Vec<(PathBuf, String)> {
    let expr = expr.peel_parens();
    let expr_offset = solgrid_ast::span_to_range(expr.span).start;
    let mut targets = Vec::new();

    match &expr.kind {
        ExprKind::Ident(ident) => {
            if ident.as_str() == "this" {
                if let Some(contract_name) = current_contract {
                    targets.push((file.path.clone(), contract_name.to_string()));
                }
            }

            for def in file.table.resolve_all(ident.as_str(), expr_offset) {
                if is_contract_container_symbol(def.kind) {
                    targets.push((file.path.clone(), def.name.clone()));
                }
                if let Some(type_info) = &def.type_info {
                    targets.extend(resolve_contract_targets_from_type_spec(
                        &file.path, type_info, context,
                    ));
                }
            }

            let cached_source = |candidate: &Path| {
                cached_source(candidate, context.semantic_files, context.get_source)
            };
            if let Some(cross) = resolve_cross_file_symbol(
                &file.table,
                ident.as_str(),
                &file.path,
                &cached_source,
                context.resolver,
            ) {
                if is_contract_container_symbol(cross.def.kind) {
                    targets.push((cross.resolved_path.clone(), cross.def.name.clone()));
                }
                if let Some(type_info) = &cross.def.type_info {
                    targets.extend(resolve_contract_targets_from_type_spec(
                        &cross.resolved_path,
                        type_info,
                        context,
                    ));
                }
            }
        }
        ExprKind::Member(base, member) => {
            for resolved in resolve_member_defs_from_expr(
                file,
                current_contract,
                base,
                member.as_str(),
                context,
            ) {
                if is_contract_container_symbol(resolved.def.kind) {
                    targets.push((resolved.path.clone(), resolved.def.name.clone()));
                }
                if let Some(type_info) = &resolved.def.type_info {
                    targets.extend(resolve_contract_targets_from_type_spec(
                        &resolved.path,
                        type_info,
                        context,
                    ));
                }
            }
        }
        ExprKind::Call(callee, args) => {
            for target in resolved_callable_targets(file, current_contract, callee, args, context) {
                if let Some(signature) = callable_signature_for_target(&target, context) {
                    if let Some(first_return_type) = signature.first_return_type {
                        targets.extend(resolve_contract_targets_from_type_spec(
                            &target.path,
                            &first_return_type,
                            context,
                        ));
                    }
                }
            }
        }
        ExprKind::Index(base, IndexKind::Index(_) | IndexKind::Range(_, _)) => {
            for resolved in infer_value_types_from_expr(file, current_contract, base, context) {
                if let Some(indexed) = resolved.ty.index_result() {
                    targets.extend(resolve_contract_targets_from_type_spec(
                        &resolved.path,
                        indexed,
                        context,
                    ));
                }
            }
        }
        ExprKind::New(ty) => {
            let type_spec = symbols::type_spec_from_ast(&file.source, ty, None, expr_offset);
            targets.extend(resolve_contract_targets_from_type_spec(
                &file.path, &type_spec, context,
            ));
        }
        _ => {}
    }

    dedup_contract_targets(targets)
}

fn dedup_callable_targets(candidates: Vec<CallableTargetKey>) -> Vec<CallableTargetKey> {
    let mut unique = candidates;
    unique.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.offset.cmp(&right.offset))
    });
    unique.dedup();
    unique
}

fn resolved_callable_targets(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    callee: &Expr<'_>,
    args: &solar_ast::CallArgs<'_>,
    context: &SinkSummaryContext<'_>,
) -> Vec<CallableTargetKey> {
    let candidates =
        resolved_callable_targets_by_arity(file, current_contract, callee, args.len(), context);
    narrow_callable_targets_by_arguments(file, current_contract, args, candidates, context)
}

fn resolved_callable_targets_by_arity(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    callee: &Expr<'_>,
    arg_count: usize,
    context: &SinkSummaryContext<'_>,
) -> Vec<CallableTargetKey> {
    let callee = callee.peel_parens();
    match &callee.kind {
        ExprKind::Ident(ident) => {
            let offset = solgrid_ast::span_to_range(ident.span).start;
            let defs = file
                .table
                .resolve_all(ident.as_str(), offset)
                .into_iter()
                .filter(|def| is_callable_symbol(def, arg_count))
                .map(|def| CallableTargetKey {
                    path: file.path.clone(),
                    offset: def.name_span.start,
                })
                .collect::<Vec<_>>();
            let mut candidates = dedup_callable_targets(defs);

            if let Some(contract_name) = current_contract {
                let mut visited = HashSet::new();
                candidates.extend(resolve_contract_callable_targets(
                    file,
                    contract_name,
                    ident.as_str(),
                    arg_count,
                    context,
                    true,
                    &mut visited,
                ));
            }

            let cached_source = |candidate: &Path| {
                cached_source(candidate, context.semantic_files, context.get_source)
            };
            candidates.extend(
                resolve_cross_file_symbol_defs(
                    &file.table,
                    ident.as_str(),
                    &file.path,
                    &cached_source,
                    context.resolver,
                )
                .into_iter()
                .filter(|(_, def)| is_callable_symbol(def, arg_count))
                .map(|(path, def)| CallableTargetKey {
                    path,
                    offset: def.name_span.start,
                }),
            );
            dedup_callable_targets(candidates)
        }
        ExprKind::Member(base, member) => {
            let base = base.peel_parens();
            if let ExprKind::Ident(container) = &base.kind {
                let container_name = container.as_str();
                if container_name == "super" {
                    let Some(contract_name) = current_contract else {
                        return Vec::new();
                    };
                    let mut visited = HashSet::new();
                    return dedup_callable_targets(resolve_contract_callable_targets(
                        file,
                        contract_name,
                        member.as_str(),
                        arg_count,
                        context,
                        false,
                        &mut visited,
                    ));
                }
                if container_name == "this" {
                    let Some(contract_name) = current_contract else {
                        return Vec::new();
                    };
                    let mut visited = HashSet::new();
                    return dedup_callable_targets(resolve_contract_callable_targets(
                        file,
                        contract_name,
                        member.as_str(),
                        arg_count,
                        context,
                        true,
                        &mut visited,
                    ));
                }
                if file.contracts.contains_key(container_name) {
                    let mut visited = HashSet::new();
                    return dedup_callable_targets(resolve_contract_callable_targets(
                        file,
                        container_name,
                        member.as_str(),
                        arg_count,
                        context,
                        true,
                        &mut visited,
                    ));
                }

                let offset = solgrid_ast::span_to_range(container.span).start;
                let mut candidates = file
                    .table
                    .resolve(container_name, offset)
                    .map(|container_def| {
                        file.table
                            .resolve_member_all(container_def, member.as_str())
                            .into_iter()
                            .filter(|def| is_callable_symbol(def, arg_count))
                            .map(|def| CallableTargetKey {
                                path: file.path.clone(),
                                offset: def.name_span.start,
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                if let Some(container_def) = file.table.resolve(container_name, offset) {
                    if let Some(type_path) = container_def
                        .type_info
                        .as_ref()
                        .and_then(TypeSpec::member_target)
                    {
                        if let Some((contract_path, contract_name)) =
                            resolve_contract_path_target(file, type_path, context)
                        {
                            if let Some(contract_file) = context.semantic_files.get(&contract_path)
                            {
                                let mut visited = HashSet::new();
                                candidates.extend(resolve_contract_callable_targets(
                                    contract_file,
                                    &contract_name,
                                    member.as_str(),
                                    arg_count,
                                    context,
                                    true,
                                    &mut visited,
                                ));
                            }
                        }
                    }
                }

                let cached_source = |candidate: &Path| {
                    cached_source(candidate, context.semantic_files, context.get_source)
                };
                candidates.extend(
                    resolve_cross_file_member_defs(
                        &file.table,
                        container_name,
                        member.as_str(),
                        &file.path,
                        &cached_source,
                        context.resolver,
                    )
                    .into_iter()
                    .filter(|(_, def)| is_callable_symbol(def, arg_count))
                    .map(|(path, def)| CallableTargetKey {
                        path,
                        offset: def.name_span.start,
                    }),
                );
                dedup_callable_targets(candidates)
            } else {
                let candidates =
                    resolve_contract_targets_from_expr(file, current_contract, base, context)
                        .into_iter()
                        .flat_map(|(path, contract_name)| {
                            let contract_file = context.semantic_files.get(&path)?;
                            let mut visited = HashSet::new();
                            Some(resolve_contract_callable_targets(
                                contract_file,
                                &contract_name,
                                member.as_str(),
                                arg_count,
                                context,
                                true,
                                &mut visited,
                            ))
                        })
                        .flatten()
                        .collect::<Vec<_>>();
                dedup_callable_targets(candidates)
            }
        }
        _ => Vec::new(),
    }
}

fn narrow_callable_targets_by_arguments(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    args: &solar_ast::CallArgs<'_>,
    candidates: Vec<CallableTargetKey>,
    context: &SinkSummaryContext<'_>,
) -> Vec<CallableTargetKey> {
    if candidates.len() <= 1 {
        return candidates;
    }

    let matching = candidates
        .iter()
        .filter(|target| {
            callable_target_accepts_arguments(file, current_contract, args, target, context)
        })
        .cloned()
        .collect::<Vec<_>>();

    // Type inference is intentionally partial. If it cannot identify any
    // candidate, retain the arity-filtered set and let conservative summary
    // intersection decide whether a finding is sound.
    if matching.is_empty() {
        candidates
    } else {
        matching
    }
}

fn callable_target_accepts_arguments(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    args: &solar_ast::CallArgs<'_>,
    target: &CallableTargetKey,
    context: &SinkSummaryContext<'_>,
) -> bool {
    let Some(parameters) = context
        .semantic_files
        .get(&target.path)
        .and_then(|target_file| target_file.callable_parameters.get(&target.offset))
    else {
        return true;
    };

    let argument_pairs = match &args.kind {
        solar_ast::CallArgsKind::Unnamed(_) => args
            .exprs()
            .enumerate()
            .filter_map(|(index, argument)| {
                parameters.get(index).map(|parameter| (argument, parameter))
            })
            .collect::<Vec<_>>(),
        solar_ast::CallArgsKind::Named(named) => {
            let mut pairs = Vec::with_capacity(named.len());
            for argument in named.iter() {
                let Some(parameter) = parameters
                    .iter()
                    .find(|parameter| parameter.name.as_deref() == Some(argument.name.as_str()))
                else {
                    return false;
                };
                pairs.push((&*argument.value, parameter));
            }
            pairs
        }
    };

    argument_pairs.into_iter().all(|(argument, parameter)| {
        let inferred = infer_value_types_from_expr(file, current_contract, argument, context);
        inferred.is_empty()
            || inferred
                .iter()
                .any(|argument_type| type_specs_compatible(&argument_type.ty, &parameter.ty))
    })
}

fn type_specs_compatible(argument: &TypeSpec, parameter: &TypeSpec) -> bool {
    match (argument, parameter) {
        (
            TypeSpec::Elementary { display: argument },
            TypeSpec::Elementary { display: parameter },
        ) => canonical_type_display(argument) == canonical_type_display(parameter),
        (
            TypeSpec::Custom { path: argument, .. },
            TypeSpec::Custom {
                path: parameter, ..
            },
        ) => argument == parameter,
        (
            TypeSpec::Array {
                element: argument,
                display: argument_display,
            },
            TypeSpec::Array {
                element: parameter,
                display: parameter_display,
            },
        ) => {
            type_specs_compatible(argument, parameter)
                && canonical_type_display(argument_display)
                    == canonical_type_display(parameter_display)
        }
        (TypeSpec::Function { display: argument }, TypeSpec::Function { display: parameter })
        | (TypeSpec::Other { display: argument }, TypeSpec::Other { display: parameter }) => {
            canonical_type_display(argument) == canonical_type_display(parameter)
        }
        _ => false,
    }
}

fn canonical_type_display(display: &str) -> String {
    display
        .split_whitespace()
        .filter(|part| !matches!(*part, "memory" | "calldata" | "storage" | "payable"))
        .map(|part| match part {
            "uint" => "uint256",
            "int" => "int256",
            "byte" => "bytes1",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn callable_target_signature_key(
    target: &CallableTargetKey,
    context: &SinkSummaryContext<'_>,
) -> Option<Vec<String>> {
    context
        .semantic_files
        .get(&target.path)?
        .callable_parameters
        .get(&target.offset)
        .map(|parameters| {
            parameters
                .iter()
                .map(|parameter| canonical_type_display(parameter.ty.display()))
                .collect()
        })
}

fn resolve_contract_callable_targets(
    file: &FileSemanticInfo,
    contract_name: &str,
    callable_name: &str,
    arg_count: usize,
    context: &SinkSummaryContext<'_>,
    include_current: bool,
    visited: &mut HashSet<(PathBuf, String)>,
) -> Vec<CallableTargetKey> {
    if !visited.insert((file.path.clone(), contract_name.to_string())) {
        return Vec::new();
    }

    let Some(contract) = file.contracts.get(contract_name) else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    let mut current_candidates = Vec::new();
    if include_current {
        if let Some(callables) = contract.callables.get(callable_name) {
            current_candidates.extend(
                callables
                    .iter()
                    .filter(|callable| callable.arg_count == arg_count)
                    .map(|callable| callable.target.clone()),
            );
        }
    }

    for base in &contract.bases {
        let Some((base_path, base_name)) = resolve_contract_path_target(file, base, context) else {
            continue;
        };
        let Some(base_file) = context.semantic_files.get(&base_path) else {
            continue;
        };
        candidates.extend(resolve_contract_callable_targets(
            base_file,
            &base_name,
            callable_name,
            arg_count,
            context,
            true,
            visited,
        ));
    }

    if !current_candidates.is_empty() {
        let current_signatures = current_candidates
            .iter()
            .filter_map(|target| callable_target_signature_key(target, context))
            .collect::<HashSet<_>>();
        candidates.retain(|target| {
            callable_target_signature_key(target, context)
                .is_none_or(|signature| !current_signatures.contains(&signature))
        });
        candidates.extend(current_candidates);
    }

    candidates
}

fn resolve_contract_path_target(
    file: &FileSemanticInfo,
    path: &TypePath,
    context: &SinkSummaryContext<'_>,
) -> Option<(PathBuf, String)> {
    let cached_source =
        |candidate: &Path| cached_source(candidate, context.semantic_files, context.get_source);

    match path.segments.as_slice() {
        [] => None,
        [name] => {
            if file.contracts.contains_key(name) {
                return Some((file.path.clone(), name.clone()));
            }
            let cross = resolve_cross_file_symbol(
                &file.table,
                name,
                &file.path,
                &cached_source,
                context.resolver,
            )?;
            matches!(
                cross.def.kind,
                SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
            )
            .then_some((cross.resolved_path, cross.def.name))
        }
        [namespace, member, ..] => {
            let cross = resolve_cross_file_member_symbol(
                &file.table,
                namespace,
                member,
                &file.path,
                &cached_source,
                context.resolver,
            )?;
            matches!(
                cross.def.kind,
                SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
            )
            .then_some((cross.resolved_path, cross.def.name))
        }
    }
}

fn resolve_cross_file_symbol_defs(
    current_table: &SymbolTable,
    name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &SharedImportResolver,
) -> Vec<(PathBuf, SymbolDef)> {
    let mut visited = HashSet::new();
    let mut resolved = Vec::new();
    resolve_cross_file_symbol_defs_inner(
        current_table,
        name,
        importing_file,
        get_source,
        resolver,
        &mut visited,
        &mut resolved,
    );
    dedup_cross_file_symbol_defs(resolved)
}

fn resolve_cross_file_symbol_defs_inner(
    current_table: &SymbolTable,
    name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &SharedImportResolver,
    visited: &mut HashSet<PathBuf>,
    resolved_defs: &mut Vec<(PathBuf, SymbolDef)>,
) {
    for import in &current_table.imports {
        let target_name = match &import.symbols {
            ImportedSymbols::Named(names) => names
                .iter()
                .find_map(|(original, alias)| {
                    let local_name = alias.as_deref().unwrap_or(original.as_str());
                    (local_name == name).then_some(original.as_str())
                })
                .unwrap_or_default(),
            ImportedSymbols::Plain(None) => name,
            ImportedSymbols::Plain(Some(_)) | ImportedSymbols::Glob(_) => "",
        };
        if target_name.is_empty() {
            continue;
        }

        let Some(resolved_path) = resolver.resolve(&import.path, importing_file) else {
            continue;
        };
        if !visited.insert(resolved_path.clone()) {
            continue;
        }

        let Some(imported_source) = get_source(&resolved_path) else {
            continue;
        };
        let filename = resolved_path.to_string_lossy().to_string();
        let Some(imported_table) = symbols::build_symbol_table(&imported_source, &filename) else {
            continue;
        };

        let direct_defs = imported_table.resolve_all(target_name, 0);
        if !direct_defs.is_empty() {
            resolved_defs.extend(
                direct_defs
                    .into_iter()
                    .map(|def| (resolved_path.clone(), def.clone())),
            );
        } else {
            resolve_cross_file_symbol_defs_inner(
                &imported_table,
                target_name,
                &resolved_path,
                get_source,
                resolver,
                visited,
                resolved_defs,
            );
        }
    }
}

fn resolve_cross_file_member_defs(
    current_table: &SymbolTable,
    container_name: &str,
    member_name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &SharedImportResolver,
) -> Vec<(PathBuf, SymbolDef)> {
    let mut visited = HashSet::new();
    let mut resolved = Vec::new();
    resolve_cross_file_member_defs_inner(
        current_table,
        CrossFileMemberQuery {
            container_name,
            member_name,
        },
        importing_file,
        get_source,
        resolver,
        &mut visited,
        &mut resolved,
    );
    dedup_cross_file_symbol_defs(resolved)
}

fn resolve_cross_file_member_defs_inner(
    current_table: &SymbolTable,
    query: CrossFileMemberQuery<'_>,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &SharedImportResolver,
    visited: &mut HashSet<PathBuf>,
    resolved_defs: &mut Vec<(PathBuf, SymbolDef)>,
) {
    for import in &current_table.imports {
        let target = match &import.symbols {
            ImportedSymbols::Named(names) => names.iter().find_map(|(original, alias)| {
                let local_name = alias.as_deref().unwrap_or(original.as_str());
                (local_name == query.container_name).then_some((original.as_str(), false))
            }),
            ImportedSymbols::Plain(None) => Some((query.container_name, false)),
            ImportedSymbols::Plain(Some(alias)) if alias == query.container_name => {
                Some((query.member_name, true))
            }
            ImportedSymbols::Glob(alias) if alias == query.container_name => {
                Some((query.member_name, true))
            }
            _ => None,
        };
        let Some((target_name, is_namespace_import)) = target else {
            continue;
        };

        let Some(resolved_path) = resolver.resolve(&import.path, importing_file) else {
            continue;
        };
        if !visited.insert(resolved_path.clone()) {
            continue;
        }

        let Some(imported_source) = get_source(&resolved_path) else {
            continue;
        };
        let filename = resolved_path.to_string_lossy().to_string();
        let Some(imported_table) = symbols::build_symbol_table(&imported_source, &filename) else {
            continue;
        };

        if is_namespace_import {
            let direct_defs = imported_table.resolve_all(target_name, 0);
            if !direct_defs.is_empty() {
                resolved_defs.extend(
                    direct_defs
                        .into_iter()
                        .map(|def| (resolved_path.clone(), def.clone())),
                );
                continue;
            }
        }

        if let Some(container_def) = imported_table.resolve(target_name, 0) {
            let members = imported_table.resolve_member_all(container_def, query.member_name);
            if !members.is_empty() {
                resolved_defs.extend(
                    members
                        .into_iter()
                        .map(|member_def| (resolved_path.clone(), member_def.clone())),
                );
                continue;
            }
        }

        resolve_cross_file_member_defs_inner(
            &imported_table,
            CrossFileMemberQuery {
                container_name: target_name,
                member_name: query.member_name,
            },
            &resolved_path,
            get_source,
            resolver,
            visited,
            resolved_defs,
        );
    }
}

fn dedup_cross_file_symbol_defs(defs: Vec<(PathBuf, SymbolDef)>) -> Vec<(PathBuf, SymbolDef)> {
    let mut unique = defs;
    unique.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.name_span.start.cmp(&right.1.name_span.start))
            .then_with(|| left.1.name.cmp(&right.1.name))
    });
    unique.dedup_by(|left, right| {
        left.0 == right.0
            && left.1.name_span.start == right.1.name_span.start
            && left.1.name == right.1.name
    });
    unique
}

fn cached_source(
    path: &Path,
    semantic_files: &HashMap<PathBuf, FileSemanticInfo>,
    get_source: &dyn Fn(&Path) -> Option<String>,
) -> Option<String> {
    semantic_files
        .get(path)
        .map(|file| file.source.clone())
        .or_else(|| get_source(path))
}

fn sink_indices_for_summary(
    callee_sinks: &HashSet<usize>,
    callee_parameter_names: &[Option<String>],
    argument_parameters: &ArgumentParameterBindings,
    current_parameter_names: &[Option<String>],
) -> HashSet<usize> {
    let mut propagated = HashSet::new();
    for callee_index in callee_sinks {
        let Some(argument_name) = argument_parameter_for_index(
            argument_parameters,
            callee_parameter_names,
            *callee_index,
        ) else {
            continue;
        };
        let Some(current_index) = current_parameter_names
            .iter()
            .position(|name| name.as_deref() == Some(argument_name))
        else {
            continue;
        };
        propagated.insert(current_index);
    }
    propagated
}

fn argument_parameter_for_index<'a>(
    bindings: &'a ArgumentParameterBindings,
    callee_parameter_names: &[Option<String>],
    index: usize,
) -> Option<&'a str> {
    match bindings {
        ArgumentParameterBindings::Positional(parameters) => parameters.get(index)?.as_deref(),
        ArgumentParameterBindings::Named(parameters) => {
            let parameter_name = callee_parameter_names.get(index)?.as_deref()?;
            parameters.get(parameter_name)?.as_deref()
        }
    }
}

fn sink_indices_for_summary_kind(
    summary: &FunctionSinkSummary,
    sink_kind: SinkKind,
    argument_parameters: &ArgumentParameterBindings,
    current_parameter_names: &[Option<String>],
) -> HashSet<usize> {
    let callee_sinks = match sink_kind {
        SinkKind::Delegatecall => &summary.delegatecall_parameters,
        SinkKind::EthTransfer => &summary.eth_transfer_parameters,
    };
    sink_indices_for_summary(
        callee_sinks,
        &summary.parameter_names,
        argument_parameters,
        current_parameter_names,
    )
}

fn propagate_sink_indices_through_edge(
    destinations: &mut HashSet<usize>,
    sink_kind: SinkKind,
    edge: &FunctionCallEdge,
    previous: &HashMap<CallableTargetKey, FunctionSinkSummary>,
    current_parameter_names: &[Option<String>],
) -> bool {
    let mut propagated_candidates = Vec::<HashSet<usize>>::new();
    for callee in &edge.callees {
        let Some(summary) = previous.get(callee) else {
            continue;
        };
        let propagated = sink_indices_for_summary_kind(
            summary,
            sink_kind,
            &edge.argument_parameters,
            current_parameter_names,
        );
        if propagated_candidates.contains(&propagated) {
            continue;
        }
        propagated_candidates.push(propagated);
    }

    let Some(mut propagated) = propagated_candidates.pop() else {
        return false;
    };
    for candidate in propagated_candidates {
        propagated.retain(|index| candidate.contains(index));
        if propagated.is_empty() {
            return false;
        }
    }
    let previous_len = destinations.len();
    destinations.extend(propagated);
    destinations.len() != previous_len
}

fn normalized_propagated_parameters(
    mut propagated: Vec<(String, SinkKind)>,
) -> Vec<(String, SinkKind)> {
    propagated.sort();
    propagated.dedup();
    propagated
}

fn common_propagated_parameters(
    candidates: Vec<Vec<(String, SinkKind)>>,
) -> Vec<(String, SinkKind)> {
    let mut sets = candidates
        .into_iter()
        .map(|candidate| candidate.into_iter().collect::<BTreeSet<_>>());
    let Some(mut common) = sets.next() else {
        return Vec::new();
    };
    for candidate in sets {
        common = common.intersection(&candidate).cloned().collect();
        if common.is_empty() {
            break;
        }
    }
    common.into_iter().collect()
}

fn propagated_sink_summary(
    file: &FileSemanticInfo,
    expr: &Expr<'_>,
    summaries: &HashMap<CallableTargetKey, FunctionSinkSummary>,
    semantic_files: &HashMap<PathBuf, FileSemanticInfo>,
    resolver: &SharedImportResolver,
    get_source: &dyn Fn(&Path) -> Option<String>,
) -> Option<PropagatedSinkFinding> {
    let ExprKind::Call(callee_expr, args) = &expr.kind else {
        return None;
    };
    let context = SinkSummaryContext {
        semantic_files,
        resolver,
        get_source,
    };
    let call_offset = solgrid_ast::span_to_range(expr.span).start;
    let current_contract = file
        .table
        .find_enclosing_function(call_offset)
        .and_then(|function| file.callable_contracts.get(&function.name_span.start))
        .and_then(|contract| contract.as_deref());
    let (call_name, call_span) = call_site_label_and_span(callee_expr)?;
    let candidates = resolved_callable_targets(file, current_contract, callee_expr, args, &context);
    let mut propagated_candidates = candidates
        .into_iter()
        .filter_map(|callee| {
            let summary = summaries.get(&callee)?;
            let propagated = normalized_propagated_parameters(propagated_parameters_for_summary(
                file, args, summary,
            ));
            Some(propagated)
        })
        .collect::<Vec<_>>();
    propagated_candidates.sort();
    propagated_candidates.dedup();
    let propagated = common_propagated_parameters(propagated_candidates);
    (!propagated.is_empty()).then_some((call_span, call_name, propagated))
}

fn propagated_parameters_for_summary(
    file: &FileSemanticInfo,
    args: &solar_ast::CallArgs<'_>,
    summary: &FunctionSinkSummary,
) -> Vec<(String, SinkKind)> {
    let mut propagated = Vec::new();
    for index in &summary.delegatecall_parameters {
        let Some(argument) = call_argument_for_parameter(args, &summary.parameter_names, *index)
        else {
            continue;
        };
        if let Some(parameter_name) = resolved_parameter_name(file, argument) {
            propagated.push((parameter_name, SinkKind::Delegatecall));
        }
    }
    for index in &summary.eth_transfer_parameters {
        let Some(argument) = call_argument_for_parameter(args, &summary.parameter_names, *index)
        else {
            continue;
        };
        if let Some(parameter_name) = resolved_parameter_name(file, argument) {
            propagated.push((parameter_name, SinkKind::EthTransfer));
        }
    }
    propagated
}

fn call_argument_for_parameter<'a, 'ast>(
    args: &'a solar_ast::CallArgs<'ast>,
    parameter_names: &[Option<String>],
    index: usize,
) -> Option<&'a Expr<'ast>> {
    match &args.kind {
        solar_ast::CallArgsKind::Unnamed(_) => args.exprs().nth(index),
        solar_ast::CallArgsKind::Named(named) => {
            let parameter_name = parameter_names.get(index)?.as_deref()?;
            named
                .iter()
                .find(|argument| argument.name.as_str() == parameter_name)
                .map(|argument| &*argument.value)
        }
    }
}

fn argument_parameter_bindings(args: &solar_ast::CallArgs<'_>) -> ArgumentParameterBindings {
    match &args.kind {
        solar_ast::CallArgsKind::Unnamed(_) => ArgumentParameterBindings::Positional(
            args.exprs().map(parameter_name_for_expr).collect(),
        ),
        solar_ast::CallArgsKind::Named(named) => ArgumentParameterBindings::Named(
            named
                .iter()
                .map(|argument| {
                    (
                        argument.name.as_str().to_string(),
                        parameter_name_for_expr(argument.value),
                    )
                })
                .collect(),
        ),
    }
}

fn resolved_parameter_name(file: &FileSemanticInfo, expr: &Expr<'_>) -> Option<String> {
    let (name, span) = parameter_name_and_span_for_expr(expr)?;
    let resolved = file.table.resolve(&name, span.start)?;
    (resolved.kind == SymbolKind::Parameter).then_some(name)
}

fn parameter_name_for_expr(expr: &Expr<'_>) -> Option<String> {
    parameter_name_and_span_for_expr(expr).map(|(name, _)| name)
}

fn parameter_name_and_span_for_expr(expr: &Expr<'_>) -> Option<(String, std::ops::Range<usize>)> {
    match &expr.peel_parens().kind {
        ExprKind::Ident(ident) => Some((
            ident.as_str().to_string(),
            solgrid_ast::span_to_range(ident.span),
        )),
        ExprKind::Payable(args) => args
            .exprs()
            .next()
            .and_then(parameter_name_and_span_for_expr),
        ExprKind::Call(callee, args)
            if matches!(callee.kind, ExprKind::Type(_)) && args.len() == 1 =>
        {
            args.exprs()
                .next()
                .and_then(parameter_name_and_span_for_expr)
        }
        _ => None,
    }
}

fn delegatecall_target_identifier_from_sink_expr(
    file: &FileSemanticInfo,
    expr: &Expr<'_>,
) -> Option<(String, std::ops::Range<usize>)> {
    let ExprKind::Call(callee, _) = &expr.kind else {
        return None;
    };
    let callee = match &callee.kind {
        ExprKind::CallOptions(inner, _) => inner,
        _ => callee,
    };
    let ExprKind::Member(base, member) = &callee.kind else {
        return None;
    };
    (member.as_str() == "delegatecall"
        && is_low_level_address_receiver(&file.source, &file.table, base))
    .then(|| delegatecall_target_identifier(base))
    .flatten()
}

fn eth_transfer_target_identifier_from_sink_expr(
    file: &FileSemanticInfo,
    expr: &Expr<'_>,
) -> Option<(String, &'static str, std::ops::Range<usize>)> {
    let ExprKind::Call(callee, args) = &expr.kind else {
        return None;
    };

    let (base, member, has_value_option) = match &callee.kind {
        ExprKind::Member(base, member) => (base, member, false),
        ExprKind::CallOptions(inner, options) => {
            let ExprKind::Member(base, member) = &inner.kind else {
                return None;
            };
            (
                base,
                member,
                call_options_contain_nonzero_named_arg(options, "value"),
            )
        }
        _ => return None,
    };

    let method_label = match member.as_str() {
        "send" if args.len() == 1 => ".send()",
        "transfer" if args.len() == 1 => ".transfer()",
        "call" if has_value_option => ".call{value: ...}()",
        _ => return None,
    };
    if !is_low_level_address_receiver(&file.source, &file.table, base) {
        return None;
    }
    let (target_name, span) = delegatecall_target_identifier(base)?;
    Some((target_name, method_label, span))
}

fn is_callable_symbol(def: &SymbolDef, arg_count: usize) -> bool {
    matches!(def.kind, SymbolKind::Function | SymbolKind::Constructor)
        && def
            .signature
            .as_ref()
            .map(|signature| signature.parameters.len() == arg_count)
            .unwrap_or(false)
}

fn function_target_offset(function: &solar_ast::ItemFunction<'_>) -> Option<usize> {
    function
        .header
        .name
        .map(|name| solgrid_ast::span_to_range(name.span).start)
}

fn call_site_label_and_span(callee: &Expr<'_>) -> Option<(String, std::ops::Range<usize>)> {
    let callee = callee.peel_parens();
    match &callee.kind {
        ExprKind::Ident(ident) => Some((
            ident.as_str().to_string(),
            solgrid_ast::span_to_range(ident.span),
        )),
        ExprKind::Member(_, member) => Some((
            member.as_str().to_string(),
            solgrid_ast::span_to_range(member.span),
        )),
        _ => None,
    }
}

fn is_builtin_error_path(path: &solar_ast::AstPath<'_>) -> bool {
    matches!(
        path.segments(),
        [segment] if matches!(segment.as_str(), "Error" | "Panic")
    )
}

fn diagnostic_code(diagnostic: &ls_types::Diagnostic) -> Option<&str> {
    match &diagnostic.code {
        Some(ls_types::NumberOrString::String(code)) => Some(code.as_str()),
        _ => None,
    }
}

fn suppressed_rule_ids(code: &str) -> &'static [&'static str] {
    match code {
        UNCHECKED_LOW_LEVEL_CALL_ID | USER_CONTROLLED_DELEGATECALL_ID => {
            &["security/low-level-calls"]
        }
        USER_CONTROLLED_ETH_TRANSFER_ID => {
            &["security/arbitrary-send-eth", "security/low-level-calls"]
        }
        _ => &[],
    }
}

fn ranges_overlap(left: &ls_types::Range, right: &ls_types::Range) -> bool {
    compare_positions(&left.start, &right.end).is_lt()
        && compare_positions(&right.start, &left.end).is_lt()
}

fn compare_positions(left: &ls_types::Position, right: &ls_types::Position) -> std::cmp::Ordering {
    left.line
        .cmp(&right.line)
        .then_with(|| left.character.cmp(&right.character))
}

fn compiler_lsp_diagnostic(
    source: &str,
    id: &str,
    title: &str,
    message: String,
    span: std::ops::Range<usize>,
) -> ls_types::Diagnostic {
    let data = serde_json::to_value(FindingMeta::compiler(
        id.to_string(),
        title.to_string(),
        Severity::Error,
    ))
    .ok();

    ls_types::Diagnostic {
        range: convert::span_to_range(source, &span),
        severity: Some(ls_types::DiagnosticSeverity::ERROR),
        code: Some(ls_types::NumberOrString::String(id.into())),
        code_description: None,
        source: Some("solgrid".into()),
        message,
        related_information: None,
        tags: None,
        data,
    }
}

fn detector_lsp_diagnostic(
    source: &str,
    id: &str,
    title: &str,
    message: String,
    span: std::ops::Range<usize>,
    severity: Severity,
    confidence: Confidence,
) -> ls_types::Diagnostic {
    let data = serde_json::to_value(FindingMeta {
        id: id.to_string(),
        title: title.to_string(),
        category: "security".into(),
        severity,
        kind: FindingKind::Detector,
        confidence: Some(confidence),
        help_url: semantic_detector_help_url(id).map(str::to_string),
        suppressible: true,
        has_fix: false,
    })
    .ok();

    ls_types::Diagnostic {
        range: convert::span_to_range(source, &span),
        severity: Some(convert::severity_to_lsp(severity)),
        code: Some(ls_types::NumberOrString::String(id.into())),
        code_description: None,
        source: Some("solgrid".into()),
        message,
        related_information: None,
        tags: None,
        data,
    }
}

fn semantic_detector_help_url(id: &str) -> Option<&'static str> {
    match id {
        UNCHECKED_LOW_LEVEL_CALL_ID => Some(
            "https://github.com/TateB/solgrid/blob/main/docs/semantic-detectors.md#security-unchecked-low-level-call",
        ),
        USER_CONTROLLED_DELEGATECALL_ID => Some(
            "https://github.com/TateB/solgrid/blob/main/docs/semantic-detectors.md#security-user-controlled-delegatecall",
        ),
        USER_CONTROLLED_ETH_TRANSFER_ID => Some(
            "https://github.com/TateB/solgrid/blob/main/docs/semantic-detectors.md#security-user-controlled-eth-transfer",
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::ImportResolver;
    use solgrid_project::ProjectIndex;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_lint_to_lsp_diagnostics_detects_issues() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Test {
    function bad() public {
        require(tx.origin == msg.sender);
    }
}
"#;
        let engine = LintEngine::new();
        let config = Config::default();
        let diagnostics = lint_to_lsp_diagnostics(&engine, source, Path::new("test.sol"), &config);

        // Should detect at least the tx.origin usage
        assert!(
            !diagnostics.is_empty(),
            "should detect diagnostics in source with known issues"
        );

        // Verify LSP diagnostic structure
        let first = &diagnostics[0];
        assert_eq!(first.source, Some("solgrid".into()));
        assert!(first.severity.is_some());
        assert!(first.code.is_some());
    }

    #[test]
    fn test_lint_to_lsp_diagnostics_clean_file() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;
contract Test {
    function good() public pure returns (uint256) {
        return 42;
    }
}
"#;
        let engine = LintEngine::new();
        let mut config = Config::default();
        // Disable some rules that might fire on this simple example
        config
            .lint
            .rules
            .insert("docs/natspec".into(), solgrid_config::RuleLevel::Off);
        let diagnostics = lint_to_lsp_diagnostics(&engine, source, Path::new("clean.sol"), &config);
        // Should not detect any security/naming issues on clean code
        let security_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                matches!(&d.code, Some(ls_types::NumberOrString::String(id)) if id.starts_with("security/"))
            })
            .collect();
        assert!(
            security_diags.is_empty(),
            "clean source should have no security diagnostics, found: {:?}",
            security_diags.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_lint_to_lsp_diagnostics_with_remappings_detects_prefer_remappings() {
        let source = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "../utils/Helper.sol";
contract Test {}
"#;
        let engine = LintEngine::new();
        let mut config = Config::default();
        config.lint.preset = solgrid_config::RulePreset::All;
        let remappings = vec![("@src/".to_string(), PathBuf::from("/project/src/"))];
        let diagnostics = lint_to_lsp_diagnostics_with_remappings(
            &engine,
            source,
            Path::new("/project/src/contracts/Token.sol"),
            &config,
            &remappings,
        );

        assert!(diagnostics.iter().any(|d| {
            matches!(
                &d.code,
                Some(ls_types::NumberOrString::String(id)) if id == "style/prefer-remappings"
            )
        }));
    }

    #[test]
    fn test_unresolved_import_produces_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        let importing = dir.path().join("Main.sol");
        fs::write(&importing, "").unwrap();

        let source = r#"import "./NonExistent.sol";"#;
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let diags = unresolved_import_diagnostics(source, &importing, &resolver);

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(ls_types::DiagnosticSeverity::ERROR));
        assert_eq!(
            diags[0].code,
            Some(ls_types::NumberOrString::String(
                "compiler/unresolved-import".into()
            ))
        );
        assert!(diags[0].message.contains("NonExistent.sol"));
        assert!(diags[0].data.is_some());
    }

    #[test]
    fn test_resolved_import_no_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        let token_file = dir.path().join("Token.sol");
        fs::write(&token_file, "contract Token {}").unwrap();
        let importing = dir.path().join("Main.sol");
        fs::write(&importing, "").unwrap();

        let source = r#"import "./Token.sol";"#;
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let diags = unresolved_import_diagnostics(source, &importing, &resolver);

        assert!(diags.is_empty());
    }

    #[test]
    fn test_mixed_resolved_and_unresolved_imports() {
        let dir = tempfile::tempdir().unwrap();
        let token_file = dir.path().join("Token.sol");
        fs::write(&token_file, "contract Token {}").unwrap();
        let importing = dir.path().join("Main.sol");
        fs::write(&importing, "").unwrap();

        let source = "import \"./Token.sol\";\nimport \"./Missing.sol\";";
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let diags = unresolved_import_diagnostics(source, &importing, &resolver);

        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Missing.sol"));
    }

    #[test]
    fn test_unresolved_import_parse_failure_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let importing = dir.path().join("Bad.sol");
        fs::write(&importing, "").unwrap();

        let source = "this is not valid solidity {{{";
        let resolver = ImportResolver::new(Some(dir.path().to_path_buf()));
        let diags = unresolved_import_diagnostics(source, &importing, &resolver);

        assert!(diags.is_empty());
    }

    #[test]
    fn test_lint_diagnostics_include_finding_metadata() {
        let source = r#"pragma solidity ^0.8.0;
contract Test {
    function bad() public {
        require(tx.origin == msg.sender);
    }
}
"#;
        let engine = LintEngine::new();
        let diagnostics =
            lint_to_lsp_diagnostics(&engine, source, Path::new("Test.sol"), &Config::default());
        let tx_origin = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        "security/tx-origin".into(),
                    ))
            })
            .expect("expected tx.origin finding");

        let data = tx_origin.data.clone().expect("metadata should be attached");
        let finding: FindingMeta = serde_json::from_value(data).expect("valid finding metadata");
        assert_eq!(finding.id, "security/tx-origin");
        assert_eq!(finding.kind, solgrid_diagnostics::FindingKind::Detector);
    }

    #[test]
    fn test_compiler_diagnostics_report_unresolved_custom_type_and_base_contract() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Broken.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Broken is MissingBase {
    MissingType private value;
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    "compiler/unresolved-base-contract".into(),
                ))
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    "compiler/unresolved-type".into(),
                ))
        }));
    }

    #[test]
    fn test_compiler_diagnostics_use_overlay_for_imported_base_resolution() {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("Base.sol");
        let main_path = dir.path().join("Main.sol");
        let base_source = "pragma solidity ^0.8.0; contract Base {}";
        let overlay_source = "pragma solidity ^0.8.0; contract RenamedBase {}";
        let main_source = r#"pragma solidity ^0.8.0;
import {Base} from "./Base.sol";
contract Main is Base {}
"#;
        fs::write(&base_path, base_source).unwrap();
        fs::write(&main_path, main_source).unwrap();
        let index = ProjectIndex::build(dir.path());
        let canonical_base = base_path.canonicalize().unwrap();
        let get_source = |candidate: &Path| {
            if candidate.canonicalize().ok().as_ref() == Some(&canonical_base) {
                Some(overlay_source.to_string())
            } else {
                fs::read_to_string(candidate).ok()
            }
        };

        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic_code(diagnostic) == Some("compiler/unresolved-base-contract")
            }),
            "diagnostics: {diagnostics:#?}"
        );
    }

    #[test]
    fn test_compiler_diagnostics_resolve_imported_custom_type_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let dep = dir.path().join("Types.sol");
        let main = dir.path().join("Main.sol");
        fs::write(&dep, "pragma solidity ^0.8.0; struct Point { uint256 x; }").unwrap();

        let source = r#"pragma solidity ^0.8.0;
import {Point as Coord} from "./Types.sol";
contract Main {
    Coord private point;
}
"#;
        fs::write(&main, source).unwrap();

        let mut index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        index.update_file(&dep, &std::fs::read_to_string(&dep).unwrap());

        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &main, &get_source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    "compiler/unresolved-type".into(),
                ))
        }));
    }

    #[test]
    fn test_compiler_diagnostics_resolve_imported_inherited_modifier() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("Base.sol");
        let main = dir.path().join("Main.sol");
        fs::write(
            &base,
            r#"pragma solidity ^0.8.0;
contract Base {
    modifier onlyRootRoles(uint256 roleBitmap) {
        _;
    }
}
"#,
        )
        .unwrap();

        let source = r#"pragma solidity ^0.8.0;
import {Base} from "./Base.sol";
contract Main is Base {
    function update() external onlyRootRoles(1) {}
}
"#;
        fs::write(&main, source).unwrap();

        let mut index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        index.update_file(&base, &std::fs::read_to_string(&base).unwrap());

        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &main, &get_source);

        assert!(
            !diagnostics.iter().any(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        "compiler/unresolved-modifier".into(),
                    ))
            }),
            "got diagnostics: {diagnostics:?}"
        );
    }

    #[test]
    fn test_compiler_diagnostics_resolve_imported_inherited_error_and_event() {
        let dir = tempfile::tempdir().unwrap();
        let interface = dir.path().join("ErrorsAndEvents.sol");
        let main = dir.path().join("Main.sol");
        fs::write(
            &interface,
            r#"pragma solidity ^0.8.0;
interface ErrorsAndEvents {
    error NotValid(string label);
    event Checked(address indexed account);
}
"#,
        )
        .unwrap();

        let source = r#"pragma solidity ^0.8.0;
import {ErrorsAndEvents} from "./ErrorsAndEvents.sol";
contract Main is ErrorsAndEvents {
    function check(string calldata label) external {
        emit Checked(msg.sender);
        revert NotValid(label);
    }
}
"#;
        fs::write(&main, source).unwrap();

        let mut index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        index.update_file(&interface, &std::fs::read_to_string(&interface).unwrap());

        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &main, &get_source);

        for code in ["compiler/unresolved-error", "compiler/unresolved-event"] {
            assert!(
                !diagnostics.iter().any(|diagnostic| {
                    diagnostic.code == Some(ls_types::NumberOrString::String(code.into()))
                }),
                "got diagnostics for {code}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn test_compiler_diagnostics_report_unresolved_event_and_error_targets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("BrokenTargets.sol");
        let source = r#"pragma solidity ^0.8.0;
contract BrokenTargets {
    function fail() external {
        emit MissingEvent(1);
        revert MissingError(2);
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    "compiler/unresolved-event".into(),
                ))
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    "compiler/unresolved-error".into(),
                ))
        }));
    }

    #[test]
    fn test_compiler_diagnostics_skip_builtin_revert_error_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("BuiltinError.sol");
        let source = r#"pragma solidity ^0.8.0;
contract BuiltinError {
    function fail() external pure {
        revert Error("failed");
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    "compiler/unresolved-error".into(),
                ))
        }));
    }

    #[test]
    fn test_compiler_diagnostics_report_unchecked_low_level_call() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("LowLevelCall.sol");
        let source = r#"pragma solidity ^0.8.0;
contract LowLevelCall {
    function run(address target, bytes memory payload) external {
        target.call(payload);
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        let finding = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        UNCHECKED_LOW_LEVEL_CALL_ID.into(),
                    ))
            })
            .expect("expected unchecked low-level call finding");

        assert_eq!(
            finding.severity,
            Some(ls_types::DiagnosticSeverity::WARNING)
        );
        let meta: FindingMeta = serde_json::from_value(
            finding
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.id, UNCHECKED_LOW_LEVEL_CALL_ID);
        assert_eq!(meta.kind, FindingKind::Detector);
        assert_eq!(meta.confidence, Some(Confidence::High));
        assert_eq!(meta.category, "security");
        assert_eq!(
            meta.help_url.as_deref(),
            semantic_detector_help_url(UNCHECKED_LOW_LEVEL_CALL_ID)
        );
    }

    #[test]
    fn test_compiler_diagnostics_skip_checked_low_level_call() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CheckedCall.sol");
        let source = r#"pragma solidity ^0.8.0;
contract CheckedCall {
    function run(address target, bytes memory payload) external {
        (bool ok,) = target.call(payload);
        require(ok, "call failed");
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    UNCHECKED_LOW_LEVEL_CALL_ID.into(),
                ))
        }));
    }

    #[test]
    fn test_compiler_diagnostics_report_user_controlled_delegatecall() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Delegatecall.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Delegatecall {
    function run(address implementation, bytes memory payload) external {
        implementation.delegatecall(payload);
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        let finding = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
            })
            .expect("expected user-controlled delegatecall finding");

        assert_eq!(finding.severity, Some(ls_types::DiagnosticSeverity::ERROR));
        let meta: FindingMeta = serde_json::from_value(
            finding
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.id, USER_CONTROLLED_DELEGATECALL_ID);
        assert_eq!(meta.kind, FindingKind::Detector);
        assert_eq!(meta.confidence, Some(Confidence::High));
        assert_eq!(meta.severity, Severity::Error);
        assert_eq!(
            meta.help_url.as_deref(),
            semantic_detector_help_url(USER_CONTROLLED_DELEGATECALL_ID)
        );
    }

    #[test]
    fn test_compiler_diagnostics_skip_delegatecall_to_state_variable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Delegatecall.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Delegatecall {
    address private implementation;

    function run(bytes memory payload) external {
        implementation.delegatecall(payload);
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    USER_CONTROLLED_DELEGATECALL_ID.into(),
                ))
        }));
    }

    #[test]
    fn test_compiler_diagnostics_report_interprocedural_delegatecall_flow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("DelegatecallWrapper.sol");
        let source = r#"pragma solidity ^0.8.0;
contract DelegatecallWrapper {
    function run(address implementation, bytes memory payload) external {
        _delegate(implementation, payload);
    }

    function _delegate(address target, bytes memory payload) internal {
        target.delegatecall(payload);
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into delegatecall via `_delegate`")
            })
            .expect("expected propagated delegatecall finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_inherited_delegatecall_flow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("DelegatecallInheritance.sol");
        let source = r#"pragma solidity ^0.8.0;
contract BaseDelegatecall {
    function _delegate(address target, bytes memory payload) internal {
        target.delegatecall(payload);
    }
}

contract DerivedDelegatecall is BaseDelegatecall {
    function run(address implementation, bytes memory payload) external {
        _delegate(implementation, payload);
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into delegatecall via `_delegate`")
            })
            .expect("expected inherited delegatecall finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_user_controlled_eth_transfer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("EthTransfer.sol");
        let source = r#"pragma solidity ^0.8.0;
contract EthTransfer {
    function pay(address recipient, uint256 amount) external payable {
        payable(recipient).call{value: amount}("");
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        let finding = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
            })
            .expect("expected user-controlled ETH transfer finding");

        assert_eq!(
            finding.severity,
            Some(ls_types::DiagnosticSeverity::WARNING)
        );
        let meta: FindingMeta = serde_json::from_value(
            finding
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.id, USER_CONTROLLED_ETH_TRANSFER_ID);
        assert_eq!(meta.kind, FindingKind::Detector);
        assert_eq!(meta.confidence, Some(Confidence::High));
        assert_eq!(meta.severity, Severity::Warning);
        assert_eq!(
            meta.help_url.as_deref(),
            semantic_detector_help_url(USER_CONTROLLED_ETH_TRANSFER_ID)
        );
    }

    #[test]
    fn test_compiler_diagnostics_skip_eth_transfer_to_state_variable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("EthTransfer.sol");
        let source = r#"pragma solidity ^0.8.0;
contract EthTransfer {
    address payable private treasury;

    function pay(uint256 amount) external payable {
        treasury.transfer(amount);
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                ))
        }));
    }

    #[test]
    fn test_compiler_diagnostics_report_interprocedural_eth_transfer_flow() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("EthTransferWrapper.sol");
        let source = r#"pragma solidity ^0.8.0;
contract EthTransferWrapper {
    function pay(address recipient, uint256 amount) external payable {
        _pay(recipient, amount);
    }

    function _pay(address target, uint256 amount) internal {
        payable(target).call{value: amount}("");
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, &path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `_pay`")
            })
            .expect("expected propagated ETH transfer finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_cross_file_inherited_eth_transfer_flow() {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("BasePay.sol");
        let derived_path = dir.path().join("DerivedPay.sol");
        let base_source = r#"pragma solidity ^0.8.0;
contract BasePay {
    function _pay(address target, uint256 amount) internal {
        payable(target).call{value: amount}("");
    }
}
"#;
        let derived_source = r#"pragma solidity ^0.8.0;
import "./BasePay.sol";

contract DerivedPay is BasePay {
    function pay(address recipient, uint256 amount) external payable {
        _pay(recipient, amount);
    }
}
"#;
        fs::write(&base_path, base_source).unwrap();
        fs::write(&derived_path, derived_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics =
            compiler_to_lsp_diagnostics(&index, derived_source, &derived_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `_pay`")
            })
            .expect("expected inherited ETH transfer finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_contract_typed_delegatecall_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("DelegateHelper.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
contract DelegateHelper {
    function run(address target, bytes memory payload) public {
        target.delegatecall(payload);
    }
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./DelegateHelper.sol";

contract Main {
    DelegateHelper private helper;

    constructor(DelegateHelper initialHelper) {
        helper = initialHelper;
    }

    function run(address implementation, bytes memory payload) external {
        helper.run(implementation, payload);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into delegatecall via `run`")
            })
            .expect("expected contract-typed delegatecall wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_contract_typed_eth_transfer_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("PayHelper.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
contract PayHelper {
    function pay(address target, uint256 amount) public payable {
        payable(target).call{value: amount}("");
    }
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./PayHelper.sol";

contract Main {
    PayHelper private helper;

    constructor(PayHelper initialHelper) {
        helper = initialHelper;
    }

    function pay(address recipient, uint256 amount) external payable {
        helper.pay(recipient, amount);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `pay`")
            })
            .expect("expected contract-typed ETH transfer wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_getter_returned_delegatecall_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("DelegateHelper.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
contract DelegateHelper {
    function run(address target, bytes memory payload) public {
        target.delegatecall(payload);
    }
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./DelegateHelper.sol";

contract Main {
    DelegateHelper private helper;

    constructor(DelegateHelper initialHelper) {
        helper = initialHelper;
    }

    function getHelper() internal view returns (DelegateHelper) {
        return helper;
    }

    function run(address implementation, bytes memory payload) external {
        getHelper().run(implementation, payload);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into delegatecall via `run`")
            })
            .expect("expected getter-returned delegatecall wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_overloaded_getter_returned_eth_transfer_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("PayHelper.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
contract PayHelper {
    function pay(address target, uint256 amount) public payable {
        payable(target).call{value: amount}("");
    }
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./PayHelper.sol";

contract Main {
    PayHelper private helper;

    constructor(PayHelper initialHelper) {
        helper = initialHelper;
    }

    function getHelper(address recipient) internal view returns (PayHelper) {
        recipient;
        return helper;
    }

    function getHelper(address payable recipient) internal view returns (PayHelper) {
        recipient;
        return helper;
    }

    function pay(address payable recipient, uint256 amount) external payable {
        getHelper(recipient).pay(recipient, amount);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `pay`")
            })
            .expect("expected overloaded getter-returned ETH transfer wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_common_subset_non_unique_helper_contract_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_a_path = dir.path().join("PayHelperA.sol");
        let helper_b_path = dir.path().join("PayHelperB.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_a_source = r#"pragma solidity ^0.8.0;
contract PayHelperA {
    function pay(address target, address refund, uint256 amount) public payable {
        payable(target).call{value: amount}("");
        payable(refund).call{value: amount}("");
    }
}
"#;
        let helper_b_source = r#"pragma solidity ^0.8.0;
contract PayHelperB {
    function pay(address target, address refund, uint256 amount) public payable {
        refund;
        payable(target).call{value: amount}("");
    }
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./PayHelperA.sol";
import "./PayHelperB.sol";

contract Main {
    PayHelperA private helperA;
    PayHelperB private helperB;

    constructor(PayHelperA initialHelperA, PayHelperB initialHelperB) {
        helperA = initialHelperA;
        helperB = initialHelperB;
    }

    function getHelper(bytes memory route) internal view returns (PayHelperA) {
        route;
        return helperA;
    }

    function getHelper(string memory route) internal view returns (PayHelperB) {
        route;
        return helperB;
    }

    function pay(address recipient, address refund, uint256 amount, bytes memory route) external payable {
        getHelper(route).pay(recipient, refund, amount);
    }
}
"#;
        fs::write(&helper_a_path, helper_a_source).unwrap();
        fs::write(&helper_b_path, helper_b_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `pay`")
                    && diagnostic.message.contains("`recipient`")
            })
            .expect("expected common-subset non-unique helper contract finding");

        assert!(!propagated.message.contains("`refund`"));
        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_indexed_eth_transfer_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("PayHelper.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
contract PayHelper {
    function pay(address target, uint256 amount) public payable {
        payable(target).call{value: amount}("");
    }
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./PayHelper.sol";

contract Main {
    mapping(uint256 => PayHelper) private helpers;

    function pay(uint256 helperId, address recipient, uint256 amount) external payable {
        helpers[helperId].pay(recipient, amount);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `pay`")
            })
            .expect("expected indexed ETH transfer wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_imported_overloaded_delegatecall_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("BridgeHelper.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
function bridge(address target, bytes memory payload) {
    target.delegatecall(payload);
}

function bridge(uint256 marker, bytes memory payload) {
    marker;
    payload.length;
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./BridgeHelper.sol";

contract Main {
    function run(address implementation, bytes memory payload) external {
        bridge(implementation, payload);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into delegatecall via `bridge`")
            })
            .expect("expected imported overloaded delegatecall wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_imported_overloaded_eth_transfer_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("PayLib.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
library PayLib {
    function pay(address target, uint256 amount) internal {
        payable(target).call{value: amount}("");
    }

    function pay(uint256 marker, uint256 amount) internal {
        marker;
        amount;
    }
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./PayLib.sol";

contract Main {
    function pay(address recipient, uint256 amount) external payable {
        PayLib.pay(recipient, amount);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `pay`")
            })
            .expect("expected imported overloaded ETH transfer wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_same_summary_overloaded_delegatecall_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("BridgeHelper.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
function bridge(address target, bytes memory payload) {
    target.delegatecall(payload);
}

function bridge(address payable target, bytes memory payload) {
    address(target).delegatecall(payload);
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./BridgeHelper.sol";

contract Main {
    function run(address payable implementation, bytes memory payload) external {
        _run(implementation, payload);
    }

    function _run(address payable implementation, bytes memory payload) internal {
        bridge(implementation, payload);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into delegatecall via `_run`")
            })
            .expect("expected same-summary overloaded delegatecall wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_common_subset_overloaded_delegatecall_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("BridgeHelper.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
function bridge(address target, address admin, bytes memory payload) {
    target.delegatecall(payload);
    admin.delegatecall(payload);
}

function bridge(address target, address admin, string memory payload) {
    admin;
    target.delegatecall(bytes(payload));
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./BridgeHelper.sol";

contract Main {
    function run(address implementation, address admin, bytes memory payload) external {
        bridge(implementation, admin, payload);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into delegatecall via `bridge`")
                    && diagnostic.message.contains("`implementation`")
            })
            .expect("expected common-subset overloaded delegatecall wrapper finding");

        assert!(!propagated.message.contains("`admin`"));
        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_transitive_imported_delegatecall_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("BridgeHelper.sol");
        let forwarder_path = dir.path().join("Forwarder.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
function bridge(address target, bytes memory payload) {
    target.delegatecall(payload);
}

function bridge(address payable target, bytes memory payload) {
    address(target).delegatecall(payload);
}
"#;
        let forwarder_source = r#"pragma solidity ^0.8.0;
import "./BridgeHelper.sol";

function forward(address payable target, bytes memory payload) {
    bridge(target, payload);
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./Forwarder.sol";

contract Main {
    function run(address payable implementation, bytes memory payload) external {
        forward(implementation, payload);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&forwarder_path, forwarder_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into delegatecall via `forward`")
            })
            .expect("expected transitive imported delegatecall wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_transitive_common_subset_delegatecall_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("BridgeHelper.sol");
        let forwarder_path = dir.path().join("Forwarder.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
function bridge(address target, address admin, bytes memory payload) {
    target.delegatecall(payload);
    admin.delegatecall(payload);
}

function bridge(address target, address admin, string memory payload) {
    admin;
    target.delegatecall(bytes(payload));
}
"#;
        let forwarder_source = r#"pragma solidity ^0.8.0;
import "./BridgeHelper.sol";

function forward(address implementation, address admin, bytes memory payload) {
    bridge(implementation, admin, payload);
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./Forwarder.sol";

contract Main {
    function run(address implementation, address admin, bytes memory payload) external {
        forward(implementation, admin, payload);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&forwarder_path, forwarder_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_DELEGATECALL_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into delegatecall via `forward`")
                    && diagnostic.message.contains("`implementation`")
            })
            .expect("expected transitive common-subset delegatecall wrapper finding");

        assert!(!propagated.message.contains("`admin`"));
        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_same_summary_overloaded_eth_transfer_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("PayLib.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
library PayLib {
    function pay(address target, uint256 amount) internal {
        payable(target).call{value: amount}("");
    }

    function pay(address payable target, uint256 amount) internal {
        target.call{value: amount}("");
    }
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./PayLib.sol";

contract Main {
    function pay(address payable recipient, uint256 amount) external payable {
        PayLib.pay(recipient, amount);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `pay`")
            })
            .expect("expected same-summary overloaded ETH transfer wrapper finding");

        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_compiler_diagnostics_report_common_subset_overloaded_eth_transfer_wrapper_flow() {
        let dir = tempfile::tempdir().unwrap();
        let helper_path = dir.path().join("PayLib.sol");
        let main_path = dir.path().join("Main.sol");
        let helper_source = r#"pragma solidity ^0.8.0;
library PayLib {
    function pay(address target, address refund, uint256 amount, bytes memory note) internal {
        note;
        payable(target).call{value: amount}("");
        payable(refund).call{value: amount}("");
    }

    function pay(address target, address refund, uint256 amount, string memory note) internal {
        refund;
        note;
        payable(target).call{value: amount}("");
    }
}
"#;
        let main_source = r#"pragma solidity ^0.8.0;
import "./PayLib.sol";

contract Main {
    function pay(address recipient, address refund, uint256 amount, bytes memory note) external payable {
        PayLib.pay(recipient, refund, amount, note);
    }
}
"#;
        fs::write(&helper_path, helper_source).unwrap();
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::new(Some(dir.path().to_path_buf()));
        let get_source = |candidate: &Path| std::fs::read_to_string(candidate).ok();
        let diagnostics = compiler_to_lsp_diagnostics(&index, main_source, &main_path, &get_source);

        let propagated = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(ls_types::NumberOrString::String(
                        USER_CONTROLLED_ETH_TRANSFER_ID.into(),
                    ))
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `pay`")
                    && diagnostic.message.contains("`recipient`")
            })
            .expect("expected common-subset overloaded ETH transfer wrapper finding");

        assert!(!propagated.message.contains("`refund`"));
        let meta: FindingMeta = serde_json::from_value(
            propagated
                .data
                .clone()
                .expect("semantic detector should attach finding metadata"),
        )
        .expect("valid finding metadata");
        assert_eq!(meta.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn test_native_detectors_ignore_ordinary_abi_methods_named_like_primitives() {
        let source = r#"pragma solidity ^0.8.0;
interface Executor {
    function delegatecall(bytes calldata payload) external returns (bool);
    function call(bytes calldata payload) external payable returns (bool);
    function transfer(uint256 amount) external;
    function send(uint256 amount) external returns (bool);
}

contract Main {
    function wrapper(Executor executor, bytes calldata payload) internal {
        executor.delegatecall(payload);
        executor.call{value: 1}(payload);
        executor.transfer(1);
        executor.send(1);
    }

    function run(Executor executor, bytes calldata payload) external {
        wrapper(executor, payload);
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, path, &get_source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic_code(diagnostic),
                Some(
                    UNCHECKED_LOW_LEVEL_CALL_ID
                        | USER_CONTROLLED_DELEGATECALL_ID
                        | USER_CONTROLLED_ETH_TRANSFER_ID
                )
            )
        }));
    }

    #[test]
    fn test_native_detectors_recognize_typed_address_expressions() {
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    mapping(uint256 => address payable) recipients;

    function implementation() internal view returns (address) {
        return msg.sender;
    }

    function run(bytes calldata payload) external {
        msg.sender.call(payload);
        recipients[0].call{value: 1}("");
        implementation().delegatecall(payload);
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, path, &get_source);
        let unchecked = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic_code(diagnostic) == Some(UNCHECKED_LOW_LEVEL_CALL_ID))
            .count();

        assert_eq!(unchecked, 3, "diagnostics: {diagnostics:#?}");
    }

    #[test]
    fn test_zero_value_call_is_not_an_eth_transfer() {
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function run(address payable recipient) external {
        recipient.call{value: 0}("");
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, path, &get_source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(USER_CONTROLLED_ETH_TRANSFER_ID)
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(UNCHECKED_LOW_LEVEL_CALL_ID)
        }));
    }

    #[test]
    fn test_sink_summary_preserves_unnamed_parameter_positions() {
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function sink(uint256, address target, bytes memory payload) internal {
        target.delegatecall(payload);
    }

    function run(uint256 marker, address implementation, bytes memory payload) external {
        sink(marker, implementation, payload);
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, path, &get_source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(USER_CONTROLLED_DELEGATECALL_ID)
                && diagnostic.message.contains("argument `implementation`")
                && diagnostic.message.contains("via `sink`")
        }));
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(USER_CONTROLLED_DELEGATECALL_ID)
                && diagnostic.message.contains("argument `marker`")
        }));
    }

    #[test]
    fn test_named_call_arguments_map_to_declared_parameters() {
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function sink(uint256 marker, address target, bytes memory payload) internal {
        marker;
        target.delegatecall(payload);
    }

    function run(uint256 marker, address implementation, bytes memory payload) external {
        sink({payload: payload, marker: marker, target: implementation});
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, path, &get_source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(USER_CONTROLLED_DELEGATECALL_ID)
                && diagnostic.message.contains("argument `implementation`")
                && diagnostic.message.contains("via `sink`")
        }));
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(USER_CONTROLLED_DELEGATECALL_ID)
                && diagnostic.message.contains("argument `marker`")
        }));
    }

    #[test]
    fn test_safe_same_arity_overload_does_not_inherit_dangerous_summary() {
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function bridge(address target, bytes memory payload) internal {
        target.delegatecall(payload);
    }

    function bridge(bytes memory data, bytes memory payload) internal pure {
        data;
        payload;
    }

    function run(bytes memory data, bytes memory payload) external {
        bridge(data, payload);
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, path, &get_source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(USER_CONTROLLED_DELEGATECALL_ID)
                && diagnostic
                    .message
                    .contains("flows into delegatecall via `bridge`")
        }));
    }

    #[test]
    fn test_safe_override_does_not_inherit_base_summary() {
        let source = r#"pragma solidity ^0.8.0;
contract Base {
    function bridge(address target, bytes memory payload) internal virtual {
        target.delegatecall(payload);
    }
}

contract Main is Base {
    function bridge(address target, bytes memory payload) internal override {
        target;
        payload;
    }

    function run(address implementation, bytes memory payload) external {
        bridge(implementation, payload);
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, path, &get_source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(USER_CONTROLLED_DELEGATECALL_ID)
                && diagnostic
                    .message
                    .contains("flows into delegatecall via `bridge`")
        }));
    }

    #[test]
    fn test_multiple_propagated_sinks_at_same_call_are_preserved() {
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function sink(address first, address second, uint256 amount) internal {
        payable(first).call{value: amount}("");
        payable(second).call{value: amount}("");
    }

    function run(address recipient, address refund, uint256 amount) external {
        sink(recipient, refund, amount);
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, path, &get_source);
        let propagated = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic_code(diagnostic) == Some(USER_CONTROLLED_ETH_TRANSFER_ID)
                    && diagnostic
                        .message
                        .contains("flows into an ETH transfer via `sink`")
            })
            .collect::<Vec<_>>();

        assert_eq!(propagated.len(), 2);
        assert!(propagated
            .iter()
            .any(|diagnostic| diagnostic.message.contains("`recipient`")));
        assert!(propagated
            .iter()
            .any(|diagnostic| diagnostic.message.contains("`refund`")));
    }

    #[test]
    fn test_native_detector_honors_config_and_inline_suppression() {
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function run(address implementation, bytes memory payload) external {
        // solgrid-disable-next-line security/user-controlled-delegatecall
        implementation.delegatecall(payload);
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics_with_config(
            &index,
            source,
            path,
            &get_source,
            &Config::default(),
        );
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(USER_CONTROLLED_DELEGATECALL_ID)
        }));

        let unsuppressed = source.replace(
            "// solgrid-disable-next-line security/user-controlled-delegatecall\n",
            "",
        );
        let mut config = Config::default();
        config.lint.rules.insert(
            USER_CONTROLLED_DELEGATECALL_ID.to_string(),
            solgrid_config::RuleLevel::Off,
        );
        let diagnostics = compiler_to_lsp_diagnostics_with_config(
            &index,
            &unsuppressed,
            path,
            &get_source,
            &config,
        );
        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some(USER_CONTROLLED_DELEGATECALL_ID)
        }));

        config.lint.rules.insert(
            USER_CONTROLLED_DELEGATECALL_ID.to_string(),
            solgrid_config::RuleLevel::Info,
        );
        let diagnostics = compiler_to_lsp_diagnostics_with_config(
            &index,
            &unsuppressed,
            path,
            &get_source,
            &config,
        );
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic_code(diagnostic) == Some(USER_CONTROLLED_DELEGATECALL_ID))
            .expect("configured detector should be emitted");
        assert_eq!(
            diagnostic.severity,
            Some(ls_types::DiagnosticSeverity::INFORMATION)
        );
    }

    #[test]
    fn test_member_resolution_requires_the_expected_symbol_kind() {
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function Missing() internal {}

    function run() external {
        emit Missing();
        revert Missing();
    }
}
"#;
        let path = Path::new("Main.sol");
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        let diagnostics = compiler_to_lsp_diagnostics(&index, source, path, &get_source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some("compiler/unresolved-event")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic_code(diagnostic) == Some("compiler/unresolved-error")
        }));
    }

    #[test]
    fn test_suppress_redundant_diagnostics_drops_overlapping_low_level_call_rule() {
        let source = r#"pragma solidity ^0.8.0;
contract Calls {
    function run(address target, bytes memory payload) external {
        target.call(payload);
    }
}
"#;
        let path = Path::new("Calls.sol");
        let engine = LintEngine::new();
        let mut combined = lint_to_lsp_diagnostics(&engine, source, path, &Config::default());
        let index = ProjectIndex::new(None);
        let get_source = |_candidate: &Path| None;
        combined.extend(compiler_to_lsp_diagnostics(
            &index,
            source,
            path,
            &get_source,
        ));

        let filtered = suppress_redundant_diagnostics(combined);
        assert!(filtered.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    UNCHECKED_LOW_LEVEL_CALL_ID.into(),
                ))
        }));
        assert!(!filtered.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    "security/low-level-calls".into(),
                ))
        }));
    }

    #[test]
    fn test_suppress_redundant_diagnostics_keeps_distinct_semantic_findings() {
        let diagnostics = vec![
            detector_lsp_diagnostic(
                "implementation.delegatecall(payload)",
                USER_CONTROLLED_DELEGATECALL_ID,
                USER_CONTROLLED_DELEGATECALL_TITLE,
                "user-controlled delegatecall".into(),
                0..14,
                Severity::Error,
                Confidence::High,
            ),
            detector_lsp_diagnostic(
                "implementation.delegatecall(payload)",
                UNCHECKED_LOW_LEVEL_CALL_ID,
                UNCHECKED_LOW_LEVEL_CALL_TITLE,
                "unchecked delegatecall".into(),
                15..27,
                Severity::Warning,
                Confidence::High,
            ),
            detector_lsp_diagnostic(
                "implementation.delegatecall(payload)",
                "security/low-level-calls",
                "Low-level calls",
                "avoid low-level calls".into(),
                14..28,
                Severity::Warning,
                Confidence::High,
            ),
        ];

        let filtered = suppress_redundant_diagnostics(diagnostics);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    USER_CONTROLLED_DELEGATECALL_ID.into(),
                ))
        }));
        assert!(filtered.iter().any(|diagnostic| {
            diagnostic.code
                == Some(ls_types::NumberOrString::String(
                    UNCHECKED_LOW_LEVEL_CALL_ID.into(),
                ))
        }));
    }

    #[test]
    fn test_suppress_redundant_diagnostics_drops_overlapping_arbitrary_send_eth_rule() {
        let diagnostics = vec![
            detector_lsp_diagnostic(
                "payable(recipient).call{value: amount}(\"\")",
                USER_CONTROLLED_ETH_TRANSFER_ID,
                USER_CONTROLLED_ETH_TRANSFER_TITLE,
                "user-controlled eth transfer".into(),
                19..23,
                Severity::Warning,
                Confidence::High,
            ),
            detector_lsp_diagnostic(
                "payable(recipient).call{value: amount}(\"\")",
                "security/arbitrary-send-eth",
                "Arbitrary send eth",
                "heuristic eth send".into(),
                18..29,
                Severity::Warning,
                Confidence::High,
            ),
        ];

        let filtered = suppress_redundant_diagnostics(diagnostics);
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].code,
            Some(ls_types::NumberOrString::String(
                USER_CONTROLLED_ETH_TRANSFER_ID.into()
            ))
        );
    }
}
