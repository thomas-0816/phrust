use super::*;
use std::collections::BTreeSet;

// Each unit function is also published as its own native entry. Large source
// units therefore compile one root per module instead of reproducing the same
// transitive bodies dozens of times. Small units retain bounded native-to-
// native direct-call graphs. Calls beyond the selected bound use the typed
// native trampoline rather than rebuilding a transitive call graph.
const SMALL_UNIT_MAX_LOCAL_CALL_GRAPH_FUNCTIONS: usize = 4;
const LARGE_UNIT_FUNCTION_THRESHOLD: usize = 16;
const MAX_LOCAL_CALLEE_REGISTERS: u32 = 1024;

const fn max_local_call_graph_functions(unit_function_count: usize) -> usize {
    if unit_function_count > LARGE_UNIT_FUNCTION_THRESHOLD {
        1
    } else {
        SMALL_UNIT_MAX_LOCAL_CALL_GRAPH_FUNCTIONS
    }
}

fn region_contains(
    region: &RegionGraph,
    predicate: impl Fn(&RegionInstructionKind) -> bool,
) -> bool {
    region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| predicate(&instruction.kind))
}

fn declare_value_operation(
    module: &mut JITModule,
    symbol: &str,
    arity: u8,
    address: usize,
) -> Result<NativeHelper, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I32));
    for _ in 0..arity {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    let import_symbol = native_helper_import_symbol(symbol, address);
    let function = module
        .declare_function(&import_symbol, Linkage::Import, &signature)
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                format!("failed to declare {symbol}: {error}"),
            )
        })?;
    Ok(NativeHelper { function })
}

fn declare_native_helper(
    module: &mut JITModule,
    symbol: &str,
    signature: &ir::Signature,
    address: usize,
) -> Result<NativeHelper, CraneliftLoweringError> {
    let import_symbol = native_helper_import_symbol(symbol, address);
    let function = module
        .declare_function(&import_symbol, Linkage::Import, signature)
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                format!("failed to declare {symbol}: {error}"),
            )
        })?;
    Ok(NativeHelper { function })
}

