fn lower_direct_array_child_entry_body(builder: &mut FunctionBuilder<'_>) {
    let entry = builder.create_block();
    let inspect = builder.create_block();
    let search = builder.create_block();
    let compare = builder.create_block();
    let next = builder.create_block();
    let found = builder.create_block();
    let failed = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.append_block_param(search, types::I64);
    builder.append_block_param(next, types::I64);
    let pointer_type = builder.func.dfg.value_type(builder.block_params(entry)[0]);
    builder.append_block_param(found, pointer_type);

    builder.switch_to_block(entry);
    let deopt_out = builder.block_params(entry)[0];
    let array = builder.block_params(entry)[1];
    let key = builder.block_params(entry)[2];
    let is_array = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    let encoded_index = builder.ins().ireduce(types::I32, array);
    let is_direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        encoded_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(is_array, is_direct_index);
    builder.ins().brif(direct, inspect, &[], failed, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, array, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let direct_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let key_runtime = lower_is_runtime_handle(builder, key);
    let key_constant = lower_value_has_namespace_tag(builder, key, crate::JIT_VALUE_CONSTANT_TAG);
    let namespaced = builder.ins().bor(key_runtime, key_constant);
    let key_immediate = builder.ins().icmp_imm(IntCC::Equal, namespaced, 0);
    let key_string = lower_value_has_tag(builder, key, crate::JIT_VALUE_RUNTIME_STRING_TAG);
    let supported_key = builder.ins().bor(key_immediate, key_string);
    let supported_key = builder.ins().bor(supported_key, key_constant);
    let admitted = builder.ins().band(direct_kind, supported_key);
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
    let zero = builder.ins().iconst(types::I64, 0);
    builder
        .ins()
        .brif(admitted, search, &[zero.into()], failed, &[]);

    builder.switch_to_block(search);
    let index = builder.block_params(search)[0];
    let exhausted = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(exhausted, failed, &[], compare, &[]);

    builder.switch_to_block(compare);
    let pointer_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let candidate_entry = builder.ins().iadd(entries, offset);
    let candidate = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), candidate_entry, 0);
    let matches = lower_native_array_key_equal(builder, candidate, key, deopt_out);
    builder.ins().brif(
        matches,
        found,
        &[candidate_entry.into()],
        next,
        &[index.into()],
    );

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(search, &[index.into()]);

    builder.switch_to_block(found);
    let candidate_entry = builder.block_params(found)[0];
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        candidate_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    builder.ins().return_(&[value, candidate_entry]);

    builder.switch_to_block(failed);
    let zero_value = builder.ins().iconst(types::I64, 0);
    let null_entry = builder.ins().iconst(pointer_type, 0);
    builder.ins().return_(&[zero_value, null_entry]);
}


fn lower_free_direct_array_entries(
    builder: &mut FunctionBuilder<'_>,
    deopt_out: ir::Value,
    entries: ir::Value,
    capacity: ir::Value,
) {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = lower_active_runtime_view(builder, deopt_out);
    let arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_entries) as i32,
    );
    let free_heads = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_free_heads) as i32,
    );
    let leading = builder.ins().clz(capacity);
    let ceiling = builder.ins().iconst(types::I32, 31);
    let bucket = builder.ins().isub(ceiling, leading);
    let wide_bucket = builder.ins().uextend(pointer_type, bucket);
    let bucket_offset = builder.ins().ishl_imm(wide_bucket, 2);
    let head_ptr = builder.ins().iadd(free_heads, bucket_offset);
    let old_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), head_ptr, 0);
    let byte_offset = builder.ins().isub(entries, arena);
    let entry_index = builder.ins().ushr_imm(byte_offset, 4);
    let entry_index = if pointer_type == types::I32 {
        entry_index
    } else {
        builder.ins().ireduce(types::I32, entry_index)
    };
    builder
        .ins()
        .store(MemFlagsData::new(), old_head, entries, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), entry_index, head_ptr, 0);
}

