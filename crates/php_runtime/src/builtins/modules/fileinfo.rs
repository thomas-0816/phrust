//! Fileinfo MVP for common media MIME checks.

// Bounded libmagic FFI: every unsafe block is a direct host-library call
// wrapped in a checked result.
#![allow(unsafe_code)]

use super::core::{
    argument_type_error, argument_value_error, arity_error, int_arg, read_file_value,
    resolve_runtime_path, resource_arg, string_arg,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::object::{ClassEntry, ClassFlags, ObjectRef, normalize_class_name};
use crate::{PhpArray, ResourceKind, Value};
use libc::{c_char, c_int, c_void, size_t};
use std::ffi::{CStr, CString};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "finfo_buffer",
        builtin_finfo_buffer,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "finfo_close",
        builtin_finfo_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("finfo_file", builtin_finfo_file, BuiltinCompatibility::Php),
    BuiltinEntry::new("finfo_open", builtin_finfo_open, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "finfo_set_flags",
        builtin_finfo_set_flags,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mime_content_type",
        builtin_mime_content_type,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "image_type_to_mime_type",
        builtin_image_type_to_mime_type,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "image_type_to_extension",
        builtin_image_type_to_extension,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) const FILEINFO_NONE: i64 = 0;
pub(in crate::builtins::modules) const FILEINFO_MIME_TYPE: i64 = 16;
pub(in crate::builtins::modules) const FILEINFO_MIME_ENCODING: i64 = 1024;
pub(in crate::builtins::modules) const FILEINFO_MIME: i64 = 1040;

type MagicHandle = *mut c_void;

unsafe extern "C" {
    fn magic_open(flags: c_int) -> MagicHandle;
    fn magic_close(handle: MagicHandle);
    fn magic_load(handle: MagicHandle, filename: *const c_char) -> c_int;
    fn magic_buffer(handle: MagicHandle, buffer: *const c_void, length: size_t) -> *const c_char;
    fn magic_error(handle: MagicHandle) -> *const c_char;
}

struct MagicDetector {
    handle: MagicHandle,
}

impl MagicDetector {
    fn open(flags: i64, magic_file: Option<&str>) -> Result<Self, String> {
        let flags = c_int::try_from(flags).map_err(|_| format!("Invalid mode '{flags}'."))?;
        // SAFETY: `magic_open` does not retain Rust references; the returned handle is owned
        // by `MagicDetector` and closed in `Drop`.
        let handle = unsafe { magic_open(flags) };
        if handle.is_null() {
            return Err(format!("Invalid mode '{flags}'."));
        }
        let detector = Self { handle };
        let magic_file = magic_file.filter(|path| !path.is_empty());
        let magic_file_c = magic_file
            .map(|path| CString::new(path).map_err(|_| "magic file path contains null bytes"))
            .transpose()
            .map_err(str::to_owned)?;
        let magic_file_ptr = magic_file_c
            .as_ref()
            .map_or(std::ptr::null(), |path| path.as_ptr());
        // SAFETY: `handle` is valid and `magic_file_ptr` is either null or a live C string
        // for the duration of the call. libmagic copies/loads the database during this call.
        let loaded = unsafe { magic_load(detector.handle, magic_file_ptr) };
        if loaded == -1 {
            let detail = detector
                .last_error()
                .unwrap_or_else(|| "unknown libmagic error".to_owned());
            return Err(format!("Failed to load magic database: {detail}"));
        }
        Ok(detector)
    }

    fn buffer(&self, bytes: &[u8]) -> Result<String, String> {
        // SAFETY: `handle` is valid and the byte slice pointer/length are valid for the call.
        let result = unsafe {
            magic_buffer(
                self.handle,
                bytes.as_ptr().cast::<c_void>(),
                bytes.len() as size_t,
            )
        };
        if result.is_null() {
            return Err(self
                .last_error()
                .unwrap_or_else(|| "unknown libmagic error".to_owned()));
        }
        // SAFETY: libmagic returns a null-terminated string valid until the next call on handle.
        Ok(unsafe { CStr::from_ptr(result) }
            .to_string_lossy()
            .into_owned())
    }

    fn last_error(&self) -> Option<String> {
        // SAFETY: `handle` is valid and `magic_error` returns a nullable C string.
        let error = unsafe { magic_error(self.handle) };
        (!error.is_null()).then(|| {
            // SAFETY: non-null libmagic error pointers are null-terminated strings.
            unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned()
        })
    }
}

impl Drop for MagicDetector {
    fn drop(&mut self) {
        // SAFETY: `handle` is owned by this wrapper and closed exactly once.
        unsafe { magic_close(self.handle) };
    }
}

pub(in crate::builtins::modules) const IMAGETYPE_GIF: i64 = 1;
pub(in crate::builtins::modules) const IMAGETYPE_JPEG: i64 = 2;
pub(in crate::builtins::modules) const IMAGETYPE_PNG: i64 = 3;
pub(in crate::builtins::modules) const IMAGETYPE_WEBP: i64 = 18;
pub(in crate::builtins::modules) const IMAGETYPE_AVIF: i64 = 19;
pub(in crate::builtins::modules) const IMAGETYPE_SVG: i64 = 21;

fn builtin_finfo_open(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error("finfo_open", "zero to two argument(s)"));
    }
    let flags = args
        .first()
        .map(|value| int_arg("finfo_open", value))
        .transpose()?
        .unwrap_or(FILEINFO_NONE);
    let magic_file = args
        .get(1)
        .map(|value| string_arg("finfo_open", value).map(|path| path.to_string_lossy()))
        .transpose()?;
    if let Err(message) = MagicDetector::open(flags, magic_file.as_deref()) {
        context.php_warning("E_PHP_RUNTIME_FILEINFO_MAGIC", message, _span);
        return Ok(Value::Bool(false));
    }
    Ok(Value::Object(fileinfo_object_with_options(
        flags, magic_file,
    )))
}

