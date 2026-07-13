//! Fileinfo internal-class adapter.

use super::builtin_adapter::{builtin_source_span, execute_builtin_entry, unknown_builtin_result};
use super::prelude::*;

pub(super) struct FileinfoMethodCall<'a> {
    pub(super) vm: &'a Vm,
    pub(super) compiled: &'a CompiledUnit,
    pub(super) object: ObjectRef,
    pub(super) method: &'a str,
    pub(super) call_span: Option<IrSpan>,
    pub(super) output: &'a mut OutputBuffer,
    pub(super) stack: &'a mut CallStack,
    pub(super) state: &'a mut ExecutionState,
}

impl FileinfoMethodCall<'_> {
    pub(super) fn execute(self, args: Vec<CallArgument>) -> VmResult {
        let values = match call_args_to_positional(&format!("finfo::{}", self.method), args) {
            Ok(values) => values,
            Err(message) => {
                return self
                    .vm
                    .runtime_error(self.output, self.compiled, self.stack, message);
            }
        };
        self.execute_values(values)
    }

    fn execute_values(self, mut values: Vec<Value>) -> VmResult {
        if normalize_method_name(self.method) == "__construct" {
            if values.len() > 2 {
                return self.vm.runtime_error(
                    self.output,
                    self.compiled,
                    self.stack,
                    format!(
                        "E_PHP_VM_FILEINFO_ARG_COUNT: finfo::__construct expects 0 to 2 argument(s), {} given",
                        values.len()
                    ),
                );
            }
            let flags = match values.first().map(to_int).transpose() {
                Ok(flags) => flags.unwrap_or(0),
                Err(message) => {
                    return self
                        .vm
                        .runtime_error(self.output, self.compiled, self.stack, message);
                }
            };
            let magic_file = match values.get(1) {
                Some(Value::Null | Value::Uninitialized) | None => None,
                Some(value) => match to_string(value) {
                    Ok(path) => Some(path.to_string_lossy()),
                    Err(message) => {
                        return self.vm.runtime_error(
                            self.output,
                            self.compiled,
                            self.stack,
                            message,
                        );
                    }
                },
            };
            if let Err(message) =
                php_runtime::api::validate_fileinfo_options(flags, magic_file.as_deref())
            {
                return self.vm.runtime_error(
                    self.output,
                    self.compiled,
                    self.stack,
                    format!("E_PHP_VM_FILEINFO_MAGIC: {message}"),
                );
            }
            self.object
                .set_property("__fileinfo_flags", Value::Int(flags));
            self.object.set_property(
                "__fileinfo_magic_file",
                magic_file.map(Value::string).unwrap_or(Value::Null),
            );
            return VmResult::success_no_output(Some(Value::Null));
        }
        let Some(function) = fileinfo_method_builtin_name(self.method) else {
            return self.vm.runtime_error(
                self.output,
                self.compiled,
                self.stack,
                format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {}::{} is not defined",
                    self.object.class_name(),
                    self.method
                ),
            );
        };
        values.insert(0, Value::Object(self.object));
        let Some(entry) = BuiltinRegistry::new().get(function) else {
            return unknown_builtin_result(function, self.output);
        };
        execute_builtin_entry(
            entry,
            values,
            self.output,
            &self.vm.options.runtime_context,
            self.state,
            builtin_source_span(self.compiled, self.call_span),
        )
    }
}

fn fileinfo_method_builtin_name(method: &str) -> Option<&'static str> {
    match normalize_method_name(method).as_str() {
        "buffer" => Some("finfo_buffer"),
        "file" => Some("finfo_file"),
        "set_flags" => Some("finfo_set_flags"),
        _ => None,
    }
}
