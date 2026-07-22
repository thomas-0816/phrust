use super::executable_region::{
    instruction_has_native_transition, select_native_region_tier, validate_pre_regalloc_structure,
};
use super::{
    CraneliftNativeCompiler, NativeCompilePlan, StableCallbackBuiltin, StableSymbolQueryBuiltin,
    build_trivial_add_clif_smoke, native_dim_operation, native_local_store_operation,
    ordinary_local_fast_path, runtime_helper_abi_hash, stable_builtin_dense_id,
    stable_builtin_symbol_query, stable_builtin_type_predicate,
};
use crate::region_ir::{
    BaselineRegionBuilder, CompileMetadata, NativeCompilerTier, RegionCallTarget,
};
use crate::{
    JIT_RUNTIME_ABI_HASH, JitCompileRequest, JitCompileStatus, NativeCompileRequest,
    NativeCompilerApi,
};
use cranelift_codegen::ir::{Function, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use php_ir::instruction::{IrCallArg, IrCallArgValueKind};
use php_ir::{
    BinaryOp, CastKind, ClassEntry, ClassFlags, ClassId, ClassMethodEntry, ClassMethodFlags,
    FunctionFlags, FunctionId, InstructionKind, IrBuilder, IrConstant, IrParam, IrReturnType,
    IrSpan, LocalId, Operand, UnaryOp, UnitId,
};
use std::sync::atomic::{AtomicUsize, Ordering};

static NATIVE_DYNAMIC_EFFECTS: AtomicUsize = AtomicUsize::new(0);
static SSA_FORBIDDEN_HELPER_CALLS: AtomicUsize = AtomicUsize::new(0);
static LOCAL_ARRAY_INSERT_CALLS: AtomicUsize = AtomicUsize::new(0);
static ARRAY_FETCH_FALLBACK_CALLS: AtomicUsize = AtomicUsize::new(0);
static FOREACH_NEXT_FALLBACK_CALLS: AtomicUsize = AtomicUsize::new(0);
static NESTED_TRANSITION_CALLS: AtomicUsize = AtomicUsize::new(0);
static NESTED_TRANSITION_FUNCTION: AtomicUsize = AtomicUsize::new(0);
static EXACT_CALLBACK_CALLS: AtomicUsize = AtomicUsize::new(0);

fn activate_direct_test_arena(
    slots: &mut [crate::JitNativeValueSlot],
    next_slot: &mut u32,
    entries: &mut [crate::JitNativeDirectArrayEntry],
    next_entry: &mut u32,
) -> crate::JitNativeRuntimeViewGuard {
    let free_value = Box::leak(Box::new(crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE));
    let states = Box::leak(
        vec![crate::JitNativeDirectArrayState::default(); slots.len()].into_boxed_slice(),
    );
    for (index, slot) in slots.iter().enumerate() {
        if slot.kind != crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
            continue;
        }
        let length = usize::try_from(slot.payload).expect("test array length");
        // SAFETY: every direct-array fixture points into the `entries` slice
        // retained by this activation helper.
        let array_entries = unsafe {
            std::slice::from_raw_parts(
                slot.aux as usize as *const crate::JitNativeDirectArrayEntry,
                length,
            )
        };
        let next_append_key = array_entries
            .iter()
            .filter_map(|entry| {
                (crate::jit_decode_runtime_value(entry.key).is_none()
                    && crate::jit_decode_constant(entry.key).is_none())
                .then_some(entry.key)
            })
            .map(|key| key.saturating_add(1))
            .max();
        states[index] = crate::JitNativeDirectArrayState {
            next_append_key: next_append_key.unwrap_or(0),
            has_next_append_key: u32::from(next_append_key.is_some()),
            reserved: 0,
        };
    }
    let free_heads = Box::leak(Box::new(
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS],
    ));
    crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(next_slot) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(free_value) as usize as u64,
        direct_array_states: states.as_mut_ptr() as usize as u64,
        direct_array_entries: entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(next_entry) as usize as u64,
        direct_array_free_heads: free_heads.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    })
}

fn assert_optimizing_artifact(handle: &crate::JitFunctionHandle) {
    let metadata = handle
        .region_state_metadata()
        .expect("native artifact metadata");
    assert_eq!(
        metadata.compiler_tier,
        NativeCompilerTier::Optimizing,
        "test silently compiled through the baseline tier"
    );
    assert!(
        !metadata.production_lowering.is_empty(),
        "optimizing artifact omitted its production lowering manifest"
    );
    assert!(
        metadata.production_lowering.iter().all(|entry| {
            !entry.operation.is_empty()
                && (!entry.operation_local_transition
                    || entry.class == crate::JitProductionLoweringClass::BaselineFragmentTransition)
        }),
        "optimizing artifact concealed an emitted local transition behind a direct class"
    );
    let forbidden = handle
        .relocatable_code()
        .expect("optimizer relocatable artifact")
        .relocations
        .iter()
        .filter_map(|relocation| match &relocation.target {
            crate::JitRelocatableTarget::Helper(symbol)
                if symbol.starts_with("phrust_native_preg_")
                    || symbol.starts_with("phrust_native_json_")
                    || StableSymbolQueryBuiltin::all()
                        .iter()
                        .any(|builtin| builtin.symbol() == symbol)
                    || StableCallbackBuiltin::all()
                        .iter()
                        .any(|builtin| builtin.symbol() == symbol) =>
            {
                None
            }
            crate::JitRelocatableTarget::Helper(symbol) => Some(symbol.as_str()),
            crate::JitRelocatableTarget::InternalFunction(_) => None,
        })
        .collect::<Vec<_>>();
    assert!(
        forbidden.is_empty(),
        "optimizer artifact imports forbidden helpers: {forbidden:?}"
    );
}

#[test]
fn stable_builtin_identity_survives_symbolic_function_metadata() {
    let predicate = RegionCallTarget::Function {
        name: "is_string".to_owned(),
        function: Some(FunctionId::new(17)),
    };
    assert_eq!(stable_builtin_type_predicate(&predicate), Some(4));
    assert!(stable_builtin_dense_id(&predicate).is_some());

    let namespaced = RegionCallTarget::Function {
        name: "Vendor\\is_string".to_owned(),
        function: Some(FunctionId::new(18)),
    };
    assert_eq!(stable_builtin_type_predicate(&namespaced), None);
    assert_eq!(stable_builtin_dense_id(&namespaced), None);

    let defined = RegionCallTarget::Function {
        name: "defined".to_owned(),
        function: Some(FunctionId::new(19)),
    };
    assert_eq!(
        stable_builtin_symbol_query(&defined),
        Some(StableSymbolQueryBuiltin::Defined)
    );
    let function_exists = RegionCallTarget::Function {
        name: "\\FUNCTION_EXISTS".to_owned(),
        function: None,
    };
    assert_eq!(
        stable_builtin_symbol_query(&function_exists),
        Some(StableSymbolQueryBuiltin::FunctionExists)
    );
}

#[test]
fn optimizing_callback_builtin_uses_exact_native_abi() {
    EXACT_CALLBACK_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_242));
    let file = builder.add_file("optimizing-exact-callback.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_exact_callback", FunctionFlags::default(), span);
    let callback = untyped_param(&mut builder, function, "callback");
    let value = untyped_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    let callback_value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: callback_value,
            local: callback,
        },
        span,
    );
    let argument_value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: argument_value,
            local: value,
        },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "call_user_func".to_owned(),
            args: [callback_value, argument_value]
                .into_iter()
                .map(|register| IrCallArg {
                    name: None,
                    value: Operand::Register(register),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                })
                .collect(),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.exact-callback").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_call_user_func: return_exact_callback_argument as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing exact callback handle");
    assert_optimizing_artifact(&handle);
    let helpers = handle
        .relocatable_code()
        .expect("optimizer relocatable artifact")
        .relocations
        .iter()
        .filter_map(|relocation| match &relocation.target {
            crate::JitRelocatableTarget::Helper(symbol) => Some(symbol.as_str()),
            crate::JitRelocatableTarget::InternalFunction(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(helpers, vec!["phrust_native_call_user_func"]);
    assert_eq!(
        handle
            .invoke_i64(&[11, 77], JIT_RUNTIME_ABI_HASH)
            .expect("exact callback execution"),
        77
    );
    assert_eq!(EXACT_CALLBACK_CALLS.load(Ordering::SeqCst), 1);
}

#[test]
fn plain_local_flags_exclude_php_visible_global_slots() {
    let locals = vec!["value".to_owned(), "GLOBALS".to_owned(), "_GET".to_owned()];

    assert!(ordinary_local_fast_path(false, &locals, LocalId::new(0)));
    assert_eq!(
        native_local_store_operation(false, &locals, LocalId::new(0)),
        crate::JIT_LOCAL_STORE_PLAIN_LOCAL
    );
    assert!(!ordinary_local_fast_path(true, &locals, LocalId::new(0)));
    assert!(!ordinary_local_fast_path(false, &locals, LocalId::new(1)));
    assert!(!ordinary_local_fast_path(false, &locals, LocalId::new(2)));
    assert_eq!(
        native_local_store_operation(false, &locals, LocalId::new(2)),
        0
    );
}

#[test]
fn persistent_helper_abi_identity_ignores_process_addresses() {
    let first = crate::JitRuntimeHelperAddresses {
        native_binary: 0x1000,
        native_local_fetch: 0x2000,
        ..crate::JitRuntimeHelperAddresses::default()
    };
    let second = crate::JitRuntimeHelperAddresses {
        native_binary: 0x3000,
        native_local_fetch: 0x4000,
        ..crate::JitRuntimeHelperAddresses::default()
    };
    assert_eq!(
        runtime_helper_abi_hash(first),
        runtime_helper_abi_hash(second)
    );
}

#[test]
fn optimizer_partitions_unsupported_effect_without_downgrading_function() {
    let mut builder = IrBuilder::new(UnitId::new(798));
    let file = builder.add_file("optimizer-baseline-firewall.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("firewall", FunctionFlags::default(), span);
    builder.set_entry(function);
    let block = builder.append_block(function);
    let dead_constant = builder.add_constant(IrConstant::Int(17));
    let live_constant = builder.add_constant(IrConstant::Int(23));
    let dead = builder.alloc_register(function);
    let live = builder.alloc_register(function);
    builder.emit_load_const(function, block, dead, dead_constant, span);
    builder.emit_load_const(function, block, live, live_constant, span);
    builder.emit(
        function,
        block,
        InstructionKind::Echo {
            src: Operand::Register(live),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(live)), span);
    let unit = builder.finish();
    let mut region = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            ir_fingerprint: "optimizer-baseline-firewall".to_owned(),
            tier: NativeCompilerTier::Optimizing,
            helper_abi_hash: 0,
            target_cpu: "test".to_owned(),
            semantic_config_hash: 0,
            dependency_identity: "test".to_owned(),
        },
    )
    .expect("region");
    let plan = NativeCompilePlan::for_region(&region);

    select_native_region_tier(&mut region, &plan, &unit.constants);

    assert_eq!(
        region.compile_metadata.tier,
        NativeCompilerTier::Optimizing,
        "one baseline island downgraded the complete optimizing function"
    );
    let echo = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|instruction| {
            matches!(
                instruction.kind,
                crate::region_ir::RegionInstructionKind::Echo { .. }
            )
        })
        .expect("echo baseline island");
    assert!(echo.optimizer_transition_entry);
    assert!(instruction_has_native_transition(
        echo,
        NativeCompilerTier::Baseline
    ));
}

#[test]
fn optimizer_keeps_top_level_body_on_native_request_scope_slots() {
    let mut builder = IrBuilder::new(UnitId::new(799));
    let file = builder.add_file("optimizer-top-level-scope.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "{main}",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.set_entry(function);
    let local = builder.intern_local(function, "wp_version");
    let value = builder.add_constant(IrConstant::String("6.8.3".to_owned()));
    let register = builder.alloc_register(function);
    let block = builder.append_block(function);
    builder.emit_load_const(function, block, register, value, span);
    builder.emit(
        function,
        block,
        InstructionKind::StoreLocal {
            local,
            src: Operand::Register(register),
        },
        span,
    );
    builder.terminate_return(function, block, None, span);
    let unit = builder.finish();
    let mut region = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            ir_fingerprint: "optimizer-top-level-scope".to_owned(),
            tier: NativeCompilerTier::Optimizing,
            helper_abi_hash: 0,
            target_cpu: "test".to_owned(),
            semantic_config_hash: 0,
            dependency_identity: "test".to_owned(),
        },
    )
    .expect("top-level region");
    let plan = NativeCompilePlan::for_region(&region);

    select_native_region_tier(&mut region, &plan, &unit.constants);

    assert_eq!(region.compile_metadata.tier, NativeCompilerTier::Optimizing);
    let flow = crate::region_ir::analyze_executable_value_flow(&region, &unit.constants);
    assert_eq!(
        flow.local_storage(local),
        crate::region_ir::LocalStorageClass::RequestGlobal
    );
}

#[test]
fn optimizing_top_level_store_condition_and_return_use_authoritative_request_slot() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_247));
    let file = builder.add_file("optimizing-top-level-request-slot.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "{main}",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.set_entry(function);
    let local = builder.intern_local(function, "request_value");
    let value = builder.intern_constant(IrConstant::Int(73));
    let zero = builder.intern_constant(IrConstant::Int(0));
    let entry = builder.append_block(function);
    let success = builder.append_block(function);
    let failure = builder.append_block(function);
    builder.emit(
        function,
        entry,
        InstructionKind::StoreLocal {
            local,
            src: Operand::Constant(value),
        },
        span,
    );
    builder.terminate_jump_if(
        function,
        entry,
        Operand::Local(local),
        success,
        failure,
        span,
    );
    builder.terminate_return(function, success, Some(Operand::Local(local)), span);
    builder.terminate_return(function, failure, Some(Operand::Constant(zero)), span);
    let unit = builder.finish();

    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.top-level-request-slot").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_local_store: forbidden_local_store as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing top-level request slot");
    assert_optimizing_artifact(&handle);

    let reference = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    let mut direct_slots = [crate::JitNativeValueSlot {
        refcount: 2,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        reserved: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        payload: crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED) as u64,
        ..crate::JitNativeValueSlot::default()
    }];
    let mut function_offsets = vec![0_u32; function.index() + 1];
    let mut request_slots = vec![crate::JitNativeRequestLocalSlot {
        encoded: reference,
        state: crate::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED,
        reserved: 0,
    }];
    let mut roots_dirty = 0_u32;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        trusted_request_local_function_offsets: function_offsets.as_mut_ptr() as usize as u64,
        trusted_request_local_function_count: function_offsets.len() as u32,
        trusted_request_local_slots: request_slots.as_mut_ptr() as usize as u64,
        trusted_request_local_slot_count: request_slots.len() as u32,
        root_mutation_pending: std::ptr::from_mut(&mut roots_dirty) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("top-level request-slot execution"),
        73
    );
    assert_eq!(direct_slots[0].payload as i64, 73);
    assert_eq!(request_slots[0].encoded, reference);
    assert_eq!(roots_dirty, 1);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn oversized_finished_clif_is_rejected_before_regalloc() {
    let mut ir_builder = IrBuilder::new(UnitId::new(799));
    let file = ir_builder.add_file("pre-regalloc-budget.php");
    let span = IrSpan::new(file, 0, 1);
    let function = ir_builder.start_function("bounded", FunctionFlags::default(), span);
    ir_builder.set_entry(function);
    let block = ir_builder.append_block(function);
    ir_builder.terminate_return(function, block, None, span);
    let unit = ir_builder.finish();
    let region = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            ir_fingerprint: "pre-regalloc-budget".to_owned(),
            tier: NativeCompilerTier::Baseline,
            helper_abi_hash: 0,
            target_cpu: "test".to_owned(),
            semantic_config_hash: 0,
            dependency_identity: "test".to_owned(),
        },
    )
    .unwrap();
    let mut clif =
        Function::with_name_signature(UserFuncName::user(0, 0), Signature::new(CallConv::SystemV));
    let mut context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut clif, &mut context);
        for _ in 0..2_049 {
            let block = builder.create_block();
            builder.switch_to_block(block);
            builder.ins().return_(&[]);
        }
        builder.seal_all_blocks();
        builder.finalize();
    }
    let error = validate_pre_regalloc_structure(&clif, &region, Some(7)).unwrap_err();
    assert_eq!(error.code, "JIT_CRANELIFT_PRE_REGALLOC_BUDGET");
    assert!(error.detail.contains("clif_blocks=2049/768"));
}