fn builtin_finfo_close(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("finfo_close", "one argument"));
    }
    Ok(Value::Bool(is_fileinfo_handle(&args[0])))
}

fn builtin_finfo_buffer(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error("finfo_buffer", "two to four argument(s)"));
    }
    let Some((resource_flags, magic_file)) = fileinfo_options(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let data = string_arg("finfo_buffer", &args[1])?;
    let flags = args
        .get(2)
        .map(|value| int_arg("finfo_buffer", value))
        .transpose()?
        .unwrap_or(resource_flags);
    Ok(Value::string(detect_buffer_mime(
        context,
        data.as_bytes(),
        None,
        flags,
        magic_file.as_deref(),
        span,
    )))
}

fn builtin_finfo_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error("finfo_file", "two to four argument(s)"));
    }
    let Some((resource_flags, magic_file)) = fileinfo_options(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    let path = filename_arg("finfo_file", "#2 ($filename)", &args[1], "string", false)?;
    let flags = args
        .get(2)
        .map(|value| int_arg("finfo_file", value))
        .transpose()?
        .unwrap_or(resource_flags);
    mime_for_file(
        context,
        "finfo_file",
        &path,
        flags,
        magic_file.as_deref(),
        span,
    )
}

fn builtin_finfo_set_flags(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("finfo_set_flags", "two arguments"));
    }
    if let Some(object) = fileinfo_object(&args[0]) {
        let flags = int_arg("finfo_set_flags", &args[1])?;
        object.set_property("__fileinfo_flags", Value::Int(flags));
        return Ok(Value::Bool(true));
    }
    let Some(resource) = resource_arg(&args[0]) else {
        return Ok(Value::Bool(false));
    };
    if resource.kind() != ResourceKind::FileInfo {
        return Ok(Value::Bool(false));
    }
    let flags = int_arg("finfo_set_flags", &args[1])?;
    Ok(Value::Bool(resource.set_fileinfo_flags(flags)))
}

fn builtin_mime_content_type(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("mime_content_type", "one argument"));
    }
    let path = filename_arg(
        "mime_content_type",
        "#1 ($filename)",
        &args[0],
        "resource|string",
        true,
    )?;
    mime_for_file(
        context,
        "mime_content_type",
        &path,
        FILEINFO_MIME_TYPE,
        None,
        span,
    )
}

fn builtin_image_type_to_mime_type(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("image_type_to_mime_type", "one argument"));
    }
    Ok(Value::string(image_type_to_mime_type(int_arg(
        "image_type_to_mime_type",
        &args[0],
    )?)))
}

