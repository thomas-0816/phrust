//! Streams builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, StreamWrapperRegistry, Value};
use std::path::Path;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("closedir", builtin_closedir, BuiltinCompatibility::Php),
    BuiltinEntry::new("fclose", builtin_fclose, BuiltinCompatibility::Php),
    BuiltinEntry::new("feof", builtin_feof, BuiltinCompatibility::Php),
    BuiltinEntry::new("fflush", builtin_fflush, BuiltinCompatibility::Php),
    BuiltinEntry::new("fgetc", builtin_fgetc, BuiltinCompatibility::Php),
    BuiltinEntry::new("fgets", builtin_fgets, BuiltinCompatibility::Php),
    BuiltinEntry::new("fopen", builtin_fopen, BuiltinCompatibility::Php),
    BuiltinEntry::new("fprintf", builtin_fprintf, BuiltinCompatibility::Php),
    BuiltinEntry::new("fread", builtin_fread, BuiltinCompatibility::Php),
    BuiltinEntry::new("fseek", builtin_fseek, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftell", builtin_ftell, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftruncate", builtin_ftruncate, BuiltinCompatibility::Php),
    BuiltinEntry::new("fwrite", builtin_fwrite, BuiltinCompatibility::Php),
    BuiltinEntry::new("vfprintf", builtin_vfprintf, BuiltinCompatibility::Php),
    BuiltinEntry::new("opendir", builtin_opendir, BuiltinCompatibility::Php),
    BuiltinEntry::new("readdir", builtin_readdir, BuiltinCompatibility::Php),
    BuiltinEntry::new("rewind", builtin_rewind, BuiltinCompatibility::Php),
    BuiltinEntry::new("rewinddir", builtin_rewinddir, BuiltinCompatibility::Php),
    BuiltinEntry::new("scandir", builtin_scandir, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "stream_context_create",
        builtin_stream_context_create,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_context_get_options",
        builtin_stream_context_get_options,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_context_set_option",
        builtin_stream_context_set_option,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_copy_to_stream",
        builtin_stream_copy_to_stream,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_get_contents",
        builtin_stream_get_contents,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_get_meta_data",
        builtin_stream_get_meta_data,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_get_wrappers",
        builtin_stream_get_wrappers,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_is_local",
        builtin_stream_is_local,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_isatty",
        builtin_stream_isatty,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_resolve_include_path",
        builtin_stream_resolve_include_path,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_fprintf(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 {
        return Err(arity_error("fprintf", "two or more argument(s)"));
    }
    let Some(stream) = resource_arg(&args[0]) else {
        return Err(type_error("fprintf", "resource", &args[0]));
    };
    let format = string_arg("fprintf", &args[1])?;
    let rendered = php_format("fprintf", format.as_bytes(), &args[2..], context, span)?;
    let written = stream
        .write_bytes(&rendered)
        .map_err(|error| value_error("fprintf", &error.to_string()))?;
    Ok(Value::Int(written as i64))
}
pub(in crate::builtins::modules) fn builtin_vfprintf(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            format!(
                "vfprintf() expects exactly 3 arguments, {} given",
                args.len()
            ),
        ));
    }
    let Some(stream) = resource_arg(&args[0]) else {
        return Err(argument_type_error(
            "vfprintf",
            "#1 ($stream)",
            "resource",
            &args[0],
        ));
    };
    let format = string_needle_arg("vfprintf", "#2 ($format)", &args[1])?;
    let values = format_array_values("vfprintf", "#3 ($values)", &args[2])?;
    let rendered = php_format("vfprintf", format.as_bytes(), &values, context, span)?;
    let written = stream
        .write_bytes(&rendered)
        .map_err(|error| value_error("vfprintf", &error.to_string()))?;
    Ok(Value::Int(written as i64))
}
pub(in crate::builtins::modules) fn builtin_fopen(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fopen", &args, 2)?;
    let uri = string_arg("fopen", &args[0])?.to_string_lossy();
    let mode = string_arg("fopen", &args[1])?.to_string_lossy();
    let cwd = context.cwd().to_path_buf();
    let filesystem = context.filesystem_capabilities().clone();
    let open_result = {
        let Some(resources) = context.resources() else {
            return Ok(Value::Bool(false));
        };
        StreamWrapperRegistry::new().open(resources, &uri, &mode, &cwd, &filesystem)
    };
    match open_result {
        Ok(resource) => Ok(Value::Resource(resource)),
        Err(error) => {
            context.php_warning(
                error.diagnostic_id(),
                format!("fopen({uri}): {}", error.message()),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}
pub(in crate::builtins::modules) fn builtin_fclose(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fclose", &args, 1)?;
    Ok(resource_arg(&args[0]).map_or(Value::Bool(false), |resource| Value::Bool(resource.close())))
}
pub(in crate::builtins::modules) fn builtin_fread(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fread", &args, 2)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let length = int_arg("fread", &args[1])?.max(0) as usize;
    Ok(resource
        .read_bytes(length)
        .map_or(Value::Bool(false), Value::string))
}
pub(in crate::builtins::modules) fn builtin_fwrite(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("fwrite", "two or three argument(s)"));
    }
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let mut bytes = string_arg("fwrite", &args[1])?.as_bytes().to_vec();
    if let Some(length) = args.get(2) {
        bytes.truncate(int_arg("fwrite", length)?.max(0) as usize);
    }
    Ok(resource
        .write_bytes(&bytes)
        .map_or(Value::Bool(false), |written| Value::Int(written as i64)))
}
pub(in crate::builtins::modules) fn builtin_fgets(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("fgets", "one or two argument(s)"));
    }
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let mut line = resource.read_line().unwrap_or_default();
    if let Some(length) = args.get(1) {
        line.truncate(int_arg("fgets", length)?.max(0) as usize);
    }
    if line.is_empty() {
        Ok(Value::Bool(false))
    } else {
        Ok(Value::string(line))
    }
}
pub(in crate::builtins::modules) fn builtin_fgetc(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fgetc", &args, 1)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let byte = resource.read_bytes(1).unwrap_or_default();
    if byte.is_empty() {
        Ok(Value::Bool(false))
    } else {
        Ok(Value::string(byte))
    }
}
pub(in crate::builtins::modules) fn builtin_feof(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("feof", &args, 1)?;
    Ok(
        resource_arg(&args[0]).map_or(Value::Bool(true), |resource| {
            Value::Bool(resource.eof().unwrap_or(true))
        }),
    )
}
pub(in crate::builtins::modules) fn builtin_fflush(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fflush", &args, 1)?;
    Ok(
        resource_arg(&args[0]).map_or(Value::Bool(false), |resource| {
            Value::Bool(resource.flush().is_ok())
        }),
    )
}
pub(in crate::builtins::modules) fn builtin_fseek(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("fseek", "two or three argument(s)"));
    }
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Int(-1));
    };
    let offset = int_arg("fseek", &args[1])?.max(0) as usize;
    Ok(if resource.seek(offset).is_ok() {
        Value::Int(0)
    } else {
        Value::Int(-1)
    })
}
pub(in crate::builtins::modules) fn builtin_ftell(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("ftell", &args, 1)?;
    Ok(
        resource_arg(&args[0]).map_or(Value::Bool(false), |resource| {
            resource
                .tell()
                .map_or(Value::Bool(false), |offset| Value::Int(offset as i64))
        }),
    )
}
pub(in crate::builtins::modules) fn builtin_ftruncate(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("ftruncate", &args, 2)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let size = int_arg("ftruncate", &args[1])?;
    if size < 0 {
        return Err(value_error(
            "ftruncate",
            "size must be greater than or equal to 0",
        ));
    }
    Ok(Value::Bool(resource.truncate(size as usize).is_ok()))
}
pub(in crate::builtins::modules) fn builtin_rewind(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("rewind", &args, 1)?;
    Ok(
        resource_arg(&args[0]).map_or(Value::Bool(false), |resource| {
            Value::Bool(resource.rewind().is_ok())
        }),
    )
}
pub(in crate::builtins::modules) fn builtin_opendir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("opendir", &args, 1)?;
    let path = string_arg("opendir", &args[0])?.to_string_lossy();
    let resolved = resolve_runtime_path(context, &path);
    if !context.filesystem_capabilities().allows_path(&resolved) || !resolved.is_dir() {
        return Ok(Value::Bool(false));
    }
    let Some(entries) = directory_entries_with_dots(&resolved) else {
        return Ok(Value::Bool(false));
    };
    let uri = resolved.to_string_lossy().to_string();
    let Some(resources) = context.resources() else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Resource(
        resources.register_directory(resolved, entries, uri),
    ))
}
pub(in crate::builtins::modules) fn builtin_readdir(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("readdir", "zero or one argument(s)"));
    }
    let Some(resource) = args.first().and_then(resource_arg) else {
        return Ok(Value::Bool(false));
    };
    Ok(resource
        .read_dir_entry()
        .ok()
        .flatten()
        .map_or(Value::Bool(false), Value::string))
}
pub(in crate::builtins::modules) fn builtin_rewinddir(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("rewinddir", "zero or one argument(s)"));
    }
    let Some(resource) = args.first().and_then(resource_arg) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(resource.rewind_dir().is_ok()))
}
pub(in crate::builtins::modules) fn builtin_closedir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("closedir", &args, 1)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let Some(resources) = context.resources() else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(resources.close(resource.id())))
}
pub(in crate::builtins::modules) fn builtin_stream_get_wrappers(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_get_wrappers", &args, 0)?;
    Ok(Value::packed_array(vec![
        Value::string("file"),
        Value::string("php"),
    ]))
}
pub(in crate::builtins::modules) fn builtin_stream_get_meta_data(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_get_meta_data", &args, 1)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let metadata = resource.metadata();
    let flags = resource.flags();
    let mut array = crate::PhpArray::new();
    array.insert(
        string_array_key("wrapper_type"),
        Value::string(metadata.wrapper_type),
    );
    array.insert(
        string_array_key("stream_type"),
        Value::string(metadata.stream_type),
    );
    array.insert(string_array_key("mode"), Value::string(metadata.mode));
    array.insert(string_array_key("uri"), Value::string(metadata.uri));
    array.insert(string_array_key("seekable"), Value::Bool(flags.seekable));
    array.insert(
        string_array_key("eof"),
        Value::Bool(resource.eof().unwrap_or(true)),
    );
    array.insert(string_array_key("timed_out"), Value::Bool(false));
    array.insert(string_array_key("blocked"), Value::Bool(true));
    Ok(Value::Array(array))
}
pub(in crate::builtins::modules) fn builtin_stream_get_contents(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error(
            "stream_get_contents",
            "one to three argument(s)",
        ));
    }
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    if let Some(offset) = args
        .get(2)
        .map(|value| int_arg("stream_get_contents", value))
        .transpose()?
        && offset >= 0
        && resource.seek(offset as usize).is_err()
    {
        return Ok(Value::Bool(false));
    }
    let bytes = if let Some(length) = args
        .get(1)
        .map(|value| int_arg("stream_get_contents", value))
        .transpose()?
    {
        if length < 0 {
            resource.read_to_end()
        } else {
            resource.read_bytes(length as usize)
        }
    } else {
        resource.read_to_end()
    };
    Ok(bytes.map_or(Value::Bool(false), Value::string))
}
pub(in crate::builtins::modules) fn builtin_stream_copy_to_stream(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error(
            "stream_copy_to_stream",
            "two to four argument(s)",
        ));
    }
    let Some(source) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let Some(destination) = resource_arg(&args[1]) else {
        return Ok(Value::Bool(false));
    };
    if let Some(offset) = args
        .get(3)
        .map(|value| int_arg("stream_copy_to_stream", value))
        .transpose()?
        && offset >= 0
        && source.seek(offset as usize).is_err()
    {
        return Ok(Value::Bool(false));
    }
    let bytes = if let Some(length) = args
        .get(2)
        .map(|value| int_arg("stream_copy_to_stream", value))
        .transpose()?
    {
        if length < 0 {
            source.read_to_end()
        } else {
            source.read_bytes(length as usize)
        }
    } else {
        source.read_to_end()
    };
    let Ok(bytes) = bytes else {
        return Ok(Value::Bool(false));
    };
    Ok(destination
        .write_bytes(&bytes)
        .map(|written| Value::Int(written as i64))
        .unwrap_or(Value::Bool(false)))
}
pub(in crate::builtins::modules) fn builtin_stream_context_create(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "stream_context_create",
            "zero or one argument(s)",
        ));
    }
    let options = match args.first().map(deref_value) {
        None => crate::PhpArray::new(),
        Some(Value::Array(array)) => array,
        Some(_) => return Ok(Value::Bool(false)),
    };
    let Some(resources) = context.resources() else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Resource(resources.register_stream_context(options)))
}
pub(in crate::builtins::modules) fn builtin_stream_context_get_options(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_context_get_options", &args, 1)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    Ok(resource
        .context_options()
        .map_or(Value::Bool(false), Value::Array))
}
pub(in crate::builtins::modules) fn builtin_stream_context_set_option(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 && args.len() != 4 {
        return Err(arity_error(
            "stream_context_set_option",
            "two or four argument(s)",
        ));
    }
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    if args.len() == 2 {
        let Value::Array(options) = deref_value(&args[1]) else {
            return Ok(Value::Bool(false));
        };
        for (wrapper_key, wrapper_value) in options.iter() {
            let wrapper = match wrapper_key {
                ArrayKey::String(wrapper) => wrapper.to_string_lossy(),
                ArrayKey::Int(_) => continue,
            };
            let Value::Array(wrapper_options) = deref_value(wrapper_value) else {
                continue;
            };
            for (option_key, option_value) in wrapper_options.iter() {
                let option = match option_key {
                    ArrayKey::String(option) => option.to_string_lossy(),
                    ArrayKey::Int(_) => continue,
                };
                if resource
                    .set_context_option(wrapper.clone(), option, option_value.clone())
                    .is_err()
                {
                    return Ok(Value::Bool(false));
                }
            }
        }
        return Ok(Value::Bool(true));
    }
    let wrapper = string_arg("stream_context_set_option", &args[1])?.to_string_lossy();
    let option = string_arg("stream_context_set_option", &args[2])?.to_string_lossy();
    Ok(Value::Bool(
        resource
            .set_context_option(wrapper, option, args[3].clone())
            .is_ok(),
    ))
}
pub(in crate::builtins::modules) fn builtin_stream_resolve_include_path(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_resolve_include_path", &args, 1)?;
    let file = string_arg("stream_resolve_include_path", &args[0])?.to_string_lossy();
    let raw = Path::new(&file);
    let mut candidates = Vec::new();
    if raw.is_absolute() {
        candidates.push(normalize_runtime_path(raw));
    } else {
        for entry in context.include_path() {
            let base = if entry.is_absolute() {
                entry.clone()
            } else {
                context.cwd().join(entry)
            };
            candidates.push(normalize_runtime_path(&base.join(raw)));
        }
    }
    for candidate in candidates {
        if context.filesystem_capabilities().allows_path(&candidate) && candidate.exists() {
            return Ok(Value::string(
                candidate.to_string_lossy().as_bytes().to_vec(),
            ));
        }
    }
    Ok(Value::Bool(false))
}
pub(in crate::builtins::modules) fn builtin_stream_is_local(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_is_local", &args, 1)?;
    match deref_value(&args[0]) {
        Value::Resource(resource) => {
            let metadata = resource.metadata();
            Ok(Value::Bool(matches!(
                metadata.wrapper_type.as_str(),
                "plainfile" | "PHP"
            )))
        }
        Value::String(path) => {
            let path = path.to_string_lossy();
            if is_remote_stream_uri(&path) {
                return Ok(Value::Bool(false));
            }
            if path.starts_with("php://") {
                return Ok(Value::Bool(true));
            }
            let resolved = resolve_runtime_path(context, &path);
            Ok(Value::Bool(
                context.filesystem_capabilities().allows_path(&resolved),
            ))
        }
        _ => Ok(Value::Bool(false)),
    }
}
pub(in crate::builtins::modules) fn builtin_stream_isatty(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_isatty", &args, 1)?;
    Ok(Value::Bool(false))
}

pub(in crate::builtins::modules) fn builtin_scandir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("scandir", "one or two argument(s)"));
    }
    let path = resolve_runtime_path(context, &string_arg("scandir", &args[0])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) || !path.is_dir() {
        return Ok(Value::Bool(false));
    }
    let Some(mut entries) = directory_entries_with_dots(&path) else {
        return Ok(Value::Bool(false));
    };
    if args
        .get(1)
        .map(|value| int_arg("scandir", value))
        .transpose()?
        == Some(1)
    {
        entries.reverse();
    }
    Ok(Value::packed_array(
        entries.into_iter().map(Value::string).collect(),
    ))
}
