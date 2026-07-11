use super::builtin_adapter::{BuiltinTypeError, builtin_source_span};
use super::builtin_array_sort::{
    array_callback_key_value, compare_sort_values, emit_sort_bool_compare_deprecation,
    multisort_array_entries, multisort_duplicate_flag_error, multisort_numeric_values,
    multisort_reference_cell_at, multisort_reorder_entries, natural_compare_bytes,
    sort_argument_is_array, sort_callback_args, sort_callback_ordering, sort_entries_stable,
    sort_numeric_float, sort_reference_cell, sort_string_value,
};
use super::builtin_callback_validation::{array_callback_type_error, validate_array_callback_arg};
use super::prelude::*;

impl Vm {
    pub(super) fn call_array_callback_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let result = match name {
            "array_walk" => {
                self.call_array_walk_builtin(compiled, args, call_span, output, stack, state)
            }
            "array_walk_recursive" => self
                .call_array_walk_recursive_builtin(compiled, args, call_span, output, stack, state),
            _ => {
                let args = match call_args_to_positional(name, args) {
                    Ok(args) => args,
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                match name {
                    "array_map" => {
                        self.call_array_map_builtin(compiled, args, output, stack, state)
                    }
                    "array_filter" => {
                        self.call_array_filter_builtin(compiled, args, output, stack, state)
                    }
                    "array_reduce" => {
                        self.call_array_reduce_builtin(compiled, args, output, stack, state)
                    }
                    "array_any" | "array_all" | "array_find" | "array_find_key" => self
                        .call_array_predicate_builtin(compiled, name, args, output, stack, state),
                    _ => Err(ArrayCallbackError::Message(format!(
                        "E_PHP_VM_UNKNOWN_ARRAY_CALLBACK_BUILTIN: {name}"
                    ))),
                }
            }
        };
        match result {
            Ok(value) => VmResult::success_no_output(Some(value)),
            Err(ArrayCallbackError::Runtime(result)) => *result,
            Err(ArrayCallbackError::BuiltinType { function, actual }) => {
                array_callback_type_error(output, compiled, stack, function, &actual)
            }
            Err(ArrayCallbackError::BuiltinTypeMessage(message)) => BuiltinTypeError {
                output,
                compiled,
                stack,
                state,
                function: name,
                values: &[],
                call_span,
            }
            .result(message),
            Err(ArrayCallbackError::Message(message)) => {
                self.runtime_error(output, compiled, stack, message)
            }
        }
    }

    pub(super) fn call_array_map_builtin(
        &self,
        compiled: &CompiledUnit,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        if args.len() < 2 {
            return Err(ArrayCallbackError::Message(
                "E_PHP_VM_BUILTIN_ARITY: array_map expects at least two argument(s)".to_owned(),
            ));
        }
        let callback = args[0].clone();
        validate_array_callback_arg(compiled, state, "array_map", 1, "callback", true, &callback)?;
        let arrays = args[1..]
            .iter()
            .map(|arg| array_callback_entries("array_map", arg))
            .collect::<Result<Vec<_>, _>>()?;
        let max_len = arrays.iter().map(Vec::len).max().unwrap_or(0);
        let preserve_single_keys = arrays.len() == 1;
        let mut result = PhpArray::new();
        for index in 0..max_len {
            let callback_args = arrays
                .iter()
                .map(|array| {
                    array
                        .get(index)
                        .map_or(Value::Null, |(_, value)| value.clone())
                })
                .collect::<Vec<_>>();
            let mapped = if matches!(callback, Value::Null) {
                if callback_args.len() == 1 {
                    callback_args[0].clone()
                } else {
                    Value::packed_array(callback_args)
                }
            } else {
                self.invoke_array_callback(
                    compiled,
                    callback.clone(),
                    callback_args,
                    output,
                    stack,
                    state,
                )?
            };
            if preserve_single_keys {
                if let Some((key, _)) = arrays[0].get(index) {
                    result.insert(key.clone(), mapped);
                }
            } else {
                result.append(mapped);
            }
        }
        Ok(Value::Array(result))
    }

