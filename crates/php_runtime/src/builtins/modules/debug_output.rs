//! Debug-output formatting for core inspection builtins.

use crate::{ArrayKey, CallableValue, ObjectRef, OutputBuffer, PhpArray, Value, value::FloatValue};
use std::collections::BTreeSet;

const INTERNAL_THROWABLE_TRACE_STRING_PROPERTY: &str = "__phrust_trace_string";

pub(in crate::builtins::modules) struct DebugFormatter {
    active_references: BTreeSet<u64>,
    active_arrays: BTreeSet<u64>,
    active_objects: BTreeSet<u64>,
    var_export_saw_recursion: bool,
    /// `serialize_precision` ini value applied to var_dump floats (`-1` selects
    /// the shortest round-trippable representation).
    serialize_precision: i32,
}

impl Default for DebugFormatter {
    fn default() -> Self {
        Self {
            active_references: BTreeSet::new(),
            active_arrays: BTreeSet::new(),
            active_objects: BTreeSet::new(),
            var_export_saw_recursion: false,
            serialize_precision: -1,
        }
    }
}

impl DebugFormatter {
    pub(in crate::builtins::modules) fn with_serialize_precision(serialize_precision: i32) -> Self {
        Self {
            serialize_precision,
            ..Self::default()
        }
    }

    pub(in crate::builtins::modules) const fn var_export_saw_recursion(&self) -> bool {
        self.var_export_saw_recursion
    }

    pub(in crate::builtins::modules) fn write_var_dump_value(
        &mut self,
        output: &mut OutputBuffer,
        value: &Value,
        indent: usize,
    ) {
        match value {
            Value::Null | Value::Uninitialized => output.write_test_str("NULL\n"),
            Value::Bool(true) => output.write_test_str("bool(true)\n"),
            Value::Bool(false) => output.write_test_str("bool(false)\n"),
            Value::Int(value) => output.write_test_str(&format!("int({value})\n")),
            Value::Float(value) => {
                output.write_test_str(&format!(
                    "float({})\n",
                    php_float_debug_string(*value, self.serialize_precision)
                ));
            }
            Value::String(value) => {
                output.write_test_str(&format!("string({}) \"", value.len()));
                output.write_php_string(value);
                output.write_test_str("\"\n");
            }
            Value::Array(array) => {
                let id = array.gc_debug_id();
                if !self.active_arrays.insert(id) {
                    output.write_test_str("*RECURSION*\n");
                    return;
                }
                output.write_test_str(&format!("array({}) {{\n", array.len()));
                for (key, element) in array.iter() {
                    write_indent(output, indent + 2);
                    write_array_key_dump(output, &key);
                    write_indent(output, indent + 2);
                    self.write_var_dump_value(output, element, indent + 2);
                }
                write_indent(output, indent);
                output.write_test_str("}\n");
                self.active_arrays.remove(&id);
            }
            Value::Object(object) => {
                if !self.active_objects.insert(object.id()) {
                    output.write_test_str("*RECURSION*\n");
                    return;
                }
                let properties = php_visible_debug_properties(object);
                output.write_test_str(&format!(
                    "object({})#{} ({}) {{\n",
                    object.display_name(),
                    object.id(),
                    properties.len()
                ));
                for (name, property) in properties {
                    write_indent(output, indent + 2);
                    let label = object.property_debug_label(&name);
                    output.write_test_str(&format!("[{label}]=>\n"));
                    write_indent(output, indent + 2);
                    self.write_var_dump_value(output, &property, indent + 2);
                }
                write_indent(output, indent);
                output.write_test_str("}\n");
                self.active_objects.remove(&object.id());
            }
            Value::Resource(resource) => output.write_test_str(&format!(
                "resource({}) of type ({})\n",
                resource.id().get(),
                resource.resource_type()
            )),
            Value::Fiber(_) => output.write_test_str("object(Fiber)#0 (0) {\n}\n"),
            Value::Generator(_) => output.write_test_str("object(Generator)#0 (0) {\n}\n"),
            Value::Callable(callable) => self.write_callable_var_dump(output, callable, indent),
            Value::Reference(cell) => {
                let id = cell.gc_debug_id();
                if !self.active_references.insert(id) {
                    output.write_test_str("*RECURSION*\n");
                    return;
                }
                if let Value::Array(array) = cell.get()
                    && self.active_arrays.contains(&array.gc_debug_id())
                {
                    output.write_test_str("*RECURSION*\n");
                    self.active_references.remove(&id);
                    return;
                }
                output.write_test_str("&");
                self.write_var_dump_value(output, &cell.get(), indent);
                self.active_references.remove(&id);
            }
        }
    }

