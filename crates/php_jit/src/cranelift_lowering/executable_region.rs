use super::*;

pub(super) fn compile_executable_region_native(
    unit: &IrUnit,
    region: &ExecutableRegion,
    request: &JitCompileRequest,
) -> Result<NativeScalarRegionCompileResult, CraneliftLoweringError> {
    let regions = collect_executable_regions(unit, region)?;
    let function = region.function;
    let arity = region_arity(region)?;
    let fast_path_hits = regions
        .values()
        .map(|region| region.fast_path_operations)
        .sum();
    let has_control_flow = regions.values().any(ExecutableRegion::has_control_flow);
    let compiled = compile_managed_native(
        request,
        function,
        "executable-region-v2",
        &[],
        |module, name| {
            let mut functions = BTreeMap::new();
            for candidate in regions.values() {
                let symbol = if candidate.function == function {
                    name.to_owned()
                } else {
                    format!("{name}.callee.{}", candidate.function.raw())
                };
                let signature = executable_region_signature(module, candidate)?;
                let func_id = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare executable region {symbol}: {error}"),
                        )
                    })?;
                functions.insert(candidate.function, func_id);
            }

            let mut code_bytes = 0_u64;
            let mut native_pc_ranges = Vec::new();
            for candidate in regions.values() {
                let func_id = functions[&candidate.function];
                let (candidate_bytes, mut candidate_ranges) =
                    define_executable_region_function(module, candidate, func_id, &functions)?;
                code_bytes = code_bytes.saturating_add(candidate_bytes);
                native_pc_ranges.append(&mut candidate_ranges);
            }
            module.finalize_definitions().map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize executable region call graph: {error}"),
                )
            })?;
            let root = functions[&function];
            let address = module.get_finalized_function(root) as usize;
            let region_state_metadata =
                executable_region_metadata(region.local_count, regions.values(), native_pc_ranges);
            let handle = JitFunctionHandle::i64_status_out_native(
                u64::from(function.raw()) + 1,
                request.region_id.clone(),
                CraneliftCompilerIdentity,
                address,
                arity,
                code_bytes,
                0,
                fast_path_hits,
                region_state_metadata,
            );
            Ok((handle, code_bytes))
        },
    )?;
    Ok(NativeScalarRegionCompileResult {
        handle: compiled.handle,
        code_bytes: compiled.code_bytes,
        fast_path_hits,
        has_control_flow,
    })
}

fn collect_executable_regions(
    unit: &IrUnit,
    root: &ExecutableRegion,
) -> Result<BTreeMap<FunctionId, ExecutableRegion>, CraneliftLoweringError> {
    let mut regions = BTreeMap::new();
    regions.insert(root.function, root.clone());
    let mut pending = root.direct_callees();
    while let Some(function) = pending.pop() {
        if regions.contains_key(&function) {
            continue;
        }
        let region = build_executable_region(unit, function).map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_DIRECT_CALLEE",
                format!(
                    "direct callee {} is not executable: {error}",
                    function.raw()
                ),
            )
        })?;
        pending.extend(region.direct_callees());
        regions.insert(function, region);
    }
    for region in regions.values() {
        region.verify().map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_REGION_VERIFY", error.to_string())
        })?;
    }
    Ok(regions)
}

fn region_arity(region: &ExecutableRegion) -> Result<u8, CraneliftLoweringError> {
    region.arity().try_into().map_err(|_| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_REGION_ARITY",
            "executable Region IR arity does not fit the native ABI",
        )
    })
}

fn executable_region_signature(
    module: &JITModule,
    region: &ExecutableRegion,
) -> Result<Signature, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    for _ in 0..region_arity(region)? {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(types::I32));
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    Ok(signature)
}

