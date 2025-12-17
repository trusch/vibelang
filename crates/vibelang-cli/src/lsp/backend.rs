//! LSP backend implementation for VibeLang.
//!
//! Implements the Language Server Protocol using tower-lsp.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use super::analysis::{analyze_document, get_completion_context, get_word_at_position, AnalysisResult};
use super::completion::get_completions;
use super::definition::{get_import_definition, get_variable_definition};
use super::diagnostics::all_diagnostics;
use super::document::DocumentStore;
use super::hover::{get_hover, init_ugen_cache, ParamInfo, SynthdefInfo};

/// VibeLang Language Server.
pub struct VibeLangServer {
    /// LSP client for sending notifications.
    client: Client,
    /// Document store.
    documents: DocumentStore,
    /// Import paths for resolution.
    import_paths: Arc<Vec<PathBuf>>,
    /// Known synthdefs (name -> info).
    synthdefs: DashMap<String, SynthdefInfo>,
    /// Known effects (name -> info).
    effects: DashMap<String, SynthdefInfo>,
    /// Analysis results per document.
    analysis_cache: DashMap<Url, AnalysisResult>,
}

impl VibeLangServer {
    /// Create a new VibeLang server.
    pub fn new(client: Client) -> Self {
        let mut import_paths = Vec::new();

        // Add stdlib path
        let stdlib_path = PathBuf::from(vibelang_std::stdlib_path());
        import_paths.push(stdlib_path.clone());
        if let Some(parent) = stdlib_path.parent() {
            import_paths.push(parent.to_path_buf());
        }

        let server = Self {
            client,
            documents: DocumentStore::new(),
            import_paths: Arc::new(import_paths),
            synthdefs: DashMap::new(),
            effects: DashMap::new(),
            analysis_cache: DashMap::new(),
        };

        // Load stdlib synthdefs and effects
        server.load_stdlib_definitions();

        server
    }

    /// Load synthdef and effect definitions from stdlib.
    fn load_stdlib_definitions(&self) {
        // Scan stdlib for synthdef definitions
        let stdlib_path = PathBuf::from(vibelang_std::stdlib_path());

        // Known synthdef categories and their typical parameters
        self.load_drum_synthdefs();
        self.load_bass_synthdefs();
        self.load_lead_synthdefs();
        self.load_pad_synthdefs();
        self.load_effect_synthdefs();

        // Also scan actual files for custom synthdefs
        self.scan_stdlib_directory(&stdlib_path);
    }

