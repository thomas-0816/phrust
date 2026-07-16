use php_jit::region_ir::{
    BASELINE_INSTRUCTION_MANIFEST, BASELINE_TERMINATOR_MANIFEST, BaselineLoweringClass,
    BaselineLoweringManifestEntry,
};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn class(entry: &BaselineLoweringManifestEntry) -> String {
    match entry.class {
        BaselineLoweringClass::DirectClif => "direct_clif".to_owned(),
        BaselineLoweringClass::TypedRuntimeHelper(helper) => {
            format!("typed_runtime_helper:{}", helper.0)
        }
        BaselineLoweringClass::NativeControlFlow => "native_control_flow".to_owned(),
        BaselineLoweringClass::NativeStateMachine => "native_state_machine".to_owned(),
        BaselineLoweringClass::CompileTimeFatal => "compile_time_fatal".to_owned(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = Path::new("target/cranelift-only");
    fs::create_dir_all(output)?;
    let entries = BASELINE_INSTRUCTION_MANIFEST
        .iter()
        .map(|entry| ("instruction", entry))
        .chain(
            BASELINE_TERMINATOR_MANIFEST
                .iter()
                .map(|entry| ("terminator", entry)),
        )
        .collect::<Vec<_>>();

    let mut json = String::from("{\n  \"schema_version\": 1,\n  \"entries\": [\n");
    let mut markdown = String::from(
        "# Cranelift baseline instruction coverage\n\n| kind | variant | class | helper | effects | throw | diagnose | user code | suspend | safepoint |\n|---|---|---|---|---:|---:|---:|---:|---:|---:|\n",
    );
    for (index, (kind, entry)) in entries.iter().enumerate() {
        let class = class(entry);
        let helper = match entry.class {
            BaselineLoweringClass::TypedRuntimeHelper(id) => id.0.to_string(),
            _ => String::new(),
        };
        writeln!(
            json,
            "    {{\"kind\":\"{kind}\",\"variant\":\"{}\",\"class\":\"{class}\",\"helper_id\":{},\"effect_flags\":{},\"may_throw\":{},\"may_diagnose\":{},\"may_call_user_code\":{},\"may_suspend\":{},\"requires_safepoint\":{}}}{}",
            entry.variant,
            if helper.is_empty() { "null" } else { &helper },
            entry.effects.bits(),
            entry.may_throw,
            entry.may_diagnose,
            entry.may_call_user_code,
            entry.may_suspend,
            entry.requires_safepoint,
            if index + 1 == entries.len() { "" } else { "," },
        )?;
        writeln!(
            markdown,
            "| {kind} | {} | {class} | {} | {} | {} | {} | {} | {} | {} |",
            entry.variant,
            if helper.is_empty() { "-" } else { &helper },
            entry.effects.bits(),
            entry.may_throw,
            entry.may_diagnose,
            entry.may_call_user_code,
            entry.may_suspend,
            entry.requires_safepoint,
        )?;
    }
    json.push_str("  ]\n}\n");
    fs::write(output.join("instruction-coverage.json"), json)?;
    fs::write(output.join("instruction-coverage.md"), markdown)?;
    println!("wrote {} lowering entries", entries.len());
    Ok(())
}
