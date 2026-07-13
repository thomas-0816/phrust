use super::prelude::*;

impl Vm {
    pub(super) fn call_spl_heap_method(
        &self,
        cursor: ExecutionCursor<'_>,
        object: ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
    ) -> Result<Value, SplHeapMethodError> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let class_name = object.class_name();
        let normalized_class =
            spl_runtime_marker(&object).unwrap_or_else(|| normalize_class_name(&class_name));
        let normalized_method = normalize_method_name(method);

        if !matches!(
            normalized_method.as_str(),
            "insert" | "extract" | "next" | "rewind"
        ) || !self
            .spl_object_has_userland_method(compiled, state, &object, "compare")
            .map_err(SplHeapMethodError::Message)?
        {
            return call_spl_heap_method(object, method, args).map_err(SplHeapMethodError::Message);
        }

        if spl_heap_is_modifying(&object) {
            return Err(SplHeapMethodError::Message(spl_heap_modifying_error()));
        }
        if spl_heap_is_corrupted(&object) {
            return Err(SplHeapMethodError::Message(spl_heap_corruption_error()));
        }

        match normalized_method.as_str() {
            "rewind" => {
                validate_spl_iterator_arg_count(&class_name, &args, 0, 0)
                    .map_err(SplHeapMethodError::Message)?;
                spl_set_position(&object, 0);
                Ok(Value::Null)
            }
            "insert" => {
                let expected = if normalized_class == "splpriorityqueue" {
                    2
                } else {
                    1
                };
                validate_spl_iterator_arg_count(&class_name, &args, expected, expected)
                    .map_err(SplHeapMethodError::Message)?;
                let value = if normalized_class == "splpriorityqueue" {
                    spl_priority_queue_entry(args[0].value.clone(), args[1].value.clone())
                } else {
                    args[0].value.clone()
                };
                spl_heap_set_modifying(&object, true);
                let inserted = self.spl_heap_insert_entry_with_userland_compare(
                    ExecutionCursor::new(compiled, output, stack, state),
                    &object,
                    &normalized_class,
                    value,
                );
                spl_heap_set_modifying(&object, false);
                inserted?;
                Ok(Value::Null)
            }
            "extract" => {
                validate_spl_iterator_arg_count(&class_name, &args, 0, 0)
                    .map_err(SplHeapMethodError::Message)?;
                let raw = self.spl_heap_extract_raw_with_userland_compare(
                    compiled,
                    &object,
                    &normalized_class,
                    output,
                    stack,
                    state,
                )?;
                Ok(spl_heap_extract_value(&object, &normalized_class, raw))
            }
            "next" => {
                validate_spl_iterator_arg_count(&class_name, &args, 0, 0)
                    .map_err(SplHeapMethodError::Message)?;
                if !spl_entries(&object).is_empty() {
                    let _ = self.spl_heap_extract_raw_with_userland_compare(
                        compiled,
                        &object,
                        &normalized_class,
                        output,
                        stack,
                        state,
                    )?;
                }
                Ok(Value::Null)
            }
            _ => unreachable!("caller filters heap methods requiring userland compare"),
        }
    }

    pub(super) fn spl_heap_insert_entry_with_userland_compare(
        &self,
        cursor: ExecutionCursor<'_>,
        object: &ObjectRef,
        class_name: &str,
        value: Value,
    ) -> Result<(), SplHeapMethodError> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let normalized_class = normalize_class_name(class_name);
        let mut entries = spl_entries(object);
        entries.push((ArrayKey::Int(entries.len() as i64), value));
        let mut child = entries.len().saturating_sub(1);
        while child > 0 {
            let parent = (child - 1) / 2;
            let ordering = match self.spl_heap_compare_entries_with_userland_compare(
                ExecutionCursor::new(compiled, output, stack, state),
                object,
                &normalized_class,
                &entries[parent].1,
                &entries[child].1,
            ) {
                Ok(ordering) => ordering,
                Err(error) => {
                    spl_reindex_and_set_entries(object, entries);
                    spl_heap_set_corrupted(object, true);
                    return Err(error);
                }
            };
            if ordering != std::cmp::Ordering::Greater {
                break;
            }
            entries.swap(parent, child);
            child = parent;
        }
        spl_reindex_and_set_entries(object, entries);
        Ok(())
    }

    pub(super) fn spl_heap_extract_raw_with_userland_compare(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        class_name: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, SplHeapMethodError> {
        let mut entries = spl_entries(object);
        if entries.is_empty() {
            return Err(SplHeapMethodError::Message(
                "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Can't extract from an empty heap".to_owned(),
            ));
        }
        let (_, raw) = entries.swap_remove(0);
        if !entries.is_empty() {
            spl_heap_set_modifying(object, true);
            let sifted = self.spl_heap_sift_down_with_userland_compare(
                ExecutionCursor::new(compiled, output, stack, state),
                object,
                &mut entries,
                class_name,
                0,
            );
            spl_heap_set_modifying(object, false);
            if let Err(error) = sifted {
                spl_reindex_and_set_entries(object, entries);
                spl_heap_set_corrupted(object, true);
                return Err(error);
            }
        }
        spl_reindex_and_set_entries(object, entries);
        Ok(raw)
    }

    pub(super) fn spl_heap_sift_down_with_userland_compare(
        &self,
        cursor: ExecutionCursor<'_>,
        object: &ObjectRef,
        entries: &mut [(ArrayKey, Value)],
        class_name: &str,
        mut parent: usize,
    ) -> Result<(), SplHeapMethodError> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let normalized_class = normalize_class_name(class_name);
        loop {
            let left = parent.saturating_mul(2).saturating_add(1);
            let right = left.saturating_add(1);
            let mut best = parent;
            if left < entries.len()
                && self.spl_heap_compare_entries_with_userland_compare(
                    ExecutionCursor::new(compiled, output, stack, state),
                    object,
                    &normalized_class,
                    &entries[best].1,
                    &entries[left].1,
                )? == std::cmp::Ordering::Greater
            {
                best = left;
            }
            if right < entries.len()
                && self.spl_heap_compare_entries_with_userland_compare(
                    ExecutionCursor::new(compiled, output, stack, state),
                    object,
                    &normalized_class,
                    &entries[best].1,
                    &entries[right].1,
                )? == std::cmp::Ordering::Greater
            {
                best = right;
            }
            if best == parent {
                break;
            }
            entries.swap(parent, best);
            parent = best;
        }
        Ok(())
    }

    pub(super) fn spl_heap_compare_entries_with_userland_compare(
        &self,
        cursor: ExecutionCursor<'_>,
        object: &ObjectRef,
        class_name: &str,
        left: &Value,
        right: &Value,
    ) -> Result<std::cmp::Ordering, SplHeapMethodError> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let normalized_class = normalize_class_name(class_name);
        let (left_arg, right_arg) = if normalized_class == "splpriorityqueue" {
            (
                spl_priority_queue_entry_parts(left)
                    .map(|(_, priority)| priority)
                    .unwrap_or(Value::Null),
                spl_priority_queue_entry_parts(right)
                    .map(|(_, priority)| priority)
                    .unwrap_or(Value::Null),
            )
        } else {
            (effective_value(left), effective_value(right))
        };
        let result = self
            .call_object_method_value_with_positional_args(
                ExecutionCursor::new(compiled, output, stack, state),
                object.clone(),
                "compare",
                vec![left_arg, right_arg],
            )
            .map_err(|result| SplHeapMethodError::Runtime(Box::new(*result)))?;
        let int = to_int(&result).map_err(SplHeapMethodError::Message)?;
        let ordering = int.cmp(&0);
        Ok(if normalized_class == "splminheap" {
            ordering
        } else {
            ordering.reverse()
        })
    }
}
