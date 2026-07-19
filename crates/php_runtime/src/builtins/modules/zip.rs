//! Legacy zip resource helpers over the Rust zip reader.

use super::core::{
    argument_value_error, arity_error, int_arg, resolve_runtime_path, resource_arg, string_arg,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ResourceRef, StreamFlags, StreamMetadata, Value};
use std::fs;
use std::io::Read;
use std::path::Path;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("zip_close", builtin_zip_close, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "zip_entry_close",
        builtin_zip_entry_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "zip_entry_compressedsize",
        builtin_zip_entry_compressedsize,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "zip_entry_compressionmethod",
        builtin_zip_entry_compressionmethod,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "zip_entry_filesize",
        builtin_zip_entry_filesize,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "zip_entry_name",
        builtin_zip_entry_name,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "zip_entry_open",
        builtin_zip_entry_open,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "zip_entry_read",
        builtin_zip_entry_read,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("zip_open", builtin_zip_open, BuiltinCompatibility::Php),
    BuiltinEntry::new("zip_read", builtin_zip_read, BuiltinCompatibility::Php),
];

fn builtin_zip_open(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(context, "zip_open", Some("ZipArchive::open"), span);
    expect_zip_arity("zip_open", args.len(), 1, 1)?;
    let path_arg = string_arg("zip_open", &args[0])?.to_string_lossy();
    if path_arg.is_empty() {
        return Err(argument_value_error(
            "zip_open",
            "#1 ($filename)",
            "must not be empty",
        ));
    }
    let path = resolve_runtime_path(context, &path_arg);
    if !context.filesystem_capabilities().allows_path(&path) || zip_open_archive(&path).is_err() {
        return Ok(Value::Bool(false));
    }
    let Some(resources) = context.resources() else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Resource(resources.register_stream(
        StreamFlags::new(true, false, true),
        StreamMetadata::new("zip", "Zip Directory", "r", path.to_string_lossy()),
    )))
}

fn builtin_zip_close(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(context, "zip_close", Some("ZipArchive::close"), span);
    expect_zip_arity("zip_close", args.len(), 1, 1)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Err(zip_directory_resource_type_error("zip_close"));
    };
    if !is_open_zip_archive_resource(&resource) {
        return Err(zip_directory_resource_type_error("zip_close"));
    }
    resource.close();
    Ok(Value::Null)
}

fn builtin_zip_read(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(context, "zip_read", Some("ZipArchive::statIndex"), span);
    expect_zip_arity("zip_read", args.len(), 1, 1)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let Some(path) = zip_archive_path(&resource) else {
        return Ok(Value::Bool(false));
    };
    let index = resource.tell().unwrap_or(0);
    let mut archive = match zip_open_archive(Path::new(&path)) {
        Ok(archive) => archive,
        Err(_) => return Ok(Value::Bool(false)),
    };
    if index >= archive.len() {
        return Ok(Value::Bool(false));
    }
    let entry = match zip_entry_snapshot(&mut archive, index) {
        Ok(entry) => entry,
        Err(_) => return Ok(Value::Bool(false)),
    };
    let _ = resource.seek(index + 1);
    let Some(resources) = context.resources() else {
        return Ok(Value::Bool(false));
    };
    let entry_resource = resources.register_stream(
        StreamFlags::new(true, true, true),
        StreamMetadata::new(
            "zip",
            format!("Zip Entry:{}", entry.name),
            index.to_string(),
            path,
        ),
    );
    if entry_resource.write_bytes(&entry.contents).is_err() {
        return Ok(Value::Bool(false));
    }
    let _ = entry_resource.rewind();
    Ok(Value::Resource(entry_resource))
}

fn builtin_zip_entry_open(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(context, "zip_entry_open", None, span);
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("zip_entry_open", "two or three argument(s)"));
    }
    let archive = resource_arg(&args[0]).is_some_and(|resource| is_zip_archive_resource(&resource));
    let entry = resource_arg(&args[1]).is_some_and(|resource| zip_entry_meta(&resource).is_some());
    Ok(Value::Bool(archive && entry))
}

fn builtin_zip_entry_close(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(context, "zip_entry_close", None, span);
    expect_zip_arity("zip_entry_close", args.len(), 1, 1)?;
    let Some(resource) = resource_arg(&args[0]) else {
        return Err(zip_entry_resource_type_error("zip_entry_close"));
    };
    if !resource.is_open() || zip_entry_meta(&resource).is_none() {
        return Err(zip_entry_resource_type_error("zip_entry_close"));
    }
    Ok(Value::Bool(resource.close()))
}

fn builtin_zip_entry_read(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(
        context,
        "zip_entry_read",
        Some("ZipArchive::getFromIndex"),
        span,
    );
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("zip_entry_read", "one or two argument(s)"));
    }
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    if zip_entry_meta(&resource).is_none() {
        return Ok(Value::Bool(false));
    }
    let length = args
        .get(1)
        .map(|value| int_arg("zip_entry_read", value))
        .transpose()?
        .unwrap_or(1024)
        .max(0) as usize;
    Ok(resource
        .read_bytes(length)
        .map_or(Value::Bool(false), Value::string))
}

fn builtin_zip_entry_name(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(
        context,
        "zip_entry_name",
        Some("ZipArchive::statIndex"),
        span,
    );
    expect_zip_arity("zip_entry_name", args.len(), 1, 1)?;
    Ok(zip_entry_meta_value(&args[0], |entry| {
        Value::string(entry.name.into_bytes())
    }))
}

