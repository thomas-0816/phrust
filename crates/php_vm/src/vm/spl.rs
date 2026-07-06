//! Runtime implementations of the SPL classes (iterators, ArrayObject/ArrayIterator,
//! SplStack/Queue/DoublyLinkedList, SplHeap/PriorityQueue, SplFixedArray, SplObjectStorage,
//! Recursive* iterators, and related helpers), extracted from the VM module.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::result_large_err)]

use super::prelude::*;

pub(super) fn is_spl_iterator_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "arrayiterator"
            | "recursivearrayiterator"
            | "directoryiterator"
            | "filesystemiterator"
            | "recursivedirectoryiterator"
            | "iteratoriterator"
            | "limititerator"
            | "emptyiterator"
            | "appenditerator"
            | "recursiveiteratoriterator"
            | "cachingiterator"
            | "recursivecachingiterator"
            | "regexiterator"
            | "recursiveregexiterator"
            | "norewinditerator"
            | "infiniteiterator"
            | "filteriterator"
            | "recursivefilteriterator"
            | "parentiterator"
            | "recursivetreeiterator"
            | "multipleiterator"
            | "globiterator"
    )
}

pub(super) fn is_supported_spl_runtime_class(class_name: &str) -> bool {
    is_spl_interface_runtime_class(class_name)
        || is_spl_iterator_runtime_class(class_name)
        || is_spl_container_runtime_class(class_name)
        || is_spl_heap_runtime_class(class_name)
        || is_spl_file_runtime_class(class_name)
}

pub(super) fn is_spl_interface_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "traversable"
            | "iterator"
            | "iteratoraggregate"
            | "arrayaccess"
            | "countable"
            | "serializable"
            | "outeriterator"
            | "seekableiterator"
            | "recursiveiterator"
    )
}

pub(super) fn is_spl_array_access_runtime_class(class_name: &str) -> bool {
    is_spl_container_runtime_class(class_name)
        || matches!(
            normalize_class_name(class_name).as_str(),
            "arrayiterator"
                | "recursivearrayiterator"
                | "cachingiterator"
                | "recursivecachingiterator"
        )
}

pub(super) fn spl_runtime_marker(object: &ObjectRef) -> Option<String> {
    let class_name = normalize_class_name(&object.class_name());
    if is_supported_spl_runtime_class(&class_name) {
        return Some(class_name);
    }
    let marker = object.get_property(SPL_RUNTIME_CLASS_PROPERTY)?;
    let Value::String(value) = effective_value(&marker) else {
        return None;
    };
    let normalized = normalize_class_name(&value.to_string_lossy());
    is_supported_spl_runtime_class(&normalized).then_some(normalized)
}

pub(super) fn spl_bool_property(object: &ObjectRef, name: &str) -> bool {
    matches!(
        object
            .get_property(name)
            .map(|value| effective_value(&value)),
        Some(Value::Bool(true))
    )
}

pub(super) fn spl_set_bool_property(object: &ObjectRef, name: &str, value: bool) {
    object.set_property(name, Value::Bool(value));
}

pub(super) fn spl_runtime_display_name(class_name: &str) -> String {
    let normalized = normalize_class_name(class_name);
    if is_spl_iterator_runtime_class(&normalized) {
        return spl_iterator_display_name(&normalized).to_owned();
    }
    if is_spl_container_runtime_class(&normalized) {
        return spl_container_display_name(&normalized).to_owned();
    }
    if is_spl_heap_runtime_class(&normalized) {
        return spl_heap_display_name(&normalized).to_owned();
    }
    if is_spl_file_runtime_class(&normalized) {
        return spl_file_display_name(&normalized).to_owned();
    }
    class_name.to_owned()
}

pub(super) fn call_spl_runtime_method(
    object: &ObjectRef,
    class_name: &str,
    method: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
) -> Option<Result<Value, String>> {
    let normalized = normalize_class_name(class_name);
    if is_spl_iterator_runtime_class(&normalized) && spl_iterator_method_is_supported(method) {
        return Some(call_spl_iterator_method(
            object.clone(),
            method,
            args,
            runtime_context,
        ));
    }
    if is_spl_container_runtime_class(&normalized) && spl_container_method_is_supported(method) {
        return Some(call_spl_container_method(object.clone(), method, args));
    }
    if is_spl_heap_runtime_class(&normalized) && spl_heap_method_is_supported(method) {
        return Some(call_spl_heap_method(object.clone(), method, args));
    }
    if is_spl_file_runtime_class(&normalized) && spl_file_method_is_supported(method) {
        return Some(call_spl_file_method(object, method, args, runtime_context));
    }
    None
}

pub(super) fn spl_inner_iterator_delegation_target(object: &ObjectRef) -> Option<ObjectRef> {
    let class = spl_runtime_marker(object)?;
    if !is_spl_iterator_runtime_class(&class)
        || !matches!(
            class.as_str(),
            "iteratoriterator"
                | "limititerator"
                | "recursiveiteratoriterator"
                | "cachingiterator"
                | "recursivecachingiterator"
                | "regexiterator"
                | "recursiveregexiterator"
                | "norewinditerator"
                | "infiniteiterator"
                | "filteriterator"
                | "recursivefilteriterator"
                | "parentiterator"
                | "recursivetreeiterator"
        )
    {
        return None;
    }
    match object
        .get_property("__inner_iterator")
        .map(|value| effective_value(&value))
    {
        Some(Value::Object(inner)) => Some(inner),
        _ => None,
    }
}

pub(super) fn spl_limit_iterator_uses_live_inner(object: &ObjectRef) -> bool {
    spl_inner_iterator_delegation_target(object)
        .is_some_and(|inner| spl_runtime_marker(&inner).as_deref() != Some("infiniteiterator"))
}

pub(super) fn spl_caching_iterator_uses_live_inner(object: &ObjectRef) -> bool {
    spl_inner_iterator_delegation_target(object).is_some_and(|inner| {
        spl_runtime_marker(&inner).as_deref() == Some("limititerator")
            && spl_limit_iterator_uses_live_inner(&inner)
    })
}

pub(super) fn spl_delegation_target_supports_method(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    object: &ObjectRef,
    method: &str,
) -> bool {
    if let Some(class) = spl_runtime_marker(object) {
        if is_spl_iterator_runtime_class(&class) && spl_iterator_method_is_supported(method) {
            return true;
        }
        if is_spl_container_runtime_class(&class) && spl_container_method_is_supported(method) {
            return true;
        }
        if is_spl_heap_runtime_class(&class) && spl_heap_method_is_supported(method) {
            return true;
        }
        if is_spl_file_runtime_class(&class) && spl_file_method_is_supported(method) {
            return true;
        }
    }
    lookup_resolved_method_in_state(compiled, state, &object.class_name(), method, None)
        .ok()
        .flatten()
        .is_some()
}

pub(super) fn spl_runtime_parent_for_class(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class: &php_ir::module::ClassEntry,
) -> Option<String> {
    let mut parent = class.parent.as_deref().map(normalize_class_name)?;
    let mut seen = Vec::new();
    loop {
        if is_supported_spl_runtime_class(&parent) {
            return Some(parent);
        }
        if seen.iter().any(|name| name == &parent) {
            return None;
        }
        seen.push(parent.clone());
        let entry = lookup_class_in_state(compiled, state, &parent)?;
        parent = entry.parent.as_deref().map(normalize_class_name)?;
    }
}

pub(super) fn initialize_spl_runtime_subclass_storage(
    object: &ObjectRef,
    spl_class: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
    resources: Option<&mut ResourceTable>,
) -> Result<(), String> {
    if normalize_class_name(spl_class) == "appenditerator"
        && spl_bool_property(object, "__append_initialized")
    {
        return Err(
            "E_PHP_VM_SPL_BAD_METHOD_CALL: AppendIterator::getIterator() must be called exactly once per instance"
                .to_owned(),
        );
    }
    let source = if is_spl_iterator_runtime_class(spl_class) {
        new_spl_iterator_object(spl_class, args, runtime_context, resources)?
    } else if is_spl_container_runtime_class(spl_class) {
        new_spl_container_object(spl_class, args)?
    } else if is_spl_heap_runtime_class(spl_class) {
        new_spl_heap_object(spl_class, args)?
    } else if is_spl_file_runtime_class(spl_class) {
        new_spl_file_object(spl_class, args, runtime_context)?
    } else {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_PARENT_CLASS: unsupported SPL runtime parent {spl_class}"
        ));
    };
    for (name, value) in source.properties_snapshot() {
        if name.starts_with("__") {
            object.set_property(name, value);
        }
    }
    source.release_php_handle();
    object.set_property(
        SPL_RUNTIME_CLASS_PROPERTY,
        Value::string(normalize_class_name(spl_class).into_bytes()),
    );
    Ok(())
}

pub(super) fn internal_spl_iterator_instanceof(
    object_class: &str,
    target_class: &str,
) -> Option<bool> {
    if !is_spl_iterator_runtime_class(object_class) {
        return None;
    }
    let object_class = normalize_class_name(object_class);
    let target_class = normalize_class_name(target_class);
    Some(match target_class.as_str() {
        "traversable" | "iterator" => true,
        "countable" => matches!(
            object_class.as_str(),
            "arrayiterator"
                | "recursivearrayiterator"
                | "appenditerator"
                | "cachingiterator"
                | "recursivecachingiterator"
        ),
        "arrayaccess" => matches!(
            object_class.as_str(),
            "arrayiterator"
                | "recursivearrayiterator"
                | "cachingiterator"
                | "recursivecachingiterator"
        ),
        "seekableiterator" => matches!(
            object_class.as_str(),
            "arrayiterator"
                | "recursivearrayiterator"
                | "directoryiterator"
                | "filesystemiterator"
                | "recursivedirectoryiterator"
        ),
        "recursiveiterator" => matches!(
            object_class.as_str(),
            "recursivearrayiterator" | "recursivedirectoryiterator"
        ),
        "outeriterator" => matches!(
            object_class.as_str(),
            "iteratoriterator"
                | "limititerator"
                | "recursiveiteratoriterator"
                | "cachingiterator"
                | "recursivecachingiterator"
                | "regexiterator"
                | "recursiveregexiterator"
                | "norewinditerator"
                | "infiniteiterator"
                | "filteriterator"
                | "recursivefilteriterator"
                | "parentiterator"
                | "recursivetreeiterator"
        ),
        "appenditerator" => object_class == "appenditerator",
        "arrayiterator" => {
            object_class == "arrayiterator" || object_class == "recursivearrayiterator"
        }
        "recursivearrayiterator" => object_class == "recursivearrayiterator",
        "iteratoriterator" => object_class == "iteratoriterator",
        "limititerator" => object_class == "limititerator",
        "emptyiterator" => object_class == "emptyiterator",
        "recursiveiteratoriterator" => object_class == "recursiveiteratoriterator",
        "cachingiterator" => matches!(
            object_class.as_str(),
            "cachingiterator" | "recursivecachingiterator"
        ),
        "recursivecachingiterator" => object_class == "recursivecachingiterator",
        "regexiterator" => matches!(
            object_class.as_str(),
            "regexiterator" | "recursiveregexiterator"
        ),
        "recursiveregexiterator" => object_class == "recursiveregexiterator",
        "norewinditerator" => object_class == "norewinditerator",
        "infiniteiterator" => object_class == "infiniteiterator",
        "filteriterator" => matches!(
            object_class.as_str(),
            "filteriterator" | "recursivefilteriterator" | "parentiterator"
        ),
        "recursivefilteriterator" => object_class == "recursivefilteriterator",
        "parentiterator" => object_class == "parentiterator",
        "recursivetreeiterator" => object_class == "recursivetreeiterator",
        "multipleiterator" => object_class == "multipleiterator",
        "globiterator" => object_class == "globiterator",
        "splfileinfo" => matches!(
            object_class.as_str(),
            "directoryiterator" | "filesystemiterator" | "recursivedirectoryiterator"
        ),
        "directoryiterator" => matches!(
            object_class.as_str(),
            "directoryiterator" | "filesystemiterator" | "recursivedirectoryiterator"
        ),
        "filesystemiterator" => matches!(
            object_class.as_str(),
            "filesystemiterator" | "recursivedirectoryiterator"
        ),
        "recursivedirectoryiterator" => object_class == "recursivedirectoryiterator",
        _ => false,
    })
}

pub(super) fn new_spl_iterator_object(
    class_name: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
    resources: Option<&mut ResourceTable>,
) -> Result<ObjectRef, String> {
    if let Some(name) = args.iter().find_map(|arg| arg.name.as_deref()) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_NAMED_ARG: {class_name}::__construct has no builtin parameter ${name}"
        ));
    }
    let normalized = normalize_class_name(class_name);
    let original_args = args.clone();
    let mut entry_depths = None;
    let mut entry_sub_iterators = None;
    let mut entry_hook_iterators = None;
    let mut recursive_iterator_mode = None;
    let entries = match normalized.as_str() {
        "emptyiterator" => {
            validate_spl_constructor_arg_count(class_name, &args, 0, 0)?;
            Vec::new()
        }
        "arrayiterator" | "recursivearrayiterator" => {
            validate_spl_constructor_arg_count(class_name, &args, 0, 1)?;
            args.first()
                .map(|arg| spl_entries_from_value(&arg.value))
                .transpose()?
                .unwrap_or_default()
        }
        "directoryiterator" | "filesystemiterator" | "recursivedirectoryiterator" => {
            validate_spl_constructor_arg_count(
                class_name,
                &args,
                1,
                if normalized == "directoryiterator" {
                    1
                } else {
                    2
                },
            )?;
            let directory = to_string(&args[0].value)?.to_string_lossy();
            let default_flags = if normalized == "filesystemiterator" {
                SPL_FILESYSTEM_KEY_AS_PATHNAME
                    | SPL_FILESYSTEM_CURRENT_AS_FILEINFO
                    | SPL_FILESYSTEM_SKIP_DOTS
            } else {
                SPL_FILESYSTEM_KEY_AS_PATHNAME | SPL_FILESYSTEM_CURRENT_AS_FILEINFO
            };
            let flags = args
                .get(1)
                .map(|arg| to_int(&arg.value))
                .transpose()?
                .unwrap_or(default_flags);
            spl_directory_entries(&normalized, &directory, flags, runtime_context)?
        }
        "iteratoriterator" => {
            validate_spl_constructor_arg_count(class_name, &args, 1, 2)?;
            spl_entries_from_value(&args[0].value)?
        }
        "infiniteiterator"
        | "filteriterator"
        | "recursivefilteriterator"
        | "parentiterator"
        | "norewinditerator" => {
            validate_spl_constructor_arg_count(class_name, &args, 1, 1)?;
            spl_entries_from_value(&args[0].value)?
        }
        "cachingiterator" | "recursivecachingiterator" => {
            validate_spl_constructor_arg_count(class_name, &args, 1, 2)?;
            validate_spl_caching_iterator_flags(class_name, &args)?;
            spl_entries_from_value(&args[0].value)?
        }
        "recursiveiteratoriterator" | "recursivetreeiterator" => {
            validate_spl_constructor_arg_count(class_name, &args, 1, 4)?;
            if normalized == "recursivetreeiterator" {
                validate_recursive_tree_iterator_source(&args[0].value)?;
            }
            if matches!(
                effective_value(&args[0].value),
                Value::Object(object)
                    if spl_runtime_marker(&object).as_deref() == Some("arrayiterator")
            ) {
                return Err(
                    "E_PHP_VM_SPL_INVALID_ARGUMENT: An instance of RecursiveIterator or IteratorAggregate creating it is required"
                        .to_owned(),
                );
            }
            let mode = if normalized == "recursivetreeiterator" {
                args.get(3)
                    .map(|arg| to_int(&arg.value))
                    .transpose()?
                    .unwrap_or(SPL_RII_SELF_FIRST)
            } else {
                args.get(1)
                    .map(|arg| to_int(&arg.value))
                    .transpose()?
                    .unwrap_or(SPL_RII_LEAVES_ONLY)
            };
            recursive_iterator_mode = Some(mode);
            match effective_value(&args[0].value) {
                Value::Object(object)
                    if spl_runtime_marker(&object).as_deref()
                        == Some("recursivedirectoryiterator") =>
                {
                    spl_recursive_directory_entries(&object, runtime_context)?
                }
                Value::Object(object)
                    if spl_runtime_marker(&object).as_deref() == Some("parentiterator") =>
                {
                    let recursive_entries = spl_parent_iterator_recursive_entries(&object, mode)?;
                    let mut depths = Vec::with_capacity(recursive_entries.len());
                    let mut sub_iterators = Vec::with_capacity(recursive_entries.len());
                    let mut hook_iterators_vec = Vec::with_capacity(recursive_entries.len());
                    let mut entries = Vec::with_capacity(recursive_entries.len());
                    for entry in recursive_entries {
                        let SplRecursiveEntry {
                            key,
                            value,
                            depth,
                            iterators,
                            hook_iterators,
                        } = entry;
                        entries.push((key, value));
                        depths.push(depth);
                        sub_iterators.push(iterators);
                        hook_iterators_vec.push(hook_iterators);
                    }
                    entry_depths = Some(depths);
                    entry_sub_iterators = Some(sub_iterators);
                    entry_hook_iterators = Some(hook_iterators_vec);
                    entries
                }
                Value::Object(object)
                    if spl_runtime_marker(&object).as_deref()
                        == Some("recursivecachingiterator") =>
                {
                    let recursive_entries = spl_recursive_caching_entries_with_context_from_object(
                        &object,
                        0,
                        Vec::new(),
                    )?;
                    let mut depths = Vec::with_capacity(recursive_entries.len());
                    let mut sub_iterators = Vec::with_capacity(recursive_entries.len());
                    let mut hook_iterators_vec = Vec::with_capacity(recursive_entries.len());
                    let mut entries = Vec::with_capacity(recursive_entries.len());
                    for entry in recursive_entries {
                        let SplRecursiveEntry {
                            key,
                            value,
                            depth,
                            iterators,
                            hook_iterators,
                        } = entry;
                        entries.push((key, value));
                        depths.push(depth);
                        sub_iterators.push(iterators);
                        hook_iterators_vec.push(hook_iterators);
                    }
                    entry_depths = Some(depths);
                    entry_sub_iterators = Some(sub_iterators);
                    entry_hook_iterators = Some(hook_iterators_vec);
                    entries
                }
                _ => {
                    let recursive_entries = if normalized == "recursivetreeiterator" {
                        spl_recursive_tree_entries_with_context_from_value(
                            &args[0].value,
                            0,
                            Vec::new(),
                        )?
                    } else {
                        spl_recursive_entries_with_context_from_value_and_mode(
                            &args[0].value,
                            0,
                            Vec::new(),
                            mode,
                        )?
                    };
                    let mut depths = Vec::with_capacity(recursive_entries.len());
                    let mut sub_iterators = Vec::with_capacity(recursive_entries.len());
                    let mut hook_iterators_vec = Vec::with_capacity(recursive_entries.len());
                    let mut entries = Vec::with_capacity(recursive_entries.len());
                    for entry in recursive_entries {
                        let SplRecursiveEntry {
                            key,
                            value,
                            depth,
                            iterators,
                            hook_iterators,
                        } = entry;
                        entries.push((key, value));
                        depths.push(depth);
                        sub_iterators.push(iterators);
                        hook_iterators_vec.push(hook_iterators);
                    }
                    entry_depths = Some(depths);
                    entry_sub_iterators = Some(sub_iterators);
                    entry_hook_iterators = Some(hook_iterators_vec);
                    entries
                }
            }
        }
        "regexiterator" | "recursiveregexiterator" => {
            validate_spl_constructor_arg_count(class_name, &args, 2, 5)?;
            let entries = if normalized == "recursiveregexiterator" {
                spl_recursive_entries_from_value(&args[0].value)?
            } else {
                spl_entries_from_value(&args[0].value)?
            };
            let _pattern = to_string(&args[1].value)?.to_string_lossy();
            let mode = args
                .get(2)
                .map(|arg| to_int(&arg.value))
                .transpose()?
                .unwrap_or(SPL_REGEX_MATCH);
            if !(SPL_REGEX_MATCH..=SPL_REGEX_REPLACE).contains(&mode) {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: RegexIterator::__construct(): Argument #3 ($mode) must be RegexIterator::MATCH, RegexIterator::GET_MATCH, RegexIterator::ALL_MATCHES, RegexIterator::SPLIT, or RegexIterator::REPLACE"
                        .to_owned(),
                );
            }
            let _flags = args
                .get(3)
                .map(|arg| to_int(&arg.value))
                .transpose()?
                .unwrap_or(0);
            let _preg_flags = args
                .get(4)
                .map(|arg| to_int(&arg.value))
                .transpose()?
                .unwrap_or(0);
            entries
        }
        "limititerator" => {
            validate_spl_constructor_arg_count(class_name, &args, 1, 3)?;
            let entries = spl_entries_from_value(&args[0].value)?;
            let offset = args
                .get(1)
                .map(|arg| to_int(&arg.value))
                .transpose()?
                .unwrap_or(0);
            if offset < 0 {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: LimitIterator::__construct(): Argument #2 ($offset) must be greater than or equal to 0"
                        .to_owned(),
                );
            }
            let count = args.get(2).map(|arg| to_int(&arg.value)).transpose()?;
            if let Some(count) = count
                && count < -1
            {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: LimitIterator::__construct(): Argument #3 ($limit) must be greater than or equal to -1"
                        .to_owned(),
                );
            }
            if matches!(
                effective_value(&args[0].value),
                Value::Object(object)
                    if spl_runtime_marker(&object).as_deref() == Some("infiniteiterator")
            ) && let Some(count) = count
                && count >= 0
            {
                if entries.is_empty() {
                    Vec::new()
                } else {
                    (0..count as usize)
                        .map(|index| entries[(offset as usize + index) % entries.len()].clone())
                        .collect()
                }
            } else {
                let source_len = entries.len();
                if offset as usize > source_len {
                    return Err(format!(
                        "E_PHP_VM_SPL_OUT_OF_BOUNDS: Seek position {offset} is out of range"
                    ));
                }
                let iter = entries.into_iter().skip(offset as usize);
                match count {
                    Some(count) if count >= 0 => iter.take(count as usize).collect(),
                    _ => iter.collect(),
                }
            }
        }
        "appenditerator" => {
            validate_spl_constructor_arg_count(class_name, &args, 0, 0)?;
            Vec::new()
        }
        "multipleiterator" => {
            validate_spl_constructor_arg_count(class_name, &args, 0, 1)?;
            Vec::new()
        }
        "globiterator" => {
            validate_spl_constructor_arg_count(class_name, &args, 1, 2)?;
            let pattern = to_string(&args[0].value)?.to_string_lossy();
            if let Some(resources) = resources {
                resources.register_internal_glob(pattern.clone());
            }
            spl_glob_entries(&pattern, runtime_context)?
        }
        _ => unreachable!("is_spl_iterator_runtime_class validates class names"),
    };
    let object = ObjectRef::new_with_display_name(
        &spl_iterator_class(class_name),
        spl_iterator_display_name(class_name),
    );
    spl_set_entries(&object, entries);
    if let Some(depths) = entry_depths {
        spl_set_entry_depths(&object, depths);
    }
    if let Some(sub_iterators) = entry_sub_iterators {
        spl_set_sub_iterators(&object, sub_iterators);
    }
    if let Some(hook_iterators) = entry_hook_iterators {
        spl_set_hook_iterators(&object, hook_iterators);
    }
    object.set_property(
        SPL_RUNTIME_CLASS_PROPERTY,
        Value::string(normalized.clone().into_bytes()),
    );
    spl_set_position(&object, 0);
    if let Some(inner) = args.first() {
        object.set_property("__inner_iterator", effective_value(&inner.value));
    }
    if let Some(mode) = recursive_iterator_mode {
        object.set_property("__rii_mode", Value::Int(mode));
        let flags = original_args
            .get(2)
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(0);
        object.set_property("__rii_flags", Value::Int(flags));
        object.set_property(
            "__rii_entered_child_positions",
            Value::Array(PhpArray::new()),
        );
        object.set_property(
            "__rii_checked_child_positions",
            Value::Array(PhpArray::new()),
        );
    }
    if normalized == "recursivetreeiterator" {
        let flags = original_args
            .get(1)
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(SPL_RTI_BYPASS_KEY);
        object.set_property("__rti_flags", Value::Int(flags));
        object.set_property("__rti_prefix_parts", spl_rti_default_prefix_parts_value());
    }
    if normalized == "recursiveiteratoriterator"
        && normalize_class_name(class_name) == "recursiveiteratoriterator"
        && spl_rii_should_use_direct_root(&object)
    {
        object.set_property("__rii_direct_at_root", Value::Bool(true));
        object.set_property("__rii_direct_root_consumed", Value::Bool(false));
    }
    if matches!(
        normalized.as_str(),
        "directoryiterator" | "filesystemiterator" | "recursivedirectoryiterator"
    ) {
        let directory = to_string(&original_args[0].value)?.to_string_lossy();
        let default_flags = if normalized == "filesystemiterator" {
            SPL_FILESYSTEM_KEY_AS_PATHNAME
                | SPL_FILESYSTEM_CURRENT_AS_FILEINFO
                | SPL_FILESYSTEM_SKIP_DOTS
        } else {
            SPL_FILESYSTEM_KEY_AS_PATHNAME | SPL_FILESYSTEM_CURRENT_AS_FILEINFO
        };
        let flags = original_args
            .get(1)
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(default_flags);
        object.set_property("__directory", Value::string(directory.into_bytes()));
        object.set_property("__flags", Value::Int(flags));
    }
    if matches!(
        normalized.as_str(),
        "regexiterator" | "recursiveregexiterator"
    ) {
        object.set_property("__regex", original_args[1].value.clone());
        object.set_property("__regex_accept_pre_parent", Value::Bool(false));
        let mode = original_args
            .get(2)
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(0);
        let flags = original_args
            .get(3)
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(0);
        let preg_flags = original_args
            .get(4)
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(0);
        object.set_property("__regex_mode", Value::Int(mode));
        object.set_property("__regex_flags", Value::Int(flags));
        object.set_property("__regex_preg_flags", Value::Int(preg_flags));
    }
    if matches!(
        normalized.as_str(),
        "cachingiterator" | "recursivecachingiterator"
    ) {
        let flags = original_args
            .get(1)
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(1);
        object.set_property("__caching_flags", Value::Int(flags));
        object.set_property("__caching_seen_count", Value::Int(0));
        object.set_property("__caching_cache", Value::Array(PhpArray::new()));
    }
    if normalized == "limititerator" {
        let offset = original_args
            .get(1)
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(0);
        let count = original_args
            .get(2)
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(-1);
        object.set_property("__limit_offset", Value::Int(offset));
        object.set_property("__limit_count", Value::Int(count));
    }
    if normalized == "appenditerator" {
        object.set_property("__append_iterators", Value::Array(PhpArray::new()));
        object.set_property(
            "__append_entry_iterator_indices",
            Value::Array(PhpArray::new()),
        );
        spl_set_bool_property(&object, "__append_initialized", true);
    }
    if normalized == "multipleiterator" {
        let flags = original_args
            .first()
            .map(|arg| to_int(&arg.value))
            .transpose()?
            .unwrap_or(SPL_MULTIPLE_ITERATOR_NEED_ALL | SPL_MULTIPLE_ITERATOR_KEYS_NUMERIC);
        object.set_property("__regex_flags", Value::Int(flags));
        object.set_property("__attached_iterators", Value::Array(PhpArray::new()));
        object.set_property("__attached_iterator_ids", Value::Array(PhpArray::new()));
        object.set_property("__iterator_count", Value::Int(0));
    }
    Ok(object)
}

