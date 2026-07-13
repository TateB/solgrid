//! LSP server — main server implementation using tower-lsp-server.

use crate::document::DocumentStore;
use crate::{
    actions, completion, convert, definition, diagnostics, format, hover, semantic, signature_help,
};
use serde_json::json;
use solgrid_config::Config;
use solgrid_diagnostics::{FindingKind, FindingMeta, Severity as FindingSeverity};
use solgrid_linter::LintEngine;
use solgrid_project::{
    CallHierarchyEntry, GraphDocument, GraphKind, GraphLensSpec, ProjectIndex, ProjectSnapshot,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::notification::Notification;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};

/// Server settings from the client.
#[derive(Debug, Clone)]
pub struct ServerSettings {
    pub fix_on_save: bool,
    pub fix_on_save_unsafe: bool,
    pub format_on_save: bool,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            fix_on_save: true,
            fix_on_save_unsafe: false,
            format_on_save: true,
        }
    }
}

const RERUN_SECURITY_ANALYSIS_COMMAND: &str = "solgrid.workspace.rerunSecurityAnalysis";
const IMPORTS_GRAPH_COMMAND: &str = "solgrid.graph.imports";
const INHERITANCE_GRAPH_COMMAND: &str = "solgrid.graph.inheritance";
const LINEARIZED_INHERITANCE_GRAPH_COMMAND: &str = "solgrid.graph.linearizedInheritance";
const CONTROL_FLOW_GRAPH_COMMAND: &str = "solgrid.graph.controlFlow";
const PROJECT_INDEX_STATUS_NOTIFICATION: &str = "solgrid/projectIndexStatus";
const CHANGE_LINT_DEBOUNCE: Duration = Duration::from_millis(100);
const WORKSPACE_ANALYSIS_MAX_ATTEMPTS: usize = 3;

enum ProjectIndexStatusNotification {}

impl Notification for ProjectIndexStatusNotification {
    type Params = ProjectIndexStatusParams;

    const METHOD: &'static str = PROJECT_INDEX_STATUS_NOTIFICATION;
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectIndexStatusParams {
    state: String,
    files: usize,
    duration_ms: Option<u128>,
}

#[derive(Debug, Clone)]
struct OpenDocumentOverlay {
    uri: Uri,
    source: String,
    version: i32,
}

struct WorkspaceAnalysisInput {
    uri: Uri,
    source: String,
    version: Option<i32>,
    open_documents: HashMap<PathBuf, OpenDocumentOverlay>,
    project_generation: u64,
    disk_generation: u64,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceAnalysisSummary {
    files_analyzed: usize,
    diagnostics_published: usize,
    stale_diagnostics_cleared: usize,
    open_documents: usize,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphCommandArgs {
    uri: Uri,
    symbol_name: Option<String>,
    target_offset: Option<usize>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallHierarchyItemData {
    uri: Uri,
    target_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DetectorHintAnchor {
    name: String,
    kind_label: String,
    offset: usize,
    def_span: std::ops::Range<usize>,
}

#[derive(Debug, Clone)]
struct DetectorHintFinding {
    meta: FindingMeta,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetectorInlayHint {
    offset: usize,
    label: String,
    tooltip: String,
}

/// The solgrid LSP server.
#[derive(Clone)]
pub struct SolgridServer {
    client: Client,
    engine: Arc<RwLock<LintEngine>>,
    documents: Arc<RwLock<DocumentStore>>,
    pending_save_documents: Arc<RwLock<HashMap<Uri, String>>>,
    settings: Arc<RwLock<ServerSettings>>,
    workspace_root: Arc<RwLock<Option<PathBuf>>>,
    config_path: Arc<RwLock<Option<PathBuf>>>,
    config_cache: Arc<RwLock<ServerConfigCache>>,
    /// Cache of last-published LSP diagnostics per URI, for hover lookups.
    published_diagnostics: Arc<RwLock<HashMap<Uri, Vec<Diagnostic>>>>,
    /// Shared project index for workspace-wide navigation and remapping state.
    project_index: Arc<RwLock<ProjectIndex>>,
    /// False while the initial workspace-wide index is still being built.
    project_index_ready: Arc<AtomicBool>,
    /// Changes whenever project-wide semantic inputs change.
    project_generation: Arc<AtomicU64>,
    /// Changes when on-disk project inputs may have changed during a rebuild.
    project_disk_generation: Arc<AtomicU64>,
    /// Per-document epochs used to discard superseded debounced lint jobs.
    lint_epochs: Arc<RwLock<HashMap<Uri, u64>>>,
}

impl SolgridServer {
    /// Create a new server instance connected to the given LSP client.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            engine: Arc::new(RwLock::new(LintEngine::new())),
            documents: Arc::new(RwLock::new(DocumentStore::new())),
            pending_save_documents: Arc::new(RwLock::new(HashMap::new())),
            settings: Arc::new(RwLock::new(ServerSettings::default())),
            workspace_root: Arc::new(RwLock::new(None)),
            config_path: Arc::new(RwLock::new(None)),
            config_cache: Arc::new(RwLock::new(ServerConfigCache::default())),
            published_diagnostics: Arc::new(RwLock::new(HashMap::new())),
            project_index: Arc::new(RwLock::new(ProjectIndex::new(None))),
            project_index_ready: Arc::new(AtomicBool::new(true)),
            project_generation: Arc::new(AtomicU64::new(0)),
            project_disk_generation: Arc::new(AtomicU64::new(0)),
            lint_epochs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn resolve_config_for_path(
        &self,
        path: &std::path::Path,
    ) -> std::result::Result<Arc<Config>, String> {
        if let Some(config_path) = self.config_path.read().await.clone() {
            if let Some(config) = self.config_cache.read().await.explicit_config(&config_path) {
                return config.cloned_result();
            }

            let config = match load_explicit_config_entry(&config_path) {
                Ok(config) => config,
                Err(error) => {
                    self.client
                        .log_message(
                            MessageType::WARNING,
                            format!(
                                "Failed to load configured solgrid config {}: {error}",
                                config_path.display()
                            ),
                        )
                        .await;
                    CachedConfigEntry::Failed(error.into())
                }
            };
            self.config_cache
                .write()
                .await
                .store_explicit(config_path, config.clone());
            return config.into_result();
        }

        let cache_key = config_cache_key(path);
        if let Some(config) = self.config_cache.read().await.nearest_config(&cache_key) {
            return config.cloned_result();
        }

        let config = match load_nearest_config_entry(path) {
            Ok(config) => config,
            Err(error) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!(
                            "Failed to load solgrid config for {}: {error}",
                            path.display()
                        ),
                    )
                    .await;
                CachedConfigEntry::Failed(error.into())
            }
        };
        self.config_cache
            .write()
            .await
            .store_nearest(cache_key, config.clone());
        config.into_result()
    }

    async fn clear_config_cache(&self) {
        self.config_cache.write().await.clear();
    }

    async fn collect_open_document_overlays(
        &self,
    ) -> HashMap<std::path::PathBuf, OpenDocumentOverlay> {
        let documents = self.documents.read().await;
        documents
            .uris()
            .filter_map(|uri| {
                let document = documents.get(uri)?;
                Some((
                    normalize_path(&uri_to_path(uri)),
                    OpenDocumentOverlay {
                        uri: document.uri.clone(),
                        source: document.content.clone(),
                        version: document.version,
                    },
                ))
            })
            .collect()
    }

    async fn workspace_analysis_input(
        &self,
        path: &std::path::Path,
    ) -> Option<WorkspaceAnalysisInput> {
        let path = normalize_path(path);
        let documents = self.documents.read().await;
        let open_documents = documents
            .uris()
            .filter_map(|uri| {
                let document = documents.get(uri)?;
                Some((
                    normalize_path(&uri_to_path(uri)),
                    OpenDocumentOverlay {
                        uri: document.uri.clone(),
                        source: document.content.clone(),
                        version: document.version,
                    },
                ))
            })
            .collect::<HashMap<_, _>>();
        let project_generation = self.project_generation.load(Ordering::SeqCst);
        let disk_generation = self.project_disk_generation.load(Ordering::SeqCst);
        drop(documents);

        if let Some(document) = open_documents.get(&path) {
            return Some(WorkspaceAnalysisInput {
                uri: document.uri.clone(),
                source: document.source.clone(),
                version: Some(document.version),
                open_documents,
                project_generation,
                disk_generation,
            });
        }

        Some(WorkspaceAnalysisInput {
            uri: self
                .published_uri_for_path(&path)
                .await
                .or_else(|| path_to_uri_option(&path))?,
            source: std::fs::read_to_string(&path).ok()?,
            version: None,
            open_documents,
            project_generation,
            disk_generation,
        })
    }

    async fn workspace_analysis_input_is_current(
        &self,
        path: &std::path::Path,
        input: &WorkspaceAnalysisInput,
    ) -> bool {
        let documents = self.documents.read().await;
        workspace_analysis_input_is_current(
            &documents,
            path,
            &input.source,
            input.version,
            input.project_generation,
            self.project_generation.load(Ordering::SeqCst),
            input.disk_generation,
            self.project_disk_generation.load(Ordering::SeqCst),
        )
    }

    async fn rebuild_project_index(&self) {
        self.project_disk_generation.fetch_add(1, Ordering::SeqCst);
        self.project_index_ready.store(false, Ordering::SeqCst);
        let indexed_files = self.project_index.read().await.indexed_file_count();
        send_project_index_status(&self.client, "building", indexed_files, None).await;

        let started_at = Instant::now();
        loop {
            let disk_generation = self.project_disk_generation.load(Ordering::SeqCst);
            let workspace_root = self.workspace_root.read().await.clone();
            let fallback_root = workspace_root.clone();
            let mut rebuilt = match tokio::task::spawn_blocking(move || {
                workspace_root
                    .as_deref()
                    .map_or_else(|| ProjectIndex::new(None), ProjectIndex::build)
            })
            .await
            {
                Ok(index) => index,
                Err(error) => {
                    self.client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to rebuild project index: {error}"),
                        )
                        .await;
                    ProjectIndex::new(fallback_root)
                }
            };

            let documents = self.documents.read().await;
            for uri in documents.uris() {
                let Some(document) = documents.get(uri) else {
                    continue;
                };
                let path = normalize_path(&uri_to_path(uri));
                if is_solidity_path(&path) {
                    rebuilt.update_file(&path, &document.content);
                }
            }

            let mut live_index = self.project_index.write().await;
            if self.project_disk_generation.load(Ordering::SeqCst) != disk_generation {
                drop(live_index);
                drop(documents);
                continue;
            }

            let indexed_files = rebuilt.indexed_file_count();
            *live_index = rebuilt;
            self.project_generation.fetch_add(1, Ordering::SeqCst);
            self.project_index_ready.store(true, Ordering::SeqCst);
            drop(live_index);
            drop(documents);

            send_project_index_status(
                &self.client,
                "ready",
                indexed_files,
                Some(started_at.elapsed().as_millis()),
            )
            .await;
            let _ = self.client.code_lens_refresh().await;
            let _ = self.client.semantic_tokens_refresh().await;
            break;
        }
    }

    async fn publish_config_error(&self, uri: &Uri, version: Option<i32>, error: &str) {
        let diagnostics = vec![Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("solgrid".into()),
            message: format!("Error loading config: {error}"),
            ..Default::default()
        }];

        {
            let mut cache = self.published_diagnostics.write().await;
            cache.insert(uri.clone(), diagnostics.clone());
        }

        self.client
            .publish_diagnostics(uri.clone(), diagnostics, version)
            .await;
    }

    async fn set_pending_save_document(&self, uri: &Uri, source: Option<String>) {
        let mut pending = self.pending_save_documents.write().await;
        if let Some(source) = source {
            pending.insert(uri.clone(), source);
        } else {
            pending.remove(uri);
        }
    }

    async fn source_for_formatting(&self, uri: &Uri) -> Option<String> {
        if let Some(source) = self.pending_save_documents.read().await.get(uri).cloned() {
            return Some(source);
        }

        let documents = self.documents.read().await;
        documents.get(uri).map(|doc| doc.content.clone())
    }

    async fn relint_open_documents(&self) {
        let uris: Vec<Uri> = {
            let documents = self.documents.read().await;
            documents.uris().cloned().collect()
        };

        for uri in uris {
            if is_solidity_file(&uri) {
                self.lint_and_publish(&uri).await;
            }
        }
    }

    fn schedule_relint_paths(&self, paths: Vec<PathBuf>) {
        self.schedule_relint_paths_with_options(paths, false);
    }

    fn schedule_workspace_analysis_paths(&self, paths: Vec<PathBuf>) {
        self.schedule_relint_paths_with_options(paths, true);
    }

    fn schedule_relint_paths_with_options(&self, paths: Vec<PathBuf>, publish_unseen: bool) {
        let server = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(CHANGE_LINT_DEBOUNCE).await;
            let generation = server.project_generation.load(Ordering::SeqCst);
            server
                .relint_paths_with_options(&paths, generation, publish_unseen)
                .await;
        });
    }

