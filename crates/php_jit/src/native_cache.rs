//! Restart-persistent validated native machine-code artifacts.

use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const PNA_MAGIC: [u8; 4] = *b"PNA1";
pub const PNA_FORMAT_VERSION: u16 = 1;
const HEADER_LEN: usize = 64;
const SECTION_RECORD_LEN: usize = 32;
const MAX_SECTIONS: usize = 32;
const DEFAULT_MAX_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_MAX_RELOCATIONS: usize = 65_536;
const DEFAULT_MAX_CODE_BYTES: usize = 32 * 1024 * 1024;
const LOCK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NativeCacheMode {
    #[default]
    Off,
    Read,
    Write,
    ReadWrite,
}

impl NativeCacheMode {
    #[must_use]
    pub const fn can_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    #[must_use]
    pub const fn can_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Read => "read",
            Self::Write => "write",
            Self::ReadWrite => "read-write",
        }
    }
}

impl FromStr for NativeCacheMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            "read-write" | "read_write" | "readwrite" => Ok(Self::ReadWrite),
            _ => Err(format!(
                "invalid native cache mode `{value}`; expected off, read, write, or read-write"
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCacheConfig {
    pub mode: NativeCacheMode,
    pub directory: PathBuf,
    pub max_artifact_bytes: usize,
    pub max_cache_bytes: u64,
    pub max_relocations: usize,
    pub max_code_bytes: usize,
}

impl Default for NativeCacheConfig {
    fn default() -> Self {
        Self {
            mode: std::env::var("PHRUST_NATIVE_CACHE")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(NativeCacheMode::Off),
            directory: std::env::var_os("PHRUST_NATIVE_CACHE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("phrust-native-cache")),
            max_artifact_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
            max_cache_bytes: DEFAULT_MAX_CACHE_BYTES,
            max_relocations: DEFAULT_MAX_RELOCATIONS,
            max_code_bytes: DEFAULT_MAX_CODE_BYTES,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeCacheIdentity {
    pub source_hash: String,
    pub ir_hash: String,
    pub dependency_graph_hash: String,
    pub build_id: String,
    pub cranelift_version: String,
    pub cranelift_settings_hash: u64,
    pub region_ir_schema_version: u32,
    pub runtime_abi_hash: u64,
    pub helper_abi_hash: u64,
    pub target_triple: String,
    pub pointer_width: u8,
    pub cpu_feature_fingerprint: u64,
    pub optimization_tier: String,
    pub optimization_config_hash: u64,
    pub php_semantic_config_hash: u64,
}

impl NativeCacheIdentity {
    #[must_use]
    pub fn cache_key(&self) -> String {
        hex_digest(&self.encode())
    }

    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for value in [
            self.source_hash.as_str(),
            self.ir_hash.as_str(),
            self.dependency_graph_hash.as_str(),
            self.build_id.as_str(),
            self.cranelift_version.as_str(),
            self.target_triple.as_str(),
            self.optimization_tier.as_str(),
        ] {
            put_string(&mut out, value);
        }
        put_u64(&mut out, self.cranelift_settings_hash);
        put_u32(&mut out, self.region_ir_schema_version);
        put_u64(&mut out, self.runtime_abi_hash);
        put_u64(&mut out, self.helper_abi_hash);
        out.push(self.pointer_width);
        put_u64(&mut out, self.cpu_feature_fingerprint);
        put_u64(&mut out, self.optimization_config_hash);
        put_u64(&mut out, self.php_semantic_config_hash);
        out
    }

    fn decode(bytes: &[u8]) -> Result<Self, NativeCacheError> {
        let mut cursor = Cursor::new(bytes);
        let source_hash = cursor.string()?;
        let ir_hash = cursor.string()?;
        let dependency_graph_hash = cursor.string()?;
        let build_id = cursor.string()?;
        let cranelift_version = cursor.string()?;
        let target_triple = cursor.string()?;
        let optimization_tier = cursor.string()?;
        let cranelift_settings_hash = cursor.u64()?;
        let region_ir_schema_version = cursor.u32()?;
        let runtime_abi_hash = cursor.u64()?;
        let helper_abi_hash = cursor.u64()?;
        let pointer_width = cursor.u8()?;
        let cpu_feature_fingerprint = cursor.u64()?;
        let optimization_config_hash = cursor.u64()?;
        let php_semantic_config_hash = cursor.u64()?;
        cursor.finish()?;
        Ok(Self {
            source_hash,
            ir_hash,
            dependency_graph_hash,
            build_id,
            cranelift_version,
            cranelift_settings_hash,
            region_ir_schema_version,
            runtime_abi_hash,
            helper_abi_hash,
            target_triple,
            pointer_width,
            cpu_feature_fingerprint,
            optimization_tier,
            optimization_config_hash,
            php_semantic_config_hash,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFunctionImage {
    pub function_id: u32,
    pub code_offset: u64,
    pub code_len: u64,
    pub arity: u8,
    pub abi: NativeFunctionAbi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NativeFunctionAbi {
    I64StatusOut = 1,
    PackedI64StatusOut = 2,
}

impl NativeFunctionAbi {
    fn from_raw(raw: u8) -> Result<Self, NativeCacheError> {
        match raw {
            1 => Ok(Self::I64StatusOut),
            2 => Ok(Self::PackedI64StatusOut),
            _ => Err(NativeCacheError::InvalidHeader(format!(
                "unknown native function ABI {raw}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum NativeRelocationKind {
    Abs64 = 1,
    X86PcRel4 = 2,
    X86CallPcRel4 = 3,
    Arm64Call = 4,
}

impl NativeRelocationKind {
    fn from_raw(raw: u16) -> Result<Self, NativeCacheError> {
        match raw {
            1 => Ok(Self::Abs64),
            2 => Ok(Self::X86PcRel4),
            3 => Ok(Self::X86CallPcRel4),
            4 => Ok(Self::Arm64Call),
            _ => Err(NativeCacheError::InvalidRelocation(format!(
                "unknown relocation kind {raw}"
            ))),
        }
    }

    const fn patch_width(self) -> usize {
        match self {
            Self::Abs64 => 8,
            Self::X86PcRel4 | Self::X86CallPcRel4 | Self::Arm64Call => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRelocationTarget {
    Helper(u32),
    InternalSymbol(u32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeRelocation {
    pub offset: u64,
    pub kind: NativeRelocationKind,
    pub target: NativeRelocationTarget,
    pub addend: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeHelperImport {
    pub stable_id: u32,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSymbol {
    pub stable_id: u32,
    pub code_offset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeContinuationEntry {
    pub function_id: u32,
    pub continuation_id: u32,
    pub code_offset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeTrapEntry {
    pub code_offset: u64,
    pub trap_code: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeExceptionEntry {
    pub start_offset: u64,
    pub end_offset: u64,
    pub handler_offset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeRootMap {
    pub code_offset: u64,
    pub slots: Vec<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeResumeEntry {
    pub function_id: u32,
    pub continuation_id: u32,
    pub resume_id: i32,
    pub code_offset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeArtifactImage {
    pub identity: NativeCacheIdentity,
    pub code: Vec<u8>,
    pub read_only_data: Vec<u8>,
    pub functions: Vec<NativeFunctionImage>,
    pub continuations: Vec<NativeContinuationEntry>,
    pub relocations: Vec<NativeRelocation>,
    pub helper_imports: Vec<NativeHelperImport>,
    pub internal_symbols: Vec<NativeSymbol>,
    pub traps: Vec<NativeTrapEntry>,
    pub root_maps: Vec<NativeRootMap>,
    pub resume_entries: Vec<NativeResumeEntry>,
    pub exception_metadata: Vec<NativeExceptionEntry>,
    pub signature_metadata: Vec<u8>,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct CachedRegionMetadataEnvelope {
    format: String,
    entries: Vec<CachedRegionMetadataEntry>,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct CachedRegionMetadataEntry {
    function_id: u32,
    metadata: crate::JitRegionStateMetadata,
}

impl NativeArtifactImage {
    #[must_use]
    pub fn minimal(
        identity: NativeCacheIdentity,
        code: Vec<u8>,
        function: NativeFunctionImage,
    ) -> Self {
        let symbol = NativeSymbol {
            stable_id: function.function_id,
            code_offset: function.code_offset,
        };
        Self {
            identity,
            code,
            read_only_data: Vec::new(),
            functions: vec![function],
            continuations: Vec::new(),
            relocations: Vec::new(),
            helper_imports: Vec::new(),
            internal_symbols: vec![symbol],
            traps: Vec::new(),
            root_maps: Vec::new(),
            resume_entries: Vec::new(),
            exception_metadata: Vec::new(),
            signature_metadata: Vec::new(),
        }
    }

    /// Builds one PNA1 image from the exact machine code retained by the
    /// production Cranelift compilation records.
    ///
    /// AMD64 helper calls are routed through artifact-local trampolines. The
    /// trampoline's immediate is an `Abs64` helper relocation resolved by the
    /// loader, so no process address is persisted and the original `call
    /// rel32` remains in range even when the RX mapping is far from the host
    /// executable.
    pub fn from_compile_records(
        identity: NativeCacheIdentity,
        records: &[crate::JitUnitCompileRecord],
    ) -> Result<Self, NativeCacheError> {
        if !identity.target_triple.contains("x86_64") {
            return Err(NativeCacheError::UnsupportedPlatform);
        }

        enum PendingTarget {
            Internal(u32),
            Helper(String),
        }
        struct PendingRelocation {
            offset: u64,
            kind: NativeRelocationKind,
            target: PendingTarget,
            addend: i64,
        }

        let mut code = Vec::new();
        let mut functions = Vec::new();
        let mut internal_symbols = Vec::new();
        let mut pending_relocations = Vec::new();
        let mut helper_names = BTreeSet::new();
        let mut cached_metadata = Vec::new();
        let mut emitted_graphs = BTreeMap::<usize, (u64, BTreeMap<php_ir::FunctionId, u32>)>::new();
        let mut emitted_symbol_ids = BTreeSet::new();
        let mut next_symbol_id = records
            .iter()
            .map(|record| record.function.raw())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| NativeCacheError::SizeLimit {
                what: "internal symbol count",
                actual: u64::from(u32::MAX),
                limit: u64::from(u32::MAX - 1),
            })?;

        for record in records {
            let handle = record.result.handle.as_ref().ok_or_else(|| {
                NativeCacheError::InvalidHeader(format!(
                    "function {} has no native handle",
                    record.function.raw()
                ))
            })?;
            let relocatable = handle.relocatable_code().ok_or_else(|| {
                NativeCacheError::InvalidRelocation(format!(
                    "function {} has no relocation-aware machine-code image",
                    record.function.raw()
                ))
            })?;
            let mut metadata = handle.region_state_metadata().cloned().ok_or_else(|| {
                NativeCacheError::InvalidHeader(format!(
                    "function {} has no native state metadata",
                    record.function.raw()
                ))
            })?;
            for entry in &mut metadata.function_entries {
                // PNA1 never persists process addresses. They are rebound to
                // validated function entries after the RX mapping is created.
                entry.address = 0;
            }
            cached_metadata.push(CachedRegionMetadataEntry {
                function_id: record.function.raw(),
                metadata,
            });
            let graph_key = relocatable as *const crate::JitRelocatableCode as usize;
            let (graph_offset, _graph_symbols) =
                if let Some((offset, symbols)) = emitted_graphs.get(&graph_key) {
                    (*offset, symbols.clone())
                } else {
                    align_vec(&mut code, 16);
                    let graph_offset = code.len() as u64;
                    code.extend_from_slice(&relocatable.code);

                    let mut graph_symbols = BTreeMap::new();
                    for function in &relocatable.functions {
                        let stable_id = if function.function == record.function {
                            record.function.raw()
                        } else {
                            let stable_id = next_symbol_id;
                            next_symbol_id = next_symbol_id.checked_add(1).ok_or_else(|| {
                                NativeCacheError::SizeLimit {
                                    what: "internal symbol count",
                                    actual: u64::from(u32::MAX),
                                    limit: u64::from(u32::MAX - 1),
                                }
                            })?;
                            stable_id
                        };
                        graph_symbols.insert(function.function, stable_id);
                        emitted_symbol_ids.insert(stable_id);
                        internal_symbols.push(NativeSymbol {
                            stable_id,
                            code_offset: graph_offset.saturating_add(function.code_offset),
                        });
                    }

                    for relocation in &relocatable.relocations {
                        let kind = match relocation.kind {
                            crate::JitRelocatableKind::Abs64 => NativeRelocationKind::Abs64,
                            crate::JitRelocatableKind::X86PcRel4 => NativeRelocationKind::X86PcRel4,
                            crate::JitRelocatableKind::X86CallPcRel4 => {
                                NativeRelocationKind::X86CallPcRel4
                            }
                            crate::JitRelocatableKind::Arm64Call => {
                                return Err(NativeCacheError::InvalidRelocation(
                                    "Arm64 relocation in an AMD64 cache image".to_owned(),
                                ));
                            }
                        };
                        let target = match &relocation.target {
                            crate::JitRelocatableTarget::InternalFunction(function) => {
                                let stable_id =
                                    graph_symbols.get(function).copied().ok_or_else(|| {
                                        NativeCacheError::UnknownInternalSymbol(function.raw())
                                    })?;
                                PendingTarget::Internal(stable_id)
                            }
                            crate::JitRelocatableTarget::Helper(name) => {
                                helper_names.insert(name.clone());
                                PendingTarget::Helper(name.clone())
                            }
                        };
                        pending_relocations.push(PendingRelocation {
                            offset: graph_offset.saturating_add(relocation.offset),
                            kind,
                            target,
                            addend: relocation.addend,
                        });
                    }
                    emitted_graphs.insert(graph_key, (graph_offset, graph_symbols.clone()));
                    (graph_offset, graph_symbols)
                };

            let root = relocatable
                .functions
                .iter()
                .find(|function| function.function == record.function)
                .or_else(|| {
                    relocatable
                        .functions
                        .iter()
                        .find(|function| function.function == relocatable.root)
                })
                .ok_or_else(|| {
                    NativeCacheError::InvalidHeader(format!(
                        "function {} is absent from its relocatable graph",
                        record.function.raw()
                    ))
                })?;
            functions.push(NativeFunctionImage {
                function_id: record.function.raw(),
                code_offset: graph_offset.saturating_add(root.code_offset),
                code_len: root.code_len,
                arity: root.arity,
                abi: NativeFunctionAbi::PackedI64StatusOut,
            });
            if !emitted_symbol_ids.contains(&record.function.raw()) {
                emitted_symbol_ids.insert(record.function.raw());
                internal_symbols.push(NativeSymbol {
                    stable_id: record.function.raw(),
                    code_offset: graph_offset.saturating_add(root.code_offset),
                });
            }
        }

        let mut helper_imports = Vec::new();
        let mut helper_stubs = BTreeMap::new();
        for name in helper_names {
            let helper = crate::lookup_helper_by_name(&name).ok_or_else(|| {
                NativeCacheError::InvalidRelocation(format!(
                    "Cranelift imported unregistered helper `{name}`"
                ))
            })?;
            helper_imports.push(NativeHelperImport {
                stable_id: helper.id.0,
                name: name.clone(),
            });
            align_vec(&mut code, 16);
            let code_offset = code.len() as u64;
            // movabs rax, <helper>; jmp rax
            code.extend_from_slice(&[0x48, 0xb8]);
            code.extend_from_slice(&0_u64.to_le_bytes());
            code.extend_from_slice(&[0xff, 0xe0]);
            let stable_id = next_symbol_id;
            next_symbol_id =
                next_symbol_id
                    .checked_add(1)
                    .ok_or_else(|| NativeCacheError::SizeLimit {
                        what: "internal symbol count",
                        actual: u64::from(u32::MAX),
                        limit: u64::from(u32::MAX - 1),
                    })?;
            internal_symbols.push(NativeSymbol {
                stable_id,
                code_offset,
            });
            helper_stubs.insert(name, (stable_id, helper.id.0, code_offset + 2));
        }

        let mut relocations = helper_stubs
            .values()
            .map(|(_, helper_id, immediate_offset)| NativeRelocation {
                offset: *immediate_offset,
                kind: NativeRelocationKind::Abs64,
                target: NativeRelocationTarget::Helper(*helper_id),
                addend: 0,
            })
            .collect::<Vec<_>>();
        for relocation in pending_relocations {
            let target = match relocation.target {
                PendingTarget::Internal(stable_id) => {
                    NativeRelocationTarget::InternalSymbol(stable_id)
                }
                PendingTarget::Helper(name) => {
                    let (stub_id, _, _) = helper_stubs[&name];
                    NativeRelocationTarget::InternalSymbol(stub_id)
                }
            };
            relocations.push(NativeRelocation {
                offset: relocation.offset,
                kind: relocation.kind,
                target,
                addend: relocation.addend,
            });
        }
        relocations.sort_by_key(|relocation| relocation.offset);

        let signature_metadata = serde_json::to_vec(&CachedRegionMetadataEnvelope {
            format: "PRM3".to_owned(),
            entries: cached_metadata,
        })
        .map_err(|error| {
            NativeCacheError::InvalidSection(format!(
                "failed to encode native state metadata: {error}"
            ))
        })?;

        Ok(Self {
            identity,
            code,
            read_only_data: Vec::new(),
            functions,
            continuations: Vec::new(),
            relocations,
            helper_imports,
            internal_symbols,
            traps: Vec::new(),
            root_maps: Vec::new(),
            resume_entries: Vec::new(),
            exception_metadata: Vec::new(),
            signature_metadata,
        })
    }
}

fn align_vec(bytes: &mut Vec<u8>, alignment: usize) {
    let padding = (alignment - bytes.len() % alignment) % alignment;
    bytes.resize(bytes.len().saturating_add(padding), 0);
}

fn decode_region_metadata(
    bytes: &[u8],
) -> Result<BTreeMap<u32, crate::JitRegionStateMetadata>, NativeCacheError> {
    if bytes.is_empty() {
        return Ok(BTreeMap::new());
    }
    let envelope =
        serde_json::from_slice::<CachedRegionMetadataEnvelope>(bytes).map_err(|error| {
            NativeCacheError::InvalidSection(format!("invalid native state metadata: {error}"))
        })?;
    if envelope.format != "PRM3" {
        return Err(NativeCacheError::InvalidSection(
            "unsupported native state metadata format".to_owned(),
        ));
    }
    let mut metadata = BTreeMap::new();
    for entry in envelope.entries {
        if entry
            .metadata
            .function_entries
            .iter()
            .any(|function| function.address != 0)
        {
            return Err(NativeCacheError::InvalidSection(
                "native state metadata contains a persisted process address".to_owned(),
            ));
        }
        if metadata.insert(entry.function_id, entry.metadata).is_some() {
            return Err(NativeCacheError::InvalidSection(format!(
                "duplicate native state metadata for function {}",
                entry.function_id
            )));
        }
    }
    Ok(metadata)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NativeCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub writes: u64,
    pub rebuilds: u64,
    pub invalid_artifacts: u64,
    pub compile_waits: u64,
    pub bytes_loaded: u64,
    pub bytes_written: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCacheEvent {
    Disabled,
    Hit,
    Miss,
    Written,
    Rebuilt,
}

#[derive(Debug)]
pub enum NativeCacheError {
    Io(std::io::Error),
    Disabled,
    InvalidHeader(String),
    InvalidSection(String),
    IdentityMismatch,
    ChecksumMismatch,
    InvalidRelocation(String),
    UnknownHelper(u32),
    UnknownInternalSymbol(u32),
    UnsafePath(String),
    SizeLimit {
        what: &'static str,
        actual: u64,
        limit: u64,
    },
    LockTimeout(PathBuf),
    UnsupportedPlatform,
    NativeStatus(i32),
}

impl fmt::Display for NativeCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "native cache I/O error: {error}"),
            Self::Disabled => f.write_str("native cache mode does not permit this operation"),
            Self::InvalidHeader(detail) => write!(f, "invalid PNA1 header: {detail}"),
            Self::InvalidSection(detail) => write!(f, "invalid PNA1 section: {detail}"),
            Self::IdentityMismatch => f.write_str("PNA1 identity does not match this process"),
            Self::ChecksumMismatch => f.write_str("PNA1 checksum mismatch"),
            Self::InvalidRelocation(detail) => write!(f, "invalid PNA1 relocation: {detail}"),
            Self::UnknownHelper(id) => write!(f, "PNA1 references unknown helper ID {id}"),
            Self::UnknownInternalSymbol(id) => {
                write!(f, "PNA1 references unknown internal symbol {id}")
            }
            Self::UnsafePath(detail) => write!(f, "unsafe native cache path: {detail}"),
            Self::SizeLimit {
                what,
                actual,
                limit,
            } => {
                write!(f, "native cache {what} size {actual} exceeds limit {limit}")
            }
            Self::LockTimeout(path) => write!(f, "timed out waiting for {}", path.display()),
            Self::UnsupportedPlatform => f.write_str("native cache executable mapping unsupported"),
            Self::NativeStatus(status) => write!(f, "cached native entry returned status {status}"),
        }
    }
}

impl std::error::Error for NativeCacheError {}

impl From<std::io::Error> for NativeCacheError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Default)]
struct AtomicStats {
    hits: AtomicU64,
    misses: AtomicU64,
    writes: AtomicU64,
    rebuilds: AtomicU64,
    invalid_artifacts: AtomicU64,
    compile_waits: AtomicU64,
    bytes_loaded: AtomicU64,
    bytes_written: AtomicU64,
}

pub struct NativeArtifactCache {
    config: NativeCacheConfig,
    stats: AtomicStats,
}

impl fmt::Debug for NativeArtifactCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeArtifactCache")
            .field("config", &self.config)
            .field("stats", &self.stats())
            .finish()
    }
}

impl NativeArtifactCache {
    pub fn new(config: NativeCacheConfig) -> Result<Self, NativeCacheError> {
        if config.mode != NativeCacheMode::Off {
            prepare_cache_directory(&config.directory)?;
        }
        Ok(Self {
            config,
            stats: AtomicStats::default(),
        })
    }

    #[must_use]
    pub fn config(&self) -> &NativeCacheConfig {
        &self.config
    }

    #[must_use]
    pub fn stats(&self) -> NativeCacheStats {
        NativeCacheStats {
            hits: self.stats.hits.load(Ordering::Relaxed),
            misses: self.stats.misses.load(Ordering::Relaxed),
            writes: self.stats.writes.load(Ordering::Relaxed),
            rebuilds: self.stats.rebuilds.load(Ordering::Relaxed),
            invalid_artifacts: self.stats.invalid_artifacts.load(Ordering::Relaxed),
            compile_waits: self.stats.compile_waits.load(Ordering::Relaxed),
            bytes_loaded: self.stats.bytes_loaded.load(Ordering::Relaxed),
            bytes_written: self.stats.bytes_written.load(Ordering::Relaxed),
        }
    }

    pub fn load(
        &self,
        identity: &NativeCacheIdentity,
        resolve_helper: impl Fn(u32) -> Option<usize>,
    ) -> Result<Option<NativeLoadedArtifact>, NativeCacheError> {
        if !self.config.mode.can_read() {
            return Ok(None);
        }
        let path = self.artifact_path(identity);
        match self.load_path(&path, identity, &resolve_helper) {
            Ok(Some((artifact, bytes))) => {
                self.stats.hits.fetch_add(1, Ordering::Relaxed);
                self.stats.bytes_loaded.fetch_add(bytes, Ordering::Relaxed);
                Ok(Some(artifact))
            }
            Ok(None) => {
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            Err(error) => {
                self.stats.invalid_artifacts.fetch_add(1, Ordering::Relaxed);
                quarantine(&path)?;
                Err(error)
            }
        }
    }

    pub fn get_or_compile(
        &self,
        identity: &NativeCacheIdentity,
        resolve_helper: impl Fn(u32) -> Option<usize>,
        compile: impl FnOnce() -> Result<NativeArtifactImage, NativeCacheError>,
    ) -> Result<(NativeLoadedArtifact, NativeCacheEvent), NativeCacheError> {
        let mut rebuilding = false;
        if self.config.mode.can_read() {
            match self.load(identity, &resolve_helper) {
                Ok(Some(artifact)) => return Ok((artifact, NativeCacheEvent::Hit)),
                Ok(None) => {}
                Err(_) if self.config.mode.can_write() => {
                    self.stats.rebuilds.fetch_add(1, Ordering::Relaxed);
                    rebuilding = true;
                }
                Err(error) => return Err(error),
            }
        }
        if !self.config.mode.can_write() {
            return Err(NativeCacheError::Disabled);
        }
        let lock_path = self.lock_path(identity);
        let (lock, waited) = acquire_lock(&lock_path)?;
        if waited {
            self.stats.compile_waits.fetch_add(1, Ordering::Relaxed);
        }
        if self.config.mode.can_read()
            && let Some((artifact, bytes)) =
                self.load_path(&self.artifact_path(identity), identity, &resolve_helper)?
        {
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            self.stats.bytes_loaded.fetch_add(bytes, Ordering::Relaxed);
            return Ok((artifact, NativeCacheEvent::Hit));
        }
        let image = compile()?;
        if &image.identity != identity {
            return Err(NativeCacheError::IdentityMismatch);
        }
        let bytes = encode_artifact(&image, &self.config)?;
        self.write_atomic(identity, &bytes)?;
        drop(lock);
        let (artifact, loaded_bytes) = self
            .load_path(&self.artifact_path(identity), identity, &resolve_helper)?
            .ok_or_else(|| {
                NativeCacheError::InvalidHeader("atomic write disappeared".to_owned())
            })?;
        self.stats.writes.fetch_add(1, Ordering::Relaxed);
        self.stats
            .bytes_written
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        self.stats
            .bytes_loaded
            .fetch_add(loaded_bytes, Ordering::Relaxed);
        Ok((
            artifact,
            if rebuilding {
                NativeCacheEvent::Rebuilt
            } else {
                NativeCacheEvent::Written
            },
        ))
    }

    pub fn clear(&self) -> Result<u64, NativeCacheError> {
        if self.config.mode == NativeCacheMode::Off {
            return Err(NativeCacheError::Disabled);
        }
        let mut removed = 0_u64;
        for entry in fs::read_dir(&self.config.directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            let extension = path.extension().and_then(|value| value.to_str());
            if metadata.is_file() && matches!(extension, Some("pna" | "invalid" | "tmp" | "lock")) {
                fs::remove_file(path)?;
                removed = removed.saturating_add(1);
            }
        }
        Ok(removed)
    }

    fn artifact_path(&self, identity: &NativeCacheIdentity) -> PathBuf {
        self.config
            .directory
            .join(format!("{}.pna", identity.cache_key()))
    }

    fn lock_path(&self, identity: &NativeCacheIdentity) -> PathBuf {
        self.config
            .directory
            .join(format!("{}.lock", identity.cache_key()))
    }

    fn load_path(
        &self,
        path: &Path,
        identity: &NativeCacheIdentity,
        resolve_helper: &impl Fn(u32) -> Option<usize>,
    ) -> Result<Option<(NativeLoadedArtifact, u64)>, NativeCacheError> {
        let mut file = match open_no_follow(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(NativeCacheError::UnsafePath(
                "artifact is not a regular file".to_owned(),
            ));
        }
        if metadata.len() > self.config.max_artifact_bytes as u64 {
            return Err(NativeCacheError::SizeLimit {
                what: "artifact",
                actual: metadata.len(),
                limit: self.config.max_artifact_bytes as u64,
            });
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes)?;
        let image = decode_artifact(&bytes, identity, &self.config)?;
        let artifact = NativeLoadedArtifact::map(image, resolve_helper)?;
        Ok(Some((artifact, metadata.len())))
    }

    fn write_atomic(
        &self,
        identity: &NativeCacheIdentity,
        bytes: &[u8],
    ) -> Result<(), NativeCacheError> {
        enforce_total_cache_limit(
            &self.config.directory,
            self.config.max_cache_bytes,
            bytes.len(),
        )?;
        let key = identity.cache_key();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp =
            self.config
                .directory
                .join(format!(".{key}.{}.{}.tmp", std::process::id(), nonce));
        let destination = self.artifact_path(identity);
        // Validate the final serialized representation before it can become
        // visible to concurrent readers. This also guards serializer changes.
        let _ = decode_artifact(bytes, identity, &self.config)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, &destination)?;
        sync_directory(&self.config.directory)?;
        Ok(())
    }
}

pub struct NativeLoadedArtifact {
    image: NativeArtifactImage,
    mapping: ExecutableMapping,
    region_metadata: BTreeMap<u32, crate::JitRegionStateMetadata>,
}

impl fmt::Debug for NativeLoadedArtifact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeLoadedArtifact")
            .field("identity", &self.image.identity)
            .field("functions", &self.image.functions)
            .field("mapping_len", &self.mapping.len)
            .finish()
    }
}

impl NativeLoadedArtifact {
    fn map(
        image: NativeArtifactImage,
        resolve_helper: &impl Fn(u32) -> Option<usize>,
    ) -> Result<Self, NativeCacheError> {
        let mut region_metadata = decode_region_metadata(&image.signature_metadata)?;
        let mut mapping = ExecutableMapping::new(image.code.len())?;
        mapping.bytes_mut()[..image.code.len()].copy_from_slice(&image.code);
        apply_relocations(&mut mapping, &image, resolve_helper)?;
        mapping.make_executable()?;
        for metadata in region_metadata.values_mut() {
            for function_entry in &mut metadata.function_entries {
                let function = image
                    .functions
                    .iter()
                    .find(|function| function.function_id == function_entry.function.raw())
                    .ok_or(NativeCacheError::UnknownInternalSymbol(
                        function_entry.function.raw(),
                    ))?;
                function_entry.address = mapping
                    .address
                    .checked_add(function.code_offset as usize)
                    .ok_or_else(|| {
                        NativeCacheError::InvalidSection(
                            "cached function address overflow".to_owned(),
                        )
                    })?;
            }
        }
        Ok(Self {
            image,
            mapping,
            region_metadata,
        })
    }

    #[must_use]
    pub fn image(&self) -> &NativeArtifactImage {
        &self.image
    }

    /// Returns the address-rebound state metadata for one cached root.
    #[must_use]
    pub fn region_metadata(&self, function_id: u32) -> Option<&crate::JitRegionStateMetadata> {
        self.region_metadata.get(&function_id)
    }

    pub fn entry_address(&self, function_id: u32) -> Result<usize, NativeCacheError> {
        let function = self
            .image
            .functions
            .iter()
            .find(|entry| entry.function_id == function_id)
            .ok_or(NativeCacheError::UnknownInternalSymbol(function_id))?;
        Ok(self
            .mapping
            .address
            .saturating_add(function.code_offset as usize))
    }

    pub fn invoke_i64_status_out(&self, function_id: u32) -> Result<i64, NativeCacheError> {
        let function = self
            .image
            .functions
            .iter()
            .find(|entry| entry.function_id == function_id)
            .ok_or(NativeCacheError::UnknownInternalSymbol(function_id))?;
        if function.arity != 0 {
            return Err(NativeCacheError::InvalidHeader(
                "cache probe supports only zero-arity entries".to_owned(),
            ));
        }
        if !matches!(
            function.abi,
            NativeFunctionAbi::I64StatusOut | NativeFunctionAbi::PackedI64StatusOut
        ) {
            return Err(NativeCacheError::InvalidHeader(
                "cached entry does not use the status/out ABI".to_owned(),
            ));
        }
        let address = self.entry_address(function_id)?;
        let mut out = 0_i64;
        let mut state = crate::JitDeoptState::default();
        // SAFETY: PNA validation proved the entry range and signature metadata;
        // the mapping was writable only before its RX transition.
        let status = unsafe {
            match function.abi {
                NativeFunctionAbi::I64StatusOut => {
                    let entry: extern "C" fn(
                        *mut i64,
                        *mut crate::JitDeoptState,
                        i32,
                        *const crate::JitDeoptState,
                    ) -> i32 = std::mem::transmute(address);
                    entry(&mut out, &mut state, -1, std::ptr::null())
                }
                NativeFunctionAbi::PackedI64StatusOut => {
                    let entry: extern "C" fn(
                        *const i64,
                        *mut i64,
                        *mut crate::JitDeoptState,
                        i32,
                        *const crate::JitDeoptState,
                    ) -> i32 = std::mem::transmute(address);
                    entry(
                        std::ptr::NonNull::<i64>::dangling().as_ptr(),
                        &mut out,
                        &mut state,
                        -1,
                        std::ptr::null(),
                    )
                }
            }
        };
        if status == crate::JitCallStatus::RETURN.0 as i32 {
            Ok(out)
        } else {
            Err(NativeCacheError::NativeStatus(status))
        }
    }

    #[must_use]
    pub const fn writable_and_executable(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u16)]
enum SectionKind {
    Identity = 1,
    Code = 2,
    ReadOnlyData = 3,
    FunctionEntries = 4,
    Continuations = 5,
    Relocations = 6,
    HelperImports = 7,
    InternalSymbols = 8,
    Traps = 9,
    ExceptionMetadata = 10,
    RootMaps = 11,
    ResumeEntries = 12,
    SignatureMetadata = 13,
}

impl SectionKind {
    fn from_raw(raw: u16) -> Result<Self, NativeCacheError> {
        match raw {
            1 => Ok(Self::Identity),
            2 => Ok(Self::Code),
            3 => Ok(Self::ReadOnlyData),
            4 => Ok(Self::FunctionEntries),
            5 => Ok(Self::Continuations),
            6 => Ok(Self::Relocations),
            7 => Ok(Self::HelperImports),
            8 => Ok(Self::InternalSymbols),
            9 => Ok(Self::Traps),
            10 => Ok(Self::ExceptionMetadata),
            11 => Ok(Self::RootMaps),
            12 => Ok(Self::ResumeEntries),
            13 => Ok(Self::SignatureMetadata),
            _ => Err(NativeCacheError::InvalidSection(format!(
                "unknown section kind {raw}"
            ))),
        }
    }
}

#[derive(Clone, Debug)]
struct SectionRecord {
    kind: SectionKind,
    alignment: u32,
    offset: u64,
    length: u64,
    count: u32,
}

fn encode_artifact(
    image: &NativeArtifactImage,
    config: &NativeCacheConfig,
) -> Result<Vec<u8>, NativeCacheError> {
    validate_image(image, config)?;
    let sections = vec![
        (SectionKind::Identity, image.identity.encode(), 1, 1),
        (
            SectionKind::Code,
            image.code.clone(),
            16,
            image.code.len() as u32,
        ),
        (
            SectionKind::ReadOnlyData,
            image.read_only_data.clone(),
            8,
            image.read_only_data.len() as u32,
        ),
        (
            SectionKind::FunctionEntries,
            encode_functions(&image.functions),
            8,
            image.functions.len() as u32,
        ),
        (
            SectionKind::Continuations,
            encode_continuations(&image.continuations),
            8,
            image.continuations.len() as u32,
        ),
        (
            SectionKind::Relocations,
            encode_relocations(&image.relocations),
            8,
            image.relocations.len() as u32,
        ),
        (
            SectionKind::HelperImports,
            encode_helpers(&image.helper_imports),
            4,
            image.helper_imports.len() as u32,
        ),
        (
            SectionKind::InternalSymbols,
            encode_symbols(&image.internal_symbols),
            8,
            image.internal_symbols.len() as u32,
        ),
        (
            SectionKind::Traps,
            encode_traps(&image.traps),
            8,
            image.traps.len() as u32,
        ),
        (
            SectionKind::ExceptionMetadata,
            encode_exceptions(&image.exception_metadata),
            8,
            image.exception_metadata.len() as u32,
        ),
        (
            SectionKind::RootMaps,
            encode_root_maps(&image.root_maps),
            8,
            image.root_maps.len() as u32,
        ),
        (
            SectionKind::ResumeEntries,
            encode_resumes(&image.resume_entries),
            8,
            image.resume_entries.len() as u32,
        ),
        (
            SectionKind::SignatureMetadata,
            image.signature_metadata.clone(),
            4,
            u32::from(!image.signature_metadata.is_empty()),
        ),
    ];
    let section_table_len = sections.len() * SECTION_RECORD_LEN;
    let mut offset = HEADER_LEN + section_table_len;
    let mut records = Vec::with_capacity(sections.len());
    for (kind, data, alignment, count) in &sections {
        offset = align_up(offset, *alignment as usize)?;
        records.push(SectionRecord {
            kind: *kind,
            alignment: *alignment,
            offset: offset as u64,
            length: data.len() as u64,
            count: *count,
        });
        offset = offset.checked_add(data.len()).ok_or_else(|| {
            NativeCacheError::InvalidHeader("artifact length overflow".to_owned())
        })?;
    }
    if offset > config.max_artifact_bytes {
        return Err(NativeCacheError::SizeLimit {
            what: "artifact",
            actual: offset as u64,
            limit: config.max_artifact_bytes as u64,
        });
    }
    let mut out = vec![0_u8; offset];
    out[0..4].copy_from_slice(&PNA_MAGIC);
    out[4..6].copy_from_slice(&PNA_FORMAT_VERSION.to_le_bytes());
    out[6..8].copy_from_slice(&(HEADER_LEN as u16).to_le_bytes());
    out[8..16].copy_from_slice(&(offset as u64).to_le_bytes());
    out[16..20].copy_from_slice(&(sections.len() as u32).to_le_bytes());
    out[20..24].copy_from_slice(&(SECTION_RECORD_LEN as u32).to_le_bytes());
    for (index, record) in records.iter().enumerate() {
        let start = HEADER_LEN + index * SECTION_RECORD_LEN;
        out[start..start + 2].copy_from_slice(&(record.kind as u16).to_le_bytes());
        out[start + 4..start + 8].copy_from_slice(&record.alignment.to_le_bytes());
        out[start + 8..start + 16].copy_from_slice(&record.offset.to_le_bytes());
        out[start + 16..start + 24].copy_from_slice(&record.length.to_le_bytes());
        out[start + 24..start + 28].copy_from_slice(&record.count.to_le_bytes());
    }
    for ((_, data, _, _), record) in sections.iter().zip(&records) {
        let start = record.offset as usize;
        out[start..start + data.len()].copy_from_slice(data);
    }
    let checksum = artifact_checksum(&out);
    out[24..56].copy_from_slice(&checksum);
    Ok(out)
}

fn decode_artifact(
    bytes: &[u8],
    expected: &NativeCacheIdentity,
    config: &NativeCacheConfig,
) -> Result<NativeArtifactImage, NativeCacheError> {
    if bytes.len() < HEADER_LEN || bytes[0..4] != PNA_MAGIC {
        return Err(NativeCacheError::InvalidHeader(
            "bad magic or truncated header".to_owned(),
        ));
    }
    if read_u16(bytes, 4)? != PNA_FORMAT_VERSION {
        return Err(NativeCacheError::InvalidHeader(
            "unsupported format version".to_owned(),
        ));
    }
    if read_u16(bytes, 6)? as usize != HEADER_LEN {
        return Err(NativeCacheError::InvalidHeader(
            "unexpected header size".to_owned(),
        ));
    }
    if read_u64(bytes, 8)? as usize != bytes.len() {
        return Err(NativeCacheError::InvalidHeader(
            "total length mismatch".to_owned(),
        ));
    }
    if bytes.len() > config.max_artifact_bytes {
        return Err(NativeCacheError::SizeLimit {
            what: "artifact",
            actual: bytes.len() as u64,
            limit: config.max_artifact_bytes as u64,
        });
    }
    let section_count = read_u32(bytes, 16)? as usize;
    if section_count == 0 || section_count > MAX_SECTIONS {
        return Err(NativeCacheError::InvalidHeader(
            "invalid section count".to_owned(),
        ));
    }
    if read_u32(bytes, 20)? as usize != SECTION_RECORD_LEN {
        return Err(NativeCacheError::InvalidHeader(
            "unexpected section record size".to_owned(),
        ));
    }
    let expected_checksum = &bytes[24..56];
    if artifact_checksum(bytes) != expected_checksum {
        return Err(NativeCacheError::ChecksumMismatch);
    }
    let table_end = HEADER_LEN
        .checked_add(section_count * SECTION_RECORD_LEN)
        .ok_or_else(|| NativeCacheError::InvalidHeader("section table overflow".to_owned()))?;
    if table_end > bytes.len() {
        return Err(NativeCacheError::InvalidHeader(
            "truncated section table".to_owned(),
        ));
    }
    let mut records = Vec::with_capacity(section_count);
    let mut kinds = BTreeSet::new();
    for index in 0..section_count {
        let start = HEADER_LEN + index * SECTION_RECORD_LEN;
        let kind = SectionKind::from_raw(read_u16(bytes, start)?)?;
        if !kinds.insert(kind) {
            return Err(NativeCacheError::InvalidSection(
                "duplicate section".to_owned(),
            ));
        }
        let alignment = read_u32(bytes, start + 4)?;
        let offset = read_u64(bytes, start + 8)?;
        let length = read_u64(bytes, start + 16)?;
        let count = read_u32(bytes, start + 24)?;
        if alignment == 0 || !alignment.is_power_of_two() || alignment > 4096 {
            return Err(NativeCacheError::InvalidSection(
                "invalid alignment".to_owned(),
            ));
        }
        let end = offset
            .checked_add(length)
            .ok_or_else(|| NativeCacheError::InvalidSection("range overflow".to_owned()))?;
        if offset < table_end as u64
            || end > bytes.len() as u64
            || offset % u64::from(alignment) != 0
        {
            return Err(NativeCacheError::InvalidSection(
                "out-of-bounds or misaligned range".to_owned(),
            ));
        }
        records.push(SectionRecord {
            kind,
            alignment,
            offset,
            length,
            count,
        });
    }
    let mut ordered = records
        .iter()
        .filter(|record| record.length > 0)
        .collect::<Vec<_>>();
    ordered.sort_by_key(|record| record.offset);
    for pair in ordered.windows(2) {
        if pair[0].offset + pair[0].length > pair[1].offset {
            return Err(NativeCacheError::InvalidSection(
                "overlapping sections".to_owned(),
            ));
        }
    }
    let section = |kind| -> Result<(&[u8], u32), NativeCacheError> {
        let record = records
            .iter()
            .find(|record| record.kind == kind)
            .ok_or_else(|| NativeCacheError::InvalidSection(format!("missing {kind:?}")))?;
        Ok((
            &bytes[record.offset as usize..(record.offset + record.length) as usize],
            record.count,
        ))
    };
    let identity = NativeCacheIdentity::decode(section(SectionKind::Identity)?.0)?;
    if &identity != expected {
        return Err(NativeCacheError::IdentityMismatch);
    }
    let code = section(SectionKind::Code)?.0.to_vec();
    let read_only_data = section(SectionKind::ReadOnlyData)?.0.to_vec();
    let functions = decode_functions(section(SectionKind::FunctionEntries)?)?;
    let continuations = decode_continuations(section(SectionKind::Continuations)?)?;
    let relocations = decode_relocations(section(SectionKind::Relocations)?)?;
    let helper_imports = decode_helpers(section(SectionKind::HelperImports)?)?;
    let internal_symbols = decode_symbols(section(SectionKind::InternalSymbols)?)?;
    let traps = decode_traps(section(SectionKind::Traps)?)?;
    let root_maps = decode_root_maps(section(SectionKind::RootMaps)?)?;
    let resume_entries = decode_resumes(section(SectionKind::ResumeEntries)?)?;
    let exception_metadata = decode_exceptions(section(SectionKind::ExceptionMetadata)?)?;
    let signature_metadata = section(SectionKind::SignatureMetadata)?.0.to_vec();
    let image = NativeArtifactImage {
        identity,
        code,
        read_only_data,
        functions,
        continuations,
        relocations,
        helper_imports,
        internal_symbols,
        traps,
        root_maps,
        resume_entries,
        exception_metadata,
        signature_metadata,
    };
    validate_image(&image, config)?;
    Ok(image)
}

fn validate_image(
    image: &NativeArtifactImage,
    config: &NativeCacheConfig,
) -> Result<(), NativeCacheError> {
    if image.code.is_empty() || image.code.len() > config.max_code_bytes {
        return Err(NativeCacheError::SizeLimit {
            what: "code",
            actual: image.code.len() as u64,
            limit: config.max_code_bytes as u64,
        });
    }
    if image.relocations.len() > config.max_relocations {
        return Err(NativeCacheError::SizeLimit {
            what: "relocation count",
            actual: image.relocations.len() as u64,
            limit: config.max_relocations as u64,
        });
    }
    if image.functions.is_empty() {
        return Err(NativeCacheError::InvalidSection(
            "no function entries".to_owned(),
        ));
    }
    let helper_ids = image
        .helper_imports
        .iter()
        .map(|entry| entry.stable_id)
        .collect::<BTreeSet<_>>();
    let symbol_ids = image
        .internal_symbols
        .iter()
        .map(|entry| entry.stable_id)
        .collect::<BTreeSet<_>>();
    let function_ids = image
        .functions
        .iter()
        .map(|entry| entry.function_id)
        .collect::<BTreeSet<_>>();
    if helper_ids.len() != image.helper_imports.len()
        || symbol_ids.len() != image.internal_symbols.len()
        || function_ids.len() != image.functions.len()
    {
        return Err(NativeCacheError::InvalidSection(
            "duplicate import or symbol ID".to_owned(),
        ));
    }
    for import in &image.helper_imports {
        let helper = crate::lookup_helper_by_id(crate::JitHelperId(import.stable_id))
            .ok_or(NativeCacheError::UnknownHelper(import.stable_id))?;
        if helper.name != import.name {
            return Err(NativeCacheError::InvalidRelocation(format!(
                "helper ID {} name mismatch",
                import.stable_id
            )));
        }
    }
    for function in &image.functions {
        validate_code_range(
            function.code_offset,
            function.code_len,
            image.code.len(),
            "function",
        )?;
        if !symbol_ids.contains(&function.function_id) {
            return Err(NativeCacheError::UnknownInternalSymbol(
                function.function_id,
            ));
        }
    }
    for symbol in &image.internal_symbols {
        validate_code_range(symbol.code_offset, 1, image.code.len(), "symbol")?;
    }
    for relocation in &image.relocations {
        validate_code_range(
            relocation.offset,
            relocation.kind.patch_width() as u64,
            image.code.len(),
            "relocation",
        )?;
        match relocation.kind {
            NativeRelocationKind::X86PcRel4 | NativeRelocationKind::X86CallPcRel4
                if !image.identity.target_triple.contains("x86_64") =>
            {
                return Err(NativeCacheError::InvalidRelocation(
                    "x86 relocation in a non-x86 artifact".to_owned(),
                ));
            }
            NativeRelocationKind::Arm64Call
                if !image.identity.target_triple.contains("aarch64") =>
            {
                return Err(NativeCacheError::InvalidRelocation(
                    "Arm64 relocation in a non-Arm64 artifact".to_owned(),
                ));
            }
            _ => {}
        }
        match relocation.target {
            NativeRelocationTarget::Helper(id) if !helper_ids.contains(&id) => {
                return Err(NativeCacheError::UnknownHelper(id));
            }
            NativeRelocationTarget::InternalSymbol(id) if !symbol_ids.contains(&id) => {
                return Err(NativeCacheError::UnknownInternalSymbol(id));
            }
            _ => {}
        }
    }
    for continuation in &image.continuations {
        let Some(function) = image
            .functions
            .iter()
            .find(|function| function.function_id == continuation.function_id)
        else {
            return Err(NativeCacheError::UnknownInternalSymbol(
                continuation.function_id,
            ));
        };
        validate_code_range(
            continuation.code_offset,
            1,
            image.code.len(),
            "continuation",
        )?;
        if continuation.code_offset < function.code_offset
            || continuation.code_offset >= function.code_offset + function.code_len
        {
            return Err(NativeCacheError::InvalidSection(
                "continuation lies outside its function".to_owned(),
            ));
        }
    }
    for trap in &image.traps {
        validate_code_range(trap.code_offset, 1, image.code.len(), "trap")?;
    }
    for roots in &image.root_maps {
        validate_code_range(roots.code_offset, 1, image.code.len(), "root map")?;
    }
    let continuation_ids = image
        .continuations
        .iter()
        .map(|entry| (entry.function_id, entry.continuation_id))
        .collect::<BTreeSet<_>>();
    if continuation_ids.len() != image.continuations.len() {
        return Err(NativeCacheError::InvalidSection(
            "duplicate continuation ID".to_owned(),
        ));
    }
    let resume_ids = image
        .resume_entries
        .iter()
        .map(|entry| (entry.function_id, entry.continuation_id, entry.resume_id))
        .collect::<BTreeSet<_>>();
    if resume_ids.len() != image.resume_entries.len() {
        return Err(NativeCacheError::InvalidSection(
            "duplicate resume ID".to_owned(),
        ));
    }
    for resume in &image.resume_entries {
        if !function_ids.contains(&resume.function_id)
            || !continuation_ids.contains(&(resume.function_id, resume.continuation_id))
        {
            return Err(NativeCacheError::InvalidSection(
                "resume entry references an unknown function or continuation".to_owned(),
            ));
        }
        validate_code_range(resume.code_offset, 1, image.code.len(), "resume")?;
        let function = image
            .functions
            .iter()
            .find(|function| function.function_id == resume.function_id)
            .ok_or(NativeCacheError::UnknownInternalSymbol(resume.function_id))?;
        if resume.code_offset < function.code_offset
            || resume.code_offset >= function.code_offset + function.code_len
        {
            return Err(NativeCacheError::InvalidSection(
                "resume entry lies outside its function".to_owned(),
            ));
        }
    }
    for exception in &image.exception_metadata {
        if exception.start_offset >= exception.end_offset
            || exception.end_offset > image.code.len() as u64
        {
            return Err(NativeCacheError::InvalidSection(
                "exception range outside code".to_owned(),
            ));
        }
        validate_code_range(
            exception.handler_offset,
            1,
            image.code.len(),
            "exception handler",
        )?;
    }
    Ok(())
}

fn validate_code_range(
    offset: u64,
    len: u64,
    code_len: usize,
    name: &str,
) -> Result<(), NativeCacheError> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| NativeCacheError::InvalidSection(format!("{name} range overflow")))?;
    if len == 0 || end > code_len as u64 {
        return Err(NativeCacheError::InvalidSection(format!(
            "{name} range outside code"
        )));
    }
    Ok(())
}

fn apply_relocations(
    mapping: &mut ExecutableMapping,
    image: &NativeArtifactImage,
    resolve_helper: &impl Fn(u32) -> Option<usize>,
) -> Result<(), NativeCacheError> {
    for relocation in &image.relocations {
        let target = match relocation.target {
            NativeRelocationTarget::Helper(id) => {
                resolve_helper(id).ok_or(NativeCacheError::UnknownHelper(id))?
            }
            NativeRelocationTarget::InternalSymbol(id) => {
                let symbol = image
                    .internal_symbols
                    .iter()
                    .find(|symbol| symbol.stable_id == id)
                    .ok_or(NativeCacheError::UnknownInternalSymbol(id))?;
                mapping
                    .address
                    .checked_add(symbol.code_offset as usize)
                    .ok_or_else(|| {
                        NativeCacheError::InvalidRelocation(
                            "internal symbol address overflow".to_owned(),
                        )
                    })?
            }
        };
        let offset = relocation.offset as usize;
        let place = mapping.address.checked_add(offset).ok_or_else(|| {
            NativeCacheError::InvalidRelocation("relocation address overflow".to_owned())
        })?;
        match relocation.kind {
            NativeRelocationKind::Abs64 => {
                let value = (target as i128 + relocation.addend as i128)
                    .try_into()
                    .map_err(|_| {
                        NativeCacheError::InvalidRelocation("Abs64 overflow".to_owned())
                    })?;
                mapping.bytes_mut()[offset..offset + 8].copy_from_slice(&u64::to_le_bytes(value));
            }
            NativeRelocationKind::X86PcRel4 | NativeRelocationKind::X86CallPcRel4 => {
                let value = target as i128 + relocation.addend as i128 - place as i128;
                let value: i32 = value.try_into().map_err(|_| {
                    NativeCacheError::InvalidRelocation("PC-relative overflow".to_owned())
                })?;
                mapping.bytes_mut()[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
            NativeRelocationKind::Arm64Call => {
                let delta = target as i128 + relocation.addend as i128 - place as i128;
                if delta % 4 != 0 || !(-(1_i128 << 27)..(1_i128 << 27)).contains(&delta) {
                    return Err(NativeCacheError::InvalidRelocation(
                        "Arm64 call out of range".to_owned(),
                    ));
                }
                let instruction_bytes: [u8; 4] = mapping.bytes_mut()[offset..offset + 4]
                    .try_into()
                    .map_err(|_| {
                        NativeCacheError::InvalidRelocation(
                            "truncated Arm64 instruction".to_owned(),
                        )
                    })?;
                let instruction = u32::from_le_bytes(instruction_bytes);
                let patched = (instruction & 0xfc00_0000) | (((delta >> 2) as u32) & 0x03ff_ffff);
                mapping.bytes_mut()[offset..offset + 4].copy_from_slice(&patched.to_le_bytes());
            }
        }
    }
    Ok(())
}

struct ExecutableMapping {
    address: usize,
    len: usize,
    writable: bool,
}

impl ExecutableMapping {
    #[cfg(unix)]
    fn new(requested: usize) -> Result<Self, NativeCacheError> {
        if requested == 0 {
            return Err(NativeCacheError::InvalidSection(
                "empty code mapping".to_owned(),
            ));
        }
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page <= 0 {
            return Err(NativeCacheError::UnsupportedPlatform);
        }
        let len = align_up(requested, page as usize)?;
        // SAFETY: anonymous private mapping; length was checked and no executable
        // permission is requested while the bytes are writable.
        let pointer = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if pointer == libc::MAP_FAILED {
            return Err(NativeCacheError::Io(std::io::Error::last_os_error()));
        }
        Ok(Self {
            address: pointer as usize,
            len,
            writable: true,
        })
    }

    #[cfg(not(unix))]
    fn new(_requested: usize) -> Result<Self, NativeCacheError> {
        Err(NativeCacheError::UnsupportedPlatform)
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        assert!(self.writable, "RX native cache mapping cannot be mutated");
        // SAFETY: the mapping owns `len` initialized writable bytes.
        unsafe { std::slice::from_raw_parts_mut(self.address as *mut u8, self.len) }
    }

    #[cfg(unix)]
    fn make_executable(&mut self) -> Result<(), NativeCacheError> {
        if !self.writable {
            return Ok(());
        }
        // SAFETY: the mapping is page-aligned and owned; this removes write
        // permission in the same transition that adds execute permission.
        let result = unsafe {
            libc::mprotect(
                self.address as *mut libc::c_void,
                self.len,
                libc::PROT_READ | libc::PROT_EXEC,
            )
        };
        if result != 0 {
            return Err(NativeCacheError::Io(std::io::Error::last_os_error()));
        }
        self.writable = false;
        Ok(())
    }

    #[cfg(not(unix))]
    fn make_executable(&mut self) -> Result<(), NativeCacheError> {
        Err(NativeCacheError::UnsupportedPlatform)
    }
}

impl Drop for ExecutableMapping {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::munmap(self.address as *mut libc::c_void, self.len);
        }
    }
}

struct LockGuard {
    path: PathBuf,
    _file: File,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_lock(path: &Path) -> Result<(LockGuard, bool), NativeCacheError> {
    let started = Instant::now();
    let mut waited = false;
    loop {
        match OpenOptions::new().create_new(true).write(true).open(path) {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())?;
                file.sync_all()?;
                return Ok((
                    LockGuard {
                        path: path.to_owned(),
                        _file: file,
                    },
                    waited,
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                waited = true;
                if lock_owner_is_dead(path)? {
                    match fs::remove_file(path) {
                        Ok(()) => continue,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(error) => return Err(error.into()),
                    }
                }
                if started.elapsed() >= LOCK_TIMEOUT {
                    return Err(NativeCacheError::LockTimeout(path.to_owned()));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn lock_owner_is_dead(path: &Path) -> Result<bool, NativeCacheError> {
    let mut owner = String::new();
    match open_no_follow(path) {
        Ok(mut file) => {
            file.read_to_string(&mut owner)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.into()),
    }
    let Ok(pid) = owner.trim().parse::<u32>() else {
        // Another process may have created the lock but not yet written its
        // owner record. Only an old malformed record is safe to reap.
        let age = fs::metadata(path)?
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .unwrap_or_default();
        return Ok(age >= Duration::from_secs(1));
    };
    #[cfg(unix)]
    {
        let pid = i32::try_from(pid).unwrap_or(i32::MAX);
        // SAFETY: signal 0 performs no mutation; it only probes whether the
        // recorded lock owner still exists or is visible to this process.
        let result = unsafe { libc::kill(pid, 0) };
        if result == 0 {
            return Ok(false);
        }
        Ok(std::io::Error::last_os_error().raw_os_error() != Some(libc::EPERM))
    }
    #[cfg(not(unix))]
    {
        let age = fs::metadata(path)?
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .unwrap_or_default();
        Ok(age >= LOCK_TIMEOUT)
    }
}

fn prepare_cache_directory(path: &Path) -> Result<(), NativeCacheError> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(NativeCacheError::UnsafePath(
            "parent traversal is forbidden".to_owned(),
        ));
    }
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(NativeCacheError::UnsafePath(
            "cache root is not a real directory".to_owned(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: geteuid has no arguments and only reads process credentials.
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(NativeCacheError::UnsafePath(
                "cache root is not owned by the current user".to_owned(),
            ));
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> std::io::Result<File> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "symlink",
        ));
    }
    File::open(path)
}

fn quarantine(path: &Path) -> Result<(), NativeCacheError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    let quarantined = path.with_extension(format!("{}.invalid", std::process::id()));
    match fs::rename(path, quarantined) {
        Ok(()) => Ok(()),
        Err(_) => fs::remove_file(path).map_err(Into::into),
    }
}

fn sync_directory(path: &Path) -> Result<(), NativeCacheError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn enforce_total_cache_limit(
    path: &Path,
    limit: u64,
    incoming: usize,
) -> Result<(), NativeCacheError> {
    let mut total = incoming as u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "pna")
        {
            total = total.saturating_add(metadata.len());
        }
    }
    if total > limit {
        return Err(NativeCacheError::SizeLimit {
            what: "total cache",
            actual: total,
            limit,
        });
    }
    Ok(())
}

fn artifact_checksum(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..24.min(bytes.len())]);
    if bytes.len() > 56 {
        hasher.update([0_u8; 32]);
        hasher.update(&bytes[56..]);
    }
    hasher.finalize().into()
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn align_up(value: usize, alignment: usize) -> Result<usize, NativeCacheError> {
    value
        .checked_add(alignment.saturating_sub(1))
        .map(|value| value & !(alignment - 1))
        .ok_or_else(|| NativeCacheError::InvalidHeader("alignment overflow".to_owned()))
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_string(out: &mut Vec<u8>, value: &str) {
    put_u32(out, value.len() as u32);
    out.extend_from_slice(value.as_bytes());
}

fn encode_functions(values: &[NativeFunctionImage]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        put_u32(&mut out, value.function_id);
        put_u64(&mut out, value.code_offset);
        put_u64(&mut out, value.code_len);
        out.push(value.arity);
        out.push(value.abi as u8);
        out.extend_from_slice(&[0; 2]);
    }
    out
}

fn decode_functions(
    (bytes, count): (&[u8], u32),
) -> Result<Vec<NativeFunctionImage>, NativeCacheError> {
    let mut cursor = Cursor::new(bytes);
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let function_id = cursor.u32()?;
        let code_offset = cursor.u64()?;
        let code_len = cursor.u64()?;
        let arity = cursor.u8()?;
        let abi = NativeFunctionAbi::from_raw(cursor.u8()?)?;
        cursor.take(2)?;
        values.push(NativeFunctionImage {
            function_id,
            code_offset,
            code_len,
            arity,
            abi,
        });
    }
    cursor.finish()?;
    Ok(values)
}

fn encode_continuations(values: &[NativeContinuationEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        put_u32(&mut out, value.function_id);
        put_u32(&mut out, value.continuation_id);
        put_u64(&mut out, value.code_offset);
    }
    out
}

fn decode_continuations(
    (bytes, count): (&[u8], u32),
) -> Result<Vec<NativeContinuationEntry>, NativeCacheError> {
    let mut cursor = Cursor::new(bytes);
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        values.push(NativeContinuationEntry {
            function_id: cursor.u32()?,
            continuation_id: cursor.u32()?,
            code_offset: cursor.u64()?,
        });
    }
    cursor.finish()?;
    Ok(values)
}

fn encode_relocations(values: &[NativeRelocation]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        put_u64(&mut out, value.offset);
        put_u16(&mut out, value.kind as u16);
        match value.target {
            NativeRelocationTarget::Helper(id) => {
                out.push(1);
                out.push(0);
                put_u32(&mut out, id);
            }
            NativeRelocationTarget::InternalSymbol(id) => {
                out.push(2);
                out.push(0);
                put_u32(&mut out, id);
            }
        }
        put_i64(&mut out, value.addend);
    }
    out
}

fn decode_relocations(
    (bytes, count): (&[u8], u32),
) -> Result<Vec<NativeRelocation>, NativeCacheError> {
    let mut cursor = Cursor::new(bytes);
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let offset = cursor.u64()?;
        let kind = NativeRelocationKind::from_raw(cursor.u16()?)?;
        let target_kind = cursor.u8()?;
        cursor.u8()?;
        let target_id = cursor.u32()?;
        let target = match target_kind {
            1 => NativeRelocationTarget::Helper(target_id),
            2 => NativeRelocationTarget::InternalSymbol(target_id),
            _ => {
                return Err(NativeCacheError::InvalidRelocation(
                    "unknown target kind".to_owned(),
                ));
            }
        };
        let addend = cursor.i64()?;
        values.push(NativeRelocation {
            offset,
            kind,
            target,
            addend,
        });
    }
    cursor.finish()?;
    Ok(values)
}

fn encode_helpers(values: &[NativeHelperImport]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        put_u32(&mut out, value.stable_id);
        put_string(&mut out, &value.name);
    }
    out
}

fn decode_helpers(
    (bytes, count): (&[u8], u32),
) -> Result<Vec<NativeHelperImport>, NativeCacheError> {
    let mut cursor = Cursor::new(bytes);
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        values.push(NativeHelperImport {
            stable_id: cursor.u32()?,
            name: cursor.string()?,
        });
    }
    cursor.finish()?;
    Ok(values)
}

fn encode_symbols(values: &[NativeSymbol]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        put_u32(&mut out, value.stable_id);
        put_u32(&mut out, 0);
        put_u64(&mut out, value.code_offset);
    }
    out
}

fn decode_symbols((bytes, count): (&[u8], u32)) -> Result<Vec<NativeSymbol>, NativeCacheError> {
    let mut cursor = Cursor::new(bytes);
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let stable_id = cursor.u32()?;
        cursor.u32()?;
        values.push(NativeSymbol {
            stable_id,
            code_offset: cursor.u64()?,
        });
    }
    cursor.finish()?;
    Ok(values)
}

fn encode_traps(values: &[NativeTrapEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        put_u64(&mut out, value.code_offset);
        put_u16(&mut out, value.trap_code);
        out.extend_from_slice(&[0; 6]);
    }
    out
}

fn decode_traps((bytes, count): (&[u8], u32)) -> Result<Vec<NativeTrapEntry>, NativeCacheError> {
    let mut cursor = Cursor::new(bytes);
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let code_offset = cursor.u64()?;
        let trap_code = cursor.u16()?;
        cursor.take(6)?;
        values.push(NativeTrapEntry {
            code_offset,
            trap_code,
        });
    }
    cursor.finish()?;
    Ok(values)
}

fn encode_exceptions(values: &[NativeExceptionEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        put_u64(&mut out, value.start_offset);
        put_u64(&mut out, value.end_offset);
        put_u64(&mut out, value.handler_offset);
    }
    out
}

fn decode_exceptions(
    (bytes, count): (&[u8], u32),
) -> Result<Vec<NativeExceptionEntry>, NativeCacheError> {
    let mut cursor = Cursor::new(bytes);
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        values.push(NativeExceptionEntry {
            start_offset: cursor.u64()?,
            end_offset: cursor.u64()?,
            handler_offset: cursor.u64()?,
        });
    }
    cursor.finish()?;
    Ok(values)
}

fn encode_root_maps(values: &[NativeRootMap]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        put_u64(&mut out, value.code_offset);
        put_u32(&mut out, value.slots.len() as u32);
        for slot in &value.slots {
            put_u32(&mut out, *slot);
        }
    }
    out
}

fn decode_root_maps((bytes, count): (&[u8], u32)) -> Result<Vec<NativeRootMap>, NativeCacheError> {
    let mut cursor = Cursor::new(bytes);
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let code_offset = cursor.u64()?;
        let slot_count = cursor.u32()? as usize;
        if slot_count > 4096 {
            return Err(NativeCacheError::InvalidSection(
                "root map too large".to_owned(),
            ));
        }
        let mut slots = Vec::with_capacity(slot_count);
        for _ in 0..slot_count {
            slots.push(cursor.u32()?);
        }
        values.push(NativeRootMap { code_offset, slots });
    }
    cursor.finish()?;
    Ok(values)
}

fn encode_resumes(values: &[NativeResumeEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        put_u32(&mut out, value.function_id);
        put_u32(&mut out, value.continuation_id);
        put_i32(&mut out, value.resume_id);
        put_u32(&mut out, 0);
        put_u64(&mut out, value.code_offset);
    }
    out
}

fn decode_resumes(
    (bytes, count): (&[u8], u32),
) -> Result<Vec<NativeResumeEntry>, NativeCacheError> {
    let mut cursor = Cursor::new(bytes);
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let function_id = cursor.u32()?;
        let continuation_id = cursor.u32()?;
        let resume_id = cursor.i32()?;
        cursor.u32()?;
        let code_offset = cursor.u64()?;
        values.push(NativeResumeEntry {
            function_id,
            continuation_id,
            resume_id,
            code_offset,
        });
    }
    cursor.finish()?;
    Ok(values)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], NativeCacheError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| NativeCacheError::InvalidSection("cursor overflow".to_owned()))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| NativeCacheError::InvalidSection("truncated record".to_owned()))?;
        self.offset = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, NativeCacheError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, NativeCacheError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, NativeCacheError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, NativeCacheError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, NativeCacheError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, NativeCacheError> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn string(&mut self) -> Result<String, NativeCacheError> {
        let len = self.u32()? as usize;
        if len > 1024 * 1024 {
            return Err(NativeCacheError::InvalidSection(
                "string too long".to_owned(),
            ));
        }
        String::from_utf8(self.take(len)?.to_vec())
            .map_err(|_| NativeCacheError::InvalidSection("non-UTF-8 string".to_owned()))
    }
    fn finish(self) -> Result<(), NativeCacheError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(NativeCacheError::InvalidSection(
                "trailing record bytes".to_owned(),
            ))
        }
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, NativeCacheError> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| NativeCacheError::InvalidHeader("truncated u16".to_owned()))
}
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, NativeCacheError> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| NativeCacheError::InvalidHeader("truncated u32".to_owned()))
}
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, NativeCacheError> {
    bytes
        .get(offset..offset + 8)
        .and_then(|value| value.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| NativeCacheError::InvalidHeader("truncated u64".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    fn identity(tag: &str) -> NativeCacheIdentity {
        NativeCacheIdentity {
            source_hash: format!("source-{tag}"),
            ir_hash: format!("ir-{tag}"),
            dependency_graph_hash: "deps".to_owned(),
            build_id: "build".to_owned(),
            cranelift_version: crate::CRANELIFT_VERSION.to_owned(),
            cranelift_settings_hash: 1,
            region_ir_schema_version: crate::region_ir::REGION_IR_SCHEMA_VERSION,
            runtime_abi_hash: crate::JIT_RUNTIME_ABI_HASH,
            helper_abi_hash: crate::JIT_HELPER_REGISTRY_ABI_HASH,
            target_triple: std::env::consts::ARCH.to_owned(),
            pointer_width: usize::BITS as u8,
            cpu_feature_fingerprint: 2,
            optimization_tier: "baseline".to_owned(),
            optimization_config_hash: 3,
            php_semantic_config_hash: 4,
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn returning_image(tag: &str) -> NativeArtifactImage {
        // System-V: out pointer is rdi. This matches the zero-arity status/out
        // native entry ABI and is used only to validate the loader itself.
        let code = vec![
            0x48,
            0xc7,
            0x07,
            0x2a,
            0x00,
            0x00,
            0x00, // mov qword [rdi], 42
            0xb8,
            crate::JitCallStatus::RETURN.0 as u8,
            0,
            0,
            0,    // mov eax, RETURN
            0xc3, // ret
        ];
        NativeArtifactImage::minimal(
            identity(tag),
            code.clone(),
            NativeFunctionImage {
                function_id: 0,
                code_offset: 0,
                code_len: code.len() as u64,
                arity: 0,
                abi: NativeFunctionAbi::I64StatusOut,
            },
        )
    }

    fn temp_cache(tag: &str, mode: NativeCacheMode) -> (PathBuf, NativeArtifactCache) {
        let path = std::env::temp_dir().join(format!(
            "phrust-pna-test-{}-{}-{tag}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cache = NativeArtifactCache::new(NativeCacheConfig {
            mode,
            directory: path.clone(),
            ..NativeCacheConfig::default()
        })
        .unwrap();
        (path, cache)
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn pna_roundtrip_maps_rx_and_executes_after_reload() {
        let (path, cache) = temp_cache("roundtrip", NativeCacheMode::ReadWrite);
        let expected = identity("roundtrip");
        let (artifact, event) = cache
            .get_or_compile(&expected, |_| None, || Ok(returning_image("roundtrip")))
            .unwrap();
        assert_eq!(event, NativeCacheEvent::Written);
        assert_eq!(artifact.invoke_i64_status_out(0).unwrap(), 42);
        assert!(!artifact.writable_and_executable());
        drop(artifact);
        let second = NativeArtifactCache::new(NativeCacheConfig {
            mode: NativeCacheMode::Read,
            directory: path.clone(),
            ..NativeCacheConfig::default()
        })
        .unwrap();
        let loaded = second.load(&expected, |_| None).unwrap().unwrap();
        assert_eq!(loaded.invoke_i64_status_out(0).unwrap(), 42);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn checksum_corruption_and_truncation_are_never_loaded() {
        for (tag, mutate) in [("checksum", 80_usize), ("truncated", usize::MAX)] {
            let (path, cache) = temp_cache(tag, NativeCacheMode::ReadWrite);
            let expected = identity(tag);
            cache
                .get_or_compile(&expected, |_| None, || Ok(returning_image(tag)))
                .unwrap();
            let artifact_path = cache.artifact_path(&expected);
            let mut bytes = fs::read(&artifact_path).unwrap();
            if mutate == usize::MAX {
                bytes.truncate(bytes.len() - 1);
            } else {
                bytes[mutate] ^= 0x5a;
            }
            fs::write(&artifact_path, bytes).unwrap();
            assert!(cache.load(&expected, |_| None).is_err());
            assert!(!artifact_path.exists());
            fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn identity_dimensions_invalidate_artifacts() {
        let (path, cache) = temp_cache("identity", NativeCacheMode::ReadWrite);
        let expected = identity("identity");
        cache
            .get_or_compile(&expected, |_| None, || Ok(returning_image("identity")))
            .unwrap();
        let source_path = cache.artifact_path(&expected);
        let mut foreign_identities = Vec::new();
        macro_rules! changed {
            ($field:ident, $value:expr) => {{
                let mut foreign = expected.clone();
                foreign.$field = $value;
                foreign_identities.push(foreign);
            }};
        }
        changed!(source_hash, "changed-source".to_owned());
        changed!(ir_hash, "changed-ir".to_owned());
        changed!(dependency_graph_hash, "changed-deps".to_owned());
        changed!(build_id, "changed-build".to_owned());
        changed!(cranelift_version, "changed-cranelift".to_owned());
        changed!(cranelift_settings_hash, 101);
        changed!(region_ir_schema_version, 102);
        changed!(runtime_abi_hash, 103);
        changed!(helper_abi_hash, 104);
        changed!(target_triple, "changed-target".to_owned());
        changed!(pointer_width, 32);
        changed!(cpu_feature_fingerprint, 105);
        changed!(optimization_tier, "optimizing".to_owned());
        changed!(optimization_config_hash, 106);
        changed!(php_semantic_config_hash, 107);
        for foreign in foreign_identities {
            let foreign_path = cache.artifact_path(&foreign);
            fs::copy(&source_path, &foreign_path).unwrap();
            assert!(matches!(
                cache.load(&foreign, |_| None),
                Err(NativeCacheError::IdentityMismatch)
            ));
        }
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn concurrent_writers_compile_exactly_once() {
        let path = std::env::temp_dir().join(format!("phrust-pna-race-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        let identity = identity("race");
        let compiles = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::new();
        for _ in 0..4 {
            let path = path.clone();
            let identity = identity.clone();
            let compiles = Arc::clone(&compiles);
            workers.push(std::thread::spawn(move || {
                let cache = NativeArtifactCache::new(NativeCacheConfig {
                    mode: NativeCacheMode::ReadWrite,
                    directory: path,
                    ..NativeCacheConfig::default()
                })
                .unwrap();
                cache
                    .get_or_compile(
                        &identity,
                        |_| None,
                        || {
                            compiles.fetch_add(1, Ordering::SeqCst);
                            Ok(returning_image("race"))
                        },
                    )
                    .unwrap()
                    .0
                    .invoke_i64_status_out(0)
                    .unwrap()
            }));
        }
        for worker in workers {
            assert_eq!(worker.join().unwrap(), 42);
        }
        assert_eq!(compiles.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn invalid_relocation_unknown_helper_and_path_traversal_are_rejected() {
        #[cfg(target_arch = "x86_64")]
        {
            let mut image = returning_image("invalid-reloc");
            image.relocations.push(NativeRelocation {
                offset: image.code.len() as u64,
                kind: NativeRelocationKind::Abs64,
                target: NativeRelocationTarget::Helper(999),
                addend: 0,
            });
            assert!(matches!(
                validate_image(&image, &NativeCacheConfig::default()),
                Err(NativeCacheError::InvalidSection(_))
            ));
            image.relocations[0].offset = 0;
            assert!(matches!(
                validate_image(&image, &NativeCacheConfig::default()),
                Err(NativeCacheError::UnknownHelper(999))
            ));
        }
        let error = NativeArtifactCache::new(NativeCacheConfig {
            mode: NativeCacheMode::ReadWrite,
            directory: PathBuf::from("../escape"),
            ..NativeCacheConfig::default()
        })
        .unwrap_err();
        assert!(matches!(error, NativeCacheError::UnsafePath(_)));
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn unknown_serialized_relocation_kind_and_unresolved_helper_are_rejected() {
        let mut image = returning_image("serialized-relocation");
        let helper = crate::JIT_HELPER_SYMBOLS[0];
        image.helper_imports.push(NativeHelperImport {
            stable_id: helper.id.0,
            name: helper.name.to_owned(),
        });
        image.relocations.push(NativeRelocation {
            offset: 0,
            kind: NativeRelocationKind::Abs64,
            target: NativeRelocationTarget::Helper(helper.id.0),
            addend: 0,
        });
        let config = NativeCacheConfig::default();
        let mut bytes = encode_artifact(&image, &config).unwrap();
        let relocation_record = HEADER_LEN + 5 * SECTION_RECORD_LEN;
        let relocation_offset = read_u64(&bytes, relocation_record + 8).unwrap() as usize;
        bytes[relocation_offset + 8..relocation_offset + 10]
            .copy_from_slice(&u16::MAX.to_le_bytes());
        let checksum = artifact_checksum(&bytes);
        bytes[24..56].copy_from_slice(&checksum);
        assert!(matches!(
            decode_artifact(&bytes, &image.identity, &config),
            Err(NativeCacheError::InvalidRelocation(_))
        ));

        let error = NativeLoadedArtifact::map(image, &|_| None).unwrap_err();
        assert!(matches!(
            error,
            NativeCacheError::UnknownHelper(id) if id == helper.id.0
        ));
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn killed_writer_lock_and_partial_temp_are_recovered() {
        let (path, cache) = temp_cache("killed-writer", NativeCacheMode::ReadWrite);
        let expected = identity("killed-writer");
        fs::write(cache.lock_path(&expected), b"4294967294\n").unwrap();
        fs::write(path.join(".interrupted-write.tmp"), b"PNA1-partial").unwrap();
        let (artifact, event) = cache
            .get_or_compile(&expected, |_| None, || Ok(returning_image("killed-writer")))
            .unwrap();
        assert_eq!(event, NativeCacheEvent::Written);
        assert_eq!(artifact.invoke_i64_status_out(0).unwrap(), 42);
        assert!(path.join(".interrupted-write.tmp").exists());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    #[cfg(all(unix, target_arch = "x86_64"))]
    fn symlink_artifact_is_not_followed() {
        use std::os::unix::fs::symlink;
        let (path, cache) = temp_cache("symlink", NativeCacheMode::Read);
        let expected = identity("symlink");
        let outside = path.join("outside");
        fs::write(&outside, b"not an artifact").unwrap();
        symlink(&outside, cache.artifact_path(&expected)).unwrap();
        assert!(cache.load(&expected, |_| None).is_err());
        fs::remove_dir_all(path).unwrap();
    }
}
