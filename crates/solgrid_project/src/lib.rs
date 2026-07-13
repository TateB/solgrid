//! Shared workspace and navigation index for editor features.
//!
//! `solgrid_project` owns workspace discovery, import/remapping resolution,
//! cached file snapshots, workspace-symbol indexing, and the first batch of
//! navigation features used by the language server.

use serde::{Deserialize, Serialize};
use serde_json::json;
use solgrid_ast::resolve::ImportResolver;
use solgrid_ast::symbols::{self, ImportedSymbols, SymbolDef, SymbolKind, SymbolTable, TypePath};
use solgrid_parser::solar_ast::{
    self, yul, CallArgsKind, FunctionKind, ItemFunction, ItemKind, Stmt, StmtKind, Visibility,
};
use solgrid_parser::with_parsed_ast_sequential;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::ops::Range as ByteRange;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use tower_lsp_server::ls_types::{
    CodeLens, Command, DocumentLink, DocumentSymbol, DocumentSymbolResponse, Location, Position,
    Range, SymbolInformation, SymbolKind as LspSymbolKind, Uri, WorkspaceSymbolResponse,
};

/// Trait implemented by semantic/navigation backends.
///
/// The first implementation is Solar-based via `solgrid_ast`, but the trait
/// boundary keeps the server ready for future `solar-sema` adoption.
pub trait NavBackend: Clone + Send + Sync + 'static {
    fn snapshot(&self, path: &Path, source: &str) -> Option<ProjectSnapshot>;
}

/// Default Solar-based navigation backend.
#[derive(Debug, Clone, Default)]
pub struct SolarNavBackend;

impl NavBackend for SolarNavBackend {
    fn snapshot(&self, path: &Path, source: &str) -> Option<ProjectSnapshot> {
        let path = normalize_path(path);
        let filename = path.to_string_lossy().to_string();
        let table = symbols::build_symbol_table(source, &filename)?;
        let contracts = collect_contract_declarations(source, &filename);
        let callables = collect_callable_declarations(source, &filename);
        Some(ProjectSnapshot {
            path,
            source: source.to_string(),
            table,
            contracts,
            callables,
        })
    }
}

/// Parsed snapshot of a single Solidity file.
#[derive(Debug, Clone)]
pub struct ProjectSnapshot {
    pub path: PathBuf,
    pub source: String,
    pub table: SymbolTable,
    contracts: Vec<ContractDecl>,
    callables: Vec<CallableDecl>,
}

/// Cached project data for a single file.
#[derive(Debug, Clone)]
pub struct ProjectFileData {
    pub path: PathBuf,
    pub workspace_root: Option<PathBuf>,
    pub snapshot: ProjectSnapshot,
    pub import_paths: Vec<PathBuf>,
    pub exported_symbols: Vec<WorkspaceSymbolEntry>,
}

/// Workspace-wide symbol entry used for auto-imports and workspace/symbol.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspaceSymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: PathBuf,
    pub name_span: ByteRange<usize>,
    pub def_span: ByteRange<usize>,
    pub container_name: Option<String>,
}

/// Stable identity for a resolvable symbol reference.
#[derive(Debug, Clone)]
pub struct ReferenceTarget {
    pub file_path: PathBuf,
    pub name: String,
    pub kind: SymbolKind,
    pub name_span: ByteRange<usize>,
    pub def_span: ByteRange<usize>,
    pub container_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReferenceCacheKey {
    generation: u64,
    target: ReferenceTarget,
}

#[derive(Debug, Clone)]
struct ReferenceSearchResult {
    locations: Vec<Location>,
    declaration: Option<Location>,
}

/// Safe rename plan derived from the current reference index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePlan {
    pub range: Range,
    pub placeholder: String,
    pub locations: Vec<Location>,
}

/// Prepared call hierarchy item derived from the shared project model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallHierarchyEntry {
    pub path: PathBuf,
    pub name: String,
    pub detail: Option<String>,
    pub kind: LspSymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub target_offset: usize,
}

/// Incoming calls for a prepared call hierarchy item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingCallEntry {
    pub from: CallHierarchyEntry,
    pub from_ranges: Vec<Range>,
}

/// Outgoing calls for a prepared call hierarchy item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingCallEntry {
    pub to: CallHierarchyEntry,
    pub from_ranges: Vec<Range>,
}

impl PartialEq for ReferenceTarget {
    fn eq(&self, other: &Self) -> bool {
        self.file_path == other.file_path
            && self.name == other.name
            && self.kind == other.kind
            && self.name_span == other.name_span
            && self.def_span == other.def_span
    }
}

impl Eq for ReferenceTarget {}

impl Hash for ReferenceTarget {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.file_path.hash(state);
        self.name.hash(state);
        self.kind.hash(state);
        self.name_span.hash(state);
        self.def_span.hash(state);
    }
}

/// A symbol resolved from an imported file.
#[derive(Debug, Clone)]
pub struct CrossFileSymbol {
    pub source: String,
    pub table: SymbolTable,
    pub def: SymbolDef,
    pub resolved_path: PathBuf,
}

/// Graphs available from the shared project model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphKind {
    Imports,
    Inheritance,
    LinearizedInheritance,
    ControlFlow,
}

/// Optional semantic category for graph nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphNodeKind {
    File,
    Contract,
    Entry,
    Exit,
    Modifier,
    Declaration,
    Assignment,
    Call,
    Emit,
    Branch,
    Loop,
    LoopNext,
    TerminalReturn,
    TerminalRevert,
    ControlTransfer,
    Assembly,
    Try,
    Catch,
    Block,
    Statement,
}

/// Optional semantic category for graph edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphEdgeKind {
    Imports,
    Inherits,
    Precedes,
    Normal,
    BranchTrue,
    BranchFalse,
    LoopBack,
    Return,
    Revert,
    Break,
    Continue,
}

/// Serializable graph node for editor and future CLI use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<GraphNodeKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

/// Serializable graph edge for editor and future CLI use.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<GraphEdgeKind>,
}

/// Shared graph document returned by the project model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphDocument {
    pub kind: GraphKind,
    pub title: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_node_id: Option<String>,
}

/// Code-lens entry point for editor graph commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphLensSpec {
    pub range: Range,
    pub title: String,
    pub kind: GraphKind,
    pub symbol_name: Option<String>,
    pub target_offset: Option<usize>,
}

/// Project-derived declaration annotation exposed as an editor inlay hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InheritanceHint {
    pub offset: usize,
    pub label: String,
    pub tooltip: String,
}

#[derive(Debug, Clone)]
struct ContractDecl {
    name: String,
    kind: SymbolKind,
    name_span: ByteRange<usize>,
    bases: Vec<TypePath>,
}

#[derive(Debug, Clone)]
struct CallableDecl {
    name: String,
    label: String,
    detail: Option<String>,
    container_name: Option<String>,
    kind: SymbolKind,
    kind_label: String,
    target_offset: usize,
    def_span: ByteRange<usize>,
    lens_span: ByteRange<usize>,
}

#[derive(Debug, Clone)]
struct ResolvedContract {
    path: PathBuf,
    decl: ContractDecl,
}

#[derive(Debug, Clone)]
struct ResolvedPathTarget {
    snapshot: ProjectSnapshot,
    def: SymbolDef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InheritedMemberKey {
    Function { name: String, signature: String },
    Modifier { name: String, signature: String },
    StateVariable { name: String, ty: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum InheritedSurfaceKey {
    Function { name: String, signature: String },
    Modifier { name: String, signature: String },
    StateVariable { name: String, ty: Option<String> },
    Event { name: String },
    Error { name: String },
    Struct { name: String },
    Enum { name: String },
    Udvt { name: String },
}

#[derive(Debug, Clone)]
struct InheritedMemberSurface {
    origin: String,
    member_name: String,
    kind_label: &'static str,
}

#[derive(Debug, Clone)]
struct PendingEdge {
    from: String,
    label: Option<String>,
    kind: Option<GraphEdgeKind>,
}

#[derive(Debug, Clone)]
struct ModifierPlan<'ast> {
    label: String,
    detail: String,
    body_source: Option<Arc<str>>,
    body_source_base: usize,
    body_stmts: Option<&'ast [Stmt<'ast>]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ModifierLookupKey {
    path: PathBuf,
    contract_name: String,
    modifier_name: String,
    arity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResolvedContractRef {
    path: PathBuf,
    contract_name: String,
}

#[derive(Debug, Clone)]
struct ModifierLookupEntry<'ast> {
    body_source: Arc<str>,
    body_source_base: usize,
    body_stmts: Option<&'ast [Stmt<'ast>]>,
}

#[derive(Debug, Clone, Copy)]
struct YulFunctionLookupEntry<'ast> {
    function: &'ast yul::Function<'ast>,
}

#[derive(Debug, Clone)]
struct YulFunctionGraph {
    entry: String,
}

#[derive(Debug, Default)]
struct YulBuildContext<'ast> {
    functions: HashMap<String, YulFunctionLookupEntry<'ast>>,
    graphs: HashMap<String, YulFunctionGraph>,
    active_calls: HashSet<String>,
}

#[derive(Debug, Clone, Copy)]
struct PlaceholderExpansion<'ast, 'modifiers, 'source> {
    body_source: &'source str,
    body_source_base: usize,
    body_stmts: &'ast [Stmt<'ast>],
    modifiers: &'modifiers [ModifierPlan<'ast>],
}

#[derive(Debug, Clone)]
struct FlowSegment {
    entry: String,
    fallthroughs: Vec<PendingEdge>,
    returns: Vec<PendingEdge>,
    breaks: Vec<PendingEdge>,
    continues: Vec<PendingEdge>,
}

#[derive(Debug, Clone)]
struct ResolvedOutgoingCall {
    target: ReferenceTarget,
    from_range: Range,
}

#[derive(Debug, Clone)]
struct LoopContext {
    _continue_target: String,
}

struct ControlFlowBuilder<'a> {
    path: &'a Path,
    workspace_root: Option<&'a Path>,
    prefix: String,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    next_node_index: usize,
    entry_id: String,
    exit_id: String,
}

/// Shared workspace/project index.
pub struct ProjectIndex<B = SolarNavBackend> {
    backend: B,
    workspace_root: Option<PathBuf>,
    resolver: ImportResolver,
    files: HashMap<PathBuf, ProjectFileData>,
    symbols: HashMap<String, Vec<WorkspaceSymbolEntry>>,
    reference_generation: u64,
    reference_cache: Mutex<HashMap<ReferenceCacheKey, ReferenceSearchResult>>,
}

impl Default for ProjectIndex<SolarNavBackend> {
    fn default() -> Self {
        Self::new(None)
    }
}

impl ProjectIndex<SolarNavBackend> {
    /// Create a new Solar-backed project index.
    pub fn new(workspace_root: Option<PathBuf>) -> Self {
        Self::with_backend(workspace_root, SolarNavBackend)
    }

    /// Build the index by scanning a workspace root for Solidity files.
    pub fn build(root: &Path) -> Self {
        let mut index = Self::new(Some(root.to_path_buf()));
        index.rebuild_from_workspace();
        index
    }
}

