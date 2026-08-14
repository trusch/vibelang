//! Script engine - the main entry point for executing VibeLang scripts.
//!
//! The [`ScriptEngine`] wraps a Rhai engine with all VibeLang API functions registered.

use rhai::Engine;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use vibelang_core::candidate::{
    Candidate, ContractDigest, EngineInstanceId, EvaluationIdentity, LanguageContract,
};
use vibelang_core::mutation::{
    Atomicity, CandidateOrigin, FailurePhase, MutationEventSink, MutationKind, MutationReceipt,
    MutationReplySink, MutationSource, RequestMaterial, Submission, SupersessionPolicy,
};
use vibelang_core::reload::ScriptState;
use vibelang_core::{MutationAttempt, ReloadMessage, RuntimeHandle};

use crate::api;
use crate::context;
use crate::error::{Error, Result};
use crate::foundation;
#[cfg(not(target_arch = "wasm32"))]
use crate::version::select_import_language;
use crate::version::{select_language, LanguageSelectionError, LanguageVersion};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct VersionedFileModuleResolver {
    inner: rhai::module_resolvers::FileModuleResolver,
    importer: LanguageVersion,
}

#[cfg(not(target_arch = "wasm32"))]
impl VersionedFileModuleResolver {
    fn new(base_path: Option<PathBuf>, importer: LanguageVersion) -> Self {
        let mut inner = rhai::module_resolvers::FileModuleResolver::new();
        if let Some(base_path) = base_path {
            inner.set_base_path(base_path);
        }
        inner.set_extension("vibe");
        Self { inner, importer }
    }

