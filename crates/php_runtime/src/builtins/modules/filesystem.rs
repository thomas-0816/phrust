//! Filesystem builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use crate::{PhpArray, ResourceRef, ResourceTable, StreamWrapperRegistry, Value};
#[cfg(unix)]
use nix::unistd::{Gid, Group, Uid, User, chown};
#[cfg(unix)]
use std::ffi::CString;
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const FILE_APPEND_FLAG: i64 = 8;
const FILE_IGNORE_NEW_LINES_FLAG: i64 = 2;
const FILE_SKIP_EMPTY_LINES_FLAG: i64 = 4;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("basename", builtin_basename, BuiltinCompatibility::Php),
    BuiltinEntry::new("chdir", builtin_chdir, BuiltinCompatibility::Php),
    BuiltinEntry::new("chgrp", builtin_chgrp, BuiltinCompatibility::Php),
    BuiltinEntry::new("chmod", builtin_chmod, BuiltinCompatibility::Php),
    BuiltinEntry::new("chown", builtin_chown, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "clearstatcache",
        builtin_clearstatcache,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("copy", builtin_copy, BuiltinCompatibility::Php),
    BuiltinEntry::new("dirname", builtin_dirname, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "disk_free_space",
        builtin_disk_free_space,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "disk_total_space",
        builtin_disk_total_space,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("file", builtin_file, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "file_exists",
        builtin_file_exists,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "file_get_contents",
        builtin_file_get_contents,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "file_put_contents",
        builtin_file_put_contents,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("filegroup", builtin_filegroup, BuiltinCompatibility::Php),
    BuiltinEntry::new("filemtime", builtin_filemtime, BuiltinCompatibility::Php),
    BuiltinEntry::new("fileowner", builtin_fileowner, BuiltinCompatibility::Php),
    BuiltinEntry::new("fileperms", builtin_fileperms, BuiltinCompatibility::Php),
    BuiltinEntry::new("filesize", builtin_filesize, BuiltinCompatibility::Php),
    BuiltinEntry::new("filetype", builtin_filetype, BuiltinCompatibility::Php),
    BuiltinEntry::new("ftok", builtin_ftok, BuiltinCompatibility::Php),
    BuiltinEntry::new("getcwd", builtin_getcwd, BuiltinCompatibility::Php),
    BuiltinEntry::new("glob", builtin_glob, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_dir", builtin_is_dir, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_file", builtin_is_file, BuiltinCompatibility::Php),
    BuiltinEntry::new("is_link", builtin_is_link, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "is_readable",
        builtin_is_readable,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "is_uploaded_file",
        builtin_is_uploaded_file,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "is_writable",
        builtin_is_writable,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("lstat", builtin_lstat, BuiltinCompatibility::Php),
    BuiltinEntry::new("mkdir", builtin_mkdir, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "move_uploaded_file",
        builtin_move_uploaded_file,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("pathinfo", builtin_pathinfo, BuiltinCompatibility::Php),
    BuiltinEntry::new("readfile", builtin_readfile, BuiltinCompatibility::Php),
    BuiltinEntry::new("realpath", builtin_realpath, BuiltinCompatibility::Php),
    BuiltinEntry::new("rename", builtin_rename, BuiltinCompatibility::Php),
    BuiltinEntry::new("rmdir", builtin_rmdir, BuiltinCompatibility::Php),
    BuiltinEntry::new("stat", builtin_stat, BuiltinCompatibility::Php),
    BuiltinEntry::new("symlink", builtin_symlink, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "sys_get_temp_dir",
        builtin_sys_get_temp_dir,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("tempnam", builtin_tempnam, BuiltinCompatibility::Php),
    BuiltinEntry::new("tmpfile", builtin_tmpfile, BuiltinCompatibility::Php),
    BuiltinEntry::new("touch", builtin_touch, BuiltinCompatibility::Php),
    BuiltinEntry::new("umask", builtin_umask, BuiltinCompatibility::Php),
    BuiltinEntry::new("unlink", builtin_unlink, BuiltinCompatibility::Php),
];

pub(in crate::builtins::modules) fn builtin_basename(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("basename", "one or two argument(s)"));
    }
    let path = string_arg("basename", &args[0])?;
    let suffix = args
        .get(1)
        .map(|value| string_arg("basename", value))
        .transpose()?;
    let output = native_basename(
        path.as_bytes(),
        suffix.as_ref().map(crate::PhpString::as_bytes),
    );
    Ok(Value::string(
        output.bytes(path.as_bytes()).unwrap_or_default().to_vec(),
    ))
}

/// A byte-exact path result that either borrows a range of the stable source
/// string or names a static PHP result such as `"."`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativePathOutput {
    source_start: usize,
    source_length: usize,
    static_bytes: Option<&'static [u8]>,
}

impl NativePathOutput {
    fn source(start: usize, end: usize) -> Self {
        Self {
            source_start: start,
            source_length: end.saturating_sub(start),
            static_bytes: None,
        }
    }

    fn dot() -> Self {
        Self {
            source_start: 0,
            source_length: 0,
            static_bytes: Some(b"."),
        }
    }

    fn bytes(self, source: &[u8]) -> Option<&[u8]> {
        match self.static_bytes {
            Some(bytes) => Some(bytes),
            None => {
                source.get(self.source_start..self.source_start.saturating_add(self.source_length))
            }
        }
    }

    /// Exact number of bytes the native publisher must reserve.
    pub fn output_length(self) -> usize {
        self.static_bytes.map_or(self.source_length, <[u8]>::len)
    }

    /// Copies the planned result directly from the stable source into the
    /// authoritative native string reservation.
    pub fn write_into(self, source: &[u8], output: &mut [u8]) -> bool {
        let Some(bytes) = self.bytes(source) else {
            return false;
        };
        if output.len() != bytes.len() {
            return false;
        }
        output.copy_from_slice(bytes);
        true
    }
}

fn is_native_path_separator(byte: u8) -> bool {
    byte == b'/' || cfg!(windows) && byte == b'\\'
}

fn native_trimmed_path_end(path: &[u8]) -> usize {
    let mut end = path.len();
    while end > 0 && is_native_path_separator(path[end - 1]) {
        end -= 1;
    }
    if end == 0
        && path
            .first()
            .is_some_and(|byte| is_native_path_separator(*byte))
    {
        1
    } else {
        end
    }
}

/// Exact native `basename` plan over stable string bytes.
///
/// No `String`, `Vec`, `BuiltinContext`, or Rust `Value` is constructed.
pub fn native_basename(path: &[u8], suffix: Option<&[u8]>) -> NativePathOutput {
    let end = native_trimmed_path_end(path);
    if end == 0 {
        return NativePathOutput::source(0, 0);
    }
    let start = path[..end]
        .iter()
        .rposition(|byte| is_native_path_separator(*byte))
        .map_or(0, |index| index.saturating_add(1));
    let mut result_end = end;
    if let Some(suffix) = suffix
        && !suffix.is_empty()
        && result_end.saturating_sub(start) > suffix.len()
        && path[start..result_end].ends_with(suffix)
    {
        result_end -= suffix.len();
    }
    NativePathOutput::source(start, result_end)
}

pub(in crate::builtins::modules) fn builtin_dirname(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("dirname", "one or two argument(s)"));
    }
    let path = string_arg("dirname", &args[0])?;
    let levels = args
        .get(1)
        .map(|value| int_arg("dirname", value))
        .transpose()?
        .unwrap_or(1)
        .max(1);
    let output = native_dirname(path.as_bytes(), levels);
    Ok(Value::string(
        output.bytes(path.as_bytes()).unwrap_or_default().to_vec(),
    ))
}

fn native_dirname_once(path: &[u8], current: NativePathOutput) -> NativePathOutput {
    if current.static_bytes.is_some() {
        return current;
    }
    let Some(path) = current.bytes(path) else {
        return NativePathOutput::source(0, 0);
    };
    let end = native_trimmed_path_end(path);
    if end == 0 {
        return NativePathOutput::source(0, 0);
    }
    let Some(index) = path[..end]
        .iter()
        .rposition(|byte| is_native_path_separator(*byte))
    else {
        return NativePathOutput::dot();
    };
    if index == 0 {
        return NativePathOutput::source(0, 1);
    }
    let mut parent_end = index;
    while parent_end > 0 && is_native_path_separator(path[parent_end - 1]) {
        parent_end -= 1;
    }
    if parent_end == 0 {
        NativePathOutput::dot()
    } else {
        NativePathOutput::source(0, parent_end)
    }
}

/// Exact native `dirname` plan over a stable string view.
pub fn native_dirname(path: &[u8], levels: i64) -> NativePathOutput {
    let mut output = NativePathOutput::source(0, path.len());
    for _ in 0..levels.max(1) {
        output = native_dirname_once(path, output);
    }
    output
}