fn builtin_zip_entry_filesize(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(
        context,
        "zip_entry_filesize",
        Some("ZipArchive::statIndex"),
        span,
    );
    expect_zip_arity("zip_entry_filesize", args.len(), 1, 1)?;
    Ok(zip_entry_snapshot_value(&args[0], |entry| {
        Value::Int(entry.size as i64)
    }))
}

fn builtin_zip_entry_compressedsize(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(
        context,
        "zip_entry_compressedsize",
        Some("ZipArchive::statIndex"),
        span,
    );
    expect_zip_arity("zip_entry_compressedsize", args.len(), 1, 1)?;
    Ok(zip_entry_snapshot_value(&args[0], |entry| {
        Value::Int(entry.compressed_size as i64)
    }))
}

fn builtin_zip_entry_compressionmethod(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    emit_zip_deprecation(
        context,
        "zip_entry_compressionmethod",
        Some("ZipArchive::statIndex"),
        span,
    );
    expect_zip_arity("zip_entry_compressionmethod", args.len(), 1, 1)?;
    Ok(zip_entry_snapshot_value(&args[0], |entry| {
        Value::string(entry.compression_method)
    }))
}

fn zip_entry_meta_value(value: &Value, f: impl FnOnce(ZipEntryMeta) -> Value) -> Value {
    let Some(resource) = resource_arg(value) else {
        return Value::Bool(false);
    };
    zip_entry_meta(&resource).map_or(Value::Bool(false), f)
}

fn zip_entry_snapshot_value(value: &Value, f: impl FnOnce(ZipEntrySnapshot) -> Value) -> Value {
    let Some(resource) = resource_arg(value) else {
        return Value::Bool(false);
    };
    let Some(meta) = zip_entry_meta(&resource) else {
        return Value::Bool(false);
    };
    let mut archive = match zip_open_archive(Path::new(&meta.path)) {
        Ok(archive) => archive,
        Err(_) => return Value::Bool(false),
    };
    zip_entry_snapshot(&mut archive, meta.index).map_or(Value::Bool(false), f)
}

fn is_zip_archive_resource(resource: &ResourceRef) -> bool {
    zip_archive_path(resource).is_some()
}

fn is_open_zip_archive_resource(resource: &ResourceRef) -> bool {
    resource.is_open() && is_zip_archive_resource(resource)
}

fn zip_directory_resource_type_error(function: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_TYPE",
        format!("{function}(): supplied resource is not a valid Zip Directory resource"),
    )
}

fn zip_entry_resource_type_error(function: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_TYPE",
        format!("{function}(): supplied resource is not a valid Zip Entry resource"),
    )
}

fn emit_zip_deprecation(
    context: &mut BuiltinContext<'_>,
    function: &str,
    replacement: Option<&str>,
    span: RuntimeSourceSpan,
) {
    let message = match replacement {
        Some(replacement) => {
            format!("Function {function}() is deprecated since 8.0, use {replacement}() instead")
        }
        None => format!("Function {function}() is deprecated since 8.0"),
    };
    context.php_deprecation("E_PHP_RUNTIME_ZIP_FUNCTION_DEPRECATED", message, span);
}

fn zip_archive_path(resource: &ResourceRef) -> Option<String> {
    let metadata = resource.metadata();
    if metadata.wrapper_type == "zip" && metadata.stream_type == "Zip Directory" {
        Some(metadata.uri)
    } else {
        None
    }
}

fn zip_entry_meta(resource: &ResourceRef) -> Option<ZipEntryMeta> {
    let metadata = resource.metadata();
    if metadata.wrapper_type != "zip" {
        return None;
    }
    let name = metadata
        .stream_type
        .strip_prefix("Zip Entry:")
        .map(str::to_owned)?;
    let index = metadata.mode.parse::<usize>().ok()?;
    Some(ZipEntryMeta {
        path: metadata.uri,
        index,
        name,
    })
}

fn zip_open_archive(path: &Path) -> Result<zip::ZipArchive<fs::File>, zip::result::ZipError> {
    let file = fs::File::open(path).map_err(zip::result::ZipError::Io)?;
    zip::ZipArchive::new(file)
}

fn zip_entry_snapshot(
    archive: &mut zip::ZipArchive<fs::File>,
    index: usize,
) -> Result<ZipEntrySnapshot, zip::result::ZipError> {
    let mut file = archive.by_index(index)?;
    let name = file.name().to_owned();
    let size = file.size();
    let compressed_size = file.compressed_size();
    let compression_method = match file.compression() {
        zip::CompressionMethod::Stored => "stored",
        zip::CompressionMethod::Deflated => "deflated",
        _ => "unknown",
    }
    .as_bytes()
    .to_vec();
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)
        .map_err(zip::result::ZipError::Io)?;
    Ok(ZipEntrySnapshot {
        name,
        size,
        compressed_size,
        compression_method,
        contents,
    })
}

fn expect_zip_arity(
    name: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), crate::builtins::BuiltinError> {
    if actual < min || actual > max {
        return Err(arity_error(name, "the expected number of argument(s)"));
    }
    Ok(())
}

struct ZipEntryMeta {
    path: String,
    index: usize,
    name: String,
}

struct ZipEntrySnapshot {
    name: String,
    size: u64,
    compressed_size: u64,
    compression_method: Vec<u8>,
    contents: Vec<u8>,
}