pub(super) fn spl_iterator_method_is_supported(method: &str) -> bool {
    matches!(
        normalize_method_name(method).as_str(),
        "rewind"
            | "valid"
            | "current"
            | "key"
            | "next"
            | "getpathname"
            | "getfilename"
            | "getbasename"
            | "getextension"
            | "getpath"
            | "getpathinfo"
            | "getrealpath"
            | "getsize"
            | "getmtime"
            | "isfile"
            | "isdir"
            | "islink"
            | "isreadable"
            | "isdot"
            | "count"
            | "getarraycopy"
            | "getinneriterator"
            | "getsubiterator"
            | "getregex"
            | "getmode"
            | "setmode"
            | "getflags"
            | "setflags"
            | "getpregflags"
            | "setpregflags"
            | "accept"
            | "haschildren"
            | "getchildren"
            | "getsubpath"
            | "getsubpathname"
            | "getcache"
            | "seek"
            | "getposition"
            | "getdepth"
            | "getmaxdepth"
            | "setmaxdepth"
            | "callhaschildren"
            | "callgetchildren"
            | "beginchildren"
            | "endchildren"
            | "beginiteration"
            | "enditeration"
            | "nextelement"
            | "__construct"
            | "hasnext"
            | "__tostring"
            | "offsetget"
            | "offsetexists"
            | "offsetset"
            | "offsetunset"
            | "append"
            | "additerator"
            | "attachiterator"
            | "containsiterator"
            | "detachiterator"
            | "countiterators"
            | "getarrayiterator"
            | "getiteratorindex"
            | "getprefix"
            | "getpostfix"
            | "setpostfix"
            | "getentry"
            | "setprefixpart"
    )
}

pub(super) fn call_spl_iterator_method(
    object: ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    let class_name = object.class_name();
    let runtime_class_name =
        spl_runtime_marker(&object).unwrap_or_else(|| normalize_class_name(&class_name));
    let method = normalize_method_name(method);
    match method.as_str() {
        "rewind" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if runtime_class_name == "multipleiterator" {
                for (iterator, _) in spl_multiple_iterator_records(&object) {
                    spl_set_position(&iterator, 0);
                }
                spl_set_position(&object, 0);
                return Ok(Value::Null);
            }
            if runtime_class_name != "norewinditerator" {
                spl_set_position(&object, 0);
            }
            if is_spl_caching_iterator_class(&runtime_class_name) {
                object.set_property("__caching_seen_count", Value::Int(0));
                object.set_property("__caching_cache", Value::Array(PhpArray::new()));
                spl_caching_iterator_note_current_seen(&object);
            }
            Ok(Value::Null)
        }
        "valid" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if runtime_class_name == "multipleiterator" {
                return Ok(Value::Bool(spl_multiple_iterator_is_valid(&object)));
            }
            if matches!(
                runtime_class_name.as_str(),
                "regexiterator" | "recursiveregexiterator"
            ) {
                while let Some((key, value)) = spl_current_entry(&object) {
                    if spl_regex_current_value(&object, &key, value).0 {
                        return Ok(Value::Bool(true));
                    }
                    spl_set_position(&object, spl_position(&object).saturating_add(1));
                }
                return Ok(Value::Bool(false));
            }
            if matches!(
                runtime_class_name.as_str(),
                "recursiveiteratoriterator" | "recursivetreeiterator"
            ) {
                while spl_position(&object) < spl_entries(&object).len() {
                    let position = spl_position(&object);
                    let max_depth = spl_rii_max_depth(&object);
                    let depth = spl_entry_depths(&object)
                        .get(position)
                        .copied()
                        .unwrap_or(0);
                    if max_depth < 0 || depth <= max_depth {
                        return Ok(Value::Bool(true));
                    }
                    spl_set_position(&object, position.saturating_add(1));
                }
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(
                spl_position(&object) < spl_entries(&object).len(),
            ))
        }
        "current" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if runtime_class_name == "multipleiterator" {
                return spl_multiple_iterator_current(&object);
            }
            if runtime_class_name == "emptyiterator" {
                return Err(
                    "E_PHP_VM_SPL_BAD_METHOD_CALL: Accessing the value of an EmptyIterator"
                        .to_owned(),
                );
            }
            if runtime_class_name == "recursiveiteratoriterator" && spl_rii_direct_at_root(&object)
            {
                object.set_property("__rii_direct_root_consumed", Value::Bool(true));
                return Ok(spl_rii_root_current(&object).unwrap_or(Value::Null));
            }
            if runtime_class_name == "recursiveiteratoriterator"
                && let Some((_, value)) = spl_rii_active_call_get_children_current_entry(&object)
            {
                return Ok(value);
            }
            if runtime_class_name == "recursiveiteratoriterator"
                && let Some((_, value)) = spl_rii_pruned_parent_current_entry(&object)
            {
                return Ok(value);
            }
            if matches!(
                runtime_class_name.as_str(),
                "regexiterator" | "recursiveregexiterator"
            ) {
                if spl_regex_accept_pre_parent(&object) {
                    return Ok(spl_current_entry(&object)
                        .map(|(_, value)| value)
                        .unwrap_or(Value::Null));
                }
                if spl_regex_last_accept_rejected(&object)
                    && !spl_regex_uses_key(&object)
                    && let Some((_, value)) = spl_current_entry(&object)
                    && matches!(effective_value(&value), Value::Array(_))
                {
                    return Ok(value);
                }
                return Ok(spl_current_entry(&object)
                    .map(|(key, value)| spl_regex_current_value(&object, &key, value).1)
                    .unwrap_or(Value::Null));
            }
            let entry = spl_current_entry(&object);
            if entry.is_some() && is_spl_caching_iterator_class(&runtime_class_name) {
                spl_caching_iterator_note_current_seen(&object);
            }
            if runtime_class_name == "recursivetreeiterator" {
                return entry
                    .map(|(_, value)| {
                        if spl_rti_flags(&object) & SPL_RTI_BYPASS_CURRENT != 0 {
                            return Ok(value);
                        }
                        Ok(Value::string(
                            format!(
                                "{}{}{}",
                                spl_rti_prefix(&object),
                                spl_rti_entry_text(&value)?,
                                spl_rti_postfix(&object)
                            )
                            .into_bytes(),
                        ))
                    })
                    .transpose()
                    .map(|value| value.unwrap_or(Value::Null));
            }
            Ok(entry.map(|(_, value)| value).unwrap_or(Value::Null))
        }
        "key" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if runtime_class_name == "multipleiterator" {
                return spl_multiple_iterator_key(&object);
            }
            if runtime_class_name == "emptyiterator" {
                return Err(
                    "E_PHP_VM_SPL_BAD_METHOD_CALL: Accessing the key of an EmptyIterator"
                        .to_owned(),
                );
            }
            if runtime_class_name == "recursiveiteratoriterator" && spl_rii_direct_at_root(&object)
            {
                object.set_property("__rii_direct_root_consumed", Value::Bool(true));
                return Ok(spl_rii_root_key(&object).unwrap_or(Value::Null));
            }
            if runtime_class_name == "recursiveiteratoriterator"
                && let Some((key, _)) = spl_rii_active_call_get_children_current_entry(&object)
            {
                return Ok(array_key_to_value(key));
            }
            if runtime_class_name == "recursiveiteratoriterator"
                && let Some((key, _)) = spl_rii_pruned_parent_current_entry(&object)
            {
                return Ok(array_key_to_value(key));
            }
            if runtime_class_name == "recursivetreeiterator" {
                return spl_current_entry(&object)
                    .map(|(key, _)| {
                        let key = array_key_to_value(key);
                        if spl_rti_flags(&object) & SPL_RTI_BYPASS_KEY != 0 {
                            return Ok(key);
                        }
                        Ok(Value::string(
                            format!(
                                "{}{}",
                                spl_rti_prefix(&object),
                                to_string(&key)?.to_string_lossy()
                            )
                            .into_bytes(),
                        ))
                    })
                    .transpose()
                    .map(|value| value.unwrap_or(Value::Null));
            }
            Ok(spl_current_entry(&object)
                .map(|(key, _)| array_key_to_value(key))
                .unwrap_or(Value::Null))
        }
        "next" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if runtime_class_name == "multipleiterator" {
                for (iterator, _) in spl_multiple_iterator_records(&object) {
                    spl_set_position(&iterator, spl_position(&iterator).saturating_add(1));
                }
                spl_set_position(&object, spl_position(&object).saturating_add(1));
                return Ok(Value::Null);
            }
            if runtime_class_name == "recursiveiteratoriterator" && spl_rii_direct_at_root(&object)
            {
                let consumed_root = spl_rii_direct_root_consumed(&object);
                object.set_property("__rii_direct_at_root", Value::Bool(false));
                object.set_property("__rii_direct_root_consumed", Value::Bool(false));
                if consumed_root {
                    spl_set_position(&object, 0);
                } else {
                    spl_set_position(&object, spl_position(&object).saturating_add(1));
                }
                return Ok(Value::Null);
            }
            spl_set_position(&object, spl_position(&object).saturating_add(1));
            Ok(Value::Null)
        }
        "getpathname" | "getfilename" | "getbasename" | "getextension" | "getpath"
        | "getpathinfo" | "getrealpath" | "getsize" | "getmtime" | "isfile" | "isdir"
        | "islink" | "isreadable" => {
            validate_spl_iterator_arg_count(
                &class_name,
                &args,
                0,
                if method == "getbasename" || method == "getpathinfo" {
                    1
                } else {
                    0
                },
            )?;
            let path = spl_directory_current_path(&object).unwrap_or_default();
            let file = spl_file_info_object(&path);
            call_spl_file_method(&file, method.as_str(), args, runtime_context)
        }
        "isdot" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Bool(
                spl_directory_current_path(&object)
                    .map(|path| spl_directory_is_dot_text(&path))
                    .unwrap_or(false),
            ))
        }
        "count" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if matches!(
                runtime_class_name.as_str(),
                "cachingiterator" | "recursivecachingiterator"
            ) && !spl_caching_iterator_uses_full_cache(&object)
            {
                return Err(format!(
                    "E_PHP_VM_SPL_BAD_METHOD_CALL: {} does not use a full cache (see CachingIterator::__construct)",
                    object.display_name()
                ));
            }
            if is_spl_caching_iterator_class(&runtime_class_name) {
                Ok(Value::Int(spl_caching_iterator_cache(&object).len() as i64))
            } else {
                Ok(Value::Int(spl_entries(&object).len() as i64))
            }
        }
        "getarraycopy" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Array(spl_entries_to_php_array(spl_entries(&object))))
        }
        "getinneriterator" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 1)?;
            if matches!(
                runtime_class_name.as_str(),
                "recursiveiteratoriterator" | "recursivetreeiterator"
            ) && object
                .get_property("__inner_iterator")
                .map(|value| effective_value(&value))
                .is_some_and(|value| {
                    matches!(
                        value,
                        Value::Object(inner)
                            if spl_runtime_marker(&inner).as_deref()
                                == Some("recursivecachingiterator")
                    )
                })
                && let Some(iterator) = spl_current_sub_iterator(&object)
            {
                return Ok(Value::Object(iterator));
            }
            Ok(object
                .get_property("__inner_iterator")
                .map(|value| effective_value(&value))
                .unwrap_or(Value::Null))
        }
        "getsubiterator" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 1)?;
            if !matches!(
                runtime_class_name.as_str(),
                "recursiveiteratoriterator" | "recursivetreeiterator"
            ) {
                return Ok(object
                    .get_property("__inner_iterator")
                    .map(|value| effective_value(&value))
                    .unwrap_or(Value::Null));
            }
            let position = spl_position(&object);
            let sub_iterators = spl_sub_iterators(&object);
            if sub_iterators.is_empty() {
                return Ok(Value::Null);
            }
            let context_position = position.min(sub_iterators.len().saturating_sub(1));
            let Some(iterators) = sub_iterators.into_iter().nth(context_position) else {
                return Ok(Value::Null);
            };
            let level = if let Some(arg) = args.first() {
                let level = to_int(&arg.value)?;
                if level < 0 {
                    return Ok(Value::Null);
                }
                level as usize
            } else {
                let depth = spl_entry_depths(&object)
                    .get(context_position)
                    .copied()
                    .unwrap_or(0)
                    .max(0) as usize;
                let at_first_child_entry = if depth > 0 {
                    match (
                        spl_entries(&object).into_iter().nth(context_position),
                        iterators.get(depth),
                    ) {
                        (Some((current_key, _)), Some(child)) => spl_entries(child)
                            .first()
                            .is_some_and(|(first_key, _)| first_key == &current_key),
                        _ => false,
                    }
                } else {
                    false
                };
                if at_first_child_entry {
                    depth - 1
                } else {
                    depth
                }
            };
            if level == 0
                && let Some(Value::Object(inner)) = object
                    .get_property("__inner_iterator")
                    .map(|value| effective_value(&value))
            {
                if let Some(root) = iterators.first() {
                    spl_set_position(&inner, spl_position(root));
                }
                return Ok(Value::Object(inner));
            }
            Ok(iterators
                .get(level)
                .cloned()
                .map(Value::Object)
                .unwrap_or(Value::Null))
        }
        "getregex" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(object
                .get_property("__regex")
                .map(|value| effective_value(&value))
                .unwrap_or_else(|| Value::string(Vec::new())))
        }
        "getmode" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(object
                .get_property("__regex_mode")
                .map(|value| effective_value(&value))
                .unwrap_or(Value::Int(0)))
        }
        "setmode" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            let mode = to_int(&args[0].value)?;
            if matches!(
                runtime_class_name.as_str(),
                "regexiterator" | "recursiveregexiterator"
            ) && !(0..=4).contains(&mode)
            {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: RegexIterator::setMode(): Argument #1 ($mode) must be RegexIterator::MATCH, RegexIterator::GET_MATCH, RegexIterator::ALL_MATCHES, RegexIterator::SPLIT, or RegexIterator::REPLACE"
                        .to_owned(),
                );
            }
            object.set_property("__regex_mode", Value::Int(mode));
            Ok(Value::Null)
        }
        "getflags" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if matches!(
                runtime_class_name.as_str(),
                "filesystemiterator" | "recursivedirectoryiterator"
            ) {
                return Ok(Value::Int(spl_filesystem_flags(&object)));
            }
            if is_spl_caching_iterator_class(&runtime_class_name) {
                return Ok(Value::Int(spl_caching_iterator_flags(&object)));
            }
            Ok(object
                .get_property("__regex_flags")
                .map(|value| effective_value(&value))
                .unwrap_or(if runtime_class_name == "multipleiterator" {
                    Value::Int(SPL_MULTIPLE_ITERATOR_NEED_ALL | SPL_MULTIPLE_ITERATOR_KEYS_NUMERIC)
                } else {
                    Value::Int(0)
                }))
        }
        "setflags" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            let flags = to_int(&args[0].value)?;
            if matches!(
                runtime_class_name.as_str(),
                "filesystemiterator" | "recursivedirectoryiterator"
            ) {
                let directory = object
                    .get_property("__directory")
                    .and_then(|value| match effective_value(&value) {
                        Value::String(directory) => Some(directory.to_string_lossy()),
                        _ => None,
                    })
                    .unwrap_or_default();
                spl_set_entries(
                    &object,
                    spl_directory_entries(&runtime_class_name, &directory, flags, runtime_context)?,
                );
                spl_set_position(&object, 0);
                object.set_property("__flags", Value::Int(flags));
            } else if is_spl_caching_iterator_class(&runtime_class_name) {
                validate_spl_caching_string_flags(
                    &class_name,
                    "setFlags",
                    "Argument #1 ($flags)",
                    flags,
                )?;
                let old_flags = spl_caching_iterator_flags(&object);
                if old_flags & SPL_CACHING_CALL_TOSTRING != 0
                    && flags & SPL_CACHING_CALL_TOSTRING == 0
                {
                    return Err(
                        "E_PHP_VM_SPL_BAD_METHOD_CALL: Unsetting flag CALL_TO_STRING is not possible"
                            .to_owned(),
                    );
                }
                if old_flags & SPL_CACHING_TOSTRING_USE_INNER != 0
                    && flags & SPL_CACHING_TOSTRING_USE_INNER == 0
                {
                    return Err(
                        "E_PHP_VM_SPL_BAD_METHOD_CALL: Unsetting flag TOSTRING_USE_INNER is not possible"
                            .to_owned(),
                    );
                }
                object.set_property("__caching_flags", Value::Int(flags));
            } else if runtime_class_name == "multipleiterator" {
                let valid_flags = SPL_MULTIPLE_ITERATOR_NEED_ALL
                    | SPL_MULTIPLE_ITERATOR_KEYS_ASSOC
                    | SPL_MULTIPLE_ITERATOR_KEYS_NUMERIC
                    | SPL_MULTIPLE_ITERATOR_NEED_ANY;
                if flags & !valid_flags != 0 {
                    return Err(
                        "E_PHP_VM_SPL_VALUE_ERROR: MultipleIterator::setFlags(): invalid flags"
                            .to_owned(),
                    );
                }
                object.set_property("__regex_flags", Value::Int(flags));
            } else {
                object.set_property("__regex_flags", Value::Int(flags));
            }
            Ok(Value::Null)
        }
        "getpregflags" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(object
                .get_property("__regex_preg_flags")
                .map(|value| effective_value(&value))
                .unwrap_or(Value::Int(0)))
        }
        "setpregflags" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            object.set_property("__regex_preg_flags", Value::Int(to_int(&args[0].value)?));
            Ok(Value::Null)
        }
        "accept" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if matches!(
                runtime_class_name.as_str(),
                "regexiterator" | "recursiveregexiterator"
            ) {
                object.set_property("__regex_accept_pre_parent", Value::Bool(false));
                let accepted = spl_current_entry(&object)
                    .is_some_and(|(key, value)| spl_regex_current_value(&object, &key, value).0);
                object.set_property("__regex_last_accept_result", Value::Bool(accepted));
                return Ok(Value::Bool(accepted));
            }
            Ok(Value::Bool(true))
        }
        "haschildren" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 1)?;
            if runtime_class_name == "recursivearrayiterator" {
                return Ok(Value::Bool(matches!(
                    spl_current_entry(&object).map(|(_, value)| effective_value(&value)),
                    Some(Value::Array(_) | Value::Object(_))
                )));
            }
            if runtime_class_name == "recursivefilteriterator"
                && let Some(inner) = spl_inner_iterator_delegation_target(&object)
            {
                return call_spl_iterator_method(inner, "hasChildren", Vec::new(), runtime_context);
            }
            if runtime_class_name != "recursivedirectoryiterator" {
                return Ok(Value::Bool(false));
            }
            let allow_links = args
                .first()
                .map(|arg| to_bool(&arg.value))
                .transpose()?
                .unwrap_or(false);
            let Some(path_text) = spl_current_entry(&object)
                .and_then(|(_, value)| spl_directory_path_from_value(&value))
            else {
                return Ok(Value::Bool(false));
            };
            let path = spl_file_resolve_path(&path_text, runtime_context);
            if spl_directory_is_dot_text(&path_text) {
                return Ok(Value::Bool(false));
            }
            let metadata = if allow_links {
                fs::metadata(path)
            } else {
                fs::symlink_metadata(path)
            };
            Ok(Value::Bool(
                metadata.map(|metadata| metadata.is_dir()).unwrap_or(false),
            ))
        }
        "getchildren" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if runtime_class_name == "recursivearrayiterator" {
                let Some((_, value)) = spl_current_entry(&object) else {
                    return Ok(Value::Null);
                };
                if !matches!(effective_value(&value), Value::Array(_) | Value::Object(_)) {
                    return Err(format!(
                        "E_PHP_VM_SPL_TYPE_ERROR: ArrayIterator::__construct(): Argument #1 ($array) must be of type array, {} given",
                        type_error_value_name(&value)
                    ));
                }
                let child = object.clone_shallow();
                spl_set_entries(&child, spl_entries_from_value(&value)?);
                spl_set_position(&child, 0);
                child.set_property(
                    SPL_RUNTIME_CLASS_PROPERTY,
                    Value::string(b"recursivearrayiterator".to_vec()),
                );
                return Ok(Value::Object(child));
            }
            if runtime_class_name == "recursivefilteriterator"
                && let Some(inner) = spl_inner_iterator_delegation_target(&object)
            {
                let child_inner =
                    call_spl_iterator_method(inner, "getChildren", Vec::new(), runtime_context)?;
                if let Value::Object(child_inner) = effective_value(&child_inner) {
                    let child = object.clone_shallow();
                    spl_set_entries(&child, spl_entries(&child_inner));
                    spl_set_position(&child, 0);
                    child.set_property("__inner_iterator", Value::Object(child_inner));
                    child.set_property(
                        SPL_RUNTIME_CLASS_PROPERTY,
                        Value::string(b"recursivefilteriterator".to_vec()),
                    );
                    return Ok(Value::Object(child));
                }
                return Ok(child_inner);
            }
            if runtime_class_name != "recursivedirectoryiterator" {
                return Ok(Value::Null);
            }
            let Some(path_text) = spl_current_entry(&object)
                .and_then(|(_, value)| spl_directory_path_from_value(&value))
            else {
                return Ok(Value::Null);
            };
            Ok(Value::Object(spl_directory_child_iterator(
                &object,
                &path_text,
                runtime_context,
            )?))
        }
        "getsubpath" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::string(Vec::new()))
        }
        "getsubpathname" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::string(
                spl_directory_current_path(&object)
                    .unwrap_or_default()
                    .into_bytes(),
            ))
        }
        "getcache" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if matches!(
                runtime_class_name.as_str(),
                "cachingiterator" | "recursivecachingiterator"
            ) && !spl_caching_iterator_uses_full_cache(&object)
            {
                return Err(format!(
                    "E_PHP_VM_SPL_BAD_METHOD_CALL: {} does not use a full cache (see CachingIterator::__construct)",
                    object.display_name()
                ));
            }
            if is_spl_caching_iterator_class(&runtime_class_name) {
                Ok(Value::Array(spl_caching_iterator_cache(&object)))
            } else {
                Ok(Value::Array(spl_entries_to_php_array(spl_entries(&object))))
            }
        }
        "seek" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            let position = to_int(&args[0].value)?.max(0) as usize;
            if runtime_class_name == "limititerator" {
                let offset = spl_limit_offset(&object);
                if position < offset {
                    return Err(format!(
                        "E_PHP_VM_SPL_OUT_OF_BOUNDS: Cannot seek to {position} which is below the offset {offset}"
                    ));
                }
                if let Some(count) = spl_limit_count(&object) {
                    let upper = offset.saturating_add(count);
                    if position >= upper {
                        return Err(format!(
                            "E_PHP_VM_SPL_OUT_OF_BOUNDS: Cannot seek to {position} which is behind offset {offset} plus count {count}"
                        ));
                    }
                }
                spl_set_position(&object, position - offset);
            } else {
                spl_set_position(&object, position);
            }
            Ok(Value::Null)
        }
        "getposition" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let position = if runtime_class_name == "limititerator" {
                spl_limit_offset(&object).saturating_add(spl_position(&object))
            } else {
                spl_position(&object)
            };
            Ok(Value::Int(position as i64))
        }
        "getdepth" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if let Some(depth) = spl_rii_hook_depth(&object) {
                return Ok(Value::Int(depth));
            }
            Ok(Value::Int(
                spl_entry_depths(&object)
                    .get(spl_position(&object))
                    .copied()
                    .unwrap_or(0),
            ))
        }
        "getmaxdepth" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let max_depth = spl_rii_max_depth(&object);
            if max_depth < 0 {
                Ok(Value::Bool(false))
            } else {
                Ok(Value::Int(max_depth))
            }
        }
        "setmaxdepth" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 1)?;
            let max_depth = args
                .first()
                .map(|arg| to_int(&arg.value))
                .transpose()?
                .unwrap_or(-1);
            if max_depth < -1 {
                return Err(format!(
                    "E_PHP_VM_SPL_VALUE_ERROR: {}::setMaxDepth(): Argument #1 ($maxDepth) must be greater than or equal to -1",
                    object.display_name()
                ));
            }
            object.set_property("__max_depth", Value::Int(max_depth));
            Ok(Value::Null)
        }
        "callhaschildren" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if matches!(
                runtime_class_name.as_str(),
                "recursiveiteratoriterator" | "recursivetreeiterator"
            ) {
                if let Some(iterator) = spl_rii_call_get_children_target(&object) {
                    return call_spl_iterator_method(
                        iterator,
                        "haschildren",
                        Vec::new(),
                        runtime_context,
                    );
                }
                return Ok(Value::Bool(false));
            }
            call_spl_iterator_method(object.clone(), "haschildren", Vec::new(), runtime_context)
        }
        "callgetchildren" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if runtime_class_name == "recursiveiteratoriterator"
                && let Some(iterator) = spl_rii_call_get_children_target(&object)
            {
                return call_spl_iterator_method(
                    iterator,
                    "getchildren",
                    Vec::new(),
                    runtime_context,
                );
            }
            call_spl_iterator_method(object.clone(), "getchildren", Vec::new(), runtime_context)
        }
        "beginiteration" | "enditeration" | "beginchildren" | "endchildren" | "nextelement" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Null)
        }
        "__construct" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, usize::MAX)?;
            if matches!(
                runtime_class_name.as_str(),
                "arrayiterator" | "recursivearrayiterator"
            ) {
                validate_spl_constructor_arg_count(&class_name, &args, 0, 1)?;
                let entries = args
                    .first()
                    .map(|arg| spl_entries_from_value(&arg.value))
                    .transpose()?
                    .unwrap_or_default();
                spl_set_entries(&object, entries);
                spl_set_position(&object, 0);
            }
            if runtime_class_name == "appenditerator" {
                validate_spl_constructor_arg_count(&class_name, &args, 0, 0)?;
                if spl_bool_property(&object, "__append_initialized") {
                    return Err(
                        "E_PHP_VM_SPL_BAD_METHOD_CALL: AppendIterator::getIterator() must be called exactly once per instance"
                            .to_owned(),
                    );
                }
                spl_set_entries(&object, Vec::new());
                object.set_property("__append_iterators", Value::Array(PhpArray::new()));
                object.set_property(
                    "__append_entry_iterator_indices",
                    Value::Array(PhpArray::new()),
                );
                spl_set_position(&object, 0);
                spl_set_bool_property(&object, "__append_initialized", true);
            }
            Ok(Value::Null)
        }
        "hasnext" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Bool(
                spl_position(&object).saturating_add(1) < spl_entries(&object).len(),
            ))
        }
        "__tostring" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if is_spl_caching_iterator_class(&runtime_class_name) {
                spl_caching_iterator_note_current_seen(&object);
                return spl_caching_iterator_to_string_value(&object);
            }
            if matches!(
                runtime_class_name.as_str(),
                "directoryiterator" | "filesystemiterator" | "recursivedirectoryiterator"
            ) {
                return Ok(Value::string(
                    spl_directory_current_path(&object)
                        .map(|path| spl_file_basename(&path))
                        .unwrap_or_default()
                        .into_bytes(),
                ));
            }
            Ok(spl_current_entry(&object)
                .map(|(_, value)| to_string(&value).map(Value::String))
                .transpose()?
                .unwrap_or_else(|| Value::string(Vec::new())))
        }
        "offsetget" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if is_spl_caching_iterator_class(&runtime_class_name) {
                spl_caching_iterator_require_full_cache(&object, &object.display_name())?;
                return spl_caching_iterator_offset_get(&object, &args[0].value);
            }
            spl_container_offset_get(&object, &args[0].value)
        }
        "offsetexists" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if is_spl_caching_iterator_class(&runtime_class_name) {
                spl_caching_iterator_require_full_cache(&object, &object.display_name())?;
                return spl_caching_iterator_offset_exists(&object, &args[0].value);
            }
            spl_container_offset_exists(&object, &args[0].value)
        }
        "offsetset" => {
            validate_spl_iterator_arg_count(&class_name, &args, 2, 2)?;
            if is_spl_caching_iterator_class(&runtime_class_name) {
                spl_caching_iterator_require_full_cache(&object, &object.display_name())?;
                spl_caching_iterator_offset_set(&object, &args[0].value, args[1].value.clone())?;
                return Ok(Value::Null);
            }
            spl_container_offset_set(&object, args[0].value.clone(), args[1].value.clone())?;
            Ok(Value::Null)
        }
        "offsetunset" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if is_spl_caching_iterator_class(&runtime_class_name) {
                spl_caching_iterator_require_full_cache(&object, &object.display_name())?;
                spl_caching_iterator_offset_unset(&object, &args[0].value)?;
                return Ok(Value::Null);
            }
            spl_container_offset_unset(&object, &args[0].value)?;
            Ok(Value::Null)
        }
        "append" | "additerator" | "attachiterator" => {
            if !matches!(
                runtime_class_name.as_str(),
                "appenditerator" | "multipleiterator"
            ) {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            validate_spl_iterator_arg_count(&class_name, &args, 1, 3)?;
            if runtime_class_name == "multipleiterator" {
                spl_multiple_iterator_attach(&object, &args)?;
                return Ok(Value::Null);
            }
            if !spl_bool_property(&object, "__append_initialized") {
                return Err(
                    "E_PHP_VM_SPL_ERROR: The object is in an invalid state as the parent constructor was not called"
                        .to_owned(),
                );
            }
            if !matches!(effective_value(&args[0].value), Value::Object(_)) {
                return Err(format!(
                    "E_PHP_VM_SPL_TYPE_ERROR: AppendIterator::append(): Argument #1 ($iterator) must be of type Iterator, {} given",
                    type_error_value_name(&args[0].value)
                ));
            }
            let Value::Object(iterator) = effective_value(&args[0].value) else {
                unreachable!("AppendIterator::append object type was validated above")
            };
            let appended_entries = spl_entries_from_value(&args[0].value)?;
            let iterator_index = spl_append_iterators(&object).len() as i64;
            let mut append_iterators = spl_append_iterators_array(&object);
            append_iterators.append(Value::Object(iterator));
            object.set_property("__append_iterators", Value::Array(append_iterators));
            let mut entry_indices = spl_append_entry_indices_array(&object);
            for _ in &appended_entries {
                entry_indices.append(Value::Int(iterator_index));
            }
            object.set_property(
                "__append_entry_iterator_indices",
                Value::Array(entry_indices),
            );
            let mut entries = spl_entries(&object);
            entries.extend(appended_entries);
            spl_set_entries(&object, entries);
            if runtime_class_name == "multipleiterator" {
                let count = object
                    .get_property("__iterator_count")
                    .and_then(|value| match effective_value(&value) {
                        Value::Int(value) => Some(value),
                        _ => None,
                    })
                    .unwrap_or(0)
                    .saturating_add(1);
                object.set_property("__iterator_count", Value::Int(count));
                if let Value::Object(iterator) = effective_value(&args[0].value) {
                    let mut attached = object
                        .get_property("__attached_iterator_ids")
                        .and_then(|value| match effective_value(&value) {
                            Value::Array(array) => Some(array),
                            _ => None,
                        })
                        .unwrap_or_default();
                    attached.append(Value::Int(iterator.id() as i64));
                    object.set_property("__attached_iterator_ids", Value::Array(attached));
                }
            }
            Ok(Value::Null)
        }
        "containsiterator" => {
            if runtime_class_name != "multipleiterator" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            let Value::Object(iterator) = effective_value(&args[0].value) else {
                return Ok(Value::Bool(false));
            };
            let target_id = iterator.id() as i64;
            let contains = object
                .get_property("__attached_iterator_ids")
                .and_then(|value| match effective_value(&value) {
                    Value::Array(array) => Some(array),
                    _ => None,
                })
                .is_some_and(|array| {
                    array.iter().any(|(_, value)| match effective_value(value) {
                        Value::Int(id) => id == target_id,
                        _ => false,
                    })
                });
            Ok(Value::Bool(contains))
        }
        "detachiterator" => {
            if runtime_class_name != "multipleiterator" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            let Value::Object(iterator) = effective_value(&args[0].value) else {
                return Ok(Value::Null);
            };
            let target_id = iterator.id() as i64;
            let mut removed = false;
            let mut records = Vec::new();
            for (attached, info) in spl_multiple_iterator_records(&object) {
                if attached.id() as i64 == target_id && !removed {
                    removed = true;
                } else {
                    records.push((attached, info));
                }
            }
            if removed {
                spl_multiple_iterator_set_records(&object, records);
            }
            Ok(Value::Null)
        }
        "countiterators" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(object
                .get_property("__iterator_count")
                .map(|value| effective_value(&value))
                .unwrap_or(Value::Int(0)))
        }
        "getarrayiterator" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let iterator = ObjectRef::new_with_display_name(
                &spl_iterator_class("ArrayIterator"),
                spl_iterator_display_name("ArrayIterator"),
            );
            let entries = if runtime_class_name == "appenditerator" {
                spl_append_iterators(&object)
                    .into_iter()
                    .enumerate()
                    .map(|(index, iterator)| (ArrayKey::Int(index as i64), Value::Object(iterator)))
                    .collect()
            } else {
                spl_entries(&object)
            };
            spl_set_entries(&iterator, entries);
            spl_set_position(&iterator, 0);
            Ok(Value::Object(iterator))
        }
        "getiteratorindex" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if runtime_class_name == "appenditerator" {
                return Ok(Value::Int(
                    spl_append_entry_iterator_indices(&object)
                        .get(spl_position(&object))
                        .copied()
                        .unwrap_or(0),
                ));
            }
            Ok(Value::Int(0))
        }
        "getprefix" | "getpostfix" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if runtime_class_name == "recursivetreeiterator" && method == "getprefix" {
                return Ok(Value::string(spl_rti_prefix(&object).into_bytes()));
            }
            let property = if method == "getpostfix" {
                "__postfix"
            } else {
                "__prefix"
            };
            Ok(object
                .get_property(property)
                .map(|value| effective_value(&value))
                .unwrap_or_else(|| Value::string(Vec::new())))
        }
        "setpostfix" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            object.set_property("__postfix", args[0].value.clone());
            Ok(Value::Null)
        }
        "getentry" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_current_entry(&object)
                .map(|(_, value)| {
                    Ok::<Value, String>(Value::string(spl_rti_entry_text(&value)?.into_bytes()))
                })
                .transpose()?
                .unwrap_or_else(|| Value::string(Vec::new())))
        }
        "setprefixpart" => {
            validate_spl_iterator_arg_count(&class_name, &args, 2, 2)?;
            let part = to_int(&args[0].value)?;
            if !(0..=5).contains(&part) {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: RecursiveTreeIterator::setPrefixPart(): Argument #1 ($part) must be a RecursiveTreeIterator::PREFIX_* constant"
                        .to_owned(),
                );
            }
            spl_rti_set_prefix_part(&object, part as usize, args[1].value.clone())?;
            Ok(Value::Null)
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn validate_spl_iterator_arg_count(
    name: &str,
    args: &[CallArgument],
    min: usize,
    max: usize,
) -> Result<(), String> {
    if args.len() < min {
        let word = if min == 1 { "argument" } else { "arguments" };
        return Err(format!(
            "E_PHP_RUNTIME_BUILTIN_ARITY: {name} expects at least {min} {word}, {} given",
            args.len()
        ));
    }
    if args.len() > max {
        let word = if max == 1 { "argument" } else { "arguments" };
        return Err(format!(
            "E_PHP_RUNTIME_BUILTIN_ARITY: {name} expects at most {max} {word}, {} given",
            args.len()
        ));
    }
    Ok(())
}

