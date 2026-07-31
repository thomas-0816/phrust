const EXACT_RECURSIVE_ARRAY_DEPTH_LIMIT: usize = 64;

struct ExactRecursiveArrayPath {
    pairs: [(usize, usize); EXACT_RECURSIVE_ARRAY_DEPTH_LIMIT],
    length: usize,
}

impl ExactRecursiveArrayPath {
    const fn new() -> Self {
        Self {
            pairs: [(0, 0); EXACT_RECURSIVE_ARRAY_DEPTH_LIMIT],
            length: 0,
        }
    }

    fn contains(&self, pair: (usize, usize)) -> bool {
        self.pairs[..self.length].contains(&pair)
    }

    fn push(&mut self, pair: (usize, usize)) -> Result<(), &'static str> {
        let slot = self
            .pairs
            .get_mut(self.length)
            .ok_or("native recursive-array path exceeded its fixed depth")?;
        *slot = pair;
        self.length += 1;
        Ok(())
    }

    fn pop(&mut self) {
        self.length = self.length.saturating_sub(1);
    }
}

#[derive(Clone, Copy)]
enum ExactNativeArrayKey {
    Int(i64),
    String,
}

fn exact_direct_dereference(fast: &NativeRequestFastState, mut encoded: i64) -> Option<i64> {
    for _ in 0..EXACT_RECURSIVE_ARRAY_DEPTH_LIMIT {
        let Some((_, slot)) = fast.direct_slot(encoded) else {
            return Some(encoded);
        };
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR {
            return Some(encoded);
        }
        if slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            || native_reference_state(slot.reserved)
                == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            return None;
        }
        encoded = slot.payload as i64;
    }
    None
}

#[derive(Clone, Copy)]
struct ExactNativeArrayRange {
    identity: usize,
    entries: *const php_jit::JitNativeDirectArrayEntry,
    length: usize,
}

impl ExactNativeArrayRange {
    fn get(self, index: usize) -> Option<php_jit::JitNativeDirectArrayEntry> {
        if index >= self.length {
            return None;
        }
        // SAFETY: the range comes from the request's stable array arena and
        // its encoded owner remains live throughout each recursive operation.
        #[allow(unsafe_code)]
        Some(unsafe { *self.entries.add(index) })
    }

    fn iter(self) -> impl ExactSizeIterator<Item = php_jit::JitNativeDirectArrayEntry> {
        (0..self.length).map(move |index| {
            self.get(index)
                .expect("stable native array range index is in bounds")
        })
    }

    fn prefix(self, length: usize) -> Self {
        Self {
            length: length.min(self.length),
            ..self
        }
    }
}

fn exact_direct_array_range(
    fast: &NativeRequestFastState,
    encoded: i64,
) -> Option<ExactNativeArrayRange> {
    let encoded = exact_direct_dereference(fast, encoded)?;
    let (identity, slot) = fast.direct_slot(encoded)?;
    if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
        return None;
    }
    let (entries, length) = fast.stable_native_array_range(encoded)?;
    Some(ExactNativeArrayRange {
        identity,
        entries,
        length,
    })
}

fn exact_native_array_key(
    fast: &NativeRequestFastState,
    encoded: i64,
) -> Option<ExactNativeArrayKey> {
    match fast.native_comparison_value(encoded)? {
        super::NativeComparisonValue::Int(value) => Some(ExactNativeArrayKey::Int(value)),
        super::NativeComparisonValue::String(_) => Some(ExactNativeArrayKey::String),
        _ => None,
    }
}

fn exact_native_array_keys_equal(
    fast: &NativeRequestFastState,
    left: i64,
    right: i64,
) -> Option<bool> {
    Some(fast.native_compare_array_keys(left, right)? == std::cmp::Ordering::Equal)
}

fn exact_retain_entry(
    fast: &mut NativeRequestFastState,
    entry: php_jit::JitNativeDirectArrayEntry,
) -> Result<php_jit::JitNativeDirectArrayEntry, &'static str> {
    fast.retain_direct_encoded(entry.key)?;
    if let Err(error) = fast.retain_direct_encoded(entry.value) {
        fast.rollback_direct_retain(entry.key);
        return Err(error);
    }
    Ok(entry)
}

fn exact_publish_retained_list(
    fast: &mut NativeRequestFastState,
    values: &[i64],
) -> Result<i64, &'static str> {
    fast.publish_retained_direct_array_from_iter(values.iter().copied().enumerate().map(
        |(index, value)| php_jit::JitNativeDirectArrayEntry {
            // Direct arrays are bounded by a u32 arena capacity, so every
            // packed index is representable in the native i64 key lane.
            key: i64::try_from(index).unwrap_or(i64::MAX),
            value,
        },
    ))
}

