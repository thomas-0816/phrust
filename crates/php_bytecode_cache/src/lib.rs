//! performance bytecode-cache metadata envelope.
//!
//! The cache format is documented in `docs/adr/0015-bytecode-cache-format.md`.
//! This crate intentionally serializes only the untrusted header/metadata
//! envelope. It does not serialize VM bytecode or change runtime behavior.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use php_ir::{IrUnit, VerificationError, verify_unit};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current bytecode-cache format version supported by this reader/writer.
pub const CURRENT_CACHE_FORMAT_VERSION: u16 = 1;

/// PHP compatibility target for performance bytecode-cache artifacts.
pub const PHP_TARGET_VERSION: &str = "8.5.7";

/// performance frontend fingerprint version marker.
pub const FRONTEND_FORMAT_VERSION: &str = "performance-frontend-1";

/// performance cache metadata fingerprint version marker.
pub const CACHE_FINGERPRINT_VERSION: &str = "performance-cache-fingerprint-1";

/// runtime IR version currently consumed by the VM.
pub const IR_FORMAT_VERSION: &str = "runtime-ir-1";

/// Magic bytes for project-owned bytecode-cache artifacts.
pub const CACHE_MAGIC: [u8; 8] = *b"PHRBC\0\0\x01";

const ENVELOPE_FIXED_LEN: usize = 8 + 2 + 4 + 8 + 4 + 4;

/// Version marker for the cache envelope and metadata schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CacheFormatVersion(u16);

impl CacheFormatVersion {
    /// Returns the current format version.
    #[must_use]
    pub const fn current() -> Self {
        Self(CURRENT_CACHE_FORMAT_VERSION)
    }

    /// Creates a version from raw metadata.
    #[must_use]
    pub const fn new(version: u16) -> Self {
        Self(version)
    }

    /// Returns the raw numeric version.
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }
}

impl Default for CacheFormatVersion {
    fn default() -> Self {
        Self::current()
    }
}

/// Compatibility fingerprints that must match before a payload can be used.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CacheFingerprint {
    /// Hash or stable identifier for the current source bytes.
    pub source: String,
    /// Hash of frontend, semantic, IR, feature, and optimization inputs.
    pub compiler: String,
    /// Hash of relevant INI and runtime configuration.
    pub config: String,
    /// Full digest over all fingerprint dimensions.
    pub digest: String,
    /// Optional canonical source path or source identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Engine crate or workspace version.
    pub engine_version: String,
    /// Engine git commit when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_git_commit: Option<String>,
    /// PHP compatibility target.
    pub php_target_version: String,
    /// Frontend format marker.
    pub frontend_format_version: String,
    /// Cache metadata format version.
    pub cache_format_version: CacheFormatVersion,
    /// IR format marker.
    pub ir_format_version: String,
    /// Optimization level used to produce the cached artifact.
    pub opt_level: String,
    /// Sorted engine feature flags.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub feature_flags: BTreeMap<String, bool>,
    /// Rust target triple.
    pub target_triple: String,
    /// Sorted INI configuration values that influence compile/runtime behavior.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ini_config: BTreeMap<String, String>,
    /// Sorted runtime configuration values that influence compile/runtime behavior.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub runtime_config: BTreeMap<String, String>,
    /// Additional deterministic components reserved for later performance work.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

impl CacheFingerprint {
    /// Creates a minimal deterministic fingerprint record.
    #[must_use]
    pub fn new(
        source: impl Into<String>,
        compiler: impl Into<String>,
        config: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            compiler: compiler.into(),
            config: config.into(),
            digest: String::new(),
            source_path: None,
            engine_version: String::new(),
            engine_git_commit: None,
            php_target_version: PHP_TARGET_VERSION.to_owned(),
            frontend_format_version: FRONTEND_FORMAT_VERSION.to_owned(),
            cache_format_version: CacheFormatVersion::current(),
            ir_format_version: IR_FORMAT_VERSION.to_owned(),
            opt_level: "0".to_owned(),
            feature_flags: BTreeMap::new(),
            target_triple: String::new(),
            ini_config: BTreeMap::new(),
            runtime_config: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }

    /// Builds a robust deterministic fingerprint from cache-relevant inputs.
    pub fn from_inputs(input: CacheFingerprintInput) -> Result<Self, CacheStoreError> {
        let source_hash = sha256_hex(&input.source_bytes);
        let compiler_material = CompilerFingerprintMaterial {
            engine_version: &input.engine_version,
            engine_git_commit: input.engine_git_commit.as_deref(),
            php_target_version: &input.php_target_version,
            frontend_format_version: &input.frontend_format_version,
            cache_format_version: input.cache_format_version.as_u16(),
            ir_format_version: &input.ir_format_version,
            opt_level: &input.opt_level,
            feature_flags: &input.feature_flags,
            target_triple: &input.target_triple,
            extra: &input.extra,
        };
        let compiler_hash = sha256_json(&compiler_material)?;
        let config_material = ConfigFingerprintMaterial {
            ini_config: &input.ini_config,
            runtime_config: &input.runtime_config,
        };
        let config_hash = sha256_json(&config_material)?;

        let digest_material = FullFingerprintMaterial {
            source: &source_hash,
            source_path: input.source_path.as_deref(),
            compiler: &compiler_hash,
            config: &config_hash,
        };
        let digest = sha256_json(&digest_material)?;

        Ok(Self {
            source: source_hash,
            compiler: compiler_hash,
            config: config_hash,
            digest,
            source_path: input.source_path,
            engine_version: input.engine_version,
            engine_git_commit: input.engine_git_commit,
            php_target_version: input.php_target_version,
            frontend_format_version: input.frontend_format_version,
            cache_format_version: input.cache_format_version,
            ir_format_version: input.ir_format_version,
            opt_level: input.opt_level,
            feature_flags: input.feature_flags,
            target_triple: input.target_triple,
            ini_config: input.ini_config,
            runtime_config: input.runtime_config,
            extra: input.extra,
        })
    }

