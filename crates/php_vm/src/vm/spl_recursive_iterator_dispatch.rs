use super::prelude::*;

impl Vm {
    pub(super) fn call_spl_recursive_iterator_iterator_method(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let normalized = normalize_method_name(method);
        let previous_position = spl_position(&object);
        let previous_depth = spl_entry_depths(&object)
            .get(previous_position)
            .copied()
            .unwrap_or(0);
        if normalized == "callhaschildren" {
            if let Err(message) = validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0)
            {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
            let Some(iterator) = spl_rii_call_get_children_target(&object) else {
                return Ok(Value::Bool(false));
            };
            let call_target = spl_recursive_caching_inner_iterator(&iterator).unwrap_or(iterator);
            let value = self.call_object_method_value(
                compiled,
                call_target,
                "hasChildren",
                output,
                stack,
                state,
            )?;
            let has_children = to_bool(&value)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
            return Ok(Value::Bool(has_children));
        }
        if normalized == "callgetchildren" {
            if let Err(message) = validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0)
            {
                return Err(self.runtime_error(output, compiled, stack, message));
            }
            let Some(iterator) = spl_rii_call_get_children_target(&object) else {
                return Ok(Value::Null);
            };
            let call_target = spl_recursive_caching_inner_iterator(&iterator).unwrap_or(iterator);
            return self.call_object_method_value(
                compiled,
                call_target,
                "getChildren",
                output,
                stack,
                state,
            );
        }
        if normalized == "next" && !spl_rii_iteration_active(&object) {
            object.set_property("__rii_iteration_active", Value::Bool(true));
            self.call_object_method_value(
                compiled,
                object.clone(),
                "beginIteration",
                output,
                stack,
                state,
            )?;
            let current_depth = spl_entry_depths(&object)
                .get(spl_position(&object))
                .copied()
                .unwrap_or(0);
            if current_depth > 0 && !spl_rii_catches_get_child(&object) {
                self.call_spl_rii_child_hook_at_depth(
                    compiled,
                    &object,
                    "beginChildren",
                    current_depth,
                    "RecursiveIteratorIterator->next",
                    call_span,
                    output,
                    stack,
                    state,
                )?;
                self.call_spl_rii_child_hook_at_depth(
                    compiled,
                    &object,
                    "nextElement",
                    current_depth,
                    "RecursiveIteratorIterator->next",
                    call_span,
                    output,
                    stack,
                    state,
                )?;
            }
            self.call_spl_rii_child_hook(
                compiled,
                &object,
                "callHasChildren",
                "RecursiveIteratorIterator->next",
                call_span,
                output,
                stack,
                state,
            )?;
            if self.call_spl_rii_get_children_transition(
                compiled,
                &object,
                None,
                "RecursiveIteratorIterator->next",
                call_span,
                output,
                stack,
                state,
            )? {
                self.call_spl_rii_child_hook(
                    compiled,
                    &object,
                    "beginChildren",
                    "RecursiveIteratorIterator->next",
                    call_span,
                    output,
                    stack,
                    state,
                )?;
                self.call_spl_rii_child_hook(
                    compiled,
                    &object,
                    "nextElement",
                    "RecursiveIteratorIterator->next",
                    call_span,
                    output,
                    stack,
                    state,
                )?;
            }
        }
        let value =
            call_spl_iterator_method(object.clone(), method, args, &self.options.runtime_context)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?;

        if normalized == "next"
            && spl_runtime_marker(&object).as_deref() == Some("recursiveiteratoriterator")
        {
            spl_rii_skip_pruned_positions(&object);
        }