    pub(in crate::builtins::modules) fn write_debug_zval_dump_value(
        &mut self,
        output: &mut OutputBuffer,
        value: &Value,
        indent: usize,
    ) {
        match value {
            Value::Object(object) => {
                if !self.active_objects.insert(object.id()) {
                    output.write_test_str("*RECURSION*\n");
                    return;
                }
                if let Some(entries) = spl_fixed_array_export_entries(object) {
                    output.write_test_str(&format!(
                        "object({})#{} ({}) refcount({}){{\n",
                        object.display_name(),
                        object.id(),
                        entries.len(),
                        spl_fixed_array_debug_zval_refcount(object)
                    ));
                    for (key, property) in entries {
                        write_indent(output, indent + 2);
                        write_array_key_dump(output, &key);
                        write_indent(output, indent + 2);
                        self.write_debug_zval_dump_value(output, &property, indent + 2);
                    }
                    write_indent(output, indent);
                    output.write_test_str("}\n");
                    self.active_objects.remove(&object.id());
                    return;
                }
                let properties = php_visible_debug_properties(object);
                output.write_test_str(&format!(
                    "object({})#{} ({}) refcount({}){{\n",
                    object.display_name(),
                    object.id(),
                    properties.len(),
                    object.gc_refcount_estimate().saturating_add(3)
                ));
                for (name, property) in properties {
                    write_indent(output, indent + 2);
                    let label = object.property_debug_label(&name);
                    output.write_test_str(&format!("[{label}]=>\n"));
                    write_indent(output, indent + 2);
                    self.write_debug_zval_dump_value(output, &property, indent + 2);
                }
                write_indent(output, indent);
                output.write_test_str("}\n");
                self.active_objects.remove(&object.id());
            }
            Value::Reference(cell) => {
                let id = cell.gc_debug_id();
                if !self.active_references.insert(id) {
                    output.write_test_str("*RECURSION*\n");
                    return;
                }
                self.write_debug_zval_dump_value(output, &cell.get(), indent);
                self.active_references.remove(&id);
            }
            value => self.write_var_dump_value(output, value, indent),
        }
    }