fn exact_next_append_key(
    fast: &NativeRequestFastState,
    entries: ExactNativeArrayRange,
) -> Result<i64, &'static str> {
    let mut next = 0_i64;
    for entry in entries.iter() {
        match exact_native_array_key(fast, entry.key)
            .ok_or("native recursive-array key is not an integer or string")?
        {
            ExactNativeArrayKey::Int(key) if key >= next => {
                next = key
                    .checked_add(1)
                    .ok_or("native recursive-array append key overflow")?;
            }
            ExactNativeArrayKey::Int(_) | ExactNativeArrayKey::String => {}
        }
    }
    Ok(next)
}

fn exact_find_array_key(
    fast: &NativeRequestFastState,
    entries: ExactNativeArrayRange,
    key: i64,
) -> Result<Option<usize>, &'static str> {
    for (index, entry) in entries.iter().enumerate() {
        if exact_native_array_keys_equal(fast, entry.key, key)
            .ok_or("native recursive-array key comparison failed")?
        {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn exact_find_last_array_key(
    fast: &NativeRequestFastState,
    entries: ExactNativeArrayRange,
    key: i64,
) -> Result<Option<usize>, &'static str> {
    for index in (0..entries.length).rev() {
        let entry = entries
            .get(index)
            .ok_or("native recursive-array range is truncated")?;
        if exact_native_array_keys_equal(fast, entry.key, key)
            .ok_or("native recursive-array key comparison failed")?
        {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn exact_count_replace_appends(
    fast: &NativeRequestFastState,
    left: ExactNativeArrayRange,
    right: ExactNativeArrayRange,
) -> Result<usize, &'static str> {
    let mut appended = 0_usize;
    for index in 0..right.length {
        let entry = right
            .get(index)
            .ok_or("native recursive replace range is truncated")?;
        if exact_find_array_key(fast, left, entry.key)?.is_none()
            && exact_find_array_key(fast, right.prefix(index), entry.key)?.is_none()
        {
            appended = appended
                .checked_add(1)
                .ok_or("native recursive replace length overflow")?;
        }
    }
    Ok(appended)
}

fn exact_nth_replace_append(
    fast: &NativeRequestFastState,
    left: ExactNativeArrayRange,
    right: ExactNativeArrayRange,
    target: usize,
) -> Result<php_jit::JitNativeDirectArrayEntry, &'static str> {
    let mut found = 0_usize;
    for index in 0..right.length {
        let entry = right
            .get(index)
            .ok_or("native recursive replace range is truncated")?;
        if exact_find_array_key(fast, left, entry.key)?.is_none()
            && exact_find_array_key(fast, right.prefix(index), entry.key)?.is_none()
        {
            if found == target {
                return Ok(entry);
            }
            found += 1;
        }
    }
    Err("native recursive replace lost an appended entry")
}

fn exact_merge_entry_is_appended(
    fast: &NativeRequestFastState,
    left: ExactNativeArrayRange,
    right: ExactNativeArrayRange,
    right_index: usize,
) -> Result<bool, &'static str> {
    let entry = right
        .get(right_index)
        .ok_or("native recursive merge right range is truncated")?;
    match exact_native_array_key(fast, entry.key)
        .ok_or("native recursive-array key is not an integer or string")?
    {
        ExactNativeArrayKey::Int(_) => Ok(true),
        ExactNativeArrayKey::String => Ok(
            exact_find_array_key(fast, left, entry.key)?.is_none()
                && exact_find_array_key(fast, right.prefix(right_index), entry.key)?.is_none(),
        ),
    }
}

fn exact_count_merge_appends(
    fast: &NativeRequestFastState,
    left: ExactNativeArrayRange,
    right: ExactNativeArrayRange,
) -> Result<usize, &'static str> {
    let mut appended = 0_usize;
    for index in 0..right.length {
        if exact_merge_entry_is_appended(fast, left, right, index)? {
            appended = appended
                .checked_add(1)
                .ok_or("native recursive merge length overflow")?;
        }
    }
    Ok(appended)
}

fn exact_nth_merge_append(
    fast: &NativeRequestFastState,
    left: ExactNativeArrayRange,
    right: ExactNativeArrayRange,
    target: usize,
) -> Result<php_jit::JitNativeDirectArrayEntry, &'static str> {
    let mut found = 0_usize;
    for index in 0..right.length {
        if exact_merge_entry_is_appended(fast, left, right, index)? {
            if found == target {
                return right
                    .get(index)
                    .ok_or("native recursive merge right range is truncated");
            }
            found += 1;
        }
    }
    Err("native recursive merge lost an appended entry")
}

