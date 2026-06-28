use php_ir::{
    LoweringOptions, lower_frontend_result,
    module::{IrUnit, normalize_class_name},
    verify_unit,
};
use php_optimizer::{PassContext, PassPipeline};
use php_runtime::{
    ErrorReporting, ExitStatus, FilesystemCapabilities, RuntimeContext, RuntimeHttpResponseState,
};
use php_semantics::{FrontendResult, Severity, analyze_source, diagnostics::DiagnosticId};
use php_source::{SourceText, TextRange};
use php_vm::{IncludeLoader, Vm, VmCounters, VmOptions};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::UNIX_EPOCH;

pub use php_optimizer::OptimizationLevel;

const EXIT_SUCCESS: i32 = 0;
const EXIT_PHP_ERROR: i32 = 255;

/// Transport-independent PHP executor.
#[derive(Clone, Debug, Default)]
pub struct PhpExecutor {
    options: PhpExecutorOptions,
}

impl PhpExecutor {
    /// Creates an executor with default VM and optimizer options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an executor with explicit defaults.
    #[must_use]
    pub fn with_options(options: PhpExecutorOptions) -> Self {
        Self { options }
    }

    /// Compiles source into a reusable artifact.
    pub fn compile_source(
        &self,
        input: PhpCompileInput,
    ) -> Result<CompiledPhpScript, PhpExecutionError> {
        let mut pipeline =
            compile_source(&input.source, &input.source_path).map_err(PhpExecutionError::Engine)?;
        apply_optimization(
            &mut pipeline,
            input
                .optimization_level
                .unwrap_or(self.options.optimization_level),
        )
        .map_err(PhpExecutionError::Engine)?;
        if !pipeline.ok() {
            let diagnostics_text =
                render_frontend_diagnostics(&pipeline).map_err(PhpExecutionError::Engine)?;
            return Err(PhpExecutionError::Compile(Box::new(PhpExecutionOutput {
                stdout: Vec::new(),
                diagnostics_text,
                status: PhpExecutionStatus::CompileError,
                runtime_diagnostics: Vec::new(),
                http_response: RuntimeHttpResponseState::default(),
                counters: None,
            })));
        }
        Ok(CompiledPhpScript { pipeline })
    }

    /// Executes a previously compiled script with per-request runtime context.
    #[must_use]
    pub fn execute_compiled(
        &self,
        compiled: &CompiledPhpScript,
        input: PhpRequestExecutionInput,
    ) -> PhpExecutionOutput {
        let include_loader = match include_loader_for_request(&input) {
            Ok(loader) => loader,
            Err(error) => {
                return PhpExecutionOutput::engine_error(error);
            }
        };
        let mut runtime_context = input.runtime_context;
        let mut capabilities = FilesystemCapabilities::none().with_stdio(true);
        if let Some(loader) = &include_loader {
            capabilities = capabilities.with_allowed_roots(loader.allowed_roots().to_vec());
        }
        runtime_context = runtime_context.with_filesystem_capabilities(capabilities);
        let vm = Vm::with_options(VmOptions {
            include_loader,
            runtime_context,
            collect_counters: input.collect_counters,
            ..self.options.vm_options.clone()
        });
        let result = vm.execute(compiled.pipeline.lowering.unit.clone());
        execution_output_from_vm(&compiled.pipeline, result)
    }

    /// Compiles and executes source in one step.
    #[must_use]
    pub fn execute_source(&self, input: PhpExecutionInput) -> PhpExecutionOutput {
        let compiled = match self.compile_source(PhpCompileInput {
            source: input.source,
            source_path: input.source_path,
            optimization_level: input.optimization_level,
        }) {
            Ok(compiled) => compiled,
            Err(PhpExecutionError::Compile(output)) => return *output,
            Err(PhpExecutionError::Engine(error)) => {
                return PhpExecutionOutput::engine_error(error);
            }
        };
        self.execute_compiled(
            &compiled,
            PhpRequestExecutionInput {
                real_path: input.real_path,
                cwd: input.cwd,
                include_roots: input.include_roots,
                runtime_context: input.runtime_context,
                collect_counters: input.collect_counters,
            },
        )
    }
}

/// Executor-wide defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpExecutorOptions {
    pub optimization_level: OptimizationLevel,
    pub vm_options: VmOptions,
}

impl Default for PhpExecutorOptions {
    fn default() -> Self {
        Self {
            optimization_level: OptimizationLevel::O0,
            vm_options: VmOptions::default(),
        }
    }
}

/// Source compilation input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpCompileInput {
    pub source: String,
    pub source_path: String,
    pub optimization_level: Option<OptimizationLevel>,
}

/// Compiled, reusable PHP script artifact.
#[derive(Clone, Debug)]
pub struct CompiledPhpScript {
    pipeline: Pipeline,
}

/// One-shot compile and execute input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpExecutionInput {
    pub source: String,
    pub source_path: String,
    pub real_path: Option<PathBuf>,
    pub cwd: PathBuf,
    pub include_roots: Vec<PathBuf>,
    pub runtime_context: RuntimeContext,
    pub optimization_level: Option<OptimizationLevel>,
    pub collect_counters: bool,
}

/// Per-request execution input for a compiled script.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpRequestExecutionInput {
    pub real_path: Option<PathBuf>,
    pub cwd: PathBuf,
    pub include_roots: Vec<PathBuf>,
    pub runtime_context: RuntimeContext,
    pub collect_counters: bool,
}

/// Owned PHP execution output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpExecutionOutput {
    pub stdout: Vec<u8>,
    pub diagnostics_text: String,
    pub status: PhpExecutionStatus,
    pub runtime_diagnostics: Vec<php_runtime::RuntimeDiagnostic>,
    pub http_response: RuntimeHttpResponseState,
    pub counters: Option<VmCounters>,
}

impl PhpExecutionOutput {
    fn engine_error(error: String) -> Self {
        Self {
            stdout: Vec::new(),
            diagnostics_text: error,
            status: PhpExecutionStatus::Fatal,
            runtime_diagnostics: Vec::new(),
            http_response: RuntimeHttpResponseState::default(),
            counters: None,
        }
    }
}