pub(super) fn validate_spl_constructor_arg_count(
    class_name: &str,
    args: &[CallArgument],
    min: usize,
    max: usize,
) -> Result<(), String> {
    let display = spl_runtime_display_name(class_name);
    if args.len() < min {
        let expectation = if min == max {
            format!("exactly {min} argument")
        } else {
            format!("at least {min} argument")
        };
        return Err(format!(
            "E_PHP_VM_SPL_TYPE_ERROR: {display}::__construct() expects {expectation}, {} given",
            args.len()
        ));
    }
    if args.len() > max {
        let expectation = if min == max {
            format!("exactly {max} argument")
        } else {
            format!("at most {max} arguments")
        };
        return Err(format!(
            "E_PHP_VM_SPL_TYPE_ERROR: {display}::__construct() expects {expectation}, {} given",
            args.len()
        ));
    }
    Ok(())
}

pub(super) fn spl_iterator_class(class_name: &str) -> RuntimeClassEntry {
    let normalized = normalize_class_name(class_name);
    let mut interfaces = vec!["Iterator".to_owned(), "Traversable".to_owned()];
    if matches!(
        normalized.as_str(),
        "arrayiterator"
            | "recursivearrayiterator"
            | "appenditerator"
            | "cachingiterator"
            | "recursivecachingiterator"
            | "globiterator"
    ) {
        interfaces.push("Countable".to_owned());
    }
    if matches!(
        normalized.as_str(),
        "arrayiterator" | "recursivearrayiterator"
    ) {
        interfaces.push("ArrayAccess".to_owned());
        interfaces.push("SeekableIterator".to_owned());
    }
    if matches!(
        normalized.as_str(),
        "directoryiterator" | "filesystemiterator" | "recursivedirectoryiterator"
    ) {
        interfaces.push("SeekableIterator".to_owned());
    }
    if matches!(
        normalized.as_str(),
        "recursivearrayiterator" | "recursivedirectoryiterator"
    ) {
        interfaces.push("RecursiveIterator".to_owned());
    }
    if matches!(
        normalized.as_str(),
        "iteratoriterator"
            | "limititerator"
            | "recursiveiteratoriterator"
            | "cachingiterator"
            | "recursivecachingiterator"
            | "regexiterator"
            | "recursiveregexiterator"
            | "norewinditerator"
            | "infiniteiterator"
            | "filteriterator"
            | "recursivefilteriterator"
            | "parentiterator"
            | "recursivetreeiterator"
    ) {
        interfaces.push("OuterIterator".to_owned());
    }
    RuntimeClassEntry {
        name: normalize_class_name(class_name),
        parent: match normalized.as_str() {
            "directoryiterator" => Some(normalize_class_name("SplFileInfo")),
            "filesystemiterator" => Some(normalize_class_name("DirectoryIterator")),
            "recursivedirectoryiterator" => Some(normalize_class_name("FilesystemIterator")),
            "recursivecachingiterator" => Some(normalize_class_name("CachingIterator")),
            "recursiveregexiterator" => Some(normalize_class_name("RegexIterator")),
            "recursivefilteriterator" | "parentiterator" => {
                Some(normalize_class_name("FilterIterator"))
            }
            "recursivetreeiterator" => Some(normalize_class_name("RecursiveIteratorIterator")),
            _ => None,
        },
        interfaces,
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn spl_iterator_display_name(class_name: &str) -> &'static str {
    match normalize_class_name(class_name).as_str() {
        "arrayiterator" => "ArrayIterator",
        "recursivearrayiterator" => "RecursiveArrayIterator",
        "directoryiterator" => "DirectoryIterator",
        "filesystemiterator" => "FilesystemIterator",
        "recursivedirectoryiterator" => "RecursiveDirectoryIterator",
        "iteratoriterator" => "IteratorIterator",
        "limititerator" => "LimitIterator",
        "emptyiterator" => "EmptyIterator",
        "appenditerator" => "AppendIterator",
        "recursiveiteratoriterator" => "RecursiveIteratorIterator",
        "cachingiterator" => "CachingIterator",
        "recursivecachingiterator" => "RecursiveCachingIterator",
        "regexiterator" => "RegexIterator",
        "recursiveregexiterator" => "RecursiveRegexIterator",
        "norewinditerator" => "NoRewindIterator",
        "infiniteiterator" => "InfiniteIterator",
        "filteriterator" => "FilterIterator",
        "recursivefilteriterator" => "RecursiveFilterIterator",
        "parentiterator" => "ParentIterator",
        "recursivetreeiterator" => "RecursiveTreeIterator",
        "multipleiterator" => "MultipleIterator",
        "globiterator" => "GlobIterator",
        _ => "ArrayIterator",
    }
}

pub(super) fn spl_iterator_display_method(method: &str) -> String {
    match normalize_method_name(method).as_str() {
        "offsetget" => "offsetGet".to_owned(),
        "offsetexists" => "offsetExists".to_owned(),
        "offsetset" => "offsetSet".to_owned(),
        "offsetunset" => "offsetUnset".to_owned(),
        _ => method.to_owned(),
    }
}

pub(super) fn validate_spl_caching_iterator_flags(
    class_name: &str,
    args: &[CallArgument],
) -> Result<(), String> {
    let Some(flags_arg) = args.get(1) else {
        return Ok(());
    };
    let flags = to_int(&flags_arg.value)?;
    validate_spl_caching_string_flags(class_name, "__construct", "Argument #2 ($flags)", flags)
}

pub(super) const SPL_CACHING_CALL_TOSTRING: i64 = 1;
pub(super) const SPL_CACHING_TOSTRING_USE_KEY: i64 = 2;
pub(super) const SPL_CACHING_TOSTRING_USE_CURRENT: i64 = 4;
pub(super) const SPL_CACHING_TOSTRING_USE_INNER: i64 = 8;
pub(super) const SPL_CACHING_FULL_CACHE: i64 = 256;
pub(super) const SPL_CACHING_STRING_FLAGS: i64 = SPL_CACHING_CALL_TOSTRING
    | SPL_CACHING_TOSTRING_USE_KEY
    | SPL_CACHING_TOSTRING_USE_CURRENT
    | SPL_CACHING_TOSTRING_USE_INNER;

pub(super) fn is_spl_caching_iterator_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "cachingiterator" | "recursivecachingiterator"
    )
}

pub(super) fn spl_filtering_iterator_accepts_current(object: &ObjectRef) -> bool {
    matches!(
        spl_runtime_marker(object).as_deref(),
        Some(
            "regexiterator"
                | "recursiveregexiterator"
                | "filteriterator"
                | "recursivefilteriterator"
                | "parentiterator"
        )
    )
}

pub(super) fn validate_spl_caching_string_flags(
    class_name: &str,
    method: &str,
    argument: &str,
    flags: i64,
) -> Result<(), String> {
    let string_mode_bits = flags & SPL_CACHING_STRING_FLAGS;
    if string_mode_bits.count_ones() > 1 {
        return Err(format!(
            "E_PHP_VM_SPL_VALUE_ERROR: {}::{method}(): {argument} must contain only one of CachingIterator::CALL_TOSTRING, CachingIterator::TOSTRING_USE_KEY, CachingIterator::TOSTRING_USE_CURRENT, or CachingIterator::TOSTRING_USE_INNER",
            spl_iterator_display_name(class_name),
        ));
    }
    Ok(())
}

pub(super) fn spl_caching_iterator_flags(object: &ObjectRef) -> i64 {
    object
        .get_property("__caching_flags")
        .map(|flags| effective_value(&flags))
        .and_then(|flags| match flags {
            Value::Int(flags) => Some(flags),
            _ => None,
        })
        .unwrap_or(SPL_CACHING_CALL_TOSTRING)
}

pub(super) fn spl_caching_iterator_uses_full_cache(object: &ObjectRef) -> bool {
    spl_caching_iterator_flags(object) & SPL_CACHING_FULL_CACHE != 0
}

pub(super) fn spl_caching_iterator_diagnostic_class(object: &ObjectRef) -> &'static str {
    match spl_runtime_marker(object).as_deref() {
        Some("recursivecachingiterator") => "RecursiveCachingIterator",
        _ => "CachingIterator",
    }
}

pub(super) fn spl_caching_iterator_require_full_cache(
    object: &ObjectRef,
    class_name: &str,
) -> Result<(), String> {
    if spl_caching_iterator_uses_full_cache(object) {
        Ok(())
    } else {
        Err(format!(
            "E_PHP_VM_SPL_BAD_METHOD_CALL: {} does not use a full cache (see CachingIterator::__construct)",
            class_name
        ))
    }
}

pub(super) fn spl_caching_iterator_cache(object: &ObjectRef) -> PhpArray {
    object
        .get_property("__caching_cache")
        .map(|value| effective_value(&value))
        .and_then(|value| match value {
            Value::Array(array) => Some(array),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_set_caching_iterator_cache(object: &ObjectRef, cache: PhpArray) {
    object.set_property("__caching_cache", Value::Array(cache));
}

pub(super) fn spl_caching_iterator_seen_count(object: &ObjectRef) -> usize {
    let len = spl_entries(object).len();
    object
        .get_property("__caching_seen_count")
        .map(|value| effective_value(&value))
        .and_then(|value| match value {
            Value::Int(value) if value > 0 => Some(value as usize),
            _ => None,
        })
        .unwrap_or(0)
        .min(len)
}

pub(super) fn spl_caching_iterator_note_current_seen(object: &ObjectRef) {
    let len = spl_entries(object).len();
    let seen = spl_caching_iterator_seen_count(object);
    let position = spl_position(object);
    let position_seen = position.saturating_add(1).min(len);
    if let Some((key, value)) = spl_entries(object).into_iter().nth(position) {
        let mut cache = spl_caching_iterator_cache(object);
        cache.insert(key, value);
        spl_set_caching_iterator_cache(object, cache);
    }
    object.set_property(
        "__caching_seen_count",
        Value::Int(seen.max(position_seen) as i64),
    );
}

pub(super) fn spl_caching_iterator_offset_get(
    object: &ObjectRef,
    key: &Value,
) -> Result<Value, String> {
    let key = array_key_from_value(key)?;
    Ok(spl_caching_iterator_cache(object)
        .get(&key)
        .map(effective_value)
        .unwrap_or(Value::Null))
}

pub(super) fn spl_caching_iterator_offset_exists(
    object: &ObjectRef,
    key: &Value,
) -> Result<Value, String> {
    let key = array_key_from_value(key)?;
    Ok(Value::Bool(
        spl_caching_iterator_cache(object)
            .get(&key)
            .is_some_and(|value| !matches!(effective_value(value), Value::Null)),
    ))
}

pub(super) fn spl_caching_iterator_offset_set(
    object: &ObjectRef,
    key: &Value,
    value: Value,
) -> Result<(), String> {
    let key = array_key_from_value(key)?;
    let mut cache = spl_caching_iterator_cache(object);
    cache.insert(key, value);
    spl_set_caching_iterator_cache(object, cache);
    Ok(())
}

pub(super) fn spl_caching_iterator_offset_unset(
    object: &ObjectRef,
    key: &Value,
) -> Result<(), String> {
    let key = array_key_from_value(key)?;
    let mut cache = spl_caching_iterator_cache(object);
    cache.remove(&key);
    spl_set_caching_iterator_cache(object, cache);
    Ok(())
}

pub(super) fn spl_caching_iterator_to_string_value(object: &ObjectRef) -> Result<Value, String> {
    let flags = spl_caching_iterator_flags(object);
    let Some((key, value)) = spl_current_entry(object) else {
        return Ok(Value::string(Vec::new()));
    };
    if flags & SPL_CACHING_CALL_TOSTRING != 0 {
        return Ok(to_string(&value)
            .map(Value::String)
            .unwrap_or_else(|_| Value::string(Vec::new())));
    }
    if flags & SPL_CACHING_TOSTRING_USE_KEY != 0 {
        return to_string(&array_key_to_value(key)).map(Value::String);
    }
    if flags & SPL_CACHING_TOSTRING_USE_CURRENT != 0 {
        return to_string(&value).map(Value::String);
    }
    if flags & SPL_CACHING_TOSTRING_USE_INNER != 0 {
        let mut bytes = to_string(&array_key_to_value(key))?.as_bytes().to_vec();
        bytes.push(b':');
        bytes.extend_from_slice(to_string(&value)?.as_bytes());
        return Ok(Value::String(PhpString::from_bytes(bytes)));
    }
    Err(
        "E_PHP_VM_SPL_BAD_METHOD_CALL: CachingIterator does not fetch string value (see CachingIterator::__construct)"
            .to_owned(),
    )
}

pub(super) fn validate_recursive_tree_iterator_source(value: &Value) -> Result<(), String> {
    let Value::Object(object) = effective_value(value) else {
        return Ok(());
    };
    if spl_runtime_marker(&object).as_deref() == Some("arrayiterator") {
        return Err(format!(
            "E_PHP_VM_SPL_TYPE_ERROR: RecursiveCachingIterator::__construct(): Argument #1 ($iterator) must be of type RecursiveIterator, {} given",
            type_error_value_name(&Value::Object(object))
        ));
    }
    Ok(())
}

pub(super) fn spl_glob_entries(
    pattern: &str,
    runtime_context: &RuntimeContext,
) -> Result<Vec<(ArrayKey, Value)>, String> {
    let resolved = spl_file_resolve_path(pattern, runtime_context);
    let directory = resolved.parent().unwrap_or_else(|| Path::new("."));
    let basename_pattern = resolved
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !runtime_context.filesystem.allows_path(directory) {
        return Err(format!(
            "E_PHP_VM_SPL_FILE_DENIED: local directory access denied for `{}`",
            directory.to_string_lossy()
        ));
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| {
        format!(
            "E_PHP_VM_SPL_GLOB_READ: failed to read `{}`: {error}",
            directory.to_string_lossy()
        )
    })? {
        let entry = entry.map_err(|error| {
            format!(
                "E_PHP_VM_SPL_GLOB_READ: failed to read `{}`: {error}",
                directory.to_string_lossy()
            )
        })?;
        let path = entry.path();
        if !runtime_context.filesystem.allows_path(&path) {
            continue;
        }
        let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
            continue;
        };
        if spl_glob_name_matches(&basename_pattern, &name) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            (
                ArrayKey::Int(index as i64),
                Value::string(path.to_string_lossy().into_owned().into_bytes()),
            )
        })
        .collect())
}

pub(super) fn spl_glob_name_matches(pattern: &str, name: &str) -> bool {
    fn matches_inner(pattern: &[u8], name: &[u8]) -> bool {
        match pattern.split_first() {
            None => name.is_empty(),
            Some((&b'*', rest)) => {
                matches_inner(rest, name)
                    || (!name.is_empty() && matches_inner(pattern, &name[1..]))
            }
            Some((&b'?', rest)) => !name.is_empty() && matches_inner(rest, &name[1..]),
            Some((&expected, rest)) => {
                name.first().is_some_and(|actual| *actual == expected)
                    && matches_inner(rest, &name[1..])
            }
        }
    }
    matches_inner(pattern.as_bytes(), name.as_bytes())
}

pub(super) fn spl_directory_entries(
    class_name: &str,
    directory: &str,
    flags: i64,
    runtime_context: &RuntimeContext,
) -> Result<Vec<(ArrayKey, Value)>, String> {
    if directory.is_empty() {
        return Err(format!(
            "E_PHP_VM_SPL_VALUE_ERROR: {}::__construct(): Argument #1 ($directory) must not be empty",
            spl_iterator_display_name(class_name)
        ));
    }
    if directory.as_bytes().contains(&0) {
        return Err(format!(
            "E_PHP_VM_SPL_VALUE_ERROR: {}::__construct(): Argument #1 ($directory) must not contain any null bytes",
            spl_iterator_display_name(class_name)
        ));
    }
    let resolved = spl_file_resolve_path(directory, runtime_context);
    if !runtime_context.filesystem.allows_path(&resolved) {
        return Err(format!(
            "E_PHP_VM_SPL_FILE_DENIED: local directory access denied for `{}`",
            resolved.to_string_lossy()
        ));
    }
    if !resolved.is_dir() {
        return Err(format!(
            "E_PHP_VM_SPL_UNEXPECTED_VALUE: {}::__construct({}): Failed to open directory",
            spl_iterator_display_name(class_name),
            resolved.to_string_lossy()
        ));
    }

    let mut paths = Vec::new();
    if flags & SPL_FILESYSTEM_SKIP_DOTS == 0 {
        paths.push(resolved.join("."));
        paths.push(resolved.join(".."));
    }
    for entry in fs::read_dir(&resolved).map_err(|error| {
        format!(
            "E_PHP_VM_SPL_DIRECTORY_READ: failed to read `{}`: {error}",
            resolved.to_string_lossy()
        )
    })? {
        let entry = entry.map_err(|error| {
            format!(
                "E_PHP_VM_SPL_DIRECTORY_READ: failed to read `{}`: {error}",
                resolved.to_string_lossy()
            )
        })?;
        let path = entry.path();
        if runtime_context.filesystem.allows_path(&path) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| spl_directory_entry(class_name, flags, index, path))
        .collect())
}

pub(super) fn spl_directory_entry(
    class_name: &str,
    flags: i64,
    index: usize,
    path: PathBuf,
) -> (ArrayKey, Value) {
    let path_text = spl_directory_path_text(&path, flags);
    let key = if normalize_class_name(class_name) == "directoryiterator" {
        ArrayKey::Int(index as i64)
    } else if flags & SPL_FILESYSTEM_KEY_MODE_MASK == SPL_FILESYSTEM_KEY_AS_FILENAME {
        ArrayKey::String(PhpString::from_test_str(&spl_file_basename(&path_text)))
    } else {
        ArrayKey::String(PhpString::from_test_str(&path_text))
    };
    let value = if normalize_class_name(class_name) == "directoryiterator"
        || flags & SPL_FILESYSTEM_CURRENT_MODE_MASK == SPL_FILESYSTEM_CURRENT_AS_SELF
    {
        Value::Object(spl_directory_entry_object(class_name, &path_text))
    } else if flags & SPL_FILESYSTEM_CURRENT_MODE_MASK == SPL_FILESYSTEM_CURRENT_AS_PATHNAME {
        Value::string(path_text.into_bytes())
    } else {
        Value::Object(spl_file_info_object(&path_text))
    };
    (key, value)
}

