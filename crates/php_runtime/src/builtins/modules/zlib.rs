//! Bounded zlib compression helpers and gzip file resources.

use super::core::{
    argument_type_error, arity_error, int_arg, resolve_runtime_path, resource_arg, string_arg,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::resource::{ResourceRef, StreamFlags, StreamSeekWhence, decode_gzip_bytes};
use crate::{ClassEntry, ClassFlags, ObjectRef, PhpArray, Value, normalize_class_name};
use flate2::Compression;
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use flate2::write::{DeflateEncoder, GzEncoder, ZlibEncoder};
use std::io::{Cursor, Read, Write};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "deflate_add",
        builtin_deflate_add,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "deflate_init",
        builtin_deflate_init,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("gzclose", builtin_gzclose, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzdeflate", builtin_gzdeflate, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzcompress", builtin_gzcompress, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzdecode", builtin_gzdecode, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzencode", builtin_gzencode, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzeof", builtin_gzeof, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzfile", builtin_gzfile, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzgetc", builtin_gzgetc, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzgets", builtin_gzgets, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzopen", builtin_gzopen, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzpassthru", builtin_gzpassthru, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzputs", builtin_gzwrite, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzread", builtin_gzread, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzrewind", builtin_gzrewind, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzseek", builtin_gzseek, BuiltinCompatibility::Php),
    BuiltinEntry::new("gztell", builtin_gztell, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzwrite", builtin_gzwrite, BuiltinCompatibility::Php),
    BuiltinEntry::new("gzinflate", builtin_gzinflate, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gzuncompress",
        builtin_gzuncompress,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "inflate_add",
        builtin_inflate_add,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "inflate_get_read_len",
        builtin_inflate_get_read_len,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "inflate_get_status",
        builtin_inflate_get_status,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "inflate_init",
        builtin_inflate_init,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "zlib_decode",
        builtin_zlib_decode,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "zlib_get_coding_type",
        builtin_zlib_get_coding_type,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("readgzfile", builtin_readgzfile, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "zlib_encode",
        builtin_zlib_encode,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) const ZLIB_ENCODING_RAW: i64 = -15;
pub(in crate::builtins::modules) const ZLIB_ENCODING_GZIP: i64 = 31;
pub(in crate::builtins::modules) const ZLIB_ENCODING_DEFLATE: i64 = 15;
pub(in crate::builtins::modules) const ZLIB_SYNC_FLUSH: i64 = 2;
pub(in crate::builtins::modules) const ZLIB_FINISH: i64 = 4;
pub(in crate::builtins::modules) const ZLIB_OK: i64 = 0;
pub(in crate::builtins::modules) const ZLIB_STREAM_END: i64 = 1;
pub(in crate::builtins::modules) const ZLIB_DATA_ERROR: i64 = -3;

const DEFLATE_CONTEXT_CLASS: &str = "DeflateContext";
const INFLATE_CONTEXT_CLASS: &str = "InflateContext";
const ZLIB_CONTEXT_MODE_PROPERTY: &str = "__zlib_context_mode";
const ZLIB_CONTEXT_BUFFER_PROPERTY: &str = "__zlib_context_buffer";
const ZLIB_CONTEXT_FINISHED_PROPERTY: &str = "__zlib_context_finished";
const ZLIB_INFLATE_STATUS_PROPERTY: &str = "__zlib_inflate_status";
const ZLIB_INFLATE_READ_LEN_PROPERTY: &str = "__zlib_inflate_read_len";
const ZLIB_INFLATE_OUTPUT_LEN_PROPERTY: &str = "__zlib_inflate_output_len";
const ZLIB_PREFIX_DETECT_MAX_BYTES: usize = 4096;

fn builtin_deflate_init(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("deflate_init", "one or two argument(s)"));
    }
    let encoding = int_arg("deflate_init", &args[0])?;
    if !zlib_encoding_is_supported(encoding) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Object(zlib_context_object(
        DEFLATE_CONTEXT_CLASS,
        encoding,
    )))
}