    async fn relint_paths(&self, paths: &[PathBuf], expected_generation: u64) {
        self.relint_paths_with_options(paths, expected_generation, false)
            .await;
    }

    async fn relint_paths_with_options(
        &self,
        paths: &[PathBuf],
        expected_generation: u64,
        publish_unseen: bool,
    ) {
        let expected_disk_generation = self.project_disk_generation.load(Ordering::SeqCst);
        if self.project_generation.load(Ordering::SeqCst) != expected_generation {
            self.schedule_relint_paths_with_options(paths.to_vec(), publish_unseen);
            return;
        }
        let open_docs = self.collect_open_document_overlays().await;
        let mut paths = paths
            .iter()
            .map(|path| normalize_path(path))
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();

        for path in &paths {
            if self.project_generation.load(Ordering::SeqCst) != expected_generation
                || self.project_disk_generation.load(Ordering::SeqCst) != expected_disk_generation
            {
                self.schedule_relint_paths_with_options(paths.to_vec(), publish_unseen);
                return;
            }
            let (uri, version, source) = if let Some(document) = open_docs.get(path) {
                (
                    document.uri.clone(),
                    Some(document.version),
                    document.source.clone(),
                )
            } else {
                let Some(uri) = self
                    .published_uri_for_path(path)
                    .await
                    .or_else(|| publish_unseen.then(|| path_to_uri_option(path)).flatten())
                else {
                    continue;
                };
                let Ok(source) = std::fs::read_to_string(path) else {
                    self.clear_published_diagnostics(uri).await;
                    continue;
                };
                (uri, None, source)
            };

            let result = self.lint_source(path, &source, &open_docs).await;
            let is_current = {
                let documents = self.documents.read().await;
                workspace_analysis_input_is_current(
                    &documents,
                    path,
                    &source,
                    version,
                    expected_generation,
                    self.project_generation.load(Ordering::SeqCst),
                    expected_disk_generation,
                    self.project_disk_generation.load(Ordering::SeqCst),
                )
            };
            if !is_current {
                self.schedule_relint_paths_with_options(paths.to_vec(), publish_unseen);
                return;
            }

            match result {
                Ok(diagnostics) => {
                    self.publish_cached_diagnostics(uri, diagnostics, version)
                        .await;
                }
                Err(error) => self.publish_config_error(&uri, version, &error).await,
            }
        }
    }

    async fn invalidate_pending_lint(&self, uri: &Uri) -> u64 {
        let mut epochs = self.lint_epochs.write().await;
        let epoch = epochs.entry(uri.clone()).or_default();
        *epoch = epoch.saturating_add(1);
        *epoch
    }