fn lower_free_direct_string_bytes(
    builder: &mut FunctionBuilder<'_>,
    deopt_out: ir::Value,
    bytes: ir::Value,
    reserved: ir::Value,
) {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = lower_active_runtime_view(builder, deopt_out);
    let arena = builder.ins().load(
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
    let capacity = builder.ins().ushr_imm(
        reserved,
        crate::JIT_NATIVE_DIRECT_STRING_CAPACITY_SHIFT as i64,
    );
    let leading = builder.ins().clz(capacity);
    let ceiling = builder.ins().iconst(types::I32, 31);
    let bucket = builder.ins().isub(ceiling, leading);
    let wide_bucket = builder.ins().uextend(pointer_type, bucket);
    let bucket_offset = builder.ins().ishl_imm(wide_bucket, 2);
    let head_ptr = builder.ins().iadd(free_heads, bucket_offset);
    let old_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), head_ptr, 0);
    let byte_offset = builder.ins().isub(bytes, arena);
    let byte_offset = if pointer_type == types::I32 {
        byte_offset
    } else {
        builder.ins().ireduce(types::I32, byte_offset)
    };
    builder.ins().store(MemFlagsData::new(), old_head, bytes, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), byte_offset, head_ptr, 0);
}

fn lower_direct_value_release_commit_body(
    module: &mut JITModule,
    builder: &mut FunctionBuilder<'_>,
    func_id: FuncId,
) {
    let entry = builder.create_block();
    let inspect = builder.create_block();
    let decrement = builder.create_block();
    let inspect_last = builder.create_block();
    let release_reference = builder.create_block();
    let inspect_composite = builder.create_block();
    let inspect_foreach = builder.create_block();
    let release_foreach = builder.create_block();
    let free_string = builder.create_block();
    let scan = builder.create_block();
    let release_entry = builder.create_block();
    let next = builder.create_block();
    let free_array = builder.create_block();
    let free_slot = builder.create_block();
    let accepted = builder.create_block();
    let rejected = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.append_block_param(inspect_composite, types::I8);
    builder.append_block_param(inspect_foreach, types::I8);
    builder.append_block_param(scan, types::I64);
    builder.append_block_param(next, types::I64);

    let recurse = module.declare_func_in_func(func_id, builder.func);
    builder.switch_to_block(entry);
    let deopt_out = builder.block_params(entry)[0];
    let value = builder.block_params(entry)[1];
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let runtime = lower_is_runtime_handle(builder, value);
    builder.ins().brif(runtime, inspect, &[], accepted, &[]);

    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, value, deopt_out);
    let refcount = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    let shared = builder
        .ins()
        .icmp_imm(IntCC::UnsignedGreaterThan, refcount, 1);
    builder
        .ins()
        .brif(shared, decrement, &[], inspect_last, &[]);

    builder.switch_to_block(decrement);
    let remaining = builder.ins().iadd_imm(refcount, -1);
    builder.ins().store(
        MemFlagsData::new(),
        remaining,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
    );
    builder.ins().jump(accepted, &[]);

    builder.switch_to_block(inspect_last);
    let last = builder.ins().icmp_imm(IntCC::Equal, refcount, 1);
    let index = builder.ins().ireduce(types::I32, value);
    let direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let valid_last = builder.ins().band(last, direct);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let direct_reference = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR),
    );
    let direct_string = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_STRING),
    );
    let direct_float = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_FLOAT),
    );
    let direct_int = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_INT),
    );
    let direct_number = builder.ins().bor(direct_float, direct_int);
    let string = builder.ins().band(valid_last, direct_string);
    let number = builder.ins().band(valid_last, direct_number);
    let reference = builder.ins().band(valid_last, direct_reference);
    let inspect_reference = builder.create_block();
    builder
        .ins()
        .brif(string, free_string, &[], inspect_reference, &[]);

    builder.switch_to_block(inspect_reference);
    let inspect_float = builder.create_block();
    builder
        .ins()
        .brif(reference, release_reference, &[], inspect_float, &[]);

    builder.switch_to_block(inspect_float);
    builder.ins().brif(
        number,
        free_slot,
        &[],
        inspect_composite,
        &[valid_last.into()],
    );

    builder.switch_to_block(free_string);
    let bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let reserved = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    lower_free_direct_string_bytes(builder, deopt_out, bytes, reserved);
    builder.ins().jump(free_slot, &[]);

    builder.switch_to_block(release_reference);
    let payload = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let _ = builder.ins().call(recurse, &[deopt_out, payload]);
    builder.ins().jump(free_slot, &[]);

    builder.switch_to_block(inspect_composite);
    let valid_last = builder.block_params(inspect_composite)[0];
    let direct_array = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let direct_array = builder.ins().band(valid_last, direct_array);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().brif(
        direct_array,
        scan,
        &[zero.into()],
        inspect_foreach,
        &[valid_last.into()],
    );

    builder.switch_to_block(inspect_foreach);
    let valid_last = builder.block_params(inspect_foreach)[0];
    let direct_foreach = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH),
    );
    let direct_foreach = builder.ins().band(valid_last, direct_foreach);
    builder
        .ins()
        .brif(direct_foreach, release_foreach, &[], rejected, &[]);

    builder.switch_to_block(release_foreach);
    let source = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let _ = builder.ins().call(recurse, &[deopt_out, source]);
    builder.ins().jump(free_slot, &[]);

    builder.switch_to_block(scan);
    let scan_index = builder.block_params(scan)[0];
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let finished = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, scan_index, length);
    builder
        .ins()
        .brif(finished, free_array, &[], release_entry, &[]);

    builder.switch_to_block(release_entry);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let pointer_index = if pointer_type == types::I64 {
        scan_index
    } else {
        builder.ins().ireduce(pointer_type, scan_index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let array_entry = builder.ins().iadd(entries, offset);
    let key = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), array_entry, 0);
    let child = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        array_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let _ = builder.ins().call(recurse, &[deopt_out, key]);
    let _ = builder.ins().call(recurse, &[deopt_out, child]);
    builder.ins().jump(next, &[scan_index.into()]);

    builder.switch_to_block(next);
    let scan_index = builder.block_params(next)[0];
    let next_index = builder.ins().iadd_imm(scan_index, 1);
    builder.ins().jump(scan, &[next_index.into()]);

    builder.switch_to_block(free_array);
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let capacity = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    lower_free_direct_array_entries(builder, deopt_out, entries, capacity);
    builder.ins().jump(free_slot, &[]);

    builder.switch_to_block(free_slot);
    lower_free_direct_scalar_slot(builder, value, slot, deopt_out);
    builder.ins().jump(accepted, &[]);

    builder.switch_to_block(accepted);
    let yes = builder.ins().iconst(types::I8, 1);
    builder.ins().return_(&[yes]);
    builder.switch_to_block(rejected);
    let no = builder.ins().iconst(types::I8, 0);
    builder.ins().return_(&[no]);
}