    /// Serializes this fingerprint as stable pretty JSON with a trailing newline.
    pub fn to_stable_json(&self) -> Result<String, CacheStoreError> {
        let mut json =
            serde_json::to_string_pretty(self).map_err(CacheStoreError::MetadataEncode)?;
        json.push('\n');
        Ok(json)
    }

    /// Formats the most important fingerprint dimensions for diagnostics.
    #[must_use]
    pub fn to_debug_text(&self) -> String {
        let mut out = String::new();
        out.push_str("cache_fingerprint\n");
        out.push_str(&format!("digest={}\n", self.digest));
        out.push_str(&format!("source={}\n", self.source));
        if let Some(path) = &self.source_path {
            out.push_str(&format!("source_path={path}\n"));
        }
        out.push_str(&format!("compiler={}\n", self.compiler));
        out.push_str(&format!("config={}\n", self.config));
        out.push_str(&format!("engine_version={}\n", self.engine_version));
        if let Some(commit) = &self.engine_git_commit {
            out.push_str(&format!("engine_git_commit={commit}\n"));
        }
        out.push_str(&format!("php_target_version={}\n", self.php_target_version));
        out.push_str(&format!("opt_level={}\n", self.opt_level));
        out.push_str(&format!("target_triple={}\n", self.target_triple));
        out
    }

    /// Adds an optional canonical source path.
    #[must_use]
    pub fn with_source_path(mut self, source_path: impl Into<String>) -> Self {
        self.source_path = Some(source_path.into());
        self
    }

    /// Adds an extra sorted fingerprint component.
    #[must_use]
    pub fn with_extra(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(name.into(), value.into());
        self
    }
}

/// Input dimensions used to compute a cache fingerprint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheFingerprintInput {
    /// Source bytes to hash.
    pub source_bytes: Vec<u8>,
    /// Optional canonical source path or stable source identity.
    pub source_path: Option<String>,
    /// Engine crate or workspace version.
    pub engine_version: String,
    /// Engine git commit when available.
    pub engine_git_commit: Option<String>,
    /// PHP compatibility target.
    pub php_target_version: String,
    /// Frontend format marker.
    pub frontend_format_version: String,
    /// Cache metadata format marker.
    pub cache_format_version: CacheFormatVersion,
    /// IR format marker.
    pub ir_format_version: String,
    /// Optimization level.
    pub opt_level: String,
    /// Sorted feature flags.
    pub feature_flags: BTreeMap<String, bool>,
    /// Rust target triple.
    pub target_triple: String,
    /// Sorted INI config values.
    pub ini_config: BTreeMap<String, String>,
    /// Sorted runtime config values.
    pub runtime_config: BTreeMap<String, String>,
    /// Extra deterministic components.
    pub extra: BTreeMap<String, String>,
}

impl CacheFingerprintInput {
    /// Creates fingerprint input with performance defaults.
    #[must_use]
    pub fn new(
        source_bytes: impl Into<Vec<u8>>,
        engine_version: impl Into<String>,
        target_triple: impl Into<String>,
    ) -> Self {
        Self {
            source_bytes: source_bytes.into(),
            source_path: None,
            engine_version: engine_version.into(),
            engine_git_commit: None,
            php_target_version: PHP_TARGET_VERSION.to_owned(),
            frontend_format_version: FRONTEND_FORMAT_VERSION.to_owned(),
            cache_format_version: CacheFormatVersion::current(),
            ir_format_version: IR_FORMAT_VERSION.to_owned(),
            opt_level: "0".to_owned(),
            feature_flags: BTreeMap::new(),
            target_triple: target_triple.into(),
            ini_config: BTreeMap::new(),
            runtime_config: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }

    /// Adds a normalized source path.
    #[must_use]
    pub fn with_source_path(mut self, source_path: impl Into<String>) -> Self {
        self.source_path = Some(source_path.into());
        self
    }

    /// Adds an engine git commit.
    #[must_use]
    pub fn with_engine_git_commit(mut self, engine_git_commit: impl Into<String>) -> Self {
        self.engine_git_commit = Some(engine_git_commit.into());
        self
    }

    /// Sets the optimization level.
    #[must_use]
    pub fn with_opt_level(mut self, opt_level: impl Into<String>) -> Self {
        self.opt_level = opt_level.into();
        self
    }

    /// Adds or updates a feature flag.
    #[must_use]
    pub fn with_feature_flag(mut self, name: impl Into<String>, enabled: bool) -> Self {
        self.feature_flags.insert(name.into(), enabled);
        self
    }

    /// Adds or updates an INI config value.
    #[must_use]
    pub fn with_ini_config(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.ini_config.insert(name.into(), value.into());
        self
    }

    /// Adds or updates a runtime config value.
    #[must_use]
    pub fn with_runtime_config(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.runtime_config.insert(name.into(), value.into());
        self
    }

    /// Adds or updates an extra deterministic component.
    #[must_use]
    pub fn with_extra(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(name.into(), value.into());
        self
    }
}

#[derive(Serialize)]
struct CompilerFingerprintMaterial<'a> {
    engine_version: &'a str,
    engine_git_commit: Option<&'a str>,
    php_target_version: &'a str,
    frontend_format_version: &'a str,
    cache_format_version: u16,
    ir_format_version: &'a str,
    opt_level: &'a str,
    feature_flags: &'a BTreeMap<String, bool>,
    target_triple: &'a str,
    extra: &'a BTreeMap<String, String>,
}