    fn validate(
        &self,
        source: Option<&str>,
        path: &str,
        pos: rhai::Position,
    ) -> std::result::Result<(), Box<rhai::EvalAltResult>> {
        let source_path = source.and_then(|source| Path::new(source).parent());
        let file_path = self.inner.get_file_path(path, source_path);
        let Ok(module_source) = std::fs::read_to_string(file_path) else {
            return Ok(());
        };
        select_import_language(&module_source, self.importer).map_err(|error| {
            Box::new(rhai::EvalAltResult::ErrorInModule(
                path.to_string(),
                Box::new(rhai::EvalAltResult::ErrorRuntime(
                    error.to_string().into(),
                    pos,
                )),
                pos,
            ))
        })?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl rhai::module_resolvers::ModuleResolver for VersionedFileModuleResolver {
    fn resolve(
        &self,
        engine: &Engine,
        source: Option<&str>,
        path: &str,
        pos: rhai::Position,
    ) -> std::result::Result<rhai::Shared<rhai::Module>, Box<rhai::EvalAltResult>> {
        self.validate(source, path, pos)?;
        self.inner.resolve(engine, source, path, pos)
    }

    fn resolve_raw(
        &self,
        engine: &Engine,
        global: &mut rhai::GlobalRuntimeState,
        scope: &mut rhai::Scope,
        path: &str,
        pos: rhai::Position,
    ) -> std::result::Result<rhai::Shared<rhai::Module>, Box<rhai::EvalAltResult>> {
        self.validate(global.source(), path, pos)?;
        self.inner.resolve_raw(engine, global, scope, path, pos)
    }

    fn resolve_ast(
        &self,
        engine: &Engine,
        source: Option<&str>,
        path: &str,
        pos: rhai::Position,
    ) -> Option<std::result::Result<rhai::AST, Box<rhai::EvalAltResult>>> {
        if let Err(error) = self.validate(source, path, pos) {
            return Some(Err(error));
        }
        self.inner.resolve_ast(engine, source, path, pos)
    }
}

// ============================================================================
// In-memory module resolver for WASM
// ============================================================================

#[cfg(target_arch = "wasm32")]
mod wasm_resolver {
    use crate::version::{select_import_language, LanguageVersion};
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
        /// Language major inherited by unversioned modules.
        importer: LanguageVersion,
    }

    impl InMemoryModuleResolver {
        /// Create a new in-memory resolver with the given modules.
        pub fn new(modules: HashMap<String, String>) -> Self {
            Self::for_language(modules, LanguageVersion::V1)
        }

        pub fn for_language(modules: HashMap<String, String>, importer: LanguageVersion) -> Self {
            Self {
                modules: Arc::new(modules),
                extension: "vibe".to_string(),
                importer,
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

            select_import_language(source, self.importer).map_err(|error| {
                Box::new(rhai::EvalAltResult::ErrorInModule(
                    path.to_string(),
                    Box::new(rhai::EvalAltResult::ErrorRuntime(
                        error.to_string().into(),
                        pos,
                    )),
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

            if let Err(error) = select_import_language(source, self.importer) {
                return Some(Err(Box::new(rhai::EvalAltResult::ErrorInModule(
                    path.to_string(),
                    Box::new(rhai::EvalAltResult::ErrorRuntime(
                        error.to_string().into(),
                        pos,
                    )),
                    pos,
                ))));
            }

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

/// Canonical runtime outcome carrier for one evaluated v1 Rhai script.
///
/// Rhai terminal return values remain evaluation-local. This outer carrier is
/// the authoritative source for queue admission, readiness transitions, and
/// the eventual runtime outcome.
pub struct HostMutation {
    initial_receipt: MutationReceipt,
    latest_receipt: Arc<Mutex<Option<MutationReceipt>>>,
    receipt_updates: mpsc::Receiver<MutationReceipt>,
}

impl HostMutation {
    /// The receipt returned by submission. `accepted` is pending only; a
    /// pre-admission failure may instead already be terminal.
    #[must_use]
    pub fn initial_receipt(&self) -> &MutationReceipt {
        &self.initial_receipt
    }

    /// The newest canonical receipt published by the runtime.
    pub fn latest_receipt(&self) -> Result<MutationReceipt> {
        self.latest_receipt
            .lock()
            .map_err(|_| Error::Runtime("host receipt state is poisoned".into()))?
            .clone()
            .ok_or_else(|| Error::Runtime("host receipt was not published".into()))
    }

    /// Read the next readiness or terminal transition without blocking.
    ///
    /// `None` means no newer transition is currently available. Callers must
    /// not reinterpret that as success; [`HostMutation::latest_receipt`] stays
    /// authoritative.
    pub fn try_next_receipt(&self) -> Option<MutationReceipt> {
        while let Ok(receipt) = self.receipt_updates.try_recv() {
            if receipt.event_sequence > self.initial_receipt.event_sequence {
                return Some(receipt);
            }
        }
        None
    }

    /// Acknowledge the current fenced Partial so a host may deliberately
    /// continue v1 best-effort mutation.
    pub fn continue_best_effort(&self, handle: &RuntimeHandle) -> Result<()> {
        let latest = self.latest_receipt()?;
        handle
            .continue_best_effort(latest.attempt_id)
            .map_err(|error| Error::Runtime(error.to_string()))
    }
}

fn host_receipt_sink() -> (
    Arc<Mutex<Option<MutationReceipt>>>,
    mpsc::Receiver<MutationReceipt>,
    MutationReplySink,
) {
    let (send, receive) = mpsc::channel();
    let latest: Arc<Mutex<Option<MutationReceipt>>> = Arc::new(Mutex::new(None));
    let sink_latest = Arc::clone(&latest);
    let sink = MutationReplySink::new(move |receipt| {
        let mut publish = false;
        if let Ok(mut current) = sink_latest.lock() {
            let replace = current.as_ref().is_none_or(|current| {
                current.attempt_id == receipt.attempt_id
                    && current.runtime_epoch == receipt.runtime_epoch
                    && receipt.event_sequence > current.event_sequence
            });
            if replace {
                *current = Some(receipt.clone());
                publish = true;
            }
        }
        if publish {
            let _ = send.send(receipt);
        }
    });
    (latest, receive, sink)
}

struct PendingHostMutation {
    attempt: MutationAttempt,
    latest_receipt: Arc<Mutex<Option<MutationReceipt>>>,
    receipt_updates: mpsc::Receiver<MutationReceipt>,
}

impl PendingHostMutation {
    fn into_carrier(self, initial_receipt: MutationReceipt) -> HostMutation {
        HostMutation {
            initial_receipt,
            latest_receipt: self.latest_receipt,
            receipt_updates: self.receipt_updates,
        }
    }
}

fn host_failure(error: &Error) -> (FailurePhase, &'static str) {
    match error {
        Error::Io(_) => (FailurePhase::Decode, "script_read_failed"),
        Error::Parse(_) => (FailurePhase::Parse, "script_parse_failed"),
        Error::Language(_) => (FailurePhase::Decode, "language_contract_rejected"),
        Error::Foundation(_) => (FailurePhase::Evaluate, "candidate_validation_failed"),
        Error::Script(_) | Error::Runtime(_) => {
            (FailurePhase::Evaluate, "script_evaluation_failed")
        }
    }
}

fn host_submission_receipt(
    result: vibelang_core::Result<MutationReceipt>,
    latest_receipt: &Arc<Mutex<Option<MutationReceipt>>>,
) -> Result<MutationReceipt> {
    match result {
        Ok(receipt) => Ok(receipt),
        Err(error) => latest_receipt
            .lock()
            .map_err(|_| Error::Runtime("host receipt state is poisoned".into()))?
            .clone()
            .filter(|receipt| receipt.state.is_terminal())
            .ok_or_else(|| Error::Runtime(error.to_string())),
    }
}

fn host_submission(runtime_epoch: vibelang_core::mutation::RuntimeEpoch) -> Result<Submission> {
    let material = RequestMaterial::new(
        &("compat.vibelang.v1", "rhai_host", "reload_apply"),
        Some(&("compat.vibelang.v1", "rhai_host", "reload_apply")),
    )
    .map_err(|error| Error::Runtime(error.to_string()))?;
    Ok(Submission {
        kind: MutationKind::Candidate {
            origin: CandidateOrigin::RhaiHost,
        },
        source: MutationSource::Rhai {
            engine_id: "compat.vibelang.v1.rhai_host".into(),
        },
        caller_namespace: "compat.vibelang.v1.local".into(),
        idempotency_key: None,
        require_idempotency_key: false,
        retry_epoch: Some(runtime_epoch),
        expected_revision: None,
        atomicity: Atomicity::BestEffort,
        supersession: SupersessionPolicy::Fifo,
        material,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct V2EngineConfig {
    manifest_digest: ContractDigest,
    runtime_epoch: vibelang_core::mutation::RuntimeEpoch,
}

impl V2EngineConfig {
    #[must_use]
    pub fn new(
        manifest_digest: ContractDigest,
        runtime_epoch: vibelang_core::mutation::RuntimeEpoch,
    ) -> Self {
        Self {
            manifest_digest,
            runtime_epoch,
        }
    }

    #[must_use]
    pub fn manifest_digest(&self) -> &ContractDigest {
        &self.manifest_digest
    }

    #[must_use]
    pub const fn runtime_epoch(&self) -> vibelang_core::mutation::RuntimeEpoch {
        self.runtime_epoch
    }
}

struct V2Engine {
    engine: Engine,
    identity: EvaluationIdentity,
}

pub struct CompiledScript {
    language: LanguageVersion,
    identity: Option<EvaluationIdentity>,
    ast: rhai::AST,
}

impl CompiledScript {
    #[must_use]
    pub const fn language(&self) -> LanguageVersion {
        self.language
    }

    #[must_use]
    pub fn identity(&self) -> Option<&EvaluationIdentity> {
        self.identity.as_ref()
    }
}

#[derive(Clone, Debug)]
pub enum ScriptEvaluation {
    V1(Box<ScriptState>),
    V2(Candidate),
}

fn base_engine() -> Engine {
    let mut engine = Engine::new();
    engine.set_max_expr_depths(4096, 4096);
    engine.set_max_call_levels(4096);
    engine.on_print(|text| {
        log::info!("[script] {}", text);
    });
    engine.on_debug(|text, source, pos| {
        let loc = match (source, pos) {
            (Some(src), pos) if !pos.is_none() => format!(" ({}:{})", src, pos),
            (Some(src), _) => format!(" ({})", src),
            (None, pos) if !pos.is_none() => format!(" ({})", pos),
            _ => String::new(),
        };
        log::debug!("[script]{} {}", loc, text);
    });
    engine
}

fn v1_engine() -> Engine {
    let mut engine = base_engine();
    api::register_api(&mut engine);
    vibelang_dsp::register_dsp_api(&mut engine);

    #[cfg(target_arch = "wasm32")]
    {
        let resolver = InMemoryModuleResolver::for_language(
            vibelang_std::get_stdlib_files(),
            LanguageVersion::V1,
        );
        engine.set_module_resolver(resolver);
    }

    engine
}

fn v2_engine(identity: EvaluationIdentity) -> V2Engine {
    let mut engine = base_engine();
    foundation::register(&mut engine);
    api::install_v2_api(&mut engine);

    #[cfg(target_arch = "wasm32")]
    {
        let resolver = InMemoryModuleResolver::for_language(
            vibelang_std::get_stdlib_files(),
            LanguageVersion::V2,
        );
        engine.set_module_resolver(resolver);
    }

    V2Engine { engine, identity }
}

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
    v2: Option<V2Engine>,
    #[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
    extension_config: Option<crate::extensions::ExtensionConfig>,
    #[cfg(not(target_arch = "wasm32"))]
    import_paths: Vec<PathBuf>,
    #[cfg(not(target_arch = "wasm32"))]
    module_base_path: Option<PathBuf>,
}

impl ScriptEngine {
    /// Create a new script engine with all VibeLang API registered.
    pub fn new() -> Self {
        Self {
            engine: v1_engine(),
            v2: None,
            #[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
            extension_config: None,
            #[cfg(not(target_arch = "wasm32"))]
            import_paths: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            module_base_path: None,
        }
    }

    pub fn with_v2(config: V2EngineConfig) -> Result<Self> {
        let mut engine = Self::new();
        engine.enable_v2(config)?;
        Ok(engine)
    }

    pub fn enable_v2(&mut self, config: V2EngineConfig) -> Result<()> {
        if self.v2.is_some() {
            return Err(Error::Runtime(
                "vibe-api 2 is already enabled for this ScriptEngine".into(),
            ));
        }
        let identity = EvaluationIdentity::new(
            LanguageContract::v2(config.manifest_digest),
            EngineInstanceId::new(),
            config.runtime_epoch,
        );
        self.v2 = Some(v2_engine(identity));
        #[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
        if let (Some(v2), Some(extension_config)) =
            (self.v2.as_mut(), self.extension_config.as_ref())
        {
            crate::extensions::register_extensions_v2(&mut v2.engine, extension_config);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(base_path) = self.module_base_path.clone() {
            self.setup_module_resolver(base_path);
        }
        Ok(())
    }

    #[must_use]
    pub fn v2_identity(&self) -> Option<&EvaluationIdentity> {
        self.v2.as_ref().map(|v2| &v2.identity)
    }

    /// Serialize the effective native registrations for the versioned public API manifest.
    #[cfg(feature = "api-manifest")]
    pub fn public_api_metadata_json() -> std::result::Result<String, String> {
        let mut script_engine = Self::new();
        script_engine.register_all_extensions();
        script_engine
            .engine
            .gen_fn_metadata_to_json(false)
            .map_err(|error| error.to_string())
    }

    /// Add import paths for module resolution (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn add_import_path(&mut self, path: impl Into<PathBuf>) {
        self.import_paths.push(path.into());
    }

    pub fn compile_versioned(&self, script: &str) -> Result<CompiledScript> {
        let language = select_language(script)?;
        match language {
            LanguageVersion::V1 => Ok(CompiledScript {
                language,
                identity: None,
                ast: self.engine.compile(script).map_err(Error::from)?,
            }),
            LanguageVersion::V2 => {
                let v2 = self.v2.as_ref().ok_or(LanguageSelectionError::V2Disabled)?;
                Ok(CompiledScript {
                    language,
                    identity: Some(v2.identity.clone()),
                    ast: v2.engine.compile(script).map_err(Error::from)?,
                })
            }
        }
    }

    pub fn evaluate(&mut self, script: &str) -> Result<ScriptEvaluation> {
        let compiled = self.compile_versioned(script)?;
        self.execute_compiled(&compiled)
    }

    pub fn execute_compiled(&mut self, compiled: &CompiledScript) -> Result<ScriptEvaluation> {
        match compiled.language {
            LanguageVersion::V1 => {
                if compiled.identity.is_some() {
                    return Err(Error::Runtime(
                        "a v1 compiled script carried a v2 evaluation identity".into(),
                    ));
                }
                Ok(ScriptEvaluation::V1(Box::new(
                    self.execute_precompiled(&compiled.ast)?,
                )))
            }
            LanguageVersion::V2 => {
                let v2 = self.v2.as_mut().ok_or(LanguageSelectionError::V2Disabled)?;
                let compiled_identity = compiled.identity.as_ref().ok_or_else(|| {
                    Error::Runtime("a v2 compiled script has no evaluation identity".into())
                })?;
                v2.identity
                    .ensure_compatible(compiled_identity)
                    .map_err(foundation::FoundationError::from)?;
                foundation::begin_evaluation(v2.identity.clone())?;
                if let Err(error) = v2.engine.run_ast(&compiled.ast) {
                    foundation::abort_evaluation();
                    return Err(Error::from(error));
                }
                Ok(ScriptEvaluation::V2(foundation::finish_evaluation()?))
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn evaluate_file(&mut self, path: impl AsRef<Path>) -> Result<ScriptEvaluation> {
        let path = path.as_ref();
        let script = std::fs::read_to_string(path)?;
        let base_path = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        self.setup_module_resolver(base_path);
        let mut compiled = self.compile_versioned(&script)?;
        compiled.ast.set_source(path.to_string_lossy().to_string());
        self.execute_compiled(&compiled)
    }

    /// Execute a script from a string.
    ///
    /// Returns the collected ScriptState that can be applied to a runtime.
    pub fn execute(&mut self, script: &str) -> Result<ScriptState> {
        if select_language(script)? == LanguageVersion::V2 {
            return Err(LanguageSelectionError::V2RequiresVersionedEntryPoint.into());
        }
        self.compile(script)?;

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

    /// Execute a previously compiled whole script with normal v1 setup and
    /// exit handling. Hosts may use this to keep parsing ahead of eager work.
    pub fn execute_precompiled(&mut self, ast: &rhai::AST) -> Result<ScriptState> {
        // Clear object registries before each execution
        api::clear_all_registries();

        // Reset exit code before execution
        crate::reset_exit_code();

        // Initialize context
        context::init_context();

        // Execute script
        let result = self.engine.run_ast(ast).map_err(Error::from);

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

    /// Evaluate a v1 Rhai script and submit its collected state through the
    /// canonical runtime receipt ledger.
    ///
    /// The returned [`HostMutation`] initially contains queue-admission truth,
    /// not an applied claim. Hosts should observe it until the canonical
    /// receipt becomes terminal.
    pub async fn execute_and_submit(
        &mut self,
        script: &str,
        handle: &RuntimeHandle,
    ) -> Result<HostMutation> {
        let mut pending = Self::begin_host_attempt(handle)?;
        if !pending.attempt.is_active() {
            let receipt = pending.attempt.receipt().clone();
            return Ok(pending.into_carrier(receipt));
        }
        let state = match self
            .compile(script)
            .and_then(|ast| self.execute_precompiled(&ast))
        {
            Ok(state) => state,
            Err(error) => {
                let (phase, code) = host_failure(&error);
                let message = error.to_string();
                let terminal_message = if error.definitely_no_effect() {
                    message
                } else {
                    match pending.attempt.record_uncertain_effect(
                        "rhai/evaluation",
                        "evaluate",
                        code,
                        message.clone(),
                    ) {
                        Ok(()) => message,
                        Err(accounting) => {
                            format!("{message}; effect accounting failed: {accounting}")
                        }
                    }
                };
                return Self::finish_host_attempt(handle, pending, phase, code, terminal_message);
            }
        };
        if let Err(error) = pending.attempt.record_uncertain_effect(
                "rhai/evaluation",
                "evaluate",
                "rhai_eager_effects_possible",
                "Rhai evaluation may have updated process-global registries or invoked a deploy callback",
            ) {
            return Self::finish_host_attempt(
                handle,
                pending,
                FailurePhase::Evaluate,
                "evaluation_effect_accounting_failed",
                error.to_string(),
            );
        }
        Self::submit_host_attempt(handle, state, pending).await
    }

    /// Submit an already evaluated v1 [`ScriptState`] and expose all canonical
    /// receipt transitions to the Rhai host.
    pub async fn submit_state(handle: &RuntimeHandle, state: ScriptState) -> Result<HostMutation> {
        let pending = Self::begin_host_attempt(handle)?;
        Self::submit_host_attempt(handle, state, pending).await
    }

    fn begin_host_attempt(handle: &RuntimeHandle) -> Result<PendingHostMutation> {
        let (latest_receipt, receipt_updates, reply_sink) = host_receipt_sink();
        let submission = host_submission(handle.mutation_status().runtime_epoch)?;
        let attempt = handle
            .begin_attempt(submission, reply_sink, MutationEventSink::default())
            .map_err(|error| Error::Runtime(error.to_string()))?;
        Ok(PendingHostMutation {
            attempt,
            latest_receipt,
            receipt_updates,
        })
    }

    fn finish_host_attempt(
        handle: &RuntimeHandle,
        pending: PendingHostMutation,
        phase: FailurePhase,
        code: &str,
        message: String,
    ) -> Result<HostMutation> {
        let PendingHostMutation {
            attempt,
            latest_receipt,
            receipt_updates,
        } = pending;
        let receipt = handle
            .finish_attempt_failure(attempt, phase, code, message)
            .map_err(|error| Error::Runtime(error.to_string()))?;
        Ok(HostMutation {
            initial_receipt: receipt,
            latest_receipt,
            receipt_updates,
        })
    }

    async fn submit_host_attempt(
        handle: &RuntimeHandle,
        state: ScriptState,
        pending: PendingHostMutation,
    ) -> Result<HostMutation> {
        if !pending.attempt.is_active() {
            let receipt = pending.attempt.receipt().clone();
            return Ok(pending.into_carrier(receipt));
        }
        let PendingHostMutation {
            attempt,
            latest_receipt,
            receipt_updates,
        } = pending;
        let submission_result = handle
            .submit_attempt(ReloadMessage::Apply { state }.into(), attempt)
            .await;
        let initial_receipt = host_submission_receipt(submission_result, &latest_receipt)?;
        Ok(HostMutation {
            initial_receipt,
            latest_receipt,
            receipt_updates,
        })
    }

    /// Execute a script from a file (native only).
    ///
    /// Returns the collected ScriptState that can be applied to a runtime.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn execute_file(&mut self, path: impl AsRef<Path>) -> Result<ScriptState> {
        let path = path.as_ref();

        // Read script
        let script = std::fs::read_to_string(path)?;
        if select_language(&script)? == LanguageVersion::V2 {
            return Err(LanguageSelectionError::V2RequiresVersionedEntryPoint.into());
        }

        // Set up module resolver
        let base_path = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        self.setup_module_resolver(base_path);

        let ast = self.engine.compile(&script).map_err(Error::from)?;

        // Clear object registries before each execution
        api::clear_all_registries();

        // Reset exit code before execution
        crate::reset_exit_code();

        // Initialize context
        context::init_context();
        context::set_current_file(Some(path.to_path_buf()));
        context::set_import_paths(self.import_paths.clone());

        // Execute script
        let result = self.engine.run_ast(&ast).map_err(Error::from);

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

    /// Evaluate a v1 Rhai file and submit its state through the canonical
    /// runtime receipt ledger.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn execute_file_and_submit(
        &mut self,
        path: impl AsRef<Path>,
        handle: &RuntimeHandle,
    ) -> Result<HostMutation> {
        let mut pending = Self::begin_host_attempt(handle)?;
        if !pending.attempt.is_active() {
            let receipt = pending.attempt.receipt().clone();
            return Ok(pending.into_carrier(receipt));
        }
        let state = match self.execute_file(path) {
            Ok(state) => state,
            Err(error) => {
                let (phase, code) = host_failure(&error);
                let message = error.to_string();
                let terminal_message = if error.definitely_no_effect() {
                    message
                } else {
                    match pending.attempt.record_uncertain_effect(
                        "rhai/file_evaluation",
                        "evaluate",
                        code,
                        message.clone(),
                    ) {
                        Ok(()) => message,
                        Err(accounting) => {
                            format!("{message}; effect accounting failed: {accounting}")
                        }
                    }
                };
                return Self::finish_host_attempt(handle, pending, phase, code, terminal_message);
            }
        };
        if let Err(error) = pending.attempt.record_uncertain_effect(
                "rhai/file_evaluation",
                "evaluate",
                "rhai_eager_effects_possible",
                "Rhai file evaluation may have updated process-global registries or invoked a deploy callback",
            ) {
            return Self::finish_host_attempt(
                handle,
                pending,
                FailurePhase::Evaluate,
                "evaluation_effect_accounting_failed",
                error.to_string(),
            );
        }
        Self::submit_host_attempt(handle, state, pending).await
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
        if select_language(&script)? == LanguageVersion::V2 {
            return Err(LanguageSelectionError::V2RequiresVersionedEntryPoint.into());
        }

        let base_path = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        self.setup_module_resolver(base_path);

        let ast = self.engine.compile(&script).map_err(Error::from)?;

        api::clear_all_registries();
        crate::reset_exit_code();

        context::init_context();
        context::set_current_file(Some(path.to_path_buf()));
        context::set_import_paths(self.import_paths.clone());

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
        if select_language(script)? == LanguageVersion::V2 {
            return Err(LanguageSelectionError::V2RequiresVersionedEntryPoint.into());
        }
        self.engine.compile(script).map_err(Error::from)
    }

    /// Compile a script file to AST (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn compile_file(&self, path: impl AsRef<Path>) -> Result<rhai::AST> {
        let path = path.as_ref();
        let source = std::fs::read_to_string(path)?;
        if select_language(&source)? == LanguageVersion::V2 {
            return Err(LanguageSelectionError::V2RequiresVersionedEntryPoint.into());
        }
        self.engine
            .compile_file(path.to_path_buf())
            .map_err(Error::from)
    }

    /// Set up module resolver for import statements (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn setup_module_resolver(&mut self, base_path: PathBuf) {
        self.module_base_path = Some(base_path.clone());
        let mut v1_collection = rhai::module_resolvers::ModuleResolversCollection::new();

        // 1. Source-relative resolver (highest priority)
        v1_collection.push(VersionedFileModuleResolver::new(None, LanguageVersion::V1));

        // 2. Base path resolver
        v1_collection.push(VersionedFileModuleResolver::new(
            Some(base_path.clone()),
            LanguageVersion::V1,
        ));

        // 3. Additional import paths
        for import_path in &self.import_paths {
            v1_collection.push(VersionedFileModuleResolver::new(
                Some(import_path.clone()),
                LanguageVersion::V1,
            ));
        }

        self.engine.set_module_resolver(v1_collection);

        if let Some(v2) = self.v2.as_mut() {
            let mut v2_collection = rhai::module_resolvers::ModuleResolversCollection::new();
            v2_collection.push(VersionedFileModuleResolver::new(None, LanguageVersion::V2));
            v2_collection.push(VersionedFileModuleResolver::new(
                Some(base_path),
                LanguageVersion::V2,
            ));
            for import_path in &self.import_paths {
                v2_collection.push(VersionedFileModuleResolver::new(
                    Some(import_path.clone()),
                    LanguageVersion::V2,
                ));
            }
            v2.engine.set_module_resolver(v2_collection);
        }
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
        if let Some(v2) = self.v2.as_mut() {
            crate::extensions::register_extensions_v2(&mut v2.engine, config);
        }
        self.extension_config = Some(config.clone());
    }

    /// Register all available extensions.
    ///
    /// This is a convenience method that enables all compiled-in extensions.
    /// Use with caution in production environments.
    #[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
    pub fn register_all_extensions(&mut self) {
        let config = crate::extensions::ExtensionConfig::enable_all();
        crate::extensions::register_extensions(&mut self.engine, &config);
        if let Some(v2) = self.v2.as_mut() {
            crate::extensions::register_extensions_v2(&mut v2.engine, &config);
        }
        self.extension_config = Some(config);
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
    use async_trait::async_trait;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use vibelang_core::compat::Instant;
    use vibelang_core::mutation::{ReceiptState, TerminalOutcome};
    use vibelang_core::{AddAction, Backend, BufferId, BufferInfo, NodeId, ParamMap, Runtime};

    #[derive(Debug)]
    struct CarrierBackendError;

    impl std::fmt::Display for CarrierBackendError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("carrier backend error")
        }
    }

    impl std::error::Error for CarrierBackendError {}

    struct CarrierBackend;

    #[async_trait]
    impl Backend for CarrierBackend {
        type Error = CarrierBackendError;

        async fn load_synthdef(
            &self,
            _name: &str,
            _data: &[u8],
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn create_synth(
            &self,
            _def: &str,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
            _params: &ParamMap,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn create_group(
            &self,
            _node: NodeId,
            _target: NodeId,
            _action: AddAction,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_node(&self, _node: NodeId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn run_node(
            &self,
            _node: NodeId,
            _running: bool,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn set_param(
            &self,
            _node: NodeId,
            _param: &str,
            _value: f32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn map_param_to_bus(
            &self,
            _node: NodeId,
            _param: &str,
            _bus: u32,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn load_buffer(
            &self,
            _id: BufferId,
            _path: &Path,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames: 0,
                channels: 1,
                sample_rate: 44_100.0,
            })
        }

        async fn alloc_buffer(
            &self,
            _id: BufferId,
            frames: u32,
            channels: u16,
        ) -> std::result::Result<BufferInfo, Self::Error> {
            Ok(BufferInfo {
                frames,
                channels,
                sample_rate: 44_100.0,
            })
        }

        async fn write_buffer(
            &self,
            _id: BufferId,
            _path: &Path,
        ) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        async fn free_buffer(&self, _id: BufferId) -> std::result::Result<(), Self::Error> {
            Ok(())
        }

        fn current_time(&self) -> Instant {
            Instant::now()
        }
    }

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

    fn v2_config(seed: u8) -> V2EngineConfig {
        V2EngineConfig::new(
            ContractDigest::from_bytes(&[seed]),
            vibelang_core::mutation::RuntimeEpoch::new(),
        )
    }

    #[test]
    fn versioned_evaluation_selects_exact_v2_while_unversioned_stays_v1() {
        let mut engine = ScriptEngine::with_v2(v2_config(1)).unwrap();

        let ScriptEvaluation::V1(state) = engine.evaluate("set_tempo(143);").unwrap() else {
            panic!("unversioned source must remain v1");
        };
        assert_eq!(state.tempo, 143.0);

        let ScriptEvaluation::V2(candidate) = engine
            .evaluate("// vibe-api: 2\nlet detached = 1 + 1;")
            .unwrap()
        else {
            panic!("the exact directive must select v2");
        };
        assert!(candidate.declarations().is_empty());
        assert_eq!(candidate.identity(), engine.v2_identity().unwrap());
    }

    #[test]
    fn disabled_v2_and_legacy_entry_point_reject_without_v1_dispatch() {
        let mut disabled = ScriptEngine::new();
        assert!(matches!(
            disabled.evaluate("// vibe-api: 2\nset_tempo(199);"),
            Err(Error::Language(LanguageSelectionError::V2Disabled))
        ));
        assert_eq!(disabled.execute("set_tempo(144);").unwrap().tempo, 144.0);

        let mut enabled = ScriptEngine::with_v2(v2_config(2)).unwrap();
        assert!(matches!(
            enabled.execute("// vibe-api: 2\nset_tempo(199);"),
            Err(Error::Language(
                LanguageSelectionError::V2RequiresVersionedEntryPoint
            ))
        ));
        assert!(enabled
            .evaluate("// vibe-api: 2\nset_tempo(199);")
            .unwrap_err()
            .to_string()
            .contains("set_tempo"));
        let ScriptEvaluation::V2(candidate) = enabled.evaluate("// vibe-api: 2").unwrap() else {
            panic!("v2 context must be clean after evaluation failure");
        };
        assert!(candidate.declarations().is_empty());
    }

    #[test]
    fn compiled_v2_script_rejects_cross_engine_identity_without_candidate_residue() {
        let first = ScriptEngine::with_v2(v2_config(3)).unwrap();
        let compiled = first
            .compile_versioned("// vibe-api: 2\nlet x = 1;")
            .unwrap();
        let mut second = ScriptEngine::with_v2(v2_config(3)).unwrap();

        assert!(matches!(
            second.execute_compiled(&compiled),
            Err(Error::Foundation(
                foundation::FoundationError::Compatibility(
                    vibelang_core::candidate::CompatibilityError::Engine { .. }
                )
            ))
        ));
        let ScriptEvaluation::V2(candidate) = second.evaluate("// vibe-api: 2").unwrap() else {
            panic!("cross-engine rejection must leave no active Candidate");
        };
        assert!(candidate.declarations().is_empty());
    }

    #[test]
    fn v2_engine_installs_all_m08_authoring_families_for_production_scripts() {
        let mut engine = ScriptEngine::with_v2(v2_config(8)).unwrap();
        let ScriptEvaluation::V2(candidate) = engine
            .evaluate(
                r#"// vibe-api: 2
define_group("band", || {});
let lead = voice("lead").synth("sine").apply();
pattern("kick").on(lead).step("x...").apply();
melody("hook").on(lead).notes("C4 E4 G4 C5").apply();
sequence("intro").loop_beats(4.0).apply();
fade("swell").on_group(group("band")).over(2.0).apply();
define_synthdef("tone").param("freq", 440.0).body(|freq| dc_ar(0.0)).apply();
define_effect("room").param("mix", 0.5).body(|input, mix| input).apply();
fx("verb");
"#,
            )
            .unwrap()
        else {
            panic!("the v2 directive must produce a candidate");
        };

        let keys: std::collections::BTreeSet<_> = candidate
            .declarations()
            .iter()
            .map(|declaration| declaration.address().key().as_str().to_owned())
            .collect();
        for key in ["band", "lead", "kick", "hook", "intro", "swell"] {
            assert!(keys.contains(key), "missing v2 {key} declaration");
        }
        assert_eq!(candidate.dsp_definitions().definitions().count(), 2);
    }

    #[test]
    #[cfg(all(feature = "midi", not(target_arch = "wasm32")))]
    fn v2_engine_installs_all_m09_families_for_production_scripts() {
        let mut engine = ScriptEngine::with_v2(v2_config(9)).unwrap();
        let ScriptEvaluation::V2(candidate) = engine
            .evaluate(
                r#"// vibe-api: 2
define_group("band", || {});
let lead = voice("lead").synth("sine").apply();
let wob = voice("wob").synth("sine").apply();
sample("kick", "samples/kick.wav").one_shot().apply();
buffer("scratch").frames(64).channels(2).clear().apply();
sfz("piano", "instruments/piano.sfz").apply();
record("take1")
    .from(group_ref("band"))
    .beats(16.0)
    .to_file("takes/one.wav")
    .channels(2)
    .apply();
output(lead, "out").to_groups([group_ref("band")]);
output(wob, "cv").scale(0.5).set(lead, "cutoff");
let mpk = midi_device("mpk").port("MPK Mini").input().apply();
keyboard_route(mpk).channel(2).to(lead);
"#,
            )
            .unwrap()
        else {
            panic!("the v2 directive must produce a candidate");
        };

        let keys: std::collections::BTreeSet<_> = candidate
            .declarations()
            .iter()
            .map(|declaration| declaration.address().key().as_str().to_owned())
            .collect();
        for key in [
            "band", "lead", "wob", "kick", "scratch", "piano", "take1", "mpk",
        ] {
            assert!(keys.contains(key), "missing v2 {key} declaration");
        }
        let topology = candidate.route_topology();
        assert_eq!(topology.audio.len(), 1);
        assert_eq!(topology.params.len(), 1);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn imports_inherit_v2_and_cached_cross_major_changes_reject() {
        let dir = write_test_project(&[("helper.vibe", "let helper_value = 1;")]);
        let mut v2 = ScriptEngine::with_v2(v2_config(4)).unwrap();
        v2.setup_module_resolver(dir.clone());
        let ScriptEvaluation::V2(candidate) = v2
            .evaluate("// vibe-api: 2\nimport \"helper.vibe\";")
            .unwrap()
        else {
            panic!("unversioned import must inherit v2");
        };
        assert!(candidate.declarations().is_empty());

        let mut v1 = ScriptEngine::new();
        v1.setup_module_resolver(dir.clone());
        v1.execute("import \"helper.vibe\";").unwrap();
        std::fs::write(
            dir.join("helper.vibe"),
            "// vibe-api: 2\nlet helper_value = 2;",
        )
        .unwrap();
        let error = v1.execute("import \"helper.vibe\";").unwrap_err();
        std::fs::remove_dir_all(&dir).ok();

        assert!(error.to_string().contains("cross-major import rejected"));
    }

    fn host_receipt(state: vibelang_core::mutation::ReceiptState) -> MutationReceipt {
        use vibelang_core::mutation::{
            Atomicity, EventSequence, MutationKind, MutationSource, ReceiptTimestamps,
            RequestIdentity, RuntimeEpoch, SupersessionPolicy, Timestamp, MUTATION_SCHEMA_VERSION,
        };

        let now = Timestamp::parse("2026-07-17T08:00:00Z").unwrap();
        MutationReceipt {
            schema_version: MUTATION_SCHEMA_VERSION,
            attempt_id: vibelang_core::mutation::AttemptId::new(),
            runtime_epoch: RuntimeEpoch::new(),
            revision: Some(vibelang_core::mutation::RevisionId::new(1).unwrap()),
            event_sequence: EventSequence::new(1).unwrap(),
            request: RequestIdentity {
                kind: MutationKind::Candidate {
                    origin: CandidateOrigin::RhaiHost,
                },
                source: MutationSource::Rhai {
                    engine_id: "compat.vibelang.v1.rhai_host".into(),
                },
                submission_digest: None,
                operation_digest: None,
                idempotency_key_present: false,
                expected_revision: None,
                atomicity: Atomicity::BestEffort,
                supersession: SupersessionPolicy::Fifo,
            },
            state,
            previous_confirmed_revision: None,
            timestamps: ReceiptTimestamps {
                submitted_at: now.clone(),
                accepted_at: Some(now.clone()),
                last_transition_at: now,
                terminal_at: None,
            },
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn host_submission_is_candidate_local_best_effort() {
        let epoch = vibelang_core::mutation::RuntimeEpoch::new();
        let submission = host_submission(epoch).unwrap();

        assert_eq!(
            submission.kind,
            MutationKind::Candidate {
                origin: CandidateOrigin::RhaiHost
            }
        );
        assert_eq!(
            submission.source,
            MutationSource::Rhai {
                engine_id: "compat.vibelang.v1.rhai_host".into()
            }
        );
        assert_eq!(submission.atomicity, Atomicity::BestEffort);
        assert_eq!(submission.supersession, SupersessionPolicy::Fifo);
        assert_eq!(submission.retry_epoch, Some(epoch));
    }

    #[test]
    fn host_carrier_keeps_late_partial_canonical() {
        use vibelang_core::mutation::{
            FailurePhase, Partial, ReceiptState, RollbackState, TerminalOutcome,
        };

        let accepted = host_receipt(ReceiptState::Accepted {
            queue_position: Some(1),
        });
        let (latest_receipt, receipt_updates, sink) = host_receipt_sink();
        sink.publish(accepted.clone());
        let carrier = HostMutation {
            initial_receipt: accepted,
            latest_receipt,
            receipt_updates,
        };
        let mut partial = host_receipt(ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: FailurePhase::BackendBarrier,
            code: "backend_sync_failed".into(),
            components: Vec::new(),
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: None,
        })));
        partial.attempt_id = carrier.initial_receipt().attempt_id;
        partial.runtime_epoch = carrier.initial_receipt().runtime_epoch;
        partial.revision = carrier.initial_receipt().revision;
        partial.event_sequence = vibelang_core::mutation::EventSequence::new(2).unwrap();
        sink.publish(partial.clone());

        assert_eq!(carrier.try_next_receipt().unwrap(), partial);
        assert!(carrier.try_next_receipt().is_none());
        assert_eq!(carrier.latest_receipt().unwrap(), partial);
    }

    #[test]
    fn host_carrier_preserves_terminal_receipt_when_admission_channel_closes() {
        use vibelang_core::mutation::{
            FailurePhase, ReceiptState, Rejected, RollbackState, TerminalOutcome,
        };

        let rejected = host_receipt(ReceiptState::Terminal(TerminalOutcome::Rejected(
            Rejected {
                phase: FailurePhase::Admission,
                code: "queue_closed".into(),
                message: "the runtime mutation queue is closed".into(),
                rollback: RollbackState::NotNeeded,
                preserved_revision: None,
            },
        )));
        let latest = Arc::new(Mutex::new(Some(rejected.clone())));

        let canonical =
            host_submission_receipt(Err(vibelang_core::Error::ChannelClosed), &latest).unwrap();

        assert_eq!(canonical, rejected);
    }

    #[test]
    fn host_receipt_sink_rejects_reordered_and_foreign_callbacks() {
        let accepted = host_receipt(ReceiptState::Accepted {
            queue_position: Some(1),
        });
        let mut terminal = accepted.clone();
        terminal.event_sequence = vibelang_core::mutation::EventSequence::new(3).unwrap();
        terminal.state = ReceiptState::Terminal(TerminalOutcome::Rejected(
            vibelang_core::mutation::Rejected {
                phase: FailurePhase::Evaluate,
                code: "script_evaluation_failed".into(),
                message: "script aborted".into(),
                rollback: vibelang_core::mutation::RollbackState::NotNeeded,
                preserved_revision: None,
            },
        ));
        let mut stale = accepted;
        stale.event_sequence = vibelang_core::mutation::EventSequence::new(2).unwrap();
        let foreign = host_receipt(ReceiptState::Accepted {
            queue_position: Some(2),
        });
        let (latest, updates, sink) = host_receipt_sink();

        sink.publish(terminal.clone());
        sink.publish(stale);
        sink.publish(foreign);

        assert_eq!(updates.try_recv().unwrap(), terminal);
        assert!(updates.try_recv().is_err());
        assert_eq!(latest.lock().unwrap().as_ref(), Some(&terminal));
    }

    #[tokio::test]
    async fn host_success_preserves_preallocated_attempt_through_admission() {
        let runtime = Runtime::new(CarrierBackend);
        let handle = runtime.handle();
        let mut engine = ScriptEngine::new();

        let carrier = engine
            .execute_and_submit("set_tempo(141);", &handle)
            .await
            .unwrap();

        assert!(matches!(
            carrier.initial_receipt().state,
            ReceiptState::Accepted { .. }
        ));
        assert!(carrier.initial_receipt().revision.is_some());
        assert_eq!(
            carrier.latest_receipt().unwrap().attempt_id,
            carrier.initial_receipt().attempt_id
        );
        assert_eq!(
            carrier.latest_receipt().unwrap().revision,
            carrier.initial_receipt().revision
        );
    }

    #[tokio::test]
    async fn host_parse_failure_is_effect_free_rejected_attempt() {
        let runtime = Runtime::new(CarrierBackend);
        let handle = runtime.handle();
        let mut engine = ScriptEngine::new();

        let carrier = engine.execute_and_submit("let = ;", &handle).await.unwrap();

        assert!(carrier.initial_receipt().revision.is_none());
        let ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) =
            &carrier.initial_receipt().state
        else {
            panic!("parse failure must be rejected");
        };
        assert_eq!(rejected.phase, FailurePhase::Parse);
        assert_eq!(rejected.code, "script_parse_failed");
        assert_eq!(
            carrier.latest_receipt().unwrap().attempt_id,
            carrier.initial_receipt().attempt_id
        );
    }

    #[tokio::test]
    async fn host_runtime_failure_after_eager_effect_is_fenced_partial() {
        let runtime = Runtime::new(CarrierBackend);
        let handle = runtime.handle();
        let effect_ran = Arc::new(AtomicBool::new(false));
        let effect_capture = Arc::clone(&effect_ran);
        let mut engine = ScriptEngine::new();
        engine.engine.register_fn("eager_effect", move || {
            effect_capture.store(true, Ordering::SeqCst);
        });

        let carrier = engine
            .execute_and_submit("eager_effect(); throw \"stop\";", &handle)
            .await
            .unwrap();

        assert!(effect_ran.load(Ordering::SeqCst));
        assert!(carrier.initial_receipt().revision.is_none());
        let ReceiptState::Terminal(TerminalOutcome::Partial(partial)) =
            &carrier.initial_receipt().state
        else {
            panic!("runtime failure after eager work must be partial");
        };
        assert!(partial.fenced);
        assert_eq!(partial.phase, FailurePhase::Evaluate);
        assert_eq!(partial.code, "script_evaluation_failed");
        assert_eq!(partial.components[0].path, "rhai/evaluation");
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
            .all(|body| body.target_group == drums_id && body.target_path == "Drums"));
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
            .all(|body| body.target_group == drums_id && body.target_path == "Drums"));
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

        assert_eq!(alias_target.path, "Drums");
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
            vec![("Drums", 0), ("Drums", 1), ("Drums", 2)]
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
            vec!["Drums", "Drums"]
        );
    }

    #[test]
    fn test_repeated_nested_group_bodies_merge_single_file_content() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            group("Song").body(|| {
                group("Drums").body(|| {
                    voice("kick").synth("kick_synth");
                });
            });

            group("Song").body(|| {
                group("Drums").body(|| {
                    fx("drums_comp").synth("compressor").apply();
                });
            });

            group("Song").body(|| {
                group("Drums").body(|| {
                    voice("snare").synth("snare_synth");
                });
            });
        "#,
            )
            .unwrap();

        let song_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Song").then_some(*id))
            .expect("Song group should exist");
        let drums_id = state
            .groups
            .iter()
            .find_map(|(id, config)| {
                (config.name == "Drums" && config.parent == Some(song_id)).then_some(*id)
            })
            .expect("nested Drums group should exist");
        let drums = state.groups.get(&drums_id).unwrap();

        assert_eq!(
            state
                .groups
                .values()
                .filter(|config| config.name == "Drums")
                .count(),
            1,
            "repeated nested bodies should merge into one Drums group"
        );
        assert_eq!(drums.effects.len(), 1);
        assert!(state.voices.values().all(|voice| voice.group == drums_id));

        let voice_names = state
            .voice_order
            .iter()
            .map(|id| state.voices.get(id).unwrap().name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(voice_names, vec!["kick", "snare"]);

        let effect_synthdefs = drums
            .effects
            .iter()
            .map(|id| state.effects.get(id).unwrap().synthdef.as_str())
            .collect::<Vec<_>>();
        assert_eq!(effect_synthdefs, vec!["compressor"]);

        assert_eq!(
            state
                .body_contributions
                .iter()
                .filter(|body| body.target_group == drums_id)
                .map(|body| body.target_path.as_str())
                .collect::<Vec<_>>(),
            vec!["Song/Drums", "Song/Drums", "Song/Drums"]
        );
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_nested_group_bodies_merge_across_imported_files() {
        let dir = write_test_project(&[
            (
                "main.vibe",
                r#"
            import "voices.vibe";
            import "effects.vibe";

            group("Song").body(|| {
                group("Drums").body(|| {
                    voice("snare").synth("snare_synth");
                });
            });
        "#,
            ),
            (
                "voices.vibe",
                r#"
            group("Song").body(|| {
                group("Drums").body(|| {
                    voice("kick").synth("kick_synth");
                });
            });
        "#,
            ),
            (
                "effects.vibe",
                r#"
            group("Song").body(|| {
                group("Drums").body(|| {
                    fx("drums_room").synth("room_reverb").apply();
                });
            });
        "#,
            ),
        ]);

        let mut engine = ScriptEngine::new();
        let state = engine.execute_file(dir.join("main.vibe")).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        let song_id = state
            .groups
            .iter()
            .find_map(|(id, config)| (config.name == "Song").then_some(*id))
            .expect("Song group should exist");
        let drums_id = state
            .groups
            .iter()
            .find_map(|(id, config)| {
                (config.name == "Drums" && config.parent == Some(song_id)).then_some(*id)
            })
            .expect("nested Drums group should exist");
        let drums = state.groups.get(&drums_id).unwrap();

        assert_eq!(
            state
                .groups
                .values()
                .filter(|config| config.name == "Drums")
                .count(),
            1,
            "imported nested bodies should merge into one Drums group"
        );
        assert_eq!(drums.effects.len(), 1);
        assert!(state.voices.values().all(|voice| voice.group == drums_id));

        let voice_names = state
            .voice_order
            .iter()
            .map(|id| state.voices.get(id).unwrap().name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(voice_names, vec!["kick", "snare"]);

        let effect_synthdefs = state
            .effect_order
            .iter()
            .map(|id| state.effects.get(id).unwrap().synthdef.as_str())
            .collect::<Vec<_>>();
        assert_eq!(effect_synthdefs, vec!["room_reverb"]);

        assert_eq!(
            state
                .body_contributions
                .iter()
                .filter(|body| body.target_group == drums_id)
                .map(|body| body.ordinal)
                .collect::<Vec<_>>(),
            vec![1, 3, 5]
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

        assert_eq!(kit.path, "Drums");
        assert_eq!(beat.path, "Drums");
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

        assert_eq!(target.path, "Song/Drums");
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

        assert_eq!(kit.path, "Drums");
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
        assert!(msg.contains("Drums"), "{msg}");
        assert!(msg.contains("Bass"), "{msg}");
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
            group("Drums").gain(0.5);
            group("Bass").alias("Drums");
        "#,
            )
            .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("group alias 'Drums' collides with canonical group"),
            "{msg}"
        );
        assert!(msg.contains("Drums"), "{msg}");
        assert!(msg.contains("Bass"), "{msg}");
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

        assert_eq!(alias_target.path, "Drums");
        assert_eq!(drums.name, "Drums");
        assert!(drums.parent.is_none());
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

        assert_eq!(alias_target.path, "Drums");
        assert_eq!(kick.group, alias_target.group_id);
        assert_eq!(snare.group, alias_target.group_id);
        assert_eq!(pad.group, song_id);
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "alias body should not create Song/kit"
        );
        assert_eq!(
            state
                .body_contributions
                .iter()
                .filter(|body| body.target_group == alias_target.group_id)
                .map(|body| body.target_path.as_str())
                .collect::<Vec<_>>(),
            vec!["Drums", "Drums"]
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

        assert_eq!(kit.path, "Drums");
        assert_eq!(snare.group, kit.group_id);
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "nested alias lookup should not create Song/Verse/kit"
        );
        assert!(state
            .body_contributions
            .iter()
            .any(|body| body.target_group == kit.group_id && body.target_path == "Drums"));
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

        assert_eq!(drums.path, "Drums");
        assert_eq!(hat.group, drums.group_id);
        assert!(
            state.groups.values().all(|config| config.name != "kit"),
            "define_group should not create Song/kit for an alias"
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

        assert_eq!(fx.path, "Fx");
        assert_eq!(send.path, "Fx");
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

        assert_eq!(first_kit.path, "Drums");
        assert_eq!(second_kit.path, "Drums");
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
        assert!(msg.contains("Song/kit"), "{msg}");
        assert!(msg.contains("Drums"), "{msg}");
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
            (state.quantization.unwrap() - 4.0).abs() < 0.001,
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

        // Default quantization should be unset (runtime default grid)
        assert!(
            state.quantization.is_none(),
            "Default quantization should be unset"
        );
    }

    #[test]
    fn test_quantization_explicit_zero_is_distinct_from_unset() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
            set_tempo(120);
            set_quantization(0); // Explicit: swap immediately, no boundary wait
        "#,
            )
            .unwrap();

        assert_eq!(
            state.quantization,
            Some(0.0),
            "explicit set_quantization(0) must be recorded as Some(0.0), not unset"
        );
    }

    #[test]
    fn synthdef_body_hash_stable_across_evals_and_changes_on_body_edit() {
        vibelang_dsp::set_deploy_callback(|_| Ok(()));
        let mut engine = ScriptEngine::new();

        const SYNTH: &str = "engine_hash_probe_synth";
        let script_v1 = format!(
            r#"
            define_synthdef("{SYNTH}")
                .param("freq", 440.0)
                .body(|freq| {{
                    let sig = saw_ar(freq) * 0.2;
                    [sig, sig]
                }});
            define_group("Probe", || {{
                let v = voice("probe").synth("{SYNTH}");
            }});
        "#
        );

        let first = engine.execute(&script_v1).unwrap();
        let second = engine.execute(&script_v1).unwrap();
        let h1 = first
            .synthdef_hashes
            .get(SYNTH)
            .copied()
            .expect("hash recorded for script-deployed synthdef");
        let h2 = second
            .synthdef_hashes
            .get(SYNTH)
            .copied()
            .expect("hash recorded on re-eval");
        assert_eq!(
            h1, h2,
            "identical synthdef body must hash identically across evals (no reload churn)"
        );

        // Body edit: same name, same params, different gain constant.
        let script_v2 = script_v1.replace("* 0.2", "* 0.3");
        let third = engine.execute(&script_v2).unwrap();
        let h3 = third
            .synthdef_hashes
            .get(SYNTH)
            .copied()
            .expect("hash recorded after body edit");
        assert_ne!(
            h1, h3,
            "a body-only edit must change the synthdef content hash"
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
                (state.quantization.unwrap() - value).abs() < 0.001,
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

        assert!(
            state.quantization.unwrap() >= 0.0,
            "Quantization should be >= 0"
        );
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
        assert!((state.quantization.unwrap() - 4.0).abs() < 0.001);

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
        assert_eq!(route.group, None, "No UMP group filter by default");
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
        assert_eq!(
            route.channel,
            Some(0),
            "Channel 1 (user-facing) stored as 0 (internal)"
        );
        assert_eq!(
            route.curve, "logarithmic",
            "\"log\" maps to \"logarithmic\""
        );
        assert_eq!(route.param, "cutoff", "Param should be cutoff");
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_map_cc_group_and_curve_aliases_are_canonical() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let synth = voice("synth");
                let effect = fx("echo").synth("delay").apply();
                let dev = midi_device("test-device");
                dev.map_cc(18).group(3).curve("s-curve").to(synth, "shape", 0.0, 1.0);
                dev.map_cc(19).curve("unknown").to(synth, "fallback", 0.0, 1.0);
                dev.map_cc(20).to(effect, "mix", 0.0, 1.0);
            "#,
            )
            .unwrap();

        assert_eq!(state.advanced_cc_routes.len(), 3);
        assert_eq!(state.advanced_cc_routes[0].group, Some(3));
        assert_eq!(state.advanced_cc_routes[0].curve, "s_curve");
        assert_eq!(state.advanced_cc_routes[1].group, None);
        assert_eq!(state.advanced_cc_routes[1].curve, "linear");
        assert!(matches!(
            state.advanced_cc_routes[2].target,
            vibelang_core::traits::FadeTarget::Effect(_)
        ));
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_cc32_is_equivalent_alias_into_advanced_registry() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let synth = voice("synth");
                let dev = midi_device("test-device");
                dev.map_cc(74).group(2).channel(1).curve("log").to(synth, "cutoff", 200.0, 8000.0);
                dev.cc32(74).group(2).channel(1).curve("log").to(synth, "cutoff", 200.0, 8000.0);
            "#,
            )
            .unwrap();

        assert_eq!(state.advanced_cc_routes.len(), 2);
        assert_eq!(state.advanced_cc_routes[0], state.advanced_cc_routes[1]);
        assert_eq!(state.advanced_cc_routes[1].curve, "logarithmic");
    }

    #[cfg(feature = "midi")]
    #[test]
    fn test_map_bend_to_voice_registers_advanced_bend_route() {
        let mut engine = ScriptEngine::new();
        let state = engine
            .execute(
                r#"
                let synth = voice("mpk_keys");
                let dev = midi_device("MPK mini 3");
                dev.map_bend().channel(1).curve("exp").to(synth, "morph", 0.0, 1.0);
            "#,
            )
            .unwrap();

        assert_eq!(
            state.advanced_bend_routes.len(),
            1,
            "Should have 1 bend route"
        );
        let route = &state.advanced_bend_routes[0];
        assert_eq!(
            route.channel,
            Some(0),
            "Channel 1 (user-facing) stored as 0 (internal)"
        );
        assert_eq!(
            route.curve, "exponential",
            "\"exp\" maps to \"exponential\""
        );
        assert_eq!(route.param, "morph", "Param should be morph");
        assert!((route.min - 0.0f32).abs() < 0.001, "Min should be 0.0");
        assert!((route.max - 1.0f32).abs() < 0.001, "Max should be 1.0");
        assert!(
            matches!(route.target, vibelang_core::traits::FadeTarget::Voice(_)),
            "Target should be a Voice"
        );
        assert!(
            state.midi_inputs.contains(&route.device_id),
            "Bend mapping should open the device for MIDI input"
        );
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
        assert_eq!(
            route.velocity_curve, "soft",
            "Velocity curve should be soft"
        );
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
        assert_eq!(
            state.voices.len(),
            1,
            "Bare voice() in .to() must not clobber .synth()"
        );
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
        assert_eq!(
            state.voices.values().next().unwrap().synthdef,
            "sample_voice"
        );
    }
}