fn builtin_image_type_to_extension(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error(
            "image_type_to_extension",
            "one or two argument(s)",
        ));
    }
    let image_type = int_arg("image_type_to_extension", &args[0])?;
    let include_dot = args
        .get(1)
        .map(crate::to_bool)
        .transpose()
        .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_BUILTIN_TYPE", message))?
        .unwrap_or(true);
    Ok(image_type_to_extension(image_type, include_dot)
        .map(Value::string)
        .unwrap_or(Value::Bool(false)))
}

fn is_fileinfo_handle(value: &Value) -> bool {
    fileinfo_object(value).is_some()
        || resource_arg(value).is_some_and(|resource| resource.kind() == ResourceKind::FileInfo)
}

fn fileinfo_options(value: &Value) -> Option<(i64, Option<String>)> {
    if let Some(object) = fileinfo_object(value) {
        let flags = match object.get_property("__fileinfo_flags") {
            Some(Value::Int(flags)) => flags,
            _ => FILEINFO_NONE,
        };
        let magic_file = match object.get_property("__fileinfo_magic_file") {
            Some(Value::String(path)) => Some(path.to_string_lossy()),
            _ => None,
        };
        return Some((flags, magic_file));
    }
    let resource = resource_arg(value)?;
    (resource.kind() == ResourceKind::FileInfo)
        .then(|| resource.fileinfo_options())
        .flatten()
}

fn fileinfo_object(value: &Value) -> Option<crate::object::ObjectRef> {
    let Value::Object(object) = super::core::deref_value(value) else {
        return None;
    };
    (normalize_class_name(&object.class_name()) == "finfo").then(|| object.clone())
}

fn fileinfo_object_with_options(flags: i64, magic_file: Option<String>) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&fileinfo_runtime_class(), "finfo");
    object.set_property("__fileinfo_flags", Value::Int(flags));
    object.set_property(
        "__fileinfo_magic_file",
        magic_file.map(Value::string).unwrap_or(Value::Null),
    );
    object
}

fn fileinfo_runtime_class() -> ClassEntry {
    ClassEntry {
        name: "finfo".to_owned(),
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

fn filename_arg(
    name: &str,
    argument: &str,
    value: &Value,
    expected: &str,
    null_byte_is_type_error: bool,
) -> Result<String, BuiltinError> {
    let Value::String(path) = super::core::deref_value(value) else {
        return Err(argument_type_error(name, argument, expected, value));
    };
    if path.as_bytes().is_empty() {
        return Err(argument_value_error(name, argument, "must not be empty"));
    }
    if path.as_bytes().contains(&0) {
        if null_byte_is_type_error {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_TYPE",
                format!("{name}(): Argument {argument} must not contain any null bytes"),
            ));
        }
        return Err(argument_value_error(
            name,
            argument,
            "must not contain any null bytes",
        ));
    }
    Ok(path.to_string_lossy())
}

fn mime_for_file(
    context: &mut BuiltinContext<'_>,
    name: &str,
    path: &str,
    flags: i64,
    magic_file: Option<&str>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !path.starts_with("php://") && !crate::phar::is_phar_uri(path) {
        let resolved = resolve_runtime_path(context, path);
        if context.filesystem_capabilities().allows_path(&resolved) && resolved.is_dir() {
            return Ok(Value::string("directory"));
        }
    }
    match read_file_value(context, name, path, span.clone())? {
        Value::String(bytes) => Ok(Value::string(detect_buffer_mime(
            context,
            bytes.as_bytes(),
            Some(path),
            flags,
            magic_file,
            span,
        ))),
        _ => Ok(Value::Bool(false)),
    }
}

fn detect_buffer_mime(
    context: &mut BuiltinContext<'_>,
    bytes: &[u8],
    path: Option<&str>,
    flags: i64,
    magic_file: Option<&str>,
    span: RuntimeSourceSpan,
) -> String {
    match MagicDetector::open(flags, magic_file).and_then(|detector| detector.buffer(bytes)) {
        Ok(value) => {
            let fallback = format_mime(mime_for_bytes(bytes, path), flags);
            if is_inconclusive_magic_result(&value) && !is_inconclusive_magic_result(&fallback) {
                fallback
            } else {
                value
            }
        }
        Err(message) => {
            context.php_warning("E_PHP_RUNTIME_FILEINFO_MAGIC", message, span);
            format_mime(mime_for_bytes(bytes, path), flags)
        }
    }
}

