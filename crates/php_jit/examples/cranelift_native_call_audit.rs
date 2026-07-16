use php_jit::{
    JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION, JitNativeCallArgument, JitNativeCallFrame,
    JitNativeCallKind, JitNativeCallTarget,
};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const IR_CALL_FORMS: &[&str] = &[
    "CallFunction",
    "CallMethod",
    "CallStaticMethod",
    "CallClosure",
    "CallCallable",
    "Pipe",
    "BindReferenceFromCall",
    "BindReferenceFromMethodCall",
    "NewObject",
    "DynamicNewObject",
];

const CALLBACKS: &[(&str, JitNativeCallKind)] = &[
    (
        "builtin_runtime_callback",
        JitNativeCallKind::BUILTIN_CALLBACK,
    ),
    ("magic_method", JitNativeCallKind::MAGIC_METHOD),
    ("property_hook", JitNativeCallKind::PROPERTY_HOOK),
    ("autoload_callback", JitNativeCallKind::AUTOLOAD_CALLBACK),
    ("error_handler", JitNativeCallKind::ERROR_HANDLER),
    ("shutdown_function", JitNativeCallKind::SHUTDOWN_FUNCTION),
    ("destructor", JitNativeCallKind::DESTRUCTOR),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = Path::new("target/cranelift-only");
    fs::create_dir_all(output)?;
    let mut json = format!(
        "{{\n  \"schema_version\": 1,\n  \"runtime_abi_version\": {JIT_RUNTIME_ABI_VERSION},\n  \"runtime_abi_hash\": \"{JIT_RUNTIME_ABI_HASH:016x}\",\n  \"frame_size\": {},\n  \"argument_size\": {},\n  \"target_size\": {},\n  \"ir_call_forms\": [",
        std::mem::size_of::<JitNativeCallFrame>(),
        std::mem::size_of::<JitNativeCallArgument>(),
        std::mem::size_of::<JitNativeCallTarget>(),
    );
    for (index, form) in IR_CALL_FORMS.iter().enumerate() {
        write!(json, "{}\"{form}\"", if index == 0 { "" } else { "," })?;
    }
    json.push_str("],\n  \"callback_kinds\": [\n");
    for (index, (name, kind)) in CALLBACKS.iter().enumerate() {
        writeln!(
            json,
            "    {{\"name\":\"{name}\",\"kind\":{}}}{}",
            kind.0,
            if index + 1 == CALLBACKS.len() {
                ""
            } else {
                ","
            },
        )?;
    }
    json.push_str("  ],\n  \"direct_path\": \"generation_bound_compiled_to_compiled\",\n  \"dynamic_path\": \"typed_native_dispatch_trampoline\",\n  \"interpreter_reentry\": false\n}\n");

    let mut markdown =
        String::from("# Native call-model audit\n\n| contract | value |\n|---|---|\n");
    writeln!(markdown, "| Runtime ABI | v{JIT_RUNTIME_ABI_VERSION} |")?;
    writeln!(
        markdown,
        "| Frame / argument / target bytes | {} / {} / {} |",
        std::mem::size_of::<JitNativeCallFrame>(),
        std::mem::size_of::<JitNativeCallArgument>(),
        std::mem::size_of::<JitNativeCallTarget>(),
    )?;
    writeln!(markdown, "| IR call forms | {} |", IR_CALL_FORMS.join(", "))?;
    writeln!(
        markdown,
        "| Callback kinds | {} |",
        CALLBACKS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    markdown.push_str("| Interpreter re-entry | forbidden |\n");
    fs::write(output.join("native-call-model.json"), json)?;
    fs::write(output.join("native-call-model.md"), markdown)?;
    println!(
        "wrote native call audit for {} IR forms and {} callback kinds",
        IR_CALL_FORMS.len(),
        CALLBACKS.len()
    );
    Ok(())
}
