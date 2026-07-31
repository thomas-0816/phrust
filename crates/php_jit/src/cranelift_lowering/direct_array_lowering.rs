fn lower_baseline_direct_new_array(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    helper: Option<NativeHelper>,
    result_out: ir::Value,
    deopt_out: ir::Value,
) -> Result<ir::Value, CraneliftLoweringError> {
    let accepted = builder.create_block();
    let rejected = builder.create_block();
    let merge = builder.create_block();
    builder.append_block_param(merge, types::I64);
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = lower_active_runtime_view(builder, deopt_out);
    let entry_next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_next) as i32,
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
    let placeholder = lower_native_value_operation(module, builder, helper, 0, &[], result_out)?;
    builder.ins().jump(merge, &[placeholder.into()]);

    builder.switch_to_block(accepted);
    let next = lower_reserve_direct_value_index(builder, deopt_out, Some(rejected));
    builder
        .ins()
        .store(MemFlagsData::new(), entry_end, entry_next_ptr, 0);
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_entries) as i32,
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
    let state = lower_direct_array_state_address(builder, encoded, deopt_out);
    builder.ins().store(
        MemFlagsData::new(),
        zero,
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, next_append_key) as i32,
    );
    let zero32 = builder.ins().iconst(types::I32, 0);
    builder.ins().store(
        MemFlagsData::new(),
        zero32,
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, has_next_append_key) as i32,
    );
    builder.ins().jump(merge, &[encoded.into()]);

    builder.switch_to_block(merge);
    Ok(builder.block_params(merge)[0])
}

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
    deopt_out: ir::Value,
) -> ir::Value {
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
    lower_optimizing_retain(builder, key, deopt_out);
    lower_optimizing_retain(builder, value, deopt_out);
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

    let (integer, raw_key) = lower_native_array_key_integer_candidate(builder, key, deopt_out);
    let update_state = builder.create_block();
    let done = builder.create_block();
    builder
        .ins()
        .brif(integer, update_state, &[], done, &[]);
    builder.switch_to_block(update_state);
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
    builder.ins().jump(done, &[]);
    builder.switch_to_block(done);
    array
}
#[allow(clippy::too_many_arguments)]
fn lower_direct_array_append(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    key: Option<ir::Value>,
    value: ir::Value,
    move_value: bool,
    result_out: ir::Value,
    deopt_out: ir::Value,
    fallback: NativeBaselineArrayWriteFallback<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    let literal_value_borrowed = builder.ins().iconst(types::I8, 0);
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
    let prepare_append = builder.create_block();
    let scan_append_key = builder.create_block();
    let scan_append_entry = builder.create_block();
    let finish_append_key = builder.create_block();
    let append = builder.create_block();
    let rejected = builder.create_block();
    let done = builder.create_block();
    builder.append_block_param(copy_entries, types::I64);
    builder.append_block_param(growth_allocated, pointer_type);
    builder.append_block_param(scan_append_key, types::I64);
    builder.append_block_param(scan_append_key, types::I64);
    builder.append_block_param(scan_append_key, types::I8);
    builder.append_block_param(finish_append_key, types::I64);
    builder.append_block_param(finish_append_key, types::I8);
    builder.append_block_param(append, types::I64);
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
        .brif(has_room, prepare_append, &[], inspect_growth, &[]);

    // A direct array owns a contiguous slice in the request arena. Growing it
    // allocates a new slice, copies encoded entries without changing their
    // ownership, and atomically switches the descriptor before appending. The
    // old slice is dead arena storage, not a second owner. This removes the
    // previous capacity-eight transition into the Rust PhpArray path.
    builder.switch_to_block(inspect_growth);
    let view = lower_active_runtime_view(builder, deopt_out);
    let next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_next) as i32,
    );
    let arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_entries) as i32,
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
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_free_heads) as i32,
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
    let reused_bytes_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_reused_bytes) as i32,
    );
    let reused_bytes = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), reused_bytes_ptr, 0);
    let grown_capacity_wide = builder.ins().uextend(types::I64, grown_capacity);
    let reused_delta = builder.ins().imul_imm(
        grown_capacity_wide,
        std::mem::size_of::<crate::JitNativeDirectArrayEntry>() as i64,
    );
    let reused_bytes = builder.ins().iadd(reused_bytes, reused_delta);
    builder
        .ins()
        .store(MemFlagsData::new(), reused_bytes, reused_bytes_ptr, 0);
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
    builder.ins().jump(prepare_append, &[]);

    builder.switch_to_block(prepare_append);
    if let Some(entry_key) = key {
        builder.ins().jump(append, &[entry_key.into()]);
    } else {
        let state = lower_direct_array_state_address(builder, array, deopt_out);
        let next_key = builder.ins().load(
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
        let next_key = builder.ins().select(absent, zero, next_key);
        let at_maximum = builder.ins().icmp_imm(IntCC::Equal, next_key, i64::MAX);
        let no = builder.ins().iconst(types::I8, 0);
        builder.ins().brif(
            at_maximum,
            scan_append_key,
            &[zero.into(), next_key.into(), no.into()],
            append,
            &[next_key.into()],
        );
    }

    if key.is_none() {
        // At i64::MAX PHP admits one append only while that exact key is absent.
        // The authoritative auto-index state handles every ordinary append;
        // this scan is therefore confined to the terminal-key edge case.
        builder.switch_to_block(scan_append_key);
        let scan_index = builder.block_params(scan_append_key)[0];
        let next_key = builder.block_params(scan_append_key)[1];
        let found_maximum = builder.block_params(scan_append_key)[2];
        let scanned_all = builder
            .ins()
            .icmp(IntCC::UnsignedGreaterThanOrEqual, scan_index, length);
        builder.ins().brif(
            scanned_all,
            finish_append_key,
            &[next_key.into(), found_maximum.into()],
            scan_append_entry,
            &[],
        );

        builder.switch_to_block(scan_append_entry);
        let entries = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
        );
        let scan_pointer = if pointer_type == types::I64 {
            scan_index
        } else {
            builder.ins().ireduce(pointer_type, scan_index)
        };
        let scan_offset = builder.ins().ishl_imm(scan_pointer, 4);
        let scan_entry = builder.ins().iadd(entries, scan_offset);
        let candidate = builder
            .ins()
            .load(types::I64, MemFlagsData::new(), scan_entry, 0);
        let (candidate_integer, candidate_raw) =
            lower_native_array_key_integer_candidate(builder, candidate, deopt_out);
        let maximum = builder
            .ins()
            .icmp_imm(IntCC::Equal, candidate_raw, i64::MAX);
        let found = builder.ins().band(candidate_integer, maximum);
        let found_maximum = builder.ins().bor(found_maximum, found);
        let next_scan = builder.ins().iadd_imm(scan_index, 1);
        builder.ins().jump(
            scan_append_key,
            &[next_scan.into(), next_key.into(), found_maximum.into()],
        );

        builder.switch_to_block(finish_append_key);
        let next_key = builder.block_params(finish_append_key)[0];
        let overflow = builder.block_params(finish_append_key)[1];
        builder
            .ins()
            .brif(overflow, rejected, &[], append, &[next_key.into()]);
    }

    builder.switch_to_block(append);
    let entry_key = builder.block_params(append)[0];
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
    lower_optimizing_retain(builder, entry_key, deopt_out);
    if !move_value {
        lower_optimizing_retain(builder, value, deopt_out);
    } else {
        lower_optimizing_retain_if(builder, value, literal_value_borrowed, deopt_out);
    }
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
    let state = lower_direct_array_state_address(builder, array, deopt_out);
    let current_next = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, next_append_key) as i32,
    );
    let has_current_next = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, has_next_append_key) as i32,
    );
    let (integer_key, integer_raw) =
        lower_native_array_key_integer_candidate(builder, entry_key, deopt_out);
    let maximum_key = builder.ins().icmp_imm(IntCC::Equal, integer_raw, i64::MAX);
    let incremented_key = builder.ins().iadd_imm(integer_raw, 1);
    let candidate_next = builder
        .ins()
        .select(maximum_key, integer_raw, incremented_key);
    let advances = builder
        .ins()
        .icmp(IntCC::SignedGreaterThan, candidate_next, current_next);
    let absent = builder.ins().icmp_imm(IntCC::Equal, has_current_next, 0);
    let advances = builder.ins().bor(absent, advances);
    let advances = builder.ins().band(integer_key, advances);
    let next_append_key = builder.ins().select(advances, candidate_next, current_next);
    builder.ins().store(
        MemFlagsData::new(),
        next_append_key,
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, next_append_key) as i32,
    );
    let has_next = builder.ins().icmp_imm(IntCC::NotEqual, integer_key, 0);
    let had_next = builder.ins().icmp_imm(IntCC::NotEqual, has_current_next, 0);
    let has_next = builder.ins().bor(has_next, had_next);
    let has_next = builder.ins().uextend(types::I32, has_next);
    builder.ins().store(
        MemFlagsData::new(),
        has_next,
        state,
        std::mem::offset_of!(crate::JitNativeDirectArrayState, has_next_append_key) as i32,
    );
    // PhpArray initializes an absent internal pointer when the first entry is
    // appended (including after the pointer ran past the end). Preserve that
    // behavior in the authoritative dense representation.
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let cursor = builder
        .ins()
        .ushr_imm(flags, crate::JIT_NATIVE_DIRECT_ARRAY_CURSOR_SHIFT as i64);
    let absent = builder.ins().icmp_imm(
        IntCC::Equal,
        cursor,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_CURSOR_NONE),
    );
    let first = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION),
    );
    let flags = builder.ins().select(absent, first, flags);
    builder.ins().store(
        MemFlagsData::new(),
        flags,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
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
    constant_string_key: bool,
    value: ir::Value,
    move_value: bool,
    result_out: ir::Value,
    deopt_out: ir::Value,
    fallback: NativeBaselineArrayWriteFallback<'_>,
) -> Result<ir::Value, CraneliftLoweringError> {
    if constant_string_key {
        return lower_array_write_fallback(
            module, builder, fallback, array, key, value, result_out, deopt_out,
        );
    }
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
    // Baseline keeps the complete PHP key-conversion semantics behind its
    // single typed continuation. String literals are already published native
    // values here, so the continuation never sees a unit-local encoding.
    let key_runtime = lower_is_runtime_handle(builder, key);
    let key_constant =
        lower_value_has_namespace_tag(builder, key, crate::JIT_VALUE_CONSTANT_TAG);
    let immediate = builder.ins().icmp_imm(IntCC::Equal, key_runtime, 0);
    let supported_key = builder.ins().band_not(immediate, key_constant);
    let _ = constant_string_key;
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
    let matches = builder.ins().icmp(IntCC::Equal, candidate, key);
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
    let literal_value_borrowed = builder.ins().iconst(types::I8, 0);
    let NativeBaselineArrayWriteFallback::Baseline {
        lifecycle,
        operation,
        ..
    } = fallback;
    let _ = lower_guarded_value_release(
        module,
        builder,
        lifecycle,
        operation | 1,
        old,
        result_out,
        deopt_out,
    )?;
    if !move_value {
        lower_optimizing_retain(builder, value, deopt_out);
    } else {
        lower_optimizing_retain_if(builder, value, literal_value_borrowed, deopt_out);
    }
    let stored_value = value;
    builder.ins().store(
        MemFlagsData::new(),
        stored_value,
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
        move_value,
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
    let entry = lower_optimizing_direct_array_entry_address(
        builder,
        source_entries,
        index,
        pointer_type,
    );
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
    let _ = lower_total_fresh_array_insert(builder, array, key, value, deopt_out);
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(scan, &[next.into()]);

    builder.switch_to_block(done);
    array
}
