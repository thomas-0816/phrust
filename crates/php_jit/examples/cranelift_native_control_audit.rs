use php_jit::{
    JIT_RUNTIME_ABI_HASH, JIT_RUNTIME_ABI_VERSION, JitCallStatus, JitNativeControlRecord,
    JitNativeDestructorPoint, JitNativeExceptionHandler, JitNativeFrameHeader, JitNativePcMetadata,
    JitNativeRootEntry,
};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const STATUSES: &[(&str, JitCallStatus)] = &[
    ("Continue", JitCallStatus::CONTINUE),
    ("Return", JitCallStatus::RETURN),
    ("ReturnReference", JitCallStatus::RETURN_REFERENCE),
    ("Throw", JitCallStatus::THROW),
    ("Exit", JitCallStatus::EXIT),
    ("SuspendGenerator", JitCallStatus::SUSPEND_GENERATOR),
    ("SuspendFiber", JitCallStatus::SUSPEND_FIBER),
    ("RuntimeError", JitCallStatus::RUNTIME_ERROR),
    ("CompileRequired", JitCallStatus::COMPILE_REQUIRED),
    ("RecompileRequested", JitCallStatus::RECOMPILE_REQUESTED),
];

const CONTROL_OPS: &[&str] = &[
    "EnterTry",
    "LeaveTry",
    "EndFinally",
    "Throw",
    "MakeException",
    "ReturnThroughFinally",
    "ThrowThroughFinally",
    "ExitThroughFinally",
];

const DESTRUCTOR_POINTS: &[(&str, JitNativeDestructorPoint)] = &[
    ("local_overwrite", JitNativeDestructorPoint::LOCAL_OVERWRITE),
    ("discard", JitNativeDestructorPoint::DISCARD),
    ("frame_return", JitNativeDestructorPoint::FRAME_RETURN),
    (
        "exception_unwind",
        JitNativeDestructorPoint::EXCEPTION_UNWIND,
    ),
    (
        "request_shutdown",
        JitNativeDestructorPoint::REQUEST_SHUTDOWN,
    ),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = Path::new("target/cranelift-only");
    fs::create_dir_all(output)?;
    let mut json = format!(
        "{{\n  \"schema_version\": 1,\n  \"runtime_abi_version\": {JIT_RUNTIME_ABI_VERSION},\n  \"runtime_abi_hash\": \"{JIT_RUNTIME_ABI_HASH:016x}\",\n  \"layouts\": {{\"frame\":{},\"control\":{},\"handler\":{},\"root\":{},\"pc\":{}}},\n  \"statuses\": [\n",
        std::mem::size_of::<JitNativeFrameHeader>(),
        std::mem::size_of::<JitNativeControlRecord>(),
        std::mem::size_of::<JitNativeExceptionHandler>(),
        std::mem::size_of::<JitNativeRootEntry>(),
        std::mem::size_of::<JitNativePcMetadata>(),
    );
    for (index, (name, status)) in STATUSES.iter().enumerate() {
        writeln!(
            json,
            "    {{\"name\":\"{name}\",\"tag\":{}}}{}",
            status.0,
            if index + 1 == STATUSES.len() { "" } else { "," }
        )?;
    }
    json.push_str("  ],\n  \"control_operations\": [");
    for (index, operation) in CONTROL_OPS.iter().enumerate() {
        write!(json, "{}\"{operation}\"", if index == 0 { "" } else { "," })?;
    }
    json.push_str("],\n  \"destructor_points\": [\n");
    for (index, (name, point)) in DESTRUCTOR_POINTS.iter().enumerate() {
        writeln!(
            json,
            "    {{\"name\":\"{name}\",\"tag\":{}}}{}",
            point.0,
            if index + 1 == DESTRUCTOR_POINTS.len() {
                ""
            } else {
                ","
            }
        )?;
    }
    json.push_str(
        "  ],\n  \"native_unwind\": true,\n  \"rust_unwind_across_generated\": false,\n  \"interpreter_exception_dispatch\": false,\n  \"baseline_roots\": \"published_frame_slots\",\n  \"optimized_roots\": \"stack_map_or_shadow_slot\",\n  \"native_pc_source_metadata\": true\n}\n",
    );

    let mut markdown =
        String::from("# Native control-flow audit\n\n| contract | value |\n|---|---|\n");
    writeln!(markdown, "| Runtime ABI | v{JIT_RUNTIME_ABI_VERSION} |")?;
    writeln!(
        markdown,
        "| Stable statuses | {} |",
        STATUSES
            .iter()
            .map(|(name, status)| format!("{name}={}", status.0))
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    writeln!(
        markdown,
        "| Native control operations | {} |",
        CONTROL_OPS.join(", ")
    )?;
    writeln!(
        markdown,
        "| Destructor release points | {} |",
        DESTRUCTOR_POINTS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    markdown.push_str("| Exception dispatch | explicit native unwind |\n");
    markdown.push_str("| GC roots | baseline slots; optimized stack maps/shadow slots |\n");
    markdown.push_str("| Backtraces | native PC to precise IR span |\n");
    fs::write(output.join("native-control-flow.json"), json)?;
    fs::write(output.join("native-control-flow.md"), markdown)?;
    println!(
        "wrote native control audit for {} statuses and {} control operations",
        STATUSES.len(),
        CONTROL_OPS.len()
    );
    Ok(())
}