pub(super) fn compile_region_graph_native(
    unit: &IrUnit,
    region: &RegionGraph,
    runtime_helpers: crate::JitRuntimeHelperAddresses,
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
    let mut trampoline_functions = regions
        .iter()
        .filter_map(|(function, region)| {
            (!region.exception_regions.is_empty()
                || region
                    .params
                    .iter()
                    .any(|parameter| parameter.type_.is_some())
                || region.params.iter().any(|parameter| parameter.by_ref)
                || region_contains(region, |kind| {
                    matches!(
                        kind,
                        RegionInstructionKind::NativeControl(RegionNativeControl::Throw { .. })
                            | RegionInstructionKind::NativeDynamicCode(
                                RegionNativeDynamicCode::MakeClosure { .. }
                            )
                    )
                })
                || region.attributes.iter().any(|attribute| {
                    attribute
                        .resolved_name
                        .as_deref()
                        .or(attribute.fallback_name.as_deref())
                        .unwrap_or(&attribute.name)
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case("deprecated")
                }))
            .then_some(*function)
        })
        .collect::<BTreeSet<_>>();
    loop {
        let callers = regions
            .iter()
            .filter_map(|(function, region)| {
                region
                    .direct_callees()
                    .iter()
                    .any(|callee| trampoline_functions.contains(callee))
                    .then_some(*function)
            })
            .collect::<Vec<_>>();
        let previous = trampoline_functions.len();
        trampoline_functions.extend(callers);
        if trampoline_functions.len() == previous {
            break;
        }
    }
    let needs_call_trampoline = regions.values().any(|region| {
        region.has_native_trampoline_calls()
            || region
                .direct_callees()
                .iter()
                .any(|callee| !regions.contains_key(callee))
            || region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(call) if
                    call.direct_compiled_target().is_some_and(|target| {
                        trampoline_functions.contains(&target)
                    })
                )
            })
    });
    if needs_call_trampoline && runtime_helpers.native_call_dispatch == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_CALL_TRAMPOLINE",
            "dynamic or complex call requires the typed native dispatch trampoline",
        ));
    }
    let needs_dynamic_code = regions.values().any(RegionGraph::has_native_dynamic_code);
    if needs_dynamic_code && runtime_helpers.native_dynamic_code == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_DYNAMIC_CODE",
            "include, eval, or runtime declaration requires the native dynamic-code compiler",
        ));
    }
    let native_call_symbol = NATIVE_CALL_DISPATCH_SYMBOL.to_owned();
    let native_dynamic_code_symbol = NATIVE_DYNAMIC_CODE_SYMBOL.to_owned();
    let needs_unary = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Unary { .. }
                    | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::EmptyLocal { .. }
            )
        })
    });
    let needs_binary = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::Binary { .. })
        })
    });
    let needs_compare = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Compare { .. }
                    | RegionInstructionKind::IssetDim { .. }
                    | RegionInstructionKind::IssetLocal { .. }
            )
        })
    });
    let needs_cast = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Cast { .. }
                    | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::EmptyLocal { .. }
            )
        })
    });
    let needs_echo = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::Echo { .. })
        })
    });
    let needs_local_fetch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::LoadLocal { .. }
                    | RegionInstructionKind::FetchDim {
                        array: RegionOperand::Local(_),
                        ..
                    }
                    | RegionInstructionKind::AssignDim { .. }
                    | RegionInstructionKind::AppendDim { .. }
                    | RegionInstructionKind::UnsetDim { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
                    | RegionInstructionKind::IssetDim { .. }
                    | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::IssetLocal { .. }
                    | RegionInstructionKind::EmptyLocal { .. }
            )
        })
    });
    let needs_local_store = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::StoreLocal { .. }
                    | RegionInstructionKind::AssignLocalResult { .. }
                    | RegionInstructionKind::AssignDim { .. }
                    | RegionInstructionKind::AppendDim { .. }
                    | RegionInstructionKind::UnsetDim { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
            )
        })
    });
    let needs_value_lifecycle = true;
    // Local publication is part of the native frame ABI, not just explicit
    // PHP reference syntax.  Stores, unsets, foreach-by-reference and array
    // root updates can all publish a local through the same helper.  Keep the
    // helper available for every executable region so adding publication to a
    // lowering cannot accidentally make an otherwise supported function
    // uncompilable.
    let needs_reference_bind = true;
    let _has_explicit_reference_bind = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::BindReference { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
                    | RegionInstructionKind::BindReferenceIntoDim { .. }
                    | RegionInstructionKind::BindReferenceProperty { .. }
                    | RegionInstructionKind::BindReferenceFromProperty { .. }
                    | RegionInstructionKind::BindReferenceFromPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
                    | RegionInstructionKind::InitStaticLocal { .. }
            ) || matches!(kind, RegionInstructionKind::NativeCall(call) if
                call.needs_local_reference_binding()
                    || call.direct_compiled_target().is_some_and(|target| {
                        regions.get(&target).is_some_and(|callee| {
                            callee.params.iter().any(|parameter| parameter.by_ref)
                        })
                    })
            )
        })
    });
    let needs_return_check = true;
    let needs_exception_new = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::NativeControl(RegionNativeControl::MakeException { .. })
            )
        })
    });
    let needs_array_new = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NewArray { .. })
                || matches!(kind, RegionInstructionKind::NativeCall(call) if call.variadic)
        })
    });
    let needs_object_new = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NewObject { .. })
        })
    });
    let needs_property_fetch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::FetchProperty { .. }
                    | RegionInstructionKind::FetchDynamicStaticProperty { .. }
                    | RegionInstructionKind::FetchObjectClassName { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
            )
        })
    });
    let needs_property_assign = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::AssignProperty { .. }
                    | RegionInstructionKind::BindReferenceProperty { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
            )
        })
    });
    let needs_object_clone = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::CloneObject { .. })
        })
    });
    let needs_object_clone_with = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::CloneWith { .. })
        })
    });
    let needs_array_insert = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::ArrayInsert { .. }
                    | RegionInstructionKind::AssignDim { .. }
                    | RegionInstructionKind::AppendDim { .. }
                    | RegionInstructionKind::UnsetDim { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
                    | RegionInstructionKind::BindReferenceIntoDim { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
            ) || matches!(kind, RegionInstructionKind::NativeCall(call) if call.variadic)
        })
    });
    let needs_array_fetch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::FetchDim { .. }
                    | RegionInstructionKind::AssignDim { .. }
                    | RegionInstructionKind::AppendDim { .. }
                    | RegionInstructionKind::IssetDim { .. }
                    | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::UnsetDim { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
                    | RegionInstructionKind::BindReferenceIntoDim { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
            )
        })
    });
    let needs_array_unset = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::UnsetDim { .. })
        })
    });
    let needs_array_spread = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::ArraySpread { .. })
        })
    });
    let needs_foreach_init = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::ForeachInit { .. }
                    | RegionInstructionKind::ForeachInitRef { .. }
            )
        })
    });
    let needs_foreach_next = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::ForeachNext { .. }
                    | RegionInstructionKind::ForeachNextRef { .. }
            )
        })
    });
    let needs_foreach_cleanup = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::ForeachCleanup { .. })
        })
    });
    let needs_constant_fetch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::FetchConst { .. })
        })
    });
    let needs_truthy = regions.values().any(|region| {
        region.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                RegionTerminator::JumpIfFalse { .. }
                    | RegionTerminator::JumpIfTrue { .. }
                    | RegionTerminator::JumpIf { .. }
            )
        })
    });
    let needs_runtime_fatal = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::RuntimeFatal { .. })
        })
    });
    let needs_execution_poll = regions
        .values()
        .any(|region| !region.osr_entries().is_empty());
    let mut imports = vec![(
        "region-runtime-helper-abi".to_owned(),
        region.compile_metadata.helper_abi_hash as usize,
    )];
    if needs_call_trampoline {
        imports.push((
            native_call_symbol.clone(),
            runtime_helpers.native_call_dispatch,
        ));
    }
    if needs_dynamic_code {
        imports.push((
            native_dynamic_code_symbol.clone(),
            runtime_helpers.native_dynamic_code,
        ));
    }
    for (needed, configured, fallback, symbol) in [
        (
            needs_unary,
            runtime_helpers.native_unary,
            test_native_unary_fallback as *const () as usize,
            "phrust_native_unary",
        ),
        (
            needs_binary,
            runtime_helpers.native_binary,
            test_native_binary_fallback as *const () as usize,
            "phrust_native_binary",
        ),
        (
            needs_compare,
            runtime_helpers.native_compare,
            test_native_compare_fallback as *const () as usize,
            "phrust_native_compare",
        ),
        (
            needs_cast,
            runtime_helpers.native_cast,
            test_native_cast_fallback as *const () as usize,
            "phrust_native_cast",
        ),
        (
            needs_echo,
            runtime_helpers.native_echo,
            test_native_echo_fallback as *const () as usize,
            "phrust_native_echo",
        ),
        (
            needs_local_fetch,
            runtime_helpers.native_local_fetch,
            test_native_local_fetch_fallback as *const () as usize,
            "phrust_native_local_fetch",
        ),
        (
            needs_local_store,
            runtime_helpers.native_local_store,
            test_native_local_store_fallback as *const () as usize,
            "phrust_native_local_store",
        ),
        (
            needs_value_lifecycle,
            runtime_helpers.native_value_lifecycle,
            test_native_value_lifecycle_fallback as *const () as usize,
            "phrust_native_value_lifecycle",
        ),
        (
            needs_reference_bind,
            runtime_helpers.native_reference_bind,
            test_native_reference_bind_fallback as *const () as usize,
            "phrust_native_reference_bind",
        ),
        (
            needs_return_check,
            runtime_helpers.native_return_check,
            test_native_return_check_fallback as *const () as usize,
            "phrust_native_return_check",
        ),
        (
            needs_exception_new,
            runtime_helpers.native_exception_new,
            test_native_exception_new_fallback as *const () as usize,
            "phrust_native_exception_new",
        ),
        (
            needs_array_new,
            runtime_helpers.native_array_new,
            test_native_array_new_fallback as *const () as usize,
            "phrust_native_array_new",
        ),
        (
            needs_object_new,
            runtime_helpers.native_object_new,
            test_native_object_new_fallback as *const () as usize,
            "phrust_native_object_new",
        ),
        (
            needs_property_fetch,
            runtime_helpers.native_property_fetch,
            test_native_property_fetch_fallback as *const () as usize,
            "phrust_native_property_fetch",
        ),
        (
            needs_property_assign,
            runtime_helpers.native_property_assign,
            test_native_property_assign_fallback as *const () as usize,
            "phrust_native_property_assign",
        ),
        (
            needs_object_clone,
            runtime_helpers.native_object_clone,
            test_native_object_clone_fallback as *const () as usize,
            "phrust_native_object_clone",
        ),
        (
            needs_object_clone_with,
            runtime_helpers.native_object_clone_with,
            test_native_object_clone_with_fallback as *const () as usize,
            "phrust_native_object_clone_with",
        ),
        (
            needs_array_insert,
            runtime_helpers.native_array_insert,
            test_native_array_insert_fallback as *const () as usize,
            "phrust_native_array_insert",
        ),
        (
            needs_array_fetch,
            runtime_helpers.native_array_fetch,
            test_native_array_fetch_fallback as *const () as usize,
            "phrust_native_array_fetch",
        ),
        (
            needs_array_unset,
            runtime_helpers.native_array_unset,
            test_native_array_unset_fallback as *const () as usize,
            "phrust_native_array_unset",
        ),
        (
            needs_array_spread,
            runtime_helpers.native_array_spread,
            test_native_array_spread_fallback as *const () as usize,
            "phrust_native_array_spread",
        ),
        (
            needs_foreach_init,
            runtime_helpers.native_foreach_init,
            test_native_foreach_init_fallback as *const () as usize,
            "phrust_native_foreach_init",
        ),
        (
            needs_foreach_next,
            runtime_helpers.native_foreach_next,
            test_native_foreach_next_fallback as *const () as usize,
            "phrust_native_foreach_next",
        ),
        (
            needs_foreach_cleanup,
            runtime_helpers.native_foreach_cleanup,
            test_native_foreach_cleanup_fallback as *const () as usize,
            "phrust_native_foreach_cleanup",
        ),
        (
            needs_constant_fetch,
            runtime_helpers.native_constant_fetch,
            test_native_constant_fetch_fallback as *const () as usize,
            "phrust_native_constant_fetch",
        ),
        (
            needs_truthy,
            runtime_helpers.native_truthy,
            test_native_truthy_fallback as *const () as usize,
            "phrust_native_truthy",
        ),
        (
            needs_runtime_fatal,
            runtime_helpers.native_runtime_fatal,
            test_native_runtime_fatal_fallback as *const () as usize,
            "phrust_native_runtime_fatal",
        ),
        (
            needs_execution_poll,
            runtime_helpers.native_execution_poll,
            test_native_execution_poll_fallback as *const () as usize,
            "phrust_native_execution_poll",
        ),
    ] {
        if needed {
            let address = if configured == 0 {
                fallback
            } else {
                configured
            };
            imports.push((symbol.to_owned(), address));
        }
    }
    #[cfg(test)]
    {
        let aliases = imports
            .iter()
            .skip(1)
            .map(|(symbol, address)| (native_helper_import_symbol(symbol, *address), *address))
            .collect::<Vec<_>>();
        imports.extend(aliases);
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
            let helper_address = |symbol: &str| {
                imports
                    .iter()
                    .find_map(|(name, address)| (name == symbol).then_some(*address))
                    .expect("required native helper address must be imported")
            };
            let native_call_helper = if needs_call_trampoline {
                let pointer_type = module.target_config().pointer_type();
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                Some(declare_native_helper(
                    module,
                    &native_call_symbol,
                    &signature,
                    helper_address(&native_call_symbol),
                )?)
            } else {
                None
            };
            let native_dynamic_code_helper = if needs_dynamic_code {
                let pointer_type = module.target_config().pointer_type();
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                Some(declare_native_helper(
                    module,
                    &native_dynamic_code_symbol,
                    &signature,
                    helper_address(&native_dynamic_code_symbol),
                )?)
            } else {
                None
            };
            let mut native_operations = NativeOperationFunctions::default();
            let pointer_type = module.target_config().pointer_type();
            if needs_unary {
                native_operations.unary = Some(declare_value_operation(
                    module,
                    "phrust_native_unary",
                    1,
                    helper_address("phrust_native_unary"),
                )?);
            }
            if needs_binary {
                native_operations.binary = Some(declare_value_operation(
                    module,
                    "phrust_native_binary",
                    4,
                    helper_address("phrust_native_binary"),
                )?);
            }
            if needs_compare {
                native_operations.compare = Some(declare_value_operation(
                    module,
                    "phrust_native_compare",
                    2,
                    helper_address("phrust_native_compare"),
                )?);
            }
            if needs_cast {
                native_operations.cast = Some(declare_value_operation(
                    module,
                    "phrust_native_cast",
                    1,
                    helper_address("phrust_native_cast"),
                )?);
            }
            if needs_echo {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.echo = Some(declare_native_helper(
                    module,
                    "phrust_native_echo",
                    &signature,
                    helper_address("phrust_native_echo"),
                )?);
            }
            if needs_local_fetch {
                native_operations.local_fetch = Some(declare_value_operation(
                    module,
                    "phrust_native_local_fetch",
                    5,
                    helper_address("phrust_native_local_fetch"),
                )?);
            }
            if needs_local_store {
                native_operations.local_store = Some(declare_value_operation(
                    module,
                    "phrust_native_local_store",
                    4,
                    helper_address("phrust_native_local_store"),
                )?);
            }
            if needs_value_lifecycle {
                native_operations.value_lifecycle = Some(declare_value_operation(
                    module,
                    "phrust_native_value_lifecycle",
                    1,
                    helper_address("phrust_native_value_lifecycle"),
                )?);
            }
            if needs_reference_bind {
                native_operations.reference_bind = Some(declare_value_operation(
                    module,
                    "phrust_native_reference_bind",
                    3,
                    helper_address("phrust_native_reference_bind"),
                )?);
            }
            if needs_return_check {
                native_operations.return_check = Some(declare_value_operation(
                    module,
                    "phrust_native_return_check",
                    2,
                    helper_address("phrust_native_return_check"),
                )?);
            }
            if needs_exception_new {
                native_operations.exception_new = Some(declare_value_operation(
                    module,
                    "phrust_native_exception_new",
                    3,
                    helper_address("phrust_native_exception_new"),
                )?);
            }
            if needs_array_new {
                native_operations.array_new = Some(declare_value_operation(
                    module,
                    "phrust_native_array_new",
                    0,
                    helper_address("phrust_native_array_new"),
                )?);
            }
            if needs_object_new {
                native_operations.object_new = Some(declare_value_operation(
                    module,
                    "phrust_native_object_new",
                    0,
                    helper_address("phrust_native_object_new"),
                )?);
            }
            if needs_property_fetch {
                native_operations.property_fetch = Some(declare_value_operation(
                    module,
                    "phrust_native_property_fetch",
                    3,
                    helper_address("phrust_native_property_fetch"),
                )?);
            }
            if needs_property_assign {
                native_operations.property_assign = Some(declare_value_operation(
                    module,
                    "phrust_native_property_assign",
                    4,
                    helper_address("phrust_native_property_assign"),
                )?);
            }
            if needs_object_clone {
                native_operations.object_clone = Some(declare_value_operation(
                    module,
                    "phrust_native_object_clone",
                    1,
                    helper_address("phrust_native_object_clone"),
                )?);
            }
            if needs_object_clone_with {
                native_operations.object_clone_with = Some(declare_value_operation(
                    module,
                    "phrust_native_object_clone_with",
                    2,
                    helper_address("phrust_native_object_clone_with"),
                )?);
            }
            if needs_array_insert {
                native_operations.array_insert = Some(declare_value_operation(
                    module,
                    "phrust_native_array_insert",
                    3,
                    helper_address("phrust_native_array_insert"),
                )?);
            }
            if needs_array_fetch {
                native_operations.array_fetch = Some(declare_value_operation(
                    module,
                    "phrust_native_array_fetch",
                    2,
                    helper_address("phrust_native_array_fetch"),
                )?);
            }
            if needs_array_unset {
                native_operations.array_unset = Some(declare_value_operation(
                    module,
                    "phrust_native_array_unset",
                    2,
                    helper_address("phrust_native_array_unset"),
                )?);
            }
            if needs_array_spread {
                native_operations.array_spread = Some(declare_value_operation(
                    module,
                    "phrust_native_array_spread",
                    2,
                    helper_address("phrust_native_array_spread"),
                )?);
            }
            if needs_foreach_init {
                native_operations.foreach_init = Some(declare_value_operation(
                    module,
                    "phrust_native_foreach_init",
                    3,
                    helper_address("phrust_native_foreach_init"),
                )?);
            }
            if needs_foreach_next {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.foreach_next = Some(declare_native_helper(
                    module,
                    "phrust_native_foreach_next",
                    &signature,
                    helper_address("phrust_native_foreach_next"),
                )?);
            }
            if needs_foreach_cleanup {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.foreach_cleanup = Some(declare_native_helper(
                    module,
                    "phrust_native_foreach_cleanup",
                    &signature,
                    helper_address("phrust_native_foreach_cleanup"),
                )?);
            }
            if needs_constant_fetch {
                native_operations.constant_fetch = Some(declare_value_operation(
                    module,
                    "phrust_native_constant_fetch",
                    2,
                    helper_address("phrust_native_constant_fetch"),
                )?);
            }
            if needs_truthy {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.truthy = Some(declare_native_helper(
                    module,
                    "phrust_native_truthy",
                    &signature,
                    helper_address("phrust_native_truthy"),
                )?);
            }
            if needs_runtime_fatal {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.runtime_fatal = Some(declare_native_helper(
                    module,
                    "phrust_native_runtime_fatal",
                    &signature,
                    helper_address("phrust_native_runtime_fatal"),
                )?);
            }
            if needs_execution_poll {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.execution_poll = Some(declare_native_helper(
                    module,
                    "phrust_native_execution_poll",
                    &signature,
                    helper_address("phrust_native_execution_poll"),
                )?);
            }
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
            let mut relocatable_bytes = Vec::new();
            let mut relocatable_functions = Vec::new();
            let mut relocatable_relocations = Vec::new();
            // Keep parameter metadata for every function in the source unit,
            // including callees deliberately omitted from a bounded local
            // call graph. The typed trampoline still needs the declared
            // by-reference contract for those functions; otherwise ordinary
            // lvalue arguments (such as `$this->property`) are conservatively
            // rebound as references before dispatch.
            let mut function_params = unit
                .functions
                .iter()
                .enumerate()
                .filter_map(|(index, function)| {
                    let function_id = u32::try_from(index).ok().map(FunctionId::new)?;
                    Some((
                        function_id,
                        (
                            function.name.clone(),
                            function.params.clone(),
                            true,
                            function.params.len(),
                        ),
                    ))
                })
                .collect::<BTreeMap<_, _>>();
            for (index, signature) in request.external_function_signatures.iter().enumerate() {
                let Ok(index) = u32::try_from(index) else {
                    break;
                };
                let function_id = FunctionId::new(u32::MAX.saturating_sub(index));
                if function_params.contains_key(&function_id) {
                    continue;
                }
                let params = signature
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| php_ir::IrParam {
                        name: parameter.name.clone(),
                        local: LocalId::new(u32::try_from(index).unwrap_or(u32::MAX)),
                        required: false,
                        default: None,
                        type_: None,
                        by_ref: parameter.by_ref,
                        variadic: parameter.variadic,
                        attributes: Vec::new(),
                    })
                    .collect::<Vec<_>>();
                function_params.insert(
                    function_id,
                    (signature.name.clone(), params, true, signature.params.len()),
                );
            }
            function_params.extend(regions.iter().map(|(function, region)| {
                (
                    *function,
                    (
                        unit.functions[function.index()].name.clone(),
                        region.params.clone(),
                        trampoline_functions.contains(function),
                        region.arity(),
                    ),
                )
            }));
            for candidate in regions.values() {
                let func_id = functions[&candidate.function];
                let mut defined = define_region_graph_function(
                    module,
                    candidate,
                    func_id,
                    &functions,
                    &function_params,
                    native_call_helper,
                    native_dynamic_code_helper,
                    native_operations,
                )?;
                let alignment = usize::try_from(defined.alignment).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CACHE_ALIGNMENT",
                        "native function alignment does not fit usize",
                    )
                })?;
                let padding = if alignment == 0 {
                    0
                } else {
                    (alignment - relocatable_bytes.len() % alignment) % alignment
                };
                relocatable_bytes.resize(relocatable_bytes.len().saturating_add(padding), 0);
                let code_offset = relocatable_bytes.len() as u64;
                let candidate_bytes = defined.code.len() as u64;
                relocatable_bytes.extend_from_slice(&defined.code);
                for relocation in &mut defined.relocations {
                    relocation.offset = relocation.offset.saturating_add(code_offset);
                }
                relocatable_relocations.append(&mut defined.relocations);
                relocatable_functions.push(crate::JitRelocatableFunction {
                    function: candidate.function,
                    code_offset,
                    code_len: candidate_bytes,
                    arity: region_arity(candidate)?,
                    local_count: candidate.local_count,
                });
                code_bytes = code_bytes.saturating_add(candidate_bytes);
                native_pc_ranges.append(&mut defined.native_pc_ranges);
            }
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                module
                    .finalize_definitions()
                    .map_err(|error| error.to_string())
            }))
            .map_err(|payload| {
                let message = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| {
                        payload
                            .downcast_ref::<&str>()
                            .map(|value| (*value).to_owned())
                    })
                    .unwrap_or_else(|| "Cranelift finalization panicked".to_owned());
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize executable region call graph: {message}"),
                )
            })?
            .map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FINALIZE",
                    format!("failed to finalize executable region call graph: {error}"),
                )
            })?;
            let function_entries = regions
                .values()
                .map(|candidate| {
                    Ok(crate::JitNativeFunctionEntryMetadata {
                        function: candidate.function,
                        address: module.get_finalized_function(functions[&candidate.function])
                            as usize,
                        arity: region_arity(candidate)?,
                        local_count: candidate.local_count,
                    })
                })
                .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
            let root = functions[&function];
            let address = module.get_finalized_function(root) as usize;
            let region_state_metadata = region_graph_metadata(
                region.local_count,
                regions.values(),
                native_pc_ranges,
                function_entries,
            );
            let mut handle = JitFunctionHandle::i64_status_out_native(
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
            handle.bind_relocatable_code(crate::JitRelocatableCode {
                root: function,
                code: relocatable_bytes,
                functions: relocatable_functions,
                relocations: relocatable_relocations,
            });
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
    let max_functions = max_local_call_graph_functions(unit.functions.len());
    let mut regions = BTreeMap::new();
    regions.insert(root.function, root.clone());
    let mut pending = root.direct_callees();
    pending.extend(dynamic_class_body_functions(unit, root));
    while let Some(function) = pending.pop() {
        if regions.contains_key(&function) {
            continue;
        }
        if regions.len() >= max_functions {
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
        if region.register_count > MAX_LOCAL_CALLEE_REGISTERS {
            continue;
        }
        pending.extend(region.direct_callees());
        pending.extend(dynamic_class_body_functions(unit, &region));
        regions.insert(function, region);
    }
    for region in regions.values() {
        region.verify().map_err(|error| {
            CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_REGION_VERIFY", error.to_string())
        })?;
    }
    Ok(regions)
}

fn dynamic_class_body_functions(unit: &IrUnit, region: &RegionGraph) -> Vec<FunctionId> {
    let declared = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| {
            let RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::DeclareClass {
                name,
            }) = &instruction.kind
            else {
                return None;
            };
            Some(name)
        });
    let mut functions = std::collections::BTreeSet::new();
    for name in declared {
        if let Some(class) = unit
            .classes
            .iter()
            .find(|class| class.name.eq_ignore_ascii_case(name))
        {
            functions.extend(class.methods.iter().map(|method| method.function));
        }
    }
    functions.into_iter().collect()
}

