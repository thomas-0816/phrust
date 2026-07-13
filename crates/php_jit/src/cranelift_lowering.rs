//! Optional Cranelift IR lowering and native-entry prototype for performance.
//!
//! This module produces and verifies
//! Cranelift IR text for constrained integer, array, string, property, and
//! dispatch-helper subsets. Native execution is still default-off and requires
//! the caller to opt in explicitly.

use crate::code_manager::ManagedCompileError;
#[cfg(test)]
use crate::region_ir::build_baseline_region;
use crate::region_ir::{
    BaselineRegionBuilder, CompileMetadata, NativeCompilerTier, RegionBinaryOp, RegionCallResult,
    RegionCallTarget, RegionCompareOpCode, RegionGraph, RegionInstruction, RegionInstructionKind,
    RegionNativeCall, RegionNativeControl, RegionOperand, RegionTerminator,
};
use crate::{
    CraneliftCodeKey, CraneliftCompilerIdentity, JIT_RUNTIME_ABI_HASH, JitCompileRequest,
    JitCompileStatus, JitEligibility, JitFunctionHandle, ManagedJitFunction, NativeCompileOutcome,
    NativeCompileRequest, NativeCompilerApi, analyze_jit_eligibility, global_code_manager,
};
#[cfg(test)]
use crate::{JitNativeSpecialization, JitPropertyLoadMetadata};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{
    self, AbiParam, Function, InstBuilder, MemFlagsData, Signature, StackSlotData, StackSlotKind,
    UserFuncName, types,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verifier::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};
#[cfg(test)]
use php_ir::IrParam;
#[cfg(test)]
use php_ir::instruction::Terminator;
use php_ir::instruction::TerminatorKind;
use php_ir::{
    BinaryOp, BlockId, FunctionId, InstructionKind, IrConstant, IrFunction, IrReturnType, IrUnit,
    LocalId, Operand, RegId,
};
use std::collections::BTreeMap;
use std::fmt;
use std::time::Instant;

mod executable_region;

/// Stable Cranelift lowering result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CraneliftLoweringResult {
    /// Function ID lowered from the IR unit.
    pub function: FunctionId,
    /// IR function name used for diagnostics.
    pub function_name: String,
    /// Generated Cranelift IR text.
    pub clif: String,
    /// Prototype counters.
    pub stats: CraneliftLoweringStats,
    /// Native execution handle. This remains `None` for CLIF-only lowering.
    pub machine_code_handle: Option<CraneliftMachineCodeHandle>,
}

/// Opaque future machine-code handle for CLIF-only reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CraneliftMachineCodeHandle {
    /// Stable opaque handle ID.
    pub id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct ConstantReturnCandidate {
    value: i64,
    arity: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct NativeConstantCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeScalarRegionCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
    fast_path_hits: u64,
    has_control_flow: bool,
}

fn compile_managed_native(
    request: &JitCompileRequest,
    function: FunctionId,
    specialization: &str,
    helpers: &[(&str, usize)],
    compile: impl FnOnce(
        &mut JITModule,
        &str,
    ) -> Result<(JitFunctionHandle, u64), CraneliftLoweringError>,
) -> Result<ManagedJitFunction, CraneliftLoweringError> {
    let compiled_unit = request
        .ir_fingerprint
        .clone()
        .unwrap_or_else(|| format!("unfingerprinted-function-{}", function.raw()));
    let identity = crate::cranelift_host_isa_identity().map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_NATIVE_TARGET", error.to_string())
    })?;
    let mut config_hash = if request.config_hash == 0 {
        identity.feature_fingerprint ^ u64::from(request.opt_level)
    } else {
        request.config_hash
    };
    for (symbol, address) in helpers {
        for byte in symbol.as_bytes().iter().chain(address.to_le_bytes().iter()) {
            config_hash ^= u64::from(*byte);
            config_hash = config_hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    let dependency_identity = request
        .dependency_identity
        .clone()
        .unwrap_or_else(|| format!("ir:{compiled_unit}"));
    let key = CraneliftCodeKey {
        compiled_unit,
        region: format!("{}:function:{}", request.region_id, function.raw()),
        abi_hash: JIT_RUNTIME_ABI_HASH,
        compiler_tier: if request.opt_level == 0 {
            "baseline".to_owned()
        } else {
            "optimizing".to_owned()
        },
        helper_abi_hash: config_hash
            ^ JIT_RUNTIME_ABI_HASH
            ^ crate::JIT_HELPER_REGISTRY_ABI_HASH
            ^ php_runtime::api::NATIVE_OPERATION_ABI_HASH,
        target_cpu: format!(
            "{}:{}:{}",
            identity.target_triple, identity.isa_name, identity.feature_fingerprint
        ),
        semantic_config_hash: request.config_hash,
        dependency_identity,
        config_hash,
        invalidation_generation: request.invalidation_generation,
        specialization: specialization.to_owned(),
    };
    let manager = global_code_manager().map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_CODE_MANAGER", error.to_string())
    })?;
    manager
        .compile_once(key, helpers, compile)
        .map_err(|error| match error {
            ManagedCompileError::Manager(error) => {
                CraneliftLoweringError::new("JIT_CRANELIFT_CODE_MANAGER", error.to_string())
            }
            ManagedCompileError::Compile(error) => error,
        })
}

#[cfg(test)]
fn runtime_helper_symbol(base: &str, address: usize) -> String {
    format!("{base}_{address:016x}")
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct PackedArrayFetchCandidate {
    array_param: LocalId,
    index_param: LocalId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct NativePackedArrayFetchCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct PackedForeachIntSumCandidate {
    array_param: LocalId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct NativePackedForeachIntSumCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
enum KnownCallKind {
    Strlen,
    Count,
}

#[cfg(test)]
impl KnownCallKind {
    const fn function_name(self) -> &'static str {
        match self {
            Self::Strlen => "strlen",
            Self::Count => "count",
        }
    }

    const fn helper_symbol(self) -> &'static str {
        match self {
            Self::Strlen => KNOWN_STRLEN_HELPER_SYMBOL,
            Self::Count => KNOWN_COUNT_HELPER_SYMBOL,
        }
    }

    const fn specialization(self) -> JitNativeSpecialization {
        match self {
            Self::Strlen => JitNativeSpecialization::KnownCallStrlen,
            Self::Count => JitNativeSpecialization::KnownCallCount,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct KnownCallCandidate {
    kind: KnownCallKind,
    value_param: LocalId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct NativeKnownCallCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct StringConcatCandidate {
    lhs_param: LocalId,
    rhs_param: LocalId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct NativeStringConcatCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct PropertyLoadCandidate {
    object_param: LocalId,
    metadata: JitPropertyLoadMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct NativePropertyLoadCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[cfg(test)]
const JIT_PACKED_ARRAY_STATUS_BOUNDS_EXIT: i32 = 2;
#[cfg(test)]
const JIT_PACKED_ARRAY_STATUS_LAYOUT_EXIT: i32 = 3;
#[cfg(test)]
const PACKED_ARRAY_LEN_HELPER_SYMBOL: &str = "phrust_jit_array_len_abi";
#[cfg(test)]
const PACKED_ARRAY_FETCH_HELPER_SYMBOL: &str = "phrust_jit_array_fetch_int_slow_abi";
#[cfg(test)]
const KNOWN_STRLEN_HELPER_SYMBOL: &str = "phrust_jit_strlen_known_abi";
#[cfg(test)]
const KNOWN_COUNT_HELPER_SYMBOL: &str = "phrust_jit_count_known_abi";
#[cfg(test)]
const STRING_CONCAT_HELPER_SYMBOL: &str = "php_jit_concat_string_string_fast";
#[cfg(test)]
const RECORD_ARRAY_LOOKUP_HELPER_SYMBOL: &str = "phrust_jit_record_array_lookup";
#[cfg(test)]
const PROPERTY_LOAD_HELPER_SYMBOL: &str = "php_jit_property_load_monomorphic_fast";
const NATIVE_CALL_DISPATCH_SYMBOL: &str = "phrust_jit_native_call_dispatch";

/// Mandatory Cranelift native compiler.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CraneliftNativeCompiler;

impl NativeCompilerApi for CraneliftNativeCompiler {
    fn compile_region(&mut self, request: &NativeCompileRequest<'_>) -> NativeCompileOutcome {
        compile_authoritative_region(request)
    }
}

fn compile_authoritative_region(request: &NativeCompileRequest<'_>) -> NativeCompileOutcome {
    let (Some(unit), Some(function)) = (request.unit, request.function) else {
        return NativeCompileOutcome::skipped(
            JitCompileStatus::Rejected {
                reason: "JIT_CRANELIFT_REJECT_MISSING_IR".to_owned(),
            },
            format!(
                "Cranelift native compiler missing IR context for region `{}`",
                request.compile.region_id
            ),
        );
    };
    let isa = match crate::cranelift_host_isa_identity() {
        Ok(isa) => isa,
        Err(error) => {
            return NativeCompileOutcome::skipped(
                JitCompileStatus::Rejected {
                    reason: error.code.to_owned(),
                },
                error.to_string(),
            );
        }
    };
    let metadata = CompileMetadata {
        ir_fingerprint: request
            .compile
            .ir_fingerprint
            .clone()
            .unwrap_or_else(|| format!("unit-{}-function-{}", unit.id.raw(), function.raw())),
        tier: if request.compile.opt_level == 0 {
            NativeCompilerTier::Baseline
        } else {
            NativeCompilerTier::Optimizing
        },
        helper_abi_hash: runtime_helper_abi_hash(request.runtime_helpers),
        target_cpu: format!(
            "{}:{}:{}",
            isa.target_triple, isa.isa_name, isa.feature_fingerprint
        ),
        semantic_config_hash: request.compile.config_hash,
        dependency_identity: request
            .compile
            .dependency_identity
            .clone()
            .unwrap_or_else(|| format!("unit-{}-version-{}", unit.id.raw(), unit.version)),
    };
    let region = match BaselineRegionBuilder::build(unit, function, &metadata) {
        Ok(region) => region,
        Err(error) => {
            return NativeCompileOutcome::skipped(
                JitCompileStatus::Rejected {
                    reason: error.code.to_owned(),
                },
                error.to_string(),
            );
        }
    };
    let start = Instant::now();
    match executable_region::compile_region_graph_native(
        unit,
        &region,
        request.runtime_helpers.native_call_dispatch,
        request.compile,
    ) {
        Ok(compiled) => {
            let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
            NativeCompileOutcome::compiled(
                compiled.handle,
                format!(
                    "Cranelift baseline Region IR `{}` function={} abi_hash={} code_bytes={} fast_path_hits={} control_flow={}",
                    request.compile.region_id,
                    function.raw(),
                    JIT_RUNTIME_ABI_HASH,
                    compiled.code_bytes,
                    compiled.fast_path_hits,
                    compiled.has_control_flow
                ),
                compiled.code_bytes,
                elapsed.max(1),
            )
        }
        Err(error) => NativeCompileOutcome::skipped(
            JitCompileStatus::Rejected {
                reason: error.code.to_owned(),
            },
            format!(
                "Cranelift baseline Region IR compile rejected region `{}`: {error}",
                request.compile.region_id
            ),
        ),
    }
}

fn runtime_helper_abi_hash(helpers: crate::JitRuntimeHelperAddresses) -> u64 {
    let mut hash = JIT_RUNTIME_ABI_HASH
        ^ crate::JIT_HELPER_REGISTRY_ABI_HASH
        ^ php_runtime::api::NATIVE_OPERATION_ABI_HASH;
    for address in [
        helpers.helper_table,
        helpers.packed_array_len,
        helpers.packed_array_fetch_int_slow,
        helpers.known_strlen,
        helpers.known_count,
        helpers.string_concat,
        helpers.property_load,
        helpers.record_array_lookup,
        helpers.native_call_dispatch,
    ] {
        for byte in address.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

#[cfg(test)]
impl CraneliftNativeCompiler {
    #[allow(dead_code)]
    fn compile_candidate_differential(
        &mut self,
        request: &NativeCompileRequest<'_>,
    ) -> NativeCompileOutcome {
        let (Some(unit), Some(function)) = (request.unit, request.function) else {
            return NativeCompileOutcome::skipped(
                JitCompileStatus::Rejected {
                    reason: "cranelift native compiler requires IR unit and function".to_owned(),
                },
                format!(
                    "Cranelift native compiler missing IR context for region `{}`",
                    request.compile.region_id
                ),
            );
        };

        if let Ok(candidate) = constant_return_candidate(unit, function) {
            let start = Instant::now();
            match compile_constant_return_native(
                function,
                candidate.value,
                candidate.arity,
                request.compile,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return NativeCompileOutcome::compiled(
                        compiled.handle,
                        format!(
                            "Cranelift native constant-return region `{}` function={} abi_hash={} code_bytes={}",
                            request.compile.region_id,
                            function.raw(),
                            JIT_RUNTIME_ABI_HASH,
                            compiled.code_bytes
                        ),
                        compiled.code_bytes,
                        elapsed.max(1),
                    );
                }
                Err(error) => {
                    return NativeCompileOutcome::skipped(
                        JitCompileStatus::Rejected {
                            reason: error.code.to_owned(),
                        },
                        format!(
                            "Cranelift native constant-return compile rejected region `{}`: {}",
                            request.compile.region_id, error
                        ),
                    );
                }
            }
        }

        if let Ok(candidate) = packed_array_fetch_candidate(unit, function) {
            let start = Instant::now();
            match compile_packed_array_fetch_native(
                function,
                &candidate,
                request.runtime_helpers.packed_array_fetch_int_slow,
                request.compile,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return NativeCompileOutcome::compiled(
                        compiled.handle,
                        format!(
                            "Cranelift native packed-array fetch region `{}` function={} abi_hash={} code_bytes={} helper=php_jit_array_fetch_int_slow",
                            request.compile.region_id,
                            function.raw(),
                            JIT_RUNTIME_ABI_HASH,
                            compiled.code_bytes
                        ),
                        compiled.code_bytes,
                        elapsed.max(1),
                    );
                }
                Err(error) => {
                    return NativeCompileOutcome::skipped(
                        JitCompileStatus::Rejected {
                            reason: error.code.to_owned(),
                        },
                        format!(
                            "Cranelift native packed-array fetch compile rejected region `{}`: {}",
                            request.compile.region_id, error
                        ),
                    );
                }
            }
        }

        if let Ok(candidate) = packed_foreach_int_sum_candidate(unit, function) {
            let start = Instant::now();
            match compile_packed_foreach_int_sum_native(
                function,
                &candidate,
                request.runtime_helpers.packed_array_len,
                request.runtime_helpers.packed_array_fetch_int_slow,
                request.compile,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return NativeCompileOutcome::compiled(
                        compiled.handle,
                        format!(
                            "Cranelift native packed-foreach int-sum region `{}` function={} abi_hash={} code_bytes={} helpers=php_jit_array_len,php_jit_array_fetch_int_slow",
                            request.compile.region_id,
                            function.raw(),
                            JIT_RUNTIME_ABI_HASH,
                            compiled.code_bytes
                        ),
                        compiled.code_bytes,
                        elapsed.max(1),
                    );
                }
                Err(error) => {
                    return NativeCompileOutcome::skipped(
                        JitCompileStatus::Rejected {
                            reason: error.code.to_owned(),
                        },
                        format!(
                            "Cranelift native packed-foreach int-sum compile rejected region `{}`: {}",
                            request.compile.region_id, error
                        ),
                    );
                }
            }
        }

        if let Ok(candidate) = known_call_candidate(unit, function) {
            let helper_address = match candidate.kind {
                KnownCallKind::Strlen => request.runtime_helpers.known_strlen,
                KnownCallKind::Count => request.runtime_helpers.known_count,
            };
            let start = Instant::now();
            match compile_known_call_native(function, &candidate, helper_address, request.compile) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return NativeCompileOutcome::compiled(
                        compiled.handle,
                        format!(
                            "Cranelift native known-call {} region `{}` function={} abi_hash={} code_bytes={} helper={}",
                            candidate.kind.function_name(),
                            request.compile.region_id,
                            function.raw(),
                            JIT_RUNTIME_ABI_HASH,
                            compiled.code_bytes,
                            candidate.kind.helper_symbol()
                        ),
                        compiled.code_bytes,
                        elapsed.max(1),
                    );
                }
                Err(error) => {
                    return NativeCompileOutcome::skipped(
                        JitCompileStatus::Rejected {
                            reason: error.code.to_owned(),
                        },
                        format!(
                            "Cranelift native known-call compile rejected region `{}`: {}",
                            request.compile.region_id, error
                        ),
                    );
                }
            }
        }

        if let Ok(candidate) = string_concat_candidate(unit, function) {
            let start = Instant::now();
            match compile_string_concat_native(
                function,
                &candidate,
                request.runtime_helpers.string_concat,
                request.compile,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return NativeCompileOutcome::compiled(
                        compiled.handle,
                        format!(
                            "Cranelift native string-concat region `{}` function={} abi_hash={} code_bytes={} helper={}",
                            request.compile.region_id,
                            function.raw(),
                            JIT_RUNTIME_ABI_HASH,
                            compiled.code_bytes,
                            STRING_CONCAT_HELPER_SYMBOL
                        ),
                        compiled.code_bytes,
                        elapsed.max(1),
                    );
                }
                Err(error) => {
                    return NativeCompileOutcome::skipped(
                        JitCompileStatus::Rejected {
                            reason: error.code.to_owned(),
                        },
                        format!(
                            "Cranelift native string-concat compile rejected region `{}`: {}",
                            request.compile.region_id, error
                        ),
                    );
                }
            }
        }

        if let Ok(candidate) = record_array_lookup_candidate(unit, function) {
            let start = Instant::now();
            match compile_record_array_lookup_native(
                function,
                &candidate,
                request.runtime_helpers.record_array_lookup,
                request.compile,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return NativeCompileOutcome::compiled(
                        compiled.handle,
                        format!(
                            "Cranelift native record-lookup region `{}` function={} abi_hash={} code_bytes={} helper={}",
                            request.compile.region_id,
                            function.raw(),
                            JIT_RUNTIME_ABI_HASH,
                            compiled.code_bytes,
                            RECORD_ARRAY_LOOKUP_HELPER_SYMBOL
                        ),
                        compiled.code_bytes,
                        elapsed.max(1),
                    );
                }
                Err(error) => {
                    return NativeCompileOutcome::skipped(
                        JitCompileStatus::Rejected {
                            reason: error.code.to_owned(),
                        },
                        format!(
                            "Cranelift native record-lookup compile rejected region `{}`: {}",
                            request.compile.region_id, error
                        ),
                    );
                }
            }
        }

        if let Ok(candidate) = property_load_candidate(unit, function) {
            let start = Instant::now();
            match compile_property_load_native(
                function,
                &candidate,
                request.runtime_helpers.property_load,
                request.compile,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return NativeCompileOutcome::compiled(
                        compiled.handle,
                        format!(
                            "Cranelift native property-load region `{}` function={} abi_hash={} code_bytes={} helper={} class={} property=${}",
                            request.compile.region_id,
                            function.raw(),
                            JIT_RUNTIME_ABI_HASH,
                            compiled.code_bytes,
                            PROPERTY_LOAD_HELPER_SYMBOL,
                            candidate.metadata.receiver_class,
                            candidate.metadata.property
                        ),
                        compiled.code_bytes,
                        elapsed.max(1),
                    );
                }
                Err(error) => {
                    return NativeCompileOutcome::skipped(
                        JitCompileStatus::Rejected {
                            reason: error.code.to_owned(),
                        },
                        format!(
                            "Cranelift native property-load compile rejected region `{}`: {}",
                            request.compile.region_id, error
                        ),
                    );
                }
            }
        }

        if let Ok(region) = build_baseline_region(unit, function) {
            let start = Instant::now();
            match executable_region::compile_region_graph_native(
                unit,
                &region,
                request.runtime_helpers.native_call_dispatch,
                request.compile,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return NativeCompileOutcome::compiled(
                        compiled.handle,
                        format!(
                            "Cranelift executable Region IR `{}` function={} abi_hash={} code_bytes={} fast_path_hits={} control_flow={}",
                            request.compile.region_id,
                            function.raw(),
                            JIT_RUNTIME_ABI_HASH,
                            compiled.code_bytes,
                            compiled.fast_path_hits,
                            compiled.has_control_flow
                        ),
                        compiled.code_bytes,
                        elapsed.max(1),
                    );
                }
                Err(error) => {
                    return NativeCompileOutcome::skipped(
                        JitCompileStatus::Rejected {
                            reason: error.code.to_owned(),
                        },
                        format!(
                            "Cranelift executable Region IR compile rejected region `{}`: {}",
                            request.compile.region_id, error
                        ),
                    );
                }
            }
        }

        match lower_function_to_cranelift(unit, function) {
            Ok(result) => NativeCompileOutcome::skipped(
                JitCompileStatus::Rejected {
                    reason: "cranelift backend verified CLIF but region is not in native executable subset"
                        .to_owned(),
                },
                format!(
                    "Cranelift backend verified region `{}` function={} clif_bytes={} blocks={} instructions={} native_subset=unsupported-php-shape",
                    request.compile.region_id,
                    function.raw(),
                    result.clif.len(),
                    result.stats.blocks_lowered,
                    result.stats.instructions_lowered
                ),
            ),
            Err(error) => NativeCompileOutcome::skipped(
                JitCompileStatus::Rejected {
                    reason: error.code.to_owned(),
                },
                format!(
                    "Cranelift backend rejected region `{}`: {}",
                    request.compile.region_id, error
                ),
            ),
        }
    }
}

