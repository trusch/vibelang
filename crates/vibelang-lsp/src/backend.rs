//! LSP backend implementation for VibeLang.
//!
//! This module implements the Language Server Protocol handler
//! using tower-lsp.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::analysis::{analyze_document, get_completion_context, get_word_at_position, AnalysisResult};
use crate::completion::get_completions;
use crate::definition::{get_import_definition, get_variable_definition};
use crate::diagnostics::all_diagnostics;
use crate::document::DocumentStore;
use crate::hover::{get_hover, SynthdefInfo};

/// VibeLang Language Server.
pub struct VibeLangServer {
    /// The LSP client connection.
    client: Client,
    /// Document store for open files.
    documents: DocumentStore,
    /// Known synthdef names (from stdlib imports).
    known_synthdefs: RwLock<HashSet<String>>,
    /// Known effect names (from stdlib imports).
    known_effects: RwLock<HashSet<String>>,
    /// Import search paths.
    import_paths: RwLock<Vec<PathBuf>>,
    /// Workspace root path.
    workspace_root: RwLock<Option<PathBuf>>,
    /// Cached analysis results per document.
    analysis_cache: RwLock<std::collections::HashMap<Url, AnalysisResult>>,
    /// Synthdef info for hover (name -> SynthdefInfo).
    synthdef_info: RwLock<std::collections::HashMap<String, SynthdefInfo>>,
    /// Effect info for hover.
    effect_info: RwLock<std::collections::HashMap<String, SynthdefInfo>>,
}

impl VibeLangServer {
    /// Create a new VibeLang language server.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: DocumentStore::new(),
            known_synthdefs: RwLock::new(HashSet::new()),
            known_effects: RwLock::new(HashSet::new()),
            import_paths: RwLock::new(Vec::new()),
            workspace_root: RwLock::new(None),
            analysis_cache: RwLock::new(std::collections::HashMap::new()),
            synthdef_info: RwLock::new(std::collections::HashMap::new()),
            effect_info: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Analyze a document and publish diagnostics.
    async fn analyze_and_publish(&self, uri: Url) {
        if let Some(doc) = self.documents.get(&uri) {
            let file_path = uri.to_file_path().ok();
            let import_paths = self.import_paths.read().unwrap().clone();

            // Run analysis
            let analysis = analyze_document(&doc.text(), file_path.as_ref(), &import_paths);

            // Get all diagnostics
            let known_synthdefs = self.known_synthdefs.read().unwrap().clone();
            let known_effects = self.known_effects.read().unwrap().clone();
            let diagnostics = all_diagnostics(&analysis, &known_synthdefs, &known_effects);

            // Cache analysis
            {
                let mut cache = self.analysis_cache.write().unwrap();
                cache.insert(uri.clone(), analysis);
            }

            // Publish diagnostics
            self.client
                .publish_diagnostics(uri, diagnostics, Some(doc.version))
                .await;
        }
    }