impl<B: NavBackend> ProjectIndex<B> {
    /// Create a project index using an explicit backend.
    pub fn with_backend(workspace_root: Option<PathBuf>, backend: B) -> Self {
        let workspace_root = workspace_root.map(|root| normalize_path(&root));
        let resolver = ImportResolver::new(workspace_root.clone());
        Self {
            backend,
            workspace_root,
            resolver,
            files: HashMap::new(),
            symbols: HashMap::new(),
            reference_generation: 0,
            reference_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Return the current workspace root, if any.
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// Return the import resolver currently backing the project index.
    pub fn resolver(&self) -> &ImportResolver {
        &self.resolver
    }

    /// Return the resolved remappings for a specific file.
    pub fn remappings_for_file(&self, path: &Path) -> Vec<(String, PathBuf)> {
        self.resolver.remappings_for_file(path)
    }

    /// Return cached data for a file path, if indexed.
    pub fn file(&self, path: &Path) -> Option<&ProjectFileData> {
        self.files.get(&normalize_path(path))
    }

    /// Return all indexed Solidity file paths.
    pub fn indexed_paths(&self) -> Vec<PathBuf> {
        self.files.keys().cloned().collect()
    }

    /// Return every indexed file that transitively imports `path`.
    ///
    /// The target itself is excluded, including when an import cycle leads
    /// back to it. Results are normalized and sorted for deterministic LSP
    /// invalidation.
    pub fn transitive_import_dependents(&self, path: &Path) -> Vec<PathBuf> {
        let target = normalize_path(path);
        let mut visited = HashSet::from([target.clone()]);
        let mut queue = VecDeque::from([target]);
        let mut dependents = Vec::new();

        while let Some(imported_path) = queue.pop_front() {
            for (candidate_path, file) in &self.files {
                if visited.contains(candidate_path)
                    || !file.import_paths.iter().any(|path| path == &imported_path)
                {
                    continue;
                }

                visited.insert(candidate_path.clone());
                queue.push_back(candidate_path.clone());
                dependents.push(candidate_path.clone());
            }
        }

        dependents.sort();
        dependents
    }

    /// Return the number of Solidity files currently held in the workspace index.
    pub fn indexed_file_count(&self) -> usize {
        self.files.len()
    }

    #[cfg(test)]
    fn reference_cache_entry_count(&self) -> usize {
        self.reference_cache
            .lock()
            .map(|cache| cache.len())
            .unwrap_or(0)
    }

    /// Parse a temporary snapshot without mutating the index.
    pub fn snapshot_for_source(&self, path: &Path, source: &str) -> Option<ProjectSnapshot> {
        self.backend.snapshot(path, source)
    }

    /// Rebuild the resolver and full workspace index from disk.
    pub fn refresh_workspace_state(&mut self) {
        self.resolver = ImportResolver::new(self.workspace_root.clone());
        self.rebuild_from_workspace();
    }

    /// Re-index a single file from source content.
    pub fn update_file(&mut self, path: &Path, source: &str) {
        let path = normalize_path(path);
        self.remove_file(&path);

        let Some(snapshot) = self.backend.snapshot(&path, source) else {
            return;
        };

        let workspace_root = workspace_root_for_file(&path, self.workspace_root.as_deref());
        let import_paths = snapshot
            .table
            .imports
            .iter()
            .filter_map(|import| self.resolver.resolve(&import.path, &path))
            .map(|resolved| normalize_path(&resolved))
            .collect::<Vec<_>>();

        let exported_symbols = snapshot
            .table
            .file_level_symbols()
            .iter()
            .filter(|sym| is_exportable(sym.kind))
            .map(|sym| WorkspaceSymbolEntry {
                name: sym.name.clone(),
                kind: sym.kind,
                file_path: path.clone(),
                name_span: sym.name_span.clone(),
                def_span: sym.def_span.clone(),
                container_name: None,
            })
            .collect::<Vec<_>>();

        for entry in &exported_symbols {
            self.symbols
                .entry(entry.name.clone())
                .or_default()
                .push(entry.clone());
        }

        self.files.insert(
            path.clone(),
            ProjectFileData {
                path,
                workspace_root,
                snapshot,
                import_paths,
                exported_symbols,
            },
        );
        self.invalidate_reference_cache();
    }

    /// Remove all indexed data for a file.
    pub fn remove_file(&mut self, path: &Path) {
        let path = normalize_path(path);
        if let Some(previous) = self.files.remove(&path) {
            self.invalidate_reference_cache();
            for entry in previous.exported_symbols {
                if let Some(bucket) = self.symbols.get_mut(&entry.name) {
                    bucket.retain(|candidate| candidate.file_path != path);
                    if bucket.is_empty() {
                        self.symbols.remove(&entry.name);
                    }
                }
            }
        }
    }

    /// Re-sync a closed file from disk, removing it if the file no longer exists.
    pub fn sync_closed_file(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(source) => self.update_file(path, &source),
            Err(_) => self.remove_file(path),
        }
    }

    /// Return exported symbols whose names start with `prefix`.
    pub fn symbols_matching(&self, prefix: &str) -> Vec<&WorkspaceSymbolEntry> {
        let mut matches = if prefix.is_empty() {
            self.symbols.values().flatten().collect::<Vec<_>>()
        } else {
            self.symbols
                .iter()
                .filter(|(name, _)| name.starts_with(prefix))
                .flat_map(|(_, entries)| entries)
                .collect::<Vec<_>>()
        };
        matches.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.file_path.cmp(&right.file_path))
        });
        matches
    }

    /// Return top-level exported workspace symbols matching a user query.
    pub fn workspace_symbols(&self, query: &str) -> WorkspaceSymbolResponse {
        let query = query.trim().to_ascii_lowercase();
        let mut entries = self
            .symbols
            .values()
            .flatten()
            .filter(|entry| {
                if query.is_empty() {
                    return true;
                }
                let name = entry.name.to_ascii_lowercase();
                name.contains(&query) || name.starts_with(&query)
            })
            .cloned()
            .collect::<Vec<_>>();

        entries.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.file_path.cmp(&right.file_path))
        });

        let result = entries
            .into_iter()
            .filter_map(|entry| {
                let uri = path_to_uri(&entry.file_path)?;
                let source = self.files.get(&entry.file_path)?.snapshot.source.as_str();
                Some(symbol_information(entry, uri, source))
            })
            .collect::<Vec<_>>();

        WorkspaceSymbolResponse::from(result)
    }

    /// Build hierarchical document symbols for a file source.
    pub fn document_symbols(&self, path: &Path, source: &str) -> Option<DocumentSymbolResponse> {
        let snapshot = self.snapshot_for_source(path, source)?;
        let symbols = snapshot
            .table
            .file_level_symbols()
            .iter()
            .map(|def| document_symbol_from_def(&snapshot, def))
            .collect::<Vec<_>>();
        Some(DocumentSymbolResponse::Nested(symbols))
    }

    /// Build import-path document links for a file source.
    pub fn document_links(&self, path: &Path, source: &str) -> Vec<DocumentLink> {
        let Some(snapshot) = self.snapshot_for_source(path, source) else {
            return Vec::new();
        };

        snapshot
            .table
            .imports
            .iter()
            .filter_map(|import| {
                let resolved = self.resolver.resolve(&import.path, &snapshot.path)?;
                let target = path_to_uri(&resolved)?;
                Some(DocumentLink {
                    range: span_to_range(source, &import.path_span),
                    target: Some(target),
                    tooltip: None,
                    data: None,
                })
            })
            .collect()
    }

    /// Resolve the symbol under `position` and return all project references to it.
    pub fn find_references(
        &self,
        path: &Path,
        source: &str,
        position: Position,
        include_declaration: bool,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Vec<Location> {
        let Some(target) = self.reference_target_at_position(path, source, position, get_source)
        else {
            return Vec::new();
        };

        self.find_references_for_target(
            &target,
            Some((normalize_path(path), source)),
            include_declaration,
            get_source,
        )
    }

    /// Build first-pass reference-count code lenses for a file source.
    pub fn code_lenses(
        &self,
        path: &Path,
        source: &str,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Vec<CodeLens> {
        let Some(snapshot) = self.snapshot_for_source(path, source) else {
            return Vec::new();
        };

        let mut targets = Vec::new();
        for def in snapshot.table.file_level_symbols() {
            if is_exportable(def.kind) {
                targets.push((def, None));
            }

            if matches!(
                def.kind,
                SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
            ) {
                if let Some(scope_id) = def.scope {
                    for member in snapshot.table.scope_symbols(scope_id) {
                        if is_code_lens_member(member.kind) {
                            targets.push((member, Some(def.name.clone())));
                        }
                    }
                }
            }
        }

        let reference_targets = targets
            .iter()
            .map(|(def, container_name)| {
                reference_target_from_def(&snapshot.path, def, container_name.clone())
            })
            .collect::<Vec<_>>();
        let current_file = (snapshot.path.clone(), source);
        let cacheable = self.reference_cache_usable(Some(&current_file));
        let mut reference_results = vec![None; reference_targets.len()];
        let mut missing_indices = Vec::new();
        let mut missing_targets = Vec::new();
        for (index, target) in reference_targets.iter().enumerate() {
            if cacheable {
                reference_results[index] = self.cached_references_for_target(target);
            }
            if reference_results[index].is_none() {
                missing_indices.push(index);
                missing_targets.push(target.clone());
            }
        }

        let computed = self.compute_references_for_targets_batch(
            &missing_targets,
            Some(&current_file),
            get_source,
        );
        for ((index, target), result) in missing_indices
            .into_iter()
            .zip(missing_targets)
            .zip(computed)
        {
            if cacheable {
                self.store_references_for_target(&target, result.clone());
            }
            reference_results[index] = Some(result);
        }

        targets
            .into_iter()
            .zip(
                reference_results
                    .into_iter()
                    .map(|result| result.expect("every CodeLens target must have references")),
            )
            .map(|((def, _container_name), result)| {
                let locations = result
                    .locations
                    .into_iter()
                    .filter(|location| result.declaration.as_ref() != Some(location))
                    .collect::<Vec<_>>();
                let count = locations.len();
                let range = span_to_range(source, &def.name_span);
                let arguments = path_to_uri(&snapshot.path).map(|uri| {
                    vec![json!({
                        "locations": locations,
                        "position": range.start,
                        "uri": uri,
                    })]
                });
                CodeLens {
                    range,
                    command: Some(Command {
                        title: reference_count_title(count),
                        command: "solgrid.showReferences".to_string(),
                        arguments,
                    }),
                    data: None,
                }
            })
            .collect()
    }

    /// Build declaration inlay hints for overriding inherited members.
    pub fn inheritance_hints(
        &self,
        path: &Path,
        source: &str,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Vec<InheritanceHint> {
        let Some(snapshot) = self.snapshot_for_source(path, source) else {
            return Vec::new();
        };
        let current_file = (snapshot.path.clone(), source);
        let mut hints = Vec::new();

        for def in snapshot.table.file_level_symbols() {
            let Some(contract_decl) = find_contract_decl(&snapshot, def).cloned() else {
                continue;
            };
            let Some(scope_id) = def.scope else {
                continue;
            };
            if contract_decl.bases.is_empty() {
                continue;
            }

            let contract = ResolvedContract {
                path: snapshot.path.clone(),
                decl: contract_decl,
            };
            let contract_offset = declaration_hint_offset(source, &def.def_span);
            if let Some(hint) = self.contract_inheritance_hint(
                &contract,
                contract_offset,
                Some(&current_file),
                get_source,
            ) {
                hints.push(hint);
            }
            if let Some(hint) = self.contract_inherited_members_hint(
                &contract,
                contract_offset,
                Some(&current_file),
                get_source,
            ) {
                hints.push(hint);
            }
            for member in snapshot.table.scope_symbols(scope_id) {
                let Some(key) = inherited_member_key(member) else {
                    continue;
                };
                let origins = self.nearest_inherited_member_origins(
                    &contract,
                    &key,
                    Some(&current_file),
                    get_source,
                    &mut HashSet::new(),
                );
                if origins.is_empty() {
                    continue;
                }
                let offset = declaration_hint_offset(source, &member.def_span);
                hints.push(inheritance_hint_from_origins(member, origins, offset));
            }
        }

        hints.sort_by(|left, right| left.offset.cmp(&right.offset));
        hints
    }

    /// Build graph-entry code lenses for a file source.
    pub fn graph_lenses(&self, path: &Path, source: &str) -> Vec<GraphLensSpec> {
        let Some(snapshot) = self.snapshot_for_source(path, source) else {
            return Vec::new();
        };

        let mut lenses = Vec::new();
        if let Some(import) = snapshot.table.imports.first() {
            lenses.push(GraphLensSpec {
                range: span_to_range(source, &import.path_span),
                title: "Imports graph".to_string(),
                kind: GraphKind::Imports,
                symbol_name: None,
                target_offset: None,
            });
        }

        for contract in &snapshot.contracts {
            if matches!(
                contract.kind,
                SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
            ) {
                lenses.push(GraphLensSpec {
                    range: span_to_range(source, &contract.name_span),
                    title: "Inheritance graph".to_string(),
                    kind: GraphKind::Inheritance,
                    symbol_name: Some(contract.name.clone()),
                    target_offset: Some(contract.name_span.start),
                });

                if !contract.bases.is_empty() {
                    lenses.push(GraphLensSpec {
                        range: span_to_range(source, &contract.name_span),
                        title: "Linearized inheritance".to_string(),
                        kind: GraphKind::LinearizedInheritance,
                        symbol_name: Some(contract.name.clone()),
                        target_offset: Some(contract.name_span.start),
                    });
                }
            }
        }

        for callable in &snapshot.callables {
            lenses.push(GraphLensSpec {
                range: span_to_range(source, &callable.lens_span),
                title: "Control-flow graph".to_string(),
                kind: GraphKind::ControlFlow,
                symbol_name: Some(callable.label.clone()),
                target_offset: Some(callable.target_offset),
            });
        }

        lenses
    }

    /// Build an imports graph rooted at `path`.
    pub fn imports_graph(
        &self,
        path: &Path,
        source: &str,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<GraphDocument> {
        let snapshot = self.snapshot_for_source(path, source)?;
        let mut nodes = HashMap::new();
        let mut edges = HashSet::new();
        let mut visited = HashSet::new();
        self.collect_import_graph(&snapshot, get_source, &mut visited, &mut nodes, &mut edges);
        let mut node_values = nodes.into_values().collect::<Vec<_>>();
        node_values.sort_by(|left, right| left.label.cmp(&right.label));
        let mut edge_values = edges.into_iter().collect::<Vec<_>>();
        edge_values.sort_by(|left, right| {
            left.from
                .cmp(&right.from)
                .then_with(|| left.to.cmp(&right.to))
        });

        Some(GraphDocument {
            kind: GraphKind::Imports,
            title: format!(
                "Imports graph for {}",
                display_path(&snapshot.path, self.workspace_root())
            ),
            nodes: node_values,
            edges: edge_values,
            focus_node_id: Some(file_graph_node_id(&snapshot.path)),
        })
    }

    /// Build an inheritance graph rooted at `symbol_name` in `path`.
    pub fn inheritance_graph(
        &self,
        path: &Path,
        source: &str,
        symbol_name: &str,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<GraphDocument> {
        let snapshot = self.snapshot_for_source(path, source)?;
        let contract = snapshot
            .contracts
            .iter()
            .find(|contract| contract.name == symbol_name)?;
        let root = ResolvedContract {
            path: snapshot.path.clone(),
            decl: contract.clone(),
        };

        let mut nodes = HashMap::new();
        let mut edges = HashSet::new();
        let mut visited = HashSet::new();
        self.collect_inheritance_graph(&root, get_source, &mut visited, &mut nodes, &mut edges);
        let mut node_values = nodes.into_values().collect::<Vec<_>>();
        node_values.sort_by(|left, right| left.label.cmp(&right.label));
        let mut edge_values = edges.into_iter().collect::<Vec<_>>();
        edge_values.sort_by(|left, right| {
            left.from
                .cmp(&right.from)
                .then_with(|| left.to.cmp(&right.to))
        });

        Some(GraphDocument {
            kind: GraphKind::Inheritance,
            title: format!("Inheritance graph for {}", contract.name),
            nodes: node_values,
            edges: edge_values,
            focus_node_id: Some(contract_graph_node_id(&snapshot.path, &contract.name_span)),
        })
    }

    /// Build a linearized inheritance graph rooted at `symbol_name` in `path`.
    pub fn linearized_inheritance_graph(
        &self,
        path: &Path,
        source: &str,
        symbol_name: &str,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<GraphDocument> {
        let snapshot = self.snapshot_for_source(path, source)?;
        let contract = snapshot
            .contracts
            .iter()
            .find(|contract| contract.name == symbol_name)?;
        let root = ResolvedContract {
            path: snapshot.path.clone(),
            decl: contract.clone(),
        };
        let mut cache = HashMap::new();
        let mut active = HashSet::new();
        let current_file = (snapshot.path.clone(), source);
        let linearized = self.linearized_inheritance_order(
            &root,
            Some(&current_file),
            get_source,
            &mut cache,
            &mut active,
        )?;

        let mut nodes = Vec::with_capacity(linearized.len());
        let mut edges = Vec::new();

        for (index, contract) in linearized.iter().enumerate() {
            nodes.push(linearized_contract_graph_node(
                &contract.path,
                &contract.decl,
                self.workspace_root(),
                index,
            ));
        }

        for pair in linearized.windows(2) {
            let from = contract_graph_node_id(&pair[0].path, &pair[0].decl.name_span);
            let to = contract_graph_node_id(&pair[1].path, &pair[1].decl.name_span);
            edges.push(GraphEdge {
                from,
                to,
                label: Some("precedes".to_string()),
                kind: Some(GraphEdgeKind::Precedes),
            });
        }

        Some(GraphDocument {
            kind: GraphKind::LinearizedInheritance,
            title: format!("Linearized inheritance for {}", contract.name),
            nodes,
            edges,
            focus_node_id: Some(contract_graph_node_id(&snapshot.path, &contract.name_span)),
        })
    }

    /// Build a function- or modifier-level control-flow graph.
    pub fn control_flow_graph(
        &self,
        path: &Path,
        source: &str,
        target_offset: usize,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<GraphDocument> {
        let snapshot = self.snapshot_for_source(path, source)?;
        let filename = snapshot.path.to_string_lossy().to_string();
        let contract_linearization = linearized_contract_refs(self, &snapshot, get_source);

        let session = solgrid_parser::solar_interface::Session::builder()
            .with_buffer_emitter(solgrid_parser::solar_interface::ColorChoice::Never)
            .build();

        session.enter_sequential(|| {
            let arena = solar_ast::Arena::new();
            let filename_obj =
                solgrid_parser::solar_interface::source_map::FileName::Custom(filename.clone());
            let parser = solgrid_parser::solar_parse::Parser::from_source_code(
                &session,
                &arena,
                filename_obj,
                source.to_string(),
            )
            .ok()?;
            let mut parser = parser;
            let source_unit = parser.parse_file().ok()?;
            let source_base = session
                .source_map()
                .get_file_ref(
                    &solgrid_parser::solar_interface::source_map::FileName::Custom(
                        filename.clone(),
                    ),
                )?
                .start_pos
                .0 as usize;
            let mut modifier_lookup = HashMap::new();
            collect_modifier_lookup(
                &snapshot.path,
                Arc::<str>::from(source),
                source_base,
                &source_unit,
                &mut modifier_lookup,
            );
            let mut parsed_modifier_units = Vec::new();

            let mut modifier_paths = contract_linearization
                .values()
                .flatten()
                .map(|contract| contract.path.clone())
                .collect::<Vec<_>>();
            modifier_paths.sort();
            modifier_paths.dedup();

            for modifier_path in modifier_paths {
                if modifier_path == snapshot.path {
                    continue;
                }
                let modifier_source = get_source(&modifier_path)?;
                let modifier_source = Arc::<str>::from(modifier_source);
                let modifier_filename = modifier_path.to_string_lossy().to_string();
                let modifier_filename_obj =
                    solgrid_parser::solar_interface::source_map::FileName::Custom(
                        modifier_filename.clone(),
                    );
                let modifier_parser = solgrid_parser::solar_parse::Parser::from_source_code(
                    &session,
                    &arena,
                    modifier_filename_obj,
                    modifier_source.to_string(),
                )
                .ok()?;
                let mut modifier_parser = modifier_parser;
                let modifier_source_unit = Box::new(modifier_parser.parse_file().ok()?);
                let modifier_source_base = session
                    .source_map()
                    .get_file_ref(
                        &solgrid_parser::solar_interface::source_map::FileName::Custom(
                            modifier_filename.clone(),
                        ),
                    )?
                    .start_pos
                    .0 as usize;
                parsed_modifier_units.push((
                    modifier_path,
                    modifier_source,
                    modifier_source_base,
                    modifier_source_unit,
                ));
            }

            for (modifier_path, modifier_source, modifier_source_base, modifier_source_unit) in
                &parsed_modifier_units
            {
                collect_modifier_lookup(
                    modifier_path,
                    modifier_source.clone(),
                    *modifier_source_base,
                    modifier_source_unit.as_ref(),
                    &mut modifier_lookup,
                );
            }

            build_control_flow_graph_from_source_unit(
                &source_unit,
                source,
                &snapshot,
                &snapshot.path,
                self.workspace_root(),
                &contract_linearization,
                &modifier_lookup,
                target_offset,
                get_source,
                &self.resolver,
            )
        })
    }

    /// Resolve the reference target at a specific position in a file source.
    pub fn reference_target_at_position(
        &self,
        path: &Path,
        source: &str,
        position: Position,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<ReferenceTarget> {
        let snapshot = self.snapshot_for_source(path, source)?;
        let offset = position_to_offset(source, position);
        resolve_reference_target_at_offset(&snapshot, offset, get_source, &self.resolver)
    }

    /// Build a safe rename plan for the symbol at `position`.
    ///
    /// The current implementation is deliberately conservative: it only returns
    /// a plan when every resolved occurrence can be updated mechanically
    /// without guessing alias semantics.
    pub fn rename_plan(
        &self,
        path: &Path,
        source: &str,
        position: Position,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<RenamePlan> {
        let snapshot = self.snapshot_for_source(path, source)?;
        let offset = position_to_offset(source, position);
        let (_name, current_span) = symbols::find_ident_at_offset(source, offset)?;
        let target =
            resolve_reference_target_at_offset(&snapshot, offset, get_source, &self.resolver)?;
        let current_path = normalize_path(path);
        let locations = self
            .find_references_for_target(
                &target,
                Some((current_path.clone(), source)),
                true,
                get_source,
            )
            .into_iter()
            .filter(|location| {
                rename_location_matches_target_name(
                    location,
                    &target.name,
                    &current_path,
                    source,
                    get_source,
                )
            })
            .collect::<Vec<_>>();
        if locations.is_empty() {
            return None;
        }
        let has_declaration = locations.iter().any(|location| {
            location
                .uri
                .to_file_path()
                .map(|path| normalize_path(path.as_ref()))
                .as_deref()
                == Some(current_path.as_path())
                && location.range == span_to_range(source, &current_span)
        });
        if !has_declaration {
            return None;
        }

        Some(RenamePlan {
            range: span_to_range(source, &current_span),
            placeholder: target.name,
            locations,
        })
    }

    /// Prepare a conservative call hierarchy item for the symbol at `position`.
    pub fn prepare_call_hierarchy(
        &self,
        path: &Path,
        source: &str,
        position: Position,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<CallHierarchyEntry> {
        let snapshot = self.snapshot_for_source(path, source)?;
        let offset = position_to_offset(source, position);

        if let Some(callable) = snapshot
            .callables
            .iter()
            .find(|callable| callable.lens_span.contains(&offset))
        {
            return Some(call_hierarchy_entry_from_decl(&snapshot, callable));
        }

        if let Some(target) =
            callable_target_at_offset(&snapshot, offset, get_source, &self.resolver)
        {
            if let Some((target_snapshot, callable)) =
                self.callable_decl_for_target(&target, get_source)
            {
                return Some(call_hierarchy_entry_from_decl(&target_snapshot, &callable));
            }
        }
        None
    }

    /// Resolve outgoing calls for the prepared call hierarchy item at `target_offset`.
    pub fn outgoing_call_hierarchy(
        &self,
        path: &Path,
        source: &str,
        target_offset: usize,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Vec<OutgoingCallEntry> {
        let Some(snapshot) = self.snapshot_for_source(path, source) else {
            return Vec::new();
        };
        let Some(callable) = snapshot
            .callables
            .iter()
            .find(|callable| callable.target_offset == target_offset)
            .cloned()
        else {
            return Vec::new();
        };

        let outgoing =
            collect_outgoing_calls_for_callable(&snapshot, &callable, get_source, &self.resolver);
        let mut grouped = HashMap::<(PathBuf, usize), (CallHierarchyEntry, Vec<Range>)>::new();
        for call in outgoing {
            let Some((target_snapshot, target_callable)) =
                self.callable_decl_for_target(&call.target, get_source)
            else {
                continue;
            };
            let entry = call_hierarchy_entry_from_decl(&target_snapshot, &target_callable);
            grouped
                .entry((entry.path.clone(), entry.target_offset))
                .or_insert_with(|| (entry, Vec::new()))
                .1
                .push(call.from_range);
        }

        let mut result = grouped
            .into_values()
            .map(|(to, mut from_ranges)| {
                from_ranges.sort_by_key(|range| (range.start.line, range.start.character));
                OutgoingCallEntry { to, from_ranges }
            })
            .collect::<Vec<_>>();
        result.sort_by(|left, right| {
            left.to
                .path
                .cmp(&right.to.path)
                .then_with(|| {
                    left.to
                        .selection_range
                        .start
                        .line
                        .cmp(&right.to.selection_range.start.line)
                })
                .then_with(|| {
                    left.to
                        .selection_range
                        .start
                        .character
                        .cmp(&right.to.selection_range.start.character)
                })
        });
        result
    }

    /// Resolve incoming calls for the prepared call hierarchy item at `target_offset`.
    pub fn incoming_call_hierarchy(
        &self,
        path: &Path,
        source: &str,
        target_offset: usize,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Vec<IncomingCallEntry> {
        let Some(snapshot) = self.snapshot_for_source(path, source) else {
            return Vec::new();
        };
        let Some(target_callable) = snapshot
            .callables
            .iter()
            .find(|callable| callable.target_offset == target_offset)
            .cloned()
        else {
            return Vec::new();
        };
        let target_entry = call_hierarchy_entry_from_decl(&snapshot, &target_callable);

        let mut candidate_paths = self.files.keys().cloned().collect::<Vec<_>>();
        if !candidate_paths
            .iter()
            .any(|candidate| candidate == &snapshot.path)
        {
            candidate_paths.push(snapshot.path.clone());
        }
        candidate_paths.sort();
        candidate_paths.dedup();
        let current_file = (snapshot.path.clone(), source);

        let mut grouped = HashMap::<(PathBuf, usize), (CallHierarchyEntry, Vec<Range>)>::new();
        for candidate_path in candidate_paths {
            let Some(candidate_snapshot) =
                self.candidate_snapshot(&candidate_path, Some(&current_file), get_source)
            else {
                continue;
            };

            for caller in &candidate_snapshot.callables {
                let outgoing = collect_outgoing_calls_for_callable(
                    &candidate_snapshot,
                    caller,
                    get_source,
                    &self.resolver,
                );
                let matching_ranges = outgoing
                    .into_iter()
                    .filter_map(|call| {
                        let target_callable =
                            self.callable_decl_for_target(&call.target, get_source)?;
                        let entry =
                            call_hierarchy_entry_from_decl(&target_callable.0, &target_callable.1);
                        ((entry.path == target_entry.path)
                            && (entry.target_offset == target_entry.target_offset))
                            .then_some(call.from_range)
                    })
                    .collect::<Vec<_>>();
                if matching_ranges.is_empty() {
                    continue;
                }

                let caller_entry = call_hierarchy_entry_from_decl(&candidate_snapshot, caller);
                grouped
                    .entry((caller_entry.path.clone(), caller_entry.target_offset))
                    .or_insert_with(|| (caller_entry, Vec::new()))
                    .1
                    .extend(matching_ranges);
            }
        }

        let mut result = grouped
            .into_values()
            .map(|(from, mut from_ranges)| {
                from_ranges.sort_by_key(|range| (range.start.line, range.start.character));
                IncomingCallEntry { from, from_ranges }
            })
            .collect::<Vec<_>>();
        result.sort_by(|left, right| {
            left.from
                .path
                .cmp(&right.from.path)
                .then_with(|| {
                    left.from
                        .selection_range
                        .start
                        .line
                        .cmp(&right.from.selection_range.start.line)
                })
                .then_with(|| {
                    left.from
                        .selection_range
                        .start
                        .character
                        .cmp(&right.from.selection_range.start.character)
                })
        });
        result
    }

    fn rebuild_from_workspace(&mut self) {
        self.invalidate_reference_cache();
        self.files.clear();
        self.symbols.clear();

        let Some(root) = self.workspace_root.clone() else {
            return;
        };

        let mut paths = discover_solidity_files(&root);
        paths.sort();
        for path in paths {
            if let Ok(source) = std::fs::read_to_string(&path) {
                self.update_file(&path, &source);
            }
        }
    }

    fn callable_decl_for_target(
        &self,
        target: &ReferenceTarget,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<(ProjectSnapshot, CallableDecl)> {
        if !is_call_hierarchy_kind(target.kind) {
            return None;
        }
        let snapshot = self.candidate_snapshot(&target.file_path, None, get_source)?;
        let callable = snapshot
            .callables
            .iter()
            .find(|callable| {
                callable.kind == target.kind
                    && callable.def_span == target.def_span
                    && (callable.kind == SymbolKind::Constructor
                        || callable.lens_span == target.name_span)
            })
            .cloned()?;
        Some((snapshot, callable))
    }

    fn collect_import_graph(
        &self,
        snapshot: &ProjectSnapshot,
        get_source: &dyn Fn(&Path) -> Option<String>,
        visited: &mut HashSet<PathBuf>,
        nodes: &mut HashMap<String, GraphNode>,
        edges: &mut HashSet<GraphEdge>,
    ) {
        if !visited.insert(snapshot.path.clone()) {
            return;
        }

        let node_id = file_graph_node_id(&snapshot.path);
        nodes
            .entry(node_id.clone())
            .or_insert_with(|| file_graph_node(snapshot.path.clone(), self.workspace_root()));

        for import in &snapshot.table.imports {
            let Some(resolved) = self.resolver.resolve(&import.path, &snapshot.path) else {
                continue;
            };
            let resolved = normalize_path(&resolved);
            let target_id = file_graph_node_id(&resolved);
            nodes
                .entry(target_id.clone())
                .or_insert_with(|| file_graph_node(resolved.clone(), self.workspace_root()));
            edges.insert(GraphEdge {
                from: node_id.clone(),
                to: target_id,
                label: Some("imports".to_string()),
                kind: Some(GraphEdgeKind::Imports),
            });

            let Some(next_snapshot) = self.candidate_snapshot(&resolved, None, get_source) else {
                continue;
            };
            self.collect_import_graph(&next_snapshot, get_source, visited, nodes, edges);
        }
    }

    fn collect_inheritance_graph(
        &self,
        contract: &ResolvedContract,
        get_source: &dyn Fn(&Path) -> Option<String>,
        visited: &mut HashSet<(PathBuf, usize)>,
        nodes: &mut HashMap<String, GraphNode>,
        edges: &mut HashSet<GraphEdge>,
    ) {
        let visit_key = (contract.path.clone(), contract.decl.name_span.start);
        if !visited.insert(visit_key) {
            return;
        }

        let node_id = contract_graph_node_id(&contract.path, &contract.decl.name_span);
        nodes.entry(node_id.clone()).or_insert_with(|| {
            contract_graph_node(&contract.path, &contract.decl, self.workspace_root())
        });

        let Some(snapshot) = self.candidate_snapshot(&contract.path, None, get_source) else {
            return;
        };

        for base in &contract.decl.bases {
            let Some(resolved_base) = self.resolve_contract_decl_path(&snapshot, base, get_source)
            else {
                continue;
            };
            let target_id =
                contract_graph_node_id(&resolved_base.path, &resolved_base.decl.name_span);
            nodes.entry(target_id.clone()).or_insert_with(|| {
                contract_graph_node(
                    &resolved_base.path,
                    &resolved_base.decl,
                    self.workspace_root(),
                )
            });
            edges.insert(GraphEdge {
                from: node_id.clone(),
                to: target_id,
                label: Some("inherits".to_string()),
                kind: Some(GraphEdgeKind::Inherits),
            });
            self.collect_inheritance_graph(&resolved_base, get_source, visited, nodes, edges);
        }
    }

    fn candidate_snapshot(
        &self,
        path: &Path,
        current_file: Option<&(PathBuf, &str)>,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<ProjectSnapshot> {
        let normalized = normalize_path(path);
        if let Some((current_path, current_source)) = current_file {
            if *current_path == normalized {
                return self.snapshot_for_source(&normalized, current_source);
            }
        }

        if let Some(source) = get_source(&normalized) {
            if let Some(snapshot) = self.snapshot_for_source(&normalized, &source) {
                return Some(snapshot);
            }
        }

        if let Some(file) = self.files.get(&normalized) {
            return Some(file.snapshot.clone());
        }

        None
    }

    fn resolve_contract_decl_path(
        &self,
        snapshot: &ProjectSnapshot,
        path: &TypePath,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<ResolvedContract> {
        self.resolve_path_target(snapshot, path, get_source)
            .and_then(|resolved| {
                find_contract_decl(&resolved.snapshot, &resolved.def)
                    .cloned()
                    .map(|decl| ResolvedContract {
                        path: resolved.snapshot.path,
                        decl,
                    })
            })
    }

    fn resolve_path_target(
        &self,
        snapshot: &ProjectSnapshot,
        path: &TypePath,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<ResolvedPathTarget> {
        if path.segments.is_empty() {
            return None;
        }

        if path.segments.len() >= 2 {
            if let Some(namespace_target) =
                self.resolve_namespace_path_target(snapshot, path, get_source)
            {
                return Some(namespace_target);
            }
        }

        let resolve_offset = 0;
        for def in snapshot
            .table
            .resolve_all(&path.segments[0], resolve_offset)
        {
            if let Some(resolved) =
                self.resolve_member_chain_target(snapshot.clone(), def.clone(), &path.segments[1..])
            {
                return Some(resolved);
            }
        }

        if let Some(cross_file) = resolve_cross_file_symbol(
            &snapshot.table,
            &path.segments[0],
            &snapshot.path,
            get_source,
            &self.resolver,
        ) {
            let cross_snapshot = ProjectSnapshot {
                path: cross_file.resolved_path.clone(),
                source: cross_file.source.clone(),
                table: cross_file.table.clone(),
                contracts: collect_contract_declarations(
                    &cross_file.source,
                    &cross_file.resolved_path.to_string_lossy(),
                ),
                callables: collect_callable_declarations(
                    &cross_file.source,
                    &cross_file.resolved_path.to_string_lossy(),
                ),
            };
            return self.resolve_member_chain_target(
                cross_snapshot,
                cross_file.def.clone(),
                &path.segments[1..],
            );
        }

        None
    }

    fn resolve_namespace_path_target(
        &self,
        snapshot: &ProjectSnapshot,
        path: &TypePath,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<ResolvedPathTarget> {
        let namespace = &path.segments[0];
        for import in &snapshot.table.imports {
            let matches_namespace = match &import.symbols {
                ImportedSymbols::Plain(Some(alias)) | ImportedSymbols::Glob(alias) => {
                    alias == namespace
                }
                ImportedSymbols::Plain(None) | ImportedSymbols::Named(_) => false,
            };
            if !matches_namespace {
                continue;
            }

            let Some(resolved) = self.resolver.resolve(&import.path, &snapshot.path) else {
                continue;
            };
            let Some(imported_snapshot) = self.candidate_snapshot(&resolved, None, get_source)
            else {
                continue;
            };
            let Some(def) = imported_snapshot
                .table
                .resolve(&path.segments[1], 0)
                .cloned()
            else {
                continue;
            };
            if let Some(resolved) =
                self.resolve_member_chain_target(imported_snapshot, def, &path.segments[2..])
            {
                return Some(resolved);
            }
        }

        None
    }

    fn resolve_member_chain_target(
        &self,
        snapshot: ProjectSnapshot,
        root: SymbolDef,
        remaining: &[String],
    ) -> Option<ResolvedPathTarget> {
        let mut current = root;
        for segment in remaining {
            let next = snapshot.table.resolve_member(&current, segment)?.clone();
            current = next;
        }
        Some(ResolvedPathTarget {
            snapshot,
            def: current,
        })
    }

    fn nearest_inherited_member_origins(
        &self,
        contract: &ResolvedContract,
        key: &InheritedMemberKey,
        current_file: Option<&(PathBuf, &str)>,
        get_source: &dyn Fn(&Path) -> Option<String>,
        visited: &mut HashSet<(PathBuf, usize)>,
    ) -> Vec<ResolvedContract> {
        let visit_key = resolved_contract_key(contract);
        if !visited.insert(visit_key) {
            return Vec::new();
        }

        let Some(snapshot) = self.candidate_snapshot(&contract.path, current_file, get_source)
        else {
            return Vec::new();
        };

        let mut origins = Vec::new();
        let mut seen = HashSet::new();
        for base in &contract.decl.bases {
            let Some(resolved_base) = self.resolve_contract_decl_path(&snapshot, base, get_source)
            else {
                continue;
            };

            let branch_origins =
                if self.contract_declares_member(&resolved_base, key, current_file, get_source) {
                    vec![resolved_base]
                } else {
                    self.nearest_inherited_member_origins(
                        &resolved_base,
                        key,
                        current_file,
                        get_source,
                        visited,
                    )
                };

            for origin in branch_origins {
                let origin_key = resolved_contract_key(&origin);
                if seen.insert(origin_key) {
                    origins.push(origin);
                }
            }
        }

        origins
    }

    fn contract_inheritance_hint(
        &self,
        contract: &ResolvedContract,
        offset: usize,
        current_file: Option<&(PathBuf, &str)>,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<InheritanceHint> {
        let mut cache = HashMap::new();
        let mut active = HashSet::new();
        let linearized = self.linearized_inheritance_order(
            contract,
            current_file,
            get_source,
            &mut cache,
            &mut active,
        )?;
        if linearized.len() <= 1 {
            return None;
        }

        let precedence = linearized
            .iter()
            .skip(1)
            .map(|resolved| resolved.decl.name.clone())
            .collect::<Vec<_>>();
        let direct_bases = contract
            .decl
            .bases
            .iter()
            .map(TypePath::as_display)
            .collect::<Vec<_>>();
        let full_order = linearized
            .iter()
            .map(|resolved| resolved.decl.name.clone())
            .collect::<Vec<_>>();

        Some(InheritanceHint {
            offset,
            label: format!("linearized: {}", precedence.join(" -> ")),
            tooltip: format!(
                "Direct bases for `{}`: {}. Linearized precedence: {}",
                contract.decl.name,
                direct_bases.join(", "),
                full_order.join(" -> ")
            ),
        })
    }

    fn contract_inherited_members_hint(
        &self,
        contract: &ResolvedContract,
        offset: usize,
        current_file: Option<&(PathBuf, &str)>,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Option<InheritanceHint> {
        let surfaces =
            self.accessible_inherited_member_surfaces(contract, current_file, get_source);
        if surfaces.is_empty() {
            return None;
        }

        let preview = surfaces
            .iter()
            .take(3)
            .map(|surface| surface.member_name.clone())
            .collect::<Vec<_>>();
        let label = if surfaces.len() > preview.len() {
            format!(
                "inherits members: {} (+{} more)",
                preview.join(", "),
                surfaces.len() - preview.len()
            )
        } else {
            format!("inherits members: {}", preview.join(", "))
        };

        let details = surfaces
            .iter()
            .map(|surface| {
                format!(
                    "{}.{} ({})",
                    surface.origin, surface.member_name, surface.kind_label
                )
            })
            .collect::<Vec<_>>();

        Some(InheritanceHint {
            offset,
            label,
            tooltip: format!(
                "Accessible inherited members for `{}`: {}",
                contract.decl.name,
                details.join(", ")
            ),
        })
    }

    fn contract_declares_member(
        &self,
        contract: &ResolvedContract,
        key: &InheritedMemberKey,
        current_file: Option<&(PathBuf, &str)>,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> bool {
        let Some(snapshot) = self.candidate_snapshot(&contract.path, current_file, get_source)
        else {
            return false;
        };
        let Some(contract_def) = find_contract_symbol_def(&snapshot, &contract.decl) else {
            return false;
        };
        let Some(scope_id) = contract_def.scope else {
            return false;
        };

        snapshot
            .table
            .scope_symbols(scope_id)
            .iter()
            .any(|member| inherited_member_key(member).as_ref() == Some(key))
    }

    fn accessible_inherited_member_surfaces(
        &self,
        contract: &ResolvedContract,
        current_file: Option<&(PathBuf, &str)>,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Vec<InheritedMemberSurface> {
        let Some(snapshot) = self.candidate_snapshot(&contract.path, current_file, get_source)
        else {
            return Vec::new();
        };
        let Some(contract_def) = find_contract_symbol_def(&snapshot, &contract.decl) else {
            return Vec::new();
        };
        let Some(scope_id) = contract_def.scope else {
            return Vec::new();
        };

        let mut cache = HashMap::new();
        let mut active = HashSet::new();
        let Some(linearized) = self.linearized_inheritance_order(
            contract,
            current_file,
            get_source,
            &mut cache,
            &mut active,
        ) else {
            return Vec::new();
        };
        if linearized.len() <= 1 {
            return Vec::new();
        }

        let mut seen = snapshot
            .table
            .scope_symbols(scope_id)
            .iter()
            .filter_map(inherited_surface_key)
            .collect::<HashSet<_>>();
        let mut surfaces = Vec::new();

        for base in linearized.iter().skip(1) {
            let Some(base_snapshot) = self.candidate_snapshot(&base.path, current_file, get_source)
            else {
                continue;
            };
            let Some(base_def) = find_contract_symbol_def(&base_snapshot, &base.decl) else {
                continue;
            };
            let Some(base_scope_id) = base_def.scope else {
                continue;
            };

            for member in base_snapshot.table.scope_symbols(base_scope_id) {
                let Some(key) = inherited_surface_key(member) else {
                    continue;
                };
                if !member_is_inheritable(member) || !seen.insert(key) {
                    continue;
                }
                surfaces.push(InheritedMemberSurface {
                    origin: base.decl.name.clone(),
                    member_name: member.name.clone(),
                    kind_label: inherited_surface_kind_label(member.kind),
                });
            }
        }

        surfaces
    }

    // Solidity gives precedence to the rightmost direct base, so we run C3
    // linearization over the reversed direct-base list.
    fn linearized_inheritance_order(
        &self,
        contract: &ResolvedContract,
        current_file: Option<&(PathBuf, &str)>,
        get_source: &dyn Fn(&Path) -> Option<String>,
        cache: &mut HashMap<(PathBuf, usize), Vec<ResolvedContract>>,
        active: &mut HashSet<(PathBuf, usize)>,
    ) -> Option<Vec<ResolvedContract>> {
        let key = resolved_contract_key(contract);
        if let Some(cached) = cache.get(&key) {
            return Some(cached.clone());
        }
        if !active.insert(key.clone()) {
            return None;
        }

        let Some(snapshot) = self.candidate_snapshot(&contract.path, current_file, get_source)
        else {
            active.remove(&key);
            return None;
        };

        let mut direct_bases = Vec::new();
        for base in &contract.decl.bases {
            let Some(resolved) = self.resolve_contract_decl_path(&snapshot, base, get_source)
            else {
                active.remove(&key);
                return None;
            };
            direct_bases.push(resolved);
        }
        direct_bases.reverse();

        let mut sequences = Vec::new();
        for base in &direct_bases {
            let Some(linearized_base) =
                self.linearized_inheritance_order(base, current_file, get_source, cache, active)
            else {
                active.remove(&key);
                return None;
            };
            sequences.push(linearized_base);
        }
        sequences.push(direct_bases.clone());

        let Some(merged) = merge_linearized_contracts(sequences) else {
            active.remove(&key);
            return None;
        };

        let mut result = Vec::with_capacity(1 + merged.len());
        result.push(contract.clone());
        result.extend(merged);

        active.remove(&key);
        cache.insert(key, result.clone());
        Some(result)
    }

    fn find_references_for_target(
        &self,
        target: &ReferenceTarget,
        current_file: Option<(PathBuf, &str)>,
        include_declaration: bool,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Vec<Location> {
        let cacheable = self.reference_cache_usable(current_file.as_ref());
        let cached = cacheable
            .then(|| self.cached_references_for_target(target))
            .flatten();
        let result = match cached {
            Some(result) => result,
            None => {
                let result =
                    self.compute_references_for_target(target, current_file.as_ref(), get_source);
                if cacheable {
                    self.store_references_for_target(target, result.clone());
                }
                result
            }
        };

        if include_declaration {
            result.locations
        } else {
            result
                .locations
                .into_iter()
                .filter(|location| result.declaration.as_ref() != Some(location))
                .collect()
        }
    }

    fn compute_references_for_target(
        &self,
        target: &ReferenceTarget,
        current_file: Option<&(PathBuf, &str)>,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> ReferenceSearchResult {
        let mut candidate_paths = self.files.keys().cloned().collect::<Vec<_>>();
        if !candidate_paths.iter().any(|path| path == &target.file_path) {
            candidate_paths.push(target.file_path.clone());
        }
        if let Some((current_path, _)) = current_file {
            if !candidate_paths.iter().any(|path| path == current_path) {
                candidate_paths.push(current_path.clone());
            }
        }
        candidate_paths.sort();
        candidate_paths.dedup();

        let mut locations = Vec::new();
        let mut declaration = None;
        let mut seen = HashSet::new();

        for path in candidate_paths {
            let Some(snapshot) = self.candidate_snapshot(&path, current_file, get_source) else {
                continue;
            };

            for name in reference_scan_names(&snapshot, target, get_source, &self.resolver) {
                let spans = find_identifier_occurrences(&snapshot.source, &name)
                    .into_iter()
                    .chain(find_natspec_identifier_occurrences(&snapshot.source, &name));
                for span in spans {
                    let Some(resolved_target) = resolve_reference_target_at_offset(
                        &snapshot,
                        span.start,
                        get_source,
                        &self.resolver,
                    ) else {
                        continue;
                    };

                    if resolved_target != *target {
                        continue;
                    }

                    if seen.insert((snapshot.path.clone(), span.clone())) {
                        if let Some(uri) = path_to_uri(&snapshot.path) {
                            let location = Location {
                                uri,
                                range: span_to_range(&snapshot.source, &span),
                            };
                            if snapshot.path == target.file_path && span == target.name_span {
                                declaration = Some(location.clone());
                            }
                            locations.push(location);
                        }
                    }
                }
            }
        }

        locations.sort_by(|left, right| {
            left.uri
                .as_str()
                .cmp(right.uri.as_str())
                .then_with(|| left.range.start.line.cmp(&right.range.start.line))
                .then_with(|| left.range.start.character.cmp(&right.range.start.character))
        });
        ReferenceSearchResult {
            locations,
            declaration,
        }
    }

    fn compute_references_for_targets_batch(
        &self,
        targets: &[ReferenceTarget],
        current_file: Option<&(PathBuf, &str)>,
        get_source: &dyn Fn(&Path) -> Option<String>,
    ) -> Vec<ReferenceSearchResult> {
        if targets.is_empty() {
            return Vec::new();
        }

        let mut candidate_paths = self.files.keys().cloned().collect::<Vec<_>>();
        candidate_paths.extend(targets.iter().map(|target| target.file_path.clone()));
        if let Some((current_path, _)) = current_file {
            candidate_paths.push(current_path.clone());
        }
        candidate_paths.sort();
        candidate_paths.dedup();

        let mut locations = vec![Vec::new(); targets.len()];
        let mut declarations = vec![None; targets.len()];
        let mut seen = vec![HashSet::new(); targets.len()];

        for path in candidate_paths {
            let Some(snapshot) = self.candidate_snapshot(&path, current_file, get_source) else {
                continue;
            };
            let mut candidates_by_name = HashMap::<String, Vec<usize>>::new();
            for (target_index, target) in targets.iter().enumerate() {
                for name in reference_scan_names(&snapshot, target, get_source, &self.resolver) {
                    candidates_by_name
                        .entry(name)
                        .or_default()
                        .push(target_index);
                }
            }

            let mut spans = find_all_identifier_occurrences(&snapshot.source);
            for name in candidates_by_name.keys() {
                spans.extend(find_natspec_identifier_occurrences(&snapshot.source, name));
            }
            spans.sort_by_key(|span| span.start);
            spans.dedup();

            for span in spans {
                let name = &snapshot.source[span.clone()];
                let Some(candidate_indices) = candidates_by_name.get(name) else {
                    continue;
                };
                let Some(resolved_target) = resolve_reference_target_at_offset(
                    &snapshot,
                    span.start,
                    get_source,
                    &self.resolver,
                ) else {
                    continue;
                };

                for &target_index in candidate_indices {
                    let target = &targets[target_index];
                    if resolved_target != *target
                        || !seen[target_index].insert((snapshot.path.clone(), span.clone()))
                    {
                        continue;
                    }
                    let Some(uri) = path_to_uri(&snapshot.path) else {
                        continue;
                    };
                    let location = Location {
                        uri,
                        range: span_to_range(&snapshot.source, &span),
                    };
                    if snapshot.path == target.file_path && span == target.name_span {
                        declarations[target_index] = Some(location.clone());
                    }
                    locations[target_index].push(location);
                }
            }
        }

        locations
            .into_iter()
            .zip(declarations)
            .map(|(mut locations, declaration)| {
                locations.sort_by(|left, right| {
                    left.uri
                        .as_str()
                        .cmp(right.uri.as_str())
                        .then_with(|| left.range.start.line.cmp(&right.range.start.line))
                        .then_with(|| left.range.start.character.cmp(&right.range.start.character))
                });
                ReferenceSearchResult {
                    locations,
                    declaration,
                }
            })
            .collect()
    }

    fn cached_references_for_target(
        &self,
        target: &ReferenceTarget,
    ) -> Option<ReferenceSearchResult> {
        let key = ReferenceCacheKey {
            generation: self.reference_generation,
            target: target.clone(),
        };
        self.reference_cache.lock().ok()?.get(&key).cloned()
    }

    fn store_references_for_target(&self, target: &ReferenceTarget, result: ReferenceSearchResult) {
        let key = ReferenceCacheKey {
            generation: self.reference_generation,
            target: target.clone(),
        };
        if let Ok(mut cache) = self.reference_cache.lock() {
            cache.insert(key, result);
        }
    }

    fn reference_cache_usable(&self, current_file: Option<&(PathBuf, &str)>) -> bool {
        let Some((path, source)) = current_file else {
            return true;
        };
        self.files
            .get(&normalize_path(path))
            .is_some_and(|file| file.snapshot.source == *source)
    }

    fn invalidate_reference_cache(&mut self) {
        self.reference_generation = self.reference_generation.wrapping_add(1);
        if let Ok(mut cache) = self.reference_cache.lock() {
            cache.clear();
        }
    }
}

impl<'a> ControlFlowBuilder<'a> {
    fn new(
        path: &'a Path,
        workspace_root: Option<&'a Path>,
        callable: &CallableDecl,
        header_detail: String,
    ) -> Self {
        let prefix = format!(
            "cfg:{}:{}",
            file_graph_node_id(path),
            callable.target_offset
        );
        let entry_id = format!("{prefix}:entry");
        let exit_id = format!("{prefix}:exit");
        let uri = path_to_uri(path).map(|uri| uri.to_string());
        let nodes = vec![GraphNode {
            id: entry_id.clone(),
            label: "Entry".to_string(),
            detail: header_detail,
            kind: Some(GraphNodeKind::Entry),
            uri,
        }];

        Self {
            path,
            workspace_root,
            prefix,
            nodes,
            edges: Vec::new(),
            next_node_index: 0,
            entry_id,
            exit_id,
        }
    }

    fn add_node(&mut self, label: String, detail: String, kind: GraphNodeKind) -> String {
        let id = format!("{}:n{}", self.prefix, self.next_node_index);
        self.next_node_index += 1;
        self.nodes.push(GraphNode {
            id: id.clone(),
            label,
            detail,
            kind: Some(kind),
            uri: path_to_uri(self.path).map(|uri| uri.to_string()),
        });
        id
    }

    fn add_edge(
        &mut self,
        from: &str,
        to: &str,
        label: Option<String>,
        kind: Option<GraphEdgeKind>,
    ) {
        self.edges.push(GraphEdge {
            from: from.to_string(),
            to: to.to_string(),
            label,
            kind,
        });
    }

    fn connect_pending(&mut self, edges: Vec<PendingEdge>, target: &str) {
        for edge in edges {
            self.add_edge(&edge.from, target, edge.label, edge.kind);
        }
    }

    fn finalize(mut self, title: String) -> GraphDocument {
        self.nodes.push(GraphNode {
            id: self.exit_id.clone(),
            label: "Exit".to_string(),
            detail: format!(
                "Flow leaves {}",
                display_path(self.path, self.workspace_root)
            ),
            kind: Some(GraphNodeKind::Exit),
            uri: path_to_uri(self.path).map(|uri| uri.to_string()),
        });
        GraphDocument {
            kind: GraphKind::ControlFlow,
            title,
            nodes: self.nodes,
            edges: self.edges,
            focus_node_id: Some(self.entry_id),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_control_flow_graph_from_source_unit<'ast>(
    source_unit: &'ast solar_ast::SourceUnit<'ast>,
    source: &str,
    snapshot: &ProjectSnapshot,
    path: &Path,
    workspace_root: Option<&Path>,
    contract_linearization: &HashMap<String, Vec<ResolvedContractRef>>,
    modifier_lookup: &HashMap<ModifierLookupKey, ModifierLookupEntry<'ast>>,
    target_offset: usize,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<GraphDocument> {
    struct ControlFlowSearch<'a, 'ast> {
        source: &'a str,
        snapshot: &'a ProjectSnapshot,
        path: &'a Path,
        workspace_root: Option<&'a Path>,
        contract_stack: Vec<String>,
        contract_linearization: &'a HashMap<String, Vec<ResolvedContractRef>>,
        modifier_lookup: &'a HashMap<ModifierLookupKey, ModifierLookupEntry<'ast>>,
        target_offset: usize,
        get_source: &'a dyn Fn(&Path) -> Option<String>,
        resolver: &'a ImportResolver,
    }

    impl<'a, 'ast> ControlFlowSearch<'a, 'ast> {
        fn visit_item(&mut self, item: &'ast solar_ast::Item<'ast>) -> Option<GraphDocument> {
            match &item.kind {
                ItemKind::Contract(contract) => {
                    self.contract_stack.push(contract.name.as_str().to_string());
                    for body_item in contract.body.iter() {
                        if let Some(graph) = self.visit_item(body_item) {
                            self.contract_stack.pop();
                            return Some(graph);
                        }
                    }
                    self.contract_stack.pop();
                    None
                }
                ItemKind::Function(function) => {
                    let callable = callable_declaration(
                        self.source,
                        &self.contract_stack,
                        solgrid_ast::span_to_range(item.span),
                        function,
                    )?;
                    (callable.target_offset == self.target_offset).then(|| {
                        let modifier_plans = resolve_modifier_plans(
                            self.source,
                            self.snapshot,
                            self.path,
                            self.contract_stack.last().map(String::as_str),
                            function,
                            self.contract_linearization,
                            self.modifier_lookup,
                            self.get_source,
                            self.resolver,
                        );
                        build_control_flow_graph_document(
                            self.source,
                            self.path,
                            self.workspace_root,
                            &callable,
                            function,
                            &modifier_plans,
                        )
                    })
                }
                _ => None,
            }
        }
    }

    let mut search = ControlFlowSearch {
        source,
        snapshot,
        path,
        workspace_root,
        contract_stack: Vec::new(),
        contract_linearization,
        modifier_lookup,
        target_offset,
        get_source,
        resolver,
    };

    for item in source_unit.items.iter() {
        if let Some(graph) = search.visit_item(item) {
            return Some(graph);
        }
    }

    None
}

fn build_control_flow_graph_document<'ast>(
    source: &str,
    path: &Path,
    workspace_root: Option<&Path>,
    callable: &CallableDecl,
    function: &'ast ItemFunction<'ast>,
    modifiers: &[ModifierPlan<'ast>],
) -> GraphDocument {
    let header_detail = format!(
        "{} {}",
        callable.kind_label,
        normalize_graph_text(&source[solgrid_ast::span_to_range(function.header.span)])
    );
    let mut builder = ControlFlowBuilder::new(path, workspace_root, callable, header_detail);
    let body = function
        .body
        .as_ref()
        .expect("implemented callable must have a body");
    let entry_id = builder.entry_id.clone();
    let exit_id = builder.exit_id.clone();

    if let Some(segment) = build_callable_flow(&mut builder, source, 0, body.stmts, modifiers, None)
    {
        builder.add_edge(&entry_id, &segment.entry, None, Some(GraphEdgeKind::Normal));
        builder.connect_pending(segment.fallthroughs, &exit_id);
        builder.connect_pending(segment.returns, &exit_id);
        builder.connect_pending(segment.breaks, &exit_id);
        builder.connect_pending(segment.continues, &exit_id);
    } else {
        builder.add_edge(&entry_id, &exit_id, None, Some(GraphEdgeKind::Normal));
    }

    builder.finalize(format!("Control-flow graph for {}", callable.label))
}

fn build_callable_flow<'ast>(
    builder: &mut ControlFlowBuilder<'_>,
    body_source: &str,
    body_source_base: usize,
    body_stmts: &'ast [Stmt<'ast>],
    modifiers: &[ModifierPlan<'ast>],
    loop_context: Option<&LoopContext>,
) -> Option<FlowSegment> {
    let Some((modifier, remaining_modifiers)) = modifiers.split_first() else {
        return build_block_flow(
            builder,
            body_source,
            body_source_base,
            body_stmts,
            loop_context,
            None,
        );
    };

    let modifier_node = builder.add_node(
        format!("modifier {}", modifier.label),
        modifier.detail.clone(),
        GraphNodeKind::Modifier,
    );
    let next = if let Some(modifier_body_stmts) = modifier.body_stmts {
        let modifier_source = modifier.body_source.as_deref().unwrap_or(body_source);
        let modifier_source_base = modifier.body_source_base;
        build_block_flow(
            builder,
            modifier_source,
            modifier_source_base,
            modifier_body_stmts,
            loop_context,
            Some(PlaceholderExpansion {
                body_source,
                body_source_base,
                body_stmts,
                modifiers: remaining_modifiers,
            }),
        )
    } else {
        build_callable_flow(
            builder,
            body_source,
            body_source_base,
            body_stmts,
            remaining_modifiers,
            loop_context,
        )
    };

    match next {
        Some(next) => {
            builder.add_edge(
                &modifier_node,
                &next.entry,
                None,
                Some(GraphEdgeKind::Normal),
            );
            Some(FlowSegment {
                entry: modifier_node,
                fallthroughs: next.fallthroughs,
                returns: next.returns,
                breaks: next.breaks,
                continues: next.continues,
            })
        }
        None => Some(FlowSegment {
            entry: modifier_node.clone(),
            fallthroughs: vec![PendingEdge {
                from: modifier_node,
                label: None,
                kind: Some(GraphEdgeKind::Normal),
            }],
            returns: Vec::new(),
            breaks: Vec::new(),
            continues: Vec::new(),
        }),
    }
}

fn build_block_flow<'ast>(
    builder: &mut ControlFlowBuilder<'_>,
    source: &str,
    source_base: usize,
    stmts: &'ast [Stmt<'ast>],
    loop_context: Option<&LoopContext>,
    placeholder: Option<PlaceholderExpansion<'ast, '_, '_>>,
) -> Option<FlowSegment> {
    let mut segment = None;

    for stmt in stmts {
        let next = build_stmt_flow(
            builder,
            source,
            source_base,
            stmt,
            loop_context,
            placeholder,
        );
        segment = compose_flow_segments(builder, segment, next);
    }

    segment
}

fn compose_flow_segments(
    builder: &mut ControlFlowBuilder<'_>,
    left: Option<FlowSegment>,
    right: Option<FlowSegment>,
) -> Option<FlowSegment> {
    match (left, right) {
        (None, segment) | (segment, None) => segment,
        (Some(mut left), Some(right)) => {
            builder.connect_pending(left.fallthroughs, &right.entry);
            left.fallthroughs = right.fallthroughs;
            left.returns.extend(right.returns);
            left.breaks.extend(right.breaks);
            left.continues.extend(right.continues);
            Some(left)
        }
    }
}

fn retag_pending_edges(edges: Vec<PendingEdge>, kind: GraphEdgeKind) -> Vec<PendingEdge> {
    edges
        .into_iter()
        .map(|edge| PendingEdge {
            from: edge.from,
            label: edge.label,
            kind: Some(kind),
        })
        .collect()
}

fn build_stmt_flow<'ast>(
    builder: &mut ControlFlowBuilder<'_>,
    source: &str,
    source_base: usize,
    stmt: &'ast Stmt<'ast>,
    loop_context: Option<&LoopContext>,
    placeholder: Option<PlaceholderExpansion<'ast, '_, '_>>,
) -> Option<FlowSegment> {
    match &stmt.kind {
        StmtKind::Assembly(assembly) => {
            build_assembly_flow(builder, source, source_base, stmt, assembly, loop_context)
        }
        StmtKind::DeclSingle(_)
        | StmtKind::DeclMulti(_, _)
        | StmtKind::Expr(_)
        | StmtKind::Emit(_, _) => {
            let (label, detail, kind) = stmt_node_descriptor(source, source_base, stmt);
            Some(single_node_segment(builder, label, detail, kind))
        }
        StmtKind::Return(_) => {
            let node = builder.add_node(
                compact_stmt_label(source, source_base, stmt),
                stmt_snippet(source, source_base, stmt),
                GraphNodeKind::TerminalReturn,
            );
            Some(FlowSegment {
                entry: node.clone(),
                fallthroughs: Vec::new(),
                returns: vec![PendingEdge {
                    from: node,
                    label: Some("return".to_string()),
                    kind: Some(GraphEdgeKind::Return),
                }],
                breaks: Vec::new(),
                continues: Vec::new(),
            })
        }
        StmtKind::Revert(_, _) => {
            let node = builder.add_node(
                compact_stmt_label(source, source_base, stmt),
                stmt_snippet(source, source_base, stmt),
                GraphNodeKind::TerminalRevert,
            );
            let exit_id = builder.exit_id.clone();
            builder.add_edge(
                &node,
                &exit_id,
                Some("revert".to_string()),
                Some(GraphEdgeKind::Revert),
            );
            Some(FlowSegment {
                entry: node,
                fallthroughs: Vec::new(),
                returns: Vec::new(),
                breaks: Vec::new(),
                continues: Vec::new(),
            })
        }
        StmtKind::Break => {
            let node = builder.add_node(
                "break".to_string(),
                "break".to_string(),
                GraphNodeKind::ControlTransfer,
            );
            Some(FlowSegment {
                entry: node.clone(),
                fallthroughs: Vec::new(),
                returns: Vec::new(),
                breaks: vec![PendingEdge {
                    from: node,
                    label: Some("break".to_string()),
                    kind: Some(GraphEdgeKind::Break),
                }],
                continues: Vec::new(),
            })
        }
        StmtKind::Continue => {
            let node = builder.add_node(
                "continue".to_string(),
                "continue".to_string(),
                GraphNodeKind::ControlTransfer,
            );
            Some(FlowSegment {
                entry: node.clone(),
                fallthroughs: Vec::new(),
                returns: Vec::new(),
                breaks: Vec::new(),
                continues: vec![PendingEdge {
                    from: node,
                    label: Some("continue".to_string()),
                    kind: Some(GraphEdgeKind::Continue),
                }],
            })
        }
        StmtKind::Placeholder => match placeholder {
            Some(placeholder) => build_callable_flow(
                builder,
                placeholder.body_source,
                placeholder.body_source_base,
                placeholder.body_stmts,
                placeholder.modifiers,
                loop_context,
            )
            .map(|mut expanded| {
                // A Solidity return inside the wrapped function resumes at the
                // modifier continuation after `_`, allowing each modifier's
                // postlude to run before the callable exits.
                expanded.fallthroughs.append(&mut expanded.returns);
                expanded
            }),
            None => Some(single_node_segment(
                builder,
                "_".to_string(),
                "modifier placeholder".to_string(),
                GraphNodeKind::Modifier,
            )),
        },
        StmtKind::Block(block) => build_block_flow(
            builder,
            source,
            source_base,
            block.stmts,
            loop_context,
            placeholder,
        ),
        StmtKind::UncheckedBlock(block) => {
            let node = builder.add_node(
                "unchecked".to_string(),
                "unchecked block".to_string(),
                GraphNodeKind::Block,
            );
            if let Some(body) = build_block_flow(
                builder,
                source,
                source_base,
                block.stmts,
                loop_context,
                placeholder,
            ) {
                builder.add_edge(&node, &body.entry, None, Some(GraphEdgeKind::Normal));
                Some(FlowSegment {
                    entry: node,
                    fallthroughs: body.fallthroughs,
                    returns: body.returns,
                    breaks: body.breaks,
                    continues: body.continues,
                })
            } else {
                Some(FlowSegment {
                    entry: node.clone(),
                    fallthroughs: vec![PendingEdge {
                        from: node,
                        label: None,
                        kind: Some(GraphEdgeKind::Normal),
                    }],
                    returns: Vec::new(),
                    breaks: Vec::new(),
                    continues: Vec::new(),
                })
            }
        }
        StmtKind::If(cond, then_stmt, else_stmt) => {
            let condition = builder.add_node(
                format!("if {}", expr_snippet(source, source_base, cond)),
                range_snippet(source, source_base, solgrid_ast::span_to_range(stmt.span)),
                GraphNodeKind::Branch,
            );
            let then_flow = build_stmt_flow(
                builder,
                source,
                source_base,
                then_stmt,
                loop_context,
                placeholder,
            );
            let else_flow = else_stmt.as_deref().and_then(|stmt| {
                build_stmt_flow(
                    builder,
                    source,
                    source_base,
                    stmt,
                    loop_context,
                    placeholder,
                )
            });
            let mut fallthroughs = Vec::new();
            let mut returns = Vec::new();
            let mut breaks = Vec::new();
            let mut continues = Vec::new();

            if let Some(then_flow) = then_flow {
                builder.add_edge(
                    &condition,
                    &then_flow.entry,
                    Some("true".to_string()),
                    Some(GraphEdgeKind::BranchTrue),
                );
                fallthroughs.extend(then_flow.fallthroughs);
                returns.extend(then_flow.returns);
                breaks.extend(then_flow.breaks);
                continues.extend(then_flow.continues);
            } else {
                fallthroughs.push(PendingEdge {
                    from: condition.clone(),
                    label: Some("true".to_string()),
                    kind: Some(GraphEdgeKind::BranchTrue),
                });
            }

            if let Some(else_flow) = else_flow {
                builder.add_edge(
                    &condition,
                    &else_flow.entry,
                    Some("false".to_string()),
                    Some(GraphEdgeKind::BranchFalse),
                );
                fallthroughs.extend(else_flow.fallthroughs);
                returns.extend(else_flow.returns);
                breaks.extend(else_flow.breaks);
                continues.extend(else_flow.continues);
            } else {
                fallthroughs.push(PendingEdge {
                    from: condition.clone(),
                    label: Some("false".to_string()),
                    kind: Some(GraphEdgeKind::BranchFalse),
                });
            }

            Some(FlowSegment {
                entry: condition,
                fallthroughs,
                returns,
                breaks,
                continues,
            })
        }
        StmtKind::While(cond, body) => {
            let condition = builder.add_node(
                format!("while {}", expr_snippet(source, source_base, cond)),
                range_snippet(source, source_base, solgrid_ast::span_to_range(stmt.span)),
                GraphNodeKind::Loop,
            );
            let loop_context = LoopContext {
                _continue_target: condition.clone(),
            };
            let body_flow = build_stmt_flow(
                builder,
                source,
                source_base,
                body,
                Some(&loop_context),
                placeholder,
            );
            let mut fallthroughs = vec![PendingEdge {
                from: condition.clone(),
                label: Some("false".to_string()),
                kind: Some(GraphEdgeKind::BranchFalse),
            }];
            let mut returns = Vec::new();

            if let Some(body_flow) = body_flow {
                builder.add_edge(
                    &condition,
                    &body_flow.entry,
                    Some("true".to_string()),
                    Some(GraphEdgeKind::BranchTrue),
                );
                builder.connect_pending(
                    retag_pending_edges(body_flow.fallthroughs, GraphEdgeKind::LoopBack),
                    &condition,
                );
                builder.connect_pending(body_flow.continues, &condition);
                fallthroughs.extend(body_flow.breaks);
                returns.extend(body_flow.returns);
            } else {
                builder.add_edge(
                    &condition,
                    &condition,
                    Some("true".to_string()),
                    Some(GraphEdgeKind::LoopBack),
                );
            }

            Some(FlowSegment {
                entry: condition,
                fallthroughs,
                returns,
                breaks: Vec::new(),
                continues: Vec::new(),
            })
        }
        StmtKind::DoWhile(body, cond) => {
            let condition = builder.add_node(
                format!("do while {}", expr_snippet(source, source_base, cond)),
                range_snippet(source, source_base, solgrid_ast::span_to_range(stmt.span)),
                GraphNodeKind::Loop,
            );
            let loop_context = LoopContext {
                _continue_target: condition.clone(),
            };
            let body_flow = build_stmt_flow(
                builder,
                source,
                source_base,
                body,
                Some(&loop_context),
                placeholder,
            );
            let mut fallthroughs = vec![PendingEdge {
                from: condition.clone(),
                label: Some("false".to_string()),
                kind: Some(GraphEdgeKind::BranchFalse),
            }];
            let mut returns = Vec::new();

            let entry = if let Some(body_flow) = body_flow {
                builder.connect_pending(
                    retag_pending_edges(body_flow.fallthroughs, GraphEdgeKind::LoopBack),
                    &condition,
                );
                builder.connect_pending(body_flow.continues, &condition);
                fallthroughs.extend(body_flow.breaks);
                returns.extend(body_flow.returns);
                builder.add_edge(
                    &condition,
                    &body_flow.entry,
                    Some("true".to_string()),
                    Some(GraphEdgeKind::LoopBack),
                );
                body_flow.entry
            } else {
                builder.add_edge(
                    &condition,
                    &condition,
                    Some("true".to_string()),
                    Some(GraphEdgeKind::LoopBack),
                );
                condition.clone()
            };

            Some(FlowSegment {
                entry,
                fallthroughs,
                returns,
                breaks: Vec::new(),
                continues: Vec::new(),
            })
        }
        StmtKind::For {
            init,
            cond,
            next,
            body,
        } => {
            let condition = builder.add_node(
                cond.as_deref()
                    .map(|expr| format!("for {}", expr_snippet(source, source_base, expr)))
                    .unwrap_or_else(|| "for loop".to_string()),
                range_snippet(source, source_base, solgrid_ast::span_to_range(stmt.span)),
                GraphNodeKind::Loop,
            );
            let next_node = next.as_deref().map(|expr| {
                builder.add_node(
                    format!("next {}", expr_snippet(source, source_base, expr)),
                    range_snippet(source, source_base, solgrid_ast::span_to_range(expr.span)),
                    GraphNodeKind::LoopNext,
                )
            });
            let continue_target = next_node.clone().unwrap_or_else(|| condition.clone());
            let loop_context = LoopContext {
                _continue_target: continue_target,
            };
            let body_flow = build_stmt_flow(
                builder,
                source,
                source_base,
                body,
                Some(&loop_context),
                placeholder,
            );
            let mut loop_segment = FlowSegment {
                entry: condition.clone(),
                fallthroughs: if cond.is_some() {
                    vec![PendingEdge {
                        from: condition.clone(),
                        label: Some("false".to_string()),
                        kind: Some(GraphEdgeKind::BranchFalse),
                    }]
                } else {
                    Vec::new()
                },
                returns: Vec::new(),
                breaks: Vec::new(),
                continues: Vec::new(),
            };

            match (body_flow, next_node) {
                (Some(body_flow), Some(next_node)) => {
                    builder.add_edge(
                        &condition,
                        &body_flow.entry,
                        Some("true".to_string()),
                        Some(GraphEdgeKind::BranchTrue),
                    );
                    builder.connect_pending(body_flow.fallthroughs, &next_node);
                    builder.connect_pending(body_flow.continues, &next_node);
                    builder.add_edge(
                        &next_node,
                        &condition,
                        Some("loop".to_string()),
                        Some(GraphEdgeKind::LoopBack),
                    );
                    loop_segment.fallthroughs.extend(body_flow.breaks);
                    loop_segment.returns.extend(body_flow.returns);
                }
                (Some(body_flow), None) => {
                    builder.add_edge(
                        &condition,
                        &body_flow.entry,
                        Some("true".to_string()),
                        Some(GraphEdgeKind::BranchTrue),
                    );
                    builder.connect_pending(
                        retag_pending_edges(body_flow.fallthroughs, GraphEdgeKind::LoopBack),
                        &condition,
                    );
                    builder.connect_pending(body_flow.continues, &condition);
                    loop_segment.fallthroughs.extend(body_flow.breaks);
                    loop_segment.returns.extend(body_flow.returns);
                }
                (None, Some(next_node)) => {
                    builder.add_edge(
                        &condition,
                        &next_node,
                        Some("true".to_string()),
                        Some(GraphEdgeKind::BranchTrue),
                    );
                    builder.add_edge(
                        &next_node,
                        &condition,
                        Some("loop".to_string()),
                        Some(GraphEdgeKind::LoopBack),
                    );
                }
                (None, None) => {
                    builder.add_edge(
                        &condition,
                        &condition,
                        Some("true".to_string()),
                        Some(GraphEdgeKind::LoopBack),
                    );
                }
            }

            if let Some(init) = init.as_deref() {
                let init_flow = build_stmt_flow(
                    builder,
                    source,
                    source_base,
                    init,
                    Some(&loop_context),
                    placeholder,
                );
                compose_flow_segments(builder, init_flow, Some(loop_segment))
            } else {
                Some(loop_segment)
            }
        }
        StmtKind::Try(try_stmt) => {
            let try_node = builder.add_node(
                format!("try {}", expr_snippet(source, source_base, try_stmt.expr)),
                range_snippet(source, source_base, solgrid_ast::span_to_range(stmt.span)),
                GraphNodeKind::Try,
            );
            let mut fallthroughs = Vec::new();
            let mut returns = Vec::new();
            let mut breaks = Vec::new();
            let mut continues = Vec::new();

            for clause in try_stmt.clauses.iter() {
                let clause_label = try_clause_label(source, source_base, clause);
                let clause_node = builder.add_node(
                    clause_label.clone(),
                    clause_label.clone(),
                    GraphNodeKind::Catch,
                );
                builder.add_edge(
                    &try_node,
                    &clause_node,
                    Some(clause_label),
                    Some(GraphEdgeKind::Normal),
                );

                if let Some(clause_flow) = build_block_flow(
                    builder,
                    source,
                    source_base,
                    clause.block.stmts,
                    loop_context,
                    placeholder,
                ) {
                    builder.add_edge(
                        &clause_node,
                        &clause_flow.entry,
                        None,
                        Some(GraphEdgeKind::Normal),
                    );
                    fallthroughs.extend(clause_flow.fallthroughs);
                    returns.extend(clause_flow.returns);
                    breaks.extend(clause_flow.breaks);
                    continues.extend(clause_flow.continues);
                } else {
                    fallthroughs.push(PendingEdge {
                        from: clause_node,
                        label: None,
                        kind: Some(GraphEdgeKind::Normal),
                    });
                }
            }

            Some(FlowSegment {
                entry: try_node,
                fallthroughs,
                returns,
                breaks,
                continues,
            })
        }
    }
}

fn build_assembly_flow<'ast>(
    builder: &mut ControlFlowBuilder<'_>,
    source: &str,
    source_base: usize,
    stmt: &'ast Stmt<'ast>,
    assembly: &'ast solar_ast::StmtAssembly<'ast>,
    loop_context: Option<&LoopContext>,
) -> Option<FlowSegment> {
    let assembly_node = builder.add_node(
        assembly_label(source, source_base, assembly),
        stmt_snippet(source, source_base, stmt),
        GraphNodeKind::Assembly,
    );
    let mut yul_context = YulBuildContext::default();
    collect_yul_function_defs(&assembly.block, &mut yul_context.functions);
    let exit_target = builder.exit_id.clone();

    match build_yul_block_flow(
        builder,
        source,
        source_base,
        &assembly.block,
        loop_context,
        &mut yul_context,
        &exit_target,
    ) {
        Some(body) => {
            builder.add_edge(
                &assembly_node,
                &body.entry,
                None,
                Some(GraphEdgeKind::Normal),
            );
            Some(FlowSegment {
                entry: assembly_node,
                fallthroughs: body.fallthroughs,
                returns: body.returns,
                breaks: body.breaks,
                continues: body.continues,
            })
        }
        None => Some(FlowSegment {
            entry: assembly_node.clone(),
            fallthroughs: vec![PendingEdge {
                from: assembly_node,
                label: None,
                kind: Some(GraphEdgeKind::Normal),
            }],
            returns: Vec::new(),
            breaks: Vec::new(),
            continues: Vec::new(),
        }),
    }
}

fn build_yul_block_flow<'ast>(
    builder: &mut ControlFlowBuilder<'_>,
    source: &str,
    source_base: usize,
    block: &'ast yul::Block<'ast>,
    loop_context: Option<&LoopContext>,
    yul_context: &mut YulBuildContext<'ast>,
    exit_target: &str,
) -> Option<FlowSegment> {
    let mut segment = None;

    for stmt in block.stmts.iter() {
        let next = build_yul_stmt_flow(
            builder,
            source,
            source_base,
            stmt,
            loop_context,
            yul_context,
            exit_target,
        );
        segment = compose_flow_segments(builder, segment, next);
    }

    segment
}

fn build_yul_stmt_flow<'ast>(
    builder: &mut ControlFlowBuilder<'_>,
    source: &str,
    source_base: usize,
    stmt: &'ast yul::Stmt<'ast>,
    loop_context: Option<&LoopContext>,
    yul_context: &mut YulBuildContext<'ast>,
    exit_target: &str,
) -> Option<FlowSegment> {
    match &stmt.kind {
        yul::StmtKind::Block(block) => build_yul_block_flow(
            builder,
            source,
            source_base,
            block,
            loop_context,
            yul_context,
            exit_target,
        ),
        yul::StmtKind::AssignSingle(_, _)
        | yul::StmtKind::AssignMulti(_, _)
        | yul::StmtKind::Expr(_)
        | yul::StmtKind::VarDecl(_, _) => {
            build_yul_leaf_stmt_flow(builder, source, source_base, stmt, yul_context)
        }
        yul::StmtKind::FunctionDef(function) => {
            ensure_yul_function_graph(builder, source, source_base, function, yul_context);
            None
        }
        yul::StmtKind::If(cond, body) => {
            let condition = builder.add_node(
                format!("if {}", yul_expr_snippet(source, source_base, cond)),
                yul_stmt_snippet(source, source_base, stmt),
                GraphNodeKind::Branch,
            );
            let body_flow = build_yul_block_flow(
                builder,
                source,
                source_base,
                body,
                loop_context,
                yul_context,
                exit_target,
            );
            let mut fallthroughs = vec![PendingEdge {
                from: condition.clone(),
                label: Some("false".to_string()),
                kind: Some(GraphEdgeKind::BranchFalse),
            }];
            let mut breaks = Vec::new();
            let mut continues = Vec::new();

            if let Some(body_flow) = body_flow {
                builder.add_edge(
                    &condition,
                    &body_flow.entry,
                    Some("true".to_string()),
                    Some(GraphEdgeKind::BranchTrue),
                );
                fallthroughs.extend(body_flow.fallthroughs);
                breaks.extend(body_flow.breaks);
                continues.extend(body_flow.continues);
            } else {
                fallthroughs.push(PendingEdge {
                    from: condition.clone(),
                    label: Some("true".to_string()),
                    kind: Some(GraphEdgeKind::BranchTrue),
                });
            }

            Some(FlowSegment {
                entry: condition,
                fallthroughs,
                returns: Vec::new(),
                breaks,
                continues,
            })
        }
        yul::StmtKind::For(for_stmt) => {
            let condition = builder.add_node(
                format!(
                    "for {}",
                    yul_expr_snippet(source, source_base, &for_stmt.cond)
                ),
                yul_stmt_snippet(source, source_base, stmt),
                GraphNodeKind::Loop,
            );
            let loop_context = LoopContext {
                _continue_target: condition.clone(),
            };
            let step_flow = build_yul_block_flow(
                builder,
                source,
                source_base,
                &for_stmt.step,
                Some(&loop_context),
                yul_context,
                exit_target,
            );
            let body_flow = build_yul_block_flow(
                builder,
                source,
                source_base,
                &for_stmt.body,
                Some(&loop_context),
                yul_context,
                exit_target,
            );
            let mut loop_segment = FlowSegment {
                entry: condition.clone(),
                fallthroughs: vec![PendingEdge {
                    from: condition.clone(),
                    label: Some("false".to_string()),
                    kind: Some(GraphEdgeKind::BranchFalse),
                }],
                returns: Vec::new(),
                breaks: Vec::new(),
                continues: Vec::new(),
            };

            match (body_flow, step_flow) {
                (Some(body_flow), Some(step_flow)) => {
                    builder.add_edge(
                        &condition,
                        &body_flow.entry,
                        Some("true".to_string()),
                        Some(GraphEdgeKind::BranchTrue),
                    );
                    builder.connect_pending(body_flow.fallthroughs, &step_flow.entry);
                    builder.connect_pending(body_flow.continues, &step_flow.entry);
                    builder.connect_pending(
                        retag_pending_edges(step_flow.fallthroughs, GraphEdgeKind::LoopBack),
                        &condition,
                    );
                    builder.connect_pending(step_flow.continues, &condition);
                    loop_segment.fallthroughs.extend(body_flow.breaks);
                    loop_segment.fallthroughs.extend(step_flow.breaks);
                }
                (Some(body_flow), None) => {
                    builder.add_edge(
                        &condition,
                        &body_flow.entry,
                        Some("true".to_string()),
                        Some(GraphEdgeKind::BranchTrue),
                    );
                    builder.connect_pending(
                        retag_pending_edges(body_flow.fallthroughs, GraphEdgeKind::LoopBack),
                        &condition,
                    );
                    builder.connect_pending(body_flow.continues, &condition);
                    loop_segment.fallthroughs.extend(body_flow.breaks);
                }
                (None, Some(step_flow)) => {
                    builder.add_edge(
                        &condition,
                        &step_flow.entry,
                        Some("true".to_string()),
                        Some(GraphEdgeKind::BranchTrue),
                    );
                    builder.connect_pending(
                        retag_pending_edges(step_flow.fallthroughs, GraphEdgeKind::LoopBack),
                        &condition,
                    );
                    builder.connect_pending(step_flow.continues, &condition);
                    loop_segment.fallthroughs.extend(step_flow.breaks);
                }
                (None, None) => {
                    builder.add_edge(
                        &condition,
                        &condition,
                        Some("true".to_string()),
                        Some(GraphEdgeKind::LoopBack),
                    );
                }
            }

            let init_flow = build_yul_block_flow(
                builder,
                source,
                source_base,
                &for_stmt.init,
                Some(&loop_context),
                yul_context,
                exit_target,
            );
            compose_flow_segments(builder, init_flow, Some(loop_segment))
        }
        yul::StmtKind::Switch(switch) => {
            let switch_node = builder.add_node(
                format!(
                    "switch {}",
                    yul_expr_snippet(source, source_base, &switch.selector)
                ),
                yul_stmt_snippet(source, source_base, stmt),
                GraphNodeKind::Branch,
            );
            let mut fallthroughs = Vec::new();
            let mut breaks = Vec::new();
            let mut continues = Vec::new();
            let mut has_default = false;

            for case in switch.cases.iter() {
                let label = yul_switch_case_label(source, source_base, case);
                if case.constant.is_none() {
                    has_default = true;
                }

                if let Some(case_flow) = build_yul_block_flow(
                    builder,
                    source,
                    source_base,
                    &case.body,
                    loop_context,
                    yul_context,
                    exit_target,
                ) {
                    builder.add_edge(
                        &switch_node,
                        &case_flow.entry,
                        Some(label),
                        Some(GraphEdgeKind::Normal),
                    );
                    fallthroughs.extend(case_flow.fallthroughs);
                    breaks.extend(case_flow.breaks);
                    continues.extend(case_flow.continues);
                } else {
                    fallthroughs.push(PendingEdge {
                        from: switch_node.clone(),
                        label: Some(label),
                        kind: Some(GraphEdgeKind::Normal),
                    });
                }
            }

            if !has_default {
                fallthroughs.push(PendingEdge {
                    from: switch_node.clone(),
                    label: Some("no match".to_string()),
                    kind: Some(GraphEdgeKind::Normal),
                });
            }

            Some(FlowSegment {
                entry: switch_node,
                fallthroughs,
                returns: Vec::new(),
                breaks,
                continues,
            })
        }
        yul::StmtKind::Leave => {
            let node = builder.add_node(
                "leave".to_string(),
                yul_stmt_snippet(source, source_base, stmt),
                GraphNodeKind::TerminalReturn,
            );
            builder.add_edge(
                &node,
                exit_target,
                Some("leave".to_string()),
                Some(GraphEdgeKind::Return),
            );
            Some(FlowSegment {
                entry: node,
                fallthroughs: Vec::new(),
                returns: Vec::new(),
                breaks: Vec::new(),
                continues: Vec::new(),
            })
        }
        yul::StmtKind::Break => {
            let node = builder.add_node(
                "break".to_string(),
                "break".to_string(),
                GraphNodeKind::ControlTransfer,
            );
            Some(FlowSegment {
                entry: node.clone(),
                fallthroughs: Vec::new(),
                returns: Vec::new(),
                breaks: vec![PendingEdge {
                    from: node,
                    label: Some("break".to_string()),
                    kind: Some(GraphEdgeKind::Break),
                }],
                continues: Vec::new(),
            })
        }
        yul::StmtKind::Continue => {
            let node = builder.add_node(
                "continue".to_string(),
                "continue".to_string(),
                GraphNodeKind::ControlTransfer,
            );
            Some(FlowSegment {
                entry: node.clone(),
                fallthroughs: Vec::new(),
                returns: Vec::new(),
                breaks: Vec::new(),
                continues: vec![PendingEdge {
                    from: node,
                    label: Some("continue".to_string()),
                    kind: Some(GraphEdgeKind::Continue),
                }],
            })
        }
    }
}

fn build_yul_leaf_stmt_flow<'ast>(
    builder: &mut ControlFlowBuilder<'_>,
    source: &str,
    source_base: usize,
    stmt: &'ast yul::Stmt<'ast>,
    yul_context: &mut YulBuildContext<'ast>,
) -> Option<FlowSegment> {
    let (label, detail, kind) = yul_stmt_node_descriptor(source, source_base, stmt);
    if let Some((edge_label, edge_kind, node_kind)) = yul_terminal_stmt_semantics(stmt) {
        let node = builder.add_node(edge_label.clone(), detail, node_kind);
        let global_exit = builder.exit_id.clone();
        builder.add_edge(&node, &global_exit, Some(edge_label), Some(edge_kind));
        return Some(FlowSegment {
            entry: node,
            fallthroughs: Vec::new(),
            returns: Vec::new(),
            breaks: Vec::new(),
            continues: Vec::new(),
        });
    }

    let segment = single_node_segment(builder, label, detail, kind);
    if let Some(call_name) = yul_called_function_name(stmt) {
        if let Some(function_graph) =
            ensure_yul_function_graph_by_name(builder, source, source_base, &call_name, yul_context)
        {
            builder.add_edge(
                &segment.entry,
                &function_graph.entry,
                Some("calls".to_string()),
                Some(GraphEdgeKind::Normal),
            );
        }
    }
    Some(segment)
}

fn collect_yul_function_defs<'ast>(
    block: &'ast yul::Block<'ast>,
    functions: &mut HashMap<String, YulFunctionLookupEntry<'ast>>,
) {
    for stmt in block.stmts.iter() {
        match &stmt.kind {
            yul::StmtKind::Block(block) => collect_yul_function_defs(block, functions),
            yul::StmtKind::If(_, body) => collect_yul_function_defs(body, functions),
            yul::StmtKind::For(for_stmt) => {
                collect_yul_function_defs(&for_stmt.init, functions);
                collect_yul_function_defs(&for_stmt.step, functions);
                collect_yul_function_defs(&for_stmt.body, functions);
            }
            yul::StmtKind::Switch(switch) => {
                for case in switch.cases.iter() {
                    collect_yul_function_defs(&case.body, functions);
                }
            }
            yul::StmtKind::FunctionDef(function) => {
                functions
                    .entry(function.name.as_str().to_string())
                    .or_insert(YulFunctionLookupEntry { function });
                collect_yul_function_defs(&function.body, functions);
            }
            _ => {}
        }
    }
}

fn ensure_yul_function_graph_by_name<'ast>(
    builder: &mut ControlFlowBuilder<'_>,
    source: &str,
    source_base: usize,
    name: &str,
    yul_context: &mut YulBuildContext<'ast>,
) -> Option<YulFunctionGraph> {
    let function = yul_context.functions.get(name)?.function;
    ensure_yul_function_graph(builder, source, source_base, function, yul_context)
}

fn ensure_yul_function_graph<'ast>(
    builder: &mut ControlFlowBuilder<'_>,
    source: &str,
    source_base: usize,
    function: &'ast yul::Function<'ast>,
    yul_context: &mut YulBuildContext<'ast>,
) -> Option<YulFunctionGraph> {
    let name = function.name.as_str().to_string();
    if let Some(graph) = yul_context.graphs.get(&name) {
        return Some(graph.clone());
    }

    if !yul_context.active_calls.insert(name.clone()) {
        return yul_context.graphs.get(&name).cloned();
    }

    let entry = builder.add_node(
        format!("function {name}"),
        yul_function_detail(source, source_base, function),
        GraphNodeKind::Declaration,
    );
    let exit = builder.add_node(
        format!("end {name}"),
        format!("return from Yul function `{name}`"),
        GraphNodeKind::Block,
    );
    let graph = YulFunctionGraph {
        entry: entry.clone(),
    };
    yul_context.graphs.insert(name.clone(), graph.clone());

    if let Some(body) = build_yul_block_flow(
        builder,
        source,
        source_base,
        &function.body,
        None,
        yul_context,
        &exit,
    ) {
        builder.add_edge(
            &entry,
            &body.entry,
            Some("body".to_string()),
            Some(GraphEdgeKind::Normal),
        );
        builder.connect_pending(body.fallthroughs, &exit);
        builder.connect_pending(body.breaks, &exit);
        builder.connect_pending(body.continues, &exit);
    } else {
        builder.add_edge(
            &entry,
            &exit,
            Some("body".to_string()),
            Some(GraphEdgeKind::Normal),
        );
    }

    yul_context.active_calls.remove(&name);
    Some(graph)
}

fn yul_function_detail(source: &str, source_base: usize, function: &yul::Function<'_>) -> String {
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| parameter.as_str().to_string())
        .collect::<Vec<_>>();
    let returns = function
        .returns
        .iter()
        .map(|value| value.as_str().to_string())
        .collect::<Vec<_>>();
    let mut detail = format!("function {}({})", function.name, parameters.join(", "));
    if !returns.is_empty() {
        detail.push_str(" -> ");
        detail.push_str(&returns.join(", "));
    }
    let body = range_snippet(
        source,
        source_base,
        solgrid_ast::span_to_range(function.body.span),
    );
    if !body.is_empty() {
        detail.push_str(": ");
        detail.push_str(&body);
    }
    detail
}

fn yul_called_function_name(stmt: &yul::Stmt<'_>) -> Option<String> {
    match &stmt.kind {
        yul::StmtKind::Expr(expr) => yul_called_function_name_from_expr(expr),
        yul::StmtKind::AssignSingle(_, expr) | yul::StmtKind::AssignMulti(_, expr) => {
            yul_called_function_name_from_expr(expr)
        }
        yul::StmtKind::VarDecl(_, Some(expr)) => yul_called_function_name_from_expr(expr),
        _ => None,
    }
}

fn yul_called_function_name_from_expr(expr: &yul::Expr<'_>) -> Option<String> {
    let yul::ExprKind::Call(call) = &expr.kind else {
        return None;
    };
    Some(call.name.as_str().to_string())
}

fn yul_terminal_stmt_semantics(
    stmt: &yul::Stmt<'_>,
) -> Option<(String, GraphEdgeKind, GraphNodeKind)> {
    let yul::StmtKind::Expr(expr) = &stmt.kind else {
        return None;
    };
    let yul::ExprKind::Call(call) = &expr.kind else {
        return None;
    };
    match call.name.as_str() {
        "revert" | "invalid" => Some((
            call.name.as_str().to_string(),
            GraphEdgeKind::Revert,
            GraphNodeKind::TerminalRevert,
        )),
        "return" | "stop" | "selfdestruct" => Some((
            call.name.as_str().to_string(),
            GraphEdgeKind::Return,
            GraphNodeKind::TerminalReturn,
        )),
        _ => None,
    }
}

fn single_node_segment(
    builder: &mut ControlFlowBuilder<'_>,
    label: String,
    detail: String,
    kind: GraphNodeKind,
) -> FlowSegment {
    let node = builder.add_node(label, detail, kind);
    FlowSegment {
        entry: node.clone(),
        fallthroughs: vec![PendingEdge {
            from: node,
            label: None,
            kind: Some(GraphEdgeKind::Normal),
        }],
        returns: Vec::new(),
        breaks: Vec::new(),
        continues: Vec::new(),
    }
}

fn linearized_contract_refs<B: NavBackend>(
    index: &ProjectIndex<B>,
    snapshot: &ProjectSnapshot,
    get_source: &dyn Fn(&Path) -> Option<String>,
) -> HashMap<String, Vec<ResolvedContractRef>> {
    let mut order = HashMap::new();
    let current_file = (snapshot.path.clone(), snapshot.source.as_str());

    for contract in &snapshot.contracts {
        let root = ResolvedContract {
            path: snapshot.path.clone(),
            decl: contract.clone(),
        };
        let mut cache = HashMap::new();
        let mut active = HashSet::new();
        let contracts = index
            .linearized_inheritance_order(
                &root,
                Some(&current_file),
                get_source,
                &mut cache,
                &mut active,
            )
            .map(|linearized| {
                linearized
                    .into_iter()
                    .map(|resolved| ResolvedContractRef {
                        path: resolved.path,
                        contract_name: resolved.decl.name,
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|contracts| !contracts.is_empty())
            .unwrap_or_else(|| {
                vec![ResolvedContractRef {
                    path: snapshot.path.clone(),
                    contract_name: contract.name.clone(),
                }]
            });
        order.insert(contract.name.clone(), contracts);
    }
    order
}

#[allow(clippy::too_many_arguments)]
fn resolve_modifier_plans<'ast>(
    source: &str,
    snapshot: &ProjectSnapshot,
    current_path: &Path,
    current_contract: Option<&str>,
    function: &ItemFunction<'ast>,
    contract_linearization: &HashMap<String, Vec<ResolvedContractRef>>,
    modifier_lookup: &HashMap<ModifierLookupKey, ModifierLookupEntry<'ast>>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<ModifierPlan<'ast>> {
    let Some(current_contract) = current_contract else {
        return Vec::new();
    };
    let search_order = contract_linearization
        .get(current_contract)
        .cloned()
        .unwrap_or_else(|| {
            vec![ResolvedContractRef {
                path: normalize_path(current_path),
                contract_name: current_contract.to_string(),
            }]
        });

    function
        .header
        .modifiers
        .iter()
        .map(|modifier| {
            let label = normalize_graph_text(&source[solgrid_ast::span_to_range(modifier.span())]);
            let segments = modifier.name.segments();
            let modifier_name = segments
                .last()
                .map(|segment| segment.as_str().to_string())
                .unwrap_or_else(|| modifier.name.to_string());
            let argument_count = modifier.arguments.exprs().len();

            let lookup_body = |contract: &ResolvedContractRef| {
                modifier_lookup.get(&ModifierLookupKey {
                    path: contract.path.clone(),
                    contract_name: contract.contract_name.clone(),
                    modifier_name: modifier_name.clone(),
                    arity: argument_count,
                })
            };
            let body = if segments.len() > 1 {
                let qualifier = TypePath {
                    segments: segments[..segments.len() - 1]
                        .iter()
                        .map(|segment| segment.as_str().to_string())
                        .collect(),
                };
                resolve_contract_type_path(snapshot, &qualifier, get_source, resolver)
                    .and_then(|resolved| {
                        search_order.iter().find(|contract| {
                            contract.path == resolved.snapshot.path
                                && contract.contract_name == resolved.def.name
                        })
                    })
                    .and_then(lookup_body)
                    .cloned()
            } else {
                search_order
                    .iter()
                    .find_map(|contract| lookup_body(contract).cloned())
            };

            ModifierPlan {
                detail: label.clone(),
                label,
                body_source: body.as_ref().map(|entry| entry.body_source.clone()),
                body_source_base: body
                    .as_ref()
                    .map(|entry| entry.body_source_base)
                    .unwrap_or(0),
                body_stmts: body.and_then(|entry| entry.body_stmts),
            }
        })
        .collect()
}

fn collect_modifier_lookup<'ast>(
    path: &Path,
    source: Arc<str>,
    source_base: usize,
    source_unit: &'ast solar_ast::SourceUnit<'ast>,
    modifier_lookup: &mut HashMap<ModifierLookupKey, ModifierLookupEntry<'ast>>,
) {
    for item in source_unit.items.iter() {
        let ItemKind::Contract(contract) = &item.kind else {
            continue;
        };
        for body_item in contract.body.iter() {
            let ItemKind::Function(function) = &body_item.kind else {
                continue;
            };
            if function.kind != FunctionKind::Modifier {
                continue;
            }
            let Some(name) = function.header.name.map(|name| name.as_str().to_string()) else {
                continue;
            };
            modifier_lookup.insert(
                ModifierLookupKey {
                    path: normalize_path(path),
                    contract_name: contract.name.as_str().to_string(),
                    modifier_name: name,
                    arity: function.header.parameters.len(),
                },
                ModifierLookupEntry {
                    body_source: source.clone(),
                    body_source_base: source_base,
                    body_stmts: function.body.as_ref().map(|body| &**body.stmts),
                },
            );
        }
    }
}

fn callable_declaration(
    source: &str,
    contract_stack: &[String],
    def_span: ByteRange<usize>,
    function: &ItemFunction<'_>,
) -> Option<CallableDecl> {
    if !function.is_implemented() {
        return None;
    }

    let lens_span = callable_lens_span(source, function);
    let container_name = contract_stack.last().cloned();
    let name = callable_name(function);
    let label = callable_label(source, contract_stack, function);
    Some(CallableDecl {
        name,
        label: label.clone(),
        detail: Some(label),
        container_name,
        kind: callable_symbol_kind(function.kind),
        kind_label: function.kind.to_str().to_string(),
        target_offset: lens_span.start,
        def_span,
        lens_span,
    })
}

fn collect_callable_declarations(source: &str, filename: &str) -> Vec<CallableDecl> {
    with_parsed_ast_sequential(source, filename, |source_unit| {
        fn visit_item(
            item: &solar_ast::Item<'_>,
            source: &str,
            contract_stack: &mut Vec<String>,
            callables: &mut Vec<CallableDecl>,
        ) {
            match &item.kind {
                ItemKind::Contract(contract) => {
                    contract_stack.push(contract.name.as_str().to_string());
                    for body_item in contract.body.iter() {
                        visit_item(body_item, source, contract_stack, callables);
                    }
                    contract_stack.pop();
                }
                ItemKind::Function(function) => {
                    if let Some(callable) = callable_declaration(
                        source,
                        contract_stack,
                        solgrid_ast::span_to_range(item.span),
                        function,
                    ) {
                        callables.push(callable);
                    }
                }
                _ => {}
            }
        }

        let mut callables = Vec::new();
        let mut contract_stack = Vec::new();
        for item in source_unit.items.iter() {
            visit_item(item, source, &mut contract_stack, &mut callables);
        }
        callables
    })
    .unwrap_or_default()
}

fn callable_name(function: &ItemFunction<'_>) -> String {
    match function.kind {
        FunctionKind::Function | FunctionKind::Modifier => function
            .header
            .name
            .map(|name| name.as_str().to_string())
            .unwrap_or_else(|| function.kind.to_str().to_string()),
        FunctionKind::Constructor => "constructor".to_string(),
        FunctionKind::Fallback => "fallback".to_string(),
        FunctionKind::Receive => "receive".to_string(),
    }
}

fn callable_label(source: &str, contract_stack: &[String], function: &ItemFunction<'_>) -> String {
    let container = contract_stack.last();
    let parameters = function
        .header
        .parameters
        .iter()
        .map(|parameter| callable_type_identity(source, &parameter.ty))
        .collect::<Vec<_>>()
        .join(",");
    match function.kind {
        FunctionKind::Function | FunctionKind::Modifier => {
            let name = function
                .header
                .name
                .map(|name| name.as_str().to_string())
                .unwrap_or_else(|| function.kind.to_str().to_string());
            container
                .map(|container| format!("{container}.{name}({parameters})"))
                .unwrap_or_else(|| format!("{name}({parameters})"))
        }
        FunctionKind::Constructor => container
            .map(|container| format!("{container}.constructor({parameters})"))
            .unwrap_or_else(|| format!("constructor({parameters})")),
        FunctionKind::Fallback => container
            .map(|container| format!("{container}.fallback()"))
            .unwrap_or_else(|| "fallback()".to_string()),
        FunctionKind::Receive => container
            .map(|container| format!("{container}.receive()"))
            .unwrap_or_else(|| "receive()".to_string()),
    }
}

fn callable_type_identity(source: &str, ty: &solar_ast::Type<'_>) -> String {
    match &ty.kind {
        solar_ast::TypeKind::Elementary(elementary) => match elementary {
            solar_ast::ElementaryType::Address(_) => "address".to_string(),
            solar_ast::ElementaryType::Bool => "bool".to_string(),
            solar_ast::ElementaryType::String => "string".to_string(),
            solar_ast::ElementaryType::Bytes => "bytes".to_string(),
            solar_ast::ElementaryType::Int(size) => format!("int{}", size.bits()),
            solar_ast::ElementaryType::UInt(size) => format!("uint{}", size.bits()),
            solar_ast::ElementaryType::FixedBytes(size) => format!("bytes{}", size.bytes()),
            solar_ast::ElementaryType::Fixed(size, fraction) => {
                format!("fixed{}x{}", size.bits(), fraction.get())
            }
            solar_ast::ElementaryType::UFixed(size, fraction) => {
                format!("ufixed{}x{}", size.bits(), fraction.get())
            }
        },
        solar_ast::TypeKind::Custom(path) => path
            .segments()
            .iter()
            .map(|segment| segment.as_str())
            .collect::<Vec<_>>()
            .join("."),
        solar_ast::TypeKind::Array(array) => {
            let element = callable_type_identity(source, &array.element);
            let length = array.size.as_ref().map(|size| match &size.kind {
                solar_ast::ExprKind::Lit(literal, None) => match literal.kind {
                    solar_ast::LitKind::Number(value) => value.to_string(),
                    _ => range_snippet(source, 0, solgrid_ast::span_to_range(size.span)),
                },
                _ => range_snippet(source, 0, solgrid_ast::span_to_range(size.span)),
            });
            match length {
                Some(length) => format!("{element}[{length}]"),
                None => format!("{element}[]"),
            }
        }
        solar_ast::TypeKind::Mapping(mapping) => format!(
            "mapping({}=>{})",
            callable_type_identity(source, &mapping.key),
            callable_type_identity(source, &mapping.value)
        ),
        solar_ast::TypeKind::Function(function) => {
            let parameters = function
                .parameters
                .iter()
                .map(|parameter| callable_type_identity(source, &parameter.ty))
                .collect::<Vec<_>>()
                .join(",");
            let mut identity = format!("function({parameters})");
            if let Some(visibility) = function.visibility {
                identity.push(' ');
                identity.push_str(&visibility.data.to_string());
            }
            if let Some(state_mutability) = function.state_mutability {
                identity.push(' ');
                identity.push_str(&state_mutability.data.to_string());
            }
            if let Some(returns) = &function.returns {
                let return_types = returns
                    .iter()
                    .map(|parameter| callable_type_identity(source, &parameter.ty))
                    .collect::<Vec<_>>()
                    .join(",");
                identity.push_str(" returns(");
                identity.push_str(&return_types);
                identity.push(')');
            }
            identity
        }
    }
}

fn callable_symbol_kind(kind: FunctionKind) -> SymbolKind {
    match kind {
        FunctionKind::Constructor => SymbolKind::Constructor,
        FunctionKind::Modifier => SymbolKind::Modifier,
        _ => SymbolKind::Function,
    }
}

fn collect_outgoing_calls_for_callable(
    snapshot: &ProjectSnapshot,
    callable: &CallableDecl,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<ResolvedOutgoingCall> {
    let filename = snapshot.path.to_string_lossy().to_string();
    with_parsed_ast_sequential(&snapshot.source, &filename, |source_unit| {
        struct OutgoingCallCollectContext<'a> {
            source: &'a str,
            snapshot: &'a ProjectSnapshot,
            target_offset: usize,
            get_source: &'a dyn Fn(&Path) -> Option<String>,
            resolver: &'a ImportResolver,
        }

        fn visit_item(
            item: &solar_ast::Item<'_>,
            contract_stack: &mut Vec<String>,
            context: &OutgoingCallCollectContext<'_>,
            calls: &mut Vec<ResolvedOutgoingCall>,
        ) {
            match &item.kind {
                ItemKind::Contract(contract) => {
                    contract_stack.push(contract.name.as_str().to_string());
                    for body_item in contract.body.iter() {
                        visit_item(body_item, contract_stack, context, calls);
                    }
                    contract_stack.pop();
                }
                ItemKind::Function(function) => {
                    let Some(current) = callable_declaration(
                        context.source,
                        contract_stack,
                        solgrid_ast::span_to_range(item.span),
                        function,
                    ) else {
                        return;
                    };
                    if current.target_offset != context.target_offset {
                        return;
                    }

                    for modifier in function.header.modifiers.iter() {
                        collect_modifier_outgoing_call(
                            context.snapshot,
                            context.source,
                            modifier,
                            context.get_source,
                            context.resolver,
                            calls,
                        );
                    }
                    if let Some(body) = &function.body {
                        collect_block_outgoing_calls(
                            context.snapshot,
                            context.source,
                            body.stmts,
                            context.get_source,
                            context.resolver,
                            calls,
                        );
                    }
                }
                _ => {}
            }
        }

        let mut calls = Vec::new();
        let mut contract_stack = Vec::new();
        let context = OutgoingCallCollectContext {
            source: &snapshot.source,
            snapshot,
            target_offset: callable.target_offset,
            get_source,
            resolver,
        };
        for item in source_unit.items.iter() {
            visit_item(item, &mut contract_stack, &context, &mut calls);
        }
        calls
    })
    .unwrap_or_default()
}

fn collect_block_outgoing_calls(
    snapshot: &ProjectSnapshot,
    source: &str,
    stmts: &[Stmt<'_>],
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    calls: &mut Vec<ResolvedOutgoingCall>,
) {
    for stmt in stmts {
        collect_stmt_outgoing_calls(snapshot, source, stmt, get_source, resolver, calls);
    }
}

fn collect_stmt_outgoing_calls(
    snapshot: &ProjectSnapshot,
    source: &str,
    stmt: &Stmt<'_>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    calls: &mut Vec<ResolvedOutgoingCall>,
) {
    match &stmt.kind {
        StmtKind::Assembly(_) | StmtKind::Break | StmtKind::Continue | StmtKind::Placeholder => {}
        StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
            collect_block_outgoing_calls(
                snapshot,
                source,
                block.stmts,
                get_source,
                resolver,
                calls,
            );
        }
        StmtKind::DeclSingle(var) => {
            if let Some(initializer) = &var.initializer {
                collect_expr_outgoing_calls(
                    snapshot,
                    source,
                    initializer,
                    get_source,
                    resolver,
                    calls,
                );
            }
        }
        StmtKind::DeclMulti(_, expr) | StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => {
            collect_expr_outgoing_calls(snapshot, source, expr, get_source, resolver, calls);
        }
        StmtKind::Return(None) => {}
        StmtKind::DoWhile(body, condition) | StmtKind::While(condition, body) => {
            collect_stmt_outgoing_calls(snapshot, source, body, get_source, resolver, calls);
            collect_expr_outgoing_calls(snapshot, source, condition, get_source, resolver, calls);
        }
        StmtKind::Emit(_, args) | StmtKind::Revert(_, args) => {
            for arg in args.exprs() {
                collect_expr_outgoing_calls(snapshot, source, arg, get_source, resolver, calls);
            }
        }
        StmtKind::For {
            init,
            cond,
            next,
            body,
        } => {
            if let Some(init) = init {
                collect_stmt_outgoing_calls(snapshot, source, init, get_source, resolver, calls);
            }
            if let Some(cond) = cond {
                collect_expr_outgoing_calls(snapshot, source, cond, get_source, resolver, calls);
            }
            if let Some(next) = next {
                collect_expr_outgoing_calls(snapshot, source, next, get_source, resolver, calls);
            }
            collect_stmt_outgoing_calls(snapshot, source, body, get_source, resolver, calls);
        }
        StmtKind::If(condition, then_branch, else_branch) => {
            collect_expr_outgoing_calls(snapshot, source, condition, get_source, resolver, calls);
            collect_stmt_outgoing_calls(snapshot, source, then_branch, get_source, resolver, calls);
            if let Some(else_branch) = else_branch {
                collect_stmt_outgoing_calls(
                    snapshot,
                    source,
                    else_branch,
                    get_source,
                    resolver,
                    calls,
                );
            }
        }
        StmtKind::Try(try_stmt) => {
            collect_expr_outgoing_calls(
                snapshot,
                source,
                try_stmt.expr,
                get_source,
                resolver,
                calls,
            );
            for clause in try_stmt.clauses.iter() {
                collect_block_outgoing_calls(
                    snapshot,
                    source,
                    clause.block.stmts,
                    get_source,
                    resolver,
                    calls,
                );
            }
        }
    }
}

fn collect_expr_outgoing_calls(
    snapshot: &ProjectSnapshot,
    source: &str,
    expr: &solar_ast::Expr<'_>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    calls: &mut Vec<ResolvedOutgoingCall>,
) {
    let expr = expr.peel_parens();
    match &expr.kind {
        solar_ast::ExprKind::Array(exprs) => {
            for expr in exprs.iter() {
                collect_expr_outgoing_calls(snapshot, source, expr, get_source, resolver, calls);
            }
        }
        solar_ast::ExprKind::Tuple(exprs) => {
            for expr in exprs.iter() {
                if let solgrid_parser::solar_interface::SpannedOption::Some(expr) = expr {
                    collect_expr_outgoing_calls(
                        snapshot, source, expr, get_source, resolver, calls,
                    );
                }
            }
        }
        solar_ast::ExprKind::Assign(lhs, _, rhs) | solar_ast::ExprKind::Binary(lhs, _, rhs) => {
            collect_expr_outgoing_calls(snapshot, source, lhs, get_source, resolver, calls);
            collect_expr_outgoing_calls(snapshot, source, rhs, get_source, resolver, calls);
        }
        solar_ast::ExprKind::Call(callee, args) => {
            if let Some(offset) = callable_expr_target_offset(callee) {
                if let Some(target) =
                    callable_target_at_offset(snapshot, offset, get_source, resolver)
                {
                    calls.push(ResolvedOutgoingCall {
                        target,
                        from_range: span_to_range(
                            source,
                            &callable_target_span(callee)
                                .unwrap_or_else(|| solgrid_ast::span_to_range(callee.span)),
                        ),
                    });
                }
            }
            collect_expr_outgoing_calls(snapshot, source, callee, get_source, resolver, calls);
            for arg in args.exprs() {
                collect_expr_outgoing_calls(snapshot, source, arg, get_source, resolver, calls);
            }
        }
        solar_ast::ExprKind::CallOptions(callee, args) => {
            collect_expr_outgoing_calls(snapshot, source, callee, get_source, resolver, calls);
            for arg in args.iter() {
                collect_expr_outgoing_calls(
                    snapshot, source, arg.value, get_source, resolver, calls,
                );
            }
        }
        solar_ast::ExprKind::Delete(expr) | solar_ast::ExprKind::Unary(_, expr) => {
            collect_expr_outgoing_calls(snapshot, source, expr, get_source, resolver, calls);
        }
        solar_ast::ExprKind::Index(lhs, kind) => {
            collect_expr_outgoing_calls(snapshot, source, lhs, get_source, resolver, calls);
            match kind {
                solar_ast::IndexKind::Index(expr) => {
                    if let Some(expr) = expr {
                        collect_expr_outgoing_calls(
                            snapshot, source, expr, get_source, resolver, calls,
                        );
                    }
                }
                solar_ast::IndexKind::Range(start, end) => {
                    if let Some(start) = start {
                        collect_expr_outgoing_calls(
                            snapshot, source, start, get_source, resolver, calls,
                        );
                    }
                    if let Some(end) = end {
                        collect_expr_outgoing_calls(
                            snapshot, source, end, get_source, resolver, calls,
                        );
                    }
                }
            }
        }
        solar_ast::ExprKind::Member(expr, _) => {
            collect_expr_outgoing_calls(snapshot, source, expr, get_source, resolver, calls);
        }
        solar_ast::ExprKind::Payable(args) => {
            for arg in args.exprs() {
                collect_expr_outgoing_calls(snapshot, source, arg, get_source, resolver, calls);
            }
        }
        solar_ast::ExprKind::Ternary(condition, if_true, if_false) => {
            collect_expr_outgoing_calls(snapshot, source, condition, get_source, resolver, calls);
            collect_expr_outgoing_calls(snapshot, source, if_true, get_source, resolver, calls);
            collect_expr_outgoing_calls(snapshot, source, if_false, get_source, resolver, calls);
        }
        solar_ast::ExprKind::Ident(_)
        | solar_ast::ExprKind::Lit(_, _)
        | solar_ast::ExprKind::New(_)
        | solar_ast::ExprKind::TypeCall(_)
        | solar_ast::ExprKind::Type(_) => {}
    }
}

fn collect_modifier_outgoing_call(
    snapshot: &ProjectSnapshot,
    source: &str,
    modifier: &solar_ast::Modifier<'_>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    calls: &mut Vec<ResolvedOutgoingCall>,
) {
    if let Some(offset) = modifier_target_offset(modifier) {
        if let Some(target) = callable_target_at_offset(snapshot, offset, get_source, resolver) {
            calls.push(ResolvedOutgoingCall {
                target,
                from_range: span_to_range(
                    source,
                    &modifier_target_span(modifier)
                        .unwrap_or_else(|| solgrid_ast::span_to_range(modifier.span())),
                ),
            });
        }
    }

    for arg in modifier.arguments.exprs() {
        collect_expr_outgoing_calls(snapshot, source, arg, get_source, resolver, calls);
    }
}

fn callable_expr_target_offset(expr: &solar_ast::Expr<'_>) -> Option<usize> {
    let expr = expr.peel_parens();
    match &expr.kind {
        solar_ast::ExprKind::Call(callee, _) | solar_ast::ExprKind::CallOptions(callee, _) => {
            callable_expr_target_offset(callee)
        }
        solar_ast::ExprKind::Ident(ident) => Some(solgrid_ast::span_to_range(ident.span).start),
        solar_ast::ExprKind::Member(_, member) => {
            Some(solgrid_ast::span_to_range(member.span).start)
        }
        _ => None,
    }
}

fn callable_target_span(expr: &solar_ast::Expr<'_>) -> Option<ByteRange<usize>> {
    let expr = expr.peel_parens();
    match &expr.kind {
        solar_ast::ExprKind::Call(callee, _) | solar_ast::ExprKind::CallOptions(callee, _) => {
            callable_target_span(callee)
        }
        solar_ast::ExprKind::Ident(ident) => Some(solgrid_ast::span_to_range(ident.span)),
        solar_ast::ExprKind::Member(_, member) => Some(solgrid_ast::span_to_range(member.span)),
        _ => None,
    }
}

fn modifier_target_offset(modifier: &solar_ast::Modifier<'_>) -> Option<usize> {
    modifier
        .name
        .segments()
        .last()
        .map(|ident| solgrid_ast::span_to_range(ident.span).start)
}

fn modifier_target_span(modifier: &solar_ast::Modifier<'_>) -> Option<ByteRange<usize>> {
    modifier
        .name
        .segments()
        .last()
        .map(|ident| solgrid_ast::span_to_range(ident.span))
}

fn rename_location_matches_target_name(
    location: &Location,
    target_name: &str,
    current_path: &Path,
    current_source: &str,
    get_source: &dyn Fn(&Path) -> Option<String>,
) -> bool {
    let Some(path) = location.uri.to_file_path() else {
        return false;
    };
    let path = normalize_path(path.as_ref());
    let source = if path == current_path {
        Cow::Borrowed(current_source)
    } else {
        let Some(source) = get_source(&path) else {
            return false;
        };
        Cow::Owned(source)
    };
    range_text_matches(source.as_ref(), location.range, target_name)
}

fn range_text_matches(source: &str, range: Range, expected: &str) -> bool {
    let start = position_to_offset(source, range.start);
    let end = position_to_offset(source, range.end);
    match source.get(start..end) {
        Some(text) => text == expected,
        None => false,
    }
}

fn is_call_hierarchy_kind(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Constructor | SymbolKind::Function | SymbolKind::Modifier
    )
}

fn call_hierarchy_entry_from_decl(
    snapshot: &ProjectSnapshot,
    callable: &CallableDecl,
) -> CallHierarchyEntry {
    CallHierarchyEntry {
        path: snapshot.path.clone(),
        name: callable.name.clone(),
        detail: callable.detail.clone(),
        kind: callable_kind_to_call_hierarchy_kind(
            callable.kind,
            callable.container_name.is_some(),
        ),
        range: span_to_range(&snapshot.source, &callable.def_span),
        selection_range: span_to_range(&snapshot.source, &callable.lens_span),
        target_offset: callable.target_offset,
    }
}

fn callable_kind_to_call_hierarchy_kind(kind: SymbolKind, has_container: bool) -> LspSymbolKind {
    match kind {
        SymbolKind::Constructor => LspSymbolKind::CONSTRUCTOR,
        SymbolKind::Modifier => LspSymbolKind::METHOD,
        SymbolKind::Function if has_container => LspSymbolKind::METHOD,
        SymbolKind::Function => LspSymbolKind::FUNCTION,
        _ => symbol_kind_to_lsp(kind),
    }
}

fn callable_lens_span(source: &str, function: &ItemFunction<'_>) -> ByteRange<usize> {
    if let Some(name) = function.header.name {
        return solgrid_ast::span_to_range(name.span);
    }

    let start = solgrid_ast::span_to_range(function.header.span).start;
    let end = (start + function.kind.to_str().len()).min(source.len());
    start..end
}

fn local_span_range(
    span: ByteRange<usize>,
    source_base: usize,
    source_len: usize,
) -> ByteRange<usize> {
    let start = span.start.saturating_sub(source_base).min(source_len);
    let end = span.end.saturating_sub(source_base).min(source_len);
    start..end
}

fn range_snippet(source: &str, source_base: usize, span: ByteRange<usize>) -> String {
    let range = local_span_range(span, source_base, source.len());
    normalize_graph_text(&source[range])
}

fn stmt_snippet(source: &str, source_base: usize, stmt: &Stmt<'_>) -> String {
    range_snippet(source, source_base, solgrid_ast::span_to_range(stmt.span))
}

fn expr_snippet(source: &str, source_base: usize, expr: &solar_ast::Expr<'_>) -> String {
    range_snippet(source, source_base, solgrid_ast::span_to_range(expr.span))
}

fn yul_stmt_snippet(source: &str, source_base: usize, stmt: &yul::Stmt<'_>) -> String {
    range_snippet(source, source_base, solgrid_ast::span_to_range(stmt.span))
}

fn yul_expr_snippet(source: &str, source_base: usize, expr: &yul::Expr<'_>) -> String {
    range_snippet(source, source_base, solgrid_ast::span_to_range(expr.span))
}

fn stmt_node_descriptor(
    source: &str,
    source_base: usize,
    stmt: &Stmt<'_>,
) -> (String, String, GraphNodeKind) {
    let detail = stmt_snippet(source, source_base, stmt);
    match &stmt.kind {
        StmtKind::DeclSingle(def) => (
            declaration_label_from_var(source, source_base, def),
            detail,
            GraphNodeKind::Declaration,
        ),
        StmtKind::DeclMulti(vars, expr) => (
            multi_declaration_label(source, source_base, vars, expr),
            detail,
            GraphNodeKind::Declaration,
        ),
        StmtKind::Expr(expr) => expression_node_descriptor(source, source_base, expr, detail),
        StmtKind::Emit(path, _) => (
            format!(
                "emit {}",
                range_snippet(source, source_base, solgrid_ast::span_to_range(path.span()))
            ),
            detail,
            GraphNodeKind::Emit,
        ),
        _ => (
            compact_stmt_label(source, source_base, stmt),
            detail,
            GraphNodeKind::Statement,
        ),
    }
}

fn yul_stmt_node_descriptor(
    source: &str,
    source_base: usize,
    stmt: &yul::Stmt<'_>,
) -> (String, String, GraphNodeKind) {
    let detail = yul_stmt_snippet(source, source_base, stmt);
    match &stmt.kind {
        yul::StmtKind::AssignSingle(_, _) | yul::StmtKind::AssignMulti(_, _) => (
            truncate_graph_text(detail.trim_end_matches(';')),
            detail,
            GraphNodeKind::Assignment,
        ),
        yul::StmtKind::Expr(expr) => yul_expr_node_descriptor(source, source_base, expr, detail),
        yul::StmtKind::FunctionDef(function) => (
            format!("function {}", function.name.as_str()),
            detail,
            GraphNodeKind::Declaration,
        ),
        yul::StmtKind::VarDecl(_, _) => (
            truncate_graph_text(detail.trim_end_matches(';')),
            detail,
            GraphNodeKind::Declaration,
        ),
        _ => (
            truncate_graph_text(detail.trim_end_matches(';')),
            detail,
            GraphNodeKind::Statement,
        ),
    }
}

fn expression_node_descriptor(
    source: &str,
    source_base: usize,
    expr: &solar_ast::Expr<'_>,
    detail: String,
) -> (String, String, GraphNodeKind) {
    match &expr.peel_parens().kind {
        solar_ast::ExprKind::Assign(lhs, _, _) => (
            format!("assign {}", expr_snippet(source, source_base, lhs)),
            detail,
            GraphNodeKind::Assignment,
        ),
        solar_ast::ExprKind::Call(callee, _) | solar_ast::ExprKind::CallOptions(callee, _) => (
            format!("call {}", expr_snippet(source, source_base, callee)),
            detail,
            GraphNodeKind::Call,
        ),
        solar_ast::ExprKind::New(ty) => (
            format!(
                "instantiate {}",
                range_snippet(source, source_base, solgrid_ast::span_to_range(ty.span))
            ),
            detail,
            GraphNodeKind::Call,
        ),
        _ => (
            truncate_graph_text(detail.trim_end_matches(';')),
            detail,
            GraphNodeKind::Statement,
        ),
    }
}

fn yul_expr_node_descriptor(
    source: &str,
    source_base: usize,
    expr: &yul::Expr<'_>,
    detail: String,
) -> (String, String, GraphNodeKind) {
    match &expr.kind {
        yul::ExprKind::Call(call) => {
            let kind = if is_yul_log_builtin(call.name.as_str()) {
                GraphNodeKind::Emit
            } else {
                GraphNodeKind::Call
            };
            (format!("call {}", call.name.as_str()), detail, kind)
        }
        _ => (
            truncate_graph_text(yul_expr_snippet(source, source_base, expr).trim_end_matches(';')),
            detail,
            GraphNodeKind::Statement,
        ),
    }
}

fn is_yul_log_builtin(name: &str) -> bool {
    matches!(name, "log0" | "log1" | "log2" | "log3" | "log4")
}

fn declaration_label_from_var(
    source: &str,
    source_base: usize,
    def: &solar_ast::VariableDefinition<'_>,
) -> String {
    let name = def
        .name
        .map(|name| name.as_str().to_string())
        .unwrap_or_else(|| "value".to_string());
    let ty = range_snippet(source, source_base, solgrid_ast::span_to_range(def.ty.span));
    if def.initializer.is_some() {
        format!("declare {name}: {ty}")
    } else {
        format!("declare {name}")
    }
}

fn multi_declaration_label(
    source: &str,
    source_base: usize,
    vars: &[solgrid_parser::solar_interface::SpannedOption<solar_ast::VariableDefinition<'_>>],
    expr: &solar_ast::Expr<'_>,
) -> String {
    let names = vars
        .iter()
        .map(|var| match var {
            solgrid_parser::solar_interface::SpannedOption::Some(var) => var
                .name
                .map(|name| name.as_str().to_string())
                .unwrap_or_else(|| "_".to_string()),
            solgrid_parser::solar_interface::SpannedOption::None(_) => "_".to_string(),
        })
        .collect::<Vec<_>>();
    let rhs = expr.peel_parens();
    let prefix = match &rhs.kind {
        solar_ast::ExprKind::Call(callee, _) | solar_ast::ExprKind::CallOptions(callee, _) => {
            format!(
                "declare ({}) from call {}",
                names.join(", "),
                expr_snippet(source, source_base, callee)
            )
        }
        _ => format!("declare ({})", names.join(", ")),
    };
    truncate_graph_text(&prefix)
}

fn compact_stmt_label(source: &str, source_base: usize, stmt: &Stmt<'_>) -> String {
    let label = match &stmt.kind {
        StmtKind::Return(_) => "return".to_string(),
        StmtKind::Revert(_, _) => "revert".to_string(),
        StmtKind::Emit(path, _) => format!(
            "emit {}",
            range_snippet(source, source_base, solgrid_ast::span_to_range(path.span()))
        ),
        StmtKind::DeclSingle(_) | StmtKind::DeclMulti(_, _) | StmtKind::Expr(_) => {
            stmt_snippet(source, source_base, stmt)
        }
        StmtKind::Assembly(_) => "assembly".to_string(),
        StmtKind::Placeholder => "_".to_string(),
        _ => stmt_snippet(source, source_base, stmt),
    };
    truncate_graph_text(label.trim_end_matches(';'))
}

fn try_clause_label(
    source: &str,
    source_base: usize,
    clause: &solar_ast::TryCatchClause<'_>,
) -> String {
    let snippet = range_snippet(source, source_base, solgrid_ast::span_to_range(clause.span));
    let header = snippet.split('{').next().map(str::trim).unwrap_or("catch");
    truncate_graph_text(header)
}

fn assembly_label(
    source: &str,
    source_base: usize,
    assembly: &solar_ast::StmtAssembly<'_>,
) -> String {
    let mut parts = vec!["assembly".to_string()];
    if let Some(dialect) = &assembly.dialect {
        parts.push(range_snippet(
            source,
            source_base,
            solgrid_ast::span_to_range(dialect.span),
        ));
    }
    truncate_graph_text(&parts.join(" "))
}

fn yul_switch_case_label(
    source: &str,
    source_base: usize,
    case: &yul::StmtSwitchCase<'_>,
) -> String {
    match case.constant {
        Some(constant) => format!(
            "case {}",
            range_snippet(
                source,
                source_base,
                solgrid_ast::span_to_range(constant.span)
            )
        ),
        None => "default".to_string(),
    }
}

fn normalize_graph_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_graph_text(text: &str) -> String {
    const MAX_LABEL_CHARS: usize = 72;
    let mut truncated = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= MAX_LABEL_CHARS {
            truncated.push_str("...");
            return truncated;
        }
        truncated.push(ch);
    }
    truncated
}

fn reference_scan_names(
    snapshot: &ProjectSnapshot,
    target: &ReferenceTarget,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> HashSet<String> {
    let mut names = HashSet::from([target.name.clone()]);

    for import in &snapshot.table.imports {
        let ImportedSymbols::Named(imported_names) = &import.symbols else {
            continue;
        };

        for (original, alias) in imported_names {
            let local_name = alias.as_deref().unwrap_or(original);
            let Some(imported) = resolve_cross_file_symbol(
                &snapshot.table,
                local_name,
                &snapshot.path,
                get_source,
                resolver,
            ) else {
                continue;
            };
            let imported_target =
                reference_target_from_def(&imported.resolved_path, &imported.def, None);
            if imported_target == *target {
                names.insert(local_name.to_string());
            }
        }
    }

    names
}

/// Resolve a symbol name by searching the current file's imports transitively.
pub fn resolve_cross_file_symbol(
    current_table: &SymbolTable,
    name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<CrossFileSymbol> {
    let mut visited = HashSet::new();
    resolve_cross_file_symbol_inner(
        current_table,
        name,
        &normalize_path(importing_file),
        get_source,
        resolver,
        &mut visited,
    )
}

fn resolve_cross_file_symbol_inner(
    current_table: &SymbolTable,
    name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    visited: &mut HashSet<PathBuf>,
) -> Option<CrossFileSymbol> {
    for import in &current_table.imports {
        let target_name = match &import.symbols {
            ImportedSymbols::Named(names) => {
                let mut found = None;
                for (original, alias) in names {
                    let local_name = alias.as_deref().unwrap_or(original.as_str());
                    if local_name == name {
                        found = Some(original.as_str());
                        break;
                    }
                }
                match found {
                    Some(found) => found,
                    None => continue,
                }
            }
            ImportedSymbols::Plain(None) => name,
            ImportedSymbols::Plain(Some(_)) | ImportedSymbols::Glob(_) => continue,
        };

        let Some(resolved) = resolver.resolve(&import.path, importing_file) else {
            continue;
        };
        let resolved = normalize_path(&resolved);

        if !visited.insert(resolved.clone()) {
            continue;
        }

        let Some(imported_source) = get_source(&resolved) else {
            continue;
        };
        let filename = resolved.to_string_lossy().to_string();
        let Some(imported_table) = symbols::build_symbol_table(&imported_source, &filename) else {
            continue;
        };

        let definitions = imported_table.resolve_all(target_name, 0);
        if definitions.len() == 1 {
            let def = definitions[0].clone();
            return Some(CrossFileSymbol {
                source: imported_source,
                table: imported_table,
                def,
                resolved_path: resolved,
            });
        }

        if let Some(result) = resolve_cross_file_symbol_inner(
            &imported_table,
            target_name,
            &resolved,
            get_source,
            resolver,
            visited,
        ) {
            return Some(result);
        }
    }

    None
}

/// Resolve a `Container.member` access across file boundaries.
pub fn resolve_cross_file_member_symbol(
    current_table: &SymbolTable,
    container_name: &str,
    member_name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<CrossFileSymbol> {
    let mut visited = HashSet::new();
    resolve_cross_file_member_symbol_inner(
        current_table,
        container_name,
        member_name,
        &normalize_path(importing_file),
        get_source,
        resolver,
        &mut visited,
    )
}

fn resolve_cross_file_member_symbol_inner(
    current_table: &SymbolTable,
    container_name: &str,
    member_name: &str,
    importing_file: &Path,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    visited: &mut HashSet<PathBuf>,
) -> Option<CrossFileSymbol> {
    for import in &current_table.imports {
        let target = match &import.symbols {
            ImportedSymbols::Named(names) => {
                let mut found = None;
                for (original, alias) in names {
                    let local_name = alias.as_deref().unwrap_or(original.as_str());
                    if local_name == container_name {
                        found = Some(original.as_str());
                        break;
                    }
                }
                match found {
                    Some(found) => (found, false),
                    None => continue,
                }
            }
            ImportedSymbols::Plain(None) => (container_name, false),
            ImportedSymbols::Plain(Some(alias)) if alias == container_name => (member_name, true),
            ImportedSymbols::Glob(alias) if alias == container_name => (member_name, true),
            _ => continue,
        };

        let Some(resolved) = resolver.resolve(&import.path, importing_file) else {
            continue;
        };
        let resolved = normalize_path(&resolved);

        if !visited.insert(resolved.clone()) {
            continue;
        }

        let Some(imported_source) = get_source(&resolved) else {
            continue;
        };
        let filename = resolved.to_string_lossy().to_string();
        let Some(imported_table) = symbols::build_symbol_table(&imported_source, &filename) else {
            continue;
        };

        if target.1 {
            let definitions = imported_table.resolve_all(target.0, 0);
            if definitions.len() == 1 {
                let def = definitions[0].clone();
                return Some(CrossFileSymbol {
                    source: imported_source,
                    table: imported_table,
                    def,
                    resolved_path: resolved,
                });
            }
        }

        if let Some(container_def) = imported_table.resolve(target.0, 0) {
            let member_definitions = imported_table.resolve_member_all(container_def, member_name);
            if member_definitions.len() == 1 {
                let def = member_definitions[0].clone();
                return Some(CrossFileSymbol {
                    source: imported_source,
                    table: imported_table,
                    def,
                    resolved_path: resolved,
                });
            }
        }

        if let Some(result) = resolve_cross_file_member_symbol_inner(
            &imported_table,
            target.0,
            member_name,
            &resolved,
            get_source,
            resolver,
            visited,
        ) {
            return Some(result);
        }
    }

    None
}

/// Resolve the semantic reference target at a byte offset in an existing snapshot.
///
/// The caller supplies the import resolver so editor surfaces with custom
/// remappings can share the same inherited-member behavior.
pub fn resolve_reference_target_at_offset(
    snapshot: &ProjectSnapshot,
    offset: usize,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<ReferenceTarget> {
    if let Some((container, _member, member_range)) =
        symbols::find_member_access_at_offset(&snapshot.source, offset)
    {
        let member_name = &snapshot.source[member_range.clone()];
        let pseudo_receiver_access = match container.as_str() {
            "this" => Some(InheritedMemberAccess::This),
            "super" => Some(InheritedMemberAccess::Super),
            _ => None,
        };
        if let Some(access) = pseudo_receiver_access {
            let targets = resolve_effective_member_targets(
                snapshot,
                offset,
                member_name,
                access,
                get_source,
                resolver,
            );
            let [target] = targets.as_slice() else {
                return None;
            };
            return Some(reference_target_from_def(
                &target.snapshot.path,
                &target.def,
                Some(container),
            ));
        }

        if let Some(container_def) = snapshot.table.resolve(&container, offset) {
            let definitions = snapshot
                .table
                .resolve_member_all(container_def, member_name);
            if definitions.len() == 1 {
                return Some(reference_target_from_def(
                    &snapshot.path,
                    definitions[0],
                    Some(container),
                ));
            }
            if !definitions.is_empty() {
                return None;
            }
        }

        if let Some(cross) = resolve_cross_file_member_symbol(
            &snapshot.table,
            &container,
            member_name,
            &snapshot.path,
            get_source,
            resolver,
        ) {
            return Some(reference_target_from_def(
                &cross.resolved_path,
                &cross.def,
                Some(container),
            ));
        }

        return None;
    }

    let (name, ident_range) = symbols::find_ident_at_offset(&snapshot.source, offset)?;

    if let Some(definition) = snapshot.table.definition_at_name_span(&ident_range) {
        return Some(reference_target_from_def(&snapshot.path, definition, None));
    }

    if let Some(target) = natspec_bound_parameter_target(snapshot, &ident_range) {
        return Some(target);
    }

    if identifier_may_be_named_argument_key(&snapshot.source, &ident_range) {
        match named_argument_target_at_offset(snapshot, &ident_range, get_source, resolver) {
            NamedArgumentResolution::Resolved(target) => return Some(target),
            NamedArgumentResolution::Ambiguous => return None,
            NamedArgumentResolution::NotNamedArgument => {}
        }
    }

    let definitions = snapshot.table.resolve_all(&name, offset);
    if definitions.len() == 1 {
        return Some(reference_target_from_def(
            &snapshot.path,
            definitions[0],
            None,
        ));
    }
    if !definitions.is_empty() {
        return None;
    }

    let inherited = resolve_effective_member_targets(
        snapshot,
        offset,
        &name,
        InheritedMemberAccess::Unqualified,
        get_source,
        resolver,
    );
    if let [target] = inherited.as_slice() {
        return Some(reference_target_from_def(
            &target.snapshot.path,
            &target.def,
            None,
        ));
    }
    if !inherited.is_empty() {
        return None;
    }

    if let Some(target) =
        import_clause_reference_target(snapshot, &name, &ident_range, get_source, resolver)
    {
        return Some(target);
    }

    if let Some(cross) =
        resolve_cross_file_symbol(&snapshot.table, &name, &snapshot.path, get_source, resolver)
    {
        return Some(reference_target_from_def(
            &cross.resolved_path,
            &cross.def,
            None,
        ));
    }

    None
}

fn identifier_may_be_named_argument_key(source: &str, span: &ByteRange<usize>) -> bool {
    source.as_bytes().get(span.end..).and_then(|suffix| {
        suffix
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
    }) == Some(b':')
}

fn natspec_bound_parameter_target(
    snapshot: &ProjectSnapshot,
    ident_span: &ByteRange<usize>,
) -> Option<ReferenceTarget> {
    let line_start = solgrid_ast::natspec::line_start(&snapshot.source, ident_span.start);
    let line_prefix = &snapshot.source[line_start..ident_span.start];
    let expected_kind = if line_prefix.contains("@param") {
        SymbolKind::Parameter
    } else if line_prefix.contains("@return") {
        SymbolKind::ReturnParameter
    } else {
        return None;
    };
    let name = &snapshot.source[ident_span.clone()];
    let filename = snapshot.path.to_string_lossy().to_string();

    with_parsed_ast_sequential(&snapshot.source, &filename, |source_unit| {
        fn find_in_items<'a, 'ast>(
            snapshot: &ProjectSnapshot,
            items: impl Iterator<Item = &'a solar_ast::Item<'ast>>,
            ident_span: &ByteRange<usize>,
            name: &str,
            expected_kind: SymbolKind,
        ) -> Option<ReferenceTarget>
        where
            'ast: 'a,
        {
            for item in items {
                match &item.kind {
                    ItemKind::Contract(contract) => {
                        if let Some(target) = find_in_items(
                            snapshot,
                            contract.body.iter(),
                            ident_span,
                            name,
                            expected_kind,
                        ) {
                            return Some(target);
                        }
                    }
                    ItemKind::Function(function) => {
                        let item_span = solgrid_ast::span_to_range(item.span);
                        let Some(block) = solgrid_ast::natspec::find_attached_natspec(
                            &snapshot.source,
                            item_span.start,
                        ) else {
                            continue;
                        };
                        if !block.range.contains(&ident_span.start) {
                            continue;
                        }
                        let function_definition = if let Some(function_name) = function.header.name
                        {
                            snapshot
                                .table
                                .definition_at_name_span(&solgrid_ast::span_to_range(
                                    function_name.span,
                                ))
                        } else {
                            None
                        }?;
                        let scope_id = function_definition.scope?;
                        let matches = snapshot
                            .table
                            .scope_symbols(scope_id)
                            .iter()
                            .filter(|definition| {
                                definition.kind == expected_kind && definition.name == name
                            })
                            .collect::<Vec<_>>();
                        if let [definition] = matches.as_slice() {
                            return Some(reference_target_from_def(
                                &snapshot.path,
                                definition,
                                None,
                            ));
                        }
                        return None;
                    }
                    _ => {}
                }
            }
            None
        }

        find_in_items(
            snapshot,
            source_unit.items.iter(),
            ident_span,
            name,
            expected_kind,
        )
    })
    .ok()
    .flatten()
}

enum NamedArgumentResolution {
    NotNamedArgument,
    Resolved(ReferenceTarget),
    Ambiguous,
}

struct NamedArgumentFinder<'a> {
    snapshot: &'a ProjectSnapshot,
    target_span: &'a ByteRange<usize>,
    get_source: &'a dyn Fn(&Path) -> Option<String>,
    resolver: &'a ImportResolver,
    saw_named_argument: bool,
    targets: Vec<ReferenceTarget>,
}

fn named_argument_target_at_offset(
    snapshot: &ProjectSnapshot,
    target_span: &ByteRange<usize>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> NamedArgumentResolution {
    let filename = snapshot.path.to_string_lossy().to_string();
    let (saw_named_argument, targets) =
        with_parsed_ast_sequential(&snapshot.source, &filename, |source_unit| {
            let mut finder = NamedArgumentFinder {
                snapshot,
                target_span,
                get_source,
                resolver,
                saw_named_argument: false,
                targets: Vec::new(),
            };
            for item in source_unit.items.iter() {
                scan_named_argument_item(item, &mut finder);
            }
            (finder.saw_named_argument, finder.targets)
        })
        .unwrap_or_default();

    if !saw_named_argument {
        NamedArgumentResolution::NotNamedArgument
    } else if let [target] = targets.as_slice() {
        NamedArgumentResolution::Resolved(target.clone())
    } else {
        NamedArgumentResolution::Ambiguous
    }
}

fn named_call_argument_targets(
    snapshot: &ProjectSnapshot,
    callee: &solar_ast::Expr<'_>,
    argument_names: &[&str],
    target_name: &str,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<ReferenceTarget> {
    let callee = callee.peel_parens();
    let mut targets = Vec::new();

    match &callee.kind {
        solar_ast::ExprKind::CallOptions(inner, _) => {
            return named_call_argument_targets(
                snapshot,
                inner,
                argument_names,
                target_name,
                get_source,
                resolver,
            );
        }
        solar_ast::ExprKind::Ident(identifier) => {
            let offset = solgrid_ast::span_to_range(identifier.span).start;
            let definitions = snapshot.table.resolve_all(identifier.as_str(), offset);
            collect_named_key_targets(
                &snapshot.table,
                &snapshot.path,
                definitions.iter().copied(),
                argument_names,
                target_name,
                &mut targets,
            );

            if definitions.is_empty() {
                if let Some(cross) = resolve_cross_file_symbol(
                    &snapshot.table,
                    identifier.as_str(),
                    &snapshot.path,
                    get_source,
                    resolver,
                ) {
                    collect_named_key_targets(
                        &cross.table,
                        &cross.resolved_path,
                        [&cross.def],
                        argument_names,
                        target_name,
                        &mut targets,
                    );
                }
            }
        }
        solar_ast::ExprKind::Member(container_expr, member) => {
            let container_expr = container_expr.peel_parens();
            let solar_ast::ExprKind::Ident(container) = &container_expr.kind else {
                return targets;
            };
            let offset = solgrid_ast::span_to_range(member.span).start;
            let container_definition = if container.as_str() == "this" {
                snapshot
                    .table
                    .file_level_symbols()
                    .iter()
                    .find(|definition| {
                        matches!(
                            definition.kind,
                            SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
                        ) && definition.def_span.contains(&offset)
                    })
            } else {
                snapshot.table.resolve(container.as_str(), offset)
            };
            if let Some(container_definition) = container_definition {
                collect_named_key_targets(
                    &snapshot.table,
                    &snapshot.path,
                    snapshot
                        .table
                        .resolve_member_all(container_definition, member.as_str()),
                    argument_names,
                    target_name,
                    &mut targets,
                );
                collect_typed_container_named_key_targets(
                    snapshot,
                    container_definition,
                    member.as_str(),
                    argument_names,
                    target_name,
                    get_source,
                    resolver,
                    &mut targets,
                );
            }
        }
        _ => {}
    }

    targets
}

fn named_path_call_argument_targets(
    snapshot: &ProjectSnapshot,
    path: &solar_ast::AstPath<'_>,
    argument_names: &[&str],
    target_name: &str,
    expected_kind: SymbolKind,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<ReferenceTarget> {
    let segments = path
        .segments()
        .iter()
        .map(|segment| segment.as_str())
        .collect::<Vec<_>>();
    let Some(first) = segments.first().copied() else {
        return Vec::new();
    };
    let offset = solgrid_ast::span_to_range(path.span()).start;
    let mut targets = Vec::new();

    if segments.len() == 1 {
        let local = snapshot.table.resolve_all(first, offset);
        if !local.is_empty() {
            collect_named_key_targets(
                &snapshot.table,
                &snapshot.path,
                local
                    .into_iter()
                    .filter(|definition| definition.kind == expected_kind),
                argument_names,
                target_name,
                &mut targets,
            );
            return targets;
        }

        if let Some(cross) =
            resolve_cross_file_symbol(&snapshot.table, first, &snapshot.path, get_source, resolver)
        {
            if cross.def.kind == expected_kind {
                collect_named_key_targets(
                    &cross.table,
                    &cross.resolved_path,
                    [&cross.def],
                    argument_names,
                    target_name,
                    &mut targets,
                );
            }
        }
        collect_inherited_named_path_call_targets(
            snapshot,
            offset,
            first,
            expected_kind,
            argument_names,
            target_name,
            get_source,
            resolver,
            &mut targets,
        );
        return targets;
    }

    if segments.len() == 2 {
        let local_containers = snapshot.table.resolve_all(first, offset);
        if !local_containers.is_empty() {
            for container in local_containers {
                collect_named_key_targets(
                    &snapshot.table,
                    &snapshot.path,
                    snapshot
                        .table
                        .resolve_member_all(container, segments[1])
                        .into_iter()
                        .filter(|definition| definition.kind == expected_kind),
                    argument_names,
                    target_name,
                    &mut targets,
                );
            }
            return targets;
        }

        if let Some(cross) = resolve_cross_file_member_symbol(
            &snapshot.table,
            first,
            segments[1],
            &snapshot.path,
            get_source,
            resolver,
        ) {
            if cross.def.kind == expected_kind {
                collect_named_key_targets(
                    &cross.table,
                    &cross.resolved_path,
                    [&cross.def],
                    argument_names,
                    target_name,
                    &mut targets,
                );
            }
        }
        return targets;
    }

    let Some(cross) = resolve_cross_file_member_symbol(
        &snapshot.table,
        first,
        segments[1],
        &snapshot.path,
        get_source,
        resolver,
    ) else {
        return targets;
    };
    let mut current = cross.def.clone();
    for segment in &segments[2..segments.len() - 1] {
        let definitions = cross.table.resolve_member_all(&current, segment);
        let [definition] = definitions.as_slice() else {
            return targets;
        };
        current = (*definition).clone();
    }
    collect_named_key_targets(
        &cross.table,
        &cross.resolved_path,
        cross
            .table
            .resolve_member_all(&current, segments[segments.len() - 1])
            .into_iter()
            .filter(|definition| definition.kind == expected_kind),
        argument_names,
        target_name,
        &mut targets,
    );

    targets
}

#[allow(clippy::too_many_arguments)]
fn collect_inherited_named_path_call_targets(
    snapshot: &ProjectSnapshot,
    offset: usize,
    member_name: &str,
    expected_kind: SymbolKind,
    argument_names: &[&str],
    target_name: &str,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    targets: &mut Vec<ReferenceTarget>,
) {
    let Some(contract_def) = snapshot
        .table
        .file_level_symbols()
        .iter()
        .find(|definition| {
            is_contract_container_symbol(definition.kind) && definition.def_span.contains(&offset)
        })
    else {
        return;
    };
    let mut visited = HashSet::new();
    collect_inherited_named_path_call_targets_from_contract(
        snapshot,
        contract_def,
        member_name,
        expected_kind,
        argument_names,
        target_name,
        get_source,
        resolver,
        targets,
        &mut visited,
    );
}

#[allow(clippy::too_many_arguments)]
fn collect_inherited_named_path_call_targets_from_contract(
    snapshot: &ProjectSnapshot,
    contract_def: &SymbolDef,
    member_name: &str,
    expected_kind: SymbolKind,
    argument_names: &[&str],
    target_name: &str,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    targets: &mut Vec<ReferenceTarget>,
    visited: &mut HashSet<(PathBuf, usize)>,
) {
    let Some(contract) = find_contract_decl(snapshot, contract_def) else {
        return;
    };
    for base in &contract.bases {
        let Some(resolved) = resolve_contract_type_path(snapshot, base, get_source, resolver)
        else {
            continue;
        };
        let visit_key = (resolved.snapshot.path.clone(), resolved.def.name_span.start);
        if !visited.insert(visit_key) {
            continue;
        }

        collect_named_key_targets(
            &resolved.snapshot.table,
            &resolved.snapshot.path,
            resolved
                .snapshot
                .table
                .resolve_member_all(&resolved.def, member_name)
                .into_iter()
                .filter(|definition| definition.kind == expected_kind),
            argument_names,
            target_name,
            targets,
        );
        collect_inherited_named_path_call_targets_from_contract(
            &resolved.snapshot,
            &resolved.def,
            member_name,
            expected_kind,
            argument_names,
            target_name,
            get_source,
            resolver,
            targets,
            visited,
        );
    }
}

fn resolve_contract_type_path(
    snapshot: &ProjectSnapshot,
    path: &TypePath,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<ResolvedPathTarget> {
    let [first, rest @ ..] = path.segments.as_slice() else {
        return None;
    };

    if let Some(definition) = snapshot.table.resolve(first, 0) {
        let mut definition = definition.clone();
        for segment in rest {
            let definitions = snapshot.table.resolve_member_all(&definition, segment);
            let [next] = definitions.as_slice() else {
                return None;
            };
            definition = (*next).clone();
        }
        return is_contract_container_symbol(definition.kind).then(|| ResolvedPathTarget {
            snapshot: snapshot.clone(),
            def: definition,
        });
    }

    let cross = if rest.is_empty() {
        resolve_cross_file_symbol(&snapshot.table, first, &snapshot.path, get_source, resolver)
    } else {
        let mut cross = resolve_cross_file_member_symbol(
            &snapshot.table,
            first,
            &rest[0],
            &snapshot.path,
            get_source,
            resolver,
        )?;
        for segment in &rest[1..] {
            let definitions = cross.table.resolve_member_all(&cross.def, segment);
            let [next] = definitions.as_slice() else {
                return None;
            };
            cross.def = (*next).clone();
        }
        Some(cross)
    }?;
    if !is_contract_container_symbol(cross.def.kind) {
        return None;
    }
    Some(ResolvedPathTarget {
        snapshot: ProjectSnapshot {
            path: cross.resolved_path.clone(),
            source: cross.source.clone(),
            table: cross.table,
            contracts: collect_contract_declarations(
                &cross.source,
                &cross.resolved_path.to_string_lossy(),
            ),
            callables: collect_callable_declarations(
                &cross.source,
                &cross.resolved_path.to_string_lossy(),
            ),
        },
        def: cross.def,
    })
}

#[derive(Clone, Copy)]
enum InheritedMemberAccess {
    Unqualified,
    This,
    Super,
}

fn resolve_effective_member_targets(
    snapshot: &ProjectSnapshot,
    offset: usize,
    member_name: &str,
    access: InheritedMemberAccess,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Vec<ResolvedPathTarget> {
    let Some(contract_def) = snapshot
        .table
        .file_level_symbols()
        .iter()
        .find(|definition| {
            is_contract_container_symbol(definition.kind) && definition.def_span.contains(&offset)
        })
        .cloned()
    else {
        return Vec::new();
    };
    let current = ResolvedPathTarget {
        snapshot: snapshot.clone(),
        def: contract_def,
    };
    let mut cache = HashMap::new();
    let mut active = HashSet::new();
    let Some(linearized) =
        linearized_contract_targets(&current, get_source, resolver, &mut cache, &mut active)
    else {
        return Vec::new();
    };

    let skip_current = matches!(
        access,
        InheritedMemberAccess::Unqualified | InheritedMemberAccess::Super
    );
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    for container in linearized.into_iter().skip(usize::from(skip_current)) {
        for member in container
            .snapshot
            .table
            .resolve_member_all(&container.def, member_name)
        {
            // An inaccessible override still shadows the same signature lower
            // in the hierarchy, so record the identity before filtering.
            if !seen.insert(inherited_member_lookup_identity(member)) {
                continue;
            }
            if !inherited_member_is_accessible(member, access) {
                continue;
            }
            targets.push(ResolvedPathTarget {
                snapshot: container.snapshot.clone(),
                def: member.clone(),
            });
        }
    }
    targets
}

fn inherited_member_is_accessible(def: &SymbolDef, access: InheritedMemberAccess) -> bool {
    match access {
        InheritedMemberAccess::This => match def.kind {
            SymbolKind::Function => matches!(
                def.visibility,
                Some(Visibility::Public | Visibility::External)
            ),
            SymbolKind::StateVariable => def.visibility == Some(Visibility::Public),
            _ => false,
        },
        InheritedMemberAccess::Unqualified | InheritedMemberAccess::Super => {
            def.visibility != Some(Visibility::Private)
                && !(def.kind == SymbolKind::Function
                    && def.visibility == Some(Visibility::External))
        }
    }
}

fn inherited_member_lookup_identity(def: &SymbolDef) -> (SymbolKind, String) {
    if let Some(signature) = public_state_getter_signature(def) {
        return (SymbolKind::Function, signature);
    }
    let signature = callable_signature_identity(def).unwrap_or_else(|| def.name.clone());
    (def.kind, signature)
}

fn linearized_contract_targets(
    current: &ResolvedPathTarget,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    cache: &mut HashMap<(PathBuf, usize), Vec<ResolvedPathTarget>>,
    active: &mut HashSet<(PathBuf, usize)>,
) -> Option<Vec<ResolvedPathTarget>> {
    let key = resolved_path_target_key(current);
    if let Some(cached) = cache.get(&key) {
        return Some(cached.clone());
    }
    if !active.insert(key.clone()) {
        return None;
    }

    let Some(contract) = find_contract_decl(&current.snapshot, &current.def) else {
        active.remove(&key);
        return None;
    };
    let mut direct_bases = Vec::new();
    for base in &contract.bases {
        let Some(resolved) =
            resolve_contract_type_path(&current.snapshot, base, get_source, resolver)
        else {
            active.remove(&key);
            return None;
        };
        direct_bases.push(resolved);
    }
    direct_bases.reverse();

    let mut sequences = Vec::new();
    for base in &direct_bases {
        let Some(linearized) =
            linearized_contract_targets(base, get_source, resolver, cache, active)
        else {
            active.remove(&key);
            return None;
        };
        sequences.push(linearized);
    }
    sequences.push(direct_bases);
    let Some(merged) = merge_linearized_contract_targets(sequences) else {
        active.remove(&key);
        return None;
    };

    let mut result = Vec::with_capacity(1 + merged.len());
    result.push(current.clone());
    result.extend(merged);
    active.remove(&key);
    cache.insert(key, result.clone());
    Some(result)
}

fn resolved_path_target_key(target: &ResolvedPathTarget) -> (PathBuf, usize) {
    (target.snapshot.path.clone(), target.def.name_span.start)
}

fn merge_linearized_contract_targets(
    mut sequences: Vec<Vec<ResolvedPathTarget>>,
) -> Option<Vec<ResolvedPathTarget>> {
    let mut result = Vec::new();
    loop {
        sequences.retain(|sequence| !sequence.is_empty());
        if sequences.is_empty() {
            return Some(result);
        }

        let mut selected = None;
        for sequence in &sequences {
            let candidate = sequence.first()?.clone();
            let candidate_key = resolved_path_target_key(&candidate);
            let blocked = sequences.iter().any(|other| {
                other
                    .iter()
                    .skip(1)
                    .any(|item| resolved_path_target_key(item) == candidate_key)
            });
            if !blocked {
                selected = Some(candidate);
                break;
            }
        }

        let selected = selected?;
        let selected_key = resolved_path_target_key(&selected);
        result.push(selected);
        for sequence in &mut sequences {
            sequence.retain(|item| resolved_path_target_key(item) != selected_key);
        }
    }
}

fn is_contract_container_symbol(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
    )
}

#[allow(clippy::too_many_arguments)]
fn collect_typed_container_named_key_targets(
    snapshot: &ProjectSnapshot,
    container_definition: &SymbolDef,
    member_name: &str,
    argument_names: &[&str],
    target_name: &str,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
    targets: &mut Vec<ReferenceTarget>,
) {
    let Some(type_path) = container_definition
        .type_info
        .as_ref()
        .and_then(|ty| ty.member_target())
    else {
        return;
    };
    let Some(root_name) = type_path.segments.first() else {
        return;
    };
    let resolve_offset = container_definition
        .type_info
        .as_ref()
        .map_or(0, |ty| ty.resolve_offset());

    let local_roots = snapshot.table.resolve_all(root_name, resolve_offset);
    if !local_roots.is_empty() {
        for root in local_roots {
            collect_named_keys_from_type_root(
                &snapshot.table,
                &snapshot.path,
                root,
                &type_path.segments[1..],
                member_name,
                argument_names,
                target_name,
                targets,
            );
        }
        return;
    }

    if let Some(cross) = resolve_cross_file_symbol(
        &snapshot.table,
        root_name,
        &snapshot.path,
        get_source,
        resolver,
    ) {
        collect_named_keys_from_type_root(
            &cross.table,
            &cross.resolved_path,
            &cross.def,
            &type_path.segments[1..],
            member_name,
            argument_names,
            target_name,
            targets,
        );
        return;
    }

    let Some(second_segment) = type_path.segments.get(1) else {
        return;
    };
    if let Some(cross) = resolve_cross_file_member_symbol(
        &snapshot.table,
        root_name,
        second_segment,
        &snapshot.path,
        get_source,
        resolver,
    ) {
        collect_named_keys_from_type_root(
            &cross.table,
            &cross.resolved_path,
            &cross.def,
            &type_path.segments[2..],
            member_name,
            argument_names,
            target_name,
            targets,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_named_keys_from_type_root(
    table: &SymbolTable,
    path: &Path,
    root: &SymbolDef,
    remaining_type_path: &[String],
    member_name: &str,
    argument_names: &[&str],
    target_name: &str,
    targets: &mut Vec<ReferenceTarget>,
) {
    let mut current = root.clone();
    for segment in remaining_type_path {
        let definitions = table.resolve_member_all(&current, segment);
        let [definition] = definitions.as_slice() else {
            return;
        };
        current = (*definition).clone();
    }

    collect_named_key_targets(
        table,
        path,
        table.resolve_member_all(&current, member_name),
        argument_names,
        target_name,
        targets,
    );
}

fn collect_named_key_targets<'a, I>(
    table: &SymbolTable,
    path: &Path,
    definitions: I,
    argument_names: &[&str],
    target_name: &str,
    targets: &mut Vec<ReferenceTarget>,
) where
    I: IntoIterator<Item = &'a SymbolDef>,
{
    for definition in definitions {
        let member_kind = match definition.kind {
            SymbolKind::Function | SymbolKind::Modifier => SymbolKind::Parameter,
            SymbolKind::Event | SymbolKind::Error => SymbolKind::Parameter,
            SymbolKind::Struct => SymbolKind::StructField,
            _ => continue,
        };
        let Some(scope_id) = definition.scope else {
            continue;
        };
        let members = table
            .scope_symbols(scope_id)
            .iter()
            .filter(|member| member.kind == member_kind)
            .collect::<Vec<_>>();
        if members.len() != argument_names.len()
            || !argument_names
                .iter()
                .all(|name| members.iter().any(|member| member.name == *name))
        {
            continue;
        }
        let Some(member) = members.iter().find(|member| member.name == target_name) else {
            continue;
        };
        let target = reference_target_from_def(path, member, None);
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
}

fn scan_named_argument_item(item: &solar_ast::Item<'_>, finder: &mut NamedArgumentFinder<'_>) {
    match &item.kind {
        ItemKind::Contract(contract) => {
            for base in contract.bases.iter() {
                for argument in base.arguments.exprs() {
                    scan_named_argument_expr(argument, finder);
                }
            }
            for body_item in contract.body.iter() {
                scan_named_argument_item(body_item, finder);
            }
        }
        ItemKind::Function(function) => {
            for modifier in function.header.modifiers.iter() {
                for argument in modifier.arguments.exprs() {
                    scan_named_argument_expr(argument, finder);
                }
            }
            if let Some(body) = &function.body {
                scan_named_argument_stmts(body.stmts, finder);
            }
        }
        ItemKind::Variable(variable) => {
            if let Some(initializer) = &variable.initializer {
                scan_named_argument_expr(initializer, finder);
            }
        }
        _ => {}
    }
}

fn scan_named_path_call_arguments(
    path: &solar_ast::AstPath<'_>,
    arguments: &solar_ast::CallArgs<'_>,
    expected_kind: SymbolKind,
    finder: &mut NamedArgumentFinder<'_>,
) {
    let CallArgsKind::Named(arguments) = &arguments.kind else {
        return;
    };
    let Some(argument) = arguments
        .iter()
        .find(|argument| solgrid_ast::span_to_range(argument.name.span) == *finder.target_span)
    else {
        return;
    };

    finder.saw_named_argument = true;
    let argument_names = arguments
        .iter()
        .map(|argument| argument.name.as_str())
        .collect::<Vec<_>>();
    for target in named_path_call_argument_targets(
        finder.snapshot,
        path,
        &argument_names,
        argument.name.as_str(),
        expected_kind,
        finder.get_source,
        finder.resolver,
    ) {
        if !finder.targets.contains(&target) {
            finder.targets.push(target);
        }
    }
}

fn scan_named_argument_stmts(stmts: &[Stmt<'_>], finder: &mut NamedArgumentFinder<'_>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assembly(_)
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Placeholder => {}
            StmtKind::Block(block) | StmtKind::UncheckedBlock(block) => {
                scan_named_argument_stmts(block.stmts, finder);
            }
            StmtKind::DeclSingle(variable) => {
                if let Some(initializer) = &variable.initializer {
                    scan_named_argument_expr(initializer, finder);
                }
            }
            StmtKind::DeclMulti(_, expression)
            | StmtKind::Expr(expression)
            | StmtKind::Return(Some(expression)) => scan_named_argument_expr(expression, finder),
            StmtKind::Return(None) => {}
            StmtKind::DoWhile(body, condition) | StmtKind::While(condition, body) => {
                scan_named_argument_stmts(std::slice::from_ref(body), finder);
                scan_named_argument_expr(condition, finder);
            }
            StmtKind::Emit(path, arguments) => {
                scan_named_path_call_arguments(path, arguments, SymbolKind::Event, finder);
                for argument in arguments.exprs() {
                    scan_named_argument_expr(argument, finder);
                }
            }
            StmtKind::Revert(path, arguments) => {
                scan_named_path_call_arguments(path, arguments, SymbolKind::Error, finder);
                for argument in arguments.exprs() {
                    scan_named_argument_expr(argument, finder);
                }
            }
            StmtKind::For {
                init,
                cond,
                next,
                body,
            } => {
                if let Some(init) = init {
                    scan_named_argument_stmts(std::slice::from_ref(init), finder);
                }
                if let Some(condition) = cond {
                    scan_named_argument_expr(condition, finder);
                }
                if let Some(next) = next {
                    scan_named_argument_expr(next, finder);
                }
                scan_named_argument_stmts(std::slice::from_ref(body), finder);
            }
            StmtKind::If(condition, then_branch, else_branch) => {
                scan_named_argument_expr(condition, finder);
                scan_named_argument_stmts(std::slice::from_ref(then_branch), finder);
                if let Some(else_branch) = else_branch {
                    scan_named_argument_stmts(std::slice::from_ref(else_branch), finder);
                }
            }
            StmtKind::Try(try_stmt) => {
                scan_named_argument_expr(try_stmt.expr, finder);
                for clause in try_stmt.clauses.iter() {
                    scan_named_argument_stmts(clause.block.stmts, finder);
                }
            }
        }
    }
}

fn scan_named_argument_expr(expr: &solar_ast::Expr<'_>, finder: &mut NamedArgumentFinder<'_>) {
    use solgrid_parser::solar_interface::SpannedOption;

    if let solar_ast::ExprKind::Call(callee, arguments) = &expr.kind {
        if let CallArgsKind::Named(arguments) = &arguments.kind {
            if let Some(argument) = arguments.iter().find(|argument| {
                solgrid_ast::span_to_range(argument.name.span) == *finder.target_span
            }) {
                finder.saw_named_argument = true;
                let argument_names = arguments
                    .iter()
                    .map(|argument| argument.name.as_str())
                    .collect::<Vec<_>>();
                for target in named_call_argument_targets(
                    finder.snapshot,
                    callee,
                    &argument_names,
                    argument.name.as_str(),
                    finder.get_source,
                    finder.resolver,
                ) {
                    if !finder.targets.contains(&target) {
                        finder.targets.push(target);
                    }
                }
            }
        }
    }

    match &expr.kind {
        solar_ast::ExprKind::Array(expressions) => {
            for expression in expressions.iter() {
                scan_named_argument_expr(expression, finder);
            }
        }
        solar_ast::ExprKind::Assign(left, _, right)
        | solar_ast::ExprKind::Binary(left, _, right) => {
            scan_named_argument_expr(left, finder);
            scan_named_argument_expr(right, finder);
        }
        solar_ast::ExprKind::Call(callee, arguments) => {
            scan_named_argument_expr(callee, finder);
            for argument in arguments.exprs() {
                scan_named_argument_expr(argument, finder);
            }
        }
        solar_ast::ExprKind::CallOptions(callee, arguments) => {
            scan_named_argument_expr(callee, finder);
            for argument in arguments.iter() {
                scan_named_argument_expr(argument.value, finder);
            }
        }
        solar_ast::ExprKind::Delete(expression) | solar_ast::ExprKind::Unary(_, expression) => {
            scan_named_argument_expr(expression, finder);
        }
        solar_ast::ExprKind::Index(expression, index) => {
            scan_named_argument_expr(expression, finder);
            match index {
                solar_ast::IndexKind::Index(Some(index)) => scan_named_argument_expr(index, finder),
                solar_ast::IndexKind::Range(start, end) => {
                    if let Some(start) = start {
                        scan_named_argument_expr(start, finder);
                    }
                    if let Some(end) = end {
                        scan_named_argument_expr(end, finder);
                    }
                }
                solar_ast::IndexKind::Index(None) => {}
            }
        }
        solar_ast::ExprKind::Member(expression, _) => scan_named_argument_expr(expression, finder),
        solar_ast::ExprKind::Payable(arguments) => {
            for argument in arguments.exprs() {
                scan_named_argument_expr(argument, finder);
            }
        }
        solar_ast::ExprKind::Ternary(condition, if_true, if_false) => {
            scan_named_argument_expr(condition, finder);
            scan_named_argument_expr(if_true, finder);
            scan_named_argument_expr(if_false, finder);
        }
        solar_ast::ExprKind::Tuple(expressions) => {
            for expression in expressions.iter() {
                if let SpannedOption::Some(expression) = expression {
                    scan_named_argument_expr(expression, finder);
                }
            }
        }
        solar_ast::ExprKind::Ident(_)
        | solar_ast::ExprKind::Lit(_, _)
        | solar_ast::ExprKind::New(_)
        | solar_ast::ExprKind::TypeCall(_)
        | solar_ast::ExprKind::Type(_) => {}
    }
}

fn callable_target_at_offset(
    snapshot: &ProjectSnapshot,
    offset: usize,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<ReferenceTarget> {
    if let Some((container, _member, member_range)) =
        symbols::find_member_access_at_offset(&snapshot.source, offset)
    {
        let member_name = &snapshot.source[member_range.clone()];
        let pseudo_receiver_access = match container.as_str() {
            "this" => Some(InheritedMemberAccess::This),
            "super" => Some(InheritedMemberAccess::Super),
            _ => None,
        };
        if let Some(access) = pseudo_receiver_access {
            let targets = resolve_effective_member_targets(
                snapshot,
                offset,
                member_name,
                access,
                get_source,
                resolver,
            )
            .into_iter()
            .filter(|target| is_call_hierarchy_kind(target.def.kind))
            .collect::<Vec<_>>();
            let [target] = targets.as_slice() else {
                return None;
            };
            return Some(reference_target_from_def(
                &target.snapshot.path,
                &target.def,
                Some(container),
            ));
        }

        if let Some(container_def) = snapshot.table.resolve(&container, offset) {
            let defs = snapshot
                .table
                .resolve_member_all(container_def, member_name)
                .into_iter()
                .filter(|def| is_call_hierarchy_kind(def.kind))
                .collect::<Vec<_>>();
            if defs.len() == 1 {
                return Some(reference_target_from_def(
                    &snapshot.path,
                    defs[0],
                    Some(container),
                ));
            }
            if !defs.is_empty() {
                return None;
            }
        }

        if let Some(cross) = resolve_cross_file_member_symbol(
            &snapshot.table,
            &container,
            member_name,
            &snapshot.path,
            get_source,
            resolver,
        ) {
            if is_call_hierarchy_kind(cross.def.kind) {
                return Some(reference_target_from_def(
                    &cross.resolved_path,
                    &cross.def,
                    Some(container),
                ));
            }
        }

        return None;
    }

    let (name, ident_range) = symbols::find_ident_at_offset(&snapshot.source, offset)?;
    if let Some(definition) = snapshot.table.definition_at_name_span(&ident_range) {
        return is_call_hierarchy_kind(definition.kind)
            .then(|| reference_target_from_def(&snapshot.path, definition, None));
    }
    let defs = snapshot
        .table
        .resolve_all(&name, offset)
        .into_iter()
        .filter(|def| is_call_hierarchy_kind(def.kind))
        .collect::<Vec<_>>();
    if defs.len() == 1 {
        return Some(reference_target_from_def(&snapshot.path, defs[0], None));
    }
    if !defs.is_empty() {
        return None;
    }

    let inherited = resolve_effective_member_targets(
        snapshot,
        offset,
        &name,
        InheritedMemberAccess::Unqualified,
        get_source,
        resolver,
    )
    .into_iter()
    .filter(|target| is_call_hierarchy_kind(target.def.kind))
    .collect::<Vec<_>>();
    if let [target] = inherited.as_slice() {
        return Some(reference_target_from_def(
            &target.snapshot.path,
            &target.def,
            None,
        ));
    }
    if !inherited.is_empty() {
        return None;
    }

    if let Some(target) =
        import_clause_reference_target(snapshot, &name, &ident_range, get_source, resolver)
    {
        if is_call_hierarchy_kind(target.kind) {
            return Some(target);
        }
    }

    if let Some(cross) =
        resolve_cross_file_symbol(&snapshot.table, &name, &snapshot.path, get_source, resolver)
    {
        if is_call_hierarchy_kind(cross.def.kind) {
            return Some(reference_target_from_def(
                &cross.resolved_path,
                &cross.def,
                None,
            ));
        }
    }

    None
}

fn import_clause_reference_target(
    snapshot: &ProjectSnapshot,
    name: &str,
    ident_range: &ByteRange<usize>,
    get_source: &dyn Fn(&Path) -> Option<String>,
    resolver: &ImportResolver,
) -> Option<ReferenceTarget> {
    for import in &snapshot.table.imports {
        if !import_clause_contains_ident(&snapshot.source, import, ident_range) {
            continue;
        }

        let ImportedSymbols::Named(names) = &import.symbols else {
            continue;
        };

        if !names.iter().any(|(original, _alias)| original == name) {
            continue;
        }

        let resolved = resolver.resolve(&import.path, &snapshot.path)?;
        let resolved = normalize_path(&resolved);
        let imported_source = get_source(&resolved)?;
        let filename = resolved.to_string_lossy().to_string();
        let imported_table = symbols::build_symbol_table(&imported_source, &filename)?;
        let definitions = imported_table.resolve_all(name, 0);
        if definitions.len() == 1 {
            return Some(reference_target_from_def(&resolved, definitions[0], None));
        }
        if !definitions.is_empty() {
            return None;
        }

        let imported =
            resolve_cross_file_symbol(&imported_table, name, &resolved, get_source, resolver)?;
        return Some(reference_target_from_def(
            &imported.resolved_path,
            &imported.def,
            None,
        ));
    }

    None
}

fn import_clause_contains_ident(
    source: &str,
    import: &symbols::ImportInfo,
    ident_range: &ByteRange<usize>,
) -> bool {
    if ident_range.end > import.path_span.start {
        return false;
    }

    let prefix = &source[..import.path_span.start];
    let Some(import_start) = prefix.rfind("import") else {
        return false;
    };

    if prefix[import_start..].contains(';') {
        return false;
    }

    ident_range.start >= import_start && ident_range.end <= import.path_span.start
}

fn reference_target_from_def(
    file_path: &Path,
    def: &SymbolDef,
    container_name: Option<String>,
) -> ReferenceTarget {
    ReferenceTarget {
        file_path: normalize_path(file_path),
        name: def.name.clone(),
        kind: def.kind,
        name_span: def.name_span.clone(),
        def_span: def.def_span.clone(),
        container_name,
    }
}

fn document_symbol_from_def(snapshot: &ProjectSnapshot, def: &SymbolDef) -> DocumentSymbol {
    let detail = def
        .signature
        .as_ref()
        .map(|signature| signature.label.clone())
        .or_else(|| def.type_info.as_ref().map(|ty| ty.display().to_string()));

    let children = def.scope.map(|scope_id| {
        snapshot
            .table
            .scope_symbols(scope_id)
            .iter()
            .filter(|child| is_document_symbol(child.kind))
            .map(|child| document_symbol_from_def(snapshot, child))
            .collect::<Vec<_>>()
    });
    let children = children.filter(|children| !children.is_empty());

    #[allow(deprecated)]
    DocumentSymbol {
        name: def.name.clone(),
        detail,
        kind: symbol_kind_to_lsp(def.kind),
        tags: None,
        deprecated: None,
        range: span_to_range(&snapshot.source, &def.def_span),
        selection_range: span_to_range(&snapshot.source, &def.name_span),
        children,
    }
}

fn symbol_information(entry: WorkspaceSymbolEntry, uri: Uri, source: &str) -> SymbolInformation {
    #[allow(deprecated)]
    SymbolInformation {
        name: entry.name,
        kind: symbol_kind_to_lsp(entry.kind),
        tags: None,
        deprecated: None,
        location: Location {
            uri,
            range: span_to_range(source, &entry.name_span),
        },
        container_name: entry.container_name,
    }
}

fn discover_solidity_files(root: &Path) -> Vec<PathBuf> {
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            if entry
                .file_type()
                .is_some_and(|file_type| file_type.is_dir())
            {
                return !matches!(
                    name.as_ref(),
                    "node_modules" | "out" | "artifacts" | "cache" | "typechain-types"
                );
            }
            true
        })
        .build();

    walker
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("sol") {
                return None;
            }
            Some(normalize_path(path))
        })
        .collect()
}

fn workspace_root_for_file(path: &Path, fallback_root: Option<&Path>) -> Option<PathBuf> {
    let search_root = path.parent().unwrap_or(path);
    solgrid_config::find_workspace_root(search_root).or_else(|| {
        fallback_root
            .filter(|root| search_root.starts_with(root))
            .map(normalize_path)
    })
}

fn collect_contract_declarations(source: &str, filename: &str) -> Vec<ContractDecl> {
    with_parsed_ast_sequential(source, filename, |source_unit| {
        source_unit
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Contract(contract) => {
                    let kind = match contract.kind {
                        solar_ast::ContractKind::Interface => SymbolKind::Interface,
                        solar_ast::ContractKind::Library => SymbolKind::Library,
                        solar_ast::ContractKind::Contract
                        | solar_ast::ContractKind::AbstractContract => SymbolKind::Contract,
                    };
                    Some(ContractDecl {
                        name: contract.name.as_str().to_string(),
                        kind,
                        name_span: solgrid_ast::span_to_range(contract.name.span),
                        bases: contract
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
                            .collect(),
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default()
}

fn find_contract_decl<'a>(
    snapshot: &'a ProjectSnapshot,
    def: &SymbolDef,
) -> Option<&'a ContractDecl> {
    snapshot.contracts.iter().find(|contract| {
        contract.name_span == def.name_span
            && matches!(
                def.kind,
                SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
            )
    })
}

fn find_contract_symbol_def<'a>(
    snapshot: &'a ProjectSnapshot,
    contract: &ContractDecl,
) -> Option<&'a SymbolDef> {
    snapshot.table.file_level_symbols().iter().find(|def| {
        def.name == contract.name
            && def.name_span == contract.name_span
            && matches!(
                def.kind,
                SymbolKind::Contract | SymbolKind::Interface | SymbolKind::Library
            )
    })
}

fn inherited_member_key(def: &SymbolDef) -> Option<InheritedMemberKey> {
    match def.kind {
        SymbolKind::Function => Some(InheritedMemberKey::Function {
            name: def.name.clone(),
            signature: callable_signature_identity(def)?,
        }),
        SymbolKind::Modifier => Some(InheritedMemberKey::Modifier {
            name: def.name.clone(),
            signature: callable_signature_identity(def)?,
        }),
        SymbolKind::StateVariable => {
            if let Some(signature) = public_state_getter_signature(def) {
                Some(InheritedMemberKey::Function {
                    name: def.name.clone(),
                    signature,
                })
            } else {
                Some(InheritedMemberKey::StateVariable {
                    name: def.name.clone(),
                    ty: def.type_info.as_ref().map(|ty| ty.display().to_string()),
                })
            }
        }
        _ => None,
    }
}

fn inherited_surface_key(def: &SymbolDef) -> Option<InheritedSurfaceKey> {
    match def.kind {
        SymbolKind::Function => Some(InheritedSurfaceKey::Function {
            name: def.name.clone(),
            signature: callable_signature_identity(def)?,
        }),
        SymbolKind::Modifier => Some(InheritedSurfaceKey::Modifier {
            name: def.name.clone(),
            signature: callable_signature_identity(def)?,
        }),
        SymbolKind::StateVariable => {
            if let Some(signature) = public_state_getter_signature(def) {
                Some(InheritedSurfaceKey::Function {
                    name: def.name.clone(),
                    signature,
                })
            } else {
                Some(InheritedSurfaceKey::StateVariable {
                    name: def.name.clone(),
                    ty: def.type_info.as_ref().map(|ty| ty.display().to_string()),
                })
            }
        }
        SymbolKind::Event => Some(InheritedSurfaceKey::Event {
            name: def.name.clone(),
        }),
        SymbolKind::Error => Some(InheritedSurfaceKey::Error {
            name: def.name.clone(),
        }),
        SymbolKind::Struct => Some(InheritedSurfaceKey::Struct {
            name: def.name.clone(),
        }),
        SymbolKind::Enum => Some(InheritedSurfaceKey::Enum {
            name: def.name.clone(),
        }),
        SymbolKind::Udvt => Some(InheritedSurfaceKey::Udvt {
            name: def.name.clone(),
        }),
        _ => None,
    }
}

fn member_is_inheritable(def: &SymbolDef) -> bool {
    match def.kind {
        SymbolKind::Function | SymbolKind::Modifier | SymbolKind::StateVariable => {
            def.visibility != Some(Visibility::Private)
        }
        SymbolKind::Event | SymbolKind::Error | SymbolKind::Struct | SymbolKind::Enum => true,
        SymbolKind::Udvt => true,
        _ => false,
    }
}

fn inherited_surface_kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Modifier => "modifier",
        SymbolKind::StateVariable => "state variable",
        SymbolKind::Event => "event",
        SymbolKind::Error => "error",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::Udvt => "type",
        _ => "member",
    }
}

fn callable_signature_identity(def: &SymbolDef) -> Option<String> {
    let signature = def.signature.as_ref()?;
    let parameters = signature
        .parameters
        .iter()
        .map(|param| parameter_type_identity(&param.label))
        .collect::<Vec<_>>();
    Some(format!("{}({})", def.name, parameters.join(",")))
}

fn public_state_getter_signature(def: &SymbolDef) -> Option<String> {
    if def.kind != SymbolKind::StateVariable || def.visibility != Some(Visibility::Public) {
        return None;
    }
    let mut parameters = Vec::new();
    collect_public_getter_parameters(def.type_info.as_ref()?, &mut parameters);
    Some(format!("{}({})", def.name, parameters.join(",")))
}

fn collect_public_getter_parameters(ty: &symbols::TypeSpec, parameters: &mut Vec<String>) {
    match ty {
        symbols::TypeSpec::Mapping { key, value, .. } => {
            parameters.push(type_spec_signature_identity(key));
            collect_public_getter_parameters(value, parameters);
        }
        symbols::TypeSpec::Array { element, .. } => {
            parameters.push("uint256".to_string());
            collect_public_getter_parameters(element, parameters);
        }
        _ => {}
    }
}

fn type_spec_signature_identity(ty: &symbols::TypeSpec) -> String {
    match ty {
        symbols::TypeSpec::Custom { path, .. } => path.as_display(),
        _ => parameter_type_identity(ty.display()),
    }
}

fn parameter_type_identity(label: &str) -> String {
    let trimmed = label.trim();
    let type_text = match trimmed.rsplit_once(char::is_whitespace) {
        Some((prefix, last)) if is_signature_identifier(last) && !is_signature_modifier(last) => {
            prefix.trim_end()
        }
        _ => trimmed,
    };
    let normalized = type_text
        .split_whitespace()
        .filter(|part| !matches!(*part, "memory" | "storage" | "calldata"))
        .collect::<Vec<_>>()
        .join(" ");
    canonicalize_elementary_signature_alias(&normalized)
}

fn canonicalize_elementary_signature_alias(value: &str) -> String {
    for (short, canonical) in [
        ("uint", "uint256"),
        ("int", "int256"),
        ("fixed", "fixed128x18"),
        ("ufixed", "ufixed128x18"),
    ] {
        if value == short {
            return canonical.to_string();
        }
        if let Some(suffix) = value.strip_prefix(short) {
            if suffix.starts_with('[') {
                return format!("{canonical}{suffix}");
            }
        }
    }
    value.to_string()
}

fn is_signature_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first == '$' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

fn is_signature_modifier(value: &str) -> bool {
    matches!(
        value,
        "memory" | "storage" | "calldata" | "payable" | "indexed" | "virtual" | "override"
    )
}

fn declaration_hint_offset(source: &str, span: &ByteRange<usize>) -> usize {
    let bytes = source.as_bytes();
    let start = span.start.min(bytes.len());
    let end = span.end.min(bytes.len());
    let mut paren_depth = 0usize;
    let mut idx = start;

    while idx < end {
        match bytes[idx] {
            b'(' => paren_depth += 1,
            b')' => paren_depth = paren_depth.saturating_sub(1),
            b'{' | b';' if paren_depth == 0 => {
                let mut offset = idx;
                while offset > start && bytes[offset - 1].is_ascii_whitespace() {
                    offset -= 1;
                }
                return offset;
            }
            _ => {}
        }
        idx += 1;
    }

    end
}

fn inheritance_hint_from_origins(
    member: &SymbolDef,
    origins: Vec<ResolvedContract>,
    offset: usize,
) -> InheritanceHint {
    let mut overrides = Vec::new();
    let mut implements = Vec::new();
    for origin in &origins {
        if origin.decl.kind == SymbolKind::Interface {
            implements.push(origin.decl.name.clone());
        } else {
            overrides.push(origin.decl.name.clone());
        }
    }

    let mut label_parts = Vec::new();
    if !overrides.is_empty() {
        label_parts.push(format!("overrides {}", overrides.join(", ")));
    }
    if !implements.is_empty() {
        label_parts.push(format!("implements {}", implements.join(", ")));
    }

    let origin_details = origins
        .iter()
        .map(|origin| format!("{}.{}", origin.decl.name, member.name))
        .collect::<Vec<_>>();
    let kind_label = match member.kind {
        SymbolKind::Function => "function",
        SymbolKind::Modifier => "modifier",
        SymbolKind::StateVariable => "state variable",
        _ => "member",
    };

    InheritanceHint {
        offset,
        label: label_parts.join("; "),
        tooltip: format!(
            "Nearest inherited {kind_label} origin(s) for `{}`: {}",
            member.name,
            origin_details.join(", ")
        ),
    }
}

fn file_graph_node(path: PathBuf, workspace_root: Option<&Path>) -> GraphNode {
    GraphNode {
        id: file_graph_node_id(&path),
        label: display_path(&path, workspace_root),
        detail: "Solidity file".to_string(),
        kind: Some(GraphNodeKind::File),
        uri: path_to_uri(&path).map(|uri| uri.to_string()),
    }
}

fn contract_graph_node(
    path: &Path,
    decl: &ContractDecl,
    workspace_root: Option<&Path>,
) -> GraphNode {
    GraphNode {
        id: contract_graph_node_id(path, &decl.name_span),
        label: decl.name.clone(),
        detail: format!(
            "{} in {}",
            contract_kind_label(decl.kind),
            display_path(path, workspace_root)
        ),
        kind: Some(GraphNodeKind::Contract),
        uri: path_to_uri(path).map(|uri| uri.to_string()),
    }
}

fn linearized_contract_graph_node(
    path: &Path,
    decl: &ContractDecl,
    workspace_root: Option<&Path>,
    order: usize,
) -> GraphNode {
    GraphNode {
        id: contract_graph_node_id(path, &decl.name_span),
        label: decl.name.clone(),
        detail: format!(
            "#{} {} in {}",
            order + 1,
            contract_kind_label(decl.kind),
            display_path(path, workspace_root)
        ),
        kind: Some(GraphNodeKind::Contract),
        uri: path_to_uri(path).map(|uri| uri.to_string()),
    }
}

fn file_graph_node_id(path: &Path) -> String {
    normalize_path(path).to_string_lossy().to_string()
}

fn contract_graph_node_id(path: &Path, name_span: &ByteRange<usize>) -> String {
    format!("{}#{}", file_graph_node_id(path), name_span.start)
}

fn display_path(path: &Path, workspace_root: Option<&Path>) -> String {
    if let Some(root) = workspace_root {
        if let Ok(relative) = normalize_path(path).strip_prefix(root) {
            return relative.display().to_string();
        }
    }
    path.file_name()
        .map(|file_name| file_name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn contract_kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Interface => "Interface",
        SymbolKind::Library => "Library",
        _ => "Contract",
    }
}

fn resolved_contract_key(contract: &ResolvedContract) -> (PathBuf, usize) {
    (contract.path.clone(), contract.decl.name_span.start)
}

fn merge_linearized_contracts(
    mut sequences: Vec<Vec<ResolvedContract>>,
) -> Option<Vec<ResolvedContract>> {
    let mut result = Vec::new();

    loop {
        sequences.retain(|sequence| !sequence.is_empty());
        if sequences.is_empty() {
            return Some(result);
        }

        let mut selected = None;
        for sequence in &sequences {
            let candidate = sequence.first()?.clone();
            let candidate_key = resolved_contract_key(&candidate);
            let blocked = sequences.iter().any(|other| {
                other
                    .iter()
                    .skip(1)
                    .any(|item| resolved_contract_key(item) == candidate_key)
            });
            if !blocked {
                selected = Some(candidate);
                break;
            }
        }

        let selected = selected?;
        let selected_key = resolved_contract_key(&selected);
        result.push(selected);

        for sequence in &mut sequences {
            sequence.retain(|item| resolved_contract_key(item) != selected_key);
        }
    }
}

fn is_exportable(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Contract
            | SymbolKind::Interface
            | SymbolKind::Library
            | SymbolKind::Function
            | SymbolKind::Error
            | SymbolKind::Event
            | SymbolKind::Struct
            | SymbolKind::Enum
            | SymbolKind::Udvt
    )
}

fn is_document_symbol(kind: SymbolKind) -> bool {
    !matches!(
        kind,
        SymbolKind::LocalVariable | SymbolKind::Parameter | SymbolKind::ReturnParameter
    )
}

fn is_code_lens_member(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::Modifier
            | SymbolKind::Event
            | SymbolKind::Error
            | SymbolKind::Struct
            | SymbolKind::Enum
            | SymbolKind::Udvt
            | SymbolKind::StateVariable
    )
}

fn symbol_kind_to_lsp(kind: SymbolKind) -> LspSymbolKind {
    match kind {
        SymbolKind::Contract => LspSymbolKind::CLASS,
        SymbolKind::Interface => LspSymbolKind::INTERFACE,
        SymbolKind::Library => LspSymbolKind::MODULE,
        SymbolKind::Constructor => LspSymbolKind::CONSTRUCTOR,
        SymbolKind::Function => LspSymbolKind::FUNCTION,
        SymbolKind::Modifier => LspSymbolKind::METHOD,
        SymbolKind::Event => LspSymbolKind::EVENT,
        SymbolKind::Error => LspSymbolKind::OBJECT,
        SymbolKind::Struct => LspSymbolKind::STRUCT,
        SymbolKind::StructField => LspSymbolKind::FIELD,
        SymbolKind::Enum => LspSymbolKind::ENUM,
        SymbolKind::Udvt => LspSymbolKind::TYPE_PARAMETER,
        SymbolKind::StateVariable
        | SymbolKind::LocalVariable
        | SymbolKind::Parameter
        | SymbolKind::ReturnParameter => LspSymbolKind::VARIABLE,
        SymbolKind::EnumVariant => LspSymbolKind::ENUM_MEMBER,
    }
}

fn reference_count_title(count: usize) -> String {
    match count {
        1 => "1 reference".to_string(),
        _ => format!("{count} references"),
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn path_to_uri(path: &Path) -> Option<Uri> {
    Uri::from_file_path(path)
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut character = 0u32;

    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }

    Position { line, character }
}

fn position_to_offset(source: &str, position: Position) -> usize {
    let mut current_line = 0u32;
    let mut current_character = 0u32;

    for (index, ch) in source.char_indices() {
        if current_line == position.line && current_character == position.character {
            return index;
        }
        if ch == '\n' {
            if current_line == position.line {
                return index;
            }
            current_line += 1;
            current_character = 0;
        } else {
            current_character += ch.len_utf16() as u32;
        }
    }

    source.len()
}

fn span_to_range(source: &str, span: &ByteRange<usize>) -> Range {
    Range {
        start: offset_to_position(source, span.start),
        end: offset_to_position(source, span.end),
    }
}

fn find_identifier_occurrences(source: &str, needle: &str) -> Vec<ByteRange<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }

    find_all_identifier_occurrences(source)
        .into_iter()
        .filter(|span| &source[span.clone()] == needle)
        .collect()
}

fn find_all_identifier_occurrences(source: &str) -> Vec<ByteRange<usize>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Normal,
        LineComment,
        BlockComment,
        SingleQuotedString,
        DoubleQuotedString,
    }

    let bytes = source.as_bytes();
    let mut state = State::Normal;
    let mut index = 0usize;
    let mut matches = Vec::new();

    while index < bytes.len() {
        match state {
            State::Normal => {
                if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/') {
                    state = State::LineComment;
                    index += 2;
                    continue;
                }
                if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                    state = State::BlockComment;
                    index += 2;
                    continue;
                }
                if bytes[index] == b'"' {
                    state = State::DoubleQuotedString;
                    index += 1;
                    continue;
                }
                if bytes[index] == b'\'' {
                    state = State::SingleQuotedString;
                    index += 1;
                    continue;
                }

                if is_identifier_start(bytes[index]) {
                    let start = index;
                    index += 1;
                    while index < bytes.len() && is_identifier_continue(bytes[index]) {
                        index += 1;
                    }
                    matches.push(start..index);
                    continue;
                }

                index += 1;
            }
            State::LineComment => {
                if bytes[index] == b'\n' {
                    state = State::Normal;
                }
                index += 1;
            }
            State::BlockComment => {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    state = State::Normal;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            State::SingleQuotedString => {
                if bytes[index] == b'\\' {
                    index += 2;
                } else if bytes[index] == b'\'' {
                    state = State::Normal;
                    index += 1;
                } else {
                    index += 1;
                }
            }
            State::DoubleQuotedString => {
                if bytes[index] == b'\\' {
                    index += 2;
                } else if bytes[index] == b'"' {
                    state = State::Normal;
                    index += 1;
                } else {
                    index += 1;
                }
            }
        }
    }

    matches
}

fn find_natspec_identifier_occurrences(source: &str, needle: &str) -> Vec<ByteRange<usize>> {
    let mut matches = Vec::new();
    for tag in ["@param", "@return", "@inheritdoc", "@link"] {
        for (tag_start, _) in source.match_indices(tag) {
            if !is_natspec_comment_offset(source, tag_start) {
                continue;
            }

            let mut cursor = tag_start + tag.len();
            while source
                .as_bytes()
                .get(cursor)
                .is_some_and(u8::is_ascii_whitespace)
            {
                cursor += 1;
            }

            loop {
                let Some(byte) = source.as_bytes().get(cursor).copied() else {
                    break;
                };
                if !is_identifier_start(byte) {
                    break;
                }
                let start = cursor;
                cursor += 1;
                while source
                    .as_bytes()
                    .get(cursor)
                    .is_some_and(|byte| is_identifier_continue(*byte))
                {
                    cursor += 1;
                }
                if &source[start..cursor] == needle {
                    matches.push(start..cursor);
                }

                if source.as_bytes().get(cursor) != Some(&b'.') {
                    break;
                }
                cursor += 1;
            }
        }
    }
    matches.sort_by_key(|span| span.start);
    matches.dedup();
    matches
}

fn is_natspec_comment_offset(source: &str, offset: usize) -> bool {
    let line_start = solgrid_ast::natspec::line_start(source, offset);
    if source[line_start..offset].trim_start().starts_with("///") {
        return true;
    }

    let prefix = &source[..offset];
    let Some(comment_start) = prefix.rfind("/*") else {
        return false;
    };
    if prefix[comment_start..].contains("*/") {
        return false;
    }
    source[comment_start..].starts_with("/**")
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Default)]
    struct CountingNavBackend {
        snapshots: Arc<AtomicUsize>,
    }

    impl NavBackend for CountingNavBackend {
        fn snapshot(&self, path: &Path, source: &str) -> Option<ProjectSnapshot> {
            self.snapshots.fetch_add(1, Ordering::Relaxed);
            SolarNavBackend.snapshot(path, source)
        }
    }

    #[test]
    fn test_build_indexes_exported_symbols() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Token.sol"),
            r#"pragma solidity ^0.8.0;

contract Token {}
interface IERC20 {}
error Unauthorized();
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let matches = index.symbols_matching("T");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "Token");

        let iface = index.symbols_matching("IERC20");
        assert_eq!(iface.len(), 1);
        assert_eq!(iface[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn test_update_and_remove_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test.sol");
        fs::write(&path, "pragma solidity ^0.8.0;\ncontract OldName {}\n").unwrap();

        let mut index = ProjectIndex::build(dir.path());
        assert_eq!(index.symbols_matching("OldName").len(), 1);

        index.update_file(&path, "pragma solidity ^0.8.0;\ncontract NewName {}\n");
        assert!(index.symbols_matching("OldName").is_empty());
        assert_eq!(index.symbols_matching("NewName").len(), 1);

        index.remove_file(&path);
        assert!(index.symbols_matching("NewName").is_empty());
    }

    #[test]
    fn test_import_graph_and_workspace_context() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("remappings.txt"), "@oz/=lib/oz/\n").unwrap();
        fs::create_dir_all(dir.path().join("lib/oz/token")).unwrap();
        fs::write(
            dir.path().join("lib/oz/token/ERC20.sol"),
            "pragma solidity ^0.8.0;\ncontract ERC20 {}\n",
        )
        .unwrap();
        let main = dir.path().join("src/Main.sol");
        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
import {ERC20} from "@oz/token/ERC20.sol";
contract Main is ERC20 {}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let file = index.file(&main).unwrap();
        assert_eq!(file.import_paths.len(), 1);
        assert_eq!(
            file.workspace_root
                .as_ref()
                .map(|root| normalize_path(root)),
            Some(normalize_path(dir.path()))
        );
        assert_eq!(index.remappings_for_file(&main).len(), 1);
    }

    #[test]
    fn test_refresh_workspace_state_reloads_new_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("One.sol"),
            "pragma solidity ^0.8.0;\ncontract One {}\n",
        )
        .unwrap();

        let mut index = ProjectIndex::build(dir.path());
        assert_eq!(index.symbols_matching("One").len(), 1);
        assert!(index.symbols_matching("Two").is_empty());

        fs::write(
            dir.path().join("Two.sol"),
            "pragma solidity ^0.8.0;\ncontract Two {}\n",
        )
        .unwrap();
        index.refresh_workspace_state();
        assert_eq!(index.symbols_matching("Two").len(), 1);
    }

    #[test]
    fn test_transitive_import_dependents_are_cycle_safe_and_exclude_target() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("A.sol");
        let b = dir.path().join("B.sol");
        let c = dir.path().join("C.sol");
        let unrelated = dir.path().join("Unrelated.sol");
        fs::write(
            &a,
            "pragma solidity ^0.8.0; import \"./B.sol\"; contract A {}\n",
        )
        .unwrap();
        fs::write(
            &b,
            "pragma solidity ^0.8.0; import \"./A.sol\"; contract B {}\n",
        )
        .unwrap();
        fs::write(
            &c,
            "pragma solidity ^0.8.0; import \"./A.sol\"; contract C {}\n",
        )
        .unwrap();
        fs::write(
            &unrelated,
            "pragma solidity ^0.8.0; contract Unrelated {}\n",
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let dependents = index.transitive_import_dependents(&a);

        assert_eq!(dependents, vec![normalize_path(&b), normalize_path(&c)]);
        assert!(!dependents.contains(&normalize_path(&a)));
        assert!(!dependents.contains(&normalize_path(&unrelated)));
    }

    #[test]
    fn test_find_references_same_file_lexical_symbol() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Token.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Token {
    function foo(uint256 amount) public pure returns (uint256) {
        uint256 doubled = amount + amount;
        return doubled;
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let position = offset_to_position(source, source.find("amount +").unwrap());
        let refs = index.find_references(&path, source, position, true, &|candidate| {
            fs::read_to_string(candidate).ok()
        });

        assert_eq!(refs.len(), 3);
    }

    #[test]
    fn test_find_references_uses_and_invalidates_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Token.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Token {
    function foo(uint256 amount) public pure returns (uint256) {
        return amount + amount;
    }
}
"#;
        fs::write(&path, source).unwrap();

        let mut index = ProjectIndex::build(dir.path());
        let position = offset_to_position(source, source.find("amount +").unwrap());
        let refs_without_decl =
            index.find_references(&path, source, position, false, &|candidate| {
                fs::read_to_string(candidate).ok()
            });
        assert_eq!(refs_without_decl.len(), 2);
        assert_eq!(index.reference_cache_entry_count(), 1);

        let refs_with_decl = index.find_references(&path, source, position, true, &|candidate| {
            fs::read_to_string(candidate).ok()
        });
        assert_eq!(refs_with_decl.len(), 3);
        assert_eq!(index.reference_cache_entry_count(), 1);

        index.update_file(
            &path,
            "pragma solidity ^0.8.0;\ncontract Token { function foo(uint256 count) public pure returns (uint256) { return count; } }\n",
        );
        assert_eq!(index.reference_cache_entry_count(), 0);
    }

    #[test]
    fn test_code_lenses_batch_reference_results_and_warm_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Token.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Token {
    function ping() public {}
    function invoke() external { ping(); ping(); }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let lenses = index.code_lenses(&path, source, &|candidate| {
            fs::read_to_string(candidate).ok()
        });
        let ping_line = offset_to_position(source, source.find("ping()").unwrap()).line;
        let ping_lens = lenses
            .iter()
            .find(|lens| lens.range.start.line == ping_line)
            .expect("ping code lens");

        assert_eq!(
            ping_lens
                .command
                .as_ref()
                .map(|command| command.title.as_str()),
            Some("2 references")
        );
        assert_eq!(index.reference_cache_entry_count(), lenses.len());
    }

    #[test]
    fn test_code_lenses_reuse_cached_reference_results() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Token.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Token {
    function ping() public {}
    function invoke() external { ping(); }
}
"#;
        fs::write(&path, source).unwrap();

        let backend = CountingNavBackend::default();
        let snapshots = backend.snapshots.clone();
        let mut index = ProjectIndex::with_backend(Some(dir.path().to_path_buf()), backend);
        index.update_file(&path, source);
        snapshots.store(0, Ordering::Relaxed);

        let first = index.code_lenses(&path, source, &|candidate| {
            fs::read_to_string(candidate).ok()
        });
        let after_first = snapshots.load(Ordering::Relaxed);
        let second = index.code_lenses(&path, source, &|candidate| {
            fs::read_to_string(candidate).ok()
        });
        let after_second = snapshots.load(Ordering::Relaxed);

        assert_eq!(first, second);
        assert_eq!(
            after_first, 2,
            "initial request parses the lens and scan snapshots"
        );
        assert_eq!(
            after_second,
            after_first + 1,
            "cached request should only parse the requested lens snapshot"
        );
    }

    #[test]
    fn test_candidate_snapshot_prefers_overlay_and_preserves_source_fallbacks() {
        let dir = tempfile::tempdir().unwrap();
        let indexed_path = dir.path().join("Indexed.sol");
        let indexed_source = "pragma solidity ^0.8.0; contract Indexed {}\n";
        fs::write(&indexed_path, indexed_source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let overlay_source = "pragma solidity ^0.8.0; contract Overlay {}\n";
        let overlay_snapshot = index
            .candidate_snapshot(&indexed_path, None, &|_| Some(overlay_source.to_string()))
            .expect("overlay snapshot");
        assert_eq!(overlay_snapshot.source, overlay_source);

        let indexed_snapshot = index
            .candidate_snapshot(&indexed_path, None, &|_| None)
            .expect("indexed fallback snapshot");
        assert_eq!(indexed_snapshot.source, indexed_source);

        let disk_path = dir.path().join("AddedAfterIndex.sol");
        let disk_source = "pragma solidity ^0.8.0; contract AddedAfterIndex {}\n";
        fs::write(&disk_path, disk_source).unwrap();
        let disk_snapshot = index
            .candidate_snapshot(&disk_path, None, &|candidate| {
                fs::read_to_string(candidate).ok()
            })
            .expect("disk fallback snapshot");
        assert_eq!(disk_snapshot.source, disk_source);
    }

    #[test]
    fn test_find_references_cross_file_named_import_and_alias() {
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join("Token.sol");
        fs::write(&token_path, "pragma solidity ^0.8.0;\ncontract Token {}\n").unwrap();

        let main_path = dir.path().join("Main.sol");
        let main_source = r#"pragma solidity ^0.8.0;
import {Token as T} from "./Token.sol";
contract Main is T {}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let token_source = fs::read_to_string(&token_path).unwrap();
        let position = offset_to_position(&token_source, token_source.find("Token").unwrap());
        let refs =
            index.find_references(&token_path, &token_source, position, true, &|candidate| {
                fs::read_to_string(candidate).ok()
            });

        assert_eq!(refs.len(), 4);
        assert!(refs
            .iter()
            .any(|location| location.uri == path_to_uri(&normalize_path(&main_path)).unwrap()));
    }

    #[test]
    fn test_inherited_calls_resolve_across_unqualified_this_and_super_access() {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("Base.sol");
        let base_source = r#"pragma solidity ^0.8.0;
contract Base {
    function inheritedCall() public virtual {}
}
"#;
        fs::write(&base_path, base_source).unwrap();

        let main_path = dir.path().join("Main.sol");
        let main_source = r#"pragma solidity ^0.8.0;
import {Base as Parent} from "./Base.sol";
contract Main is Parent {
    function invoke() external {
        inheritedCall();
        this.inheritedCall();
        super.inheritedCall();
    }
}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let get_source = |candidate: &Path| fs::read_to_string(candidate).ok();
        let declaration = base_source.find("inheritedCall").unwrap();
        let references = index.find_references(
            &base_path,
            base_source,
            offset_to_position(base_source, declaration),
            true,
            &get_source,
        );
        assert_eq!(references.len(), 4);
        assert_eq!(
            references
                .iter()
                .filter(|location| {
                    location.uri == path_to_uri(&normalize_path(&main_path)).unwrap()
                })
                .count(),
            3
        );

        let rename = index
            .rename_plan(
                &base_path,
                base_source,
                offset_to_position(base_source, declaration),
                &get_source,
            )
            .expect("inherited calls should be safely renameable");
        assert_eq!(rename.locations.len(), 4);

        let invoke_offset = main_source.find("invoke").unwrap();
        let outgoing =
            index.outgoing_call_hierarchy(&main_path, main_source, invoke_offset, &get_source);
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].to.path, normalize_path(&base_path));
        assert_eq!(outgoing[0].to.name, "inheritedCall");
        assert_eq!(outgoing[0].from_ranges.len(), 3);
    }

    #[test]
    fn test_this_and_super_calls_respect_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Base {
    function ping() public virtual {}
}
contract Main is Base {
    function ping() public override {}
    function invoke() external {
        ping();
        this.ping();
        super.ping();
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let get_source = |candidate: &Path| fs::read_to_string(candidate).ok();
        let base_declaration = source.find("ping() public virtual").unwrap();
        let derived_declaration = source.find("ping() public override").unwrap();

        let base_references = index.find_references(
            &path,
            source,
            offset_to_position(source, base_declaration),
            true,
            &get_source,
        );
        assert_eq!(base_references.len(), 2);
        let derived_references = index.find_references(
            &path,
            source,
            offset_to_position(source, derived_declaration),
            true,
            &get_source,
        );
        assert_eq!(derived_references.len(), 3);
    }

    #[test]
    fn test_this_call_resolves_public_state_getter_override() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
interface IOwned {
    function owner() external view returns (address);
}
contract Main is IOwned {
    address public override owner;
    function readOwner() external view { this.owner(); }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let declaration = source.find("owner;").unwrap();
        let references = index.find_references(
            &path,
            source,
            offset_to_position(source, declaration),
            true,
            &|candidate| fs::read_to_string(candidate).ok(),
        );
        assert_eq!(references.len(), 2);
    }

    #[test]
    fn test_transitive_reexport_alias_references_and_code_lens_count() {
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join("Token.sol");
        let token_source = "pragma solidity ^0.8.0;\ncontract Token {}\n";
        fs::write(&token_path, token_source).unwrap();

        let reexport_path = dir.path().join("Reexport.sol");
        fs::write(
            &reexport_path,
            "pragma solidity ^0.8.0;\nimport {Token} from \"./Token.sol\";\n",
        )
        .unwrap();

        let consumer_path = dir.path().join("Consumer.sol");
        fs::write(
            &consumer_path,
            "pragma solidity ^0.8.0;\nimport {Token as T} from \"./Reexport.sol\";\ncontract Consumer is T {}\n",
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let position = offset_to_position(token_source, token_source.find("Token").unwrap());
        let get_source = |candidate: &Path| fs::read_to_string(candidate).ok();
        let references =
            index.find_references(&token_path, token_source, position, true, &get_source);

        assert_eq!(references.len(), 5);
        for (path, expected) in [(&token_path, 1), (&reexport_path, 1), (&consumer_path, 3)] {
            let uri = path_to_uri(&normalize_path(path)).unwrap();
            assert_eq!(
                references
                    .iter()
                    .filter(|location| location.uri == uri)
                    .count(),
                expected,
                "unexpected reference count for {}",
                path.display()
            );
        }

        let code_lens_index = ProjectIndex::build(dir.path());
        let token_lens = code_lens_index
            .code_lenses(&token_path, token_source, &get_source)
            .into_iter()
            .find(|lens| lens.range.start.line == 1)
            .expect("Token code lens");
        assert_eq!(
            token_lens
                .command
                .as_ref()
                .map(|command| command.title.as_str()),
            Some("4 references")
        );
    }

    #[test]
    fn test_rename_plan_for_same_file_local_symbol() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function run(uint256 amount) external returns (uint256) {
        uint256 localAmount = amount + 1;
        return localAmount;
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let position = offset_to_position(source, source.find("localAmount").unwrap());
        let plan = index
            .rename_plan(&path, source, position, &|candidate| {
                fs::read_to_string(candidate).ok()
            })
            .unwrap();

        assert_eq!(plan.placeholder, "localAmount");
        assert_eq!(plan.locations.len(), 2);
        assert_eq!(plan.range.start.line, 3);
    }

    #[test]
    fn test_rename_plan_keeps_overload_declarations_and_ambiguous_calls_separate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function run(uint256 value) public {}
    function run(address value) public {}
    function invoke(uint256 value) external { run(value); }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let declaration = source.find("run(uint256").unwrap();
        let position = offset_to_position(source, declaration);
        let plan = index
            .rename_plan(&path, source, position, &|candidate| {
                fs::read_to_string(candidate).ok()
            })
            .unwrap();

        assert_eq!(plan.placeholder, "run");
        assert_eq!(plan.locations.len(), 1);
        assert_eq!(
            plan.locations[0].range,
            span_to_range(source, &(declaration..declaration + "run".len()))
        );
    }

    #[test]
    fn test_rename_plan_includes_semantically_resolved_named_argument_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function configure(uint256 amount, address recipient) internal {}
    function invoke() external {
        configure({recipient: address(0), amount: 1});
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let declaration = source.find("amount, address").unwrap();
        let plan = index
            .rename_plan(
                &path,
                source,
                offset_to_position(source, declaration),
                &|candidate| fs::read_to_string(candidate).ok(),
            )
            .unwrap();

        assert_eq!(plan.locations.len(), 2);
        assert!(plan.locations.iter().any(|location| {
            location.range
                == span_to_range(
                    source,
                    &(source.rfind("amount: 1").unwrap()
                        ..source.rfind("amount: 1").unwrap() + "amount".len()),
                )
        }));
    }

    #[test]
    fn test_rename_plan_resolves_named_argument_keys_through_typed_instances() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
interface Token {
    function transfer(address recipient, uint256 amount) external;
}
contract Main {
    function send(Token token) external {
        token.transfer({amount: 1, recipient: address(0)});
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let declaration = source.find("recipient, uint256").unwrap();
        let plan = index
            .rename_plan(
                &path,
                source,
                offset_to_position(source, declaration),
                &|candidate| fs::read_to_string(candidate).ok(),
            )
            .unwrap();

        assert_eq!(plan.locations.len(), 2);
        let named_key = source.rfind("recipient: address").unwrap();
        assert!(plan.locations.iter().any(|location| {
            location.range == span_to_range(source, &(named_key..named_key + "recipient".len()))
        }));
    }

    #[test]
    fn test_rename_plan_includes_struct_literal_field_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    struct Quote { address token; uint256 amount; }
    function build() external pure returns (Quote memory) {
        return Quote({amount: 1, token: address(0)});
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let declaration = source.find("amount; }").unwrap();
        let plan = index
            .rename_plan(
                &path,
                source,
                offset_to_position(source, declaration),
                &|candidate| fs::read_to_string(candidate).ok(),
            )
            .unwrap();

        assert_eq!(plan.locations.len(), 2);
        assert!(plan.locations.iter().any(|location| {
            location.range
                == span_to_range(
                    source,
                    &(source.rfind("amount: 1").unwrap()
                        ..source.rfind("amount: 1").unwrap() + "amount".len()),
                )
        }));
    }

    #[test]
    fn test_rename_plan_includes_named_emit_and_revert_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    event Paid(address payer, uint256 amount);
    error Failed(address account, uint256 code);

    function run() external {
        emit Paid({amount: 1, payer: msg.sender});
        revert Failed({code: 2, account: msg.sender});
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        for (declaration, named_key) in [
            ("payer, uint256", "payer: msg.sender"),
            ("account, uint256", "account: msg.sender"),
        ] {
            let declaration = source.find(declaration).unwrap();
            let plan = index
                .rename_plan(
                    &path,
                    source,
                    offset_to_position(source, declaration),
                    &|candidate| fs::read_to_string(candidate).ok(),
                )
                .unwrap();

            assert_eq!(plan.locations.len(), 2);
            let named_key = source.find(named_key).unwrap();
            assert!(plan.locations.iter().any(|location| {
                location.range
                    == span_to_range(source, &(named_key..named_key + plan.placeholder.len()))
            }));
        }
    }

    #[test]
    fn test_rename_plan_includes_imported_named_error_key() {
        let dir = tempfile::tempdir().unwrap();
        let errors_path = dir.path().join("Errors.sol");
        let errors_source = r#"pragma solidity ^0.8.0;
error Unauthorized(address account, uint256 code);
"#;
        fs::write(&errors_path, errors_source).unwrap();
        let main_path = dir.path().join("Main.sol");
        let main_source = r#"pragma solidity ^0.8.0;
import {Unauthorized} from "./Errors.sol";
contract Main {
    function run() external {
        revert Unauthorized({code: 1, account: msg.sender});
    }
}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let declaration = errors_source.find("account, uint256").unwrap();
        let plan = index
            .rename_plan(
                &errors_path,
                errors_source,
                offset_to_position(errors_source, declaration),
                &|candidate| fs::read_to_string(candidate).ok(),
            )
            .unwrap();

        assert_eq!(plan.locations.len(), 2);
        let named_key = main_source.find("account: msg.sender").unwrap();
        assert!(plan.locations.iter().any(|location| {
            location.uri == path_to_uri(&normalize_path(&main_path)).unwrap()
                && location.range
                    == span_to_range(main_source, &(named_key..named_key + "account".len()))
        }));
    }

    #[test]
    fn test_rename_plan_includes_imported_named_event_key() {
        let dir = tempfile::tempdir().unwrap();
        let events_path = dir.path().join("Events.sol");
        let events_source = r#"pragma solidity ^0.8.0;
contract Events {
    event Paid(address payer, uint256 amount);
}
"#;
        fs::write(&events_path, events_source).unwrap();
        let main_path = dir.path().join("Main.sol");
        let main_source = r#"pragma solidity ^0.8.0;
import {Events} from "./Events.sol";
contract Main {
    function run() external {
        emit Events.Paid({amount: 1, payer: msg.sender});
    }
}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let declaration = events_source.find("payer, uint256").unwrap();
        let plan = index
            .rename_plan(
                &events_path,
                events_source,
                offset_to_position(events_source, declaration),
                &|candidate| fs::read_to_string(candidate).ok(),
            )
            .unwrap();

        assert_eq!(plan.locations.len(), 2);
        let named_key = main_source.find("payer: msg.sender").unwrap();
        assert!(plan.locations.iter().any(|location| {
            location.uri == path_to_uri(&normalize_path(&main_path)).unwrap()
                && location.range
                    == span_to_range(main_source, &(named_key..named_key + "payer".len()))
        }));
    }

    #[test]
    fn test_rename_plan_includes_inherited_unqualified_event_and_error_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Base {
    event Paid(address payer, uint256 amount);
    error Failed(address account, uint256 code);
}
contract Main is Base {
    function run() external {
        emit Paid({amount: 1, payer: msg.sender});
        revert Failed({code: 2, account: msg.sender});
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        for (declaration, named_key) in [
            ("payer, uint256", "payer: msg.sender"),
            ("account, uint256", "account: msg.sender"),
        ] {
            let declaration = source.find(declaration).unwrap();
            let plan = index
                .rename_plan(
                    &path,
                    source,
                    offset_to_position(source, declaration),
                    &|candidate| fs::read_to_string(candidate).ok(),
                )
                .unwrap();

            assert_eq!(plan.locations.len(), 2);
            let named_key = source.find(named_key).unwrap();
            assert!(plan.locations.iter().any(|location| {
                location.range
                    == span_to_range(source, &(named_key..named_key + plan.placeholder.len()))
            }));
        }
    }

    #[test]
    fn test_rename_plan_includes_imported_inherited_event_and_error_keys() {
        let dir = tempfile::tempdir().unwrap();
        let base_path = dir.path().join("Base.sol");
        let base_source = r#"pragma solidity ^0.8.0;
contract Base {
    event Paid(address payer, uint256 amount);
    error Failed(address account, uint256 code);
}
"#;
        fs::write(&base_path, base_source).unwrap();
        let main_path = dir.path().join("Main.sol");
        let main_source = r#"pragma solidity ^0.8.0;
import {Base} from "./Base.sol";
contract Main is Base {
    function run() external {
        emit Paid({amount: 1, payer: msg.sender});
        revert Failed({code: 2, account: msg.sender});
    }
}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::build(dir.path());
        for (declaration, named_key) in [
            ("payer, uint256", "payer: msg.sender"),
            ("account, uint256", "account: msg.sender"),
        ] {
            let declaration = base_source.find(declaration).unwrap();
            let plan = index
                .rename_plan(
                    &base_path,
                    base_source,
                    offset_to_position(base_source, declaration),
                    &|candidate| fs::read_to_string(candidate).ok(),
                )
                .unwrap();

            assert_eq!(plan.locations.len(), 2);
            let named_key = main_source.find(named_key).unwrap();
            assert!(plan.locations.iter().any(|location| {
                location.uri == path_to_uri(&normalize_path(&main_path)).unwrap()
                    && location.range
                        == span_to_range(
                            main_source,
                            &(named_key..named_key + plan.placeholder.len()),
                        )
            }));
        }
    }

    #[test]
    fn test_rename_plan_includes_attached_natspec_parameter_references() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    /// @param amount Amount to return.
    function echo(uint256 amount) external pure returns (uint256) {
        return amount;
    }
}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let declaration = source.find("amount) external").unwrap();
        let plan = index
            .rename_plan(
                &path,
                source,
                offset_to_position(source, declaration),
                &|candidate| fs::read_to_string(candidate).ok(),
            )
            .unwrap();

        assert_eq!(plan.locations.len(), 3);
        let documented = source.find("amount Amount").unwrap();
        assert!(plan.locations.iter().any(|location| {
            location.range == span_to_range(source, &(documented..documented + "amount".len()))
        }));
    }

    #[test]
    fn test_rename_plan_includes_natspec_inheritdoc_references() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Base {}
/// @inheritdoc Base
contract Derived is Base {}
"#;
        fs::write(&path, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let declaration = source.find("Base {}").unwrap();
        let plan = index
            .rename_plan(
                &path,
                source,
                offset_to_position(source, declaration),
                &|candidate| fs::read_to_string(candidate).ok(),
            )
            .unwrap();

        assert_eq!(plan.locations.len(), 3);
        let documented = source.find("Base\ncontract").unwrap();
        assert!(plan.locations.iter().any(|location| {
            location.range == span_to_range(source, &(documented..documented + "Base".len()))
        }));
    }

    #[test]
    fn test_rename_plan_allows_cross_file_exported_symbol_with_alias_import() {
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join("Token.sol");
        fs::write(&token_path, "pragma solidity ^0.8.0;\ncontract Token {}\n").unwrap();

        let main_path = dir.path().join("Main.sol");
        let main_source = r#"pragma solidity ^0.8.0;
import {Token as T} from "./Token.sol";
contract Main is T {}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let token_source = fs::read_to_string(&token_path).unwrap();
        let position = offset_to_position(&token_source, token_source.find("Token").unwrap());
        let plan = index
            .rename_plan(&token_path, &token_source, position, &|candidate| {
                fs::read_to_string(candidate).ok()
            })
            .unwrap();
        assert_eq!(plan.placeholder, "Token");
        assert_eq!(plan.locations.len(), 2);
    }

    #[test]
    fn test_rename_plan_allows_cross_file_exported_symbol_without_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join("Token.sol");
        let token_source = "pragma solidity ^0.8.0;\ncontract Token {}\n";
        fs::write(&token_path, token_source).unwrap();

        let main_path = dir.path().join("Main.sol");
        let main_source = r#"pragma solidity ^0.8.0;
import {Token} from "./Token.sol";
contract Main {
    Token token;
}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let position = offset_to_position(token_source, token_source.find("Token").unwrap());
        let plan = index
            .rename_plan(&token_path, token_source, position, &|candidate| {
                fs::read_to_string(candidate).ok()
            })
            .unwrap();

        assert_eq!(plan.placeholder, "Token");
        assert_eq!(plan.locations.len(), 3);
        assert!(plan.locations.iter().any(|location| {
            location.uri == path_to_uri(&normalize_path(&token_path)).unwrap()
        }));
        assert!(plan
            .locations
            .iter()
            .any(|location| { location.uri == path_to_uri(&normalize_path(&main_path)).unwrap() }));
    }

    #[test]
    fn test_rename_plan_rejects_alias_usage_site() {
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join("Token.sol");
        fs::write(&token_path, "pragma solidity ^0.8.0;\ncontract Token {}\n").unwrap();

        let main_path = dir.path().join("Main.sol");
        let main_source = r#"pragma solidity ^0.8.0;
import {Token as T} from "./Token.sol";
contract Main is T {}
"#;
        fs::write(&main_path, main_source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let position = offset_to_position(main_source, main_source.rfind("T").unwrap());

        assert!(index
            .rename_plan(&main_path, main_source, position, &|candidate| {
                fs::read_to_string(candidate).ok()
            })
            .is_none());
    }

    #[test]
    fn test_document_links_resolve_import_targets() {
        let dir = tempfile::tempdir().unwrap();
        let dep = dir.path().join("Dep.sol");
        fs::write(&dep, "pragma solidity ^0.8.0;\ncontract Dep {}\n").unwrap();
        let main = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
import {Dep} from "./Dep.sol";
contract Main {}
"#;
        fs::write(&main, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let links = index.document_links(&main, source);
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].target,
            Some(path_to_uri(&normalize_path(&dep)).unwrap())
        );
    }

    #[test]
    fn test_find_identifier_occurrences_skips_comments_and_strings() {
        let source = r#"pragma solidity ^0.8.0;
// Token should not count
contract Token {
    string constant NAME = "Token";
}
"#;
        let spans = find_identifier_occurrences(source, "Token");
        assert_eq!(spans.len(), 1);
    }

    #[test]
    fn test_imports_graph_collects_transitive_imports() {
        let dir = tempfile::tempdir().unwrap();
        let dep = dir.path().join("Dep.sol");
        let leaf = dir.path().join("Leaf.sol");
        let main = dir.path().join("Main.sol");

        fs::write(&leaf, "pragma solidity ^0.8.0;\ncontract Leaf {}\n").unwrap();
        fs::write(
            &dep,
            "pragma solidity ^0.8.0;\nimport \"./Leaf.sol\";\ncontract Dep is Leaf {}\n",
        )
        .unwrap();
        fs::write(
            &main,
            "pragma solidity ^0.8.0;\nimport \"./Dep.sol\";\ncontract Main is Dep {}\n",
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let main_source = fs::read_to_string(&main).unwrap();
        let graph = index
            .imports_graph(&main, &main_source, &|path| fs::read_to_string(path).ok())
            .expect("imports graph");

        assert_eq!(graph.kind, GraphKind::Imports);
        assert_eq!(graph.nodes.len(), 3);
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("imports")));
    }

    #[test]
    fn test_inheritance_graph_collects_transitive_bases() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("Base.sol");
        let dep = dir.path().join("Dep.sol");
        let main = dir.path().join("Main.sol");

        fs::write(&base, "pragma solidity ^0.8.0;\ncontract Base {}\n").unwrap();
        fs::write(
            &dep,
            "pragma solidity ^0.8.0;\nimport \"./Base.sol\";\ncontract Dep is Base {}\n",
        )
        .unwrap();
        fs::write(
            &main,
            "pragma solidity ^0.8.0;\nimport \"./Dep.sol\";\ncontract Main is Dep {}\n",
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let main_source = fs::read_to_string(&main).unwrap();
        let graph = index
            .inheritance_graph(&main, &main_source, "Main", &|path| {
                fs::read_to_string(path).ok()
            })
            .expect("inheritance graph");

        assert_eq!(graph.kind, GraphKind::Inheritance);
        assert!(graph.nodes.iter().any(|node| node.label == "Main"));
        assert!(graph.nodes.iter().any(|node| node.label == "Dep"));
        assert!(graph.nodes.iter().any(|node| node.label == "Base"));
        assert_eq!(graph.edges.len(), 2);
    }

    #[test]
    fn test_linearized_inheritance_graph_orders_rightmost_base_first() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");

        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
contract Root {}
contract Left is Root {}
contract Right is Root {}
contract Main is Left, Right {}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let main_source = fs::read_to_string(&main).unwrap();
        let graph = index
            .linearized_inheritance_graph(&main, &main_source, "Main", &|path| {
                fs::read_to_string(path).ok()
            })
            .expect("linearized inheritance graph");

        assert_eq!(graph.kind, GraphKind::LinearizedInheritance);
        assert_eq!(
            graph
                .nodes
                .iter()
                .map(|node| node.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Main", "Right", "Left", "Root"]
        );
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(
            graph
                .edges
                .iter()
                .map(|edge| edge.label.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("precedes"), Some("precedes"), Some("precedes")]
        );
    }

    #[test]
    fn test_control_flow_graph_builds_branching_function_flow() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");

        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
contract Main {
    function run(uint256 amount) public returns (uint256) {
        if (amount == 0) {
            return 1;
        }

        return amount;
    }
}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let main_source = fs::read_to_string(&main).unwrap();
        let snapshot = index.snapshot_for_source(&main, &main_source).unwrap();
        eprintln!(
            "linearized={:#?}",
            linearized_contract_refs(&index, &snapshot, &|path| fs::read_to_string(path).ok())
        );
        let graph = index
            .control_flow_graph(
                &main,
                &main_source,
                main_source.find("run").unwrap(),
                &|path| fs::read_to_string(path).ok(),
            )
            .expect("control-flow graph");
        eprintln!(
            "nodes={:#?}",
            graph
                .nodes
                .iter()
                .map(|node| node.label.clone())
                .collect::<Vec<_>>()
        );

        assert_eq!(graph.kind, GraphKind::ControlFlow);
        assert!(graph.nodes.iter().any(|node| node.label == "Entry"));
        assert!(graph.nodes.iter().any(|node| node.label == "Exit"));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "if amount == 0"));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "Entry" && node.kind == Some(GraphNodeKind::Entry)));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "Exit" && node.kind == Some(GraphNodeKind::Exit)));
        assert!(
            graph
                .nodes
                .iter()
                .any(|node| node.label == "if amount == 0"
                    && node.kind == Some(GraphNodeKind::Branch))
        );
        assert!(graph.nodes.iter().any(|node| {
            node.label == "return" && node.kind == Some(GraphNodeKind::TerminalReturn)
        }));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("true")));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("return")));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == Some(GraphEdgeKind::BranchTrue)));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.kind == Some(GraphEdgeKind::Return)));
    }

    #[test]
    fn test_control_flow_graph_expands_same_file_modifiers() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");

        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
