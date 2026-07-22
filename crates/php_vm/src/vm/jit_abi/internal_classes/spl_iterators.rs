use super::*;

fn spl_iterator_object(
    class_name: &'static str,
    display_name: &'static str,
) -> php_runtime::api::ObjectRef {
    let class = php_runtime::api::ClassEntry {
        name: std::sync::Arc::from(class_name),
        parent: None,
        interfaces: vec!["traversable".to_owned(), "iterator".to_owned()],
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: php_runtime::api::ClassFlags::default(),
    };
    php_runtime::api::ObjectRef::new_with_display_name(&class, display_name)
}

fn dereference(mut value: Value) -> Value {
    for _ in 0..16 {
        let Value::Reference(reference) = value else {
            return value;
        };
        value = reference.get();
    }
    value
}

fn collect_recursive_directory_paths(
    directory: &std::path::Path,
    paths: &mut Vec<std::path::PathBuf>,
) -> Result<(), String> {
    let mut children = std::fs::read_dir(directory)
        .map_err(|error| format!("RecursiveDirectoryIterator could not open directory: {error}"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("RecursiveDirectoryIterator could not read directory: {error}"))?;
    children.sort();
    for path in children {
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            format!("RecursiveDirectoryIterator could not inspect path: {error}")
        })?;
        if metadata.is_dir() {
            collect_recursive_directory_paths(&path, paths)?;
        } else if metadata.is_file() {
            paths.push(path);
        }
    }
    Ok(())
}

pub(in crate::vm::jit_abi) fn construct_native_spl_iterator(
    context: &mut NativeRequestColdState<'_>,
    class_name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let normalized = normalize_class_name(class_name);
    let (canonical, display) = match normalized.as_str() {
        "arrayiterator" => ("arrayiterator", "ArrayIterator"),
        "recursivearrayiterator" => ("recursivearrayiterator", "RecursiveArrayIterator"),
        "recursivedirectoryiterator" => {
            ("recursivedirectoryiterator", "RecursiveDirectoryIterator")
        }
        "recursiveiteratoriterator" => ("recursiveiteratoriterator", "RecursiveIteratorIterator"),
        "regexiterator" => ("regexiterator", "RegexIterator"),
        "recursiveregexiterator" => ("recursiveregexiterator", "RecursiveRegexIterator"),
        _ => return None,
    };
    let result = (|| {
        let object = spl_iterator_object(canonical, display);
        match normalized.as_str() {
            "arrayiterator" | "recursivearrayiterator" => {
                let entries = arguments
                    .first()
                    .map(|value| context.decode(*value))
                    .transpose()?
                    .map(dereference)
                    .unwrap_or_else(|| Value::Array(php_runtime::api::PhpArray::new()));
                let Value::Array(entries) = entries else {
                    return Err(format!(
                        "{display}::__construct(): Argument #1 ($array) must be of type array, {} given",
                        native_value_type_name(&entries)
                    ));
                };
                object.set_property("__entries", Value::Array(entries));
            }
            "recursiveiteratoriterator" => {
                let inner = arguments
                    .first()
                    .ok_or_else(|| {
                        "RecursiveIteratorIterator::__construct() expects an iterator".to_owned()
                    })
                    .and_then(|value| context.decode(*value))?;
                let inner = dereference(inner);
                let Value::Object(inner) = inner else {
                    return Err(
                        "RecursiveIteratorIterator::__construct() expects an iterator".to_owned(),
                    );
                };
                object.set_property("__inner", Value::Object(inner));
            }
            "recursivedirectoryiterator" => {
                let path = arguments
                    .first()
                    .ok_or_else(|| {
                        "RecursiveDirectoryIterator::__construct() expects a path".to_owned()
                    })
                    .and_then(|value| context.decode(*value))
                    .map(dereference)
                    .and_then(native_string)?;
                let path = std::path::PathBuf::from(String::from_utf8_lossy(&path).into_owned());
                let mut paths = Vec::new();
                collect_recursive_directory_paths(&path, &mut paths)?;
                let mut entries = php_runtime::api::PhpArray::new();
                for path in paths {
                    let path = path.to_string_lossy().into_owned().into_bytes();
                    let path = PhpString::from_bytes(path);
                    entries.insert(
                        php_runtime::api::ArrayKey::String(path.clone()),
                        Value::String(path),
                    );
                }
                object.set_property("__entries", Value::Array(entries));
            }
            "regexiterator" | "recursiveregexiterator" => {
                let inner = arguments
                    .first()
                    .ok_or_else(|| format!("{display}::__construct() expects an iterator"))
                    .and_then(|value| context.decode(*value))
                    .map(dereference)?;
                let Value::Object(inner) = inner else {
                    return Err(format!("{display}::__construct() expects an iterator"));
                };
                let pattern = arguments
                    .get(1)
                    .ok_or_else(|| format!("{display}::__construct() expects a regex"))
                    .and_then(|value| context.decode(*value))
                    .map(dereference)
                    .and_then(native_string)?;
                let pattern = PhpString::from_bytes(pattern);
                let mut cache = php_runtime::experimental::pcre::PcreCache::default();
                let compiled = cache
                    .compile(&pattern)
                    .map_err(|error| format!("{display} regex is invalid: {}", error.message()))?;
                let mode = arguments
                    .get(2)
                    .map(|value| context.decode(*value))
                    .transpose()?
                    .map(dereference)
                    .and_then(|value| match value {
                        Value::Int(value) => Some(value),
                        _ => None,
                    })
                    .unwrap_or(0);
                let mut entries = php_runtime::api::PhpArray::new();
                for (key, value) in native_spl_iterator_entries(&inner).unwrap_or_default() {
                    let subject = native_string(value.clone())?;
                    let Some(captures) = compiled.captures(&subject).map_err(|error| {
                        format!("{display} regex match failed: {}", error.message())
                    })?
                    else {
                        continue;
                    };
                    let value = if mode == 1 {
                        let matches = (0..captures.len())
                            .map(|index| {
                                let capture = captures.get(index);
                                capture.map_or(
                                    Value::String(PhpString::from_bytes(Vec::new())),
                                    |capture| {
                                        Value::String(PhpString::from_bytes(
                                            subject[capture.start()..capture.end()].to_vec(),
                                        ))
                                    },
                                )
                            })
                            .collect::<Vec<_>>();
                        Value::Array(php_runtime::api::PhpArray::from_packed(matches))
                    } else {
                        value
                    };
                    let key = match key {
                        Value::Int(key) => php_runtime::api::ArrayKey::Int(key),
                        Value::String(key) => php_runtime::api::ArrayKey::String(key),
                        _ => continue,
                    };
                    entries.insert(key, value);
                }
                object.set_property("__entries", Value::Array(entries));
            }
            _ => return Err(format!("unsupported SPL iterator class {display}")),
        }
        context.encode(Value::Object(object))
    })();
    Some(result)
}