fn is_inconclusive_magic_result(value: &str) -> bool {
    value == "application/octet-stream" || value == "application/octet-stream; charset=binary"
}

pub(in crate::builtins::modules) fn mime_for_bytes(
    bytes: &[u8],
    path: Option<&str>,
) -> &'static str {
    let trimmed = bytes
        .iter()
        .copied()
        .skip_while(|byte| byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG\r\n\x1A\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.len() >= 12 && &bytes[4..12] == b"ftypavif" {
        "image/avif"
    } else if bytes.starts_with(b"%PDF-") {
        "application/pdf"
    } else if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
    {
        "application/zip"
    } else if is_svg_document(&trimmed) {
        "image/svg+xml"
    } else if trimmed.starts_with(b"{") || trimmed.starts_with(b"[") {
        "application/json"
    } else if trimmed.starts_with(b"<?xml") {
        "text/xml"
    } else if is_likely_text(bytes) {
        "text/plain"
    } else {
        path.and_then(mime_from_extension)
            .unwrap_or("application/octet-stream")
    }
}

fn is_svg_document(trimmed: &[u8]) -> bool {
    if starts_with_svg_tag(trimmed) {
        return true;
    }
    let Some(after_declaration) = trimmed.strip_prefix(b"<?xml") else {
        return false;
    };
    let Some(end) = after_declaration
        .windows(2)
        .position(|window| window == b"?>")
    else {
        return false;
    };
    starts_with_svg_tag(after_declaration[end + 2..].trim_ascii_start())
}

fn starts_with_svg_tag(bytes: &[u8]) -> bool {
    bytes.len() >= 4
        && bytes[0] == b'<'
        && bytes[1].eq_ignore_ascii_case(&b's')
        && bytes[2].eq_ignore_ascii_case(&b'v')
        && bytes[3].eq_ignore_ascii_case(&b'g')
        && bytes
            .get(4)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'>')
}

fn is_likely_text(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|byte| matches!(*byte, b'\t' | b'\n' | b'\r' | 0x20..=0x7e) || *byte >= 0x80)
}

fn mime_from_extension(path: &str) -> Option<&'static str> {
    match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        "pdf" => Some("application/pdf"),
        "txt" => Some("text/plain"),
        "json" => Some("application/json"),
        "zip" => Some("application/zip"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

fn format_mime(mime: &str, flags: i64) -> String {
    match flags {
        FILEINFO_MIME => format!("{mime}; charset=binary"),
        FILEINFO_MIME_ENCODING => "binary".to_owned(),
        _ => mime.to_owned(),
    }
}

pub(in crate::builtins::modules) fn image_type(bytes: &[u8]) -> Option<i64> {
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        Some(IMAGETYPE_JPEG)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1A\n") {
        Some(IMAGETYPE_PNG)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(IMAGETYPE_GIF)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(IMAGETYPE_WEBP)
    } else if bytes.len() >= 12 && &bytes[4..12] == b"ftypavif" {
        Some(IMAGETYPE_AVIF)
    } else {
        None
    }
}

