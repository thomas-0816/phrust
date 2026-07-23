use super::*;
use crate::region_ir::{
    SsaOwnership, analyze_baseline_value_ownership, analyze_executable_value_flow,
};
use php_ir::instruction::{IrCallDimTarget, IrCallPropertyTarget};
use php_ir::{
    ClassEntry, ClassFlags, ClassId, ClassMethodEntry, ClassMethodFlags, FunctionFlags, IrBuilder,
    IrCapture, IrParam, IrSpan, UnitId,
};

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
    builder.emit(
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