#[derive(Serialize)]
struct ConfigFingerprintMaterial<'a> {
    ini_config: &'a BTreeMap<String, String>,
    runtime_config: &'a BTreeMap<String, String>,
}

#[derive(Serialize)]
struct FullFingerprintMaterial<'a> {
    source: &'a str,
    source_path: Option<&'a str>,
    compiler: &'a str,
    config: &'a str,
}

/// Metadata header decoded before any cache payload can be trusted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CacheHeader {
    /// Cache format version.
    pub format_version: CacheFormatVersion,
    /// Engine crate or workspace version that wrote the artifact.
    pub engine_version: String,
    /// PHP target version. performance accepts only `8.5.7`.
    pub php_target_version: String,
    /// Engine ABI marker for runtime/IR compatibility.
    pub abi_version: String,
    /// Writer endianness.
    pub endianness: String,
    /// Writer target triple.
    pub target_triple: String,
    /// Source, compiler, and config fingerprints.
    pub fingerprint: CacheFingerprint,
    /// Include/require dependencies known when the artifact was written.
    pub dependencies: Vec<CacheDependency>,
    /// Tool or crate label that wrote the artifact.
    pub created_with: String,
}

impl CacheHeader {
    /// Creates a performance header for the current cache format.
    #[must_use]
    pub fn new(
        engine_version: impl Into<String>,
        abi_version: impl Into<String>,
        target_triple: impl Into<String>,
        fingerprint: CacheFingerprint,
    ) -> Self {
        Self {
            format_version: CacheFormatVersion::current(),
            engine_version: engine_version.into(),
            php_target_version: PHP_TARGET_VERSION.to_owned(),
            abi_version: abi_version.into(),
            endianness: native_endianness().to_owned(),
            target_triple: target_triple.into(),
            fingerprint,
            dependencies: Vec::new(),
            created_with: concat!("php_bytecode_cache/", env!("CARGO_PKG_VERSION")).to_owned(),
        }
    }

    /// Adds a dependency entry.
    #[must_use]
    pub fn with_dependency(mut self, dependency: CacheDependency) -> Self {
        self.dependencies.push(dependency);
        self
    }

    /// Serializes the header metadata to deterministic pretty JSON.
    pub fn to_metadata_json(&self) -> Result<String, CacheStoreError> {
        let mut json =
            serde_json::to_string_pretty(self).map_err(CacheStoreError::MetadataEncode)?;
        json.push('\n');
        Ok(json)
    }
}

/// One known include/require dependency recorded in a cache header.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CacheDependency {
    /// Resolved dependency path or stable identity.
    pub path: String,
    /// Dependency source fingerprint.
    pub fingerprint: String,
    /// Resolution mode such as `include`, `require`, or `autoload`.
    pub resolution: String,
}

impl CacheDependency {
    /// Creates a dependency metadata record.
    #[must_use]
    pub fn new(
        path: impl Into<String>,
        fingerprint: impl Into<String>,
        resolution: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            fingerprint: fingerprint.into(),
            resolution: resolution.into(),
        }
    }
}

/// Header plus opaque payload bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheArtifact {
    /// Validated cache metadata header.
    pub header: CacheHeader,
    /// Opaque payload bytes. performance metadata tests keep this empty.
    pub payload: Vec<u8>,
}

impl CacheArtifact {
    /// Creates an artifact from a header and payload bytes.
    #[must_use]
    pub fn new(header: CacheHeader, payload: Vec<u8>) -> Self {
        Self { header, payload }
    }

    /// Serializes a verified IR unit into a cache artifact payload.
    pub fn from_ir_unit(header: CacheHeader, unit: &IrUnit) -> Result<Self, CacheStoreError> {
        verify_unit(unit).map_err(|errors| CacheStoreError::PayloadVerification {
            diagnostics: verification_diagnostics(&errors),
        })?;
        let payload = serde_json::to_vec(unit).map_err(CacheStoreError::PayloadEncode)?;
        Ok(Self::new(header, payload))
    }

    /// Loads a cache artifact as a verified IR unit.
    pub fn load_ir_unit(
        bytes: &[u8],
        expected_target_triple: &str,
        expected_fingerprint: &CacheFingerprint,
    ) -> Result<CachedIrArtifact, CacheLoadError> {
        let artifact = Self::from_bytes(bytes, expected_target_triple)?;
        if artifact.header.fingerprint.digest != expected_fingerprint.digest {
            return Err(CacheLoadError::FingerprintMismatch {
                expected: expected_fingerprint.digest.clone(),
                actual: artifact.header.fingerprint.digest.clone(),
            });
        }
        let unit: IrUnit =
            serde_json::from_slice(&artifact.payload).map_err(CacheLoadError::PayloadDecode)?;
        verify_unit(&unit).map_err(|errors| CacheLoadError::PayloadVerification {
            diagnostics: verification_diagnostics(&errors),
        })?;
        Ok(CachedIrArtifact {
            header: artifact.header,
            unit,
        })
    }

