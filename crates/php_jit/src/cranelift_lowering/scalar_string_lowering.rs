fn lower_optimizing_strict_scalar_equal(
    builder: &mut FunctionBuilder<'_>,
    lhs: ir::Value,
    rhs: ir::Value,
    deopt_out: ir::Value,
) -> (ir::Value, ir::Value) {
    let compare_integer = builder.create_block();
    let inspect_encoded = builder.create_block();
    let inspect = builder.create_block();
    let inspect_string_kind = builder.create_block();
    let inspect_runtime = builder.create_block();
    let inspect_same_kind = builder.create_block();
    let compare_string = builder.create_block();
    let matched = builder.create_block();
    let different = builder.create_block();
    let unsupported = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I8);
    builder.append_block_param(merge, types::I8);

    let (lhs_integer, lhs_raw) = lower_optimizing_integer_candidate(builder, lhs, deopt_out);
    let (rhs_integer, rhs_raw) = lower_optimizing_integer_candidate(builder, rhs, deopt_out);
    let either_integer = builder.ins().bor(lhs_integer, rhs_integer);
    builder
        .ins()
        .brif(either_integer, compare_integer, &[], inspect_encoded, &[]);

    builder.switch_to_block(compare_integer);
    let both_integer = builder.ins().band(lhs_integer, rhs_integer);
    let same_integer = builder.ins().icmp(IntCC::Equal, lhs_raw, rhs_raw);
    let same_integer = builder.ins().band(both_integer, same_integer);
    let yes = builder.ins().iconst(types::I8, 1);
    builder
        .ins()
        .jump(merge, &[yes.into(), same_integer.into()]);

    builder.switch_to_block(inspect_encoded);
    let identical = builder.ins().icmp(IntCC::Equal, lhs, rhs);
    builder.ins().brif(identical, matched, &[], inspect, &[]);

    builder.switch_to_block(inspect);
    let (lhs_string, _, _) = lower_native_string_key_descriptor(builder, lhs, deopt_out);
    let (rhs_string, _, _) = lower_native_string_key_descriptor(builder, rhs, deopt_out);
    let either_string = builder.ins().bor(lhs_string, rhs_string);
    builder.ins().brif(
        either_string,
        inspect_string_kind,
        &[],
        inspect_runtime,
        &[],
    );

    builder.switch_to_block(inspect_string_kind);
    let both_strings = builder.ins().band(lhs_string, rhs_string);
    builder
        .ins()
        .brif(both_strings, compare_string, &[], different, &[]);

    builder.switch_to_block(inspect_runtime);
    let lhs_runtime = lower_is_runtime_handle(builder, lhs);
    let rhs_runtime = lower_is_runtime_handle(builder, rhs);
    let both_runtime = builder.ins().band(lhs_runtime, rhs_runtime);
    let lhs_kind = builder
        .ins()
        .band_imm(lhs, crate::JIT_VALUE_RUNTIME_KIND_MASK as i64);
    let rhs_kind = builder
        .ins()
        .band_imm(rhs, crate::JIT_VALUE_RUNTIME_KIND_MASK as i64);
    let same_kind = builder.ins().icmp(IntCC::Equal, lhs_kind, rhs_kind);
    let inspect_runtime_kind = builder.ins().band(both_runtime, same_kind);
    builder
        .ins()
        .brif(inspect_runtime_kind, inspect_same_kind, &[], different, &[]);

    builder.switch_to_block(inspect_same_kind);
    let arrays = lower_value_has_tag(builder, lhs, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    let floats = lower_value_has_tag(builder, lhs, crate::JIT_VALUE_RUNTIME_FLOAT_TAG);
    let references = lower_value_has_tag(builder, lhs, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    let composite = builder.ins().bor(arrays, floats);
    let composite = builder.ins().bor(composite, references);
    builder
        .ins()
        .brif(composite, unsupported, &[], different, &[]);

    builder.switch_to_block(compare_string);
    let equal = lower_native_array_key_equal(builder, lhs, rhs, deopt_out);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[yes.into(), equal.into()]);

    builder.switch_to_block(matched);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[yes.into(), yes.into()]);
    builder.switch_to_block(different);
    let yes = builder.ins().iconst(types::I8, 1);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[yes.into(), no.into()]);
    builder.switch_to_block(unsupported);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[no.into(), no.into()]);

    builder.switch_to_block(merge);
    (
        builder.block_params(merge)[0],
        builder.block_params(merge)[1],
    )
}

#[derive(Clone, Copy)]
struct NativeNumericCandidate {
    /// Full numeric value accepted by PHP comparison semantics.
    is_numeric: ir::Value,
    /// Full or leading numeric prefix accepted by PHP cast/arithmetic rules.
    has_numeric_prefix: ir::Value,
    is_integer: ir::Value,
    is_nan: ir::Value,
    integer: ir::Value,
    number: ir::Value,
}

/// Calls the one pure compiled PHP numeric-string parser over an authoritative
/// native byte slice. The returned two-word record is already a native typed
/// value; no request state or Rust `Value` participates.
fn lower_optimizing_numeric_string_candidate(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    numeric_string: Option<NativeHelper>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<NativeNumericCandidate, CraneliftLoweringError> {
    let (length, bytes) = lower_optimizing_string_descriptor(builder, value, transition)?;
    lower_optimizing_numeric_bytes_candidate(module, builder, bytes, length, numeric_string)
}

fn lower_optimizing_numeric_bytes_candidate(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    bytes: ir::Value,
    length: ir::Value,
    numeric_string: Option<NativeHelper>,
) -> Result<NativeNumericCandidate, CraneliftLoweringError> {
    let helper = numeric_string.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_NATIVE_NUMERIC_STRING",
            "pure native numeric-string handler was not declared",
        )
    })?;
    let call = call_native_pure_handler(module, builder, helper, &[bytes, length]);
    let kind = builder.inst_results(call)[0];
    let payload = builder.inst_results(call)[1];
    let is_integer = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        crate::JIT_NATIVE_NUMERIC_STRING_INT as i64,
    );
    let is_float = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        crate::JIT_NATIVE_NUMERIC_STRING_FLOAT as i64,
    );
    let is_leading_integer = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        crate::JIT_NATIVE_NUMERIC_STRING_LEADING_INT as i64,
    );
    let is_leading_float = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        crate::JIT_NATIVE_NUMERIC_STRING_LEADING_FLOAT as i64,
    );
    let is_numeric = builder.ins().bor(is_integer, is_float);
    let is_leading = builder.ins().bor(is_leading_integer, is_leading_float);
    let has_numeric_prefix = builder.ins().bor(is_numeric, is_leading);
    let is_integer = builder.ins().bor(is_integer, is_leading_integer);
    let integer_number = builder.ins().fcvt_from_sint(types::F64, payload);
    let float_number = builder
        .ins()
        .bitcast(types::F64, MemFlagsData::new(), payload);
    let number = builder
        .ins()
        .select(is_integer, integer_number, float_number);
    let is_nan = builder.ins().iconst(types::I8, 0);
    Ok(NativeNumericCandidate {
        is_numeric,
        has_numeric_prefix,
        is_integer,
        is_nan,
        integer: payload,
        number,
    })
}

fn lower_optimizing_numeric_candidate_f64(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    deopt_out: ir::Value,
) -> NativeNumericCandidate {
    let integer = builder.create_block();
    let inspect_float = builder.create_block();
    let float = builder.create_block();
    let unsupported = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I8);
    builder.append_block_param(merge, types::I8);
    builder.append_block_param(merge, types::I64);
    builder.append_block_param(merge, types::F64);

    let (is_integer, integer_value) = lower_optimizing_integer_candidate(builder, value, deopt_out);
    builder
        .ins()
        .brif(is_integer, integer, &[], inspect_float, &[]);

    builder.switch_to_block(integer);
    let number = builder.ins().fcvt_from_sint(types::F64, integer_value);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(
        merge,
        &[yes.into(), yes.into(), integer_value.into(), number.into()],
    );

    builder.switch_to_block(inspect_float);
    let runtime = lower_is_runtime_handle(builder, value);
    let tagged = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_FLOAT_TAG);
    let index = builder.ins().ireduce(types::I32, value);
    let direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let admitted = builder.ins().band(runtime, tagged);
    let admitted = builder.ins().band(admitted, direct);
    builder.ins().brif(admitted, float, &[], unsupported, &[]);

    builder.switch_to_block(float);
    let slot = lower_optimizing_slot_address(builder, value, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let kind_matches = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_FLOAT),
    );
    let bits = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let number = builder.ins().bitcast(types::F64, MemFlagsData::new(), bits);
    let not_integer = builder.ins().iconst(types::I8, 0);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().brif(
        kind_matches,
        merge,
        &[
            kind_matches.into(),
            not_integer.into(),
            zero.into(),
            number.into(),
        ],
        unsupported,
        &[],
    );

    builder.switch_to_block(unsupported);
    let no = builder.ins().iconst(types::I8, 0);
    let zero_integer = builder.ins().iconst(types::I64, 0);
    let zero_float = builder
        .ins()
        .f64const(cranelift_codegen::ir::immediates::Ieee64::with_bits(0));
    builder.ins().jump(
        merge,
        &[no.into(), no.into(), zero_integer.into(), zero_float.into()],
    );

    builder.switch_to_block(merge);
    let number = builder.block_params(merge)[3];
    let is_nan = builder.ins().fcmp(FloatCC::Unordered, number, number);
    NativeNumericCandidate {
        is_numeric: builder.block_params(merge)[0],
        has_numeric_prefix: builder.block_params(merge)[0],
        is_integer: builder.block_params(merge)[1],
        is_nan,
        integer: builder.block_params(merge)[2],
        number,
    }
}

fn lower_optimizing_native_numeric_candidate(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    deopt_out: ir::Value,
) -> NativeNumericCandidate {
    lower_optimizing_numeric_candidate_f64(builder, value, deopt_out)
}

fn lower_optimizing_require_direct_numeric_candidate(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<NativeNumericCandidate, CraneliftLoweringError> {
    let value = lower_optimizing_reference_scalar(builder, value, false, transition)?;
    Ok(lower_optimizing_numeric_candidate_f64(
        builder,
        value,
        transition.deopt_out,
    ))
}

fn lower_optimizing_boolean_result(
    builder: &mut FunctionBuilder<'_>,
    condition: ir::Value,
) -> ir::Value {
    let yes = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
    );
    let no = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    builder.ins().select(condition, yes, no)
}

fn lower_optimizing_scalar_math(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operation: StableScalarMathBuiltin,
    arguments: &[ir::Value],
    fmod_f64: Option<NativeHelper>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    if operation == StableScalarMathBuiltin::Pi {
        let value = builder
            .ins()
            .f64const(cranelift_codegen::ir::immediates::Ieee64::with_float(
                std::f64::consts::PI,
            ));
        return lower_optimizing_encode_float(builder, value, transition);
    }

    let first =
        lower_optimizing_require_direct_numeric_candidate(builder, arguments[0], transition)?;
    match operation {
        StableScalarMathBuiltin::Abs => {
            let integer = builder.create_block();
            let integer_abs = builder.create_block();
            let float = builder.create_block();
            let merge = builder.create_block();
            builder.append_block_param(merge, types::I64);
            builder
                .ins()
                .brif(first.is_integer, integer, &[], float, &[]);

            builder.switch_to_block(integer);
            let minimum = builder
                .ins()
                .icmp_imm(IntCC::Equal, first.integer, i64::MIN);
            builder.ins().brif(minimum, float, &[], integer_abs, &[]);

            builder.switch_to_block(integer_abs);
            let negative = builder
                .ins()
                .icmp_imm(IntCC::SignedLessThan, first.integer, 0);
            let negated = builder.ins().ineg(first.integer);
            let value = builder.ins().select(negative, negated, first.integer);
            let value = lower_optimizing_admit_integer_result(builder, value, transition)?;
            builder.ins().jump(merge, &[value.into()]);

            builder.switch_to_block(float);
            let value = builder.ins().fabs(first.number);
            let value = lower_optimizing_encode_float(builder, value, transition)?;
            builder.ins().jump(merge, &[value.into()]);

            builder.switch_to_block(merge);
            Ok(builder.block_params(merge)[0])
        }
        StableScalarMathBuiltin::Ceil
        | StableScalarMathBuiltin::Floor
        | StableScalarMathBuiltin::Sqrt => {
            let value = match operation {
                StableScalarMathBuiltin::Ceil => builder.ins().ceil(first.number),
                StableScalarMathBuiltin::Floor => builder.ins().floor(first.number),
                StableScalarMathBuiltin::Sqrt => builder.ins().sqrt(first.number),
                _ => unreachable!(),
            };
            lower_optimizing_encode_float(builder, value, transition)
        }
        StableScalarMathBuiltin::Fdiv | StableScalarMathBuiltin::Fmod => {
            let second = lower_optimizing_require_direct_numeric_candidate(
                builder,
                arguments[1],
                transition,
            )?;
            let value = if operation == StableScalarMathBuiltin::Fdiv {
                builder.ins().fdiv(first.number, second.number)
            } else {
                let handler = fmod_f64.ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_NATIVE_FMOD_F64",
                        "pure native f64 remainder handler was not declared",
                    )
                })?;
                let call = call_native_pure_handler(
                    module,
                    builder,
                    handler,
                    &[first.number, second.number],
                );
                builder.inst_results(call)[0]
            };
            lower_optimizing_encode_float(builder, value, transition)
        }
        StableScalarMathBuiltin::IsFinite
        | StableScalarMathBuiltin::IsInfinite
        | StableScalarMathBuiltin::IsNan => {
            let condition = match operation {
                StableScalarMathBuiltin::IsFinite => {
                    let magnitude = builder.ins().fabs(first.number);
                    let maximum = builder.ins().f64const(
                        cranelift_codegen::ir::immediates::Ieee64::with_float(f64::MAX),
                    );
                    builder
                        .ins()
                        .fcmp(FloatCC::LessThanOrEqual, magnitude, maximum)
                }
                StableScalarMathBuiltin::IsInfinite => {
                    let magnitude = builder.ins().fabs(first.number);
                    let infinity = builder.ins().f64const(
                        cranelift_codegen::ir::immediates::Ieee64::with_float(f64::INFINITY),
                    );
                    builder.ins().fcmp(FloatCC::Equal, magnitude, infinity)
                }
                StableScalarMathBuiltin::IsNan => first.is_nan,
                _ => unreachable!(),
            };
            Ok(lower_optimizing_boolean_result(builder, condition))
        }
        StableScalarMathBuiltin::Pi => unreachable!(),
    }
}