/// Exact native `pathinfo` publication into the authoritative structured
/// value sink. No Rust `Value` or compatibility array is constructed.
pub fn native_pathinfo_into<P: super::json::NativeStructuredValuePublisher>(
    path: &[u8],
    flags: Option<i64>,
    publisher: &mut P,
) -> Option<P::Output> {
    let dirname = native_dirname(path, 1).bytes(path)?;
    let basename = native_basename(path, None).bytes(path)?;
    let extension_separator = basename.iter().rposition(|byte| *byte == b'.');
    let filename = extension_separator.map_or(basename, |index| &basename[..index]);
    let extension = extension_separator.map(|index| &basename[index.saturating_add(1)..]);
    match flags {
        None => publisher
            .publish_object_stream::<()>(|publisher, push| {
                let mut publish = |key: &[u8], bytes: &[u8]| {
                    let value = publisher.publish_string(bytes).ok_or(())?;
                    push(publisher, key, value).ok_or(())
                };
                if !dirname.is_empty() {
                    publish(b"dirname", dirname)?;
                }
                publish(b"basename", basename)?;
                if let Some(extension) = extension {
                    publish(b"extension", extension)?;
                }
                publish(b"filename", filename)?;
                Ok(())
            })
            .ok()
            .flatten(),
        Some(flags) if flags & 1 != 0 => publisher.publish_string(dirname),
        Some(flags) if flags & 2 != 0 => publisher.publish_string(basename),
        Some(flags) if flags & 4 != 0 => publisher.publish_string(extension.unwrap_or_default()),
        Some(flags) if flags & 8 != 0 => publisher.publish_string(filename),
        Some(_) => publisher
            .publish_array_stream::<()>(|_, _| Ok(()))
            .ok()
            .flatten(),
    }
}

pub(in crate::builtins::modules) fn builtin_pathinfo(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("pathinfo", "one or two argument(s)"));
    }
    let path = string_arg("pathinfo", &args[0])?;
    let flags = args
        .get(1)
        .map(|value| int_arg("pathinfo", value))
        .transpose()?;
    let path = path.as_bytes();
    let dirname = native_dirname(path, 1).bytes(path).unwrap_or_default();
    let basename = native_basename(path, None).bytes(path).unwrap_or_default();
    let extension_separator = basename.iter().rposition(|byte| *byte == b'.');
    let filename = extension_separator.map_or(basename, |index| &basename[..index]);
    let extension = extension_separator.map(|index| &basename[index.saturating_add(1)..]);
    match flags {
        None => {
            let mut array = crate::PhpArray::new();
            if !dirname.is_empty() {
                array.insert(string_array_key("dirname"), Value::string(dirname.to_vec()));
            }
            array.insert(
                string_array_key("basename"),
                Value::string(basename.to_vec()),
            );
            if let Some(extension) = extension {
                array.insert(
                    string_array_key("extension"),
                    Value::string(extension.to_vec()),
                );
            }
            array.insert(
                string_array_key("filename"),
                Value::string(filename.to_vec()),
            );
            Ok(Value::Array(array))
        }
        Some(flags) if flags & 1 != 0 => Ok(Value::string(dirname.to_vec())),
        Some(flags) if flags & 2 != 0 => Ok(Value::string(basename.to_vec())),
        Some(flags) if flags & 4 != 0 => {
            Ok(extension.map_or(Value::string(""), |value| Value::string(value.to_vec())))
        }
        Some(flags) if flags & 8 != 0 => Ok(Value::string(filename.to_vec())),
        Some(_) => Ok(Value::Array(crate::PhpArray::new())),
    }
}

pub(in crate::builtins::modules) fn builtin_realpath(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("realpath", &args, 1)?;
    let path = string_arg("realpath", &args[0])?.to_string_lossy();
    let resolved = resolve_runtime_path(context, &path);
    if !context.filesystem_capabilities().allows_path(&resolved) {
        return Ok(Value::Bool(false));
    }
    Ok(
        fs::canonicalize(&resolved).map_or(Value::Bool(false), |path| {
            Value::string(path.to_string_lossy().as_bytes().to_vec())
        }),
    )
}

/// Exact native `realpath` capability operation.
///
/// `None` is the PHP-visible `false` result. Unsupported argument shapes are
/// rejected by the caller before this function is entered.
pub fn native_realpath(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Vec<u8>> {
    let path = String::from_utf8_lossy(path);
    let raw = Path::new(path.as_ref());
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        cwd.join(raw)
    };
    let resolved = normalize_runtime_path(&joined);
    if !filesystem.allows_path(&resolved) {
        return None;
    }
    fs::canonicalize(resolved)
        .ok()
        .map(|path| path.to_string_lossy().into_owned().into_bytes())
}

