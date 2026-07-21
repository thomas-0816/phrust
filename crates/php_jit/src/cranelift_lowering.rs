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
use std::cell::Cell;
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
use terminators::{
    lower_optimizing_region_terminator, lower_owned_frame_locals, lower_region_terminator,
};
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
    runtime: Option<ir::Value>,
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

    fn with_runtime(self, runtime: ir::Value) -> Self {
        Self {
            runtime: Some(runtime),
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
    runtime: Option<ir::Value>,
    builtin_dispatch: Option<NativeHelper>,
    semantic_dispatch: Option<NativeHelper>,
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
    value_release: Option<NativeHelper>,
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
    array_insert_local: Option<NativeHelper>,
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
    string_predicate: Option<NativeHelper>,
    runtime_fatal: Option<NativeHelper>,
    execution_poll: Option<NativeHelper>,
}

/// Runtime entrypoints are a baseline-only capability.  The optimizing tier
/// deliberately has no variant carrying helper addresses, so an optimizing
/// artifact cannot acquire a generic warm-runtime import by accident.
#[derive(Clone, Copy, Debug)]
enum NativeTierOperations {
    Baseline {
        call: Option<NativeHelper>,
        dynamic_code: Option<NativeHelper>,
        operations: NativeOperationFunctions,
    },
    Optimizing,
}

impl NativeOperationFunctions {
    fn with_runtime(mut self, runtime: ir::Value) -> Self {
        self.runtime = Some(runtime);
        macro_rules! bind {
            ($($field:ident),+ $(,)?) => {
                $(self.$field = self.$field.map(|helper| helper.with_runtime(runtime));)+
            };
        }
        bind!(
            builtin_dispatch,
            semantic_dispatch,
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
            value_release,
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
            array_insert_local,
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
            string_predicate,
            runtime_fatal,
            execution_poll,
        );
        self
    }

    fn with_terminal_exit(mut self, terminal_exit: NativeTerminalExit) -> Self {
        macro_rules! bind {
            ($($field:ident),+ $(,)?) => {
                $(self.$field = self.$field.map(|helper| helper.with_terminal_exit(terminal_exit));)+
            };
        }
        bind!(
            builtin_dispatch,
            semantic_dispatch,
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
            value_release,
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
            array_insert_local,
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
            string_predicate,
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
    let runtime = helper
        .runtime
        .expect("native helper must be bound to the request fast-state value");
    let mut direct_arguments = Vec::with_capacity(arguments.len() + 1);
    direct_arguments.push(runtime);
    direct_arguments.extend_from_slice(arguments);
    builder.ins().call(callee, &direct_arguments)
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
        compiler_tier: if request.opt_level < 2 {
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
const NATIVE_BUILTIN_DISPATCH_SYMBOL: &str = "phrust_jit_native_builtin_dispatch";
const NATIVE_SEMANTIC_DISPATCH_SYMBOL: &str = "phrust_jit_native_semantic_dispatch";
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
            request.compile.opt_level >= 2,
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
        tier: if request.compile.opt_level < 2 {
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
            if request.compile.opt_level >= 2
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

fn lower_prepared_native_call_operand(
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    constants: &[IrConstant],
    operand: RegionOperand,
) -> Result<ir::Value, CraneliftLoweringError> {
    let RegionOperand::Constant(constant) = operand else {
        return lower_region_operand(builder, locals, registers, operand);
    };
    let value = match constants.get(constant as usize) {
        Some(IrConstant::Null) => crate::jit_encode_constant(u32::MAX),
        Some(IrConstant::Bool(false)) => crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
        Some(IrConstant::Bool(true)) => crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        Some(IrConstant::Int(value)) => *value,
        _ => crate::jit_encode_constant(constant),
    };
    Ok(builder.ins().iconst(types::I64, value))
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
    publish_native_continuation_state(
        builder,
        deopt_out,
        function,
        local_count,
        instruction.continuation_id,
        &instruction.live_locals,
        locals,
        native_version,
    )
}

#[allow(clippy::too_many_arguments)]
fn publish_native_continuation_state(
    builder: &mut FunctionBuilder<'_>,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    continuation_id: u32,
    live_locals: &[LocalId],
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
        continuation_id,
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
    publish_native_local_masks(builder, deopt_out, live_locals);
    if !copy_native_local_state_values(
        builder,
        deopt_out,
        locals,
        live_locals,
        NativeLocalCopyDirection::FrameToState,
    )? {
        for local in live_locals {
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
) -> Result<(), CraneliftLoweringError> {
    let live_values = live_registers
        .iter()
        .copied()
        .map(|register| Ok((register, use_region_register(builder, registers, register)?)))
        .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
    publish_native_register_values(builder, state_out, &live_values)
}

fn publish_native_register_values(
    builder: &mut FunctionBuilder<'_>,
    state_out: ir::Value,
    live_values: &[(RegId, ir::Value)],
) -> Result<(), CraneliftLoweringError> {
    if live_values.len() > crate::JIT_DEOPT_MAX_REGISTERS {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_TRANSITION_REGISTER_LIMIT",
            format!(
                "native transition requires {} registers but the ABI supports {}",
                live_values.len(),
                crate::JIT_DEOPT_MAX_REGISTERS
            ),
        ));
    }
    // Snapshot slots are an ABI shared with the statically generated resume
    // loader. Never filter or compact this list independently: doing so moves
    // every later value to a different slot and restores it into the wrong
    // SSA register. A required value missing at this program point is a
    // lowering error, not an optional snapshot entry.
    let initialized_count = live_values.len();
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
    for (snapshot_slot, (register, value)) in live_values.iter().copied().enumerate() {
        let register_id = builder.ins().iconst(types::I32, i64::from(register.raw()));
        let id_offset = std::mem::offset_of!(crate::JitDeoptState, register_ids)
            .saturating_add(snapshot_slot.saturating_mul(4));
        builder.ins().store(
            MemFlagsData::new(),
            register_id,
            state_out,
            id_offset as i32,
        );
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
    Ok(())
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
    let status = builder.inst_results(call)[0];
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), result_out, 0);
    require_native_value_operation_ok(builder, status, helper.terminal_exit()?, value)?;
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
        let locator = builder
            .ins()
            .iconst(types::I64, i64::from(instruction.continuation_id));
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
        let locator = builder
            .ins()
            .iconst(types::I64, i64::from(instruction.continuation_id));
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
    publish_native_register_state(builder, deopt_out, registers, live_registers)?;
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

fn lower_fast_array_key_exists(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    array: ir::Value,
    key: ir::Value,
    result_out: ir::Value,
) -> Result<(ir::Value, ir::Value), CraneliftLoweringError> {
    let helper = helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_ARRAY_KEY_EXISTS",
            "array_key_exists fast path has no declared array lookup helper",
        )
    })?;
    let operation = builder.ins().iconst(types::I32, 2);
    let args = [operation, array, key, result_out];
    let call = call_native_helper(module, builder, helper, &args);
    let status = builder.inst_results(call)[0];
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), result_out, 0);
    Ok((status, value))
}

fn lower_fast_string_predicate(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    operation: u32,
    haystack: ir::Value,
    needle: ir::Value,
    result_out: ir::Value,
) -> Result<(ir::Value, ir::Value), CraneliftLoweringError> {
    let helper = helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_PREDICATE",
            "string predicate fast path has no declared runtime helper",
        )
    })?;
    let operation = builder.ins().iconst(types::I32, i64::from(operation));
    let call = call_native_helper(
        module,
        builder,
        helper,
        &[operation, haystack, needle, result_out],
    );
    let status = builder.inst_results(call)[0];
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), result_out, 0);
    Ok((status, value))
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
    let tag_matches = lower_value_has_tag(builder, value, expected_tag);
    builder.ins().brif(tag_matches, inspect, &[], slow, &[]);

    builder.switch_to_block(inspect);
    let descriptor = lower_optimizing_slot_address(builder, value, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let expected_kind = if op == 0 {
        crate::JIT_NATIVE_VALUE_VIEW_STRING
    } else {
        crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
    };
    let representation_matches =
        builder
            .ins()
            .icmp_imm(IntCC::Equal, kind, i64::from(expected_kind));
    builder
        .ins()
        .brif(representation_matches, direct, &[], slow, &[]);

    builder.switch_to_block(direct);
    let payload = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().jump(merge, &[payload.into()]);

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
#[derive(Clone, Copy)]
struct NativeOptimizingTransition<'a> {
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    instruction: &'a RegionInstruction,
    locals: &'a NativeLocalMap,
    live_values: &'a [(RegId, ir::Value)],
    native_version: u32,
    emitted_transition: &'a Cell<bool>,
}

impl NativeOptimizingTransition<'_> {
    fn emit_value(
        self,
        builder: &mut FunctionBuilder<'_>,
    ) -> Result<ir::Value, CraneliftLoweringError> {
        self.emit_value_with_detail(builder, 0)
    }

    fn emit_value_with_detail(
        self,
        builder: &mut FunctionBuilder<'_>,
        detail: u32,
    ) -> Result<ir::Value, CraneliftLoweringError> {
        self.emitted_transition.set(true);
        publish_native_call_state(
            builder,
            self.deopt_out,
            self.function,
            self.local_count,
            self.instruction,
            self.locals,
            self.native_version,
        )?;
        publish_native_register_values(builder, self.deopt_out, self.live_values)?;
        let detail = builder.ins().iconst(types::I32, i64::from(detail));
        builder.ins().store(
            MemFlagsData::new(),
            detail,
            self.deopt_out,
            std::mem::offset_of!(crate::JitDeoptState, control_reserved) as i32,
        );
        let value = builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
        builder
            .ins()
            .store(MemFlagsData::new(), value, self.result_out, 0);
        let status = builder.ins().iconst(
            types::I32,
            i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
        );
        builder.ins().return_(&[status]);

        // Keep lowering structurally total after the terminal transition. The
        // block has no predecessor and is removed before machine-code emission.
        let unreachable = builder.create_block();
        builder.switch_to_block(unreachable);
        builder.seal_block(unreachable);
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EmittedOptimizingInstruction {
    pub(super) class: crate::JitProductionLoweringClass,
    pub(super) operation_local_transition: bool,
}

#[derive(Clone, Copy)]
enum NativeArrayAppendFallback<'a> {
    Optimizing(NativeOptimizingTransition<'a>),
    Baseline {
        helper: Option<NativeHelper>,
        lifecycle: Option<NativeHelper>,
        operation: u32,
        function: FunctionId,
        local_count: u32,
        instruction: &'a RegionInstruction,
        locals: &'a NativeLocalMap,
        native_version: u32,
    },
}

#[allow(clippy::too_many_arguments)]
fn lower_array_write_fallback(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    fallback: NativeArrayAppendFallback<'_>,
    array: ir::Value,
    key: ir::Value,
    value: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    match fallback {
        NativeArrayAppendFallback::Optimizing(transition) => transition.emit_value(builder),
        NativeArrayAppendFallback::Baseline {
            helper,
            operation,
            function,
            local_count,
            instruction,
            locals,
            native_version,
            ..
        } => lower_native_value_operation_with_state(
            module,
            builder,
            helper,
            operation,
            &[array, key, value],
            result_out,
            deopt_out,
            function,
            local_count,
            instruction,
            locals,
            native_version,
        ),
    }
}

fn lower_optimizing_slot_address(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    deopt_out: ir::Value,
) -> ir::Value {
    let direct_path = builder.create_block();
    let normal_path = builder.create_block();
    let merge = builder.create_block();
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    builder.append_block_param(merge, pointer_type);
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let normal_slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, value_slots) as i32,
    );
    let direct_slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let index = builder.ins().ireduce(types::I32, value);
    let direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    builder
        .ins()
        .brif(direct, direct_path, &[], normal_path, &[]);

    builder.switch_to_block(direct_path);
    let direct_index = builder
        .ins()
        .iadd_imm(index, -i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE));
    let direct_index = builder.ins().uextend(pointer_type, direct_index);
    let direct_offset = builder.ins().ishl_imm(direct_index, 5);
    let direct_slot = builder.ins().iadd(direct_slots, direct_offset);
    builder.ins().jump(merge, &[direct_slot.into()]);

    builder.switch_to_block(normal_path);
    let normal_index = builder.ins().uextend(pointer_type, index);
    let normal_offset = builder.ins().ishl_imm(normal_index, 5);
    let normal_slot = builder.ins().iadd(normal_slots, normal_offset);
    builder.ins().jump(merge, &[normal_slot.into()]);

    builder.switch_to_block(merge);
    builder.block_params(merge)[0]
}

/// Reserve one request-owned direct value slot without entering Rust. Released
/// slots form an intrusive single-linked list through `reserved`; only when
/// that list is empty does allocation advance the stable arena high-water.
fn lower_reserve_direct_value_index(
    builder: &mut FunctionBuilder<'_>,
    deopt_out: ir::Value,
    rejected: ir::Block,
) -> ir::Value {
    let reuse = builder.create_block();
    let bump = builder.create_block();
    let bump_accepted = builder.create_block();
    let allocated = builder.create_block();
    builder.append_block_param(allocated, types::I32);
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_next) as i32,
    );
    let free_head_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_free_head) as i32,
    );
    let free_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), free_head_ptr, 0);
    let has_free = builder.ins().icmp_imm(
        IntCC::NotEqual,
        free_head,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE),
    );
    builder.ins().brif(has_free, reuse, &[], bump, &[]);

    builder.switch_to_block(reuse);
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let free_index = builder.ins().uextend(pointer_type, free_head);
    let free_offset = builder.ins().ishl_imm(free_index, 5);
    let free_slot = builder.ins().iadd(slots, free_offset);
    let preceding = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        free_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    builder
        .ins()
        .store(MemFlagsData::new(), preceding, free_head_ptr, 0);
    builder.ins().jump(allocated, &[free_head.into()]);

    builder.switch_to_block(bump);
    let next = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), next_ptr, 0);
    let has_room = builder.ins().icmp_imm(
        IntCC::UnsignedLessThan,
        next,
        crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY as i64,
    );
    builder
        .ins()
        .brif(has_room, bump_accepted, &[], rejected, &[]);

    builder.switch_to_block(bump_accepted);
    let next_value = builder.ins().iadd_imm(next, 1);
    builder
        .ins()
        .store(MemFlagsData::new(), next_value, next_ptr, 0);
    builder.ins().jump(allocated, &[next.into()]);

    builder.switch_to_block(allocated);
    builder.block_params(allocated)[0]
}

fn lower_direct_new_array(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    result_out: ir::Value,
    deopt_out: ir::Value,
    optimizing_transition: Option<NativeOptimizingTransition<'_>>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let accepted = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let entry_next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_next) as i32,
    );
    let entry_next = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), entry_next_ptr, 0);
    let entry_end = builder.ins().iadd_imm(
        entry_next,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY),
    );
    let entry_room = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        entry_end,
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
    );
    builder.ins().brif(entry_room, accepted, &[], rejected, &[]);

    builder.switch_to_block(rejected);
    let placeholder = if let Some(transition) = optimizing_transition {
        transition.emit_value(builder)?
    } else {
        lower_native_value_operation(module, builder, helper, 0, &[], result_out)?
    };
    builder.ins().jump(merge, &[placeholder.into()]);

    builder.switch_to_block(accepted);
    let next = lower_reserve_direct_value_index(builder, deopt_out, rejected);
    builder
        .ins()
        .store(MemFlagsData::new(), entry_end, entry_next_ptr, 0);
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_entries) as i32,
    );
    let next_pointer = builder.ins().uextend(pointer_type, next);
    let slot_offset = builder.ins().ishl_imm(next_pointer, 5);
    let slot = builder.ins().iadd(slots, slot_offset);
    let entry_pointer = builder.ins().uextend(pointer_type, entry_next);
    let entry_offset = builder.ins().ishl_imm(entry_pointer, 4);
    let entry = builder.ins().iadd(entries, entry_offset);
    for (value, offset) in [
        (
            builder.ins().iconst(types::I32, 1),
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount),
        ),
        (
            builder.ins().iconst(
                types::I32,
                i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
            ),
            std::mem::offset_of!(crate::JitNativeValueSlot, kind),
        ),
        (
            builder.ins().iconst(
                types::I32,
                i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION),
            ),
            std::mem::offset_of!(crate::JitNativeValueSlot, flags),
        ),
        (
            builder.ins().iconst(
                types::I32,
                i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY),
            ),
            std::mem::offset_of!(crate::JitNativeValueSlot, reserved),
        ),
    ] {
        builder
            .ins()
            .store(MemFlagsData::new(), value, slot, offset as i32);
    }
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().store(
        MemFlagsData::new(),
        zero,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        entry,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let encoded_index = builder
        .ins()
        .iadd_imm(next, i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE));
    let encoded_index = builder.ins().uextend(types::I64, encoded_index);
    let encoded = builder
        .ins()
        .bor_imm(encoded_index, crate::JIT_VALUE_RUNTIME_ARRAY_TAG as i64);
    builder.ins().jump(merge, &[encoded.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_direct_array_append(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    key: Option<ir::Value>,
    value: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
    fallback: NativeArrayAppendFallback<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let inspect = builder.create_block();
    let inspect_capacity = builder.create_block();
    let inspect_growth = builder.create_block();
    let reuse_growth = builder.create_block();
    let bump_growth = builder.create_block();
    let growth_allocated = builder.create_block();
    let copy_entries = builder.create_block();
    let copy_entry = builder.create_block();
    let growth_done = builder.create_block();
    let append = builder.create_block();
    let rejected = builder.create_block();
    let done = builder.create_block();
    builder.append_block_param(copy_entries, types::I64);
    builder.append_block_param(growth_allocated, pointer_type);
    builder.append_block_param(done, types::I64);
    let array_kind = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    let index = builder.ins().ireduce(types::I32, array);
    let direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(array_kind, direct_index);
    builder.ins().brif(direct, inspect, &[], rejected, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, array, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let refcount = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let capacity = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let direct_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let unique = builder.ins().icmp_imm(IntCC::Equal, refcount, 1);
    let admitted = builder.ins().band(direct_kind, unique);
    builder
        .ins()
        .brif(admitted, inspect_capacity, &[], rejected, &[]);

    builder.switch_to_block(inspect_capacity);
    let capacity_wide = builder.ins().uextend(types::I64, capacity);
    let has_room = builder
        .ins()
        .icmp(IntCC::UnsignedLessThan, length, capacity_wide);
    builder
        .ins()
        .brif(has_room, append, &[], inspect_growth, &[]);

    // A direct array owns a contiguous slice in the request arena. Growing it
    // allocates a new slice, copies encoded entries without changing their
    // ownership, and atomically switches the descriptor before appending. The
    // old slice is dead arena storage, not a second owner. This removes the
    // previous capacity-eight transition into the Rust PhpArray path.
    builder.switch_to_block(inspect_growth);
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_next) as i32,
    );
    let arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_entries) as i32,
    );
    let next = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), next_ptr, 0);
    let doubled = builder.ins().imul_imm(capacity, 2);
    let minimum = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY),
    );
    let capacity_is_zero = builder.ins().icmp_imm(IntCC::Equal, capacity, 0);
    let grown_capacity = builder.ins().select(capacity_is_zero, minimum, doubled);
    let free_heads = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_free_heads) as i32,
    );
    let grown_leading_zeros = builder.ins().clz(grown_capacity);
    let bit_index_ceiling = builder.ins().iconst(types::I32, 31);
    let bucket = builder.ins().isub(bit_index_ceiling, grown_leading_zeros);
    let bucket_wide = builder.ins().uextend(pointer_type, bucket);
    let bucket_offset = builder.ins().ishl_imm(bucket_wide, 2);
    let free_head_ptr = builder.ins().iadd(free_heads, bucket_offset);
    let free_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), free_head_ptr, 0);
    let has_free = builder.ins().icmp_imm(
        IntCC::NotEqual,
        free_head,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE),
    );
    let old_entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    builder
        .ins()
        .brif(has_free, reuse_growth, &[], bump_growth, &[]);

    builder.switch_to_block(reuse_growth);
    let free_head_wide = builder.ins().uextend(pointer_type, free_head);
    let free_offset = builder.ins().ishl_imm(free_head_wide, 4);
    let reused_entries = builder.ins().iadd(arena, free_offset);
    let preceding_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), reused_entries, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), preceding_head, free_head_ptr, 0);
    builder
        .ins()
        .jump(growth_allocated, &[reused_entries.into()]);

    builder.switch_to_block(bump_growth);
    let grown_end = builder.ins().iadd(next, grown_capacity);
    let arena_room = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        grown_end,
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
    );
    let next_wide = builder.ins().uextend(pointer_type, next);
    let grown_offset = builder.ins().ishl_imm(next_wide, 4);
    let bumped_entries = builder.ins().iadd(arena, grown_offset);
    let bump_accepted = builder.create_block();
    builder
        .ins()
        .brif(arena_room, bump_accepted, &[], rejected, &[]);
    builder.switch_to_block(bump_accepted);
    builder
        .ins()
        .store(MemFlagsData::new(), grown_end, next_ptr, 0);
    builder
        .ins()
        .jump(growth_allocated, &[bumped_entries.into()]);

    builder.switch_to_block(growth_allocated);
    let grown_entries = builder.block_params(growth_allocated)[0];
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(copy_entries, &[zero.into()]);

    builder.switch_to_block(copy_entries);
    let copy_index = builder.block_params(copy_entries)[0];
    let copied_all = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, copy_index, length);
    builder
        .ins()
        .brif(copied_all, growth_done, &[], copy_entry, &[]);

    builder.switch_to_block(copy_entry);
    let copy_pointer = if pointer_type == types::I64 {
        copy_index
    } else {
        builder.ins().ireduce(pointer_type, copy_index)
    };
    let copy_offset = builder.ins().ishl_imm(copy_pointer, 4);
    let old_entry = builder.ins().iadd(old_entries, copy_offset);
    let new_entry = builder.ins().iadd(grown_entries, copy_offset);
    let copied_key = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), old_entry, 0);
    let copied_value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        old_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    builder
        .ins()
        .store(MemFlagsData::new(), copied_key, new_entry, 0);
    builder.ins().store(
        MemFlagsData::new(),
        copied_value,
        new_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let next_copy = builder.ins().iadd_imm(copy_index, 1);
    builder.ins().jump(copy_entries, &[next_copy.into()]);

    builder.switch_to_block(growth_done);
    // The copied range is no longer an owner. Publish it in the exact-size
    // request-local free bucket so the next growth reuses it without Rust.
    let old_leading_zeros = builder.ins().clz(capacity);
    let old_bit_index_ceiling = builder.ins().iconst(types::I32, 31);
    let old_bucket = builder.ins().isub(old_bit_index_ceiling, old_leading_zeros);
    let old_bucket_wide = builder.ins().uextend(pointer_type, old_bucket);
    let old_bucket_offset = builder.ins().ishl_imm(old_bucket_wide, 2);
    let old_head_ptr = builder.ins().iadd(free_heads, old_bucket_offset);
    let old_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), old_head_ptr, 0);
    let old_offset = builder.ins().isub(old_entries, arena);
    let old_index_wide = builder.ins().ushr_imm(old_offset, 4);
    let old_index = builder.ins().ireduce(types::I32, old_index_wide);
    builder
        .ins()
        .store(MemFlagsData::new(), old_head, old_entries, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), old_index, old_head_ptr, 0);
    builder.ins().store(
        MemFlagsData::new(),
        grown_entries,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        grown_capacity,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    builder.ins().jump(append, &[]);

    builder.switch_to_block(append);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let pointer_type = builder.func.dfg.value_type(entries);
    let entry_index = if pointer_type == types::I64 {
        length
    } else {
        builder.ins().ireduce(pointer_type, length)
    };
    let entry_offset = builder.ins().ishl_imm(entry_index, 4);
    let entry = builder.ins().iadd(entries, entry_offset);
    let entry_key = key.unwrap_or(length);
    lower_optimizing_retain(builder, entry_key, deopt_out);
    lower_optimizing_retain(builder, value, deopt_out);
    builder
        .ins()
        .store(MemFlagsData::new(), entry_key, entry, 0);
    builder.ins().store(
        MemFlagsData::new(),
        value,
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let next_length = builder.ins().iadd_imm(length, 1);
    builder.ins().store(
        MemFlagsData::new(),
        next_length,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().jump(done, &[array.into()]);

    builder.switch_to_block(rejected);
    let null = builder
        .ins()
        .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
    let updated = lower_array_write_fallback(
        module,
        builder,
        fallback,
        array,
        key.unwrap_or(null),
        value,
        result_out,
        deopt_out,
    )?;
    // A slow-path COW separation may return a distinct array handle.
    builder.ins().jump(done, &[updated.into()]);

    builder.switch_to_block(done);
    Ok(builder.block_params(done)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_direct_array_insert(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    key: ir::Value,
    value: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
    fallback: NativeArrayAppendFallback<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let inspect = builder.create_block();
    let search = builder.create_block();
    let compare = builder.create_block();
    let next = builder.create_block();
    let found = builder.create_block();
    let replace = builder.create_block();
    let missing = builder.create_block();
    let rejected = builder.create_block();
    let done = builder.create_block();
    let pointer_type = module.target_config().pointer_type();
    builder.append_block_param(search, types::I64);
    builder.append_block_param(next, types::I64);
    builder.append_block_param(found, pointer_type);
    builder.append_block_param(done, types::I64);

    let array_kind = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    let index = builder.ins().ireduce(types::I32, array);
    let direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(array_kind, direct_index);
    builder.ins().brif(direct, inspect, &[], rejected, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, array, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let refcount = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    let direct_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let unique = builder.ins().icmp_imm(IntCC::Equal, refcount, 1);
    let key_runtime = lower_is_runtime_handle(builder, key);
    let key_string = lower_value_has_tag(builder, key, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    let key_constant = lower_value_has_namespace_tag(builder, key, crate::JIT_VALUE_CONSTANT_TAG);
    let immediate = builder.ins().icmp_imm(IntCC::Equal, key_runtime, 0);
    let immediate = builder.ins().band_not(immediate, key_constant);
    let supported_key = match fallback {
        NativeArrayAppendFallback::Optimizing(_) => builder.ins().bor(immediate, key_string),
        // The baseline compatibility tier deliberately routes string-key
        // semantics through its typed cold operation. Replicating the byte
        // comparison loop at every large literal-table insertion inflated
        // functions such as remove_accents() beyond the fragment ceiling.
        NativeArrayAppendFallback::Baseline { .. } => immediate,
    };
    let admitted = builder.ins().band(direct_kind, unique);
    let admitted = builder.ins().band(admitted, supported_key);
    let zero = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .brif(admitted, search, &[zero.into()], rejected, &[]);

    builder.switch_to_block(search);
    let search_index = builder.block_params(search)[0];
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, search_index, length);
    builder.ins().brif(exhausted, missing, &[], compare, &[]);

    builder.switch_to_block(compare);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let entry_index = if pointer_type == types::I64 {
        search_index
    } else {
        builder.ins().ireduce(pointer_type, search_index)
    };
    let entry_offset = builder.ins().ishl_imm(entry_index, 4);
    let entry = builder.ins().iadd(entries, entry_offset);
    let candidate = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), entry, 0);
    let matches = match fallback {
        NativeArrayAppendFallback::Optimizing(_) => {
            lower_native_array_key_equal(builder, candidate, key, deopt_out)
        }
        NativeArrayAppendFallback::Baseline { .. } => {
            builder.ins().icmp(IntCC::Equal, candidate, key)
        }
    };
    builder.ins().brif(
        matches,
        found,
        &[entry.into()],
        next,
        &[search_index.into()],
    );

    builder.switch_to_block(next);
    let current_index = builder.block_params(next)[0];
    let next_index = builder.ins().iadd_imm(current_index, 1);
    builder.ins().jump(search, &[next_index.into()]);

    builder.switch_to_block(found);
    let entry = builder.block_params(found)[0];
    let old = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let unchanged = builder.ins().icmp(IntCC::Equal, old, value);
    builder
        .ins()
        .brif(unchanged, done, &[array.into()], replace, &[]);

    builder.switch_to_block(replace);
    match fallback {
        NativeArrayAppendFallback::Optimizing(transition) => {
            lower_optimizing_release(builder, old, transition)?;
        }
        NativeArrayAppendFallback::Baseline {
            lifecycle,
            operation,
            ..
        } => {
            let _ = lower_guarded_value_release(
                module,
                builder,
                lifecycle,
                operation | 1,
                old,
                result_out,
                deopt_out,
            )?;
        }
    }
    lower_optimizing_retain(builder, value, deopt_out);
    builder.ins().store(
        MemFlagsData::new(),
        value,
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    builder.ins().jump(done, &[array.into()]);

    builder.switch_to_block(missing);
    let updated = lower_direct_array_append(
        module,
        builder,
        array,
        Some(key),
        value,
        result_out,
        deopt_out,
        fallback,
    )?;
    builder.ins().jump(done, &[updated.into()]);

    builder.switch_to_block(rejected);
    let updated = lower_array_write_fallback(
        module, builder, fallback, array, key, value, result_out, deopt_out,
    )?;
    builder.ins().jump(done, &[updated.into()]);

    builder.switch_to_block(done);
    Ok(builder.block_params(done)[0])
}

/// Compare PHP array keys without reconstructing a Rust `Value` or crossing a
/// runtime-helper boundary. Integer keys compare as encoded immediates. String
/// keys compare their publication-owned byte views, so independently encoded
/// handles with equal contents name the same PHP array element.
fn lower_native_string_key_descriptor(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    deopt_out: ir::Value,
) -> (ir::Value, ir::Value, ir::Value) {
    let inspect_constant = builder.create_block();
    let runtime = builder.create_block();
    let constant = builder.create_block();
    let invalid = builder.create_block();
    let merge = builder.create_block();
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    builder.append_block_param(merge, types::I8);
    builder.append_block_param(merge, types::I64);
    builder.append_block_param(merge, pointer_type);

    let runtime_string = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    builder
        .ins()
        .brif(runtime_string, runtime, &[], inspect_constant, &[]);

    builder.switch_to_block(inspect_constant);
    let constant_tag = lower_value_has_namespace_tag(builder, value, crate::JIT_VALUE_CONSTANT_TAG);
    let index = builder.ins().ireduce(types::I32, value);
    let runtime_view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let count = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        deopt_out,
        runtime_view_offset
            + std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_constant_view_count) as i32,
    );
    let in_bounds = builder.ins().icmp(IntCC::UnsignedLessThan, index, count);
    let admitted = builder.ins().band(constant_tag, in_bounds);
    builder.ins().brif(admitted, constant, &[], invalid, &[]);

    builder.switch_to_block(runtime);
    let slot = lower_optimizing_slot_address(builder, value, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let kind_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_STRING),
    );
    let version_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        flags,
        i64::from(crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION),
    );
    let valid = builder.ins().band(kind_ok, version_ok);
    builder
        .ins()
        .jump(merge, &[valid.into(), length.into(), bytes.into()]);

    builder.switch_to_block(constant);
    let views = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        runtime_view_offset
            + std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_constant_views) as i32,
    );
    let wide_index = builder.ins().uextend(pointer_type, index);
    let offset = builder.ins().imul_imm(
        wide_index,
        i64::try_from(std::mem::size_of::<crate::JitNativeConstantView>()).unwrap_or(i64::MAX),
    );
    let descriptor = builder.ins().iadd(views, offset);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeConstantView, kind) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeConstantView, length) as i32,
    );
    let bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeConstantView, bytes) as i32,
    );
    let valid = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_CONSTANT_VIEW_STRING),
    );
    builder
        .ins()
        .jump(merge, &[valid.into(), length.into(), bytes.into()]);

    builder.switch_to_block(invalid);
    let no = builder.ins().iconst(types::I8, 0);
    let zero = builder.ins().iconst(types::I64, 0);
    let null = builder.ins().iconst(pointer_type, 0);
    builder
        .ins()
        .jump(merge, &[no.into(), zero.into(), null.into()]);

    builder.switch_to_block(merge);
    (
        builder.block_params(merge)[0],
        builder.block_params(merge)[1],
        builder.block_params(merge)[2],
    )
}