extern "C" fn forbidden_local_fetch(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _value: i64,
    _function: i64,
    _local: i64,
    _file: i64,
    _start: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_local_store(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _current: i64,
    _value: i64,
    _function: i64,
    _local: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_release(_runtime: *mut std::ffi::c_void, _value: i64) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn frame_cleanup_release(_runtime: *mut std::ffi::c_void, _value: i64) -> i32 {
    0
}

extern "C" fn forbidden_binary(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _lhs: i64,
    _rhs: i64,
    _function: i64,
    _continuation: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_compare(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _lhs: i64,
    _rhs: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_truthy(
    _runtime: *mut std::ffi::c_void,
    _value: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn baseline_truthy_true(
    _runtime: *mut std::ffi::c_void,
    _value: i64,
    out: *mut i64,
) -> i32 {
    // SAFETY: the baseline truthiness ABI supplies one writable result slot.
    unsafe { out.write(1) };
    crate::JitCallStatus::CONTINUE.0 as i32
}

extern "C" fn forbidden_cast(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _value: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_unary(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _value: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_type_predicate(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _value: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_stable_length(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _value: i64,
    _function: i64,
    _continuation: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_call_dispatch(
    _runtime: *mut std::ffi::c_void,
    _context: u64,
    _frame: *mut crate::JitNativeCallFrame,
    _out: *mut crate::JitCallResult,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_reference_bind(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _value: i64,
    _key: i64,
    _reserved: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_property_fetch(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _object: i64,
    _function: i64,
    _instruction: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_property_assign(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _object: i64,
    _value: i64,
    _function: i64,
    _instruction: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

#[allow(unsafe_code)]
extern "C" fn return_first_call_argument(
    _runtime: *mut std::ffi::c_void,
    _context: u64,
    frame: *mut crate::JitNativeCallFrame,
    out: *mut crate::JitCallResult,
) -> i32 {
    assert!(!frame.is_null());
    assert!(!out.is_null());
    // SAFETY: generated code owns the frame, argument table, and result for
    // the complete synchronous call.
    let frame = unsafe { &*frame };
    assert_eq!(frame.argument_count, 1);
    let argument = unsafe { &*(frame.arguments as *const crate::JitNativeCallArgument) };
    unsafe {
        out.write(crate::JitCallResult {
            status: crate::JitCallStatus::RETURN,
            detail: 0,
            value: argument.value,
        });
    }
    crate::JitCallStatus::RETURN.0 as i32
}

extern "C" fn passthrough_release(_runtime: *mut std::ffi::c_void, _value: i64) -> i32 {
    0
}

extern "C" fn return_exact_callback_argument(
    _runtime: *mut std::ffi::c_void,
    _caller_function: u32,
    _source_file: u32,
    _source_start: u32,
    _source_end: u32,
    argument_count: u32,
    _callback: i64,
    argument: i64,
    _argument_2: i64,
    _argument_3: i64,
    _argument_4: i64,
    _argument_5: i64,
) -> crate::JitNativeControlResult {
    assert_eq!(argument_count, 2);
    EXACT_CALLBACK_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitNativeControlResult::returning(argument)
}

#[allow(unsafe_code)]
extern "C" fn passthrough_local_fetch(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    value: i64,
    _function: i64,
    _local: i64,
    _file: i64,
    _start: i64,
    out: *mut i64,
) -> i32 {
    assert!(!out.is_null());
    unsafe { out.write(value) };
    0
}

#[allow(unsafe_code)]
extern "C" fn test_array_new(_runtime: *mut std::ffi::c_void, _op: u32, out: *mut i64) -> i32 {
    assert!(!out.is_null());
    unsafe { out.write(crate::jit_encode_runtime_value(7)) };
    0
}

#[allow(unsafe_code)]
extern "C" fn test_array_insert(
    _runtime: *mut std::ffi::c_void,
    append: u32,
    array: i64,
    _key: i64,
    _value: i64,
    out: *mut i64,
) -> i32 {
    let append = if append & 0x8000_0000 != 0 {
        append & 1
    } else {
        append
    };
    if append != 1 {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    assert!(!out.is_null());
    unsafe { out.write(array) };
    0
}

#[allow(unsafe_code)]
extern "C" fn test_keyed_array_insert_returns_array(
    _runtime: *mut std::ffi::c_void,
    _operation: u32,
    array: i64,
    _key: i64,
    _value: i64,
    out: *mut i64,
) -> i32 {
    assert!(!out.is_null());
    unsafe { out.write(array) };
    0
}

extern "C" fn forbidden_array_insert(
    _runtime: *mut std::ffi::c_void,
    _append: u32,
    _array: i64,
    _key: i64,
    _value: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn test_local_array_insert(
    _runtime: *mut std::ffi::c_void,
    append: u32,
    array: i64,
    key: i64,
    value: i64,
    out: *mut i64,
) -> i32 {
    LOCAL_ARRAY_INSERT_CALLS.fetch_add(1, Ordering::SeqCst);
    test_array_insert(_runtime, append, array, key, value, out)
}

#[allow(unsafe_code)]
extern "C" fn test_array_fetch_typed_string(
    _runtime: *mut std::ffi::c_void,
    _quiet: u32,
    _array: i64,
    _key: i64,
    out: *mut i64,
) -> i32 {
    assert!(!out.is_null());
    unsafe {
        out.write(crate::jit_encode_typed_runtime_value(
            7,
            crate::JIT_VALUE_RUNTIME_STRING_TAG,
        ))
    };
    0
}

extern "C" fn forbidden_cached_array_fetch(
    _runtime: *mut std::ffi::c_void,
    _quiet: u32,
    _array: i64,
    _key: i64,
    _out: *mut i64,
) -> i32 {
    ARRAY_FETCH_FALLBACK_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_foreach_next(
    _runtime: *mut std::ffi::c_void,
    _iterator: i64,
    _key_out: *mut i64,
    _value_out: *mut i64,
    _has_out: *mut i64,
) -> i32 {
    FOREACH_NEXT_FALLBACK_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_foreach_init(
    _runtime: *mut std::ffi::c_void,
    _op: u32,
    _source: i64,
    _function: i64,
    _continuation: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_foreach_cleanup(_runtime: *mut std::ffi::c_void, _iterator: i64) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

#[allow(unsafe_code)]
extern "C" fn test_array_key_exists_fast(
    _runtime: *mut std::ffi::c_void,
    operation: u32,
    array: i64,
    key: i64,
    out: *mut i64,
) -> i32 {
    if operation != 2
        || array != crate::jit_encode_typed_runtime_value(3, crate::JIT_VALUE_RUNTIME_ARRAY_TAG)
        || key != 7
    {
        return crate::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    assert!(!out.is_null());
    unsafe { out.write(crate::jit_encode_constant(crate::JIT_VALUE_TRUE)) };
    0
}

#[allow(unsafe_code)]
extern "C" fn test_string_predicate_fast(
    _runtime: *mut std::ffi::c_void,
    operation: u32,
    _haystack: i64,
    _needle: i64,
    out: *mut i64,
) -> i32 {
    if operation & 0xff != 1 {
        return crate::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    assert!(!out.is_null());
    unsafe { out.write(crate::jit_encode_constant(crate::JIT_VALUE_TRUE)) };
    0
}

#[test]
fn lifecycle_operation_carries_function_and_continuation_context() {
    let encoded = native_dim_operation(1, FunctionId::new(37), 91_337);

    assert_ne!(encoded & 0x8000_0000, 0);
    assert_eq!(encoded & 1, 1);
    assert_eq!((encoded >> 1) & 0x03ff, 37);
    assert_eq!((encoded >> 11) & 0x0f_ffff, 91_337);
    assert_eq!(native_dim_operation(0, FunctionId::new(1_024), 1), 0);
    assert_eq!(native_dim_operation(1, FunctionId::new(1), 0x10_0000), 1);
}

extern "C" fn test_native_dynamic_code(
    _runtime: *mut std::ffi::c_void,
    _vm_context: u64,
    request: *mut crate::JitNativeDynamicCodeRequest,
    out: *mut crate::JitCallResult,
) -> i32 {
    if request.is_null() || out.is_null() {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    // SAFETY: Generated code owns both records for this synchronous call.
    let request = unsafe { &*request };
    let payload = if request.kind == crate::JitNativeDynamicCodeKind::REQUIRE_ONCE {
        if request.source.payload != 91 {
            return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        123
    } else if request.kind == crate::JitNativeDynamicCodeKind::EVAL {
        if request.source.payload != 92 {
            return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        321
    } else if request.kind == crate::JitNativeDynamicCodeKind::REQUIRE {
        NATIVE_DYNAMIC_EFFECTS.fetch_add(1, Ordering::SeqCst);
        41
    } else {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    };
    // SAFETY: `out` is checked and caller-owned.
    unsafe {
        out.write(crate::JitCallResult {
            status: crate::JitCallStatus::RETURN,
            detail: request.continuation_id,
            value: crate::JitAbiSlot {
                tag: 3,
                flags: 0,
                payload,
            },
        });
    }
    crate::JitCallStatus::RETURN.0 as i32
}

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
fn optimizing_scalar_ssa_executes_without_local_truthy_or_lifecycle_helpers() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_202));
    let file = builder.add_file("optimizing-ssa.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("optimizing_ssa", FunctionFlags::default(), span);
    let local = builder.intern_local(function, "value");
    let entry = builder.append_block(function);
    let success = builder.append_block(function);
    let failure = builder.append_block(function);
    let forty = builder.intern_constant(IrConstant::Int(40));
    let two = builder.intern_constant(IrConstant::Int(2));
    builder.emit(
        function,
        entry,
        InstructionKind::StoreLocal {
            local,
            src: Operand::Constant(forty),
        },
        span,
    );
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    let copied = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::Move {
            dst: copied,
            src: Operand::Register(loaded),
        },
        span,
    );
    let sum = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::Binary {
            dst: sum,
            op: BinaryOp::Add,
            lhs: Operand::Register(copied),
            rhs: Operand::Constant(two),
        },
        span,
    );
    builder.terminate_jump_if(
        function,
        entry,
        Operand::Register(sum),
        success,
        failure,
        span,
    );
    builder.terminate_return(function, success, Some(Operand::Register(sum)), span);
    builder.terminate_return(function, failure, Some(Operand::Constant(two)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let request = JitCompileRequest::new("cl.optimizing.executable-ssa").with_opt_level(2);
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &request,
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_binary: forbidden_binary as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_local_store: forbidden_local_store as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            native_truthy: forbidden_truthy as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing SSA handle");
    assert_optimizing_artifact(&handle);
    let relocatable = handle
        .relocatable_code()
        .expect("optimizing SSA artifact publishes relocations");
    assert!(
        relocatable.relocations.iter().all(|relocation| !matches!(
            &relocation.target,
            crate::JitRelocatableTarget::Helper(_)
        )),
        "the optimizing tier may not publish any runtime helper import"
    );
    let (promoted_locals, promoted_registers, _) = handle.ssa_metrics();
    assert!(promoted_locals > 0);
    assert!(promoted_registers > 0);
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("optimizing SSA execution"),
        42
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_integer_shift_keeps_php_large_shift_semantics_in_clif() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_206));
    let file = builder.add_file("optimizing-shift.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("optimizing_shift", FunctionFlags::default(), span);
    let local = builder.intern_local(function, "value");
    builder.push_param(
        function,
        IrParam {
            name: "value".to_owned(),
            local,
            required: true,
            default: None,
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    let amount = builder.intern_constant(IrConstant::Int(65));
    let shifted = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: shifted,
            op: BinaryOp::ShiftRight,
            lhs: Operand::Register(loaded),
            rhs: Operand::Constant(amount),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(shifted)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.shift").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_binary: forbidden_binary as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing shift handle");
    assert_optimizing_artifact(&handle);
    assert_eq!(
        handle
            .invoke_i64(&[-3], JIT_RUNTIME_ABI_HASH)
            .expect("optimizing shift execution"),
        -1
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_manifest_records_the_emitted_division_transition() {
    let mut builder = IrBuilder::new(UnitId::new(4_299));
    let file = builder.add_file("optimizing-emitted-transition.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_emitted_transition",
        FunctionFlags::default(),
        span,
    );
    let lhs_local = typed_int_param(&mut builder, function, "lhs");
    let rhs_local = typed_int_param(&mut builder, function, "rhs");
    let block = builder.append_block(function);
    let lhs = builder.alloc_register(function);
    let rhs = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: lhs,
            local: lhs_local,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: rhs,
            local: rhs_local,
        },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Div,
            lhs: Operand::Register(lhs),
            rhs: Operand::Register(rhs),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.emitted-transition").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing division handle");
    assert_optimizing_artifact(&handle);
    let metadata = handle.region_state_metadata().expect("native metadata");
    let division = metadata
        .production_lowering
        .iter()
        .find(|entry| entry.operation == "Binary")
        .expect("division lowering row");
    assert_eq!(
        division.class,
        crate::JitProductionLoweringClass::BaselineFragmentTransition
    );
    assert!(division.operation_local_transition);
}

#[test]
fn optimizing_boolean_relational_compare_normalizes_tagged_payloads() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_209));
    let file = builder.add_file("optimizing-bool-compare.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_bool_compare", FunctionFlags::default(), span);
    let local = builder.intern_local(function, "value");
    builder.push_param(
        function,
        IrParam {
            name: "value".to_owned(),
            local,
            required: true,
            default: None,
            type_: Some(IrReturnType::Bool),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    let true_ = builder.intern_constant(IrConstant::Bool(true));
    let compared = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Compare {
            dst: compared,
            op: php_ir::CompareOp::Less,
            lhs: Operand::Register(loaded),
            rhs: Operand::Constant(true_),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(compared)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.bool-compare").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_compare: forbidden_compare as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing bool compare");
    assert_optimizing_artifact(&handle);
    assert_eq!(
        handle
            .invoke_i64(
                &[crate::jit_encode_constant(crate::JIT_VALUE_FALSE)],
                JIT_RUNTIME_ABI_HASH,
            )
            .expect("false < true"),
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE)
    );
    assert_eq!(
        handle
            .invoke_i64(
                &[crate::jit_encode_constant(crate::JIT_VALUE_TRUE)],
                JIT_RUNTIME_ABI_HASH,
            )
            .expect("true < true"),
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE)
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_borrowed_parameter_discard_does_not_release_owner() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_230));
    let file = builder.add_file("optimizing-borrowed-discard.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_borrowed_discard",
        FunctionFlags::default(),
        span,
    );
    let local = builder.intern_local(function, "object");
    builder.push_param(
        function,
        IrParam {
            name: "object".to_owned(),
            local,
            required: true,
            default: None,
            type_: Some(IrReturnType::Object),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(loaded),
        },
        span,
    );
    builder.terminate_return(function, block, None, span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.borrowed-discard").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing borrowed discard");
    assert_optimizing_artifact(&handle);
    let mut slots = vec![crate::JitNativeValueSlot::default(); 4];
    slots[3].refcount = 2;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let object = crate::jit_encode_typed_runtime_value(3, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
    assert_eq!(
        handle.invoke_i64(&[object], JIT_RUNTIME_ABI_HASH),
        Ok(crate::jit_encode_constant(u32::MAX))
    );
    assert_eq!(slots[3].refcount, 2, "borrowed discard released its owner");
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_reference_local_reads_published_scalar_view_without_helper() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_217));
    let file = builder.add_file("optimizing-reference-scalar-view.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_reference_scalar_view",
        FunctionFlags::default(),
        span,
    );
    let local = builder.intern_local(function, "value");
    builder.push_param(
        function,
        IrParam {
            name: "value".to_owned(),
            local,
            required: true,
            default: None,
            type_: None,
            by_ref: true,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(loaded)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.reference-scalar-view").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing reference scalar view");
    assert_optimizing_artifact(&handle);
    let mut reference_view = crate::JitNativeReferenceScalarView {
        abi_version: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        state: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        encoded: 73,
    };
    let mut slots = vec![crate::JitNativeValueSlot::default(); 8];
    slots[7] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        payload: std::ptr::from_mut(&mut reference_view) as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let referenced_string =
        crate::jit_encode_typed_runtime_value(5, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    slots[5] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_slots = [crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        reserved: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        payload: referenced_string as u64,
        ..crate::JitNativeValueSlot::default()
    }];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let reference =
        crate::jit_encode_typed_runtime_value(7, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    assert_eq!(
        handle
            .invoke_i64(&[reference], JIT_RUNTIME_ABI_HASH)
            .expect("cached scalar reference read"),
        73
    );
    let direct_reference = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[direct_reference], JIT_RUNTIME_ABI_HASH)
            .expect("direct reference payload read"),
        referenced_string
    );
    assert_eq!(
        slots[5].refcount, 2,
        "direct reference read did not retain its returned runtime payload"
    );

    // Reference-capable storage describes a local's lifetime, not the tag of
    // every value ever held in that local. Before a later binding executes it
    // may contain an ordinary array; optimized loading must return that value
    // directly instead of interpreting the array-length payload as a pointer
    // to JitNativeReferenceScalarView.
    slots[6] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_ARRAY,
        flags: crate::JIT_NATIVE_ARRAY_VIEW_ABI_VERSION,
        payload: 11,
        ..crate::JitNativeValueSlot::default()
    };
    let array = crate::jit_encode_typed_runtime_value(6, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    assert_eq!(
        handle
            .invoke_i64(&[array], JIT_RUNTIME_ABI_HASH)
            .expect("ordinary value in reference-capable local"),
        array
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_direct_reference_replaces_native_handle_and_tests_payload() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_246));
    let file = builder.add_file("optimizing-direct-reference-handle.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_direct_reference_handle",
        FunctionFlags::default(),
        span,
    );
    let target = builder.intern_local(function, "target");
    builder.push_param(
        function,
        IrParam {
            name: "target".to_owned(),
            local: target,
            required: true,
            default: None,
            type_: None,
            by_ref: true,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let replacement = typed_string_param(&mut builder, function, "replacement");
    let entry = builder.append_block(function);
    let check_empty = builder.append_block(function);
    let success = builder.append_block(function);
    let failure = builder.append_block(function);
    builder.emit(
        function,
        entry,
        InstructionKind::StoreLocal {
            local: target,
            src: Operand::Local(replacement),
        },
        span,
    );
    let isset = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::IssetLocal {
            dst: isset,
            local: target,
        },
        span,
    );
    builder.terminate_jump_if(
        function,
        entry,
        Operand::Register(isset),
        check_empty,
        failure,
        span,
    );

    let empty = builder.alloc_register(function);
    builder.emit(
        function,
        check_empty,
        InstructionKind::EmptyLocal {
            dst: empty,
            local: target,
        },
        span,
    );
    builder.terminate_jump_if(
        function,
        check_empty,
        Operand::Register(empty),
        failure,
        success,
        span,
    );

    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        success,
        InstructionKind::LoadLocal {
            dst: loaded,
            local: target,
        },
        span,
    );
    builder.terminate_return(function, success, Some(Operand::Register(loaded)), span);
    let false_ = builder.intern_constant(IrConstant::Bool(false));
    builder.terminate_return(function, failure, Some(Operand::Constant(false_)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.direct-reference-handle").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_local_store: forbidden_local_store as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("direct reference handle replacement");
    assert_optimizing_artifact(&handle);

    let previous_bytes = b"old";
    let replacement_bytes = b"replacement";
    let mut string_bytes = vec![0_u8; 16];
    string_bytes[..previous_bytes.len()].copy_from_slice(previous_bytes);
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    direct_slots[1] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        reserved: crate::jit_native_direct_string_reserved(4, false),
        payload: previous_bytes.len() as u64,
        aux: string_bytes.as_mut_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    direct_slots[2] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: replacement_bytes.len() as u64,
        aux: replacement_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let previous = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let replacement = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 2,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    direct_slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        reserved: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        payload: previous as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_next = 3_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut string_next = 4_u32;
    let mut string_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS];
    let mut roots_dirty = 0_u32;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_string_bytes: string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(&mut string_next) as usize as u64,
        direct_string_free_heads: string_free.as_mut_ptr() as usize as u64,
        root_mutation_pending: std::ptr::from_mut(&mut roots_dirty) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let reference = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[reference, replacement], JIT_RUNTIME_ABI_HASH)
            .expect("direct reference handle execution"),
        replacement
    );
    assert_eq!(direct_slots[0].payload as i64, replacement);
    assert_eq!(direct_slots[1].refcount, 0);
    assert!(direct_slots[2].refcount >= 2);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_reference_array_load_acquires_intrusive_cow_owner_without_helper() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_220));
    let file = builder.add_file("optimizing-reference-array-load.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_reference_array_load",
        FunctionFlags::default(),
        span,
    );
    let local = builder.intern_local(function, "array");
    builder.push_param(
        function,
        IrParam {
            name: "array".to_owned(),
            local,
            required: true,
            default: None,
            type_: None,
            by_ref: true,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(loaded)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.reference-array-load").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing reference array load");
    assert_optimizing_artifact(&handle);

    let mut strong = 1_usize;
    let mut scalar = crate::JitNativeReferenceScalarView {
        abi_version: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        state: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY,
        encoded: 0,
    };
    let mut array = crate::JitNativeReferenceArrayView {
        abi_version: crate::JIT_NATIVE_REFERENCE_ARRAY_VIEW_ABI_VERSION,
        state: crate::JIT_NATIVE_REFERENCE_ARRAY_VIEW_PUBLISHED,
        length: 0,
        entries: 0,
        storage_refcount: std::ptr::from_mut(&mut strong) as usize as u64,
        dirty: 0,
        reserved: 0,
    };
    let mut slots = vec![crate::JitNativeValueSlot::default(); 8];
    slots[7] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        payload: std::ptr::from_mut(&mut scalar) as usize as u64,
        aux: std::ptr::from_mut(&mut array) as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_slots = vec![crate::JitNativeValueSlot::default(); 8];
    let mut direct_next = 0_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let reference =
        crate::jit_encode_typed_runtime_value(7, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    let loaded = handle
        .invoke_i64(&[reference], JIT_RUNTIME_ABI_HASH)
        .expect("native reference array load");
    assert_eq!(
        loaded,
        (crate::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            as i64
    );
    assert_eq!(strong, 2, "native load did not acquire a COW owner");
    assert_eq!(direct_next, 1);
    assert_eq!(direct_slots[0].refcount, 1);
    assert_eq!(
        direct_slots[0].kind,
        crate::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
    );
    assert_eq!(
        direct_slots[0].payload,
        std::ptr::from_mut(&mut strong) as usize as u64
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_owned_handle_moves_into_plain_local_without_refcount_pair() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_207));
    let file = builder.add_file("optimizing-handle-move.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("optimizing_handle_move", FunctionFlags::default(), span);
    let local = builder.intern_local(function, "value");
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::StoreLocal {
            local,
            src: Operand::Register(array),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(array),
        },
        span,
    );
    let child = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: child },
        span,
    );
    let inserted = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::AppendDim {
            dst: inserted,
            local,
            dims: Vec::new(),
            value: Operand::Register(child),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(child),
        },
        span,
    );
    builder.terminate_return(function, block, None, span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.handle-move").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: test_array_new as *const () as usize,
            native_local_store: forbidden_local_store as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing handle move");
    assert_optimizing_artifact(&handle);
    assert!(handle.ssa_metrics().2 > 0);
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = activate_direct_test_arena(
        &mut direct_slots,
        &mut next_slot,
        &mut direct_entries,
        &mut next_entry,
    );
    handle
        .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
        .expect("optimizing handle move execution");
    assert_eq!(next_slot, 2);
    assert_eq!(
        next_entry,
        crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY * 2
    );
    assert_eq!(direct_slots[0].refcount, 0);
    assert_eq!(direct_slots[0].kind, crate::JIT_NATIVE_VALUE_VIEW_NONE);
    assert_eq!(direct_slots[1].refcount, 0);
    assert_eq!(direct_slots[1].kind, crate::JIT_NATIVE_VALUE_VIEW_NONE);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn baseline_new_array_uses_direct_native_arena_before_cold_helper() {
    let mut builder = IrBuilder::new(UnitId::new(4_207_1));
    let file = builder.add_file("baseline-direct-new-array.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("baseline_direct_new_array", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(array)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.baseline.direct-new-array"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: test_array_new as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("baseline direct new-array handle");
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = activate_direct_test_arena(
        &mut direct_slots,
        &mut next_slot,
        &mut direct_entries,
        &mut next_entry,
    );
    let returned = handle
        .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
        .expect("baseline direct array allocation");
    assert_eq!(
        crate::jit_decode_runtime_value(returned),
        Some(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
    );
    assert_eq!(
        direct_slots[0].kind,
        crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
    );
    assert_eq!(next_slot, 1);
}

#[test]
fn baseline_array_append_and_fetch_stay_on_direct_native_data_plane() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_207_2));
    let file = builder.add_file("baseline-direct-array-append.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "baseline_direct_array_append",
        FunctionFlags::default(),
        span,
    );
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    let value = builder.intern_constant(IrConstant::Int(42));
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array,
            key: None,
            value: Operand::Constant(value),
            by_ref_local: None,
        },
        span,
    );
    let zero = builder.intern_constant(IrConstant::Int(0));
    let fetched = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchDim {
            dst: fetched,
            array: Operand::Register(array),
            key: Operand::Constant(zero),
            quiet: false,
            mode: php_ir::instruction::DimFetchMode::Read,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(fetched)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.baseline.direct-array-append"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: test_array_new as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_array_fetch: forbidden_cached_array_fetch as *const () as usize,
            native_value_release: frame_cleanup_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("baseline direct array append handle");
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = activate_direct_test_arena(
        &mut direct_slots,
        &mut next_slot,
        &mut direct_entries,
        &mut next_entry,
    );
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("baseline direct append and fetch"),
        42
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(direct_slots[0].payload, 1);
    assert_eq!(direct_entries[0].value, 42);
}

#[test]
fn baseline_keyed_array_insert_and_overwrite_stay_on_direct_native_data_plane() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_207_3));
    let file = builder.add_file("baseline-direct-keyed-array.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "baseline_direct_keyed_array",
        FunctionFlags::default(),
        span,
    );
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    let key = builder.intern_constant(IrConstant::Int(7));
    for value in [42, 43] {
        let value = builder.intern_constant(IrConstant::Int(value));
        builder.emit(
            function,
            block,
            InstructionKind::ArrayInsert {
                array,
                key: Some(Operand::Constant(key)),
                value: Operand::Constant(value),
                by_ref_local: None,
            },
            span,
        );
    }
    let fetched = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchDim {
            dst: fetched,
            array: Operand::Register(array),
            key: Operand::Constant(key),
            quiet: false,
            mode: php_ir::instruction::DimFetchMode::Read,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(fetched)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.baseline.direct-keyed-array"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: test_array_new as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_array_fetch: forbidden_cached_array_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("baseline direct keyed-array handle");
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = activate_direct_test_arena(
        &mut direct_slots,
        &mut next_slot,
        &mut direct_entries,
        &mut next_entry,
    );
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("baseline direct keyed insert and overwrite"),
        43
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(direct_slots[0].payload, 1);
    assert_eq!(direct_entries[0].key, 7);
    assert_eq!(direct_entries[0].value, 43);
}

#[test]
fn optimizing_direct_array_matches_distinct_equal_string_key_handles() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    ARRAY_FETCH_FALLBACK_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_207_4));
    let file = builder.add_file("baseline-direct-string-key-array.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "baseline_direct_string_key_array",
        FunctionFlags::default(),
        span,
    );
    let insert_key = untyped_param(&mut builder, function, "insert_key");
    let fetch_key = untyped_param(&mut builder, function, "fetch_key");
    let block = builder.append_block(function);
    let inserted_key = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: inserted_key,
            local: insert_key,
        },
        span,
    );
    let fetched_key = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: fetched_key,
            local: fetch_key,
        },
        span,
    );
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    let value = builder.intern_constant(IrConstant::Int(71));
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array,
            key: Some(Operand::Register(inserted_key)),
            value: Operand::Constant(value),
            by_ref_local: None,
        },
        span,
    );
    let fetched = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchDim {
            dst: fetched,
            array: Operand::Register(array),
            key: Operand::Register(fetched_key),
            quiet: false,
            mode: php_ir::instruction::DimFetchMode::Read,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(fetched)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.direct-string-key-array").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: test_array_new as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_array_fetch: forbidden_cached_array_fetch as *const () as usize,
            native_local_fetch: passthrough_local_fetch as *const () as usize,
            native_value_release: frame_cleanup_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing direct string-key handle");
    assert_optimizing_artifact(&handle);
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let left = b"key";
    let right = b"key";
    for (slot, bytes) in direct_slots[..2]
        .iter_mut()
        .zip([left.as_slice(), right.as_slice()])
    {
        *slot = crate::JitNativeValueSlot {
            refcount: 1,
            kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            payload: bytes.len() as u64,
            aux: bytes.as_ptr() as usize as u64,
            ..crate::JitNativeValueSlot::default()
        };
    }
    let (mut next_slot, mut next_entry) = (2, 0);
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut next_slot) as usize as u64,
        direct_array_entries: direct_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut next_entry) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let first = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let second = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[first, second], JIT_RUNTIME_ABI_HASH)
            .expect("baseline equal string-key lookup"),
        71
    );
    assert_eq!(ARRAY_FETCH_FALLBACK_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(direct_slots[2].payload, 1);
    assert_eq!(direct_entries[0].key, first);
}

#[test]
fn optimizing_constant_key_transition_preserves_array_register_identity() {
    let mut builder = IrBuilder::new(UnitId::new(4_207_5));
    let file = builder.add_file("optimizing-constant-key-transition.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_constant_key_transition",
        FunctionFlags::default(),
        span,
    );
    let first_key_local = untyped_param(&mut builder, function, "first_key");
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    let key = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: key,
            local: first_key_local,
        },
        span,
    );
    let nested = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: nested },
        span,
    );
    let nested_value = builder.intern_constant(IrConstant::Int(41));
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array: nested,
            key: None,
            value: Operand::Constant(nested_value),
            by_ref_local: None,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array,
            key: Some(Operand::Register(key)),
            value: Operand::Register(nested),
            by_ref_local: None,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(key),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(nested),
        },
        span,
    );
    let second_key_constant = builder.intern_constant(IrConstant::String("selector".to_owned()));
    let second_key = builder.alloc_register(function);
    builder.emit_load_const(function, block, second_key, second_key_constant, span);
    let null = builder.intern_constant(IrConstant::Null);
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array,
            key: Some(Operand::Register(second_key)),
            value: Operand::Constant(null),
            by_ref_local: None,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(array)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.constant-key-transition").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing constant-key handle");
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let key_bytes = b"path";
    direct_slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: key_bytes.len() as u64,
        aux: key_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let (mut next_slot, mut next_entry) = (1, 0);
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut next_slot) as usize as u64,
        direct_array_entries: direct_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut next_entry) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let first_key = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let outcome = handle
        .invoke_i64_with_deopt(&[first_key], JIT_RUNTIME_ABI_HASH)
        .expect("constant string key must exit to baseline");
    let crate::JitI64InvokeOutcome::SideExit { status, state, .. } = outcome else {
        panic!("constant string key unexpectedly stayed in optimizing code");
    };
    assert_eq!(status, crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32);
    assert!(
        (0..crate::JIT_DEOPT_MAX_REGISTERS).any(|slot| {
            state.initialized_register_mask & (1_u64 << slot) != 0
                && state.register_ids[slot] == second_key.raw()
        }),
        "the transition must be the second keyed insert, after the first direct insert"
    );
    let slot = (0..crate::JIT_DEOPT_MAX_REGISTERS)
        .find(|slot| {
            state.initialized_register_mask & (1_u64 << slot) != 0
                && state.register_ids[*slot] == array.raw()
        })
        .expect("outer array must be live across the transition");
    assert_eq!(
        (state.registers[slot] as u64) & crate::JIT_VALUE_RUNTIME_KIND_MASK,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
        "the array register must not alias the adjacent constant-key register"
    );
    assert_eq!(
        state.registers[slot] as u64,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG
            | u64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1),
        "the outer array register must not alias the nested direct array"
    );
}

#[test]
fn constant_key_transition_restores_array_into_the_baseline_register() {
    let mut builder = IrBuilder::new(UnitId::new(4_207_6));
    let file = builder.add_file("constant-key-baseline-transition.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "constant_key_baseline_transition",
        FunctionFlags::default(),
        span,
    );
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    let key_constant = builder.intern_constant(IrConstant::String("path".to_owned()));
    let key = builder.alloc_register(function);
    builder.emit_load_const(function, block, key, key_constant, span);
    let value = builder.intern_constant(IrConstant::Int(41));
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array,
            key: Some(Operand::Register(key)),
            value: Operand::Constant(value),
            by_ref_local: None,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(array)), span);
    let unit = builder.finish();
    let helpers = crate::JitRuntimeHelperAddresses {
        native_array_insert: test_keyed_array_insert_returns_array as *const () as usize,
        ..crate::JitRuntimeHelperAddresses::default()
    };
    let mut backend = CraneliftNativeCompiler;
    let baseline = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.constant-key-transition.baseline"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: helpers,
    });
    let optimized = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.constant-key-transition.optimized").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: helpers,
    });
    assert_eq!(baseline.status, JitCompileStatus::Compiled, "{baseline:?}");
    assert_eq!(
        optimized.status,
        JitCompileStatus::Compiled,
        "{optimized:?}"
    );
    let baseline = baseline.handle.expect("baseline transition owner");
    let optimized = optimized.handle.expect("optimizing transition owner");
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = activate_direct_test_arena(
        &mut direct_slots,
        &mut next_slot,
        &mut direct_entries,
        &mut next_entry,
    );
    let outcome = optimized
        .invoke_i64_with_native_transition(&baseline, &[], JIT_RUNTIME_ABI_HASH)
        .expect("constant string key should resume in baseline native code");
    let crate::JitI64InvokeOutcome::Returned(value) = outcome else {
        panic!("baseline continuation did not return");
    };
    assert_eq!(
        value as u64,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
        "the baseline loader restored the adjacent constant key into the array register"
    );
}

