//! LSP server implementation for VibeLang.
//!
//! This module implements the Language Server Protocol handler using tower-lsp.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::RwLock;

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::analysis::{analyze_document, AnalysisResult};
use crate::data::{get_api_docs, DocumentStore, SynthdefInfo};
use crate::features::{
    code_actions, completion, definition, document_links, document_symbols, folding, formatting,
    hover, inlay_hints, references, rename, semantic_tokens, signature_help, workspace_symbols,
};
use crate::validation::ValidationEngine;

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
    analysis_cache: DashMap<Url, AnalysisResult>,
    /// Synthdef info for hover (name -> SynthdefInfo).
    synthdef_info: RwLock<HashMap<String, SynthdefInfo>>,
    /// Effect info for hover.
    effect_info: RwLock<HashMap<String, SynthdefInfo>>,
    /// Validation engine for vibelang-core based validation.
    validation_engine: RwLock<Option<ValidationEngine>>,
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
            analysis_cache: DashMap::new(),
            synthdef_info: RwLock::new(HashMap::new()),
            effect_info: RwLock::new(HashMap::new()),
            validation_engine: RwLock::new(None),
        }
    }

    /// Analyze a document and publish diagnostics.
    async fn analyze_and_publish(&self, uri: Url) {
        if let Some(doc) = self.documents.get(&uri) {
            let file_path = uri.to_file_path().ok();
            let import_paths = self.import_paths.read().unwrap().clone();

            // Run analysis
            let analysis = analyze_document(&doc.text(), file_path.as_ref(), &import_paths);

            // Get known synthdefs and effects for validation
            let known_synthdefs = self.known_synthdefs.read().unwrap().clone();
            let known_effects = self.known_effects.read().unwrap().clone();

            // Run validation engine if available
            let validation_diagnostics =
                if let Some(engine) = self.validation_engine.read().unwrap().as_ref() {
                    let result = engine.validate_with_context(&doc.text(), file_path.clone());
                    result.all_diagnostics()
                } else {
                    Vec::new()
                };

            // Combine all diagnostics
            let mut all_diagnostics = Vec::new();
            all_diagnostics.extend(analysis.syntax_errors.clone());
            all_diagnostics.extend(analysis.semantic_diagnostics.clone());
            all_diagnostics.extend(analysis.lint_diagnostics.clone());
            all_diagnostics.extend(validation_diagnostics);

            // Filter out duplicate diagnostics
            all_diagnostics =
                crate::features::diagnostics::deduplicate_diagnostics(all_diagnostics);

            // Check for undefined synthdefs
            for synthdef_ref in &analysis.synthdef_refs {
                if !known_synthdefs.contains(&synthdef_ref.name)
                    && !analysis.local_synthdefs.contains(&synthdef_ref.name)
                {
                    all_diagnostics.push(Diagnostic {
                        range: synthdef_ref.range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("undefined-synthdef".to_string())),
                        source: Some("vibelang".to_string()),
                        message: format!(
                            "Unknown synthdef '{}'. Import from stdlib or define it.",
                            synthdef_ref.name
                        ),
                        ..Default::default()
                    });
                }
            }

            // Check for undefined effects
            for effect_ref in &analysis.effect_refs {
                if !known_effects.contains(&effect_ref.name)
                    && !analysis.local_effects.contains(&effect_ref.name)
                {
                    all_diagnostics.push(Diagnostic {
                        range: effect_ref.range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("undefined-effect".to_string())),
                        source: Some("vibelang".to_string()),
                        message: format!(
                            "Unknown effect '{}'. Import from stdlib or define it.",
                            effect_ref.name
                        ),
                        ..Default::default()
                    });
                }
            }

            // Cache analysis
            self.analysis_cache.insert(uri.clone(), analysis);

            // Publish diagnostics
            self.client
                .publish_diagnostics(uri, all_diagnostics, Some(doc.version))
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
        self.scan_directory_for_defs(
            &stdlib_path,
            &mut synthdefs,
            &mut effects,
            &mut synthdef_info_map,
            &mut effect_info_map,
        );
    }

    fn scan_directory_for_defs(
        &self,
        dir: &PathBuf,
        synthdefs: &mut HashSet<String>,
        effects: &mut HashSet<String>,
        synthdef_info: &mut HashMap<String, SynthdefInfo>,
        effect_info: &mut HashMap<String, SynthdefInfo>,
    ) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.scan_directory_for_defs(
                        &path,
                        synthdefs,
                        effects,
                        synthdef_info,
                        effect_info,
                    );
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
        synthdef_info: &mut HashMap<String, SynthdefInfo>,
        effect_info: &mut HashMap<String, SynthdefInfo>,
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

                        // Extract parameters
                        let params = self.extract_params_from_content(&content, &name);

                        synthdef_info.insert(
                            name.clone(),
                            SynthdefInfo {
                                name: name.clone(),
                                description: None,
                                parameters: params,
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

                        let params = self.extract_params_from_content(&content, &name);

                        effect_info.insert(
                            name.clone(),
                            SynthdefInfo {
                                name: name.clone(),
                                description: None,
                                parameters: params,
                                category,
                            },
                        );
                    }
                }
            }
        }
    }

    /// Extract parameters from synthdef/fx definition content.
    fn extract_params_from_content(
        &self,
        content: &str,
        _name: &str,
    ) -> Vec<crate::data::ParamInfo> {
        let mut params = Vec::new();
        let param_re = regex::Regex::new(r#"\.param\s*\(\s*["']([^"']+)["']\s*,\s*([^)]+)\)"#).ok();

        if let Some(re) = param_re {
            for cap in re.captures_iter(content) {
                if let (Some(name_match), Some(default_match)) = (cap.get(1), cap.get(2)) {
                    let default: f64 = default_match.as_str().trim().parse().unwrap_or(0.0);
                    params.push(crate::data::ParamInfo {
                        name: name_match.as_str().to_string(),
                        default,
                        description: None,
                    });
                }
            }
        }

        params
    }

    /// Initialize the validation engine.
    fn init_validation_engine(&self) {
        let mut engine = ValidationEngine::new();
        for path in self.import_paths.read().unwrap().iter() {
            engine.add_import_path(path.clone());
        }
        *self.validation_engine.write().unwrap() = Some(engine);
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
                crate::data::init_ugen_cache(Some(&path));
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

        // Initialize validation engine
        self.init_validation_engine();

        // Define semantic token types
        let token_types = vec![
            SemanticTokenType::FUNCTION,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::STRING,
            SemanticTokenType::NUMBER,
            SemanticTokenType::KEYWORD,
            SemanticTokenType::COMMENT,
            SemanticTokenType::TYPE,
            SemanticTokenType::PARAMETER,
            SemanticTokenType::PROPERTY,
            SemanticTokenType::METHOD,
            SemanticTokenType::new("synthdef"),
            SemanticTokenType::new("voice"),
            SemanticTokenType::new("pattern"),
            SemanticTokenType::new("melody"),
            SemanticTokenType::new("group"),
            SemanticTokenType::new("note"),
            SemanticTokenType::new("patternToken"),
        ];

        let token_modifiers = vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::DEFINITION,
            SemanticTokenModifier::READONLY,
        ];

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
                        "(".to_string(),
                        ",".to_string(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    ..Default::default()
                }),
                document_symbol_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: None,
                    file_operations: None,
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types,
                                token_modifiers,
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: Some(false),
                            ..Default::default()
                        },
                    ),
                ),
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
                name: "vibelang-lsp2".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "VibeLang LSP2 server initialized")
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
        self.analysis_cache.remove(&uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let context = crate::analysis::get_completion_context(
            &content,
            position.line as usize,
            position.character as usize,
        );

        let import_paths = self.import_paths.read().unwrap().clone();
        let known_synthdefs = self.known_synthdefs.read().unwrap().clone();
        let known_effects = self.known_effects.read().unwrap().clone();
        let file_path = uri.to_file_path().ok();

        // Get cached analysis for local definitions
        let analysis = self.analysis_cache.get(&uri);

        let completions = completion::get_completions(
            &context,
            &known_synthdefs,
            &known_effects,
            &import_paths,
            file_path.as_ref(),
            analysis.as_ref().map(|a| a.value()),
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
        let word = match crate::analysis::get_word_at_position(
            &content,
            position.line as usize,
            position.character as usize,
        ) {
            Some(w) => w,
            None => return Ok(None),
        };

        let synthdef_info = self.synthdef_info.read().unwrap().clone();
        let effect_info = self.effect_info.read().unwrap().clone();

        Ok(hover::get_hover(
            &word,
            &content,
            position,
            &synthdef_info,
            &effect_info,
        ))
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
        let analysis = match self.analysis_cache.get(&uri) {
            Some(a) => a.clone(),
            None => return Ok(None),
        };

        // Check for import definition
        if let Some(response) = definition::get_import_definition(&analysis.imports, position) {
            return Ok(Some(response));
        }

        // Check for variable definition
        let word = match crate::analysis::get_word_at_position(
            &content,
            position.line as usize,
            position.character as usize,
        ) {
            Some(w) => w,
            None => return Ok(None),
        };

        if let Some(response) =
            definition::get_variable_definition(&analysis.variable_defs, &uri, &word, position)
        {
            return Ok(Some(response));
        }

        Ok(None)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        // Get the current line
        let line = content.lines().nth(position.line as usize).unwrap_or("");

        let api_docs: Vec<_> = get_api_docs().values().cloned().collect();
        Ok(signature_help::get_signature_help(
            line,
            position.character,
            &api_docs,
        ))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();

        // Get cached analysis
        let analysis = self.analysis_cache.get(&uri);

        let symbols =
            document_symbols::get_document_symbols(&content, analysis.as_ref().map(|a| a.value()));

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let context = params.context;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let analysis = self.analysis_cache.get(&uri);

        let actions = code_actions::get_code_actions(
            &uri,
            range,
            &context.diagnostics,
            &content,
            analysis.as_ref().map(|a| a.value()),
        );

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let api_docs: Vec<_> = get_api_docs().values().cloned().collect();
        let hints = inlay_hints::get_inlay_hints(&content, &api_docs);

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
        let uri = params.text_document.uri;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let analysis = self.analysis_cache.get(&uri);

        let tokens =
            semantic_tokens::get_semantic_tokens(&content, analysis.as_ref().map(|a| a.value()));

        Ok(Some(SemanticTokensResult::Tokens(tokens)))
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

        // Combine all diagnostics
        let mut diagnostics = Vec::new();
        diagnostics.extend(analysis.syntax_errors.clone());
        diagnostics.extend(analysis.semantic_diagnostics.clone());
        diagnostics.extend(analysis.lint_diagnostics.clone());

        // Cache analysis
        self.analysis_cache.insert(uri.clone(), analysis);

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

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();

        // Get the symbol at position
        let line = content.lines().nth(position.line as usize).unwrap_or("");
        let char_pos = position.character as usize;

        // Check if we're inside a string (entity reference) or on an identifier
        let is_in_string = is_position_in_string(line, char_pos);

        if is_in_string {
            // Find entity references (voice, pattern, etc.)
            if let Some(entity_name) = extract_string_at_position(line, char_pos) {
                let refs = references::find_entity_references(
                    &content,
                    &entity_name,
                    &uri,
                    include_declaration,
                );
                return Ok(Some(refs));
            }
        } else {
            // Find variable references
            if let Some(word) = get_word_at_position(line, char_pos) {
                let refs = references::find_references(&content, &word, &uri, include_declaration);
                return Ok(Some(refs));
            }
        }

        Ok(None)
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        Ok(rename::prepare_rename(&content, position))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        Ok(rename::rename_symbol(&content, &uri, position, &new_name))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = &params.query;
        let symbols = workspace_symbols::get_workspace_symbols(query, &self.documents);

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(symbols))
        }
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let ranges = folding::get_folding_ranges(&content);

        if ranges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ranges))
        }
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let file_path = uri.to_file_path().ok();
        let import_paths = self.import_paths.read().unwrap().clone();

        let links = document_links::get_document_links(&content, file_path.as_ref(), &import_paths);

        if links.is_empty() {
            Ok(None)
        } else {
            Ok(Some(links))
        }
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let options = params.options;

        let doc = match self.documents.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let tab_size = options.tab_size;
        let insert_spaces = options.insert_spaces;

        let edits = formatting::format_document(&content, tab_size, insert_spaces);

        if edits.is_empty() {
            Ok(None)
        } else {
            Ok(Some(edits))
        }
    }
}

