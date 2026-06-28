//! Filesystem builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use crate::{StreamWrapperRegistry, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("basename", builtin_basename, BuiltinCompatibility::Php),
    BuiltinEntry::new("chdir", builtin_chdir, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "clearstatcache",
        builtin_clearstatcache,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("copy", builtin_copy, BuiltinCompatibility::Php),
    BuiltinEntry::new("dirname", builtin_dirname, BuiltinCompatibility::Php),
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
    BuiltinEntry::new("filemtime", builtin_filemtime, BuiltinCompatibility::Php),
    BuiltinEntry::new("filesize", builtin_filesize, BuiltinCompatibility::Php),
    BuiltinEntry::new("filetype", builtin_filetype, BuiltinCompatibility::Php),
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
    BuiltinEntry::new("tempnam", builtin_tempnam, BuiltinCompatibility::Php),
    BuiltinEntry::new("tmpfile", builtin_tmpfile, BuiltinCompatibility::Php),
    BuiltinEntry::new("touch", builtin_touch, BuiltinCompatibility::Php),
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
            array.insert(
                string_array_key("dirname"),
                Value::string(dirname.into_bytes()),
            );
            array.insert(
                string_array_key("basename"),
                Value::string(basename.into_bytes()),
            );
            if let Some(extension) = extension.clone() {
                array.insert(
                    string_array_key("extension"),
                    Value::string(extension.into_bytes()),
                );
            }
            array.insert(
                string_array_key("filename"),
                Value::string(filename.into_bytes()),
            );
            Ok(Value::Array(array))
        }
        Some(1) => Ok(Value::string(dirname.into_bytes())),
        Some(2) => Ok(Value::string(basename.into_bytes())),
        Some(4) => {
            Ok(extension.map_or(Value::string(""), |value| Value::string(value.into_bytes())))
        }
        Some(8) => Ok(Value::string(filename.into_bytes())),
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
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("file_get_contents", "one or two argument(s)"));
    }
    let path = string_arg("file_get_contents", &args[0])?.to_string_lossy();
    read_file_value(context, "file_get_contents", &path, span)
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
    let resolved = resolve_runtime_path(context, &path);
    if !context.filesystem_capabilities().allows_path(&resolved) {
        return Ok(Value::Bool(false));
    }
    Ok(fs::write(&resolved, &bytes).map_or(Value::Bool(false), |_| Value::Int(bytes.len() as i64)))
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
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("copy", &args, 2)?;
    let from = resolve_runtime_path(context, &string_arg("copy", &args[0])?.to_string_lossy());
    let to = resolve_runtime_path(context, &string_arg("copy", &args[1])?.to_string_lossy());
    if !context.filesystem_capabilities().allows_path(&from)
        || !context.filesystem_capabilities().allows_path(&to)
    {
        return Ok(Value::Bool(false));
    }
    if same_filesystem_path(&from, &to) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(fs::copy(from, to).is_ok()))
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
        context.php_warning(
            "E_PHP_UPLOAD_INVALID_SOURCE",
            "move_uploaded_file(): source is not a valid uploaded file",
            span.clone(),
        );
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
    Ok(Value::Bool(fs::create_dir(&path).is_ok()))
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
    let dir = resolve_runtime_path(context, &string_arg("tempnam", &args[0])?.to_string_lossy());
    let prefix = string_arg("tempnam", &args[1])?.to_string_lossy();
    if !context.filesystem_capabilities().allows_path(&dir) {
        return Ok(Value::Bool(false));
    }
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
        .open(resources, &path.to_string_lossy(), "c+", &cwd, &filesystem)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FilesystemCapabilities, OutputBuffer, RuntimeUploadedFile, UploadRegistry};

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