fn lower_native_array_key_equal(
    builder: &mut FunctionBuilder<'_>,
    lhs: ir::Value,
    rhs: ir::Value,
    deopt_out: ir::Value,
) -> ir::Value {
    let inspect_strings = builder.create_block();
    let compare_length = builder.create_block();
    let compare_loop = builder.create_block();
    let compare_byte = builder.create_block();
    let next_byte = builder.create_block();
    let matched = builder.create_block();
    let different = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(compare_loop, types::I64);
    builder.append_block_param(next_byte, types::I64);
    builder.append_block_param(merge, types::I8);

    let identical = builder.ins().icmp(IntCC::Equal, lhs, rhs);
    builder
        .ins()
        .brif(identical, matched, &[], inspect_strings, &[]);

    builder.switch_to_block(inspect_strings);
    let (lhs_valid, lhs_length, lhs_bytes) =
        lower_native_string_key_descriptor(builder, lhs, deopt_out);
    let (rhs_valid, rhs_length, rhs_bytes) =
        lower_native_string_key_descriptor(builder, rhs, deopt_out);
    let descriptors_ok = builder.ins().band(lhs_valid, rhs_valid);
    builder
        .ins()
        .brif(descriptors_ok, compare_length, &[], different, &[]);

    builder.switch_to_block(compare_length);
    let same_length = builder.ins().icmp(IntCC::Equal, lhs_length, rhs_length);
    let zero = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .brif(same_length, compare_loop, &[zero.into()], different, &[]);

    builder.switch_to_block(compare_loop);
    let index = builder.block_params(compare_loop)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, lhs_length);
    builder
        .ins()
        .brif(exhausted, matched, &[], compare_byte, &[]);

    builder.switch_to_block(compare_byte);
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let byte_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let lhs_address = builder.ins().iadd(lhs_bytes, byte_index);
    let rhs_address = builder.ins().iadd(rhs_bytes, byte_index);
    let lhs_byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), lhs_address, 0);
    let rhs_byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), rhs_address, 0);
    let equal = builder.ins().icmp(IntCC::Equal, lhs_byte, rhs_byte);
    builder
        .ins()
        .brif(equal, next_byte, &[index.into()], different, &[]);

    builder.switch_to_block(next_byte);
    let index = builder.block_params(next_byte)[0];
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(compare_loop, &[next.into()]);

    builder.switch_to_block(matched);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[yes.into()]);

    builder.switch_to_block(different);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[no.into()]);

    builder.switch_to_block(merge);
    builder.block_params(merge)[0]
}

/// Resolve `isset($reference[$key])` against the immutable array view owned by
/// the reference cell. The descriptor is invalidated before every PHP-visible
/// mutation, so the admitted path needs neither a Rust `Value` conversion nor
/// a runtime helper.
fn lower_optimizing_reference_array_isset(
    builder: &mut FunctionBuilder<'_>,
    reference: ir::Value,
    key: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let inspect_view = builder.create_block();
    let loop_block = builder.create_block();
    let compare = builder.create_block();
    let next = builder.create_block();
    let found = builder.create_block();
    let missing = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(loop_block, types::I64);
    builder.append_block_param(next, types::I64);
    builder.append_block_param(found, types::I32);
    builder.append_block_param(merge, types::I64);

    let slot = lower_optimizing_slot_address(builder, reference, transition.deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let view = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
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
    let view_ok = builder.ins().icmp_imm(IntCC::NotEqual, view, 0);
    let admitted = builder.ins().band(kind_ok, flags_ok);
    let admitted = builder.ins().band(admitted, view_ok);
    builder
        .ins()
        .brif(admitted, inspect_view, &[], rejected, &[]);

    builder.switch_to_block(inspect_view);
    let abi_version = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, abi_version) as i32,
    );
    let state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, state) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, length) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, entries) as i32,
    );
    let version_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        abi_version,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION),
    );
    let published = builder.ins().icmp_imm(
        IntCC::Equal,
        state,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED),
    );
    let key_runtime = lower_is_runtime_handle(builder, key);
    let key_constant = lower_value_has_namespace_tag(builder, key, crate::JIT_VALUE_CONSTANT_TAG);
    let namespaced = builder.ins().bor(key_runtime, key_constant);
    let immediate = builder.ins().icmp_imm(IntCC::Equal, namespaced, 0);
    let string = lower_value_has_tag(builder, key, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    let key_supported = builder.ins().bor(immediate, string);
    let key_supported = builder.ins().bor(key_supported, key_constant);
    let admitted = builder.ins().band(version_ok, published);
    let admitted = builder.ins().band(admitted, key_supported);
    let zero = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .brif(admitted, loop_block, &[zero.into()], rejected, &[]);

    builder.switch_to_block(loop_block);
    let index = builder.block_params(loop_block)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(exhausted, missing, &[], compare, &[]);

    builder.switch_to_block(compare);
    let entry_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let entry_offset = builder.ins().ishl_imm(entry_index, 6);
    let entry = builder.ins().iadd(entries, entry_offset);
    let entry_kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, kind) as i32,
    );
    let integer = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, integer) as i32,
    );
    let int_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        entry_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_KEY_INT),
    );
    let integer_equal = builder.ins().icmp(IntCC::Equal, integer, key);
    let integer_equal = builder.ins().band(int_kind, integer_equal);
    let integer_equal = builder.ins().band(immediate, integer_equal);

    let (string_valid, key_length, key_bytes) =
        lower_native_string_key_descriptor(builder, key, transition.deopt_out);
    let entry_string_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        entry_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_KEY_STRING),
    );
    let entry_length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, string_length) as i32,
    );
    let entry_bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, string_bytes) as i32,
    );
    let same_length = builder.ins().icmp(IntCC::Equal, entry_length, key_length);
    let string_admitted = builder.ins().band(string_valid, entry_string_kind);
    let string_admitted = builder.ins().band(string_admitted, same_length);
    let string_gate = builder.create_block();
    let string_compare = builder.create_block();
    let string_byte = builder.create_block();
    let string_next = builder.create_block();
    let string_matched = builder.create_block();
    let different = builder.create_block();
    builder.append_block_param(string_compare, types::I64);
    builder.append_block_param(string_byte, types::I64);
    builder.append_block_param(string_next, types::I64);
    builder
        .ins()
        .brif(integer_equal, string_matched, &[], string_gate, &[]);

    builder.switch_to_block(string_gate);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().brif(
        string_admitted,
        string_compare,
        &[zero.into()],
        different,
        &[],
    );

    builder.switch_to_block(string_compare);
    let byte_index = builder.block_params(string_compare)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, byte_index, key_length);
    builder.ins().brif(
        exhausted,
        string_matched,
        &[],
        string_byte,
        &[byte_index.into()],
    );

    builder.switch_to_block(string_byte);
    let byte_index = builder.block_params(string_byte)[0];
    let offset = if pointer_type == types::I64 {
        byte_index
    } else {
        builder.ins().ireduce(pointer_type, byte_index)
    };
    let lhs = builder.ins().iadd(entry_bytes, offset);
    let rhs = builder.ins().iadd(key_bytes, offset);
    let lhs = builder.ins().load(types::I8, MemFlagsData::new(), lhs, 0);
    let rhs = builder.ins().load(types::I8, MemFlagsData::new(), rhs, 0);
    let equal = builder.ins().icmp(IntCC::Equal, lhs, rhs);
    builder
        .ins()
        .brif(equal, string_next, &[byte_index.into()], different, &[]);

    builder.switch_to_block(string_next);
    let byte_index = builder.block_params(string_next)[0];
    let byte_index = builder.ins().iadd_imm(byte_index, 1);
    builder.ins().jump(string_compare, &[byte_index.into()]);

    builder.switch_to_block(string_matched);
    let non_null = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, non_null) as i32,
    );
    builder.ins().jump(found, &[non_null.into()]);

    builder.switch_to_block(different);
    builder.ins().jump(next, &[index.into()]);

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(loop_block, &[index.into()]);

    builder.switch_to_block(found);
    let non_null = builder.block_params(found)[0];
    let non_null = builder.ins().ireduce(types::I8, non_null);
    let value = encode_native_bool(builder, non_null);
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(missing);
    let value = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(rejected);
    let value = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_direct_foreach_init(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    source: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
    optimizing_transition: Option<NativeOptimizingTransition<'_>>,
    baseline_helper: Option<NativeHelper>,
    function: FunctionId,
    continuation_id: u32,
) -> Result<ir::Value, CraneliftLoweringError> {
    let inspect = builder.create_block();
    let allocate = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let source_kind = lower_value_has_tag(builder, source, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    let source_index = builder.ins().ireduce(types::I32, source);
    let direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        source_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(source_kind, direct_index);
    builder.ins().brif(direct, inspect, &[], rejected, &[]);

    builder.switch_to_block(inspect);
    let source_slot = lower_optimizing_slot_address(builder, source, deopt_out);
    let source_kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        source_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let direct_array = builder.ins().icmp_imm(
        IntCC::Equal,
        source_kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    builder
        .ins()
        .brif(direct_array, allocate, &[], rejected, &[]);

    builder.switch_to_block(allocate);
    let next = lower_reserve_direct_value_index(builder, deopt_out, rejected);
    lower_optimizing_retain(builder, source, deopt_out);
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let next_pointer = builder.ins().uextend(pointer_type, next);
    let slot_offset = builder.ins().ishl_imm(next_pointer, 5);
    let slot = builder.ins().iadd(slots, slot_offset);
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        source_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    for (value, offset) in [
        (
            builder.ins().iconst(types::I32, 1),
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount),
        ),
        (
            builder.ins().iconst(
                types::I32,
                i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH),
            ),
            std::mem::offset_of!(crate::JitNativeValueSlot, kind),
        ),
        (
            builder.ins().iconst(
                types::I32,
                i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION),
            ),
            std::mem::offset_of!(crate::JitNativeValueSlot, flags),
        ),
        (
            builder.ins().ireduce(types::I32, length),
            std::mem::offset_of!(crate::JitNativeValueSlot, reserved),
        ),
    ] {
        builder
            .ins()
            .store(MemFlagsData::new(), value, slot, offset as i32);
    }
    builder.ins().store(
        MemFlagsData::new(),
        source,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().store(
        MemFlagsData::new(),
        zero,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let encoded_index = builder
        .ins()
        .iadd_imm(next, i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE));
    let encoded_index = builder.ins().uextend(types::I64, encoded_index);
    let iterator = builder
        .ins()
        .bor_imm(encoded_index, crate::JIT_VALUE_RUNTIME_ITERATOR_TAG as i64);
    builder.ins().jump(merge, &[iterator.into()]);

    builder.switch_to_block(rejected);
    let placeholder = if let Some(transition) = optimizing_transition {
        transition.emit_value(builder)?
    } else {
        let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
        let continuation = builder.ins().iconst(types::I64, i64::from(continuation_id));
        lower_native_value_operation(
            module,
            builder,
            baseline_helper,
            0,
            &[source, function, continuation],
            result_out,
        )?
    };
    builder.ins().jump(merge, &[placeholder.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_direct_arena_foreach_next(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    iterator: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
    optimizing_transition: Option<NativeOptimizingTransition<'_>>,
    baseline_helper: Option<NativeHelper>,
    baseline_lifecycle: Option<NativeHelper>,
) -> Result<(ir::Value, ir::Value, ir::Value), CraneliftLoweringError> {
    let inspect = builder.create_block();
    let advance = builder.create_block();
    let present = builder.create_block();
    let exhausted = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);
    builder.append_block_param(merge, types::I64);
    builder.append_block_param(merge, types::I64);

    let iterator_kind =
        lower_value_has_tag(builder, iterator, crate::JIT_VALUE_RUNTIME_ITERATOR_TAG);
    let index = builder.ins().ireduce(types::I32, iterator);
    let direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(iterator_kind, direct_index);
    builder.ins().brif(direct, inspect, &[], rejected, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, iterator, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let direct_iterator = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH),
    );
    builder
        .ins()
        .brif(direct_iterator, advance, &[], rejected, &[]);

    builder.switch_to_block(advance);
    let source = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let cursor = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let length = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let length = builder.ins().uextend(types::I64, length);
    let has_value = builder.ins().icmp(IntCC::UnsignedLessThan, cursor, length);
    builder.ins().brif(has_value, present, &[], exhausted, &[]);

    builder.switch_to_block(present);
    let source_slot = lower_optimizing_slot_address(builder, source, deopt_out);
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        source_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let entry_index = if pointer_type == types::I64 {
        cursor
    } else {
        builder.ins().ireduce(pointer_type, cursor)
    };
    let entry_offset = builder.ins().ishl_imm(entry_index, 4);
    let entry = builder.ins().iadd(entries, entry_offset);
    let key = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, key) as i32,
    );
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    lower_optimizing_retain(builder, key, deopt_out);
    lower_optimizing_retain(builder, value, deopt_out);
    let next = builder.ins().iadd_imm(cursor, 1);
    builder.ins().store(
        MemFlagsData::new(),
        next,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let one = builder.ins().iconst(types::I64, 1);
    builder
        .ins()
        .jump(merge, &[key.into(), value.into(), one.into()]);

    builder.switch_to_block(exhausted);
    let null = builder
        .ins()
        .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
    let zero = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .jump(merge, &[null.into(), null.into(), zero.into()]);

    builder.switch_to_block(rejected);
    if let Some(transition) = optimizing_transition {
        let placeholder = transition.emit_value(builder)?;
        builder.ins().jump(
            merge,
            &[placeholder.into(), placeholder.into(), placeholder.into()],
        );
    } else {
        let (key, value, has) = lower_direct_foreach_next(
            module,
            builder,
            baseline_helper,
            baseline_lifecycle,
            iterator,
            result_out,
            deopt_out,
        )?;
        builder
            .ins()
            .jump(merge, &[key.into(), value.into(), has.into()]);
    }

    builder.switch_to_block(merge);
    let params = builder.block_params(merge);
    Ok((params[0], params[1], params[2]))
}

#[allow(clippy::too_many_arguments)]
fn lower_direct_arena_foreach_cleanup(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    iterator: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
    optimizing_transition: Option<NativeOptimizingTransition<'_>>,
    baseline_helper: Option<NativeHelper>,
    baseline_lifecycle: Option<NativeHelper>,
) -> Result<(), CraneliftLoweringError> {
    let inspect = builder.create_block();
    let cleanup = builder.create_block();
    let rejected = builder.create_block();
    let done = builder.create_block();
    let iterator_kind =
        lower_value_has_tag(builder, iterator, crate::JIT_VALUE_RUNTIME_ITERATOR_TAG);
    let index = builder.ins().ireduce(types::I32, iterator);
    let direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(iterator_kind, direct_index);
    builder.ins().brif(direct, inspect, &[], rejected, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, iterator, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let direct_iterator = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH),
    );
    builder
        .ins()
        .brif(direct_iterator, cleanup, &[], rejected, &[]);

    builder.switch_to_block(cleanup);
    let source = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    if let Some(transition) = optimizing_transition {
        lower_optimizing_release(builder, source, transition)?;
    } else {
        let _ = lower_guarded_value_release(
            module,
            builder,
            baseline_lifecycle,
            1,
            source,
            result_out,
            deopt_out,
        )?;
    }
    let zero = builder.ins().iconst(types::I32, 0);
    builder.ins().store(
        MemFlagsData::new(),
        zero,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        zero,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    builder.ins().jump(done, &[]);

    builder.switch_to_block(rejected);
    if let Some(transition) = optimizing_transition {
        let _ = transition.emit_value(builder)?;
    } else {
        let helper = baseline_helper.ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                "native foreach-cleanup helper was not declared",
            )
        })?;
        let call = call_native_helper(module, builder, helper, &[iterator]);
        require_native_operation_ok(
            builder,
            builder.inst_results(call)[0],
            helper.terminal_exit()?,
        )?;
    }
    builder.ins().jump(done, &[]);

    builder.switch_to_block(done);
    Ok(())
}