fn builtin_deflate_add(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("deflate_add", "two or three argument(s)"));
    }
    let context = zlib_context_arg("deflate_add", &args[0], DEFLATE_CONTEXT_CLASS)?;
    let data = string_arg("deflate_add", &args[1])?.as_bytes().to_vec();
    let flush = args
        .get(2)
        .map(|value| int_arg("deflate_add", value))
        .transpose()?
        .unwrap_or(ZLIB_SYNC_FLUSH);
    if zlib_context_finished(&context) && !data.is_empty() {
        zlib_context_reset(&context);
    }
    zlib_context_append(&context, data);
    if flush != ZLIB_FINISH {
        return Ok(Value::string(Vec::new()));
    }
    let input = zlib_context_buffer(&context);
    let mode = zlib_context_mode(&context).unwrap_or(ZLIB_ENCODING_RAW);
    let output = zlib_encode_bytes(&input, mode, Compression::default());
    context.set_property(ZLIB_CONTEXT_FINISHED_PROPERTY, Value::Bool(true));
    output
}

fn builtin_inflate_init(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("inflate_init", "one or two argument(s)"));
    }
    let encoding = int_arg("inflate_init", &args[0])?;
    if !zlib_encoding_is_supported(encoding) {
        return Ok(Value::Bool(false));
    }
    let object = zlib_context_object(INFLATE_CONTEXT_CLASS, encoding);
    object.set_property(ZLIB_INFLATE_STATUS_PROPERTY, Value::Int(ZLIB_OK));
    object.set_property(ZLIB_INFLATE_READ_LEN_PROPERTY, Value::Int(0));
    object.set_property(ZLIB_INFLATE_OUTPUT_LEN_PROPERTY, Value::Int(0));
    Ok(Value::Object(object))
}

fn builtin_inflate_add(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("inflate_add", "two or three argument(s)"));
    }
    let context = zlib_context_arg("inflate_add", &args[0], INFLATE_CONTEXT_CLASS)?;
    let data = string_arg("inflate_add", &args[1])?.as_bytes().to_vec();
    let flush = args
        .get(2)
        .map(|value| int_arg("inflate_add", value))
        .transpose()?
        .unwrap_or(ZLIB_SYNC_FLUSH);
    if (zlib_context_finished(&context) || zlib_inflate_status(&context) == ZLIB_STREAM_END)
        && !data.is_empty()
    {
        zlib_context_reset(&context);
    }
    zlib_inflate_add_read_len(&context, data.len());
    zlib_context_append(&context, data);
    let input = zlib_context_buffer(&context);
    let mode = zlib_context_mode(&context).unwrap_or(ZLIB_ENCODING_RAW);
    if flush != ZLIB_FINISH {
        if mode == ZLIB_ENCODING_RAW {
            context.set_property(ZLIB_INFLATE_STATUS_PROPERTY, Value::Int(ZLIB_OK));
            return Ok(Value::string(Vec::new()));
        }
        if mode != ZLIB_ENCODING_DEFLATE {
            context.set_property(ZLIB_INFLATE_STATUS_PROPERTY, Value::Int(ZLIB_OK));
            return Ok(Value::string(Vec::new()));
        }
        return match zlib_decode_prefix(&input, mode) {
            Some((decoded, read_len)) => {
                let previous_len = zlib_inflate_output_len(&context);
                let suffix = decoded
                    .get(previous_len..)
                    .map_or_else(Vec::new, ToOwned::to_owned);
                context.set_property(ZLIB_INFLATE_STATUS_PROPERTY, Value::Int(ZLIB_STREAM_END));
                context.set_property(ZLIB_INFLATE_READ_LEN_PROPERTY, Value::Int(read_len as i64));
                context.set_property(
                    ZLIB_INFLATE_OUTPUT_LEN_PROPERTY,
                    Value::Int(decoded.len() as i64),
                );
                Ok(Value::string(suffix))
            }
            None => {
                context.set_property(ZLIB_INFLATE_STATUS_PROPERTY, Value::Int(ZLIB_OK));
                Ok(Value::string(Vec::new()))
            }
        };
    }
    let output = zlib_decode_bytes(&input, mode, None);
    context.set_property(ZLIB_CONTEXT_FINISHED_PROPERTY, Value::Bool(true));
    match output {
        Ok(value) => {
            context.set_property(ZLIB_INFLATE_STATUS_PROPERTY, Value::Int(ZLIB_STREAM_END));
            context.set_property(
                ZLIB_INFLATE_READ_LEN_PROPERTY,
                Value::Int(input.len() as i64),
            );
            Ok(value)
        }
        Err(()) => {
            context.set_property(ZLIB_INFLATE_STATUS_PROPERTY, Value::Int(ZLIB_DATA_ERROR));
            Ok(Value::Bool(false))
        }
    }
}

