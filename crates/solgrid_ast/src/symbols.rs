//! Symbol table — build a per-file scope tree from the Solar AST and resolve
//! identifiers to their definitions.

use crate::span_to_range;
use solgrid_parser::solar_ast::{ImportItems, ItemKind, Stmt, StmtKind};
use solgrid_parser::with_parsed_ast_sequential;
use std::ops::Range;

/// Kind of symbol declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Contract,
    Interface,
    Library,
    Function,
    Modifier,
    Event,
    Error,
    Struct,
    StructField,
    Enum,
    Udvt,
    StateVariable,
    LocalVariable,
    Parameter,
    ReturnParameter,
    EnumVariant,
}

/// A single symbol definition.
#[derive(Debug, Clone)]
pub struct SymbolDef {
    pub name: String,
    pub kind: SymbolKind,
    pub name_span: Range<usize>,
    pub def_span: Range<usize>,
    pub scope: Option<ScopeId>,
}

/// An import statement with its path and imported symbols.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub path: String,
    pub path_span: Range<usize>,
    pub symbols: ImportedSymbols,
}

/// How symbols are imported from a file.
#[derive(Debug, Clone)]
pub enum ImportedSymbols {
    Plain(Option<String>),
    Named(Vec<(String, Option<String>)>),
    Glob(String),
}

/// Index into the scope arena.
pub type ScopeId = usize;

#[derive(Debug)]
struct Scope {
    parent: Option<ScopeId>,
    span: Range<usize>,
    symbols: Vec<SymbolDef>,
}

/// Per-file symbol table with nested scopes.
#[derive(Debug)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
    pub imports: Vec<ImportInfo>,
}

impl SymbolTable {
    fn new() -> Self {
        Self {
            scopes: Vec::new(),
            imports: Vec::new(),
        }
    }

    fn push_scope(&mut self, parent: Option<ScopeId>, span: Range<usize>) -> ScopeId {
        let id = self.scopes.len();
        self.scopes.push(Scope {
            parent,
            span,
            symbols: Vec::new(),
        });
        id
    }

    fn add_symbol(&mut self, scope: ScopeId, def: SymbolDef) {
        self.scopes[scope].symbols.push(def);
    }

    /// Find the innermost scope containing `offset`, then walk up looking for `name`.
    pub fn resolve(&self, name: &str, offset: usize) -> Option<&SymbolDef> {
        let mut best: Option<ScopeId> = None;
        for (id, scope) in self.scopes.iter().enumerate() {
            if scope.span.contains(&offset) {
                match best {
                    None => best = Some(id),
                    Some(prev) => {
                        if scope.span.len() < self.scopes[prev].span.len() {
                            best = Some(id);
                        }
                    }
                }
            }
        }

        let mut current = best;
        while let Some(id) = current {
            let scope = &self.scopes[id];
            for symbol in &scope.symbols {
                if symbol.name == name {
                    return Some(symbol);
                }
            }
            current = scope.parent;
        }

        None
    }

    /// Find the innermost function or modifier whose span contains `offset`.
    pub fn find_enclosing_function(&self, offset: usize) -> Option<&SymbolDef> {
        let mut best: Option<&SymbolDef> = None;
        for scope in &self.scopes {
            for symbol in &scope.symbols {
                if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Modifier)
                    && symbol.def_span.contains(&offset)
                {
                    match best {
                        None => best = Some(symbol),
                        Some(prev) if symbol.def_span.len() < prev.def_span.len() => {
                            best = Some(symbol);
                        }
                        _ => {}
                    }
                }
            }
        }
        best
    }

    /// Return all direct symbol definitions in the given scope.
    pub fn scope_symbols(&self, scope_id: ScopeId) -> &[SymbolDef] {
        &self.scopes[scope_id].symbols
    }

    /// Resolve a member inside a container symbol's scope.
    pub fn resolve_member(
        &self,
        container_def: &SymbolDef,
        member_name: &str,
    ) -> Option<&SymbolDef> {
        let scope_id = container_def.scope?;
        let scope = &self.scopes[scope_id];
        scope
            .symbols
            .iter()
            .find(|symbol| symbol.name == member_name)
    }
}

/// Detect a member access pattern at the given offset.
pub fn find_member_access_at_offset(
    source: &str,
    offset: usize,
) -> Option<(String, String, Range<usize>)> {
    let bytes = source.as_bytes();
    let (member_name, member_range) = find_ident_at_offset(source, offset)?;

    let mut pos = member_range.start;
    if pos == 0 {
        return None;
    }
    pos -= 1;
    while pos > 0 && bytes[pos].is_ascii_whitespace() {
        pos -= 1;
    }
    if bytes[pos] != b'.' {
        return None;
    }
    if pos == 0 {
        return None;
    }
    pos -= 1;
    while pos > 0 && bytes[pos].is_ascii_whitespace() {
        pos -= 1;
    }

    let (container_name, _) = find_ident_at_offset(source, pos)?;
    Some((container_name, member_name, member_range))
}

