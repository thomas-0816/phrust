//! Resource handles and stream metadata for standard-library.

use crate::{ArrayKey, PhpArray, PhpString, Value};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

/// Stable resource identifier exposed by `get_resource_id`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResourceId(u64);

impl ResourceId {
    /// Creates a resource ID from a stable integer.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the integer payload.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// PHP resource kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    /// Stream resource.
    Stream,
    /// Directory resource.
    Directory,
    /// Stream context resource.
    StreamContext,
    /// File information detector resource.
    FileInfo,
    /// Closed resource placeholder.
    Closed,
}

/// Stream capability flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StreamFlags {
    /// Stream supports reads.
    pub readable: bool,
    /// Stream supports writes.
    pub writable: bool,
    /// Stream supports seeking.
    pub seekable: bool,
}

/// Seek origin for PHP stream cursor movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamSeekWhence {
    /// Seek from the beginning of the stream.
    Set,
    /// Seek from the current stream cursor.
    Current,
    /// Seek from the end of the stream buffer.
    End,
}

impl StreamFlags {
    /// Creates stream flags.
    #[must_use]
    pub const fn new(readable: bool, writable: bool, seekable: bool) -> Self {
        Self {
            readable,
            writable,
            seekable,
        }
    }
}

/// Stable stream metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamMetadata {
    /// Wrapper name, for example `plainfile` or `php`.
    pub wrapper_type: String,
    /// Stream type returned by `get_resource_type`.
    pub stream_type: String,
    /// Open mode string.
    pub mode: String,
    /// Deterministic URI or logical stream name.
    pub uri: String,
}

impl StreamMetadata {
    /// Creates stream metadata.
    #[must_use]
    pub fn new(
        wrapper_type: impl Into<String>,
        stream_type: impl Into<String>,
        mode: impl Into<String>,
        uri: impl Into<String>,
    ) -> Self {
        Self {
            wrapper_type: wrapper_type.into(),
            stream_type: stream_type.into(),
            mode: mode.into(),
            uri: uri.into(),
        }
    }
}

/// Root-constrained filesystem capabilities for local wrappers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FilesystemCapabilities {
    allowed_roots: Vec<PathBuf>,
    allow_stdio: bool,
}

impl FilesystemCapabilities {
    /// Creates capabilities that deny host filesystem and stdio access.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            allowed_roots: Vec::new(),
            allow_stdio: false,
        }
    }

    /// Allows local filesystem access below the provided roots.
    #[must_use]
    pub fn with_allowed_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.allowed_roots = roots.into_iter().map(normalize_path).collect();
        self
    }

    /// Allows deterministic stdio pseudo streams.
    #[must_use]
    pub const fn with_stdio(mut self, allow_stdio: bool) -> Self {
        self.allow_stdio = allow_stdio;
        self
    }

    /// Returns whether a path is within an allowed local filesystem root.
    #[must_use]
    pub fn allows_path(&self, path: &Path) -> bool {
        let path = normalize_path(path);
        self.allowed_roots
            .iter()
            .any(|root| path == *root || path.starts_with(root))
    }

    /// Returns the first allowed root, used for temporary file MVPs.
    #[must_use]
    pub fn first_allowed_root(&self) -> Option<&Path> {
        self.allowed_roots.first().map(PathBuf::as_path)
    }

    fn allows_stdio(&self) -> bool {
        self.allow_stdio
    }
}

/// Error returned by stream wrapper operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamOpenError {
    diagnostic_id: &'static str,
    message: String,
}

impl StreamOpenError {
    fn new(diagnostic_id: &'static str, message: impl Into<String>) -> Self {
        Self {
            diagnostic_id,
            message: message.into(),
        }
    }

    /// Stable diagnostic ID.
    #[must_use]
    pub const fn diagnostic_id(&self) -> &'static str {
        self.diagnostic_id
    }

    /// Human-readable deterministic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for StreamOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.diagnostic_id, self.message)
    }
}

impl std::error::Error for StreamOpenError {}

/// Stream open mode after standard-library MVP parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamOpenMode {
    readable: bool,
    writable: bool,
    seekable: bool,
    truncate: bool,
    append: bool,
    exclusive: bool,
}

impl StreamOpenMode {
    /// Parses common PHP stream mode strings.
    pub fn parse(mode: &str) -> Result<Self, StreamOpenError> {
        let Some(first) = mode.as_bytes().first().copied() else {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_MODE",
                "stream mode must not be empty",
            ));
        };
        let plus = mode.as_bytes().contains(&b'+');
        let mut parsed = match first {
            b'r' => Self {
                readable: true,
                writable: plus,
                seekable: true,
                truncate: false,
                append: false,
                exclusive: false,
            },
            b'w' => Self {
                readable: plus,
                writable: true,
                seekable: true,
                truncate: true,
                append: false,
                exclusive: false,
            },
            b'a' => Self {
                readable: plus,
                writable: true,
                seekable: true,
                truncate: false,
                append: true,
                exclusive: false,
            },
            b'x' => Self {
                readable: plus,
                writable: true,
                seekable: true,
                truncate: false,
                append: false,
                exclusive: true,
            },
            b'c' => Self {
                readable: plus,
                writable: true,
                seekable: true,
                truncate: false,
                append: false,
                exclusive: false,
            },
            _ => {
                return Err(StreamOpenError::new(
                    "E_PHP_RUNTIME_STREAM_MODE",
                    format!("unsupported stream mode `{mode}`"),
                ));
            }
        };
        if mode.contains('b') || mode.contains('t') {
            parsed.seekable = true;
        }
        Ok(parsed)
    }

    /// Returns stream capability flags for this mode.
    #[must_use]
    pub const fn flags(self) -> StreamFlags {
        StreamFlags::new(self.readable, self.writable, self.seekable)
    }
}