fn builtin_inflate_get_status(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("inflate_get_status", args.len(), 1, 1)?;
    let context = zlib_context_arg("inflate_get_status", &args[0], INFLATE_CONTEXT_CLASS)?;
    Ok(context
        .get_property(ZLIB_INFLATE_STATUS_PROPERTY)
        .unwrap_or(Value::Int(ZLIB_OK)))
}

fn builtin_inflate_get_read_len(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("inflate_get_read_len", args.len(), 1, 1)?;
    let context = zlib_context_arg("inflate_get_read_len", &args[0], INFLATE_CONTEXT_CLASS)?;
    Ok(context
        .get_property(ZLIB_INFLATE_READ_LEN_PROPERTY)
        .unwrap_or(Value::Int(0)))
}

fn builtin_gzopen(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("gzopen", "two or three argument(s)"));
    }
    let path_arg = string_arg("gzopen", &args[0])?.to_string_lossy();
    let mode = string_arg("gzopen", &args[1])?.to_string_lossy();
    let path = resolve_runtime_path(context, &path_arg);
    if !context.filesystem_capabilities().allows_path(&path) {
        return Ok(Value::Bool(false));
    }
    let readable = mode.starts_with('r');
    let writable = matches!(mode.as_bytes().first().copied(), Some(b'w' | b'a'));
    if !readable && !writable {
        return Ok(Value::Bool(false));
    }
    let buffer = if readable || mode.starts_with('a') {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(_) if writable => Vec::new(),
            Err(_) => return Ok(Value::Bool(false)),
        };
        if bytes.is_empty() {
            Vec::new()
        } else {
            match decode_gzip_bytes(&bytes) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(Value::Bool(false)),
            }
        }
    } else {
        Vec::new()
    };
    let cursor = if mode.starts_with('a') {
        buffer.len()
    } else {
        0
    };
    let Some(resources) = context.resources() else {
        return Ok(Value::Bool(false));
    };
    let flags = StreamFlags::new(
        readable || mode.contains('+'),
        writable || mode.contains('+'),
        true,
    );
    Ok(Value::Resource(
        resources.register_gzip_file(path, mode, flags, buffer, cursor),
    ))
}

fn builtin_gzread(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("gzread", args.len(), 2, 2)?;
    let resource = open_stream_arg("gzread", &args[0])?;
    let length = int_arg("gzread", &args[1])?.max(0) as usize;
    Ok(resource
        .read_bytes(length)
        .map_or(Value::Bool(false), Value::string))
}

fn builtin_gzgetc(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("gzgetc", args.len(), 1, 1)?;
    let resource = open_stream_arg("gzgetc", &args[0])?;
    let bytes = resource.read_bytes(1).unwrap_or_default();
    if bytes.is_empty() {
        Ok(Value::Bool(false))
    } else {
        Ok(Value::string(bytes))
    }
}

