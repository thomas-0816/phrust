//! Exhaustive Region IR lowering for the mandatory Cranelift native compiler.

use crate::code_manager::ManagedCompileError;
use crate::region_ir::{
    BaselineRegionBuilder, CompileMetadata, ExecutableValueFlow, NativeCompilerTier,
    RegionBinaryOp, RegionCallResult, RegionCallTarget, RegionCastOp, RegionCompareOpCode,
    RegionGraph, RegionInstruction, RegionInstructionKind, RegionNativeCall, RegionNativeControl,
    RegionNativeDynamicCode, RegionNativeSuspend, RegionOperand, RegionTerminator, RegionUnaryOp,
    SsaOwnership, SsaValueClass, value_copy_requires_retain, value_release_required,
};
use crate::{
    CraneliftCodeKey, CraneliftCompilerIdentity, JIT_RUNTIME_ABI_HASH, JitCompileRequest,
    JitCompileStatus, JitFunctionHandle, ManagedJitFunction, NativeCompileOutcome,
    NativeCompileRequest, NativeCompilerApi, global_code_manager,
};
use cranelift_codegen::binemit::Reloc;
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
use cranelift_module::{FuncId, Linkage, Module, ModuleReloc, ModuleRelocTarget};
use php_ir::{BlockId, FunctionId, IrConstant, IrSpan, IrUnit, LocalId, RegId};
use std::collections::BTreeMap;
use std::fmt;

type NativeFunctionMetadata = (String, Vec<php_ir::IrParam>, bool, usize);
use std::time::Instant;

mod call_metadata;
mod dynamic_code;
mod executable_region;
mod fallback_helpers;
mod module_layout;
mod native_linkage;
mod terminators;
mod value_lowering;

pub use module_layout::NativeCompilePlan;
use native_linkage::BASELINE_FUNCTION_SPECIALIZATION;
pub use native_linkage::{
    NativeFunctionKey, NativeFunctionTier, NativeIndirectionCell, NativeIndirectionState,
    native_function_key,
};

use call_metadata::*;
use dynamic_code::*;
use fallback_helpers::*;
use terminators::{lower_owned_frame_locals, lower_region_terminator};
use value_lowering::{encode_native_bool, lower_direct_cast, lower_direct_compare, scalar_truthy};

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeScalarRegionCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
    fast_path_hits: u64,
    has_control_flow: bool,
    plan: NativeCompilePlan,
}

#[derive(Clone, Copy, Debug)]
struct NativeHelper {
    function: FuncId,
}

#[derive(Clone, Copy, Debug)]
enum NativeDirectCallee {
    Local(FuncId),
    Resolved(FunctionId),
}

#[derive(Clone, Copy, Debug, Default)]
struct NativeOperationFunctions {
    function_resolve: Option<NativeHelper>,
    frame_alloc: Option<NativeHelper>,
    frame_release: Option<NativeHelper>,
    unary: Option<NativeHelper>,
    binary: Option<NativeHelper>,
    compare: Option<NativeHelper>,
    cast: Option<NativeHelper>,
    echo: Option<NativeHelper>,
    local_fetch: Option<NativeHelper>,
    local_store: Option<NativeHelper>,
    value_lifecycle: Option<NativeHelper>,
    reference_bind: Option<NativeHelper>,
    argument_check: Option<NativeHelper>,
    return_check: Option<NativeHelper>,
    exception_new: Option<NativeHelper>,
    array_new: Option<NativeHelper>,
    object_new: Option<NativeHelper>,
    property_fetch: Option<NativeHelper>,
    property_assign: Option<NativeHelper>,
    object_clone: Option<NativeHelper>,
    object_clone_with: Option<NativeHelper>,
    array_insert: Option<NativeHelper>,
    array_fetch: Option<NativeHelper>,
    array_unset: Option<NativeHelper>,
    array_spread: Option<NativeHelper>,
    foreach_init: Option<NativeHelper>,
    foreach_next: Option<NativeHelper>,
    foreach_cleanup: Option<NativeHelper>,
    constant_fetch: Option<NativeHelper>,
    truthy: Option<NativeHelper>,
    runtime_fatal: Option<NativeHelper>,
    execution_poll: Option<NativeHelper>,
}

fn call_native_helper(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: NativeHelper,
    arguments: &[ir::Value],
) -> ir::Inst {
    let callee = module.declare_func_in_func(helper.function, builder.func);
    builder.ins().call(callee, arguments)
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

fn native_helper_import_symbol(base: &str, address: usize) -> String {
    #[cfg(test)]
    {
        runtime_helper_symbol(base, address)
    }
    #[cfg(not(test))]
    {
        let _ = address;
        base.to_owned()
    }
}

const NATIVE_CALL_DISPATCH_SYMBOL: &str = "phrust_jit_native_call_dispatch";
const NATIVE_FUNCTION_RESOLVE_SYMBOL: &str = "phrust_jit_native_function_resolve";
const NATIVE_DYNAMIC_CODE_SYMBOL: &str = "phrust_jit_native_dynamic_code";

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
    let deployment_unit = request
        .compile
        .ir_fingerprint
        .clone()
        .unwrap_or_else(|| crate::stable_ir_fingerprint(unit));
    let declarations = unit
        .functions
        .iter()
        .enumerate()
        .filter_map(|(index, function)| {
            Some(native_function_key(
                deployment_unit.clone(),
                u32::try_from(index).ok()?,
                function.params.len(),
                function.local_count,
                request.compile.opt_level != 0,
                request.compile.invalidation_generation,
            ))
        });
    let manager = match global_code_manager() {
        Ok(manager) => manager,
        Err(error) => {
            return NativeCompileOutcome::skipped(
                JitCompileStatus::Rejected {
                    reason: "JIT_CRANELIFT_CODE_MANAGER".to_owned(),
                },
                error.to_string(),
            );
        }
    };
    if let Err(error) = manager.declare_function_cells(declarations) {
        return NativeCompileOutcome::skipped(
            JitCompileStatus::Rejected {
                reason: "JIT_CRANELIFT_DECLARE_NATIVE_UNIT".to_owned(),
            },
            error.to_string(),
        );
    }
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
    let plan = NativeCompilePlan::for_region(&region);
    if plan.function != function {
        return NativeCompileOutcome::skipped(
            JitCompileStatus::Rejected {
                reason: "JIT_CRANELIFT_REJECT_COMPILE_PLAN_ROOT".to_owned(),
            },
            format!(
                "native compile plan root {} does not match requested function {}",
                plan.function.raw(),
                function.raw()
            ),
        );
    }
    let start = Instant::now();
    match executable_region::compile_region_graph_native(
        unit,
        &region,
        plan,
        request.runtime_helpers,
        request.compile,
    ) {
        Ok(compiled) => {
            let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
            NativeCompileOutcome::compiled(
                compiled.handle,
                format!(
                    "Cranelift baseline Region IR `{}` function={} abi_hash={} code_bytes={} fast_path_hits={} control_flow={} plan_ir_instructions={} plan_php_blocks={} plan_estimated_clif_blocks={} plan_virtual_values={} plan_safepoints={} plan_live_sum={}",
                    request.compile.region_id,
                    function.raw(),
                    JIT_RUNTIME_ABI_HASH,
                    compiled.code_bytes,
                    compiled.fast_path_hits,
                    compiled.has_control_flow,
                    compiled.plan.ir_instructions,
                    compiled.plan.php_cfg_blocks,
                    compiled.plan.estimated_clif_blocks,
                    compiled.plan.virtual_values,
                    compiled.plan.safepoint_count,
                    compiled.plan.safepoint_live_set_sum,
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
        helpers.native_call_dispatch,
        helpers.native_function_resolve,
        helpers.native_dynamic_code,
        helpers.native_unary,
        helpers.native_binary,
        helpers.native_compare,
        helpers.native_cast,
        helpers.native_echo,
        helpers.native_local_fetch,
        helpers.native_local_store,
        helpers.native_value_lifecycle,
        helpers.native_reference_bind,
        helpers.native_argument_check,
        helpers.native_return_check,
        helpers.native_exception_new,
        helpers.native_array_new,
        helpers.native_object_new,
        helpers.native_property_fetch,
        helpers.native_property_assign,
        helpers.native_object_clone,
        helpers.native_object_clone_with,
        helpers.native_array_insert,
        helpers.native_array_fetch,
        helpers.native_array_unset,
        helpers.native_array_spread,
        helpers.native_foreach_init,
        helpers.native_foreach_next,
        helpers.native_foreach_cleanup,
        helpers.native_constant_fetch,
        helpers.native_truthy,
        helpers.native_runtime_fatal,
        helpers.native_execution_poll,
    ] {
        for byte in address.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
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
    registers: &BTreeMap<RegId, Variable>,
    operand: RegionOperand,
) -> Result<ir::Value, CraneliftLoweringError> {
    match operand {
        RegionOperand::Register(reg) => {
            let variable = registers.get(&reg).copied().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
                    format!("register {} has not been lowered in this block", reg.raw()),
                )
            })?;
            builder.try_use_var(variable).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
                    format!("register {} has no native value: {error}", reg.raw()),
                )
            })
        }
        RegionOperand::I64(value) => Ok(builder.ins().iconst(types::I64, value)),
        RegionOperand::Constant(constant) => Ok(builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(constant))),
        RegionOperand::Local(local) => use_local_variable(builder, locals, local),
    }
}

fn lower_ir_operand(
    builder: &mut FunctionBuilder<'_>,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, Variable>,
    operand: php_ir::Operand,
) -> Result<ir::Value, CraneliftLoweringError> {
    match operand {
        php_ir::Operand::Register(register) => {
            let variable = registers.get(&register).copied().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
                    format!("source operand register {} is unavailable", register.raw()),
                )
            })?;
            builder.try_use_var(variable).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
                    format!(
                        "source operand register {} has no native value: {error}",
                        register.raw()
                    ),
                )
            })
        }
        php_ir::Operand::Local(local) => use_local_variable(builder, locals, local),
        php_ir::Operand::Constant(constant) => Ok(builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(constant.raw()))),
    }
}

fn define_region_register(
    builder: &mut FunctionBuilder<'_>,
    register_variables: &BTreeMap<RegId, Variable>,
    registers: &mut BTreeMap<RegId, Variable>,
    register: RegId,
    value: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    let variable = register_variables.get(&register).copied().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
            format!(
                "register {} has no declared native variable",
                register.raw()
            ),
        )
    })?;
    builder.def_var(variable, value);
    registers.insert(register, variable);
    Ok(())
}

fn require_native_operation_ok(
    builder: &mut FunctionBuilder<'_>,
    status: ir::Value,
    result_out: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    let ok = builder.create_block();
    let failed = builder.create_block();
    let is_ok = builder.ins().icmp_imm(IntCC::Equal, status, 0);
    builder.ins().brif(is_ok, ok, &[], failed, &[]);
    builder.switch_to_block(failed);
    let empty = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), empty, result_out, 0);
    builder.ins().return_(&[status]);
    builder.switch_to_block(ok);
    Ok(())
}

fn allocate_native_frame_storage(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operations: NativeOperationFunctions,
    bytes: u32,
    alignment_log2: u8,
    result_out: ir::Value,
) -> ir::Value {
    let pointer_type = module.target_config().pointer_type();
    let Some(helper) = operations.frame_alloc else {
        let slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            bytes,
            alignment_log2,
        ));
        return builder.ins().stack_addr(pointer_type, slot, 0);
    };
    let context = builder.ins().iconst(types::I64, 0);
    let bytes = builder.ins().iconst(types::I64, i64::from(bytes));
    let alignment = builder.ins().iconst(types::I64, 1_i64 << alignment_log2);
    let call = call_native_helper(module, builder, helper, &[context, bytes, alignment]);
    let pointer = builder.inst_results(call)[0];
    let allocated = builder.create_block();
    let failed = builder.create_block();
    let non_null = builder.ins().icmp_imm(IntCC::NotEqual, pointer, 0);
    builder.ins().brif(non_null, allocated, &[], failed, &[]);
    builder.switch_to_block(failed);
    let empty = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), empty, result_out, 0);
    let status = builder
        .ins()
        .iconst(types::I32, i64::from(crate::JitCallStatus::RUNTIME_ERROR.0));
    builder.ins().return_(&[status]);
    builder.switch_to_block(allocated);
    pointer
}

fn release_native_frame_storage(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operations: NativeOperationFunctions,
    pointer: ir::Value,
    result_out: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    let Some(helper) = operations.frame_release else {
        return Ok(());
    };
    let context = builder.ins().iconst(types::I64, 0);
    let call = call_native_helper(module, builder, helper, &[context, pointer]);
    require_native_operation_ok(builder, builder.inst_results(call)[0], result_out)
}

fn require_native_value_operation_ok(
    builder: &mut FunctionBuilder<'_>,
    status: ir::Value,
    result_out: ir::Value,
    value: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    let ok = builder.create_block();
    let failed = builder.create_block();
    let is_ok = builder.ins().icmp_imm(IntCC::Equal, status, 0);
    builder.ins().brif(is_ok, ok, &[], failed, &[]);
    builder.switch_to_block(failed);
    builder
        .ins()
        .store(MemFlagsData::new(), value, result_out, 0);
    builder.ins().return_(&[status]);
    builder.switch_to_block(ok);
    Ok(())
}

