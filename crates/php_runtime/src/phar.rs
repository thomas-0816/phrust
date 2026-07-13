//! Read-only PHAR archive support for local stream/include MVPs.

use crate::FilesystemCapabilities;
use md5::{Digest, Md5};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

const HALT_COMPILER: &[u8] = b"__HALT_COMPILER();";
const MANIFEST_FIXED_LEN: usize = 18;
const FILE_COMPRESSION_MASK: u32 = 0x0000_F000;
const PHAR_ARCHIVE_SUFFIXES: &[&str] = &[".phar.zip", ".phar.tar", ".phar"];
const TAR_BLOCK_LEN: usize = 512;
const PHAR_SIGNATURE_MAGIC: &[u8; 4] = b"GBMB";
const PHAR_SIG_MD5: u32 = 0x0001;
const PHAR_SIG_SHA1: u32 = 0x0002;
const PHAR_SIG_SHA256: u32 = 0x0003;
const PHAR_SIG_SHA512: u32 = 0x0004;
const PHAR_SIG_OPENSSL: u32 = 0x0010;
const PHAR_SIG_OPENSSL_SHA256: u32 = 0x0011;
const PHAR_SIG_OPENSSL_SHA512: u32 = 0x0012;

/// Error returned by the read-only PHAR parser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PharError {
    diagnostic_id: &'static str,
    message: String,
}

impl PharError {
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

impl fmt::Display for PharError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.diagnostic_id, self.message)
    }
}

impl std::error::Error for PharError {}

/// One uncompressed file entry inside a PHAR archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PharEntry {
    /// Manifest filename, using forward slashes.
    pub name: String,
    /// Raw file bytes.
    pub contents: Vec<u8>,
    /// Serialized entry metadata bytes from the manifest.
    pub metadata: Vec<u8>,
    /// Entry flags from the manifest.
    pub flags: u32,
}

/// Supported hash signature kind on a PHAR archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PharSignatureKind {
    /// MD5 hash signature.
    Md5,
    /// SHA-1 hash signature.
    Sha1,
    /// SHA-256 hash signature.
    Sha256,
    /// SHA-512 hash signature.
    Sha512,
}

impl PharSignatureKind {
    fn from_type(value: u32) -> Result<Self, PharError> {
        match value {
            PHAR_SIG_MD5 => Ok(Self::Md5),
            PHAR_SIG_SHA1 => Ok(Self::Sha1),
            PHAR_SIG_SHA256 => Ok(Self::Sha256),
            PHAR_SIG_SHA512 => Ok(Self::Sha512),
            PHAR_SIG_OPENSSL | PHAR_SIG_OPENSSL_SHA256 | PHAR_SIG_OPENSSL_SHA512 => {
                Err(PharError::new(
                    "E_PHP_RUNTIME_PHAR_SIGNATURE_UNSUPPORTED",
                    "OpenSSL PHAR signatures require a public key and are not supported yet",
                ))
            }
            _ => Err(PharError::new(
                "E_PHP_RUNTIME_PHAR_SIGNATURE_UNSUPPORTED",
                format!("unsupported PHAR signature type 0x{value:04x}"),
            )),
        }
    }

    fn digest_len(&self) -> usize {
        match self {
            Self::Md5 => 16,
            Self::Sha1 => 20,
            Self::Sha256 => 32,
            Self::Sha512 => 64,
        }
    }

    fn digest(&self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Self::Md5 => Md5::digest(bytes).to_vec(),
            Self::Sha1 => Sha1::digest(bytes).to_vec(),
            Self::Sha256 => Sha256::digest(bytes).to_vec(),
            Self::Sha512 => Sha512::digest(bytes).to_vec(),
        }
    }
}

/// Verified PHAR archive signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PharSignature {
    /// Signature algorithm.
    pub kind: PharSignatureKind,
    /// Stored digest bytes from the archive trailer.
    pub digest: Vec<u8>,
}

/// Parsed read-only PHAR archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PharArchive {
    /// Local archive path.
    pub path: PathBuf,
    /// Stub bytes before the manifest.
    pub stub: Vec<u8>,
    /// Alias from the manifest, when present.
    pub alias: Option<String>,
    /// Serialized archive metadata bytes from the manifest.
    pub metadata: Vec<u8>,
    /// Verified archive signature, when present.
    pub signature: Option<PharSignature>,
    entries: BTreeMap<String, PharEntry>,
}