fn image_type_to_mime_type(image_type: i64) -> &'static str {
    match image_type {
        1 => "image/gif",
        2 => "image/jpeg",
        3 => "image/png",
        4 | 13 => "application/x-shockwave-flash",
        5 => "image/psd",
        6 => "image/bmp",
        7 | 8 => "image/tiff",
        9 => "application/octet-stream",
        10 => "image/jp2",
        14 => "image/iff",
        15 => "image/vnd.wap.wbmp",
        16 => "image/xbm",
        17 => "image/vnd.microsoft.icon",
        18 => "image/webp",
        19 => "image/avif",
        20 => "image/heif",
        21 => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn image_type_to_extension(image_type: i64, include_dot: bool) -> Option<&'static str> {
    let extension = match image_type {
        1 => "gif",
        2 => "jpeg",
        3 => "png",
        4 | 13 => "swf",
        5 => "psd",
        6 => "bmp",
        7 | 8 => "tiff",
        9 => "jpc",
        10 => "jp2",
        11 => "jpx",
        12 => "jb2",
        14 => "iff",
        15 => "bmp",
        16 => "xbm",
        17 => "ico",
        18 => "webp",
        19 => "avif",
        20 => "heif",
        21 => "svg",
        _ => return None,
    };
    Some(if include_dot {
        match extension {
            "gif" => ".gif",
            "jpeg" => ".jpeg",
            "png" => ".png",
            "swf" => ".swf",
            "psd" => ".psd",
            "bmp" => ".bmp",
            "tiff" => ".tiff",
            "jpc" => ".jpc",
            "jp2" => ".jp2",
            "jpx" => ".jpx",
            "jb2" => ".jb2",
            "iff" => ".iff",
            "xbm" => ".xbm",
            "ico" => ".ico",
            "webp" => ".webp",
            "avif" => ".avif",
            "heif" => ".heif",
            "svg" => ".svg",
            _ => unreachable!("all extension values are covered"),
        }
    } else {
        extension
    })
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::builtins::modules) struct ImageInfo {
    pub width: i64,
    pub height: i64,
    pub image_type: i64,
    pub mime: &'static str,
    pub bits: Option<i64>,
    pub channels: Option<i64>,
}

pub(in crate::builtins::modules) fn image_size(bytes: &[u8]) -> Option<ImageInfo> {
    if bytes.starts_with(b"\x89PNG\r\n\x1A\n") && bytes.len() >= 24 {
        let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?) as i64;
        let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?) as i64;
        let bits = bytes.get(24).copied().map(i64::from);
        return Some(ImageInfo {
            width,
            height,
            image_type: IMAGETYPE_PNG,
            mime: "image/png",
            bits,
            channels: None,
        });
    }
    if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 10 {
        let width = u16::from_le_bytes(bytes[6..8].try_into().ok()?) as i64;
        let height = u16::from_le_bytes(bytes[8..10].try_into().ok()?) as i64;
        let bits = bytes.get(10).map(|packed| i64::from((packed & 0x07) + 1));
        return Some(ImageInfo {
            width,
            height,
            image_type: IMAGETYPE_GIF,
            mime: "image/gif",
            bits,
            channels: Some(3),
        });
    }
    if bytes.len() >= 30
        && &bytes[0..4] == b"RIFF"
        && &bytes[8..12] == b"WEBP"
        && &bytes[12..16] == b"VP8 "
    {
        let width = (u16::from_le_bytes(bytes[26..28].try_into().ok()?) & 0x3fff) as i64;
        let height = (u16::from_le_bytes(bytes[28..30].try_into().ok()?) & 0x3fff) as i64;
        return Some(ImageInfo {
            width,
            height,
            image_type: IMAGETYPE_WEBP,
            mime: "image/webp",
            bits: None,
            channels: None,
        });
    }
    if let Some((width, height)) = svg_size(bytes) {
        return Some(ImageInfo {
            width,
            height,
            image_type: IMAGETYPE_SVG,
            mime: "image/svg+xml",
            bits: None,
            channels: None,
        });
    }
    jpeg_info(bytes)
}

pub(in crate::builtins::modules) fn image_app_info(bytes: &[u8]) -> PhpArray {
    let mut info = PhpArray::new();
    if !bytes.starts_with(b"\xFF\xD8") {
        return info;
    }
    let mut offset = 2usize;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xFF {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        offset += 2;
        if marker == 0xD9 || marker == 0xDA || offset + 2 > bytes.len() {
            break;
        }
        let Some(len_bytes) = bytes.get(offset..offset + 2) else {
            break;
        };
        let Ok(raw_len): Result<[u8; 2], _> = len_bytes.try_into() else {
            break;
        };
        let len = u16::from_be_bytes(raw_len) as usize;
        if len < 2 || offset + len > bytes.len() {
            break;
        }
        if (0xE0..=0xEF).contains(&marker) {
            let app_index = marker - 0xE0;
            info.insert(
                crate::builtins::modules::core::string_array_key(&format!("APP{app_index}")),
                Value::string(bytes[offset + 2..offset + len].to_vec()),
            );
        }
        offset += len;
    }
    info
}