/// Resolves a successful local `chdir()` without touching request state.
///
/// The outer `None` preserves the single baseline continuation for wrapper
/// paths. `Some(None)` also requests that continuation for a denied, missing,
/// or non-directory target so PHP's warning machinery runs before returning
/// `false`. Only `Some(Some(path))` is safe for the exact handler to publish
/// atomically into the request-owned current-directory slot.
pub fn native_chdir_target(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<PathBuf>> {
    let resolved = native_local_path(cwd, filesystem, path)??;
    let canonical = fs::canonicalize(resolved).ok()?;
    Some((filesystem.allows_path(&canonical) && canonical.is_dir()).then_some(canonical))
}

pub(in crate::builtins::modules) fn builtin_file_exists(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("file_exists", &args, 1)?;
    let path = string_arg("file_exists", &args[0])?.to_string_lossy();
    if crate::phar::is_phar_uri(&path) {
        return Ok(Value::Bool(
            crate::phar::read_uri(&path, context.cwd(), context.filesystem_capabilities()).is_ok(),
        ));
    }
    Ok(Value::Bool(
        metadata_for_arg(context, "file_exists", &args[0], true)?.is_some(),
    ))
}

/// Exact native local-filesystem `file_exists` capability operation.
///
/// Phar URIs require archive/request coordination and therefore return
/// `None` so the optimizing caller can take its one baseline continuation
/// before any filesystem-visible effect. Local paths return `Some(result)`.
pub fn native_file_exists(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<bool> {
    native_local_metadata(cwd, filesystem, path, true).map(|metadata| metadata.is_some())
}

/// Exact native local-filesystem `is_file` query.
///
/// The outer `None` marks a registered/wrapper URI that requires the single
/// baseline continuation. Local denial or metadata failure is the
/// PHP-visible `false` result.
pub fn native_is_file(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<bool> {
    native_local_metadata(cwd, filesystem, path, true)
        .map(|metadata| metadata.is_some_and(|metadata| metadata.is_file()))
}

/// Exact native local-filesystem `is_dir` query.
pub fn native_is_dir(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<bool> {
    native_local_metadata(cwd, filesystem, path, true)
        .map(|metadata| metadata.is_some_and(|metadata| metadata.is_dir()))
}

/// Exact native local-filesystem `is_readable` query.
///
/// This deliberately preserves the runtime's established PHP-visible
/// semantics: an allowed path with readable metadata is considered readable.
pub fn native_is_readable(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<bool> {
    native_local_metadata(cwd, filesystem, path, true).map(|metadata| metadata.is_some())
}

/// Exact native local-filesystem `is_writable` query.
pub fn native_is_writable(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<bool> {
    native_local_metadata(cwd, filesystem, path, true)
        .map(|metadata| metadata.is_some_and(|metadata| !metadata.permissions().readonly()))
}

/// Exact native local-filesystem symbolic-link query.
pub fn native_is_link(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<bool> {
    native_local_metadata(cwd, filesystem, path, false)
        .map(|metadata| metadata.is_some_and(|metadata| metadata.file_type().is_symlink()))
}

/// Exact scalar metadata queries. `Some(None)` is PHP-visible `false`; outer
/// `None` retains the one wrapper-backed baseline continuation.
pub fn native_fileperms(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<i64>> {
    native_local_metadata(cwd, filesystem, path, true)
        .map(|metadata| metadata.map(|metadata| i64::from(metadata_mode(&metadata))))
}

pub fn native_fileowner(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<i64>> {
    native_local_metadata(cwd, filesystem, path, true)
        .map(|metadata| metadata.map(|metadata| i64::from(metadata_owner(&metadata))))
}

pub fn native_filegroup(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<i64>> {
    native_local_metadata(cwd, filesystem, path, true)
        .map(|metadata| metadata.map(|metadata| i64::from(metadata_group(&metadata))))
}

pub fn native_filetype(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<&'static [u8]>> {
    native_local_metadata(cwd, filesystem, path, false)
        .map(|metadata| metadata.map(|metadata| file_type_name(&metadata).as_bytes()))
}

/// Exact scalar disk-space query matching the runtime's current capability
/// model without constructing a `Value`.
pub fn native_disk_space(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<f64>> {
    native_local_path(cwd, filesystem, path).map(|path| {
        path.filter(|path| path.exists())
            .map(|_| 1_099_511_627_776.0)
    })
}

/// Stable Value-free metadata record consumed by exact `stat`/`lstat`
/// handlers before direct publication into the authoritative native array.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeStatRecord {
    pub mode: i64,
    pub size: i64,
    pub mtime: i64,
    pub file_type: &'static [u8],
}

/// Exact native `stat`/`lstat` query. Outer `None` preserves the single
/// wrapper-backed baseline continuation; `Some(None)` is PHP `false`.
pub fn native_stat(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
    follow_links: bool,
) -> Option<Option<NativeStatRecord>> {
    let Some(metadata) = native_local_metadata(cwd, filesystem, path, follow_links)? else {
        return Some(None);
    };
    Some(Some(NativeStatRecord {
        mode: i64::from(metadata_mode(&metadata)),
        size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
        mtime: metadata_mtime(&metadata),
        file_type: file_type_name(&metadata).as_bytes(),
    }))
}

/// Exact native local-filesystem `filesize` query.
///
/// `Some(None)` is the PHP-visible `false` result; outer `None` requests the
/// one baseline continuation for wrapper-backed paths.
pub fn native_filesize(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<i64>> {
    native_local_metadata(cwd, filesystem, path, true)
        .map(|metadata| metadata.map(|metadata| metadata.len() as i64))
}

/// Exact native local-filesystem `filemtime` query.
pub fn native_filemtime(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<i64>> {
    native_local_metadata(cwd, filesystem, path, true)
        .map(|metadata| metadata.map(|metadata| metadata_mtime(&metadata)))
}

/// Exact native local-file `file_get_contents` implementation.
///
/// Stream wrappers, denied capabilities, and I/O errors return `None` before
/// publication so the caller can take its single baseline continuation and
/// preserve wrapper coordination and PHP warnings. A successful local read
/// returns only the final byte allocation.
pub fn native_file_get_contents(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
    offset: i64,
    length: Option<i64>,
) -> Option<Vec<u8>> {
    let resolved = native_local_path(cwd, filesystem, path)??;
    let contents = fs::read(resolved).ok()?;
    Some(file_get_contents_slice(&contents, offset, length))
}

fn next_native_file_line<'a>(
    contents: &'a [u8],
    start: &mut usize,
    ignore_new_lines: bool,
    skip_empty: bool,
) -> Option<&'a [u8]> {
    while *start < contents.len() {
        let end = contents[*start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(contents.len(), |offset| *start + offset + 1);
        let mut line = &contents[*start..end];
        *start = end;
        if ignore_new_lines && line.last() == Some(&b'\n') {
            line = &line[..line.len() - 1];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
        }
        if !skip_empty || !line.is_empty() {
            return Some(line);
        }
    }
    None
}

/// Exact native local-file line splitting for `file()`, published directly
/// into the authoritative native array. Wrapper, capability, I/O, and
/// publication failures retain the single baseline continuation.
pub fn native_file_lines_into<P: super::json::NativeStructuredValuePublisher>(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
    flags: i64,
    publisher: &mut P,
) -> Option<P::Output> {
    if flags & !(FILE_IGNORE_NEW_LINES_FLAG | FILE_SKIP_EMPTY_LINES_FLAG) != 0 {
        return None;
    }
    let resolved = native_local_path(cwd, filesystem, path)??;
    let contents = fs::read(resolved).ok()?;
    let ignore_new_lines = flags & FILE_IGNORE_NEW_LINES_FLAG != 0;
    let skip_empty = flags & FILE_SKIP_EMPTY_LINES_FLAG != 0;
    let mut count_start = 0;
    let mut length = 0;
    while next_native_file_line(&contents, &mut count_start, ignore_new_lines, skip_empty).is_some()
    {
        length += 1;
    }
    let mut publish_start = 0;
    publisher.publish_array_with(length, |publisher, _| {
        let line =
            next_native_file_line(&contents, &mut publish_start, ignore_new_lines, skip_empty)?;
        publisher.publish_string(line)
    })
}

/// Exact result of publishing one local-directory glob.
#[doc(hidden)]
pub enum NativeGlobPublished<T> {
    /// A sorted native array was published.
    Matches(T),
    /// PHP-visible `false` for a missing or unreadable local directory.
    False,
}

/// Exact native local-directory glob, publishing matched path strings
/// directly into the authoritative native array. `None` is the one
/// wrapper-backed or publication baseline continuation.
pub fn native_glob_into<P: super::json::NativeStructuredValuePublisher>(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    pattern: &[u8],
    publisher: &mut P,
) -> Option<NativeGlobPublished<P::Output>> {
    let pattern = String::from_utf8_lossy(pattern);
    if crate::phar::is_phar_uri(&pattern) || pattern.contains("://") {
        return None;
    }
    let wildcard_index = pattern.find(['*', '?']).unwrap_or(pattern.len());
    let parent_end = pattern[..wildcard_index]
        .rfind(php_path_separators())
        .map_or(0, |index| index + 1);
    let (directory, file_pattern) = pattern.split_at(parent_end);
    let directory = if directory.is_empty() {
        cwd.to_path_buf()
    } else {
        let raw = Path::new(directory);
        let joined = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            cwd.join(raw)
        };
        normalize_runtime_path(&joined)
    };
    if !filesystem.allows_path(&directory) || !directory.is_dir() {
        return Some(NativeGlobPublished::False);
    }
    let Ok(read_dir) = fs::read_dir(&directory) else {
        return Some(NativeGlobPublished::False);
    };
    let mut matches = Vec::new();
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if glob_pattern_matches(file_pattern, &name) {
            matches.push(entry.path());
        }
    }
    matches.sort();
    let length = matches.len();
    let mut matches = matches.into_iter();
    let published = publisher.publish_array_with(length, |publisher, _| {
        let path = matches.next()?;
        let path = path.to_string_lossy();
        publisher.publish_string(path.as_bytes())
    })?;
    Some(NativeGlobPublished::Matches(published))
}

/// Exact local-directory projection shared by `opendir()` and `scandir()`.
///
/// The outer `None` is reserved for wrapper-backed paths that require the one
/// baseline continuation. `Some(None)` is PHP-visible `false` for a denied,
/// missing, or unreadable local directory.
pub fn native_directory_entries(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<(PathBuf, Vec<String>)>> {
    let path = String::from_utf8_lossy(path);
    if crate::phar::is_phar_uri(&path) || path.contains("://") {
        return None;
    }
    let raw = Path::new(path.as_ref());
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        cwd.join(raw)
    };
    let resolved = normalize_runtime_path(&joined);
    if !filesystem.allows_path(&resolved) || !resolved.is_dir() {
        return Some(None);
    }
    Some(directory_entries_with_dots(&resolved).map(|entries| (resolved, entries)))
}

/// Exact `scandir()` publication into an authoritative native array.
pub fn native_scandir_into<P: super::json::NativeStructuredValuePublisher>(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
    descending: bool,
    publisher: &mut P,
) -> Option<NativeGlobPublished<P::Output>> {
    let Some((_, mut entries)) = native_directory_entries(cwd, filesystem, path)? else {
        return Some(NativeGlobPublished::False);
    };
    if descending {
        entries.reverse();
    }
    let length = entries.len();
    let mut entries = entries.into_iter();
    publisher
        .publish_array_with(length, |publisher, _| {
            publisher.publish_string(entries.next()?.as_bytes())
        })
        .map(NativeGlobPublished::Matches)
}

/// Exact native local-file `file_put_contents` implementation.
///
/// Outer `None` is reserved for wrapper-backed paths that require the one
/// baseline continuation. Once a local path has been admitted, the operation
/// completes exactly once and returns `Some(None)` for PHP-visible `false` or
/// `Some(Some(bytes))` for the written byte count; it never asks the caller to
/// replay a mutation through the baseline tier.
pub fn native_file_put_contents(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
    bytes: &[u8],
    flags: i64,
) -> Option<Option<i64>> {
    let Some(resolved) = native_local_path(cwd, filesystem, path)? else {
        return Some(None);
    };
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true);
    if flags & FILE_APPEND_FLAG != 0 {
        options.append(true);
    } else {
        options.truncate(true);
    }
    Some(
        options
            .open(resolved)
            .and_then(|mut file| std::io::Write::write_all(&mut file, bytes))
            .ok()
            .map(|()| i64::try_from(bytes.len()).unwrap_or(i64::MAX)),
    )
}

/// Exact native local-file rename. No failure after admission requests a
/// baseline replay because the filesystem may already have observed the
/// mutation.
pub fn native_rename(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    from: &[u8],
    to: &[u8],
) -> Option<bool> {
    let Some(from) = native_local_path(cwd, filesystem, from)? else {
        return Some(false);
    };
    let Some(to) = native_local_path(cwd, filesystem, to)? else {
        return Some(false);
    };
    Some(fs::rename(from, to).is_ok())
}

/// Terminal result of one admitted native uploaded-file move.
///
/// The operation owns its mutation once called: failure variants must be
/// reported by the generated caller and must never be replayed through the
/// baseline builtin implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeMoveUploadedFileResult {
    NotActiveUpload,
    DestinationDenied,
    SamePath,
    MoveFailed,
    Moved,
}

/// Exact request-local uploaded-file move over the stable upload registry and
/// filesystem capability. Outer `None` is reserved for a wrapper-backed
/// destination and is returned before any filesystem mutation.
pub fn native_move_uploaded_file(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    registry: &mut crate::UploadRegistry,
    from: &[u8],
    to: &[u8],
) -> Option<NativeMoveUploadedFileResult> {
    let from_text = String::from_utf8_lossy(from);
    if !registry.is_active_upload(&from_text) {
        return Some(NativeMoveUploadedFileResult::NotActiveUpload);
    }
    let Some(to) = native_local_path(cwd, filesystem, to)? else {
        return Some(NativeMoveUploadedFileResult::DestinationDenied);
    };
    let from_path = PathBuf::from(from_text.as_ref());
    if same_filesystem_path(&from_path, &to) {
        return Some(NativeMoveUploadedFileResult::SamePath);
    }
    if move_upload_temp_file(&from_path, &to).is_err() {
        return Some(NativeMoveUploadedFileResult::MoveFailed);
    }
    registry.mark_moved(&from_text);
    Some(NativeMoveUploadedFileResult::Moved)
}

/// Exact native local-file removal.
pub fn native_unlink(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<bool> {
    native_local_path(cwd, filesystem, path)
        .map(|path| path.is_some_and(|path| fs::remove_file(path).is_ok()))
}

/// Exact native local-directory creation after scalar argument coercion and
/// stream-context validation have completed at the generated call boundary.
pub fn native_mkdir(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
    mode: i64,
    recursive: bool,
    umask: i64,
) -> Option<bool> {
    native_local_path(cwd, filesystem, path).map(|path| {
        path.is_some_and(|path| {
            if recursive && path.exists() {
                return false;
            }
            let result = if recursive {
                fs::create_dir_all(&path)
            } else {
                fs::create_dir(&path)
            };
            if result.is_err() {
                return false;
            }
            let masked = mode & !umask;
            let _ = set_permissions_mode(&path, masked as u32);
            true
        })
    })
}

/// Exact native local-directory removal.
pub fn native_rmdir(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<bool> {
    native_local_path(cwd, filesystem, path)
        .map(|path| path.is_some_and(|path| fs::remove_dir(path).is_ok()))
}

/// Exact native local-file touch for the default-argument shape.
pub fn native_touch(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<bool> {
    native_local_path(cwd, filesystem, path).map(|path| {
        path.is_some_and(|path| {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .is_ok()
        })
    })
}

/// Exact native local-file permission mutation.
///
/// Wrapper paths retain their one baseline continuation. Capability denial
/// and operating-system failure are final `false` results because the
/// baseline implementation emits no diagnostic for either case.
pub fn native_chmod(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
    mode: i64,
) -> Option<bool> {
    native_local_path(cwd, filesystem, path)
        .map(|path| path.is_some_and(|path| set_permissions_mode(&path, mode as u32).is_ok()))
}

/// Exact native local symbolic-link mutation.
///
/// Both paths are admitted before the effect. Once admitted, failure is the
/// final PHP-visible `false` result and is never replayed through baseline.
pub fn native_symlink(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    target: &[u8],
    link: &[u8],
) -> Option<bool> {
    let Some(target) = native_local_path(cwd, filesystem, target)? else {
        return Some(false);
    };
    let Some(link) = native_local_path(cwd, filesystem, link)? else {
        return Some(false);
    };
    Some(create_symlink(&target, &link).is_ok())
}

/// Creates one exact request-local temporary file and returns its path.
///
/// Outer `None` retains the wrapper-backed baseline continuation.
/// `Some(None)` is a final capability or creation failure. The caller owns
/// rollback if publishing the resulting native string fails.
pub fn native_tempnam(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    directory: &[u8],
    prefix: &[u8],
) -> Option<Option<PathBuf>> {
    let requested = native_local_path(cwd, filesystem, directory)?;
    let directory = requested.or_else(|| filesystem.first_allowed_root().map(Path::to_path_buf));
    let Some(directory) = directory else {
        return Some(None);
    };
    let prefix = String::from_utf8_lossy(prefix);
    for index in 0..1_000 {
        let path = directory.join(format!("{prefix}{}-{index}", std::process::id()));
        if fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .is_ok()
        {
            return Some(Some(path));
        }
    }
    Some(None)
}

/// Creates one uniquely owned temporary stream.
///
/// The stream resource owns removal of its backing path, so exact native and
/// baseline callers share the same close/finalization semantics without
/// maintaining a second cleanup map.
pub fn native_tmpfile(
    resources: &mut ResourceTable,
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    stdin: &[u8],
) -> Option<ResourceRef> {
    let root = filesystem.first_allowed_root()?;
    for index in 0..1_000 {
        let path = root.join(format!("phrust-tmpfile-{}-{index}", std::process::id()));
        if fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .is_err()
        {
            continue;
        }
        let resource = match StreamWrapperRegistry::new().open(
            resources,
            &path.to_string_lossy(),
            "c+",
            cwd,
            filesystem,
            stdin,
        ) {
            Ok(resource) => resource,
            Err(_) => {
                let _ = fs::remove_file(path);
                return None;
            }
        };
        if resource.mark_delete_on_close() {
            return Some(resource);
        }
        resource.close();
        let _ = fs::remove_file(path);
        return None;
    }
    None
}

fn native_local_metadata(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
    follow_links: bool,
) -> Option<Option<fs::Metadata>> {
    let Some(resolved) = native_local_path(cwd, filesystem, path)? else {
        return Some(None);
    };
    let metadata = if follow_links {
        fs::metadata(resolved)
    } else {
        fs::symlink_metadata(resolved)
    };
    Some(metadata.ok())
}

/// Resolves a local path without consulting runtime `Value` or builtin state.
///
/// Outer `None` is a wrapper URI, `Some(None)` is capability denial, and
/// `Some(Some(path))` is an admitted local path.
fn native_local_path(
    cwd: &Path,
    filesystem: &crate::FilesystemCapabilities,
    path: &[u8],
) -> Option<Option<PathBuf>> {
    let path = String::from_utf8_lossy(path);
    if crate::phar::is_phar_uri(&path) || path.contains("://") {
        return None;
    }
    let raw = Path::new(path.as_ref());
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        cwd.join(raw)
    };
    let resolved = normalize_runtime_path(&joined);
    Some(filesystem.allows_path(&resolved).then_some(resolved))
}

pub(in crate::builtins::modules) fn builtin_is_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_file", &args, 1)?;
    let path = string_arg("is_file", &args[0])?.to_string_lossy();
    if crate::phar::is_phar_uri(&path) {
        return Ok(Value::Bool(
            crate::phar::read_uri(&path, context.cwd(), context.filesystem_capabilities()).is_ok(),
        ));
    }
    Ok(Value::Bool(
        metadata_for_arg(context, "is_file", &args[0], true)?
            .is_some_and(|metadata| metadata.is_file()),
    ))
}

pub(in crate::builtins::modules) fn builtin_is_dir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_dir", &args, 1)?;
    Ok(Value::Bool(
        metadata_for_arg(context, "is_dir", &args[0], true)?
            .is_some_and(|metadata| metadata.is_dir()),
    ))
}

pub(in crate::builtins::modules) fn builtin_is_link(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_link", &args, 1)?;
    Ok(Value::Bool(
        metadata_for_arg(context, "is_link", &args[0], false)?
            .is_some_and(|metadata| metadata.file_type().is_symlink()),
    ))
}

pub(in crate::builtins::modules) fn builtin_is_readable(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_readable", &args, 1)?;
    Ok(Value::Bool(
        metadata_for_arg(context, "is_readable", &args[0], true)?.is_some(),
    ))
}

pub(in crate::builtins::modules) fn builtin_is_writable(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_writable", &args, 1)?;
    Ok(Value::Bool(
        metadata_for_arg(context, "is_writable", &args[0], true)?
            .is_some_and(|metadata| !metadata.permissions().readonly()),
    ))
}

pub(in crate::builtins::modules) fn builtin_is_uploaded_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("is_uploaded_file", &args, 1)?;
    let path = string_arg("is_uploaded_file", &args[0])?.to_string_lossy();
    Ok(Value::Bool(
        context
            .upload_registry()
            .is_some_and(|registry| registry.is_active_upload(&path)),
    ))
}

pub(in crate::builtins::modules) fn builtin_filesize(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("filesize", &args, 1)?;
    Ok(metadata_for_arg(context, "filesize", &args[0], true)?
        .map_or(Value::Bool(false), |metadata| {
            Value::Int(metadata.len() as i64)
        }))
}

pub(in crate::builtins::modules) fn builtin_filemtime(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("filemtime", &args, 1)?;
    Ok(metadata_for_arg(context, "filemtime", &args[0], true)?
        .map_or(Value::Bool(false), |metadata| {
            Value::Int(metadata_mtime(&metadata))
        }))
}

pub(in crate::builtins::modules) fn builtin_fileperms(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fileperms", &args, 1)?;
    Ok(metadata_for_arg(context, "fileperms", &args[0], true)?
        .map_or(Value::Bool(false), |metadata| {
            Value::Int(metadata_mode(&metadata) as i64)
        }))
}

pub(in crate::builtins::modules) fn builtin_fileowner(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("fileowner", &args, 1)?;
    Ok(metadata_for_arg(context, "fileowner", &args[0], true)?
        .map_or(Value::Bool(false), |metadata| {
            Value::Int(metadata_owner(&metadata) as i64)
        }))
}

pub(in crate::builtins::modules) fn builtin_filegroup(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("filegroup", &args, 1)?;
    Ok(metadata_for_arg(context, "filegroup", &args[0], true)?
        .map_or(Value::Bool(false), |metadata| {
            Value::Int(metadata_group(&metadata) as i64)
        }))
}

pub(in crate::builtins::modules) fn builtin_filetype(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("filetype", &args, 1)?;
    Ok(metadata_for_arg(context, "filetype", &args[0], false)?
        .map_or(Value::Bool(false), |metadata| {
            Value::string(file_type_name(&metadata))
        }))
}

pub(in crate::builtins::modules) fn builtin_chmod(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("chmod", &args, 2)?;
    let path = resolve_runtime_path(context, &string_arg("chmod", &args[0])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) {
        return Ok(Value::Bool(false));
    }
    let mode = int_arg("chmod", &args[1])?;
    Ok(Value::Bool(
        set_permissions_mode(&path, mode as u32).is_ok(),
    ))
}

pub(in crate::builtins::modules) fn builtin_chown(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    change_owner_or_group(context, args, span, "chown", OwnershipTarget::User)
}

pub(in crate::builtins::modules) fn builtin_chgrp(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    change_owner_or_group(context, args, span, "chgrp", OwnershipTarget::Group)
}

#[derive(Clone, Copy)]
enum OwnershipTarget {
    User,
    Group,
}

fn change_owner_or_group(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
    function: &str,
    target: OwnershipTarget,
) -> BuiltinResult {
    expect_arity(function, &args, 2)?;
    let path = resolve_runtime_path(context, &string_arg(function, &args[0])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) {
        return Ok(Value::Bool(false));
    }
    change_owner_or_group_path(context, &path, &args[1], span, function, target)
}

#[cfg(unix)]
fn change_owner_or_group_path(
    context: &mut BuiltinContext<'_>,
    path: &Path,
    value: &Value,
    span: RuntimeSourceSpan,
    function: &str,
    target: OwnershipTarget,
) -> BuiltinResult {
    let Some(id) = ownership_id(context, value, span.clone(), function, target)? else {
        return Ok(Value::Bool(false));
    };
    let result = match target {
        OwnershipTarget::User => chown(path, Some(Uid::from_raw(id)), None),
        OwnershipTarget::Group => chown(path, None, Some(Gid::from_raw(id))),
    };
    if result.is_ok() {
        return Ok(Value::Bool(true));
    }
    context.php_warning(
        "E_PHP_RUNTIME_CHOWN_FAILED",
        format!(
            "{function}(): {}",
            result
                .err()
                .map_or_else(|| "Operation failed".to_owned(), errno_message)
        ),
        span,
    );
    Ok(Value::Bool(false))
}

#[cfg(not(unix))]
fn change_owner_or_group_path(
    _context: &mut BuiltinContext<'_>,
    _path: &Path,
    _value: &Value,
    _span: RuntimeSourceSpan,
    _function: &str,
    _target: OwnershipTarget,
) -> BuiltinResult {
    Ok(Value::Bool(false))
}

#[cfg(unix)]
fn ownership_id(
    context: &mut BuiltinContext<'_>,
    value: &Value,
    span: RuntimeSourceSpan,
    function: &str,
    target: OwnershipTarget,
) -> Result<Option<u32>, crate::builtins::BuiltinError> {
    match deref_value(value) {
        Value::String(name) => {
            let name = name.to_string_lossy();
            let Some(id) = lookup_owner_or_group_id(&name, target) else {
                let (kind, label) = match target {
                    OwnershipTarget::User => ("uid", "user"),
                    OwnershipTarget::Group => ("gid", "group"),
                };
                context.php_warning(
                    "E_PHP_RUNTIME_CHOWN_LOOKUP_FAILED",
                    format!("{function}(): Unable to find {kind} for {label} {name}"),
                    span,
                );
                return Ok(None);
            };
            Ok(Some(id))
        }
        _ => Ok(Some(int_arg(function, value)? as u32)),
    }
}

#[cfg(unix)]
fn lookup_owner_or_group_id(name: &str, target: OwnershipTarget) -> Option<u32> {
    match target {
        OwnershipTarget::User => User::from_name(name)
            .ok()
            .flatten()
            .map(|user| user.uid.as_raw()),
        OwnershipTarget::Group => Group::from_name(name)
            .ok()
            .flatten()
            .map(|group| group.gid.as_raw()),
    }
}

#[cfg(unix)]
fn errno_message(errno: nix::errno::Errno) -> String {
    errno.desc().to_owned()
}

pub(in crate::builtins::modules) fn builtin_umask(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("umask", "zero or one argument(s)"));
    }
    let previous = context.filesystem_state().umask();
    if let Some(value) = args.first() {
        let mode = int_arg("umask", value)?;
        context.filesystem_state().set_umask(mode);
    }
    Ok(Value::Int(previous))
}

pub(in crate::builtins::modules) fn builtin_sys_get_temp_dir(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("sys_get_temp_dir", &args, 0)?;
    let path = std::env::temp_dir();
    Ok(Value::string(path.to_string_lossy().as_bytes().to_vec()))
}

pub(in crate::builtins::modules) fn builtin_disk_free_space(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("disk_free_space", &args, 1)?;
    disk_space_value(context, "disk_free_space", &args[0])
}

pub(in crate::builtins::modules) fn builtin_disk_total_space(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("disk_total_space", &args, 1)?;
    disk_space_value(context, "disk_total_space", &args[0])
}

pub(in crate::builtins::modules) fn builtin_stat(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("stat", &args, 1)?;
    Ok(metadata_for_arg(context, "stat", &args[0], true)?.map_or(Value::Bool(false), stat_array))
}

pub(in crate::builtins::modules) fn builtin_lstat(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("lstat", &args, 1)?;
    Ok(metadata_for_arg(context, "lstat", &args[0], false)?.map_or(Value::Bool(false), stat_array))
}

pub(in crate::builtins::modules) fn builtin_clearstatcache(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error(
            "clearstatcache",
            "zero, one, or two argument(s)",
        ));
    }
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_file_get_contents(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 5 {
        return Err(arity_error("file_get_contents", "one to five argument(s)"));
    }
    let path = string_arg("file_get_contents", &args[0])?.to_string_lossy();
    let offset = args
        .get(3)
        .filter(|value| !matches!(deref_value(value), Value::Null))
        .map(|value| int_arg("file_get_contents", value))
        .transpose()?
        .unwrap_or(0);
    let length = args
        .get(4)
        .filter(|value| !matches!(deref_value(value), Value::Null))
        .map(|value| int_arg("file_get_contents", value))
        .transpose()?;
    if matches!(length, Some(length) if length < 0) {
        return Err(argument_value_error(
            "file_get_contents",
            "#5 ($length)",
            "must be greater than or equal to 0",
        ));
    }

    match read_file_value(context, "file_get_contents", &path, span)? {
        Value::String(contents) if offset != 0 || length.is_some() => Ok(Value::string(
            file_get_contents_slice(contents.as_bytes(), offset, length),
        )),
        value => Ok(value),
    }
}

pub(in crate::builtins::modules) fn builtin_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("file", "one to three argument(s)"));
    }
    let path = string_arg("file", &args[0])?.to_string_lossy();
    let flags = args
        .get(1)
        .map(|value| int_arg("file", value))
        .transpose()?
        .unwrap_or(0);
    let contents = match read_file_value(context, "file", &path, span)? {
        Value::String(contents) => contents,
        value => return Ok(value),
    };
    let ignore_new_lines = flags & FILE_IGNORE_NEW_LINES_FLAG != 0;
    let skip_empty = flags & FILE_SKIP_EMPTY_LINES_FLAG != 0;
    let mut lines = Vec::new();
    let mut start = 0;
    let bytes = contents.as_bytes();
    while start < bytes.len() {
        let end = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| start + offset + 1);
        let mut line = bytes[start..end].to_vec();
        if ignore_new_lines && line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        if !skip_empty || !line.is_empty() {
            lines.push(Value::string(line));
        }
        start = end;
    }
    Ok(Value::Array(PhpArray::from_packed(lines)))
}

