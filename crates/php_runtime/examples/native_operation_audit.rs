use php_runtime::api::{
    NATIVE_OPERATION_ABI_HASH, NATIVE_OPERATION_ABI_VERSION, NATIVE_OPERATION_REGISTRY,
    NativeOperationDescriptor,
};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("static runtime-operation metadata is valid JSON")
}

fn json_strings(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| json_string(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn args(operation: &NativeOperationDescriptor) -> String {
    operation
        .args
        .iter()
        .map(|argument| json_string(&format!("{argument:?}")))
        .collect::<Vec<_>>()
        .join(",")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = Path::new("target/cranelift-only");
    fs::create_dir_all(output)?;
    let mut json = format!(
        "{{\n  \"schema_version\": 1,\n  \"abi_version\": {NATIVE_OPERATION_ABI_VERSION},\n  \"abi_hash\": \"{NATIVE_OPERATION_ABI_HASH:016x}\",\n  \"operations\": [\n"
    );
    let mut markdown = String::from(
        "# Typed runtime-operation audit\n\n| id | name | family | signature | result | ownership | implementation | native | direct callers | native callers | user code | allocate | throw | diagnose | suspend | safepoint |\n|---:|---|---|---|---|---|---|---:|---|---|---:|---:|---:|---:|---:|---:|\n",
    );

    for (index, operation) in NATIVE_OPERATION_REGISTRY.iter().enumerate() {
        writeln!(
            json,
            "    {{\"id\":{},\"name\":{},\"family\":{},\"signature_version\":{},\"args\":[{}],\"result\":{},\"ownership\":{},\"implementation\":{},\"native_callable\":{},\"direct_callers\":[{}],\"native_callers\":[{}],\"may_call_user_code\":{},\"may_allocate\":{},\"may_throw\":{},\"may_diagnose\":{},\"may_suspend\":{},\"gc_safepoint\":{}}}{}",
            operation.id.0,
            json_string(operation.name),
            json_string(&format!("{:?}", operation.family)),
            operation.signature_version,
            args(operation),
            json_string(&format!("{:?}", operation.result)),
            json_string(&format!("{:?}", operation.ownership)),
            json_string(operation.implementation),
            operation.native_callable,
            json_strings(operation.direct_callers),
            json_strings(operation.native_callers),
            operation.may_call_user_code,
            operation.may_allocate,
            operation.may_throw,
            operation.may_diagnose,
            operation.may_suspend,
            operation.gc_safepoint,
            if index + 1 == NATIVE_OPERATION_REGISTRY.len() {
                ""
            } else {
                ","
            },
        )?;
        writeln!(
            markdown,
            "| {} | {} | {:?} | v{}({}) | {:?} | {:?} | `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            operation.id.0,
            operation.name,
            operation.family,
            operation.signature_version,
            operation
                .args
                .iter()
                .map(|argument| format!("{argument:?}"))
                .collect::<Vec<_>>()
                .join(", "),
            operation.result,
            operation.ownership,
            operation.implementation,
            operation.native_callable,
            operation.direct_callers.join(", "),
            operation.native_callers.join(", "),
            operation.may_call_user_code,
            operation.may_allocate,
            operation.may_throw,
            operation.may_diagnose,
            operation.may_suspend,
            operation.gc_safepoint,
        )?;
    }
    json.push_str("  ]\n}\n");
    fs::write(output.join("runtime-helper-audit.json"), json)?;
    fs::write(output.join("runtime-helper-audit.md"), markdown)?;
    println!(
        "wrote {} typed runtime-operation audit rows",
        NATIVE_OPERATION_REGISTRY.len()
    );
    Ok(())
}