fn svg_size(bytes: &[u8]) -> Option<(i64, i64)> {
    let text = std::str::from_utf8(bytes)
        .ok()?
        .trim_start_matches('\u{feff}');
    let text = text.trim_start();
    if !text.starts_with("<svg") {
        return None;
    }
    let tag_end = text.find('>')?;
    let tag = &text[..tag_end];
    let width = svg_dimension_attribute(tag, "width")?;
    let height = svg_dimension_attribute(tag, "height")?;
    Some((width, height))
}

fn svg_dimension_attribute(tag: &str, name: &str) -> Option<i64> {
    let mut rest = tag;
    loop {
        let offset = rest.find(name)?;
        let candidate = &rest[offset + name.len()..];
        let before = rest[..offset].chars().next_back();
        if before.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
            rest = candidate;
            continue;
        }
        let candidate = candidate.trim_start();
        let Some(candidate) = candidate.strip_prefix('=') else {
            rest = candidate;
            continue;
        };
        let candidate = candidate.trim_start();
        let quote = candidate.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let value_start = quote.len_utf8();
        let value_end = candidate[value_start..].find(quote)? + value_start;
        let value = candidate[value_start..value_end].trim();
        return parse_svg_dimension(value);
    }
}

fn parse_svg_dimension(value: &str) -> Option<i64> {
    let numeric = value
        .strip_suffix("px")
        .unwrap_or(value)
        .trim()
        .parse::<f64>()
        .ok()?;
    if numeric.is_finite() && numeric > 0.0 {
        Some(numeric as i64)
    } else {
        None
    }
}

