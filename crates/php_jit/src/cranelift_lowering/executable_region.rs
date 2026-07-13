use super::*;

pub(super) fn compile_region_graph_native(
    unit: &IrUnit,
    region: &RegionGraph,
    native_call_dispatch: usize,
    request: &JitCompileRequest,
) -> Result<NativeScalarRegionCompileResult, CraneliftLoweringError> {
    validate_region_native_coverage(region)?;
    let regions = collect_region_graphs(unit, region)?;
    let function = region.function;
    let arity = region_arity(region)?;
    let fast_path_hits = regions
        .values()
        .map(|region| region.fast_path_operations)
        .sum();
    let has_control_flow = regions.values().any(RegionGraph::has_control_flow);
    let needs_call_trampoline = regions
        .values()
        .any(RegionGraph::has_native_trampoline_calls);
    if needs_call_trampoline && native_call_dispatch == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_CALL_TRAMPOLINE",
            "dynamic or complex call requires the typed native dispatch trampoline",
        ));
    }
    let native_call_symbol = NATIVE_CALL_DISPATCH_SYMBOL.to_owned();
    let mut imports = vec![(
        "region-runtime-helper-abi".to_owned(),
        region.compile_metadata.helper_abi_hash as usize,
    )];
    if needs_call_trampoline {
        imports.push((native_call_symbol.clone(), native_call_dispatch));
    }
    let import_refs = imports
        .iter()
        .map(|(name, address)| (name.as_str(), *address))
        .collect::<Vec<_>>();
    let compiled = compile_managed_native(
        request,
        function,
        "executable-region-v2",
        &import_refs,
        |module, name| {
            let native_call_helper = if needs_call_trampoline {
                let pointer_type = module.target_config().pointer_type();
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                Some(
                    module
                        .declare_function(&native_call_symbol, Linkage::Import, &signature)
                        .map_err(|error| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_NATIVE_CALL_TRAMPOLINE",
                                format!("failed to declare native call trampoline: {error}"),
                            )
                        })?,
                )
            } else {
                None
            };
            let mut functions = BTreeMap::new();
            for candidate in regions.values() {
                let symbol = if candidate.function == function {
                    name.to_owned()
                } else {
                    format!("{name}.callee.{}", candidate.function.raw())
                };
                let signature = region_graph_signature(module, candidate)?;
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
                let (candidate_bytes, mut candidate_ranges) = define_region_graph_function(
                    module,
                    candidate,
                    func_id,
                    &functions,
                    native_call_helper,
                )?;
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
                region_graph_metadata(region.local_count, regions.values(), native_pc_ranges);
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

fn collect_region_graphs(
    unit: &IrUnit,
    root: &RegionGraph,
) -> Result<BTreeMap<FunctionId, RegionGraph>, CraneliftLoweringError> {
    let mut regions = BTreeMap::new();
    regions.insert(root.function, root.clone());
    let mut pending = root.direct_callees();
    while let Some(function) = pending.pop() {
        if regions.contains_key(&function) {
            continue;
        }
        let region = BaselineRegionBuilder::build(unit, function, &root.compile_metadata).map_err(
            |error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DIRECT_CALLEE",
                    format!(
                        "direct callee {} is not executable: {error}",
                        function.raw()
                    ),
                )
            },
        )?;
        validate_region_native_coverage(&region)?;
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

fn validate_region_native_coverage(region: &RegionGraph) -> Result<(), CraneliftLoweringError> {
    if region.params.len() > 4 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_MISSING_FUNCTION_ABI_LOWERING",
            format!(
                "function {} has {} parameters; native status ABI currently supports four",
                region.function_name,
                region.params.len()
            ),
        ));
    }
    if region.local_count as usize > crate::JIT_DEOPT_MAX_SLOTS {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_MISSING_DEOPT_SLOT_LOWERING",
            format!(
                "function {} has {} locals; native state ABI supports {}",
                region.function_name,
                region.local_count,
                crate::JIT_DEOPT_MAX_SLOTS
            ),
        ));
    }
    if region.return_type.as_ref() != Some(&IrReturnType::Int) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_MISSING_RETURN_ABI_LOWERING",
            format!(
                "function {} return metadata {:?} has no native ABI lowering",
                region.function_name, region.return_type
            ),
        ));
    }
    if let Some(param) = region.params.iter().find(|param| {
        param.by_ref
            || param.variadic
            || param.default.is_some()
            || param.type_.as_ref() != Some(&IrReturnType::Int)
    }) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_MISSING_PARAMETER_ABI_LOWERING",
            format!(
                "function {} parameter ${} metadata has no native ABI lowering",
                region.function_name, param.name
            ),
        ));
    }
    if !region.captures.is_empty() || region.flags.is_generator {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_MISSING_FUNCTION_STATE_LOWERING",
            format!(
                "function {} flags={:?} returns_by_ref={} captures={} has no native state lowering",
                region.function_name,
                region.flags,
                region.returns_by_ref,
                region.captures.len()
            ),
        ));
    }
    for block in &region.blocks {
        for instruction in &block.instructions {
            if let RegionInstructionKind::CompileTimeFatal { diagnostic_id } = &instruction.kind {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_IR_COMPILE_FATAL",
                    format!(
                        "function={} diagnostic={} span={}:{}-{}",
                        region.function_name,
                        diagnostic_id,
                        instruction.span.file.raw(),
                        instruction.span.start,
                        instruction.span.end
                    ),
                ));
            }
            if matches!(instruction.kind, RegionInstructionKind::MissingLowering) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_MISSING_INSTRUCTION_LOWERING",
                    format!(
                        "function={} instruction={:?} span={}:{}-{}",
                        region.function_name,
                        instruction.source_kind,
                        instruction.span.file.raw(),
                        instruction.span.start,
                        instruction.span.end
                    ),
                ));
            }
        }
        if matches!(block.terminator, RegionTerminator::MissingLowering) {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_MISSING_TERMINATOR_LOWERING",
                format!(
                    "function={} terminator={:?} span={}:{}-{}",
                    region.function_name,
                    block.source_terminator,
                    block.terminator_span.file.raw(),
                    block.terminator_span.start,
                    block.terminator_span.end
                ),
            ));
        }
    }
    Ok(())
}