    async fn schedule_lint_and_publish(&self, uri: Uri) {
        let epoch = self.invalidate_pending_lint(&uri).await;
        let server = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(CHANGE_LINT_DEBOUNCE).await;
            let is_current = server
                .lint_epochs
                .read()
                .await
                .get(&uri)
                .is_some_and(|current| *current == epoch);
            if is_current {
                server.lint_and_publish(&uri).await;
            }
        });
    }

    async fn lint_source(
        &self,
        path: &std::path::Path,
        source: &str,
        open_docs: &HashMap<std::path::PathBuf, OpenDocumentOverlay>,
    ) -> std::result::Result<Vec<Diagnostic>, String> {
        let config = self.resolve_config_for_path(path).await?;

        let get_source = |candidate: &std::path::Path| -> Option<String> {
            if let Some(document) = open_docs.get(&normalize_path(candidate)) {
                return Some(document.source.clone());
            }
            std::fs::read_to_string(candidate).ok()
        };

        let (remappings, compiler_diags) = {
            let project_index = self.project_index.read().await;
            let remappings = project_index.remappings_for_file(path);
            let compiler_diags = diagnostics::compiler_to_lsp_diagnostics_with_config(
                &project_index,
                source,
                path,
                &get_source,
                &config,
            );
            (remappings, compiler_diags)
        };

        let engine = self.engine.read().await;
        let mut lsp_diags = diagnostics::lint_to_lsp_diagnostics_with_remappings(
            &engine,
            source,
            path,
            &config,
            &remappings,
        );
        drop(engine);

        lsp_diags.extend(compiler_diags);
        Ok(diagnostics::suppress_redundant_diagnostics(lsp_diags))
    }

    async fn graph_document(
        &self,
        kind: GraphKind,
        args: &GraphCommandArgs,
    ) -> Option<GraphDocument> {
        if !is_solidity_file(&args.uri) {
            return None;
        }

        let path = normalize_path(&uri_to_path(&args.uri));
        let open_docs = self.collect_open_document_overlays().await;
        let source = open_docs
            .get(&path)
            .map(|document| document.source.clone())
            .or_else(|| std::fs::read_to_string(&path).ok())?;

        let get_source = |candidate: &std::path::Path| -> Option<String> {
            if let Some(document) = open_docs.get(&normalize_path(candidate)) {
                return Some(document.source.clone());
            }
            std::fs::read_to_string(candidate).ok()
        };

        let project_index = self.project_index.read().await;
        match kind {
            GraphKind::Imports => project_index.imports_graph(&path, &source, &get_source),
            GraphKind::Inheritance => project_index.inheritance_graph(
                &path,
                &source,
                args.symbol_name.as_deref()?,
                &get_source,
            ),
            GraphKind::LinearizedInheritance => project_index.linearized_inheritance_graph(
                &path,
                &source,
                args.symbol_name.as_deref()?,
                &get_source,
            ),
            GraphKind::ControlFlow => {
                project_index.control_flow_graph(&path, &source, args.target_offset?, &get_source)
            }
        }
    }

    async fn publish_cached_diagnostics(
        &self,
        uri: Uri,
        diagnostics: Vec<Diagnostic>,
        version: Option<i32>,
    ) {
        {
            let mut cache = self.published_diagnostics.write().await;
            cache.insert(uri.clone(), diagnostics.clone());
        }

        self.client
            .publish_diagnostics(uri, diagnostics, version)
            .await;
    }

    async fn clear_published_diagnostics(&self, uri: Uri) {
        {
            let mut cache = self.published_diagnostics.write().await;
            cache.remove(&uri);
        }
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn publish_closed_file_diagnostics(&self, uri: Uri, path: std::path::PathBuf) {
        let mut affected_paths = {
            let mut project_index = self.project_index.write().await;
            project_index.sync_closed_file(&path);
            project_index.transitive_import_dependents(&path)
        };
        let refresh_semantic_dependents = !affected_paths.is_empty();
        let generation = self.project_generation.fetch_add(1, Ordering::SeqCst) + 1;

        if path.exists() {
            affected_paths.push(path);
        } else {
            self.clear_published_diagnostics(uri).await;
        }
        if refresh_semantic_dependents {
            let _ = self.client.semantic_tokens_refresh().await;
        }
        self.relint_paths(&affected_paths, generation).await;
    }

    async fn published_uri_for_path(&self, path: &std::path::Path) -> Option<Uri> {
        let path = normalize_path(path);
        let cache = self.published_diagnostics.read().await;
        cache.keys().find_map(|uri| {
            let candidate = uri_to_path_option(uri)?;
            (normalize_path(&candidate) == path).then(|| uri.clone())
        })
    }

    async fn rerun_workspace_analysis(&self) -> WorkspaceAnalysisSummary {
        self.clear_config_cache().await;

        self.rebuild_project_index().await;
        let open_docs = self.collect_open_document_overlays().await;

        let candidate_paths = {
            let project_index = self.project_index.read().await;
            let mut paths = project_index.indexed_paths();
            paths.extend(open_docs.keys().cloned());
            paths.sort();
            paths.dedup();
            paths
        };
        let previously_published: Vec<Uri> = {
            let cache = self.published_diagnostics.read().await;
            cache.keys().cloned().collect()
        };

        let mut published = HashSet::new();
        let mut files_analyzed = 0;
        let mut diagnostics_published = 0;

        for path in candidate_paths {
            if !is_solidity_path(&path) {
                continue;
            }

            for attempt in 0..WORKSPACE_ANALYSIS_MAX_ATTEMPTS {
                let Some(input) = self.workspace_analysis_input(&path).await else {
                    break;
                };
                let result = self
                    .lint_source(&path, &input.source, &input.open_documents)
                    .await;
                if !self
                    .workspace_analysis_input_is_current(&path, &input)
                    .await
                {
                    if attempt + 1 == WORKSPACE_ANALYSIS_MAX_ATTEMPTS {
                        // Keep any newer publication intact and let the
                        // debounced guarded path finish once edits settle.
                        published.insert(input.uri);
                        self.schedule_workspace_analysis_paths(vec![path.clone()]);
                        break;
                    }
                    continue;
                }

                files_analyzed += 1;
                published.insert(input.uri.clone());
                match result {
                    Ok(lsp_diags) => {
                        diagnostics_published += lsp_diags.len();
                        self.publish_cached_diagnostics(input.uri, lsp_diags, input.version)
                            .await;
                    }
                    Err(error) => {
                        diagnostics_published += 1;
                        self.publish_config_error(&input.uri, input.version, &error)
                            .await;
                    }
                }
                break;
            }
        }

        let mut stale_diagnostics_cleared = 0;
        for uri in previously_published {
            if published.contains(&uri) {
                continue;
            }
            stale_diagnostics_cleared += 1;
            self.clear_published_diagnostics(uri).await;
        }

        WorkspaceAnalysisSummary {
            files_analyzed,
            diagnostics_published,
            stale_diagnostics_cleared,
            open_documents: self.collect_open_document_overlays().await.len(),
        }
    }

    /// Lint a document and publish diagnostics to the client.
    async fn lint_and_publish(&self, uri: &Uri) {
        let (open_docs, path, document, dependents, generation, disk_generation) = loop {
            // Snapshot every overlay while holding the document read lock, then
            // update the index under the same lock. An imported didChange cannot
            // slip between the snapshot and this file's generation increment.
            let documents = self.documents.read().await;
            let open_docs = documents
                .uris()
                .filter_map(|open_uri| {
                    let document = documents.get(open_uri)?;
                    Some((
                        normalize_path(&uri_to_path(open_uri)),
                        OpenDocumentOverlay {
                            uri: document.uri.clone(),
                            source: document.content.clone(),
                            version: document.version,
                        },
                    ))
                })
                .collect::<HashMap<_, _>>();
            let path = normalize_path(&uri_to_path(uri));
            let Some(document) = open_docs.get(&path).cloned() else {
                return;
            };
            let input_generation = self.project_generation.load(Ordering::SeqCst);
            let mut project_index = self.project_index.write().await;
            if self.project_generation.load(Ordering::SeqCst) != input_generation {
                drop(project_index);
                drop(documents);
                continue;
            }
            project_index.update_file(&path, &document.source);
            let dependents = project_index.transitive_import_dependents(&path);
            let generation = self.project_generation.fetch_add(1, Ordering::SeqCst) + 1;
            let disk_generation = self.project_disk_generation.load(Ordering::SeqCst);
            drop(project_index);
            drop(documents);
            break (
                open_docs,
                path,
                document,
                dependents,
                generation,
                disk_generation,
            );
        };

        let result = self.lint_source(&path, &document.source, &open_docs).await;
        let is_current = {
            let documents = self.documents.read().await;
            workspace_analysis_input_is_current(
                &documents,
                &path,
                &document.source,
                Some(document.version),
                generation,
                self.project_generation.load(Ordering::SeqCst),
                disk_generation,
                self.project_disk_generation.load(Ordering::SeqCst),
            )
        };
        if !is_current {
            let mut retry_paths = dependents;
            retry_paths.push(path);
            self.schedule_relint_paths(retry_paths);
            return;
        }

        match result {
            Ok(lsp_diags) => {
                self.publish_cached_diagnostics(uri.clone(), lsp_diags, Some(document.version))
                    .await;
            }
            Err(error) => {
                self.publish_config_error(uri, Some(document.version), &error)
                    .await;
            }
        }
        if !dependents.is_empty() {
            let _ = self.client.semantic_tokens_refresh().await;
        }
        self.relint_paths(&dependents, generation).await;
    }
}

