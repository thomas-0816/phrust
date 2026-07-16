fn main() -> Result<(), Box<dyn std::error::Error>> {
    let isa = php_jit::cranelift_host_isa_identity()?;
    println!("runtime_abi_version={}", php_jit::JIT_RUNTIME_ABI_VERSION);
    println!("runtime_abi_hash={:#018x}", php_jit::JIT_RUNTIME_ABI_HASH);
    println!(
        "helper_abi_hash={:#018x}",
        php_jit::JIT_HELPER_REGISTRY_ABI_HASH
    );
    println!(
        "region_ir_schema_version={}",
        php_jit::region_ir::REGION_IR_SCHEMA_VERSION
    );
    println!("isa_name={}", isa.isa_name);
    println!("target_triple={}", isa.target_triple);
    println!("cpu_feature_fingerprint={:#018x}", isa.feature_fingerprint);
    println!("cpu_identity={}", isa.display);
    Ok(())
}
