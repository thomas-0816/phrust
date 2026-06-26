//! Optional Cranelift IR lowering and native-entry prototype for performance.
//!
//! This module is compiled only with `jit-cranelift`. It produces and verifies
//! Cranelift IR text for constrained integer, array, string, property, and
//! dispatch-helper subsets. Native execution is still default-off and requires
//! the caller to opt in explicitly.

use crate::{
    JIT_HELPER_STATUS_OK, JIT_HELPER_STATUS_OVERFLOW, JIT_RUNTIME_ABI_HASH, JitBackend,
    JitBackendApi, JitBackendCompileOutcome, JitBackendCompileRequest, JitCompileStatus,
    JitEligibility, JitFunctionHandle, JitNativeSpecialization, JitPropertyLoadMetadata,
    analyze_jit_eligibility,
};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{
    self, AbiParam, Function, InstBuilder, MemFlagsData, Signature, StackSlotData, StackSlotKind,
    UserFuncName, types,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::verifier::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use php_ir::instruction::{Terminator, TerminatorKind};
use php_ir::{
    BinaryOp, BlockId, CompareOp, FunctionId, Instruction, InstructionKind, IrConstant, IrFunction,
    IrParam, IrReturnType, IrUnit, LocalId, Operand, RegId,
};
use std::collections::BTreeMap;
use std::fmt;
use std::time::Instant;

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
struct ConstantReturnCandidate {
    value: i64,
    arity: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeConstantCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HelperArithmeticCandidate {
    arity: u8,
    fast_path_hits: u64,
    has_control_flow: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeHelperCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
    fast_path_hits: u64,
    has_control_flow: bool,
}

fn leak_jit_module_for_handle_lifetime(module: JITModule) {
    // Keep Cranelift-owned executable memory alive for every copied
    // `JitFunctionHandle`. performance intentionally leaks the module instead of
    // exposing a reclamation path that could invalidate raw function pointers.
    let leaked_module: &'static mut JITModule = Box::leak(Box::new(module));
    let _ = leaked_module;
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackedArrayFetchCandidate {
    array_param: LocalId,
    index_param: LocalId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativePackedArrayFetchCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackedForeachIntSumCandidate {
    array_param: LocalId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativePackedForeachIntSumCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KnownCallKind {
    Strlen,
    Count,
}

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
struct KnownCallCandidate {
    kind: KnownCallKind,
    value_param: LocalId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeKnownCallCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StringConcatCandidate {
    lhs_param: LocalId,
    rhs_param: LocalId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeStringConcatCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PropertyLoadCandidate {
    object_param: LocalId,
    metadata: JitPropertyLoadMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativePropertyLoadCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
}

const JIT_PACKED_ARRAY_STATUS_BOUNDS_EXIT: i32 = 2;
const JIT_PACKED_ARRAY_STATUS_LAYOUT_EXIT: i32 = 3;
const PACKED_ARRAY_LEN_HELPER_SYMBOL: &str = "phrust_jit_array_len_abi";
const PACKED_ARRAY_FETCH_HELPER_SYMBOL: &str = "phrust_jit_array_fetch_int_slow_abi";
const KNOWN_STRLEN_HELPER_SYMBOL: &str = "phrust_jit_strlen_known_abi";
const KNOWN_COUNT_HELPER_SYMBOL: &str = "phrust_jit_count_known_abi";
const STRING_CONCAT_HELPER_SYMBOL: &str = "php_jit_concat_string_string_fast";
const PROPERTY_LOAD_HELPER_SYMBOL: &str = "php_jit_property_load_monomorphic_fast";

/// Cranelift backend skeleton that lowers and verifies CLIF but never executes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CraneliftNoExecBackend;

impl JitBackendApi for CraneliftNoExecBackend {
    fn backend(&self) -> JitBackend {
        JitBackend::CraneliftExperiment
    }

    fn compile_region(
        &mut self,
        request: &JitBackendCompileRequest<'_>,
    ) -> JitBackendCompileOutcome {
        if !request.allow_native_execution {
            return JitBackendCompileOutcome::skipped(
                JitCompileStatus::NativeExecutionDisabled,
                format!(
                    "Cranelift no-exec backend refused native entry for region `{}`",
                    request.compile.region_id
                ),
            );
        }

        let (Some(unit), Some(function)) = (request.unit, request.function) else {
            return JitBackendCompileOutcome::skipped(
                JitCompileStatus::Rejected {
                    reason: "cranelift no-exec backend requires IR unit and function".to_owned(),
                },
                format!(
                    "Cranelift no-exec backend missing IR context for region `{}`",
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
                &request.compile.region_id,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return JitBackendCompileOutcome::compiled(
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
                    return JitBackendCompileOutcome::skipped(
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
                &request.compile.region_id,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return JitBackendCompileOutcome::compiled(
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
                    return JitBackendCompileOutcome::skipped(
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
                &request.compile.region_id,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return JitBackendCompileOutcome::compiled(
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
                    return JitBackendCompileOutcome::skipped(
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
            match compile_known_call_native(
                function,
                &candidate,
                helper_address,
                &request.compile.region_id,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return JitBackendCompileOutcome::compiled(
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
                    return JitBackendCompileOutcome::skipped(
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
                &request.compile.region_id,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return JitBackendCompileOutcome::compiled(
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
                    return JitBackendCompileOutcome::skipped(
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

        if let Ok(candidate) = property_load_candidate(unit, function) {
            let start = Instant::now();
            match compile_property_load_native(
                function,
                &candidate,
                request.runtime_helpers.property_load,
                &request.compile.region_id,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return JitBackendCompileOutcome::compiled(
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
                    return JitBackendCompileOutcome::skipped(
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

        if let Ok(candidate) = helper_arithmetic_candidate(unit, function) {
            let start = Instant::now();
            match compile_helper_arithmetic_native(
                unit,
                function,
                candidate.arity,
                candidate.fast_path_hits,
                candidate.has_control_flow,
                &request.compile.region_id,
            ) {
                Ok(compiled) => {
                    let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
                    return JitBackendCompileOutcome::compiled(
                        compiled.handle,
                        format!(
                            "Cranelift native inline-arithmetic region `{}` function={} abi_hash={} code_bytes={} fast_path_hits={} control_flow={}",
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
                    return JitBackendCompileOutcome::skipped(
                        JitCompileStatus::Rejected {
                            reason: error.code.to_owned(),
                        },
                        format!(
                            "Cranelift native inline-arithmetic compile rejected region `{}`: {}",
                            request.compile.region_id, error
                        ),
                    );
                }
            }
        }

        match lower_function_to_cranelift(unit, function) {
            Ok(result) => JitBackendCompileOutcome::skipped(
                JitCompileStatus::Rejected {
                    reason: "cranelift backend verified CLIF but region is not in native executable subset"
                        .to_owned(),
                },
                format!(
                    "Cranelift backend verified region `{}` function={} clif_bytes={} blocks={} instructions={} native_subset=constant-return-or-inline-arithmetic",
                    request.compile.region_id,
                    function.raw(),
                    result.clif.len(),
                    result.stats.blocks_lowered,
                    result.stats.instructions_lowered
                ),
            ),
            Err(error) => JitBackendCompileOutcome::skipped(
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

fn helper_arithmetic_candidate(
    unit: &IrUnit,
    function: FunctionId,
) -> Result<HelperArithmeticCandidate, CraneliftLoweringError> {
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
            "JIT_CRANELIFT_REJECT_HELPER_SHAPE",
            "inline-arithmetic native subset only accepts ordinary leaf functions",
        ));
    }
    if !matches!(
        ir_function.return_type.as_ref(),
        Some(php_ir::IrReturnType::Int)
    ) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_HELPER_RETURN_TYPE",
            "inline-arithmetic native subset requires explicit int return type",
        ));
    }
    let arity: u8 = ir_function.params.len().try_into().map_err(|_| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_HELPER_ARITY",
            "inline-arithmetic native subset supports at most 255 params",
        )
    })?;
    if arity > 4 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_HELPER_ARITY",
            "inline-arithmetic native subset supports at most four params",
        ));
    }
    for param in &ir_function.params {
        if param.by_ref
            || param.variadic
            || param.default.is_some()
            || !matches!(param.type_.as_ref(), Some(php_ir::IrReturnType::Int))
        {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_PARAM",
                "inline-arithmetic native subset requires plain int params",
            ));
        }
    }
    let mut fast_path_hits = 0_u64;
    let has_control_flow = ir_function.blocks.len() > 1;
    for block in &ir_function.blocks {
        for instruction in &block.instructions {
            match &instruction.kind {
                InstructionKind::Nop
                | InstructionKind::LoadConst { .. }
                | InstructionKind::Move { .. }
                | InstructionKind::LoadLocal { .. }
                | InstructionKind::LoadLocalQuiet { .. }
                | InstructionKind::StoreLocal { .. }
                | InstructionKind::Discard { .. } => {}
                InstructionKind::Binary { op, .. } => match op {
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => fast_path_hits += 1,
                    other => {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_HELPER_BINARY",
                            format!("binary op {other:?} is outside inline-arithmetic subset"),
                        ));
                    }
                },
                InstructionKind::Compare { op, .. } => match op {
                    CompareOp::Equal
                    | CompareOp::NotEqual
                    | CompareOp::Identical
                    | CompareOp::NotIdentical
                    | CompareOp::Less
                    | CompareOp::LessEqual
                    | CompareOp::Greater
                    | CompareOp::GreaterEqual => fast_path_hits += 1,
                    other => {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_HELPER_COMPARE",
                            format!("compare op {other:?} is outside inline-arithmetic subset"),
                        ));
                    }
                },
                other => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_HELPER_OPCODE",
                        format!("instruction {other:?} is outside inline-arithmetic native subset"),
                    ));
                }
            }
        }
        match &block.terminator {
            Some(terminator) => match &terminator.kind {
                TerminatorKind::Jump { .. }
                | TerminatorKind::JumpIfFalse { .. }
                | TerminatorKind::JumpIfTrue { .. }
                | TerminatorKind::JumpIf { .. } => {}
                TerminatorKind::Return {
                    value: Some(_),
                    by_ref_local: None,
                } => {}
                other => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_HELPER_TERMINATOR",
                        format!("terminator {other:?} is outside inline-arithmetic native subset"),
                    ));
                }
            },
            None => {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_HELPER_TERMINATOR",
                    "inline-arithmetic native subset requires terminators",
                ));
            }
        }
    }
    if fast_path_hits == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_HELPER_NO_CALLS",
            "inline-arithmetic native subset requires add, sub, mul, or compare",
        ));
    }
    Ok(HelperArithmeticCandidate {
        arity,
        fast_path_hits,
        has_control_flow,
    })
}

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

    let [return_load] = after.instructions.as_slice() else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_RETURN",
            "return block must load the accumulator",
        ));
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
    if matches!(
        ir_function.return_type.as_ref(),
        None | Some(IrReturnType::Void | IrReturnType::Never)
    ) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_RETURN",
            "property-load fast path requires a value return type",
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
    let Some(IrReturnType::Class { name }) = param.type_.as_ref() else {
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

    Ok(PropertyLoadCandidate {
        object_param: param.local,
        metadata: JitPropertyLoadMetadata {
            receiver_class: normalize_class_name(&class.name),
            class_id: class.id.raw(),
            property: property_entry.name.clone(),
            storage_name: property_storage_name(declaring_class, property_entry),
            property_slot_index,
            layout_version: 0,
        },
    })
}

fn lookup_class<'a>(unit: &'a IrUnit, name: &str) -> Option<&'a php_ir::module::ClassEntry> {
    let normalized = normalize_class_name(name);
    unit.classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalized)
}

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

