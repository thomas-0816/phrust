//! Exhaustive Region IR lowering for the mandatory Cranelift native compiler.

use crate::code_manager::{ManagedCompileError, NativeCompileAdmission};
use crate::region_ir::{
    BaselineRegionBuilder, CompileMetadata, ExecutableValueFlow, NativeCompilerTier,
    RegionBinaryOp, RegionCallResult, RegionCallTarget, RegionCastOp, RegionCompareOpCode,
    RegionGraph, RegionInstruction, RegionInstructionKind, RegionNativeCall, RegionNativeControl,
    RegionNativeDynamicCode, RegionNativeSuspend, RegionOperand, RegionTerminator, RegionUnaryOp,
    SsaOwnership, SsaValueClass, SsaValueFact, value_copy_requires_retain, value_release_required,
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
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Switch, Variable};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module, ModuleReloc, ModuleRelocTarget};
use php_ir::{BlockId, FunctionId, IrConstant, IrSpan, IrUnit, LocalId, RegId};
use std::collections::BTreeMap;
use std::fmt;

type NativeFunctionMetadata = (String, Vec<php_ir::IrParam>, bool, usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BoundedInlineValue {
    Constant(RegionOperand),
    Argument { index: usize, arity: usize },
}

fn bounded_inline_call_operand(
    call: &RegionNativeCall,
    value: BoundedInlineValue,
) -> Option<RegionOperand> {
    match value {
        BoundedInlineValue::Constant(value) => call.operands.is_empty().then_some(value),
        BoundedInlineValue::Argument { index, arity } => (call.argument_operand_offset == 0
            && call.operands.len() == call.args.len()
            && call.args.len() == arity
            && call.args.iter().all(|argument| {
                argument.name.is_none()
                    && !argument.unpack
                    && argument.value_kind == php_ir::instruction::IrCallArgValueKind::Direct
                    && argument.by_ref_local.is_none()
                    && argument.by_ref_dim.is_none()
                    && argument.by_ref_property.is_none()
                    && argument.by_ref_property_dim.is_none()
            }))
        .then(|| call.operands.get(index).copied().flatten())
        .flatten(),
    }
}
use std::time::Instant;

mod baseline_streaming;
mod call_metadata;
mod dynamic_code;
mod executable_region;
mod fallback_helpers;
mod module_layout;
mod native_linkage;
mod terminators;
mod value_lowering;

use module_layout::split_oversized_region_blocks;
pub use module_layout::{NATIVE_FRAGMENT_PLAN_SCHEMA_VERSION, NativeCompilePlan};
pub use native_linkage::{
    NativeFunctionKey, NativeFunctionTier, NativeIndirectionCell, NativeIndirectionState,
    native_function_key,
};

/// Stable persistent identity of the selected native lowering contract.
#[must_use]
pub const fn native_compiler_mode_identity(optimizing: bool) -> &'static str {
    if optimizing {
        native_linkage::OPTIMIZING_FUNCTION_SPECIALIZATION
    } else {
        native_linkage::BASELINE_FUNCTION_SPECIALIZATION
    }
}

use call_metadata::*;
use dynamic_code::*;
use fallback_helpers::*;
use terminators::{lower_owned_frame_locals, lower_region_terminator};
use value_lowering::{encode_native_bool, lower_direct_cast, lower_direct_compare, scalar_truthy};

#[derive(Clone, Debug, Eq, PartialEq)]
struct NativeScalarRegionCompileResult {
    handle: JitFunctionHandle,
    code_bytes: u64,
    clif_blocks: Option<usize>,
    maximum_pre_regalloc: Option<executable_region::PreRegallocMetrics>,
    maximum_temporary_cache_entries: Option<usize>,
    fragment_frame_slots: usize,
    fragment_shared_register_slots: usize,
    fragment_scratch_register_slots: usize,
    pre_regalloc_replans: usize,
    fast_path_hits: u64,
    has_control_flow: bool,
    compilation_mode: baseline_streaming::NativeCompilationMode,
    plan: NativeCompilePlan,
}

#[derive(Clone, Copy, Debug)]
struct NativeTerminalExit {
    block: ir::Block,
}

#[derive(Clone, Copy, Debug)]
struct NativeStreamingCallExit {
    block: ir::Block,
}

#[derive(Clone, Copy, Debug)]
struct NativeHelper {
    function: FuncId,
    terminal_exit: Option<NativeTerminalExit>,
    inline_runtime_view: bool,
}

impl NativeHelper {
    fn with_terminal_exit(self, terminal_exit: NativeTerminalExit) -> Self {
        Self {
            terminal_exit: Some(terminal_exit),
            ..self
        }
    }

    fn terminal_exit(self) -> Result<NativeTerminalExit, CraneliftLoweringError> {
        self.terminal_exit.ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_COLD_EXIT",
                "fallible native helper has no function-local terminal exit",
            )
        })
    }

    fn with_inline_runtime_view(self) -> Self {
        Self {
            inline_runtime_view: true,
            ..self
        }
    }
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
    type_predicate: Option<NativeHelper>,
    stable_length: Option<NativeHelper>,
    runtime_fatal: Option<NativeHelper>,
    execution_poll: Option<NativeHelper>,
}

impl NativeOperationFunctions {
    fn with_terminal_exit(mut self, terminal_exit: NativeTerminalExit) -> Self {
        macro_rules! bind {
            ($($field:ident),+ $(,)?) => {
                $(self.$field = self.$field.map(|helper| helper.with_terminal_exit(terminal_exit));)+
            };
        }
        bind!(
            function_resolve,
            frame_alloc,
            frame_release,
            unary,
            binary,
            compare,
            cast,
            echo,
            local_fetch,
            local_store,
            value_lifecycle,
            reference_bind,
            argument_check,
            return_check,
            exception_new,
            array_new,
            object_new,
            property_fetch,
            property_assign,
            object_clone,
            object_clone_with,
            array_insert,
            array_fetch,
            array_unset,
            array_spread,
            foreach_init,
            foreach_next,
            foreach_cleanup,
            constant_fetch,
            truthy,
            type_predicate,
            stable_length,
            runtime_fatal,
            execution_poll,
        );
        self
    }
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

fn native_php_entry_signature(module: &JITModule) -> Signature {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    #[cfg(target_arch = "x86_64")]
    {
        signature.call_conv = CallConv::Tail;
    }
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(types::I32));
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    signature
}

