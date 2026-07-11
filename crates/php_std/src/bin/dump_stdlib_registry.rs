use php_extensions::BuiltinRegistry;
use php_std::{ClassKind, ExtensionRegistry, SymbolVisibility};

fn main() {
    let registry = ExtensionRegistry::standard_library();
    let builtins = BuiltinRegistry::new();

    println!("{{");
    println!("  \"extensions\": [");
    let extensions = registry.extensions().collect::<Vec<_>>();
    for (extension_index, extension) in extensions.iter().enumerate() {
        println!("    {{");
        println!("      \"name\": \"{}\",", json_escape(extension.name()));
        println!(
            "      \"enabled_by_default\": {},",
            extension.is_enabled_by_default()
        );

        println!("      \"functions\": [");
        let functions = extension
            .functions()
            .iter()
            .filter(|function| function.visibility() == SymbolVisibility::PhpVisible)
            .collect::<Vec<_>>();
        for (index, function) in functions.iter().enumerate() {
            let arginfo = function.arginfo();
            print!(
                "        {{\"name\": \"{}\", \"runtime_builtin\": {}, \"arginfo_source\": {}, \"required_parameters\": {}, \"total_parameters\": {}, \"variadic\": {}}}",
                json_escape(function.name()),
                builtins.get(function.name()).is_some() || is_vm_builtin(function.name()),
                arginfo
                    .map(|metadata| format!("\"{}\"", json_escape(metadata.source)))
                    .unwrap_or_else(|| "null".to_owned()),
                arginfo
                    .map(|metadata| metadata
                        .params
                        .iter()
                        .filter(|param| !param.optional)
                        .count())
                    .unwrap_or(0),
                arginfo.map_or(0, |metadata| metadata.params.len()),
                arginfo.is_some_and(|metadata| metadata.params.iter().any(|param| param.variadic))
            );
            println!("{}", comma(index, functions.len()));
        }
        println!("      ],");

        println!("      \"classes\": [");
        let classes = extension.classes();
        for (index, class) in classes.iter().enumerate() {
            print!(
                "        {{\"name\": \"{}\", \"kind\": \"{}\"}}",
                json_escape(class.name()),
                class_kind_name(class.kind())
            );
            println!("{}", comma(index, classes.len()));
        }
        println!("      ],");

        println!("      \"constants\": [");
        let constants = extension.constants();
        for (index, constant) in constants.iter().enumerate() {
            print!(
                "        {{\"name\": \"{}\", \"has_value\": {}}}",
                json_escape(constant.name()),
                constant.value().is_some()
            );
            println!("{}", comma(index, constants.len()));
        }
        println!("      ]");
        print!("    }}");
        println!("{}", comma(extension_index, extensions.len()));
    }
    println!("  ]");
    println!("}}");
}

const fn class_kind_name(kind: ClassKind) -> &'static str {
    match kind {
        ClassKind::Class => "class",
        ClassKind::Interface => "interface",
        ClassKind::Trait => "trait",
        ClassKind::Enum => "enum",
    }
}

fn comma(index: usize, len: usize) -> &'static str {
    if index + 1 == len { "" } else { "," }
}

fn is_vm_builtin(name: &str) -> bool {
    matches!(
        name,
        "define"
            | "defined"
            | "constant"
            | "extension_loaded"
            | "function_exists"
            | "clone"
            | "die"
            | "exit"
            | "compact"
            | "class_exists"
            | "class_alias"
            | "call_user_func"
            | "call_user_func_array"
            | "forward_static_call"
            | "debug_backtrace"
            | "debug_print_backtrace"
            | "func_get_arg"
            | "func_get_args"
            | "func_num_args"
            | "get_called_class"
            | "interface_exists"
            | "trait_exists"
            | "enum_exists"
            | "method_exists"
            | "property_exists"
            | "is_callable"
            | "is_a"
            | "is_subclass_of"
            | "get_class"
            | "get_class_methods"
            | "get_class_vars"
            | "get_parent_class"
            | "class_parents"
            | "class_implements"
            | "get_declared_classes"
            | "get_declared_interfaces"
            | "get_declared_traits"
            | "get_defined_functions"
            | "get_defined_constants"
            | "get_defined_vars"
            | "get_error_handler"
            | "get_exception_handler"
            | "get_extension_funcs"
            | "get_included_files"
            | "get_loaded_extensions"
            | "get_required_files"
            | "zend_version"
            | "phpversion"
            | "get_mangled_object_vars"
            | "get_object_vars"
    )
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                out.push_str(&format!("\\u{:04x}", u32::from(ch)));
            }
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::json_escape;

    #[test]
    fn registry_dump_json_escape_is_stable() {
        assert_eq!(json_escape("a\"b\\c\n"), "a\\\"b\\\\c\\n");
    }
}
