// Audited native ABI surface; see ADR 0017. The product compiler graph always
// includes this module.
use php_ir::module::normalize_class_name;
use php_runtime::api::PhpString;
use php_runtime::api::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod call_dispatch;
mod call_support;
mod diagnostics;
mod dynamic_code;
mod dynamic_units;
mod internal_classes;
mod native_builtins;
mod object_support;
mod request_state;
mod runtime_ops;
mod telemetry;

use dynamic_units::*;

pub(super) use call_dispatch::jit_native_call_dispatch_abi;
use call_support::*;
use diagnostics::*;
pub(super) use dynamic_code::jit_native_dynamic_code_abi;
use internal_classes::*;
use native_builtins::{
    emit_native_deprecated_call, emit_native_float_offset_warning, emit_native_php_diagnostic,
    emit_native_php_warning, execute_native_builtin, native_source_line,
    native_source_line_for_span, native_string,
};
use object_support::*;
use request_state::{
    NativeBacktraceFrame, NativeFunctionNameScope, NativeLastError,
    NativeRegisteredExtensionRequestState,
};
pub(super) use runtime_ops::{
    jit_native_array_fetch_abi, jit_native_array_insert_abi, jit_native_array_new_abi,
    jit_native_array_spread_abi, jit_native_array_unset_abi, jit_native_binary_abi,
    jit_native_cast_abi, jit_native_compare_abi, jit_native_constant_fetch_abi,
    jit_native_echo_abi, jit_native_exception_new_abi, jit_native_execution_poll_abi,
    jit_native_foreach_cleanup_abi, jit_native_foreach_init_abi, jit_native_foreach_next_abi,
    jit_native_local_fetch_abi, jit_native_local_store_abi, jit_native_object_clone_abi,
    jit_native_object_clone_with_abi, jit_native_object_new_abi, jit_native_property_assign_abi,
    jit_native_property_fetch_abi, jit_native_reference_bind_abi, jit_native_return_check_abi,
    jit_native_runtime_fatal_abi, jit_native_truthy_abi, jit_native_unary_abi,
    jit_native_value_lifecycle_abi,
};
use telemetry::NativeRuntimeTelemetry;

thread_local! {
    static ACTIVE_NATIVE_CONTEXT: Cell<*mut ()> = const { Cell::new(std::ptr::null_mut()) };
    static NATIVE_CALL_DEPTH: Cell<usize> = const { Cell::new(0) };
    static NATIVE_INCLUDE_GLOBALS: RefCell<Option<std::collections::BTreeMap<String, Value>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_CONSTANTS: RefCell<Option<std::collections::BTreeMap<String, Value>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_INI: RefCell<Option<php_runtime::api::IniRegistry>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_DEFAULT_TIMEZONE: RefCell<Option<String>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_HTTP_RESPONSE: RefCell<Option<php_runtime::api::RuntimeHttpResponseState>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_FILES: RefCell<Option<std::collections::BTreeSet<std::path::PathBuf>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_MYSQL: RefCell<Option<std::rc::Rc<RefCell<php_runtime::api::MysqlState>>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_FUNCTION_NAMES: RefCell<Option<Rc<NativeFunctionNameScope>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_SYMBOLS: RefCell<Option<NativeIncludeSymbols>> = const { RefCell::new(None) };
    static NATIVE_INCLUDE_EXPORTS: RefCell<Option<NativeIncludeExports>> =
        const { RefCell::new(None) };
}

static NATIVE_TEMPNAM_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
// Real applications routinely cross dozens of PHP frames (for example,
// WordPress metadata and hook dispatch). Keep a deterministic native-stack
// guard, but leave enough headroom for those non-recursive call chains.
const NATIVE_CALL_DEPTH_LIMIT: usize = 256;

struct NativeCallDepthGuard;

impl Drop for NativeCallDepthGuard {
    fn drop(&mut self) {
        NATIVE_CALL_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

fn enter_native_call(name: &str) -> Result<NativeCallDepthGuard, String> {
    NATIVE_CALL_DEPTH.with(|depth| {
        let current = depth.get();
        if current >= NATIVE_CALL_DEPTH_LIMIT {
            return Err(format!(
                "E_PHP_NATIVE_CALL_DEPTH: maximum native call depth of {NATIVE_CALL_DEPTH_LIMIT} exceeded in {name}()"
            ));
        }
        depth.set(current + 1);
        Ok(NativeCallDepthGuard)
    })
}

#[derive(Default)]
struct NativeIncludeExports {
    functions: Vec<(String, php_ir::FunctionId)>,
    native_entries: std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>,
    native_entry_signature_hashes: std::collections::BTreeMap<php_ir::FunctionId, u64>,
    classes: Vec<String>,
    constants: std::collections::BTreeMap<String, Value>,
    autoload_callbacks: Vec<Value>,
    shutdown_callbacks: Vec<NativeShutdownCallback>,
}

#[derive(Clone, Default)]
struct NativeIncludeSymbols {
    external_functions: std::collections::BTreeMap<String, NativeDynamicFunction>,
    dynamic_units: Vec<NativeDynamicUnit>,
    dynamic_classes: std::collections::BTreeSet<String>,
    class_aliases: std::collections::BTreeMap<String, String>,
    autoload_callbacks: Vec<Value>,
    shutdown_callbacks: Vec<NativeShutdownCallback>,
    static_properties: std::collections::BTreeMap<(String, String), Value>,
    static_locals: std::collections::BTreeMap<(u64, u32, u32), php_runtime::api::ReferenceCell>,
    enum_cases: std::collections::BTreeMap<(String, String), php_runtime::api::ObjectRef>,
    destroyed_objects: std::collections::BTreeSet<u64>,
    last_error: Option<NativeLastError>,
}

#[derive(Clone)]
struct NativeShutdownCallback {
    callable: Value,
    arguments: Vec<Value>,
    source: php_ir::Instruction,
}

#[derive(Clone)]
struct NativeErrorHandler {
    callback: Value,
    levels: i64,
}

#[derive(Clone, Copy)]
struct NativeDynamicFunction {
    unit: usize,
    function: php_ir::FunctionId,
}

#[derive(Clone)]
struct NativeDynamicUnit {
    compiled: crate::compiled_unit::CompiledUnit,
    native_entries: std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>,
    native_entry_signature_hashes: std::collections::BTreeMap<php_ir::FunctionId, u64>,
    exported_classes: std::collections::BTreeSet<String>,
}

pub(super) struct NativeExecutionContext<'a> {
    compiled: &'a crate::compiled_unit::CompiledUnit,
    unit: &'a php_ir::IrUnit,
    unit_identity: u64,
    options: &'a super::VmOptions,
    worker_state: &'a super::VmWorkerState,
    native_entries: std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>,
    call_arguments: Vec<Vec<Value>>,
    pub(super) output: php_runtime::api::OutputBuffer,
    values: Vec<Option<NativeStoredValue>>,
    value_refcounts: Vec<u32>,
    resources: php_runtime::api::ResourceTable,
    builtin_request_state: php_runtime::api::BuiltinRequestState,
    registered_extensions: NativeRegisteredExtensionRequestState,
    pub(super) http_response: php_runtime::api::RuntimeHttpResponseState,
    pub(super) upload_registry: php_runtime::api::UploadRegistry,
    pub(super) session: php_runtime::api::SessionState,
    session_global: php_runtime::api::ReferenceCell,
    filter_input_arrays: Rc<std::collections::BTreeMap<i64, php_runtime::api::PhpArray>>,
    ini_registry: php_runtime::api::IniRegistry,
    default_timezone: String,
    mysql_state: std::rc::Rc<RefCell<php_runtime::api::MysqlState>>,
    dynamic_constants: std::collections::BTreeMap<String, Value>,
    visible_function_names: Rc<NativeFunctionNameScope>,
    inherited_autoload_callback_count: usize,
    inherited_shutdown_callback_count: usize,
    dynamic_functions: std::collections::BTreeMap<String, php_ir::FunctionId>,
    external_functions: std::collections::BTreeMap<String, NativeDynamicFunction>,
    dynamic_units: Vec<NativeDynamicUnit>,
    current_dynamic_unit: Option<usize>,
    static_properties: std::collections::BTreeMap<(String, String), Value>,
    static_locals: std::collections::BTreeMap<(u64, u32, u32), php_runtime::api::ReferenceCell>,
    enum_cases: std::collections::BTreeMap<(String, String), php_runtime::api::ObjectRef>,
    generator_iterators: std::collections::BTreeMap<u64, i64>,
    fiber_executions: std::collections::BTreeMap<u64, NativeFiberExecution>,
    active_fiber: Option<u64>,
    pending_fiber_suspension_value: Option<i64>,
    pending_nested_fiber_execution: Option<NativeFiberExecution>,
    completed_nested_fiber_call: Option<(u32, u32, i64)>,
    pending_throwable: Option<Value>,
    called_classes: Vec<String>,
    lexical_scope_classes: Vec<String>,
    call_frames: Vec<NativeBacktraceFrame>,
    dynamic_classes: std::collections::BTreeSet<String>,
    class_aliases: std::collections::BTreeMap<String, String>,
    autoload_callbacks: Vec<Value>,
    shutdown_callbacks: Vec<NativeShutdownCallback>,
    destroyed_objects: std::collections::BTreeSet<u64>,
    autoload_in_progress: std::collections::BTreeSet<String>,
    error_reporting: i64,
    last_error: Option<NativeLastError>,
    error_handlers: Vec<NativeErrorHandler>,
    exception_handlers: Vec<Value>,
    explicit_reference_ids: std::collections::BTreeSet<u64>,
    environment: Vec<(String, String)>,
    included_files: std::collections::BTreeSet<std::path::PathBuf>,
    include_path: Vec<std::path::PathBuf>,
    cwd: std::path::PathBuf,
    inherited_globals: std::collections::BTreeMap<String, Value>,
    continuation_instructions:
        std::sync::Arc<std::collections::BTreeMap<(u32, u32), php_ir::Instruction>>,
    include_child: bool,
    execution_deadline_at: Option<std::time::Instant>,
    execution_deadline_mutable: bool,
    runtime_telemetry: Rc<RefCell<NativeRuntimeTelemetry>>,
    pub(super) diagnostic: Option<php_runtime::api::RuntimeDiagnostic>,
}

enum NativeStoredValue {
    Php(Value),
    Iterator {
        entries: Vec<(Value, Value)>,
        index: usize,
        live_source: Option<i64>,
        live_global: Option<String>,
        live_object: Option<php_runtime::api::ObjectRef>,
        user_iterator: Option<php_runtime::api::ObjectRef>,
        user_iterator_started: bool,
    },
    GeneratorIterator {
        generator: php_runtime::api::GeneratorRef,
        handle: Box<php_jit::JitFunctionHandle>,
        arguments: Vec<i64>,
        state: Box<Option<php_jit::JitDeoptState>>,
        delegation: Option<NativeGeneratorDelegation>,
        yields_seen: u64,
        finished: bool,
    },
}

#[derive(Clone)]
enum NativeGeneratorDelegation {
    Array {
        entries: Vec<(Value, Value)>,
        index: usize,
    },
    Generator {
        generator: php_runtime::api::GeneratorRef,
        iterator: i64,
    },
}

struct NativeFiberExecution {
    handle: php_jit::JitFunctionHandle,
    arguments: Vec<i64>,
    state: php_jit::JitDeoptState,
    nested: Option<Box<NativeFiberExecution>>,
}

impl<'a> NativeExecutionContext<'a> {
    pub(super) const fn process_exit_terminates_process(&self) -> bool {
        self.registered_extensions.is_fork_child()
    }