    /// Scan a directory for .vibe files and extract synthdef definitions.
    fn scan_stdlib_directory(&self, dir: &PathBuf) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    self.scan_stdlib_directory(&path);
                } else if path.extension().map(|e| e == "vibe").unwrap_or(false) {
                    self.extract_synthdefs_from_file(&path);
                }
            }
        }
    }

    /// Extract synthdef definitions from a .vibe file.
    fn extract_synthdefs_from_file(&self, path: &PathBuf) {
        if let Ok(content) = std::fs::read_to_string(path) {
            // Look for define_synthdef("name") patterns
            let synthdef_re = regex::Regex::new(r#"define_synthdef\s*\(\s*["']([^"']+)["']\s*\)"#).ok();
            let fx_re = regex::Regex::new(r#"define_fx\s*\(\s*["']([^"']+)["']\s*\)"#).ok();

            if let Some(re) = synthdef_re {
                for cap in re.captures_iter(&content) {
                    if let Some(name) = cap.get(1) {
                        let name_str = name.as_str().to_string();
                        if !self.synthdefs.contains_key(&name_str) {
                            // Extract category from path
                            let category = path
                                .parent()
                                .and_then(|p| p.file_name())
                                .and_then(|n| n.to_str())
                                .map(|s| s.to_string());

                            // Try to extract parameters
                            let params = self.extract_params_from_content(&content, &name_str);

                            self.synthdefs.insert(
                                name_str.clone(),
                                SynthdefInfo {
                                    name: name_str,
                                    description: None,
                                    parameters: params,
                                    category,
                                },
                            );
                        }
                    }
                }
            }

            if let Some(re) = fx_re {
                for cap in re.captures_iter(&content) {
                    if let Some(name) = cap.get(1) {
                        let name_str = name.as_str().to_string();
                        if !self.effects.contains_key(&name_str) {
                            let category = path
                                .parent()
                                .and_then(|p| p.file_name())
                                .and_then(|n| n.to_str())
                                .map(|s| s.to_string());

                            let params = self.extract_params_from_content(&content, &name_str);

                            self.effects.insert(
                                name_str.clone(),
                                SynthdefInfo {
                                    name: name_str,
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
    }

    /// Extract parameters from synthdef definition.
    fn extract_params_from_content(&self, content: &str, synthdef_name: &str) -> Vec<ParamInfo> {
        let mut params = Vec::new();

        // Find the synthdef definition and extract .param() calls
        // This is a simplified extraction - in practice you'd want a proper parser
        let pattern = format!(
            r#"define_(?:synthdef|fx)\s*\(\s*["']{}["']\s*\)([^;]*)"#,
            regex::escape(synthdef_name)
        );

        if let Ok(re) = regex::Regex::new(&pattern) {
            if let Some(cap) = re.captures(content) {
                if let Some(body) = cap.get(1) {
                    let body_str = body.as_str();

                    // Extract .param("name", default) calls
                    if let Ok(param_re) =
                        regex::Regex::new(r#"\.param\s*\(\s*["']([^"']+)["']\s*,\s*([0-9.+-]+)"#)
                    {
                        for param_cap in param_re.captures_iter(body_str) {
                            if let (Some(name), Some(default)) = (param_cap.get(1), param_cap.get(2))
                            {
                                params.push(ParamInfo {
                                    name: name.as_str().to_string(),
                                    default: default.as_str().parse().unwrap_or(0.0),
                                    description: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        params
    }

    /// Load common drum synthdefs.
    fn load_drum_synthdefs(&self) {
        let drums = vec![
            ("kick_808", "Classic 808 kick drum", vec![("freq", 60.0), ("amp", 1.0), ("decay", 0.5)]),
            ("kick_909", "Classic 909 kick drum", vec![("freq", 60.0), ("amp", 1.0), ("decay", 0.3)]),
            ("snare_808", "Classic 808 snare drum", vec![("freq", 200.0), ("amp", 1.0), ("decay", 0.2)]),
            ("snare_909", "Classic 909 snare drum", vec![("freq", 200.0), ("amp", 1.0), ("decay", 0.15)]),
            ("hihat_808", "Classic 808 hi-hat", vec![("amp", 1.0), ("decay", 0.1)]),
            ("hihat_909", "Classic 909 hi-hat", vec![("amp", 1.0), ("decay", 0.08)]),
            ("clap_808", "Classic 808 clap", vec![("amp", 1.0), ("decay", 0.2)]),
            ("clap_909", "Classic 909 clap", vec![("amp", 1.0), ("decay", 0.15)]),
        ];

        for (name, desc, params) in drums {
            self.synthdefs.insert(
                name.to_string(),
                SynthdefInfo {
                    name: name.to_string(),
                    description: Some(desc.to_string()),
                    parameters: params
                        .into_iter()
                        .map(|(n, d)| ParamInfo {
                            name: n.to_string(),
                            default: d,
                            description: None,
                        })
                        .collect(),
                    category: Some("drums".to_string()),
                },
            );
        }
    }

    /// Load common bass synthdefs.
    fn load_bass_synthdefs(&self) {
        let basses = vec![
            ("bass_sub", "Deep sub bass", vec![("freq", 55.0), ("amp", 1.0)]),
            ("bass_acid", "Acid 303-style bass", vec![("freq", 55.0), ("amp", 1.0), ("cutoff", 1000.0), ("resonance", 0.5)]),
            ("bass_reese", "Reese bass with detuning", vec![("freq", 55.0), ("amp", 1.0), ("detune", 0.1)]),
        ];

        for (name, desc, params) in basses {
            self.synthdefs.insert(
                name.to_string(),
                SynthdefInfo {
                    name: name.to_string(),
                    description: Some(desc.to_string()),
                    parameters: params
                        .into_iter()
                        .map(|(n, d)| ParamInfo {
                            name: n.to_string(),
                            default: d,
                            description: None,
                        })
                        .collect(),
                    category: Some("bass".to_string()),
                },
            );
        }
    }

    /// Load common lead synthdefs.
    fn load_lead_synthdefs(&self) {
        let leads = vec![
            ("lead_saw", "Sawtooth lead", vec![("freq", 440.0), ("amp", 0.5), ("cutoff", 2000.0)]),
            ("lead_square", "Square wave lead", vec![("freq", 440.0), ("amp", 0.5), ("width", 0.5)]),
            ("lead_supersaw", "Supersaw lead", vec![("freq", 440.0), ("amp", 0.5), ("detune", 0.1), ("voices", 7.0)]),
        ];

        for (name, desc, params) in leads {
            self.synthdefs.insert(
                name.to_string(),
                SynthdefInfo {
                    name: name.to_string(),
                    description: Some(desc.to_string()),
                    parameters: params
                        .into_iter()
                        .map(|(n, d)| ParamInfo {
                            name: n.to_string(),
                            default: d,
                            description: None,
                        })
                        .collect(),
                    category: Some("leads".to_string()),
                },
            );
        }
    }

    /// Load common pad synthdefs.
    fn load_pad_synthdefs(&self) {
        let pads = vec![
            ("pad_warm", "Warm analog pad", vec![("freq", 220.0), ("amp", 0.5), ("attack", 0.5), ("release", 2.0)]),
            ("pad_lush", "Lush detuned pad", vec![("freq", 220.0), ("amp", 0.5), ("detune", 0.05)]),
        ];

        for (name, desc, params) in pads {
            self.synthdefs.insert(
                name.to_string(),
                SynthdefInfo {
                    name: name.to_string(),
                    description: Some(desc.to_string()),
                    parameters: params
                        .into_iter()
                        .map(|(n, d)| ParamInfo {
                            name: n.to_string(),
                            default: d,
                            description: None,
                        })
                        .collect(),
                    category: Some("pads".to_string()),
                },
            );
        }
    }

    /// Load common effect synthdefs.
    fn load_effect_synthdefs(&self) {
        let effects = vec![
            ("reverb", "Reverb effect", vec![("mix", 0.3), ("room", 0.5), ("damp", 0.5)]),
            ("delay", "Delay effect", vec![("time", 0.25), ("feedback", 0.5), ("mix", 0.3)]),
            ("chorus", "Chorus effect", vec![("rate", 0.5), ("depth", 0.5), ("mix", 0.5)]),
            ("distortion", "Distortion effect", vec![("drive", 0.5), ("mix", 1.0)]),
            ("lpf", "Low-pass filter", vec![("cutoff", 1000.0), ("resonance", 0.5)]),
            ("hpf", "High-pass filter", vec![("cutoff", 200.0), ("resonance", 0.5)]),
            ("compressor", "Compressor", vec![("threshold", -12.0), ("ratio", 4.0), ("attack", 0.01), ("release", 0.1)]),
        ];

        for (name, desc, params) in effects {
            self.effects.insert(
                name.to_string(),
                SynthdefInfo {
                    name: name.to_string(),
                    description: Some(desc.to_string()),
                    parameters: params
                        .into_iter()
                        .map(|(n, d)| ParamInfo {
                            name: n.to_string(),
                            default: d,
                            description: None,
                        })
                        .collect(),
                    category: Some("effects".to_string()),
                },
            );
        }
    }

    /// Analyze a document and publish diagnostics.
    async fn analyze_and_publish(&self, uri: &Url) {
        if let Some(doc) = self.documents.get(uri) {
            let content = doc.text();
            let file_path = uri.to_file_path().ok();

            eprintln!("[LSP] Analyzing document: {}", uri);

            // Analyze the document
            let analysis = analyze_document(&content, file_path.as_ref(), &self.import_paths);

            eprintln!("[LSP] Analysis found {} syntax errors, {} synthdef refs",
                analysis.syntax_errors.len(), analysis.synthdef_refs.len());

            // Store analysis result
            self.analysis_cache.insert(uri.clone(), analysis.clone());

            // Generate diagnostics
            let known_synthdefs: HashSet<String> =
                self.synthdefs.iter().map(|e| e.key().clone()).collect();
            let known_effects: HashSet<String> =
                self.effects.iter().map(|e| e.key().clone()).collect();

            let diagnostics = all_diagnostics(&analysis, &known_synthdefs, &known_effects);

            eprintln!("[LSP] Publishing {} diagnostics for {}", diagnostics.len(), uri);

            // Publish diagnostics
            self.client
                .publish_diagnostics(uri.clone(), diagnostics, Some(doc.version))
                .await;
        }
    }

    /// Get known synthdefs as a HashSet.
    fn known_synthdefs(&self) -> HashSet<String> {
        self.synthdefs.iter().map(|e| e.key().clone()).collect()
    }

    /// Get known effects as a HashSet.
    fn known_effects(&self) -> HashSet<String> {
        self.effects.iter().map(|e| e.key().clone()).collect()
    }

    /// Get synthdef info as a HashMap.
    fn synthdef_info(&self) -> HashMap<String, SynthdefInfo> {
        self.synthdefs
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    /// Get effect info as a HashMap.
    fn effect_info(&self) -> HashMap<String, SynthdefInfo> {
        self.effects
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for VibeLangServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Add workspace folders to import paths and initialize UGen cache
        if let Some(folders) = &params.workspace_folders {
            let mut paths = (*self.import_paths).clone();
            for folder in folders {
                if let Ok(path) = folder.uri.to_file_path() {
                    // Initialize UGen cache with the first workspace folder
                    if paths.len() == self.import_paths.len() {
                        init_ugen_cache(Some(&path));
                    }
                    paths.push(path);
                }
            }
        } else {
            // Initialize UGen cache without workspace path
            init_ugen_cache(None);
        }

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
                // We use push-based diagnostics via publishDiagnostics (no capability needed)
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
        let content = params.text_document.text;
        let version = params.text_document.version;

        self.documents.open(uri.clone(), &content, version);
        self.analyze_and_publish(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // We use full sync, so there's only one change with the full content
        if let Some(change) = params.content_changes.into_iter().next() {
            self.documents.update(&uri, &change.text, version);
            self.analyze_and_publish(&uri).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.close(&uri);
        self.analysis_cache.remove(&uri);

        // Clear diagnostics
        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let context = get_completion_context(&content, position.line as usize, position.character as usize);

        let file_path = uri.to_file_path().ok();
        let completions = get_completions(
            &context,
            &self.known_synthdefs(),
            &self.known_effects(),
            &self.import_paths,
            file_path.as_ref(),
        );

        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let content = doc.text();
        let word = match get_word_at_position(&content, position.line as usize, position.character as usize) {
            Some(w) => w,
            None => return Ok(None),
        };

        let hover = get_hover(&word, &self.synthdef_info(), &self.effect_info());
        Ok(hover)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Get cached analysis
        let analysis = match self.analysis_cache.get(uri) {
            Some(a) => a.clone(),
            None => return Ok(None),
        };

        // Try import definition first
        if let Some(definition) = get_import_definition(&analysis.imports, position) {
            return Ok(Some(definition));
        }

        // Try variable definition
        let doc = match self.documents.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let content = doc.text();
        if let Some(word) = get_word_at_position(&content, position.line as usize, position.character as usize) {
            if let Some(definition) = get_variable_definition(&analysis.variable_defs, uri, &word, position) {
                return Ok(Some(definition));
            }
        }

        Ok(None)
    }
}
