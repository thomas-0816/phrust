use super::*;
use php_ir::builder::IrBuilder;
use php_ir::{
    BinaryOp, ClassEntry, ClassFlags, ClassId, ClassMethodEntry, ClassMethodFlags, FunctionFlags,
    InstructionKind, IrConstant, IrParam, IrReturnType, IrSpan, Operand, UnitId,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn production_and_diagnostic_helpers_use_distinct_tables() {
    let production = runtime_helper_addresses(false);
    let diagnostic = runtime_helper_addresses(true);

    assert_ne!(
        production.native_execution_poll,
        diagnostic.native_execution_poll
    );
    let execution_poll = php_jit::lookup_helper_by_name("phrust_native_execution_poll")
        .expect("execution-poll helper is registered")
        .id
        .0;
    assert_eq!(
        resolve_native_cache_helper(execution_poll, false),
        Some(production.native_execution_poll)
    );
    assert_eq!(
        resolve_native_cache_helper(execution_poll, true),
        Some(diagnostic.native_execution_poll)
    );
}

fn returning_unit(value: i64) -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(991));
    let file = builder.add_file("native-cache-vm.php");
    let span = IrSpan::new(file, 0, 20);
    let constant = builder.intern_constant(IrConstant::Int(value));
    let function = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let block = builder.append_block(function);
    let register = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::LoadConst {
            dst: register,
            constant,
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(register)), span);
    builder.set_entry(function);
    CompiledUnit::new(builder.finish())
}

fn background_tiering_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    lock_unpoisoned(LOCK.get_or_init(|| Mutex::new(())))
}

#[test]
fn optimizing_scheduler_retains_more_than_one_pending_batch() {
    let scheduler = NativeOptimizationScheduler::start("phrust-optimize-test-batch");
    let (completed, receiver) = std::sync::mpsc::channel();
    let jobs = NATIVE_OPTIMIZATION_BATCH_CAPACITY + 17;
    for function in 0..jobs {
        let key = native_compile_cache::NativeCompileCacheKey::new(
            0x51a7,
            php_ir::FunctionId::new(u32::try_from(function).expect("test function id")),
            NativeOptimizationPolicy::Optimizing.opt_level(),
            0,
        );
        let completed = completed.clone();
        scheduler.submit(
            key,
            u64::try_from(jobs - function).expect("test heat"),
            Box::new(move || completed.send(function).expect("test completion receiver")),
        );
    }
    drop(completed);
    let mut observed = HashSet::new();
    for _ in 0..jobs {
        observed.insert(
            receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("every queued optimizer candidate completes"),
        );
    }
    assert_eq!(observed.len(), jobs);
}

