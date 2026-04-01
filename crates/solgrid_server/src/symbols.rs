//! Symbol table — build a per-file scope tree from the Solar AST
//! and resolve identifiers to their definitions.

use solgrid_parser::solar_ast::{
    DataLocation, FunctionKind, ImportItems, ItemFunction, ItemKind, Stmt, StmtKind, Type,
    TypeKind, VariableDefinition,
};
use solgrid_parser::solar_interface::SpannedOption;
use solgrid_parser::with_parsed_ast_sequential;
use std::collections::HashSet;
use std::ops::Range;

/// Kind of symbol declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Contract,
    Interface,
    Library,
    Constructor,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypePath {
    pub segments: Vec<String>,
}

impl TypePath {
    pub fn as_display(&self) -> String {
        self.segments.join(".")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeSpec {
    Elementary {
        display: String,
    },
    Custom {
        path: TypePath,
        display: String,
        resolve_offset: usize,
    },
    Array {
        element: Box<TypeSpec>,
        display: String,
    },
    Mapping {
        value: Box<TypeSpec>,
        display: String,
    },
    Function {
        display: String,
    },
    Other {
        display: String,
    },
}

impl TypeSpec {
    pub fn display(&self) -> &str {
        match self {
            Self::Elementary { display }
            | Self::Custom { display, .. }
            | Self::Array { display, .. }
            | Self::Mapping { display, .. }
            | Self::Function { display }
            | Self::Other { display } => display,
        }
    }

    pub fn member_target(&self) -> Option<&TypePath> {
        match self {
            Self::Custom { path, .. } => Some(path),
            _ => None,
        }
    }

    pub fn resolve_offset(&self) -> usize {
        match self {
            Self::Custom { resolve_offset, .. } => *resolve_offset,
            Self::Array { element, .. } | Self::Mapping { value: element, .. } => {
                element.resolve_offset()
            }
            _ => 0,
        }
    }

    pub fn index_result(&self) -> Option<&TypeSpec> {
        match self {
            Self::Array { element, .. } | Self::Mapping { value: element, .. } => Some(element),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureParam {
    pub label: String,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureData {
    pub label: String,
    pub parameters: Vec<SignatureParam>,
    pub return_types: Vec<TypeSpec>,
    pub first_return_type: Option<TypeSpec>,
}

/// A single symbol definition.
#[derive(Debug, Clone)]
pub struct SymbolDef {
    pub name: String,
    pub kind: SymbolKind,
    /// Byte range of the name identifier in the source.
    pub name_span: Range<usize>,
    /// Byte range of the full definition item in the source.
    pub def_span: Range<usize>,
    /// If this symbol creates a child scope (contract, struct, enum), the scope id.
    pub scope: Option<ScopeId>,
    /// The declared type for values such as variables and parameters.
    pub type_info: Option<TypeSpec>,
    /// Callable signature metadata for functions and modifiers.
    pub signature: Option<SignatureData>,
}

/// An import statement with its path and imported symbols.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The raw import path string (e.g., `"./Token.sol"` or `"@openzeppelin/contracts/..."`).
    pub path: String,
    /// Byte range of the import path literal in source.
    pub path_span: Range<usize>,
    /// Which symbols are imported.
    pub symbols: ImportedSymbols,
}

/// How symbols are imported from a file.
#[derive(Debug, Clone)]
pub enum ImportedSymbols {
    /// `import "file.sol"` or `import "file.sol" as Alias`.
    Plain(Option<String>),
    /// `import {Foo, Bar as Baz} from "file.sol"` — `(original, optional alias)`.
    Named(Vec<(String, Option<String>)>),
    /// `import * as Alias from "file.sol"`.
    Glob(String),
}

/// Index into the scope arena.
pub type ScopeId = usize;

/// A lexical scope containing symbol definitions.
#[derive(Debug, Clone)]
struct Scope {
    parent: Option<ScopeId>,
    /// Byte range this scope covers in the source.
    span: Range<usize>,
    symbols: Vec<SymbolDef>,
}

/// Per-file symbol table with nested scopes.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
    /// Import statements found in the file.
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
        self.resolve_all(name, offset).into_iter().next()
    }

    /// Find every definition visible for `name` at `offset`.
    pub fn resolve_all(&self, name: &str, offset: usize) -> Vec<&SymbolDef> {
        // Find the innermost scope containing the offset.
        let mut best: Option<ScopeId> = None;
        for (id, scope) in self.scopes.iter().enumerate() {
            if scope.span.contains(&offset) {
                match best {
                    None => best = Some(id),
                    Some(prev) => {
                        // Prefer narrower scope.
                        let prev_len = self.scopes[prev].span.len();
                        if scope.span.len() < prev_len {
                            best = Some(id);
                        }
                    }
                }
            }
        }

        // Walk up the scope chain.
        let mut current = best;
        let mut results = Vec::new();
        while let Some(id) = current {
            let scope = &self.scopes[id];
            for sym in &scope.symbols {
                if sym.name == name {
                    results.push(sym);
                }
            }
            current = scope.parent;
        }

        results
    }

    /// Find the innermost function or modifier whose def_span contains `offset`.
    pub fn find_enclosing_function(&self, offset: usize) -> Option<&SymbolDef> {
        let mut best: Option<&SymbolDef> = None;
        for scope in &self.scopes {
            for sym in &scope.symbols {
                if matches!(
                    sym.kind,
                    SymbolKind::Constructor | SymbolKind::Function | SymbolKind::Modifier
                ) && sym.def_span.contains(&offset)
                {
                    match best {
                        None => best = Some(sym),
                        Some(prev) if sym.def_span.len() < prev.def_span.len() => {
                            best = Some(sym);
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

    /// Return all symbols defined in the file-level scope (scope 0).
    ///
    /// These are the symbols that can be exported/imported from the file:
    /// contracts, interfaces, libraries, free functions, errors, events,
    /// structs, enums, UDVTs.
    pub fn file_level_symbols(&self) -> &[SymbolDef] {
        if self.scopes.is_empty() {
            return &[];
        }
        &self.scopes[0].symbols
    }

    /// Collect all symbols visible at the given byte offset.
    ///
    /// Finds the innermost scope containing `offset`, then walks up the scope
    /// chain collecting all symbols. When multiple symbols share the same name,
    /// the innermost (shadowing) definition wins.
    pub fn visible_symbols_at(&self, offset: usize) -> Vec<&SymbolDef> {
        // Find the innermost scope containing the offset.
        let mut best: Option<ScopeId> = None;
        for (id, scope) in self.scopes.iter().enumerate() {
            if scope.span.contains(&offset) {
                match best {
                    None => best = Some(id),
                    Some(prev) => {
                        if self.scopes[prev].span.len() > scope.span.len() {
                            best = Some(id);
                        }
                    }
                }
            }
        }

        let mut seen = HashSet::new();
        let mut result = Vec::new();

        // Walk up the scope chain, collecting symbols.
        let mut current = best;
        while let Some(id) = current {
            let scope = &self.scopes[id];
            for sym in &scope.symbols {
                if seen.insert(&sym.name) {
                    result.push(sym);
                }
            }
            current = scope.parent;
        }

        result
    }

    /// Resolve a member inside a container symbol's scope.
    ///
    /// E.g., resolve `someFunction` inside `MyContract`'s scope.
    pub fn resolve_member(
        &self,
        container_def: &SymbolDef,
        member_name: &str,
    ) -> Option<&SymbolDef> {
        self.resolve_member_all(container_def, member_name)
            .into_iter()
            .next()
    }

    /// Resolve all members inside a container symbol's scope.
    pub fn resolve_member_all(
        &self,
        container_def: &SymbolDef,
        member_name: &str,
    ) -> Vec<&SymbolDef> {
        let Some(scope_id) = container_def.scope else {
            return Vec::new();
        };
        let scope = &self.scopes[scope_id];
        scope
            .symbols
            .iter()
            .filter(|s| s.name == member_name)
            .collect()
    }

    /// Return constructor definitions attached to a container scope.
    pub fn constructors(&self, container_def: &SymbolDef) -> Vec<&SymbolDef> {
        let Some(scope_id) = container_def.scope else {
            return Vec::new();
        };
        self.scopes[scope_id]
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Constructor)
            .collect()
    }
}

/// Detect a member access pattern at the given offset.
///
/// If the cursor is on `bar` in `foo.bar`, returns `(container_name, member_name, member_ident_range)`.
pub fn find_member_access_at_offset(
    source: &str,
    offset: usize,
) -> Option<(String, String, Range<usize>)> {
    let bytes = source.as_bytes();
    let (member_name, member_range) = find_ident_at_offset(source, offset)?;

    // Scan backward from the member start to find a dot.
    let mut pos = member_range.start;
    if pos == 0 {
        return None;
    }
    pos -= 1;
    // Skip whitespace (Solidity doesn't allow it, but be tolerant).
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
    // Extract the container identifier ending at pos.
    let (container_name, _) = find_ident_at_offset(source, pos)?;

    Some((container_name, member_name, member_range))
}

/// Extract the identifier word at a byte offset in source text.
///
/// Returns `(name, byte_range)` if the offset is on an identifier.
pub fn find_ident_at_offset(source: &str, offset: usize) -> Option<(String, Range<usize>)> {
    let bytes = source.as_bytes();
    if offset >= bytes.len() {
        return None;
    }

    // Must be on an identifier character.
    if !is_ident_char(bytes[offset]) {
        return None;
    }

    // Scan backwards to find start.
    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    // First char must be letter or underscore (not digit).
    if bytes[start].is_ascii_digit() {
        return None;
    }

    // Scan forwards to find end.
    let mut end = offset + 1;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }

    let name = source[start..end].to_string();
    Some((name, start..end))
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

pub(crate) fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_was_ws = false;
    for ch in text.trim().chars() {
        if ch.is_whitespace() {
            if !prev_was_ws {
                result.push(' ');
                prev_was_ws = true;
            }
        } else {
            result.push(ch);
            prev_was_ws = false;
        }
    }
    result
}

pub(crate) fn display_for_type(
    source: &str,
    ty: &Type<'_>,
    data_location: Option<DataLocation>,
) -> String {
    let mut display = normalize_whitespace(&source[solgrid_ast::span_to_range(ty.span)]);
    if let Some(location) = data_location {
        display.push(' ');
        display.push_str(&location.to_string());
    }
    display
}

pub(crate) fn type_spec_from_ast(
    source: &str,
    ty: &Type<'_>,
    data_location: Option<DataLocation>,
    resolve_offset: usize,
) -> TypeSpec {
    let display = display_for_type(source, ty, data_location);
    match &ty.kind {
        TypeKind::Custom(path) => TypeSpec::Custom {
            path: TypePath {
                segments: path
                    .segments()
                    .iter()
                    .map(|segment| segment.as_str().to_string())
                    .collect(),
            },
            display,
            resolve_offset,
        },
        TypeKind::Array(array) => TypeSpec::Array {
            element: Box::new(type_spec_from_ast(
                source,
                &array.element,
                None,
                resolve_offset,
            )),
            display,
        },
        TypeKind::Mapping(mapping) => TypeSpec::Mapping {
            value: Box::new(type_spec_from_ast(
                source,
                &mapping.value,
                None,
                resolve_offset,
            )),
            display,
        },
        TypeKind::Function(_) => TypeSpec::Function { display },
        TypeKind::Elementary(_) => TypeSpec::Elementary { display },
    }
}

fn parameter_label(source: &str, param: &VariableDefinition<'_>) -> String {
    normalize_whitespace(&source[solgrid_ast::span_to_range(param.span)])
}

fn signature_data_for_function(source: &str, func: &ItemFunction<'_>) -> SignatureData {
    let label = normalize_whitespace(&source[solgrid_ast::span_to_range(func.header.span)]);
    let param_labels: Vec<String> = func
        .header
        .parameters
        .iter()
        .map(|param| parameter_label(source, param))
        .collect();
    let parameters = map_parameter_offsets(&label, &param_labels);
    let return_types: Vec<TypeSpec> = func
        .header
        .returns()
        .iter()
        .map(|param| {
            type_spec_from_ast(
                source,
                &param.ty,
                param.data_location,
                solgrid_ast::span_to_range(func.header.span).start,
            )
        })
        .collect();

    SignatureData {
        label,
        parameters,
        first_return_type: return_types.first().cloned(),
        return_types,
    }
}

fn map_parameter_offsets(label: &str, param_labels: &[String]) -> Vec<SignatureParam> {
    let mut search_start = 0usize;
    let mut parameters = Vec::with_capacity(param_labels.len());

    for param_label in param_labels {
        let found = label[search_start..]
            .find(param_label)
            .map(|relative| search_start + relative)
            .or_else(|| label.find(param_label));
        let Some(start) = found else {
            continue;
        };
        let end = start + param_label.len();
        parameters.push(SignatureParam {
            label: param_label.clone(),
            start: start as u32,
            end: end as u32,
        });
        search_start = end;
    }

    parameters
}

/// Build a symbol table from Solidity source. Returns `None` on parse error.
pub fn build_symbol_table(source: &str, filename: &str) -> Option<SymbolTable> {
    with_parsed_ast_sequential(source, filename, |source_unit| {
        let mut table = SymbolTable::new();
        let file_span = 0..source.len();
        let file_scope = table.push_scope(None, file_span);

        for item in source_unit.items.iter() {
            collect_item(&mut table, file_scope, source, item);
        }

        table
    })
    .ok()
}

fn collect_item(
    table: &mut SymbolTable,
    parent_scope: ScopeId,
    source: &str,
    item: &solgrid_parser::solar_ast::Item<'_>,
) {
    match &item.kind {
        ItemKind::Contract(contract) => {
            let kind = match contract.kind {
                solgrid_parser::solar_ast::ContractKind::Interface => SymbolKind::Interface,
                solgrid_parser::solar_ast::ContractKind::Library => SymbolKind::Library,
                _ => SymbolKind::Contract,
            };

            let name_span = solgrid_ast::span_to_range(contract.name.span);
            let def_span = solgrid_ast::span_to_range(item.span);
            let contract_scope = table.push_scope(Some(parent_scope), def_span.clone());
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: contract.name.as_str().to_string(),
                    kind,
                    name_span,
                    def_span,
                    scope: Some(contract_scope),
                    type_info: None,
                    signature: None,
                },
            );

            for body_item in contract.body.iter() {
                collect_item(table, contract_scope, source, body_item);
            }
        }

        ItemKind::Function(func) => {
            let def_span = solgrid_ast::span_to_range(item.span);
            let func_scope = table.push_scope(Some(parent_scope), def_span.clone());
            let signature = signature_data_for_function(source, func);
            if let Some(name_ident) = func.header.name {
                let kind = match func.kind {
                    FunctionKind::Modifier => SymbolKind::Modifier,
                    _ => SymbolKind::Function,
                };
                let name_span = solgrid_ast::span_to_range(name_ident.span);
                table.add_symbol(
                    parent_scope,
                    SymbolDef {
                        name: name_ident.as_str().to_string(),
                        kind,
                        name_span,
                        def_span: def_span.clone(),
                        scope: Some(func_scope),
                        type_info: None,
                        signature: Some(signature.clone()),
                    },
                );
            } else if func.kind == FunctionKind::Constructor {
                let header_start = solgrid_ast::span_to_range(func.header.span).start;
                table.add_symbol(
                    parent_scope,
                    SymbolDef {
                        name: "<constructor>".to_string(),
                        kind: SymbolKind::Constructor,
                        name_span: header_start..header_start,
                        def_span: def_span.clone(),
                        scope: Some(func_scope),
                        type_info: None,
                        signature: Some(signature.clone()),
                    },
                );
            }

            // Register parameters.
            for param in func.header.parameters.iter() {
                if let Some(name_ident) = param.name {
                    let name_span = solgrid_ast::span_to_range(name_ident.span);
                    let param_def_span = solgrid_ast::span_to_range(param.span);
                    table.add_symbol(
                        func_scope,
                        SymbolDef {
                            name: name_ident.as_str().to_string(),
                            kind: SymbolKind::Parameter,
                            name_span,
                            def_span: param_def_span.clone(),
                            scope: None,
                            type_info: Some(type_spec_from_ast(
                                source,
                                &param.ty,
                                param.data_location,
                                param_def_span.start,
                            )),
                            signature: None,
                        },
                    );
                }
            }

            // Register return parameters.
            if let Some(returns) = &func.header.returns {
                for param in returns.iter() {
                    if let Some(name_ident) = param.name {
                        let name_span = solgrid_ast::span_to_range(name_ident.span);
                        let param_def_span = solgrid_ast::span_to_range(param.span);
                        table.add_symbol(
                            func_scope,
                            SymbolDef {
                                name: name_ident.as_str().to_string(),
                                kind: SymbolKind::ReturnParameter,
                                name_span,
                                def_span: param_def_span.clone(),
                                scope: None,
                                type_info: Some(type_spec_from_ast(
                                    source,
                                    &param.ty,
                                    param.data_location,
                                    param_def_span.start,
                                )),
                                signature: None,
                            },
                        );
                    }
                }
            }

            // Walk function body for local variables.
            if let Some(body) = &func.body {
                collect_stmts(table, func_scope, source, body.stmts);
            }
        }

        ItemKind::Variable(var) => {
            if let Some(name_ident) = var.name {
                let name_span = solgrid_ast::span_to_range(name_ident.span);
                table.add_symbol(
                    parent_scope,
                    SymbolDef {
                        name: name_ident.as_str().to_string(),
                        kind: SymbolKind::StateVariable,
                        name_span,
                        def_span: solgrid_ast::span_to_range(item.span),
                        scope: None,
                        type_info: Some(type_spec_from_ast(
                            source,
                            &var.ty,
                            var.data_location,
                            solgrid_ast::span_to_range(item.span).start,
                        )),
                        signature: None,
                    },
                );
            }
        }

        ItemKind::Event(ev) => {
            let name_span = solgrid_ast::span_to_range(ev.name.span);
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: ev.name.as_str().to_string(),
                    kind: SymbolKind::Event,
                    name_span,
                    def_span: solgrid_ast::span_to_range(item.span),
                    scope: None,
                    type_info: None,
                    signature: None,
                },
            );
        }

        ItemKind::Error(err) => {
            let name_span = solgrid_ast::span_to_range(err.name.span);
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: err.name.as_str().to_string(),
                    kind: SymbolKind::Error,
                    name_span,
                    def_span: solgrid_ast::span_to_range(item.span),
                    scope: None,
                    type_info: None,
                    signature: None,
                },
            );
        }

        ItemKind::Struct(s) => {
            let name_span = solgrid_ast::span_to_range(s.name.span);
            let struct_span = solgrid_ast::span_to_range(item.span);
            let struct_scope = table.push_scope(Some(parent_scope), struct_span.clone());
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: s.name.as_str().to_string(),
                    kind: SymbolKind::Struct,
                    name_span,
                    def_span: struct_span,
                    scope: Some(struct_scope),
                    type_info: None,
                    signature: None,
                },
            );
            // Register struct fields.
            for field in s.fields.iter() {
                if let Some(name_ident) = field.name {
                    let f_name_span = solgrid_ast::span_to_range(name_ident.span);
                    let f_def_span = solgrid_ast::span_to_range(field.span);
                    table.add_symbol(
                        struct_scope,
                        SymbolDef {
                            name: name_ident.as_str().to_string(),
                            kind: SymbolKind::StructField,
                            name_span: f_name_span,
                            def_span: f_def_span.clone(),
                            scope: None,
                            type_info: Some(type_spec_from_ast(
                                source,
                                &field.ty,
                                field.data_location,
                                f_def_span.start,
                            )),
                            signature: None,
                        },
                    );
                }
            }
        }

