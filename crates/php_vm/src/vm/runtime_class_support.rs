use super::*;

#[derive(Clone, Debug)]
pub(super) struct RuntimeClassEntryError {
    pub(super) message: String,
    pub(super) constant_initializer_span: Option<IrSpan>,
}

impl RuntimeClassEntryError {
    pub(super) fn new(message: String) -> Self {
        Self {
            message,
            constant_initializer_span: None,
        }
    }

    pub(super) fn with_constant_initializer_span(message: String, span: IrSpan) -> Self {
        Self {
            message,
            constant_initializer_span: Some(span),
        }
    }

    pub(super) fn into_message(self) -> String {
        self.message
    }
}

impl From<String> for RuntimeClassEntryError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<RuntimeClassEntryError> for String {
    fn from(error: RuntimeClassEntryError) -> Self {
        error.into_message()
    }
}

#[derive(Clone, Debug)]
pub(super) enum PhpTokenStaticMethodError {
    RuntimeClass(RuntimeClassEntryError),
    Runtime(String),
}

impl PhpTokenStaticMethodError {
    pub(super) fn into_message(self) -> String {
        match self {
            Self::RuntimeClass(error) => error.into_message(),
            Self::Runtime(message) => message,
        }
    }
}

/// Activation-context handles for one class-name spelling; see
/// `Vm::class_name_handles`.
#[derive(Clone, Debug)]
pub(super) struct ClassNameHandles {
    /// `normalize_class_name` form for scope/declaring-class fields.
    pub(super) normalized: Arc<str>,
    /// `display_class_name` form for the late-static-binding called class.
    pub(super) display: Arc<str>,
}

/// Late-static-binding handle for a receiver object. The stored display name
/// already carries the PHP-visible spelling, so the shared handle is reused
/// directly unless a leading root slash still needs stripping.
/// Call-site strictness resolved at the caller: per-file when the site has a
/// span (linked multi-file units mix strict and weak files in one unit), the
/// unit flag otherwise. Spans are unit-local, so this must only ever be
/// called with the unit that produced the span.
pub(super) fn call_site_strictness(compiled: &CompiledUnit, span: Option<IrSpan>) -> bool {
    span.map_or(compiled.unit().strict_types, |span| {
        compiled.unit().strict_types_for_span(span)
    })
}

pub(super) fn object_called_class_handle(object: &ObjectRef) -> Arc<str> {
    let display = object.display_name_handle();
    if display.starts_with('\\') {
        Arc::from(display_class_name(&display))
    } else {
        display
    }
}

/// Cache of resolved runtime class entries, keyed by normalized class name and
/// guarded by the class-table epoch. Rebuilding a `RuntimeClassEntry` walks the
/// lineage, evaluates property/constant defaults, and maps every method, so a
/// hot instantiation site would otherwise pay that on every `new`. Entries are
/// shared via `Rc` (the object model copies only the property defaults it needs
/// and never retains the entry); property defaults are scalar/string/array-COW
/// or shared enum-case singletons, so sharing a cached entry is behavior-neutral.
#[derive(Clone, Debug, Default)]
pub(super) struct RuntimeClassEntryCache {
    pub(super) epoch: u64,
    pub(super) entries: HashMap<String, Rc<RuntimeClassEntry>>,
}

/// Cache of resolved raw IR class entries, keyed by normalized class name and
/// guarded by the class-table epoch. `lookup_class_in_state` returns a shared
/// `Arc<ClassEntry>` (a cheap refcount bump), but a hot `new` site still needs an
/// owned `ClassEntry` and would deep-clone the whole class definition out of that
/// `Arc` per instantiation. Sharing the owned entry via `Rc` is behavior-neutral:
/// within a class-table epoch a class definition is immutable (redeclaration is a
/// fatal), and the cache is dropped when the epoch changes.
#[derive(Clone, Debug, Default)]
pub(super) struct IrClassEntryCache {
    pub(super) epoch: u64,
    pub(super) entries: HashMap<String, Rc<php_ir::module::ClassEntry>>,
}

