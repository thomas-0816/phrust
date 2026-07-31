use super::*;
use crate::region_ir::{
    SsaCertainty, SsaOwnership, SsaValueClass, SsaValueFact, analyze_baseline_value_ownership,
    analyze_executable_value_flow,
};
use php_ir::instruction::{ClosureCaptureArg, IrCallDimTarget, IrCallPropertyTarget};
use php_ir::{
    ClassEntry, ClassFlags, ClassId, ClassMethodEntry, ClassMethodFlags, ConstId, FunctionFlags,
    IrBuilder, IrCapture, IrParam, IrSpan, UnitId,
};

#[test]
fn colliding_integer_constant_keeps_its_constant_identity_until_direct_publication() {
    let mut unit = php_ir::IrUnit::new(UnitId::new(0));
    unit.constants.push(IrConstant::Int(0x7ff1_0000_0000_0000));
    unit.constants.push(IrConstant::Int(42));

    assert_eq!(
        lower_constant(&unit, ConstId::new(0)),
        RegionOperand::Constant(0)
    );
    assert_eq!(
        lower_constant(&unit, ConstId::new(1)),
        RegionOperand::I64(42)
    );
}

fn builtin_call_with_local_arguments(name: &str, argument_count: usize) -> RegionNativeCall {
    let local = LocalId::new(0);
    let args = (0..argument_count)
        .map(|_| IrCallArg {
            name: None,
            value: Operand::Local(local),
            unpack: false,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: Some(local),
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        })
        .collect();
    RegionNativeCall {
        result: RegionCallResult::Discard,
        target: RegionCallTarget::Function {
            name: name.to_owned(),
            function: None,
        },
        args,
        argument_operand_offset: 0,
        operands: vec![None; argument_count],
        direct_arity: None,
        variadic: false,
        returns_by_reference: false,
        caller_strict_types: false,
    }
}

#[test]
fn native_call_liveness_includes_by_reference_property_object() {
    let mut builder = IrBuilder::new(UnitId::new(96));
    let file = builder.add_file("call-property.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function("call_property", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let constant = builder.intern_constant(IrConstant::Int(1));
    let object = builder.alloc_register(function);
    let value = builder.alloc_register(function);
    for register in [object, value] {
        builder.emit(
            function,
            block,
            InstructionKind::LoadConst {
                dst: register,
                constant,
            },
            span,
        );
    }
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "mysqli_query".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(value),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: Some(IrCallPropertyTarget {
                    object: Operand::Register(object),
                    property: "dbh".to_owned(),
                }),
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("native call region");
    let call = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|instruction| matches!(instruction.kind, RegionInstructionKind::NativeCall(_)))
        .expect("native call instruction");
    assert!(call.register_uses().contains(&object));
}

#[test]
fn namespaced_builtin_reference_requirements_fall_back_to_global_metadata() {
    let preg_match = builtin_call_with_local_arguments("wporg\\requests\\preg_match", 3);
    assert!(!preg_match.argument_requires_reference_binding(0));
    assert!(!preg_match.argument_requires_reference_binding(1));
    assert!(preg_match.argument_requires_reference_binding(2));

    let get_object_vars = builtin_call_with_local_arguments("fixture\\magic\\get_object_vars", 1);
    assert!(!get_object_vars.argument_requires_reference_binding(0));
}

#[test]
fn namespaced_builtin_publication_uses_the_fixed_global_identity() {
    assert_eq!(
        resolved_internal_builtin_name("WpOrg\\Requests\\ksort"),
        Some("ksort")
    );
    let parameters = internal_builtin_binding_parameters("WpOrg\\Requests\\ksort")
        .expect("global ksort arginfo");
    assert!(parameters[0].by_ref);
    assert!(!parameters[1].by_ref);
}

#[test]
fn namespaced_builtin_reference_argument_load_is_quiet() {
    let mut builder = IrBuilder::new(UnitId::new(97));
    let file = builder.add_file("namespaced-reference.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function("Fixture\\Preg\\parse", FunctionFlags::default(), span);
    let matches = builder.intern_local(function, "matches");
    let block = builder.append_block(function);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadLocal {
            dst: loaded,
            local: matches,
        },
        span,
    );
    let null = builder.intern_constant(IrConstant::Null);
    let argument = |value, by_ref_local| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "fixture\\preg\\preg_match".to_owned(),
            args: vec![
                argument(Operand::Constant(null), None),
                argument(Operand::Constant(null), None),
                argument(Operand::Register(loaded), Some(matches)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("namespaced builtin region");
    assert!(region.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::LoadLocal {
                dst,
                local,
                quiet: true,
            } if dst == loaded && local == matches
        )
    }));
}

#[test]
fn known_by_reference_dimension_binds_the_existing_slot_identity() {
    let mut builder = IrBuilder::new(UnitId::new(9_701));
    let file = builder.add_file("by-reference-dimension.php");
    let span = IrSpan::new(file, 0, 40);

    let caller = builder.start_function("caller", FunctionFlags::default(), span);
    let array = builder.intern_local(caller, "array");
    let caller_block = builder.append_block(caller);
    let zero = builder.intern_constant(IrConstant::Int(0));
    let key = builder.alloc_register(caller);
    builder.emit_load_const(caller, caller_block, key, zero, span);
    let value = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::FetchDim {
            dst: value,
            array: Operand::Local(array),
            key: Operand::Register(key),
            quiet: false,
            mode: php_ir::instruction::DimFetchMode::Lvalue,
        },
        span,
    );

    let callee = builder.start_function("callee", FunctionFlags::default(), span);
    builder.register_function_name("callee", callee);
    let parameter = builder.intern_local(callee, "value");
    builder.push_param(
        callee,
        IrParam {
            name: "value".to_owned(),
            local: parameter,
            required: true,
            type_: None,
            by_ref: true,
            variadic: false,
            default: None,
            attributes: Vec::new(),
        },
    );
    let callee_block = builder.append_block(callee);
    builder.terminate_return(callee, callee_block, None, span);

    let result = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "callee".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(value),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: Some(IrCallDimTarget {
                    local: array,
                    dims: vec![Operand::Register(key)],
                }),
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(caller, caller_block, None, span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, caller).expect("by-reference dimension region");
    let binding = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::BindReferenceDim {
                target,
                array: bound_array,
                keys,
            } => Some((*target, *bound_array, keys.clone())),
            _ => None,
        })
        .expect("dimension reference binding");
    assert_eq!(binding.1, array);
    assert_eq!(binding.2, vec![RegionOperand::Register(key)]);
    assert!(!region.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::BindReferenceIntoDim { .. }
        )
    }));
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
        .expect("native call");
    assert_eq!(call.args[0].by_ref_local, Some(binding.0));
    assert!(call.args[0].by_ref_dim.is_none());
    assert!(call.args[0].by_ref_property.is_none());
    assert!(call.args[0].by_ref_property_dim.is_none());

    let optimizing_region = BaselineRegionBuilder::build(
        &unit,
        caller,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing by-reference dimension region");
    let optimizing_call_instruction = optimizing_region.blocks[0]
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.kind, RegionInstructionKind::NativeCall(_)))
        .expect("optimizing native call");
    let RegionInstructionKind::NativeCall(optimizing_call) = &optimizing_call_instruction.kind
    else {
        unreachable!("filtered native call")
    };
    assert!(optimizing_call.args[0].by_ref_local.is_some());
    assert!(optimizing_call.args[0].by_ref_dim.is_none());
    assert!(optimizing_call.args[0].by_ref_property.is_none());
    assert!(optimizing_call.args[0].by_ref_property_dim.is_none());
    assert!(!optimizing_call_instruction.register_uses().contains(&key));

    let mut noncanonical = optimizing_region.clone();
    let noncanonical_call = noncanonical.blocks[0]
        .instructions
        .iter_mut()
        .find_map(|instruction| match &mut instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
        .expect("noncanonical native call");
    noncanonical_call.args[0].by_ref_dim = Some(IrCallDimTarget {
        local: array,
        dims: vec![Operand::Register(key)],
    });
    let error = noncanonical
        .verify()
        .expect_err("optimizing call must reject a second lvalue authority");
    assert_eq!(
        error.code,
        "JIT_REGION_REJECT_NONCANONICAL_REFERENCE_ARGUMENT"
    );
}

#[test]
fn malformed_conditional_terminator_returns_contextual_compile_error() {
    let mut builder = IrBuilder::new(UnitId::new(98));
    let file = builder.add_file("missing-fallthrough.php");
    let span = IrSpan::new(file, 4, 12);
    let function = builder.start_function("missing_fallthrough", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let condition = builder.intern_constant(IrConstant::Bool(true));
    builder.terminate_jump_if_false(function, block, Operand::Constant(condition), block, span);

    let error = build_baseline_region(&builder.finish(), function)
        .expect_err("last-block conditional terminator must be rejected");
    assert_eq!(error.code, "JIT_REGION_REJECT_FALLTHROUGH");
    assert!(error.detail.contains("function=missing_fallthrough"));
    assert!(error.detail.contains("block=0"));
    assert!(error.detail.contains("span=0:4-12"));
}

#[test]
fn invalid_operand_returns_instruction_context_before_cranelift() {
    let mut builder = IrBuilder::new(UnitId::new(99));
    let file = builder.add_file("invalid-operand.php");
    let span = IrSpan::new(file, 8, 19);
    let function = builder.start_function("invalid_operand", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let dst = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::Move {
            dst,
            src: Operand::Register(RegId::new(99)),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(dst)), span);

    let error = build_baseline_region(&builder.finish(), function)
        .expect_err("invalid operand must be rejected before publication");
    assert_eq!(error.code, "JIT_REGION_REJECT_INVALID_IR");
    assert!(error.detail.contains("function=0"), "{}", error.detail);
    assert!(error.detail.contains("block=0"), "{}", error.detail);
    assert!(error.detail.contains("instruction=0"), "{}", error.detail);
    assert!(error.detail.contains("span=0:8-19"), "{}", error.detail);
    assert!(error.detail.contains("operand/state"), "{}", error.detail);
    assert!(error.detail.contains("register 99"), "{}", error.detail);
}

#[test]
fn builds_verified_multiblock_region_from_php_ir() {
    let mut builder = IrBuilder::new(UnitId::new(91));
    let file = builder.add_file("region.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("region", FunctionFlags::default(), span);
    let local = builder.intern_local(function, "value");
    builder.push_param(
        function,
        IrParam {
            name: "value".to_owned(),
            local,
            required: true,
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            default: None,
            attributes: Vec::new(),
        },
    );
    builder.set_return_type(function, Some(IrReturnType::Int));
    let entry = builder.append_block(function);
    let body = builder.append_block(function);
    builder.terminate_jump(function, entry, body, span);
    let loaded = builder.alloc_register(function);
    builder.emit(
        function,
        body,
        InstructionKind::LoadLocal { dst: loaded, local },
        span,
    );
    builder.terminate_return(function, body, Some(Operand::Register(loaded)), span);
    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("region");
    assert_eq!(region.arity(), 1);
    assert_eq!(region.blocks.len(), 2);
    region.verify().expect("verified region");
}

#[test]
fn object_class_and_dynamic_static_property_enter_native_region_ir() {
    let mut builder = IrBuilder::new(UnitId::new(98));
    let file = builder.add_file("dynamic-property.php");
    let span = IrSpan::new(file, 0, 30);
    let function = builder.start_function("dynamic_property", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let class = builder.intern_constant(IrConstant::String("Widget".into()));
    let class_value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: class_value,
            constant: class,
        },
        span,
    );
    let property_value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchDynamicStaticProperty {
            dst: property_value,
            class_name: Operand::Register(class_value),
            property: "value".to_owned(),
        },
        span,
    );
    let class_name = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchObjectClassName {
            dst: class_name,
            object: Operand::Register(property_value),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(class_name)), span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("dynamic property region");
    assert!(matches!(
        region.blocks[0].instructions[1].kind,
        RegionInstructionKind::FetchDynamicStaticProperty { .. }
    ));
    assert!(matches!(
        region.blocks[0].instructions[2].kind,
        RegionInstructionKind::FetchObjectClassName { .. }
    ));
}