/// Stable status classification for transport layers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhpExecutionStatus {
    Success,
    CompileError,
    RuntimeError,
    Unsupported,
    Fatal,
}

impl From<ExitStatus> for PhpExecutionStatus {
    fn from(status: ExitStatus) -> Self {
        match status {
            ExitStatus::Success => Self::Success,
            ExitStatus::CompileError => Self::CompileError,
            ExitStatus::RuntimeError => Self::RuntimeError,
            ExitStatus::Unsupported => Self::Unsupported,
            ExitStatus::Fatal => Self::Fatal,
        }
    }
}

/// Executor failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PhpExecutionError {
    Compile(Box<PhpExecutionOutput>),
    Engine(String),
}

/// Sharded process-local cache for immutable compiled scripts.
#[derive(Debug)]
pub struct CompiledScriptCache {
    enabled: bool,
    shards: Vec<Mutex<HashMap<CompiledScriptCacheKey, Arc<CompiledPhpScript>>>>,
    stats: CompiledScriptCacheCounters,
}

impl CompiledScriptCache {
    /// Creates an enabled cache with at least one shard.
    #[must_use]
    pub fn new(shards: usize) -> Self {
        let shard_count = shards.max(1);
        Self {
            enabled: true,
            shards: (0..shard_count)
                .map(|_| Mutex::new(HashMap::new()))
                .collect(),
            stats: CompiledScriptCacheCounters::default(),
        }
    }

    /// Creates a cache facade that always compiles and never stores entries.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            shards: vec![Mutex::new(HashMap::new())],
            stats: CompiledScriptCacheCounters::default(),
        }
    }

    /// Returns a cached script or compiles and stores a fresh artifact.
    pub fn get_or_compile_script(
        &self,
        executor: &PhpExecutor,
        input: PhpScriptCacheInput,
    ) -> Result<CompiledScriptCacheLookup, PhpExecutionError> {
        let source = fs::read_to_string(&input.path).map_err(|error| {
            PhpExecutionError::Engine(format!("{}: {error}", input.path.display()))
        })?;
        let metadata = fs::metadata(&input.path).map_err(|error| {
            PhpExecutionError::Engine(format!("{}: {error}", input.path.display()))
        })?;
        let key = CompiledScriptCacheKey::new(&input, &source, &metadata)?;
        if !self.enabled {
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            return self
                .compile_uncached(executor, input, source)
                .map(|compiled| CompiledScriptCacheLookup {
                    compiled: Arc::new(compiled),
                    hit: false,
                });
        }

        let shard_index = self.shard_index(&key);
        let mut shard = self.shards[shard_index]
            .lock()
            .expect("compiled script cache shard mutex poisoned");
        let stale = remove_stale_path_entries(&mut shard, &key);
        if stale > 0 {
            self.stats
                .stale_invalidations
                .fetch_add(stale as u64, Ordering::Relaxed);
            self.stats
                .entries
                .fetch_sub(stale as u64, Ordering::Relaxed);
        }
        if let Some(compiled) = shard.get(&key) {
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(CompiledScriptCacheLookup {
                compiled: Arc::clone(compiled),
                hit: true,
            });
        }
        self.stats.misses.fetch_add(1, Ordering::Relaxed);
        match self.compile_uncached(executor, input, source) {
            Ok(compiled) => {
                let compiled = Arc::new(compiled);
                shard.insert(key, Arc::clone(&compiled));
                self.stats.entries.fetch_add(1, Ordering::Relaxed);
                Ok(CompiledScriptCacheLookup {
                    compiled,
                    hit: false,
                })
            }
            Err(error) => {
                self.stats.compile_errors.fetch_add(1, Ordering::Relaxed);
                Err(error)
            }
        }
    }

    /// Returns current cache counters.
    #[must_use]
    pub fn cache_stats(&self) -> CompiledScriptCacheStats {
        CompiledScriptCacheStats {
            hits: self.stats.hits.load(Ordering::Relaxed),
            misses: self.stats.misses.load(Ordering::Relaxed),
            stale_invalidations: self.stats.stale_invalidations.load(Ordering::Relaxed),
            compile_errors: self.stats.compile_errors.load(Ordering::Relaxed),
            entries: self.stats.entries.load(Ordering::Relaxed),
        }
    }

    /// Clears all cached entries and resets the approximate entry count.
    pub fn clear(&self) {
        for shard in &self.shards {
            let mut shard = shard
                .lock()
                .expect("compiled script cache shard mutex poisoned");
            shard.clear();
        }
        self.stats.entries.store(0, Ordering::Relaxed);
    }

    fn compile_uncached(
        &self,
        executor: &PhpExecutor,
        input: PhpScriptCacheInput,
        source: String,
    ) -> Result<CompiledPhpScript, PhpExecutionError> {
        executor.compile_source(PhpCompileInput {
            source,
            source_path: input.source_path,
            optimization_level: Some(input.optimization_level),
        })
    }

    fn shard_index(&self, key: &CompiledScriptCacheKey) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.path.hash(&mut hasher);
        (hasher.finish() as usize) % self.shards.len()
    }
}

impl Default for CompiledScriptCache {
    fn default() -> Self {
        Self::new(default_cache_shards())
    }
}

/// File-backed script compilation input for the process-local cache.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhpScriptCacheInput {
    pub path: PathBuf,
    pub source_path: String,
    pub optimization_level: OptimizationLevel,
}

/// Cache lookup result.
#[derive(Clone, Debug)]
pub struct CompiledScriptCacheLookup {
    pub compiled: Arc<CompiledPhpScript>,
    pub hit: bool,
}

/// Snapshot of cache counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompiledScriptCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub stale_invalidations: u64,
    pub compile_errors: u64,
    pub entries: u64,
}

