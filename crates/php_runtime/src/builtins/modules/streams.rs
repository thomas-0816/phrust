//! Streams builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{
    ArrayKey, ResourceRef, StreamFilterMode, StreamSeekWhence, StreamWrapperRegistry, Value,
};
use std::io::Write;
use std::path::Path;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("closedir", builtin_closedir, BuiltinCompatibility::Php),
    BuiltinEntry::new("dir", builtin_dir, BuiltinCompatibility::Php),
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
        "stream_context_get_default",
        builtin_stream_context_get_default,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_context_get_options",
        builtin_stream_context_get_options,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_context_set_default",
        builtin_stream_context_set_default,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_context_set_option",
        builtin_stream_context_set_option,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_context_set_options",
        builtin_stream_context_set_options,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_copy_to_stream",
        builtin_stream_copy_to_stream,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_filter_append",
        builtin_stream_filter_append,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_filter_prepend",
        builtin_stream_filter_prepend,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_filter_register",
        builtin_stream_filter_register,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_filter_remove",
        builtin_stream_filter_remove,
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
        "stream_set_timeout",
        builtin_stream_set_timeout,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_socket_server",
        builtin_stream_socket_server,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_resolve_include_path",
        builtin_stream_resolve_include_path,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "stream_wrapper_register",
        builtin_stream_wrapper_register,
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
    let written = write_stream_bytes(context, &stream, &rendered, "fprintf")?;
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
    let written = write_stream_bytes(context, &stream, &rendered, "vfprintf")?;
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
    let php_input = context.php_input().to_vec();
    let open_result = {
        let Some(resources) = context.resources() else {
            return Ok(Value::Bool(false));
        };
        StreamWrapperRegistry::new().open(resources, &uri, &mode, &cwd, &filesystem, &php_input)
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
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fclose", &args, 1)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    if !resource.is_user_closable() {
        context.php_warning(
            "E_PHP_RUNTIME_RESOURCE_NOT_USER_CLOSABLE",
            "fclose(): cannot close the provided stream, as it must not be manually closed",
            span,
        );
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(resource.close()))
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
    context: &mut BuiltinContext<'_>,
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
    Ok(write_stream_bytes(context, &resource, &bytes, "fwrite")
        .map_or(Value::Bool(false), |written| Value::Int(written as i64)))
}

fn write_stream_bytes(
    context: &mut BuiltinContext<'_>,
    resource: &ResourceRef,
    bytes: &[u8],
    function_name: &str,
) -> Result<usize, BuiltinError> {
    let written = resource
        .write_bytes(bytes)
        .map_err(|error| value_error(function_name, &error.to_string()))?;
    if resource.metadata().uri == "php://stdout" {
        context.output().write_bytes(&bytes[..written]);
    } else if resource.metadata().uri == "php://stderr" {
        let _ = std::io::stderr().lock().write_all(&bytes[..written]);
    }
    Ok(written)
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
    let offset = int_arg("fseek", &args[1])?;
    let whence = match args
        .get(2)
        .map(|value| int_arg("fseek", value))
        .transpose()?
    {
        None | Some(0) => Some(StreamSeekWhence::Set),
        Some(1) => Some(StreamSeekWhence::Current),
        Some(2) => Some(StreamSeekWhence::End),
        Some(_) => None,
    };
    let Some(whence) = whence else {
        return Ok(Value::Int(-1));
    };
    Ok(if resource.seek_from(offset, whence).is_ok() {
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
            if !resource.flags().seekable {
                return Value::Bool(false);
            }
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

pub(in crate::builtins::modules) fn builtin_dir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("dir", "one or two argument(s)"));
    }
    builtin_opendir(context, vec![args[0].clone()], span)
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
    let _ = resources.close(resource.id());
    Ok(Value::Null)
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

pub(in crate::builtins::modules) fn builtin_stream_wrapper_register(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error(
            "stream_wrapper_register",
            "two or three argument(s)",
        ));
    }
    Ok(Value::Bool(false))
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

pub(in crate::builtins::modules) fn builtin_stream_filter_append(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    stream_filter_attach(context, args, span, false, "stream_filter_append")
}

