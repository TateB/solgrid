//! Diagnostics — real-time lint integration for the LSP server.

use crate::convert;
use crate::resolve::ImportResolver;
use solgrid_ast::resolve::ImportResolver as SharedImportResolver;
use solgrid_ast::symbols::{
    self, ImportedSymbols, SignatureData, SymbolDef, SymbolKind, SymbolTable, TypePath, TypeSpec,
};
use solgrid_config::Config;
use solgrid_diagnostics::{Confidence, FileResult, FindingKind, FindingMeta, RuleMeta, Severity};
use solgrid_linter::LintEngine;
use solgrid_parser::solar_ast::{self, Expr, ExprKind, IndexKind, ItemKind, Stmt, StmtKind, Type};
use solgrid_parser::solar_interface::SpannedOption;
use solgrid_parser::with_parsed_ast_sequential;
use solgrid_project::{
    resolve_cross_file_member_symbol, resolve_cross_file_symbol, NavBackend, ProjectIndex,
    ProjectSnapshot,
};
use std::collections::{HashMap, HashSet};
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
                .and_then(|meta| serde_json::to_value(meta.finding_meta(diag.severity)).ok());
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
    let mut diagnostics = unresolved_import_diagnostics(source, path, project_index.resolver());
    let Some(snapshot) = project_index.snapshot_for_source(path, source) else {
        return diagnostics;
    };

    let filename = snapshot.path.to_string_lossy().to_string();
    let mut context =
        CompilerDiagnosticContext::new(&snapshot, project_index.resolver(), get_source);

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
    seen: HashSet<(String, usize, usize)>,
    semantic_files: HashMap<PathBuf, FileSemanticInfo>,
    function_summaries: HashMap<CallableTargetKey, FunctionSinkSummary>,
}

impl<'a> CompilerDiagnosticContext<'a> {
    fn new(
        snapshot: &'a ProjectSnapshot,
        resolver: &'a SharedImportResolver,
        get_source: &'a dyn Fn(&Path) -> Option<String>,
    ) -> Self {
        let (semantic_files, function_summaries) =
            build_function_sink_summaries(snapshot, resolver, get_source);
        Self {
            snapshot,
            resolver,
            get_source,
            diagnostics: Vec::new(),
            seen: HashSet::new(),
            semantic_files,
            function_summaries,
        }
    }

    fn finish(self) -> Vec<ls_types::Diagnostic> {
        self.diagnostics
    }