impl PharArchive {
    /// Opens and parses a local `.phar` archive.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, PharError> {
        let path = path.into();
        let bytes = fs::read(&path).map_err(|error| {
            PharError::new(
                "E_PHP_RUNTIME_PHAR_OPEN",
                format!("{}: {error}", path.display()),
            )
        })?;
        Self::parse(path, &bytes)
    }

    /// Parses PHAR bytes from a local archive path.
    pub fn parse(path: PathBuf, bytes: &[u8]) -> Result<Self, PharError> {
        if looks_like_zip(bytes) {
            return Self::parse_zip(path, bytes);
        }
        if looks_like_tar(bytes) {
            return Self::parse_tar(path, bytes);
        }
        if let Some(halt_offset) = find_halt_offset(bytes) {
            return Self::parse_native(path, bytes, halt_offset);
        }
        Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_FORMAT",
            format!(
                "internal corruption of phar `{}` (__HALT_COMPILER(); not found)",
                path.display()
            ),
        ))
    }

    fn parse_native(path: PathBuf, bytes: &[u8], halt_offset: usize) -> Result<Self, PharError> {
        let manifest_offset = manifest_offset_after_stub(bytes, halt_offset)?;
        let mut cursor = manifest_offset;
        let manifest_len = read_u32(bytes, &mut cursor, "manifest length")? as usize;
        let manifest_end = cursor.checked_add(manifest_len).ok_or_else(|| {
            PharError::new("E_PHP_RUNTIME_PHAR_FORMAT", "PHAR manifest length overflow")
        })?;
        if manifest_len < MANIFEST_FIXED_LEN || manifest_end > bytes.len() {
            return Err(PharError::new(
                "E_PHP_RUNTIME_PHAR_FORMAT",
                "internal corruption of phar (truncated manifest header)",
            ));
        }

        let mut manifest_cursor = cursor;
        let manifest_count = read_u32(bytes, &mut manifest_cursor, "manifest entry count")?;
        if manifest_count == 0 {
            return Err(PharError::new(
                "E_PHP_RUNTIME_PHAR_FORMAT",
                "manifest claims to have zero entries",
            ));
        }
        let _api_version = read_u16_be(bytes, &mut manifest_cursor, "manifest API version")?;
        let _global_flags = read_u32(bytes, &mut manifest_cursor, "global flags")?;
        let alias_len = read_u32(bytes, &mut manifest_cursor, "alias length")? as usize;
        let alias =
            read_bytes(bytes, &mut manifest_cursor, alias_len, "alias").and_then(|value| {
                String::from_utf8(value.to_vec()).map_err(|_| {
                    PharError::new("E_PHP_RUNTIME_PHAR_FORMAT", "PHAR alias is not UTF-8")
                })
            })?;
        let metadata_len = read_u32(bytes, &mut manifest_cursor, "metadata length")? as usize;
        let metadata = read_bytes(bytes, &mut manifest_cursor, metadata_len, "metadata")?.to_vec();

        let mut contents_offset = manifest_end;
        let mut pending = Vec::new();
        for _ in 0..manifest_count {
            let name_len = read_u32(bytes, &mut manifest_cursor, "filename length")? as usize;
            if name_len == 0 {
                return Err(PharError::new(
                    "E_PHP_RUNTIME_PHAR_FORMAT",
                    "zero-length filename encountered in phar",
                ));
            }
            let name_bytes = read_bytes(bytes, &mut manifest_cursor, name_len, "filename")?;
            let name = String::from_utf8(name_bytes.to_vec()).map_err(|_| {
                PharError::new("E_PHP_RUNTIME_PHAR_FORMAT", "PHAR filename is not UTF-8")
            })?;
            let entry_name = checked_entry_name(&name)?;
            let uncompressed_size =
                read_u32(bytes, &mut manifest_cursor, "uncompressed size")? as usize;
            let _timestamp = read_u32(bytes, &mut manifest_cursor, "timestamp")?;
            let compressed_size =
                read_u32(bytes, &mut manifest_cursor, "compressed size")? as usize;
            let _crc32 = read_u32(bytes, &mut manifest_cursor, "crc32")?;
            let flags = read_u32(bytes, &mut manifest_cursor, "entry flags")?;
            let entry_metadata_len =
                read_u32(bytes, &mut manifest_cursor, "entry metadata length")? as usize;
            let entry_metadata = read_bytes(
                bytes,
                &mut manifest_cursor,
                entry_metadata_len,
                "entry metadata",
            )?
            .to_vec();
            if flags & FILE_COMPRESSION_MASK != 0 {
                return Err(PharError::new(
                    "E_PHP_RUNTIME_PHAR_COMPRESSION_GAP",
                    format!("compressed PHAR entry `{name}` is not supported"),
                ));
            }
            if compressed_size != uncompressed_size {
                return Err(PharError::new(
                    "E_PHP_RUNTIME_PHAR_FORMAT",
                    format!("PHAR entry `{name}` has inconsistent uncompressed size"),
                ));
            }
            pending.push((name, entry_name, uncompressed_size, flags, entry_metadata));
        }
        if manifest_cursor > manifest_end {
            return Err(PharError::new(
                "E_PHP_RUNTIME_PHAR_FORMAT",
                "PHAR manifest entries overrun manifest length",
            ));
        }

        let mut entries = BTreeMap::new();
        for (name, entry_name, size, flags, metadata) in pending {
            let end = contents_offset.checked_add(size).ok_or_else(|| {
                PharError::new("E_PHP_RUNTIME_PHAR_FORMAT", "PHAR entry size overflow")
            })?;
            if end > bytes.len() {
                return Err(PharError::new(
                    "E_PHP_RUNTIME_PHAR_FORMAT",
                    format!("PHAR entry `{name}` is truncated"),
                ));
            }
            entries.insert(
                entry_name,
                PharEntry {
                    name,
                    contents: bytes[contents_offset..end].to_vec(),
                    metadata,
                    flags,
                },
            );
            contents_offset = end;
        }
        let signature = parse_native_signature(path.as_path(), bytes, contents_offset)?;

        Ok(Self {
            path,
            stub: bytes[..manifest_offset].to_vec(),
            alias: (!alias.is_empty()).then_some(alias),
            metadata,
            signature,
            entries,
        })
    }

    fn parse_zip(path: PathBuf, bytes: &[u8]) -> Result<Self, PharError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
            PharError::new(
                "E_PHP_RUNTIME_PHAR_ZIP",
                format!(
                    "failed to parse zip-based PHAR `{}`: {error}",
                    path.display()
                ),
            )
        })?;
        let mut entries = BTreeMap::new();
        let mut stub = Vec::new();
        let mut alias = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned);
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).map_err(|error| {
                PharError::new(
                    "E_PHP_RUNTIME_PHAR_ZIP",
                    format!("failed to read zip PHAR entry {index}: {error}"),
                )
            })?;
            let name = file.name().to_owned();
            if file.is_dir() {
                continue;
            }
            if name == ".phar/stub.php" {
                file.read_to_end(&mut stub).map_err(|error| {
                    PharError::new(
                        "E_PHP_RUNTIME_PHAR_ZIP",
                        format!("stub read failed: {error}"),
                    )
                })?;
                continue;
            }
            if name == ".phar/alias.txt" {
                let mut alias_bytes = Vec::new();
                file.read_to_end(&mut alias_bytes).map_err(|error| {
                    PharError::new(
                        "E_PHP_RUNTIME_PHAR_ZIP",
                        format!("alias read failed: {error}"),
                    )
                })?;
                alias = Some(String::from_utf8_lossy(&alias_bytes).into_owned());
                continue;
            }
            if name.starts_with(".phar/") {
                continue;
            }
            let entry_name = checked_entry_name(&name)?;
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).map_err(|error| {
                PharError::new(
                    "E_PHP_RUNTIME_PHAR_ZIP",
                    format!("failed to read zip PHAR entry `{name}`: {error}"),
                )
            })?;
            entries.insert(
                entry_name,
                PharEntry {
                    name,
                    contents,
                    metadata: Vec::new(),
                    flags: 0,
                },
            );
        }

        Ok(Self {
            path,
            stub,
            alias: alias.filter(|value| !value.is_empty()),
            metadata: Vec::new(),
            signature: None,
            entries,
        })
    }

    fn parse_tar(path: PathBuf, bytes: &[u8]) -> Result<Self, PharError> {
        let mut offset = 0;
        let mut entries = BTreeMap::new();
        let mut stub = Vec::new();
        let mut alias = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned);
        while offset + TAR_BLOCK_LEN <= bytes.len() {
            let header = &bytes[offset..offset + TAR_BLOCK_LEN];
            if header.iter().all(|byte| *byte == 0) {
                break;
            }
            let name = tar_name(header)?;
            let size = tar_octal(&header[124..136], "size")?;
            let typeflag = header[156];
            let data_start = offset + TAR_BLOCK_LEN;
            let data_end = data_start.checked_add(size).ok_or_else(|| {
                PharError::new("E_PHP_RUNTIME_PHAR_TAR", "tar PHAR entry size overflow")
            })?;
            let padded_size = size.div_ceil(TAR_BLOCK_LEN) * TAR_BLOCK_LEN;
            let next_offset = data_start.checked_add(padded_size).ok_or_else(|| {
                PharError::new("E_PHP_RUNTIME_PHAR_TAR", "tar PHAR entry padding overflow")
            })?;
            if data_end > bytes.len() || next_offset > bytes.len() {
                return Err(PharError::new(
                    "E_PHP_RUNTIME_PHAR_TAR",
                    format!("tar PHAR entry `{name}` is truncated"),
                ));
            }
            match typeflag {
                0 | b'0' => {
                    if name == ".phar/stub.php" {
                        stub.extend_from_slice(&bytes[data_start..data_end]);
                    } else if name == ".phar/alias.txt" {
                        alias = Some(String::from_utf8_lossy(&bytes[data_start..data_end]).into());
                    } else if !name.starts_with(".phar/") {
                        let entry_name = checked_entry_name(&name)?;
                        entries.insert(
                            entry_name,
                            PharEntry {
                                name,
                                contents: bytes[data_start..data_end].to_vec(),
                                metadata: Vec::new(),
                                flags: 0,
                            },
                        );
                    }
                }
                b'5' => {}
                _ => {}
            }
            offset = next_offset;
        }
        Ok(Self {
            path,
            stub,
            alias: alias.filter(|value| !value.is_empty()),
            metadata: Vec::new(),
            signature: None,
            entries,
        })
    }

    /// Returns an entry by name, accepting leading slash and `./` variants.
    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&PharEntry> {
        self.entries.get(&normalize_entry_name(name))
    }

    /// Returns the number of manifest entries in the archive.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the archive contains no manifest entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Parsed local `phar://` URI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PharUri {
    /// Local archive path.
    pub archive_path: PathBuf,
    /// Entry path inside the archive.
    pub entry_path: String,
    /// Canonical synthetic path for source maps and include_once tracking.
    pub synthetic_path: PathBuf,
}