fn array_entries(array: &php_runtime::api::PhpArray) -> Vec<(Value, Value)> {
    array
        .iter()
        .map(|(key, value)| {
            let key = match key {
                php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                php_runtime::api::ArrayKey::String(key) => Value::String(key.clone()),
            };
            (key, value.clone())
        })
        .collect()
}

fn flatten_array(array: &php_runtime::api::PhpArray, entries: &mut Vec<(Value, Value)>) {
    for (key, value) in array.iter() {
        match dereference(value.clone()) {
            Value::Array(nested) => flatten_array(&nested, entries),
            value => {
                let key = match key {
                    php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                    php_runtime::api::ArrayKey::String(key) => Value::String(key.clone()),
                };
                entries.push((key, value));
            }
        }
    }
}

pub(in crate::vm::jit_abi) fn native_spl_iterator_entries(
    object: &php_runtime::api::ObjectRef,
) -> Option<Vec<(Value, Value)>> {
    match normalize_class_name(&object.class_name()).as_str() {
        "arrayiterator"
        | "recursivearrayiterator"
        | "recursivedirectoryiterator"
        | "regexiterator"
        | "recursiveregexiterator" => {
            let Value::Array(entries) = object.get_property("__entries")? else {
                return None;
            };
            Some(array_entries(&entries))
        }
        "recursiveiteratoriterator" => {
            let Value::Object(inner) = object.get_property("__inner").map(dereference)? else {
                return None;
            };
            let Value::Array(entries) = inner.get_property("__entries").map(dereference)? else {
                return None;
            };
            let mut flattened = Vec::new();
            flatten_array(&entries, &mut flattened);
            Some(flattened)
        }
        _ => None,
    }
}

pub(in crate::vm::jit_abi) fn spl_iterator_class_constant(
    class_name: &str,
    constant: &str,
) -> Option<Value> {
    match (
        normalize_class_name(class_name).as_str(),
        constant.to_ascii_lowercase().as_str(),
    ) {
        ("regexiterator" | "recursiveregexiterator", "get_match") => Some(Value::Int(1)),
        ("filesystemiterator" | "recursivedirectoryiterator", "skip_dots") => {
            Some(Value::Int(4096))
        }
        ("filesystemiterator" | "recursivedirectoryiterator", "unix_paths") => {
            Some(Value::Int(8192))
        }
        _ => None,
    }
}
