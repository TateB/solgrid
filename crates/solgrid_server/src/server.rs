//! LSP server — main server implementation using tower-lsp-server.

use crate::document::DocumentStore;
use crate::resolve::ImportResolver;
use crate::{actions, completion, convert, definition, diagnostics, format, hover};
use solgrid_config::Config;
use solgrid_linter::LintEngine;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
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

/// The solgrid LSP server.
pub struct SolgridServer {
    client: Client,
    engine: Arc<LintEngine>,
    documents: Arc<RwLock<DocumentStore>>,
    config: Arc<RwLock<Config>>,
    settings: Arc<RwLock<ServerSettings>>,
    /// Cache of last-published LSP diagnostics per URI, for hover lookups.
    published_diagnostics: Arc<RwLock<std::collections::HashMap<Uri, Vec<Diagnostic>>>>,
    /// Import path resolver for cross-file go-to-definition.
    resolver: Arc<RwLock<ImportResolver>>,
}

impl SolgridServer {
    /// Create a new server instance connected to the given LSP client.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            engine: Arc::new(LintEngine::new()),
            documents: Arc::new(RwLock::new(DocumentStore::new())),
            config: Arc::new(RwLock::new(Config::default())),
            settings: Arc::new(RwLock::new(ServerSettings::default())),
            published_diagnostics: Arc::new(RwLock::new(std::collections::HashMap::new())),
            resolver: Arc::new(RwLock::new(ImportResolver::new(None))),
        }
    }

    /// Lint a document and publish diagnostics to the client.
    async fn lint_and_publish(&self, uri: &Uri) {
        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return,
        };

        let source = doc.content.clone();
        let version = doc.version;
        drop(documents);

        let path = uri_to_path(uri);
        let config = self.config.read().await.clone();

        let mut lsp_diags =
            diagnostics::lint_to_lsp_diagnostics(&self.engine, &source, &path, &config);

        let resolver = self.resolver.read().await;
        lsp_diags.extend(diagnostics::unresolved_import_diagnostics(
            &source, &path, &resolver,
        ));
        drop(resolver);

        // Cache the diagnostics for hover lookups
        {
            let mut cache: tokio::sync::RwLockWriteGuard<
                '_,
                std::collections::HashMap<Uri, Vec<Diagnostic>>,
            > = self.published_diagnostics.write().await;
            cache.insert(uri.clone(), lsp_diags.clone());
        }

        self.client
            .publish_diagnostics(uri.clone(), lsp_diags, Some(version))
            .await;
    }

    /// Apply fix-on-save and/or format-on-save edits.
    async fn on_save_actions(&self, uri: &Uri) {
        let settings = self.settings.read().await.clone();
        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return,
        };

        let source = doc.content.clone();
        drop(documents);

        let path = uri_to_path(uri);
        let config = self.config.read().await.clone();
        let mut current_source = source;

        // Apply safe fixes
        if settings.fix_on_save {
            let (fixed, _remaining) = self.engine.fix_source(
                &current_source,
                &path,
                &config,
                settings.fix_on_save_unsafe,
            );
            current_source = fixed;
        }

        // Apply formatting
        if settings.format_on_save {
            if let Ok(formatted) = solgrid_formatter::format_source(&current_source, &config.format)
            {
                current_source = formatted;
            }
        }

        // If the source changed, the client should apply the edits and
        // the next did_change will re-lint. The LSP doesn't directly support
        // server-initiated edits on save via textDocument/didSave, so the
        // client extension handles this by watching for will_save_wait_until.
        // We store the result for the will_save handler.
        let _ = current_source;
    }
}