#[derive(Debug, Default)]
struct CompiledScriptCacheCounters {
    hits: AtomicU64,
    misses: AtomicU64,
    stale_invalidations: AtomicU64,
    compile_errors: AtomicU64,
    entries: AtomicU64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CompiledScriptCacheKey {
    path: PathBuf,
    len: u64,
    modified_nanos: u128,
    source_hash: u64,
    optimization_level: &'static str,
    executor_version: &'static str,
    debug_assertions: bool,
}

impl CompiledScriptCacheKey {
    fn new(
        input: &PhpScriptCacheInput,
        source: &str,
        metadata: &fs::Metadata,
    ) -> Result<Self, PhpExecutionError> {
        let path = input.path.canonicalize().map_err(|error| {
            PhpExecutionError::Engine(format!(
                "{}: canonicalize failed: {error}",
                input.path.display()
            ))
        })?;
        let modified_nanos = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos());
        Ok(Self {
            path,
            len: metadata.len(),
            modified_nanos,
            source_hash: stable_source_hash(source),
            optimization_level: input.optimization_level.as_str(),
            executor_version: env!("CARGO_PKG_VERSION"),
            debug_assertions: cfg!(debug_assertions),
        })
    }
}

fn remove_stale_path_entries(
    shard: &mut HashMap<CompiledScriptCacheKey, Arc<CompiledPhpScript>>,
    key: &CompiledScriptCacheKey,
) -> usize {
    let before = shard.len();
    shard.retain(|existing, _| existing.path != key.path || existing == key);
    before.saturating_sub(shard.len())
}

fn stable_source_hash(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn default_cache_shards() -> usize {
    std::thread::available_parallelism().map_or(16, |count| count.get().clamp(1, 64))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CliIniOptions {
    pub include_path: Option<Vec<PathBuf>>,
    pub display_errors: Option<bool>,
    pub error_reporting: Option<i64>,
    /// Raw `-d name=value` ini overrides forwarded to the runtime registry.
    pub overrides: Vec<(String, String)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineInput {
    pub source: String,
    pub source_path: String,
    pub real_path: Option<PathBuf>,
    pub script_name: String,
    pub script_args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub ini: CliIniOptions,
    pub stdin: Vec<u8>,
}

pub fn execute_php<W, E>(input: EngineInput, stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    W: Write,
    E: Write,
{
    let pipeline = compile_source(&input.source, &input.source_path)?;
    if !pipeline.ok() {
        write_frontend_diagnostics(stderr, &pipeline)?;
        return Ok(EXIT_PHP_ERROR);
    }
    let include_loader = include_loader_for(&input)?;
    let runtime_context = runtime_context_for(&input, include_loader.as_ref());
    let vm = Vm::with_options(VmOptions {
        include_loader,
        runtime_context,
        ..VmOptions::default()
    });
    let result = vm.execute(pipeline.lowering.unit.clone());
    stdout
        .write_all(result.output.as_bytes())
        .map_err(|error| error.to_string())?;
    match result.status.exit_status() {
        ExitStatus::Success => Ok(EXIT_SUCCESS),
        ExitStatus::CompileError => {
            if write_vm_compile_fatal_line(stderr, &pipeline, &result.status)? {
                return Ok(EXIT_PHP_ERROR);
            }
            write_runtime_diagnostics(stderr, &input.source_path, &result.diagnostics)?;
            writeln!(stderr, "{}: {}", input.source_path, result.status)
                .map_err(|error| error.to_string())?;
            Ok(EXIT_PHP_ERROR)
        }
        ExitStatus::RuntimeError | ExitStatus::Fatal | ExitStatus::Unsupported => {
            // An uncaught exception has already been rendered to stdout as a PHP
            // `Fatal error:`; emitting the internal diagnostic dump as well would
            // duplicate it and pollute PHPT output comparison.
            let rendered_uncaught = result
                .diagnostics
                .first()
                .is_some_and(|diagnostic| diagnostic.id() == "E_PHP_VM_UNCAUGHT_EXCEPTION");
            if !rendered_uncaught {
                write_runtime_diagnostics(stderr, &input.source_path, &result.diagnostics)?;
                writeln!(stderr, "{}: {}", input.source_path, result.status)
                    .map_err(|error| error.to_string())?;
            }
            Ok(EXIT_PHP_ERROR)
        }
    }
}

#[derive(Clone, Debug)]
struct Pipeline {
    path: String,
    source: SourceText,
    frontend: FrontendResult,
    lowering: php_ir::LoweringResult,
}

impl Pipeline {
    fn ok(&self) -> bool {
        !self.frontend.has_errors()
            && self.lowering.diagnostics.is_empty()
            && self.lowering.verification.is_ok()
    }
}

fn compile_source(source: &str, source_path: &str) -> Result<Pipeline, String> {
    let frontend = analyze_source(source);
    let mut lowering = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_path: source_path.to_string(),
            source_text: Some(source.to_string()),
            ..LoweringOptions::default()
        },
    );
    if !frontend.has_errors() && lowering.verification.is_ok() {
        verify_unit(&lowering.unit).map_err(|errors| {
            format!(
                "{source_path}: IR verification failed: {} error(s)",
                errors.len()
            )
        })?;
        lowering.verification = verify_unit(&lowering.unit);
    }
    Ok(Pipeline {
        path: source_path.to_string(),
        source: SourceText::new(source),
        frontend,
        lowering,
    })
}

fn apply_optimization(
    pipeline: &mut Pipeline,
    optimization_level: OptimizationLevel,
) -> Result<(), String> {
    if !pipeline.ok() || !optimization_level.runs_pipeline() {
        return Ok(());
    }
    PassPipeline::performance()
        .run(
            &mut pipeline.lowering.unit,
            &PassContext::new(optimization_level),
        )
        .map(|_| ())
        .map_err(|error| format!("{}: optimizer failed: {error}", pipeline.path))
}

fn render_frontend_diagnostics(pipeline: &Pipeline) -> Result<String, String> {
    let mut stderr = Vec::new();
    write_frontend_diagnostics(&mut stderr, pipeline)?;
    String::from_utf8(stderr).map_err(|error| error.to_string())
}

fn execution_output_from_vm(pipeline: &Pipeline, result: php_vm::VmResult) -> PhpExecutionOutput {
    let status = PhpExecutionStatus::from(result.status.exit_status());
    let mut diagnostics = Vec::new();
    match result.status.exit_status() {
        ExitStatus::Success => {}
        ExitStatus::CompileError => {
            match write_vm_compile_fatal_line(&mut diagnostics, pipeline, &result.status) {
                Ok(true) => {}
                Ok(false) => {
                    let _ = write_runtime_diagnostics(
                        &mut diagnostics,
                        &pipeline.path,
                        &result.diagnostics,
                    );
                    let _ = writeln!(diagnostics, "{}: {}", pipeline.path, result.status);
                }
                Err(error) => {
                    let _ = writeln!(diagnostics, "{error}");
                }
            }
        }
        ExitStatus::RuntimeError | ExitStatus::Fatal | ExitStatus::Unsupported => {
            let rendered_uncaught = result
                .diagnostics
                .first()
                .is_some_and(|diagnostic| diagnostic.id() == "E_PHP_VM_UNCAUGHT_EXCEPTION");
            if !rendered_uncaught {
                let _ = write_runtime_diagnostics(
                    &mut diagnostics,
                    &pipeline.path,
                    &result.diagnostics,
                );
                let _ = writeln!(diagnostics, "{}: {}", pipeline.path, result.status);
            }
        }
    }
    let diagnostics_text = String::from_utf8(diagnostics).unwrap_or_default();
    PhpExecutionOutput {
        stdout: result.output.as_bytes().to_vec(),
        diagnostics_text,
        status,
        runtime_diagnostics: result.diagnostics,
        http_response: result.http_response,
        counters: result.counters,
    }
}

fn include_loader_for(input: &EngineInput) -> Result<Option<IncludeLoader>, String> {
    let mut roots = Vec::new();
    push_existing_root(&mut roots, &input.cwd);
    if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
        push_existing_root(&mut roots, real_path);
    }
    if let Some(include_path) = &input.ini.include_path {
        for entry in include_path {
            if entry.is_absolute() {
                push_existing_root(&mut roots, entry);
            } else {
                push_existing_root(&mut roots, &input.cwd.join(entry));
                if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
                    push_existing_root(&mut roots, &real_path.join(entry));
                }
            }
        }
    }
    if roots.is_empty() {
        return Ok(None);
    }
    IncludeLoader::new(roots).map(Some)
}