    fn write_callable_var_dump(
        &mut self,
        output: &mut OutputBuffer,
        callable: &CallableValue,
        indent: usize,
    ) {
        match callable {
            CallableValue::Closure(payload) if payload.debug.is_some() => {
                let debug = payload.debug.as_ref().expect("checked above");
                let has_static = !payload.captures.is_empty();
                let has_this = payload.bound_this.is_some();
                let has_parameters = !debug.parameters.is_empty();
                let property_count = 3
                    + usize::from(has_parameters)
                    + usize::from(has_static)
                    + usize::from(has_this);
                output.write_test_str(&format!(
                    "object(Closure)#{} ({property_count}) {{\n",
                    payload.id
                ));
                self.write_var_dump_property(
                    output,
                    "name",
                    Value::string(debug.name.clone()),
                    indent,
                );
                self.write_var_dump_property(
                    output,
                    "file",
                    Value::string(debug.file.clone()),
                    indent,
                );
                self.write_var_dump_property(output, "line", Value::Int(debug.line), indent);
                // Reference PHP emits closure debug fields in the order
                // static, this, parameter after name/file/line.
                if has_static {
                    self.write_closure_static_var_dump(
                        output,
                        payload.function,
                        &payload.captures,
                        indent,
                    );
                }
                if let Some(bound_this) = &payload.bound_this {
                    self.write_var_dump_property(
                        output,
                        "this",
                        Value::Object(bound_this.clone()),
                        indent,
                    );
                }
                if has_parameters {
                    self.write_var_dump_property(
                        output,
                        "parameter",
                        closure_parameter_debug_array(&debug.parameters),
                        indent,
                    );
                }
                write_indent(output, indent);
                output.write_test_str("}\n");
            }
            CallableValue::UserFunction { name } | CallableValue::InternalBuiltin { name } => {
                output.write_test_str("object(Closure)#1 (1) {\n");
                self.write_var_dump_property(
                    output,
                    "function",
                    Value::string(name.clone()),
                    indent,
                );
                write_indent(output, indent);
                output.write_test_str("}\n");
            }
            CallableValue::MethodPlaceholder { target } => {
                output.write_test_str("object(Closure)#1 (1) {\n");
                self.write_var_dump_property(
                    output,
                    "function",
                    Value::string(target.clone()),
                    indent,
                );
                write_indent(output, indent);
                output.write_test_str("}\n");
            }
            CallableValue::BoundMethod { target, method, .. } => {
                let class_name = match target {
                    crate::CallableMethodTarget::Object(object) => object.display_name(),
                    crate::CallableMethodTarget::Class(class_name) => class_name.clone(),
                };
                output.write_test_str("object(Closure)#1 (1) {\n");
                self.write_var_dump_property(
                    output,
                    "function",
                    Value::string(format!("{class_name}::{method}")),
                    indent,
                );
                write_indent(output, indent);
                output.write_test_str("}\n");
            }
            CallableValue::Closure(payload) => {
                output.write_test_str(&format!("object(Closure)#{} (0) {{\n", payload.id));
                write_indent(output, indent);
                output.write_test_str("}\n");
            }
            CallableValue::UnresolvedDynamic { .. } => {
                output.write_test_str("object(Closure)#1 (0) {\n");
                write_indent(output, indent);
                output.write_test_str("}\n");
            }
        }
    }

    fn write_closure_static_var_dump(
        &mut self,
        output: &mut OutputBuffer,
        function: u32,
        captures: &[crate::ClosureCaptureValue],
        indent: usize,
    ) {
        write_indent(output, indent + 2);
        output.write_test_str("[\"static\"]=>\n");
        write_indent(output, indent + 2);
        output.write_test_str(&format!("array({}) {{\n", captures.len()));
        for capture in captures {
            write_indent(output, indent + 4);
            output.write_test_str(&format!("[\"{}\"]=>\n", capture.name));
            write_indent(output, indent + 4);
            if self.capture_is_self_recursive(function, capture) {
                output.write_test_str("*RECURSION*\n");
                continue;
            }
            let value = capture
                .value()
                .cloned()
                .or_else(|| capture.reference().map(|reference| reference.get()))
                .unwrap_or(Value::Null);
            self.write_var_dump_value(output, &value, indent + 4);
        }
        write_indent(output, indent + 2);
        output.write_test_str("}\n");
    }

    fn capture_is_self_recursive(
        &self,
        function: u32,
        capture: &crate::ClosureCaptureValue,
    ) -> bool {
        let value = capture
            .value()
            .cloned()
            .or_else(|| capture.reference().map(|reference| reference.get()));
        matches!(
            value.as_ref().and_then(Value::as_closure),
            Some(payload) if payload.function == function
        )
    }

    fn write_var_dump_property(
        &mut self,
        output: &mut OutputBuffer,
        name: &str,
        value: Value,
        indent: usize,
    ) {
        write_indent(output, indent + 2);
        output.write_test_str(&format!("[\"{name}\"]=>\n"));
        write_indent(output, indent + 2);
        self.write_var_dump_value(output, &value, indent + 2);
    }