fn compile_managed_native(
    request: &JitCompileRequest,
    function: FunctionId,
    function_key: NativeFunctionKey,
    admission: NativeCompileAdmission,
    specialization: &str,
    helpers: &[(&str, usize)],
    compile: impl FnOnce(
        &mut JITModule,
        &mut cranelift_codegen::Context,
        &mut FunctionBuilderContext,
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
    let config_hash = if request.config_hash == 0 {
        identity.feature_fingerprint ^ u64::from(request.opt_level)
    } else {
        request.config_hash
    };
    let mut helper_binding_hash = 0xcbf2_9ce4_8422_2325_u64;
    for (symbol, address) in helpers {
        for byte in symbol.as_bytes().iter().chain(address.to_le_bytes().iter()) {
            helper_binding_hash ^= u64::from(*byte);
            helper_binding_hash = helper_binding_hash.wrapping_mul(0x0000_0100_0000_01b3);
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
        helper_abi_hash: JIT_RUNTIME_ABI_HASH
            ^ crate::JIT_HELPER_REGISTRY_ABI_HASH
            ^ php_runtime::api::NATIVE_OPERATION_ABI_HASH,
        helper_binding_hash,
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
        .compile_once_with_scratch_admission(key, Some(function_key), admission, helpers, compile)
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
        .deployment_identity
        .clone()
        .unwrap_or_else(|| crate::stable_ir_fingerprint(unit));
    // Publishing the unit entry establishes every symbolic function cell for
    // the deployment. A later function-on-demand miss must only ensure its own
    // cell exists; rebuilding the complete declaration set for every called
    // function makes lazy compilation repeatedly scale with source-unit size.
    let declaration_ids: Box<dyn Iterator<Item = FunctionId>> = if function == unit.entry {
        Box::new(
            (0..unit.functions.len())
                .filter_map(|index| u32::try_from(index).ok())
                .map(FunctionId::new),
        )
    } else {
        Box::new(std::iter::once(function))
    };
    let declarations = declaration_ids.filter_map(|function_id| {
        let function = unit.functions.get(function_id.index())?;
        Some(native_function_key(
            deployment_unit.clone(),
            function_id.raw(),
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
    let region = split_oversized_region_blocks(region);
    if let Err(error) = region.verify() {
        return NativeCompileOutcome::skipped(
            JitCompileStatus::Rejected {
                reason: "JIT_CRANELIFT_FRAGMENT_NORMALIZE".to_owned(),
            },
            format!("normalized native Region IR failed verification: {error}"),
        );
    }
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
        region,
        plan,
        request.runtime_helpers,
        request.compile,
    ) {
        Ok(compiled) => {
            let elapsed = start.elapsed().as_nanos().try_into().unwrap_or(u64::MAX);
            NativeCompileOutcome::compiled(
                compiled.handle,
                format!(
                    "Cranelift baseline Region IR `{}` function={} compilation_mode={} abi_hash={} code_bytes={} clif_blocks={} max_fragment_clif_blocks={} max_fragment_clif_values={} max_fragment_clif_instructions={} max_fragment_block_parameters={} max_fragment_clif_loads={} max_fragment_clif_stores={} max_fragment_loads_per_source_instruction_milli={} max_fragment_stores_per_source_instruction_milli={} max_temporary_cache_entries={} fragment_frame_slots={} fragment_shared_register_slots={} fragment_scratch_register_slots={} pre_regalloc_replans={} fast_path_hits={} control_flow={} plan_ir_instructions={} plan_php_blocks={} plan_estimated_clif_blocks={} plan_virtual_values={} plan_safepoints={} plan_live_sum={} plan_fragments={} plan_max_fragment_blocks={} plan_max_fragment_instructions={} plan_max_fragment_estimated_clif_blocks={}",
                    request.compile.region_id,
                    function.raw(),
                    compiled.compilation_mode.as_str(),
                    JIT_RUNTIME_ABI_HASH,
                    compiled.code_bytes,
                    compiled
                        .clif_blocks
                        .map_or_else(|| "cached".to_owned(), |blocks| blocks.to_string()),
                    compiled
                        .maximum_pre_regalloc
                        .map_or(0, |metrics| metrics.blocks),
                    compiled
                        .maximum_pre_regalloc
                        .map_or(0, |metrics| metrics.values),
                    compiled
                        .maximum_pre_regalloc
                        .map_or(0, |metrics| metrics.instructions),
                    compiled
                        .maximum_pre_regalloc
                        .map_or(0, |metrics| metrics.block_parameters),
                    compiled
                        .maximum_pre_regalloc
                        .map_or(0, |metrics| metrics.loads),
                    compiled
                        .maximum_pre_regalloc
                        .map_or(0, |metrics| metrics.stores),
                    compiled
                        .maximum_pre_regalloc
                        .map_or(0, |metrics| { metrics.loads_per_source_instruction_milli }),
                    compiled
                        .maximum_pre_regalloc
                        .map_or(0, |metrics| { metrics.stores_per_source_instruction_milli }),
                    compiled.maximum_temporary_cache_entries.unwrap_or(0),
                    compiled.fragment_frame_slots,
                    compiled.fragment_shared_register_slots,
                    compiled.fragment_scratch_register_slots,
                    compiled.pre_regalloc_replans,
                    compiled.fast_path_hits,
                    compiled.has_control_flow,
                    compiled.plan.ir_instructions,
                    compiled.plan.php_cfg_blocks,
                    compiled.plan.estimated_clif_blocks,
                    compiled.plan.virtual_values,
                    compiled.plan.safepoint_count,
                    compiled.plan.safepoint_live_set_sum,
                    compiled.plan.fragments.len(),
                    compiled
                        .plan
                        .fragments
                        .iter()
                        .map(|fragment| fragment.blocks.len())
                        .max()
                        .unwrap_or(0),
                    compiled
                        .plan
                        .fragments
                        .iter()
                        .map(|fragment| fragment.ir_instructions)
                        .max()
                        .unwrap_or(0),
                    compiled
                        .plan
                        .fragments
                        .iter()
                        .map(|fragment| fragment.estimated_clif_blocks)
                        .max()
                        .unwrap_or(0),
                ),
                compiled.code_bytes,
                elapsed.max(1),
            )
        }
        Err(error)
            if request.compile.opt_level != 0
                && error.code == "JIT_CRANELIFT_PRE_REGALLOC_BUDGET" =>
        {
            let mut baseline_compile = request.compile.clone();
            baseline_compile.opt_level = 0;
            baseline_compile.region_id = format!("{}.baseline", baseline_compile.region_id);
            let baseline_request = NativeCompileRequest {
                compile: &baseline_compile,
                unit: request.unit,
                function: request.function,
                runtime_helpers: request.runtime_helpers,
            };
            let mut outcome = compile_authoritative_region(&baseline_request);
            outcome.diagnostics.insert(
                0,
                format!(
                    "optimizing compile exceeded its exact backend budget and used baseline code: {error}"
                ),
            );
            if outcome.handle.is_some() {
                outcome.compile_time_nanos = start
                    .elapsed()
                    .as_nanos()
                    .try_into()
                    .unwrap_or(u64::MAX)
                    .max(1);
            }
            outcome
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

fn runtime_helper_abi_hash(_helpers: crate::JitRuntimeHelperAddresses) -> u64 {
    JIT_RUNTIME_ABI_HASH
        ^ crate::JIT_HELPER_REGISTRY_ABI_HASH
        ^ php_runtime::api::NATIVE_OPERATION_ABI_HASH
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
) -> Result<BTreeMap<BlockId, ir::Block>, CraneliftLoweringError> {
    let mut blocks = BTreeMap::new();
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
        blocks.insert(region_block.id, builder.create_block());
    }
    Ok(blocks)
}

fn cranelift_block(
    blocks: &BTreeMap<BlockId, ir::Block>,
    block_id: BlockId,
) -> Result<ir::Block, CraneliftLoweringError> {
    blocks.get(&block_id).copied().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_HELPER_CONTROL_FLOW",
            format!("target block {} is outside the lowered CFG", block_id.raw()),
        )
    })
}

#[derive(Clone, Copy, Debug)]
enum NativeLocalStorage {
    Variable(Variable),
    FrameSlot { frame: ir::Value, offset: i32 },
}

type NativeLocalMap = BTreeMap<LocalId, NativeLocalStorage>;

/// Baseline values are materialized in compact fragment-frame slots instead
/// of frontend SSA variables. `Cached` is deliberately block-local: the
/// authoritative value has already been written to its frame slot when one
/// exists, so dropping the cache at a merge or helper boundary cannot lose
/// state. The optimizing tier continues to use Cranelift frontend variables.
#[derive(Clone, Copy, Debug)]
enum NativeRegisterStorage {
    Variable(Variable),
    FrameSlot {
        frame: ir::Value,
        offset: i32,
        type_: ir::Type,
    },
    Transient {
        type_: ir::Type,
    },
    Cached(ir::Value),
}

type NativeRegisterMap = BTreeMap<RegId, NativeRegisterStorage>;

fn use_region_register(
    builder: &mut FunctionBuilder<'_>,
    registers: &NativeRegisterMap,
    register: RegId,
) -> Result<ir::Value, CraneliftLoweringError> {
    match registers.get(&register).copied() {
        Some(NativeRegisterStorage::Variable(variable)) => {
            builder.try_use_var(variable).map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
                    format!("register {} has no native value: {error}", register.raw()),
                )
            })
        }
        Some(NativeRegisterStorage::FrameSlot {
            frame,
            offset,
            type_,
        }) => {
            let value = builder
                .ins()
                .load(types::I64, MemFlagsData::new(), frame, offset);
            Ok(if type_ == types::I64 {
                value
            } else {
                builder.ins().ireduce(type_, value)
            })
        }
        Some(NativeRegisterStorage::Cached(value)) => Ok(value),
        Some(NativeRegisterStorage::Transient { .. }) | None => Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
            format!(
                "register {} has no dominating baseline value or frame slot",
                register.raw()
            ),
        )),
    }
}

fn local_storage(
    locals: &NativeLocalMap,
    local: LocalId,
) -> Result<NativeLocalStorage, CraneliftLoweringError> {
    locals.get(&local).copied().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_LOCAL",
            format!("local {} has not been declared", local.raw()),
        )
    })
}

fn use_local_variable(
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    local: LocalId,
) -> Result<ir::Value, CraneliftLoweringError> {
    match local_storage(locals, local)? {
        NativeLocalStorage::Variable(variable) => builder.try_use_var(variable).map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_MISSING_LOCAL",
                format!("local {} has no dominating value: {error}", local.raw()),
            )
        }),
        NativeLocalStorage::FrameSlot { frame, offset } => {
            Ok(builder
                .ins()
                .load(types::I64, MemFlagsData::new(), frame, offset))
        }
    }
}

fn define_local_variable(
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    local: LocalId,
    value: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    match local_storage(locals, local)? {
        NativeLocalStorage::Variable(variable) => builder.def_var(variable, value),
        NativeLocalStorage::FrameSlot { frame, offset } => {
            builder
                .ins()
                .store(MemFlagsData::new(), value, frame, offset);
        }
    }
    Ok(())
}

fn lower_region_operand(
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    operand: RegionOperand,
) -> Result<ir::Value, CraneliftLoweringError> {
    match operand {
        RegionOperand::Register(reg) => use_region_register(builder, registers, reg),
        RegionOperand::I64(value) => Ok(builder.ins().iconst(types::I64, value)),
        RegionOperand::Constant(constant) => Ok(builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(constant))),
        RegionOperand::Local(local) => use_local_variable(builder, locals, local),
    }
}

fn lower_ir_operand(
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    operand: php_ir::Operand,
) -> Result<ir::Value, CraneliftLoweringError> {
    match operand {
        php_ir::Operand::Register(register) => use_region_register(builder, registers, register),
        php_ir::Operand::Local(local) => use_local_variable(builder, locals, local),
        php_ir::Operand::Constant(constant) => Ok(builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(constant.raw()))),
    }
}

fn define_region_register(
    builder: &mut FunctionBuilder<'_>,
    register_variables: &NativeRegisterMap,
    registers: &mut NativeRegisterMap,
    register: RegId,
    value: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    let storage = register_variables.get(&register).copied().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_MISSING_REGISTER",
            format!("register {} has no declared native storage", register.raw()),
        )
    })?;
    match storage {
        NativeRegisterStorage::Variable(variable) => {
            builder.def_var(variable, value);
            registers.insert(register, NativeRegisterStorage::Variable(variable));
        }
        NativeRegisterStorage::FrameSlot {
            frame,
            offset,
            type_,
        } => {
            let stored = if type_ == types::I64 {
                value
            } else {
                builder.ins().uextend(types::I64, value)
            };
            builder
                .ins()
                .store(MemFlagsData::new(), stored, frame, offset);
            registers.insert(register, NativeRegisterStorage::Cached(value));
        }
        NativeRegisterStorage::Transient { type_ } => {
            debug_assert_eq!(builder.func.dfg.value_type(value), type_);
            registers.insert(register, NativeRegisterStorage::Cached(value));
        }
        NativeRegisterStorage::Cached(_) => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_REGISTER_STORAGE",
                format!("register {} declaration is already cached", register.raw()),
            ));
        }
    }
    Ok(())
}

fn require_native_operation_ok(
    builder: &mut FunctionBuilder<'_>,
    status: ir::Value,
    terminal_exit: NativeTerminalExit,
) -> Result<(), CraneliftLoweringError> {
    let ok = builder.create_block();
    let is_ok = builder.ins().icmp_imm(IntCC::Equal, status, 0);
    let empty = builder.ins().iconst(types::I64, 0);
    builder.ins().brif(
        is_ok,
        ok,
        &[],
        terminal_exit.block,
        &[status.into(), empty.into()],
    );
    builder.switch_to_block(ok);
    Ok(())
}