fn publish_native_local_masks(
    builder: &mut FunctionBuilder<'_>,
    state_out: ir::Value,
    live_locals: &[LocalId],
) {
    let mut masks = [0_u64; crate::JIT_DEOPT_LOCAL_MASK_WORDS];
    for local in live_locals {
        let index = local.index();
        if index < crate::JIT_DEOPT_MAX_SLOTS {
            masks[index / u64::BITS as usize] |= 1_u64 << (index % u64::BITS as usize);
        }
    }
    let initialized = builder.ins().iconst(types::I64, masks[0] as i64);
    builder.ins().store(
        MemFlagsData::new(),
        initialized,
        state_out,
        std::mem::offset_of!(crate::JitDeoptState, initialized_mask) as i32,
    );
    let high_base = std::mem::offset_of!(crate::JitDeoptState, initialized_masks_high);
    for (index, mask) in masks[1..].iter().enumerate() {
        let initialized = builder.ins().iconst(types::I64, *mask as i64);
        builder.ins().store(
            MemFlagsData::new(),
            initialized,
            state_out,
            high_base.saturating_add(index.saturating_mul(8)) as i32,
        );
    }
}

fn publish_native_call_state(
    builder: &mut FunctionBuilder<'_>,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    instruction: &RegionInstruction,
    locals: &BTreeMap<LocalId, Variable>,
    native_version: u32,
) -> Result<(), CraneliftLoweringError> {
    let store_i32 = |builder: &mut FunctionBuilder<'_>, offset: usize, value: u32| {
        let value = builder.ins().iconst(types::I32, i64::from(value));
        builder
            .ins()
            .store(MemFlagsData::new(), value, deopt_out, offset as i32);
    };
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitDeoptState, function_id),
        function.raw(),
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitDeoptState, continuation_id),
        instruction.continuation_id,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitDeoptState, slot_count),
        local_count,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitDeoptState, native_version),
        native_version,
    );
    publish_native_local_masks(builder, deopt_out, &instruction.live_locals);
    for local in &instruction.live_locals {
        let value = use_local_variable(builder, locals, *local)?;
        let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
            .saturating_add(local.index().saturating_mul(8));
        builder
            .ins()
            .store(MemFlagsData::new(), value, deopt_out, offset as i32);
    }
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().store(
        MemFlagsData::new(),
        zero,
        deopt_out,
        std::mem::offset_of!(crate::JitDeoptState, initialized_register_mask) as i32,
    );
    Ok(())
}

fn publish_native_register_state(
    builder: &mut FunctionBuilder<'_>,
    state_out: ir::Value,
    registers: &BTreeMap<RegId, Variable>,
) {
    let encodable_registers = registers
        .iter()
        .filter(|(register, _)| register.index() < crate::JIT_DEOPT_MAX_REGISTERS);
    let initialized_mask = encodable_registers
        .clone()
        .fold(0_u64, |mask, (register, _)| {
            mask | 1_u64.checked_shl(register.raw()).unwrap_or(0)
        });
    let initialized = builder.ins().iconst(types::I64, initialized_mask as i64);
    builder.ins().store(
        MemFlagsData::new(),
        initialized,
        state_out,
        std::mem::offset_of!(crate::JitDeoptState, initialized_register_mask) as i32,
    );
    for (register, variable) in encodable_registers {
        let value = builder.use_var(*variable);
        let value = if builder.func.dfg.value_type(value) == types::I64 {
            value
        } else {
            builder.ins().uextend(types::I64, value)
        };
        let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
            .saturating_add(register.index().saturating_mul(8));
        builder
            .ins()
            .store(MemFlagsData::new(), value, state_out, offset as i32);
    }
}

fn lower_native_value_operation(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    opcode: u32,
    operands: &[ir::Value],
    result_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    let helper = helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
            format!("native value operation {opcode} has no declared helper"),
        )
    })?;
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let pointer_type = module.target_config().pointer_type();
    let out = builder.ins().stack_addr(pointer_type, slot, 0);
    let context = builder.ins().iconst(types::I64, 0);
    let opcode = builder.ins().iconst(types::I32, i64::from(opcode));
    let mut args = Vec::with_capacity(operands.len() + 3);
    args.extend([context, opcode]);
    args.extend_from_slice(operands);
    args.push(out);
    let call = call_native_helper(module, builder, helper, &args);
    let value = builder.ins().stack_load(types::I64, slot, 0);
    require_native_value_operation_ok(builder, builder.inst_results(call)[0], result_out, value)?;
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
fn lower_native_value_operation_with_state(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    opcode: u32,
    operands: &[ir::Value],
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    instruction: &RegionInstruction,
    locals: &BTreeMap<LocalId, Variable>,
    native_version: u32,
) -> Result<ir::Value, CraneliftLoweringError> {
    let helper = helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
            format!("native value operation {opcode} has no declared helper"),
        )
    })?;
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let pointer_type = module.target_config().pointer_type();
    let out = builder.ins().stack_addr(pointer_type, slot, 0);
    let context = builder.ins().iconst(types::I64, 0);
    let opcode = builder.ins().iconst(types::I32, i64::from(opcode));
    let mut args = Vec::with_capacity(operands.len() + 3);
    args.extend([context, opcode]);
    args.extend_from_slice(operands);
    args.push(out);
    let call = call_native_helper(module, builder, helper, &args);
    let status = builder.inst_results(call)[0];
    let value = builder.ins().stack_load(types::I64, slot, 0);
    let ok = builder.create_block();
    let failed = builder.create_block();
    let is_ok = builder.ins().icmp_imm(IntCC::Equal, status, 0);
    builder.ins().brif(is_ok, ok, &[], failed, &[]);
    builder.switch_to_block(failed);
    publish_native_call_state(
        builder,
        deopt_out,
        function,
        local_count,
        instruction,
        locals,
        native_version,
    )?;
    builder
        .ins()
        .store(MemFlagsData::new(), value, result_out, 0);
    builder.ins().return_(&[status]);
    builder.switch_to_block(ok);
    Ok(value)
}

fn publish_native_reference_local(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    value: ir::Value,
    function: FunctionId,
    local: LocalId,
    result_out: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
    let local = builder.ins().iconst(types::I64, i64::from(local.raw()));
    let _ = lower_native_value_operation(
        module,
        builder,
        helper,
        4,
        &[value, function, local],
        result_out,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_direct_reference_argument(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, Variable>,
    argument: &php_ir::instruction::IrCallArg,
    argument_index: usize,
    fallback_value: ir::Value,
    source_block: BlockId,
    instruction: &RegionInstruction,
    function: FunctionId,
    result_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if let Some(local) = argument.by_ref_local {
        let value = use_local_variable(builder, locals, local)?;
        let zero = builder.ins().iconst(types::I64, 0);
        let reference = lower_native_value_operation(
            module,
            builder,
            helper,
            0,
            &[value, zero, zero],
            result_out,
        )?;
        builder.def_var(local_variable(locals, local)?, reference);
        publish_native_reference_local(
            module, builder, helper, reference, function, local, result_out,
        )?;
        return Ok(reference);
    }
    if let Some(target) = &argument.by_ref_dim {
        let root = use_local_variable(builder, locals, target.local)?;
        let mut reference = root;
        for dimension in &target.dims {
            let key = lower_ir_operand(builder, locals, registers, *dimension)?;
            let zero = builder.ins().iconst(types::I64, 0);
            reference = lower_native_value_operation(
                module,
                builder,
                helper,
                native_dim_operation(1, function, instruction.continuation_id),
                &[reference, key, zero],
                result_out,
            )?;
        }
        publish_native_reference_local(
            module,
            builder,
            helper,
            root,
            function,
            target.local,
            result_out,
        )?;
        return Ok(reference);
    }
    if let Some(target) = &argument.by_ref_property {
        let object = lower_ir_operand(builder, locals, registers, target.object)?;
        let function_and_argument = u64::from(function.raw())
            | (u64::from(
                u32::try_from(argument_index)
                    .unwrap_or(u32::MAX - 1)
                    .saturating_add(1),
            ) << 32);
        let function_value = builder
            .ins()
            .iconst(types::I64, function_and_argument as i64);
        let locator = builder.ins().iconst(
            types::I64,
            native_instruction_locator(source_block, instruction.id),
        );
        return lower_native_value_operation(
            module,
            builder,
            helper,
            3,
            &[object, function_value, locator],
            result_out,
        );
    }
    if let Some(target) = &argument.by_ref_property_dim {
        let object = lower_ir_operand(builder, locals, registers, target.object)?;
        let function_and_argument = u64::from(function.raw())
            | (u64::from(
                u32::try_from(argument_index)
                    .unwrap_or(u32::MAX - 1)
                    .saturating_add(1),
            ) << 32);
        let function_value = builder
            .ins()
            .iconst(types::I64, function_and_argument as i64);
        let locator = builder.ins().iconst(
            types::I64,
            native_instruction_locator(source_block, instruction.id),
        );
        let mut reference = lower_native_value_operation(
            module,
            builder,
            helper,
            3,
            &[object, function_value, locator],
            result_out,
        )?;
        for dimension in &target.dims {
            let key = lower_ir_operand(builder, locals, registers, *dimension)?;
            let zero = builder.ins().iconst(types::I64, 0);
            reference = lower_native_value_operation(
                module,
                builder,
                helper,
                native_dim_operation(1, function, instruction.continuation_id),
                &[reference, key, zero],
                result_out,
            )?;
        }
        return Ok(reference);
    }
    let zero = builder.ins().iconst(types::I64, 0);
    lower_native_value_operation(
        module,
        builder,
        helper,
        0,
        &[fallback_value, zero, zero],
        result_out,
    )
}

#[allow(clippy::too_many_arguments)]
fn lower_native_binary_operation(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    opcode: u32,
    lhs: ir::Value,
    rhs: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    instruction: &RegionInstruction,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, Variable>,
    native_version: u32,
) -> Result<ir::Value, CraneliftLoweringError> {
    let helper = helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
            format!("native binary operation {opcode} has no declared helper"),
        )
    })?;
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let pointer_type = module.target_config().pointer_type();
    let out = builder.ins().stack_addr(pointer_type, slot, 0);
    let context = builder.ins().iconst(types::I64, 0);
    let opcode = builder.ins().iconst(types::I32, i64::from(opcode));
    let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
    let continuation = builder
        .ins()
        .iconst(types::I64, i64::from(instruction.continuation_id));
    let call = call_native_helper(
        module,
        builder,
        helper,
        &[context, opcode, lhs, rhs, function_value, continuation, out],
    );
    let status = builder.inst_results(call)[0];
    let value = builder.ins().stack_load(types::I64, slot, 0);
    let ok = builder.create_block();
    let failed = builder.create_block();
    let is_ok = builder.ins().icmp_imm(IntCC::Equal, status, 0);
    builder.ins().brif(is_ok, ok, &[], failed, &[]);
    builder.switch_to_block(failed);
    publish_native_call_state(
        builder,
        deopt_out,
        function,
        local_count,
        instruction,
        locals,
        native_version,
    )?;
    publish_native_register_state(builder, deopt_out, registers);
    builder
        .ins()
        .store(MemFlagsData::new(), value, result_out, 0);
    builder.ins().return_(&[status]);
    builder.switch_to_block(ok);
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
fn lower_native_local_fetch(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    value: ir::Value,
    quiet: bool,
    function: FunctionId,
    local: LocalId,
    span: IrSpan,
    result_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
    let local = builder.ins().iconst(types::I64, i64::from(local.raw()));
    let file = builder.ins().iconst(types::I64, i64::from(span.file.raw()));
    let start = builder.ins().iconst(types::I64, i64::from(span.start));
    lower_native_value_operation(
        module,
        builder,
        helper,
        u32::from(quiet),
        &[value, function, local, file, start],
        result_out,
    )
}

const fn native_unary_opcode(op: RegionUnaryOp) -> u32 {
    match op {
        RegionUnaryOp::Plus => 0,
        RegionUnaryOp::Minus => 1,
        RegionUnaryOp::Not => 2,
        RegionUnaryOp::BitNot => 3,
    }
}

const fn native_binary_opcode(op: RegionBinaryOp) -> u32 {
    match op {
        RegionBinaryOp::Add => 0,
        RegionBinaryOp::Sub => 1,
        RegionBinaryOp::Mul => 2,
        RegionBinaryOp::Div => 3,
        RegionBinaryOp::Mod => 4,
        RegionBinaryOp::Concat => 5,
        RegionBinaryOp::Pow => 6,
        RegionBinaryOp::BitAnd => 7,
        RegionBinaryOp::BitOr => 8,
        RegionBinaryOp::BitXor => 9,
        RegionBinaryOp::ShiftLeft => 10,
        RegionBinaryOp::ShiftRight => 11,
    }
}

const fn native_compare_opcode(op: RegionCompareOpCode) -> u32 {
    match op {
        RegionCompareOpCode::Equal => 0,
        RegionCompareOpCode::NotEqual => 1,
        RegionCompareOpCode::Identical => 2,
        RegionCompareOpCode::NotIdentical => 3,
        RegionCompareOpCode::Less => 4,
        RegionCompareOpCode::LessEqual => 5,
        RegionCompareOpCode::Greater => 6,
        RegionCompareOpCode::GreaterEqual => 7,
        RegionCompareOpCode::Spaceship => 8,
    }
}

const fn native_cast_opcode(op: RegionCastOp) -> u32 {
    match op {
        RegionCastOp::Bool => 0,
        RegionCastOp::Int => 1,
        RegionCastOp::Float => 2,
        RegionCastOp::String => 3,
        RegionCastOp::Array => 4,
        RegionCastOp::Object => 5,
        RegionCastOp::Void => 6,
    }
}