    pub(super) fn call_array_filter_builtin(
        &self,
        compiled: &CompiledUnit,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        if !(1..=3).contains(&args.len()) {
            return Err(ArrayCallbackError::Message(
                "E_PHP_VM_BUILTIN_ARITY: array_filter expects one to three argument(s)".to_owned(),
            ));
        }
        let entries = array_callback_entries("array_filter", &args[0])?;
        let callback = args.get(1).cloned().unwrap_or(Value::Null);
        let mode = args
            .get(2)
            .map(to_int)
            .transpose()
            .map_err(|message| ArrayCallbackError::Message(format!("array_filter: {message}")))?
            .unwrap_or(0);
        let mut result = PhpArray::new();
        for (key, value) in entries {
            let keep = if matches!(callback, Value::Null) {
                to_bool(&value).map_err(|message| {
                    ArrayCallbackError::Message(format!("array_filter: {message}"))
                })?
            } else {
                let callback_args = match mode {
                    1 => vec![value.clone(), array_callback_key_value(&key)],
                    2 => vec![array_callback_key_value(&key)],
                    _ => vec![value.clone()],
                };
                let predicate = self.invoke_array_callback(
                    compiled,
                    callback.clone(),
                    callback_args,
                    output,
                    stack,
                    state,
                )?;
                to_bool(&predicate).map_err(|message| {
                    ArrayCallbackError::Message(format!("array_filter: {message}"))
                })?
            };
            if keep {
                result.insert(key, value);
            }
        }
        Ok(Value::Array(result))
    }

    pub(super) fn call_array_reduce_builtin(
        &self,
        compiled: &CompiledUnit,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        if !(2..=3).contains(&args.len()) {
            return Err(ArrayCallbackError::Message(
                "E_PHP_VM_BUILTIN_ARITY: array_reduce expects two or three argument(s)".to_owned(),
            ));
        }
        let entries = array_callback_entries("array_reduce", &args[0])?;
        let callback = args[1].clone();
        let mut carry = args.get(2).cloned().unwrap_or(Value::Null);
        for (_, value) in entries {
            carry = self.invoke_array_callback(
                compiled,
                callback.clone(),
                vec![carry, value],
                output,
                stack,
                state,
            )?;
        }
        Ok(carry)
    }

    pub(super) fn call_array_walk_builtin(
        &self,
        compiled: &CompiledUnit,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        if !(2..=3).contains(&args.len()) {
            return Err(ArrayCallbackError::Message(
                "E_PHP_VM_BUILTIN_ARITY: array_walk expects two or three argument(s)".to_owned(),
            ));
        }
        let (array_cell, callback, userdata) = array_walk_args(
            compiled,
            state,
            "array_walk",
            args,
            call_span,
            output,
            stack,
        )?;
        let entries =
            array_walk_reference_entries("array_walk", compiled, state, stack, &array_cell)?;
        for (key, cell) in entries {
            let mut callback_args = vec![Value::Reference(cell), array_callback_key_value(&key)];
            if let Some(userdata) = &userdata {
                callback_args.push(userdata.clone());
            }
            self.invoke_array_callback(
                compiled,
                callback.clone(),
                callback_args,
                output,
                stack,
                state,
            )?;
        }
        Ok(Value::Bool(true))
    }

    pub(super) fn call_array_walk_recursive_builtin(
        &self,
        compiled: &CompiledUnit,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        if !(2..=3).contains(&args.len()) {
            return Err(ArrayCallbackError::Message(
                "E_PHP_VM_BUILTIN_ARITY: array_walk_recursive expects two or three argument(s)"
                    .to_owned(),
            ));
        }
        let (array_cell, callback, userdata) = array_walk_args(
            compiled,
            state,
            "array_walk_recursive",
            args,
            call_span,
            output,
            stack,
        )?;
        self.walk_recursive_value(
            compiled, array_cell, callback, userdata, output, stack, state,
        )?;
        Ok(Value::Bool(true))
    }

