//! Document store for managing open files.

use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::lsp_types::Url;

/// A document in the store.
#[derive(Debug)]
pub struct Document {
    /// The document content as a rope.
    rope: Rope,
    /// The document version.
    pub version: i32,
}

impl Document {
    /// Create a new document.
    pub fn new(text: &str, version: i32) -> Self {
        Self {
            rope: Rope::from_str(text),
            version,
        }
    }

    /// Get the document text.
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Update the document content.
    pub fn update(&mut self, text: &str, version: i32) {
        self.rope = Rope::from_str(text);
        self.version = version;
    }

    /// Get the number of lines.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Get a specific line.
    pub fn line(&self, line_idx: usize) -> Option<String> {
        if line_idx < self.rope.len_lines() {
            Some(self.rope.line(line_idx).to_string())
        } else {
            None
        }
    }

    /// Get the character offset for a line/column position.
    pub fn offset_at(&self, line: usize, col: usize) -> Option<usize> {
        if line >= self.rope.len_lines() {
            return None;
        }
        let line_start = self.rope.line_to_char(line);
        Some(line_start + col)
    }
}

/// Document store for managing open files.
#[derive(Debug, Default)]
pub struct DocumentStore {
    documents: DashMap<Url, Document>,
}

impl DocumentStore {
    /// Create a new document store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a document.
    pub fn open(&self, uri: Url, text: &str, version: i32) {
        self.documents.insert(uri, Document::new(text, version));
    }

    /// Update a document.
    pub fn update(&self, uri: &Url, text: &str, version: i32) {
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.update(text, version);
        }
    }

    /// Close a document.
    pub fn close(&self, uri: &Url) {
        self.documents.remove(uri);
    }

    /// Get a document.
    pub fn get(&self, uri: &Url) -> Option<dashmap::mapref::one::Ref<'_, Url, Document>> {
        self.documents.get(uri)
    }

    /// Check if a document is open.
    pub fn contains(&self, uri: &Url) -> bool {
        self.documents.contains_key(uri)
    }

    /// Get all open document URIs.
    pub fn uris(&self) -> Vec<Url> {
        self.documents.iter().map(|r| r.key().clone()).collect()
    }
}