        ItemKind::Enum(e) => {
            let name_span = solgrid_ast::span_to_range(e.name.span);
            let enum_span = solgrid_ast::span_to_range(item.span);
            let enum_scope = table.push_scope(Some(parent_scope), enum_span.clone());
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: e.name.as_str().to_string(),
                    kind: SymbolKind::Enum,
                    name_span,
                    def_span: enum_span,
                    scope: Some(enum_scope),
                    type_info: None,
                    signature: None,
                },
            );

            // Register enum variants.
            for variant in e.variants.iter() {
                let v_span = solgrid_ast::span_to_range(variant.span);
                table.add_symbol(
                    enum_scope,
                    SymbolDef {
                        name: variant.as_str().to_string(),
                        kind: SymbolKind::EnumVariant,
                        name_span: v_span.clone(),
                        def_span: v_span,
                        scope: None,
                        type_info: None,
                        signature: None,
                    },
                );
            }
        }

        ItemKind::Udvt(u) => {
            let name_span = solgrid_ast::span_to_range(u.name.span);
            table.add_symbol(
                parent_scope,
                SymbolDef {
                    name: u.name.as_str().to_string(),
                    kind: SymbolKind::Udvt,
                    name_span,
                    def_span: solgrid_ast::span_to_range(item.span),
                    scope: None,
                    type_info: Some(type_spec_from_ast(
                        source,
                        &u.ty,
                        None,
                        solgrid_ast::span_to_range(item.span).start,
                    )),
                    signature: None,
                },
            );
        }

        ItemKind::Import(import) => {
            let path = import.path.value.as_str().to_string();
            let path_span = solgrid_ast::span_to_range(import.path.span);
            let symbols = match &import.items {
                ImportItems::Plain(alias) => {
                    ImportedSymbols::Plain(alias.map(|a| a.as_str().to_string()))
                }
                ImportItems::Aliases(aliases) => ImportedSymbols::Named(
                    aliases
                        .iter()
                        .map(|(name, alias)| {
                            (
                                name.as_str().to_string(),
                                alias.map(|a| a.as_str().to_string()),
                            )
                        })
                        .collect(),
                ),
                ImportItems::Glob(alias) => ImportedSymbols::Glob(alias.as_str().to_string()),
            };
            table.imports.push(ImportInfo {
                path,
                path_span,
                symbols,
            });
        }

        // Pragma, Using — skip.
        _ => {}
    }
}