fn lower_optimizing_pure_math(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operation: StablePureMathBuiltin,
    arguments: &[ir::Value],
    handler: Option<NativeHelper>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let handler = handler.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_NATIVE_PURE_MATH",
            format!(
                "exact pure math handler {} was not declared",
                operation.symbol()
            ),
        )
    })?;
    let mut native_arguments = Vec::with_capacity(arguments.len());
    for &argument in arguments {
        native_arguments.push(
            lower_optimizing_require_direct_numeric_candidate(builder, argument, transition)?
                .number,
        );
    }
    let call = call_native_pure_handler(module, builder, handler, &native_arguments);
    let value = builder.inst_results(call)[0];
    lower_optimizing_encode_float(builder, value, transition)
}

#[allow(clippy::too_many_arguments)]
fn lower_optimizing_scalar_conversion(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operation: StableScalarConsumerBuiltin,
    encoded: ir::Value,
    int_cast: Option<NativeHelper>,
    float_cast: Option<NativeHelper>,
    string_cast: Option<NativeHelper>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    match operation {
        StableScalarConsumerBuiltin::BoolVal => {
            let truthy = lower_optimizing_truthy(builder, encoded, transition)?;
            Ok(encode_native_bool(builder, truthy))
        }
        StableScalarConsumerBuiltin::FloatVal => emit_total_exact_scalar_value!(
            module,
            builder,
            float_cast,
            &[encoded],
            transition,
            "exact native float-cast handler was not declared",
        ),
        StableScalarConsumerBuiltin::IntVal => emit_total_exact_scalar_value!(
            module,
            builder,
            int_cast,
            &[encoded],
            transition,
            "exact native integer-cast handler was not declared",
        ),
        StableScalarConsumerBuiltin::StrVal => emit_total_exact_scalar_value!(
            module,
            builder,
            string_cast,
            &[encoded],
            transition,
            "exact native string-cast handler was not declared",
        ),
        StableScalarConsumerBuiltin::GetType | StableScalarConsumerBuiltin::GetDebugType => {
            unreachable!("type-name consumers use their dedicated native lowering")
        }
    }
}

fn lower_optimizing_type_name(
    builder: &mut FunctionBuilder<'_>,
    operation: StableScalarConsumerBuiltin,
    fact: SsaValueFact,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    debug_assert!(matches!(
        operation,
        StableScalarConsumerBuiltin::GetType | StableScalarConsumerBuiltin::GetDebugType
    ));
    let debug = operation == StableScalarConsumerBuiltin::GetDebugType;
    let bytes: Option<&[u8]> = match fact.class {
        SsaValueClass::Null => {
            if debug {
                Some(b"null")
            } else {
                Some(b"NULL")
            }
        }
        SsaValueClass::Bool => {
            if debug {
                Some(b"bool")
            } else {
                Some(b"boolean")
            }
        }
        SsaValueClass::Int => {
            if debug {
                Some(b"int")
            } else {
                Some(b"integer")
            }
        }
        SsaValueClass::Float => {
            if debug {
                Some(b"float")
            } else {
                Some(b"double")
            }
        }
        SsaValueClass::StringHandle => Some(b"string"),
        SsaValueClass::ArrayHandle => Some(b"array"),
        SsaValueClass::CallableHandle => Some(if debug { b"Closure" } else { b"object" }),
        SsaValueClass::ResourceHandle => Some(b"resource"),
        SsaValueClass::GeneratorHandle => Some(if debug { b"Generator" } else { b"object" }),
        SsaValueClass::FiberHandle => Some(if debug { b"Fiber" } else { b"object" }),
        SsaValueClass::ObjectHandle if !debug => Some(b"object"),
        SsaValueClass::Uninitialized
        | SsaValueClass::ObjectHandle
        | SsaValueClass::ReferenceHandle
        | SsaValueClass::MixedHandle => None,
    };
    let bytes = bytes.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_TYPE_NAME_PUBLICATION",
            "dynamic get_debug_type class naming requires an exact published class-name plan",
        )
    })?;
    lower_optimizing_static_string(builder, bytes, transition)
}

fn lower_optimizing_require_direct_integer(
    builder: &mut FunctionBuilder<'_>,
    encoded: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let encoded = lower_optimizing_reference_scalar(builder, encoded, false, transition)?;
    Ok(lower_optimizing_integer_candidate(builder, encoded, transition.deopt_out).1)
}

fn lower_optimizing_intdiv(
    builder: &mut FunctionBuilder<'_>,
    dividend: ir::Value,
    divisor: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let dividend = lower_optimizing_require_direct_integer(builder, dividend, transition)?;
    let divisor = lower_optimizing_require_direct_integer(builder, divisor, transition)?;
    let result = builder.ins().sdiv(dividend, divisor);
    lower_optimizing_admit_integer_result(builder, result, transition)
}

fn lower_optimizing_round(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    arguments: &[ir::Value],
    handler: Option<NativeHelper>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let value =
        lower_optimizing_require_direct_numeric_candidate(builder, arguments[0], transition)?
            .number;
    let precision = if let Some(&precision) = arguments.get(1) {
        lower_optimizing_require_direct_integer(builder, precision, transition)?
    } else {
        builder.ins().iconst(types::I64, 0)
    };
    let mode = if let Some(&mode) = arguments.get(2) {
        lower_optimizing_require_direct_integer(builder, mode, transition)?
    } else {
        builder.ins().iconst(types::I64, 1)
    };
    let handler = handler.ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_NATIVE_ROUND_F64",
            "pure native PHP round handler was not declared",
        )
    })?;
    let call = call_native_pure_handler(module, builder, handler, &[value, precision, mode]);
    let result = builder.inst_results(call)[0];
    lower_optimizing_encode_float(builder, result, transition)
}

fn lower_optimizing_numeric_candidates_equal(
    builder: &mut FunctionBuilder<'_>,
    lhs: NativeNumericCandidate,
    rhs: NativeNumericCandidate,
) -> ir::Value {
    let both_integer = builder.ins().band(lhs.is_integer, rhs.is_integer);
    let integer_equal = builder.ins().icmp(IntCC::Equal, lhs.integer, rhs.integer);
    let numeric_equal = builder.ins().fcmp(FloatCC::Equal, lhs.number, rhs.number);
    builder
        .ins()
        .select(both_integer, integer_equal, numeric_equal)
}


/// PHP loose equality for authoritative native values. Scalar comparisons,
/// including exact numeric strings and PHP 8 number/string coercion, remain
/// native. Only unsupported compound semantics return an unsupported marker
/// for the caller's one exact full-operation continuation.
#[allow(clippy::too_many_arguments)]
fn lower_optimizing_loose_scalar_equal(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    lhs: ir::Value,
    rhs: ir::Value,
    constants: &[IrConstant],
    float_to_string: Option<NativeHelper>,
    numeric_string: Option<NativeHelper>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<(ir::Value, ir::Value), CraneliftLoweringError> {
    let lhs = lower_optimizing_reference_scalar(builder, lhs, false, transition)?;
    let rhs = lower_optimizing_reference_scalar(builder, rhs, false, transition)?;
    let inspect_null = builder.create_block();
    let compare_boolean = builder.create_block();
    let compare_null = builder.create_block();
    let classify_null_other = builder.create_block();
    let inspect_strings = builder.create_block();
    let compare_strings = builder.create_block();
    let unequal_strings = builder.create_block();
    let compare_numeric_strings = builder.create_block();
    let compare_mixed_string = builder.create_block();
    let compare_mixed_numeric = builder.create_block();
    let compare_mixed_non_nan = builder.create_block();
    let compare_mixed_numeric_values = builder.create_block();
    let compare_mixed_lexical = builder.create_block();
    let inspect_numeric = builder.create_block();
    let compare_numeric = builder.create_block();
    let inspect_identity = builder.create_block();
    let matched = builder.create_block();
    let different = builder.create_block();
    let unsupported = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I8);
    builder.append_block_param(merge, types::I8);

    let is_bool = |builder: &mut FunctionBuilder<'_>, value| {
        let false_ = builder.ins().icmp_imm(
            IntCC::Equal,
            value,
            crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
        );
        let true_ = builder.ins().icmp_imm(
            IntCC::Equal,
            value,
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        );
        builder.ins().bor(false_, true_)
    };
    let lhs_bool = is_bool(builder, lhs);
    let rhs_bool = is_bool(builder, rhs);
    let either_bool = builder.ins().bor(lhs_bool, rhs_bool);
    builder
        .ins()
        .brif(either_bool, compare_boolean, &[], inspect_null, &[]);

    builder.switch_to_block(compare_boolean);
    let lhs_truthy = lower_optimizing_truthy(builder, lhs, transition)?;
    let rhs_truthy = lower_optimizing_truthy(builder, rhs, transition)?;
    let equal = builder.ins().icmp(IntCC::Equal, lhs_truthy, rhs_truthy);
    builder.ins().brif(equal, matched, &[], different, &[]);

    builder.switch_to_block(inspect_null);
    let lhs_null = builder
        .ins()
        .icmp_imm(IntCC::Equal, lhs, crate::jit_encode_constant(u32::MAX));
    let rhs_null = builder
        .ins()
        .icmp_imm(IntCC::Equal, rhs, crate::jit_encode_constant(u32::MAX));
    let either_null = builder.ins().bor(lhs_null, rhs_null);
    builder
        .ins()
        .brif(either_null, compare_null, &[], inspect_strings, &[]);

    builder.switch_to_block(compare_null);
    let both_null = builder.ins().band(lhs_null, rhs_null);
    builder
        .ins()
        .brif(both_null, matched, &[], classify_null_other, &[]);

    builder.switch_to_block(classify_null_other);
    let other = builder.ins().select(lhs_null, rhs, lhs);
    let (other_string, other_length, _) =
        lower_native_string_key_descriptor(builder, other, transition.deopt_out);
    let compare_null_string = builder.create_block();
    let compare_null_boolean = builder.create_block();
    builder.ins().brif(
        other_string,
        compare_null_string,
        &[],
        compare_null_boolean,
        &[],
    );

    builder.switch_to_block(compare_null_string);
    let empty = builder.ins().icmp_imm(IntCC::Equal, other_length, 0);
    builder.ins().brif(empty, matched, &[], different, &[]);

    builder.switch_to_block(compare_null_boolean);
    let lhs_truthy = lower_optimizing_truthy(builder, lhs, transition)?;
    let rhs_truthy = lower_optimizing_truthy(builder, rhs, transition)?;
    let equal = builder.ins().icmp(IntCC::Equal, lhs_truthy, rhs_truthy);
    builder.ins().brif(equal, matched, &[], different, &[]);

    builder.switch_to_block(inspect_strings);
    let (lhs_string, _, _) = lower_native_string_key_descriptor(builder, lhs, transition.deopt_out);
    let (rhs_string, _, _) = lower_native_string_key_descriptor(builder, rhs, transition.deopt_out);
    let either_string = builder.ins().bor(lhs_string, rhs_string);
    let both_strings = builder.ins().band(lhs_string, rhs_string);
    builder
        .ins()
        .brif(either_string, compare_strings, &[], inspect_numeric, &[]);

    builder.switch_to_block(compare_strings);
    builder.ins().brif(
        both_strings,
        unequal_strings,
        &[],
        compare_mixed_string,
        &[],
    );

    builder.switch_to_block(unequal_strings);
    let bytes_equal = lower_native_array_key_equal(builder, lhs, rhs, transition.deopt_out);
    let inspect_unequal_strings = builder.create_block();
    builder
        .ins()
        .brif(bytes_equal, matched, &[], inspect_unequal_strings, &[]);

    builder.switch_to_block(inspect_unequal_strings);
    let lhs_numeric = lower_optimizing_numeric_string_candidate(
        module,
        builder,
        lhs,
        numeric_string,
        transition,
    )?;
    let rhs_numeric = lower_optimizing_numeric_string_candidate(
        module,
        builder,
        rhs,
        numeric_string,
        transition,
    )?;
    let numeric_comparison = builder
        .ins()
        .band(lhs_numeric.is_numeric, rhs_numeric.is_numeric);
    builder.ins().brif(
        numeric_comparison,
        compare_numeric_strings,
        &[],
        different,
        &[],
    );

    builder.switch_to_block(compare_numeric_strings);
    let equal = lower_optimizing_numeric_candidates_equal(builder, lhs_numeric, rhs_numeric);
    builder.ins().brif(equal, matched, &[], different, &[]);

    builder.switch_to_block(compare_mixed_string);
    let string = builder.ins().select(lhs_string, lhs, rhs);
    let other = builder.ins().select(lhs_string, rhs, lhs);
    let string_numeric = lower_optimizing_numeric_string_candidate(
        module,
        builder,
        string,
        numeric_string,
        transition,
    )?;
    let other_numeric =
        lower_optimizing_native_numeric_candidate(builder, other, transition.deopt_out);
    builder.ins().brif(
        other_numeric.is_numeric,
        compare_mixed_numeric,
        &[],
        unsupported,
        &[],
    );

    builder.switch_to_block(compare_mixed_numeric);
    builder.ins().brif(
        other_numeric.is_nan,
        different,
        &[],
        compare_mixed_non_nan,
        &[],
    );

    builder.switch_to_block(compare_mixed_non_nan);
    builder.ins().brif(
        string_numeric.is_numeric,
        compare_mixed_numeric_values,
        &[],
        compare_mixed_lexical,
        &[],
    );

    builder.switch_to_block(compare_mixed_numeric_values);
    let equal = lower_optimizing_numeric_candidates_equal(builder, string_numeric, other_numeric);
    builder.ins().brif(equal, matched, &[], different, &[]);

    builder.switch_to_block(compare_mixed_lexical);
    let coerced = lower_optimizing_scalar_string_coercion(
        module,
        builder,
        other,
        RegionOperand::I64(0),
        constants,
        float_to_string,
        unsupported,
        transition,
    )?;
    let equal = lower_native_array_key_equal(builder, string, coerced.value, transition.deopt_out);
    lower_optimizing_commit_owned_value_if(builder, coerced.value, coerced.temporary, transition);
    builder.ins().brif(equal, matched, &[], different, &[]);

    builder.switch_to_block(inspect_numeric);
    let lhs_numeric = lower_optimizing_native_numeric_candidate(builder, lhs, transition.deopt_out);
    let rhs_numeric = lower_optimizing_native_numeric_candidate(builder, rhs, transition.deopt_out);
    let both_numeric = builder
        .ins()
        .band(lhs_numeric.is_numeric, rhs_numeric.is_numeric);
    builder
        .ins()
        .brif(both_numeric, compare_numeric, &[], inspect_identity, &[]);

    builder.switch_to_block(compare_numeric);
    let equal = lower_optimizing_numeric_candidates_equal(builder, lhs_numeric, rhs_numeric);
    builder.ins().brif(equal, matched, &[], different, &[]);

    builder.switch_to_block(inspect_identity);
    let identical = builder.ins().icmp(IntCC::Equal, lhs, rhs);
    builder
        .ins()
        .brif(identical, matched, &[], unsupported, &[]);

    builder.switch_to_block(matched);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[yes.into(), yes.into()]);
    builder.switch_to_block(different);
    let yes = builder.ins().iconst(types::I8, 1);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[yes.into(), no.into()]);
    builder.switch_to_block(unsupported);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[no.into(), no.into()]);
    builder.switch_to_block(merge);
    Ok((
        builder.block_params(merge)[0],
        builder.block_params(merge)[1],
    ))
}