fn normalize_class_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

fn compile_constant_return_native(
    function: FunctionId,
    value: i64,
    arity: u8,
    region_id: &str,
) -> Result<NativeConstantCompileResult, CraneliftLoweringError> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
        })?;
    flag_builder.set("is_pic", "false").map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
    })?;
    let isa_builder = cranelift_native::builder().map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
            format!("host target is unsupported: {error}"),
        )
    })?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
                format!("host ISA setup failed: {error}"),
            )
        })?;
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let mut signature = module.make_signature();
    for _ in 0..arity {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.returns.push(AbiParam::new(types::I64));

    let name = format!(
        "phrust_cl_const_{}_{}",
        function.raw(),
        sanitize_symbol_component(region_id)
    );
    let func_id = module
        .declare_function(&name, Linkage::Local, &signature)
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
    leak_jit_module_for_handle_lifetime(module);
    let handle = JitFunctionHandle::i64_native(
        u64::from(function.raw()) + 1,
        region_id.to_owned(),
        JitBackend::CraneliftExperiment,
        address,
        arity,
        code_bytes,
    );
    Ok(NativeConstantCompileResult { handle, code_bytes })
}

fn compile_helper_arithmetic_native(
    unit: &IrUnit,
    function: FunctionId,
    arity: u8,
    fast_path_hits: u64,
    has_control_flow: bool,
    region_id: &str,
) -> Result<NativeHelperCompileResult, CraneliftLoweringError> {
    let ir_function = unit.functions.get(function.index()).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_FUNCTION",
            format!("function id {} is not present", function.raw()),
        )
    })?;
    let mut flag_builder = settings::builder();
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
        })?;
    flag_builder.set("is_pic", "false").map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
    })?;
    let isa_builder = cranelift_native::builder().map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
            format!("host target is unsupported: {error}"),
        )
    })?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
                format!("host ISA setup failed: {error}"),
            )
        })?;
    let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
    let pointer_type = module.target_config().pointer_type();

    let mut signature = module.make_signature();
    for _ in 0..arity {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));

    let name = format!(
        "phrust_cl_helper_{}_{}",
        function.raw(),
        sanitize_symbol_component(region_id)
    );
    let func_id = module
        .declare_function(&name, Linkage::Local, &signature)
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
        let blocks = create_cranelift_blocks(&mut builder, ir_function)?;
        let entry = blocks.first().copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_CONTROL_FLOW",
                "inline-arithmetic native subset requires at least one block",
            )
        })?;
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let params = builder.block_params(entry).to_vec();
        let result_out = params.get(usize::from(arity)).copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_ARITY",
                "missing native output pointer parameter",
            )
        })?;
        let mut locals = BTreeMap::new();
        for local_index in 0..ir_function.local_count {
            locals.insert(LocalId::new(local_index), builder.declare_var(types::I64));
        }
        for (param, cl_value) in ir_function
            .params
            .iter()
            .zip(params.iter().copied().take(usize::from(arity)))
        {
            let variable = local_variable(&locals, param.local)?;
            builder.def_var(variable, cl_value);
        }

        for ir_block in &ir_function.blocks {
            let block = cranelift_block(&blocks, ir_block.id)?;
            builder.switch_to_block(block);
            let mut registers = BTreeMap::new();
            for instruction in &ir_block.instructions {
                lower_inline_cfg_instruction(
                    &mut builder,
                    unit,
                    &locals,
                    &mut registers,
                    instruction,
                )?;
            }
            lower_inline_cfg_terminator(
                &mut builder,
                unit,
                ir_function,
                &blocks,
                &locals,
                &registers,
                result_out,
                ir_block.id,
                ir_block.terminator.as_ref(),
            )?;
        }
        builder.seal_all_blocks();
        builder.finalize();
    }

    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("Cranelift verifier rejected native inline-arithmetic IR: {error}"),
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
    leak_jit_module_for_handle_lifetime(module);
    let handle = JitFunctionHandle::i64_status_out_native(
        u64::from(function.raw()) + 1,
        region_id.to_owned(),
        JitBackend::CraneliftExperiment,
        address,
        arity,
        code_bytes,
        0,
        fast_path_hits,
    );
    Ok(NativeHelperCompileResult {
        handle,
        code_bytes,
        fast_path_hits,
        has_control_flow,
    })
}