impl LanguageServer for SolgridServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Try to load config from the workspace root
        let root_uri = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .map(|f| &f.uri);
        #[allow(deprecated)]
        let root_uri = root_uri.or(params.root_uri.as_ref());
        if let Some(root_uri) = root_uri {
            if let Some(root_path) = uri_to_path_option(root_uri) {
                let resolved = solgrid_config::resolve_config(&root_path);
                let mut config = self.config.write().await;
                *config = resolved;

                // Initialize import resolver with workspace root.
                let mut resolver = self.resolver.write().await;
                *resolver = ImportResolver::new(Some(root_path));
            }
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
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["/".into(), " ".into()]),
                    ..Default::default()
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
        }

        self.lint_and_publish(&uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        if !is_solidity_file(&uri) {
            return;
        }
        // Trigger on-save actions (fix + format)
        self.on_save_actions(&uri).await;
        // Re-lint after save
        self.lint_and_publish(&uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        {
            let mut documents = self.documents.write().await;
            documents.close(&uri);
        }
        // Clear diagnostics for closed files
        {
            let mut cache: tokio::sync::RwLockWriteGuard<
                '_,
                std::collections::HashMap<Uri, Vec<Diagnostic>>,
            > = self.published_diagnostics.write().await;
            cache.remove(&uri);
        }
        self.client.publish_diagnostics(uri, vec![], None).await;
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
            return Ok(None);
        }

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        drop(documents);

        let path = uri_to_path(uri);
        let config = self.config.read().await.clone();
        let mut current_source = source.clone();
        let mut edits = Vec::new();

        // Apply safe fixes
        if settings.fix_on_save {
            let fix_edits = actions::safe_fix_edits(&self.engine, &current_source, &path, &config);
            if !fix_edits.is_empty() {
                // Apply the fixes to get the intermediate source
                let (fixed, _) = self.engine.fix_source(
                    &current_source,
                    &path,
                    &config,
                    settings.fix_on_save_unsafe,
                );
                current_source = fixed;
                edits.extend(fix_edits);
            }
        }

        // Apply formatting
        if settings.format_on_save {
            let format_edits = format::format_document(&current_source, &config.format);
            edits.extend(format_edits);
        }

        if edits.is_empty() {
            Ok(None)
        } else {
            // Since we may have both fix and format edits that can conflict,
            // produce a single full-document replacement for correctness
            let mut final_source = source.clone();

            if settings.fix_on_save {
                let (fixed, _) = self.engine.fix_source(
                    &final_source,
                    &path,
                    &config,
                    settings.fix_on_save_unsafe,
                );
                final_source = fixed;
            }

            if settings.format_on_save {
                if let Ok(formatted) =
                    solgrid_formatter::format_source(&final_source, &config.format)
                {
                    final_source = formatted;
                }
            }

            if final_source == source {
                Ok(None)
            } else {
                Ok(Some(vec![TextEdit {
                    range: full_document_range(&source),
                    new_text: final_source,
                }]))
            }
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

        let path = uri_to_path(uri);
        let config = self.config.read().await.clone();

        let result =
            actions::code_actions(&self.engine, &source, &path, &config, &params.range, uri);

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

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        drop(documents);

        let config = self.config.read().await.clone();
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

        let documents = self.documents.read().await;
        let doc = match documents.get(uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let source = doc.content.clone();
        drop(documents);

        let config = self.config.read().await.clone();
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
                let path = uri_to_path(u);
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let source = source.unwrap_or_default();

        let resolver = self.resolver.read().await;
        let get_source = |path: &std::path::Path| -> Option<String> {
            if let Some(content) = open_docs.get(path) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };

        Ok(hover::hover_at_position(
            &self.engine,
            &lsp_diags,
            position,
            &source,
            uri,
            &get_source,
            &resolver,
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
        drop(documents);

        let position = &params.text_document_position.position;
        let items = completion::suppression_completions(&self.engine, &source, position);

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
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
                let path = uri_to_path(u);
                let content = documents.get(u).map(|d| d.content.clone())?;
                Some((path, content))
            })
            .collect();
        drop(documents);

        let resolver = self.resolver.read().await;
        let get_source = |path: &std::path::Path| -> Option<String> {
            // Check open documents first, then fall back to disk.
            if let Some(content) = open_docs.get(path) {
                return Some(content.clone());
            }
            std::fs::read_to_string(path).ok()
        };

        Ok(definition::goto_definition(
            &source,
            &params.text_document_position_params.position,
            uri,
            &get_source,
            &resolver,
        ))
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        // Try to extract settings from the notification
        if let Ok(settings) = serde_json::from_value::<ClientSettings>(params.settings) {
            let mut server_settings = self.settings.write().await;
            server_settings.fix_on_save = settings.fix_on_save.unwrap_or(true);
            server_settings.fix_on_save_unsafe = settings.fix_on_save_unsafe.unwrap_or(false);
            server_settings.format_on_save = settings.format_on_save.unwrap_or(true);
        }

        // Reload config from disk
        // Try workspace root first (from initialize), fallback to current dir
        let config = solgrid_config::resolve_config(&std::env::current_dir().unwrap_or_default());
        {
            let mut cfg = self.config.write().await;
            *cfg = config;
        }

        // Re-lint all open documents with new config
        let uris: Vec<Uri> = {
            let documents = self.documents.read().await;
            documents.uris().cloned().collect()
        };

        for uri in uris {
            self.lint_and_publish(&uri).await;
        }
    }
}

/// Client settings sent via didChangeConfiguration.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientSettings {
    fix_on_save: Option<bool>,
    fix_on_save_unsafe: Option<bool>,
    format_on_save: Option<bool>,
}

/// Check if a URI points to a Solidity file.
fn is_solidity_file(uri: &Uri) -> bool {
    uri.as_str().ends_with(".sol")
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
}