fn file_get_contents_slice(bytes: &[u8], offset: i64, length: Option<i64>) -> Vec<u8> {
    let byte_len = bytes.len() as i128;
    let offset = offset as i128;
    let start = if offset >= 0 {
        offset.min(byte_len)
    } else {
        (byte_len + offset).max(0)
    };
    let end = match length {
        Some(length) => (start + i128::from(length)).min(byte_len),
        None => byte_len,
    };
    bytes[start as usize..end as usize].to_vec()
}

pub(in crate::builtins::modules) fn builtin_ftok(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("ftok", &args, 2)?;
    let filename = string_arg("ftok", &args[0])?;
    if filename.as_bytes().is_empty() {
        return Err(argument_value_error(
            "ftok",
            "#1 ($filename)",
            "must not be empty",
        ));
    }
    if filename.as_bytes().contains(&0) {
        return Err(argument_value_error(
            "ftok",
            "#1 ($filename)",
            "must not contain any null bytes",
        ));
    }

    let project_id = string_arg("ftok", &args[1])?;
    if project_id.as_bytes().len() != 1 {
        return Err(argument_value_error(
            "ftok",
            "#2 ($project_id)",
            "must be a single character",
        ));
    }

    let resolved = resolve_runtime_path(context, &filename.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&resolved) {
        return Ok(Value::Int(-1));
    }

    ftok_key(context, &resolved, project_id.as_bytes()[0], span)
}