    /// Serializes an artifact into the performance cache envelope.
    pub fn to_bytes(&self) -> Result<Vec<u8>, CacheStoreError> {
        if self.header.format_version.as_u16() != CURRENT_CACHE_FORMAT_VERSION {
            return Err(CacheStoreError::UnsupportedFormatVersion {
                version: self.header.format_version.as_u16(),
                current: CURRENT_CACHE_FORMAT_VERSION,
            });
        }

        let metadata = self.header.to_metadata_json()?.into_bytes();
        let header_len =
            u32::try_from(metadata.len()).map_err(|_| CacheStoreError::HeaderTooLarge {
                len: metadata.len(),
            })?;
        let payload_len =
            u64::try_from(self.payload.len()).map_err(|_| CacheStoreError::PayloadTooLarge {
                len: self.payload.len(),
            })?;
        let header_checksum = crc32fast::hash(&metadata);
        let payload_checksum = crc32fast::hash(&self.payload);

        let mut bytes = Vec::with_capacity(
            ENVELOPE_FIXED_LEN
                .saturating_add(metadata.len())
                .saturating_add(self.payload.len()),
        );
        bytes.extend_from_slice(&CACHE_MAGIC);
        bytes.extend_from_slice(&CURRENT_CACHE_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&header_len.to_le_bytes());
        bytes.extend_from_slice(&payload_len.to_le_bytes());
        bytes.extend_from_slice(&header_checksum.to_le_bytes());
        bytes.extend_from_slice(&payload_checksum.to_le_bytes());
        bytes.extend_from_slice(&metadata);
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }

    /// Deserializes an artifact and validates its envelope metadata.
    pub fn from_bytes(bytes: &[u8], expected_target_triple: &str) -> Result<Self, CacheLoadError> {
        if bytes.len() < ENVELOPE_FIXED_LEN {
            return Err(CacheLoadError::Truncated {
                len: bytes.len(),
                min: ENVELOPE_FIXED_LEN,
            });
        }

        if bytes[..8] != CACHE_MAGIC {
            return Err(CacheLoadError::BadMagic);
        }

        let version = read_u16(bytes, 8)?;
        if version > CURRENT_CACHE_FORMAT_VERSION {
            return Err(CacheLoadError::UnknownFutureFormat {
                version,
                current: CURRENT_CACHE_FORMAT_VERSION,
            });
        }
        if version != CURRENT_CACHE_FORMAT_VERSION {
            return Err(CacheLoadError::UnsupportedFormatVersion {
                version,
                current: CURRENT_CACHE_FORMAT_VERSION,
            });
        }

        let header_len = read_u32(bytes, 10)? as usize;
        let payload_len_u64 = read_u64(bytes, 14)?;
        let payload_len =
            usize::try_from(payload_len_u64).map_err(|_| CacheLoadError::LengthOverflow {
                field: "payload",
                len: payload_len_u64,
            })?;
        let header_checksum = read_u32(bytes, 22)?;
        let payload_checksum = read_u32(bytes, 26)?;

        let metadata_start = ENVELOPE_FIXED_LEN;
        let payload_start =
            metadata_start
                .checked_add(header_len)
                .ok_or(CacheLoadError::LengthOverflow {
                    field: "header",
                    len: header_len as u64,
                })?;
        let total_len =
            payload_start
                .checked_add(payload_len)
                .ok_or(CacheLoadError::LengthOverflow {
                    field: "payload",
                    len: payload_len_u64,
                })?;
        if total_len != bytes.len() {
            return Err(CacheLoadError::LengthMismatch {
                declared: total_len,
                actual: bytes.len(),
            });
        }

        let metadata = &bytes[metadata_start..payload_start];
        let payload = &bytes[payload_start..total_len];
        let actual_header_checksum = crc32fast::hash(metadata);
        if actual_header_checksum != header_checksum {
            return Err(CacheLoadError::ChecksumMismatch {
                section: "header",
                expected: header_checksum,
                actual: actual_header_checksum,
            });
        }
        let actual_payload_checksum = crc32fast::hash(payload);
        if actual_payload_checksum != payload_checksum {
            return Err(CacheLoadError::ChecksumMismatch {
                section: "payload",
                expected: payload_checksum,
                actual: actual_payload_checksum,
            });
        }

        let header: CacheHeader =
            serde_json::from_slice(metadata).map_err(CacheLoadError::MetadataDecode)?;
        if header.format_version.as_u16() != version {
            return Err(CacheLoadError::MetadataVersionMismatch {
                envelope: version,
                metadata: header.format_version.as_u16(),
            });
        }
        if header.php_target_version != PHP_TARGET_VERSION {
            return Err(CacheLoadError::PhpTargetMismatch {
                expected: PHP_TARGET_VERSION.to_owned(),
                actual: header.php_target_version,
            });
        }
        if header.target_triple != expected_target_triple {
            return Err(CacheLoadError::TargetMismatch {
                expected: expected_target_triple.to_owned(),
                actual: header.target_triple,
            });
        }

        Ok(Self {
            header,
            payload: payload.to_vec(),
        })
    }
}

/// Verified IR payload loaded from a cache artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct CachedIrArtifact {
    /// Validated cache metadata header.
    pub header: CacheHeader,
    /// Deserialized and verifier-approved IR unit.
    pub unit: IrUnit,
}