fn builtin_gzgets(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gzgets", "one or two argument(s)"));
    }
    let resource = open_stream_arg("gzgets", &args[0])?;
    let line = if let Some(length) = args.get(1) {
        let length = int_arg("gzgets", length)?.max(0) as usize;
        read_line_limited(&resource, length.saturating_sub(1))
    } else {
        resource.read_line().unwrap_or_default()
    };
    if line.is_empty() {
        Ok(Value::Bool(false))
    } else {
        Ok(Value::string(line))
    }
}

fn builtin_gzwrite(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("gzwrite", "two or three argument(s)"));
    }
    let resource = open_stream_arg("gzwrite", &args[0])?;
    let mut bytes = string_arg("gzwrite", &args[1])?.as_bytes().to_vec();
    if let Some(length) = args.get(2) {
        bytes.truncate(int_arg("gzwrite", length)?.max(0) as usize);
    }
    Ok(resource
        .write_bytes(&bytes)
        .map_or(Value::Bool(false), |written| Value::Int(written as i64)))
}

fn builtin_gzpassthru(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("gzpassthru", args.len(), 1, 1)?;
    let resource = open_stream_arg("gzpassthru", &args[0])?;
    let bytes = resource.read_to_end().unwrap_or_default();
    context.output().write_bytes(&bytes);
    Ok(Value::Int(bytes.len() as i64))
}

fn builtin_gzrewind(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("gzrewind", args.len(), 1, 1)?;
    let resource = open_stream_arg("gzrewind", &args[0])?;
    Ok(Value::Bool(resource.rewind().is_ok()))
}

fn builtin_gzseek(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("gzseek", "two or three argument(s)"));
    }
    let resource = open_stream_arg("gzseek", &args[0])?;
    let offset = int_arg("gzseek", &args[1])?;
    let whence = match args
        .get(2)
        .map(|value| int_arg("gzseek", value))
        .transpose()?
    {
        Some(1) => StreamSeekWhence::Current,
        Some(2) => StreamSeekWhence::End,
        Some(0) | None => StreamSeekWhence::Set,
        Some(_) => return Ok(Value::Int(-1)),
    };
    Ok(if resource.seek_from(offset, whence).is_ok() {
        Value::Int(0)
    } else {
        Value::Int(-1)
    })
}

fn builtin_gztell(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("gztell", args.len(), 1, 1)?;
    let resource = open_stream_arg("gztell", &args[0])?;
    Ok(resource
        .tell()
        .map_or(Value::Bool(false), |offset| Value::Int(offset as i64)))
}

fn builtin_gzeof(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("gzeof", args.len(), 1, 1)?;
    let resource = open_stream_arg("gzeof", &args[0])?;
    Ok(Value::Bool(resource.eof().unwrap_or(true)))
}

fn builtin_gzclose(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("gzclose", args.len(), 1, 1)?;
    Ok(Value::Bool(open_stream_arg("gzclose", &args[0])?.close()))
}

fn builtin_gzfile(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gzfile", "one or two argument(s)"));
    }
    let path = string_arg("gzfile", &args[0])?.to_string_lossy();
    let Some(bytes) = decode_gzip_path(context, &path) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Array(PhpArray::from_packed(gzip_lines(&bytes))))
}

fn builtin_readgzfile(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("readgzfile", "one or two argument(s)"));
    }
    let path = string_arg("readgzfile", &args[0])?.to_string_lossy();
    let Some(bytes) = decode_gzip_path(context, &path) else {
        return Ok(Value::Bool(false));
    };
    context.output().write_bytes(&bytes);
    Ok(Value::Int(bytes.len() as i64))
}

fn builtin_gzencode(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("gzencode", "one to three argument(s)"));
    }
    let input = string_arg("gzencode", &args[0])?;
    let level = compression_level("gzencode", args.get(1))?;
    let mut encoder = GzEncoder::new(Vec::new(), level);
    if encoder.write_all(input.as_bytes()).is_err() {
        return Ok(Value::Bool(false));
    }
    Ok(encoder.finish().map_or(Value::Bool(false), Value::string))
}