/// Per-lowering counters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CraneliftLoweringStats {
    /// Basic blocks lowered.
    pub blocks_lowered: u64,
    /// Instructions lowered.
    pub instructions_lowered: u64,
    /// Cranelift verifier ran successfully.
    pub verified: bool,
}

/// Standalone CLIF smoke result for Cranelift CLIF.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CraneliftClifSmokeResult {
    /// Stable smoke function name.
    pub function_name: &'static str,
    /// Generated Cranelift IR text.
    pub clif: String,
    /// Smoke lowering counters.
    pub stats: CraneliftLoweringStats,
}

/// Typed Cranelift lowering failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CraneliftLoweringError {
    /// Stable machine-readable rejection code.
    pub code: &'static str,
    /// Human-readable detail.
    pub detail: String,
}

impl CraneliftLoweringError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for CraneliftLoweringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for CraneliftLoweringError {}

/// Builds and verifies a trivial `fn(i64, i64) -> i64` add function.
///
/// This smoke intentionally does not consume PHP IR and never allocates or
/// executes native code. It exists to prove that the Cranelift frontend and
/// verifier are wired into the build in a deterministic, default-off path.
pub fn build_trivial_add_clif_smoke() -> Result<CraneliftClifSmokeResult, CraneliftLoweringError> {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(types::I64));

    let mut function = Function::with_name_signature(UserFuncName::user(0, 0), signature);
    let mut builder_context = FunctionBuilderContext::new();
    let mut stats = CraneliftLoweringStats::default();

    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        builder.seal_block(block);
        let params = builder.block_params(block).to_vec();
        let sum = builder.ins().iadd(params[0], params[1]);
        builder.ins().return_(&[sum]);
        builder.finalize();
    }

    let flags = settings::Flags::new(settings::builder());
    verify_function(&function, &flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("Cranelift verifier rejected standalone smoke IR: {error}"),
        )
    })?;
    stats.blocks_lowered = 1;
    stats.instructions_lowered = 2;
    stats.verified = true;

    Ok(CraneliftClifSmokeResult {
        function_name: "trivial_add_i64",
        clif: function.display().to_string(),
        stats,
    })
}

/// Lowers one eligible integer leaf function into Cranelift IR text.
///
/// The supported subset is intentionally minimal:
/// - integer constants,
/// - integer add/sub/mul,
/// - register moves of lowered integer values,
/// - a single integer return.
pub fn lower_function_to_cranelift(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<CraneliftLoweringResult, CraneliftLoweringError> {
    let eligibility = analyze_jit_eligibility(unit, function);
    match &eligibility.eligibility {
        JitEligibility::Eligible => {}
        JitEligibility::Rejected { reason } => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ELIGIBILITY",
                format!(
                    "eligibility rejected function before lowering: {} ({})",
                    reason.code, reason.detail
                ),
            ));
        }
        JitEligibility::Unknown { reason } => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ELIGIBILITY_UNKNOWN",
                format!(
                    "eligibility could not classify function before lowering: {} ({})",
                    reason.code, reason.detail
                ),
            ));
        }
    }

    let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_FUNCTION",
            format!("function id {} is not present", function.raw()),
        )
    })?;
    lower_checked_function(unit, function, ir_function)
}

fn lower_checked_function(
    unit: &IrUnit,
    function_id: FunctionId,
    ir_function: &IrFunction,
) -> Result<CraneliftLoweringResult, CraneliftLoweringError> {
    if ir_function.blocks.len() != 1 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CONTROL_FLOW",
            format!(
                "expected exactly one basic block, found {}",
                ir_function.blocks.len()
            ),
        ));
    }

    let mut signature = Signature::new(CallConv::SystemV);
    for _ in &ir_function.params {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.returns.push(AbiParam::new(types::I64));
    let mut function =
        Function::with_name_signature(UserFuncName::user(0, function_id.raw()), signature);
    let mut builder_context = FunctionBuilderContext::new();
    let mut registers = BTreeMap::new();
    let mut locals = BTreeMap::new();
    let mut stats = CraneliftLoweringStats::default();

    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let block = builder.create_block();
        builder.append_block_params_for_function_params(block);
        builder.switch_to_block(block);
        builder.seal_block(block);
        for (param, cl_value) in ir_function
            .params
            .iter()
            .zip(builder.block_params(block).iter().copied())
        {
            locals.insert(param.local, cl_value);
        }

        let ir_block = &ir_function.blocks[0];
        stats.blocks_lowered = 1;
        for instruction in &ir_block.instructions {
            match &instruction.kind {
                InstructionKind::Nop => {}
                InstructionKind::LoadConst { dst, constant } => {
                    let value = constant_value(unit, *constant)?;
                    let cl_value = builder.ins().iconst(types::I64, value);
                    registers.insert(*dst, cl_value);
                    stats.instructions_lowered += 1;
                }
                InstructionKind::Move { dst, src } => {
                    let cl_value = lower_operand(&mut builder, &registers, &locals, unit, src)?;
                    registers.insert(*dst, cl_value);
                    stats.instructions_lowered += 1;
                }
                InstructionKind::LoadLocal { dst, local } => {
                    let cl_value = locals.get(local).copied().ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_MISSING_LOCAL",
                            format!("local {} has not been lowered", local.raw()),
                        )
                    })?;
                    registers.insert(*dst, cl_value);
                    stats.instructions_lowered += 1;
                }
                InstructionKind::StoreLocal { local, src } => {
                    let cl_value = lower_operand(&mut builder, &registers, &locals, unit, src)?;
                    locals.insert(*local, cl_value);
                    stats.instructions_lowered += 1;
                }
                InstructionKind::Binary { dst, op, lhs, rhs } => {
                    let lhs = lower_operand(&mut builder, &registers, &locals, unit, lhs)?;
                    let rhs = lower_operand(&mut builder, &registers, &locals, unit, rhs)?;
                    let cl_value = match op {
                        BinaryOp::Add => builder.ins().iadd(lhs, rhs),
                        BinaryOp::Sub => builder.ins().isub(lhs, rhs),
                        BinaryOp::Mul => builder.ins().imul(lhs, rhs),
                        other => {
                            return Err(CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_UNSUPPORTED_BINARY",
                                format!("binary op {other:?} is outside the prototype subset"),
                            ));
                        }
                    };
                    registers.insert(*dst, cl_value);
                    stats.instructions_lowered += 1;
                }
                other => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_UNSUPPORTED_OPCODE",
                        format!("instruction {other:?} is outside the prototype subset"),
                    ));
                }
            }
        }

        match &ir_block.terminator {
            Some(terminator) => match &terminator.kind {
                TerminatorKind::Return {
                    value: Some(value),
                    by_ref_local: None,
                } => {
                    let value = lower_operand(&mut builder, &registers, &locals, unit, value)?;
                    builder.ins().return_(&[value]);
                }
                TerminatorKind::Return {
                    value: None,
                    by_ref_local: _,
                } => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_RETURN",
                        "prototype requires an integer return value",
                    ));
                }
                TerminatorKind::Return {
                    value: Some(_),
                    by_ref_local: Some(_),
                } => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_BY_REF_RETURN",
                        "by-reference returns are outside the prototype subset",
                    ));
                }
                other => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CONTROL_FLOW",
                        format!("terminator {other:?} is outside the prototype subset"),
                    ));
                }
            },
            None => {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_MISSING_TERMINATOR",
                    "basic block has no return terminator",
                ));
            }
        }

        builder.finalize();
    }

    let flags = settings::Flags::new(settings::builder());
    verify_function(&function, &flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("Cranelift verifier rejected generated IR: {error}"),
        )
    })?;
    stats.verified = true;

    Ok(CraneliftLoweringResult {
        function: function_id,
        function_name: ir_function.name.clone(),
        clif: function.display().to_string(),
        stats,
        machine_code_handle: None,
    })
}

fn lower_operand(
    builder: &mut FunctionBuilder<'_>,
    registers: &BTreeMap<RegId, ir::Value>,
    locals: &BTreeMap<LocalId, ir::Value>,
    unit: &IrUnit,
    operand: &Operand,
) -> Result<ir::Value, CraneliftLoweringError> {
    match operand {
        Operand::Register(reg) => registers.get(reg).copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
                format!("register {} has not been lowered", reg.raw()),
            )
        }),
        Operand::Constant(constant) => {
            let value = constant_value(unit, *constant)?;
            Ok(builder.ins().iconst(types::I64, value))
        }
        Operand::Local(local) => locals.get(local).copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_MISSING_LOCAL",
                format!("local {} has not been lowered", local.raw()),
            )
        }),
    }
}

fn constant_value(unit: &IrUnit, constant: php_ir::ConstId) -> Result<i64, CraneliftLoweringError> {
    match unit.constants.get(constant.index()) {
        Some(IrConstant::Int(value)) => Ok(*value),
        Some(other) => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NON_INT_CONSTANT",
            format!("constant {other:?} is not an integer"),
        )),
        None => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_CONSTANT",
            format!("constant id {} is not present", constant.raw()),
        )),
    }
}

#[cfg(test)]
fn constant_return_candidate(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<ConstantReturnCandidate, CraneliftLoweringError> {
    let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_FUNCTION",
            format!("function id {} is not present", function.raw()),
        )
    })?;
    if ir_function.flags.is_top_level
        || ir_function.flags.is_closure
        || ir_function.flags.is_method
        || ir_function.flags.is_generator
        || ir_function.returns_by_ref
        || !ir_function.captures.is_empty()
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_SHAPE",
            "constant-return native subset only accepts ordinary leaf functions",
        ));
    }
    if !matches!(
        ir_function.return_type.as_ref(),
        Some(php_ir::IrReturnType::Int)
    ) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_TYPE",
            "constant-return native subset requires explicit int return type",
        ));
    }
    let arity: u8 = ir_function.params.len().try_into().map_err(|_| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_ARITY",
            "constant-return native subset supports at most 255 params",
        )
    })?;
    if arity > 4 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_ARITY",
            "constant-return native subset supports at most four params",
        ));
    }
    for param in &ir_function.params {
        if param.by_ref
            || param.variadic
            || param.default.is_some()
            || !matches!(param.type_.as_ref(), Some(php_ir::IrReturnType::Int))
        {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_PARAM",
                "constant-return native subset requires plain int params",
            ));
        }
    }
    if ir_function.blocks.len() != 1 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_CONTROL_FLOW",
            format!(
                "constant-return native subset expected one block, found {}",
                ir_function.blocks.len()
            ),
        ));
    }

    let mut registers = BTreeMap::new();
    let block = &ir_function.blocks[0];
    for instruction in &block.instructions {
        match &instruction.kind {
            InstructionKind::Nop => {}
            InstructionKind::LoadConst { dst, constant } => {
                registers.insert(*dst, constant_value(unit, *constant)?);
            }
            InstructionKind::Move { dst, src } => {
                let value = constant_operand_value(unit, &registers, src)?;
                registers.insert(*dst, value);
            }
            other => {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_OPCODE",
                    format!("instruction {other:?} is outside constant-return native subset"),
                ));
            }
        }
    }
    let value = match &block.terminator {
        Some(terminator) => match &terminator.kind {
            TerminatorKind::Return {
                value: Some(value),
                by_ref_local: None,
            } => constant_operand_value(unit, &registers, value)?,
            other => {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_TERMINATOR",
                    format!("terminator {other:?} is outside constant-return native subset"),
                ));
            }
        },
        None => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_TERMINATOR",
                "constant-return native subset requires a return terminator",
            ));
        }
    };
    Ok(ConstantReturnCandidate { value, arity })
}

#[cfg(test)]
fn constant_operand_value(
    unit: &IrUnit,
    registers: &BTreeMap<RegId, i64>,
    operand: &Operand,
) -> Result<i64, CraneliftLoweringError> {
    match operand {
        Operand::Constant(constant) => constant_value(unit, *constant),
        Operand::Register(register) => registers.get(register).copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_REGISTER",
                format!("register {} is not a known constant", register.raw()),
            )
        }),
        Operand::Local(local) => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CONSTANT_RETURN_LOCAL",
            format!(
                "local {} is outside constant-return native subset",
                local.raw()
            ),
        )),
    }
}

#[cfg(test)]
fn packed_array_fetch_candidate(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<PackedArrayFetchCandidate, CraneliftLoweringError> {
    let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_FUNCTION",
            format!("function id {} is not present", function.raw()),
        )
    })?;
    if ir_function.blocks.len() != 1 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FETCH_CONTROL_FLOW",
            "packed-array fetch subset requires one straight-line block",
        ));
    }
    if !matches!(ir_function.return_type.as_ref(), Some(IrReturnType::Int)) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FETCH_RETURN",
            "packed-array fetch subset requires declared int return",
        ));
    }
    let [array_param, index_param] = ir_function.params.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FETCH_PARAMS",
            "packed-array fetch subset requires array and int parameters",
        ));
    };
    check_packed_fetch_param(array_param, IrReturnType::Array, "array")?;
    check_packed_fetch_param(index_param, IrReturnType::Int, "index")?;

    let block = &ir_function.blocks[0];
    let mut array_reg = None;
    let mut index_reg = None;
    let mut fetch_reg = None;
    for instruction in &block.instructions {
        match &instruction.kind {
            InstructionKind::LoadLocal { dst, local } if *local == array_param.local => {
                array_reg = Some(*dst);
            }
            InstructionKind::LoadLocal { dst, local } if *local == index_param.local => {
                index_reg = Some(*dst);
            }
            InstructionKind::FetchDim {
                dst,
                array,
                key,
                quiet: false,
            } => {
                if *array
                    != Operand::Register(array_reg.ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_PACKED_FETCH_SHAPE",
                            "fetch_dim appears before array local load",
                        )
                    })?)
                    || *key
                        != Operand::Register(index_reg.ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_PACKED_FETCH_SHAPE",
                                "fetch_dim appears before index local load",
                            )
                        })?)
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PACKED_FETCH_SHAPE",
                        "fetch_dim operands do not match array and index parameters",
                    ));
                }
                fetch_reg = Some(*dst);
            }
            other => {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_PACKED_FETCH_OPCODE",
                    format!("instruction {other:?} is outside packed-array fetch subset"),
                ));
            }
        }
    }
    match &block.terminator {
        Some(Terminator {
            kind:
                TerminatorKind::Return {
                    value: Some(Operand::Register(return_reg)),
                    by_ref_local: None,
                },
            ..
        }) if Some(*return_reg) == fetch_reg => Ok(PackedArrayFetchCandidate {
            array_param: array_param.local,
            index_param: index_param.local,
        }),
        Some(other) => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FETCH_TERMINATOR",
            format!(
                "terminator {:?} is outside packed-array fetch subset",
                other.kind
            ),
        )),
        None => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FETCH_TERMINATOR",
            "packed-array fetch subset requires a return terminator",
        )),
    }
}

#[cfg(test)]
fn check_packed_fetch_param(
    param: &IrParam,
    expected: IrReturnType,
    role: &'static str,
) -> Result<(), CraneliftLoweringError> {
    if param.by_ref || param.variadic || param.default.is_some() {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FETCH_PARAMS",
            format!("packed-array fetch {role} parameter must be required by-value"),
        ));
    }
    if param.type_.as_ref() != Some(&expected) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FETCH_PARAMS",
            format!("packed-array fetch {role} parameter has wrong type"),
        ));
    }
    Ok(())
}