contract Main {
    modifier onlyPositive(uint256 amount) {
        require(amount > 0);
        _;
    }

    function run(uint256 amount) public onlyPositive(amount) returns (uint256) {
        return amount;
    }
}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let main_source = fs::read_to_string(&main).unwrap();
        let graph = index
            .control_flow_graph(
                &main,
                &main_source,
                main_source.find("run").unwrap(),
                &|path| fs::read_to_string(path).ok(),
            )
            .expect("control-flow graph");

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "modifier onlyPositive(amount)"));
        assert!(graph.nodes.iter().any(|node| {
            node.label == "modifier onlyPositive(amount)"
                && node.kind == Some(GraphNodeKind::Modifier)
        }));
        assert!(graph.nodes.iter().any(|node| node.label == "call require"));
        assert!(graph.nodes.iter().any(|node| {
            node.label == "call require" && node.kind == Some(GraphNodeKind::Call)
        }));
        assert!(!graph.nodes.iter().any(|node| node.label == "_"));
    }

    #[test]
    fn test_control_flow_graph_routes_returns_through_modifier_postludes() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    bool cleaned;

    modifier cleanup() {
        _;
        cleaned = true;
    }

    function run() public cleanup returns (uint256) {
        return 1;
    }
}
"#;
        fs::write(&main, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let graph = index
            .control_flow_graph(&main, source, source.find("run").unwrap(), &|path| {
                fs::read_to_string(path).ok()
            })
            .expect("control-flow graph");
        let return_id = graph
            .nodes
            .iter()
            .find(|node| node.label == "return")
            .map(|node| node.id.as_str())
            .expect("return node");
        let cleanup_id = graph
            .nodes
            .iter()
            .find(|node| node.label == "assign cleaned")
            .map(|node| node.id.as_str())
            .expect("modifier postlude node");
        let exit_id = graph
            .nodes
            .iter()
            .find(|node| node.kind == Some(GraphNodeKind::Exit))
            .map(|node| node.id.as_str())
            .expect("exit node");

        assert!(graph.edges.iter().any(|edge| {
            edge.from == return_id
                && edge.to == cleanup_id
                && edge.kind == Some(GraphEdgeKind::Return)
        }));
        assert!(!graph
            .edges
            .iter()
            .any(|edge| edge.from == return_id && edge.to == exit_id));
    }

    #[test]
    fn test_graph_lenses_preserve_function_pointer_type_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract Main {
    function run(function(uint) internal returns (uint) callback) internal {}
    function run(function(address) internal returns (address) callback) internal {}
}
"#;
        fs::write(&main, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let symbols = index
            .graph_lenses(&main, source)
            .into_iter()
            .filter(|lens| lens.kind == GraphKind::ControlFlow)
            .filter_map(|lens| lens.symbol_name)
            .collect::<HashSet<_>>();

        assert!(symbols.contains("Main.run(function(uint256) internal returns(uint256))"));
        assert!(symbols.contains("Main.run(function(address) internal returns(address))"));
        assert_eq!(symbols.len(), 2);
    }

    #[test]
    fn test_control_flow_graph_expands_cross_file_inherited_modifiers() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("Base.sol");
        let main = dir.path().join("Main.sol");

        fs::write(
            &base,
            r#"pragma solidity ^0.8.0;
contract Base {
    modifier onlyPositive(uint256 amount) {
        require(amount > 0);
        _;
    }
}
"#,
        )
        .unwrap();
        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