pub(super) fn spl_directory_path_text(path: &Path, flags: i64) -> String {
    let mut text = path.to_string_lossy().into_owned();
    if flags & SPL_FILESYSTEM_UNIX_PATHS != 0 {
        text = text.replace('\\', "/");
    }
    text
}

pub(super) fn spl_file_info_object(path: &str) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &spl_file_class("SplFileInfo"),
        spl_file_display_name("SplFileInfo"),
    );
    spl_file_set_path(&object, path);
    object.set_property(
        SPL_RUNTIME_CLASS_PROPERTY,
        Value::string(b"splfileinfo".to_vec()),
    );
    object
}

pub(super) fn spl_directory_entry_object(class_name: &str, path: &str) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &spl_iterator_class(class_name),
        spl_iterator_display_name(class_name),
    );
    spl_file_set_path(&object, path);
    object.set_property("__flags", Value::Int(SPL_FILESYSTEM_CURRENT_AS_FILEINFO));
    object
}

pub(super) fn spl_directory_current_path(object: &ObjectRef) -> Option<String> {
    if let Some((_, value)) = spl_current_entry(object)
        && let Some(path) = spl_directory_path_from_value(&value)
    {
        return Some(path);
    }
    object
        .get_property("__path")
        .and_then(|value| match effective_value(&value) {
            Value::String(path) => Some(path.to_string_lossy()),
            _ => None,
        })
}

pub(super) fn spl_directory_path_from_value(value: &Value) -> Option<String> {
    match effective_value(value) {
        Value::String(path) => Some(path.to_string_lossy()),
        Value::Object(object) => object
            .get_property("__path")
            .and_then(|value| match effective_value(&value) {
                Value::String(path) => Some(path.to_string_lossy()),
                _ => None,
            })
            .or_else(|| spl_directory_current_path(&object)),
        _ => None,
    }
}

pub(super) fn spl_recursive_directory_entries(
    object: &ObjectRef,
    runtime_context: &RuntimeContext,
) -> Result<Vec<(ArrayKey, Value)>, String> {
    let mut flattened = Vec::new();
    spl_collect_recursive_directory_entries(object, runtime_context, &mut flattened)?;
    Ok(flattened)
}

pub(super) fn spl_collect_recursive_directory_entries(
    object: &ObjectRef,
    runtime_context: &RuntimeContext,
    flattened: &mut Vec<(ArrayKey, Value)>,
) -> Result<(), String> {
    let flags = spl_filesystem_flags(object);
    for (key, value) in spl_entries(object) {
        let Some(path_text) = spl_directory_path_from_value(&value) else {
            flattened.push((key, value));
            continue;
        };
        if spl_directory_is_dot_text(&path_text) {
            continue;
        }
        let path = spl_file_resolve_path(&path_text, runtime_context);
        let is_directory = if flags & SPL_FILESYSTEM_FOLLOW_SYMLINKS != 0 {
            fs::metadata(&path)
        } else {
            fs::symlink_metadata(&path)
        }
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
        if is_directory {
            let child = spl_directory_child_iterator(object, &path_text, runtime_context)?;
            spl_collect_recursive_directory_entries(&child, runtime_context, flattened)?;
        } else {
            flattened.push((key, value));
        }
    }
    Ok(())
}

pub(super) fn spl_directory_child_iterator(
    object: &ObjectRef,
    path: &str,
    runtime_context: &RuntimeContext,
) -> Result<ObjectRef, String> {
    let flags = spl_filesystem_flags(object);
    let child = ObjectRef::new_with_display_name(
        &spl_iterator_class("RecursiveDirectoryIterator"),
        spl_iterator_display_name("RecursiveDirectoryIterator"),
    );
    spl_set_entries(
        &child,
        spl_directory_entries("RecursiveDirectoryIterator", path, flags, runtime_context)?,
    );
    spl_set_position(&child, 0);
    child.set_property("__directory", Value::string(path.as_bytes().to_vec()));
    child.set_property("__flags", Value::Int(flags));
    Ok(child)
}

pub(super) fn spl_filesystem_flags(object: &ObjectRef) -> i64 {
    object
        .get_property("__flags")
        .and_then(|value| match effective_value(&value) {
            Value::Int(flags) => Some(flags),
            _ => None,
        })
        .unwrap_or(SPL_FILESYSTEM_CURRENT_AS_FILEINFO)
}

pub(super) fn spl_directory_is_dot_text(path: &str) -> bool {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let name = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
    name == "." || name == ".."
}

pub(super) fn spl_entries_from_value(value: &Value) -> Result<Vec<(ArrayKey, Value)>, String> {
    match effective_value(value) {
        Value::Array(array) => Ok(array
            .iter()
            .map(|(key, value)| (key.clone(), effective_value(value)))
            .collect()),
        Value::Object(object)
            if spl_runtime_marker(&object)
                .is_some_and(|class| is_spl_iterator_runtime_class(&class)) =>
        {
            Ok(spl_entries(&object))
        }
        Value::Object(object)
            if spl_runtime_marker(&object)
                .is_some_and(|class| is_spl_container_runtime_class(&class)) =>
        {
            Ok(spl_container_entries(&object))
        }
        Value::Object(object) => Ok(object
            .properties_snapshot()
            .into_iter()
            .filter(|(name, _)| !name.starts_with("__"))
            .map(|(name, value)| (ArrayKey::String(PhpString::from_test_str(&name)), value))
            .collect()),
        other => Err(format!(
            "E_PHP_VM_SPL_ITERATOR_SOURCE: {} is not iterable for SPL iterator MVP",
            value_type_name(&other)
        )),
    }
}

pub(super) fn spl_recursive_entries_from_value(
    value: &Value,
) -> Result<Vec<(ArrayKey, Value)>, String> {
    Ok(spl_recursive_entries_with_depth_from_value(value, 0)?
        .into_iter()
        .map(|(key, value, _)| (key, value))
        .collect())
}

pub(super) struct SplRecursiveEntry {
    key: ArrayKey,
    value: Value,
    depth: i64,
    iterators: Vec<ObjectRef>,
    hook_iterators: Vec<ObjectRef>,
}

pub(super) fn spl_recursive_entries_with_depth_from_value(
    value: &Value,
    depth: i64,
) -> Result<Vec<(ArrayKey, Value, i64)>, String> {
    Ok(
        spl_recursive_entries_with_context_from_value(value, depth, Vec::new())?
            .into_iter()
            .map(|entry| (entry.key, entry.value, entry.depth))
            .collect(),
    )
}

pub(super) fn spl_parent_iterator_recursive_entries(
    parent: &ObjectRef,
    mode: i64,
) -> Result<Vec<SplRecursiveEntry>, String> {
    if mode == SPL_RII_LEAVES_ONLY {
        return Ok(Vec::new());
    }

    let source = parent
        .get_property("__inner_iterator")
        .map(|value| effective_value(&value))
        .unwrap_or_else(|| Value::Object(parent.clone()));
    spl_parent_iterator_recursive_entries_from_value(&source, 0, Vec::new(), mode)
}

pub(super) fn spl_parent_iterator_recursive_entries_from_value(
    value: &Value,
    depth: i64,
    ancestors: Vec<ObjectRef>,
    mode: i64,
) -> Result<Vec<SplRecursiveEntry>, String> {
    let (entries, current_iterator) = spl_recursive_context_entries_from_value(value)?;
    let iterators = ancestors;

    let mut flattened = Vec::new();
    for (position, (key, value)) in entries.into_iter().enumerate() {
        let positioned_iterators =
            spl_hook_iterators_for_position(&iterators, current_iterator.as_ref(), position);
        let hook_iterators =
            spl_hook_iterators_for_position(&iterators, current_iterator.as_ref(), position);
        if !spl_value_has_recursive_children(&value) {
            continue;
        }

        let children = spl_parent_iterator_recursive_entries_from_value(
            &value,
            depth + 1,
            positioned_iterators.clone(),
            mode,
        )?;
        let parent_entry = SplRecursiveEntry {
            key,
            value,
            depth,
            iterators: positioned_iterators,
            hook_iterators,
        };

        if mode == SPL_RII_CHILD_FIRST {
            flattened.extend(children);
            flattened.push(parent_entry);
        } else {
            flattened.push(parent_entry);
            flattened.extend(children);
        }
    }

    Ok(flattened)
}

pub(super) fn spl_recursive_entries_with_context_from_value(
    value: &Value,
    depth: i64,
    ancestors: Vec<ObjectRef>,
) -> Result<Vec<SplRecursiveEntry>, String> {
    spl_recursive_entries_with_context_from_value_and_mode(
        value,
        depth,
        ancestors,
        SPL_RII_LEAVES_ONLY,
    )
}

pub(super) fn spl_recursive_entries_with_context_from_value_and_mode(
    value: &Value,
    depth: i64,
    ancestors: Vec<ObjectRef>,
    mode: i64,
) -> Result<Vec<SplRecursiveEntry>, String> {
    spl_recursive_entries_with_context_and_hooks_from_value_and_mode(
        value,
        depth,
        ancestors.clone(),
        ancestors,
        mode,
    )
}

pub(super) fn spl_recursive_entries_with_context_and_hooks_from_value_and_mode(
    value: &Value,
    depth: i64,
    ancestors: Vec<ObjectRef>,
    hook_ancestors: Vec<ObjectRef>,
    mode: i64,
) -> Result<Vec<SplRecursiveEntry>, String> {
    if let Value::Object(object) = effective_value(value)
        && spl_runtime_marker(&object).as_deref() == Some("arrayobject")
        && normalize_class_name(&spl_array_object_iterator_class(&object))
            != "recursivearrayiterator"
    {
        return Err(
            "E_PHP_VM_SPL_INVALID_ARGUMENT: An instance of RecursiveIterator or IteratorAggregate creating it is required"
                .to_owned(),
        );
    }
    if let Value::Object(object) = effective_value(value)
        && matches!(
            spl_runtime_marker(&object).as_deref(),
            Some("regexiterator" | "recursiveregexiterator")
        )
    {
        return Ok(spl_entries(&object)
            .into_iter()
            .filter_map(|(key, value)| {
                let (accepted, current) = spl_regex_current_value(&object, &key, value);
                accepted.then_some(SplRecursiveEntry {
                    key,
                    value: current,
                    depth,
                    iterators: ancestors.clone(),
                    hook_iterators: hook_ancestors.clone(),
                })
            })
            .collect());
    }
    let (entries, current_iterator) = spl_recursive_context_entries_from_value(value)?;
    let iterators = ancestors;
    let mut flattened = Vec::new();
    for (position, (key, value)) in entries.into_iter().enumerate() {
        let positioned_iterators =
            spl_hook_iterators_for_position(&iterators, current_iterator.as_ref(), position);
        let hook_iterators =
            spl_hook_iterators_for_position(&hook_ancestors, current_iterator.as_ref(), position);
        match effective_value(&value) {
            Value::Array(_) => {
                let child_source =
                    spl_recursive_array_child_context_value(&hook_iterators, &value)?;
                let children = spl_recursive_entries_with_context_and_hooks_from_value_and_mode(
                    &child_source,
                    depth + 1,
                    positioned_iterators.clone(),
                    hook_iterators.clone(),
                    mode,
                )?;
                let parent = SplRecursiveEntry {
                    key,
                    value,
                    depth,
                    iterators: positioned_iterators,
                    hook_iterators,
                };
                if mode == SPL_RII_CHILD_FIRST {
                    flattened.extend(children);
                    flattened.push(parent);
                } else if mode == SPL_RII_SELF_FIRST {
                    flattened.push(parent);
                    flattened.extend(children);
                } else {
                    flattened.extend(children);
                }
            }
            Value::Object(object)
                if spl_runtime_marker(&object).as_deref() == Some("recursivearrayiterator") =>
            {
                let recursive_value = Value::Object(object);
                let children = spl_recursive_entries_with_context_and_hooks_from_value_and_mode(
                    &recursive_value,
                    depth + 1,
                    positioned_iterators.clone(),
                    hook_iterators.clone(),
                    mode,
                )?;
                let parent = SplRecursiveEntry {
                    key,
                    value: recursive_value,
                    depth,
                    iterators: positioned_iterators,
                    hook_iterators,
                };
                if mode == SPL_RII_CHILD_FIRST {
                    flattened.extend(children);
                    flattened.push(parent);
                } else if mode == SPL_RII_SELF_FIRST {
                    flattened.push(parent);
                    flattened.extend(children);
                } else {
                    flattened.extend(children);
                }
            }
            Value::Object(object)
                if spl_runtime_marker(&object).as_deref() == Some("arrayiterator") =>
            {
                let recursive_value = Value::Object(object.clone());
                let children = spl_recursive_entries_with_context_and_hooks_from_value_and_mode(
                    &Value::Object(object),
                    depth + 1,
                    positioned_iterators.clone(),
                    hook_iterators.clone(),
                    mode,
                )?;
                let parent = SplRecursiveEntry {
                    key,
                    value: recursive_value,
                    depth,
                    iterators: positioned_iterators,
                    hook_iterators,
                };
                if mode == SPL_RII_CHILD_FIRST {
                    flattened.extend(children);
                    flattened.push(parent);
                } else if mode == SPL_RII_SELF_FIRST {
                    flattened.push(parent);
                    flattened.extend(children);
                } else {
                    flattened.extend(children);
                }
            }
            _ => flattened.push(SplRecursiveEntry {
                key,
                value,
                depth,
                iterators: positioned_iterators,
                hook_iterators,
            }),
        }
    }
    Ok(flattened)
}

pub(super) fn spl_hook_iterators_for_position(
    hook_ancestors: &[ObjectRef],
    current_iterator: Option<&ObjectRef>,
    position: usize,
) -> Vec<ObjectRef> {
    let mut hook_iterators = hook_ancestors.to_vec();
    if let Some(current_iterator) = current_iterator {
        hook_iterators.push(spl_iterator_snapshot_at_position(
            current_iterator,
            position,
        ));
    }
    hook_iterators
}

pub(super) fn spl_iterator_snapshot_at_position(source: &ObjectRef, position: usize) -> ObjectRef {
    let snapshot = source.clone_shallow();
    spl_set_position(&snapshot, position);
    snapshot.set_property(
        "__snapshot_source_id",
        Value::Int(spl_iterator_context_id(source) as i64),
    );
    if let Some(Value::Object(inner)) = source
        .get_property("__inner_iterator")
        .map(|value| effective_value(&value))
        && spl_runtime_marker(&inner).is_some()
    {
        snapshot.set_property(
            "__inner_iterator",
            Value::Object(spl_iterator_snapshot_at_position(&inner, position)),
        );
    }
    snapshot
}

pub(super) fn spl_iterator_context_id(object: &ObjectRef) -> u64 {
    object
        .get_property("__snapshot_source_id")
        .and_then(|value| match effective_value(&value) {
            Value::Int(id) if id >= 0 => Some(id as u64),
            _ => None,
        })
        .unwrap_or_else(|| object.id())
}

pub(super) fn spl_recursive_array_child_context_value(
    hook_iterators: &[ObjectRef],
    value: &Value,
) -> Result<Value, String> {
    let Some(parent) = hook_iterators.last() else {
        return Ok(value.clone());
    };
    if spl_runtime_marker(parent).as_deref() != Some("recursivearrayiterator") {
        return Ok(value.clone());
    }
    let child = parent.clone_shallow();
    spl_set_entries(&child, spl_entries_from_value(value)?);
    spl_set_position(&child, 0);
    child.set_property("__snapshot_source_id", Value::Int(child.id() as i64));
    child.set_property(
        SPL_RUNTIME_CLASS_PROPERTY,
        Value::string(b"recursivearrayiterator".to_vec()),
    );
    Ok(Value::Object(child))
}

pub(super) fn spl_recursive_tree_entries_with_context_from_value(
    value: &Value,
    depth: i64,
    ancestors: Vec<ObjectRef>,
) -> Result<Vec<SplRecursiveEntry>, String> {
    if let Value::Object(object) = effective_value(value)
        && spl_runtime_marker(&object).as_deref() == Some("arrayobject")
        && normalize_class_name(&spl_array_object_iterator_class(&object))
            != "recursivearrayiterator"
    {
        return Err(
            "E_PHP_VM_SPL_INVALID_ARGUMENT: An instance of RecursiveIterator or IteratorAggregate creating it is required"
                .to_owned(),
        );
    }

    let (entries, current_iterator) = spl_recursive_context_entries_from_value(value)?;
    let mut iterators = ancestors;
    if let Some(current_iterator) = current_iterator.as_ref() {
        iterators.push(current_iterator.clone());
    }

    let mut flattened = Vec::new();
    for (position, (key, value)) in entries.into_iter().enumerate() {
        let hook_iterators =
            spl_hook_iterators_for_position(&iterators, current_iterator.as_ref(), position);
        flattened.push(SplRecursiveEntry {
            key: key.clone(),
            value: value.clone(),
            depth,
            iterators: iterators.clone(),
            hook_iterators: hook_iterators.clone(),
        });
        match effective_value(&value) {
            Value::Array(_) => {
                flattened.extend(spl_recursive_tree_entries_with_context_from_value(
                    &value,
                    depth + 1,
                    iterators.clone(),
                )?);
            }
            Value::Object(object)
                if spl_runtime_marker(&object).as_deref() == Some("recursivearrayiterator") =>
            {
                flattened.extend(spl_recursive_tree_entries_with_context_from_value(
                    &Value::Object(object),
                    depth + 1,
                    iterators.clone(),
                )?);
            }
            _ => {}
        }
    }
    Ok(flattened)
}

pub(super) fn spl_recursive_caching_entries_with_context_from_object(
    object: &ObjectRef,
    depth: i64,
    ancestors: Vec<ObjectRef>,
) -> Result<Vec<SplRecursiveEntry>, String> {
    let entries = spl_entries(object);
    let mut flattened = Vec::new();
    for (position, (key, value)) in entries.iter().cloned().enumerate() {
        let mut iterators = ancestors.clone();
        iterators.push(spl_recursive_caching_iterator_snapshot(
            object,
            entries.clone(),
            position,
        ));
        if matches!(effective_value(&value), Value::Array(_)) {
            let child = spl_recursive_caching_child_iterator(object, &value)?;
            flattened.extend(spl_recursive_caching_entries_with_context_from_object(
                &child,
                depth + 1,
                iterators,
            )?);
        } else {
            let hook_iterators = iterators.clone();
            flattened.push(SplRecursiveEntry {
                key,
                value,
                depth,
                iterators,
                hook_iterators,
            });
        }
    }
    Ok(flattened)
}

pub(super) fn spl_recursive_caching_iterator_snapshot(
    source: &ObjectRef,
    entries: Vec<(ArrayKey, Value)>,
    position: usize,
) -> ObjectRef {
    let snapshot = ObjectRef::new_with_display_name(
        &spl_iterator_class("RecursiveCachingIterator"),
        spl_iterator_display_name("RecursiveCachingIterator"),
    );
    spl_set_entries(&snapshot, entries);
    spl_set_position(&snapshot, position);
    snapshot.set_property(
        SPL_RUNTIME_CLASS_PROPERTY,
        Value::string(b"recursivecachingiterator".to_vec()),
    );
    snapshot.set_property(
        "__caching_flags",
        Value::Int(spl_caching_iterator_flags(source)),
    );
    snapshot.set_property("__caching_seen_count", Value::Int(0));
    snapshot.set_property("__caching_cache", Value::Array(PhpArray::new()));
    if let Some(Value::Object(inner)) = source
        .get_property("__inner_iterator")
        .map(|value| effective_value(&value))
    {
        let inner = inner.clone_shallow();
        spl_set_position(&inner, position);
        snapshot.set_property("__inner_iterator", Value::Object(inner));
    }
    snapshot
}

pub(super) fn spl_recursive_caching_inner_iterator(object: &ObjectRef) -> Option<ObjectRef> {
    if spl_runtime_marker(object).as_deref() != Some("recursivecachingiterator") {
        return None;
    }
    object
        .get_property("__inner_iterator")
        .and_then(|value| match effective_value(&value) {
            Value::Object(inner) => Some(inner),
            _ => None,
        })
}

pub(super) fn spl_recursive_caching_child_iterator(
    source: &ObjectRef,
    value: &Value,
) -> Result<ObjectRef, String> {
    let entries = spl_entries_from_value(value)?;
    let snapshot = spl_recursive_caching_iterator_snapshot(source, entries, 0);
    if let Some(Value::Object(inner)) = source
        .get_property("__inner_iterator")
        .map(|value| effective_value(&value))
        && spl_runtime_marker(&inner).as_deref() == Some("recursivearrayiterator")
    {
        let child = inner.clone_shallow();
        spl_set_entries(&child, spl_entries_from_value(value)?);
        spl_set_position(&child, 0);
        child.set_property(
            SPL_RUNTIME_CLASS_PROPERTY,
            Value::string(b"recursivearrayiterator".to_vec()),
        );
        snapshot.set_property("__inner_iterator", Value::Object(child));
    }
    Ok(snapshot)
}

type SplRecursiveContextEntries = (Vec<(ArrayKey, Value)>, Option<ObjectRef>);

pub(super) fn spl_recursive_context_entries_from_value(
    value: &Value,
) -> Result<SplRecursiveContextEntries, String> {
    match effective_value(value) {
        Value::Array(_) => {
            let iterator = ObjectRef::new_with_display_name(
                &spl_iterator_class("RecursiveArrayIterator"),
                spl_iterator_display_name("RecursiveArrayIterator"),
            );
            let entries = spl_entries_from_value(value)?;
            spl_set_entries(&iterator, entries.clone());
            spl_set_position(&iterator, 0);
            Ok((entries, Some(iterator)))
        }
        Value::Object(object)
            if spl_runtime_marker(&object).as_deref() == Some("recursivearrayiterator") =>
        {
            Ok((spl_entries(&object), Some(object)))
        }
        Value::Object(object)
            if spl_runtime_marker(&object)
                .is_some_and(|class| is_spl_iterator_runtime_class(&class)) =>
        {
            Ok((spl_entries(&object), Some(object)))
        }
        _ => Ok((spl_entries_from_value(value)?, None)),
    }
}

pub(super) fn spl_value_has_recursive_children(value: &Value) -> bool {
    match effective_value(value) {
        Value::Array(_) => true,
        Value::Object(object) => {
            spl_runtime_marker(&object).as_deref() == Some("recursivearrayiterator")
        }
        _ => false,
    }
}

pub(super) struct SplRegexPattern {
    body: String,
    case_insensitive: bool,
}

impl SplRegexPattern {
    fn parse(pattern: &str) -> Self {
        if pattern.len() >= 2 {
            let mut chars = pattern.chars();
            let delimiter = chars.next().unwrap_or('/');
            if !delimiter.is_ascii_alphanumeric()
                && !delimiter.is_ascii_whitespace()
                && delimiter != '\\'
                && let Some(end) = pattern.rfind(delimiter)
                && end > delimiter.len_utf8() - 1
            {
                let body_start = delimiter.len_utf8();
                let modifiers_start = end + delimiter.len_utf8();
                let body = pattern[body_start..end].to_owned();
                let modifiers = &pattern[modifiers_start..];
                return Self {
                    body,
                    case_insensitive: modifiers.contains('i'),
                };
            }
        }

        Self {
            body: pattern.to_owned(),
            case_insensitive: false,
        }
    }

    fn matches(&self, subject: &str) -> bool {
        if self.body.contains("\\d") {
            return self.captures(subject).is_some();
        }

        let mut body = self.body.as_str();
        if body.is_empty() || body == ".*" || body == ".+" {
            return true;
        }

        let anchored_start = body.starts_with('^');
        let anchored_end = body.ends_with('$');
        if anchored_start {
            body = &body[1..];
        }
        if anchored_end {
            body = &body[..body.len().saturating_sub(1)];
        }

        let wildcard_prefix = body.starts_with(".*") || body.starts_with(".+");
        let requires_non_empty = body.starts_with(".+");
        if wildcard_prefix {
            body = &body[2..];
        }
        let wildcard_suffix = body.ends_with(".*") || body.ends_with(".+");
        if wildcard_suffix {
            body = &body[..body.len().saturating_sub(2)];
        }

        let literal = spl_regex_literal_text(body);
        let subject_text;
        let literal_text;
        let (subject, literal) = if self.case_insensitive {
            subject_text = subject.to_ascii_lowercase();
            literal_text = literal.to_ascii_lowercase();
            (subject_text.as_str(), literal_text.as_str())
        } else {
            (subject, literal.as_str())
        };
        if literal.is_empty() {
            return !requires_non_empty || !subject.is_empty();
        }
        if requires_non_empty && subject.is_empty() {
            return false;
        }

        match (
            anchored_start && !wildcard_prefix,
            anchored_end && !wildcard_suffix,
        ) {
            (true, true) => subject == literal,
            (true, false) => subject.starts_with(literal),
            (false, true) => subject.ends_with(literal),
            (false, false) => subject.contains(literal),
        }
    }

    fn captures(&self, subject: &str) -> Option<Vec<String>> {
        self.all_captures(subject)
            .into_iter()
            .next()
            .map(|captures| captures.into_iter().map(|(_, value)| value).collect())
    }

    fn all_captures(&self, subject: &str) -> Vec<Vec<(usize, String)>> {
        match self.body.as_str() {
            "(\\d)" => subject
                .chars()
                .filter(|ch| ch.is_ascii_digit())
                .map(|ch| vec![(0, ch.to_string()), (1, ch.to_string())])
                .collect(),
            "(\\d),(\\d)" => {
                let chars = subject.char_indices().collect::<Vec<_>>();
                let mut captures = Vec::new();
                let mut last_end = 0;
                for window in chars.windows(3) {
                    let [(_, left), (_, comma), (_, right)] = window else {
                        continue;
                    };
                    let [(left_index, _), _, (right_index, _)] = window else {
                        continue;
                    };
                    let end = right_index + right.len_utf8();
                    if *left_index >= last_end
                        && left.is_ascii_digit()
                        && *comma == ','
                        && right.is_ascii_digit()
                    {
                        captures.push(vec![
                            (0, format!("{left},{right}")),
                            (1, left.to_string()),
                            (2, right.to_string()),
                        ]);
                        last_end = end;
                    }
                }
                captures
            }
            _ => Vec::new(),
        }
    }

    fn split(&self, subject: &str) -> Option<Vec<String>> {
        let ranges = self.match_ranges(subject);
        if ranges.is_empty() {
            return None;
        }
        let mut pieces = Vec::new();
        let mut start = 0;
        for (match_start, match_end) in ranges {
            pieces.push(subject[start..match_start].to_owned());
            start = match_end;
        }
        pieces.push(subject[start..].to_owned());
        Some(pieces)
    }

