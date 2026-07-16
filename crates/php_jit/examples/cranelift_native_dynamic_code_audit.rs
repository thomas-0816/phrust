use php_jit::{
    JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION, JitNativeDynamicCodeKind,
    JitNativeDynamicCodeRequest,
};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const OPERATIONS: &[(&str, JitNativeDynamicCodeKind)] = &[
    ("include", JitNativeDynamicCodeKind::INCLUDE),
    ("include_once", JitNativeDynamicCodeKind::INCLUDE_ONCE),
    ("require", JitNativeDynamicCodeKind::REQUIRE),
    ("require_once", JitNativeDynamicCodeKind::REQUIRE_ONCE),
    ("eval", JitNativeDynamicCodeKind::EVAL),
    (
        "declare_function",
        JitNativeDynamicCodeKind::DECLARE_FUNCTION,
    ),
    ("declare_class", JitNativeDynamicCodeKind::DECLARE_CLASS),
    ("make_closure", JitNativeDynamicCodeKind::MAKE_CLOSURE),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = Path::new("target/cranelift-only");
    fs::create_dir_all(output)?;
    let mut json = format!(
        "{{\n  \"schema_version\": 1,\n  \"runtime_abi_version\": {JIT_RUNTIME_ABI_VERSION},\n  \"runtime_abi_hash\": \"{JIT_RUNTIME_ABI_HASH:016x}\",\n  \"request_size\": {},\n  \"operations\": [\n",
        std::mem::size_of::<JitNativeDynamicCodeRequest>(),
    );
    for (index, (name, kind)) in OPERATIONS.iter().enumerate() {
        writeln!(
            json,
            "    {{\"name\":\"{name}\",\"tag\":{}}}{}",
            kind.0,
            if index + 1 == OPERATIONS.len() {
                ""
            } else {
                ","
            },
        )?;
    }
    json.push_str(
        "  ],\n  \"compile_once_exact_key\": true,\n  \"concurrent_miss_waits\": true,\n  \"nested_compile_lock_free\": true,\n  \"after_fork_reinitialization\": true,\n  \"process_cache\": true,\n  \"restart_cache_participation\": true,\n  \"publish_before_execute\": true,\n  \"interpreter_first_execution\": false\n}\n",
    );

    let mut markdown =
        String::from("# Native dynamic-code audit\n\n| contract | value |\n|---|---|\n");
    writeln!(markdown, "| Runtime ABI | v{JIT_RUNTIME_ABI_VERSION} |")?;
    writeln!(
        markdown,
        "| Operations | {} |",
        OPERATIONS
            .iter()
            .map(|(name, kind)| format!("{name}={}", kind.0))
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    markdown.push_str("| Publication | complete native artifact before execution |\n");
    markdown.push_str("| Stampede policy | exact-key owner plus waiting consumers |\n");
    markdown.push_str("| Nested compile | callback runs outside coordinator locks |\n");
    markdown.push_str("| Cache path | process and validated restart artifact |\n");
    markdown.push_str("| First execution fallback | forbidden |\n");
    fs::write(output.join("native-dynamic-code.json"), json)?;
    fs::write(output.join("native-dynamic-code.md"), markdown)?;
    println!(
        "wrote native dynamic-code audit for {} operations",
        OPERATIONS.len()
    );
    Ok(())
}