#[allow(clippy::too_many_arguments)]
fn lower_optimizing_array_lookup(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operation: StableArrayLookupBuiltin,
    needle: ir::Value,
    array: ir::Value,
    strict: Option<ir::Value>,
    constants: &[IrConstant],
    float_to_string: Option<NativeHelper>,
    numeric_string: Option<NativeHelper>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let strict = strict
        .map(|strict| lower_optimizing_require_boolean_flag(builder, strict, transition))
        .transpose()?;
    let scan = builder.create_block();
    let compare = builder.create_block();
    let next = builder.create_block();
    let matched = builder.create_block();
    let missing = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(next, types::I64);
    builder.append_block_param(matched, types::I64);
    builder.append_block_param(merge, types::I64);
    let (_, length, entries) =
        lower_optimizing_direct_array_descriptor(builder, array, transition)?;
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(scan, &[zero.into()]);

    builder.switch_to_block(scan);
    let index = builder.block_params(scan)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(exhausted, missing, &[], compare, &[]);

    builder.switch_to_block(compare);
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let entry_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let offset = builder.ins().ishl_imm(entry_index, 4);
    let entry = builder.ins().iadd(entries, offset);
    let candidate = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let candidate = lower_optimizing_reference_scalar(builder, candidate, false, transition)?;
    let (_supported, equal) = if let Some(strict) = strict {
        let compare_strict = builder.create_block();
        let compare_loose = builder.create_block();
        let compared = builder.create_block();
        builder.append_block_param(compared, types::I8);
        builder.append_block_param(compared, types::I8);
        builder
            .ins()
            .brif(strict, compare_strict, &[], compare_loose, &[]);
        builder.switch_to_block(compare_strict);
        let (supported, equal) =
            lower_optimizing_strict_scalar_equal(builder, candidate, needle, transition.deopt_out);
        builder
            .ins()
            .jump(compared, &[supported.into(), equal.into()]);
        builder.switch_to_block(compare_loose);
        let (supported, equal) = lower_optimizing_loose_scalar_equal(
            module,
            builder,
            candidate,
            needle,
            constants,
            float_to_string,
            numeric_string,
            transition,
        )?;
        builder
            .ins()
            .jump(compared, &[supported.into(), equal.into()]);
        builder.switch_to_block(compared);
        (
            builder.block_params(compared)[0],
            builder.block_params(compared)[1],
        )
    } else {
        lower_optimizing_loose_scalar_equal(
            module,
            builder,
            candidate,
            needle,
            constants,
            float_to_string,
            numeric_string,
            transition,
        )?
    };
    builder
        .ins()
        .brif(equal, matched, &[index.into()], next, &[index.into()]);

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let next_index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(scan, &[next_index.into()]);

    builder.switch_to_block(matched);
    let index = builder.block_params(matched)[0];
    if matches!(operation, StableArrayLookupBuiltin::InArray) {
        let yes = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        );
        builder.ins().jump(merge, &[yes.into()]);
    } else {
        let entry_index = if pointer_type == types::I64 {
            index
        } else {
            builder.ins().ireduce(pointer_type, index)
        };
        let offset = builder.ins().ishl_imm(entry_index, 4);
        let entry = builder.ins().iadd(entries, offset);
        let key = builder
            .ins()
            .load(types::I64, MemFlagsData::new(), entry, 0);
        lower_optimizing_retain(builder, key, transition.deopt_out);
        builder.ins().jump(merge, &[key.into()]);
    }

    builder.switch_to_block(missing);
    let no = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    builder.ins().jump(merge, &[no.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_optimizing_array_edge_key(
    builder: &mut FunctionBuilder<'_>,
    operation: StableArrayEdgeKeyBuiltin,
    array: ir::Value,
    total: bool,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (_, length, entries) = if total {
        lower_total_native_array_descriptor(builder, array, transition.deopt_out)
    } else {
        lower_optimizing_direct_array_descriptor(builder, array, transition)?
    };
    let present = builder.create_block();
    let missing = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);
    let empty = builder.ins().icmp_imm(IntCC::Equal, length, 0);
    builder.ins().brif(empty, missing, &[], present, &[]);

    builder.switch_to_block(present);
    let index = match operation {
        StableArrayEdgeKeyBuiltin::First => builder.ins().iconst(types::I64, 0),
        StableArrayEdgeKeyBuiltin::Last => builder.ins().iadd_imm(length, -1),
    };
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let offset = builder.ins().ishl_imm(index, 4);
    let entry = builder.ins().iadd(entries, offset);
    let key = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), entry, 0);
    lower_optimizing_retain(builder, key, transition.deopt_out);
    builder.ins().jump(merge, &[key.into()]);

    builder.switch_to_block(missing);
    let null = builder
        .ins()
        .iconst(types::I64, crate::jit_encode_constant(u32::MAX));
    builder.ins().jump(merge, &[null.into()]);
    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

fn lower_optimizing_array_is_list(
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    total: bool,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (_, length, entries) = if total {
        lower_total_native_array_descriptor(builder, array, transition.deopt_out)
    } else {
        lower_optimizing_direct_array_descriptor(builder, array, transition)?
    };
    let scan = builder.create_block();
    let compare = builder.create_block();
    let matched = builder.create_block();
    let different = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(merge, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(scan, &[zero.into()]);

    builder.switch_to_block(scan);
    let index = builder.block_params(scan)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(exhausted, matched, &[], compare, &[]);

    builder.switch_to_block(compare);
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let pointer_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let entry = builder.ins().iadd(entries, offset);
    let key = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), entry, 0);
    let same = builder.ins().icmp(IntCC::Equal, key, index);
    let next = builder.ins().iadd_imm(index, 1);
    builder
        .ins()
        .brif(same, scan, &[next.into()], different, &[]);

    builder.switch_to_block(matched);
    let yes = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
    );
    builder.ins().jump(merge, &[yes.into()]);
    builder.switch_to_block(different);
    let no = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    builder.ins().jump(merge, &[no.into()]);
    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

#[derive(Clone, Copy)]
enum NativeStringMaterialization {
    Copy { source: ir::Value },
    Byte { value: ir::Value },
}

#[derive(Clone, Copy)]
struct NativeStringAllocation {
    value: ir::Value,
    output: ir::Value,
    slot: ir::Value,
}

fn lower_optimizing_allocate_string(
    builder: &mut FunctionBuilder<'_>,
    length: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<NativeStringAllocation, CraneliftLoweringError> {
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let sizing = builder.create_block();
    let resources = builder.create_block();
    let allocate = builder.create_block();
    let reuse = builder.create_block();
    let bump = builder.create_block();
    let initialize = builder.create_block();
    for block in [sizing, resources, allocate, reuse, bump] {
        builder.append_block_param(block, types::I32);
    }
    builder.append_block_param(initialize, pointer_type);
    builder.append_block_param(initialize, types::I32);

    let length32 = builder.ins().ireduce(types::I32, length);
    let minimum = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY),
    );
    builder.ins().jump(sizing, &[minimum.into()]);

    builder.switch_to_block(sizing);
    let capacity = builder.block_params(sizing)[0];
    let enough = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, capacity, length32);
    let doubled = builder.ins().imul_imm(capacity, 2);
    builder.ins().brif(
        enough,
        resources,
        &[capacity.into()],
        sizing,
        &[doubled.into()],
    );

    builder.switch_to_block(resources);
    let capacity = builder.block_params(resources)[0];
    let view = lower_active_runtime_view(builder, transition.deopt_out);
    let byte_next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_next) as i32,
    );
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let byte_arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_bytes) as i32,
    );
    let free_heads = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_free_heads) as i32,
    );
    let leading = builder.ins().clz(capacity);
    let ceiling = builder.ins().iconst(types::I32, 31);
    let bucket = builder.ins().isub(ceiling, leading);
    let wide_bucket = builder.ins().uextend(pointer_type, bucket);
    let bucket_offset = builder.ins().ishl_imm(wide_bucket, 2);
    let free_head_ptr = builder.ins().iadd(free_heads, bucket_offset);
    let free_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), free_head_ptr, 0);
    let has_free = builder.ins().icmp_imm(
        IntCC::NotEqual,
        free_head,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE),
    );
    let byte_next = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), byte_next_ptr, 0);
    let byte_end = builder.ins().iadd(byte_next, capacity);
    builder.ins().jump(allocate, &[capacity.into()]);

    builder.switch_to_block(allocate);
    let capacity = builder.block_params(allocate)[0];
    let slot_next = lower_reserve_direct_value_index(builder, transition.deopt_out, None);
    builder.ins().brif(
        has_free,
        reuse,
        &[capacity.into()],
        bump,
        &[capacity.into()],
    );

    builder.switch_to_block(reuse);
    let capacity = builder.block_params(reuse)[0];
    let wide_head = builder.ins().uextend(pointer_type, free_head);
    let output = builder.ins().iadd(byte_arena, wide_head);
    let preceding = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), output, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), preceding, free_head_ptr, 0);
    let reused_bytes_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_reused_bytes) as i32,
    );
    let reused_bytes = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), reused_bytes_ptr, 0);
    let capacity_wide = builder.ins().uextend(types::I64, capacity);
    let reused_bytes = builder.ins().iadd(reused_bytes, capacity_wide);
    builder
        .ins()
        .store(MemFlagsData::new(), reused_bytes, reused_bytes_ptr, 0);
    builder
        .ins()
        .jump(initialize, &[output.into(), capacity.into()]);

    builder.switch_to_block(bump);
    let capacity = builder.block_params(bump)[0];
    builder
        .ins()
        .store(MemFlagsData::new(), byte_end, byte_next_ptr, 0);
    let byte_offset = builder.ins().uextend(pointer_type, byte_next);
    let output = builder.ins().iadd(byte_arena, byte_offset);
    builder
        .ins()
        .jump(initialize, &[output.into(), capacity.into()]);

    builder.switch_to_block(initialize);
    let output = builder.block_params(initialize)[0];
    let capacity = builder.block_params(initialize)[1];
    let slot_index = builder.ins().uextend(pointer_type, slot_next);
    let slot_offset = builder.ins().ishl_imm(slot_index, 5);
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
    let reserved = builder.ins().ishl_imm(
        capacity,
        crate::JIT_NATIVE_DIRECT_STRING_CAPACITY_SHIFT as i64,
    );
    builder.ins().store(
        MemFlagsData::new(),
        reserved,
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
    Ok(NativeStringAllocation {
        value: encoded,
        output,
        slot,
    })
}