    fn match_ranges(&self, subject: &str) -> Vec<(usize, usize)> {
        match self.body.as_str() {
            "," => subject
                .char_indices()
                .filter_map(|(index, ch)| (ch == ',').then_some((index, index + ch.len_utf8())))
                .collect(),
            "(\\d),(\\d)" => {
                let chars = subject.char_indices().collect::<Vec<_>>();
                let mut ranges = Vec::new();
                let mut last_end = 0;
                for window in chars.windows(3) {
                    let [(left_index, left), (_, comma), (right_index, right)] = window else {
                        continue;
                    };
                    let end = right_index + right.len_utf8();
                    if *left_index >= last_end
                        && left.is_ascii_digit()
                        && *comma == ','
                        && right.is_ascii_digit()
                    {
                        ranges.push((*left_index, end));
                        last_end = end;
                    }
                }
                ranges
            }
            _ => Vec::new(),
        }
    }

    fn capture_group_count(&self) -> usize {
        match self.body.as_str() {
            "(\\d)" => 2,
            "(\\d),(\\d)" => 3,
            _ => 1,
        }
    }
}

pub(super) fn spl_regex_literal_text(body: &str) -> String {
    let mut literal = String::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                if matches!(next, '.' | '/' | '\\' | '-' | '_' | '$' | '^') {
                    literal.push(next);
                } else {
                    literal.push('\\');
                    literal.push(next);
                }
            } else {
                literal.push(ch);
            }
        } else {
            literal.push(ch);
        }
    }
    literal
}

pub(super) fn spl_regex_subject_text(key: &ArrayKey, value: &Value, use_key: bool) -> String {
    if use_key {
        let key_value = array_key_to_value(key.clone());
        return to_string(&key_value)
            .map(|text| text.to_string_lossy())
            .unwrap_or_default();
    }

    spl_directory_path_from_value(value).unwrap_or_else(|| {
        to_string(value)
            .map(|text| text.to_string_lossy())
            .unwrap_or_default()
    })
}

pub(super) fn spl_regex_match_array(captures: Vec<String>) -> Value {
    let mut matches = PhpArray::new();
    for (index, capture) in captures.into_iter().enumerate() {
        matches.insert(
            ArrayKey::Int(index as i64),
            Value::string(capture.into_bytes()),
        );
    }
    Value::Array(matches)
}

pub(super) fn spl_regex_all_matches_array(captures: Vec<Vec<(usize, String)>>) -> Value {
    let mut matches = PhpArray::new();
    for capture_set in captures {
        for (group, capture) in capture_set {
            let key = ArrayKey::Int(group as i64);
            let mut group_matches = matches
                .get(&key)
                .and_then(|value| match effective_value(value) {
                    Value::Array(array) => Some(array),
                    _ => None,
                })
                .unwrap_or_default();
            group_matches.append(Value::string(capture.into_bytes()));
            matches.insert(key, Value::Array(group_matches));
        }
    }
    Value::Array(matches)
}

pub(super) fn spl_regex_split_array(pieces: Vec<String>) -> Value {
    let mut matches = PhpArray::new();
    for piece in pieces {
        matches.append(Value::string(piece.into_bytes()));
    }
    Value::Array(matches)
}

pub(super) fn spl_regex_current_value(
    object: &ObjectRef,
    key: &ArrayKey,
    value: Value,
) -> (bool, Value) {
    let pattern_text = object
        .get_property("__regex")
        .and_then(|value| match effective_value(&value) {
            Value::String(pattern) => Some(pattern.to_string_lossy()),
            _ => None,
        })
        .unwrap_or_default();
    let pattern = SplRegexPattern::parse(&pattern_text);
    let mode = object
        .get_property("__regex_mode")
        .and_then(|value| match effective_value(&value) {
            Value::Int(mode) => Some(mode),
            _ => None,
        })
        .unwrap_or(SPL_REGEX_MATCH);
    let flags = object
        .get_property("__regex_flags")
        .and_then(|value| match effective_value(&value) {
            Value::Int(flags) => Some(flags),
            _ => None,
        })
        .unwrap_or(0);
    let use_key = flags & SPL_REGEX_USE_KEY != 0;
    let invert_match = flags & SPL_REGEX_INVERT_MATCH != 0;
    let subject = spl_regex_subject_text(key, &value, use_key);
    let captures = pattern.captures(&subject);
    let all_captures = if mode == SPL_REGEX_ALL_MATCHES {
        pattern.all_captures(&subject)
    } else {
        Vec::new()
    };
    let matched = captures.is_some() || !all_captures.is_empty() || pattern.matches(&subject);
    let accepted = matched != invert_match;
    let current = match mode {
        SPL_REGEX_GET_MATCH => captures.map(spl_regex_match_array).unwrap_or_else(|| {
            if accepted {
                spl_regex_match_array(vec![subject.clone()])
            } else {
                spl_regex_match_array(Vec::new())
            }
        }),
        SPL_REGEX_ALL_MATCHES => {
            if all_captures.is_empty() {
                if accepted {
                    spl_regex_all_matches_array(vec![vec![(0, subject.clone())]])
                } else {
                    spl_regex_empty_all_matches_array(pattern.capture_group_count())
                }
            } else {
                spl_regex_all_matches_array(all_captures)
            }
        }
        SPL_REGEX_SPLIT => pattern
            .split(&subject)
            .map(spl_regex_split_array)
            .unwrap_or_else(|| spl_regex_split_array(vec![subject])),
        SPL_REGEX_MATCH | SPL_REGEX_REPLACE => value,
        _ => value,
    };
    (accepted, current)
}

pub(super) fn spl_regex_empty_all_matches_array(group_count: usize) -> Value {
    let mut matches = PhpArray::new();
    for group in 0..group_count {
        matches.insert(ArrayKey::Int(group as i64), Value::Array(PhpArray::new()));
    }
    Value::Array(matches)
}

pub(super) fn spl_entries_to_php_array(entries: Vec<(ArrayKey, Value)>) -> PhpArray {
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(key, value);
    }
    array
}

pub(super) fn spl_entries_to_debug_php_array_excluding(
    entries: Vec<(ArrayKey, Value)>,
    excluded_object_id: u64,
) -> PhpArray {
    let excluded_object_id = (excluded_object_id != 0).then_some(excluded_object_id);
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(key, spl_debug_view_value(value, excluded_object_id));
    }
    array
}

pub(super) fn spl_entries(object: &ObjectRef) -> Vec<(ArrayKey, Value)> {
    spl_entries_from_property(object, "__entries")
}

pub(super) fn spl_entries_from_property(
    object: &ObjectRef,
    property: &str,
) -> Vec<(ArrayKey, Value)> {
    let Some(Value::Array(entries)) = object.get_property(property) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(_, entry)| {
            let Value::Array(pair) = effective_value(entry) else {
                return None;
            };
            let key = pair.get(&ArrayKey::Int(0)).and_then(ArrayKey::from_value)?;
            let value = pair.get(&ArrayKey::Int(1)).map(effective_value)?;
            Some((key, value))
        })
        .collect()
}

pub(super) fn spl_set_entries(object: &ObjectRef, entries: Vec<(ArrayKey, Value)>) {
    let packed = entries
        .into_iter()
        .map(|(key, value)| Value::packed_array(vec![array_key_to_value(key), value]))
        .collect();
    object.set_property("__entries", Value::packed_array(packed));
}

pub(super) fn spl_set_entry_depths(object: &ObjectRef, depths: Vec<i64>) {
    object.set_property(
        "__entry_depths",
        Value::packed_array(depths.into_iter().map(Value::Int).collect()),
    );
}