/// Extract the identifier word at a byte offset in source text.
pub fn find_ident_at_offset(source: &str, offset: usize) -> Option<(String, Range<usize>)> {
    let bytes = source.as_bytes();
    if offset >= bytes.len() || !is_ident_char(bytes[offset]) {
        return None;
    }

    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    if bytes[start].is_ascii_digit() {
        return None;
    }

    let mut end = offset + 1;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }

    Some((source[start..end].to_string(), start..end))
}

/// Build a symbol table from Solidity source. Returns `None` on parse error.
pub fn build_symbol_table(source: &str, filename: &str) -> Option<SymbolTable> {
    with_parsed_ast_sequential(source, filename, |source_unit| {
        let mut table = SymbolTable::new();
        let file_scope = table.push_scope(None, 0..source.len());
        for item in source_unit.items.iter() {
            collect_item(&mut table, file_scope, item);
        }
        table
    })
    .ok()
}

fn collect_item(
    table: &mut SymbolTable,
    parent_scope: ScopeId,
    item: &solgrid_parser::solar_ast::Item<'_>,
) {
    use solgrid_parser::solar_ast::{ContractKind, FunctionKind};

    match &item.kind {
        ItemKind::Contract(contract) => {
            let def_span = span_to_range(item.span);
            let kind = match contract.kind {
                ContractKind::Interface => SymbolKind::Interface,
                ContractKind::Library => SymbolKind::Library,
                _ => SymbolKind::Contract,
            };
            let contract_scope = table.push_scope(Some(parent_scope), def_span.clone());
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: contract.name.as_str().to_string(),
                    kind,
                    name_span: span_to_range(contract.name.span),
                    def_span,
                    scope: Some(contract_scope),
                },
            );
            for body_item in contract.body.iter() {
                collect_item(table, contract_scope, body_item);
            }
        }
        ItemKind::Function(func) => {
            let def_span = span_to_range(item.span);
            let func_scope = table.push_scope(Some(parent_scope), def_span.clone());
            if let Some(name) = func.header.name {
                let kind = match func.kind {
                    FunctionKind::Modifier => SymbolKind::Modifier,
                    _ => SymbolKind::Function,
                };
                table.add_symbol(
                    parent_scope,
                    SymbolDef {
                        name: name.as_str().to_string(),
                        kind,
                        name_span: span_to_range(name.span),
                        def_span: def_span.clone(),
                        scope: Some(func_scope),
                    },
                );
            }

            for param in func.header.parameters.iter() {
                if let Some(name) = param.name {
                    table.add_symbol(
                        func_scope,
                        SymbolDef {
                            name: name.as_str().to_string(),
                            kind: SymbolKind::Parameter,
                            name_span: span_to_range(name.span),
                            def_span: span_to_range(param.span),
                            scope: None,
                        },
                    );
                }
            }

            if let Some(returns) = &func.header.returns {
                for param in returns.iter() {
                    if let Some(name) = param.name {
                        table.add_symbol(
                            func_scope,
                            SymbolDef {
                                name: name.as_str().to_string(),
                                kind: SymbolKind::ReturnParameter,
                                name_span: span_to_range(name.span),
                                def_span: span_to_range(param.span),
                                scope: None,
                            },
                        );
                    }
                }
            }

            if let Some(body) = &func.body {
                collect_stmts(table, func_scope, body.stmts);
            }
        }
        ItemKind::Variable(var) => {
            if let Some(name) = var.name {
                table.add_symbol(
                    parent_scope,
                    SymbolDef {
                        name: name.as_str().to_string(),
                        kind: SymbolKind::StateVariable,
                        name_span: span_to_range(name.span),
                        def_span: span_to_range(item.span),
                        scope: None,
                    },
                );
            }
        }
        ItemKind::Event(event) => {
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: event.name.as_str().to_string(),
                    kind: SymbolKind::Event,
                    name_span: span_to_range(event.name.span),
                    def_span: span_to_range(item.span),
                    scope: None,
                },
            );
        }
        ItemKind::Error(error) => {
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: error.name.as_str().to_string(),
                    kind: SymbolKind::Error,
                    name_span: span_to_range(error.name.span),
                    def_span: span_to_range(item.span),
                    scope: None,
                },
            );
        }
        ItemKind::Struct(struct_def) => {
            let def_span = span_to_range(item.span);
            let struct_scope = table.push_scope(Some(parent_scope), def_span.clone());
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: struct_def.name.as_str().to_string(),
                    kind: SymbolKind::Struct,
                    name_span: span_to_range(struct_def.name.span),
                    def_span,
                    scope: Some(struct_scope),
                },
            );
            for field in struct_def.fields.iter() {
                if let Some(name) = field.name {
                    table.add_symbol(
                        struct_scope,
                        SymbolDef {
                            name: name.as_str().to_string(),
                            kind: SymbolKind::StructField,
                            name_span: span_to_range(name.span),
                            def_span: span_to_range(field.span),
                            scope: None,
                        },
                    );
                }
            }
        }
        ItemKind::Enum(enum_def) => {
            let def_span = span_to_range(item.span);
            let enum_scope = table.push_scope(Some(parent_scope), def_span.clone());
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: enum_def.name.as_str().to_string(),
                    kind: SymbolKind::Enum,
                    name_span: span_to_range(enum_def.name.span),
                    def_span,
                    scope: Some(enum_scope),
                },
            );
            for variant in enum_def.variants.iter() {
                table.add_symbol(
                    enum_scope,
                    SymbolDef {
                        name: variant.as_str().to_string(),
                        kind: SymbolKind::EnumVariant,
                        name_span: span_to_range(variant.span),
                        def_span: span_to_range(variant.span),
                        scope: None,
                    },
                );
            }
        }
        ItemKind::Udvt(udvt) => {
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: udvt.name.as_str().to_string(),
                    kind: SymbolKind::Udvt,
                    name_span: span_to_range(udvt.name.span),
                    def_span: span_to_range(item.span),
                    scope: None,
                },
            );
        }
        ItemKind::Import(import) => {
            let symbols = match &import.items {
                ImportItems::Plain(alias) => {
                    ImportedSymbols::Plain(alias.map(|value| value.as_str().to_string()))
                }
                ImportItems::Aliases(aliases) => ImportedSymbols::Named(
                    aliases
                        .iter()
                        .map(|(original, alias)| {
                            (
                                original.as_str().to_string(),
                                alias.map(|value| value.as_str().to_string()),
                            )
                        })
                        .collect(),
                ),
                ImportItems::Glob(alias) => ImportedSymbols::Glob(alias.as_str().to_string()),
            };
            table.imports.push(ImportInfo {
                path: import.path.value.to_string(),
                path_span: span_to_range(import.path.span),
                symbols,
            });
        }
        _ => {}
    }
}