fn lower_optimizing_finish_string(
    builder: &mut FunctionBuilder<'_>,
    allocation: NativeStringAllocation,
    length: ir::Value,
) {
    let inspect_zero = builder.create_block();
    let store_flag = builder.create_block();
    builder.append_block_param(store_flag, types::I32);
    let length_is_one = builder.ins().icmp_imm(IntCC::Equal, length, 1);
    let no_zero_flag = builder.ins().iconst(types::I32, 0);
    builder.ins().brif(
        length_is_one,
        inspect_zero,
        &[],
        store_flag,
        &[no_zero_flag.into()],
    );

    builder.switch_to_block(inspect_zero);
    let first = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), allocation.output, 0);
    let first_is_zero = builder.ins().icmp_imm(IntCC::Equal, first, b'0' as i64);
    let yes_zero_flag = builder
        .ins()
        .iconst(types::I32, i64::from(crate::JIT_NATIVE_STRING_VALUE_ZERO));
    let zero_flag = builder
        .ins()
        .select(first_is_zero, yes_zero_flag, no_zero_flag);
    builder.ins().jump(store_flag, &[zero_flag.into()]);

    builder.switch_to_block(store_flag);
    let zero_flag = builder.block_params(store_flag)[0];
    let reserved = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        allocation.slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let reserved = builder.ins().bor(reserved, zero_flag);
    builder.ins().store(
        MemFlagsData::new(),
        reserved,
        allocation.slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
}

fn lower_optimizing_materialize_string(
    builder: &mut FunctionBuilder<'_>,
    length: ir::Value,
    materialization: NativeStringMaterialization,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let allocation = lower_optimizing_allocate_string(builder, length, transition)?;
    let copy = builder.create_block();
    let copy_byte = builder.create_block();
    let finish = builder.create_block();
    builder.append_block_param(copy, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(copy, &[zero.into()]);

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let done = builder.ins().icmp(IntCC::Equal, index, length);
    builder.ins().brif(done, finish, &[], copy_byte, &[]);

    builder.switch_to_block(copy_byte);
    let byte = match materialization {
        NativeStringMaterialization::Copy { source } => {
            let source = builder.ins().iadd(source, index);
            builder
                .ins()
                .load(types::I8, MemFlagsData::new(), source, 0)
        }
        NativeStringMaterialization::Byte { value } => value,
    };
    let destination = builder.ins().iadd(allocation.output, index);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(copy, &[next.into()]);

    builder.switch_to_block(finish);
    lower_optimizing_finish_string(builder, allocation, length);
    Ok(allocation.value)
}

fn lower_optimizing_string_descriptor(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<(ir::Value, ir::Value), CraneliftLoweringError> {
    let (_, length, bytes) =
        lower_native_string_key_descriptor(builder, value, transition.deopt_out);
    Ok((length, bytes))
}

fn lower_optimizing_integer_to_string(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let count = builder.create_block();
    let count_more = builder.create_block();
    let counted = builder.create_block();
    let write_sign = builder.create_block();
    let write_digits = builder.create_block();
    let write_next = builder.create_block();
    let finish = builder.create_block();
    builder.append_block_param(count, types::I64);
    builder.append_block_param(count, types::I64);
    builder.append_block_param(counted, types::I64);
    builder.append_block_param(write_digits, types::I64);
    builder.append_block_param(write_digits, types::I64);
    builder.append_block_param(write_next, types::I64);
    builder.append_block_param(write_next, types::I64);

    let negative = builder.ins().icmp_imm(IntCC::SignedLessThan, value, 0);
    let negated = builder.ins().ineg(value);
    let magnitude = builder.ins().select(negative, negated, value);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(count, &[magnitude.into(), zero.into()]);

    builder.switch_to_block(count);
    let remaining = builder.block_params(count)[0];
    let digits = builder.block_params(count)[1];
    let next_digits = builder.ins().iadd_imm(digits, 1);
    let last = builder
        .ins()
        .icmp_imm(IntCC::UnsignedLessThan, remaining, 10);
    builder
        .ins()
        .brif(last, counted, &[next_digits.into()], count_more, &[]);

    builder.switch_to_block(count_more);
    let count_base = builder.ins().iconst(types::I64, 10);
    let quotient = builder.ins().udiv(remaining, count_base);
    builder
        .ins()
        .jump(count, &[quotient.into(), next_digits.into()]);

    builder.switch_to_block(counted);
    let digits = builder.block_params(counted)[0];
    let sign = builder.ins().uextend(types::I64, negative);
    let length = builder.ins().iadd(digits, sign);
    let allocation = lower_optimizing_allocate_string(builder, length, transition)?;
    builder.ins().brif(
        negative,
        write_sign,
        &[],
        write_digits,
        &[magnitude.into(), length.into()],
    );

    builder.switch_to_block(write_sign);
    let minus = builder.ins().iconst(types::I8, i64::from(b'-'));
    builder
        .ins()
        .store(MemFlagsData::new(), minus, allocation.output, 0);
    builder
        .ins()
        .jump(write_digits, &[magnitude.into(), length.into()]);

    builder.switch_to_block(write_digits);
    let remaining = builder.block_params(write_digits)[0];
    let position = builder.block_params(write_digits)[1];
    let write_base = builder.ins().iconst(types::I64, 10);
    let digit = builder.ins().urem(remaining, write_base);
    let digit = builder.ins().iadd_imm(digit, i64::from(b'0'));
    let digit = builder.ins().ireduce(types::I8, digit);
    let position = builder.ins().iadd_imm(position, -1);
    let destination = builder.ins().iadd(allocation.output, position);
    builder
        .ins()
        .store(MemFlagsData::new(), digit, destination, 0);
    let remaining = builder.ins().udiv(remaining, write_base);
    let done = builder.ins().icmp_imm(IntCC::Equal, remaining, 0);
    builder.ins().brif(
        done,
        finish,
        &[],
        write_next,
        &[remaining.into(), position.into()],
    );

    builder.switch_to_block(write_next);
    let remaining = builder.block_params(write_next)[0];
    let position = builder.block_params(write_next)[1];
    builder
        .ins()
        .jump(write_digits, &[remaining.into(), position.into()]);

    builder.switch_to_block(finish);
    lower_optimizing_finish_string(builder, allocation, length);
    Ok(allocation.value)
}

#[derive(Clone, Copy)]
struct NativeScalarStringCoercion {
    value: ir::Value,
    /// True when this coercion allocated the only owner of `value`. Consumers
    /// that merely inspect/copy its bytes must commit that owner themselves.
    temporary: ir::Value,
}

fn lower_total_native_scalar_string_coercion(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    encoded: ir::Value,
    fact: SsaValueFact,
    float_to_string: Option<NativeHelper>,
    string_cast: Option<NativeHelper>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<NativeScalarStringCoercion, CraneliftLoweringError> {
    let encoded = lower_optimizing_reference_scalar(builder, encoded, false, transition)?;
    let temporary = |builder: &mut FunctionBuilder<'_>, value, owned| NativeScalarStringCoercion {
        value,
        temporary: builder.ins().iconst(types::I8, i64::from(owned)),
    };
    match fact.class {
        SsaValueClass::Int => {
            let integer =
                lower_optimizing_authoritative_integer(builder, encoded, transition.deopt_out);
            let value = lower_optimizing_integer_to_string(builder, integer, transition)?;
            Ok(temporary(builder, value, true))
        }
        SsaValueClass::StringHandle => Ok(temporary(builder, encoded, false)),
        SsaValueClass::Null => {
            let length = builder.ins().iconst(types::I64, 0);
            let byte = builder.ins().iconst(types::I8, 0);
            let value = lower_optimizing_materialize_string(
                builder,
                length,
                NativeStringMaterialization::Byte { value: byte },
                transition,
            )?;
            Ok(temporary(builder, value, true))
        }
        SsaValueClass::Bool => {
            let is_true = builder.ins().icmp_imm(
                IntCC::Equal,
                encoded,
                crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
            );
            let length = builder.ins().uextend(types::I64, is_true);
            let byte = builder.ins().iconst(types::I8, i64::from(b'1'));
            let value = lower_optimizing_materialize_string(
                builder,
                length,
                NativeStringMaterialization::Byte { value: byte },
                transition,
            )?;
            Ok(temporary(builder, value, true))
        }
        SsaValueClass::Float => {
            let slot =
                lower_optimizing_slot_address(builder, encoded, transition.deopt_out);
            let bits = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
            );
            let value = builder
                .ins()
                .bitcast(types::F64, MemFlagsData::new(), bits);
            let value = emit_total_exact_scalar_value!(
                module,
                builder,
                float_to_string,
                &[value],
                transition,
                "exact native float-to-string handler was not declared",
            )?;
            Ok(temporary(builder, value, true))
        }
        _ => {
            let value = emit_total_exact_runtime_value!(
                module,
                builder,
                string_cast,
                &[encoded],
                transition,
                "exact native string-cast handler was not declared",
            )?;
            Ok(temporary(builder, value, true))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_optimizing_scalar_string_coercion(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    encoded: ir::Value,
    operand: RegionOperand,
    constants: &[IrConstant],
    float_to_string: Option<NativeHelper>,
    rejected: ir::Block,
    transition: NativeOptimizingTransition<'_>,
) -> Result<NativeScalarStringCoercion, CraneliftLoweringError> {
    let encoded = lower_optimizing_reference_scalar(builder, encoded, false, transition)?;
    let integer = builder.create_block();
    let inspect_string = builder.create_block();
    let string = builder.create_block();
    let inspect_immediate = builder.create_block();
    let true_string = builder.create_block();
    let empty_string = builder.create_block();
    let inspect_runtime = builder.create_block();
    let inspect_float_slot = builder.create_block();
    let float = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(integer, types::I64);
    builder.append_block_param(float, types::F64);
    builder.append_block_param(merge, types::I64);
    builder.append_block_param(merge, types::I8);

    let constant_integer = match operand {
        RegionOperand::Constant(index) => match constants.get(index as usize) {
            Some(IrConstant::Int(value)) => Some(*value),
            _ => None,
        },
        RegionOperand::Register(_)
        | RegionOperand::Local(_)
        | RegionOperand::LinkedConstant { .. }
        | RegionOperand::I64(_) => None,
    };
    let constant_float = match operand {
        RegionOperand::Constant(index) => match constants.get(index as usize) {
            Some(IrConstant::Float(value)) => Some(*value),
            _ => None,
        },
        RegionOperand::Register(_)
        | RegionOperand::Local(_)
        | RegionOperand::LinkedConstant { .. }
        | RegionOperand::I64(_) => None,
    };
    if let Some(value) = constant_integer {
        let value = builder.ins().iconst(types::I64, value);
        builder.ins().jump(integer, &[value.into()]);
    } else {
        let (is_integer, integer_value) =
            lower_optimizing_integer_candidate(builder, encoded, transition.deopt_out);
        builder.ins().brif(
            is_integer,
            integer,
            &[integer_value.into()],
            inspect_string,
            &[],
        );
    }

    builder.switch_to_block(integer);
    let integer_value = builder.block_params(integer)[0];
    let value = lower_optimizing_integer_to_string(builder, integer_value, transition)?;
    let temporary = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[value.into(), temporary.into()]);

    builder.switch_to_block(inspect_string);
    let (is_string, _, _) =
        lower_native_string_key_descriptor(builder, encoded, transition.deopt_out);
    builder
        .ins()
        .brif(is_string, string, &[], inspect_immediate, &[]);

    builder.switch_to_block(string);
    let borrowed = builder.ins().iconst(types::I8, 0);
    builder
        .ins()
        .jump(merge, &[encoded.into(), borrowed.into()]);

    builder.switch_to_block(inspect_immediate);
    if let Some(value) = constant_float {
        let value = builder
            .ins()
            .f64const(cranelift_codegen::ir::immediates::Ieee64::with_float(value));
        builder.ins().jump(float, &[value.into()]);
    } else {
        let is_true = builder.ins().icmp_imm(
            IntCC::Equal,
            encoded,
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        );
        let is_false = builder.ins().icmp_imm(
            IntCC::Equal,
            encoded,
            crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
        );
        let is_null =
            builder
                .ins()
                .icmp_imm(IntCC::Equal, encoded, crate::jit_encode_constant(u32::MAX));
        let false_or_null = builder.ins().bor(is_false, is_null);
        let classify_false = builder.create_block();
        builder
            .ins()
            .brif(is_true, true_string, &[], classify_false, &[]);
        builder.switch_to_block(classify_false);
        builder
            .ins()
            .brif(false_or_null, empty_string, &[], inspect_runtime, &[]);
    }

    builder.switch_to_block(true_string);
    let one_length = builder.ins().iconst(types::I64, 1);
    let one = builder.ins().iconst(types::I8, i64::from(b'1'));
    let value = lower_optimizing_materialize_string(
        builder,
        one_length,
        NativeStringMaterialization::Byte { value: one },
        transition,
    )?;
    let temporary = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[value.into(), temporary.into()]);

    builder.switch_to_block(empty_string);
    let zero_length = builder.ins().iconst(types::I64, 0);
    let zero = builder.ins().iconst(types::I8, 0);
    let value = lower_optimizing_materialize_string(
        builder,
        zero_length,
        NativeStringMaterialization::Byte { value: zero },
        transition,
    )?;
    let temporary = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[value.into(), temporary.into()]);

    builder.switch_to_block(inspect_runtime);
    let runtime = lower_is_runtime_handle(builder, encoded);
    let is_float = lower_value_has_tag(builder, encoded, crate::JIT_VALUE_RUNTIME_FLOAT_TAG);
    let index = builder.ins().ireduce(types::I32, encoded);
    let direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let admitted = builder.ins().band(runtime, is_float);
    let admitted = builder.ins().band(admitted, direct);
    builder
        .ins()
        .brif(admitted, inspect_float_slot, &[], rejected, &[]);

    builder.switch_to_block(inspect_float_slot);
    let slot = lower_optimizing_slot_address(builder, encoded, transition.deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let kind_matches = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_FLOAT),
    );
    let bits = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let value = builder.ins().bitcast(types::F64, MemFlagsData::new(), bits);
    builder
        .ins()
        .brif(kind_matches, float, &[value.into()], rejected, &[]);

    builder.switch_to_block(float);
    let value = builder.block_params(float)[0];
    let value = emit_total_exact_scalar_value!(
        module,
        builder,
        float_to_string,
        &[value],
        transition,
        "exact native float-to-string handler was not declared",
    )?;
    let temporary = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[value.into(), temporary.into()]);

    builder.switch_to_block(merge);
    Ok(NativeScalarStringCoercion {
        value: builder.block_params(merge)[0],
        temporary: builder.block_params(merge)[1],
    })
}