    pub(in crate::builtins::modules) fn write_print_r_value(
        &mut self,
        output: &mut OutputBuffer,
        value: &Value,
        indent: usize,
    ) {
        match value {
            Value::Null | Value::Uninitialized | Value::Bool(false) => {}
            Value::Bool(true) => output.write_test_str("1"),
            Value::Int(value) => output.write_test_str(&value.to_string()),
            Value::Float(value) => {
                output.write_test_str(&php_float_debug_string(*value, self.serialize_precision));
            }
            Value::String(value) => output.write_php_string(value),
            Value::Array(array) => {
                let id = array.gc_debug_id();
                if !self.active_arrays.insert(id) {
                    output.write_test_str("Array\n *RECURSION*");
                    return;
                }
                output.write_test_str("Array\n");
                write_indent(output, indent);
                output.write_test_str("(\n");
                for (key, element) in array.iter() {
                    write_indent(output, indent + 4);
                    write_print_r_key(output, &key);
                    output.write_test_str(" => ");
                    let element_indent = if print_r_value_starts_multiline(element) {
                        indent + 8
                    } else {
                        indent + 4
                    };
                    self.write_print_r_value(output, element, element_indent);
                    output.write_test_str("\n");
                }
                write_indent(output, indent);
                output.write_test_str(")\n");
                self.active_arrays.remove(&id);
            }
            Value::Object(object) => {
                output.write_test_str(&format!("{} Object\n", object.display_name()));
                write_indent(output, indent);
                output.write_test_str("(\n");
                for (name, property) in php_visible_debug_properties(object) {
                    write_indent(output, indent + 4);
                    // print_r annotates visibility as `name`, `name:protected`,
                    // or `name:Class:private` — the var_dump label without quotes.
                    let label = object.property_debug_label(&name).replace('"', "");
                    output.write_test_str(&format!("[{label}] => "));
                    let property_indent = if print_r_value_starts_multiline(&property) {
                        indent + 8
                    } else {
                        indent + 4
                    };
                    self.write_print_r_value(output, &property, property_indent);
                    output.write_test_str("\n");
                }
                write_indent(output, indent);
                output.write_test_str(")\n");
            }
            Value::Resource(resource) => {
                output.write_test_str(&format!("Resource id #{}", resource.id().get()));
            }
            Value::Fiber(_) => output.write_test_str("Fiber Object\n(\n)\n"),
            Value::Generator(_) => output.write_test_str("Generator Object\n(\n)\n"),
            Value::Callable(_) => output.write_test_str("Closure Object\n(\n)\n"),
            Value::Reference(cell) => {
                let id = cell.gc_debug_id();
                if !self.active_references.insert(id) {
                    output.write_test_str("*RECURSION*");
                    return;
                }
                self.write_print_r_value(output, &cell.get(), indent);
                self.active_references.remove(&id);
            }
        }
    }