/// Returns whether `uri` uses the `phar://` stream wrapper.
#[must_use]
pub fn is_phar_uri(uri: &str) -> bool {
    uri.starts_with("phar://")
}

/// Parses a local `phar://archive.phar/path` URI under runtime capabilities.
pub fn parse_uri(
    uri: &str,
    cwd: &Path,
    capabilities: &FilesystemCapabilities,
) -> Result<PharUri, PharError> {
    let Some(rest) = uri.strip_prefix("phar://") else {
        return Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_URI",
            format!("not a phar:// URI: `{uri}`"),
        ));
    };
    let (archive_part, entry_part) = split_archive_and_entry(rest)?;
    let archive_path = normalize_path(&if Path::new(archive_part).is_absolute() {
        PathBuf::from(archive_part)
    } else {
        cwd.join(archive_part)
    });
    if !capabilities.allows_path(&archive_path) {
        return Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_CAPABILITY",
            format!(
                "PHAR archive `{}` is outside allowed filesystem roots",
                archive_path.display()
            ),
        ));
    }
    Ok(PharUri {
        archive_path,
        entry_path: normalize_entry_name(entry_part),
        synthetic_path: PathBuf::from(format!("phar://{}", rest)),
    })
}

/// Reads one file entry from a `phar://` URI.
pub fn read_uri(
    uri: &str,
    cwd: &Path,
    capabilities: &FilesystemCapabilities,
) -> Result<Vec<u8>, PharError> {
    let parsed = parse_uri(uri, cwd, capabilities)?;
    read_entry(&parsed.archive_path, &parsed.entry_path)
}

