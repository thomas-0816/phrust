//! Filesystem builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use crate::{StreamWrapperRegistry, Value};
#[cfg(unix)]
use nix::unistd::{Gid, Group, Uid, User, chown};
#[cfg(unix)]
use std::ffi::CString;
use std::fs;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

const FILE_APPEND_FLAG: i64 = 8;

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
    let path = string_arg("basename", &args[0])?.to_string_lossy();
    let suffix = args
        .get(1)
        .map(|value| string_arg("basename", value).map(|value| value.to_string_lossy()))
        .transpose()?;
    let mut base = php_basename(&path);
    if let Some(suffix) = suffix
        && !suffix.is_empty()
        && base.len() > suffix.len()
        && base.ends_with(&suffix)
    {
        base.truncate(base.len() - suffix.len());
    }
    Ok(Value::string(base.into_bytes()))
}

pub(in crate::builtins::modules) fn builtin_dirname(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("dirname", "one or two argument(s)"));
    }
    let mut path = string_arg("dirname", &args[0])?.to_string_lossy();
    let levels = args
        .get(1)
        .map(|value| int_arg("dirname", value))
        .transpose()?
        .unwrap_or(1)
        .max(1);
    for _ in 0..levels {
        path = php_dirname_once(&path);
    }
    Ok(Value::string(path.into_bytes()))
}

pub(in crate::builtins::modules) fn builtin_pathinfo(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("pathinfo", "one or two argument(s)"));
    }
    let path = string_arg("pathinfo", &args[0])?.to_string_lossy();
    let flags = args
        .get(1)
        .map(|value| int_arg("pathinfo", value))
        .transpose()?;
    let dirname = php_dirname_once(&path);
    let basename = php_basename(&path);
    let (filename, extension) = split_extension(&basename);
    match flags {
        None => {
            let mut array = crate::PhpArray::new();
            if !dirname.is_empty() {
                array.insert(
                    string_array_key("dirname"),
                    Value::string(dirname.as_bytes().to_vec()),
                );
            }
            array.insert(
                string_array_key("basename"),
                Value::string(basename.as_bytes().to_vec()),
            );
            if let Some(extension) = extension.clone() {
                array.insert(
                    string_array_key("extension"),
                    Value::string(extension.into_bytes()),
                );
            }
            array.insert(
                string_array_key("filename"),
                Value::string(filename.as_bytes().to_vec()),
            );
            Ok(Value::Array(array))
        }
        Some(flags) if flags & 1 != 0 => Ok(Value::string(dirname.into_bytes())),
        Some(flags) if flags & 2 != 0 => Ok(Value::string(basename.into_bytes())),
        Some(flags) if flags & 4 != 0 => {
            Ok(extension.map_or(Value::string(""), |value| Value::string(value.into_bytes())))
        }
        Some(flags) if flags & 8 != 0 => Ok(Value::string(filename.into_bytes())),
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
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("sys_get_temp_dir", &args, 0)?;
    let path = context
        .filesystem_capabilities()
        .first_allowed_root()
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);
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
    let Some(root) = context.filesystem_capabilities().first_allowed_root() else {
        return Ok(Value::Bool(false));
    };
    let path = root.join(format!("phrust-tmpfile-{}", std::process::id()));
    let _ = fs::write(&path, []);
    let cwd = context.cwd().to_path_buf();
    let filesystem = context.filesystem_capabilities().clone();
    let Some(resources) = context.resources() else {
        return Ok(Value::Bool(false));
    };
    Ok(StreamWrapperRegistry::new()
        .open(
            resources,
            &path.to_string_lossy(),
            "c+",
            &cwd,
            &filesystem,
            &[],
        )
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
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("chdir", &args, 1)?;
    let path = resolve_runtime_path(context, &string_arg("chdir", &args[0])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&path) || !path.is_dir() {
        return Ok(Value::Bool(false));
    }
    context.set_cwd(path);
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
    use crate::{FilesystemCapabilities, OutputBuffer, RuntimeUploadedFile, UploadRegistry};

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