fn compile_packed_array_fetch_native(
    function: FunctionId,
    _candidate: &PackedArrayFetchCandidate,
    helper_address: usize,
    region_id: &str,
) -> Result<NativePackedArrayFetchCompileResult, CraneliftLoweringError> {
    if helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FETCH_HELPER",
            "packed-array fetch requires a runtime helper address",
        ));
    }

    let mut flag_builder = settings::builder();
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
        })?;
    flag_builder.set("is_pic", "false").map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
    })?;
    let isa_builder = cranelift_native::builder().map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
            format!("host target is unsupported: {error}"),
        )
    })?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
                format!("host ISA setup failed: {error}"),
            )
        })?;
    let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
    jit_builder.symbol(
        PACKED_ARRAY_FETCH_HELPER_SYMBOL,
        helper_address as *const u8,
    );
    let mut module = JITModule::new(jit_builder);
    let pointer_type = module.target_config().pointer_type();

    let mut helper_signature = module.make_signature();
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.params.push(AbiParam::new(types::I64));
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.returns.push(AbiParam::new(types::I32));
    let helper_func = module
        .declare_function(
            PACKED_ARRAY_FETCH_HELPER_SYMBOL,
            Linkage::Import,
            &helper_signature,
        )
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

    let name = format!(
        "phrust_cl_packed_fetch_{}_{}",
        function.raw(),
        sanitize_symbol_component(region_id)
    );
    let func_id = module
        .declare_function(&name, Linkage::Local, &signature)
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
    leak_jit_module_for_handle_lifetime(module);
    let handle = JitFunctionHandle::value_i64_status_out_native(
        u64::from(function.raw()) + 1,
        region_id.to_owned(),
        JitBackend::CraneliftExperiment,
        address,
        code_bytes,
        1,
        1,
    );
    Ok(NativePackedArrayFetchCompileResult { handle, code_bytes })
}