import "./Base.sol";

contract Main is Base {
    function run(uint256 amount) public onlyPositive(amount) returns (uint256) {
        return amount;
    }
}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let main_source = fs::read_to_string(&main).unwrap();
        let graph = index
            .control_flow_graph(
                &main,
                &main_source,
                main_source.find("run").unwrap(),
                &|path| fs::read_to_string(path).ok(),
            )
            .expect("control-flow graph");

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "modifier onlyPositive(amount)"));
        assert!(graph.nodes.iter().any(|node| {
            node.label == "modifier onlyPositive(amount)"
                && node.kind == Some(GraphNodeKind::Modifier)
        }));
        assert!(graph.nodes.iter().any(|node| node.label == "call require"));
        assert!(graph.nodes.iter().any(|node| {
            node.label == "call require" && node.kind == Some(GraphNodeKind::Call)
        }));
        assert!(!graph.nodes.iter().any(|node| node.label == "_"));
    }

    #[test]
    fn test_control_flow_graph_resolves_qualified_modifier_contract() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
contract BaseA {
    function baseAOnly() internal {}
    modifier onlyOwner() {
        baseAOnly();
        _;
    }
}
contract BaseB {
    function baseBOnly() internal {}
    modifier onlyOwner() {
        baseBOnly();
        _;
    }
}
contract Main is BaseA, BaseB {
    function run() public BaseA.onlyOwner {}
}
"#;
        fs::write(&main, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let graph = index
            .control_flow_graph(&main, source, source.find("run").unwrap(), &|path| {
                fs::read_to_string(path).ok()
            })
            .expect("control-flow graph");

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "call baseAOnly"));
        assert!(!graph
            .nodes
            .iter()
            .any(|node| node.label == "call baseBOnly"));
    }

    #[test]
    fn test_control_flow_graph_uses_unsaved_cross_file_modifier_source() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("Base.sol");
        let main = dir.path().join("Main.sol");

        fs::write(
            &base,
            r#"pragma solidity ^0.8.0;
contract Base {
    modifier guarded() {
        require(false);
        _;
    }
}
"#,
        )
        .unwrap();
        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