fn builtin_gzcompress(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("gzcompress", "one to three argument(s)"));
    }
    let input = string_arg("gzcompress", &args[0])?;
    let level = compression_level("gzcompress", args.get(1))?;
    let mut encoder = ZlibEncoder::new(Vec::new(), level);
    if encoder.write_all(input.as_bytes()).is_err() {
        return Ok(Value::Bool(false));
    }
    Ok(encoder.finish().map_or(Value::Bool(false), Value::string))
}

fn builtin_gzdeflate(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("gzdeflate", "one to three argument(s)"));
    }
    let input = string_arg("gzdeflate", &args[0])?;
    let level = compression_level("gzdeflate", args.get(1))?;
    let mut encoder = DeflateEncoder::new(Vec::new(), level);
    if encoder.write_all(input.as_bytes()).is_err() {
        return Ok(Value::Bool(false));
    }
    Ok(encoder.finish().map_or(Value::Bool(false), Value::string))
}

fn builtin_gzdecode(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gzdecode", "one or two argument(s)"));
    }
    let input = string_arg("gzdecode", &args[0])?;
    decode_with(
        GzDecoder::new(input.as_bytes()),
        max_length("gzdecode", args.get(1))?,
    )
}

fn builtin_gzuncompress(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gzuncompress", "one or two argument(s)"));
    }
    let input = string_arg("gzuncompress", &args[0])?;
    decode_with(
        ZlibDecoder::new(input.as_bytes()),
        max_length("gzuncompress", args.get(1))?,
    )
}

fn builtin_gzinflate(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("gzinflate", "one or two argument(s)"));
    }
    let input = string_arg("gzinflate", &args[0])?;
    decode_with(
        DeflateDecoder::new(input.as_bytes()),
        max_length("gzinflate", args.get(1))?,
    )
}

fn builtin_zlib_decode(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("zlib_decode", "one or two argument(s)"));
    }
    let input = string_arg("zlib_decode", &args[0])?;
    let max_length = max_length("zlib_decode", args.get(1))?;
    let bytes = input.as_bytes();
    let gzip = decode_with(GzDecoder::new(bytes), max_length);
    if !matches!(gzip, Ok(Value::Bool(false))) {
        return gzip;
    }
    let zlib = decode_with(ZlibDecoder::new(bytes), max_length);
    if !matches!(zlib, Ok(Value::Bool(false))) {
        return zlib;
    }
    decode_with(DeflateDecoder::new(bytes), max_length)
}

fn builtin_zlib_encode(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("zlib_encode", "two or three argument(s)"));
    }
    let input = string_arg("zlib_encode", &args[0])?;
    let encoding = int_arg("zlib_encode", &args[1])?;
    let level = compression_level("zlib_encode", args.get(2))?;
    match encoding {
        ZLIB_ENCODING_RAW => {
            let mut encoder = DeflateEncoder::new(Vec::new(), level);
            if encoder.write_all(input.as_bytes()).is_err() {
                return Ok(Value::Bool(false));
            }
            Ok(encoder.finish().map_or(Value::Bool(false), Value::string))
        }
        ZLIB_ENCODING_GZIP => {
            let mut encoder = GzEncoder::new(Vec::new(), level);
            if encoder.write_all(input.as_bytes()).is_err() {
                return Ok(Value::Bool(false));
            }
            Ok(encoder.finish().map_or(Value::Bool(false), Value::string))
        }
        ZLIB_ENCODING_DEFLATE => {
            let mut encoder = ZlibEncoder::new(Vec::new(), level);
            if encoder.write_all(input.as_bytes()).is_err() {
                return Ok(Value::Bool(false));
            }
            Ok(encoder.finish().map_or(Value::Bool(false), Value::string))
        }
        _ => Ok(Value::Bool(false)),
    }
}