#[cfg(test)]
fn packed_foreach_int_sum_candidate(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<PackedForeachIntSumCandidate, CraneliftLoweringError> {
    let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_FUNCTION",
            format!("function id {} is not present", function.raw()),
        )
    })?;
    if !matches!(ir_function.return_type.as_ref(), Some(IrReturnType::Int)) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_RETURN",
            "packed foreach sum subset requires declared int return",
        ));
    }
    let [array_param] = ir_function.params.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_PARAMS",
            "packed foreach sum subset requires one array parameter",
        ));
    };
    if array_param.by_ref
        || array_param.variadic
        || array_param.default.is_some()
        || array_param.type_.as_ref() != Some(&IrReturnType::Array)
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_PARAMS",
            "packed foreach sum array parameter must be required by-value array",
        ));
    }
    let [entry, condition, body, after] = ir_function.blocks.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONTROL_FLOW",
            "packed foreach sum subset requires four canonical blocks",
        ));
    };

    let [
        init_value,
        store_sum,
        discard_init,
        load_array,
        foreach_init,
    ] = entry.instructions.as_slice()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum entry block has unexpected instructions",
        ));
    };
    let InstructionKind::LoadConst {
        dst: zero_reg,
        constant,
    } = init_value.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum must initialize the accumulator from a constant",
        ));
    };
    if unit.constants.get(constant.index()) != Some(&IrConstant::Int(0)) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum accumulator must start at integer zero",
        ));
    }
    let InstructionKind::StoreLocal {
        local: sum_local,
        src: Operand::Register(store_reg),
    } = store_sum.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum must store zero to an accumulator local",
        ));
    };
    if store_reg != zero_reg {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
            "accumulator store must use the initialized zero register",
        ));
    }
    match discard_init.kind.clone() {
        InstructionKind::Discard {
            src: Operand::Register(reg),
        } if reg == zero_reg => {}
        _ => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
                "packed foreach sum entry must discard the initializer result",
            ));
        }
    }
    let InstructionKind::LoadLocal {
        dst: array_reg,
        local,
    } = load_array.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum must load the array parameter",
        ));
    };
    if local != array_param.local {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
            "foreach source must be the array parameter",
        ));
    }
    let InstructionKind::ForeachInit {
        iterator,
        source: Operand::Register(source_reg),
    } = foreach_init.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum must use by-value foreach init",
        ));
    };
    if source_reg != array_reg {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_ENTRY",
            "foreach init source must be the array parameter load",
        ));
    }
    match &entry.terminator {
        Some(terminator) => match terminator.kind.clone() {
            TerminatorKind::Jump { target } if target == condition.id => {}
            _ => {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                    "entry block must jump to the foreach condition",
                ));
            }
        },
        None => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                "entry block requires a terminator",
            ));
        }
    }

    let [foreach_next] = condition.instructions.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONDITION",
            "condition block must contain one foreach_next",
        ));
    };
    let InstructionKind::ForeachNext {
        has_value,
        iterator: next_iterator,
        key: None,
        value,
    } = foreach_next.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONDITION",
            "packed foreach sum must be by-value without key binding",
        ));
    };
    if next_iterator != iterator {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONDITION",
            "foreach_next iterator does not match foreach_init",
        ));
    }
    match &condition.terminator {
        Some(terminator) => match terminator.kind.clone() {
            TerminatorKind::JumpIf {
                condition: Operand::Register(condition_reg),
                if_true,
                if_false,
            } if condition_reg == has_value && if_true == body.id && if_false == after.id => {}
            _ => {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                    "foreach condition must branch to the body or return block",
                ));
            }
        },
        None => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                "condition block requires a terminator",
            ));
        }
    }

    let [
        store_value,
        load_sum,
        load_value,
        add,
        store_accumulator,
        discard_add,
    ] = body.instructions.as_slice()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body must contain only element store and int accumulation",
        ));
    };
    let InstructionKind::StoreLocal {
        local: value_local,
        src: Operand::Register(stored_value_reg),
    } = store_value.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
            "body must store the current element local",
        ));
    };
    if stored_value_reg != value {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
            "stored element must come from foreach_next",
        ));
    }
    let InstructionKind::LoadLocal {
        dst: loaded_sum,
        local: loaded_sum_local,
    } = load_sum.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
            "body must load the accumulator",
        ));
    };
    if loaded_sum_local != sum_local {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
            "body must load the initialized accumulator",
        ));
    }
    let InstructionKind::LoadLocal {
        dst: loaded_value,
        local: loaded_value_local,
    } = load_value.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
            "body must load the current element",
        ));
    };
    if loaded_value_local != value_local {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
            "body must add the current element local",
        ));
    }
    let InstructionKind::Binary {
        dst: add_result,
        op: BinaryOp::Add,
        lhs: Operand::Register(add_lhs),
        rhs: Operand::Register(add_rhs),
    } = add.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
            "body must contain one integer addition",
        ));
    };
    if add_lhs != loaded_sum || add_rhs != loaded_value {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
            "addition must use accumulator plus current element",
        ));
    }
    match store_accumulator.kind.clone() {
        InstructionKind::StoreLocal {
            local,
            src: Operand::Register(reg),
        } if local == sum_local && reg == add_result => {}
        _ => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
                "body must store the addition result to the accumulator",
            ));
        }
    }
    match discard_add.kind.clone() {
        InstructionKind::Discard {
            src: Operand::Register(reg),
        } if reg == add_result => {}
        _ => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PACKED_FOREACH_BODY",
                "body must discard the addition result",
            ));
        }
    }
    match &body.terminator {
        Some(terminator) => match terminator.kind.clone() {
            TerminatorKind::Jump { target } if target == condition.id => {}
            _ => {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                    "body must loop back to the condition block",
                ));
            }
        },
        None => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                "body requires a terminator",
            ));
        }
    }

    let return_load = match after.instructions.as_slice() {
        [return_load] => return_load,
        [cleanup, return_load] => {
            match cleanup.kind {
                InstructionKind::ForeachCleanup {
                    iterator: cleanup_iterator,
                } if cleanup_iterator == iterator => {}
                _ => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PACKED_FOREACH_RETURN",
                        "return block cleanup must match the foreach iterator",
                    ));
                }
            }
            return_load
        }
        _ => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PACKED_FOREACH_RETURN",
                "return block must optionally cleanup foreach then load the accumulator",
            ));
        }
    };
    let InstructionKind::LoadLocal {
        dst: return_reg,
        local: return_local,
    } = return_load.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_RETURN",
            "return block must load the accumulator",
        ));
    };
    if return_local != sum_local {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_RETURN",
            "return block must read the accumulator local",
        ));
    }
    match &after.terminator {
        Some(terminator) => match terminator.kind.clone() {
            TerminatorKind::Return {
                value: Some(Operand::Register(reg)),
                by_ref_local: None,
            } if reg == return_reg => Ok(PackedForeachIntSumCandidate {
                array_param: array_param.local,
            }),
            _ => Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PACKED_FOREACH_RETURN",
                "packed foreach sum must return the accumulator by value",
            )),
        },
        None => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_RETURN",
            "return block requires a terminator",
        )),
    }
}

#[cfg(test)]
fn known_call_candidate(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<KnownCallCandidate, CraneliftLoweringError> {
    let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_FUNCTION",
            format!("function id {} is not present", function.raw()),
        )
    })?;
    if ir_function.flags.is_top_level
        || ir_function.flags.is_closure
        || ir_function.flags.is_method
        || ir_function.flags.is_generator
        || ir_function.returns_by_ref
        || !ir_function.captures.is_empty()
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_SHAPE",
            "known-call fast path requires an ordinary leaf function",
        ));
    }
    if !matches!(ir_function.return_type.as_ref(), Some(IrReturnType::Int)) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_RETURN",
            "known-call fast path requires declared int return",
        ));
    }
    let [param] = ir_function.params.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_PARAMS",
            "known-call fast path requires one argument parameter",
        ));
    };
    if param.by_ref || param.variadic || param.default.is_some() {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_PARAMS",
            "known-call parameter must be required and by-value",
        ));
    }

    let [block] = ir_function.blocks.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_CONTROL_FLOW",
            "known-call fast path requires one straight-line block",
        ));
    };
    let [load, call] = block.instructions.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_INSTRUCTIONS",
            "known-call fast path expects load-local then call",
        ));
    };
    let InstructionKind::LoadLocal {
        dst: loaded,
        local: loaded_local,
    } = load.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_LOAD",
            "known-call fast path must load its sole parameter",
        ));
    };
    if loaded_local != param.local {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_LOAD",
            "known-call fast path load must read the sole parameter local",
        ));
    }
    let InstructionKind::CallFunction { dst, name, args } = &call.kind else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_OPCODE",
            "known-call fast path expects a direct function call",
        ));
    };
    let kind = match name.as_str() {
        "strlen" => KnownCallKind::Strlen,
        "count" => KnownCallKind::Count,
        _ => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_KNOWN_CALL_TARGET",
                "known-call fast path only supports strlen and count",
            ));
        }
    };
    if unit
        .function_table
        .iter()
        .any(|entry| entry.name.eq_ignore_ascii_case(kind.function_name()))
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_OVERRIDE",
            "known-call fast path rejected a user function override ambiguity",
        ));
    }
    let [arg] = args.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_ARITY",
            "known-call fast path requires exactly one call argument",
        ));
    };
    if arg.name.is_some() || arg.unpack {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_ARGUMENT_MODE",
            "known-call fast path rejects named and unpacked arguments",
        ));
    }
    if arg.value != Operand::Register(loaded) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_ARGUMENT",
            "known-call fast path argument must be the loaded parameter",
        ));
    }
    match (kind, param.type_.as_ref()) {
        (KnownCallKind::Strlen, None | Some(IrReturnType::String))
        | (KnownCallKind::Count, None | Some(IrReturnType::Array)) => {}
        (KnownCallKind::Strlen, _) => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_KNOWN_CALL_PARAM_TYPE",
                "strlen known-call parameter must be string-typed or untyped",
            ));
        }
        (KnownCallKind::Count, _) => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_KNOWN_CALL_PARAM_TYPE",
                "count known-call parameter must be array-typed or untyped",
            ));
        }
    }
    let Some(Terminator {
        kind:
            TerminatorKind::Return {
                value: Some(Operand::Register(return_reg)),
                by_ref_local: None,
            },
        ..
    }) = block.terminator.as_ref()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_RETURN",
            "known-call fast path must return the call result",
        ));
    };
    if *return_reg != *dst {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_RETURN",
            "known-call fast path return must use the call destination",
        ));
    }

    Ok(KnownCallCandidate {
        kind,
        value_param: param.local,
    })
}

#[cfg(test)]
struct RecordArrayLookupCandidate {
    #[allow(dead_code)]
    array_param: LocalId,
    #[allow(dead_code)]
    key_param: LocalId,
}

#[cfg(test)]
struct NativeRecordArrayLookupCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

/// Leaf shape `function f(array $map, string $key) { return $map[$key]; }`
/// with untyped return: the record-shape and key-symbol guards live in the
/// runtime helper, which reports exact side-exit statuses.
#[cfg(test)]
fn record_array_lookup_candidate(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<RecordArrayLookupCandidate, CraneliftLoweringError> {
    let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_FUNCTION",
            format!("function id {} is not present", function.raw()),
        )
    })?;
    if ir_function.flags.is_top_level
        || ir_function.flags.is_closure
        || ir_function.flags.is_method
        || ir_function.flags.is_generator
        || ir_function.returns_by_ref
        || !ir_function.captures.is_empty()
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_SHAPE",
            "record-lookup fast path requires an ordinary leaf function",
        ));
    }
    if ir_function.return_type.is_some() {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_RETURN",
            "record-lookup fast path requires an undeclared return type",
        ));
    }
    let [array_param, key_param] = ir_function.params.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_PARAMS",
            "record-lookup fast path requires array and string parameters",
        ));
    };
    for (param, expected) in [
        (array_param, IrReturnType::Array),
        (key_param, IrReturnType::String),
    ] {
        if param.by_ref || param.variadic || param.default.is_some() {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_PARAMS",
                "record-lookup parameters must be required and by-value",
            ));
        }
        if param.type_.as_ref() != Some(&expected) {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_PARAM_TYPE",
                "record-lookup fast path requires declared array and string operands",
            ));
        }
    }
    let [block] = ir_function.blocks.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_CONTROL_FLOW",
            "record-lookup fast path requires one straight-line block",
        ));
    };
    let [load_array, load_key, fetch] = block.instructions.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_INSTRUCTIONS",
            "record-lookup fast path expects load, load, fetch_dim",
        ));
    };
    let InstructionKind::LoadLocal {
        dst: array_reg,
        local: array_local,
    } = load_array.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_LOAD",
            "record-lookup fast path must load the array parameter",
        ));
    };
    let InstructionKind::LoadLocal {
        dst: key_reg,
        local: key_local,
    } = load_key.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_LOAD",
            "record-lookup fast path must load the key parameter",
        ));
    };
    if array_local != array_param.local || key_local != key_param.local {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_LOAD",
            "record-lookup operands must load the declared parameters",
        ));
    }
    let InstructionKind::FetchDim {
        dst,
        array: Operand::Register(fetch_array),
        key: Operand::Register(fetch_key),
        quiet: false,
    } = fetch.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_OPCODE",
            "record-lookup fast path expects a loud fetch_dim",
        ));
    };
    if fetch_array != array_reg || fetch_key != key_reg {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_SHAPE",
            "fetch_dim operands must match the loaded parameters",
        ));
    }
    match &block.terminator {
        Some(Terminator {
            kind:
                TerminatorKind::Return {
                    value: Some(Operand::Register(return_reg)),
                    by_ref_local: None,
                },
            ..
        }) if *return_reg == dst => Ok(RecordArrayLookupCandidate {
            array_param: array_param.local,
            key_param: key_param.local,
        }),
        other => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_TERMINATOR",
            format!("terminator {other:?} is outside record-lookup subset"),
        )),
    }
}

#[cfg(test)]
fn compile_record_array_lookup_native(
    function: FunctionId,
    _candidate: &RecordArrayLookupCandidate,
    helper_address: usize,
    request: &JitCompileRequest,
) -> Result<NativeRecordArrayLookupCompileResult, CraneliftLoweringError> {
    if helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_RECORD_LOOKUP_HELPER",
            "record-lookup fast path requires a runtime helper address",
        ));
    }

    let helper_symbol = runtime_helper_symbol(RECORD_ARRAY_LOOKUP_HELPER_SYMBOL, helper_address);
    let compiled = compile_managed_native(
        request,
        function,
        "record-array-lookup-v1",
        &[(helper_symbol.as_str(), helper_address)],
        |module, name| {
            let pointer_type = module.target_config().pointer_type();

            let mut helper_signature = module.make_signature();
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.returns.push(AbiParam::new(types::I32));
            let helper = module
                .declare_function(&helper_symbol, Linkage::Import, &helper_signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare record-lookup helper import: {error}"),
                    )
                })?;

            let mut signature = module.make_signature();
            signature.params.push(AbiParam::new(pointer_type));
            signature.params.push(AbiParam::new(pointer_type));
            signature.params.push(AbiParam::new(pointer_type));
            signature.returns.push(AbiParam::new(types::I32));

            let func_id = module
                .declare_function(name, Linkage::Local, &signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare native record-lookup function: {error}"),
                    )
                })?;
            let mut ctx = module.make_context();
            ctx.func.signature = signature;
            ctx.func.name = UserFuncName::user(0, func_id.as_u32());

            let mut builder_context = FunctionBuilderContext::new();
            {
                let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);
                let entry = builder.create_block();
                builder.append_block_params_for_function_params(entry);
                builder.switch_to_block(entry);
                let params = builder.block_params(entry).to_vec();
                let array_ptr = params[0];
                let key_ptr = params[1];
                let out_ptr = params[2];
                let helper_ref = module.declare_func_in_func(helper, builder.func);
                let call = builder
                    .ins()
                    .call(helper_ref, &[array_ptr, key_ptr, out_ptr]);
                let status = builder.inst_results(call)[0];
                builder.ins().return_(&[status]);
                builder.seal_block(entry);
                builder.finalize();
            }

            let verifier_flags = settings::Flags::new(settings::builder());
            verify_function(&ctx.func, &verifier_flags).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_VERIFIER",
                    format!("Cranelift verifier rejected native record-lookup IR: {error}"),
                )
            })?;
            module.define_function(func_id, &mut ctx).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DEFINE",
                    format!("failed to define native record-lookup function: {error}"),
                )
            })?;
            let code_bytes = ctx
                .compiled_code()
                .map(|compiled| compiled.code_buffer().len() as u64)
                .unwrap_or(0);
            module.clear_context(&mut ctx);
            module.finalize_definitions().map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize native record-lookup function: {error}"),
                )
            })?;
            let address = module.get_finalized_function(func_id) as usize;
            let handle = JitFunctionHandle::value_value_status_out_native(
                u64::from(function.raw()) + 1,
                request.region_id.clone(),
                CraneliftCompilerIdentity,
                address,
                code_bytes,
                1,
                1,
                JitNativeSpecialization::RecordArrayLookup,
            );
            Ok((handle, code_bytes))
        },
    )?;
    Ok(NativeRecordArrayLookupCompileResult {
        handle: compiled.handle,
        code_bytes: compiled.code_bytes,
    })
}

#[cfg(test)]
fn string_concat_candidate(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<StringConcatCandidate, CraneliftLoweringError> {
    let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_FUNCTION",
            format!("function id {} is not present", function.raw()),
        )
    })?;
    if ir_function.flags.is_top_level
        || ir_function.flags.is_closure
        || ir_function.flags.is_method
        || ir_function.flags.is_generator
        || ir_function.returns_by_ref
        || !ir_function.captures.is_empty()
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_SHAPE",
            "string-concat fast path requires an ordinary leaf function",
        ));
    }
    if !matches!(ir_function.return_type.as_ref(), Some(IrReturnType::String)) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_RETURN",
            "string-concat fast path requires declared string return",
        ));
    }
    let [lhs_param, rhs_param] = ir_function.params.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_PARAMS",
            "string-concat fast path requires exactly two parameters",
        ));
    };
    for param in [lhs_param, rhs_param] {
        if param.by_ref || param.variadic || param.default.is_some() {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_STRING_CONCAT_PARAMS",
                "string-concat parameters must be required and by-value",
            ));
        }
        if !matches!(param.type_.as_ref(), Some(IrReturnType::String)) {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_STRING_CONCAT_PARAM_TYPE",
                "string-concat fast path requires declared string operands",
            ));
        }
    }

    let [block] = ir_function.blocks.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_CONTROL_FLOW",
            "string-concat fast path requires one straight-line block",
        ));
    };
    let [load_lhs, load_rhs, concat] = block.instructions.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_INSTRUCTIONS",
            "string-concat fast path expects load, load, concat",
        ));
    };
    let InstructionKind::LoadLocal {
        dst: loaded_lhs,
        local: loaded_lhs_local,
    } = load_lhs.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_LOAD",
            "string-concat fast path must load the left parameter",
        ));
    };
    if loaded_lhs_local != lhs_param.local {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_LOAD",
            "left concat operand must load the left parameter",
        ));
    }
    let InstructionKind::LoadLocal {
        dst: loaded_rhs,
        local: loaded_rhs_local,
    } = load_rhs.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_LOAD",
            "string-concat fast path must load the right parameter",
        ));
    };
    if loaded_rhs_local != rhs_param.local {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_LOAD",
            "right concat operand must load the right parameter",
        ));
    }
    let InstructionKind::Binary {
        dst,
        op: BinaryOp::Concat,
        lhs: Operand::Register(lhs_reg),
        rhs: Operand::Register(rhs_reg),
    } = concat.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_OPCODE",
            "string-concat fast path expects a binary concat",
        ));
    };
    if lhs_reg != loaded_lhs || rhs_reg != loaded_rhs {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_OPERANDS",
            "string-concat operands must be the loaded parameters",
        ));
    }
    let Some(Terminator {
        kind:
            TerminatorKind::Return {
                value: Some(Operand::Register(return_reg)),
                by_ref_local: None,
            },
        ..
    }) = block.terminator.as_ref()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_RETURN",
            "string-concat fast path must return the concat result",
        ));
    };
    if *return_reg != dst {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_RETURN",
            "string-concat fast path return must use the concat destination",
        ));
    }

    Ok(StringConcatCandidate {
        lhs_param: lhs_param.local,
        rhs_param: rhs_param.local,
    })
}