pub(super) fn spl_entry_depths(object: &ObjectRef) -> Vec<i64> {
    object
        .get_property("__entry_depths")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(
                array
                    .iter()
                    .filter_map(|(_, value)| match effective_value(value) {
                        Value::Int(depth) => Some(depth),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_set_sub_iterators(object: &ObjectRef, iterators: Vec<Vec<ObjectRef>>) {
    let packed = iterators
        .into_iter()
        .map(|levels| {
            Value::packed_array(levels.into_iter().map(Value::Object).collect::<Vec<_>>())
        })
        .collect();
    object.set_property("__sub_iterators", Value::packed_array(packed));
}

pub(super) fn spl_sub_iterators(object: &ObjectRef) -> Vec<Vec<ObjectRef>> {
    object
        .get_property("__sub_iterators")
        .and_then(|value| match effective_value(&value) {
            Value::Array(entries) => Some(
                entries
                    .iter()
                    .map(|(_, entry)| match effective_value(entry) {
                        Value::Array(levels) => levels
                            .iter()
                            .filter_map(|(_, level)| match effective_value(level) {
                                Value::Object(iterator) => Some(iterator),
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_set_hook_iterators(object: &ObjectRef, iterators: Vec<Vec<ObjectRef>>) {
    let packed = iterators
        .into_iter()
        .map(|levels| {
            Value::packed_array(levels.into_iter().map(Value::Object).collect::<Vec<_>>())
        })
        .collect();
    object.set_property("__rii_hook_iterators", Value::packed_array(packed));
}

pub(super) fn spl_hook_iterators(object: &ObjectRef) -> Vec<Vec<ObjectRef>> {
    object
        .get_property("__rii_hook_iterators")
        .and_then(|value| match effective_value(&value) {
            Value::Array(entries) => Some(
                entries
                    .iter()
                    .map(|(_, entry)| match effective_value(entry) {
                        Value::Array(levels) => levels
                            .iter()
                            .filter_map(|(_, level)| match effective_value(level) {
                                Value::Object(iterator) => Some(iterator),
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_current_sub_iterator(object: &ObjectRef) -> Option<ObjectRef> {
    let position = spl_position(object);
    let sub_iterators = spl_sub_iterators(object);
    let context_position = position.min(sub_iterators.len().saturating_sub(1));
    sub_iterators
        .into_iter()
        .nth(context_position)
        .and_then(|iterators| iterators.into_iter().next_back())
}

pub(super) fn spl_rii_sub_iterator_branch_changed(
    object: &ObjectRef,
    previous_position: usize,
    current_position: usize,
    depth: i64,
) -> bool {
    let level = depth as usize;
    let sub_iterators = spl_sub_iterators(object);
    let previous = sub_iterators
        .get(previous_position)
        .and_then(|iterators| iterators.get(level));
    let current = sub_iterators
        .get(current_position)
        .and_then(|iterators| iterators.get(level));
    match (previous, current) {
        (Some(previous), Some(current))
            if spl_iterator_context_id(previous) != spl_iterator_context_id(current) =>
        {
            return true;
        }
        _ => {}
    }

    if depth <= 0 {
        return false;
    }
    let parent_level = depth.saturating_sub(1) as usize;
    let hook_iterators = spl_hook_iterators(object);
    let previous_parent = hook_iterators
        .get(previous_position)
        .and_then(|iterators| iterators.get(parent_level));
    let current_parent = hook_iterators
        .get(current_position)
        .and_then(|iterators| iterators.get(parent_level));
    match (previous_parent, current_parent) {
        (Some(previous), Some(current)) => {
            spl_iterator_context_id(previous) != spl_iterator_context_id(current)
                || spl_position(previous) != spl_position(current)
        }
        _ => false,
    }
}

pub(super) fn spl_rii_first_changed_iterator_level(
    object: &ObjectRef,
    previous_position: usize,
    current_position: usize,
) -> Option<i64> {
    let sub_iterators = spl_sub_iterators(object);
    let previous = sub_iterators.get(previous_position)?;
    let current = sub_iterators.get(current_position)?;
    let max_len = previous.len().max(current.len());
    for level in 0..max_len {
        match (previous.get(level), current.get(level)) {
            (Some(previous), Some(current))
                if spl_iterator_context_id(previous) == spl_iterator_context_id(current) => {}
            _ => return Some(level as i64),
        }
    }
    None
}

pub(super) fn spl_append_iterators_array(object: &ObjectRef) -> PhpArray {
    object
        .get_property("__append_iterators")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(array),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_append_iterators(object: &ObjectRef) -> Vec<ObjectRef> {
    spl_append_iterators_array(object)
        .iter()
        .filter_map(|(_, value)| match effective_value(value) {
            Value::Object(iterator) => Some(iterator),
            _ => None,
        })
        .collect()
}

pub(super) fn spl_append_entry_indices_array(object: &ObjectRef) -> PhpArray {
    object
        .get_property("__append_entry_iterator_indices")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(array),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_append_entry_iterator_indices(object: &ObjectRef) -> Vec<i64> {
    spl_append_entry_indices_array(object)
        .iter()
        .filter_map(|(_, value)| match effective_value(value) {
            Value::Int(index) => Some(index),
            _ => None,
        })
        .collect()
}

pub(super) fn spl_append_rewound_iterator_ids(object: &ObjectRef) -> Vec<i64> {
    object
        .get_property("__append_rewound_iterator_ids")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(
                array
                    .iter()
                    .filter_map(|(_, value)| match effective_value(value) {
                        Value::Int(id) => Some(id),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_append_note_rewound_iterator_id(object: &ObjectRef, id: i64) {
    let mut ids = object
        .get_property("__append_rewound_iterator_ids")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(array),
            _ => None,
        })
        .unwrap_or_default();
    ids.append(Value::Int(id));
    object.set_property("__append_rewound_iterator_ids", Value::Array(ids));
}

pub(super) fn spl_position(object: &ObjectRef) -> usize {
    object
        .get_property("__position")
        .and_then(|value| match effective_value(&value) {
            Value::Int(value) if value > 0 => Some(value as usize),
            _ => None,
        })
        .unwrap_or(0)
}

pub(super) fn spl_set_position(object: &ObjectRef, position: usize) {
    object.set_property("__position", Value::Int(position as i64));
}

pub(super) fn spl_limit_offset(object: &ObjectRef) -> usize {
    object
        .get_property("__limit_offset")
        .and_then(|value| match effective_value(&value) {
            Value::Int(value) if value > 0 => Some(value as usize),
            _ => None,
        })
        .unwrap_or(0)
}

pub(super) fn spl_limit_count(object: &ObjectRef) -> Option<usize> {
    object
        .get_property("__limit_count")
        .and_then(|value| match effective_value(&value) {
            Value::Int(value) if value >= 0 => Some(value as usize),
            _ => None,
        })
}

pub(super) fn spl_current_entry(object: &ObjectRef) -> Option<(ArrayKey, Value)> {
    spl_entries(object).into_iter().nth(spl_position(object))
}

pub(super) fn spl_regex_accept_pre_parent(object: &ObjectRef) -> bool {
    object
        .get_property("__regex_accept_pre_parent")
        .is_some_and(|value| matches!(effective_value(&value), Value::Bool(true)))
}

pub(super) fn spl_regex_last_accept_rejected(object: &ObjectRef) -> bool {
    object
        .get_property("__regex_last_accept_result")
        .is_some_and(|value| matches!(effective_value(&value), Value::Bool(false)))
}

pub(super) fn spl_regex_uses_key(object: &ObjectRef) -> bool {
    object
        .get_property("__regex_flags")
        .and_then(|value| match effective_value(&value) {
            Value::Int(flags) => Some(flags & SPL_REGEX_USE_KEY != 0),
            _ => None,
        })
        .unwrap_or(false)
}

pub(super) fn spl_rii_direct_at_root(object: &ObjectRef) -> bool {
    object
        .get_property("__rii_direct_at_root")
        .is_some_and(|value| matches!(effective_value(&value), Value::Bool(true)))
}

pub(super) fn spl_rii_should_use_direct_root(object: &ObjectRef) -> bool {
    let Some(Value::Object(inner)) = object
        .get_property("__inner_iterator")
        .map(|value| effective_value(&value))
    else {
        return true;
    };

    spl_runtime_marker(&inner).as_deref() != Some("parentiterator")
        && spl_current_entry(&inner)
            .map(|(_, value)| spl_value_has_recursive_children(&value))
            .unwrap_or(false)
}

pub(super) fn spl_rii_direct_root_consumed(object: &ObjectRef) -> bool {
    object
        .get_property("__rii_direct_root_consumed")
        .is_some_and(|value| matches!(effective_value(&value), Value::Bool(true)))
}

pub(super) fn spl_rii_max_depth(object: &ObjectRef) -> i64 {
    object
        .get_property("__max_depth")
        .and_then(|value| match effective_value(&value) {
            Value::Int(depth) => Some(depth),
            _ => None,
        })
        .unwrap_or(-1)
}

pub(super) fn spl_rii_flags(object: &ObjectRef) -> i64 {
    object
        .get_property("__rii_flags")
        .and_then(|value| match effective_value(&value) {
            Value::Int(flags) => Some(flags),
            _ => None,
        })
        .unwrap_or(0)
}

pub(super) fn spl_rti_flags(object: &ObjectRef) -> i64 {
    object
        .get_property("__rti_flags")
        .and_then(|value| match effective_value(&value) {
            Value::Int(flags) => Some(flags),
            _ => None,
        })
        .unwrap_or(SPL_RTI_BYPASS_KEY)
}

pub(super) fn spl_rti_default_prefix_parts_value() -> Value {
    Value::packed_array(vec![
        Value::string(Vec::new()),
        Value::string(b"| ".to_vec()),
        Value::string(b"  ".to_vec()),
        Value::string(b"|-".to_vec()),
        Value::string(b"\\-".to_vec()),
        Value::string(Vec::new()),
    ])
}

pub(super) fn spl_rti_prefix_parts(object: &ObjectRef) -> [String; 6] {
    let mut parts = [
        String::new(),
        "| ".to_owned(),
        "  ".to_owned(),
        "|-".to_owned(),
        "\\-".to_owned(),
        String::new(),
    ];
    if let Some(Value::Array(array)) = object.get_property("__rti_prefix_parts") {
        for (index, part) in parts.iter_mut().enumerate() {
            if let Some(value) = array.get(&ArrayKey::Int(index as i64))
                && let Ok(text) = to_string(value)
            {
                *part = text.to_string_lossy();
            }
        }
    }
    parts
}

pub(super) fn spl_rti_set_prefix_part(
    object: &ObjectRef,
    part: usize,
    value: Value,
) -> Result<(), String> {
    let mut values = spl_rti_prefix_parts(object)
        .into_iter()
        .map(|text| Value::string(text.into_bytes()))
        .collect::<Vec<_>>();
    values[part] = Value::string(to_string(&value)?.to_string_lossy().into_bytes());
    object.set_property("__rti_prefix_parts", Value::packed_array(values));
    Ok(())
}

pub(super) fn spl_rti_prefix(object: &ObjectRef) -> String {
    let position = spl_position(object);
    let depths = spl_entry_depths(object);
    let depth = depths.get(position).copied().unwrap_or(0).max(0) as usize;
    let parts = spl_rti_prefix_parts(object);
    let mut prefix = parts[0].clone();
    for level in 0..depth {
        if spl_rti_has_later_sibling_at_depth(&depths, position, level as i64) {
            prefix.push_str(&parts[1]);
        } else {
            prefix.push_str(&parts[2]);
        }
    }
    if spl_rti_has_later_sibling_at_depth(&depths, position, depth as i64) {
        prefix.push_str(&parts[3]);
    } else {
        prefix.push_str(&parts[4]);
    }
    prefix.push_str(&parts[5]);
    prefix
}

pub(super) fn spl_rti_has_later_sibling_at_depth(
    depths: &[i64],
    position: usize,
    depth: i64,
) -> bool {
    for later_depth in depths.iter().skip(position.saturating_add(1)).copied() {
        if later_depth < depth {
            return false;
        }
        if later_depth == depth {
            return true;
        }
    }
    false
}

pub(super) fn spl_rti_postfix(object: &ObjectRef) -> String {
    object
        .get_property("__postfix")
        .and_then(|value| to_string(&value).ok().map(|text| text.to_string_lossy()))
        .unwrap_or_default()
}

pub(super) fn spl_rti_entry_text(value: &Value) -> Result<String, String> {
    match effective_value(value) {
        Value::Array(_) => Ok("Array".to_owned()),
        Value::Object(object) => Err(format!(
            "E_PHP_RUNTIME_OBJECT_TO_STRING_GAP: \nDeprecated: ArrayIterator::__construct(): Using an object as a backing array for ArrayIterator is deprecated, as it allows violating class constraints and invariants in unknown on line 0\nObject of class {} could not be converted to string",
            object.display_name()
        )),
        other => Ok(to_string(&other)?.to_string_lossy()),
    }
}

pub(super) fn spl_rii_catches_get_child(object: &ObjectRef) -> bool {
    spl_rii_flags(object) & SPL_RII_CATCH_GET_CHILD != 0
}

pub(super) fn spl_rii_root_current(object: &ObjectRef) -> Option<Value> {
    let Value::Object(inner) = object
        .get_property("__inner_iterator")
        .map(|value| effective_value(&value))?
    else {
        return None;
    };
    spl_current_entry(&inner).map(|(_, value)| value)
}

pub(super) fn spl_rii_root_key(object: &ObjectRef) -> Option<Value> {
    let Value::Object(inner) = object
        .get_property("__inner_iterator")
        .map(|value| effective_value(&value))?
    else {
        return None;
    };
    spl_current_entry(&inner).map(|(key, _)| array_key_to_value(key))
}

pub(super) fn spl_rii_call_get_children_target(object: &ObjectRef) -> Option<ObjectRef> {
    if spl_rii_direct_at_root(object)
        && let Some(Value::Object(inner)) = object
            .get_property("__inner_iterator")
            .map(|value| effective_value(&value))
    {
        return Some(inner);
    }

    let position = spl_position(object);
    let hook_iterators = spl_hook_iterators(object);
    let sub_iterators = if hook_iterators.is_empty() {
        spl_sub_iterators(object)
    } else {
        hook_iterators
    };
    let iterators = sub_iterators.into_iter().nth(position)?;
    if let Some(depth) = spl_rii_hook_depth(object)
        && depth >= 0
    {
        return iterators.into_iter().nth(depth as usize);
    }
    iterators.into_iter().last()
}

pub(super) fn spl_rii_hook_depth(object: &ObjectRef) -> Option<i64> {
    object
        .get_property("__rii_hook_depth")
        .and_then(|value| match effective_value(&value) {
            Value::Int(depth) => Some(depth),
            _ => None,
        })
}

pub(super) fn spl_rii_notified_position(object: &ObjectRef) -> Option<i64> {
    object
        .get_property("__rii_notified_position")
        .and_then(|value| match effective_value(&value) {
            Value::Int(position) => Some(position),
            _ => None,
        })
}

pub(super) fn spl_rii_end_iteration_called(object: &ObjectRef) -> bool {
    object
        .get_property("__rii_end_iteration_called")
        .is_some_and(|value| matches!(effective_value(&value), Value::Bool(true)))
}

pub(super) fn spl_rii_iteration_active(object: &ObjectRef) -> bool {
    object
        .get_property("__rii_iteration_active")
        .is_some_and(|value| matches!(effective_value(&value), Value::Bool(true)))
}

pub(super) fn spl_rii_child_hook_entered_positions(object: &ObjectRef) -> PhpArray {
    object
        .get_property("__rii_entered_child_positions")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(array),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_rii_child_hook_checked_positions(object: &ObjectRef) -> PhpArray {
    object
        .get_property("__rii_checked_child_positions")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(array),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_rii_child_hook_checked_results(object: &ObjectRef) -> PhpArray {
    object
        .get_property("__rii_checked_child_results")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(array),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_rii_child_hook_checked_result(object: &ObjectRef, position: i64) -> Option<bool> {
    spl_rii_child_hook_checked_results(object)
        .iter()
        .find_map(|(_, value)| match effective_value(value) {
            Value::Array(entry) => {
                let key_matches = entry.get(&ArrayKey::Int(0)).is_some_and(
                    |value| matches!(effective_value(value), Value::Int(key) if key == position),
                );
                if !key_matches {
                    return None;
                }
                entry
                    .get(&ArrayKey::Int(1))
                    .and_then(|value| match effective_value(value) {
                        Value::Bool(result) => Some(result),
                        _ => None,
                    })
            }
            _ => None,
        })
}

pub(super) fn spl_rii_false_child_hook_depth_for_position(
    object: &ObjectRef,
    position: usize,
) -> Option<i64> {
    let base = (position as i64).saturating_mul(1024);
    spl_rii_child_hook_checked_results(object)
        .iter()
        .filter_map(|(_, value)| {
            let Value::Array(entry) = effective_value(value) else {
                return None;
            };
            let key = entry.get(&ArrayKey::Int(0)).and_then(|value| {
                if let Value::Int(key) = effective_value(value) {
                    Some(key)
                } else {
                    None
                }
            })?;
            let result = entry.get(&ArrayKey::Int(1)).and_then(|value| {
                if let Value::Bool(result) = effective_value(value) {
                    Some(result)
                } else {
                    None
                }
            })?;
            (!result && key >= base && key < base.saturating_add(1024)).then_some(key - base)
        })
        .min()
}

pub(super) fn spl_rii_pruned_parent_current_entry(object: &ObjectRef) -> Option<(ArrayKey, Value)> {
    let position = spl_position(object);
    let entry_depth = spl_entry_depths(object).get(position).copied().unwrap_or(0);
    let parent_depth = spl_rii_false_child_hook_depth_for_position(object, position)?;
    if parent_depth >= entry_depth {
        return None;
    }
    let parent = spl_hook_iterators(object)
        .into_iter()
        .nth(position)?
        .into_iter()
        .nth(parent_depth.max(0) as usize)?;
    spl_current_entry(&parent)
}

pub(super) fn spl_rii_active_call_get_children_current_entry(
    object: &ObjectRef,
) -> Option<(ArrayKey, Value)> {
    if !object
        .get_property("__rii_call_get_children_active")
        .is_some_and(|value| matches!(effective_value(&value), Value::Bool(true)))
    {
        return None;
    }
    let position = spl_position(object);
    let depth = spl_rii_hook_depth(object)?.max(0) as usize;
    let iterator = spl_hook_iterators(object)
        .into_iter()
        .nth(position)?
        .into_iter()
        .nth(depth)?;
    spl_current_entry(&iterator)
}

pub(super) fn spl_rii_note_child_hook_checked_result(
    object: &ObjectRef,
    position: i64,
    result: bool,
) {
    let mut results = spl_rii_child_hook_checked_results(object);
    results.append(Value::packed_array(vec![
        Value::Int(position),
        Value::Bool(result),
    ]));
    object.set_property("__rii_checked_child_results", Value::Array(results));
}

pub(super) fn spl_rii_active_child_depths(object: &ObjectRef) -> Vec<i64> {
    object
        .get_property("__rii_active_child_depths")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(
                array
                    .iter()
                    .filter_map(|(_, value)| match effective_value(value) {
                        Value::Int(depth) => Some(depth),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_rii_child_depth_is_active(object: &ObjectRef, depth: i64) -> bool {
    let depth = depth.max(0);
    spl_rii_active_child_depths(object)
        .into_iter()
        .any(|active_depth| active_depth == depth)
}

pub(super) fn spl_rii_note_active_child_depth(object: &ObjectRef, depth: i64) {
    let depth = depth.max(0);
    let mut depths = spl_rii_active_child_depths(object);
    depths.push(depth);
    object.set_property(
        "__rii_active_child_depths",
        Value::packed_array(depths.into_iter().map(Value::Int).collect()),
    );
}

pub(super) fn spl_rii_remove_active_child_depth(object: &ObjectRef, depth: i64) {
    let depth = depth.max(0);
    let mut depths = spl_rii_active_child_depths(object);
    if let Some(index) = depths
        .iter()
        .rposition(|active_depth| *active_depth == depth)
    {
        depths.remove(index);
    }
    object.set_property(
        "__rii_active_child_depths",
        Value::packed_array(depths.into_iter().map(Value::Int).collect()),
    );
}

pub(super) fn spl_rii_pruned_branches(object: &ObjectRef) -> PhpArray {
    object
        .get_property("__rii_pruned_branches")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(array),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_rii_pruned_leaf_position(object: &ObjectRef, position: usize) -> bool {
    let position = position as i64;
    spl_rii_pruned_branches(object)
        .iter()
        .any(|(_, value)| match effective_value(value) {
            Value::Array(branch) => branch.get(&ArrayKey::Int(0)).is_some_and(
                |value| matches!(effective_value(value), Value::Int(pos) if pos == position),
            ),
            _ => false,
        })
}

pub(super) fn spl_rii_should_call_valid_child_hook(object: &ObjectRef) -> bool {
    let position = spl_position(object);
    position < spl_entries(object).len() && !spl_rii_pruned_leaf_position(object, position)
}

pub(super) fn spl_rii_note_pruned_branch(object: &ObjectRef, position: usize, depth: i64) {
    let position = position as i64;
    if spl_rii_pruned_leaf_position(object, position as usize) {
        return;
    }
    let mut branches = spl_rii_pruned_branches(object);
    branches.append(Value::packed_array(vec![
        Value::Int(position),
        Value::Int(depth.max(0)),
    ]));
    object.set_property("__rii_pruned_branches", Value::Array(branches));
}

pub(super) fn spl_rii_skip_pruned_positions(object: &ObjectRef) {
    let depths = spl_entry_depths(object);
    let branches = spl_rii_pruned_branches(object);
    loop {
        let position = spl_position(object);
        let Some(current_depth) = depths.get(position).copied() else {
            return;
        };
        let should_skip = branches.iter().any(|(_, value)| {
            let Value::Array(branch) = effective_value(value) else {
                return false;
            };
            let Some(Value::Int(branch_position)) =
                branch.get(&ArrayKey::Int(0)).map(effective_value)
            else {
                return false;
            };
            let Some(Value::Int(branch_depth)) = branch.get(&ArrayKey::Int(1)).map(effective_value)
            else {
                return false;
            };
            position as i64 > branch_position && current_depth > branch_depth
        });
        if !should_skip {
            return;
        }
        spl_set_position(object, position.saturating_add(1));
    }
}

pub(super) fn spl_rii_skip_branch_at_position(
    object: &ObjectRef,
    branch_position: usize,
    branch_depth: i64,
) {
    let depths = spl_entry_depths(object);
    let mut position = branch_position;
    while position < depths.len()
        && (position == branch_position || depths[position] > branch_depth.max(0))
    {
        position = position.saturating_add(1);
    }
    spl_set_position(object, position);
}

pub(super) fn spl_rii_child_hook_entered_key(object: &ObjectRef, position: usize) -> i64 {
    let depth = spl_rii_hook_depth(object)
        .unwrap_or_else(|| spl_entry_depths(object).get(position).copied().unwrap_or(0));
    (position as i64).saturating_mul(1024) + depth.max(0)
}

pub(super) fn spl_rii_child_hook_was_entered(object: &ObjectRef, position: i64) -> bool {
    spl_rii_child_hook_entered_positions(object)
        .iter()
        .any(|(_, value)| matches!(effective_value(value), Value::Int(value) if value == position))
}

pub(super) fn spl_rii_child_hook_checked_key(object: &ObjectRef, position: usize) -> i64 {
    let depth = spl_rii_hook_depth(object)
        .unwrap_or_else(|| spl_entry_depths(object).get(position).copied().unwrap_or(0));
    (position as i64).saturating_mul(1024) + depth.max(0)
}

pub(super) fn spl_rii_child_hook_was_checked(object: &ObjectRef, position: i64) -> bool {
    spl_rii_child_hook_checked_positions(object)
        .iter()
        .any(|(_, value)| matches!(effective_value(value), Value::Int(value) if value == position))
}

pub(super) fn spl_rii_note_child_hook_entered(object: &ObjectRef, position: i64) {
    let mut positions = spl_rii_child_hook_entered_positions(object);
    positions.append(Value::Int(position));
    object.set_property("__rii_entered_child_positions", Value::Array(positions));
}

pub(super) fn spl_rii_note_child_hook_checked(object: &ObjectRef, position: i64) {
    let mut positions = spl_rii_child_hook_checked_positions(object);
    positions.append(Value::Int(position));
    object.set_property("__rii_checked_child_positions", Value::Array(positions));
}

pub(super) fn spl_rii_current_enters_recursive_caching_child(object: &ObjectRef) -> bool {
    let Some(Value::Object(inner)) = object
        .get_property("__inner_iterator")
        .map(|value| effective_value(&value))
    else {
        return false;
    };
    if spl_runtime_marker(&inner).as_deref() != Some("recursivecachingiterator") {
        return false;
    }

    let position = spl_position(object);
    let depths = spl_entry_depths(object);
    let Some(depth) = depths.get(position).copied() else {
        return false;
    };
    let previous_depth = position
        .checked_sub(1)
        .and_then(|previous| depths.get(previous).copied())
        .unwrap_or(0);
    depth > previous_depth
}

pub(super) fn spl_rii_array_string_warning_positions(object: &ObjectRef) -> PhpArray {
    object
        .get_property("__rii_array_string_warning_positions")
        .and_then(|value| match effective_value(&value) {
            Value::Array(array) => Some(array),
            _ => None,
        })
        .unwrap_or_default()
}

pub(super) fn spl_rii_array_string_warning_was_emitted(object: &ObjectRef) -> bool {
    let position = spl_position(object) as i64;
    spl_rii_array_string_warning_positions(object)
        .iter()
        .any(|(_, value)| matches!(effective_value(value), Value::Int(value) if value == position))
}

pub(super) fn spl_rii_note_array_string_warning(object: &ObjectRef) {
    let mut positions = spl_rii_array_string_warning_positions(object);
    positions.append(Value::Int(spl_position(object) as i64));
    object.set_property(
        "__rii_array_string_warning_positions",
        Value::Array(positions),
    );
}

pub(super) fn is_spl_container_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "arrayobject"
            | "splfixedarray"
            | "splobjectstorage"
            | "spldoublylinkedlist"
            | "splstack"
            | "splqueue"
    )
}

pub(super) fn internal_spl_container_instanceof(
    object_class: &str,
    target_class: &str,
) -> Option<bool> {
    if !is_spl_container_runtime_class(object_class) {
        return None;
    }
    let object_class = normalize_class_name(object_class);
    let target_class = normalize_class_name(target_class);
    Some(match target_class.as_str() {
        "traversable" | "iterator" | "countable" => true,
        "arrayaccess" => matches!(
            object_class.as_str(),
            "arrayobject" | "splfixedarray" | "splobjectstorage"
        ),
        "arrayobject" => object_class == "arrayobject",
        "splfixedarray" => object_class == "splfixedarray",
        "splobjectstorage" => object_class == "splobjectstorage",
        "spldoublylinkedlist" => matches!(
            object_class.as_str(),
            "spldoublylinkedlist" | "splstack" | "splqueue"
        ),
        "splstack" => object_class == "splstack",
        "splqueue" => object_class == "splqueue",
        _ => false,
    })
}

pub(super) fn new_spl_container_object(
    class_name: &str,
    args: Vec<CallArgument>,
) -> Result<ObjectRef, String> {
    if let Some(name) = args.iter().find_map(|arg| arg.name.as_deref()) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_NAMED_ARG: {class_name}::__construct has no builtin parameter ${name}"
        ));
    }
    let normalized = normalize_class_name(class_name);
    let object = ObjectRef::new_with_display_name(
        &spl_container_class(class_name),
        spl_container_display_name(class_name),
    );
    match normalized.as_str() {
        "arrayobject" => {
            validate_spl_constructor_arg_count(class_name, &args, 0, 3)?;
            let entries = args
                .first()
                .map(|arg| spl_entries_from_value(&arg.value))
                .transpose()?
                .unwrap_or_default();
            let flags = args
                .get(1)
                .map(|arg| to_int(&arg.value))
                .transpose()?
                .unwrap_or(0);
            let iterator_class = args
                .get(2)
                .map(|arg| to_string(&arg.value).map(|value| value.to_string_lossy()))
                .transpose()?
                .unwrap_or_else(|| "ArrayIterator".to_owned());
            spl_set_entries(&object, entries);
            object.set_property("__flags", Value::Int(flags));
            object.set_property(
                "__iterator_class",
                Value::string(iterator_class.into_bytes()),
            );
        }
        "splfixedarray" => {
            validate_spl_iterator_arg_count(class_name, &args, 0, 1)?;
            let size = args
                .first()
                .map(|arg| to_int(&arg.value))
                .transpose()?
                .unwrap_or(0)
                .max(0) as usize;
            spl_fixed_array_resize(&object, size);
        }
        "splobjectstorage" => {
            validate_spl_iterator_arg_count(class_name, &args, 0, 0)?;
            spl_set_storage_entries(&object, Vec::new());
        }
        "spldoublylinkedlist" | "splstack" | "splqueue" => {
            validate_spl_iterator_arg_count(class_name, &args, 0, 0)?;
            spl_set_entries(&object, Vec::new());
        }
        _ => unreachable!("is_spl_container_runtime_class validates class names"),
    }
    spl_set_position(&object, 0);
    Ok(object)
}

pub(super) fn spl_container_method_is_supported(method: &str) -> bool {
    matches!(
        normalize_method_name(method).as_str(),
        "rewind"
            | "valid"
            | "current"
            | "key"
            | "next"
            | "count"
            | "isempty"
            | "getarraycopy"
            | "toarray"
            | "append"
            | "push"
            | "pop"
            | "shift"
            | "unshift"
            | "top"
            | "bottom"
            | "getsize"
            | "setsize"
            | "exchangearray"
            | "offsetget"
            | "offsetexists"
            | "offsetset"
            | "offsetunset"
            | "attach"
            | "detach"
            | "contains"
            | "getinfo"
            | "setinfo"
            | "getiterator"
            | "getiteratorclass"
            | "setiteratorclass"
            | "getflags"
            | "setflags"
            | "add"
            | "removeall"
            | "serialize"
            | "__serialize"
            | "__debuginfo"
            | "__unserialize"
    )
}

pub(super) fn spl_array_object_iterator_class(object: &ObjectRef) -> String {
    object
        .get_property("__iterator_class")
        .and_then(|value| match effective_value(&value) {
            Value::String(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .unwrap_or_else(|| "ArrayIterator".to_owned())
}

pub(super) fn spl_array_object_flags(object: &ObjectRef) -> i64 {
    object
        .get_property("__flags")
        .map(|value| effective_value(&value))
        .and_then(|value| match value {
            Value::Int(flags) => Some(flags),
            _ => None,
        })
        .unwrap_or(0)
}

pub(super) fn spl_array_object_uses_array_as_props(object: &ObjectRef) -> bool {
    spl_runtime_marker(object).as_deref() == Some("arrayobject")
        && (spl_array_object_flags(object) & SPL_ARRAY_OBJECT_ARRAY_AS_PROPS) != 0
}

pub(super) fn spl_object_user_properties_array(object: &ObjectRef) -> PhpArray {
    let mut properties = PhpArray::new();
    for (name, value) in object.properties_snapshot() {
        if name.starts_with("__") || name == SPL_RUNTIME_CLASS_PROPERTY {
            continue;
        }
        let key = if let Some(rest) = name.strip_prefix("private:") {
            if let Some((class_name, property)) = rest.split_once(':') {
                format!("\0{class_name}\0{property}")
            } else {
                name
            }
        } else if let Some(property) = name.strip_prefix("protected:") {
            format!("\0*\0{property}")
        } else {
            name
        };
        properties.insert(ArrayKey::String(PhpString::from_test_str(&key)), value);
    }
    properties
}

pub(super) fn spl_object_user_debug_properties(object: &ObjectRef) -> Vec<(String, String, Value)> {
    let mut properties = Vec::new();
    for (name, value) in object.properties_snapshot() {
        if name.starts_with("__") || name == SPL_RUNTIME_CLASS_PROPERTY {
            continue;
        }
        let (storage_name, debug_label) = if let Some(rest) = name.strip_prefix("private:") {
            if let Some((class_name, property)) = rest.split_once(':') {
                (
                    name.clone(),
                    format!("\"{property}\":\"{class_name}\":private"),
                )
            } else {
                (name.clone(), name.clone())
            }
        } else if let Some(property) = name.strip_prefix("protected:") {
            (name.clone(), format!("\"{property}\":protected"))
        } else {
            (name.clone(), format!("\"{name}\""))
        };
        properties.push((
            storage_name,
            debug_label,
            spl_debug_view_value(value, Some(object.id())),
        ));
    }
    properties
}

pub(super) fn call_spl_container_method(
    object: ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let class_name = object.class_name();
    let normalized_class =
        spl_runtime_marker(&object).unwrap_or_else(|| normalize_class_name(&class_name));
    let method = normalize_method_name(method);
    match method.as_str() {
        "rewind" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            spl_set_position(&object, 0);
            Ok(Value::Null)
        }
        "valid" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Bool(
                spl_position(&object) < spl_container_entries(&object).len(),
            ))
        }
        "current" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_container_current_entry(&object)
                .map(|(_, value)| value)
                .unwrap_or(Value::Null))
        }
        "key" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_container_current_entry(&object)
                .map(|(key, _)| array_key_to_value(key))
                .unwrap_or_else(|| {
                    if normalized_class == "spldoublylinkedlist" {
                        Value::Int(0)
                    } else {
                        Value::Null
                    }
                }))
        }
        "next" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            spl_set_position(&object, spl_position(&object).saturating_add(1));
            Ok(Value::Null)
        }
        "count" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Int(spl_container_entries(&object).len() as i64))
        }
        "isempty" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            match normalized_class.as_str() {
                "spldoublylinkedlist" | "splstack" | "splqueue" => {
                    Ok(Value::Bool(spl_entries(&object).is_empty()))
                }
                _ => Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                )),
            }
        }
        "getarraycopy" | "toarray" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Array(spl_entries_to_php_array(
                spl_container_entries(&object),
            )))
        }
        "append" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            match normalized_class.as_str() {
                "arrayobject" | "spldoublylinkedlist" | "splstack" | "splqueue" => {
                    let mut entries = spl_entries(&object);
                    let next = entries
                        .iter()
                        .filter_map(|(key, _)| match key {
                            ArrayKey::Int(value) => Some(*value),
                            ArrayKey::String(_) => None,
                        })
                        .max()
                        .map_or(0, |value| value.saturating_add(1));
                    entries.push((ArrayKey::Int(next), args[0].value.clone()));
                    spl_set_entries(&object, entries);
                    Ok(Value::Null)
                }
                _ => Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                )),
            }
        }
        "push" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            spl_container_push(&object, args[0].value.clone());
            Ok(Value::Null)
        }
        "pop" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_container_pop(&object).unwrap_or(Value::Null))
        }
        "shift" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_container_shift(&object).unwrap_or(Value::Null))
        }
        "unshift" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            let mut entries = spl_entries(&object);
            entries.insert(0, (ArrayKey::Int(0), args[0].value.clone()));
            spl_reindex_and_set_entries(&object, entries);
            Ok(Value::Null)
        }
        "top" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_entries(&object)
                .last()
                .map(|(_, value)| value.clone())
                .unwrap_or(Value::Null))
        }
        "bottom" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_entries(&object)
                .first()
                .map(|(_, value)| value.clone())
                .unwrap_or(Value::Null))
        }
        "getsize" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Int(spl_entries(&object).len() as i64))
        }
        "setsize" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if normalized_class != "splfixedarray" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            let size = to_int(&args[0].value)?.max(0) as usize;
            spl_fixed_array_resize(&object, size);
            Ok(Value::Null)
        }
        "exchangearray" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if normalized_class != "arrayobject" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            let old = Value::Array(spl_entries_to_php_array(spl_entries(&object)));
            spl_set_entries(&object, spl_entries_from_value(&args[0].value)?);
            Ok(old)
        }
        "offsetget" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            spl_container_offset_get(&object, &args[0].value)
        }
        "offsetexists" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            spl_container_offset_exists(&object, &args[0].value)
        }
        "offsetset" => {
            validate_spl_iterator_arg_count(&class_name, &args, 2, 2)?;
            spl_container_offset_set(&object, args[0].value.clone(), args[1].value.clone())?;
            Ok(Value::Null)
        }
        "offsetunset" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            spl_container_offset_unset(&object, &args[0].value)?;
            Ok(Value::Null)
        }
        "attach" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 2)?;
            let info = args
                .get(1)
                .map(|arg| arg.value.clone())
                .unwrap_or(Value::Null);
            spl_object_storage_attach(&object, &args[0].value, info)?;
            Ok(Value::Null)
        }
        "detach" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            spl_object_storage_detach(&object, &args[0].value)?;
            Ok(Value::Null)
        }
        "contains" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            Ok(Value::Bool(
                spl_object_storage_find(&object, &args[0].value).is_some(),
            ))
        }
        "getinfo" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let pos = spl_position(&object);
            Ok(spl_storage_entries(&object)
                .get(pos)
                .map(|(_, _, info)| effective_value(info))
                .unwrap_or(Value::Null))
        }
        "setinfo" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            let mut entries = spl_storage_entries(&object);
            let pos = spl_position(&object);
            if let Some((_, _, info)) = entries.get_mut(pos) {
                *info = args[0].value.clone();
            }
            spl_set_storage_entries(&object, entries);
            Ok(Value::Null)
        }
        "getiterator" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if normalized_class != "arrayobject" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            let iterator_class = spl_array_object_iterator_class(&object);
            if !matches!(
                normalize_class_name(&iterator_class).as_str(),
                "arrayiterator" | "recursivearrayiterator"
            ) {
                return Err(
                    "E_PHP_VM_SPL_TYPE_ERROR: An instance of RecursiveIterator or IteratorAggregate creating it is required"
                        .to_owned(),
                );
            }
            let iterator = ObjectRef::new_with_display_name(
                &spl_iterator_class(&iterator_class),
                spl_iterator_display_name(&iterator_class),
            );
            spl_set_entries(&iterator, spl_entries(&object));
            spl_set_position(&iterator, 0);
            Ok(Value::Object(iterator))
        }
        "getiteratorclass" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if normalized_class != "arrayobject" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            Ok(Value::string(
                spl_array_object_iterator_class(&object).into_bytes(),
            ))
        }
        "setiteratorclass" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if normalized_class != "arrayobject" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            let iterator_class = to_string(&args[0].value)?.to_string_lossy();
            object.set_property(
                "__iterator_class",
                Value::string(iterator_class.into_bytes()),
            );
            Ok(Value::Null)
        }
        "getflags" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if normalized_class != "arrayobject" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            Ok(Value::Int(spl_array_object_flags(&object)))
        }
        "setflags" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if normalized_class != "arrayobject" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            object.set_property("__flags", Value::Int(to_int(&args[0].value)?));
            Ok(Value::Null)
        }
        "add" => {
            validate_spl_iterator_arg_count(&class_name, &args, 2, 2)?;
            if !matches!(
                normalized_class.as_str(),
                "spldoublylinkedlist" | "splstack" | "splqueue"
            ) {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            let index = to_int(&args[0].value)?.clamp(0, spl_entries(&object).len() as i64);
            let mut entries = spl_entries(&object);
            entries.insert(
                index as usize,
                (ArrayKey::Int(index), args[1].value.clone()),
            );
            spl_reindex_and_set_entries(&object, entries);
            Ok(Value::Null)
        }
        "removeall" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if normalized_class != "splobjectstorage" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            let Value::Object(other) = effective_value(&args[0].value) else {
                return Ok(Value::Null);
            };
            let remove_ids = spl_storage_entries(&other)
                .into_iter()
                .map(|(id, _, _)| id)
                .collect::<BTreeSet<_>>();
            let entries = spl_storage_entries(&object)
                .into_iter()
                .filter(|(id, _, _)| !remove_ids.contains(id))
                .collect();
            spl_set_storage_entries(&object, entries);
            Ok(Value::Null)
        }
        "serialize" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::string(Vec::new()))
        }
        "__serialize" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            match normalized_class.as_str() {
                "arrayobject" => Ok(Value::packed_array(vec![
                    object.get_property("__flags").unwrap_or(Value::Int(0)),
                    Value::Array(spl_entries_to_php_array(spl_entries(&object))),
                    Value::Array(spl_object_user_properties_array(&object)),
                    if spl_array_object_iterator_class(&object) == "ArrayIterator" {
                        Value::Null
                    } else {
                        Value::string(spl_array_object_iterator_class(&object).into_bytes())
                    },
                ])),
                "spldoublylinkedlist" | "splstack" | "splqueue" => Ok(Value::packed_array(vec![
                    object.get_property("__flags").unwrap_or(Value::Int(0)),
                    Value::Array(spl_entries_to_php_array(spl_entries(&object))),
                    Value::Array(spl_object_user_properties_array(&object)),
                ])),
                "splobjectstorage" => Ok(Value::packed_array(vec![
                    Value::Array(spl_entries_to_php_array(
                        spl_storage_entries(&object)
                            .into_iter()
                            .map(|(_, object, info)| {
                                (
                                    ArrayKey::Int(0),
                                    Value::packed_array(vec![Value::Object(object), info]),
                                )
                            })
                            .collect(),
                    )),
                    Value::Array(spl_object_user_properties_array(&object)),
                ])),
                _ => Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                )),
            }
        }
        "__debuginfo" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if normalized_class == "splobjectstorage" {
                return Ok(Value::Array(spl_object_storage_debug_info_array(&object)));
            }
            Ok(Value::Array(spl_entries_to_php_array(
                spl_container_entries(&object),
            )))
        }
        "__unserialize" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if normalized_class == "splfixedarray" {
                spl_set_entries(&object, spl_entries_from_value(&args[0].value)?);
            }
            Ok(Value::Null)
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn spl_container_class(class_name: &str) -> RuntimeClassEntry {
    let normalized = normalize_class_name(class_name);
    let mut interfaces = vec![
        "Iterator".to_owned(),
        "Traversable".to_owned(),
        "Countable".to_owned(),
    ];
    if matches!(
        normalized.as_str(),
        "arrayobject" | "splfixedarray" | "splobjectstorage"
    ) {
        interfaces.push("ArrayAccess".to_owned());
    }
    RuntimeClassEntry {
        name: normalize_class_name(class_name),
        parent: match normalized.as_str() {
            "splstack" | "splqueue" => Some(normalize_class_name("SplDoublyLinkedList")),
            _ => None,
        },
        interfaces,
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn spl_container_display_name(class_name: &str) -> &'static str {
    match normalize_class_name(class_name).as_str() {
        "arrayobject" => "ArrayObject",
        "splfixedarray" => "SplFixedArray",
        "splobjectstorage" => "SplObjectStorage",
        "spldoublylinkedlist" => "SplDoublyLinkedList",
        "splstack" => "SplStack",
        "splqueue" => "SplQueue",
        _ => "ArrayObject",
    }
}

pub(super) fn spl_container_entries(object: &ObjectRef) -> Vec<(ArrayKey, Value)> {
    if spl_runtime_marker(object).as_deref() == Some("splobjectstorage") {
        return spl_storage_entries(object)
            .into_iter()
            .enumerate()
            .map(|(index, (_, object, _))| (ArrayKey::Int(index as i64), Value::Object(object)))
            .collect();
    }
    spl_entries(object)
}

pub(super) fn spl_container_current_entry(object: &ObjectRef) -> Option<(ArrayKey, Value)> {
    spl_container_entries(object)
        .into_iter()
        .nth(spl_position(object))
}

pub(super) fn spl_fixed_array_resize(object: &ObjectRef, size: usize) {
    let mut entries = spl_entries(object);
    entries.resize_with(size, || (ArrayKey::Int(0), Value::Null));
    let entries = entries
        .into_iter()
        .enumerate()
        .map(|(index, (_, value))| (ArrayKey::Int(index as i64), value))
        .collect();
    spl_set_entries(object, entries);
}

pub(super) fn spl_container_push(object: &ObjectRef, value: Value) {
    let mut entries = spl_entries(object);
    entries.push((ArrayKey::Int(entries.len() as i64), value));
    spl_set_entries(object, entries);
}

pub(super) fn spl_container_pop(object: &ObjectRef) -> Option<Value> {
    let mut entries = spl_entries(object);
    let value = entries.pop().map(|(_, value)| value);
    spl_reindex_and_set_entries(object, entries);
    value
}

pub(super) fn spl_container_shift(object: &ObjectRef) -> Option<Value> {
    let mut entries = spl_entries(object);
    if entries.is_empty() {
        return None;
    }
    let value = entries.remove(0).1;
    spl_reindex_and_set_entries(object, entries);
    Some(value)
}

pub(super) fn spl_reindex_and_set_entries(object: &ObjectRef, entries: Vec<(ArrayKey, Value)>) {
    spl_set_entries(
        object,
        entries
            .into_iter()
            .enumerate()
            .map(|(index, (_, value))| (ArrayKey::Int(index as i64), value))
            .collect(),
    );
}

pub(super) fn spl_doubly_linked_list_default_flags(class_name: &str) -> i64 {
    match normalize_class_name(class_name).as_str() {
        "splstack" => SPL_DLLIST_IT_MODE_LIFO | 4 | SPL_DLLIST_IT_MODE_KEEP,
        "splqueue" => SPL_DLLIST_IT_MODE_FIFO | 4 | SPL_DLLIST_IT_MODE_KEEP,
        _ => SPL_DLLIST_IT_MODE_FIFO | SPL_DLLIST_IT_MODE_KEEP,
    }
}

pub(super) fn spl_doubly_linked_list_flags(object: &ObjectRef, class_name: &str) -> i64 {
    object
        .get_property("__flags")
        .map(|value| effective_value(&value))
        .and_then(|value| match value {
            Value::Int(flags) => Some(flags),
            _ => None,
        })
        .unwrap_or_else(|| spl_doubly_linked_list_default_flags(class_name))
}

pub(super) fn spl_heap_debug_flags(object: &ObjectRef, class_name: &str) -> i64 {
    if normalize_class_name(class_name) == "splpriorityqueue" {
        return spl_priority_queue_extract_flags(object);
    }
    0
}

pub(super) fn spl_heap_serialize_array(object: &ObjectRef, class_name: &str) -> PhpArray {
    let mut internal = PhpArray::new();
    internal.insert(
        ArrayKey::String(PhpString::from_test_str("flags")),
        Value::Int(spl_heap_debug_flags(object, class_name)),
    );
    internal.insert(
        ArrayKey::String(PhpString::from_test_str("heap_elements")),
        Value::Array(spl_entries_to_php_array(spl_entries(object))),
    );
    PhpArray::from_packed(vec![
        Value::Array(spl_object_user_properties_array(object)),
        Value::Array(internal),
    ])
}

pub(super) fn spl_heap_is_corrupted(object: &ObjectRef) -> bool {
    spl_bool_property(object, "__is_corrupted")
}

pub(super) fn spl_heap_set_corrupted(object: &ObjectRef, corrupted: bool) {
    object.set_property("__is_corrupted", Value::Bool(corrupted));
}

pub(super) fn spl_heap_is_modifying(object: &ObjectRef) -> bool {
    spl_bool_property(object, "__is_modifying")
}

pub(super) fn spl_heap_set_modifying(object: &ObjectRef, modifying: bool) {
    object.set_property("__is_modifying", Value::Bool(modifying));
}

pub(super) fn spl_heap_corruption_error() -> String {
    "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Heap is corrupted, heap properties are no longer ensured."
        .to_owned()
}

pub(super) fn spl_heap_modifying_error() -> String {
    "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Heap cannot be changed when it is already being modified."
        .to_owned()
}

pub(super) fn spl_container_offset_get(object: &ObjectRef, key: &Value) -> Result<Value, String> {
    if spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class)) {
        spl_caching_iterator_require_full_cache(object, &object.display_name())?;
        return spl_caching_iterator_offset_get(object, key);
    }
    match spl_runtime_marker(object)
        .unwrap_or_else(|| normalize_class_name(&object.class_name()))
        .as_str()
    {
        "splobjectstorage" => Ok(spl_object_storage_find(object, key)
            .map(|(_, _, info)| effective_value(&info))
            .unwrap_or(Value::Null)),
        _ => {
            let key = array_key_from_value(key)?;
            Ok(spl_entries(object)
                .into_iter()
                .find_map(|(entry_key, value)| (entry_key == key).then_some(value))
                .unwrap_or(Value::Null))
        }
    }
}