fn compile_packed_foreach_int_sum_native(
    function: FunctionId,
    candidate: &PackedForeachIntSumCandidate,
    len_helper_address: usize,
    fetch_helper_address: usize,
    region_id: &str,
) -> Result<NativePackedForeachIntSumCompileResult, CraneliftLoweringError> {
    if len_helper_address == 0 || fetch_helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PACKED_FOREACH_HELPER",
            "packed foreach sum requires length and fetch runtime helper addresses",
        ));
    }

    let mut flag_builder = settings::builder();
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
        })?;
    flag_builder.set("is_pic", "false").map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
    })?;
    let isa_builder = cranelift_native::builder().map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
            format!("host target is unsupported: {error}"),
        )
    })?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
                format!("host ISA setup failed: {error}"),
            )
        })?;
    let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
    jit_builder.symbol(
        PACKED_ARRAY_LEN_HELPER_SYMBOL,
        len_helper_address as *const u8,
    );
    jit_builder.symbol(
        PACKED_ARRAY_FETCH_HELPER_SYMBOL,
        fetch_helper_address as *const u8,
    );
    let mut module = JITModule::new(jit_builder);
    let pointer_type = module.target_config().pointer_type();

    let mut len_signature = module.make_signature();
    len_signature.params.push(AbiParam::new(pointer_type));
    len_signature.params.push(AbiParam::new(pointer_type));
    len_signature.returns.push(AbiParam::new(types::I32));
    let len_helper = module
        .declare_function(
            PACKED_ARRAY_LEN_HELPER_SYMBOL,
            Linkage::Import,
            &len_signature,
        )
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
        .declare_function(
            PACKED_ARRAY_FETCH_HELPER_SYMBOL,
            Linkage::Import,
            &fetch_signature,
        )
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

    let name = format!(
        "phrust_cl_packed_foreach_sum_{}_{}_{}",
        function.raw(),
        candidate.array_param.raw(),
        sanitize_symbol_component(region_id)
    );
    let func_id = module
        .declare_function(&name, Linkage::Local, &signature)
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

        let len_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let element_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let len_out = builder.ins().stack_addr(pointer_type, len_slot, 0);
        let len_ref = module.declare_func_in_func(len_helper, builder.func);
        let len_call = builder.ins().call(len_ref, &[value_ptr, len_out]);
        let len_status = builder.inst_results(len_call)[0];
        let ok_status = builder
            .ins()
            .iconst(types::I32, i64::from(JIT_HELPER_STATUS_OK));
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
            .iconst(types::I32, i64::from(JIT_HELPER_STATUS_OVERFLOW));
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
    leak_jit_module_for_handle_lifetime(module);
    let handle = JitFunctionHandle::value_status_out_native(
        u64::from(function.raw()) + 1,
        region_id.to_owned(),
        JitBackend::CraneliftExperiment,
        address,
        code_bytes,
        0,
        1,
        JitNativeSpecialization::PackedForeachIntSum,
    );
    Ok(NativePackedForeachIntSumCompileResult { handle, code_bytes })
}

