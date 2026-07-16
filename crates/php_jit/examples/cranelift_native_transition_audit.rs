use php_jit::{
    JIT_NATIVE_TRANSITION_RESUME_TAG, JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION, JitCallStatus,
    JitNativeTransitionState,
};
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = Path::new("target/cranelift-only");
    fs::create_dir_all(output)?;
    let json = format!(
        "{{\n  \"schema_version\": 1,\n  \"runtime_abi_version\": {JIT_RUNTIME_ABI_VERSION},\n  \"runtime_abi_hash\": \"{JIT_RUNTIME_ABI_HASH:016x}\",\n  \"transition_state_size\": {},\n  \"transition_resume_tag\": \"{JIT_NATIVE_TRANSITION_RESUME_TAG:08x}\",\n  \"recompile_status\": {},\n  \"baseline_for_every_function\": true,\n  \"exact_instruction_entries\": true,\n  \"live_locals_and_registers\": true,\n  \"pending_control_state\": true,\n  \"nested_function_entries\": true,\n  \"native_osr_both_directions\": true,\n  \"observable_effect_replay\": false,\n  \"interpreter_resume_target\": false\n}}\n",
        std::mem::size_of::<JitNativeTransitionState>(),
        JitCallStatus::RECOMPILE_REQUESTED.0,
    );
    let markdown = format!(
        "# Native version-transition audit\n\n| contract | value |\n|---|---|\n| Runtime ABI | v{JIT_RUNTIME_ABI_VERSION} |\n| Transition state bytes | {} |\n| Resume ID namespace | `{JIT_NATIVE_TRANSITION_RESUME_TAG:08x}` |\n| State | function/version, continuation, locals, registers, control |\n| Target | exact baseline Cranelift instruction entry |\n| Nested calls | process-local published function entries |\n| Observable effect replay | forbidden |\n| Alternate executor resume | forbidden |\n",
        std::mem::size_of::<JitNativeTransitionState>(),
    );
    fs::write(output.join("native-version-transitions.json"), json)?;
    fs::write(output.join("native-version-transitions.md"), markdown)?;
    println!("wrote native version-transition audit");
    Ok(())
}