fn builtin_zlib_get_coding_type(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_zlib_arity("zlib_get_coding_type", args.len(), 0, 0)?;
    Ok(Value::Bool(false))
}

fn zlib_encoding_is_supported(encoding: i64) -> bool {
    matches!(
        encoding,
        ZLIB_ENCODING_RAW | ZLIB_ENCODING_GZIP | ZLIB_ENCODING_DEFLATE
    )
}

fn zlib_encode_bytes(input: &[u8], encoding: i64, level: Compression) -> BuiltinResult {
    match encoding {
        ZLIB_ENCODING_RAW => {
            let mut encoder = DeflateEncoder::new(Vec::new(), level);
            if encoder.write_all(input).is_err() {
                return Ok(Value::Bool(false));
            }
            Ok(encoder.finish().map_or(Value::Bool(false), Value::string))
        }
        ZLIB_ENCODING_GZIP => {
            let mut encoder = GzEncoder::new(Vec::new(), level);
            if encoder.write_all(input).is_err() {
                return Ok(Value::Bool(false));
            }
            Ok(encoder.finish().map_or(Value::Bool(false), Value::string))
        }
        ZLIB_ENCODING_DEFLATE => {
            let mut encoder = ZlibEncoder::new(Vec::new(), level);
            if encoder.write_all(input).is_err() {
                return Ok(Value::Bool(false));
            }
            Ok(encoder.finish().map_or(Value::Bool(false), Value::string))
        }
        _ => Ok(Value::Bool(false)),
    }
}

fn zlib_decode_bytes(input: &[u8], encoding: i64, max_length: Option<usize>) -> Result<Value, ()> {
    let result = match encoding {
        ZLIB_ENCODING_RAW => decode_with(DeflateDecoder::new(input), max_length),
        ZLIB_ENCODING_GZIP => decode_with(GzDecoder::new(input), max_length),
        ZLIB_ENCODING_DEFLATE => decode_with(ZlibDecoder::new(input), max_length),
        _ => return Err(()),
    };
    match result {
        Ok(Value::Bool(false)) | Err(_) => Err(()),
        Ok(value) => Ok(value),
    }
}

fn zlib_decode_prefix(input: &[u8], encoding: i64) -> Option<(Vec<u8>, usize)> {
    if encoding != ZLIB_ENCODING_DEFLATE {
        return None;
    }
    if input.len() > ZLIB_PREFIX_DETECT_MAX_BYTES {
        return None;
    }
    let mut probe = input.to_vec();
    probe.push(0);
    let mut output = Vec::new();
    let mut decoder = ZlibDecoder::new(Cursor::new(probe));
    if decoder.read_to_end(&mut output).is_ok() {
        let read_len = decoder.total_in() as usize;
        if read_len > 0 && read_len <= input.len() {
            return Some((output, read_len));
        }
        None
    } else {
        None
    }
}

fn zlib_context_object(class_name: &str, encoding: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&zlib_context_class(class_name), class_name);
    object.set_property(ZLIB_CONTEXT_MODE_PROPERTY, Value::Int(encoding));
    object.set_property(ZLIB_CONTEXT_BUFFER_PROPERTY, Value::string(Vec::new()));
    object.set_property(ZLIB_CONTEXT_FINISHED_PROPERTY, Value::Bool(false));
    object
}

fn zlib_context_class(class_name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(class_name).into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: ClassFlags {
            is_final: true,
            ..ClassFlags::default()
        },
    }
}

fn zlib_context_arg(
    name: &str,
    value: &Value,
    expected_class: &str,
) -> Result<ObjectRef, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(
            name,
            "#1 ($context)",
            expected_class,
            value,
        ));
    };
    if normalize_class_name(&object.class_name()) != normalize_class_name(expected_class) {
        return Err(argument_type_error(
            name,
            "#1 ($context)",
            expected_class,
            value,
        ));
    }
    Ok(object.clone())
}