fn lower_native_byte_slice_equal(
    builder: &mut FunctionBuilder<'_>,
    lhs: ir::Value,
    rhs: ir::Value,
    length: ir::Value,
) -> ir::Value {
    let compare = builder.create_block();
    let compare_byte = builder.create_block();
    let next = builder.create_block();
    let matched = builder.create_block();
    let different = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(compare, types::I64);
    builder.append_block_param(next, types::I64);
    builder.append_block_param(merge, types::I8);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(compare, &[zero.into()]);

    builder.switch_to_block(compare);
    let index = builder.block_params(compare)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder
        .ins()
        .brif(exhausted, matched, &[], compare_byte, &[]);

    builder.switch_to_block(compare_byte);
    let lhs_at = builder.ins().iadd(lhs, index);
    let rhs_at = builder.ins().iadd(rhs, index);
    let lhs_byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), lhs_at, 0);
    let rhs_byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), rhs_at, 0);
    let equal = builder.ins().icmp(IntCC::Equal, lhs_byte, rhs_byte);
    builder
        .ins()
        .brif(equal, next, &[index.into()], different, &[]);

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let next_index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(compare, &[next_index.into()]);

    builder.switch_to_block(matched);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().jump(merge, &[yes.into()]);
    builder.switch_to_block(different);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().jump(merge, &[no.into()]);
    builder.switch_to_block(merge);
    builder.block_params(merge)[0]
}

fn lower_optimizing_explode(
    builder: &mut FunctionBuilder<'_>,
    delimiter: ir::Value,
    input: ir::Value,
    piece_limit: Option<u64>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (delimiter_length, delimiter_bytes) =
        lower_optimizing_string_descriptor(builder, delimiter, transition)?;
    let (input_length, input_bytes) =
        lower_optimizing_string_descriptor(builder, input, transition)?;
    let scan = builder.create_block();
    let inspect = builder.create_block();
    let matched = builder.create_block();
    let different = builder.create_block();
    let counted = builder.create_block();
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(matched, types::I64);
    builder.append_block_param(matched, types::I64);
    builder.append_block_param(different, types::I64);
    builder.append_block_param(different, types::I64);
    builder.append_block_param(counted, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    let one = builder.ins().iconst(types::I64, 1);
    builder.ins().jump(scan, &[zero.into(), one.into()]);

    builder.switch_to_block(scan);
    let position = builder.block_params(scan)[0];
    let count = builder.block_params(scan)[1];
    let end = builder.ins().iadd(position, delimiter_length);
    let has_candidate = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, end, input_length);
    builder
        .ins()
        .brif(has_candidate, inspect, &[], counted, &[count.into()]);

    builder.switch_to_block(inspect);
    let candidate = builder.ins().iadd(input_bytes, position);
    let equal =
        lower_native_byte_slice_equal(builder, candidate, delimiter_bytes, delimiter_length);
    builder.ins().brif(
        equal,
        matched,
        &[position.into(), count.into()],
        different,
        &[position.into(), count.into()],
    );

    builder.switch_to_block(matched);
    let position = builder.block_params(matched)[0];
    let count = builder.block_params(matched)[1];
    let next_position = builder.ins().iadd(position, delimiter_length);
    let next_count = builder.ins().iadd_imm(count, 1);
    builder
        .ins()
        .jump(scan, &[next_position.into(), next_count.into()]);

    builder.switch_to_block(different);
    let position = builder.block_params(different)[0];
    let count = builder.block_params(different)[1];
    let next_position = builder.ins().iadd_imm(position, 1);
    builder
        .ins()
        .jump(scan, &[next_position.into(), count.into()]);

    builder.switch_to_block(counted);
    let count = builder.block_params(counted)[0];
    let count = if let Some(piece_limit) = piece_limit {
        let limit = builder.ins().iconst(types::I64, piece_limit as i64);
        let exceeds_limit = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThan, count, limit);
        builder.ins().select(exceeds_limit, limit, count)
    } else {
        count
    };
    let required_slots = builder.ins().iadd_imm(count, 1);
    lower_optimizing_require_direct_value_capacity(builder, required_slots, transition)?;
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let result = lower_optimizing_allocate_direct_array(builder, count, false, transition)?;
    let result_slot = lower_optimizing_slot_address(builder, result, transition.deopt_out);
    let result_entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        result_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let pieces = builder.create_block();
    let find_end = builder.create_block();
    let inspect_end = builder.create_block();
    let found_end = builder.create_block();
    let advance_end = builder.create_block();
    let materialize = builder.create_block();
    let finish = builder.create_block();
    for block in [pieces, find_end, inspect_end, found_end, advance_end] {
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I64);
    }
    builder.append_block_param(find_end, types::I64);
    builder.append_block_param(inspect_end, types::I64);
    builder.append_block_param(found_end, types::I64);
    builder.append_block_param(advance_end, types::I64);
    builder.append_block_param(materialize, types::I64);
    builder.append_block_param(materialize, types::I64);
    builder.append_block_param(materialize, types::I64);
    builder.ins().jump(pieces, &[zero.into(), zero.into()]);

    builder.switch_to_block(pieces);
    let piece_index = builder.block_params(pieces)[0];
    let start = builder.block_params(pieces)[1];
    let done = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, piece_index, count);
    let last_index = builder.ins().iadd_imm(count, -1);
    let last = builder.ins().icmp(IntCC::Equal, piece_index, last_index);
    let choose = builder.create_block();
    builder.ins().brif(done, finish, &[], choose, &[]);
    builder.switch_to_block(choose);
    builder.ins().brif(
        last,
        materialize,
        &[piece_index.into(), start.into(), input_length.into()],
        find_end,
        &[piece_index.into(), start.into(), start.into()],
    );

    builder.switch_to_block(find_end);
    let piece_index = builder.block_params(find_end)[0];
    let start = builder.block_params(find_end)[1];
    let candidate_end = builder.block_params(find_end)[2];
    let delimiter_end = builder.ins().iadd(candidate_end, delimiter_length);
    let in_bounds = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, delimiter_end, input_length);
    builder.ins().brif(
        in_bounds,
        inspect_end,
        &[piece_index.into(), start.into(), candidate_end.into()],
        materialize,
        &[piece_index.into(), start.into(), input_length.into()],
    );

    builder.switch_to_block(inspect_end);
    let piece_index = builder.block_params(inspect_end)[0];
    let start = builder.block_params(inspect_end)[1];
    let candidate_end = builder.block_params(inspect_end)[2];
    let candidate = builder.ins().iadd(input_bytes, candidate_end);
    let equal =
        lower_native_byte_slice_equal(builder, candidate, delimiter_bytes, delimiter_length);
    builder.ins().brif(
        equal,
        found_end,
        &[piece_index.into(), start.into(), candidate_end.into()],
        advance_end,
        &[piece_index.into(), start.into(), candidate_end.into()],
    );

    builder.switch_to_block(found_end);
    let piece_index = builder.block_params(found_end)[0];
    let start = builder.block_params(found_end)[1];
    let end = builder.block_params(found_end)[2];
    builder
        .ins()
        .jump(materialize, &[piece_index.into(), start.into(), end.into()]);

    builder.switch_to_block(advance_end);
    let piece_index = builder.block_params(advance_end)[0];
    let start = builder.block_params(advance_end)[1];
    let candidate_end = builder.block_params(advance_end)[2];
    let next_candidate = builder.ins().iadd_imm(candidate_end, 1);
    builder.ins().jump(
        find_end,
        &[piece_index.into(), start.into(), next_candidate.into()],
    );

    builder.switch_to_block(materialize);
    let piece_index = builder.block_params(materialize)[0];
    let start = builder.block_params(materialize)[1];
    let end = builder.block_params(materialize)[2];
    let piece_length = builder.ins().isub(end, start);
    let source = builder.ins().iadd(input_bytes, start);
    let value = lower_optimizing_materialize_string(
        builder,
        piece_length,
        NativeStringMaterialization::Copy { source },
        transition,
    )?;
    let entry_index = if pointer_type == types::I64 {
        piece_index
    } else {
        builder.ins().ireduce(pointer_type, piece_index)
    };
    let entry_offset = builder.ins().ishl_imm(entry_index, 4);
    let entry = builder.ins().iadd(result_entries, entry_offset);
    builder
        .ins()
        .store(MemFlagsData::new(), piece_index, entry, 0);
    builder.ins().store(
        MemFlagsData::new(),
        value,
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let next_piece = builder.ins().iadd_imm(piece_index, 1);
    let next_start = builder.ins().iadd(end, delimiter_length);
    builder
        .ins()
        .jump(pieces, &[next_piece.into(), next_start.into()]);

    builder.switch_to_block(finish);
    builder.ins().store(
        MemFlagsData::new(),
        count,
        result_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    Ok(result)
}

fn lower_optimizing_implode(
    builder: &mut FunctionBuilder<'_>,
    separator: Option<ir::Value>,
    array: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (separator_length, separator_bytes) = if let Some(separator) = separator {
        lower_optimizing_string_descriptor(builder, separator, transition)?
    } else {
        let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
        (
            builder.ins().iconst(types::I64, 0),
            builder.ins().iconst(pointer_type, 0),
        )
    };
    let (_, length, entries) =
        lower_optimizing_direct_array_descriptor(builder, array, transition)?;
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let preflight = builder.create_block();
    let inspect_value = builder.create_block();
    let preflight_next = builder.create_block();
    let allocate = builder.create_block();
    builder.append_block_param(preflight, types::I64);
    builder.append_block_param(preflight, types::I64);
    builder.append_block_param(preflight_next, types::I64);
    builder.append_block_param(preflight_next, types::I64);
    builder.append_block_param(allocate, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(preflight, &[zero.into(), zero.into()]);

    builder.switch_to_block(preflight);
    let index = builder.block_params(preflight)[0];
    let total = builder.block_params(preflight)[1];
    let done = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder
        .ins()
        .brif(done, allocate, &[total.into()], inspect_value, &[]);

    builder.switch_to_block(inspect_value);
    let pointer_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let entry_offset = builder.ins().ishl_imm(pointer_index, 4);
    let entry = builder.ins().iadd(entries, entry_offset);
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let value = lower_optimizing_admitted_reference_scalar(
        builder,
        value,
        transition.deopt_out,
        transition.reference_payload_proof,
    );
    let (_, value_length, _) =
        lower_native_string_key_descriptor(builder, value, transition.deopt_out);
    let first = builder.ins().icmp_imm(IntCC::Equal, index, 0);
    let separator_addition = builder.ins().select(first, zero, separator_length);
    let separated = builder.ins().iadd(total, separator_addition);
    let next_total = builder.ins().iadd(separated, value_length);
    builder
        .ins()
        .jump(preflight_next, &[index.into(), next_total.into()]);

    builder.switch_to_block(preflight_next);
    let index = builder.block_params(preflight_next)[0];
    let total = builder.block_params(preflight_next)[1];
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(preflight, &[next.into(), total.into()]);

    builder.switch_to_block(allocate);
    let total = builder.block_params(allocate)[0];
    let allocation = lower_optimizing_allocate_string(builder, total, transition)?;
    let copy_piece = builder.create_block();
    let choose_separator = builder.create_block();
    let copy_separator = builder.create_block();
    let store_separator = builder.create_block();
    let start_value = builder.create_block();
    let copy_value = builder.create_block();
    let store_value = builder.create_block();
    let next_piece = builder.create_block();
    let finish = builder.create_block();
    for block in [copy_piece, choose_separator, start_value, next_piece] {
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I64);
    }
    for block in [copy_separator, store_separator] {
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I64);
    }
    for block in [copy_value, store_value] {
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, pointer_type);
    }
    builder.ins().jump(copy_piece, &[zero.into(), zero.into()]);

    builder.switch_to_block(copy_piece);
    let index = builder.block_params(copy_piece)[0];
    let output_index = builder.block_params(copy_piece)[1];
    let done = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(
        done,
        finish,
        &[],
        choose_separator,
        &[index.into(), output_index.into()],
    );

    builder.switch_to_block(choose_separator);
    let index = builder.block_params(choose_separator)[0];
    let output_index = builder.block_params(choose_separator)[1];
    let first = builder.ins().icmp_imm(IntCC::Equal, index, 0);
    builder.ins().brif(
        first,
        start_value,
        &[index.into(), output_index.into()],
        copy_separator,
        &[index.into(), output_index.into(), zero.into()],
    );

    builder.switch_to_block(copy_separator);
    let index = builder.block_params(copy_separator)[0];
    let output_index = builder.block_params(copy_separator)[1];
    let separator_index = builder.block_params(copy_separator)[2];
    let copied = builder.ins().icmp(
        IntCC::UnsignedGreaterThanOrEqual,
        separator_index,
        separator_length,
    );
    builder.ins().brif(
        copied,
        start_value,
        &[index.into(), output_index.into()],
        store_separator,
        &[index.into(), output_index.into(), separator_index.into()],
    );

    builder.switch_to_block(store_separator);
    let index = builder.block_params(store_separator)[0];
    let output_index = builder.block_params(store_separator)[1];
    let separator_index = builder.block_params(store_separator)[2];
    let source = builder.ins().iadd(separator_bytes, separator_index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    let destination = builder.ins().iadd(allocation.output, output_index);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next_output = builder.ins().iadd_imm(output_index, 1);
    let next_separator = builder.ins().iadd_imm(separator_index, 1);
    builder.ins().jump(
        copy_separator,
        &[index.into(), next_output.into(), next_separator.into()],
    );

    builder.switch_to_block(start_value);
    let index = builder.block_params(start_value)[0];
    let output_index = builder.block_params(start_value)[1];
    let pointer_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let entry_offset = builder.ins().ishl_imm(pointer_index, 4);
    let entry = builder.ins().iadd(entries, entry_offset);
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let value = lower_optimizing_admitted_reference_scalar(
        builder,
        value,
        transition.deopt_out,
        transition.reference_payload_proof,
    );
    let (_, value_length, value_bytes) =
        lower_native_string_key_descriptor(builder, value, transition.deopt_out);
    builder.ins().jump(
        copy_value,
        &[
            index.into(),
            output_index.into(),
            zero.into(),
            value_length.into(),
            value_bytes.into(),
        ],
    );

    builder.switch_to_block(copy_value);
    let index = builder.block_params(copy_value)[0];
    let output_index = builder.block_params(copy_value)[1];
    let value_index = builder.block_params(copy_value)[2];
    let value_length = builder.block_params(copy_value)[3];
    let value_bytes = builder.block_params(copy_value)[4];
    let copied = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, value_index, value_length);
    builder.ins().brif(
        copied,
        next_piece,
        &[index.into(), output_index.into()],
        store_value,
        &[
            index.into(),
            output_index.into(),
            value_index.into(),
            value_length.into(),
            value_bytes.into(),
        ],
    );

    builder.switch_to_block(store_value);
    let index = builder.block_params(store_value)[0];
    let output_index = builder.block_params(store_value)[1];
    let value_index = builder.block_params(store_value)[2];
    let value_length = builder.block_params(store_value)[3];
    let value_bytes = builder.block_params(store_value)[4];
    let source = builder.ins().iadd(value_bytes, value_index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    let destination = builder.ins().iadd(allocation.output, output_index);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next_output = builder.ins().iadd_imm(output_index, 1);
    let next_value = builder.ins().iadd_imm(value_index, 1);
    builder.ins().jump(
        copy_value,
        &[
            index.into(),
            next_output.into(),
            next_value.into(),
            value_length.into(),
            value_bytes.into(),
        ],
    );

    builder.switch_to_block(next_piece);
    let index = builder.block_params(next_piece)[0];
    let output_index = builder.block_params(next_piece)[1];
    let next = builder.ins().iadd_imm(index, 1);
    builder
        .ins()
        .jump(copy_piece, &[next.into(), output_index.into()]);

    builder.switch_to_block(finish);
    lower_optimizing_finish_string(builder, allocation, total);
    Ok(allocation.value)
}