#[test]
fn formerly_missing_instruction_families_enter_native_region_ir() {
    let mut builder = IrBuilder::new(UnitId::new(99));
    let file = builder.add_file("formerly-missing.php");
    let span = IrSpan::new(file, 0, 30);
    let function = builder.start_function("formerly_missing", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let target = builder.intern_local(function, "target");
    let source = builder.intern_local(function, "source");
    let array = builder.intern_constant(IrConstant::Array(Vec::new()));
    let index = builder.intern_constant(IrConstant::Int(0));
    let value = builder.intern_constant(IrConstant::Int(7));
    let object = builder.intern_constant(IrConstant::Null);
    let array_result = builder.alloc_register(function);
    let assign_result = builder.alloc_register(function);
    let isset_result = builder.alloc_register(function);
    let empty_result = builder.alloc_register(function);

    builder.emit(
        function,
        block,
        InstructionKind::ArrayGet {
            dst: array_result,
            array: Operand::Constant(array),
            index: Operand::Constant(index),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::AssignDim {
            dst: assign_result,
            local: target,
            dims: Vec::new(),
            value: Operand::Constant(value),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::IssetDim {
            dst: isset_result,
            local: target,
            dims: Vec::new(),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::EmptyDim {
            dst: empty_result,
            local: target,
            dims: Vec::new(),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::UnsetDim {
            local: target,
            dims: Vec::new(),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::BindReferenceFromPropertyDim {
            target,
            object: Operand::Constant(object),
            property: "value".to_owned(),
            dims: vec![Operand::Constant(index)],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::BindReferenceStaticProperty {
            class_name: "Widget".to_owned(),
            property: "value".to_owned(),
            source,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::RegisterConstant {
            name: "RUNTIME_VALUE".to_owned(),
            value: Operand::Constant(value),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::EmitDiagnostic {
            severity: php_ir::IrDiagnosticSeverity::Deprecation,
            diagnostic_id: "PHP_DEPRECATED_TEST".to_owned(),
            message: "deprecated test".to_owned(),
            leading_newline: false,
        },
        span,
    );
    builder.terminate_return(function, block, None, span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("formerly missing region");
    let instructions = &region.blocks[0].instructions;
    assert!(
        instructions
            .iter()
            .any(|instruction| matches!(instruction.kind, RegionInstructionKind::FetchDim { .. }))
    );
    assert!(instructions.iter().any(|instruction| matches!(
        instruction.kind,
        RegionInstructionKind::AssignLocalResult { .. }
    )));
    assert!(
        instructions.iter().any(|instruction| matches!(
            instruction.kind,
            RegionInstructionKind::IssetLocal { .. }
        ))
    );
    assert!(
        instructions.iter().any(|instruction| matches!(
            instruction.kind,
            RegionInstructionKind::EmptyLocal { .. }
        ))
    );
    assert!(
        instructions.iter().any(|instruction| matches!(
            instruction.kind,
            RegionInstructionKind::UnsetLocal { .. }
        ))
    );
    assert!(instructions.iter().any(|instruction| matches!(
        instruction.kind,
        RegionInstructionKind::BindReferenceFromPropertyDim { .. }
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        &instruction.kind,
        RegionInstructionKind::NativeCall(RegionNativeCall {
            target: RegionCallTarget::Semantic {
                operation: RegionSemanticOp::StaticPropertyReference {
                    bind_source_into_property: true,
                    ..
                },
            },
            ..
        })
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction.kind,
        RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::RegisterConstant { .. })
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction.kind,
        RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::EmitDiagnostic)
    )));
}

#[test]
fn preserves_method_declaration_and_strict_types_metadata() {
    let mut builder = IrBuilder::new(UnitId::new(92));
    let file = builder.add_file("method.php");
    builder.set_strict_types(true);
    builder.set_file_strict_types(file, true);
    let span = IrSpan::new(file, 4, 40);
    let function = builder.start_function(
        "Widget::value",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.set_return_type(function, Some(IrReturnType::Int));
    let this = builder.intern_local(function, "this");
    let entry = builder.append_block(function);
    let block = builder.append_block(function);
    builder.terminate_jump(function, entry, block, span);
    let constant = builder.intern_constant(IrConstant::Int(7));
    let value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: value,
            constant,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(value)), span);
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "widget".to_owned(),
        display_name: "Widget".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "value".to_owned(),
            origin_class: "widget".to_owned(),
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
    let region = BaselineRegionBuilder::build(&unit, function, &CompileMetadata::default())
        .expect("method graph");

    assert!(region.flags.is_method);
    assert!(region.strict_types);
    assert_eq!(region.parameter_locals, vec![this]);
    assert_eq!(region.blocks[0].entry_live_locals, vec![this]);
    assert_eq!(region.blocks[1].entry_live_locals, vec![this]);
    let method = region.declarations.method.expect("method identity");
    assert_eq!(method.class_display_name, "Widget");
    assert_eq!(method.method.function, function);
}

#[test]
fn exact_receiver_links_public_non_final_method() {
    let mut builder = IrBuilder::new(UnitId::new(96));
    let file = builder.add_file("monomorphic-method.php");
    let span = IrSpan::new(file, 0, 40);
    let method = builder.start_function(
        "Widget::value",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let method_block = builder.append_block(method);
    builder.terminate_return(method, method_block, None, span);

    let caller = builder.start_function("main", FunctionFlags::default(), span);
    let caller_block = builder.append_block(caller);
    let object = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::NewObject {
            dst: object,
            display_class_name: "Widget".to_owned(),
            class_name: "widget".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    let result = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallMethod {
            dst: result,
            object: Operand::Register(object),
            method: "value".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(caller, caller_block, None, span);
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "widget".to_owned(),
        display_name: "Widget".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "value".to_owned(),
            origin_class: "widget".to_owned(),
            function: method,
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
    builder.set_entry(caller);
    let unit = builder.finish();
    let region = build_baseline_region(&unit, caller).expect("caller region");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
        .expect("native method call");

    assert!(matches!(
        call.target,
        RegionCallTarget::Function {
            function: Some(function),
            ..
        } if function == method
    ));
    assert_eq!(call.argument_operand_offset, 1);
}

#[test]
fn published_external_parent_prepares_local_object_family() {
    let mut builder = IrBuilder::new(UnitId::new(9_602));
    let file = builder.add_file("published-external-parent.php");
    let span = IrSpan::new(file, 0, 40);
    let caller = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(caller);
    let object = builder.alloc_register(caller);
    builder.emit(
        caller,
        block,
        InstructionKind::NewObject {
            dst: object,
            display_class_name: "LocalChild".to_owned(),
            class_name: "localchild".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    let class_name = builder.alloc_register(caller);
    builder.emit(
        caller,
        block,
        InstructionKind::FetchObjectClassName {
            dst: class_name,
            object: Operand::Register(object),
        },
        span,
    );
    builder.terminate_return(caller, block, Some(Operand::Register(class_name)), span);
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "localchild".to_owned(),
        display_name: "LocalChild".to_owned(),
        parent: Some("externalbase".to_owned()),
        parent_display_name: Some("ExternalBase".to_owned()),
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor: None,
        flags: ClassFlags::default(),
        span,
    });
    builder.set_entry(caller);
    let unit = builder.finish();
    let signature = crate::JitExternalFunctionSignature {
        name: "ExternalBase::__construct".to_owned(),
        link_index: 0,
        published: true,
        params: Vec::new(),
        native_params: Vec::new(),
        native_default_constant_indices: Vec::new(),
        native_arity: 0,
        requires_non_reference_trampoline: false,
        returns_by_reference: false,
        exception_routes: None,
    };
    let metadata = CompileMetadata {
        tier: NativeCompilerTier::Optimizing,
        ..CompileMetadata::default()
    };
    let published = BaselineRegionBuilder::build_with_external_function_signatures(
        &unit,
        caller,
        &metadata,
        std::slice::from_ref(&signature),
    )
    .expect("published external parent region");
    let mut inherited_constructor = signature;
    inherited_constructor.native_arity = 1;
    let constructor = BaselineRegionBuilder::build_with_external_function_signatures(
        &unit,
        caller,
        &metadata,
        &[inherited_constructor],
    )
    .expect("external parent constructor region");
    let default_constructor = BaselineRegionBuilder::build_with_external_function_signatures(
        &unit,
        caller,
        &metadata,
        &[crate::JitExternalFunctionSignature {
            name: "ExternalBase::__construct".to_owned(),
            link_index: 7,
            published: true,
            params: vec![crate::JitExternalParameterSignature {
                name: "count".to_owned(),
                by_ref: false,
                variadic: false,
            }],
            native_params: vec![IrParam {
                name: "count".to_owned(),
                local: LocalId::new(1),
                required: false,
                default: Some(IrConstant::Int(4)),
                type_: Some(php_ir::IrReturnType::Int),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            }],
            native_default_constant_indices: vec![Some(2)],
            native_arity: 2,
            requires_non_reference_trampoline: false,
            returns_by_reference: false,
            exception_routes: None,
        }],
    )
    .expect("external parent default constructor region");
    let unresolved = BaselineRegionBuilder::build(&unit, caller, &metadata)
        .expect("unresolved external parent region");

    assert!(published.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::NewObject {
                prepared: true,
                linked_class: None,
                ..
            }
        )
    }));
    assert!(published.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::FetchObjectClassName {
                prepared_class: Some(0),
                ..
            }
        )
    }));
    assert!(unresolved.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::Discard {
                src: RegionOperand::I64(0),
            }
        )
    }));
    assert!(unresolved.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::NativeCall(RegionNativeCall {
                target: RegionCallTarget::Constructor { .. },
                ..
            })
        )
    }));
    assert!(
        constructor.blocks[0]
            .instructions
            .iter()
            .any(|instruction| {
                matches!(
                    instruction.kind,
                    RegionInstructionKind::NewObject { prepared: true, .. }
                )
            })
    );
    assert!(
        default_constructor.blocks[0]
            .instructions
            .iter()
            .any(|instruction| {
                matches!(
                    &instruction.kind,
                    RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Function { function: None, .. },
                        operands,
                        ..
                    }) if matches!(
                        operands.as_slice(),
                        [
                            Some(RegionOperand::Register(_)),
                            Some(RegionOperand::LinkedConstant {
                                link_index: 7,
                                constant: 2,
                                class: SsaValueClass::Int,
                            }),
                        ]
                    )
                )
            })
    );
}