const fn native_dim_operation(low_bits: u32, function: FunctionId, continuation: u32) -> u32 {
    if function.raw() <= 0x3ff && continuation < 0x0f_ffff {
        0x8000_0000 | low_bits | (function.raw() << 1) | (continuation << 11)
    } else {
        low_bits
    }
}

const fn native_frame_cleanup_operation(function: FunctionId) -> u32 {
    if function.raw() <= 0x3ff {
        0x8000_0001 | (function.raw() << 1) | (0x0f_ffff << 11)
    } else {
        1
    }
}

const fn native_instruction_locator(block: BlockId, instruction: php_ir::InstrId) -> i64 {
    ((block.raw() as u64) << 32 | instruction.raw() as u64) as i64
}

#[allow(clippy::too_many_arguments)]
fn lower_region_instruction(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    functions: &BTreeMap<FunctionId, FuncId>,
    inline_constants: &BTreeMap<FunctionId, RegionOperand>,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    native_call_helper: Option<NativeHelper>,
    native_dynamic_code_helper: Option<NativeHelper>,
    native_operations: NativeOperationFunctions,
    register_variables: &BTreeMap<RegId, Variable>,
    blocks: &[ir::Block],
    suspension_blocks: &BTreeMap<u32, ir::Block>,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &mut BTreeMap<RegId, Variable>,
    source_block: BlockId,
    instruction: &RegionInstruction,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
    result_out: ir::Value,
    deopt_out: ir::Value,
    resume_state: ir::Value,
    pending_status: Variable,
    pending_value: Variable,
    function: FunctionId,
    local_count: u32,
    native_version: u32,
    pointer_type: ir::Type,
) -> Result<(), CraneliftLoweringError> {
    match &instruction.kind {
        RegionInstructionKind::Nop => {}
        RegionInstructionKind::Move { dst, src } => {
            let cl_value = lower_region_operand(builder, locals, registers, *src)?;
            let fact = value_flow.operand_fact(constants, *src);
            let cl_value = if value_copy_requires_retain(fact) {
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.value_lifecycle,
                    native_dim_operation(0, function, instruction.continuation_id),
                    &[cl_value],
                    result_out,
                )?
            } else {
                cl_value
            };
            define_region_register(builder, register_variables, registers, *dst, cl_value)?;
        }
        RegionInstructionKind::LoadLocal { dst, local, quiet } => {
            let value = use_local_variable(builder, locals, *local)?;
            let fact = value_flow.local_fact(*local);
            let direct = value_flow.local_storage(*local).is_promoted()
                && instruction.live_locals.contains(local)
                && (!value_copy_requires_retain(fact)
                    || value_flow.can_borrow_local_load(instruction.continuation_id));
            let value = if direct {
                value
            } else {
                lower_native_local_fetch(
                    module,
                    builder,
                    native_operations.local_fetch,
                    value,
                    *quiet,
                    function,
                    *local,
                    instruction.span,
                    result_out,
                )?
            };
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::StoreLocal { local, src } => {
            let current = use_local_variable(builder, locals, *local)?;
            let src_operand = *src;
            let src = lower_region_operand(builder, locals, registers, src_operand)?;
            let fact = value_flow.operand_fact(constants, src_operand);
            let direct = value_flow.local_storage(*local).is_promoted()
                && fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && !matches!(
                    fact.class,
                    SsaValueClass::ReferenceHandle | SsaValueClass::MixedHandle
                );
            let cl_value = if direct {
                let stored = if value_copy_requires_retain(fact)
                    && !value_flow.moves_value_into_local(instruction.continuation_id)
                {
                    lower_native_value_operation(
                        module,
                        builder,
                        native_operations.value_lifecycle,
                        native_dim_operation(0, function, instruction.continuation_id),
                        &[src],
                        result_out,
                    )?
                } else {
                    src
                };
                let current_fact = value_flow.local_fact(*local);
                if instruction.live_locals.contains(local)
                    && (current_fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || value_release_required(current_fact))
                {
                    let _ = lower_native_value_operation(
                        module,
                        builder,
                        native_operations.value_lifecycle,
                        native_dim_operation(1, function, instruction.continuation_id),
                        &[current],
                        result_out,
                    )?;
                }
                stored
            } else {
                let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
                let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.local_store,
                    0,
                    &[current, src, function_value, local_value],
                    result_out,
                )?
            };
            let variable = local_variable(locals, *local)?;
            builder.def_var(variable, cl_value);
        }
        RegionInstructionKind::AssignLocalResult { dst, local, value } => {
            let current = use_local_variable(builder, locals, *local)?;
            let value_operand = *value;
            let value = lower_region_operand(builder, locals, registers, value_operand)?;
            let fact = value_flow.operand_fact(constants, value_operand);
            let direct =
                value_flow.local_storage(*local).is_promoted() && !value_copy_requires_retain(fact);
            let stored = if direct {
                value
            } else {
                let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
                let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.local_store,
                    0,
                    &[current, value, function_value, local_value],
                    result_out,
                )?
            };
            builder.def_var(local_variable(locals, *local)?, stored);
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::BindReference { target, source } => {
            let source_value = use_local_variable(builder, locals, *source)?;
            let zero = builder.ins().iconst(types::I64, 0);
            let reference = lower_native_value_operation(
                module,
                builder,
                native_operations.reference_bind,
                0,
                &[source_value, zero, zero],
                result_out,
            )?;
            builder.def_var(local_variable(locals, *source)?, reference);
            builder.def_var(local_variable(locals, *target)?, reference);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                reference,
                function,
                *source,
                result_out,
            )?;
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                reference,
                function,
                *target,
                result_out,
            )?;
        }
        RegionInstructionKind::BindReferenceDim {
            target,
            array,
            keys,
        } => {
            let current = use_local_variable(builder, locals, *array)?;
            let root = lower_native_local_fetch(
                module,
                builder,
                native_operations.local_fetch,
                current,
                false,
                function,
                *array,
                instruction.span,
                result_out,
            )?;
            let keys = keys
                .iter()
                .map(|key| lower_region_operand(builder, locals, registers, *key))
                .collect::<Result<Vec<_>, _>>()?;
            let mut arrays = Vec::with_capacity(keys.len());
            arrays.push(root);
            let mut nested = root;
            for key in keys.iter().take(keys.len().saturating_sub(1)) {
                nested = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[nested, *key],
                    result_out,
                )?;
                arrays.push(nested);
            }
            let zero = builder.ins().iconst(types::I64, 0);
            let reference = lower_native_value_operation(
                module,
                builder,
                native_operations.reference_bind,
                native_dim_operation(1, function, instruction.continuation_id),
                &[
                    nested,
                    *keys
                        .last()
                        .expect("reference-from-dimension retains at least one key"),
                    zero,
                ],
                result_out,
            )?;
            let mut updated = nested;
            for index in (0..keys.len().saturating_sub(1)).rev() {
                updated = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_insert,
                    native_dim_operation(0, function, instruction.continuation_id),
                    &[arrays[index], keys[index], updated],
                    result_out,
                )?;
            }
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let local_value = builder.ins().iconst(types::I64, i64::from(array.raw()));
            let stored = lower_native_value_operation(
                module,
                builder,
                native_operations.local_store,
                0,
                &[current, updated, function_value, local_value],
                result_out,
            )?;
            builder.def_var(local_variable(locals, *array)?, stored);
            builder.def_var(local_variable(locals, *target)?, reference);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                stored,
                function,
                *array,
                result_out,
            )?;
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                reference,
                function,
                *target,
                result_out,
            )?;
        }
        RegionInstructionKind::BindReferenceIntoDim {
            array,
            keys,
            append,
            source,
        } => {
            let source_value = use_local_variable(builder, locals, *source)?;
            let zero = builder.ins().iconst(types::I64, 0);
            let reference = lower_native_value_operation(
                module,
                builder,
                native_operations.reference_bind,
                0,
                &[source_value, zero, zero],
                result_out,
            )?;
            builder.def_var(local_variable(locals, *source)?, reference);
            let root = use_local_variable(builder, locals, *array)?;
            let keys = keys
                .iter()
                .map(|key| lower_region_operand(builder, locals, registers, *key))
                .collect::<Result<Vec<_>, _>>()?;
            let mut arrays = Vec::with_capacity(keys.len());
            arrays.push(root);
            let mut nested = root;
            let parent_key_count = if *append {
                keys.len()
            } else {
                keys.len().saturating_sub(1)
            };
            for key in keys.iter().take(parent_key_count) {
                nested = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[nested, *key],
                    result_out,
                )?;
                arrays.push(nested);
            }
            let target_key = if *append {
                builder
                    .ins()
                    .iconst(types::I64, crate::jit_encode_constant(u32::MAX))
            } else {
                *keys
                    .last()
                    .expect("non-append reference binding retains a dimension")
            };
            let mut updated = lower_native_value_operation(
                module,
                builder,
                native_operations.array_insert,
                native_dim_operation(u32::from(*append), function, instruction.continuation_id),
                &[nested, target_key, reference],
                result_out,
            )?;
            for index in (0..parent_key_count).rev() {
                updated = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_insert,
                    native_dim_operation(0, function, instruction.continuation_id),
                    &[arrays[index], keys[index], updated],
                    result_out,
                )?;
            }
            builder.def_var(local_variable(locals, *array)?, updated);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                updated,
                function,
                *array,
                result_out,
            )?;
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                reference,
                function,
                *source,
                result_out,
            )?;
        }
        RegionInstructionKind::BindReferenceProperty { object, source } => {
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let source_value = use_local_variable(builder, locals, *source)?;
            let zero = builder.ins().iconst(types::I64, 0);
            let reference = lower_native_value_operation(
                module,
                builder,
                native_operations.reference_bind,
                0,
                &[source_value, zero, zero],
                result_out,
            )?;
            builder.def_var(local_variable(locals, *source)?, reference);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                reference,
                function,
                *source,
                result_out,
            )?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let _ = lower_native_value_operation(
                module,
                builder,
                native_operations.property_assign,
                1,
                &[object, reference, function_value, instruction_id],
                result_out,
            )?;
        }
        RegionInstructionKind::BindReferenceFromProperty { target, object } => {
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let reference = lower_native_value_operation(
                module,
                builder,
                native_operations.reference_bind,
                3,
                &[object, function_value, instruction_id],
                result_out,
            )?;
            builder.def_var(local_variable(locals, *target)?, reference);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                reference,
                function,
                *target,
                result_out,
            )?;
        }
        RegionInstructionKind::BindReferenceFromPropertyDim {
            target,
            object,
            keys,
        } => {
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let mut reference = lower_native_value_operation(
                module,
                builder,
                native_operations.reference_bind,
                3,
                &[object, function_value, instruction_id],
                result_out,
            )?;
            for key in keys {
                let key = lower_region_operand(builder, locals, registers, *key)?;
                let zero = builder.ins().iconst(types::I64, 0);
                reference = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.reference_bind,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[reference, key, zero],
                    result_out,
                )?;
            }
            builder.def_var(local_variable(locals, *target)?, reference);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                reference,
                function,
                *target,
                result_out,
            )?;
        }
        RegionInstructionKind::BindReferenceIntoPropertyDim {
            object,
            keys,
            append,
            source,
        } => {
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let source_value = use_local_variable(builder, locals, *source)?;
            let zero = builder.ins().iconst(types::I64, 0);
            let reference = lower_native_value_operation(
                module,
                builder,
                native_operations.reference_bind,
                0,
                &[source_value, zero, zero],
                result_out,
            )?;
            builder.def_var(local_variable(locals, *source)?, reference);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                reference,
                function,
                *source,
                result_out,
            )?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let root = lower_native_value_operation(
                module,
                builder,
                native_operations.property_fetch,
                0,
                &[object, function_value, instruction_id],
                result_out,
            )?;
            let keys = keys
                .iter()
                .map(|key| lower_region_operand(builder, locals, registers, *key))
                .collect::<Result<Vec<_>, _>>()?;
            let parent_key_count = if *append {
                keys.len()
            } else {
                keys.len().saturating_sub(1)
            };
            let mut arrays = Vec::with_capacity(parent_key_count.saturating_add(1));
            arrays.push(root);
            let mut nested = root;
            for key in keys.iter().take(parent_key_count) {
                nested = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[nested, *key],
                    result_out,
                )?;
                arrays.push(nested);
            }
            let target_key = if *append {
                builder
                    .ins()
                    .iconst(types::I64, crate::jit_encode_constant(u32::MAX))
            } else {
                *keys.last().ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_EMPTY_PROPERTY_DIM_REFERENCE",
                        "property-dimension reference binding has no target dimension",
                    )
                })?
            };
            let mut updated = lower_native_value_operation(
                module,
                builder,
                native_operations.array_insert,
                native_dim_operation(u32::from(*append), function, instruction.continuation_id),
                &[nested, target_key, reference],
                result_out,
            )?;
            for index in (0..parent_key_count).rev() {
                updated = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_insert,
                    native_dim_operation(0, function, instruction.continuation_id),
                    &[arrays[index], keys[index], updated],
                    result_out,
                )?;
            }
            let _ = lower_native_value_operation(
                module,
                builder,
                native_operations.property_assign,
                0,
                &[object, updated, function_value, instruction_id],
                result_out,
            )?;
        }
        RegionInstructionKind::BindReferenceDimFromProperty {
            array,
            keys,
            append,
            object,
        } => {
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            if keys.is_empty() && !*append {
                // `$local =& $object->property` must reuse an existing
                // property reference. Fetching the dereferenced value,
                // wrapping it, then assigning through the old reference
                // creates Reference(Reference(...)) on every repeated bind.
                let reference = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.reference_bind,
                    3,
                    &[object, function_value, instruction_id],
                    result_out,
                )?;
                builder.def_var(local_variable(locals, *array)?, reference);
                publish_native_reference_local(
                    module,
                    builder,
                    native_operations.reference_bind,
                    reference,
                    function,
                    *array,
                    result_out,
                )?;
            } else {
                let property = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.property_fetch,
                    0,
                    &[object, function_value, instruction_id],
                    result_out,
                )?;
                let zero = builder.ins().iconst(types::I64, 0);
                let reference = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.reference_bind,
                    0,
                    &[property, zero, zero],
                    result_out,
                )?;
                let _ = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.property_assign,
                    1,
                    &[object, reference, function_value, instruction_id],
                    result_out,
                )?;
                let root = use_local_variable(builder, locals, *array)?;
                let keys = keys
                    .iter()
                    .map(|key| lower_region_operand(builder, locals, registers, *key))
                    .collect::<Result<Vec<_>, _>>()?;
                let parent_key_count = if *append {
                    keys.len()
                } else {
                    keys.len().saturating_sub(1)
                };
                let mut arrays = Vec::with_capacity(parent_key_count);
                arrays.push(root);
                let mut nested = root;
                for key in keys.iter().take(parent_key_count) {
                    nested = lower_native_value_operation(
                        module,
                        builder,
                        native_operations.array_fetch,
                        native_dim_operation(1, function, instruction.continuation_id),
                        &[nested, *key],
                        result_out,
                    )?;
                    arrays.push(nested);
                }
                let target_key = if *append {
                    builder
                        .ins()
                        .iconst(types::I64, crate::jit_encode_constant(u32::MAX))
                } else {
                    keys[keys.len() - 1]
                };
                let mut updated = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_insert,
                    native_dim_operation(u32::from(*append), function, instruction.continuation_id),
                    &[nested, target_key, reference],
                    result_out,
                )?;
                for index in (0..parent_key_count).rev() {
                    updated = lower_native_value_operation(
                        module,
                        builder,
                        native_operations.array_insert,
                        native_dim_operation(0, function, instruction.continuation_id),
                        &[arrays[index], keys[index], updated],
                        result_out,
                    )?;
                }
                builder.def_var(local_variable(locals, *array)?, updated);
                publish_native_reference_local(
                    module,
                    builder,
                    native_operations.reference_bind,
                    updated,
                    function,
                    *array,
                    result_out,
                )?;
            }
        }
        RegionInstructionKind::BindReferenceStaticProperty { source } => {
            publish_native_call_state(
                builder,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            publish_native_register_state(builder, deopt_out, registers);
            let source_value = use_local_variable(builder, locals, *source)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let reference = lower_native_value_operation(
                module,
                builder,
                native_operations.reference_bind,
                5,
                &[source_value, function_value, instruction_id],
                result_out,
            )?;
            builder.def_var(local_variable(locals, *source)?, reference);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                reference,
                function,
                *source,
                result_out,
            )?;
        }
        RegionInstructionKind::InitStaticLocal { local, default } => {
            let default = lower_region_operand(builder, locals, registers, *default)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
            let reference = lower_native_value_operation(
                module,
                builder,
                native_operations.reference_bind,
                2,
                &[default, function_value, local_value],
                result_out,
            )?;
            builder.def_var(local_variable(locals, *local)?, reference);
        }
        RegionInstructionKind::Discard { src } => {
            if value_flow.elides_discard(instruction.continuation_id) {
                return Ok(());
            }
            let value = lower_region_operand(builder, locals, registers, *src)?;
            let fact = value_flow.operand_fact(constants, *src);
            if value_release_required(fact) {
                let _ = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.value_lifecycle,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[value],
                    result_out,
                )?;
            }
        }
        RegionInstructionKind::Binary { dst, op, lhs, rhs } => {
            let lhs_operand = *lhs;
            let rhs_operand = *rhs;
            let lhs = lower_region_operand(builder, locals, registers, lhs_operand)?;
            let rhs = lower_region_operand(builder, locals, registers, rhs_operand)?;
            let lhs_fact = value_flow.operand_fact(constants, lhs_operand);
            let rhs_fact = value_flow.operand_fact(constants, rhs_operand);
            let direct_int = lhs_fact.class == SsaValueClass::Int
                && rhs_fact.class == SsaValueClass::Int
                && lhs_fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && rhs_fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && matches!(
                    op,
                    RegionBinaryOp::Add
                        | RegionBinaryOp::Sub
                        | RegionBinaryOp::Mul
                        | RegionBinaryOp::BitAnd
                        | RegionBinaryOp::BitOr
                        | RegionBinaryOp::BitXor
                        | RegionBinaryOp::ShiftLeft
                        | RegionBinaryOp::ShiftRight
                );
            let cl_value = if direct_int {
                lower_checked_region_binary(
                    module,
                    builder,
                    native_operations.binary,
                    *op,
                    lhs,
                    rhs,
                    result_out,
                    deopt_out,
                    function,
                    local_count,
                    instruction,
                    locals,
                    registers,
                    native_version,
                )?
            } else if native_operations.binary.is_some() {
                lower_native_binary_operation(
                    module,
                    builder,
                    native_operations.binary,
                    native_binary_opcode(*op),
                    lhs,
                    rhs,
                    result_out,
                    deopt_out,
                    function,
                    local_count,
                    instruction,
                    locals,
                    registers,
                    native_version,
                )?
            } else {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                    format!("binary operation {op:?} has no typed runtime helper"),
                ));
            };
            define_region_register(builder, register_variables, registers, *dst, cl_value)?;
        }
        RegionInstructionKind::Unary { dst, op, src } => {
            let src_operand = *src;
            let src = lower_region_operand(builder, locals, registers, src_operand)?;
            let fact = value_flow.operand_fact(constants, src_operand);
            let unary_operation =
                if function.raw() <= 0x03ff && instruction.continuation_id <= 0x07_ffff {
                    0x8000_0000
                        | native_unary_opcode(*op)
                        | (function.raw() << 2)
                        | (instruction.continuation_id << 12)
                } else {
                    native_unary_opcode(*op)
                };
            let value = if fact.certainty != crate::region_ir::SsaCertainty::Unknown {
                match (*op, fact.class) {
                    (RegionUnaryOp::Not, class) => {
                        scalar_truthy(builder, src, class).map(|truthy| {
                            let inverted = builder.ins().bxor_imm(truthy, 1);
                            encode_native_bool(builder, inverted)
                        })
                    }
                    (RegionUnaryOp::Plus, SsaValueClass::Int) => Some(src),
                    (RegionUnaryOp::BitNot, SsaValueClass::Int) => Some(builder.ins().bnot(src)),
                    _ => None,
                }
            } else {
                None
            }
            .map_or_else(
                || {
                    lower_native_value_operation(
                        module,
                        builder,
                        native_operations.unary,
                        unary_operation,
                        &[src],
                        result_out,
                    )
                },
                Ok,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::NativeCall(call) => {
            let direct_target = call
                .direct_compiled_target()
                .filter(|_| {
                    !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                        && call
                            .args
                            .iter()
                            .all(|argument| argument.name.is_none() && !argument.unpack)
                })
                .filter(|target| {
                    function_params
                        .get(target)
                        .is_some_and(|(_, _, has_handlers, _)| !has_handlers)
                });
            if call.operands.is_empty()
                && !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                && let Some(target) = direct_target
                && let Some(value) = inline_constants.get(&target).copied()
            {
                let value = lower_region_operand(builder, locals, registers, value)?;
                match call.result {
                    RegionCallResult::Register(destination) => define_region_register(
                        builder,
                        register_variables,
                        registers,
                        destination,
                        value,
                    )?,
                    RegionCallResult::Discard => {}
                    RegionCallResult::ReferenceLocal(_) => unreachable!("filtered above"),
                }
                return Ok(());
            }
            let direct = direct_target
                .and_then(|target| {
                    functions
                        .get(&target)
                        .copied()
                        .map(NativeDirectCallee::Local)
                        .or_else(|| {
                            native_operations
                                .function_resolve
                                .map(|_| NativeDirectCallee::Resolved(target))
                        })
                })
                .map(|callee| (call.result, callee));
            let Some((destination, callee)) = direct else {
                lower_native_call_trampoline(
                    module,
                    builder,
                    native_call_helper,
                    native_operations,
                    native_operations.reference_bind,
                    native_operations.value_lifecycle,
                    value_flow,
                    locals,
                    register_variables,
                    registers,
                    function_params,
                    call,
                    source_block,
                    instruction,
                    result_out,
                    deopt_out,
                    function,
                    local_count,
                    native_version,
                    pointer_type,
                )?;
                return Ok(());
            };
            let result_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let callee_result_out = builder.ins().stack_addr(pointer_type, result_slot, 0);
            let mut prepared_call_args = Vec::with_capacity(call.operands.len());
            let direct_target = call
                .direct_compiled_target()
                .expect("direct target was resolved above");
            let (_, callee_params, _, callee_arity) =
                function_params.get(&direct_target).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_MISSING_CALLEE_PARAMS",
                        format!("callee {} has no parameter metadata", direct_target.raw()),
                    )
                })?;
            for (index, operand) in call.operands.iter().enumerate() {
                let operand = operand.ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_NATIVE_CALL_BINDER_REQUIRED",
                        "direct call argument requires the typed native binder",
                    )
                })?;
                let mut value = lower_region_operand(builder, locals, registers, operand)?;
                if let Some((visible_index, argument)) = index
                    .checked_sub(call.argument_operand_offset)
                    .and_then(|index| call.args.get(index).map(|argument| (index, argument)))
                    && callee_params
                        .get(visible_index)
                        .or_else(|| callee_params.last().filter(|parameter| parameter.variadic))
                        .is_some_and(|parameter| parameter.by_ref)
                {
                    value = lower_direct_reference_argument(
                        module,
                        builder,
                        native_operations.reference_bind,
                        locals,
                        registers,
                        argument,
                        visible_index,
                        value,
                        source_block,
                        instruction,
                        function,
                        result_out,
                    )?;
                }
                if let Some(visible_index) = index.checked_sub(call.argument_operand_offset)
                    && callee_params
                        .get(visible_index)
                        .or_else(|| callee_params.last().filter(|parameter| parameter.variadic))
                        .is_some_and(|parameter| parameter.type_.is_some())
                {
                    let target = builder
                        .ins()
                        .iconst(types::I64, i64::from(direct_target.raw()));
                    let parameter_flags =
                        (visible_index as u64) | (u64::from(call.caller_strict_types) << 32);
                    let parameter_flags = builder.ins().iconst(types::I64, parameter_flags as i64);
                    let caller = builder.ins().iconst(types::I64, i64::from(function.raw()));
                    let continuation = builder
                        .ins()
                        .iconst(types::I64, i64::from(instruction.continuation_id));
                    value = lower_native_value_operation_with_state(
                        module,
                        builder,
                        native_operations.argument_check,
                        0,
                        &[value, target, parameter_flags, caller, continuation],
                        result_out,
                        deopt_out,
                        function,
                        local_count,
                        instruction,
                        locals,
                        native_version,
                    )?;
                }
                prepared_call_args.push(value);
            }
            let mut call_args = Vec::with_capacity(prepared_call_args.len());
            let mut consumed_call_arguments = Vec::new();
            for (index, mut value) in prepared_call_args.into_iter().enumerate() {
                value = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.value_lifecycle,
                    native_dim_operation(0, function, instruction.continuation_id),
                    &[value],
                    result_out,
                )?;
                let consumed_by_call = index < call.argument_operand_offset
                    || index
                        .checked_sub(call.argument_operand_offset)
                        .and_then(|index| call.args.get(index))
                        .is_some_and(native_argument_has_location);
                if consumed_by_call {
                    consumed_call_arguments.push(value);
                }
                call_args.push(value);
            }
            if call.variadic {
                let fixed_arity = call.direct_arity.unwrap_or(1).saturating_sub(1) as usize;
                let variadic_values = call_args.split_off(fixed_arity.min(call_args.len()));
                let mut variadic_array = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_new,
                    0,
                    &[],
                    result_out,
                )?;
                for value in variadic_values {
                    let key = builder
                        .ins()
                        .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
                    variadic_array = lower_native_value_operation(
                        module,
                        builder,
                        native_operations.array_insert,
                        1,
                        &[variadic_array, key, value],
                        result_out,
                    )?;
                }
                call_args.push(variadic_array);
            }
            // Ordinary temporary operands have an explicit IR `Discard`
            // after the call. Only lvalue-backed operands are consumed by the
            // call instruction itself. Keep the historical variadic packing
            // ownership until that path can track the packed operands
            // individually.
            let released_call_arguments = if call.variadic {
                call_args.clone()
            } else {
                consumed_call_arguments
            };
            if call_args.len() != *callee_arity {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_NATIVE_CALL_ARITY",
                    format!(
                        "direct native call to function {} produced {} packed arguments; expected {}",
                        direct_target.raw(),
                        call_args.len(),
                        callee_arity
                    ),
                ));
            }
            let packed_size =
                u32::try_from(call_args.len().max(1).saturating_mul(8)).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_NATIVE_CALL_ARITY",
                        "direct native call argument storage exceeds the native stack-slot ABI",
                    )
                })?;
            let arguments = allocate_native_frame_storage(
                module,
                builder,
                native_operations,
                packed_size,
                3,
                result_out,
            );
            for (index, value) in call_args.iter().copied().enumerate() {
                builder.ins().store(
                    MemFlagsData::new(),
                    value,
                    arguments,
                    i32::try_from(index.saturating_mul(8)).map_err(|_| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_NATIVE_CALL_ARITY",
                            "direct native call argument offset exceeds the native ABI",
                        )
                    })?,
                );
            }
            let callee_call_args = [
                arguments,
                callee_result_out,
                deopt_out,
                builder.ins().iconst(types::I32, -1),
                builder.ins().iconst(pointer_type, 0),
            ];
            let expected_return_status = if call.returns_by_reference {
                crate::JitCallStatus::RETURN_REFERENCE.0
            } else {
                crate::JitCallStatus::RETURN.0
            };
            let call = match callee {
                NativeDirectCallee::Local(callee) => {
                    let callee_ref = module.declare_func_in_func(callee, builder.func);
                    builder.ins().call(callee_ref, &callee_call_args)
                }
                NativeDirectCallee::Resolved(target) => {
                    let helper = native_operations.function_resolve.ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_NATIVE_FUNCTION_RESOLVER",
                            "statically known callee has no compile-on-demand resolver",
                        )
                    })?;
                    let address_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        u32::from(pointer_type.bytes()),
                        3,
                    ));
                    let address_out = builder.ins().stack_addr(pointer_type, address_slot, 0);
                    let vm_context = builder.ins().iconst(types::I64, 0);
                    let function_id = builder.ins().iconst(types::I64, i64::from(target.raw()));
                    let resolve = call_native_helper(
                        module,
                        builder,
                        helper,
                        &[vm_context, function_id, address_out],
                    );
                    require_native_operation_ok(
                        builder,
                        builder.inst_results(resolve)[0],
                        result_out,
                    )?;
                    let address = builder.ins().stack_load(pointer_type, address_slot, 0);
                    let signature = builder.func.signature.clone();
                    let signature = builder.import_signature(signature);
                    builder
                        .ins()
                        .call_indirect(signature, address, &callee_call_args)
                }
            };
            let status = builder.inst_results(call)[0];
            release_native_frame_storage(
                module,
                builder,
                native_operations,
                arguments,
                result_out,
            )?;
            let ok = builder.create_block();
            let side_exit = builder.create_block();
            let is_ok =
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, status, i64::from(expected_return_status));
            builder.ins().brif(is_ok, ok, &[], side_exit, &[]);
            builder.switch_to_block(side_exit);
            let control_value = builder.ins().stack_load(types::I64, result_slot, 0);
            for argument in &released_call_arguments {
                let _ = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.value_lifecycle,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[*argument],
                    result_out,
                )?;
            }
            builder
                .ins()
                .store(MemFlagsData::new(), control_value, result_out, 0);
            let caller_unwind = builder.create_block();
            let preserve_callee = builder.create_block();
            let is_throw = builder.ins().icmp_imm(
                IntCC::Equal,
                status,
                i64::from(crate::JitCallStatus::THROW.0),
            );
            let is_exit = builder.ins().icmp_imm(
                IntCC::Equal,
                status,
                i64::from(crate::JitCallStatus::EXIT.0),
            );
            let unwinds_caller = builder.ins().bor(is_throw, is_exit);
            builder
                .ins()
                .brif(unwinds_caller, caller_unwind, &[], preserve_callee, &[]);
            builder.switch_to_block(caller_unwind);
            // A throw or exit must now traverse this caller's catch/finally
            // table. Publish the call-site continuation and live locals before
            // returning the explicit control status to the unwind driver.
            publish_native_call_state(
                builder,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            builder.ins().return_(&[status]);
            builder.switch_to_block(preserve_callee);
            // Guard exits and other non-unwind statuses retain the callee's
            // precise continuation so native transitions can resume it.
            builder.ins().return_(&[status]);
            builder.switch_to_block(ok);
            let value = builder.ins().stack_load(types::I64, result_slot, 0);
            if !matches!(destination, RegionCallResult::Discard)
                && let Some(first) = call_args.first()
            {
                let mut aliases_argument = builder.ins().icmp(IntCC::Equal, value, *first);
                for argument in &call_args[1..] {
                    let aliases = builder.ins().icmp(IntCC::Equal, value, *argument);
                    aliases_argument = builder.ins().bor(aliases_argument, aliases);
                }
                let preserve_result = builder.create_block();
                let release_arguments = builder.create_block();
                builder.ins().brif(
                    aliases_argument,
                    preserve_result,
                    &[],
                    release_arguments,
                    &[],
                );
                builder.switch_to_block(preserve_result);
                let _ = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.value_lifecycle,
                    native_dim_operation(0, function, instruction.continuation_id),
                    &[value],
                    result_out,
                )?;
                builder.ins().jump(release_arguments, &[]);
                builder.switch_to_block(release_arguments);
            }
            for argument in &released_call_arguments {
                let _ = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.value_lifecycle,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[*argument],
                    result_out,
                )?;
            }
            match destination {
                RegionCallResult::Register(dst) => {
                    define_region_register(builder, register_variables, registers, dst, value)?;
                }
                RegionCallResult::ReferenceLocal(local) => {
                    builder.def_var(local_variable(locals, local)?, value);
                }
                RegionCallResult::Discard => {}
            }
        }
        RegionInstructionKind::NativeControl(control) => match control {
            RegionNativeControl::EnterTry { .. } | RegionNativeControl::LeaveTry => {
                // Handler state is published in `JitRegionStateMetadata` and
                // consumed by explicit native unwind. These markers do not
                // enter the explicit native exception-unwind path.
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
                    lower_owned_frame_locals(
                        module,
                        builder,
                        locals,
                        native_operations,
                        value_flow,
                        function,
                        result_out,
                    )?;
                    publish_native_call_state(
                        builder,
                        deopt_out,
                        function,
                        local_count,
                        instruction,
                        locals,
                        native_version,
                    )?;
                    builder
                        .ins()
                        .store(MemFlagsData::new(), value, result_out, 0);
                    builder.ins().return_(&[status]);
                }
                let unreachable = builder.create_block();
                builder.switch_to_block(unreachable);
                builder.seal_block(unreachable);
            }
            RegionNativeControl::Throw {
                value,
                catch,
                finally,
                exception_local,
            } => {
                let value = lower_region_operand(builder, locals, registers, *value)?;
                if let Some(catch) = catch {
                    if let Some(local) = exception_local {
                        builder.def_var(local_variable(locals, *local)?, value);
                    }
                    builder.ins().jump(cranelift_block(blocks, *catch)?, &[]);
                } else if let Some(finally) = finally {
                    let status = builder
                        .ins()
                        .iconst(types::I32, i64::from(crate::JitCallStatus::THROW.0));
                    builder.def_var(pending_status, status);
                    builder.def_var(pending_value, value);
                    builder.ins().jump(cranelift_block(blocks, *finally)?, &[]);
                } else {
                    lower_owned_frame_locals(
                        module,
                        builder,
                        locals,
                        native_operations,
                        value_flow,
                        function,
                        result_out,
                    )?;
                    publish_native_call_state(
                        builder,
                        deopt_out,
                        function,
                        local_count,
                        instruction,
                        locals,
                        native_version,
                    )?;
                    builder
                        .ins()
                        .store(MemFlagsData::new(), value, result_out, 0);
                    let status = builder
                        .ins()
                        .iconst(types::I32, i64::from(crate::JitCallStatus::THROW.0));
                    builder.ins().return_(&[status]);
                }
                let unreachable = builder.create_block();
                builder.switch_to_block(unreachable);
                builder.seal_block(unreachable);
            }
            RegionNativeControl::MakeException { dst, message, .. } => {
                let message = if let Some(message) = message {
                    lower_region_operand(builder, locals, registers, *message)?
                } else {
                    builder
                        .ins()
                        .iconst(types::I64, crate::jit_encode_constant(u32::MAX))
                };
                let function_id = builder.ins().iconst(types::I64, i64::from(function.raw()));
                let continuation_id = builder
                    .ins()
                    .iconst(types::I64, i64::from(instruction.continuation_id));
                let value = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.exception_new,
                    0,
                    &[message, function_id, continuation_id],
                    result_out,
                )?;
                define_region_register(builder, register_variables, registers, *dst, value)?;
            }
        },
        RegionInstructionKind::NativeSuspend(suspend) => {
            lower_native_suspension(
                builder,
                suspension_blocks,
                locals,
                register_variables,
                registers,
                suspend,
                instruction,
                result_out,
                deopt_out,
                resume_state,
                function,
                local_count,
            )?;
        }
        RegionInstructionKind::NativeDynamicCode(operation) => {
            lower_native_dynamic_code(
                module,
                builder,
                native_dynamic_code_helper,
                locals,
                register_variables,
                registers,
                operation,
                instruction,
                result_out,
                function,
                pointer_type,
            )?;
        }
        RegionInstructionKind::Compare { dst, op, lhs, rhs } => {
            let lhs_operand = *lhs;
            let rhs_operand = *rhs;
            let lhs = lower_region_operand(builder, locals, registers, lhs_operand)?;
            let rhs = lower_region_operand(builder, locals, registers, rhs_operand)?;
            let lhs_fact = value_flow.operand_fact(constants, lhs_operand);
            let rhs_fact = value_flow.operand_fact(constants, rhs_operand);
            let direct = (lhs_fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && rhs_fact.certainty != crate::region_ir::SsaCertainty::Unknown)
                .then(|| {
                    lower_direct_compare(builder, *op, lhs, rhs, lhs_fact.class, rhs_fact.class)
                })
                .flatten();
            let cl_value = if let Some(value) = direct {
                value
            } else if native_operations.compare.is_some() {
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.compare,
                    native_compare_opcode(*op),
                    &[lhs, rhs],
                    result_out,
                )?
            } else {
                let compared = builder.ins().icmp(region_compare_intcc(*op), lhs, rhs);
                builder.ins().uextend(types::I64, compared)
            };
            define_region_register(builder, register_variables, registers, *dst, cl_value)?;
        }
        RegionInstructionKind::Cast { dst, op, src } => {
            let src_operand = *src;
            let src = lower_region_operand(builder, locals, registers, src_operand)?;
            let fact = value_flow.operand_fact(constants, src_operand);
            let cast_operation =
                if function.raw() <= 0x03ff && instruction.continuation_id <= 0x03_ffff {
                    0x8000_0000
                        | native_cast_opcode(*op)
                        | (function.raw() << 3)
                        | (instruction.continuation_id << 13)
                } else {
                    native_cast_opcode(*op)
                };
            let direct = (fact.certainty != crate::region_ir::SsaCertainty::Unknown)
                .then(|| lower_direct_cast(builder, *op, src, fact.class))
                .flatten();
            let value = if let Some(value) = direct {
                value
            } else {
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.cast,
                    cast_operation,
                    &[src],
                    result_out,
                )?
            };
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::Echo { src } => {
            let src = lower_region_operand(builder, locals, registers, *src)?;
            let helper = native_operations.echo.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                    "native echo helper was not declared",
                )
            })?;
            let context = builder.ins().iconst(types::I64, 0);
            let call = call_native_helper(module, builder, helper, &[context, src]);
            require_native_operation_ok(builder, builder.inst_results(call)[0], result_out)?;
        }
        RegionInstructionKind::NewArray { dst } => {
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.array_new,
                0,
                &[],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::NewObject { dst, class } => {
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.object_new,
                *class,
                &[],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::FetchProperty { dst, object } => {
            publish_native_call_state(
                builder,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            publish_native_register_state(builder, deopt_out, registers);
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.property_fetch,
                0,
                &[object, function, instruction_id],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::FetchDynamicStaticProperty { dst, class_name } => {
            publish_native_call_state(
                builder,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            publish_native_register_state(builder, deopt_out, registers);
            let class_name = lower_region_operand(builder, locals, registers, *class_name)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.property_fetch,
                2,
                &[class_name, function_value, instruction_id],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::FetchObjectClassName { dst, object } => {
            publish_native_call_state(
                builder,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            publish_native_register_state(builder, deopt_out, registers);
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.property_fetch,
                1,
                &[object, function_value, instruction_id],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::AssignProperty { dst, object, value } => {
            publish_native_call_state(
                builder,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            publish_native_register_state(builder, deopt_out, registers);
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.property_assign,
                0,
                &[object, value, function, instruction_id],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::CloneObject { dst, object } => {
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.object_clone,
                0,
                &[object],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::CloneWith {
            dst,
            object,
            replacements,
        } => {
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let replacements = lower_region_operand(builder, locals, registers, *replacements)?;
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.object_clone_with,
                0,
                &[object, replacements],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::ArrayInsert {
            array,
            key,
            value,
            by_ref_local,
        } => {
            let array_value =
                lower_region_operand(builder, locals, registers, RegionOperand::Register(*array))?;
            let append = key.is_none();
            let key = match key {
                Some(key) => lower_region_operand(builder, locals, registers, *key)?,
                None => builder
                    .ins()
                    .iconst(types::I64, crate::jit_encode_constant(u32::MAX)),
            };
            let mut value = lower_region_operand(builder, locals, registers, *value)?;
            if let Some(local) = by_ref_local {
                let zero = builder.ins().iconst(types::I64, 0);
                value = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.reference_bind,
                    0,
                    &[value, zero, zero],
                    result_out,
                )?;
                builder.def_var(local_variable(locals, *local)?, value);
                publish_native_reference_local(
                    module,
                    builder,
                    native_operations.reference_bind,
                    value,
                    function,
                    *local,
                    result_out,
                )?;
            }
            let updated = lower_native_value_operation_with_state(
                module,
                builder,
                native_operations.array_insert,
                native_dim_operation(u32::from(append), function, instruction.continuation_id),
                &[array_value, key, value],
                result_out,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            define_region_register(builder, register_variables, registers, *array, updated)?;
        }
        RegionInstructionKind::ArraySpread { array, source } => {
            let array_value =
                lower_region_operand(builder, locals, registers, RegionOperand::Register(*array))?;
            let source = lower_region_operand(builder, locals, registers, *source)?;
            let updated = lower_native_value_operation(
                module,
                builder,
                native_operations.array_spread,
                0,
                &[array_value, source],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *array, updated)?;
        }
        RegionInstructionKind::FetchDim {
            dst,
            array,
            key,
            quiet,
        } => {
            let release_array = matches!(array, RegionOperand::Local(_));
            let array = if let RegionOperand::Local(local) = array {
                let current = use_local_variable(builder, locals, *local)?;
                lower_native_local_fetch(
                    module,
                    builder,
                    native_operations.local_fetch,
                    current,
                    *quiet,
                    function,
                    *local,
                    instruction.span,
                    result_out,
                )?
            } else {
                lower_region_operand(builder, locals, registers, *array)?
            };
            let key = lower_region_operand(builder, locals, registers, *key)?;
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.array_fetch,
                native_dim_operation(u32::from(*quiet), function, instruction.continuation_id),
                &[array, key],
                result_out,
            )?;
            if release_array {
                let _ = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.value_lifecycle,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[array],
                    result_out,
                )?;
            }
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::FetchConst { dst } => {
            let function_id = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let continuation_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.constant_fetch,
                0,
                &[function_id, continuation_id],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::AssignDim {
            dst,
            local,
            keys,
            value,
        } => {
            // Dimension writes may raise a catchable TypeError (for example
            // when a string is indexed with a string key). Publish the
            // current continuation before entering the helper so native
            // unwind can resume this function's catch/finally table.
            publish_native_call_state(
                builder,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            publish_native_register_state(builder, deopt_out, registers);
            let current = use_local_variable(builder, locals, *local)?;
            let local_fact = value_flow.local_fact(*local);
            let direct_array_local = value_flow.local_storage(*local).is_promoted()
                && local_fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && local_fact.class == SsaValueClass::ArrayHandle;
            let root = if direct_array_local {
                current
            } else {
                lower_native_local_fetch(
                    module,
                    builder,
                    native_operations.local_fetch,
                    current,
                    false,
                    function,
                    *local,
                    instruction.span,
                    result_out,
                )?
            };
            let keys = keys
                .iter()
                .map(|key| lower_region_operand(builder, locals, registers, *key))
                .collect::<Result<Vec<_>, _>>()?;
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let mut arrays = Vec::with_capacity(keys.len());
            arrays.push(root);
            let mut nested = root;
            for key in keys.iter().take(keys.len().saturating_sub(1)) {
                nested = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_fetch,
                    // Intermediate write dimensions are lvalue
                    // auto-vivification probes. Missing keys must not emit
                    // the read-side undefined-array-key warning.
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[nested, *key],
                    result_out,
                )?;
                arrays.push(nested);
            }
            let array_insert_operation =
                native_dim_operation(0, function, instruction.continuation_id);
            let mut updated = lower_native_value_operation(
                module,
                builder,
                native_operations.array_insert,
                array_insert_operation,
                &[
                    nested,
                    *keys.last().expect("AssignDim retains at least one key"),
                    value,
                ],
                result_out,
            )?;
            for index in (0..keys.len().saturating_sub(1)).rev() {
                updated = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_insert,
                    array_insert_operation,
                    &[arrays[index], keys[index], updated],
                    result_out,
                )?;
            }
            let stored = if direct_array_local {
                updated
            } else {
                let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
                let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.local_store,
                    0,
                    &[current, updated, function_value, local_value],
                    result_out,
                )?
            };
            builder.def_var(local_variable(locals, *local)?, stored);
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::AppendDim {
            dst,
            local,
            keys,
            value,
        } => {
            let current = use_local_variable(builder, locals, *local)?;
            let local_fact = value_flow.local_fact(*local);
            let direct_array_local = value_flow.local_storage(*local).is_promoted()
                && local_fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && local_fact.class == SsaValueClass::ArrayHandle;
            let root = if direct_array_local {
                current
            } else {
                lower_native_local_fetch(
                    module,
                    builder,
                    native_operations.local_fetch,
                    current,
                    false,
                    function,
                    *local,
                    instruction.span,
                    result_out,
                )?
            };
            let keys = keys
                .iter()
                .map(|key| lower_region_operand(builder, locals, registers, *key))
                .collect::<Result<Vec<_>, _>>()?;
            let mut arrays = Vec::with_capacity(keys.len());
            arrays.push(root);
            let mut nested = root;
            for key in &keys {
                nested = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[nested, *key],
                    result_out,
                )?;
                arrays.push(nested);
            }
            let key = builder
                .ins()
                .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let mut updated = lower_native_value_operation(
                module,
                builder,
                native_operations.array_insert,
                native_dim_operation(1, function, instruction.continuation_id),
                &[nested, key, value],
                result_out,
            )?;
            for index in (0..keys.len()).rev() {
                updated = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_insert,
                    native_dim_operation(0, function, instruction.continuation_id),
                    &[arrays[index], keys[index], updated],
                    result_out,
                )?;
            }
            let stored = if direct_array_local {
                updated
            } else {
                let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
                let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.local_store,
                    0,
                    &[current, updated, function_value, local_value],
                    result_out,
                )?
            };
            builder.def_var(local_variable(locals, *local)?, stored);
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::IssetDim { dst, local, keys } => {
            let mut value = use_local_variable(builder, locals, *local)?;
            value = lower_native_local_fetch(
                module,
                builder,
                native_operations.local_fetch,
                value,
                true,
                function,
                *local,
                instruction.span,
                result_out,
            )?;
            for key in keys {
                let key = lower_region_operand(builder, locals, registers, *key)?;
                value = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[value, key],
                    result_out,
                )?;
            }
            let null = builder
                .ins()
                .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
            let result = lower_native_value_operation(
                module,
                builder,
                native_operations.compare,
                native_compare_opcode(RegionCompareOpCode::NotIdentical),
                &[value, null],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::EmptyDim { dst, local, keys } => {
            let mut value = use_local_variable(builder, locals, *local)?;
            value = lower_native_local_fetch(
                module,
                builder,
                native_operations.local_fetch,
                value,
                true,
                function,
                *local,
                instruction.span,
                result_out,
            )?;
            for key in keys {
                let key = lower_region_operand(builder, locals, registers, *key)?;
                value = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[value, key],
                    result_out,
                )?;
            }
            let truthy = lower_native_value_operation(
                module,
                builder,
                native_operations.cast,
                native_cast_opcode(RegionCastOp::Bool),
                &[value],
                result_out,
            )?;
            let result = lower_native_value_operation(
                module,
                builder,
                native_operations.unary,
                native_unary_opcode(RegionUnaryOp::Not),
                &[truthy],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::UnsetDim { local, keys } => {
            let current = use_local_variable(builder, locals, *local)?;
            let root = lower_native_local_fetch(
                module,
                builder,
                native_operations.local_fetch,
                current,
                true,
                function,
                *local,
                instruction.span,
                result_out,
            )?;
            let keys = keys
                .iter()
                .map(|key| lower_region_operand(builder, locals, registers, *key))
                .collect::<Result<Vec<_>, _>>()?;
            let mut arrays = Vec::with_capacity(keys.len());
            arrays.push(root);
            let mut nested = root;
            for key in keys.iter().take(keys.len().saturating_sub(1)) {
                nested = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_dim_operation(1, function, instruction.continuation_id),
                    &[nested, *key],
                    result_out,
                )?;
                arrays.push(nested);
            }
            let mut updated = lower_native_value_operation(
                module,
                builder,
                native_operations.array_unset,
                native_dim_operation(0, function, instruction.continuation_id),
                &[
                    nested,
                    *keys.last().expect("UnsetDim retains at least one key"),
                ],
                result_out,
            )?;
            for index in (0..keys.len().saturating_sub(1)).rev() {
                updated = lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_insert,
                    0,
                    &[arrays[index], keys[index], updated],
                    result_out,
                )?;
            }
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
            let stored = lower_native_value_operation(
                module,
                builder,
                native_operations.local_store,
                0,
                &[current, updated, function_value, local_value],
                result_out,
            )?;
            builder.def_var(local_variable(locals, *local)?, stored);
        }
        RegionInstructionKind::IssetLocal { dst, local } => {
            let value = use_local_variable(builder, locals, *local)?;
            let value = lower_native_local_fetch(
                module,
                builder,
                native_operations.local_fetch,
                value,
                true,
                function,
                *local,
                instruction.span,
                result_out,
            )?;
            let null = builder
                .ins()
                .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
            let result = lower_native_value_operation(
                module,
                builder,
                native_operations.compare,
                native_compare_opcode(RegionCompareOpCode::NotIdentical),
                &[value, null],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::EmptyLocal { dst, local } => {
            let value = use_local_variable(builder, locals, *local)?;
            let value = lower_native_local_fetch(
                module,
                builder,
                native_operations.local_fetch,
                value,
                true,
                function,
                *local,
                instruction.span,
                result_out,
            )?;
            let truthy = lower_native_value_operation(
                module,
                builder,
                native_operations.cast,
                native_cast_opcode(RegionCastOp::Bool),
                &[value],
                result_out,
            )?;
            let result = lower_native_value_operation(
                module,
                builder,
                native_operations.unary,
                native_unary_opcode(RegionUnaryOp::Not),
                &[truthy],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::UnsetLocal { local } => {
            let current = use_local_variable(builder, locals, *local)?;
            let uninitialized = builder.ins().iconst(
                types::I64,
                crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
            );
            builder.def_var(local_variable(locals, *local)?, uninitialized);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                uninitialized,
                function,
                *local,
                result_out,
            )?;
            let _ = lower_native_value_operation(
                module,
                builder,
                native_operations.value_lifecycle,
                native_dim_operation(1, function, instruction.continuation_id),
                &[current],
                result_out,
            )?;
        }
        RegionInstructionKind::ForeachInit { iterator, source } => {
            let source = lower_region_operand(builder, locals, registers, *source)?;
            let none = builder.ins().iconst(types::I64, i64::from(u32::MAX));
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.foreach_init,
                0,
                &[source, none, none],
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *iterator, value)?;
        }
        RegionInstructionKind::ForeachInitRef { iterator, local } => {
            let source = use_local_variable(builder, locals, *local)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.foreach_init,
                1,
                &[source, function_value, local_value],
                result_out,
            )?;
            // Foreach-by-reference replaces the array elements with reference
            // cells during initialization. Publish that changed root before
            // the loop so script-scope/global reads observe the same cells
            // instead of the pre-separation COW snapshot.
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                source,
                function,
                *local,
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *iterator, value)?;
        }
        RegionInstructionKind::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            let helper = native_operations.foreach_next.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                    "native foreach-next helper was not declared",
                )
            })?;
            let key_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let value_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let has_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let iterator_value = lower_region_operand(
                builder,
                locals,
                registers,
                RegionOperand::Register(*iterator),
            )?;
            let context = builder.ins().iconst(types::I64, 0);
            let key_out = builder.ins().stack_addr(pointer_type, key_slot, 0);
            let value_out = builder.ins().stack_addr(pointer_type, value_slot, 0);
            let has_out = builder.ins().stack_addr(pointer_type, has_slot, 0);
            let call = call_native_helper(
                module,
                builder,
                helper,
                &[context, iterator_value, key_out, value_out, has_out],
            );
            require_native_operation_ok(builder, builder.inst_results(call)[0], result_out)?;
            let has = builder.ins().stack_load(types::I64, has_slot, 0);
            let next_value = builder.ins().stack_load(types::I64, value_slot, 0);
            define_region_register(builder, register_variables, registers, *has_value, has)?;
            define_region_register(builder, register_variables, registers, *value, next_value)?;
            if let Some(key) = key {
                let next_key = builder.ins().stack_load(types::I64, key_slot, 0);
                define_region_register(builder, register_variables, registers, *key, next_key)?;
            }
        }
        RegionInstructionKind::ForeachCleanup { iterator } => {
            let helper = native_operations.foreach_cleanup.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                    "native foreach-cleanup helper was not declared",
                )
            })?;
            let iterator = lower_region_operand(
                builder,
                locals,
                registers,
                RegionOperand::Register(*iterator),
            )?;
            let context = builder.ins().iconst(types::I64, 0);
            let call = call_native_helper(module, builder, helper, &[context, iterator]);
            require_native_operation_ok(builder, builder.inst_results(call)[0], result_out)?;
        }
        RegionInstructionKind::ForeachNextRef {
            has_value,
            iterator,
            key,
            value_local,
        } => {
            let helper = native_operations.foreach_next.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                    "native foreach-next helper was not declared",
                )
            })?;
            let key_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let value_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let has_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let iterator_value = lower_region_operand(
                builder,
                locals,
                registers,
                RegionOperand::Register(*iterator),
            )?;
            let context = builder.ins().iconst(types::I64, 0);
            let key_out = builder.ins().stack_addr(pointer_type, key_slot, 0);
            let value_out = builder.ins().stack_addr(pointer_type, value_slot, 0);
            let has_out = builder.ins().stack_addr(pointer_type, has_slot, 0);
            let call = call_native_helper(
                module,
                builder,
                helper,
                &[context, iterator_value, key_out, value_out, has_out],
            );
            require_native_operation_ok(builder, builder.inst_results(call)[0], result_out)?;
            let has = builder.ins().stack_load(types::I64, has_slot, 0);
            let next_value = builder.ins().stack_load(types::I64, value_slot, 0);
            define_region_register(builder, register_variables, registers, *has_value, has)?;
            builder.def_var(local_variable(locals, *value_local)?, next_value);
            publish_native_reference_local(
                module,
                builder,
                native_operations.reference_bind,
                next_value,
                function,
                *value_local,
                result_out,
            )?;
            if let Some(key) = key {
                let next_key = builder.ins().stack_load(types::I64, key_slot, 0);
                define_region_register(builder, register_variables, registers, *key, next_key)?;
            }
        }
        RegionInstructionKind::RuntimeFatal { dst, .. } => {
            if let Some(dst) = dst {
                let unreachable_value = builder.ins().iconst(types::I64, 0);
                define_region_register(
                    builder,
                    register_variables,
                    registers,
                    *dst,
                    unreachable_value,
                )?;
            }
            let helper = native_operations.runtime_fatal.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                    "native runtime-fatal helper was not declared",
                )
            })?;
            let context = builder.ins().iconst(types::I64, 0);
            let function_id = builder.ins().iconst(types::I32, i64::from(function.raw()));
            let continuation_id = builder
                .ins()
                .iconst(types::I32, i64::from(instruction.continuation_id));
            let call = call_native_helper(
                module,
                builder,
                helper,
                &[context, function_id, continuation_id],
            );
            let status = builder.inst_results(call)[0];
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
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_native_suspension(
    builder: &mut FunctionBuilder<'_>,
    suspension_blocks: &BTreeMap<u32, ir::Block>,
    locals: &BTreeMap<LocalId, Variable>,
    register_variables: &BTreeMap<RegId, Variable>,
    registers: &mut BTreeMap<RegId, Variable>,
    suspend: &RegionNativeSuspend,
    instruction: &RegionInstruction,
    result_out: ir::Value,
    state_out: ir::Value,
    resume_state: ir::Value,
    function: FunctionId,
    local_count: u32,
) -> Result<(), CraneliftLoweringError> {
    let (dst, kind, key, yielded, delegation, status) = match suspend {
        RegionNativeSuspend::GeneratorYield { dst, key, value } => (
            *dst,
            crate::JitNativeSuspendKind::GENERATOR_YIELD,
            key.map(|key| lower_region_operand(builder, locals, registers, key))
                .transpose()?,
            value
                .map(|value| lower_region_operand(builder, locals, registers, value))
                .transpose()?
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0)),
            None,
            crate::JitCallStatus::SUSPEND_GENERATOR,
        ),
        RegionNativeSuspend::GeneratorDelegate { dst, source } => {
            let source = lower_region_operand(builder, locals, registers, *source)?;
            (
                *dst,
                crate::JitNativeSuspendKind::GENERATOR_DELEGATE,
                None,
                source,
                Some(source),
                crate::JitCallStatus::SUSPEND_GENERATOR,
            )
        }
        RegionNativeSuspend::FiberSuspend { dst, value } => (
            *dst,
            crate::JitNativeSuspendKind::FIBER_SUSPEND,
            None,
            value
                .map(|value| lower_region_operand(builder, locals, registers, value))
                .transpose()?
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0)),
            None,
            crate::JitCallStatus::SUSPEND_FIBER,
        ),
    };
    builder
        .ins()
        .store(MemFlagsData::new(), yielded, result_out, 0);
    let store_i32 = |builder: &mut FunctionBuilder<'_>, offset: usize, value: u32| {
        let value = builder.ins().iconst(types::I32, i64::from(value));
        builder
            .ins()
            .store(MemFlagsData::new(), value, state_out, offset as i32);
    };
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitDeoptState, function_id),
        function.raw(),
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitDeoptState, continuation_id),
        instruction.continuation_id,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitDeoptState, slot_count),
        local_count,
    );
    publish_native_local_masks(builder, state_out, &instruction.live_locals);
    for local in &instruction.live_locals {
        let value = use_local_variable(builder, locals, *local)?;
        let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
            .saturating_add(local.index().saturating_mul(8));
        builder
            .ins()
            .store(MemFlagsData::new(), value, state_out, offset as i32);
    }
    let register_ids = registers
        .keys()
        .filter(|register| register.index() < crate::JIT_DEOPT_MAX_REGISTERS)
        .copied()
        .collect::<Vec<_>>();
    let register_mask = register_ids.iter().fold(0_u64, |mask, register| {
        mask | 1_u64.checked_shl(register.raw()).unwrap_or(0)
    });
    let mask = builder.ins().iconst(types::I64, register_mask as i64);
    builder.ins().store(
        MemFlagsData::new(),
        mask,
        state_out,
        std::mem::offset_of!(crate::JitDeoptState, initialized_register_mask) as i32,
    );
    for register in &register_ids {
        let value = builder.use_var(registers[register]);
        let value = if builder.func.dfg.value_type(value) == types::I64 {
            value
        } else {
            builder.ins().uextend(types::I64, value)
        };
        let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
            .saturating_add(register.index().saturating_mul(8));
        builder
            .ins()
            .store(MemFlagsData::new(), value, state_out, offset as i32);
    }
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitDeoptState, suspend_kind),
        kind.0,
    );
    let flags = u32::from(key.is_some()) | if delegation.is_some() { 1 << 1 } else { 0 };
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitDeoptState, suspend_flags),
        flags,
    );
    let key = key.unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
    builder.ins().store(
        MemFlagsData::new(),
        key,
        state_out,
        std::mem::offset_of!(crate::JitDeoptState, yielded_key) as i32,
    );
    let delegation = delegation.unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
    builder.ins().store(
        MemFlagsData::new(),
        delegation,
        state_out,
        std::mem::offset_of!(crate::JitDeoptState, delegation_handle) as i32,
    );
    let status_value = builder.ins().iconst(types::I32, i64::from(status.0));
    builder.ins().return_(&[status_value]);

    let resume_block = *suspension_blocks
        .get(&instruction.continuation_id)
        .ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_SUSPENSION_ENTRY",
                format!(
                    "continuation {} has no native suspension entry",
                    instruction.continuation_id
                ),
            )
        })?;
    builder.switch_to_block(resume_block);
    let resume_status = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        resume_state,
        std::mem::offset_of!(crate::JitDeoptState, control_status) as i32,
    );
    let resume_value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        resume_state,
        std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
    );
    let resume_ok = builder.create_block();
    let propagate = builder.create_block();
    let is_value = builder.ins().icmp_imm(
        IntCC::Equal,
        resume_status,
        i64::from(crate::JitCallStatus::CONTINUE.0),
    );
    builder.ins().brif(is_value, resume_ok, &[], propagate, &[]);
    builder.switch_to_block(propagate);
    builder
        .ins()
        .store(MemFlagsData::new(), resume_value, result_out, 0);
    builder.ins().return_(&[resume_status]);
    builder.switch_to_block(resume_ok);
    registers.clear();
    for register in register_ids {
        let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
            .saturating_add(register.index().saturating_mul(8));
        let value =
            builder
                .ins()
                .load(types::I64, MemFlagsData::new(), resume_state, offset as i32);
        define_region_register(builder, register_variables, registers, register, value)?;
    }
    define_region_register(builder, register_variables, registers, dst, resume_value)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_native_call_trampoline(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    native_call_helper: Option<NativeHelper>,
    native_frame_operations: NativeOperationFunctions,
    native_reference_bind_helper: Option<NativeHelper>,
    native_value_lifecycle_helper: Option<NativeHelper>,
    value_flow: &ExecutableValueFlow,
    locals: &BTreeMap<LocalId, Variable>,
    register_variables: &BTreeMap<RegId, Variable>,
    registers: &mut BTreeMap<RegId, Variable>,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    call: &RegionNativeCall,
    source_block: BlockId,
    instruction: &RegionInstruction,
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    native_version: u32,
    pointer_type: ir::Type,
) -> Result<(), CraneliftLoweringError> {
    let helper = native_call_helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_CALL_TRAMPOLINE",
            "native call site has no typed dispatch trampoline",
        )
    })?;
    let argument_size = std::mem::size_of::<crate::JitNativeCallArgument>();
    // Direct-call regions append scalar defaults to `operands` so compiled
    // callees can receive a complete fixed-arity frame. Once a call falls
    // back to the runtime binder, however, those defaults must be bound by
    // the callee and must not masquerade as explicitly supplied PHP
    // arguments. Keep only target operands (receiver/callable/captures) and
    // the arguments that actually occur at the call site.
    let argument_count = if matches!(
        call.target,
        RegionCallTarget::Function {
            function: Some(_),
            ..
        }
    ) {
        call.argument_operand_offset
            .checked_add(call.args.len())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_CALL_ARGUMENTS",
                    "native call argument count overflowed",
                )
            })?
    } else {
        // Runtime operations represented by synthetic function targets have
        // no PHP call arguments; every lowered operand belongs to the
        // operation and still has to cross the trampoline.
        call.args.len().max(call.operands.len())
    };
    let mut consumed_arguments = Vec::new();
    let mut speculative_local_bindings = BTreeMap::<LocalId, (ir::Value, Vec<i32>)>::new();
    let arguments_ptr = if argument_count == 0 {
        builder.ins().iconst(pointer_type, 0)
    } else {
        let bytes = argument_size.checked_mul(argument_count).ok_or_else(|| {
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
        let pointer = allocate_native_frame_storage(
            module,
            builder,
            native_frame_operations,
            bytes,
            3,
            result_out,
        );
        for index in 0..argument_count {
            let argument = index
                .checked_sub(call.argument_operand_offset)
                .and_then(|index| call.args.get(index));
            let base = i32::try_from(index.saturating_mul(argument_size)).unwrap_or(i32::MAX);
            let mut lowered = call
                .operands
                .get(index)
                .copied()
                .flatten()
                .map(|operand| lower_region_operand(builder, locals, registers, operand))
                .transpose()?;
            let visible_index = index.saturating_sub(call.argument_operand_offset);
            let requires_reference = known_user_argument_requires_reference(
                call,
                visible_index,
                function_params,
                function,
            )
            .unwrap_or_else(|| call.argument_requires_reference_binding(visible_index));
            let speculative_original_local = if requires_reference
                && call
                    .declared_argument_reference_requirement(visible_index)
                    .is_none()
                && matches!(
                    call.target,
                    RegionCallTarget::Function { function: None, .. }
                )
                && let Some(local) = argument.and_then(|argument| argument.by_ref_local)
            {
                Some((local, use_local_variable(builder, locals, local)?))
            } else {
                None
            };
            if call
                .operands
                .get(index)
                .copied()
                .flatten()
                .is_some_and(|operand| matches!(operand, RegionOperand::Register(_)))
                && !call
                    .operands
                    .get(index)
                    .copied()
                    .flatten()
                    .is_some_and(|operand| {
                        matches!(
                            operand,
                            RegionOperand::Register(register)
                                if value_flow.register_fact(register).ownership
                                    == SsaOwnership::Borrowed
                        )
                    })
                && matches!(
                    call.target,
                    RegionCallTarget::Function { function: None, .. }
                )
                && !requires_reference
                && argument.is_some_and(native_argument_has_location)
                && let Some(value) = lowered
            {
                consumed_arguments.push(value);
            }
            // A declared by-reference parameter does not make an arbitrary
            // expression referenceable. Preserve non-lvalues as values so
            // the runtime binder emits PHP's "could not be passed by
            // reference" error instead of manufacturing a temporary cell.
            if requires_reference
                && argument.is_some_and(native_argument_has_location)
                && let (Some(argument), Some(value)) = (argument, lowered)
            {
                lowered = Some(lower_direct_reference_argument(
                    module,
                    builder,
                    native_reference_bind_helper,
                    locals,
                    registers,
                    argument,
                    visible_index,
                    value,
                    source_block,
                    instruction,
                    function,
                    result_out,
                )?);
                if let Some((local, original)) = speculative_original_local {
                    let flags_offset = base + 24;
                    speculative_local_bindings
                        .entry(local)
                        .and_modify(|(_, offsets)| offsets.push(flags_offset))
                        .or_insert_with(|| (original, vec![flags_offset]));
                }
            }
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
            let name_hash = argument
                .and_then(|argument| argument.name.as_deref())
                .map_or(0, stable_call_symbol_hash);
            let name_hash = builder.ins().iconst(types::I64, name_hash as i64);
            builder
                .ins()
                .store(MemFlagsData::new(), name_hash, pointer, base + 16);
            let flags = builder.ins().iconst(
                types::I32,
                i64::from(argument.map_or(0, native_argument_flags)),
            );
            builder
                .ins()
                .store(MemFlagsData::new(), flags, pointer, base + 24);
            let source_slot = argument
                .and_then(|argument| argument.by_ref_local)
                .map_or(u32::MAX, LocalId::raw);
            let source_slot = builder.ins().iconst(types::I32, i64::from(source_slot));
            builder
                .ins()
                .store(MemFlagsData::new(), source_slot, pointer, base + 28);
            let property_receiver = argument
                .and_then(|argument| argument.by_ref_property.as_ref())
                .map(|target| lower_ir_operand(builder, locals, registers, target.object))
                .transpose()?
                .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            builder
                .ins()
                .store(MemFlagsData::new(), property_receiver, pointer, base + 32);
        }
        pointer
    };

    let published_local_count = match &call.target {
        RegionCallTarget::Function { name, .. }
            if name
                .trim_start_matches('\\')
                .eq_ignore_ascii_case("compact") =>
        {
            local_count
        }
        _ => 0,
    };
    let local_slot_size = std::mem::size_of::<crate::JitAbiSlot>();
    let local_slots_ptr = if published_local_count == 0 {
        builder.ins().iconst(pointer_type, 0)
    } else {
        let bytes = local_slot_size
            .checked_mul(published_local_count as usize)
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_CALL_LOCALS",
                    "native call local table exceeds stack-slot limits",
                )
            })?;
        let pointer = allocate_native_frame_storage(
            module,
            builder,
            native_frame_operations,
            bytes,
            3,
            result_out,
        );
        for index in 0..published_local_count {
            let base = i32::try_from(index as usize * local_slot_size).unwrap_or(i32::MAX);
            let tag = builder.ins().iconst(types::I32, 3);
            builder.ins().store(MemFlagsData::new(), tag, pointer, base);
            let flags = builder.ins().iconst(types::I32, 0);
            builder
                .ins()
                .store(MemFlagsData::new(), flags, pointer, base + 4);
            let value = use_local_variable(builder, locals, LocalId::new(index))?;
            builder
                .ins()
                .store(MemFlagsData::new(), value, pointer, base + 8);
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
    let frame_ptr = allocate_native_frame_storage(
        module,
        builder,
        native_frame_operations,
        frame_size,
        3,
        result_out,
    );
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
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, source_block_id),
        source_block.raw(),
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, source_instruction_id),
        instruction.id.raw(),
    );
    let result_slot = match call.result {
        RegionCallResult::Register(register) => register.raw(),
        RegionCallResult::ReferenceLocal(local) => local.raw(),
        RegionCallResult::Discard => u32::MAX,
    };
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, result_slot),
        result_slot,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, local_count),
        published_local_count,
    );
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, argument_count),
        u32::try_from(argument_count).unwrap_or(u32::MAX),
    );
    let direct_builtin_helper = stable_builtin_helper_id(&call.target);
    let direct_external = direct_builtin_helper.is_none()
        && matches!(&call.target, RegionCallTarget::Function { name, function: None }
        if function_params.values().any(|(candidate, ..)| {
            candidate.trim_start_matches('\\').eq_ignore_ascii_case(name.trim_start_matches('\\'))
        }));
    let frame_flags = if call.caller_strict_types {
        crate::JitNativeCallFrame::FLAG_STRICT_TYPES
    } else {
        0
    } | if call.returns_by_reference {
        crate::JitNativeCallFrame::FLAG_RETURN_REFERENCE
    } else {
        0
    } | if direct_builtin_helper.is_some() {
        crate::JitNativeCallFrame::FLAG_DIRECT_BUILTIN
    } else {
        0
    } | if direct_external {
        crate::JitNativeCallFrame::FLAG_DIRECT_EXTERNAL
    } else {
        0
    };
    store_i32(
        builder,
        std::mem::offset_of!(crate::JitNativeCallFrame, flags),
        frame_flags,
    );
    builder.ins().store(
        MemFlagsData::new(),
        local_slots_ptr,
        frame_ptr,
        std::mem::offset_of!(crate::JitNativeCallFrame, local_slots) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        arguments_ptr,
        frame_ptr,
        std::mem::offset_of!(crate::JitNativeCallFrame, arguments) as i32,
    );
    if let RegionCallTarget::StaticMethod { class_name, .. } = &call.target
        && matches!(
            class_name.to_ascii_lowercase().as_str(),
            "self" | "parent" | "static"
        )
        && let Ok(receiver) = use_local_variable(builder, locals, LocalId::new(0))
    {
        builder.ins().store(
            MemFlagsData::new(),
            receiver,
            frame_ptr,
            std::mem::offset_of!(crate::JitNativeCallFrame, receiver_handle) as i32,
        );
    }
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
        direct_builtin_helper.unwrap_or(target_function),
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
    let vm_context = builder.ins().iconst(types::I64, 0);
    let helper_call =
        call_native_helper(module, builder, helper, &[vm_context, frame_ptr, out_ptr]);
    let status = builder.inst_results(helper_call)[0];
    let ok = builder.create_block();
    let side_exit = builder.create_block();
    let expected_return_status = if call.returns_by_reference {
        crate::JitCallStatus::RETURN_REFERENCE.0
    } else {
        crate::JitCallStatus::RETURN.0
    };
    let is_ok = builder
        .ins()
        .icmp_imm(IntCC::Equal, status, i64::from(expected_return_status));
    builder.ins().brif(is_ok, ok, &[], side_exit, &[]);
    builder.switch_to_block(side_exit);
    publish_native_call_state(
        builder,
        deopt_out,
        function,
        local_count,
        instruction,
        locals,
        native_version,
    )?;
    publish_native_register_state(builder, deopt_out, registers);
    let control_value = builder.ins().stack_load(types::I64, out_slot, 16);
    builder
        .ins()
        .store(MemFlagsData::new(), control_value, result_out, 0);
    release_native_frame_storage(
        module,
        builder,
        native_frame_operations,
        frame_ptr,
        result_out,
    )?;
    if published_local_count != 0 {
        release_native_frame_storage(
            module,
            builder,
            native_frame_operations,
            local_slots_ptr,
            result_out,
        )?;
    }
    if argument_count != 0 {
        release_native_frame_storage(
            module,
            builder,
            native_frame_operations,
            arguments_ptr,
            result_out,
        )?;
    }
    builder.ins().return_(&[status]);
    builder.switch_to_block(ok);
    for (local, (original, flag_offsets)) in speculative_local_bindings {
        let mut actual_reference = None;
        for offset in flag_offsets {
            let flags = builder
                .ins()
                .load(types::I32, MemFlagsData::new(), arguments_ptr, offset);
            let mask = builder.ins().iconst(
                types::I32,
                i64::from(crate::JitNativeArgFlags::BY_REFERENCE.0),
            );
            let bound = builder.ins().band(flags, mask);
            actual_reference =
                Some(actual_reference.map_or(bound, |current| builder.ins().bor(current, bound)));
        }
        let is_actual_reference = builder.ins().icmp_imm(
            IntCC::NotEqual,
            actual_reference.expect("speculative binding retains one argument flag"),
            0,
        );
        let keep = builder.create_block();
        let restore = builder.create_block();
        let merge = builder.create_block();
        builder.append_block_param(merge, types::I64);
        let current = use_local_variable(builder, locals, local)?;
        builder
            .ins()
            .brif(is_actual_reference, keep, &[], restore, &[]);

        builder.switch_to_block(keep);
        let keep_args = [current.into()];
        builder.ins().jump(merge, &keep_args);

        builder.switch_to_block(restore);
        let restore_without_release = builder.create_block();
        let restore_after_release = builder.create_block();
        let reuses_original = builder.ins().icmp(IntCC::Equal, current, original);
        builder.ins().brif(
            reuses_original,
            restore_without_release,
            &[],
            restore_after_release,
            &[],
        );

        builder.switch_to_block(restore_after_release);
        let _ = lower_native_value_operation(
            module,
            builder,
            native_value_lifecycle_helper,
            native_dim_operation(1, function, instruction.continuation_id),
            &[current],
            result_out,
        )?;
        let restore_args = [original.into()];
        builder.ins().jump(merge, &restore_args);

        builder.switch_to_block(restore_without_release);
        builder.ins().jump(merge, &restore_args);

        builder.switch_to_block(merge);
        builder.def_var(
            local_variable(locals, local)?,
            builder.block_params(merge)[0],
        );
    }
    for argument in consumed_arguments {
        let _ = lower_native_value_operation(
            module,
            builder,
            native_value_lifecycle_helper,
            native_dim_operation(1, function, instruction.continuation_id),
            &[argument],
            result_out,
        )?;
    }
    let value = builder.ins().stack_load(types::I64, out_slot, 16);
    release_native_frame_storage(
        module,
        builder,
        native_frame_operations,
        frame_ptr,
        result_out,
    )?;
    if published_local_count != 0 {
        release_native_frame_storage(
            module,
            builder,
            native_frame_operations,
            local_slots_ptr,
            result_out,
        )?;
    }
    if argument_count != 0 {
        release_native_frame_storage(
            module,
            builder,
            native_frame_operations,
            arguments_ptr,
            result_out,
        )?;
    }
    match call.result {
        RegionCallResult::Register(register) => {
            define_region_register(builder, register_variables, registers, register, value)?;
        }
        RegionCallResult::ReferenceLocal(local) => {
            builder.def_var(local_variable(locals, local)?, value);
        }
        RegionCallResult::Discard => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_checked_region_binary(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    op: RegionBinaryOp,
    lhs: ir::Value,
    rhs: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    instruction: &RegionInstruction,
    locals: &BTreeMap<LocalId, Variable>,
    registers: &BTreeMap<RegId, Variable>,
    native_version: u32,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (result, overflow) = match op {
        RegionBinaryOp::Add => builder.ins().sadd_overflow(lhs, rhs),
        RegionBinaryOp::Sub => builder.ins().ssub_overflow(lhs, rhs),
        RegionBinaryOp::Mul => builder.ins().smul_overflow(lhs, rhs),
        RegionBinaryOp::BitAnd => {
            return Ok(builder.ins().band(lhs, rhs));
        }
        RegionBinaryOp::BitOr => {
            return Ok(builder.ins().bor(lhs, rhs));
        }
        RegionBinaryOp::BitXor => {
            return Ok(builder.ins().bxor(lhs, rhs));
        }
        RegionBinaryOp::ShiftLeft | RegionBinaryOp::ShiftRight => {
            let slow_block = builder.create_block();
            let fast_block = builder.create_block();
            let merge_block = builder.create_block();
            builder.append_block_param(merge_block, types::I64);
            let negative = builder.ins().icmp_imm(IntCC::SignedLessThan, rhs, 0);
            builder
                .ins()
                .brif(negative, slow_block, &[], fast_block, &[]);

            builder.switch_to_block(slow_block);
            let slow = lower_native_binary_operation(
                module,
                builder,
                helper,
                native_binary_opcode(op),
                lhs,
                rhs,
                result_out,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                registers,
                native_version,
            )?;
            builder.ins().jump(merge_block, &[slow.into()]);

            builder.switch_to_block(fast_block);
            let large = builder
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, rhs, 64);
            let shifted = if op == RegionBinaryOp::ShiftLeft {
                builder.ins().ishl(lhs, rhs)
            } else {
                builder.ins().sshr(lhs, rhs)
            };
            let out_of_range = if op == RegionBinaryOp::ShiftLeft {
                builder.ins().iconst(types::I64, 0)
            } else {
                builder.ins().sshr_imm(lhs, 63)
            };
            let fast = builder.ins().select(large, out_of_range, shifted);
            builder.ins().jump(merge_block, &[fast.into()]);
            builder.switch_to_block(merge_block);
            return Ok(builder.block_params(merge_block)[0]);
        }
        RegionBinaryOp::Div
        | RegionBinaryOp::Mod
        | RegionBinaryOp::Concat
        | RegionBinaryOp::Pow => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                format!("binary operation {op:?} requires its typed runtime helper"),
            ));
        }
    };
    let overflow_block = builder.create_block();
    let ok_block = builder.create_block();
    let merge_block = builder.create_block();
    builder.append_block_param(merge_block, types::I64);
    builder
        .ins()
        .brif(overflow, overflow_block, &[], ok_block, &[]);
    builder.switch_to_block(overflow_block);
    let slow = lower_native_binary_operation(
        module,
        builder,
        helper,
        native_binary_opcode(op),
        lhs,
        rhs,
        result_out,
        deopt_out,
        function,
        local_count,
        instruction,
        locals,
        registers,
        native_version,
    )?;
    let slow_args = [slow.into()];
    builder.ins().jump(merge_block, &slow_args);
    builder.switch_to_block(ok_block);
    let result_args = [result.into()];
    builder.ins().jump(merge_block, &result_args);
    builder.switch_to_block(merge_block);
    Ok(builder.block_params(merge_block)[0])
}

fn region_compare_intcc(op: RegionCompareOpCode) -> IntCC {
    match op {
        RegionCompareOpCode::Equal => IntCC::Equal,
        RegionCompareOpCode::NotEqual => IntCC::NotEqual,
        RegionCompareOpCode::Identical => IntCC::Equal,
        RegionCompareOpCode::NotIdentical => IntCC::NotEqual,
        RegionCompareOpCode::Less => IntCC::SignedLessThan,
        RegionCompareOpCode::LessEqual => IntCC::SignedLessThanOrEqual,
        RegionCompareOpCode::Greater => IntCC::SignedGreaterThan,
        RegionCompareOpCode::GreaterEqual => IntCC::SignedGreaterThanOrEqual,
        RegionCompareOpCode::Spaceship => IntCC::Equal,
    }
}

#[cfg(test)]
mod tests;