#[test]
fn generic_native_key_ignores_current_external_signature_state() {
    let unit = external_call_unit();
    let function = unit.unit().entry;
    let link_index = unit.prepared_external_function_calls(function)[0].link_index;
    let unpublished = php_jit::JitExternalFunctionSignature {
        name: "external_helper".to_owned(),
        link_index,
        published: false,
        params: Vec::new(),
        native_params: Vec::new(),
        native_default_constant_indices: Vec::new(),
        native_arity: 0,
        requires_non_reference_trampoline: false,
        returns_by_reference: false,
        return_type: None,
        exception_routes: None,
    };
    let mut published = unpublished.clone();
    published.published = true;
    published.native_arity = 2;

    let generic_unpublished = native_compile_cache_key(
        &unit,
        function,
        NativeOptimizationPolicy::Generic.opt_level(),
        std::slice::from_ref(&unpublished),
    );
    let generic_published = native_compile_cache_key(
        &unit,
        function,
        NativeOptimizationPolicy::Generic.opt_level(),
        std::slice::from_ref(&published),
    );
    assert_eq!(generic_unpublished, generic_published);

    let optimizing_unpublished = native_compile_cache_key(
        &unit,
        function,
        NativeOptimizationPolicy::Optimizing.opt_level(),
        &[unpublished],
    );
    let optimizing_published = native_compile_cache_key(
        &unit,
        function,
        NativeOptimizationPolicy::Optimizing.opt_level(),
        &[published],
    );
    assert_ne!(optimizing_unpublished, optimizing_published);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn native_compile_descriptor_attributes_identity_trigger_and_publication() {
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Generic,
        native_cache: php_jit::NativeCacheMode::Off,
        collect_counters: true,
        ..VmOptions::default()
    };
    let result = Vm::with_options(options).execute(returning_unit(7_302));
    assert_eq!(result.return_value, Some(Value::Int(7_302)), "{result:#?}");
    let counters = result.counters.expect("native counters");
    let descriptor = counters
        .native_compile_descriptors
        .first()
        .expect("bounded compile-cause descriptor");
    assert_eq!(descriptor.tier, "generic");
    assert_eq!(descriptor.trigger, "foreground-or-precompile");
    assert_eq!(descriptor.cache_disposition, "miss");
    assert_eq!(descriptor.publication_result, "published-generic");
    assert_eq!(descriptor.replan_index, 0);
    assert!(descriptor.generic_key.ends_with(":generic"));
    assert_eq!(descriptor.external_signatures_hash, 0);
    assert_eq!(descriptor.receiver_layout_hash, 0);
    assert!(descriptor.code_bytes > 0);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn unsuccessful_request_still_schedules_observed_hot_entry() {
    let _guard = background_tiering_test_guard();
    let tiering = crate::tiering::TieringOptions {
        collect_stats: true,
        function_entry_threshold: 1,
        native_max_functions: 1,
        ..crate::tiering::TieringOptions::default()
    };
    let worker = VmWorkerState::new_with_background_tiering(tiering.clone());
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        tiering,
        ..VmOptions::default()
    };
    let mut builder = IrBuilder::new(UnitId::new(992));
    let file = builder.add_file("unsuccessful-hot-entry.php");
    let span = IrSpan::new(file, 0, 1);
    let function = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    builder.emit(
        function,
        block,
        InstructionKind::RuntimeError {
            diagnostic_id: "E_HOT_FAILURE".to_owned(),
            message: "observed failure".to_owned(),
        },
        span,
    );
    builder.terminate_return(function, block, None, span);
    builder.set_entry(function);
    let unit = CompiledUnit::new(builder.finish());

    let result = Vm::with_options_and_worker_state(options, worker.clone()).execute(unit);
    assert!(!result.status.is_success(), "{result:#?}");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !lock_unpoisoned(&worker.tiering_state).scheduled.is_empty() && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(worker.tiering_stats().optimized_candidates, 1);
    let state = lock_unpoisoned(&worker.tiering_state);
    assert!(state.scheduled.is_empty());
    assert_eq!(
        state.stats.native_compiled_functions
            + u64::try_from(state.failed.len()).unwrap_or(u64::MAX),
        1,
        "the unsuccessful request's observed entry must be processed"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn server_worker_publishes_optimized_entry_after_hot_baseline_threshold() {
    let _guard = background_tiering_test_guard();
    let tiering = crate::tiering::TieringOptions {
        collect_stats: true,
        function_entry_threshold: 2,
        native_max_functions: 1,
        ..crate::tiering::TieringOptions::default()
    };
    let worker = VmWorkerState::new_with_background_tiering(tiering.clone());
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        tiering,
        collect_counters: true,
        ..VmOptions::default()
    };
    let unit = returning_unit(7_301);

    let first =
        Vm::with_options_and_worker_state(options.clone(), worker.clone()).execute(unit.clone());
    assert_eq!(first.return_value, Some(Value::Int(7_301)), "{first:#?}");
    let function = unit.unit().entry;
    let metadata = &unit.unit().functions[function.index()];
    let function_key = php_jit::native_function_key(
        unit.prepared_ir_fingerprint().to_owned(),
        function.raw(),
        metadata.params.len(),
        metadata.local_count,
        true,
        0,
    );
    let (baseline_cell, _) = php_jit::global_code_manager()
        .unwrap()
        .published_function(&function_key)
        .unwrap_or_else(|| {
            panic!("tiered baseline publication missing for {function_key:?}: {first:#?}")
        });
    let baseline_address = baseline_cell
        .resolve(function_key.signature_hash, 0)
        .expect("tiered baseline address");

    let second =
        Vm::with_options_and_worker_state(options.clone(), worker.clone()).execute(unit.clone());
    assert_eq!(second.return_value, Some(Value::Int(7_301)), "{second:#?}");

    let deadline = Instant::now() + Duration::from_secs(10);
    while worker.tiering_stats().native_compiled_functions == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let published = worker.tiering_stats();
    assert_eq!(published.baseline_entries, 2);
    assert_eq!(published.optimized_candidates, 1);
    assert_eq!(published.native_compiled_functions, 1);
    let (optimized_cell, _) = php_jit::global_code_manager()
        .unwrap()
        .published_function(&function_key)
        .expect("optimized publication");
    assert!(Arc::ptr_eq(&baseline_cell, &optimized_cell));
    let optimized_address = optimized_cell
        .resolve(function_key.signature_hash, 0)
        .expect("optimized address");
    assert_ne!(
        Some(optimized_address),
        Some(baseline_address),
        "optimized code must atomically replace the less-specialized target"
    );
    assert_eq!(
        unit.prepared_deployment_image().generic_function_entries[function.index()]
            .load(std::sync::atomic::Ordering::Acquire),
        baseline_address,
        "nested compiled calls must retain a side-exit-free baseline target"
    );
    assert_eq!(
        unit.prepared_deployment_image().preferred_function_entries[function.index()]
            .load(std::sync::atomic::Ordering::Acquire),
        optimized_address,
        "optimizing callers must observe the independently published optimizing target"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn background_cross_unit_optimizer_waits_for_foreground_publication_boundary() {
    let _guard = background_tiering_test_guard();
    let tiering = crate::tiering::TieringOptions {
        collect_stats: true,
        function_entry_threshold: 1,
        native_max_functions: 1,
        ..crate::tiering::TieringOptions::default()
    };
    let worker = VmWorkerState::new_with_background_tiering(tiering.clone());
    let unit = external_call_unit();
    let function = unit.unit().entry;
    let link_index = unit.prepared_external_function_calls(function)[0].link_index;
    let external_signatures = vec![php_jit::JitExternalFunctionSignature {
        name: "external_helper".to_owned(),
        link_index,
        published: true,
        params: Vec::new(),
        native_params: Vec::new(),
        native_default_constant_indices: Vec::new(),
        native_arity: 0,
        requires_non_reference_trampoline: false,
        returns_by_reference: false,
        return_type: None,
        exception_routes: None,
    }];
    let optimizing = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        tiering,
        ..VmOptions::default()
    };
    let mut baseline = optimizing.clone();
    baseline.native_optimization = NativeOptimizationPolicy::Generic;
    baseline.tiering.enabled = false;
    let baseline_address = worker
        .prepare_native_generic_entry(&unit, function, &baseline, &external_signatures)
        .expect("cross-unit baseline publication");
    let decision = worker
        .background_tiering_decision(&unit, function, &optimizing, &external_signatures)
        .expect("hot cross-unit optimizer decision");
    worker.schedule_background_optimization(
        decision,
        unit.clone(),
        function,
        &optimizing,
        external_signatures.clone(),
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    while worker.tiering_stats().native_compiled_functions == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(worker.tiering_stats().native_compiled_functions, 1);
    assert_eq!(
        unit.prepared_deployment_image().preferred_function_entries[function.index()]
            .load(std::sync::atomic::Ordering::Acquire),
        baseline_address,
        "background code must not become visible against an in-flight request's link graph"
    );
    assert!(
        worker
            .resolved_native_function(&unit, function, &optimizing, &external_signatures,)
            .is_none(),
        "dynamic dispatch must not adopt the unpublished background product"
    );
    assert!(worker.has_compiled_optimizing_function(&unit, function, &external_signatures,));

    let adopted = worker
        .resolve_native_function(&unit, function, &optimizing, &external_signatures)
        .expect("foreground publication boundary must adopt the compiled product")
        .native_entry_address()
        .expect("adopted optimizer address");
    assert_ne!(adopted, baseline_address);
    assert_eq!(
        unit.prepared_deployment_image().preferred_function_entries[function.index()]
            .load(std::sync::atomic::Ordering::Acquire),
        adopted
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn baseline_publication_reuses_code_across_external_signature_changes() {
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let unit = external_call_unit();
    let function = unit.unit().entry;
    let call = &unit.prepared_external_function_calls(function)[0];
    let unpublished = php_jit::JitExternalFunctionSignature {
        name: call.source_name.to_string(),
        link_index: call.link_index,
        published: false,
        params: Vec::new(),
        native_params: Vec::new(),
        native_default_constant_indices: Vec::new(),
        native_arity: 0,
        requires_non_reference_trampoline: false,
        returns_by_reference: false,
        return_type: None,
        exception_routes: None,
    };
    let published = php_jit::JitExternalFunctionSignature {
        published: true,
        ..unpublished.clone()
    };
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Generic,
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    };

    let first = worker
        .prepare_native_generic_entry(
            &unit,
            function,
            &options,
            std::slice::from_ref(&unpublished),
        )
        .expect("pre-declaration baseline publication");
    unit.prepared_deployment_image().preferred_function_entries[function.index()]
        .store(usize::MAX, std::sync::atomic::Ordering::Release);
    let second = worker
        .prepare_native_generic_entry(&unit, function, &options, std::slice::from_ref(&published))
        .expect("post-declaration baseline publication");

    assert_ne!(first, 0);
    assert_eq!(second, first);
    assert!(
        worker
            .resolved_native_function(&unit, function, &options, std::slice::from_ref(&published),)
            .is_some(),
        "the published address must retain its exact generation-owning handle"
    );
    let deployment = unit.prepared_deployment_image();
    assert_eq!(
        deployment.generic_function_entries[function.index()]
            .load(std::sync::atomic::Ordering::Acquire),
        first
    );
    assert_eq!(
        deployment.preferred_function_entries[function.index()]
            .load(std::sync::atomic::Ordering::Acquire),
        usize::MAX,
        "mutable link state must not invalidate a preferred specialization"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn worker_uses_one_generic_body_before_and_after_declaration() {
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let unit = external_call_unit();
    let function = unit.unit().entry;
    let call = &unit.prepared_external_function_calls(function)[0];
    let unpublished = php_jit::JitExternalFunctionSignature {
        name: call.source_name.to_string(),
        link_index: call.link_index,
        published: false,
        params: Vec::new(),
        native_params: Vec::new(),
        native_default_constant_indices: Vec::new(),
        native_arity: 0,
        requires_non_reference_trampoline: false,
        returns_by_reference: false,
        return_type: None,
        exception_routes: None,
    };
    let published = php_jit::JitExternalFunctionSignature {
        published: true,
        ..unpublished.clone()
    };
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Generic,
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    };

    worker
        .resolve_native_function(
            &unit,
            function,
            &options,
            std::slice::from_ref(&unpublished),
        )
        .expect("pre-declaration native ABI");
    worker
        .resolve_native_function(&unit, function, &options, &[published])
        .expect("post-declaration native ABI");
    let after_both = worker.native_compile_cache_stats();
    assert_eq!(after_both.entries, 1);
    assert_eq!(after_both.misses, 1);

    let resolved_hits = worker.resolved_native_entry_hits();
    worker
        .resolve_native_function(&unit, function, &options, &[unpublished])
        .expect("warm pre-declaration native ABI");
    assert_eq!(worker.native_compile_cache_stats(), after_both);
    assert_eq!(worker.resolved_native_entry_hits(), resolved_hits + 1);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn prewarm_resolves_and_publishes_the_optimizing_entry_without_execution() {
    let unit = returning_unit(7_302);
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    };
    let vm = Vm::with_options_and_worker_state(
        options,
        VmWorkerState::new(crate::tiering::TieringOptions::default()),
    );

    assert_eq!(vm.prewarm_cranelift(&unit), 1);
    let deployment = unit.prepared_deployment_image();
    let baseline = deployment.generic_function_entries[unit.unit().entry.index()]
        .load(std::sync::atomic::Ordering::Acquire);
    let preferred = deployment.preferred_function_entries[unit.unit().entry.index()]
        .load(std::sync::atomic::Ordering::Acquire);
    assert_ne!(baseline, 0, "optimizing prewarm must publish its baseline");
    assert_ne!(
        preferred, 0,
        "optimizing prewarm must publish its preferred entry"
    );
    assert_ne!(
        preferred, baseline,
        "prewarm must leave the optimizing entry preferred"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn prewarm_publishes_the_complete_function_working_set() {
    let unit = direct_call_unit();
    let function_count = u64::try_from(unit.unit().functions.len()).expect("function count");
    assert!(function_count > 1);
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    };
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let vm = Vm::with_options_and_worker_state(options, worker.clone());

    assert_eq!(vm.prewarm_cranelift(&unit), function_count);
    let deployment = unit.prepared_deployment_image();
    assert!(
        deployment
            .generic_function_entries
            .iter()
            .all(|entry| entry.load(std::sync::atomic::Ordering::Acquire) != 0)
    );
    assert!(
        deployment
            .preferred_function_entries
            .iter()
            .all(|entry| entry.load(std::sync::atomic::Ordering::Acquire) != 0)
    );
    let before = worker.native_compile_cache_stats();
    assert_eq!(vm.prewarm_cranelift(&unit), function_count);
    assert_eq!(worker.native_compile_cache_stats().misses, before.misses);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn cold_prewarm_persists_exactly_the_requested_baseline_and_optimizing_tiers() {
    let directory = std::env::temp_dir().join(format!(
        "phrust-vm-cold-prewarm-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let worker = VmWorkerState::isolated_for_restart_test();
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::ReadWrite,
        native_cache_dir: directory.clone(),
        ..VmOptions::default()
    };
    let unit = returning_unit(7_304);
    let vm = Vm::with_options_and_worker_state(options.clone(), worker.clone());

    assert_eq!(vm.prewarm_cranelift(&unit), 1);
    let stats = worker.native_compile_cache_stats();
    assert_eq!(
        stats.misses, 2,
        "cold optimizing prewarm must compile one baseline and one optimizer"
    );
    let mut baseline_options = options.clone();
    baseline_options.native_optimization = NativeOptimizationPolicy::Generic;
    baseline_options.tiering.enabled = false;
    for identity in [
        native_cache_identity(&unit, unit.unit().entry, &baseline_options, &[]).unwrap(),
        native_cache_identity(&unit, unit.unit().entry, &options, &[]).unwrap(),
    ] {
        assert!(
            directory
                .join(format!("{}.pna", identity.cache_key()))
                .is_file()
        );
    }
    assert_eq!(
        std::fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|value| value == "pna"))
            .count(),
        2,
        "cold prewarm must not compile an unused third baseline policy"
    );
    let before = worker.native_compile_cache_stats();
    assert_eq!(vm.prewarm_cranelift(&unit), 1);
    let after = worker.native_compile_cache_stats();
    assert_eq!(after.misses, before.misses);
    assert_eq!(after.compile_time_nanos, before.compile_time_nanos);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(target_arch = "x86_64")]
fn background_optimization_persists_and_prewarm_reloads_both_tiers() {
    let _guard = background_tiering_test_guard();
    let directory = std::env::temp_dir().join(format!(
        "phrust-vm-background-prewarm-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let tiering = crate::tiering::TieringOptions {
        function_entry_threshold: 2,
        native_max_functions: 1,
        ..crate::tiering::TieringOptions::default()
    };
    let worker = VmWorkerState::new_with_background_tiering(tiering.clone());
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::ReadWrite,
        native_cache_dir: directory.clone(),
        tiering,
        ..VmOptions::default()
    };
    let unit = returning_unit(7_303);

    for _ in 0..2 {
        let result = Vm::with_options_and_worker_state(options.clone(), worker.clone())
            .execute(unit.clone());
        assert_eq!(result.return_value, Some(Value::Int(7_303)), "{result:#?}");
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while worker.tiering_stats().native_compiled_functions == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(worker.tiering_stats().native_compiled_functions, 1);
    let mut baseline_options = options.clone();
    baseline_options.native_optimization = NativeOptimizationPolicy::Generic;
    baseline_options.tiering.enabled = false;
    let baseline_identity =
        native_cache_identity(&unit, unit.unit().entry, &baseline_options, &[]).unwrap();
    let optimizing_identity =
        native_cache_identity(&unit, unit.unit().entry, &options, &[]).unwrap();
    assert!(
        directory
            .join(format!("{}.pna", baseline_identity.cache_key()))
            .is_file(),
        "background tiering must persist the baseline artifact"
    );
    assert!(
        directory
            .join(format!("{}.pna", optimizing_identity.cache_key()))
            .is_file(),
        "background tiering must persist the optimizing artifact"
    );
    let artifacts = std::fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "pna")
        })
        .count();
    assert_eq!(
        artifacts, 2,
        "background publication must persist baseline and optimizing artifacts"
    );

    let restarted = CompiledUnit::from(unit.unit().clone());
    let restart_worker = VmWorkerState::isolated_for_restart_test();
    let restart_options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Read,
        native_cache_dir: directory.clone(),
        ..VmOptions::default()
    };
    let vm = Vm::with_options_and_worker_state(restart_options, restart_worker.clone());
    assert_eq!(vm.prewarm_cranelift(&restarted), 1);
    assert_eq!(restart_worker.native_compile_cache_stats().misses, 0);
    let deployment = restarted.prepared_deployment_image();
    let baseline = deployment.generic_function_entries[restarted.unit().entry.index()]
        .load(std::sync::atomic::Ordering::Acquire);
    let preferred = deployment.preferred_function_entries[restarted.unit().entry.index()]
        .load(std::sync::atomic::Ordering::Acquire);
    assert_ne!(baseline, 0);
    assert_ne!(preferred, 0);
    assert_ne!(preferred, baseline);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(target_arch = "x86_64")]
fn server_worker_uses_direct_entry_heat_for_on_demand_callee() {
    let _guard = background_tiering_test_guard();
    let tiering = crate::tiering::TieringOptions {
        collect_stats: true,
        function_entry_threshold: 2,
        native_max_functions: 2,
        ..crate::tiering::TieringOptions::default()
    };
    let worker = VmWorkerState::new_with_background_tiering(tiering.clone());
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        tiering,
        ..VmOptions::default()
    };
    let unit = direct_call_unit_with_identity(9_951, "native-hot-callee.php");
    let callee = unit
        .unit()
        .function_table
        .iter()
        .find_map(|entry| (entry.name == "callee").then_some(entry.function))
        .expect("callee function id");

    for _ in 0..2 {
        let result = Vm::with_options_and_worker_state(options.clone(), worker.clone())
            .execute(unit.clone());
        assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");
    }
    assert!(
        unit.prepared_deployment_image()
            .generic_function_entry_counts[callee.index()]
        .load(std::sync::atomic::Ordering::Relaxed)
            >= 2
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let deployment = unit.prepared_deployment_image();
        let baseline = deployment.generic_function_entries[callee.index()]
            .load(std::sync::atomic::Ordering::Acquire);
        let preferred = deployment.preferred_function_entries[callee.index()]
            .load(std::sync::atomic::Ordering::Acquire);
        if baseline != 0 && preferred != baseline {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "hot on-demand callee did not publish an optimizing preferred entry"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let baseline_entries_before = unit
        .prepared_deployment_image()
        .generic_function_entry_counts[callee.index()]
    .load(std::sync::atomic::Ordering::Acquire);
    let optimized = Vm::with_options_and_worker_state(options, worker).execute(unit.clone());
    assert_eq!(
        optimized.return_value,
        Some(Value::Int(42)),
        "{optimized:#?}"
    );
    assert_eq!(
        unit.prepared_deployment_image()
            .generic_function_entry_counts[callee.index()]
        .load(std::sync::atomic::Ordering::Acquire),
        baseline_entries_before,
        "the baseline caller must consume the published optimizing entry"
    );
}

fn declaration_heavy_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(9_901));
    let file = builder.add_file("function-on-demand-breadth.php");
    let span = IrSpan::new(file, 0, 32);
    let constant = builder.intern_constant(IrConstant::Int(17));
    for index in 0..121 {
        let function = builder.start_function(
            format!("breadth_function_{index}"),
            FunctionFlags::default(),
            span,
        );
        builder.set_return_type(function, Some(IrReturnType::Int));
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
        if index == 0 {
            builder.set_entry(function);
        }
    }
    CompiledUnit::new(builder.finish())
}

fn looping_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(992));
    let file = builder.add_file("native-deadline-vm.php");
    let span = IrSpan::new(file, 0, 24);
    let function = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(function);
    builder.terminate_jump(function, block, block, span);
    builder.set_entry(function);
    CompiledUnit::new(builder.finish())
}