fn lower_optimizing_str_replace(
    builder: &mut FunctionBuilder<'_>,
    search: ir::Value,
    replacement: ir::Value,
    subject: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<(ir::Value, ir::Value), CraneliftLoweringError> {
    let (search_length, search_bytes) =
        lower_optimizing_string_descriptor(builder, search, transition)?;
    let (replacement_length, replacement_bytes) =
        lower_optimizing_string_descriptor(builder, replacement, transition)?;
    let (subject_length, subject_bytes) =
        lower_optimizing_string_descriptor(builder, subject, transition)?;
    let empty_search = builder.create_block();
    let scan = builder.create_block();
    let inspect = builder.create_block();
    let matched = builder.create_block();
    let different = builder.create_block();
    let counted = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(matched, types::I64);
    builder.append_block_param(matched, types::I64);
    builder.append_block_param(matched, types::I64);
    builder.append_block_param(different, types::I64);
    builder.append_block_param(different, types::I64);
    builder.append_block_param(different, types::I64);
    builder.append_block_param(counted, types::I64);
    builder.append_block_param(counted, types::I64);
    builder.append_block_param(counted, types::I64);
    builder.append_block_param(merge, types::I64);
    builder.append_block_param(merge, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    let search_empty = builder.ins().icmp_imm(IntCC::Equal, search_length, 0);
    builder.ins().brif(
        search_empty,
        empty_search,
        &[],
        scan,
        &[zero.into(), zero.into(), zero.into()],
    );

    builder.switch_to_block(empty_search);
    let unchanged = lower_optimizing_materialize_string(
        builder,
        subject_length,
        NativeStringMaterialization::Copy {
            source: subject_bytes,
        },
        transition,
    )?;
    builder
        .ins()
        .jump(merge, &[unchanged.into(), zero.into()]);

    builder.switch_to_block(scan);
    let position = builder.block_params(scan)[0];
    let output_length = builder.block_params(scan)[1];
    let replacement_count = builder.block_params(scan)[2];
    let end = builder.ins().iadd(position, search_length);
    let has_candidate = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, end, subject_length);
    builder.ins().brif(
        has_candidate,
        inspect,
        &[],
        counted,
        &[
            position.into(),
            output_length.into(),
            replacement_count.into(),
        ],
    );

    builder.switch_to_block(inspect);
    let candidate = builder.ins().iadd(subject_bytes, position);
    let equal = lower_native_byte_slice_equal(builder, candidate, search_bytes, search_length);
    builder.ins().brif(
        equal,
        matched,
        &[
            position.into(),
            output_length.into(),
            replacement_count.into(),
        ],
        different,
        &[
            position.into(),
            output_length.into(),
            replacement_count.into(),
        ],
    );

    builder.switch_to_block(matched);
    let position = builder.block_params(matched)[0];
    let output_length = builder.block_params(matched)[1];
    let replacement_count = builder.block_params(matched)[2];
    let next_output = builder.ins().iadd(output_length, replacement_length);
    let next_position = builder.ins().iadd(position, search_length);
    let next_count = builder.ins().iadd_imm(replacement_count, 1);
    builder.ins().jump(
        scan,
        &[next_position.into(), next_output.into(), next_count.into()],
    );

    builder.switch_to_block(different);
    let position = builder.block_params(different)[0];
    let output_length = builder.block_params(different)[1];
    let replacement_count = builder.block_params(different)[2];
    let next_position = builder.ins().iadd_imm(position, 1);
    let next_output = builder.ins().iadd_imm(output_length, 1);
    builder.ins().jump(
        scan,
        &[
            next_position.into(),
            next_output.into(),
            replacement_count.into(),
        ],
    );

    builder.switch_to_block(counted);
    let position = builder.block_params(counted)[0];
    let output_length = builder.block_params(counted)[1];
    let replacement_count = builder.block_params(counted)[2];
    // Once no complete search string can begin, the remaining suffix is
    // literal output and is included in the one final allocation.
    let suffix_length = builder.ins().isub(subject_length, position);
    let output_length = builder.ins().iadd(output_length, suffix_length);
    let allocation = lower_optimizing_allocate_string(builder, output_length, transition)?;
    let copy = builder.create_block();
    let inspect_copy = builder.create_block();
    let copy_match = builder.create_block();
    let copy_replacement = builder.create_block();
    let store_replacement = builder.create_block();
    let copy_different = builder.create_block();
    let finish = builder.create_block();
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(copy_match, types::I64);
    builder.append_block_param(copy_match, types::I64);
    for block in [copy_replacement, store_replacement] {
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I64);
    }
    builder.append_block_param(copy_different, types::I64);
    builder.append_block_param(copy_different, types::I64);
    builder.ins().jump(copy, &[zero.into(), zero.into()]);

    builder.switch_to_block(copy);
    let position = builder.block_params(copy)[0];
    let output_index = builder.block_params(copy)[1];
    let done = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, position, subject_length);
    builder.ins().brif(done, finish, &[], inspect_copy, &[]);

    builder.switch_to_block(inspect_copy);
    let end = builder.ins().iadd(position, search_length);
    let has_candidate = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, end, subject_length);
    let compare_candidate = builder.create_block();
    builder.ins().brif(
        has_candidate,
        compare_candidate,
        &[],
        copy_different,
        &[position.into(), output_index.into()],
    );
    builder.switch_to_block(compare_candidate);
    let candidate = builder.ins().iadd(subject_bytes, position);
    let equal = lower_native_byte_slice_equal(builder, candidate, search_bytes, search_length);
    builder.ins().brif(
        equal,
        copy_match,
        &[position.into(), output_index.into()],
        copy_different,
        &[position.into(), output_index.into()],
    );

    builder.switch_to_block(copy_match);
    let position = builder.block_params(copy_match)[0];
    let output_index = builder.block_params(copy_match)[1];
    builder.ins().jump(
        copy_replacement,
        &[position.into(), output_index.into(), zero.into()],
    );

    builder.switch_to_block(copy_replacement);
    let position = builder.block_params(copy_replacement)[0];
    let output_index = builder.block_params(copy_replacement)[1];
    let replacement_index = builder.block_params(copy_replacement)[2];
    let copied = builder.ins().icmp(
        IntCC::UnsignedGreaterThanOrEqual,
        replacement_index,
        replacement_length,
    );
    let replacement_done = builder.create_block();
    builder.ins().brif(
        copied,
        replacement_done,
        &[],
        store_replacement,
        &[
            position.into(),
            output_index.into(),
            replacement_index.into(),
        ],
    );
    builder.switch_to_block(replacement_done);
    let next_position = builder.ins().iadd(position, search_length);
    builder
        .ins()
        .jump(copy, &[next_position.into(), output_index.into()]);

    builder.switch_to_block(store_replacement);
    let position = builder.block_params(store_replacement)[0];
    let output_index = builder.block_params(store_replacement)[1];
    let replacement_index = builder.block_params(store_replacement)[2];
    let source = builder.ins().iadd(replacement_bytes, replacement_index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    let destination = builder.ins().iadd(allocation.output, output_index);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next_output = builder.ins().iadd_imm(output_index, 1);
    let next_replacement = builder.ins().iadd_imm(replacement_index, 1);
    builder.ins().jump(
        copy_replacement,
        &[position.into(), next_output.into(), next_replacement.into()],
    );

    builder.switch_to_block(copy_different);
    let position = builder.block_params(copy_different)[0];
    let output_index = builder.block_params(copy_different)[1];
    let source = builder.ins().iadd(subject_bytes, position);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    let destination = builder.ins().iadd(allocation.output, output_index);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next_position = builder.ins().iadd_imm(position, 1);
    let next_output = builder.ins().iadd_imm(output_index, 1);
    builder
        .ins()
        .jump(copy, &[next_position.into(), next_output.into()]);

    builder.switch_to_block(finish);
    lower_optimizing_finish_string(builder, allocation, output_length);
    builder.ins().jump(
        merge,
        &[allocation.value.into(), replacement_count.into()],
    );

    builder.switch_to_block(merge);
    Ok((
        builder.block_params(merge)[0],
        builder.block_params(merge)[1],
    ))
}