fn compile_known_call_native(
    function: FunctionId,
    candidate: &KnownCallCandidate,
    helper_address: usize,
    region_id: &str,
) -> Result<NativeKnownCallCompileResult, CraneliftLoweringError> {
    if helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_KNOWN_CALL_HELPER",
            "known-call fast path requires a runtime helper address",
        ));
    }

    let mut flag_builder = settings::builder();
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
        })?;
    flag_builder.set("is_pic", "false").map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
    })?;
    let isa_builder = cranelift_native::builder().map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
            format!("host target is unsupported: {error}"),
        )
    })?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
                format!("host ISA setup failed: {error}"),
            )
        })?;
    let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
    jit_builder.symbol(candidate.kind.helper_symbol(), helper_address as *const u8);
    let mut module = JITModule::new(jit_builder);
    let pointer_type = module.target_config().pointer_type();

    let mut helper_signature = module.make_signature();
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.returns.push(AbiParam::new(types::I32));
    let helper = module
        .declare_function(
            candidate.kind.helper_symbol(),
            Linkage::Import,
            &helper_signature,
        )
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

    let name = format!(
        "phrust_cl_known_call_{}_{}_{}_{}",
        candidate.kind.function_name(),
        function.raw(),
        candidate.value_param.raw(),
        sanitize_symbol_component(region_id)
    );
    let func_id = module
        .declare_function(&name, Linkage::Local, &signature)
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
    leak_jit_module_for_handle_lifetime(module);
    let handle = JitFunctionHandle::value_status_out_native(
        u64::from(function.raw()) + 1,
        region_id.to_owned(),
        JitBackend::CraneliftExperiment,
        address,
        code_bytes,
        1,
        1,
        candidate.kind.specialization(),
    );
    Ok(NativeKnownCallCompileResult { handle, code_bytes })
}

fn compile_string_concat_native(
    function: FunctionId,
    candidate: &StringConcatCandidate,
    helper_address: usize,
    region_id: &str,
) -> Result<NativeStringConcatCompileResult, CraneliftLoweringError> {
    if helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_CONCAT_HELPER",
            "string-concat fast path requires a runtime helper address",
        ));
    }

    let mut flag_builder = settings::builder();
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
        })?;
    flag_builder.set("is_pic", "false").map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
    })?;
    let isa_builder = cranelift_native::builder().map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
            format!("host target is unsupported: {error}"),
        )
    })?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
                format!("host ISA setup failed: {error}"),
            )
        })?;
    let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
    jit_builder.symbol(STRING_CONCAT_HELPER_SYMBOL, helper_address as *const u8);
    let mut module = JITModule::new(jit_builder);
    let pointer_type = module.target_config().pointer_type();

    let mut helper_signature = module.make_signature();
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.returns.push(AbiParam::new(types::I32));
    let helper = module
        .declare_function(
            STRING_CONCAT_HELPER_SYMBOL,
            Linkage::Import,
            &helper_signature,
        )
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

    let name = format!(
        "phrust_cl_string_concat_{}_{}_{}_{}",
        function.raw(),
        candidate.lhs_param.raw(),
        candidate.rhs_param.raw(),
        sanitize_symbol_component(region_id)
    );
    let func_id = module
        .declare_function(&name, Linkage::Local, &signature)
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
    leak_jit_module_for_handle_lifetime(module);
    let handle = JitFunctionHandle::value_value_status_out_native(
        u64::from(function.raw()) + 1,
        region_id.to_owned(),
        JitBackend::CraneliftExperiment,
        address,
        code_bytes,
        1,
        1,
        JitNativeSpecialization::StringConcat,
    );
    Ok(NativeStringConcatCompileResult { handle, code_bytes })
}

