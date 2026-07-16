use super::*;
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
    assert!(
        region.blocks[0]
            .instructions
            .iter()
            .all(|instruction| !matches!(instruction.kind, RegionInstructionKind::MissingLowering))
    );
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
        instruction.kind,
        RegionInstructionKind::BindReferenceStaticProperty { .. }
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction.kind,
        RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::RegisterConstant { .. })
    )));
    assert!(instructions.iter().any(|instruction| matches!(
        instruction.kind,
        RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::EmitDiagnostic)
    )));
    assert!(
        instructions
            .iter()
            .all(|instruction| !matches!(instruction.kind, RegionInstructionKind::MissingLowering))
    );
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
    let block = builder.append_block(function);
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
    let method = region.declarations.method.expect("method identity");
    assert_eq!(method.class_display_name, "Widget");
    assert_eq!(method.method.function, function);
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
    assert!(
        native_calls
            .iter()
            .all(|instruction| !matches!(instruction.kind, RegionInstructionKind::MissingLowering))
    );
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
    assert!(
        !region
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction.kind, RegionInstructionKind::MissingLowering))
    );
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