import "./Base.sol";

contract Main is Base {
    function run() public guarded returns (uint256) {
        return 1;
    }
}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let main_source = fs::read_to_string(&main).unwrap();
        let unsaved_base = r#"pragma solidity ^0.8.0;
contract Base {
    modifier guarded() {
        assert(true);
        _;
    }
}
"#;
        let get_source = |path: &Path| {
            if normalize_path(path) == normalize_path(&base) {
                Some(unsaved_base.to_string())
            } else {
                fs::read_to_string(path).ok()
            }
        };

        let graph = index
            .control_flow_graph(
                &main,
                &main_source,
                main_source.find("run").unwrap(),
                &get_source,
            )
            .expect("control-flow graph");

        assert!(graph.nodes.iter().any(|node| node.label == "call assert"));
        assert!(!graph.nodes.iter().any(|node| node.label == "call require"));
    }

    #[test]
    fn test_control_flow_graph_expands_inline_assembly_yul_flow() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");

        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
contract Main {
    function run(uint256 amount) public returns (uint256) {
        assembly {
            let result := add(amount, 1)
            if gt(result, 10) {
                mstore(0x00, result)
                leave
            }
        }

        return amount;
    }
}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let main_source = fs::read_to_string(&main).unwrap();
        let graph = index
            .control_flow_graph(
                &main,
                &main_source,
                main_source.find("run").unwrap(),
                &|path| fs::read_to_string(path).ok(),
            )
            .expect("control-flow graph");

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "assembly" && node.kind == Some(GraphNodeKind::Assembly)));
        assert!(graph.nodes.iter().any(|node| {
            node.label == "let result := add(amount, 1)"
                && node.kind == Some(GraphNodeKind::Declaration)
        }));
        assert!(graph.nodes.iter().any(|node| {
            node.label == "if gt(result, 10)" && node.kind == Some(GraphNodeKind::Branch)
        }));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "call mstore" && node.kind == Some(GraphNodeKind::Call)));
        assert!(graph.nodes.iter().any(|node| {
            node.label == "leave" && node.kind == Some(GraphNodeKind::TerminalReturn)
        }));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("leave")
                && edge.kind == Some(GraphEdgeKind::Return)));
    }

    #[test]
    fn test_control_flow_graph_expands_yul_function_subgraphs_and_terminal_builtins() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");

        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;