/// Cache of default declared-slot templates, keyed by normalized class name and
/// guarded by the class-table epoch. Each template is the slot-index-aligned
/// default vector a fresh instance starts from; cloning it into a new object
/// skips the per-property iterate + filter + `slot_by_name` hash-lookup loop
/// that `ObjectRef::new_with_display_name` runs. Sharing it via `Rc` is
/// behavior-neutral within a class-table epoch (a class definition is immutable
/// until redeclaration, which bumps the epoch and drops the cache), and the
/// template is byte-identical to the defaults the slow path builds.
#[derive(Clone, Debug, Default)]
pub(super) struct DefaultSlotTemplateCache {
    pub(super) epoch: u64,
    pub(super) entries: HashMap<String, Rc<Vec<Option<Value>>>>,
}

/// Cache of `__construct` resolution outcomes, keyed by (normalized class name,
/// normalized caller scope) and guarded by the class-table epoch. Resolving a
/// constructor runs a private-scope probe plus the full inheritance + visibility
/// walk (`lookup_resolved_method_in_state`); a hot instantiation site would
/// otherwise pay that on every `new`. Caching the whole `Result` — `Ok(Some)`,
/// `Ok(None)`, and `Err` alike — reproduces the exact outcome on a hit (a
/// visibility/cycle diagnostic is replayed, never re-walked or bypassed). The
/// caller scope is part of the key because private/protected resolution differs
/// by calling scope. Cloning the cached `ResolvedMethodOwned` bumps the shared
/// `Arc<ClassEntry>` and clones the resolved method entry, which is exactly what
/// the walk's terminal step does, so a hit is behavior-neutral; the cache is
/// dropped when the epoch changes.
#[derive(Clone, Debug, Default)]
pub(super) struct ConstructorResolutionCache {
    pub(super) epoch: u64,
    pub(super) entries:
        HashMap<(String, Option<String>), Result<Option<ResolvedMethodOwned>, String>>,
}

pub(super) fn is_reflection_runtime_class(name: &str) -> bool {
    let name = normalize_class_name(name);
    [
        "reflectionclass",
        "reflectionfunction",
        "reflectionmethod",
        "reflectionproperty",
        "reflectionclassconstant",
        "reflectionenum",
        "reflectionenumunitcase",
        "reflectionenumbackedcase",
        "reflectionparameter",
        "reflectionattribute",
        "reflectionnamedtype",
        "reflectionextension",
    ]
    .contains(&name.as_str())
}

pub(crate) fn normalize_function_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

pub(super) fn normalize_stream_wrapper_protocol(protocol: &str) -> String {
    protocol.trim().trim_end_matches("://").to_ascii_lowercase()
}

pub(super) fn stream_uri_protocol(uri: &str) -> Option<String> {
    uri.find("://")
        .map(|index| normalize_stream_wrapper_protocol(&uri[..index]))
        .filter(|protocol| !protocol.is_empty())
}

pub(super) fn normalize_exit_code(code: i64) -> i32 {
    code.clamp(0, 255) as i32
}

pub(super) fn script_exit_result(
    output: &OutputBuffer,
    state: &ExecutionState,
    code: i32,
) -> VmResult {
    VmResult::script_exit(
        output.clone(),
        code,
        state.builtins.pcntl_state.is_fork_child(),
    )
}

pub(super) fn compiled_unit_cache_key(compiled: &CompiledUnit) -> u64 {
    compiled.cache_identity()
}

/// True when cloning `value` would allocate or bump a refcount (a refcounted
/// heap value), i.e. a last-use move genuinely avoided clone work. Scalars are
/// `Copy`-like and moving them saves nothing observable.
pub(super) fn value_clone_is_heap(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_)
            | Value::Array(_)
            | Value::Object(_)
            | Value::Resource(_)
            | Value::Fiber(_)
            | Value::Generator(_)
            | Value::Callable(_)
            | Value::Reference(_)
    )
}

pub(super) fn instruction_runtime_error_context(
    message: String,
    unit: &IrUnit,
    function: &IrFunction,
    block_id: BlockId,
    instruction_index: usize,
    instruction: &Instruction,
) -> String {
    let source = unit
        .files
        .first()
        .map(|file| file.path.as_str())
        .unwrap_or("<unknown>");
    format!(
        "{message} in {source} function={} block:{} instruction:{} {:?} constants={}",
        function.name,
        block_id.raw(),
        instruction_index,
        instruction.kind,
        unit.constants.len()
    )
}