fn include_loader_for_request(
    input: &PhpRequestExecutionInput,
) -> Result<Option<IncludeLoader>, String> {
    let mut roots = Vec::new();
    push_existing_root(&mut roots, &input.cwd);
    if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
        push_existing_root(&mut roots, real_path);
    }
    for entry in &input.include_roots {
        if entry.is_absolute() {
            push_existing_root(&mut roots, entry);
        } else {
            push_existing_root(&mut roots, &input.cwd.join(entry));
            if let Some(real_path) = input.real_path.as_ref().and_then(|path| path.parent()) {
                push_existing_root(&mut roots, &real_path.join(entry));
            }
        }
    }
    if roots.is_empty() {
        return Ok(None);
    }
    IncludeLoader::new(roots).map(Some)
}

fn push_existing_root(roots: &mut Vec<PathBuf>, path: &Path) {
    if path.exists() {
        roots.push(path.to_path_buf());
    }
}

fn runtime_context_for(
    input: &EngineInput,
    include_loader: Option<&IncludeLoader>,
) -> RuntimeContext {
    let include_path = input
        .ini
        .include_path
        .clone()
        .unwrap_or_else(|| vec![PathBuf::from(".")]);
    let mut context =
        RuntimeContext::controlled_cli(input.script_name.clone(), input.script_args.clone())
            .with_cwd(input.cwd.clone())
            .with_include_path(include_path)
            .with_env(input.env.clone())
            .with_ini_overrides(input.ini.overrides.clone())
            .with_stdin(input.stdin.clone());
    if let Some(mask) = input.ini.error_reporting {
        context.ini.error_reporting = ErrorReporting { mask };
    }
    if let Some(display_errors) = input.ini.display_errors {
        context.ini.display_errors = display_errors;
    }
    let mut capabilities = FilesystemCapabilities::none().with_stdio(true);
    if let Some(loader) = include_loader {
        capabilities = capabilities.with_allowed_roots(loader.allowed_roots().to_vec());
    }
    context.with_filesystem_capabilities(capabilities)
}