fn lower_optimizing_retain(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    deopt_out: ir::Value,
) {
    let retain = builder.create_block();
    let done = builder.create_block();
    let runtime = lower_is_runtime_handle(builder, value);
    builder.ins().brif(runtime, retain, &[], done, &[]);

    builder.switch_to_block(retain);
    let slot = lower_optimizing_slot_address(builder, value, deopt_out);
    let refcount = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    let retained = builder.ins().iadd_imm(refcount, 1);
    builder.ins().store(
        MemFlagsData::new(),
        retained,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    builder.ins().jump(done, &[]);

    builder.switch_to_block(done);
}

fn lower_optimizing_release(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<(), CraneliftLoweringError> {
    let inspect = builder.create_block();
    let release = builder.create_block();
    let last = builder.create_block();
    let done = builder.create_block();
    let runtime = lower_is_runtime_handle(builder, value);
    builder.ins().brif(runtime, inspect, &[], done, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, value, transition.deopt_out);
    let refcount = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    let is_last = builder.ins().icmp_imm(IntCC::Equal, refcount, 1);
    builder.ins().brif(is_last, last, &[], release, &[]);

    builder.switch_to_block(release);
    let released = builder.ins().iadd_imm(refcount, -1);
    builder.ins().store(
        MemFlagsData::new(),
        released,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    builder.ins().jump(done, &[]);

    builder.switch_to_block(last);
    let _ = transition.emit_value(builder)?;
    builder.ins().jump(done, &[]);

    builder.switch_to_block(done);
    Ok(())
}

fn lower_optimizing_type_predicate(
    builder: &mut FunctionBuilder<'_>,
    operation: u32,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let direct = builder.create_block();
    let reference = builder.create_block();
    let is_reference = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    builder
        .ins()
        .brif(is_reference, reference, &[], direct, &[]);

    builder.switch_to_block(reference);
    let _ = transition.emit_value(builder)?;
    builder.ins().jump(direct, &[]);

    builder.switch_to_block(direct);
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
    let is_bool = builder.ins().bor(is_false, is_true);
    let is_constant = lower_value_has_namespace_tag(builder, value, crate::JIT_VALUE_CONSTANT_TAG);
    let is_runtime = lower_is_runtime_handle(builder, value);
    let not_constant = builder.ins().icmp_imm(IntCC::Equal, is_constant, 0);
    let not_runtime = builder.ins().icmp_imm(IntCC::Equal, is_runtime, 0);
    let is_int = builder.ins().band(not_constant, not_runtime);
    let has_kind =
        |builder: &mut FunctionBuilder<'_>, tag| lower_value_has_tag(builder, value, tag);
    let matched = match operation {
        0 => is_null,
        1 => is_bool,
        2 => is_int,
        3 => has_kind(builder, crate::JIT_VALUE_RUNTIME_FLOAT_TAG),
        4 => has_kind(builder, crate::JIT_VALUE_RUNTIME_STRING_TAG),
        5 => has_kind(builder, crate::JIT_VALUE_RUNTIME_ARRAY_TAG),
        6 => {
            let object = has_kind(builder, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
            let callable = has_kind(builder, crate::JIT_VALUE_RUNTIME_CALLABLE_TAG);
            let generator = has_kind(builder, crate::JIT_VALUE_RUNTIME_GENERATOR_TAG);
            let fiber = has_kind(builder, crate::JIT_VALUE_RUNTIME_FIBER_TAG);
            let object = builder.ins().bor(object, callable);
            let object = builder.ins().bor(object, generator);
            builder.ins().bor(object, fiber)
        }
        7 => has_kind(builder, crate::JIT_VALUE_RUNTIME_RESOURCE_TAG),
        8 => {
            let float = has_kind(builder, crate::JIT_VALUE_RUNTIME_FLOAT_TAG);
            let string = has_kind(builder, crate::JIT_VALUE_RUNTIME_STRING_TAG);
            let scalar = builder.ins().bor(is_bool, is_int);
            let scalar = builder.ins().bor(scalar, float);
            builder.ins().bor(scalar, string)
        }
        _ => unreachable!("stable predicate operation is compile-time selected"),
    };
    Ok(encode_native_bool(builder, matched))
}

fn lower_optimizing_reference_scalar(
    builder: &mut FunctionBuilder<'_>,
    reference: ir::Value,
    retain_value: bool,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let inspect_reference = builder.create_block();
    let inspect_scalar = builder.create_block();
    let inspect_array = builder.create_block();
    let load_array = builder.create_block();
    let plain = builder.create_block();
    let direct_scalar = builder.create_block();
    let direct_array = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    // A local that is bound by reference anywhere in the function requires
    // reference-capable storage for its complete lifetime, but it can still
    // contain an ordinary PHP value before the binding instruction executes.
    // Only an actual reference-tagged value owns a reference descriptor.
    // Treating every value in such a local as a descriptor made an ordinary
    // array length look like a pointer in optimized WordPress code.
    let is_reference =
        lower_value_has_tag(builder, reference, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    builder
        .ins()
        .brif(is_reference, inspect_reference, &[], plain, &[]);

    builder.switch_to_block(plain);
    if retain_value {
        lower_optimizing_retain(builder, reference, transition.deopt_out);
    }
    builder.ins().jump(merge, &[reference.into()]);

    builder.switch_to_block(inspect_reference);
    let slot = lower_optimizing_slot_address(builder, reference, transition.deopt_out);
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let scalar_descriptor = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let array_descriptor = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
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
    let descriptor_ok = builder.ins().band(kind_ok, flags_ok);
    let scalar_present = builder
        .ins()
        .icmp_imm(IntCC::NotEqual, scalar_descriptor, 0);
    let scalar_ok = builder.ins().band(descriptor_ok, scalar_present);
    builder
        .ins()
        .brif(scalar_ok, inspect_scalar, &[], inspect_array, &[]);

    builder.switch_to_block(inspect_scalar);
    let state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        scalar_descriptor,
        std::mem::offset_of!(crate::JitNativeReferenceScalarView, state) as i32,
    );
    let published = builder.ins().icmp_imm(
        IntCC::Equal,
        state,
        i64::from(crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED),
    );
    builder
        .ins()
        .brif(published, direct_scalar, &[], inspect_array, &[]);

    builder.switch_to_block(inspect_array);
    let array_present = builder.ins().icmp_imm(IntCC::NotEqual, array_descriptor, 0);
    let array_present = builder.ins().band(descriptor_ok, array_present);
    builder
        .ins()
        .brif(array_present, load_array, &[], rejected, &[]);

    builder.switch_to_block(load_array);
    let abi_version = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        array_descriptor,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, abi_version) as i32,
    );
    let array_state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        array_descriptor,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, state) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        array_descriptor,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, length) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        array_descriptor,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, entries) as i32,
    );
    let storage_refcount = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        array_descriptor,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, storage_refcount) as i32,
    );
    let version_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        abi_version,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION),
    );
    let published = builder.ins().icmp_imm(
        IntCC::Equal,
        array_state,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED),
    );
    let storage_ok = builder.ins().icmp_imm(IntCC::NotEqual, storage_refcount, 0);
    let length_ok =
        builder
            .ins()
            .icmp_imm(IntCC::UnsignedLessThanOrEqual, length, i64::from(u32::MAX));
    let array_ok = builder.ins().band(version_ok, published);
    let array_ok = builder.ins().band(array_ok, storage_ok);
    let array_ok = builder.ins().band(array_ok, length_ok);
    builder
        .ins()
        .brif(array_ok, direct_array, &[], rejected, &[]);

    builder.switch_to_block(rejected);
    let placeholder = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[placeholder.into()]);

    builder.switch_to_block(direct_scalar);
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        scalar_descriptor,
        std::mem::offset_of!(crate::JitNativeReferenceScalarView, encoded) as i32,
    );
    if retain_value {
        lower_optimizing_retain(builder, value, transition.deopt_out);
    }
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(direct_array);
    let slot_index = lower_reserve_direct_value_index(builder, transition.deopt_out, rejected);
    if retain_value {
        let strong = builder
            .ins()
            .load(pointer_type, MemFlagsData::new(), storage_refcount, 0);
        let retained = builder.ins().iadd_imm(strong, 1);
        builder
            .ins()
            .store(MemFlagsData::new(), retained, storage_refcount, 0);
    }
    let runtime_view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        runtime_view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let wide_index = builder.ins().uextend(pointer_type, slot_index);
    let slot_offset = builder.ins().ishl_imm(wide_index, 5);
    let slot = builder.ins().iadd(slots, slot_offset);
    let one = builder.ins().iconst(types::I32, 1);
    builder.ins().store(MemFlagsData::new(), one, slot, 0);
    let kind = if retain_value {
        crate::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
    } else {
        crate::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY
    };
    let kind = builder.ins().iconst(types::I32, i64::from(kind));
    builder.ins().store(
        MemFlagsData::new(),
        kind,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_SHARED_ARRAY_ABI_VERSION),
    );
    builder.ins().store(
        MemFlagsData::new(),
        flags,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let length = builder.ins().ireduce(types::I32, length);
    builder.ins().store(
        MemFlagsData::new(),
        length,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        storage_refcount,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        entries,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let encoded_index = builder.ins().iadd_imm(
        slot_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let encoded_index = builder.ins().uextend(types::I64, encoded_index);
    let value = builder
        .ins()
        .bor_imm(encoded_index, crate::JIT_VALUE_RUNTIME_ARRAY_TAG as i64);
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_optimizing_truthy(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let inspect_runtime = builder.create_block();
    let inspect_non_runtime = builder.create_block();
    let inspect_descriptor = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I8);

    let is_true = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
    );
    let is_false = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    let is_null = builder
        .ins()
        .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX));
    let is_uninitialized = builder.ins().icmp_imm(
        IntCC::Equal,
        value,
        crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
    );
    let false_lane = builder.ins().bor(is_false, is_null);
    let false_lane = builder.ins().bor(false_lane, is_uninitialized);
    let reserved = builder.ins().bor(is_true, false_lane);
    let runtime = lower_is_runtime_handle(builder, value);
    builder
        .ins()
        .brif(runtime, inspect_runtime, &[], inspect_non_runtime, &[]);

    builder.switch_to_block(inspect_non_runtime);
    let constant = lower_value_has_namespace_tag(builder, value, crate::JIT_VALUE_CONSTANT_TAG);
    let not_reserved = builder.ins().icmp_imm(IntCC::Equal, reserved, 0);
    let opaque_constant = builder.ins().band(constant, not_reserved);
    let integer_truthy = builder.ins().icmp_imm(IntCC::NotEqual, value, 0);
    let direct_truthy = builder.ins().select(reserved, is_true, integer_truthy);
    builder.ins().brif(
        opaque_constant,
        rejected,
        &[],
        merge,
        &[direct_truthy.into()],
    );

    builder.switch_to_block(inspect_runtime);
    let runtime_kind = builder
        .ins()
        .band_imm(value, crate::JIT_VALUE_RUNTIME_KIND_MASK as i64);
    let is_array = builder.ins().icmp_imm(
        IntCC::Equal,
        runtime_kind,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG as i64,
    );
    let is_string = builder.ins().icmp_imm(
        IntCC::Equal,
        runtime_kind,
        crate::JIT_VALUE_RUNTIME_STRING_TAG as i64,
    );
    let descriptor = builder.ins().bor(is_array, is_string);
    builder
        .ins()
        .brif(descriptor, inspect_descriptor, &[], rejected, &[]);

    builder.switch_to_block(inspect_descriptor);
    let slot = lower_optimizing_slot_address(builder, value, transition.deopt_out);
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let non_empty = builder.ins().icmp_imm(IntCC::NotEqual, length, 0);
    let zero_string = builder.ins().icmp_imm(
        IntCC::Equal,
        flags,
        i64::from(crate::JIT_NATIVE_STRING_VALUE_ZERO),
    );
    let not_zero_string = builder.ins().icmp_imm(IntCC::Equal, zero_string, 0);
    let string_truthy = builder.ins().band(non_empty, not_zero_string);
    let result = builder.ins().select(is_string, string_truthy, non_empty);
    builder.ins().jump(merge, &[result.into()]);

    builder.switch_to_block(rejected);
    let _ = transition.emit_value(builder)?;
    let unreachable_false = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[unreachable_false.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_optimizing_value_slot(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    expected_tag: u64,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let accepted = builder.create_block();
    let rejected = builder.create_block();
    let matches = lower_value_has_tag(builder, value, expected_tag);
    builder.ins().brif(matches, accepted, &[], rejected, &[]);

    builder.switch_to_block(rejected);
    let _ = transition.emit_value(builder)?;
    builder.ins().jump(accepted, &[]);

    builder.switch_to_block(accepted);
    Ok(lower_optimizing_slot_address(
        builder,
        value,
        transition.deopt_out,
    ))
}

fn lower_optimizing_length(
    builder: &mut FunctionBuilder<'_>,
    operation: u32,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let tag = if operation == 0 {
        crate::JIT_VALUE_RUNTIME_STRING_TAG
    } else {
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG
    };
    let slot = lower_optimizing_value_slot(builder, value, tag, transition)?;
    Ok(builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    ))
}

fn lower_optimizing_concat(
    builder: &mut FunctionBuilder<'_>,
    lhs: ir::Value,
    rhs: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let (lhs_valid, lhs_len, lhs_bytes) =
        lower_native_string_key_descriptor(builder, lhs, transition.deopt_out);
    let (rhs_valid, rhs_len, rhs_bytes) =
        lower_native_string_key_descriptor(builder, rhs, transition.deopt_out);
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let byte_next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_next) as i32,
    );
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let byte_arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_bytes) as i32,
    );
    let byte_next = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), byte_next_ptr, 0);
    let (length, length_overflow) = builder.ins().uadd_overflow(lhs_len, rhs_len);
    let length32 = builder.ins().ireduce(types::I32, length);
    let length_round_trip = builder.ins().uextend(types::I64, length32);
    let length_fits = builder.ins().icmp(IntCC::Equal, length, length_round_trip);
    let byte_end = builder.ins().iadd(byte_next, length32);
    let byte_room = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        byte_end,
        crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY as i64,
    );
    let no_overflow = builder.ins().icmp_imm(IntCC::Equal, length_overflow, 0);
    let strings_valid = builder.ins().band(lhs_valid, rhs_valid);
    let admitted = builder.ins().band(strings_valid, no_overflow);
    let admitted = builder.ins().band(admitted, length_fits);
    let admitted = builder.ins().band(admitted, byte_room);
    let allocate = builder.create_block();
    let rejected = builder.create_block();
    let copy_lhs = builder.create_block();
    let copy_lhs_byte = builder.create_block();
    let copy_rhs = builder.create_block();
    let copy_rhs_byte = builder.create_block();
    let finish = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(copy_lhs, types::I64);
    builder.append_block_param(copy_rhs, types::I64);
    builder.append_block_param(merge, types::I64);
    builder.ins().brif(admitted, allocate, &[], rejected, &[]);

    builder.switch_to_block(rejected);
    let placeholder = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[placeholder.into()]);

    builder.switch_to_block(allocate);
    let slot_next = lower_reserve_direct_value_index(builder, transition.deopt_out, rejected);
    builder
        .ins()
        .store(MemFlagsData::new(), byte_end, byte_next_ptr, 0);
    let byte_offset = builder.ins().uextend(pointer_type, byte_next);
    let output = builder.ins().iadd(byte_arena, byte_offset);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(copy_lhs, &[zero.into()]);

    builder.switch_to_block(copy_lhs);
    let index = builder.block_params(copy_lhs)[0];
    let lhs_done = builder.ins().icmp(IntCC::Equal, index, lhs_len);
    builder
        .ins()
        .brif(lhs_done, copy_rhs, &[zero.into()], copy_lhs_byte, &[]);

    builder.switch_to_block(copy_lhs_byte);
    let source = builder.ins().iadd(lhs_bytes, index);
    let destination = builder.ins().iadd(output, index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(copy_lhs, &[next.into()]);

    builder.switch_to_block(copy_rhs);
    let index = builder.block_params(copy_rhs)[0];
    let rhs_done = builder.ins().icmp(IntCC::Equal, index, rhs_len);
    builder
        .ins()
        .brif(rhs_done, finish, &[], copy_rhs_byte, &[]);

    builder.switch_to_block(copy_rhs_byte);
    let source = builder.ins().iadd(rhs_bytes, index);
    let destination_index = builder.ins().iadd(lhs_len, index);
    let destination = builder.ins().iadd(output, destination_index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(copy_rhs, &[next.into()]);

    builder.switch_to_block(finish);
    let slot_index = builder.ins().uextend(pointer_type, slot_next);
    let slot_offset = builder.ins().ishl_imm(slot_index, 5);
    let slot = builder.ins().iadd(slots, slot_offset);
    let one32 = builder.ins().iconst(types::I32, 1);
    builder.ins().store(MemFlagsData::new(), one32, slot, 0);
    let kind = builder
        .ins()
        .iconst(types::I32, i64::from(crate::JIT_NATIVE_VALUE_VIEW_STRING));
    builder.ins().store(
        MemFlagsData::new(),
        kind,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION),
    );
    builder.ins().store(
        MemFlagsData::new(),
        flags,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let inspect_zero_string = builder.create_block();
    let store_descriptor = builder.create_block();
    builder.append_block_param(store_descriptor, types::I32);
    let length_is_one = builder.ins().icmp_imm(IntCC::Equal, length, 1);
    let no_zero_flag = builder.ins().iconst(types::I32, 0);
    builder.ins().brif(
        length_is_one,
        inspect_zero_string,
        &[],
        store_descriptor,
        &[no_zero_flag.into()],
    );

    builder.switch_to_block(inspect_zero_string);
    let first = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), output, 0);
    let first_is_zero = builder.ins().icmp_imm(IntCC::Equal, first, b'0' as i64);
    let yes_zero_flag = builder
        .ins()
        .iconst(types::I32, i64::from(crate::JIT_NATIVE_STRING_VALUE_ZERO));
    let zero_flag = builder
        .ins()
        .select(first_is_zero, yes_zero_flag, no_zero_flag);
    builder.ins().jump(store_descriptor, &[zero_flag.into()]);

    builder.switch_to_block(store_descriptor);
    let zero_flag = builder.block_params(store_descriptor)[0];
    builder.ins().store(
        MemFlagsData::new(),
        zero_flag,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        length,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        output,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let encoded_index = builder.ins().iadd_imm(
        slot_next,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let encoded_index = builder.ins().uextend(types::I64, encoded_index);
    let encoded = builder
        .ins()
        .bor_imm(encoded_index, crate::JIT_VALUE_RUNTIME_STRING_TAG as i64);
    builder.ins().jump(merge, &[encoded.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_optimizing_string_predicate(
    builder: &mut FunctionBuilder<'_>,
    operation: u32,
    haystack: ir::Value,
    needle: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let haystack_slot = lower_optimizing_value_slot(
        builder,
        haystack,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
        transition,
    )?;
    let needle_slot = lower_optimizing_value_slot(
        builder,
        needle,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
        transition,
    )?;
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let haystack_len = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        haystack_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let needle_len = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        needle_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let haystack_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        haystack_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let needle_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        needle_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );

    // Starts/ends-with are bytewise native loops. `contains` uses the same
    // bounded inner comparison at every candidate offset.
    let outer = builder.create_block();
    let compare = builder.create_block();
    let next_outer = builder.create_block();
    let matched = builder.create_block();
    let failed = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(outer, types::I64);
    builder.append_block_param(compare, types::I64);
    builder.append_block_param(compare, types::I64);
    builder.append_block_param(next_outer, types::I64);
    builder.append_block_param(merge, types::I8);

    let fits = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, needle_len, haystack_len);
    let empty = builder.ins().icmp_imm(IntCC::Equal, needle_len, 0);
    let initial = match operation {
        0 | 1 => builder.ins().iconst(types::I64, 0),
        2 => builder.ins().isub(haystack_len, needle_len),
        _ => unreachable!("stable string predicate operation is compile-time selected"),
    };
    let start = if operation == 2 {
        initial
    } else {
        builder.ins().iconst(types::I64, 0)
    };
    let non_empty = builder.ins().icmp_imm(IntCC::Equal, empty, 0);
    let can_compare = builder.ins().band(fits, non_empty);
    builder
        .ins()
        .brif(empty, matched, &[], outer, &[start.into()]);

    builder.switch_to_block(outer);
    let offset = builder.block_params(outer)[0];
    if operation == 0 {
        builder.ins().brif(
            can_compare,
            compare,
            &[offset.into(), initial.into()],
            failed,
            &[],
        );
    } else {
        let last = builder.ins().isub(haystack_len, needle_len);
        let in_range = builder
            .ins()
            .icmp(IntCC::UnsignedLessThanOrEqual, offset, last);
        let valid = builder.ins().band(fits, in_range);
        builder.ins().brif(
            valid,
            compare,
            &[offset.into(), initial.into()],
            failed,
            &[],
        );
    }

    builder.switch_to_block(compare);
    let offset = builder.block_params(compare)[0];
    let index = builder.block_params(compare)[1];
    let done = builder.ins().icmp(IntCC::Equal, index, needle_len);
    let compare_bytes = builder.create_block();
    builder.ins().brif(done, matched, &[], compare_bytes, &[]);

    builder.switch_to_block(compare_bytes);
    let haystack_index = builder.ins().iadd(offset, index);
    let haystack_at = builder.ins().iadd(haystack_ptr, haystack_index);
    let needle_at = builder.ins().iadd(needle_ptr, index);
    let left = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), haystack_at, 0);
    let right = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), needle_at, 0);
    let equal = builder.ins().icmp(IntCC::Equal, left, right);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().brif(
        equal,
        compare,
        &[offset.into(), next.into()],
        next_outer,
        &[offset.into()],
    );

    builder.switch_to_block(next_outer);
    let offset = builder.block_params(next_outer)[0];
    if operation == 0 || operation == 2 {
        builder.ins().jump(failed, &[]);
    } else {
        let next = builder.ins().iadd_imm(offset, 1);
        builder.ins().jump(outer, &[next.into()]);
    }

    builder.switch_to_block(matched);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[yes.into()]);
    builder.switch_to_block(failed);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[no.into()]);
    builder.switch_to_block(merge);
    Ok(encode_native_bool(builder, builder.block_params(merge)[0]))
}

fn lower_optimizing_ascii_case(
    builder: &mut FunctionBuilder<'_>,
    operation: u32,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let input_slot = lower_optimizing_value_slot(
        builder,
        value,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
        transition,
    )?;
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        input_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let input = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        input_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let byte_next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_next) as i32,
    );
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let byte_arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_bytes) as i32,
    );
    let byte_next = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), byte_next_ptr, 0);
    let length32 = builder.ins().ireduce(types::I32, length);
    let length_round_trip = builder.ins().uextend(types::I64, length32);
    let length_fits = builder.ins().icmp(IntCC::Equal, length, length_round_trip);
    let byte_end = builder.ins().iadd(byte_next, length32);
    let byte_room = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        byte_end,
        crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY as i64,
    );
    let admitted = builder.ins().band(length_fits, byte_room);
    let allocate = builder.create_block();
    let rejected = builder.create_block();
    let copy = builder.create_block();
    let copy_byte = builder.create_block();
    let finish = builder.create_block();
    let inspect_zero_string = builder.create_block();
    let store_descriptor = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(store_descriptor, types::I32);
    builder.append_block_param(merge, types::I64);
    builder.ins().brif(admitted, allocate, &[], rejected, &[]);

    builder.switch_to_block(rejected);
    let placeholder = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[placeholder.into()]);

    builder.switch_to_block(allocate);
    let slot_next = lower_reserve_direct_value_index(builder, transition.deopt_out, rejected);
    builder
        .ins()
        .store(MemFlagsData::new(), byte_end, byte_next_ptr, 0);
    let byte_offset = builder.ins().uextend(pointer_type, byte_next);
    let output = builder.ins().iadd(byte_arena, byte_offset);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(copy, &[zero.into()]);

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let done = builder.ins().icmp(IntCC::Equal, index, length);
    builder.ins().brif(done, finish, &[], copy_byte, &[]);

    builder.switch_to_block(copy_byte);
    let source = builder.ins().iadd(input, index);
    let destination = builder.ins().iadd(output, index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    let lower_bound = if operation == 0 { b'A' } else { b'a' };
    let upper_bound = if operation == 0 { b'Z' } else { b'z' };
    let at_least_lower = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        byte,
        i64::from(lower_bound),
    );
    let at_most_upper =
        builder
            .ins()
            .icmp_imm(IntCC::UnsignedLessThanOrEqual, byte, i64::from(upper_bound));
    let ascii_letter = builder.ins().band(at_least_lower, at_most_upper);
    let converted = if operation == 0 {
        builder.ins().iadd_imm(byte, 32)
    } else {
        builder.ins().iadd_imm(byte, -32)
    };
    let converted = builder.ins().select(ascii_letter, converted, byte);
    builder
        .ins()
        .store(MemFlagsData::new(), converted, destination, 0);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(copy, &[next.into()]);

    builder.switch_to_block(finish);
    let slot_index = builder.ins().uextend(pointer_type, slot_next);
    let slot_offset = builder.ins().ishl_imm(slot_index, 5);
    let slot = builder.ins().iadd(slots, slot_offset);
    let one32 = builder.ins().iconst(types::I32, 1);
    builder.ins().store(MemFlagsData::new(), one32, slot, 0);
    let kind = builder
        .ins()
        .iconst(types::I32, i64::from(crate::JIT_NATIVE_VALUE_VIEW_STRING));
    builder.ins().store(
        MemFlagsData::new(),
        kind,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION),
    );
    builder.ins().store(
        MemFlagsData::new(),
        flags,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let length_is_one = builder.ins().icmp_imm(IntCC::Equal, length, 1);
    let no_zero_flag = builder.ins().iconst(types::I32, 0);
    builder.ins().brif(
        length_is_one,
        inspect_zero_string,
        &[],
        store_descriptor,
        &[no_zero_flag.into()],
    );

    builder.switch_to_block(inspect_zero_string);
    let first = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), output, 0);
    let first_is_zero = builder.ins().icmp_imm(IntCC::Equal, first, b'0' as i64);
    let yes_zero_flag = builder
        .ins()
        .iconst(types::I32, i64::from(crate::JIT_NATIVE_STRING_VALUE_ZERO));
    let zero_flag = builder
        .ins()
        .select(first_is_zero, yes_zero_flag, no_zero_flag);
    builder.ins().jump(store_descriptor, &[zero_flag.into()]);

    builder.switch_to_block(store_descriptor);
    let zero_flag = builder.block_params(store_descriptor)[0];
    builder.ins().store(
        MemFlagsData::new(),
        zero_flag,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        length,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        output,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let encoded_index = builder.ins().iadd_imm(
        slot_next,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let encoded_index = builder.ins().uextend(types::I64, encoded_index);
    let encoded = builder
        .ins()
        .bor_imm(encoded_index, crate::JIT_VALUE_RUNTIME_STRING_TAG as i64);
    builder.ins().jump(merge, &[encoded.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_optimizing_property_fetch(
    builder: &mut FunctionBuilder<'_>,
    object: ir::Value,
    property: &str,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let inspect = builder.create_block();
    let inspect_entry = builder.create_block();
    let hit = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let is_object = lower_value_has_tag(builder, object, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
    builder.ins().brif(is_object, inspect, &[], rejected, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, object, transition.deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let epoch_pointer = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let kind_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_OBJECT_PROPERTIES),
    );
    let entries_ok = builder.ins().icmp_imm(IntCC::NotEqual, entries, 0);
    let epoch_pointer_ok = builder.ins().icmp_imm(IntCC::NotEqual, epoch_pointer, 0);
    let admitted = builder.ins().band(kind_ok, entries_ok);
    let admitted = builder.ins().band(admitted, epoch_pointer_ok);
    builder
        .ins()
        .brif(admitted, inspect_entry, &[], rejected, &[]);

    builder.switch_to_block(inspect_entry);
    let name_hash = crate::jit_native_property_name_hash(property);
    let cache_index = name_hash as usize & (crate::JIT_NATIVE_OBJECT_PROPERTY_CACHE_SIZE - 1);
    let entry = builder.ins().iadd_imm(
        entries,
        i64::try_from(
            cache_index.saturating_mul(std::mem::size_of::<crate::JitNativePropertyCacheEntry>()),
        )
        .unwrap_or(i64::MAX),
    );
    let cached_hash = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativePropertyCacheEntry, name_hash) as i32,
    );
    let cached_epoch = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativePropertyCacheEntry, property_epoch) as i32,
    );
    let live_epoch = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), epoch_pointer, 0);
    let matches = builder
        .ins()
        .icmp_imm(IntCC::Equal, cached_hash, name_hash as i64);
    let epoch_matches = builder.ins().icmp(IntCC::Equal, cached_epoch, live_epoch);
    let matches = builder.ins().band(matches, epoch_matches);
    builder.ins().brif(matches, hit, &[], rejected, &[]);

    builder.switch_to_block(hit);
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativePropertyCacheEntry, value) as i32,
    );
    lower_optimizing_retain(builder, value, transition.deopt_out);
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(rejected);
    let placeholder = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[placeholder.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn define_optimizing_call_result(
    builder: &mut FunctionBuilder<'_>,
    register_variables: &NativeRegisterMap,
    registers: &mut NativeRegisterMap,
    result: RegionCallResult,
    value: ir::Value,
) -> Result<(), CraneliftLoweringError> {
    match result {
        RegionCallResult::Register(destination) => {
            define_region_register(builder, register_variables, registers, destination, value)
        }
        RegionCallResult::Discard => Ok(()),
        RegionCallResult::ReferenceLocal(_) => {
            unreachable!("optimizer admission rejected reference result")
        }
    }
}

fn lower_reference_array_entry_key_equal(
    builder: &mut FunctionBuilder<'_>,
    entry: ir::Value,
    key: ir::Value,
    deopt_out: ir::Value,
) -> ir::Value {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let entry_kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, kind) as i32,
    );
    let integer = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, integer) as i32,
    );
    let key_runtime = lower_is_runtime_handle(builder, key);
    let key_constant = lower_value_has_namespace_tag(builder, key, crate::JIT_VALUE_CONSTANT_TAG);
    let namespaced = builder.ins().bor(key_runtime, key_constant);
    let immediate = builder.ins().icmp_imm(IntCC::Equal, namespaced, 0);
    let int_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        entry_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_KEY_INT),
    );
    let integer_equal = builder.ins().icmp(IntCC::Equal, integer, key);
    let integer_equal = builder.ins().band(int_kind, integer_equal);
    let integer_equal = builder.ins().band(immediate, integer_equal);
    let (string_valid, key_length, key_bytes) =
        lower_native_string_key_descriptor(builder, key, deopt_out);
    let string_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        entry_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_KEY_STRING),
    );
    let entry_length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, string_length) as i32,
    );
    let entry_bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, string_bytes) as i32,
    );
    let same_length = builder.ins().icmp(IntCC::Equal, entry_length, key_length);
    let string_admitted = builder.ins().band(string_valid, string_kind);
    let string_admitted = builder.ins().band(string_admitted, same_length);
    let string_gate = builder.create_block();
    let string_compare = builder.create_block();
    let string_byte = builder.create_block();
    let string_next = builder.create_block();
    let matched = builder.create_block();
    let different = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(string_compare, types::I64);
    builder.append_block_param(string_byte, types::I64);
    builder.append_block_param(string_next, types::I64);
    builder.append_block_param(merge, types::I8);
    builder
        .ins()
        .brif(integer_equal, matched, &[], string_gate, &[]);

    builder.switch_to_block(string_gate);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().brif(
        string_admitted,
        string_compare,
        &[zero.into()],
        different,
        &[],
    );

    builder.switch_to_block(string_compare);
    let index = builder.block_params(string_compare)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, key_length);
    builder
        .ins()
        .brif(exhausted, matched, &[], string_byte, &[index.into()]);

    builder.switch_to_block(string_byte);
    let index = builder.block_params(string_byte)[0];
    let offset = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let lhs = builder.ins().iadd(entry_bytes, offset);
    let rhs = builder.ins().iadd(key_bytes, offset);
    let lhs = builder.ins().load(types::I8, MemFlagsData::new(), lhs, 0);
    let rhs = builder.ins().load(types::I8, MemFlagsData::new(), rhs, 0);
    let equal = builder.ins().icmp(IntCC::Equal, lhs, rhs);
    builder
        .ins()
        .brif(equal, string_next, &[index.into()], different, &[]);

    builder.switch_to_block(string_next);
    let index = builder.block_params(string_next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(string_compare, &[index.into()]);

    builder.switch_to_block(matched);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[yes.into()]);
    builder.switch_to_block(different);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[no.into()]);
    builder.switch_to_block(merge);
    builder.block_params(merge)[0]
}