#[cfg(feature = "jit-cranelift")]
pub(super) fn jit_compile_cache_key(
    function_id: FunctionId,
    function: &IrFunction,
    options: &VmOptions,
) -> JitCompileCacheKey {
    JitCompileCacheKey {
        function: function_id.raw(),
        ir_fingerprint: stable_hash_bytes(format!("{function:?}").as_bytes()),
        abi_hash: php_jit::JIT_RUNTIME_ABI_HASH,
        config_hash: jit_config_hash(options),
        target_isa: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
    }
}

#[cfg(feature = "jit-cranelift")]
pub(super) fn jit_config_hash(options: &VmOptions) -> u64 {
    let config = format!(
        "jit={};quickening={};inline_caches={};blacklist={};typecheck={};threshold={};loop_threshold={}",
        options.jit.as_str(),
        options.quickening.enabled(),
        options.inline_caches.enabled(),
        options.jit_blacklist.as_str(),
        options.typecheck_fast_paths,
        options.tiering.function_entry_threshold,
        options.tiering.loop_backedge_threshold
    );
    stable_hash_bytes(config.as_bytes())
}

#[cfg(feature = "jit-cranelift")]
pub(super) fn stable_hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(super) fn empty_array_value() -> Value {
    Value::Array(PhpArray::new())
}

pub(super) fn enum_case_object(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    class: &php_ir::module::ClassEntry,
    case: &php_ir::module::ClassEnumCaseEntry,
    constant_value: &impl Fn(ConstId) -> Result<Value, String>,
) -> Result<ObjectRef, String> {
    let key = (
        normalize_class_name(&class.name),
        case.name.to_ascii_lowercase(),
    );
    if let Some(object) = state.enum_cases.get(&key) {
        return Ok(object.clone());
    }
    let runtime_class = runtime_class_entry(
        compiled,
        state,
        class,
        constant_value,
        &|reference| class_constant_reference_value(compiled, state, reference),
        &|reference| named_constant_reference_value(compiled, state, reference),
    )
    .map_err(RuntimeClassEntryError::into_message)?;
    let object = ObjectRef::new_with_display_name(&runtime_class, class.display_name.clone());
    object.set_property("name", Value::String(PhpString::from_test_str(&case.name)));
    if runtime_class.enum_backing_type.is_some() {
        let value = case.value.map(constant_value).transpose()?.ok_or_else(|| {
            format!(
                "E_PHP_VM_ENUM_CASE_MISSING_VALUE: backed enum case {}::{} has no value",
                class.name, case.name
            )
        })?;
        object.set_property("value", value);
    }
    state.enum_cases.insert(key, object.clone());
    Ok(object)
}

pub(super) fn enum_static_method(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    class: &php_ir::module::ClassEntry,
    method: &str,
    args: Vec<CallArgument>,
    constant_value: &impl Fn(ConstId) -> Result<Value, String>,
) -> Result<Value, String> {
    match normalize_method_name(method).as_str() {
        "cases" => {
            if !args.is_empty() {
                return Err(format!(
                    "E_PHP_VM_TOO_MANY_ARGS: enum {}::cases expects no arguments",
                    class.name
                ));
            }
            let mut array = PhpArray::new();
            for case in &class.enum_cases {
                array.append(Value::Object(enum_case_object(
                    compiled,
                    state,
                    class,
                    case,
                    constant_value,
                )?));
            }
            Ok(Value::Array(array))
        }
        "from" | "tryfrom" => {
            enum_backed_lookup(compiled, state, class, method, args, constant_value)
        }
        _ => unreachable!("enum_static_method called for non-enum method"),
    }
}

pub(super) fn enum_backed_lookup(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    class: &php_ir::module::ClassEntry,
    method: &str,
    args: Vec<CallArgument>,
    constant_value: &impl Fn(ConstId) -> Result<Value, String>,
) -> Result<Value, String> {
    let normalized_method = normalize_method_name(method);
    if args.len() != 1 {
        return Err(format!(
            "E_PHP_VM_ENUM_LOOKUP_ARITY: enum {}::{} expects exactly one argument",
            class.name, method
        ));
    }
    if class.enum_backing_type.is_none() {
        return Err(format!(
            "E_PHP_VM_ENUM_LOOKUP_ON_UNIT_ENUM: enum {} has no backing values",
            class.name
        ));
    }
    let needle = &args[0].value;
    for case in &class.enum_cases {
        let Some(value_id) = case.value else {
            continue;
        };
        let value = constant_value(value_id)?;
        if identical(&value, needle) {
            return Ok(Value::Object(enum_case_object(
                compiled,
                state,
                class,
                case,
                constant_value,
            )?));
        }
    }
    if normalized_method == "tryfrom" {
        Ok(Value::Null)
    } else {
        Err(format!(
            "E_PHP_VM_ENUM_VALUE_ERROR: value is not a valid backing value for enum {}",
            class.name
        ))
    }
}