impl LanguageServer for SolgridServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let init_settings = params
            .initialization_options
            .clone()
            .and_then(|options| serde_json::from_value::<ClientSettings>(options).ok());
        if let Some(settings) = &init_settings {
            let mut server_settings = self.settings.write().await;
            server_settings.fix_on_save = settings.fix_on_save.unwrap_or(true);
            server_settings.fix_on_save_unsafe = settings.fix_on_save_unsafe.unwrap_or(false);
            server_settings.format_on_save = settings.format_on_save.unwrap_or(true);
        }

        let root_uri = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .map(|f| &f.uri);
        #[allow(deprecated)]
        let root_uri = root_uri.or(params.root_uri.as_ref());
        if let Some(root_uri) = root_uri {
            if let Some(root_path) = uri_to_path_option(root_uri) {
                let mut workspace_root = self.workspace_root.write().await;
                *workspace_root = Some(root_path.clone());
                drop(workspace_root);

                if let Some(settings) = &init_settings {
                    if let Some(config_path) = settings.config_path.clone() {
                        let mut config_path_slot = self.config_path.write().await;
                        *config_path_slot =
                            Some(resolve_config_path(config_path, Some(&root_path)));
                    }
                }

                *self.project_index.write().await = ProjectIndex::new(Some(root_path.clone()));
                self.project_index_ready.store(false, Ordering::SeqCst);

                // Rebuild off-thread and atomically merge the latest open
                // buffers before replacing the provisional live index.
                let server = self.clone();
                tokio::spawn(async move {
                    server.rebuild_project_index().await;
                });
            }
        } else if let Some(settings) = &init_settings {
            if let Some(config_path) = settings.config_path.clone() {
                let mut config_path_slot = self.config_path.write().await;
                *config_path_slot = Some(resolve_config_path(config_path, None));
            }
            *self.project_index.write().await = ProjectIndex::new(None);
            self.project_generation.fetch_add(1, Ordering::SeqCst);
            self.project_index_ready.store(true, Ordering::SeqCst);
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        will_save: None,
                        will_save_wait_until: Some(true),
                    },
                )),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::REFACTOR,
                            CodeActionKind::REFACTOR_REWRITE,
                            CodeActionKind::SOURCE_FIX_ALL,
                        ]),
                        resolve_provider: None,
                        work_done_progress_options: Default::default(),
                    },
                )),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
                    InlayHintOptions {
                        resolve_provider: Some(false),
                        work_done_progress_options: Default::default(),
                    },
                ))),
                semantic_tokens_provider: Some(
                    SemanticTokensOptions {
                        work_done_progress_options: Default::default(),
                        legend: semantic::semantic_token_legend(),
                        range: Some(true),
                        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                    }
                    .into(),
                ),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        RERUN_SECURITY_ANALYSIS_COMMAND.to_string(),
                        IMPORTS_GRAPH_COMMAND.to_string(),
                        INHERITANCE_GRAPH_COMMAND.to_string(),
                        LINEARIZED_INHERITANCE_GRAPH_COMMAND.to_string(),
                        CONTROL_FLOW_GRAPH_COMMAND.to_string(),
                    ],
                    work_done_progress_options: Default::default(),
                }),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["/".into(), " ".into(), ".".into()]),
                    ..Default::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    retrigger_characters: Some(vec![",".into()]),
                    work_done_progress_options: Default::default(),
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "solgrid".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            offset_encoding: None,
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "solgrid LSP server initialized")
            .await;
        let indexed_files = self.project_index.read().await.indexed_file_count();
        if self.project_index_ready.load(Ordering::SeqCst) {
            send_project_index_status(&self.client, "ready", indexed_files, None).await;
            let _ = self.client.code_lens_refresh().await;
        } else {
            send_project_index_status(&self.client, "building", indexed_files, None).await;
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        if !is_solidity_file(&uri) {
            return;
        }

        {
            let mut documents = self.documents.write().await;
            documents.open(
                uri.clone(),
                params.text_document.text,
                params.text_document.version,
            );
        }
        self.project_generation.fetch_add(1, Ordering::SeqCst);
        self.set_pending_save_document(&uri, None).await;

        self.invalidate_pending_lint(&uri).await;
        self.lint_and_publish(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if !is_solidity_file(&uri) {
            return;
        }

        // With full sync, the last content change contains the complete text
        if let Some(change) = params.content_changes.into_iter().last() {
            let mut documents = self.documents.write().await;
            documents.update(&uri, change.text, params.text_document.version);
            self.project_generation.fetch_add(1, Ordering::SeqCst);
        }
        self.set_pending_save_document(&uri, None).await;

        self.schedule_lint_and_publish(uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let path = normalize_path(&uri_to_path(&uri));
        let config_path = self.config_path.read().await.clone();
        let refresh_config = is_config_refresh_path(&path, config_path.as_deref());
        let refresh_workspace_state = is_workspace_state_refresh_path(&path);
        if refresh_config || refresh_workspace_state {
            self.rerun_workspace_analysis().await;
            return;
        }

        if !is_solidity_file(&uri) {
            return;
        }
        self.project_disk_generation.fetch_add(1, Ordering::SeqCst);

        let saved_source = match params.text {
            Some(text) => Some(text),
            None => self.pending_save_documents.read().await.get(&uri).cloned(),
        };
        if let Some(saved_source) = saved_source {
            let mut documents = self.documents.write().await;
            documents.set_content(&uri, saved_source);
            self.project_generation.fetch_add(1, Ordering::SeqCst);
        }
        self.set_pending_save_document(&uri, None).await;
        // Fixes and formatting are applied by willSaveWaitUntil; re-lint the
        // final buffer after the corresponding didChange/didSave sequence.
        self.invalidate_pending_lint(&uri).await;
        self.lint_and_publish(&uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let path = normalize_path(&uri_to_path(&uri));
        self.invalidate_pending_lint(&uri).await;
        {
            let mut documents = self.documents.write().await;
            documents.close(&uri);
        }
        if is_solidity_file(&uri) {
            self.project_disk_generation.fetch_add(1, Ordering::SeqCst);
            self.publish_closed_file_diagnostics(uri.clone(), path)
                .await;
        } else {
            self.clear_published_diagnostics(uri.clone()).await;
        }
        self.set_pending_save_document(&uri, None).await;
    }

    async fn will_save_wait_until(
        &self,
        params: WillSaveTextDocumentParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let settings = self.settings.read().await.clone();
        if !settings.fix_on_save && !settings.format_on_save {
            self.set_pending_save_document(uri, None).await;
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        drop(documents);

        let path = normalize_path(&uri_to_path(uri));
        let config = match self.resolve_config_for_path(&path).await {
            Ok(config) => config,
            Err(_) => {
                self.set_pending_save_document(uri, None).await;
                return Ok(None);
            }
        };
        let remappings = self.project_index.read().await.remappings_for_file(&path);
        let mut final_source = source.clone();

        // Apply safe fixes
        let engine = self.engine.read().await;
        if settings.fix_on_save {
            let (fixed, _) = engine.fix_source_with_remappings(
                &final_source,
                &path,
                &config,
                settings.fix_on_save_unsafe,
                &remappings,
            );
            final_source = fixed;
        }
        drop(engine);

        // Apply formatting
        if settings.format_on_save {
            if let Ok(formatted) = solgrid_formatter::format_source(&final_source, &config.format) {
                final_source = formatted;
            }
        }

        if final_source == source {
            self.set_pending_save_document(uri, None).await;
            Ok(None)
        } else {
            self.set_pending_save_document(uri, Some(final_source.clone()))
                .await;
            Ok(Some(vec![TextEdit {
                range: full_document_range(&source),
                new_text: final_source,
            }]))
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        drop(documents);

        let path = normalize_path(&uri_to_path(uri));
        let Ok(config) = self.resolve_config_for_path(&path).await else {
            return Ok(None);
        };
        let remappings = self.project_index.read().await.remappings_for_file(&path);

        let engine = self.engine.read().await;
        let result = actions::code_actions_with_remappings(
            &engine,
            &source,
            &path,
            &config,
            &params.range,
            uri,
            &remappings,
        );
        drop(engine);

        if result.is_empty() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let source = match self.source_for_formatting(uri).await {
            Some(source) => source,
            None => return Ok(None),
        };

        let path = normalize_path(&uri_to_path(uri));
        let Ok(config) = self.resolve_config_for_path(&path).await else {
            return Ok(None);
        };
        let edits = format::format_document(&source, &config.format);

        if edits.is_empty() {
            Ok(None)
        } else {
            Ok(Some(edits))
        }
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let source = match self.source_for_formatting(uri).await {
            Some(source) => source,
            None => return Ok(None),
        };

        let path = normalize_path(&uri_to_path(uri));
        let Ok(config) = self.resolve_config_for_path(&path).await else {
            return Ok(None);
        };
        let edits = format::format_range(&source, &params.range, &config.format);

        if edits.is_empty() {
            Ok(None)
        } else {
            Ok(Some(edits))
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let position = &params.text_document_position_params.position;

        let cache = self.published_diagnostics.read().await;
        let lsp_diags: Vec<Diagnostic> = cache.get(uri).cloned().unwrap_or_default();
        drop(cache);

        let documents = self.documents.read().await;
        let source = documents.get(uri).map(|d| d.content.clone());
        // Collect open document contents for cross-file lookups.
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let source = source.unwrap_or_default();

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let project_index = self.project_index.read().await;

        let engine = self.engine.read().await;
        Ok(hover::hover_at_position(
            &engine,
            &lsp_diags,
            position,
            &source,
            uri,
            &get_source,
            project_index.resolver(),
        ))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        // Collect open document contents for cross-file lookups.
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let project_index = self.project_index.read().await;

        let position = &params.text_document_position.position;
        let engine = self.engine.read().await;
        let items = completion::completions(
            &engine,
            &source,
            position,
            uri,
            &get_source,
            project_index.resolver(),
            &project_index,
        );
        drop(engine);

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let current_file = normalize_path(&uri_to_path(uri));
        let project_index = self.project_index.read().await;

        Ok(signature_help::signature_help_at_position(
            &source,
            &params.text_document_position_params.position,
            &get_source,
            project_index.resolver(),
            Some(current_file.as_path()),
        ))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let current_file = normalize_path(&uri_to_path(uri));
        let detector_diagnostics = {
            let cache = self.published_diagnostics.read().await;
            cache.get(uri).cloned().unwrap_or_default()
        };
        let project_index = self.project_index.read().await;
        let start = convert::position_to_offset(&source, params.range.start);
        let end = convert::position_to_offset(&source, params.range.end);
        let mut hints = semantic::parameter_name_hints_in_range(
            &source,
            start,
            end,
            Some(current_file.as_path()),
            &get_source,
            project_index.resolver(),
        )
        .into_iter()
        .map(|hint| InlayHint {
            position: convert::offset_to_position(&source, hint.offset),
            label: InlayHintLabel::String(hint.label),
            kind: Some(InlayHintKind::PARAMETER),
            text_edits: None,
            tooltip: None,
            padding_left: None,
            padding_right: Some(true),
            data: None,
        })
        .collect::<Vec<_>>();
        hints.extend(
            semantic::selector_hints_in_range(
                &source,
                start,
                end,
                Some(current_file.as_path()),
                &get_source,
                project_index.resolver(),
            )
            .into_iter()
            .map(|hint| InlayHint {
                position: convert::offset_to_position(&source, hint.offset),
                label: InlayHintLabel::String(hint.label),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: Some(InlayHintTooltip::String(hint.tooltip)),
                padding_left: Some(true),
                padding_right: None,
                data: None,
            }),
        );
        hints.extend(
            project_index
                .inheritance_hints(&current_file, &source, &get_source)
                .into_iter()
                .filter(|hint| hint.offset >= start && hint.offset <= end)
                .map(|hint| InlayHint {
                    position: convert::offset_to_position(&source, hint.offset),
                    label: InlayHintLabel::String(hint.label),
                    kind: Some(InlayHintKind::TYPE),
                    text_edits: None,
                    tooltip: Some(InlayHintTooltip::String(hint.tooltip)),
                    padding_left: Some(true),
                    padding_right: None,
                    data: None,
                }),
        );
        if let Some(snapshot) = project_index.snapshot_for_source(&current_file, &source) {
            hints.extend(
                detector_inlay_hints(&snapshot, &detector_diagnostics, start, end)
                    .into_iter()
                    .map(|hint| InlayHint {
                        position: convert::offset_to_position(&source, hint.offset),
                        label: InlayHintLabel::String(hint.label),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: Some(InlayHintTooltip::String(hint.tooltip)),
                        padding_left: Some(true),
                        padding_right: None,
                        data: None,
                    }),
            );
        }
        hints.sort_by_key(|hint| (hint.position.line, hint.position.character));

        if hints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hints))
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        let version = doc.version;
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        // Document changes bump the project generation while holding the
        // document write lock. Capture it under this read lock so token data
        // can never be tagged with a generation newer than its source snapshot.
        let generation = self.project_generation.load(Ordering::SeqCst);
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let current_file = normalize_path(&uri_to_path(uri));
        let project_index = self.project_index.read().await;
        let tokens = semantic::semantic_tokens(
            &source,
            Some(current_file.as_path()),
            &get_source,
            project_index.resolver(),
        );

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: Some(semantic_tokens_result_id(version, generation)),
            data: tokens,
        })))
    }

    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        let version = doc.version;
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        let generation = self.project_generation.load(Ordering::SeqCst);
        drop(documents);

        let current_result_id = semantic_tokens_result_id(version, generation);
        if params.previous_result_id == current_result_id {
            return Ok(Some(SemanticTokensFullDeltaResult::TokensDelta(
                SemanticTokensDelta {
                    result_id: Some(current_result_id),
                    edits: Vec::new(),
                },
            )));
        }

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let current_file = normalize_path(&uri_to_path(uri));
        let project_index = self.project_index.read().await;
        let tokens = semantic::semantic_tokens(
            &source,
            Some(current_file.as_path()),
            &get_source,
            project_index.resolver(),
        );

        Ok(Some(SemanticTokensFullDeltaResult::Tokens(
            SemanticTokens {
                result_id: Some(current_result_id),
                data: tokens,
            },
        )))
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let current_file = normalize_path(&uri_to_path(uri));
        let project_index = self.project_index.read().await;
        let start = convert::position_to_offset(&source, params.range.start);
        let end = convert::position_to_offset(&source, params.range.end);
        let tokens = semantic::semantic_tokens_in_range(
            &source,
            start..end,
            Some(current_file.as_path()),
            &get_source,
            project_index.resolver(),
        );

        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        // Collect open document contents for cross-file lookups.
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            // Check open documents first, then fall back to disk.
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let project_index = self.project_index.read().await;

        Ok(definition::goto_definition(
            &source,
            &params.text_document_position_params.position,
            uri,
            &get_source,
            project_index.resolver(),
        ))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let path = normalize_path(&uri_to_path(uri));
        let project_index = self.project_index.read().await;
        let references = project_index.find_references(
            &path,
            &source,
            params.text_document_position.position,
            params.context.include_declaration,
            &get_source,
        );

        if references.is_empty() {
            Ok(None)
        } else {
            Ok(Some(references))
        }
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };
        let source = doc.content.clone();
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let path = normalize_path(&uri_to_path(uri));
        let project_index = self.project_index.read().await;
        let Some(plan) = project_index.rename_plan(&path, &source, params.position, &get_source)
        else {
            return Ok(None);
        };

        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: plan.range,
            placeholder: plan.placeholder,
        }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        if !is_valid_solidity_identifier(&params.new_name) {
            return Ok(None);
        }

        let uri = &params.text_document_position.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };
        let source = doc.content.clone();
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let path = normalize_path(&uri_to_path(uri));
        let project_index = self.project_index.read().await;
        let Some(plan) = project_index.rename_plan(
            &path,
            &source,
            params.text_document_position.position,
            &get_source,
        ) else {
            return Ok(None);
        };

        let mut changes = HashMap::new();
        for location in plan.locations {
            changes
                .entry(location.uri)
                .or_insert_with(Vec::new)
                .push(TextEdit {
                    range: location.range,
                    new_text: params.new_name.clone(),
                });
        }

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };
        let source = doc.content.clone();
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let path = normalize_path(&uri_to_path(uri));
        let project_index = self.project_index.read().await;
        let Some(entry) = project_index.prepare_call_hierarchy(
            &path,
            &source,
            params.text_document_position_params.position,
            &get_source,
        ) else {
            return Ok(None);
        };

        Ok(Some(vec![call_hierarchy_item(entry)]))
    }

    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        let Some(data) = call_hierarchy_item_data(&params.item) else {
            return Ok(None);
        };
        if !is_solidity_file(&data.uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let path = normalize_path(&uri_to_path(&data.uri));
        let Some(source) = open_docs
            .get(&path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&path).ok())
        else {
            return Ok(None);
        };

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let project_index = self.project_index.read().await;
        let calls =
            project_index.incoming_call_hierarchy(&path, &source, data.target_offset, &get_source);

        if calls.is_empty() {
            Ok(None)
        } else {
            Ok(Some(
                calls
                    .into_iter()
                    .map(|call| CallHierarchyIncomingCall {
                        from: call_hierarchy_item(call.from),
                        from_ranges: call.from_ranges,
                    })
                    .collect(),
            ))
        }
    }

    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        let Some(data) = call_hierarchy_item_data(&params.item) else {
            return Ok(None);
        };
        if !is_solidity_file(&data.uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let path = normalize_path(&uri_to_path(&data.uri));
        let Some(source) = open_docs
            .get(&path)
            .cloned()
            .or_else(|| std::fs::read_to_string(&path).ok())
        else {
            return Ok(None);
        };

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };
        let project_index = self.project_index.read().await;
        let calls =
            project_index.outgoing_call_hierarchy(&path, &source, data.target_offset, &get_source);

        if calls.is_empty() {
            Ok(None)
        } else {
            Ok(Some(
                calls
                    .into_iter()
                    .map(|call| CallHierarchyOutgoingCall {
                        to: call_hierarchy_item(call.to),
                        from_ranges: call.from_ranges,
                    })
                    .collect(),
            ))
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };
        let source = doc.content.clone();
        drop(documents);

        let path = normalize_path(&uri_to_path(uri));
        Ok(self
            .project_index
            .read()
            .await
            .document_symbols(&path, &source))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<WorkspaceSymbolResponse>> {
        let result = self
            .project_index
            .read()
            .await
            .workspace_symbols(&params.query);
        Ok(Some(result))
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };
        let source = doc.content.clone();
        drop(documents);

        let path = normalize_path(&uri_to_path(uri));
        let links = self
            .project_index
            .read()
            .await
            .document_links(&path, &source);
        if links.is_empty() {
            Ok(None)
        } else {
            Ok(Some(links))
        }
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;
        if !is_solidity_file(uri) {
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };
        let source = doc.content.clone();
        let open_docs: std::collections::HashMap<std::path::PathBuf, String> = documents
            .uris()
            .filter_map(|u| {
                let path = normalize_path(&uri_to_path(u));
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(&normalize_path(path)) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };

        let path = normalize_path(&uri_to_path(uri));
        let project_index = self.project_index.read().await;
        let mut lenses = if self.project_index_ready.load(Ordering::SeqCst) {
            project_index.code_lenses(&path, &source, &get_source)
        } else {
            Vec::new()
        };
        lenses.extend(
            project_index
                .graph_lenses(&path, &source)
                .into_iter()
                .map(|spec| graph_code_lens(spec, uri)),
        );
        if lenses.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lenses))
        }
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let explicit_config_path = self.config_path.read().await.clone();
        let changes = params.changes;
        if changes.iter().any(|change| {
            let path = uri_to_path(&change.uri);
            is_solidity_path(&path)
                || is_config_refresh_path(&path, explicit_config_path.as_deref())
                || is_workspace_state_refresh_path(&path)
        }) {
            self.project_disk_generation.fetch_add(1, Ordering::SeqCst);
        }
        let mut rerun_workspace_analysis = false;
        let mut affected_paths = Vec::new();
        let mut deleted_paths = Vec::new();
        let open_paths = self
            .collect_open_document_overlays()
            .await
            .into_keys()
            .collect::<HashSet<_>>();

        {
            let mut project_index = self.project_index.write().await;
            for change in changes {
                let path = normalize_path(&uri_to_path(&change.uri));
                if is_config_refresh_path(&path, explicit_config_path.as_deref()) {
                    rerun_workspace_analysis = true;
                }
                if is_workspace_state_refresh_path(&path) {
                    rerun_workspace_analysis = true;
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) == Some("sol") {
                    if !open_paths.contains(&path) {
                        match change.typ {
                            FileChangeType::DELETED => {
                                project_index.remove_file(&path);
                                deleted_paths.push(path.clone());
                            }
                            _ => project_index.sync_closed_file(&path),
                        }
                    }
                    affected_paths.extend(project_index.transitive_import_dependents(&path));
                    affected_paths.push(path);
                }
            }
        }

        let analysis_generation = (!affected_paths.is_empty())
            .then(|| self.project_generation.fetch_add(1, Ordering::SeqCst) + 1);

        if rerun_workspace_analysis {
            self.rerun_workspace_analysis().await;
            return;
        }
        for path in deleted_paths {
            if let Some(uri) = self
                .published_uri_for_path(&path)
                .await
                .or_else(|| path_to_uri_option(&path))
            {
                self.clear_published_diagnostics(uri).await;
            }
        }
        if let Some(generation) = analysis_generation {
            let _ = self.client.semantic_tokens_refresh().await;
            self.relint_paths(&affected_paths, generation).await;
        }
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        let previous_config_path = self.config_path.read().await.clone();
        let mut rerun_workspace_analysis = false;

        // Try to extract settings from the notification
        if let Ok(settings) = serde_json::from_value::<ClientSettings>(params.settings) {
            let mut server_settings = self.settings.write().await;
            server_settings.fix_on_save = settings.fix_on_save.unwrap_or(true);
            server_settings.fix_on_save_unsafe = settings.fix_on_save_unsafe.unwrap_or(false);
            server_settings.format_on_save = settings.format_on_save.unwrap_or(true);
            drop(server_settings);

            let workspace_root = self.workspace_root.read().await.clone();
            let config_path = settings
                .config_path
                .map(|config_path| resolve_config_path(config_path, workspace_root.as_deref()));
            rerun_workspace_analysis = config_path != previous_config_path;
            let mut config_path_slot = self.config_path.write().await;
            *config_path_slot = config_path;
        }

        if rerun_workspace_analysis {
            self.rerun_workspace_analysis().await;
        } else {
            self.clear_config_cache().await;
            self.relint_open_documents().await;
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            RERUN_SECURITY_ANALYSIS_COMMAND => {
                let summary = self.rerun_workspace_analysis().await;
                Ok(Some(json!(summary)))
            }
            IMPORTS_GRAPH_COMMAND => {
                let args = first_command_arg::<GraphCommandArgs>(&params.arguments)?;
                Ok(self
                    .graph_document(GraphKind::Imports, &args)
                    .await
                    .map(|graph| json!(graph)))
            }
            INHERITANCE_GRAPH_COMMAND => {
                let args = first_command_arg::<GraphCommandArgs>(&params.arguments)?;
                Ok(self
                    .graph_document(GraphKind::Inheritance, &args)
                    .await
                    .map(|graph| json!(graph)))
            }
            LINEARIZED_INHERITANCE_GRAPH_COMMAND => {
                let args = first_command_arg::<GraphCommandArgs>(&params.arguments)?;
                Ok(self
                    .graph_document(GraphKind::LinearizedInheritance, &args)
                    .await
                    .map(|graph| json!(graph)))
            }
            CONTROL_FLOW_GRAPH_COMMAND => {
                let args = first_command_arg::<GraphCommandArgs>(&params.arguments)?;
                Ok(self
                    .graph_document(GraphKind::ControlFlow, &args)
                    .await
                    .map(|graph| json!(graph)))
            }
            _ => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
enum CachedConfigEntry {
    Loaded(Arc<Config>),
    Failed(Arc<str>),
}

impl CachedConfigEntry {
    fn cloned_result(&self) -> std::result::Result<Arc<Config>, String> {
        match self {
            Self::Loaded(config) => Ok(config.clone()),
            Self::Failed(error) => Err(error.to_string()),
        }
    }

    fn into_result(self) -> std::result::Result<Arc<Config>, String> {
        self.cloned_result()
    }
}

fn load_explicit_config_entry(
    path: &std::path::Path,
) -> std::result::Result<CachedConfigEntry, String> {
    solgrid_config::load_config(path).map(|config| CachedConfigEntry::Loaded(Arc::new(config)))
}

fn load_nearest_config_entry(
    path: &std::path::Path,
) -> std::result::Result<CachedConfigEntry, String> {
    solgrid_config::resolve_config(path).map(|config| CachedConfigEntry::Loaded(Arc::new(config)))
}

/// Client settings sent via didChangeConfiguration.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientSettings {
    fix_on_save: Option<bool>,
    fix_on_save_unsafe: Option<bool>,
    format_on_save: Option<bool>,
    config_path: Option<String>,
}