    /// Load known synthdefs and effects from stdlib.
    fn load_stdlib_definitions(&self) {
        let stdlib_path = PathBuf::from(vibelang_std::stdlib_path());

        if !stdlib_path.exists() {
            return;
        }

        let mut synthdefs = self.known_synthdefs.write().unwrap();
        let mut effects = self.known_effects.write().unwrap();
        let mut synthdef_info_map = self.synthdef_info.write().unwrap();
        let mut effect_info_map = self.effect_info.write().unwrap();

        // Scan stdlib directory for synthdef and effect definitions
        if let Ok(entries) = std::fs::read_dir(&stdlib_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Recurse into subdirectories
                    self.scan_directory_for_defs(
                        &path,
                        &mut synthdefs,
                        &mut effects,
                        &mut synthdef_info_map,
                        &mut effect_info_map,
                    );
                } else if path.extension().is_some_and(|e| e == "vibe") {
                    self.scan_file_for_defs(
                        &path,
                        &mut synthdefs,
                        &mut effects,
                        &mut synthdef_info_map,
                        &mut effect_info_map,
                    );
                }
            }
        }
    }

    fn scan_directory_for_defs(
        &self,
        dir: &PathBuf,
        synthdefs: &mut HashSet<String>,
        effects: &mut HashSet<String>,
        synthdef_info: &mut std::collections::HashMap<String, SynthdefInfo>,
        effect_info: &mut std::collections::HashMap<String, SynthdefInfo>,
    ) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.scan_directory_for_defs(&path, synthdefs, effects, synthdef_info, effect_info);
                } else if path.extension().is_some_and(|e| e == "vibe") {
                    self.scan_file_for_defs(&path, synthdefs, effects, synthdef_info, effect_info);
                }
            }
        }
    }

    fn scan_file_for_defs(
        &self,
        path: &PathBuf,
        synthdefs: &mut HashSet<String>,
        effects: &mut HashSet<String>,
        synthdef_info: &mut std::collections::HashMap<String, SynthdefInfo>,
        effect_info: &mut std::collections::HashMap<String, SynthdefInfo>,
    ) {
        if let Ok(content) = std::fs::read_to_string(path) {
            // Look for define_synthdef("name") patterns
            let synthdef_re = regex::Regex::new(r#"define_synthdef\s*\(\s*["']([^"']+)["']"#).ok();
            if let Some(re) = synthdef_re {
                for cap in re.captures_iter(&content) {
                    if let Some(m) = cap.get(1) {
                        let name = m.as_str().to_string();
                        synthdefs.insert(name.clone());

                        // Extract category from path
                        let category = path
                            .parent()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string());

                        synthdef_info.insert(
                            name.clone(),
                            SynthdefInfo {
                                name: name.clone(),
                                description: None,
                                parameters: Vec::new(),
                                category,
                            },
                        );
                    }
                }
            }

            // Look for define_fx("name") patterns
            let fx_re = regex::Regex::new(r#"define_fx\s*\(\s*["']([^"']+)["']"#).ok();
            if let Some(re) = fx_re {
                for cap in re.captures_iter(&content) {
                    if let Some(m) = cap.get(1) {
                        let name = m.as_str().to_string();
                        effects.insert(name.clone());

                        let category = path
                            .parent()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string());

                        effect_info.insert(
                            name.clone(),
                            SynthdefInfo {
                                name: name.clone(),
                                description: None,
                                parameters: Vec::new(),
                                category,
                            },
                        );
                    }
                }
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for VibeLangServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Store workspace root
        if let Some(root_uri) = params.root_uri {
            if let Ok(path) = root_uri.to_file_path() {
                *self.workspace_root.write().unwrap() = Some(path.clone());

                // Initialize UGen cache with workspace root
                crate::hover::init_ugen_cache(Some(&path));
            }
        }

        // Set up import paths
        {
            let mut paths = self.import_paths.write().unwrap();
            paths.clear();

            // Add stdlib path
            let stdlib_path = PathBuf::from(vibelang_std::stdlib_path());
            if stdlib_path.exists() {
                paths.push(stdlib_path.clone());
                if let Some(parent) = stdlib_path.parent() {
                    paths.push(parent.to_path_buf());
                }
            }

            // Add workspace root
            if let Some(ref root) = *self.workspace_root.read().unwrap() {
                paths.push(root.clone());
            }
        }

        // Load stdlib definitions
        self.load_stdlib_definitions();

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "\"".to_string(),
                        "'".to_string(),
                        "/".to_string(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("vibelang".to_string()),
                        inter_file_dependencies: true,
                        workspace_diagnostics: false,
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "vibelang-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "VibeLang LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let version = params.text_document.version;

        self.documents.open(uri.clone(), &text, version);
        self.analyze_and_publish(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // Get the full text (we use FULL sync mode)
        if let Some(change) = params.content_changes.into_iter().next() {
            self.documents.update(&uri, &change.text, version);
            self.analyze_and_publish(uri).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.close(&uri);

        // Clear diagnostics
        self.client
            .publish_diagnostics(uri.clone(), vec![], None)
            .await;

        // Remove from cache
        self.analysis_cache.write().unwrap().remove(&uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let context = get_completion_context(&content, position.line as usize, position.character as usize);

        let import_paths = self.import_paths.read().unwrap().clone();
        let known_synthdefs = self.known_synthdefs.read().unwrap().clone();
        let known_effects = self.known_effects.read().unwrap().clone();
        let file_path = uri.to_file_path().ok();

        let completions = get_completions(
            &context,
            &known_synthdefs,
            &known_effects,
            &import_paths,
            file_path.as_ref(),
        );

        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let word = match get_word_at_position(&content, position.line as usize, position.character as usize) {
            Some(w) => w,
            None => return Ok(None),
        };

        let synthdef_info = self.synthdef_info.read().unwrap().clone();
        let effect_info = self.effect_info.read().unwrap().clone();

        Ok(get_hover(&word, &synthdef_info, &effect_info))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();

        // Check cache for analysis
        let analysis = {
            let cache = self.analysis_cache.read().unwrap();
            cache.get(&uri).cloned()
        };

        let analysis = match analysis {
            Some(a) => a,
            None => return Ok(None),
        };

        // Check for import definition
        if let Some(response) = get_import_definition(&analysis.imports, position) {
            return Ok(Some(response));
        }

        // Check for variable definition
        let word = match get_word_at_position(&content, position.line as usize, position.character as usize) {
            Some(w) => w,
            None => return Ok(None),
        };

        if let Some(response) = get_variable_definition(&analysis.variable_defs, &uri, &word, position) {
            return Ok(Some(response));
        }

        Ok(None)
    }

    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let uri = params.text_document.uri;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => {
                return Ok(DocumentDiagnosticReportResult::Report(
                    DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                        related_documents: None,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: None,
                            items: vec![],
                        },
                    }),
                ));
            }
        };

        let file_path = uri.to_file_path().ok();
        let import_paths = self.import_paths.read().unwrap().clone();

        // Run analysis
        let analysis = analyze_document(&doc.text(), file_path.as_ref(), &import_paths);

        // Get all diagnostics
        let known_synthdefs = self.known_synthdefs.read().unwrap().clone();
        let known_effects = self.known_effects.read().unwrap().clone();
        let diagnostics = all_diagnostics(&analysis, &known_synthdefs, &known_effects);

        // Cache analysis
        {
            let mut cache = self.analysis_cache.write().unwrap();
            cache.insert(uri.clone(), analysis);
        }

        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: diagnostics,
                },
            }),
        ))
    }
}