fn write_frontend_diagnostics<W: Write>(stderr: &mut W, pipeline: &Pipeline) -> Result<(), String> {
    for diagnostic in pipeline.frontend.parser_diagnostics() {
        write_parser_diagnostic(
            stderr,
            &pipeline.path,
            &pipeline.source,
            diagnostic.span,
            diagnostic.id.as_str(),
            &diagnostic.message,
        )?;
    }
    for diagnostic in pipeline.frontend.semantic_diagnostics() {
        if diagnostic.severity() == Severity::Error {
            if let Some(span) = diagnostic.span() {
                if let Some(message) = semantic_diagnostic_php_fatal_message(
                    diagnostic.id(),
                    diagnostic.message(),
                    span,
                    &pipeline.lowering.unit,
                ) {
                    write_php_fatal_line(stderr, &pipeline.path, &pipeline.source, span, &message)?;
                    continue;
                }
                if diagnostic.id() == DiagnosticId::InvalidTypeCallableContext {
                    write_php_fatal_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
                    continue;
                }
                if semantic_diagnostic_uses_php_parse_error_line(diagnostic.id()) {
                    write_php_parse_error_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
                    return Ok(());
                }
                if semantic_diagnostic_uses_php_fatal_line(diagnostic.id()) {
                    write_php_fatal_line(
                        stderr,
                        &pipeline.path,
                        &pipeline.source,
                        span,
                        diagnostic.message(),
                    )?;
                    if semantic_diagnostic_is_immediate_php_fatal(diagnostic.id()) {
                        return Ok(());
                    }
                    continue;
                }
                write_span_line(
                    stderr,
                    &pipeline.path,
                    span,
                    diagnostic.id().as_str(),
                    diagnostic.message(),
                )?;
            } else {
                writeln!(
                    stderr,
                    "{}: {}: {}",
                    pipeline.path,
                    diagnostic.id().as_str(),
                    diagnostic.message()
                )
                .map_err(|error| error.to_string())?;
            }
        }
    }
    for diagnostic in &pipeline.lowering.diagnostics {
        writeln!(
            stderr,
            "{}:{}..{}: {}: {}",
            pipeline.path,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.id,
            diagnostic.message
        )
        .map_err(|error| error.to_string())?;
    }
    if let Err(errors) = &pipeline.lowering.verification {
        writeln!(
            stderr,
            "{}: IR verification failed: {} error(s)",
            pipeline.path,
            errors.len()
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_php_fatal_line<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &SourceText,
    span: TextRange,
    message: &str,
) -> Result<(), String> {
    let line = line_number_for_span(source, span);
    writeln!(stderr, "Fatal error: {message} in {path} on line {line}")
        .map_err(|error| error.to_string())
}

fn write_vm_compile_fatal_line<W: Write>(
    stderr: &mut W,
    pipeline: &Pipeline,
    status: &php_runtime::ExecutionStatus,
) -> Result<bool, String> {
    let Some(message) = status.message() else {
        return Ok(false);
    };
    let Some(display_message) = vm_compile_error_php_fatal_message(message) else {
        return Ok(false);
    };
    let span = if let Some((class_name, method_name)) = vm_compile_error_interface_method(message) {
        class_method_span(&pipeline.lowering.unit, &class_name, &method_name)
    } else if let Some((class_name, _, _)) = vm_compile_error_interface_method_missing(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some((class_name, constant_name)) = vm_compile_error_interface_constant(message) {
        class_constant_span(&pipeline.lowering.unit, &class_name, &constant_name)
    } else if vm_compile_error_interface_property(message) {
        pipeline
            .lowering
            .unit
            .classes
            .iter()
            .find(|class| class.flags.is_interface)
            .map(|class| class.span)
    } else if let Some((class_name, method_name)) = vm_compile_error_child_method(message) {
        class_method_span(&pipeline.lowering.unit, &class_name, &method_name)
    } else if let Some((class_name, _property_name)) = vm_compile_error_child_property(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some((class_name, _constant_name)) = vm_compile_error_child_constant(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some((parent_class, method_name)) = vm_compile_error_final_method(message) {
        overriding_method_span(&pipeline.lowering.unit, &parent_class, &method_name)
    } else if let Some(class_name) = vm_compile_error_traversable_direct(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else if let Some(class_name) = vm_compile_error_child_class(message) {
        class_span(&pipeline.lowering.unit, &class_name)
    } else {
        None
    };
    let Some(span) = span else {
        return Ok(false);
    };
    write_php_fatal_line(
        stderr,
        &pipeline.path,
        &pipeline.source,
        TextRange::new(span.start as usize, span.end as usize),
        &display_message,
    )?;
    Ok(true)
}

fn semantic_diagnostic_php_fatal_message(
    id: DiagnosticId,
    message: &str,
    span: TextRange,
    unit: &IrUnit,
) -> Option<String> {
    match id {
        DiagnosticId::InvalidConstExpr => {
            Some("Constant expression contains invalid operations".to_owned())
        }
        DiagnosticId::DuplicateClassMember => {
            let constant_name = message
                .strip_prefix("duplicate class constant `")?
                .strip_suffix('`')?;
            let class_name = class_display_name_containing_span(unit, span)?;
            Some(format!(
                "Cannot redefine class constant {class_name}::{constant_name}"
            ))
        }
        DiagnosticId::IncompatibleModifiers => match message {
            "`static` modifier is not allowed on class constant" => {
                Some("Cannot use the static modifier on a class constant".to_owned())
            }
            "`abstract` modifier is not allowed on class constant" => {
                Some("Cannot use the abstract modifier on a class constant".to_owned())
            }
            "method cannot be both abstract and final" => {
                Some("Cannot use the final modifier on an abstract method".to_owned())
            }
            _ => None,
        },
        _ => None,
    }
}

fn class_display_name_containing_span(unit: &IrUnit, span: TextRange) -> Option<&str> {
    let start = span.start().to_usize();
    let end = span.end().to_usize();
    unit.classes
        .iter()
        .filter(|class| class.span.start as usize <= start && end <= class.span.end as usize)
        .min_by_key(|class| class.span.end.saturating_sub(class.span.start))
        .map(|class| class.display_name.as_str())
}

fn vm_compile_error_php_fatal_message(message: &str) -> Option<String> {
    if let Some((class_name, interface_name, method_name)) =
        vm_compile_error_interface_method_missing(message)
    {
        return Some(format!(
            "Class {class_name} contains 1 abstract method and must therefore be declared abstract or implement the remaining method ({interface_name}::{method_name})"
        ));
    }

    message
        .strip_prefix("E_PHP_VM_METHOD_VISIBILITY_OVERRIDE: ")
        .or_else(|| message.strip_prefix("E_PHP_VM_STATIC_METHOD_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_VISIBILITY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_BODY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_SIGNATURE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_PROPERTY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_INTERFACE_CONSTANT_VISIBILITY: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_FINAL_CLASS_EXTEND: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_FINAL_METHOD_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_PROPERTY_STATIC_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_PROPERTY_VISIBILITY_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_CLASS_EXTENDS_INTERFACE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_IMPLEMENTS_NON_INTERFACE: "))
        .or_else(|| message.strip_prefix("E_PHP_VM_TRAVERSABLE_DIRECT_IMPLEMENTATION: "))
        .map(str::to_owned)
}

fn vm_compile_error_interface_method(message: &str) -> Option<(String, String)> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_VISIBILITY: ") {
        let target = rest
            .strip_prefix("Access type for interface method ")?
            .split_once("()")?
            .0;
        return split_class_method(target);
    }
    if let Some(rest) = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_BODY: ") {
        let target = rest
            .strip_prefix("Interface function ")?
            .split_once("()")?
            .0;
        return split_class_method(target);
    }
    None
}

fn vm_compile_error_interface_property(message: &str) -> bool {
    message.starts_with("E_PHP_VM_INTERFACE_PROPERTY: ")
}

fn vm_compile_error_interface_method_missing(message: &str) -> Option<(String, String, String)> {
    let rest = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_MISSING: ")?;
    let rest = rest.strip_prefix("class ")?;
    let (class_name, target) = rest.split_once(" must implement ")?;
    let (interface_name, method_name) = target.split_once("::")?;
    Some((
        class_name.to_owned(),
        interface_name.to_owned(),
        method_name.to_owned(),
    ))
}

fn vm_compile_error_interface_constant(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("E_PHP_VM_INTERFACE_CONSTANT_VISIBILITY: ")?;
    let target = rest
        .strip_prefix("Access type for interface constant ")?
        .split_once(" must be public")?
        .0;
    split_class_method(target)
}

fn vm_compile_error_child_method(message: &str) -> Option<(String, String)> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_METHOD_VISIBILITY_OVERRIDE: ") {
        let target = rest.strip_prefix("Access level to ")?.split_once("()")?.0;
        return split_class_method(target);
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_STATIC_METHOD_OVERRIDE: ") {
        let (parent_method, class_name) = rest
            .strip_prefix("Cannot make static method ")
            .and_then(|rest| rest.split_once("() non static in class "))
            .or_else(|| {
                rest.strip_prefix("Cannot make non static method ")
                    .and_then(|rest| rest.split_once("() static in class "))
            })?;
        let (_, method_name) = split_class_method(parent_method)?;
        return Some((class_name.to_owned(), method_name));
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_METHOD_SIGNATURE_OVERRIDE: ") {
        let target = rest
            .strip_prefix("Declaration of ")?
            .split_once(" must be compatible with ")?
            .0;
        return split_class_method(target);
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_INTERFACE_METHOD_SIGNATURE: ") {
        let target = rest
            .strip_prefix("Declaration of ")?
            .split_once(" must be compatible with ")?
            .0;
        return split_class_method(target);
    }

    None
}

fn vm_compile_error_child_class(message: &str) -> Option<String> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_FINAL_CLASS_EXTEND: ") {
        return Some(
            rest.strip_prefix("Class ")?
                .split_once(" cannot extend final class ")?
                .0
                .to_owned(),
        );
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_CLASS_EXTENDS_INTERFACE: ") {
        return Some(
            rest.strip_prefix("Class ")?
                .split_once(" cannot extend interface ")?
                .0
                .to_owned(),
        );
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_IMPLEMENTS_NON_INTERFACE: ") {
        return Some(rest.split_once(" cannot implement ")?.0.to_owned());
    }

    None
}