fn compile_property_load_native(
    function: FunctionId,
    candidate: &PropertyLoadCandidate,
    helper_address: usize,
    region_id: &str,
) -> Result<NativePropertyLoadCompileResult, CraneliftLoweringError> {
    if helper_address == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_PROPERTY_LOAD_HELPER",
            "property-load fast path requires a runtime helper address",
        ));
    }

    let mut flag_builder = settings::builder();
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
        })?;
    flag_builder.set("is_pic", "false").map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_FLAGS", error.to_string())
    })?;
    let isa_builder = cranelift_native::builder().map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
            format!("host target is unsupported: {error}"),
        )
    })?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_TARGET",
                format!("host ISA setup failed: {error}"),
            )
        })?;
    let mut jit_builder = JITBuilder::with_isa(isa, default_libcall_names());
    jit_builder.symbol(PROPERTY_LOAD_HELPER_SYMBOL, helper_address as *const u8);
    let mut module = JITModule::new(jit_builder);
    let pointer_type = module.target_config().pointer_type();

    let mut helper_signature = module.make_signature();
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.params.push(AbiParam::new(pointer_type));
    helper_signature.returns.push(AbiParam::new(types::I32));
    let helper = module
        .declare_function(
            PROPERTY_LOAD_HELPER_SYMBOL,
            Linkage::Import,
            &helper_signature,
        )
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

    let name = format!(
        "phrust_cl_property_load_{}_{}_{}_{}",
        function.raw(),
        candidate.object_param.raw(),
        candidate.metadata.property,
        sanitize_symbol_component(region_id)
    );
    let func_id = module
        .declare_function(&name, Linkage::Local, &signature)
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
    leak_jit_module_for_handle_lifetime(module);
    let handle = JitFunctionHandle::value_metadata_status_out_native(
        u64::from(function.raw()) + 1,
        region_id.to_owned(),
        JitBackend::CraneliftExperiment,
        address,
        code_bytes,
        1,
        1,
        candidate.metadata.clone(),
    );
    Ok(NativePropertyLoadCompileResult { handle, code_bytes })
}

fn create_cranelift_blocks(
    builder: &mut FunctionBuilder<'_>,
    ir_function: &IrFunction,
) -> Result<Vec<ir::Block>, CraneliftLoweringError> {
    let mut blocks = Vec::with_capacity(ir_function.blocks.len());
    for (index, ir_block) in ir_function.blocks.iter().enumerate() {
        if ir_block.id.index() != index {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_CONTROL_FLOW",
                format!(
                    "non-dense block id {} at position {} is outside inline CFG subset",
                    ir_block.id.raw(),
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

fn next_ir_block_id(
    ir_function: &IrFunction,
    current: BlockId,
) -> Result<BlockId, CraneliftLoweringError> {
    let next_index = current.index() + 1;
    ir_function
        .blocks
        .get(next_index)
        .map(|block| block.id)
        .ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_CONTROL_FLOW",
                format!("block {} has no fallthrough successor", current.raw()),
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

fn lower_inline_cfg_operand(
    builder: &mut FunctionBuilder<'_>,
    unit: &IrUnit,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, ir::Value>,
    operand: &Operand,
) -> Result<ir::Value, CraneliftLoweringError> {
    match operand {
        Operand::Register(reg) => registers.get(reg).copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
                format!("register {} has not been lowered in this block", reg.raw()),
            )
        }),
        Operand::Constant(constant) => {
            let value = constant_value(unit, *constant)?;
            Ok(builder.ins().iconst(types::I64, value))
        }
        Operand::Local(local) => use_local_variable(builder, locals, *local),
    }
}

fn lower_inline_cfg_instruction(
    builder: &mut FunctionBuilder<'_>,
    unit: &IrUnit,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &mut BTreeMap<RegId, ir::Value>,
    instruction: &Instruction,
) -> Result<(), CraneliftLoweringError> {
    match &instruction.kind {
        InstructionKind::Nop => {}
        InstructionKind::LoadConst { dst, constant } => {
            let value = constant_value(unit, *constant)?;
            let cl_value = builder.ins().iconst(types::I64, value);
            registers.insert(*dst, cl_value);
        }
        InstructionKind::Move { dst, src } => {
            let cl_value = lower_inline_cfg_operand(builder, unit, locals, registers, src)?;
            registers.insert(*dst, cl_value);
        }
        InstructionKind::LoadLocal { dst, local }
        | InstructionKind::LoadLocalQuiet { dst, local } => {
            let cl_value = use_local_variable(builder, locals, *local)?;
            registers.insert(*dst, cl_value);
        }
        InstructionKind::StoreLocal { local, src } => {
            let cl_value = lower_inline_cfg_operand(builder, unit, locals, registers, src)?;
            let variable = local_variable(locals, *local)?;
            builder.def_var(variable, cl_value);
        }
        InstructionKind::Discard { src } => {
            let _ = lower_inline_cfg_operand(builder, unit, locals, registers, src)?;
        }
        InstructionKind::Binary { dst, op, lhs, rhs } => {
            let lhs = lower_inline_cfg_operand(builder, unit, locals, registers, lhs)?;
            let rhs = lower_inline_cfg_operand(builder, unit, locals, registers, rhs)?;
            let cl_value = lower_checked_inline_binary(builder, *op, lhs, rhs)?;
            registers.insert(*dst, cl_value);
        }
        InstructionKind::Compare { dst, op, lhs, rhs } => {
            let lhs = lower_inline_cfg_operand(builder, unit, locals, registers, lhs)?;
            let rhs = lower_inline_cfg_operand(builder, unit, locals, registers, rhs)?;
            let cl_value = builder.ins().icmp(compare_intcc(*op)?, lhs, rhs);
            registers.insert(*dst, cl_value);
        }
        other => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_OPCODE",
                format!("instruction {other:?} is outside inline CFG native subset"),
            ));
        }
    }
    Ok(())
}

