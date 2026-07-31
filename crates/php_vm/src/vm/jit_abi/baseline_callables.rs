//! Baseline-only materialized callable compatibility.
//!
//! Prepared fixed calls and native closure plans never enter this module.
//! Dynamic PHP callable shapes cross this explicit Value boundary once.

use super::*;
use php_runtime::api::Value;

pub(super) fn execute_baseline_acquire_callable(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !matches!(
        instruction.kind,
        php_ir::InstructionKind::AcquireCallable { .. }
    ) {
        return None;
    }
    let Some(value) = arguments.first() else {
        return Some(Err("callable value is missing".to_owned()));
    };
    let direct = context.dereference_direct_encoding(*value);
    match context.native_encoded_value_kind(direct) {
        Some(NativeEncodedValueKind::Callable)
            if context.prepared_callable_dispatch(direct).is_some() =>
        {
            return Some(context.retain(direct).map(|()| direct));
        }
        Some(NativeEncodedValueKind::String) => {
            let Some(name) = context.native_string_name_bytes(direct) else {
                return Some(Err("callable string has no native bytes".to_owned()));
            };
            return Some(context.encode_prepared_callable(Box::new(
                php_runtime::api::CallableValue::UserFunction {
                    name: String::from_utf8_lossy(&name).into_owned(),
                },
            )));
        }
        Some(NativeEncodedValueKind::Object) => {
            let Some(object) = context.native_query_object(direct) else {
                return Some(Err("callable object has no native owner".to_owned()));
            };
            return Some(context.encode_prepared_callable(Box::new(
                php_runtime::api::CallableValue::BoundMethod {
                    target: php_runtime::api::CallableMethodTarget::Object(object),
                    method: "__invoke".to_owned(),
                    scope: None,
                },
            )));
        }
        Some(NativeEncodedValueKind::Array) => {
            if let Some(entries) = context.direct_array_entries_for(direct) {
                let mut target = None;
                let mut method = None;
                for entry in entries {
                    match context.native_encoded_int(entry.key) {
                        Some(0) => target = Some(context.dereference_direct_encoding(entry.value)),
                        Some(1) => method = Some(context.dereference_direct_encoding(entry.value)),
                        _ => {}
                    }
                }
                let Some(target) = target else {
                    return Some(Err("callable array target is missing".to_owned()));
                };
                let Some(method) = method
                    .and_then(|method| context.native_string_name_bytes(method))
                    .map(|method| String::from_utf8_lossy(&method).into_owned())
                else {
                    return Some(Err("callable array method must be a string".to_owned()));
                };
                let target = if let Some(object) = context.native_query_object(target) {
                    php_runtime::api::CallableMethodTarget::Object(object)
                } else if let Some(class) = context.native_string_name_bytes(target) {
                    php_runtime::api::CallableMethodTarget::Class(
                        String::from_utf8_lossy(&class).into_owned(),
                    )
                } else {
                    return Some(Err(format!(
                        "callable array target must be object or class-string, {} given",
                        context.native_encoded_type_name(target)
                    )));
                };
                return Some(context.encode_prepared_callable(Box::new(
                    php_runtime::api::CallableValue::BoundMethod {
                        target,
                        method,
                        scope: None,
                    },
                )));
            }
        }
        _ => {}
    }
    // Baseline-only compatibility values may still reach acquisition from a
    // materialized ReferenceCell. Direct producers above never decode.
    let value = match context.decode_baseline_value(*value) {
        Ok(value) => dereference_native_callable_value(value),
        Err(error) => return Some(Err(error)),
    };
    let callable = match value {
        Value::Callable(callable) => {
            return Some(context.encode_baseline_value(Value::Callable(callable)));
        }
        Value::String(name) => php_runtime::api::CallableValue::UserFunction {
            name: name.to_string_lossy(),
        },
        Value::Object(object) => php_runtime::api::CallableValue::BoundMethod {
            target: php_runtime::api::CallableMethodTarget::Object(object),
            method: "__invoke".to_owned(),
            scope: None,
        },
        Value::Array(array) => {
            let target = array
                .get(&php_runtime::api::ArrayKey::Int(0))
                .cloned()
                .map(dereference_native_callable_value)
                .ok_or_else(|| "callable array target is missing".to_owned());
            let method = array
                .get(&php_runtime::api::ArrayKey::Int(1))
                .cloned()
                .map(dereference_native_callable_value)
                .ok_or_else(|| "callable array method is missing".to_owned());
            let (target, method) = match (target, method) {
                (Ok(target), Ok(Value::String(method))) => (target, method.to_string_lossy()),
                (Err(error), _) | (_, Err(error)) => return Some(Err(error)),
                _ => return Some(Err("callable array method must be a string".to_owned())),
            };
            let target = match target {
                Value::Object(object) => php_runtime::api::CallableMethodTarget::Object(object),
                Value::String(class) => {
                    php_runtime::api::CallableMethodTarget::Class(class.to_string_lossy())
                }
                value => {
                    return Some(Err(format!(
                        "callable array target must be object or class-string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            php_runtime::api::CallableValue::BoundMethod {
                target,
                method,
                scope: None,
            }
        }
        other => {
            return Some(Err(format!(
                "{} is not callable",
                native_value_type_name(&other)
            )));
        }
    };
    Some(context.encode_baseline_value(Value::Callable(Box::new(callable))))
}

impl<'a> NativeRequestColdState<'a> {
    pub(super) fn encode_prepared_closure(
        &mut self,
        callable: php_runtime::api::CallableValue,
    ) -> Result<i64, String> {
        let closure = match callable {
            php_runtime::api::CallableValue::Closure(closure) => closure,
            _ => unreachable!(),
        };
        if let Some(index) = self.direct_closure_handles.get(&closure.id).copied() {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0
                        && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
                        && slot.payload == closure.id
                })
                .ok_or_else(|| "direct native closure identity points at a dead slot".to_owned())?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "direct native closure refcount overflow".to_owned())?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "direct native closure handle overflow".to_owned())?;
            return Ok(php_jit::jit_encode_typed_runtime_value(
                runtime_index,
                php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
            ));
        }
        let implicit_this = closure
            .bound_this
            .as_ref()
            .map(|object| self.encode_native_object_owner(object.clone()))
            .transpose()?;
        let capture_descriptors = closure
            .captures
            .iter()
            .map(|capture| (capture.name.clone(), capture.reference.is_some()))
            .collect::<Vec<_>>();
        let mut capture_values = Vec::with_capacity(closure.captures.len());
        for capture in &closure.captures {
            let encoded = if capture.name.eq_ignore_ascii_case("this")
                && let Some(object) = &closure.bound_this
            {
                self.encode_native_object_owner(object.clone())
            } else if let Some(reference) = capture.reference() {
                self.encode_native_reference_owner(reference)
            } else {
                self.encode_baseline_value(capture.value().cloned().unwrap_or(Value::Null))
            };
            match encoded {
                Ok(encoded) => capture_values.push(encoded),
                Err(error) => {
                    if let Some(implicit_this) = implicit_this {
                        let _ = self.release(implicit_this);
                    }
                    for capture in capture_values {
                        let _ = self.release(capture);
                    }
                    return Err(error);
                }
            }
        }
        let mut closure = closure;
        closure.bound_this = None;
        closure.captures.clear();
        self.publish_prepared_closure_owned(NativePreparedClosure::new(
            closure,
            Arc::from(capture_descriptors),
            implicit_this,
            capture_values.into_boxed_slice(),
            None,
            false,
            false,
            false,
            false,
        ))
    }

    pub(super) fn publish_prepared_closure_owned(
        &mut self,
        prepared: NativePreparedClosure,
    ) -> Result<i64, String> {
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                if let Some(implicit_this) = prepared.implicit_this {
                    let _ = self.release(implicit_this);
                }
                for capture in prepared.captures.iter().copied() {
                    let _ = self.release(capture);
                }
                return Err(error);
            }
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .expect("direct closure index is bounded by the native value arena");
        let closure_id = prepared.closure.id;
        let owner = Box::into_raw(Box::new(NativePreparedCallableOwner::closure(prepared)));
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE,
            flags: php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION,
            payload: closure_id,
            aux: owner as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.direct_closure_handles.insert(closure_id, index as u32);
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
        ))
    }
}