fn allocate_native_frame_storage(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operations: NativeOperationFunctions,
    bytes: u32,
    alignment_log2: u8,
    _result_out: ir::Value,
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
    let non_null = builder.ins().icmp_imm(IntCC::NotEqual, pointer, 0);
    let empty = builder.ins().iconst(types::I64, 0);
    let status = builder
        .ins()
        .iconst(types::I32, i64::from(crate::JitCallStatus::RUNTIME_ERROR.0));
    let terminal_exit = helper
        .terminal_exit
        .expect("function-local frame allocator must have a terminal exit");
    builder.ins().brif(
        non_null,
        allocated,
        &[],
        terminal_exit.block,
        &[status.into(), empty.into()],
    );
    builder.switch_to_block(allocated);
    pointer
}

fn allocate_native_stack_storage(
    builder: &mut FunctionBuilder<'_>,
    pointer_type: ir::Type,
    bytes: u32,
    alignment_log2: u8,
) -> ir::Value {
    let slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        bytes,
        alignment_log2,
    ));
    builder.ins().stack_addr(pointer_type, slot, 0)
}

fn release_native_frame_storage(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operations: NativeOperationFunctions,
    pointer: ir::Value,
    _result_out: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    let Some(helper) = operations.frame_release else {
        return Ok(());
    };
    let context = builder.ins().iconst(types::I64, 0);
    let call = call_native_helper(module, builder, helper, &[context, pointer]);
    require_native_operation_ok(
        builder,
        builder.inst_results(call)[0],
        helper.terminal_exit()?,
    )
}

fn require_native_value_operation_ok(
    builder: &mut FunctionBuilder<'_>,
    status: ir::Value,
    terminal_exit: NativeTerminalExit,
    value: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    let ok = builder.create_block();
    let is_ok = builder.ins().icmp_imm(IntCC::Equal, status, 0);
    builder.ins().brif(
        is_ok,
        ok,
        &[],
        terminal_exit.block,
        &[status.into(), value.into()],
    );
    builder.switch_to_block(ok);
    Ok(())
}

fn native_local_mask_words(live_locals: &[LocalId]) -> [u64; crate::JIT_DEOPT_LOCAL_MASK_WORDS] {
    let mut masks = [0_u64; crate::JIT_DEOPT_LOCAL_MASK_WORDS];
    for local in live_locals {
        let index = local.index();
        if index < crate::JIT_DEOPT_MAX_SLOTS {
            masks[index / u64::BITS as usize] |= 1_u64 << (index % u64::BITS as usize);
        }
    }
    masks
}

fn publish_native_local_masks(
    builder: &mut FunctionBuilder<'_>,
    state_out: ir::Value,
    live_locals: &[LocalId],
) {
    let masks = native_local_mask_words(live_locals);
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

#[derive(Clone, Copy)]
enum NativeLocalCopyDirection {
    FrameToState,
    StateToFrame,
}

fn copy_native_local_state_values(
    builder: &mut FunctionBuilder<'_>,
    state: ir::Value,
    locals: &NativeLocalMap,
    live_locals: &[LocalId],
    direction: NativeLocalCopyDirection,
) -> Result<bool, CraneliftLoweringError> {
    let mut slots = Vec::with_capacity(live_locals.len());
    let mut frame = None;
    for local in live_locals {
        let NativeLocalStorage::FrameSlot {
            frame: local_frame,
            offset: frame_offset,
        } = local_storage(locals, *local)?
        else {
            return Ok(false);
        };
        if frame.is_some_and(|frame| frame != local_frame) {
            return Ok(false);
        }
        frame = Some(local_frame);
        let state_offset = std::mem::offset_of!(crate::JitDeoptState, slots)
            .saturating_add(local.index().saturating_mul(8));
        slots.push((frame_offset, state_offset as i32));
    }
    let Some(frame) = frame else {
        return Ok(true);
    };

    let mut start = 0;
    while start < slots.len() {
        let mut end = start + 1;
        while end < slots.len()
            && slots[end].0 == slots[end - 1].0.saturating_add(8)
            && slots[end].1 == slots[end - 1].1.saturating_add(8)
        {
            end += 1;
        }
        let (frame_base, state_base) = slots[start];
        let mut byte_offset = 0_i32;
        let mut bytes = (end - start).saturating_mul(8);
        while bytes >= 16 {
            let (source, source_offset, destination, destination_offset) = match direction {
                NativeLocalCopyDirection::FrameToState => (frame, frame_base, state, state_base),
                NativeLocalCopyDirection::StateToFrame => (state, state_base, frame, frame_base),
            };
            let value = builder.ins().load(
                types::I8X16,
                MemFlagsData::new(),
                source,
                source_offset.saturating_add(byte_offset),
            );
            builder.ins().store(
                MemFlagsData::new(),
                value,
                destination,
                destination_offset.saturating_add(byte_offset),
            );
            byte_offset = byte_offset.saturating_add(16);
            bytes -= 16;
        }
        if bytes == 8 {
            let (source, source_offset, destination, destination_offset) = match direction {
                NativeLocalCopyDirection::FrameToState => (frame, frame_base, state, state_base),
                NativeLocalCopyDirection::StateToFrame => (state, state_base, frame, frame_base),
            };
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                source,
                source_offset.saturating_add(byte_offset),
            );
            builder.ins().store(
                MemFlagsData::new(),
                value,
                destination,
                destination_offset.saturating_add(byte_offset),
            );
        }
        start = end;
    }
    Ok(true)
}

fn restore_native_local_state_values(
    builder: &mut FunctionBuilder<'_>,
    state: ir::Value,
    locals: &NativeLocalMap,
    live_locals: &[LocalId],
) -> Result<(), CraneliftLoweringError> {
    if copy_native_local_state_values(
        builder,
        state,
        locals,
        live_locals,
        NativeLocalCopyDirection::StateToFrame,
    )? {
        return Ok(());
    }
    for local in live_locals {
        let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
            .saturating_add(local.index().saturating_mul(8));
        let value = builder
            .ins()
            .load(types::I64, MemFlagsData::new(), state, offset as i32);
        define_local_variable(builder, locals, *local, value)?;
    }
    Ok(())
}

fn publish_native_call_state(
    builder: &mut FunctionBuilder<'_>,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    instruction: &RegionInstruction,
    locals: &NativeLocalMap,
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
    if !copy_native_local_state_values(
        builder,
        deopt_out,
        locals,
        &instruction.live_locals,
        NativeLocalCopyDirection::FrameToState,
    )? {
        for local in &instruction.live_locals {
            let value = use_local_variable(builder, locals, *local)?;
            let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                .saturating_add(local.index().saturating_mul(8));
            builder
                .ins()
                .store(MemFlagsData::new(), value, deopt_out, offset as i32);
        }
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
    registers: &NativeRegisterMap,
    live_registers: &[RegId],
) {
    let encodable_registers = live_registers
        .iter()
        .take(crate::JIT_DEOPT_MAX_REGISTERS)
        .filter_map(|register| {
            use_region_register(builder, registers, *register)
                .ok()
                .map(|value| (*register, value))
        })
        .collect::<Vec<_>>();
    let initialized_count = encodable_registers.len();
    let initialized_mask = if initialized_count >= u64::BITS as usize {
        u64::MAX
    } else {
        1_u64
            .checked_shl(u32::try_from(initialized_count).unwrap_or(u32::MAX))
            .unwrap_or(0)
            .saturating_sub(1)
    };
    let initialized = builder.ins().iconst(types::I64, initialized_mask as i64);
    builder.ins().store(
        MemFlagsData::new(),
        initialized,
        state_out,
        std::mem::offset_of!(crate::JitDeoptState, initialized_register_mask) as i32,
    );
    for (snapshot_slot, (_register, value)) in encodable_registers.into_iter().enumerate() {
        let value = if builder.func.dfg.value_type(value) == types::I64 {
            value
        } else {
            builder.ins().uextend(types::I64, value)
        };
        let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
            .saturating_add(snapshot_slot.saturating_mul(8));
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
    let opcode = builder.ins().iconst(types::I32, i64::from(opcode));
    let mut args = Vec::with_capacity(operands.len() + 2);
    args.push(opcode);
    args.extend_from_slice(operands);
    args.push(result_out);
    let call = call_native_helper(module, builder, helper, &args);
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), result_out, 0);
    require_native_value_operation_ok(
        builder,
        builder.inst_results(call)[0],
        helper.terminal_exit()?,
        value,
    )?;
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
    locals: &NativeLocalMap,
    native_version: u32,
) -> Result<ir::Value, CraneliftLoweringError> {
    let helper = helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
            format!("native value operation {opcode} has no declared helper"),
        )
    })?;
    let opcode = builder.ins().iconst(types::I32, i64::from(opcode));
    let mut args = Vec::with_capacity(operands.len() + 2);
    args.push(opcode);
    args.extend_from_slice(operands);
    args.push(result_out);
    let call = call_native_helper(module, builder, helper, &args);
    let status = builder.inst_results(call)[0];
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), result_out, 0);
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
    let Some(helper) = helper else {
        return Ok(());
    };
    let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
    let local = builder.ins().iconst(types::I64, i64::from(local.raw()));
    let _ = lower_native_value_operation(
        module,
        builder,
        Some(helper),
        4,
        &[value, function, local],
        result_out,
    )?;
    Ok(())
}