fn lower_direct_array_ensure_unique_body(builder: &mut FunctionBuilder<'_>) {
    let entry = builder.create_block();
    let inspect = builder.create_block();
    let choose = builder.create_block();
    let grow = builder.create_block();
    let allocate = builder.create_block();
    let reuse = builder.create_block();
    let bump = builder.create_block();
    let range_ready = builder.create_block();
    let clone_slot = builder.create_block();
    let move_slot = builder.create_block();
    let copy = builder.create_block();
    let copy_entry = builder.create_block();
    let retain_entry = builder.create_block();
    let store_entry = builder.create_block();
    let finalize = builder.create_block();
    let finalize_clone = builder.create_block();
    let release_cloned_source = builder.create_block();
    let complete_clone = builder.create_block();
    let finalize_move = builder.create_block();
    let failed = builder.create_block();
    let succeeded = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.append_block_param(choose, types::I8);
    builder.append_block_param(grow, types::I32);
    builder.append_block_param(grow, types::I8);
    builder.append_block_param(allocate, types::I32);
    builder.append_block_param(allocate, types::I8);
    let pointer_type = builder.func.dfg.value_type(builder.block_params(entry)[0]);
    builder.append_block_param(range_ready, pointer_type);
    builder.append_block_param(range_ready, types::I32);
    builder.append_block_param(range_ready, types::I8);
    for block in [copy, finalize] {
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, pointer_type);
        builder.append_block_param(block, types::I64);
        builder.append_block_param(block, types::I8);
    }
    builder.append_block_param(store_entry, types::I64);
    builder.append_block_param(store_entry, pointer_type);
    builder.append_block_param(store_entry, types::I64);
    builder.append_block_param(store_entry, types::I8);
    builder.append_block_param(succeeded, types::I64);

    builder.switch_to_block(entry);
    let deopt_out = builder.block_params(entry)[0];
    let array = builder.block_params(entry)[1];
    let additional = builder.block_params(entry)[2];
    let consume_owner = builder.block_params(entry)[3];
    let is_array = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    let encoded_index = builder.ins().ireduce(types::I32, array);
    let is_direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        encoded_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(is_array, is_direct_index);
    builder.ins().brif(direct, inspect, &[], failed, &[]);

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
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let old_entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let required = builder.ins().iadd(length, additional);
    let wrapped = builder
        .ins()
        .icmp(IntCC::UnsignedLessThan, required, length);
    let within_limit = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        required,
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
    );
    let kind_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let live = builder.ins().icmp_imm(IntCC::NotEqual, refcount, 0);
    let valid = builder.ins().band(kind_ok, live);
    let valid = builder.ins().band_not(valid, wrapped);
    let valid = builder.ins().band(valid, within_limit);
    let unique = builder.ins().icmp_imm(IntCC::Equal, refcount, 1);
    let clone = builder.ins().icmp_imm(IntCC::Equal, unique, 0);
    builder
        .ins()
        .brif(valid, choose, &[clone.into()], failed, &[]);

    builder.switch_to_block(choose);
    let clone = builder.block_params(choose)[0];
    let capacity_wide = builder.ins().uextend(types::I64, capacity);
    let enough = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, capacity_wide, required);
    let unique_and_enough = builder.ins().band(unique, enough);
    builder.ins().brif(
        unique_and_enough,
        succeeded,
        &[array.into()],
        grow,
        &[capacity.into(), clone.into()],
    );

    builder.switch_to_block(grow);
    let candidate = builder.block_params(grow)[0];
    let clone = builder.block_params(grow)[1];
    let wide = builder.ins().uextend(types::I64, candidate);
    let enough = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, wide, required);
    let double = builder.create_block();
    builder.ins().brif(
        enough,
        allocate,
        &[candidate.into(), clone.into()],
        double,
        &[],
    );
    builder.switch_to_block(double);
    let at_limit = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        candidate,
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
    );
    let doubled = builder.ins().imul_imm(candidate, 2);
    let minimum = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY),
    );
    let zero_capacity = builder.ins().icmp_imm(IntCC::Equal, candidate, 0);
    let next = builder.ins().select(zero_capacity, minimum, doubled);
    builder
        .ins()
        .brif(at_limit, failed, &[], grow, &[next.into(), clone.into()]);

    builder.switch_to_block(allocate);
    let destination_capacity = builder.block_params(allocate)[0];
    let clone = builder.block_params(allocate)[1];
    let view = lower_active_runtime_view(builder, deopt_out);
    let arena = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_entries) as i32,
    );
    let free_heads = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_free_heads) as i32,
    );
    let leading = builder.ins().clz(destination_capacity);
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
    builder.ins().brif(has_free, reuse, &[], bump, &[]);

    builder.switch_to_block(reuse);
    let wide_free_head = builder.ins().uextend(pointer_type, free_head);
    let free_offset = builder.ins().ishl_imm(wide_free_head, 4);
    let destination = builder.ins().iadd(arena, free_offset);
    let preceding = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), destination, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), preceding, free_head_ptr, 0);
    builder.ins().jump(
        range_ready,
        &[
            destination.into(),
            destination_capacity.into(),
            clone.into(),
        ],
    );

    builder.switch_to_block(bump);
    let next_ptr = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_next) as i32,
    );
    let next_entry = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), next_ptr, 0);
    let end = builder.ins().iadd(next_entry, destination_capacity);
    let room = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        end,
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
    );
    let wide_next_entry = builder.ins().uextend(pointer_type, next_entry);
    let offset = builder.ins().ishl_imm(wide_next_entry, 4);
    let destination = builder.ins().iadd(arena, offset);
    let bump_ok = builder.create_block();
    builder.ins().brif(room, bump_ok, &[], failed, &[]);
    builder.switch_to_block(bump_ok);
    builder.ins().store(MemFlagsData::new(), end, next_ptr, 0);
    builder.ins().jump(
        range_ready,
        &[
            destination.into(),
            destination_capacity.into(),
            clone.into(),
        ],
    );

    builder.switch_to_block(range_ready);
    let destination_entries = builder.block_params(range_ready)[0];
    let destination_capacity = builder.block_params(range_ready)[1];
    let clone = builder.block_params(range_ready)[2];
    builder.ins().brif(clone, clone_slot, &[], move_slot, &[]);

    builder.switch_to_block(clone_slot);
    let new_index = lower_reserve_direct_value_index(builder, deopt_out, Some(failed));
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_slots) as i32,
    );
    let wide_new_index = builder.ins().uextend(pointer_type, new_index);
    let new_slot_offset = builder.ins().ishl_imm(wide_new_index, 5);
    let destination_slot = builder.ins().iadd(slots, new_slot_offset);
    let runtime_index = builder.ins().iadd_imm(
        new_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let runtime_index = builder.ins().uextend(types::I64, runtime_index);
    let destination_handle = builder
        .ins()
        .bor_imm(runtime_index, crate::JIT_VALUE_RUNTIME_ARRAY_TAG as i64);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(
        copy,
        &[
            zero.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
    );

    builder.switch_to_block(move_slot);
    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(
        copy,
        &[zero.into(), slot.into(), array.into(), clone.into()],
    );

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let destination_slot = builder.block_params(copy)[1];
    let destination_handle = builder.block_params(copy)[2];
    let clone = builder.block_params(copy)[3];
    let finished = builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
    builder.ins().brif(
        finished,
        finalize,
        &[
            index.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
        copy_entry,
        &[],
    );

    builder.switch_to_block(copy_entry);
    let pointer_index = if pointer_type == types::I64 {
        index
    } else {
        builder.ins().ireduce(pointer_type, index)
    };
    let offset = builder.ins().ishl_imm(pointer_index, 4);
    let source_entry = builder.ins().iadd(old_entries, offset);
    let destination_entry = builder.ins().iadd(destination_entries, offset);
    let key = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), source_entry, 0);
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        source_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    builder.ins().brif(
        clone,
        retain_entry,
        &[],
        store_entry,
        &[
            index.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
    );
    builder.switch_to_block(retain_entry);
    lower_optimizing_retain(builder, key, deopt_out);
    lower_optimizing_retain(builder, value, deopt_out);
    builder.ins().jump(
        store_entry,
        &[
            index.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
    );
    builder.switch_to_block(store_entry);
    let index = builder.block_params(store_entry)[0];
    let destination_slot = builder.block_params(store_entry)[1];
    let destination_handle = builder.block_params(store_entry)[2];
    let clone = builder.block_params(store_entry)[3];
    builder
        .ins()
        .store(MemFlagsData::new(), key, destination_entry, 0);
    builder.ins().store(
        MemFlagsData::new(),
        value,
        destination_entry,
        std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
    );
    let next = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(
        copy,
        &[
            next.into(),
            destination_slot.into(),
            destination_handle.into(),
            clone.into(),
        ],
    );

    builder.switch_to_block(finalize);
    let destination_slot = builder.block_params(finalize)[1];
    let destination_handle = builder.block_params(finalize)[2];
    let clone = builder.block_params(finalize)[3];
    builder
        .ins()
        .brif(clone, finalize_clone, &[], finalize_move, &[]);
    builder.switch_to_block(finalize_clone);
    let one = builder.ins().iconst(types::I32, 1);
    let array_kind = builder.ins().iconst(
        types::I32,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    for (value, offset) in [
        (
            one,
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount),
        ),
        (
            array_kind,
            std::mem::offset_of!(crate::JitNativeValueSlot, kind),
        ),
        (
            flags,
            std::mem::offset_of!(crate::JitNativeValueSlot, flags),
        ),
        (
            destination_capacity,
            std::mem::offset_of!(crate::JitNativeValueSlot, reserved),
        ),
    ] {
        builder
            .ins()
            .store(MemFlagsData::new(), value, destination_slot, offset as i32);
    }
    builder.ins().store(
        MemFlagsData::new(),
        length,
        destination_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        destination_entries,
        destination_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    // The PHP auto-index cursor is slot-parallel state, not reconstructible
    // from live entries: unset preserves it while pop may move it backwards.
    // A COW clone therefore owns an exact copy of the source cursor just as
    // it owns an exact copy of the entries. Leaving the newly reserved state
    // zeroed made the first append after a foreach-triggered clone reuse key
    // zero.
    let source_state = lower_direct_array_state_address(builder, array, deopt_out);
    let destination_state =
        lower_direct_array_state_address(builder, destination_handle, deopt_out);
    for offset in [0, 8] {
        let state_word =
            builder
                .ins()
                .load(types::I64, MemFlagsData::new(), source_state, offset);
        builder
            .ins()
            .store(MemFlagsData::new(), state_word, destination_state, offset);
    }
    builder.ins().brif(
        consume_owner,
        release_cloned_source,
        &[],
        complete_clone,
        &[],
    );

    builder.switch_to_block(release_cloned_source);
    let remaining = builder.ins().iadd_imm(refcount, -1);
    builder.ins().store(MemFlagsData::new(), remaining, slot, 0);
    builder.ins().jump(complete_clone, &[]);

    builder.switch_to_block(complete_clone);
    builder.ins().jump(succeeded, &[destination_handle.into()]);

    builder.switch_to_block(finalize_move);
    let old_leading = builder.ins().clz(capacity);
    let old_ceiling = builder.ins().iconst(types::I32, 31);
    let old_bucket = builder.ins().isub(old_ceiling, old_leading);
    let wide_old_bucket = builder.ins().uextend(pointer_type, old_bucket);
    let old_bucket_offset = builder.ins().ishl_imm(wide_old_bucket, 2);
    let old_head_ptr = builder.ins().iadd(free_heads, old_bucket_offset);
    let old_head = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), old_head_ptr, 0);
    let old_byte_offset = builder.ins().isub(old_entries, arena);
    let old_entry_index = builder.ins().ushr_imm(old_byte_offset, 4);
    let old_index = if pointer_type == types::I32 {
        old_entry_index
    } else {
        builder.ins().ireduce(types::I32, old_entry_index)
    };
    builder
        .ins()
        .store(MemFlagsData::new(), old_head, old_entries, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), old_index, old_head_ptr, 0);
    builder.ins().store(
        MemFlagsData::new(),
        destination_capacity,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    builder.ins().store(
        MemFlagsData::new(),
        destination_entries,
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    builder.ins().jump(succeeded, &[array.into()]);

    builder.switch_to_block(failed);
    let status = builder.ins().iconst(types::I32, 1);
    builder.ins().return_(&[status, array]);
    builder.switch_to_block(succeeded);
    let result = builder.block_params(succeeded)[0];
    let status = builder.ins().iconst(types::I32, 0);
    builder.ins().return_(&[status, result]);
}

fn define_direct_array_child_entry_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    func_id: FuncId,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    ctx.func.signature = direct_array_child_entry_signature(module);
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        lower_direct_array_child_entry_body(&mut builder);
        builder.seal_all_blocks();
        builder.finalize();
    }
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("direct array child-entry verifier failure: {error}"),
        )
    })?;
    let clif_blocks = ctx.func.layout.blocks().count();
    let pre_regalloc = PreRegallocMetrics {
        blocks: clif_blocks,
        values: ctx.func.dfg.num_values(),
        instructions: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.layout.block_insts(block).count())
            .sum(),
        block_parameters: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.dfg.block_params(block).len())
            .sum(),
        ..PreRegallocMetrics::default()
    };
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define direct array child-entry function: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            "Cranelift returned no direct array child-entry code",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |layout| layout.frame_to_fp_offset);
    let code = compiled.code_buffer().to_vec();
    let alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations: Vec::new(),
        native_pc_ranges: Vec::new(),
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries: 0,
        production_lowering: Vec::new(),
    })
}