fn lower_copy_native_string_view(
    builder: &mut FunctionBuilder<'_>,
    input: ir::Value,
    length: ir::Value,
    zero_flag: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let view = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let byte_next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_next) as i32,
    );
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        view + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_bytes) as i32,
    );
    let byte_next = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), byte_next_ptr, 0);
    let length32 = builder.ins().ireduce(types::I32, length);
    let round_trip = builder.ins().uextend(types::I64, length32);
    let length_fits = builder.ins().icmp(IntCC::Equal, length, round_trip);
    let byte_end = builder.ins().iadd(byte_next, length32);
    let room = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        byte_end,
        crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY as i64,
    );
    let admitted = builder.ins().band(length_fits, room);
    let allocate = builder.create_block();
    let rejected = builder.create_block();
    let copy = builder.create_block();
    let copy_byte = builder.create_block();
    let finish = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(merge, types::I64);
    builder.ins().brif(admitted, allocate, &[], rejected, &[]);

    builder.switch_to_block(rejected);
    let placeholder = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[placeholder.into()]);

    builder.switch_to_block(allocate);
    let slot_index = lower_reserve_direct_value_index(builder, transition.deopt_out, rejected);
    builder
        .ins()
        .store(MemFlagsData::new(), byte_end, byte_next_ptr, 0);
    let offset = builder.ins().uextend(pointer_type, byte_next);
    let output = builder.ins().iadd(bytes, offset);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(copy, &[zero.into()]);

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let done = builder.ins().icmp(IntCC::Equal, index, length);
    builder.ins().brif(done, finish, &[], copy_byte, &[]);

    builder.switch_to_block(copy_byte);
    let source = builder.ins().iadd(input, index);
    let destination = builder.ins().iadd(output, index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(copy, &[next.into()]);

    builder.switch_to_block(finish);
    let wide_index = builder.ins().uextend(pointer_type, slot_index);
    let slot_offset = builder.ins().ishl_imm(wide_index, 5);
    let slot = builder.ins().iadd(slots, slot_offset);
    let one = builder.ins().iconst(types::I32, 1);
    builder.ins().store(MemFlagsData::new(), one, slot, 0);
    let kind = builder
        .ins()
        .iconst(types::I32, i64::from(crate::JIT_NATIVE_VALUE_VIEW_STRING));
    builder.ins().store(
        MemFlagsData::new(),
        kind,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION),
    );
    builder.ins().store(
        MemFlagsData::new(),
        flags,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        zero_flag,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        length,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        output,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let encoded_index = builder.ins().iadd_imm(
        slot_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let encoded_index = builder.ins().uextend(types::I64, encoded_index);
    let encoded = builder
        .ins()
        .bor_imm(encoded_index, crate::JIT_VALUE_RUNTIME_STRING_TAG as i64);
    builder.ins().jump(merge, &[encoded.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_shared_array_fetch(
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    key: ir::Value,
    operation: u32,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let slot = lower_optimizing_slot_address(builder, array, transition.deopt_out);
    let length32 = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let length = builder.ins().uextend(types::I64, length32);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let search = builder.create_block();
    let compare = builder.create_block();
    let next = builder.create_block();
    let found = builder.create_block();
    let missing = builder.create_block();
    let scalar = builder.create_block();
    let string = builder.create_block();
    let unsupported = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(search, types::I64);
    builder.append_block_param(next, types::I64);
    builder.append_block_param(found, pointer_type);
    builder.append_block_param(scalar, pointer_type);
    builder.append_block_param(string, pointer_type);
    builder.append_block_param(merge, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(search, &[zero.into()]);

    builder.switch_to_block(search);
    let index = builder.block_params(search)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(exhausted, missing, &[], compare, &[]);

    builder.switch_to_block(compare);
    let wide_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let entry_offset = builder.ins().ishl_imm(wide_index, 6);
    let entry = builder.ins().iadd(entries, entry_offset);
    let matches = lower_reference_array_entry_key_equal(builder, entry, key, transition.deopt_out);
    builder
        .ins()
        .brif(matches, found, &[entry.into()], next, &[index.into()]);

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(search, &[index.into()]);

    builder.switch_to_block(found);
    let entry = builder.block_params(found)[0];
    if operation == 2 {
        let value = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        );
        builder.ins().jump(merge, &[value.into()]);
    } else if operation == 3 {
        let non_null = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            entry,
            std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, non_null) as i32,
        );
        let non_null = builder.ins().ireduce(types::I8, non_null);
        let value = encode_native_bool(builder, non_null);
        builder.ins().jump(merge, &[value.into()]);
    } else {
        let value_kind = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            entry,
            std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_kind) as i32,
        );
        let string_kind = builder.ins().icmp_imm(
            IntCC::Equal,
            value_kind,
            i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_STRING),
        );
        let scalar_kind = builder.ins().icmp_imm(
            IntCC::UnsignedGreaterThanOrEqual,
            value_kind,
            i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_NULL),
        );
        let scalar_upper = builder.ins().icmp_imm(
            IntCC::UnsignedLessThanOrEqual,
            value_kind,
            i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_INT),
        );
        let scalar_kind = builder.ins().band(scalar_kind, scalar_upper);
        let inspect_scalar = builder.create_block();
        builder.append_block_param(inspect_scalar, pointer_type);
        builder.ins().brif(
            string_kind,
            string,
            &[entry.into()],
            inspect_scalar,
            &[entry.into()],
        );
        builder.switch_to_block(inspect_scalar);
        let entry = builder.block_params(inspect_scalar)[0];
        builder
            .ins()
            .brif(scalar_kind, scalar, &[entry.into()], unsupported, &[]);
    }

    builder.switch_to_block(scalar);
    let entry = builder.block_params(scalar)[0];
    let value_kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_kind) as i32,
    );
    let payload = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_payload) as i32,
    );
    let is_null = builder.ins().icmp_imm(
        IntCC::Equal,
        value_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_NULL),
    );
    let is_uninitialized = builder.ins().icmp_imm(
        IntCC::Equal,
        value_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_UNINITIALIZED),
    );
    let is_false = builder.ins().icmp_imm(
        IntCC::Equal,
        value_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_FALSE),
    );
    let is_true = builder.ins().icmp_imm(
        IntCC::Equal,
        value_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_TRUE),
    );
    let null = builder
        .ins()
        .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
    let uninitialized = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
    );
    let false_value = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    let true_value = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
    );
    let value = builder.ins().select(is_true, true_value, payload);
    let value = builder.ins().select(is_false, false_value, value);
    let value = builder.ins().select(is_uninitialized, uninitialized, value);
    let value = builder.ins().select(is_null, null, value);
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(string);
    let entry = builder.block_params(string)[0];
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_flags) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_length) as i32,
    );
    let bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_bytes) as i32,
    );
    let value = lower_copy_native_string_view(builder, bytes, length, flags, transition)?;
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(unsupported);
    let value = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(missing);
    if operation == 2 || operation == 3 {
        let value = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
        );
        builder.ins().jump(merge, &[value.into()]);
    } else if operation & 1 == 1 {
        let value = builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
        builder.ins().jump(merge, &[value.into()]);
    } else {
        let value = transition.emit_value(builder)?;
        builder.ins().jump(merge, &[value.into()]);
    }

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_reference_array_assign(
    builder: &mut FunctionBuilder<'_>,
    reference: ir::Value,
    key: ir::Value,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let inspect_view = builder.create_block();
    let search = builder.create_block();
    let compare = builder.create_block();
    let next = builder.create_block();
    let found = builder.create_block();
    let write = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    builder.append_block_param(search, types::I64);
    builder.append_block_param(next, types::I64);
    builder.append_block_param(found, pointer_type);
    builder.append_block_param(merge, types::I64);

    let slot = lower_optimizing_slot_address(builder, reference, transition.deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let view = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
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
    let view_ok = builder.ins().icmp_imm(IntCC::NotEqual, view, 0);
    let value_runtime = lower_is_runtime_handle(builder, value);
    let value_constant =
        lower_value_has_namespace_tag(builder, value, crate::JIT_VALUE_CONSTANT_TAG);
    let value_namespaced = builder.ins().bor(value_runtime, value_constant);
    let integer_value = builder.ins().icmp_imm(IntCC::Equal, value_namespaced, 0);
    let admitted = builder.ins().band(kind_ok, flags_ok);
    let admitted = builder.ins().band(admitted, view_ok);
    let admitted = builder.ins().band(admitted, integer_value);
    builder
        .ins()
        .brif(admitted, inspect_view, &[], rejected, &[]);

    builder.switch_to_block(inspect_view);
    let abi_version = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, abi_version) as i32,
    );
    let state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, state) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, length) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, entries) as i32,
    );
    let storage_refcount = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, storage_refcount) as i32,
    );
    let version_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        abi_version,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION),
    );
    let published = builder.ins().icmp_imm(
        IntCC::Equal,
        state,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED),
    );
    let admitted = builder.ins().band(version_ok, published);
    let storage_present = builder.ins().icmp_imm(IntCC::NotEqual, storage_refcount, 0);
    let strong = builder
        .ins()
        .load(pointer_type, MemFlagsData::new(), storage_refcount, 0);
    let unique = builder.ins().icmp_imm(IntCC::Equal, strong, 1);
    let admitted = builder.ins().band(admitted, storage_present);
    let admitted = builder.ins().band(admitted, unique);
    let zero = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .brif(admitted, search, &[zero.into()], rejected, &[]);

    builder.switch_to_block(search);
    let index = builder.block_params(search)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(exhausted, rejected, &[], compare, &[]);

    builder.switch_to_block(compare);
    let entry_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let entry_offset = builder.ins().ishl_imm(entry_index, 6);
    let entry = builder.ins().iadd(entries, entry_offset);
    let matches = lower_reference_array_entry_key_equal(builder, entry, key, transition.deopt_out);
    builder
        .ins()
        .brif(matches, found, &[entry.into()], next, &[index.into()]);

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(search, &[index.into()]);

    builder.switch_to_block(found);
    let entry = builder.block_params(found)[0];
    let old_kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_kind) as i32,
    );
    let supported_lower = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        old_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_NULL),
    );
    let supported_upper = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        old_kind,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_STRING),
    );
    let supported = builder.ins().band(supported_lower, supported_upper);
    builder.ins().brif(supported, write, &[], rejected, &[]);

    builder.switch_to_block(write);
    let value_kind = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_INT),
    );
    let zero32 = builder.ins().iconst(types::I32, 0);
    let zero64 = builder.ins().iconst(types::I64, 0);
    let one32 = builder.ins().iconst(types::I32, 1);
    builder.ins().store(
        MemFlagsData::new(),
        one32,
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, non_null) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        value_kind,
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_kind) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        zero32,
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_flags) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        value,
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_payload) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        zero64,
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_length) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        zero64,
        entry,
        std::mem::offset_of!(crate::JitNativeReferenceArrayEntry, value_bytes) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        one32,
        view,
        std::mem::offset_of!(crate::JitNativeReferenceArrayView, dirty) as i32,
    );
    builder.ins().jump(merge, &[reference.into()]);

    builder.switch_to_block(rejected);
    let result = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[result.into()]);
    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_guarded_integer_binary(
    builder: &mut FunctionBuilder<'_>,
    operation: RegionBinaryOp,
    lhs: ir::Value,
    rhs: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let calculate = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);
    let lhs_runtime = lower_is_runtime_handle(builder, lhs);
    let lhs_constant = lower_value_has_namespace_tag(builder, lhs, crate::JIT_VALUE_CONSTANT_TAG);
    let rhs_runtime = lower_is_runtime_handle(builder, rhs);
    let rhs_constant = lower_value_has_namespace_tag(builder, rhs, crate::JIT_VALUE_CONSTANT_TAG);
    let lhs_namespaced = builder.ins().bor(lhs_runtime, lhs_constant);
    let rhs_namespaced = builder.ins().bor(rhs_runtime, rhs_constant);
    let namespaced = builder.ins().bor(lhs_namespaced, rhs_namespaced);
    let raw_integers = builder.ins().icmp_imm(IntCC::Equal, namespaced, 0);
    builder
        .ins()
        .brif(raw_integers, calculate, &[], rejected, &[]);

    builder.switch_to_block(calculate);
    match operation {
        RegionBinaryOp::BitAnd | RegionBinaryOp::BitOr | RegionBinaryOp::BitXor => {
            let value = match operation {
                RegionBinaryOp::BitAnd => builder.ins().band(lhs, rhs),
                RegionBinaryOp::BitOr => builder.ins().bor(lhs, rhs),
                RegionBinaryOp::BitXor => builder.ins().bxor(lhs, rhs),
                _ => unreachable!(),
            };
            builder.ins().jump(merge, &[value.into()]);
        }
        RegionBinaryOp::Add | RegionBinaryOp::Sub | RegionBinaryOp::Mul => {
            let (value, overflow) = match operation {
                RegionBinaryOp::Add => builder.ins().sadd_overflow(lhs, rhs),
                RegionBinaryOp::Sub => builder.ins().ssub_overflow(lhs, rhs),
                RegionBinaryOp::Mul => builder.ins().smul_overflow(lhs, rhs),
                _ => unreachable!(),
            };
            let accepted = builder.create_block();
            builder.ins().brif(overflow, rejected, &[], accepted, &[]);
            builder.switch_to_block(accepted);
            builder.ins().jump(merge, &[value.into()]);
        }
        RegionBinaryOp::ShiftLeft | RegionBinaryOp::ShiftRight => {
            let shifted = builder.create_block();
            let negative = builder.ins().icmp_imm(IntCC::SignedLessThan, rhs, 0);
            builder.ins().brif(negative, rejected, &[], shifted, &[]);
            builder.switch_to_block(shifted);
            let large = builder
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, rhs, 64);
            let value = if operation == RegionBinaryOp::ShiftLeft {
                builder.ins().ishl(lhs, rhs)
            } else {
                builder.ins().sshr(lhs, rhs)
            };
            let out_of_range = if operation == RegionBinaryOp::ShiftLeft {
                builder.ins().iconst(types::I64, 0)
            } else {
                builder.ins().sshr_imm(lhs, 63)
            };
            let value = builder.ins().select(large, out_of_range, value);
            builder.ins().jump(merge, &[value.into()]);
        }
        RegionBinaryOp::Mod => {
            let inspect_overflow = builder.create_block();
            let overflow = builder.create_block();
            let remainder = builder.create_block();
            let zero = builder.ins().icmp_imm(IntCC::Equal, rhs, 0);
            builder
                .ins()
                .brif(zero, rejected, &[], inspect_overflow, &[]);
            builder.switch_to_block(inspect_overflow);
            let minimum = builder.ins().icmp_imm(IntCC::Equal, lhs, i64::MIN);
            let negative_one = builder.ins().icmp_imm(IntCC::Equal, rhs, -1);
            let exceptional = builder.ins().band(minimum, negative_one);
            builder
                .ins()
                .brif(exceptional, overflow, &[], remainder, &[]);
            builder.switch_to_block(overflow);
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().jump(merge, &[zero.into()]);
            builder.switch_to_block(remainder);
            let value = builder.ins().srem(lhs, rhs);
            builder.ins().jump(merge, &[value.into()]);
        }
        RegionBinaryOp::Concat | RegionBinaryOp::Div | RegionBinaryOp::Pow => unreachable!(),
    }

    builder.switch_to_block(rejected);
    let value = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[value.into()]);
    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_cached_array_fetch(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    lifecycle: Option<NativeHelper>,
    operation: u32,
    array: ir::Value,
    key: ir::Value,
    constant_string_key: bool,
    _unit_identity: u64,
    result_out: ir::Value,
    deopt_out: ir::Value,
    optimizing_transition: Option<NativeOptimizingTransition<'_>>,
) -> Result<ir::Value, CraneliftLoweringError> {
    if let Some(transition) = optimizing_transition {
        let inspect = builder.create_block();
        let shared = builder.create_block();
        let ordinary = builder.create_block();
        let merge = builder.create_block();
        builder.append_block_param(merge, types::I64);
        let is_array = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
        let index = builder.ins().ireduce(types::I32, array);
        let direct = builder.ins().icmp_imm(
            IntCC::UnsignedGreaterThanOrEqual,
            index,
            i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
        );
        let inspect_direct = builder.ins().band(is_array, direct);
        builder
            .ins()
            .brif(inspect_direct, inspect, &[], ordinary, &[]);

        builder.switch_to_block(inspect);
        let slot = lower_optimizing_slot_address(builder, array, deopt_out);
        let kind = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
        );
        let flags = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
        );
        let shared_kind = builder.ins().icmp_imm(
            IntCC::Equal,
            kind,
            i64::from(crate::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY),
        );
        let borrowed_kind = builder.ins().icmp_imm(
            IntCC::Equal,
            kind,
            i64::from(crate::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY),
        );
        let shared_kind = builder.ins().bor(shared_kind, borrowed_kind);
        let shared_version = builder.ins().icmp_imm(
            IntCC::Equal,
            flags,
            i64::from(crate::JIT_NATIVE_SHARED_ARRAY_ABI_VERSION),
        );
        let admitted = builder.ins().band(shared_kind, shared_version);
        builder.ins().brif(admitted, shared, &[], ordinary, &[]);

        builder.switch_to_block(shared);
        let value = lower_shared_array_fetch(builder, array, key, operation, transition)?;
        builder.ins().jump(merge, &[value.into()]);

        builder.switch_to_block(ordinary);
        let value = lower_cached_array_fetch_inner(
            module,
            builder,
            helper,
            lifecycle,
            operation,
            array,
            key,
            constant_string_key,
            _unit_identity,
            result_out,
            deopt_out,
            Some(transition),
        )?;
        builder.ins().jump(merge, &[value.into()]);

        builder.switch_to_block(merge);
        return Ok(builder.block_params(merge)[0]);
    }
    lower_cached_array_fetch_inner(
        module,
        builder,
        helper,
        lifecycle,
        operation,
        array,
        key,
        constant_string_key,
        _unit_identity,
        result_out,
        deopt_out,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn lower_cached_array_fetch_inner(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    lifecycle: Option<NativeHelper>,
    operation: u32,
    array: ir::Value,
    key: ir::Value,
    constant_string_key: bool,
    _unit_identity: u64,
    result_out: ir::Value,
    deopt_out: ir::Value,
    optimizing_transition: Option<NativeOptimizingTransition<'_>>,
) -> Result<ir::Value, CraneliftLoweringError> {
    if optimizing_transition.is_none() && !helper.is_some_and(|helper| helper.inline_runtime_view) {
        return lower_native_value_operation(
            module,
            builder,
            helper,
            operation,
            &[array, key],
            result_out,
        );
    }
    let inspect_runtime = builder.create_block();
    let inspect_normal = builder.create_block();
    let inspect_direct = builder.create_block();
    let inspect_descriptor = builder.create_block();
    let entry_loop = builder.create_block();
    let entry_compare = builder.create_block();
    let entry_next = builder.create_block();
    let entry_missing = builder.create_block();
    let cached = builder.create_block();
    let slow_not_tagged = builder.create_block();
    let slow_view_missing = builder.create_block();
    let slow_key_unsupported = builder.create_block();
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(inspect_descriptor, module.target_config().pointer_type());
    builder.append_block_param(cached, types::I64);
    builder.append_block_param(entry_loop, types::I64);
    builder.append_block_param(entry_next, types::I64);
    builder.append_block_param(merge, types::I64);

    let is_array = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    builder
        .ins()
        .brif(is_array, inspect_runtime, &[], slow_not_tagged, &[]);

    let pointer_type = module.target_config().pointer_type();
    let view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    builder.switch_to_block(inspect_runtime);
    let raw_index = builder.ins().ireduce(types::I32, array);
    let is_direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        raw_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    builder
        .ins()
        .brif(is_direct, inspect_direct, &[], inspect_normal, &[]);

    builder.switch_to_block(inspect_normal);
    let views = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, value_slots) as i32,
    );
    let array_index = builder.ins().ireduce(types::I32, array);
    let array_index = builder.ins().uextend(pointer_type, array_index);
    let descriptor_offset = builder.ins().ishl_imm(array_index, 5);
    let descriptor = builder.ins().iadd(views, descriptor_offset);
    builder.ins().jump(inspect_descriptor, &[descriptor.into()]);

    builder.switch_to_block(inspect_direct);
    let descriptor = lower_optimizing_slot_address(builder, array, deopt_out);
    builder.ins().jump(inspect_descriptor, &[descriptor.into()]);

    builder.switch_to_block(inspect_descriptor);
    let descriptor = builder.block_params(inspect_descriptor)[0];
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let runtime_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_ARRAY),
    );
    let arena_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let array_kind = builder.ins().bor(runtime_kind, arena_kind);
    let key_runtime = lower_is_runtime_handle(builder, key);
    let key_constant = lower_value_has_namespace_tag(builder, key, crate::JIT_VALUE_CONSTANT_TAG);
    let namespaced = builder.ins().bor(key_runtime, key_constant);
    let key_is_immediate = builder.ins().icmp_imm(IntCC::Equal, namespaced, 0);
    let key_is_string = lower_value_has_tag(builder, key, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    let supported_key = if optimizing_transition.is_some() {
        let supported = builder.ins().bor(key_is_immediate, key_is_string);
        if constant_string_key {
            builder.ins().bor(supported, key_constant)
        } else {
            supported
        }
    } else {
        key_is_immediate
    };
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let zero = builder.ins().iconst(types::I64, 0);
    let inspect_key = builder.create_block();
    builder
        .ins()
        .brif(array_kind, inspect_key, &[], slow_view_missing, &[]);

    builder.switch_to_block(inspect_key);
    builder.ins().brif(
        supported_key,
        entry_loop,
        &[zero.into()],
        slow_key_unsupported,
        &[],
    );

    builder.switch_to_block(entry_loop);
    let index = builder.block_params(entry_loop)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder
        .ins()
        .brif(exhausted, entry_missing, &[], entry_compare, &[]);

    builder.switch_to_block(entry_compare);
    let entry_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let entry_offset = builder.ins().ishl_imm(entry_index, 4);
    let entry = builder.ins().iadd(entries, entry_offset);
    let candidate = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), entry, 0);
    let matches = if optimizing_transition.is_some() {
        lower_native_array_key_equal(builder, candidate, key, deopt_out)
    } else {
        builder.ins().icmp(IntCC::Equal, candidate, key)
    };
    let found = if operation == 2 {
        builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        )
    } else {
        let value = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            entry,
            std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
        );
        if operation == 3 {
            let is_null =
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX));
            let non_null = builder.ins().icmp_imm(IntCC::Equal, is_null, 0);
            encode_native_bool(builder, non_null)
        } else {
            value
        }
    };
    builder.ins().brif(
        matches,
        cached,
        &[found.into()],
        entry_next,
        &[index.into()],
    );

    builder.switch_to_block(entry_next);
    let index = builder.block_params(entry_next)[0];
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(entry_loop, &[next.into()]);

    builder.switch_to_block(entry_missing);
    if operation == 2 || operation == 3 {
        let missing = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
        );
        builder.ins().jump(cached, &[missing.into()]);
    } else if operation & 1 == 1 {
        let missing = builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
        builder.ins().jump(cached, &[missing.into()]);
    } else {
        builder.ins().jump(slow, &[]);
    }

    builder.switch_to_block(cached);
    let value = builder.block_params(cached)[0];
    let value = if let Some(transition) = optimizing_transition {
        lower_optimizing_retain(builder, value, transition.deopt_out);
        value
    } else {
        lower_guarded_value_release(module, builder, lifecycle, 0, value, result_out, deopt_out)?
    };
    builder.ins().jump(merge, &[value.into()]);

    builder.switch_to_block(slow);
    let value = if let Some(transition) = optimizing_transition {
        transition.emit_value(builder)?
    } else {
        lower_native_value_operation(
            module,
            builder,
            helper,
            operation,
            &[array, key],
            result_out,
        )?
    };
    builder.ins().jump(merge, &[value.into()]);

    for (block, detail) in [
        (slow_not_tagged, crate::JIT_OPTIMIZING_EXIT_ARRAY_NOT_TAGGED),
        (
            slow_view_missing,
            crate::JIT_OPTIMIZING_EXIT_ARRAY_VIEW_MISSING,
        ),
        (
            slow_key_unsupported,
            crate::JIT_OPTIMIZING_EXIT_ARRAY_KEY_UNSUPPORTED,
        ),
    ] {
        builder.switch_to_block(block);
        if let Some(transition) = optimizing_transition {
            let value = transition.emit_value_with_detail(builder, detail)?;
            builder.ins().jump(merge, &[value.into()]);
        } else {
            builder.ins().jump(slow, &[]);
        }
    }

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[allow(clippy::too_many_arguments)]
fn lower_direct_foreach_next(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    lifecycle: Option<NativeHelper>,
    iterator: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<(ir::Value, ir::Value, ir::Value), CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let helper = helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
            "native foreach-next helper was not declared",
        )
    })?;
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);
    builder.append_block_param(merge, types::I64);
    builder.append_block_param(merge, types::I64);

    if helper.inline_runtime_view {
        let inspect_runtime = builder.create_block();
        let inspect_descriptor = builder.create_block();
        let inspect_cursor = builder.create_block();
        let present = builder.create_block();
        let exhausted = builder.create_block();

        let is_iterator =
            lower_value_has_tag(builder, iterator, crate::JIT_VALUE_RUNTIME_ITERATOR_TAG);
        builder
            .ins()
            .brif(is_iterator, inspect_runtime, &[], slow, &[]);

        let view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
        builder.switch_to_block(inspect_runtime);
        let views = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            deopt_out,
            view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, value_slots) as i32,
        );
        let index = builder.ins().ireduce(types::I32, iterator);
        builder.ins().jump(inspect_descriptor, &[]);

        builder.switch_to_block(inspect_descriptor);
        let index = builder.ins().uextend(pointer_type, index);
        let descriptor_offset = builder.ins().ishl_imm(index, 5);
        let descriptor = builder.ins().iadd(views, descriptor_offset);
        let kind = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            descriptor,
            std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
        );
        let flags = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            descriptor,
            std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
        );
        let payload = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            descriptor,
            std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
        );
        let kind_ok = builder.ins().icmp_imm(
            IntCC::Equal,
            kind,
            i64::from(crate::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT),
        );
        let flags_ok = builder.ins().icmp_imm(
            IntCC::Equal,
            flags,
            i64::from(crate::JIT_NATIVE_FOREACH_VIEW_ABI_VERSION),
        );
        let payload_ok = builder.ins().icmp_imm(IntCC::NotEqual, payload, 0);
        let descriptor_ok = builder.ins().band(kind_ok, flags_ok);
        let descriptor_ok = builder.ins().band(descriptor_ok, payload_ok);
        builder
            .ins()
            .brif(descriptor_ok, inspect_cursor, &[], slow, &[]);

        builder.switch_to_block(inspect_cursor);
        let foreach_view = if pointer_type == types::I64 {
            payload
        } else {
            builder.ins().ireduce(pointer_type, payload)
        };
        let cursor = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            foreach_view,
            std::mem::offset_of!(crate::JitNativeForeachView, cursor) as i32,
        );
        let length = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            foreach_view,
            std::mem::offset_of!(crate::JitNativeForeachView, length) as i32,
        );
        let has_value = builder.ins().icmp(IntCC::UnsignedLessThan, cursor, length);
        builder.ins().brif(has_value, present, &[], exhausted, &[]);

        builder.switch_to_block(present);
        let entries = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            foreach_view,
            std::mem::offset_of!(crate::JitNativeForeachView, entries) as i32,
        );
        let entry_offset = builder.ins().ishl_imm(cursor, 4);
        let entry_offset = if pointer_type == types::I64 {
            entry_offset
        } else {
            builder.ins().ireduce(pointer_type, entry_offset)
        };
        let entry = builder.ins().iadd(entries, entry_offset);
        let key = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            entry,
            std::mem::offset_of!(crate::JitNativeForeachEntry, key) as i32,
        );
        let value = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            entry,
            std::mem::offset_of!(crate::JitNativeForeachEntry, value) as i32,
        );
        let next = builder.ins().iadd_imm(cursor, 1);
        builder.ins().store(
            MemFlagsData::new(),
            next,
            foreach_view,
            std::mem::offset_of!(crate::JitNativeForeachView, cursor) as i32,
        );
        let key =
            lower_guarded_value_release(module, builder, lifecycle, 0, key, result_out, deopt_out)?;
        let value = lower_guarded_value_release(
            module, builder, lifecycle, 0, value, result_out, deopt_out,
        )?;
        let one = builder.ins().iconst(types::I64, 1);
        builder
            .ins()
            .jump(merge, &[key.into(), value.into(), one.into()]);

        builder.switch_to_block(exhausted);
        let null = builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
        let zero = builder.ins().iconst(types::I64, 0);
        builder
            .ins()
            .jump(merge, &[null.into(), null.into(), zero.into()]);
    } else {
        builder.ins().jump(slow, &[]);
    }

    builder.switch_to_block(slow);
    let key_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let value_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let has_slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let key_out = builder.ins().stack_addr(pointer_type, key_slot, 0);
    let value_out = builder.ins().stack_addr(pointer_type, value_slot, 0);
    let has_out = builder.ins().stack_addr(pointer_type, has_slot, 0);
    let call = call_native_helper(
        module,
        builder,
        helper,
        &[iterator, key_out, value_out, has_out],
    );
    require_native_operation_ok(
        builder,
        builder.inst_results(call)[0],
        helper.terminal_exit()?,
    )?;
    let key = builder.ins().stack_load(types::I64, key_slot, 0);
    let value = builder.ins().stack_load(types::I64, value_slot, 0);
    let has = builder.ins().stack_load(types::I64, has_slot, 0);
    builder
        .ins()
        .jump(merge, &[key.into(), value.into(), has.into()]);

    builder.switch_to_block(merge);
    let params = builder.block_params(merge);
    Ok((params[0], params[1], params[2]))
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
        deopt_out,
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