/// Deterministic standard-library wrapper registry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StreamWrapperRegistry;

impl StreamWrapperRegistry {
    /// Creates the default wrapper registry.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Opens a URI through the standard-library wrapper MVP.
    pub fn open(
        &self,
        table: &mut ResourceTable,
        uri: &str,
        mode: &str,
        cwd: &Path,
        capabilities: &FilesystemCapabilities,
        php_input: &[u8],
    ) -> Result<ResourceRef, StreamOpenError> {
        let parsed_mode = StreamOpenMode::parse(mode)?;
        if is_remote_uri(uri) {
            return Err(StreamOpenError::new(
                "E_PHP_STREAM_WRAPPER_UNSUPPORTED",
                format!("remote stream wrapper is disabled for `{uri}`"),
            ));
        }
        if let Some(target) = uri.strip_prefix("php://") {
            return open_php_stream(table, target, mode, parsed_mode, capabilities, php_input);
        }
        if uri.starts_with("phar://") {
            return open_phar_stream(table, uri, mode, parsed_mode, cwd, capabilities);
        }
        let path = uri.strip_prefix("file://").unwrap_or(uri);
        open_file_stream(table, path, mode, parsed_mode, cwd, capabilities)
    }
}

/// Minimal stream interface for future file and `php://` wrappers.
pub trait Stream {
    /// Returns stream metadata.
    fn metadata(&self) -> StreamMetadata;
    /// Returns capability flags.
    fn flags(&self) -> StreamFlags;
    /// Returns whether the stream is closed.
    fn is_closed(&self) -> bool;
    /// Closes the stream. Returns `true` only for the first close.
    fn close(&mut self) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResourceState {
    id: ResourceId,
    kind: ResourceKind,
    flags: StreamFlags,
    metadata: StreamMetadata,
    user_closable: bool,
    data: StreamData,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StreamData {
    Memory {
        buffer: Vec<u8>,
        cursor: usize,
    },
    Stdio {
        buffer: Vec<u8>,
        cursor: usize,
    },
    File {
        path: PathBuf,
        buffer: Vec<u8>,
        cursor: usize,
    },
    GzipFile {
        path: PathBuf,
        buffer: Vec<u8>,
        cursor: usize,
    },
    Directory {
        path: PathBuf,
        entries: Vec<String>,
        cursor: usize,
    },
    Context {
        options: PhpArray,
    },
    FileInfo {
        flags: i64,
        magic_file: Option<String>,
    },
}

/// Reference-counted runtime resource handle.
#[derive(Clone, Debug)]
pub struct ResourceRef(Rc<RefCell<ResourceState>>);

impl ResourceRef {
    fn new(id: ResourceId, flags: StreamFlags, metadata: StreamMetadata, data: StreamData) -> Self {
        Self(Rc::new(RefCell::new(ResourceState {
            id,
            kind: ResourceKind::Stream,
            flags,
            metadata,
            user_closable: true,
            data,
        })))
    }

    /// Returns the resource ID.
    #[must_use]
    pub fn id(&self) -> ResourceId {
        self.0.borrow().id
    }

    /// Returns the current resource kind.
    #[must_use]
    pub fn kind(&self) -> ResourceKind {
        self.0.borrow().kind.clone()
    }

    /// Returns whether the resource is still open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.kind() != ResourceKind::Closed
    }

    /// Returns capability flags.
    #[must_use]
    pub fn flags(&self) -> StreamFlags {
        self.0.borrow().flags
    }

    /// Returns a metadata snapshot.
    #[must_use]
    pub fn metadata(&self) -> StreamMetadata {
        self.0.borrow().metadata.clone()
    }

    /// Returns the PHP resource type name.
    #[must_use]
    pub fn resource_type(&self) -> String {
        let state = self.0.borrow();
        match state.kind {
            ResourceKind::Stream => "stream".to_string(),
            ResourceKind::Directory | ResourceKind::StreamContext => {
                state.metadata.stream_type.clone()
            }
            ResourceKind::FileInfo => "file_info".to_string(),
            ResourceKind::Closed => "Unknown".to_string(),
        }
    }

    /// Returns the stored fileinfo flags and optional magic database path.
    #[must_use]
    pub fn fileinfo_options(&self) -> Option<(i64, Option<String>)> {
        let state = self.0.borrow();
        match &state.data {
            StreamData::FileInfo { flags, magic_file } => Some((*flags, magic_file.clone())),
            _ => None,
        }
    }

    /// Updates the stored fileinfo flags for a fileinfo resource.
    pub fn set_fileinfo_flags(&self, flags: i64) -> bool {
        let mut state = self.0.borrow_mut();
        match &mut state.data {
            StreamData::FileInfo {
                flags: stored_flags,
                ..
            } => {
                *stored_flags = flags;
                true
            }
            _ => false,
        }
    }

    /// Returns whether userland close functions may close this resource.
    #[must_use]
    pub fn is_user_closable(&self) -> bool {
        self.0.borrow().user_closable
    }

    /// Writes bytes into writable in-memory, temp, stdio, or file-backed buffers.
    pub fn write_bytes(&self, bytes: &[u8]) -> Result<usize, StreamOpenError> {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot write to a closed stream",
            ));
        }
        if !state.flags.writable {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_WRITABLE",
                "stream is not writable",
            ));
        }
        match &mut state.data {
            StreamData::Memory { buffer, cursor }
            | StreamData::Stdio { buffer, cursor }
            | StreamData::File { buffer, cursor, .. }
            | StreamData::GzipFile { buffer, cursor, .. } => {
                if *cursor > buffer.len() {
                    buffer.resize(*cursor, 0);
                }
                let end = cursor.saturating_add(bytes.len());
                if end > buffer.len() {
                    buffer.resize(end, 0);
                }
                buffer[*cursor..end].copy_from_slice(bytes);
                *cursor = end;
            }
            StreamData::Directory { .. }
            | StreamData::Context { .. }
            | StreamData::FileInfo { .. } => {
                return Err(StreamOpenError::new(
                    "E_PHP_RUNTIME_STREAM_NOT_WRITABLE",
                    "directory resource is not writable",
                ));
            }
        }
        flush_file_data(&state.data)?;
        Ok(bytes.len())
    }

    /// Reads up to `length` bytes from a readable stream buffer.
    pub fn read_bytes(&self, length: usize) -> Result<Vec<u8>, StreamOpenError> {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot read from a closed stream",
            ));
        }
        if !state.flags.readable {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_READABLE",
                "stream is not readable",
            ));
        }
        match &mut state.data {
            StreamData::Memory { buffer, cursor }
            | StreamData::Stdio { buffer, cursor }
            | StreamData::File { buffer, cursor, .. }
            | StreamData::GzipFile { buffer, cursor, .. } => {
                let end = (*cursor).saturating_add(length).min(buffer.len());
                let bytes = buffer[*cursor..end].to_vec();
                *cursor = end;
                Ok(bytes)
            }
            StreamData::Directory { .. }
            | StreamData::Context { .. }
            | StreamData::FileInfo { .. } => Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_READABLE",
                "directory resource is not byte-readable",
            )),
        }
    }

    /// Reads one line, including the trailing newline when present.
    pub fn read_line(&self) -> Result<Vec<u8>, StreamOpenError> {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot read from a closed stream",
            ));
        }
        if !state.flags.readable {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_READABLE",
                "stream is not readable",
            ));
        }
        match &mut state.data {
            StreamData::Memory { buffer, cursor }
            | StreamData::Stdio { buffer, cursor }
            | StreamData::File { buffer, cursor, .. }
            | StreamData::GzipFile { buffer, cursor, .. } => {
                let remaining = &buffer[*cursor..];
                let len = php_source::byte_kernel::find_byte(remaining, b'\n')
                    .map_or(remaining.len(), |index| index + 1);
                let end = *cursor + len;
                let bytes = buffer[*cursor..end].to_vec();
                *cursor = end;
                Ok(bytes)
            }
            StreamData::Directory { .. }
            | StreamData::Context { .. }
            | StreamData::FileInfo { .. } => Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_READABLE",
                "directory resource is not line-readable",
            )),
        }
    }

    /// Reads remaining bytes from a readable stream buffer.
    pub fn read_to_end(&self) -> Result<Vec<u8>, StreamOpenError> {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot read from a closed stream",
            ));
        }
        if !state.flags.readable {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_READABLE",
                "stream is not readable",
            ));
        }
        match &mut state.data {
            StreamData::Memory { buffer, cursor }
            | StreamData::Stdio { buffer, cursor }
            | StreamData::File { buffer, cursor, .. }
            | StreamData::GzipFile { buffer, cursor, .. } => {
                let bytes = buffer.get(*cursor..).unwrap_or_default().to_vec();
                *cursor = buffer.len();
                Ok(bytes)
            }
            StreamData::Directory { .. }
            | StreamData::Context { .. }
            | StreamData::FileInfo { .. } => Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_READABLE",
                "directory resource is not byte-readable",
            )),
        }
    }

    /// Rewinds seekable stream buffers to offset 0.
    pub fn rewind(&self) -> Result<(), StreamOpenError> {
        self.seek(0)
    }

    /// Moves the stream cursor to an absolute byte offset.
    pub fn seek(&self, offset: usize) -> Result<(), StreamOpenError> {
        self.seek_from(offset as i64, StreamSeekWhence::Set)
    }

    /// Moves the stream cursor relative to a PHP seek origin.
    pub fn seek_from(&self, offset: i64, whence: StreamSeekWhence) -> Result<(), StreamOpenError> {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot seek a closed stream",
            ));
        }
        if !state.flags.seekable {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_SEEKABLE",
                "stream is not seekable",
            ));
        }
        match &mut state.data {
            StreamData::Memory { buffer, cursor }
            | StreamData::Stdio { buffer, cursor }
            | StreamData::File { buffer, cursor, .. }
            | StreamData::GzipFile { buffer, cursor, .. } => {
                let base = match whence {
                    StreamSeekWhence::Set => 0_i64,
                    StreamSeekWhence::Current => (*cursor).try_into().map_err(|_| {
                        StreamOpenError::new(
                            "E_PHP_RUNTIME_STREAM_SEEK",
                            "stream cursor is outside supported range",
                        )
                    })?,
                    StreamSeekWhence::End => buffer.len().try_into().map_err(|_| {
                        StreamOpenError::new(
                            "E_PHP_RUNTIME_STREAM_SEEK",
                            "stream length is outside supported range",
                        )
                    })?,
                };
                let Some(target) = base.checked_add(offset) else {
                    return Err(StreamOpenError::new(
                        "E_PHP_RUNTIME_STREAM_SEEK",
                        "stream seek offset overflowed",
                    ));
                };
                if target < 0 {
                    return Err(StreamOpenError::new(
                        "E_PHP_RUNTIME_STREAM_SEEK",
                        "stream seek offset is negative",
                    ));
                }
                *cursor = target as usize;
            }
            StreamData::Directory { .. }
            | StreamData::Context { .. }
            | StreamData::FileInfo { .. } => {
                return Err(StreamOpenError::new(
                    "E_PHP_RUNTIME_STREAM_NOT_SEEKABLE",
                    "directory resource is not byte-seekable",
                ));
            }
        }
        Ok(())
    }

    /// Truncates writable stream buffers to `length` bytes.
    pub fn truncate(&self, length: usize) -> Result<(), StreamOpenError> {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot truncate a closed stream",
            ));
        }
        if !state.flags.writable {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_WRITABLE",
                "stream is not writable",
            ));
        }
        match &mut state.data {
            StreamData::Memory { buffer, cursor }
            | StreamData::Stdio { buffer, cursor }
            | StreamData::File { buffer, cursor, .. }
            | StreamData::GzipFile { buffer, cursor, .. } => {
                buffer.resize(length, 0);
                if *cursor > length {
                    *cursor = length;
                }
            }
            StreamData::Directory { .. }
            | StreamData::Context { .. }
            | StreamData::FileInfo { .. } => {
                return Err(StreamOpenError::new(
                    "E_PHP_RUNTIME_STREAM_NOT_WRITABLE",
                    "resource is not a writable byte stream",
                ));
            }
        }
        flush_file_data(&state.data)
    }

    /// Returns the current stream cursor.
    pub fn tell(&self) -> Result<usize, StreamOpenError> {
        let state = self.0.borrow();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot tell a closed stream",
            ));
        }
        Ok(match &state.data {
            StreamData::Memory { cursor, .. }
            | StreamData::Stdio { cursor, .. }
            | StreamData::File { cursor, .. }
            | StreamData::GzipFile { cursor, .. }
            | StreamData::Directory { cursor, .. } => *cursor,
            StreamData::Context { .. } | StreamData::FileInfo { .. } => 0,
        })
    }

    /// Returns whether the cursor is at or past the end of buffered data.
    pub fn eof(&self) -> Result<bool, StreamOpenError> {
        let state = self.0.borrow();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot inspect a closed stream",
            ));
        }
        Ok(match &state.data {
            StreamData::Memory { buffer, cursor }
            | StreamData::Stdio { buffer, cursor }
            | StreamData::File { buffer, cursor, .. }
            | StreamData::GzipFile { buffer, cursor, .. } => *cursor >= buffer.len(),
            StreamData::Directory {
                entries, cursor, ..
            } => *cursor >= entries.len(),
            StreamData::Context { .. } | StreamData::FileInfo { .. } => true,
        })
    }

    /// Reads the next directory entry name from a directory resource.
    pub fn read_dir_entry(&self) -> Result<Option<String>, StreamOpenError> {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot read from a closed directory",
            ));
        }
        match &mut state.data {
            StreamData::Directory {
                entries, cursor, ..
            } => {
                let Some(entry) = entries.get(*cursor).cloned() else {
                    return Ok(None);
                };
                *cursor += 1;
                Ok(Some(entry))
            }
            _ => Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_DIRECTORY",
                "resource is not a directory",
            )),
        }
    }

    /// Rewinds a directory resource to the first entry.
    pub fn rewind_dir(&self) -> Result<(), StreamOpenError> {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot rewind a closed directory",
            ));
        }
        match &mut state.data {
            StreamData::Directory { cursor, .. } => {
                *cursor = 0;
                Ok(())
            }
            _ => Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_DIRECTORY",
                "resource is not a directory",
            )),
        }
    }

    /// Returns stream context options when this resource is a context.
    pub fn context_options(&self) -> Option<PhpArray> {
        let state = self.0.borrow();
        match &state.data {
            StreamData::Context { options } if state.kind != ResourceKind::Closed => {
                Some(options.clone())
            }
            _ => None,
        }
    }

    /// Sets one stream context option while preserving unknown wrappers/options.
    pub fn set_context_option(
        &self,
        wrapper: impl Into<String>,
        option: impl Into<String>,
        value: Value,
    ) -> Result<(), StreamOpenError> {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_CLOSED",
                "cannot update a closed stream context",
            ));
        }
        let StreamData::Context { options } = &mut state.data else {
            return Err(StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_NOT_CONTEXT",
                "resource is not a stream context",
            ));
        };
        let wrapper = wrapper.into();
        let option = option.into();
        let wrapper_key = ArrayKey::String(PhpString::from_test_str(&wrapper));
        let option_key = ArrayKey::String(PhpString::from_test_str(&option));
        let mut wrapper_options = match options.get(&wrapper_key) {
            Some(Value::Array(array)) => array.clone(),
            _ => PhpArray::new(),
        };
        wrapper_options.insert(option_key, value);
        options.insert(wrapper_key, Value::Array(wrapper_options));
        Ok(())
    }

    /// Flushes file-backed buffers to disk. Memory and stdio buffers are no-ops.
    pub fn flush(&self) -> Result<(), StreamOpenError> {
        let state = self.0.borrow();
        flush_file_data(&state.data)
    }

    /// Closes the resource. Returns `true` only for the first close.
    pub fn close(&self) -> bool {
        let mut state = self.0.borrow_mut();
        if state.kind == ResourceKind::Closed {
            return false;
        }
        let _ = flush_file_data(&state.data);
        state.kind = ResourceKind::Closed;
        true
    }
}

