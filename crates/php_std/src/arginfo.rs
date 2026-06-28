//! Arginfo, parameter validation, and builtin coercion support.

use crate::generated::arginfo as generated_arginfo;
use php_runtime::{
    PhpString, RuntimeDiagnostic, RuntimeSeverity, RuntimeSourceSpan, Value,
    numeric_string::{NumericStringKind, NumericStringValue, classify_php_string},
    to_bool, to_float, to_int, to_string,
};

/// PHP builtin coercion mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoercionMode {
    /// Accept only exact runtime types.
    Strict,
    /// Apply PHP-style weak scalar coercions for internal functions.
    Weak,
}

/// Supported arginfo type atom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgType {
    /// `mixed`.
    Mixed,
    /// `null`.
    Null,
    /// `bool`.
    Bool,
    /// `int`.
    Int,
    /// `float`.
    Float,
    /// `string`.
    String,
    /// `array`.
    Array,
    /// `object`.
    Object,
    /// `callable`.
    Callable,
}

impl ArgType {
    /// Stable PHP spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mixed => "mixed",
            Self::Null => "null",
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Float => "float",
            Self::String => "string",
            Self::Array => "array",
            Self::Object => "object",
            Self::Callable => "callable",
        }
    }
}

/// Union-like parameter or return type descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeSpec {
    atoms: Vec<ArgType>,
    nullable: bool,
}

impl TypeSpec {
    /// Creates a non-nullable single type.
    #[must_use]
    pub fn one(atom: ArgType) -> Self {
        Self {
            atoms: vec![atom],
            nullable: atom == ArgType::Mixed || atom == ArgType::Null,
        }
    }

    /// Creates a union-like type.
    #[must_use]
    pub fn union(atoms: impl IntoIterator<Item = ArgType>) -> Self {
        let atoms: Vec<_> = atoms.into_iter().collect();
        let nullable = atoms
            .iter()
            .any(|atom| matches!(atom, ArgType::Mixed | ArgType::Null));
        Self { atoms, nullable }
    }

    /// Marks this type nullable.
    #[must_use]
    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    /// Returns true when null is accepted.
    #[must_use]
    pub const fn is_nullable(&self) -> bool {
        self.nullable
    }

    /// Type atoms.
    #[must_use]
    pub fn atoms(&self) -> &[ArgType] {
        &self.atoms
    }

    /// Stable PHP spelling.
    #[must_use]
    pub fn display(&self) -> String {
        if self.atoms.contains(&ArgType::Mixed) {
            return "mixed".to_owned();
        }
        let mut names = Vec::new();
        for name in self.atoms.iter().map(|atom| atom.as_str()) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let joined = names.join("|");
        if self.nullable && !names.contains(&"null") {
            format!("?{joined}")
        } else {
            joined
        }
    }
}

/// Default parameter value metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefaultValue {
    /// No default value.
    None,
    /// Default `null`.
    Null,
    /// Default bool.
    Bool(bool),
    /// Default int.
    Int(i64),
    /// Default string bytes.
    String(PhpString),
}

/// One parameter descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterInfo {
    name: &'static str,
    type_spec: TypeSpec,
    required: bool,
    variadic: bool,
    by_ref: bool,
    default: DefaultValue,
}

impl ParameterInfo {
    /// Creates a required by-value parameter.
    #[must_use]
    pub fn required(name: &'static str, type_spec: TypeSpec) -> Self {
        Self {
            name,
            type_spec,
            required: true,
            variadic: false,
            by_ref: false,
            default: DefaultValue::None,
        }
    }

    /// Creates an optional by-value parameter.
    #[must_use]
    pub fn optional(name: &'static str, type_spec: TypeSpec, default: DefaultValue) -> Self {
        Self {
            name,
            type_spec,
            required: false,
            variadic: false,
            by_ref: false,
            default,
        }
    }

    /// Marks this parameter variadic.
    #[must_use]
    pub fn variadic(mut self) -> Self {
        self.variadic = true;
        self.required = false;
        self
    }

    /// Marks this parameter by-reference.
    #[must_use]
    pub fn by_ref(mut self) -> Self {
        self.by_ref = true;
        self
    }

    /// Parameter name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Parameter type.
    #[must_use]
    pub const fn type_spec(&self) -> &TypeSpec {
        &self.type_spec
    }