fn first_command_arg<T>(arguments: &[serde_json::Value]) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    arguments
        .first()
        .cloned()
        .ok_or_else(|| {
            tower_lsp_server::jsonrpc::Error::invalid_params("missing command arguments")
        })
        .and_then(|value| {
            serde_json::from_value(value).map_err(|error| {
                tower_lsp_server::jsonrpc::Error::invalid_params(error.to_string())
            })
        })
}

/// Check if a URI points to a Solidity file.
fn is_solidity_file(uri: &Uri) -> bool {
    uri.as_str().ends_with(".sol")
}

fn is_solidity_path(path: &std::path::Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("sol")
}

fn graph_code_lens(spec: GraphLensSpec, uri: &Uri) -> CodeLens {
    CodeLens {
        range: spec.range,
        command: Some(Command {
            title: spec.title,
            command: "solgrid.graph.show".to_string(),
            arguments: Some(vec![json!({
                "kind": spec.kind,
                "uri": uri,
                "symbolName": spec.symbol_name,
                "targetOffset": spec.target_offset,
            })]),
        }),
        data: None,
    }
}

async fn send_project_index_status(
    client: &Client,
    state: &'static str,
    files: usize,
    duration_ms: Option<u128>,
) {
    client
        .send_notification::<ProjectIndexStatusNotification>(ProjectIndexStatusParams {
            state: state.to_string(),
            files,
            duration_ms,
        })
        .await;
}