fn vm_compile_error_final_method(message: &str) -> Option<(String, String)> {
    let target = message
        .strip_prefix("E_PHP_VM_FINAL_METHOD_OVERRIDE: ")?
        .strip_prefix("Cannot override final method ")?
        .split_once("()")?
        .0;
    split_class_method(target)
}

fn vm_compile_error_traversable_direct(message: &str) -> Option<String> {
    let rest = message.strip_prefix("E_PHP_VM_TRAVERSABLE_DIRECT_IMPLEMENTATION: ")?;
    Some(
        rest.strip_prefix("Class ")?
            .split_once(" must implement interface Traversable ")?
            .0
            .to_owned(),
    )
}

fn vm_compile_error_child_property(message: &str) -> Option<(String, String)> {
    if let Some(rest) = message.strip_prefix("E_PHP_VM_PROPERTY_VISIBILITY_OVERRIDE: ") {
        let target = rest
            .strip_prefix("Access level to ")?
            .split_once(" must be ")?
            .0;
        return split_class_property(target);
    }

    if let Some(rest) = message.strip_prefix("E_PHP_VM_PROPERTY_STATIC_OVERRIDE: ") {
        let target = rest
            .split_once(" as static ")
            .or_else(|| rest.split_once(" as non static "))?
            .1;
        return split_class_property(target);
    }

    None
}

fn vm_compile_error_child_constant(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("E_PHP_VM_CLASS_CONSTANT_VISIBILITY_OVERRIDE: ")?;
    let target = rest
        .strip_prefix("Access level to ")?
        .split_once(" must be ")?
        .0;
    split_class_constant(target)
}

fn split_class_method(target: &str) -> Option<(String, String)> {
    let (class_name, method_name) = target.rsplit_once("::")?;
    let method_name = method_name
        .split_once('(')
        .map_or(method_name, |(name, _)| name);
    Some((class_name.to_owned(), method_name.to_owned()))
}

fn split_class_property(target: &str) -> Option<(String, String)> {
    let (class_name, property_name) = target.rsplit_once("::$")?;
    Some((class_name.to_owned(), property_name.to_owned()))
}

fn split_class_constant(target: &str) -> Option<(String, String)> {
    let (class_name, constant_name) = target.rsplit_once("::")?;
    Some((class_name.to_owned(), constant_name.to_owned()))
}

fn class_span(unit: &IrUnit, class_name: &str) -> Option<php_ir::IrSpan> {
    let normalized_class = normalize_class_name(class_name);
    unit.classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)
        .map(|class| class.span)
}

fn class_method_span(unit: &IrUnit, class_name: &str, method_name: &str) -> Option<php_ir::IrSpan> {
    let normalized_class = normalize_class_name(class_name);
    let normalized_method = method_name.to_ascii_lowercase();
    let class = unit
        .classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)?;
    let method = class
        .methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(&normalized_method))?;
    unit.functions
        .get(method.function.index())
        .map(|function| function.span)
}

fn class_constant_span(
    unit: &IrUnit,
    class_name: &str,
    constant_name: &str,
) -> Option<php_ir::IrSpan> {
    let normalized_class = normalize_class_name(class_name);
    let class = unit
        .classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized_class)?;
    class
        .constants
        .iter()
        .find(|constant| constant.name.eq_ignore_ascii_case(constant_name))
        .map(|constant| constant.span)
}

fn overriding_method_span(
    unit: &IrUnit,
    parent_class: &str,
    method_name: &str,
) -> Option<php_ir::IrSpan> {
    let normalized_method = method_name.to_ascii_lowercase();
    unit.classes.iter().find_map(|class| {
        if !unit_class_extends(unit, class, parent_class) {
            return None;
        }
        let method = class
            .methods
            .iter()
            .find(|method| method.name.eq_ignore_ascii_case(&normalized_method))?;
        unit.functions
            .get(method.function.index())
            .map(|function| function.span)
    })
}