fn collect_stmts(table: &mut SymbolTable, scope: ScopeId, source: &str, stmts: &[Stmt<'_>]) {
    for stmt in stmts {
        collect_stmt(table, scope, source, stmt);
    }
}

fn collect_stmt(table: &mut SymbolTable, scope: ScopeId, source: &str, stmt: &Stmt<'_>) {
    match &stmt.kind {
        StmtKind::DeclSingle(var_def) => {
            if let Some(name_ident) = var_def.name {
                let name_span = solgrid_ast::span_to_range(name_ident.span);
                table.add_symbol(
                    scope,
                    SymbolDef {
                        name: name_ident.as_str().to_string(),
                        kind: SymbolKind::LocalVariable,
                        name_span,
                        def_span: solgrid_ast::span_to_range(stmt.span),
                        scope: None,
                        type_info: Some(type_spec_from_ast(
                            source,
                            &var_def.ty,
                            var_def.data_location,
                            solgrid_ast::span_to_range(stmt.span).start,
                        )),
                        signature: None,
                    },
                );
            }
        }

        StmtKind::DeclMulti(var_defs, _) => {
            for decl in var_defs.iter() {
                if let SpannedOption::Some(var) = decl {
                    if let Some(name_ident) = var.name {
                        let name_span = solgrid_ast::span_to_range(name_ident.span);
                        table.add_symbol(
                            scope,
                            SymbolDef {
                                name: name_ident.as_str().to_string(),
                                kind: SymbolKind::LocalVariable,
                                name_span,
                                def_span: solgrid_ast::span_to_range(var.span),
                                scope: None,
                                type_info: Some(type_spec_from_ast(
                                    source,
                                    &var.ty,
                                    var.data_location,
                                    solgrid_ast::span_to_range(var.span).start,
                                )),
                                signature: None,
                            },
                        );
                    }
                }
            }
        }

        StmtKind::Block(block) => {
            let block_span = solgrid_ast::span_to_range(stmt.span);
            let block_scope = table.push_scope(Some(scope), block_span);
            collect_stmts(table, block_scope, source, block.stmts);
        }

        StmtKind::UncheckedBlock(block) => {
            let block_span = solgrid_ast::span_to_range(stmt.span);
            let block_scope = table.push_scope(Some(scope), block_span);
            collect_stmts(table, block_scope, source, block.stmts);
        }

        StmtKind::If(_, then_stmt, else_stmt) => {
            collect_stmt(table, scope, source, then_stmt);
            if let Some(else_s) = else_stmt {
                collect_stmt(table, scope, source, else_s);
            }
        }

        StmtKind::For { init, body, .. } => {
            let for_span = solgrid_ast::span_to_range(stmt.span);
            let for_scope = table.push_scope(Some(scope), for_span);
            if let Some(init_stmt) = init {
                collect_stmt(table, for_scope, source, init_stmt);
            }
            collect_stmt(table, for_scope, source, body);
        }

        StmtKind::While(_, body) => {
            collect_stmt(table, scope, source, body);
        }

        StmtKind::DoWhile(body, _) => {
            collect_stmt(table, scope, source, body);
        }

        StmtKind::Try(try_stmt) => {
            for clause in try_stmt.clauses.iter() {
                let clause_span = solgrid_ast::span_to_range(clause.block.span);
                let clause_scope = table.push_scope(Some(scope), clause_span);
                collect_stmts(table, clause_scope, source, clause.block.stmts);
            }
        }

        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_for(source: &str) -> SymbolTable {
        build_symbol_table(source, "test.sol").expect("parse failed")
    }

    #[test]
    fn test_find_ident_at_offset() {
        let source = "uint256 myVar = 42;";
        // On 'm' of myVar (offset 8)
        let (name, range) = find_ident_at_offset(source, 8).unwrap();
        assert_eq!(name, "myVar");
        assert_eq!(range, 8..13);

        // On a digit — not an identifier start
        assert!(find_ident_at_offset(source, 16).is_none()); // on '4'

        // On space
        assert!(find_ident_at_offset(source, 7).is_none());
    }

    #[test]
    fn test_contract_and_function() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MyContract {
    function foo() public {}
    function bar(uint256 x) public returns (uint256 y) {}
}
"#;
        let table = table_for(source);

        // Contract should be findable.
        let def = table.resolve("MyContract", 80).unwrap();
        assert_eq!(def.kind, SymbolKind::Contract);

        // Functions should be findable inside the contract.
        let def = table.resolve("foo", 80).unwrap();
        assert_eq!(def.kind, SymbolKind::Function);

        let def = table.resolve("bar", 120).unwrap();
        assert_eq!(def.kind, SymbolKind::Function);
    }

    #[test]
    fn test_state_variable() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256 public value;
    function get() public view returns (uint256) {
        return value;
    }
}
"#;
        let table = table_for(source);

        // Inside the get() function body, `value` should resolve to the state variable.
        // Find offset inside function body.
        let offset = source.find("return value").unwrap() + 7; // on 'v' of value
        let def = table.resolve("value", offset).unwrap();
        assert_eq!(def.kind, SymbolKind::StateVariable);
    }

    #[test]
    fn test_local_variable_shadows_state() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    uint256 public x;
    function foo() public {
        uint256 x = 1;
        uint256 y = x;
    }
}
"#;
        let table = table_for(source);

        // Inside foo(), `x` should resolve to the local variable.
        let offset = source.find("y = x").unwrap() + 4; // on 'x' in `y = x`
        let def = table.resolve("x", offset).unwrap();
        assert_eq!(def.kind, SymbolKind::LocalVariable);
    }

    #[test]
    fn test_function_parameters() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return a + b;
    }
}
"#;
        let table = table_for(source);

        let offset = source.find("return a").unwrap() + 7; // on 'a'
        let def = table.resolve("a", offset).unwrap();
        assert_eq!(def.kind, SymbolKind::Parameter);
    }

    #[test]
    fn test_return_parameters() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function add(uint256 a, uint256 b) public pure returns (uint256 result) {
        result = a + b;
    }
}
"#;
        let table = table_for(source);

        let offset = source.find("result = a").unwrap();
        let def = table.resolve("result", offset).unwrap();
        assert_eq!(def.kind, SymbolKind::ReturnParameter);
    }

    #[test]
    fn test_events_errors_structs() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    event Transfer(address indexed from, address indexed to, uint256 amount);
    error Unauthorized();
    struct Info { uint256 id; }
}
"#;
        let table = table_for(source);

        let offset = 80;
        assert_eq!(
            table.resolve("Transfer", offset).unwrap().kind,
            SymbolKind::Event
        );
        assert_eq!(
            table.resolve("Unauthorized", offset).unwrap().kind,
            SymbolKind::Error
        );
        assert_eq!(
            table.resolve("Info", offset).unwrap().kind,
            SymbolKind::Struct
        );
    }

    #[test]
    fn test_enum_and_udvt() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