fn lower_guarded_isset_value(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    value: ir::Value,
    result_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    let null = builder
        .ins()
        .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
    if !helper.is_some_and(|helper| helper.inline_runtime_view) {
        return lower_native_value_operation(
            module,
            builder,
            helper,
            native_compare_opcode(RegionCompareOpCode::NotIdentical),
            &[value, null],
            result_out,
        );
    }

    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);

    let kind = builder
        .ins()
        .band_imm(value, crate::JIT_VALUE_RUNTIME_KIND_MASK as i64);
    let is_reference = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG as i64,
    );
    let is_boxed_scalar =
        builder
            .ins()
            .icmp_imm(IntCC::Equal, kind, crate::JIT_VALUE_RUNTIME_TAG as i64);
    let is_constant = lower_value_has_namespace_tag(builder, value, crate::JIT_VALUE_CONSTANT_TAG);
    let constant_index = builder.ins().ireduce(types::I32, value);
    let is_reserved = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        constant_index,
        i64::from(crate::JIT_VALUE_TRUE),
    );
    let is_not_reserved = lower_not_bool(builder, is_reserved);
    let is_opaque_constant = builder.ins().band(is_constant, is_not_reserved);
    let needs_decode = builder.ins().bor(is_reference, is_boxed_scalar);
    let needs_decode = builder.ins().bor(needs_decode, is_opaque_constant);
    let is_set = builder.ins().icmp(IntCC::NotEqual, value, null);
    let is_set = encode_native_bool(builder, is_set);
    builder
        .ins()
        .brif(needs_decode, slow, &[], merge, &[is_set.into()]);

    builder.switch_to_block(slow);
    let is_set = lower_native_value_operation(
        module,
        builder,
        helper,
        native_compare_opcode(RegionCompareOpCode::NotIdentical),
        &[value, null],
        result_out,
    )?;
    builder.ins().jump(merge, &[is_set.into()]);

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