#[cfg(test)]
fn property_load_candidate(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<PropertyLoadCandidate, CraneliftLoweringError> {
    let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_FUNCTION",
            format!("function id {} is not present", function.raw()),
        )
    })?;
    if ir_function.flags.is_top_level
        || ir_function.flags.is_closure
        || ir_function.flags.is_method
        || ir_function.flags.is_generator
        || ir_function.returns_by_ref
        || !ir_function.captures.is_empty()
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_SHAPE",
            "property-load fast path requires an ordinary leaf function",
        ));
    }
    // The native result bypasses the interpreter's return-site coercion, so
    // this tier only admits scalar return types and (below) requires the
    // declared property type to be the *same* scalar — the typed-property
    // invariant then proves the runtime value already matches and no coercion
    // (`bool` → `int(1)`, `int` → `float`, `TypeError`) is skipped.
    if !matches!(
        ir_function.return_type.as_ref(),
        Some(IrReturnType::Int | IrReturnType::Float | IrReturnType::Bool)
    ) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_RETURN",
            "property-load fast path requires a scalar return type",
        ));
    }
    let [param] = ir_function.params.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_PARAMS",
            "property-load fast path requires one object parameter",
        ));
    };
    if param.by_ref || param.variadic || param.default.is_some() {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_PARAMS",
            "property-load parameter must be required and by-value",
        ));
    }
    let Some(IrReturnType::Class { name, .. }) = param.type_.as_ref() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_PARAM_TYPE",
            "property-load parameter must have a class type",
        ));
    };
    let class = lookup_class(unit, name).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_CLASS",
            format!("property-load class `{name}` is not present in the IR unit"),
        )
    })?;

    let [block] = ir_function.blocks.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_CONTROL_FLOW",
            "property-load fast path requires one straight-line block",
        ));
    };
    let [load, fetch] = block.instructions.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_INSTRUCTIONS",
            "property-load fast path expects load-local then fetch-property",
        ));
    };
    let InstructionKind::LoadLocal {
        dst: loaded,
        local: loaded_local,
    } = load.kind.clone()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_LOAD",
            "property-load fast path must load the object parameter",
        ));
    };
    if loaded_local != param.local {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_LOAD",
            "property-load fast path load must read the object parameter local",
        ));
    }
    let InstructionKind::FetchProperty {
        dst,
        object: Operand::Register(object_reg),
        property,
    } = &fetch.kind
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_OPCODE",
            "property-load fast path expects a direct property fetch",
        ));
    };
    if *object_reg != loaded {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_OBJECT",
            "property-load fast path object must be the loaded parameter",
        ));
    }
    let Some((declaring_class, property_entry)) = lookup_property_in_unit(unit, class, property)
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_DECLARED",
            "property-load fast path requires a declared property",
        ));
    };
    if property_entry.flags.is_static
        || property_entry.flags.is_private
        || property_entry.flags.is_protected
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_VISIBILITY",
            "property-load fast path requires a visible instance property",
        ));
    }
    if property_entry.hooks.get.is_some() || property_entry.hooks.set.is_some() {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_HOOK",
            "property-load fast path rejects property hooks",
        ));
    }
    if class_or_parent_has_public_magic_get(unit, class) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_MAGIC_GET",
            "property-load fast path rejects public __get",
        ));
    }
    // Static exactness rule (see the return-type check above): the property's
    // declared type must be exactly the declared return type, so the committed
    // value provably needs no return-site coercion. Untyped or
    // differently-typed properties reject here.
    if property_entry.type_ != ir_function.return_type {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_RETURN",
            "property-load fast path requires the property type to equal the return type",
        ));
    }
    let Some(Terminator {
        kind:
            TerminatorKind::Return {
                value: Some(Operand::Register(return_reg)),
                by_ref_local: None,
            },
        ..
    }) = block.terminator.as_ref()
    else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_RETURN",
            "property-load fast path must return the property value",
        ));
    };
    if *return_reg != *dst {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_RETURN",
            "property-load fast path return must use the fetch destination",
        ));
    }
    let property_slot_index = declaring_class
        .properties
        .iter()
        .position(|entry| entry.name == property_entry.name)
        .ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_SLOT",
                "property-load metadata could not determine the property slot",
            )
        })?;

    // The helper does not consult this metadata because the static exactness
    // rule above already proves the committed value has this tag.
    let expected_result_tag = match ir_function.return_type.as_ref() {
        Some(IrReturnType::Int) => crate::JitCValueTag::Int as u16,
        Some(IrReturnType::Float) => crate::JitCValueTag::FloatBits as u16,
        Some(IrReturnType::Bool) => crate::JitCValueTag::Bool as u16,
        Some(IrReturnType::String) => crate::JitCValueTag::OpaqueString as u16,
        Some(IrReturnType::Array) => crate::JitCValueTag::OpaqueArray as u16,
        Some(IrReturnType::Object) => crate::JitCValueTag::OpaqueObject as u16,
        Some(IrReturnType::Null) => crate::JitCValueTag::Null as u16,
        _ => 0,
    };
    Ok(PropertyLoadCandidate {
        object_param: param.local,
        metadata: JitPropertyLoadMetadata {
            receiver_class: normalize_class_name(&class.name),
            class_id: class.id.raw(),
            property: property_entry.name.clone(),
            storage_name: property_storage_name(declaring_class, property_entry),
            property_slot_index,
            layout_version: 0,
            expected_result_tag,
        },
    })
}

#[cfg(test)]
fn lookup_class<'a>(unit: &'a IrUnit, name: &str) -> Option<&'a php_ir::module::ClassEntry> {
    let normalized = normalize_class_name(name);
    unit.classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized)
}

#[cfg(test)]
fn lookup_property_in_unit<'a>(
    unit: &'a IrUnit,
    class: &'a php_ir::module::ClassEntry,
    property: &str,
) -> Option<(
    &'a php_ir::module::ClassEntry,
    &'a php_ir::module::ClassPropertyEntry,
)> {
    if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
        return Some((class, entry));
    }
    let parent = class
        .parent
        .as_deref()
        .and_then(|parent| lookup_class(unit, parent))?;
    lookup_property_in_unit(unit, parent, property)
}

#[cfg(test)]
fn class_or_parent_has_public_magic_get(unit: &IrUnit, class: &php_ir::module::ClassEntry) -> bool {
    if class.methods.iter().any(|method| {
        method.name.eq_ignore_ascii_case("__get")
            && !method.flags.is_static
            && !method.flags.is_private
            && !method.flags.is_protected
    }) {
        return true;
    }
    class
        .parent
        .as_deref()
        .and_then(|parent| lookup_class(unit, parent))
        .is_some_and(|parent| class_or_parent_has_public_magic_get(unit, parent))
}

#[cfg(test)]
fn property_storage_name(
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
) -> String {
    if property.flags.is_private {
        format!(
            "private:{}:{}",
            normalize_class_name(&class.name),
            property.name
        )
    } else {
        property.name.clone()
    }
}

#[cfg(test)]
fn normalize_class_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

#[cfg(test)]
fn compile_constant_return_native(
    function: FunctionId,
    value: i64,
    arity: u8,
    request: &JitCompileRequest,
) -> Result<NativeConstantCompileResult, CraneliftLoweringError> {
    let compiled =
        compile_managed_native(request, function, "constant-v1", &[], |module, name| {
            let mut signature = module.make_signature();
            for _ in 0..arity {
                signature.params.push(AbiParam::new(types::I64));
            }
            signature.returns.push(AbiParam::new(types::I64));

            let func_id = module
                .declare_function(name, Linkage::Local, &signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare native function: {error}"),
                    )
                })?;
            let mut ctx = module.make_context();
            ctx.func.signature = signature;
            ctx.func.name = UserFuncName::user(0, func_id.as_u32());
            let mut builder_context = FunctionBuilderContext::new();
            {
                let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);
                let block = builder.create_block();
                builder.append_block_params_for_function_params(block);
                builder.switch_to_block(block);
                builder.seal_block(block);
                let constant = builder.ins().iconst(types::I64, value);
                builder.ins().return_(&[constant]);
                builder.finalize();
            }
            let verifier_flags = settings::Flags::new(settings::builder());
            verify_function(&ctx.func, &verifier_flags).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_VERIFIER",
                    format!("Cranelift verifier rejected native constant-return IR: {error}"),
                )
            })?;
            module.define_function(func_id, &mut ctx).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DEFINE",
                    format!("failed to define native function: {error}"),
                )
            })?;
            let code_bytes = ctx
                .compiled_code()
                .map(|compiled| compiled.code_buffer().len() as u64)
                .unwrap_or(0);
            module.clear_context(&mut ctx);
            module.finalize_definitions().map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize native function: {error}"),
                )
            })?;
            let address = module.get_finalized_function(func_id) as usize;
            let handle = JitFunctionHandle::i64_native(
                u64::from(function.raw()) + 1,
                request.region_id.clone(),
                CraneliftCompilerIdentity,
                address,
                arity,
                code_bytes,
            );
            Ok((handle, code_bytes))
        })?;
    Ok(NativeConstantCompileResult {
        handle: compiled.handle,
        code_bytes: compiled.code_bytes,
    })
}

#[cfg(test)]
fn compile_packed_array_fetch_native(
    function: FunctionId,
    _candidate: &PackedArrayFetchCandidate,
    helper_address: usize,
    request: &JitCompileRequest,
) -> Result<NativePackedArrayFetchCompileResult, CraneliftLoweringError> {
    if helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FETCH_HELPER",
            "packed-array fetch requires a runtime helper address",
        ));
    }

    let helper_symbol = runtime_helper_symbol(PACKED_ARRAY_FETCH_HELPER_SYMBOL, helper_address);
    let compiled = compile_managed_native(
        request,
        function,
        "packed-array-fetch-v1",
        &[(helper_symbol.as_str(), helper_address)],
        |module, name| {
            let pointer_type = module.target_config().pointer_type();

            let mut helper_signature = module.make_signature();
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.params.push(AbiParam::new(types::I64));
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.returns.push(AbiParam::new(types::I32));
            let helper_func = module
                .declare_function(&helper_symbol, Linkage::Import, &helper_signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare packed-array helper import: {error}"),
                    )
                })?;

            let mut signature = module.make_signature();
            signature.params.push(AbiParam::new(pointer_type));
            signature.params.push(AbiParam::new(types::I64));
            signature.params.push(AbiParam::new(pointer_type));
            signature.returns.push(AbiParam::new(types::I32));

            let func_id = module
                .declare_function(name, Linkage::Local, &signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare native packed-array fetch function: {error}"),
                    )
                })?;
            let mut ctx = module.make_context();
            ctx.func.signature = signature;
            ctx.func.name = UserFuncName::user(0, func_id.as_u32());

            let mut builder_context = FunctionBuilderContext::new();
            {
                let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);
                let entry = builder.create_block();
                let bounds_exit = builder.create_block();
                let helper_call = builder.create_block();
                builder.append_block_params_for_function_params(entry);

                builder.switch_to_block(entry);
                let params = builder.block_params(entry).to_vec();
                let value_ptr = params[0];
                let index = params[1];
                let out_ptr = params[2];
                let negative_index = builder.ins().icmp_imm(IntCC::SignedLessThan, index, 0);
                builder
                    .ins()
                    .brif(negative_index, bounds_exit, &[], helper_call, &[]);

                builder.switch_to_block(bounds_exit);
                builder.seal_block(bounds_exit);
                let status = builder
                    .ins()
                    .iconst(types::I32, i64::from(JIT_PACKED_ARRAY_STATUS_BOUNDS_EXIT));
                builder.ins().return_(&[status]);

                builder.switch_to_block(helper_call);
                builder.seal_block(helper_call);
                let helper_ref = module.declare_func_in_func(helper_func, builder.func);
                let call = builder.ins().call(helper_ref, &[value_ptr, index, out_ptr]);
                let status = builder.inst_results(call)[0];
                builder.ins().return_(&[status]);

                builder.seal_block(entry);
                builder.finalize();
            }

            let verifier_flags = settings::Flags::new(settings::builder());
            verify_function(&ctx.func, &verifier_flags).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_VERIFIER",
                    format!("Cranelift verifier rejected native packed-array fetch IR: {error}"),
                )
            })?;
            module.define_function(func_id, &mut ctx).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DEFINE",
                    format!("failed to define native packed-array fetch function: {error}"),
                )
            })?;
            let code_bytes = ctx
                .compiled_code()
                .map(|compiled| compiled.code_buffer().len() as u64)
                .unwrap_or(0);
            module.clear_context(&mut ctx);
            module.finalize_definitions().map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize native packed-array fetch function: {error}"),
                )
            })?;
            let address = module.get_finalized_function(func_id) as usize;
            let handle = JitFunctionHandle::value_i64_status_out_native(
                u64::from(function.raw()) + 1,
                request.region_id.clone(),
                CraneliftCompilerIdentity,
                address,
                code_bytes,
                1,
                1,
            );
            Ok((handle, code_bytes))
        },
    )?;
    Ok(NativePackedArrayFetchCompileResult {
        handle: compiled.handle,
        code_bytes: compiled.code_bytes,
    })
}

#[cfg(test)]
fn compile_packed_foreach_int_sum_native(
    function: FunctionId,
    _candidate: &PackedForeachIntSumCandidate,
    len_helper_address: usize,
    fetch_helper_address: usize,
    request: &JitCompileRequest,
) -> Result<NativePackedForeachIntSumCompileResult, CraneliftLoweringError> {
    if len_helper_address == 0 || fetch_helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_HELPER",
            "packed foreach sum requires length and fetch runtime helper addresses",
        ));
    }

    let len_helper_symbol =
        runtime_helper_symbol(PACKED_ARRAY_LEN_HELPER_SYMBOL, len_helper_address);
    let fetch_helper_symbol =
        runtime_helper_symbol(PACKED_ARRAY_FETCH_HELPER_SYMBOL, fetch_helper_address);
    let compiled = compile_managed_native(
        request,
        function,
        "packed-foreach-int-sum-v1",
        &[
            (len_helper_symbol.as_str(), len_helper_address),
            (fetch_helper_symbol.as_str(), fetch_helper_address),
        ],
        |module, name| {
            let pointer_type = module.target_config().pointer_type();

            let mut len_signature = module.make_signature();
            len_signature.params.push(AbiParam::new(pointer_type));
            len_signature.params.push(AbiParam::new(pointer_type));
            len_signature.returns.push(AbiParam::new(types::I32));
            let len_helper = module
                .declare_function(&len_helper_symbol, Linkage::Import, &len_signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare packed-array length helper import: {error}"),
                    )
                })?;

            let mut fetch_signature = module.make_signature();
            fetch_signature.params.push(AbiParam::new(pointer_type));
            fetch_signature.params.push(AbiParam::new(types::I64));
            fetch_signature.params.push(AbiParam::new(pointer_type));
            fetch_signature.returns.push(AbiParam::new(types::I32));
            let fetch_helper = module
                .declare_function(&fetch_helper_symbol, Linkage::Import, &fetch_signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare packed-array fetch helper import: {error}"),
                    )
                })?;

            let mut signature = module.make_signature();
            signature.params.push(AbiParam::new(pointer_type));
            signature.params.push(AbiParam::new(pointer_type));
            signature.returns.push(AbiParam::new(types::I32));

            let func_id = module
                .declare_function(name, Linkage::Local, &signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare native packed-foreach sum function: {error}"),
                    )
                })?;
            let mut ctx = module.make_context();
            ctx.func.signature = signature;
            ctx.func.name = UserFuncName::user(0, func_id.as_u32());

            let mut builder_context = FunctionBuilderContext::new();
            {
                let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);
                let entry = builder.create_block();
                let len_ready = builder.create_block();
                let loop_header = builder.create_block();
                let loop_body = builder.create_block();
                let continue_block = builder.create_block();
                let advance_block = builder.create_block();
                let done_block = builder.create_block();
                let non_zero_status = builder.create_block();
                let remap_fetch_status = builder.create_block();
                let overflow_exit = builder.create_block();
                builder.append_block_params_for_function_params(entry);
                builder.append_block_param(loop_header, types::I64);
                builder.append_block_param(loop_header, types::I64);
                builder.append_block_param(non_zero_status, types::I32);
                builder.append_block_param(remap_fetch_status, types::I32);

                builder.switch_to_block(entry);
                let params = builder.block_params(entry).to_vec();
                let value_ptr = params[0];
                let out_ptr = params[1];

                let len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let element_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let len_out = builder.ins().stack_addr(pointer_type, len_slot, 0);
                let len_ref = module.declare_func_in_func(len_helper, builder.func);
                let len_call = builder.ins().call(len_ref, &[value_ptr, len_out]);
                let len_status = builder.inst_results(len_call)[0];
                let ok_status = builder
                    .ins()
                    .iconst(types::I32, i64::from(crate::JIT_HELPER_STATUS_OK));
                let len_ok = builder.ins().icmp(IntCC::Equal, len_status, ok_status);
                let len_status_args = [len_status.into()];
                builder
                    .ins()
                    .brif(len_ok, len_ready, &[], non_zero_status, &len_status_args);

                builder.switch_to_block(len_ready);
                builder.seal_block(len_ready);
                let zero = builder.ins().iconst(types::I64, 0);
                let length = builder.ins().stack_load(types::I64, len_slot, 0);
                let initial_loop_args = [zero.into(), zero.into()];
                builder.ins().jump(loop_header, &initial_loop_args);

                builder.switch_to_block(loop_header);
                let loop_params = builder.block_params(loop_header).to_vec();
                let index = loop_params[0];
                let sum = loop_params[1];
                let done = builder
                    .ins()
                    .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
                builder.ins().brif(done, done_block, &[], loop_body, &[]);

                builder.switch_to_block(loop_body);
                let element_out = builder.ins().stack_addr(pointer_type, element_slot, 0);
                let fetch_ref = module.declare_func_in_func(fetch_helper, builder.func);
                let fetch_call = builder
                    .ins()
                    .call(fetch_ref, &[value_ptr, index, element_out]);
                let fetch_status = builder.inst_results(fetch_call)[0];
                let fetch_ok = builder.ins().icmp(IntCC::Equal, fetch_status, ok_status);
                let fetch_status_args = [fetch_status.into()];
                builder.ins().brif(
                    fetch_ok,
                    continue_block,
                    &[],
                    remap_fetch_status,
                    &fetch_status_args,
                );

                builder.switch_to_block(continue_block);
                let element = builder.ins().stack_load(types::I64, element_slot, 0);
                let (next_sum, overflow) = builder.ins().sadd_overflow(sum, element);
                builder
                    .ins()
                    .brif(overflow, overflow_exit, &[], advance_block, &[]);

                builder.switch_to_block(advance_block);
                builder.seal_block(advance_block);
                let one = builder.ins().iconst(types::I64, 1);
                let next_index = builder.ins().iadd(index, one);
                let next_loop_args = [next_index.into(), next_sum.into()];
                builder.ins().jump(loop_header, &next_loop_args);

                builder.switch_to_block(done_block);
                builder.seal_block(done_block);
                builder.ins().store(MemFlagsData::new(), sum, out_ptr, 0);
                builder.ins().return_(&[ok_status]);

                builder.switch_to_block(non_zero_status);
                let status = builder.block_params(non_zero_status)[0];
                builder.ins().return_(&[status]);

                builder.switch_to_block(remap_fetch_status);
                let fetch_status = builder.block_params(remap_fetch_status)[0];
                let bounds_status = builder
                    .ins()
                    .iconst(types::I32, i64::from(JIT_PACKED_ARRAY_STATUS_BOUNDS_EXIT));
                let is_bounds = builder
                    .ins()
                    .icmp(IntCC::Equal, fetch_status, bounds_status);
                let layout_status = builder
                    .ins()
                    .iconst(types::I32, i64::from(JIT_PACKED_ARRAY_STATUS_LAYOUT_EXIT));
                let remapped = builder.ins().select(is_bounds, layout_status, fetch_status);
                builder.ins().return_(&[remapped]);

                builder.switch_to_block(overflow_exit);
                builder.seal_block(overflow_exit);
                let overflow_status = builder
                    .ins()
                    .iconst(types::I32, i64::from(crate::JIT_HELPER_STATUS_OVERFLOW));
                builder.ins().return_(&[overflow_status]);

                builder.seal_block(entry);
                builder.seal_block(loop_header);
                builder.seal_block(loop_body);
                builder.seal_block(continue_block);
                builder.seal_block(non_zero_status);
                builder.seal_block(remap_fetch_status);
                builder.finalize();
            }

            let verifier_flags = settings::Flags::new(settings::builder());
            verify_function(&ctx.func, &verifier_flags).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_VERIFIER",
                    format!("Cranelift verifier rejected native packed-foreach sum IR: {error}"),
                )
            })?;
            module.define_function(func_id, &mut ctx).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DEFINE",
                    format!("failed to define native packed-foreach sum function: {error}"),
                )
            })?;
            let code_bytes = ctx
                .compiled_code()
                .map(|compiled| compiled.code_buffer().len() as u64)
                .unwrap_or(0);
            module.clear_context(&mut ctx);
            module.finalize_definitions().map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize native packed-foreach sum function: {error}"),
                )
            })?;
            let address = module.get_finalized_function(func_id) as usize;
            let handle = JitFunctionHandle::value_status_out_native(
                u64::from(function.raw()) + 1,
                request.region_id.clone(),
                CraneliftCompilerIdentity,
                address,
                code_bytes,
                0,
                1,
                JitNativeSpecialization::PackedForeachIntSum,
            );
            Ok((handle, code_bytes))
        },
    )?;
    Ok(NativePackedForeachIntSumCompileResult {
        handle: compiled.handle,
        code_bytes: compiled.code_bytes,
    })
}