fn collect_stmts(table: &mut SymbolTable, scope: ScopeId, stmts: &[Stmt<'_>]) {
    for stmt in stmts {
        collect_stmt(table, scope, stmt);
    }
}

fn collect_stmt(table: &mut SymbolTable, scope: ScopeId, stmt: &Stmt<'_>) {
    match &stmt.kind {
        StmtKind::DeclSingle(var) => {
            if let Some(name) = var.name {
                table.add_symbol(
                    scope,
                    SymbolDef {
                        name: name.as_str().to_string(),
                        kind: SymbolKind::LocalVariable,
                        name_span: span_to_range(name.span),
                        def_span: span_to_range(var.span),
                        scope: None,
                    },
                );
            }
        }
        StmtKind::DeclMulti(vars, _) => {
            for var in vars.iter() {
                if let solgrid_parser::solar_interface::SpannedOption::Some(var) = var {
                    if let Some(name) = var.name {
                        table.add_symbol(
                            scope,
                            SymbolDef {
                                name: name.as_str().to_string(),
                                kind: SymbolKind::LocalVariable,
                                name_span: span_to_range(name.span),
                                def_span: span_to_range(var.span),
                                scope: None,
                            },
                        );
                    }
                }
            }
        }
        StmtKind::Block(block) => {
            let block_scope = table.push_scope(Some(scope), span_to_range(block.span));
            collect_stmts(table, block_scope, block.stmts);
        }
        StmtKind::UncheckedBlock(block) => {
            let block_scope = table.push_scope(Some(scope), span_to_range(block.span));
            collect_stmts(table, block_scope, block.stmts);
        }
        StmtKind::If(_, then_stmt, else_stmt) => {
            collect_stmt(table, scope, then_stmt);
            if let Some(else_stmt) = else_stmt {
                collect_stmt(table, scope, else_stmt);
            }
        }
        StmtKind::For { init, body, .. } => {
            let for_scope = table.push_scope(Some(scope), span_to_range(stmt.span));
            if let Some(init) = init {
                collect_stmt(table, for_scope, init);
            }
            collect_stmt(table, for_scope, body);
        }
        StmtKind::While(_, body) | StmtKind::DoWhile(body, _) => {
            collect_stmt(table, scope, body);
        }
        StmtKind::Try(try_stmt) => {
            for clause in try_stmt.clauses.iter() {
                let clause_scope = table.push_scope(Some(scope), span_to_range(clause.block.span));
                collect_stmts(table, clause_scope, clause.block.stmts);
            }
        }
        _ => {}
    }
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}