fn lower_guarded_value_release(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    operation: u32,
    value: ir::Value,
    _result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    if operation & 1 == 0 {
        let retain = builder.create_block();
        let direct_slot = builder.create_block();
        let normal_slot = builder.create_block();
        let update = builder.create_block();
        let done = builder.create_block();
        let pointer_type = module.target_config().pointer_type();
        builder.append_block_param(update, pointer_type);
        let is_runtime = lower_is_runtime_handle(builder, value);
        builder.ins().brif(is_runtime, retain, &[], done, &[]);

        builder.switch_to_block(retain);
        let view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
        let index = builder.ins().ireduce(types::I32, value);
        let is_direct = builder.ins().icmp_imm(
            IntCC::UnsignedGreaterThanOrEqual,
            index,
            i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
        );
        builder
            .ins()
            .brif(is_direct, direct_slot, &[], normal_slot, &[]);

        builder.switch_to_block(direct_slot);
        let direct_slots = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            deopt_out,
            view_offset
                + std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
        );
        let direct_index = builder
            .ins()
            .iadd_imm(index, -i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE));
        let direct_index = builder.ins().uextend(pointer_type, direct_index);
        let direct_offset = builder.ins().ishl_imm(direct_index, 5);
        let direct_cell = builder.ins().iadd(direct_slots, direct_offset);
        builder.ins().jump(update, &[direct_cell.into()]);

        builder.switch_to_block(normal_slot);
        let value_slots = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            deopt_out,
            view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, value_slots) as i32,
        );
        let normal_index = builder.ins().uextend(pointer_type, index);
        let normal_offset = builder.ins().ishl_imm(normal_index, 5);
        let normal_cell = builder.ins().iadd(value_slots, normal_offset);
        builder.ins().jump(update, &[normal_cell.into()]);

        builder.switch_to_block(update);
        let cell = builder.block_params(update)[0];
        let count = builder.ins().load(types::I32, MemFlagsData::new(), cell, 0);
        let incremented = builder.ins().iadd_imm(count, 1);
        builder
            .ins()
            .store(MemFlagsData::new(), incremented, cell, 0);
        builder.ins().jump(done, &[]);

        builder.switch_to_block(done);
        return Ok(value);
    }
    let inspect = builder.create_block();
    let decrement = builder.create_block();
    let slow = builder.create_block();
    let done = builder.create_block();
    let is_runtime = lower_is_runtime_handle(builder, value);
    builder.ins().brif(is_runtime, inspect, &[], done, &[]);

    builder.switch_to_block(inspect);
    let cell = lower_optimizing_slot_address(builder, value, deopt_out);
    let count = builder.ins().load(types::I32, MemFlagsData::new(), cell, 0);
    let shared = builder.ins().icmp_imm(IntCC::UnsignedGreaterThan, count, 1);
    builder.ins().brif(shared, decrement, &[], slow, &[]);

    builder.switch_to_block(decrement);
    let updated = builder.ins().iadd_imm(count, -1);
    builder.ins().store(MemFlagsData::new(), updated, cell, 0);
    builder.ins().jump(done, &[]);

    builder.switch_to_block(slow);
    let helper = helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
            "cold final-release helper was not declared",
        )
    })?;
    let call = call_native_helper(module, builder, helper, &[value]);
    let status = builder.inst_results(call)[0];
    require_native_operation_ok(builder, status, helper.terminal_exit()?)?;
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
        lower_guarded_value_release(
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
        let _ = lower_guarded_value_release(
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
    let pointer_type = module.target_config().pointer_type();
    let views = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        view_offset + std::mem::offset_of!(crate::JitNativeRuntimeView, value_slots) as i32,
    );
    let index = builder.ins().ireduce(types::I32, value);
    builder.ins().jump(inspect_descriptor, &[]);

    builder.switch_to_block(inspect_descriptor);
    let index = builder.ins().uextend(pointer_type, index);
    let byte_offset = builder.ins().ishl_imm(index, 5);
    let descriptor = builder.ins().iadd(views, byte_offset);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let address = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        descriptor,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
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
        lower_guarded_value_release(
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

fn lowering_operand_fact(
    value_flow: &ExecutableValueFlow,
    constants: &[IrConstant],
    operand: RegionOperand,
) -> SsaValueFact {
    value_flow.operand_fact(constants, operand)
}

fn lower_array_key_operand(
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    registers: &NativeRegisterMap,
    constants: &[IrConstant],
    operand: RegionOperand,
) -> Result<ir::Value, CraneliftLoweringError> {
    match operand {
        RegionOperand::Constant(index) => match constants.get(index as usize) {
            Some(IrConstant::Int(value)) => Ok(builder.ins().iconst(types::I64, *value)),
            Some(IrConstant::Bool(value)) => {
                Ok(builder.ins().iconst(types::I64, i64::from(*value)))
            }
            _ => lower_region_operand(builder, locals, registers, operand),
        },
        _ => lower_region_operand(builder, locals, registers, operand),
    }
}

fn array_key_is_string_constant(constants: &[IrConstant], operand: RegionOperand) -> bool {
    matches!(
        operand,
        RegionOperand::Constant(index)
            if matches!(
                constants.get(index as usize),
                Some(IrConstant::String(_) | IrConstant::StringBytes(_))
            )
    )
}

#[allow(clippy::too_many_arguments)]
fn optimizing_compare_is_direct(
    op: RegionCompareOpCode,
    lhs: crate::region_ir::SsaValueFact,
    rhs: crate::region_ir::SsaValueFact,
) -> bool {
    if matches!(
        op,
        RegionCompareOpCode::Identical | RegionCompareOpCode::NotIdentical
    ) && ((lhs.certainty != crate::region_ir::SsaCertainty::Unknown
        && matches!(
            lhs.class,
            SsaValueClass::Null | SsaValueClass::Bool | SsaValueClass::Int
        ))
        || (rhs.certainty != crate::region_ir::SsaCertainty::Unknown
            && matches!(
                rhs.class,
                SsaValueClass::Null | SsaValueClass::Bool | SsaValueClass::Int
            )))
    {
        return true;
    }
    if lhs.certainty == crate::region_ir::SsaCertainty::Unknown
        || rhs.certainty == crate::region_ir::SsaCertainty::Unknown
    {
        return false;
    }
    (lhs.class == SsaValueClass::Int && rhs.class == SsaValueClass::Int)
        || (lhs.class == rhs.class
            && matches!(lhs.class, SsaValueClass::Bool | SsaValueClass::Null))
        || (lhs.class != rhs.class
            && matches!(
                op,
                RegionCompareOpCode::Identical | RegionCompareOpCode::NotIdentical
            ))
}

fn optimizing_cast_is_direct(op: RegionCastOp, fact: crate::region_ir::SsaValueFact) -> bool {
    if fact.certainty == crate::region_ir::SsaCertainty::Unknown {
        return false;
    }
    match op {
        RegionCastOp::Bool => matches!(
            fact.class,
            SsaValueClass::Null | SsaValueClass::Bool | SsaValueClass::Int
        ),
        RegionCastOp::Int => matches!(
            fact.class,
            SsaValueClass::Null | SsaValueClass::Bool | SsaValueClass::Int
        ),
        RegionCastOp::Void => true,
        RegionCastOp::Float | RegionCastOp::String | RegionCastOp::Array | RegionCastOp::Object => {
            false
        }
    }
}

fn optimizing_fact_satisfies_type(
    fact: crate::region_ir::SsaValueFact,
    type_: &php_ir::IrReturnType,
) -> bool {
    if fact.certainty == crate::region_ir::SsaCertainty::Unknown {
        return matches!(type_, php_ir::IrReturnType::Mixed);
    }
    match type_ {
        php_ir::IrReturnType::Int => fact.class == SsaValueClass::Int,
        php_ir::IrReturnType::Float => fact.class == SsaValueClass::Float,
        php_ir::IrReturnType::String => fact.class == SsaValueClass::StringHandle,
        php_ir::IrReturnType::Array => fact.class == SsaValueClass::ArrayHandle,
        php_ir::IrReturnType::Callable => fact.class == SsaValueClass::CallableHandle,
        php_ir::IrReturnType::Object => fact.class == SsaValueClass::ObjectHandle,
        php_ir::IrReturnType::Bool => fact.class == SsaValueClass::Bool,
        php_ir::IrReturnType::Null | php_ir::IrReturnType::Void => {
            fact.class == SsaValueClass::Null
        }
        // A general boolean fact does not prove either literal singleton
        // type.  Treating it as proof allowed `true` to enter a `false`
        // parameter (and vice versa) without the PHP-visible type check.
        php_ir::IrReturnType::False | php_ir::IrReturnType::True => false,
        php_ir::IrReturnType::Mixed => true,
        php_ir::IrReturnType::Nullable { inner } => {
            fact.class == SsaValueClass::Null || optimizing_fact_satisfies_type(fact, inner)
        }
        php_ir::IrReturnType::Union { members } => members
            .iter()
            .any(|member| optimizing_fact_satisfies_type(fact, member)),
        php_ir::IrReturnType::Iterable => matches!(
            fact.class,
            SsaValueClass::ArrayHandle | SsaValueClass::ObjectHandle
        ),
        php_ir::IrReturnType::Never
        | php_ir::IrReturnType::Class { .. }
        | php_ir::IrReturnType::Intersection { .. }
        | php_ir::IrReturnType::Dnf { .. } => false,
    }
}

/// Whether an unknown value has a sound, helper-free admission lane for this
/// PHP parameter type.  The predicate is deliberately allowed to recognize a
/// strict subset of the PHP type: values outside that subset take the single
/// baseline-continuation transition, where weak scalar coercion and complex
/// class/interface checks remain authoritative.
fn optimizing_type_has_direct_guard(type_: &php_ir::IrReturnType) -> bool {
    match type_ {
        php_ir::IrReturnType::Int
        | php_ir::IrReturnType::Float
        | php_ir::IrReturnType::String
        | php_ir::IrReturnType::Array
        | php_ir::IrReturnType::Callable
        | php_ir::IrReturnType::Iterable
        | php_ir::IrReturnType::Object
        | php_ir::IrReturnType::Bool
        | php_ir::IrReturnType::Null
        | php_ir::IrReturnType::False
        | php_ir::IrReturnType::True
        | php_ir::IrReturnType::Mixed => true,
        php_ir::IrReturnType::Nullable { .. } => true,
        php_ir::IrReturnType::Union { members } => {
            members.iter().any(optimizing_type_has_direct_guard)
        }
        php_ir::IrReturnType::Void
        | php_ir::IrReturnType::Never
        | php_ir::IrReturnType::Class { .. }
        | php_ir::IrReturnType::Intersection { .. }
        | php_ir::IrReturnType::Dnf { .. } => false,
    }
}

/// Emit the exact native-tag test for the subset of `type_` admitted by the
/// optimizing compiled-call ABI.  This contains no runtime helper, name
/// lookup, operation ID, or Rust `Value` conversion.
fn lower_optimizing_call_argument_type_guard(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    type_: &php_ir::IrReturnType,
) -> Option<ir::Value> {
    let equals_constant = |builder: &mut FunctionBuilder<'_>, constant| {
        builder.ins().icmp_imm(IntCC::Equal, value, constant)
    };
    let has_kind =
        |builder: &mut FunctionBuilder<'_>, tag| lower_value_has_tag(builder, value, tag);
    match type_ {
        php_ir::IrReturnType::Int => {
            let constant =
                lower_value_has_namespace_tag(builder, value, crate::JIT_VALUE_CONSTANT_TAG);
            Some(lower_is_immediate_int(builder, value, constant))
        }
        php_ir::IrReturnType::Float => Some(has_kind(builder, crate::JIT_VALUE_RUNTIME_FLOAT_TAG)),
        php_ir::IrReturnType::String => {
            Some(has_kind(builder, crate::JIT_VALUE_RUNTIME_STRING_TAG))
        }
        php_ir::IrReturnType::Array => Some(has_kind(builder, crate::JIT_VALUE_RUNTIME_ARRAY_TAG)),
        php_ir::IrReturnType::Callable => {
            Some(has_kind(builder, crate::JIT_VALUE_RUNTIME_CALLABLE_TAG))
        }
        // Array is the directly provable iterable lane. Traversable objects
        // require class/interface metadata and therefore transition once to
        // the baseline binder rather than reintroducing a runtime helper.
        php_ir::IrReturnType::Iterable => {
            Some(has_kind(builder, crate::JIT_VALUE_RUNTIME_ARRAY_TAG))
        }
        php_ir::IrReturnType::Object => {
            let object = has_kind(builder, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
            let callable = has_kind(builder, crate::JIT_VALUE_RUNTIME_CALLABLE_TAG);
            let generator = has_kind(builder, crate::JIT_VALUE_RUNTIME_GENERATOR_TAG);
            let fiber = has_kind(builder, crate::JIT_VALUE_RUNTIME_FIBER_TAG);
            let object = builder.ins().bor(object, callable);
            let object = builder.ins().bor(object, generator);
            Some(builder.ins().bor(object, fiber))
        }
        php_ir::IrReturnType::Bool => {
            let false_ =
                equals_constant(builder, crate::jit_encode_constant(crate::JIT_VALUE_FALSE));
            let true_ = equals_constant(builder, crate::jit_encode_constant(crate::JIT_VALUE_TRUE));
            Some(builder.ins().bor(false_, true_))
        }
        php_ir::IrReturnType::Null => Some(equals_constant(
            builder,
            crate::jit_encode_constant(u32::MAX),
        )),
        php_ir::IrReturnType::False => Some(equals_constant(
            builder,
            crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
        )),
        php_ir::IrReturnType::True => Some(equals_constant(
            builder,
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        )),
        php_ir::IrReturnType::Mixed => Some(builder.ins().iconst(types::I8, 1)),
        php_ir::IrReturnType::Nullable { inner } => {
            let null = equals_constant(builder, crate::jit_encode_constant(u32::MAX));
            let inner = lower_optimizing_call_argument_type_guard(builder, value, inner);
            Some(inner.map_or(null, |inner| builder.ins().bor(null, inner)))
        }
        php_ir::IrReturnType::Union { members } => members.iter().fold(None, |accepted, member| {
            let member = lower_optimizing_call_argument_type_guard(builder, value, member);
            match (accepted, member) {
                (None, member) => member,
                (accepted, None) => accepted,
                (Some(accepted), Some(member)) => Some(builder.ins().bor(accepted, member)),
            }
        }),
        php_ir::IrReturnType::Void
        | php_ir::IrReturnType::Never
        | php_ir::IrReturnType::Class { .. }
        | php_ir::IrReturnType::Intersection { .. }
        | php_ir::IrReturnType::Dnf { .. } => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_optimizing_region_instruction(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    register_variables: &NativeRegisterMap,
    locals: &NativeLocalMap,
    registers: &mut NativeRegisterMap,
    instruction: &RegionInstruction,
    transition_live_registers: &[RegId],
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
    inline_constants: &BTreeMap<FunctionId, BoundedInlineValue>,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    runtime: ir::Value,
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    native_version: u32,
    unit_identity: u64,
) -> Result<EmittedOptimizingInstruction, CraneliftLoweringError> {
    let emitted_transition = Cell::new(false);
    // Resolve the semantic live set at the real instruction boundary. These
    // values dominate every direct-data-path block emitted for the operation,
    // so cold exits do not reconstruct them through Cranelift frontend
    // Variable alias chains created by the operation's internal CFG.
    let transition_live_values = transition_live_registers
        .iter()
        .copied()
        .map(|register| Ok((register, use_region_register(builder, registers, register)?)))
        .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
    let transition = NativeOptimizingTransition {
        result_out,
        deopt_out,
        function,
        local_count,
        instruction,
        locals,
        live_values: &transition_live_values,
        native_version,
        emitted_transition: &emitted_transition,
    };
    let mut emitted_class = crate::JitProductionLoweringClass::DirectClif;
    match &instruction.kind {
        RegionInstructionKind::Nop => {}
        RegionInstructionKind::Move { dst, src } => {
            let value = lower_region_operand(builder, locals, registers, *src)?;
            if value_copy_requires_retain(lowering_operand_fact(value_flow, constants, *src))
                && !value_flow.moves_value_into_register(instruction.continuation_id)
            {
                lower_optimizing_retain(builder, value, deopt_out);
            }
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::LoadLocal { dst, local, .. } => {
            let stored = use_local_variable(builder, locals, *local)?;
            let fact = value_flow.local_fact(*local);
            let borrows_for_dimension =
                value_flow.passes_reference_to_typed_consumer(instruction.continuation_id);
            let retain_plain_value = value_copy_requires_retain(fact)
                && !value_flow.can_borrow_local_load(instruction.continuation_id)
                && !borrows_for_dimension;
            let value = if value_flow.local_storage(*local)
                == crate::region_ir::LocalStorageClass::MemoryReference
            {
                lower_optimizing_reference_scalar(builder, stored, retain_plain_value, transition)?
            } else {
                stored
            };
            if value == stored && retain_plain_value {
                lower_optimizing_retain(builder, value, deopt_out);
            }
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::StoreLocal { local, src } => {
            let value = lower_region_operand(builder, locals, registers, *src)?;
            let current = use_local_variable(builder, locals, *local)?;
            if instruction.live_locals.contains(local)
                && value_release_required(value_flow.local_fact(*local))
            {
                lower_optimizing_release(builder, current, transition)?;
            }
            if value_copy_requires_retain(lowering_operand_fact(value_flow, constants, *src))
                && !value_flow.moves_value_into_local(instruction.continuation_id)
            {
                lower_optimizing_retain(builder, value, deopt_out);
            }
            define_local_variable(builder, locals, *local, value)?;
        }
        RegionInstructionKind::AssignLocalResult { dst, local, value } => {
            let operand = *value;
            let value = lower_region_operand(builder, locals, registers, operand)?;
            let current = use_local_variable(builder, locals, *local)?;
            if instruction.live_locals.contains(local)
                && value_release_required(value_flow.local_fact(*local))
            {
                lower_optimizing_release(builder, current, transition)?;
            }
            if value_copy_requires_retain(lowering_operand_fact(value_flow, constants, operand)) {
                lower_optimizing_retain(builder, value, deopt_out);
            }
            define_local_variable(builder, locals, *local, value)?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::Discard { src } => {
            if !value_flow.elides_discard(instruction.continuation_id) {
                let fact = lowering_operand_fact(value_flow, constants, *src);
                if value_release_required(fact) {
                    let value = lower_region_operand(builder, locals, registers, *src)?;
                    lower_optimizing_release(builder, value, transition)?;
                }
            }
        }
        RegionInstructionKind::IssetLocal { dst, local } => {
            let value = use_local_variable(builder, locals, *local)?;
            let reference =
                lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
            let direct = builder.create_block();
            let rejected = builder.create_block();
            builder.ins().brif(reference, rejected, &[], direct, &[]);
            builder.switch_to_block(rejected);
            let _ = transition.emit_value(builder)?;
            builder.ins().jump(direct, &[]);
            builder.switch_to_block(direct);
            let null =
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX));
            let uninitialized = builder.ins().icmp_imm(
                IntCC::Equal,
                value,
                crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
            );
            let absent = builder.ins().bor(null, uninitialized);
            let present = builder.ins().icmp_imm(IntCC::Equal, absent, 0);
            let result = encode_native_bool(builder, present);
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::EmptyLocal { dst, local } => {
            let value = use_local_variable(builder, locals, *local)?;
            let truthy = lower_optimizing_truthy(builder, value, transition)?;
            let empty = builder.ins().bxor_imm(truthy, 1);
            let result = encode_native_bool(builder, empty);
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::UnsetLocal { local } => {
            let value = use_local_variable(builder, locals, *local)?;
            if value_release_required(value_flow.local_fact(*local)) {
                lower_optimizing_release(builder, value, transition)?;
            }
            let uninitialized = builder.ins().iconst(
                types::I64,
                crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
            );
            define_local_variable(builder, locals, *local, uninitialized)?;
        }
        RegionInstructionKind::NewArray { dst } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let array = lower_direct_new_array(
                module,
                builder,
                None,
                result_out,
                deopt_out,
                Some(transition),
            )?;
            define_region_register(builder, register_variables, registers, *dst, array)?;
        }
        RegionInstructionKind::ArrayInsert {
            array,
            key: None,
            value,
            by_ref_local: None,
        } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let current = use_region_register(builder, registers, *array)?;
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let updated = lower_direct_array_append(
                module,
                builder,
                current,
                None,
                value,
                result_out,
                deopt_out,
                NativeArrayAppendFallback::Optimizing(transition),
            )?;
            define_region_register(builder, register_variables, registers, *array, updated)?;
        }
        RegionInstructionKind::ArrayInsert {
            array,
            key: Some(key),
            value,
            by_ref_local: None,
        } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let current = use_region_register(builder, registers, *array)?;
            let key = lower_array_key_operand(builder, locals, registers, constants, *key)?;
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let updated = lower_direct_array_insert(
                module,
                builder,
                current,
                key,
                value,
                result_out,
                deopt_out,
                NativeArrayAppendFallback::Optimizing(transition),
            )?;
            define_region_register(builder, register_variables, registers, *array, updated)?;
        }
        RegionInstructionKind::AppendDim {
            dst,
            local,
            keys,
            value,
        } if keys.is_empty() => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let current = use_local_variable(builder, locals, *local)?;
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let updated = lower_direct_array_append(
                module,
                builder,
                current,
                None,
                value,
                result_out,
                deopt_out,
                NativeArrayAppendFallback::Optimizing(transition),
            )?;
            define_local_variable(builder, locals, *local, updated)?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::AssignDim {
            dst,
            local,
            keys,
            value,
        } if keys.len() == 1 => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let current = use_local_variable(builder, locals, *local)?;
            let key = lower_array_key_operand(builder, locals, registers, constants, keys[0])?;
            let value = lower_region_operand(builder, locals, registers, *value)?;
            let updated = if value_flow.local_storage(*local)
                == crate::region_ir::LocalStorageClass::MemoryReference
            {
                lower_reference_array_assign(builder, current, key, value, transition)?
            } else {
                lower_direct_array_insert(
                    module,
                    builder,
                    current,
                    key,
                    value,
                    result_out,
                    deopt_out,
                    NativeArrayAppendFallback::Optimizing(transition),
                )?
            };
            define_local_variable(builder, locals, *local, updated)?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::IssetDim { dst, local, keys } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            if keys.len() == 1 {
                let key_operand = keys[0];
                let constant_string_key = array_key_is_string_constant(constants, key_operand);
                let key =
                    lower_array_key_operand(builder, locals, registers, constants, key_operand)?;
                let current = use_local_variable(builder, locals, *local)?;
                let value = if value_flow.local_storage(*local)
                    == crate::region_ir::LocalStorageClass::MemoryReference
                {
                    let reference = builder.create_block();
                    let plain = builder.create_block();
                    let merge = builder.create_block();
                    builder.append_block_param(merge, types::I64);
                    let is_reference = lower_value_has_tag(
                        builder,
                        current,
                        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
                    );
                    builder.ins().brif(is_reference, reference, &[], plain, &[]);

                    builder.switch_to_block(reference);
                    let value =
                        lower_optimizing_reference_array_isset(builder, current, key, transition)?;
                    builder.ins().jump(merge, &[value.into()]);

                    builder.switch_to_block(plain);
                    let value = lower_cached_array_fetch(
                        module,
                        builder,
                        None,
                        None,
                        3,
                        current,
                        key,
                        constant_string_key,
                        unit_identity,
                        result_out,
                        deopt_out,
                        Some(transition),
                    )?;
                    builder.ins().jump(merge, &[value.into()]);

                    builder.switch_to_block(merge);
                    builder.block_params(merge)[0]
                } else {
                    lower_cached_array_fetch(
                        module,
                        builder,
                        None,
                        None,
                        3,
                        current,
                        key,
                        constant_string_key,
                        unit_identity,
                        result_out,
                        deopt_out,
                        Some(transition),
                    )?
                };
                define_region_register(builder, register_variables, registers, *dst, value)?;
                let operation_local_transition = emitted_transition.get();
                return Ok(EmittedOptimizingInstruction {
                    class: if operation_local_transition {
                        crate::JitProductionLoweringClass::BaselineFragmentTransition
                    } else {
                        emitted_class
                    },
                    operation_local_transition,
                });
            }
            let mut value = use_local_variable(builder, locals, *local)?;
            for key in keys {
                let constant_string_key = array_key_is_string_constant(constants, *key);
                let key = lower_array_key_operand(builder, locals, registers, constants, *key)?;
                value = lower_cached_array_fetch(
                    module,
                    builder,
                    None,
                    None,
                    native_dim_operation(1, function, instruction.continuation_id),
                    value,
                    key,
                    constant_string_key,
                    unit_identity,
                    result_out,
                    deopt_out,
                    Some(transition),
                )?;
            }
            let null =
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX));
            let present = builder.ins().icmp_imm(IntCC::Equal, null, 0);
            let result = encode_native_bool(builder, present);
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::EmptyDim { dst, local, keys } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let mut value = use_local_variable(builder, locals, *local)?;
            for key in keys {
                let constant_string_key = array_key_is_string_constant(constants, *key);
                let key = lower_array_key_operand(builder, locals, registers, constants, *key)?;
                value = lower_cached_array_fetch(
                    module,
                    builder,
                    None,
                    None,
                    native_dim_operation(1, function, instruction.continuation_id),
                    value,
                    key,
                    constant_string_key,
                    unit_identity,
                    result_out,
                    deopt_out,
                    Some(transition),
                )?;
            }
            let truthy = lower_optimizing_truthy(builder, value, transition)?;
            let empty = builder.ins().bxor_imm(truthy, 1);
            let result = encode_native_bool(builder, empty);
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::Binary { dst, op, lhs, rhs } => {
            let lhs_operand = *lhs;
            let rhs_operand = *rhs;
            let lhs = lower_region_operand(builder, locals, registers, lhs_operand)?;
            let rhs = lower_region_operand(builder, locals, registers, rhs_operand)?;
            let value = match op {
                RegionBinaryOp::Concat => lower_optimizing_concat(builder, lhs, rhs, transition)?,
                RegionBinaryOp::Div | RegionBinaryOp::Pow => transition.emit_value(builder)?,
                integer => lower_guarded_integer_binary(builder, *integer, lhs, rhs, transition)?,
            };
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::Unary { dst, op, src } => {
            let src_operand = *src;
            let src = lower_region_operand(builder, locals, registers, src_operand)?;
            let fact = lowering_operand_fact(value_flow, constants, src_operand);
            let direct = fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && match op {
                    RegionUnaryOp::Not => matches!(
                        fact.class,
                        SsaValueClass::Null | SsaValueClass::Bool | SsaValueClass::Int
                    ),
                    RegionUnaryOp::Plus | RegionUnaryOp::Minus | RegionUnaryOp::BitNot => {
                        fact.class == SsaValueClass::Int
                    }
                };
            let value = if !direct {
                transition.emit_value(builder)?
            } else {
                match op {
                    RegionUnaryOp::Not => {
                        let truthy = scalar_truthy(builder, src, fact.class)
                            .expect("direct unary truthiness was checked");
                        let inverted = builder.ins().bxor_imm(truthy, 1);
                        encode_native_bool(builder, inverted)
                    }
                    RegionUnaryOp::Plus => src,
                    RegionUnaryOp::BitNot => builder.ins().bnot(src),
                    RegionUnaryOp::Minus => {
                        let rejected = builder.create_block();
                        let accepted = builder.create_block();
                        let minimum = builder.ins().icmp_imm(IntCC::Equal, src, i64::MIN);
                        builder.ins().brif(minimum, rejected, &[], accepted, &[]);
                        builder.switch_to_block(rejected);
                        let _ = transition.emit_value(builder)?;
                        builder.switch_to_block(accepted);
                        builder.ins().ineg(src)
                    }
                }
            };
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::Compare { dst, op, lhs, rhs } => {
            let lhs_value = lower_region_operand(builder, locals, registers, *lhs)?;
            let rhs_value = lower_region_operand(builder, locals, registers, *rhs)?;
            let lhs_fact = lowering_operand_fact(value_flow, constants, *lhs);
            let rhs_fact = lowering_operand_fact(value_flow, constants, *rhs);
            let value = if optimizing_compare_is_direct(*op, lhs_fact, rhs_fact) {
                lower_direct_compare(
                    builder,
                    *op,
                    lhs_value,
                    rhs_value,
                    lhs_fact.class,
                    rhs_fact.class,
                )
                .expect("direct comparison contract was checked")
            } else {
                transition.emit_value(builder)?
            };
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::Cast { dst, op, src } => {
            let value = lower_region_operand(builder, locals, registers, *src)?;
            let fact = lowering_operand_fact(value_flow, constants, *src);
            let value = if optimizing_cast_is_direct(*op, fact) {
                lower_direct_cast(builder, *op, value, fact.class)
                    .expect("direct cast contract was checked")
            } else {
                transition.emit_value(builder)?
            };
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::FetchDim {
            dst,
            array,
            key,
            quiet,
            mode: php_ir::instruction::DimFetchMode::Read,
        } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let array = lower_region_operand(builder, locals, registers, *array)?;
            let constant_string_key = array_key_is_string_constant(constants, *key);
            let key = lower_array_key_operand(builder, locals, registers, constants, *key)?;
            let operation =
                native_dim_operation(u32::from(*quiet), function, instruction.continuation_id);
            let value = lower_cached_array_fetch(
                module,
                builder,
                None,
                None,
                operation,
                array,
                key,
                constant_string_key,
                unit_identity,
                result_out,
                deopt_out,
                Some(transition),
            )?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::ForeachInit { iterator, source } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let source = lower_region_operand(builder, locals, registers, *source)?;
            let iterator_value = lower_direct_foreach_init(
                module,
                builder,
                source,
                result_out,
                deopt_out,
                Some(transition),
                None,
                function,
                instruction.continuation_id,
            )?;
            define_region_register(
                builder,
                register_variables,
                registers,
                *iterator,
                iterator_value,
            )?;
        }
        RegionInstructionKind::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let iterator_value = use_region_register(builder, registers, *iterator)?;
            let (next_key, next_value, has) = lower_direct_arena_foreach_next(
                module,
                builder,
                iterator_value,
                result_out,
                deopt_out,
                Some(transition),
                None,
                None,
            )?;
            define_region_register(builder, register_variables, registers, *has_value, has)?;
            define_region_register(builder, register_variables, registers, *value, next_value)?;
            if let Some(key) = key {
                define_region_register(builder, register_variables, registers, *key, next_key)?;
            }
        }
        RegionInstructionKind::ForeachCleanup { iterator } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let iterator_value = use_region_register(builder, registers, *iterator)?;
            lower_direct_arena_foreach_cleanup(
                module,
                builder,
                iterator_value,
                result_out,
                deopt_out,
                Some(transition),
                None,
                None,
            )?;
        }
        RegionInstructionKind::FetchProperty {
            dst,
            object,
            property,
        } => {
            emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let value = lower_optimizing_property_fetch(builder, object, property, transition)?;
            define_region_register(builder, register_variables, registers, *dst, value)?;
        }
        RegionInstructionKind::NativeCall(call) => {
            emitted_class = crate::JitProductionLoweringClass::BaselineFragmentTransition;
            // The Region builder appends immutable scalar defaults to a
            // statically resolved call's operand vector. Those operands are
            // already the complete prepared binder plan; requiring one source
            // `IrCallArg` per operand sent ordinary omitted-default calls back
            // through the generic runtime dispatcher.
            let fixed_arguments = call.operands.len()
                >= call.argument_operand_offset.saturating_add(call.args.len())
                && call.operands.iter().all(Option::is_some)
                && call.args.iter().all(|argument| {
                    argument.name.is_none()
                        && !argument.unpack
                        && argument.value_kind == php_ir::instruction::IrCallArgValueKind::Direct
                        && argument.by_ref_local.is_none()
                        && argument.by_ref_dim.is_none()
                        && argument.by_ref_property.is_none()
                        && argument.by_ref_property_dim.is_none()
                });
            let argument = |index: usize| {
                fixed_arguments
                    .then(|| {
                        call.operands
                            .get(call.argument_operand_offset.saturating_add(index))
                            .copied()
                            .flatten()
                    })
                    .flatten()
            };
            let inline = call
                .direct_compiled_target()
                .and_then(|target| inline_constants.get(&target).copied())
                .and_then(|inline| bounded_inline_call_operand(call, inline));
            if let Some(operand) = inline {
                emitted_class = crate::JitProductionLoweringClass::DirectClif;
                let value = lower_region_operand(builder, locals, registers, operand)?;
                define_optimizing_call_result(
                    builder,
                    register_variables,
                    registers,
                    call.result,
                    value,
                )?;
            } else if let Some(target) = call.direct_compiled_target()
                && fixed_arguments
                && !call.variadic
                && !call.returns_by_reference
                && !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                && function_params.get(&target).is_some_and(
                    |(_, params, requires_trampoline, arity)| {
                        !*requires_trampoline
                            && *arity == call.operands.len()
                            && params.len()
                                == call
                                    .operands
                                    .len()
                                    .saturating_sub(call.argument_operand_offset)
                            && params
                                .iter()
                                .zip(call.operands.iter().skip(call.argument_operand_offset))
                                .all(|(parameter, operand)| {
                                    !parameter.by_ref
                                        && parameter.type_.as_ref().is_none_or(|type_| {
                                            operand.is_some_and(|operand| {
                                                optimizing_fact_satisfies_type(
                                                    lowering_operand_fact(
                                                        value_flow, constants, operand,
                                                    ),
                                                    type_,
                                                ) || optimizing_type_has_direct_guard(type_)
                                            })
                                        })
                                })
                    },
                )
            {
                emitted_class = crate::JitProductionLoweringClass::CompiledNativeCall;
                let mut call_args = Vec::with_capacity(call.operands.len());
                for operand in &call.operands {
                    let operand = operand.expect("fixed optimizing call has every operand");
                    call_args.push(lower_prepared_native_call_operand(
                        builder, locals, registers, constants, operand,
                    )?);
                }

                // Static SSA facts are an optimization, not an admission
                // requirement.  For an unknown but stable argument, test the
                // native representation in CLIF and keep the ordinary case on
                // the compiled-to-compiled path.  A mismatch performs the one
                // permitted transition to the exact baseline continuation;
                // it never calls the Rust dispatcher from optimized code.
                let (_, parameters, _, _) = function_params
                    .get(&target)
                    .expect("compiled-call target metadata was admitted above");
                let mut arguments_match = None;
                for (index, parameter) in parameters.iter().enumerate() {
                    let Some(type_) = parameter.type_.as_ref() else {
                        continue;
                    };
                    let operand_index = call.argument_operand_offset.saturating_add(index);
                    let operand = call.operands[operand_index]
                        .expect("fixed optimizing call has every visible operand");
                    if optimizing_fact_satisfies_type(
                        lowering_operand_fact(value_flow, constants, operand),
                        type_,
                    ) {
                        continue;
                    }
                    let matched = lower_optimizing_call_argument_type_guard(
                        builder,
                        call_args[operand_index],
                        type_,
                    )
                    .expect("compiled-call parameter type has an admitted native guard");
                    arguments_match = Some(
                        arguments_match
                            .map_or(matched, |accepted| builder.ins().band(accepted, matched)),
                    );
                }
                if let Some(arguments_match) = arguments_match {
                    let admitted = builder.create_block();
                    let rejected = builder.create_block();
                    builder
                        .ins()
                        .brif(arguments_match, admitted, &[], rejected, &[]);
                    builder.switch_to_block(rejected);
                    let _ = transition.emit_value(builder)?;
                    builder.ins().jump(admitted, &[]);
                    builder.switch_to_block(admitted);
                }

                let pointer_type = module.target_config().pointer_type();
                let runtime_view_offset =
                    std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
                let baseline_entries = builder.ins().load(
                    pointer_type,
                    MemFlagsData::new(),
                    deopt_out,
                    runtime_view_offset
                        + std::mem::offset_of!(
                            crate::JitNativeRuntimeView,
                            trusted_function_entries,
                        ) as i32,
                );
                let optimizing_entries = builder.ins().load(
                    pointer_type,
                    MemFlagsData::new(),
                    deopt_out,
                    runtime_view_offset
                        + std::mem::offset_of!(
                            crate::JitNativeRuntimeView,
                            trusted_optimizing_function_entries,
                        ) as i32,
                );
                let entry_offset =
                    i64::try_from(target.index().saturating_mul(pointer_type.bytes() as usize))
                        .unwrap_or(i64::MAX);
                let baseline_entry = builder.ins().iadd_imm(baseline_entries, entry_offset);
                let optimizing_entry = builder.ins().iadd_imm(optimizing_entries, entry_offset);
                let baseline_address =
                    builder
                        .ins()
                        .atomic_load(pointer_type, MemFlagsData::new(), baseline_entry);
                let optimizing_address =
                    builder
                        .ins()
                        .atomic_load(pointer_type, MemFlagsData::new(), optimizing_entry);
                let has_optimizing = builder
                    .ins()
                    .icmp_imm(IntCC::NotEqual, optimizing_address, 0);
                let address =
                    builder
                        .ins()
                        .select(has_optimizing, optimizing_address, baseline_address);
                let invoke = builder.create_block();
                let unavailable = builder.create_block();
                let published = builder.ins().icmp_imm(IntCC::NotEqual, address, 0);
                builder.ins().brif(published, invoke, &[], unavailable, &[]);

                builder.switch_to_block(unavailable);
                let _ = transition.emit_value(builder)?;
                builder.ins().jump(invoke, &[]);

                builder.switch_to_block(invoke);
                for (index, value) in call_args.iter().copied().enumerate() {
                    if !value_flow.moves_value_into_call(instruction.continuation_id, index) {
                        lower_optimizing_retain(builder, value, deopt_out);
                    }
                }
                let packed_size =
                    u32::try_from(call_args.len().max(1).saturating_mul(8)).map_err(|_| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_NATIVE_CALL_ARITY",
                            "optimizing direct-call arguments exceed stack storage",
                        )
                    })?;
                let arguments =
                    allocate_native_stack_storage(builder, pointer_type, packed_size, 3);
                for (index, value) in call_args.iter().copied().enumerate() {
                    builder.ins().store(
                        MemFlagsData::new(),
                        value,
                        arguments,
                        i32::try_from(index.saturating_mul(8)).unwrap_or(i32::MAX),
                    );
                }
                let result_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let callee_result_out = builder.ins().stack_addr(pointer_type, result_slot, 0);
                let resume_id = builder.ins().iconst(types::I32, -1);
                let resume_state = builder.ins().iconst(pointer_type, 0);
                let signature = builder.import_signature(native_php_entry_signature(module));
                let native_call = builder.ins().call_indirect(
                    signature,
                    address,
                    &[
                        runtime,
                        arguments,
                        callee_result_out,
                        deopt_out,
                        resume_id,
                        resume_state,
                    ],
                );
                let status = builder.inst_results(native_call)[0];
                let returned = builder.create_block();
                let inspect_status = builder.create_block();
                let resume_callee = builder.create_block();
                let propagate = builder.create_block();
                builder.append_block_param(propagate, types::I32);
                let is_return = builder.ins().icmp_imm(
                    IntCC::Equal,
                    status,
                    i64::from(crate::JitCallStatus::RETURN.0),
                );
                builder
                    .ins()
                    .brif(is_return, returned, &[], inspect_status, &[]);

                builder.switch_to_block(inspect_status);
                let is_transition = builder.ins().icmp_imm(
                    IntCC::Equal,
                    status,
                    i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
                );
                builder.ins().brif(
                    is_transition,
                    resume_callee,
                    &[],
                    propagate,
                    &[status.into()],
                );

                builder.switch_to_block(resume_callee);
                let rejected_function = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    deopt_out,
                    std::mem::offset_of!(crate::JitDeoptState, function_id) as i32,
                );
                let rejected_function = builder.ins().uextend(pointer_type, rejected_function);
                let rejected_offset = builder
                    .ins()
                    .imul_imm(rejected_function, i64::from(pointer_type.bytes()));
                let rejected_entry = builder.ins().iadd(baseline_entries, rejected_offset);
                let rejected_address =
                    builder
                        .ins()
                        .atomic_load(pointer_type, MemFlagsData::new(), rejected_entry);
                let rejected_continuation = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    deopt_out,
                    std::mem::offset_of!(crate::JitDeoptState, continuation_id) as i32,
                );
                let rejected_resume_id = builder.ins().bor_imm(
                    rejected_continuation,
                    i64::from(crate::JIT_NATIVE_TRANSITION_RESUME_TAG),
                );
                let resumed = builder.ins().call_indirect(
                    signature,
                    rejected_address,
                    &[
                        runtime,
                        arguments,
                        callee_result_out,
                        deopt_out,
                        rejected_resume_id,
                        deopt_out,
                    ],
                );
                let resumed_status = builder.inst_results(resumed)[0];
                let resumed_return = builder.ins().icmp_imm(
                    IntCC::Equal,
                    resumed_status,
                    i64::from(crate::JitCallStatus::RETURN.0),
                );
                builder.ins().brif(
                    resumed_return,
                    returned,
                    &[],
                    propagate,
                    &[resumed_status.into()],
                );

                builder.switch_to_block(propagate);
                let propagated_status = builder.block_params(propagate)[0];
                let control = builder.ins().stack_load(types::I64, result_slot, 0);
                builder
                    .ins()
                    .store(MemFlagsData::new(), control, result_out, 0);
                builder.ins().return_(&[propagated_status]);

                builder.switch_to_block(returned);
                let result = builder.ins().stack_load(types::I64, result_slot, 0);
                match call.result {
                    RegionCallResult::Register(destination) => define_region_register(
                        builder,
                        register_variables,
                        registers,
                        destination,
                        result,
                    )?,
                    RegionCallResult::Discard => {
                        lower_optimizing_release(builder, result, transition)?;
                    }
                    RegionCallResult::ReferenceLocal(_) => unreachable!("filtered above"),
                }
            } else if matches!(
                &call.target,
                RegionCallTarget::Semantic {
                    operation: crate::region_ir::RegionSemanticOp::BindGlobal { .. }
                }
            ) {
                emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
                lower_optimizing_cached_bind_global(
                    builder,
                    locals,
                    call,
                    instruction,
                    transition,
                    unit_identity,
                    function,
                )?;
            } else if let Some(operation) = stable_builtin_type_predicate(&call.target)
                && let Some(operand) = argument(0)
            {
                emitted_class = crate::JitProductionLoweringClass::DirectClif;
                let value = lower_region_operand(builder, locals, registers, operand)?;
                let result =
                    lower_optimizing_type_predicate(builder, operation, value, transition)?;
                define_optimizing_call_result(
                    builder,
                    register_variables,
                    registers,
                    call.result,
                    result,
                )?;
            } else if let Some(operation) = stable_builtin_length(&call.target)
                && let Some(operand) = argument(0)
            {
                emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
                let value = lower_region_operand(builder, locals, registers, operand)?;
                let result = lower_optimizing_length(builder, operation, value, transition)?;
                define_optimizing_call_result(
                    builder,
                    register_variables,
                    registers,
                    call.result,
                    result,
                )?;
            } else if let Some(operation) = stable_builtin_string_predicate(&call.target)
                && let (Some(haystack), Some(needle)) = (argument(0), argument(1))
            {
                emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
                let haystack = lower_region_operand(builder, locals, registers, haystack)?;
                let needle = lower_region_operand(builder, locals, registers, needle)?;
                let result = lower_optimizing_string_predicate(
                    builder, operation, haystack, needle, transition,
                )?;
                define_optimizing_call_result(
                    builder,
                    register_variables,
                    registers,
                    call.result,
                    result,
                )?;
            } else if let Some(operation) = stable_builtin_ascii_case(&call.target)
                && call.argument_operand_offset == 0
                && call.operands.len() == 1
                && call.args.len() == 1
                && let Some(value) = argument(0)
            {
                emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
                let value = lower_region_operand(builder, locals, registers, value)?;
                let result = lower_optimizing_ascii_case(builder, operation, value, transition)?;
                define_optimizing_call_result(
                    builder,
                    register_variables,
                    registers,
                    call.result,
                    result,
                )?;
            } else if stable_builtin_array_key_exists(&call.target)
                && let (Some(key), Some(array)) = (argument(0), argument(1))
            {
                emitted_class = crate::JitProductionLoweringClass::DirectNativeData;
                let constant_string_key = array_key_is_string_constant(constants, key);
                let key = lower_array_key_operand(builder, locals, registers, constants, key)?;
                let array = lower_region_operand(builder, locals, registers, array)?;
                let result = lower_cached_array_fetch(
                    module,
                    builder,
                    None,
                    None,
                    2,
                    array,
                    key,
                    constant_string_key,
                    unit_identity,
                    result_out,
                    deopt_out,
                    Some(transition),
                )?;
                define_optimizing_call_result(
                    builder,
                    register_variables,
                    registers,
                    call.result,
                    result,
                )?;
            } else {
                let placeholder = transition.emit_value(builder)?;
                for register in instruction.register_definitions() {
                    define_region_register(
                        builder,
                        register_variables,
                        registers,
                        register,
                        placeholder,
                    )?;
                }
            }
        }
        _ => {
            emitted_class = crate::JitProductionLoweringClass::BaselineFragmentTransition;
            let placeholder = transition.emit_value(builder)?;
            for register in instruction.register_definitions() {
                define_region_register(
                    builder,
                    register_variables,
                    registers,
                    register,
                    placeholder,
                )?;
            }
        }
    }
    let operation_local_transition = emitted_transition.get()
        && emitted_class != crate::JitProductionLoweringClass::BaselineFragmentTransition;
    if emitted_transition.get() {
        emitted_class = crate::JitProductionLoweringClass::BaselineFragmentTransition;
    }
    Ok(EmittedOptimizingInstruction {
        class: emitted_class,
        operation_local_transition,
    })
}