    /// Whether the parameter is required.
    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.required
    }

    /// Whether the parameter is variadic.
    #[must_use]
    pub const fn is_variadic(&self) -> bool {
        self.variadic
    }

    /// Whether the parameter requires by-reference passing.
    #[must_use]
    pub const fn is_by_ref(&self) -> bool {
        self.by_ref
    }

    /// Default value metadata.
    #[must_use]
    pub const fn default(&self) -> &DefaultValue {
        &self.default
    }
}

/// Function signature metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionArgInfo {
    name: &'static str,
    params: Vec<ParameterInfo>,
    return_type: TypeSpec,
}

impl FunctionArgInfo {
    /// Creates function arginfo.
    #[must_use]
    pub fn new(name: &'static str, params: Vec<ParameterInfo>, return_type: TypeSpec) -> Self {
        Self {
            name,
            params,
            return_type,
        }
    }

    /// Creates runtime validation arginfo from generated php-src metadata when
    /// every declared type maps to a scalar/runtime atom this validator can
    /// check without class-table context.
    #[must_use]
    pub fn from_generated(
        metadata: &'static generated_arginfo::GeneratedFunctionMetadata,
    ) -> Option<Self> {
        let mut params = Vec::with_capacity(metadata.params.len());
        for param in metadata.params {
            let type_spec = parse_generated_type(param.type_decl)?;
            let mut info = if param.optional {
                ParameterInfo::optional(param.name, type_spec, DefaultValue::None)
            } else {
                ParameterInfo::required(param.name, type_spec)
            };
            if param.by_ref {
                info = info.by_ref();
            }
            if param.variadic {
                info = info.variadic();
            }
            params.push(info);
        }

        Some(Self::new(
            metadata.name,
            params,
            parse_generated_type(metadata.return_type)
                .unwrap_or_else(|| TypeSpec::one(ArgType::Mixed)),
        ))
    }

    /// Function name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Parameter metadata.
    #[must_use]
    pub fn params(&self) -> &[ParameterInfo] {
        &self.params
    }

    /// Return type metadata.
    #[must_use]
    pub const fn return_type(&self) -> &TypeSpec {
        &self.return_type
    }
}

/// Successful validation output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedArguments {
    values: Vec<Value>,
    diagnostics: Vec<RuntimeDiagnostic>,
}

impl ValidatedArguments {
    /// Coerced values in parameter order.
    #[must_use]
    pub fn values(&self) -> &[Value] {
        &self.values
    }

    /// Non-fatal diagnostics emitted while coercing arguments.
    #[must_use]
    pub fn diagnostics(&self) -> &[RuntimeDiagnostic] {
        &self.diagnostics
    }
}

/// Central argument validator for PHP standard-library builtins.
#[derive(Clone, Debug)]
pub struct ArgumentValidator {
    mode: CoercionMode,
}

impl ArgumentValidator {
    /// Creates a validator.
    #[must_use]
    pub const fn new(mode: CoercionMode) -> Self {
        Self { mode }
    }

    /// Validates and coerces positional arguments.
    pub fn validate(
        &self,
        info: &FunctionArgInfo,
        args: &[Value],
        span: RuntimeSourceSpan,
    ) -> Result<ValidatedArguments, ArginfoError> {
        let required = info
            .params()
            .iter()
            .filter(|param| param.is_required())
            .count();
        let variadic = info.params().last().is_some_and(ParameterInfo::is_variadic);
        if args.len() < required {
            return Err(ArginfoError::type_error(
                "E_PHP_STD_MISSING_ARGUMENT",
                format!(
                    "{}() expects at least {} argument{}, {} given",
                    info.name(),
                    required,
                    plural(required),
                    args.len()
                ),
                span,
            ));
        }
        if !variadic && args.len() > info.params().len() {
            let expected = if required == info.params().len() {
                format!(
                    "{}() expects exactly {} argument{}, {} given",
                    info.name(),
                    info.params().len(),
                    plural(info.params().len()),
                    args.len()
                )
            } else {
                format!(
                    "{}() expects at most {} argument{}, {} given",
                    info.name(),
                    info.params().len(),
                    plural(info.params().len()),
                    args.len()
                )
            };
            return Err(ArginfoError::type_error(
                "E_PHP_STD_TOO_MANY_ARGUMENTS",
                expected,
                span,
            ));
        }

        let mut values = Vec::new();
        let mut diagnostics = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            let param = if let Some(param) = info.params().get(index) {
                param
            } else {
                info.params().last().expect("variadic param exists")
            };
            if self.should_deprecate_null_scalar_coercion(param, arg) {
                diagnostics.push(RuntimeDiagnostic::new(
                    "E_PHP_STD_NULL_SCALAR_ARG",
                    RuntimeSeverity::Deprecation,
                    format!(
                        "{}(): Passing null to parameter #{} (${}) of type {} is deprecated",
                        info.name(),
                        index + 1,
                        param.name(),
                        param.type_spec().display()
                    ),
                    span.clone(),
                    Vec::new(),
                    None,
                ));
            }
            values.push(self.coerce(info.name(), index, param, arg, span.clone())?);
        }