fn lower_optimizing_chr(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let value = lower_optimizing_require_immediate_integer(builder, value, transition)?;
    let byte = builder.ins().ireduce(types::I8, value);
    let one = builder.ins().iconst(types::I64, 1);
    lower_optimizing_materialize_string(
        builder,
        one,
        NativeStringMaterialization::Byte { value: byte },
        transition,
    )
}

fn lower_trim_default_byte(builder: &mut FunctionBuilder<'_>, byte: ir::Value) -> ir::Value {
    let mut matched = builder.ins().icmp_imm(IntCC::Equal, byte, 0);
    for candidate in [9_u8, 10, 11, 13, 32] {
        let candidate = builder
            .ins()
            .icmp_imm(IntCC::Equal, byte, i64::from(candidate));
        matched = builder.ins().bor(matched, candidate);
    }
    matched
}

fn lower_optimizing_default_trim(
    builder: &mut FunctionBuilder<'_>,
    operation: StableDefaultTrimBuiltin,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (length, bytes) = lower_optimizing_string_descriptor(builder, value, transition)?;
    let left = builder.create_block();
    let inspect_left = builder.create_block();
    let left_done = builder.create_block();
    let right = builder.create_block();
    let inspect_right = builder.create_block();
    let bounds = builder.create_block();
    builder.append_block_param(left, types::I64);
    builder.append_block_param(left_done, types::I64);
    builder.append_block_param(right, types::I64);
    builder.append_block_param(right, types::I64);
    builder.append_block_param(bounds, types::I64);
    builder.append_block_param(bounds, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    if !operation.trims_left() {
        builder.ins().jump(right, &[zero.into(), length.into()]);
    } else {
        builder.ins().jump(left, &[zero.into()]);
    }

    builder.switch_to_block(left);
    let start = builder.block_params(left)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, start, length);
    builder
        .ins()
        .brif(exhausted, left_done, &[start.into()], inspect_left, &[]);

    builder.switch_to_block(inspect_left);
    let at = builder.ins().iadd(bytes, start);
    let byte = builder.ins().load(types::I8, MemFlagsData::new(), at, 0);
    let trim = lower_trim_default_byte(builder, byte);
    let next = builder.ins().iadd_imm(start, 1);
    builder
        .ins()
        .brif(trim, left, &[next.into()], left_done, &[start.into()]);

    builder.switch_to_block(left_done);
    let start = builder.block_params(left_done)[0];
    if !operation.trims_right() {
        builder.ins().jump(bounds, &[start.into(), length.into()]);
    } else {
        builder.ins().jump(right, &[start.into(), length.into()]);
    }

    builder.switch_to_block(right);
    let start = builder.block_params(right)[0];
    let end = builder.block_params(right)[1];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, end, start);
    builder.ins().brif(
        exhausted,
        bounds,
        &[start.into(), end.into()],
        inspect_right,
        &[],
    );

    builder.switch_to_block(inspect_right);
    let previous = builder.ins().iadd_imm(end, -1);
    let at = builder.ins().iadd(bytes, previous);
    let byte = builder.ins().load(types::I8, MemFlagsData::new(), at, 0);
    let trim = lower_trim_default_byte(builder, byte);
    builder.ins().brif(
        trim,
        right,
        &[start.into(), previous.into()],
        bounds,
        &[start.into(), end.into()],
    );

    builder.switch_to_block(bounds);
    let start = builder.block_params(bounds)[0];
    let end = builder.block_params(bounds)[1];
    let slice_length = builder.ins().isub(end, start);
    let source = builder.ins().iadd(bytes, start);
    lower_optimizing_materialize_string(
        builder,
        slice_length,
        NativeStringMaterialization::Copy { source },
        transition,
    )
}

fn lower_optimizing_substr(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    offset: ir::Value,
    requested_length: Option<ir::Value>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (length, bytes) = lower_optimizing_string_descriptor(builder, value, transition)?;
    let offset = lower_optimizing_require_immediate_integer(builder, offset, transition)?;
    let negative = builder.ins().icmp_imm(IntCC::SignedLessThan, offset, 0);
    let magnitude = builder.ins().ineg(offset);
    let negative_fits = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, magnitude, length);
    let negative_start = builder.ins().isub(length, magnitude);
    let zero = builder.ins().iconst(types::I64, 0);
    let negative_start = builder.ins().select(negative_fits, negative_start, zero);
    let positive_fits = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, offset, length);
    let positive_start = builder.ins().select(positive_fits, offset, length);
    let start = builder
        .ins()
        .select(negative, negative_start, positive_start);
    let remaining = builder.ins().isub(length, start);
    let slice_length = if let Some(requested) = requested_length {
        let is_null = builder.ins().icmp_imm(
            IntCC::Equal,
            requested,
            crate::jit_encode_constant(u32::MAX),
        );
        let (_, requested_raw) =
            lower_optimizing_integer_candidate(builder, requested, transition.deopt_out);
        let requested_negative = builder
            .ins()
            .icmp_imm(IntCC::SignedLessThan, requested_raw, 0);
        let requested_fits =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, requested_raw, remaining);
        let positive = builder
            .ins()
            .select(requested_fits, requested_raw, remaining);
        let requested_magnitude = builder.ins().ineg(requested_raw);
        let magnitude_fits =
            builder
                .ins()
                .icmp(IntCC::UnsignedLessThanOrEqual, requested_magnitude, length);
        let end = builder.ins().isub(length, requested_magnitude);
        let end = builder.ins().select(magnitude_fits, end, zero);
        let end_after_start = builder.ins().icmp(IntCC::UnsignedGreaterThan, end, start);
        let negative_length = builder.ins().isub(end, start);
        let negative_length = builder.ins().select(end_after_start, negative_length, zero);
        let specified = builder
            .ins()
            .select(requested_negative, negative_length, positive);
        builder.ins().select(is_null, remaining, specified)
    } else {
        remaining
    };
    let source = builder.ins().iadd(bytes, start);
    lower_optimizing_materialize_string(
        builder,
        slice_length,
        NativeStringMaterialization::Copy { source },
        transition,
    )
}

fn lower_optimizing_ascii_case(
    builder: &mut FunctionBuilder<'_>,
    operation: StableAsciiCaseBuiltin,
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
    let allocation = lower_optimizing_allocate_string(builder, length, transition)?;
    let output = allocation.output;
    let copy = builder.create_block();
    let copy_byte = builder.create_block();
    let finish = builder.create_block();
    builder.append_block_param(copy, types::I64);
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
    let lowercase = matches!(operation, StableAsciiCaseBuiltin::Lower);
    let lower_bound = if lowercase { b'A' } else { b'a' };
    let upper_bound = if lowercase { b'Z' } else { b'z' };
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
    let converted = if lowercase {
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
    lower_optimizing_finish_string(builder, allocation, length);
    Ok(allocation.value)
}

fn lower_optimizing_string_transform(
    builder: &mut FunctionBuilder<'_>,
    operation: StableStringTransformBuiltin,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (length, input) = lower_optimizing_string_descriptor(builder, value, transition)?;
    let allocation = lower_optimizing_allocate_string(builder, length, transition)?;
    let copy = builder.create_block();
    let copy_byte = builder.create_block();
    let finish = builder.create_block();
    builder.append_block_param(copy, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(copy, &[zero.into()]);

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let done = builder.ins().icmp(IntCC::Equal, index, length);
    builder.ins().brif(done, finish, &[], copy_byte, &[]);

    builder.switch_to_block(copy_byte);
    let source_index = if matches!(operation, StableStringTransformBuiltin::Reverse) {
        let last = builder.ins().iadd_imm(length, -1);
        builder.ins().isub(last, index)
    } else {
        index
    };
    let source = builder.ins().iadd(input, source_index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    let byte = if !matches!(operation, StableStringTransformBuiltin::Reverse) {
        let first = builder.ins().icmp_imm(IntCC::Equal, index, 0);
        let lowercase_first = matches!(operation, StableStringTransformBuiltin::LowercaseFirst);
        let lower = if lowercase_first { b'A' } else { b'a' };
        let upper = if lowercase_first { b'Z' } else { b'z' };
        let at_least =
            builder
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, byte, i64::from(lower));
        let at_most =
            builder
                .ins()
                .icmp_imm(IntCC::UnsignedLessThanOrEqual, byte, i64::from(upper));
        let in_range = builder.ins().band(at_least, at_most);
        let convert = builder.ins().band(first, in_range);
        let converted = if lowercase_first {
            builder.ins().iadd_imm(byte, 32)
        } else {
            builder.ins().iadd_imm(byte, -32)
        };
        builder.ins().select(convert, converted, byte)
    } else {
        byte
    };
    let destination = builder.ins().iadd(allocation.output, index);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(copy, &[next.into()]);

    builder.switch_to_block(finish);
    lower_optimizing_finish_string(builder, allocation, length);
    Ok(allocation.value)
}

fn lower_optimizing_str_repeat(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    count: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (length, input) = lower_optimizing_string_descriptor(builder, value, transition)?;
    let count = lower_optimizing_require_immediate_integer(builder, count, transition)?;
    let output_length = builder.ins().imul(length, count);
    let allocation = lower_optimizing_allocate_string(builder, output_length, transition)?;
    let copy = builder.create_block();
    let copy_byte = builder.create_block();
    let finish = builder.create_block();
    builder.append_block_param(copy, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(copy, &[zero.into()]);
    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let done = builder.ins().icmp(IntCC::Equal, index, output_length);
    builder.ins().brif(done, finish, &[], copy_byte, &[]);
    builder.switch_to_block(copy_byte);
    // Reaching this block proves both the input and output are non-empty.
    let source_index = builder.ins().urem(index, length);
    let source = builder.ins().iadd(input, source_index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    let destination = builder.ins().iadd(allocation.output, index);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(copy, &[next.into()]);
    builder.switch_to_block(finish);
    lower_optimizing_finish_string(builder, allocation, output_length);
    Ok(allocation.value)
}

fn lower_optimizing_addslashes(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (length, input) = lower_optimizing_string_descriptor(builder, value, transition)?;
    let count = builder.create_block();
    let count_byte = builder.create_block();
    let count_next = builder.create_block();
    let allocate = builder.create_block();
    builder.append_block_param(count, types::I64);
    builder.append_block_param(count, types::I64);
    builder.append_block_param(count_next, types::I64);
    builder.append_block_param(count_next, types::I64);
    builder.append_block_param(allocate, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(count, &[zero.into(), zero.into()]);

    builder.switch_to_block(count);
    let index = builder.block_params(count)[0];
    let extra = builder.block_params(count)[1];
    let exhausted = builder.ins().icmp(IntCC::Equal, index, length);
    builder
        .ins()
        .brif(exhausted, allocate, &[extra.into()], count_byte, &[]);

    builder.switch_to_block(count_byte);
    let at = builder.ins().iadd(input, index);
    let byte = builder.ins().load(types::I8, MemFlagsData::new(), at, 0);
    let single = builder.ins().icmp_imm(IntCC::Equal, byte, i64::from(b'\''));
    let double = builder.ins().icmp_imm(IntCC::Equal, byte, i64::from(b'"'));
    let slash = builder.ins().icmp_imm(IntCC::Equal, byte, i64::from(b'\\'));
    let nul = builder.ins().icmp_imm(IntCC::Equal, byte, 0);
    let escaped = builder.ins().bor(single, double);
    let escaped = builder.ins().bor(escaped, slash);
    let escaped = builder.ins().bor(escaped, nul);
    let escaped = builder.ins().uextend(types::I64, escaped);
    let extra = builder.ins().iadd(extra, escaped);
    builder
        .ins()
        .jump(count_next, &[index.into(), extra.into()]);

    builder.switch_to_block(count_next);
    let index = builder.block_params(count_next)[0];
    let extra = builder.block_params(count_next)[1];
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(count, &[next.into(), extra.into()]);

    builder.switch_to_block(allocate);
    let extra = builder.block_params(allocate)[0];
    let output_length = builder.ins().iadd(length, extra);
    let allocation = lower_optimizing_allocate_string(builder, output_length, transition)?;
    let copy = builder.create_block();
    let copy_byte = builder.create_block();
    let write_escape = builder.create_block();
    let write_plain = builder.create_block();
    let copy_next = builder.create_block();
    let finish = builder.create_block();
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(write_escape, types::I64);
    builder.append_block_param(write_escape, types::I64);
    builder.append_block_param(write_escape, types::I8);
    builder.append_block_param(write_plain, types::I64);
    builder.append_block_param(write_plain, types::I64);
    builder.append_block_param(write_plain, types::I8);
    builder.append_block_param(copy_next, types::I64);
    builder.append_block_param(copy_next, types::I64);
    builder.ins().jump(copy, &[zero.into(), zero.into()]);

    builder.switch_to_block(copy);
    let input_index = builder.block_params(copy)[0];
    let output_index = builder.block_params(copy)[1];
    let exhausted = builder.ins().icmp(IntCC::Equal, input_index, length);
    builder.ins().brif(exhausted, finish, &[], copy_byte, &[]);

    builder.switch_to_block(copy_byte);
    let source = builder.ins().iadd(input, input_index);
    let byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), source, 0);
    let single = builder.ins().icmp_imm(IntCC::Equal, byte, i64::from(b'\''));
    let double = builder.ins().icmp_imm(IntCC::Equal, byte, i64::from(b'"'));
    let slash = builder.ins().icmp_imm(IntCC::Equal, byte, i64::from(b'\\'));
    let nul = builder.ins().icmp_imm(IntCC::Equal, byte, 0);
    let escaped = builder.ins().bor(single, double);
    let escaped = builder.ins().bor(escaped, slash);
    let escaped = builder.ins().bor(escaped, nul);
    builder.ins().brif(
        escaped,
        write_escape,
        &[input_index.into(), output_index.into(), byte.into()],
        write_plain,
        &[input_index.into(), output_index.into(), byte.into()],
    );

    builder.switch_to_block(write_escape);
    let input_index = builder.block_params(write_escape)[0];
    let output_index = builder.block_params(write_escape)[1];
    let byte = builder.block_params(write_escape)[2];
    let slash_at = builder.ins().iadd(allocation.output, output_index);
    let slash = builder.ins().iconst(types::I8, i64::from(b'\\'));
    builder.ins().store(MemFlagsData::new(), slash, slash_at, 0);
    let value_at = builder.ins().iadd_imm(slash_at, 1);
    let nul = builder.ins().icmp_imm(IntCC::Equal, byte, 0);
    let ascii_zero = builder.ins().iconst(types::I8, i64::from(b'0'));
    let escaped_byte = builder.ins().select(nul, ascii_zero, byte);
    builder
        .ins()
        .store(MemFlagsData::new(), escaped_byte, value_at, 0);
    let output_index = builder.ins().iadd_imm(output_index, 2);
    builder
        .ins()
        .jump(copy_next, &[input_index.into(), output_index.into()]);

    builder.switch_to_block(write_plain);
    let input_index = builder.block_params(write_plain)[0];
    let output_index = builder.block_params(write_plain)[1];
    let byte = builder.block_params(write_plain)[2];
    let destination = builder.ins().iadd(allocation.output, output_index);
    builder
        .ins()
        .store(MemFlagsData::new(), byte, destination, 0);
    let output_index = builder.ins().iadd_imm(output_index, 1);
    builder
        .ins()
        .jump(copy_next, &[input_index.into(), output_index.into()]);

    builder.switch_to_block(copy_next);
    let input_index = builder.block_params(copy_next)[0];
    let output_index = builder.block_params(copy_next)[1];
    let input_index = builder.ins().iadd_imm(input_index, 1);
    builder
        .ins()
        .jump(copy, &[input_index.into(), output_index.into()]);

    builder.switch_to_block(finish);
    lower_optimizing_finish_string(builder, allocation, output_length);
    Ok(allocation.value)
}

fn lower_optimizing_substr_count(
    builder: &mut FunctionBuilder<'_>,
    haystack: ir::Value,
    needle: ir::Value,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (haystack_length, haystack_bytes) =
        lower_optimizing_string_descriptor(builder, haystack, transition)?;
    let (needle_length, needle_bytes) =
        lower_optimizing_string_descriptor(builder, needle, transition)?;
    let scan = builder.create_block();
    let compare = builder.create_block();
    let compare_byte = builder.create_block();
    let matched = builder.create_block();
    let advance = builder.create_block();
    let finish = builder.create_block();
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(compare, types::I64);
    builder.append_block_param(compare, types::I64);
    builder.append_block_param(compare, types::I64);
    builder.append_block_param(matched, types::I64);
    builder.append_block_param(matched, types::I64);
    builder.append_block_param(advance, types::I64);
    builder.append_block_param(advance, types::I64);
    builder.append_block_param(finish, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(scan, &[zero.into(), zero.into()]);

    builder.switch_to_block(scan);
    let index = builder.block_params(scan)[0];
    let count = builder.block_params(scan)[1];
    let needle_fits = builder.ins().icmp(
        IntCC::UnsignedLessThanOrEqual,
        needle_length,
        haystack_length,
    );
    let remaining = builder.ins().isub(haystack_length, index);
    let fits_here = builder
        .ins()
        .icmp(IntCC::UnsignedLessThanOrEqual, needle_length, remaining);
    let can_compare = builder.ins().band(needle_fits, fits_here);
    builder.ins().brif(
        can_compare,
        compare,
        &[index.into(), count.into(), zero.into()],
        finish,
        &[count.into()],
    );

    builder.switch_to_block(compare);
    let index = builder.block_params(compare)[0];
    let count = builder.block_params(compare)[1];
    let needle_index = builder.block_params(compare)[2];
    let complete = builder
        .ins()
        .icmp(IntCC::Equal, needle_index, needle_length);
    builder.ins().brif(
        complete,
        matched,
        &[index.into(), count.into()],
        compare_byte,
        &[],
    );

    builder.switch_to_block(compare_byte);
    let haystack_index = builder.ins().iadd(index, needle_index);
    let haystack_at = builder.ins().iadd(haystack_bytes, haystack_index);
    let needle_at = builder.ins().iadd(needle_bytes, needle_index);
    let haystack_byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), haystack_at, 0);
    let needle_byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), needle_at, 0);
    let equal = builder.ins().icmp(IntCC::Equal, haystack_byte, needle_byte);
    let next_needle = builder.ins().iadd_imm(needle_index, 1);
    builder.ins().brif(
        equal,
        compare,
        &[index.into(), count.into(), next_needle.into()],
        advance,
        &[index.into(), count.into()],
    );

    builder.switch_to_block(matched);
    let index = builder.block_params(matched)[0];
    let count = builder.block_params(matched)[1];
    let index = builder.ins().iadd(index, needle_length);
    let count = builder.ins().iadd_imm(count, 1);
    builder.ins().jump(scan, &[index.into(), count.into()]);

    builder.switch_to_block(advance);
    let index = builder.block_params(advance)[0];
    let count = builder.block_params(advance)[1];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(scan, &[index.into(), count.into()]);

    builder.switch_to_block(finish);
    let count = builder.block_params(finish)[0];
    Ok(count)
}