fn define_executable_region_function(
    module: &mut JITModule,
    region: &ExecutableRegion,
    func_id: FuncId,
    functions: &BTreeMap<FunctionId, FuncId>,
) -> Result<(u64, Vec<crate::JitNativePcRange>), CraneliftLoweringError> {
    let arity = region_arity(region)?;
    let pointer_type = module.target_config().pointer_type();
    let mut ctx = module.make_context();
    ctx.func.signature = executable_region_signature(module, region)?;
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);
        let blocks = create_region_cranelift_blocks(&mut builder, region)?;
        let normal_entry = blocks.first().copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_CONTROL_FLOW",
                "executable region requires at least one block",
            )
        })?;
        let native_entry = builder.create_block();
        builder.append_block_params_for_function_params(native_entry);
        builder.switch_to_block(native_entry);
        let params = builder.block_params(native_entry).to_vec();
        let result_out = params[usize::from(arity)];
        let deopt_out = params[usize::from(arity) + 1];
        let resume_id = params[usize::from(arity) + 2];
        let resume_state = params[usize::from(arity) + 3];
        let mut locals = BTreeMap::new();
        for local_index in 0..region.local_count {
            locals.insert(LocalId::new(local_index), builder.declare_var(types::I64));
        }
        for (param, value) in region
            .parameter_locals
            .iter()
            .zip(params.iter().copied().take(usize::from(arity)))
        {
            builder.def_var(local_variable(&locals, *param)?, value);
        }
        for osr_entry in region.osr_entries() {
            let loader = builder.create_block();
            let next = builder.create_block();
            let requested =
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, resume_id, i64::from(osr_entry.id));
            builder.ins().brif(requested, loader, &[], next, &[]);
            builder.switch_to_block(loader);
            for local in &osr_entry.live_locals {
                let offset = 24_i32.saturating_add((local.raw() as i32).saturating_mul(8));
                let value =
                    builder
                        .ins()
                        .load(types::I64, MemFlagsData::new(), resume_state, offset);
                builder.def_var(local_variable(&locals, *local)?, value);
            }
            builder
                .ins()
                .jump(cranelift_block(&blocks, osr_entry.block)?, &[]);
            builder.switch_to_block(next);
        }
        builder.ins().jump(normal_entry, &[]);

        for region_block in &region.blocks {
            builder.switch_to_block(cranelift_block(&blocks, region_block.id)?);
            let mut registers = BTreeMap::new();
            for instruction in &region_block.instructions {
                builder.set_srcloc(ir::SourceLoc::new(
                    instruction.continuation_id.saturating_add(1),
                ));
                lower_region_instruction(
                    module,
                    &mut builder,
                    functions,
                    &locals,
                    &mut registers,
                    instruction,
                    deopt_out,
                    region.function,
                    region.local_count,
                    pointer_type,
                )?;
            }
            builder.set_srcloc(ir::SourceLoc::new(
                region_block.terminator_continuation_id.saturating_add(1),
            ));
            lower_region_terminator(
                &mut builder,
                &blocks,
                &locals,
                &registers,
                result_out,
                &region_block.terminator,
            )?;
        }
        builder.seal_all_blocks();
        builder.finalize();
    }
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("Cranelift verifier rejected executable Region IR: {error}"),
        )
    })?;
    module.define_function(func_id, &mut ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define native function: {error}"),
        )
    })?;
    let code_bytes = ctx
        .compiled_code()
        .map(|compiled| compiled.code_buffer().len() as u64)
        .unwrap_or(0);
    let native_pc_ranges = ctx
        .compiled_code()
        .into_iter()
        .flat_map(|compiled| compiled.buffer.get_srclocs_sorted())
        .filter_map(|range| {
            let source = range.loc.bits();
            (source != 0).then_some(crate::JitNativePcRange {
                function: region.function,
                start: range.start,
                end: range.end,
                continuation_id: source - 1,
            })
        })
        .collect();
    module.clear_context(&mut ctx);
    Ok((code_bytes, native_pc_ranges))
}

fn executable_region_metadata<'a>(
    root_local_count: u32,
    regions: impl Iterator<Item = &'a ExecutableRegion>,
    native_pc_ranges: Vec<crate::JitNativePcRange>,
) -> crate::JitRegionStateMetadata {
    let regions = regions.collect::<Vec<_>>();
    let continuations = regions
        .iter()
        .flat_map(|region| {
            region.blocks.iter().flat_map(move |block| {
                block
                    .instructions
                    .iter()
                    .map(move |instruction| crate::JitContinuationMetadata {
                        id: instruction.continuation_id,
                        function: region.function,
                        block: block.id,
                        instruction: Some(instruction.id),
                        span: instruction.span,
                        live_locals: instruction.live_locals.clone(),
                    })
                    .chain(std::iter::once(crate::JitContinuationMetadata {
                        id: block.terminator_continuation_id,
                        function: region.function,
                        block: block.id,
                        instruction: None,
                        span: block.terminator_span,
                        live_locals: block.terminator_live_locals.clone(),
                    }))
            })
        })
        .collect();
    let osr_entries = regions
        .iter()
        .flat_map(|region| {
            region
                .osr_entries()
                .into_iter()
                .map(move |entry| crate::JitOsrEntryMetadata {
                    id: entry.id,
                    function: region.function,
                    block: entry.block,
                    continuation_id: entry.continuation_id,
                    live_locals: entry.live_locals,
                })
        })
        .collect();
    crate::JitRegionStateMetadata {
        local_count: root_local_count,
        compiled_to_compiled_call_sites: regions
            .iter()
            .flat_map(|region| &region.blocks)
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(
                    instruction.kind,
                    ExecutableRegionInstructionKind::DirectCall { .. }
                )
            })
            .count() as u64,
        continuations,
        native_pc_ranges,
        osr_entries,
    }
}