fn lower_baseline_region_instruction(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    functions: &BTreeMap<FunctionId, FuncId>,
    inline_constants: &BTreeMap<FunctionId, BoundedInlineValue>,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
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
    unit_identity: u64,
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
            let cl_value = if value_copy_requires_retain(fact)
                && !value_flow.moves_value_into_register(instruction.continuation_id)
            {
                lower_guarded_value_release(
                    module,
                    builder,
                    native_operations.value_release,
                    native_dim_operation(0, function, instruction.continuation_id),
                    cl_value,
                    result_out,
                    deopt_out,
                )?
            } else {
                cl_value
            };
            define_region_register(builder, register_variables, registers, *dst, cl_value)?;
        }
        RegionInstructionKind::LoadLocal { dst, local, quiet } => {
            let value = use_local_variable(builder, locals, *local)?;
            let fact = value_flow.local_fact(*local);
            let direct = !function_is_top_level
                && value_flow.local_storage(*local).is_promoted()
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
                    native_operations.value_release,
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
                    native_operations.value_release,
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
            let direct = !function_is_top_level
                && value_flow.local_storage(*local).is_promoted()
                && fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && !matches!(
                    fact.class,
                    SsaValueClass::ReferenceHandle | SsaValueClass::MixedHandle
                );
            let cl_value = if direct {
                let stored = if value_copy_requires_retain(fact)
                    && !value_flow.moves_value_into_local(instruction.continuation_id)
                {
                    lower_guarded_value_release(
                        module,
                        builder,
                        native_operations.value_release,
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
                    let _ = lower_guarded_value_release(
                        module,
                        builder,
                        native_operations.value_release,
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
                    native_operations.value_release,
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
            let direct = !function_is_top_level
                && value_flow.local_storage(*local).is_promoted()
                && !value_copy_requires_retain(fact);
            let stored = if direct {
                value
            } else if value_flow.local_storage(*local).is_native_frame_local() {
                lower_guarded_native_local_store(
                    module,
                    builder,
                    native_operations.local_store,
                    native_operations.value_release,
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
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
            publish_native_register_state(
                builder,
                deopt_out,
                registers,
                transition_live_registers,
            )?;
            let source_value = use_local_variable(builder, locals, *source)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
                let _ = lower_guarded_value_release(
                    module,
                    builder,
                    native_operations.value_release,
                    native_dim_operation(1, function, instruction.continuation_id),
                    value,
                    result_out,
                    deopt_out,
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
                    deopt_out,
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
            if let Some(operation) = stable_builtin_string_predicate(&call.target)
                && call.argument_operand_offset == 0
                && call.args.len() == 2
                && call
                    .args
                    .iter()
                    .all(|argument| argument.name.is_none() && !argument.unpack)
                && call.operands.len() == 2
                && !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                && let (Some(haystack_operand), Some(needle_operand)) =
                    (call.operands[0], call.operands[1])
            {
                let haystack = lower_region_operand(builder, locals, registers, haystack_operand)?;
                let needle = lower_region_operand(builder, locals, registers, needle_operand)?;
                let consume_haystack = matches!(
                    haystack_operand,
                    RegionOperand::Register(register)
                        if value_flow.register_fact(register).ownership != SsaOwnership::Borrowed
                ) && native_argument_has_location(&call.args[0]);
                let consume_needle = matches!(
                    needle_operand,
                    RegionOperand::Register(register)
                        if value_flow.register_fact(register).ownership != SsaOwnership::Borrowed
                ) && native_argument_has_location(&call.args[1]);
                let operation = operation
                    | if consume_haystack { 1 << 8 } else { 0 }
                    | if consume_needle { 1 << 9 } else { 0 };
                let (status, value) = lower_fast_string_predicate(
                    module,
                    builder,
                    native_operations.string_predicate,
                    operation,
                    haystack,
                    needle,
                    result_out,
                )?;
                let fast = builder.create_block();
                let slow = builder.create_block();
                let merge = builder.create_block();
                let result_register = match call.result {
                    RegionCallResult::Register(destination) => {
                        builder.append_block_param(merge, types::I64);
                        Some(destination)
                    }
                    RegionCallResult::Discard => None,
                    RegionCallResult::ReferenceLocal(_) => unreachable!("filtered above"),
                };
                let hit = builder.ins().icmp_imm(IntCC::Equal, status, 0);
                builder.ins().brif(hit, fast, &[], slow, &[]);

                builder.switch_to_block(fast);
                if result_register.is_some() {
                    builder.ins().jump(merge, &[value.into()]);
                } else {
                    builder.ins().jump(merge, &[]);
                }

                builder.switch_to_block(slow);
                lower_native_call_trampoline(
                    module,
                    builder,
                    native_call_helper,
                    native_operations.reference_bind,
                    native_operations.value_release,
                    value_flow,
                    locals,
                    register_variables,
                    registers,
                    function_params,
                    external_function_signatures,
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
                if let Some(destination) = result_register {
                    let slow_value = use_region_register(builder, registers, destination)?;
                    builder.ins().jump(merge, &[slow_value.into()]);
                } else {
                    builder.ins().jump(merge, &[]);
                }
                builder.switch_to_block(merge);
                if let Some(destination) = result_register {
                    let merged = builder.block_params(merge)[0];
                    define_region_register(
                        builder,
                        register_variables,
                        registers,
                        destination,
                        merged,
                    )?;
                }
                return Ok(());
            }
            if stable_builtin_array_key_exists(&call.target)
                && call.argument_operand_offset == 0
                && call.args.len() == 2
                && call
                    .args
                    .iter()
                    .all(|argument| argument.name.is_none() && !argument.unpack)
                && call.operands.len() == 2
                && !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                && let (Some(key_operand), Some(array_operand)) =
                    (call.operands[0], call.operands[1])
            {
                let key = lower_region_operand(builder, locals, registers, key_operand)?;
                let array = lower_region_operand(builder, locals, registers, array_operand)?;
                let (status, value) = lower_fast_array_key_exists(
                    module,
                    builder,
                    native_operations.array_fetch,
                    array,
                    key,
                    result_out,
                )?;
                let fast = builder.create_block();
                let slow = builder.create_block();
                let merge = builder.create_block();
                let result_register = match call.result {
                    RegionCallResult::Register(destination) => {
                        builder.append_block_param(merge, types::I64);
                        Some(destination)
                    }
                    RegionCallResult::Discard => None,
                    RegionCallResult::ReferenceLocal(_) => unreachable!("filtered above"),
                };
                let hit = builder.ins().icmp_imm(IntCC::Equal, status, 0);
                builder.ins().brif(hit, fast, &[], slow, &[]);

                builder.switch_to_block(fast);
                for (argument, (operand, source)) in call
                    .args
                    .iter()
                    .zip([(key_operand, key), (array_operand, array)])
                {
                    if matches!(
                        operand,
                        RegionOperand::Register(register)
                            if value_flow.register_fact(register).ownership
                                != SsaOwnership::Borrowed
                    ) && native_argument_has_location(argument)
                    {
                        let _ = lower_guarded_value_release(
                            module,
                            builder,
                            native_operations.value_release,
                            native_dim_operation(1, function, instruction.continuation_id),
                            source,
                            result_out,
                            deopt_out,
                        )?;
                    }
                }
                if result_register.is_some() {
                    builder.ins().jump(merge, &[value.into()]);
                } else {
                    builder.ins().jump(merge, &[]);
                }

                builder.switch_to_block(slow);
                lower_native_call_trampoline(
                    module,
                    builder,
                    native_call_helper,
                    native_operations.reference_bind,
                    native_operations.value_release,
                    value_flow,
                    locals,
                    register_variables,
                    registers,
                    function_params,
                    external_function_signatures,
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
                if let Some(destination) = result_register {
                    let slow_value = use_region_register(builder, registers, destination)?;
                    builder.ins().jump(merge, &[slow_value.into()]);
                } else {
                    builder.ins().jump(merge, &[]);
                }
                builder.switch_to_block(merge);
                if let Some(destination) = result_register {
                    let merged = builder.block_params(merge)[0];
                    define_region_register(
                        builder,
                        register_variables,
                        registers,
                        destination,
                        merged,
                    )?;
                }
                return Ok(());
            }
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
                    let _ = lower_guarded_value_release(
                        module,
                        builder,
                        native_operations.value_release,
                        native_dim_operation(1, function, instruction.continuation_id),
                        source,
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
                    let _ = lower_guarded_value_release(
                        module,
                        builder,
                        native_operations.value_release,
                        native_dim_operation(1, function, instruction.continuation_id),
                        source,
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
            if let Some(builtin_id) = stable_builtin_dense_id(&call.target)
                && call.argument_operand_offset == 0
                && call.operands.len() == call.args.len()
                && call.args.iter().enumerate().all(|(index, argument)| {
                    argument.name.is_none()
                        && !argument.unpack
                        && !call.argument_requires_reference_binding(index)
                })
                && !call.returns_by_reference
                && !matches!(call.result, RegionCallResult::ReferenceLocal(_))
            {
                lower_direct_builtin_call(
                    module,
                    builder,
                    native_operations.builtin_dispatch,
                    native_operations.value_release,
                    value_flow,
                    locals,
                    register_variables,
                    registers,
                    call,
                    builtin_id,
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
            }
            if let RegionCallTarget::Semantic { operation } = &call.target {
                if matches!(
                    operation,
                    crate::region_ir::RegionSemanticOp::BindGlobal { .. }
                ) {
                    lower_cached_bind_global(
                        module,
                        builder,
                        native_operations.semantic_dispatch,
                        native_operations.value_release,
                        locals,
                        register_variables,
                        registers,
                        call,
                        instruction,
                        transition_live_registers,
                        streaming_call_exit,
                        result_out,
                        deopt_out,
                        function,
                        local_count,
                        native_version,
                        unit_identity,
                        pointer_type,
                    )?;
                    return Ok(());
                }
                lower_direct_semantic_call(
                    module,
                    builder,
                    native_operations.semantic_dispatch,
                    locals,
                    register_variables,
                    registers,
                    call,
                    operation.operation_id(),
                    instruction,
                    transition_live_registers,
                    streaming_call_exit,
                    result_out,
                    deopt_out,
                    function,
                    local_count,
                    native_version,
                    unit_identity,
                    pointer_type,
                )?;
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
                    value = lower_guarded_value_release(
                        module,
                        builder,
                        native_operations.value_release,
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
                    native_operations.value_release,
                    value_flow,
                    locals,
                    register_variables,
                    registers,
                    function_params,
                    external_function_signatures,
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
                value = lower_guarded_value_release(
                    module,
                    builder,
                    native_operations.value_release,
                    native_dim_operation(0, function, instruction.continuation_id),
                    value,
                    result_out,
                    deopt_out,
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
                native_operations
                    .runtime
                    .expect("native call must carry request fast state"),
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
            let (call, baseline_entries) = match callee {
                NativeDirectCallee::Local(callee) => {
                    let callee_ref = module.declare_func_in_func(callee, builder.func);
                    (builder.ins().call(callee_ref, &callee_call_args), None)
                }
                NativeDirectCallee::Resolved(target) => {
                    let helper = native_operations.function_resolve.ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_NATIVE_FUNCTION_RESOLVER",
                            "statically known callee has no compile-on-demand resolver",
                        )
                    })?;
                    let resolve = builder.create_block();
                    let address_ready = builder.create_block();
                    builder.append_block_param(address_ready, pointer_type);

                    let runtime_view_offset =
                        std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
                    let baseline_entries = builder.ins().load(
                        pointer_type,
                        MemFlagsData::new(),
                        deopt_out,
                        runtime_view_offset
                            + std::mem::offset_of!(
                                crate::JitNativeRuntimeView,
                                trusted_function_entries,
                            ) as i32,
                    );
                    let entry_offset =
                        i64::try_from(target.index().saturating_mul(pointer_type.bytes() as usize))
                            .unwrap_or(i64::MAX);
                    let entry = builder.ins().iadd_imm(baseline_entries, entry_offset);
                    // A baseline continuation is the stable target of an
                    // optimizing guard exit. Re-entering optimizing callees
                    // from that continuation lets a nested side exit escape
                    // past its native callers and loses the PHP call chain.
                    // Stay baseline-native until this PHP frame returns.
                    let address =
                        builder
                            .ins()
                            .atomic_load(pointer_type, MemFlagsData::new(), entry);
                    let published = builder.ins().icmp_imm(IntCC::NotEqual, address, 0);
                    builder
                        .ins()
                        .brif(published, address_ready, &[address.into()], resolve, &[]);

                    builder.switch_to_block(resolve);
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
                    builder.ins().jump(address_ready, &[address.into()]);

                    builder.switch_to_block(address_ready);
                    let address = builder.block_params(address_ready)[0];
                    let signature = native_php_entry_signature(module);
                    let signature = builder.import_signature(signature);
                    (
                        builder
                            .ins()
                            .call_indirect(signature, address, &callee_call_args),
                        Some((baseline_entries, signature)),
                    )
                }
            };
            let mut status = builder.inst_results(call)[0];
            if let Some((baseline_entries, signature)) = baseline_entries {
                let resume_callee = builder.create_block();
                let status_ready = builder.create_block();
                builder.append_block_param(status_ready, types::I32);
                let is_transition = builder.ins().icmp_imm(
                    IntCC::Equal,
                    status,
                    i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
                );
                builder.ins().brif(
                    is_transition,
                    resume_callee,
                    &[],
                    status_ready,
                    &[status.into()],
                );

                builder.switch_to_block(resume_callee);
                let rejected_function = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    deopt_out,
                    std::mem::offset_of!(crate::JitDeoptState, function_id) as i32,
                );
                let rejected_function = builder.ins().uextend(pointer_type, rejected_function);
                let rejected_offset = builder
                    .ins()
                    .imul_imm(rejected_function, i64::from(pointer_type.bytes()));
                let rejected_entry = builder.ins().iadd(baseline_entries, rejected_offset);
                let rejected_address =
                    builder
                        .ins()
                        .atomic_load(pointer_type, MemFlagsData::new(), rejected_entry);
                let rejected_continuation = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    deopt_out,
                    std::mem::offset_of!(crate::JitDeoptState, continuation_id) as i32,
                );
                let rejected_resume_id = builder.ins().bor_imm(
                    rejected_continuation,
                    i64::from(crate::JIT_NATIVE_TRANSITION_RESUME_TAG),
                );
                let resumed = builder.ins().call_indirect(
                    signature,
                    rejected_address,
                    &[
                        native_operations
                            .runtime
                            .expect("native call must carry request fast state"),
                        arguments,
                        callee_result_out,
                        deopt_out,
                        rejected_resume_id,
                        deopt_out,
                    ],
                );
                let resumed_status = builder.inst_results(resumed)[0];
                let nested_transition = builder.ins().icmp_imm(
                    IntCC::Equal,
                    resumed_status,
                    i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
                );
                builder.ins().brif(
                    nested_transition,
                    resume_callee,
                    &[],
                    status_ready,
                    &[resumed_status.into()],
                );

                builder.switch_to_block(status_ready);
                status = builder.block_params(status_ready)[0];
            }
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
                let _ = lower_guarded_value_release(
                    module,
                    builder,
                    native_operations.value_release,
                    native_dim_operation(1, function, instruction.continuation_id),
                    *argument,
                    result_out,
                    deopt_out,
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
                let _ = lower_guarded_value_release(
                    module,
                    builder,
                    native_operations.value_release,
                    native_dim_operation(0, function, instruction.continuation_id),
                    value,
                    result_out,
                    deopt_out,
                )?;
                builder.ins().jump(release_arguments, &[]);
                builder.switch_to_block(release_arguments);
            }
            for argument in &released_call_arguments {
                let _ = lower_guarded_value_release(
                    module,
                    builder,
                    native_operations.value_release,
                    native_dim_operation(1, function, instruction.continuation_id),
                    *argument,
                    result_out,
                    deopt_out,
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
                        deopt_out,
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
                        deopt_out,
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
            let value = lower_direct_new_array(
                module,
                builder,
                native_operations.array_new,
                result_out,
                deopt_out,
                None,
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
        RegionInstructionKind::FetchProperty { dst, object, .. } => {
            publish_native_call_state(
                builder,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            publish_native_register_state(
                builder,
                deopt_out,
                registers,
                transition_live_registers,
            )?;
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
            publish_native_register_state(
                builder,
                deopt_out,
                registers,
                transition_live_registers,
            )?;
            let class_name = lower_region_operand(builder, locals, registers, *class_name)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
            publish_native_register_state(
                builder,
                deopt_out,
                registers,
                transition_live_registers,
            )?;
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let function_value = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
        RegionInstructionKind::AssignProperty {
            dst, object, value, ..
        } => {
            publish_native_call_state(
                builder,
                deopt_out,
                function,
                local_count,
                instruction,
                locals,
                native_version,
            )?;
            publish_native_register_state(
                builder,
                deopt_out,
                registers,
                transition_live_registers,
            )?;
            let object = lower_region_operand(builder, locals, registers, *object)?;
            let value_operand = *value;
            let value = lower_region_operand(builder, locals, registers, value_operand)?;
            let move_value = matches!(
                lowering_operand_fact(value_flow, constants, value_operand).ownership,
                SsaOwnership::Owned | SsaOwnership::Moved
            );
            let function = builder.ins().iconst(types::I64, i64::from(function.raw()));
            let instruction_id = builder
                .ins()
                .iconst(types::I64, i64::from(instruction.continuation_id));
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
                Some(key) => lower_array_key_operand(builder, locals, registers, constants, *key)?,
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
            let operation =
                native_dim_operation(u32::from(append), function, instruction.continuation_id);
            let updated = if append && by_ref_local.is_none() {
                lower_direct_array_append(
                    module,
                    builder,
                    array_value,
                    None,
                    value,
                    result_out,
                    deopt_out,
                    NativeArrayAppendFallback::Baseline {
                        helper: native_operations.array_insert,
                        lifecycle: native_operations.value_release,
                        operation,
                        function,
                        local_count,
                        instruction,
                        locals,
                        native_version,
                    },
                )?
            } else if !append && by_ref_local.is_none() {
                lower_direct_array_insert(
                    module,
                    builder,
                    array_value,
                    key,
                    value,
                    result_out,
                    deopt_out,
                    NativeArrayAppendFallback::Baseline {
                        helper: native_operations.array_insert,
                        lifecycle: native_operations.value_release,
                        operation,
                        function,
                        local_count,
                        instruction,
                        locals,
                        native_version,
                    },
                )?
            } else {
                lower_native_value_operation_with_state(
                    module,
                    builder,
                    native_operations.array_insert,
                    operation,
                    &[array_value, key, value],
                    result_out,
                    deopt_out,
                    function,
                    local_count,
                    instruction,
                    locals,
                    native_version,
                )?
            };
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
            mode,
        } => {
            // The dimension helper borrows and dereferences its target. Passing
            // the encoded local directly avoids manufacturing an owned copy
            // solely to release it again after the helper returns.
            let (array, release_array) = if let RegionOperand::Local(local) = array {
                let current = use_local_variable(builder, locals, *local)?;
                if ordinary_local_fast_path(function_is_top_level, function_local_names, *local) {
                    (current, false)
                } else {
                    (
                        lower_native_local_fetch(
                            module,
                            builder,
                            native_operations.local_fetch,
                            current,
                            *quiet,
                            false,
                            function,
                            *local,
                            instruction.span,
                            result_out,
                        )?,
                        true,
                    )
                }
            } else {
                (
                    lower_region_operand(builder, locals, registers, *array)?,
                    false,
                )
            };
            let key = lower_array_key_operand(builder, locals, registers, constants, *key)?;
            let operation =
                native_dim_operation(u32::from(*quiet), function, instruction.continuation_id);
            let value = if *mode == php_ir::instruction::DimFetchMode::Read {
                lower_cached_array_fetch(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_operations.value_release,
                    operation,
                    array,
                    key,
                    false,
                    unit_identity,
                    result_out,
                    deopt_out,
                    None,
                )?
            } else {
                lower_native_value_operation(
                    module,
                    builder,
                    native_operations.array_fetch,
                    operation,
                    &[array, key],
                    result_out,
                )?
            };
            if release_array {
                let _ = lower_guarded_value_release(
                    module,
                    builder,
                    native_operations.value_release,
                    native_dim_operation(1, function, instruction.continuation_id),
                    array,
                    result_out,
                    deopt_out,
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
            publish_native_register_state(
                builder,
                deopt_out,
                registers,
                transition_live_registers,
            )?;
            let current = use_local_variable(builder, locals, *local)?;
            let local_fact = value_flow.local_fact(*local);
            let direct_array_local = value_flow.local_storage(*local).is_promoted()
                && local_fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && local_fact.class == SsaValueClass::ArrayHandle;
            let local_array_write =
                ordinary_local_fast_path(function_is_top_level, function_local_names, *local);
            let root = if direct_array_local || local_array_write {
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
            if keys.len() == 1 && (direct_array_local || local_array_write) {
                let key = lower_array_key_operand(builder, locals, registers, constants, keys[0])?;
                let value = lower_region_operand(builder, locals, registers, *value)?;
                let operation = native_dim_operation(0, function, instruction.continuation_id);
                let updated = lower_direct_array_insert(
                    module,
                    builder,
                    root,
                    key,
                    value,
                    result_out,
                    deopt_out,
                    NativeArrayAppendFallback::Baseline {
                        helper: if local_array_write {
                            native_operations.array_insert_local
                        } else {
                            native_operations.array_insert
                        },
                        lifecycle: native_operations.value_release,
                        operation,
                        function,
                        local_count,
                        instruction,
                        locals,
                        native_version,
                    },
                )?;
                define_local_variable(builder, locals, *local, updated)?;
                define_region_register(builder, register_variables, registers, *dst, value)?;
                return Ok(());
            }
            let keys = keys
                .iter()
                .map(|key| lower_array_key_operand(builder, locals, registers, constants, *key))
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
                if local_array_write && keys.len() == 1 {
                    native_operations.array_insert_local
                } else {
                    native_operations.array_insert
                },
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
                    if local_array_write && index == 0 {
                        native_operations.array_insert_local
                    } else {
                        native_operations.array_insert
                    },
                    array_insert_operation,
                    &[arrays[index], keys[index], updated],
                    result_out,
                )?;
            }
            let stored = if direct_array_local || local_array_write {
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
            let local_array_write =
                ordinary_local_fast_path(function_is_top_level, function_local_names, *local);
            let root = if direct_array_local || local_array_write {
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
            if keys.is_empty() && (direct_array_local || local_array_write) {
                let value = lower_region_operand(builder, locals, registers, *value)?;
                let operation = native_dim_operation(1, function, instruction.continuation_id);
                let updated = lower_direct_array_append(
                    module,
                    builder,
                    root,
                    None,
                    value,
                    result_out,
                    deopt_out,
                    NativeArrayAppendFallback::Baseline {
                        helper: if local_array_write {
                            native_operations.array_insert_local
                        } else {
                            native_operations.array_insert
                        },
                        lifecycle: native_operations.value_release,
                        operation,
                        function,
                        local_count,
                        instruction,
                        locals,
                        native_version,
                    },
                )?;
                define_local_variable(builder, locals, *local, updated)?;
                define_region_register(builder, register_variables, registers, *dst, value)?;
                return Ok(());
            }
            let keys = keys
                .iter()
                .map(|key| lower_array_key_operand(builder, locals, registers, constants, *key))
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
                if local_array_write && keys.is_empty() {
                    native_operations.array_insert_local
                } else {
                    native_operations.array_insert
                },
                native_dim_operation(1, function, instruction.continuation_id),
                &[nested, key, value],
                result_out,
            )?;
            for index in (0..keys.len()).rev() {
                updated = lower_native_value_operation(
                    module,
                    builder,
                    if local_array_write && index == 0 {
                        native_operations.array_insert_local
                    } else {
                        native_operations.array_insert
                    },
                    native_dim_operation(0, function, instruction.continuation_id),
                    &[arrays[index], keys[index], updated],
                    result_out,
                )?;
            }
            let stored = if direct_array_local || local_array_write {
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
            if !ordinary_local_fast_path(function_is_top_level, function_local_names, *local) {
                value = lower_native_local_fetch(
                    module,
                    builder,
                    native_operations.local_fetch,
                    value,
                    true,
                    false,
                    function,
                    *local,
                    instruction.span,
                    result_out,
                )?;
            }
            for key in keys {
                let key = lower_array_key_operand(builder, locals, registers, constants, *key)?;
                value = lower_cached_array_fetch(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_operations.value_release,
                    native_dim_operation(1, function, instruction.continuation_id),
                    value,
                    key,
                    false,
                    unit_identity,
                    result_out,
                    deopt_out,
                    None,
                )?;
            }
            let result = lower_guarded_isset_value(
                module,
                builder,
                native_operations.compare,
                value,
                result_out,
            )?;
            define_region_register(builder, register_variables, registers, *dst, result)?;
        }
        RegionInstructionKind::EmptyDim { dst, local, keys } => {
            let mut value = use_local_variable(builder, locals, *local)?;
            if !ordinary_local_fast_path(function_is_top_level, function_local_names, *local) {
                value = lower_native_local_fetch(
                    module,
                    builder,
                    native_operations.local_fetch,
                    value,
                    true,
                    false,
                    function,
                    *local,
                    instruction.span,
                    result_out,
                )?;
            }
            for key in keys {
                let key = lower_array_key_operand(builder, locals, registers, constants, *key)?;
                value = lower_cached_array_fetch(
                    module,
                    builder,
                    native_operations.array_fetch,
                    native_operations.value_release,
                    native_dim_operation(1, function, instruction.continuation_id),
                    value,
                    key,
                    false,
                    unit_identity,
                    result_out,
                    deopt_out,
                    None,
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
                native_operations.value_release,
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
                native_operations.value_release,
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
            let _ = lower_guarded_value_release(
                module,
                builder,
                native_operations.value_release,
                native_dim_operation(1, function, instruction.continuation_id),
                current,
                result_out,
                deopt_out,
            )?;
        }
        RegionInstructionKind::ForeachInit { iterator, source } => {
            let source = lower_region_operand(builder, locals, registers, *source)?;
            let value = lower_direct_foreach_init(
                module,
                builder,
                source,
                result_out,
                deopt_out,
                None,
                native_operations.foreach_init,
                function,
                instruction.continuation_id,
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
            let iterator_value = lower_region_operand(
                builder,
                locals,
                registers,
                RegionOperand::Register(*iterator),
            )?;
            let (next_key, next_value, has) = lower_direct_arena_foreach_next(
                module,
                builder,
                iterator_value,
                result_out,
                deopt_out,
                None,
                native_operations.foreach_next,
                native_operations.value_release,
            )?;
            define_region_register(builder, register_variables, registers, *has_value, has)?;
            define_region_register(builder, register_variables, registers, *value, next_value)?;
            if let Some(key) = key {
                define_region_register(builder, register_variables, registers, *key, next_key)?;
            }
        }
        RegionInstructionKind::ForeachCleanup { iterator } => {
            let iterator = lower_region_operand(
                builder,
                locals,
                registers,
                RegionOperand::Register(*iterator),
            )?;
            lower_direct_arena_foreach_cleanup(
                module,
                builder,
                iterator,
                result_out,
                deopt_out,
                None,
                native_operations.foreach_cleanup,
                native_operations.value_release,
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
fn lower_cached_bind_global(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    semantic_helper: Option<NativeHelper>,
    lifecycle: Option<NativeHelper>,
    locals: &NativeLocalMap,
    register_variables: &NativeRegisterMap,
    registers: &mut NativeRegisterMap,
    call: &RegionNativeCall,
    instruction: &RegionInstruction,
    transition_live_registers: &[RegId],
    streaming_call_exit: Option<NativeStreamingCallExit>,
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    native_version: u32,
    unit_identity: u64,
    pointer_type: ir::Type,
) -> Result<(), CraneliftLoweringError> {
    let RegionCallResult::ReferenceLocal(destination) = call.result else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_BIND_GLOBAL_RESULT",
            "BindGlobal must publish a reference local",
        ));
    };
    let inspect = builder.create_block();
    let cached = builder.create_block();
    let slow = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(cached, types::I64);
    builder.append_block_param(merge, types::I64);

    let runtime_view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let cache = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        deopt_out,
        runtime_view_offset
            + std::mem::offset_of!(crate::JitNativeRuntimeView, global_reference_cache) as i32,
    );
    builder.ins().jump(inspect, &[]);

    builder.switch_to_block(inspect);
    let cache_index = crate::jit_native_global_reference_cache_index(
        unit_identity,
        function.raw(),
        instruction.continuation_id,
        (crate::JIT_NATIVE_GLOBAL_REFERENCE_CACHE_SIZE - 1) as u32,
    );
    let record = builder.ins().iadd_imm(
        cache,
        i64::try_from(cache_index.saturating_mul(std::mem::size_of::<
            crate::JitNativeGlobalReferenceCacheRecord,
        >()))
        .unwrap_or(i64::MAX),
    );
    let cached_unit = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, unit_identity) as i32,
    );
    let encoded = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, encoded) as i32,
    );
    let cached_function = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, function_id) as i32,
    );
    let cached_continuation = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, continuation_id) as i32,
    );
    let valid = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, valid) as i32,
    );
    let unit_ok = builder
        .ins()
        .icmp_imm(IntCC::Equal, cached_unit, unit_identity as i64);
    let function_ok =
        builder
            .ins()
            .icmp_imm(IntCC::Equal, cached_function, i64::from(function.raw()));
    let continuation_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        cached_continuation,
        i64::from(instruction.continuation_id),
    );
    let valid = builder.ins().icmp_imm(IntCC::NotEqual, valid, 0);
    let record_ok = builder.ins().band(unit_ok, function_ok);
    let record_ok = builder.ins().band(record_ok, continuation_ok);
    let record_ok = builder.ins().band(record_ok, valid);
    builder
        .ins()
        .brif(record_ok, cached, &[encoded.into()], slow, &[]);

    builder.switch_to_block(cached);
    let encoded = builder.block_params(cached)[0];
    let encoded = lower_guarded_value_release(
        module, builder, lifecycle, 0, encoded, result_out, deopt_out,
    )?;
    builder.ins().jump(merge, &[encoded.into()]);

    builder.switch_to_block(slow);
    lower_direct_semantic_call(
        module,
        builder,
        semantic_helper,
        locals,
        register_variables,
        registers,
        call,
        crate::region_ir::RegionSemanticOperationId::BindGlobal,
        instruction,
        transition_live_registers,
        streaming_call_exit,
        result_out,
        deopt_out,
        function,
        local_count,
        native_version,
        unit_identity,
        pointer_type,
    )?;
    let encoded = use_local_variable(builder, locals, destination)?;
    builder.ins().jump(merge, &[encoded.into()]);

    builder.switch_to_block(merge);
    let encoded = builder.block_params(merge)[0];
    define_local_variable(builder, locals, destination, encoded)?;
    Ok(())
}

fn lower_optimizing_cached_bind_global(
    builder: &mut FunctionBuilder<'_>,
    locals: &NativeLocalMap,
    call: &RegionNativeCall,
    instruction: &RegionInstruction,
    transition: NativeOptimizingTransition<'_>,
    unit_identity: u64,
    function: FunctionId,
) -> Result<(), CraneliftLoweringError> {
    let RegionCallResult::ReferenceLocal(destination) = call.result else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_BIND_GLOBAL_RESULT",
            "BindGlobal must publish a reference local",
        ));
    };
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let cached = builder.create_block();
    let miss = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(cached, types::I64);
    builder.append_block_param(merge, types::I64);

    let runtime_view_offset = std::mem::offset_of!(crate::JitDeoptState, runtime_view) as i32;
    let cache = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        transition.deopt_out,
        runtime_view_offset
            + std::mem::offset_of!(crate::JitNativeRuntimeView, global_reference_cache) as i32,
    );
    let cache_index = crate::jit_native_global_reference_cache_index(
        unit_identity,
        function.raw(),
        instruction.continuation_id,
        (crate::JIT_NATIVE_GLOBAL_REFERENCE_CACHE_SIZE - 1) as u32,
    );
    let record = builder.ins().iadd_imm(
        cache,
        i64::try_from(cache_index.saturating_mul(std::mem::size_of::<
            crate::JitNativeGlobalReferenceCacheRecord,
        >()))
        .unwrap_or(i64::MAX),
    );
    let cached_unit = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, unit_identity) as i32,
    );
    let encoded = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, encoded) as i32,
    );
    let cached_function = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, function_id) as i32,
    );
    let cached_continuation = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, continuation_id) as i32,
    );
    let valid = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        record,
        std::mem::offset_of!(crate::JitNativeGlobalReferenceCacheRecord, valid) as i32,
    );
    let unit_ok = builder
        .ins()
        .icmp_imm(IntCC::Equal, cached_unit, unit_identity as i64);
    let function_ok =
        builder
            .ins()
            .icmp_imm(IntCC::Equal, cached_function, i64::from(function.raw()));
    let continuation_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        cached_continuation,
        i64::from(instruction.continuation_id),
    );
    let valid = builder.ins().icmp_imm(IntCC::NotEqual, valid, 0);
    let record_ok = builder.ins().band(unit_ok, function_ok);
    let record_ok = builder.ins().band(record_ok, continuation_ok);
    let record_ok = builder.ins().band(record_ok, valid);
    builder
        .ins()
        .brif(record_ok, cached, &[encoded.into()], miss, &[]);

    builder.switch_to_block(cached);
    let encoded = builder.block_params(cached)[0];
    // The cache owns one reference. The newly bound local owns a second one.
    // This is a direct slot update, not a lifecycle helper boundary.
    lower_optimizing_retain(builder, encoded, transition.deopt_out);
    builder.ins().jump(merge, &[encoded.into()]);

    builder.switch_to_block(miss);
    let resumed = transition.emit_value(builder)?;
    builder.ins().jump(merge, &[resumed.into()]);

    builder.switch_to_block(merge);
    let encoded = builder.block_params(merge)[0];
    define_local_variable(builder, locals, destination, encoded)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_direct_semantic_call(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    semantic_helper: Option<NativeHelper>,
    locals: &NativeLocalMap,
    register_variables: &NativeRegisterMap,
    registers: &mut NativeRegisterMap,
    call: &RegionNativeCall,
    operation: crate::region_ir::RegionSemanticOperationId,
    instruction: &RegionInstruction,
    transition_live_registers: &[RegId],
    streaming_call_exit: Option<NativeStreamingCallExit>,
    result_out: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    local_count: u32,
    native_version: u32,
    unit_identity: u64,
    pointer_type: ir::Type,
) -> Result<(), CraneliftLoweringError> {
    let helper = semantic_helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_SEMANTIC_DISPATCH",
            "typed semantic operation has no direct dispatcher",
        )
    })?;
    let operand_count = call.operands.len();
    let operands_ptr = if operand_count == 0 {
        builder.ins().iconst(pointer_type, 0)
    } else {
        let bytes = operand_count
            .checked_mul(std::mem::size_of::<i64>())
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_SEMANTIC_OPERANDS",
                    "semantic operand table exceeds native stack limits",
                )
            })?;
        let pointer = allocate_native_stack_storage(builder, pointer_type, bytes, 3);
        for (index, operand) in call.operands.iter().enumerate() {
            let operand = operand.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_SEMANTIC_OPERANDS",
                    "semantic operation is missing a materialized operand",
                )
            })?;
            let value = lower_region_operand(builder, locals, registers, operand)?;
            builder.ins().store(
                MemFlagsData::new(),
                value,
                pointer,
                i32::try_from(index.saturating_mul(8)).unwrap_or(i32::MAX),
            );
        }
        pointer
    };

    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        std::mem::size_of::<crate::JitCallResult>() as u32,
        3,
    ));
    let out_ptr = builder.ins().stack_addr(pointer_type, out_slot, 0);
    let function_id = builder.ins().iconst(types::I32, i64::from(function.raw()));
    let unit_identity = builder.ins().iconst(types::I64, unit_identity as i64);
    let continuation = builder
        .ins()
        .iconst(types::I32, i64::from(instruction.continuation_id));
    let operation_id = builder.ins().iconst(types::I32, i64::from(operation.raw()));
    let operand_count = builder
        .ins()
        .iconst(types::I32, i64::try_from(operand_count).unwrap_or(i64::MAX));
    let helper_call = call_native_helper(
        module,
        builder,
        helper,
        &[
            unit_identity,
            function_id,
            continuation,
            operation_id,
            operands_ptr,
            operand_count,
            out_ptr,
        ],
    );
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
    publish_native_register_state(builder, deopt_out, registers, transition_live_registers)?;
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
fn lower_direct_builtin_call(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    builtin_helper: Option<NativeHelper>,
    native_value_release_helper: Option<NativeHelper>,
    value_flow: &ExecutableValueFlow,
    locals: &NativeLocalMap,
    register_variables: &NativeRegisterMap,
    registers: &mut NativeRegisterMap,
    call: &RegionNativeCall,
    builtin_id: u32,
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
    let helper = builtin_helper.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_BUILTIN_DISPATCH",
            "direct builtin call has no stable-ID dispatcher",
        )
    })?;
    let argument_count = call.args.len();
    let mut consumed_arguments = Vec::new();
    let arguments_ptr = if argument_count == 0 {
        builder.ins().iconst(pointer_type, 0)
    } else {
        let bytes = argument_count
            .checked_mul(std::mem::size_of::<i64>())
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_BUILTIN_ARGUMENTS",
                    "direct builtin argument table exceeds native stack limits",
                )
            })?;
        let pointer = allocate_native_stack_storage(builder, pointer_type, bytes, 3);
        for (index, (argument, operand)) in call.args.iter().zip(&call.operands).enumerate() {
            let operand = operand.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_BUILTIN_ARGUMENTS",
                    "direct builtin argument is missing its native operand",
                )
            })?;
            let value = lower_region_operand(builder, locals, registers, operand)?;
            if matches!(
                operand,
                RegionOperand::Register(register)
                    if value_flow.register_fact(register).ownership != SsaOwnership::Borrowed
            ) && native_argument_has_location(argument)
            {
                consumed_arguments.push(value);
            }
            builder.ins().store(
                MemFlagsData::new(),
                value,
                pointer,
                i32::try_from(index.saturating_mul(8)).unwrap_or(i32::MAX),
            );
        }
        pointer
    };

    let publishes_locals = matches!(&call.target, RegionCallTarget::Function { name, .. }
        if name.trim_start_matches('\\').eq_ignore_ascii_case("compact"));
    let published_local_count = if publishes_locals { local_count } else { 0 };
    let local_slots_ptr = if published_local_count == 0 {
        builder.ins().iconst(pointer_type, 0)
    } else {
        let slot_size = std::mem::size_of::<crate::JitAbiSlot>();
        let bytes = (published_local_count as usize)
            .checked_mul(slot_size)
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_BUILTIN_LOCALS",
                    "direct builtin local publication exceeds native stack limits",
                )
            })?;
        let pointer = allocate_native_stack_storage(builder, pointer_type, bytes, 3);
        for index in 0..published_local_count {
            let base = i32::try_from(index as usize * slot_size).unwrap_or(i32::MAX);
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

    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        std::mem::size_of::<crate::JitCallResult>() as u32,
        3,
    ));
    let out_ptr = builder.ins().stack_addr(pointer_type, out_slot, 0);
    let builtin_id = builder.ins().iconst(types::I32, i64::from(builtin_id));
    let function_id = builder.ins().iconst(types::I32, i64::from(function.raw()));
    let source_file = builder
        .ins()
        .iconst(types::I32, i64::from(instruction.span.file.raw()));
    let source_start = builder
        .ins()
        .iconst(types::I32, i64::from(instruction.span.start));
    let source_end = builder
        .ins()
        .iconst(types::I32, i64::from(instruction.span.end));
    let argument_count_value = builder.ins().iconst(
        types::I32,
        i64::try_from(argument_count).unwrap_or(i64::MAX),
    );
    let local_count_value = builder
        .ins()
        .iconst(types::I32, i64::from(published_local_count));
    let helper_call = call_native_helper(
        module,
        builder,
        helper,
        &[
            builtin_id,
            function_id,
            source_file,
            source_start,
            source_end,
            arguments_ptr,
            argument_count_value,
            local_slots_ptr,
            local_count_value,
            out_ptr,
        ],
    );
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
    publish_native_register_state(builder, deopt_out, registers, transition_live_registers)?;
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
    for argument in consumed_arguments {
        let _ = lower_guarded_value_release(
            module,
            builder,
            native_value_release_helper,
            native_dim_operation(1, function, instruction.continuation_id),
            argument,
            result_out,
            deopt_out,
        )?;
    }
    let value = builder.ins().stack_load(types::I64, out_slot, 16);
    match call.result {
        RegionCallResult::Register(register) => {
            define_region_register(builder, register_variables, registers, register, value)?;
        }
        RegionCallResult::Discard => {}
        RegionCallResult::ReferenceLocal(_) => unreachable!("filtered before direct builtin call"),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_native_call_trampoline(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    native_call_helper: Option<NativeHelper>,
    native_reference_bind_helper: Option<NativeHelper>,
    native_value_release_helper: Option<NativeHelper>,
    value_flow: &ExecutableValueFlow,
    locals: &NativeLocalMap,
    register_variables: &NativeRegisterMap,
    registers: &mut NativeRegisterMap,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
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
    let direct_builtin_helper = stable_builtin_helper_id(&call.target);
    let compact_builtin_arguments = direct_builtin_helper.is_some()
        && call.argument_operand_offset == 0
        && call.operands.len() == call.args.len()
        && call.args.iter().enumerate().all(|(index, argument)| {
            argument.name.is_none()
                && !argument.unpack
                && !call.argument_requires_reference_binding(index)
        });
    let argument_size = if compact_builtin_arguments {
        std::mem::size_of::<i64>()
    } else {
        std::mem::size_of::<crate::JitNativeCallArgument>()
    };
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
                external_function_signatures,
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
            let payload = lowered.unwrap_or_else(|| builder.ins().iconst(types::I64, 0));
            if compact_builtin_arguments {
                builder
                    .ins()
                    .store(MemFlagsData::new(), payload, pointer, base);
                continue;
            }
            let tag = if lowered.is_some() { 3 } else { 0 };
            let tag = builder.ins().iconst(types::I32, tag);
            builder.ins().store(MemFlagsData::new(), tag, pointer, base);
            let abi_flags = builder.ins().iconst(types::I32, 0);
            builder
                .ins()
                .store(MemFlagsData::new(), abi_flags, pointer, base + 4);
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
    } | if compact_builtin_arguments {
        crate::JitNativeCallFrame::FLAG_COMPACT_ARGUMENTS
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
    publish_native_register_state(builder, deopt_out, registers, transition_live_registers)?;
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
        let _ = lower_guarded_value_release(
            module,
            builder,
            native_value_release_helper,
            native_dim_operation(1, function, instruction.continuation_id),
            current,
            result_out,
            deopt_out,
        )?;
        let restore_args = [original.into()];
        builder.ins().jump(merge, &restore_args);

        builder.switch_to_block(restore_without_release);
        builder.ins().jump(merge, &restore_args);

        builder.switch_to_block(merge);
        define_local_variable(builder, locals, local, builder.block_params(merge)[0])?;
    }
    for argument in consumed_arguments {
        let _ = lower_guarded_value_release(
            module,
            builder,
            native_value_release_helper,
            native_dim_operation(1, function, instruction.continuation_id),
            argument,
            result_out,
            deopt_out,
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