fn exact_finish_consuming_array(
    fast: &mut NativeRequestFastState,
    consumed: i64,
    result: Result<i64, &'static str>,
) -> Result<i64, &'static str> {
    let released = fast.discard_owned_direct_value(consumed);
    match (result, released) {
        (Ok(result), Ok(())) => Ok(result),
        (Ok(result), Err(error)) => {
            let _ = fast.discard_owned_direct_value(result);
            Err(error)
        }
        (Err(error), _) => Err(error),
    }
}

fn exact_replace_recursive_owned(
    fast: &mut NativeRequestFastState,
    left_owned: i64,
    right: i64,
    depth: usize,
    active: &mut ExactRecursiveArrayPath,
) -> Result<i64, &'static str> {
    if depth >= EXACT_RECURSIVE_ARRAY_DEPTH_LIMIT {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err("native recursive-array depth exceeded");
    }
    let Some(left_source) = exact_direct_array_range(fast, left_owned) else {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err("native recursive replace accumulator is not a direct array");
    };
    let Some(right_source) = exact_direct_array_range(fast, right) else {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err("native recursive replace operand is not a direct array");
    };
    let pair = (left_source.identity, right_source.identity);
    if active.contains(pair) {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err("native recursive replace encountered a cyclic array graph");
    }
    if let Err(error) = active.push(pair) {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err(error);
    }
    let result = (|| {
        let appended = exact_count_replace_appends(fast, left_source, right_source)?;
        let output_length = left_source
            .length
            .checked_add(appended)
            .ok_or("native recursive replace length overflow")?;
        fast.publish_owned_direct_array_with(output_length, |fast, output_index| {
            if output_index >= left_source.length {
                let entry = exact_nth_replace_append(
                    fast,
                    left_source,
                    right_source,
                    output_index - left_source.length,
                )?;
                return exact_retain_entry(fast, entry);
            }

            let left_entry = left_source
                .get(output_index)
                .ok_or("native recursive replace left range is truncated")?;
            let Some(right_index) =
                exact_find_last_array_key(fast, right_source, left_entry.key)?
            else {
                return exact_retain_entry(fast, left_entry);
            };
            let right_entry = right_source
                .get(right_index)
                .ok_or("native recursive replace right range is truncated")?;
            fast.retain_direct_encoded(left_entry.key)?;
            let replacement = if exact_direct_array_range(fast, left_entry.value).is_some()
                && exact_direct_array_range(fast, right_entry.value).is_some()
            {
                match fast.retain_direct_encoded(left_entry.value) {
                    Ok(()) => exact_replace_recursive_owned(
                        fast,
                        left_entry.value,
                        right_entry.value,
                        depth + 1,
                        active,
                    ),
                    Err(error) => Err(error),
                }
            } else {
                fast.retain_direct_encoded(right_entry.value)
                    .map(|()| right_entry.value)
            };
            match replacement {
                Ok(value) => Ok(php_jit::JitNativeDirectArrayEntry {
                    key: left_entry.key,
                    value,
                }),
                Err(error) => {
                    fast.rollback_direct_retain(left_entry.key);
                    Err(error)
                }
            }
        })
    })();
    active.pop();
    exact_finish_consuming_array(fast, left_owned, result)
}

fn exact_merge_recursive_values(
    fast: &mut NativeRequestFastState,
    left: i64,
    right: i64,
    depth: usize,
    active: &mut ExactRecursiveArrayPath,
) -> Result<i64, &'static str> {
    let left_array = exact_direct_array_range(fast, left).is_some();
    let right_array = exact_direct_array_range(fast, right).is_some();
    match (left_array, right_array) {
        (true, true) => {
            fast.retain_direct_encoded(left)?;
            exact_merge_recursive_owned(fast, left, right, depth + 1, active)
        }
        (true, false) => {
            let left_entries = exact_direct_array_range(fast, left)
                .ok_or("native recursive merge lost its left array")?;
            let output_length = left_entries
                .length
                .checked_add(1)
                .ok_or("native recursive merge length overflow")?;
            let next = exact_next_append_key(fast, left_entries)?;
            fast.publish_owned_direct_array_with(output_length, |fast, index| {
                if index < left_entries.length {
                    return exact_retain_entry(
                        fast,
                        left_entries
                            .get(index)
                            .ok_or("native recursive merge left range is truncated")?,
                    );
                }
                let key = fast.publish_direct_int(next)?;
                if let Err(error) = fast.retain_direct_encoded(right) {
                    let _ = fast.discard_owned_direct_value(key);
                    return Err(error);
                }
                Ok(php_jit::JitNativeDirectArrayEntry { key, value: right })
            })
        }
        (false, true) => {
            let packed = exact_publish_retained_list(fast, &[left])?;
            exact_merge_recursive_owned(fast, packed, right, depth + 1, active)
        }
        (false, false) => exact_publish_retained_list(fast, &[left, right]),
    }
}