    pub(in crate::builtins::modules) fn write_var_export_value(
        &mut self,
        output: &mut OutputBuffer,
        value: &Value,
        indent: usize,
    ) {
        match value {
            Value::Null | Value::Uninitialized => output.write_test_str("NULL"),
            Value::Bool(true) => output.write_test_str("true"),
            Value::Bool(false) => output.write_test_str("false"),
            Value::Int(value) => output.write_test_str(&value.to_string()),
            Value::Float(value) => {
                output.write_test_str(&php_float_export_string(*value, self.serialize_precision));
            }
            Value::String(value) => write_export_string(output, &value.to_string_lossy()),
            Value::Array(array) => {
                output.write_test_str("array (\n");
                for (key, element) in array.iter() {
                    write_indent(output, indent + 2);
                    write_export_key(output, &key);
                    output.write_test_str(" => ");
                    if self.var_export_child_starts_multiline(element) {
                        output.write_test_str("\n");
                        write_indent(output, indent + 2);
                    }
                    self.write_var_export_value(output, element, indent + 2);
                    output.write_test_str(",\n");
                }
                write_indent(output, indent);
                output.write_test_str(")");
            }
            Value::Object(object) => {
                if !self.active_objects.insert(object.id()) {
                    self.var_export_saw_recursion = true;
                    output.write_test_str("NULL");
                    return;
                }
                if object.class_name().eq_ignore_ascii_case("stdClass") {
                    output.write_test_str("(object) array(\n");
                    for (name, property) in php_visible_debug_properties(object) {
                        write_indent(output, indent + 3);
                        write_export_string(output, &name);
                        output.write_test_str(" => ");
                        if self.var_export_child_starts_multiline(&property) {
                            output.write_test_str("\n");
                            write_indent(output, indent + 2);
                        }
                        self.write_var_export_value(output, &property, indent + 2);
                        output.write_test_str(",\n");
                    }
                    write_indent(output, indent);
                    output.write_test_str(")");
                    self.active_objects.remove(&object.id());
                    return;
                }
                if let Some(entries) = spl_fixed_array_export_entries(object) {
                    output.write_test_str(&format!(
                        "\\{}::__set_state(array(\n",
                        object.display_name()
                    ));
                    for (key, property) in entries {
                        write_indent(output, indent + 3);
                        write_export_key(output, &key);
                        output.write_test_str(" => ");
                        if self.var_export_child_starts_multiline(&property) {
                            output.write_test_str("\n");
                            write_indent(output, indent + 2);
                        }
                        self.write_var_export_value(output, &property, indent + 2);
                        output.write_test_str(",\n");
                    }
                    write_indent(output, indent);
                    output.write_test_str("))");
                    self.active_objects.remove(&object.id());
                    return;
                }
                output.write_test_str(&format!(
                    "\\{}::__set_state(array(\n",
                    object.display_name()
                ));
                for (name, property) in php_visible_debug_properties(object) {
                    write_indent(output, indent + 3);
                    write_export_string(output, &name);
                    output.write_test_str(" => ");
                    if self.var_export_child_starts_multiline(&property) {
                        output.write_test_str("\n");
                        write_indent(output, indent + 2);
                    }
                    self.write_var_export_value(output, &property, indent + 2);
                    output.write_test_str(",\n");
                }
                write_indent(output, indent);
                output.write_test_str("))");
                self.active_objects.remove(&object.id());
            }
            Value::Resource(resource) => {
                output.write_test_str(&format!("NULL /* resource #{} */", resource.id().get()));
            }
            Value::Fiber(_) => output.write_test_str("Fiber::__set_state(array(\n))"),
            Value::Generator(_) => output.write_test_str("Generator::__set_state(array(\n))"),
            Value::Callable(_) => output.write_test_str("Closure::__set_state(array(\n))"),
            Value::Reference(cell) => {
                let id = cell.gc_debug_id();
                if !self.active_references.insert(id) {
                    self.var_export_saw_recursion = true;
                    output.write_test_str("NULL");
                    return;
                }
                self.write_var_export_value(output, &cell.get(), indent);
                self.active_references.remove(&id);
            }
        }
    }

    fn var_export_child_starts_multiline(&self, value: &Value) -> bool {
        match value {
            Value::Object(object) if self.active_objects.contains(&object.id()) => false,
            Value::Reference(cell) => {
                let value = cell.get();
                self.var_export_child_starts_multiline(&value)
            }
            _ => var_export_value_starts_multiline(value),
        }
    }
}

fn spl_fixed_array_export_entries(object: &ObjectRef) -> Option<Vec<(ArrayKey, Value)>> {
    if !object_is_spl_fixed_array(object) {
        return None;
    }
    let Some(Value::Array(entries)) = object.get_property("__entries") else {
        return Some(Vec::new());
    };
    Some(
        entries
            .iter()
            .filter_map(|(_, entry)| {
                let Value::Array(pair) = debug_deref_value(entry) else {
                    return None;
                };
                let key = pair
                    .get(&ArrayKey::Int(0))
                    .map(debug_deref_value)
                    .as_ref()
                    .and_then(ArrayKey::from_value)?;
                let value = pair.get(&ArrayKey::Int(1)).cloned().unwrap_or(Value::Null);
                Some((key, value))
            })
            .collect(),
    )
}

fn object_is_spl_fixed_array(object: &ObjectRef) -> bool {
    if object.class_name().eq_ignore_ascii_case("splfixedarray") {
        return true;
    }
    matches!(
        object.get_property("__spl_runtime_class").as_ref().map(debug_deref_value),
        Some(Value::String(class_name)) if class_name.to_string_lossy().eq_ignore_ascii_case("splfixedarray")
    )
}