fn direct_call_unit() -> CompiledUnit {
    direct_call_unit_with_identity(993, "native-direct-counter.php")
}

fn external_call_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(9_954));
    let file = builder.add_file("native-background-external-publication.php");
    let span = IrSpan::new(file, 0, 32);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "external_helper".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(entry, block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    CompiledUnit::new(builder.finish())
}

fn direct_call_unit_with_identity(unit_id: u32, source: &str) -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(unit_id));
    let file = builder.add_file(source);
    let span = IrSpan::new(file, 0, 24);
    let constant = builder.intern_constant(IrConstant::Int(42));
    let callee = builder.start_function("callee", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let callee_block = builder.append_block(callee);
    let value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadConst {
            dst: value,
            constant,
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
    builder.register_function_name("callee", callee);

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(entry, Some(IrReturnType::Int));
    let entry_block = builder.append_block(entry);
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "callee".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    CompiledUnit::new(builder.finish())
}

fn request_local_on_demand_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(9_947));
    let file = builder.add_file("native-request-local-on-demand.php");
    let span = IrSpan::new(file, 0, 24);
    let callee = builder.start_function("read_server", FunctionFlags::default(), span);
    let server = builder.intern_local(callee, "_SERVER");
    let callee_block = builder.append_block(callee);
    let value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: value,
            local: server,
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
    builder.register_function_name("read_server", callee);

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let entry_block = builder.append_block(entry);
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "read_server".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    CompiledUnit::new(builder.finish())
}

fn optimizing_reference_call_to_baseline_unit() -> (CompiledUnit, php_ir::FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(9_937));
    let file = builder.add_file("native-reference-call-baseline.php");
    let span = IrSpan::new(file, 0, 32);
    let one = builder.intern_constant(IrConstant::Int(1));
    let four = builder.intern_constant(IrConstant::Int(4));

    let callee = builder.start_function("identity_ref", FunctionFlags::default(), span);
    builder.set_returns_by_ref(callee, true);
    let parameter = builder.intern_local(callee, "value");
    builder.push_param(
        callee,
        IrParam {
            name: "value".to_owned(),
            local: parameter,
            required: true,
            default: None,
            type_: None,
            by_ref: true,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let callee_block = builder.append_block(callee);
    builder.terminate_return_ref(callee, callee_block, parameter, span);
    builder.register_function_name("identity_ref", callee);

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(entry, Some(IrReturnType::Int));
    let source = builder.intern_local(entry, "source");
    let alias = builder.intern_local(entry, "alias");
    let entry_block = builder.append_block(entry);
    let initial = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::LoadConst {
            dst: initial,
            constant: one,
        },
        span,
    );
    builder.emit(
        entry,
        entry_block,
        InstructionKind::StoreLocal {
            local: source,
            src: Operand::Register(initial),
        },
        span,
    );
    let argument = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::LoadLocal {
            dst: argument,
            local: source,
        },
        span,
    );
    builder.emit(
        entry,
        entry_block,
        InstructionKind::BindReferenceFromCall {
            target: alias,
            name: "identity_ref".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Register(argument),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: Some(source),
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    let replacement = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::LoadConst {
            dst: replacement,
            constant: four,
        },
        span,
    );
    builder.emit(
        entry,
        entry_block,
        InstructionKind::StoreLocal {
            local: alias,
            src: Operand::Register(replacement),
        },
        span,
    );
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::LoadLocal {
            dst: result,
            local: source,
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    (CompiledUnit::new(builder.finish()), callee)
}

fn optimizing_array_to_baseline_mutation_unit() -> (CompiledUnit, php_ir::FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(9_934));
    let file = builder.add_file("native-direct-array-baseline-mutation.php");
    let span = IrSpan::new(file, 0, 32);
    let nine = builder.intern_constant(IrConstant::Int(9));

    let callee = builder.start_function("append_value", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let items = builder.intern_local(callee, "items");
    builder.push_required_param(callee, "items", items);
    let callee_block = builder.append_block(callee);
    let appended = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::AppendDim {
            dst: appended,
            local: items,
            dims: Vec::new(),
            value: Operand::Constant(nine),
        },
        span,
    );
    builder.terminate_return(
        callee,
        callee_block,
        Some(Operand::Register(appended)),
        span,
    );
    builder.register_function_name("append_value", callee);

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(entry, Some(IrReturnType::Int));
    let entry_block = builder.append_block(entry);
    let array = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "append_value".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Register(array),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    (CompiledUnit::new(builder.finish()), callee)
}