    fn push(&mut self, id: &str, title: &str, message: String, span: std::ops::Range<usize>) {
        if !self.seen.insert((id.to_string(), span.start, span.end)) {
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
        severity: Severity,
        confidence: Confidence,
    ) {
        if !self.seen.insert((id.to_string(), span.start, span.end)) {
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

                for body_item in contract.body.iter() {
                    self.visit_item(body_item);
                }
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
                    if !self.resolve_ast_path(&modifier.name, modifier_span.start) {
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
                if !self.resolve_ast_path(path, event_span.start) {
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
                if !is_builtin_error_path(path) && !self.resolve_ast_path(path, error_span.start) {
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
        let Some((method, span)) = unchecked_low_level_call_site(expr) else {
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
        let path = TypePath {
            segments: path
                .segments()
                .iter()
                .map(|segment| segment.as_str().to_string())
                .collect(),
        };
        self.resolve_path(&path, resolve_offset)
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
                    call_options_contain_named_arg(options, "value"),
                )
            }
            _ => return None,
        };

        let method_label = match member.as_str() {
            "send" => ".send()",
            "transfer" if args.len() == 1 => ".transfer()",
            "call" if has_value_option => ".call{value: ...}()",
            _ => return None,
        };

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
    expr: &Expr<'_>,
) -> Option<(&'static str, std::ops::Range<usize>)> {
    let ExprKind::Call(callee, _) = &expr.kind else {
        return None;
    };
    let callee = match &callee.kind {
        ExprKind::CallOptions(inner, _) => inner,
        _ => callee,
    };
    let ExprKind::Member(_, member) = &callee.kind else {
        return None;
    };

    let method = match member.as_str() {
        "call" => "call",
        "delegatecall" => "delegatecall",
        "staticcall" => "staticcall",
        _ => return None,
    };

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

fn call_options_contain_named_arg(options: &solar_ast::NamedArgList<'_>, name: &str) -> bool {
    options.iter().any(|arg| arg.name.as_str() == name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}

#[derive(Debug, Clone)]
struct FunctionCallEdge {
    callee: CallableTargetKey,
    argument_parameters: Vec<Option<String>>,
}

#[derive(Debug, Clone, Default)]
struct FunctionSinkSummary {
    parameter_names: Vec<String>,
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
                let Some(callee) = previous.get(&edge.callee) else {
                    continue;
                };

                changed |= propagate_sink_indices(
                    &mut summary.delegatecall_parameters,
                    &callee.delegatecall_parameters,
                    &callee.parameter_names,
                    &edge.argument_parameters,
                    &summary.parameter_names,
                );
                changed |= propagate_sink_indices(
                    &mut summary.eth_transfer_parameters,
                    &callee.eth_transfer_parameters,
                    &callee.parameter_names,
                    &edge.argument_parameters,
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

    let Ok((contracts, callable_contracts, callable_signatures)) =
        with_parsed_ast_sequential(&source, &filename, |source_unit| {
            let mut contracts = HashMap::<String, ContractSemanticInfo>::new();
            let mut callable_contracts = HashMap::<usize, Option<String>>::new();
            let mut callable_signatures = HashMap::<usize, SignatureData>::new();

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
                        callable_contracts.insert(target_offset, None);
                    }
                    _ => {}
                }
            }

            (contracts, callable_contracts, callable_signatures)
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
        },
    );

    for import_path in import_paths {
        let _ = load_semantic_file(&import_path, None, resolver, get_source, semantic_files);
    }

    true
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
        .filter_map(|parameter| parameter.name.map(|name| name.as_str().to_string()))
        .collect::<Vec<_>>();
    let mut summary = FunctionSinkSummary {
        parameter_names: parameter_names.clone(),
        ..FunctionSinkSummary::default()
    };
    let parameter_indexes = parameter_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
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
            if let Some((parameter_name, _)) = delegatecall_target_identifier_from_sink_expr(expr) {
                if let Some(index) = parameter_indexes.get(&parameter_name) {
                    summary.delegatecall_parameters.insert(*index);
                }
            }
            if let Some((parameter_name, _, _)) =
                eth_transfer_target_identifier_from_sink_expr(expr)
            {
                if let Some(index) = parameter_indexes.get(&parameter_name) {
                    summary.eth_transfer_parameters.insert(*index);
                }
            }
            if let Some(callee) =
                resolved_callable_target(file, current_contract, callee, args.len(), context)
            {
                summary.call_edges.push(FunctionCallEdge {
                    callee,
                    argument_parameters: args.exprs().map(parameter_name_for_expr).collect(),
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
            if let Some(target) =
                resolved_callable_target(file, current_contract, callee, args.len(), context)
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
            if let Some(target) =
                resolved_callable_target(file, current_contract, callee, args.len(), context)
            {
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

fn resolved_callable_target(
    file: &FileSemanticInfo,
    current_contract: Option<&str>,
    callee: &Expr<'_>,
    arg_count: usize,
    context: &SinkSummaryContext<'_>,
) -> Option<CallableTargetKey> {
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
            if let Some(target) = unique_callable_target(defs) {
                return Some(target);
            }

            if let Some(contract_name) = current_contract {
                if let Some(target) = resolve_named_contract_callable_target(
                    file,
                    contract_name,
                    ident.as_str(),
                    arg_count,
                    context,
                    false,
                ) {
                    return Some(target);
                }
            }

            let cached_source = |candidate: &Path| {
                cached_source(candidate, context.semantic_files, context.get_source)
            };
            let cross = resolve_cross_file_symbol(
                &file.table,
                ident.as_str(),
                &file.path,
                &cached_source,
                context.resolver,
            )?;
            is_callable_symbol(&cross.def, arg_count).then_some(CallableTargetKey {
                path: cross.resolved_path,
                offset: cross.def.name_span.start,
            })
        }
        ExprKind::Member(base, member) => {
            let base = base.peel_parens();
            if let ExprKind::Ident(container) = &base.kind {
                let container_name = container.as_str();
                if container_name == "super" {
                    let contract_name = current_contract?;
                    return resolve_named_contract_callable_target(
                        file,
                        contract_name,
                        member.as_str(),
                        arg_count,
                        context,
                        true,
                    );
                }
                if container_name == "this" {
                    let contract_name = current_contract?;
                    return resolve_named_contract_callable_target(
                        file,
                        contract_name,
                        member.as_str(),
                        arg_count,
                        context,
                        false,
                    );
                }
                if file.contracts.contains_key(container_name) {
                    return resolve_contract_callable_target(
                        file,
                        container_name,
                        member.as_str(),
                        arg_count,
                        context,
                    );
                }

                let offset = solgrid_ast::span_to_range(container.span).start;
                let defs = file
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
                if let Some(target) = unique_callable_target(defs) {
                    return Some(target);
                }

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
                                if let Some(target) = resolve_contract_callable_target(
                                    contract_file,
                                    &contract_name,
                                    member.as_str(),
                                    arg_count,
                                    context,
                                ) {
                                    return Some(target);
                                }
                            }
                        }
                    }
                }

                let cached_source = |candidate: &Path| {
                    cached_source(candidate, context.semantic_files, context.get_source)
                };
                let cross = resolve_cross_file_member_symbol(
                    &file.table,
                    container_name,
                    member.as_str(),
                    &file.path,
                    &cached_source,
                    context.resolver,
                )?;
                is_callable_symbol(&cross.def, arg_count).then_some(CallableTargetKey {
                    path: cross.resolved_path,
                    offset: cross.def.name_span.start,
                })
            } else {
                let candidates =
                    resolve_contract_targets_from_expr(file, current_contract, base, context)
                        .into_iter()
                        .filter_map(|(path, contract_name)| {
                            let contract_file = context.semantic_files.get(&path)?;
                            resolve_contract_callable_target(
                                contract_file,
                                &contract_name,
                                member.as_str(),
                                arg_count,
                                context,
                            )
                        })
                        .collect::<Vec<_>>();
                unique_callable_target(candidates)
            }
        }
        _ => None,
    }
}

fn resolve_named_contract_callable_target(
    file: &FileSemanticInfo,
    contract_name: &str,
    callable_name: &str,
    arg_count: usize,
    context: &SinkSummaryContext<'_>,
    super_only: bool,
) -> Option<CallableTargetKey> {
    let mut visited = HashSet::new();
    let candidates = resolve_contract_callable_targets(
        file,
        contract_name,
        callable_name,
        arg_count,
        context,
        !super_only,
        &mut visited,
    );
    unique_callable_target(candidates)
}

fn resolve_contract_callable_target(
    file: &FileSemanticInfo,
    contract_name: &str,
    callable_name: &str,
    arg_count: usize,
    context: &SinkSummaryContext<'_>,
) -> Option<CallableTargetKey> {
    let mut visited = HashSet::new();
    let candidates = resolve_contract_callable_targets(
        file,
        contract_name,
        callable_name,
        arg_count,
        context,
        true,
        &mut visited,
    );
    unique_callable_target(candidates)
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
    if include_current {
        if let Some(callables) = contract.callables.get(callable_name) {
            candidates.extend(
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

fn unique_callable_target(candidates: Vec<CallableTargetKey>) -> Option<CallableTargetKey> {
    let mut unique = candidates;
    unique.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.offset.cmp(&right.offset))
    });
    unique.dedup();
    (unique.len() == 1).then(|| unique.remove(0))
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

fn propagate_sink_indices(
    destinations: &mut HashSet<usize>,
    callee_sinks: &HashSet<usize>,
    callee_parameter_names: &[String],
    argument_parameters: &[Option<String>],
    current_parameter_names: &[String],
) -> bool {
    let mut changed = false;
    for callee_index in callee_sinks {
        let Some(parameter_name) = callee_parameter_names.get(*callee_index) else {
            continue;
        };
        let Some(Some(argument_name)) = argument_parameters.get(*callee_index) else {
            continue;
        };
        let Some(current_index) = current_parameter_names
            .iter()
            .position(|name| name == argument_name)
        else {
            continue;
        };
        if destinations.insert(current_index) {
            changed = true;
        }
        if parameter_name == argument_name && destinations.insert(current_index) {
            changed = true;
        }
    }
    changed
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
    let callee =
        resolved_callable_target(file, current_contract, callee_expr, args.len(), &context)?;
    let summary = summaries.get(&callee)?;
    if summary.delegatecall_parameters.is_empty() && summary.eth_transfer_parameters.is_empty() {
        return None;
    }
    let (call_name, call_span) = call_site_label_and_span(callee_expr)?;
    let mut propagated = Vec::new();
    for index in &summary.delegatecall_parameters {
        let Some(argument) = args.exprs().nth(*index) else {
            continue;
        };
        if let Some(parameter_name) = resolved_parameter_name(file, argument) {
            propagated.push((parameter_name, SinkKind::Delegatecall));
        }
    }
    for index in &summary.eth_transfer_parameters {
        let Some(argument) = args.exprs().nth(*index) else {
            continue;
        };
        if let Some(parameter_name) = resolved_parameter_name(file, argument) {
            propagated.push((parameter_name, SinkKind::EthTransfer));
        }
    }
    (!propagated.is_empty()).then_some((call_span, call_name, propagated))
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
    (member.as_str() == "delegatecall")
        .then(|| delegatecall_target_identifier(base))
        .flatten()
}

fn eth_transfer_target_identifier_from_sink_expr(
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
                call_options_contain_named_arg(options, "value"),
            )
        }
        _ => return None,
    };

    let method_label = match member.as_str() {
        "send" => ".send()",
        "transfer" if args.len() == 1 => ".transfer()",
        "call" if has_value_option => ".call{value: ...}()",
        _ => return None,
    };
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