fn lower_guarded_reference_binding(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    value: ir::Value,
    result_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    let reference = builder.create_block();
    let generic = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let is_reference = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    builder
        .ins()
        .brif(is_reference, reference, &[], generic, &[]);

    builder.switch_to_block(reference);
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(generic);
    let zero = builder.ins().iconst(types::I64, 0);
    let value =
        lower_native_value_operation(module, builder, helper, 0, &[value, zero, zero], result_out)?;
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_direct_reference_argument(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    argument: &php_ir::instruction::IrCallArg,
    argument_index: usize,
    fallback_value: ir::Value,
    source_block: BlockId,
    instruction: &RegionInstruction,
    function: FunctionId,
    defer_until_signature_published: bool,
    publish_reference_locals: bool,
    result_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if let Some(local) = argument.by_ref_local {
        let value = use_local_variable(builder, locals, local)?;
        if defer_until_signature_published {
            let callsite =
                u64::from(function.raw()) | (u64::from(instruction.continuation_id) << 32);
            let callsite = builder.ins().iconst(types::I64, callsite as i64);
            let argument_index = builder.ins().iconst(
                types::I64,
                i64::try_from(argument_index).unwrap_or(i64::MAX),
            );
            let candidate = lower_native_value_operation(
                module,
                builder,
                helper,
                6,
                &[value, callsite, argument_index],
                result_out,
            )?;
            let bind_local = builder.create_block();
            let keep_local = builder.create_block();
            let merge = builder.create_block();
            builder.append_block_param(merge, types::I64);
            let is_reference =
                lower_value_has_tag(builder, candidate, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
            builder
                .ins()
                .brif(is_reference, bind_local, &[], keep_local, &[]);

            builder.switch_to_block(bind_local);
            define_local_variable(builder, locals, local, candidate)?;
            publish_native_reference_local(
                module,
                builder,
                publish_reference_locals.then_some(helper).flatten(),
                candidate,
                function,
                local,
                result_out,
            )?;
            builder.ins().jump(merge, &[candidate.into()]);

            builder.switch_to_block(keep_local);
            define_local_variable(builder, locals, local, value)?;
            builder.ins().jump(merge, &[candidate.into()]);

            builder.switch_to_block(merge);
            return Ok(builder.block_params(merge)[0]);
        }
        let reference =
            lower_guarded_reference_binding(module, builder, helper, value, result_out)?;
        define_local_variable(builder, locals, local, reference)?;
        publish_native_reference_local(
            module,
            builder,
            publish_reference_locals.then_some(helper).flatten(),
            reference,
            function,
            local,
            result_out,
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
            publish_reference_locals.then_some(helper).flatten(),
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
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    live_registers: &[RegId],
    native_version: u32,
) -> Result<ir::Value, CraneliftLoweringError> {
    let helper = helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
            format!("native binary operation {opcode} has no declared helper"),
        )
    })?;
    let opcode = builder.ins().iconst(types::I32, i64::from(opcode));
    let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
    let continuation = builder
        .ins()
        .iconst(types::I64, i64::from(instruction.continuation_id));
    let call = call_native_helper(
        module,
        builder,
        helper,
        &[opcode, lhs, rhs, function_value, continuation, result_out],
    );
    let status = builder.inst_results(call)[0];
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), result_out, 0);
    if native_version == 0 {
        require_native_value_operation_ok(builder, status, helper.terminal_exit()?, value)?;
        return Ok(value);
    }
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
    publish_native_register_state(builder, deopt_out, registers, live_registers);
    builder
        .ins()
        .store(MemFlagsData::new(), value, result_out, 0);
    builder.ins().return_(&[status]);
    builder.switch_to_block(ok);
    Ok(value)
}

fn lower_value_has_tag(builder: &mut FunctionBuilder<'_>, value: ir::Value, tag: u64) -> ir::Value {
    let encoded_tag = builder
        .ins()
        .band_imm(value, crate::JIT_VALUE_RUNTIME_KIND_MASK as i64);
    builder
        .ins()
        .icmp_imm(IntCC::Equal, encoded_tag, tag as i64)
}

fn lower_is_runtime_handle(builder: &mut FunctionBuilder<'_>, value: ir::Value) -> ir::Value {
    let tag = builder
        .ins()
        .band_imm(value, crate::JIT_VALUE_TAG_MASK as i64);
    builder
        .ins()
        .icmp_imm(IntCC::Equal, tag, crate::JIT_VALUE_RUNTIME_TAG as i64)
}

fn lower_value_has_namespace_tag(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    tag: u64,
) -> ir::Value {
    let encoded_tag = builder
        .ins()
        .band_imm(value, crate::JIT_VALUE_TAG_MASK as i64);
    builder
        .ins()
        .icmp_imm(IntCC::Equal, encoded_tag, tag as i64)
}

fn lower_not_bool(builder: &mut FunctionBuilder<'_>, value: ir::Value) -> ir::Value {
    builder.ins().icmp_imm(IntCC::Equal, value, 0)
}

fn lower_is_immediate_int(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    is_constant: ir::Value,
) -> ir::Value {
    let is_runtime = lower_is_runtime_handle(builder, value);
    let is_not_runtime = lower_not_bool(builder, is_runtime);
    let is_not_constant = lower_not_bool(builder, is_constant);
    builder.ins().band(is_not_runtime, is_not_constant)
}