pub(super) fn spl_container_offset_exists(
    object: &ObjectRef,
    key: &Value,
) -> Result<Value, String> {
    if spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class)) {
        spl_caching_iterator_require_full_cache(object, &object.display_name())?;
        return spl_caching_iterator_offset_exists(object, key);
    }
    let exists = match spl_runtime_marker(object)
        .unwrap_or_else(|| normalize_class_name(&object.class_name()))
        .as_str()
    {
        "splobjectstorage" => spl_object_storage_find(object, key).is_some(),
        _ => {
            let key = array_key_from_value(key)?;
            spl_entries(object)
                .into_iter()
                .any(|(entry_key, value)| entry_key == key && !matches!(value, Value::Null))
        }
    };
    Ok(Value::Bool(exists))
}

pub(super) fn spl_container_offset_set(
    object: &ObjectRef,
    key: Value,
    value: Value,
) -> Result<(), String> {
    if spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class)) {
        spl_caching_iterator_require_full_cache(object, &object.display_name())?;
        return spl_caching_iterator_offset_set(object, &key, value);
    }
    match spl_runtime_marker(object)
        .unwrap_or_else(|| normalize_class_name(&object.class_name()))
        .as_str()
    {
        "splobjectstorage" => spl_object_storage_attach(object, &key, value),
        "splfixedarray" => {
            let key = array_key_from_value(&key)?;
            let ArrayKey::Int(index) = key else {
                return Err(
                    "E_PHP_VM_SPL_FIXED_ARRAY_KEY: SplFixedArray keys must be integers".to_owned(),
                );
            };
            let mut entries = spl_entries(object);
            if index < 0 || index as usize >= entries.len() {
                return Err(
                    "E_PHP_VM_SPL_FIXED_ARRAY_BOUNDS: SplFixedArray index out of range".to_owned(),
                );
            }
            entries[index as usize] = (ArrayKey::Int(index), value);
            spl_set_entries(object, entries);
            Ok(())
        }
        _ => {
            let mut entries = spl_entries(object);
            if matches!(key, Value::Null) {
                let next = entries
                    .iter()
                    .filter_map(|(key, _)| match key {
                        ArrayKey::Int(value) => Some(*value),
                        ArrayKey::String(_) => None,
                    })
                    .max()
                    .map_or(0, |value| value.saturating_add(1));
                entries.push((ArrayKey::Int(next), value));
                spl_set_entries(object, entries);
                return Ok(());
            }
            let key = array_key_from_value(&key)?;
            if let Some((_, entry_value)) =
                entries.iter_mut().find(|(entry_key, _)| entry_key == &key)
            {
                *entry_value = value;
            } else {
                entries.push((key, value));
            }
            spl_set_entries(object, entries);
            Ok(())
        }
    }
}

pub(super) fn spl_container_offset_unset(object: &ObjectRef, key: &Value) -> Result<(), String> {
    if spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class)) {
        spl_caching_iterator_require_full_cache(object, &object.display_name())?;
        return spl_caching_iterator_offset_unset(object, key);
    }
    match spl_runtime_marker(object)
        .unwrap_or_else(|| normalize_class_name(&object.class_name()))
        .as_str()
    {
        "splobjectstorage" => spl_object_storage_detach(object, key),
        "splfixedarray" => {
            let key = array_key_from_value(key)?;
            let ArrayKey::Int(index) = key else {
                return Err(
                    "E_PHP_VM_SPL_FIXED_ARRAY_KEY: SplFixedArray keys must be integers".to_owned(),
                );
            };
            let mut entries = spl_entries(object);
            if index >= 0
                && let Some((_, value)) = entries.get_mut(index as usize)
            {
                *value = Value::Null;
                spl_set_entries(object, entries);
            }
            Ok(())
        }
        "spldoublylinkedlist" | "splstack" | "splqueue" => {
            let key = array_key_from_value(key)?;
            let ArrayKey::Int(index) = key else {
                return Ok(());
            };
            let mut entries = spl_entries(object);
            if index >= 0 && (index as usize) < entries.len() {
                entries.remove(index as usize);
                spl_reindex_and_set_entries(object, entries);
            }
            Ok(())
        }
        _ => {
            let key = array_key_from_value(key)?;
            let entries = spl_entries(object)
                .into_iter()
                .filter(|(entry_key, _)| entry_key != &key)
                .collect();
            spl_set_entries(object, entries);
            Ok(())
        }
    }
}

pub(super) fn spl_storage_entries(object: &ObjectRef) -> Vec<(u64, ObjectRef, Value)> {
    let Some(Value::Array(entries)) = object.get_property("__storage") else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(_, entry)| {
            let Value::Array(pair) = effective_value(entry) else {
                return None;
            };
            let id = match pair.get(&ArrayKey::Int(0)).map(effective_value)? {
                Value::Int(value) if value >= 0 => value as u64,
                _ => return None,
            };
            let object = match pair.get(&ArrayKey::Int(1)).map(effective_value)? {
                Value::Object(object) => object,
                _ => return None,
            };
            let info = pair.get(&ArrayKey::Int(2)).map(effective_value)?;
            Some((id, object, info))
        })
        .collect()
}

pub(super) fn spl_set_storage_entries(object: &ObjectRef, entries: Vec<(u64, ObjectRef, Value)>) {
    let packed = entries
        .into_iter()
        .map(|(id, object, info)| {
            Value::packed_array(vec![Value::Int(id as i64), Value::Object(object), info])
        })
        .collect();
    object.set_property("__storage", Value::packed_array(packed));
}

pub(super) fn spl_object_storage_debug_info_array(object: &ObjectRef) -> PhpArray {
    let mut result = PhpArray::new();
    result.insert(
        ArrayKey::String(PhpString::from_test_str("\0SplObjectStorage\0storage")),
        Value::Array(spl_object_storage_debug_records_array(object)),
    );
    result
}

pub(super) fn spl_object_storage_debug_records_array(object: &ObjectRef) -> PhpArray {
    let excluded_object_id = Some(object.id());
    let records = spl_storage_entries(object)
        .into_iter()
        .map(|(_, object, info)| {
            let mut record = PhpArray::new();
            record.insert(
                ArrayKey::String(PhpString::from_test_str("obj")),
                spl_debug_view_value(Value::Object(object), excluded_object_id),
            );
            record.insert(
                ArrayKey::String(PhpString::from_test_str("inf")),
                spl_debug_view_value(effective_value(&info), excluded_object_id),
            );
            Value::Array(record)
        })
        .collect();
    PhpArray::from_packed(records)
}

pub(super) fn spl_object_storage_find(
    object: &ObjectRef,
    key: &Value,
) -> Option<(u64, ObjectRef, Value)> {
    let Value::Object(needle) = effective_value(key) else {
        return None;
    };
    let id = needle.id();
    spl_storage_entries(object)
        .into_iter()
        .find(|(entry_id, _, _)| *entry_id == id)
}

pub(super) fn spl_object_storage_attach(
    object: &ObjectRef,
    key: &Value,
    info: Value,
) -> Result<(), String> {
    let Value::Object(attached) = effective_value(key) else {
        return Err(
            "E_PHP_VM_SPL_OBJECT_STORAGE_KEY: SplObjectStorage keys must be objects".to_owned(),
        );
    };
    let id = attached.id();
    let mut entries = spl_storage_entries(object);
    if let Some((_, _, entry_info)) = entries.iter_mut().find(|(entry_id, _, _)| *entry_id == id) {
        *entry_info = info;
    } else {
        entries.push((id, attached, info));
    }
    spl_set_storage_entries(object, entries);
    Ok(())
}

pub(super) fn spl_object_storage_detach(object: &ObjectRef, key: &Value) -> Result<(), String> {
    let Value::Object(needle) = effective_value(key) else {
        return Err(
            "E_PHP_VM_SPL_OBJECT_STORAGE_KEY: SplObjectStorage keys must be objects".to_owned(),
        );
    };
    let id = needle.id();
    let entries = spl_storage_entries(object)
        .into_iter()
        .filter(|(entry_id, _, _)| *entry_id != id)
        .collect();
    spl_set_storage_entries(object, entries);
    Ok(())
}

pub(super) fn is_spl_heap_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "splheap" | "splmaxheap" | "splminheap" | "splpriorityqueue"
    )
}

pub(super) fn internal_spl_heap_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_spl_heap_runtime_class(object_class) {
        return None;
    }
    let object_class = normalize_class_name(object_class);
    let target_class = normalize_class_name(target_class);
    Some(match target_class.as_str() {
        "traversable" | "iterator" | "countable" => true,
        "splheap" => matches!(
            object_class.as_str(),
            "splheap" | "splmaxheap" | "splminheap"
        ),
        "splmaxheap" => object_class == "splmaxheap",
        "splminheap" => object_class == "splminheap",
        "splpriorityqueue" => object_class == "splpriorityqueue",
        _ => false,
    })
}

pub(super) fn new_spl_heap_object(
    class_name: &str,
    args: Vec<CallArgument>,
) -> Result<ObjectRef, String> {
    if let Some(name) = args.iter().find_map(|arg| arg.name.as_deref()) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_NAMED_ARG: {class_name}::__construct has no builtin parameter ${name}"
        ));
    }
    validate_spl_iterator_arg_count(class_name, &args, 0, 0)?;
    let object = ObjectRef::new_with_display_name(
        &spl_heap_class(class_name),
        spl_heap_display_name(class_name),
    );
    spl_set_entries(&object, Vec::new());
    spl_set_position(&object, 0);
    object.set_property("__extract_flags", Value::Int(SPL_PRIORITY_QUEUE_EXTR_DATA));
    spl_heap_set_corrupted(&object, false);
    spl_heap_set_modifying(&object, false);
    Ok(object)
}

pub(super) fn spl_heap_method_is_supported(method: &str) -> bool {
    matches!(
        normalize_method_name(method).as_str(),
        "rewind"
            | "valid"
            | "current"
            | "key"
            | "next"
            | "count"
            | "isempty"
            | "insert"
            | "top"
            | "extract"
            | "setextractflags"
            | "getextractflags"
            | "compare"
            | "iscorrupted"
            | "recoverfromcorruption"
            | "__serialize"
            | "__unserialize"
    )
}

pub(super) enum SplHeapMethodError {
    Message(String),
    Runtime(Box<VmResult>),
}

pub(super) fn call_spl_heap_method(
    object: ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
) -> Result<Value, String> {
    let class_name = object.class_name();
    let normalized_class =
        spl_runtime_marker(&object).unwrap_or_else(|| normalize_class_name(&class_name));
    let method = normalize_method_name(method);
    if spl_heap_is_modifying(&object) && !matches!(method.as_str(), "compare" | "iscorrupted") {
        return Err(spl_heap_modifying_error());
    }
    if spl_heap_is_corrupted(&object)
        && matches!(
            method.as_str(),
            "rewind" | "current" | "key" | "next" | "insert" | "top" | "extract"
        )
    {
        return Err(spl_heap_corruption_error());
    }
    match method.as_str() {
        "rewind" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            spl_heap_sort_entries(&object, &normalized_class);
            spl_set_position(&object, 0);
            Ok(Value::Null)
        }
        "valid" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Bool(
                spl_position(&object) < spl_entries(&object).len(),
            ))
        }
        "current" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_heap_current_entry(&object, &normalized_class)
                .map(|(_, value)| value)
                .unwrap_or(Value::Null))
        }
        "key" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_heap_current_entry(&object, &normalized_class)
                .map(|(key, _)| array_key_to_value(key))
                .unwrap_or(Value::Null))
        }
        "next" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            spl_set_position(&object, spl_position(&object).saturating_add(1));
            Ok(Value::Null)
        }
        "count" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Int(spl_entries(&object).len() as i64))
        }
        "isempty" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Bool(spl_entries(&object).is_empty()))
        }
        "insert" => {
            let expected = if normalized_class == "splpriorityqueue" {
                2
            } else {
                1
            };
            validate_spl_iterator_arg_count(&class_name, &args, expected, expected)?;
            let value = if normalized_class == "splpriorityqueue" {
                spl_priority_queue_entry(args[0].value.clone(), args[1].value.clone())
            } else {
                args[0].value.clone()
            };
            spl_heap_insert_entry(&object, &normalized_class, value);
            Ok(Value::Null)
        }
        "top" | "extract" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let mut entries = spl_entries(&object);
            if entries.is_empty() {
                let action = if method == "top" {
                    "peek at"
                } else {
                    "extract from"
                };
                return Err(format!(
                    "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Can't {action} an empty heap"
                ));
            }
            let raw = if method == "extract" {
                let (_, raw) = entries.swap_remove(0);
                if !entries.is_empty() {
                    spl_heap_sift_down(&mut entries, &normalized_class, 0);
                }
                spl_reindex_and_set_entries(&object, entries);
                raw
            } else {
                entries[0].1.clone()
            };
            Ok(spl_heap_extract_value(&object, &normalized_class, raw))
        }
        "setextractflags" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            if normalized_class != "splpriorityqueue" {
                return Err(format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
                ));
            }
            let flags = to_int(&args[0].value)?;
            if !matches!(
                flags,
                SPL_PRIORITY_QUEUE_EXTR_DATA
                    | SPL_PRIORITY_QUEUE_EXTR_PRIORITY
                    | SPL_PRIORITY_QUEUE_EXTR_BOTH
            ) {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: extract flags must be EXTR_DATA, EXTR_PRIORITY, or EXTR_BOTH"
                        .to_owned(),
                );
            }
            object.set_property("__extract_flags", Value::Int(flags));
            Ok(Value::Null)
        }
        "getextractflags" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Int(spl_priority_queue_extract_flags(&object)))
        }
        "compare" => {
            validate_spl_iterator_arg_count(&class_name, &args, 2, 2)?;
            let ordering = compare(
                &effective_value(&args[0].value),
                &effective_value(&args[1].value),
            )
            .map_err(|message| format!("E_PHP_VM_SPL_COMPARE: {message}"))?;
            Ok(Value::Int(match ordering {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            }))
        }
        "iscorrupted" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Bool(spl_heap_is_corrupted(&object)))
        }
        "recoverfromcorruption" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            spl_heap_set_corrupted(&object, false);
            Ok(Value::Null)
        }
        "__serialize" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Array(spl_heap_serialize_array(
                &object,
                &normalized_class,
            )))
        }
        "__unserialize" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            spl_set_entries(&object, spl_entries_from_value(&args[0].value)?);
            spl_heap_sort_entries(&object, &normalized_class);
            Ok(Value::Null)
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn spl_heap_class(class_name: &str) -> RuntimeClassEntry {
    let normalized = normalize_class_name(class_name);
    RuntimeClassEntry {
        name: normalized.clone(),
        parent: match normalized.as_str() {
            "splmaxheap" | "splminheap" => Some(normalize_class_name("SplHeap")),
            _ => None,
        },
        interfaces: vec![
            "Iterator".to_owned(),
            "Traversable".to_owned(),
            "Countable".to_owned(),
        ],
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn spl_heap_display_name(class_name: &str) -> &'static str {
    match normalize_class_name(class_name).as_str() {
        "splheap" => "SplHeap",
        "splmaxheap" => "SplMaxHeap",
        "splminheap" => "SplMinHeap",
        "splpriorityqueue" => "SplPriorityQueue",
        _ => "SplHeap",
    }
}

pub(super) const SPL_MULTIPLE_ITERATOR_NEED_ANY: i64 = 0;
pub(super) const SPL_MULTIPLE_ITERATOR_NEED_ALL: i64 = 1;
pub(super) const SPL_MULTIPLE_ITERATOR_KEYS_NUMERIC: i64 = 0;
pub(super) const SPL_MULTIPLE_ITERATOR_KEYS_ASSOC: i64 = 2;

pub(super) fn spl_multiple_iterator_flags(object: &ObjectRef) -> i64 {
    object
        .get_property("__regex_flags")
        .map(|value| effective_value(&value))
        .and_then(|value| match value {
            Value::Int(flags) => Some(flags),
            _ => None,
        })
        .unwrap_or(SPL_MULTIPLE_ITERATOR_NEED_ALL | SPL_MULTIPLE_ITERATOR_KEYS_NUMERIC)
}

pub(super) fn spl_multiple_iterator_needs_all(object: &ObjectRef) -> bool {
    spl_multiple_iterator_flags(object) & SPL_MULTIPLE_ITERATOR_NEED_ALL != 0
}

pub(super) fn spl_multiple_iterator_uses_assoc_keys(object: &ObjectRef) -> bool {
    spl_multiple_iterator_flags(object) & SPL_MULTIPLE_ITERATOR_KEYS_ASSOC != 0
}

pub(super) fn spl_multiple_iterator_records(object: &ObjectRef) -> Vec<(ObjectRef, Value)> {
    let Some(Value::Array(records)) = object.get_property("__attached_iterators") else {
        return Vec::new();
    };
    records
        .iter()
        .filter_map(|(_, record)| {
            let Value::Array(pair) = effective_value(record) else {
                return None;
            };
            let iterator = match pair.get(&ArrayKey::Int(0)).map(effective_value) {
                Some(Value::Object(iterator)) => iterator,
                _ => return None,
            };
            let info = pair
                .get(&ArrayKey::Int(1))
                .map(effective_value)
                .unwrap_or(Value::Null);
            Some((iterator, info))
        })
        .collect()
}

pub(super) fn spl_multiple_iterator_set_records(
    object: &ObjectRef,
    records: Vec<(ObjectRef, Value)>,
) {
    let mut packed_records = PhpArray::new();
    let mut ids = PhpArray::new();
    for (iterator, info) in records {
        let record = Value::packed_array(vec![Value::Object(iterator.clone()), info]);
        packed_records.append(record);
        ids.append(Value::Int(iterator.id() as i64));
    }
    let count = packed_records.len() as i64;
    object.set_property("__attached_iterators", Value::Array(packed_records));
    object.set_property("__attached_iterator_ids", Value::Array(ids));
    object.set_property("__iterator_count", Value::Int(count));
}

pub(super) fn spl_multiple_iterator_info_key(info: &Value) -> Result<Option<ArrayKey>, String> {
    match effective_value(info) {
        Value::Null => Ok(None),
        Value::Int(value) => Ok(Some(ArrayKey::Int(value))),
        Value::String(value) => Ok(Some(ArrayKey::String(value))),
        other => Err(format!(
            "E_PHP_VM_SPL_TYPE_ERROR: MultipleIterator::attachIterator(): Argument #2 ($info) must be of type string|int|null, {} given",
            type_error_value_name(&other)
        )),
    }
}

pub(super) fn spl_multiple_iterator_type_error(prefix: &str, value: &Value) -> String {
    if prefix.starts_with("Can only attach objects") {
        prefix.to_owned()
    } else {
        format!("{prefix}, {} given", type_error_value_name(value))
    }
}

pub(super) fn spl_multiple_iterator_attach_validated(
    object: &ObjectRef,
    iterator: ObjectRef,
    info: Value,
) -> Result<(), String> {
    let info_key = spl_multiple_iterator_info_key(&info)?;
    let mut records = spl_multiple_iterator_records(object);
    let existing_index = records
        .iter()
        .position(|(attached, _)| attached.id() == iterator.id());
    if let Some(new_key) = &info_key {
        let duplicate = records
            .iter()
            .enumerate()
            .any(|(index, (attached, attached_info))| {
                existing_index != Some(index)
                    && spl_multiple_iterator_info_key(attached_info)
                        .ok()
                        .flatten()
                        .as_ref()
                        == Some(new_key)
                    && attached.id() != iterator.id()
            });
        if duplicate {
            return Err("E_PHP_VM_SPL_INVALID_ARGUMENT: Key duplication error".to_owned());
        }
    }
    if let Some(index) = existing_index {
        records[index].1 = info;
    } else {
        records.push((iterator, info));
    }
    spl_multiple_iterator_set_records(object, records);
    Ok(())
}

pub(super) fn spl_multiple_iterator_attach(
    object: &ObjectRef,
    args: &[CallArgument],
) -> Result<(), String> {
    let Value::Object(iterator) = effective_value(&args[0].value) else {
        return Err(format!(
            "E_PHP_VM_SPL_TYPE_ERROR: MultipleIterator::attachIterator(): Argument #1 ($iterator) must be of type Iterator, {} given",
            type_error_value_name(&args[0].value)
        ));
    };
    let info = args
        .get(1)
        .map(|arg| effective_value(&arg.value))
        .unwrap_or(Value::Null);
    spl_multiple_iterator_attach_validated(object, iterator, info)
}

pub(super) fn spl_multiple_iterator_is_valid(object: &ObjectRef) -> bool {
    let records = spl_multiple_iterator_records(object);
    if records.is_empty() {
        return false;
    }
    if spl_multiple_iterator_needs_all(object) {
        records
            .iter()
            .all(|(iterator, _)| spl_position(iterator) < spl_entries(iterator).len())
    } else {
        records
            .iter()
            .any(|(iterator, _)| spl_position(iterator) < spl_entries(iterator).len())
    }
}

pub(super) fn spl_multiple_iterator_current(object: &ObjectRef) -> Result<Value, String> {
    let records = spl_multiple_iterator_records(object);
    if records.is_empty() {
        return Err(
            "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Called current() on an invalid iterator".to_owned(),
        );
    }
    let use_assoc = spl_multiple_iterator_uses_assoc_keys(object);
    let need_all = spl_multiple_iterator_needs_all(object);
    let mut values = PhpArray::new();
    let mut any_valid = false;
    for (index, (iterator, info)) in records.iter().enumerate() {
        let valid = spl_position(iterator) < spl_entries(iterator).len();
        any_valid |= valid;
        if need_all && !valid {
            return Err(
                "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Called current() with non valid sub iterator"
                    .to_owned(),
            );
        }
        let key = if use_assoc {
            spl_multiple_iterator_info_key(info)?.ok_or_else(|| {
                "E_PHP_VM_SPL_INVALID_ARGUMENT: Sub-Iterator is associated with NULL".to_owned()
            })?
        } else {
            ArrayKey::Int(index as i64)
        };
        let value = if valid {
            spl_current_entry(iterator)
                .map(|(_, value)| value)
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        values.insert(key, value);
    }
    if !any_valid {
        return Err(
            "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Called current() with non valid sub iterator"
                .to_owned(),
        );
    }
    Ok(Value::Array(values))
}

pub(super) fn spl_multiple_iterator_key(object: &ObjectRef) -> Result<Value, String> {
    let records = spl_multiple_iterator_records(object);
    if records.is_empty() {
        return Err(
            "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Called key() on an invalid iterator".to_owned(),
        );
    }
    let use_assoc = spl_multiple_iterator_uses_assoc_keys(object);
    let need_all = spl_multiple_iterator_needs_all(object);
    let mut keys = PhpArray::new();
    let mut any_valid = false;
    for (index, (iterator, info)) in records.iter().enumerate() {
        let valid = spl_position(iterator) < spl_entries(iterator).len();
        any_valid |= valid;
        if need_all && !valid {
            return Err(
                "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Called key() with non valid sub iterator"
                    .to_owned(),
            );
        }
        let outer_key = if use_assoc {
            spl_multiple_iterator_info_key(info)?.ok_or_else(|| {
                "E_PHP_VM_SPL_INVALID_ARGUMENT: Sub-Iterator is associated with NULL".to_owned()
            })?
        } else {
            ArrayKey::Int(index as i64)
        };
        let value = if valid {
            spl_current_entry(iterator)
                .map(|(key, _)| array_key_to_value(key))
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        keys.insert(outer_key, value);
    }
    if !any_valid {
        return Err(
            "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Called key() with non valid sub iterator".to_owned(),
        );
    }
    Ok(Value::Array(keys))
}

pub(super) fn spl_class_constant_value(class_name: &str, constant: &str) -> Option<Value> {
    let class_name = normalize_class_name(class_name);
    if matches!(class_name.as_str(), "arrayiterator" | "arrayobject") {
        return Some(Value::Int(match normalize_class_name(constant).as_str() {
            "std_prop_list" => SPL_ARRAY_OBJECT_STD_PROP_LIST,
            "array_as_props" => SPL_ARRAY_OBJECT_ARRAY_AS_PROPS,
            _ => return None,
        }));
    }
    if matches!(
        class_name.as_str(),
        "regexiterator" | "recursiveregexiterator"
    ) {
        return Some(Value::Int(match normalize_class_name(constant).as_str() {
            "match" => 0,
            "get_match" => 1,
            "all_matches" => 2,
            "split" => 3,
            "replace" => 4,
            "use_key" => 1,
            "invert_match" => 2,
            _ => return None,
        }));
    }
    if matches!(
        class_name.as_str(),
        "cachingiterator" | "recursivecachingiterator"
    ) {
        return Some(Value::Int(match normalize_class_name(constant).as_str() {
            "call_tostring" => 1,
            "tostring_use_key" => 2,
            "tostring_use_current" => 4,
            "tostring_use_inner" => 8,
            "catch_get_child" => 16,
            "full_cache" => 256,
            _ => return None,
        }));
    }
    if matches!(
        class_name.as_str(),
        "recursiveiteratoriterator" | "recursivetreeiterator"
    ) && let Some(value) = match normalize_class_name(constant).as_str() {
        "leaves_only" => Some(0),
        "self_first" => Some(1),
        "child_first" => Some(2),
        "catch_get_child" => Some(16),
        _ => None,
    } {
        return Some(Value::Int(value));
    }
    if class_name == "recursivetreeiterator" {
        return Some(Value::Int(match normalize_class_name(constant).as_str() {
            "bypass_current" => 4,
            "bypass_key" => 8,
            "prefix_left" => 0,
            "prefix_mid_has_next" => 1,
            "prefix_mid_last" => 2,
            "prefix_end_has_next" => 3,
            "prefix_end_last" => 4,
            "prefix_right" => 5,
            _ => return None,
        }));
    }
    if class_name == "multipleiterator" {
        return Some(Value::Int(match normalize_class_name(constant).as_str() {
            "mit_need_any" => 0,
            "mit_need_all" => 1,
            "mit_keys_numeric" => 0,
            "mit_keys_assoc" => 2,
            _ => return None,
        }));
    }
    if matches!(
        class_name.as_str(),
        "filesystemiterator" | "recursivedirectoryiterator" | "globiterator"
    ) {
        return Some(Value::Int(match normalize_class_name(constant).as_str() {
            "current_mode_mask" => SPL_FILESYSTEM_CURRENT_MODE_MASK,
            "current_as_pathname" => SPL_FILESYSTEM_CURRENT_AS_PATHNAME,
            "current_as_fileinfo" => SPL_FILESYSTEM_CURRENT_AS_FILEINFO,
            "current_as_self" => SPL_FILESYSTEM_CURRENT_AS_SELF,
            "key_mode_mask" => SPL_FILESYSTEM_KEY_MODE_MASK,
            "key_as_pathname" => SPL_FILESYSTEM_KEY_AS_PATHNAME,
            "follow_symlinks" => SPL_FILESYSTEM_FOLLOW_SYMLINKS,
            "key_as_filename" => SPL_FILESYSTEM_KEY_AS_FILENAME,
            "new_current_and_key" => SPL_FILESYSTEM_KEY_AS_FILENAME,
            "other_mode_mask" => SPL_FILESYSTEM_OTHER_MODE_MASK,
            "skip_dots" => SPL_FILESYSTEM_SKIP_DOTS,
            "unix_paths" => SPL_FILESYSTEM_UNIX_PATHS,
            _ => return None,
        }));
    }
    if class_name == "splpriorityqueue" {
        return Some(Value::Int(match normalize_class_name(constant).as_str() {
            "extr_data" => SPL_PRIORITY_QUEUE_EXTR_DATA,
            "extr_priority" => SPL_PRIORITY_QUEUE_EXTR_PRIORITY,
            "extr_both" => SPL_PRIORITY_QUEUE_EXTR_BOTH,
            _ => return None,
        }));
    }
    None
}

pub(super) fn spl_class_constant_value_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    constant: &str,
) -> Option<Value> {
    let mut current = normalize_class_name(class_name);
    let mut seen = Vec::new();
    loop {
        if let Some(value) = spl_class_constant_value(&current, constant) {
            return Some(value);
        }
        if seen.iter().any(|name| name == &current) {
            return None;
        }
        seen.push(current.clone());
        let class = lookup_class_in_state(compiled, state, &current)?;
        current = class.parent.as_deref().map(normalize_class_name)?;
    }
}

pub(super) fn spl_heap_sort_entries(object: &ObjectRef, class_name: &str) {
    let normalized_class = normalize_class_name(class_name);
    let mut entries = spl_entries(object);
    entries
        .sort_by(|(_, left), (_, right)| spl_heap_compare_entries(&normalized_class, left, right));
    spl_reindex_and_set_entries(object, entries);
}

pub(super) fn spl_heap_insert_entry(object: &ObjectRef, class_name: &str, value: Value) {
    let normalized_class = normalize_class_name(class_name);
    let mut entries = spl_entries(object);
    entries.push((ArrayKey::Int(entries.len() as i64), value));
    let mut child = entries.len().saturating_sub(1);
    while child > 0 {
        let parent = (child - 1) / 2;
        if spl_heap_compare_entries(&normalized_class, &entries[parent].1, &entries[child].1)
            != std::cmp::Ordering::Greater
        {
            break;
        }
        entries.swap(parent, child);
        child = parent;
    }
    spl_reindex_and_set_entries(object, entries);
}

pub(super) fn spl_heap_sift_down(
    entries: &mut [(ArrayKey, Value)],
    class_name: &str,
    mut parent: usize,
) {
    let normalized_class = normalize_class_name(class_name);
    loop {
        let left = parent.saturating_mul(2).saturating_add(1);
        let right = left.saturating_add(1);
        let mut best = parent;
        if left < entries.len()
            && spl_heap_compare_entries(&normalized_class, &entries[best].1, &entries[left].1)
                == std::cmp::Ordering::Greater
        {
            best = left;
        }
        if right < entries.len()
            && spl_heap_compare_entries(&normalized_class, &entries[best].1, &entries[right].1)
                == std::cmp::Ordering::Greater
        {
            best = right;
        }
        if best == parent {
            break;
        }
        entries.swap(parent, best);
        parent = best;
    }
}

pub(super) fn spl_heap_compare_entries(
    class_name: &str,
    left: &Value,
    right: &Value,
) -> std::cmp::Ordering {
    let normalized_class = normalize_class_name(class_name);
    if normalized_class == "splpriorityqueue" {
        let left_priority = spl_priority_queue_entry_parts(left)
            .map(|(_, priority)| priority)
            .unwrap_or(Value::Null);
        let right_priority = spl_priority_queue_entry_parts(right)
            .map(|(_, priority)| priority)
            .unwrap_or(Value::Null);
        return compare(&right_priority, &left_priority).unwrap_or(std::cmp::Ordering::Equal);
    }
    let ordering = compare(&effective_value(left), &effective_value(right))
        .unwrap_or(std::cmp::Ordering::Equal);
    if normalized_class == "splminheap" {
        ordering
    } else {
        ordering.reverse()
    }
}

pub(super) fn spl_heap_current_entry(
    object: &ObjectRef,
    class_name: &str,
) -> Option<(ArrayKey, Value)> {
    let normalized_class = normalize_class_name(class_name);
    let entries = spl_entries(object);
    let position = spl_position(object);
    let len = entries.len();
    let raw = entries.into_iter().nth(position)?.1;
    let iteration_key = ArrayKey::Int(len.saturating_sub(position + 1) as i64);
    if normalized_class == "splpriorityqueue" {
        let (data, priority) = spl_priority_queue_entry_parts(&raw)?;
        return Some((
            iteration_key,
            spl_priority_queue_extract_value(object, data, priority),
        ));
    }
    Some((iteration_key, raw))
}

pub(super) fn spl_heap_extract_value(object: &ObjectRef, class_name: &str, raw: Value) -> Value {
    if normalize_class_name(class_name) == "splpriorityqueue"
        && let Some((data, priority)) = spl_priority_queue_entry_parts(&raw)
    {
        return spl_priority_queue_extract_value(object, data, priority);
    }
    raw
}

pub(super) fn spl_priority_queue_entry(data: Value, priority: Value) -> Value {
    let mut entry = PhpArray::new();
    entry.insert(ArrayKey::String(PhpString::from_test_str("data")), data);
    entry.insert(
        ArrayKey::String(PhpString::from_test_str("priority")),
        priority,
    );
    Value::Array(entry)
}

pub(super) fn spl_priority_queue_entry_parts(value: &Value) -> Option<(Value, Value)> {
    let Value::Array(entry) = effective_value(value) else {
        return None;
    };
    let data = entry
        .get(&ArrayKey::String(PhpString::from_test_str("data")))
        .map(effective_value)?;
    let priority = entry
        .get(&ArrayKey::String(PhpString::from_test_str("priority")))
        .map(effective_value)?;
    Some((data, priority))
}

pub(super) fn spl_priority_queue_extract_value(
    object: &ObjectRef,
    data: Value,
    priority: Value,
) -> Value {
    spl_priority_queue_extract_value_from_flags(
        spl_priority_queue_extract_flags(object),
        data,
        priority,
    )
}

pub(super) fn spl_priority_queue_extract_value_from_flags(
    flags: i64,
    data: Value,
    priority: Value,
) -> Value {
    match flags {
        SPL_PRIORITY_QUEUE_EXTR_PRIORITY => priority,
        SPL_PRIORITY_QUEUE_EXTR_BOTH => spl_priority_queue_entry(data, priority),
        _ => data,
    }
}

pub(super) fn spl_priority_queue_extract_flags(object: &ObjectRef) -> i64 {
    match object
        .get_property("__extract_flags")
        .map(|value| effective_value(&value))
    {
        Some(Value::Int(flags)) => flags,
        _ => SPL_PRIORITY_QUEUE_EXTR_DATA,
    }
}

pub(super) fn is_spl_file_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "splfileinfo" | "splfileobject" | "spltempfileobject"
    )
}

