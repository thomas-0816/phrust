fn entry_array_root_local(
    operand: RegionOperand,
    definitions: &BTreeMap<RegId, RegionOperand>,
) -> Option<LocalId> {
    let mut operand = operand;
    for _ in 0..32 {
        match operand {
            RegionOperand::Register(register) => operand = *definitions.get(&register)?,
            RegionOperand::Local(local) => return Some(local),
            RegionOperand::Constant(_)
            | RegionOperand::I64(_)
            | RegionOperand::LinkedConstant { .. } => return None,
        }
    }
    None
}

fn publication_entry_array_source(
    region: &RegionGraph,
    definitions: &BTreeMap<RegId, RegionOperand>,
    parameter_indices: &BTreeMap<LocalId, usize>,
    by_ref_parameters: &BTreeSet<LocalId>,
    operand: RegionOperand,
    continuation_id: u32,
    family: &str,
) -> Result<NativeEntryArraySource, CraneliftLoweringError> {
    let source = entry_array_source(operand, definitions, parameter_indices).ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_SHAPE",
            format!("{family} at continuation {continuation_id} is not rooted at an entry array",),
        )
    })?;
    let NativeEntryArraySource::Parameter(index) = source else {
        unreachable!("operand-rooted array families are parameters")
    };
    if by_ref_parameters.contains(&region.parameter_locals[index]) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_REFERENCE",
            format!(
                "{family} at continuation {continuation_id} receives a by-reference entry array",
            ),
        ));
    }
    Ok(source)
}

#[derive(Clone, Copy)]
struct PublicationArraySourceContext<'a> {
    region: &'a RegionGraph,
    definitions: &'a BTreeMap<RegId, RegionOperand>,
    parameter_indices: &'a BTreeMap<LocalId, usize>,
    by_ref_parameters: &'a BTreeSet<LocalId>,
    internal_sources: &'a BTreeMap<RegId, BTreeSet<NativeEntryArraySource>>,
}

fn publication_array_sources(
    context: PublicationArraySourceContext<'_>,
    operand: RegionOperand,
    continuation_id: u32,
    family: &str,
    allow_by_reference: bool,
) -> Result<BTreeSet<NativeEntryArraySource>, CraneliftLoweringError> {
    let root = publication_root_operand(operand, context.definitions);
    let sources = if let Some(source) =
        entry_array_source(root, context.definitions, context.parameter_indices)
    {
        BTreeSet::from([source])
    } else if let RegionOperand::Register(register) = root {
        context.internal_sources.get(&register).cloned().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_SHAPE",
                format!(
                    "{family} at continuation {continuation_id} has no publication-total array producer",
                ),
            )
        })?
    } else {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_SHAPE",
            format!("{family} at continuation {continuation_id} is not a published native array",),
        ));
    };
    for source in &sources {
        if !allow_by_reference
            && let NativeEntryArraySource::Parameter(index) = *source
            && context
                .by_ref_parameters
                .contains(&context.region.parameter_locals[index])
        {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_REFERENCE",
                format!(
                    "{family} at continuation {continuation_id} receives a by-reference entry array",
                ),
            ));
        }
    }
    Ok(sources)
}