#[test]
fn optimizing_array_append_keeps_promoted_array_out_of_local_helpers() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_210));
    let file = builder.add_file("optimizing-array-append.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_array_append", FunctionFlags::default(), span);
    let local = builder.intern_local(function, "items");
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::StoreLocal {
            local,
            src: Operand::Register(array),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(array),
        },
        span,
    );
    let nine = builder.intern_constant(IrConstant::Int(9));
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::AppendDim {
            dst: result,
            local,
            dims: Vec::new(),
            value: Operand::Constant(nine),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.array-append").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: test_array_new as *const () as usize,
            native_array_insert: test_array_insert as *const () as usize,
            native_array_insert_local: test_array_insert as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_local_store: forbidden_local_store as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = activate_direct_test_arena(
        &mut direct_slots,
        &mut next_slot,
        &mut direct_entries,
        &mut next_entry,
    );
    let handle = outcome.handle.expect("optimizing array append handle");
    assert_optimizing_artifact(&handle);
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("optimizing array append execution"),
        9
    );
    assert_eq!(direct_slots[0].refcount, 0);
    assert_eq!(direct_slots[0].kind, crate::JIT_NATIVE_VALUE_VIEW_NONE);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_array_growth_stays_native_past_initial_capacity() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_225));
    let file = builder.add_file("optimizing-array-growth.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_array_growth", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    for value in 0..20_i64 {
        let value = builder.intern_constant(IrConstant::Int(value));
        builder.emit(
            function,
            block,
            InstructionKind::ArrayInsert {
                array,
                key: None,
                value: Operand::Constant(value),
                by_ref_local: None,
            },
            span,
        );
    }
    let key = builder.intern_constant(IrConstant::Int(19));
    let fetched = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchDim {
            dst: fetched,
            array: Operand::Register(array),
            key: Operand::Constant(key),
            quiet: false,
            mode: php_ir::instruction::DimFetchMode::Read,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(fetched)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.array-growth").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: forbidden_release as *const () as usize,
            native_array_insert: forbidden_release as *const () as usize,
            native_array_fetch: forbidden_release as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing array growth handle");
    assert_optimizing_artifact(&handle);
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = activate_direct_test_arena(
        &mut direct_slots,
        &mut next_slot,
        &mut direct_entries,
        &mut next_entry,
    );
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("grown array execution"),
        19
    );
    assert_eq!(direct_slots[0].reserved, 32);
    assert_eq!(direct_slots[0].payload, 20);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_direct_array_foreach_has_no_runtime_helper_import() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    FOREACH_NEXT_FALLBACK_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_224));
    let file = builder.add_file("optimizing-direct-array-foreach.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_direct_array_foreach",
        FunctionFlags::default(),
        span,
    );
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    let value = builder.intern_constant(IrConstant::Int(73));
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array,
            key: None,
            value: Operand::Constant(value),
            by_ref_local: None,
        },
        span,
    );
    let iterator = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::ForeachInit {
            iterator,
            source: Operand::Register(array),
        },
        span,
    );
    let has = builder.alloc_register(function);
    let key = builder.alloc_register(function);
    let next_value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::ForeachNext {
            has_value: has,
            iterator,
            key: Some(key),
            value: next_value,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::ForeachCleanup { iterator },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(next_value)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.direct-array-foreach").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: test_array_new as *const () as usize,
            native_array_insert: test_array_insert as *const () as usize,
            native_foreach_next: forbidden_foreach_next as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing direct foreach handle");
    assert_optimizing_artifact(&handle);
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = activate_direct_test_arena(
        &mut direct_slots,
        &mut next_slot,
        &mut direct_entries,
        &mut next_entry,
    );
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("optimizing direct foreach execution"),
        73
    );
    assert_eq!(next_slot, 2);
    assert_eq!(direct_slots[1].refcount, 0);
    assert_eq!(direct_slots[1].kind, crate::JIT_NATIVE_VALUE_VIEW_NONE);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(FOREACH_NEXT_FALLBACK_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn baseline_direct_array_foreach_executes_without_foreach_helpers() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    FOREACH_NEXT_FALLBACK_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_224_1));
    let file = builder.add_file("baseline-direct-array-foreach.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "baseline_direct_array_foreach",
        FunctionFlags::default(),
        span,
    );
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    let value = builder.intern_constant(IrConstant::Int(73));
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array,
            key: None,
            value: Operand::Constant(value),
            by_ref_local: None,
        },
        span,
    );
    let iterator = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::ForeachInit {
            iterator,
            source: Operand::Register(array),
        },
        span,
    );
    let has = builder.alloc_register(function);
    let key = builder.alloc_register(function);
    let next_value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::ForeachNext {
            has_value: has,
            iterator,
            key: Some(key),
            value: next_value,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::ForeachCleanup { iterator },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(next_value)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.baseline.direct-array-foreach"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: test_array_new as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_foreach_init: forbidden_foreach_init as *const () as usize,
            native_foreach_next: forbidden_foreach_next as *const () as usize,
            native_foreach_cleanup: forbidden_foreach_cleanup as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("baseline direct foreach handle");
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = activate_direct_test_arena(
        &mut direct_slots,
        &mut next_slot,
        &mut direct_entries,
        &mut next_entry,
    );
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("baseline direct foreach execution"),
        73
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(FOREACH_NEXT_FALLBACK_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn baseline_array_append_consumes_plain_local_on_direct_data_plane() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    LOCAL_ARRAY_INSERT_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_211));
    let file = builder.add_file("baseline-local-array-append.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "baseline_local_array_append",
        FunctionFlags::default(),
        span,
    );
    let local = builder.intern_local(function, "items");
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::StoreLocal {
            local,
            src: Operand::Register(array),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(array),
        },
        span,
    );
    let nine = builder.intern_constant(IrConstant::Int(9));
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::AppendDim {
            dst: result,
            local,
            dims: Vec::new(),
            value: Operand::Constant(nine),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.baseline.local-array-append"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_new: test_array_new as *const () as usize,
            native_array_insert: test_array_insert as *const () as usize,
            native_array_insert_local: test_local_array_insert as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_local_store: forbidden_local_store as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let mut value_slots = vec![crate::JitNativeValueSlot::default(); 8];
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let (mut next_slot, mut next_entry) = (0, 0);
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: value_slots.len() as u32,
        value_slots: value_slots.as_mut_ptr() as usize as u64,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut next_slot) as usize as u64,
        direct_array_entries: direct_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut next_entry) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        outcome
            .handle
            .expect("baseline local array append handle")
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("baseline local array append execution"),
        9
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(LOCAL_ARRAY_INSERT_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn production_pipeline_executes_dynamic_return_abi() {
    let (unit, function) = arithmetic_fixture();
    let mut backend = CraneliftNativeCompiler;
    let request = JitCompileRequest::new("cl.no_exec.verified");
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &request,
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("dynamic return ABI native handle");
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("dynamic return ABI executes"),
        20
    );
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
fn native_helpers_publish_symbolic_restart_cache_relocations() {
    let mut builder = IrBuilder::new(UnitId::new(1704));
    let file = builder.add_file("far-native-helper.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("far_native_helper", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    builder.emit(
        function,
        block,
        InstructionKind::RuntimeError {
            diagnostic_id: "E_TEST_FAR_NATIVE_HELPER".to_owned(),
            message: "must compile through a symbolic helper import".to_owned(),
        },
        span,
    );
    builder.terminate_return(function, block, None, span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let request = JitCompileRequest::new("cl.far-native-helper");
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &request,
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_runtime_fatal: 1,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let relocatable = outcome
        .handle
        .as_ref()
        .and_then(crate::JitFunctionHandle::relocatable_code)
        .expect("production lowering must retain relocatable machine code");
    assert!(!relocatable.code.is_empty());
    assert!(relocatable.relocations.iter().any(|relocation| {
        matches!(
            &relocation.target,
            crate::JitRelocatableTarget::Helper(name)
                if name == "phrust_native_runtime_fatal"
        )
    }));
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
            None,
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
    extern "C" fn throwing_trampoline(
        _runtime: *mut std::ffi::c_void,
        _vm_context: u64,
        frame: *mut crate::JitNativeCallFrame,
        out: *mut crate::JitCallResult,
    ) -> i32 {
        assert!(!frame.is_null());
        assert!(!out.is_null());
        // SAFETY: The generated call owns this synchronous result record.
        unsafe {
            out.write(crate::JitCallResult {
                status: crate::JitCallStatus::THROW,
                detail: 0,
                value: crate::JitAbiSlot {
                    tag: 3,
                    flags: 0,
                    payload: 33,
                },
            });
        }
        crate::JitCallStatus::THROW.0 as i32
    }

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
    let thrown = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: thrown,
            name: "runtime_throw".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_jump(function, entry, after, span);
    let caught = builder.alloc_register(function);
    builder.emit(
        function,
        catch,
        InstructionKind::LoadLocal {
            dst: caught,
            local: exception_local,
        },
        span,
    );
    builder.terminate_return(function, catch, Some(Operand::Register(caught)), span);
    let fallback = builder.intern_constant(IrConstant::Int(0));
    builder.terminate_return(function, after, Some(Operand::Constant(fallback)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-catch"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: throwing_trampoline as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let native = outcome
        .handle
        .expect("native catch handle")
        .invoke_i64_with_native_unwind(&[], JIT_RUNTIME_ABI_HASH, |types, value| {
            value == 33 && types == ["runtimeexception"]
        })
        .expect("explicit native unwind");
    assert_eq!(native, crate::JitI64InvokeOutcome::Returned(33));
}

#[test]
fn native_unwind_catches_throw_from_direct_compiled_callee() {
    extern "C" fn throwing_trampoline(
        _runtime: *mut std::ffi::c_void,
        _vm_context: u64,
        frame: *mut crate::JitNativeCallFrame,
        out: *mut crate::JitCallResult,
    ) -> i32 {
        assert!(!frame.is_null());
        assert!(!out.is_null());
        // SAFETY: The generated call owns this synchronous result record.
        unsafe {
            out.write(crate::JitCallResult {
                status: crate::JitCallStatus::THROW,
                detail: 0,
                value: crate::JitAbiSlot {
                    tag: 3,
                    flags: 0,
                    payload: 33,
                },
            });
        }
        crate::JitCallStatus::THROW.0 as i32
    }

    let mut builder = IrBuilder::new(UnitId::new(709));
    let file = builder.add_file("native-direct-call-catch.php");
    let span = IrSpan::new(file, 0, 30);

    let callee = builder.start_function("throwing_callee", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let _callee_value = untyped_param(&mut builder, callee, "value");
    let callee_entry = builder.append_block(callee);
    let trampoline_result = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_entry,
        InstructionKind::CallFunction {
            dst: trampoline_result,
            name: "runtime_throw".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(
        callee,
        callee_entry,
        Some(Operand::Register(trampoline_result)),
        span,
    );
    builder.register_function_name("throwing_callee", callee);

    let caller = builder.start_function("catching_caller", FunctionFlags::default(), span);
    builder.set_entry(caller);
    builder.set_return_type(caller, Some(IrReturnType::Int));
    let caller_value = untyped_param(&mut builder, caller, "value");
    let entry = builder.append_block(caller);
    let catch = builder.append_block(caller);
    let after = builder.append_block(caller);
    let exception_local = builder.intern_local(caller, "exception");
    builder.emit(
        caller,
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
    let argument = builder.alloc_register(caller);
    builder.emit(
        caller,
        entry,
        InstructionKind::LoadLocal {
            dst: argument,
            local: caller_value,
        },
        span,
    );
    let call_result = builder.alloc_register(caller);
    builder.emit(
        caller,
        entry,
        InstructionKind::CallFunction {
            dst: call_result,
            name: "throwing_callee".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(argument),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: Some(caller_value),
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.emit(caller, entry, InstructionKind::LeaveTry, span);
    builder.terminate_jump(caller, entry, after, span);
    let caught = builder.alloc_register(caller);
    builder.emit(
        caller,
        catch,
        InstructionKind::LoadLocal {
            dst: caught,
            local: exception_local,
        },
        span,
    );
    builder.terminate_return(caller, catch, Some(Operand::Register(caught)), span);
    let fallback = builder.intern_constant(IrConstant::Int(0));
    builder.terminate_return(caller, after, Some(Operand::Constant(fallback)), span);

    let unit = builder.finish();
    let graph = crate::region_ir::build_baseline_region(&unit, caller)
        .expect("direct-call catch Region IR");
    assert_eq!(graph.direct_callees(), vec![callee], "{graph:#?}");
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-direct-call-catch"),
        unit: Some(&unit),
        function: Some(caller),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: throwing_trampoline as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let native = outcome
        .handle
        .expect("native direct-call catch handle")
        .invoke_i64_with_native_unwind(&[0], JIT_RUNTIME_ABI_HASH, |types, value| {
            value == 33 && types == ["runtimeexception"]
        })
        .expect("direct callee throw should unwind through caller catch");
    assert_eq!(native, crate::JitI64InvokeOutcome::Returned(33));
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
fn generator_yield_send_and_throw_use_native_resume_entry() {
    let mut builder = IrBuilder::new(UnitId::new(709));
    let file = builder.add_file("native-generator.php");
    let span = IrSpan::new(file, 0, 30);
    let function = builder.start_function(
        "native_generator",
        FunctionFlags {
            is_generator: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.set_return_type(function, Some(IrReturnType::Int));
    let entry = builder.append_block(function);
    let key = builder.intern_constant(IrConstant::Int(3));
    let yielded = builder.intern_constant(IrConstant::Int(9));
    let sent = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::Yield {
            dst: sent,
            key: Some(Operand::Constant(key)),
            value: Some(Operand::Constant(yielded)),
        },
        span,
    );
    builder.terminate_return(function, entry, Some(Operand::Register(sent)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-generator"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native generator handle");
    let crate::JitI64InvokeOutcome::SideExit {
        status,
        value,
        state,
    } = handle
        .invoke_i64_with_deopt(&[], JIT_RUNTIME_ABI_HASH)
        .expect("initial generator entry")
    else {
        panic!("generator did not suspend");
    };
    assert_eq!(status, crate::JitCallStatus::SUSPEND_GENERATOR.0 as i32);
    assert_eq!(value, 9);
    assert_eq!(state.yielded_key, 3);
    assert_eq!(
        state.suspend_kind,
        crate::JitNativeSuspendKind::GENERATOR_YIELD.0
    );
    assert_eq!(
        handle
            .region_state_metadata()
            .expect("generator metadata")
            .suspensions
            .len(),
        1
    );
    assert_eq!(
        handle
            .invoke_i64_suspension_resume(
                &[],
                &state,
                crate::JitNativeResumeInputKind::VALUE,
                42,
                JIT_RUNTIME_ABI_HASH,
            )
            .expect("generator send"),
        crate::JitI64InvokeOutcome::Returned(42)
    );
    let thrown = handle
        .invoke_i64_suspension_resume(
            &[],
            &state,
            crate::JitNativeResumeInputKind::THROW,
            77,
            JIT_RUNTIME_ABI_HASH,
        )
        .expect("throw into generator");
    assert!(matches!(
        thrown,
        crate::JitI64InvokeOutcome::SideExit { status, value: 77, .. }
            if status == crate::JitCallStatus::THROW.0 as i32
    ));
}

#[test]
fn yield_from_publishes_native_delegation_state() {
    let mut builder = IrBuilder::new(UnitId::new(710));
    let file = builder.add_file("native-yield-from.php");
    let span = IrSpan::new(file, 0, 30);
    let function = builder.start_function(
        "native_yield_from",
        FunctionFlags {
            is_generator: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.set_return_type(function, Some(IrReturnType::Int));
    let entry = builder.append_block(function);
    let source = builder.intern_constant(IrConstant::Int(91));
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::YieldFrom {
            dst: result,
            source: Operand::Constant(source),
        },
        span,
    );
    builder.terminate_return(function, entry, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-yield-from"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native yield-from handle");
    let crate::JitI64InvokeOutcome::SideExit { state, .. } = handle
        .invoke_i64_with_deopt(&[], JIT_RUNTIME_ABI_HASH)
        .expect("delegated generator entry")
    else {
        panic!("yield-from did not suspend");
    };
    assert_eq!(state.delegation_handle, 91);
    assert_eq!(state.suspend_flags & (1 << 1), 1 << 1);
    assert_eq!(
        handle
            .invoke_i64_suspension_resume(
                &[],
                &state,
                crate::JitNativeResumeInputKind::VALUE,
                88,
                JIT_RUNTIME_ABI_HASH,
            )
            .expect("delegated return"),
        crate::JitI64InvokeOutcome::Returned(88)
    );
}

#[test]
fn fiber_suspend_and_resume_use_native_continuation() {
    let mut builder = IrBuilder::new(UnitId::new(711));
    let file = builder.add_file("native-fiber.php");
    let span = IrSpan::new(file, 0, 30);
    let function = builder.start_function("native_fiber", FunctionFlags::default(), span);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let entry = builder.append_block(function);
    let suspended = builder.intern_constant(IrConstant::Int(5));
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallStaticMethod {
            dst: result,
            class_name: "Fiber".to_owned(),
            method: "suspend".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Constant(suspended),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(function, entry, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-fiber"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native fiber handle");
    let crate::JitI64InvokeOutcome::SideExit {
        status,
        value,
        state,
    } = handle
        .invoke_i64_with_deopt(&[], JIT_RUNTIME_ABI_HASH)
        .expect("fiber start")
    else {
        panic!("fiber did not suspend");
    };
    assert_eq!(status, crate::JitCallStatus::SUSPEND_FIBER.0 as i32);
    assert_eq!(value, 5);
    assert_eq!(
        handle
            .invoke_i64_suspension_resume(
                &[],
                &state,
                crate::JitNativeResumeInputKind::VALUE,
                44,
                JIT_RUNTIME_ABI_HASH,
            )
            .expect("fiber resume"),
        crate::JitI64InvokeOutcome::Returned(44)
    );
}

#[test]
fn generator_resume_runs_compiled_finally() {
    let mut builder = IrBuilder::new(UnitId::new(712));
    let file = builder.add_file("generator-finally.php");
    let span = IrSpan::new(file, 0, 40);
    let function = builder.start_function(
        "generator_finally",
        FunctionFlags {
            is_generator: true,
            ..FunctionFlags::default()
        },
        span,
    );
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
    let yielded = builder.intern_constant(IrConstant::Int(1));
    let sent = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::Yield {
            dst: sent,
            key: None,
            value: Some(Operand::Constant(yielded)),
        },
        span,
    );
    builder.terminate_return(function, entry, Some(Operand::Register(sent)), span);
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
        compile: &JitCompileRequest::new("cl.generator-finally"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("generator finally handle");
    let crate::JitI64InvokeOutcome::SideExit { state, .. } = handle
        .invoke_i64_with_deopt(&[], JIT_RUNTIME_ABI_HASH)
        .expect("generator suspension")
    else {
        panic!("generator did not suspend");
    };
    assert_eq!(
        handle
            .invoke_i64_suspension_resume(
                &[],
                &state,
                crate::JitNativeResumeInputKind::VALUE,
                64,
                JIT_RUNTIME_ABI_HASH,
            )
            .expect("generator finally resume"),
        crate::JitI64InvokeOutcome::Returned(64)
    );
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
fn baseline_and_default_policies_both_execute_native_code() {
    let (unit, function) = constant_return_fixture();
    for (preset, opt_level) in [("baseline", 0), ("default", 2)] {
        let mut compiler = CraneliftNativeCompiler;
        let request =
            JitCompileRequest::new(format!("cl.policy.{preset}")).with_opt_level(opt_level);
        let outcome = compiler.compile_region(&NativeCompileRequest {
            compile: &request,
            unit: Some(&unit),
            function: Some(function),
            runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
        });
        assert_eq!(outcome.status, JitCompileStatus::Compiled, "{preset}");
        assert_eq!(
            outcome
                .handle
                .expect("policy must publish native code")
                .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
                .expect("policy native entry must execute"),
            42,
            "{preset}",
        );
    }
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
fn cranelift_backend_executes_region_ir_without_whole_function_candidate_gate() {
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
fn function_scoped_compile_routes_same_unit_callee_through_trampoline() {
    extern "C" fn trampoline(
        _runtime: *mut std::ffi::c_void,
        _vm_context: u64,
        frame: *mut crate::JitNativeCallFrame,
        out: *mut crate::JitCallResult,
    ) -> i32 {
        assert!(!frame.is_null());
        assert!(!out.is_null());
        // SAFETY: The generated call owns both records synchronously.
        unsafe {
            out.write(crate::JitCallResult {
                status: crate::JitCallStatus::RETURN,
                detail: 0,
                value: crate::JitAbiSlot {
                    tag: 3,
                    flags: 0,
                    payload: 42,
                },
            });
        }
        crate::JitCallStatus::RETURN.0 as i32
    }

    let (unit, function, callee) = scalar_direct_call_fixture();
    let mut backend = CraneliftNativeCompiler;
    let request = JitCompileRequest::new("cl.region.direct-call");
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
    assert!(outcome.diagnostics[0].contains("plan_php_blocks="));
    let handle = outcome.handle.expect("direct-call region should compile");
    assert_eq!(handle.compiled_to_compiled_calls_per_invocation(), 0);
    let metadata = handle.region_state_metadata().expect("region metadata");
    assert_eq!(metadata.function_entries.len(), 1);
    assert_eq!(metadata.function_entries[0].function, function);
    assert!(
        metadata
            .continuations
            .iter()
            .all(|entry| entry.function == function)
    );
    assert!(
        metadata
            .continuations
            .iter()
            .all(|entry| entry.function != callee)
    );
    assert_eq!(
        metadata.native_transitions.len(),
        3,
        "only the guarded load, call continuation, and typed return may transition"
    );
    assert_eq!(metadata.native_transitions[0].function, function);
    assert_eq!(
        handle
            .invoke_i64(&[41], JIT_RUNTIME_ABI_HASH)
            .expect("native caller should dispatch the callee"),
        42
    );
}

#[test]
fn published_same_unit_entry_bypasses_the_warm_resolver() {
    extern "C" fn forbidden_resolver(
        _runtime: *mut std::ffi::c_void,
        _vm_context: u64,
        _function: u64,
        _out: *mut usize,
    ) -> i32 {
        panic!("published same-unit entry unexpectedly entered the resolver")
    }

    #[allow(unsafe_code)]
    extern "C" fn published_callee(
        _runtime: *mut std::ffi::c_void,
        _arguments: *const i64,
        out: *mut i64,
        _deopt: *mut crate::JitDeoptState,
        _resume_id: i32,
        _resume_state: *mut std::ffi::c_void,
    ) -> i32 {
        // SAFETY: the generated caller owns this result slot synchronously.
        unsafe { out.write(42) };
        crate::JitCallStatus::RETURN.0 as i32
    }

    #[allow(unsafe_code)]
    extern "C" fn replacement_callee(
        _runtime: *mut std::ffi::c_void,
        _arguments: *const i64,
        out: *mut i64,
        _deopt: *mut crate::JitDeoptState,
        _resume_id: i32,
        _resume_state: *mut std::ffi::c_void,
    ) -> i32 {
        // SAFETY: the generated caller owns this result slot synchronously.
        unsafe { out.write(84) };
        crate::JitCallStatus::RETURN.0 as i32
    }

    extern "C" fn forbidden_optimizing_callee(
        _runtime: *mut std::ffi::c_void,
        _arguments: *const i64,
        _out: *mut i64,
        _deopt: *mut crate::JitDeoptState,
        _resume_id: i32,
        _resume_state: *mut std::ffi::c_void,
    ) -> i32 {
        panic!("baseline continuation re-entered an optimizing callee")
    }

    let (unit, function, callee) = scalar_direct_call_fixture();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.region.cached-direct-call"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_function_resolve: forbidden_resolver as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("cached direct-call region");

    let mut entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let mut optimizing_entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    entries[callee.index()].store(
        published_callee as *const () as usize,
        std::sync::atomic::Ordering::Release,
    );
    optimizing_entries[callee.index()].store(
        forbidden_optimizing_callee as *const () as usize,
        std::sync::atomic::Ordering::Release,
    );
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        trusted_function_entries: entries.as_mut_ptr() as usize as u64,
        trusted_function_entry_count: entries.len() as u32,
        trusted_optimizing_function_entries: optimizing_entries.as_mut_ptr() as usize as u64,
        trusted_optimizing_function_entry_count: optimizing_entries.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        handle
            .invoke_i64(&[41], JIT_RUNTIME_ABI_HASH)
            .expect("published direct callee execution"),
        42
    );
    entries[callee.index()].store(
        replacement_callee as *const () as usize,
        std::sync::atomic::Ordering::Release,
    );
    optimizing_entries[callee.index()].store(
        forbidden_optimizing_callee as *const () as usize,
        std::sync::atomic::Ordering::Release,
    );
    assert_eq!(
        handle
            .invoke_i64(&[41], JIT_RUNTIME_ABI_HASH)
            .expect("replacement direct callee execution"),
        84,
        "compiled caller cached the old baseline address instead of reloading the publication cell"
    );
}

#[test]
fn direct_call_resumes_nested_compile_transitions_before_returning_to_caller() {
    extern "C" fn forbidden_resolver(
        _runtime: *mut std::ffi::c_void,
        _vm_context: u64,
        _function: u64,
        _out: *mut usize,
    ) -> i32 {
        panic!("published nested-transition callee unexpectedly entered the resolver")
    }

    #[allow(unsafe_code)]
    extern "C" fn transitioning_callee(
        _runtime: *mut std::ffi::c_void,
        _arguments: *const i64,
        out: *mut i64,
        deopt: *mut crate::JitDeoptState,
        _resume_id: i32,
        _resume_state: *mut std::ffi::c_void,
    ) -> i32 {
        let call = NESTED_TRANSITION_CALLS.fetch_add(1, Ordering::SeqCst);
        if call < 2 {
            // SAFETY: the generated caller owns both records for the
            // synchronous direct call and passes the same transition state
            // back to the published baseline entry.
            unsafe {
                (*deopt).function_id = NESTED_TRANSITION_FUNCTION.load(Ordering::SeqCst) as u32;
                (*deopt).continuation_id = (call + 1) as u32;
                out.write(40 + call as i64);
            }
            return crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32;
        }
        // SAFETY: the result slot remains caller-owned for this invocation.
        unsafe { out.write(84) };
        crate::JitCallStatus::RETURN.0 as i32
    }

    let (unit, function, callee) = scalar_direct_call_fixture();
    NESTED_TRANSITION_CALLS.store(0, Ordering::SeqCst);
    NESTED_TRANSITION_FUNCTION.store(callee.index(), Ordering::SeqCst);
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.region.nested-transition"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_function_resolve: forbidden_resolver as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("nested-transition direct caller");
    let mut entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    entries[callee.index()].store(
        transitioning_callee as *const () as usize,
        std::sync::atomic::Ordering::Release,
    );
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        trusted_function_entries: entries.as_mut_ptr() as usize as u64,
        trusted_function_entry_count: entries.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        handle
            .invoke_i64(&[41], JIT_RUNTIME_ABI_HASH)
            .expect("nested transitions resume inside the direct caller"),
        84,
        "an intermediate compile-on-demand result escaped as the PHP function return"
    );
    assert_eq!(NESTED_TRANSITION_CALLS.load(Ordering::SeqCst), 3);
}

#[test]
fn cranelift_packed_region_abi_supports_more_than_sixteen_arguments() {
    let (unit, function) = wide_parameter_fixture();
    let mut backend = CraneliftNativeCompiler;
    let request = JitCompileRequest::new("cl.region.packed-arguments");
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &request,
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("wide native region should compile");
    let arguments = (0_i64..18).collect::<Vec<_>>();
    assert_eq!(
        handle
            .invoke_i64(&arguments, JIT_RUNTIME_ABI_HASH)
            .expect("packed arguments should execute"),
        17
    );
}

#[test]
fn cranelift_dynamic_call_uses_typed_native_trampoline() {
    extern "C" fn trampoline(
        _runtime: *mut std::ffi::c_void,
        _vm_context: u64,
        frame: *mut crate::JitNativeCallFrame,
        out: *mut crate::JitCallResult,
    ) -> i32 {
        assert!(!frame.is_null());
        assert!(!out.is_null());
        // SAFETY: The generated call owns both ABI records for this
        // synchronous test invocation.
        let frame = unsafe { &*frame };
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
fn published_external_by_value_signature_skips_reference_probe() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let (unit, function) = external_lvalue_call_fixture();
    let mut backend = CraneliftNativeCompiler;
    let request = JitCompileRequest::new("cl.region.external-by-value")
        .with_external_function_signatures(vec![crate::JitExternalFunctionSignature {
            name: "deployment_function".to_owned(),
            params: vec![crate::JitExternalParameterSignature {
                name: "value".to_owned(),
                by_ref: false,
                variadic: false,
            }],
        }]);
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &request,
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: return_first_call_argument as *const () as usize,
            native_reference_bind: forbidden_reference_bind as *const () as usize,
            native_local_fetch: passthrough_local_fetch as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    assert_eq!(
        outcome
            .handle
            .expect("external by-value call")
            .invoke_i64(&[41], JIT_RUNTIME_ABI_HASH)
            .expect("published by-value call executes"),
        41
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn include_executes_only_after_native_dynamic_compiler_returns_entry_result() {
    let (unit, function) = scalar_native_include_fixture();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.region.native-include"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_dynamic_code: test_native_dynamic_code as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native include should compile");
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("native include entry should execute"),
        123
    );
}

#[test]
fn eval_executes_only_after_native_dynamic_compiler_returns_entry_result() {
    let (unit, function) = scalar_native_eval_fixture();
    let mut compiler = CraneliftNativeCompiler;
    let outcome = compiler.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.region.native-eval"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_dynamic_code: test_native_dynamic_code as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    assert_eq!(
        outcome
            .handle
            .expect("native eval should compile")
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("native eval entry should execute"),
        321,
    );
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
fn optimizing_overflow_materializes_precise_region_continuation() {
    let (unit, function) = helper_overflow_fixture();
    let mut backend = CraneliftNativeCompiler;
    let request = JitCompileRequest::new("cl.region.deopt-state").with_opt_level(2);
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &request,
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled);
    let handle = outcome.handle.expect("overflow region should compile");
    assert_optimizing_artifact(&handle);
    let metadata = handle
        .region_state_metadata()
        .expect("executable regions publish state metadata");
    assert!(!metadata.continuations.is_empty());
    assert!(!metadata.native_pc_ranges.is_empty());
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_next = crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY as u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
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
fn native_side_exit_bounds_published_registers_to_abi_capacity() {
    let mut builder = IrBuilder::new(UnitId::new(709));
    let file = builder.add_file("native-register-capacity.php");
    let span = IrSpan::new(file, 0, 80);
    let function =
        builder.start_function("native_register_capacity", FunctionFlags::default(), span);
    builder.set_entry(function);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let argument = typed_int_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    for value in 0..70 {
        let constant = builder.add_constant(IrConstant::Int(value));
        let register = builder.alloc_register(function);
        builder.emit_load_const(function, block, register, constant, span);
    }
    let source = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: source,
            local: argument,
        },
        span,
    );
    let one = builder.add_constant(IrConstant::Int(1));
    let increment = builder.alloc_register(function);
    builder.emit_load_const(function, block, increment, one, span);
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(source),
            rhs: Operand::Register(increment),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.region.register-capacity").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome
        .handle
        .expect("large-register region should compile");
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_next = crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY as u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let crate::JitI64InvokeOutcome::SideExit { status, state, .. } = handle
        .invoke_i64_with_deopt(&[i64::MAX], JIT_RUNTIME_ABI_HASH)
        .expect("overflow should side-exit without corrupting the native frame")
    else {
        panic!("overflow unexpectedly returned");
    };
    assert_eq!(status, crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32);
    assert_eq!(state.initialized_register_mask, 0b11);
    assert_eq!(state.registers[0], i64::MAX);
    assert_eq!(state.registers[1], 1);
}

#[test]
fn ordinary_instructions_do_not_create_resume_or_clif_entry_blocks() {
    let mut builder = IrBuilder::new(UnitId::new(710));
    let file = builder.add_file("native-resume-loader-capacity.php");
    let span = IrSpan::new(file, 0, 80);
    let function = builder.start_function(
        "native_resume_loader_capacity",
        FunctionFlags::default(),
        span,
    );
    builder.set_entry(function);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let block = builder.append_block(function);
    let mut result = None;
    for value in 0..512 {
        let constant = builder.add_constant(IrConstant::Int(value));
        let register = builder.alloc_register(function);
        builder.emit_load_const(function, block, register, constant, span);
        result = Some(register);
    }
    builder.terminate_return(function, block, result.map(Operand::Register), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.region.resume-loader-capacity"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let compile_diagnostic = outcome.diagnostics[0].clone();
    let handle = outcome
        .handle
        .expect("large-register region should compile");
    let max_fragment_clif_blocks = outcome.diagnostics[0]
        .split_ascii_whitespace()
        .find_map(|field| field.strip_prefix("max_fragment_clif_blocks="))
        .and_then(|value| value.parse::<usize>().ok())
        .expect("compile diagnostic must report the actual maximum fragment CLIF block count");
    assert!(
        max_fragment_clif_blocks <= 16,
        "a straight-line fragment may add bounded entry/return and fragment-edge plumbing, not one block per instruction: {max_fragment_clif_blocks} blocks; {compile_diagnostic}"
    );
    assert!(
        handle.code_bytes() < 64_000,
        "straight-line code grew beyond the instruction-block-free budget: {} bytes",
        handle.code_bytes()
    );
    let metadata = handle
        .region_state_metadata()
        .expect("resume loader metadata");
    let terminator_continuations = metadata
        .continuations
        .iter()
        .filter(|entry| entry.instruction.is_none())
        .map(|entry| entry.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        metadata
            .native_transitions
            .iter()
            .all(|entry| terminator_continuations.contains(&entry.continuation_id)),
        "pure constant-load instructions must not advertise native resume transitions"
    );
    assert!(
        metadata.native_transitions.iter().all(|transition| {
            transition.live_registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS
        })
    );
    assert_eq!(
        handle.invoke_i64(&[], JIT_RUNTIME_ABI_HASH),
        Ok(511),
        "bounded resume metadata must not change normal native execution"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn oversized_php_cfg_compiles_as_bounded_direct_native_fragments() {
    let mut builder = IrBuilder::new(UnitId::new(711));
    let file = builder.add_file("native-fragments.php");
    let span = IrSpan::new(file, 0, 80);
    let function = builder.start_function("native_fragments", FunctionFlags::default(), span);
    builder.set_entry(function);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let blocks = (0..300)
        .map(|_| builder.append_block(function))
        .collect::<Vec<_>>();
    for (index, block) in blocks.iter().copied().enumerate() {
        let constant = builder.add_constant(IrConstant::Int(index as i64));
        let value = builder.alloc_register(function);
        builder.emit_load_const(function, block, value, constant, span);
        if let Some(next) = blocks.get(index + 1).copied() {
            builder.terminate_jump(function, block, next, span);
        } else {
            builder.terminate_return(function, block, Some(Operand::Register(value)), span);
        }
    }
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.region.bounded-fragments"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    assert!(
        outcome.diagnostics[0].contains("plan_fragments=5"),
        "{outcome:?}"
    );
    let handle = outcome.handle.expect("fragmented function handle");
    assert_eq!(handle.invoke_i64(&[], JIT_RUNTIME_ABI_HASH), Ok(299));
    let metadata = handle.region_state_metadata().expect("fragment metadata");
    assert_eq!(metadata.function_entries.len(), 1);
    assert_eq!(metadata.function_entries[0].function, function);
    let relocatable = handle.relocatable_code().expect("fragment artifact");
    assert_eq!(relocatable.root, function);
    assert_eq!(relocatable.functions.len(), 6);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn implicit_method_receiver_survives_native_fragment_boundary() {
    let mut builder = IrBuilder::new(UnitId::new(713));
    let file = builder.add_file("native-method-fragments.php");
    let span = IrSpan::new(file, 0, 80);
    let function = builder.start_function(
        "Fixture::receiver",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.set_entry(function);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let this = builder.intern_local(function, "this");
    let blocks = (0..300)
        .map(|_| builder.append_block(function))
        .collect::<Vec<_>>();
    for (index, block) in blocks.iter().copied().enumerate() {
        if let Some(next) = blocks.get(index + 1).copied() {
            builder.terminate_jump(function, block, next, span);
        } else {
            let receiver = builder.alloc_register(function);
            builder.emit(
                function,
                block,
                InstructionKind::LoadLocal {
                    dst: receiver,
                    local: this,
                },
                span,
            );
            builder.terminate_return(function, block, Some(Operand::Register(receiver)), span);
        }
    }
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "fixture".to_owned(),
        display_name: "Fixture".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "receiver".to_owned(),
            origin_class: "fixture".to_owned(),
            function,
            flags: ClassMethodFlags {
                has_body: true,
                ..ClassMethodFlags::default()
            },
            attributes: Vec::new(),
        }],
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor: None,
        flags: ClassFlags::default(),
        span,
    });
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.region.fragment-method-receiver"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    assert!(
        outcome.diagnostics[0].contains("plan_fragments=5"),
        "{outcome:?}"
    );
    let handle = outcome.handle.expect("fragmented method handle");
    assert_eq!(handle.invoke_i64(&[73], JIT_RUNTIME_ABI_HASH), Ok(73));
    assert_eq!(
        handle
            .region_state_metadata()
            .expect("fragment metadata")
            .compiler_tier,
        crate::region_ir::NativeCompilerTier::Baseline
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn cross_fragment_backedge_does_not_alias_osr_entry_zero() {
    let mut builder = IrBuilder::new(UnitId::new(712));
    let file = builder.add_file("native-fragment-backedge.php");
    let span = IrSpan::new(file, 0, 80);
    let function =
        builder.start_function("native_fragment_backedge", FunctionFlags::default(), span);
    builder.set_entry(function);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let blocks = (0..300)
        .map(|_| builder.append_block(function))
        .collect::<Vec<_>>();
    builder.terminate_jump(function, blocks[0], blocks[299], span);
    for (index, block) in blocks.iter().copied().enumerate().skip(1).take(298) {
        let constant = builder.add_constant(IrConstant::Int(if index == 1 { 42 } else { 0 }));
        let value = builder.alloc_register(function);
        builder.emit_load_const(function, block, value, constant, span);
        builder.terminate_return(function, block, Some(Operand::Register(value)), span);
    }
    builder.terminate_jump(function, blocks[299], blocks[1], span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.region.fragment-backedge"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    assert!(
        outcome.diagnostics[0].contains("plan_fragments=5"),
        "{outcome:?}"
    );
    let handle = outcome.handle.expect("fragmented backedge handle");
    assert_eq!(handle.invoke_i64(&[], JIT_RUNTIME_ABI_HASH), Ok(42));
    let osr = handle
        .region_state_metadata()
        .and_then(|metadata| metadata.osr_entries.first())
        .expect("backedge must publish an OSR entry");
    assert_eq!(osr.id, 0);
    assert_eq!(osr.block, blocks[1]);
}

#[test]
fn baseline_native_continuation_resumes_exact_instruction() {
    let (unit, function) = helper_overflow_fixture();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-transition.baseline"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("baseline handle");
    let metadata = handle.region_state_metadata().expect("transition metadata");
    assert_eq!(
        metadata.compiler_tier,
        crate::region_ir::NativeCompilerTier::Baseline
    );
    let transition = metadata
        .native_transitions
        .iter()
        .find(|transition| transition.live_registers.len() == 2)
        .expect("binary continuation");
    let mut state = crate::JitNativeTransitionState {
        function_id: function.raw(),
        continuation_id: transition.continuation_id,
        slot_count: metadata.local_count,
        ..crate::JitNativeTransitionState::default()
    };
    for local in &transition.live_locals {
        state.mark_local_initialized(*local);
        state.slots[local.index()] = 41;
    }
    for (snapshot_slot, register) in transition.live_registers.iter().enumerate() {
        state.initialized_register_mask |= 1_u64 << snapshot_slot;
        state.register_ids[snapshot_slot] = register.raw();
        state.registers[snapshot_slot] = if snapshot_slot == 0 { 41 } else { 1 };
    }
    assert_eq!(
        handle
            .invoke_i64_native_transition(&state, JIT_RUNTIME_ABI_HASH)
            .expect("baseline continuation should execute"),
        crate::JitI64InvokeOutcome::Returned(42)
    );
}

#[test]
fn function_scoped_compile_publishes_only_requested_transition_metadata() {
    let (unit, _root, callee) = scalar_direct_call_fixture();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-transition.single-function"),
        unit: Some(&unit),
        function: Some(callee),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("compiled function");
    let metadata = handle.region_state_metadata().expect("transition metadata");
    assert_eq!(metadata.function_entries.len(), 1);
    assert_eq!(metadata.function_entries[0].function, callee);
    assert!(
        metadata
            .continuations
            .iter()
            .all(|entry| entry.function == callee)
    );
    let transition = metadata
        .native_transitions
        .iter()
        .find(|transition| transition.function == callee && transition.live_registers.len() == 1)
        .expect("callee add continuation");
    let mut state = crate::JitNativeTransitionState {
        function_id: callee.raw(),
        continuation_id: transition.continuation_id,
        slot_count: metadata.local_count,
        ..crate::JitNativeTransitionState::default()
    };
    for local in &transition.live_locals {
        state.mark_local_initialized(*local);
        state.slots[local.index()] = 41;
    }
    for (snapshot_slot, register) in transition.live_registers.iter().enumerate() {
        state.initialized_register_mask |= 1_u64 << snapshot_slot;
        state.register_ids[snapshot_slot] = register.raw();
        state.registers[snapshot_slot] = 41;
    }
    assert_eq!(
        handle
            .invoke_i64_native_transition(&state, JIT_RUNTIME_ABI_HASH)
            .expect("callee transition should execute"),
        crate::JitI64InvokeOutcome::Returned(42)
    );
}

#[test]
fn optimizer_transitions_once_to_dynamic_baseline_without_repeating_effect() {
    NATIVE_DYNAMIC_EFFECTS.store(0, Ordering::SeqCst);
    let (unit, function) = effect_then_direct_fixture();
    let helpers = crate::JitRuntimeHelperAddresses {
        native_dynamic_code: test_native_dynamic_code as *const () as usize,
        ..crate::JitRuntimeHelperAddresses::default()
    };
    let mut backend = CraneliftNativeCompiler;
    let baseline = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.transition.effect.baseline"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: helpers,
    });
    let optimized_request =
        JitCompileRequest::new("cl.transition.effect.optimized").with_opt_level(2);
    let optimized = backend.compile_region(&NativeCompileRequest {
        compile: &optimized_request,
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: helpers,
    });
    assert_eq!(baseline.status, JitCompileStatus::Compiled, "{baseline:?}");
    assert_eq!(
        optimized.status,
        JitCompileStatus::Compiled,
        "{optimized:?}"
    );
    let optimized = optimized.handle.expect("requested optimizing handle");
    assert_eq!(
        optimized
            .region_state_metadata()
            .expect("native metadata")
            .compiler_tier,
        crate::region_ir::NativeCompilerTier::Optimizing,
        "one baseline-only operation must not downgrade the complete PHP function"
    );
    let baseline = baseline.handle.expect("baseline island owner");
    let mut baseline_entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let mut optimizing_entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    baseline_entries[function.index()].store(
        baseline
            .native_entry_address()
            .expect("baseline executable address"),
        std::sync::atomic::Ordering::Release,
    );
    optimizing_entries[function.index()].store(
        optimized
            .native_entry_address()
            .expect("optimizing executable address"),
        std::sync::atomic::Ordering::Release,
    );
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        trusted_function_entries: baseline_entries.as_mut_ptr() as usize as u64,
        trusted_function_entry_count: baseline_entries.len() as u32,
        trusted_optimizing_function_entries: optimizing_entries.as_mut_ptr() as usize as u64,
        trusted_optimizing_function_entry_count: optimizing_entries.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    let outcome = optimized
        .invoke_i64_with_native_transition(&baseline, &[], JIT_RUNTIME_ABI_HASH)
        .expect("dynamic operation should transition once through baseline native code");
    assert_eq!(outcome, crate::JitI64InvokeOutcome::Returned(42));
    assert_eq!(NATIVE_DYNAMIC_EFFECTS.load(Ordering::SeqCst), 1);
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
    let file =
        builder.add_file("tests/fixtures/performance/cranelift/helper-call/add-mul-expression.php");
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
    let function = builder.start_function("jit_scalar_identity", FunctionFlags::default(), span);
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
    let callee_value = untyped_param(&mut builder, callee, "value");
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
    let caller_value = untyped_param(&mut builder, caller, "value");
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

fn wide_parameter_fixture() -> (php_ir::IrUnit, FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("packed-arguments.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("wide_parameters", FunctionFlags::default(), span);
    builder.set_entry(function);
    let parameters = (0..18)
        .map(|index| untyped_param(&mut builder, function, &format!("value_{index}")))
        .collect::<Vec<_>>();
    let block = builder.append_block(function);
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: result,
            local: parameters[17],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    (builder.finish(), function)
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

fn external_lvalue_call_fixture() -> (php_ir::IrUnit, FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(4_224));
    let file = builder.add_file("external-lvalue-call.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("external_lvalue_wrapper", FunctionFlags::default(), span);
    builder.set_entry(function);
    let local = untyped_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "deployment_function".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(loaded),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: Some(local),
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    (builder.finish(), function)
}

fn scalar_native_include_fixture() -> (php_ir::IrUnit, FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("native-include.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("native_include", FunctionFlags::default(), span);
    builder.set_entry(function);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let block = builder.append_block(function);
    let path = builder.add_constant(IrConstant::Int(91));
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Include {
            dst: result,
            kind: php_ir::instruction::IncludeKind::RequireOnce,
            path: Operand::Constant(path),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    (builder.finish(), function)
}

fn scalar_native_eval_fixture() -> (php_ir::IrUnit, FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("native-eval.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("native_eval", FunctionFlags::default(), span);
    builder.set_entry(function);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let block = builder.append_block(function);
    let source = builder.add_constant(IrConstant::Int(92));
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Eval {
            dst: result,
            code: Operand::Constant(source),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    (builder.finish(), function)
}

fn effect_then_direct_fixture() -> (php_ir::IrUnit, FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("native-transition-effect.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("effect_then_guard", FunctionFlags::default(), span);
    builder.set_entry(function);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let block = builder.append_block(function);
    let path = builder.add_constant(IrConstant::Int(5));
    let forty_one = builder.add_constant(IrConstant::Int(41));
    let one = builder.add_constant(IrConstant::Int(1));
    let effect = builder.alloc_register(function);
    let base = builder.alloc_register(function);
    let increment = builder.alloc_register(function);
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Include {
            dst: effect,
            kind: php_ir::instruction::IncludeKind::Require,
            path: Operand::Constant(path),
        },
        span,
    );
    builder.emit_load_const(function, block, base, forty_one, span);
    builder.emit_load_const(function, block, increment, one, span);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(base),
            rhs: Operand::Register(increment),
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
    let function = builder.start_function("jit_helper_overflow", FunctionFlags::default(), span);
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

fn untyped_param(builder: &mut IrBuilder, function: FunctionId, name: &str) -> LocalId {
    let local = builder.intern_local(function, name);
    builder.push_param(
        function,
        IrParam {
            name: name.to_owned(),
            local,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    local
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

fn typed_array_reference_param(
    builder: &mut IrBuilder,
    function: FunctionId,
    name: &str,
) -> LocalId {
    let local = builder.intern_local(function, name);
    builder.push_param(
        function,
        IrParam {
            name: name.to_owned(),
            local,
            required: true,
            default: None,
            type_: Some(IrReturnType::Array),
            by_ref: true,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    local
}

#[test]
fn optimizing_unknown_scalar_truthiness_uses_guarded_native_lanes() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_210));
    let file = builder.add_file("optimizing-guarded-truthiness.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_guarded_truthiness",
        FunctionFlags::default(),
        span,
    );
    let local = untyped_param(&mut builder, function, "value");
    let entry = builder.append_block(function);
    let truthy = builder.append_block(function);
    let falsey = builder.append_block(function);
    builder.terminate_jump_if(function, entry, Operand::Local(local), truthy, falsey, span);
    let one = builder.intern_constant(IrConstant::Int(1));
    let zero = builder.intern_constant(IrConstant::Int(0));
    builder.terminate_return(function, truthy, Some(Operand::Constant(one)), span);
    builder.terminate_return(function, falsey, Some(Operand::Constant(zero)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.guarded-truthiness").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_truthy: forbidden_truthy as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("guarded truthiness handle");
    assert_optimizing_artifact(&handle);
    let metadata = handle.region_state_metadata().expect("native metadata");
    let conditional = metadata
        .production_lowering
        .iter()
        .find(|entry| entry.operation == "JumpIf")
        .expect("conditional lowering row");
    assert_eq!(
        conditional.class,
        crate::JitProductionLoweringClass::BaselineFragmentTransition
    );
    assert!(conditional.operation_local_transition);
    for (value, expected) in [
        (0, 0),
        (-17, 1),
        (crate::jit_encode_constant(u32::MAX), 0),
        (crate::jit_encode_constant(crate::JIT_VALUE_FALSE), 0),
        (crate::jit_encode_constant(crate::JIT_VALUE_TRUE), 1),
    ] {
        assert_eq!(
            handle
                .invoke_i64(&[value], JIT_RUNTIME_ABI_HASH)
                .expect("guarded truthiness execution"),
            expected
        );
    }
    let mut slots = vec![crate::JitNativeValueSlot::default(); 11];
    slots[7] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_ARRAY,
        flags: crate::JIT_NATIVE_ARRAY_VIEW_ABI_VERSION,
        payload: 0,
        ..crate::JitNativeValueSlot::default()
    };
    slots[8] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        reserved: crate::JIT_NATIVE_STRING_VALUE_ZERO,
        payload: 1,
        ..crate::JitNativeValueSlot::default()
    };
    slots[9] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: 4,
        ..crate::JitNativeValueSlot::default()
    };
    slots[10] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_ARRAY,
        flags: crate::JIT_NATIVE_ARRAY_VIEW_ABI_VERSION,
        payload: 3,
        ..crate::JitNativeValueSlot::default()
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    for (index, tag, expected) in [
        (7, crate::JIT_VALUE_RUNTIME_ARRAY_TAG, 0),
        (8, crate::JIT_VALUE_RUNTIME_STRING_TAG, 0),
        (9, crate::JIT_VALUE_RUNTIME_STRING_TAG, 1),
        (10, crate::JIT_VALUE_RUNTIME_ARRAY_TAG, 1),
    ] {
        let value = crate::jit_encode_typed_runtime_value(index, tag);
        assert_eq!(
            handle
                .invoke_i64(&[value], JIT_RUNTIME_ABI_HASH)
                .expect("guarded runtime truthiness execution"),
            expected
        );
    }
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_terminator_rejects_to_matching_baseline_continuation() {
    let mut builder = IrBuilder::new(UnitId::new(4_210_1));
    let file = builder.add_file("optimizing-terminator-transition.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_terminator_transition",
        FunctionFlags::default(),
        span,
    );
    let local = untyped_param(&mut builder, function, "value");
    let entry = builder.append_block(function);
    let truthy = builder.append_block(function);
    let falsey = builder.append_block(function);
    builder.terminate_jump_if(function, entry, Operand::Local(local), truthy, falsey, span);
    let one = builder.intern_constant(IrConstant::Int(1));
    let zero = builder.intern_constant(IrConstant::Int(0));
    builder.terminate_return(function, truthy, Some(Operand::Constant(one)), span);
    builder.terminate_return(function, falsey, Some(Operand::Constant(zero)), span);
    let unit = builder.finish();
    let helpers = crate::JitRuntimeHelperAddresses {
        native_truthy: baseline_truthy_true as *const () as usize,
        ..crate::JitRuntimeHelperAddresses::default()
    };
    let mut backend = CraneliftNativeCompiler;
    let baseline = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.terminator-transition.baseline"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: helpers,
    });
    let optimized = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.terminator-transition.optimized").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: helpers,
    });
    assert_eq!(baseline.status, JitCompileStatus::Compiled, "{baseline:?}");
    assert_eq!(
        optimized.status,
        JitCompileStatus::Compiled,
        "{optimized:?}"
    );
    let baseline = baseline.handle.expect("baseline terminator owner");
    let optimized = optimized.handle.expect("optimizing terminator owner");
    assert_optimizing_artifact(&optimized);
    assert!(
        optimized
            .relocatable_code()
            .expect("optimizing relocations")
            .relocations
            .iter()
            .all(|relocation| !matches!(
                &relocation.target,
                crate::JitRelocatableTarget::Helper(symbol) if symbol.contains("truthy")
            )),
        "the optimizer must reject to baseline instead of importing truthiness"
    );
    let opaque_object =
        crate::jit_encode_typed_runtime_value(7, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
    assert_eq!(
        optimized
            .invoke_i64_with_native_transition(&baseline, &[opaque_object], JIT_RUNTIME_ABI_HASH,)
            .expect("terminator guard should continue in baseline"),
        crate::JitI64InvokeOutcome::Returned(1)
    );
}

#[test]
fn optimizing_property_array_chain_uses_baseline_snapshot_order() {
    let mut builder = IrBuilder::new(UnitId::new(4_210_2));
    let file = builder.add_file("optimizing-property-array-chain.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_property_array_chain",
        FunctionFlags::default(),
        span,
    );
    let receiver_local = untyped_param(&mut builder, function, "receiver");
    let block = builder.append_block(function);
    let receiver = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: receiver,
            local: receiver_local,
        },
        span,
    );
    let source = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchProperty {
            dst: source,
            object: Operand::Register(receiver),
            property: "source".to_owned(),
        },
        span,
    );
    let zero = builder.intern_constant(IrConstant::Int(0));
    let key = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchDim {
            dst: key,
            array: Operand::Register(source),
            key: Operand::Constant(zero),
            quiet: false,
            mode: php_ir::instruction::DimFetchMode::Read,
        },
        span,
    );
    let replacement = builder.intern_constant(IrConstant::String("replacement".to_owned()));
    let assigned = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::AssignPropertyDim {
            dst: assigned,
            object: Operand::Register(receiver),
            property: "target".to_owned(),
            dims: vec![Operand::Register(key)],
            value: Operand::Constant(replacement),
            append: false,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(assigned)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let helpers = crate::JitRuntimeHelperAddresses {
        native_semantic_dispatch: 1,
        ..crate::JitRuntimeHelperAddresses::default()
    };
    let baseline = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.property-array-chain.baseline"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: helpers,
    });
    let optimized = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.property-array-chain.optimized").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: helpers,
    });
    assert_eq!(baseline.status, JitCompileStatus::Compiled, "{baseline:?}");
    assert_eq!(
        optimized.status,
        JitCompileStatus::Compiled,
        "{optimized:?}"
    );
    let baseline = baseline.handle.expect("baseline property chain");
    let optimized = optimized.handle.expect("optimizing property chain");
    let baseline_transitions = &baseline
        .region_state_metadata()
        .expect("baseline transitions")
        .native_transitions;
    for optimized_transition in &optimized
        .region_state_metadata()
        .expect("optimizing transitions")
        .native_transitions
    {
        let baseline_transition = baseline_transitions
            .iter()
            .find(|entry| entry.continuation_id == optimized_transition.continuation_id)
            .unwrap_or_else(|| {
                panic!(
                    "baseline transition {} is missing",
                    optimized_transition.continuation_id
                )
            });
        assert_eq!(
            baseline_transition.live_registers, optimized_transition.live_registers,
            "transition {} changes sparse register order between tiers",
            optimized_transition.continuation_id
        );
    }
}

#[test]
fn optimizing_unknown_value_strict_null_identity_stays_native() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_211));
    let file = builder.add_file("optimizing-strict-null.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("optimizing_strict_null", FunctionFlags::default(), span);
    let local = untyped_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    let null = builder.intern_constant(IrConstant::Null);
    let compared = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Compare {
            dst: compared,
            op: php_ir::CompareOp::NotIdentical,
            lhs: Operand::Register(loaded),
            rhs: Operand::Constant(null),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(compared)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.strict-null").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_compare: forbidden_compare as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing strict-null handle");
    assert_optimizing_artifact(&handle);
    for (input, expected) in [
        (
            crate::jit_encode_constant(u32::MAX),
            crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
        ),
        (41, crate::jit_encode_constant(crate::JIT_VALUE_TRUE)),
        (
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        ),
    ] {
        assert_eq!(
            handle
                .invoke_i64(&[input], JIT_RUNTIME_ABI_HASH)
                .expect("strict null identity execution"),
            expected
        );
    }
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_isset_dim_matches_literal_key_without_array_or_compare_helper() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_218));
    let file = builder.add_file("optimizing-isset-dim.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("optimizing_isset_dim", FunctionFlags::default(), span);
    let array = untyped_param(&mut builder, function, "array");
    let key = builder.intern_constant(IrConstant::String("post_type".to_owned()));
    let block = builder.append_block(function);
    let isset = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::IssetDim {
            dst: isset,
            local: array,
            dims: vec![Operand::Constant(key)],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(isset)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.isset-dim").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_fetch: test_array_fetch_typed_string as *const () as usize,
            native_compare: forbidden_compare as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing isset-dim handle");
    assert_optimizing_artifact(&handle);
    let array_index = crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
    let string_index = array_index + 1;
    let string =
        crate::jit_encode_typed_runtime_value(string_index, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    let mut cache = [crate::JitNativeDirectArrayEntry {
        key: string,
        value: 73,
    }];
    let mut slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_ARRAY,
        flags: crate::JIT_NATIVE_ARRAY_VIEW_ABI_VERSION,
        payload: 1,
        aux: cache.as_mut_ptr() as usize as u64,
        reserved: 0,
    };
    slots[1] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: "post_type".len() as u64,
        aux: "post_type".as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut constant_views = vec![crate::JitNativeConstantView::default(); unit.constants.len()];
    constant_views[key.index()] = crate::JitNativeConstantView {
        kind: crate::JIT_NATIVE_CONSTANT_VIEW_STRING,
        reserved: 0,
        length: "post_type".len() as u64,
        bytes: "post_type".as_ptr() as usize as u64,
    };
    let mut direct_next = 2_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        trusted_constant_views: constant_views.as_mut_ptr() as usize as u64,
        trusted_constant_view_count: constant_views.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    let array =
        crate::jit_encode_typed_runtime_value(array_index, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    assert_eq!(
        handle
            .invoke_i64(&[array], JIT_RUNTIME_ABI_HASH)
            .expect("typed isset-dim execution"),
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE)
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_isset_dim_reads_direct_reference_array_without_helper() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_219));
    let file = builder.add_file("optimizing-reference-array-isset.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_reference_array_isset",
        FunctionFlags::default(),
        span,
    );
    let local = builder.intern_local(function, "array");
    builder.push_param(
        function,
        IrParam {
            name: "array".to_owned(),
            local,
            required: true,
            default: None,
            type_: None,
            by_ref: true,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let key = builder.intern_constant(IrConstant::String("post_type".to_owned()));
    let block = builder.append_block(function);
    let isset = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::IssetDim {
            dst: isset,
            local,
            dims: vec![Operand::Constant(key)],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(isset)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.reference-array-isset").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_fetch: forbidden_cached_array_fetch as *const () as usize,
            native_compare: forbidden_compare as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("reference-array isset handle");
    assert_optimizing_artifact(&handle);

    let array_index = crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1;
    let string_index = crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 2;
    let string =
        crate::jit_encode_typed_runtime_value(string_index, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    let mut entries = [crate::JitNativeDirectArrayEntry {
        key: string,
        value: 1,
    }];
    let mut slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        reserved: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        payload: crate::jit_encode_typed_runtime_value(
            array_index,
            crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
        ) as u64,
        ..crate::JitNativeValueSlot::default()
    };
    slots[1] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: crate::jit_native_direct_array_flags(None),
        reserved: crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY,
        payload: entries.len() as u64,
        aux: entries.as_mut_ptr() as usize as u64,
    };
    slots[2] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: "post_type".len() as u64,
        aux: "post_type".as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut constant_views = vec![crate::JitNativeConstantView::default(); unit.constants.len()];
    constant_views[key.index()] = crate::JitNativeConstantView {
        kind: crate::JIT_NATIVE_CONSTANT_VIEW_STRING,
        reserved: 0,
        length: "post_type".len() as u64,
        bytes: "post_type".as_ptr() as usize as u64,
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: slots.as_mut_ptr() as usize as u64,
        trusted_constant_views: constant_views.as_mut_ptr() as usize as u64,
        trusted_constant_view_count: constant_views.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    let reference = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[reference], JIT_RUNTIME_ABI_HASH)
            .expect("reference array isset hit"),
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE)
    );
    // SAFETY: the native artifact reads this stable test-owned direct entry.
    unsafe {
        std::ptr::addr_of_mut!(entries[0].value)
            .write_volatile(crate::jit_encode_constant(u32::MAX));
    }
    assert_eq!(
        handle
            .invoke_i64(&[reference], JIT_RUNTIME_ABI_HASH)
            .expect("reference array isset null"),
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE)
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_reference_array_assign_updates_direct_slot_without_helper() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_220));
    let file = builder.add_file("optimizing-reference-array-assign.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_reference_array_assign",
        FunctionFlags::default(),
        span,
    );
    let array = builder.intern_local(function, "array");
    builder.push_param(
        function,
        IrParam {
            name: "array".to_owned(),
            local: array,
            required: true,
            default: None,
            type_: None,
            by_ref: true,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let value = typed_int_param(&mut builder, function, "value");
    let key = builder.intern_constant(IrConstant::Int(0));
    let block = builder.append_block(function);
    let assigned = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::AssignDim {
            dst: assigned,
            local: array,
            dims: vec![Operand::Constant(key)],
            value: Operand::Local(value),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(assigned)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.reference-array-assign").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("reference array assign handle");
    assert_optimizing_artifact(&handle);
    let relocatable = handle
        .relocatable_code()
        .expect("reference assign artifact");
    assert!(relocatable.relocations.iter().all(|relocation| {
        !matches!(
            &relocation.target,
            crate::JitRelocatableTarget::Helper(name)
                if name == "phrust_native_array_insert"
        )
    }));

    let mut entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    entries[0] = crate::JitNativeDirectArrayEntry { key: 0, value: 41 };
    let mut slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let array_value = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        reserved: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        payload: array_value as u64,
        ..crate::JitNativeValueSlot::default()
    };
    slots[1] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: crate::jit_native_direct_array_flags(None),
        reserved: crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY,
        payload: 1,
        aux: entries.as_mut_ptr() as usize as u64,
    };
    let mut next_slot = 2_u32;
    let mut free_slot = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut next_entry = crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY;
    let mut free_entries =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS];
    let mut roots_dirty = 0_u32;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut next_slot) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut free_slot) as usize as u64,
        direct_array_entries: entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut next_entry) as usize as u64,
        direct_array_free_heads: free_entries.as_mut_ptr() as usize as u64,
        root_mutation_pending: std::ptr::from_mut(&mut roots_dirty) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let reference = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[reference, 42], JIT_RUNTIME_ABI_HASH)
            .expect("direct reference array assign"),
        42
    );
    assert_eq!(entries[0].value, 42);
    assert_eq!(slots[0].payload, array_value as u64);
    assert_eq!(roots_dirty, 1);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn array_fetch_reads_complete_native_array_view_without_helper() {
    ARRAY_FETCH_FALLBACK_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_222));
    let file = builder.add_file("native-array-cache.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("native_array_cache", FunctionFlags::default(), span);
    let array = untyped_param(&mut builder, function, "array");
    let block = builder.append_block(function);
    let key = builder.intern_constant(IrConstant::Int(7));
    let fetched = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchDim {
            dst: fetched,
            array: Operand::Local(array),
            key: Operand::Constant(key),
            quiet: false,
            mode: php_ir::instruction::DimFetchMode::Read,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(fetched)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-array-cache").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_array_fetch: forbidden_cached_array_fetch as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native array cache handle");
    let relocatable = handle.relocatable_code().expect("optimized array artifact");
    assert!(relocatable.relocations.iter().all(|relocation| {
        !matches!(
            &relocation.target,
            crate::JitRelocatableTarget::Helper(name)
                if name == "phrust_native_array_fetch"
        )
    }));

    let encoded_key = 7_i64;
    let mut entries = [crate::JitNativeDirectArrayEntry {
        key: encoded_key,
        value: 73,
    }];
    let mut slots = vec![crate::JitNativeValueSlot::default(); 4];
    slots[3] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_ARRAY,
        flags: crate::JIT_NATIVE_ARRAY_VIEW_ABI_VERSION,
        reserved: 0,
        payload: 1,
        aux: entries.as_mut_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let array = crate::jit_encode_typed_runtime_value(3, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    assert_eq!(
        handle
            .invoke_i64(&[array], JIT_RUNTIME_ABI_HASH)
            .expect("direct mapped array fetch"),
        73
    );
    assert_eq!(ARRAY_FETCH_FALLBACK_CALLS.load(Ordering::SeqCst), 0);

    entries[0].key = 8;
    assert_eq!(
        handle.invoke_i64(&[array], JIT_RUNTIME_ABI_HASH),
        Err(crate::JitInvokeError::NativeStatus(
            crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32,
        ))
    );
    assert_eq!(
        ARRAY_FETCH_FALLBACK_CALLS.load(Ordering::SeqCst),
        0,
        "an optimizing view miss must transition, never call the generic array helper"
    );
    drop(view);

    let mut shared_entries = [crate::JitNativeReferenceArrayEntry {
        kind: crate::JIT_NATIVE_REFERENCE_ARRAY_KEY_INT,
        non_null: 1,
        integer: 7,
        string_length: 0,
        string_bytes: 0,
        value_kind: crate::JIT_NATIVE_REFERENCE_ARRAY_VALUE_INT,
        value_flags: 0,
        value_payload: 91,
        value_length: 0,
        value_bytes: 0,
    }];
    let mut direct_slots = vec![crate::JitNativeValueSlot::default(); 2];
    direct_slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY,
        flags: crate::JIT_NATIVE_SHARED_ARRAY_ABI_VERSION,
        reserved: 1,
        payload: 1,
        aux: shared_entries.as_mut_ptr() as usize as u64,
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let shared = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[shared], JIT_RUNTIME_ABI_HASH)
            .expect("shared native array fetch"),
        91
    );
    assert_eq!(ARRAY_FETCH_FALLBACK_CALLS.load(Ordering::SeqCst), 0);

    std::hint::black_box(&entries);
}

#[test]
fn foreach_next_advances_published_snapshot_without_helper() {
    FOREACH_NEXT_FALLBACK_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_223));
    let file = builder.add_file("native-foreach-view.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("native_foreach_view", FunctionFlags::default(), span);
    let iterator_local = untyped_param(&mut builder, function, "iterator");
    let block = builder.append_block(function);
    let iterator = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: iterator,
            local: iterator_local,
        },
        span,
    );
    let has_value = builder.alloc_register(function);
    let key = builder.alloc_register(function);
    let value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::ForeachNext {
            has_value,
            iterator,
            key: Some(key),
            value,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(value)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-foreach-view"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_foreach_next: forbidden_foreach_next as *const () as usize,
            native_local_fetch: passthrough_local_fetch as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native foreach view handle");

    let mut entries = [crate::JitNativeForeachEntry { key: 7, value: 73 }];
    let mut foreach_view = crate::JitNativeForeachView {
        cursor: 0,
        length: entries.len() as u64,
        entries: entries.as_mut_ptr() as usize as u64,
    };
    let mut slots = vec![crate::JitNativeValueSlot::default(); 4];
    slots[3] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT,
        flags: crate::JIT_NATIVE_FOREACH_VIEW_ABI_VERSION,
        payload: std::ptr::from_mut(&mut foreach_view) as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let iterator = crate::jit_encode_typed_runtime_value(3, crate::JIT_VALUE_RUNTIME_ITERATOR_TAG);
    assert_eq!(
        handle
            .invoke_i64(&[iterator], JIT_RUNTIME_ABI_HASH)
            .expect("direct foreach execution"),
        73
    );
    assert_eq!(foreach_view.cursor, 1);
    assert_eq!(FOREACH_NEXT_FALLBACK_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_array_key_exists_bypasses_generic_builtin_dispatch() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_219));
    let file = builder.add_file("optimizing-array-key-exists.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_array_key_exists",
        FunctionFlags::default(),
        span,
    );
    let array = untyped_param(&mut builder, function, "array");
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: loaded,
            local: array,
        },
        span,
    );
    let key = builder.intern_constant(IrConstant::Int(7));
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "array_key_exists".to_owned(),
            args: vec![
                IrCallArg {
                    name: None,
                    value: Operand::Constant(key),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
                IrCallArg {
                    name: None,
                    value: Operand::Register(loaded),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.array-key-exists").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_array_fetch: test_array_key_exists_fast as *const () as usize,
            native_local_fetch: passthrough_local_fetch as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("array_key_exists handle");
    assert_optimizing_artifact(&handle);
    let mut cache = [crate::JitNativeDirectArrayEntry {
        key: 7,
        value: crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
    }];
    let mut slots = vec![crate::JitNativeValueSlot::default(); 4];
    slots[3] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_ARRAY,
        flags: crate::JIT_NATIVE_ARRAY_VIEW_ABI_VERSION,
        payload: 1,
        aux: cache.as_mut_ptr() as usize as u64,
        reserved: 0,
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let array = crate::jit_encode_typed_runtime_value(3, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    assert_eq!(
        handle
            .invoke_i64(&[array], JIT_RUNTIME_ABI_HASH)
            .expect("array_key_exists execution"),
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE)
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_declared_property_slot_bypasses_property_helper() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_232));
    let file = builder.add_file("optimizing-declared-property.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_declared_property",
        FunctionFlags::default(),
        span,
    );
    let object_local = untyped_param(&mut builder, function, "object");
    let block = builder.append_block(function);
    let object = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: object,
            local: object_local,
        },
        span,
    );
    let value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchProperty {
            dst: value,
            object: Operand::Register(object),
            property: "post_count".to_owned(),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(value)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.declared-property").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_property_fetch: forbidden_property_fetch as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing property handle");
    assert_optimizing_artifact(&handle);
    let layout_id = 17_u64;
    let mut property_slots = [php_runtime::api::NativeDeclaredPropertySlot {
        initialized: 1,
        reserved: 0,
        value: 73,
    }];
    let mut direct_slots = vec![crate::JitNativeValueSlot::default(); 4];
    direct_slots[3] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
        flags: crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
        reserved: property_slots.len() as u32,
        payload: layout_id,
        aux: property_slots.as_mut_ptr() as usize as u64,
    };
    let mut direct_next = 4_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut function_offsets = vec![0_u32; function.index().saturating_add(1)];
    let plan_index = function_offsets[function.index()] as usize + 1;
    let mut property_plans = vec![crate::JitNativeTrustedPropertySlot::default(); plan_index + 1];
    property_plans[plan_index] = crate::JitNativeTrustedPropertySlot {
        state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
        slot_index: 0,
        layout_id,
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        trusted_property_function_offsets: function_offsets.as_mut_ptr() as usize as u64,
        trusted_property_function_count: function_offsets.len() as u32,
        trusted_property_slots: property_plans.as_mut_ptr() as usize as u64,
        trusted_property_slot_count: property_plans.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    let object = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 3,
        crate::JIT_VALUE_RUNTIME_OBJECT_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[object], JIT_RUNTIME_ABI_HASH)
            .expect("direct property fetch"),
        73
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_static_property_uses_authoritative_native_slot() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_239));
    let file = builder.add_file("optimizing-static-property.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_static_property", FunctionFlags::default(), span);
    let input_local = untyped_param(&mut builder, function, "input");
    let block = builder.append_block(function);
    let input = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: input,
            local: input_local,
        },
        span,
    );
    let assigned = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::AssignStaticProperty {
            dst: assigned,
            class_name: "Counter".to_owned(),
            property: "value".to_owned(),
            value: Operand::Register(input),
        },
        span,
    );
    let present = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::IssetStaticProperty {
            dst: present,
            class_name: "Counter".to_owned(),
            property: "value".to_owned(),
        },
        span,
    );
    let empty = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::EmptyStaticProperty {
            dst: empty,
            class_name: "Counter".to_owned(),
            property: "value".to_owned(),
        },
        span,
    );
    let fetched = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchStaticProperty {
            dst: fetched,
            class_name: "Counter".to_owned(),
            property: "value".to_owned(),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(fetched)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.static-property").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_semantic_dispatch: forbidden_call_dispatch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing static-property handle");
    assert_optimizing_artifact(&handle);

    let mut static_slots = [crate::JitNativeStaticPropertySlot {
        value: crate::jit_encode_constant(u32::MAX),
        initialized: 1,
        reserved: 0,
    }];
    let mut function_offsets = vec![0_u32; function.index().saturating_add(1)];
    let plan_base = function_offsets[function.index()] as usize;
    let mut plans = vec![crate::JitNativeTrustedStaticPropertySlot::default(); plan_base + 5];
    plans[plan_base + 1] = crate::JitNativeTrustedStaticPropertySlot {
        state: crate::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_WRITABLE,
        slot_index: 0,
    };
    for continuation in 2..=4 {
        plans[plan_base + continuation] = crate::JitNativeTrustedStaticPropertySlot {
            state: crate::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_READABLE,
            slot_index: 0,
        };
    }
    let mut roots_dirty = 0_u32;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        trusted_property_function_offsets: function_offsets.as_mut_ptr() as usize as u64,
        trusted_property_function_count: function_offsets.len() as u32,
        static_property_slots: static_slots.as_mut_ptr() as usize as u64,
        static_property_slot_count: static_slots.len() as u32,
        trusted_static_property_slots: plans.as_mut_ptr() as usize as u64,
        trusted_static_property_slot_count: plans.len() as u32,
        root_mutation_pending: std::ptr::from_mut(&mut roots_dirty) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        handle
            .invoke_i64(&[73], JIT_RUNTIME_ABI_HASH)
            .expect("direct static-property execution"),
        73
    );
    assert_eq!(static_slots[0].value, 73);
    assert_eq!(roots_dirty, 1);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_declared_property_store_bypasses_property_helper() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_233));
    let file = builder.add_file("optimizing-declared-property-store.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_declared_property_store",
        FunctionFlags::default(),
        span,
    );
    let object_local = untyped_param(&mut builder, function, "object");
    let value_local = untyped_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    let object = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: object,
            local: object_local,
        },
        span,
    );
    let input = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: input,
            local: value_local,
        },
        span,
    );
    let assigned = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::AssignProperty {
            dst: assigned,
            object: Operand::Register(object),
            property: "post_count".to_owned(),
            value: Operand::Register(input),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(assigned)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.declared-property-store").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_property_assign: forbidden_property_assign as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing property-store handle");
    assert_optimizing_artifact(&handle);

    let layout_id = 19_u64;
    let old_array = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 4,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    let mut property_slots = [php_runtime::api::NativeDeclaredPropertySlot {
        initialized: 1,
        reserved: 0,
        value: old_array,
    }];
    let mut array_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize
    ];
    let mut direct_slots = vec![crate::JitNativeValueSlot::default(); 8];
    direct_slots[3] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
        flags: crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
        reserved: property_slots.len() as u32,
        payload: layout_id,
        aux: property_slots.as_mut_ptr() as usize as u64,
    };
    direct_slots[4] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        reserved: crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY,
        payload: 0,
        aux: array_entries.as_mut_ptr() as usize as u64,
    };
    let mut direct_next = 5_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut array_next = crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY;
    let mut array_free_heads =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS];
    let mut root_mutation_pending = 0_u32;
    let mut function_offsets = vec![0_u32; function.index().saturating_add(1)];
    let mut property_plans = vec![
        crate::JitNativeTrustedPropertySlot {
            state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE,
            slot_index: 0,
            layout_id,
        };
        16
    ];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_array_entries: array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut array_next) as usize as u64,
        direct_array_free_heads: array_free_heads.as_mut_ptr() as usize as u64,
        root_mutation_pending: std::ptr::from_mut(&mut root_mutation_pending) as usize as u64,
        trusted_property_function_offsets: function_offsets.as_mut_ptr() as usize as u64,
        trusted_property_function_count: function_offsets.len() as u32,
        trusted_property_slots: property_plans.as_mut_ptr() as usize as u64,
        trusted_property_slot_count: property_plans.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    let object = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 3,
        crate::JIT_VALUE_RUNTIME_OBJECT_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[object, 73], JIT_RUNTIME_ABI_HASH)
            .expect("direct property store"),
        73
    );
    assert_eq!(property_slots[0].initialized, 1);
    assert_eq!(property_slots[0].value, 73);
    assert_eq!(direct_slots[4].refcount, 0);
    assert_eq!(direct_slots[4].kind, crate::JIT_NATIVE_VALUE_VIEW_NONE);
    assert_eq!(direct_free, 4);
    assert_eq!(root_mutation_pending, 1);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_declared_property_references_bind_exact_slot_without_helpers() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_234));
    let file = builder.add_file("optimizing-declared-property-reference.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_declared_property_reference",
        FunctionFlags::default(),
        span,
    );
    let object_local = untyped_param(&mut builder, function, "object");
    let source_local = untyped_param(&mut builder, function, "source");
    let alias_local = builder.intern_local(function, "alias");
    let block = builder.append_block(function);
    let object = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: object,
            local: object_local,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::BindReferenceProperty {
            object: Operand::Register(object),
            property: "post_count".to_owned(),
            source: source_local,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::BindReferenceFromProperty {
            target: alias_local,
            object: Operand::Register(object),
            property: "post_count".to_owned(),
        },
        span,
    );
    let value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: value,
            local: alias_local,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(value)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.declared-property-reference")
            .with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_reference_bind: forbidden_reference_bind as *const () as usize,
            native_property_fetch: forbidden_property_fetch as *const () as usize,
            native_property_assign: forbidden_property_assign as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome
        .handle
        .expect("optimizing property-reference handle");
    assert_optimizing_artifact(&handle);

    let layout_id = 23_u64;
    let mut property_slots = [php_runtime::api::NativeDeclaredPropertySlot {
        initialized: 1,
        reserved: 0,
        value: 73,
    }];
    let mut direct_slots = vec![crate::JitNativeValueSlot::default(); 8];
    direct_slots[3] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
        flags: crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
        reserved: property_slots.len() as u32,
        payload: layout_id,
        aux: property_slots.as_mut_ptr() as usize as u64,
    };
    let mut direct_next = 4_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut root_mutation_pending = 0_u32;
    let mut function_offsets = vec![0_u32; function.index().saturating_add(1)];
    let mut property_plans = vec![
        crate::JitNativeTrustedPropertySlot {
            state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE,
            slot_index: 0,
            layout_id,
        };
        16
    ];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        root_mutation_pending: std::ptr::from_mut(&mut root_mutation_pending) as usize as u64,
        trusted_property_function_offsets: function_offsets.as_mut_ptr() as usize as u64,
        trusted_property_function_count: function_offsets.len() as u32,
        trusted_property_slots: property_plans.as_mut_ptr() as usize as u64,
        trusted_property_slot_count: property_plans.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    let object = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 3,
        crate::JIT_VALUE_RUNTIME_OBJECT_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[object, 91], JIT_RUNTIME_ABI_HASH)
            .expect("direct property reference"),
        91
    );
    let reference = property_slots[0].value;
    assert_eq!(
        reference,
        crate::jit_encode_typed_runtime_value(
            crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 4,
            crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
        )
    );
    assert_eq!(
        direct_slots[4].kind,
        crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
    );
    assert_eq!(direct_slots[4].payload, 91_u64);
    assert_eq!(direct_slots[4].refcount, 1);
    assert_eq!(root_mutation_pending, 1);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_declared_property_dimension_reference_mutates_exact_array_slot() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_235));
    let file = builder.add_file("optimizing-declared-property-dimension-reference.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_declared_property_dimension_reference",
        FunctionFlags::default(),
        span,
    );
    let object_local = untyped_param(&mut builder, function, "object");
    let source_local = untyped_param(&mut builder, function, "source");
    let alias_local = builder.intern_local(function, "alias");
    let missing_alias_local = builder.intern_local(function, "missing_alias");
    let block = builder.append_block(function);
    let object = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: object,
            local: object_local,
        },
        span,
    );
    let zero = builder.intern_constant(IrConstant::Int(0));
    let one = builder.intern_constant(IrConstant::Int(1));
    let two = builder.intern_constant(IrConstant::Int(2));
    builder.emit(
        function,
        block,
        InstructionKind::BindReferencePropertyDim {
            object: Operand::Register(object),
            property: "items".to_owned(),
            dims: vec![Operand::Constant(zero), Operand::Constant(one)],
            append: false,
            source: source_local,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::BindReferenceFromPropertyDim {
            target: alias_local,
            object: Operand::Register(object),
            property: "items".to_owned(),
            dims: vec![Operand::Constant(zero), Operand::Constant(one)],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::BindReferenceFromPropertyDim {
            target: missing_alias_local,
            object: Operand::Register(object),
            property: "items".to_owned(),
            dims: vec![Operand::Constant(zero), Operand::Constant(two)],
        },
        span,
    );
    let value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: value,
            local: alias_local,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(value)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.declared-property-dimension-reference")
            .with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_reference_bind: forbidden_reference_bind as *const () as usize,
            native_property_fetch: forbidden_property_fetch as *const () as usize,
            native_property_assign: forbidden_property_assign as *const () as usize,
            native_array_fetch: forbidden_cached_array_fetch as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome
        .handle
        .expect("optimizing property-dimension-reference handle");
    assert_optimizing_artifact(&handle);

    let layout_id = 29_u64;
    let array = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 4,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    let child = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 5,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    let mut property_slots = [php_runtime::api::NativeDeclaredPropertySlot {
        initialized: 1,
        reserved: 0,
        value: array,
    }];
    let mut array_entries = vec![crate::JitNativeDirectArrayEntry::default(); 16];
    array_entries[0] = crate::JitNativeDirectArrayEntry {
        key: 0,
        value: child,
    };
    array_entries[8] = crate::JitNativeDirectArrayEntry { key: 1, value: 73 };
    let mut direct_slots = vec![crate::JitNativeValueSlot::default(); 10];
    direct_slots[3] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
        flags: crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
        reserved: property_slots.len() as u32,
        payload: layout_id,
        aux: property_slots.as_mut_ptr() as usize as u64,
    };
    direct_slots[4] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        reserved: 8,
        payload: 1,
        aux: array_entries.as_mut_ptr() as usize as u64,
    };
    direct_slots[5] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        reserved: 8,
        payload: 1,
        aux: array_entries[8..].as_mut_ptr() as usize as u64,
    };
    let mut direct_next = 6_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut array_next = array_entries.len() as u32;
    let mut array_free_heads =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS];
    let mut root_mutation_pending = 0_u32;
    let mut function_offsets = vec![0_u32; function.index().saturating_add(1)];
    let mut property_plans = vec![
        crate::JitNativeTrustedPropertySlot {
            state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE,
            slot_index: 0,
            layout_id,
        };
        16
    ];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_array_entries: array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut array_next) as usize as u64,
        direct_array_free_heads: array_free_heads.as_mut_ptr() as usize as u64,
        root_mutation_pending: std::ptr::from_mut(&mut root_mutation_pending) as usize as u64,
        trusted_property_function_offsets: function_offsets.as_mut_ptr() as usize as u64,
        trusted_property_function_count: function_offsets.len() as u32,
        trusted_property_slots: property_plans.as_mut_ptr() as usize as u64,
        trusted_property_slot_count: property_plans.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    let object = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 3,
        crate::JIT_VALUE_RUNTIME_OBJECT_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[object, 91], JIT_RUNTIME_ABI_HASH)
            .expect("direct property dimension reference"),
        91
    );
    let reference = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 6,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    assert_eq!(property_slots[0].value, array);
    assert_eq!(array_entries[0].value, child);
    assert_eq!(array_entries[8].value, reference);
    let missing_reference = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 7,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    assert_eq!(array_entries[9].key, 2);
    assert_eq!(array_entries[9].value, missing_reference);
    assert_eq!(
        direct_slots[6].kind,
        crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
    );
    assert_eq!(direct_slots[6].payload, 91_u64);
    assert_eq!(direct_slots[6].refcount, 1);
    assert_eq!(
        direct_slots[7].kind,
        crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
    );
    assert_eq!(
        direct_slots[7].payload,
        crate::jit_encode_constant(u32::MAX) as u64
    );
    assert_eq!(direct_slots[7].refcount, 1);
    assert_eq!(root_mutation_pending, 1);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_local_dimension_binds_exact_declared_property_reference() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_236));
    let file = builder.add_file("optimizing-local-dimension-from-property-reference.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_local_dimension_from_property_reference",
        FunctionFlags::default(),
        span,
    );
    let object_local = untyped_param(&mut builder, function, "object");
    let array_local = untyped_param(&mut builder, function, "array");
    let block = builder.append_block(function);
    let object = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: object,
            local: object_local,
        },
        span,
    );
    let zero = builder.intern_constant(IrConstant::Int(0));
    builder.emit(
        function,
        block,
        InstructionKind::BindReferenceDimFromProperty {
            local: array_local,
            dims: vec![Operand::Constant(zero)],
            append: false,
            object: Operand::Register(object),
            property: "value".to_owned(),
        },
        span,
    );
    let returned = builder.intern_constant(IrConstant::Int(1));
    builder.terminate_return(function, block, Some(Operand::Constant(returned)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.local-dimension-from-property-reference")
            .with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_reference_bind: forbidden_reference_bind as *const () as usize,
            native_property_fetch: forbidden_property_fetch as *const () as usize,
            native_property_assign: forbidden_property_assign as *const () as usize,
            native_array_fetch: forbidden_cached_array_fetch as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome
        .handle
        .expect("optimizing local-dimension-from-property-reference handle");
    assert_optimizing_artifact(&handle);

    let layout_id = 31_u64;
    let array = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 4,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    let reference = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 5,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    let mut property_slots = [php_runtime::api::NativeDeclaredPropertySlot {
        initialized: 1,
        reserved: 0,
        value: reference,
    }];
    let mut array_entries = vec![crate::JitNativeDirectArrayEntry::default(); 8];
    array_entries[0] = crate::JitNativeDirectArrayEntry { key: 0, value: 73 };
    let mut direct_slots = vec![crate::JitNativeValueSlot::default(); 8];
    direct_slots[3] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
        flags: crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
        reserved: property_slots.len() as u32,
        payload: layout_id,
        aux: property_slots.as_mut_ptr() as usize as u64,
    };
    direct_slots[4] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        reserved: array_entries.len() as u32,
        payload: 1,
        aux: array_entries.as_mut_ptr() as usize as u64,
    };
    direct_slots[5] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
        payload: 41,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_next = 6_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut array_next = array_entries.len() as u32;
    let mut array_free_heads =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS];
    let mut root_mutation_pending = 0_u32;
    let mut function_offsets = vec![0_u32; function.index().saturating_add(1)];
    let mut property_plans = vec![
        crate::JitNativeTrustedPropertySlot {
            state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE,
            slot_index: 0,
            layout_id,
        };
        16
    ];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_array_entries: array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut array_next) as usize as u64,
        direct_array_free_heads: array_free_heads.as_mut_ptr() as usize as u64,
        root_mutation_pending: std::ptr::from_mut(&mut root_mutation_pending) as usize as u64,
        trusted_property_function_offsets: function_offsets.as_mut_ptr() as usize as u64,
        trusted_property_function_count: function_offsets.len() as u32,
        trusted_property_slots: property_plans.as_mut_ptr() as usize as u64,
        trusted_property_slot_count: property_plans.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    let object = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 3,
        crate::JIT_VALUE_RUNTIME_OBJECT_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[object, array], JIT_RUNTIME_ABI_HASH)
            .expect("direct local dimension from property reference"),
        1
    );
    assert_eq!(property_slots[0].value, reference);
    assert_eq!(array_entries[0].value, reference);
    assert_eq!(direct_slots[4].refcount, 1);
    assert_eq!(direct_slots[5].refcount, 2);
    assert_eq!(root_mutation_pending, 1);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_string_predicate_bypasses_generic_builtin_dispatch() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_220));
    let file = builder.add_file("optimizing-string-predicate.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_string_predicate",
        FunctionFlags::default(),
        span,
    );
    let haystack = untyped_param(&mut builder, function, "haystack");
    let needle = untyped_param(&mut builder, function, "needle");
    let block = builder.append_block(function);
    let loaded_haystack = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: loaded_haystack,
            local: haystack,
        },
        span,
    );
    let loaded_needle = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: loaded_needle,
            local: needle,
        },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "str_starts_with".to_owned(),
            args: vec![
                IrCallArg {
                    name: None,
                    value: Operand::Register(loaded_haystack),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
                IrCallArg {
                    name: None,
                    value: Operand::Register(loaded_needle),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
            ],
        },
        span,
    );
    let yes = builder.append_block(function);
    let no = builder.append_block(function);
    builder.terminate_jump_if(function, block, Operand::Register(result), yes, no, span);
    let true_value = builder.intern_constant(IrConstant::Bool(true));
    let false_value = builder.intern_constant(IrConstant::Bool(false));
    builder.terminate_return(function, yes, Some(Operand::Constant(true_value)), span);
    builder.terminate_return(function, no, Some(Operand::Constant(false_value)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.string-predicate").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_string_predicate: test_string_predicate_fast as *const () as usize,
            native_local_fetch: passthrough_local_fetch as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("string predicate handle");
    assert_optimizing_artifact(&handle);
    let haystack_bytes = b"foobar";
    let needle_bytes = b"foo";
    let mut slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: haystack_bytes.len() as u64,
        aux: haystack_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    slots[1] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: needle_bytes.len() as u64,
        aux: needle_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_next = 2_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        handle
            .invoke_i64(
                &[
                    crate::jit_encode_typed_runtime_value(
                        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
                        crate::JIT_VALUE_RUNTIME_STRING_TAG,
                    ),
                    crate::jit_encode_typed_runtime_value(
                        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
                        crate::JIT_VALUE_RUNTIME_STRING_TAG,
                    ),
                ],
                JIT_RUNTIME_ABI_HASH,
            )
            .expect("string predicate execution"),
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE)
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_string_position_and_ord_bypass_generic_builtin_dispatch() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_237));
    let file = builder.add_file("optimizing-string-position.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_string_position_and_ord",
        FunctionFlags::default(),
        span,
    );
    let haystack_local = typed_string_param(&mut builder, function, "haystack");
    let needle_local = typed_string_param(&mut builder, function, "needle");
    let offset_local = typed_int_param(&mut builder, function, "offset");
    let block = builder.append_block(function);
    let haystack = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: haystack,
            local: haystack_local,
        },
        span,
    );
    let needle = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: needle,
            local: needle_local,
        },
        span,
    );
    let offset = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: offset,
            local: offset_local,
        },
        span,
    );
    let position = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: position,
            name: "strripos".to_owned(),
            args: vec![
                IrCallArg {
                    name: None,
                    value: Operand::Register(haystack),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
                IrCallArg {
                    name: None,
                    value: Operand::Register(needle),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
                IrCallArg {
                    name: None,
                    value: Operand::Register(offset),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
            ],
        },
        span,
    );
    let first_byte = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: first_byte,
            name: "ord".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(needle),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    let span_arg = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let non_mask_prefix = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: non_mask_prefix,
            name: "strcspn".to_owned(),
            args: vec![
                span_arg(Operand::Register(haystack)),
                span_arg(Operand::Register(needle)),
            ],
        },
        span,
    );
    let mask_prefix = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: mask_prefix,
            name: "strspn".to_owned(),
            args: vec![
                span_arg(Operand::Register(needle)),
                span_arg(Operand::Register(needle)),
            ],
        },
        span,
    );
    let sum = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: sum,
            op: BinaryOp::Add,
            lhs: Operand::Register(position),
            rhs: Operand::Register(first_byte),
        },
        span,
    );
    let with_non_mask = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: with_non_mask,
            op: BinaryOp::Add,
            lhs: Operand::Register(sum),
            rhs: Operand::Register(non_mask_prefix),
        },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(with_non_mask),
            rhs: Operand::Register(mask_prefix),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.string-position").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_binary: forbidden_binary as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("string position handle");
    assert_optimizing_artifact(&handle);

    let haystack_bytes = b"abABab";
    let needle_bytes = b"AB";
    let mut slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: haystack_bytes.len() as u64,
        aux: haystack_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    slots[1] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: needle_bytes.len() as u64,
        aux: needle_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_next = 2_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let haystack = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let needle = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[haystack, needle, -1], JIT_RUNTIME_ABI_HASH)
            .expect("string position execution"),
        73
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_ascii_case_builtin_has_no_builtin_or_operation_helper_import() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_234));
    let file = builder.add_file("optimizing-ascii-case.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("optimizing_ascii_case", FunctionFlags::default(), span);
    let input_local = typed_string_param(&mut builder, function, "input");
    let block = builder.append_block(function);
    let input = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: input,
            local: input_local,
        },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "strtolower".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(input),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.ascii-case").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing ASCII case handle");
    assert_optimizing_artifact(&handle);

    let input_bytes = b"WordPress-ABC-xyz-123-\xC4";
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    direct_slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: input_bytes.len() as u64,
        aux: input_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_slot_next = 1_u32;
    let mut string_bytes = vec![0_u8; crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY];
    let mut string_next = 0_u32;
    let mut string_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_slot_next) as usize as u64,
        direct_string_bytes: string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(&mut string_next) as usize as u64,
        direct_string_free_heads: string_free.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let input = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let result = handle
        .invoke_i64(&[input], JIT_RUNTIME_ABI_HASH)
        .expect("direct ASCII case conversion");
    assert_eq!(
        crate::jit_decode_runtime_value(result),
        Some(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1)
    );
    assert_eq!(
        &string_bytes[..input_bytes.len()],
        b"wordpress-abc-xyz-123-\xC4"
    );
    assert_eq!(direct_slots[1].payload, input_bytes.len() as u64);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_native_string_transform_family_stays_on_direct_bytes() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_241));
    let file = builder.add_file("optimizing-string-transform-family.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_string_transform_family",
        FunctionFlags::default(),
        span,
    );
    let input_local = typed_string_param(&mut builder, function, "input");
    let count_local = typed_int_param(&mut builder, function, "count");
    let block = builder.append_block(function);
    let input = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: input,
            local: input_local,
        },
        span,
    );
    let count = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: count,
            local: count_local,
        },
        span,
    );
    let arg = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let lowered = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: lowered,
            name: "lcfirst".to_owned(),
            args: vec![arg(Operand::Register(input))],
        },
        span,
    );
    let raised = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: raised,
            name: "ucfirst".to_owned(),
            args: vec![arg(Operand::Register(lowered))],
        },
        span,
    );
    let reversed = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: reversed,
            name: "strrev".to_owned(),
            args: vec![arg(Operand::Register(raised))],
        },
        span,
    );
    let repeated = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: repeated,
            name: "str_repeat".to_owned(),
            args: vec![
                arg(Operand::Register(reversed)),
                arg(Operand::Register(count)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(repeated)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.string-transform-family").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native string transform handle");
    assert_optimizing_artifact(&handle);

    let input_bytes = b"aBc";
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    direct_slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: input_bytes.len() as u64,
        aux: input_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_next = 1_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut string_bytes = vec![0_u8; crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY];
    let mut string_next = 0_u32;
    let mut string_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_string_bytes: string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(&mut string_next) as usize as u64,
        direct_string_free_heads: string_free.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let input = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let result = handle
        .invoke_i64(&[input, 2], JIT_RUNTIME_ABI_HASH)
        .expect("direct native string transform execution");
    let result_index = crate::jit_decode_runtime_value(result)
        .expect("direct string result index")
        .checked_sub(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
        .expect("direct string index base") as usize;
    let result_slot = direct_slots[result_index];
    assert_eq!(result_slot.kind, crate::JIT_NATIVE_VALUE_VIEW_STRING);
    assert_eq!(result_slot.payload, 6);
    let start = usize::try_from(result_slot.aux)
        .expect("result address")
        .checked_sub(string_bytes.as_ptr() as usize)
        .expect("result string arena offset");
    assert_eq!(&string_bytes[start..start + 6], b"cBAcBA");
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_string_byte_analysis_keeps_native_input_and_result() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_245));
    let file = builder.add_file("optimizing-string-byte-analysis.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_string_byte_analysis",
        FunctionFlags::default(),
        span,
    );
    let input_local = typed_string_param(&mut builder, function, "input");
    let needle_local = typed_string_param(&mut builder, function, "needle");
    let block = builder.append_block(function);
    let input = builder.alloc_register(function);
    let needle = builder.alloc_register(function);
    for (dst, local) in [(input, input_local), (needle, needle_local)] {
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal { dst, local },
            span,
        );
    }
    let arg = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let escaped = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: escaped,
            name: "addslashes".to_owned(),
            args: vec![arg(Operand::Register(input))],
        },
        span,
    );
    let count = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: count,
            name: "substr_count".to_owned(),
            args: vec![
                arg(Operand::Register(escaped)),
                arg(Operand::Register(needle)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(count)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.string-byte-analysis").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native byte-analysis handle");
    assert_optimizing_artifact(&handle);

    let input_bytes = b"a'b\"c\\d\0";
    let needle_bytes = b"\\";
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    for (slot, bytes) in direct_slots[..2]
        .iter_mut()
        .zip([input_bytes.as_slice(), needle_bytes.as_slice()])
    {
        *slot = crate::JitNativeValueSlot {
            refcount: 1,
            kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            payload: bytes.len() as u64,
            aux: bytes.as_ptr() as usize as u64,
            ..crate::JitNativeValueSlot::default()
        };
    }
    let mut direct_next = 2_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut string_bytes = vec![0_u8; crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY];
    let mut string_next = 0_u32;
    let mut string_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_string_bytes: string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(&mut string_next) as usize as u64,
        direct_string_free_heads: string_free.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let input = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let needle = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[input, needle], JIT_RUNTIME_ABI_HASH)
            .expect("direct native byte-analysis execution"),
        5
    );
    let escaped = direct_slots[2];
    assert_eq!(escaped.kind, crate::JIT_NATIVE_VALUE_VIEW_STRING);
    let start = usize::try_from(escaped.aux)
        .expect("escaped address")
        .checked_sub(string_bytes.as_ptr() as usize)
        .expect("escaped string arena offset");
    assert_eq!(
        &string_bytes[start..start + escaped.payload as usize],
        b"a\\'b\\\"c\\\\d\\0"
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_native_string_compare_family_stays_on_direct_bytes() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_242));
    let file = builder.add_file("optimizing-string-compare-family.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_string_compare_family",
        FunctionFlags::default(),
        span,
    );
    let lhs_local = typed_string_param(&mut builder, function, "lhs");
    let rhs_local = typed_string_param(&mut builder, function, "rhs");
    let length_local = typed_int_param(&mut builder, function, "length");
    let block = builder.append_block(function);
    let lhs = builder.alloc_register(function);
    let rhs = builder.alloc_register(function);
    let length = builder.alloc_register(function);
    for (dst, local) in [(lhs, lhs_local), (rhs, rhs_local), (length, length_local)] {
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal { dst, local },
            span,
        );
    }
    let arg = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let mut results = Vec::new();
    for (name, bounded) in [
        ("strcmp", false),
        ("strcasecmp", false),
        ("strncmp", true),
        ("strncasecmp", true),
    ] {
        let result = builder.alloc_register(function);
        let mut args = vec![arg(Operand::Register(lhs)), arg(Operand::Register(rhs))];
        if bounded {
            args.push(arg(Operand::Register(length)));
        }
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: result,
                name: name.to_owned(),
                args,
            },
            span,
        );
        results.push(result);
    }
    let first_sum = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: first_sum,
            op: BinaryOp::Add,
            lhs: Operand::Register(results[0]),
            rhs: Operand::Register(results[1]),
        },
        span,
    );
    let second_sum = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: second_sum,
            op: BinaryOp::Add,
            lhs: Operand::Register(results[2]),
            rhs: Operand::Register(results[3]),
        },
        span,
    );
    let sum = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: sum,
            op: BinaryOp::Add,
            lhs: Operand::Register(first_sum),
            rhs: Operand::Register(second_sum),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(sum)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.string-compare-family").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_binary: forbidden_binary as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native string compare handle");
    assert_optimizing_artifact(&handle);

    let left = b"Alpha";
    let right = b"alphaZ";
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    for (slot, bytes) in direct_slots[..2]
        .iter_mut()
        .zip([left.as_slice(), right.as_slice()])
    {
        *slot = crate::JitNativeValueSlot {
            refcount: 1,
            kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            payload: bytes.len() as u64,
            aux: bytes.as_ptr() as usize as u64,
            ..crate::JitNativeValueSlot::default()
        };
    }
    let mut direct_next = 2_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let lhs = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let rhs = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[lhs, rhs, 5], JIT_RUNTIME_ABI_HASH)
            .expect("direct native string compare execution"),
        -65
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_string_array_materialization_bypasses_generic_builtins() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_238));
    let file = builder.add_file("optimizing-string-materialization.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_string_array_materialization",
        FunctionFlags::default(),
        span,
    );
    let input_local = typed_string_param(&mut builder, function, "input");
    let delimiter_local = typed_string_param(&mut builder, function, "delimiter");
    let offset_local = typed_int_param(&mut builder, function, "offset");
    let length_local = typed_int_param(&mut builder, function, "length");
    let byte_local = typed_int_param(&mut builder, function, "byte");
    let block = builder.append_block(function);
    let input = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: input,
            local: input_local,
        },
        span,
    );
    let delimiter = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: delimiter,
            local: delimiter_local,
        },
        span,
    );
    let offset = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: offset,
            local: offset_local,
        },
        span,
    );
    let length = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: length,
            local: length_local,
        },
        span,
    );
    let byte = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: byte,
            local: byte_local,
        },
        span,
    );
    let arg = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let trimmed = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: trimmed,
            name: "trim".to_owned(),
            args: vec![arg(Operand::Register(input))],
        },
        span,
    );
    let pieces = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: pieces,
            name: "explode".to_owned(),
            args: vec![
                arg(Operand::Register(delimiter)),
                arg(Operand::Register(trimmed)),
            ],
        },
        span,
    );
    let separator = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: separator,
            name: "chr".to_owned(),
            args: vec![arg(Operand::Register(byte))],
        },
        span,
    );
    let joined = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: joined,
            name: "implode".to_owned(),
            args: vec![
                arg(Operand::Register(separator)),
                arg(Operand::Register(pieces)),
            ],
        },
        span,
    );
    let sliced = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: sliced,
            name: "substr".to_owned(),
            args: vec![
                arg(Operand::Register(joined)),
                arg(Operand::Register(offset)),
                arg(Operand::Register(length)),
            ],
        },
        span,
    );
    let replaced = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: replaced,
            name: "str_replace".to_owned(),
            args: vec![
                arg(Operand::Register(separator)),
                arg(Operand::Register(delimiter)),
                arg(Operand::Register(sliced)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(replaced)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.string-materialization").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_binary: forbidden_binary as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome
        .handle
        .expect("native string materialization handle");
    assert_optimizing_artifact(&handle);

    let input_bytes = b"  abcABC  ";
    let delimiter_bytes = b"A";
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    direct_slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: input_bytes.len() as u64,
        aux: input_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    direct_slots[1] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: delimiter_bytes.len() as u64,
        aux: delimiter_bytes.as_ptr() as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_slot_next = 2_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut string_bytes = vec![0_u8; crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY];
    let mut string_next = 0_u32;
    let mut string_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS];
    let mut array_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let mut array_next = 0_u32;
    let mut array_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_slot_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_string_bytes: string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(&mut string_next) as usize as u64,
        direct_string_free_heads: string_free.as_mut_ptr() as usize as u64,
        direct_array_entries: array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut array_next) as usize as u64,
        direct_array_free_heads: array_free.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let input = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let delimiter = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let result = handle
        .invoke_i64(&[input, delimiter, 1, -1, 33], JIT_RUNTIME_ABI_HASH)
        .expect("native string/array builtin pipeline");
    let result_index = crate::jit_decode_runtime_value(result)
        .expect("typed direct string result")
        .checked_sub(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
        .expect("direct string index") as usize;
    let result_slot = direct_slots[result_index];
    assert_eq!(result_slot.kind, crate::JIT_NATIVE_VALUE_VIEW_STRING);
    assert_eq!(result_slot.payload, 4);
    let byte_offset = (result_slot.aux as usize)
        .checked_sub(string_bytes.as_ptr() as usize)
        .expect("result points into native string arena");
    assert_eq!(&string_bytes[byte_offset..byte_offset + 4], b"bcAB");
    assert!(direct_slots.iter().any(|slot| {
        slot.kind == crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY && slot.payload == 2
    }));
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_array_builtin_family_preserves_direct_arrays() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_239));
    let file = builder.add_file("optimizing-array-builtins.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_array_builtin_family",
        FunctionFlags::default(),
        span,
    );
    let array_local = typed_array_param(&mut builder, function, "array");
    let needle_local = typed_string_param(&mut builder, function, "needle");
    let strict = builder.intern_constant(IrConstant::Bool(true));
    let zero = builder.intern_constant(IrConstant::Int(0));
    let two = builder.intern_constant(IrConstant::Int(2));
    let failure = builder.intern_constant(IrConstant::Int(-1));
    let entry = builder.append_block(function);
    let check_list = builder.append_block(function);
    let success = builder.append_block(function);
    let failed = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::LoadLocal {
            dst: array,
            local: array_local,
        },
        span,
    );
    let needle = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::LoadLocal {
            dst: needle,
            local: needle_local,
        },
        span,
    );
    let arg = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let values = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: values,
            name: "array_values".to_owned(),
            args: vec![arg(Operand::Register(array))],
        },
        span,
    );
    let sliced = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: sliced,
            name: "array_slice".to_owned(),
            args: vec![
                arg(Operand::Register(values)),
                arg(Operand::Constant(zero)),
                arg(Operand::Constant(two)),
            ],
        },
        span,
    );
    let reversed = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: reversed,
            name: "array_reverse".to_owned(),
            args: vec![arg(Operand::Register(sliced))],
        },
        span,
    );
    let merged = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: merged,
            name: "array_merge".to_owned(),
            args: vec![
                arg(Operand::Register(values)),
                arg(Operand::Register(reversed)),
            ],
        },
        span,
    );
    let found = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: found,
            name: "array_search".to_owned(),
            args: vec![
                arg(Operand::Register(needle)),
                arg(Operand::Register(merged)),
            ],
        },
        span,
    );
    let contains = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: contains,
            name: "in_array".to_owned(),
            args: vec![
                arg(Operand::Register(needle)),
                arg(Operand::Register(merged)),
                arg(Operand::Constant(strict)),
            ],
        },
        span,
    );
    let list = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: list,
            name: "array_is_list".to_owned(),
            args: vec![arg(Operand::Register(merged))],
        },
        span,
    );
    let keys = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: keys,
            name: "array_keys".to_owned(),
            args: vec![arg(Operand::Register(array))],
        },
        span,
    );
    let first = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: first,
            name: "array_key_first".to_owned(),
            args: vec![arg(Operand::Register(keys))],
        },
        span,
    );
    let last = builder.alloc_register(function);
    builder.emit(
        function,
        entry,
        InstructionKind::CallFunction {
            dst: last,
            name: "array_key_last".to_owned(),
            args: vec![arg(Operand::Register(keys))],
        },
        span,
    );
    builder.terminate_jump_if(
        function,
        entry,
        Operand::Register(contains),
        check_list,
        failed,
        span,
    );
    builder.terminate_jump_if(
        function,
        check_list,
        Operand::Register(list),
        success,
        failed,
        span,
    );
    let subtotal = builder.alloc_register(function);
    builder.emit(
        function,
        success,
        InstructionKind::Binary {
            dst: subtotal,
            op: BinaryOp::Add,
            lhs: Operand::Register(found),
            rhs: Operand::Register(first),
        },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        success,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(subtotal),
            rhs: Operand::Register(last),
        },
        span,
    );
    builder.terminate_return(function, success, Some(Operand::Register(result)), span);
    builder.terminate_return(function, failed, Some(Operand::Constant(failure)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.array-builtins").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_binary: forbidden_binary as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native array builtin family handle");
    assert_optimizing_artifact(&handle);

    let alpha = b"alpha";
    let needle_bytes = b"needle";
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    for (slot, bytes) in direct_slots[..2]
        .iter_mut()
        .zip([alpha.as_slice(), needle_bytes.as_slice()])
    {
        *slot = crate::JitNativeValueSlot {
            refcount: 1,
            kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            payload: bytes.len() as u64,
            aux: bytes.as_ptr() as usize as u64,
            ..crate::JitNativeValueSlot::default()
        };
    }
    let alpha_value = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let needle_value = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let mut entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    entries[0] = crate::JitNativeDirectArrayEntry {
        key: 5,
        value: alpha_value,
    };
    entries[1] = crate::JitNativeDirectArrayEntry {
        key: 7,
        value: needle_value,
    };
    direct_slots[2] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        reserved: crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY,
        payload: 2,
        aux: entries.as_mut_ptr() as usize as u64,
    };
    let mut direct_next = 3_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut entry_next = crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY;
    let mut entry_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_array_entries: entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut entry_next) as usize as u64,
        direct_array_free_heads: entry_free.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let array = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 2,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[array, needle_value], JIT_RUNTIME_ABI_HASH)
            .expect("direct array builtin family execution"),
        2
    );
    assert_eq!(
        direct_slots[3].kind,
        crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
    );
    assert_eq!(direct_slots[3].payload, 2);
    assert_eq!(
        direct_slots[4].kind,
        crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
    );
    assert_eq!(direct_slots[4].payload, 2);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_array_push_pop_use_authoritative_native_storage() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_243));
    let file = builder.add_file("optimizing-array-push-pop.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_array_push_pop", FunctionFlags::default(), span);
    let array_local = typed_array_param(&mut builder, function, "array");
    let first_local = typed_int_param(&mut builder, function, "first");
    let second_local = typed_int_param(&mut builder, function, "second");
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    let first = builder.alloc_register(function);
    let second = builder.alloc_register(function);
    for (dst, local) in [
        (array, array_local),
        (first, first_local),
        (second, second_local),
    ] {
        builder.emit(
            function,
            block,
            InstructionKind::LoadLocal { dst, local },
            span,
        );
    }
    let value_arg = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let array_arg = |value| IrCallArg {
        by_ref_local: Some(array_local),
        ..value_arg(value)
    };
    let pushed = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: pushed,
            name: "array_push".to_owned(),
            args: vec![
                array_arg(Operand::Register(array)),
                value_arg(Operand::Register(first)),
                value_arg(Operand::Register(second)),
            ],
        },
        span,
    );
    let updated = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: updated,
            local: array_local,
        },
        span,
    );
    let popped = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: popped,
            name: "array_pop".to_owned(),
            args: vec![array_arg(Operand::Register(updated))],
        },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(pushed),
            rhs: Operand::Register(popped),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.array-push-pop").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_binary: forbidden_binary as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_local_store: forbidden_local_store as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native array push/pop handle");
    assert_optimizing_artifact(&handle);

    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    entries[0] = crate::JitNativeDirectArrayEntry { key: 5, value: 1 };
    direct_slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: crate::jit_native_direct_array_flags(Some(0)),
        reserved: crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY,
        payload: 1,
        aux: entries.as_mut_ptr() as usize as u64,
    };
    let mut direct_next = 1_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut array_states = vec![crate::JitNativeDirectArrayState::default(); direct_slots.len()];
    array_states[0].next_append_key = 6;
    array_states[0].has_next_append_key = 1;
    let mut entry_next = crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY;
    let mut entry_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS];
    let mut roots_dirty = 0_u32;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_array_states: array_states.as_mut_ptr() as usize as u64,
        direct_array_entries: entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut entry_next) as usize as u64,
        direct_array_free_heads: entry_free.as_mut_ptr() as usize as u64,
        root_mutation_pending: std::ptr::from_mut(&mut roots_dirty) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let array = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[array, 10, 20], JIT_RUNTIME_ABI_HASH)
            .expect("direct native array push/pop execution"),
        23
    );
    // The function parameter is by-value. Its local COW mutation must not
    // overwrite the caller-owned slot even though both begin with one handle.
    assert_eq!(direct_slots[0].payload, 1);
    assert_eq!(
        entries[0],
        crate::JitNativeDirectArrayEntry { key: 5, value: 1 }
    );
    assert_eq!(roots_dirty, 1);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_array_pointer_builtins_mutate_authoritative_native_cursor() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_240));
    let file = builder.add_file("optimizing-array-pointer.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_array_pointer", FunctionFlags::default(), span);
    let array_local = typed_array_reference_param(&mut builder, function, "array");
    let block = builder.append_block(function);
    let array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: array,
            local: array_local,
        },
        span,
    );
    let by_value = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let by_local = |value| IrCallArg {
        by_ref_local: Some(array_local),
        ..by_value(value)
    };
    let current = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: current,
            name: "current".to_owned(),
            args: vec![by_value(Operand::Register(array))],
        },
        span,
    );
    let first_key = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: first_key,
            name: "key".to_owned(),
            args: vec![by_value(Operand::Register(array))],
        },
        span,
    );
    let next = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: next,
            name: "next".to_owned(),
            args: vec![by_local(Operand::Register(array))],
        },
        span,
    );
    let after_next = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: after_next,
            local: array_local,
        },
        span,
    );
    let second_key = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: second_key,
            name: "key".to_owned(),
            args: vec![by_value(Operand::Register(after_next))],
        },
        span,
    );
    let reset = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: reset,
            name: "reset".to_owned(),
            args: vec![by_local(Operand::Register(after_next))],
        },
        span,
    );
    let subtotal = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: subtotal,
            op: BinaryOp::Add,
            lhs: Operand::Register(current),
            rhs: Operand::Register(first_key),
        },
        span,
    );
    let subtotal_two = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: subtotal_two,
            op: BinaryOp::Add,
            lhs: Operand::Register(subtotal),
            rhs: Operand::Register(next),
        },
        span,
    );
    let subtotal_three = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: subtotal_three,
            op: BinaryOp::Add,
            lhs: Operand::Register(subtotal_two),
            rhs: Operand::Register(second_key),
        },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(subtotal_three),
            rhs: Operand::Register(reset),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.array-pointer").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_binary: forbidden_binary as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_local_store: forbidden_local_store as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("native array pointer handle");
    assert_optimizing_artifact(&handle);

    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    entries[0] = crate::JitNativeDirectArrayEntry { key: 4, value: 10 };
    entries[1] = crate::JitNativeDirectArrayEntry { key: 9, value: 20 };
    direct_slots[1] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: crate::jit_native_direct_array_flags(Some(0)),
        reserved: crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY,
        payload: 2,
        aux: entries.as_mut_ptr() as usize as u64,
    };
    let array = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    direct_slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        reserved: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        payload: array as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let mut direct_next = 2_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut entry_next = crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY;
    let mut entry_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS];
    let mut roots_dirty = 0_u32;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_array_entries: entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(&mut entry_next) as usize as u64,
        direct_array_free_heads: entry_free.as_mut_ptr() as usize as u64,
        root_mutation_pending: std::ptr::from_mut(&mut roots_dirty) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let reference = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[reference], JIT_RUNTIME_ABI_HASH)
            .expect("direct array pointer execution"),
        53
    );
    let referenced_array = crate::jit_decode_runtime_value(direct_slots[0].payload as i64)
        .expect("reference payload remains an encoded direct array")
        .checked_sub(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
        .expect("reference payload uses the direct arena") as usize;
    assert_eq!(
        crate::jit_native_direct_array_cursor(direct_slots[referenced_array].flags),
        Some(0)
    );
    assert_eq!(roots_dirty, 1);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_bind_global_uses_trusted_reference_slot_without_semantic_dispatch() {
    let mut builder = IrBuilder::new(UnitId::new(4_235));
    let file = builder.add_file("optimizing-bind-global.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("optimizing_bind_global", FunctionFlags::default(), span);
    let global = builder.intern_local(function, "wpdb");
    let block = builder.append_block(function);
    builder.emit(
        function,
        block,
        InstructionKind::BindGlobal {
            local: global,
            name: "wpdb".to_owned(),
        },
        span,
    );
    let null = builder.intern_constant(IrConstant::Null);
    builder.terminate_return(function, block, Some(Operand::Constant(null)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.bind-global")
            .with_opt_level(2)
            .with_deployment_runtime_identity(42),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_semantic_dispatch: forbidden_call_dispatch as *const () as usize,
            native_reference_bind: forbidden_reference_bind as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing global-binding handle");
    assert_optimizing_artifact(&handle);
    let metadata = handle
        .region_state_metadata()
        .expect("global-binding lowering metadata");
    assert!(
        metadata.production_lowering.iter().any(|entry| {
            entry.operation.contains("BindGlobal")
                && entry.class == crate::JitProductionLoweringClass::BaselineFragmentTransition
                && entry.operation_local_transition
        }),
        "unexpected lowering manifest: {:?}",
        metadata.production_lowering
    );

    let encoded_reference =
        crate::jit_encode_typed_runtime_value(0, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    let mut slots = vec![crate::JitNativeValueSlot::default(); 1];
    slots[0] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        ..crate::JitNativeValueSlot::default()
    };
    let mut function_offsets = vec![0_u32; function.index() + 1];
    let mut trusted_slots = vec![crate::JitNativeTrustedGlobalReferenceSlot {
        encoded: encoded_reference,
        reference_identity: 1,
        state: crate::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED,
        reserved: 0,
        reserved_wide: 0,
    }];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        trusted_property_function_offsets: function_offsets.as_mut_ptr() as usize as u64,
        trusted_property_function_count: function_offsets.len() as u32,
        trusted_global_reference_slots: trusted_slots.as_mut_ptr() as usize as u64,
        trusted_global_reference_slot_count: trusted_slots.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("trusted global binding must not side-exit"),
        crate::jit_encode_constant(u32::MAX)
    );
}

#[test]
fn optimizing_string_concat_allocates_direct_native_string_without_binary_helper() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_233));
    let file = builder.add_file("optimizing-string-concat.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_string_concat", FunctionFlags::default(), span);
    let left_local = typed_string_param(&mut builder, function, "left");
    let right_local = typed_string_param(&mut builder, function, "right");
    let block = builder.append_block(function);
    let left = builder.alloc_register(function);
    let right = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: left,
            local: left_local,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: right,
            local: right_local,
        },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Concat,
            lhs: Operand::Register(left),
            rhs: Operand::Register(right),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.string-concat").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_binary: forbidden_binary as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing concat handle");
    assert_optimizing_artifact(&handle);

    let left_bytes = b"hello ";
    let right_bytes = b"world";
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    for (slot, bytes) in direct_slots[..2]
        .iter_mut()
        .zip([left_bytes.as_slice(), right_bytes.as_slice()])
    {
        *slot = crate::JitNativeValueSlot {
            refcount: 1,
            kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            reserved: 0,
            payload: bytes.len() as u64,
            aux: bytes.as_ptr() as usize as u64,
        };
    }
    let mut direct_slot_next = 2_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut string_bytes = vec![0_u8; crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY];
    let mut string_next = 0_u32;
    let mut string_free =
        [crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE; crate::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS];
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_slot_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        direct_string_bytes: string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(&mut string_next) as usize as u64,
        direct_string_free_heads: string_free.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let left = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let right = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let result = handle
        .invoke_i64(&[left, right], JIT_RUNTIME_ABI_HASH)
        .expect("direct string concat");
    assert_eq!(
        crate::jit_decode_runtime_value(result),
        Some(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 2)
    );
    assert_eq!(&string_bytes[..11], b"hello world");
    assert_eq!(direct_slots[2].payload, 11);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_empty_local_uses_guarded_native_truthiness() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_216));
    let file = builder.add_file("optimizing-empty-local.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("optimizing_empty_local", FunctionFlags::default(), span);
    let local = untyped_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    let empty = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::EmptyLocal { dst: empty, local },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(empty)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.empty-local").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_cast: forbidden_cast as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_stable_length: forbidden_stable_length as *const () as usize,
            native_truthy: forbidden_truthy as *const () as usize,
            native_unary: forbidden_unary as *const () as usize,
            native_value_release: forbidden_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing empty-local handle");
    assert_optimizing_artifact(&handle);
    for (input, expected) in [
        (0, crate::jit_encode_constant(crate::JIT_VALUE_TRUE)),
        (1, crate::jit_encode_constant(crate::JIT_VALUE_FALSE)),
        (
            crate::jit_encode_constant(u32::MAX),
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        ),
        (
            crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        ),
    ] {
        assert_eq!(
            handle
                .invoke_i64(&[input], JIT_RUNTIME_ABI_HASH)
                .expect("empty-local execution"),
            expected
        );
    }
    let mut slots = vec![crate::JitNativeValueSlot::default(); 8];
    slots[7] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_ARRAY,
        flags: crate::JIT_NATIVE_ARRAY_VIEW_ABI_VERSION,
        payload: 0,
        ..crate::JitNativeValueSlot::default()
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let array = crate::jit_encode_typed_runtime_value(7, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    assert_eq!(
        handle
            .invoke_i64(&[array], JIT_RUNTIME_ABI_HASH)
            .expect("empty array execution"),
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE)
    );
    slots[7].payload = 1;
    assert_eq!(slots[7].payload, 1);
    assert_eq!(
        handle
            .invoke_i64(&[array], JIT_RUNTIME_ABI_HASH)
            .expect("non-empty array execution"),
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE)
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_unknown_truthiness_keeps_direct_values_in_clif() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_247));
    let file = builder.add_file("optimizing-unknown-truthiness.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function(
        "optimizing_unknown_truthiness",
        FunctionFlags::default(),
        span,
    );
    let local = untyped_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    let boolean = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Cast {
            dst: boolean,
            kind: CastKind::Bool,
            src: Operand::Register(loaded),
        },
        span,
    );
    let inverted = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Unary {
            dst: inverted,
            op: UnaryOp::Not,
            src: Operand::Register(boolean),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(inverted)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.unknown-truthiness").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_cast: forbidden_cast as *const () as usize,
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_truthy: forbidden_truthy as *const () as usize,
            native_unary: forbidden_unary as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("unknown truthiness handle");
    assert_optimizing_artifact(&handle);

    let mut direct_slots = vec![crate::JitNativeValueSlot::default(); 5];
    for (index, bits) in [0.0_f64.to_bits(), (-0.0_f64).to_bits(), 1.5_f64.to_bits()]
        .into_iter()
        .enumerate()
    {
        direct_slots[index] = crate::JitNativeValueSlot {
            refcount: 1,
            kind: crate::JIT_NATIVE_VALUE_VIEW_FLOAT,
            payload: bits,
            ..crate::JitNativeValueSlot::default()
        };
    }
    direct_slots[3] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY,
        flags: crate::JIT_NATIVE_SHARED_ARRAY_ABI_VERSION,
        reserved: 0,
        payload: 1,
        ..crate::JitNativeValueSlot::default()
    };
    direct_slots[4] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY,
        flags: crate::JIT_NATIVE_SHARED_ARRAY_ABI_VERSION,
        reserved: 2,
        payload: 1,
        ..crate::JitNativeValueSlot::default()
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let direct = |index, tag| {
        crate::jit_encode_typed_runtime_value(
            crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            tag,
        )
    };
    let true_ = crate::jit_encode_constant(crate::JIT_VALUE_TRUE);
    let false_ = crate::jit_encode_constant(crate::JIT_VALUE_FALSE);
    for (input, expected) in [
        (0, true_),
        (1, false_),
        (crate::jit_encode_constant(u32::MAX), true_),
        (crate::jit_encode_constant(crate::JIT_VALUE_FALSE), true_),
        (direct(0, crate::JIT_VALUE_RUNTIME_FLOAT_TAG), true_),
        (direct(1, crate::JIT_VALUE_RUNTIME_FLOAT_TAG), true_),
        (direct(2, crate::JIT_VALUE_RUNTIME_FLOAT_TAG), false_),
        (direct(3, crate::JIT_VALUE_RUNTIME_ARRAY_TAG), true_),
        (direct(4, crate::JIT_VALUE_RUNTIME_ARRAY_TAG), false_),
        (direct(0, crate::JIT_VALUE_RUNTIME_OBJECT_TAG), false_),
    ] {
        assert_eq!(
            handle
                .invoke_i64(&[input], JIT_RUNTIME_ABI_HASH)
                .expect("direct truthiness execution"),
            expected
        );
    }
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_builtin_type_predicate_uses_native_tag_test() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_213));
    let file = builder.add_file("optimizing-type-predicate.php");
    let span = IrSpan::new(file, 0, 1);
    // Deployment linking can attach a symbolic FunctionId to an internal
    // builtin.  Give that symbol an intentionally wrong body: optimized code
    // must still classify the global internal name before considering this
    // incidental user-function target or its inline summary.
    let symbolic_builtin =
        builder.start_function("internal::is_scalar", FunctionFlags::default(), span);
    untyped_param(&mut builder, symbolic_builtin, "value");
    let symbolic_block = builder.append_block(symbolic_builtin);
    let false_ = builder.intern_constant(IrConstant::Bool(false));
    builder.terminate_return(
        symbolic_builtin,
        symbolic_block,
        Some(Operand::Constant(false_)),
        span,
    );
    builder.register_function_name("is_scalar", symbolic_builtin);
    let function =
        builder.start_function("optimizing_type_predicate", FunctionFlags::default(), span);
    let local = untyped_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "is_scalar".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(loaded),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                // Preserve an lvalue location exactly as the frontend does
                // for WordPress calls. `is_scalar` is still a fixed by-value
                // builtin and must not enter the generic binder.
                by_ref_local: Some(local),
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.type-predicate").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_type_predicate: forbidden_type_predicate as *const () as usize,
            native_local_fetch: passthrough_local_fetch as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing type-predicate handle");
    assert_optimizing_artifact(&handle);
    let referenced_string =
        crate::jit_encode_typed_runtime_value(8, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    let mut reference_view = crate::JitNativeReferenceScalarView {
        abi_version: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        state: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        encoded: referenced_string,
    };
    let mut slots = vec![crate::JitNativeValueSlot::default(); 9];
    slots[7] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR,
        flags: crate::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        payload: std::ptr::from_mut(&mut reference_view) as usize as u64,
        ..crate::JitNativeValueSlot::default()
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: slots.len() as u32,
        value_slots: slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    for (input, expected) in [
        (
            crate::jit_encode_typed_runtime_value(7, crate::JIT_VALUE_RUNTIME_ARRAY_TAG),
            crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
        ),
        (
            crate::jit_encode_typed_runtime_value(8, crate::JIT_VALUE_RUNTIME_STRING_TAG),
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        ),
        (
            crate::jit_encode_typed_runtime_value(9, crate::JIT_VALUE_RUNTIME_OBJECT_TAG),
            crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
        ),
        (41, crate::jit_encode_constant(crate::JIT_VALUE_TRUE)),
        (
            crate::jit_encode_typed_runtime_value(7, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG),
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        ),
    ] {
        assert_eq!(
            handle
                .invoke_i64(&[input], JIT_RUNTIME_ABI_HASH)
                .expect("native type predicate execution"),
            expected
        );
    }
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_is_numeric_parses_native_string_bytes() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_240));
    let file = builder.add_file("optimizing-is-numeric.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("optimizing_is_numeric", FunctionFlags::default(), span);
    let local = untyped_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "is_numeric".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(loaded),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.is-numeric").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_type_predicate: forbidden_type_predicate as *const () as usize,
            native_local_fetch: passthrough_local_fetch as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing is_numeric handle");
    assert_optimizing_artifact(&handle);

    assert_eq!(
        handle
            .invoke_i64(&[42], JIT_RUNTIME_ABI_HASH)
            .expect("integer is_numeric"),
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE)
    );
    assert_eq!(
        handle
            .invoke_i64(
                &[crate::jit_encode_constant(crate::JIT_VALUE_TRUE)],
                JIT_RUNTIME_ABI_HASH,
            )
            .expect("boolean is_numeric"),
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE)
    );

    let mut direct_slots = [crate::JitNativeValueSlot::default(); 1];
    let encoded = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    for (bytes, expected) in [
        (&b"  +1.5e2 \x0c"[..], true),
        (&b".5"[..], true),
        (&b"1."[..], true),
        (&b"1e"[..], false),
        (&b"+"[..], false),
        (&b"1.5x"[..], false),
        (&b""[..], false),
    ] {
        direct_slots[0] = crate::JitNativeValueSlot {
            refcount: 1,
            kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            payload: bytes.len() as u64,
            aux: bytes.as_ptr() as usize as u64,
            reserved: 0,
        };
        assert_eq!(
            handle
                .invoke_i64(&[encoded], JIT_RUNTIME_ABI_HASH)
                .expect("native string is_numeric"),
            crate::jit_encode_constant(if expected {
                crate::JIT_VALUE_TRUE
            } else {
                crate::JIT_VALUE_FALSE
            }),
            "input={bytes:?}",
        );
    }
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_error_reporting_uses_exact_request_capability() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_241));
    let file = builder.add_file("optimizing-error-reporting.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_error_reporting", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let requested = builder.intern_constant(IrConstant::Int(123));
    let previous = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: previous,
            name: "error_reporting".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Constant(requested),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    let current = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: current,
            name: "error_reporting".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    let combined = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: combined,
            op: BinaryOp::Add,
            lhs: Operand::Register(previous),
            rhs: Operand::Register(current),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(combined)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.error-reporting").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_binary: forbidden_binary as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing error_reporting handle");
    assert_optimizing_artifact(&handle);
    let mut mask = 7_i64;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        error_reporting: std::ptr::from_mut(&mut mask) as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("direct error_reporting execution"),
        130
    );
    assert_eq!(mask, 123);
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_builtin_length_uses_versioned_value_view() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_214));
    let file = builder.add_file("optimizing-stable-length.php");
    let span = IrSpan::new(file, 0, 1);
    let function =
        builder.start_function("optimizing_stable_length", FunctionFlags::default(), span);
    let local = untyped_param(&mut builder, function, "value");
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "strlen".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(loaded),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.stable-length").with_opt_level(2),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_builtin_dispatch: forbidden_call_dispatch as *const () as usize,
            native_stable_length: forbidden_stable_length as *const () as usize,
            native_local_fetch: passthrough_local_fetch as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing stable-length handle");
    assert_optimizing_artifact(&handle);
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    direct_slots[7] = crate::JitNativeValueSlot {
        refcount: 1,
        kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: 17,
        ..crate::JitNativeValueSlot::default()
    };
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        ..crate::JitNativeRuntimeView::default()
    });
    let input = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 7,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    assert_eq!(
        handle
            .invoke_i64(&[input], JIT_RUNTIME_ABI_HASH)
            .expect("stable string length execution"),
        17
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_bounded_argument_wrapper_inlines_without_dispatch() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_215));
    let file = builder.add_file("optimizing-inline-argument.php");
    let span = IrSpan::new(file, 0, 1);
    let callee = builder.start_function("identity_wrapper", FunctionFlags::default(), span);
    let callee_local = untyped_param(&mut builder, callee, "value");
    let callee_block = builder.append_block(callee);
    let callee_value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: callee_value,
            local: callee_local,
        },
        span,
    );
    builder.terminate_return(
        callee,
        callee_block,
        Some(Operand::Register(callee_value)),
        span,
    );
    builder.register_function_name("identity_wrapper", callee);
    let caller = builder.start_function("inline_argument_caller", FunctionFlags::default(), span);
    let caller_local = untyped_param(&mut builder, caller, "value");
    let caller_block = builder.append_block(caller);
    let caller_value = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::LoadLocal {
            dst: caller_value,
            local: caller_local,
        },
        span,
    );
    let result = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "identity_wrapper".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(caller_value),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(caller, caller_block, Some(Operand::Register(result)), span);
    let unit = builder.finish();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.inline-argument").with_opt_level(2),
        unit: Some(&unit),
        function: Some(caller),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_local_fetch: passthrough_local_fetch as *const () as usize,
            native_value_release: passthrough_release as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("bounded argument inline handle");
    assert_optimizing_artifact(&handle);
    assert_eq!(handle.inlined_calls_per_invocation(), 1);
    assert_eq!(
        handle
            .invoke_i64(&[42], JIT_RUNTIME_ABI_HASH)
            .expect("bounded argument inline execution"),
        42
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_runtime_guarded_function_cell_calls_native_callee_without_dispatch() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_231));
    let file = builder.add_file("optimizing-published-function-cell.php");
    let span = IrSpan::new(file, 0, 1);

    let callee = builder.start_function("cell_increment", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let callee_local = typed_int_param(&mut builder, callee, "value");
    let callee_block = builder.append_block(callee);
    let loaded = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: loaded,
            local: callee_local,
        },
        span,
    );
    let one = builder.intern_constant(IrConstant::Int(1));
    let incremented = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::Binary {
            dst: incremented,
            op: BinaryOp::Add,
            lhs: Operand::Register(loaded),
            rhs: Operand::Constant(one),
        },
        span,
    );
    builder.terminate_return(
        callee,
        callee_block,
        Some(Operand::Register(incremented)),
        span,
    );
    builder.register_function_name("cell_increment", callee);

    let caller = builder.start_function("cell_caller", FunctionFlags::default(), span);
    builder.set_entry(caller);
    // The caller deliberately has no static parameter type.  The optimizing
    // artifact must emit a native integer guard and still call the published
    // callee cell directly for the admitted value; routing this through the
    // generic call dispatcher is forbidden.
    let caller_local = untyped_param(&mut builder, caller, "value");
    let caller_block = builder.append_block(caller);
    let argument = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::LoadLocal {
            dst: argument,
            local: caller_local,
        },
        span,
    );
    let result = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "cell_increment".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(argument),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                // The frontend preserves the caller lvalue even for a
                // by-value parameter. Callee metadata, not this location,
                // decides whether a binder is required.
                by_ref_local: Some(caller_local),
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(caller, caller_block, Some(Operand::Register(result)), span);
    let unit = builder.finish();

    let mut backend = CraneliftNativeCompiler;
    let callee_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.cell-callee").with_opt_level(2),
        unit: Some(&unit),
        function: Some(callee),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    let caller_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.cell-caller").with_opt_level(2),
        unit: Some(&unit),
        function: Some(caller),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_function_resolve: forbidden_call_dispatch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(
        callee_outcome.status,
        JitCompileStatus::Compiled,
        "{callee_outcome:?}"
    );
    assert_eq!(
        caller_outcome.status,
        JitCompileStatus::Compiled,
        "{caller_outcome:?}"
    );
    let callee_handle = callee_outcome.handle.expect("published callee handle");
    let caller_handle = caller_outcome.handle.expect("cell caller handle");
    assert_optimizing_artifact(&callee_handle);
    assert_optimizing_artifact(&caller_handle);
    let mut entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let mut optimizing_entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    entries[callee.index()].store(
        callee_handle
            .native_entry_address()
            .expect("callee executable address"),
        std::sync::atomic::Ordering::Release,
    );
    optimizing_entries[callee.index()].store(
        callee_handle
            .native_entry_address()
            .expect("callee executable address"),
        std::sync::atomic::Ordering::Release,
    );
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        trusted_function_entries: entries.as_mut_ptr() as usize as u64,
        trusted_function_entry_count: entries.len() as u32,
        trusted_optimizing_function_entries: optimizing_entries.as_mut_ptr() as usize as u64,
        trusted_optimizing_function_entry_count: optimizing_entries.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        caller_handle
            .invoke_i64(&[41], JIT_RUNTIME_ABI_HASH)
            .expect("compiled-to-compiled call"),
        42
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_variadic_call_packs_one_authoritative_native_array() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_244));
    let file = builder.add_file("optimizing-variadic-native-call.php");
    let span = IrSpan::new(file, 0, 1);

    let callee = builder.start_function("native_variadic_pick", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let rest = builder.intern_local(callee, "rest");
    builder.push_param(
        callee,
        IrParam {
            name: "rest".to_owned(),
            local: rest,
            required: false,
            default: None,
            type_: None,
            by_ref: false,
            variadic: true,
            attributes: Vec::new(),
        },
    );
    let callee_block = builder.append_block(callee);
    let array = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: array,
            local: rest,
        },
        span,
    );
    let picked = builder.alloc_register(callee);
    let one = builder.intern_constant(IrConstant::Int(1));
    builder.emit(
        callee,
        callee_block,
        InstructionKind::FetchDim {
            dst: picked,
            array: Operand::Register(array),
            key: Operand::Constant(one),
            quiet: false,
            mode: php_ir::instruction::DimFetchMode::Read,
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(picked)), span);
    builder.register_function_name("native_variadic_pick", callee);

    let caller = builder.start_function("native_variadic_caller", FunctionFlags::default(), span);
    builder.set_entry(caller);
    let caller_block = builder.append_block(caller);
    let result = builder.alloc_register(caller);
    let arguments = [10_i64, 20, 30]
        .into_iter()
        .map(|value| IrCallArg {
            name: None,
            value: Operand::Constant(builder.intern_constant(IrConstant::Int(value))),
            unpack: false,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: None,
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        })
        .collect();
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "native_variadic_pick".to_owned(),
            args: arguments,
        },
        span,
    );
    builder.terminate_return(caller, caller_block, Some(Operand::Register(result)), span);
    let unit = builder.finish();

    let mut backend = CraneliftNativeCompiler;
    let callee_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.variadic-callee").with_opt_level(2),
        unit: Some(&unit),
        function: Some(callee),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    let caller_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.variadic-caller").with_opt_level(2),
        unit: Some(&unit),
        function: Some(caller),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_function_resolve: forbidden_call_dispatch as *const () as usize,
            native_array_new: forbidden_array_insert as *const () as usize,
            native_array_insert: forbidden_array_insert as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(
        callee_outcome.status,
        JitCompileStatus::Compiled,
        "{callee_outcome:?}"
    );
    assert_eq!(
        caller_outcome.status,
        JitCompileStatus::Compiled,
        "{caller_outcome:?}"
    );
    let callee_handle = callee_outcome.handle.expect("variadic callee handle");
    let caller_handle = caller_outcome.handle.expect("variadic caller handle");
    assert_optimizing_artifact(&callee_handle);
    assert_optimizing_artifact(&caller_handle);

    let mut entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let mut optimizing_entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let callee_address = callee_handle
        .native_entry_address()
        .expect("variadic callee executable address");
    entries[callee.index()].store(callee_address, std::sync::atomic::Ordering::Release);
    optimizing_entries[callee.index()].store(callee_address, std::sync::atomic::Ordering::Release);
    let mut direct_slots =
        vec![crate::JitNativeValueSlot::default(); crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut direct_entries = vec![
        crate::JitNativeDirectArrayEntry::default();
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
    ];
    let mut direct_next = 0_u32;
    let mut entry_next = 0_u32;
    let _arena = activate_direct_test_arena(
        &mut direct_slots,
        &mut direct_next,
        &mut direct_entries,
        &mut entry_next,
    );
    let _entries = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        trusted_function_entries: entries.as_mut_ptr() as usize as u64,
        trusted_function_entry_count: entries.len() as u32,
        trusted_optimizing_function_entries: optimizing_entries.as_mut_ptr() as usize as u64,
        trusted_optimizing_function_entry_count: optimizing_entries.len() as u32,
        ..crate::abi::current_native_runtime_view()
    });
    assert_eq!(
        caller_handle
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("compiled variadic call"),
        20
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_compiled_call_releases_its_borrowed_argument_owner() {
    let mut builder = IrBuilder::new(UnitId::new(4_239));
    let file = builder.add_file("optimizing-call-argument-owner.php");
    let span = IrSpan::new(file, 0, 1);

    let callee = builder.start_function("ignore_string", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let callee_local = builder.intern_local(callee, "value");
    builder.push_param(
        callee,
        IrParam {
            name: "value".to_owned(),
            local: callee_local,
            required: true,
            default: None,
            type_: Some(IrReturnType::String),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let increment_local = typed_int_param(&mut builder, callee, "increment");
    let callee_block = builder.append_block(callee);
    let increment = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: increment,
            local: increment_local,
        },
        span,
    );
    let one = builder.intern_constant(IrConstant::Int(1));
    let incremented = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::Binary {
            dst: incremented,
            op: BinaryOp::Add,
            lhs: Operand::Register(increment),
            rhs: Operand::Constant(one),
        },
        span,
    );
    builder.terminate_return(
        callee,
        callee_block,
        Some(Operand::Register(incremented)),
        span,
    );
    builder.register_function_name("ignore_string", callee);

    let caller = builder.start_function("call_ignore_string", FunctionFlags::default(), span);
    builder.set_entry(caller);
    let caller_local = builder.intern_local(caller, "value");
    builder.push_param(
        caller,
        IrParam {
            name: "value".to_owned(),
            local: caller_local,
            required: true,
            default: None,
            type_: Some(IrReturnType::String),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let caller_block = builder.append_block(caller);
    let argument = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::LoadLocal {
            dst: argument,
            local: caller_local,
        },
        span,
    );
    let result = builder.alloc_register(caller);
    let forty_one = builder.intern_constant(IrConstant::Int(41));
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "ignore_string".to_owned(),
            args: vec![
                IrCallArg {
                    name: None,
                    value: Operand::Register(argument),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: Some(caller_local),
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
                IrCallArg {
                    name: None,
                    value: Operand::Constant(forty_one),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
            ],
        },
        span,
    );
    builder.emit(
        caller,
        caller_block,
        InstructionKind::Discard {
            src: Operand::Register(argument),
        },
        span,
    );
    builder.terminate_return(caller, caller_block, Some(Operand::Register(result)), span);
    let unit = builder.finish();

    let mut backend = CraneliftNativeCompiler;
    let callee_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.owner-callee").with_opt_level(2),
        unit: Some(&unit),
        function: Some(callee),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    let caller_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.owner-caller").with_opt_level(2),
        unit: Some(&unit),
        function: Some(caller),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_function_resolve: forbidden_call_dispatch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(callee_outcome.status, JitCompileStatus::Compiled);
    assert_eq!(caller_outcome.status, JitCompileStatus::Compiled);
    let callee_handle = callee_outcome.handle.expect("owner callee handle");
    let caller_handle = caller_outcome.handle.expect("owner caller handle");
    assert_optimizing_artifact(&callee_handle);
    assert_optimizing_artifact(&caller_handle);

    let mut entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let mut optimizing_entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let callee_address = callee_handle
        .native_entry_address()
        .expect("owner callee executable address");
    entries[callee.index()].store(callee_address, std::sync::atomic::Ordering::Release);
    optimizing_entries[callee.index()].store(callee_address, std::sync::atomic::Ordering::Release);
    let bytes = b"owned once";
    let mut direct_slots = vec![
        crate::JitNativeValueSlot {
            refcount: 1,
            kind: crate::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: crate::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            payload: bytes.len() as u64,
            aux: bytes.as_ptr() as usize as u64,
            ..crate::JitNativeValueSlot::default()
        };
        crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY
    ];
    let mut direct_next = 1_u32;
    let mut direct_free = crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: direct_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(&mut direct_next) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(&mut direct_free) as usize as u64,
        trusted_function_entries: entries.as_mut_ptr() as usize as u64,
        trusted_function_entry_count: entries.len() as u32,
        trusted_optimizing_function_entries: optimizing_entries.as_mut_ptr() as usize as u64,
        trusted_optimizing_function_entry_count: optimizing_entries.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    let encoded = crate::jit_encode_typed_runtime_value(
        crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        crate::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    assert_eq!(
        caller_handle
            .invoke_i64(&[encoded], JIT_RUNTIME_ABI_HASH)
            .expect("compiled call with borrowed string"),
        42
    );
    assert_eq!(
        direct_slots[0].refcount, 1,
        "compiled call leaked its argument owner"
    );
}

#[test]
fn optimizing_prepared_default_calls_native_callee_without_dispatch() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_233));
    let file = builder.add_file("optimizing-prepared-default-call.php");
    let span = IrSpan::new(file, 0, 1);
    let default = builder.intern_constant(IrConstant::Int(1));

    let callee = builder.start_function("default_increment", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let value = typed_int_param(&mut builder, callee, "value");
    let increment = builder.intern_local(callee, "increment");
    builder.push_param(
        callee,
        IrParam {
            name: "increment".to_owned(),
            local: increment,
            required: false,
            default: Some(IrConstant::Int(1)),
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let callee_block = builder.append_block(callee);
    let loaded_value = builder.alloc_register(callee);
    let loaded_increment = builder.alloc_register(callee);
    let result = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: loaded_value,
            local: value,
        },
        span,
    );
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: loaded_increment,
            local: increment,
        },
        span,
    );
    builder.emit(
        callee,
        callee_block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(loaded_value),
            rhs: Operand::Register(loaded_increment),
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(result)), span);
    builder.register_function_name("default_increment", callee);

    let caller = builder.start_function("default_increment_caller", FunctionFlags::default(), span);
    builder.set_entry(caller);
    let caller_value = typed_int_param(&mut builder, caller, "value");
    let caller_block = builder.append_block(caller);
    let argument = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::LoadLocal {
            dst: argument,
            local: caller_value,
        },
        span,
    );
    let call_result = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallFunction {
            dst: call_result,
            name: "default_increment".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(argument),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
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
    let unit = builder.finish();
    assert_eq!(unit.constants[default.index()], IrConstant::Int(1));

    let mut backend = CraneliftNativeCompiler;
    let callee_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.prepared-default-callee").with_opt_level(2),
        unit: Some(&unit),
        function: Some(callee),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    let caller_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.prepared-default-caller").with_opt_level(2),
        unit: Some(&unit),
        function: Some(caller),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_function_resolve: forbidden_call_dispatch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(
        callee_outcome.status,
        JitCompileStatus::Compiled,
        "{callee_outcome:?}"
    );
    assert_eq!(
        caller_outcome.status,
        JitCompileStatus::Compiled,
        "{caller_outcome:?}"
    );
    let callee_handle = callee_outcome
        .handle
        .expect("prepared-default callee handle");
    let caller_handle = caller_outcome
        .handle
        .expect("prepared-default caller handle");
    assert_optimizing_artifact(&callee_handle);
    assert_optimizing_artifact(&caller_handle);

    let mut entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let mut optimizing_entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let callee_address = callee_handle
        .native_entry_address()
        .expect("prepared-default callee executable address");
    entries[callee.index()].store(callee_address, std::sync::atomic::Ordering::Release);
    optimizing_entries[callee.index()].store(callee_address, std::sync::atomic::Ordering::Release);
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        trusted_function_entries: entries.as_mut_ptr() as usize as u64,
        trusted_function_entry_count: entries.len() as u32,
        trusted_optimizing_function_entries: optimizing_entries.as_mut_ptr() as usize as u64,
        trusted_optimizing_function_entry_count: optimizing_entries.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        callee_handle
            .invoke_i64(&[41, 1], JIT_RUNTIME_ABI_HASH)
            .expect("prepared-default callee direct execution"),
        42
    );
    assert_eq!(
        caller_handle
            .invoke_i64(&[41], JIT_RUNTIME_ABI_HASH)
            .expect("prepared-default compiled call"),
        42
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn optimizing_prepared_method_default_calls_native_callee_without_dispatch() {
    SSA_FORBIDDEN_HELPER_CALLS.store(0, Ordering::SeqCst);
    let mut builder = IrBuilder::new(UnitId::new(4_234));
    let file = builder.add_file("optimizing-prepared-method-default-call.php");
    let span = IrSpan::new(file, 0, 1);
    builder.intern_constant(IrConstant::Int(1));

    let callee = builder.start_function(
        "PreparedDefault::increment",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.set_return_type(callee, Some(IrReturnType::Int));
    builder.intern_local(callee, "this");
    let value = typed_int_param(&mut builder, callee, "value");
    let increment = builder.intern_local(callee, "increment");
    builder.push_param(
        callee,
        IrParam {
            name: "increment".to_owned(),
            local: increment,
            required: false,
            default: Some(IrConstant::Int(1)),
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let callee_block = builder.append_block(callee);
    let loaded_value = builder.alloc_register(callee);
    let loaded_increment = builder.alloc_register(callee);
    let result = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: loaded_value,
            local: value,
        },
        span,
    );
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: loaded_increment,
            local: increment,
        },
        span,
    );
    builder.emit(
        callee,
        callee_block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(loaded_value),
            rhs: Operand::Register(loaded_increment),
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(result)), span);

    let caller = builder.start_function(
        "PreparedDefault::callIncrement",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.set_entry(caller);
    let this = builder.intern_local(caller, "this");
    let caller_value = typed_int_param(&mut builder, caller, "value");
    let caller_block = builder.append_block(caller);
    let receiver = builder.alloc_register(caller);
    let argument = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::LoadLocal {
            dst: receiver,
            local: this,
        },
        span,
    );
    builder.emit(
        caller,
        caller_block,
        InstructionKind::LoadLocal {
            dst: argument,
            local: caller_value,
        },
        span,
    );
    let call_result = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallMethod {
            dst: call_result,
            object: Operand::Register(receiver),
            method: "increment".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(argument),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
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
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "prepareddefault".to_owned(),
        display_name: "PreparedDefault".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![
            ClassMethodEntry {
                name: "increment".to_owned(),
                origin_class: "prepareddefault".to_owned(),
                function: callee,
                flags: ClassMethodFlags {
                    has_body: true,
                    ..ClassMethodFlags::default()
                },
                attributes: Vec::new(),
            },
            ClassMethodEntry {
                name: "callincrement".to_owned(),
                origin_class: "prepareddefault".to_owned(),
                function: caller,
                flags: ClassMethodFlags {
                    has_body: true,
                    ..ClassMethodFlags::default()
                },
                attributes: Vec::new(),
            },
        ],
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor: None,
        flags: ClassFlags {
            is_final: true,
            ..ClassFlags::default()
        },
        span,
    });
    let unit = builder.finish();

    let mut backend = CraneliftNativeCompiler;
    let callee_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.prepared-method-default-callee")
            .with_opt_level(2),
        unit: Some(&unit),
        function: Some(callee),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    let caller_outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.optimizing.prepared-method-default-caller")
            .with_opt_level(2),
        unit: Some(&unit),
        function: Some(caller),
        runtime_helpers: crate::JitRuntimeHelperAddresses {
            native_call_dispatch: forbidden_call_dispatch as *const () as usize,
            native_function_resolve: forbidden_call_dispatch as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });
    assert_eq!(
        callee_outcome.status,
        JitCompileStatus::Compiled,
        "{callee_outcome:?}"
    );
    assert_eq!(
        caller_outcome.status,
        JitCompileStatus::Compiled,
        "{caller_outcome:?}"
    );
    let callee_handle = callee_outcome
        .handle
        .expect("prepared-method-default callee handle");
    let caller_handle = caller_outcome
        .handle
        .expect("prepared-method-default caller handle");
    assert_optimizing_artifact(&callee_handle);
    assert_optimizing_artifact(&caller_handle);

    let mut entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let mut optimizing_entries = (0..unit.functions.len())
        .map(|_| std::sync::atomic::AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let callee_address = callee_handle
        .native_entry_address()
        .expect("prepared-method-default callee executable address");
    entries[callee.index()].store(callee_address, std::sync::atomic::Ordering::Release);
    optimizing_entries[callee.index()].store(callee_address, std::sync::atomic::Ordering::Release);
    let _view = crate::activate_native_runtime_view(crate::JitNativeRuntimeView {
        abi_version: crate::JIT_RUNTIME_ABI_VERSION,
        trusted_function_entries: entries.as_mut_ptr() as usize as u64,
        trusted_function_entry_count: entries.len() as u32,
        trusted_optimizing_function_entries: optimizing_entries.as_mut_ptr() as usize as u64,
        trusted_optimizing_function_entry_count: optimizing_entries.len() as u32,
        ..crate::JitNativeRuntimeView::default()
    });
    assert_eq!(
        caller_handle
            .invoke_i64(&[0, 41], JIT_RUNTIME_ABI_HASH)
            .expect("prepared-method-default compiled call"),
        42
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
}
