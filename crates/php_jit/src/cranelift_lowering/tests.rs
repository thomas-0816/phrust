use super::{CraneliftNativeCompiler, build_trivial_add_clif_smoke, native_dim_operation};
use crate::{
    JIT_RUNTIME_ABI_HASH, JitCompileRequest, JitCompileStatus, NativeCompileRequest,
    NativeCompilerApi,
};
use php_ir::instruction::{IrCallArg, IrCallArgValueKind};
use php_ir::{
    BinaryOp, FunctionFlags, FunctionId, InstructionKind, IrBuilder, IrConstant, IrParam,
    IrReturnType, IrSpan, LocalId, Operand, UnitId,
};
use std::sync::atomic::{AtomicUsize, Ordering};

static NATIVE_DYNAMIC_EFFECTS: AtomicUsize = AtomicUsize::new(0);
static SSA_FORBIDDEN_HELPER_CALLS: AtomicUsize = AtomicUsize::new(0);

extern "C" fn forbidden_local_fetch(
    _context: u64,
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
    _context: u64,
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

extern "C" fn forbidden_lifecycle(_context: u64, _op: u32, _value: i64, _out: *mut i64) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

#[allow(unsafe_code)]
extern "C" fn frame_cleanup_only_lifecycle(
    _context: u64,
    op: u32,
    value: i64,
    out: *mut i64,
) -> i32 {
    let is_frame_cleanup =
        op & 0x8000_0000 != 0 && op & 1 == 1 && ((op >> 11) & 0x0f_ffff) == 0x0f_ffff;
    if !is_frame_cleanup || out.is_null() {
        SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    // SAFETY: generated code owns this synchronous stack output slot.
    unsafe { out.write(value) };
    0
}

extern "C" fn forbidden_binary(
    _context: u64,
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
    _context: u64,
    _op: u32,
    _lhs: i64,
    _rhs: i64,
    _out: *mut i64,
) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

extern "C" fn forbidden_truthy(_context: u64, _value: i64, _out: *mut i64) -> i32 {
    SSA_FORBIDDEN_HELPER_CALLS.fetch_add(1, Ordering::SeqCst);
    crate::JitCallStatus::RUNTIME_ERROR.0 as i32
}

#[allow(unsafe_code)]
extern "C" fn test_array_new(_context: u64, _op: u32, out: *mut i64) -> i32 {
    if out.is_null() {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    // SAFETY: generated code owns this synchronous stack output slot.
    unsafe { out.write(crate::jit_encode_runtime_value(7)) };
    0
}

#[allow(unsafe_code)]
extern "C" fn test_array_insert(
    _context: u64,
    append: u32,
    array: i64,
    _key: i64,
    _value: i64,
    out: *mut i64,
) -> i32 {
    if append != 1 || out.is_null() {
        return crate::JitCallStatus::RUNTIME_ERROR.0 as i32;
    }
    // SAFETY: generated code owns this synchronous stack output slot.
    unsafe { out.write(array) };
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
        i64::MAX as u64
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
            native_value_lifecycle: forbidden_lifecycle as *const () as usize,
            native_truthy: forbidden_truthy as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing SSA handle");
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
    assert_eq!(
        outcome
            .handle
            .expect("optimizing shift handle")
            .invoke_i64(&[-3], JIT_RUNTIME_ABI_HASH)
            .expect("optimizing shift execution"),
        -1
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
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
            native_value_lifecycle: frame_cleanup_only_lifecycle as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("optimizing handle move");
    assert!(handle.ssa_metrics().2 > 0);
    handle
        .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
        .expect("optimizing handle move execution");
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
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
            native_local_fetch: forbidden_local_fetch as *const () as usize,
            native_local_store: forbidden_local_store as *const () as usize,
            native_value_lifecycle: frame_cleanup_only_lifecycle as *const () as usize,
            ..crate::JitRuntimeHelperAddresses::default()
        },
    });

    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    assert_eq!(
        outcome
            .handle
            .expect("optimizing array append handle")
            .invoke_i64(&[], JIT_RUNTIME_ABI_HASH)
            .expect("optimizing array append execution"),
        9
    );
    assert_eq!(SSA_FORBIDDEN_HELPER_CALLS.load(Ordering::SeqCst), 0);
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
fn native_unwind_catches_throw_from_direct_compiled_callee() {
    extern "C" fn throwing_trampoline(
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
    let caught = builder.intern_constant(IrConstant::Int(77));
    builder.terminate_return(caller, catch, Some(Operand::Constant(caught)), span);
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
        compile: &JitCompileRequest::new("cl.region.register-capacity"),
        unit: Some(&unit),
        function: Some(function),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let crate::JitI64InvokeOutcome::SideExit { status, state, .. } = outcome
        .handle
        .expect("large-register region should compile")
        .invoke_i64_with_deopt(&[i64::MAX], JIT_RUNTIME_ABI_HASH)
        .expect("overflow should side-exit without corrupting the native frame")
    else {
        panic!("overflow unexpectedly returned");
    };
    assert_eq!(status, crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32);
    assert_eq!(state.initialized_register_mask, u64::MAX);
    assert_eq!(state.registers[63], 63);
}

#[test]
fn native_resume_loader_code_is_bounded_by_abi_register_capacity() {
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
    let handle = outcome
        .handle
        .expect("large-register region should compile");
    assert!(
        handle.code_bytes() < 600_000,
        "resume loader code grew beyond the ABI-bounded budget: {} bytes",
        handle.code_bytes()
    );
    let metadata = handle
        .region_state_metadata()
        .expect("resume loader metadata");
    assert!(
        metadata.native_transitions.len() <= crate::JIT_DEOPT_MAX_REGISTERS + 1,
        "metadata must not advertise transitions after the bounded loader stops"
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
    for (index, register) in transition.live_registers.iter().enumerate() {
        state.initialized_register_mask |= 1_u64 << register.raw();
        state.registers[register.index()] = if index == 0 { 41 } else { 1 };
    }
    assert_eq!(
        handle
            .invoke_i64_native_transition(&state, JIT_RUNTIME_ABI_HASH)
            .expect("baseline continuation should execute"),
        crate::JitI64InvokeOutcome::Returned(42)
    );
}

#[test]
fn nested_callee_transition_uses_published_native_function_entry() {
    let (unit, root, callee) = scalar_direct_call_fixture();
    let mut backend = CraneliftNativeCompiler;
    let outcome = backend.compile_region(&NativeCompileRequest {
        compile: &JitCompileRequest::new("cl.native-transition.nested"),
        unit: Some(&unit),
        function: Some(root),
        runtime_helpers: crate::JitRuntimeHelperAddresses::default(),
    });
    assert_eq!(outcome.status, JitCompileStatus::Compiled, "{outcome:?}");
    let handle = outcome.handle.expect("compiled call graph");
    let metadata = handle.region_state_metadata().expect("transition metadata");
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
    for register in &transition.live_registers {
        state.initialized_register_mask |= 1_u64 << register.raw();
        state.registers[register.index()] = 41;
    }
    assert_eq!(
        handle
            .invoke_i64_native_transition(&state, JIT_RUNTIME_ABI_HASH)
            .expect("callee transition should execute"),
        crate::JitI64InvokeOutcome::Returned(42)
    );
}

#[test]
fn optimized_exit_after_effect_does_not_repeat_effect_in_baseline() {
    NATIVE_DYNAMIC_EFFECTS.store(0, Ordering::SeqCst);
    let (unit, function) = effect_then_overflow_fixture();
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
    let outcome = optimized
        .handle
        .expect("optimized handle")
        .invoke_i64_with_native_transition(
            &baseline.handle.expect("baseline handle"),
            &[],
            JIT_RUNTIME_ABI_HASH,
        )
        .expect("native-to-native transition should execute");
    assert!(matches!(
        outcome,
        crate::JitI64InvokeOutcome::SideExit { status, .. }
            if status == crate::JitCallStatus::RECOMPILE_REQUESTED.0 as i32
    ));
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

fn effect_then_overflow_fixture() -> (php_ir::IrUnit, FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(0));
    let file = builder.add_file("native-transition-effect.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("effect_then_guard", FunctionFlags::default(), span);
    builder.set_entry(function);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let block = builder.append_block(function);
    let path = builder.add_constant(IrConstant::Int(5));
    let one = builder.add_constant(IrConstant::Int(1));
    let effect = builder.alloc_register(function);
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
    builder.emit_load_const(function, block, increment, one, span);
    builder.emit(
        function,
        block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(effect),
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