    pub(super) fn new(
        compiled: &'a crate::compiled_unit::CompiledUnit,
        unit_identity: u64,
        options: &'a super::VmOptions,
        worker_state: &'a super::VmWorkerState,
        output: php_runtime::api::OutputBuffer,
        native_entries: std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>,
    ) -> Self {
        let unit = compiled.unit();
        let inherited_globals = NATIVE_INCLUDE_GLOBALS.with(|globals| globals.borrow_mut().take());
        let inherited_constants =
            NATIVE_INCLUDE_CONSTANTS.with(|constants| constants.borrow_mut().take());
        let inherited_ini = NATIVE_INCLUDE_INI.with(|ini| ini.borrow_mut().take());
        let inherited_default_timezone =
            NATIVE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| timezone.borrow_mut().take());
        let inherited_http_response =
            NATIVE_INCLUDE_HTTP_RESPONSE.with(|response| response.borrow_mut().take());
        let inherited_files = NATIVE_INCLUDE_FILES.with(|files| files.borrow_mut().take());
        let inherited_mysql = NATIVE_INCLUDE_MYSQL.with(|mysql| mysql.borrow_mut().take());
        let inherited_function_names = NATIVE_INCLUDE_FUNCTION_NAMES.with(|names| {
            names
                .borrow_mut()
                .take()
                .unwrap_or_else(|| Rc::new(NativeFunctionNameScope::default()))
        });
        let visible_function_names = NativeFunctionNameScope::child(
            inherited_function_names,
            unit.function_table
                .iter()
                .map(|entry| entry.name.to_ascii_lowercase()),
        );
        let inherited_symbols =
            NATIVE_INCLUDE_SYMBOLS.with(|symbols| symbols.borrow_mut().take().unwrap_or_default());
        let inherited_autoload_callback_count = inherited_symbols.autoload_callbacks.len();
        let inherited_shutdown_callback_count = inherited_symbols.shutdown_callbacks.len();
        let include_child = inherited_globals.is_some();
        let mut inherited_globals = inherited_globals.unwrap_or_default();
        let session = options.runtime_context.session.clone();
        let session_global = inherited_globals
            .get("_SESSION")
            .and_then(|value| match value {
                Value::Reference(reference) => Some(reference.clone()),
                _ => None,
            })
            .unwrap_or_else(|| {
                php_runtime::api::ReferenceCell::new(
                    if session.status() == php_runtime::api::PHP_SESSION_ACTIVE || session.started()
                    {
                        session.data_value()
                    } else {
                        Value::Uninitialized
                    },
                )
            });
        inherited_globals.insert(
            "_SESSION".to_owned(),
            Value::Reference(session_global.clone()),
        );
        let filter_input_arrays = Rc::new(
            [0_i64, 1, 2, 4, 5]
                .into_iter()
                .filter_map(|source| {
                    options
                        .runtime_context
                        .filter_input_array(source)
                        .map(|array| (source, array))
                })
                .collect(),
        );
        let mut resources = php_runtime::api::ResourceTable::new();
        let stdin = resources.register_stdin(options.runtime_context.stdin.to_vec());
        let stdout = resources.register_stdout();
        let stderr = resources.register_stderr();
        let mut dynamic_constants = inherited_constants.unwrap_or_default();
        dynamic_constants
            .entry("STDIN".to_owned())
            .or_insert(Value::Resource(stdin));
        dynamic_constants
            .entry("STDOUT".to_owned())
            .or_insert(Value::Resource(stdout));
        dynamic_constants
            .entry("STDERR".to_owned())
            .or_insert(Value::Resource(stderr));
        let continuation_instructions = compiled.prepared_continuation_instructions();
        Self {
            compiled,
            unit,
            unit_identity,
            options,
            worker_state,
            native_entries,
            call_arguments: Vec::new(),
            output,
            values: Vec::new(),
            value_refcounts: Vec::new(),
            resources,
            builtin_request_state: php_runtime::api::BuiltinRequestState::new(),
            registered_extensions: NativeRegisteredExtensionRequestState::default(),
            http_response: inherited_http_response.unwrap_or_default(),
            upload_registry: options.runtime_context.upload_registry(),
            session,
            session_global,
            filter_input_arrays,
            ini_registry: inherited_ini.unwrap_or_else(|| options.runtime_context.ini_registry()),
            default_timezone: inherited_default_timezone
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned()),
            mysql_state: inherited_mysql
                .unwrap_or_else(|| std::rc::Rc::new(RefCell::new(Default::default()))),
            dynamic_constants,
            visible_function_names,
            inherited_autoload_callback_count,
            inherited_shutdown_callback_count,
            dynamic_functions: std::collections::BTreeMap::new(),
            external_functions: inherited_symbols.external_functions,
            dynamic_units: inherited_symbols.dynamic_units,
            current_dynamic_unit: None,
            static_properties: inherited_symbols.static_properties,
            static_locals: inherited_symbols.static_locals,
            enum_cases: inherited_symbols.enum_cases,
            generator_iterators: std::collections::BTreeMap::new(),
            fiber_executions: std::collections::BTreeMap::new(),
            active_fiber: None,
            pending_fiber_suspension_value: None,
            pending_nested_fiber_execution: None,
            completed_nested_fiber_call: None,
            pending_throwable: None,
            called_classes: Vec::new(),
            lexical_scope_classes: Vec::new(),
            call_frames: Vec::new(),
            dynamic_classes: inherited_symbols.dynamic_classes,
            class_aliases: inherited_symbols.class_aliases,
            autoload_callbacks: inherited_symbols.autoload_callbacks,
            shutdown_callbacks: inherited_symbols.shutdown_callbacks,
            destroyed_objects: inherited_symbols.destroyed_objects,
            autoload_in_progress: std::collections::BTreeSet::new(),
            error_reporting: options.runtime_context.ini.error_reporting.mask,
            last_error: inherited_symbols.last_error,
            error_handlers: Vec::new(),
            exception_handlers: Vec::new(),
            explicit_reference_ids: std::collections::BTreeSet::new(),
            environment: options.runtime_context.env.as_ref().clone(),
            included_files: inherited_files.unwrap_or_default(),
            include_path: options.runtime_context.include_path.clone(),
            cwd: options.runtime_context.cwd.clone(),
            inherited_globals,
            continuation_instructions,
            include_child,
            execution_deadline_at: options
                .runtime_context
                .execution_time_limit
                .and_then(|limit| std::time::Instant::now().checked_add(limit)),
            execution_deadline_mutable: options.runtime_context.execution_time_limit.is_some(),
            runtime_telemetry: Rc::new(RefCell::new(NativeRuntimeTelemetry::default())),
            diagnostic: None,
        }
    }

    fn reset_execution_deadline_seconds(&mut self, seconds: u64) {
        if !self.execution_deadline_mutable {
            return;
        }
        self.execution_deadline_at = if seconds == 0 {
            None
        } else {
            std::time::Instant::now().checked_add(std::time::Duration::from_secs(seconds))
        };
    }

    pub(super) fn install_root_dynamic_unit(
        &mut self,
        compiled: crate::compiled_unit::CompiledUnit,
    ) {
        if self.include_child || self.current_dynamic_unit.is_some() {
            return;
        }
        let unit = self.dynamic_units.len();
        let exported_classes = compiled
            .unit()
            .classes
            .iter()
            .filter(|class| class.span.start != 0 || class.span.end != 0)
            .map(|class| class.name.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let native_entry_signature_hashes = self
            .native_entries
            .keys()
            .copied()
            .map(|function| {
                let signatures = visible_external_function_signatures(self, &compiled, function);
                (
                    function,
                    super::external_function_signatures_hash(&signatures),
                )
            })
            .collect();
        self.dynamic_units.push(NativeDynamicUnit {
            compiled: compiled.clone(),
            native_entry_signature_hashes,
            native_entries: self.native_entries.clone(),
            exported_classes: exported_classes.clone(),
        });
        for entry in &compiled.unit().function_table {
            self.external_functions
                .entry(entry.name.to_ascii_lowercase())
                .or_insert(NativeDynamicFunction {
                    unit,
                    function: entry.function,
                });
        }
        self.dynamic_classes.extend(exported_classes);
        self.current_dynamic_unit = Some(unit);
    }

    fn decode(&self, encoded: i64) -> Result<Value, String> {
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            if constant == u32::MAX {
                return Ok(Value::Null);
            }
            if constant == php_jit::JIT_VALUE_UNINITIALIZED {
                return Ok(Value::Uninitialized);
            }
            let constant = self
                .unit
                .constants
                .get(constant as usize)
                .ok_or_else(|| format!("native constant {constant} is missing"))?;
            // Constants embedded in native operands can still require the
            // active request context (for example a runtime-defined constant
            // used as a default argument in a bounded large-unit call graph).
            return native_runtime_constant_value(self, constant);
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            return match self.values.get(index as usize).and_then(Option::as_ref) {
                Some(NativeStoredValue::Php(value)) => Ok(value.clone()),
                Some(
                    NativeStoredValue::Iterator { .. }
                    | NativeStoredValue::GeneratorIterator { .. },
                ) => Err(format!(
                    "native runtime value {index} is a foreach iterator"
                )),
                None => Err(format!("native runtime value {index} is missing")),
            };
        }
        Ok(Value::Int(encoded))
    }

    fn encode(&mut self, value: Value) -> Result<i64, String> {
        if let Value::Int(value) = value
            && php_jit::jit_decode_constant(value).is_none()
            && php_jit::jit_decode_runtime_value(value).is_none()
        {
            return Ok(value);
        }
        let index = u32::try_from(self.values.len())
            .map_err(|_| "native runtime value table exhausted".to_owned())?;
        self.values.push(Some(NativeStoredValue::Php(value)));
        self.value_refcounts.push(1);
        Ok(php_jit::jit_encode_runtime_value(index))
    }

    fn retain(&mut self, encoded: i64) -> Result<(), String> {
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        let index = index as usize;
        if self.values.get(index).and_then(Option::as_ref).is_none() {
            return Err(format!("native runtime value {index} is missing"));
        }
        let refcount = self
            .value_refcounts
            .get_mut(index)
            .ok_or_else(|| format!("native runtime value {index} has no refcount"))?;
        *refcount = refcount
            .checked_add(1)
            .ok_or_else(|| format!("native runtime value {index} refcount overflow"))?;
        Ok(())
    }

    fn release(&mut self, encoded: i64) -> Result<(), String> {
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        let index = index as usize;
        let reached_zero = {
            let refcount = self
                .value_refcounts
                .get_mut(index)
                .ok_or_else(|| format!("native runtime value {index} has no refcount"))?;
            if *refcount == 0 {
                return Err(format!("native runtime value {index} was already released"));
            }
            *refcount -= 1;
            *refcount == 0
        };
        if reached_zero {
            let value = self
                .values
                .get_mut(index)
                .ok_or_else(|| format!("native runtime value {index} is missing"))?
                .take();
            if let Some(NativeStoredValue::Php(Value::Object(object))) = value {
                let uniquely_owned = object.gc_refcount_estimate() == 1;
                self.record_object_release_root_check(uniquely_owned);
                if uniquely_owned || !self.object_is_request_rooted(object.id()) {
                    self.run_object_destructor(object)?;
                }
            }
        }
        Ok(())
    }

    fn release_if_live(&mut self, encoded: i64) -> Result<(), String> {
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        if self
            .value_refcounts
            .get(index as usize)
            .is_some_and(|refcount| *refcount == 0)
        {
            return Ok(());
        }
        self.release(encoded)
    }

    fn object_is_request_rooted(&self, object_id: u64) -> bool {
        fn reaches_object(
            value: &Value,
            object_id: u64,
            seen_objects: &mut std::collections::HashSet<u64>,
            seen_references: &mut std::collections::HashSet<u64>,
        ) -> bool {
            match value {
                Value::Object(object) => {
                    if object.id() == object_id {
                        return true;
                    }
                    if !seen_objects.insert(object.id()) {
                        return false;
                    }
                    object
                        .try_any_property_value(|value| {
                            reaches_object(value, object_id, seen_objects, seen_references)
                        })
                        .unwrap_or(false)
                }
                Value::Array(array) => array.iter().any(|(_, value)| {
                    reaches_object(value, object_id, seen_objects, seen_references)
                }),
                Value::Reference(reference) => {
                    if !seen_references.insert(reference.gc_debug_id()) {
                        return false;
                    }
                    reference
                        .try_with_value(|value| {
                            reaches_object(value, object_id, seen_objects, seen_references)
                        })
                        .unwrap_or(false)
                }
                _ => false,
            }
        }

        let mut seen_objects = std::collections::HashSet::new();
        let mut seen_references = std::collections::HashSet::new();
        let mut rooted = |value: &Value| {
            reaches_object(value, object_id, &mut seen_objects, &mut seen_references)
        };

        self.static_properties.values().any(&mut rooted)
            || self
                .static_locals
                .values()
                .any(|reference| rooted(&Value::Reference(reference.clone())))
            || self.dynamic_constants.values().any(&mut rooted)
            || self.inherited_globals.values().any(&mut rooted)
            || rooted(&Value::Reference(self.session_global.clone()))
            || self.autoload_callbacks.iter().any(&mut rooted)
            || self.shutdown_callbacks.iter().any(|callback| {
                rooted(&callback.callable) || callback.arguments.iter().any(&mut rooted)
            })
            || self
                .error_handlers
                .iter()
                .any(|handler| rooted(&handler.callback))
            || self.exception_handlers.iter().any(&mut rooted)
            || self.pending_throwable.as_ref().is_some_and(&mut rooted)
            || self
                .enum_cases
                .values()
                .any(|object| object.id() == object_id)
    }

    fn run_object_destructor(&mut self, object: php_runtime::api::ObjectRef) -> Result<(), String> {
        if !self.destroyed_objects.insert(object.id()) {
            return Ok(());
        }
        let class_name = object.class_name();
        let receiver = self.encode(Value::Object(object))?;
        if let Some(function) = self
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(&class_name))
            .and_then(|class| {
                class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__destruct"))
            })
            .map(|method| method.function)
        {
            let _ = invoke_native_method(self, function, &[receiver])?;
        } else if let Some((function, _)) = native_external_method(self, &class_name, "__destruct")
        {
            let _ = invoke_native_external_function(
                self,
                function,
                &[receiver],
                Some(class_name),
                self.unit.strict_types,
            )?;
        }
        self.release(receiver)
    }

    fn function_id(&self, name: &str) -> Option<php_ir::FunctionId> {
        self.unit
            .function_table
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
            .map(|entry| entry.function)
            .or_else(|| {
                self.dynamic_functions
                    .get(&name.to_ascii_lowercase())
                    .copied()
            })
    }

    fn visible_include_function_names(&self) -> Rc<NativeFunctionNameScope> {
        self.visible_function_names.clone()
    }

    fn publish_function_names(&mut self, names: impl IntoIterator<Item = String>) {
        self.visible_function_names =
            NativeFunctionNameScope::child(self.visible_function_names.clone(), names);
    }

    fn take_include_symbols(&mut self) -> NativeIncludeSymbols {
        NativeIncludeSymbols {
            external_functions: std::mem::take(&mut self.external_functions),
            dynamic_units: std::mem::take(&mut self.dynamic_units),
            dynamic_classes: std::mem::take(&mut self.dynamic_classes),
            class_aliases: std::mem::take(&mut self.class_aliases),
            autoload_callbacks: std::mem::take(&mut self.autoload_callbacks),
            shutdown_callbacks: std::mem::take(&mut self.shutdown_callbacks),
            static_properties: std::mem::take(&mut self.static_properties),
            static_locals: std::mem::take(&mut self.static_locals),
            enum_cases: std::mem::take(&mut self.enum_cases),
            destroyed_objects: std::mem::take(&mut self.destroyed_objects),
            last_error: self.last_error.take(),
        }
    }

    fn restore_include_symbols(&mut self, symbols: NativeIncludeSymbols) {
        self.external_functions = symbols.external_functions;
        self.dynamic_units = symbols.dynamic_units;
        self.dynamic_classes = symbols.dynamic_classes;
        self.class_aliases = symbols.class_aliases;
        self.autoload_callbacks = symbols.autoload_callbacks;
        self.shutdown_callbacks = symbols.shutdown_callbacks;
        self.static_properties = symbols.static_properties;
        self.static_locals = symbols.static_locals;
        self.enum_cases = symbols.enum_cases;
        self.destroyed_objects = symbols.destroyed_objects;
        self.last_error = symbols.last_error;
    }

    fn external_function(&self, name: &str) -> Option<NativeDynamicFunction> {
        self.external_functions
            .get(&name.to_ascii_lowercase())
            .copied()
    }

    fn array_mut(&mut self, encoded: i64) -> Result<&mut php_runtime::api::PhpArray, String> {
        let index = php_jit::jit_decode_runtime_value(encoded)
            .ok_or_else(|| "native value is not an array handle".to_owned())?;
        match self.values.get_mut(index as usize).and_then(Option::as_mut) {
            Some(NativeStoredValue::Php(Value::Array(array))) => Ok(array),
            _ => Err(format!("native array value {index} is missing")),
        }
    }

    fn mutate_array(
        &mut self,
        encoded: i64,
        mutate: impl FnOnce(&mut php_runtime::api::PhpArray),
    ) -> Result<(), String> {
        self.mutate_array_with(encoded, mutate)
    }

    fn mutate_array_with<T>(
        &mut self,
        encoded: i64,
        mutate: impl FnOnce(&mut php_runtime::api::PhpArray) -> T,
    ) -> Result<T, String> {
        if let Value::Reference(reference) = self.decode(encoded)? {
            let mut value = reference.get();
            let Value::Array(array) = &mut value else {
                return Err("native reference does not contain an array".to_owned());
            };
            let result = mutate(array);
            reference.set(value);
            return Ok(result);
        }
        Ok(mutate(self.array_mut(encoded)?))
    }

    fn encode_iterator(
        &mut self,
        entries: Vec<(Value, Value)>,
        live_source: Option<i64>,
        live_global: Option<String>,
        live_object: Option<php_runtime::api::ObjectRef>,
        user_iterator: Option<php_runtime::api::ObjectRef>,
    ) -> Result<i64, String> {
        let index = u32::try_from(self.values.len())
            .map_err(|_| "native runtime value table exhausted".to_owned())?;
        self.values.push(Some(NativeStoredValue::Iterator {
            entries,
            index: 0,
            live_source,
            live_global,
            live_object,
            user_iterator,
            user_iterator_started: false,
        }));
        self.value_refcounts.push(1);
        Ok(php_jit::jit_encode_runtime_value(index))
    }

    fn encode_generator_iterator(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        let function = php_ir::FunctionId::new(generator.function());
        let handle = ensure_native_entry(self, function)?;
        let arguments = generator
            .args()
            .into_iter()
            .map(|value| self.encode(value))
            .collect::<Result<Vec<_>, _>>()?;
        let index = u32::try_from(self.values.len())
            .map_err(|_| "native runtime value table exhausted".to_owned())?;
        self.values.push(Some(NativeStoredValue::GeneratorIterator {
            generator,
            handle: Box::new(handle),
            arguments,
            state: Box::new(None),
            delegation: None,
            yields_seen: 0,
            finished: false,
        }));
        self.value_refcounts.push(1);
        Ok(php_jit::jit_encode_runtime_value(index))
    }

    fn generator_iterator(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        if let Some(encoded) = self.generator_iterators.get(&generator.id()).copied() {
            return Ok(encoded);
        }
        let id = generator.id();
        let encoded = self.encode_generator_iterator(generator)?;
        self.generator_iterators.insert(id, encoded);
        Ok(encoded)
    }

    fn generator_resume(
        &mut self,
        encoded: i64,
        resume_kind: php_jit::JitNativeResumeInputKind,
        resume_value: i64,
    ) -> Result<Option<(Value, Value)>, String> {
        let index = php_jit::jit_decode_runtime_value(encoded)
            .ok_or_else(|| "native value is not a foreach iterator handle".to_owned())?;
        let user_iterator = match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Iterator {
                user_iterator: Some(object),
                user_iterator_started,
                ..
            }) => Some((object.clone(), *user_iterator_started)),
            _ => None,
        };
        if let Some((object, started)) = user_iterator {
            let class_name = object.class_name();
            let receiver = self.encode(Value::Object(object))?;
            if started {
                let next = native_method_in_hierarchy(self, &class_name, "next")
                    .ok_or_else(|| "Iterator::next() is missing".to_owned())?;
                invoke_native_method(self, next, &[receiver])?;
            }
            let valid = native_method_in_hierarchy(self, &class_name, "valid")
                .ok_or_else(|| "Iterator::valid() is missing".to_owned())?;
            let valid = invoke_native_method(self, valid, &[receiver])?;
            if !native_property_truthy(&self.decode(valid)?) {
                return Ok(None);
            }
            let current = native_method_in_hierarchy(self, &class_name, "current")
                .ok_or_else(|| "Iterator::current() is missing".to_owned())?;
            let key = native_method_in_hierarchy(self, &class_name, "key")
                .ok_or_else(|| "Iterator::key() is missing".to_owned())?;
            let current = invoke_native_method(self, current, &[receiver])?;
            let key = invoke_native_method(self, key, &[receiver])?;
            if let Some(NativeStoredValue::Iterator {
                user_iterator_started,
                ..
            }) = self.values.get_mut(index as usize).and_then(Option::as_mut)
            {
                *user_iterator_started = true;
            }
            return Ok(Some((self.decode(key)?, self.decode(current)?)));
        }
        let object_entry = match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Iterator {
                entries,
                index,
                live_object: Some(object),
                ..
            }) => entries
                .get(*index)
                .map(|(key, _)| (object.clone(), key.clone(), *index)),
            _ => None,
        };
        if let Some((object, key, cursor)) = object_entry {
            let name = match &key {
                Value::Int(key) => key.to_string(),
                Value::String(key) => key.to_string_lossy(),
                _ => return Err("native object iterator key is invalid".to_owned()),
            };
            let value = object.get_property(&name).unwrap_or(Value::Null);
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            if let Some(NativeStoredValue::Iterator { index, .. }) =
                self.values.get_mut(index as usize).and_then(Option::as_mut)
            {
                *index = cursor.saturating_add(1);
            }
            return Ok(Some((key, value)));
        }
        let live = match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Iterator {
                index,
                live_source: Some(source),
                live_global,
                ..
            }) => Some((*source, *index, live_global.clone())),
            _ => None,
        };
        if let Some((source, cursor, live_global)) = live {
            let reference_entry = |array: &mut php_runtime::api::PhpArray| {
                let (key, value) = array
                    .iter()
                    .nth(cursor)
                    .map(|(key, value)| (key.clone(), value.clone()))?;
                let reference = match value {
                    Value::Reference(reference) => reference,
                    value => {
                        let reference = php_runtime::api::ReferenceCell::new(value);
                        array.insert(key.clone(), Value::Reference(reference.clone()));
                        reference
                    }
                };
                let key = match key {
                    php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                    php_runtime::api::ArrayKey::String(key) => Value::String(key),
                };
                Some((key, Value::Reference(reference)))
            };
            let entry = if let Some(global) = live_global {
                let Some(root) = self.inherited_globals.get(&global).cloned() else {
                    return Ok(None);
                };
                match root {
                    Value::Reference(reference) => {
                        let Value::Array(mut array) = reference.get() else {
                            return Ok(None);
                        };
                        let entry = reference_entry(&mut array);
                        reference.set(Value::Array(array));
                        entry
                    }
                    Value::Array(mut array) => {
                        let entry = reference_entry(&mut array);
                        self.inherited_globals.insert(global, Value::Array(array));
                        entry
                    }
                    _ => None,
                }
            } else {
                self.mutate_array_with(source, reference_entry)?
            };
            let Some(entry) = entry else {
                return Ok(None);
            };
            if let Some(NativeStoredValue::Iterator { index: cursor, .. }) =
                self.values.get_mut(index as usize).and_then(Option::as_mut)
            {
                *cursor = cursor.saturating_add(1);
            }
            return Ok(Some(entry));
        }
        if let Some(NativeStoredValue::Iterator {
            entries,
            index: cursor,
            ..
        }) = self.values.get_mut(index as usize).and_then(Option::as_mut)
        {
            let entry = entries.get(*cursor).cloned().map(|(key, value)| {
                let value = match value {
                    Value::Reference(reference) => reference.get(),
                    value => value,
                };
                (key, value)
            });
            *cursor = cursor.saturating_add(usize::from(entry.is_some()));
            return Ok(entry);
        }
        let (generator, handle, arguments, state, delegation, finished) =
            match self.values.get(index as usize).and_then(Option::as_ref) {
                Some(NativeStoredValue::GeneratorIterator {
                    generator,
                    handle,
                    arguments,
                    state,
                    delegation,
                    finished,
                    ..
                }) => (
                    generator.clone(),
                    handle.clone(),
                    arguments.clone(),
                    state.clone(),
                    delegation.clone(),
                    *finished,
                ),
                _ => return Err(format!("native foreach iterator {index} is missing")),
            };
        if finished {
            return Ok(None);
        }
        let mut effective_resume_kind = resume_kind;
        let mut effective_resume_value = resume_value;
        if let Some(delegation) = delegation {
            match delegation {
                NativeGeneratorDelegation::Array {
                    entries,
                    index: cursor,
                } => {
                    if let Some((key, value)) = entries.get(cursor).cloned() {
                        if let Some(NativeStoredValue::GeneratorIterator {
                            delegation: Some(NativeGeneratorDelegation::Array { index: cursor, .. }),
                            ..
                        }) = self.values.get_mut(index as usize).and_then(Option::as_mut)
                        {
                            *cursor = cursor.saturating_add(1);
                        }
                        generator.suspend_forwarded(Some(key.clone()), value.clone());
                        if let Some(NativeStoredValue::GeneratorIterator { yields_seen, .. }) =
                            self.values.get_mut(index as usize).and_then(Option::as_mut)
                        {
                            *yields_seen = yields_seen.saturating_add(1);
                        }
                        return Ok(Some((key, value)));
                    }
                    if let Some(NativeStoredValue::GeneratorIterator { delegation, .. }) =
                        self.values.get_mut(index as usize).and_then(Option::as_mut)
                    {
                        *delegation = None;
                    }
                    effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                    effective_resume_value = php_jit::jit_encode_constant(u32::MAX);
                }
                NativeGeneratorDelegation::Generator {
                    generator: delegated,
                    iterator,
                } => {
                    if let Some((key, value)) = self.iterator_next(iterator)? {
                        generator.suspend_forwarded(Some(key.clone()), value.clone());
                        if let Some(NativeStoredValue::GeneratorIterator { yields_seen, .. }) =
                            self.values.get_mut(index as usize).and_then(Option::as_mut)
                        {
                            *yields_seen = yields_seen.saturating_add(1);
                        }
                        return Ok(Some((key, value)));
                    }
                    effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                    effective_resume_value =
                        self.encode(delegated.return_value().unwrap_or(Value::Null))?;
                    if let Some(NativeStoredValue::GeneratorIterator { delegation, .. }) =
                        self.values.get_mut(index as usize).and_then(Option::as_mut)
                    {
                        *delegation = None;
                    }
                }
            }
        }
        let outcome = if let Some(state) = state.as_ref() {
            handle.invoke_i64_suspension_resume_with_native_unwind(
                &arguments,
                state,
                effective_resume_kind,
                effective_resume_value,
                php_jit::JIT_RUNTIME_ABI_HASH,
                |types, value| native_catch_matches(self, types, value),
            )
        } else {
            handle.invoke_i64_with_deopt(&arguments, php_jit::JIT_RUNTIME_ABI_HASH)
        }
        .map_err(|error| format!("native generator invocation failed: {error:?}"))?;
        match outcome {
            php_jit::JitI64InvokeOutcome::SideExit {
                status,
                value,
                state,
            } if status == php_jit::JitCallStatus::SUSPEND_GENERATOR.0 as i32 => {
                if state.suspend_kind == php_jit::JitNativeSuspendKind::GENERATOR_DELEGATE.0 {
                    let delegated = self.decode(state.delegation_handle as i64)?;
                    let delegation = match delegated {
                        Value::Array(array) => NativeGeneratorDelegation::Array {
                            entries: array
                                .iter()
                                .map(|(key, value)| {
                                    let key = match key {
                                        php_runtime::api::ArrayKey::Int(value) => Value::Int(value),
                                        php_runtime::api::ArrayKey::String(value) => {
                                            Value::String(value.clone())
                                        }
                                    };
                                    (key, value.clone())
                                })
                                .collect(),
                            index: 0,
                        },
                        Value::Generator(delegated) => NativeGeneratorDelegation::Generator {
                            iterator: self.generator_iterator(delegated.clone())?,
                            generator: delegated,
                        },
                        other => {
                            return Err(format!(
                                "yield from expects an array or Traversable, got {}",
                                native_value_type_name(&other)
                            ));
                        }
                    };
                    if let Some(NativeStoredValue::GeneratorIterator {
                        state: saved_state,
                        delegation: saved_delegation,
                        ..
                    }) = self.values.get_mut(index as usize).and_then(Option::as_mut)
                    {
                        **saved_state = Some(state);
                        *saved_delegation = Some(delegation);
                    }
                    return self.iterator_next(encoded);
                }
                let key = if state.suspend_flags & 1 != 0 {
                    Some(self.decode(state.yielded_key)?)
                } else {
                    None
                };
                let value = self.decode(value)?;
                generator.suspend(key, value.clone());
                if let Some(NativeStoredValue::GeneratorIterator {
                    state: saved_state, ..
                }) = self.values.get_mut(index as usize).and_then(Option::as_mut)
                {
                    **saved_state = Some(state);
                }
                if let Some(NativeStoredValue::GeneratorIterator { yields_seen, .. }) =
                    self.values.get_mut(index as usize).and_then(Option::as_mut)
                {
                    *yields_seen = yields_seen.saturating_add(1);
                }
                let (key, value) = generator
                    .current()
                    .ok_or_else(|| "native generator suspension value is missing".to_owned())?;
                Ok(Some((key.unwrap_or(Value::Null), value)))
            }
            php_jit::JitI64InvokeOutcome::Returned(value)
            | php_jit::JitI64InvokeOutcome::SideExit {
                status: 1 | 2,
                value,
                ..
            } => {
                generator.close(Some(self.decode(value)?));
                if let Some(NativeStoredValue::GeneratorIterator { finished, .. }) =
                    self.values.get_mut(index as usize).and_then(Option::as_mut)
                {
                    *finished = true;
                }
                Ok(None)
            }
            php_jit::JitI64InvokeOutcome::SideExit { status, .. } => {
                Err(format!("native generator returned status {status}"))
            }
        }
    }

    fn iterator_next(&mut self, encoded: i64) -> Result<Option<(Value, Value)>, String> {
        self.generator_resume(
            encoded,
            php_jit::JitNativeResumeInputKind::VALUE,
            php_jit::jit_encode_constant(u32::MAX),
        )
    }

    fn generator_can_rewind(&self, encoded: i64) -> bool {
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return false;
        };
        matches!(
            self.values.get(index as usize).and_then(Option::as_ref),
            Some(NativeStoredValue::GeneratorIterator {
                yields_seen: 0 | 1,
                finished: false,
                ..
            })
        )
    }

    fn close_iterator(&mut self, encoded: i64) -> Result<(), String> {
        let index = php_jit::jit_decode_runtime_value(encoded)
            .ok_or_else(|| "native value is not a foreach iterator handle".to_owned())?;
        let value = self
            .values
            .get_mut(index as usize)
            .ok_or_else(|| format!("native foreach iterator {index} is missing"))?;
        match value.take() {
            Some(
                NativeStoredValue::Iterator { .. } | NativeStoredValue::GeneratorIterator { .. },
            ) => {
                if let Some(refcount) = self.value_refcounts.get_mut(index as usize) {
                    *refcount = 0;
                }
                Ok(())
            }
            other => {
                *value = other;
                Err(format!("native foreach iterator {index} is missing"))
            }
        }
    }

    fn instruction_for_continuation(
        &self,
        function: u32,
        continuation: u32,
    ) -> Option<&php_ir::Instruction> {
        if let Some(instruction) = self
            .continuation_instructions
            .get(&(function, continuation))
        {
            return Some(instruction);
        }
        let function = self.unit.functions.get(function as usize)?;
        let mut current = 0_u32;
        for block in &function.blocks {
            for instruction in &block.instructions {
                if current == continuation {
                    return Some(instruction);
                }
                current = current.saturating_add(1);
            }
            current = current.saturating_add(1);
        }
        None
    }

    fn instruction_for_source(
        &self,
        function: u32,
        block: u32,
        instruction: u32,
    ) -> Option<&php_ir::Instruction> {
        self.unit
            .functions
            .get(function as usize)?
            .blocks
            .get(block as usize)?
            .instructions
            .iter()
            .find(|candidate| candidate.id.raw() == instruction)
    }

    fn lookup_constant(&self, name: &str) -> Result<Value, String> {
        if let Some(value) = self.dynamic_constants.get(name) {
            return Ok(value.clone());
        }
        if let Some(constant) = self
            .unit
            .constant_table
            .iter()
            .find(|constant| constant.name == name)
            .and_then(|constant| self.unit.constants.get(constant.value.index()))
        {
            return ir_constant_value(constant);
        }
        php_std::ExtensionRegistry::standard_library()
            .enabled_constant(name)
            .and_then(php_std::ConstantDescriptor::value)
            .map(php_std::constants::constant_to_value)
            .ok_or_else(|| format!("Undefined constant \"{name}\""))
    }

    fn visible_include_constants(&self) -> std::collections::BTreeMap<String, Value> {
        let mut constants = self.dynamic_constants.clone();
        for entry in &self.unit.constant_table {
            if let Some(value) = self.unit.constants.get(entry.value.index())
                && let Ok(value) = ir_constant_value(value)
            {
                constants.entry(entry.name.clone()).or_insert(value);
            }
        }
        constants
    }

    pub(super) fn decode_result(&self, encoded: i64) -> Result<Value, String> {
        self.decode(encoded)
    }

    fn record_last_error(&mut self, error_type: i64, message: &str, file: &str, line: usize) {
        self.last_error = Some(NativeLastError {
            error_type,
            message: message.to_owned(),
            file: file.to_owned(),
            line,
        });
    }

    fn last_error_value(&self) -> Value {
        let Some(error) = &self.last_error else {
            return Value::Null;
        };
        let mut value = php_runtime::api::PhpArray::new();
        for (name, field) in [
            ("type", Value::Int(error.error_type)),
            (
                "message",
                Value::String(PhpString::from_bytes(error.message.as_bytes().to_vec())),
            ),
            (
                "file",
                Value::String(PhpString::from_bytes(error.file.as_bytes().to_vec())),
            ),
            (
                "line",
                Value::Int(i64::try_from(error.line).unwrap_or(i64::MAX)),
            ),
        ] {
            value.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
                field,
            );
        }
        Value::Array(value)
    }

    pub(super) fn take_pending_throwable(&mut self) -> Option<Value> {
        self.pending_throwable.take()
    }

    pub(super) fn run_shutdown_callbacks(&mut self) -> Result<(), String> {
        if self.include_child {
            return Ok(());
        }
        while !self.shutdown_callbacks.is_empty() {
            let NativeShutdownCallback {
                callable,
                arguments,
                source,
            } = self.shutdown_callbacks.remove(0);
            let result = invoke_native_callable_value(self, callable, &arguments, &source, None);
            if matches!(&result, Err(error) if error == "E_PHP_RETHROW")
                && let Some(throwable) = self.pending_throwable.take()
            {
                self.pending_throwable = Some(native_throwable_with_internal_frame(
                    self, throwable, &source,
                ));
            }
            result?;
        }
        loop {
            let mut objects = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for stored in &self.values {
                let Some(NativeStoredValue::Php(Value::Object(object))) = stored else {
                    continue;
                };
                if !self.destroyed_objects.contains(&object.id()) && seen.insert(object.id()) {
                    objects.push(object.clone());
                }
            }
            let Some(object) = objects.pop() else {
                break;
            };
            self.destroyed_objects.insert(object.id());
            let class_name = object.class_name();
            let receiver = self.encode(Value::Object(object))?;
            if let Some(function) = self
                .unit
                .classes
                .iter()
                .find(|class| class.name == normalize_class_name(&class_name))
                .and_then(|class| {
                    class
                        .methods
                        .iter()
                        .find(|method| method.name.eq_ignore_ascii_case("__destruct"))
                })
                .map(|method| method.function)
            {
                let _ = invoke_native_method(self, function, &[receiver])?;
            } else if let Some((function, _)) =
                native_external_method(self, &class_name, "__destruct")
            {
                let _ = invoke_native_external_function(
                    self,
                    function,
                    &[receiver],
                    Some(class_name),
                    self.unit.strict_types,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn handle_uncaught_throwable(&mut self, encoded: i64) -> Result<bool, String> {
        let Some(handler) = self.exception_handlers.last().cloned() else {
            return Ok(false);
        };
        let throwable = self.decode(encoded)?;
        let source = self
            .unit
            .functions
            .get(self.unit.entry.index())
            .and_then(|function| {
                function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .next()
            })
            .cloned()
            .ok_or_else(|| "exception handler call source is missing".to_owned())?;
        let _ = invoke_native_callable_value(self, handler, &[throwable], &source, None)?;
        Ok(true)
    }

    pub(super) fn publish_include_globals(&mut self) {
        if self.include_child {
            let entry_file = self
                .unit
                .functions
                .get(self.unit.entry.index())
                .map(|function| function.span.file);
            NATIVE_INCLUDE_GLOBALS.with(|globals| {
                globals.replace(Some(std::mem::take(&mut self.inherited_globals)));
            });
            NATIVE_INCLUDE_INI.with(|ini| {
                ini.replace(Some(std::mem::take(&mut self.ini_registry)));
            });
            NATIVE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| {
                timezone.replace(Some(std::mem::take(&mut self.default_timezone)));
            });
            NATIVE_INCLUDE_HTTP_RESPONSE.with(|response| {
                response.replace(Some(std::mem::take(&mut self.http_response)));
            });
            NATIVE_INCLUDE_FILES.with(|files| {
                files.replace(Some(std::mem::take(&mut self.included_files)));
            });
            NATIVE_INCLUDE_MYSQL.with(|mysql| {
                mysql.replace(Some(self.mysql_state.clone()));
            });
            let mut functions = self
                .unit
                .function_table
                .iter()
                .map(|entry| (entry.name.clone(), entry.function))
                .collect::<Vec<_>>();
            functions.extend(
                self.dynamic_functions
                    .iter()
                    .map(|(name, function)| (name.clone(), *function)),
            );
            let classes = self
                .unit
                .classes
                .iter()
                .filter(|class| {
                    (!class.flags.is_conditional
                        || self
                            .dynamic_classes
                            .contains(&normalize_class_name(&class.name)))
                        && (class.span.start != 0 || class.span.end != 0)
                        && entry_file.is_none_or(|file| class.span.file == file)
                })
                .map(|class| class.name.clone())
                .collect::<Vec<_>>();
            let mut constants = std::collections::BTreeMap::new();
            for entry in &self.unit.constant_table {
                if entry_file.is_none_or(|file| entry.span.file == file)
                    && let Some(value) = self.unit.constants.get(entry.value.index())
                    && let Ok(value) = ir_constant_value(value)
                {
                    constants.insert(entry.name.clone(), value);
                }
            }
            NATIVE_INCLUDE_CONSTANTS.with(|constants| {
                constants.replace(Some(std::mem::take(&mut self.dynamic_constants)));
            });
            let autoload_callbacks = self
                .autoload_callbacks
                .split_off(self.inherited_autoload_callback_count);
            let shutdown_callbacks = self
                .shutdown_callbacks
                .split_off(self.inherited_shutdown_callback_count);
            let native_entry_signature_hashes = self
                .native_entries
                .keys()
                .copied()
                .map(|function| {
                    let signatures =
                        visible_external_function_signatures(self, self.compiled, function);
                    (
                        function,
                        super::external_function_signatures_hash(&signatures),
                    )
                })
                .collect();
            let mut symbols = self.take_include_symbols();
            for class in &classes {
                symbols.dynamic_classes.remove(&normalize_class_name(class));
            }
            NATIVE_INCLUDE_SYMBOLS.with(|slot| {
                slot.replace(Some(symbols));
            });
            NATIVE_INCLUDE_EXPORTS.with(|exports| {
                exports.replace(Some(NativeIncludeExports {
                    functions,
                    native_entry_signature_hashes,
                    native_entries: std::mem::take(&mut self.native_entries),
                    classes,
                    constants,
                    autoload_callbacks,
                    shutdown_callbacks,
                }));
            });
        }
    }
}

pub(super) struct NativeExecutionContextGuard {
    previous: *mut (),
}

impl Drop for NativeExecutionContextGuard {
    fn drop(&mut self) {
        ACTIVE_NATIVE_CONTEXT.with(|active| active.set(self.previous));
    }
}

pub(super) fn activate_native_context(
    context: &mut NativeExecutionContext<'_>,
) -> NativeExecutionContextGuard {
    let pointer = context as *mut NativeExecutionContext<'_> as *mut ();
    let previous = ACTIVE_NATIVE_CONTEXT.with(|active| active.replace(pointer));
    NativeExecutionContextGuard { previous }
}

#[allow(unsafe_code)]
fn with_native_context<R>(
    operation: impl FnOnce(&mut NativeExecutionContext<'_>) -> R,
) -> Option<R> {
    ACTIVE_NATIVE_CONTEXT.with(|active| {
        let pointer = active.get();
        if pointer.is_null() {
            return None;
        }
        // SAFETY: `activate_native_context` installs a live, thread-confined
        // context for the exact synchronous duration of native entry execution.
        Some(operation(unsafe {
            &mut *(pointer as *mut NativeExecutionContext<'_>)
        }))
    })
}

fn with_native_context_for<R>(
    helper_id: &'static str,
    operation: impl FnOnce(&mut NativeExecutionContext<'_>) -> R,
) -> Option<R> {
    with_native_context(|context| {
        let collect_counters = context.options.collect_counters;
        if collect_counters {
            context.enter_runtime_helper(helper_id);
        }
        let result = operation(context);
        if collect_counters {
            context.exit_runtime_helper(helper_id);
        }
        result
    })
}

fn ir_constant_value(constant: &php_ir::IrConstant) -> Result<Value, String> {
    match constant {
        php_ir::IrConstant::Null => Ok(Value::Null),
        php_ir::IrConstant::Bool(value) => Ok(Value::Bool(*value)),
        php_ir::IrConstant::Int(value) => Ok(Value::Int(*value)),
        php_ir::IrConstant::Float(value) => Ok(Value::float(*value)),
        php_ir::IrConstant::String(value) => Ok(Value::String(PhpString::from_bytes(
            value.as_bytes().to_vec(),
        ))),
        php_ir::IrConstant::StringBytes(value) => {
            Ok(Value::String(PhpString::from_bytes(value.clone())))
        }
        php_ir::IrConstant::Array(entries) => {
            let mut array = php_runtime::api::PhpArray::new();
            for entry in entries {
                let value = ir_constant_value(&entry.value)?;
                if let Some(key) = &entry.key {
                    let key = ir_constant_value(key)?;
                    let key = php_runtime::api::ArrayKey::from_value(&key)
                        .ok_or_else(|| "native constant array key is invalid".to_owned())?;
                    array.insert(key, value);
                } else {
                    array
                        .try_append(value)
                        .map_err(|error| format!("E_PHP_THROW:Error:{error}"))?;
                }
            }
            Ok(Value::Array(array))
        }
        other => Err(format!(
            "native constant {other:?} requires runtime resolution"
        )),
    }
}

fn native_runtime_constant_value(
    context: &NativeExecutionContext<'_>,
    constant: &php_ir::IrConstant,
) -> Result<Value, String> {
    fn resolve(
        context: &NativeExecutionContext<'_>,
        constant: &php_ir::IrConstant,
        depth: usize,
    ) -> Result<Value, String> {
        if depth > 32 {
            return Err("native constant resolution exceeded its recursion limit".to_owned());
        }
        match constant {
            php_ir::IrConstant::NamedConstant(name) => context.lookup_constant(name),
            php_ir::IrConstant::ClassConstant {
                class_name,
                display_class_name: _,
                constant_name,
            } => {
                let normalized = normalize_class_name(class_name);
                if let Some(entry) = context
                    .unit
                    .classes
                    .iter()
                    .find(|class| class.name == normalized)
                    .and_then(|class| {
                        class
                            .constants
                            .iter()
                            .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
                    })
                {
                    if let Some(value) = entry
                        .value
                        .and_then(|id| context.unit.constants.get(id.index()))
                    {
                        return resolve(context, value, depth + 1);
                    }
                    if let Some(reference) = &entry.value_named_constant {
                        for name in &reference.names {
                            if let Ok(value) = context.lookup_constant(name) {
                                return Ok(value);
                            }
                        }
                    }
                }
                if let Some((unit, class)) = native_external_class(context, &normalized)
                    && let Some(entry) = class
                        .constants
                        .iter()
                        .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
                    && let Some(value) = entry.value.and_then(|id| {
                        context
                            .dynamic_units
                            .get(unit)
                            .and_then(|package| package.compiled.unit().constants.get(id.index()))
                    })
                {
                    return resolve(context, value, depth + 1);
                }
                Err(format!("Undefined constant {class_name}::{constant_name}"))
            }
            php_ir::IrConstant::Array(entries) => {
                let mut array = php_runtime::api::PhpArray::new();
                for entry in entries {
                    let value = resolve(context, &entry.value, depth + 1)?;
                    if let Some(key) = &entry.key {
                        let key = resolve(context, key, depth + 1)?;
                        let key = php_runtime::api::ArrayKey::from_value(&key)
                            .ok_or_else(|| "native constant array key is invalid".to_owned())?;
                        array.insert(key, value);
                    } else {
                        array
                            .try_append(value)
                            .map_err(|error| format!("E_PHP_THROW:Error:{error}"))?;
                    }
                }
                Ok(Value::Array(array))
            }
            value => ir_constant_value(value),
        }
    }
    resolve(context, constant, 0)
}

fn native_runtime_type(type_: &php_ir::IrReturnType) -> php_runtime::api::RuntimeType {
    use php_ir::IrReturnType as Ir;
    use php_runtime::api::RuntimeType as Runtime;
    match type_ {
        Ir::Int => Runtime::Int,
        Ir::Float => Runtime::Float,
        Ir::String => Runtime::String,
        Ir::Array => Runtime::Array,
        Ir::Callable => Runtime::Callable,
        Ir::Iterable => Runtime::Iterable,
        Ir::Object => Runtime::Object,
        Ir::Bool => Runtime::Bool,
        Ir::Null => Runtime::Null,
        Ir::Void => Runtime::Void,
        Ir::Mixed => Runtime::Mixed,
        Ir::Never => Runtime::Never,
        Ir::False => Runtime::False,
        Ir::True => Runtime::True,
        Ir::Class { name, display_name } => Runtime::Class {
            name: name.clone(),
            display_name: display_name.clone(),
        },
        Ir::Nullable { inner } => Runtime::Nullable {
            inner: Box::new(native_runtime_type(inner)),
        },
        Ir::Union { members } => Runtime::Union {
            members: members.iter().map(native_runtime_type).collect(),
        },
        Ir::Intersection { members } => Runtime::Intersection {
            members: members.iter().map(native_runtime_type).collect(),
        },
        Ir::Dnf { members } => Runtime::Dnf {
            clauses: members.iter().map(native_runtime_type).collect(),
        },
    }
}

fn native_value_matches_ir_type(value: &Value, type_: &php_ir::IrReturnType) -> bool {
    use php_ir::IrReturnType as Ir;
    let value = match value {
        Value::Reference(reference) => {
            return native_value_matches_ir_type(&reference.get(), type_);
        }
        value => value,
    };
    match type_ {
        Ir::Int => matches!(value, Value::Int(_)),
        Ir::Float => matches!(value, Value::Float(_) | Value::Int(_)),
        Ir::String => matches!(value, Value::String(_)),
        Ir::Array => matches!(value, Value::Array(_)),
        Ir::Callable => matches!(value, Value::Callable(_)),
        Ir::Iterable => matches!(value, Value::Array(_) | Value::Object(_)),
        Ir::Object | Ir::Class { .. } => matches!(value, Value::Object(_)),
        Ir::Bool => matches!(value, Value::Bool(_)),
        Ir::Null | Ir::Void => matches!(value, Value::Null),
        Ir::Mixed => true,
        Ir::Never => false,
        Ir::False => matches!(value, Value::Bool(false)),
        Ir::True => matches!(value, Value::Bool(true)),
        Ir::Nullable { inner } => {
            matches!(value, Value::Null) || native_value_matches_ir_type(value, inner)
        }
        Ir::Union { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type(value, member)),
        Ir::Intersection { members } => members
            .iter()
            .all(|member| native_value_matches_ir_type(value, member)),
        Ir::Dnf { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type(value, member)),
    }
}

fn native_value_matches_ir_type_in_context(
    context: &NativeExecutionContext<'_>,
    value: &Value,
    type_: &php_ir::IrReturnType,
) -> bool {
    use php_ir::IrReturnType as Ir;
    let value = match value {
        Value::Reference(reference) => {
            return native_value_matches_ir_type_in_context(context, &reference.get(), type_);
        }
        value => value,
    };
    match type_ {
        Ir::Class { name, .. } => match value {
            Value::Object(object) => native_class_is_a(context, &object.class_name(), name),
            _ => false,
        },
        Ir::Nullable { inner } => {
            matches!(value, Value::Null)
                || native_value_matches_ir_type_in_context(context, value, inner)
        }
        Ir::Union { members } | Ir::Dnf { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type_in_context(context, value, member)),
        Ir::Intersection { members } => members
            .iter()
            .all(|member| native_value_matches_ir_type_in_context(context, value, member)),
        _ => native_value_matches_ir_type(value, type_),
    }
}

fn native_value_is_callable(context: &NativeExecutionContext<'_>, value: &Value) -> bool {
    match value {
        Value::Reference(reference) => native_value_is_callable(context, &reference.get()),
        Value::Callable(_) => true,
        Value::Object(object) => {
            native_method_in_hierarchy(context, &object.class_name(), "__invoke").is_some()
                || native_external_method(context, &object.class_name(), "__invoke").is_some()
        }
        Value::String(name) => {
            let name = name.to_string_lossy();
            if let Some((class, method)) = name.split_once("::") {
                native_method_in_hierarchy(context, class, method).is_some()
                    || native_external_method(context, class, method).is_some()
            } else {
                context.function_id(&name).is_some()
                    || context.external_function(&name).is_some()
                    || php_extensions::BuiltinRegistry::new().contains(&name.to_ascii_lowercase())
            }
        }
        Value::Array(array) if array.len() == 2 => {
            let target = array.get(&php_runtime::api::ArrayKey::Int(0));
            let method = array.get(&php_runtime::api::ArrayKey::Int(1));
            match (target, method) {
                (Some(Value::Object(object)), Some(Value::String(method))) => {
                    let class = object.class_name();
                    native_method_in_hierarchy(context, &class, &method.to_string_lossy()).is_some()
                        || native_external_method(context, &class, &method.to_string_lossy())
                            .is_some()
                }
                (Some(Value::String(class)), Some(Value::String(method))) => {
                    let class = class.to_string_lossy();
                    native_method_in_hierarchy(context, &class, &method.to_string_lossy()).is_some()
                        || native_external_method(context, &class, &method.to_string_lossy())
                            .is_some()
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn native_ir_type_name(type_: &php_ir::IrReturnType) -> String {
    use php_ir::IrReturnType as Ir;
    match type_ {
        Ir::Int => "int".to_owned(),
        Ir::Float => "float".to_owned(),
        Ir::String => "string".to_owned(),
        Ir::Array => "array".to_owned(),
        Ir::Callable => "callable".to_owned(),
        Ir::Iterable => "iterable".to_owned(),
        Ir::Object => "object".to_owned(),
        Ir::Bool => "bool".to_owned(),
        Ir::Null => "null".to_owned(),
        Ir::Void => "void".to_owned(),
        Ir::Mixed => "mixed".to_owned(),
        Ir::Never => "never".to_owned(),
        Ir::False => "false".to_owned(),
        Ir::True => "true".to_owned(),
        Ir::Class { display_name, name } => display_name.clone().unwrap_or_else(|| name.clone()),
        Ir::Nullable { inner } => format!("?{}", native_ir_type_name(inner)),
        Ir::Union { members } => {
            let mut names = members.iter().map(native_ir_type_name).collect::<Vec<_>>();
            if names.len() == 2
                && names.iter().any(|name| name == "int")
                && names.iter().any(|name| name == "string")
            {
                names = vec!["string".to_owned(), "int".to_owned()];
            }
            names.join("|")
        }
        Ir::Intersection { members } => members
            .iter()
            .map(native_ir_type_name)
            .collect::<Vec<_>>()
            .join("&"),
        Ir::Dnf { members } => members
            .iter()
            .map(native_ir_type_name)
            .collect::<Vec<_>>()
            .join("|"),
    }
}

fn native_runtime_class(
    context: &NativeExecutionContext<'_>,
    class: &php_ir::module::ClassEntry,
) -> Result<php_runtime::api::ClassEntry, String> {
    native_runtime_class_with_owner(context, None, class)
}

fn native_runtime_class_with_owner(
    context: &NativeExecutionContext<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
) -> Result<php_runtime::api::ClassEntry, String> {
    use php_runtime::api as runtime;

    let owner_ir_unit = |owner: Option<usize>| -> Option<&php_ir::IrUnit> {
        match owner {
            None => Some(context.unit),
            Some(unit) => context
                .dynamic_units
                .get(unit)
                .map(|package| package.compiled.unit()),
        }
    };
    let mut lineage = Vec::new();
    let mut current = Some((owner_unit, class.clone()));
    let mut visited = std::collections::BTreeSet::new();
    while let Some((owner, candidate)) = current {
        if !visited.insert(candidate.name.clone()) {
            return Err(format!(
                "native class hierarchy for {} contains a cycle",
                class.display_name
            ));
        }
        let parent = candidate.parent.clone();
        lineage.push((owner, candidate));
        current = parent.as_deref().and_then(|parent| {
            let parent = normalize_class_name(parent);
            owner_ir_unit(owner)
                .into_iter()
                .flat_map(|unit| &unit.classes)
                .find(|class| class.name == parent)
                .cloned()
                .map(|class| (owner, class))
                .or_else(|| {
                    native_external_class(context, &parent).map(|(unit, class)| (Some(unit), class))
                })
        });
    }
    lineage.reverse();
    let properties = lineage
        .iter()
        .flat_map(|(owner, class)| {
            class
                .properties
                .iter()
                .map(move |property| (*owner, property))
        })
        .map(|(owner, property)| {
            let default = property
                .default
                .and_then(|constant| owner_ir_unit(owner)?.constants.get(constant.index()))
                .map(|value| native_runtime_constant_value(context, value))
                .transpose()?
                .unwrap_or_else(|| {
                    if property.flags.is_typed {
                        Value::Uninitialized
                    } else {
                        Value::Null
                    }
                });
            Ok(runtime::ClassPropertyEntry {
                name: property.name.clone(),
                default,
                type_: property.type_.as_ref().map(native_runtime_type),
                flags: runtime::ClassPropertyFlags {
                    is_static: property.flags.is_static,
                    is_private: property.flags.is_private,
                    is_protected: property.flags.is_protected,
                    set_is_private: property.flags.set_is_private,
                    set_is_protected: property.flags.set_is_protected,
                    is_readonly: property.flags.is_readonly,
                    is_typed: property.flags.is_typed,
                },
                hooks: runtime::ClassPropertyHooks {
                    get_function_id: property.hooks.get.map(|function| function.raw()),
                    set_function_id: property.hooks.set.map(|function| function.raw()),
                    backed: property.hooks.backed,
                },
                attributes: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(runtime::ClassEntry {
        name: class.name.clone().into(),
        parent: class.parent.clone(),
        interfaces: class.interfaces.clone(),
        methods: lineage
            .iter()
            .flat_map(|(_, class)| &class.methods)
            .map(|method| runtime::ClassMethodEntry {
                name: method.name.clone(),
                origin_class: method.origin_class.clone(),
                function_id: method.function.raw(),
                flags: runtime::ClassMethodFlags {
                    is_static: method.flags.is_static,
                    is_private: method.flags.is_private,
                    is_protected: method.flags.is_protected,
                    is_abstract: method.flags.is_abstract,
                    is_final: method.flags.is_final,
                },
                attributes: Vec::new(),
            })
            .collect(),
        properties,
        constants: class
            .constants
            .iter()
            .filter_map(|constant| {
                let value = constant
                    .value
                    .and_then(|value| owner_ir_unit(owner_unit)?.constants.get(value.index()))
                    .and_then(|value| native_runtime_constant_value(context, value).ok())?;
                Some(runtime::ClassConstantEntry {
                    name: constant.name.clone(),
                    value,
                    flags: runtime::ClassConstantFlags {
                        is_private: constant.flags.is_private,
                        is_protected: constant.flags.is_protected,
                    },
                    attributes: Vec::new(),
                })
            })
            .collect(),
        enum_cases: class
            .enum_cases
            .iter()
            .map(|case| runtime::ClassEnumCaseEntry {
                name: case.name.clone(),
                value: case
                    .value
                    .and_then(|value| owner_ir_unit(owner_unit)?.constants.get(value.index()))
                    .and_then(|value| ir_constant_value(value).ok()),
                attributes: Vec::new(),
            })
            .collect(),
        attributes: Vec::new(),
        enum_backing_type: class.enum_backing_type.map(|backing| match backing {
            php_ir::module::ClassEnumBackingType::Int => runtime::ClassEnumBackingType::Int,
            php_ir::module::ClassEnumBackingType::String => runtime::ClassEnumBackingType::String,
        }),
        constructor_id: class.constructor.map(|function| function.raw()),
        flags: runtime::ClassFlags {
            is_abstract: class.flags.is_abstract || class.flags.is_trait,
            is_final: class.flags.is_final,
            is_readonly: class.flags.is_readonly,
            is_interface: class.flags.is_interface,
            is_enum: class.flags.is_enum,
        },
    })
}

fn native_prepare_runtime_class_constants(
    context: &mut NativeExecutionContext<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
    source: &php_ir::Instruction,
) -> Result<(), String> {
    fn prepare_constant(
        context: &mut NativeExecutionContext<'_>,
        constant: &php_ir::IrConstant,
        source: &php_ir::Instruction,
    ) -> Result<(), String> {
        match constant {
            php_ir::IrConstant::ClassConstant {
                class_name,
                display_class_name,
                ..
            } => {
                let autoload_name = if display_class_name.is_empty() {
                    class_name
                } else {
                    display_class_name
                };
                native_autoload_class(context, autoload_name, source)
            }
            php_ir::IrConstant::Array(entries) => {
                for entry in entries {
                    if let Some(key) = &entry.key {
                        prepare_constant(context, key, source)?;
                    }
                    prepare_constant(context, &entry.value, source)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    let constants = match owner_unit {
        None => &context.unit.constants,
        Some(unit) => {
            &context
                .dynamic_units
                .get(unit)
                .ok_or_else(|| format!("dynamic native unit {unit} is missing"))?
                .compiled
                .unit()
                .constants
        }
    };
    let defaults = class
        .properties
        .iter()
        .filter_map(|property| {
            property
                .default
                .and_then(|constant| constants.get(constant.index()))
                .cloned()
        })
        .collect::<Vec<_>>();
    for constant in &defaults {
        prepare_constant(context, constant, source)?;
    }
    Ok(())
}

fn encode_native_enum_case(
    context: &mut NativeExecutionContext<'_>,
    class: &php_ir::module::ClassEntry,
    case: &php_ir::module::ClassEnumCaseEntry,
) -> Result<i64, String> {
    let key = (class.name.clone(), case.name.clone());
    if let Some(object) = context.enum_cases.get(&key).cloned() {
        return context.encode(Value::Object(object));
    }
    let runtime_class = native_runtime_class(context, class)?;
    let object = php_runtime::api::ObjectRef::new_with_display_name(
        &runtime_class,
        class.display_name.clone(),
    );
    object.set_property(
        "name",
        Value::String(PhpString::from_bytes(case.name.as_bytes().to_vec())),
    );
    if let Some(value) = case
        .value
        .and_then(|value| context.unit.constants.get(value.index()))
        .and_then(|value| ir_constant_value(value).ok())
    {
        object.set_property("value", value);
    }
    context.enum_cases.insert(key, object.clone());
    context.encode(Value::Object(object))
}

fn native_static_property_declaration(
    context: &NativeExecutionContext<'_>,
    class_name: &str,
    property: &str,
) -> Option<(
    Option<usize>,
    php_ir::module::ClassEntry,
    php_ir::module::ClassPropertyEntry,
)> {
    let mut candidate = normalize_class_name(class_name);
    let mut visited = std::collections::BTreeSet::new();
    while visited.insert(candidate.clone()) {
        let (unit, class) = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)
            .cloned()
            .map(|class| (None, class))
            .or_else(|| {
                native_external_class(context, &candidate).map(|(unit, class)| (Some(unit), class))
            })?;
        if let Some(entry) = class
            .properties
            .iter()
            .find(|entry| entry.flags.is_static && entry.name == property)
            .cloned()
        {
            return Some((unit, class, entry));
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
    None
}

fn native_nested_array_reference(
    value: &mut Value,
    keys: &[php_runtime::api::ArrayKey],
) -> Result<php_runtime::api::ReferenceCell, String> {
    if keys.is_empty() {
        return Ok(match value {
            Value::Reference(reference) => reference.clone(),
            value => {
                let reference = php_runtime::api::ReferenceCell::new(value.clone());
                *value = Value::Reference(reference.clone());
                reference
            }
        });
    }

    if let Value::Reference(reference) = value {
        let mut referenced = reference.get();
        let result = native_nested_array_reference(&mut referenced, keys)?;
        reference.set(referenced);
        return Ok(result);
    }

    if matches!(value, Value::Null | Value::Uninitialized) {
        *value = Value::Array(php_runtime::api::PhpArray::new());
    }
    let Value::Array(array) = value else {
        return Err(format!(
            "Cannot use a value of type {} as an array",
            native_value_type_name(value)
        ));
    };

    let key = keys[0].clone();
    let mut element = array.get(&key).cloned().unwrap_or(Value::Null);
    let reference = native_nested_array_reference(&mut element, &keys[1..])?;
    array.insert(key, element);
    Ok(reference)
}

fn dereference_native_assignment_value(mut value: Value) -> Value {
    for _ in 0..16 {
        let Value::Reference(reference) = value else {
            break;
        };
        value = reference.get();
    }
    value
}

fn execute_native_static_property(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
) -> Option<Result<i64, String>> {
    if let php_ir::InstructionKind::BindReferenceFromStaticPropertyDim {
        class_name,
        property,
        dims,
        ..
    } = &instruction.kind
    {
        let keys = match arguments
            .iter()
            .map(|argument| {
                context.decode(*argument).and_then(|value| {
                    php_runtime::api::ArrayKey::from_value(&value)
                        .ok_or_else(|| "Illegal offset type".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(keys) if keys.len() == dims.len() => keys,
            Ok(_) => {
                return Some(Err(
                    "static property dimension operands are missing".to_owned()
                ));
            }
            Err(error) => return Some(Err(error)),
        };
        let calling_class = native_calling_class(context, caller_function);
        let resolved_class = match class_name.to_ascii_lowercase().as_str() {
            "self" => calling_class.map_or_else(|| class_name.clone(), |class| class.name.clone()),
            "parent" => calling_class
                .and_then(|class| class.parent.clone())
                .unwrap_or_else(|| class_name.clone()),
            "static" => context
                .called_classes
                .last()
                .cloned()
                .or_else(|| calling_class.map(|class| class.name.clone()))
                .unwrap_or_else(|| class_name.clone()),
            _ => class_name.clone(),
        };
        let Some((owner_unit, owner, entry)) =
            native_static_property_declaration(context, &resolved_class, property)
        else {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Access to undeclared static property {resolved_class}::${property}"
            )));
        };
        let key = (owner.name.clone(), property.clone());
        let current = context.static_properties.get(&key).cloned().or_else(|| {
            entry
                .default
                .and_then(|constant| {
                    if owner_unit.is_none() {
                        context.unit.constants.get(constant.index())
                    } else {
                        owner_unit.and_then(|unit| {
                            context.dynamic_units.get(unit).and_then(|package| {
                                package.compiled.unit().constants.get(constant.index())
                            })
                        })
                    }
                })
                .and_then(|constant| ir_constant_value(constant).ok())
        });
        let reference = match current.unwrap_or(Value::Null) {
            Value::Reference(reference) => reference,
            value => php_runtime::api::ReferenceCell::new(value),
        };
        context
            .static_properties
            .insert(key, Value::Reference(reference.clone()));
        if keys.is_empty() {
            return Some(context.encode(Value::Reference(reference)));
        }
        let mut root = Value::Reference(reference);
        let reference = match native_nested_array_reference(&mut root, &keys) {
            Ok(reference) => reference,
            Err(error) => return Some(Err(error)),
        };
        return Some(context.encode(Value::Reference(reference)));
    }
    let (class_name, property, assigned, bind_reference) = match &instruction.kind {
        php_ir::InstructionKind::FetchStaticProperty {
            class_name,
            property,
            ..
        } => (class_name.clone(), property.clone(), None, false),
        php_ir::InstructionKind::AssignStaticProperty {
            class_name,
            property,
            ..
        } => {
            let Some(value) = arguments.first() else {
                return Some(Err("static property assignment value is missing".to_owned()));
            };
            (class_name.clone(), property.clone(), Some(*value), false)
        }
        php_ir::InstructionKind::AssignDynamicStaticProperty { property, .. } => {
            let [class_name, value] = arguments else {
                return Some(Err(
                    "dynamic static property assignment operands are missing".to_owned(),
                ));
            };
            let class_name = match context.decode(*class_name) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let class_name = match class_name {
                Value::String(class_name) => class_name.to_string_lossy(),
                Value::Object(object) => object.class_name(),
                value => {
                    return Some(Err(format!(
                        "class name must be a valid object or a string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            (class_name, property.clone(), Some(*value), false)
        }
        php_ir::InstructionKind::FetchDynamicStaticProperty { property, .. } => {
            let Some(class_name) = arguments.first() else {
                return Some(Err(
                    "dynamic static property class operand is missing".to_owned()
                ));
            };
            let class_name = match context.decode(*class_name) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let class_name = match class_name {
                Value::String(class_name) => class_name.to_string_lossy(),
                Value::Object(object) => object.class_name(),
                value => {
                    return Some(Err(format!(
                        "class name must be a valid object or a string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            (class_name, property.clone(), None, false)
        }
        php_ir::InstructionKind::BindReferenceStaticProperty {
            class_name,
            property,
            ..
        } => {
            let Some(value) = arguments.first() else {
                return Some(Err("static property reference source is missing".to_owned()));
            };
            (class_name.clone(), property.clone(), Some(*value), true)
        }
        php_ir::InstructionKind::IssetStaticProperty {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::EmptyStaticProperty {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::IssetStaticPropertyDim {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::EmptyStaticPropertyDim {
            class_name,
            property,
            ..
        }
        | php_ir::InstructionKind::UnsetStaticPropertyDim {
            class_name,
            property,
            ..
        } => (class_name.clone(), property.clone(), None, false),
        _ => return None,
    };
    let calling_class = native_calling_class(context, caller_function);
    let resolved_class = match class_name.to_ascii_lowercase().as_str() {
        "self" => calling_class.map_or_else(|| class_name.clone(), |class| class.name.clone()),
        "parent" => calling_class
            .and_then(|class| class.parent.clone())
            .unwrap_or_else(|| class_name.clone()),
        "static" => context
            .called_classes
            .last()
            .cloned()
            .or_else(|| calling_class.map(|class| class.name.clone()))
            .unwrap_or_else(|| class_name.clone()),
        _ => class_name.clone(),
    };
    let normalized = normalize_class_name(&resolved_class);
    let requested_local_class = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalized)
        .cloned();
    if requested_local_class.is_none()
        && native_external_class(context, &resolved_class).is_none()
        && context.autoload_in_progress.insert(normalized.clone())
    {
        let callbacks = context.autoload_callbacks.clone();
        for callback in callbacks {
            if let Err(error) = invoke_native_callable_value(
                context,
                callback,
                &[Value::String(PhpString::from_bytes(
                    resolved_class.as_bytes().to_vec(),
                ))],
                instruction,
                None,
            ) {
                context.autoload_in_progress.remove(&normalized);
                return Some(Err(error));
            }
            if native_external_class(context, &resolved_class).is_some() {
                break;
            }
        }
        context.autoload_in_progress.remove(&normalized);
    }
    let requested_external_class = native_external_class(context, &resolved_class);
    let requested_class = requested_local_class.or_else(|| {
        requested_external_class
            .as_ref()
            .map(|(_, class)| class.clone())
    });
    let requested_display_name = requested_class.as_ref().map_or_else(
        || resolved_class.clone(),
        |class| class.display_name.clone(),
    );
    let Some((owner_unit, class, entry)) =
        native_static_property_declaration(context, &resolved_class, &property)
    else {
        if matches!(
            instruction.kind,
            php_ir::InstructionKind::IssetStaticProperty { .. }
                | php_ir::InstructionKind::IssetStaticPropertyDim { .. }
        ) {
            return Some(context.encode(Value::Bool(false)));
        }
        if matches!(
            instruction.kind,
            php_ir::InstructionKind::EmptyStaticProperty { .. }
                | php_ir::InstructionKind::EmptyStaticPropertyDim { .. }
        ) {
            return Some(context.encode(Value::Bool(true)));
        }
        return Some(Err(format!(
            "E_PHP_THROW:Error:Access to undeclared static property {requested_display_name}::${property}"
        )));
    };
    let display_name = class.display_name.clone();
    let caller_owns_scope = class
        .methods
        .iter()
        .any(|method| method.function.raw() == caller_function);
    if (entry.flags.is_private || entry.flags.is_protected) && !caller_owns_scope {
        return Some(Err(format!(
            "E_PHP_THROW:Error:Cannot access {} property {}::${property}",
            if entry.flags.is_private {
                "private"
            } else {
                "protected"
            },
            display_name
        )));
    }
    let key = (class.name.clone(), property.clone());
    let result = if bind_reference {
        let Some(source) = assigned else {
            return Some(Err("static property reference source is missing".to_owned()));
        };
        let value = match context.decode(source) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        let reference = match value {
            Value::Reference(reference) => reference,
            value => php_runtime::api::ReferenceCell::new(value),
        };
        let effective = reference.get();
        if let Some(type_) = &entry.type_
            && !native_value_matches_ir_type_in_context(context, &effective, type_)
        {
            return Some(Err(format!(
                "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                native_assignment_type_name(&effective),
                display_name,
                property,
                native_ir_type_name(type_)
            )));
        }
        let previous = context
            .static_properties
            .insert(key.clone(), Value::Reference(reference.clone()));
        if let Some(previous) = previous.map(dereference_native_assignment_value)
            && let Value::Object(previous) = previous
            && let Err(error) = context.run_object_destructor(previous)
        {
            return Some(Err(error));
        }
        Value::Reference(reference)
    } else if let Some(assigned) = assigned {
        let mut value = match context.decode(assigned) {
            Ok(value) => dereference_native_assignment_value(value),
            Err(error) => return Some(Err(error)),
        };
        if owner_unit.is_some() {
            // Closure function ids are unit-local. Preserve the assigning
            // unit when a closure crosses into a class owned by another unit.
            value = native_value_with_owner_unit(value, context.current_dynamic_unit);
        }
        if let Some(type_) = &entry.type_
            && !native_value_matches_ir_type_in_context(context, &value, type_)
        {
            return Some(Err(format!(
                "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                native_assignment_type_name(&value),
                display_name,
                property,
                native_ir_type_name(type_)
            )));
        }
        let existing_reference = context.static_properties.get(&key).and_then(|current| {
            let Value::Reference(reference) = current else {
                return None;
            };
            Some(reference.clone())
        });
        let previous = if let Some(reference) = existing_reference {
            let previous = reference.get();
            reference.set(value.clone());
            Some(previous)
        } else {
            context.static_properties.insert(key.clone(), value.clone())
        };
        if let Some(Value::Object(previous)) = previous
            && let Err(error) = context.run_object_destructor(previous)
        {
            return Some(Err(error));
        }
        value
    } else if let Some(value) = context.static_properties.get(&key).cloned() {
        value
    } else {
        let value = entry.default.and_then(|constant| {
            if owner_unit.is_none() {
                context.unit.constants.get(constant.index())
            } else {
                owner_unit.and_then(|unit| {
                    context
                        .dynamic_units
                        .get(unit)
                        .and_then(|package| package.compiled.unit().constants.get(constant.index()))
                })
            }
        });
        let value = value.map_or(Ok(Value::Null), |value| {
            native_runtime_constant_value(context, value)
        });
        match value {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        }
    };
    let result = match &instruction.kind {
        php_ir::InstructionKind::IssetStaticProperty { .. } => {
            Value::Bool(!matches!(result, Value::Null | Value::Uninitialized))
        }
        php_ir::InstructionKind::EmptyStaticProperty { .. } => {
            Value::Bool(!native_property_truthy(&result))
        }
        php_ir::InstructionKind::IssetStaticPropertyDim { dims, .. } => {
            let value = native_static_property_dim_value(context, result, arguments, dims.len());
            Value::Bool(
                value.is_some_and(|value| !matches!(value, Value::Null | Value::Uninitialized)),
            )
        }
        php_ir::InstructionKind::EmptyStaticPropertyDim { dims, .. } => {
            let value = native_static_property_dim_value(context, result, arguments, dims.len());
            Value::Bool(value.is_none_or(|value| !native_property_truthy(&value)))
        }
        php_ir::InstructionKind::UnsetStaticPropertyDim { dims, .. } => {
            let keys = arguments
                .iter()
                .take(dims.len())
                .map(|encoded| {
                    context
                        .decode(*encoded)
                        .ok()
                        .and_then(|value| php_runtime::api::ArrayKey::from_value(&value))
                })
                .collect::<Option<Vec<_>>>();
            if let Some(keys) = keys {
                match result {
                    Value::Reference(reference) => {
                        let mut value = reference.get();
                        unset_native_array_dims(&mut value, &keys);
                        reference.set(value);
                    }
                    mut value => {
                        unset_native_array_dims(&mut value, &keys);
                        context.static_properties.insert(key.clone(), value);
                    }
                }
            }
            Value::Null
        }
        php_ir::InstructionKind::FetchStaticProperty { .. }
        | php_ir::InstructionKind::FetchDynamicStaticProperty { .. }
        | php_ir::InstructionKind::AssignStaticProperty { .. }
        | php_ir::InstructionKind::AssignDynamicStaticProperty { .. } => {
            dereference_native_assignment_value(result)
        }
        php_ir::InstructionKind::BindReferenceStaticProperty { .. } => result,
        _ => result,
    };
    Some(context.encode(result))
}

fn native_static_property_dim_value(
    context: &NativeExecutionContext<'_>,
    mut value: Value,
    arguments: &[i64],
    dimension_count: usize,
) -> Option<Value> {
    if arguments.len() != dimension_count {
        return None;
    }
    for encoded in arguments {
        while let Value::Reference(reference) = value {
            value = reference.get();
        }
        let key = context.decode(*encoded).ok()?;
        let key = php_runtime::api::ArrayKey::from_value(&key)?;
        value = match value {
            Value::Array(array) => array.get(&key).cloned()?,
            _ => return None,
        };
    }
    while let Value::Reference(reference) = value {
        value = reference.get();
    }
    Some(value)
}

fn native_property_truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Uninitialized | Value::Bool(false) => false,
        Value::Int(0) => false,
        Value::Float(value) if value.to_f64() == 0.0 => false,
        Value::String(value) if value.as_bytes().is_empty() || value.as_bytes() == b"0" => false,
        Value::Array(value) if value.is_empty() => false,
        Value::Reference(reference) => native_property_truthy(&reference.get()),
        Value::Object(object) if native_simple_xml_empty(object).is_some() => {
            !native_simple_xml_empty(object).unwrap_or(true)
        }
        _ => true,
    }
}

fn native_property_is_set(value: &Value) -> bool {
    match value {
        Value::Null | Value::Uninitialized => false,
        Value::Reference(reference) => native_property_is_set(&reference.get()),
        _ => true,
    }
}

fn unset_native_array_dims(value: &mut Value, keys: &[php_runtime::api::ArrayKey]) {
    if let Value::Reference(reference) = value {
        let mut target = reference.get();
        unset_native_array_dims(&mut target, keys);
        reference.set(target);
        return;
    }
    let Some((key, rest)) = keys.split_first() else {
        return;
    };
    let Value::Array(array) = value else {
        return;
    };
    if rest.is_empty() {
        array.remove(key);
    } else if let Some(mut nested) = array.get_mut(key) {
        unset_native_array_dims(&mut nested, rest);
    }
}

fn assign_native_array_dims(
    value: &mut Value,
    keys: &[php_runtime::api::ArrayKey],
    replacement: Value,
    append: bool,
) {
    if let Value::Reference(reference) = value {
        let mut target = reference.get();
        assign_native_array_dims(&mut target, keys, replacement, append);
        reference.set(target);
        return;
    }
    if !matches!(value, Value::Array(_)) {
        *value = Value::Array(php_runtime::api::PhpArray::new());
    }
    let Value::Array(array) = value else {
        unreachable!("array value was initialized above")
    };
    let Some((key, rest)) = keys.split_first() else {
        if append {
            array.append(replacement);
        }
        return;
    };
    if rest.is_empty() && !append {
        if let Some(Value::Reference(reference)) = array.get(key).cloned() {
            reference.set(replacement);
        } else {
            array.insert(key.clone(), replacement);
        }
    } else {
        let mut nested = array.get(key).cloned().unwrap_or(Value::Null);
        assign_native_array_dims(&mut nested, rest, replacement, append);
        array.insert(key.clone(), nested);
    }
}

fn native_external_method(
    context: &NativeExecutionContext<'_>,
    class_name: &str,
    method: &str,
) -> Option<(NativeDynamicFunction, php_ir::module::ClassMethodEntry)> {
    let (mut unit, mut class) = native_external_class(context, class_name).or_else(|| {
        let local = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(class_name))?;
        native_external_class(context, local.parent.as_deref()?)
    })?;
    loop {
        if let Some(entry) = class
            .methods
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(method))
            .cloned()
        {
            return Some((
                NativeDynamicFunction {
                    unit,
                    function: entry.function,
                },
                entry,
            ));
        }
        let parent = class.parent.as_deref()?;
        let normalized_parent = normalize_class_name(parent);
        let (parent_unit, parent_class) = context
            .current_dynamic_unit
            .and_then(|unit| {
                context
                    .unit
                    .classes
                    .iter()
                    .find(|class| class.name == normalized_parent)
                    .cloned()
                    .map(|class| (unit, class))
            })
            .or_else(|| native_external_class(context, parent))?;
        unit = parent_unit;
        class = parent_class;
    }
}

fn create_native_external_object(
    context: &mut NativeExecutionContext<'_>,
    class_name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let (unit, class) = native_external_class(context, class_name)
        .ok_or_else(|| format!("E_PHP_VM_UNKNOWN_CLASS: Class {class_name} not found"))?;
    if class.flags.is_abstract
        || class.flags.is_interface
        || class.flags.is_trait
        || class.flags.is_enum
    {
        return Err(format!(
            "Cannot instantiate {} {}",
            class_name, class.display_name
        ));
    }
    native_prepare_runtime_class_constants(context, Some(unit), &class, source)?;
    let runtime_class = native_runtime_class_with_owner(context, Some(unit), &class)?;
    let object = php_runtime::api::ObjectRef::new_with_display_name(
        &runtime_class,
        class.display_name.clone(),
    );
    let receiver = context.encode(Value::Object(object))?;
    if let Some((constructor, _)) = native_external_method(context, class_name, "__construct") {
        let mut constructor_arguments = Vec::with_capacity(arguments.len() + 1);
        constructor_arguments.push(receiver);
        constructor_arguments.extend_from_slice(arguments);
        let _ = invoke_native_external_function(
            context,
            constructor,
            &constructor_arguments,
            Some(class.name),
            context.unit.strict_types,
        )?;
    }
    Ok(receiver)
}

fn native_coerce_call_argument(value: Value, type_: &php_ir::IrReturnType, strict: bool) -> Value {
    use php_ir::IrReturnType as Type;
    if let Value::Reference(reference) = &value {
        return Value::Reference(reference.clone());
    }
    if matches!(type_, Type::Float)
        && let Value::Int(value) = value
    {
        return Value::Float(php_runtime::api::FloatValue::from_f64(value as f64));
    }
    if strict || native_value_matches_ir_type(&value, type_) {
        return value;
    }
    match (type_, value) {
        (Type::Int, Value::String(value)) => value
            .to_string_lossy()
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or(Value::String(value)),
        (Type::Int, Value::Float(value)) => Value::Int(value.to_f64() as i64),
        (Type::Int, Value::Bool(value)) => Value::Int(i64::from(value)),
        (Type::Float, Value::String(value)) => value
            .to_string_lossy()
            .trim()
            .parse::<f64>()
            .map(|value| Value::Float(php_runtime::api::FloatValue::from_f64(value)))
            .unwrap_or(Value::String(value)),
        (Type::Float, Value::Bool(value)) => {
            Value::Float(php_runtime::api::FloatValue::from_f64(if value {
                1.0
            } else {
                0.0
            }))
        }
        (Type::String, Value::Int(value)) => {
            Value::String(PhpString::from_bytes(value.to_string().into_bytes()))
        }
        (Type::String, Value::Float(value)) => Value::String(PhpString::from_bytes(
            value.to_f64().to_string().into_bytes(),
        )),
        (Type::String, Value::Bool(value)) => Value::String(PhpString::from_bytes(if value {
            b"1".to_vec()
        } else {
            Vec::new()
        })),
        (Type::Bool, value @ (Value::Int(_) | Value::Float(_) | Value::String(_))) => {
            Value::Bool(native_property_truthy(&value))
        }
        (Type::Nullable { inner }, value) => native_coerce_call_argument(value, inner, strict),
        (Type::Union { members }, value) => members
            .iter()
            .map(|member| native_coerce_call_argument(value.clone(), member, strict))
            .find(|candidate| native_value_matches_ir_type(candidate, type_))
            .unwrap_or(value),
        (_, value) => value,
    }
}

fn native_function_has_implicit_closure_this(function: &php_ir::IrFunction) -> bool {
    function.flags.is_closure
        && !function.flags.is_static
        && function.locals.first().is_some_and(|name| name == "this")
        && !function
            .captures
            .iter()
            .any(|capture| capture.local == php_ir::LocalId::new(0))
}

fn native_backtrace_frame(
    compiled: &crate::compiled_unit::CompiledUnit,
    function: php_ir::FunctionId,
    called_class: Option<&str>,
    object: Option<php_runtime::api::ObjectRef>,
    arguments: Vec<Value>,
) -> NativeBacktraceFrame {
    let unit = compiled.unit();
    let target = unit.functions.get(function.index());
    let method = unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == function)
            .map(|method| (class, method))
    });
    let (class, call_type) = method.map_or((None, None), |(class, method)| {
        (
            Some(called_class.map_or_else(|| class.display_name.clone(), str::to_owned)),
            Some(if method.flags.is_static { "::" } else { "->" }),
        )
    });
    let function = target.map_or_else(
        || "{unknown}".to_owned(),
        |target| {
            target
                .name
                .rsplit_once("::")
                .map_or_else(|| target.name.clone(), |(_, method)| method.to_owned())
        },
    );
    let span = target.map(|target| target.span);
    let file = span.and_then(|span| {
        unit.files
            .get(span.file.index())
            .map(|file| file.path.clone())
    });
    let line = span
        .and_then(|span| compiled.source_display_line(span, false))
        .unwrap_or(0);
    NativeBacktraceFrame {
        function,
        class,
        call_type,
        file,
        line,
        object,
        arguments,
    }
}

fn seed_native_child_state(
    context: &mut NativeExecutionContext<'_>,
    child: &mut NativeExecutionContext<'_>,
) {
    child.runtime_telemetry = context.runtime_telemetry.clone();
    child.visible_function_names = NativeFunctionNameScope::child(
        context.visible_function_names.clone(),
        child
            .unit
            .function_table
            .iter()
            .map(|entry| entry.name.to_ascii_lowercase()),
    );
    child.builtin_request_state = std::mem::replace(
        &mut context.builtin_request_state,
        php_runtime::api::BuiltinRequestState::new(),
    );
    child.registered_extensions = std::mem::take(&mut context.registered_extensions);
    child.http_response = std::mem::take(&mut context.http_response);
    child.upload_registry = std::mem::take(&mut context.upload_registry);
    child.session = std::mem::take(&mut context.session);
    child.session_global = context.session_global.clone();
    child
        .filter_input_arrays
        .clone_from(&context.filter_input_arrays);
    child.dynamic_units = std::mem::take(&mut context.dynamic_units);
    child.external_functions = std::mem::take(&mut context.external_functions);
    child.dynamic_classes = std::mem::take(&mut context.dynamic_classes);
    child.dynamic_constants = std::mem::take(&mut context.dynamic_constants);
    child.static_properties = std::mem::take(&mut context.static_properties);
    child.static_locals = std::mem::take(&mut context.static_locals);
    child.enum_cases = std::mem::take(&mut context.enum_cases);
    child.class_aliases = std::mem::take(&mut context.class_aliases);
    child.autoload_in_progress = std::mem::take(&mut context.autoload_in_progress);
    child.autoload_callbacks = std::mem::take(&mut context.autoload_callbacks)
        .into_iter()
        .map(|callback| native_value_with_owner_unit(callback, context.current_dynamic_unit))
        .collect();
    child.shutdown_callbacks = std::mem::take(&mut context.shutdown_callbacks)
        .into_iter()
        .map(|mut callback| {
            callback.callable =
                native_value_with_owner_unit(callback.callable, context.current_dynamic_unit);
            callback
        })
        .collect();
    child.destroyed_objects = std::mem::take(&mut context.destroyed_objects);
    child.included_files = std::mem::take(&mut context.included_files);
    child.include_path = std::mem::take(&mut context.include_path);
    child.cwd = std::mem::take(&mut context.cwd);
    child.inherited_globals = std::mem::take(&mut context.inherited_globals);
    child.error_reporting = context.error_reporting;
    child.last_error = context.last_error.take();
    child.error_handlers = std::mem::take(&mut context.error_handlers);
    child.exception_handlers = std::mem::take(&mut context.exception_handlers);
    child.environment = std::mem::take(&mut context.environment);
    child.ini_registry = std::mem::take(&mut context.ini_registry);
    child.mysql_state.clone_from(&context.mysql_state);
    child.lexical_scope_classes = std::mem::take(&mut context.lexical_scope_classes);
    child.call_frames = std::mem::take(&mut context.call_frames);
}

fn merge_native_child_state(
    context: &mut NativeExecutionContext<'_>,
    child: &mut NativeExecutionContext<'_>,
) {
    context.visible_function_names = child.visible_function_names.clone();
    context.builtin_request_state = std::mem::replace(
        &mut child.builtin_request_state,
        php_runtime::api::BuiltinRequestState::new(),
    );
    context.registered_extensions = std::mem::take(&mut child.registered_extensions);
    context.http_response = std::mem::take(&mut child.http_response);
    context.upload_registry = std::mem::take(&mut child.upload_registry);
    context.session = std::mem::take(&mut child.session);
    context.session_global = child.session_global.clone();
    context
        .filter_input_arrays
        .clone_from(&child.filter_input_arrays);
    context.dynamic_units = std::mem::take(&mut child.dynamic_units);
    context.external_functions = std::mem::take(&mut child.external_functions);
    context.dynamic_classes = std::mem::take(&mut child.dynamic_classes);
    context.dynamic_constants = std::mem::take(&mut child.dynamic_constants);
    context.static_properties = std::mem::take(&mut child.static_properties);
    context.static_locals = std::mem::take(&mut child.static_locals);
    context.enum_cases = std::mem::take(&mut child.enum_cases);
    context.class_aliases = std::mem::take(&mut child.class_aliases);
    context.autoload_in_progress = std::mem::take(&mut child.autoload_in_progress);
    context.autoload_callbacks = std::mem::take(&mut child.autoload_callbacks);
    context.shutdown_callbacks = std::mem::take(&mut child.shutdown_callbacks);
    context.destroyed_objects = std::mem::take(&mut child.destroyed_objects);
    context.included_files = std::mem::take(&mut child.included_files);
    context.include_path = std::mem::take(&mut child.include_path);
    context.cwd = std::mem::take(&mut child.cwd);
    context.inherited_globals = std::mem::take(&mut child.inherited_globals);
    context.error_reporting = child.error_reporting;
    context.last_error = child.last_error.take();
    context.error_handlers = std::mem::take(&mut child.error_handlers);
    context.exception_handlers = std::mem::take(&mut child.exception_handlers);
    context.environment = std::mem::take(&mut child.environment);
    context.ini_registry = std::mem::take(&mut child.ini_registry);
    context.mysql_state.clone_from(&child.mysql_state);
    context.lexical_scope_classes = std::mem::take(&mut child.lexical_scope_classes);
    context.call_frames = std::mem::take(&mut child.call_frames);
}

fn invoke_native_external_function(
    context: &mut NativeExecutionContext<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    called_class: Option<String>,
    strict: bool,
) -> Result<i64, String> {
    invoke_native_external_function_with_metadata(
        context,
        target,
        arguments,
        None,
        called_class,
        strict,
    )
}

fn invoke_native_external_function_with_metadata(
    context: &mut NativeExecutionContext<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    called_class: Option<String>,
    strict: bool,
) -> Result<i64, String> {
    let handle = ensure_dynamic_native_entry(context, target.unit, target.function)?;
    let package = context
        .dynamic_units
        .get(target.unit)
        .cloned()
        .ok_or_else(|| "dynamic native unit is missing".to_owned())?;
    let function = package
        .compiled
        .unit()
        .functions
        .get(target.function.index())
        .ok_or_else(|| "dynamic native function metadata is missing".to_owned())?;
    let values = arguments
        .iter()
        .enumerate()
        .map(|(index, argument)| {
            context.decode(*argument).map_err(|error| {
                format!(
                    "{}() native argument {} could not be decoded: {error}",
                    function.name, index
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let _depth_guard = enter_native_call(&function.name)?;
    let instance_method = package.compiled.unit().classes.iter().any(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function == target.function && !method.flags.is_static)
    });
    let leading = function.captures.len()
        + usize::from(instance_method)
        + usize::from(native_function_has_implicit_closure_this(function));
    if values.len() < leading {
        return Err(format!(
            "{}() is missing its native receiver/capture arguments",
            function.name
        ));
    }
    let mut bound = values[..leading].to_vec();
    let raw_supplied = &values[leading..];
    let mut supplied = Vec::<(Option<String>, Value)>::new();
    if let Some(metadata) = metadata {
        if metadata.len() != raw_supplied.len() {
            return Err(format!(
                "{}() native argument metadata mismatch: expected {}, received {}",
                function.name,
                metadata.len(),
                raw_supplied.len()
            ));
        }
        for (argument, value) in metadata.iter().zip(raw_supplied) {
            if argument.unpack {
                let unpacked = match value {
                    Value::Reference(reference) => reference.get(),
                    value => value.clone(),
                };
                let Value::Array(array) = unpacked else {
                    return Err("Only arrays and Traversables can be unpacked".to_owned());
                };
                supplied.extend(array.iter().map(|(key, value)| {
                    let name = match key {
                        php_runtime::api::ArrayKey::Int(_) => None,
                        php_runtime::api::ArrayKey::String(name) => Some(name.to_string_lossy()),
                    };
                    (name, value.clone())
                }));
            } else {
                supplied.push((argument.name.clone(), value.clone()));
            }
        }
    } else {
        supplied.extend(raw_supplied.iter().cloned().map(|value| (None, value)));
    }
    let variadic_index = function
        .params
        .iter()
        .position(|parameter| parameter.variadic);
    let fixed_count = variadic_index.unwrap_or(function.params.len());
    let mut assigned = vec![None; fixed_count];
    let mut variadic = php_runtime::api::PhpArray::new();
    let mut positional = 0usize;
    let mut saw_named = false;
    for (name, value) in &supplied {
        if let Some(name) = name {
            saw_named = true;
            if let Some(index) = function.params[..fixed_count]
                .iter()
                .position(|parameter| parameter.name.eq_ignore_ascii_case(name))
            {
                if assigned[index].replace(value.clone()).is_some() {
                    return Err(format!(
                        "Named parameter ${name} overwrites previous argument"
                    ));
                }
            } else if variadic_index.is_some() {
                variadic.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        name.as_bytes().to_vec(),
                    )),
                    value.clone(),
                );
            } else {
                return Err(format!("Unknown named parameter ${name}"));
            }
        } else {
            if saw_named {
                return Err("Cannot use positional argument after named argument".to_owned());
            }
            while positional < fixed_count && assigned[positional].is_some() {
                positional += 1;
            }
            if positional < fixed_count {
                assigned[positional] = Some(value.clone());
                positional += 1;
            } else if variadic_index.is_some() {
                variadic.append(value.clone());
            }
        }
    }

    for (index, parameter) in function.params.iter().enumerate() {
        if parameter.variadic {
            let mut values = php_runtime::api::PhpArray::new();
            for (key, value) in variadic.iter() {
                let value = parameter.type_.as_ref().map_or_else(
                    || value.clone(),
                    |type_| native_coerce_call_argument(value.clone(), type_, strict),
                );
                values.insert(key.clone(), value);
            }
            bound.push(Value::Array(values));
            continue;
        }
        let supplied_value = assigned[index].clone();
        let mut value = if let Some(value) = supplied_value.clone() {
            value
        } else if let Some(default) = &parameter.default {
            native_runtime_constant_value(context, default)?
        } else {
            return Err(format!("Too few arguments to function {}()", function.name));
        };
        if parameter.by_ref {
            if supplied_value.is_none() {
                value = Value::Reference(php_runtime::api::ReferenceCell::new(value));
            }
            let Value::Reference(reference) = &value else {
                return Err(format!(
                    "E_PHP_THROW:Error:{}(): Argument #{} (${}) could not be passed by reference",
                    function.name,
                    index + 1,
                    parameter.name
                ));
            };
            if matches!(reference.get(), Value::Uninitialized) {
                reference.set(Value::Null);
            }
        } else if let Value::Reference(reference) = value {
            value = reference.get();
        }
        if let Some(type_) = &parameter.type_ {
            let checked = match &value {
                Value::Reference(reference) => reference.get(),
                value => value.clone(),
            };
            let checked = native_coerce_call_argument(checked, type_, strict);
            if !(native_value_matches_ir_type_in_context(context, &checked, type_)
                || matches!(type_, php_ir::IrReturnType::Callable)
                    && native_value_is_callable(context, &checked))
            {
                return Err(format!(
                    "E_PHP_THROW:TypeError:{}(): Argument #{} (${}) must be of type {}, {} given",
                    function.name,
                    index + 1,
                    parameter.name,
                    native_ir_type_name(type_),
                    native_value_type_name(&checked)
                ));
            }
            if let Value::Reference(reference) = &value {
                reference.set(checked);
            } else {
                value = checked;
            }
        }
        bound.push(value);
    }
    let visible_arguments = supplied
        .iter()
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let mut child = NativeExecutionContext::new(
        &package.compiled,
        package.compiled.cache_identity(),
        context.options,
        context.worker_state,
        php_runtime::api::OutputBuffer::default(),
        package.native_entries,
    );
    seed_native_child_state(context, &mut child);
    child.current_dynamic_unit = Some(target.unit);
    if let Some(called_class) = called_class.as_ref() {
        child.called_classes.push(called_class.clone());
    }
    child.call_arguments.push(visible_arguments.clone());
    let object = instance_method
        .then(|| values.first())
        .flatten()
        .and_then(|value| match value {
            Value::Object(object) => Some(object.clone()),
            _ => None,
        });
    child.call_frames.push(native_backtrace_frame(
        &package.compiled,
        target.function,
        called_class.as_deref(),
        object,
        visible_arguments.clone(),
    ));
    let encoded = bound
        .into_iter()
        .map(|value| child.encode(value))
        .collect::<Result<Vec<_>, _>>()?;
    let transition_started_at = context.options.collect_counters.then(|| {
        (
            std::time::Instant::now(),
            context.active_helper_child_time_nanos(),
        )
    });
    let guard = activate_native_context(&mut child);
    let outcome = handle.invoke_i64_with_native_unwind(
        &encoded,
        php_jit::JIT_RUNTIME_ABI_HASH,
        |types, value| native_catch_matches(&child, types, value),
    );
    drop(guard);
    if let Some((started_at, child_time_before)) = transition_started_at {
        let nested_helper_time = context
            .active_helper_child_time_nanos()
            .saturating_sub(child_time_before);
        context.record_native_transition("external_unit", started_at.elapsed(), nested_helper_time);
    }
    context.output.write_bytes(child.output.as_bytes());
    let child_diagnostic_message = child
        .diagnostic
        .as_ref()
        .map(|diagnostic| diagnostic.message().to_owned());
    if let Some(diagnostic) = child.diagnostic.take() {
        context.diagnostic = Some(diagnostic);
    }
    merge_native_child_state(context, &mut child);
    match outcome {
        Ok(php_jit::JitI64InvokeOutcome::Returned(value)) => {
            let value = native_external_return_value(child.decode(value)?, target.unit);
            context.encode(value)
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
            if status == php_jit::JitCallStatus::RETURN_REFERENCE.0 as i32 =>
        {
            let value = native_external_return_value(child.decode(value)?, target.unit);
            context.encode(value)
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            state,
        }) if status == php_jit::JitCallStatus::THROW.0 as i32 => {
            let throwable = child.decode(value).map_err(|error| {
                let continuation = child
                    .instruction_for_continuation(state.function_id, state.continuation_id)
                    .map(|instruction| format!(" at {:?}", instruction.kind))
                    .unwrap_or_else(|| {
                        format!(
                            " at native continuation {}:{}",
                            state.function_id, state.continuation_id
                        )
                    });
                format!(
                    "dynamic native function {} returned an undecodable throwable {value}{continuation}: {error}",
                    function.name
                )
            })?;
            context.pending_throwable = Some(native_throwable_with_frame(
                throwable,
                &function.name,
                visible_arguments,
            ));
            Err("E_PHP_RETHROW".to_owned())
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
            if status == php_jit::JitCallStatus::EXIT.0 as i32 =>
        {
            let value = child.decode(value)?;
            let value = context.encode(value)?;
            Err(format!("E_PHP_EXIT:{value}"))
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. }) => {
            let continuation = child
                .instruction_for_continuation(state.function_id, state.continuation_id)
                .map(|instruction| format!(" at {:?}", instruction.kind))
                .unwrap_or_else(|| {
                    format!(
                        " at native continuation {}:{}",
                        state.function_id, state.continuation_id
                    )
                });
            Err(format!(
                "dynamic native function {} returned status {status}{continuation}{}",
                function.name,
                child_diagnostic_message
                    .as_deref()
                    .map_or_else(String::new, |message| format!(": {message}"))
            ))
        }
        Err(error) => Err(format!(
            "dynamic native function invocation failed: {error:?}"
        )),
    }
}

fn native_external_return_value(value: Value, owner_unit: usize) -> Value {
    match value {
        Value::Callable(callable) => match callable.as_ref() {
            php_runtime::api::CallableValue::Closure(closure)
                if closure.context.owner_unit.is_none() =>
            {
                Value::Callable(Box::new(php_runtime::api::CallableValue::Closure(
                    closure.clone().with_owner_unit(Some(owner_unit)),
                )))
            }
            _ => Value::Callable(callable),
        },
        value => value,
    }
}

fn native_value_with_owner_unit(value: Value, owner_unit: Option<usize>) -> Value {
    match value {
        Value::Callable(callable) => match callable.as_ref() {
            php_runtime::api::CallableValue::Closure(closure)
                if closure.context.owner_unit.is_none() && owner_unit.is_some() =>
            {
                Value::Callable(Box::new(php_runtime::api::CallableValue::Closure(
                    closure.clone().with_owner_unit(owner_unit),
                )))
            }
            _ => Value::Callable(callable),
        },
        value => value,
    }
}

fn invoke_native_method(
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
) -> Result<i64, String> {
    invoke_native_method_with_trace_arguments(context, function, arguments, None)
}

fn invoke_native_method_with_trace_arguments(
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    trace_arguments: Option<&[Value]>,
) -> Result<i64, String> {
    let function_name = context
        .unit
        .functions
        .get(function.index())
        .map_or("<unknown>", |function| function.name.as_str());
    let _depth_guard = enter_native_call(function_name)?;
    let handle = ensure_native_entry(context, function)?;
    let instance_method = context.unit.classes.iter().any(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function == function && !method.flags.is_static)
    });
    let object = instance_method
        .then(|| arguments.first())
        .flatten()
        .and_then(|receiver| context.decode(*receiver).ok())
        .and_then(|receiver| match receiver {
            Value::Object(object) => Some(object),
            _ => None,
        });
    let called_class = object
        .as_ref()
        .map(php_runtime::api::ObjectRef::class_name)
        .or_else(|| context.called_classes.last().cloned());
    let pushed_called_class = called_class.is_some();
    if let Some(class) = called_class.as_ref() {
        context.called_classes.push(class.clone());
    }
    let leading = context
        .unit
        .functions
        .get(function.index())
        .map_or(0, |target| {
            target.captures.len()
                + usize::from(instance_method)
                + usize::from(native_function_has_implicit_closure_this(target))
        });
    let frame_arguments = trace_arguments.map_or_else(
        || {
            arguments
                .iter()
                .skip(leading)
                .map(|argument| context.decode(*argument))
                .collect::<Result<Vec<_>, _>>()
        },
        |arguments| Ok(arguments.to_vec()),
    )?;
    context.call_frames.push(native_backtrace_frame(
        context.compiled,
        function,
        called_class.as_deref(),
        object,
        frame_arguments.clone(),
    ));
    let transition_started_at = context.options.collect_counters.then(|| {
        (
            std::time::Instant::now(),
            context.active_helper_child_time_nanos(),
        )
    });
    let outcome = handle.invoke_i64_with_native_unwind(
        arguments,
        php_jit::JIT_RUNTIME_ABI_HASH,
        |types, value| native_catch_matches(context, types, value),
    );
    if let Some((started_at, child_time_before)) = transition_started_at {
        let nested_helper_time = context
            .active_helper_child_time_nanos()
            .saturating_sub(child_time_before);
        context.record_native_transition("same_unit", started_at.elapsed(), nested_helper_time);
    }
    context.call_frames.pop();
    if pushed_called_class {
        context.called_classes.pop();
    }
    match outcome {
        Ok(php_jit::JitI64InvokeOutcome::Returned(value)) => {
            let returns_by_ref = context
                .unit
                .functions
                .get(function.index())
                .is_some_and(|function| function.returns_by_ref);
            if returns_by_ref {
                let target = &context.unit.functions[function.index()];
                let span = target
                    .blocks
                    .iter()
                    .filter_map(|block| block.terminator.as_ref())
                    .find(|terminator| {
                        matches!(
                            terminator.kind,
                            php_ir::instruction::TerminatorKind::Return {
                                by_ref_local: None,
                                ..
                            }
                        )
                    })
                    .map_or(target.span, |terminator| terminator.span);
                let path = context
                    .unit
                    .files
                    .get(span.file.index())
                    .map_or("<unknown>", |file| file.path.as_str());
                let line = std::fs::read(path).ok().map_or(1, |bytes| {
                    bytes
                        .iter()
                        .take(span.start as usize)
                        .filter(|byte| **byte == b'\n')
                        .count()
                        + 1
                });
                context.output.write_bytes(format!(
                    "\nNotice: Only variable references should be returned by reference in {path} on line {line}\n"
                ));
                let value = context.decode(value)?;
                return context.encode(Value::Reference(php_runtime::api::ReferenceCell::new(
                    value,
                )));
            }
            Ok(value)
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
            if status == php_jit::JitCallStatus::RETURN_REFERENCE.0 as i32 =>
        {
            Ok(value)
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            state,
        }) if status == php_jit::JitCallStatus::THROW.0 as i32 => {
            let throwable = context.decode(value).map_err(|error| {
                let continuation = context
                    .instruction_for_continuation(state.function_id, state.continuation_id)
                    .map(|instruction| format!(" at {:?}", instruction.kind))
                    .unwrap_or_else(|| {
                        format!(
                            " at native continuation {}:{}",
                            state.function_id, state.continuation_id
                        )
                    });
                format!(
                    "native method {function_name} returned an undecodable throwable {value}{continuation}: {error}"
                )
            })?;
            context.pending_throwable = Some(native_throwable_with_frame(
                throwable,
                function_name,
                frame_arguments,
            ));
            Err("E_PHP_RETHROW".to_owned())
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
            if status == php_jit::JitCallStatus::EXIT.0 as i32 =>
        {
            Err(format!("E_PHP_EXIT:{value}"))
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            state,
        }) if status == php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32
            && context.active_fiber.is_some() =>
        {
            context.pending_nested_fiber_execution = Some(NativeFiberExecution {
                handle,
                arguments: arguments.to_vec(),
                state,
                nested: None,
            });
            context.pending_fiber_suspension_value = Some(value);
            Err("E_PHP_SUSPEND_FIBER".to_owned())
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. }) => {
            let continuation = context
                .instruction_for_continuation(state.function_id, state.continuation_id)
                .map(|instruction| format!(" at {:?}", instruction.kind))
                .unwrap_or_else(|| {
                    format!(
                        " at native continuation {}:{}",
                        state.function_id, state.continuation_id
                    )
                });
            let diagnostic = context
                .diagnostic
                .as_ref()
                .map(|diagnostic| format!(": {}", diagnostic.message()))
                .unwrap_or_default();
            Err(format!(
                "native method {function_name} returned status {status}{continuation}{diagnostic}"
            ))
        }
        Err(error) => Err(format!("native method invocation failed: {error:?}")),
    }
}

fn invoke_native_property_magic(
    context: &mut NativeExecutionContext<'_>,
    class: &php_ir::module::ClassEntry,
    receiver: i64,
    property: &str,
    magic: &str,
    caller_function: u32,
) -> Result<Option<Value>, String> {
    let Some(method) = class
        .methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(magic))
    else {
        return Ok(None);
    };
    if method.function.raw() == caller_function {
        return Ok(None);
    }
    let name = context.encode(Value::String(PhpString::from_bytes(
        property.as_bytes().to_vec(),
    )))?;
    let value = invoke_native_method(context, method.function, &[receiver, name])?;
    context.decode(value).map(Some)
}

fn execute_native_property_instruction(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
) -> Option<Result<i64, String>> {
    use php_ir::InstructionKind;
    let (object, property, dynamic_property) = match &instruction.kind {
        InstructionKind::FetchDynamicProperty { .. }
        | InstructionKind::IssetDynamicProperty { .. }
        | InstructionKind::EmptyDynamicProperty { .. }
        | InstructionKind::IssetDynamicPropertyDim { .. }
        | InstructionKind::EmptyDynamicPropertyDim { .. }
        | InstructionKind::AssignDynamicProperty { .. }
        | InstructionKind::UnsetDynamicProperty { .. } => {
            let [object, property, ..] = arguments else {
                return Some(Err("dynamic property operands are missing".to_owned()));
            };
            (*object, String::new(), Some(*property))
        }
        InstructionKind::IssetProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::EmptyProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::UnsetProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::UnsetPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::AssignPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::IssetPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::EmptyPropertyDim {
            object: _,
            property,
            ..
        } => {
            let [object, ..] = arguments else {
                return Some(Err("property object operand is missing".to_owned()));
            };
            (*object, property.clone(), None)
        }
        _ => return None,
    };
    let property = if let Some(property) = dynamic_property {
        match context.decode(property).and_then(native_string) {
            Ok(property) => String::from_utf8_lossy(&property).into_owned(),
            Err(error) => return Some(Err(error)),
        }
    } else {
        property
    };
    let object_encoded = object;
    let closure_operand = context
        .unit
        .functions
        .get(caller_function as usize)
        .and_then(|function| {
            let object_register = match &instruction.kind {
                InstructionKind::AssignDynamicProperty {
                    object: php_ir::Operand::Register(register),
                    ..
                } => Some(*register),
                _ => None,
            }?;
            let local = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|candidate| match candidate.kind {
                    InstructionKind::LoadLocal { dst, local }
                    | InstructionKind::LoadLocalQuiet { dst, local }
                        if dst == object_register =>
                    {
                        Some(local)
                    }
                    _ => None,
                })?;
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|candidate| match candidate.kind {
                    InstructionKind::StoreLocal {
                        local: target,
                        src: php_ir::Operand::Register(source),
                    } if target == local => function
                        .blocks
                        .iter()
                        .flat_map(|block| &block.instructions)
                        .any(|origin| {
                            matches!(origin.kind, InstructionKind::MakeClosure { dst, .. } if dst == source)
                        }),
                    _ => false,
                })
                .then_some(())
        })
        .is_some();
    let mut decoded_object = match context.decode(object) {
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
    };
    for _ in 0..16 {
        let Value::Reference(reference) = decoded_object else {
            break;
        };
        decoded_object = reference.get();
    }
    if !matches!(decoded_object, Value::Object(_)) {
        let quiet_result = match instruction.kind {
            InstructionKind::IssetProperty { .. }
            | InstructionKind::IssetDynamicProperty { .. }
            | InstructionKind::IssetDynamicPropertyDim { .. }
            | InstructionKind::IssetPropertyDim { .. } => Some(false),
            InstructionKind::EmptyProperty { .. }
            | InstructionKind::EmptyDynamicProperty { .. }
            | InstructionKind::EmptyDynamicPropertyDim { .. }
            | InstructionKind::EmptyPropertyDim { .. } => Some(true),
            _ => None,
        };
        if let Some(value) = quiet_result {
            return Some(context.encode(Value::Bool(value)));
        }
    }
    let object = match decoded_object {
        Value::Object(object) => object,
        Value::Callable(_) => {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Cannot create dynamic property Closure::${property}"
            )));
        }
        _ if closure_operand => {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Cannot create dynamic property Closure::${property}"
            )));
        }
        value => {
            return Some(Err(format!(
                "Attempt to access property {property} on {}",
                native_value_type_name(&value)
            )));
        }
    };
    let normalized_class = normalize_class_name(&object.class_name());
    let class = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalized_class)
        .cloned();
    let caller_owns_class_scope = class.as_ref().is_some_and(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function.raw() == caller_function)
    });
    let result = match &instruction.kind {
        InstructionKind::FetchDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && native_calling_class(context, caller_function).is_some_and(|class| {
                    class.methods.iter().any(|method| {
                        method.function.raw() == caller_function
                            && method.name.eq_ignore_ascii_case("__get")
                    })
                })
            {
                return Some(Err(format!(
                    "Undefined property: {}::${property}",
                    object.display_name()
                )));
            }
            object.get_property(&property).unwrap_or(Value::Null)
        }
        InstructionKind::IssetProperty { .. } | InstructionKind::IssetDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && let Some(class) = &class
                && let Some(value) = match invoke_native_property_magic(
                    context,
                    class,
                    object_encoded,
                    &property,
                    "__isset",
                    caller_function,
                ) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                }
            {
                Value::Bool(native_property_truthy(&value))
            } else {
                Value::Bool(
                    object
                        .get_property(&property)
                        .is_some_and(|value| native_property_is_set(&value)),
                )
            }
        }
        InstructionKind::EmptyProperty { .. } | InstructionKind::EmptyDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && let Some(class) = &class
                && let Some(isset) = match invoke_native_property_magic(
                    context,
                    class,
                    object_encoded,
                    &property,
                    "__isset",
                    caller_function,
                ) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                }
            {
                if native_property_truthy(&isset) {
                    let value = match invoke_native_property_magic(
                        context,
                        class,
                        object_encoded,
                        &property,
                        "__get",
                        caller_function,
                    ) {
                        Ok(value) => value.unwrap_or(Value::Null),
                        Err(error) => return Some(Err(error)),
                    };
                    Value::Bool(!native_property_truthy(&value))
                } else {
                    Value::Bool(true)
                }
            } else {
                Value::Bool(
                    object
                        .get_property(&property)
                        .is_none_or(|value| !native_property_truthy(&value)),
                )
            }
        }
        InstructionKind::IssetPropertyDim { dims, .. }
        | InstructionKind::EmptyPropertyDim { dims, .. }
        | InstructionKind::IssetDynamicPropertyDim { dims, .. }
        | InstructionKind::EmptyDynamicPropertyDim { dims, .. } => {
            let key_offset = match instruction.kind {
                InstructionKind::IssetDynamicPropertyDim { .. }
                | InstructionKind::EmptyDynamicPropertyDim { .. } => 2,
                _ => 1,
            };
            let keys = arguments
                .iter()
                .skip(key_offset)
                .take(dims.len())
                .map(|key| {
                    context
                        .decode(*key)
                        .ok()
                        .and_then(|key| php_runtime::api::ArrayKey::from_value(&key))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = keys else {
                return Some(Err("property dimension key is invalid".to_owned()));
            };
            let mut value = object.get_property(&property);
            for key in keys {
                value = match value {
                    Some(Value::Reference(reference)) => match reference.get() {
                        Value::Array(array) => array.get(&key).cloned(),
                        Value::Object(object) => native_simple_xml_dimension(&object, &key),
                        _ => None,
                    },
                    Some(Value::Array(array)) => array.get(&key).cloned(),
                    Some(Value::Object(object)) => native_simple_xml_dimension(&object, &key),
                    _ => None,
                };
            }
            if matches!(
                instruction.kind,
                InstructionKind::IssetPropertyDim { .. }
                    | InstructionKind::IssetDynamicPropertyDim { .. }
            ) {
                Value::Bool(value.is_some_and(|value| native_property_is_set(&value)))
            } else {
                Value::Bool(value.is_none_or(|value| !native_property_truthy(&value)))
            }
        }
        InstructionKind::AssignDynamicProperty { .. } => {
            let Some(value) = arguments.get(2).copied() else {
                return Some(Err(
                    "dynamic property assignment value is missing".to_owned()
                ));
            };
            let value = match context.decode(value) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if let Some(class) = &class {
                if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
                    if let Some(type_) = &entry.type_
                        && !native_value_matches_ir_type_in_context(context, &value, type_)
                    {
                        return Some(Err(format!(
                            "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                            native_assignment_type_name(&value),
                            class.display_name,
                            property,
                            native_ir_type_name(type_)
                        )));
                    }
                    if entry.flags.is_private && !caller_owns_class_scope {
                        return Some(Err(format!(
                            "E_PHP_THROW:Error:Cannot access private property {}::${property}",
                            class.display_name
                        )));
                    }
                } else if let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__set"))
                    .filter(|method| method.function.raw() != caller_function)
                {
                    let name = match context.encode(Value::String(PhpString::from_bytes(
                        property.as_bytes().to_vec(),
                    ))) {
                        Ok(name) => name,
                        Err(error) => return Some(Err(error)),
                    };
                    if let Err(error) = invoke_native_method(
                        context,
                        method.function,
                        &[object_encoded, name, arguments[2]],
                    ) {
                        return Some(Err(error));
                    }
                    return Some(context.encode(value));
                }
            }
            object.set_property(property, value.clone());
            value
        }
        InstructionKind::UnsetProperty { .. } | InstructionKind::UnsetDynamicProperty { .. } => {
            if let Some(class) = &class {
                if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
                    if entry.flags.is_private && !caller_owns_class_scope {
                        return Some(Err(format!(
                            "E_PHP_THROW:Error:Cannot access private property {}::${property}",
                            class.display_name
                        )));
                    }
                } else if let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__unset"))
                    .filter(|method| method.function.raw() != caller_function)
                {
                    let name = match context.encode(Value::String(PhpString::from_bytes(
                        property.as_bytes().to_vec(),
                    ))) {
                        Ok(name) => name,
                        Err(error) => return Some(Err(error)),
                    };
                    if let Err(error) =
                        invoke_native_method(context, method.function, &[object_encoded, name])
                    {
                        return Some(Err(error));
                    }
                    return Some(context.encode(Value::Null));
                }
            }
            object.unset_property(&property);
            Value::Null
        }
        InstructionKind::UnsetPropertyDim { dims, .. } => {
            let keys = arguments
                .iter()
                .skip(1)
                .take(dims.len())
                .map(|key| {
                    context
                        .decode(*key)
                        .ok()
                        .and_then(|key| php_runtime::api::ArrayKey::from_value(&key))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = keys else {
                return Some(Err("property dimension key is invalid".to_owned()));
            };
            let _ = object.try_modify_property_value(&property, |value| {
                unset_native_array_dims(value, &keys);
            });
            Value::Null
        }
        InstructionKind::AssignPropertyDim { dims, append, .. } => {
            let value_index = 1 + dims.len();
            let Some(replacement) = arguments.get(value_index).copied() else {
                return Some(Err("property dimension value is missing".to_owned()));
            };
            let replacement = match context.decode(replacement) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let keys = arguments
                .iter()
                .skip(1)
                .take(dims.len())
                .map(|key| {
                    context
                        .decode(*key)
                        .ok()
                        .and_then(|key| php_runtime::api::ArrayKey::from_value(&key))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = keys else {
                return Some(Err("property dimension key is invalid".to_owned()));
            };
            if let Some(class) = &class
                && let Some(entry) = class.properties.iter().find(|entry| entry.name == property)
                && entry.flags.is_readonly
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot indirectly modify readonly property {}::${property}",
                    class.display_name
                )));
            }
            if let Some(Value::Object(target)) = object.get_property(&property)
                && let Some(target_class) = context.unit.classes.iter().find(|class| {
                    class.name == normalize_class_name(&target.class_name())
                        && class
                            .interfaces
                            .iter()
                            .any(|interface| interface.eq_ignore_ascii_case("ArrayAccess"))
                })
                && let Some(offset_set) = target_class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("offsetSet"))
            {
                let key = keys.first().cloned().map_or(Value::Null, |key| match key {
                    php_runtime::api::ArrayKey::Int(value) => Value::Int(value),
                    php_runtime::api::ArrayKey::String(value) => Value::String(value),
                });
                let receiver = match context.encode(Value::Object(target)) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let key = match context.encode(key) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let replacement_encoded = match context.encode(replacement.clone()) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                if let Err(error) = invoke_native_method(
                    context,
                    offset_set.function,
                    &[receiver, key, replacement_encoded],
                ) {
                    return Some(Err(error));
                }
                return Some(context.encode(replacement));
            }
            let result = replacement.clone();
            let modified = object.try_modify_property_value(&property, |value| {
                assign_native_array_dims(value, &keys, replacement, *append);
            });
            if !matches!(modified, Ok(Some(()))) {
                let mut value = object.get_property(&property).unwrap_or(Value::Null);
                assign_native_array_dims(&mut value, &keys, result.clone(), *append);
                object.set_property(property, value);
            }
            result
        }
        _ => return None,
    };
    Some(context.encode(result))
}

fn execute_native_class_constant(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    caller_function: u32,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::FetchClassConstant {
        class_name,
        constant,
        ..
    } = &instruction.kind
    else {
        return None;
    };
    let resolved_class = match class_name.to_ascii_lowercase().as_str() {
        "self" => {
            native_effective_calling_class(context, caller_function).map(|class| class.name.clone())
        }
        "static" => context.called_classes.last().cloned().or_else(|| {
            native_effective_calling_class(context, caller_function).map(|class| class.name.clone())
        }),
        "parent" => native_effective_calling_class(context, caller_function)
            .and_then(|class| class.parent.clone()),
        _ => Some(normalize_class_name(class_name)),
    };
    let Some(mut resolved_class) = resolved_class else {
        let message = if class_name.eq_ignore_ascii_case("self") {
            "Cannot use \"self\" in the global scope".to_owned()
        } else if class_name.eq_ignore_ascii_case("parent") {
            "Cannot use \"parent\" when no class scope is active".to_owned()
        } else {
            format!("Cannot resolve class {class_name}")
        };
        return Some(Err(format!("E_PHP_THROW:Error:{message}")));
    };
    if let Some(original) = context
        .class_aliases
        .get(&normalize_class_name(&resolved_class))
    {
        resolved_class = original.clone();
    }
    if constant.eq_ignore_ascii_case("class") {
        let display = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(&resolved_class))
            .map_or(resolved_class.as_str(), |class| class.display_name.as_str());
        return Some(context.encode(Value::String(PhpString::from_bytes(
            display.as_bytes().to_vec(),
        ))));
    }
    resolved_class = normalize_class_name(&resolved_class);
    if class_name.eq_ignore_ascii_case("ArrayObject")
        && constant.eq_ignore_ascii_case("ARRAY_AS_PROPS")
    {
        return Some(Ok(2));
    }
    if let Some(value) = native_internal_class_constant(&resolved_class, constant) {
        return Some(context.encode(value));
    }
    let mut candidate = resolved_class.clone();
    while let Some(class) = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == candidate)
        .cloned()
    {
        if let Some(entry) = class
            .constants
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(constant))
        {
            let caller = native_calling_class(context, caller_function);
            if entry.flags.is_private && caller.is_none_or(|caller| caller.name != class.name) {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access private constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if entry.flags.is_protected
                && caller
                    .is_none_or(|caller| !native_class_is_a(context, &caller.name, &class.name))
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access protected constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if let Some(value) = entry
                .value
                .and_then(|value| context.unit.constants.get(value.index()))
            {
                return Some(
                    native_runtime_constant_value(context, value)
                        .and_then(|value| context.encode(value)),
                );
            }
            if let Some(reference) = &entry.value_named_constant {
                for name in &reference.names {
                    if let Ok(value) = context.lookup_constant(name) {
                        return Some(context.encode(value));
                    }
                }
            }
            if let Some(reference) = &entry.value_class_constant {
                let value = php_ir::IrConstant::ClassConstant {
                    class_name: reference.class_name.clone(),
                    display_class_name: reference.display_class_name.clone(),
                    constant_name: reference.constant_name.clone(),
                };
                return Some(
                    native_runtime_constant_value(context, &value)
                        .and_then(|value| context.encode(value)),
                );
            }
        }
        if let Some(case) = class
            .enum_cases
            .iter()
            .find(|case| case.name.eq_ignore_ascii_case(constant))
            .cloned()
        {
            return Some(encode_native_enum_case(context, &class, &case));
        }
        let Some(parent) = class.parent else {
            break;
        };
        candidate = normalize_class_name(&parent);
    }
    if context
        .unit
        .classes
        .iter()
        .all(|class| class.name != resolved_class)
        && native_external_class(context, &resolved_class).is_none()
    {
        let normalized = resolved_class.clone();
        let autoload_name = if matches!(
            class_name.to_ascii_lowercase().as_str(),
            "self" | "static" | "parent"
        ) {
            resolved_class.as_str()
        } else {
            class_name.as_str()
        };
        if context.autoload_in_progress.insert(normalized.clone()) {
            let callbacks = context.autoload_callbacks.clone();
            for callback in callbacks {
                if let Err(error) = invoke_native_callable_value(
                    context,
                    callback,
                    &[Value::String(PhpString::from_bytes(
                        autoload_name.as_bytes().to_vec(),
                    ))],
                    instruction,
                    None,
                ) {
                    context.autoload_in_progress.remove(&normalized);
                    return Some(Err(error));
                }
                if native_external_class(context, &resolved_class).is_some() {
                    break;
                }
            }
            context.autoload_in_progress.remove(&normalized);
        }
    }
    // The late-static class may live in another unit while the requested
    // constant is declared by a parent in the current unit (or vice versa).
    // Walk the combined hierarchy instead of checking only the first external
    // class.
    let mut candidate = resolved_class.clone();
    loop {
        let (owner_unit, class) = if let Some(class) = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)
            .cloned()
        {
            (None, class)
        } else if let Some((unit, class)) = native_external_class(context, &candidate) {
            (Some(unit), class)
        } else {
            break;
        };
        if let Some(entry) = class
            .constants
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(constant))
        {
            let caller = native_calling_class(context, caller_function);
            if entry.flags.is_private && caller.is_none_or(|caller| caller.name != class.name) {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access private constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if entry.flags.is_protected
                && caller
                    .is_none_or(|caller| !native_class_is_a(context, &caller.name, &class.name))
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access protected constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if let Some(value) = entry.value.and_then(|value| {
                owner_unit.map_or_else(
                    || context.unit.constants.get(value.index()),
                    |unit| {
                        context.dynamic_units.get(unit).and_then(|package| {
                            package.compiled.unit().constants.get(value.index())
                        })
                    },
                )
            }) {
                return Some(
                    native_runtime_constant_value(context, value)
                        .and_then(|value| context.encode(value)),
                );
            }
            if let Some(reference) = &entry.value_named_constant {
                for name in &reference.names {
                    if let Ok(value) = context.lookup_constant(name) {
                        return Some(context.encode(value));
                    }
                }
            }
            if let Some(reference) = &entry.value_class_constant {
                let value = php_ir::IrConstant::ClassConstant {
                    class_name: reference.class_name.clone(),
                    display_class_name: reference.display_class_name.clone(),
                    constant_name: reference.constant_name.clone(),
                };
                return Some(
                    native_runtime_constant_value(context, &value)
                        .and_then(|value| context.encode(value)),
                );
            }
        }
        let Some(parent) = class.parent else {
            break;
        };
        candidate = normalize_class_name(&parent);
    }
    Some(Err(format!(
        "Undefined constant {resolved_class}::{constant}"
    )))
}

fn execute_native_enum_static_method(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallStaticMethod {
        class_name, method, ..
    } = &instruction.kind
    else {
        return None;
    };
    let class = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalize_class_name(class_name) && class.flags.is_enum)
        .cloned()?;
    if method.eq_ignore_ascii_case("cases") {
        let mut result = php_runtime::api::PhpArray::new();
        for case in &class.enum_cases {
            let encoded = match encode_native_enum_case(context, &class, case) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let value = match context.decode(encoded) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            result.append(value);
        }
        return Some(context.encode(Value::Array(result)));
    }
    if method.eq_ignore_ascii_case("from") || method.eq_ignore_ascii_case("tryFrom") {
        let Some(argument) = arguments.first() else {
            return Some(Err(format!(
                "{class_name}::{method}() expects exactly 1 argument"
            )));
        };
        let argument = match context.decode(*argument) {
            Ok(Value::Reference(reference)) => reference.get(),
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        let matching = class.enum_cases.iter().find(|case| {
            case.value
                .and_then(|value| context.unit.constants.get(value.index()))
                .and_then(|value| ir_constant_value(value).ok())
                .is_some_and(|value| value == argument)
        });
        if let Some(case) = matching {
            return Some(encode_native_enum_case(context, &class, case));
        }
        if method.eq_ignore_ascii_case("tryFrom") {
            return Some(context.encode(Value::Null));
        }
        return Some(Err(format!(
            "E_PHP_THROW:ValueError:{} is not a valid backing value for enum {}",
            native_value_type_name(&argument),
            class.display_name
        )));
    }
    None
}

fn native_class_is_a(context: &NativeExecutionContext<'_>, class_name: &str, target: &str) -> bool {
    let target = normalize_class_name(target);
    let class_name = normalize_class_name(class_name);
    if class_name == "arrayiterator" && matches!(target.as_str(), "iterator" | "traversable") {
        return true;
    }
    let mut pending = vec![class_name];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(candidate) = pending.pop() {
        if candidate == target {
            return true;
        }
        if !visited.insert(candidate.clone()) {
            continue;
        }
        if let Some(class) = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)
        {
            if let Some(parent) = &class.parent {
                pending.push(normalize_class_name(parent));
            }
            pending.extend(
                class
                    .interfaces
                    .iter()
                    .map(|interface| normalize_class_name(interface)),
            );
        } else if let Some((_, class)) = native_external_class(context, &candidate) {
            if let Some(parent) = &class.parent {
                pending.push(normalize_class_name(parent));
            }
            pending.extend(
                class
                    .interfaces
                    .iter()
                    .map(|interface| normalize_class_name(interface)),
            );
        }
    }
    false
}