fn direct_reference_array_cow_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(9_942));
    let file = builder.add_file("native-direct-reference-array-cow.php");
    let span = IrSpan::new(file, 0, 32);
    let zero = builder.intern_constant(IrConstant::Int(0));
    let one = builder.intern_constant(IrConstant::Int(1));
    let nine = builder.intern_constant(IrConstant::Int(9));

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let source = builder.intern_local(entry, "source");
    let alias = builder.intern_local(entry, "alias");
    let snapshot = builder.intern_local(entry, "snapshot");
    let entry_block = builder.append_block(entry);
    let initial = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::NewArray { dst: initial },
        span,
    );
    builder.emit(
        entry,
        entry_block,
        InstructionKind::StoreLocal {
            local: source,
            src: Operand::Register(initial),
        },
        span,
    );
    let copied = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::LoadLocal {
            dst: copied,
            local: source,
        },
        span,
    );
    builder.emit(
        entry,
        entry_block,
        InstructionKind::StoreLocal {
            local: snapshot,
            src: Operand::Register(copied),
        },
        span,
    );
    builder.emit(
        entry,
        entry_block,
        InstructionKind::BindReference {
            target: alias,
            source,
        },
        span,
    );
    let appended = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::AppendDim {
            dst: appended,
            local: alias,
            dims: Vec::new(),
            value: Operand::Constant(nine),
        },
        span,
    );
    let changed = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::LoadLocal {
            dst: changed,
            local: source,
        },
        span,
    );
    let unchanged = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::LoadLocal {
            dst: unchanged,
            local: snapshot,
        },
        span,
    );
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::NewArray { dst: result },
        span,
    );
    builder.emit(
        entry,
        entry_block,
        InstructionKind::ArrayInsert {
            array: result,
            key: Some(Operand::Constant(zero)),
            value: Operand::Register(changed),
            by_ref_local: None,
        },
        span,
    );
    builder.emit(
        entry,
        entry_block,
        InstructionKind::ArrayInsert {
            array: result,
            key: Some(Operand::Constant(one)),
            value: Operand::Register(unchanged),
            by_ref_local: None,
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    CompiledUnit::new(builder.finish())
}

fn optimizing_nested_callee_transition_unit() -> (CompiledUnit, php_ir::FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(9_938));
    let file = builder.add_file("native-nested-optimizing-transition.php");
    let span = IrSpan::new(file, 0, 32);
    let negative_nine = builder.intern_constant(IrConstant::Int(-9));
    let one = builder.intern_constant(IrConstant::Int(1));

    let callee = builder.start_function("absolute_value", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let callee_block = builder.append_block(callee);
    let absolute = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::CallFunction {
            dst: absolute,
            name: "abs".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Constant(negative_nine),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(
        callee,
        callee_block,
        Some(Operand::Register(absolute)),
        span,
    );
    builder.register_function_name("absolute_value", callee);

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(entry, Some(IrReturnType::Int));
    let entry_block = builder.append_block(entry);
    let called = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::CallFunction {
            dst: called,
            name: "absolute_value".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Add,
            lhs: Operand::Register(called),
            rhs: Operand::Constant(one),
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    (CompiledUnit::new(builder.finish()), callee)
}

fn nested_builtin_type_error_catch_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(9_956));
    let file = builder.add_file("native-nested-builtin-catch.php");
    let span = IrSpan::new(file, 0, 48);
    let scalar = builder.intern_constant(IrConstant::Int(42));
    let caught = builder.intern_constant(IrConstant::Int(77));
    let missed = builder.intern_constant(IrConstant::Int(0));

    let callee = builder.start_function("scalar_sizeof", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let entry = builder.append_block(callee);
    let protected = builder.append_block(callee);
    let catch = builder.append_block(callee);
    let after = builder.append_block(callee);
    builder.emit(
        callee,
        entry,
        InstructionKind::EnterTry {
            catch: Some(catch),
            catch_types: vec!["typeerror".to_owned()],
            finally: None,
            after,
            exception_local: None,
        },
        span,
    );
    builder.terminate_jump(callee, entry, protected, span);
    let result = builder.alloc_register(callee);
    builder.emit(
        callee,
        protected,
        InstructionKind::CallFunction {
            dst: result,
            name: "sizeof".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Constant(scalar),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.emit(callee, protected, InstructionKind::LeaveTry, span);
    builder.terminate_jump(callee, protected, after, span);
    builder.terminate_return(callee, catch, Some(Operand::Constant(caught)), span);
    builder.terminate_return(callee, after, Some(Operand::Constant(missed)), span);
    builder.register_function_name("scalar_sizeof", callee);

    let main = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(main, Some(IrReturnType::Int));
    let main_block = builder.append_block(main);
    let value = builder.alloc_register(main);
    builder.emit(
        main,
        main_block,
        InstructionKind::CallFunction {
            dst: value,
            name: "scalar_sizeof".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(main, main_block, Some(Operand::Register(value)), span);
    builder.set_entry(main);
    CompiledUnit::new(builder.finish())
}

fn optimizing_nested_constant_key_array_transition_unit() -> (CompiledUnit, php_ir::FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(99_381));
    let file = builder.add_file("native-nested-constant-key-array-transition.php");
    let span = IrSpan::new(file, 0, 48);
    let first_key_constant = builder.intern_constant(IrConstant::String("path".to_owned()));
    let second_key_constant = builder.intern_constant(IrConstant::String("selector".to_owned()));
    let nested_value = builder.intern_constant(IrConstant::Int(41));
    let null = builder.intern_constant(IrConstant::Null);

    let callee = builder.start_function("build_array", FunctionFlags::default(), span);
    let callee_block = builder.append_block(callee);
    let array = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::NewArray { dst: array },
        span,
    );
    let first_key = builder.alloc_register(callee);
    builder.emit_load_const(callee, callee_block, first_key, first_key_constant, span);
    let nested = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::NewArray { dst: nested },
        span,
    );
    builder.emit(
        callee,
        callee_block,
        InstructionKind::ArrayInsert {
            array: nested,
            key: None,
            value: Operand::Constant(nested_value),
            by_ref_local: None,
        },
        span,
    );
    builder.emit(
        callee,
        callee_block,
        InstructionKind::ArrayInsert {
            array,
            key: Some(Operand::Register(first_key)),
            value: Operand::Register(nested),
            by_ref_local: None,
        },
        span,
    );
    builder.emit(
        callee,
        callee_block,
        InstructionKind::Discard {
            src: Operand::Register(first_key),
        },
        span,
    );
    builder.emit(
        callee,
        callee_block,
        InstructionKind::Discard {
            src: Operand::Register(nested),
        },
        span,
    );
    let second_key = builder.alloc_register(callee);
    builder.emit_load_const(callee, callee_block, second_key, second_key_constant, span);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::ArrayInsert {
            array,
            key: Some(Operand::Register(second_key)),
            value: Operand::Constant(null),
            by_ref_local: None,
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(array)), span);
    builder.register_function_name("build_array", callee);

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let entry_block = builder.append_block(entry);
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "build_array".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    (CompiledUnit::new(builder.finish()), callee)
}

fn optimizing_nested_builtin_constants_unit() -> (CompiledUnit, php_ir::FunctionId) {
    let mut builder = IrBuilder::new(UnitId::new(9_939));
    let file = builder.add_file("native-nested-builtin-constants.php");
    let span = IrSpan::new(file, 0, 64);
    let pattern = builder.intern_constant(IrConstant::String("/-[0-9]+$/".to_owned()));
    let replacement = builder.intern_constant(IrConstant::String(String::new()));
    let subject = builder.intern_constant(IrConstant::String("widget-12".to_owned()));
    let suffix = builder.intern_constant(IrConstant::String("!".to_owned()));

    let callee = builder.start_function("strip_widget_id", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::String));
    let id = builder.intern_local(callee, "id");
    builder.push_required_param(callee, "id", id);
    let callee_block = builder.append_block(callee);
    let pattern_value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadConst {
            dst: pattern_value,
            constant: pattern,
        },
        span,
    );
    let replacement_value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadConst {
            dst: replacement_value,
            constant: replacement,
        },
        span,
    );
    let subject_value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: subject_value,
            local: id,
        },
        span,
    );
    let value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::CallFunction {
            dst: value,
            name: "preg_replace".to_owned(),
            args: [
                Operand::Register(pattern_value),
                Operand::Register(replacement_value),
                Operand::Register(subject_value),
            ]
            .into_iter()
            .map(|value| php_ir::instruction::IrCallArg {
                name: None,
                value,
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            })
            .collect(),
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
    builder.register_function_name("strip_widget_id", callee);

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(entry, Some(IrReturnType::String));
    let entry_block = builder.append_block(entry);
    let called = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::CallFunction {
            dst: called,
            name: "strip_widget_id".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Constant(subject),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::Binary {
            dst: result,
            op: BinaryOp::Concat,
            lhs: Operand::Register(called),
            rhs: Operand::Constant(suffix),
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    (CompiledUnit::new(builder.finish()), callee)
}

fn direct_method_on_demand_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(9_931));
    let file = builder.add_file("native-direct-method.php");
    let span = IrSpan::new(file, 0, 32);
    let constant = builder.intern_constant(IrConstant::Int(42));
    let method = builder.start_function(
        "Widget::value",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.intern_local(method, "this");
    builder.set_return_type(method, Some(IrReturnType::Int));
    let method_block = builder.append_block(method);
    let value = builder.alloc_register(method);
    builder.emit(
        method,
        method_block,
        InstructionKind::LoadConst {
            dst: value,
            constant,
        },
        span,
    );
    builder.terminate_return(method, method_block, Some(Operand::Register(value)), span);
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "widget".to_owned(),
        display_name: "Widget".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "value".to_owned(),
            origin_class: "widget".to_owned(),
            function: method,
            flags: ClassMethodFlags {
                has_body: true,
                ..ClassMethodFlags::default()
            },
            attributes: Vec::new(),
        }],
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor: None,
        flags: ClassFlags::default(),
        span,
    });

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(entry, Some(IrReturnType::Int));
    let entry_block = builder.append_block(entry);
    let object = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::NewObject {
            dst: object,
            display_class_name: "Widget".to_owned(),
            class_name: "widget".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::CallMethod {
            dst: result,
            object: Operand::Register(object),
            method: "value".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    CompiledUnit::new(builder.finish())
}

fn typed_direct_call_unit(strict_types: bool) -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(998));
    let file = builder.add_file("native-typed-direct.php");
    builder.set_file_strict_types(file, strict_types);
    builder.set_strict_types(strict_types);
    let span = IrSpan::new(file, 0, 32);
    let callee = builder.start_function("typed_callee", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::Int));
    let parameter = builder.intern_local(callee, "value");
    builder.push_param(
        callee,
        IrParam {
            name: "value".to_owned(),
            local: parameter,
            required: true,
            default: None,
            type_: Some(IrReturnType::Int),
            by_ref: false,
            variadic: false,
            attributes: Vec::new(),
        },
    );
    let callee_block = builder.append_block(callee);
    let value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadLocal {
            dst: value,
            local: parameter,
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
    builder.register_function_name("typed_callee", callee);

    let argument = builder.intern_constant(IrConstant::String("42".to_owned()));
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(entry, Some(IrReturnType::Int));
    let entry_block = builder.append_block(entry);
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "typed_callee".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Constant(argument),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    CompiledUnit::new(builder.finish())
}

fn invalid_return_type_on_demand_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(9_999));
    let file = builder.add_file("native-return-type-on-demand.php");
    let span = IrSpan::new(file, 0, 48);
    let invalid = builder.intern_constant(IrConstant::Array(Vec::new()));
    let callee = builder.start_function("invalid_return", FunctionFlags::default(), span);
    builder.set_return_type(callee, Some(IrReturnType::String));
    let callee_block = builder.append_block(callee);
    let value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadConst {
            dst: value,
            constant: invalid,
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
    builder.register_function_name("invalid_return", callee);

    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let entry_block = builder.append_block(entry);
    let result = builder.alloc_register(entry);
    builder.emit(
        entry,
        entry_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "invalid_return".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(entry, entry_block, Some(Operand::Register(result)), span);
    builder.set_entry(entry);
    CompiledUnit::new(builder.finish())
}

fn direct_builtin_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(994));
    let file = builder.add_file("native-direct-builtin.php");
    let span = IrSpan::new(file, 0, 32);
    let value = builder.intern_constant(IrConstant::Int(-6));
    let function = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(function, Some(IrReturnType::Int));
    let block = builder.append_block(function);
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "abs".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Constant(value),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    builder.set_entry(function);
    CompiledUnit::new(builder.finish())
}

fn direct_type_predicate_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(993));
    let file = builder.add_file("native-direct-type-predicate.php");
    let span = IrSpan::new(file, 0, 32);
    let string = builder.intern_constant(IrConstant::String("phrust".to_owned()));
    let function = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(function, Some(IrReturnType::Bool));
    let block = builder.append_block(function);
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "is_string".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Constant(string),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    builder.set_entry(function);
    CompiledUnit::new(builder.finish())
}