impl PartialEq for ResourceRef {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for ResourceRef {}

impl Stream for ResourceRef {
    fn metadata(&self) -> StreamMetadata {
        self.metadata()
    }

    fn flags(&self) -> StreamFlags {
        self.flags()
    }

    fn is_closed(&self) -> bool {
        !self.is_open()
    }

    fn close(&mut self) -> bool {
        ResourceRef::close(self)
    }
}

/// Request-local table for resource handles.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceTable {
    next_id: u64,
    resources: BTreeMap<ResourceId, ResourceRef>,
}

impl ResourceTable {
    /// Creates an empty resource table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_id: 1,
            resources: BTreeMap::new(),
        }
    }

    /// Registers a stream resource.
    pub fn register_stream(&mut self, flags: StreamFlags, metadata: StreamMetadata) -> ResourceRef {
        self.register_stream_data(
            flags,
            metadata,
            StreamData::Memory {
                buffer: Vec::new(),
                cursor: 0,
            },
        )
    }

    /// Registers a deterministic CLI stdin stream.
    pub fn register_stdin(&mut self, buffer: Vec<u8>) -> ResourceRef {
        self.register_stream_data(
            StreamFlags::new(true, false, false),
            StreamMetadata::new("PHP", "stream", "r", "php://stdin"),
            StreamData::Stdio { buffer, cursor: 0 },
        )
    }

    /// Registers a deterministic CLI stdout stream.
    pub fn register_stdout(&mut self) -> ResourceRef {
        self.register_stream_data(
            StreamFlags::new(false, true, false),
            StreamMetadata::new("PHP", "stream", "w", "php://stdout"),
            StreamData::Stdio {
                buffer: Vec::new(),
                cursor: 0,
            },
        )
    }

    /// Registers a deterministic CLI stderr stream.
    pub fn register_stderr(&mut self) -> ResourceRef {
        self.register_stream_data(
            StreamFlags::new(false, true, false),
            StreamMetadata::new("PHP", "stream", "w", "php://stderr"),
            StreamData::Stdio {
                buffer: Vec::new(),
                cursor: 0,
            },
        )
    }

    fn register_stream_data(
        &mut self,
        flags: StreamFlags,
        metadata: StreamMetadata,
        data: StreamData,
    ) -> ResourceRef {
        let id = ResourceId::new(self.next_id);
        self.next_id += 1;
        let resource = ResourceRef::new(id, flags, metadata, data);
        self.resources.insert(id, resource.clone());
        resource
    }

    /// Registers a deterministic directory resource.
    pub fn register_directory(
        &mut self,
        path: PathBuf,
        entries: Vec<String>,
        uri: impl Into<String>,
    ) -> ResourceRef {
        let id = ResourceId::new(self.next_id);
        self.next_id += 1;
        let resource = ResourceRef(Rc::new(RefCell::new(ResourceState {
            id,
            kind: ResourceKind::Directory,
            flags: StreamFlags::new(true, false, true),
            metadata: StreamMetadata::new("plainfile", "stream", "r", uri),
            user_closable: true,
            data: StreamData::Directory {
                path,
                entries,
                cursor: 0,
            },
        })));
        self.resources.insert(id, resource.clone());
        resource
    }

    /// Registers a stream context resource.
    pub fn register_stream_context(&mut self, options: PhpArray) -> ResourceRef {
        let id = ResourceId::new(self.next_id);
        self.next_id += 1;
        let resource = ResourceRef(Rc::new(RefCell::new(ResourceState {
            id,
            kind: ResourceKind::StreamContext,
            flags: StreamFlags::new(false, false, false),
            metadata: StreamMetadata::new("PHP", "stream-context", "", "stream-context"),
            user_closable: true,
            data: StreamData::Context { options },
        })));
        self.resources.insert(id, resource.clone());
        resource
    }

    /// Registers a fileinfo detector resource.
    pub fn register_fileinfo(&mut self, flags: i64, magic_file: Option<String>) -> ResourceRef {
        let id = ResourceId::new(self.next_id);
        self.next_id += 1;
        let resource = ResourceRef(Rc::new(RefCell::new(ResourceState {
            id,
            kind: ResourceKind::FileInfo,
            flags: StreamFlags::new(false, false, false),
            metadata: StreamMetadata::new("fileinfo", "file_info", "", "fileinfo"),
            user_closable: true,
            data: StreamData::FileInfo { flags, magic_file },
        })));
        self.resources.insert(id, resource.clone());
        resource
    }

    /// Registers an internal glob resource that is closed by owner cleanup.
    pub fn register_internal_glob(&mut self, pattern: impl Into<String>) -> ResourceRef {
        let id = ResourceId::new(self.next_id);
        self.next_id += 1;
        let resource = ResourceRef(Rc::new(RefCell::new(ResourceState {
            id,
            kind: ResourceKind::Stream,
            flags: StreamFlags::new(true, false, false),
            metadata: StreamMetadata::new("glob", "stream", "r", pattern),
            user_closable: false,
            data: StreamData::Memory {
                buffer: Vec::new(),
                cursor: 0,
            },
        })));
        self.resources.insert(id, resource.clone());
        resource
    }

    /// Registers a gzip file resource backed by decompressed bytes.
    pub fn register_gzip_file(
        &mut self,
        path: PathBuf,
        mode: impl Into<String>,
        flags: StreamFlags,
        buffer: Vec<u8>,
        cursor: usize,
    ) -> ResourceRef {
        self.register_stream_data(
            flags,
            StreamMetadata::new("zlib", "ZLIB", mode, path.to_string_lossy()),
            StreamData::GzipFile {
                path,
                buffer,
                cursor,
            },
        )
    }

    /// Looks up a resource by ID.
    #[must_use]
    pub fn get(&self, id: ResourceId) -> Option<ResourceRef> {
        self.resources.get(&id).cloned()
    }

    /// Returns all resources in stable resource-id order.
    #[must_use]
    pub fn resources(&self) -> Vec<ResourceRef> {
        self.resources.values().cloned().collect()
    }

    /// Closes a resource by ID. Double-close is safe and returns `false`.
    pub fn close(&mut self, id: ResourceId) -> bool {
        self.get(id).is_some_and(|resource| resource.close())
    }

    /// Closes all resources. Safe to call repeatedly during finalization.
    pub fn finalize_all(&mut self) {
        for resource in self.resources.values() {
            let _ = resource.close();
        }
    }

    /// Returns the number of registered handles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.resources.len()
    }

    /// Returns whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }
}