#[test]
fn property_assignment_borrows_implicit_method_receiver() {
    let mut builder = IrBuilder::new(UnitId::new(4_212));
    let file = builder.add_file("method-property-borrow.php");
    let span = IrSpan::new(file, 0, 40);
    let method = builder.start_function(
        "Widget::__construct",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let this = builder.intern_local(method, "this");
    let argument = builder.intern_local(method, "value");
    builder.push_param(
        method,
        IrParam {
            name: "value".to_owned(),
            local: argument,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(method);
    let receiver = builder.alloc_register(method);
    builder.emit(
        method,
        block,
        InstructionKind::LoadLocal {
            dst: receiver,
            local: this,
        },
        span,
    );
    let value = builder.alloc_register(method);
    builder.emit(
        method,
        block,
        InstructionKind::LoadLocal {
            dst: value,
            local: argument,
        },
        span,
    );
    let result = builder.alloc_register(method);
    builder.emit(
        method,
        block,
        InstructionKind::AssignProperty {
            dst: result,
            object: Operand::Register(receiver),
            property: "value".to_owned(),
            value: Operand::Register(value),
        },
        span,
    );
    builder.emit(
        method,
        block,
        InstructionKind::Discard {
            src: Operand::Register(result),
        },
        span,
    );
    builder.terminate_return(method, block, None, span);
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "widget".to_owned(),
        display_name: "Widget".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "__construct".to_owned(),
            origin_class: "widget".to_owned(),
            function: method,
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
        constructor: Some(method),
        flags: ClassFlags::default(),
        span,
    });
    let unit = builder.finish();
    let region = build_baseline_region(&unit, method).expect("constructor region");
    let flow = analyze_executable_value_flow(&region, &unit.constants);

    assert_eq!(region.parameter_locals, vec![this, argument]);
    assert!(flow.can_borrow_local_load(region.blocks[0].instructions[0].continuation_id));
    assert_eq!(
        flow.register_fact(receiver).ownership,
        SsaOwnership::Borrowed
    );
    assert_eq!(
        flow.local_fact(this),
        SsaValueFact {
            class: SsaValueClass::MixedHandle,
            certainty: SsaCertainty::Unknown,
            ownership: SsaOwnership::Borrowed,
            integer_range: None,
        }
    );
    assert_eq!(flow.register_fact(receiver), flow.local_fact(this));
    flow.verify_ownership(&region)
        .expect("property receiver borrow should verify");

    let baseline = analyze_baseline_value_ownership(&region);
    assert!(baseline.can_borrow_local_load(region.blocks[0].instructions[0].continuation_id));
    assert_eq!(
        baseline.register_fact(receiver).ownership,
        SsaOwnership::Borrowed
    );
    assert_eq!(baseline.local_fact(this).ownership, SsaOwnership::Borrowed);
    assert_eq!(
        baseline.local_fact(argument).ownership,
        SsaOwnership::Borrowed
    );
    assert!(!baseline.releases_local_at_frame_exit(this));
    assert!(!baseline.releases_local_at_frame_exit(argument));
    assert_eq!(baseline.ssa().phi_count(), 0);
    baseline
        .verify_ownership(&region)
        .expect("streaming baseline borrow should verify without SSA");
}

#[test]
fn this_receiver_keeps_virtual_method_dispatch_in_non_final_class() {
    let mut builder = IrBuilder::new(UnitId::new(97));
    let file = builder.add_file("virtual-method.php");
    let span = IrSpan::new(file, 0, 40);
    let item = builder.start_function(
        "Widget::item",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let item_block = builder.append_block(item);
    builder.terminate_return(item, item_block, None, span);

    let run = builder.start_function(
        "Widget::run",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let this = builder.intern_local(run, "this");
    let run_block = builder.append_block(run);
    let receiver = builder.alloc_register(run);
    builder.emit(
        run,
        run_block,
        InstructionKind::LoadLocal {
            dst: receiver,
            local: this,
        },
        span,
    );
    let result = builder.alloc_register(run);
    let call_instruction = builder.emit(
        run,
        run_block,
        InstructionKind::CallMethod {
            dst: result,
            object: Operand::Register(receiver),
            method: "item".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(run, run_block, Some(Operand::Register(result)), span);
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "widget".to_owned(),
        display_name: "Widget".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![
            ClassMethodEntry {
                name: "item".to_owned(),
                origin_class: "widget".to_owned(),
                function: item,
                flags: ClassMethodFlags {
                    has_body: true,
                    ..ClassMethodFlags::default()
                },
                attributes: Vec::new(),
            },
            ClassMethodEntry {
                name: "run".to_owned(),
                origin_class: "widget".to_owned(),
                function: run,
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
        flags: ClassFlags::default(),
        span,
    });
    let unit = builder.finish();
    let region = build_baseline_region(&unit, run).expect("method region");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
        .expect("native method call");

    assert!(matches!(
        call.target,
        RegionCallTarget::Method { ref method, .. } if method == "item"
    ));
    assert_eq!(call.direct_compiled_target(), None);

    let specialized = BaselineRegionBuilder::build_with_runtime_specializations(
        &unit,
        run,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
        &[],
        &[crate::JitMethodSpecialization {
            instruction_id: call_instruction.raw(),
            receiver_layout_id: 0x5a17,
            target: crate::JitMethodSpecializationTarget::Local(item),
        }],
    )
    .expect("profile-specialized method region");
    let call = specialized.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
        .expect("specialized native method call");
    assert!(matches!(
        call.target,
        RegionCallTarget::Method {
            function: Some(target),
            linked_function: None,
            receiver_layout_id: Some(0x5a17),
            ..
        } if target == item
    ));
    assert_eq!(call.direct_compiled_target(), Some(item));
}

#[test]
fn object_syntax_static_method_call_omits_receiver_from_native_abi() {
    let mut builder = IrBuilder::new(UnitId::new(93));
    let file = builder.add_file("static-method.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function(
        "Widget::normalize",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let parameter = builder.intern_local(function, "value");
    builder.push_param(
        function,
        IrParam {
            name: "value".to_owned(),
            local: parameter,
            required: true,
            type_: None,
            by_ref: false,
            variadic: false,
            default: None,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(function);
    builder.terminate_return(function, block, Some(Operand::Local(parameter)), span);
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "widget".to_owned(),
        display_name: "Widget".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "normalize".to_owned(),
            origin_class: "widget".to_owned(),
            function,
            flags: ClassMethodFlags {
                is_static: true,
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
    let value = builder.intern_constant(IrConstant::Int(7));
    let unit = builder.finish();
    let argument = IrCallArg {
        name: None,
        value: Operand::Constant(value),
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let RegionInstructionKind::NativeCall(call) = lower_direct_method_call(
        &unit,
        RegId::new(0),
        function,
        Operand::Constant(value),
        &[argument],
    ) else {
        panic!("static method should use the unified native call model");
    };
    assert_eq!(call.argument_operand_offset, 0);
    assert_eq!(call.direct_arity, Some(1));
    assert_eq!(call.operands.len(), 1);
    assert_eq!(call.direct_compiled_target(), Some(function));
}

#[test]
fn named_user_call_prepares_native_parameter_order() {
    let mut builder = IrBuilder::new(UnitId::new(9_801));
    let file = builder.add_file("named-direct-call.php");
    let span = IrSpan::new(file, 0, 40);
    let function = builder.start_function("named_target", FunctionFlags::default(), span);
    let first = builder.intern_local(function, "first");
    let second = builder.intern_local(function, "second");
    let third = builder.intern_local(function, "third");
    let second_default = IrConstant::Int(20);
    builder.intern_constant(second_default.clone());
    for parameter in [
        IrParam {
            name: "first".to_owned(),
            local: first,
            required: true,
            default: None,
            type_: None,
            by_ref: true,
            variadic: false,
            attributes: Vec::new(),
        },
        IrParam {
            name: "second".to_owned(),
            local: second,
            required: false,
            default: Some(second_default),
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
        IrParam {
            name: "third".to_owned(),
            local: third,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    ] {
        builder.push_param(function, parameter);
    }
    let block = builder.append_block(function);
    builder.terminate_return(function, block, None, span);

    let caller = builder.start_function("named_caller", FunctionFlags::default(), span);
    let first_source = builder.intern_local(caller, "first_source");
    let third_value = builder.intern_constant(IrConstant::Int(30));
    let unit = builder.finish();
    let args = vec![
        IrCallArg {
            name: Some("third".to_owned()),
            value: Operand::Constant(third_value),
            unpack: false,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: None,
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        },
        IrCallArg {
            name: Some("first".to_owned()),
            value: Operand::Local(first_source),
            unpack: false,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: Some(first_source),
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        },
    ];
    let RegionInstructionKind::NativeCall(call) = lower_direct_function_call(
        &unit,
        RegId::new(0),
        "named_target".to_owned(),
        function,
        &args,
    ) else {
        panic!("named target should use the unified native call model");
    };

    assert_eq!(call.direct_arity, Some(3));
    assert_eq!(call.direct_compiled_target(), Some(function));
    assert_eq!(
        call.prepared_argument_sources(&unit.functions[function.index()].params),
        Some(vec![Some(1), None, Some(0)])
    );
    assert_eq!(
        call.operands,
        vec![
            Some(RegionOperand::Local(first_source)),
            Some(RegionOperand::Constant(0)),
            Some(RegionOperand::I64(30)),
        ]
    );
    assert_eq!(
        call.args[1].value_kind,
        IrCallArgValueKind::ByRefLocationPlaceholder
    );
    let plan = call
        .prepared_argument_plan(&unit.functions[function.index()].params)
        .expect("named call has a native argument trace plan");
    assert_eq!(plan.visible_fixed_count, 3);
    assert!(plan.visible_variadic_sources.is_empty());
    assert!(plan.extra_sources.is_empty());
}

#[test]
fn native_argument_trace_plan_preserves_php_visible_shapes() {
    let parameter = |name: &str, local: u32, variadic: bool| IrParam {
        name: name.to_owned(),
        local: LocalId::new(local),
        required: false,
        default: (!variadic).then_some(IrConstant::Null),
        type_: None,
        by_ref: false,
        variadic,
        attributes: Vec::new(),
    };
    let argument = |source: u32, name: Option<&str>| IrCallArg {
        name: name.map(str::to_owned),
        value: Operand::Constant(ConstId::new(source)),
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };

    let fixed = [parameter("first", 0, false), parameter("second", 1, false)];
    let named = [argument(0, Some("second"))];
    let plan = prepared_call_argument_plan(&named, &fixed).expect("fixed named argument plan");
    assert_eq!(plan.parameter_sources, vec![None, Some(0)]);
    assert_eq!(plan.visible_fixed_count, 2);
    assert!(plan.visible_variadic_sources.is_empty());
    assert!(plan.extra_sources.is_empty());
    assert!(
        prepared_call_argument_plan(&[argument(0, Some("SECOND"))], &fixed).is_none(),
        "PHP named parameter binding is case-sensitive"
    );

    let variadic = [
        parameter("first", 0, false),
        parameter("second", 1, false),
        parameter("rest", 2, true),
    ];
    let positional = [argument(0, None), argument(1, None), argument(2, None)];
    let plan =
        prepared_call_argument_plan(&positional, &variadic).expect("positional variadic plan");
    assert_eq!(plan.parameter_sources, vec![Some(0), Some(1), Some(2)]);
    assert_eq!(plan.visible_fixed_count, 2);
    assert_eq!(plan.visible_variadic_sources, vec![2]);
    assert!(plan.extra_sources.is_empty());

    let nonvariadic = [parameter("first", 0, false)];
    let plan =
        prepared_call_argument_plan(&positional, &nonvariadic).expect("surplus argument plan");
    assert_eq!(plan.parameter_sources, vec![Some(0)]);
    assert_eq!(plan.visible_fixed_count, 1);
    assert!(plan.visible_variadic_sources.is_empty());
    assert_eq!(plan.extra_sources, vec![1, 2]);

    let unknown_named = [argument(0, Some("unknown"))];
    assert!(
        prepared_call_argument_plan(&unknown_named, &variadic).is_none(),
        "keyed unknown named variadics keep one baseline continuation"
    );
}

#[test]
fn optimizing_caller_keeps_frame_introspection_target_as_a_direct_callee() {
    let mut builder = IrBuilder::new(UnitId::new(9_802));
    let file = builder.add_file("direct-frame-introspection-target.php");
    let span = IrSpan::new(file, 0, 40);
    let target = builder.start_function("frame_target", FunctionFlags::default(), span);
    builder.register_function_name("frame_target", target);
    let parameter = builder.intern_local(target, "value");
    builder.push_param(
        target,
        IrParam {
            name: "value".to_owned(),
            local: parameter,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let target_block = builder.append_block(target);
    let observed = builder.alloc_register(target);
    builder.emit(
        target,
        target_block,
        InstructionKind::CallFunction {
            dst: observed,
            name: "func_get_args".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(
        target,
        target_block,
        Some(Operand::Register(observed)),
        span,
    );

    let caller = builder.start_function("frame_caller", FunctionFlags::default(), span);
    let caller_block = builder.append_block(caller);
    let supplied = builder.intern_constant(IrConstant::Int(7));
    let result = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "frame_target".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Constant(supplied),
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
    let region = BaselineRegionBuilder::build(
        &unit,
        caller,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing direct frame-introspection caller");
    let call = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call)
                if call.direct_compiled_target() == Some(target) =>
            {
                Some(call)
            }
            _ => None,
        })
        .expect("frame-introspection target remains a direct compiled callee");
    assert_eq!(
        call.prepared_argument_plan(&unit.functions[target.index()].params)
            .map(|plan| (
                plan.visible_fixed_count,
                plan.visible_variadic_sources,
                plan.extra_sources,
            )),
        Some((1, Vec::new(), Vec::new()))
    );
}

#[test]
fn static_syntax_non_static_method_uses_runtime_receiver_binding() {
    let mut builder = IrBuilder::new(UnitId::new(98));
    let file = builder.add_file("non-static-method.php");
    let span = IrSpan::new(file, 0, 20);
    let method = builder.start_function(
        "Widget::render",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let method_block = builder.append_block(method);
    builder.terminate_return(method, method_block, None, span);
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "widget".to_owned(),
        display_name: "Widget".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "render".to_owned(),
            origin_class: "widget".to_owned(),
            function: method,
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
    let caller = builder.start_function(
        "call_render",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let caller_block = builder.append_block(caller);
    let result = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::CallStaticMethod {
            dst: result,
            class_name: "Widget".to_owned(),
            method: "render".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(caller, caller_block, None, span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, caller).expect("caller region");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
        .expect("native call");
    assert!(matches!(
        call.target,
        RegionCallTarget::StaticMethod { ref class_name, ref method }
            if class_name == "Widget" && method == "render"
    ));
    assert_eq!(call.direct_arity, None);
    assert_eq!(call.direct_compiled_target(), None);
}

#[test]
fn static_closure_this_storage_is_not_a_native_argument() {
    let mut builder = IrBuilder::new(UnitId::new(99));
    let file = builder.add_file("static-closure.php");
    let span = IrSpan::new(file, 0, 20);
    let closure = builder.start_function(
        "closure@0",
        FunctionFlags {
            is_closure: true,
            is_static: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.intern_local(closure, "this");
    let block = builder.append_block(closure);
    builder.terminate_return(closure, block, None, span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, closure).expect("static closure region");
    assert!(region.flags.is_static);
    assert!(region.parameter_locals.is_empty());
    assert_eq!(region.arity(), 0);
}

#[test]
fn global_binding_state_reaches_later_native_blocks() {
    let mut builder = IrBuilder::new(UnitId::new(101));
    let file = builder.add_file("global-live-state.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function("global_live_state", FunctionFlags::default(), span);
    let global = builder.intern_local(function, "wpdb");
    let entry = builder.append_block(function);
    let after = builder.append_block(function);
    builder.emit(
        function,
        entry,
        InstructionKind::BindGlobal {
            local: global,
            name: "wpdb".to_owned(),
        },
        span,
    );
    builder.terminate_jump(function, entry, after, span);
    builder.terminate_return(function, after, Some(Operand::Local(global)), span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("global binding region");

    assert_eq!(region.blocks[1].entry_live_locals, vec![global]);
    assert_eq!(region.blocks[1].terminator_live_locals, vec![global]);
}

#[test]
fn fragment_state_keeps_path_dependent_local_separate_from_snapshot_liveness() {
    let mut builder = IrBuilder::new(UnitId::new(102));
    let file = builder.add_file("conditional-fragment-state.php");
    let span = IrSpan::new(file, 0, 20);
    let function =
        builder.start_function("conditional_fragment_state", FunctionFlags::default(), span);
    let local = builder.intern_local(function, "cache_key");
    let entry = builder.append_block(function);
    let initialized = builder.append_block(function);
    let uninitialized = builder.append_block(function);
    let join = builder.append_block(function);
    let condition = builder.intern_constant(IrConstant::Bool(true));
    let value = builder.intern_constant(IrConstant::String("cache-key".to_owned()));
    builder.terminate_jump_if(
        function,
        entry,
        Operand::Constant(condition),
        initialized,
        uninitialized,
        span,
    );
    let register = builder.alloc_register(function);
    builder.emit_load_const(function, initialized, register, value, span);
    builder.emit(
        function,
        initialized,
        InstructionKind::StoreLocal {
            local,
            src: Operand::Register(register),
        },
        span,
    );
    builder.terminate_jump(function, initialized, join, span);
    builder.terminate_jump(function, uninitialized, join, span);
    builder.terminate_return(function, join, Some(Operand::Local(local)), span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("conditional state region");

    assert!(region.blocks[3].entry_live_locals.is_empty());
    assert_eq!(region.blocks[3].entry_state_locals, vec![local]);
}

#[test]
fn every_ir_call_form_enters_the_unified_native_call_model() {
    let mut builder = IrBuilder::new(UnitId::new(95));
    let file = builder.add_file("calls.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function("calls", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let constant = builder.intern_constant(IrConstant::Int(1));
    let value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: value,
            constant,
        },
        span,
    );
    let argument = IrCallArg {
        name: None,
        value: Operand::Register(value),
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let local = builder.intern_local(function, "reference");
    let calls = [
        InstructionKind::CallFunction {
            dst: builder.alloc_register(function),
            name: "f".to_owned(),
            args: vec![argument.clone()],
        },
        InstructionKind::CallMethod {
            dst: builder.alloc_register(function),
            object: Operand::Register(value),
            method: "m".to_owned(),
            args: vec![argument.clone()],
        },
        InstructionKind::CallStaticMethod {
            dst: builder.alloc_register(function),
            class_name: "c".to_owned(),
            method: "m".to_owned(),
            args: vec![argument.clone()],
        },
        InstructionKind::CallClosure {
            dst: builder.alloc_register(function),
            callee: Operand::Register(value),
            args: vec![argument.clone()],
        },
        InstructionKind::CallCallable {
            dst: builder.alloc_register(function),
            callee: Operand::Register(value),
            args: vec![argument.clone()],
        },
        InstructionKind::Pipe {
            dst: builder.alloc_register(function),
            input: Operand::Register(value),
            callable: Operand::Register(value),
        },
        InstructionKind::BindReferenceFromCall {
            target: local,
            name: "by_ref".to_owned(),
            args: vec![argument.clone()],
        },
        InstructionKind::BindReferenceFromMethodCall {
            target: local,
            object: Operand::Register(value),
            method: "byRef".to_owned(),
            args: vec![argument.clone()],
        },
        InstructionKind::NewObject {
            dst: builder.alloc_register(function),
            display_class_name: "C".to_owned(),
            class_name: "c".to_owned(),
            args: vec![argument.clone()],
        },
        InstructionKind::DynamicNewObject {
            dst: builder.alloc_register(function),
            class_name: Operand::Register(value),
            args: vec![argument],
        },
    ];
    for call in calls {
        builder.emit(function, block, call, span);
    }
    builder.terminate_return(function, block, Some(Operand::Register(value)), span);
    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("call graph");
    let native_calls = region.blocks[0]
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.kind, RegionInstructionKind::NativeCall(_)))
        .collect::<Vec<_>>();
    assert_eq!(native_calls.len(), 10);
    let offsets = native_calls
        .iter()
        .map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => call.argument_operand_offset,
            _ => unreachable!("filtered to native calls"),
        })
        .collect::<Vec<_>>();
    assert_eq!(offsets, vec![0, 1, 0, 1, 1, 1, 0, 1, 0, 1]);
    let RegionInstructionKind::NativeCall(dynamic_constructor) = &native_calls[9].kind else {
        unreachable!("filtered to native calls");
    };
    assert_eq!(dynamic_constructor.operands.len(), 2);
}

#[test]
fn exception_instructions_enter_the_native_control_model() {
    let mut builder = IrBuilder::new(UnitId::new(96));
    let file = builder.add_file("exceptions.php");
    let span = IrSpan::new(file, 0, 30);
    let function = builder.start_function("exceptions", FunctionFlags::default(), span);
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
    let message = builder.intern_constant(IrConstant::Int(17));
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
    builder.emit(function, entry, InstructionKind::LeaveTry, span);
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
    let region = build_baseline_region(&unit, function).expect("exception region");
    let controls = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter(|instruction| matches!(instruction.kind, RegionInstructionKind::NativeControl(_)))
        .count();
    assert_eq!(controls, 5);
    assert_eq!(region.exception_regions.len(), 1);
}

#[test]
fn returns_unwind_through_nested_finally_regions_innermost_first() {
    let mut builder = IrBuilder::new(UnitId::new(97));
    let file = builder.add_file("nested-finally.php");
    let span = IrSpan::new(file, 0, 30);
    let function = builder.start_function("nested", FunctionFlags::default(), span);
    let blocks = (0..7)
        .map(|_| builder.append_block(function))
        .collect::<Vec<_>>();
    builder.emit(
        function,
        blocks[0],
        InstructionKind::EnterTry {
            catch: None,
            catch_types: Vec::new(),
            finally: Some(blocks[3]),
            after: blocks[1],
            exception_local: None,
        },
        span,
    );
    builder.terminate_jump(function, blocks[0], blocks[2], span);
    builder.terminate_return(function, blocks[1], None, span);
    builder.emit(
        function,
        blocks[2],
        InstructionKind::EnterTry {
            catch: None,
            catch_types: Vec::new(),
            finally: Some(blocks[6]),
            after: blocks[4],
            exception_local: None,
        },
        span,
    );
    builder.terminate_jump(function, blocks[2], blocks[5], span);
    builder.emit(
        function,
        blocks[3],
        InstructionKind::EndFinally { after: blocks[1] },
        span,
    );
    builder.terminate_jump(function, blocks[3], blocks[1], span);
    builder.emit(function, blocks[4], InstructionKind::LeaveTry, span);
    builder.terminate_jump(function, blocks[4], blocks[3], span);
    let value = builder.intern_constant(IrConstant::String("inner".to_owned()));
    builder.terminate_return(function, blocks[5], Some(Operand::Constant(value)), span);
    builder.emit(
        function,
        blocks[6],
        InstructionKind::EndFinally { after: blocks[4] },
        span,
    );
    builder.terminate_jump(function, blocks[6], blocks[4], span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("nested finally region");
    assert_eq!(region.exception_regions.len(), 2);
    assert_eq!(region.exception_regions[0].finally, Some(blocks[3]));
    assert!(
        region.exception_regions[0]
            .protected_blocks
            .contains(&blocks[2])
    );
    assert_eq!(region.exception_regions[1].finally, Some(blocks[6]));
    let RegionTerminator::Return { finally, .. } = region.blocks[5].terminator else {
        panic!("expected return terminator");
    };
    assert_eq!(finally, Some(blocks[6]));
    let outer_finally = region.blocks[6]
        .instructions
        .iter()
        .find_map(|instruction| match instruction.kind {
            RegionInstructionKind::NativeControl(RegionNativeControl::EndFinally {
                outer_finally,
                ..
            }) => Some(outer_finally),
            _ => None,
        })
        .expect("end finally control");
    assert_eq!(outer_finally, Some(blocks[3]));
}

#[test]
fn closure_and_constant_fetch_remain_in_the_semantic_graph() {
    let mut builder = IrBuilder::new(UnitId::new(93));
    let file = builder.add_file("closure.php");
    let span = IrSpan::new(file, 10, 20);
    let function = builder.start_function(
        "{closure}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let captured = builder.intern_local(function, "captured");
    builder.push_capture(
        function,
        IrCapture {
            name: "captured".to_owned(),
            local: captured,
            by_ref: true,
        },
    );
    builder.set_return_type(function, Some(IrReturnType::Int));
    let block = builder.append_block(function);
    let dst = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::FetchConst {
            dst,
            name: "DYNAMIC".to_owned(),
            fallback: None,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(dst)), span);
    let unit = builder.finish();
    let region = BaselineRegionBuilder::build(&unit, function, &CompileMetadata::default())
        .expect("closure graph");

    assert!(region.flags.is_closure);
    assert_eq!(region.captures[0].name, "captured");
    let instruction = &region.blocks[0].instructions[0];
    assert!(matches!(
        instruction.kind,
        RegionInstructionKind::FetchConst { dst: candidate } if candidate == dst
    ));
    assert!(matches!(
        instruction.source_kind,
        InstructionKind::FetchConst { .. }
    ));
    assert_eq!(instruction.span, span);
}

#[test]
fn direct_closure_call_reads_the_authoritative_prepared_capture() {
    let mut builder = IrBuilder::new(UnitId::new(94));
    let file = builder.add_file("closure-capture-snapshot.php");
    let span = IrSpan::new(file, 0, 20);
    let closure = builder.start_function(
        "{closure}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let captured = builder.intern_local(closure, "x");
    builder.push_capture(
        closure,
        IrCapture {
            name: "x".to_owned(),
            local: captured,
            by_ref: false,
        },
    );
    let parameter = builder.intern_local(closure, "y");
    builder.push_param(
        closure,
        IrParam {
            name: "y".to_owned(),
            local: parameter,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let closure_block = builder.append_block(closure);
    builder.terminate_return(closure, closure_block, Some(Operand::Local(captured)), span);

    let main = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let source = builder.intern_local(main, "x");
    let callable = builder.intern_local(main, "f");
    let block = builder.append_block(main);
    let two = builder.intern_constant(IrConstant::Int(2));
    let hundred = builder.intern_constant(IrConstant::Int(100));
    let three = builder.intern_constant(IrConstant::Int(3));
    builder.emit(
        main,
        block,
        InstructionKind::StoreLocal {
            local: source,
            src: Operand::Constant(two),
        },
        span,
    );
    let closure_value = builder.alloc_register(main);
    builder.emit(
        main,
        block,
        InstructionKind::MakeClosure {
            dst: closure_value,
            function: closure,
            captures: vec![ClosureCaptureArg {
                name: "x".to_owned(),
                src: Operand::Local(source),
                by_ref: false,
            }],
        },
        span,
    );
    builder.emit(
        main,
        block,
        InstructionKind::StoreLocal {
            local: callable,
            src: Operand::Register(closure_value),
        },
        span,
    );
    builder.emit(
        main,
        block,
        InstructionKind::StoreLocal {
            local: source,
            src: Operand::Constant(hundred),
        },
        span,
    );
    let loaded_callable = builder.alloc_register(main);
    builder.emit(
        main,
        block,
        InstructionKind::LoadLocal {
            dst: loaded_callable,
            local: callable,
        },
        span,
    );
    let result = builder.alloc_register(main);
    builder.emit(
        main,
        block,
        InstructionKind::CallCallable {
            dst: result,
            callee: Operand::Register(loaded_callable),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Constant(three),
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
    builder.terminate_return(main, block, Some(Operand::Register(result)), span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, main).expect("direct closure region");
    let call_instruction = region.blocks[0]
        .instructions
        .iter()
        .find(|instruction| {
            matches!(
                &instruction.kind,
                RegionInstructionKind::NativeCall(call)
                    if matches!(
                        call.target,
                        RegionCallTarget::Closure {
                            function: Some(candidate),
                            capture_count: 1,
                            ..
                        } if candidate == closure
                    )
            )
        })
        .expect("direct closure call");
    let RegionInstructionKind::NativeCall(call) = &call_instruction.kind else {
        unreachable!("filtered above");
    };
    assert_eq!(
        region.local_count,
        unit.functions[main.index()].local_count,
        "direct closure calls must not retain a second mutable capture plane"
    );
    assert_eq!(call.argument_operand_offset, 1);
    assert_eq!(call.operands[0], Some(RegionOperand::I64(0)));
    assert_eq!(call.operands[1], Some(RegionOperand::I64(3)));
    assert!(
        call_instruction.register_uses().contains(&loaded_callable),
        "the prepared closure source must remain live even though it is not packed into the callee frame"
    );
}

#[test]
fn named_closure_call_keeps_one_callable_baseline_boundary() {
    let mut builder = IrBuilder::new(UnitId::new(103));
    let file = builder.add_file("named-closure-call.php");
    let span = IrSpan::new(file, 0, 20);
    let closure = builder.start_function(
        "{closure}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    for (name, required, variadic) in [
        ("first", true, false),
        ("second", true, false),
        ("rest", false, true),
    ] {
        let local = builder.intern_local(closure, name);
        builder.push_param(
            closure,
            IrParam {
                name: name.to_owned(),
                local,
                required,
                default: None,
                type_: None,
                by_ref: false,
                variadic,
                attributes: Vec::new(),
            },
        );
    }
    let closure_block = builder.append_block(closure);
    builder.terminate_return(closure, closure_block, None, span);

    let main = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(main);
    let callable = builder.alloc_register(main);
    builder.emit(
        main,
        block,
        InstructionKind::MakeClosure {
            dst: callable,
            function: closure,
            captures: Vec::new(),
        },
        span,
    );
    let one = builder.intern_constant(IrConstant::Int(1));
    let two = builder.intern_constant(IrConstant::Int(2));
    let three = builder.intern_constant(IrConstant::Int(3));
    let result = builder.alloc_register(main);
    let named_argument = |name: &str, value| IrCallArg {
        name: Some(name.to_owned()),
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    builder.emit(
        main,
        block,
        InstructionKind::CallCallable {
            dst: result,
            callee: Operand::Register(callable),
            args: vec![
                named_argument("second", Operand::Constant(two)),
                named_argument("first", Operand::Constant(one)),
                named_argument("third", Operand::Constant(three)),
            ],
        },
        span,
    );
    builder.terminate_return(main, block, Some(Operand::Register(result)), span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, main).expect("named closure region");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call)
                if matches!(
                    call.target,
                    RegionCallTarget::Closure { function: None, .. }
                ) =>
            {
                Some(call)
            }
            _ => None,
        })
        .expect("named closure baseline boundary");
    assert_eq!(call.argument_operand_offset, 1);
    assert_eq!(
        call.operands.first(),
        Some(&Some(RegionOperand::Register(callable)))
    );
}

#[test]
fn optimizing_call_user_func_array_uses_prepared_closure_and_keeps_baseline_original() {
    let mut builder = IrBuilder::new(UnitId::new(102));
    let file = builder.add_file("closure-call-user-func-array.php");
    let span = IrSpan::new(file, 0, 20);
    let closure = builder.start_function(
        "{closure}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let captured = builder.intern_local(closure, "offset");
    builder.push_capture(
        closure,
        IrCapture {
            name: "offset".to_owned(),
            local: captured,
            by_ref: false,
        },
    );
    let value = builder.intern_local(closure, "value");
    builder.push_param(
        closure,
        IrParam {
            name: "value".to_owned(),
            local: value,
            required: true,
            default: None,
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let closure_block = builder.append_block(closure);
    builder.terminate_return(closure, closure_block, Some(Operand::Local(value)), span);

    let main = builder.start_function(
        "closure_call_user_func_array",
        FunctionFlags::default(),
        span,
    );
    let values = builder.intern_local(main, "values");
    builder.push_param(
        main,
        IrParam {
            name: "values".to_owned(),
            local: values,
            required: true,
            default: None,
            type_: Some(IrReturnType::Array),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let offset = builder.intern_local(main, "offset");
    let five = builder.intern_constant(IrConstant::Int(5));
    let block = builder.append_block(main);
    builder.emit(
        main,
        block,
        InstructionKind::StoreLocal {
            local: offset,
            src: Operand::Constant(five),
        },
        span,
    );
    let callable = builder.alloc_register(main);
    builder.emit(
        main,
        block,
        InstructionKind::MakeClosure {
            dst: callable,
            function: closure,
            captures: vec![ClosureCaptureArg {
                name: "offset".to_owned(),
                src: Operand::Local(offset),
                by_ref: false,
            }],
        },
        span,
    );
    let result = builder.alloc_register(main);
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    builder.emit(
        main,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "call_user_func_array".to_owned(),
            args: vec![
                argument(Operand::Register(callable)),
                argument(Operand::Local(values)),
            ],
        },
        span,
    );
    builder.terminate_return(main, block, Some(Operand::Register(result)), span);

    let unit = builder.finish();
    let optimizing = BaselineRegionBuilder::build(
        &unit,
        main,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing closure call_user_func_array region");
    let direct = optimizing.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call)
                if matches!(
                    call.target,
                    RegionCallTarget::Closure {
                        function: Some(candidate),
                        capture_count: 1,
                        ..
                    } if candidate == closure
                ) =>
            {
                Some(call)
            }
            _ => None,
        })
        .expect("prepared closure unpack call");
    assert_eq!(direct.argument_operand_offset, 1);
    assert_eq!(direct.trailing_unpack_argument(), Some(0));
    assert_eq!(direct.direct_compiled_unpack_target(), Some(closure));

    let baseline = BaselineRegionBuilder::build(&unit, main, &CompileMetadata::default())
        .expect("baseline closure call_user_func_array region");
    assert!(baseline.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            &instruction.kind,
            RegionInstructionKind::NativeCall(RegionNativeCall {
                target: RegionCallTarget::Function { name, function: None },
                ..
            }) if name.eq_ignore_ascii_case("call_user_func_array")
        )
    }));
}

#[test]
fn optimizing_runtime_callable_array_preserves_one_native_unpack_boundary() {
    let mut builder = IrBuilder::new(UnitId::new(103));
    let file = builder.add_file("runtime-callable-array.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function("runtime_callable_array", FunctionFlags::default(), span);
    let callable = builder.intern_local(function, "callback");
    builder.push_param(
        function,
        IrParam {
            name: "callback".to_owned(),
            local: callable,
            required: true,
            default: None,
            type_: Some(IrReturnType::Callable),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let values = builder.intern_local(function, "values");
    builder.push_param(
        function,
        IrParam {
            name: "values".to_owned(),
            local: values,
            required: true,
            default: None,
            type_: Some(IrReturnType::Array),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(function);
    let result = builder.alloc_register(function);
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "call_user_func_array".to_owned(),
            args: vec![
                argument(Operand::Local(callable)),
                argument(Operand::Local(values)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();

    let optimizing = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing runtime callable array region");
    let unpack = optimizing.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call)
                if matches!(call.target, RegionCallTarget::Callable { .. }) =>
            {
                Some(call)
            }
            _ => None,
        })
        .expect("runtime callable must retain a native callable target");
    assert_eq!(unpack.argument_operand_offset, 1);
    assert_eq!(unpack.operands.len(), 2);
    assert_eq!(unpack.trailing_unpack_argument(), Some(0));

    let baseline = BaselineRegionBuilder::build(&unit, function, &CompileMetadata::default())
        .expect("baseline runtime callable array region");
    assert!(baseline.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            &instruction.kind,
            RegionInstructionKind::NativeCall(RegionNativeCall {
                target: RegionCallTarget::Function { name, function: None },
                ..
            }) if name.eq_ignore_ascii_case("call_user_func_array")
        )
    }));
}

#[test]
fn optimizing_array_map_preserves_runtime_callable_for_one_native_loop() {
    let mut builder = IrBuilder::new(UnitId::new(103));
    let file = builder.add_file("runtime-array-callback.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function("runtime_array_callback", FunctionFlags::default(), span);
    let callable = builder.intern_local(function, "callback");
    builder.push_param(
        function,
        IrParam {
            name: "callback".to_owned(),
            local: callable,
            required: true,
            default: None,
            type_: Some(IrReturnType::Callable),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let values = builder.intern_local(function, "values");
    builder.push_param(
        function,
        IrParam {
            name: "values".to_owned(),
            local: values,
            required: true,
            default: None,
            type_: Some(IrReturnType::Array),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(function);
    let result = builder.alloc_register(function);
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "array_map".to_owned(),
            args: vec![
                argument(Operand::Local(callable)),
                argument(Operand::Local(values)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();

    let optimizing = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing runtime array callback region");
    let callback = optimizing.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::ArrayCallback(call) => Some(call),
            _ => None,
        })
        .expect("runtime callback must use the native array loop");
    assert_eq!(
        callback.callback,
        RegionArrayCallbackTarget::Runtime(RegionOperand::Local(callable))
    );
    assert_eq!(callback.operation, RegionArrayCallbackOperation::Map);
    assert!(
        !optimizing.blocks[0].instructions.iter().any(|instruction| {
            matches!(
                &instruction.kind,
                RegionInstructionKind::NativeCall(RegionNativeCall {
                    target: RegionCallTarget::Function { name, .. },
                    ..
                }) if name.eq_ignore_ascii_case("array_map")
            )
        })
    );
}

#[test]
fn optimizing_preg_replace_callback_uses_native_match_plan_for_string_callback() {
    let mut builder = IrBuilder::new(UnitId::new(109));
    let file = builder.add_file("preg-replace-callback-native.php");
    let span = IrSpan::new(file, 0, 20);

    let callback = builder.start_function("replace_match", FunctionFlags::default(), span);
    builder.register_function_name("replace_match", callback);
    builder.set_return_type(callback, Some(IrReturnType::String));
    let matches = builder.intern_local(callback, "matches");
    builder.push_param(
        callback,
        IrParam {
            name: "matches".to_owned(),
            local: matches,
            required: true,
            default: None,
            type_: Some(IrReturnType::Array),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let replacement = builder.intern_constant(IrConstant::String("x".to_owned()));
    let callback_block = builder.append_block(callback);
    builder.terminate_return(
        callback,
        callback_block,
        Some(Operand::Constant(replacement)),
        span,
    );

    let function = builder.start_function("replace_subject", FunctionFlags::default(), span);
    let pattern = builder.intern_local(function, "pattern");
    let subject = builder.intern_local(function, "subject");
    for (name, local) in [("pattern", pattern), ("subject", subject)] {
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
    }
    let callback_name = builder.intern_constant(IrConstant::String("replace_match".to_owned()));
    let block = builder.append_block(function);
    let result = builder.alloc_register(function);
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "preg_replace_callback".to_owned(),
            args: vec![
                argument(Operand::Local(pattern)),
                argument(Operand::Constant(callback_name)),
                argument(Operand::Local(subject)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();

    let region = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing preg_replace_callback region");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::ArrayCallback(call)
                if call.operation == RegionArrayCallbackOperation::PregReplace =>
            {
                Some(call)
            }
            _ => None,
        })
        .expect("native PCRE callback plan");
    assert_eq!(
        call.arrays,
        vec![
            RegionOperand::Local(pattern),
            RegionOperand::Local(subject),
            RegionOperand::I64(-1),
            RegionOperand::I64(0),
        ]
    );
    let callback_plan = call.callback.stable().expect("stable callback");
    assert_eq!(callback_plan.function, Some(callback));
    assert!(callback_plan.returns_string);
    assert_eq!(region.direct_callees(), vec![callback]);
}

#[test]
fn optimizing_preg_replace_callback_preserves_one_runtime_callable_boundary() {
    let mut builder = IrBuilder::new(UnitId::new(110));
    let file = builder.add_file("preg-replace-runtime-callback-native.php");
    let span = IrSpan::new(file, 0, 20);
    let function =
        builder.start_function("replace_runtime_subject", FunctionFlags::default(), span);
    let callback = builder.intern_local(function, "callback");
    let pattern = builder.intern_local(function, "pattern");
    let subject = builder.intern_local(function, "subject");
    for (name, local, type_) in [
        ("callback", callback, IrReturnType::Callable),
        ("pattern", pattern, IrReturnType::String),
        ("subject", subject, IrReturnType::String),
    ] {
        builder.push_param(
            function,
            IrParam {
                name: name.to_owned(),
                local,
                required: true,
                default: None,
                type_: Some(type_),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
    }
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let block = builder.append_block(function);
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "preg_replace_callback".to_owned(),
            args: vec![
                argument(Operand::Local(pattern)),
                argument(Operand::Local(callback)),
                argument(Operand::Local(subject)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();

    let region = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing runtime preg_replace_callback region");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::ArrayCallback(call)
                if call.operation == RegionArrayCallbackOperation::PregReplace =>
            {
                Some(call)
            }
            _ => None,
        })
        .expect("runtime native PCRE callback plan");
    assert_eq!(
        call.callback,
        RegionArrayCallbackTarget::Runtime(RegionOperand::Local(callback))
    );
    assert_eq!(
        call.arrays,
        vec![
            RegionOperand::Local(pattern),
            RegionOperand::Local(subject),
            RegionOperand::I64(-1),
            RegionOperand::I64(0),
        ]
    );
}

#[test]
fn optimizing_preg_replace_callback_array_preserves_ordered_native_entries() {
    let mut builder = IrBuilder::new(UnitId::new(111));
    let file = builder.add_file("preg-replace-callback-array-native.php");
    let span = IrSpan::new(file, 0, 20);

    let callback = builder.start_function("replace_array_match", FunctionFlags::default(), span);
    let callback_matches = builder.intern_local(callback, "matches");
    builder.push_param(
        callback,
        IrParam {
            name: "matches".to_owned(),
            local: callback_matches,
            required: true,
            default: None,
            type_: Some(IrReturnType::Array),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    builder.set_return_type(callback, Some(IrReturnType::String));
    let replacement = builder.intern_constant(IrConstant::String("X".to_owned()));
    let callback_block = builder.append_block(callback);
    builder.terminate_return(
        callback,
        callback_block,
        Some(Operand::Constant(replacement)),
        span,
    );
    builder.register_function_name("replace_array_match", callback);

    let function = builder.start_function("replace_array_subject", FunctionFlags::default(), span);
    let subject = builder.intern_local(function, "subject");
    let count = builder.intern_local(function, "count");
    builder.push_param(
        function,
        IrParam {
            name: "subject".to_owned(),
            local: subject,
            required: true,
            default: None,
            type_: Some(IrReturnType::String),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let pattern = builder.intern_constant(IrConstant::String("/a+/".to_owned()));
    let callback_name =
        builder.intern_constant(IrConstant::String("replace_array_match".to_owned()));
    let limit = builder.intern_constant(IrConstant::Int(2));
    let missing = builder.intern_constant(IrConstant::Null);
    let block = builder.append_block(function);
    let callback_map = builder.alloc_register(function);
    let pattern_register = builder.alloc_register(function);
    let callback_register = builder.alloc_register(function);
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: callback_map },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: pattern_register,
            constant: pattern,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: callback_register,
            constant: callback_name,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array: callback_map,
            key: Some(Operand::Register(pattern_register)),
            value: Operand::Register(callback_register),
            by_ref_local: None,
        },
        span,
    );
    for register in [pattern_register, callback_register] {
        builder.emit(
            function,
            block,
            InstructionKind::Discard {
                src: Operand::Register(register),
            },
            span,
        );
    }
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let mut count_argument = argument(Operand::Constant(missing));
    count_argument.by_ref_local = Some(count);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "preg_replace_callback_array".to_owned(),
            args: vec![
                argument(Operand::Register(callback_map)),
                argument(Operand::Local(subject)),
                argument(Operand::Constant(limit)),
                count_argument,
            ],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(callback_map),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();

    let region = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing preg_replace_callback_array region");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::PregCallbackArray(call) => Some(call),
            _ => None,
        })
        .expect("ordered native PCRE callback map");
    assert_eq!(call.entries.len(), 1);
    assert_eq!(
        call.entries[0].pattern,
        RegionOperand::Register(pattern_register)
    );
    assert_eq!(call.subject, RegionOperand::Local(subject));
    assert_eq!(call.count_local, Some(count));
    assert_eq!(
        call.entries[0]
            .callback
            .stable()
            .and_then(|callback| callback.function),
        Some(callback)
    );
    assert!(region.blocks[0].instructions.iter().all(|instruction| {
        !matches!(
            instruction.kind,
            RegionInstructionKind::NewArray { dst, .. }
                | RegionInstructionKind::ArrayInsert { array: dst, .. }
                if dst == callback_map
        )
    }));
    assert_eq!(region.direct_callees(), vec![callback]);
}

#[test]
fn optimizing_preg_replace_callback_array_reads_runtime_callable_from_map() {
    let mut builder = IrBuilder::new(UnitId::new(112));
    let file = builder.add_file("preg-replace-runtime-callback-array-native.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function(
        "replace_runtime_array_subject",
        FunctionFlags::default(),
        span,
    );
    let callback = builder.intern_local(function, "callback");
    let subject = builder.intern_local(function, "subject");
    for (name, local, type_) in [
        ("callback", callback, IrReturnType::Callable),
        ("subject", subject, IrReturnType::String),
    ] {
        builder.push_param(
            function,
            IrParam {
                name: name.to_owned(),
                local,
                required: true,
                default: None,
                type_: Some(type_),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            },
        );
    }
    let pattern = builder.intern_constant(IrConstant::String("/a+/".to_owned()));
    let block = builder.append_block(function);
    let callback_map = builder.alloc_register(function);
    let pattern_register = builder.alloc_register(function);
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: callback_map },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: pattern_register,
            constant: pattern,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array: callback_map,
            key: Some(Operand::Register(pattern_register)),
            value: Operand::Local(callback),
            by_ref_local: None,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(pattern_register),
        },
        span,
    );
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "preg_replace_callback_array".to_owned(),
            args: vec![
                argument(Operand::Register(callback_map)),
                argument(Operand::Local(subject)),
            ],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(callback_map),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    let unit = builder.finish();

    let region = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing runtime preg_replace_callback_array region");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::PregCallbackArray(call) => Some(call),
            _ => None,
        })
        .expect("runtime native PCRE callback map");
    assert_eq!(call.entries.len(), 1);
    assert_eq!(
        call.entries[0].callback,
        RegionArrayCallbackTarget::Runtime(RegionOperand::Register(callback_map))
    );
    assert!(region.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::NewArray { dst, .. } if dst == callback_map
        )
    }));
    assert!(region.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::ArrayInsert { array, .. } if array == callback_map
        )
    }));
}

#[test]
fn known_closure_bind_preserves_the_runtime_closure_value() {
    let mut builder = IrBuilder::new(UnitId::new(96));
    let file = builder.add_file("closure-bind.php");
    let span = IrSpan::new(file, 0, 20);
    let closure = builder.start_function(
        "{closure}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let closure_block = builder.append_block(closure);
    builder.terminate_return(closure, closure_block, None, span);

    let function = builder.start_function(
        "closure_bind",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let block = builder.append_block(function);
    let closure_value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::MakeClosure {
            dst: closure_value,
            function: closure,
            captures: Vec::new(),
        },
        span,
    );
    let null = builder.intern_constant(IrConstant::Null);
    let bound = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallStaticMethod {
            dst: bound,
            class_name: "Closure".to_owned(),
            method: "bind".to_owned(),
            args: vec![
                IrCallArg {
                    name: None,
                    value: Operand::Register(closure_value),
                    unpack: false,
                    value_kind: IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                },
                IrCallArg {
                    name: None,
                    value: Operand::Constant(null),
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
    builder.terminate_return(function, block, Some(Operand::Register(bound)), span);

    let unit = builder.finish();
    let region = build_baseline_region(&unit, function).expect("closure bind region");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find(|instruction| {
            matches!(
                instruction.source_kind,
                InstructionKind::CallStaticMethod { ref class_name, ref method, .. }
                    if class_name == "Closure" && method == "bind"
            )
        })
        .expect("Closure::bind instruction");
    assert!(matches!(
        &call.kind,
        RegionInstructionKind::NativeCall(RegionNativeCall {
            result: RegionCallResult::Register(candidate),
            target: RegionCallTarget::StaticMethod { class_name, method },
            ..
        }) if *candidate == bound && class_name == "Closure" && method == "bind"
    ));
}

#[test]
fn optimizing_array_callback_carries_exact_prepared_closure_plan() {
    let mut builder = IrBuilder::new(UnitId::new(97));
    let file = builder.add_file("array-callback-closure.php");
    let span = IrSpan::new(file, 0, 20);
    let closure = builder.start_function(
        "{closure}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let captured = builder.intern_local(closure, "offset");
    builder.push_capture(
        closure,
        IrCapture {
            name: "offset".to_owned(),
            local: captured,
            by_ref: false,
        },
    );
    let value = builder.intern_local(closure, "value");
    builder.push_param(
        closure,
        IrParam {
            name: "value".to_owned(),
            local: value,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let closure_block = builder.append_block(closure);
    builder.terminate_return(closure, closure_block, Some(Operand::Local(value)), span);

    let function = builder.start_function("map_closure", FunctionFlags::default(), span);
    let array = builder.intern_local(function, "array");
    builder.push_param(
        function,
        IrParam {
            name: "array".to_owned(),
            local: array,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let offset = builder.intern_local(function, "offset");
    let block = builder.append_block(function);
    let one = builder.intern_constant(IrConstant::Int(1));
    builder.emit(
        function,
        block,
        InstructionKind::StoreLocal {
            local: offset,
            src: Operand::Constant(one),
        },
        span,
    );
    let callable = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::MakeClosure {
            dst: callable,
            function: closure,
            captures: vec![ClosureCaptureArg {
                name: "offset".to_owned(),
                src: Operand::Local(offset),
                by_ref: false,
            }],
        },
        span,
    );
    let mapped = builder.alloc_register(function);
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: mapped,
            name: "array_map".to_owned(),
            args: vec![
                argument(Operand::Register(callable)),
                argument(Operand::Local(array)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(mapped)), span);
    let unit = builder.finish();
    let region = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing closure callback region");
    let instruction = region.blocks[0]
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.kind, RegionInstructionKind::ArrayCallback(_)))
        .expect("native array callback");
    let RegionInstructionKind::ArrayCallback(call) = &instruction.kind else {
        unreachable!("filtered above");
    };
    let callback_plan = call.callback.stable().expect("stable closure callback");
    assert_eq!(callback_plan.function, Some(closure));
    assert_eq!(
        callback_plan.closure,
        Some(RegionOperand::Register(callable))
    );
    assert_eq!(callback_plan.bound_object_count, 0);
    assert_eq!(callback_plan.capture_count, 1);
    assert!(instruction.register_uses().contains(&callable));
    assert_eq!(region.direct_callees(), vec![closure]);
}

#[test]
fn optimizing_array_callback_uses_closure_returned_by_native_factory() {
    let mut builder = IrBuilder::new(UnitId::new(98));
    let file = builder.add_file("array-callback-closure-factory.php");
    let span = IrSpan::new(file, 0, 20);
    let closure = builder.start_function(
        "{closure}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let captured = builder.intern_local(closure, "offset");
    builder.push_capture(
        closure,
        IrCapture {
            name: "offset".to_owned(),
            local: captured,
            by_ref: false,
        },
    );
    let value = builder.intern_local(closure, "value");
    builder.push_param(
        closure,
        IrParam {
            name: "value".to_owned(),
            local: value,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let closure_block = builder.append_block(closure);
    builder.terminate_return(closure, closure_block, Some(Operand::Local(value)), span);

    let factory = builder.start_function("adder_factory", FunctionFlags::default(), span);
    builder.register_function_name("adder_factory", factory);
    let offset = builder.intern_local(factory, "offset");
    builder.push_param(
        factory,
        IrParam {
            name: "offset".to_owned(),
            local: offset,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let factory_block = builder.append_block(factory);
    let closure_value = builder.alloc_register(factory);
    builder.emit(
        factory,
        factory_block,
        InstructionKind::MakeClosure {
            dst: closure_value,
            function: closure,
            captures: vec![ClosureCaptureArg {
                name: "offset".to_owned(),
                src: Operand::Local(offset),
                by_ref: false,
            }],
        },
        span,
    );
    builder.terminate_return(
        factory,
        factory_block,
        Some(Operand::Register(closure_value)),
        span,
    );

    let function = builder.start_function("map_factory_closure", FunctionFlags::default(), span);
    let array = builder.intern_local(function, "array");
    builder.push_param(
        function,
        IrParam {
            name: "array".to_owned(),
            local: array,
            required: true,
            default: None,
            type_: None,
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let block = builder.append_block(function);
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let six = builder.intern_constant(IrConstant::Int(6));
    let callable = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: callable,
            name: "adder_factory".to_owned(),
            args: vec![argument(Operand::Constant(six))],
        },
        span,
    );
    let mapped = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: mapped,
            name: "array_map".to_owned(),
            args: vec![
                argument(Operand::Register(callable)),
                argument(Operand::Local(array)),
            ],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(mapped)), span);

    let unit = builder.finish();
    let region = BaselineRegionBuilder::build(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing factory closure callback region");
    let instruction = region.blocks[0]
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.kind, RegionInstructionKind::ArrayCallback(_)))
        .expect("native array callback");
    let RegionInstructionKind::ArrayCallback(call) = &instruction.kind else {
        unreachable!("filtered above");
    };
    let callback_plan = call.callback.stable().expect("stable factory callback");
    assert_eq!(callback_plan.function, Some(closure));
    assert_eq!(
        callback_plan.closure,
        Some(RegionOperand::Register(callable))
    );
    assert_eq!(callback_plan.bound_object_count, 0);
    assert_eq!(callback_plan.capture_count, 1);
    assert!(instruction.register_uses().contains(&callable));
    assert!(region.direct_callees().contains(&factory));
    assert!(region.direct_callees().contains(&closure));
}

#[test]
fn published_external_static_method_stays_linked_across_callback_families() {
    let mut builder = IrBuilder::new(UnitId::new(100));
    let file = builder.add_file("linked-static-method-callback.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function(
        "linked_static_method_callback",
        FunctionFlags::default(),
        span,
    );
    let values = builder.intern_local(function, "values");
    builder.push_param(
        function,
        IrParam {
            name: "values".to_owned(),
            local: values,
            required: true,
            default: None,
            type_: Some(IrReturnType::Array),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let callback = builder.intern_constant(IrConstant::String("ExternalCallbacks::map".to_owned()));
    let callback_class =
        builder.intern_constant(IrConstant::String("ExternalCallbacks".to_owned()));
    let callback_method = builder.intern_constant(IrConstant::String("map".to_owned()));
    let four = builder.intern_constant(IrConstant::Int(4));
    let block = builder.append_block(function);
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };

    let mapped = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: mapped,
            name: "array_map".to_owned(),
            args: vec![
                argument(Operand::Constant(callback)),
                argument(Operand::Local(values)),
            ],
        },
        span,
    );
    let callable_array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray {
            dst: callable_array,
        },
        span,
    );
    let class_value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: class_value,
            constant: callback_class,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array: callable_array,
            key: None,
            value: Operand::Register(class_value),
            by_ref_local: None,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(class_value),
        },
        span,
    );
    let method_value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: method_value,
            constant: callback_method,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array: callable_array,
            key: None,
            value: Operand::Register(method_value),
            by_ref_local: None,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(method_value),
        },
        span,
    );
    let array_mapped = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: array_mapped,
            name: "array_map".to_owned(),
            args: vec![
                argument(Operand::Register(callable_array)),
                argument(Operand::Local(values)),
            ],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(callable_array),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(array_mapped),
        },
        span,
    );
    let called = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: called,
            name: "call_user_func".to_owned(),
            args: vec![
                argument(Operand::Constant(callback)),
                argument(Operand::Constant(four)),
            ],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(called),
        },
        span,
    );
    let called_array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: called_array,
            name: "call_user_func_array".to_owned(),
            args: vec![
                argument(Operand::Constant(callback)),
                argument(Operand::Local(values)),
            ],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(called_array),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(mapped)), span);

    let unit = builder.finish();
    let signature = crate::JitExternalFunctionSignature {
        name: "ExternalCallbacks::map".to_owned(),
        link_index: 7,
        published: true,
        params: vec![crate::JitExternalParameterSignature {
            name: "value".to_owned(),
            by_ref: false,
            variadic: false,
        }],
        native_params: vec![IrParam {
            name: "value".to_owned(),
            local: LocalId::new(0),
            required: true,
            default: None,
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        }],
        native_default_constant_indices: Vec::new(),
        native_arity: 1,
        requires_non_reference_trampoline: false,
        returns_by_reference: false,
        exception_routes: None,
    };
    let region = BaselineRegionBuilder::build_with_external_function_signatures(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
        &[signature],
    )
    .expect("linked static callback region");

    let callbacks = region.blocks[0]
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::ArrayCallback(call) => Some(call),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(callbacks.len(), 2);
    let callback = callbacks[0];
    let callback_plan = callback.callback.stable().expect("stable linked callback");
    assert_eq!(callback_plan.name, "ExternalCallbacks::map");
    assert_eq!(callback_plan.function, None);

    let linked_calls = region.blocks[0]
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call)
                if matches!(
                    &call.target,
                    RegionCallTarget::Function {
                        name,
                        function: None
                    } if name == "ExternalCallbacks::map"
                ) =>
            {
                Some(call)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(linked_calls.len(), 2);
    assert!(linked_calls.iter().any(|call| call.direct_arity == Some(1)));
    assert!(region.blocks[0].instructions.iter().all(|instruction| {
        !matches!(
            instruction.kind,
            RegionInstructionKind::NativeCall(RegionNativeCall {
                target: RegionCallTarget::StaticMethod { .. },
                ..
            })
        )
    }));
    assert!(region.blocks[0].instructions.iter().all(|instruction| {
        !matches!(
            instruction.kind,
            RegionInstructionKind::NewArray { .. } | RegionInstructionKind::ArrayInsert { .. }
        )
    }));
    assert!(region.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::Discard {
                src: RegionOperand::Register(register)
            } if register == class_value
        )
    }));
    assert!(region.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::Discard {
                src: RegionOperand::Register(register)
            } if register == method_value
        )
    }));
}

#[test]
fn exact_external_instance_callback_carries_receiver_without_callable_array() {
    let mut builder = IrBuilder::new(UnitId::new(101));
    let file = builder.add_file("linked-instance-method-callback.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function(
        "linked_instance_method_callback",
        FunctionFlags::default(),
        span,
    );
    let values = builder.intern_local(function, "values");
    builder.push_param(
        function,
        IrParam {
            name: "values".to_owned(),
            local: values,
            required: true,
            default: None,
            type_: Some(IrReturnType::Array),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let method = builder.intern_constant(IrConstant::String("map".to_owned()));
    let four = builder.intern_constant(IrConstant::Int(4));
    let block = builder.append_block(function);
    let object = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewObject {
            dst: object,
            class_name: "ExternalCallbacks".to_owned(),
            display_class_name: "ExternalCallbacks".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    let callable = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::NewArray { dst: callable },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array: callable,
            key: None,
            value: Operand::Register(object),
            by_ref_local: None,
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(object),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::ArrayInsert {
            array: callable,
            key: None,
            value: Operand::Constant(method),
            by_ref_local: None,
        },
        span,
    );
    let mapped = builder.alloc_register(function);
    let argument = |value| IrCallArg {
        name: None,
        value,
        unpack: false,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: mapped,
            name: "array_map".to_owned(),
            args: vec![
                argument(Operand::Register(callable)),
                argument(Operand::Local(values)),
            ],
        },
        span,
    );
    let called = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: called,
            name: "call_user_func".to_owned(),
            args: vec![
                argument(Operand::Register(callable)),
                argument(Operand::Constant(four)),
            ],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(called),
        },
        span,
    );
    let called_array = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: called_array,
            name: "call_user_func_array".to_owned(),
            args: vec![
                argument(Operand::Register(callable)),
                argument(Operand::Local(values)),
            ],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(called_array),
        },
        span,
    );
    let invoked = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallCallable {
            dst: invoked,
            callee: Operand::Register(callable),
            args: vec![argument(Operand::Constant(four))],
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(invoked),
        },
        span,
    );
    builder.emit(
        function,
        block,
        InstructionKind::Discard {
            src: Operand::Register(callable),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(mapped)), span);

    let unit = builder.finish();
    let signatures = [
        crate::JitExternalFunctionSignature {
            name: "ExternalCallbacks::__construct".to_owned(),
            link_index: 6,
            published: true,
            params: Vec::new(),
            native_params: Vec::new(),
            native_default_constant_indices: Vec::new(),
            native_arity: 0,
            requires_non_reference_trampoline: false,
            returns_by_reference: false,
            exception_routes: None,
        },
        crate::JitExternalFunctionSignature {
            name: "ExternalCallbacks::map".to_owned(),
            link_index: 7,
            published: true,
            params: vec![crate::JitExternalParameterSignature {
                name: "value".to_owned(),
                by_ref: false,
                variadic: false,
            }],
            native_params: vec![IrParam {
                name: "value".to_owned(),
                local: LocalId::new(1),
                required: true,
                default: None,
                type_: Some(IrReturnType::Int),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            }],
            native_default_constant_indices: Vec::new(),
            native_arity: 2,
            requires_non_reference_trampoline: false,
            returns_by_reference: false,
            exception_routes: None,
        },
    ];
    let region = BaselineRegionBuilder::build_with_external_function_signatures(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
        &signatures,
    )
    .expect("linked instance callback region");

    let instruction = region.blocks[0]
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.kind, RegionInstructionKind::ArrayCallback(_)))
        .expect("external instance array callback");
    let RegionInstructionKind::ArrayCallback(call) = &instruction.kind else {
        unreachable!("filtered above");
    };
    let callback_plan = call
        .callback
        .stable()
        .expect("stable linked instance callback");
    assert_eq!(callback_plan.name, "ExternalCallbacks::map");
    assert_eq!(callback_plan.function, None);
    assert_eq!(
        callback_plan.receiver,
        Some(RegionOperand::Register(object))
    );
    assert_eq!(callback_plan.bound_object_count, 1);
    assert_eq!(callback_plan.capture_count, 0);
    assert!(instruction.register_uses().contains(&object));
    assert!(!instruction.register_uses().contains(&callable));
    let linked_calls = region.blocks[0]
        .instructions
        .iter()
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call)
                if matches!(
                    &call.target,
                    RegionCallTarget::Function {
                        name,
                        function: None
                    } if name == "ExternalCallbacks::map"
                ) =>
            {
                Some(call)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(linked_calls.len(), 3);
    assert!(linked_calls.iter().all(|call| {
        call.argument_operand_offset == 1
            && call.operands.first() == Some(&Some(RegionOperand::Register(object)))
    }));
    assert!(linked_calls.iter().any(|call| call.direct_arity == Some(2)));
    assert!(
        linked_calls
            .iter()
            .any(|call| call.trailing_unpack_argument() == Some(0))
    );
    assert!(region.blocks[0].instructions.iter().all(|instruction| {
        !matches!(
            instruction.kind,
            RegionInstructionKind::NewArray { .. } | RegionInstructionKind::ArrayInsert { .. }
        )
    }));
    assert!(region.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::Discard {
                src: RegionOperand::Register(register)
            } if register == object
        )
    }));
}