#[allow(unsafe_code)] // direct libc call, result checked
#[cfg(unix)]
fn ftok_key(
    context: &mut BuiltinContext<'_>,
    path: &Path,
    project_id: u8,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        argument_value_error("ftok", "#1 ($filename)", "must not contain any null bytes")
    })?;
    let key = unsafe { libc::ftok(c_path.as_ptr(), i32::from(project_id)) };
    if key == -1 {
        context.php_warning(
            "E_PHP_RUNTIME_FTOK_FAILED",
            format!(
                "ftok(): ftok() failed - {}",
                php_io_error_message(&std::io::Error::last_os_error())
            ),
            span,
        );
    }
    Ok(Value::Int(i64::from(key)))
}

#[cfg(not(unix))]
fn ftok_key(
    context: &mut BuiltinContext<'_>,
    _path: &Path,
    _project_id: u8,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    context.php_warning(
        "E_PHP_RUNTIME_FTOK_UNSUPPORTED",
        "ftok(): ftok() failed - Function not implemented",
        span,
    );
    Ok(Value::Int(-1))
}

pub(in crate::builtins::modules) fn builtin_file_put_contents(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error(
            "file_put_contents",
            "two, three, or four argument(s)",
        ));
    }
    let path = string_arg("file_put_contents", &args[0])?.to_string_lossy();
    let bytes = string_arg("file_put_contents", &args[1])?
        .as_bytes()
        .to_vec();
    let flags = args
        .get(2)
        .map(|value| int_arg("file_put_contents", value))
        .transpose()?
        .unwrap_or(0);
    let resolved = resolve_runtime_path(context, &path);
    if !context.filesystem_capabilities().allows_path(&resolved) {
        return Ok(Value::Bool(false));
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true);
    if flags & FILE_APPEND_FLAG != 0 {
        options.append(true);
    } else {
        options.truncate(true);
    }
    Ok(options
        .open(&resolved)
        .and_then(|mut file| std::io::Write::write_all(&mut file, &bytes))
        .map_or(Value::Bool(false), |_| Value::Int(bytes.len() as i64)))
}