        match normalized.as_str() {
            "rewind" => {
                if spl_rii_iteration_active(&object) {
                    let active_depths = spl_rii_active_child_depths(&object);
                    for depth in active_depths.into_iter().rev() {
                        let hook_depth = depth.saturating_sub(1);
                        if hook_depth != depth
                            && !spl_rii_child_depth_is_active(&object, hook_depth)
                        {
                            spl_rii_note_active_child_depth(&object, hook_depth);
                        }
                        self.call_spl_rii_child_hook_at_depth(
                            compiled,
                            &object,
                            "endChildren",
                            hook_depth,
                            "RecursiveIteratorIterator->rewind",
                            call_span,
                            output,
                            stack,
                            state,
                        )?;
                    }
                }
                object.set_property("__rii_end_iteration_called", Value::Bool(false));
                object.set_property("__rii_notified_position", Value::Int(-1));
                object.set_property("__rii_pruned_branches", Value::Array(PhpArray::new()));
                object.set_property(
                    "__rii_array_string_warning_positions",
                    Value::Array(PhpArray::new()),
                );
                object.set_property(
                    "__rii_entered_child_positions",
                    Value::Array(PhpArray::new()),
                );
                object.set_property("__rii_active_child_depths", Value::Array(PhpArray::new()));
                object.set_property(
                    "__rii_checked_child_positions",
                    Value::Array(PhpArray::new()),
                );
                object.set_property("__rii_checked_child_results", Value::Array(PhpArray::new()));
                if !spl_rii_iteration_active(&object) {
                    object.set_property("__rii_iteration_active", Value::Bool(true));
                    self.call_object_method_value(
                        compiled,
                        object.clone(),
                        "beginIteration",
                        output,
                        stack,
                        state,
                    )?;
                    if normalize_class_name(&object.class_name()) == "recursiveiteratoriterator"
                        && spl_rii_should_use_direct_root(&object)
                    {
                        object.set_property("__rii_direct_at_root", Value::Bool(true));
                        object.set_property("__rii_direct_root_consumed", Value::Bool(false));
                    }
                }
                if let Some(Value::Object(inner)) = object
                    .get_property("__inner_iterator")
                    .map(|value| effective_value(&value))
                {
                    self.call_object_method_value(compiled, inner, "rewind", output, stack, state)?;
                }
                self.call_spl_rii_child_hook(
                    compiled,
                    &object,
                    "callHasChildren",
                    "RecursiveIteratorIterator->rewind",
                    call_span,
                    output,
                    stack,
                    state,
                )?;
            }
            "valid" => {
                if matches!(effective_value(&value), Value::Bool(true)) {
                    let position = spl_position(&object) as i64;
                    if !spl_rii_pruned_leaf_position(&object, position as usize)
                        && !object
                            .get_property("__rii_last_call_has_children")
                            .is_some_and(|value| {
                                matches!(effective_value(&value), Value::Bool(false))
                            })
                    {
                        self.call_spl_rii_child_hook(
                            compiled,
                            &object,
                            "callHasChildren",
                            "RecursiveIteratorIterator->valid",
                            call_span,
                            output,
                            stack,
                            state,
                        )?;
                    }
                    if spl_rii_notified_position(&object) != Some(position) {
                        object.set_property("__rii_notified_position", Value::Int(position));
                        self.call_object_method_value(
                            compiled,
                            object,
                            "nextElement",
                            output,
                            stack,
                            state,
                        )?;
                    }
                } else if spl_runtime_marker(&object).as_deref()
                    == Some("recursiveiteratoriterator")
                    && spl_position(&object) >= spl_entries(&object).len()
                    && let Some(previous_position) = spl_entries(&object).len().checked_sub(1)
                {
                    self.call_spl_rii_exhausted_valid_at_depth(
                        compiled,
                        &object,
                        previous_position,
                        0,
                        output,
                        stack,
                        state,
                    )?;
                }
            }
            "current" => {
                self.emit_spl_rii_recursive_caching_child_warning_if_needed(
                    compiled, &object, call_span, output, stack, state,
                )?;
            }
            "next" => {
                let current_position = spl_position(&object);
                let current_depth = spl_entry_depths(&object)
                    .get(current_position)
                    .copied()
                    .unwrap_or(0);
                if spl_runtime_marker(&object).as_deref() == Some("recursiveiteratoriterator") {
                    self.call_spl_rii_next_depth_transitions(
                        compiled,
                        &object,
                        previous_position,
                        previous_depth,
                        current_position,
                        current_depth,
                        call_span,
                        output,
                        stack,
                        state,
                    )?;
                    if spl_position(&object) >= spl_entries(&object).len()
                        && !spl_rii_end_iteration_called(&object)
                    {
                        object.set_property("__rii_end_iteration_called", Value::Bool(true));
                        object.set_property("__rii_iteration_active", Value::Bool(false));
                        self.call_object_method_value(
                            compiled,
                            object,
                            "endIteration",
                            output,
                            stack,
                            state,
                        )?;
                    }
                    return Ok(value);
                }
                let branch_changed_at_same_depth = current_position < spl_entries(&object).len()
                    && current_depth == previous_depth
                    && current_depth > 0
                    && spl_rii_sub_iterator_branch_changed(
                        &object,
                        previous_position,
                        current_position,
                        current_depth,
                    );
                if previous_depth > current_depth || branch_changed_at_same_depth {
                    self.call_spl_rii_child_hook(
                        compiled,
                        &object,
                        "endChildren",
                        "RecursiveIteratorIterator->next",
                        call_span,
                        output,
                        stack,
                        state,
                    )?;
                }
                if current_position < spl_entries(&object).len()
                    && (current_depth > previous_depth || branch_changed_at_same_depth)
                {
                    self.call_spl_rii_child_hook_at_depth(
                        compiled,
                        &object,
                        "callHasChildren",
                        previous_depth,
                        "RecursiveIteratorIterator->next",
                        call_span,
                        output,
                        stack,
                        state,
                    )?;
                    if self.call_spl_rii_get_children_transition(
                        compiled,
                        &object,
                        Some(previous_depth),
                        "RecursiveIteratorIterator->next",
                        call_span,
                        output,
                        stack,
                        state,
                    )? {
                        self.call_spl_rii_child_hook(
                            compiled,
                            &object,
                            "beginChildren",
                            "RecursiveIteratorIterator->next",
                            call_span,
                            output,
                            stack,
                            state,
                        )?;
                    }
                }
                if spl_position(&object) >= spl_entries(&object).len()
                    && !spl_rii_end_iteration_called(&object)
                {
                    object.set_property("__rii_end_iteration_called", Value::Bool(true));
                    object.set_property("__rii_iteration_active", Value::Bool(false));
                    self.call_object_method_value(
                        compiled,
                        object,
                        "endIteration",
                        output,
                        stack,
                        state,
                    )?;
                }
            }
            _ => {}
        }