    pub(super) fn walk_recursive_value(
        &self,
        compiled: &CompiledUnit,
        cell: ReferenceCell,
        callback: Value,
        userdata: Option<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), ArrayCallbackError> {
        for (key, entry_cell) in
            array_walk_reference_entries("array_walk_recursive", compiled, state, stack, &cell)?
        {
            if matches!(
                callable_resolve_reference(entry_cell.get()),
                Value::Array(_)
            ) {
                self.walk_recursive_value(
                    compiled,
                    entry_cell,
                    callback.clone(),
                    userdata.clone(),
                    output,
                    stack,
                    state,
                )?;
                continue;
            }
            let mut callback_args =
                vec![Value::Reference(entry_cell), array_callback_key_value(&key)];
            if let Some(userdata) = &userdata {
                callback_args.push(userdata.clone());
            }
            self.invoke_array_callback(
                compiled,
                callback.clone(),
                callback_args,
                output,
                stack,
                state,
            )?;
        }
        Ok(())
    }

    pub(super) fn call_array_predicate_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        if args.len() != 2 {
            return Err(ArrayCallbackError::Message(format!(
                "E_PHP_VM_BUILTIN_ARITY: {name} expects two argument(s)"
            )));
        }
        let entries = array_callback_entries(name, &args[0])?;
        let callback = args[1].clone();
        if name == "array_all" && entries.is_empty() {
            return Ok(Value::Bool(true));
        }
        for (key, value) in entries {
            let predicate = self.invoke_array_callback(
                compiled,
                callback.clone(),
                vec![value.clone(), array_callback_key_value(&key)],
                output,
                stack,
                state,
            )?;
            let truthy = to_bool(&predicate)
                .map_err(|message| ArrayCallbackError::Message(format!("{name}: {message}")))?;
            match name {
                "array_any" if truthy => return Ok(Value::Bool(true)),
                "array_all" if !truthy => return Ok(Value::Bool(false)),
                "array_find" if truthy => return Ok(value),
                "array_find_key" if truthy => return Ok(array_callback_key_value(&key)),
                _ => {}
            }
        }
        Ok(match name {
            "array_all" => Value::Bool(true),
            "array_any" => Value::Bool(false),
            "array_find" | "array_find_key" => Value::Null,
            _ => Value::Null,
        })
    }

    pub(super) fn invoke_array_callback(
        &self,
        compiled: &CompiledUnit,
        callback: Value,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        let call_args = args.into_iter().map(CallArgument::positional).collect();
        let mut result = self.call_callable_with_by_ref_value_warnings(
            compiled, callback, call_args, output, stack, state,
        );
        if !result.status.is_success() {
            return Err(ArrayCallbackError::Runtime(Box::new(result)));
        }
        if result.fiber_suspension.is_some() {
            return Err(ArrayCallbackError::Message(
                "E_PHP_VM_ARRAY_CALLBACK_FIBER_GAP: suspending inside array callbacks is not implemented"
                    .to_owned(),
            ));
        }
        state.diagnostics.append(&mut result.diagnostics);
        Ok(result.return_value.unwrap_or(Value::Null))
    }

    pub(super) fn resolve_sort_callback(
        &self,
        compiled: &CompiledUnit,
        state: &mut ExecutionState,
        callback: &Value,
    ) -> Option<FunctionCallCacheTarget> {
        let name = match callback {
            Value::String(name) => String::from_utf8_lossy(name.as_bytes()).into_owned(),
            Value::Callable(callable) => match callable.as_ref() {
                CallableValue::UserFunction { name } => name.clone(),
                _ => {
                    self.record_counter_sort_callback("fallback", Some("closure_or_complex"));
                    return None;
                }
            },
            _ => {
                self.record_counter_sort_callback("fallback", Some("closure_or_complex"));
                return None;
            }
        };
        if name.contains("::") {
            self.record_counter_sort_callback("fallback", Some("method_callable"));
            return None;
        }
        let lowered = normalize_function_name(&name);
        let resolved = self.resolve_function_call_target(compiled, state, &lowered);
        if resolved.is_some() {
            self.record_counter_sort_callback("resolved", None);
        } else {
            self.record_counter_sort_callback("fallback", Some("unresolved_name"));
        }
        resolved
    }

    pub(super) fn invoke_resolved_sort_callback(
        &self,
        compiled: &CompiledUnit,
        target: &FunctionCallCacheTarget,
        args: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        let call_args = args.into_iter().map(CallArgument::positional).collect();
        let mut result = self.execute_function_call_target(
            compiled,
            target.clone(),
            call_args,
            None,
            None,
            output,
            stack,
            state,
            &None,
        );
        if !result.status.is_success() {
            return Err(ArrayCallbackError::Runtime(Box::new(result)));
        }
        if result.fiber_suspension.is_some() {
            return Err(ArrayCallbackError::Message(
                "E_PHP_VM_ARRAY_CALLBACK_FIBER_GAP: suspending inside array callbacks is not implemented"
                    .to_owned(),
            ));
        }
        self.record_counter_sort_callback("direct", None);
        state.diagnostics.append(&mut result.diagnostics);
        Ok(result.return_value.unwrap_or(Value::Null))
    }

    pub(super) fn call_array_sort_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let result = if name == "array_multisort" {
            self.call_array_multisort_builtin_inner(compiled, name, args, output, stack, state)
        } else {
            self.call_array_sort_builtin_inner(compiled, name, args, output, stack, state)
        };
        match result {
            Ok(value) => VmResult::success_no_output(Some(value)),
            Err(ArrayCallbackError::Runtime(result)) => *result,
            Err(ArrayCallbackError::BuiltinType { function, actual }) => {
                array_callback_type_error(output, compiled, stack, function, &actual)
            }
            Err(ArrayCallbackError::BuiltinTypeMessage(message)) => BuiltinTypeError {
                output,
                compiled,
                stack,
                state,
                function: name,
                values: &[],
                call_span: None,
            }
            .result(message),
            Err(ArrayCallbackError::Message(message)) => {
                self.runtime_error(output, compiled, stack, message)
            }
        }
    }

    pub(super) fn call_array_sort_builtin_inner(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        let expects_callback = matches!(name, "usort" | "uasort" | "uksort");
        let max_arity = 2;
        if args.is_empty() || args.len() > max_arity || (expects_callback && args.len() != 2) {
            return Err(ArrayCallbackError::Message(format!(
                "E_PHP_VM_BUILTIN_ARITY: {name} expects {}",
                if expects_callback {
                    "two argument(s)"
                } else {
                    "one or two argument(s)"
                }
            )));
        }
        if args.iter().any(|arg| arg.name.is_some()) {
            return Err(ArrayCallbackError::Message(format!(
                "E_PHP_VM_UNKNOWN_NAMED_ARG: function {name} has no builtin parameter"
            )));
        }

        let mut args = args.into_iter();
        let first = args.next().expect("checked non-empty");
        let cell = sort_reference_cell(compiled, state, name, first, stack)?;
        let Value::Array(array) = cell.get() else {
            return Err(ArrayCallbackError::Message(format!(
                "E_PHP_VM_BUILTIN_TYPE: {name} expects array"
            )));
        };
        let mut entries = array
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        let second = args.next().map(|arg| arg.value);
        let callback = expects_callback.then(|| second.clone()).flatten();
        let flags = if expects_callback {
            SORT_REGULAR
        } else {
            second
                .as_ref()
                .map(to_int)
                .transpose()
                .map_err(|message| ArrayCallbackError::Message(format!("{name}: {message}")))?
                .unwrap_or(SORT_REGULAR)
        };
        let descending = matches!(name, "rsort" | "arsort" | "krsort");
        let resolved_callback = callback
            .as_ref()
            .and_then(|callback| self.resolve_sort_callback(compiled, state, callback));
        let mut bool_compare_deprecated = false;
        sort_entries_stable(&mut entries, |left, right| {
            let (left, right) = if descending {
                (right, left)
            } else {
                (left, right)
            };
            self.compare_sort_entries(
                compiled,
                name,
                callback.as_ref(),
                resolved_callback.as_ref(),
                flags,
                left,
                right,
                output,
                stack,
                state,
                &mut bool_compare_deprecated,
            )
        })?;

        let mut sorted = PhpArray::new();
        let reindex = matches!(name, "sort" | "rsort" | "usort");
        for (key, value) in entries {
            if reindex {
                sorted.append(value);
            } else {
                sorted.insert(key, value);
            }
        }
        cell.set(Value::Array(sorted));
        Ok(Value::Bool(true))
    }

    pub(super) fn call_array_multisort_builtin_inner(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        if args.is_empty() {
            return Err(ArrayCallbackError::Message(format!(
                "E_PHP_VM_BUILTIN_ARITY: {name} expects at least one argument"
            )));
        }
        if args.iter().any(|arg| arg.name.is_some()) {
            return Err(ArrayCallbackError::Message(format!(
                "E_PHP_VM_UNKNOWN_NAMED_ARG: function {name} has no builtin parameter"
            )));
        }

        let mut args = args.into_iter().enumerate().peekable();
        let mut specs = Vec::new();
        while let Some((arg_index, arg)) = args.next() {
            let position = arg_index + 1;
            let cell = multisort_reference_cell_at(compiled, state, name, arg, stack, position)?;
            let entries = multisort_array_entries(name, position, &cell.get())?;
            let mut descending = false;
            let mut flags = SORT_REGULAR;
            let mut order_flag_seen = false;
            let mut sort_flag_seen = false;
            while let Some((_, next)) = args.peek() {
                if sort_argument_is_array(compiled, state, next, stack)? {
                    break;
                }
                let (flag_index, flag_arg) = args.next().expect("peeked argument");
                let flag_position = flag_index + 1;
                let Value::Int(flag) = flag_arg.value else {
                    return Err(ArrayCallbackError::Message(format!(
                        "E_PHP_RUNTIME_BUILTIN_TYPE: {name}(): Argument #{flag_position} must be an array or a sort flag"
                    )));
                };
                match flag {
                    SORT_ASC => {
                        if order_flag_seen {
                            return Err(multisort_duplicate_flag_error(name, flag_position));
                        }
                        order_flag_seen = true;
                        descending = false;
                    }
                    SORT_DESC => {
                        if order_flag_seen {
                            return Err(multisort_duplicate_flag_error(name, flag_position));
                        }
                        order_flag_seen = true;
                        descending = true;
                    }
                    SORT_REGULAR | SORT_NUMERIC | SORT_STRING | SORT_LOCALE_STRING
                    | SORT_NATURAL => {
                        if sort_flag_seen {
                            return Err(multisort_duplicate_flag_error(name, flag_position));
                        }
                        sort_flag_seen = true;
                        flags = flag;
                    }
                    value
                        if value == (SORT_STRING | SORT_FLAG_CASE)
                            || value == (SORT_NATURAL | SORT_FLAG_CASE) =>
                    {
                        if sort_flag_seen {
                            return Err(multisort_duplicate_flag_error(name, flag_position));
                        }
                        sort_flag_seen = true;
                        flags = value;
                    }
                    _ => {
                        return Err(ArrayCallbackError::Message(format!(
                            "E_PHP_RUNTIME_BUILTIN_VALUE: {name}(): Argument #{flag_position} must be a valid sort flag"
                        )));
                    }
                }
            }
            let numeric_values = if flags == SORT_NUMERIC {
                Some(multisort_numeric_values(
                    &entries,
                    output,
                    state,
                    builtin_source_span(compiled, None),
                )?)
            } else {
                None
            };
            specs.push(MultisortArraySpec {
                cell,
                entries,
                numeric_values,
                descending,
                flags,
            });
        }

        let len = specs
            .first()
            .map(|spec| spec.entries.len())
            .expect("checked non-empty");
        if specs.iter().any(|spec| spec.entries.len() != len) {
            return Err(ArrayCallbackError::Message(
                "E_PHP_RUNTIME_BUILTIN_VALUE: Array sizes are inconsistent".to_string(),
            ));
        }

        let mut order = (0..len).collect::<Vec<_>>();
        for index in 1..order.len() {
            let mut current = index;
            while current > 0
                && self
                    .multisort_compare_indices(
                        compiled,
                        &specs,
                        order[current - 1],
                        order[current],
                        output,
                        stack,
                        state,
                    )?
                    .is_gt()
            {
                order.swap(current - 1, current);
                current -= 1;
            }
        }

        for spec in specs {
            let sorted = multisort_reorder_entries(&spec.entries, &order);
            spec.cell.set(Value::Array(sorted));
        }

        Ok(Value::Bool(true))
    }

    pub(super) fn compare_sort_entries(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        callback: Option<&Value>,
        resolved_callback: Option<&FunctionCallCacheTarget>,
        flags: i64,
        left: &(ArrayKey, Value),
        right: &(ArrayKey, Value),
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        bool_compare_deprecated: &mut bool,
    ) -> Result<std::cmp::Ordering, ArrayCallbackError> {
        if let Some(callback) = callback {
            let invoke = |vm: &Self,
                          args: Vec<Value>,
                          output: &mut OutputBuffer,
                          stack: &mut CallStack,
                          state: &mut ExecutionState|
             -> Result<Value, ArrayCallbackError> {
                if let Some(target) = resolved_callback {
                    vm.invoke_resolved_sort_callback(compiled, target, args, output, stack, state)
                } else {
                    vm.invoke_array_callback(compiled, callback.clone(), args, output, stack, state)
                }
            };
            let result = invoke(
                self,
                sort_callback_args(name, left, right),
                output,
                stack,
                state,
            )?;
            if let Value::Bool(value) = result {
                emit_sort_bool_compare_deprecation(
                    compiled,
                    name,
                    output,
                    stack,
                    state,
                    bool_compare_deprecated,
                );
                if value {
                    return Ok(std::cmp::Ordering::Greater);
                }
                let reversed = invoke(
                    self,
                    sort_callback_args(name, right, left),
                    output,
                    stack,
                    state,
                )?;
                return sort_callback_ordering(name, reversed, true);
            }
            return sort_callback_ordering(name, result, false);
        }
        let left_value;
        let right_value;
        let (left_sort, right_sort) = if matches!(name, "ksort" | "krsort") {
            left_value = array_callback_key_value(&left.0);
            right_value = array_callback_key_value(&right.0);
            (&left_value, &right_value)
        } else {
            (&left.1, &right.1)
        };
        if matches!(name, "natsort" | "natcasesort") {
            return self.compare_sort_natural_values(
                compiled,
                left_sort,
                right_sort,
                name == "natcasesort",
                output,
                stack,
                state,
            );
        }
        if flags == SORT_REGULAR {
            return self.compare_sort_regular_values(
                compiled, left_sort, right_sort, output, stack, state,
            );
        }
        if flags == SORT_NUMERIC {
            return self.compare_sort_numeric_values(
                left_sort,
                right_sort,
                output,
                state,
                builtin_source_span(compiled, None),
            );
        }
        if matches!(flags & !SORT_FLAG_CASE, SORT_STRING | SORT_LOCALE_STRING) {
            return self.compare_sort_string_values(
                compiled, left_sort, right_sort, flags, output, stack, state,
            );
        }
        compare_sort_values(left_sort, right_sort, flags)
            .map_err(|message| ArrayCallbackError::Message(format!("{name}: {message}")))
    }

    pub(super) fn multisort_compare_indices(
        &self,
        compiled: &CompiledUnit,
        specs: &[MultisortArraySpec],
        left: usize,
        right: usize,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<std::cmp::Ordering, ArrayCallbackError> {
        for spec in specs {
            let ordering = if spec.flags == SORT_REGULAR {
                self.compare_sort_regular_values(
                    compiled,
                    &spec.entries[left].1,
                    &spec.entries[right].1,
                    output,
                    stack,
                    state,
                )?
            } else if let Some(values) = &spec.numeric_values {
                values[left]
                    .partial_cmp(&values[right])
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else if matches!(
                spec.flags & !SORT_FLAG_CASE,
                SORT_STRING | SORT_LOCALE_STRING
            ) {
                self.compare_sort_string_values(
                    compiled,
                    &spec.entries[left].1,
                    &spec.entries[right].1,
                    spec.flags,
                    output,
                    stack,
                    state,
                )?
            } else {
                compare_sort_values(&spec.entries[left].1, &spec.entries[right].1, spec.flags)
                    .map_err(|message| {
                        ArrayCallbackError::Message(format!("array_multisort: {message}"))
                    })?
            };
            let ordering = if spec.descending {
                ordering.reverse()
            } else {
                ordering
            };
            if !ordering.is_eq() {
                return Ok(ordering);
            }
        }
        Ok(std::cmp::Ordering::Equal)
    }

    pub(super) fn compare_sort_numeric_values(
        &self,
        left: &Value,
        right: &Value,
        output: &mut OutputBuffer,
        state: &mut ExecutionState,
        source_span: RuntimeSourceSpan,
    ) -> Result<std::cmp::Ordering, ArrayCallbackError> {
        let left = sort_numeric_float(left, output, state, source_span.clone())?;
        let right = sort_numeric_float(right, output, state, source_span)?;
        Ok(left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal))
    }

    pub(super) fn compare_sort_string_values(
        &self,
        compiled: &CompiledUnit,
        left: &Value,
        right: &Value,
        flags: i64,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<std::cmp::Ordering, ArrayCallbackError> {
        let case_insensitive = (flags & SORT_FLAG_CASE) != 0;
        let left = self.sort_string_value_for_compare(
            compiled,
            left,
            case_insensitive,
            output,
            stack,
            state,
        )?;
        let right = self.sort_string_value_for_compare(
            compiled,
            right,
            case_insensitive,
            output,
            stack,
            state,
        )?;
        Ok(left.cmp(&right))
    }

    pub(super) fn sort_string_value_for_compare(
        &self,
        compiled: &CompiledUnit,
        value: &Value,
        case_insensitive: bool,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<String, ArrayCallbackError> {
        let mut text = self
            .value_to_string(compiled, value, output, stack, state)
            .map_err(|result| ArrayCallbackError::Runtime(Box::new(result)))?
            .to_string_lossy();
        if case_insensitive {
            text = text.to_ascii_lowercase();
        }
        Ok(text)
    }

    pub(super) fn compare_sort_natural_values(
        &self,
        compiled: &CompiledUnit,
        left: &Value,
        right: &Value,
        case_insensitive: bool,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<std::cmp::Ordering, ArrayCallbackError> {
        let left =
            self.sort_natural_string_value(compiled, left, case_insensitive, output, stack, state)?;
        let right = self.sort_natural_string_value(
            compiled,
            right,
            case_insensitive,
            output,
            stack,
            state,
        )?;
        Ok(natural_compare_bytes(left.as_bytes(), right.as_bytes()))
    }

    pub(super) fn sort_natural_string_value(
        &self,
        compiled: &CompiledUnit,
        value: &Value,
        case_insensitive: bool,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<String, ArrayCallbackError> {
        let text = match value {
            Value::Reference(cell) => {
                return self.sort_natural_string_value(
                    compiled,
                    &cell.get(),
                    case_insensitive,
                    output,
                    stack,
                    state,
                );
            }
            Value::Object(_) => self
                .value_to_string(compiled, value, output, stack, state)
                .map_err(|result| ArrayCallbackError::Runtime(Box::new(result)))?
                .to_string_lossy(),
            other => sort_string_value(other, false),
        };
        Ok(if case_insensitive {
            text.to_ascii_lowercase()
        } else {
            text
        })
    }

    pub(super) fn compare_sort_regular_values(
        &self,
        compiled: &CompiledUnit,
        left: &Value,
        right: &Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<std::cmp::Ordering, ArrayCallbackError> {
        let left = Self::sort_regular_comparable_value(left);
        let right = Self::sort_regular_comparable_value(right);
        if matches!((&left, &right), (Value::Object(_), Value::Object(_))) {
            return compare(&left, &right)
                .map_err(|message| ArrayCallbackError::Message(format!("sort: {message}")));
        }
        let left =
            self.sort_regular_mixed_comparable_value(compiled, &left, output, stack, state)?;
        let right =
            self.sort_regular_mixed_comparable_value(compiled, &right, output, stack, state)?;
        compare(&left, &right)
            .map_err(|message| ArrayCallbackError::Message(format!("sort: {message}")))
    }

    pub(super) fn sort_regular_comparable_value(value: &Value) -> Value {
        match value {
            Value::Reference(cell) => Self::sort_regular_comparable_value(&cell.get()),
            other => other.clone(),
        }
    }

    pub(super) fn sort_regular_mixed_comparable_value(
        &self,
        compiled: &CompiledUnit,
        value: &Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        match value {
            Value::Object(object) if object_has_public_to_string(compiled, object) => self
                .object_to_string(compiled, object.clone(), output, stack, state)
                .map(Value::String)
                .map_err(|result| ArrayCallbackError::Runtime(Box::new(result))),
            other => Ok(other.clone()),
        }
    }
}
