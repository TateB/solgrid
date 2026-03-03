//! Document store — tracks open files and their content.

use std::collections::HashMap;
use tower_lsp_server::ls_types::Uri;

/// A tracked document with its content and version.
#[derive(Debug, Clone)]
pub struct Document {
    /// The document URI.
    pub uri: Uri,
    /// Current source text.
    pub content: String,
    /// LSP version number.
    pub version: i32,
}

/// In-memory store for open documents.
#[derive(Debug, Default)]
pub struct DocumentStore {
    documents: HashMap<Uri, Document>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open or update a document.
    pub fn open(&mut self, uri: Uri, content: String, version: i32) {
        self.documents.insert(
            uri.clone(),
            Document {
                uri,
                content,
                version,
            },
        );
    }

    /// Update a document's content (full sync).
    pub fn update(&mut self, uri: &Uri, content: String, version: i32) {
        if let Some(doc) = self.documents.get_mut(uri) {
            doc.content = content;
            doc.version = version;
        }
    }

    /// Close a document.
    pub fn close(&mut self, uri: &Uri) {
        self.documents.remove(uri);
    }

    /// Get a document by URI.
    pub fn get(&self, uri: &Uri) -> Option<&Document> {
        self.documents.get(uri)
    }

    /// Get all open document URIs.
    pub fn uris(&self) -> impl Iterator<Item = &Uri> {
        self.documents.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_store_open_get_close() {
        let mut store = DocumentStore::new();
        let uri: Uri = "file:///test.sol".parse().unwrap();

        store.open(uri.clone(), "contract Test {}".into(), 1);
        assert!(store.get(&uri).is_some());
        assert_eq!(store.get(&uri).unwrap().content, "contract Test {}");
        assert_eq!(store.get(&uri).unwrap().version, 1);

        store.close(&uri);
        assert!(store.get(&uri).is_none());
    }

    #[test]
    fn test_document_store_update() {
        let mut store = DocumentStore::new();
        let uri: Uri = "file:///test.sol".parse().unwrap();

        store.open(uri.clone(), "v1".into(), 1);
        store.update(&uri, "v2".into(), 2);

        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.content, "v2");
        assert_eq!(doc.version, 2);
    }
}