fn lower_optimizing_string_compare(
    builder: &mut FunctionBuilder<'_>,
    operation: StableStringCompareBuiltin,
    lhs: ir::Value,
    rhs: ir::Value,
    requested_length: Option<ir::Value>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let (lhs_length, lhs_bytes) = lower_optimizing_string_descriptor(builder, lhs, transition)?;
    let (rhs_length, rhs_bytes) = lower_optimizing_string_descriptor(builder, rhs, transition)?;
    let minimum = builder.ins().umin(lhs_length, rhs_length);
    let (limit, bounded) = if operation.bounded() {
        let requested = lower_optimizing_require_immediate_integer(
            builder,
            requested_length.expect("bounded string comparison has a length"),
            transition,
        )?;
        let limit = builder.ins().umin(minimum, requested);
        (limit, Some(requested))
    } else {
        (minimum, None)
    };

    let scan = builder.create_block();
    let compare = builder.create_block();
    let next = builder.create_block();
    let after_bytes = builder.create_block();
    let less = builder.create_block();
    let greater = builder.create_block();
    let equal = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(next, types::I64);
    builder.append_block_param(merge, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(scan, &[zero.into()]);
    builder.switch_to_block(scan);
    let index = builder.block_params(scan)[0];
    let exhausted = builder.ins().icmp(IntCC::Equal, index, limit);
    builder
        .ins()
        .brif(exhausted, after_bytes, &[], compare, &[]);
    builder.switch_to_block(compare);
    let lhs_at = builder.ins().iadd(lhs_bytes, index);
    let rhs_at = builder.ins().iadd(rhs_bytes, index);
    let lhs_byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), lhs_at, 0);
    let rhs_byte = builder
        .ins()
        .load(types::I8, MemFlagsData::new(), rhs_at, 0);
    let fold = |builder: &mut FunctionBuilder<'_>, byte| {
        let uppercase_start =
            builder
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, byte, i64::from(b'A'));
        let uppercase_end =
            builder
                .ins()
                .icmp_imm(IntCC::UnsignedLessThanOrEqual, byte, i64::from(b'Z'));
        let uppercase = builder.ins().band(uppercase_start, uppercase_end);
        let lowered = builder.ins().iadd_imm(byte, 32);
        builder.ins().select(uppercase, lowered, byte)
    };
    let lhs_byte = if operation.case_insensitive() {
        fold(builder, lhs_byte)
    } else {
        lhs_byte
    };
    let rhs_byte = if operation.case_insensitive() {
        fold(builder, rhs_byte)
    } else {
        rhs_byte
    };
    let same = builder.ins().icmp(IntCC::Equal, lhs_byte, rhs_byte);
    let different = builder.create_block();
    builder
        .ins()
        .brif(same, next, &[index.into()], different, &[]);
    builder.switch_to_block(different);
    let lhs_byte = builder.ins().uextend(types::I64, lhs_byte);
    let rhs_byte = builder.ins().uextend(types::I64, rhs_byte);
    let difference = builder.ins().isub(lhs_byte, rhs_byte);
    builder.ins().jump(merge, &[difference.into()]);
    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let next_index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(scan, &[next_index.into()]);

    builder.switch_to_block(after_bytes);
    if let Some(requested) = bounded {
        let reached_requested = builder.ins().icmp(IntCC::Equal, limit, requested);
        let compare_lengths = builder.create_block();
        builder
            .ins()
            .brif(reached_requested, equal, &[], compare_lengths, &[]);
        builder.switch_to_block(compare_lengths);
    }
    let same_length = builder.ins().icmp(IntCC::Equal, lhs_length, rhs_length);
    let lhs_shorter = builder
        .ins()
        .icmp(IntCC::UnsignedLessThan, lhs_length, rhs_length);
    let length_different = builder.create_block();
    builder
        .ins()
        .brif(same_length, equal, &[], length_different, &[]);
    builder.switch_to_block(length_different);
    builder.ins().brif(lhs_shorter, less, &[], greater, &[]);

    builder.switch_to_block(less);
    let negative = builder.ins().iconst(types::I64, -1);
    builder.ins().jump(merge, &[negative.into()]);
    builder.switch_to_block(greater);
    let positive = builder.ins().iconst(types::I64, 1);
    builder.ins().jump(merge, &[positive.into()]);
    builder.switch_to_block(equal);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(merge, &[zero.into()]);
    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}


#[allow(clippy::too_many_arguments)]
fn lower_optimizing_extrema(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operation: StableExtremaBuiltin,
    arguments: &[ir::Value],
    spaceship: Option<NativeHelper>,
    transition: NativeOptimizingTransition<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    debug_assert!(!arguments.is_empty());
    let choose_candidate = |builder: &mut FunctionBuilder<'_>, ordering| match operation {
        StableExtremaBuiltin::Max => builder
            .ins()
            .icmp_imm(IntCC::SignedGreaterThan, ordering, 0),
        StableExtremaBuiltin::Min => builder.ins().icmp_imm(IntCC::SignedLessThan, ordering, 0),
    };
    if arguments.len() > 1 {
        let mut selected =
            lower_optimizing_reference_scalar(builder, arguments[0], false, transition)?;
        for &candidate in &arguments[1..] {
            let candidate =
                lower_optimizing_reference_scalar(builder, candidate, false, transition)?;
            let ordering = emit_total_exact_scalar_value!(
                module,
                builder,
                spaceship,
                &[candidate, selected],
                transition,
                "exact native spaceship handler was not declared",
            )?;
            let replace = choose_candidate(builder, ordering);
            selected = builder.ins().select(replace, candidate, selected);
        }
        lower_optimizing_retain(builder, selected, transition.deopt_out);
        return Ok(selected);
    }

    let array = lower_optimizing_reference_scalar(builder, arguments[0], false, transition)?;
    let (_, length, entries) =
        lower_optimizing_direct_array_descriptor(builder, array, transition)?;
    let scan = builder.create_block();
    let inspect = builder.create_block();
    let done = builder.create_block();
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(done, types::I64);
    let first = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entries,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let first = lower_optimizing_reference_scalar(builder, first, false, transition)?;
    let one = builder.ins().iconst(types::I64, 1);
    builder.ins().jump(scan, &[one.into(), first.into()]);

    builder.switch_to_block(scan);
    let index = builder.block_params(scan)[0];
    let selected = builder.block_params(scan)[1];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder
        .ins()
        .brif(exhausted, done, &[selected.into()], inspect, &[]);

    builder.switch_to_block(inspect);
    let pointer_type = builder.func.dfg.value_type(transition.deopt_out);
    let pointer_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let entry = builder.ins().iadd(entries, offset);
    let candidate = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let candidate = lower_optimizing_reference_scalar(builder, candidate, false, transition)?;
    let ordering = emit_total_exact_scalar_value!(
        module,
        builder,
        spaceship,
        &[candidate, selected],
        transition,
        "exact native spaceship handler was not declared",
    )?;
    let replace = choose_candidate(builder, ordering);
    let selected = builder.ins().select(replace, candidate, selected);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(scan, &[next.into(), selected.into()]);

    builder.switch_to_block(done);
    let selected = builder.block_params(done)[0];
    lower_optimizing_retain(builder, selected, transition.deopt_out);
    Ok(selected)
}
