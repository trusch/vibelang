//! Script engine - the main entry point for executing VibeLang scripts.
//!
//! The [`ScriptEngine`] wraps a Rhai engine with all VibeLang API functions registered.

use rhai::Engine;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use vibelang_core::reload::ScriptState;

use crate::api;
use crate::context;
use crate::error::{Error, Result};

// ============================================================================
// In-memory module resolver for WASM
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod wasm_resolver {
    use rhai::module_resolvers::ModuleResolver;
    use rhai::{Engine, Module, Position, Scope, AST};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// In-memory module resolver for WASM.
    ///
    /// This resolver looks up module source code from an in-memory HashMap
    /// instead of the filesystem.
    #[derive(Clone)]
    pub struct InMemoryModuleResolver {
        /// Map of module path -> source code
        modules: Arc<HashMap<String, String>>,
        /// File extension to add (default: "vibe")
        extension: String,
    }

    impl InMemoryModuleResolver {
        /// Create a new in-memory resolver with the given modules.
        pub fn new(modules: HashMap<String, String>) -> Self {
            Self {
                modules: Arc::new(modules),
                extension: "vibe".to_string(),
            }
        }

        /// Normalize a module path for lookup.
        fn normalize_path(&self, path: &str) -> String {
            let mut normalized = path.to_string();

            // Remove leading "./" or "/"
            if normalized.starts_with("./") {
                normalized = normalized[2..].to_string();
            } else if normalized.starts_with('/') {
                normalized = normalized[1..].to_string();
            }

            // Remove "stdlib/" prefix if present (we store without it)
            if normalized.starts_with("stdlib/") {
                normalized = normalized[7..].to_string();
            }

            // Add extension if not present
            if !normalized.ends_with(&format!(".{}", self.extension)) {
                normalized = format!("{}.{}", normalized, self.extension);
            }

            normalized
        }
    }

    impl ModuleResolver for InMemoryModuleResolver {
        fn resolve(
            &self,
            engine: &Engine,
            _source: Option<&str>,
            path: &str,
            pos: Position,
        ) -> Result<rhai::Shared<Module>, Box<rhai::EvalAltResult>> {
            let normalized = self.normalize_path(path);

            // Look up the module source
            let source = self.modules.get(&normalized).ok_or_else(|| {
                Box::new(rhai::EvalAltResult::ErrorModuleNotFound(
                    format!("Module not found: {} (looked for: {})", path, normalized),
                    pos,
                ))
            })?;

            // Compile and create module
            let ast = engine.compile(source).map_err(|e| {
                Box::new(rhai::EvalAltResult::ErrorInModule(
                    path.to_string(),
                    e.into(),
                    pos,
                ))
            })?;

            // Create module from AST
            let module = Module::eval_ast_as_new(Scope::new(), &ast, engine).map_err(|e| {
                Box::new(rhai::EvalAltResult::ErrorInModule(path.to_string(), e, pos))
            })?;

            Ok(rhai::Shared::new(module))
        }

        fn resolve_ast(
            &self,
            engine: &Engine,
            _source: Option<&str>,
            path: &str,
            pos: Position,
        ) -> Option<Result<AST, Box<rhai::EvalAltResult>>> {
            let normalized = self.normalize_path(path);

            // Look up the module source
            let source = match self.modules.get(&normalized) {
                Some(s) => s,
                None => {
                    return Some(Err(Box::new(rhai::EvalAltResult::ErrorModuleNotFound(
                        format!("Module not found: {} (looked for: {})", path, normalized),
                        pos,
                    ))))
                }
            };

            // Compile the source
            Some(engine.compile(source).map_err(|e| {
                Box::new(rhai::EvalAltResult::ErrorInModule(
                    path.to_string(),
                    e.into(),
                    pos,
                ))
            }))
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_resolver::InMemoryModuleResolver;

/// Script engine for executing VibeLang scripts.
///
/// The ScriptEngine:
/// 1. Creates a Rhai engine with all VibeLang API registered
/// 2. Executes scripts that call the API functions
/// 3. Collects the resulting ScriptState
///
/// # Example
///
/// ```ignore
/// let mut engine = ScriptEngine::new();
/// let state = engine.execute_file("song.vibe")?;
/// // Apply state to runtime via reload system
/// ```
pub struct ScriptEngine {
    engine: Engine,
    #[cfg(not(target_arch = "wasm32"))]
    import_paths: Vec<PathBuf>,
}

impl ScriptEngine {
    /// Create a new script engine with all VibeLang API registered.
    pub fn new() -> Self {
        let mut engine = Engine::new();

        // Set appropriate limits for complex scripts
        engine.set_max_expr_depths(4096, 4096);
        engine.set_max_call_levels(4096);

        // Override print() to route through the log system
        engine.on_print(|text| {
            log::info!("[script] {}", text);
        });

        // Override debug() similarly
        engine.on_debug(|text, source, pos| {
            let loc = match (source, pos) {
                (Some(src), pos) if !pos.is_none() => format!(" ({}:{})", src, pos),
                (Some(src), _) => format!(" ({})", src),
                (None, pos) if !pos.is_none() => format!(" ({})", pos),
                _ => String::new(),
            };
            log::debug!("[script]{} {}", loc, text);
        });

        // Register VibeLang API
        api::register_api(&mut engine);

        // Register vibelang-dsp API for define_synthdef
        vibelang_dsp::register_dsp_api(&mut engine);

        // Set up stdlib module resolver for WASM
        #[cfg(target_arch = "wasm32")]
        {
            let stdlib_files = vibelang_std::get_stdlib_files();
            let resolver = InMemoryModuleResolver::new(stdlib_files);
            engine.set_module_resolver(resolver);
        }

        Self {
            engine,
            #[cfg(not(target_arch = "wasm32"))]
            import_paths: Vec::new(),
        }
    }

    /// Add import paths for module resolution (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn add_import_path(&mut self, path: impl Into<PathBuf>) {
        self.import_paths.push(path.into());
    }

    /// Execute a script from a string.
    ///
    /// Returns the collected ScriptState that can be applied to a runtime.
    pub fn execute(&mut self, script: &str) -> Result<ScriptState> {
        // Clear object registries before each execution
        api::clear_all_registries();

        // Reset exit code before execution
        crate::reset_exit_code();

        // Initialize context
        context::init_context();

        // Execute script
        let result = self.engine.run(script).map_err(Error::from);

        // Take state regardless of result (to clean up context)
        let state = context::take_state();
        context::clear_context();

        // Check if script exited via exit() - this is not an error
        if crate::get_exit_code().is_some() {
            // Script requested exit, return state normally
            return Ok(state);
        }

        // Return error if script failed for other reasons
        result?;

        Ok(state)
    }

    /// Execute a script from a file (native only).
    ///
    /// Returns the collected ScriptState that can be applied to a runtime.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn execute_file(&mut self, path: impl AsRef<Path>) -> Result<ScriptState> {
        let path = path.as_ref();

        // Read script
        let script = std::fs::read_to_string(path)?;

        // Set up module resolver
        let base_path = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        self.setup_module_resolver(base_path);

        // Clear object registries before each execution
        api::clear_all_registries();

        // Reset exit code before execution
        crate::reset_exit_code();

        // Initialize context
        context::init_context();
        context::set_current_file(Some(path.to_path_buf()));
        context::set_import_paths(self.import_paths.clone());

        // Execute script
        let result = self.engine.run(&script).map_err(Error::from);

        // Take state regardless of result
        let state = context::take_state();
        context::clear_context();

        // Debug: log melody state after script execution
        tracing::debug!(
            "Script execution complete: {} melodies, {} playing_melodies, {} voices",
            state.melodies.len(),
            state.playing_melodies.len(),
            state.voices.len()
        );
        for (id, config) in &state.melodies {
            tracing::debug!(
                "  Melody {:?} '{}': voice={:?}, notes={}, length={:.2}",
                id,
                config.name,
                config.voice,
                config.notes.len(),
                config.length.to_f64()
            );
        }
        for id in &state.playing_melodies {
            tracing::debug!("  Playing melody: {:?}", id);
        }

        // Check if script exited via exit() - this is not an error
        if crate::get_exit_code().is_some() {
            // Script requested exit, return state normally
            return Ok(state);
        }

        // Return error if script failed for other reasons
        result?;

        Ok(state)
    }

    /// Execute a script from a file and return state, AST, and any registered
    /// MIDI callbacks captured during execution (native + `midi` feature only).
    ///
    /// The returned `AST` is the compiled script source — the runtime needs it
    /// to invoke `FnPtr` callbacks (e.g. `mpk.on_note(|n, v, on| ...)`) after
    /// the original script run has finished.
    #[cfg(all(not(target_arch = "wasm32"), feature = "midi"))]
    pub fn execute_file_full(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(
        ScriptState,
        rhai::AST,
        std::collections::HashMap<u64, rhai::FnPtr>,
    )> {
        let path = path.as_ref();

        let script = std::fs::read_to_string(path)?;

        let base_path = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        self.setup_module_resolver(base_path);

        api::clear_all_registries();
        crate::reset_exit_code();

        context::init_context();
        context::set_current_file(Some(path.to_path_buf()));
        context::set_import_paths(self.import_paths.clone());

        let ast = self.engine.compile(&script).map_err(Error::from);
        let ast = match ast {
            Ok(a) => a,
            Err(e) => {
                context::clear_context();
                return Err(e);
            }
        };

        let result = self.engine.run_ast(&ast).map_err(Error::from);

        let state = context::take_state();
        let midi_callbacks = context::take_midi_callbacks();
        context::clear_context();

        if crate::get_exit_code().is_some() {
            return Ok((state, ast, midi_callbacks));
        }

        result?;

        Ok((state, ast, midi_callbacks))
    }

    /// Execute an AST (pre-compiled script).
    pub fn execute_ast(&mut self, ast: &rhai::AST) -> Result<ScriptState> {
        // Clear object registries before each execution
        api::clear_all_registries();

        // Initialize context
        context::init_context();

        // Execute
        let result = self.engine.run_ast(ast).map_err(Error::from);

        // Take state
        let state = context::take_state();
        context::clear_context();

        result?;

        Ok(state)
    }

    /// Compile a script to AST for repeated execution.
    pub fn compile(&self, script: &str) -> Result<rhai::AST> {
        self.engine.compile(script).map_err(Error::from)
    }

    /// Compile a script file to AST (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn compile_file(&self, path: impl AsRef<Path>) -> Result<rhai::AST> {
        let path = path.as_ref();
        self.engine
            .compile_file(path.to_path_buf())
            .map_err(Error::from)
    }

    /// Set up module resolver for import statements (native only).
    #[cfg(not(target_arch = "wasm32"))]
    fn setup_module_resolver(&mut self, base_path: PathBuf) {
        let mut collection = rhai::module_resolvers::ModuleResolversCollection::new();

        // 1. Source-relative resolver (highest priority)
        let mut source_resolver = rhai::module_resolvers::FileModuleResolver::new();
        source_resolver.set_extension("vibe");
        collection.push(source_resolver);

        // 2. Base path resolver
        let mut base_resolver = rhai::module_resolvers::FileModuleResolver::new();
        base_resolver.set_base_path(base_path);
        base_resolver.set_extension("vibe");
        collection.push(base_resolver);

        // 3. Additional import paths
        for import_path in &self.import_paths {
            let mut resolver = rhai::module_resolvers::FileModuleResolver::new();
            resolver.set_base_path(import_path.clone());
            resolver.set_extension("vibe");
            collection.push(resolver);
        }

        self.engine.set_module_resolver(collection);
    }

    /// Get a reference to the underlying Rhai engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Get a mutable reference to the underlying Rhai engine.
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Register optional extensions with the script engine.
    ///
    /// Extensions provide additional capabilities like filesystem access,
    /// shell command execution, and networking. These are disabled by default
    /// and must be explicitly enabled.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use vibelang_rhai::{ScriptEngine, ExtensionConfig};
    ///
    /// let mut engine = ScriptEngine::new();
    ///
    /// // Enable specific extensions
    /// let config = ExtensionConfig::new()
    ///     .with_filesystem()
    ///     .with_exec();
    /// engine.register_extensions(&config);
    /// ```
    ///
    /// # Security
    ///
    /// These extensions provide powerful capabilities. Only enable them
    /// in trusted environments where script authors are trusted.
    #[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
    pub fn register_extensions(&mut self, config: &crate::extensions::ExtensionConfig) {
        crate::extensions::register_extensions(&mut self.engine, config);
    }

    /// Register all available extensions.
    ///
    /// This is a convenience method that enables all compiled-in extensions.
    /// Use with caution in production environments.
    #[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
    pub fn register_all_extensions(&mut self) {
        let config = crate::extensions::ExtensionConfig::enable_all();
        crate::extensions::register_extensions(&mut self.engine, &config);
    }
}

impl Default for ScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    fn write_test_project(files: &[(&str, &str)]) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!(
            "vibelang-rhai-test-{}-{}",
            std::process::id(),
            nonce
        ));
        std::fs::create_dir_all(&dir).unwrap();

        for (name, contents) in files {
            std::fs::write(dir.join(name), contents).unwrap();
        }

        dir
    }

    #[test]
    fn test_execute_simple_script() {
        let mut engine = ScriptEngine::new();
        let state = engine.execute("set_tempo(140);").unwrap();
        assert_eq!(state.tempo, 140.0);
    }

    #[test]
    fn test_execute_with_group() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(128);
            define_group("Drums", || {
                let kick = voice("kick").synth("kick_synth").gain(db(-6));
            });
        "#,
            )
            .unwrap();

        assert_eq!(state.tempo, 128.0);
        assert!(!state.groups.is_empty());
        assert!(!state.voices.is_empty());
    }

    #[test]
    fn test_repeated_group_body_appends_to_existing_group() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Drums").gain(0.5).body(|| {
                voice("kick").synth("kick_synth");
                fx("drums_comp").synth("compressor").apply();
            });

            group("Drums").body(|| {
                voice("snare").synth("snare_synth");
            });
        "#,
            )
            .unwrap();

        let drums_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Drums").then_some(*id))
            .expect("Drums group should exist");
        let drums = state.groups.get(&drums_id).unwrap();

        assert_eq!(
            state
                .groups
                .values()
                .filter(|config| config.name == "Drums")
                .count(),
            1,
            "Repeated bodies should not create duplicate Drums groups"
        );
        assert_eq!(state.voices.len(), 2, "Both body voices should remain");
        assert!(state
            .voices
            .values()
            .any(|voice| voice.name == "kick" && voice.group == drums_id));
        assert!(state
            .voices
            .values()
            .any(|voice| voice.name == "snare" && voice.group == drums_id));
        assert_eq!(
            drums.effects.len(),
            1,
            "Later bodies should not clear effects"
        );
        assert_eq!(drums.params.get("amp"), Some(&0.5));
        assert_eq!(state.body_contributions.len(), 2);
        assert!(state
            .body_contributions
            .iter()
            .all(|body| body.target_group == drums_id && body.target_path == "main/Drums"));
        assert_eq!(state.body_contributions[0].ordinal, 0);
        assert_eq!(state.body_contributions[1].ordinal, 1);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_group_bodies_merge_across_imported_files_in_eval_order() {
        let dir = write_test_project(&[
            (
                "main.vibe",
                r#"
            group("Drums").gain(0.5).body(|| {
                voice("kick").synth("kick_synth");
            });

            import "effects.vibe";

            group("Drums").body(|| {
                voice("hat").synth("hat_synth");
            });
        "#,
            ),
            (
                "effects.vibe",
                r#"
            group("Drums").gain(0.75).body(|| {
                voice("snare").synth("snare_synth");
            });
        "#,
            ),
        ]);

        let mut engine = ScriptEngine::new();
        let state = engine.execute_file(dir.join("main.vibe")).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        let drums_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Drums").then_some(*id))
            .expect("Drums group should exist");
        let drums = state.groups.get(&drums_id).unwrap();

        assert_eq!(
            state
                .groups
                .values()
                .filter(|config| config.name == "Drums")
                .count(),
            1,
            "main and imported bodies should merge into one Drums group"
        );
        assert_eq!(drums.params.get("amp"), Some(&0.75));
        assert!(state
            .body_contributions
            .iter()
            .all(|body| body.target_group == drums_id && body.target_path == "main/Drums"));
        assert_eq!(
            state
                .body_contributions
                .iter()
                .map(|body| body.ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );

        let voice_names = state
            .voice_order
            .iter()
            .map(|id| state.voices.get(id).unwrap().name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(voice_names, vec!["kick", "snare", "hat"]);
        assert!(state.voices.values().all(|voice| voice.group == drums_id));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_alias_body_merges_with_canonical_body_across_imports() {
        let dir = write_test_project(&[
            (
                "main.vibe",
                r#"
            group("Drums").body(|| {
                voice("kick").synth("kick_synth");
            });

            import "aliases.vibe";

            group("kit").body(|| {
                voice("hat").synth("hat_synth");
            });
        "#,
            ),
            (
                "aliases.vibe",
                r#"
            group("Drums").alias("kit");

            group("kit").body(|| {
                voice("snare").synth("snare_synth");
            });
        "#,
            ),
        ]);

        let mut engine = ScriptEngine::new();
        let state = engine.execute_file(dir.join("main.vibe")).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        let alias_target = state.group_aliases.get("kit").unwrap();
        let drums = state.groups.get(&alias_target.group_id).unwrap();

        assert_eq!(alias_target.path, "main/Drums");
        assert_eq!(drums.name, "Drums");
        assert_eq!(
            state
                .groups
                .values()
                .filter(|config| config.name == "Drums")
                .count(),
            1,
            "canonical and alias bodies should merge into one Drums group"
        );
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "alias body should not create a kit group"
        );
        assert_eq!(
            state
                .body_contributions
                .iter()
                .map(|body| (body.target_path.as_str(), body.ordinal))
                .collect::<Vec<_>>(),
            vec![("main/Drums", 0), ("main/Drums", 1), ("main/Drums", 2)]
        );

        let voice_names = state
            .voice_order
            .iter()
            .map(|id| state.voices.get(id).unwrap().name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(voice_names, vec!["kick", "snare", "hat"]);
        assert!(state
            .voices
            .values()
            .all(|voice| voice.group == alias_target.group_id));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_imported_body_uses_resolved_handle_not_caller_context() {
        let dir = write_test_project(&[
            (
                "main.vibe",
                r#"
            group("Drums").body(|| {
                voice("kick").synth("kick_synth");
            });

            import "fills.vibe";
        "#,
            ),
            (
                "fills.vibe",
                r#"
            let drums = group("Drums");

            group("Song").body(|| {
                drums.body(|| {
                    voice("snare").synth("snare_synth");
                });

                voice("pad").synth("pad_synth");
            });
        "#,
            ),
        ]);

        let mut engine = ScriptEngine::new();
        let state = engine.execute_file(dir.join("main.vibe")).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        let drums_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Drums").then_some(*id))
            .expect("Drums group should exist");
        let song_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Song").then_some(*id))
            .expect("Song group should exist");

        let kick = state
            .voices
            .values()
            .find(|voice| voice.name == "kick")
            .expect("kick voice should exist");
        let snare = state
            .voices
            .values()
            .find(|voice| voice.name == "snare")
            .expect("snare voice should exist");
        let pad = state
            .voices
            .values()
            .find(|voice| voice.name == "pad")
            .expect("pad voice should exist");

        assert_eq!(kick.group, drums_id);
        assert_eq!(snare.group, drums_id);
        assert_eq!(pad.group, song_id);
        assert_eq!(
            state
                .groups
                .values()
                .filter(|config| config.name == "Drums")
                .count(),
            1,
            "resolved Drums handle should not be retargeted under Song"
        );
        assert_eq!(
            state
                .body_contributions
                .iter()
                .filter(|body| body.target_group == drums_id)
                .map(|body| body.target_path.as_str())
                .collect::<Vec<_>>(),
            vec!["main/Drums", "main/Drums"]
        );
    }

    #[test]
    fn test_repeated_group_body_records_deterministic_content_order() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Drums").gain(0.5).body(|| {
                let kick = voice("kick").synth("kick_synth");
                kick.output("out").to(group("dry"));
                fx("drums_comp").synth("compressor").apply();
            });

            group("Drums").gain(0.75).body(|| {
                let snare = voice("snare").synth("snare_synth");
                snare.output("out").to(group("wet"));
                fx("drums_verb").synth("reverb").apply();
                voice("kick").synth("kick_v2");
            });
        "#,
            )
            .unwrap();

        let drums_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Drums").then_some(*id))
            .expect("Drums group should exist");
        let drums = state.groups.get(&drums_id).unwrap();

        let voice_names = state
            .voice_order
            .iter()
            .map(|id| state.voices.get(id).unwrap().name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(voice_names, vec!["kick", "snare"]);
        assert_eq!(
            state.voices.get(&state.voice_order[0]).unwrap().synthdef,
            "kick_v2",
            "Duplicate voice names keep their order slot but use last config"
        );

        let effect_synthdefs = state
            .effect_order
            .iter()
            .map(|id| state.effects.get(id).unwrap().synthdef.as_str())
            .collect::<Vec<_>>();
        assert_eq!(effect_synthdefs, vec!["compressor", "reverb"]);
        let chain_synthdefs = drums
            .effects
            .iter()
            .map(|id| state.effects.get(id).unwrap().synthdef.as_str())
            .collect::<Vec<_>>();
        assert_eq!(chain_synthdefs, vec!["compressor", "reverb"]);

        let route_keys = state
            .route_order
            .iter()
            .map(|(voice_id, port)| {
                format!("{}:{}", state.voices.get(voice_id).unwrap().name, port)
            })
            .collect::<Vec<_>>();
        assert_eq!(route_keys, vec!["kick:out", "snare:out"]);
        assert_eq!(drums.params.get("amp"), Some(&0.75));
        assert_eq!(
            state
                .body_contributions
                .iter()
                .map(|body| body.ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn test_group_alias_registers_chained_aliases() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Drums")
                .alias("kit")
                .alias("beat")
                .gain(0.5);

            group("Drums").alias("kit");
        "#,
            )
            .unwrap();

        let kit = state
            .group_aliases
            .get("kit")
            .expect("kit alias should be registered");
        let beat = state
            .group_aliases
            .get("beat")
            .expect("beat alias should be registered");

        assert_eq!(kit.path, "main/Drums");
        assert_eq!(beat.path, "main/Drums");
        assert_eq!(kit.group_id, beat.group_id);
        assert_eq!(state.group_aliases.len(), 2);

        let drums = state.groups.get(&kit.group_id).unwrap();
        assert_eq!(drums.name, "Drums");
        assert_eq!(drums.params.get("amp"), Some(&0.5));
    }

    #[test]
    fn test_group_alias_uses_canonical_full_group_path() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Song").body(|| {
                group("Drums").alias("kit");
            });
        "#,
            )
            .unwrap();

        let target = state
            .group_aliases
            .get("kit")
            .expect("kit alias should be registered");

        assert_eq!(target.path, "main/Song/Drums");
        let drums = state.groups.get(&target.group_id).unwrap();
        assert_eq!(drums.name, "Drums");
    }

    #[test]
    fn test_group_alias_simple_lookup_uses_canonical_group() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Drums").alias("kit");

            group("kit").body(|| {
                voice("kick").synth("kick_synth");
            });
        "#,
            )
            .unwrap();

        let kit = state.group_aliases.get("kit").unwrap();
        let kick = state
            .voices
            .values()
            .find(|voice| voice.name == "kick")
            .expect("kick voice should exist");

        assert_eq!(kit.path, "main/Drums");
        assert_eq!(kick.group, kit.group_id);
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "simple alias lookup should not create a kit group"
        );
    }

    #[test]
    fn test_group_alias_conflicting_target_is_script_error() {
        let mut engine = ScriptEngine::new();
        let err = engine
            .execute(
                r#"
            group("Drums").alias("kit");
            group("Bass").alias("kit");
        "#,
            )
            .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("group alias 'kit' already points to"), "{msg}");
        assert!(msg.contains("main/Drums"), "{msg}");
        assert!(msg.contains("main/Bass"), "{msg}");
    }

    #[test]
    fn test_group_alias_invalid_name_is_script_error() {
        let mut engine = ScriptEngine::new();
        let err = engine
            .execute(r#"group("Drums").alias("main/kit");"#)
            .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("group alias 'main/kit' is invalid"), "{msg}");
        assert!(msg.contains("single relative names"), "{msg}");
    }

    #[test]
    fn test_group_alias_same_as_existing_canonical_root_name_is_allowed() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("main/Drums").gain(0.5);
            group("main/Drums").alias("Drums");
        "#,
            )
            .unwrap();

        let alias = state
            .group_aliases
            .get("Drums")
            .expect("Drums alias should be registered");
        let drums = state.groups.get(&alias.group_id).unwrap();

        assert_eq!(alias.path, "main/Drums");
        assert_eq!(drums.name, "Drums");
        assert_eq!(drums.params.get("amp"), Some(&0.5));
    }

    #[test]
    fn test_group_alias_conflicting_canonical_root_name_is_script_error() {
        let mut engine = ScriptEngine::new();
        let err = engine
            .execute(
                r#"
            group("main/Drums").gain(0.5);
            group("main/Bass").alias("Drums");
        "#,
            )
            .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("group alias 'Drums' collides with canonical group"),
            "{msg}"
        );
        assert!(msg.contains("main/Drums"), "{msg}");
        assert!(msg.contains("main/Bass"), "{msg}");
    }

    #[test]
    fn test_group_alias_reference_resolves_to_canonical_group() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Drums")
                .gain(0.5)
                .output(2)
                .alias("kit");

            group("Song").body(|| {
                group("kit").gain(0.25).body(|| {
                    voice("kick").synth("kick_synth");
                });
            });
        "#,
            )
            .unwrap();

        let alias_target = state.group_aliases.get("kit").unwrap();
        let drums = state.groups.get(&alias_target.group_id).unwrap();
        let kick = state
            .voices
            .values()
            .find(|voice| voice.name == "kick")
            .expect("kick voice should exist");

        assert_eq!(alias_target.path, "main/Drums");
        assert_eq!(drums.name, "Drums");
        assert!(drums.parent.is_some());
        assert_eq!(drums.output_bus, Some(2));
        assert_eq!(drums.output_channels, Some(1));
        assert_eq!(drums.params.get("amp"), Some(&0.25));
        assert_eq!(kick.group, alias_target.group_id);
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "alias lookup should not create a contextual kit group"
        );
    }

    #[test]
    fn test_group_alias_body_appends_to_canonical_group() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Drums").body(|| {
                voice("kick").synth("kick_synth");
            });

            group("Drums").alias("kit");

            group("Song").body(|| {
                group("kit").body(|| {
                    voice("snare").synth("snare_synth");
                });

                voice("pad").synth("pad_synth");
            });
        "#,
            )
            .unwrap();

        let alias_target = state.group_aliases.get("kit").unwrap();
        let song_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Song").then_some(*id))
            .expect("Song group should exist");

        let kick = state
            .voices
            .values()
            .find(|voice| voice.name == "kick")
            .expect("kick voice should exist");
        let snare = state
            .voices
            .values()
            .find(|voice| voice.name == "snare")
            .expect("snare voice should exist");
        let pad = state
            .voices
            .values()
            .find(|voice| voice.name == "pad")
            .expect("pad voice should exist");

        assert_eq!(alias_target.path, "main/Drums");
        assert_eq!(kick.group, alias_target.group_id);
        assert_eq!(snare.group, alias_target.group_id);
        assert_eq!(pad.group, song_id);
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "alias body should not create main/Song/kit"
        );
        assert_eq!(
            state
                .body_contributions
                .iter()
                .filter(|body| body.target_group == alias_target.group_id)
                .map(|body| body.target_path.as_str())
                .collect::<Vec<_>>(),
            vec!["main/Drums", "main/Drums"]
        );
    }

    #[test]
    fn test_group_alias_shadows_contextual_group_creation() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("main/Drums").alias("kit");

            group("Song").body(|| {
                group("kit").body(|| {
                    voice("snare").synth("snare_synth");
                });
            });
        "#,
            )
            .unwrap();

        let drums = state.group_aliases.get("kit").unwrap();
        let snare = state
            .voices
            .values()
            .find(|voice| voice.name == "snare")
            .expect("snare voice should exist");

        assert_eq!(drums.path, "main/Drums");
        assert_eq!(snare.group, drums.group_id);
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "alias should shadow main/Song/kit creation"
        );
        assert!(state
            .body_contributions
            .iter()
            .any(|body| body.target_group == drums.group_id && body.target_path == "main/Drums"));
    }

    #[test]
    fn test_group_alias_lookup_ignores_nested_current_group_context() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Drums").alias("kit");

            group("Song").body(|| {
                group("Verse").body(|| {
                    group("kit").body(|| {
                        voice("snare").synth("snare_synth");
                    });
                });
            });
        "#,
            )
            .unwrap();

        let kit = state.group_aliases.get("kit").unwrap();
        let snare = state
            .voices
            .values()
            .find(|voice| voice.name == "snare")
            .expect("snare voice should exist");

        assert_eq!(kit.path, "main/Drums");
        assert_eq!(snare.group, kit.group_id);
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "nested alias lookup should not create main/Song/Verse/kit"
        );
        assert!(state
            .body_contributions
            .iter()
            .any(|body| body.target_group == kit.group_id && body.target_path == "main/Drums"));
    }

    #[test]
    fn test_define_group_alias_enters_canonical_context() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Drums").alias("kit");

            group("Song").body(|| {
                define_group("kit", || {
                    voice("hat").synth("hat_synth");
                });
            });
        "#,
            )
            .unwrap();

        let drums = state.group_aliases.get("kit").unwrap();
        let hat = state
            .voices
            .values()
            .find(|voice| voice.name == "hat")
            .expect("hat voice should exist");

        assert_eq!(drums.path, "main/Drums");
        assert_eq!(hat.group, drums.group_id);
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "define_group should not create main/Song/kit for an alias"
        );
    }

    #[test]
    fn test_group_alias_through_alias_handle_stays_canonical() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Fx").alias("fx");
            group("fx").alias("send");
        "#,
            )
            .unwrap();

        let fx = state.group_aliases.get("fx").unwrap();
        let send = state.group_aliases.get("send").unwrap();

        assert_eq!(fx.path, "main/Fx");
        assert_eq!(send.path, "main/Fx");
        assert_eq!(fx.group_id, send.group_id);
    }

    #[test]
    fn test_group_alias_target_is_stable_across_reload_execution() {
        let first_script = r#"
            group("Drums").alias("kit");

            group("kit").body(|| {
                voice("kick").synth("kick_synth");
            });
        "#;
        let second_script = r#"
            group("Drums").gain(0.75).alias("kit");

            group("kit").body(|| {
                voice("snare").synth("snare_synth");
            });
        "#;

        let mut engine = ScriptEngine::new();
        let first = engine.execute(first_script).unwrap();
        let second = engine.execute(second_script).unwrap();

        let first_kit = first.group_aliases.get("kit").unwrap();
        let second_kit = second.group_aliases.get("kit").unwrap();
        let snare = second
            .voices
            .values()
            .find(|voice| voice.name == "snare")
            .expect("snare voice should exist");

        assert_eq!(first_kit.path, "main/Drums");
        assert_eq!(second_kit.path, "main/Drums");
        assert_eq!(first_kit.group_id, second_kit.group_id);
        assert_eq!(snare.group, second_kit.group_id);
        assert_eq!(
            second
                .groups
                .get(&second_kit.group_id)
                .unwrap()
                .params
                .get("amp"),
            Some(&0.75)
        );
    }

    #[test]
    fn test_group_alias_after_contextual_claim_conflict_is_script_error() {
        let mut engine = ScriptEngine::new();
        let err = engine
            .execute(
                r#"
            group("Song").body(|| {
                group("kit").gain(0.5);
            });

            group("Drums").alias("kit");
        "#,
            )
            .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("group alias 'kit' conflicts with prior contextual group claim"),
            "{msg}"
        );
        assert!(msg.contains("main/Song/kit"), "{msg}");
        assert!(msg.contains("main/Drums"), "{msg}");
    }

    #[test]
    fn test_group_body_uses_saved_handle_canonical_context() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            let drums = group("Drums");

            group("Song").body(|| {
                drums.body(|| {
                    voice("kick").synth("kick_synth");
                });

                voice("song_voice").synth("pad_synth");
            });
        "#,
            )
            .unwrap();

        let drums_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Drums").then_some(*id))
            .expect("Drums group should exist");
        let song_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Song").then_some(*id))
            .expect("Song group should exist");

        let kick = state
            .voices
            .values()
            .find(|voice| voice.name == "kick")
            .expect("kick voice should exist");
        let song_voice = state
            .voices
            .values()
            .find(|voice| voice.name == "song_voice")
            .expect("song voice should exist");

        assert_eq!(kick.group, drums_id);
        assert_eq!(song_voice.group, song_id);
        assert_eq!(
            state
                .groups
                .values()
                .filter(|config| config.name == "Drums")
                .count(),
            1,
            "Saved handle body should not create main/Song/Drums"
        );
    }

    #[test]
    fn test_group_body_uses_absolute_nested_handle_context() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            let room = group("main/Drums/Room");

            group("Other").body(|| {
                room.body(|| {
                    voice("verb").synth("reverb_synth");
                });

                voice("other_voice").synth("other_synth");
            });
        "#,
            )
            .unwrap();

        let room_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Room").then_some(*id))
            .expect("Room group should exist");
        let other_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Other").then_some(*id))
            .expect("Other group should exist");

        let verb = state
            .voices
            .values()
            .find(|voice| voice.name == "verb")
            .expect("verb voice should exist");
        let other_voice = state
            .voices
            .values()
            .find(|voice| voice.name == "other_voice")
            .expect("other voice should exist");

        assert_eq!(verb.group, room_id);
        assert_eq!(other_voice.group, other_id);
        assert_eq!(
            state
                .groups
                .values()
                .filter(|config| config.name == "Room")
                .count(),
            1,
            "Absolute nested handle should not create main/Other/Room"
        );
    }

    #[test]
    fn test_group_body_restores_context_after_inner_body_error() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Outer").body(|| {
                group("Inner").body(|| {
                    missing_function();
                });

                voice("outer_after").synth("pad_synth");
            });
        "#,
            )
            .unwrap();

        let outer_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Outer").then_some(*id))
            .expect("Outer group should exist");
        let inner_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Inner").then_some(*id))
            .expect("Inner group should exist");

        let outer_after = state
            .voices
            .values()
            .find(|voice| voice.name == "outer_after")
            .expect("outer_after voice should exist");

        assert_eq!(outer_after.group, outer_id);
        assert_ne!(outer_after.group, inner_id);
    }

    #[test]
    fn test_helper_functions() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            let amp = db(-6);
            let midi = note("C4");
            let beats = bars(2);
            set_tempo(120);
        "#,
            )
            .unwrap();

        assert_eq!(state.tempo, 120.0);
    }

    #[test]
    fn test_pattern_api() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(130);
            define_group("Drums", || {
                let kick = voice("kick").synth("kick_909").gain(db(-6));
                let hihat = voice("hihat").synth("hihat_909").gain(db(-10));

                let kick_ptn = pattern("kick_main")
                    .on(kick)
                    .step("x... x... x... x...")
                    .apply();

                let hihat_ptn = pattern("hihat_main")
                    .on(hihat)
                    .step("..x. ..x. ..x. ..x.")
                    .apply();
            });
        "#,
            )
            .unwrap();

        assert_eq!(state.tempo, 130.0);
        assert!(!state.groups.is_empty(), "Should have groups");
        assert_eq!(state.voices.len(), 2, "Should have 2 voices");
        assert_eq!(state.patterns.len(), 2, "Should have 2 patterns");
    }

    #[test]
    fn test_melody_api() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Synth", || {
                let lead = voice("lead").synth("saw_lead").gain(db(-8));

                let melody = melody("main_melody")
                    .on(lead)
                    .notes("C4 D4 E4 F4 | G4 A4 B4 C5")
                    .transpose(0)
                    .apply();
            });
        "#,
            )
            .unwrap();

        assert_eq!(state.tempo, 120.0);
        assert_eq!(state.voices.len(), 1, "Should have 1 voice");
        assert_eq!(state.melodies.len(), 1, "Should have 1 melody");

        // Check melody has notes
        let melody_id = state.melodies.keys().next().unwrap();
        let melody_config = state.melodies.get(melody_id).unwrap();
        assert!(
            melody_config.notes.len() >= 8,
            "Should have at least 8 notes"
        );
    }

    #[test]
    fn test_sequence_api() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(128);

            define_group("Drums", || {
                let kick = voice("kick").synth("kick_909");
                let kick_ptn = pattern("kick_main")
                    .on(kick)
                    .step("x... x... x... x...")
                    .apply();
            });

            let main_seq = sequence("arrangement")
                .loop_bars(8)
                .apply();
        "#,
            )
            .unwrap();

        assert_eq!(state.tempo, 128.0);
        assert_eq!(state.sequences.len(), 1, "Should have 1 sequence");
    }

    #[test]
    fn test_euclidean_pattern() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Drums", || {
                let perc = voice("perc").synth("perc_synth");

                // Euclidean rhythm: 5 hits in 16 steps
                let euclid_ptn = pattern("euclid_5_16")
                    .on(perc)
                    .euclid(5, 16)
                    .apply();
            });
        "#,
            )
            .unwrap();

        assert_eq!(state.patterns.len(), 1, "Should have 1 pattern");
        let pattern_id = state.patterns.keys().next().unwrap();
        let pattern_config = state.patterns.get(pattern_id).unwrap();
        assert_eq!(pattern_config.steps.len(), 5, "Should have 5 steps (hits)");
    }

    #[test]
    fn test_fx_api() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Synth", || {
                let lead = voice("lead").synth("saw_lead");

                // Add reverb effect
                fx("reverb")
                    .synth("reverb_fx")
                    .param("room", 0.8)
                    .param("mix", 0.3)
                    .apply();
            });
        "#,
            )
            .unwrap();

        assert_eq!(state.effects.len(), 1, "Should have 1 effect");
    }

    #[test]
    fn test_comprehensive_script() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            // Full minimal techno-style script
            set_tempo(130);
            set_time_signature(4, 4);

            // Drums group
            define_group("Drums", || {
                let kick = voice("kick").synth("kick_909").gain(db(-6));
                let hihat = voice("hihat").synth("hihat_909").gain(db(-10));
                let clap = voice("clap").synth("clap_909").gain(db(-8));

                pattern("kick_main").on(kick).step("x... .... x... ....").apply();
                pattern("hihat_main").on(hihat).step("..x. ..x. ..x. ..x.").apply();
                pattern("clap_main").on(clap).step(".... x... .... x...").apply();
            });

            // Bass group
            define_group("Bass", || {
                let bass = voice("bass").synth("acid_bass").gain(db(-4));
                melody("bassline").on(bass).notes("C2 . C2 . | C2 . E2 F2").apply();
            });

            // Lead group with effect
            define_group("Lead", || {
                let lead = voice("lead").synth("saw_lead").gain(db(-8));
                melody("lead_melody").on(lead).notes("G4 - - . | A4 - G4 F4").apply();

                fx("lead_reverb")
                    .synth("reverb_fx")
                    .param("room", 0.6)
                    .apply();
            });

            // Main arrangement
            sequence("main")
                .loop_bars(16)
                .apply();
        "#,
            )
            .unwrap();

        // Verify all components were created
        assert_eq!(state.tempo, 130.0);
        assert!(state.groups.len() >= 3, "Should have at least 3 groups");
        assert_eq!(state.voices.len(), 5, "Should have 5 voices");
        assert_eq!(state.patterns.len(), 3, "Should have 3 patterns");
        assert_eq!(state.melodies.len(), 2, "Should have 2 melodies");
        assert_eq!(state.effects.len(), 1, "Should have 1 effect");
        assert_eq!(state.sequences.len(), 1, "Should have 1 sequence");
    }

    // ==================== Phase 1 Tests: Voice Mute/Solo ====================

    #[test]
    fn test_voice_mute() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth").mute();
            });
        "#,
            )
            .unwrap();

        let voice_id = state.voices.keys().next().unwrap();
        let voice_config = state.voices.get(voice_id).unwrap();
        assert!(voice_config.muted, "Voice should be muted");
        assert!(!voice_config.soloed, "Voice should not be soloed");
    }

    #[test]
    fn test_voice_unmute() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth").mute().unmute();
            });
        "#,
            )
            .unwrap();

        let voice_id = state.voices.keys().next().unwrap();
        let voice_config = state.voices.get(voice_id).unwrap();
        assert!(
            !voice_config.muted,
            "Voice should not be muted after unmute"
        );
    }

    #[test]
    fn test_voice_solo() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth").solo();
            });
        "#,
            )
            .unwrap();

        let voice_id = state.voices.keys().next().unwrap();
        let voice_config = state.voices.get(voice_id).unwrap();
        assert!(voice_config.soloed, "Voice should be soloed");
        assert!(!voice_config.muted, "Voice should not be muted");
    }

    #[test]
    fn test_voice_unsolo() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth").solo().unsolo();
            });
        "#,
            )
            .unwrap();

        let voice_id = state.voices.keys().next().unwrap();
        let voice_config = state.voices.get(voice_id).unwrap();
        assert!(
            !voice_config.soloed,
            "Voice should not be soloed after unsolo"
        );
    }

    #[test]
    fn test_voice_mute_solo_chain() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                // Mute, then solo, then unmute - should be soloed but not muted
                let v = voice("test_voice").synth("test_synth").mute().solo().unmute();
            });
        "#,
            )
            .unwrap();

        let voice_id = state.voices.keys().next().unwrap();
        let voice_config = state.voices.get(voice_id).unwrap();
        assert!(!voice_config.muted, "Voice should not be muted");
        assert!(voice_config.soloed, "Voice should be soloed");
    }

    #[test]
    fn test_multiple_voices_mute_solo() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v1 = voice("voice1").synth("synth").mute();
                let v2 = voice("voice2").synth("synth").solo();
                let v3 = voice("voice3").synth("synth"); // Default: not muted, not soloed
            });
        "#,
            )
            .unwrap();

        assert_eq!(state.voices.len(), 3);

        let mut muted_count = 0;
        let mut soloed_count = 0;
        let mut default_count = 0;

        for voice_config in state.voices.values() {
            if voice_config.muted && !voice_config.soloed {
                muted_count += 1;
            } else if voice_config.soloed && !voice_config.muted {
                soloed_count += 1;
            } else if !voice_config.muted && !voice_config.soloed {
                default_count += 1;
            }
        }

        assert_eq!(muted_count, 1, "Should have 1 muted voice");
        assert_eq!(soloed_count, 1, "Should have 1 soloed voice");
        assert_eq!(default_count, 1, "Should have 1 default voice");
    }

    // ==================== Phase 1 Tests: Group Mute/Solo ====================

    #[test]
    fn test_group_mute() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("MutedGroup", || {
                let v = voice("test_voice").synth("test_synth");
            });
            group("MutedGroup").mute();
        "#,
            )
            .unwrap();

        // Find the group (not the main group)
        let group = state
            .groups
            .values()
            .find(|g| g.name == "MutedGroup")
            .expect("Should find MutedGroup");

        assert!(group.muted, "Group should be muted");
        assert!(!group.soloed, "Group should not be soloed");
    }

    #[test]
    fn test_group_unmute() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("TestGroup", || {
                let v = voice("test_voice").synth("test_synth");
            });
            group("TestGroup").mute().unmute();
        "#,
            )
            .unwrap();

        let group = state
            .groups
            .values()
            .find(|g| g.name == "TestGroup")
            .expect("Should find TestGroup");

        assert!(!group.muted, "Group should not be muted after unmute");
    }

    #[test]
    fn test_group_solo() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("SoloGroup", || {
                let v = voice("test_voice").synth("test_synth");
            });
            group("SoloGroup").solo(true);
        "#,
            )
            .unwrap();

        let group = state
            .groups
            .values()
            .find(|g| g.name == "SoloGroup")
            .expect("Should find SoloGroup");

        assert!(group.soloed, "Group should be soloed");
        assert!(!group.muted, "Group should not be muted");
    }

    #[test]
    fn test_group_solo_false() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("TestGroup", || {
                let v = voice("test_voice").synth("test_synth");
            });
            group("TestGroup").solo(true).solo(false);
        "#,
            )
            .unwrap();

        let group = state
            .groups
            .values()
            .find(|g| g.name == "TestGroup")
            .expect("Should find TestGroup");

        assert!(
            !group.soloed,
            "Group should not be soloed after solo(false)"
        );
    }

    #[test]
    fn test_group_set_param() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("TestGroup", || {
                let v = voice("test_voice").synth("test_synth");
            });
            group("TestGroup").set_param("filter_cutoff", 0.75);
        "#,
            )
            .unwrap();

        let group = state
            .groups
            .values()
            .find(|g| g.name == "TestGroup")
            .expect("Should find TestGroup");

        let param = group
            .params
            .get("filter_cutoff")
            .expect("Should have filter_cutoff param");
        assert!((param - 0.75).abs() < 0.001, "filter_cutoff should be 0.75");
    }

    #[test]
    fn test_group_gain() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("TestGroup", || {
                let v = voice("test_voice").synth("test_synth");
            });
            group("TestGroup").gain(0.5);
        "#,
            )
            .unwrap();

        let group = state
            .groups
            .values()
            .find(|g| g.name == "TestGroup")
            .expect("Should find TestGroup");

        let amp = group.params.get("amp").expect("Should have amp param");
        assert!((amp - 0.5).abs() < 0.001, "amp should be 0.5");
    }

    // ==================== Phase 1 Tests: Pattern/Melody/Sequence is_playing ====================

    #[test]
    fn test_pattern_start_is_playing() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let p = pattern("test_pattern").on(v).step("x...").start();
            });
        "#,
            )
            .unwrap();

        assert_eq!(
            state.playing_patterns.len(),
            1,
            "Should have 1 playing pattern"
        );
    }

    #[test]
    fn test_pattern_stop_not_playing() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let p = pattern("test_pattern").on(v).step("x...");
                p.start();
                p.stop();
            });
        "#,
            )
            .unwrap();

        assert_eq!(
            state.playing_patterns.len(),
            0,
            "Should have 0 playing patterns after stop"
        );
    }

    #[test]
    fn test_pattern_launch() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            set_quantization(4.0); // Quantize to 1 bar
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let p = pattern("test_pattern").on(v).step("x...").launch();
            });
        "#,
            )
            .unwrap();

        // launch() should start the pattern
        assert_eq!(
            state.playing_patterns.len(),
            1,
            "Should have 1 playing pattern after launch"
        );
    }

    #[test]
    fn test_melody_start_is_playing() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let m = melody("test_melody").on(v).notes("C4 D4 E4 F4").start();
            });
        "#,
            )
            .unwrap();

        assert_eq!(
            state.playing_melodies.len(),
            1,
            "Should have 1 playing melody"
        );
    }

    #[test]
    fn test_melody_stop_not_playing() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let m = melody("test_melody").on(v).notes("C4 D4 E4 F4");
                m.start();
                m.stop();
            });
        "#,
            )
            .unwrap();

        assert_eq!(
            state.playing_melodies.len(),
            0,
            "Should have 0 playing melodies after stop"
        );
    }

    #[test]
    fn test_melody_launch() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            set_quantization(4.0);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let m = melody("test_melody").on(v).notes("C4 D4 E4 F4").launch();
            });
        "#,
            )
            .unwrap();

        assert_eq!(
            state.playing_melodies.len(),
            1,
            "Should have 1 playing melody after launch"
        );
    }

    #[test]
    fn test_sequence_start_is_playing() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            let seq = sequence("test_sequence").loop_bars(4).apply();
            seq.start();
        "#,
            )
            .unwrap();

        assert_eq!(
            state.playing_sequences.len(),
            1,
            "Should have 1 playing sequence"
        );
    }

    #[test]
    fn test_sequence_stop_not_playing() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            let seq = sequence("test_sequence").loop_bars(4).apply();
            seq.start();
            seq.stop();
        "#,
            )
            .unwrap();

        assert_eq!(
            state.playing_sequences.len(),
            0,
            "Should have 0 playing sequences after stop"
        );
    }

    #[test]
    fn test_sequence_launch() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            set_quantization(4.0);
            let seq = sequence("test_sequence").loop_bars(4).apply();
            seq.launch();
        "#,
            )
            .unwrap();

        assert_eq!(
            state.playing_sequences.len(),
            1,
            "Should have 1 playing sequence after launch"
        );
    }

    #[test]
    fn test_multiple_patterns_playing() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                pattern("p1").on(v).step("x...").start();
                pattern("p2").on(v).step(".x..").start();
                pattern("p3").on(v).step("..x.").apply(); // apply only, not start
            });
        "#,
            )
            .unwrap();

        assert_eq!(state.patterns.len(), 3, "Should have 3 patterns defined");
        assert_eq!(
            state.playing_patterns.len(),
            2,
            "Should have 2 playing patterns"
        );
    }

    // ==================== Phase 1 Tests: Quantization ====================

    #[test]
    fn test_set_quantization() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            set_quantization(4.0); // Quantize to 1 bar (4 beats)
        "#,
            )
            .unwrap();

        assert!(
            (state.quantization - 4.0).abs() < 0.001,
            "Quantization should be 4.0"
        );
    }

    #[test]
    fn test_quantization_default() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
        "#,
            )
            .unwrap();

        // Default quantization should be 0 (no quantization)
        assert!(
            (state.quantization - 0.0).abs() < 0.001,
            "Default quantization should be 0.0"
        );
    }

    #[test]
    fn test_quantization_various_values() {
        let mut engine = ScriptEngine::new();

        // Test various quantization values
        let test_cases = vec![
            (0.25, "16th note"),
            (0.5, "8th note"),
            (1.0, "quarter note"),
            (2.0, "half note"),
            (4.0, "1 bar"),
            (8.0, "2 bars"),
            (16.0, "4 bars"),
        ];

        for (value, description) in test_cases {
            let script = format!("set_tempo(120); set_quantization({});", value);
            let state = engine.execute(&script).unwrap();
            assert!(
                (state.quantization - value).abs() < 0.001,
                "Quantization should be {} for {}",
                value,
                description
            );
        }
    }

    // ==================== Phase 1 Tests: Chord and Scale Functions ====================

    #[test]
    fn test_chord_function_in_script() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                // Use chord function to get notes
                let c_major = chord("C");
                let m = melody("chord_melody").on(v).notes(c_major).apply();
            });
        "#,
            )
            .unwrap();

        let melody_config = state
            .melodies
            .values()
            .next()
            .expect("Should have a melody");
        // C major chord has 3 notes
        assert!(
            melody_config.notes.len() >= 3,
            "Melody should have at least 3 notes from chord"
        );
    }

    #[test]
    fn test_scale_function_in_script() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                // Use scale function to get notes
                let c_scale = scale("C", "major");
                let m = melody("scale_melody").on(v).notes(c_scale).apply();
            });
        "#,
            )
            .unwrap();

        let melody_config = state
            .melodies
            .values()
            .next()
            .expect("Should have a melody");
        // C major scale has 7 notes
        assert!(
            melody_config.notes.len() >= 7,
            "Melody should have at least 7 notes from scale"
        );
    }

    #[test]
    fn test_scale_degree_in_script() {
        let mut engine = ScriptEngine::new();

        // This test verifies scale_degree can be used in scripts
        // We can't easily check the actual note values, but we can ensure it doesn't error
        let result = engine.execute(
            r#"
            set_tempo(120);
            let degree_1 = scale_degree("C", "major", 1);
            let degree_5 = scale_degree("C", "major", 5);
        "#,
        );

        assert!(
            result.is_ok(),
            "Script with scale_degree should execute without errors"
        );
    }

    // ==================== Phase 1 Tests: Melody Array Input ====================

    #[test]
    fn test_melody_add_note() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let m = melody("note_melody")
                    .on(v)
                    .add_note(0.0, 60, 1.0, 0.5)    // C4 at beat 0
                    .add_note(1.0, 62, 0.8, 0.5)    // D4 at beat 1
                    .add_note(2.0, 64, 0.9, 0.5)    // E4 at beat 2
                    .apply();
            });
        "#,
            )
            .unwrap();

        let melody_config = state
            .melodies
            .values()
            .next()
            .expect("Should have a melody");
        assert_eq!(melody_config.notes.len(), 3, "Melody should have 3 notes");
    }

    #[test]
    fn test_melody_add_chord_at_beat() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let c_chord = chord("C"); // [60, 64, 67]
                let m = melody("chord_at_beat")
                    .on(v)
                    .add_chord(0.0, c_chord, 1.0, 2.0)
                    .apply();
            });
        "#,
            )
            .unwrap();

        let melody_config = state
            .melodies
            .values()
            .next()
            .expect("Should have a melody");
        // Each note in the chord becomes a separate note event
        assert_eq!(
            melody_config.notes.len(),
            3,
            "Melody should have 3 notes (C major chord)"
        );
    }

    // ==================== Per-Note Parameters Tests ====================

    #[test]
    fn test_melody_with_per_note_velocity() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                melody("vel_test")
                    .on(v)
                    .notes("C4[velocity=100] D4[vel=50] E4 F4[vel=0.3]")
                    .apply();
            });
        "#,
            )
            .unwrap();

        let melody_config = state
            .melodies
            .values()
            .next()
            .expect("Should have a melody");

        assert_eq!(melody_config.notes.len(), 4, "Should have 4 notes");

        // C4: velocity=100 (MIDI) → ~0.787
        let vel_c4 = melody_config.notes[0].velocity;
        assert!(
            (vel_c4 - 100.0 / 127.0).abs() < 0.01,
            "C4 velocity should be 100/127, got {}",
            vel_c4
        );

        // D4: vel=50 (MIDI) → ~0.394
        let vel_d4 = melody_config.notes[1].velocity;
        assert!(
            (vel_d4 - 50.0 / 127.0).abs() < 0.01,
            "D4 velocity should be 50/127, got {}",
            vel_d4
        );

        // E4: no override → default 1.0
        let vel_e4 = melody_config.notes[2].velocity;
        assert!(
            (vel_e4 - 1.0).abs() < 0.01,
            "E4 velocity should be 1.0, got {}",
            vel_e4
        );

        // F4: vel=0.3 → 0.3
        let vel_f4 = melody_config.notes[3].velocity;
        assert!(
            (vel_f4 - 0.3).abs() < 0.01,
            "F4 velocity should be 0.3, got {}",
            vel_f4
        );
    }

    #[test]
    fn test_melody_with_per_note_synth_params() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                melody("params_test")
                    .on(v)
                    .notes("C4[cutoff=2000,resonance=0.8] D4 E4[pan=-0.5]")
                    .apply();
            });
        "#,
            )
            .unwrap();

        let melody_config = state
            .melodies
            .values()
            .next()
            .expect("Should have a melody");

        assert_eq!(melody_config.notes.len(), 3, "Should have 3 notes");

        // C4: cutoff + resonance
        assert_eq!(melody_config.notes[0].params.len(), 2);
        assert!(
            (melody_config.notes[0].params["cutoff"] - 2000.0).abs() < 0.01,
            "C4 should have cutoff=2000"
        );
        assert!(
            (melody_config.notes[0].params["resonance"] - 0.8).abs() < 0.01,
            "C4 should have resonance=0.8"
        );

        // D4: no params
        assert!(
            melody_config.notes[1].params.is_empty(),
            "D4 should have no per-note params"
        );

        // E4: pan
        assert_eq!(melody_config.notes[2].params.len(), 1);
        assert!(
            (melody_config.notes[2].params["pan"] - (-0.5)).abs() < 0.01,
            "E4 should have pan=-0.5"
        );
    }

    #[test]
    fn test_melody_with_per_note_params_and_scale_degrees() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                melody("scale_params")
                    .on(v)
                    .root("C4")
                    .scale("minor")
                    .notes("1[vel=100] 3[cutoff=2000] 5 7[vel=50,pan=0.5]")
                    .apply();
            });
        "#,
            )
            .unwrap();

        let melody_config = state
            .melodies
            .values()
            .next()
            .expect("Should have a melody");

        assert_eq!(melody_config.notes.len(), 4, "Should have 4 notes");

        // Degree 1: velocity override
        let vel_1 = melody_config.notes[0].velocity;
        assert!(
            (vel_1 - 100.0 / 127.0).abs() < 0.01,
            "Degree 1 velocity should be 100/127"
        );

        // Degree 3: cutoff param
        assert!(!melody_config.notes[1].params.is_empty());
        assert!(
            (melody_config.notes[1].params["cutoff"] - 2000.0).abs() < 0.01,
            "Degree 3 should have cutoff=2000"
        );

        // Degree 5: no params
        assert!(melody_config.notes[2].params.is_empty());

        // Degree 7: velocity + pan
        let vel_7 = melody_config.notes[3].velocity;
        assert!(
            (vel_7 - 50.0 / 127.0).abs() < 0.01,
            "Degree 7 velocity should be 50/127"
        );
        assert!(
            (melody_config.notes[3].params["pan"] - 0.5).abs() < 0.01,
            "Degree 7 should have pan=0.5"
        );
    }

    #[test]
    fn test_melody_with_per_note_params_multi_bar() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                melody("multi_bar_params")
                    .on(v)
                    .notes("C4[cutoff=2000] D4 E4 F4 | G4[cutoff=500] A4 B4 C5")
                    .apply();
            });
        "#,
            )
            .unwrap();

        let melody_config = state
            .melodies
            .values()
            .next()
            .expect("Should have a melody");

        assert_eq!(melody_config.notes.len(), 8, "Should have 8 notes");

        // C4 in bar 1: cutoff=2000
        assert!(
            (melody_config.notes[0].params["cutoff"] - 2000.0).abs() < 0.01,
            "C4 should have cutoff=2000"
        );

        // G4 in bar 2: cutoff=500
        assert!(
            (melody_config.notes[4].params["cutoff"] - 500.0).abs() < 0.01,
            "G4 should have cutoff=500"
        );

        // Other notes: no params
        for i in [1, 2, 3, 5, 6, 7] {
            assert!(
                melody_config.notes[i].params.is_empty(),
                "Note at index {} should have no params",
                i
            );
        }
    }

    #[test]
    fn test_melody_without_params_backwards_compatible() {
        // Verify that normal melodies (no per-note params) still work exactly the same
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                melody("compat_test")
                    .on(v)
                    .notes("C4 D4 E4 F4 | G4 A4 B4 C5")
                    .apply();
            });
        "#,
            )
            .unwrap();

        let melody_config = state
            .melodies
            .values()
            .next()
            .expect("Should have a melody");

        assert_eq!(melody_config.notes.len(), 8, "Should have 8 notes");

        // All notes should have empty params and default velocity
        for (i, note) in melody_config.notes.iter().enumerate() {
            assert!(note.params.is_empty(), "Note {} should have no params", i);
            assert!(
                (note.velocity - 1.0).abs() < 0.01,
                "Note {} should have default velocity 1.0",
                i
            );
        }
    }

    // ==================== Phase 1 Tests: Edge Cases and Error Handling ====================

    #[test]
    fn test_voice_default_state() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth").apply();
            });
        "#,
            )
            .unwrap();

        let voice_config = state.voices.values().next().expect("Should have a voice");
        assert!(!voice_config.muted, "Voice should not be muted by default");
        assert!(
            !voice_config.soloed,
            "Voice should not be soloed by default"
        );
    }

    #[test]
    fn test_group_default_state() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("TestGroup", || {
                let v = voice("test_voice").synth("test_synth");
            });
        "#,
            )
            .unwrap();

        let group = state
            .groups
            .values()
            .find(|g| g.name == "TestGroup")
            .expect("Should find TestGroup");

        assert!(!group.muted, "Group should not be muted by default");
        assert!(!group.soloed, "Group should not be soloed by default");
    }

    #[test]
    fn test_pattern_apply_not_playing() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let p = pattern("test_pattern").on(v).step("x...").apply();
            });
        "#,
            )
            .unwrap();

        // apply() should NOT start the pattern
        assert_eq!(
            state.playing_patterns.len(),
            0,
            "Pattern should not be playing after apply()"
        );
    }

    #[test]
    fn test_melody_apply_not_playing() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            define_group("Test", || {
                let v = voice("test_voice").synth("test_synth");
                let m = melody("test_melody").on(v).notes("C4 D4 E4").apply();
            });
        "#,
            )
            .unwrap();

        // apply() should NOT start the melody
        assert_eq!(
            state.playing_melodies.len(),
            0,
            "Melody should not be playing after apply()"
        );
    }

    #[test]
    fn test_negative_quantization_clamped() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            set_quantization(-5.0); // Negative should be clamped to 0
        "#,
            )
            .unwrap();

        assert!(state.quantization >= 0.0, "Quantization should be >= 0");
    }

    #[test]
    fn test_combined_phase1_features() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            // Test all Phase 1 features together
            set_tempo(130);
            set_time_signature(4, 4);
            set_quantization(4.0);

            // Drums group - muted
            define_group("Drums", || {
                let kick = voice("kick").synth("kick_909").gain(db(-6));
                let hihat = voice("hihat").synth("hihat_909").mute(); // Muted

                pattern("kick_main").on(kick).step("x... x...").start();
                pattern("hihat_main").on(hihat).step("..x. ..x.").launch();
            });
            group("Drums").set_param("compressor", 0.6);

            // Bass group - soloed
            define_group("Bass", || {
                let bass = voice("bass").synth("acid_bass").solo(); // Soloed

                // Use scale for the bassline
                let notes = scale("C", "minor");
                melody("bassline").on(bass).notes(notes).start();
            });
            group("Bass").solo(true);

            // Lead group
            define_group("Lead", || {
                let lead = voice("lead").synth("saw_lead");

                // Use add_note for precise control
                melody("lead_melody")
                    .on(lead)
                    .add_note(0.0, 60, 1.0, 2.0)
                    .add_note(2.0, 67, 0.8, 1.0)
                    .launch();
            });
        "#,
            )
            .unwrap();

        // Verify everything was set up correctly
        assert_eq!(state.tempo, 130.0);
        assert!((state.quantization - 4.0).abs() < 0.001);

        // Check groups
        assert!(state.groups.len() >= 3);
        let bass_group = state.groups.values().find(|g| g.name == "Bass");
        assert!(bass_group.is_some());
        assert!(bass_group.unwrap().soloed);

        // Check voices
        assert_eq!(state.voices.len(), 4);

        // Check patterns and melodies are playing
        assert_eq!(state.playing_patterns.len(), 2);
        assert_eq!(state.playing_melodies.len(), 2);
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_midi_api() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);

            // Get a MIDI device (by index for testing)
            let keyboard = midi_device(0);

            define_group("Synth", || {
                let lead = voice("lead").synth("saw_lead").gain(db(-8));

                // Route keyboard to voice
                keyboard.route_to(lead);

                // Route CC 1 (mod wheel) to filter cutoff
                keyboard.route_cc(1, lead, "filter_cutoff", 0.0, 1.0);
            });

            // Open the device for input
            keyboard.open_input();
        "#,
            )
            .unwrap();

        assert_eq!(state.tempo, 120.0);
        assert_eq!(state.voices.len(), 1, "Should have 1 voice");

        // Check MIDI routing was created
        assert_eq!(
            state.midi_keyboard_routes.len(),
            1,
            "Should have 1 keyboard route"
        );
        assert_eq!(state.midi_cc_routes.len(), 1, "Should have 1 CC route");
        assert_eq!(state.midi_inputs.len(), 1, "Should have 1 MIDI input");
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_midi_output_voice() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);

            // Get a MIDI device for output
            let synth = midi_device(0);

            define_group("External", || {
                // Create a voice that outputs to MIDI instead of audio
                let ext_synth = voice("external_synth")
                    .on(synth)          // Routes to MIDI device
                    .channel(2)         // MIDI channel 3 (0-indexed)
                    .gain(db(0));

                // Create a melody that will be sent as MIDI
                melody("ext_melody").on(ext_synth).notes("C3 D3 E3 F3").apply();
            });
        "#,
            )
            .unwrap();

        assert_eq!(state.tempo, 120.0);
        assert_eq!(state.voices.len(), 1, "Should have 1 voice");

        // Check the voice has MIDI output configured
        let voice_id = state.voices.keys().next().unwrap();
        let voice_config = state.voices.get(voice_id).unwrap();
        assert!(
            voice_config.midi_output.is_some(),
            "Voice should have MIDI output"
        );
        assert_eq!(voice_config.midi_channel, 2, "Voice should be on channel 2");
    }

    // === Phase 3: Sample API tests ===

    #[test]
    fn test_sample_envelope_methods() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);

            // Test sample with envelope settings
            let kick = sample("kick", "samples/kick.wav")
                .attack(0.01)
                .sustain(0.8)
                .release(0.2);
        "#,
            )
            .unwrap();

        assert_eq!(state.samples.len(), 1, "Should have 1 sample");
        let sample_id = state.samples.keys().next().unwrap();
        let sample_config = state.samples.get(sample_id).unwrap();
        assert!(
            (sample_config.attack - 0.01).abs() < 0.001,
            "Attack should be 0.01"
        );
        assert!(
            (sample_config.sustain - 0.8).abs() < 0.001,
            "Sustain should be 0.8"
        );
        assert!(
            (sample_config.release - 0.2).abs() < 0.001,
            "Release should be 0.2"
        );
    }

    #[test]
    fn test_sample_playback_methods() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);

            // Test sample with playback settings
            let loop_sample = sample("loop", "samples/loop.wav")
                .amp(0.7)
                .rate(1.5)
                .loop_mode(true)
                .offset(0.5)
                .length(2.0);
        "#,
            )
            .unwrap();

        assert_eq!(state.samples.len(), 1, "Should have 1 sample");
        let sample_id = state.samples.keys().next().unwrap();
        let sample_config = state.samples.get(sample_id).unwrap();
        assert!((sample_config.amp - 0.7).abs() < 0.001, "Amp should be 0.7");
        assert!(
            (sample_config.rate - 1.5).abs() < 0.001,
            "Rate should be 1.5"
        );
        assert!(sample_config.loop_mode, "Loop mode should be true");
        assert!(
            (sample_config.offset - 0.5).abs() < 0.001,
            "Offset should be 0.5"
        );
        assert_eq!(sample_config.length, Some(2.0), "Length should be 2.0");
    }

    #[test]
    fn test_sample_warp_methods() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);

            // Test sample with warp/time-stretch settings
            let warped = sample("warped", "samples/loop.wav")
                .warp(true)
                .speed(0.5)
                .pitch(1.2)
                .window_size(0.15)
                .overlaps(12.0);
        "#,
            )
            .unwrap();

        assert_eq!(state.samples.len(), 1, "Should have 1 sample");
        let sample_id = state.samples.keys().next().unwrap();
        let sample_config = state.samples.get(sample_id).unwrap();
        assert!(sample_config.warp, "Warp should be true");
        assert!(
            (sample_config.speed - 0.5).abs() < 0.001,
            "Speed should be 0.5"
        );
        assert!(
            (sample_config.pitch - 1.2).abs() < 0.001,
            "Pitch should be 1.2"
        );
        assert!(
            (sample_config.window_size - 0.15).abs() < 0.001,
            "Window size should be 0.15"
        );
        assert!(
            (sample_config.overlaps - 12.0).abs() < 0.001,
            "Overlaps should be 12.0"
        );
    }

    #[test]
    fn test_sample_semitones() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);

            // Test pitch shift by semitones
            let shifted = sample("shifted", "samples/loop.wav")
                .semitones(12.0);  // One octave up
        "#,
            )
            .unwrap();

        assert_eq!(state.samples.len(), 1, "Should have 1 sample");
        let sample_id = state.samples.keys().next().unwrap();
        let sample_config = state.samples.get(sample_id).unwrap();
        assert!(sample_config.warp, "Warp should be auto-enabled");
        // 12 semitones = 2^(12/12) = 2.0 (octave up)
        assert!(
            (sample_config.pitch - 2.0).abs() < 0.001,
            "Pitch should be 2.0 for +12 semitones"
        );
    }

    #[test]
    fn test_sample_warp_to_bpm() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);

            // Test warp to BPM
            let tempo_matched = sample("tempo", "samples/loop.wav")
                .warp_to_bpm(140.0);
        "#,
            )
            .unwrap();

        assert_eq!(state.samples.len(), 1, "Should have 1 sample");
        let sample_id = state.samples.keys().next().unwrap();
        let sample_config = state.samples.get(sample_id).unwrap();
        assert!(sample_config.warp, "Warp should be auto-enabled");
        assert_eq!(
            sample_config.target_bpm,
            Some(140.0),
            "Target BPM should be 140"
        );
    }

    #[test]
    fn test_sample_slice() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);

            // Test sample slicing
            let sliced = sample("sliced", "samples/loop.wav")
                .slice(1.0, 3.0);  // 2-second slice starting at 1 second
        "#,
            )
            .unwrap();

        assert_eq!(state.samples.len(), 1, "Should have 1 sample");
        let sample_id = state.samples.keys().next().unwrap();
        let sample_config = state.samples.get(sample_id).unwrap();
        assert!(
            (sample_config.offset - 1.0).abs() < 0.001,
            "Offset should be 1.0"
        );
        assert_eq!(
            sample_config.length,
            Some(2.0),
            "Length should be 2.0 (3.0 - 1.0)"
        );
    }

    #[test]
    fn test_sample_chained_methods() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);

            // Test chaining multiple sample methods
            let full_sample = sample("full", "samples/loop.wav")
                .attack(0.005)
                .release(0.1)
                .amp(0.9)
                .warp(true)
                .speed(0.75)
                .pitch(1.1)
                .loop_mode(true);
        "#,
            )
            .unwrap();

        assert_eq!(state.samples.len(), 1, "Should have 1 sample");
        let sample_id = state.samples.keys().next().unwrap();
        let sample_config = state.samples.get(sample_id).unwrap();
        assert!((sample_config.attack - 0.005).abs() < 0.001);
        assert!((sample_config.release - 0.1).abs() < 0.001);
        assert!((sample_config.amp - 0.9).abs() < 0.001);
        assert!(sample_config.warp);
        assert!((sample_config.speed - 0.75).abs() < 0.001);
        assert!((sample_config.pitch - 1.1).abs() < 0.001);
        assert!(sample_config.loop_mode);
    }

    // ==================== MIDI Routing Tests: map_cc, keys, pad ====================

    #[cfg(feature = "midi")]
    #[test]
    fn test_map_cc_to_voice() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let synth = voice("synth");
                let dev = midi_device("test-device");
                dev.map_cc(14).to(synth, "cutoff", 200.0, 8000.0);
            "#,
            )
            .unwrap();

        assert_eq!(state.advanced_cc_routes.len(), 1, "Should have 1 CC route");
        let route = &state.advanced_cc_routes[0];
        assert_eq!(route.cc, 14, "CC number should be 14");
        assert_eq!(route.curve, "linear", "Default curve should be linear");
        assert_eq!(route.channel, None, "No channel filter by default");
        assert_eq!(route.param, "cutoff", "Param should be cutoff");
        assert!((route.min - 200.0f32).abs() < 0.01, "Min should be 200.0");
        assert!((route.max - 8000.0f32).abs() < 0.01, "Max should be 8000.0");
        assert!(
            matches!(route.target, vibelang_core::traits::FadeTarget::Voice(_)),
            "Target should be a Voice"
        );
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_map_cc_to_group() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let synths = define_group("Synths", || {});
                let dev = midi_device("test-device");
                dev.map_cc(15).to(synths, "amp", 0.0, 1.0);
            "#,
            )
            .unwrap();

        assert_eq!(state.advanced_cc_routes.len(), 1, "Should have 1 CC route");
        let route = &state.advanced_cc_routes[0];
        assert_eq!(route.cc, 15, "CC number should be 15");
        assert_eq!(route.param, "amp", "Param should be amp");
        assert!((route.min - 0.0f32).abs() < 0.001, "Min should be 0.0");
        assert!((route.max - 1.0f32).abs() < 0.001, "Max should be 1.0");
        assert!(
            matches!(route.target, vibelang_core::traits::FadeTarget::Group(_)),
            "Target should be a Group"
        );
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_map_cc_to_string_target() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let dev = midi_device("test-device");
                dev.map_cc(16).to("my_voice", "amp", 0.0, 1.0);
            "#,
            )
            .unwrap();

        assert_eq!(state.advanced_cc_routes.len(), 1, "Should have 1 CC route");
        let route = &state.advanced_cc_routes[0];
        assert_eq!(route.cc, 16, "CC number should be 16");
        assert_eq!(route.param, "amp", "Param should be amp");
        assert!(
            matches!(route.target, vibelang_core::traits::FadeTarget::Voice(_)),
            "String target defaults to Voice"
        );
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_map_cc_with_channel_and_curve() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let synth = voice("synth");
                let dev = midi_device("test-device");
                dev.map_cc(17).channel(1).curve("log").to(synth, "cutoff", 200.0, 8000.0);
            "#,
            )
            .unwrap();

        assert_eq!(state.advanced_cc_routes.len(), 1, "Should have 1 CC route");
        let route = &state.advanced_cc_routes[0];
        assert_eq!(route.cc, 17, "CC number should be 17");
        assert_eq!(route.channel, Some(0), "Channel 1 (user-facing) stored as 0 (internal)");
        assert_eq!(route.curve, "logarithmic", "\"log\" maps to \"logarithmic\"");
        assert_eq!(route.param, "cutoff", "Param should be cutoff");
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_keys_builder() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let synth = voice("synth");
                let dev = midi_device("test-device");
                dev.keys().range("C2", "C6").velocity("soft").to(synth);
            "#,
            )
            .unwrap();

        assert_eq!(
            state.advanced_keyboard_routes.len(),
            1,
            "Should have 1 keyboard route"
        );
        let route = &state.advanced_keyboard_routes[0];
        assert_eq!(route.note_min, 36, "C2 = MIDI note 36");
        assert_eq!(route.note_max, 84, "C6 = MIDI note 84");
        assert_eq!(route.velocity_curve, "soft", "Velocity curve should be soft");
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_pad_builder() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let hat = voice("hat");
                let dev = midi_device("test-device");
                dev.pad("C2").choke("hats").to(hat);
            "#,
            )
            .unwrap();

        assert_eq!(
            state.advanced_note_routes.len(),
            1,
            "Should have 1 note route"
        );
        let route = &state.advanced_note_routes[0];
        assert_eq!(route.source_note, 36, "C2 = MIDI note 36");
        assert_eq!(
            route.choke_group,
            Some("hats".to_string()),
            "Choke group should be hats"
        );
    }

    /// `voice("name")` without a source must not overwrite a configured voice when used
    /// only as a handle (e.g. inside `pad(...).to(voice("name"))`).
    #[cfg(feature = "midi")]
    #[test]
    fn test_pad_to_voice_preserves_prior_synth_config() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                voice("kick").synth("sample_voice").apply();
                let dev = midi_device("test-device");
                dev.pad(36).to(voice("kick"));
            "#,
            )
            .unwrap();

        assert_eq!(state.advanced_note_routes.len(), 1);
        assert_eq!(state.voices.len(), 1, "Bare voice() in .to() must not clobber .synth()");
        let cfg = state.voices.values().next().unwrap();
        assert_eq!(cfg.synthdef, "sample_voice");
    }

    /// Route may reference a voice by name before the full `voice(...).synth(...).apply()` line.
    #[cfg(feature = "midi")]
    #[test]
    fn test_pad_to_voice_then_define_voice() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let dev = midi_device("test-device");
                dev.pad(36).to(voice("kick"));
                voice("kick").synth("sample_voice").apply();
            "#,
            )
            .unwrap();

        assert_eq!(state.advanced_note_routes.len(), 1);
        assert_eq!(state.voices.len(), 1);
        assert_eq!(state.voices.values().next().unwrap().synthdef, "sample_voice");
    }
}