fn validate_region_native_coverage(region: &RegionGraph) -> Result<(), CraneliftLoweringError> {
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

fn region_instruction_result_register(kind: &RegionInstructionKind) -> Option<RegId> {
    match kind {
        RegionInstructionKind::Move { dst, .. }
        | RegionInstructionKind::LoadLocal { dst, .. }
        | RegionInstructionKind::AssignLocalResult { dst, .. }
        | RegionInstructionKind::Binary { dst, .. }
        | RegionInstructionKind::Unary { dst, .. }
        | RegionInstructionKind::Compare { dst, .. }
        | RegionInstructionKind::Cast { dst, .. }
        | RegionInstructionKind::NewArray { dst }
        | RegionInstructionKind::NewObject { dst, .. }
        | RegionInstructionKind::FetchProperty { dst, .. }
        | RegionInstructionKind::FetchDynamicStaticProperty { dst, .. }
        | RegionInstructionKind::FetchObjectClassName { dst, .. }
        | RegionInstructionKind::AssignProperty { dst, .. }
        | RegionInstructionKind::CloneObject { dst, .. }
        | RegionInstructionKind::CloneWith { dst, .. }
        | RegionInstructionKind::FetchDim { dst, .. }
        | RegionInstructionKind::FetchConst { dst }
        | RegionInstructionKind::AssignDim { dst, .. }
        | RegionInstructionKind::AppendDim { dst, .. }
        | RegionInstructionKind::IssetDim { dst, .. }
        | RegionInstructionKind::EmptyDim { dst, .. }
        | RegionInstructionKind::IssetLocal { dst, .. }
        | RegionInstructionKind::EmptyLocal { dst, .. }
        | RegionInstructionKind::ForeachInit { iterator: dst, .. }
        | RegionInstructionKind::ForeachInitRef { iterator: dst, .. }
        | RegionInstructionKind::ForeachNext { has_value: dst, .. }
        | RegionInstructionKind::ForeachNextRef { has_value: dst, .. } => Some(*dst),
        RegionInstructionKind::RuntimeFatal { dst: Some(dst), .. } => Some(*dst),
        RegionInstructionKind::NativeCall(RegionNativeCall {
            result: RegionCallResult::Register(dst),
            ..
        }) => Some(*dst),
        RegionInstructionKind::NativeControl(RegionNativeControl::MakeException {
            dst, ..
        }) => Some(*dst),
        RegionInstructionKind::NativeSuspend(
            RegionNativeSuspend::GeneratorYield { dst, .. }
            | RegionNativeSuspend::GeneratorDelegate { dst, .. }
            | RegionNativeSuspend::FiberSuspend { dst, .. },
        ) => Some(*dst),
        RegionInstructionKind::NativeDynamicCode(
            RegionNativeDynamicCode::Include { dst, .. }
            | RegionNativeDynamicCode::Eval { dst, .. }
            | RegionNativeDynamicCode::MakeClosure { dst, .. },
        ) => Some(*dst),
        RegionInstructionKind::Nop
        | RegionInstructionKind::StoreLocal { .. }
        | RegionInstructionKind::BindReference { .. }
        | RegionInstructionKind::BindReferenceDim { .. }
        | RegionInstructionKind::BindReferenceIntoDim { .. }
        | RegionInstructionKind::BindReferenceProperty { .. }
        | RegionInstructionKind::BindReferenceFromProperty { .. }
        | RegionInstructionKind::BindReferenceFromPropertyDim { .. }
        | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
        | RegionInstructionKind::BindReferenceDimFromProperty { .. }
        | RegionInstructionKind::BindReferenceStaticProperty { .. }
        | RegionInstructionKind::InitStaticLocal { .. }
        | RegionInstructionKind::Discard { .. }
        | RegionInstructionKind::Echo { .. }
        | RegionInstructionKind::ArrayInsert { .. }
        | RegionInstructionKind::ArraySpread { .. }
        | RegionInstructionKind::UnsetDim { .. }
        | RegionInstructionKind::UnsetLocal { .. }
        | RegionInstructionKind::ForeachCleanup { .. }
        | RegionInstructionKind::NativeCall(RegionNativeCall {
            result: RegionCallResult::ReferenceLocal(_) | RegionCallResult::Discard,
            ..
        })
        | RegionInstructionKind::NativeControl(_)
        | RegionInstructionKind::NativeDynamicCode(_)
        | RegionInstructionKind::RuntimeFatal { dst: None, .. }
        | RegionInstructionKind::CompileTimeFatal { .. }
        | RegionInstructionKind::MissingLowering => None,
    }
}

fn region_register_types(region: &RegionGraph) -> BTreeMap<RegId, ir::Type> {
    region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .flat_map(|instruction| {
            let mut registers = region_instruction_result_register(&instruction.kind)
                .into_iter()
                .collect::<Vec<_>>();
            if let RegionInstructionKind::ForeachNext { key, value, .. } = &instruction.kind {
                registers.extend(*key);
                registers.push(*value);
            }
            registers.into_iter().map(|register| (register, types::I64))
        })
        .collect()
}

fn region_graph_signature(
    module: &JITModule,
    region: &RegionGraph,
) -> Result<Signature, CraneliftLoweringError> {
    region_arity(region)?;
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    // Region arguments use a packed pointer so PHP functions are not limited
    // by a host-language list of monomorphic FFI signatures.
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(types::I32));
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    Ok(signature)
}