fn open_php_stream(
    table: &mut ResourceTable,
    target: &str,
    mode: &str,
    parsed_mode: StreamOpenMode,
    capabilities: &FilesystemCapabilities,
    php_input: &[u8],
) -> Result<ResourceRef, StreamOpenError> {
    match target {
        "memory" | "temp" => Ok(table.register_stream_data(
            parsed_mode.flags(),
            StreamMetadata::new(
                "PHP",
                target.to_ascii_uppercase(),
                php_memory_stream_metadata_mode(mode),
                format!("php://{target}"),
            ),
            StreamData::Memory {
                buffer: Vec::new(),
                cursor: 0,
            },
        )),
        "input" => {
            if parsed_mode.writable {
                return Err(StreamOpenError::new(
                    "E_PHP_RUNTIME_STREAM_NOT_WRITABLE",
                    "php://input is read-only",
                ));
            }
            Ok(table.register_stream_data(
                StreamFlags::new(true, false, true),
                StreamMetadata::new("PHP", "stream", "r", "php://input"),
                StreamData::Memory {
                    buffer: php_input.to_vec(),
                    cursor: 0,
                },
            ))
        }
        "stdin" => {
            if !capabilities.allows_stdio() {
                return Err(StreamOpenError::new(
                    "E_PHP_FILESYSTEM_CAPABILITY_DENIED",
                    "php://stdin is disabled by runtime capabilities",
                ));
            }
            Ok(table.register_stream_data(
                StreamFlags::new(true, false, false),
                StreamMetadata::new("PHP", "stream", "r", "php://stdin"),
                StreamData::Stdio {
                    buffer: Vec::new(),
                    cursor: 0,
                },
            ))
        }
        "stdout" | "stderr" => {
            if !capabilities.allows_stdio() {
                return Err(StreamOpenError::new(
                    "E_PHP_FILESYSTEM_CAPABILITY_DENIED",
                    format!("php://{target} is disabled by runtime capabilities"),
                ));
            }
            Ok(table.register_stream_data(
                StreamFlags::new(false, true, false),
                StreamMetadata::new("PHP", "stream", "w", format!("php://{target}")),
                StreamData::Stdio {
                    buffer: Vec::new(),
                    cursor: 0,
                },
            ))
        }
        _ => Err(StreamOpenError::new(
            "E_PHP_STREAM_WRAPPER_UNSUPPORTED",
            format!("unsupported php:// wrapper `{target}`"),
        )),
    }
}

