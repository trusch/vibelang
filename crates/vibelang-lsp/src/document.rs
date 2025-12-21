//! Document management for the LSP server.
//!
//! Tracks open documents and their content using rope data structures
//! for efficient text manipulation.

use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::lsp_types::Url;

/// A document being edited.
#[derive(Debug, Clone)]
pub struct Document {
    /// The document content as a rope for efficient editing.
    pub content: Rope,
    /// The document version.
    pub version: i32,
}

impl Document {
    /// Create a new document with the given content.
    pub fn new(content: &str, version: i32) -> Self {
        Self {
            content: Rope::from_str(content),
            version,
        }
    }

    /// Get the full text of the document.
    pub fn text(&self) -> String {
        self.content.to_string()
    }

}

/// Document store for managing all open documents.
#[derive(Debug, Default)]
pub struct DocumentStore {
    documents: DashMap<Url, Document>,
}

impl DocumentStore {
    /// Create a new document store.
    pub fn new() -> Self {
        Self {
            documents: DashMap::new(),
        }
    }

    /// Open a document.
    pub fn open(&self, uri: Url, content: &str, version: i32) {
        self.documents.insert(uri, Document::new(content, version));
    }

    /// Update a document.
    pub fn update(&self, uri: &Url, content: &str, version: i32) {
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.content = Rope::from_str(content);
            doc.version = version;
        }
    }

    /// Close a document.
    pub fn close(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    /// Get a document.
    pub fn get(&self, uri: &Url) -> Option<Document> {
        self.documents.get(uri).map(|doc| doc.clone())
    }
}