/// Reads one file entry from a parsed local archive path.
pub fn read_entry(archive_path: &Path, entry_path: &str) -> Result<Vec<u8>, PharError> {
    let archive = PharArchive::open(archive_path)?;
    archive
        .entry(entry_path)
        .map(|entry| entry.contents.clone())
        .ok_or_else(|| {
            PharError::new(
                "E_PHP_RUNTIME_PHAR_ENTRY_MISSING",
                format!(
                    "PHAR entry `{}` not found in `{}`",
                    entry_path,
                    archive_path.display()
                ),
            )
        })
}

fn split_archive_and_entry(rest: &str) -> Result<(&str, &str), PharError> {
    for suffix in PHAR_ARCHIVE_SUFFIXES {
        let mut search_start = 0;
        while let Some(relative_index) = rest[search_start..].find(suffix) {
            let marker_index = search_start + relative_index;
            let archive_end = marker_index + suffix.len();
            if rest.as_bytes().get(archive_end) == Some(&b'/') {
                return Ok((&rest[..archive_end], &rest[archive_end + 1..]));
            }
            search_start = archive_end;
        }
    }
    if PHAR_ARCHIVE_SUFFIXES
        .iter()
        .any(|suffix| rest.ends_with(suffix))
    {
        return Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_URI",
            "PHAR URI must name an entry inside the archive",
        ));
    }
    Err(PharError::new(
        "E_PHP_RUNTIME_PHAR_URI",
        format!("PHAR URI `{rest}` does not contain a supported .phar archive path"),
    ))
}