type Price is uint256;

contract Test {
    enum Status { Active, Paused }
}
"#;
        let table = table_for(source);

        // Use offset inside the contract body where both should be resolvable.
        let offset = source.find("enum Status").unwrap() + 15; // inside enum body
        assert_eq!(
            table.resolve("Status", offset).unwrap().kind,
            SymbolKind::Enum
        );

        // Price is at file level — resolvable from anywhere.
        assert_eq!(
            table.resolve("Price", offset).unwrap().kind,
            SymbolKind::Udvt
        );
    }

    #[test]
    fn test_unresolved_returns_none() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    function foo() public {}
}
"#;
        let table = table_for(source);
        assert!(table.resolve("nonexistent", 80).is_none());
    }

    #[test]
    fn test_imports_named() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {ERC20, Ownable as Own} from "@openzeppelin/contracts/token.sol";

contract Test {}
"#;
        let table = table_for(source);
        assert_eq!(table.imports.len(), 1);
        let import = &table.imports[0];
        assert_eq!(import.path, "@openzeppelin/contracts/token.sol");
        match &import.symbols {
            ImportedSymbols::Named(names) => {
                assert_eq!(names.len(), 2);
                assert_eq!(names[0].0, "ERC20");
                assert!(names[0].1.is_none());
                assert_eq!(names[1].0, "Ownable");
                assert_eq!(names[1].1.as_deref(), Some("Own"));
            }
            _ => panic!("expected Named import"),
        }
    }

    #[test]
    fn test_imports_plain() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "./Token.sol";
