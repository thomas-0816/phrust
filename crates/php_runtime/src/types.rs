//! Runtime type matching helpers.

use crate::{CallableValue, RuntimeType, Value};

/// Returns true when a runtime value satisfies a declared runtime type.
#[must_use]
pub fn value_matches_runtime_type(value: &Value, runtime_type: &RuntimeType) -> bool {
    if let Value::Reference(cell) = value {
        return value_matches_runtime_type(&cell.get(), runtime_type);
    }
    match runtime_type {
        RuntimeType::Mixed => true,
        RuntimeType::Null => matches!(value, Value::Null),
        RuntimeType::Void => false,
        RuntimeType::Bool => matches!(value, Value::Bool(_)),
        RuntimeType::Int => matches!(value, Value::Int(_)),
        RuntimeType::Float => matches!(value, Value::Float(_) | Value::Int(_)),
        RuntimeType::String => matches!(value, Value::String(_)),
        RuntimeType::Array => matches!(value, Value::Array(_)),
        RuntimeType::Callable => matches!(value, Value::Callable(_)),
        RuntimeType::Iterable => matches!(
            value,
            Value::Array(_) | Value::Object(_) | Value::Fiber(_) | Value::Generator(_)
        ),
        RuntimeType::Object => {
            matches!(
                value,
                Value::Object(_)
                    | Value::Fiber(_)
                    | Value::Generator(_)
                    | Value::Callable(CallableValue::Closure(_))
            )
        }
        RuntimeType::Never => false,
        RuntimeType::False => matches!(value, Value::Bool(false)),
        RuntimeType::True => matches!(value, Value::Bool(true)),
        RuntimeType::Class { name, .. } => {
            matches!(
                value,
                Value::Object(object) if object.class_name().eq_ignore_ascii_case(name)
            ) || matches!(
                value,
                Value::Fiber(_) if name.eq_ignore_ascii_case("Fiber")
            ) || matches!(
                value,
                Value::Generator(_) if name.eq_ignore_ascii_case("Generator")
            )
        }
        RuntimeType::Nullable { inner } => {
            matches!(value, Value::Null) || value_matches_runtime_type(value, inner)
        }
        RuntimeType::Union { members } => members
            .iter()
            .any(|member| value_matches_runtime_type(value, member)),
        RuntimeType::Intersection { members } => members
            .iter()
            .all(|member| value_matches_runtime_type(value, member)),
        RuntimeType::Dnf { clauses } => clauses
            .iter()
            .any(|clause| value_matches_runtime_type(value, clause)),
    }
}

/// Stable display name for runtime type diagnostics.
#[must_use]
pub fn runtime_type_name(runtime_type: &RuntimeType) -> String {
    match runtime_type {
        RuntimeType::Int => "int".to_owned(),
        RuntimeType::Float => "float".to_owned(),
        RuntimeType::String => "string".to_owned(),
        RuntimeType::Array => "array".to_owned(),
        RuntimeType::Callable => "callable".to_owned(),
        RuntimeType::Iterable => "iterable".to_owned(),
        RuntimeType::Object => "object".to_owned(),
        RuntimeType::Bool => "bool".to_owned(),
        RuntimeType::Null => "null".to_owned(),
        RuntimeType::Void => "void".to_owned(),
        RuntimeType::Mixed => "mixed".to_owned(),
        RuntimeType::Never => "never".to_owned(),
        RuntimeType::False => "false".to_owned(),
        RuntimeType::True => "true".to_owned(),
        RuntimeType::Class { name, display_name } => {
            display_name.clone().unwrap_or_else(|| name.clone())
        }
        RuntimeType::Nullable { inner } => format!("?{}", runtime_type_name(inner)),
        RuntimeType::Union { members } => members
            .iter()
            .map(runtime_type_name)
            .collect::<Vec<_>>()
            .join("|"),
        RuntimeType::Intersection { members } => members
            .iter()
            .map(runtime_type_name)
            .collect::<Vec<_>>()
            .join("&"),
        RuntimeType::Dnf { clauses } => clauses
            .iter()
            .map(|clause| match clause {
                RuntimeType::Intersection { .. } => format!("({})", runtime_type_name(clause)),
                _ => runtime_type_name(clause),
            })
            .collect::<Vec<_>>()
            .join("|"),
    }
}