fn php_memory_stream_metadata_mode(mode: &str) -> String {
    if mode.contains('b') || mode.contains('t') {
        return mode.to_string();
    }

    format!("{mode}b")
}

fn open_file_stream(
    table: &mut ResourceTable,
    path: &str,
    mode: &str,
    parsed_mode: StreamOpenMode,
    cwd: &Path,
    capabilities: &FilesystemCapabilities,
) -> Result<ResourceRef, StreamOpenError> {
    let absolute = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        cwd.join(path)
    };
    let normalized = normalize_path(&absolute);
    if !capabilities.allows_path(&normalized) {
        return Err(StreamOpenError::new(
            "E_PHP_FILESYSTEM_CAPABILITY_DENIED",
            format!(
                "local file stream `{}` is outside allowed filesystem roots",
                normalized.display()
            ),
        ));
    }
    if parsed_mode.exclusive && normalized.exists() {
        return Err(StreamOpenError::new(
            "E_PHP_RUNTIME_STREAM_OPEN",
            format!("file `{}` already exists", normalized.display()),
        ));
    }
    if parsed_mode.truncate {
        std::fs::write(&normalized, []).map_err(|error| {
            StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_OPEN",
                format!("Failed to open stream: {}", php_io_error_message(&error)),
            )
        })?;
    } else if parsed_mode.writable && !normalized.exists() {
        std::fs::write(&normalized, []).map_err(|error| {
            StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_OPEN",
                format!("Failed to open stream: {}", php_io_error_message(&error)),
            )
        })?;
    }
    let buffer = if parsed_mode.truncate {
        Vec::new()
    } else {
        std::fs::read(&normalized).map_err(|error| {
            StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_OPEN",
                format!("Failed to open stream: {}", php_io_error_message(&error)),
            )
        })?
    };
    let cursor = if parsed_mode.append { buffer.len() } else { 0 };
    Ok(table.register_stream_data(
        parsed_mode.flags(),
        StreamMetadata::new("plainfile", "STDIO", mode, normalized.to_string_lossy()),
        StreamData::File {
            path: normalized,
            buffer,
            cursor,
        },
    ))
}

