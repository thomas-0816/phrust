//! VM-side `settype` builtin: needs the VM cast machinery so conversions,
//! their warnings, and `__toString` dispatch match explicit casts.

use super::prelude::*;

impl Vm {
    pub(super) fn try_execute_settype_builtin(
        &self,
        cursor: ExecutionCursor<'_>,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
    ) -> Option<VmResult> {
        if name != "settype" || values.len() != 2 {
            return None;
        }
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let Value::Reference(cell) = &values[0] else {
            return Some(self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_BY_REF_ARG_NOT_REFERENCEABLE: settype(): Argument #1 ($var) could not be passed by reference".to_owned(),
            ));
        };
        let Value::String(type_name) = effective_value(&values[1]) else {
            return None;
        };
        let normalized = type_name.to_string_lossy().to_ascii_lowercase();
        let kind = match normalized.as_str() {
            "int" | "integer" => Some(CastKind::Int),
            "bool" | "boolean" => Some(CastKind::Bool),
            "float" | "double" => Some(CastKind::Float),
            "string" => Some(CastKind::String),
            "array" => Some(CastKind::Array),
            "object" => Some(CastKind::Object),
            "null" => None,
            "resource" => {
                return Some(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_RUNTIME_BUILTIN_VALUE: Cannot convert to resource type".to_owned(),
                ));
            }
            _ => {
                return Some(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_RUNTIME_BUILTIN_VALUE: settype(): Argument #2 ($type) must be a valid type".to_owned(),
                ));
            }
        };
        let source_span = call_span
            .map(|span| runtime_source_span(compiled, span))
            .unwrap_or_default();
        // Snapshot before the coercion warning: the user error handler may
        // mutate the variable mid-conversion. Scalar targets convert the
        // snapshot; array/object targets wrap whatever the variable holds
        // after the handler ran, mirroring the reference's in-place order.
        let initial = cell.get();
        let initial_is_nan =
            matches!(effective_value(&initial), Value::Float(value) if value.to_f64().is_nan());
        let warn_nan_coercion = |target: &str,
                                 output: &mut OutputBuffer,
                                 stack: &mut CallStack,
                                 state: &mut ExecutionState|
         -> Result<(), Box<VmResult>> {
            self.emit_cast_coercion_warning(
                ExecutionCursor::new(compiled, output, stack, state),
                "E_PHP_RUNTIME_NAN_COERCION_WARNING",
                format!("unexpected NAN value was coerced to {target}"),
                source_span.clone(),
            )
        };
        let converted = match kind {
            None => {
                if initial_is_nan
                    && let Err(result) = warn_nan_coercion("null", output, stack, state)
                {
                    return Some(*result);
                }
                Value::Null
            }
            Some(CastKind::Array) if initial_is_nan => {
                if let Err(result) = warn_nan_coercion("array", output, stack, state) {
                    return Some(*result);
                }
                let mut array = PhpArray::new();
                array.append(effective_value(&cell.get()));
                Value::Array(array)
            }
            Some(CastKind::Object) if initial_is_nan => {
                if let Err(result) = warn_nan_coercion("object", output, stack, state) {
                    return Some(*result);
                }
                // The reference's in-place double branch wraps whatever the
                // variable holds after the handler ran — even null — as the
                // `scalar` property, unlike a plain (object) cast.
                let object = ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
                object.set_property("scalar", effective_value(&cell.get()));
                Value::Object(object)
            }
            Some(kind) => {
                match self.execute_cast(
                    kind,
                    &initial,
                    source_span,
                    ExecutionCursor::new(compiled, output, stack, state),
                ) {
                    Ok(value) => value,
                    Err(result) => return Some(*result),
                }
            }
        };
        cell.set(converted);
        Some(VmResult::success(
            OutputBuffer::new(),
            Some(Value::Bool(true)),
        ))
    }
}