pub(in crate::builtins::modules) fn builtin_readfile(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("readfile", &args, 1)?;
    let path = string_arg("readfile", &args[0])?.to_string_lossy();
    let Value::String(bytes) = read_file_value(context, "readfile", &path, span)? else {
        return Ok(Value::Bool(false));
    };
    let len = bytes.len();
    context.output().write_php_string(&bytes);
    Ok(Value::Int(len as i64))
}

pub(in crate::builtins::modules) fn builtin_copy(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("copy", &args, 2)?;
    let from_arg = string_arg("copy", &args[0])?.to_string_lossy();
    let from = resolve_runtime_path(context, &from_arg);
    let to = resolve_runtime_path(context, &string_arg("copy", &args[1])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&from) {
        let message = match from.try_exists() {
            Ok(false) => "No such file or directory".to_string(),
            Ok(true) | Err(_) => "Operation not permitted".to_string(),
        };
        context.php_warning(
            "E_PHP_RUNTIME_STREAM_OPEN",
            format!("copy({from_arg}): Failed to open stream: {message}"),
            span,
        );
        return Ok(Value::Bool(false));
    }
    if !context.filesystem_capabilities().allows_path(&to) {
        return Ok(Value::Bool(false));
    }
    if same_filesystem_path(&from, &to) {
        return Ok(Value::Bool(false));
    }
    match fs::copy(from, to) {
        Ok(_) => Ok(Value::Bool(true)),
        Err(error) => {
            context.php_warning(
                "E_PHP_RUNTIME_STREAM_OPEN",
                format!(
                    "copy({from_arg}): Failed to open stream: {}",
                    php_io_error_message(&error)
                ),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_rename(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("rename", &args, 2)?;
    let from = resolve_runtime_path(context, &string_arg("rename", &args[0])?.to_string_lossy());
    let to = resolve_runtime_path(context, &string_arg("rename", &args[1])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&from)
        || !context.filesystem_capabilities().allows_path(&to)
    {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(fs::rename(from, to).is_ok()))
}

pub(in crate::builtins::modules) fn builtin_move_uploaded_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("move_uploaded_file", &args, 2)?;
    let from = string_arg("move_uploaded_file", &args[0])?.to_string_lossy();
    let to_arg = string_arg("move_uploaded_file", &args[1])?.to_string_lossy();

    if !context
        .upload_registry()
        .is_some_and(|registry| registry.is_active_upload(&from))
    {
        return Ok(Value::Bool(false));
    }

    let to = resolve_runtime_path(context, &to_arg);
    if !context.filesystem_capabilities().allows_path(&to) {
        context.php_warning(
            "E_PHP_UPLOAD_DESTINATION_DENIED",
            "move_uploaded_file(): destination is outside allowed filesystem roots",
            span.clone(),
        );
        return Ok(Value::Bool(false));
    }
    let from_path = PathBuf::from(&from);
    if same_filesystem_path(&from_path, &to) {
        context.php_warning(
            "E_PHP_UPLOAD_SAME_PATH",
            "move_uploaded_file(): source and destination must differ",
            span.clone(),
        );
        return Ok(Value::Bool(false));
    }

    if move_upload_temp_file(&from_path, &to).is_err() {
        context.php_warning(
            "E_PHP_UPLOAD_MOVE_FAILED",
            "move_uploaded_file(): failed to move uploaded file",
            span,
        );
        return Ok(Value::Bool(false));
    }
    if let Some(registry) = context.upload_registry_mut() {
        registry.mark_moved(&from);
    }
    Ok(Value::Bool(true))
}

fn move_upload_temp_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            if fs::copy(from, to).is_err() {
                return Err(rename_error);
            }
            if let Err(unlink_error) = fs::remove_file(from) {
                let _ = fs::remove_file(to);
                return Err(unlink_error);
            }
            Ok(())
        }
    }
}

pub(in crate::builtins::modules) fn builtin_unlink(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("unlink", &args, 1)?;
    let path = resolve_runtime_path(context, &string_arg("unlink", &args[0])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(fs::remove_file(path).is_ok()))
}