#[test]
fn optimizing_linked_reference_return_uses_published_native_signature() {
    let mut builder = IrBuilder::new(UnitId::new(99));
    let file = builder.add_file("linked-reference-return.php");
    let span = IrSpan::new(file, 0, 20);
    let function = builder.start_function(
        "linked_reference_return_wrapper",
        FunctionFlags::default(),
        span,
    );
    let reference = builder.intern_local(function, "reference");
    let block = builder.append_block(function);
    let four = builder.intern_constant(IrConstant::Int(4));
    builder.emit(
        function,
        block,
        InstructionKind::BindReferenceFromCall {
            target: reference,
            name: "linked_reference_target".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Constant(four),
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
    builder.terminate_return(function, block, Some(Operand::Local(reference)), span);
    let unit = builder.finish();
    let region = BaselineRegionBuilder::build_with_external_function_signatures(
        &unit,
        function,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
        &[crate::JitExternalFunctionSignature {
            name: "linked_reference_target".to_owned(),
            link_index: 3,
            published: true,
            params: vec![crate::JitExternalParameterSignature {
                name: "value".to_owned(),
                by_ref: false,
                variadic: false,
            }],
            native_params: vec![IrParam {
                name: "value".to_owned(),
                local: LocalId::new(0),
                required: true,
                default: None,
                type_: Some(IrReturnType::Int),
                by_ref: false,
                variadic: false,
                attributes: Vec::new(),
            }],
            native_default_constant_indices: Vec::new(),
            native_arity: 1,
            requires_non_reference_trampoline: false,
            returns_by_reference: true,
            exception_routes: None,
        }],
    )
    .expect("optimizing linked reference-return region");
    let instruction = region.blocks[0]
        .instructions
        .iter()
        .find(|instruction| {
            matches!(
                instruction.kind,
                RegionInstructionKind::NativeCall(RegionNativeCall {
                    result: RegionCallResult::ReferenceLocal(candidate),
                    ..
                }) if candidate == reference
            )
        })
        .expect("linked reference-return native call");
    let RegionInstructionKind::NativeCall(call) = &instruction.kind else {
        unreachable!("filtered above");
    };
    assert_eq!(
        call.target,
        RegionCallTarget::Function {
            name: "linked_reference_target".to_owned(),
            function: None,
        }
    );
    assert_eq!(call.direct_arity, Some(1));
    assert_eq!(call.operands, vec![Some(RegionOperand::I64(4))]);
    assert!(call.returns_by_reference);
    assert!(!call.variadic);
}

#[test]
fn optimizing_same_unit_reference_method_uses_one_native_lvalue_plan() {
    let mut builder = IrBuilder::new(UnitId::new(10_019));
    let file = builder.add_file("same-unit-reference-method.php");
    let span = IrSpan::new(file, 0, 40);

    let method = builder.start_function(
        "ReferenceBox::slot",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.set_returns_by_ref(method, true);
    builder.intern_local(method, "this");
    let parameter = builder.intern_local(method, "value");
    builder.push_param(
        method,
        IrParam {
            name: "value".to_owned(),
            local: parameter,
            required: true,
            default: None,
            type_: None,
            by_ref: true,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let method_block = builder.append_block(method);
    builder.terminate_return_ref(method, method_block, parameter, span);

    let caller = builder.start_function("reference_method_caller", FunctionFlags::default(), span);
    builder.set_entry(caller);
    let array = builder.intern_local(caller, "array");
    let result_reference = builder.intern_local(caller, "result");
    let caller_block = builder.append_block(caller);
    let object = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::NewObject {
            dst: object,
            display_class_name: "ReferenceBox".to_owned(),
            class_name: "referencebox".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    let zero = builder.intern_constant(IrConstant::Int(0));
    let value = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::FetchDim {
            dst: value,
            array: Operand::Local(array),
            key: Operand::Constant(zero),
            quiet: false,
            mode: php_ir::instruction::DimFetchMode::Lvalue,
        },
        span,
    );
    builder.emit(
        caller,
        caller_block,
        InstructionKind::BindReferenceFromMethodCall {
            target: result_reference,
            object: Operand::Register(object),
            method: "slot".to_owned(),
            args: vec![IrCallArg {
                name: None,
                value: Operand::Register(value),
                unpack: false,
                value_kind: IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: Some(IrCallDimTarget {
                    local: array,
                    dims: vec![Operand::Constant(zero)],
                }),
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(
        caller,
        caller_block,
        Some(Operand::Local(result_reference)),
        span,
    );

    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "referencebox".to_owned(),
        display_name: "ReferenceBox".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "slot".to_owned(),
            origin_class: "referencebox".to_owned(),
            function: method,
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
        flags: ClassFlags {
            is_final: true,
            ..ClassFlags::default()
        },
        span,
    });

    let unit = builder.finish();
    let region = BaselineRegionBuilder::build(
        &unit,
        caller,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("optimizing same-unit reference method");
    assert_eq!(
        region.blocks[0]
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.kind, RegionInstructionKind::Nop))
            .count(),
        1,
        "the prepared reference binding must delete its superseded lvalue fetch"
    );
    let binding = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match instruction.kind {
            RegionInstructionKind::BindReferenceDim { target, .. } => Some(target),
            _ => None,
        })
        .expect("method argument lvalue binding");
    let call = region.blocks[0]
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call)
                if call.result == RegionCallResult::ReferenceLocal(result_reference) =>
            {
                Some(call)
            }
            _ => None,
        })
        .expect("direct reference-return method call");
    assert_eq!(
        call.target,
        RegionCallTarget::Function {
            name: "ReferenceBox::slot".to_owned(),
            function: Some(method),
        }
    );
    assert_eq!(call.argument_operand_offset, 1);
    assert_eq!(call.direct_arity, Some(2));
    assert_eq!(call.args[0].by_ref_local, Some(binding));
    assert!(call.args[0].by_ref_dim.is_none());
    assert_eq!(call.direct_compiled_target(), Some(method));
    assert_eq!(region.direct_callees(), vec![method]);
}

#[test]
fn static_method_closure_does_not_publish_a_null_implicit_receiver() {
    let mut builder = IrBuilder::new(UnitId::new(10_021));
    let file = builder.add_file("static-closure.php");
    let span = IrSpan::new(file, 0, 40);
    let closure = builder.start_function(
        "{closure}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.intern_local(closure, "this");
    let closure_block = builder.append_block(closure);
    builder.terminate_return(closure, closure_block, None, span);

    let caller = builder.start_function(
        "Factory::make",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    // Static methods retain the ordinary method IR shape, including a local
    // named `this`, but publication must not treat that uninitialized slot as
    // an object receiver.
    builder.intern_local(caller, "this");
    let caller_block = builder.append_block(caller);
    let callable = builder.alloc_register(caller);
    builder.emit(
        caller,
        caller_block,
        InstructionKind::MakeClosure {
            dst: callable,
            function: closure,
            captures: Vec::new(),
        },
        span,
    );
    builder.terminate_return(
        caller,
        caller_block,
        Some(Operand::Register(callable)),
        span,
    );
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "factory".to_owned(),
        display_name: "Factory".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "make".to_owned(),
            origin_class: "factory".to_owned(),
            function: caller,
            flags: ClassMethodFlags {
                is_static: true,
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

    assert_eq!(
        native_closure_bound_this_local(&unit, caller, closure),
        None
    );
    let region = BaselineRegionBuilder::build(
        &unit,
        caller,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("static method closure region");
    assert!(region.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::MakeClosure {
                bound_this_local: None,
                ..
            })
        )
    }));
}

#[test]
fn nested_closure_binds_the_resolved_receiver_local_after_captures() {
    let mut builder = IrBuilder::new(UnitId::new(10_022));
    let file = builder.add_file("nested-bound-closure.php");
    let span = IrSpan::new(file, 0, 40);

    let child = builder.start_function(
        "{child}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.intern_local(child, "this");
    let child_block = builder.append_block(child);
    builder.terminate_return(child, child_block, None, span);

    let parent = builder.start_function(
        "{parent}",
        FunctionFlags {
            is_closure: true,
            ..FunctionFlags::default()
        },
        span,
    );
    let captured = builder.intern_local(parent, "captured");
    builder.push_capture(
        parent,
        IrCapture {
            name: "captured".to_owned(),
            local: captured,
            by_ref: false,
        },
    );
    let parent_this = builder.intern_local(parent, "this");
    assert_ne!(parent_this, LocalId::new(0));
    let parent_block = builder.append_block(parent);
    let callable = builder.alloc_register(parent);
    builder.emit(
        parent,
        parent_block,
        InstructionKind::MakeClosure {
            dst: callable,
            function: child,
            captures: Vec::new(),
        },
        span,
    );
    builder.terminate_return(
        parent,
        parent_block,
        Some(Operand::Register(callable)),
        span,
    );
    let unit = builder.finish();

    assert_eq!(
        native_closure_bound_this_local(&unit, parent, child),
        Some(parent_this)
    );
    let region = BaselineRegionBuilder::build(
        &unit,
        parent,
        &CompileMetadata {
            tier: NativeCompilerTier::Optimizing,
            ..CompileMetadata::default()
        },
    )
    .expect("nested closure region");
    assert!(region.blocks[0].instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            RegionInstructionKind::NativeDynamicCode(
                RegionNativeDynamicCode::MakeClosure {
                    bound_this_local: Some(local),
                    ..
                }
            ) if local == parent_this
        )
    }));
}