fn region_arity(region: &RegionGraph) -> Result<u8, CraneliftLoweringError> {
    region.arity().try_into().map_err(|_| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_REGION_ARITY",
            "executable Region IR arity does not fit the native ABI",
        )
    })
}

fn region_graph_signature(
    module: &JITModule,
    region: &RegionGraph,
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

fn define_region_graph_function(
    module: &mut JITModule,
    region: &RegionGraph,
    func_id: FuncId,
    functions: &BTreeMap<FunctionId, FuncId>,
    native_call_helper: Option<FuncId>,
) -> Result<(u64, Vec<crate::JitNativePcRange>), CraneliftLoweringError> {
    let arity = region_arity(region)?;
    let pointer_type = module.target_config().pointer_type();
    let mut ctx = module.make_context();
    ctx.func.signature = region_graph_signature(module, region)?;
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
        let pending_status = builder.declare_var(types::I32);
        let pending_value = builder.declare_var(types::I64);
        let continue_status = builder
            .ins()
            .iconst(types::I32, i64::from(crate::JitCallStatus::CONTINUE.0));
        let empty_value = builder.ins().iconst(types::I64, 0);
        builder.def_var(pending_status, continue_status);
        builder.def_var(pending_value, empty_value);
        for (param, value) in region
            .parameter_locals
            .iter()
            .zip(params.iter().copied().take(usize::from(arity)))
        {
            builder.def_var(local_variable(&locals, *param)?, value);
        }
        let handler_resume_blocks = region
            .exception_regions
            .iter()
            .flat_map(|handler| [handler.catch, handler.finally])
            .flatten()
            .collect::<std::collections::BTreeSet<_>>();
        for target in handler_resume_blocks {
            let loader = builder.create_block();
            let next = builder.create_block();
            let encoded_resume = crate::native_handler_resume_id(target);
            let requested =
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, resume_id, i64::from(encoded_resume));
            builder.ins().brif(requested, loader, &[], next, &[]);
            builder.switch_to_block(loader);
            let status = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                resume_state,
                std::mem::offset_of!(crate::JitDeoptState, control_status) as i32,
            );
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                resume_state,
                std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
            );
            builder.def_var(pending_status, status);
            builder.def_var(pending_value, value);
            let target_block = region.blocks.get(target.index()).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NATIVE_HANDLER",
                    format!("native handler block {} is missing", target.raw()),
                )
            })?;
            for local in &target_block.entry_live_locals {
                let offset = 24_i32.saturating_add((local.raw() as i32).saturating_mul(8));
                let local_value =
                    builder
                        .ins()
                        .load(types::I64, MemFlagsData::new(), resume_state, offset);
                builder.def_var(local_variable(&locals, *local)?, local_value);
            }
            builder.ins().jump(cranelift_block(&blocks, target)?, &[]);
            builder.switch_to_block(next);
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
                    native_call_helper,
                    &blocks,
                    &locals,
                    &mut registers,
                    instruction,
                    result_out,
                    deopt_out,
                    pending_status,
                    pending_value,
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
                pending_status,
                pending_value,
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
            (source != 0 && source != u32::MAX).then_some(crate::JitNativePcRange {
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

fn region_graph_metadata<'a>(
    root_local_count: u32,
    regions: impl Iterator<Item = &'a RegionGraph>,
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
                    RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Function {
                            function: Some(_),
                            ..
                        },
                        ..
                    })
                )
            })
            .count() as u64,
        continuations,
        native_pc_ranges,
        osr_entries,
        exception_handlers: regions
            .iter()
            .flat_map(|region| {
                region.exception_regions.iter().filter_map(move |handler| {
                    let enter_continuation = region
                        .blocks
                        .get(handler.block.index())?
                        .instructions
                        .iter()
                        .find(|instruction| instruction.id == handler.instruction)?
                        .continuation_id;
                    Some(crate::JitExceptionHandlerMetadata {
                        function: region.function,
                        enter_continuation,
                        catch: handler.catch,
                        catch_types: handler.catch_types.clone(),
                        finally: handler.finally,
                        after: handler.after,
                        exception_local: handler.exception_local,
                    })
                })
            })
            .collect(),
        safepoints: regions
            .iter()
            .flat_map(|region| {
                region.blocks.iter().flat_map(move |block| {
                    block.instructions.iter().filter_map(move |instruction| {
                        crate::region_ir::baseline_instruction_lowering(&instruction.source_kind)
                            .requires_safepoint
                            .then(|| crate::JitNativeSafepointMetadata {
                                function: region.function,
                                continuation_id: instruction.continuation_id,
                                baseline_frame_slots: instruction.live_locals.clone(),
                                optimized_roots_required: region.compile_metadata.tier
                                    == NativeCompilerTier::Optimizing,
                            })
                    })
                })
            })
            .collect(),
    }
}