contract Main {
    function run(uint256 amount) public returns (uint256) {
        assembly {
            function helper(value) -> result {
                if gt(value, 10) {
                    revert(0, 0)
                }
                if iszero(value) {
                    leave
                }
                result := add(value, 1)
            }

            let computed := helper(amount)
            return(0, 0)
        }
    }
}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let main_source = fs::read_to_string(&main).unwrap();
        let graph = index
            .control_flow_graph(
                &main,
                &main_source,
                main_source.find("run").unwrap(),
                &|path| fs::read_to_string(path).ok(),
            )
            .expect("control-flow graph");

        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "function helper"
                && node.kind == Some(GraphNodeKind::Declaration)));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.label == "end helper" && node.kind == Some(GraphNodeKind::Block)));
        assert!(
            graph
                .nodes
                .iter()
                .any(|node| node.label == "revert"
                    && node.kind == Some(GraphNodeKind::TerminalRevert))
        );
        assert!(
            graph
                .nodes
                .iter()
                .any(|node| node.label == "return"
                    && node.kind == Some(GraphNodeKind::TerminalReturn))
        );
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("calls")));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("revert")
                && edge.kind == Some(GraphEdgeKind::Revert)));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.label.as_deref() == Some("return")
                && edge.kind == Some(GraphEdgeKind::Return)));

        let global_exit = graph
            .nodes
            .iter()
            .find(|node| node.kind == Some(GraphNodeKind::Exit))
            .map(|node| node.id.as_str())
            .expect("global exit");
        let helper_exit = graph
            .nodes
            .iter()
            .find(|node| node.label == "end helper")
            .map(|node| node.id.as_str())
            .expect("helper exit");
        let revert = graph
            .nodes
            .iter()
            .find(|node| node.label == "revert")
            .map(|node| node.id.as_str())
            .expect("revert node");
        let leave = graph
            .nodes
            .iter()
            .find(|node| node.label == "leave")
            .map(|node| node.id.as_str())
            .expect("leave node");
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.from == revert && edge.to == global_exit));
        assert!(!graph
            .edges
            .iter()
            .any(|edge| edge.from == revert && edge.to == helper_exit));
        assert!(graph
            .edges
            .iter()
            .any(|edge| edge.from == leave && edge.to == helper_exit));
    }

    #[test]
    fn test_inheritance_hints_report_nearest_override_origins() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");

        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;

