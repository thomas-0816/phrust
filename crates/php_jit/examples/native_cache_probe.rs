use php_ir::builder::IrBuilder;
use php_ir::{FunctionFlags, InstructionKind, IrConstant, IrSpan, Operand, UnitId};
use php_jit::{
    JIT_HELPER_REGISTRY_ABI_HASH, JIT_RUNTIME_ABI_HASH, JitCompileRequest, JitCompileStatus,
    JitEngine, NativeArtifactCache, NativeArtifactImage, NativeCacheConfig, NativeCacheIdentity,
    NativeCacheMode, NativeContinuationEntry, NativeFunctionAbi, NativeFunctionImage,
    cranelift_host_isa_identity,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: native_cache_probe CACHE_DIR")?;
    let identity = cache_identity()?;
    let cache = NativeArtifactCache::new(NativeCacheConfig {
        mode: NativeCacheMode::ReadWrite,
        directory,
        ..NativeCacheConfig::default()
    })?;
    let mut compiled = false;
    let (artifact, event) = cache.get_or_compile(
        &identity,
        |_| None,
        || -> Result<NativeArtifactImage, php_jit::NativeCacheError> {
            compiled = true;
            let unit = probe_unit();
            let mut engine = JitEngine::new();
            let records = engine
                .compile_unit(
                    &unit,
                    JitCompileRequest::new("native-cache-probe")
                        .with_ir_fingerprint("probe-ir-v1")
                        .with_dependency_identity("probe-dependencies-v1"),
                )
                .map_err(|error| php_jit::NativeCacheError::InvalidHeader(error.to_string()))?;
            let record = records.first().ok_or_else(|| {
                php_jit::NativeCacheError::InvalidHeader("no compile record".to_owned())
            })?;
            if !matches!(record.result.status, JitCompileStatus::Compiled) {
                return Err(php_jit::NativeCacheError::InvalidHeader(format!(
                    "probe compile rejected: {:?}",
                    record.result.diagnostics
                )));
            }
            let handle = record.result.handle.as_ref().ok_or_else(|| {
                php_jit::NativeCacheError::InvalidHeader("probe has no native handle".to_owned())
            })?;
            let code = handle.copy_relocation_free_machine_code().ok_or_else(|| {
                php_jit::NativeCacheError::InvalidRelocation(
                    "probe unexpectedly requires a relocation".to_owned(),
                )
            })?;
            let mut image = NativeArtifactImage::minimal(
                identity.clone(),
                code.clone(),
                NativeFunctionImage {
                    function_id: 0,
                    code_offset: 0,
                    code_len: code.len() as u64,
                    arity: 0,
                    abi: NativeFunctionAbi::PackedI64StatusOut,
                },
            );
            if let Some(metadata) = handle.region_state_metadata() {
                let mut seen = std::collections::BTreeSet::new();
                image.continuations = metadata
                    .native_pc_ranges
                    .iter()
                    .map(|range| NativeContinuationEntry {
                        function_id: range.function.raw(),
                        continuation_id: range.continuation_id,
                        code_offset: u64::from(range.start),
                    })
                    .filter(|entry| entry.code_offset < image.code.len() as u64)
                    .filter(|entry| seen.insert((entry.function_id, entry.continuation_id)))
                    .collect();
            }
            Ok(image)
        },
    )?;
    let value = artifact.invoke_i64_status_out(0)?;
    let stats = cache.stats();
    println!(
        "{{\"event\":\"{}\",\"compiled\":{},\"value\":{},\"hits\":{},\"writes\":{},\"rebuilds\":{},\"invalid_artifacts\":{}}}",
        match event {
            php_jit::NativeCacheEvent::Disabled => "disabled",
            php_jit::NativeCacheEvent::Hit => "hit",
            php_jit::NativeCacheEvent::Miss => "miss",
            php_jit::NativeCacheEvent::Written => "written",
            php_jit::NativeCacheEvent::Rebuilt => "rebuilt",
        },
        compiled,
        value,
        stats.hits,
        stats.writes,
        stats.rebuilds,
        stats.invalid_artifacts
    );
    Ok(())
}

fn probe_unit() -> php_ir::IrUnit {
    let mut builder = IrBuilder::new(UnitId::new(1200));
    let file = builder.add_file("native-cache-probe.php");
    let span = IrSpan::new(file, 0, 18);
    let constant = builder.intern_constant(IrConstant::Int(42));
    let function = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    let value = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: value,
            constant,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(value)), span);
    builder.set_entry(function);
    builder.finish()
}

fn cache_identity() -> Result<NativeCacheIdentity, Box<dyn std::error::Error>> {
    let isa = cranelift_host_isa_identity()?;
    Ok(NativeCacheIdentity {
        source_hash: "sha256:probe-source-v1".to_owned(),
        ir_hash: "sha256:probe-ir-v1".to_owned(),
        dependency_graph_hash: "sha256:probe-dependencies-v1".to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        cranelift_version: php_jit::CRANELIFT_VERSION.to_owned(),
        cranelift_settings_hash: isa.feature_fingerprint,
        region_ir_schema_version: php_jit::region_ir::REGION_IR_SCHEMA_VERSION,
        runtime_abi_hash: JIT_RUNTIME_ABI_HASH,
        helper_abi_hash: JIT_HELPER_REGISTRY_ABI_HASH,
        target_triple: isa.target_triple,
        pointer_width: usize::BITS as u8,
        cpu_feature_fingerprint: isa.feature_fingerprint,
        optimization_tier: "baseline".to_owned(),
        optimization_config_hash: 0,
        php_semantic_config_hash: 0,
    })
}