fn native_method_in_hierarchy(
    context: &NativeExecutionContext<'_>,
    class_name: &str,
    method: &str,
) -> Option<php_ir::FunctionId> {
    let mut candidate = normalize_class_name(class_name);
    loop {
        let class = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)?;
        if let Some(entry) = class
            .methods
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(method))
        {
            return Some(entry.function);
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
}

fn native_function_is_generator(
    context: &NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
) -> bool {
    context
        .unit
        .functions
        .get(function.index())
        .is_some_and(|function| {
            function.flags.is_generator
                || function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .any(|instruction| {
                        matches!(
                            instruction.kind,
                            php_ir::InstructionKind::Yield { .. }
                                | php_ir::InstructionKind::YieldFrom { .. }
                        )
                    })
        })
}

fn native_calling_class<'a>(
    context: &'a NativeExecutionContext<'_>,
    function: u32,
) -> Option<&'a php_ir::ClassEntry> {
    context.unit.classes.iter().find(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function.raw() == function)
    })
}

fn native_effective_calling_class<'a>(
    context: &'a NativeExecutionContext<'_>,
    function: u32,
) -> Option<&'a php_ir::ClassEntry> {
    native_calling_class(context, function).or_else(|| {
        let scope = context.lexical_scope_classes.last()?;
        let normalized = normalize_class_name(scope);
        context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalized)
    })
}