pub(in crate::builtins::modules) fn builtin_symlink(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("symlink", &args, 2)?;
    let target = resolve_runtime_path(context, &string_arg("symlink", &args[0])?.to_string_lossy());
    let link = resolve_runtime_path(context, &string_arg("symlink", &args[1])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&target)
        || !context.filesystem_capabilities().allows_path(&link)
    {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(create_symlink(&target, &link).is_ok()))
}

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}

#[cfg(not(any(unix, windows)))]
fn create_symlink(_target: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "symlinks are not supported on this platform",
    ))
}

pub(in crate::builtins::modules) fn builtin_mkdir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 4 {
        return Err(arity_error("mkdir", "one to four argument(s)"));
    }
    let path = resolve_runtime_path(context, &string_arg("mkdir", &args[0])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) {
        return Ok(Value::Bool(false));
    }
    let recursive = args
        .get(2)
        .is_some_and(|value| matches!(deref_value(value), Value::Bool(true)));
    let result = if recursive {
        fs::create_dir_all(&path)
    } else {
        fs::create_dir(&path)
    };
    if result.is_ok() {
        if let Some(mode_value) = args.get(1) {
            let mode = int_arg("mkdir", mode_value)?;
            let masked = mode & !context.filesystem_state().umask();
            let _ = set_permissions_mode(&path, masked as u32);
        }
        return Ok(Value::Bool(true));
    }
    Ok(Value::Bool(false))
}

pub(in crate::builtins::modules) fn builtin_rmdir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("rmdir", &args, 1)?;
    let path = resolve_runtime_path(context, &string_arg("rmdir", &args[0])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(fs::remove_dir(path).is_ok()))
}

pub(in crate::builtins::modules) fn builtin_touch(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("touch", "one to three argument(s)"));
    }
    let path = resolve_runtime_path(context, &string_arg("touch", &args[0])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .is_ok(),
    ))
}

pub(in crate::builtins::modules) fn builtin_tempnam(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("tempnam", &args, 2)?;
    let requested_dir =
        resolve_runtime_path(context, &string_arg("tempnam", &args[0])?.to_string_lossy());
    let prefix = string_arg("tempnam", &args[1])?.to_string_lossy();
    let dir = if context
        .filesystem_capabilities()
        .allows_path(&requested_dir)
    {
        requested_dir
    } else if let Some(root) = context.filesystem_capabilities().first_allowed_root() {
        root.to_path_buf()
    } else {
        return Ok(Value::Bool(false));
    };
    for index in 0..1000 {
        let path = dir.join(format!("{prefix}{}-{index}", std::process::id()));
        if fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .is_ok()
        {
            return Ok(Value::string(path.to_string_lossy().as_bytes().to_vec()));
        }
    }
    Ok(Value::Bool(false))
}

pub(in crate::builtins::modules) fn builtin_tmpfile(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("tmpfile", &args, 0)?;
    let cwd = context.cwd().to_path_buf();
    let filesystem = context.filesystem_capabilities().clone();
    let Some(resources) = context.resources() else {
        return Ok(Value::Bool(false));
    };
    Ok(native_tmpfile(resources, &cwd, &filesystem, &[])
        .map_or(Value::Bool(false), Value::Resource))
}

pub(in crate::builtins::modules) fn builtin_glob(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("glob", "one or two argument(s)"));
    }
    let pattern = string_arg("glob", &args[0])?.to_string_lossy();
    let (directory, file_pattern) = glob_directory_and_pattern(context, &pattern);
    if !context.filesystem_capabilities().allows_path(&directory) || !directory.is_dir() {
        return Ok(Value::Bool(false));
    }
    let mut matches = Vec::new();
    let Ok(read_dir) = fs::read_dir(&directory) else {
        return Ok(Value::Bool(false));
    };
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if glob_pattern_matches(&file_pattern, &name) {
            matches.push(entry.path().to_string_lossy().to_string());
        }
    }
    matches.sort();
    Ok(Value::packed_array(
        matches.into_iter().map(Value::string).collect(),
    ))
}

pub(in crate::builtins::modules) fn builtin_getcwd(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("getcwd", &args, 0)?;
    Ok(Value::string(
        context.cwd().to_string_lossy().as_bytes().to_vec(),
    ))
}

pub(in crate::builtins::modules) fn builtin_chdir(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("chdir", &args, 1)?;
    let path = resolve_runtime_path(context, &string_arg("chdir", &args[0])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) {
        context.php_warning(
            "E_PHP_CHDIR",
            format!("chdir(): Permission denied: {}", path.display()),
            span,
        );
        return Ok(Value::Bool(false));
    }
    let canonical = match fs::canonicalize(&path) {
        Ok(path) if path.is_dir() && context.filesystem_capabilities().allows_path(&path) => path,
        Ok(_) => {
            context.php_warning(
                "E_PHP_CHDIR",
                format!("chdir(): Not a directory: {}", path.display()),
                span,
            );
            return Ok(Value::Bool(false));
        }
        Err(error) => {
            context.php_warning(
                "E_PHP_CHDIR",
                format!("chdir(): {error}: {}", path.display()),
                span,
            );
            return Ok(Value::Bool(false));
        }
    };
    context.set_cwd(canonical);
    Ok(Value::Bool(true))
}