#[cfg(test)]
fn compile_known_call_native(
    function: FunctionId,
    candidate: &KnownCallCandidate,
    helper_address: usize,
    request: &JitCompileRequest,
) -> Result<NativeKnownCallCompileResult, CraneliftLoweringError> {
    if helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_HELPER",
            "known-call fast path requires a runtime helper address",
        ));
    }

    let specialization = format!("known-call-{}-v1", candidate.kind.function_name());
    let helper_symbol = runtime_helper_symbol(candidate.kind.helper_symbol(), helper_address);
    let compiled = compile_managed_native(
        request,
        function,
        &specialization,
        &[(helper_symbol.as_str(), helper_address)],
        |module, name| {
            let pointer_type = module.target_config().pointer_type();

            let mut helper_signature = module.make_signature();
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.returns.push(AbiParam::new(types::I32));
            let helper = module
                .declare_function(&helper_symbol, Linkage::Import, &helper_signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare known-call helper import: {error}"),
                    )
                })?;

            let mut signature = module.make_signature();
            signature.params.push(AbiParam::new(pointer_type));
            signature.params.push(AbiParam::new(pointer_type));
            signature.returns.push(AbiParam::new(types::I32));

            let func_id = module
                .declare_function(name, Linkage::Local, &signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare native known-call function: {error}"),
                    )
                })?;
            let mut ctx = module.make_context();
            ctx.func.signature = signature;
            ctx.func.name = UserFuncName::user(0, func_id.as_u32());

            let mut builder_context = FunctionBuilderContext::new();
            {
                let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);
                let entry = builder.create_block();
                builder.append_block_params_for_function_params(entry);
                builder.switch_to_block(entry);
                let params = builder.block_params(entry).to_vec();
                let value_ptr = params[0];
                let out_ptr = params[1];
                let helper_ref = module.declare_func_in_func(helper, builder.func);
                let call = builder.ins().call(helper_ref, &[value_ptr, out_ptr]);
                let status = builder.inst_results(call)[0];
                builder.ins().return_(&[status]);
                builder.seal_block(entry);
                builder.finalize();
            }

            let verifier_flags = settings::Flags::new(settings::builder());
            verify_function(&ctx.func, &verifier_flags).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_VERIFIER",
                    format!("Cranelift verifier rejected native known-call IR: {error}"),
                )
            })?;
            module.define_function(func_id, &mut ctx).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DEFINE",
                    format!("failed to define native known-call function: {error}"),
                )
            })?;
            let code_bytes = ctx
                .compiled_code()
                .map(|compiled| compiled.code_buffer().len() as u64)
                .unwrap_or(0);
            module.clear_context(&mut ctx);
            module.finalize_definitions().map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize native known-call function: {error}"),
                )
            })?;
            let address = module.get_finalized_function(func_id) as usize;
            let handle = JitFunctionHandle::value_status_out_native(
                u64::from(function.raw()) + 1,
                request.region_id.clone(),
                CraneliftCompilerIdentity,
                address,
                code_bytes,
                1,
                1,
                candidate.kind.specialization(),
            );
            Ok((handle, code_bytes))
        },
    )?;
    Ok(NativeKnownCallCompileResult {
        handle: compiled.handle,
        code_bytes: compiled.code_bytes,
    })
}

#[cfg(test)]
fn compile_string_concat_native(
    function: FunctionId,
    _candidate: &StringConcatCandidate,
    helper_address: usize,
    request: &JitCompileRequest,
) -> Result<NativeStringConcatCompileResult, CraneliftLoweringError> {
    if helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_HELPER",
            "string-concat fast path requires a runtime helper address",
        ));
    }

    let helper_symbol = runtime_helper_symbol(STRING_CONCAT_HELPER_SYMBOL, helper_address);
    let compiled = compile_managed_native(
        request,
        function,
        "string-concat-v1",
        &[(helper_symbol.as_str(), helper_address)],
        |module, name| {
            let pointer_type = module.target_config().pointer_type();

            let mut helper_signature = module.make_signature();
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.returns.push(AbiParam::new(types::I32));
            let helper = module
                .declare_function(&helper_symbol, Linkage::Import, &helper_signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare string-concat helper import: {error}"),
                    )
                })?;

            let mut signature = module.make_signature();
            signature.params.push(AbiParam::new(pointer_type));
            signature.params.push(AbiParam::new(pointer_type));
            signature.params.push(AbiParam::new(pointer_type));
            signature.returns.push(AbiParam::new(types::I32));

            let func_id = module
                .declare_function(name, Linkage::Local, &signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare native string-concat function: {error}"),
                    )
                })?;
            let mut ctx = module.make_context();
            ctx.func.signature = signature;
            ctx.func.name = UserFuncName::user(0, func_id.as_u32());

            let mut builder_context = FunctionBuilderContext::new();
            {
                let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);
                let entry = builder.create_block();
                builder.append_block_params_for_function_params(entry);
                builder.switch_to_block(entry);
                let params = builder.block_params(entry).to_vec();
                let lhs_ptr = params[0];
                let rhs_ptr = params[1];
                let out_ptr = params[2];
                let helper_ref = module.declare_func_in_func(helper, builder.func);
                let call = builder.ins().call(helper_ref, &[lhs_ptr, rhs_ptr, out_ptr]);
                let status = builder.inst_results(call)[0];
                builder.ins().return_(&[status]);
                builder.seal_block(entry);
                builder.finalize();
            }

            let verifier_flags = settings::Flags::new(settings::builder());
            verify_function(&ctx.func, &verifier_flags).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_VERIFIER",
                    format!("Cranelift verifier rejected native string-concat IR: {error}"),
                )
            })?;
            module.define_function(func_id, &mut ctx).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DEFINE",
                    format!("failed to define native string-concat function: {error}"),
                )
            })?;
            let code_bytes = ctx
                .compiled_code()
                .map(|compiled| compiled.code_buffer().len() as u64)
                .unwrap_or(0);
            module.clear_context(&mut ctx);
            module.finalize_definitions().map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize native string-concat function: {error}"),
                )
            })?;
            let address = module.get_finalized_function(func_id) as usize;
            let handle = JitFunctionHandle::value_value_status_out_native(
                u64::from(function.raw()) + 1,
                request.region_id.clone(),
                CraneliftCompilerIdentity,
                address,
                code_bytes,
                1,
                1,
                JitNativeSpecialization::StringConcat,
            );
            Ok((handle, code_bytes))
        },
    )?;
    Ok(NativeStringConcatCompileResult {
        handle: compiled.handle,
        code_bytes: compiled.code_bytes,
    })
}

#[cfg(test)]
fn compile_property_load_native(
    function: FunctionId,
    candidate: &PropertyLoadCandidate,
    helper_address: usize,
    request: &JitCompileRequest,
) -> Result<NativePropertyLoadCompileResult, CraneliftLoweringError> {
    if helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_HELPER",
            "property-load fast path requires a runtime helper address",
        ));
    }

    let helper_symbol = runtime_helper_symbol(PROPERTY_LOAD_HELPER_SYMBOL, helper_address);
    let compiled = compile_managed_native(
        request,
        function,
        "property-load-v1",
        &[(helper_symbol.as_str(), helper_address)],
        |module, name| {
            let pointer_type = module.target_config().pointer_type();

            let mut helper_signature = module.make_signature();
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.params.push(AbiParam::new(pointer_type));
            helper_signature.returns.push(AbiParam::new(types::I32));
            let helper = module
                .declare_function(&helper_symbol, Linkage::Import, &helper_signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare property-load helper import: {error}"),
                    )
                })?;

            let mut signature = module.make_signature();
            signature.params.push(AbiParam::new(pointer_type));
            signature.params.push(AbiParam::new(pointer_type));
            signature.params.push(AbiParam::new(pointer_type));
            signature.returns.push(AbiParam::new(types::I32));

            let func_id = module
                .declare_function(name, Linkage::Local, &signature)
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DECLARE",
                        format!("failed to declare native property-load function: {error}"),
                    )
                })?;
            let mut ctx = module.make_context();
            ctx.func.signature = signature;
            ctx.func.name = UserFuncName::user(0, func_id.as_u32());

            let mut builder_context = FunctionBuilderContext::new();
            {
                let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);
                let entry = builder.create_block();
                builder.append_block_params_for_function_params(entry);
                builder.switch_to_block(entry);
                let params = builder.block_params(entry).to_vec();
                let object_ptr = params[0];
                let metadata_ptr = params[1];
                let out_ptr = params[2];
                let helper_ref = module.declare_func_in_func(helper, builder.func);
                let call = builder
                    .ins()
                    .call(helper_ref, &[object_ptr, metadata_ptr, out_ptr]);
                let status = builder.inst_results(call)[0];
                builder.ins().return_(&[status]);
                builder.seal_block(entry);
                builder.finalize();
            }

            let verifier_flags = settings::Flags::new(settings::builder());
            verify_function(&ctx.func, &verifier_flags).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_VERIFIER",
                    format!("Cranelift verifier rejected native property-load IR: {error}"),
                )
            })?;
            module.define_function(func_id, &mut ctx).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DEFINE",
                    format!("failed to define native property-load function: {error}"),
                )
            })?;
            let code_bytes = ctx
                .compiled_code()
                .map(|compiled| compiled.code_buffer().len() as u64)
                .unwrap_or(0);
            module.clear_context(&mut ctx);
            module.finalize_definitions().map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize native property-load function: {error}"),
                )
            })?;
            let address = module.get_finalized_function(func_id) as usize;
            let handle = JitFunctionHandle::value_metadata_status_out_native(
                u64::from(function.raw()) + 1,
                request.region_id.clone(),
                CraneliftCompilerIdentity,
                address,
                code_bytes,
                1,
                1,
                candidate.metadata.clone(),
            );
            Ok((handle, code_bytes))
        },
    )?;
    Ok(NativePropertyLoadCompileResult {
        handle: compiled.handle,
        code_bytes: compiled.code_bytes,
    })
}

fn create_region_cranelift_blocks(
    builder: &mut FunctionBuilder<'_>,
    region: &RegionGraph,
) -> Result<Vec<ir::Block>, CraneliftLoweringError> {
    let mut blocks = Vec::with_capacity(region.blocks.len());
    for (index, region_block) in region.blocks.iter().enumerate() {
        if region_block.id.index() != index {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_REGION_CONTROL_FLOW",
                format!(
                    "non-dense block id {} at position {} is outside executable Region IR",
                    region_block.id.raw(),
                    index
                ),
            ));
        }
        blocks.push(builder.create_block());
    }
    Ok(blocks)
}

fn cranelift_block(
    blocks: &[ir::Block],
    block_id: BlockId,
) -> Result<ir::Block, CraneliftLoweringError> {
    blocks.get(block_id.index()).copied().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_HELPER_CONTROL_FLOW",
            format!("target block {} is outside the lowered CFG", block_id.raw()),
        )
    })
}

fn local_variable(
    locals: &BTreeMap<LocalId, Variable>,
    local: LocalId,
) -> Result<Variable, CraneliftLoweringError> {
    locals.get(&local).copied().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_LOCAL",
            format!("local {} has not been declared", local.raw()),
        )
    })
}

fn use_local_variable(
    builder: &mut FunctionBuilder<'_>,
    locals: &BTreeMap<LocalId, Variable>,
    local: LocalId,
) -> Result<ir::Value, CraneliftLoweringError> {
    let variable = local_variable(locals, local)?;
    builder.try_use_var(variable).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_LOCAL",
            format!("local {} has no dominating value: {error}", local.raw()),
        )
    })
}

fn lower_region_operand(
    builder: &mut FunctionBuilder<'_>,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, ir::Value>,
    operand: RegionOperand,
) -> Result<ir::Value, CraneliftLoweringError> {
    match operand {
        RegionOperand::Register(reg) => registers.get(&reg).copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
                format!("register {} has not been lowered in this block", reg.raw()),
            )
        }),
        RegionOperand::I64(value) => Ok(builder.ins().iconst(types::I64, value)),
        RegionOperand::Local(local) => use_local_variable(builder, locals, local),
    }
}

fn lower_region_instruction(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    functions: &BTreeMap<FunctionId, FuncId>,
    native_call_helper: Option<FuncId>,
    blocks: &[ir::Block],
    locals: &BTreeMap<LocalId, Variable>,
    registers: &mut BTreeMap<RegId, ir::Value>,
    instruction: &RegionInstruction,
    result_out: ir::Value,
    deopt_out: ir::Value,
    pending_status: Variable,
    pending_value: Variable,
    function: FunctionId,
    local_count: u32,
    pointer_type: ir::Type,
) -> Result<(), CraneliftLoweringError> {
    match &instruction.kind {
        RegionInstructionKind::Nop => {}
        RegionInstructionKind::Move { dst, src } => {
            let cl_value = lower_region_operand(builder, locals, registers, *src)?;
            registers.insert(*dst, cl_value);
        }
        RegionInstructionKind::LoadLocal { dst, local } => {
            let cl_value = use_local_variable(builder, locals, *local)?;
            registers.insert(*dst, cl_value);
        }
        RegionInstructionKind::StoreLocal { local, src } => {
            let cl_value = lower_region_operand(builder, locals, registers, *src)?;
            let variable = local_variable(locals, *local)?;
            builder.def_var(variable, cl_value);
        }
        RegionInstructionKind::Discard { src } => {
            let _ = lower_region_operand(builder, locals, registers, *src)?;
        }
        RegionInstructionKind::Binary { dst, op, lhs, rhs } => {
            let lhs = lower_region_operand(builder, locals, registers, *lhs)?;
            let rhs = lower_region_operand(builder, locals, registers, *rhs)?;
            let cl_value = lower_checked_region_binary(
                builder,
                *op,
                lhs,
                rhs,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
            )?;
            registers.insert(*dst, cl_value);
        }
        RegionInstructionKind::NativeCall(call) => {
            let direct = match call.result {
                RegionCallResult::Register(dst) => {
                    call.direct_compiled_target().map(|target| (dst, target))
                }
                RegionCallResult::ReferenceLocal(_) => None,
            };
            let Some((dst, target)) = direct else {
                lower_native_call_trampoline(
                    module,
                    builder,
                    native_call_helper,
                    locals,
                    registers,
                    call,
                    instruction,
                    result_out,
                    function,
                    local_count,
                    pointer_type,
                )?;
                return Ok(());
            };
            let callee = functions.get(&target).copied().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DIRECT_CALLEE",
                    format!("native direct callee {} was not declared", target.raw()),
                )
            })?;
            let result_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let result_out = builder.ins().stack_addr(pointer_type, result_slot, 0);
            let mut call_args = call
                .operands
                .iter()
                .map(|operand| {
                    let operand = operand.ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_NATIVE_CALL_BINDER_REQUIRED",
                            "direct call argument requires the typed native binder",
                        )
                    })?;
                    lower_region_operand(builder, locals, registers, operand)
                })
                .collect::<Result<Vec<_>, _>>()?;
            call_args.push(result_out);
            call_args.push(deopt_out);
            call_args.push(builder.ins().iconst(types::I32, -1));
            call_args.push(builder.ins().iconst(pointer_type, 0));
            let callee_ref = module.declare_func_in_func(callee, builder.func);
            let call = builder.ins().call(callee_ref, &call_args);
            let status = builder.inst_results(call)[0];
            let ok = builder.create_block();
            let side_exit = builder.create_block();
            let is_ok = builder.ins().icmp_imm(
                IntCC::Equal,
                status,
                i64::from(crate::JitCallStatus::RETURN.0),
            );
            builder.ins().brif(is_ok, ok, &[], side_exit, &[]);
            builder.switch_to_block(side_exit);
            let control_value = builder.ins().stack_load(types::I64, result_slot, 0);
            builder
                .ins()
                .store(MemFlagsData::new(), control_value, result_out, 0);
            builder.ins().return_(&[status]);
            builder.switch_to_block(ok);
            let value = builder.ins().stack_load(types::I64, result_slot, 0);
            registers.insert(dst, value);
        }
        RegionInstructionKind::NativeControl(control) => match control {
            RegionNativeControl::EnterTry { .. } | RegionNativeControl::LeaveTry => {
                // Handler state is published in `JitRegionStateMetadata` and
                // consumed by explicit native unwind. These markers do not
                // call the VM's exception interpreter loop.
            }
            RegionNativeControl::EndFinally {
                after,
                outer_finally,
            } => {
                let status = builder.use_var(pending_status);
                let normal = cranelift_block(blocks, *after)?;
                let pending = builder.create_block();
                let is_continue = builder.ins().icmp_imm(
                    IntCC::Equal,
                    status,
                    i64::from(crate::JitCallStatus::CONTINUE.0),
                );
                builder.ins().brif(is_continue, normal, &[], pending, &[]);
                builder.switch_to_block(pending);
                if let Some(finally) = outer_finally {
                    builder.ins().jump(cranelift_block(blocks, *finally)?, &[]);
                } else {
                    let value = builder.use_var(pending_value);
                    builder
                        .ins()
                        .store(MemFlagsData::new(), value, result_out, 0);
                    builder.ins().return_(&[status]);
                }
                let unreachable = builder.create_block();
                builder.switch_to_block(unreachable);
                builder.seal_block(unreachable);
            }
            RegionNativeControl::Throw { value } => {
                let value = lower_region_operand(builder, locals, registers, *value)?;
                builder
                    .ins()
                    .store(MemFlagsData::new(), value, result_out, 0);
                let status = builder
                    .ins()
                    .iconst(types::I32, i64::from(crate::JitCallStatus::THROW.0));
                builder.ins().return_(&[status]);
                let unreachable = builder.create_block();
                builder.switch_to_block(unreachable);
                builder.seal_block(unreachable);
            }
            RegionNativeControl::MakeException {
                dst,
                class_name,
                message,
            } => {
                // The scalar baseline carries opaque VM handles as i64. The
                // runtime owns materialization; this stable token preserves
                // class/message identity until the typed exception helper
                // publishes the object in the native frame.
                let class = builder
                    .ins()
                    .iconst(types::I64, stable_call_symbol_hash(class_name) as i64);
                let value = if let Some(message) = message {
                    let message = lower_region_operand(builder, locals, registers, *message)?;
                    builder.ins().bxor(class, message)
                } else {
                    class
                };
                registers.insert(*dst, value);
            }
        },
        RegionInstructionKind::Compare { dst, op, lhs, rhs } => {
            let lhs = lower_region_operand(builder, locals, registers, *lhs)?;
            let rhs = lower_region_operand(builder, locals, registers, *rhs)?;
            let cl_value = builder.ins().icmp(region_compare_intcc(*op), lhs, rhs);
            registers.insert(*dst, cl_value);
        }
        RegionInstructionKind::RuntimeFatal { .. } => {
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::RUNTIME_ERROR.0));
            builder.ins().return_(&[status]);
            let unreachable = builder.create_block();
            builder.switch_to_block(unreachable);
            builder.seal_block(unreachable);
        }
        RegionInstructionKind::CompileTimeFatal { diagnostic_id } => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_IR_COMPILE_FATAL",
                diagnostic_id.clone(),
            ));
        }
        RegionInstructionKind::MissingLowering => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_MISSING_INSTRUCTION_LOWERING",
                format!(
                    "instruction {:?} at {}:{}-{} has no native lowering",
                    instruction.source_kind,
                    instruction.span.file.raw(),
                    instruction.span.start,
                    instruction.span.end
                ),
            ));
        }
    }
    Ok(())
}