struct DefinedRegionFunction {
    code: Vec<u8>,
    alignment: u64,
    relocations: Vec<crate::JitRelocatableRelocation>,
    native_pc_ranges: Vec<crate::JitNativePcRange>,
}

fn supported_relocation_kind(kind: Reloc) -> Option<crate::JitRelocatableKind> {
    match kind {
        Reloc::Abs8 => Some(crate::JitRelocatableKind::Abs64),
        Reloc::X86PCRel4 => Some(crate::JitRelocatableKind::X86PcRel4),
        Reloc::X86CallPCRel4 | Reloc::X86CallPLTRel4 => {
            Some(crate::JitRelocatableKind::X86CallPcRel4)
        }
        Reloc::Arm64Call => Some(crate::JitRelocatableKind::Arm64Call),
        _ => None,
    }
}

fn stable_helper_import_name(name: &str) -> String {
    #[cfg(test)]
    {
        if let Some((base, suffix)) = name.rsplit_once('_')
            && suffix.len() == 16
            && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return base.to_owned();
        }
    }
    name.to_owned()
}

fn capture_relocation(
    module: &JITModule,
    relocation: ModuleReloc,
    functions: &BTreeMap<FunctionId, FuncId>,
) -> Result<crate::JitRelocatableRelocation, CraneliftLoweringError> {
    let kind = supported_relocation_kind(relocation.kind).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_RELOCATION",
            format!(
                "Cranelift emitted unsupported restart-cache relocation {:?}",
                relocation.kind
            ),
        )
    })?;
    let internal_function = |func_id: FuncId| {
        functions
            .iter()
            .find_map(|(function, candidate)| (*candidate == func_id).then_some(*function))
    };
    let (target, extra_addend) = match relocation.name {
        ModuleRelocTarget::User {
            namespace: 0,
            index,
        } => {
            let func_id = FuncId::from_u32(index);
            if let Some(function) = internal_function(func_id) {
                (crate::JitRelocatableTarget::InternalFunction(function), 0)
            } else {
                let declaration = module.declarations().get_function_decl(func_id);
                if declaration.linkage != Linkage::Import {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CACHE_SYMBOL",
                        format!("relocation target {func_id} is neither graph-local nor imported"),
                    ));
                }
                let name = declaration.name.as_deref().ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CACHE_SYMBOL",
                        format!("imported relocation target {func_id} has no stable name"),
                    )
                })?;
                (
                    crate::JitRelocatableTarget::Helper(stable_helper_import_name(name)),
                    0,
                )
            }
        }
        ModuleRelocTarget::FunctionOffset(func_id, offset) => {
            let function = internal_function(func_id).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CACHE_SYMBOL",
                    format!("function-offset relocation target {func_id} is not graph-local"),
                )
            })?;
            (
                crate::JitRelocatableTarget::InternalFunction(function),
                i64::from(offset),
            )
        }
        other => {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_CACHE_SYMBOL",
                format!("unsupported restart-cache relocation target {other}"),
            ));
        }
    };
    Ok(crate::JitRelocatableRelocation {
        offset: u64::from(relocation.offset),
        kind,
        target,
        addend: relocation.addend.saturating_add(extra_addend),
    })
}