fn lower_checked_inline_binary(
    builder: &mut FunctionBuilder<'_>,
    op: BinaryOp,
    lhs: ir::Value,
    rhs: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (result, overflow) = match op {
        BinaryOp::Add => builder.ins().sadd_overflow(lhs, rhs),
        BinaryOp::Sub => builder.ins().ssub_overflow(lhs, rhs),
        BinaryOp::Mul => builder.ins().smul_overflow(lhs, rhs),
        other => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_BINARY",
                format!("binary op {other:?} is outside inline CFG native subset"),
            ));
        }
    };
    let overflow_block = builder.create_block();
    let ok_block = builder.create_block();
    builder
        .ins()
        .brif(overflow, overflow_block, &[], ok_block, &[]);
    builder.switch_to_block(overflow_block);
    let status = builder
        .ins()
        .iconst(types::I32, i64::from(JIT_HELPER_STATUS_OVERFLOW));
    builder.ins().return_(&[status]);
    builder.switch_to_block(ok_block);
    Ok(result)
}

fn compare_intcc(op: CompareOp) -> Result<IntCC, CraneliftLoweringError> {
    match op {
        CompareOp::Equal | CompareOp::Identical => Ok(IntCC::Equal),
        CompareOp::NotEqual | CompareOp::NotIdentical => Ok(IntCC::NotEqual),
        CompareOp::Less => Ok(IntCC::SignedLessThan),
        CompareOp::LessEqual => Ok(IntCC::SignedLessThanOrEqual),
        CompareOp::Greater => Ok(IntCC::SignedGreaterThan),
        CompareOp::GreaterEqual => Ok(IntCC::SignedGreaterThanOrEqual),
        other => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_HELPER_COMPARE",
            format!("compare op {other:?} is outside inline CFG native subset"),
        )),
    }
}

fn lower_inline_cfg_condition(
    builder: &mut FunctionBuilder<'_>,
    unit: &IrUnit,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, ir::Value>,
    condition: &Operand,
) -> Result<ir::Value, CraneliftLoweringError> {
    let value = lower_inline_cfg_operand(builder, unit, locals, registers, condition)?;
    if builder.func.dfg.value_type(value) == types::I64 {
        Ok(builder.ins().icmp_imm(IntCC::NotEqual, value, 0))
    } else {
        Ok(value)
    }
}

fn lower_inline_cfg_terminator(
    builder: &mut FunctionBuilder<'_>,
    unit: &IrUnit,
    ir_function: &IrFunction,
    blocks: &[ir::Block],
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, ir::Value>,
    result_out: ir::Value,
    current_block: BlockId,
    terminator: Option<&Terminator>,
) -> Result<(), CraneliftLoweringError> {
    let Some(terminator) = terminator else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_HELPER_TERMINATOR",
            format!("block {} has no terminator", current_block.raw()),
        ));
    };
    match &terminator.kind {
        TerminatorKind::Jump { target } => {
            builder.ins().jump(cranelift_block(blocks, *target)?, &[]);
        }
        TerminatorKind::JumpIfFalse { condition, target } => {
            let condition =
                lower_inline_cfg_condition(builder, unit, locals, registers, condition)?;
            let false_block = cranelift_block(blocks, *target)?;
            let true_block =
                cranelift_block(blocks, next_ir_block_id(ir_function, current_block)?)?;
            builder
                .ins()
                .brif(condition, true_block, &[], false_block, &[]);
        }
        TerminatorKind::JumpIfTrue { condition, target } => {
            let condition =
                lower_inline_cfg_condition(builder, unit, locals, registers, condition)?;
            let true_block = cranelift_block(blocks, *target)?;
            let false_block =
                cranelift_block(blocks, next_ir_block_id(ir_function, current_block)?)?;
            builder
                .ins()
                .brif(condition, true_block, &[], false_block, &[]);
        }
        TerminatorKind::JumpIf {
            condition,
            if_true,
            if_false,
        } => {
            let condition =
                lower_inline_cfg_condition(builder, unit, locals, registers, condition)?;
            builder.ins().brif(
                condition,
                cranelift_block(blocks, *if_true)?,
                &[],
                cranelift_block(blocks, *if_false)?,
                &[],
            );
        }
        TerminatorKind::Return {
            value: Some(value),
            by_ref_local: None,
        } => {
            let value = lower_inline_cfg_operand(builder, unit, locals, registers, value)?;
            builder
                .ins()
                .store(MemFlagsData::new(), value, result_out, 0);
            let status = builder
                .ins()
                .iconst(types::I32, i64::from(JIT_HELPER_STATUS_OK));
            builder.ins().return_(&[status]);
        }
        TerminatorKind::Return {
            value: None,
            by_ref_local: _,
        } => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_RETURN",
                "inline CFG native subset requires an integer return value",
            ));
        }
        TerminatorKind::Return {
            value: Some(_),
            by_ref_local: Some(_),
        } => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_BY_REF_RETURN",
                "by-reference returns are outside inline CFG native subset",
            ));
        }
    }
    Ok(())
}