        for param in info.params().iter().skip(args.len()) {
            match param.default() {
                DefaultValue::None => {}
                DefaultValue::Null => values.push(Value::Null),
                DefaultValue::Bool(value) => values.push(Value::Bool(*value)),
                DefaultValue::Int(value) => values.push(Value::Int(*value)),
                DefaultValue::String(value) => values.push(Value::String(value.clone())),
            }
        }

        Ok(ValidatedArguments {
            values,
            diagnostics,
        })
    }

    fn should_deprecate_null_scalar_coercion(&self, param: &ParameterInfo, value: &Value) -> bool {
        if self.mode != CoercionMode::Weak || !matches!(value, Value::Null) {
            return false;
        }
        let type_spec = param.type_spec();
        !type_spec.is_nullable()
            && type_spec.atoms().iter().any(|atom| {
                matches!(
                    atom,
                    ArgType::Bool | ArgType::Int | ArgType::Float | ArgType::String
                )
            })
    }

    fn coerce(
        &self,
        function: &str,
        index: usize,
        param: &ParameterInfo,
        value: &Value,
        span: RuntimeSourceSpan,
    ) -> Result<Value, ArginfoError> {
        if matches!(value, Value::Null) && param.type_spec().is_nullable() {
            return Ok(Value::Null);
        }
        for atom in param.type_spec().atoms() {
            if let Some(value) = match_exact(*atom, value) {
                return Ok(value);
            }
        }
        if self.mode == CoercionMode::Weak {
            if is_int_float_union(param.type_spec()) {
                if let Some(value) = weak_coerce_int_float_union(value) {
                    return Ok(value);
                }
            } else {
                for atom in param.type_spec().atoms() {
                    if let Some(value) = weak_coerce(*atom, value) {
                        return Ok(value);
                    }
                }
            }
        }
        Err(ArginfoError::type_error(
            "E_PHP_STD_TYPE_ERROR",
            format!(
                "{}(): Argument #{} (${}) must be of type {}, {} given",
                function,
                index + 1,
                param.name(),
                param.type_spec().display(),
                value_type(value)
            ),
            span,
        ))
    }
}

/// Arginfo validation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArginfoError {
    diagnostic: Box<RuntimeDiagnostic>,
    class: ArginfoErrorClass,
}

impl ArginfoError {
    /// Creates a TypeError-like validation error.
    #[must_use]
    pub fn type_error(
        id: &'static str,
        message: impl Into<String>,
        span: RuntimeSourceSpan,
    ) -> Self {
        Self {
            diagnostic: Box::new(RuntimeDiagnostic::new(
                id,
                RuntimeSeverity::FatalError,
                message,
                span,
                Vec::new(),
                None,
            )),
            class: ArginfoErrorClass::TypeError,
        }
    }

    /// Creates a ValueError-like validation error.
    #[must_use]
    pub fn value_error(
        id: &'static str,
        message: impl Into<String>,
        span: RuntimeSourceSpan,
    ) -> Self {
        Self {
            diagnostic: Box::new(RuntimeDiagnostic::new(
                id,
                RuntimeSeverity::FatalError,
                message,
                span,
                Vec::new(),
                None,
            )),
            class: ArginfoErrorClass::ValueError,
        }
    }

    /// Error class.
    #[must_use]
    pub const fn class(&self) -> ArginfoErrorClass {
        self.class
    }

    /// Diagnostic.
    #[must_use]
    pub fn diagnostic(&self) -> &RuntimeDiagnostic {
        &self.diagnostic
    }
}