pub(super) fn callable_resolve_reference(value: Value) -> Value {
    match value {
        Value::Reference(cell) => callable_resolve_reference(cell.get()),
        value => value,
    }
}

pub(super) fn callable_string_value(value: Value) -> Option<String> {
    match callable_resolve_reference(value) {
        Value::String(value) => Some(value.to_string_lossy()),
        _ => None,
    }
}

pub(super) fn callable_string_ref(value: &Value) -> Option<String> {
    match value {
        Value::Reference(cell) => callable_string_value(cell.get()),
        Value::String(value) => Some(value.to_string_lossy()),
        _ => None,
    }
}

pub(super) fn magic_args_array(args: Vec<CallArgument>) -> Value {
    let mut array = PhpArray::new();
    for arg in args {
        if let Some(name) = arg.name {
            array.insert(ArrayKey::String(PhpString::from_test_str(&name)), arg.value);
        } else {
            array.append(arg.value);
        }
    }
    Value::Array(array)
}

pub(super) fn debug_info_object(source: &ObjectRef, properties: PhpArray) -> ObjectRef {
    let properties = properties
        .iter()
        .map(|(key, value)| match key {
            ArrayKey::Int(index) => (index.to_string(), index.to_string(), value.clone()),
            ArrayKey::String(name) => {
                let name = name.to_string_lossy();
                (name.clone(), format!("\"{name}\""), value.clone())
            }
        })
        .collect();
    ObjectRef::debug_view_with_properties(source, properties)
}

pub(super) fn spl_internal_debug_info_object(source: &ObjectRef) -> Option<ObjectRef> {
    let runtime_class =
        spl_runtime_marker(source).unwrap_or_else(|| normalize_class_name(&source.class_name()));
    match runtime_class.as_str() {
        "hashcontext" => hash_context_debug_info_object(source),
        "arrayiterator" | "recursivearrayiterator" => {
            let storage =
                spl_entries_to_debug_php_array_excluding(spl_entries(source), source.id());
            Some(ObjectRef::debug_view_with_properties(
                source,
                vec![(
                    "storage".to_owned(),
                    "\"storage\":\"ArrayIterator\":private".to_owned(),
                    Value::Array(storage),
                )],
            ))
        }
        "arrayobject" => {
            let storage =
                spl_entries_to_debug_php_array_excluding(spl_entries(source), source.id());
            Some(ObjectRef::debug_view_with_properties(
                source,
                vec![(
                    "storage".to_owned(),
                    "\"storage\":\"ArrayObject\":private".to_owned(),
                    Value::Array(storage),
                )],
            ))
        }
        "splfixedarray" => Some(debug_info_object(
            source,
            spl_entries_to_debug_php_array_excluding(spl_entries(source), source.id()),
        )),
        "spldoublylinkedlist" | "splstack" | "splqueue" => Some(
            spl_doubly_linked_list_debug_info_object(source, &runtime_class),
        ),
        "splobjectstorage" => Some(ObjectRef::debug_view_with_properties(
            source,
            vec![(
                "storage".to_owned(),
                "\"storage\":\"SplObjectStorage\":private".to_owned(),
                Value::Array(spl_object_storage_debug_records_array(source)),
            )],
        )),
        "splheap" | "splmaxheap" | "splminheap" | "splpriorityqueue" => {
            Some(spl_heap_debug_info_object(source, &runtime_class))
        }
        "splfileinfo" | "splfileobject" | "spltempfileobject" => {
            Some(spl_file_info_debug_info_object(source))
        }
        "phar" => Some(phar_debug_info_object(source)),
        "ziparchive" => Some(zip_archive_debug_info_object(source)),
        _ => None,
    }
}

pub(super) fn hash_context_debug_info_object(source: &ObjectRef) -> Option<ObjectRef> {
    Some(debug_info_object(
        source,
        hash_context_debug_info_array(source)?,
    ))
}