pub(in crate::builtins::modules) fn builtin_stream_filter_prepend(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    stream_filter_attach(context, args, span, true, "stream_filter_prepend")
}

fn stream_filter_attach(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
    prepend: bool,
    function_name: &str,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error(function_name, "two to four argument(s)"));
    }
    let Some(stream) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let filter_name = string_arg(function_name, &args[1])?.to_string_lossy();
    let mode = args
        .get(2)
        .map(|value| int_arg(function_name, value))
        .transpose()?
        .unwrap_or(0);
    let Some(mode) = StreamFilterMode::from_php(mode) else {
        context.php_warning(
            "E_PHP_RUNTIME_STREAM_FILTER_MODE",
            format!("{function_name}(): invalid stream filter mode"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    let attach_result = {
        let Some(resources) = context.resources() else {
            return Ok(Value::Bool(false));
        };
        resources.register_stream_filter(&stream, filter_name.clone(), mode, prepend)
    };
    match attach_result {
        Ok(Some(filter)) => Ok(Value::Resource(filter)),
        Ok(None) => {
            context.php_warning(
                "E_PHP_RUNTIME_STREAM_FILTER_UNKNOWN",
                format!("{function_name}(): Unable to create or locate filter `{filter_name}`"),
                span,
            );
            Ok(Value::Bool(false))
        }
        Err(error) => {
            context.php_warning(
                error.diagnostic_id(),
                format!("{function_name}(): {}", error.message()),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_stream_filter_register(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_filter_register", &args, 2)?;
    let filter_name = string_arg("stream_filter_register", &args[0])?.to_string_lossy();
    let _class_name = string_arg("stream_filter_register", &args[1])?;
    context.php_warning(
        "E_PHP_RUNTIME_STREAM_FILTER_REGISTER_UNSUPPORTED",
        format!("stream_filter_register(): user filter `{filter_name}` is not supported"),
        span,
    );
    Ok(Value::Bool(false))
}

pub(in crate::builtins::modules) fn builtin_stream_filter_remove(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_filter_remove", &args, 1)?;
    let Some(filter) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(filter.remove_stream_filter_resource()))
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

pub(in crate::builtins::modules) fn builtin_stream_context_get_default(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "stream_context_get_default",
            "zero or one argument(s)",
        ));
    }
    if let Some(Value::Array(options)) = args.first().map(deref_value) {
        context
            .stream_context_state()
            .set_default_options(options.clone());
    } else if !args.is_empty() {
        return Ok(Value::Bool(false));
    }
    let options = context.stream_context_state().default_options();
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

pub(in crate::builtins::modules) fn builtin_stream_context_set_default(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_context_set_default", &args, 1)?;
    let Value::Array(options) = deref_value(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    context
        .stream_context_state()
        .set_default_options(options.clone());
    let Some(resources) = context.resources() else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Resource(
        resources.register_stream_context(options.clone()),
    ))
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
        return Ok(Value::Bool(set_context_options(&resource, &options)));
    }
    let wrapper = string_arg("stream_context_set_option", &args[1])?.to_string_lossy();
    let option = string_arg("stream_context_set_option", &args[2])?.to_string_lossy();
    Ok(Value::Bool(
        resource
            .set_context_option(wrapper, option, args[3].clone())
            .is_ok(),
    ))
}

pub(in crate::builtins::modules) fn builtin_stream_context_set_options(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stream_context_set_options", &args, 2)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let Value::Array(options) = deref_value(&args[1]) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(set_context_options(&resource, &options)))
}

fn set_context_options(resource: &ResourceRef, options: &crate::PhpArray) -> bool {
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
                return false;
            }
        }
    }
    true
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

pub(in crate::builtins::modules) fn builtin_stream_set_timeout(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error(
            "stream_set_timeout",
            "two or three argument(s)",
        ));
    }
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let seconds = int_arg("stream_set_timeout", &args[1])?;
    if seconds < 0 {
        return Ok(Value::Bool(false));
    }
    if let Some(microseconds) = args.get(2) {
        let microseconds = int_arg("stream_set_timeout", microseconds)?;
        if microseconds < 0 {
            return Ok(Value::Bool(false));
        }
    }
    let _ = resource;
    Ok(Value::Bool(false))
}