fn native_call_target_metadata(target: &RegionCallTarget) -> (u32, u32, u64, u64) {
    match target {
        RegionCallTarget::Function { name, function } => (
            crate::JitNativeCallKind::FUNCTION.0,
            function.map_or(u32::MAX, FunctionId::raw),
            stable_call_symbol_hash(name),
            0,
        ),
        RegionCallTarget::Method { method, .. } => (
            crate::JitNativeCallKind::METHOD.0,
            u32::MAX,
            stable_call_symbol_hash(method),
            0,
        ),
        RegionCallTarget::StaticMethod { class_name, method } => (
            crate::JitNativeCallKind::STATIC_METHOD.0,
            u32::MAX,
            stable_call_symbol_hash(method),
            stable_call_symbol_hash(class_name),
        ),
        RegionCallTarget::Closure { .. } => (crate::JitNativeCallKind::CLOSURE.0, u32::MAX, 0, 0),
        RegionCallTarget::Callable { .. } => (crate::JitNativeCallKind::CALLABLE.0, u32::MAX, 0, 0),
        RegionCallTarget::Pipe { .. } => (crate::JitNativeCallKind::PIPE.0, u32::MAX, 0, 0),
        RegionCallTarget::Constructor { class_name, .. } => (
            crate::JitNativeCallKind::CONSTRUCTOR.0,
            u32::MAX,
            0,
            stable_call_symbol_hash(class_name),
        ),
        RegionCallTarget::DynamicConstructor { .. } => (
            crate::JitNativeCallKind::DYNAMIC_CONSTRUCTOR.0,
            u32::MAX,
            0,
            0,
        ),
    }
}

fn stable_call_symbol_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn native_argument_flags(argument: &php_ir::instruction::IrCallArg) -> u32 {
    let mut flags = crate::JitNativeArgFlags::default();
    if argument.name.is_some() {
        flags = flags.union(crate::JitNativeArgFlags::NAMED);
    }
    if argument.unpack {
        flags = flags.union(crate::JitNativeArgFlags::UNPACK);
    }
    if argument.by_ref_local.is_some()
        || argument.by_ref_dim.is_some()
        || argument.by_ref_property.is_some()
        || argument.by_ref_property_dim.is_some()
    {
        flags = flags.union(crate::JitNativeArgFlags::BY_REFERENCE);
    }
    if argument.value_kind == php_ir::instruction::IrCallArgValueKind::IndirectTemporary {
        flags = flags.union(crate::JitNativeArgFlags::INDIRECT_TEMPORARY);
    }
    flags.0
}

#[allow(clippy::too_many_arguments)]
fn lower_native_call_trampoline(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    native_call_helper: Option<FuncId>,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &mut BTreeMap<RegId, ir::Value>,
    call: &RegionNativeCall,
    instruction: &RegionInstruction,
    result_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    pointer_type: ir::Type,
) -> Result<(), CraneliftLoweringError> {
    let helper = native_call_helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_CALL_TRAMPOLINE",
            "native call site has no typed dispatch trampoline",
        )
    })?;
    let argument_size = std::mem::size_of::<crate::JitNativeCallArgument>();
    let arguments_ptr = if call.args.is_empty() {
        builder.ins().iconst(pointer_type, 0)
    } else {
        let bytes = argument_size.checked_mul(call.args.len()).ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_CALL_ARGUMENTS",
                "native call argument table size overflowed",
            )
        })?;
        let bytes = u32::try_from(bytes).map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_CALL_ARGUMENTS",
                "native call argument table exceeds stack-slot limits",
            )
        })?;
        let slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            bytes,
            3,
        ));
        let pointer = builder.ins().stack_addr(pointer_type, slot, 0);
        for (index, argument) in call.args.iter().enumerate() {
            let base = i32::try_from(index.saturating_mul(argument_size)).unwrap_or(i32::MAX);
            let lowered = call
                .operands
                .get(index)
                .copied()
                .flatten()
                .map(|operand| lower_region_operand(builder, locals, registers, operand))
                .transpose()?;
            let tag = if lowered.is_some() { 3 } else { 0 };
            let tag = builder.ins().iconst(types::I32, tag);
            builder.ins().store(MemFlagsData::new(), tag, pointer, base);
            let abi_flags = builder.ins().iconst(types::I32, 0);
            builder
                .ins()
                .store(MemFlagsData::new(), abi_flags, pointer, base + 4);
            let payload = lowered.unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            builder
                .ins()
                .store(MemFlagsData::new(), payload, pointer, base + 8);
            let name_hash = argument.name.as_deref().map_or(0, stable_call_symbol_hash);
            let name_hash = builder.ins().iconst(types::I64, name_hash as i64);
            builder
                .ins()
                .store(MemFlagsData::new(), name_hash, pointer, base + 16);
            let flags = builder
                .ins()
                .iconst(types::I32, i64::from(native_argument_flags(argument)));
            builder
                .ins()
                .store(MemFlagsData::new(), flags, pointer, base + 24);
            let source_slot = argument.by_ref_local.map_or(u32::MAX, LocalId::raw);
            let source_slot = builder.ins().iconst(types::I32, i64::from(source_slot));
            builder
                .ins()
                .store(MemFlagsData::new(), source_slot, pointer, base + 28);
        }
        pointer
    };

    let frame_size =
        u32::try_from(std::mem::size_of::<crate::JitNativeCallFrame>()).map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_CALL_FRAME",
                "native call frame exceeds stack-slot limits",
            )
        })?;
    let frame_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        frame_size,
        3,
    ));
    let frame_ptr = builder.ins().stack_addr(pointer_type, frame_slot, 0);
    let zero = builder.ins().iconst(types::I64, 0);
    for offset in (0..frame_size).step_by(8) {
        builder.ins().store(
            MemFlagsData::new(),
            zero,
            frame_ptr,
            i32::try_from(offset).unwrap_or(i32::MAX),
        );
    }
    let store_i32 = |builder: &mut FunctionBuilder<'_>, offset: usize, value: u32| {
        let value = builder.ins().iconst(types::I32, i64::from(value));
        builder.ins().store(
            MemFlagsData::new(),
            value,
            frame_ptr,
            i32::try_from(offset).unwrap_or(i32::MAX),
        );
    };
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, abi_version),
        crate::JIT_RUNTIME_ABI_VERSION,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, struct_size),
        frame_size,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, function_id),
        function.raw(),
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, region_id),
        function.raw(),
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, continuation_id),
        instruction.continuation_id,
    );
    let result_slot = match call.result {
        RegionCallResult::Register(register) => register.raw(),
        RegionCallResult::ReferenceLocal(local) => local.raw(),
    };
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, result_slot),
        result_slot,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, local_count),
        local_count,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, argument_count),
        u32::try_from(call.args.len()).unwrap_or(u32::MAX),
    );
    let frame_flags =
        u32::from(call.caller_strict_types) | if call.returns_by_reference { 1 << 1 } else { 0 };
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, flags),
        frame_flags,
    );
    builder.ins().store(
        MemFlagsData::new(),
        arguments_ptr,
        frame_ptr,
        std::mem::offset_of!(crate::JitNativeCallFrame, arguments) as i32,
    );
    let (kind, target_function, symbol_hash, class_hash) =
        native_call_target_metadata(&call.target);
    let target_offset = std::mem::offset_of!(crate::JitNativeCallFrame, target);
    store_i32(
        builder,
        target_offset + std::mem::offset_of!(crate::JitNativeCallTarget, kind),
        kind,
    );
    store_i32(
        builder,
        target_offset + std::mem::offset_of!(crate::JitNativeCallTarget, function_id),
        target_function,
    );
    for (offset, value) in [
        (
            target_offset + std::mem::offset_of!(crate::JitNativeCallTarget, symbol_hash),
            symbol_hash,
        ),
        (
            target_offset + std::mem::offset_of!(crate::JitNativeCallTarget, class_hash),
            class_hash,
        ),
    ] {
        let value = builder.ins().iconst(types::I64, value as i64);
        builder.ins().store(
            MemFlagsData::new(),
            value,
            frame_ptr,
            i32::try_from(offset).unwrap_or(i32::MAX),
        );
    }

    let out_size = std::mem::size_of::<crate::JitCallResult>() as u32;
    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        out_size,
        3,
    ));
    let out_ptr = builder.ins().stack_addr(pointer_type, out_slot, 0);
    let helper_ref = module.declare_func_in_func(helper, builder.func);
    let vm_context = builder.ins().iconst(types::I64, 0);
    let helper_call = builder
        .ins()
        .call(helper_ref, &[vm_context, frame_ptr, out_ptr]);
    let status = builder.inst_results(helper_call)[0];
    let ok = builder.create_block();
    let side_exit = builder.create_block();
    let is_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        status,
        i64::from(crate::JitCallStatus::RETURN.0),
    );
    builder.ins().brif(is_ok, ok, &[], side_exit, &[]);
    builder.switch_to_block(side_exit);
    let control_value = builder.ins().stack_load(types::I64, out_slot, 16);
    builder
        .ins()
        .store(MemFlagsData::new(), control_value, result_out, 0);
    builder.ins().return_(&[status]);
    builder.switch_to_block(ok);
    let value = builder.ins().stack_load(types::I64, out_slot, 16);
    match call.result {
        RegionCallResult::Register(register) => {
            registers.insert(register, value);
        }
        RegionCallResult::ReferenceLocal(local) => {
            builder.def_var(local_variable(locals, local)?, value);
        }
    }
    Ok(())
}

fn lower_checked_region_binary(
    builder: &mut FunctionBuilder<'_>,
    op: RegionBinaryOp,
    lhs: ir::Value,
    rhs: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    instruction: &RegionInstruction,
    locals: &BTreeMap<LocalId, Variable>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (result, overflow) = match op {
        RegionBinaryOp::Add => builder.ins().sadd_overflow(lhs, rhs),
        RegionBinaryOp::Sub => builder.ins().ssub_overflow(lhs, rhs),
        RegionBinaryOp::Mul => builder.ins().smul_overflow(lhs, rhs),
    };
    let overflow_block = builder.create_block();
    let ok_block = builder.create_block();
    builder
        .ins()
        .brif(overflow, overflow_block, &[], ok_block, &[]);
    builder.switch_to_block(overflow_block);
    let function_id = builder.ins().iconst(types::I32, i64::from(function.raw()));
    builder
        .ins()
        .store(MemFlagsData::new(), function_id, deopt_out, 0);
    let continuation = builder
        .ins()
        .iconst(types::I32, i64::from(instruction.continuation_id));
    builder
        .ins()
        .store(MemFlagsData::new(), continuation, deopt_out, 4);
    let slot_count = builder.ins().iconst(types::I32, i64::from(local_count));
    builder
        .ins()
        .store(MemFlagsData::new(), slot_count, deopt_out, 8);
    let reserved = builder.ins().iconst(types::I32, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), reserved, deopt_out, 12);
    let initialized_mask = instruction.live_locals.iter().fold(0_u64, |mask, local| {
        mask | 1_u64.checked_shl(local.raw()).unwrap_or(0)
    });
    let initialized_mask = builder.ins().iconst(types::I64, initialized_mask as i64);
    builder
        .ins()
        .store(MemFlagsData::new(), initialized_mask, deopt_out, 16);
    for local in &instruction.live_locals {
        let value = use_local_variable(builder, locals, *local)?;
        let offset = 24_i32.saturating_add((local.raw() as i32).saturating_mul(8));
        builder
            .ins()
            .store(MemFlagsData::new(), value, deopt_out, offset);
    }
    let status = builder.ins().iconst(
        types::I32,
        i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
    );
    builder.ins().return_(&[status]);
    builder.switch_to_block(ok_block);
    Ok(result)
}

fn region_compare_intcc(op: RegionCompareOpCode) -> IntCC {
    match op {
        RegionCompareOpCode::Equal => IntCC::Equal,
        RegionCompareOpCode::NotEqual => IntCC::NotEqual,
        RegionCompareOpCode::Less => IntCC::SignedLessThan,
        RegionCompareOpCode::LessEqual => IntCC::SignedLessThanOrEqual,
        RegionCompareOpCode::Greater => IntCC::SignedGreaterThan,
        RegionCompareOpCode::GreaterEqual => IntCC::SignedGreaterThanOrEqual,
    }
}