pub(super) fn hash_context_debug_info_array(source: &ObjectRef) -> Option<PhpArray> {
    let Some(Value::String(algorithm)) = source.get_property(HASH_CONTEXT_ALGORITHM_PROPERTY)
    else {
        return None;
    };
    let mut properties = PhpArray::new();
    properties.insert(
        ArrayKey::String(PhpString::from_test_str("algo")),
        Value::String(algorithm),
    );
    Some(properties)
}

pub(super) fn hash_context_method_is_supported(method: &str) -> bool {
    matches!(
        normalize_method_name(method).as_str(),
        "__debuginfo" | "__serialize" | "__unserialize"
    )
}

pub(super) fn validate_hash_context_arg_count(
    method: &str,
    args: &[CallArgument],
    expected: usize,
) -> Result<(), String> {
    if args.len() == expected {
        return Ok(());
    }
    Err(format!(
        "E_PHP_VM_ARGUMENT_COUNT: HashContext::{method}() expects exactly {expected} argument{}, {} given",
        if expected == 1 { "" } else { "s" },
        args.len()
    ))
}

pub(super) fn hash_context_runtime_exception(message: impl AsRef<str>) -> String {
    format!("E_PHP_VM_SPL_RUNTIME_EXCEPTION: {}", message.as_ref())
}

pub(super) fn hash_context_object_is_initialized(object: &ObjectRef) -> bool {
    object
        .get_property(HASH_CONTEXT_ALGORITHM_PROPERTY)
        .is_some()
}

pub(super) fn hash_context_serialize_array(object: &ObjectRef) -> Result<PhpArray, String> {
    let algorithm = match object.get_property(HASH_CONTEXT_ALGORITHM_PROPERTY) {
        Some(Value::String(algorithm)) => algorithm,
        _ => {
            return Err("E_PHP_VM_INVALID_HASH_CONTEXT: invalid HashContext state".to_owned());
        }
    };
    let flags = match object.get_property(HASH_CONTEXT_FLAGS_PROPERTY) {
        Some(Value::Int(flags)) => flags,
        _ => {
            return Err("E_PHP_VM_INVALID_HASH_CONTEXT: invalid HashContext state".to_owned());
        }
    };
    if flags & HASH_HMAC_FLAG != 0 {
        return Err(hash_context_runtime_exception(
            "HashContext with HASH_HMAC option cannot be serialized",
        ));
    }
    if matches!(
        object.get_property(HASH_CONTEXT_FINALIZED_PROPERTY),
        Some(Value::Bool(true))
    ) {
        return Err(hash_context_runtime_exception(format!(
            "HashContext for algorithm \"{}\" cannot be serialized",
            algorithm.to_string_lossy()
        )));
    }

    let mut internals = PhpArray::new();
    internals.insert(
        ArrayKey::Int(0),
        object
            .get_property(HASH_CONTEXT_DATA_PROPERTY)
            .unwrap_or_else(|| Value::string(Vec::new())),
    );

    let mut payload = PhpArray::new();
    payload.insert(ArrayKey::Int(0), Value::String(algorithm));
    payload.insert(ArrayKey::Int(1), Value::Int(flags));
    payload.insert(ArrayKey::Int(2), Value::Array(internals));
    payload.insert(ArrayKey::Int(3), Value::Int(2));
    payload.insert(ArrayKey::Int(4), Value::Array(PhpArray::new()));
    Ok(payload)
}

pub(super) fn spl_file_info_debug_info_object(source: &ObjectRef) -> ObjectRef {
    let path = spl_file_path(source);
    ObjectRef::debug_view_with_properties(
        source,
        vec![
            (
                "pathName".to_owned(),
                "\"pathName\":\"SplFileInfo\":private".to_owned(),
                Value::string(path.as_bytes().to_vec()),
            ),
            (
                "fileName".to_owned(),
                "\"fileName\":\"SplFileInfo\":private".to_owned(),
                Value::string(spl_file_basename(&path).into_bytes()),
            ),
        ],
    )
}

pub(super) fn phar_debug_info_object(source: &ObjectRef) -> ObjectRef {
    ObjectRef::debug_view_with_properties(
        source,
        vec![
            (
                "pathName".to_owned(),
                "\"pathName\":\"SplFileInfo\":private".to_owned(),
                Value::string(Vec::new()),
            ),
            (
                "fileName".to_owned(),
                "\"fileName\":\"SplFileInfo\":private".to_owned(),
                Value::string(Vec::new()),
            ),
            (
                "glob".to_owned(),
                "\"glob\":\"DirectoryIterator\":private".to_owned(),
                Value::Bool(false),
            ),
            (
                "subPathName".to_owned(),
                "\"subPathName\":\"RecursiveDirectoryIterator\":private".to_owned(),
                Value::string(Vec::new()),
            ),
        ],
    )
}