fn exact_merge_recursive_owned(
    fast: &mut NativeRequestFastState,
    left_owned: i64,
    right: i64,
    depth: usize,
    active: &mut ExactRecursiveArrayPath,
) -> Result<i64, &'static str> {
    if depth >= EXACT_RECURSIVE_ARRAY_DEPTH_LIMIT {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err("native recursive-array depth exceeded");
    }
    let Some(left_source) = exact_direct_array_range(fast, left_owned) else {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err("native recursive merge accumulator is not a direct array");
    };
    let Some(right_source) = exact_direct_array_range(fast, right) else {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err("native recursive merge operand is not a direct array");
    };
    let pair = (left_source.identity, right_source.identity);
    if active.contains(pair) {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err("native recursive merge encountered a cyclic array graph");
    }
    if let Err(error) = active.push(pair) {
        let _ = fast.discard_owned_direct_value(left_owned);
        return Err(error);
    }
    let result = (|| {
        let appended = exact_count_merge_appends(fast, left_source, right_source)?;
        let output_length = left_source
            .length
            .checked_add(appended)
            .ok_or("native recursive merge length overflow")?;
        let mut next = exact_next_append_key(fast, left_source)?;
        fast.publish_owned_direct_array_with(output_length, |fast, output_index| {
            if output_index < left_source.length {
                let left_entry = left_source
                    .get(output_index)
                    .ok_or("native recursive merge left range is truncated")?;
                if matches!(
                    exact_native_array_key(fast, left_entry.key),
                    Some(ExactNativeArrayKey::String)
                ) && let Some(right_index) =
                    exact_find_last_array_key(fast, right_source, left_entry.key)?
                {
                    let right_entry = right_source
                        .get(right_index)
                        .ok_or("native recursive merge right range is truncated")?;
                    fast.retain_direct_encoded(left_entry.key)?;
                    return match exact_merge_recursive_values(
                        fast,
                        left_entry.value,
                        right_entry.value,
                        depth,
                        active,
                    ) {
                        Ok(value) => Ok(php_jit::JitNativeDirectArrayEntry {
                            key: left_entry.key,
                            value,
                        }),
                        Err(error) => {
                            fast.rollback_direct_retain(left_entry.key);
                            Err(error)
                        }
                    };
                }
                return exact_retain_entry(fast, left_entry);
            }

            let right_entry = exact_nth_merge_append(
                fast,
                left_source,
                right_source,
                output_index - left_source.length,
            )?;
            match exact_native_array_key(fast, right_entry.key)
                .ok_or("native recursive-array key is not an integer or string")?
            {
                ExactNativeArrayKey::String => exact_retain_entry(fast, right_entry),
                ExactNativeArrayKey::Int(_) => {
                    let key_value = next;
                    next = next
                        .checked_add(1)
                        .ok_or("native recursive-array append key overflow")?;
                    let key = fast.publish_direct_int(key_value)?;
                    if let Err(error) = fast.retain_direct_encoded(right_entry.value) {
                        let _ = fast.discard_owned_direct_value(key);
                        return Err(error);
                    }
                    Ok(php_jit::JitNativeDirectArrayEntry {
                        key,
                        value: right_entry.value,
                    })
                }
            }
        })
    })();
    active.pop();
    exact_finish_consuming_array(fast, left_owned, result)
}

pub(crate) extern "C" fn jit_native_array_merge_recursive_abi(
    runtime: *mut NativeRequestFastState,
    left_owned: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request's fast state and
    // transfers exactly one owner for the left accumulator.
    #[allow(unsafe_code)]
    // Safety: generated code transfers the left owner with the active fast state.
    let fast = unsafe { &mut *runtime };
    let mut active = ExactRecursiveArrayPath::new();
    exact_merge_recursive_owned(fast, left_owned, right, 0, &mut active).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_array_replace_recursive_abi(
    runtime: *mut NativeRequestFastState,
    left_owned: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: generated code passes the active request's fast state and
    // transfers exactly one owner for the left accumulator.
    #[allow(unsafe_code)]
    // Safety: generated code transfers the left owner with the active fast state.
    let fast = unsafe { &mut *runtime };
    let mut active = ExactRecursiveArrayPath::new();
    exact_replace_recursive_owned(fast, left_owned, right, 0, &mut active).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}