fn zlib_context_mode(object: &ObjectRef) -> Option<i64> {
    match object.get_property(ZLIB_CONTEXT_MODE_PROPERTY) {
        Some(Value::Int(mode)) => Some(mode),
        _ => None,
    }
}

fn zlib_context_buffer(object: &ObjectRef) -> Vec<u8> {
    match object.get_property(ZLIB_CONTEXT_BUFFER_PROPERTY) {
        Some(Value::String(buffer)) => buffer.as_bytes().to_vec(),
        _ => Vec::new(),
    }
}

fn zlib_context_append(object: &ObjectRef, data: Vec<u8>) {
    let mut buffer = zlib_context_buffer(object);
    buffer.extend_from_slice(&data);
    object.set_property(ZLIB_CONTEXT_BUFFER_PROPERTY, Value::string(buffer));
}

fn zlib_context_reset(object: &ObjectRef) {
    object.set_property(ZLIB_CONTEXT_BUFFER_PROPERTY, Value::string(Vec::new()));
    object.set_property(ZLIB_CONTEXT_FINISHED_PROPERTY, Value::Bool(false));
    object.set_property(ZLIB_INFLATE_STATUS_PROPERTY, Value::Int(ZLIB_OK));
    object.set_property(ZLIB_INFLATE_READ_LEN_PROPERTY, Value::Int(0));
    object.set_property(ZLIB_INFLATE_OUTPUT_LEN_PROPERTY, Value::Int(0));
}

fn zlib_context_finished(object: &ObjectRef) -> bool {
    matches!(
        object.get_property(ZLIB_CONTEXT_FINISHED_PROPERTY),
        Some(Value::Bool(true))
    )
}

fn zlib_inflate_status(object: &ObjectRef) -> i64 {
    match object.get_property(ZLIB_INFLATE_STATUS_PROPERTY) {
        Some(Value::Int(status)) => status,
        _ => ZLIB_OK,
    }
}

fn zlib_inflate_output_len(object: &ObjectRef) -> usize {
    match object.get_property(ZLIB_INFLATE_OUTPUT_LEN_PROPERTY) {
        Some(Value::Int(value)) if value > 0 => value as usize,
        _ => 0,
    }
}

fn zlib_inflate_add_read_len(object: &ObjectRef, bytes_read: usize) {
    let current = match object.get_property(ZLIB_INFLATE_READ_LEN_PROPERTY) {
        Some(Value::Int(value)) => value,
        _ => 0,
    };
    object.set_property(
        ZLIB_INFLATE_READ_LEN_PROPERTY,
        Value::Int(current.saturating_add(bytes_read as i64)),
    );
}

fn compression_level(
    name: &str,
    value: Option<&Value>,
) -> Result<Compression, crate::builtins::BuiltinError> {
    let level = value
        .map(|value| int_arg(name, value))
        .transpose()?
        .unwrap_or(-1);
    Ok(if level < 0 {
        Compression::default()
    } else {
        Compression::new(level.clamp(0, 9) as u32)
    })
}

fn max_length(
    name: &str,
    value: Option<&Value>,
) -> Result<Option<usize>, crate::builtins::BuiltinError> {
    Ok(value
        .map(|value| int_arg(name, value))
        .transpose()?
        .filter(|length| *length > 0)
        .map(|length| length as usize))
}

fn decode_with(mut decoder: impl Read, max_length: Option<usize>) -> BuiltinResult {
    let mut output = Vec::new();
    Ok(
        if decoder.read_to_end(&mut output).is_ok()
            && max_length.is_none_or(|max_length| output.len() <= max_length)
        {
            Value::string(output)
        } else {
            Value::Bool(false)
        },
    )
}