fn define_direct_value_release_commit_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    func_id: FuncId,
    symbol: FunctionId,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    ctx.func.signature = direct_value_release_signature(module);
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        lower_direct_value_release_commit_body(module, &mut builder, func_id);
        builder.seal_all_blocks();
        builder.finalize();
    }
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("direct value-release commit verifier failure: {error}"),
        )
    })?;
    let clif_blocks = ctx.func.layout.blocks().count();
    let pre_regalloc = PreRegallocMetrics {
        blocks: clif_blocks,
        values: ctx.func.dfg.num_values(),
        instructions: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.layout.block_insts(block).count())
            .sum(),
        block_parameters: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.dfg.block_params(block).len())
            .sum(),
        ..PreRegallocMetrics::default()
    };
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define direct value-release commit: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            "Cranelift returned no direct value-release commit code",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |layout| layout.frame_to_fp_offset);
    let code = compiled.code_buffer().to_vec();
    let alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    let relocation_functions = BTreeMap::from([(symbol, func_id)]);
    let relocations = compiled
        .buffer
        .relocs()
        .iter()
        .map(|relocation| {
            capture_relocation(
                module,
                ModuleReloc::from_mach_reloc(relocation, &ctx.func, func_id),
                &relocation_functions,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations,
        native_pc_ranges: Vec::new(),
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries: 0,
        production_lowering: Vec::new(),
    })
}

fn define_direct_array_ensure_unique_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    func_id: FuncId,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    ctx.func.signature = direct_array_ensure_unique_signature(module);
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        lower_direct_array_ensure_unique_body(&mut builder);
        builder.seal_all_blocks();
        builder.finalize();
    }
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("direct array COW verifier failure: {error}"),
        )
    })?;
    let clif_blocks = ctx.func.layout.blocks().count();
    let pre_regalloc = PreRegallocMetrics {
        blocks: clif_blocks,
        values: ctx.func.dfg.num_values(),
        instructions: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.layout.block_insts(block).count())
            .sum(),
        block_parameters: ctx
            .func
            .layout
            .blocks()
            .map(|block| ctx.func.dfg.block_params(block).len())
            .sum(),
        ..PreRegallocMetrics::default()
    };
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define direct array COW function: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            "Cranelift returned no direct array COW code",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |layout| layout.frame_to_fp_offset);
    let code = compiled.code_buffer().to_vec();
    let alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations: Vec::new(),
        native_pc_ranges: Vec::new(),
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries: 0,
        production_lowering: Vec::new(),
    })
}