fn builtin_stream_socket_server(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=4).contains(&args.len()) {
        return Err(arity_error(
            "stream_socket_server",
            "between one and four arguments",
        ));
    }
    let uri = string_arg("stream_socket_server", &args[0])?;
    let uri_bytes = uri.as_bytes();
    let Some(mut path) = uri_bytes.strip_prefix(b"unix://").map(ToOwned::to_owned) else {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_STREAM_SOCKET_TRANSPORT",
            "stream_socket_server(): only unix:// transport is implemented",
        ));
    };
    let abstract_path = path.first() == Some(&0);
    #[cfg(target_os = "linux")]
    let maximum = if abstract_path { 108 } else { 107 };
    #[cfg(all(unix, not(target_os = "linux")))]
    let maximum = 103;
    #[cfg(not(unix))]
    let maximum = 0;
    if path.len() > maximum {
        context.php_notice(
            "E_PHP_RUNTIME_STREAM_SOCKET_PATH_TRUNCATED",
            format!(
                "stream_socket_server(): socket path exceeded the maximum allowed length of {maximum} bytes and was truncated"
            ),
            span.clone(),
        );
        path.truncate(maximum);
    }
    let path = String::from_utf8_lossy(&path).into_owned();
    #[cfg(unix)]
    let socket = context.socket_state().bind_unix_stream_server(&path);
    #[cfg(not(unix))]
    let socket = Err(libc::EAFNOSUPPORT);
    match socket {
        Ok(socket_id) => {
            let Some(resources) = context.resources() else {
                return Ok(Value::Bool(false));
            };
            Ok(Value::Resource(
                resources.register_socket_server(socket_id, &uri.to_string_lossy()),
            ))
        }
        Err(errno) => {
            assign_reference_arg(args.get(1), Value::Int(i64::from(errno)));
            let message = std::io::Error::from_raw_os_error(errno).to_string();
            assign_reference_arg(args.get(2), Value::string(message.clone()));
            context.php_warning(
                "E_PHP_RUNTIME_STREAM_SOCKET_BIND",
                format!("stream_socket_server(): unable to connect to unix://{path} ({message})"),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FilesystemCapabilities, OutputBuffer, PhpArray, ResourceTable, RuntimeSourceSpan};

    fn context<'a>(
        output: &'a mut OutputBuffer,
        resources: &'a mut ResourceTable,
    ) -> BuiltinContext<'a> {
        BuiltinContext::with_runtime(output, ".", FilesystemCapabilities::none(), Some(resources))
    }

    #[test]
    fn stream_context_set_options_mutates_nested_options() {
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();
        let mut context = context(&mut output, &mut resources);
        let context_resource =
            match builtin_stream_context_create(&mut context, vec![], RuntimeSourceSpan::default())
                .unwrap()
            {
                Value::Resource(resource) => resource,
                value => panic!("expected stream context resource, got {value:?}"),
            };

        let mut http_options = PhpArray::new();
        http_options.insert(
            string_array_key("protocol_version"),
            Value::Float(1.1.into()),
        );
        http_options.insert(string_array_key("user_agent"), Value::string("PHPT Agent"));
        let mut options = PhpArray::new();
        options.insert(string_array_key("http"), Value::Array(http_options));

        let result = builtin_stream_context_set_options(
            &mut context,
            vec![
                Value::Resource(context_resource.clone()),
                Value::Array(options.clone()),
            ],
            RuntimeSourceSpan::default(),
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true));

        let stored = builtin_stream_context_get_options(
            &mut context,
            vec![Value::Resource(context_resource)],
            RuntimeSourceSpan::default(),
        )
        .unwrap();
        assert_eq!(stored, Value::Array(options));
    }

    #[test]
    fn ftell_returns_false_for_non_seekable_standard_streams() {
        let mut output = OutputBuffer::new();
        let mut resources = ResourceTable::new();
        let stdin = resources.register_stdin(Vec::new());
        let mut context = context(&mut output, &mut resources);

        assert_eq!(
            builtin_ftell(
                &mut context,
                vec![Value::Resource(stdin)],
                RuntimeSourceSpan::default(),
            )
            .expect("ftell standard input"),
            Value::Bool(false)
        );
    }
}