interface IRouter {
    function swap(address tokenIn, uint256 amountIn) external returns (uint256);
}

abstract contract BaseRouter {
    function swap(address tokenIn, uint256 amountIn) public virtual returns (uint256);
}

contract Router is BaseRouter, IRouter {
    function swap(address tokenIn, uint256 amountIn) public override(BaseRouter, IRouter) returns (uint256) {
        return amountIn;
    }
}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let source = fs::read_to_string(&main).unwrap();
        let hints = index.inheritance_hints(&main, &source, &|path| fs::read_to_string(path).ok());
        let labels = hints
            .iter()
            .map(|hint| hint.label.clone())
            .collect::<Vec<_>>();

        assert_eq!(hints.len(), 2);
        assert!(labels
            .iter()
            .any(|label| label == "linearized: IRouter -> BaseRouter"));
        assert!(labels
            .iter()
            .any(|label| label == "overrides BaseRouter; implements IRouter"));
        assert!(hints.iter().any(|hint| hint
            .tooltip
            .contains("Linearized precedence: Router -> IRouter -> BaseRouter")));
        assert!(hints
            .iter()
            .any(|hint| hint.tooltip.contains("BaseRouter.swap")));
        assert!(hints
            .iter()
            .any(|hint| hint.tooltip.contains("IRouter.swap")));
    }

    #[test]
    fn test_inheritance_hints_ignore_parameter_data_locations_for_override_identity() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
