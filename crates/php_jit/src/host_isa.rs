//! Host-native Cranelift ISA identity for cache and benchmark contracts.

use cranelift_codegen::settings::{self, Configurable};
use std::fmt;
use std::sync::OnceLock;

/// Stable identity of the host-native Cranelift ISA used for JIT compilation.
///
/// The display and fingerprint include Cranelift's ISA-specific feature flags,
/// so cache keys cannot alias code compiled for different host CPU features.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CraneliftHostIsaIdentity {
    /// Cranelift backend name (for example `x64`).
    pub isa_name: String,
    /// Target triple selected by `cranelift_native`.
    pub target_triple: String,
    /// Human-readable shared and ISA-specific settings.
    pub display: String,
    /// Stable fingerprint of `display` for compact cache/report identities.
    pub feature_fingerprint: u64,
}

/// Typed failure while constructing the host-native ISA identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CraneliftHostIsaError {
    /// Stable machine-readable failure code.
    pub code: &'static str,
    /// Human-readable detail.
    pub detail: String,
}

impl CraneliftHostIsaError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for CraneliftHostIsaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for CraneliftHostIsaError {}

/// Returns the exact host-native Cranelift ISA identity used by this process.
///
/// The result is cached because probing host features and constructing the ISA
/// is process-invariant and this function participates in compile-cache keys.
pub fn cranelift_host_isa_identity() -> Result<CraneliftHostIsaIdentity, CraneliftHostIsaError> {
    static IDENTITY: OnceLock<Result<CraneliftHostIsaIdentity, CraneliftHostIsaError>> =
        OnceLock::new();
    IDENTITY
        .get_or_init(build_cranelift_host_isa_identity)
        .clone()
}

fn build_cranelift_host_isa_identity() -> Result<CraneliftHostIsaIdentity, CraneliftHostIsaError> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|error| {
            CraneliftHostIsaError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
        })?;
    flag_builder.set("is_pic", "false").map_err(|error| {
        CraneliftHostIsaError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
    })?;
    flag_builder
        .set("preserve_frame_pointers", "true")
        .map_err(|error| {
            CraneliftHostIsaError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
        })?;
    let isa_builder = cranelift_native::builder().map_err(|error| {
        CraneliftHostIsaError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
            format!("host target is unsupported: {error}"),
        )
    })?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|error| {
            CraneliftHostIsaError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
                format!("host ISA setup failed: {error}"),
            )
        })?;
    let mut isa_flags = isa
        .isa_flags()
        .into_iter()
        .map(|flag| flag.to_string())
        .collect::<Vec<_>>();
    isa_flags.sort();
    let display = format!("{isa}; isa_flags=[{}]", isa_flags.join(","));
    Ok(CraneliftHostIsaIdentity {
        isa_name: isa.name().to_owned(),
        target_triple: isa.triple().to_string(),
        feature_fingerprint: stable_identity_hash(display.as_bytes()),
        display,
    })
}

fn stable_identity_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::cranelift_host_isa_identity;

    #[test]
    fn host_isa_identity_is_stable_and_feature_complete() {
        let first = cranelift_host_isa_identity().expect("host ISA identity");
        let second = cranelift_host_isa_identity().expect("cached host ISA identity");

        assert_eq!(first, second);
        assert!(!first.isa_name.is_empty());
        assert!(!first.target_triple.is_empty());
        assert!(first.display.contains("isa_flags=["), "{}", first.display);
        assert_ne!(first.feature_fingerprint, 0);
    }
}