fn spl_fixed_array_debug_zval_refcount(object: &ObjectRef) -> usize {
    object.gc_refcount_estimate().min(1).saturating_add(3)
}

fn debug_deref_value(value: &Value) -> Value {
    match value {
        Value::Reference(cell) => cell.get(),
        value => value.clone(),
    }
}

pub(in crate::builtins::modules) fn write_array_key_dump(
    output: &mut OutputBuffer,
    key: &ArrayKey,
) {
    match key {
        ArrayKey::Int(index) => output.write_test_str(&format!("[{index}]=>\n")),
        ArrayKey::String(key) => {
            output.write_test_str(&format!("[\"{}\"]=>\n", key.to_string_lossy()))
        }
    }
}

pub(in crate::builtins::modules) fn var_export_value_starts_multiline(value: &Value) -> bool {
    match value {
        Value::Array(_) | Value::Object(_) => true,
        Value::Reference(cell) => var_export_value_starts_multiline(&cell.get()),
        _ => false,
    }
}

fn php_visible_debug_properties(object: &ObjectRef) -> Vec<(String, Value)> {
    object
        .properties_snapshot()
        .into_iter()
        .filter(|(name, _)| name != INTERNAL_THROWABLE_TRACE_STRING_PROPERTY)
        .collect()
}

pub(in crate::builtins::modules) fn print_r_value_starts_multiline(value: &Value) -> bool {
    match value {
        Value::Array(_) | Value::Object(_) => true,
        Value::Reference(cell) => print_r_value_starts_multiline(&cell.get()),
        _ => false,
    }
}

pub(in crate::builtins::modules) fn write_print_r_key(output: &mut OutputBuffer, key: &ArrayKey) {
    match key {
        ArrayKey::Int(index) => output.write_test_str(&format!("[{index}]")),
        ArrayKey::String(key) => output.write_test_str(&format!("[{}]", key.to_string_lossy())),
    }
}

pub(in crate::builtins::modules) fn write_export_key(output: &mut OutputBuffer, key: &ArrayKey) {
    match key {
        ArrayKey::Int(index) => output.write_test_str(&index.to_string()),
        ArrayKey::String(key) => write_export_string(output, &key.to_string_lossy()),
    }
}

pub(in crate::builtins::modules) fn write_export_string(output: &mut OutputBuffer, text: &str) {
    if text.contains('\0') {
        let mut segments = text.split('\0');
        write_export_single_quoted_string(output, segments.next().unwrap_or_default());
        for segment in segments {
            output.write_test_str(" . \"\\0\" . ");
            write_export_single_quoted_string(output, segment);
        }
        return;
    }
    write_export_single_quoted_string(output, text);
}

pub(in crate::builtins::modules) fn write_export_single_quoted_string(
    output: &mut OutputBuffer,
    text: &str,
) {
    output.write_test_str("'");
    for character in text.chars() {
        match character {
            '\\' => output.write_test_str("\\\\"),
            '\'' => output.write_test_str("\\'"),
            _ => output.write_test_str(&character.to_string()),
        }
    }
    output.write_test_str("'");
}

fn closure_parameter_debug_array(parameters: &[crate::ClosureDebugParameter]) -> Value {
    let mut array = PhpArray::new();
    for parameter in parameters {
        let state = if parameter.required {
            "<required>"
        } else {
            "<optional>"
        };
        array.insert(
            ArrayKey::String(crate::PhpString::from_test_str(&format!(
                "${}",
                parameter.name
            ))),
            Value::string(state),
        );
    }
    Value::Array(array)
}