fn find_halt_offset(bytes: &[u8]) -> Option<usize> {
    php_source::byte_kernel::find_bytes(bytes, HALT_COMPILER)
        .map(|index| index + HALT_COMPILER.len())
}

fn manifest_offset_after_stub(bytes: &[u8], halt_offset: usize) -> Result<usize, PharError> {
    let mut offset = halt_offset;
    if bytes
        .get(offset)
        .is_some_and(|byte| *byte == b' ' || *byte == b'\n')
        && bytes.get(offset + 1) == Some(&b'?')
        && bytes.get(offset + 2) == Some(&b'>')
    {
        offset += 3;
        if bytes.get(offset) == Some(&b'\r') {
            if bytes.get(offset + 1) != Some(&b'\n') {
                return Err(PharError::new(
                    "E_PHP_RUNTIME_PHAR_FORMAT",
                    "PHAR stub has carriage return not followed by newline",
                ));
            }
            offset += 1;
        }
        if bytes.get(offset) == Some(&b'\n') {
            offset += 1;
        }
    }
    Ok(offset)
}

fn read_u32(bytes: &[u8], cursor: &mut usize, field: &str) -> Result<u32, PharError> {
    let value = read_bytes(bytes, cursor, 4, field)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u16_be(bytes: &[u8], cursor: &mut usize, field: &str) -> Result<u16, PharError> {
    let value = read_bytes(bytes, cursor, 2, field)?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_bytes<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    len: usize,
    field: &str,
) -> Result<&'a [u8], PharError> {
    let end = cursor.checked_add(len).ok_or_else(|| {
        PharError::new(
            "E_PHP_RUNTIME_PHAR_FORMAT",
            format!("PHAR {field} length overflow"),
        )
    })?;
    let Some(value) = bytes.get(*cursor..end) else {
        return Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_FORMAT",
            format!("PHAR {field} is truncated"),
        ));
    };
    *cursor = end;
    Ok(value)
}