fn unit_class_extends(unit: &IrUnit, class: &php_ir::ClassEntry, parent_class: &str) -> bool {
    let normalized_parent = normalize_class_name(parent_class);
    let mut next = class.parent.as_deref();
    while let Some(name) = next {
        if normalize_class_name(name) == normalized_parent {
            return true;
        }
        next = unit
            .classes
            .iter()
            .find(|candidate| normalize_class_name(&candidate.name) == normalize_class_name(name))
            .and_then(|candidate| candidate.parent.as_deref());
    }
    false
}

fn write_php_parse_error_line<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &SourceText,
    span: TextRange,
    message: &str,
) -> Result<(), String> {
    let line = line_number_for_span(source, span);
    writeln!(stderr, "Parse error: {message} in {path} on line {line}")
        .map_err(|error| error.to_string())
}

fn line_number_for_span(source: &SourceText, span: TextRange) -> usize {
    source.line_col(span.start()).line
}

fn write_runtime_diagnostics<W: Write>(
    stderr: &mut W,
    path: &str,
    diagnostics: &[php_runtime::RuntimeDiagnostic],
) -> Result<(), String> {
    for diagnostic in diagnostics {
        writeln!(
            stderr,
            "{path}: runtime-diagnostic: {}",
            diagnostic.to_json()
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_span_line<W: Write>(
    stderr: &mut W,
    path: &str,
    span: TextRange,
    id: &str,
    message: &str,
) -> Result<(), String> {
    writeln!(
        stderr,
        "{}:{}..{}: {}: {}",
        path,
        span.start().to_usize(),
        span.end().to_usize(),
        id,
        message
    )
    .map_err(|error| error.to_string())
}

fn write_parser_diagnostic<W: Write>(
    stderr: &mut W,
    path: &str,
    source: &SourceText,
    span: TextRange,
    id: &str,
    message: &str,
) -> Result<(), String> {
    if message.starts_with("syntax error,") {
        write_php_parse_error_line(stderr, path, source, span, message)
    } else {
        write_span_line(stderr, path, span, id, message)
    }
}

fn semantic_diagnostic_uses_php_fatal_line(id: DiagnosticId) -> bool {
    matches!(
        id,
        DiagnosticId::ClosureUseDuplicatesParameter
            | DiagnosticId::DuplicateClosureUseVariable
            | DiagnosticId::ClosureUseAutoGlobal
            | DiagnosticId::ThisParameter
            | DiagnosticId::ThisReassignment
    )
}

fn semantic_diagnostic_uses_php_parse_error_line(id: DiagnosticId) -> bool {
    matches!(id, DiagnosticId::InvalidClassConstantWrite)
}

fn semantic_diagnostic_is_immediate_php_fatal(id: DiagnosticId) -> bool {
    matches!(
        id,
        DiagnosticId::ThisParameter | DiagnosticId::ThisReassignment
    )
}

pub fn read_script(path: &Path) -> Result<(String, PathBuf, String), String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let real_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let source_path = real_path.to_string_lossy().into_owned();
    Ok((source, real_path, source_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_number_for_span_uses_one_based_source_lines() {
        let source = SourceText::new("<?php\nfunction f(callable&Traversable $x) {}\n");
        assert_eq!(line_number_for_span(&source, TextRange::new(6, 14)), 2);
    }

    #[test]
    fn php_fatal_line_matches_php_compile_error_shape() {
        let source = SourceText::new("<?php\nfunction f(callable&Traversable $x) {}\n");
        let mut stderr = Vec::new();

        write_php_fatal_line(
            &mut stderr,
            "fixture.php",
            &source,
            TextRange::new(6, 14),
            "Type callable cannot be part of an intersection type",
        )
        .expect("fatal line should render");

        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Type callable cannot be part of an intersection type in fixture.php on line 2\n"
        );
    }

    #[test]
    fn php_executor_executes_source() {
        let executor = PhpExecutor::new();
        let output = executor.execute_source(PhpExecutionInput {
            source: "<?php echo \"hello\\n\";".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli("fixture.php", Vec::new()),
            optimization_level: None,
            collect_counters: false,
        });

        assert_eq!(output.status, PhpExecutionStatus::Success);
        assert_eq!(output.stdout, b"hello\n");
        assert!(output.diagnostics_text.is_empty());
        assert!(output.runtime_diagnostics.is_empty());
        assert!(output.counters.is_none());
    }

    #[test]
    fn php_executor_reports_compile_errors() {
        let executor = PhpExecutor::new();
        let output = executor.execute_source(PhpExecutionInput {
            source: "<?php function {".to_owned(),
            source_path: "broken.php".to_owned(),
            real_path: None,
            cwd: std::env::current_dir().expect("current directory"),
            include_roots: Vec::new(),
            runtime_context: RuntimeContext::controlled_cli("broken.php", Vec::new()),
            optimization_level: None,
            collect_counters: false,
        });

        assert_eq!(output.status, PhpExecutionStatus::CompileError);
        assert!(output.stdout.is_empty());
        assert!(
            output.diagnostics_text.contains("Parse error")
                || output.diagnostics_text.contains("syntax error")
                || output.diagnostics_text.contains("expected_identifier"),
            "{}",
            output.diagnostics_text
        );
    }

    #[test]
    fn php_executor_executes_compiled_script_with_http_context() {
        let executor = PhpExecutor::new();
        let compiled = executor
            .compile_source(PhpCompileInput {
                source: "<?php echo $_SERVER['REQUEST_METHOD'], '|', $_GET['name'];".to_owned(),
                source_path: "public/index.php".to_owned(),
                optimization_level: None,
            })
            .expect("compile source");
        let request = php_runtime::RuntimeHttpRequestContext::new(
            "GET",
            "localhost",
            "/index.php?name=phrust",
            "/index.php",
            "/srv/public/index.php",
            "/srv/public",
        );

        let output = executor.execute_compiled(
            &compiled,
            PhpRequestExecutionInput {
                real_path: Some(PathBuf::from("/srv/public/index.php")),
                cwd: PathBuf::from("/srv/public"),
                include_roots: vec![PathBuf::from(".")],
                runtime_context: RuntimeContext::controlled_http(request),
                collect_counters: false,
            },
        );

        assert_eq!(output.status, PhpExecutionStatus::Success);
        assert_eq!(output.stdout, b"GET|phrust");
        assert!(output.diagnostics_text.is_empty());
    }

    #[test]
    fn compiled_script_cache_hits_after_first_compile() {
        let fixture = CacheFixture::new("cache-hit");
        fixture.write("<?php echo \"hi\\n\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(2);

        let first = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("first compile");
        let second = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("second lookup");

        assert!(!first.hit);
        assert!(second.hit);
        assert_eq!(
            cache.cache_stats(),
            CompiledScriptCacheStats {
                hits: 1,
                misses: 1,
                stale_invalidations: 0,
                compile_errors: 0,
                entries: 1,
            }
        );
    }

    #[test]
    fn compiled_script_cache_invalidates_modified_script() {
        let fixture = CacheFixture::new("cache-stale");
        fixture.write("<?php echo \"one\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(1);

        let first = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("first compile");
        fixture.write("<?php echo \"two\";");
        let second = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("second compile");

        assert!(!first.hit);
        assert!(!second.hit);
        assert_eq!(cache.cache_stats().stale_invalidations, 1);
        assert_eq!(cache.cache_stats().entries, 1);
        let output = execute_cached_for_test(&executor, &second.compiled);
        assert_eq!(output.stdout, b"two");
    }

    #[test]
    fn compiled_script_cache_compile_error_does_not_poison_later_success() {
        let fixture = CacheFixture::new("cache-compile-error");
        fixture.write("<?php function {");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::new(1);

        assert!(matches!(
            cache.get_or_compile_script(&executor, fixture.input()),
            Err(PhpExecutionError::Compile(_))
        ));
        fixture.write("<?php echo \"ok\";");
        let lookup = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("successful compile after error");

        assert!(!lookup.hit);
        assert_eq!(cache.cache_stats().compile_errors, 1);
        assert_eq!(cache.cache_stats().entries, 1);
        let output = execute_cached_for_test(&executor, &lookup.compiled);
        assert_eq!(output.stdout, b"ok");
    }

    #[test]
    fn disabled_compiled_script_cache_always_compiles() {
        let fixture = CacheFixture::new("cache-disabled");
        fixture.write("<?php echo \"hi\";");
        let executor = PhpExecutor::new();
        let cache = CompiledScriptCache::disabled();

        let first = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("first compile");
        let second = cache
            .get_or_compile_script(&executor, fixture.input())
            .expect("second compile");

        assert!(!first.hit);
        assert!(!second.hit);
        assert_eq!(cache.cache_stats().hits, 0);
        assert_eq!(cache.cache_stats().misses, 2);
        assert_eq!(cache.cache_stats().entries, 0);
    }

    #[test]
    fn execute_php_renders_vm_class_table_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass Base { public function show() {} }\nclass Child extends Base {\n    protected function show() {}\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Access level to child::show() must be public (as in class base) in fixture.php on line 4\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_property_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass Base { public static $p; }\nclass Child extends Base {\n    public $p;\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Cannot redeclare static Base::$p as non static Child::$p in fixture.php on line 3\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_final_class_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nfinal class Base {}\nclass Child extends Base {}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Class child cannot extend final class base in fixture.php on line 3\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_class_constant_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass Base { public const TOKEN = 1; }\nclass Child extends Base {\n    protected const TOKEN = 2;\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Access level to Child::TOKEN must be public (as in class Base) in fixture.php on line 3\n"
        );
    }

    #[test]
    fn execute_php_renders_vm_interface_signature_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\ninterface Contract { public function __construct(); }\nclass Child implements Contract {\n    public function __construct($value) {}\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Declaration of Child::__construct($value) must be compatible with Contract::__construct() in fixture.php on line 4\n"
        );
    }

    #[test]
    fn execute_php_renders_direct_traversable_compile_error_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass test implements Traversable {\n}\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Class test must implement interface Traversable as part of either Iterator or IteratorAggregate in fixture.php on line 2\n"
        );
    }

    #[test]
    fn execute_php_renders_invalid_const_expr_as_php_fatal() {
        let input = EngineInput {
            source: "<?php\nclass C { const BAD = \"$name\"; }\n".to_owned(),
            source_path: "fixture.php".to_owned(),
            real_path: None,
            script_name: "fixture.php".to_owned(),
            script_args: Vec::new(),
            cwd: std::env::current_dir().expect("current directory"),
            env: Vec::new(),
            ini: CliIniOptions::default(),
            stdin: Vec::new(),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = execute_php(input, &mut stdout, &mut stderr).expect("execute php");

        assert_eq!(code, EXIT_PHP_ERROR);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
            "Fatal error: Constant expression contains invalid operations in fixture.php on line 2\n"
        );
    }

    struct CacheFixture {
        path: PathBuf,
        root: PathBuf,
    }

    impl CacheFixture {
        fn new(name: &str) -> Self {
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "phrust-executor-{name}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir(&root).expect("create cache fixture root");
            let path = root.join("index.php");
            Self { path, root }
        }

        fn write(&self, source: &str) {
            std::fs::write(&self.path, source).expect("write cache fixture");
        }

        fn input(&self) -> PhpScriptCacheInput {
            PhpScriptCacheInput {
                path: self.path.clone(),
                source_path: self.path.to_string_lossy().into_owned(),
                optimization_level: OptimizationLevel::O0,
            }
        }
    }

    impl Drop for CacheFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn execute_cached_for_test(
        executor: &PhpExecutor,
        compiled: &CompiledPhpScript,
    ) -> PhpExecutionOutput {
        executor.execute_compiled(
            compiled,
            PhpRequestExecutionInput {
                real_path: None,
                cwd: std::env::current_dir().expect("current directory"),
                include_roots: Vec::new(),
                runtime_context: RuntimeContext::controlled_cli("index.php", Vec::new()),
                collect_counters: false,
            },
        )
    }
}