pub(super) fn execute_baseline_resolve_callable(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::ResolveCallable { callable, .. } = &instruction.kind else {
        return None;
    };
    let name = match callable {
        php_ir::instruction::CallableKind::FunctionName { name } => name,
        php_ir::instruction::CallableKind::MethodPlaceholder { target }
        | php_ir::instruction::CallableKind::UnresolvedDynamic { target } => {
            return Some(Err(format!("E_PHP_THROW:Error:{target}")));
        }
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    let fallback = normalized
        .rsplit_once('\\')
        .map(|(_, basename)| basename.to_owned());
    let exists = context.function_id(&normalized).is_some()
        || context.external_function(&normalized).is_some()
        || context.visible_function_names.contains(&normalized)
        || php_extensions::BuiltinRegistry::new().contains(&normalized)
        || fallback.as_ref().is_some_and(|fallback| {
            context.function_id(fallback).is_some()
                || context.external_function(fallback).is_some()
                || context.visible_function_names.contains(fallback)
                || php_extensions::BuiltinRegistry::new().contains(fallback)
        });
    if !exists {
        return Some(Err(format!(
            "E_PHP_THROW:Error:Call to undefined function {name}()"
        )));
    }
    Some(context.encode_baseline_value(Value::Callable(Box::new(
        php_runtime::api::CallableValue::UserFunction { name: name.clone() },
    ))))
}

/// Cold compatibility for a Closure value that was already materialized by
/// an explicit baseline boundary. Prepared native Closures are rebound by
/// `rebind_prepared_closure` and never enter this Rust `Value` path.
pub(super) fn rebind_baseline_materialized_closure(
    closure: &php_runtime::api::ClosurePayload,
    new_this: Option<Value>,
    new_scope: Option<Value>,
) -> Result<Value, String> {
    let bound_this = match new_this {
        Some(Value::Object(object)) => Some(object),
        Some(Value::Null) | None => None,
        Some(value) => {
            return Err(format!(
                "Closure::bind(): Argument #2 ($newThis) must be of type ?object, {} given",
                native_value_type_name(&value)
            ));
        }
    };
    let scope: Option<std::sync::Arc<str>> = match new_scope {
        Some(Value::Object(object)) => Some(object.display_name().into()),
        Some(Value::String(class)) => {
            let class = class.to_string_lossy();
            (class != "static").then(|| class.into())
        }
        Some(Value::Null) => None,
        Some(value) => {
            return Err(format!(
                "Closure::bind(): Argument #3 ($newScope) must be of type object|string|null, {} given",
                native_value_type_name(&value)
            ));
        }
        None => bound_this
            .as_ref()
            .map(|object| object.display_name().into()),
    };
    let mut context = closure.context.clone();
    if let Some(scope) = scope {
        context.scope_class = Some(scope.clone());
        context.called_class = Some(scope.clone());
        context.declaring_class = Some(scope);
    }
    let rebound = php_runtime::api::ClosurePayload::new(closure.function, closure.captures.clone())
        .with_bound_this(bound_this)
        .with_context(context)
        .with_debug(closure.debug.as_deref().cloned());
    Ok(Value::Callable(Box::new(
        php_runtime::api::CallableValue::Closure(rebound),
    )))
}