fn lower_stable_builtin_type_predicate(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    op: u32,
    value: ir::Value,
    result_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if !helper.is_some_and(|helper| helper.inline_runtime_view) {
        return lower_native_value_operation(module, builder, helper, op, &[value], result_out);
    }
    let direct = builder.create_block();
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let is_reference = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    let is_constant = lower_value_has_namespace_tag(builder, value, crate::JIT_VALUE_CONSTANT_TAG);
    let is_null = builder
        .ins()
        .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX));
    let is_false = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    let is_true = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
    );
    let is_uninitialized = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
    );
    let is_reserved = builder.ins().bor(is_null, is_false);
    let is_reserved = builder.ins().bor(is_reserved, is_true);
    let is_reserved = builder.ins().bor(is_reserved, is_uninitialized);
    let is_not_reserved = lower_not_bool(builder, is_reserved);
    let is_non_reserved_constant = builder.ins().band(is_constant, is_not_reserved);
    let mut needs_slow = builder.ins().bor(is_reference, is_non_reserved_constant);
    if matches!(op, 2 | 8) {
        let is_boxed_scalar = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_TAG);
        needs_slow = builder.ins().bor(needs_slow, is_boxed_scalar);
    }
    builder.ins().brif(needs_slow, slow, &[], direct, &[]);

    builder.switch_to_block(direct);
    let is_bool = builder.ins().bor(is_false, is_true);
    let matched = match op {
        0 => is_null,
        1 => is_bool,
        2 => lower_is_immediate_int(builder, value, is_constant),
        3 => lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_FLOAT_TAG),
        4 => lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_STRING_TAG),
        5 => lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_ARRAY_TAG),
        6 => {
            let is_object =
                lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
            let is_callable =
                lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_CALLABLE_TAG);
            let is_generator =
                lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_GENERATOR_TAG);
            let is_fiber = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_FIBER_TAG);
            let is_object = builder.ins().bor(is_object, is_callable);
            let is_object = builder.ins().bor(is_object, is_generator);
            builder.ins().bor(is_object, is_fiber)
        }
        7 => lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_RESOURCE_TAG),
        8 => {
            let is_int = lower_is_immediate_int(builder, value, is_constant);
            let is_float = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_FLOAT_TAG);
            let is_string =
                lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_STRING_TAG);
            let is_scalar = builder.ins().bor(is_bool, is_int);
            let is_scalar = builder.ins().bor(is_scalar, is_float);
            builder.ins().bor(is_scalar, is_string)
        }
        _ => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_TYPE_PREDICATE",
                format!("unsupported stable type predicate opcode {op}"),
            ));
        }
    };
    let matched = encode_native_bool(builder, matched);
    builder.ins().jump(merge, &[matched.into()]);

    builder.switch_to_block(slow);
    let matched = lower_native_value_operation(module, builder, helper, op, &[value], result_out)?;
    builder.ins().jump(merge, &[matched.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_stable_builtin_length(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    op: u32,
    value: ir::Value,
    function: FunctionId,
    continuation_id: u32,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if !helper.is_some_and(|helper| helper.inline_runtime_view) {
        let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
        let continuation = builder.ins().iconst(types::I64, i64::from(continuation_id));
        return lower_native_value_operation(
            module,
            builder,
            helper,
            op,
            &[value, function, continuation],
            result_out,
        );
    }
    let inspect = builder.create_block();
    let direct = builder.create_block();
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let expected_tag = if op == 0 {
        crate::JIT_VALUE_RUNTIME_STRING_TAG
    } else {
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG
    };
    let expected_kind = if op == 0 {
        crate::JIT_NATIVE_VALUE_VIEW_STRING
    } else {
        crate::JIT_NATIVE_VALUE_VIEW_ARRAY
    };
    let tag_matches = lower_value_has_tag(builder, value, expected_tag);
    builder.ins().brif(tag_matches, inspect, &[], slow, &[]);

    builder.switch_to_block(inspect);
    let view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let version = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, abi_version) as i32,
    );
    let capacity = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, value_view_capacity) as i32,
    );
    let pointer_type = module.target_config().pointer_type();
    let views = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, value_views) as i32,
    );
    let index = builder.ins().ireduce(types::I32, value);
    let version_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        version,
        i64::from(crate::JIT_RUNTIME_ABI_VERSION),
    );
    let pointer_ok = builder.ins().icmp_imm(IntCC::NotEqual, views, 0);
    let index_ok = builder.ins().icmp(IntCC::UnsignedLessThan, index, capacity);
    let view_ok = builder.ins().band(version_ok, pointer_ok);
    let view_ok = builder.ins().band(view_ok, index_ok);
    builder.ins().brif(view_ok, direct, &[], slow, &[]);

    builder.switch_to_block(direct);
    let index = builder.ins().uextend(pointer_type, index);
    let byte_offset = builder.ins().ishl_imm(index, 4);
    let descriptor = builder.ins().iadd(views, byte_offset);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueView, kind) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueView, length) as i32,
    );
    let kind_ok = builder
        .ins()
        .icmp_imm(IntCC::Equal, kind, i64::from(expected_kind));
    let length_ok = builder.ins().icmp_imm(
        IntCC::UnsignedLessThan,
        length,
        crate::JIT_VALUE_CONSTANT_TAG as i64,
    );
    let descriptor_ok = builder.ins().band(kind_ok, length_ok);
    let publish = builder.create_block();
    builder.ins().brif(descriptor_ok, publish, &[], slow, &[]);
    builder.switch_to_block(publish);
    builder.ins().jump(merge, &[length.into()]);

    builder.switch_to_block(slow);
    let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
    let continuation = builder.ins().iconst(types::I64, i64::from(continuation_id));
    let length = lower_native_value_operation(
        module,
        builder,
        helper,
        op,
        &[value, function, continuation],
        result_out,
    )?;
    builder.ins().jump(merge, &[length.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_guarded_empty_condition(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    truthy: Option<NativeHelper>,
    stable_length: Option<NativeHelper>,
    value: ir::Value,
    function: FunctionId,
    continuation_id: u32,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    let array = builder.create_block();
    let generic = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I8);

    let is_array = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    builder.ins().brif(is_array, array, &[], generic, &[]);

    builder.switch_to_block(array);
    let length = lower_stable_builtin_length(
        module,
        builder,
        stable_length,
        1,
        value,
        function,
        continuation_id,
        result_out,
        deopt_out,
    )?;
    let array_truthy = builder.ins().icmp_imm(IntCC::NotEqual, length, 0);
    builder.ins().jump(merge, &[array_truthy.into()]);

    builder.switch_to_block(generic);
    let generic_truthy = terminators::lower_guarded_unknown_condition(
        module,
        builder,
        truthy.ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                "native empty truthiness helper was not declared",
            )
        })?,
        value,
    )?;
    builder.ins().jump(merge, &[generic_truthy.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_guarded_strict_identity(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    op: RegionCompareOpCode,
    lhs: ir::Value,
    rhs: ir::Value,
    result_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if !helper.is_some_and(|helper| helper.inline_runtime_view) {
        return lower_native_value_operation(
            module,
            builder,
            helper,
            native_compare_opcode(op),
            &[lhs, rhs],
            result_out,
        );
    }
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let values_equal = builder.ins().icmp(IntCC::Equal, lhs, rhs);
    let lhs_runtime = lower_is_runtime_handle(builder, lhs);
    let rhs_runtime = lower_is_runtime_handle(builder, rhs);
    let lhs_constant = lower_value_has_namespace_tag(builder, lhs, crate::JIT_VALUE_CONSTANT_TAG);
    let rhs_constant = lower_value_has_namespace_tag(builder, rhs, crate::JIT_VALUE_CONSTANT_TAG);
    let lhs_constant_index = builder.ins().ireduce(types::I32, lhs);
    let rhs_constant_index = builder.ins().ireduce(types::I32, rhs);
    let lhs_reserved = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        lhs_constant_index,
        i64::from(crate::JIT_VALUE_TRUE),
    );
    let rhs_reserved = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        rhs_constant_index,
        i64::from(crate::JIT_VALUE_TRUE),
    );
    let lhs_not_reserved = lower_not_bool(builder, lhs_reserved);
    let rhs_not_reserved = lower_not_bool(builder, rhs_reserved);
    let lhs_opaque_constant = builder.ins().band(lhs_constant, lhs_not_reserved);
    let rhs_opaque_constant = builder.ins().band(rhs_constant, rhs_not_reserved);
    let needs_slow = builder.ins().bor(lhs_runtime, rhs_runtime);
    let needs_slow = builder.ins().bor(needs_slow, lhs_opaque_constant);
    let needs_slow = builder.ins().bor(needs_slow, rhs_opaque_constant);
    let values_different = builder.ins().icmp_imm(IntCC::Equal, values_equal, 0);
    let needs_slow = builder.ins().band(values_different, needs_slow);
    let direct_result = if op == RegionCompareOpCode::NotIdentical {
        values_different
    } else {
        values_equal
    };
    let direct_result = encode_native_bool(builder, direct_result);
    builder
        .ins()
        .brif(needs_slow, slow, &[], merge, &[direct_result.into()]);

    builder.switch_to_block(slow);
    let value = lower_native_value_operation(
        module,
        builder,
        helper,
        native_compare_opcode(op),
        &[lhs, rhs],
        result_out,
    )?;
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_guarded_integer_compare(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    op: RegionCompareOpCode,
    lhs: ir::Value,
    rhs: ir::Value,
    result_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    let direct = builder.create_block();
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let lhs_constant = lower_value_has_namespace_tag(builder, lhs, crate::JIT_VALUE_CONSTANT_TAG);
    let rhs_constant = lower_value_has_namespace_tag(builder, rhs, crate::JIT_VALUE_CONSTANT_TAG);
    let lhs_int = lower_is_immediate_int(builder, lhs, lhs_constant);
    let rhs_int = lower_is_immediate_int(builder, rhs, rhs_constant);
    let both_int = builder.ins().band(lhs_int, rhs_int);
    builder.ins().brif(both_int, direct, &[], slow, &[]);

    builder.switch_to_block(direct);
    let value = lower_direct_compare(
        builder,
        op,
        lhs,
        rhs,
        SsaValueClass::Int,
        SsaValueClass::Int,
    )
    .expect("integer comparisons have direct CLIF lowering");
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(slow);
    let value = lower_native_value_operation(
        module,
        builder,
        helper,
        native_compare_opcode(op),
        &[lhs, rhs],
        result_out,
    )?;
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_guarded_value_lifecycle(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    operation: u32,
    value: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if !helper.is_some_and(|helper| helper.inline_runtime_view) {
        return lower_native_value_operation(
            module,
            builder,
            helper,
            operation,
            &[value],
            result_out,
        );
    }
    let slow = builder.create_block();
    let done = builder.create_block();
    let is_runtime = lower_is_runtime_handle(builder, value);
    let view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let version = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, abi_version) as i32,
    );
    let capacity = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, refcount_capacity) as i32,
    );
    let pointer_type = module.target_config().pointer_type();
    let refcounts = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, refcounts) as i32,
    );
    let index = builder.ins().ireduce(types::I32, value);
    let version_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        version,
        i64::from(crate::JIT_RUNTIME_ABI_VERSION),
    );
    let pointer_ok = builder.ins().icmp_imm(IntCC::NotEqual, refcounts, 0);
    let index_ok = builder.ins().icmp(IntCC::UnsignedLessThan, index, capacity);
    let view_ok = builder.ins().band(version_ok, pointer_ok);
    let view_ok = builder.ins().band(view_ok, index_ok);
    let direct_ok = builder.ins().band(is_runtime, view_ok);

    // Point invalid and non-runtime lanes at the always-valid result scratch
    // before loading. This keeps the common refcount update branchless without
    // speculating through a null or out-of-range runtime-view pointer.
    let safe_base = builder.ins().select(direct_ok, refcounts, result_out);
    let zero_index = builder.ins().iconst(types::I32, 0);
    let safe_index = builder.ins().select(direct_ok, index, zero_index);
    let safe_index = builder.ins().uextend(pointer_type, safe_index);
    let byte_offset = builder.ins().ishl_imm(safe_index, 2);
    let cell = builder.ins().iadd(safe_base, byte_offset);
    let count = builder.ins().load(types::I32, MemFlagsData::new(), cell, 0);
    let count_ok = if operation & 1 == 0 {
        let live = builder.ins().icmp_imm(IntCC::NotEqual, count, 0);
        let not_max = builder
            .ins()
            .icmp_imm(IntCC::NotEqual, count, u32::MAX as i64);
        builder.ins().band(live, not_max)
    } else {
        builder.ins().icmp_imm(IntCC::UnsignedGreaterThan, count, 1)
    };
    let fast = builder.ins().band(direct_ok, count_ok);
    let updated = if operation & 1 == 0 {
        builder.ins().iadd_imm(count, 1)
    } else {
        builder.ins().iadd_imm(count, -1)
    };
    let stored = builder.ins().select(fast, updated, count);
    builder.ins().store(MemFlagsData::new(), stored, cell, 0);
    let not_fast = builder.ins().icmp_imm(IntCC::Equal, fast, 0);
    let needs_slow = builder.ins().band(is_runtime, not_fast);
    builder.ins().brif(needs_slow, slow, &[], done, &[]);

    builder.switch_to_block(slow);
    let _ = lower_native_value_operation(module, builder, helper, operation, &[value], result_out)?;
    builder.ins().jump(done, &[]);

    builder.switch_to_block(done);
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
fn lower_guarded_native_local_store(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    local_store: Option<NativeHelper>,
    lifecycle: Option<NativeHelper>,
    current: ir::Value,
    value: ir::Value,
    operation: u32,
    move_input: bool,
    release_current: bool,
    function: FunctionId,
    local: LocalId,
    continuation_id: u32,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if local_store.is_some_and(|helper| !helper.inline_runtime_view)
        || operation & crate::JIT_LOCAL_STORE_PLAIN_LOCAL == 0
    {
        let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
        let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
        let operation = if move_input {
            operation | crate::JIT_LOCAL_STORE_MOVE_INPUT
        } else {
            operation
        };
        return lower_native_value_operation(
            module,
            builder,
            local_store,
            operation,
            &[current, value, function_value, local_value],
            result_out,
        );
    }
    let direct = builder.create_block();
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let current_is_reference =
        lower_value_has_tag(builder, current, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    let value_is_reference =
        lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    let needs_slow = builder.ins().bor(current_is_reference, value_is_reference);
    builder.ins().brif(needs_slow, slow, &[], direct, &[]);

    builder.switch_to_block(direct);
    let stored = if move_input {
        value
    } else {
        lower_guarded_value_lifecycle(
            module,
            builder,
            lifecycle,
            native_dim_operation(0, function, continuation_id),
            value,
            result_out,
            deopt_out,
        )?
    };
    if release_current {
        let _ = lower_guarded_value_lifecycle(
            module,
            builder,
            lifecycle,
            native_dim_operation(1, function, continuation_id),
            current,
            result_out,
            deopt_out,
        )?;
    }
    builder.ins().jump(merge, &[stored.into()]);

    builder.switch_to_block(slow);
    let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
    let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
    let operation = if move_input {
        operation | crate::JIT_LOCAL_STORE_MOVE_INPUT
    } else {
        operation
    };
    let stored = lower_native_value_operation(
        module,
        builder,
        local_store,
        operation,
        &[current, value, function_value, local_value],
        result_out,
    )?;
    builder.ins().jump(merge, &[stored.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_guarded_native_local_fetch(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    lifecycle: Option<NativeHelper>,
    value: ir::Value,
    borrowed: bool,
    quiet: bool,
    method_local_fast_path: bool,
    function: FunctionId,
    local: LocalId,
    span: IrSpan,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if helper.is_some_and(|helper| !helper.inline_runtime_view) || !method_local_fast_path {
        return lower_native_local_fetch(
            module,
            builder,
            helper,
            value,
            quiet,
            method_local_fast_path,
            function,
            local,
            span,
            result_out,
        );
    }
    let non_reference = builder.create_block();
    let inspect_reference = builder.create_block();
    let inspect_descriptor = builder.create_block();
    let load_reference = builder.create_block();
    let cached_reference = builder.create_block();
    let direct = builder.create_block();
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let is_reference = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    builder
        .ins()
        .brif(is_reference, inspect_reference, &[], non_reference, &[]);

    builder.switch_to_block(non_reference);
    let is_uninitialized = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
    );
    let needs_slow = if quiet {
        builder.ins().iconst(types::I8, 0)
    } else {
        is_uninitialized
    };
    builder.ins().brif(needs_slow, slow, &[], direct, &[]);

    builder.switch_to_block(inspect_reference);
    let view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let version = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, abi_version) as i32,
    );
    let capacity = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, value_view_capacity) as i32,
    );
    let pointer_type = module.target_config().pointer_type();
    let views = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, value_views) as i32,
    );
    let index = builder.ins().ireduce(types::I32, value);
    let version_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        version,
        i64::from(crate::JIT_RUNTIME_ABI_VERSION),
    );
    let pointer_ok = builder.ins().icmp_imm(IntCC::NotEqual, views, 0);
    let index_ok = builder.ins().icmp(IntCC::UnsignedLessThan, index, capacity);
    let descriptor_ok = builder.ins().band(version_ok, pointer_ok);
    let descriptor_ok = builder.ins().band(descriptor_ok, index_ok);
    builder
        .ins()
        .brif(descriptor_ok, inspect_descriptor, &[], slow, &[]);

    builder.switch_to_block(inspect_descriptor);
    let index = builder.ins().uextend(pointer_type, index);
    let byte_offset = builder.ins().ishl_imm(index, 4);
    let descriptor = builder.ins().iadd(views, byte_offset);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueView, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueView, flags) as i32,
    );
    let address = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueView, length) as i32,
    );
    let kind_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR),
    );
    let flags_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        flags,
        i64::from(crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION),
    );
    let address_ok = builder.ins().icmp_imm(IntCC::NotEqual, address, 0);
    let reference_ok = builder.ins().band(kind_ok, flags_ok);
    let reference_ok = builder.ins().band(reference_ok, address_ok);
    builder
        .ins()
        .brif(reference_ok, load_reference, &[], slow, &[]);

    builder.switch_to_block(load_reference);
    let reference_view = if pointer_type == types::I64 {
        address
    } else {
        builder.ins().ireduce(pointer_type, address)
    };
    let reference_version = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        reference_view,
        std::mem::offset_of!(crate::JitNativeReferenceScalarView, abi_version) as i32,
    );
    let reference_state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        reference_view,
        std::mem::offset_of!(crate::JitNativeReferenceScalarView, state) as i32,
    );
    let encoded = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        reference_view,
        std::mem::offset_of!(crate::JitNativeReferenceScalarView, encoded) as i32,
    );
    let reference_version_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        reference_version,
        i64::from(crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION),
    );
    let published = builder.ins().icmp_imm(
        IntCC::Equal,
        reference_state,
        i64::from(crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED),
    );
    let mut cached_ok = builder.ins().band(reference_version_ok, published);
    let runtime_handle = lower_is_runtime_handle(builder, encoded);
    let immediate = lower_not_bool(builder, runtime_handle);
    cached_ok = builder.ins().band(cached_ok, immediate);
    if !quiet {
        let initialized = builder.ins().icmp_imm(
            IntCC::NotEqual,
            encoded,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        cached_ok = builder.ins().band(cached_ok, initialized);
    }
    builder
        .ins()
        .brif(cached_ok, cached_reference, &[], slow, &[]);

    builder.switch_to_block(cached_reference);
    let cached = if quiet {
        let cached_is_uninitialized = builder.ins().icmp_imm(
            IntCC::Equal,
            encoded,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        let null = builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
        builder.ins().select(cached_is_uninitialized, null, encoded)
    } else {
        encoded
    };
    builder.ins().jump(merge, &[cached.into()]);

    builder.switch_to_block(direct);
    let direct_value = if quiet {
        let null = builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
        builder.ins().select(is_uninitialized, null, value)
    } else {
        value
    };
    let direct_value = if borrowed {
        direct_value
    } else {
        lower_guarded_value_lifecycle(
            module,
            builder,
            lifecycle,
            0,
            direct_value,
            result_out,
            deopt_out,
        )?
    };
    builder.ins().jump(merge, &[direct_value.into()]);

    builder.switch_to_block(slow);
    let slow_value = lower_native_local_fetch(
        module,
        builder,
        helper,
        value,
        quiet,
        method_local_fast_path,
        function,
        local,
        span,
        result_out,
    )?;
    builder.ins().jump(merge, &[slow_value.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_guarded_reference_dimension_load(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    lifecycle: Option<NativeHelper>,
    value: ir::Value,
    quiet: bool,
    method_local_fast_path: bool,
    function: FunctionId,
    local: LocalId,
    span: IrSpan,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    let reference = builder.create_block();
    let generic = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let is_reference = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    builder
        .ins()
        .brif(is_reference, reference, &[], generic, &[]);

    builder.switch_to_block(reference);
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(generic);
    let value = lower_guarded_native_local_fetch(
        module,
        builder,
        helper,
        lifecycle,
        value,
        false,
        quiet,
        method_local_fast_path,
        function,
        local,
        span,
        result_out,
        deopt_out,
    )?;
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_native_local_fetch(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    value: ir::Value,
    quiet: bool,
    method_local_fast_path: bool,
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
        u32::from(quiet)
            | if method_local_fast_path {
                crate::JIT_LOCAL_FETCH_PLAIN_LOCAL
            } else {
                0
            },
        &[value, function, local, file, start],
        result_out,
    )
}

fn ordinary_local_fast_path(
    function_is_top_level: bool,
    local_names: &[String],
    local: LocalId,
) -> bool {
    !function_is_top_level
        && local_names.get(local.index()).is_none_or(|name| {
            !matches!(
                name.as_str(),
                "GLOBALS"
                    | "_SERVER"
                    | "_GET"
                    | "_POST"
                    | "_FILES"
                    | "_COOKIE"
                    | "_SESSION"
                    | "_REQUEST"
                    | "_ENV"
            )
        })
}

fn native_local_store_operation(
    function_is_top_level: bool,
    local_names: &[String],
    local: LocalId,
) -> u32 {
    if ordinary_local_fast_path(function_is_top_level, local_names, local) {
        crate::JIT_LOCAL_STORE_PLAIN_LOCAL
    } else {
        0
    }
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

fn lowering_operand_fact(
    value_flow: &ExecutableValueFlow,
    constants: &[IrConstant],
    operand: RegionOperand,
) -> SsaValueFact {
    value_flow.operand_fact(constants, operand)
}

#[allow(clippy::too_many_arguments)]
fn lower_region_instruction(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    functions: &BTreeMap<FunctionId, FuncId>,
    inline_constants: &BTreeMap<FunctionId, BoundedInlineValue>,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    native_call_helper: Option<NativeHelper>,
    native_dynamic_code_helper: Option<NativeHelper>,
    native_operations: NativeOperationFunctions,
    register_variables: &NativeRegisterMap,
    blocks: &BTreeMap<BlockId, ir::Block>,
    suspension_blocks: &BTreeMap<u32, ir::Block>,
    locals: &NativeLocalMap,
    registers: &mut NativeRegisterMap,
    source_block: BlockId,
    instruction: &RegionInstruction,
    transition_live_registers: &[RegId],
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
    streaming_call_exit: Option<NativeStreamingCallExit>,
    result_out: ir::Value,
    deopt_out: ir::Value,
    resume_state: ir::Value,
    pending_status: Variable,
    pending_value: Variable,
    function: FunctionId,
    local_count: u32,
    native_version: u32,
    function_is_top_level: bool,
    function_local_names: &[String],
    pointer_type: ir::Type,
) -> Result<(), CraneliftLoweringError> {
    let native_reference_publish = function_is_top_level
        .then_some(native_operations.reference_bind)
        .flatten();
    match &instruction.kind {
        RegionInstructionKind::Nop => {}
        RegionInstructionKind::Move { dst, src } => {
            let cl_value = lower_region_operand(builder, locals, registers, *src)?;
            let fact = lowering_operand_fact(value_flow, constants, *src);
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
                && !*quiet
                && (!value_copy_requires_retain(fact)
                    || value_flow.can_borrow_local_load(instruction.continuation_id));
            let value = if value_flow
                .passes_reference_to_typed_consumer(instruction.continuation_id)
            {
                lower_guarded_reference_dimension_load(
                    module,
                    builder,
                    native_operations.local_fetch,
                    native_operations.value_lifecycle,
                    value,
                    *quiet,
                    ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
                    function,
                    *local,
                    instruction.span,
                    result_out,
                    deopt_out,
                )?
            } else if direct {
                value
            } else if value_flow.local_storage(*local).is_native_frame_local() {
                lower_guarded_native_local_fetch(
                    module,
                    builder,
                    native_operations.local_fetch,
                    native_operations.value_lifecycle,
                    value,
                    value_flow.can_borrow_local_load(instruction.continuation_id),
                    *quiet,
                    ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
                    function,
                    *local,
                    instruction.span,
                    result_out,
                    deopt_out,
                )?
            } else {
                lower_native_local_fetch(
                    module,
                    builder,
                    native_operations.local_fetch,
                    value,
                    *quiet,
                    ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
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
            let fact = lowering_operand_fact(value_flow, constants, src_operand);
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
                    lower_guarded_value_lifecycle(
                        module,
                        builder,
                        native_operations.value_lifecycle,
                        native_dim_operation(0, function, instruction.continuation_id),
                        src,
                        result_out,
                        deopt_out,
                    )?
                } else {
                    src
                };
                let current_fact = value_flow.local_fact(*local);
                if instruction.live_locals.contains(local)
                    && (current_fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || value_release_required(current_fact))
                {
                    let _ = lower_guarded_value_lifecycle(
                        module,
                        builder,
                        native_operations.value_lifecycle,
                        native_dim_operation(1, function, instruction.continuation_id),
                        current,
                        result_out,
                        deopt_out,
                    )?;
                }
                stored
            } else if value_flow.local_storage(*local).is_native_frame_local() {
                lower_guarded_native_local_store(
                    module,
                    builder,
                    native_operations.local_store,
                    native_operations.value_lifecycle,
                    current,
                    src,
                    native_local_store_operation(
                        function_is_top_level,
                        function_local_names,
                        *local,
                    ),
                    value_flow.moves_value_into_local(instruction.continuation_id),
                    instruction.live_locals.contains(local),
                    function,
                    *local,
                    instruction.continuation_id,
                    result_out,
                    deopt_out,
                )?
            } else {
                let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
                let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.local_store,
                    native_local_store_operation(
                        function_is_top_level,
                        function_local_names,
                        *local,
                    ),
                    &[current, src, function_value, local_value],
                    result_out,
                )?
            };
            define_local_variable(builder, locals, *local, cl_value)?;
        }
        RegionInstructionKind::AssignLocalResult { dst, local, value } => {
            let current = use_local_variable(builder, locals, *local)?;
            let value_operand = *value;
            let value = lower_region_operand(builder, locals, registers, value_operand)?;
            let fact = lowering_operand_fact(value_flow, constants, value_operand);
            let direct =
                value_flow.local_storage(*local).is_promoted() && !value_copy_requires_retain(fact);
            let stored = if direct {
                value
            } else if value_flow.local_storage(*local).is_native_frame_local() {
                lower_guarded_native_local_store(
                    module,
                    builder,
                    native_operations.local_store,
                    native_operations.value_lifecycle,
                    current,
                    value,
                    native_local_store_operation(
                        function_is_top_level,
                        function_local_names,
                        *local,
                    ),
                    false,
                    instruction.live_locals.contains(local),
                    function,
                    *local,
                    instruction.continuation_id,
                    result_out,
                    deopt_out,
                )?
            } else {
                let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
                let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.local_store,
                    native_local_store_operation(
                        function_is_top_level,
                        function_local_names,
                        *local,
                    ),
                    &[current, value, function_value, local_value],
                    result_out,
                )?
            };
            define_local_variable(builder, locals, *local, stored)?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::BindReference { target, source } => {
            let source_value = use_local_variable(builder, locals, *source)?;
            let reference = lower_guarded_reference_binding(
                module,
                builder,
                native_operations.reference_bind,
                source_value,
                result_out,
            )?;
            define_local_variable(builder, locals, *source, reference)?;
            define_local_variable(builder, locals, *target, reference)?;
            publish_native_reference_local(
                module,
                builder,
                native_reference_publish,
                reference,
                function,
                *source,
                result_out,
            )?;
            publish_native_reference_local(
                module,
                builder,
                native_reference_publish,
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
                ordinary_local_fast_path(function_is_top_level, function_local_names, *array),
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
                native_local_store_operation(function_is_top_level, function_local_names, *array)
                    | crate::JIT_LOCAL_STORE_MOVE_INPUT,
                &[current, updated, function_value, local_value],
                result_out,
            )?;
            define_local_variable(builder, locals, *array, stored)?;
            define_local_variable(builder, locals, *target, reference)?;
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
            define_local_variable(builder, locals, *source, reference)?;
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
            define_local_variable(builder, locals, *array, updated)?;
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
            define_local_variable(builder, locals, *source, reference)?;
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
            define_local_variable(builder, locals, *target, reference)?;
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
            define_local_variable(builder, locals, *target, reference)?;
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
            define_local_variable(builder, locals, *source, reference)?;
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
                define_local_variable(builder, locals, *array, reference)?;
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
                define_local_variable(builder, locals, *array, updated)?;
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
            publish_native_register_state(builder, deopt_out, registers, transition_live_registers);
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
            define_local_variable(builder, locals, *source, reference)?;
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
            define_local_variable(builder, locals, *local, reference)?;
        }
        RegionInstructionKind::Discard { src } => {
            if value_flow.elides_discard(instruction.continuation_id) {
                return Ok(());
            }
            let value = lower_region_operand(builder, locals, registers, *src)?;
            let fact = lowering_operand_fact(value_flow, constants, *src);
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
            let lhs_fact = lowering_operand_fact(value_flow, constants, lhs_operand);
            let rhs_fact = lowering_operand_fact(value_flow, constants, rhs_operand);
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
                    transition_live_registers,
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
                    transition_live_registers,
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
            let fact = lowering_operand_fact(value_flow, constants, src_operand);
            let unary_operation =
                if function.raw() <= 0x03ff && instruction.continuation_id <= 0x07_ffff {
                    0x8000_0000
                        | native_unary_opcode(*op)
                        | (function.raw() << 2)
                        | (instruction.continuation_id << 12)
                } else {
                    native_unary_opcode(*op)
                };
            let direct = if fact.certainty != crate::region_ir::SsaCertainty::Unknown {
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
            };
            let value = if let Some(value) = direct {
                value
            } else if *op == RegionUnaryOp::Not {
                let truthy = terminators::lower_guarded_unknown_condition(
                    module,
                    builder,
                    native_operations.truthy.ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                            "native unary truthiness helper was not declared",
                        )
                    })?,
                    src,
                )?;
                let inverted = builder.ins().bxor_imm(truthy, 1);
                encode_native_bool(builder, inverted)
            } else {
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.unary,
                    unary_operation,
                    &[src],
                    result_out,
                )?
            };
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::NativeCall(call) => {
            if let Some(predicate) = stable_builtin_type_predicate(&call.target)
                && call.argument_operand_offset == 0
                && call.args.len() == 1
                && call.args[0].name.is_none()
                && !call.args[0].unpack
                && call.operands.len() == 1
                && !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                && let Some(operand) = call.operands[0]
            {
                let source = lower_region_operand(builder, locals, registers, operand)?;
                let value = lower_stable_builtin_type_predicate(
                    module,
                    builder,
                    native_operations.type_predicate,
                    predicate,
                    source,
                    result_out,
                )?;
                if matches!(
                    operand,
                    RegionOperand::Register(register)
                        if value_flow.register_fact(register).ownership != SsaOwnership::Borrowed
                ) && native_argument_has_location(&call.args[0])
                {
                    let _ = lower_native_value_operation(
                        module,
                        builder,
                        native_operations.value_lifecycle,
                        native_dim_operation(1, function, instruction.continuation_id),
                        &[source],
                        result_out,
                    )?;
                }
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
            if let Some(length_op) = stable_builtin_length(&call.target)
                && call.argument_operand_offset == 0
                && call.args.len() == 1
                && call.args[0].name.is_none()
                && !call.args[0].unpack
                && call.operands.len() == 1
                && !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                && let Some(operand) = call.operands[0]
            {
                let source = lower_region_operand(builder, locals, registers, operand)?;
                let value = lower_stable_builtin_length(
                    module,
                    builder,
                    native_operations.stable_length,
                    length_op,
                    source,
                    function,
                    instruction.continuation_id,
                    result_out,
                    deopt_out,
                )?;
                if matches!(
                    operand,
                    RegionOperand::Register(register)
                        if value_flow.register_fact(register).ownership != SsaOwnership::Borrowed
                ) && native_argument_has_location(&call.args[0])
                {
                    let _ = lower_native_value_operation(
                        module,
                        builder,
                        native_operations.value_lifecycle,
                        native_dim_operation(1, function, instruction.continuation_id),
                        &[source],
                        result_out,
                    )?;
                }
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
            if !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                && let Some(target) = direct_target
                && let Some((inline, value)) =
                    inline_constants.get(&target).copied().and_then(|inline| {
                        bounded_inline_call_operand(call, inline).map(|value| (inline, value))
                    })
            {
                let mut value = lower_region_operand(builder, locals, registers, value)?;
                if matches!(inline, BoundedInlineValue::Argument { .. })
                    && matches!(call.result, RegionCallResult::Register(_))
                {
                    value = lower_guarded_value_lifecycle(
                        module,
                        builder,
                        native_operations.value_lifecycle,
                        native_dim_operation(0, function, instruction.continuation_id),
                        value,
                        result_out,
                        deopt_out,
                    )?;
                }
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
                    transition_live_registers,
                    streaming_call_exit,
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
                        false,
                        function_is_top_level,
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
                        pointer_type.bytes(),
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
                        helper.terminal_exit()?,
                    )?;
                    let address = builder.ins().stack_load(pointer_type, address_slot, 0);
                    let signature = native_php_entry_signature(module);
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
                    define_local_variable(builder, locals, local, value)?;
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
                        define_local_variable(builder, locals, *local, value)?;
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
                transition_live_registers,
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
            let lhs_fact = lowering_operand_fact(value_flow, constants, lhs_operand);
            let rhs_fact = lowering_operand_fact(value_flow, constants, rhs_operand);
            let direct = (lhs_fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && rhs_fact.certainty != crate::region_ir::SsaCertainty::Unknown)
                .then(|| {
                    lower_direct_compare(builder, *op, lhs, rhs, lhs_fact.class, rhs_fact.class)
                })
                .flatten();
            let cl_value = if let Some(value) = direct {
                value
            } else if matches!(
                op,
                RegionCompareOpCode::Identical | RegionCompareOpCode::NotIdentical
            ) {
                lower_guarded_strict_identity(
                    module,
                    builder,
                    native_operations.compare,
                    *op,
                    lhs,
                    rhs,
                    result_out,
                )?
            } else if native_operations.compare.is_some() {
                lower_guarded_integer_compare(
                    module,
                    builder,
                    native_operations.compare,
                    *op,
                    lhs,
                    rhs,
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
            let fact = lowering_operand_fact(value_flow, constants, src_operand);
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
            let call = call_native_helper(module, builder, helper, &[src]);
            require_native_operation_ok(
                builder,
                builder.inst_results(call)[0],
                helper.terminal_exit()?,
            )?;
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
            publish_native_register_state(builder, deopt_out, registers, transition_live_registers);
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
            publish_native_register_state(builder, deopt_out, registers, transition_live_registers);
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
            publish_native_register_state(builder, deopt_out, registers, transition_live_registers);
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
            publish_native_register_state(builder, deopt_out, registers, transition_live_registers);
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let value_operand = *value;
            let value = lower_region_operand(builder, locals, registers, value_operand)?;
            let move_value = matches!(
                lowering_operand_fact(value_flow, constants, value_operand).ownership,
                SsaOwnership::Owned | SsaOwnership::Moved
            );
            let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder.ins().iconst(
                types::I64,
                native_instruction_locator(source_block, instruction.id),
            );
            let value = lower_native_value_operation(
                module,
                builder,
                native_operations.property_assign,
                u32::from(move_value) << 1,
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
                define_local_variable(builder, locals, *local, value)?;
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
                    ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
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
            publish_native_register_state(builder, deopt_out, registers, transition_live_registers);
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
                    ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
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
                    native_local_store_operation(
                        function_is_top_level,
                        function_local_names,
                        *local,
                    ) | crate::JIT_LOCAL_STORE_MOVE_INPUT,
                    &[current, updated, function_value, local_value],
                    result_out,
                )?
            };
            define_local_variable(builder, locals, *local, stored)?;
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
                    ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
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
                    native_local_store_operation(
                        function_is_top_level,
                        function_local_names,
                        *local,
                    ) | crate::JIT_LOCAL_STORE_MOVE_INPUT,
                    &[current, updated, function_value, local_value],
                    result_out,
                )?
            };
            define_local_variable(builder, locals, *local, stored)?;
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
                ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
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
                ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
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
                ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
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
                native_local_store_operation(function_is_top_level, function_local_names, *local)
                    | crate::JIT_LOCAL_STORE_MOVE_INPUT,
                &[current, updated, function_value, local_value],
                result_out,
            )?;
            define_local_variable(builder, locals, *local, stored)?;
        }
        RegionInstructionKind::IssetLocal { dst, local } => {
            let value = use_local_variable(builder, locals, *local)?;
            let value = lower_guarded_native_local_fetch(
                module,
                builder,
                native_operations.local_fetch,
                native_operations.value_lifecycle,
                value,
                true,
                true,
                ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
                function,
                *local,
                instruction.span,
                result_out,
                deopt_out,
            )?;
            let is_null =
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX));
            let is_uninitialized = builder.ins().icmp_imm(
                IntCC::Equal,
                value,
                crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
            );
            let absent = builder.ins().bor(is_null, is_uninitialized);
            let present = builder.ins().icmp_imm(IntCC::Equal, absent, 0);
            let result = encode_native_bool(builder, present);
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::EmptyLocal { dst, local } => {
            let value = use_local_variable(builder, locals, *local)?;
            let value = lower_guarded_native_local_fetch(
                module,
                builder,
                native_operations.local_fetch,
                native_operations.value_lifecycle,
                value,
                true,
                true,
                ordinary_local_fast_path(function_is_top_level, function_local_names, *local),
                function,
                *local,
                instruction.span,
                result_out,
                deopt_out,
            )?;
            let truthy = lower_guarded_empty_condition(
                module,
                builder,
                native_operations.truthy,
                native_operations.stable_length,
                value,
                function,
                instruction.continuation_id,
                result_out,
                deopt_out,
            )?;
            let empty = builder.ins().bxor_imm(truthy, 1);
            let result = encode_native_bool(builder, empty);
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::UnsetLocal { local } => {
            let current = use_local_variable(builder, locals, *local)?;
            let uninitialized = builder.ins().iconst(
                types::I64,
                crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
            );
            define_local_variable(builder, locals, *local, uninitialized)?;
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
            let key_out = builder.ins().stack_addr(pointer_type, key_slot, 0);
            let value_out = builder.ins().stack_addr(pointer_type, value_slot, 0);
            let has_out = builder.ins().stack_addr(pointer_type, has_slot, 0);
            let call = call_native_helper(
                module,
                builder,
                helper,
                &[iterator_value, key_out, value_out, has_out],
            );
            require_native_operation_ok(
                builder,
                builder.inst_results(call)[0],
                helper.terminal_exit()?,
            )?;
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
            let call = call_native_helper(module, builder, helper, &[iterator]);
            require_native_operation_ok(
                builder,
                builder.inst_results(call)[0],
                helper.terminal_exit()?,
            )?;
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
            let key_out = builder.ins().stack_addr(pointer_type, key_slot, 0);
            let value_out = builder.ins().stack_addr(pointer_type, value_slot, 0);
            let has_out = builder.ins().stack_addr(pointer_type, has_slot, 0);
            let call = call_native_helper(
                module,
                builder,
                helper,
                &[iterator_value, key_out, value_out, has_out],
            );
            require_native_operation_ok(
                builder,
                builder.inst_results(call)[0],
                helper.terminal_exit()?,
            )?;
            let has = builder.ins().stack_load(types::I64, has_slot, 0);
            let next_value = builder.ins().stack_load(types::I64, value_slot, 0);
            define_region_register(builder, register_variables, registers, *has_value, has)?;
            define_local_variable(builder, locals, *value_local, next_value)?;
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
            let function_id = builder.ins().iconst(types::I32, i64::from(function.raw()));
            let continuation_id = builder
                .ins()
                .iconst(types::I32, i64::from(instruction.continuation_id));
            let call = call_native_helper(module, builder, helper, &[function_id, continuation_id]);
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
    locals: &NativeLocalMap,
    register_variables: &NativeRegisterMap,
    registers: &mut NativeRegisterMap,
    live_registers: &[RegId],
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
    if !copy_native_local_state_values(
        builder,
        state_out,
        locals,
        &instruction.live_locals,
        NativeLocalCopyDirection::FrameToState,
    )? {
        for local in &instruction.live_locals {
            let value = use_local_variable(builder, locals, *local)?;
            let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                .saturating_add(local.index().saturating_mul(8));
            builder
                .ins()
                .store(MemFlagsData::new(), value, state_out, offset as i32);
        }
    }
    let register_ids = live_registers
        .iter()
        .filter(|register| registers.contains_key(register))
        .take(crate::JIT_DEOPT_MAX_REGISTERS)
        .copied()
        .collect::<Vec<_>>();
    let register_mask = if register_ids.len() >= u64::BITS as usize {
        u64::MAX
    } else {
        1_u64
            .checked_shl(u32::try_from(register_ids.len()).unwrap_or(u32::MAX))
            .unwrap_or(0)
            .saturating_sub(1)
    };
    let mask = builder.ins().iconst(types::I64, register_mask as i64);
    builder.ins().store(
        MemFlagsData::new(),
        mask,
        state_out,
        std::mem::offset_of!(crate::JitDeoptState, initialized_register_mask) as i32,
    );
    for (snapshot_slot, register) in register_ids.iter().enumerate() {
        let value = use_region_register(builder, registers, *register)?;
        let value = if builder.func.dfg.value_type(value) == types::I64 {
            value
        } else {
            builder.ins().uextend(types::I64, value)
        };
        let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
            .saturating_add(snapshot_slot.saturating_mul(8));
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
    for (snapshot_slot, register) in register_ids.into_iter().enumerate() {
        let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
            .saturating_add(snapshot_slot.saturating_mul(8));
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
    native_reference_bind_helper: Option<NativeHelper>,
    native_value_lifecycle_helper: Option<NativeHelper>,
    value_flow: &ExecutableValueFlow,
    locals: &NativeLocalMap,
    register_variables: &NativeRegisterMap,
    registers: &mut NativeRegisterMap,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    call: &RegionNativeCall,
    source_block: BlockId,
    instruction: &RegionInstruction,
    transition_live_registers: &[RegId],
    streaming_call_exit: Option<NativeStreamingCallExit>,
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
        let pointer = allocate_native_stack_storage(builder, pointer_type, bytes, 3);
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
            let known_reference_requirement = known_user_argument_requires_reference(
                call,
                visible_index,
                function_params,
                function,
            );
            let requires_reference = known_reference_requirement
                .unwrap_or_else(|| call.argument_requires_reference_binding(visible_index));
            let defer_until_signature_published = requires_reference
                && known_reference_requirement.is_none()
                && matches!(
                    &call.target,
                    RegionCallTarget::Function { function: None, .. }
                )
                && argument
                    .and_then(|argument| argument.by_ref_local)
                    .is_some();
            let speculative_original_local = if requires_reference
                && !defer_until_signature_published
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
                    defer_until_signature_published,
                    true,
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
        let pointer = allocate_native_stack_storage(builder, pointer_type, bytes, 3);
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
    let frame_ptr = allocate_native_stack_storage(builder, pointer_type, frame_size, 3);
    let zero = builder.ins().iconst(types::I64, 0);
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
        std::mem::offset_of!(crate::JitNativeCallFrame, function_id),
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
    let receiver = if let RegionCallTarget::StaticMethod { class_name, .. } = &call.target
        && matches!(
            class_name.to_ascii_lowercase().as_str(),
            "self" | "parent" | "static"
        ) {
        use_local_variable(builder, locals, LocalId::new(0)).unwrap_or(zero)
    } else {
        zero
    };
    builder.ins().store(
        MemFlagsData::new(),
        receiver,
        frame_ptr,
        std::mem::offset_of!(crate::JitNativeCallFrame, receiver_handle) as i32,
    );
    let (kind, target_function, _, _) = native_call_target_metadata(&call.target);
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
    publish_native_register_state(builder, deopt_out, registers, transition_live_registers);
    let control_value = builder.ins().stack_load(types::I64, out_slot, 16);
    if let Some(streaming_call_exit) = streaming_call_exit {
        let continuation = builder
            .ins()
            .iconst(types::I32, i64::from(instruction.continuation_id));
        let masks = native_local_mask_words(&instruction.live_locals)
            .into_iter()
            .map(|mask| builder.ins().iconst(types::I64, mask as i64))
            .map(Into::into)
            .collect::<Vec<ir::BlockArg>>();
        let mut args = Vec::<ir::BlockArg>::with_capacity(3 + masks.len());
        args.push(status.into());
        args.push(control_value.into());
        args.push(continuation.into());
        args.extend(masks);
        builder.ins().jump(streaming_call_exit.block, &args);
    } else {
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
            .store(MemFlagsData::new(), control_value, result_out, 0);
        builder.ins().return_(&[status]);
    }
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
        define_local_variable(builder, locals, local, builder.block_params(merge)[0])?;
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
    match call.result {
        RegionCallResult::Register(register) => {
            define_region_register(builder, register_variables, registers, register, value)?;
        }
        RegionCallResult::ReferenceLocal(local) => {
            define_local_variable(builder, locals, local, value)?;
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
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    live_registers: &[RegId],
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
                live_registers,
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
        live_registers,
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