interface IReceiver {
    function receiveData(bytes calldata data) external;
}
contract Receiver is IReceiver {
    function receiveData(bytes memory data) public override {}
}
"#;
        fs::write(&main, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let hints = index.inheritance_hints(&main, source, &|path| fs::read_to_string(path).ok());

        assert!(hints
            .iter()
            .any(|hint| hint.label == "implements IReceiver"));
    }

    #[test]
    fn test_inheritance_hints_match_public_state_getters_to_interface_functions() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
interface IOwned {
    function owner() external view returns (address);
}
contract Owned is IOwned {
    address public override owner;
}
"#;
        fs::write(&main, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let hints = index.inheritance_hints(&main, source, &|path| fs::read_to_string(path).ok());

        assert!(hints.iter().any(|hint| hint.label == "implements IOwned"));
    }

    #[test]
    fn test_inheritance_hints_match_mapping_and_array_getters_to_interfaces() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");
        let source = r#"pragma solidity ^0.8.0;
interface IReadStore {
    function balanceOf(address) external view returns (uint256);
    function values(uint256) external view returns (uint256);
    function fixedValues(uint256) external view returns (uint256);
}
contract ReadStore is IReadStore {
    mapping(address => uint256) public override balanceOf;
    uint256[] public override values;
    uint256[4] public override fixedValues;
}
"#;
        fs::write(&main, source).unwrap();

        let index = ProjectIndex::build(dir.path());
        let hints = index.inheritance_hints(&main, source, &|path| fs::read_to_string(path).ok());

        assert_eq!(
            hints
                .iter()
                .filter(|hint| hint.label == "implements IReadStore")
                .count(),
            3
        );
    }

    #[test]
    fn test_inheritance_hints_surface_accessible_inherited_members() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("Main.sol");

        fs::write(
            &main,
            r#"pragma solidity ^0.8.0;

contract BaseVault {
    uint256 internal totalSupply;
    uint256 private secretSupply;
    event Transfer(address indexed from, address indexed to, uint256 amount);

    modifier onlyOwner() {
        _;
    }

    function pause() internal {}
}

contract Vault is BaseVault {}
"#,
        )
        .unwrap();

        let index = ProjectIndex::build(dir.path());
        let source = fs::read_to_string(&main).unwrap();
        let hints = index.inheritance_hints(&main, &source, &|path| fs::read_to_string(path).ok());
        let inherited = hints
            .iter()
            .find(|hint| hint.label.starts_with("inherits members:"))
            .expect("inherits members hint");

        assert!(inherited.tooltip.contains("BaseVault.totalSupply"));
        assert!(inherited.tooltip.contains("BaseVault.Transfer"));
        assert!(inherited.tooltip.contains("BaseVault.onlyOwner"));
        assert!(inherited.tooltip.contains("BaseVault.pause"));
        assert!(!inherited.tooltip.contains("BaseVault.secretSupply"));
    }
}