pub(in crate::builtins::modules) fn php_float_debug_string(
    value: FloatValue,
    serialize_precision: i32,
) -> String {
    let value = value.to_f64();
    if value.is_nan() {
        return "NAN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-INF".to_owned()
        } else {
            "INF".to_owned()
        };
    }

    // serialize_precision >= 1 selects PHP's `%.*G` formatting; -1 (and 0, which
    // PHP maps to the shortest mode here) selects the shortest round-trip form.
    if serialize_precision >= 1 {
        return php_gcvt(value, serialize_precision as usize);
    }

    if value != 0.0 {
        let abs = value.abs();
        if !(1e-4..1e17).contains(&abs) {
            return php_float_debug_scientific_string(value);
        }
    }
    value.to_string()
}

/// Reimplements PHP's `php_gcvt` (a `%.*G`-style conversion) used by var_dump
/// and serialize when `serialize_precision` is a positive number of significant
/// digits: trailing zeros are stripped, and scientific notation is chosen when
/// the decimal point falls before -4 or after `ndigit` significant digits.
pub(in crate::builtins::modules) fn php_gcvt(value: f64, ndigit: usize) -> String {
    let ndigit = ndigit.max(1);
    if value == 0.0 {
        return "0".to_owned();
    }
    let negative = value < 0.0;
    let abs = value.abs();
    // Significant digits + exponent via scientific formatting.
    let scientific = format!("{:.*E}", ndigit - 1, abs);
    let exponent: i32 = scientific
        .split_once('E')
        .and_then(|(_, exp)| exp.parse().ok())
        .unwrap_or(0);
    let decimal_point = exponent + 1;
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if exponent < -4 || exponent >= ndigit as i32 {
        let (mantissa, _) = scientific
            .split_once('E')
            .unwrap_or((scientific.as_str(), ""));
        let mut mantissa = mantissa
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned();
        if !mantissa.contains('.') {
            mantissa.push_str(".0");
        }
        out.push_str(&mantissa);
        out.push('E');
        out.push(if exponent < 0 { '-' } else { '+' });
        out.push_str(&exponent.abs().to_string());
    } else {
        let decimals = (ndigit as i32 - decimal_point).max(0) as usize;
        let fixed = format!("{abs:.decimals$}");
        let fixed = if fixed.contains('.') {
            fixed.trim_end_matches('0').trim_end_matches('.')
        } else {
            fixed.as_str()
        };
        out.push_str(fixed);
    }
    out
}

pub(in crate::builtins::modules) fn php_float_debug_scientific_string(value: f64) -> String {
    // Rust's `{:E}` uses the shortest digit sequence that round-trips, matching
    // PHP var_dump under serialize_precision=-1; we only reshape the exponent to
    // PHP's `E+dd` form and ensure a `.0` mantissa fraction.
    let output = format!("{value:E}");
    let Some(exponent_index) = output.find('E') else {
        return output;
    };
    let mut mantissa = output[..exponent_index].to_owned();
    let exponent = &output[exponent_index + 1..];
    if !mantissa.contains('.') {
        mantissa.push_str(".0");
    }
    let sign = exponent
        .strip_prefix('+')
        .map(|digits| ("+", digits))
        .or_else(|| exponent.strip_prefix('-').map(|digits| ("-", digits)))
        .unwrap_or(("+", exponent));
    let digits = sign.1.trim_start_matches('0');
    format!(
        "{}E{}{}",
        mantissa,
        sign.0,
        if digits.is_empty() { "0" } else { digits }
    )
}

pub(in crate::builtins::modules) fn php_float_export_string(
    value: FloatValue,
    serialize_precision: i32,
) -> String {
    let value = value.to_f64();
    if value.is_nan() {
        return "NAN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-INF".to_owned()
        } else {
            "INF".to_owned()
        };
    }

    let mut formatted = if serialize_precision >= 1 {
        php_gcvt(value, serialize_precision as usize)
    } else if value != 0.0 && !(1e-4..1e17).contains(&value.abs()) {
        php_float_debug_scientific_string(value)
    } else {
        value.to_string()
    };
    if !formatted.contains(['.', 'E', 'e']) {
        formatted.push_str(".0");
    }
    formatted
}

pub(in crate::builtins::modules) fn write_indent(output: &mut OutputBuffer, spaces: usize) {
    output.write_bytes(vec![b' '; spaces]);
}