pub(super) fn internal_spl_file_instanceof(object_class: &str, target_class: &str) -> Option<bool> {
    if !is_spl_file_runtime_class(object_class) {
        return None;
    }
    let object_class = normalize_class_name(object_class);
    let target_class = normalize_class_name(target_class);
    Some(match target_class.as_str() {
        "splfileinfo" => matches!(
            object_class.as_str(),
            "splfileinfo" | "splfileobject" | "spltempfileobject"
        ),
        "splfileobject" => matches!(object_class.as_str(), "splfileobject" | "spltempfileobject"),
        "spltempfileobject" => object_class == "spltempfileobject",
        "traversable" | "iterator" => {
            matches!(object_class.as_str(), "splfileobject" | "spltempfileobject")
        }
        "seekableiterator" | "recursiveiterator" => {
            matches!(object_class.as_str(), "splfileobject" | "spltempfileobject")
        }
        _ => false,
    })
}

pub(super) fn new_spl_file_object(
    class_name: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
) -> Result<ObjectRef, String> {
    if let Some(name) = args.iter().find_map(|arg| arg.name.as_deref()) {
        return Err(format!(
            "E_PHP_VM_UNKNOWN_NAMED_ARG: {class_name}::__construct has no builtin parameter ${name}"
        ));
    }
    let normalized = normalize_class_name(class_name);
    let object = ObjectRef::new_with_display_name(
        &spl_file_class(class_name),
        spl_file_display_name(class_name),
    );
    match normalized.as_str() {
        "splfileinfo" => {
            validate_spl_iterator_arg_count(class_name, &args, 1, 1)?;
            let path = to_string(&args[0].value)?.to_string_lossy();
            spl_file_set_path(&object, &path);
        }
        "splfileobject" => {
            validate_spl_iterator_arg_count(class_name, &args, 1, 2)?;
            let path = to_string(&args[0].value)?.to_string_lossy();
            let mode = args
                .get(1)
                .map(|arg| to_string(&arg.value).map(|value| value.to_string_lossy()))
                .transpose()?
                .unwrap_or_else(|| "r".to_owned());
            let content = spl_file_read_to_string(&path, runtime_context)?;
            spl_file_set_path(&object, &path);
            object.set_property("__mode", Value::string(mode.into_bytes()));
            spl_file_set_content(&object, content);
        }
        "spltempfileobject" => {
            validate_spl_iterator_arg_count(class_name, &args, 0, 1)?;
            spl_file_set_path(&object, "php://temp");
            object.set_property("__mode", Value::string(b"w+".to_vec()));
            object.set_property("__temp", Value::Bool(true));
            spl_file_set_content(&object, String::new());
        }
        _ => unreachable!("is_spl_file_runtime_class validates class names"),
    }
    object.set_property(
        SPL_RUNTIME_CLASS_PROPERTY,
        Value::string(normalized.into_bytes()),
    );
    spl_set_position(&object, 0);
    Ok(object)
}

pub(super) fn spl_file_method_is_supported(method: &str) -> bool {
    matches!(
        normalize_method_name(method).as_str(),
        "__tostring"
            | "getpathname"
            | "getfilename"
            | "getbasename"
            | "getextension"
            | "getpath"
            | "getpathinfo"
            | "getrealpath"
            | "realpath"
            | "getsize"
            | "getmtime"
            | "getlinktarget"
            | "isfile"
            | "isdir"
            | "islink"
            | "isreadable"
            | "rewind"
            | "eof"
            | "valid"
            | "key"
            | "current"
            | "next"
            | "fgets"
            | "fgetcsv"
            | "ftruncate"
            | "__construct"
    )
}

pub(super) fn call_spl_file_method(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    call_spl_file_method_with_context(object, method, args, runtime_context, None, None)
}

pub(super) fn call_spl_file_method_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
) -> Result<Value, String> {
    call_spl_file_method_with_context(
        object,
        method,
        args,
        runtime_context,
        Some(compiled),
        Some(state),
    )
}

pub(super) fn call_spl_file_method_with_context(
    object: &ObjectRef,
    method: &str,
    args: Vec<CallArgument>,
    runtime_context: &RuntimeContext,
    compiled: Option<&CompiledUnit>,
    state: Option<&ExecutionState>,
) -> Result<Value, String> {
    let class_name = object.class_name();
    let normalized_class =
        spl_runtime_marker(object).unwrap_or_else(|| normalize_class_name(&class_name));
    let method = normalize_method_name(method);
    match method.as_str() {
        "__tostring" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if matches!(
                normalized_class.as_str(),
                "splfileobject" | "spltempfileobject"
            ) {
                return Ok(spl_file_lines(object)
                    .get(spl_position(object))
                    .map(|line| Value::string(line.as_bytes().to_vec()))
                    .unwrap_or_else(|| Value::string(Vec::new())));
            }
            Ok(Value::string(spl_file_path(object).into_bytes()))
        }
        "getpathname" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::string(spl_file_path(object).into_bytes()))
        }
        "getfilename" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::string(
                spl_file_basename(&spl_file_path(object)).into_bytes(),
            ))
        }
        "getbasename" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 1)?;
            let mut base = spl_file_basename(&spl_file_path(object));
            if let Some(suffix) = args.first() {
                let suffix = to_string(&suffix.value)?.to_string_lossy();
                if !suffix.is_empty() && base.ends_with(&suffix) {
                    base.truncate(base.len() - suffix.len());
                }
            }
            Ok(Value::string(base.into_bytes()))
        }
        "getextension" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let base = spl_file_basename(&spl_file_path(object));
            let extension = base
                .rsplit_once('.')
                .map(|(_, extension)| extension)
                .unwrap_or("");
            Ok(Value::string(extension.as_bytes().to_vec()))
        }
        "getpath" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let path = spl_file_path(object);
            let parent = Path::new(&path)
                .parent()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(Value::string(parent.into_bytes()))
        }
        "getpathinfo" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 1)?;
            let path = spl_file_path(object);
            let parent = Path::new(&path)
                .parent()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (class, display_name) =
                spl_file_info_result_class(compiled, state, args.first(), "getPathInfo")?;
            let info = ObjectRef::new_with_display_name(&class, display_name);
            spl_file_set_path(&info, &parent);
            info.set_property(
                SPL_RUNTIME_CLASS_PROPERTY,
                Value::string(b"splfileinfo".to_vec()),
            );
            Ok(Value::Object(info))
        }
        "getrealpath" | "realpath" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let path = spl_file_resolve_path(&spl_file_path(object), runtime_context);
            if !runtime_context.filesystem.allows_path(&path) {
                return Ok(Value::Bool(false));
            }
            Ok(fs::canonicalize(&path)
                .ok()
                .map(|path| Value::string(path.to_string_lossy().into_owned().into_bytes()))
                .unwrap_or(Value::Bool(false)))
        }
        "getsize" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if matches!(normalized_class.as_str(), "spltempfileobject") {
                return Ok(Value::Int(spl_file_content(object).len() as i64));
            }
            Ok(Value::Int(
                spl_file_metadata(object, runtime_context)?.len() as i64,
            ))
        }
        "getmtime" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let modified = spl_file_metadata(object, runtime_context)?
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0);
            Ok(Value::Int(modified))
        }
        "getlinktarget" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let path = spl_file_resolve_path(&spl_file_path(object), runtime_context);
            if !runtime_context.filesystem.allows_path(&path) {
                return Ok(Value::Bool(false));
            }
            Ok(fs::read_link(&path)
                .ok()
                .map(|path| Value::string(path.to_string_lossy().into_owned().into_bytes()))
                .unwrap_or(Value::Bool(false)))
        }
        "isfile" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Bool(
                spl_file_metadata(object, runtime_context)
                    .map(|metadata| metadata.is_file())
                    .unwrap_or(false),
            ))
        }
        "isdir" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Bool(
                spl_file_metadata(object, runtime_context)
                    .map(|metadata| metadata.is_dir())
                    .unwrap_or(false),
            ))
        }
        "islink" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let path = spl_file_resolve_path(&spl_file_path(object), runtime_context);
            if !runtime_context.filesystem.allows_path(&path) {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(
                fs::symlink_metadata(path)
                    .map(|metadata| metadata.file_type().is_symlink())
                    .unwrap_or(false),
            ))
        }
        "isreadable" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let path = spl_file_resolve_path(&spl_file_path(object), runtime_context);
            Ok(Value::Bool(
                runtime_context.filesystem.allows_path(&path) && fs::File::open(path).is_ok(),
            ))
        }
        "rewind" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            spl_set_position(object, 0);
            Ok(Value::Null)
        }
        "eof" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            if normalized_class == "spltempfileobject"
                && spl_position(object) == 0
                && spl_file_content(object).is_empty()
            {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(
                spl_position(object) >= spl_file_lines(object).len(),
            ))
        }
        "valid" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Bool(
                spl_position(object) < spl_file_lines(object).len(),
            ))
        }
        "key" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(Value::Int(spl_position(object) as i64))
        }
        "current" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            Ok(spl_file_lines(object)
                .get(spl_position(object))
                .map(|line| Value::string(line.as_bytes().to_vec()))
                .unwrap_or(Value::Null))
        }
        "next" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            spl_set_position(object, spl_position(object).saturating_add(1));
            Ok(Value::Null)
        }
        "fgets" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 0)?;
            let lines = spl_file_lines(object);
            let pos = spl_position(object);
            if let Some(line) = lines.get(pos) {
                spl_set_position(object, pos.saturating_add(1));
                Ok(Value::string(line.as_bytes().to_vec()))
            } else {
                Ok(Value::Bool(false))
            }
        }
        "fgetcsv" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, 1)?;
            let delimiter = args
                .first()
                .map(|arg| to_string(&arg.value).map(|value| value.to_string_lossy()))
                .transpose()?
                .and_then(|value| value.bytes().next())
                .unwrap_or(b',');
            let line = match call_spl_file_method(object, "fgets", Vec::new(), runtime_context)? {
                Value::String(line) => line.to_string_lossy(),
                _ => return Ok(Value::Bool(false)),
            };
            let fields = line
                .trim_end_matches(['\r', '\n'])
                .split(delimiter as char)
                .map(|field| Value::string(field.as_bytes().to_vec()))
                .collect();
            Ok(Value::packed_array(fields))
        }
        "ftruncate" => {
            validate_spl_iterator_arg_count(&class_name, &args, 1, 1)?;
            let size = to_int(&args[0].value)?;
            if size < 0 {
                return Err(
                    "E_PHP_VM_SPL_VALUE_ERROR: SplFileObject::ftruncate(): Argument #1 ($size) must be greater than or equal to 0"
                        .to_owned(),
                );
            }
            let mut content = spl_file_content(object).into_bytes();
            content.resize(size as usize, 0);
            spl_file_set_content(object, String::from_utf8_lossy(&content).into_owned());
            Ok(Value::Bool(true))
        }
        "__construct" => {
            validate_spl_iterator_arg_count(&class_name, &args, 0, usize::MAX)?;
            if spl_file_is_initialized(object) {
                return Err("E_PHP_VM_UNKNOWN_METHOD: Cannot call constructor twice".to_owned());
            }
            Ok(Value::Null)
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {class_name}::{method} is not defined"
        )),
    }
}

pub(super) fn spl_file_info_result_class(
    compiled: Option<&CompiledUnit>,
    state: Option<&ExecutionState>,
    class_arg: Option<&CallArgument>,
    method: &str,
) -> Result<(RuntimeClassEntry, String), String> {
    let Some(class_arg) = class_arg else {
        return Ok((
            spl_file_class("SplFileInfo"),
            spl_file_display_name("SplFileInfo").to_owned(),
        ));
    };
    let class_value = effective_value(&class_arg.value);
    if matches!(class_value, Value::Null) {
        return Ok((
            spl_file_class("SplFileInfo"),
            spl_file_display_name("SplFileInfo").to_owned(),
        ));
    }
    let Value::String(class_name) = class_value else {
        return Err(format!(
            "E_PHP_VM_SPL_TYPE_ERROR: SplFileInfo::{method}(): Argument #1 ($class) must be a class name derived from SplFileInfo or null, {} given",
            value_type_name(&class_arg.value)
        ));
    };
    let class_name = class_name.to_string_lossy();
    let Some(compiled) = compiled else {
        return Ok(spl_file_info_result_base_class(&class_name));
    };
    let Some(state) = state else {
        return Ok(spl_file_info_result_base_class(&class_name));
    };
    if !class_is_a_in_state(compiled, state, &class_name, "SplFileInfo")? {
        return Err(format!(
            "E_PHP_VM_SPL_TYPE_ERROR: SplFileInfo::{method}(): Argument #1 ($class) must be a class name derived from SplFileInfo or null, {class_name} given"
        ));
    }
    Ok(lookup_class_in_state(compiled, state, &class_name)
        .map(|class| {
            let display_name = class.display_name.clone();
            (
                RuntimeClassEntry {
                    name: normalize_class_name(&class.name),
                    parent: class.parent.clone(),
                    interfaces: class.interfaces.clone(),
                    methods: Vec::new(),
                    properties: Vec::new(),
                    constants: Vec::new(),
                    enum_cases: Vec::new(),
                    attributes: Vec::new(),
                    enum_backing_type: None,
                    constructor_id: None,
                    flags: RuntimeClassFlags::default(),
                },
                display_name,
            )
        })
        .unwrap_or_else(|| spl_file_info_result_base_class(&class_name)))
}

pub(super) fn spl_file_info_result_base_class(class_name: &str) -> (RuntimeClassEntry, String) {
    if normalize_class_name(class_name) == "splfileinfo" {
        (
            spl_file_class("SplFileInfo"),
            spl_file_display_name("SplFileInfo").to_owned(),
        )
    } else {
        (
            RuntimeClassEntry {
                name: normalize_class_name(class_name),
                parent: Some(normalize_class_name("SplFileInfo")),
                interfaces: Vec::new(),
                methods: Vec::new(),
                properties: Vec::new(),
                constants: Vec::new(),
                enum_cases: Vec::new(),
                attributes: Vec::new(),
                enum_backing_type: None,
                constructor_id: None,
                flags: RuntimeClassFlags::default(),
            },
            class_name.to_owned(),
        )
    }
}

pub(super) fn spl_file_class(class_name: &str) -> RuntimeClassEntry {
    let normalized = normalize_class_name(class_name);
    RuntimeClassEntry {
        name: normalize_class_name(class_name),
        parent: match normalized.as_str() {
            "splfileobject" | "spltempfileobject" => Some(normalize_class_name("SplFileInfo")),
            _ => None,
        },
        interfaces: if matches!(normalized.as_str(), "splfileobject" | "spltempfileobject") {
            vec![
                "Iterator".to_owned(),
                "Traversable".to_owned(),
                "SeekableIterator".to_owned(),
                "RecursiveIterator".to_owned(),
            ]
        } else {
            Vec::new()
        },
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: RuntimeClassFlags::default(),
    }
}

pub(super) fn spl_file_display_name(class_name: &str) -> &'static str {
    match normalize_class_name(class_name).as_str() {
        "splfileinfo" => "SplFileInfo",
        "splfileobject" => "SplFileObject",
        "spltempfileobject" => "SplTempFileObject",
        _ => "SplFileInfo",
    }
}

pub(super) fn spl_file_set_path(object: &ObjectRef, path: &str) {
    object.set_property("__path", Value::string(path.as_bytes().to_vec()));
}

pub(super) fn spl_file_path(object: &ObjectRef) -> String {
    match object
        .get_property("__path")
        .map(|value| effective_value(&value))
    {
        Some(Value::String(path)) => path.to_string_lossy(),
        _ => String::new(),
    }
}

pub(super) fn spl_file_is_initialized(object: &ObjectRef) -> bool {
    object.get_property("__path").is_some()
}

pub(super) fn spl_file_basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(super) fn spl_file_resolve_path(path: &str, runtime_context: &RuntimeContext) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        runtime_context.cwd.join(path)
    }
}

pub(super) fn spl_file_metadata(
    object: &ObjectRef,
    runtime_context: &RuntimeContext,
) -> Result<fs::Metadata, String> {
    let path = spl_file_resolve_path(&spl_file_path(object), runtime_context);
    if !runtime_context.filesystem.allows_path(&path) {
        return Err(format!(
            "E_PHP_VM_SPL_FILE_DENIED: local file access denied for `{}`",
            path.to_string_lossy()
        ));
    }
    fs::metadata(&path).map_err(|error| {
        format!(
            "E_PHP_VM_SPL_FILE_STAT: failed to stat `{}`: {error}",
            path.to_string_lossy()
        )
    })
}

pub(super) fn spl_file_read_to_string(
    path: &str,
    runtime_context: &RuntimeContext,
) -> Result<String, String> {
    let path = spl_file_resolve_path(path, runtime_context);
    if !runtime_context.filesystem.allows_path(&path) {
        return Err(format!(
            "E_PHP_VM_SPL_FILE_DENIED: local file access denied for `{}`",
            path.to_string_lossy()
        ));
    }
    fs::read_to_string(&path).map_err(|error| {
        format!(
            "E_PHP_VM_SPL_FILE_READ: failed to read `{}`: {error}",
            path.to_string_lossy()
        )
    })
}

pub(super) fn spl_file_set_content(object: &ObjectRef, content: String) {
    let lines = content
        .split_inclusive('\n')
        .map(|line| Value::string(line.as_bytes().to_vec()))
        .collect();
    object.set_property("__content", Value::string(content.into_bytes()));
    object.set_property("__lines", Value::packed_array(lines));
}

pub(super) fn spl_file_content(object: &ObjectRef) -> String {
    match object
        .get_property("__content")
        .map(|value| effective_value(&value))
    {
        Some(Value::String(content)) => content.to_string_lossy(),
        _ => String::new(),
    }
}

pub(super) fn spl_file_lines(object: &ObjectRef) -> Vec<String> {
    let Some(Value::Array(lines)) = object.get_property("__lines") else {
        return Vec::new();
    };
    lines
        .iter()
        .filter_map(|(_, value)| match effective_value(value) {
            Value::String(line) => Some(line.to_string_lossy()),
            _ => None,
        })
        .collect()
}