pub(super) fn zip_archive_debug_info_object(source: &ObjectRef) -> ObjectRef {
    ObjectRef::debug_view_with_properties(
        source,
        vec![
            zip_archive_debug_property(source, "lastId"),
            zip_archive_debug_property(source, "status"),
            zip_archive_debug_property(source, "statusSys"),
            zip_archive_debug_property(source, "numFiles"),
            zip_archive_debug_property(source, "filename"),
            zip_archive_debug_property(source, "comment"),
        ],
    )
}

pub(super) fn zip_archive_debug_property(
    source: &ObjectRef,
    name: &str,
) -> (String, String, Value) {
    (
        name.to_owned(),
        format!("\"{name}\""),
        source.get_property(name).unwrap_or(Value::Null),
    )
}

pub(super) fn spl_doubly_linked_list_debug_info_object(
    source: &ObjectRef,
    runtime_class: &str,
) -> ObjectRef {
    ObjectRef::debug_view_with_properties(
        source,
        vec![
            (
                "flags".to_owned(),
                "\"flags\":\"SplDoublyLinkedList\":private".to_owned(),
                Value::Int(spl_doubly_linked_list_flags(source, runtime_class)),
            ),
            (
                "dllist".to_owned(),
                "\"dllist\":\"SplDoublyLinkedList\":private".to_owned(),
                Value::Array(spl_entries_to_debug_php_array_excluding(
                    spl_entries(source),
                    source.id(),
                )),
            ),
        ],
    )
}

pub(super) fn spl_heap_debug_info_object(source: &ObjectRef, runtime_class: &str) -> ObjectRef {
    let property_owner = if normalize_class_name(runtime_class) == "splpriorityqueue" {
        "SplPriorityQueue"
    } else {
        "SplHeap"
    };
    let mut properties = spl_object_user_debug_properties(source);
    properties.extend([
        (
            "flags".to_owned(),
            format!("\"flags\":\"{property_owner}\":private"),
            Value::Int(spl_heap_debug_flags(source, runtime_class)),
        ),
        (
            "isCorrupted".to_owned(),
            format!("\"isCorrupted\":\"{property_owner}\":private"),
            Value::Bool(spl_heap_is_corrupted(source)),
        ),
        (
            "heap".to_owned(),
            format!("\"heap\":\"{property_owner}\":private"),
            Value::Array(spl_entries_to_debug_php_array_excluding(
                spl_entries(source),
                source.id(),
            )),
        ),
    ]);
    ObjectRef::debug_view_with_properties(source, properties)
}

pub(super) fn spl_debug_view_value(value: Value, excluded_object_id: Option<u64>) -> Value {
    match value {
        Value::Object(object) => {
            if excluded_object_id.is_some_and(|id| id == object.id()) {
                return Value::Object(object);
            }
            spl_internal_debug_info_object(&object)
                .map(Value::Object)
                .unwrap_or(Value::Object(object))
        }
        Value::Array(array) => {
            let mut debug_array = PhpArray::new();
            for (key, value) in array.iter() {
                debug_array.insert(
                    key.clone(),
                    spl_debug_view_value(value.clone(), excluded_object_id),
                );
            }
            Value::Array(debug_array)
        }
        value => value,
    }
}

pub(super) fn emit_zip_open_empty_file_deprecation(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) {
    if !error_reporting_allows(state, php_runtime::api::PHP_E_DEPRECATED) {
        return;
    }
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_ZIP_EMPTY_FILE_DEPRECATED",
        RuntimeSeverity::Deprecation,
        "ZipArchive::open(): Using empty file as ZipArchive is deprecated",
        source_span,
        stack_trace(compiled, stack),
        None,
    );
    emit_vm_diagnostic(
        output,
        state,
        &diagnostic,
        php_runtime::api::PhpDiagnosticChannel::Deprecated,
        php_runtime::api::PHP_E_DEPRECATED,
    );
    state.diagnostics.push(diagnostic);
}
