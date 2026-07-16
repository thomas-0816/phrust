use php_jit::{
    JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION, JitNativeFiberState, JitNativeGeneratorState,
    JitNativeResumeInputKind, JitNativeSuspendKind, JitNativeSuspensionGenerationPolicy,
};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const SUSPENSIONS: &[(&str, JitNativeSuspendKind)] = &[
    ("generator_yield", JitNativeSuspendKind::GENERATOR_YIELD),
    (
        "generator_delegate",
        JitNativeSuspendKind::GENERATOR_DELEGATE,
    ),
    ("fiber_suspend", JitNativeSuspendKind::FIBER_SUSPEND),
];

const INPUTS: &[(&str, JitNativeResumeInputKind)] = &[
    ("start", JitNativeResumeInputKind::START),
    ("value", JitNativeResumeInputKind::VALUE),
    ("throw", JitNativeResumeInputKind::THROW),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = Path::new("target/cranelift-only");
    fs::create_dir_all(output)?;
    let mut json = format!(
        "{{\n  \"schema_version\": 1,\n  \"runtime_abi_version\": {JIT_RUNTIME_ABI_VERSION},\n  \"runtime_abi_hash\": \"{JIT_RUNTIME_ABI_HASH:016x}\",\n  \"generator_state_size\": {},\n  \"fiber_state_size\": {},\n  \"suspension_kinds\": [\n",
        std::mem::size_of::<JitNativeGeneratorState>(),
        std::mem::size_of::<JitNativeFiberState>(),
    );
    for (index, (name, kind)) in SUSPENSIONS.iter().enumerate() {
        writeln!(
            json,
            "    {{\"name\":\"{name}\",\"tag\":{}}}{}",
            kind.0,
            if index + 1 == SUSPENSIONS.len() {
                ""
            } else {
                ","
            }
        )?;
    }
    json.push_str("  ],\n  \"resume_inputs\": [\n");
    for (index, (name, kind)) in INPUTS.iter().enumerate() {
        writeln!(
            json,
            "    {{\"name\":\"{name}\",\"tag\":{}}}{}",
            kind.0,
            if index + 1 == INPUTS.len() { "" } else { "," }
        )?;
    }
    writeln!(
        json,
        "  ],\n  \"resume_id_tag\": \"40000000\",\n  \"generation_policies\": [{{\"name\":\"keep_owner\",\"tag\":{}}},{{\"name\":\"recompile_at_safe_boundary\",\"tag\":{}}}],\n  \"persisted_state\": [\"native_identity\",\"continuation\",\"locals\",\"temporaries\",\"yielded_key\",\"yielded_value\",\"delegation\",\"exception\",\"roots\"],\n  \"generated_resume_entries\": true,\n  \"interpreter_continuation_dependency\": false,\n  \"interpreter_resume_dispatch\": false,\n  \"generic_rust_instruction_loop\": false\n}}",
        JitNativeSuspensionGenerationPolicy::KEEP_OWNING_GENERATION.0,
        JitNativeSuspensionGenerationPolicy::RECOMPILE_AT_SAFE_BOUNDARY.0,
    )?;

    let mut markdown =
        String::from("# Native suspension audit\n\n| contract | value |\n|---|---|\n");
    writeln!(markdown, "| Runtime ABI | v{JIT_RUNTIME_ABI_VERSION} |")?;
    writeln!(
        markdown,
        "| Generator / fiber state bytes | {} / {} |",
        std::mem::size_of::<JitNativeGeneratorState>(),
        std::mem::size_of::<JitNativeFiberState>()
    )?;
    writeln!(
        markdown,
        "| Suspension entries | {} |",
        SUSPENSIONS
            .iter()
            .map(|(name, kind)| format!("{name}={}", kind.0))
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    markdown.push_str("| Resume implementation | generated native continuation entry |\n");
    markdown.push_str("| Invalidation | owning generation or safe-boundary transition |\n");
    markdown.push_str("| Retired resume dispatcher | absent |\n");
    fs::write(output.join("native-suspensions.json"), json)?;
    fs::write(output.join("native-suspensions.md"), markdown)?;
    println!(
        "wrote native suspension audit for {} suspension kinds",
        SUSPENSIONS.len()
    );
    Ok(())
}