fn lower_region_condition(
    builder: &mut FunctionBuilder<'_>,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, ir::Value>,
    condition: RegionOperand,
) -> Result<ir::Value, CraneliftLoweringError> {
    let value = lower_region_operand(builder, locals, registers, condition)?;
    if builder.func.dfg.value_type(value) == types::I64 {
        Ok(builder.ins().icmp_imm(IntCC::NotEqual, value, 0))
    } else {
        Ok(value)
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_region_terminator(
    builder: &mut FunctionBuilder<'_>,
    blocks: &[ir::Block],
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, ir::Value>,
    result_out: ir::Value,
    pending_status: Variable,
    pending_value: Variable,
    terminator: &RegionTerminator,
) -> Result<(), CraneliftLoweringError> {
    match terminator {
        RegionTerminator::Jump { target } => {
            builder.ins().jump(cranelift_block(blocks, *target)?, &[]);
        }
        RegionTerminator::JumpIfFalse {
            condition,
            target,
            fallthrough,
        } => {
            let condition = lower_region_condition(builder, locals, registers, *condition)?;
            let false_block = cranelift_block(blocks, *target)?;
            let true_block = cranelift_block(blocks, *fallthrough)?;
            builder
                .ins()
                .brif(condition, true_block, &[], false_block, &[]);
        }
        RegionTerminator::JumpIfTrue {
            condition,
            target,
            fallthrough,
        } => {
            let condition = lower_region_condition(builder, locals, registers, *condition)?;
            let true_block = cranelift_block(blocks, *target)?;
            let false_block = cranelift_block(blocks, *fallthrough)?;
            builder
                .ins()
                .brif(condition, true_block, &[], false_block, &[]);
        }
        RegionTerminator::JumpIf {
            condition,
            if_true,
            if_false,
        } => {
            let condition = lower_region_condition(builder, locals, registers, *condition)?;
            builder.ins().brif(
                condition,
                cranelift_block(blocks, *if_true)?,
                &[],
                cranelift_block(blocks, *if_false)?,
                &[],
            );
        }
        RegionTerminator::Return { value, finally } => {
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::RETURN.0));
            lower_region_frame_exit(
                builder,
                blocks,
                result_out,
                pending_status,
                pending_value,
                value,
                status,
                *finally,
            )?;
        }
        RegionTerminator::ReturnReference { local, finally } => {
            let value = use_local_variable(builder, locals, *local)?;
            let status = builder.ins().iconst(
                types::I32,
                i64::from(crate::JitCallStatus::RETURN_REFERENCE.0),
            );
            lower_region_frame_exit(
                builder,
                blocks,
                result_out,
                pending_status,
                pending_value,
                value,
                status,
                *finally,
            )?;
        }
        RegionTerminator::Exit { value, finally } => {
            let value = value
                .map(|value| lower_region_operand(builder, locals, registers, value))
                .transpose()?
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::EXIT.0));
            lower_region_frame_exit(
                builder,
                blocks,
                result_out,
                pending_status,
                pending_value,
                value,
                status,
                *finally,
            )?;
        }
        RegionTerminator::MissingLowering => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_MISSING_TERMINATOR_LOWERING",
                "terminator has no native lowering",
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_region_frame_exit(
    builder: &mut FunctionBuilder<'_>,
    blocks: &[ir::Block],
    result_out: ir::Value,
    pending_status: Variable,
    pending_value: Variable,
    value: ir::Value,
    status: ir::Value,
    finally: Option<BlockId>,
) -> Result<(), CraneliftLoweringError> {
    if let Some(finally) = finally {
        builder.def_var(pending_status, status);
        builder.def_var(pending_value, value);
        builder.ins().jump(cranelift_block(blocks, finally)?, &[]);
    } else {
        builder
            .ins()
            .store(MemFlagsData::new(), value, result_out, 0);
        builder.ins().return_(&[status]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CraneliftNativeCompiler, build_trivial_add_clif_smoke, lower_function_to_cranelift,
    };
    use crate::{
        JIT_RUNTIME_ABI_HASH, JitCompileRequest, JitCompileStatus, NativeCompileRequest,
        NativeCompilerApi,
    };
    use php_ir::{
        BinaryOp, FunctionFlags, FunctionId, InstructionKind, IrBuilder, IrConstant, IrParam,
        IrReturnType, IrSpan, LocalId, Operand, UnitId,
    };

    #[test]
    fn builds_and_verifies_standalone_trivial_add_clif_smoke() {
        let result = build_trivial_add_clif_smoke().expect("standalone CLIF smoke should verify");

        assert_eq!(result.function_name, "trivial_add_i64");
        assert!(result.clif.contains("function u0:0(i64, i64) -> i64"));
        assert!(result.clif.contains("iadd"));
        assert!(result.clif.contains("return"));
        assert!(result.stats.verified);
        assert_eq!(result.stats.blocks_lowered, 1);
        assert_eq!(result.stats.instructions_lowered, 2);
    }

    #[test]
    fn lowers_integer_arithmetic_to_cranelift_ir_without_execution() {
        let (unit, function) = arithmetic_fixture();
        let result =
            lower_function_to_cranelift(&unit, function).expect("arithmetic subset lowers");

        assert_eq!(result.function, function);
        assert_eq!(result.function_name, "jit_arithmetic");
        assert!(result.clif.contains("iconst.i64 10"));
        assert!(result.clif.contains("iconst.i64 3"));
        assert!(result.clif.contains("iadd"));
        assert!(result.clif.contains("isub"));
        assert!(result.clif.contains("imul"));
        assert!(result.clif.contains("return"));
        assert!(result.stats.verified);
        assert_eq!(result.stats.blocks_lowered, 1);
        assert_eq!(result.stats.instructions_lowered, 6);
        assert!(result.machine_code_handle.is_none());
    }

    #[test]
    fn rejects_unsupported_ir_with_typed_error() {
        let (unit, function) = unsupported_binary_fixture();
        let error = lower_function_to_cranelift(&unit, function)
            .expect_err("division must not be silently lowered");

        assert_eq!(error.code, "JIT_CRANELIFT_REJECT_ELIGIBILITY");
        assert!(
            error
                .detail
                .contains("JIT_ELIGIBILITY_REJECT_NON_PRIMITIVE_BINARY_OP")
        );
    }

    #[test]
    fn rejects_non_int_constant_after_eligibility() {
        let (unit, function) = bool_return_fixture();
        let error = lower_function_to_cranelift(&unit, function)
            .expect_err("bool constants are not part of 07.52 lowering");

        assert_eq!(error.code, "JIT_CRANELIFT_REJECT_NON_INT_CONSTANT");
    }

    #[test]
    fn production_pipeline_reports_concrete_missing_return_abi() {
        let (unit, function) = arithmetic_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.no_exec.verified");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(
            outcome.status,
            JitCompileStatus::Rejected {
                reason: "JIT_CRANELIFT_MISSING_RETURN_ABI_LOWERING".to_owned()
            }
        );
        assert!(outcome.handle.is_none());
        assert!(outcome.diagnostics[0].contains("return metadata"));
    }

    #[test]
    fn runtime_error_lowers_to_native_fatal_status() {
        let mut builder = IrBuilder::new(UnitId::new(704));
        let file = builder.add_file("runtime-fatal.php");
        let span = IrSpan::new(file, 3, 9);
        let function = builder.start_function("runtime_fatal", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        builder.emit(
            function,
            block,
            InstructionKind::RuntimeError {
                diagnostic_id: "E_TEST_RUNTIME_FATAL".to_owned(),
                message: "explicit fatal".to_owned(),
            },
            span,
        );
        let constant = builder.intern_constant(IrConstant::Int(0));
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadConst {
                dst: result,
                constant,
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        let unit = builder.finish();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.runtime-fatal");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        assert_eq!(
            outcome
                .handle
                .expect("native fatal handle")
                .invoke_i64(&[], JIT_RUNTIME_ABI_HASH),
            Err(crate::JitInvokeError::NativeStatus(
                crate::JitCallStatus::RUNTIME_ERROR.0 as i32,
            ))
        );
    }

    #[test]
    fn throw_uses_explicit_native_status_and_publishes_unwind_metadata() {
        let mut builder = IrBuilder::new(UnitId::new(705));
        let file = builder.add_file("native-throw.php");
        let span = IrSpan::new(file, 5, 25);
        let function = builder.start_function("native_throw", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let entry = builder.append_block(function);
        let finally = builder.append_block(function);
        let after = builder.append_block(function);
        builder.emit(
            function,
            entry,
            InstructionKind::EnterTry {
                catch: None,
                catch_types: Vec::new(),
                finally: Some(finally),
                after,
                exception_local: None,
            },
            span,
        );
        let message = builder.intern_constant(IrConstant::Int(23));
        let exception = builder.alloc_register(function);
        builder.emit(
            function,
            entry,
            InstructionKind::MakeException {
                dst: exception,
                class_name: "runtimeexception".to_owned(),
                message: Operand::Constant(message),
            },
            span,
        );
        builder.emit(
            function,
            entry,
            InstructionKind::Throw {
                value: Operand::Register(exception),
            },
            span,
        );
        builder.terminate_jump(function, entry, after, span);
        builder.emit(
            function,
            finally,
            InstructionKind::EndFinally { after },
            span,
        );
        builder.terminate_jump(function, finally, after, span);
        let zero = builder.intern_constant(IrConstant::Int(0));
        builder.terminate_return(function, after, Some(Operand::Constant(zero)), span);
        let unit = builder.finish();
        let mut backend = CraneliftNativeCompiler;
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &JitCompileRequest::new("cl.native-throw"),
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });
        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        let handle = outcome.handle.expect("native throw handle");
        let metadata = handle
            .region_state_metadata()
            .expect("native frame metadata");
        assert_eq!(metadata.exception_handlers.len(), 1);
        assert!(!metadata.safepoints.is_empty());
        let range = metadata
            .native_pc_ranges
            .iter()
            .find(|range| range.end > range.start)
            .expect("non-empty native PC range");
        assert!(
            metadata.resolve_native_pc(range.start).is_some(),
            "{metadata:#?}"
        );
        assert!(matches!(
            metadata.select_native_unwind(
                function,
                1,
                crate::JitCallStatus::THROW,
                |_| false,
            ),
            crate::JitNativeUnwindTarget::Finally { block, .. } if block == finally
        ));
        let crate::JitI64InvokeOutcome::SideExit { status, value, .. } = handle
            .invoke_i64_with_deopt(&[], JIT_RUNTIME_ABI_HASH)
            .expect("native throw executes")
        else {
            panic!("throw unexpectedly returned");
        };
        assert_eq!(status, crate::JitCallStatus::THROW.0 as i32);
        assert_ne!(value, 0);
    }

    #[test]
    fn native_unwind_resumes_compiled_catch_without_interpreter_frame() {
        let mut builder = IrBuilder::new(UnitId::new(708));
        let file = builder.add_file("native-catch.php");
        let span = IrSpan::new(file, 0, 30);
        let function = builder.start_function("native_catch", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let entry = builder.append_block(function);
        let catch = builder.append_block(function);
        let after = builder.append_block(function);
        let exception_local = builder.intern_local(function, "exception");
        builder.emit(
            function,
            entry,
            InstructionKind::EnterTry {
                catch: Some(catch),
                catch_types: vec!["runtimeexception".to_owned()],
                finally: None,
                after,
                exception_local: Some(exception_local),
            },
            span,
        );
        let thrown = builder.intern_constant(IrConstant::Int(33));
        builder.emit(
            function,
            entry,
            InstructionKind::Throw {
                value: Operand::Constant(thrown),
            },
            span,
        );
        builder.terminate_jump(function, entry, after, span);
        let caught = builder.intern_constant(IrConstant::Int(77));
        builder.terminate_return(function, catch, Some(Operand::Constant(caught)), span);
        let fallback = builder.intern_constant(IrConstant::Int(0));
        builder.terminate_return(function, after, Some(Operand::Constant(fallback)), span);
        let unit = builder.finish();
        let mut backend = CraneliftNativeCompiler;
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &JitCompileRequest::new("cl.native-catch"),
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });
        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        let native = outcome
            .handle
            .expect("native catch handle")
            .invoke_i64_with_native_unwind(&[], JIT_RUNTIME_ABI_HASH, |types, value| {
                value == 33 && types == ["runtimeexception"]
            })
            .expect("explicit native unwind");
        assert_eq!(native, crate::JitI64InvokeOutcome::Returned(77));
    }

    #[test]
    fn return_runs_compiled_finally_before_native_frame_return() {
        let mut builder = IrBuilder::new(UnitId::new(706));
        let file = builder.add_file("return-finally.php");
        let span = IrSpan::new(file, 0, 40);
        let function = builder.start_function("return_finally", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let entry = builder.append_block(function);
        let finally = builder.append_block(function);
        let after = builder.append_block(function);
        builder.emit(
            function,
            entry,
            InstructionKind::EnterTry {
                catch: None,
                catch_types: Vec::new(),
                finally: Some(finally),
                after,
                exception_local: None,
            },
            span,
        );
        let returned = builder.intern_constant(IrConstant::Int(41));
        builder.terminate_return(function, entry, Some(Operand::Constant(returned)), span);
        builder.emit(
            function,
            finally,
            InstructionKind::EndFinally { after },
            span,
        );
        builder.terminate_jump(function, finally, after, span);
        let fallback = builder.intern_constant(IrConstant::Int(0));
        builder.terminate_return(function, after, Some(Operand::Constant(fallback)), span);
        let unit = builder.finish();
        let mut backend = CraneliftNativeCompiler;
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &JitCompileRequest::new("cl.return-finally"),
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });
        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        assert_eq!(
            outcome
                .handle
                .expect("compiled finally handle")
                .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
                .expect("return through finally"),
            41
        );
    }

    #[test]
    fn exit_runs_compiled_finally_before_native_exit_status() {
        let mut builder = IrBuilder::new(UnitId::new(707));
        let file = builder.add_file("exit-finally.php");
        let span = IrSpan::new(file, 0, 40);
        let function = builder.start_function("exit_finally", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let entry = builder.append_block(function);
        let finally = builder.append_block(function);
        let after = builder.append_block(function);
        builder.emit(
            function,
            entry,
            InstructionKind::EnterTry {
                catch: None,
                catch_types: Vec::new(),
                finally: Some(finally),
                after,
                exception_local: None,
            },
            span,
        );
        let exit_code = builder.intern_constant(IrConstant::Int(5));
        builder.terminate_exit(function, entry, Some(Operand::Constant(exit_code)), span);
        builder.emit(
            function,
            finally,
            InstructionKind::EndFinally { after },
            span,
        );
        builder.terminate_jump(function, finally, after, span);
        let zero = builder.intern_constant(IrConstant::Int(0));
        builder.terminate_return(function, after, Some(Operand::Constant(zero)), span);
        let unit = builder.finish();
        let mut backend = CraneliftNativeCompiler;
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &JitCompileRequest::new("cl.exit-finally"),
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });
        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        let crate::JitI64InvokeOutcome::SideExit { status, value, .. } = outcome
            .handle
            .expect("compiled exit handle")
            .invoke_i64_with_deopt(&[], JIT_RUNTIME_ABI_HASH)
            .expect("exit through finally")
        else {
            panic!("exit unexpectedly returned");
        };
        assert_eq!(status, crate::JitCallStatus::EXIT.0 as i32);
        assert_eq!(value, 5);
    }

    #[test]
    fn cranelift_backend_compiles_and_invokes_constant_return_native_handle() {
        let (unit, function) = constant_return_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.const.42");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled);
        assert!(outcome.code_bytes > 0, "{outcome:?}");
        assert!(outcome.compile_time_nanos > 0, "{outcome:?}");
        let handle = outcome.handle.expect("constant return should compile");
        assert_eq!(handle.code_bytes(), outcome.code_bytes);
        assert_eq!(
            handle
                .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
                .expect("native constant return should execute"),
            42
        );
    }

    #[test]
    fn cranelift_native_handle_copy_survives_original_handle_drop() {
        let (unit, function) = constant_return_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.const.lifecycle");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled);
        let original = outcome.handle.expect("constant return should compile");
        let copied = original.clone();
        drop(original);

        assert_eq!(
            copied
                .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
                .expect("leaked Cranelift module keeps copied handle callable"),
            42
        );
    }

    #[test]
    fn cranelift_backend_compiles_and_invokes_inline_arithmetic_native_handle() {
        let (unit, function) = helper_arithmetic_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.inline.add_mul");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled);
        assert!(outcome.code_bytes > 0, "{outcome:?}");
        assert!(outcome.compile_time_nanos > 0, "{outcome:?}");
        assert!(
            outcome.diagnostics[0].contains("fast_path_hits=2"),
            "{outcome:?}"
        );
        let handle = outcome.handle.expect("inline arithmetic should compile");
        assert_eq!(handle.code_bytes(), outcome.code_bytes);
        assert_eq!(handle.helper_calls_per_invocation(), 0);
        assert_eq!(handle.fast_path_hits_per_invocation(), 2);
        assert_eq!(
            handle
                .invoke_i64(&[4], JIT_RUNTIME_ABI_HASH)
                .expect("native inline arithmetic should execute"),
            18
        );
    }

    #[test]
    fn cranelift_backend_executes_region_ir_without_leaf_recognizer() {
        let (unit, function) = scalar_identity_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.region.identity");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        assert!(outcome.diagnostics[0].contains("baseline Region IR"));
        assert!(outcome.diagnostics[0].contains("fast_path_hits=0"));
        let handle = outcome
            .handle
            .expect("generic scalar region should compile");
        assert_eq!(
            handle
                .invoke_i64(&[73], JIT_RUNTIME_ABI_HASH)
                .expect("generic scalar region should execute"),
            73
        );
    }

    #[test]
    fn cranelift_backend_executes_multiblock_region_ir() {
        let (unit, function) = scalar_branch_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.region.branch");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        assert!(outcome.diagnostics[0].contains("baseline Region IR"));
        assert!(outcome.diagnostics[0].contains("control_flow=true"));
        let handle = outcome.handle.expect("multi-block region should compile");
        assert_eq!(
            handle
                .invoke_i64(&[1], JIT_RUNTIME_ABI_HASH)
                .expect("true branch executes"),
            11
        );
        assert_eq!(
            handle
                .invoke_i64(&[0], JIT_RUNTIME_ABI_HASH)
                .expect("false branch executes"),
            22
        );
    }

    #[test]
    fn cranelift_region_calls_same_unit_compiled_callee_directly() {
        let (unit, function, callee) = scalar_direct_call_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.region.direct-call");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        assert!(outcome.diagnostics[0].contains("fast_path_hits=2"));
        let handle = outcome.handle.expect("direct-call region should compile");
        assert_eq!(handle.helper_calls_per_invocation(), 0);
        assert_eq!(handle.compiled_to_compiled_calls_per_invocation(), 1);
        assert!(
            handle
                .region_state_metadata()
                .expect("region metadata")
                .continuations
                .iter()
                .any(|continuation| continuation.function == callee)
        );
        assert_eq!(
            handle
                .invoke_i64(&[41], JIT_RUNTIME_ABI_HASH)
                .expect("native caller and callee should execute"),
            42
        );
        let crate::JitI64InvokeOutcome::SideExit { status, state, .. } = handle
            .invoke_i64_with_deopt(&[i64::MAX], JIT_RUNTIME_ABI_HASH)
            .expect("callee overflow should preserve precise state")
        else {
            panic!("callee overflow unexpectedly returned");
        };
        assert_eq!(status, crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32);
        assert_eq!(state.function_id, callee.raw());
        assert_eq!(state.slots[0], i64::MAX);
    }

    #[test]
    fn cranelift_dynamic_call_uses_typed_native_trampoline() {
        extern "C" fn trampoline(
            _vm_context: u64,
            frame: *mut crate::JitNativeCallFrame,
            out: *mut crate::JitCallResult,
        ) -> i32 {
            assert!(!frame.is_null());
            assert!(!out.is_null());
            // SAFETY: The generated call owns both ABI records for this
            // synchronous test invocation.
            let frame = unsafe { &*frame };
            assert_eq!(frame.abi_version, crate::JIT_RUNTIME_ABI_VERSION);
            assert_eq!(frame.target.kind, crate::JitNativeCallKind::FUNCTION);
            // SAFETY: `out` is a checked, caller-owned result record.
            unsafe {
                out.write(crate::JitCallResult {
                    status: crate::JitCallStatus::COMPILE_REQUIRED,
                    detail: frame.continuation_id,
                    value: crate::JitAbiSlot::default(),
                });
            }
            crate::JitCallStatus::COMPILE_REQUIRED.0 as i32
        }

        let (unit, function) = scalar_dynamic_call_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.region.dynamic-call");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses {
                native_call_dispatch: trampoline as *const () as usize,
                ..crate::JitRuntimeHelperAddresses::default()
            },
        });
        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        let handle = outcome.handle.expect("dynamic call should compile");
        let crate::JitI64InvokeOutcome::SideExit { status, .. } = handle
            .invoke_i64_with_deopt(&[], JIT_RUNTIME_ABI_HASH)
            .expect("dynamic call trampoline should execute")
        else {
            panic!("dynamic call unexpectedly returned");
        };
        assert_eq!(status, crate::JitCallStatus::COMPILE_REQUIRED.0 as i32);
    }

    #[test]
    fn cranelift_helper_arithmetic_overflow_returns_native_status() {
        let (unit, function) = helper_overflow_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.inline.overflow");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled);
        let handle = outcome.handle.expect("overflow helper should compile");
        let error = handle
            .invoke_i64(&[i64::MAX], JIT_RUNTIME_ABI_HASH)
            .expect_err("checked inline arithmetic should request fallback");
        assert_eq!(
            error,
            crate::JitInvokeError::NativeStatus(crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32,)
        );
        assert_eq!(error.side_exit().reason, crate::SideExitReason::Overflow);
    }

    #[test]
    fn cranelift_overflow_materializes_precise_region_continuation() {
        let (unit, function) = helper_overflow_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.region.deopt-state");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled);
        let handle = outcome.handle.expect("overflow region should compile");
        let metadata = handle
            .region_state_metadata()
            .expect("executable regions publish state metadata");
        assert!(!metadata.continuations.is_empty());
        assert!(!metadata.native_pc_ranges.is_empty());
        let crate::JitI64InvokeOutcome::SideExit { status, state, .. } = handle
            .invoke_i64_with_deopt(&[i64::MAX], JIT_RUNTIME_ABI_HASH)
            .expect("native invocation")
        else {
            panic!("overflow must side-exit");
        };
        assert_eq!(status, crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32);
        assert_eq!(state.function_id, function.raw());
        assert_eq!(state.slot_count, 1);
        assert_eq!(state.initialized_mask & 1, 1);
        assert_eq!(state.slots[0], i64::MAX);
        let continuation = metadata
            .continuations
            .iter()
            .find(|continuation| continuation.id == state.continuation_id)
            .expect("side exit continuation exists");
        assert_eq!(continuation.function, function);
        assert_eq!(continuation.live_locals, vec![LocalId::new(0)]);
        assert!(metadata.native_pc_ranges.iter().any(|range| {
            range.function == function
                && range.continuation_id == continuation.id
                && range.end > range.start
        }));
    }

    #[test]
    fn cranelift_loop_enters_through_native_osr_state() {
        let (unit, function) = scalar_loop_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.region.osr");
        let outcome = backend.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        let handle = outcome.handle.expect("loop region should compile");
        let metadata = handle.region_state_metadata().expect("state metadata");
        let osr = metadata.osr_entries.first().expect("loop OSR entry");
        assert_eq!(osr.live_locals, vec![LocalId::new(0), LocalId::new(1)]);
        assert_eq!(
            handle
                .invoke_i64(&[3], JIT_RUNTIME_ABI_HASH)
                .expect("normal loop entry"),
            3
        );
        let mut state = crate::JitDeoptState {
            slot_count: 2,
            initialized_mask: 0b11,
            ..crate::JitDeoptState::default()
        };
        state.slots[0] = 5;
        state.slots[1] = 2;
        assert_eq!(
            handle
                .invoke_i64_osr(&[5], osr.id, &state, JIT_RUNTIME_ABI_HASH)
                .expect("native OSR invocation"),
            crate::JitI64InvokeOutcome::Returned(5)
        );
    }

    #[test]
    fn differential_candidate_compiles_packed_array_fetch_helper_native_handle() {
        let (unit, function) = packed_array_fetch_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.packed.fetch");
        let outcome = backend.compile_candidate_differential(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses {
                helper_table: 0,
                packed_array_len: 0,
                packed_array_fetch_int_slow: test_packed_array_fetch_helper as *const () as usize,
                known_strlen: 0,
                known_count: 0,
                string_concat: 0,
                property_load: 0,
                record_array_lookup: 0,
                native_call_dispatch: 0,
            },
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled);
        assert!(outcome.code_bytes > 0, "{outcome:?}");
        assert!(
            outcome.diagnostics[0].contains("helper=php_jit_array_fetch_int_slow"),
            "{outcome:?}"
        );
        let handle = outcome.handle.expect("packed fetch should compile");
        assert!(handle.expects_value_i64());
        assert_eq!(handle.helper_calls_per_invocation(), 1);
        assert_eq!(handle.fast_path_hits_per_invocation(), 1);
        assert_eq!(
            handle
                .invoke_value_i64(0xfeed, 1, JIT_RUNTIME_ABI_HASH)
                .expect("helper-backed packed fetch should execute"),
            77
        );
        assert_eq!(
            handle
                .invoke_value_i64(0xfeed, -1, JIT_RUNTIME_ABI_HASH)
                .expect_err("negative index should side-exit before helper"),
            crate::JitInvokeError::NativeStatus(2)
        );
    }

    #[test]
    fn differential_candidate_compiles_packed_foreach_int_sum_native_loop() {
        let (unit, function) = packed_foreach_int_sum_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.packed.foreach.sum");
        let outcome = backend.compile_candidate_differential(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses {
                helper_table: 0,
                packed_array_len: test_packed_array_len_helper as *const () as usize,
                packed_array_fetch_int_slow: test_packed_array_fetch_sequence_helper as *const ()
                    as usize,
                known_strlen: 0,
                known_count: 0,
                string_concat: 0,
                property_load: 0,
                record_array_lookup: 0,
                native_call_dispatch: 0,
            },
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        assert!(outcome.code_bytes > 0, "{outcome:?}");
        assert!(
            outcome.diagnostics[0].contains("packed-foreach int-sum"),
            "{outcome:?}"
        );
        let handle = outcome.handle.expect("packed foreach sum should compile");
        assert!(handle.expects_value());
        assert_eq!(handle.fast_path_hits_per_invocation(), 1);
        assert_eq!(
            handle
                .invoke_value(0xfeed, JIT_RUNTIME_ABI_HASH)
                .expect("helper-backed packed foreach sum should execute"),
            60
        );
    }

    #[test]
    fn differential_candidate_compiles_known_strlen_helper_native_handle() {
        let (unit, function) = known_strlen_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.known.strlen");
        let outcome = backend.compile_candidate_differential(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses {
                helper_table: 0,
                packed_array_len: 0,
                packed_array_fetch_int_slow: 0,
                known_strlen: test_known_strlen_helper as *const () as usize,
                known_count: 0,
                string_concat: 0,
                property_load: 0,
                record_array_lookup: 0,
                native_call_dispatch: 0,
            },
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        assert!(outcome.code_bytes > 0, "{outcome:?}");
        assert!(
            outcome.diagnostics[0].contains("known-call strlen"),
            "{outcome:?}"
        );
        let handle = outcome.handle.expect("known strlen should compile");
        assert!(handle.expects_value());
        assert_eq!(
            handle.specialization(),
            crate::JitNativeSpecialization::KnownCallStrlen
        );
        assert_eq!(handle.helper_calls_per_invocation(), 1);
        assert_eq!(handle.fast_path_hits_per_invocation(), 1);
        assert_eq!(
            handle
                .invoke_value(0xfeed, JIT_RUNTIME_ABI_HASH)
                .expect("helper-backed known strlen should execute"),
            5
        );
    }

    #[test]
    fn differential_candidate_compiles_string_concat_helper_native_handle() {
        let (unit, function) = string_concat_fixture();
        let mut backend = CraneliftNativeCompiler;
        let request = JitCompileRequest::new("cl.string.concat");
        let outcome = backend.compile_candidate_differential(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses {
                helper_table: 0,
                packed_array_len: 0,
                packed_array_fetch_int_slow: 0,
                known_strlen: 0,
                known_count: 0,
                string_concat: test_string_concat_helper as *const () as usize,
                property_load: 0,
                record_array_lookup: 0,
                native_call_dispatch: 0,
            },
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
        assert!(outcome.code_bytes > 0, "{outcome:?}");
        assert!(
            outcome.diagnostics[0].contains("string-concat"),
            "{outcome:?}"
        );
        let handle = outcome.handle.expect("string concat should compile");
        assert!(handle.expects_value_value());
        assert_eq!(
            handle.specialization(),
            crate::JitNativeSpecialization::StringConcat
        );
        assert_eq!(handle.helper_calls_per_invocation(), 1);
        assert_eq!(handle.fast_path_hits_per_invocation(), 1);
        assert_eq!(
            handle
                .invoke_value_value(0xfeed, 0xbeef, JIT_RUNTIME_ABI_HASH)
                .expect("helper-backed string concat should execute"),
            0xabc
        );
    }

    extern "C" fn test_packed_array_fetch_helper(
        _value_ptr: usize,
        index: i64,
        out: *mut i64,
    ) -> i32 {
        if index != 1 || out.is_null() {
            return 3;
        }
        // SAFETY: The test invokes this helper through the JIT trampoline with
        // a stack-allocated out slot.
        unsafe {
            *out = 77;
        }
        crate::JIT_HELPER_STATUS_OK
    }

    extern "C" fn test_packed_array_len_helper(_value_ptr: usize, out: *mut i64) -> i32 {
        if out.is_null() {
            return 3;
        }
        // SAFETY: The test invokes this helper through the JIT trampoline with
        // a stack-allocated out slot.
        unsafe {
            *out = 3;
        }
        crate::JIT_HELPER_STATUS_OK
    }

    extern "C" fn test_packed_array_fetch_sequence_helper(
        _value_ptr: usize,
        index: i64,
        out: *mut i64,
    ) -> i32 {
        if out.is_null() {
            return 3;
        }
        let value = match index {
            0 => 10,
            1 => 20,
            2 => 30,
            _ => return 2,
        };
        // SAFETY: The test invokes this helper through the JIT trampoline with
        // a stack-allocated out slot.
        unsafe {
            *out = value;
        }
        crate::JIT_HELPER_STATUS_OK
    }

    extern "C" fn test_known_strlen_helper(_value_ptr: usize, out: *mut i64) -> i32 {
        if out.is_null() {
            return crate::JIT_HELPER_STATUS_FALLBACK;
        }
        // SAFETY: The test invokes this helper through the JIT trampoline with
        // a stack-allocated out slot.
        unsafe {
            *out = 5;
        }
        crate::JIT_HELPER_STATUS_OK
    }

    extern "C" fn test_string_concat_helper(
        lhs_ptr: usize,
        rhs_ptr: usize,
        out: *mut usize,
    ) -> i32 {
        if lhs_ptr != 0xfeed || rhs_ptr != 0xbeef || out.is_null() {
            return crate::JIT_HELPER_STATUS_FALLBACK;
        }
        // SAFETY: The test invokes this helper through the JIT trampoline with
        // a stack-allocated out slot.
        unsafe {
            *out = 0xabc;
        }
        crate::JIT_HELPER_STATUS_OK
    }

    fn arithmetic_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/jit/eligible-int-add.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_arithmetic", FunctionFlags::default(), span);
        builder.set_entry(function);
        let block = builder.append_block(function);
        let ten = builder.add_constant(IrConstant::Int(10));
        let three = builder.add_constant(IrConstant::Int(3));
        let two = builder.add_constant(IrConstant::Int(2));
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        let r3 = builder.alloc_register(function);
        let r4 = builder.alloc_register(function);
        let r5 = builder.alloc_register(function);
        builder.emit_load_const(function, block, r0, ten, span);
        builder.emit_load_const(function, block, r1, three, span);
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r2,
                op: BinaryOp::Add,
                lhs: Operand::Register(r0),
                rhs: Operand::Register(r1),
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r3,
                op: BinaryOp::Sub,
                lhs: Operand::Register(r2),
                rhs: Operand::Register(r1),
            },
            span,
        );
        builder.emit_load_const(function, block, r4, two, span);
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r5,
                op: BinaryOp::Mul,
                lhs: Operand::Register(r3),
                rhs: Operand::Register(r4),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r5)), span);
        (builder.finish(), function)
    }

    fn constant_return_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/cranelift/native/return-42.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_const_return", FunctionFlags::default(), span);
        builder.set_return_type(function, Some(IrReturnType::Int));
        builder.set_entry(function);
        let block = builder.append_block(function);
        let forty_two = builder.add_constant(IrConstant::Int(42));
        let r0 = builder.alloc_register(function);
        builder.emit_load_const(function, block, r0, forty_two, span);
        builder.terminate_return(function, block, Some(Operand::Register(r0)), span);
        (builder.finish(), function)
    }

    fn helper_arithmetic_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder
            .add_file("tests/fixtures/performance/cranelift/helper-call/add-mul-expression.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_helper_add_mul", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let local_a = typed_int_param(&mut builder, function, "a");
        let block = builder.append_block(function);
        let two = builder.add_constant(IrConstant::Int(2));
        let three = builder.add_constant(IrConstant::Int(3));
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        let r3 = builder.alloc_register(function);
        let r4 = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: r0,
                local: local_a,
            },
            span,
        );
        builder.emit_load_const(function, block, r1, two, span);
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r2,
                op: BinaryOp::Add,
                lhs: Operand::Register(r0),
                rhs: Operand::Register(r1),
            },
            span,
        );
        builder.emit_load_const(function, block, r3, three, span);
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r4,
                op: BinaryOp::Mul,
                lhs: Operand::Register(r2),
                rhs: Operand::Register(r3),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r4)), span);
        (builder.finish(), function)
    }

    fn scalar_identity_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/cranelift/region/identity.php");
        let span = IrSpan::new(file, 0, 0);
        let function =
            builder.start_function("jit_scalar_identity", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let local = typed_int_param(&mut builder, function, "value");
        let block = builder.append_block(function);
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal { dst: result, local },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        (builder.finish(), function)
    }

    fn scalar_branch_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/cranelift/region/branch.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_scalar_branch", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let local = typed_int_param(&mut builder, function, "condition");
        let entry = builder.append_block(function);
        let if_true = builder.append_block(function);
        let if_false = builder.append_block(function);
        let condition = builder.alloc_register(function);
        builder.emit(
            function,
            entry,
            InstructionKind::LoadLocal {
                dst: condition,
                local,
            },
            span,
        );
        builder.terminate_jump_if(
            function,
            entry,
            Operand::Register(condition),
            if_true,
            if_false,
            span,
        );
        let eleven = builder.add_constant(IrConstant::Int(11));
        let true_value = builder.alloc_register(function);
        builder.emit_load_const(function, if_true, true_value, eleven, span);
        builder.terminate_return(function, if_true, Some(Operand::Register(true_value)), span);
        let twenty_two = builder.add_constant(IrConstant::Int(22));
        let false_value = builder.alloc_register(function);
        builder.emit_load_const(function, if_false, false_value, twenty_two, span);
        builder.terminate_return(
            function,
            if_false,
            Some(Operand::Register(false_value)),
            span,
        );
        (builder.finish(), function)
    }

    fn scalar_direct_call_fixture() -> (php_ir::IrUnit, FunctionId, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/cranelift/region/direct-call.php");
        let span = IrSpan::new(file, 0, 0);

        let callee = builder.start_function("native_increment", FunctionFlags::default(), span);
        builder.set_return_type(callee, Some(IrReturnType::Int));
        let callee_value = typed_int_param(&mut builder, callee, "value");
        let callee_block = builder.append_block(callee);
        let loaded = builder.alloc_register(callee);
        let result = builder.alloc_register(callee);
        let one = builder.add_constant(IrConstant::Int(1));
        builder.emit(
            callee,
            callee_block,
            InstructionKind::LoadLocal {
                dst: loaded,
                local: callee_value,
            },
            span,
        );
        builder.emit(
            callee,
            callee_block,
            InstructionKind::Binary {
                dst: result,
                op: BinaryOp::Add,
                lhs: Operand::Register(loaded),
                rhs: Operand::Constant(one),
            },
            span,
        );
        builder.terminate_return(callee, callee_block, Some(Operand::Register(result)), span);
        builder.register_function_name("native_increment", callee);

        let caller = builder.start_function("native_wrapper", FunctionFlags::default(), span);
        builder.set_entry(caller);
        builder.set_return_type(caller, Some(IrReturnType::Int));
        let caller_value = typed_int_param(&mut builder, caller, "value");
        let caller_block = builder.append_block(caller);
        let argument = builder.alloc_register(caller);
        let call_result = builder.alloc_register(caller);
        builder.emit(
            caller,
            caller_block,
            InstructionKind::LoadLocal {
                dst: argument,
                local: caller_value,
            },
            span,
        );
        builder.emit(
            caller,
            caller_block,
            InstructionKind::CallFunction {
                dst: call_result,
                name: "native_increment".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Register(argument),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: Some(caller_value),
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(
            caller,
            caller_block,
            Some(Operand::Register(call_result)),
            span,
        );
        (builder.finish(), caller, callee)
    }

    fn scalar_dynamic_call_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("dynamic-call.php");
        let span = IrSpan::new(file, 0, 1);
        let function = builder.start_function("dynamic_wrapper", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let block = builder.append_block(function);
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: "deployment_function".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(result)), span);
        (builder.finish(), function)
    }

    fn scalar_loop_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/cranelift/region/loop.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_scalar_loop", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let limit = typed_int_param(&mut builder, function, "limit");
        let index = builder.intern_local(function, "index");
        let entry = builder.append_block(function);
        let header = builder.append_block(function);
        let body = builder.append_block(function);
        let exit = builder.append_block(function);
        let zero = builder.add_constant(IrConstant::Int(0));
        builder.emit(
            function,
            entry,
            InstructionKind::StoreLocal {
                local: index,
                src: Operand::Constant(zero),
            },
            span,
        );
        builder.terminate_jump(function, entry, header, span);

        let current = builder.alloc_register(function);
        let end = builder.alloc_register(function);
        let condition = builder.alloc_register(function);
        builder.emit(
            function,
            header,
            InstructionKind::LoadLocal {
                dst: current,
                local: index,
            },
            span,
        );
        builder.emit(
            function,
            header,
            InstructionKind::LoadLocal {
                dst: end,
                local: limit,
            },
            span,
        );
        builder.emit(
            function,
            header,
            InstructionKind::Compare {
                dst: condition,
                op: php_ir::CompareOp::Less,
                lhs: Operand::Register(current),
                rhs: Operand::Register(end),
            },
            span,
        );
        builder.terminate_jump_if(
            function,
            header,
            Operand::Register(condition),
            body,
            exit,
            span,
        );

        let body_current = builder.alloc_register(function);
        let incremented = builder.alloc_register(function);
        let one = builder.add_constant(IrConstant::Int(1));
        builder.emit(
            function,
            body,
            InstructionKind::LoadLocal {
                dst: body_current,
                local: index,
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::Binary {
                dst: incremented,
                op: BinaryOp::Add,
                lhs: Operand::Register(body_current),
                rhs: Operand::Constant(one),
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::StoreLocal {
                local: index,
                src: Operand::Register(incremented),
            },
            span,
        );
        builder.terminate_jump(function, body, header, span);

        let result = builder.alloc_register(function);
        builder.emit(
            function,
            exit,
            InstructionKind::LoadLocal {
                dst: result,
                local: index,
            },
            span,
        );
        builder.terminate_return(function, exit, Some(Operand::Register(result)), span);
        (builder.finish(), function)
    }

    fn helper_overflow_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file =
            builder.add_file("tests/fixtures/performance/cranelift/helper-call/overflow-add.php");
        let span = IrSpan::new(file, 0, 0);
        let function =
            builder.start_function("jit_helper_overflow", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let local_a = typed_int_param(&mut builder, function, "a");
        let block = builder.append_block(function);
        let one = builder.add_constant(IrConstant::Int(1));
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: r0,
                local: local_a,
            },
            span,
        );
        builder.emit_load_const(function, block, r1, one, span);
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r2,
                op: BinaryOp::Add,
                lhs: Operand::Register(r0),
                rhs: Operand::Register(r1),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r2)), span);
        (builder.finish(), function)
    }

    fn packed_array_fetch_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file =
            builder.add_file("tests/fixtures/performance/cranelift/arrays/packed-fetch-valid.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_packed_fetch", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let local_xs = typed_array_param(&mut builder, function, "xs");
        let local_i = typed_int_param(&mut builder, function, "i");
        let block = builder.append_block(function);
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: r0,
                local: local_xs,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: r1,
                local: local_i,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::FetchDim {
                dst: r2,
                array: Operand::Register(r0),
                key: Operand::Register(r1),
                quiet: false,
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r2)), span);
        (builder.finish(), function)
    }

    fn known_strlen_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file =
            builder.add_file("tests/fixtures/performance/cranelift/known-calls/strlen-valid.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_known_strlen", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let local_s = typed_string_param(&mut builder, function, "s");
        let block = builder.append_block(function);
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: r0,
                local: local_s,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: r1,
                name: "strlen".to_owned(),
                args: vec![php_ir::instruction::IrCallArg {
                    name: None,
                    value: Operand::Register(r0),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: Some(local_s),
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                }],
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r1)), span);
        (builder.finish(), function)
    }

    fn string_concat_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file =
            builder.add_file("tests/fixtures/performance/cranelift/string-concat/two-strings.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_string_concat", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::String));
        let local_lhs = typed_string_param(&mut builder, function, "lhs");
        let local_rhs = typed_string_param(&mut builder, function, "rhs");
        let block = builder.append_block(function);
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: r0,
                local: local_lhs,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal {
                dst: r1,
                local: local_rhs,
            },
            span,
        );
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r2,
                op: BinaryOp::Concat,
                lhs: Operand::Register(r0),
                rhs: Operand::Register(r1),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r2)), span);
        (builder.finish(), function)
    }

    fn packed_foreach_int_sum_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder
            .add_file("tests/fixtures/performance/cranelift/arrays/packed-foreach-sum-all-int.php");
        let span = IrSpan::new(file, 0, 0);
        let function =
            builder.start_function("jit_packed_foreach_sum", FunctionFlags::default(), span);
        builder.set_entry(function);
        builder.set_return_type(function, Some(IrReturnType::Int));
        let local_xs = typed_array_param(&mut builder, function, "xs");
        let local_sum = builder.intern_local(function, "sum");
        let local_x = builder.intern_local(function, "x");
        let entry = builder.append_block(function);
        let condition = builder.append_block(function);
        let body = builder.append_block(function);
        let after = builder.append_block(function);
        let zero = builder.add_constant(IrConstant::Int(0));
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        let r3 = builder.alloc_register(function);
        let r4 = builder.alloc_register(function);
        let r5 = builder.alloc_register(function);
        let r6 = builder.alloc_register(function);
        let r7 = builder.alloc_register(function);
        let r8 = builder.alloc_register(function);
        builder.emit_load_const(function, entry, r0, zero, span);
        builder.emit(
            function,
            entry,
            InstructionKind::StoreLocal {
                local: local_sum,
                src: Operand::Register(r0),
            },
            span,
        );
        builder.emit(
            function,
            entry,
            InstructionKind::Discard {
                src: Operand::Register(r0),
            },
            span,
        );
        builder.emit(
            function,
            entry,
            InstructionKind::LoadLocal {
                dst: r1,
                local: local_xs,
            },
            span,
        );
        builder.emit(
            function,
            entry,
            InstructionKind::ForeachInit {
                iterator: r2,
                source: Operand::Register(r1),
            },
            span,
        );
        builder.terminate_jump(function, entry, condition, span);
        builder.emit(
            function,
            condition,
            InstructionKind::ForeachNext {
                has_value: r3,
                iterator: r2,
                key: None,
                value: r4,
            },
            span,
        );
        builder.terminate_jump_if(
            function,
            condition,
            Operand::Register(r3),
            body,
            after,
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::StoreLocal {
                local: local_x,
                src: Operand::Register(r4),
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::LoadLocal {
                dst: r5,
                local: local_sum,
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::LoadLocal {
                dst: r6,
                local: local_x,
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::Binary {
                dst: r7,
                op: BinaryOp::Add,
                lhs: Operand::Register(r5),
                rhs: Operand::Register(r6),
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::StoreLocal {
                local: local_sum,
                src: Operand::Register(r7),
            },
            span,
        );
        builder.emit(
            function,
            body,
            InstructionKind::Discard {
                src: Operand::Register(r7),
            },
            span,
        );
        builder.terminate_jump(function, body, condition, span);
        builder.emit(
            function,
            after,
            InstructionKind::LoadLocal {
                dst: r8,
                local: local_sum,
            },
            span,
        );
        builder.terminate_return(function, after, Some(Operand::Register(r8)), span);
        (builder.finish(), function)
    }

    fn typed_int_param(builder: &mut IrBuilder, function: FunctionId, name: &str) -> LocalId {
        let local = builder.intern_local(function, name);
        builder.push_param(
            function,
            IrParam {
                name: name.to_owned(),
                local,
                required: true,
                default: None,
                type_: Some(IrReturnType::Int),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        local
    }

    fn typed_array_param(builder: &mut IrBuilder, function: FunctionId, name: &str) -> LocalId {
        let local = builder.intern_local(function, name);
        builder.push_param(
            function,
            IrParam {
                name: name.to_owned(),
                local,
                required: true,
                default: None,
                type_: Some(IrReturnType::Array),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        local
    }

    fn typed_string_param(builder: &mut IrBuilder, function: FunctionId, name: &str) -> LocalId {
        let local = builder.intern_local(function, name);
        builder.push_param(
            function,
            IrParam {
                name: name.to_owned(),
                local,
                required: true,
                default: None,
                type_: Some(IrReturnType::String),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
        local
    }

    fn unsupported_binary_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/jit/rejected-dynamic.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_division", FunctionFlags::default(), span);
        builder.set_entry(function);
        let block = builder.append_block(function);
        let six = builder.add_constant(IrConstant::Int(6));
        let three = builder.add_constant(IrConstant::Int(3));
        let r0 = builder.alloc_register(function);
        let r1 = builder.alloc_register(function);
        let r2 = builder.alloc_register(function);
        builder.emit_load_const(function, block, r0, six, span);
        builder.emit_load_const(function, block, r1, three, span);
        builder.emit(
            function,
            block,
            InstructionKind::Binary {
                dst: r2,
                op: BinaryOp::Div,
                lhs: Operand::Register(r0),
                rhs: Operand::Register(r1),
            },
            span,
        );
        builder.terminate_return(function, block, Some(Operand::Register(r2)), span);
        (builder.finish(), function)
    }

    fn bool_return_fixture() -> (php_ir::IrUnit, FunctionId) {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/performance/jit/rejected-dynamic.php");
        let span = IrSpan::new(file, 0, 0);
        let function = builder.start_function("jit_bool", FunctionFlags::default(), span);
        builder.set_entry(function);
        let block = builder.append_block(function);
        let value = builder.add_constant(IrConstant::Bool(true));
        builder.terminate_return(function, block, Some(Operand::Constant(value)), span);
        (builder.finish(), function)
    }
}