"#;
        let table = table_for(source);
        assert_eq!(table.imports.len(), 1);
        assert_eq!(table.imports[0].path, "./Token.sol");
        match &table.imports[0].symbols {
            ImportedSymbols::Plain(alias) => assert!(alias.is_none()),
            _ => panic!("expected Plain import"),
        }
    }

    #[test]
    fn test_imports_glob() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import * as Lib from "./Lib.sol";
"#;
        let table = table_for(source);
        assert_eq!(table.imports.len(), 1);
        match &table.imports[0].symbols {
            ImportedSymbols::Glob(alias) => assert_eq!(alias, "Lib"),
            _ => panic!("expected Glob import"),
        }
    }

    #[test]
    fn test_struct_fields() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    struct Position {
        address token;
        uint256 amount;
    }
}
"#;
        let table = table_for(source);
        let struct_def = table.resolve("Position", 80).unwrap();
        assert_eq!(struct_def.kind, SymbolKind::Struct);
        assert!(struct_def.scope.is_some());

        let token = table.resolve_member(struct_def, "token").unwrap();
        assert_eq!(token.kind, SymbolKind::StructField);
        assert_eq!(token.name, "token");

        let amount = table.resolve_member(struct_def, "amount").unwrap();
        assert_eq!(amount.kind, SymbolKind::StructField);
        assert_eq!(amount.name, "amount");

        // Non-existent field returns None.
        assert!(table.resolve_member(struct_def, "nonexistent").is_none());
    }

    #[test]
    fn test_contract_member_access() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MyToken {
    function transfer(address to, uint256 amount) external returns (bool) {
        return true;
    }
    uint256 public totalSupply;
    event Transfer(address from, address to, uint256 value);
}
"#;
        let table = table_for(source);
        let contract = table.resolve("MyToken", 80).unwrap();
        assert!(contract.scope.is_some());

        let transfer = table.resolve_member(contract, "transfer").unwrap();
        assert_eq!(transfer.kind, SymbolKind::Function);

        let supply = table.resolve_member(contract, "totalSupply").unwrap();
        assert_eq!(supply.kind, SymbolKind::StateVariable);

        let event = table.resolve_member(contract, "Transfer").unwrap();
        assert_eq!(event.kind, SymbolKind::Event);
    }

    #[test]
    fn test_enum_member_access() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract Test {
    enum Status { Active, Paused, Closed }
}
"#;
        let table = table_for(source);
        let enum_def = table.resolve("Status", 80).unwrap();
        assert!(enum_def.scope.is_some());

        let active = table.resolve_member(enum_def, "Active").unwrap();
        assert_eq!(active.kind, SymbolKind::EnumVariant);

        let paused = table.resolve_member(enum_def, "Paused").unwrap();
        assert_eq!(paused.kind, SymbolKind::EnumVariant);

        let closed = table.resolve_member(enum_def, "Closed").unwrap();
        assert_eq!(closed.kind, SymbolKind::EnumVariant);
    }

    #[test]
    fn test_library_member_access() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