fn open_phar_stream(
    table: &mut ResourceTable,
    uri: &str,
    mode: &str,
    parsed_mode: StreamOpenMode,
    cwd: &Path,
    capabilities: &FilesystemCapabilities,
) -> Result<ResourceRef, StreamOpenError> {
    if parsed_mode.writable {
        return Err(StreamOpenError::new(
            "E_PHP_RUNTIME_PHAR_READONLY",
            format!("phar:// streams are read-only in the current MVP: `{uri}`"),
        ));
    }
    let bytes = crate::phar::read_uri(uri, cwd, capabilities)
        .map_err(|error| StreamOpenError::new(error.diagnostic_id(), error.message().to_owned()))?;
    Ok(table.register_stream_data(
        StreamFlags::new(true, false, true),
        StreamMetadata::new("phar", "stream", mode, uri),
        StreamData::Memory {
            buffer: bytes,
            cursor: 0,
        },
    ))
}

fn php_io_error_message(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "No such file or directory".to_string(),
        io::ErrorKind::PermissionDenied => "Permission denied".to_string(),
        io::ErrorKind::AlreadyExists => "File exists".to_string(),
        _ => error.to_string(),
    }
}

fn flush_file_data(data: &StreamData) -> Result<(), StreamOpenError> {
    if let StreamData::File { path, buffer, .. } = data {
        std::fs::write(path, buffer).map_err(|error| {
            StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_FLUSH",
                format!("failed to flush `{}`: {error}", path.display()),
            )
        })?;
    }
    if let StreamData::GzipFile { path, buffer, .. } = data {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(buffer).map_err(|error| {
            StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_FLUSH",
                format!("failed to encode gzip `{}`: {error}", path.display()),
            )
        })?;
        let encoded = encoder.finish().map_err(|error| {
            StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_FLUSH",
                format!("failed to finish gzip `{}`: {error}", path.display()),
            )
        })?;
        std::fs::write(path, encoded).map_err(|error| {
            StreamOpenError::new(
                "E_PHP_RUNTIME_STREAM_FLUSH",
                format!("failed to flush `{}`: {error}", path.display()),
            )
        })?;
    }
    Ok(())
}