fn call_hierarchy_item(entry: CallHierarchyEntry) -> CallHierarchyItem {
    let uri = path_to_uri_option(&entry.path)
        .expect("call hierarchy entries always originate from filesystem paths");
    let data = CallHierarchyItemData {
        uri: uri.clone(),
        target_offset: entry.target_offset,
    };
    CallHierarchyItem {
        name: entry.name,
        kind: entry.kind,
        tags: None,
        detail: entry.detail,
        uri,
        range: entry.range,
        selection_range: entry.selection_range,
        data: Some(json!(data)),
    }
}

fn call_hierarchy_item_data(item: &CallHierarchyItem) -> Option<CallHierarchyItemData> {
    serde_json::from_value(item.data.clone()?).ok()
}

fn detector_inlay_hints(
    snapshot: &ProjectSnapshot,
    diagnostics: &[Diagnostic],
    start: usize,
    end: usize,
) -> Vec<DetectorInlayHint> {
    let mut grouped: HashMap<DetectorHintAnchor, Vec<DetectorHintFinding>> = HashMap::new();

    for diagnostic in diagnostics {
        let Some(meta) = finding_meta_from_diagnostic(diagnostic) else {
            continue;
        };
        if meta.kind != FindingKind::Detector {
            continue;
        }

        let offset = convert::position_to_offset(&snapshot.source, diagnostic.range.start);
        let Some(anchor) = detector_hint_anchor(snapshot, offset) else {
            continue;
        };
        grouped
            .entry(anchor)
            .or_default()
            .push(DetectorHintFinding {
                meta,
                message: diagnostic.message.clone(),
            });
    }

    let mut hints = grouped
        .into_iter()
        .filter_map(|(anchor, mut findings)| {
            if anchor.offset < start || anchor.offset > end {
                return None;
            }
            findings.sort_by(detector_hint_finding_sort_key);
            Some(DetectorInlayHint {
                offset: anchor.offset,
                label: detector_hint_label(&findings),
                tooltip: detector_hint_tooltip(&anchor, &findings),
            })
        })
        .collect::<Vec<_>>();
    hints.sort_by_key(|hint| hint.offset);
    hints
}

fn finding_meta_from_diagnostic(diagnostic: &Diagnostic) -> Option<FindingMeta> {
    serde_json::from_value(diagnostic.data.clone()?).ok()
}

fn detector_hint_anchor(snapshot: &ProjectSnapshot, offset: usize) -> Option<DetectorHintAnchor> {
    let def = snapshot.table.find_enclosing_declaration(offset)?;
    Some(DetectorHintAnchor {
        name: def.name.clone(),
        kind_label: detector_anchor_kind_label(def.kind).to_string(),
        offset: declaration_hint_offset(&snapshot.source, &def.def_span),
        def_span: def.def_span.clone(),
    })
}

fn detector_hint_finding_sort_key(
    left: &DetectorHintFinding,
    right: &DetectorHintFinding,
) -> std::cmp::Ordering {
    severity_rank(right.meta.severity)
        .cmp(&severity_rank(left.meta.severity))
        .then_with(|| left.meta.category.cmp(&right.meta.category))
        .then_with(|| left.meta.title.cmp(&right.meta.title))
        .then_with(|| left.message.cmp(&right.message))
}

fn detector_hint_label(findings: &[DetectorHintFinding]) -> String {
    let summary = detector_hint_summary(findings);
    let count = findings.len();
    if count == 1 {
        let finding = &findings[0];
        return format!("{summary}: {}", finding.meta.title);
    }

    format!("{summary}: {count} findings")
}

fn detector_hint_tooltip(anchor: &DetectorHintAnchor, findings: &[DetectorHintFinding]) -> String {
    let mut lines = vec![format!(
        "Detector findings for {} {}:",
        anchor.kind_label, anchor.name
    )];

    for finding in findings {
        let confidence = finding
            .meta
            .confidence
            .map(|value| format!(", {} confidence", format_confidence(value)))
            .unwrap_or_default();
        lines.push(format!(
            "- {} {}{}: {}",
            format_severity(finding.meta.severity),
            display_category(&finding.meta.category),
            confidence,
            finding.meta.title
        ));
        if finding.message != finding.meta.title {
            lines.push(format!("  {}", finding.message));
        }
        if let Some(help_url) = &finding.meta.help_url {
            lines.push(format!("  Docs: {help_url}"));
        }
    }

    lines.join("\n")
}