fn sanitize_symbol_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CraneliftNoExecBackend, build_trivial_add_clif_smoke, lower_function_to_cranelift,
    };
    use crate::{
        JIT_RUNTIME_ABI_HASH, JitBackend, JitBackendApi, JitBackendCompileRequest,
        JitCompileRequest, JitCompileStatus,
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
    fn cranelift_no_exec_backend_refuses_native_entry_by_default() {
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.no_exec.default");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: None,
            function: None,
            allow_native_execution: false,
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(backend.backend(), JitBackend::CraneliftExperiment);
        assert_eq!(outcome.status, JitCompileStatus::NativeExecutionDisabled);
        assert!(outcome.handle.is_none());
        assert!(outcome.diagnostics[0].contains("refused native entry"));
    }

    #[test]
    fn cranelift_backend_verifies_non_executable_clif_without_handle() {
        let (unit, function) = arithmetic_fixture();
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.no_exec.verified");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            allow_native_execution: true,
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(
            outcome.status,
            JitCompileStatus::Rejected {
                reason:
                    "cranelift backend verified CLIF but region is not in native executable subset"
                        .to_owned()
            }
        );
        assert!(outcome.handle.is_none());
        assert!(outcome.diagnostics[0].contains("verified"));
        assert!(outcome.diagnostics[0].contains("clif_bytes="));
    }

    #[test]
    fn cranelift_backend_compiles_and_invokes_constant_return_native_handle() {
        let (unit, function) = constant_return_fixture();
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.const.42");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            allow_native_execution: true,
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
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.const.lifecycle");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            allow_native_execution: true,
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
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.inline.add_mul");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            allow_native_execution: true,
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
    fn cranelift_helper_arithmetic_overflow_returns_native_status() {
        let (unit, function) = helper_overflow_fixture();
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.inline.overflow");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            allow_native_execution: true,
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });

        assert_eq!(outcome.status, JitCompileStatus::Compiled);
        let handle = outcome.handle.expect("overflow helper should compile");
        let error = handle
            .invoke_i64(&[i64::MAX], JIT_RUNTIME_ABI_HASH)
            .expect_err("checked inline arithmetic should request fallback");
        assert_eq!(error, crate::JitInvokeError::NativeStatus(2));
        assert_eq!(error.side_exit().reason, crate::SideExitReason::Overflow);
    }

    #[test]
    fn cranelift_backend_compiles_packed_array_fetch_helper_native_handle() {
        let (unit, function) = packed_array_fetch_fixture();
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.packed.fetch");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            allow_native_execution: true,
            runtime_helpers: crate::JitRuntimeHelperAddresses {
                packed_array_len: 0,
                packed_array_fetch_int_slow: test_packed_array_fetch_helper as *const () as usize,
                known_strlen: 0,
                known_count: 0,
                string_concat: 0,
                property_load: 0,
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
    fn cranelift_backend_compiles_packed_foreach_int_sum_native_loop() {
        let (unit, function) = packed_foreach_int_sum_fixture();
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.packed.foreach.sum");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            allow_native_execution: true,
            runtime_helpers: crate::JitRuntimeHelperAddresses {
                packed_array_len: test_packed_array_len_helper as *const () as usize,
                packed_array_fetch_int_slow: test_packed_array_fetch_sequence_helper as *const ()
                    as usize,
                known_strlen: 0,
                known_count: 0,
                string_concat: 0,
                property_load: 0,
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
    fn cranelift_backend_compiles_known_strlen_helper_native_handle() {
        let (unit, function) = known_strlen_fixture();
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.known.strlen");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            allow_native_execution: true,
            runtime_helpers: crate::JitRuntimeHelperAddresses {
                packed_array_len: 0,
                packed_array_fetch_int_slow: 0,
                known_strlen: test_known_strlen_helper as *const () as usize,
                known_count: 0,
                string_concat: 0,
                property_load: 0,
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
    fn cranelift_backend_compiles_string_concat_helper_native_handle() {
        let (unit, function) = string_concat_fixture();
        let mut backend = CraneliftNoExecBackend;
        let request = JitCompileRequest::new("cl.string.concat");
        let outcome = backend.compile_region(&JitBackendCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            allow_native_execution: true,
            runtime_helpers: crate::JitRuntimeHelperAddresses {
                packed_array_len: 0,
                packed_array_fetch_int_slow: 0,
                known_strlen: 0,
                known_count: 0,
                string_concat: test_string_concat_helper as *const () as usize,
                property_load: 0,
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