fn jpeg_info(bytes: &[u8]) -> Option<ImageInfo> {
    if !bytes.starts_with(b"\xFF\xD8") {
        return None;
    }
    let mut offset = 2;
    while offset + 9 < bytes.len() {
        if bytes[offset] != 0xFF {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        offset += 2;
        if marker == 0xD9 || marker == 0xDA {
            break;
        }
        if offset + 2 > bytes.len() {
            break;
        }
        let len = u16::from_be_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
        if (0xC0..=0xC3).contains(&marker) && offset + 7 < bytes.len() {
            let bits = i64::from(bytes[offset + 2]);
            let height = u16::from_be_bytes(bytes[offset + 3..offset + 5].try_into().ok()?) as i64;
            let width = u16::from_be_bytes(bytes[offset + 5..offset + 7].try_into().ok()?) as i64;
            let channels = bytes.get(offset + 7).copied().map(i64::from);
            return Some(ImageInfo {
                width,
                height,
                image_type: IMAGETYPE_JPEG,
                mime: "image/jpeg",
                bits: Some(bits),
                channels,
            });
        }
        offset = offset.saturating_add(len);
    }
    None
}

pub(in crate::builtins::modules) fn size_array(info: &ImageInfo) -> PhpArray {
    let mut array = PhpArray::new();
    array.insert(crate::ArrayKey::Int(0), Value::Int(info.width));
    array.insert(crate::ArrayKey::Int(1), Value::Int(info.height));
    array.insert(crate::ArrayKey::Int(2), Value::Int(info.image_type));
    array.insert(
        crate::ArrayKey::Int(3),
        Value::string(format!(
            "width=\"{}\" height=\"{}\"",
            info.width, info.height
        )),
    );
    if let Some(bits) = info.bits {
        array.insert(
            crate::builtins::modules::core::string_array_key("bits"),
            Value::Int(bits),
        );
    }
    if let Some(channels) = info.channels {
        array.insert(
            crate::builtins::modules::core::string_array_key("channels"),
            Value::Int(channels),
        );
    }
    array.insert(
        crate::builtins::modules::core::string_array_key("mime"),
        Value::string(info.mime),
    );
    array.insert(
        crate::builtins::modules::core::string_array_key("width_unit"),
        Value::string("px"),
    );
    array.insert(
        crate::builtins::modules::core::string_array_key("height_unit"),
        Value::string("px"),
    );
    array
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FilesystemCapabilities;
    use crate::OutputBuffer;
    use crate::builtins::BuiltinRegistry;

    #[test]
    fn image_type_to_mime_type_matches_php_standard_mapping() {
        let entry = BuiltinRegistry::new()
            .get("image_type_to_mime_type")
            .expect("image_type_to_mime_type exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let png = (entry.function())(
            &mut context,
            vec![Value::Int(IMAGETYPE_PNG)],
            RuntimeSourceSpan::default(),
        )
        .expect("png mime");
        assert_eq!(png, Value::string("image/png"));

        let webp = (entry.function())(
            &mut context,
            vec![Value::Int(IMAGETYPE_WEBP)],
            RuntimeSourceSpan::default(),
        )
        .expect("webp mime");
        assert_eq!(webp, Value::string("image/webp"));

        let heif = (entry.function())(
            &mut context,
            vec![Value::Int(20)],
            RuntimeSourceSpan::default(),
        )
        .expect("heif mime");
        assert_eq!(heif, Value::string("image/heif"));

        let svg = (entry.function())(
            &mut context,
            vec![Value::Int(IMAGETYPE_SVG)],
            RuntimeSourceSpan::default(),
        )
        .expect("svg mime");
        assert_eq!(svg, Value::string("image/svg+xml"));

        let unknown = (entry.function())(
            &mut context,
            vec![Value::Int(999)],
            RuntimeSourceSpan::default(),
        )
        .expect("unknown mime");
        assert_eq!(unknown, Value::string("application/octet-stream"));
    }

    #[test]
    fn image_type_to_extension_matches_php_standard_mapping() {
        let registry = BuiltinRegistry::new();
        let entry = registry
            .get("image_type_to_extension")
            .expect("image_type_to_extension exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let jpeg_with_dot = (entry.function())(
            &mut context,
            vec![Value::Int(IMAGETYPE_JPEG)],
            RuntimeSourceSpan::default(),
        )
        .expect("jpeg extension");
        assert_eq!(jpeg_with_dot, Value::string(".jpeg"));

        let png_without_dot = (entry.function())(
            &mut context,
            vec![Value::Int(IMAGETYPE_PNG), Value::Bool(false)],
            RuntimeSourceSpan::default(),
        )
        .expect("png extension");
        assert_eq!(png_without_dot, Value::string("png"));

        let unknown = (entry.function())(
            &mut context,
            vec![Value::Int(0)],
            RuntimeSourceSpan::default(),
        )
        .expect("unknown extension");
        assert_eq!(unknown, Value::Bool(false));
    }

    #[test]
    fn image_size_reads_svg_dimensions() {
        let bytes = br#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="64" viewBox="0 0 120 160"></svg>"#;

        assert_eq!(
            image_size(bytes),
            Some(ImageInfo {
                width: 48,
                height: 64,
                image_type: IMAGETYPE_SVG,
                mime: "image/svg+xml",
                bits: None,
                channels: None,
            })
        );
    }

    #[test]
    fn mime_for_bytes_distinguishes_svg_from_generic_xml() {
        assert_eq!(
            mime_for_bytes(
                br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"></svg>"#,
                None,
            ),
            "image/svg+xml"
        );
        assert_eq!(
            mime_for_bytes(
                br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"></svg>"#,
                None,
            ),
            "image/svg+xml"
        );
        assert_eq!(
            mime_for_bytes(br#"<?xml version="1.0"?><root></root>"#, None),
            "text/xml"
        );
    }

    #[test]
    fn finfo_object_uses_libmagic_flags_for_buffers() {
        let registry = BuiltinRegistry::new();
        let open = registry.get("finfo_open").expect("finfo_open exists");
        let set_flags = registry
            .get("finfo_set_flags")
            .expect("finfo_set_flags exists");
        let buffer = registry.get("finfo_buffer").expect("finfo_buffer exists");
        let mut output = OutputBuffer::new();
        let mut context =
            BuiltinContext::with_runtime(&mut output, ".", FilesystemCapabilities::none(), None);

        let finfo = (open.function())(&mut context, vec![], RuntimeSourceSpan::default())
            .expect("open finfo object");
        assert!(matches!(finfo, Value::Object(_)));
        let changed = (set_flags.function())(
            &mut context,
            vec![finfo.clone(), Value::Int(FILEINFO_MIME_TYPE)],
            RuntimeSourceSpan::default(),
        )
        .expect("set flags");
        assert_eq!(changed, Value::Bool(true));
        let detected = (buffer.function())(
            &mut context,
            vec![finfo, Value::string("Regular string here")],
            RuntimeSourceSpan::default(),
        )
        .expect("detect buffer");
        assert_eq!(detected, Value::string("text/plain"));
    }
}