library SafeMath {
    function add(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
    function sub(uint256 a, uint256 b) internal pure returns (uint256) {
        return a - b;
    }
}
"#;
        let table = table_for(source);
        let lib = table.resolve("SafeMath", 80).unwrap();
        assert_eq!(lib.kind, SymbolKind::Library);
        assert!(lib.scope.is_some());

        let add = table.resolve_member(lib, "add").unwrap();
        assert_eq!(add.kind, SymbolKind::Function);

        let sub = table.resolve_member(lib, "sub").unwrap();
        assert_eq!(sub.kind, SymbolKind::Function);
    }

    #[test]
    fn test_interface_member_access() {
        let source = r#"
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface IERC20 {
    function totalSupply() external view returns (uint256);
    function balanceOf(address account) external view returns (uint256);
    event Transfer(address indexed from, address indexed to, uint256 value);
}
"#;
        let table = table_for(source);
        let iface = table.resolve("IERC20", 80).unwrap();
        assert_eq!(iface.kind, SymbolKind::Interface);
        assert!(iface.scope.is_some());

        let supply = table.resolve_member(iface, "totalSupply").unwrap();
        assert_eq!(supply.kind, SymbolKind::Function);

        let balance = table.resolve_member(iface, "balanceOf").unwrap();
        assert_eq!(balance.kind, SymbolKind::Function);

        let transfer = table.resolve_member(iface, "Transfer").unwrap();
        assert_eq!(transfer.kind, SymbolKind::Event);
    }

    #[test]
    fn test_find_member_access_at_offset_basic() {
        let source = "MyContract.transfer(to, amount);";
        // Cursor on 'transfer' (offset 11)
        let (container, member, range) = find_member_access_at_offset(source, 11).unwrap();
        assert_eq!(container, "MyContract");
        assert_eq!(member, "transfer");
        assert_eq!(&source[range], "transfer");
    }

    #[test]
    fn test_find_member_access_at_offset_enum() {
        let source = "Status.Active";
        let (container, member, _) = find_member_access_at_offset(source, 7).unwrap();
        assert_eq!(container, "Status");
        assert_eq!(member, "Active");
    }

    #[test]
    fn test_find_member_access_at_offset_no_dot() {
        let source = "transfer(to, amount);";
        assert!(find_member_access_at_offset(source, 0).is_none());
    }

    #[test]
    fn test_find_member_access_on_container_name() {
        // When cursor is on the container name, not the member.
        let source = "MyContract.transfer(to, amount);";
        assert!(find_member_access_at_offset(source, 3).is_none());
    }
}