/// Stable display name for runtime values in diagnostics.
#[must_use]
pub fn value_type_name(value: &Value) -> &'static str {
    if let Value::Reference(cell) = value {
        return value_type_name(&cell.get());
    }
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Uninitialized => "uninitialized",
        Value::Array(_) => "array",
        Value::Object(_)
        | Value::Fiber(_)
        | Value::Generator(_)
        | Value::Callable(CallableValue::Closure(_)) => "object",
        Value::Resource(_) => "resource",
        Value::Callable(_) => "callable",
        Value::Reference(_) => unreachable!("references are handled before matching"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClassEntry, ClassFlags, ClosurePayload, ObjectRef};

    #[test]
    fn type_matcher_covers_scalars_nullable_union_and_dnf() {
        assert!(value_matches_runtime_type(
            &Value::Int(1),
            &RuntimeType::Int
        ));
        assert!(value_matches_runtime_type(
            &Value::Int(1),
            &RuntimeType::Float
        ));
        assert!(value_matches_runtime_type(
            &Value::Null,
            &RuntimeType::Nullable {
                inner: Box::new(RuntimeType::String)
            }
        ));
        assert!(value_matches_runtime_type(
            &Value::String("x".into()),
            &RuntimeType::Union {
                members: vec![RuntimeType::Int, RuntimeType::String]
            }
        ));
        assert!(value_matches_runtime_type(
            &Value::Int(1),
            &RuntimeType::Dnf {
                clauses: vec![RuntimeType::String, RuntimeType::Int]
            }
        ));
    }

    #[test]
    fn type_matcher_checks_object_class_names_case_insensitively() {
        let class = ClassEntry {
            name: "Box".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        let object = Value::Object(ObjectRef::new(&class));

        assert!(value_matches_runtime_type(
            &object,
            &RuntimeType::Class {
                name: "box".to_owned(),
                display_name: None,
            }
        ));
        assert!(value_matches_runtime_type(
            &object,
            &RuntimeType::Intersection {
                members: vec![
                    RuntimeType::Object,
                    RuntimeType::Class {
                        name: "Box".to_owned(),
                        display_name: None,
                    }
                ]
            }
        ));
    }

    #[test]
    fn type_matcher_treats_closure_callables_as_objects() {
        let closure = Value::Callable(CallableValue::Closure(ClosurePayload::new(11, Vec::new())));
        let plain_callable = Value::Callable(CallableValue::UserFunction {
            name: "strlen".into(),
        });

        assert!(value_matches_runtime_type(&closure, &RuntimeType::Object));
        assert_eq!(value_type_name(&closure), "object");
        assert!(!value_matches_runtime_type(
            &plain_callable,
            &RuntimeType::Object
        ));
    }

    #[test]
    fn type_names_are_stable_for_diagnostics() {
        assert_eq!(
            runtime_type_name(&RuntimeType::Union {
                members: vec![RuntimeType::Int, RuntimeType::String]
            }),
            "int|string"
        );
        assert_eq!(
            runtime_type_name(&RuntimeType::Dnf {
                clauses: vec![
                    RuntimeType::Intersection {
                        members: vec![
                            RuntimeType::Object,
                            RuntimeType::Class {
                                name: "Box".to_owned(),
                                display_name: None,
                            }
                        ]
                    },
                    RuntimeType::Null
                ]
            }),
            "(object&Box)|null"
        );
        assert_eq!(
            runtime_type_name(&RuntimeType::Class {
                name: "someclass".to_owned(),
                display_name: Some("SomeClass".to_owned()),
            }),
            "SomeClass"
        );
    }
}