fn bounded_inline_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(997));
    let file = builder.add_file("native-inline-constant.php");
    let span = IrSpan::new(file, 0, 32);
    let constant = builder.intern_constant(IrConstant::Int(19));
    let callee = builder.start_function("constant_wrapper", FunctionFlags::default(), span);
    let callee_block = builder.append_block(callee);
    let value = builder.alloc_register(callee);
    builder.emit(
        callee,
        callee_block,
        InstructionKind::LoadConst {
            dst: value,
            constant,
        },
        span,
    );
    builder.terminate_return(callee, callee_block, Some(Operand::Register(value)), span);
    builder.register_function_name("constant_wrapper", callee);

    let main = builder.start_function("main", FunctionFlags::default(), span);
    let main_block = builder.append_block(main);
    let result = builder.alloc_register(main);
    builder.emit(
        main,
        main_block,
        InstructionKind::CallFunction {
            dst: result,
            name: "constant_wrapper".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(main, main_block, Some(Operand::Register(result)), span);
    builder.set_entry(main);
    CompiledUnit::new(builder.finish())
}

fn unbounded_recursive_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(995));
    let file = builder.add_file("native-frame-depth.php");
    let span = IrSpan::new(file, 0, 32);
    let function = builder.start_function("recurse", FunctionFlags::default(), span);
    builder.register_function_name("recurse", function);
    let block = builder.append_block(function);
    let result = builder.alloc_register(function);
    builder.emit(
        function,
        block,
        InstructionKind::CallFunction {
            dst: result,
            name: "recurse".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(function, block, Some(Operand::Register(result)), span);
    builder.set_entry(function);
    CompiledUnit::new(builder.finish())
}

fn polymorphic_method_pic_unit() -> CompiledUnit {
    let mut builder = IrBuilder::new(UnitId::new(996));
    let file = builder.add_file("native-method-pic.php");
    let span = IrSpan::new(file, 0, 64);
    let seven = builder.intern_constant(IrConstant::Int(7));

    let method = builder.start_function(
        "Widget::value",
        FunctionFlags {
            is_method: true,
            ..FunctionFlags::default()
        },
        span,
    );
    builder.intern_local(method, "this");
    builder.set_return_type(method, Some(IrReturnType::Int));
    let method_block = builder.append_block(method);
    let method_value = builder.alloc_register(method);
    builder.emit(
        method,
        method_block,
        InstructionKind::LoadConst {
            dst: method_value,
            constant: seven,
        },
        span,
    );
    builder.terminate_return(
        method,
        method_block,
        Some(Operand::Register(method_value)),
        span,
    );

    let factory = builder.start_function("make_widget", FunctionFlags::default(), span);
    let factory_block = builder.append_block(factory);
    let object = builder.alloc_register(factory);
    builder.emit(
        factory,
        factory_block,
        InstructionKind::NewObject {
            dst: object,
            display_class_name: "Widget".to_owned(),
            class_name: "widget".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(
        factory,
        factory_block,
        Some(Operand::Register(object)),
        span,
    );
    builder.register_function_name("make_widget", factory);

    let call_value = builder.start_function("call_value", FunctionFlags::default(), span);
    builder.set_return_type(call_value, Some(IrReturnType::Int));
    let receiver_local = builder.intern_local(call_value, "receiver");
    builder.push_required_param(call_value, "receiver", receiver_local);
    let call_value_block = builder.append_block(call_value);
    let receiver_value = builder.alloc_register(call_value);
    builder.emit(
        call_value,
        call_value_block,
        InstructionKind::LoadLocal {
            dst: receiver_value,
            local: receiver_local,
        },
        span,
    );
    let call_value_result = builder.alloc_register(call_value);
    builder.emit(
        call_value,
        call_value_block,
        InstructionKind::CallMethod {
            dst: call_value_result,
            object: Operand::Register(receiver_value),
            method: "value".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    builder.terminate_return(
        call_value,
        call_value_block,
        Some(Operand::Register(call_value_result)),
        span,
    );
    builder.register_function_name("call_value", call_value);

    let main = builder.start_function("main", FunctionFlags::default(), span);
    builder.set_return_type(main, Some(IrReturnType::Int));
    let main_block = builder.append_block(main);
    let receiver = builder.alloc_register(main);
    builder.emit(
        main,
        main_block,
        InstructionKind::CallFunction {
            dst: receiver,
            name: "make_widget".to_owned(),
            args: Vec::new(),
        },
        span,
    );
    let first = builder.alloc_register(main);
    builder.emit(
        main,
        main_block,
        InstructionKind::CallFunction {
            dst: first,
            name: "call_value".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Register(receiver),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    let second = builder.alloc_register(main);
    builder.emit(
        main,
        main_block,
        InstructionKind::CallFunction {
            dst: second,
            name: "call_value".to_owned(),
            args: vec![php_ir::instruction::IrCallArg {
                name: None,
                value: Operand::Register(receiver),
                unpack: false,
                value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                by_ref_local: None,
                by_ref_dim: None,
                by_ref_property: None,
                by_ref_property_dim: None,
            }],
        },
        span,
    );
    builder.terminate_return(main, main_block, Some(Operand::Register(second)), span);
    builder.push_class(ClassEntry {
        id: ClassId::new(0),
        name: "widget".to_owned(),
        display_name: "Widget".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "value".to_owned(),
            origin_class: "widget".to_owned(),
            function: method,
            flags: ClassMethodFlags {
                has_body: true,
                ..ClassMethodFlags::default()
            },
            attributes: Vec::new(),
        }],
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor: None,
        flags: ClassFlags::default(),
        span,
    });
    builder.set_entry(main);
    CompiledUnit::new(builder.finish())
}

#[test]
#[cfg(target_arch = "x86_64")]
fn same_unit_call_resolves_on_demand_then_calls_native() {
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let unit = direct_call_unit();
    let result = Vm::with_options_and_worker_state(
        VmOptions {
            collect_counters: true,
            ..VmOptions::default()
        },
        worker.clone(),
    )
    .execute(unit.clone());

    assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");
    let counters = result.counters.expect("diagnostic counters");
    assert_eq!(counters.native_call_direct, 1);
    assert_eq!(counters.native_same_unit_direct_executed, 1);
    assert_eq!(counters.native_call_dynamic, 0);
    assert_eq!(counters.native_transition_count, 0);
    assert_eq!(counters.native_tail_calls, 0);
    assert!(counters.native_frame_arena_high_water_bytes > 0);
    let compile_stats = worker.native_compile_cache_stats();
    assert_eq!(compile_stats.entries, 2);
    assert_eq!(compile_stats.misses, 2);
    assert_eq!(compile_stats.insertions, 2);
    assert_eq!(
        unit.prepared_deployment_image()
            .generic_function_entry_counts
            .iter()
            .map(|entries| entries.load(std::sync::atomic::Ordering::Relaxed))
            .sum::<u64>(),
        2,
        "root and on-demand callee must each update their direct baseline-entry counter"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn pooled_request_republishes_metadata_for_process_published_callee() {
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let unit = request_local_on_demand_unit();
    let options = VmOptions {
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    };

    let first =
        Vm::with_options_and_worker_state(options.clone(), worker.clone()).execute(unit.clone());
    assert!(first.status.is_success(), "{first:#?}");
    let compile_stats = worker.native_compile_cache_stats();
    assert_eq!(compile_stats.entries, 2);

    let second = Vm::with_options_and_worker_state(options, worker.clone()).execute(unit);
    assert!(second.status.is_success(), "{second:#?}");
    assert_eq!(second.return_value, first.return_value);
    assert_eq!(
        worker.native_compile_cache_stats().misses,
        compile_stats.misses
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn tiered_baseline_call_miss_cannot_publish_an_optimizing_callee() {
    let unit = direct_call_unit_with_identity(9_935, "native-tiered-baseline-firewall.php");
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let result = Vm::with_options_and_worker_state(
        VmOptions {
            native_optimization: NativeOptimizationPolicy::Generic,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            ..VmOptions::default()
        },
        worker,
    )
    .execute(unit.clone());

    assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");
    let callee = unit
        .unit()
        .function_table
        .iter()
        .find_map(|entry| (entry.name == "callee").then_some(entry.function))
        .expect("callee function id");
    let metadata = &unit.unit().functions[callee.index()];
    let key = |optimizing| {
        php_jit::native_function_key(
            unit.prepared_ir_fingerprint().to_owned(),
            callee.raw(),
            metadata.params.len(),
            metadata.local_count,
            optimizing,
            0,
        )
    };
    let manager = php_jit::global_code_manager().expect("global code manager");
    assert!(
        manager.published_function_exact(&key(false)).is_some(),
        "baseline resolver must publish the baseline callee"
    );
    assert!(
        manager.published_function_exact(&key(true)).is_none(),
        "baseline resolver must never compile or publish an optimizing callee"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn optimizing_direct_call_keeps_baseline_continuation_and_upgrades_preferred_entry() {
    let unit = direct_call_unit_with_identity(9_936, "native-on-demand-optimizer-cell.php");
    let tiering = crate::tiering::TieringOptions {
        collect_stats: true,
        ..crate::tiering::TieringOptions::default()
    };
    let worker = VmWorkerState::new(tiering.clone());
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        collect_counters: true,
        tiering,
        ..VmOptions::default()
    };
    let result =
        Vm::with_options_and_worker_state(options.clone(), worker.clone()).execute(unit.clone());
    assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");

    let callee = unit
        .unit()
        .function_table
        .iter()
        .find_map(|entry| (entry.name == "callee").then_some(entry.function))
        .expect("callee function id");
    let metadata = &unit.unit().functions[callee.index()];
    let optimizing_key = php_jit::native_function_key(
        unit.prepared_ir_fingerprint().to_owned(),
        callee.raw(),
        metadata.params.len(),
        metadata.local_count,
        true,
        0,
    );
    let manager = php_jit::global_code_manager().expect("global code manager");
    let baseline_key = php_jit::native_function_key(
        unit.prepared_ir_fingerprint().to_owned(),
        callee.raw(),
        metadata.params.len(),
        metadata.local_count,
        false,
        0,
    );
    let baseline_address = manager
        .published_function_exact(&baseline_key)
        .and_then(|(_, handle)| handle.native_entry_address())
        .expect("publication-time baseline callee");
    let nested_address = unit.prepared_deployment_image().generic_function_entries[callee.index()]
        .load(std::sync::atomic::Ordering::Acquire);
    assert_eq!(nested_address, baseline_address);
    assert_eq!(
        unit.prepared_deployment_image().preferred_function_entries[callee.index()]
            .load(std::sync::atomic::Ordering::Acquire),
        baseline_address,
        "the direct callee baseline must be callable before the root optimizer is published"
    );
    assert_eq!(
        result
            .counters
            .as_ref()
            .expect("first diagnostic counters")
            .native_transition_count,
        0,
        "the first direct call must not use an operation-local publication transition"
    );
    let optimizing_handle = worker
        .resolve_native_function(&unit, callee, &options, &[])
        .expect("explicit callee optimizer publication");
    let optimizing_address = optimizing_handle
        .native_entry_address()
        .expect("callee optimizer address");
    assert_ne!(baseline_address, optimizing_address);
    assert_eq!(
        manager
            .published_function_exact(&optimizing_key)
            .and_then(|(cell, _)| cell.resolve(optimizing_key.signature_hash, 0)),
        Some(optimizing_address)
    );
    assert_eq!(
        unit.prepared_deployment_image().preferred_function_entries[callee.index()]
            .load(std::sync::atomic::Ordering::Acquire),
        optimizing_address
    );
    assert_eq!(
        worker.tiering_stats().optimized_candidates,
        0,
        "a foreground worker must not enqueue speculative optimizer work"
    );

    let warm = Vm::with_options_and_worker_state(options, worker.clone()).execute(unit.clone());
    assert_eq!(warm.return_value, Some(Value::Int(42)), "{warm:#?}");
    assert_eq!(
        warm.counters
            .as_ref()
            .expect("warm diagnostic counters")
            .native_transition_count,
        0,
        "the published optimizing callee must keep the warm call in optimizing code"
    );
    assert_eq!(
        worker.native_compile_cache_stats().entries,
        4,
        "only the root and actually reached callee need baseline and optimizing products"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn optimizing_direct_array_can_cross_into_baseline_array_mutation() {
    let (unit, callee) = optimizing_array_to_baseline_mutation_unit();
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let baseline = VmOptions {
        native_optimization: NativeOptimizationPolicy::Generic,
        native_cache: php_jit::NativeCacheMode::Off,
        collect_counters: true,
        ..VmOptions::default()
    };
    worker
        .resolve_native_function(&unit, callee, &baseline, &[])
        .expect("baseline mutation callee must be published before optimizer execution");

    let result = Vm::with_options_and_worker_state(
        VmOptions {
            native_optimization: NativeOptimizationPolicy::Optimizing,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            ..VmOptions::default()
        },
        worker,
    )
    .execute(unit);

    assert_eq!(result.return_value, Some(Value::Int(9)), "{result:#?}");
    let counters = result.counters.expect("diagnostic counters");
    assert_eq!(counters.native_call_dynamic, 0);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn direct_reference_array_insert_preserves_cow_and_alias() {
    let result = Vm::with_options(VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    })
    .execute(direct_reference_array_cow_unit());

    let Some(Value::Array(result_array)) = result.return_value else {
        panic!("direct reference array mutation did not return evidence: {result:#?}");
    };
    let Some(Value::Array(changed)) = result_array.get(&php_runtime::api::ArrayKey::Int(0)) else {
        panic!("aliased array result is missing: {result_array:?}");
    };
    assert!(
        matches!(
            changed.get(&php_runtime::api::ArrayKey::Int(0)),
            Some(Value::Int(9))
        ),
        "mutation through the reference must update the referenced array"
    );
    let Some(Value::Array(snapshot)) = result_array.get(&php_runtime::api::ArrayKey::Int(1)) else {
        panic!("array COW snapshot is missing: {result_array:?}");
    };
    assert!(
        snapshot.get(&php_runtime::api::ArrayKey::Int(0)).is_none(),
        "mutation through the reference must not modify a prior by-value copy"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn optimizing_reference_call_preserves_alias_through_generic_entry() {
    let (unit, callee) = optimizing_reference_call_to_baseline_unit();
    let tiering = crate::tiering::TieringOptions {
        enabled: false,
        ..crate::tiering::TieringOptions::default()
    };
    let worker = VmWorkerState::new(tiering.clone());
    let baseline = VmOptions {
        native_optimization: NativeOptimizationPolicy::Generic,
        native_cache: php_jit::NativeCacheMode::Off,
        collect_counters: true,
        tiering: tiering.clone(),
        ..VmOptions::default()
    };
    let baseline_address = worker
        .resolve_native_function(&unit, callee, &baseline, &[])
        .expect("reference callee baseline")
        .native_entry_address()
        .expect("reference callee baseline address");

    let result = Vm::with_options_and_worker_state(
        VmOptions {
            native_optimization: NativeOptimizationPolicy::Optimizing,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            tiering,
            ..VmOptions::default()
        },
        worker,
    )
    .execute(unit.clone());

    assert_eq!(result.return_value, Some(Value::Int(4)), "{result:#?}");
    let counters = result.counters.expect("diagnostic counters");
    assert_eq!(counters.native_call_dynamic, 0);
    assert_eq!(
        unit.prepared_deployment_image().preferred_function_entries[callee.index()]
            .load(std::sync::atomic::Ordering::Acquire),
        baseline_address,
        "the optimizing caller must preserve the reference ABI without a callee optimizer"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn compiled_caller_resumes_rejected_optimizing_callee_and_continues() {
    let (unit, callee) = optimizing_nested_callee_transition_unit();
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let baseline = VmOptions {
        native_optimization: NativeOptimizationPolicy::Generic,
        native_cache: php_jit::NativeCacheMode::Off,
        collect_counters: true,
        ..VmOptions::default()
    };
    let baseline_handle = worker
        .resolve_native_function(&unit, callee, &baseline, &[])
        .expect("baseline callee");
    let optimizing = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        collect_counters: true,
        ..VmOptions::default()
    };
    let optimizing_handle = worker
        .resolve_native_function(&unit, callee, &optimizing, &[])
        .expect("optimizing callee");
    unit.prepared_deployment_image().generic_function_entries[callee.index()].store(
        baseline_handle
            .native_entry_address()
            .expect("baseline address"),
        std::sync::atomic::Ordering::Release,
    );
    unit.prepared_deployment_image().preferred_function_entries[callee.index()].store(
        optimizing_handle
            .native_entry_address()
            .expect("optimizing address"),
        std::sync::atomic::Ordering::Release,
    );

    let result = Vm::with_options_and_worker_state(baseline, worker).execute(unit);
    assert_eq!(result.return_value, Some(Value::Int(10)), "{result:#?}");
}

#[test]
#[cfg(target_arch = "x86_64")]
fn nested_native_builtin_type_error_resumes_compiled_catch() {
    let route_unit = nested_builtin_type_error_catch_unit();
    let callee = route_unit
        .lookup_function("scalar_sizeof")
        .expect("catching callee");
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let compile = |native_optimization| {
        worker
            .resolve_native_function(
                &route_unit,
                callee,
                &VmOptions {
                    native_optimization,
                    native_cache: php_jit::NativeCacheMode::Off,
                    ..VmOptions::default()
                },
                &[],
            )
            .expect("catching callee native entry")
    };
    let baseline = compile(NativeOptimizationPolicy::Generic);
    let optimizing = compile(NativeOptimizationPolicy::Optimizing);
    let route_identity = |handle: &php_jit::JitFunctionHandle| {
        let metadata = handle.region_state_metadata().expect("region metadata");
        (
            metadata.exception_handlers.clone(),
            metadata
                .continuations
                .iter()
                .filter(|continuation| continuation.function == callee)
                .cloned()
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        route_identity(&baseline),
        route_identity(&optimizing),
        "preferred-tier publication changed compiled handler identities"
    );
    let caller = route_unit.unit().entry;
    for native_optimization in [
        NativeOptimizationPolicy::Generic,
        NativeOptimizationPolicy::Optimizing,
    ] {
        let handle = worker
            .resolve_native_function(
                &route_unit,
                caller,
                &VmOptions {
                    native_optimization,
                    native_cache: php_jit::NativeCacheMode::Off,
                    ..VmOptions::default()
                },
                &[],
            )
            .expect("compiled caller");
        let metadata = handle.region_state_metadata().expect("caller metadata");
        if native_optimization == NativeOptimizationPolicy::Optimizing {
            assert!(
                metadata.production_lowering.iter().any(|entry| {
                    entry.function == caller
                        && entry.class == php_jit::JitProductionLoweringClass::CompiledNativeCall
                }),
                "optimizing caller did not retain a compiled native call"
            );
        }
        if native_optimization == NativeOptimizationPolicy::Optimizing {
            let dispatcher_imports = handle
                .relocatable_code()
                .expect("caller relocatable artifact")
                .relocations
                .iter()
                .filter_map(|relocation| match &relocation.target {
                    php_jit::JitRelocatableTarget::Helper(symbol)
                        if symbol.contains("call_dispatch")
                            || symbol.contains("function_resolve")
                            || symbol.contains("unwind") =>
                    {
                        Some(symbol.as_str())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(
                dispatcher_imports.is_empty(),
                "optimizing caller imported {dispatcher_imports:?}"
            );
        }
    }

    for native_optimization in [
        NativeOptimizationPolicy::Generic,
        NativeOptimizationPolicy::Optimizing,
    ] {
        let result = Vm::with_options(VmOptions {
            native_optimization,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            ..VmOptions::default()
        })
        .execute(nested_builtin_type_error_catch_unit());
        assert_eq!(
            result.return_value,
            Some(Value::Int(77)),
            "{native_optimization:?}: {result:#?}"
        );
        let counters = result.counters.expect("native counters");
        assert_eq!(counters.native_call_dynamic, 0, "{native_optimization:?}");
        assert_eq!(
            counters.native_transition_count, 0,
            "{native_optimization:?}"
        );
    }
}

#[test]
#[cfg(target_arch = "x86_64")]
fn compiled_caller_preserves_array_across_constant_key_callee_transition() {
    let (unit, callee) = optimizing_nested_constant_key_array_transition_unit();
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let baseline = VmOptions {
        native_optimization: NativeOptimizationPolicy::Generic,
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    };
    let baseline_handle = worker
        .resolve_native_function(&unit, callee, &baseline, &[])
        .expect("baseline callee");
    let optimizing = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    };
    let optimizing_handle = worker
        .resolve_native_function(&unit, callee, &optimizing, &[])
        .expect("optimizing callee");
    unit.prepared_deployment_image().generic_function_entries[callee.index()].store(
        baseline_handle
            .native_entry_address()
            .expect("baseline address"),
        std::sync::atomic::Ordering::Release,
    );
    unit.prepared_deployment_image().preferred_function_entries[callee.index()].store(
        optimizing_handle
            .native_entry_address()
            .expect("optimizing address"),
        std::sync::atomic::Ordering::Release,
    );

    let result = Vm::with_options_and_worker_state(optimizing, worker).execute(unit);
    let Some(Value::Array(array)) = result.return_value else {
        panic!("nested transition did not return an array: {result:#?}");
    };
    assert!(
        array
            .get(&php_runtime::api::ArrayKey::String("path".into()))
            .is_some(),
        "first constant-key insert was lost"
    );
    assert!(
        array
            .get(&php_runtime::api::ArrayKey::String("selector".into()))
            .is_some(),
        "second constant-key insert was lost"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn worker_request_pool_reuse_preserves_returned_array_and_resets_next_request() {
    let (unit, _) = optimizing_nested_constant_key_array_transition_unit();
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    };

    let first =
        Vm::with_options_and_worker_state(options.clone(), worker.clone()).execute(unit.clone());
    let second = Vm::with_options_and_worker_state(options, worker).execute(unit);
    let assert_complete = |result: &VmResult| {
        let Some(Value::Array(array)) = result.return_value.as_ref() else {
            panic!("pooled request did not return an array: {result:#?}");
        };
        assert_eq!(
            array
                .get(&php_runtime::api::ArrayKey::String("path".into()))
                .and_then(|value| match value {
                    Value::Array(nested) => {
                        nested.get(&php_runtime::api::ArrayKey::Int(0)).cloned()
                    }
                    _ => None,
                }),
            Some(Value::Int(41))
        );
        assert!(matches!(
            array.get(&php_runtime::api::ArrayKey::String("selector".into())),
            Some(Value::Null)
        ));
    };
    // Keep the first request's returned Value alive while the second
    // checks out the same native buffers. It must remain fully detached
    // from the worker-owned arenas.
    assert_complete(&first);
    assert_complete(&second);
    assert_complete(&first);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn compiled_caller_preserves_builtin_constants_across_callee_transition() {
    let (unit, callee) = optimizing_nested_builtin_constants_unit();
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let baseline = VmOptions {
        native_optimization: NativeOptimizationPolicy::Generic,
        native_cache: php_jit::NativeCacheMode::Off,
        collect_counters: true,
        ..VmOptions::default()
    };
    let baseline_handle = worker
        .resolve_native_function(&unit, callee, &baseline, &[])
        .expect("baseline callee");
    let optimizing = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::Off,
        collect_counters: true,
        ..VmOptions::default()
    };
    let optimizing_handle = worker
        .resolve_native_function(&unit, callee, &optimizing, &[])
        .expect("optimizing callee");
    unit.prepared_deployment_image().generic_function_entries[callee.index()].store(
        baseline_handle
            .native_entry_address()
            .expect("baseline address"),
        std::sync::atomic::Ordering::Release,
    );
    unit.prepared_deployment_image().preferred_function_entries[callee.index()].store(
        optimizing_handle
            .native_entry_address()
            .expect("optimizing address"),
        std::sync::atomic::Ordering::Release,
    );

    let result = Vm::with_options_and_worker_state(baseline, worker).execute(unit);
    assert_eq!(
        result.return_value,
        Some(Value::String(php_runtime::api::PhpString::from_bytes(
            b"widget!".to_vec()
        ))),
        "{result:#?}"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn instance_method_resolver_uses_exact_packed_entry_arity() {
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let result = Vm::with_options_and_worker_state(
        VmOptions {
            collect_counters: true,
            ..VmOptions::default()
        },
        worker.clone(),
    )
    .execute(direct_method_on_demand_unit());

    assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");
    let counters = result.counters.expect("diagnostic counters");
    assert_eq!(counters.native_call_direct, 1);
    assert_eq!(counters.native_same_unit_direct_executed, 1);
    assert_eq!(counters.native_call_dynamic, 0);
    assert_eq!(counters.native_transition_count, 0);
    let compile_stats = worker.native_compile_cache_stats();
    assert_eq!(compile_stats.entries, 2);
    assert_eq!(compile_stats.misses, 2);
    assert_eq!(compile_stats.insertions, 2);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn typed_function_on_demand_call_preserves_coercion() {
    let result = Vm::with_options(VmOptions {
        collect_counters: true,
        ..VmOptions::default()
    })
    .execute(typed_direct_call_unit(false));

    assert_eq!(result.return_value, Some(Value::Int(42)), "{result:#?}");
    let counters = result.counters.expect("diagnostic counters");
    assert_eq!(counters.native_call_direct, 1);
    assert_eq!(counters.native_same_unit_direct_executed, 1);
    assert_eq!(counters.native_call_dynamic, 0);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn typed_function_on_demand_call_preserves_throw() {
    let result = Vm::with_options(VmOptions {
        collect_counters: true,
        ..VmOptions::default()
    })
    .execute(typed_direct_call_unit(true));

    assert_eq!(
        result.status.exit_status(),
        php_runtime::api::ExitStatus::Fatal,
        "{result:#?}"
    );
    assert!(
            String::from_utf8_lossy(result.output.as_bytes()).contains(
                "Uncaught TypeError: typed_callee(): Argument #1 ($value) must be of type int, string given"
            ),
            "{result:#?}"
        );
    let counters = result.counters.expect("diagnostic counters");
    assert_eq!(counters.native_call_direct, 1);
    assert_eq!(counters.native_same_unit_direct_executed, 1);
    assert_eq!(counters.native_call_dynamic, 0);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn function_on_demand_call_preserves_runtime_diagnostic() {
    let result = Vm::new().execute(invalid_return_type_on_demand_unit());

    assert_eq!(
        result.status.exit_status(),
        php_runtime::api::ExitStatus::Fatal,
        "{result:#?}"
    );
    assert!(
            String::from_utf8_lossy(result.output.as_bytes()).contains(
                "Uncaught TypeError: invalid_return(): Return value must be of type string, array returned"
            ),
            "{result:#?}"
        );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn stable_builtin_avoids_generic_dynamic_dispatch() {
    let result = Vm::with_options(VmOptions {
        collect_counters: true,
        native_cache: php_jit::NativeCacheMode::Off,
        ..VmOptions::default()
    })
    .execute(direct_builtin_unit());

    assert_eq!(result.return_value, Some(Value::Int(6)), "{result:#?}");
    let counters = result.counters.expect("diagnostic counters");
    assert!(counters.native_call_direct <= 1);
    assert_eq!(
        counters.native_builtin_direct_eligible,
        counters.native_call_direct
    );
    assert_eq!(
        counters.native_builtin_direct_executed,
        counters.native_call_direct
    );
    assert_eq!(counters.native_call_dynamic, 0);
    assert_eq!(counters.native_call_argument_allocation_bytes, 0);
    let expected_frame_bytes = if counters.native_call_direct == 0 {
        0
    } else {
        (std::mem::size_of::<php_jit::JitNativeCallFrame>() + std::mem::size_of::<i64>()) as u64
    };
    assert_eq!(counters.native_call_frame_bytes, expected_frame_bytes);
    assert_eq!(counters.native_frame_arena_high_water_bytes, 0);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn type_predicate_bypasses_the_generic_call_frame() {
    let result = Vm::with_options(VmOptions {
        collect_counters: true,
        ..VmOptions::default()
    })
    .execute(direct_type_predicate_unit());

    assert_eq!(result.return_value, Some(Value::Bool(true)), "{result:#?}");
    let counters = result.counters.expect("diagnostic counters");
    assert_eq!(counters.native_call_direct, 0);
    assert_eq!(counters.native_builtin_direct_executed, 0);
    assert_eq!(counters.native_call_argument_allocation_bytes, 0);
    assert_eq!(counters.native_frame_arena_high_water_bytes, 0);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn baseline_does_not_inline_or_widen_for_constant_wrapper() {
    let result = Vm::with_options(VmOptions {
        collect_counters: true,
        ..VmOptions::default()
    })
    .execute(bounded_inline_unit());

    assert_eq!(result.return_value, Some(Value::Int(19)), "{result:#?}");
    let counters = result.counters.expect("diagnostic counters");
    assert_eq!(counters.native_inlined_calls, 0);
    assert_eq!(counters.native_inline_calls_removed, 0);
    assert_eq!(counters.native_call_direct, 1);
    assert_eq!(counters.native_call_dynamic, 0);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn generated_method_resolution_calls_published_generic_entry() {
    let vm = Vm::with_options(VmOptions {
        collect_counters: true,
        ..VmOptions::default()
    });
    let unit = polymorphic_method_pic_unit();
    let result = vm.execute(unit.clone());

    assert_eq!(result.return_value, Some(Value::Int(7)), "{result:#?}");
    let counters = result.counters.expect("diagnostic counters");
    assert_eq!(counters.native_method_monomorphic_eligible, 0);
    assert_eq!(counters.native_method_monomorphic_executed, 0);
    assert_eq!(counters.native_transition_count, 0);

    // Every body cell is published before execution. A second request selects
    // the same generated method entry without warming a request-local PIC.
    let warm = vm.execute(unit);
    assert_eq!(warm.return_value, Some(Value::Int(7)), "{warm:#?}");
    let warm_counters = warm.counters.expect("diagnostic counters");
    assert_eq!(warm_counters.native_method_monomorphic_executed, 0);
    assert_eq!(warm_counters.native_transition_count, 0);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn reached_method_upgrades_the_preferred_entry_from_baseline() {
    let tiering = crate::tiering::TieringOptions {
        collect_stats: true,
        ..crate::tiering::TieringOptions::default()
    };
    let worker = VmWorkerState::new(tiering.clone());
    let unit = polymorphic_method_pic_unit();
    let method = unit.unit().classes[0].methods[0].function;
    let result = Vm::with_options_and_worker_state(
        VmOptions {
            native_optimization: NativeOptimizationPolicy::Optimizing,
            native_cache: php_jit::NativeCacheMode::Off,
            collect_counters: true,
            tiering,
            ..VmOptions::default()
        },
        worker,
    )
    .execute(unit.clone());
    assert_eq!(result.return_value, Some(Value::Int(7)), "{result:#?}");

    let deadline = Instant::now() + Duration::from_secs(10);
    let (baseline, preferred) = loop {
        let baseline = unit.prepared_deployment_image().generic_function_entries[method.index()]
            .load(std::sync::atomic::Ordering::Acquire);
        let preferred = unit.prepared_deployment_image().preferred_function_entries[method.index()]
            .load(std::sync::atomic::Ordering::Acquire);
        if baseline != 0 && preferred != baseline {
            break (baseline, preferred);
        }
        assert!(
            Instant::now() < deadline,
            "a reached method did not publish baseline and upgraded preferred entries"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_ne!(baseline, 0);
    assert_ne!(preferred, baseline);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn deep_direct_recursion_hits_php_frame_limit_without_stack_abort() {
    let result = Vm::new().execute(unbounded_recursive_unit());

    assert!(!result.status.is_success(), "{result:#?}");
    assert_eq!(
        result.diagnostics.first().map(|diagnostic| diagnostic.id()),
        Some("E_PHP_VM_NATIVE_FRAME_LIMIT"),
        "{result:#?}"
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn native_compile_probe_uses_production_helpers_without_execution() {
    let report = Vm::new()
        .probe_cranelift(&returning_unit(42), Some("main"))
        .expect("native compile probe");
    assert_eq!(report.function_name, "main");
    assert!(matches!(
        report.result.status,
        php_jit::JitCompileStatus::Compiled
    ));
    assert!(
        Vm::new()
            .probe_cranelift(&returning_unit(42), Some("missing"))
            .expect_err("unknown function must be strict")
            .contains("function not found")
    );
}

#[test]
#[cfg(target_arch = "x86_64")]
fn native_loop_poll_reports_stable_execution_timeout() {
    let result = Vm::with_options(VmOptions {
        runtime_context: php_runtime::api::RuntimeContext::controlled_cli(
            "native-deadline-vm.php",
            Vec::new(),
        )
        .with_execution_time_limit(Some(Duration::ZERO)),
        ..VmOptions::default()
    })
    .execute(looping_unit());

    assert_eq!(
        result.status.exit_status(),
        php_runtime::api::ExitStatus::RuntimeError
    );
    assert_eq!(result.diagnostics.len(), 1, "{result:#?}");
    assert_eq!(result.diagnostics[0].id(), "E_PHP_VM_EXECUTION_TIMEOUT");
}

#[test]
fn declaration_units_are_native_cache_candidates() {
    let unit = returning_unit(42);
    assert!(native_cache_candidate(unit.unit(), unit.unit().entry));

    let mut declaration_unit = unit.unit().clone();
    declaration_unit.classes.push(ClassEntry {
        id: ClassId::new(0),
        name: "cacheddeclaration".to_owned(),
        display_name: "CachedDeclaration".to_owned(),
        parent: None,
        parent_display_name: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor: None,
        flags: ClassFlags::default(),
        span: IrSpan::new(declaration_unit.files[0].id, 6, 32),
    });
    assert!(native_cache_candidate(
        &declaration_unit,
        declaration_unit.entry
    ));
}

#[test]
#[cfg(target_arch = "x86_64")]
fn worker_cache_skips_region_rebuild_and_invalidates_exactly() {
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let unit = returning_unit(73);
    let options = VmOptions::default();

    let first =
        Vm::with_options_and_worker_state(options.clone(), worker.clone()).execute(unit.clone());
    let second =
        Vm::with_options_and_worker_state(options.clone(), worker.clone()).execute(unit.clone());
    assert_eq!(first.return_value, Some(Value::Int(73)));
    assert_eq!(second.return_value, Some(Value::Int(73)));
    assert_eq!(second.native_compile_nanos, 0);
    let warm_stats = worker.native_compile_cache_stats();
    assert_eq!(warm_stats.entries, 1);
    assert_eq!(warm_stats.hits, 1);
    assert_eq!(warm_stats.misses, 1);
    assert_eq!(warm_stats.insertions, 1);
    assert_eq!(warm_stats.evictions, 0);
    assert_eq!(warm_stats.compile_waits, 0);
    assert_eq!(warm_stats.compile_failures, 0);
    assert!(warm_stats.compile_time_nanos > 0);

    // A separately built artifact must not borrow handles merely because
    // its source and IR happen to be equal.
    let replacement = returning_unit(73);
    let replacement_result =
        Vm::with_options_and_worker_state(options, worker.clone()).execute(replacement);
    assert_eq!(replacement_result.return_value, Some(Value::Int(73)));

    // Optimization policy is part of the cache key even for the same
    // immutable compiled-unit allocation.
    let optimizing = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        ..VmOptions::default()
    };
    let optimizing_result =
        Vm::with_options_and_worker_state(optimizing, worker.clone()).execute(unit);
    assert_eq!(optimizing_result.return_value, Some(Value::Int(73)));
    let stats = worker.native_compile_cache_stats();
    assert_eq!(stats.entries, 3);
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 3);
    assert_eq!(stats.insertions, 3);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn loading_declaration_heavy_unit_compiles_only_entry_and_declares_other_cells() {
    let worker = VmWorkerState::new(crate::tiering::TieringOptions::default());
    let unit = declaration_heavy_unit();
    let result = Vm::with_options_and_worker_state(VmOptions::default(), worker.clone())
        .execute(unit.clone());

    assert_eq!(result.return_value, Some(Value::Int(17)), "{result:#?}");
    let stats = worker.native_compile_cache_stats();
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.insertions, 1);
    assert_eq!(
        unit.prepared_unit_stats().continuation_index_runs,
        1,
        "dormant declarations must not build RegionGraph-derived runtime metadata"
    );

    let manager = php_jit::global_code_manager().expect("global code manager");
    let mut published = 0;
    let mut unpublished = 0;
    for (index, function) in unit.unit().functions.iter().enumerate() {
        let key = php_jit::native_function_key(
            unit.prepared_ir_fingerprint().to_owned(),
            index as u32,
            function.params.len(),
            function.local_count,
            false,
            0,
        );
        let cell = manager.function_cell(&key).expect("declared function cell");
        match cell.state() {
            php_jit::NativeIndirectionState::Published => published += 1,
            php_jit::NativeIndirectionState::Declared
            | php_jit::NativeIndirectionState::Queued
            | php_jit::NativeIndirectionState::Compiling
            | php_jit::NativeIndirectionState::Failed => unpublished += 1,
            php_jit::NativeIndirectionState::Retired => panic!("fresh cell was retired"),
        }
    }
    assert_eq!(published, 1);
    assert_eq!(unpublished, 120);
}

#[test]
#[cfg(target_arch = "x86_64")]
fn vm_reloads_native_artifact_without_compilation() {
    let directory = std::env::temp_dir().join(format!(
        "phrust-vm-pna-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let unit = returning_unit(42);
    let first = Vm::with_options_and_worker_state(
        VmOptions {
            native_cache: php_jit::NativeCacheMode::ReadWrite,
            native_cache_dir: directory.clone(),
            native_cache_stats: true,
            ..VmOptions::default()
        },
        VmWorkerState::isolated_for_restart_test(),
    )
    .execute(unit.clone());
    assert_eq!(
        first.return_value,
        Some(Value::Int(42)),
        "cache population result: {first:#?}"
    );
    assert_eq!(first.native_cache_stats.unwrap().writes, 1);

    let second = Vm::with_options_and_worker_state(
        VmOptions {
            native_cache: php_jit::NativeCacheMode::Read,
            native_cache_dir: directory.clone(),
            native_cache_stats: true,
            ..VmOptions::default()
        },
        VmWorkerState::isolated_for_restart_test(),
    )
    .execute(unit);
    assert_eq!(
        second.return_value,
        Some(Value::Int(42)),
        "cached execution result: {second:#?}"
    );
    assert_eq!(second.native_cache_stats.unwrap().hits, 1);
    assert_eq!(second.native_compile_nanos, 0);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(target_arch = "x86_64")]
fn function_on_demand_callee_reloads_without_compilation() {
    let directory = std::env::temp_dir().join(format!(
        "phrust-vm-pna-callee-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let unit = direct_call_unit();
    let first_worker = VmWorkerState::isolated_for_restart_test();
    let first = Vm::with_options_and_worker_state(
        VmOptions {
            native_cache: php_jit::NativeCacheMode::ReadWrite,
            native_cache_dir: directory.clone(),
            ..VmOptions::default()
        },
        first_worker.clone(),
    )
    .execute(unit.clone());
    assert_eq!(first.return_value, Some(Value::Int(42)), "{first:#?}");
    assert_eq!(first_worker.native_compile_cache_stats().misses, 2);
    let artifacts = std::fs::read_dir(&directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "pna")
        })
        .count();
    assert_eq!(
        artifacts, 2,
        "root and demanded callee must persist separately"
    );

    let second_worker = VmWorkerState::isolated_for_restart_test();
    let second = Vm::with_options_and_worker_state(
        VmOptions {
            native_cache: php_jit::NativeCacheMode::Read,
            native_cache_dir: directory.clone(),
            ..VmOptions::default()
        },
        second_worker.clone(),
    )
    .execute(unit);
    assert_eq!(second.return_value, Some(Value::Int(42)), "{second:#?}");
    assert_eq!(second.native_compile_nanos, 0);
    assert_eq!(second_worker.native_compile_cache_stats().misses, 0);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(target_arch = "x86_64")]
fn optimizing_cached_direct_call_restarts_with_prevalidated_preferred_cells() {
    let directory = std::env::temp_dir().join(format!(
        "phrust-vm-pna-optimizing-direct-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let unit = direct_call_unit_with_identity(9_952, "native-cached-optimizing-direct-call.php");
    let options = VmOptions {
        native_optimization: NativeOptimizationPolicy::Optimizing,
        native_cache: php_jit::NativeCacheMode::ReadWrite,
        native_cache_dir: directory.clone(),
        collect_counters: true,
        ..VmOptions::default()
    };
    let first =
        Vm::with_options_and_worker_state(options, VmWorkerState::isolated_for_restart_test())
            .execute(unit.clone());
    assert_eq!(first.return_value, Some(Value::Int(42)), "{first:#?}");
    assert_eq!(first.counters.as_ref().unwrap().native_transition_count, 0);

    let restarted = CompiledUnit::from(unit.unit().clone());
    let restart_worker = VmWorkerState::isolated_for_restart_test();
    let second = Vm::with_options_and_worker_state(
        VmOptions {
            native_optimization: NativeOptimizationPolicy::Optimizing,
            native_cache: php_jit::NativeCacheMode::Read,
            native_cache_dir: directory.clone(),
            collect_counters: true,
            ..VmOptions::default()
        },
        restart_worker.clone(),
    )
    .execute(restarted.clone());
    assert_eq!(second.return_value, Some(Value::Int(42)), "{second:#?}");
    assert_eq!(second.native_compile_nanos, 0);
    assert_eq!(restart_worker.native_compile_cache_stats().misses, 0);
    assert_eq!(
        second.counters.as_ref().unwrap().native_transition_count,
        0,
        "the cached root must not execute before its direct callee cell is callable"
    );

    let callee = restarted
        .unit()
        .function_table
        .iter()
        .find_map(|entry| (entry.name == "callee").then_some(entry.function))
        .expect("callee function id");
    let deployment = restarted.prepared_deployment_image();
    let callee_baseline = deployment.generic_function_entries[callee.index()]
        .load(std::sync::atomic::Ordering::Acquire);
    let callee_preferred = deployment.preferred_function_entries[callee.index()]
        .load(std::sync::atomic::Ordering::Acquire);
    assert_ne!(callee_baseline, 0);
    assert_eq!(
        callee_preferred, callee_baseline,
        "restart publication must prepare the direct callee before invoking the optimizer"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(target_arch = "x86_64")]
fn worker_fast_entry_cache_reuses_loaded_artifact_without_identity_rebuild() {
    let directory = std::env::temp_dir().join(format!(
        "phrust-vm-loaded-unit-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let worker = VmWorkerState::isolated_for_restart_test();
    let before = worker.loaded_native_unit_stats();
    let entry_hits_before = worker.resolved_native_entry_hits();
    let unit = direct_call_unit();
    let options = VmOptions {
        native_cache: php_jit::NativeCacheMode::ReadWrite,
        native_cache_dir: directory.clone(),
        ..VmOptions::default()
    };
    let first =
        Vm::with_options_and_worker_state(options.clone(), worker.clone()).execute(unit.clone());
    assert_eq!(first.return_value, Some(Value::Int(42)), "{first:#?}");

    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "pna") {
            std::fs::remove_file(path).unwrap();
        }
    }

    let second = Vm::with_options_and_worker_state(options, worker.clone()).execute(unit);
    assert_eq!(second.return_value, Some(Value::Int(42)), "{second:#?}");
    assert_eq!(second.native_compile_nanos, 0);
    let loaded = worker.loaded_native_unit_stats();
    assert_eq!(loaded.maps.saturating_sub(before.maps), 2);
    assert_eq!(
        loaded
            .entry_table_constructions
            .saturating_sub(before.entry_table_constructions),
        2
    );
    // The root entry still follows the top-level cache path. Its demanded
    // callee now comes directly from the deployment-owned atomic cell;
    // the deleted worker entry cache must receive no warm lookup at all.
    assert_eq!(loaded.hits.saturating_sub(before.hits), 1);
    assert_eq!(
        worker
            .resolved_native_entry_hits()
            .saturating_sub(entry_hits_before),
        0
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[cfg(target_arch = "x86_64")]
fn vm_reloads_helper_using_native_artifact_without_compilation() {
    let directory = std::env::temp_dir().join(format!(
        "phrust-vm-pna-helper-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut ir = returning_unit(91).unit().clone();
    ir.functions[ir.entry.index()].return_type = None;
    let unit = CompiledUnit::from(ir);
    let first = Vm::with_options_and_worker_state(
        VmOptions {
            native_cache: php_jit::NativeCacheMode::ReadWrite,
            native_cache_dir: directory.clone(),
            native_cache_stats: true,
            ..VmOptions::default()
        },
        VmWorkerState::isolated_for_restart_test(),
    )
    .execute(unit.clone());
    assert_eq!(first.return_value, Some(Value::Int(91)), "{first:#?}");
    assert_eq!(first.native_cache_stats.unwrap().writes, 1);

    let second = Vm::with_options_and_worker_state(
        VmOptions {
            native_cache: php_jit::NativeCacheMode::Read,
            native_cache_dir: directory.clone(),
            native_cache_stats: true,
            ..VmOptions::default()
        },
        VmWorkerState::isolated_for_restart_test(),
    )
    .execute(unit);
    assert_eq!(second.return_value, Some(Value::Int(91)), "{second:#?}");
    assert_eq!(second.native_cache_stats.unwrap().hits, 1);
    assert_eq!(second.native_compile_nanos, 0);
    std::fs::remove_dir_all(directory).unwrap();
}