fn decode_gzip_path(context: &mut BuiltinContext<'_>, path_arg: &str) -> Option<Vec<u8>> {
    let path = resolve_runtime_path(context, path_arg);
    if !context.filesystem_capabilities().allows_path(&path) {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    decode_gzip_bytes(&bytes).ok()
}

fn gzip_lines(bytes: &[u8]) -> Vec<Value> {
    bytes
        .split_inclusive(|byte| *byte == b'\n')
        .map(|line| Value::string(line.to_vec()))
        .collect()
}

fn expect_zlib_arity(
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

fn open_stream_arg(name: &str, value: &Value) -> Result<ResourceRef, BuiltinError> {
    match resource_arg(value) {
        Some(resource) if resource.is_open() => Ok(resource),
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("{name}(): Argument #1 ($stream) must be an open stream resource"),
        )),
    }
}

fn read_line_limited(resource: &ResourceRef, limit: usize) -> Vec<u8> {
    let mut output = Vec::new();
    for _ in 0..limit {
        let byte = resource.read_bytes(1).unwrap_or_default();
        let Some(value) = byte.first().copied() else {
            break;
        };
        output.push(value);
        if value == b'\n' {
            break;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

    #[test]
    fn deflate_context_buffers_until_finish() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_deflate_init(
            &mut context,
            vec![Value::Int(ZLIB_ENCODING_DEFLATE)],
            RuntimeSourceSpan::default(),
        )
        .expect("deflate_init succeeds");
        let Value::Object(handle) = handle else {
            panic!("expected DeflateContext");
        };

        let partial = builtin_deflate_add(
            &mut context,
            vec![Value::Object(handle.clone()), Value::string("hello ")],
            RuntimeSourceSpan::default(),
        )
        .expect("deflate_add succeeds");
        assert_eq!(partial, Value::string(Vec::new()));

        let finished = builtin_deflate_add(
            &mut context,
            vec![
                Value::Object(handle),
                Value::string("world"),
                Value::Int(ZLIB_FINISH),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("deflate_add finish succeeds");
        let Value::String(bytes) = finished else {
            panic!("expected compressed string");
        };
        assert_eq!(
            zlib_decode_bytes(bytes.as_bytes(), ZLIB_ENCODING_DEFLATE, None).unwrap(),
            Value::string("hello world")
        );
    }

    #[test]
    fn inflate_context_buffers_until_finish_and_tracks_read_len() {
        let compressed = zlib_encode_bytes(
            b"streamed payload",
            ZLIB_ENCODING_RAW,
            Compression::default(),
        )
        .expect("encode succeeds");
        let Value::String(compressed) = compressed else {
            panic!("expected compressed string");
        };
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let handle = builtin_inflate_init(
            &mut context,
            vec![Value::Int(ZLIB_ENCODING_RAW)],
            RuntimeSourceSpan::default(),
        )
        .expect("inflate_init succeeds");
        let Value::Object(handle) = handle else {
            panic!("expected InflateContext");
        };

        let partial = builtin_inflate_add(
            &mut context,
            vec![
                Value::Object(handle.clone()),
                Value::string(compressed.as_bytes()[..2].to_vec()),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("inflate_add succeeds");
        assert_eq!(partial, Value::string(Vec::new()));

        let finished = builtin_inflate_add(
            &mut context,
            vec![
                Value::Object(handle.clone()),
                Value::string(compressed.as_bytes()[2..].to_vec()),
                Value::Int(ZLIB_FINISH),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("inflate_add finish succeeds");
        assert_eq!(finished, Value::string("streamed payload"));
        assert_eq!(
            builtin_inflate_get_status(
                &mut context,
                vec![Value::Object(handle.clone())],
                RuntimeSourceSpan::default(),
            )
            .expect("status succeeds"),
            Value::Int(ZLIB_STREAM_END)
        );
        assert_eq!(
            builtin_inflate_get_read_len(
                &mut context,
                vec![Value::Object(handle)],
                RuntimeSourceSpan::default(),
            )
            .expect("read len succeeds"),
            Value::Int(compressed.as_bytes().len() as i64)
        );
    }
}