fn parse_native_signature(
    path: &Path,
    bytes: &[u8],
    signed_region_end: usize,
) -> Result<Option<PharSignature>, PharError> {
    if signed_region_end == bytes.len() {
        return Ok(None);
    }
    if bytes.len() < signed_region_end + 8 {
        return Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_SIGNATURE",
            format!(
                "PHAR `{}` has a truncated signature trailer",
                path.display()
            ),
        ));
    }
    if bytes.get(bytes.len() - 4..) != Some(PHAR_SIGNATURE_MAGIC.as_slice()) {
        return Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_SIGNATURE",
            format!(
                "PHAR `{}` has trailing bytes without a signature marker",
                path.display()
            ),
        ));
    }
    let signature_type_offset = bytes.len() - 8;
    let signature_type = u32::from_le_bytes([
        bytes[signature_type_offset],
        bytes[signature_type_offset + 1],
        bytes[signature_type_offset + 2],
        bytes[signature_type_offset + 3],
    ]);
    let kind = PharSignatureKind::from_type(signature_type)?;
    let digest_len = kind.digest_len();
    let expected_len = signed_region_end
        .checked_add(digest_len)
        .and_then(|value| value.checked_add(8))
        .ok_or_else(|| {
            PharError::new(
                "E_PHP_RUNTIME_PHAR_SIGNATURE",
                format!("PHAR `{}` signature length overflow", path.display()),
            )
        })?;
    if expected_len != bytes.len() {
        return Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_SIGNATURE",
            format!(
                "PHAR `{}` has a malformed signature trailer",
                path.display()
            ),
        ));
    }
    let stored = &bytes[signed_region_end..signed_region_end + digest_len];
    let computed = kind.digest(&bytes[..signed_region_end]);
    if stored != computed.as_slice() {
        return Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_SIGNATURE",
            format!("PHAR `{}` has a broken signature", path.display()),
        ));
    }
    Ok(Some(PharSignature {
        kind,
        digest: stored.to_vec(),
    }))
}

fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

fn looks_like_tar(bytes: &[u8]) -> bool {
    bytes.len() >= TAR_BLOCK_LEN
        && (bytes.get(257..263) == Some(b"ustar\0") || bytes.get(257..265) == Some(b"ustar  \0"))
}

fn tar_name(header: &[u8]) -> Result<String, PharError> {
    let name = tar_string(&header[0..100]);
    let prefix = tar_string(&header[345..500]);
    let full_name = if prefix.is_empty() {
        name
    } else if name.is_empty() {
        prefix
    } else {
        format!("{prefix}/{name}")
    };
    if full_name.is_empty() {
        return Err(PharError::new(
            "E_PHP_RUNTIME_PHAR_TAR",
            "tar PHAR entry has empty filename",
        ));
    }
    Ok(full_name)
}

fn tar_string(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

fn tar_octal(bytes: &[u8], field: &str) -> Result<usize, PharError> {
    let text = tar_string(bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    usize::from_str_radix(trimmed, 8).map_err(|_| {
        PharError::new(
            "E_PHP_RUNTIME_PHAR_TAR",
            format!("tar PHAR {field} is not a valid octal field"),
        )
    })
}

fn checked_entry_name(name: &str) -> Result<String, PharError> {
    if name.contains('\0') || name.starts_with('/') || name.starts_with('\\') {
        return Err(unsafe_entry_name_error(name));
    }
    let path = Path::new(name);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(unsafe_entry_name_error(name));
    }
    let mut parts = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(unsafe_entry_name_error(name)),
            part if part.contains('\\') => return Err(unsafe_entry_name_error(name)),
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(unsafe_entry_name_error(name));
    }
    Ok(parts.join("/"))
}

fn unsafe_entry_name_error(name: &str) -> PharError {
    PharError::new(
        "E_PHP_RUNTIME_PHAR_PATH_TRAVERSAL",
        format!("unsafe PHAR entry path `{name}` is not allowed"),
    )
}

