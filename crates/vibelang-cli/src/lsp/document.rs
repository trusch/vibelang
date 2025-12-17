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

    /// Get a specific line (0-indexed).
    pub fn line(&self, line_idx: usize) -> Option<String> {
        if line_idx < self.content.len_lines() {
            Some(self.content.line(line_idx).to_string())
        } else {
            None
        }
    }

    /// Get the number of lines.
    pub fn line_count(&self) -> usize {
        self.content.len_lines()
    }

    /// Convert a (line, character) position to a byte offset.
    pub fn offset_from_position(&self, line: usize, character: usize) -> Option<usize> {
        if line >= self.content.len_lines() {
            return None;
        }
        let line_start = self.content.line_to_byte(line);
        let line_text = self.content.line(line);

        // Convert character (UTF-16 code units) to byte offset
        let mut char_count = 0;
        let mut byte_offset = 0;
        for ch in line_text.chars() {
            if char_count >= character {
                break;
            }
            // UTF-16 code units: BMP chars are 1, others are 2
            char_count += ch.len_utf16();
            byte_offset += ch.len_utf8();
        }

        Some(line_start + byte_offset)
    }

    /// Convert a byte offset to a (line, character) position.
    pub fn position_from_offset(&self, offset: usize) -> (usize, usize) {
        let line = self.content.byte_to_line(offset);
        let line_start = self.content.line_to_byte(line);
        let line_text = self.content.line(line);

        // Convert byte offset within line to UTF-16 character offset
        let byte_in_line = offset - line_start;
        let mut char_count = 0;
        let mut byte_count = 0;
        for ch in line_text.chars() {
            if byte_count >= byte_in_line {
                break;
            }
            char_count += ch.len_utf16();
            byte_count += ch.len_utf8();
        }

        (line, char_count)
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

    /// Check if a document is open.
    pub fn contains(&self, uri: &Url) -> bool {
        self.documents.contains_key(uri)
    }
}
