
fn lower_direct_array_state_address(
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    deopt_out: ir::Value,
) -> ir::Value {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = lower_active_runtime_view(builder, deopt_out);
    let states = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_states) as i32,
    );
    let encoded_index = builder.ins().ireduce(types::I32, array);
    let index = builder.ins().iadd_imm(
        encoded_index,
        -i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let wide_index = builder.ins().uextend(pointer_type, index);
    let offset = builder.ins().ishl_imm(
        wide_index,
        std::mem::size_of::<crate::JitNativeDirectArrayState>().trailing_zeros() as i64,
    );
    builder.ins().iadd(states, offset)
}

fn lower_total_array_next_integer_key(
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    deopt_out: ir::Value,
) -> ir::Value {
    let state = lower_direct_array_state_address(builder, array, deopt_out);
    let next = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, next_append_key) as i32,
    );
    let has_next = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, has_next_append_key) as i32,
    );
    let absent = builder.ins().icmp_imm(IntCC::Equal, has_next, 0);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().select(absent, zero, next)
}

/// Append one publication-classified literal entry to a fresh direct array.
/// Publication proves the target identity, unique key, capacity, key
/// normalization, and plain value ownership, so this lowering contains no
/// representation or allocation fallback edge.
fn lower_total_fresh_array_insert(
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    key: ir::Value,
    value: ir::Value,
    retain_key: bool,
    retain_value: bool,
    // `Some(Some(raw))` is a proven integer key, `Some(None)` a proven
    // non-integer key, and `None` requires generated key classification.
    known_integer_key: Option<Option<ir::Value>>,
    deopt_out: ir::Value,
) -> ir::Value {
    let key = lower_normalize_native_array_key(builder, key, deopt_out);
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let slot = lower_optimizing_slot_address(builder, array, deopt_out);
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let entry = lower_optimizing_direct_array_entry_address(builder, entries, length, pointer_type);
    if retain_key {
        lower_optimizing_retain(builder, key, deopt_out);
    }
    if retain_value {
        lower_optimizing_retain(builder, value, deopt_out);
    }
    builder.ins().store(
        MemFlagsData::new(),
        key,
        entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, key) as i32,
    );
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

    if let Some(known_integer_key) = known_integer_key {
        if let Some(raw_key) = known_integer_key {
            lower_total_fresh_array_update_next_key(builder, array, raw_key, deopt_out);
        }
        lower_mark_native_roots_dirty(builder, deopt_out);
        return array;
    }
    let (integer, raw_key) =
        lower_native_array_key_integer_candidate(builder, key, deopt_out);
    let update_state = builder.create_block();
    let done = builder.create_block();
    builder.ins().brif(integer, update_state, &[], done, &[]);
    builder.switch_to_block(update_state);
    lower_total_fresh_array_update_next_key(builder, array, raw_key, deopt_out);
    builder.ins().jump(done, &[]);
    builder.switch_to_block(done);
    lower_mark_native_roots_dirty(builder, deopt_out);
    array
}

fn lower_total_fresh_array_update_next_key(
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    raw_key: ir::Value,
    deopt_out: ir::Value,
) {
    let state = lower_direct_array_state_address(builder, array, deopt_out);
    let current = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, next_append_key) as i32,
    );
    let has_current = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, has_next_append_key) as i32,
    );
    let candidate = builder.ins().iadd_imm(raw_key, 1);
    let greater = builder
        .ins()
        .icmp(IntCC::SignedGreaterThan, candidate, current);
    let absent = builder.ins().icmp_imm(IntCC::Equal, has_current, 0);
    let replace = builder.ins().bor(greater, absent);
    let next = builder.ins().select(replace, candidate, current);
    builder.ins().store(
        MemFlagsData::new(),
        next,
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, next_append_key) as i32,
    );
    let one = builder.ins().iconst(types::I32, 1);
    builder.ins().store(
        MemFlagsData::new(),
        one,
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, has_next_append_key) as i32,
    );
}

fn lower_total_direct_array_ensure_unique_capacity(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    operation: FuncId,
    array: ir::Value,
    additional: ir::Value,
    consume_owner: bool,
    deopt_out: ir::Value,
) -> ir::Value {
    let callee = module.declare_func_in_func(operation, builder.func);
    let consume_owner = builder.ins().iconst(types::I8, i64::from(consume_owner));
    let call = builder
        .ins()
        .call(callee, &[deopt_out, array, additional, consume_owner]);
    builder.inst_results(call)[1]
}

fn lower_total_fresh_array_spread(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    array_ensure_unique: FuncId,
    array: ir::Value,
    source: ir::Value,
    deopt_out: ir::Value,
) -> ir::Value {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let source_slot = lower_optimizing_slot_address(builder, source, deopt_out);
    let source_length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        source_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let source_entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        source_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let array = lower_total_direct_array_ensure_unique_capacity(
        module,
        builder,
        array_ensure_unique,
        array,
        source_length,
        true,
        deopt_out,
    );
    let scan = builder.create_block();
    let insert = builder.create_block();
    let done = builder.create_block();
    builder.append_block_param(scan, types::I64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(scan, &[zero.into()]);

    builder.switch_to_block(scan);
    let index = builder.block_params(scan)[0];
    let finished = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, source_length);
    builder.ins().brif(finished, done, &[], insert, &[]);

    builder.switch_to_block(insert);
    let entry =
        lower_optimizing_direct_array_entry_address(builder, source_entries, index, pointer_type);
    let source_key = builder.ins().load(
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
    let integer = lower_native_array_key_integer_candidate(builder, source_key, deopt_out).0;
    let append_key = lower_total_array_next_integer_key(builder, array, deopt_out);
    let key = builder.ins().select(integer, append_key, source_key);
    let _ = lower_total_fresh_array_insert(
        builder,
        array,
        key,
        value,
        true,
        true,
        None,
        deopt_out,
    );
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(scan, &[next.into()]);

    builder.switch_to_block(done);
    array
}