/// PHP error class modeled by arginfo.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArginfoErrorClass {
    /// PHP `TypeError`.
    TypeError,
    /// PHP `ValueError`.
    ValueError,
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn parse_generated_type(type_decl: &str) -> Option<TypeSpec> {
    let mut nullable = false;
    let mut normalized = type_decl.trim();
    if normalized.is_empty() || normalized == "void" || normalized == "never" {
        return Some(TypeSpec::one(ArgType::Mixed));
    }
    if let Some(stripped) = normalized.strip_prefix('?') {
        nullable = true;
        normalized = stripped;
    }

    let mut atoms = Vec::new();
    for atom in normalized.split('|') {
        match atom.trim() {
            "" => return None,
            "mixed" => atoms.push(ArgType::Mixed),
            "null" => {
                nullable = true;
                atoms.push(ArgType::Null);
            }
            "bool" | "false" | "true" => atoms.push(ArgType::Bool),
            "int" => atoms.push(ArgType::Int),
            "float" => atoms.push(ArgType::Float),
            "string" => atoms.push(ArgType::String),
            "array" => atoms.push(ArgType::Array),
            "object" => atoms.push(ArgType::Object),
            "callable" => atoms.push(ArgType::Callable),
            _ => return None,
        }
    }
    if atoms.is_empty() {
        return None;
    }
    let mut spec = TypeSpec::union(atoms);
    if nullable {
        spec = spec.nullable();
    }
    Some(spec)
}

fn match_exact(atom: ArgType, value: &Value) -> Option<Value> {
    match (atom, value) {
        (ArgType::Mixed, value) => Some(value.clone()),
        (ArgType::Null, Value::Null) => Some(Value::Null),
        (ArgType::Bool, Value::Bool(_)) => Some(value.clone()),
        (ArgType::Int, Value::Int(_)) => Some(value.clone()),
        (ArgType::Float, Value::Float(_)) => Some(value.clone()),
        (ArgType::String, Value::String(_)) => Some(value.clone()),
        (ArgType::Array, Value::Array(_)) => Some(value.clone()),
        (ArgType::Object, Value::Object(_)) => Some(value.clone()),
        (ArgType::Callable, Value::Callable(_)) => Some(value.clone()),
        _ => None,
    }
}

fn weak_coerce(atom: ArgType, value: &Value) -> Option<Value> {
    match atom {
        ArgType::Bool => to_bool(value).ok().map(Value::Bool),
        ArgType::Int => to_int(value).ok().map(Value::Int),
        ArgType::Float => to_float(value).ok().map(Value::float),
        ArgType::String if !matches!(value, Value::Resource(_)) => {
            to_string(value).ok().map(Value::String)
        }
        _ => None,
    }
}

fn is_int_float_union(type_spec: &TypeSpec) -> bool {
    type_spec.atoms().contains(&ArgType::Int) && type_spec.atoms().contains(&ArgType::Float)
}

fn weak_coerce_int_float_union(value: &Value) -> Option<Value> {
    match value {
        Value::Null | Value::Bool(_) => to_int(value).ok().map(Value::Int),
        Value::String(string) => {
            let classified = classify_php_string(string);
            match (classified.kind, classified.value?) {
                (NumericStringKind::IntString, NumericStringValue::Int(value)) => {
                    Some(Value::Int(value))
                }
                (NumericStringKind::IntString, NumericStringValue::Float(value))
                | (NumericStringKind::FloatString, NumericStringValue::Float(value)) => {
                    Some(Value::float(value))
                }
                (NumericStringKind::FloatString, NumericStringValue::Int(value)) => {
                    Some(Value::float(value as f64))
                }
                (NumericStringKind::LeadingNumeric | NumericStringKind::NonNumeric, _) => None,
            }
        }
        Value::Reference(cell) => weak_coerce_int_float_union(&cell.get()),
        _ => None,
    }
}