/// Typed cache load errors. Cache input is untrusted and must not panic.
#[derive(Debug)]
pub enum CacheLoadError {
    /// Input is shorter than the fixed envelope.
    Truncated { len: usize, min: usize },
    /// Magic bytes do not identify a bytecode-cache artifact.
    BadMagic,
    /// Format version is known to be unsupported.
    UnsupportedFormatVersion { version: u16, current: u16 },
    /// Format version is newer than this reader.
    UnknownFutureFormat { version: u16, current: u16 },
    /// A declared length cannot fit in memory on this target.
    LengthOverflow { field: &'static str, len: u64 },
    /// Declared envelope length does not match the input length.
    LengthMismatch { declared: usize, actual: usize },
    /// Header or payload checksum mismatch.
    ChecksumMismatch {
        section: &'static str,
        expected: u32,
        actual: u32,
    },
    /// Metadata JSON failed to decode.
    MetadataDecode(serde_json::Error),
    /// Payload JSON failed to decode.
    PayloadDecode(serde_json::Error),
    /// Loaded payload failed IR verification.
    PayloadVerification { diagnostics: Vec<String> },
    /// Envelope and metadata versions diverge.
    MetadataVersionMismatch { envelope: u16, metadata: u16 },
    /// Artifact fingerprint does not match current source/config inputs.
    FingerprintMismatch { expected: String, actual: String },
    /// Artifact targets an incompatible PHP version.
    PhpTargetMismatch { expected: String, actual: String },
    /// Artifact targets an incompatible Rust target triple.
    TargetMismatch { expected: String, actual: String },
}

impl fmt::Display for CacheLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { len, min } => {
                write!(
                    formatter,
                    "cache input is truncated: {len} bytes, need {min}"
                )
            }
            Self::BadMagic => write!(formatter, "cache magic bytes do not match"),
            Self::UnsupportedFormatVersion { version, current } => write!(
                formatter,
                "unsupported cache format version {version}; current version is {current}"
            ),
            Self::UnknownFutureFormat { version, current } => write!(
                formatter,
                "future cache format version {version}; current version is {current}"
            ),
            Self::LengthOverflow { field, len } => {
                write!(
                    formatter,
                    "cache {field} length {len} overflows this target"
                )
            }
            Self::LengthMismatch { declared, actual } => write!(
                formatter,
                "cache declared length {declared} bytes does not match input length {actual}"
            ),
            Self::ChecksumMismatch {
                section,
                expected,
                actual,
            } => write!(
                formatter,
                "cache {section} checksum mismatch: expected {expected:#010x}, got {actual:#010x}"
            ),
            Self::MetadataDecode(error) => {
                write!(formatter, "cache metadata decode failed: {error}")
            }
            Self::PayloadDecode(error) => write!(formatter, "cache payload decode failed: {error}"),
            Self::PayloadVerification { diagnostics } => write!(
                formatter,
                "cache payload failed IR verification: {} error(s)",
                diagnostics.len()
            ),
            Self::MetadataVersionMismatch { envelope, metadata } => write!(
                formatter,
                "cache envelope version {envelope} differs from metadata version {metadata}"
            ),
            Self::FingerprintMismatch { expected, actual } => write!(
                formatter,
                "cache fingerprint mismatch: expected {expected}, got {actual}"
            ),
            Self::PhpTargetMismatch { expected, actual } => write!(
                formatter,
                "cache PHP target mismatch: expected {expected}, got {actual}"
            ),
            Self::TargetMismatch { expected, actual } => write!(
                formatter,
                "cache target mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl Error for CacheLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MetadataDecode(error) | Self::PayloadDecode(error) => Some(error),
            _ => None,
        }
    }
}

/// Typed cache store errors.
#[derive(Debug)]
pub enum CacheStoreError {
    /// Header metadata failed to encode.
    MetadataEncode(serde_json::Error),
    /// IR payload failed to encode.
    PayloadEncode(serde_json::Error),
    /// Payload failed verification before store.
    PayloadVerification { diagnostics: Vec<String> },
    /// Writer attempted to emit an unsupported format version.
    UnsupportedFormatVersion { version: u16, current: u16 },
    /// Metadata block exceeds the envelope length field.
    HeaderTooLarge { len: usize },
    /// Payload block exceeds the envelope length field on this target.
    PayloadTooLarge { len: usize },
}

impl fmt::Display for CacheStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MetadataEncode(error) => {
                write!(formatter, "cache metadata encode failed: {error}")
            }
            Self::PayloadEncode(error) => write!(formatter, "cache payload encode failed: {error}"),
            Self::PayloadVerification { diagnostics } => write!(
                formatter,
                "cache payload failed IR verification before store: {} error(s)",
                diagnostics.len()
            ),
            Self::UnsupportedFormatVersion { version, current } => write!(
                formatter,
                "cannot store cache format version {version}; writer supports {current}"
            ),
            Self::HeaderTooLarge { len } => {
                write!(formatter, "cache metadata header is too large: {len} bytes")
            }
            Self::PayloadTooLarge { len } => {
                write!(formatter, "cache payload is too large: {len} bytes")
            }
        }
    }
}

impl Error for CacheStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MetadataEncode(error) | Self::PayloadEncode(error) => Some(error),
            _ => None,
        }
    }
}