fn detector_anchor_kind_label(kind: solgrid_ast::symbols::SymbolKind) -> &'static str {
    match kind {
        solgrid_ast::symbols::SymbolKind::Contract => "contract",
        solgrid_ast::symbols::SymbolKind::Interface => "interface",
        solgrid_ast::symbols::SymbolKind::Library => "library",
        solgrid_ast::symbols::SymbolKind::Constructor => "constructor",
        solgrid_ast::symbols::SymbolKind::Function => "function",
        solgrid_ast::symbols::SymbolKind::Modifier => "modifier",
        solgrid_ast::symbols::SymbolKind::Event => "event",
        solgrid_ast::symbols::SymbolKind::Error => "error",
        solgrid_ast::symbols::SymbolKind::Struct => "struct",
        solgrid_ast::symbols::SymbolKind::Enum => "enum",
        solgrid_ast::symbols::SymbolKind::Udvt => "type",
        solgrid_ast::symbols::SymbolKind::StateVariable => "state variable",
        solgrid_ast::symbols::SymbolKind::StructField
        | solgrid_ast::symbols::SymbolKind::Parameter
        | solgrid_ast::symbols::SymbolKind::ReturnParameter
        | solgrid_ast::symbols::SymbolKind::LocalVariable
        | solgrid_ast::symbols::SymbolKind::EnumVariant => "declaration",
    }
}

fn declaration_hint_offset(source: &str, span: &std::ops::Range<usize>) -> usize {
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

fn display_category(category: &str) -> String {
    category.replace('-', " ")
}

fn detector_hint_summary(findings: &[DetectorHintFinding]) -> String {
    let category = detector_hint_summary_category(findings);
    let severity = findings
        .iter()
        .map(|finding| finding.meta.severity)
        .max_by_key(|severity| severity_rank(*severity))
        .map(format_severity)
        .unwrap_or("info");
    let confidence = findings
        .iter()
        .filter_map(|finding| finding.meta.confidence)
        .max_by_key(|confidence| confidence_rank(*confidence))
        .map(format_confidence)
        .unwrap_or("unknown");
    format!("{category} {severity}/{confidence}")
}

fn detector_hint_summary_category(findings: &[DetectorHintFinding]) -> String {
    let mut categories = findings
        .iter()
        .map(|finding| finding.meta.category.as_str())
        .collect::<Vec<_>>();
    categories.sort_unstable();
    categories.dedup();
    if categories.len() == 1 {
        display_category(categories[0])
    } else {
        "detectors".to_string()
    }
}

fn severity_rank(severity: FindingSeverity) -> u8 {
    match severity {
        FindingSeverity::Error => 3,
        FindingSeverity::Warning => 2,
        FindingSeverity::Info => 1,
    }
}

fn format_severity(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Error => "error",
        FindingSeverity::Warning => "warning",
        FindingSeverity::Info => "info",
    }
}

fn format_confidence(confidence: solgrid_diagnostics::Confidence) -> &'static str {
    match confidence {
        solgrid_diagnostics::Confidence::Low => "low",
        solgrid_diagnostics::Confidence::Medium => "medium",
        solgrid_diagnostics::Confidence::High => "high",
    }
}

fn confidence_rank(confidence: solgrid_diagnostics::Confidence) -> u8 {
    match confidence {
        solgrid_diagnostics::Confidence::Low => 1,
        solgrid_diagnostics::Confidence::Medium => 2,
        solgrid_diagnostics::Confidence::High => 3,
    }
}

fn is_valid_solidity_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && !is_solidity_reserved_identifier(name)
}

fn is_solidity_reserved_identifier(name: &str) -> bool {
    matches!(
        name,
        "abstract"
            | "after"
            | "alias"
            | "anonymous"
            | "apply"
            | "as"
            | "assembly"
            | "auto"
            | "break"
            | "byte"
            | "calldata"
            | "case"
            | "catch"
            | "constant"
            | "constructor"
            | "continue"
            | "contract"
            | "copyof"
            | "days"
            | "default"
            | "define"
            | "delete"
            | "do"
            | "else"
            | "emit"
            | "enum"
            | "error"
            | "ether"
            | "event"
            | "external"
            | "fallback"
            | "false"
            | "final"
            | "finney"
            | "for"
            | "from"
            | "function"
            | "gwei"
            | "hours"
            | "if"
            | "immutable"
            | "implements"
            | "import"
            | "in"
            | "indexed"
            | "inline"
            | "interface"
            | "internal"
            | "is"
            | "let"
            | "library"
            | "macro"
            | "mapping"
            | "match"
            | "memory"
            | "minutes"
            | "modifier"
            | "mutable"
            | "new"
            | "null"
            | "of"
            | "override"
            | "partial"
            | "payable"
            | "pragma"
            | "private"
            | "promise"
            | "public"
            | "pure"
            | "receive"
            | "reference"
            | "relocatable"
            | "return"
            | "returns"
            | "revert"
            | "sealed"
            | "seconds"
            | "sizeof"
            | "static"
            | "storage"
            | "struct"
            | "supports"
            | "switch"
            | "szabo"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "type"
            | "typedef"
            | "typeof"
            | "unchecked"
            | "unicode"
            | "using"
            | "var"
            | "view"
            | "virtual"
            | "weeks"
            | "wei"
            | "while"
            | "years"
    ) || is_solidity_elementary_type(name)
}

fn is_solidity_elementary_type(name: &str) -> bool {
    if matches!(
        name,
        "address" | "bool" | "string" | "bytes" | "fixed" | "ufixed"
    ) {
        return true;
    }
    if let Some(width) = name.strip_prefix("bytes") {
        return width
            .parse::<u8>()
            .is_ok_and(|width| (1..=32).contains(&width));
    }
    for prefix in ["uint", "int"] {
        if name == prefix {
            return true;
        }
        if let Some(width) = name.strip_prefix(prefix) {
            return width
                .parse::<u16>()
                .is_ok_and(|width| (8..=256).contains(&width) && width % 8 == 0);
        }
    }
    for prefix in ["fixed", "ufixed"] {
        let Some((integer_bits, fractional_digits)) = name
            .strip_prefix(prefix)
            .and_then(|suffix| suffix.split_once('x'))
        else {
            continue;
        };
        if integer_bits
            .parse::<u16>()
            .is_ok_and(|width| (8..=256).contains(&width) && width % 8 == 0)
            && fractional_digits
                .parse::<u8>()
                .is_ok_and(|digits| (1..=80).contains(&digits))
        {
            return true;
        }
    }
    false
}

/// Convert a URI to a filesystem path.
fn uri_to_path(uri: &Uri) -> PathBuf {
    uri.to_file_path()
        .map(|p| p.into_owned())
        .unwrap_or_else(|| PathBuf::from(uri.path().as_str()))
}

/// Try to convert a URI to a filesystem path.
fn uri_to_path_option(uri: &Uri) -> Option<PathBuf> {
    uri.to_file_path().map(|p| p.into_owned())
}

fn path_to_uri_option(path: &std::path::Path) -> Option<Uri> {
    Uri::from_file_path(path)
}

#[allow(clippy::too_many_arguments)]
fn workspace_analysis_input_is_current(
    documents: &DocumentStore,
    path: &std::path::Path,
    source: &str,
    version: Option<i32>,
    expected_project_generation: u64,
    current_project_generation: u64,
    expected_disk_generation: u64,
    current_disk_generation: u64,
) -> bool {
    if expected_project_generation != current_project_generation
        || expected_disk_generation != current_disk_generation
    {
        return false;
    }

    let path = normalize_path(path);
    let current_document = documents.uris().find_map(|uri| {
        (normalize_path(&uri_to_path(uri)) == path)
            .then(|| documents.get(uri))
            .flatten()
    });
    match (version, current_document) {
        (Some(version), Some(document)) => {
            document.version == version && document.content == source
        }
        (None, None) => std::fs::read_to_string(&path).is_ok_and(|current| current == source),
        (Some(_), None) | (None, Some(_)) => false,
    }
}

fn semantic_tokens_result_id(version: i32, project_generation: u64) -> String {
    format!("v{version}-p{project_generation}")
}

fn normalize_path(path: &std::path::Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn resolve_config_path(config_path: String, workspace_root: Option<&std::path::Path>) -> PathBuf {
    let config_path = PathBuf::from(config_path);
    if config_path.is_absolute() {
        config_path
    } else if let Some(root) = workspace_root {
        root.join(config_path)
    } else {
        config_path
    }
}

fn config_cache_key(path: &std::path::Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

fn is_config_refresh_path(
    path: &std::path::Path,
    explicit_config_path: Option<&std::path::Path>,
) -> bool {
    if explicit_config_path.is_some_and(|configured| configured == path) {
        return true;
    }

    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("solgrid.toml") | Some("foundry.toml")
    )
}

fn is_workspace_state_refresh_path(path: &std::path::Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("foundry.toml") | Some("remappings.txt")
    )
}

#[derive(Debug, Default)]
struct ServerConfigCache {
    explicit: Option<(PathBuf, CachedConfigEntry)>,
    nearest: HashMap<PathBuf, CachedConfigEntry>,
}

impl ServerConfigCache {
    fn explicit_config(&self, path: &std::path::Path) -> Option<CachedConfigEntry> {
        self.explicit
            .as_ref()
            .filter(|(cached_path, _)| cached_path == path)
            .map(|(_, config)| config.clone())
    }

    fn store_explicit(&mut self, path: PathBuf, config: CachedConfigEntry) {
        self.explicit = Some((path, config));
    }

    fn nearest_config(&self, dir: &std::path::Path) -> Option<CachedConfigEntry> {
        self.nearest.get(dir).cloned()
    }