/// Decodes gzip bytes for gzip-backed resources.
pub fn decode_gzip_bytes(bytes: &[u8]) -> Result<Vec<u8>, StreamOpenError> {
    let mut decoder = GzDecoder::new(bytes);
    let mut output = Vec::new();
    decoder.read_to_end(&mut output).map_err(|error| {
        StreamOpenError::new(
            "E_PHP_RUNTIME_GZIP_DECODE",
            format!("failed to decode gzip stream: {error}"),
        )
    })?;
    Ok(output)
}

fn is_remote_uri(uri: &str) -> bool {
    matches!(
        uri.split_once("://").map(|(scheme, _)| scheme),
        Some("http" | "https" | "ftp" | "ftps")
    )
}

fn normalize_path(path: impl AsRef<Path>) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{
        FilesystemCapabilities, ResourceKind, ResourceTable, StreamFlags, StreamMetadata,
        StreamSeekWhence, StreamWrapperRegistry,
    };
    use std::path::Path;

    #[test]
    fn resource_table_allocates_stable_stream_handles() {
        let mut table = ResourceTable::new();
        let resource = table.register_stream(
            StreamFlags::new(true, false, true),
            StreamMetadata::new("plainfile", "stream", "r", "/tmp/example.php"),
        );

        assert_eq!(resource.id().get(), 1);
        assert_eq!(resource.kind(), ResourceKind::Stream);
        assert!(resource.is_open());
        assert_eq!(resource.resource_type(), "stream");
        assert_eq!(resource.flags(), StreamFlags::new(true, false, true));
        assert_eq!(resource.metadata().uri, "/tmp/example.php");
        assert_eq!(table.get(resource.id()), Some(resource));
    }

    #[test]
    fn close_and_finalization_are_idempotent() {
        let mut table = ResourceTable::new();
        let first = table.register_stream(
            StreamFlags::new(true, true, true),
            StreamMetadata::new("php", "stream", "w+", "php://memory"),
        );
        let second = table.register_stream(
            StreamFlags::new(false, true, false),
            StreamMetadata::new("php", "stream", "w", "php://output"),
        );

        assert!(table.close(first.id()));
        assert!(!table.close(first.id()));
        assert_eq!(first.kind(), ResourceKind::Closed);
        assert_eq!(first.resource_type(), "Unknown");

        table.finalize_all();
        table.finalize_all();
        assert_eq!(second.kind(), ResourceKind::Closed);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn php_memory_and_temp_streams_are_readable_and_writable() {
        let registry = StreamWrapperRegistry::new();
        let capabilities = FilesystemCapabilities::none();
        let mut table = ResourceTable::new();

        for uri in ["php://memory", "php://temp"] {
            let resource = registry
                .open(&mut table, uri, "w+", Path::new("."), &capabilities, &[])
                .expect("php memory/temp opens");
            assert_eq!(resource.metadata().wrapper_type, "PHP");
            assert_eq!(resource.metadata().mode, "w+b");
            assert_eq!(resource.flags(), StreamFlags::new(true, true, true));
            assert_eq!(resource.write_bytes(b"stdlib").expect("write"), 6);
            resource.rewind().expect("rewind");
            assert_eq!(resource.read_to_end().expect("read"), b"stdlib");
        }
    }

    #[test]
    fn php_input_stream_reads_request_body() {
        let registry = StreamWrapperRegistry::new();
        let capabilities = FilesystemCapabilities::none();
        let mut table = ResourceTable::new();

        let resource = registry
            .open(
                &mut table,
                "php://input",
                "rb",
                Path::new("."),
                &capabilities,
                b"name=phrust",
            )
            .expect("php input opens");

        assert_eq!(resource.read_to_end().expect("read input"), b"name=phrust");
        assert_eq!(resource.flags(), StreamFlags::new(true, false, true));
    }

    #[test]
    fn stream_seek_supports_set_current_and_end_origins() {
        let registry = StreamWrapperRegistry::new();
        let capabilities = FilesystemCapabilities::none();
        let mut table = ResourceTable::new();
        let resource = registry
            .open(
                &mut table,
                "php://memory",
                "w+",
                Path::new("."),
                &capabilities,
                &[],
            )
            .expect("php memory opens");

        resource.write_bytes(b"abcdef").expect("write stream");
        resource
            .seek_from(-2, StreamSeekWhence::End)
            .expect("seek end");
        assert_eq!(resource.tell().expect("tell after end seek"), 4);
        assert_eq!(resource.read_bytes(1).expect("read after end seek"), b"e");

        resource
            .seek_from(-3, StreamSeekWhence::Current)
            .expect("seek current");
        assert_eq!(resource.tell().expect("tell after current seek"), 2);
        assert!(resource.seek_from(-1, StreamSeekWhence::Set).is_err());
        assert_eq!(resource.tell().expect("failed seek does not move"), 2);

        resource
            .seek_from(10, StreamSeekWhence::End)
            .expect("seek beyond end");
        assert_eq!(resource.tell().expect("tell after beyond end"), 16);
    }

    #[test]
    fn local_file_wrapper_is_constrained_to_allowed_roots() {
        let root =
            std::env::temp_dir().join(format!("phrust-stdlib-streams-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temp root");
        let file = root.join("fixture.txt");
        std::fs::write(&file, b"fixture").expect("write fixture");

        let registry = StreamWrapperRegistry::new();
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut table = ResourceTable::new();

        let implicit = registry
            .open(&mut table, "fixture.txt", "r", &root, &capabilities, &[])
            .expect("implicit file wrapper opens inside root");
        assert_eq!(implicit.metadata().wrapper_type, "plainfile");
        assert_eq!(implicit.read_to_end().expect("read file"), b"fixture");

        let explicit = registry
            .open(
                &mut table,
                &format!("file://{}", file.display()),
                "r",
                Path::new("."),
                &capabilities,
                &[],
            )
            .expect("file:// wrapper opens inside root");
        assert_eq!(explicit.read_to_end().expect("read file://"), b"fixture");

        let outside = root
            .parent()
            .expect("temp root has parent")
            .join("phrust-stdlib-outside.txt");
        std::fs::write(&outside, b"outside").expect("write outside fixture");
        let error = registry
            .open(
                &mut table,
                &outside.to_string_lossy(),
                "r",
                Path::new("."),
                &capabilities,
                &[],
            )
            .expect_err("outside root is rejected");
        assert_eq!(error.diagnostic_id(), "E_PHP_FILESYSTEM_CAPABILITY_DENIED");

        let _ = std::fs::remove_file(file);
        let _ = std::fs::remove_file(outside);
        let _ = std::fs::remove_dir(root);
    }

    #[test]
    fn remote_and_stdio_wrappers_are_capability_checked() {
        let registry = StreamWrapperRegistry::new();
        let mut table = ResourceTable::new();
        let capabilities = FilesystemCapabilities::none();

        let remote = registry
            .open(
                &mut table,
                "https://example.test/file.txt",
                "r",
                Path::new("."),
                &capabilities,
                &[],
            )
            .expect_err("remote streams are disabled");
        assert_eq!(remote.diagnostic_id(), "E_PHP_STREAM_WRAPPER_UNSUPPORTED");

        let stdio = registry
            .open(
                &mut table,
                "php://stdout",
                "w",
                Path::new("."),
                &capabilities,
                &[],
            )
            .expect_err("stdio is disabled without capability");
        assert_eq!(stdio.diagnostic_id(), "E_PHP_FILESYSTEM_CAPABILITY_DENIED");

        let stdout = registry
            .open(
                &mut table,
                "php://stdout",
                "w",
                Path::new("."),
                &FilesystemCapabilities::none().with_stdio(true),
                &[],
            )
            .expect("stdio opens with explicit capability");
        assert_eq!(stdout.flags(), StreamFlags::new(false, true, false));
    }
}