fn native_resolve_scoped_class_name(
    context: &NativeExecutionContext<'_>,
    class_name: &str,
    caller_function: u32,
) -> Result<String, String> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" => native_effective_calling_class(context, caller_function)
            .map(|class| class.display_name.clone())
            .ok_or_else(|| "Cannot use \"self\" in the global scope".to_owned()),
        "static" => context
            .called_classes
            .last()
            .cloned()
            .or_else(|| {
                native_effective_calling_class(context, caller_function)
                    .map(|class| class.display_name.clone())
            })
            .ok_or_else(|| "Cannot use \"static\" in the global scope".to_owned()),
        "parent" => native_effective_calling_class(context, caller_function)
            .and_then(|class| {
                class
                    .parent_display_name
                    .clone()
                    .or_else(|| class.parent.clone())
            })
            .ok_or_else(|| "Cannot use \"parent\" when no parent scope is active".to_owned()),
        _ => Ok(class_name.to_owned()),
    }
}

fn native_method_access_error(
    context: &NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    caller_function: u32,
    _late_static_call: bool,
) -> Option<String> {
    let (declaring_class, method) = context.unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == function)
            .map(|method| (class, method))
    })?;
    if !method.flags.is_private && !method.flags.is_protected {
        return None;
    }
    let caller = native_effective_calling_class(context, caller_function);
    if method.flags.is_private && caller.is_none_or(|caller| caller.name != declaring_class.name) {
        if caller.is_none() {
            return Some(format!(
                "Call to private method {}::{}() from global scope",
                declaring_class.display_name, method.name
            ));
        }
        return Some(format!(
            "Cannot access private method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    if method.flags.is_protected
        && caller
            .is_none_or(|caller| !native_class_is_a(context, &caller.name, &declaring_class.name))
    {
        return Some(format!(
            "Cannot access protected method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    None
}

fn native_external_method_access_error(
    context: &NativeExecutionContext<'_>,
    target: NativeDynamicFunction,
    caller_function: u32,
    _late_static_call: bool,
) -> Option<String> {
    let unit = context.dynamic_units.get(target.unit)?.compiled.unit();
    let (declaring_class, method) = unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == target.function)
            .map(|method| (class, method))
    })?;
    if !method.flags.is_private && !method.flags.is_protected {
        return None;
    }
    let caller = native_effective_calling_class(context, caller_function);
    if method.flags.is_private && caller.is_none_or(|caller| caller.name != declaring_class.name) {
        if caller.is_none() {
            return Some(format!(
                "Call to private method {}::{}() from global scope",
                declaring_class.display_name, method.name
            ));
        }
        return Some(format!(
            "Cannot access private method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    if method.flags.is_protected
        && caller
            .is_none_or(|caller| !native_class_is_a(context, &caller.name, &declaring_class.name))
    {
        return Some(format!(
            "Cannot access protected method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    None
}

fn encode_native_call_arguments_array(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
) -> Result<i64, String> {
    let mut array = php_runtime::api::PhpArray::new();
    for argument in arguments {
        array.append(context.decode(*argument)?);
    }
    context.encode(Value::Array(array))
}

fn execute_native_instanceof(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let (object, static_target) = match &instruction.kind {
        php_ir::InstructionKind::InstanceOf { class_name, .. } => {
            (arguments.first().copied(), Some(class_name.as_str()))
        }
        php_ir::InstructionKind::DynamicInstanceOf { .. } => (arguments.first().copied(), None),
        _ => return None,
    };
    let Some(object) = object else {
        return Some(Err("instanceof receiver is missing".to_owned()));
    };
    let target = if let Some(target) = static_target {
        target.to_owned()
    } else {
        let Some(target) = arguments.get(1) else {
            return Some(Err("instanceof target is missing".to_owned()));
        };
        match context.decode(*target) {
            Ok(Value::String(value)) => value.to_string_lossy(),
            Ok(Value::Object(object)) => object.class_name(),
            Ok(value) => {
                return Some(Err(format!(
                    "instanceof target must be a class name, {} given",
                    native_value_type_name(&value)
                )));
            }
            Err(error) => return Some(Err(error)),
        }
    };
    let result = match context.decode(object) {
        Ok(Value::Object(object)) => native_internal_instanceof(&object.class_name(), &target)
            .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target)),
        Ok(Value::Callable(_)) => target.eq_ignore_ascii_case("Closure"),
        Ok(Value::Fiber(_)) => target.eq_ignore_ascii_case("Fiber"),
        Ok(Value::Generator(_)) => target.eq_ignore_ascii_case("Generator"),
        Ok(Value::Array(array)) => {
            super::native_exception_fields(Value::Array(array)).is_some_and(|(class, _, _)| {
                let normalized = class.to_ascii_lowercase();
                target.eq_ignore_ascii_case(&class)
                    || target.eq_ignore_ascii_case("Throwable")
                    || (target.eq_ignore_ascii_case("Exception")
                        && normalized.ends_with("exception"))
                    || (target.eq_ignore_ascii_case("Error") && normalized.ends_with("error"))
            })
        }
        Ok(Value::Reference(reference)) => match reference.get() {
            Value::Object(object) => native_internal_instanceof(&object.class_name(), &target)
                .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target)),
            Value::Callable(_) => target.eq_ignore_ascii_case("Closure"),
            Value::Fiber(_) => target.eq_ignore_ascii_case("Fiber"),
            Value::Generator(_) => target.eq_ignore_ascii_case("Generator"),
            _ => false,
        },
        Ok(_) => false,
        Err(error) => return Some(Err(error)),
    };
    Some(context.encode(Value::Bool(result)))
}

fn execute_native_acquire_callable(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !matches!(
        instruction.kind,
        php_ir::InstructionKind::AcquireCallable { .. }
    ) {
        return None;
    }
    let Some(value) = arguments.first() else {
        return Some(Err("callable value is missing".to_owned()));
    };
    let value = match context.decode(*value) {
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
    };
    let callable = match value {
        Value::Callable(callable) => return Some(context.encode(Value::Callable(callable))),
        Value::String(name) => php_runtime::api::CallableValue::UserFunction {
            name: name.to_string_lossy(),
        },
        Value::Object(object) => php_runtime::api::CallableValue::BoundMethod {
            target: php_runtime::api::CallableMethodTarget::Object(object),
            method: "__invoke".to_owned(),
            scope: None,
        },
        Value::Array(array) => {
            let target = array
                .get(&php_runtime::api::ArrayKey::Int(0))
                .cloned()
                .ok_or_else(|| "callable array target is missing".to_owned());
            let method = array
                .get(&php_runtime::api::ArrayKey::Int(1))
                .cloned()
                .ok_or_else(|| "callable array method is missing".to_owned());
            let (target, method) = match (target, method) {
                (Ok(target), Ok(Value::String(method))) => (target, method.to_string_lossy()),
                (Err(error), _) | (_, Err(error)) => return Some(Err(error)),
                _ => return Some(Err("callable array method must be a string".to_owned())),
            };
            let target = match target {
                Value::Object(object) => php_runtime::api::CallableMethodTarget::Object(object),
                Value::String(class) => {
                    php_runtime::api::CallableMethodTarget::Class(class.to_string_lossy())
                }
                value => {
                    return Some(Err(format!(
                        "callable array target must be object or class-string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            php_runtime::api::CallableValue::BoundMethod {
                target,
                method,
                scope: None,
            }
        }
        other => {
            return Some(Err(format!(
                "{} is not callable",
                native_value_type_name(&other)
            )));
        }
    };
    Some(context.encode(Value::Callable(Box::new(callable))))
}

fn execute_native_resolve_callable(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::ResolveCallable { callable, .. } = &instruction.kind else {
        return None;
    };
    let name = match callable {
        php_ir::instruction::CallableKind::FunctionName { name } => name,
        php_ir::instruction::CallableKind::MethodPlaceholder { target }
        | php_ir::instruction::CallableKind::UnresolvedDynamic { target } => {
            return Some(Err(format!("E_PHP_THROW:Error:{target}")));
        }
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    let fallback = normalized
        .rsplit_once('\\')
        .map(|(_, basename)| basename.to_owned());
    let exists = context.function_id(&normalized).is_some()
        || context.external_function(&normalized).is_some()
        || context.visible_function_names.contains(&normalized)
        || php_extensions::BuiltinRegistry::new().contains(&normalized)
        || fallback.as_ref().is_some_and(|fallback| {
            context.function_id(fallback).is_some()
                || context.external_function(fallback).is_some()
                || context.visible_function_names.contains(fallback)
                || php_extensions::BuiltinRegistry::new().contains(fallback)
        });
    if !exists {
        return Some(Err(format!(
            "E_PHP_THROW:Error:Call to undefined function {name}()"
        )));
    }
    Some(context.encode(Value::Callable(Box::new(
        php_runtime::api::CallableValue::UserFunction { name: name.clone() },
    ))))
}

fn native_rebind_closure(
    closure: &php_runtime::api::ClosurePayload,
    new_this: Option<Value>,
    new_scope: Option<Value>,
) -> Result<Value, String> {
    let bound_this = match new_this {
        Some(Value::Object(object)) => Some(object),
        Some(Value::Null) | None => None,
        Some(value) => {
            return Err(format!(
                "Closure::bind(): Argument #2 ($newThis) must be of type ?object, {} given",
                native_value_type_name(&value)
            ));
        }
    };
    let scope: Option<std::sync::Arc<str>> = match new_scope {
        Some(Value::Object(object)) => Some(object.display_name().into()),
        Some(Value::String(class)) => {
            let class = class.to_string_lossy();
            (class != "static").then(|| class.into())
        }
        Some(Value::Null) => None,
        Some(value) => {
            return Err(format!(
                "Closure::bind(): Argument #3 ($newScope) must be of type object|string|null, {} given",
                native_value_type_name(&value)
            ));
        }
        None => bound_this
            .as_ref()
            .map(|object| object.display_name().into()),
    };
    let mut context = closure.context.clone();
    if let Some(scope) = scope {
        context.scope_class = Some(scope.clone());
        context.called_class = Some(scope.clone());
        context.declaring_class = Some(scope);
    }
    Ok(Value::Callable(Box::new(
        php_runtime::api::CallableValue::Closure(
            closure
                .clone()
                .with_bound_this(bound_this)
                .with_context(context),
        ),
    )))
}

fn execute_native_bind_global(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::BindGlobal { name, .. } = &instruction.kind else {
        return None;
    };
    let current = context
        .inherited_globals
        .get(name)
        .filter(|value| !matches!(value, Value::Uninitialized))
        .cloned()
        .or_else(|| context.options.runtime_context.global_value(name))
        .unwrap_or(Value::Null);
    let reference = match current {
        Value::Reference(reference) => reference,
        value => php_runtime::api::ReferenceCell::new(value),
    };
    context
        .inherited_globals
        .insert(name.clone(), Value::Reference(reference.clone()));
    Some(context.encode(Value::Reference(reference)))
}

#[cfg(test)]
mod tests;