    fn store_nearest(&mut self, dir: PathBuf, config: CachedConfigEntry) {
        self.nearest.insert(dir, config);
    }

    fn clear(&mut self) {
        self.explicit = None;
        self.nearest.clear();
    }
}

/// Compute the LSP range covering the entire document.
fn full_document_range(source: &str) -> Range {
    let end = convert::offset_to_position(source, source.len());
    Range {
        start: Position::new(0, 0),
        end,
    }
}

/// Run the LSP server on stdio.
pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = tower_lsp_server::LspService::new(SolgridServer::new);
    tower_lsp_server::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_solidity_file() {
        assert!(is_solidity_file(
            &"file:///test.sol".parse::<Uri>().unwrap()
        ));
        assert!(is_solidity_file(
            &"file:///path/to/Contract.sol".parse::<Uri>().unwrap()
        ));
        assert!(!is_solidity_file(
            &"file:///test.ts".parse::<Uri>().unwrap()
        ));
        assert!(!is_solidity_file(
            &"file:///test.rs".parse::<Uri>().unwrap()
        ));
    }

    #[test]
    fn test_uri_to_path() {
        let uri: Uri = "file:///home/user/test.sol".parse().unwrap();
        let path = uri_to_path(&uri);
        assert_eq!(path, PathBuf::from("/home/user/test.sol"));
    }

    #[cfg(unix)]
    #[test]
    fn test_normalize_path_unifies_symlinked_overlay_paths() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let source = real.join("Imported.sol");
        std::fs::write(&source, "contract Imported {}").unwrap();
        let alias = temp.path().join("alias");
        symlink(&real, &alias).unwrap();

        assert_eq!(
            normalize_path(&source),
            normalize_path(&alias.join("Imported.sol"))
        );
    }

    #[test]
    fn test_rename_identifier_rejects_solidity_reserved_words_and_types() {
        assert!(is_valid_solidity_identifier("newOwner"));
        assert!(is_valid_solidity_identifier("_value2"));
        assert!(!is_valid_solidity_identifier("contract"));
        assert!(!is_valid_solidity_identifier("address"));
        assert!(!is_valid_solidity_identifier("uint256"));
        assert!(!is_valid_solidity_identifier("bytes32"));
        assert!(!is_valid_solidity_identifier("8value"));
    }

    #[test]
    fn test_semantic_token_result_id_includes_project_generation() {
        assert_eq!(semantic_tokens_result_id(7, 11), "v7-p11");
        assert_ne!(
            semantic_tokens_result_id(7, 11),
            semantic_tokens_result_id(7, 12)
        );
    }

    #[test]
    fn test_workspace_analysis_rejects_stale_content_and_generations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Analysis.sol");
        std::fs::write(&path, "contract Before {}").unwrap();
        let uri = Uri::from_file_path(&path).unwrap();
        let mut documents = DocumentStore::new();
        documents.open(uri.clone(), "contract Before {}".into(), 4);

        assert!(workspace_analysis_input_is_current(
            &documents,
            &path,
            "contract Before {}",
            Some(4),
            10,
            10,
            3,
            3,
        ));

        // A save can replace content without incrementing the LSP version, so
        // version equality alone must not authorize a stale publication.
        documents.set_content(&uri, "contract After {}".into());
        assert!(!workspace_analysis_input_is_current(
            &documents,
            &path,
            "contract Before {}",
            Some(4),
            10,
            10,
            3,
            3,
        ));
        assert!(!workspace_analysis_input_is_current(
            &documents,
            &path,
            "contract After {}",
            Some(4),
            10,
            11,
            3,
            3,
        ));
        assert!(!workspace_analysis_input_is_current(
            &documents,
            &path,
            "contract After {}",
            Some(4),
            10,
            10,
            3,
            4,
        ));

        documents.close(&uri);
        std::fs::write(&path, "contract After {}").unwrap();
        assert!(workspace_analysis_input_is_current(
            &documents,
            &path,
            "contract After {}",
            None,
            11,
            11,
            4,
            4,
        ));

        // Do not rely on a watched-file notification having arrived before
        // authorizing a closed-file result.
        std::fs::write(&path, "contract ChangedAgain {}").unwrap();
        assert!(!workspace_analysis_input_is_current(
            &documents,
            &path,
            "contract After {}",
            None,
            11,
            11,
            4,
            4,
        ));
    }

    #[test]
    fn test_full_document_range() {
        let source = "line1\nline2\nline3";
        let range = full_document_range(source);
        assert_eq!(range.start, Position::new(0, 0));
        assert_eq!(range.end, Position::new(2, 5));
    }

    #[test]
    fn test_server_settings_default() {
        let settings = ServerSettings::default();
        assert!(settings.fix_on_save);
        assert!(!settings.fix_on_save_unsafe);
        assert!(settings.format_on_save);
    }

    #[test]
    fn test_config_refresh_path_matches_workspace_configs() {
        assert!(is_config_refresh_path(
            &PathBuf::from("/tmp/project/solgrid.toml"),
            None
        ));
        assert!(is_config_refresh_path(
            &PathBuf::from("/tmp/project/foundry.toml"),
            None
        ));
        assert!(!is_config_refresh_path(
            &PathBuf::from("/tmp/project/src/Token.sol"),
            None
        ));
    }

    #[test]
    fn test_config_refresh_path_matches_explicit_config_path() {
        let path = PathBuf::from("/tmp/project/config/custom.toml");
        assert!(is_config_refresh_path(&path, Some(path.as_path())));
    }

    #[test]
    fn test_workspace_state_refresh_path_matches_remapping_files() {
        assert!(is_workspace_state_refresh_path(&PathBuf::from(
            "/tmp/project/remappings.txt"
        )));
        assert!(is_workspace_state_refresh_path(&PathBuf::from(
            "/tmp/project/foundry.toml"
        )));
        assert!(!is_workspace_state_refresh_path(&PathBuf::from(
            "/tmp/project/solgrid.toml"
        )));
    }

    #[test]
    fn test_server_config_cache_can_store_and_clear() {
        let mut cache = ServerConfigCache::default();
        let explicit_path = PathBuf::from("/tmp/project/solgrid.toml");
        let nearest_path = PathBuf::from("/tmp/project/src");
        let config = CachedConfigEntry::Loaded(Arc::new(Config::default()));
        let error = CachedConfigEntry::Failed("invalid config".into());

        cache.store_explicit(explicit_path.clone(), config.clone());
        cache.store_nearest(nearest_path.clone(), error.clone());

        assert!(matches!(
            cache.explicit_config(&explicit_path),
            Some(CachedConfigEntry::Loaded(_))
        ));
        assert!(matches!(
            cache.nearest_config(&nearest_path),
            Some(CachedConfigEntry::Failed(_))
        ));

        cache.clear();

        assert!(cache.explicit_config(&explicit_path).is_none());
        assert!(cache.nearest_config(&nearest_path).is_none());
    }

    #[test]
    fn test_detector_hint_tooltip_summarizes_grouped_findings() {
        let anchor = DetectorHintAnchor {
            name: "route".into(),
            kind_label: "function".into(),
            offset: 0,
            def_span: 0..10,
        };
        let findings = vec![
            DetectorHintFinding {
                meta: FindingMeta {
                    id: "security/user-controlled-delegatecall".into(),
                    title: "User-controlled delegatecall target".into(),
                    category: "security".into(),
                    severity: FindingSeverity::Warning,
                    kind: FindingKind::Detector,
                    confidence: Some(solgrid_diagnostics::Confidence::High),
                    help_url: Some("https://example.test/delegatecall".into()),
                    suppressible: true,
                    has_fix: false,
                },
                message: "delegatecall target can be user controlled".into(),
            },
            DetectorHintFinding {
                meta: FindingMeta {
                    id: "security/unchecked-low-level-call".into(),
                    title: "Unchecked low-level call".into(),
                    category: "security".into(),
                    severity: FindingSeverity::Warning,
                    kind: FindingKind::Detector,
                    confidence: Some(solgrid_diagnostics::Confidence::High),
                    help_url: None,
                    suppressible: true,
                    has_fix: false,
                },
                message: "low-level call return value is ignored".into(),
            },
        ];

        assert_eq!(
            detector_hint_label(&findings),
            "security warning/high: 2 findings"
        );

        let tooltip = detector_hint_tooltip(&anchor, &findings);
        assert!(tooltip.contains("Detector findings for function route:"));
        assert!(tooltip.contains("User-controlled delegatecall target"));
        assert!(tooltip.contains("Unchecked low-level call"));
        assert!(tooltip.contains("Docs: https://example.test/delegatecall"));
    }

    #[test]
    fn test_load_explicit_config_entry_preserves_errors() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("solgrid.toml");
        std::fs::write(
            &config_path,
            r#"
[lint]
preset = "all"

[lint.settings."docs/not-a-rule"]
enabled = true
"#,
        )
        .unwrap();

        let error = load_explicit_config_entry(&config_path).unwrap_err();
        assert!(error.contains("unknown settings entry"));
        assert!(error.contains("docs/not-a-rule"));
    }

    #[test]
    fn test_load_nearest_config_entry_preserves_errors() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            dir.path().join("solgrid.toml"),
            r#"
[lint]
preset = "all"

[lint.settings."docs/not-a-rule"]
enabled = true
"#,
        )
        .unwrap();

        let error = load_nearest_config_entry(&src.join("Token.sol")).unwrap_err();
        assert!(error.contains("unknown settings entry"));
        assert!(error.contains("docs/not-a-rule"));
    }
}