fn value_type(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(true) => "true".to_owned(),
        Value::Bool(false) => "false".to_owned(),
        Value::Int(_) => "int".to_owned(),
        Value::Float(_) => "float".to_owned(),
        Value::String(_) => "string".to_owned(),
        Value::Uninitialized => "uninitialized".to_owned(),
        Value::Array(_) => "array".to_owned(),
        Value::Object(object) => object.display_name(),
        Value::Resource(_) => "resource".to_owned(),
        Value::Fiber(_) => "Fiber".to_owned(),
        Value::Generator(_) => "Generator".to_owned(),
        Value::Callable(_) => "callable".to_owned(),
        Value::Reference(_) => "reference".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::{ClassEntry, ClassFlags, ObjectRef};

    fn span() -> RuntimeSourceSpan {
        RuntimeSourceSpan {
            file: Some("arginfo-fixture.php".to_owned()),
            start: 3,
            end: 9,
        }
    }

    fn sample_info() -> FunctionArgInfo {
        FunctionArgInfo::new(
            "stdlib_sample",
            vec![
                ParameterInfo::required("value", TypeSpec::one(ArgType::String)),
                ParameterInfo::optional("limit", TypeSpec::one(ArgType::Int), DefaultValue::Int(3)),
            ],
            TypeSpec::one(ArgType::Bool),
        )
    }

    fn empty_class(name: &str) -> ClassEntry {
        ClassEntry {
            name: name.to_owned(),
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
        }
    }

    #[test]
    fn validates_missing_args_with_snapshot_message() {
        let error = ArgumentValidator::new(CoercionMode::Weak)
            .validate(&sample_info(), &[], span())
            .expect_err("missing arg");

        assert_eq!(error.class(), ArginfoErrorClass::TypeError);
        assert_eq!(error.diagnostic().id(), "E_PHP_STD_MISSING_ARGUMENT");
        assert_eq!(
            error.diagnostic().message(),
            "stdlib_sample() expects at least 1 argument, 0 given"
        );
        assert_eq!(error.diagnostic().source_span().start, 3);
    }

    #[test]
    fn validates_too_many_args_with_snapshot_message() {
        let error = ArgumentValidator::new(CoercionMode::Weak)
            .validate(
                &sample_info(),
                &[
                    Value::String(PhpString::from("x")),
                    Value::Int(1),
                    Value::Int(2),
                ],
                span(),
            )
            .expect_err("too many args");

        assert_eq!(error.diagnostic().id(), "E_PHP_STD_TOO_MANY_ARGUMENTS");
        assert_eq!(
            error.diagnostic().message(),
            "stdlib_sample() expects at most 2 arguments, 3 given"
        );
    }

    #[test]
    fn validates_too_many_args_for_fixed_arity_with_exactly_message() {
        let info = FunctionArgInfo::new(
            "fixed_sample",
            vec![
                ParameterInfo::required("left", TypeSpec::one(ArgType::String)),
                ParameterInfo::required("right", TypeSpec::one(ArgType::Array)),
            ],
            TypeSpec::one(ArgType::Bool),
        );
        let error = ArgumentValidator::new(CoercionMode::Weak)
            .validate(
                &info,
                &[
                    Value::String(PhpString::from("x")),
                    Value::Array(Default::default()),
                    Value::Int(2),
                ],
                span(),
            )
            .expect_err("too many args");

        assert_eq!(error.diagnostic().id(), "E_PHP_STD_TOO_MANY_ARGUMENTS");
        assert_eq!(
            error.diagnostic().message(),
            "fixed_sample() expects exactly 2 arguments, 3 given"
        );
    }

    #[test]
    fn validates_wrong_type_with_snapshot_message() {
        let error = ArgumentValidator::new(CoercionMode::Strict)
            .validate(&sample_info(), &[Value::Array(Default::default())], span())
            .expect_err("wrong type");

        assert_eq!(error.diagnostic().id(), "E_PHP_STD_TYPE_ERROR");
        assert_eq!(
            error.diagnostic().message(),
            "stdlib_sample(): Argument #1 ($value) must be of type string, array given"
        );
    }

    #[test]
    fn validates_bool_type_names_with_php_truth_values() {
        let error = ArgumentValidator::new(CoercionMode::Strict)
            .validate(&sample_info(), &[Value::Bool(true)], span())
            .expect_err("wrong type");

        assert_eq!(
            error.diagnostic().message(),
            "stdlib_sample(): Argument #1 ($value) must be of type string, true given"
        );

        let error = ArgumentValidator::new(CoercionMode::Strict)
            .validate(&sample_info(), &[Value::Bool(false)], span())
            .expect_err("wrong type");

        assert_eq!(
            error.diagnostic().message(),
            "stdlib_sample(): Argument #1 ($value) must be of type string, false given"
        );
    }

    #[test]
    fn validates_object_type_names_with_class_name() {
        let object = ObjectRef::new_with_display_name(&empty_class("stdclass"), "stdClass");
        let error = ArgumentValidator::new(CoercionMode::Strict)
            .validate(&sample_info(), &[Value::Object(object)], span())
            .expect_err("wrong type");

        assert_eq!(
            error.diagnostic().message(),
            "stdlib_sample(): Argument #1 ($value) must be of type string, stdClass given"
        );
    }

    #[test]
    fn weak_coercion_and_defaults_are_applied_centrally() {
        let validated = ArgumentValidator::new(CoercionMode::Weak)
            .validate(&sample_info(), &[Value::Int(42)], span())
            .expect("validated");

        assert_eq!(
            validated.values(),
            &[Value::String(PhpString::from("42")), Value::Int(3),]
        );
    }

    #[test]
    fn weak_string_coercion_rejects_resources() {
        let mut resources = php_runtime::ResourceTable::new();
        let resource = resources.register_stream(
            php_runtime::StreamFlags::new(true, false, true),
            php_runtime::StreamMetadata::new("plainfile", "stream", "r", "/tmp/example.php"),
        );

        let error = ArgumentValidator::new(CoercionMode::Weak)
            .validate(&sample_info(), &[Value::Resource(resource)], span())
            .expect_err("resources must not weakly coerce to string parameters");

        assert_eq!(
            error.diagnostic().message(),
            "stdlib_sample(): Argument #1 ($value) must be of type string, resource given"
        );
    }

    #[test]
    fn weak_int_float_union_keeps_decimal_numeric_strings_as_float() {
        let metadata = crate::generated::arginfo::function_metadata("range").expect("range");
        let info = FunctionArgInfo::from_generated(metadata).expect("runtime arginfo");
        let validated = ArgumentValidator::new(CoercionMode::Weak)
            .validate(
                &info,
                &[
                    Value::String(PhpString::from("1")),
                    Value::String(PhpString::from("2")),
                    Value::String(PhpString::from("0.1")),
                ],
                span(),
            )
            .expect("validated");

        assert_eq!(validated.values()[2], Value::float(0.1));
    }

    #[test]
    fn weak_int_float_union_rejects_non_numeric_strings() {
        let info = FunctionArgInfo::new(
            "floor",
            vec![ParameterInfo::required(
                "num",
                TypeSpec::union([ArgType::Int, ArgType::Float]),
            )],
            TypeSpec::one(ArgType::Float),
        );

        let error = ArgumentValidator::new(CoercionMode::Weak)
            .validate(&info, &[Value::String(PhpString::from("abc"))], span())
            .expect_err("non-numeric strings must not weakly coerce to zero");

        assert_eq!(
            error.diagnostic().message(),
            "floor(): Argument #1 ($num) must be of type int|float, string given"
        );
    }

    #[test]
    fn generated_function_metadata_builds_runtime_validator_info() {
        let metadata = crate::generated::arginfo::function_metadata("strlen").expect("strlen");
        let info = FunctionArgInfo::from_generated(metadata).expect("runtime arginfo");

        assert_eq!(info.name(), "strlen");
        assert_eq!(info.params().len(), 1);
        assert_eq!(info.params()[0].name(), "string");
        assert_eq!(info.params()[0].type_spec().display(), "string");
        assert_eq!(info.return_type().display(), "int");
    }

    #[test]
    fn generated_arginfo_metadata_is_not_empty() {
        assert_eq!(
            crate::generated::arginfo::GENERATED_FUNCTIONS.len(),
            crate::generated::arginfo::GENERATED_ARGINFO_FUNCTION_COUNT
        );
        assert_eq!(
            crate::generated::arginfo::GENERATED_CLASSES.len(),
            crate::generated::arginfo::GENERATED_ARGINFO_CLASS_COUNT
        );
        assert_eq!(
            crate::generated::arginfo::GENERATED_METHODS.len(),
            crate::generated::arginfo::GENERATED_ARGINFO_METHOD_COUNT
        );
        assert_eq!(
            crate::generated::arginfo::GENERATED_CONSTANTS.len(),
            crate::generated::arginfo::GENERATED_ARGINFO_CONSTANT_COUNT
        );
        assert!(!crate::generated::arginfo::GENERATED_FUNCTIONS.is_empty());
        assert!(!crate::generated::arginfo::GENERATED_CLASSES.is_empty());
        assert!(!crate::generated::arginfo::GENERATED_METHODS.is_empty());
        assert!(!crate::generated::arginfo::GENERATED_CONSTANTS.is_empty());
    }

    #[test]
    fn generated_arginfo_stable_symbols_resolve() {
        let strlen = crate::generated::arginfo::function_metadata("strlen").expect("strlen");
        assert_eq!(strlen.extension, "core");
        assert_eq!(strlen.params[0].name, "string");

        let range = crate::generated::arginfo::function_metadata("range").expect("range");
        assert_eq!(range.extension, "standard");

        let var_dump = crate::generated::arginfo::function_metadata("var_dump").expect("var_dump");
        assert!(var_dump.params.iter().any(|param| param.variadic));

        let closure = crate::generated::arginfo::class_metadata("Closure").expect("Closure");
        assert_eq!(closure.kind, "class");

        let date_time = crate::generated::arginfo::class_metadata("DateTime").expect("DateTime");
        assert_eq!(date_time.extension, "date");

        let constructor = crate::generated::arginfo::method_metadata("DateTime", "__construct")
            .expect("DateTime::__construct");
        assert_eq!(constructor.class_name, "DateTime");

        let php_version =
            crate::generated::arginfo::constant_metadata(None, "PHP_VERSION").expect("PHP_VERSION");
        assert_eq!(php_version.owner, None);
    }

    #[test]
    fn generated_arginfo_unknown_symbols_return_none() {
        assert!(
            crate::generated::arginfo::function_metadata("__phrust_missing_function__").is_none()
        );
        assert!(crate::generated::arginfo::class_metadata("__PhrustMissingClass").is_none());
        assert!(
            crate::generated::arginfo::method_metadata("__PhrustMissingClass", "missing").is_none()
        );
        assert!(crate::generated::arginfo::method_metadata("DateTime", "missing").is_none());
        assert!(
            crate::generated::arginfo::constant_metadata(None, "__PHRUST_MISSING_CONSTANT__")
                .is_none()
        );
        assert!(
            crate::generated::arginfo::constant_metadata(
                Some("__PhrustMissingClass"),
                "__PHRUST_MISSING_CONSTANT__"
            )
            .is_none()
        );
    }

    #[test]
    fn generated_class_typed_metadata_is_left_to_call_context() {
        let metadata =
            crate::generated::arginfo::function_metadata("collator_sort").expect("collator_sort");

        assert!(FunctionArgInfo::from_generated(metadata).is_none());
    }

    #[test]
    fn union_nullable_variadic_and_by_ref_metadata_are_modelable() {
        let info = FunctionArgInfo::new(
            "stdlib_meta",
            vec![
                ParameterInfo::required(
                    "value",
                    TypeSpec::union([ArgType::String, ArgType::Int]).nullable(),
                )
                .by_ref(),
                ParameterInfo::required("rest", TypeSpec::one(ArgType::Mixed)).variadic(),
            ],
            TypeSpec::one(ArgType::Null),
        );

        assert!(info.params()[0].is_by_ref());
        assert!(info.params()[0].type_spec().is_nullable());
        assert!(info.params()[1].is_variadic());
        assert!(!info.params()[1].is_required());
        assert_eq!(info.return_type().display(), "null");
    }

    #[test]
    fn generated_variadic_tail_does_not_raise_required_arity() {
        let metadata = crate::generated::arginfo::function_metadata("var_dump").expect("var_dump");
        let info = FunctionArgInfo::from_generated(metadata).expect("runtime arginfo");

        assert_eq!(info.params().len(), 2);
        assert!(info.params()[1].is_variadic());
        assert!(!info.params()[1].is_required());

        let result = ArgumentValidator::new(CoercionMode::Weak).validate(
            &info,
            &[Value::Null],
            RuntimeSourceSpan::default(),
        );
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn value_error_class_is_available_for_builtin_range_checks() {
        let error = ArginfoError::value_error(
            "E_PHP_STD_VALUE_ERROR",
            "stdlib_sample(): Argument $limit must be greater than 0",
            span(),
        );

        assert_eq!(error.class(), ArginfoErrorClass::ValueError);
        assert_eq!(error.diagnostic().id(), "E_PHP_STD_VALUE_ERROR");
    }
}