#[must_use]
fn native_endianness() -> &'static str {
    if cfg!(target_endian = "little") {
        "little"
    } else {
        "big"
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, CacheLoadError> {
    bytes
        .get(offset..offset + 2)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(CacheLoadError::Truncated {
            len: bytes.len(),
            min: offset + 2,
        })
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, CacheLoadError> {
    bytes
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(CacheLoadError::Truncated {
            len: bytes.len(),
            min: offset + 4,
        })
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, CacheLoadError> {
    bytes
        .get(offset..offset + 8)
        .and_then(|slice| slice.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(CacheLoadError::Truncated {
            len: bytes.len(),
            min: offset + 8,
        })
}

fn sha256_json(value: &impl Serialize) -> Result<String, CacheStoreError> {
    let bytes = serde_json::to_vec(value).map_err(CacheStoreError::MetadataEncode)?;
    Ok(sha256_hex(&bytes))
}

#[must_use]
fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(hex_nibble(byte >> 4));
        out.push(hex_nibble(byte & 0x0f));
    }
    out
}

#[must_use]
fn hex_nibble(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        10..=15 => char::from(b'a' + (nibble - 10)),
        _ => unreachable!("nibble is masked to four bits"),
    }
}

fn verification_diagnostics(errors: &[VerificationError]) -> Vec<String> {
    errors
        .iter()
        .map(|error| format!("{}: {}", error.diagnostic_id(), error.message))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use php_ir::{LoweringOptions, lower_frontend_result, verify_unit};
    use php_vm::api::Vm;

    use super::{
        CACHE_MAGIC, CURRENT_CACHE_FORMAT_VERSION, CacheArtifact, CacheDependency,
        CacheFingerprint, CacheFingerprintInput, CacheFormatVersion, CacheHeader, CacheLoadError,
        PHP_TARGET_VERSION,
    };

    const TARGET: &str = "test-target-triple";

    fn artifact() -> CacheArtifact {
        let fingerprint = CacheFingerprint::new("source-hash", "compiler-hash", "config-hash")
            .with_source_path("/fixture.php")
            .with_extra("opt_level", "0");
        let header =
            CacheHeader::new("engine-test", "abi-test", TARGET, fingerprint).with_dependency(
                CacheDependency::new("/dependency.php", "dep-hash", "include"),
            );
        CacheArtifact::new(header, b"metadata-only-payload".to_vec())
    }

    #[test]
    fn metadata_header_roundtrips() {
        let artifact = artifact();
        let bytes = artifact.to_bytes().expect("serialize cache artifact");
        let decoded = CacheArtifact::from_bytes(&bytes, TARGET).expect("decode cache artifact");

        assert_eq!(decoded, artifact);
        assert_eq!(&bytes[..8], &CACHE_MAGIC);
        assert_eq!(decoded.header.php_target_version, PHP_TARGET_VERSION);
    }

    #[test]
    fn rejects_bad_magic_bytes() {
        let mut bytes = artifact().to_bytes().expect("serialize cache artifact");
        bytes[0] = b'X';

        let error = CacheArtifact::from_bytes(&bytes, TARGET).expect_err("bad magic");
        assert!(matches!(error, CacheLoadError::BadMagic));
    }

    #[test]
    fn rejects_version_mismatch() {
        let mut bytes = artifact().to_bytes().expect("serialize cache artifact");
        bytes[8..10].copy_from_slice(&0_u16.to_le_bytes());

        let error = CacheArtifact::from_bytes(&bytes, TARGET).expect_err("version mismatch");
        assert!(matches!(
            error,
            CacheLoadError::UnsupportedFormatVersion {
                version: 0,
                current: CURRENT_CACHE_FORMAT_VERSION
            }
        ));
    }

    #[test]
    fn rejects_target_mismatch() {
        let bytes = artifact().to_bytes().expect("serialize cache artifact");

        let error = CacheArtifact::from_bytes(&bytes, "other-target").expect_err("target mismatch");
        assert!(matches!(
            error,
            CacheLoadError::TargetMismatch {
                expected,
                actual
            } if expected == "other-target" && actual == TARGET
        ));
    }

    #[test]
    fn rejects_corrupt_input() {
        let mut bytes = artifact().to_bytes().expect("serialize cache artifact");
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;

        let error = CacheArtifact::from_bytes(&bytes, TARGET).expect_err("corrupt input");
        assert!(matches!(
            error,
            CacheLoadError::ChecksumMismatch {
                section: "payload",
                ..
            }
        ));
    }

    #[test]
    fn rejects_corrupt_metadata_json() {
        let mut bytes = artifact().to_bytes().expect("serialize cache artifact");
        let metadata_start = 30;
        bytes[metadata_start] = b'!';
        let metadata_end = bytes.len() - b"metadata-only-payload".len();
        let checksum = crc32fast::hash(&bytes[metadata_start..metadata_end]);
        bytes[22..26].copy_from_slice(&checksum.to_le_bytes());

        let error = CacheArtifact::from_bytes(&bytes, TARGET).expect_err("corrupt metadata");
        assert!(matches!(error, CacheLoadError::MetadataDecode(_)));
    }

    #[test]
    fn rejects_unknown_future_format() {
        let mut bytes = artifact().to_bytes().expect("serialize cache artifact");
        let future = CURRENT_CACHE_FORMAT_VERSION + 1;
        bytes[8..10].copy_from_slice(&future.to_le_bytes());

        let error = CacheArtifact::from_bytes(&bytes, TARGET).expect_err("future format");
        assert!(matches!(
            error,
            CacheLoadError::UnknownFutureFormat {
                version,
                current: CURRENT_CACHE_FORMAT_VERSION
            } if version == future
        ));
    }

    #[test]
    fn rejects_php_target_mismatch() {
        let fingerprint = CacheFingerprint::new("source-hash", "compiler-hash", "config-hash");
        let mut header = CacheHeader::new("engine-test", "abi-test", TARGET, fingerprint);
        header.php_target_version = "8.6.0".to_owned();
        let bytes = CacheArtifact::new(header, Vec::new())
            .to_bytes()
            .expect("serialize cache artifact");

        let error = CacheArtifact::from_bytes(&bytes, TARGET).expect_err("php target mismatch");
        assert!(matches!(
            error,
            CacheLoadError::PhpTargetMismatch {
                expected,
                actual
            } if expected == PHP_TARGET_VERSION && actual == "8.6.0"
        ));
    }

    #[test]
    fn store_rejects_non_current_version() {
        let mut artifact = artifact();
        artifact.header.format_version = CacheFormatVersion::new(0);

        let error = artifact.to_bytes().expect_err("non-current writer version");
        assert!(
            error
                .to_string()
                .contains("cannot store cache format version")
        );
    }

    #[test]
    fn fingerprint_is_stable_for_same_source_and_config() {
        let input = CacheFingerprintInput::new(b"<?php echo 1;\n", "engine-test", TARGET)
            .with_engine_git_commit("abcdef0")
            .with_opt_level("0")
            .with_feature_flag("bytecode_cache", true)
            .with_ini_config("short_open_tag", "0")
            .with_runtime_config("include_path", ".");

        let first = CacheFingerprint::from_inputs(input.clone()).expect("fingerprint");
        let second = CacheFingerprint::from_inputs(input).expect("fingerprint");

        assert_eq!(first, second);
        assert_eq!(
            first.to_stable_json().expect("json"),
            second.to_stable_json().expect("json")
        );
        assert!(first.to_debug_text().contains("php_target_version=8.5.7"));
    }

    #[test]
    fn fingerprint_changes_when_source_bytes_change() {
        let first = CacheFingerprint::from_inputs(CacheFingerprintInput::new(
            b"<?php echo 1;\n",
            "e",
            TARGET,
        ))
        .expect("fingerprint");
        let second = CacheFingerprint::from_inputs(CacheFingerprintInput::new(
            b"<?php echo 2;\n",
            "e",
            TARGET,
        ))
        .expect("fingerprint");

        assert_ne!(first.source, second.source);
        assert_ne!(first.digest, second.digest);
    }

    #[test]
    fn fingerprint_changes_when_config_changes() {
        let first = CacheFingerprint::from_inputs(
            CacheFingerprintInput::new(b"<?php echo 1;\n", "e", TARGET)
                .with_ini_config("short_open_tag", "0"),
        )
        .expect("fingerprint");
        let second = CacheFingerprint::from_inputs(
            CacheFingerprintInput::new(b"<?php echo 1;\n", "e", TARGET)
                .with_ini_config("short_open_tag", "1"),
        )
        .expect("fingerprint");

        assert_ne!(first.config, second.config);
        assert_ne!(first.digest, second.digest);
    }

    #[test]
    fn fingerprint_changes_when_compiler_dimensions_change() {
        let base = CacheFingerprintInput::new(b"<?php echo 1;\n", "engine-test", TARGET)
            .with_engine_git_commit("abcdef0")
            .with_opt_level("0")
            .with_feature_flag("bytecode_cache", false);
        let opt = CacheFingerprint::from_inputs(base.clone().with_opt_level("1"))
            .expect("opt fingerprint");
        let feature =
            CacheFingerprint::from_inputs(base.clone().with_feature_flag("bytecode_cache", true))
                .expect("feature fingerprint");
        let commit = CacheFingerprint::from_inputs(base.clone().with_engine_git_commit("1234567"))
            .expect("commit fingerprint");
        let target = CacheFingerprint::from_inputs(CacheFingerprintInput::new(
            b"<?php echo 1;\n",
            "engine-test",
            "other-target",
        ))
        .expect("target fingerprint");
        let base = CacheFingerprint::from_inputs(base).expect("base fingerprint");

        assert_ne!(base.compiler, opt.compiler);
        assert_ne!(base.compiler, feature.compiler);
        assert_ne!(base.compiler, commit.compiler);
        assert_ne!(base.compiler, target.compiler);
    }

    #[test]
    fn fingerprint_includes_normalized_source_path_without_temp_dependency() {
        let fingerprint = CacheFingerprint::from_inputs(
            CacheFingerprintInput::new(b"<?php echo 1;\n", "engine-test", TARGET)
                .with_source_path("fixtures/performance/cache/fingerprint.php"),
        )
        .expect("fingerprint");

        let json = fingerprint.to_stable_json().expect("json");
        assert!(json.contains("fixtures/performance/cache/fingerprint.php"));
        assert!(!json.contains("/tmp/"));
        assert_eq!(
            fingerprint.source_path.as_deref(),
            Some("fixtures/performance/cache/fingerprint.php")
        );
    }

    #[test]
    fn bytecode_cache_roundtrip_executes_loaded_ir_with_identical_output() {
        for fixture in ["simple.php", "functions.php"] {
            let path = workspace_root()
                .join("tests/fixtures/performance/bytecode_cache")
                .join(fixture);
            let expected = fs::read_to_string(path.with_extension("php.out")).expect("expected");
            let source = fs::read_to_string(&path).expect("source");
            let source_path = normalized_fixture_path(&path);
            let unit = compile_fixture(&source, &source_path);
            let baseline = Vm::new().execute(unit.clone());
            assert!(
                baseline.status.is_success(),
                "baseline status: {:?}",
                baseline.status
            );
            assert_eq!(baseline.output.to_string_lossy(), expected);

            let fingerprint = CacheFingerprint::from_inputs(
                CacheFingerprintInput::new(source.as_bytes(), "engine-test", TARGET)
                    .with_source_path(&source_path)
                    .with_opt_level("0")
                    .with_feature_flag("bytecode_cache", true),
            )
            .expect("fingerprint");
            let header = CacheHeader::new("engine-test", "abi-test", TARGET, fingerprint.clone());
            let bytes = CacheArtifact::from_ir_unit(header, &unit)
                .expect("store verified IR")
                .to_bytes()
                .expect("serialize cache");

            let cached =
                CacheArtifact::load_ir_unit(&bytes, TARGET, &fingerprint).expect("load cached IR");
            let loaded = Vm::new().execute(cached.unit);
            assert!(
                loaded.status.is_success(),
                "loaded status: {:?}",
                loaded.status
            );
            assert_eq!(loaded.output, baseline.output);
            assert_eq!(loaded.output.to_string_lossy(), expected);
        }
    }

    #[test]
    fn bytecode_cache_corrupt_payload_is_typed_error_and_compile_path_still_runs() {
        let path = workspace_root().join("tests/fixtures/performance/bytecode_cache/simple.php");
        let source = fs::read_to_string(&path).expect("source");
        let source_path = normalized_fixture_path(&path);
        let unit = compile_fixture(&source, &source_path);
        let baseline = Vm::new().execute(unit.clone());
        assert!(baseline.status.is_success());

        let fingerprint = CacheFingerprint::from_inputs(CacheFingerprintInput::new(
            source.as_bytes(),
            "engine-test",
            TARGET,
        ))
        .expect("fingerprint");
        let header = CacheHeader::new("engine-test", "abi-test", TARGET, fingerprint.clone());
        let mut bytes = CacheArtifact::from_ir_unit(header, &unit)
            .expect("store verified IR")
            .to_bytes()
            .expect("serialize cache");
        let last = bytes.len() - 1;
        bytes[last] ^= 0x20;

        let error =
            CacheArtifact::load_ir_unit(&bytes, TARGET, &fingerprint).expect_err("corrupt payload");
        assert!(matches!(
            error,
            CacheLoadError::ChecksumMismatch {
                section: "payload",
                ..
            }
        ));

        let fallback = Vm::new().execute(compile_fixture(&source, &source_path));
        assert!(fallback.status.is_success());
        assert_eq!(fallback.output, baseline.output);
    }

    #[test]
    fn bytecode_cache_rejects_payload_that_fails_ir_verifier() {
        let path = workspace_root().join("tests/fixtures/performance/bytecode_cache/simple.php");
        let source = fs::read_to_string(&path).expect("source");
        let source_path = normalized_fixture_path(&path);
        let mut unit = compile_fixture(&source, &source_path);
        unit.version = u32::MAX;

        let fingerprint = CacheFingerprint::from_inputs(CacheFingerprintInput::new(
            source.as_bytes(),
            "engine-test",
            TARGET,
        ))
        .expect("fingerprint");
        let header = CacheHeader::new("engine-test", "abi-test", TARGET, fingerprint.clone());
        let payload = serde_json::to_vec(&unit).expect("invalid unit json");
        let bytes = CacheArtifact::new(header, payload)
            .to_bytes()
            .expect("serialize invalid payload");

        let error = CacheArtifact::load_ir_unit(&bytes, TARGET, &fingerprint)
            .expect_err("verifier rejects payload");
        assert!(matches!(
            error,
            CacheLoadError::PayloadVerification { diagnostics } if diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("E_PHP_IR_VERIFY_INVALID_VERSION"))
        ));
    }

    #[test]
    fn bytecode_cache_rejects_fingerprint_mismatch_before_payload_decode() {
        let path = workspace_root().join("tests/fixtures/performance/bytecode_cache/simple.php");
        let source = fs::read_to_string(&path).expect("source");
        let source_path = normalized_fixture_path(&path);
        let unit = compile_fixture(&source, &source_path);
        let fingerprint = CacheFingerprint::from_inputs(CacheFingerprintInput::new(
            source.as_bytes(),
            "engine-test",
            TARGET,
        ))
        .expect("fingerprint");
        let other_fingerprint = CacheFingerprint::from_inputs(CacheFingerprintInput::new(
            b"<?php echo 'changed';\n",
            "engine-test",
            TARGET,
        ))
        .expect("other fingerprint");
        let header = CacheHeader::new("engine-test", "abi-test", TARGET, fingerprint);
        let bytes = CacheArtifact::from_ir_unit(header, &unit)
            .expect("store verified IR")
            .to_bytes()
            .expect("serialize cache");

        let error = CacheArtifact::load_ir_unit(&bytes, TARGET, &other_fingerprint)
            .expect_err("fingerprint mismatch");
        assert!(matches!(error, CacheLoadError::FingerprintMismatch { .. }));
    }

    fn compile_fixture(source: &str, source_path: &str) -> php_ir::IrUnit {
        let frontend = php_semantics::analyze_source(source);
        assert!(
            !frontend.has_errors(),
            "frontend errors: {:?}",
            frontend.semantic_diagnostics()
        );
        let lowering = lower_frontend_result(
            &frontend,
            LoweringOptions {
                source_path: source_path.to_owned(),
                source_text: Some(source.to_owned()),
                ..LoweringOptions::default()
            },
        );
        assert!(
            lowering.diagnostics.is_empty(),
            "{:#?}",
            lowering.diagnostics
        );
        assert!(
            lowering.verification.is_ok(),
            "{:#?}",
            lowering.verification
        );
        verify_unit(&lowering.unit).expect("IR verifier");
        lowering.unit
    }

    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root")
            .to_path_buf()
    }

    fn normalized_fixture_path(path: &Path) -> String {
        let root = workspace_root();
        path.strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}