#[allow(clippy::too_many_arguments)]
fn define_region_graph_function(
    module: &mut JITModule,
    region: &RegionGraph,
    func_id: FuncId,
    functions: &BTreeMap<FunctionId, FuncId>,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    native_call_helper: Option<NativeHelper>,
    native_dynamic_code_helper: Option<NativeHelper>,
    native_operations: NativeOperationFunctions,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let mut ctx = module.make_context();
    ctx.func.signature = region_graph_signature(module, region)?;
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);
        let blocks = create_region_cranelift_blocks(&mut builder, region)?;
        let instruction_blocks = region
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .map(|instruction| (instruction.continuation_id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let terminator_blocks = region
            .blocks
            .iter()
            .map(|block| (block.id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let suspension_blocks = region
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_))
            })
            .map(|instruction| (instruction.continuation_id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
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
        let arguments = params[0];
        let result_out = params[1];
        let deopt_out = params[2];
        let resume_id = params[3];
        let resume_state = params[4];
        let mut locals = BTreeMap::new();
        for local_index in 0..region.local_count {
            locals.insert(LocalId::new(local_index), builder.declare_var(types::I64));
        }
        let register_types = region_register_types(region);
        let register_variables = (0..region.register_count)
            .map(|index| {
                let register = RegId::new(index);
                let type_ = register_types.get(&register).copied().unwrap_or(types::I64);
                (register, builder.declare_var(type_))
            })
            .collect::<BTreeMap<_, _>>();
        let pending_status = builder.declare_var(types::I32);
        let pending_value = builder.declare_var(types::I64);
        let continue_status = builder
            .ins()
            .iconst(types::I32, i64::from(crate::JitCallStatus::CONTINUE.0));
        let empty_value = builder.ins().iconst(types::I64, 0);
        let native_version =
            u32::from(region.compile_metadata.tier == NativeCompilerTier::Optimizing);
        builder.def_var(pending_status, continue_status);
        builder.def_var(pending_value, empty_value);
        let uninitialized_value = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        for variable in locals.values().copied() {
            builder.def_var(variable, uninitialized_value);
        }
        for (index, param) in region.parameter_locals.iter().enumerate() {
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                arguments,
                i32::try_from(index.saturating_mul(8)).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_REGION_ARITY",
                        "packed region argument offset does not fit the native ABI",
                    )
                })?,
            );
            builder.def_var(local_variable(&locals, *param)?, value);
        }
        let handler_resume_blocks = region
            .exception_regions
            .iter()
            .flat_map(|handler| [handler.catch, handler.finally])
            .flatten()
            .collect::<std::collections::BTreeSet<_>>();
        let handler_exception_locals = region
            .exception_regions
            .iter()
            .filter_map(|handler| Some((handler.catch?, handler.exception_local?)))
            .fold(
                BTreeMap::<BlockId, std::collections::BTreeSet<LocalId>>::new(),
                |mut locals, (block, local)| {
                    locals.entry(block).or_default().insert(local);
                    locals
                },
            );
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
            let resume_locals = target_block
                .entry_live_locals
                .iter()
                .copied()
                .chain(
                    handler_exception_locals
                        .get(&target)
                        .into_iter()
                        .flatten()
                        .copied(),
                )
                .collect::<std::collections::BTreeSet<_>>();
            for local in resume_locals {
                let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                    .saturating_add(local.index().saturating_mul(8));
                let local_value = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    resume_state,
                    offset as i32,
                );
                builder.def_var(local_variable(&locals, local)?, local_value);
            }
            builder.ins().jump(cranelift_block(&blocks, target)?, &[]);
            builder.switch_to_block(next);
        }
        for region_block in &region.blocks {
            for instruction in &region_block.instructions {
                if !matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_)) {
                    continue;
                }
                let loader = builder.create_block();
                let next = builder.create_block();
                let encoded_resume =
                    crate::native_suspension_resume_id(instruction.continuation_id);
                let requested =
                    builder
                        .ins()
                        .icmp_imm(IntCC::Equal, resume_id, i64::from(encoded_resume));
                builder.ins().brif(requested, loader, &[], next, &[]);
                builder.switch_to_block(loader);
                let control_status = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    resume_state,
                    std::mem::offset_of!(crate::JitDeoptState, control_status) as i32,
                );
                let control_value = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    resume_state,
                    std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
                );
                builder.def_var(pending_status, control_status);
                builder.def_var(pending_value, control_value);
                for local in &instruction.live_locals {
                    let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                        .saturating_add(local.index().saturating_mul(8));
                    let value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        resume_state,
                        offset as i32,
                    );
                    builder.def_var(local_variable(&locals, *local)?, value);
                }
                builder.ins().jump(
                    *suspension_blocks
                        .get(&instruction.continuation_id)
                        .expect("suspension block was predeclared"),
                    &[],
                );
                builder.switch_to_block(next);
            }
        }
        for region_block in &region.blocks {
            let mut live_registers = Vec::new();
            for instruction in &region_block.instructions {
                if live_registers
                    .iter()
                    .all(|register: &RegId| register.index() < crate::JIT_DEOPT_MAX_REGISTERS)
                {
                    let loader = builder.create_block();
                    let next = builder.create_block();
                    let encoded_resume =
                        crate::native_transition_resume_id(instruction.continuation_id);
                    let requested =
                        builder
                            .ins()
                            .icmp_imm(IntCC::Equal, resume_id, i64::from(encoded_resume));
                    builder.ins().brif(requested, loader, &[], next, &[]);
                    builder.switch_to_block(loader);
                    let control_status = builder.ins().load(
                        types::I32,
                        MemFlagsData::new(),
                        resume_state,
                        std::mem::offset_of!(crate::JitDeoptState, control_status) as i32,
                    );
                    let control_value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        resume_state,
                        std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
                    );
                    builder.def_var(pending_status, control_status);
                    builder.def_var(pending_value, control_value);
                    for local in &instruction.live_locals {
                        let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                            .saturating_add(local.index().saturating_mul(8));
                        let value = builder.ins().load(
                            types::I64,
                            MemFlagsData::new(),
                            resume_state,
                            offset as i32,
                        );
                        builder.def_var(local_variable(&locals, *local)?, value);
                    }
                    for register in &live_registers {
                        let variable = register_variables[register];
                        let type_ = register_types.get(register).copied().unwrap_or(types::I64);
                        let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
                            .saturating_add(register.index().saturating_mul(8));
                        let value = builder.ins().load(
                            types::I64,
                            MemFlagsData::new(),
                            resume_state,
                            offset as i32,
                        );
                        let value = if type_ == types::I64 {
                            value
                        } else {
                            builder.ins().ireduce(type_, value)
                        };
                        builder.def_var(variable, value);
                    }
                    builder
                        .ins()
                        .jump(instruction_blocks[&instruction.continuation_id], &[]);
                    builder.switch_to_block(next);
                }
                if let Some(register) = region_instruction_result_register(&instruction.kind) {
                    live_registers.push(register);
                }
            }
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
                let offset = std::mem::offset_of!(crate::JitDeoptState, slots)
                    .saturating_add(local.index().saturating_mul(8));
                let value = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    resume_state,
                    offset as i32,
                );
                builder.def_var(local_variable(&locals, *local)?, value);
            }
            builder
                .ins()
                .jump(cranelift_block(&blocks, osr_entry.block)?, &[]);
            builder.switch_to_block(next);
        }
        builder.ins().jump(normal_entry, &[]);

        let loop_headers = region
            .osr_entries()
            .into_iter()
            .map(|entry| entry.block)
            .collect::<BTreeSet<_>>();

        for region_block in &region.blocks {
            let mut registers = register_variables.clone();
            builder.switch_to_block(cranelift_block(&blocks, region_block.id)?);
            if loop_headers.contains(&region_block.id)
                && let Some(helper) = native_operations.execution_poll
            {
                let context = builder.ins().iconst(types::I64, 0);
                let call = call_native_helper(module, &mut builder, helper, &[context]);
                let status = builder.inst_results(call)[0];
                require_native_operation_ok(&mut builder, status, result_out)?;
            }
            let first = region_block
                .instructions
                .first()
                .map(|instruction| instruction_blocks[&instruction.continuation_id])
                .unwrap_or(terminator_blocks[&region_block.id]);
            builder.ins().jump(first, &[]);
            for (index, instruction) in region_block.instructions.iter().enumerate() {
                builder.switch_to_block(instruction_blocks[&instruction.continuation_id]);
                builder.set_srcloc(ir::SourceLoc::new(
                    instruction.continuation_id.saturating_add(1),
                ));
                lower_region_instruction(
                    module,
                    &mut builder,
                    functions,
                    function_params,
                    native_call_helper,
                    native_dynamic_code_helper,
                    native_operations,
                    &register_variables,
                    &blocks,
                    &suspension_blocks,
                    &locals,
                    &mut registers,
                    region_block.id,
                    instruction,
                    result_out,
                    deopt_out,
                    resume_state,
                    pending_status,
                    pending_value,
                    region.function,
                    region.local_count,
                    native_version,
                    pointer_type,
                )?;
                let next = region_block
                    .instructions
                    .get(index + 1)
                    .map(|next| instruction_blocks[&next.continuation_id])
                    .unwrap_or(terminator_blocks[&region_block.id]);
                builder.ins().jump(next, &[]);
            }
            builder.switch_to_block(terminator_blocks[&region_block.id]);
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
                module,
                native_operations,
                region.function,
                region.return_type.is_some(),
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
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            "Cranelift returned no compiled machine-code buffer",
        )
    })?;
    let code = compiled.code_buffer().to_vec();
    let alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    let relocations = compiled
        .buffer
        .relocs()
        .iter()
        .map(|relocation| {
            capture_relocation(
                module,
                ModuleReloc::from_mach_reloc(relocation, &ctx.func, func_id),
                functions,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
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
    Ok(DefinedRegionFunction {
        code,
        alignment,
        relocations,
        native_pc_ranges,
    })
}

fn region_graph_metadata<'a>(
    root_local_count: u32,
    regions: impl Iterator<Item = &'a RegionGraph>,
    native_pc_ranges: Vec<crate::JitNativePcRange>,
    function_entries: Vec<crate::JitNativeFunctionEntryMetadata>,
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
        compiler_tier: regions
            .first()
            .map(|region| region.compile_metadata.tier)
            .unwrap_or_default(),
        native_version: u32::from(
            regions.first().is_some_and(|region| {
                region.compile_metadata.tier == NativeCompilerTier::Optimizing
            }),
        ),
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
                        protected_blocks: handler.protected_blocks.clone(),
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
                    block
                        .instructions
                        .iter()
                        .filter(move |instruction| {
                            crate::region_ir::baseline_instruction_lowering(
                                &instruction.source_kind,
                            )
                            .requires_safepoint
                        })
                        .map(move |instruction| crate::JitNativeSafepointMetadata {
                            function: region.function,
                            continuation_id: instruction.continuation_id,
                            baseline_frame_slots: instruction.live_locals.clone(),
                            optimized_roots_required: region.compile_metadata.tier
                                == NativeCompilerTier::Optimizing,
                        })
                })
            })
            .collect(),
        suspensions: regions
            .iter()
            .flat_map(|region| {
                region.blocks.iter().flat_map(move |block| {
                    block.instructions.iter().filter_map(move |instruction| {
                        let RegionInstructionKind::NativeSuspend(suspend) = &instruction.kind
                        else {
                            return None;
                        };
                        let kind = match suspend {
                            RegionNativeSuspend::GeneratorYield { .. } => {
                                crate::JitNativeSuspendKind::GENERATOR_YIELD
                            }
                            RegionNativeSuspend::GeneratorDelegate { .. } => {
                                crate::JitNativeSuspendKind::GENERATOR_DELEGATE
                            }
                            RegionNativeSuspend::FiberSuspend { .. } => {
                                crate::JitNativeSuspendKind::FIBER_SUSPEND
                            }
                        };
                        Some(crate::JitNativeSuspensionMetadata {
                            function: region.function,
                            continuation_id: instruction.continuation_id,
                            resume_id: crate::native_suspension_resume_id(
                                instruction.continuation_id,
                            ),
                            kind,
                            span: instruction.span,
                            live_locals: instruction.live_locals.clone(),
                            owning_generation_required: true,
                        })
                    })
                })
            })
            .collect(),
        dynamic_code: regions
            .iter()
            .flat_map(|region| {
                region.blocks.iter().flat_map(move |block| {
                    block.instructions.iter().filter_map(move |instruction| {
                        let RegionInstructionKind::NativeDynamicCode(operation) = &instruction.kind
                        else {
                            return None;
                        };
                        let (kind, declared_function) = match operation {
                            RegionNativeDynamicCode::Include { kind, .. } => (
                                match kind {
                                    php_ir::instruction::IncludeKind::Include => {
                                        crate::JitNativeDynamicCodeKind::INCLUDE
                                    }
                                    php_ir::instruction::IncludeKind::IncludeOnce => {
                                        crate::JitNativeDynamicCodeKind::INCLUDE_ONCE
                                    }
                                    php_ir::instruction::IncludeKind::Require => {
                                        crate::JitNativeDynamicCodeKind::REQUIRE
                                    }
                                    php_ir::instruction::IncludeKind::RequireOnce => {
                                        crate::JitNativeDynamicCodeKind::REQUIRE_ONCE
                                    }
                                },
                                None,
                            ),
                            RegionNativeDynamicCode::Eval { .. } => {
                                (crate::JitNativeDynamicCodeKind::EVAL, None)
                            }
                            RegionNativeDynamicCode::DeclareFunction { function, .. } => (
                                crate::JitNativeDynamicCodeKind::DECLARE_FUNCTION,
                                Some(*function),
                            ),
                            RegionNativeDynamicCode::DeclareClass { .. } => {
                                (crate::JitNativeDynamicCodeKind::DECLARE_CLASS, None)
                            }
                            RegionNativeDynamicCode::RegisterConstant { .. } => {
                                (crate::JitNativeDynamicCodeKind::REGISTER_CONSTANT, None)
                            }
                            RegionNativeDynamicCode::EmitDiagnostic => {
                                (crate::JitNativeDynamicCodeKind::EMIT_DIAGNOSTIC, None)
                            }
                            RegionNativeDynamicCode::MakeClosure { function, .. } => (
                                crate::JitNativeDynamicCodeKind::MAKE_CLOSURE,
                                Some(*function),
                            ),
                        };
                        Some(crate::JitNativeDynamicCodeMetadata {
                            function: region.function,
                            continuation_id: instruction.continuation_id,
                            kind,
                            declared_function,
                            span: instruction.span,
                            process_cache: true,
                            restart_cache: true,
                        })
                    })
                })
            })
            .collect(),
        native_transitions: regions
            .iter()
            .flat_map(|region| {
                region.blocks.iter().flat_map(move |block| {
                    let mut live_registers = Vec::new();
                    block.instructions.iter().filter_map(move |instruction| {
                        // `define_region_graph_function` emits a transition
                        // loader only while every live register fits the
                        // fixed native deopt-state mask. Publishing metadata
                        // for later instructions would advertise a nonexistent
                        // entry and quadratically clone an unbounded register
                        // prefix in declaration-heavy generated functions.
                        let publishable = live_registers.iter().all(|register: &RegId| {
                            register.index() < crate::JIT_DEOPT_MAX_REGISTERS
                        });
                        let result_register = region_instruction_result_register(&instruction.kind);
                        let transition = publishable.then(|| crate::JitNativeTransitionMetadata {
                            function: region.function,
                            native_version: u32::from(
                                region.compile_metadata.tier == NativeCompilerTier::Optimizing,
                            ),
                            continuation_id: instruction.continuation_id,
                            resume_id: crate::native_transition_resume_id(
                                instruction.continuation_id,
                            ),
                            span: instruction.span,
                            live_locals: instruction.live_locals.clone(),
                            live_registers: live_registers.clone(),
                            result_register,
                        });
                        if let Some(register) = result_register {
                            live_registers.push(register);
                        }
                        transition
                    })
                })
            })
            .collect(),
        function_entries,
    }
}

#[cfg(test)]
mod tests {
    use super::max_local_call_graph_functions;

    #[test]
    fn large_units_publish_one_root_per_native_module() {
        assert_eq!(max_local_call_graph_functions(16), 4);
        assert_eq!(max_local_call_graph_functions(17), 1);
        assert_eq!(max_local_call_graph_functions(128), 1);
    }
}