fn normalize_entry_name(name: &str) -> String {
    let mut parts = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
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
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_uncompressed_phar_entry() {
        let bytes = fixture_phar();
        let archive = PharArchive::parse(PathBuf::from("fixture.phar"), &bytes).expect("parse");

        assert_eq!(archive.alias.as_deref(), Some("fixture.phar"));
        assert_eq!(
            archive
                .entry("lib/hello.php")
                .map(|entry| entry.contents.as_slice()),
            Some(b"<?php echo 'hello';\n".as_slice())
        );
        assert_eq!(
            archive
                .entry("./data.txt")
                .map(|entry| entry.contents.as_slice()),
            Some(b"payload".as_slice())
        );
        assert!(archive.metadata.is_empty());
        assert!(archive.entry("data.txt").unwrap().metadata.is_empty());
    }

    #[test]
    fn preserves_archive_and_entry_metadata() {
        let bytes = signed_metadata_phar();
        let archive = PharArchive::parse(PathBuf::from("fixture.phar"), &bytes).expect("parse");

        assert_eq!(archive.alias.as_deref(), Some("metadata-fixture.phar"));
        assert_eq!(
            archive.signature.as_ref().map(|signature| &signature.kind),
            Some(&PharSignatureKind::Sha256)
        );
        assert_eq!(
            String::from_utf8_lossy(&archive.metadata),
            r#"a:2:{s:7:"archive";s:4:"meta";s:1:"n";i:3;}"#
        );
        assert_eq!(
            String::from_utf8_lossy(&archive.entry("data.txt").unwrap().metadata),
            r#"a:1:{s:5:"entry";s:4:"meta";}"#
        );
    }

    #[test]
    fn rejects_tampered_native_phar_signature() {
        let mut bytes = signed_metadata_phar();
        let payload_offset = bytes
            .windows(b"payload".len())
            .position(|window| window == b"payload")
            .expect("payload bytes");
        bytes[payload_offset] = b'P';

        let error = PharArchive::parse(PathBuf::from("fixture.phar"), &bytes)
            .expect_err("tampered archive rejected");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_PHAR_SIGNATURE");
        assert!(error.message().contains("broken signature"));
    }

    #[test]
    fn parses_zip_based_phar_entries() {
        let bytes = fixture_zip_phar();
        let archive = PharArchive::parse(PathBuf::from("fixture.phar.zip"), &bytes).expect("parse");

        assert_eq!(archive.alias.as_deref(), Some("fixture.phar.zip"));
        assert_eq!(
            archive
                .entry("lib/hello.php")
                .map(|entry| entry.contents.as_slice()),
            Some(b"<?php echo 'zip';\n".as_slice())
        );
        assert_eq!(
            archive
                .entry("./data.txt")
                .map(|entry| entry.contents.as_slice()),
            Some(b"zip-payload".as_slice())
        );
        assert!(archive.entry(".phar/signature.bin").is_none());
    }

    #[test]
    fn parses_tar_based_phar_entries() {
        let bytes = fixture_tar_phar();
        let archive = PharArchive::parse(PathBuf::from("fixture.phar.tar"), &bytes).expect("parse");

        assert_eq!(archive.alias.as_deref(), Some("fixture.phar.tar"));
        assert_eq!(
            archive
                .entry("lib/hello.php")
                .map(|entry| entry.contents.as_slice()),
            Some(b"<?php echo 'tar';\n".as_slice())
        );
        assert_eq!(
            archive
                .entry("./data.txt")
                .map(|entry| entry.contents.as_slice()),
            Some(b"tar-payload".as_slice())
        );
        assert!(archive.entry(".phar/signature.bin").is_none());
    }

    #[test]
    fn parses_zip_and_tar_uri_archive_suffixes() {
        assert_eq!(
            split_archive_and_entry("/tmp/app.phar.zip/data.txt").expect("zip uri"),
            ("/tmp/app.phar.zip", "data.txt")
        );
        assert_eq!(
            split_archive_and_entry("/tmp/app.phar.tar/lib/hello.php").expect("tar uri"),
            ("/tmp/app.phar.tar", "lib/hello.php")
        );
    }

    #[test]
    fn rejects_archive_entry_traversal() {
        let bytes = fixture_zip_with_entry("../evil.txt", b"evil");
        let error = PharArchive::parse(PathBuf::from("fixture.phar.zip"), &bytes)
            .expect_err("unsafe entry rejected");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_PHAR_PATH_TRAVERSAL");
    }

    fn fixture_phar() -> Vec<u8> {
        let entries = [
            ("lib/hello.php", b"<?php echo 'hello';\n".as_slice()),
            ("data.txt", b"payload".as_slice()),
        ];
        let mut bytes = b"<?php __HALT_COMPILER(); ?>\n".to_vec();
        let mut manifest = Vec::new();
        push_u32(&mut manifest, entries.len() as u32);
        manifest.extend_from_slice(&[0x11, 0x01]);
        push_u32(&mut manifest, 0);
        push_u32(&mut manifest, "fixture.phar".len() as u32);
        manifest.extend_from_slice(b"fixture.phar");
        push_u32(&mut manifest, 0);
        for (name, contents) in entries {
            push_u32(&mut manifest, name.len() as u32);
            manifest.extend_from_slice(name.as_bytes());
            push_u32(&mut manifest, contents.len() as u32);
            push_u32(&mut manifest, 1_704_067_200);
            push_u32(&mut manifest, contents.len() as u32);
            push_u32(&mut manifest, 0);
            push_u32(&mut manifest, 0);
            push_u32(&mut manifest, 0);
        }
        push_u32(&mut bytes, manifest.len() as u32);
        bytes.extend_from_slice(&manifest);
        for (_, contents) in entries {
            bytes.extend_from_slice(contents);
        }
        bytes
    }

    fn push_u32(buffer: &mut Vec<u8>, value: u32) {
        buffer.extend_from_slice(&value.to_le_bytes());
    }

    fn fixture_zip_phar() -> Vec<u8> {
        fixture_zip_with_entries(&[
            ("data.txt", b"zip-payload".as_slice()),
            ("lib/hello.php", b"<?php echo 'zip';\n".as_slice()),
            (".phar/stub.php", b"<?php __HALT_COMPILER();".as_slice()),
            (".phar/signature.bin", b"signature".as_slice()),
        ])
    }

    fn fixture_zip_with_entry(name: &str, contents: &[u8]) -> Vec<u8> {
        fixture_zip_with_entries(&[(name, contents)])
    }

    fn fixture_zip_with_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, contents) in entries {
            writer.start_file(name, options).expect("start zip file");
            writer.write_all(contents).expect("write zip file");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    fn fixture_tar_phar() -> Vec<u8> {
        let mut bytes = Vec::new();
        append_tar_file(&mut bytes, "data.txt", b"tar-payload");
        append_tar_file(&mut bytes, "lib/hello.php", b"<?php echo 'tar';\n");
        append_tar_file(&mut bytes, ".phar/stub.php", b"<?php __HALT_COMPILER();");
        append_tar_file(&mut bytes, ".phar/signature.bin", b"signature");
        bytes.extend_from_slice(&[0; TAR_BLOCK_LEN * 2]);
        bytes
    }

    fn append_tar_file(buffer: &mut Vec<u8>, name: &str, contents: &[u8]) {
        let mut header = [0u8; TAR_BLOCK_LEN];
        header[..name.len()].copy_from_slice(name.as_bytes());
        write_tar_octal(&mut header[100..108], 0o644);
        write_tar_octal(&mut header[108..116], 0);
        write_tar_octal(&mut header[116..124], 0);
        write_tar_octal(&mut header[124..136], contents.len() as u64);
        write_tar_octal(&mut header[136..148], 1_704_067_200);
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum = header.iter().map(|byte| u32::from(*byte)).sum::<u32>();
        write_tar_checksum(&mut header[148..156], checksum);

        buffer.extend_from_slice(&header);
        buffer.extend_from_slice(contents);
        let padding = (TAR_BLOCK_LEN - contents.len() % TAR_BLOCK_LEN) % TAR_BLOCK_LEN;
        buffer.extend(std::iter::repeat_n(0, padding));
    }

    fn write_tar_octal(field: &mut [u8], value: u64) {
        let text = format!("{value:0width$o}\0", width = field.len() - 1);
        field.copy_from_slice(text.as_bytes());
    }

    fn write_tar_checksum(field: &mut [u8], checksum: u32) {
        let text = format!("{checksum:06o}\0 ");
        field.copy_from_slice(text.as_bytes());
    }

    fn signed_metadata_phar() -> Vec<u8> {
        hex_fixture(
            "3c3f706870205f5f48414c545f434f4d50494c455228293b203f3e0d0abc00000002000000110000000100150000006d657461646174612d666978747572652e706861722b000000613a323a7b733a373a2261726368697665223b733a343a226d657461223b733a313a226e223b693a333b7d08000000646174612e74787407000000f93c506a07000000156a2c42a40100001d000000613a313a7b733a353a22656e747279223b733a343a226d657461223b7d0d0000006c69622f68656c6c6f2e7068702e000000f93c506a2e000000924eee49a4010000000000007061796c6f61643c3f706870206563686f202266726f6d2d706861727c223b2072657475726e2022696e636c7564652d6f6b223b0a84e76fd65c15ed5859574cf7d652aafa41b0259dc96873783289da7164e0dd0c0300000047424d42",
        )
    }

    fn hex_fixture(hex: &str) -> Vec<u8> {
        assert_eq!(hex.len() % 2, 0);
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex"))
            .collect()
    }
}