fn disk_space_value(context: &mut BuiltinContext<'_>, name: &str, value: &Value) -> BuiltinResult {
    let path = resolve_runtime_path(context, &string_arg(name, value)?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) || !path.exists() {
        return Ok(Value::Bool(false));
    }
    Ok(Value::float(1_099_511_627_776.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{FilesystemCapabilities, OutputBuffer, RuntimeUploadedFile, UploadRegistry};

    #[allow(unsafe_code)] // test compares against the same host libc oracle
    #[cfg(unix)]
    #[test]
    fn ftok_matches_host_key_for_allowed_path() {
        let root = unique_temp_dir("ftok-key");
        std::fs::create_dir_all(&root).expect("create temp root");
        let file = root.join("source.php");
        std::fs::write(&file, b"<?php").expect("write ftok source");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut context =
            BuiltinContext::with_runtime(&mut output, root.clone(), capabilities, None);
        let c_path = CString::new(file.as_os_str().as_bytes()).expect("path has no null bytes");
        let expected = unsafe { libc::ftok(c_path.as_ptr(), i32::from(b'P')) };

        assert_eq!(
            builtin_ftok(
                &mut context,
                vec![Value::string("source.php"), Value::string("P")],
                RuntimeSourceSpan::default(),
            )
            .expect("builtin should return"),
            Value::Int(i64::from(expected))
        );

        let _ = std::fs::remove_file(file);
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn ftok_requires_single_character_project_id() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let error = builtin_ftok(
            &mut context,
            vec![Value::string("source.php"), Value::string("PQ")],
            RuntimeSourceSpan::default(),
        )
        .expect_err("project id should be rejected");

        assert_eq!(
            error.message(),
            "ftok(): Argument #2 ($project_id) must be a single character"
        );
    }

    #[test]
    fn ftok_returns_minus_one_for_denied_path() {
        let root = unique_temp_dir("ftok-denied-root");
        let outside = unique_temp_dir("ftok-denied-outside");
        std::fs::create_dir_all(&root).expect("create temp root");
        std::fs::create_dir_all(&outside).expect("create outside root");
        let source = outside.join("source.php");
        std::fs::write(&source, b"<?php").expect("write outside source");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut context =
            BuiltinContext::with_runtime(&mut output, root.clone(), capabilities, None);

        assert_eq!(
            builtin_ftok(
                &mut context,
                vec![
                    Value::string(source.to_string_lossy().to_string()),
                    Value::string("P"),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("builtin should return"),
            Value::Int(-1)
        );
        assert!(context.take_diagnostics().is_empty());

        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_dir(outside);
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn is_uploaded_file_checks_request_local_registry() {
        let root = unique_temp_dir("is-uploaded");
        std::fs::create_dir_all(&root).expect("create temp root");
        let upload = root.join("upload.tmp");
        std::fs::write(&upload, b"payload").expect("write upload");
        let upload_string = upload.to_string_lossy().to_string();
        let mut registry = UploadRegistry::from_uploaded_files(&[uploaded_file(&upload_string)]);

        assert_eq!(
            call_upload_builtin(
                builtin_is_uploaded_file,
                vec![Value::string(upload_string.clone())],
                root.clone(),
                FilesystemCapabilities::none(),
                &mut registry,
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call_upload_builtin(
                builtin_is_uploaded_file,
                vec![Value::string(
                    root.join("plain.tmp").to_string_lossy().to_string()
                )],
                root.clone(),
                FilesystemCapabilities::none(),
                &mut registry,
            ),
            Value::Bool(false)
        );
        assert!(registry.mark_moved(&upload_string));
        assert_eq!(
            call_upload_builtin(
                builtin_is_uploaded_file,
                vec![Value::string(upload_string)],
                root.clone(),
                FilesystemCapabilities::none(),
                &mut registry,
            ),
            Value::Bool(false)
        );

        let _ = std::fs::remove_file(upload);
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn move_uploaded_file_moves_to_allowed_destination() {
        let root = unique_temp_dir("move-uploaded-ok");
        std::fs::create_dir_all(&root).expect("create temp root");
        let upload = root.join("upload.tmp");
        let destination = root.join("stored.txt");
        std::fs::write(&upload, b"payload").expect("write upload");
        let upload_string = upload.to_string_lossy().to_string();
        let mut registry = UploadRegistry::from_uploaded_files(&[uploaded_file(&upload_string)]);
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);

        assert_eq!(
            call_upload_builtin(
                builtin_move_uploaded_file,
                vec![
                    Value::string(upload_string.clone()),
                    Value::string("stored.txt"),
                ],
                root.clone(),
                capabilities,
                &mut registry,
            ),
            Value::Bool(true)
        );
        assert!(!upload.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"payload");
        assert!(!registry.is_active_upload(&upload_string));
        registry.cleanup_unmoved();
        assert!(destination.exists());

        let _ = std::fs::remove_file(destination);
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn move_uploaded_file_rejects_destinations_outside_allowed_roots() {
        let root = unique_temp_dir("move-uploaded-denied-root");
        let outside = unique_temp_dir("move-uploaded-denied-outside");
        std::fs::create_dir_all(&root).expect("create temp root");
        std::fs::create_dir_all(&outside).expect("create outside root");
        let upload = root.join("upload.tmp");
        let destination = outside.join("stored.txt");
        std::fs::write(&upload, b"payload").expect("write upload");
        let upload_string = upload.to_string_lossy().to_string();
        let mut registry = UploadRegistry::from_uploaded_files(&[uploaded_file(&upload_string)]);
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);

        assert_eq!(
            call_upload_builtin(
                builtin_move_uploaded_file,
                vec![
                    Value::string(upload_string.clone()),
                    Value::string(destination.to_string_lossy().to_string()),
                ],
                root.clone(),
                capabilities,
                &mut registry,
            ),
            Value::Bool(false)
        );
        assert!(upload.exists());
        assert!(!destination.exists());
        assert!(registry.is_active_upload(&upload_string));

        registry.cleanup_unmoved();
        assert!(!upload.exists());
        let _ = std::fs::remove_dir(root);
        let _ = std::fs::remove_dir(outside);
    }

    #[test]
    fn move_uploaded_file_rejects_non_upload_local_file() {
        let root = unique_temp_dir("move-uploaded-non-upload");
        std::fs::create_dir_all(&root).expect("create temp root");
        let source = root.join("plain.txt");
        let destination = root.join("stored.txt");
        std::fs::write(&source, b"plain").expect("write plain file");
        let mut registry = UploadRegistry::default();
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);

        assert_eq!(
            call_upload_builtin(
                builtin_move_uploaded_file,
                vec![
                    Value::string(source.to_string_lossy().to_string()),
                    Value::string("stored.txt"),
                ],
                root.clone(),
                capabilities,
                &mut registry,
            ),
            Value::Bool(false)
        );
        assert!(source.exists());
        assert!(!destination.exists());

        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn symlink_creates_link_inside_allowed_roots() {
        let root = unique_temp_dir("symlink-ok");
        std::fs::create_dir_all(&root).expect("create temp root");
        let target = root.join("target.txt");
        let link = root.join("link.txt");
        std::fs::write(&target, b"payload").expect("write target");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut context =
            BuiltinContext::with_runtime(&mut output, root.clone(), capabilities, None);

        let result = builtin_symlink(
            &mut context,
            vec![
                Value::string(target.to_string_lossy().to_string()),
                Value::string("link.txt"),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("builtin should return");

        #[cfg(any(unix, windows))]
        assert_eq!(result, Value::Bool(true));
        #[cfg(any(unix, windows))]
        assert_eq!(std::fs::read_link(&link).unwrap(), target);

        let _ = std::fs::remove_file(link);
        let _ = std::fs::remove_file(target);
        let _ = std::fs::remove_dir(root);
    }

    #[cfg(unix)]
    #[test]
    fn chown_and_chgrp_warn_for_missing_paths() {
        let root = unique_temp_dir("chown-missing");
        std::fs::create_dir_all(&root).expect("create temp root");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut context =
            BuiltinContext::with_runtime(&mut output, root.clone(), capabilities, None);

        assert_eq!(
            builtin_chown(
                &mut context,
                vec![Value::string("missing.txt"), Value::Int(0)],
                RuntimeSourceSpan::default(),
            )
            .expect("builtin should return"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_chgrp(
                &mut context,
                vec![Value::string("missing.txt"), Value::Int(0)],
                RuntimeSourceSpan::default(),
            )
            .expect("builtin should return"),
            Value::Bool(false)
        );

        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 2);
        assert!(
            diagnostics[0]
                .message()
                .contains("chown(): No such file or directory")
        );
        assert!(
            diagnostics[1]
                .message()
                .contains("chgrp(): No such file or directory")
        );

        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn copy_warns_for_missing_source_path() {
        let root = unique_temp_dir("copy-missing-source");
        std::fs::create_dir_all(&root).expect("create temp root");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut context =
            BuiltinContext::with_runtime(&mut output, root.clone(), capabilities, None);

        assert_eq!(
            builtin_copy(
                &mut context,
                vec![Value::string("missing.txt"), Value::string("dest.txt")],
                RuntimeSourceSpan::default(),
            )
            .expect("builtin should return"),
            Value::Bool(false)
        );

        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message(),
            "copy(missing.txt): Failed to open stream: No such file or directory"
        );
        assert!(output.to_string_lossy().contains(
            "Warning: copy(missing.txt): Failed to open stream: No such file or directory"
        ));

        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn copy_preserves_capability_denial_for_existing_source_path() {
        let root = unique_temp_dir("copy-denied-root");
        let outside = unique_temp_dir("copy-denied-outside");
        std::fs::create_dir_all(&root).expect("create temp root");
        std::fs::create_dir_all(&outside).expect("create outside root");
        let source = outside.join("source.txt");
        std::fs::write(&source, b"payload").expect("write outside source");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::new();
        let mut context =
            BuiltinContext::with_runtime(&mut output, root.clone(), capabilities, None);

        assert_eq!(
            builtin_copy(
                &mut context,
                vec![
                    Value::string(source.to_string_lossy().to_string()),
                    Value::string("dest.txt"),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("builtin should return"),
            Value::Bool(false)
        );
        assert!(!root.join("dest.txt").exists());

        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message()
                .contains("Failed to open stream: Operation not permitted")
        );

        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_dir(outside);
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn native_tmpfile_owns_unique_backing_paths_until_close() {
        let root = unique_temp_dir("native-tmpfile");
        std::fs::create_dir_all(&root).expect("create temp root");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut resources = ResourceTable::new();

        let first = native_tmpfile(&mut resources, &root, &capabilities, &[])
            .expect("first temporary stream");
        let second = native_tmpfile(&mut resources, &root, &capabilities, &[])
            .expect("second temporary stream");
        let first_path = PathBuf::from(first.metadata().uri);
        let second_path = PathBuf::from(second.metadata().uri);
        assert_ne!(first_path, second_path);
        assert!(first_path.exists());
        assert!(second_path.exists());

        first.write_bytes(b"first").expect("write temporary stream");
        assert!(first.close());
        assert!(!first_path.exists());
        assert!(second_path.exists());

        resources.finalize_all();
        assert!(!second_path.exists());
        let _ = std::fs::remove_dir(root);
    }

    fn call_upload_builtin(
        function: fn(&mut BuiltinContext<'_>, Vec<Value>, RuntimeSourceSpan) -> BuiltinResult,
        args: Vec<Value>,
        cwd: PathBuf,
        filesystem: FilesystemCapabilities,
        registry: &mut UploadRegistry,
    ) -> Value {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::with_runtime(&mut output, cwd, filesystem, None);
        context.set_upload_registry(registry);
        function(&mut context, args, RuntimeSourceSpan::default()).expect("builtin should return")
    }

    fn uploaded_file(temp_path: &str) -> RuntimeUploadedFile {
        RuntimeUploadedFile {
            field_name: "avatar".to_string(),
            client_filename: "avatar.txt".to_string(),
            content_type: "text/plain".to_string(),
            temp_path: temp_path.to_string(),
            error: 0,
            size: 7,
        }
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("phrust-{name}-{}", std::process::id()))
    }
}