        Ok(value)
    }

    pub(super) fn call_spl_rii_next_depth_transitions(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        previous_position: usize,
        previous_depth: i64,
        current_position: usize,
        current_depth: i64,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        if current_position >= spl_entries(object).len() {
            if previous_depth <= 0 {
                self.call_spl_rii_exhausted_valid_at_depth(
                    compiled,
                    object,
                    previous_position,
                    0,
                    output,
                    stack,
                    state,
                )?;
            }
            for depth in (1..=previous_depth.max(0)).rev() {
                let active_depth = spl_rii_child_depth_is_active(object, depth);
                if active_depth {
                    self.call_spl_rii_exhausted_valid_at_depth(
                        compiled,
                        object,
                        previous_position,
                        depth,
                        output,
                        stack,
                        state,
                    )?;
                }
                self.call_spl_rii_child_hook_at_depth(
                    compiled,
                    object,
                    "endChildren",
                    depth,
                    "RecursiveIteratorIterator->next",
                    call_span,
                    output,
                    stack,
                    state,
                )?;
            }
            return Ok(());
        }

        let changed_level =
            spl_rii_first_changed_iterator_level(object, previous_position, current_position);
        let changed_within_previous = changed_level.is_some_and(|level| level <= previous_depth);
        let branch_changed_at_same_depth = current_depth == previous_depth
            && current_depth > 0
            && spl_rii_sub_iterator_branch_changed(
                object,
                previous_position,
                current_position,
                current_depth,
            );

        if previous_depth > current_depth || branch_changed_at_same_depth || changed_within_previous
        {
            let close_to = if changed_within_previous {
                changed_level.unwrap_or(previous_depth)
            } else {
                current_depth.saturating_add(1)
            }
            .max(1);
            for depth in (close_to..=previous_depth.max(0)).rev() {
                let active_depth = spl_rii_child_depth_is_active(object, depth);
                if active_depth {
                    self.call_spl_rii_exhausted_valid_at_depth(
                        compiled,
                        object,
                        previous_position,
                        depth,
                        output,
                        stack,
                        state,
                    )?;
                }
                self.call_spl_rii_child_hook_at_depth(
                    compiled,
                    object,
                    "endChildren",
                    depth,
                    "RecursiveIteratorIterator->next",
                    call_span,
                    output,
                    stack,
                    state,
                )?;
            }
        }

        let enter_start = if current_depth > previous_depth {
            changed_level
                .filter(|level| *level <= previous_depth)
                .unwrap_or(previous_depth.saturating_add(1))
        } else if branch_changed_at_same_depth {
            current_depth
        } else if changed_within_previous {
            changed_level.unwrap_or(current_depth)
        } else {
            return Ok(());
        }
        .max(1);

        for enter_depth in enter_start..=current_depth.max(0) {
            let parent_depth = enter_depth.saturating_sub(1);
            let has_children = self.call_spl_rii_child_hook_at_depth(
                compiled,
                object,
                "callHasChildren",
                parent_depth,
                "RecursiveIteratorIterator->next",
                call_span,
                output,
                stack,
                state,
            )?;
            if has_children == Some(false) {
                spl_rii_note_pruned_branch(object, current_position, current_depth);
                spl_rii_skip_pruned_positions(object);
                break;
            }
            let checked_key = (current_position as i64).saturating_mul(1024) + parent_depth.max(0);
            if spl_rii_child_hook_checked_result(object, checked_key) == Some(false)
                || object
                    .get_property("__rii_last_call_has_children")
                    .is_some_and(|value| matches!(effective_value(&value), Value::Bool(false)))
            {
                spl_rii_note_pruned_branch(object, current_position, current_depth);
                spl_rii_skip_pruned_positions(object);
                break;
            }
            if !self.call_spl_rii_get_children_transition(
                compiled,
                object,
                Some(parent_depth),
                "RecursiveIteratorIterator->next",
                call_span,
                output,
                stack,
                state,
            )? {
                spl_rii_note_pruned_branch(object, current_position, current_depth);
                spl_rii_skip_branch_at_position(object, current_position, current_depth);
                break;
            }
            self.call_spl_rii_child_hook_at_depth(
                compiled,
                object,
                "beginChildren",
                enter_depth,
                "RecursiveIteratorIterator->next",
                call_span,
                output,
                stack,
                state,
            )?;
        }
        Ok(())
    }

    pub(super) fn call_spl_rii_exhausted_valid_at_depth(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        previous_position: usize,
        depth: i64,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let Some(iterator) = spl_hook_iterators(object)
            .get(previous_position)
            .and_then(|iterators| iterators.get(depth.max(0) as usize))
            .cloned()
        else {
            return Ok(());
        };
        let exhausted = iterator.clone_shallow();
        spl_set_position(&exhausted, spl_entries(&exhausted).len());
        self.call_object_method_value(compiled, exhausted, "valid", output, stack, state)?;
        Ok(())
    }

    pub(super) fn call_spl_rii_get_children_transition(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        depth: Option<i64>,
        builtin_call: &str,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<bool, VmResult> {
        if spl_runtime_marker(object).as_deref() == Some("recursivetreeiterator") {
            return Ok(true);
        }
        if object
            .get_property("__rii_last_call_has_children")
            .is_some_and(|value| matches!(effective_value(&value), Value::Bool(false)))
        {
            return Ok(false);
        }
        object.set_property("__rii_last_call_get_children_failed", Value::Bool(false));
        let has_hook = self
            .spl_object_has_userland_method(compiled, state, object, "callGetChildren")
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let result = if has_hook {
            if let Some(depth) = depth {
                self.call_spl_rii_child_hook_at_depth(
                    compiled,
                    object,
                    "callGetChildren",
                    depth,
                    builtin_call,
                    call_span,
                    output,
                    stack,
                    state,
                )
                .map(|_| ())
            } else {
                self.call_spl_rii_child_hook(
                    compiled,
                    object,
                    "callGetChildren",
                    builtin_call,
                    call_span,
                    output,
                    stack,
                    state,
                )
                .map(|_| ())
            }
        } else {
            if let Some(depth) = depth {
                object.set_property("__rii_hook_depth", Value::Int(depth));
            }
            let result = match spl_rii_call_get_children_target(object) {
                Some(iterator)
                    if {
                        let call_target = spl_recursive_caching_inner_iterator(&iterator)
                            .unwrap_or(iterator.clone());
                        self.spl_object_has_userland_method(
                            compiled,
                            state,
                            &call_target,
                            "getChildren",
                        )
                        .map_err(|message| self.runtime_error(output, compiled, stack, message))?
                    } =>
                {
                    let child = self.call_spl_recursive_iterator_iterator_method(
                        compiled,
                        object.clone(),
                        "callGetChildren",
                        Vec::new(),
                        call_span,
                        output,
                        stack,
                        state,
                    )?;
                    if let Value::Object(child_iterator) = effective_value(&child) {
                        self.call_object_method_value(
                            compiled,
                            child_iterator,
                            "rewind",
                            output,
                            stack,
                            state,
                        )?;
                    }
                    Ok(())
                }
                _ => Ok(()),
            };
            if depth.is_some() {
                object.set_property("__rii_hook_depth", Value::Null);
            }
            result
        };
        match result {
            Ok(()) => {
                if object
                    .get_property("__rii_last_call_get_children_failed")
                    .is_some_and(|value| matches!(effective_value(&value), Value::Bool(true)))
                {
                    Ok(false)
                } else {
                    self.emit_spl_rii_recursive_caching_child_warning_if_needed(
                        compiled, object, call_span, output, stack, state,
                    )?;
                    Ok(true)
                }
            }
            Err(result)
                if spl_rii_catches_get_child(object)
                    && !vm_result_has_php_fatal_output(&result) =>
            {
                state.pending_throw.take();
                state.pending_trace.take();
                object.set_property("__rii_last_call_get_children_failed", Value::Bool(true));
                Ok(false)
            }
            Err(result) => Err(result),
        }
    }

    pub(super) fn call_spl_rii_child_hook(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        hook: &str,
        builtin_call: &str,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Option<bool>, VmResult> {
        let position = spl_position(object);
        let normalized_hook = normalize_method_name(hook);
        match normalized_hook.as_str() {
            "beginchildren" => {
                let entered_key = spl_rii_child_hook_entered_key(object, position);
                if spl_rii_child_hook_was_entered(object, entered_key) {
                    return Ok(None);
                }
                spl_rii_note_child_hook_entered(object, entered_key);
                spl_rii_note_active_child_depth(
                    object,
                    spl_rii_hook_depth(object).unwrap_or_else(|| {
                        spl_entry_depths(object).get(position).copied().unwrap_or(0)
                    }),
                );
            }
            "endchildren" => {
                let depth = spl_rii_hook_depth(object).unwrap_or_else(|| {
                    spl_entry_depths(object).get(position).copied().unwrap_or(0)
                });
                if !spl_rii_child_depth_is_active(object, depth) {
                    let has_begin_hook = self
                        .spl_object_has_userland_method(compiled, state, object, "beginChildren")
                        .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                    if has_begin_hook {
                        return Ok(None);
                    }
                } else {
                    spl_rii_remove_active_child_depth(object, depth);
                }
            }
            "callhaschildren" => {
                let checked_key = spl_rii_child_hook_checked_key(object, position);
                if spl_rii_child_hook_was_checked(object, checked_key) {
                    if let Some(result) = spl_rii_child_hook_checked_result(object, checked_key) {
                        object.set_property("__rii_last_call_has_children", Value::Bool(result));
                        return Ok(Some(result));
                    }
                    return Ok(None);
                }
                spl_rii_note_child_hook_checked(object, checked_key);
                object.set_property("__rii_last_call_has_children", Value::Null);
            }
            _ => {}
        }
        let has_hook = self
            .spl_object_has_userland_method(compiled, state, object, hook)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        if !has_hook {
            if normalized_hook == "callhaschildren" {
                let value = self.call_spl_recursive_iterator_iterator_method(
                    compiled,
                    object.clone(),
                    "callHasChildren",
                    Vec::new(),
                    call_span,
                    output,
                    stack,
                    state,
                )?;
                let result = to_bool(&value).unwrap_or(false);
                object.set_property("__rii_last_call_has_children", Value::Bool(result));
                spl_rii_note_child_hook_checked_result(
                    object,
                    spl_rii_child_hook_checked_key(object, position),
                    result,
                );
                return Ok(Some(result));
            }
            return Ok(None);
        }
        let hook_display = lookup_resolved_method_in_state(
            compiled,
            state,
            &object.display_name(),
            hook,
            current_scope_class(compiled, stack).as_deref(),
        )
        .ok()
        .flatten()
        .map(|resolved| method_display_name(compiled, &resolved.method))
        .unwrap_or_else(|| hook.to_owned());
        let active_call_get_children = normalized_hook == "callgetchildren";
        if active_call_get_children {
            object.set_property("__rii_call_get_children_active", Value::Bool(true));
        }
        let hook_result =
            self.call_object_method_value(compiled, object.clone(), hook, output, stack, state);
        if active_call_get_children {
            object.set_property("__rii_call_get_children_active", Value::Bool(false));
        }
        match hook_result {
            Ok(value) => {
                if normalized_hook == "callhaschildren" {
                    let result = to_bool(&value).unwrap_or(false);
                    object.set_property("__rii_last_call_has_children", Value::Bool(result));
                    spl_rii_note_child_hook_checked_result(
                        object,
                        spl_rii_child_hook_checked_key(object, position),
                        result,
                    );
                    return Ok(Some(result));
                }
                if normalized_hook == "callgetchildren"
                    && spl_rii_catches_get_child(object)
                    && (state.pending_throw.is_some()
                        || matches!(effective_value(&value), Value::Null))
                {
                    state.pending_throw.take();
                    state.pending_trace.take();
                    object.set_property("__rii_last_call_get_children_failed", Value::Bool(true));
                }
                if normalized_hook == "callgetchildren" && !spl_rii_catches_get_child(object) {
                    let valid_child = matches!(
                        effective_value(&value),
                        Value::Object(child)
                            if spl_runtime_marker(&child).as_deref()
                                == Some("recursivearrayiterator")
                    );
                    if !valid_child {
                        let message = Value::string(
                            b"Objects returned by RecursiveIterator::getChildren() must implement RecursiveIterator"
                                .to_vec(),
                        );
                        let throwable =
                            match make_exception_object("UnexpectedValueException", &message) {
                                Ok(object) => Value::Object(object),
                                Err(message) => {
                                    return Err(
                                        self.runtime_error(output, compiled, stack, message)
                                    );
                                }
                            };
                        if let Some(call_span) = call_span {
                            tag_throwable_location(&throwable, compiled, call_span);
                        }
                        state.pending_trace = Some(capture_backtrace_string(compiled, stack));
                        state.pending_throw = Some(throwable);
                        return Err(VmResult::propagating_exception(output.clone()));
                    }
                }
                Ok(None)
            }
            Err(result)
                if spl_rii_catches_get_child(object)
                    && !vm_result_has_php_fatal_output(&result) =>
            {
                let caught_throw =
                    state.pending_throw.is_some() || runtime_error_throwable(&result).is_some();
                state.pending_throw.take();
                state.pending_trace.take();
                if caught_throw && normalized_hook == "callgetchildren" {
                    object.set_property("__rii_last_call_get_children_failed", Value::Bool(true));
                }
                Ok(None)
            }
            Err(result) => {
                if let Some(throwable) = state
                    .pending_throw
                    .take()
                    .or_else(|| runtime_error_throwable(&result))
                {
                    if let Some(call_span) = call_span {
                        state.pending_trace =
                            Some(capture_backtrace_string_with_internal_spl_method_call(
                                compiled,
                                stack,
                                &format!("{}->{hook_display}", object.display_name()),
                                builtin_call,
                                call_span,
                            ));
                    }
                    state.pending_throw = Some(throwable);
                    return Err(VmResult::propagating_exception(output.clone()));
                }
                Err(result)
            }
        }
    }

    pub(super) fn call_spl_rii_child_hook_at_depth(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        hook: &str,
        depth: i64,
        builtin_call: &str,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Option<bool>, VmResult> {
        object.set_property("__rii_hook_depth", Value::Int(depth));
        let result = self.call_spl_rii_child_hook(
            compiled,
            object,
            hook,
            builtin_call,
            call_span,
            output,
            stack,
            state,
        );
        object.set_property("__rii_hook_depth", Value::Null);
        result
    }

    pub(super) fn prepare_spl_iterator_constructor_args(
        &self,
        compiled: &CompiledUnit,
        class_name: &str,
        mut args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Vec<CallArgument>, VmResult> {
        if !matches!(
            normalize_class_name(class_name).as_str(),
            "recursiveiteratoriterator" | "recursivetreeiterator"
        ) {
            return Ok(args);
        }
        let Some(first) = args.first() else {
            return Ok(args);
        };
        let Value::Object(object) = effective_value(&first.value) else {
            return Ok(args);
        };
        if spl_runtime_marker(&object).as_deref() == Some("arrayobject") {
            let inner = self.call_object_method_value(
                compiled,
                object,
                "getIterator",
                output,
                stack,
                state,
            )?;
            args[0].value = inner;
            return Ok(args);
        }
        if spl_runtime_marker(&object).is_some()
            && spl_runtime_marker(&object).as_deref() != Some("arrayobject")
        {
            return Ok(args);
        }
        let is_iterator_aggregate =
            class_is_a_in_state(compiled, state, &object.class_name(), "IteratorAggregate")
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        if !is_iterator_aggregate {
            return Ok(args);
        }
        let inner =
            self.call_object_method_value(compiled, object, "getIterator", output, stack, state)?;
        args[0].value = inner;
        Ok(args)
    }

    pub(super) fn spl_object_has_userland_method(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        object: &ObjectRef,
        method: &str,
    ) -> Result<bool, String> {
        let Some(spl_class) = spl_runtime_marker(object) else {
            return Ok(false);
        };
        let object_display_class = object.display_name();
        if normalize_class_name(&object_display_class) == spl_class {
            return Ok(false);
        }
        let scope = current_scope_class(compiled, &CallStack::new());
        let Some(resolved) = lookup_resolved_method_in_state(
            compiled,
            state,
            &object_display_class,
            method,
            scope.as_deref(),
        )?
        else {
            return Ok(false);
        };
        Ok(internal_runtime_class_entry(&normalize_class_name(&resolved.class.name)).is_none())
    }

    pub(super) fn spl_iterator_chain_has_userland_method(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        object: &ObjectRef,
        method: &str,
    ) -> Result<bool, String> {
        let mut current = object.clone();
        for _ in 0..16 {
            if self.spl_object_has_userland_method(compiled, state, &current, method)?
                || self.object_has_userland_method(compiled, state, &current, method)?
            {
                return Ok(true);
            }
            let Some(next) = spl_inner_iterator_delegation_target(&current) else {
                return Ok(false);
            };
            current = next;
        }
        Ok(false)
    }

    pub(super) fn object_has_userland_method(
        &self,
        compiled: &CompiledUnit,
        state: &ExecutionState,
        object: &ObjectRef,
        method: &str,
    ) -> Result<bool, String> {
        let scope = current_scope_class(compiled, &CallStack::new());
        let Some(resolved) = lookup_resolved_method_in_state(
            compiled,
            state,
            &object.display_name(),
            method,
            scope.as_deref(),
        )?
        else {
            return Ok(false);
        };
        Ok(internal_runtime_class_entry(&normalize_class_name(&resolved.class.name)).is_none())
    }
}