/// Check if position is inside a string literal.
fn is_position_in_string(line: &str, char_pos: usize) -> bool {
    let mut in_string = false;
    let mut escape_next = false;
    let mut quote_char = '"';

    for (i, c) in line.char_indices() {
        if i >= char_pos {
            return in_string;
        }

        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => escape_next = true,
            '"' | '\'' => {
                if !in_string {
                    in_string = true;
                    quote_char = c;
                } else if c == quote_char {
                    in_string = false;
                }
            }
            _ => {}
        }
    }

    in_string
}

/// Extract the string content at a position.
fn extract_string_at_position(line: &str, char_pos: usize) -> Option<String> {
    let before = &line[..char_pos];
    let quote_start = before.rfind(['"', '\''])?;
    let quote_char = before.chars().nth(quote_start)?;

    let after = &line[quote_start + 1..];
    let quote_end = after.find(quote_char)?;

    Some(after[..quote_end].to_string())
}

/// Get the word (identifier) at a position.
fn get_word_at_position(line: &str, char_pos: usize) -> Option<String> {
    if char_pos > line.len() {
        return None;
    }

    let start = line[..char_pos]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let end = line[char_pos..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| char_pos + i)
        .unwrap_or(line.len());

    if start >= end {
        return None;
    }

    Some(line[start..end].to_string())
}
