use super::*;
use crate::region_ir::{RegionPropertyName, RegionSemanticOp};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NativeEntryArraySource {
    Parameter(usize),
    TrustedGlobal(u32),
}

#[derive(Clone, Debug, Default)]
struct NativeEntryArrayRequirement {
    projection_allocations: usize,
    spread_allocations: usize,
    entry_allocations_per_entry: usize,
    value_allocations_per_entry: usize,
    require_supported_keys: bool,
    require_plain_values: bool,
    require_key_values: bool,
    require_string_values: bool,
    require_scalar_values: bool,
    minimum_length: usize,
    implode_separator_lengths: Vec<usize>,
    required_integer_keys: BTreeSet<i64>,
    required_value_types: Vec<(i64, php_ir::IrReturnType)>,
    probe_paths: Vec<NativeEntryArrayProbeRequirement>,
    unpack_calls: Vec<NativeEntryArrayUnpackRequirement>,
    mutations: Vec<NativeEntryArrayMutationRequirement>,
    all_value_types: Vec<php_ir::IrReturnType>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NativeEntryArrayKey {
    Integer(i64),
    Constant(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeEntryArrayProbeLeaf {
    ExistsOnly,
    PlainValue,
}

#[derive(Clone, Debug)]
struct NativeEntryArrayProbeRequirement {
    keys: Vec<NativeEntryArrayKey>,
    leaf: NativeEntryArrayProbeLeaf,
}

#[derive(Clone, Debug)]
struct NativeEntryArrayUnpackRequirement {
    required_length: usize,
    fixed_parameters: Vec<(usize, Option<php_ir::IrReturnType>, bool)>,
    tail_start: usize,
    tail_type: Option<php_ir::IrReturnType>,
    tail_by_reference: bool,
}

#[derive(Clone, Debug)]
struct NativeEntryPropertyRequirement {
    parameter_index: usize,
    continuation_id: u32,
    required_state: u32,
    readable: bool,
    releasable: bool,
    allow_reference: bool,
    require_reference: bool,
    probe_paths: Vec<NativeEntryArrayProbeRequirement>,
    mutations: Vec<NativeEntryArrayMutationRequirement>,
}

#[derive(Clone, Debug)]
struct NativeEntryStaticPropertyRequirement {
    continuation_id: u32,
    required_state: u32,
    readable: bool,
    releasable: bool,
    allow_reference: bool,
    require_reference: bool,
    probe_paths: Vec<NativeEntryArrayProbeRequirement>,
    mutations: Vec<NativeEntryArrayMutationRequirement>,
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryObjectLayoutRequirement {
    parameter_index: usize,
    layout_id: u64,
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryInstanceofRequirement {
    parameter_index: Option<usize>,
    continuation_id: u32,
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryCallableRequirement {
    parameter_index: usize,
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryCloneRequirement {
    parameter_index: usize,
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryDynamicPropertyRequirement {
    parameter_index: usize,
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryExceptionRequirement {
    continuation_id: u32,
    include_function_frame: bool,
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryValueClassRequirement {
    parameter_index: usize,
    class: SsaValueClass,
}

#[derive(Clone, Copy, Debug)]
enum NativeEntryStringOffset {
    Constant(i64),
    Parameter(usize),
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryStringRequirement {
    parameter_index: usize,
    minimum_length: usize,
    offset: Option<NativeEntryStringOffset>,
    allocation_multiplier: usize,
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryResourceTypeRequirement {
    parameter_index: usize,
}

#[derive(Clone, Debug)]
struct NativeEntryIntegerRequirement {
    parameter_index: usize,
    minimum: i64,
    maximum: i64,
    forbidden_values: Vec<i64>,
}

#[derive(Clone, Copy, Debug)]
struct NativeEntryFloatRequirement {
    parameter_index: usize,
    forbid_zero: bool,
}

#[derive(Clone, Debug)]
enum NativeEntryArrayMutationRequirement {
    Assign { parents: Vec<i64>, key: i64 },
    Append { parents: Vec<i64> },
    Unset { parents: Vec<i64>, key: i64 },
    Reference { parents: Vec<i64>, key: i64 },
    ReferenceAppend { parents: Vec<i64> },
}

impl NativeEntryArrayMutationRequirement {
    const fn additional_entries(&self) -> usize {
        match self {
            Self::Unset { .. } => 0,
            Self::Assign { .. }
            | Self::Append { .. }
            | Self::Reference { .. }
            | Self::ReferenceAppend { .. } => 1,
        }
    }
}

/// Complete compile/publication contract for one admitted exact fixed
/// builtin. The optimizer receives only the existence of this plan; every
/// field is consumed while constructing entry guards and resource budgets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeExactCallFramePlan {
    Fixed {
        native_arity: usize,
    },
    VariadicSlice {
        length: usize,
    },
    ShutdownCallback {
        length: usize,
    },
    FrameIntrospection {
        supplied_arguments: usize,
        fixed_parameters: usize,
    },
}

impl NativeExactCallFramePlan {
    const fn supplied_arguments(self) -> usize {
        match self {
            Self::Fixed { native_arity } => native_arity,
            Self::VariadicSlice { length } | Self::ShutdownCallback { length } => length,
            Self::FrameIntrospection {
                supplied_arguments, ..
            } => supplied_arguments,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeExactCallbackPlan {
    None,
    CallableValue,
    CallbackAndArgumentArray,
    ShutdownCallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeExactSemanticOutcome {
    Return,
    Throw,
    RuntimeError,
    AbiMismatch,
}

const NATIVE_EXACT_TOTAL_OUTCOMES: [NativeExactSemanticOutcome; 4] = [
    NativeExactSemanticOutcome::Return,
    NativeExactSemanticOutcome::Throw,
    NativeExactSemanticOutcome::RuntimeError,
    NativeExactSemanticOutcome::AbiMismatch,
];

#[derive(Clone, Debug)]
struct NativeFixedBuiltinPublicationPlan {
    capability_family: &'static str,
    provided_arity: usize,
    native_arity: usize,
    defaulted_argument_count: usize,
    variadic: bool,
    defaults_published: bool,
    operand_classes: Vec<SsaValueClass>,
    operand_ownership: Vec<SsaOwnership>,
    mode_operands: Vec<usize>,
    mutation_locals: Vec<LocalId>,
    cleanup_owned_arguments: Vec<usize>,
    frame: NativeExactCallFramePlan,
    callback: NativeExactCallbackPlan,
    semantic_outcomes: [NativeExactSemanticOutcome; 4],
    reserved_value_slots: usize,
    reserved_array_entries: usize,
    reserved_string_bytes: usize,
}

impl NativeFixedBuiltinPublicationPlan {
    fn is_total(&self) -> bool {
        !self.capability_family.is_empty()
            && self.defaults_published
            && self.native_arity >= self.provided_arity
            && self.defaulted_argument_count
                == self.native_arity.saturating_sub(self.provided_arity)
            && (!self.variadic || self.defaulted_argument_count == 0)
            && self.operand_classes.len() == self.provided_arity
            && self.operand_ownership.len() == self.provided_arity
            && self
                .mode_operands
                .iter()
                .all(|index| *index < self.provided_arity)
            && self
                .mutation_locals
                .iter()
                .all(|local| local.raw() < u32::MAX)
            && self
                .cleanup_owned_arguments
                .iter()
                .all(|index| *index < self.provided_arity)
            && self.frame.supplied_arguments() >= self.provided_arity
            && match self.callback {
                NativeExactCallbackPlan::None => true,
                NativeExactCallbackPlan::CallableValue => self.provided_arity >= 1,
                NativeExactCallbackPlan::CallbackAndArgumentArray => self.provided_arity >= 2,
                NativeExactCallbackPlan::ShutdownCallback => {
                    matches!(
                        self.frame,
                        NativeExactCallFramePlan::ShutdownCallback { .. }
                    ) && self.provided_arity >= 1
                }
            }
            && self.semantic_outcomes == NATIVE_EXACT_TOTAL_OUTCOMES
            && self.reserved_value_slots != 0
            && self.reserved_array_entries != 0
            && self.reserved_string_bytes != 0
    }
}

#[derive(Clone, Debug, Default)]
struct NativeOptimizingAdmission {
    total_array_calls: BTreeSet<u32>,
    total_fixed_builtin_calls: BTreeSet<u32>,
    fixed_builtin_plans: BTreeMap<u32, NativeFixedBuiltinPublicationPlan>,
    total_array_instructions: BTreeSet<u32>,
    total_binary_instructions: BTreeSet<u32>,
    total_scalar_control_instructions: BTreeSet<u32>,
    fresh_array_instructions: BTreeSet<u32>,
    total_local_loads: BTreeSet<u32>,
    total_request_local_stores: BTreeSet<u32>,
    total_return_reference_stores: BTreeSet<u32>,
    total_return_reference_locals: BTreeSet<LocalId>,
    total_terminators: BTreeSet<u32>,
    return_plans: BTreeMap<u32, NativeOptimizingReturnPlan>,
    total_reference_locals: BTreeSet<LocalId>,
    array_requirements: BTreeMap<NativeEntryArraySource, NativeEntryArrayRequirement>,
    initialized_request_locals: BTreeSet<LocalId>,
    releasable_request_locals: BTreeSet<LocalId>,
    initialized_globals: BTreeSet<u32>,
    releasable_globals: BTreeSet<u32>,
    plain_globals: BTreeSet<u32>,
    reference_source_parameters: BTreeSet<usize>,
    property_requirements: Vec<NativeEntryPropertyRequirement>,
    static_property_requirements: Vec<NativeEntryStaticPropertyRequirement>,
    object_layout_requirements: Vec<NativeEntryObjectLayoutRequirement>,
    instanceof_requirements: Vec<NativeEntryInstanceofRequirement>,
    callable_requirements: Vec<NativeEntryCallableRequirement>,
    clone_requirements: Vec<NativeEntryCloneRequirement>,
    dynamic_property_requirements: Vec<NativeEntryDynamicPropertyRequirement>,
    exception_requirements: Vec<NativeEntryExceptionRequirement>,
    value_class_requirements: Vec<NativeEntryValueClassRequirement>,
    string_requirements: Vec<NativeEntryStringRequirement>,
    resource_type_requirements: Vec<NativeEntryResourceTypeRequirement>,
    integer_requirements: Vec<NativeEntryIntegerRequirement>,
    float_requirements: Vec<NativeEntryFloatRequirement>,
    trusted_constant_requirements: BTreeSet<u32>,
    trusted_static_local_requirements: BTreeSet<u32>,
    equal_array_length_groups: Vec<Vec<NativeEntryArraySource>>,
    fixed_value_allocations: usize,
    fixed_array_entries: usize,
    fixed_string_bytes: usize,
    fixed_lvalue_insertions: usize,
    require_non_fiber_scope: bool,
}

impl NativeOptimizingAdmission {
    fn array_call_is_total(&self, continuation_id: u32) -> bool {
        self.total_array_calls.contains(&continuation_id)
    }

    fn fixed_builtin_call_is_total(&self, continuation_id: u32) -> bool {
        self.fixed_builtin_plans
            .get(&continuation_id)
            .is_some_and(NativeFixedBuiltinPublicationPlan::is_total)
            || self.total_fixed_builtin_calls.contains(&continuation_id)
    }

    fn array_instruction_is_total(&self, continuation_id: u32) -> bool {
        self.total_array_instructions.contains(&continuation_id)
    }

    fn binary_instruction_is_total(&self, continuation_id: u32) -> bool {
        self.total_binary_instructions.contains(&continuation_id)
    }

    fn scalar_control_instruction_is_total(&self, continuation_id: u32) -> bool {
        self.total_scalar_control_instructions
            .contains(&continuation_id)
    }

    fn array_instruction_is_fresh(&self, continuation_id: u32) -> bool {
        self.fresh_array_instructions.contains(&continuation_id)
    }

    fn local_load_is_total(&self, continuation_id: u32) -> bool {
        self.total_local_loads.contains(&continuation_id)
    }

    fn request_local_store_is_total(&self, continuation_id: u32) -> bool {
        self.total_request_local_stores.contains(&continuation_id)
    }

    fn return_reference_store_is_total(&self, continuation_id: u32) -> bool {
        self.total_return_reference_stores
            .contains(&continuation_id)
    }

    fn return_reference_is_prebound(&self, local: LocalId) -> bool {
        self.total_reference_locals.contains(&local)
    }

    fn terminator_is_total(&self, continuation_id: u32) -> bool {
        self.total_terminators.contains(&continuation_id)
    }

    fn return_plan(&self, continuation_id: u32) -> Option<NativeOptimizingReturnPlan> {
        self.return_plans.get(&continuation_id).copied()
    }
}

fn publication_root_operand(
    mut operand: RegionOperand,
    definitions: &BTreeMap<RegId, RegionOperand>,
) -> RegionOperand {
    for _ in 0..=definitions.len() {
        let RegionOperand::Register(register) = operand else {
            break;
        };
        let Some(definition) = definitions.get(&register) else {
            break;
        };
        operand = *definition;
    }
    operand
}

fn publication_return_plan(
    operand: RegionOperand,
    fact: crate::region_ir::SsaValueFact,
    return_type: &php_ir::IrReturnType,
    strict: bool,
    definitions: &BTreeMap<RegId, RegionOperand>,
    constants: &[IrConstant],
) -> Option<NativeOptimizingReturnPlan> {
    use php_ir::IrReturnType as Type;
    use php_runtime::experimental::numeric_string::{
        NumericStringKind, NumericStringValue, classify,
    };

    let return_type = match return_type {
        Type::Nullable { inner } => inner.as_ref(),
        return_type => return_type,
    };
    let root = publication_root_operand(operand, definitions);
    let constant = match root {
        RegionOperand::I64(value) => Some(IrConstant::Int(value)),
        RegionOperand::Constant(index) => constants.get(index as usize).cloned(),
        RegionOperand::LinkedConstant { constant, .. } => constants.get(constant as usize).cloned(),
        RegionOperand::Register(_) | RegionOperand::Local(_) => None,
    };

    if strict {
        return match (return_type, constant) {
            // PHP's sole strict scalar widening is int -> float.
            (Type::Float, Some(IrConstant::Int(value))) => Some(
                NativeOptimizingReturnPlan::DirectFloat((value as f64).to_bits()),
            ),
            // A non-constant authoritative integer uses the same total CLIF
            // conversion after its integer class was proved by value flow.
            (Type::Float, None) if fact.class == SsaValueClass::Int => {
                Some(NativeOptimizingReturnPlan::IntToFloat)
            }
            _ => None,
        };
    }

    let immediate_integer = |value| {
        if native_integer_fits_immediate(value) {
            NativeOptimizingReturnPlan::Immediate(value)
        } else {
            NativeOptimizingReturnPlan::DirectInt(value)
        }
    };
    let numeric_string_plan = |bytes: &[u8]| {
        let classified = classify(bytes);
        if !matches!(
            classified.kind,
            NumericStringKind::IntString | NumericStringKind::FloatString
        ) || classified.overflow_or_precision_sensitive
        {
            return None;
        }
        match (return_type, classified.value?) {
            (Type::Int, NumericStringValue::Int(value)) => Some(immediate_integer(value)),
            (Type::Int, NumericStringValue::Float(value))
                if value.is_finite()
                    && value.fract() == 0.0
                    && value >= i64::MIN as f64
                    && value < -(i64::MIN as f64) =>
            {
                Some(immediate_integer(value as i64))
            }
            (Type::Float, value) => Some(NativeOptimizingReturnPlan::DirectFloat(
                value.as_f64().to_bits(),
            )),
            _ => None,
        }
    };
    match (return_type, constant) {
        (Type::Int, Some(IrConstant::Bool(value))) => {
            Some(NativeOptimizingReturnPlan::Immediate(i64::from(value)))
        }
        (Type::Float, Some(IrConstant::Bool(value))) => Some(
            NativeOptimizingReturnPlan::DirectFloat(f64::from(u8::from(value)).to_bits()),
        ),
        (Type::Float, Some(IrConstant::Int(value))) => Some(
            NativeOptimizingReturnPlan::DirectFloat((value as f64).to_bits()),
        ),
        (Type::Int | Type::Float, Some(IrConstant::String(value))) => {
            numeric_string_plan(value.as_bytes())
        }
        (Type::Int | Type::Float, Some(IrConstant::StringBytes(value))) => {
            numeric_string_plan(&value)
        }
        _ => None,
    }
}

fn entry_array_source(
    operand: RegionOperand,
    definitions: &BTreeMap<RegId, RegionOperand>,
    parameter_indices: &BTreeMap<LocalId, usize>,
) -> Option<NativeEntryArraySource> {
    let mut operand = operand;
    for _ in 0..32 {
        match operand {
            RegionOperand::Register(register) => operand = *definitions.get(&register)?,
            RegionOperand::Local(local) => {
                return parameter_indices
                    .get(&local)
                    .copied()
                    .map(NativeEntryArraySource::Parameter);
            }
            RegionOperand::Constant(_)
            | RegionOperand::I64(_)
            | RegionOperand::LinkedConstant { .. } => return None,
        }
    }
    None
}

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

fn publication_integer_array_key(constants: &[IrConstant], key: RegionOperand) -> Option<i64> {
    match key {
        RegionOperand::Constant(index) => match constants.get(index as usize) {
            Some(IrConstant::Int(value)) if native_integer_fits_immediate(*value) => Some(*value),
            _ => None,
        },
        RegionOperand::I64(value) if native_integer_fits_immediate(value) => Some(value),
        RegionOperand::Register(_)
        | RegionOperand::Local(_)
        | RegionOperand::I64(_)
        | RegionOperand::LinkedConstant { .. } => None,
    }
}

fn publication_integer_operand(
    constants: &[IrConstant],
    definitions: &BTreeMap<RegId, RegionOperand>,
    operand: RegionOperand,
) -> Option<i64> {
    match publication_root_operand(operand, definitions) {
        RegionOperand::I64(value) => Some(value),
        RegionOperand::Constant(index)
        | RegionOperand::LinkedConstant {
            constant: index, ..
        } => match constants.get(index as usize) {
            Some(IrConstant::Int(value)) => Some(*value),
            _ => None,
        },
        RegionOperand::Register(_) | RegionOperand::Local(_) => None,
    }
}

fn publication_bool_operand(
    constants: &[IrConstant],
    definitions: &BTreeMap<RegId, RegionOperand>,
    operand: RegionOperand,
) -> Option<bool> {
    match publication_root_operand(operand, definitions) {
        RegionOperand::Constant(index)
        | RegionOperand::LinkedConstant {
            constant: index, ..
        } => match constants.get(index as usize) {
            Some(IrConstant::Bool(value)) => Some(*value),
            _ => None,
        },
        RegionOperand::Register(_) | RegionOperand::Local(_) | RegionOperand::I64(_) => None,
    }
}

fn publication_string_length(
    constants: &[IrConstant],
    definitions: &BTreeMap<RegId, RegionOperand>,
    operand: RegionOperand,
) -> Option<usize> {
    match publication_root_operand(operand, definitions) {
        RegionOperand::Constant(index)
        | RegionOperand::LinkedConstant {
            constant: index, ..
        } => match constants.get(index as usize) {
            Some(IrConstant::String(value)) => Some(value.len()),
            Some(IrConstant::StringBytes(value)) => Some(value.len()),
            _ => None,
        },
        RegionOperand::Register(_) | RegionOperand::Local(_) | RegionOperand::I64(_) => None,
    }
}

fn publication_utf8_string<'a>(
    constants: &'a [IrConstant],
    definitions: &BTreeMap<RegId, RegionOperand>,
    operand: RegionOperand,
) -> Option<&'a str> {
    match publication_root_operand(operand, definitions) {
        RegionOperand::Constant(index)
        | RegionOperand::LinkedConstant {
            constant: index, ..
        } => match constants.get(index as usize)? {
            IrConstant::String(value) => Some(value.as_str()),
            IrConstant::StringBytes(value) => std::str::from_utf8(value).ok(),
            _ => None,
        },
        RegionOperand::Register(_) | RegionOperand::Local(_) | RegionOperand::I64(_) => None,
    }
}

fn publication_string_capacity(length: usize) -> Option<usize> {
    length
        .max(crate::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY as usize)
        .checked_next_power_of_two()
        .filter(|capacity| *capacity <= crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY)
}

fn publication_entry_parameter(
    operand: RegionOperand,
    definitions: &BTreeMap<RegId, RegionOperand>,
    parameter_indices: &BTreeMap<LocalId, usize>,
) -> Option<usize> {
    match entry_array_source(operand, definitions, parameter_indices)? {
        NativeEntryArraySource::Parameter(parameter_index) => Some(parameter_index),
        NativeEntryArraySource::TrustedGlobal(_) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn admit_publication_scalar_class(
    admission: &mut NativeOptimizingAdmission,
    value_flow: &ExecutableValueFlow,
    constants: &[IrConstant],
    definitions: &BTreeMap<RegId, RegionOperand>,
    parameter_indices: &BTreeMap<LocalId, usize>,
    operand: RegionOperand,
    class: SsaValueClass,
    continuation_id: u32,
    family: &str,
) -> Result<Option<usize>, CraneliftLoweringError> {
    let fact = lowering_operand_fact(value_flow, constants, operand);
    if fact.certainty == crate::region_ir::SsaCertainty::Unknown || fact.class != class {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_SCALAR_PUBLICATION",
            format!(
                "{family} at continuation {continuation_id} has no publication-proven {class:?} operand",
            ),
        ));
    }
    let parameter = publication_entry_parameter(operand, definitions, parameter_indices);
    if let Some(parameter_index) = parameter {
        admission
            .value_class_requirements
            .push(NativeEntryValueClassRequirement {
                parameter_index,
                class,
            });
    }
    Ok(parameter)
}

#[allow(clippy::too_many_arguments)]
fn admit_publication_string(
    admission: &mut NativeOptimizingAdmission,
    value_flow: &ExecutableValueFlow,
    constants: &[IrConstant],
    definitions: &BTreeMap<RegId, RegionOperand>,
    parameter_indices: &BTreeMap<LocalId, usize>,
    operand: RegionOperand,
    minimum_length: usize,
    allocation_multiplier: usize,
    continuation_id: u32,
    family: &str,
) -> Result<Option<usize>, CraneliftLoweringError> {
    if let Some(length) = publication_string_length(constants, definitions, operand) {
        if length < minimum_length {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_STRING_PUBLICATION",
                format!(
                    "{family} at continuation {continuation_id} has a fixed string shorter than {minimum_length}",
                ),
            ));
        }
        if allocation_multiplier != 0 {
            let output = length.checked_mul(allocation_multiplier).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_STRING_PUBLICATION",
                    format!("{family} output length overflows at publication"),
                )
            })?;
            let capacity = publication_string_capacity(output).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_STRING_PUBLICATION",
                    format!("{family} output exceeds the native string arena"),
                )
            })?;
            admission.fixed_string_bytes = admission.fixed_string_bytes.saturating_add(capacity);
        }
        return Ok(None);
    }
    let parameter_index = admit_publication_scalar_class(
        admission,
        value_flow,
        constants,
        definitions,
        parameter_indices,
        operand,
        SsaValueClass::StringHandle,
        continuation_id,
        family,
    )?
    .ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STRING_PUBLICATION",
            format!("{family} at continuation {continuation_id} is not rooted at an entry string",),
        )
    })?;
    admission
        .string_requirements
        .push(NativeEntryStringRequirement {
            parameter_index,
            minimum_length,
            offset: None,
            allocation_multiplier,
        });
    Ok(Some(parameter_index))
}

#[allow(clippy::too_many_arguments)]
fn admit_publication_integer(
    admission: &mut NativeOptimizingAdmission,
    value_flow: &ExecutableValueFlow,
    constants: &[IrConstant],
    definitions: &BTreeMap<RegId, RegionOperand>,
    parameter_indices: &BTreeMap<LocalId, usize>,
    operand: RegionOperand,
    minimum: i64,
    maximum: i64,
    forbidden_values: &[i64],
    continuation_id: u32,
    family: &str,
) -> Result<Option<i64>, CraneliftLoweringError> {
    if let Some(value) = publication_integer_operand(constants, definitions, operand) {
        if !(minimum..=maximum).contains(&value) || forbidden_values.contains(&value) {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_INTEGER_PUBLICATION",
                format!(
                    "{family} at continuation {continuation_id} has an invalid fixed integer {value}",
                ),
            ));
        }
        return Ok(Some(value));
    }
    let parameter_index =
        admit_publication_scalar_class(
            admission,
            value_flow,
            constants,
            definitions,
            parameter_indices,
            operand,
            SsaValueClass::Int,
            continuation_id,
            family,
        )?
        .ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_INTEGER_PUBLICATION",
                format!(
                    "{family} at continuation {continuation_id} is not rooted at an entry integer",
                ),
            )
        })?;
    admission
        .integer_requirements
        .push(NativeEntryIntegerRequirement {
            parameter_index,
            minimum,
            maximum,
            forbidden_values: forbidden_values.to_vec(),
        });
    Ok(None)
}

fn publication_native_array_key(
    constants: &[IrConstant],
    key: RegionOperand,
) -> Option<NativeEntryArrayKey> {
    match key {
        RegionOperand::Constant(index) => match constants.get(index as usize) {
            Some(IrConstant::Int(value)) if native_integer_fits_immediate(*value) => {
                Some(NativeEntryArrayKey::Integer(*value))
            }
            Some(IrConstant::Bool(value)) => Some(NativeEntryArrayKey::Integer(i64::from(*value))),
            Some(IrConstant::String(_) | IrConstant::StringBytes(_)) => {
                Some(NativeEntryArrayKey::Constant(index))
            }
            _ => None,
        },
        RegionOperand::I64(value) if native_integer_fits_immediate(value) => {
            Some(NativeEntryArrayKey::Integer(value))
        }
        RegionOperand::Register(_)
        | RegionOperand::Local(_)
        | RegionOperand::I64(_)
        | RegionOperand::LinkedConstant { .. } => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn publish_total_fixed_builtin_plan(
    admission: &mut NativeOptimizingAdmission,
    call: &RegionNativeCall,
    continuation_id: u32,
    family: &'static str,
    value_flow: &ExecutableValueFlow,
    constants: &[IrConstant],
    definitions: &BTreeMap<RegId, RegionOperand>,
    parameter_indices: &BTreeMap<LocalId, usize>,
    fixed_parameter_count: usize,
) -> Result<(), CraneliftLoweringError> {
    if call.args.iter().any(|argument| {
        argument.name.is_some()
            || argument.unpack
            || argument.by_ref_dim.is_some()
            || argument.by_ref_property.is_some()
            || argument.by_ref_property_dim.is_some()
    }) {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_FIXED_BUILTIN_ARGUMENT_PLAN",
            format!(
                "{family} builtin at continuation {continuation_id} has no total positional native argument plan",
            ),
        ));
    }

    let mut operand_classes = Vec::with_capacity(call.args.len());
    let mut operand_ownership = Vec::with_capacity(call.args.len());
    let mut mode_operands = Vec::new();
    let mut mutation_locals = Vec::new();
    for (index, argument) in call.args.iter().enumerate() {
        let operand = direct_fixed_builtin_operand(call, index)
            .or_else(|| argument.by_ref_local.map(RegionOperand::Local))
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FIXED_BUILTIN_OPERAND_PLAN",
                    format!(
                        "{family} builtin argument {index} at continuation {continuation_id} has no published native operand",
                    ),
                )
            })?;
        let fact = lowering_operand_fact(value_flow, constants, operand);
        let entry_parameter = publication_entry_parameter(operand, definitions, parameter_indices);
        let published_entry_class = entry_parameter.and_then(|parameter_index| {
            admission
                .value_class_requirements
                .iter()
                .rev()
                .find(|requirement| requirement.parameter_index == parameter_index)
                .map(|requirement| requirement.class)
        });
        let (class, ownership) = if argument.by_ref_local.is_some() {
            (
                SsaValueClass::ReferenceHandle,
                SsaOwnership::AliasedReference,
            )
        } else if fact.certainty != crate::region_ir::SsaCertainty::Unknown
            && fact.ownership != SsaOwnership::Unknown
            && !matches!(
                fact.class,
                SsaValueClass::ReferenceHandle | SsaValueClass::MixedHandle
            )
        {
            (fact.class, fact.ownership)
        } else if let Some(class) = published_entry_class {
            (class, SsaOwnership::Borrowed)
        } else {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_FIXED_BUILTIN_VALUE_PLAN",
                format!(
                    "{family} builtin argument {index} at continuation {continuation_id} has no total class/ownership plan",
                ),
            ));
        };

        // Exact known SSA facts already form part of the function-entry
        // contract. Adding a second value-class guard here would pin an
        // otherwise plain last-use parameter in local storage.
        if class == SsaValueClass::ArrayHandle
            && let Some(source) = entry_array_source(operand, definitions, parameter_indices)
        {
            let requirement = admission.array_requirements.entry(source).or_default();
            requirement.require_supported_keys = true;
            requirement.require_plain_values = true;
        }
        if matches!(class, SsaValueClass::Int | SsaValueClass::Bool) {
            mode_operands.push(index);
        }
        if let Some(local) = argument.by_ref_local {
            mutation_locals.push(local);
        }
        operand_classes.push(class);
        operand_ownership.push(ownership);
    }

    // Every exact call receives a fixed result owner, a bounded small-array
    // envelope, and a native string block before entry. Input-dependent
    // string/array requirements added by the family classifier extend these
    // fixed minima below; allocation failure inside an admitted handler is
    // therefore an ABI contract violation rather than a runtime fallback.
    const FIXED_RESULT_VALUES: usize = 8;
    const FIXED_RESULT_ENTRIES: usize = 16;
    const FIXED_RESULT_STRING_BYTES: usize = 4096;
    let provided_arity = call.args.len();
    let native_arity = if call.variadic {
        provided_arity
    } else {
        call.direct_arity
            .and_then(|arity| usize::try_from(arity).ok())
            .and_then(|arity| arity.checked_sub(call.argument_operand_offset))
            .unwrap_or(provided_arity)
            .max(provided_arity)
    };
    let defaulted_argument_count = native_arity.saturating_sub(provided_arity);
    let frame = if stable_builtin_shutdown_callback(&call.target) {
        NativeExactCallFramePlan::ShutdownCallback {
            length: provided_arity,
        }
    } else if stable_builtin_array_multisort(&call.target)
        || matches!(
            stable_builtin_format(&call.target),
            Some(StableFormatBuiltin::Sprintf | StableFormatBuiltin::Printf)
        )
        || matches!(
            stable_builtin_byte_codec(&call.target),
            Some(StableByteCodecBuiltin::Pack)
        )
    {
        NativeExactCallFramePlan::VariadicSlice {
            length: provided_arity,
        }
    } else if stable_builtin_frame_introspection(&call.target).is_some() {
        NativeExactCallFramePlan::FrameIntrospection {
            supplied_arguments: provided_arity,
            fixed_parameters: fixed_parameter_count,
        }
    } else {
        NativeExactCallFramePlan::Fixed { native_arity }
    };
    let callback = if stable_builtin_shutdown_callback(&call.target) {
        NativeExactCallbackPlan::ShutdownCallback
    } else if stable_builtin_callback_neutral_array(&call.target).is_some() {
        NativeExactCallbackPlan::CallbackAndArgumentArray
    } else if stable_builtin_callable_query(&call.target).is_some()
        || stable_builtin_callback_handler(&call.target).is_some()
        || stable_builtin_autoload_callback(&call.target).is_some()
    {
        NativeExactCallbackPlan::CallableValue
    } else {
        NativeExactCallbackPlan::None
    };
    let cleanup_owned_arguments = operand_ownership
        .iter()
        .enumerate()
        .filter_map(|(index, ownership)| (*ownership == SsaOwnership::Owned).then_some(index))
        .collect();
    admission.fixed_value_allocations = admission
        .fixed_value_allocations
        .saturating_add(FIXED_RESULT_VALUES);
    admission.fixed_array_entries = admission
        .fixed_array_entries
        .saturating_add(FIXED_RESULT_ENTRIES);
    admission.fixed_string_bytes = admission
        .fixed_string_bytes
        .saturating_add(FIXED_RESULT_STRING_BYTES);
    admission.fixed_builtin_plans.insert(
        continuation_id,
        NativeFixedBuiltinPublicationPlan {
            capability_family: family,
            provided_arity,
            native_arity,
            defaulted_argument_count,
            variadic: call.variadic,
            defaults_published: true,
            operand_classes,
            operand_ownership,
            mode_operands,
            mutation_locals,
            cleanup_owned_arguments,
            frame,
            callback,
            semantic_outcomes: NATIVE_EXACT_TOTAL_OUTCOMES,
            reserved_value_slots: FIXED_RESULT_VALUES,
            reserved_array_entries: FIXED_RESULT_ENTRIES,
            reserved_string_bytes: FIXED_RESULT_STRING_BYTES,
        },
    );
    Ok(())
}

fn optimizing_admission_for_region(
    region: &RegionGraph,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
) -> Result<NativeOptimizingAdmission, CraneliftLoweringError> {
    if region.compile_metadata.tier != NativeCompilerTier::Optimizing {
        return Ok(NativeOptimizingAdmission::default());
    }
    // Scalar result encodings are bounded by the static instruction graph.
    // Reserve a conservative fixed number of direct descriptors per
    // instruction before entering the region; array/callback loops add their
    // separate length-dependent budgets below.
    let instruction_count = region
        .blocks
        .iter()
        .map(|block| block.instructions.len())
        .sum::<usize>();
    let mut admission = NativeOptimizingAdmission {
        fixed_value_allocations: instruction_count.saturating_mul(4),
        ..NativeOptimizingAdmission::default()
    };
    if region
        .exception_regions
        .iter()
        .any(|handler| handler.exception_local.is_some())
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_HANDLER_LVALUE_BOUNDARY",
            "catch-local replacement is not total at optimizing entry",
        ));
    }
    let parameter_indices = region
        .parameter_locals
        .iter()
        .copied()
        .enumerate()
        .map(|(index, local)| (local, index))
        .collect::<BTreeMap<_, _>>();
    let by_ref_parameters = region
        .params
        .iter()
        .filter_map(|parameter| parameter.by_ref.then_some(parameter.local))
        .collect::<BTreeSet<_>>();
    let variadic_parameters = region
        .params
        .iter()
        .filter_map(|parameter| parameter.variadic.then_some(parameter.local))
        .collect::<BTreeSet<_>>();
    let definitions = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::LoadLocal {
                dst, local, quiet, ..
            } if !quiet => Some((dst, RegionOperand::Local(local))),
            RegionInstructionKind::Move { dst, src } => Some((dst, src)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let first_local_write = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| {
            let local = match &instruction.kind {
                RegionInstructionKind::StoreLocal { local, .. }
                | RegionInstructionKind::AssignLocalResult { local, .. }
                | RegionInstructionKind::BindReference { target: local, .. }
                | RegionInstructionKind::BindReferenceDim { target: local, .. }
                | RegionInstructionKind::BindReferenceFromProperty { target: local, .. }
                | RegionInstructionKind::BindReferenceFromPropertyDim { target: local, .. } => {
                    Some(*local)
                }
                RegionInstructionKind::BindReferenceDimFromProperty {
                    array: local,
                    keys,
                    append: false,
                    ..
                } if keys.is_empty() => Some(*local),
                RegionInstructionKind::NativeCall(RegionNativeCall {
                    result: RegionCallResult::ReferenceLocal(local),
                    ..
                }) => Some(*local),
                _ => None,
            }?;
            Some((local, instruction.continuation_id))
        })
        .fold(
            BTreeMap::<LocalId, u32>::new(),
            |mut writes, (local, id)| {
                writes.entry(local).or_insert(id);
                writes
            },
        );
    let new_array_registers = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::NewArray { dst } => Some(dst),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let foreach_sources = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::ForeachInit { iterator, source } => Some((iterator, source)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let foreach_reference_sources = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::ForeachInitRef { iterator, local } => Some((iterator, local)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let foreach_reference_value_locals = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::ForeachNextRef { value_local, .. } => Some(value_local),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let foreach_reference_local_writes = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::StoreLocal { local, .. }
            | RegionInstructionKind::AssignLocalResult { local, .. }
            | RegionInstructionKind::UnsetLocal { local }
                if foreach_reference_value_locals.contains(&local) =>
            {
                Some(instruction.continuation_id)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let new_array_local_sources = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::StoreLocal {
                local,
                src: RegionOperand::Register(register),
            } if new_array_registers.contains(&register) => Some((local, register)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let new_array_local_store_continuations = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::StoreLocal {
                local,
                src: RegionOperand::Register(register),
            } if new_array_registers.contains(&register) => {
                Some((local, instruction.continuation_id))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let new_array_locals = new_array_local_sources
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    let new_array_mutation_counts = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::AppendDim { local, .. }
            | RegionInstructionKind::AssignDim { local, .. }
            | RegionInstructionKind::UnsetDim { local, .. }
                if new_array_locals.contains(local) =>
            {
                Some(*local)
            }
            _ => None,
        })
        .fold(BTreeMap::<LocalId, usize>::new(), |mut counts, local| {
            *counts.entry(local).or_default() += 1;
            counts
        });
    let array_local_mutation_counts = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::AppendDim { local, .. }
            | RegionInstructionKind::AssignDim { local, .. }
            | RegionInstructionKind::UnsetDim { local, .. } => Some(local),
            _ => None,
        })
        .fold(BTreeMap::<LocalId, usize>::new(), |mut counts, local| {
            *counts.entry(local).or_default() += 1;
            counts
        });
    let new_array_assigned_integer_keys = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::AssignDim { local, keys, .. }
                if new_array_locals.contains(local) && keys.len() == 1 =>
            {
                publication_integer_array_key(constants, keys[0]).map(|key| (*local, key))
            }
            _ => None,
        })
        .fold(
            BTreeMap::<LocalId, BTreeSet<i64>>::new(),
            |mut keys, (local, key)| {
                keys.entry(local).or_default().insert(key);
                keys
            },
        );
    let invalidated_locals = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::UnsetLocal { local } => Some(local),
            RegionInstructionKind::BindReference { target, .. }
                if region.flags.is_top_level
                    && value_flow.local_storage(target)
                        == crate::region_ir::LocalStorageClass::RequestGlobal =>
            {
                Some(target)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let request_local_write_counts = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::StoreLocal { local, .. }
            | RegionInstructionKind::AssignLocalResult { local, .. }
                if matches!(
                    value_flow.local_storage(local),
                    crate::region_ir::LocalStorageClass::RequestGlobal
                        | crate::region_ir::LocalStorageClass::Superglobal
                ) =>
            {
                Some(local)
            }
            _ => None,
        })
        .fold(BTreeMap::<LocalId, usize>::new(), |mut counts, local| {
            *counts.entry(local).or_default() += 1;
            counts
        });
    let mut unstable_before = BTreeSet::new();
    let mut external_effect_seen = false;
    for (block_index, block) in region.blocks.iter().enumerate() {
        for instruction in &block.instructions {
            if block_index != 0 || external_effect_seen {
                unstable_before.insert(instruction.continuation_id);
            }
            let planned_lvalue_effect = matches!(
                &instruction.kind,
                RegionInstructionKind::BindReference { .. }
                    | RegionInstructionKind::BindReferenceProperty { .. }
                    | RegionInstructionKind::BindReferenceFromProperty { .. }
                    | RegionInstructionKind::BindReferenceIntoPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceFromPropertyDim { .. }
                    | RegionInstructionKind::BindReferenceDimFromProperty { .. }
                    | RegionInstructionKind::BindReferenceDim { .. }
                    | RegionInstructionKind::BindReferenceIntoDim { .. }
                    | RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Semantic {
                            operation: RegionSemanticOp::PropertyDimAssign { .. }
                                | RegionSemanticOp::PropertyDimUnset { .. }
                                | RegionSemanticOp::StaticPropertyDimUnset { .. }
                                | RegionSemanticOp::StaticPropertyReference { .. },
                        },
                        ..
                    })
            );
            let planned_publication_total_query = matches!(
                &instruction.kind,
                RegionInstructionKind::NativeCall(call)
                    if stable_builtin_type_predicate(&call.target).is_some()
                        || stable_builtin_length(&call.target).is_some()
                        || matches!(
                            stable_builtin_array_aggregate(&call.target),
                            Some(
                                StableArrayAggregateBuiltin::Count
                                    | StableArrayAggregateBuiltin::SizeOf
                            )
                        ) && (call.args.len() == 1
                            || call.args.len() == 2
                                && direct_fixed_builtin_operand(call, 1).is_some_and(|mode| {
                                    lowering_operand_fact(value_flow, constants, mode).integer_range
                                        == Some(crate::region_ir::SsaIntegerRange::exact(0))
                                }))
            );
            external_effect_seen |= !planned_lvalue_effect
                && !planned_publication_total_query
                && matches!(
                    instruction.kind,
                    RegionInstructionKind::NativeCall(_)
                        | RegionInstructionKind::ArrayCallback(_)
                        | RegionInstructionKind::PregCallbackArray(_)
                        | RegionInstructionKind::NativeDynamicCode(_)
                        | RegionInstructionKind::NativeSuspend(_)
                        | RegionInstructionKind::AssignProperty { .. }
                        | RegionInstructionKind::AssignDim { .. }
                        | RegionInstructionKind::AppendDim { .. }
                        | RegionInstructionKind::UnsetDim { .. }
                );
        }
    }
    admission.reference_source_parameters = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::BindReference { source, .. }
            | RegionInstructionKind::BindReferenceIntoDim { source, .. }
            | RegionInstructionKind::BindReferenceProperty { source, .. }
            | RegionInstructionKind::BindReferenceIntoPropertyDim { source, .. } => {
                parameter_indices.get(source).copied()
            }
            RegionInstructionKind::NativeCall(RegionNativeCall {
                target:
                    RegionCallTarget::Semantic {
                        operation:
                            RegionSemanticOp::StaticPropertyReference {
                                target,
                                bind_source_into_property: true,
                                ..
                            },
                    },
                ..
            }) => parameter_indices.get(target).copied(),
            _ => None,
        })
        .collect();
    admission.total_reference_locals = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .flat_map(|instruction| match &instruction.kind {
            RegionInstructionKind::BindReference { target, source } => vec![*target, *source],
            RegionInstructionKind::BindReferenceDim { target, .. }
            | RegionInstructionKind::BindReferenceFromProperty { target, .. }
            | RegionInstructionKind::BindReferenceFromPropertyDim { target, .. } => vec![*target],
            RegionInstructionKind::BindReferenceIntoDim { source, .. }
            | RegionInstructionKind::BindReferenceProperty { source, .. }
            | RegionInstructionKind::BindReferenceIntoPropertyDim { source, .. } => vec![*source],
            RegionInstructionKind::BindReferenceDimFromProperty {
                array,
                keys,
                append: false,
                ..
            } if keys.is_empty() => vec![*array],
            RegionInstructionKind::NativeCall(RegionNativeCall {
                result: RegionCallResult::ReferenceLocal(local),
                target:
                    RegionCallTarget::Semantic {
                        operation: RegionSemanticOp::StaticPropertyReference { .. },
                    },
                ..
            }) => vec![*local],
            _ => Vec::new(),
        })
        .collect();
    let mut admitted_array_fetches = BTreeMap::<RegId, (NativeEntryArraySource, i64)>::new();
    let mut entry_dependent_continuations = BTreeSet::new();
    let mut fresh_insert_modes = BTreeMap::<RegId, bool>::new();
    let mut fresh_insert_keys = BTreeMap::<RegId, BTreeSet<NativeEntryArrayKey>>::new();
    let mut fresh_insert_counts = BTreeMap::<RegId, usize>::new();
    let mut fresh_spread_targets = BTreeSet::<RegId>::new();
    for instruction in region.blocks.iter().flat_map(|block| &block.instructions) {
        if let RegionInstructionKind::Binary { dst, op, lhs, rhs } = instruction.kind {
            let lhs_fact = lowering_operand_fact(value_flow, constants, lhs);
            let rhs_fact = lowering_operand_fact(value_flow, constants, rhs);
            let result_fact = value_flow.register_fact(dst);
            let known = |fact: crate::region_ir::SsaValueFact, class| {
                fact.certainty != crate::region_ir::SsaCertainty::Unknown && fact.class == class
            };
            let numeric = |fact: crate::region_ir::SsaValueFact| {
                fact.certainty != crate::region_ir::SsaCertainty::Unknown
                    && matches!(fact.class, SsaValueClass::Int | SsaValueClass::Float)
            };
            let continuation = instruction.continuation_id;
            match op {
                RegionBinaryOp::Add
                    if known(lhs_fact, SsaValueClass::ArrayHandle)
                        && known(rhs_fact, SsaValueClass::ArrayHandle) =>
                {
                    for operand in [lhs, rhs] {
                        let source = publication_entry_array_source(
                            region,
                            &definitions,
                            &parameter_indices,
                            &by_ref_parameters,
                            operand,
                            continuation,
                            "array union",
                        )?;
                        let requirement = admission.array_requirements.entry(source).or_default();
                        // The union owns one new stable array and retains every
                        // admitted key/value at most once. Reserving one
                        // projection for each source covers the capacity of
                        // their combined upper bound.
                        requirement.projection_allocations =
                            requirement.projection_allocations.saturating_add(1);
                        requirement.require_supported_keys = true;
                        requirement.require_plain_values = true;
                    }
                    entry_dependent_continuations.insert(continuation);
                }
                RegionBinaryOp::Add | RegionBinaryOp::Sub | RegionBinaryOp::Mul
                    if known(lhs_fact, SsaValueClass::Int)
                        && known(rhs_fact, SsaValueClass::Int)
                        && result_fact.integer_range.is_some() => {}
                RegionBinaryOp::Add | RegionBinaryOp::Sub | RegionBinaryOp::Mul
                    if numeric(lhs_fact)
                        && numeric(rhs_fact)
                        && matches!(
                            (lhs_fact.class, rhs_fact.class),
                            (SsaValueClass::Float, _) | (_, SsaValueClass::Float)
                        ) => {}
                RegionBinaryOp::Div if numeric(lhs_fact) && numeric(rhs_fact) => {
                    match rhs_fact.class {
                        SsaValueClass::Int => {
                            if !rhs_fact
                                .integer_range
                                .is_some_and(|range| range.excludes(0))
                            {
                                admit_publication_integer(
                                    &mut admission,
                                    value_flow,
                                    constants,
                                    &definitions,
                                    &parameter_indices,
                                    rhs,
                                    i64::MIN,
                                    i64::MAX,
                                    &[0],
                                    continuation,
                                    "division",
                                )?;
                                entry_dependent_continuations.insert(continuation);
                            }
                        }
                        SsaValueClass::Float => {
                            let fixed = match publication_root_operand(rhs, &definitions) {
                                RegionOperand::Constant(index)
                                | RegionOperand::LinkedConstant {
                                    constant: index, ..
                                } => match constants.get(index as usize) {
                                    Some(IrConstant::Float(value)) => Some(*value),
                                    _ => None,
                                },
                                _ => None,
                            };
                            if fixed == Some(0.0) {
                                return Err(CraneliftLoweringError::new(
                                    "JIT_CRANELIFT_REJECT_BINARY_DIVISOR_PUBLICATION",
                                    format!(
                                        "division at continuation {continuation} has a zero floating divisor"
                                    ),
                                ));
                            }
                            if fixed.is_none() {
                                let parameter_index = admit_publication_scalar_class(
                                    &mut admission,
                                    value_flow,
                                    constants,
                                    &definitions,
                                    &parameter_indices,
                                    rhs,
                                    SsaValueClass::Float,
                                    continuation,
                                    "division",
                                )?
                                .ok_or_else(|| {
                                    CraneliftLoweringError::new(
                                        "JIT_CRANELIFT_REJECT_BINARY_DIVISOR_PUBLICATION",
                                        format!(
                                            "division at continuation {continuation} has no entry-rooted floating divisor"
                                        ),
                                    )
                                })?;
                                admission
                                    .float_requirements
                                    .push(NativeEntryFloatRequirement {
                                        parameter_index,
                                        forbid_zero: true,
                                    });
                                entry_dependent_continuations.insert(continuation);
                            }
                        }
                        _ => unreachable!("numeric divisor class"),
                    }
                }
                RegionBinaryOp::Mod
                    if known(lhs_fact, SsaValueClass::Int)
                        && known(rhs_fact, SsaValueClass::Int) =>
                {
                    if !rhs_fact
                        .integer_range
                        .is_some_and(|range| range.excludes(0))
                    {
                        admit_publication_integer(
                            &mut admission,
                            value_flow,
                            constants,
                            &definitions,
                            &parameter_indices,
                            rhs,
                            i64::MIN,
                            i64::MAX,
                            &[0],
                            continuation,
                            "modulo",
                        )?;
                        entry_dependent_continuations.insert(continuation);
                    }
                }
                RegionBinaryOp::ShiftLeft | RegionBinaryOp::ShiftRight
                    if known(lhs_fact, SsaValueClass::Int)
                        && known(rhs_fact, SsaValueClass::Int) =>
                {
                    if !rhs_fact
                        .integer_range
                        .is_some_and(|range| range.minimum >= 0)
                    {
                        admit_publication_integer(
                            &mut admission,
                            value_flow,
                            constants,
                            &definitions,
                            &parameter_indices,
                            rhs,
                            0,
                            i64::MAX,
                            &[],
                            continuation,
                            "shift",
                        )?;
                        entry_dependent_continuations.insert(continuation);
                    }
                }
                RegionBinaryOp::BitAnd | RegionBinaryOp::BitOr | RegionBinaryOp::BitXor
                    if known(lhs_fact, SsaValueClass::Int)
                        && known(rhs_fact, SsaValueClass::Int) => {}
                RegionBinaryOp::BitAnd | RegionBinaryOp::BitOr | RegionBinaryOp::BitXor
                    if known(lhs_fact, SsaValueClass::StringHandle)
                        && known(rhs_fact, SsaValueClass::StringHandle) =>
                {
                    for operand in [lhs, rhs] {
                        admit_publication_string(
                            &mut admission,
                            value_flow,
                            constants,
                            &definitions,
                            &parameter_indices,
                            operand,
                            0,
                            1,
                            continuation,
                            "string bit operation",
                        )?;
                    }
                    entry_dependent_continuations.insert(continuation);
                }
                RegionBinaryOp::Concat => {
                    for (operand, fact) in [(lhs, lhs_fact), (rhs, rhs_fact)] {
                        match fact.class {
                            SsaValueClass::StringHandle
                                if fact.certainty != crate::region_ir::SsaCertainty::Unknown =>
                            {
                                admit_publication_string(
                                    &mut admission,
                                    value_flow,
                                    constants,
                                    &definitions,
                                    &parameter_indices,
                                    operand,
                                    0,
                                    1,
                                    continuation,
                                    "concatenation",
                                )?;
                                entry_dependent_continuations.insert(continuation);
                            }
                            SsaValueClass::Null
                            | SsaValueClass::Bool
                            | SsaValueClass::Int
                            | SsaValueClass::Float
                            | SsaValueClass::ResourceHandle
                                if fact.certainty != crate::region_ir::SsaCertainty::Unknown =>
                            {
                                // Integer, float, and resource formatting is
                                // bounded by the fixed native scalar buffers.
                                admission.fixed_string_bytes =
                                    admission.fixed_string_bytes.saturating_add(
                                        php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY,
                                    );
                            }
                            _ => {
                                return Err(CraneliftLoweringError::new(
                                    "JIT_CRANELIFT_REJECT_BINARY_CONCAT_PUBLICATION",
                                    format!(
                                        "concatenation at continuation {continuation} has no total scalar/string publication plan"
                                    ),
                                ));
                            }
                        }
                    }
                }
                RegionBinaryOp::Pow if numeric(lhs_fact) && numeric(rhs_fact) => {}
                _ => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_BINARY_PUBLICATION",
                        format!(
                            "binary {op:?} at continuation {continuation} has no total native type/shape plan"
                        ),
                    ));
                }
            }
            admission.total_binary_instructions.insert(continuation);
        }
        if let RegionInstructionKind::Echo { src } = instruction.kind {
            let fact = lowering_operand_fact(value_flow, constants, src);
            if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || !matches!(
                    fact.class,
                    SsaValueClass::Int
                        | SsaValueClass::Float
                        | SsaValueClass::StringHandle
                        | SsaValueClass::Bool
                        | SsaValueClass::Null
                )
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ECHO_PUBLICATION",
                    format!(
                        "echo at continuation {} has no total scalar string plan",
                        instruction.continuation_id,
                    ),
                ));
            }
            if let Some(NativeEntryArraySource::Parameter(parameter_index)) =
                entry_array_source(src, &definitions, &parameter_indices)
            {
                admission
                    .value_class_requirements
                    .push(NativeEntryValueClassRequirement {
                        parameter_index,
                        class: fact.class,
                    });
                entry_dependent_continuations.insert(instruction.continuation_id);
            }
        }
        match instruction.kind {
            RegionInstructionKind::Compare { lhs, rhs, .. } => {
                for operand in [lhs, rhs] {
                    let fact = lowering_operand_fact(value_flow, constants, operand);
                    if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || !matches!(
                            fact.class,
                            SsaValueClass::Null
                                | SsaValueClass::Bool
                                | SsaValueClass::Int
                                | SsaValueClass::Float
                                | SsaValueClass::StringHandle
                                | SsaValueClass::ResourceHandle
                        )
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_COMPARE_PUBLICATION",
                            format!(
                                "comparison at continuation {} has no total scalar/resource operand plan",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    let guarded = admit_publication_scalar_class(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        operand,
                        fact.class,
                        instruction.continuation_id,
                        "comparison",
                    )?;
                    entry_dependent_continuations
                        .extend(guarded.map(|_| instruction.continuation_id));
                }
                admission
                    .total_scalar_control_instructions
                    .insert(instruction.continuation_id);
            }
            RegionInstructionKind::Unary { op, src, .. } => {
                let fact = lowering_operand_fact(value_flow, constants, src);
                let admitted = match op {
                    RegionUnaryOp::Plus | RegionUnaryOp::Minus => {
                        matches!(fact.class, SsaValueClass::Int | SsaValueClass::Float)
                    }
                    RegionUnaryOp::BitNot => {
                        matches!(fact.class, SsaValueClass::Int | SsaValueClass::StringHandle)
                    }
                    RegionUnaryOp::Not => !matches!(
                        fact.class,
                        SsaValueClass::Uninitialized
                            | SsaValueClass::ReferenceHandle
                            | SsaValueClass::MixedHandle
                    ),
                };
                if fact.certainty == crate::region_ir::SsaCertainty::Unknown || !admitted {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_UNARY_PUBLICATION",
                        format!(
                            "unary {op:?} at continuation {} has no total native operand plan",
                            instruction.continuation_id,
                        ),
                    ));
                }
                if op == RegionUnaryOp::BitNot && fact.class == SsaValueClass::StringHandle {
                    admit_publication_string(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        src,
                        0,
                        1,
                        instruction.continuation_id,
                        "string bit-not",
                    )?;
                    entry_dependent_continuations.insert(instruction.continuation_id);
                } else {
                    let guarded = admit_publication_scalar_class(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        src,
                        fact.class,
                        instruction.continuation_id,
                        "unary operation",
                    )?;
                    entry_dependent_continuations
                        .extend(guarded.map(|_| instruction.continuation_id));
                }
                admission
                    .total_scalar_control_instructions
                    .insert(instruction.continuation_id);
            }
            RegionInstructionKind::Cast { op, src, .. } => {
                let fact = lowering_operand_fact(value_flow, constants, src);
                let admitted = match op {
                    RegionCastOp::Bool => !matches!(
                        fact.class,
                        SsaValueClass::Uninitialized
                            | SsaValueClass::ReferenceHandle
                            | SsaValueClass::MixedHandle
                    ),
                    RegionCastOp::Int | RegionCastOp::Float => matches!(
                        fact.class,
                        SsaValueClass::Null
                            | SsaValueClass::Bool
                            | SsaValueClass::Int
                            | SsaValueClass::Float
                            | SsaValueClass::StringHandle
                            | SsaValueClass::ArrayHandle
                            | SsaValueClass::ResourceHandle
                    ),
                    RegionCastOp::String => matches!(
                        fact.class,
                        SsaValueClass::Null
                            | SsaValueClass::Bool
                            | SsaValueClass::Int
                            | SsaValueClass::Float
                            | SsaValueClass::StringHandle
                            | SsaValueClass::ResourceHandle
                    ),
                    RegionCastOp::Array => matches!(
                        fact.class,
                        SsaValueClass::Null
                            | SsaValueClass::Bool
                            | SsaValueClass::Int
                            | SsaValueClass::Float
                            | SsaValueClass::StringHandle
                            | SsaValueClass::ArrayHandle
                            | SsaValueClass::ResourceHandle
                    ),
                    // Object conversion allocates and may expose array/property
                    // visibility. Until that full object publication plan is
                    // available, only the identity-preserving form enters the
                    // optimizing region.
                    RegionCastOp::Object => fact.class == SsaValueClass::ObjectHandle,
                    RegionCastOp::Void => true,
                };
                if fact.certainty == crate::region_ir::SsaCertainty::Unknown || !admitted {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CAST_PUBLICATION",
                        format!(
                            "cast {op:?} at continuation {} has no total native operand plan",
                            instruction.continuation_id,
                        ),
                    ));
                }
                if op == RegionCastOp::Array && fact.class != SsaValueClass::ArrayHandle {
                    admission.fixed_array_entries = admission
                        .fixed_array_entries
                        .saturating_add(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize);
                    admission.fixed_value_allocations =
                        admission.fixed_value_allocations.saturating_add(1);
                }
                if op == RegionCastOp::String && fact.class != SsaValueClass::StringHandle {
                    admission.fixed_string_bytes = admission
                        .fixed_string_bytes
                        .saturating_add(php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY);
                }
                let guarded = admit_publication_scalar_class(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    src,
                    fact.class,
                    instruction.continuation_id,
                    "cast",
                )?;
                entry_dependent_continuations.extend(guarded.map(|_| instruction.continuation_id));
                admission
                    .total_scalar_control_instructions
                    .insert(instruction.continuation_id);
            }
            _ => {}
        }
        if let RegionInstructionKind::EmptyLocal { local, .. } = instruction.kind {
            let operand = RegionOperand::Local(local);
            let fact = lowering_operand_fact(value_flow, constants, operand);
            if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || matches!(
                    fact.class,
                    SsaValueClass::Uninitialized
                        | SsaValueClass::ReferenceHandle
                        | SsaValueClass::MixedHandle
                )
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_TRUTHINESS_PUBLICATION",
                    format!(
                        "empty-local at continuation {} has no exact truthiness shape",
                        instruction.continuation_id,
                    ),
                ));
            }
            let guarded = admit_publication_scalar_class(
                &mut admission,
                value_flow,
                constants,
                &definitions,
                &parameter_indices,
                operand,
                fact.class,
                instruction.continuation_id,
                "empty-local truthiness",
            )?;
            entry_dependent_continuations.extend(guarded.map(|_| instruction.continuation_id));
            admission
                .total_scalar_control_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::UnsetLocal { local } = instruction.kind {
            let storage = value_flow.local_storage(local);
            if storage == crate::region_ir::LocalStorageClass::Superglobal
                || region.flags.is_top_level
                    && storage == crate::region_ir::LocalStorageClass::RequestGlobal
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_GLOBAL_BINDING_UNSET",
                    format!(
                        "global-binding unset at continuation {} is assigned to baseline before region entry",
                        instruction.continuation_id,
                    ),
                ));
            }
        }
        if matches!(
            &instruction.kind,
            RegionInstructionKind::NativeDynamicCode(RegionNativeDynamicCode::Include { .. })
        ) {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_INCLUDE_PUBLICATION",
                format!(
                    "include at continuation {} is assigned to baseline before region entry",
                    instruction.continuation_id,
                ),
            ));
        }
        if matches!(
            &instruction.kind,
            RegionInstructionKind::FetchConst { .. }
                | RegionInstructionKind::NativeCall(RegionNativeCall {
                    target: RegionCallTarget::Semantic {
                        operation: RegionSemanticOp::ClassConstantFetch { .. },
                    },
                    ..
                })
        ) {
            admission
                .trusted_constant_requirements
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
        }
        if matches!(
            &instruction.kind,
            RegionInstructionKind::InitStaticLocal { .. }
        ) {
            admission
                .trusted_static_local_requirements
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::NewArray { .. } = instruction.kind {
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .fresh_array_instructions
                .insert(instruction.continuation_id);
            admission.fixed_value_allocations = admission.fixed_value_allocations.saturating_add(1);
            admission.fixed_array_entries = admission
                .fixed_array_entries
                .saturating_add(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize);
        }
        if let RegionInstructionKind::ArrayInsert {
            array,
            key,
            value,
            by_ref_local,
        } = instruction.kind
        {
            if !new_array_registers.contains(&array) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_LITERAL_ROOT",
                    format!(
                        "array insertion at continuation {} is not rooted at a fresh native array",
                        instruction.continuation_id,
                    ),
                ));
            }
            if fresh_spread_targets.contains(&array) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_SPREAD_TARGET",
                    format!(
                        "array insertion at continuation {} follows a publication-planned spread",
                        instruction.continuation_id,
                    ),
                ));
            }
            if by_ref_local.is_some() {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_LITERAL_REFERENCE",
                    format!(
                        "array insertion at continuation {} requires a reference-owner plan",
                        instruction.continuation_id,
                    ),
                ));
            }
            let value_fact = lowering_operand_fact(value_flow, constants, value);
            if value_fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || value_fact.class == SsaValueClass::ReferenceHandle
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_LITERAL_VALUE",
                    format!(
                        "array insertion at continuation {} has no plain authoritative value",
                        instruction.continuation_id,
                    ),
                ));
            }
            let append = key.is_none();
            if fresh_insert_modes
                .insert(array, append)
                .is_some_and(|previous| previous != append)
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_LITERAL_MIXED_KEYS",
                    format!(
                        "array literal at continuation {} mixes append and explicit-key plans",
                        instruction.continuation_id,
                    ),
                ));
            }
            if let Some(key) = key {
                let normalized = publication_native_array_key(constants, key).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_LITERAL_KEY",
                        format!(
                            "array insertion at continuation {} has no publication-normalized key",
                            instruction.continuation_id,
                        ),
                    )
                })?;
                if matches!(normalized, NativeEntryArrayKey::Integer(i64::MAX))
                    || !fresh_insert_keys
                        .entry(array)
                        .or_default()
                        .insert(normalized)
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_LITERAL_DUPLICATE_KEY",
                        format!(
                            "array insertion at continuation {} can overwrite or overflow a fresh key",
                            instruction.continuation_id,
                        ),
                    ));
                }
            }
            let count = fresh_insert_counts.entry(array).or_default();
            *count = count.saturating_add(1);
            if *count > crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_LITERAL_CAPACITY",
                    format!(
                        "array literal at continuation {} exceeds its publication-reserved native capacity",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .fresh_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::ArraySpread { array, source } = instruction.kind {
            if !new_array_registers.contains(&array)
                || fresh_insert_counts.get(&array).copied().unwrap_or(0) != 0
                || !fresh_spread_targets.insert(array)
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_SPREAD_TARGET",
                    format!(
                        "array spread at continuation {} is not the sole mutation of a fresh target",
                        instruction.continuation_id,
                    ),
                ));
            }
            let source =
                entry_array_source(source, &definitions, &parameter_indices).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_SPREAD_SOURCE",
                        format!(
                            "array spread at continuation {} is not rooted at an entry array",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let NativeEntryArraySource::Parameter(source_index) = source else {
                unreachable!("array spread source operands are entry parameters")
            };
            if by_ref_parameters.contains(&region.parameter_locals[source_index]) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_SPREAD_REFERENCE",
                    format!(
                        "array spread at continuation {} receives a by-reference source",
                        instruction.continuation_id,
                    ),
                ));
            }
            let requirement = admission.array_requirements.entry(source).or_default();
            requirement.spread_allocations = requirement.spread_allocations.saturating_add(1);
            requirement.require_supported_keys = true;
            requirement.require_plain_values = true;
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .fresh_array_instructions
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::ForeachInit { iterator, source } = instruction.kind {
            let source =
                entry_array_source(source, &definitions, &parameter_indices).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_FOREACH_SOURCE",
                        format!(
                            "foreach init at continuation {} is not rooted at an entry array",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let NativeEntryArraySource::Parameter(source_index) = source else {
                unreachable!("foreach source operands are entry parameters")
            };
            if by_ref_parameters.contains(&region.parameter_locals[source_index]) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FOREACH_REFERENCE_SOURCE",
                    format!(
                        "foreach init at continuation {} receives a by-reference array",
                        instruction.continuation_id,
                    ),
                ));
            }
            let requirement = admission.array_requirements.entry(source).or_default();
            requirement.require_supported_keys = true;
            requirement.require_plain_values = true;
            admission.fixed_value_allocations = admission.fixed_value_allocations.saturating_add(1);
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            debug_assert!(foreach_sources.contains_key(&iterator));
        }
        if let RegionInstructionKind::ForeachNext { iterator, .. } = instruction.kind {
            if !foreach_sources.contains_key(&iterator) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FOREACH_ITERATOR",
                    format!(
                        "foreach continuation {} has no publication-planned native iterator",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::ForeachCleanup { iterator } = instruction.kind {
            if !foreach_sources.contains_key(&iterator)
                && !foreach_reference_sources.contains_key(&iterator)
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FOREACH_ITERATOR",
                    format!(
                        "foreach cleanup {} has no publication-planned native iterator",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::ForeachInitRef { iterator, local } = instruction.kind {
            let source = new_array_local_sources.get(&local).copied().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FOREACH_REFERENCE_PUBLICATION",
                    format!(
                        "foreach-by-reference at continuation {} is not rooted at a fresh native array local",
                        instruction.continuation_id,
                    ),
                )
            })?;
            let storage = value_flow.local_storage(local);
            if !(storage == crate::region_ir::LocalStorageClass::SsaPlain
                || storage.is_reference_slot())
                || array_local_mutation_counts
                    .get(&local)
                    .copied()
                    .unwrap_or(0)
                    != 0
                || fresh_spread_targets.contains(&source)
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FOREACH_REFERENCE_OWNERSHIP",
                    format!(
                        "foreach-by-reference at continuation {} has no exclusive fresh-array owner",
                        instruction.continuation_id,
                    ),
                ));
            }
            let entry_count = fresh_insert_counts.get(&source).copied().unwrap_or(0);
            admission.fixed_value_allocations = admission
                .fixed_value_allocations
                .saturating_add(entry_count.saturating_add(2));
            if storage.is_reference_slot() {
                admission.total_reference_locals.insert(local);
                admission.total_array_instructions.insert(
                    new_array_local_store_continuations
                        .get(&local)
                        .copied()
                        .expect("fresh foreach reference local has its defining store"),
                );
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            debug_assert_eq!(foreach_reference_sources.get(&iterator), Some(&local));
        }
        if let RegionInstructionKind::ForeachNextRef {
            iterator,
            value_local,
            ..
        } = instruction.kind
        {
            if !foreach_reference_sources.contains_key(&iterator)
                || !value_flow.local_storage(value_local).is_reference_slot()
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FOREACH_REFERENCE_ITERATOR",
                    format!(
                        "foreach-by-reference continuation {} has no publication-owned iterator/value slot",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission.total_reference_locals.insert(value_local);
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .total_array_instructions
                .extend(foreach_reference_local_writes.iter().copied());
        }
        if let RegionInstructionKind::AppendDim {
            local,
            ref keys,
            value,
            ..
        } = instruction.kind
            && instruction.native_global_name.is_none()
            && keys.is_empty()
            && new_array_locals.contains(&local)
        {
            let fact = lowering_operand_fact(value_flow, constants, value);
            if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || fact.class == SsaValueClass::ReferenceHandle
                || value_flow.local_storage(local) != crate::region_ir::LocalStorageClass::SsaPlain
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_APPEND_OWNERSHIP",
                    format!(
                        "array append at continuation {} has no total native value/local ownership plan",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .fresh_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::AppendDim {
            local,
            ref keys,
            value,
            ..
        } = instruction.kind
            && instruction.native_global_name.is_none()
            && !keys.is_empty()
            && new_array_locals.contains(&local)
        {
            if new_array_mutation_counts.get(&local).copied() != Some(1)
                || keys
                    .iter()
                    .any(|key| publication_integer_array_key(constants, *key).is_none())
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NESTED_ARRAY_APPEND_SHAPE",
                    format!(
                        "nested append at continuation {} is not a single normalized fresh-array path",
                        instruction.continuation_id,
                    ),
                ));
            }
            let fact = lowering_operand_fact(value_flow, constants, value);
            if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || fact.class == SsaValueClass::ReferenceHandle
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NESTED_ARRAY_APPEND_OWNERSHIP",
                    format!(
                        "nested append at continuation {} has no total value ownership plan",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission.fixed_value_allocations =
                admission.fixed_value_allocations.saturating_add(keys.len());
            admission.fixed_array_entries = admission.fixed_array_entries.saturating_add(
                keys.len()
                    .saturating_mul(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize),
            );
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .fresh_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::AssignDim {
            local,
            ref keys,
            value,
            ..
        } = instruction.kind
            && instruction.native_global_name.is_none()
            && keys.len() == 1
            && new_array_locals.contains(&local)
        {
            if publication_integer_array_key(constants, keys[0]).is_none()
                || new_array_mutation_counts.get(&local).copied().unwrap_or(0)
                    > crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_ASSIGN_SHAPE",
                    format!(
                        "array assignment at continuation {} has no bounded normalized native entry plan",
                        instruction.continuation_id,
                    ),
                ));
            }
            let fact = lowering_operand_fact(value_flow, constants, value);
            if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || !matches!(
                    fact.class,
                    SsaValueClass::Int | SsaValueClass::Bool | SsaValueClass::Null
                )
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_ASSIGN_OWNERSHIP",
                    format!(
                        "array assignment at continuation {} has no immediate native ownership plan",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .fresh_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::AssignDim {
            local,
            ref keys,
            value,
            ..
        } = instruction.kind
            && instruction.native_global_name.is_none()
            && keys.len() > 1
            && new_array_locals.contains(&local)
        {
            if new_array_mutation_counts.get(&local).copied() != Some(1)
                || keys
                    .iter()
                    .any(|key| publication_integer_array_key(constants, *key).is_none())
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NESTED_ARRAY_ASSIGN_SHAPE",
                    format!(
                        "nested assignment at continuation {} is not a single normalized fresh-array path",
                        instruction.continuation_id,
                    ),
                ));
            }
            let fact = lowering_operand_fact(value_flow, constants, value);
            if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || !matches!(
                    fact.class,
                    SsaValueClass::Int | SsaValueClass::Bool | SsaValueClass::Null
                )
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NESTED_ARRAY_ASSIGN_OWNERSHIP",
                    format!(
                        "nested assignment at continuation {} has no immediate native ownership plan",
                        instruction.continuation_id,
                    ),
                ));
            }
            let child_count = keys.len().saturating_sub(1);
            admission.fixed_value_allocations = admission
                .fixed_value_allocations
                .saturating_add(child_count);
            admission.fixed_array_entries = admission.fixed_array_entries.saturating_add(
                child_count
                    .saturating_mul(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize),
            );
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .fresh_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::UnsetDim { local, ref keys } = instruction.kind
            && instruction.native_global_name.is_none()
            && keys.len() == 1
            && new_array_locals.contains(&local)
        {
            if publication_integer_array_key(constants, keys[0]).is_none() {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_UNSET_SHAPE",
                    format!(
                        "array unset at continuation {} has no normalized native entry plan",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .fresh_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::UnsetDim { local, ref keys } = instruction.kind
            && instruction.native_global_name.is_none()
            && keys.len() > 1
            && new_array_locals.contains(&local)
        {
            if new_array_mutation_counts.get(&local).copied() != Some(1)
                || keys
                    .iter()
                    .any(|key| publication_integer_array_key(constants, *key).is_none())
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NESTED_ARRAY_UNSET_SHAPE",
                    format!(
                        "nested unset at continuation {} is not a silent fresh-array path",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            admission
                .fresh_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::AssignDim {
            local,
            ref keys,
            value,
            ..
        } = instruction.kind
            && instruction.native_global_name.is_none()
            && !new_array_locals.contains(&local)
        {
            let source_index = parameter_indices.get(&local).copied().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_ASSIGN_ROOT",
                    format!(
                        "array assignment at continuation {} is not rooted at an entry parameter",
                        instruction.continuation_id,
                    ),
                )
            })?;
            if keys.is_empty()
                || by_ref_parameters.contains(&local)
                || value_flow.local_storage(local) != crate::region_ir::LocalStorageClass::SsaPlain
                || array_local_mutation_counts.get(&local).copied() != Some(1)
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_ASSIGN_LVALUE",
                    format!(
                        "array assignment at continuation {} has no single direct entry lvalue",
                        instruction.continuation_id,
                    ),
                ));
            }
            let normalized = keys
                .iter()
                .map(|key| publication_integer_array_key(constants, *key))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_ASSIGN_KEY_NORMALIZATION",
                        format!(
                            "array assignment at continuation {} has an unnormalized key path",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let fact = lowering_operand_fact(value_flow, constants, value);
            if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || !matches!(
                    fact.class,
                    SsaValueClass::Int | SsaValueClass::Bool | SsaValueClass::Null
                )
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_ASSIGN_OWNERSHIP",
                    format!(
                        "array assignment at continuation {} has no immediate value ownership",
                        instruction.continuation_id,
                    ),
                ));
            }
            let (&key, parents) = normalized
                .split_last()
                .expect("nonempty assignment path was admitted");
            admission
                .array_requirements
                .entry(NativeEntryArraySource::Parameter(source_index))
                .or_default()
                .mutations
                .push(NativeEntryArrayMutationRequirement::Assign {
                    parents: parents.to_vec(),
                    key,
                });
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::AppendDim {
            local,
            ref keys,
            value,
            ..
        } = instruction.kind
            && instruction.native_global_name.is_none()
            && !new_array_locals.contains(&local)
        {
            let source_index = parameter_indices.get(&local).copied().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_APPEND_ROOT",
                    format!(
                        "array append at continuation {} is not rooted at an entry parameter",
                        instruction.continuation_id,
                    ),
                )
            })?;
            if by_ref_parameters.contains(&local)
                || value_flow.local_storage(local) != crate::region_ir::LocalStorageClass::SsaPlain
                || array_local_mutation_counts.get(&local).copied() != Some(1)
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_APPEND_LVALUE",
                    format!(
                        "array append at continuation {} has no single direct entry lvalue",
                        instruction.continuation_id,
                    ),
                ));
            }
            let parents = keys
                .iter()
                .map(|key| publication_integer_array_key(constants, *key))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_APPEND_KEY_NORMALIZATION",
                        format!(
                            "array append at continuation {} has an unnormalized key path",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let fact = lowering_operand_fact(value_flow, constants, value);
            if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || fact.class == SsaValueClass::ReferenceHandle
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_APPEND_OWNERSHIP",
                    format!(
                        "array append at continuation {} has no total value ownership",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .array_requirements
                .entry(NativeEntryArraySource::Parameter(source_index))
                .or_default()
                .mutations
                .push(NativeEntryArrayMutationRequirement::Append { parents });
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::UnsetDim { local, ref keys } = instruction.kind
            && instruction.native_global_name.is_none()
            && !new_array_locals.contains(&local)
        {
            let source_index = parameter_indices.get(&local).copied().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_UNSET_ROOT",
                    format!(
                        "array unset at continuation {} is not rooted at an entry parameter",
                        instruction.continuation_id,
                    ),
                )
            })?;
            if keys.is_empty()
                || by_ref_parameters.contains(&local)
                || value_flow.local_storage(local) != crate::region_ir::LocalStorageClass::SsaPlain
                || array_local_mutation_counts.get(&local).copied() != Some(1)
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_UNSET_LVALUE",
                    format!(
                        "array unset at continuation {} has no single direct entry lvalue",
                        instruction.continuation_id,
                    ),
                ));
            }
            let normalized = keys
                .iter()
                .map(|key| publication_integer_array_key(constants, *key))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_UNSET_KEY_NORMALIZATION",
                        format!(
                            "array unset at continuation {} has an unnormalized key path",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let (&key, parents) = normalized
                .split_last()
                .expect("nonempty unset path was admitted");
            admission
                .array_requirements
                .entry(NativeEntryArraySource::Parameter(source_index))
                .or_default()
                .mutations
                .push(NativeEntryArrayMutationRequirement::Unset {
                    parents: parents.to_vec(),
                    key,
                });
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::UnsetDim { local, ref keys } = instruction.kind
            && instruction.native_global_name.is_none()
            && keys.len() > 1
            && new_array_locals.contains(&local)
        {
            if new_array_mutation_counts.get(&local).copied() != Some(1)
                || keys
                    .iter()
                    .any(|key| publication_integer_array_key(constants, *key).is_none())
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_NESTED_ARRAY_UNSET_SHAPE",
                    format!(
                        "nested unset at continuation {} is not a silent fresh-array path",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
        }
        if matches!(
            instruction.kind,
            RegionInstructionKind::NativeDynamicCode(_)
        ) {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NON_TOTAL_OPTIMIZING_REGION",
                format!(
                    "dynamic-code continuation {} must enter the baseline tier before optimizing execution",
                    instruction.continuation_id,
                ),
            ));
        }
        if let RegionInstructionKind::StoreLocal { local, .. }
        | RegionInstructionKind::AssignLocalResult { local, .. } = instruction.kind
            && matches!(
                value_flow.local_storage(local),
                crate::region_ir::LocalStorageClass::RequestGlobal
                    | crate::region_ir::LocalStorageClass::Superglobal
            )
        {
            if invalidated_locals.contains(&local)
                || request_local_write_counts.get(&local).copied() != Some(1)
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_REQUEST_LOCAL_COW_SHAPE",
                    format!(
                        "request local {} does not have one entry-preflightable native write",
                        local.raw(),
                    ),
                ));
            }
            admission
                .total_request_local_stores
                .insert(instruction.continuation_id);
            admission.releasable_request_locals.insert(local);
        }
        if let RegionInstructionKind::LoadLocal {
            local,
            quiet: false,
            ..
        } = instruction.kind
        {
            let storage = value_flow.local_storage(local);
            if storage == crate::region_ir::LocalStorageClass::SsaPlain {
                admission
                    .total_local_loads
                    .insert(instruction.continuation_id);
            } else if matches!(
                storage,
                crate::region_ir::LocalStorageClass::RequestGlobal
                    | crate::region_ir::LocalStorageClass::Superglobal
            ) {
                if invalidated_locals.contains(&local) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_REQUEST_LOCAL_LVALUE_SHAPE",
                        format!(
                            "request local {} may be detached before non-quiet load at continuation {}",
                            local.raw(),
                            instruction.continuation_id,
                        ),
                    ));
                }
                admission
                    .total_local_loads
                    .insert(instruction.continuation_id);
                admission.initialized_request_locals.insert(local);
            } else if instruction.live_locals.contains(&local) {
                admission
                    .total_local_loads
                    .insert(instruction.continuation_id);
            } else {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_LOCAL_WARNING_BOUNDARY",
                    format!(
                        "non-quiet local load at continuation {} is not definitely initialized before optimizing entry",
                        instruction.continuation_id,
                    ),
                ));
            }
        }
        if let RegionInstructionKind::FetchDim {
            dst,
            array,
            key,
            quiet,
            mode,
            ..
        } = instruction.kind
            && instruction.native_global_name.is_none()
        {
            let source = entry_array_source(array, &definitions, &parameter_indices);
            if source.is_none()
                && let Some(local) = entry_array_root_local(array, &definitions)
                && new_array_locals.contains(&local)
            {
                let key = publication_integer_array_key(constants, key).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_DIM_KEY_NORMALIZATION",
                        format!(
                            "dimension read at continuation {} has no publication-normalized integer key",
                            instruction.continuation_id,
                        ),
                    )
                })?;
                if !quiet
                    && mode == php_ir::instruction::DimFetchMode::Read
                    && !new_array_assigned_integer_keys
                        .get(&local)
                        .is_some_and(|keys| keys.contains(&key))
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_DIM_WARNING_BOUNDARY",
                        format!(
                            "dimension read at continuation {} may warn after native entry",
                            instruction.continuation_id,
                        ),
                    ));
                }
                admission
                    .total_array_instructions
                    .insert(instruction.continuation_id);
                continue;
            }
            let source = source.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_DIM_SHAPE",
                    format!(
                        "dimension read at continuation {} is not rooted at an entry array",
                        instruction.continuation_id,
                    ),
                )
            })?;
            let array_fact = lowering_operand_fact(value_flow, constants, array);
            let NativeEntryArraySource::Parameter(source_index) = source else {
                unreachable!("operand-rooted entry arrays are parameters")
            };
            let source_local = region.parameter_locals[source_index];
            if !variadic_parameters.contains(&source_local)
                && (array_fact.certainty == crate::region_ir::SsaCertainty::Unknown
                    || array_fact.class != SsaValueClass::ArrayHandle)
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_DIM_TYPE",
                    format!(
                        "dimension read at continuation {} has no compile-time array fact",
                        instruction.continuation_id,
                    ),
                ));
            }
            let key = match key {
                RegionOperand::Constant(index) => match constants.get(index as usize) {
                    Some(IrConstant::Int(value)) if native_integer_fits_immediate(*value) => *value,
                    _ => {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_ARRAY_DIM_KEY_NORMALIZATION",
                            format!(
                                "dimension read at continuation {} has no publication-normalized integer key",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                },
                RegionOperand::I64(value) if native_integer_fits_immediate(value) => value,
                _ => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_DIM_KEY_NORMALIZATION",
                        format!(
                            "dimension read at continuation {} has no publication-normalized integer key",
                            instruction.continuation_id,
                        ),
                    ));
                }
            };
            if by_ref_parameters.contains(&source_local) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_DIM_REFERENCE",
                    format!(
                        "dimension read at continuation {} is rooted at a by-reference parameter",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            admitted_array_fetches.insert(dst, (source, key));
            let requirement = admission.array_requirements.entry(source).or_default();
            if !quiet && mode == php_ir::instruction::DimFetchMode::Read {
                requirement.required_integer_keys.insert(key);
            }
        }
        if let RegionInstructionKind::IssetDim { local, keys, .. }
        | RegionInstructionKind::EmptyDim { local, keys, .. } = &instruction.kind
        {
            if keys.is_empty() {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_PROBE_PATH",
                    format!(
                        "array probe at continuation {} has no dimension",
                        instruction.continuation_id,
                    ),
                ));
            }
            let (source, dimensions) = if instruction.native_global_name.is_some() {
                admission.plain_globals.insert(instruction.continuation_id);
                (
                    NativeEntryArraySource::TrustedGlobal(instruction.continuation_id),
                    &keys[1..],
                )
            } else {
                let parameter_index = parameter_indices.get(local).copied().ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_PROBE_ROOT",
                        format!(
                            "array probe at continuation {} is not rooted at an entry parameter",
                            instruction.continuation_id,
                        ),
                    )
                })?;
                if by_ref_parameters.contains(local)
                    || value_flow.local_storage(*local)
                        != crate::region_ir::LocalStorageClass::SsaPlain
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_PROBE_REFERENCE",
                        format!(
                            "array probe at continuation {} has no plain entry owner",
                            instruction.continuation_id,
                        ),
                    ));
                }
                (
                    NativeEntryArraySource::Parameter(parameter_index),
                    keys.as_slice(),
                )
            };
            let normalized = dimensions
                .iter()
                .copied()
                .map(|key| publication_native_array_key(constants, key))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_PROBE_KEY_NORMALIZATION",
                        format!(
                            "array probe at continuation {} has a key that is not normalized at publication",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            if normalized.is_empty() {
                // A global `isset($name)`/`empty($name)` tests the already
                // published reference payload itself and needs no array plan.
            } else {
                admission
                    .array_requirements
                    .entry(source)
                    .or_default()
                    .probe_paths
                    .push(NativeEntryArrayProbeRequirement {
                        keys: normalized,
                        leaf: NativeEntryArrayProbeLeaf::PlainValue,
                    });
            }
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
        }
        if instruction.native_global_name.is_none()
            && matches!(
                instruction.kind,
                RegionInstructionKind::FetchDim { .. }
                    | RegionInstructionKind::AssignDim { .. }
                    | RegionInstructionKind::AppendDim { .. }
                    | RegionInstructionKind::UnsetDim { .. }
                    | RegionInstructionKind::IssetDim { .. }
                    | RegionInstructionKind::EmptyDim { .. }
            )
            && !admission
                .total_array_instructions
                .contains(&instruction.continuation_id)
        {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_UNCLASSIFIED_ARRAY_DIMENSION",
                format!(
                    "array dimension continuation {} has no complete publication-time shape, COW, and ownership plan",
                    instruction.continuation_id,
                ),
            ));
        }
        if instruction.native_global_name.is_none()
            && matches!(
                instruction.kind,
                RegionInstructionKind::AssignDim { local, .. }
                    | RegionInstructionKind::AppendDim { local, .. }
                    | RegionInstructionKind::UnsetDim { local, .. }
                    if !new_array_locals.contains(&local)
            )
        {
            entry_dependent_continuations.insert(instruction.continuation_id);
        }
        if instruction.native_global_name.is_some() {
            let source = NativeEntryArraySource::TrustedGlobal(instruction.continuation_id);
            match &instruction.kind {
                RegionInstructionKind::FetchDim {
                    quiet,
                    mode: php_ir::instruction::DimFetchMode::Read,
                    ..
                } => {
                    admission.plain_globals.insert(instruction.continuation_id);
                    if !quiet {
                        admission
                            .initialized_globals
                            .insert(instruction.continuation_id);
                    }
                    admission
                        .total_array_instructions
                        .insert(instruction.continuation_id);
                }
                RegionInstructionKind::AssignDim { keys, value, .. } if keys.len() == 1 => {
                    let fact = lowering_operand_fact(value_flow, constants, *value);
                    if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || !matches!(
                            fact.class,
                            SsaValueClass::Int | SsaValueClass::Bool | SsaValueClass::Null
                        )
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_GLOBAL_ASSIGN_OWNERSHIP",
                            format!(
                                "global assignment at continuation {} has no immediate ownership plan",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    admission
                        .releasable_globals
                        .insert(instruction.continuation_id);
                    admission.plain_globals.insert(instruction.continuation_id);
                    admission
                        .total_array_instructions
                        .insert(instruction.continuation_id);
                }
                RegionInstructionKind::AssignDim { keys, value, .. } if keys.len() > 1 => {
                    let normalized = keys[1..]
                        .iter()
                        .map(|key| publication_integer_array_key(constants, *key))
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_GLOBAL_ARRAY_KEY",
                                format!(
                                    "global array assignment at continuation {} has an unnormalized key path",
                                    instruction.continuation_id,
                                ),
                            )
                        })?;
                    let fact = lowering_operand_fact(value_flow, constants, *value);
                    if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || !matches!(
                            fact.class,
                            SsaValueClass::Int | SsaValueClass::Bool | SsaValueClass::Null
                        )
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_GLOBAL_ARRAY_OWNERSHIP",
                            format!(
                                "global array assignment at continuation {} has no immediate ownership plan",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    let (&key, parents) = normalized
                        .split_last()
                        .expect("nested global assignment retains a child key");
                    admission
                        .array_requirements
                        .entry(source)
                        .or_default()
                        .mutations
                        .push(NativeEntryArrayMutationRequirement::Assign {
                            parents: parents.to_vec(),
                            key,
                        });
                    admission
                        .total_array_instructions
                        .insert(instruction.continuation_id);
                }
                RegionInstructionKind::AppendDim { keys, value, .. } if !keys.is_empty() => {
                    let parents = keys[1..]
                        .iter()
                        .map(|key| publication_integer_array_key(constants, *key))
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_GLOBAL_ARRAY_KEY",
                                format!(
                                    "global array append at continuation {} has an unnormalized key path",
                                    instruction.continuation_id,
                                ),
                            )
                        })?;
                    let fact = lowering_operand_fact(value_flow, constants, *value);
                    if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || fact.class == SsaValueClass::ReferenceHandle
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_GLOBAL_ARRAY_OWNERSHIP",
                            format!(
                                "global array append at continuation {} has no total ownership plan",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    admission
                        .array_requirements
                        .entry(source)
                        .or_default()
                        .mutations
                        .push(NativeEntryArrayMutationRequirement::Append { parents });
                    admission
                        .total_array_instructions
                        .insert(instruction.continuation_id);
                }
                RegionInstructionKind::UnsetDim { keys, .. } if keys.len() == 1 => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_GLOBAL_BINDING_UNSET",
                        format!(
                            "global binding unset at continuation {} must enter baseline before optimizing execution",
                            instruction.continuation_id,
                        ),
                    ));
                }
                RegionInstructionKind::UnsetDim { keys, .. } if keys.len() > 1 => {
                    let normalized = keys[1..]
                        .iter()
                        .map(|key| publication_integer_array_key(constants, *key))
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_GLOBAL_ARRAY_KEY",
                                format!(
                                    "global array unset at continuation {} has an unnormalized key path",
                                    instruction.continuation_id,
                                ),
                            )
                        })?;
                    let (&key, parents) = normalized
                        .split_last()
                        .expect("nested global unset retains a child key");
                    admission
                        .array_requirements
                        .entry(source)
                        .or_default()
                        .mutations
                        .push(NativeEntryArrayMutationRequirement::Unset {
                            parents: parents.to_vec(),
                            key,
                        });
                    admission
                        .total_array_instructions
                        .insert(instruction.continuation_id);
                }
                RegionInstructionKind::FetchDim { .. }
                | RegionInstructionKind::AssignDim { .. }
                | RegionInstructionKind::AppendDim { .. }
                | RegionInstructionKind::UnsetDim { .. } => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_GLOBAL_DIMENSION_SHAPE",
                        format!(
                            "global dimension continuation {} has no total publication plan",
                            instruction.continuation_id,
                        ),
                    ));
                }
                _ => {}
            }
            if admission
                .total_array_instructions
                .contains(&instruction.continuation_id)
            {
                entry_dependent_continuations.insert(instruction.continuation_id);
            }
        }
        if let RegionInstructionKind::FetchProperty { object, .. }
        | RegionInstructionKind::AssignProperty { object, .. } = &instruction.kind
        {
            let NativeEntryArraySource::Parameter(parameter_index) =
                entry_array_source(*object, &definitions, &parameter_indices).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PROPERTY_OBJECT_SHAPE",
                        format!(
                            "property continuation {} is not rooted at an entry object",
                            instruction.continuation_id,
                        ),
                    )
                })?
            else {
                unreachable!("operand-rooted property objects are parameters")
            };
            let local = region.parameter_locals[parameter_index];
            if by_ref_parameters.contains(&local) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_PROPERTY_OBJECT_TYPE",
                    format!(
                        "property continuation {} has no plain direct-object entry contract",
                        instruction.continuation_id,
                    ),
                ));
            }
            let (required_state, readable, releasable) = match &instruction.kind {
                RegionInstructionKind::FetchProperty { .. } => (
                    crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
                    true,
                    false,
                ),
                RegionInstructionKind::AssignProperty { value, .. } => {
                    let value = lowering_operand_fact(value_flow, constants, *value);
                    if value.certainty == crate::region_ir::SsaCertainty::Unknown
                        || !matches!(
                            value.class,
                            SsaValueClass::Int | SsaValueClass::Bool | SsaValueClass::Null
                        )
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_PROPERTY_VALUE_OWNERSHIP",
                            format!(
                                "property assignment at continuation {} has no immediate ownership plan",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    (
                        crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE,
                        false,
                        true,
                    )
                }
                _ => unreachable!("property requirement kind"),
            };
            admission
                .property_requirements
                .push(NativeEntryPropertyRequirement {
                    parameter_index,
                    continuation_id: instruction.continuation_id,
                    required_state,
                    readable,
                    releasable,
                    allow_reference: false,
                    require_reference: false,
                    probe_paths: Vec::new(),
                    mutations: Vec::new(),
                });
            entry_dependent_continuations.insert(instruction.continuation_id);
        }
        if let RegionInstructionKind::ArrayCallback(call) = &instruction.kind {
            let RegionArrayCallbackTarget::Stable(callback) = &call.callback else {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLBACK_PUBLICATION",
                    format!(
                        "runtime callback at continuation {} is not fixed before optimizing entry",
                        instruction.continuation_id,
                    ),
                ));
            };
            let provided_argument_count = match call.operation {
                RegionArrayCallbackOperation::Map => call.arrays.len(),
                RegionArrayCallbackOperation::FilterValue
                | RegionArrayCallbackOperation::FilterKey => 1,
                RegionArrayCallbackOperation::FilterValueAndKey
                | RegionArrayCallbackOperation::All
                | RegionArrayCallbackOperation::Any
                | RegionArrayCallbackOperation::Find
                | RegionArrayCallbackOperation::FindKey => 2,
                RegionArrayCallbackOperation::Walk => {
                    2usize.saturating_add(usize::from(call.initial.is_some()))
                }
                RegionArrayCallbackOperation::Reduce
                | RegionArrayCallbackOperation::Usort
                | RegionArrayCallbackOperation::Uasort
                | RegionArrayCallbackOperation::Uksort
                | RegionArrayCallbackOperation::WalkRecursive
                | RegionArrayCallbackOperation::PregReplace => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CALLBACK_TOTAL_FAMILY",
                        format!(
                            "callback operation at continuation {} has no publication-total native family",
                            instruction.continuation_id,
                        ),
                    ));
                }
            };
            if callback.receiver.is_some()
                || callback.closure.is_some()
                || call.arrays.is_empty()
                || call.operation != RegionArrayCallbackOperation::Map && call.arrays.len() != 1
                || matches!(
                    call.operation,
                    RegionArrayCallbackOperation::FilterValue
                        | RegionArrayCallbackOperation::FilterKey
                        | RegionArrayCallbackOperation::FilterValueAndKey
                        | RegionArrayCallbackOperation::All
                        | RegionArrayCallbackOperation::Any
                        | RegionArrayCallbackOperation::Find
                        | RegionArrayCallbackOperation::FindKey
                        | RegionArrayCallbackOperation::Walk
                ) && !callback.returns_releasable_scalar
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLBACK_TOTAL_FAMILY",
                    format!(
                        "callback operation at continuation {} has no total fixed callback contract",
                        instruction.continuation_id,
                    ),
                ));
            }
            let function = callback.function.ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLBACK_TARGET",
                    format!(
                        "callback at continuation {} has no same-unit published target",
                        instruction.continuation_id,
                    ),
                )
            })?;
            let target = function_params.get(&function).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLBACK_TARGET",
                    format!(
                        "callback at continuation {} has no published native signature",
                        instruction.continuation_id,
                    ),
                )
            })?;
            let binding = optimizing_callback_binding_plan(
                OptimizingCompiledCallTarget {
                    address: OptimizingCompiledCallAddress::Local(function),
                    params: &target.params,
                    requires_trampoline: target.requires_trampoline,
                    arity: target.native_arity,
                    reference_only_trampoline: target.reference_only_trampoline,
                    returns_by_reference: target.returns_by_reference,
                    exception_routes: target.has_exception_handlers.then_some(function),
                },
                0,
                provided_argument_count,
                usize::from(call.operation == RegionArrayCallbackOperation::Walk),
            )
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLBACK_BINDING",
                    format!(
                        "callback at continuation {} has no total positional binding",
                        instruction.continuation_id,
                    ),
                )
            })?;
            if binding.variadic
                || target.returns_by_reference
                || target
                    .params
                    .iter()
                    .take(provided_argument_count)
                    .enumerate()
                    .any(|(index, parameter)| {
                        (parameter.by_ref
                            && !(call.operation == RegionArrayCallbackOperation::Walk
                                && index == 0))
                            || parameter
                                .type_
                                .as_ref()
                                .is_none_or(|type_| !optimizing_type_has_direct_guard(type_))
                    })
                || target
                    .params
                    .iter()
                    .skip(provided_argument_count)
                    .take(
                        binding
                            .fixed_parameter_count
                            .saturating_sub(provided_argument_count),
                    )
                    .any(|parameter| {
                        !matches!(
                            parameter.default,
                            Some(IrConstant::Null | IrConstant::Bool(_))
                        ) && !matches!(
                            parameter.default,
                            Some(IrConstant::Int(value))
                                if native_integer_fits_immediate(value)
                        )
                    })
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLBACK_BINDING",
                    format!(
                        "callback at continuation {} needs a reference, variadic, or allocated default binding",
                        instruction.continuation_id,
                    ),
                ));
            }
            for (index, operand) in call.arrays.iter().copied().enumerate() {
                let source =
                    entry_array_source(operand, &definitions, &parameter_indices).ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_CALLBACK_ARRAY_SHAPE",
                            format!(
                                "callback array {} at continuation {} is not rooted at native entry",
                                index, instruction.continuation_id,
                            ),
                        )
                    })?;
                let NativeEntryArraySource::Parameter(source_index) = source else {
                    unreachable!("callback entry arrays are parameters")
                };
                if by_ref_parameters.contains(&region.parameter_locals[source_index]) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CALLBACK_ARRAY_REFERENCE",
                        format!(
                            "callback array {} at continuation {} is a by-reference parameter",
                            index, instruction.continuation_id,
                        ),
                    ));
                }
                let requirement = admission.array_requirements.entry(source).or_default();
                requirement.require_plain_values = true;
                let value_parameter = match call.operation {
                    RegionArrayCallbackOperation::Map => Some(index),
                    RegionArrayCallbackOperation::FilterValue
                    | RegionArrayCallbackOperation::FilterValueAndKey
                    | RegionArrayCallbackOperation::All
                    | RegionArrayCallbackOperation::Any
                    | RegionArrayCallbackOperation::Find
                    | RegionArrayCallbackOperation::FindKey
                    | RegionArrayCallbackOperation::Walk => Some(0),
                    RegionArrayCallbackOperation::FilterKey => None,
                    _ => unreachable!("unsupported callback operation was rejected above"),
                };
                if let Some(type_) =
                    value_parameter.and_then(|parameter| target.params[parameter].type_.as_ref())
                {
                    requirement.all_value_types.push(type_.clone());
                }
                if matches!(
                    call.operation,
                    RegionArrayCallbackOperation::Map
                        | RegionArrayCallbackOperation::FilterValue
                        | RegionArrayCallbackOperation::FilterKey
                        | RegionArrayCallbackOperation::FilterValueAndKey
                ) {
                    requirement.projection_allocations =
                        requirement.projection_allocations.saturating_add(1);
                }
                if matches!(
                    call.operation,
                    RegionArrayCallbackOperation::FilterKey
                        | RegionArrayCallbackOperation::FilterValueAndKey
                        | RegionArrayCallbackOperation::All
                        | RegionArrayCallbackOperation::Any
                        | RegionArrayCallbackOperation::Find
                        | RegionArrayCallbackOperation::FindKey
                        | RegionArrayCallbackOperation::Walk
                ) {
                    requirement.require_supported_keys = true;
                }
                if call.operation == RegionArrayCallbackOperation::Walk {
                    requirement.value_allocations_per_entry =
                        requirement.value_allocations_per_entry.saturating_add(1);
                    requirement.projection_allocations =
                        requirement.projection_allocations.saturating_add(1);
                }
            }
            admission.require_non_fiber_scope = true;
            admission
                .total_array_instructions
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let RegionInstructionKind::PregCallbackArray(call) = &instruction.kind
            && call
                .entries
                .iter()
                .any(|entry| matches!(entry.callback, RegionArrayCallbackTarget::Runtime(_)))
        {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_PREG_RUNTIME_CALLBACK_PUBLICATION",
                format!(
                    "runtime preg callback map at continuation {} is assigned to baseline before region entry",
                    instruction.continuation_id,
                ),
            ));
        }
        let immediate_local = |local: LocalId| {
            let fact = lowering_operand_fact(value_flow, constants, RegionOperand::Local(local));
            value_flow.local_storage(local) == crate::region_ir::LocalStorageClass::SsaPlain
                && fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && matches!(
                    fact.class,
                    SsaValueClass::Int | SsaValueClass::Bool | SsaValueClass::Null
                )
        };
        let reference_source_local =
            |local: LocalId| immediate_local(local) || parameter_indices.contains_key(&local);
        let reference_target_local = |local: LocalId| {
            immediate_local(local)
                || (!parameter_indices.contains_key(&local)
                    && first_local_write.get(&local).copied() == Some(instruction.continuation_id))
        };
        let reference_mutation =
            |keys: &[RegionOperand],
             append: bool|
             -> Result<NativeEntryArrayMutationRequirement, CraneliftLoweringError> {
                let normalized = keys
                .iter()
                .map(|key| publication_integer_array_key(constants, *key))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_REFERENCE_DIM_KEY_PUBLICATION",
                        format!(
                            "reference dimension at continuation {} has an unnormalized key path",
                            instruction.continuation_id,
                        ),
                    )
                })?;
                if append {
                    Ok(NativeEntryArrayMutationRequirement::ReferenceAppend {
                        parents: normalized,
                    })
                } else {
                    let (&key, parents) = normalized.split_last().ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_REFERENCE_DIM_ARITY",
                            format!(
                                "reference dimension at continuation {} has no leaf key",
                                instruction.continuation_id,
                            ),
                        )
                    })?;
                    Ok(NativeEntryArrayMutationRequirement::Reference {
                        parents: parents.to_vec(),
                        key,
                    })
                }
            };
        let property_parameter = |object: RegionOperand| -> Result<usize, CraneliftLoweringError> {
            let NativeEntryArraySource::Parameter(parameter_index) = entry_array_source(
                object,
                &definitions,
                &parameter_indices,
            )
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_REFERENCE_PROPERTY_OBJECT",
                    format!(
                        "property reference at continuation {} is not rooted at an entry object",
                        instruction.continuation_id,
                    ),
                )
            })?
            else {
                unreachable!("operand-rooted property reference objects are parameters")
            };
            if by_ref_parameters.contains(&region.parameter_locals[parameter_index]) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_REFERENCE_PROPERTY_OBJECT",
                    format!(
                        "property reference at continuation {} has no plain object owner",
                        instruction.continuation_id,
                    ),
                ));
            }
            Ok(parameter_index)
        };
        match &instruction.kind {
            RegionInstructionKind::BindReference { target, source } => {
                if value_flow.local_storage(*target)
                    == crate::region_ir::LocalStorageClass::Superglobal
                    || region.flags.is_top_level
                        && value_flow.local_storage(*target)
                            == crate::region_ir::LocalStorageClass::RequestGlobal
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_REQUEST_REFERENCE_PUBLICATION",
                        format!(
                            "request-global reference at continuation {} changes symbol identity",
                            instruction.continuation_id,
                        ),
                    ));
                }
                if !reference_source_local(*source)
                    || (*target != *source && !reference_target_local(*target))
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_LOCAL_REFERENCE_PUBLICATION",
                        format!(
                            "local reference at continuation {} has no entry-stable immediate ownership plan",
                            instruction.continuation_id,
                        ),
                    ));
                }
                admission.fixed_value_allocations =
                    admission.fixed_value_allocations.saturating_add(1);
                admission
                    .total_array_instructions
                    .insert(instruction.continuation_id);
                entry_dependent_continuations.insert(instruction.continuation_id);
                continue;
            }
            RegionInstructionKind::BindReferenceDim {
                target,
                array,
                keys,
            }
            | RegionInstructionKind::BindReferenceIntoDim {
                array,
                keys,
                append: false,
                source: target,
            } => {
                if instruction.native_global_name.is_some() {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_GLOBAL_REFERENCE_DIM_PUBLICATION",
                        format!(
                            "global reference dimension at continuation {} changes symbol identity",
                            instruction.continuation_id,
                        ),
                    ));
                }
                let source_index = parameter_indices.get(array).copied().ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_REFERENCE_DIM_ROOT",
                        format!(
                            "reference dimension at continuation {} is not rooted at an entry array",
                            instruction.continuation_id,
                        ),
                    )
                })?;
                if by_ref_parameters.contains(array)
                    || value_flow.local_storage(*array)
                        != crate::region_ir::LocalStorageClass::SsaPlain
                    || match &instruction.kind {
                        RegionInstructionKind::BindReferenceDim { .. } => {
                            !reference_target_local(*target)
                        }
                        RegionInstructionKind::BindReferenceIntoDim { .. } => {
                            !reference_source_local(*target)
                        }
                        _ => unreachable!("reference dimension kind"),
                    }
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_REFERENCE_DIM_OWNERSHIP",
                        format!(
                            "reference dimension at continuation {} has no entry-stable COW/ownership plan",
                            instruction.continuation_id,
                        ),
                    ));
                }
                admission
                    .array_requirements
                    .entry(NativeEntryArraySource::Parameter(source_index))
                    .or_default()
                    .mutations
                    .push(reference_mutation(keys, false)?);
                admission.fixed_value_allocations =
                    admission.fixed_value_allocations.saturating_add(1);
                admission
                    .total_array_instructions
                    .insert(instruction.continuation_id);
                entry_dependent_continuations.insert(instruction.continuation_id);
                continue;
            }
            RegionInstructionKind::BindReferenceIntoDim {
                array,
                keys,
                append: true,
                source,
            } => {
                if instruction.native_global_name.is_some() {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_GLOBAL_REFERENCE_DIM_PUBLICATION",
                        format!(
                            "global append reference dimension at continuation {} changes symbol identity",
                            instruction.continuation_id,
                        ),
                    ));
                }
                let source_index = parameter_indices.get(array).copied().ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_REFERENCE_DIM_ROOT",
                        format!(
                            "append reference dimension at continuation {} is not rooted at an entry array",
                            instruction.continuation_id,
                        ),
                    )
                })?;
                if by_ref_parameters.contains(array)
                    || value_flow.local_storage(*array)
                        != crate::region_ir::LocalStorageClass::SsaPlain
                    || !reference_source_local(*source)
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_REFERENCE_DIM_OWNERSHIP",
                        format!(
                            "append reference dimension at continuation {} has no entry-stable COW/ownership plan",
                            instruction.continuation_id,
                        ),
                    ));
                }
                admission
                    .array_requirements
                    .entry(NativeEntryArraySource::Parameter(source_index))
                    .or_default()
                    .mutations
                    .push(reference_mutation(keys, true)?);
                admission.fixed_value_allocations =
                    admission.fixed_value_allocations.saturating_add(1);
                admission
                    .total_array_instructions
                    .insert(instruction.continuation_id);
                entry_dependent_continuations.insert(instruction.continuation_id);
                continue;
            }
            RegionInstructionKind::BindReferenceProperty { object, source, .. } => {
                if !reference_source_local(*source) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PROPERTY_REFERENCE_OWNERSHIP",
                        format!(
                            "property reference at continuation {} has no entry-stable source owner",
                            instruction.continuation_id,
                        ),
                    ));
                }
                admission
                    .property_requirements
                    .push(NativeEntryPropertyRequirement {
                        parameter_index: property_parameter(*object)?,
                        continuation_id: instruction.continuation_id,
                        required_state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE,
                        readable: false,
                        releasable: true,
                        allow_reference: true,
                        require_reference: false,
                        probe_paths: Vec::new(),
                        mutations: Vec::new(),
                    });
                admission.fixed_value_allocations =
                    admission.fixed_value_allocations.saturating_add(1);
                admission
                    .total_array_instructions
                    .insert(instruction.continuation_id);
                entry_dependent_continuations.insert(instruction.continuation_id);
                continue;
            }
            RegionInstructionKind::BindReferenceFromProperty { target, object, .. } => {
                if !reference_target_local(*target) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PROPERTY_REFERENCE_TARGET",
                        format!(
                            "property reference at continuation {} has no entry-stable target owner",
                            instruction.continuation_id,
                        ),
                    ));
                }
                admission
                    .property_requirements
                    .push(NativeEntryPropertyRequirement {
                        parameter_index: property_parameter(*object)?,
                        continuation_id: instruction.continuation_id,
                        required_state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE,
                        readable: false,
                        releasable: false,
                        allow_reference: true,
                        require_reference: false,
                        probe_paths: Vec::new(),
                        mutations: Vec::new(),
                    });
                admission.fixed_value_allocations =
                    admission.fixed_value_allocations.saturating_add(1);
                admission
                    .total_array_instructions
                    .insert(instruction.continuation_id);
                entry_dependent_continuations.insert(instruction.continuation_id);
                continue;
            }
            RegionInstructionKind::BindReferenceIntoPropertyDim {
                object,
                keys,
                append: _,
                source,
                ..
            }
            | RegionInstructionKind::BindReferenceFromPropertyDim {
                target: source,
                object,
                keys,
                ..
            } => {
                if match &instruction.kind {
                    RegionInstructionKind::BindReferenceIntoPropertyDim { .. } => {
                        !reference_source_local(*source)
                    }
                    RegionInstructionKind::BindReferenceFromPropertyDim { .. } => {
                        !reference_target_local(*source)
                    }
                    _ => unreachable!("property dimension reference kind"),
                } {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PROPERTY_DIM_REFERENCE_OWNERSHIP",
                        format!(
                            "property dimension reference at continuation {} has no entry-stable local owner",
                            instruction.continuation_id,
                        ),
                    ));
                }
                let append = matches!(
                    &instruction.kind,
                    RegionInstructionKind::BindReferenceIntoPropertyDim { append: true, .. }
                );
                admission
                    .property_requirements
                    .push(NativeEntryPropertyRequirement {
                        parameter_index: property_parameter(*object)?,
                        continuation_id: instruction.continuation_id,
                        required_state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE,
                        readable: true,
                        releasable: false,
                        allow_reference: false,
                        require_reference: false,
                        probe_paths: Vec::new(),
                        mutations: vec![reference_mutation(keys, append)?],
                    });
                admission.fixed_value_allocations =
                    admission.fixed_value_allocations.saturating_add(1);
                admission
                    .total_array_instructions
                    .insert(instruction.continuation_id);
                entry_dependent_continuations.insert(instruction.continuation_id);
                continue;
            }
            RegionInstructionKind::BindReferenceDimFromProperty {
                array,
                keys,
                append: false,
                object,
                ..
            } if keys.is_empty() => {
                if !reference_target_local(*array) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PROPERTY_REFERENCE_TARGET",
                        format!(
                            "property reference at continuation {} has no entry-stable target owner",
                            instruction.continuation_id,
                        ),
                    ));
                }
                admission
                    .property_requirements
                    .push(NativeEntryPropertyRequirement {
                        parameter_index: property_parameter(*object)?,
                        continuation_id: instruction.continuation_id,
                        required_state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE,
                        readable: false,
                        releasable: false,
                        allow_reference: true,
                        require_reference: false,
                        probe_paths: Vec::new(),
                        mutations: Vec::new(),
                    });
                admission.fixed_value_allocations =
                    admission.fixed_value_allocations.saturating_add(1);
                admission
                    .total_array_instructions
                    .insert(instruction.continuation_id);
                entry_dependent_continuations.insert(instruction.continuation_id);
                continue;
            }
            RegionInstructionKind::BindReferenceDimFromProperty {
                array,
                keys,
                append,
                object,
                ..
            } => {
                let source_index = parameter_indices.get(array).copied().ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PROPERTY_REFERENCE_DIM_ROOT",
                        format!(
                            "property-to-dimension reference at continuation {} is not rooted at an entry array",
                            instruction.continuation_id,
                        ),
                    )
                })?;
                if by_ref_parameters.contains(array)
                    || value_flow.local_storage(*array)
                        != crate::region_ir::LocalStorageClass::SsaPlain
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PROPERTY_REFERENCE_DIM_OWNERSHIP",
                        format!(
                            "property-to-dimension reference at continuation {} has no entry-stable owner",
                            instruction.continuation_id,
                        ),
                    ));
                }
                admission
                    .array_requirements
                    .entry(NativeEntryArraySource::Parameter(source_index))
                    .or_default()
                    .mutations
                    .push(reference_mutation(keys, *append)?);
                admission
                    .property_requirements
                    .push(NativeEntryPropertyRequirement {
                        parameter_index: property_parameter(*object)?,
                        continuation_id: instruction.continuation_id,
                        required_state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE,
                        readable: true,
                        releasable: false,
                        allow_reference: true,
                        require_reference: true,
                        probe_paths: Vec::new(),
                        mutations: Vec::new(),
                    });
                admission
                    .total_array_instructions
                    .insert(instruction.continuation_id);
                entry_dependent_continuations.insert(instruction.continuation_id);
                continue;
            }
            _ => {}
        }
        if let RegionInstructionKind::NativeControl(RegionNativeControl::MakeException {
            message,
            ..
        }) = &instruction.kind
        {
            if let Some(message) = message {
                let fact = lowering_operand_fact(value_flow, constants, *message);
                if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                    || !matches!(
                        fact.class,
                        SsaValueClass::Null
                            | SsaValueClass::Bool
                            | SsaValueClass::Int
                            | SsaValueClass::Float
                            | SsaValueClass::StringHandle
                    )
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_EXCEPTION_MESSAGE_PUBLICATION",
                        format!(
                            "exception message at continuation {} has no total scalar/string publication plan",
                            instruction.continuation_id,
                        ),
                    ));
                }
                if fact.class == SsaValueClass::StringHandle {
                    let guarded = admit_publication_string(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        *message,
                        0,
                        1,
                        instruction.continuation_id,
                        "exception message",
                    )?;
                    entry_dependent_continuations
                        .extend(guarded.map(|_| instruction.continuation_id));
                } else {
                    admission.fixed_string_bytes = admission.fixed_string_bytes.saturating_add(
                        publication_string_capacity(
                            php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY,
                        )
                        .unwrap_or(php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY),
                    );
                }
            } else {
                admission.fixed_string_bytes = admission
                    .fixed_string_bytes
                    .saturating_add(crate::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY as usize);
            }
            admission
                .exception_requirements
                .push(NativeEntryExceptionRequirement {
                    continuation_id: instruction.continuation_id,
                    include_function_frame: !region.flags.is_top_level,
                });
        }
        if let RegionInstructionKind::CloneObject { object, plain, .. } = &instruction.kind {
            if !plain {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CLONE_PUBLICATION",
                    format!(
                        "clone at continuation {} has no publication proof excluding __clone",
                        instruction.continuation_id,
                    ),
                ));
            }
            let fact = lowering_operand_fact(value_flow, constants, *object);
            if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                || fact.class != SsaValueClass::ObjectHandle
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CLONE_OBJECT_SHAPE",
                    format!(
                        "clone at continuation {} has no published direct-object shape",
                        instruction.continuation_id,
                    ),
                ));
            }
            if let Some(parameter_index) =
                publication_entry_parameter(*object, &definitions, &parameter_indices)
            {
                if by_ref_parameters.contains(&region.parameter_locals[parameter_index]) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CLONE_REFERENCE",
                        "plain clone source is a by-reference parameter",
                    ));
                }
                admission
                    .value_class_requirements
                    .push(NativeEntryValueClassRequirement {
                        parameter_index,
                        class: SsaValueClass::ObjectHandle,
                    });
                if !admission
                    .clone_requirements
                    .iter()
                    .any(|requirement| requirement.parameter_index == parameter_index)
                {
                    admission
                        .clone_requirements
                        .push(NativeEntryCloneRequirement { parameter_index });
                }
                entry_dependent_continuations.insert(instruction.continuation_id);
            }
            admission.fixed_value_allocations = admission.fixed_value_allocations.saturating_add(1);
        }
        let RegionInstructionKind::NativeCall(call) = &instruction.kind else {
            continue;
        };
        if let RegionCallTarget::Method {
            receiver_layout_id: Some(layout_id),
            ..
        } = &call.target
        {
            let receiver = call.operands.first().copied().flatten().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_METHOD_RECEIVER_PUBLICATION",
                    format!(
                        "specialized method at continuation {} has no receiver",
                        instruction.continuation_id,
                    ),
                )
            })?;
            let NativeEntryArraySource::Parameter(parameter_index) = entry_array_source(
                receiver,
                &definitions,
                &parameter_indices,
            )
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_METHOD_RECEIVER_PUBLICATION",
                    format!(
                        "specialized method at continuation {} is not rooted at an entry receiver",
                        instruction.continuation_id,
                    ),
                )
            })?
            else {
                unreachable!("method receiver operands are entry parameters")
            };
            if by_ref_parameters.contains(&region.parameter_locals[parameter_index]) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_METHOD_RECEIVER_REFERENCE",
                    "specialized method receiver is a by-reference parameter",
                ));
            }
            admission
                .object_layout_requirements
                .push(NativeEntryObjectLayoutRequirement {
                    parameter_index,
                    layout_id: *layout_id,
                });
            entry_dependent_continuations.insert(instruction.continuation_id);
        }
        if let RegionCallTarget::Semantic {
            operation: RegionSemanticOp::InstanceOf { object, .. },
        } = &call.target
        {
            let NativeEntryArraySource::Parameter(parameter_index) =
                entry_array_source(*object, &definitions, &parameter_indices).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_INSTANCEOF_PUBLICATION",
                        format!(
                            "instanceof at continuation {} is not rooted at an entry object",
                            instruction.continuation_id,
                        ),
                    )
                })?
            else {
                unreachable!("instanceof object operands are entry parameters")
            };
            if by_ref_parameters.contains(&region.parameter_locals[parameter_index]) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_INSTANCEOF_REFERENCE",
                    "instanceof object is a by-reference parameter",
                ));
            }
            admission
                .instanceof_requirements
                .push(NativeEntryInstanceofRequirement {
                    parameter_index: Some(parameter_index),
                    continuation_id: instruction.continuation_id,
                });
            entry_dependent_continuations.insert(instruction.continuation_id);
        }
        if let RegionCallTarget::Semantic {
            operation: RegionSemanticOp::DynamicInstanceOf { object, target, .. },
        } = &call.target
        {
            let target_name = publication_utf8_string(constants, &definitions, *target)
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DYNAMIC_INSTANCEOF_TARGET",
                        format!(
                            "dynamic instanceof at continuation {} has no fixed UTF-8 target class",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            if target_name.trim_start_matches('\\').is_empty() {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DYNAMIC_INSTANCEOF_TARGET",
                    "dynamic instanceof target class is empty",
                ));
            }
            let parameter_index =
                publication_entry_parameter(*object, &definitions, &parameter_indices);
            if parameter_index.is_some_and(|parameter_index| {
                by_ref_parameters.contains(&region.parameter_locals[parameter_index])
            }) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DYNAMIC_INSTANCEOF_REFERENCE",
                    "dynamic instanceof object is a by-reference parameter",
                ));
            }
            let _ = admit_publication_scalar_class(
                &mut admission,
                value_flow,
                constants,
                &definitions,
                &parameter_indices,
                *object,
                SsaValueClass::ObjectHandle,
                instruction.continuation_id,
                "dynamic instanceof object",
            )?;
            admission
                .instanceof_requirements
                .push(NativeEntryInstanceofRequirement {
                    parameter_index,
                    continuation_id: instruction.continuation_id,
                });
            if parameter_index.is_some() {
                entry_dependent_continuations.insert(instruction.continuation_id);
            }
        }
        if let RegionCallTarget::Semantic {
            operation: RegionSemanticOp::ResolveCallable { callable, .. },
        } = &call.target
        {
            let php_ir::instruction::CallableKind::FunctionName { name } = callable else {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLABLE_KIND_PUBLICATION",
                    format!(
                        "callable resolution at continuation {} is not a fixed function name",
                        instruction.continuation_id,
                    ),
                ));
            };
            let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
            let target = function_params.iter().find(|(_, target)| {
                target
                    .name
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(&normalized)
            });
            let Some((_, target)) = target else {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLABLE_TARGET_PUBLICATION",
                    format!(
                        "callable {name} at continuation {} has no same-unit published target",
                        instruction.continuation_id,
                    ),
                ));
            };
            if target.requires_trampoline
                || target.reference_only_trampoline
                || target.returns_by_reference
                || target.native_arity > u32::MAX as usize
                || target.params.len() > u32::MAX as usize
                || name.len() > u32::MAX as usize
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLABLE_SIGNATURE_PUBLICATION",
                    format!(
                        "callable {name} at continuation {} has no fixed direct native signature",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission.fixed_value_allocations = admission.fixed_value_allocations.saturating_add(1);
        }
        if let RegionCallTarget::Semantic {
            operation: RegionSemanticOp::AcquireCallable { value, .. },
        } = &call.target
        {
            let NativeEntryArraySource::Parameter(parameter_index) =
                entry_array_source(*value, &definitions, &parameter_indices).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CALLABLE_SHAPE_PUBLICATION",
                        format!(
                            "callable acquisition at continuation {} is not rooted at an entry callable",
                            instruction.continuation_id,
                        ),
                    )
                })?
            else {
                unreachable!("callable acquisition operands are parameters")
            };
            if by_ref_parameters.contains(&region.parameter_locals[parameter_index]) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLABLE_REFERENCE_PUBLICATION",
                    "callable acquisition receives a by-reference parameter",
                ));
            }
            admit_publication_scalar_class(
                &mut admission,
                value_flow,
                constants,
                &definitions,
                &parameter_indices,
                *value,
                SsaValueClass::CallableHandle,
                instruction.continuation_id,
                "callable acquisition",
            )?;
            if !admission
                .callable_requirements
                .iter()
                .any(|requirement| requirement.parameter_index == parameter_index)
            {
                admission
                    .callable_requirements
                    .push(NativeEntryCallableRequirement { parameter_index });
            }
            entry_dependent_continuations.insert(instruction.continuation_id);
        }
        if let RegionCallTarget::Semantic { operation } = &call.target {
            let dynamic_property = match operation {
                RegionSemanticOp::PropertyFetch {
                    object,
                    property: RegionPropertyName::Dynamic(property),
                    ..
                }
                | RegionSemanticOp::PropertyUnset {
                    object,
                    property: RegionPropertyName::Dynamic(property),
                    ..
                } => Some((*object, *property, None, false)),
                RegionSemanticOp::PropertyAssign {
                    object,
                    property: RegionPropertyName::Dynamic(property),
                    value,
                    ..
                } => Some((*object, *property, Some(*value), false)),
                RegionSemanticOp::PropertyIsset {
                    object,
                    property: RegionPropertyName::Dynamic(property),
                    ..
                }
                | RegionSemanticOp::PropertyEmpty {
                    object,
                    property: RegionPropertyName::Dynamic(property),
                    ..
                } => Some((*object, *property, None, true)),
                RegionSemanticOp::PropertyDimAssign {
                    property: RegionPropertyName::Dynamic(_),
                    ..
                }
                | RegionSemanticOp::PropertyDimUnset {
                    property: RegionPropertyName::Dynamic(_),
                    ..
                }
                | RegionSemanticOp::PropertyDimIsset {
                    property: RegionPropertyName::Dynamic(_),
                    ..
                }
                | RegionSemanticOp::PropertyDimEmpty {
                    property: RegionPropertyName::Dynamic(_),
                    ..
                } => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DYNAMIC_PROPERTY_DIM_PUBLICATION",
                        format!(
                            "dynamic property dimension at continuation {} has no total slot plan",
                            instruction.continuation_id,
                        ),
                    ));
                }
                _ => None,
            };
            if let Some((object, property, assigned, test)) = dynamic_property {
                let property_name = publication_utf8_string(constants, &definitions, property)
                    .ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DYNAMIC_PROPERTY_NAME_PUBLICATION",
                            format!(
                                "dynamic property at continuation {} has no fixed UTF-8 name",
                                instruction.continuation_id,
                            ),
                        )
                    })?;
                if property_name.is_empty() {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DYNAMIC_PROPERTY_NAME_PUBLICATION",
                        "dynamic property name is empty",
                    ));
                }
                let NativeEntryArraySource::Parameter(parameter_index) = entry_array_source(
                    object,
                    &definitions,
                    &parameter_indices,
                )
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DYNAMIC_PROPERTY_OBJECT_PUBLICATION",
                        format!(
                            "dynamic property at continuation {} is not rooted at an entry object",
                            instruction.continuation_id,
                        ),
                    )
                })?
                else {
                    unreachable!("dynamic property objects are parameters")
                };
                if by_ref_parameters.contains(&region.parameter_locals[parameter_index]) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DYNAMIC_PROPERTY_REFERENCE_PUBLICATION",
                        "dynamic property object is a by-reference parameter",
                    ));
                }
                admit_publication_scalar_class(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    object,
                    SsaValueClass::ObjectHandle,
                    instruction.continuation_id,
                    "dynamic property object",
                )?;
                if let Some(value) = assigned {
                    let fact = lowering_operand_fact(value_flow, constants, value);
                    if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || fact.class == SsaValueClass::ReferenceHandle
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DYNAMIC_PROPERTY_OWNERSHIP",
                            format!(
                                "dynamic property assignment at continuation {} has no total ownership plan",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                }
                if !admission
                    .dynamic_property_requirements
                    .iter()
                    .any(|requirement| requirement.parameter_index == parameter_index)
                {
                    admission
                        .dynamic_property_requirements
                        .push(NativeEntryDynamicPropertyRequirement { parameter_index });
                }
                if test {
                    // `isset`/`empty` may use the immutable absence cell; no
                    // insertion capacity is consumed.
                } else {
                    admission.fixed_lvalue_insertions =
                        admission.fixed_lvalue_insertions.saturating_add(1);
                }
            }
        }
        if let RegionCallTarget::Semantic {
            operation:
                RegionSemanticOp::StaticPropertyReference {
                    target,
                    class_name,
                    dimensions,
                    bind_source_into_property,
                    ..
                },
        } = &call.target
        {
            if !matches!(class_name, crate::region_ir::RegionClassName::Static(_))
                || if *bind_source_into_property {
                    !reference_source_local(*target)
                } else {
                    !reference_target_local(*target)
                }
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_STATIC_PROPERTY_REFERENCE_PUBLICATION",
                    format!(
                        "static property reference at continuation {} has no fixed class/local ownership plan",
                        instruction.continuation_id,
                    ),
                ));
            }
            let mutations = if *bind_source_into_property || dimensions.is_empty() {
                Vec::new()
            } else {
                vec![reference_mutation(dimensions, false)?]
            };
            let dimension = !mutations.is_empty();
            admission
                .static_property_requirements
                .push(NativeEntryStaticPropertyRequirement {
                    continuation_id: instruction.continuation_id,
                    required_state: if dimension {
                        crate::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_WRITABLE
                    } else {
                        crate::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_WRITABLE
                    },
                    readable: !*bind_source_into_property || dimension,
                    releasable: *bind_source_into_property && !dimension,
                    allow_reference: !dimension,
                    require_reference: false,
                    probe_paths: Vec::new(),
                    mutations,
                });
            admission.fixed_value_allocations = admission.fixed_value_allocations.saturating_add(1);
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let RegionCallTarget::Semantic { operation } = &call.target
            && matches!(
                operation,
                RegionSemanticOp::PropertyDimAssign { .. }
                    | RegionSemanticOp::PropertyDimUnset { .. }
                    | RegionSemanticOp::StaticPropertyDimUnset { .. }
            )
        {
            let (dimensions, append) = match operation {
                RegionSemanticOp::PropertyDimAssign {
                    property: RegionPropertyName::Static(_),
                    dimensions,
                    value,
                    append,
                    ..
                } => {
                    let fact = lowering_operand_fact(value_flow, constants, *value);
                    if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || !matches!(
                            fact.class,
                            SsaValueClass::Int | SsaValueClass::Bool | SsaValueClass::Null
                        )
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_PROPERTY_DIM_VALUE_OWNERSHIP",
                            format!(
                                "property dimension assignment at continuation {} has no immediate ownership plan",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    (dimensions, *append)
                }
                RegionSemanticOp::PropertyDimUnset {
                    property: RegionPropertyName::Static(_),
                    dimensions,
                    ..
                }
                | RegionSemanticOp::StaticPropertyDimUnset {
                    class_name: crate::region_ir::RegionClassName::Static(_),
                    dimensions,
                    ..
                } => (dimensions, false),
                RegionSemanticOp::PropertyDimAssign { .. }
                | RegionSemanticOp::PropertyDimUnset { .. } => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DYNAMIC_PROPERTY_DIM_MUTATION_PUBLICATION",
                        format!(
                            "dynamic property dimension mutation at continuation {} has no fixed publication slot",
                            instruction.continuation_id,
                        ),
                    ));
                }
                RegionSemanticOp::StaticPropertyDimUnset { .. } => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_DYNAMIC_STATIC_PROPERTY_DIM_MUTATION_PUBLICATION",
                        format!(
                            "runtime-class static property dimension mutation at continuation {} has no fixed publication slot",
                            instruction.continuation_id,
                        ),
                    ));
                }
                _ => unreachable!("property dimension mutation was filtered above"),
            };
            if dimensions.is_empty() && !append {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_PROPERTY_DIM_MUTATION_ARITY",
                    format!(
                        "property dimension mutation at continuation {} has no dimension",
                        instruction.continuation_id,
                    ),
                ));
            }
            let normalized = dimensions
                .iter()
                .map(|key| publication_integer_array_key(constants, *key))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PROPERTY_DIM_MUTATION_KEY_NORMALIZATION",
                        format!(
                            "property dimension mutation at continuation {} has a key that is not normalized at publication",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let mutation = if append {
                NativeEntryArrayMutationRequirement::Append {
                    parents: normalized,
                }
            } else {
                let (&key, parents) = normalized
                    .split_last()
                    .expect("non-append property mutation retains a key");
                if matches!(operation, RegionSemanticOp::PropertyDimAssign { .. }) {
                    NativeEntryArrayMutationRequirement::Assign {
                        parents: parents.to_vec(),
                        key,
                    }
                } else {
                    NativeEntryArrayMutationRequirement::Unset {
                        parents: parents.to_vec(),
                        key,
                    }
                }
            };
            match operation {
                RegionSemanticOp::PropertyDimAssign { object, .. }
                | RegionSemanticOp::PropertyDimUnset { object, .. } => {
                    let NativeEntryArraySource::Parameter(parameter_index) =
                        entry_array_source(*object, &definitions, &parameter_indices).ok_or_else(
                            || {
                                CraneliftLoweringError::new(
                                    "JIT_CRANELIFT_REJECT_PROPERTY_DIM_MUTATION_OBJECT",
                                    format!(
                                        "property dimension mutation at continuation {} is not rooted at an entry object",
                                        instruction.continuation_id,
                                    ),
                                )
                            },
                        )?
                    else {
                        unreachable!("operand-rooted property objects are parameters")
                    };
                    if by_ref_parameters.contains(&region.parameter_locals[parameter_index]) {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_PROPERTY_DIM_MUTATION_OBJECT_REFERENCE",
                            format!(
                                "property dimension mutation at continuation {} has no plain object owner",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    admission
                        .property_requirements
                        .push(NativeEntryPropertyRequirement {
                            parameter_index,
                            continuation_id: instruction.continuation_id,
                            required_state:
                                crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE,
                            readable: true,
                            releasable: false,
                            allow_reference: false,
                            require_reference: false,
                            probe_paths: Vec::new(),
                            mutations: vec![mutation],
                        });
                }
                RegionSemanticOp::StaticPropertyDimUnset { .. } => {
                    admission.static_property_requirements.push(
                        NativeEntryStaticPropertyRequirement {
                            continuation_id: instruction.continuation_id,
                            required_state: crate::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_WRITABLE,
                            readable: true,
                            releasable: false,
                            allow_reference: false,
                            require_reference: false,
                            probe_paths: Vec::new(),
                            mutations: vec![mutation],
                        },
                    );
                }
                _ => unreachable!("property dimension mutation was filtered above"),
            }
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let RegionCallTarget::Semantic { operation } = &call.target
            && matches!(
                operation,
                RegionSemanticOp::PropertyDimIsset { dimensions, .. }
                    | RegionSemanticOp::PropertyDimEmpty { dimensions, .. }
                    | RegionSemanticOp::StaticPropertyDimIsset { dimensions, .. }
                    | RegionSemanticOp::StaticPropertyDimEmpty { dimensions, .. }
                    if !dimensions.is_empty()
            )
        {
            if matches!(
                operation,
                RegionSemanticOp::PropertyDimIsset {
                    property: RegionPropertyName::Dynamic(_),
                    ..
                } | RegionSemanticOp::PropertyDimEmpty {
                    property: RegionPropertyName::Dynamic(_),
                    ..
                }
            ) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DYNAMIC_PROPERTY_DIM_PUBLICATION",
                    format!(
                        "dynamic property dimension probe at continuation {} has no fixed publication slot",
                        instruction.continuation_id,
                    ),
                ));
            }
            if matches!(
                operation,
                RegionSemanticOp::StaticPropertyDimIsset {
                    class_name: crate::region_ir::RegionClassName::Dynamic(_),
                    ..
                } | RegionSemanticOp::StaticPropertyDimEmpty {
                    class_name: crate::region_ir::RegionClassName::Dynamic(_),
                    ..
                }
            ) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DYNAMIC_STATIC_PROPERTY_DIM_PUBLICATION",
                    format!(
                        "runtime-class static property dimension probe at continuation {} has no fixed publication slot",
                        instruction.continuation_id,
                    ),
                ));
            }
            let dimensions = match operation {
                RegionSemanticOp::PropertyDimIsset { dimensions, .. }
                | RegionSemanticOp::PropertyDimEmpty { dimensions, .. }
                | RegionSemanticOp::StaticPropertyDimIsset { dimensions, .. }
                | RegionSemanticOp::StaticPropertyDimEmpty { dimensions, .. } => dimensions,
                _ => unreachable!("property dimension probe was filtered above"),
            };
            let normalized = dimensions
                .iter()
                .copied()
                .map(|key| publication_native_array_key(constants, key))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_PROPERTY_DIM_KEY_NORMALIZATION",
                        format!(
                            "property dimension probe at continuation {} has a key that is not normalized at publication",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let probe = NativeEntryArrayProbeRequirement {
                keys: normalized,
                leaf: NativeEntryArrayProbeLeaf::PlainValue,
            };
            match operation {
                RegionSemanticOp::PropertyDimIsset {
                    object,
                    property: RegionPropertyName::Static(_),
                    ..
                }
                | RegionSemanticOp::PropertyDimEmpty {
                    object,
                    property: RegionPropertyName::Static(_),
                    ..
                } => {
                    let NativeEntryArraySource::Parameter(parameter_index) =
                        entry_array_source(*object, &definitions, &parameter_indices).ok_or_else(
                            || {
                                CraneliftLoweringError::new(
                                    "JIT_CRANELIFT_REJECT_PROPERTY_DIM_OBJECT_SHAPE",
                                    format!(
                                        "property dimension probe at continuation {} is not rooted at an entry object",
                                        instruction.continuation_id,
                                    ),
                                )
                            },
                        )?
                    else {
                        unreachable!("operand-rooted property objects are parameters")
                    };
                    let local = region.parameter_locals[parameter_index];
                    if by_ref_parameters.contains(&local) {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_PROPERTY_DIM_OBJECT_REFERENCE",
                            format!(
                                "property dimension probe at continuation {} has no plain object owner",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    admission
                        .property_requirements
                        .push(NativeEntryPropertyRequirement {
                            parameter_index,
                            continuation_id: instruction.continuation_id,
                            required_state: crate::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
                            readable: true,
                            releasable: false,
                            allow_reference: false,
                            require_reference: false,
                            probe_paths: vec![probe],
                            mutations: Vec::new(),
                        });
                }
                RegionSemanticOp::StaticPropertyDimIsset { .. }
                | RegionSemanticOp::StaticPropertyDimEmpty { .. } => {
                    admission.static_property_requirements.push(
                        NativeEntryStaticPropertyRequirement {
                            continuation_id: instruction.continuation_id,
                            required_state: crate::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_READABLE,
                            readable: true,
                            releasable: false,
                            allow_reference: false,
                            require_reference: false,
                            probe_paths: vec![probe],
                            mutations: Vec::new(),
                        },
                    );
                }
                _ => unreachable!("property dimension probe was filtered above"),
            }
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if stable_builtin_type_predicate(&call.target).is_some() {
            let operand = (call.args.len() == 1)
                .then(|| direct_fixed_builtin_operand(call, 0))
                .flatten()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_TYPE_PREDICATE_PUBLICATION",
                        "type predicate requires one direct positional operand",
                    )
                })?;
            let fact = lowering_operand_fact(value_flow, constants, operand);
            if fact.certainty != crate::region_ir::SsaCertainty::Unknown {
                let guarded = admit_publication_scalar_class(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    operand,
                    fact.class,
                    instruction.continuation_id,
                    "type predicate",
                )?;
                entry_dependent_continuations.extend(guarded.map(|_| instruction.continuation_id));
            }
            admission
                .total_fixed_builtin_calls
                .insert(instruction.continuation_id);
            continue;
        }
        if stable_builtin_length(&call.target).is_some() {
            let operand = (call.args.len() == 1)
                .then(|| direct_fixed_builtin_operand(call, 0))
                .flatten()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_LENGTH_PUBLICATION",
                        "strlen requires one direct positional operand",
                    )
                })?;
            let guarded = admit_publication_scalar_class(
                &mut admission,
                value_flow,
                constants,
                &definitions,
                &parameter_indices,
                operand,
                SsaValueClass::StringHandle,
                instruction.continuation_id,
                "strlen",
            )?;
            entry_dependent_continuations.extend(guarded.map(|_| instruction.continuation_id));
            admission
                .total_fixed_builtin_calls
                .insert(instruction.continuation_id);
            continue;
        }
        if let Some(operation) = stable_builtin_symbol_query(&call.target) {
            if matches!(
                operation,
                StableSymbolQueryBuiltin::Define | StableSymbolQueryBuiltin::Constant
            ) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_SYMBOL_MUTATION_PUBLICATION",
                    format!(
                        "{operation:?} at continuation {} depends on mutable symbol state or source-aware failure",
                        instruction.continuation_id,
                    ),
                ));
            }
            if !operation.accepts_arity(call.args.len()) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_SYMBOL_QUERY_ARITY",
                    "symbol query has an unsupported argument count",
                ));
            }
            let arguments = (0..call.args.len())
                .map(|index| {
                    direct_fixed_builtin_operand(call, index).ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_SYMBOL_QUERY_ARGUMENT_FRAME",
                            "symbol query arguments are not direct positional values",
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            match operation {
                StableSymbolQueryBuiltin::Defined | StableSymbolQueryBuiltin::FunctionExists => {
                    admit_publication_scalar_class(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        arguments[0],
                        SsaValueClass::StringHandle,
                        instruction.continuation_id,
                        "symbol query name",
                    )?;
                }
                StableSymbolQueryBuiltin::ClassExists
                | StableSymbolQueryBuiltin::InterfaceExists
                | StableSymbolQueryBuiltin::TraitExists
                | StableSymbolQueryBuiltin::EnumExists => {
                    admit_publication_scalar_class(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        arguments[0],
                        SsaValueClass::StringHandle,
                        instruction.continuation_id,
                        "class-kind query name",
                    )?;
                    if arguments.len() != 2
                        || publication_bool_operand(constants, &definitions, arguments[1])
                            != Some(false)
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_SYMBOL_QUERY_AUTOLOAD",
                            format!(
                                "class-kind query at continuation {} must publish autoload=false before optimizer entry",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                }
                StableSymbolQueryBuiltin::MethodExists
                | StableSymbolQueryBuiltin::PropertyExists => {
                    let owner = lowering_operand_fact(value_flow, constants, arguments[0]);
                    if owner.certainty == crate::region_ir::SsaCertainty::Unknown
                        || !matches!(
                            owner.class,
                            SsaValueClass::ObjectHandle | SsaValueClass::StringHandle
                        )
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_MEMBER_QUERY_OWNER",
                            "member query owner is not a published object or class-name string",
                        ));
                    }
                    admit_publication_scalar_class(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        arguments[0],
                        owner.class,
                        instruction.continuation_id,
                        "member query owner",
                    )?;
                    admit_publication_scalar_class(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        arguments[1],
                        SsaValueClass::StringHandle,
                        instruction.continuation_id,
                        "member query name",
                    )?;
                }
                StableSymbolQueryBuiltin::Define | StableSymbolQueryBuiltin::Constant => {
                    unreachable!("symbol mutation/value lookup was rejected above")
                }
            }
            admission
                .total_fixed_builtin_calls
                .insert(instruction.continuation_id);
            continue;
        }
        if let Some(operation) = stable_builtin_resource_query(&call.target) {
            match operation {
                StableResourceQueryBuiltin::Id | StableResourceQueryBuiltin::Type => {
                    let operand = (call.args.len() == 1)
                        .then(|| direct_fixed_builtin_operand(call, 0))
                        .flatten()
                        .ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_RESOURCE_QUERY_PUBLICATION",
                                "resource id/type query requires one direct positional resource",
                            )
                        })?;
                    let fact = lowering_operand_fact(value_flow, constants, operand);
                    let guarded = if fact.certainty != crate::region_ir::SsaCertainty::Unknown {
                        admit_publication_scalar_class(
                            &mut admission,
                            value_flow,
                            constants,
                            &definitions,
                            &parameter_indices,
                            operand,
                            SsaValueClass::ResourceHandle,
                            instruction.continuation_id,
                            "resource query",
                        )?
                    } else {
                        let parameter_index =
                            publication_entry_parameter(operand, &definitions, &parameter_indices)
                                .ok_or_else(|| {
                                    CraneliftLoweringError::new(
                                        "JIT_CRANELIFT_REJECT_RESOURCE_QUERY_PUBLICATION",
                                        format!(
                                            "resource query at continuation {} has no entry-rooted resource operand",
                                            instruction.continuation_id,
                                        ),
                                    )
                                })?;
                        admission
                            .value_class_requirements
                            .push(NativeEntryValueClassRequirement {
                                parameter_index,
                                class: SsaValueClass::ResourceHandle,
                            });
                        Some(parameter_index)
                    };
                    entry_dependent_continuations
                        .extend(guarded.map(|_| instruction.continuation_id));
                    if operation == StableResourceQueryBuiltin::Type {
                        let parameter_index = guarded.ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_RESOURCE_TYPE_PUBLICATION",
                                format!(
                                    "resource type query at continuation {} is not rooted at an entry resource kind",
                                    instruction.continuation_id,
                                ),
                            )
                        })?;
                        admission
                            .resource_type_requirements
                            .push(NativeEntryResourceTypeRequirement { parameter_index });
                    }
                    admission
                        .total_fixed_builtin_calls
                        .insert(instruction.continuation_id);
                    continue;
                }
                StableResourceQueryBuiltin::All => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_RESOURCE_INVENTORY_PUBLICATION",
                        format!(
                            "get_resources at continuation {} depends on an unbounded live resource inventory",
                            instruction.continuation_id,
                        ),
                    ));
                }
            }
        }
        if let Some(operation) = stable_builtin_memory_query(&call.target) {
            if !operation.accepts_arity(call.args.len()) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_MEMORY_QUERY_ARITY",
                    "memory query has an unsupported argument count",
                ));
            }
            if !call.args.is_empty() {
                let operand = direct_fixed_builtin_operand(call, 0).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_MEMORY_QUERY_ARGUMENT",
                        "memory query flag is not a direct positional boolean",
                    )
                })?;
                let guarded = admit_publication_scalar_class(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    operand,
                    SsaValueClass::Bool,
                    instruction.continuation_id,
                    "memory query flag",
                )?;
                entry_dependent_continuations.extend(guarded.map(|_| instruction.continuation_id));
            }
            admission
                .total_fixed_builtin_calls
                .insert(instruction.continuation_id);
            continue;
        }
        if stable_builtin_ord(&call.target) {
            let operand = (call.args.len() == 1)
                .then(|| direct_fixed_builtin_operand(call, 0))
                .flatten()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ORD_ARGUMENT",
                        "ord requires one direct positional argument",
                    )
                })?;
            match publication_string_length(constants, &definitions, operand) {
                Some(length) if length != 0 => {}
                Some(_) => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ORD_PUBLICATION",
                        format!(
                            "ord at continuation {} receives a fixed empty string",
                            instruction.continuation_id,
                        ),
                    ));
                }
                None => {
                    let fact = lowering_operand_fact(value_flow, constants, operand);
                    let Some(NativeEntryArraySource::Parameter(parameter_index)) =
                        entry_array_source(operand, &definitions, &parameter_indices)
                    else {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_ORD_PUBLICATION",
                            format!(
                                "ord at continuation {} has no entry-rooted native string",
                                instruction.continuation_id,
                            ),
                        ));
                    };
                    if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || fact.class != SsaValueClass::StringHandle
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_ORD_PUBLICATION",
                            format!(
                                "ord at continuation {} has no published string class",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    admission
                        .value_class_requirements
                        .push(NativeEntryValueClassRequirement {
                            parameter_index,
                            class: SsaValueClass::StringHandle,
                        });
                    admission
                        .string_requirements
                        .push(NativeEntryStringRequirement {
                            parameter_index,
                            minimum_length: 1,
                            offset: None,
                            allocation_multiplier: 0,
                        });
                }
            }
            admission
                .total_fixed_builtin_calls
                .insert(instruction.continuation_id);
            continue;
        }
        if stable_builtin_string_position(&call.target).is_some() {
            if !(2..=3).contains(&call.args.len()) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_STRING_POSITION_ARITY",
                    "string position builtin requires two or three positional arguments",
                ));
            }
            let haystack = direct_fixed_builtin_operand(call, 0);
            let needle = direct_fixed_builtin_operand(call, 1);
            let (Some(haystack), Some(needle)) = (haystack, needle) else {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_STRING_POSITION_ARGUMENT",
                    "string position arguments are not direct positional values",
                ));
            };
            let haystack_length = publication_string_length(constants, &definitions, haystack);
            let needle_length = publication_string_length(constants, &definitions, needle);
            let entry_string_parameter = |operand| {
                let fact = lowering_operand_fact(value_flow, constants, operand);
                (fact.certainty != crate::region_ir::SsaCertainty::Unknown
                    && fact.class == SsaValueClass::StringHandle)
                    .then(|| {
                        entry_array_source(operand, &definitions, &parameter_indices).and_then(
                            |source| match source {
                                NativeEntryArraySource::Parameter(parameter_index) => {
                                    Some(parameter_index)
                                }
                                NativeEntryArraySource::TrustedGlobal(_) => None,
                            },
                        )
                    })
                    .flatten()
            };
            let haystack_parameter = haystack_length
                .is_none()
                .then(|| entry_string_parameter(haystack))
                .flatten();
            let needle_parameter = needle_length
                .is_none()
                .then(|| entry_string_parameter(needle))
                .flatten();
            if haystack_length.is_none() && haystack_parameter.is_none()
                || needle_length.is_none() && needle_parameter.is_none()
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_STRING_POSITION_PUBLICATION",
                    "string position operands have no fixed or entry-guarded native strings",
                ));
            }
            let offset = call
                .args
                .get(2)
                .map(|_| {
                    let operand = direct_fixed_builtin_operand(call, 2).ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_STRING_POSITION_OFFSET",
                            "string position offset is not a direct operand",
                        )
                    })?;
                    if let Some(offset) =
                        publication_integer_operand(constants, &definitions, operand)
                    {
                        return Ok(NativeEntryStringOffset::Constant(offset));
                    }
                    let fact = lowering_operand_fact(value_flow, constants, operand);
                    let parameter_index =
                        entry_array_source(operand, &definitions, &parameter_indices)
                            .and_then(|source| match source {
                                NativeEntryArraySource::Parameter(parameter_index) => {
                                    Some(parameter_index)
                                }
                                NativeEntryArraySource::TrustedGlobal(_) => None,
                            })
                            .filter(|_| {
                                fact.certainty != crate::region_ir::SsaCertainty::Unknown
                                    && fact.class == SsaValueClass::Int
                            })
                            .ok_or_else(|| {
                                CraneliftLoweringError::new(
                                    "JIT_CRANELIFT_REJECT_STRING_POSITION_OFFSET",
                                    "string position offset has no fixed or entry-guarded integer",
                                )
                            })?;
                    Ok(NativeEntryStringOffset::Parameter(parameter_index))
                })
                .transpose()?
                .unwrap_or(NativeEntryStringOffset::Constant(0));
            if let Some(haystack_length) = haystack_length {
                if let NativeEntryStringOffset::Constant(offset) = offset {
                    let in_range = if offset < 0 {
                        offset
                            .checked_neg()
                            .and_then(|magnitude| usize::try_from(magnitude).ok())
                            .is_some_and(|magnitude| magnitude <= haystack_length)
                    } else {
                        usize::try_from(offset).is_ok_and(|offset| offset <= haystack_length)
                    };
                    if !in_range {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_STRING_POSITION_OFFSET",
                            "string position offset is outside the fixed haystack",
                        ));
                    }
                }
            }
            if let Some(parameter_index) = haystack_parameter {
                admission
                    .value_class_requirements
                    .push(NativeEntryValueClassRequirement {
                        parameter_index,
                        class: SsaValueClass::StringHandle,
                    });
                admission
                    .string_requirements
                    .push(NativeEntryStringRequirement {
                        parameter_index,
                        minimum_length: 0,
                        offset: Some(offset),
                        allocation_multiplier: 0,
                    });
            } else if matches!(offset, NativeEntryStringOffset::Parameter(_)) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_STRING_POSITION_OFFSET",
                    "dynamic string-position offset requires an entry-rooted haystack",
                ));
            }
            if let Some(parameter_index) = needle_parameter {
                admission
                    .value_class_requirements
                    .push(NativeEntryValueClassRequirement {
                        parameter_index,
                        class: SsaValueClass::StringHandle,
                    });
                admission
                    .string_requirements
                    .push(NativeEntryStringRequirement {
                        parameter_index,
                        minimum_length: 0,
                        offset: None,
                        allocation_multiplier: 0,
                    });
            }
            if let NativeEntryStringOffset::Parameter(parameter_index) = offset {
                admission
                    .value_class_requirements
                    .push(NativeEntryValueClassRequirement {
                        parameter_index,
                        class: SsaValueClass::Int,
                    });
            }
            admission
                .total_fixed_builtin_calls
                .insert(instruction.continuation_id);
            continue;
        }
        if stable_builtin_array_key_exists(&call.target) {
            if call.args.len() != 2
                || call.args.iter().any(|argument| {
                    argument.name.is_some()
                        || argument.unpack
                        || argument.by_ref_local.is_some()
                        || argument.by_ref_dim.is_some()
                        || argument.by_ref_property.is_some()
                        || argument.by_ref_property_dim.is_some()
                })
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_KEY_EXISTS_ARITY",
                    format!(
                        "array_key_exists at continuation {} is not two positional by-value arguments",
                        instruction.continuation_id,
                    ),
                ));
            }
            let key = direct_fixed_builtin_operand(call, 0).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_KEY_EXISTS_KEY",
                    format!(
                        "array_key_exists at continuation {} has no direct key operand",
                        instruction.continuation_id,
                    ),
                )
            })?;
            let array = direct_fixed_builtin_operand(call, 1).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_KEY_EXISTS_ARRAY",
                    format!(
                        "array_key_exists at continuation {} has no direct array operand",
                        instruction.continuation_id,
                    ),
                )
            })?;
            let key = publication_native_array_key(constants, key).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_KEY_EXISTS_NORMALIZATION",
                    format!(
                        "array_key_exists at continuation {} has no publication-normalized key",
                        instruction.continuation_id,
                    ),
                )
            })?;
            let source =
                entry_array_source(array, &definitions, &parameter_indices).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_KEY_EXISTS_SHAPE",
                        format!(
                            "array_key_exists at continuation {} is not rooted at an entry array",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let NativeEntryArraySource::Parameter(source_index) = source else {
                unreachable!("operand-rooted array_key_exists arrays are parameters")
            };
            if by_ref_parameters.contains(&region.parameter_locals[source_index]) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_KEY_EXISTS_REFERENCE",
                    format!(
                        "array_key_exists at continuation {} receives a by-reference array",
                        instruction.continuation_id,
                    ),
                ));
            }
            admission
                .array_requirements
                .entry(source)
                .or_default()
                .probe_paths
                .push(NativeEntryArrayProbeRequirement {
                    keys: vec![key],
                    leaf: NativeEntryArrayProbeLeaf::ExistsOnly,
                });
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let Some(unpack) = call.trailing_unpack_argument()
            && let Some(function) = call.direct_compiled_unpack_target()
        {
            let parameters = &function_params
                .get(&function)
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_UNPACK_TARGET",
                        format!(
                            "compiled unpack call at continuation {} has no published target signature",
                            instruction.continuation_id,
                        ),
                    )
                })?
                .params;
            let operand = call
                .operands
                .get(call.argument_operand_offset.saturating_add(unpack))
                .copied()
                .flatten()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_UNPACK_OPERAND",
                        format!(
                            "compiled unpack call at continuation {} has no direct array operand",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let source =
                entry_array_source(operand, &definitions, &parameter_indices).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_UNPACK_SHAPE",
                        format!(
                            "compiled unpack call at continuation {} is not rooted at an entry array",
                            instruction.continuation_id,
                        ),
                    )
                })?;
            let NativeEntryArraySource::Parameter(source_index) = source else {
                unreachable!("unpack entry arrays are parameters")
            };
            if by_ref_parameters.contains(&region.parameter_locals[source_index]) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_UNPACK_REFERENCE",
                    format!(
                        "compiled unpack call at continuation {} receives a by-reference entry array",
                        instruction.continuation_id,
                    ),
                ));
            }
            let fixed_count = parameters.len().saturating_sub(usize::from(call.variadic));
            let fixed_parameters = parameters
                .iter()
                .take(fixed_count)
                .skip(unpack)
                .enumerate()
                .map(|(index, parameter)| (index, parameter.type_.clone(), parameter.by_ref))
                .collect::<Vec<_>>();
            if fixed_parameters
                .iter()
                .enumerate()
                .any(|(index, _)| parameters[unpack + index].default.is_some())
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_UNPACK_DEFAULT_OWNERSHIP",
                    format!(
                        "compiled unpack call at continuation {} still needs a conditional default owner",
                        instruction.continuation_id,
                    ),
                ));
            }
            let required_length = fixed_parameters.len();
            let variadic_parameter = call
                .variadic
                .then(|| parameters.last().filter(|parameter| parameter.variadic))
                .flatten();
            let requirement = admission.array_requirements.entry(source).or_default();
            requirement.projection_allocations =
                requirement.projection_allocations.saturating_add(1);
            requirement
                .unpack_calls
                .push(NativeEntryArrayUnpackRequirement {
                    required_length,
                    fixed_parameters,
                    tail_start: fixed_count.saturating_sub(unpack),
                    tail_type: variadic_parameter.and_then(|parameter| parameter.type_.clone()),
                    tail_by_reference: variadic_parameter.is_some_and(|parameter| parameter.by_ref),
                });
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if call.args.iter().any(|argument| argument.unpack) {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_UNPACK_PUBLICATION",
                format!(
                    "unclassified unpack call at continuation {} is assigned to baseline before region entry",
                    instruction.continuation_id,
                ),
            ));
        }
        let compiled_parameters = call
            .direct_compiled_target()
            .and_then(|target| function_params.get(&target))
            .map(|metadata| metadata.params.as_slice())
            .or_else(|| {
                let (name, link_index) = match &call.target {
                    RegionCallTarget::Function {
                        name,
                        function: None,
                    } => (Some(name.as_str()), None),
                    RegionCallTarget::Method {
                        function: None,
                        linked_function: Some(link_index),
                        receiver_layout_id: Some(_),
                        ..
                    } => (None, Some(*link_index)),
                    _ => return None,
                };
                external_function_signatures
                    .iter()
                    .find(|signature| {
                        signature.published
                            && (name.is_some_and(|name| {
                                signature
                                    .name
                                    .trim_start_matches('\\')
                                    .eq_ignore_ascii_case(name.trim_start_matches('\\'))
                            }) || link_index == Some(signature.link_index))
                    })
                    .map(|signature| signature.native_params.as_slice())
            });
        if let Some(parameters) = compiled_parameters {
            for (index, operand) in call
                .operands
                .iter()
                .skip(call.argument_operand_offset)
                .copied()
                .enumerate()
            {
                let Some(operand) = operand else {
                    continue;
                };
                let Some(parameter) = parameters
                    .get(index)
                    .or_else(|| parameters.last().filter(|parameter| parameter.variadic))
                else {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_COMPILED_CALL_ARITY",
                        format!(
                            "compiled call at continuation {} exceeds its published arity",
                            instruction.continuation_id,
                        ),
                    ));
                };
                let fact = lowering_operand_fact(value_flow, constants, operand);
                if parameter
                    .type_
                    .as_ref()
                    .is_some_and(|type_| !optimizing_fact_satisfies_type(fact, type_))
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CALL_COERCION_PUBLICATION",
                        format!(
                            "compiled call argument {index} at continuation {} still requires a runtime scalar coercion",
                            instruction.continuation_id,
                        ),
                    ));
                }
                if parameter.by_ref {
                    continue;
                }
                match fact.class {
                    SsaValueClass::Int
                    | SsaValueClass::Float
                    | SsaValueClass::StringHandle
                    | SsaValueClass::Bool
                    | SsaValueClass::Null
                        if fact.certainty != crate::region_ir::SsaCertainty::Unknown =>
                    {
                        if let Some(NativeEntryArraySource::Parameter(parameter_index)) =
                            entry_array_source(operand, &definitions, &parameter_indices)
                        {
                            admission.value_class_requirements.push(
                                NativeEntryValueClassRequirement {
                                    parameter_index,
                                    class: fact.class,
                                },
                            );
                            entry_dependent_continuations.insert(instruction.continuation_id);
                        }
                    }
                    SsaValueClass::ArrayHandle
                        if fact.certainty != crate::region_ir::SsaCertainty::Unknown =>
                    {
                        let source = publication_entry_array_source(
                            region,
                            &definitions,
                            &parameter_indices,
                            &by_ref_parameters,
                            operand,
                            instruction.continuation_id,
                            "compiled call array argument",
                        )?;
                        admission.array_requirements.entry(source).or_default();
                        entry_dependent_continuations.insert(instruction.continuation_id);
                    }
                    _ if fact.certainty == crate::region_ir::SsaCertainty::Unknown => {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_CALL_VALUE_PUBLICATION",
                            format!(
                                "compiled call argument {index} at continuation {} has no authoritative entry class",
                                instruction.continuation_id,
                            ),
                        ));
                    }
                    _ => {}
                }
            }
        }
        let direct_arguments = (0..call.args.len())
            .map(|index| direct_fixed_builtin_operand(call, index))
            .collect::<Option<Vec<_>>>();
        if (stable_builtin_scalar_consumer(&call.target).is_some()
            || stable_builtin_ctype(&call.target).is_some()
            || stable_builtin_is_numeric(&call.target))
            && direct_arguments.is_none()
        {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_FIXED_SCALAR_ARGUMENT_FRAME",
                format!(
                    "fixed scalar builtin at continuation {} is not a direct positional argument frame",
                    instruction.continuation_id,
                ),
            ));
        }
        if let Some(arguments) = direct_arguments.as_ref() {
            let continuation = instruction.continuation_id;
            macro_rules! admit_known_scalar {
                ($operand:expr, $family:expr) => {{
                    let operand = $operand;
                    let family = $family;
                    let fact = lowering_operand_fact(value_flow, constants, operand);
                    if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                        || !matches!(
                            fact.class,
                            SsaValueClass::Int
                                | SsaValueClass::Float
                                | SsaValueClass::StringHandle
                                | SsaValueClass::Bool
                                | SsaValueClass::Null
                        )
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_SCALAR_PUBLICATION",
                            format!(
                                "{family} at continuation {continuation} has no total scalar class",
                            ),
                        ));
                    }
                    admit_publication_scalar_class(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        operand,
                        fact.class,
                        continuation,
                        family,
                    )?;
                    Ok(fact.class)
                }};
            }

            if matches!(
                stable_builtin_scalar_consumer(&call.target),
                Some(
                    StableScalarConsumerBuiltin::GetType
                        | StableScalarConsumerBuiltin::GetDebugType
                )
            ) {
                if arguments.len() != 1 {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_TYPE_NAME_ARITY",
                        "type-name builtin requires one positional operand",
                    ));
                }
                admit_known_scalar!(arguments[0], "type-name")?;
                admission.fixed_string_bytes = admission.fixed_string_bytes.saturating_add(16);
            }

            if let Some(operation) = stable_builtin_scalar_consumer(&call.target)
                && !matches!(
                    operation,
                    StableScalarConsumerBuiltin::GetType
                        | StableScalarConsumerBuiltin::GetDebugType
                )
            {
                let valid_arity = arguments.len() == 1
                    || operation == StableScalarConsumerBuiltin::IntVal && arguments.len() == 2;
                if !valid_arity {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_SCALAR_CONSUMER_ARITY",
                        "fixed scalar conversion has an unsupported argument count",
                    ));
                }
                let source = arguments[0];
                let fact = lowering_operand_fact(value_flow, constants, source);
                let admitted = match operation {
                    StableScalarConsumerBuiltin::BoolVal => !matches!(
                        fact.class,
                        SsaValueClass::Uninitialized
                            | SsaValueClass::ReferenceHandle
                            | SsaValueClass::MixedHandle
                    ),
                    StableScalarConsumerBuiltin::IntVal | StableScalarConsumerBuiltin::FloatVal => {
                        matches!(
                            fact.class,
                            SsaValueClass::Null
                                | SsaValueClass::Bool
                                | SsaValueClass::Int
                                | SsaValueClass::Float
                                | SsaValueClass::StringHandle
                                | SsaValueClass::ArrayHandle
                                | SsaValueClass::ResourceHandle
                        )
                    }
                    StableScalarConsumerBuiltin::StrVal => matches!(
                        fact.class,
                        SsaValueClass::Null
                            | SsaValueClass::Bool
                            | SsaValueClass::Int
                            | SsaValueClass::Float
                            | SsaValueClass::StringHandle
                            | SsaValueClass::ResourceHandle
                    ),
                    StableScalarConsumerBuiltin::GetType
                    | StableScalarConsumerBuiltin::GetDebugType => unreachable!(),
                };
                if fact.certainty == crate::region_ir::SsaCertainty::Unknown || !admitted {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_SCALAR_CONSUMER_PUBLICATION",
                        format!(
                            "fixed scalar conversion at continuation {continuation} has no total source class"
                        ),
                    ));
                }
                admit_publication_scalar_class(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    source,
                    fact.class,
                    continuation,
                    "fixed scalar conversion",
                )?;
                if let Some(&base) = arguments.get(1) {
                    admit_publication_integer(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        base,
                        2,
                        36,
                        &[],
                        continuation,
                        "intval base",
                    )?;
                }
                if operation == StableScalarConsumerBuiltin::StrVal
                    && fact.class != SsaValueClass::StringHandle
                {
                    admission.fixed_string_bytes = admission
                        .fixed_string_bytes
                        .saturating_add(php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY);
                }
                admission.total_fixed_builtin_calls.insert(continuation);
            }

            if stable_builtin_is_numeric(&call.target) {
                if arguments.len() != 1 {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_IS_NUMERIC_ARITY",
                        "is_numeric requires one direct positional operand",
                    ));
                }
                let fact = lowering_operand_fact(value_flow, constants, arguments[0]);
                if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                    || !matches!(
                        fact.class,
                        SsaValueClass::Int | SsaValueClass::Float | SsaValueClass::StringHandle
                    )
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_IS_NUMERIC_PUBLICATION",
                        "is_numeric has no publication-total numeric/string operand",
                    ));
                }
                admit_publication_scalar_class(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    arguments[0],
                    fact.class,
                    continuation,
                    "is_numeric",
                )?;
                admission.total_fixed_builtin_calls.insert(continuation);
            }

            if stable_builtin_ctype(&call.target).is_some() {
                if arguments.len() != 1 {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CTYPE_ARITY",
                        "ctype predicate requires one direct positional operand",
                    ));
                }
                let fact = lowering_operand_fact(value_flow, constants, arguments[0]);
                if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                    || !matches!(fact.class, SsaValueClass::Int | SsaValueClass::StringHandle)
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CTYPE_PUBLICATION",
                        "ctype predicate has no publication-total integer/string operand",
                    ));
                }
                admit_publication_scalar_class(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    arguments[0],
                    fact.class,
                    continuation,
                    "ctype predicate",
                )?;
                admission.total_fixed_builtin_calls.insert(continuation);
            }

            if let Some(operation) = stable_builtin_numeric_operator(&call.target) {
                match operation {
                    StableNumericOperatorBuiltin::IntDiv => {
                        if arguments.len() != 2 {
                            return Err(CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_INTDIV_ARITY",
                                "intdiv requires two positional operands",
                            ));
                        }
                        admit_publication_integer(
                            &mut admission,
                            value_flow,
                            constants,
                            &definitions,
                            &parameter_indices,
                            arguments[0],
                            i64::MIN.saturating_add(1),
                            i64::MAX,
                            &[],
                            continuation,
                            "intdiv dividend",
                        )?;
                        admit_publication_integer(
                            &mut admission,
                            value_flow,
                            constants,
                            &definitions,
                            &parameter_indices,
                            arguments[1],
                            i64::MIN,
                            i64::MAX,
                            &[0],
                            continuation,
                            "intdiv divisor",
                        )?;
                    }
                    StableNumericOperatorBuiltin::Round => {
                        let value = arguments.first().copied().ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_ROUND_ARITY",
                                "round requires a numeric operand",
                            )
                        })?;
                        let class = admit_known_scalar!(value, "round")?;
                        if !matches!(class, SsaValueClass::Int | SsaValueClass::Float) {
                            return Err(CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_ROUND_TYPE",
                                "round operand is not publication-proven numeric",
                            ));
                        }
                        if let Some(&precision) = arguments.get(1) {
                            admit_publication_integer(
                                &mut admission,
                                value_flow,
                                constants,
                                &definitions,
                                &parameter_indices,
                                precision,
                                i64::MIN,
                                i64::MAX,
                                &[],
                                continuation,
                                "round precision",
                            )?;
                        }
                        if let Some(&mode) = arguments.get(2) {
                            admit_publication_integer(
                                &mut admission,
                                value_flow,
                                constants,
                                &definitions,
                                &parameter_indices,
                                mode,
                                1,
                                8,
                                &[],
                                continuation,
                                "round mode",
                            )?;
                        }
                    }
                    StableNumericOperatorBuiltin::Pow => {
                        for operand in arguments {
                            let root = publication_root_operand(*operand, &definitions);
                            let fact = fixed_numeric_operand_fact(value_flow, constants, root)
                                .ok_or_else(|| {
                                    CraneliftLoweringError::new(
                                        "JIT_CRANELIFT_REJECT_POWER_TYPE",
                                        format!(
                                            "pow operand {operand:?} has no exact publication-time numeric form"
                                        ),
                                    )
                                })?;
                            let original = lowering_operand_fact(value_flow, constants, root);
                            if matches!(original.class, SsaValueClass::Int | SsaValueClass::Float) {
                                admit_publication_scalar_class(
                                    &mut admission,
                                    value_flow,
                                    constants,
                                    &definitions,
                                    &parameter_indices,
                                    *operand,
                                    fact.class,
                                    continuation,
                                    "pow",
                                )?;
                            }
                        }
                    }
                }
            }

            if let Some(operation) = stable_builtin_extrema(&call.target) {
                if arguments.len() == 1 {
                    let source = publication_entry_array_source(
                        region,
                        &definitions,
                        &parameter_indices,
                        &by_ref_parameters,
                        arguments[0],
                        continuation,
                        "extrema",
                    )?;
                    let requirement = admission.array_requirements.entry(source).or_default();
                    requirement.minimum_length = requirement.minimum_length.max(1);
                    requirement.require_plain_values = true;
                    requirement.require_scalar_values = true;
                    entry_dependent_continuations.insert(continuation);
                } else {
                    for &argument in arguments {
                        admit_known_scalar!(
                            argument,
                            match operation {
                                StableExtremaBuiltin::Max => "max",
                                StableExtremaBuiltin::Min => "min",
                            }
                        )?;
                    }
                }
            }

            if let Some(_operation) = stable_builtin_array_lookup(&call.target) {
                if !(2..=3).contains(&arguments.len()) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_LOOKUP_ARGUMENTS",
                        "array lookup is not a two/three-argument positional call",
                    ));
                }
                admit_known_scalar!(arguments[0], "array lookup needle")?;
                if let Some(&strict) = arguments.get(2) {
                    admit_publication_scalar_class(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        strict,
                        SsaValueClass::Bool,
                        continuation,
                        "array lookup strict flag",
                    )?;
                }
            }

            if stable_builtin_substr(&call.target) {
                if !(2..=3).contains(&arguments.len()) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_SUBSTR_ARITY",
                        "substr requires two or three positional operands",
                    ));
                }
                admit_publication_string(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    arguments[0],
                    0,
                    1,
                    continuation,
                    "substr",
                )?;
                admit_publication_integer(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    arguments[1],
                    i64::MIN,
                    i64::MAX,
                    &[],
                    continuation,
                    "substr offset",
                )?;
                if let Some(&requested) = arguments.get(2) {
                    let fact = lowering_operand_fact(value_flow, constants, requested);
                    if fact.class != SsaValueClass::Null {
                        admit_publication_integer(
                            &mut admission,
                            value_flow,
                            constants,
                            &definitions,
                            &parameter_indices,
                            requested,
                            i64::MIN,
                            i64::MAX,
                            &[],
                            continuation,
                            "substr length",
                        )?;
                    }
                }
            }

            if stable_builtin_str_repeat(&call.target) {
                if arguments.len() != 2 {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_STR_REPEAT_ARITY",
                        "str_repeat requires two positional operands",
                    ));
                }
                let count = publication_integer_operand(constants, &definitions, arguments[1])
                    .filter(|count| *count >= 0)
                    .and_then(|count| usize::try_from(count).ok())
                    .ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_STR_REPEAT_PUBLICATION",
                            "str_repeat count is not a fixed nonnegative integer",
                        )
                    })?;
                admit_publication_string(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    arguments[0],
                    0,
                    count,
                    continuation,
                    "str_repeat",
                )?;
            }

            if stable_builtin_substr_count(&call.target) {
                if arguments.len() != 2 {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_SUBSTR_COUNT_ARITY",
                        "substr_count requires two positional operands",
                    ));
                }
                for (operand, minimum, family) in [
                    (arguments[0], 0, "substr_count haystack"),
                    (arguments[1], 1, "substr_count needle"),
                ] {
                    admit_publication_string(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        operand,
                        minimum,
                        0,
                        continuation,
                        family,
                    )?;
                }
            }

            if let Some(operation) = stable_builtin_string_compare(&call.target) {
                let expected = if operation.bounded() { 3 } else { 2 };
                if arguments.len() != expected {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_STRING_COMPARE_ARITY",
                        "string comparison has unsupported arity",
                    ));
                }
                for &operand in &arguments[..2] {
                    admit_publication_string(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        operand,
                        0,
                        0,
                        continuation,
                        "string comparison",
                    )?;
                }
                if operation.bounded() {
                    admit_publication_integer(
                        &mut admission,
                        value_flow,
                        constants,
                        &definitions,
                        &parameter_indices,
                        arguments[2],
                        0,
                        i64::MAX,
                        &[],
                        continuation,
                        "bounded string comparison length",
                    )?;
                }
            }

            if stable_builtin_str_replace(&call.target) {
                if arguments.len() != 3 {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_STR_REPLACE_ARITY",
                        "str_replace requires three positional scalar strings",
                    ));
                }
                let search_length =
                    publication_string_length(constants, &definitions, arguments[0]).ok_or_else(
                        || {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_STR_REPLACE_PUBLICATION",
                                "str_replace search string is not fixed at publication",
                            )
                        },
                    )?;
                let replacement_length =
                    publication_string_length(constants, &definitions, arguments[1]).ok_or_else(
                        || {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_STR_REPLACE_PUBLICATION",
                                "str_replace replacement string is not fixed at publication",
                            )
                        },
                    )?;
                let multiplier = if search_length == 0 {
                    1
                } else {
                    replacement_length.max(1)
                };
                admit_publication_string(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    arguments[2],
                    0,
                    multiplier,
                    continuation,
                    "str_replace subject",
                )?;
            }

            if stable_builtin_explode(&call.target) {
                if arguments.len() != 2 {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_EXPLODE_ARITY",
                        "explode requires two positional strings",
                    ));
                }
                let delimiter_length =
                    publication_string_length(constants, &definitions, arguments[0])
                        .filter(|length| *length != 0)
                        .ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_EXPLODE_PUBLICATION",
                                "explode delimiter is not a fixed nonempty string",
                            )
                        })?;
                let input_length = publication_string_length(constants, &definitions, arguments[1])
                    .ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_EXPLODE_PUBLICATION",
                            "explode input is not a fixed string",
                        )
                    })?;
                let pieces = input_length
                    .checked_div(delimiter_length)
                    .unwrap_or(0)
                    .saturating_add(1);
                admission.fixed_value_allocations = admission
                    .fixed_value_allocations
                    .saturating_add(pieces.saturating_add(1));
                admission.fixed_array_entries = admission.fixed_array_entries.saturating_add(
                    pieces
                        .max(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
                        .next_power_of_two(),
                );
                admission.fixed_string_bytes =
                    admission.fixed_string_bytes.saturating_add(
                        input_length
                            .saturating_add(pieces.saturating_mul(
                                crate::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY as usize,
                            ))
                            .saturating_mul(2),
                    );
            }

            if stable_builtin_implode(&call.target) {
                if arguments.len() != 2 {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_IMPLODE_ARITY",
                        "implode requires a fixed separator and an entry array",
                    ));
                }
                let separator_length =
                    publication_string_length(constants, &definitions, arguments[0]).ok_or_else(
                        || {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_IMPLODE_PUBLICATION",
                                "implode separator is not fixed at publication",
                            )
                        },
                    )?;
                let source = publication_entry_array_source(
                    region,
                    &definitions,
                    &parameter_indices,
                    &by_ref_parameters,
                    arguments[1],
                    continuation,
                    "implode",
                )?;
                let requirement = admission.array_requirements.entry(source).or_default();
                requirement.require_plain_values = true;
                requirement.require_string_values = true;
                requirement.implode_separator_lengths.push(separator_length);
                entry_dependent_continuations.insert(continuation);
            }

            if let Some(_operation) = stable_builtin_ascii_case(&call.target)
                && arguments.len() == 1
            {
                admit_publication_string(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    arguments[0],
                    0,
                    1,
                    continuation,
                    "ASCII case conversion",
                )?;
            }
            if let Some(_operation) = stable_builtin_string_transform(&call.target)
                && arguments.len() == 1
            {
                admit_publication_string(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    arguments[0],
                    0,
                    1,
                    continuation,
                    "string transform",
                )?;
            }
            if stable_builtin_addslashes(&call.target) && arguments.len() == 1 {
                admit_publication_string(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    arguments[0],
                    0,
                    2,
                    continuation,
                    "addslashes",
                )?;
            }
        }
        if let Some(operation) = stable_builtin_array_constructor(&call.target) {
            let arguments = direct_arguments.as_ref().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_CONSTRUCTOR_ARGUMENTS",
                    format!(
                        "array constructor at continuation {} is not fully positional and direct",
                        instruction.continuation_id,
                    ),
                )
            })?;
            let expected = match operation {
                StableArrayConstructorBuiltin::Fill => 3,
                StableArrayConstructorBuiltin::FillKeys
                | StableArrayConstructorBuiltin::Combine => 2,
                StableArrayConstructorBuiltin::Flip => 1,
            };
            if arguments.len() != expected {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_CONSTRUCTOR_ARITY",
                    format!(
                        "array constructor at continuation {} has arity {}, expected {expected}",
                        instruction.continuation_id,
                        arguments.len(),
                    ),
                ));
            }
            if operation == StableArrayConstructorBuiltin::Fill {
                let start = publication_integer_operand(constants, &definitions, arguments[0]);
                let count = publication_integer_operand(constants, &definitions, arguments[1]);
                let (Some(start), Some(count)) = (start, count) else {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_FILL_PUBLICATION",
                        format!(
                            "array_fill at continuation {} has no fixed integer range",
                            instruction.continuation_id,
                        ),
                    ));
                };
                let count = usize::try_from(count).ok().filter(|count| {
                    *count <= crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as usize
                });
                let Some(count) = count else {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_FILL_RANGE",
                        format!(
                            "array_fill at continuation {} exceeds the native result bound",
                            instruction.continuation_id,
                        ),
                    ));
                };
                if count != 0
                    && start
                        .checked_add(i64::try_from(count - 1).unwrap_or(i64::MAX))
                        .is_none()
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_FILL_RANGE",
                        format!(
                            "array_fill at continuation {} overflows its integer-key range",
                            instruction.continuation_id,
                        ),
                    ));
                }
                let value = lowering_operand_fact(value_flow, constants, arguments[2]);
                if value.certainty == crate::region_ir::SsaCertainty::Unknown
                    || matches!(
                        value.class,
                        SsaValueClass::ReferenceHandle | SsaValueClass::MixedHandle
                    )
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_FILL_VALUE",
                        format!(
                            "array_fill at continuation {} has no publication-stable value owner",
                            instruction.continuation_id,
                        ),
                    ));
                }
                let capacity = count
                    .max(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
                    .checked_next_power_of_two()
                    .filter(|capacity| {
                        *capacity <= crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as usize
                    })
                    .ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_ARRAY_FILL_CAPACITY",
                            "array_fill result capacity does not fit the native arena",
                        )
                    })?;
                admission.fixed_array_entries =
                    admission.fixed_array_entries.saturating_add(capacity);
                admission.fixed_value_allocations = admission
                    .fixed_value_allocations
                    .saturating_add(count.saturating_add(1));
            } else {
                let source_count =
                    usize::from(operation == StableArrayConstructorBuiltin::Combine) + 1;
                let mut sources = Vec::with_capacity(source_count);
                for operand in arguments.iter().copied().take(source_count) {
                    let source = publication_entry_array_source(
                        region,
                        &definitions,
                        &parameter_indices,
                        &by_ref_parameters,
                        operand,
                        instruction.continuation_id,
                        "array constructor",
                    )?;
                    let requirement = admission.array_requirements.entry(source).or_default();
                    requirement.require_plain_values = true;
                    requirement.projection_allocations =
                        requirement.projection_allocations.saturating_add(1);
                    sources.push(source);
                }
                let keys = admission
                    .array_requirements
                    .get_mut(&sources[0])
                    .expect("constructor source was inserted");
                keys.require_key_values = true;
                if operation == StableArrayConstructorBuiltin::Combine {
                    admission.equal_array_length_groups.push(sources);
                }
            }
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let Some(operation) = stable_builtin_array_shape(&call.target) {
            let arguments = direct_arguments.as_ref().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_SHAPE_ARGUMENTS",
                    format!(
                        "array shape operation at continuation {} is not fully positional and direct",
                        instruction.continuation_id,
                    ),
                )
            })?;
            if operation == StableArrayShapeBuiltin::Range {
                if !(2..=3).contains(&arguments.len()) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_RANGE_ARITY",
                        "range requires two or three positional arguments",
                    ));
                }
                let start = publication_integer_operand(constants, &definitions, arguments[0]);
                let end = publication_integer_operand(constants, &definitions, arguments[1]);
                let step = arguments
                    .get(2)
                    .map(|operand| publication_integer_operand(constants, &definitions, *operand))
                    .unwrap_or(Some(1));
                let (Some(start), Some(end), Some(step)) = (start, end, step) else {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_RANGE_PUBLICATION",
                        format!(
                            "range at continuation {} is not fixed before publication",
                            instruction.continuation_id,
                        ),
                    ));
                };
                if step <= 0 {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_RANGE_STEP",
                        "range step is not a positive native integer",
                    ));
                }
                let distance = (i128::from(end) - i128::from(start)).abs();
                let count = distance / i128::from(step) + 1;
                let count = usize::try_from(count).ok().filter(|count| {
                    *count <= crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as usize
                });
                let Some(count) = count else {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_RANGE_CAPACITY",
                        "range result exceeds the native arena",
                    ));
                };
                let capacity = count
                    .max(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
                    .checked_next_power_of_two()
                    .ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_RANGE_CAPACITY",
                            "range result capacity overflowed",
                        )
                    })?;
                admission.fixed_array_entries =
                    admission.fixed_array_entries.saturating_add(capacity);
                admission.fixed_value_allocations = admission
                    .fixed_value_allocations
                    .saturating_add(count.saturating_add(1));
            } else {
                let expected = match operation {
                    StableArrayShapeBuiltin::Pad => 3..=3,
                    StableArrayShapeBuiltin::Chunk | StableArrayShapeBuiltin::Column => 2..=3,
                    StableArrayShapeBuiltin::Unique => 1..=2,
                    StableArrayShapeBuiltin::Range => unreachable!(),
                };
                if !expected.contains(&arguments.len()) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_SHAPE_ARITY",
                        format!(
                            "array shape operation at continuation {} has unsupported arity",
                            instruction.continuation_id,
                        ),
                    ));
                }
                if operation == StableArrayShapeBuiltin::Column {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_COLUMN_NESTED_PUBLICATION",
                        format!(
                            "array_column at continuation {} requires a nested-row publication plan",
                            instruction.continuation_id,
                        ),
                    ));
                }
                let source = publication_entry_array_source(
                    region,
                    &definitions,
                    &parameter_indices,
                    &by_ref_parameters,
                    arguments[0],
                    instruction.continuation_id,
                    "array shape operation",
                )?;
                let requirement = admission.array_requirements.entry(source).or_default();
                requirement.require_plain_values = true;
                requirement.require_supported_keys = true;
                requirement.projection_allocations =
                    requirement.projection_allocations.saturating_add(1);
                match operation {
                    StableArrayShapeBuiltin::Pad => {
                        let requested =
                            publication_integer_operand(constants, &definitions, arguments[1])
                                .filter(|value| *value != i64::MIN)
                                .ok_or_else(|| {
                                    CraneliftLoweringError::new(
                                        "JIT_CRANELIFT_REJECT_ARRAY_PAD_PUBLICATION",
                                        "array_pad length is not a fixed valid integer",
                                    )
                                })?;
                        let magnitude =
                            usize::try_from(requested.unsigned_abs()).map_err(|_| {
                                CraneliftLoweringError::new(
                                    "JIT_CRANELIFT_REJECT_ARRAY_PAD_CAPACITY",
                                    "array_pad length does not fit the native arena",
                                )
                            })?;
                        let capacity = magnitude
                            .max(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
                            .checked_next_power_of_two()
                            .filter(|capacity| {
                                *capacity <= crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as usize
                            })
                            .ok_or_else(|| {
                                CraneliftLoweringError::new(
                                    "JIT_CRANELIFT_REJECT_ARRAY_PAD_CAPACITY",
                                    "array_pad result exceeds the native arena",
                                )
                            })?;
                        admission.fixed_array_entries =
                            admission.fixed_array_entries.saturating_add(capacity);
                    }
                    StableArrayShapeBuiltin::Chunk => {
                        publication_integer_operand(constants, &definitions, arguments[1])
                            .filter(|size| *size > 0)
                            .ok_or_else(|| {
                                CraneliftLoweringError::new(
                                    "JIT_CRANELIFT_REJECT_ARRAY_CHUNK_PUBLICATION",
                                    "array_chunk size is not a fixed positive integer",
                                )
                            })?;
                        requirement.value_allocations_per_entry =
                            requirement.value_allocations_per_entry.saturating_add(1);
                        requirement.entry_allocations_per_entry =
                            requirement.entry_allocations_per_entry.saturating_add(
                                crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize,
                            );
                    }
                    StableArrayShapeBuiltin::Unique => {
                        if let Some(flags) = arguments.get(1)
                            && publication_integer_operand(constants, &definitions, *flags)
                                != Some(2)
                        {
                            return Err(CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_ARRAY_UNIQUE_FLAGS",
                                "array_unique optimizing publication accepts only SORT_STRING",
                            ));
                        }
                        requirement.require_string_values = true;
                    }
                    StableArrayShapeBuiltin::Column | StableArrayShapeBuiltin::Range => {
                        unreachable!()
                    }
                }
            }
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let Some(operation) = stable_builtin_array_set(&call.target) {
            let arguments = direct_arguments.as_ref().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_SET_ARGUMENTS",
                    "array set operation is not fully positional and direct",
                )
            })?;
            if arguments.is_empty() || operation.requires_two_arrays() && arguments.len() < 2 {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_SET_ARITY",
                    "array set operation has insufficient array arguments",
                ));
            }
            for (index, operand) in arguments.iter().copied().enumerate() {
                let source = publication_entry_array_source(
                    region,
                    &definitions,
                    &parameter_indices,
                    &by_ref_parameters,
                    operand,
                    instruction.continuation_id,
                    "array set operation",
                )?;
                let requirement = admission.array_requirements.entry(source).or_default();
                requirement.require_plain_values = true;
                requirement.require_supported_keys = true;
                requirement.require_string_values |= operation.value_sensitive();
                if index == 0 || operation == StableArraySetBuiltin::Replace {
                    requirement.projection_allocations =
                        requirement.projection_allocations.saturating_add(1);
                }
            }
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let Some(operation) = stable_builtin_callback_neutral_array(&call.target) {
            let arguments = direct_arguments.as_ref().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_CALLBACK_NEUTRAL_ARGUMENTS",
                    "callback-neutral array operation is not positional and direct",
                )
            })?;
            let arrays = match operation {
                StableCallbackNeutralArrayBuiltin::MapNull if arguments.len() >= 2 => {
                    &arguments[1..]
                }
                StableCallbackNeutralArrayBuiltin::FilterTruthy
                    if (1..=2).contains(&arguments.len()) =>
                {
                    &arguments[..1]
                }
                _ => {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_CALLBACK_NEUTRAL_ARITY",
                        "callback-neutral array operation has unsupported arity",
                    ));
                }
            };
            if operation == StableCallbackNeutralArrayBuiltin::MapNull {
                let callback = lowering_operand_fact(value_flow, constants, arguments[0]);
                if callback.certainty == crate::region_ir::SsaCertainty::Unknown
                    || callback.class != SsaValueClass::Null
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_MAP_NULL_PUBLICATION",
                        "array_map callback is not publication-proven null",
                    ));
                }
            } else if let Some(callback) = arguments.get(1) {
                let callback = lowering_operand_fact(value_flow, constants, *callback);
                if callback.certainty == crate::region_ir::SsaCertainty::Unknown
                    || callback.class != SsaValueClass::Null
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_FILTER_NULL_PUBLICATION",
                        "array_filter callback is not publication-proven null",
                    ));
                }
            }
            for operand in arrays {
                let source = publication_entry_array_source(
                    region,
                    &definitions,
                    &parameter_indices,
                    &by_ref_parameters,
                    *operand,
                    instruction.continuation_id,
                    "callback-neutral array operation",
                )?;
                let requirement = admission.array_requirements.entry(source).or_default();
                requirement.require_plain_values = true;
                requirement.require_supported_keys = true;
                requirement.projection_allocations =
                    requirement.projection_allocations.saturating_add(1);
                if operation == StableCallbackNeutralArrayBuiltin::MapNull && arrays.len() > 1 {
                    requirement.value_allocations_per_entry =
                        requirement.value_allocations_per_entry.saturating_add(1);
                    requirement.entry_allocations_per_entry =
                        requirement.entry_allocations_per_entry.saturating_add(
                            arrays
                                .len()
                                .max(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
                                .next_power_of_two(),
                        );
                }
            }
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if stable_builtin_array_slice(&call.target)
            || stable_builtin_array_reverse(&call.target)
            || stable_builtin_array_merge(&call.target)
        {
            let arguments = direct_arguments.as_ref().ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_COPY_ARGUMENTS",
                    "array copy operation is not positional and direct",
                )
            })?;
            let array_operands: &[RegionOperand] = if stable_builtin_array_merge(&call.target) {
                if arguments.is_empty() {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_MERGE_ARITY",
                        "array_merge requires at least one array",
                    ));
                }
                arguments
            } else {
                if stable_builtin_array_slice(&call.target) && !(2..=4).contains(&arguments.len())
                    || stable_builtin_array_reverse(&call.target)
                        && !(1..=2).contains(&arguments.len())
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_COPY_ARITY",
                        "array copy operation has unsupported arity",
                    ));
                }
                &arguments[..1]
            };
            if stable_builtin_array_slice(&call.target) {
                publication_integer_operand(constants, &definitions, arguments[1]).ok_or_else(
                    || {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_ARRAY_SLICE_OFFSET",
                            "array_slice offset is not fixed at publication",
                        )
                    },
                )?;
                if let Some(length) = arguments.get(2) {
                    let fact = lowering_operand_fact(value_flow, constants, *length);
                    if publication_integer_operand(constants, &definitions, *length).is_none()
                        && fact.class != SsaValueClass::Null
                    {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_ARRAY_SLICE_LENGTH",
                            "array_slice length is neither fixed integer nor null",
                        ));
                    }
                }
            }
            for operand in array_operands {
                let source = publication_entry_array_source(
                    region,
                    &definitions,
                    &parameter_indices,
                    &by_ref_parameters,
                    *operand,
                    instruction.continuation_id,
                    "array copy operation",
                )?;
                let requirement = admission.array_requirements.entry(source).or_default();
                requirement.require_plain_values = true;
                requirement.require_supported_keys = true;
                requirement.projection_allocations =
                    requirement.projection_allocations.saturating_add(1);
            }
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let Some(_operation) = stable_builtin_array_lookup(&call.target) {
            let arguments = direct_arguments
                .as_ref()
                .filter(|args| (2..=3).contains(&args.len()))
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_LOOKUP_ARGUMENTS",
                        "array lookup is not a two/three-argument positional call",
                    )
                })?;
            let source = publication_entry_array_source(
                region,
                &definitions,
                &parameter_indices,
                &by_ref_parameters,
                arguments[1],
                instruction.continuation_id,
                "array lookup",
            )?;
            let requirement = admission.array_requirements.entry(source).or_default();
            requirement.require_plain_values = true;
            requirement.require_supported_keys = true;
            requirement.require_scalar_values = true;
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let Some(operation) = stable_builtin_array_pointer(&call.target) {
            if call.args.len() != 1 {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_POINTER_ARITY",
                    "array pointer operation requires one argument",
                ));
            }
            let operand = if operation.is_read_only() {
                direct_fixed_builtin_operand(call, 0).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_POINTER_ARGUMENT",
                        "read-only array pointer has no direct array operand",
                    )
                })?
            } else {
                let local = call.args[0]
                    .by_ref_local
                    .filter(|_| call.args[0].name.is_none() && !call.args[0].unpack)
                    .ok_or_else(|| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_ARRAY_POINTER_LVALUE",
                            "mutating array pointer has no direct local lvalue",
                        )
                    })?;
                RegionOperand::Local(local)
            };
            let source = publication_entry_array_source(
                region,
                &definitions,
                &parameter_indices,
                &by_ref_parameters,
                operand,
                instruction.continuation_id,
                "array pointer operation",
            )?;
            let requirement = admission.array_requirements.entry(source).or_default();
            requirement.require_supported_keys = true;
            requirement.require_plain_values = true;
            if !operation.is_read_only() {
                requirement.projection_allocations =
                    requirement.projection_allocations.saturating_add(1);
            }
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if let Some(operation) = stable_builtin_array_stack(&call.target) {
            if call.args.len() < operation.minimum_arity() {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_STACK_ARITY",
                    "array stack operation has insufficient arguments",
                ));
            }
            let local = call.args[0]
                .by_ref_local
                .filter(|_| call.args[0].name.is_none() && !call.args[0].unpack)
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_STACK_LVALUE",
                        "array stack operation has no direct local lvalue",
                    )
                })?;
            let source = publication_entry_array_source(
                region,
                &definitions,
                &parameter_indices,
                &by_ref_parameters,
                RegionOperand::Local(local),
                instruction.continuation_id,
                "array stack operation",
            )?;
            if call
                .args
                .iter()
                .skip(1)
                .enumerate()
                .any(|(index, argument)| {
                    argument.name.is_some()
                        || argument.unpack
                        || direct_fixed_builtin_operand(call, index + 1).is_none_or(|operand| {
                            let fact = lowering_operand_fact(value_flow, constants, operand);
                            fact.certainty == crate::region_ir::SsaCertainty::Unknown
                                || matches!(
                                    fact.class,
                                    SsaValueClass::ReferenceHandle | SsaValueClass::MixedHandle
                                )
                        })
                })
            {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_STACK_VALUE",
                    "array stack values have no publication-stable ownership",
                ));
            }
            let requirement = admission.array_requirements.entry(source).or_default();
            requirement.require_supported_keys = true;
            requirement.require_plain_values = true;
            requirement.projection_allocations =
                requirement.projection_allocations.saturating_add(1);
            admission.fixed_array_entries = admission.fixed_array_entries.saturating_add(
                call.args
                    .len()
                    .saturating_sub(1)
                    .saturating_mul(crate::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize),
            );
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        if stable_builtin_array_splice(&call.target) {
            if !(2..=4).contains(&call.args.len()) {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_SPLICE_ARITY",
                    "array_splice has unsupported arity",
                ));
            }
            let local = call.args[0]
                .by_ref_local
                .filter(|_| call.args[0].name.is_none() && !call.args[0].unpack)
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_SPLICE_LVALUE",
                        "array_splice has no direct local lvalue",
                    )
                })?;
            let source = publication_entry_array_source(
                region,
                &definitions,
                &parameter_indices,
                &by_ref_parameters,
                RegionOperand::Local(local),
                instruction.continuation_id,
                "array_splice",
            )?;
            let arguments = (1..call.args.len())
                .map(|index| direct_fixed_builtin_operand(call, index))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_SPLICE_ARGUMENTS",
                        "array_splice arguments are not direct positional values",
                    )
                })?;
            publication_integer_operand(constants, &definitions, arguments[0]).ok_or_else(
                || {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_SPLICE_OFFSET",
                        "array_splice offset is not fixed at publication",
                    )
                },
            )?;
            if let Some(length) = arguments.get(1) {
                let fact = lowering_operand_fact(value_flow, constants, *length);
                if publication_integer_operand(constants, &definitions, *length).is_none()
                    && fact.class != SsaValueClass::Null
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_SPLICE_LENGTH",
                        "array_splice length is neither fixed integer nor null",
                    ));
                }
            }
            let requirement = admission.array_requirements.entry(source).or_default();
            requirement.require_supported_keys = true;
            requirement.require_plain_values = true;
            requirement.projection_allocations =
                requirement.projection_allocations.saturating_add(2);
            if let Some(replacement) = arguments.get(2) {
                let replacement_source = publication_entry_array_source(
                    region,
                    &definitions,
                    &parameter_indices,
                    &by_ref_parameters,
                    *replacement,
                    instruction.continuation_id,
                    "array_splice replacement",
                )?;
                let replacement = admission
                    .array_requirements
                    .entry(replacement_source)
                    .or_default();
                replacement.require_plain_values = true;
                replacement.require_supported_keys = true;
                replacement.projection_allocations =
                    replacement.projection_allocations.saturating_add(1);
            }
            admission
                .total_array_calls
                .insert(instruction.continuation_id);
            entry_dependent_continuations.insert(instruction.continuation_id);
            continue;
        }
        let projection = stable_builtin_array_projection(&call.target).is_some();
        let read_only_pointer =
            stable_builtin_array_pointer(&call.target) == Some(StableArrayPointerBuiltin::Key);
        let count_family = matches!(
            stable_builtin_array_aggregate(&call.target),
            Some(StableArrayAggregateBuiltin::Count | StableArrayAggregateBuiltin::SizeOf)
        );
        let direct_count = count_family
            && (call.args.len() == 1
                || call.args.len() == 2
                    && direct_fixed_builtin_operand(call, 1).is_some_and(|mode| {
                        publication_integer_operand(constants, &definitions, mode) == Some(0)
                    }));
        if count_family && !direct_count {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_COUNT_PUBLICATION",
                format!(
                    "count/sizeof at continuation {} has no publication-total non-recursive mode",
                    instruction.continuation_id,
                ),
            ));
        }
        if !projection
            && stable_builtin_array_edge_key(&call.target).is_none()
            && !stable_builtin_array_is_list(&call.target)
            && !read_only_pointer
            && !direct_count
        {
            continue;
        }
        if !(call.args.len() == 1 || direct_count && call.args.len() == 2)
            || call.args.iter().any(|argument| {
                argument.name.is_some()
                    || argument.unpack
                    || argument.by_ref_local.is_some()
                    || argument.by_ref_dim.is_some()
                    || argument.by_ref_property.is_some()
                    || argument.by_ref_property_dim.is_some()
            })
        {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_ARITY_SHAPE",
                format!(
                    "array operation at continuation {} has no total positional by-value argument frame",
                    instruction.continuation_id
                ),
            ));
        }
        let operand = direct_fixed_builtin_operand(call, 0).ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_OPERAND",
                format!(
                    "array operation at continuation {} has no direct operand",
                    instruction.continuation_id
                ),
            )
        })?;
        let fact = lowering_operand_fact(value_flow, constants, operand);
        if fact.certainty == crate::region_ir::SsaCertainty::Unknown
            || fact.class != SsaValueClass::ArrayHandle
        {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_TYPE",
                format!(
                    "array operation at continuation {} has no compile-time array fact",
                    instruction.continuation_id
                ),
            ));
        }
        let source = entry_array_source(operand, &definitions, &parameter_indices).ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_SHAPE",
                format!(
                    "array operation at continuation {} is not provably available at optimizing entry",
                    instruction.continuation_id
                ),
            )
        })?;
        match source {
            NativeEntryArraySource::Parameter(index) => {
                let local = region.parameter_locals[index];
                if by_ref_parameters.contains(&local) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_FAMILY_REFERENCE",
                        format!(
                            "array operation at continuation {} receives a by-reference parameter",
                            instruction.continuation_id
                        ),
                    ));
                }
            }
            NativeEntryArraySource::TrustedGlobal(_) => {
                unreachable!("operand-rooted array families are parameters")
            }
        }
        admission
            .total_array_calls
            .insert(instruction.continuation_id);
        entry_dependent_continuations.insert(instruction.continuation_id);
        admission
            .array_requirements
            .entry(source)
            .or_default()
            .projection_allocations += usize::from(projection);
    }
    if let Some(continuation_id) = entry_dependent_continuations
        .intersection(&unstable_before)
        .next()
        .copied()
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STALE_ENTRY_PLAN",
            format!(
                "continuation {continuation_id} depends on entry data after an external effect or control-flow join",
            ),
        ));
    }
    if let Some(return_type) = region.return_type.as_ref()
        && optimizing_type_has_direct_guard(return_type)
    {
        for block in &region.blocks {
            let RegionTerminator::Return {
                value: RegionOperand::Register(register),
                finally: None,
            } = block.terminator
            else {
                continue;
            };
            let Some((source, key)) = admitted_array_fetches.get(&register).copied() else {
                continue;
            };
            if optimizing_fact_satisfies_type(
                lowering_operand_fact(value_flow, constants, RegionOperand::Register(register)),
                return_type,
            ) {
                continue;
            }
            admission
                .array_requirements
                .entry(source)
                .or_default()
                .required_value_types
                .push((key, return_type.clone()));
            admission
                .total_terminators
                .insert(block.terminator_continuation_id);
        }
    }
    let cleanup_total_reference_locals = admission.total_reference_locals.clone();
    let cleanup_local_is_total = |local: LocalId| {
        if !value_flow.releases_local_at_frame_exit(local) {
            return true;
        }
        let storage = value_flow.local_storage(local);
        let fact = value_flow.local_fact(local);
        cleanup_total_reference_locals.contains(&local)
            || by_ref_parameters.contains(&local)
            || storage == crate::region_ir::LocalStorageClass::SsaPlain
                && fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && (matches!(
                    fact.class,
                    SsaValueClass::Float
                        | SsaValueClass::StringHandle
                        | SsaValueClass::ArrayHandle
                        | SsaValueClass::ReferenceHandle
                ) || new_array_locals.contains(&local))
    };
    let operand_root_local = |mut operand: RegionOperand| {
        for _ in 0..=definitions.len() {
            match operand {
                RegionOperand::Local(local) => return Some(local),
                RegionOperand::Register(register) => {
                    operand = *definitions.get(&register)?;
                }
                RegionOperand::Constant(_)
                | RegionOperand::I64(_)
                | RegionOperand::LinkedConstant { .. } => return None,
            }
        }
        None
    };
    let local_store_sources = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            RegionInstructionKind::StoreLocal { local, src } => {
                Some((local, (instruction.continuation_id, src)))
            }
            _ => None,
        })
        .fold(
            BTreeMap::<LocalId, Vec<(u32, RegionOperand)>>::new(),
            |mut sources, (local, write)| {
                sources.entry(local).or_default().push(write);
                sources
            },
        );
    let has_instruction_frame_exit = region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| {
            matches!(
                instruction.kind,
                RegionInstructionKind::NativeControl(RegionNativeControl::Throw { .. })
                    | RegionInstructionKind::NativeControl(RegionNativeControl::EndFinally {
                        outer_finally: None,
                        ..
                    })
            )
        });
    if has_instruction_frame_exit
        && let Some(local) = value_flow
            .frame_cleanup_locals()
            .find(|local| !cleanup_local_is_total(*local))
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DESTRUCTOR_PUBLICATION",
            format!(
                "frame-exit cleanup for local {} can require a PHP-visible destructor or cold owner",
                local.raw(),
            ),
        ));
    }
    for block in &region.blocks {
        let return_plan = region.return_type.as_ref().and_then(|return_type| {
            let (operand, fact) = match block.terminator {
                RegionTerminator::Return {
                    value,
                    finally: None,
                } => (value, lowering_operand_fact(value_flow, constants, value)),
                RegionTerminator::ReturnReference {
                    local,
                    finally: None,
                } => {
                    let sources = local_store_sources.get(&local)?;
                    let [(_, source)] = sources.as_slice() else {
                        return None;
                    };
                    (
                        *source,
                        lowering_operand_fact(value_flow, constants, *source),
                    )
                }
                _ => return None,
            };
            (!optimizing_fact_satisfies_type(fact, return_type))
                .then(|| {
                    publication_return_plan(
                        operand,
                        fact,
                        return_type,
                        region.strict_types,
                        &definitions,
                        constants,
                    )
                })
                .flatten()
        });
        if let Some(plan) = return_plan {
            admission
                .return_plans
                .insert(block.terminator_continuation_id, plan);
            admission.fixed_value_allocations = admission
                .fixed_value_allocations
                .saturating_add(plan.value_allocations());
        }
        if let RegionTerminator::ReturnReference {
            local,
            finally: None,
        } = block.terminator
            && !admission.total_reference_locals.contains(&local)
            && let Some([(store, source)]) = local_store_sources.get(&local).map(Vec::as_slice)
        {
            let fact = lowering_operand_fact(value_flow, constants, *source);
            if fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && !matches!(
                    fact.class,
                    SsaValueClass::ReferenceHandle | SsaValueClass::MixedHandle
                )
            {
                admission.total_return_reference_stores.insert(*store);
                admission.total_return_reference_locals.insert(local);
                admission.fixed_value_allocations =
                    admission.fixed_value_allocations.saturating_add(1);
            }
        }
        let cleanup_total = block
            .terminator_live_locals
            .iter()
            .copied()
            .all(&cleanup_local_is_total);
        let branch_condition_total = match block.terminator {
            RegionTerminator::JumpIfFalse { condition, .. }
            | RegionTerminator::JumpIfTrue { condition, .. }
            | RegionTerminator::JumpIf { condition, .. } => {
                let fact = lowering_operand_fact(value_flow, constants, condition);
                if fact.certainty == crate::region_ir::SsaCertainty::Unknown
                    || matches!(
                        fact.class,
                        SsaValueClass::Uninitialized
                            | SsaValueClass::ReferenceHandle
                            | SsaValueClass::MixedHandle
                    )
                {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_TRUTHINESS_PUBLICATION",
                        format!(
                            "branch at continuation {} has no exact truthiness shape",
                            block.terminator_continuation_id,
                        ),
                    ));
                }
                let guarded = admit_publication_scalar_class(
                    &mut admission,
                    value_flow,
                    constants,
                    &definitions,
                    &parameter_indices,
                    condition,
                    fact.class,
                    block.terminator_continuation_id,
                    "branch truthiness",
                )?;
                entry_dependent_continuations
                    .extend(guarded.map(|_| block.terminator_continuation_id));
                true
            }
            _ => true,
        };
        let total = match block.terminator {
            RegionTerminator::Jump { .. } => true,
            RegionTerminator::JumpIfFalse { .. }
            | RegionTerminator::JumpIfTrue { .. }
            | RegionTerminator::JumpIf { .. } => branch_condition_total,
            RegionTerminator::Return {
                value,
                finally: None,
            } => {
                let return_type_total = region.return_type.as_ref().is_none_or(|return_type| {
                    optimizing_fact_satisfies_type(
                        lowering_operand_fact(value_flow, constants, value),
                        return_type,
                    ) || matches!(return_type, php_ir::IrReturnType::Array)
                        && operand_root_local(value)
                            .is_some_and(|local| new_array_locals.contains(&local))
                        || admission
                            .return_plans
                            .contains_key(&block.terminator_continuation_id)
                        || admission
                            .total_terminators
                            .contains(&block.terminator_continuation_id)
                });
                return_type_total && cleanup_total
            }
            RegionTerminator::ReturnReference {
                local,
                finally: None,
            } => {
                let reference_total = value_flow.local_storage(local).is_reference_slot()
                    && (admission.total_reference_locals.contains(&local)
                        || by_ref_parameters.contains(&local)
                        || admission.total_return_reference_locals.contains(&local));
                let return_type_total = region.return_type.as_ref().is_none_or(|return_type| {
                    optimizing_fact_satisfies_type(
                        lowering_operand_fact(value_flow, constants, RegionOperand::Local(local)),
                        return_type,
                    ) || admission
                        .return_plans
                        .contains_key(&block.terminator_continuation_id)
                        || local_store_sources.get(&local).is_some_and(|sources| {
                            matches!(
                                sources.as_slice(),
                                [(_, source)]
                                    if optimizing_fact_satisfies_type(
                                        lowering_operand_fact(value_flow, constants, *source),
                                        return_type,
                                    )
                            )
                        })
                        || region.params.iter().any(|parameter| {
                            parameter.local == local
                                && parameter.by_ref
                                && parameter.type_.as_ref() == Some(return_type)
                        })
                });
                reference_total && return_type_total && cleanup_total
            }
            RegionTerminator::Exit {
                value,
                finally: None,
            } => {
                let value_total = value.is_none_or(|value| match value {
                    RegionOperand::Local(local)
                        if value_flow.local_storage(local).is_reference_slot() =>
                    {
                        admission.total_reference_locals.contains(&local)
                            || by_ref_parameters.contains(&local)
                    }
                    _ => {
                        lowering_operand_fact(value_flow, constants, value).certainty
                            != crate::region_ir::SsaCertainty::Unknown
                    }
                });
                value_total && cleanup_total
            }
            RegionTerminator::Return {
                finally: Some(_), ..
            }
            | RegionTerminator::ReturnReference {
                finally: Some(_), ..
            }
            | RegionTerminator::Exit {
                finally: Some(_), ..
            } => false,
        };
        if !total {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NON_TOTAL_TERMINATOR_PUBLICATION",
                format!(
                    "terminator at continuation {} has no total native return/type/ownership/cleanup plan",
                    block.terminator_continuation_id,
                ),
            ));
        }
        admission
            .total_terminators
            .insert(block.terminator_continuation_id);
    }
    if let Some(continuation_id) = entry_dependent_continuations
        .intersection(&unstable_before)
        .next()
        .copied()
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_STALE_TRUTHINESS_PLAN",
            format!(
                "truthiness at continuation {continuation_id} depends on entry data after an external effect or control-flow join",
            ),
        ));
    }
    for instruction in region.blocks.iter().flat_map(|block| &block.instructions) {
        let RegionInstructionKind::NativeCall(call) = &instruction.kind else {
            continue;
        };
        let Some(family) = stable_exact_control_builtin_family(&call.target) else {
            continue;
        };
        publish_total_fixed_builtin_plan(
            &mut admission,
            call,
            instruction.continuation_id,
            family,
            value_flow,
            constants,
            &definitions,
            &parameter_indices,
            region
                .params
                .iter()
                .take_while(|parameter| !parameter.variadic)
                .count(),
        )?;
    }
    admission.fixed_lvalue_insertions = admission
        .array_requirements
        .values()
        .flat_map(|requirement| &requirement.mutations)
        .chain(
            admission
                .property_requirements
                .iter()
                .flat_map(|requirement| &requirement.mutations),
        )
        .chain(
            admission
                .static_property_requirements
                .iter()
                .flat_map(|requirement| &requirement.mutations),
        )
        .map(NativeEntryArrayMutationRequirement::additional_entries)
        .sum();
    Ok(admission)
}

#[derive(Clone, Debug)]
struct NativeFragmentLayout {
    id: u32,
    blocks: BTreeSet<BlockId>,
    normal_entries: BTreeSet<BlockId>,
    external_targets: BTreeSet<BlockId>,
    locals: BTreeSet<LocalId>,
    registers: BTreeSet<RegId>,
    stored_registers: BTreeSet<RegId>,
}

#[derive(Clone, Debug)]
struct NativeFunctionFragmentLayout {
    fragments: Vec<NativeFragmentLayout>,
    block_owner: BTreeMap<BlockId, u32>,
    resume_owner: BTreeMap<i32, u32>,
    frame: NativeFragmentFrameLayout,
    register_liveness: NativeRegisterLiveness,
}

#[derive(Clone, Debug)]
struct NativeFragmentFrameLayout {
    local_slots: BTreeMap<LocalId, usize>,
    register_slots: BTreeMap<(u32, RegId), usize>,
    shared_register_slots: usize,
    scratch_register_slots: usize,
    value_slots: usize,
}

#[derive(Clone, Copy)]
struct NativeFragmentDefinition<'a> {
    layout: &'a NativeFunctionFragmentLayout,
    fragment: &'a NativeFragmentLayout,
    functions: &'a BTreeMap<u32, FuncId>,
}

impl NativeFragmentFrameLayout {
    fn for_fragments(
        region: &RegionGraph,
        fragments: &[NativeFragmentLayout],
        shared_registers: &BTreeSet<RegId>,
    ) -> Self {
        let mut locals = (0..region.local_count)
            .map(LocalId::new)
            .collect::<BTreeSet<_>>();
        for block in &region.blocks {
            locals.extend(block.entry_state_locals.iter().copied());
            locals.extend(block.terminator_state_locals.iter().copied());
            locals.extend(block.terminator_live_locals.iter().copied());
            for instruction in &block.instructions {
                locals.extend(instruction.live_locals.iter().copied());
            }
        }
        let local_slots = locals
            .into_iter()
            .enumerate()
            .map(|(slot, local)| (local, slot))
            .collect::<BTreeMap<_, _>>();
        let shared_base = local_slots.len();
        let shared_slots = shared_registers
            .iter()
            .enumerate()
            .map(|(slot, register)| (*register, shared_base.saturating_add(slot)))
            .collect::<BTreeMap<_, _>>();
        let scratch_base = shared_base.saturating_add(shared_slots.len());
        let mut register_slots = BTreeMap::new();
        let mut scratch_register_slots = 0_usize;
        for fragment in fragments {
            let mut next_scratch = 0_usize;
            for register in &fragment.stored_registers {
                let slot = shared_slots.get(register).copied().unwrap_or_else(|| {
                    let slot = scratch_base.saturating_add(next_scratch);
                    next_scratch = next_scratch.saturating_add(1);
                    slot
                });
                register_slots.insert((fragment.id, *register), slot);
            }
            scratch_register_slots = scratch_register_slots.max(next_scratch);
        }
        let value_slots = scratch_base.saturating_add(scratch_register_slots);
        Self {
            local_slots,
            register_slots,
            shared_register_slots: shared_slots.len(),
            scratch_register_slots,
            value_slots,
        }
    }

    fn frame_bytes(&self) -> Result<u32, CraneliftLoweringError> {
        let slots = u64::try_from(self.value_slots)
            .unwrap_or(u64::MAX)
            .saturating_add(8);
        let bytes = slots.saturating_mul(8);
        let bytes = u32::try_from(bytes).map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_FRAGMENT_FRAME_SIZE",
                format!("native fragment frame requires {bytes} bytes"),
            )
        })?;
        if bytes > MAX_NATIVE_SPILL_FRAME_BYTES {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_FRAGMENT_FRAME_LIMIT",
                format!(
                    "native fragment frame requires {bytes} bytes; limit is {MAX_NATIVE_SPILL_FRAME_BYTES}"
                ),
            ));
        }
        Ok(bytes.max(16))
    }

    fn local_offset(&self, local: LocalId) -> Result<i32, CraneliftLoweringError> {
        self.local_slots
            .get(&local)
            .copied()
            .and_then(|slot| i32::try_from(slot.saturating_mul(8)).ok())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_FRAGMENT_LOCAL_SLOT",
                    format!("local {} has no compact fragment-frame slot", local.raw()),
                )
            })
    }

    fn register_offset(
        &self,
        fragment: u32,
        register: RegId,
    ) -> Result<i32, CraneliftLoweringError> {
        self.register_slots
            .get(&(fragment, register))
            .copied()
            .and_then(|slot| i32::try_from(slot.saturating_mul(8)).ok())
            .ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_FRAGMENT_REGISTER_SLOT",
                    format!(
                        "register {} has no compact slot in fragment {fragment}",
                        register.raw(),
                    ),
                )
            })
    }

    fn register_offset_if_present(&self, fragment: u32, register: RegId) -> Option<i32> {
        self.register_slots
            .get(&(fragment, register))
            .copied()
            .and_then(|slot| i32::try_from(slot.saturating_mul(8)).ok())
    }

    fn control_offset(&self, index: usize) -> i32 {
        i32::try_from(self.value_slots.saturating_add(index).saturating_mul(8)).unwrap_or(i32::MAX)
    }

    fn pending_status_offset(&self) -> i32 {
        self.control_offset(0)
    }
    fn pending_value_offset(&self) -> i32 {
        self.control_offset(1)
    }
    fn entry_id_offset(&self) -> i32 {
        self.control_offset(2)
    }
    fn arguments_offset(&self) -> i32 {
        self.control_offset(3)
    }
    fn result_out_offset(&self) -> i32 {
        self.control_offset(4)
    }
    fn deopt_out_offset(&self) -> i32 {
        self.control_offset(5)
    }
    fn resume_id_offset(&self) -> i32 {
        self.control_offset(6)
    }
    fn resume_state_offset(&self) -> i32 {
        self.control_offset(7)
    }
}

fn region_control_targets(block: &crate::region_ir::RegionBlock) -> BTreeSet<BlockId> {
    let mut targets = native_transition_successors(&block.terminator)
        .into_iter()
        .collect::<BTreeSet<_>>();
    match block.terminator {
        RegionTerminator::Return { finally, .. }
        | RegionTerminator::ReturnReference { finally, .. }
        | RegionTerminator::Exit { finally, .. } => {
            targets.extend(finally);
        }
        RegionTerminator::Jump { .. }
        | RegionTerminator::JumpIfFalse { .. }
        | RegionTerminator::JumpIfTrue { .. }
        | RegionTerminator::JumpIf { .. } => {}
    }
    for instruction in &block.instructions {
        if let RegionInstructionKind::NativeControl(control) = &instruction.kind {
            match control {
                RegionNativeControl::EndFinally {
                    after,
                    outer_finally,
                } => {
                    targets.insert(*after);
                    targets.extend(*outer_finally);
                }
                RegionNativeControl::Throw { .. } => {}
                RegionNativeControl::EnterTry { .. }
                | RegionNativeControl::LeaveTry
                | RegionNativeControl::MakeException { .. } => {}
            }
        }
    }
    targets
}

fn region_block_entry_continuation(block: &crate::region_ir::RegionBlock) -> u32 {
    block.entry_continuation_id
}

impl NativeFunctionFragmentLayout {
    fn for_plan(
        region: &RegionGraph,
        plan: &NativeCompilePlan,
    ) -> Result<Self, CraneliftLoweringError> {
        let mut block_owner = BTreeMap::new();
        for fragment in &plan.fragments {
            for block in &fragment.blocks {
                if block_owner.insert(*block, fragment.id).is_some() {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_DUPLICATE_BLOCK",
                        format!("Region block {} occurs in multiple fragments", block.raw()),
                    ));
                }
            }
        }
        if block_owner.len() != region.blocks.len() {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_FRAGMENT_INCOMPLETE_PLAN",
                format!(
                    "fragment plan owns {} of {} Region blocks",
                    block_owner.len(),
                    region.blocks.len()
                ),
            ));
        }
        let register_liveness = NativeRegisterLiveness::analyze(region);
        let register_live_in = &register_liveness.block_live_in;
        let mut fragments = plan
            .fragments
            .iter()
            .map(|fragment| {
                // Locals carry PHP reference/destructor semantics and every
                // write must remain observable even when classical liveness
                // says the value is dead. Keep the bounded function-local set
                // until the semantic local-access table can distinguish frame
                // cleanup roots from ordinary values. Registers, which drive
                // the pathological regalloc graph, are fragment-local below.
                let mut locals = (0..region.local_count)
                    .map(LocalId::new)
                    .collect::<BTreeSet<_>>();
                let mut registers = BTreeSet::new();
                let mut stored_registers = BTreeSet::new();
                for block_id in &fragment.blocks {
                    let block = &region.blocks[block_id.index()];
                    let mut block_definitions = BTreeSet::new();
                    locals.extend(block.entry_state_locals.iter().copied());
                    locals.extend(block.terminator_state_locals.iter().copied());
                    locals.extend(block.terminator_live_locals.iter().copied());
                    registers.extend(block.terminator.register_uses());
                    registers.extend(
                        register_live_in
                            .get(block_id)
                            .into_iter()
                            .flatten()
                            .copied(),
                    );
                    stored_registers.extend(
                        register_live_in
                            .get(block_id)
                            .into_iter()
                            .flatten()
                            .copied(),
                    );
                    for instruction in &block.instructions {
                        locals.extend(instruction.live_locals.iter().copied());
                        let uses = instruction.register_uses();
                        registers.extend(uses.iter().copied());
                        // Region liveness deliberately models semantic CFG
                        // state, but executable lowering also contains
                        // synthesized/path-dependent operands. Materialize
                        // every use not dominated by a definition in this
                        // real block; same-block definitions remain cached.
                        stored_registers.extend(
                            uses.into_iter()
                                .filter(|register| !block_definitions.contains(register)),
                        );
                        if instruction_has_sparse_snapshot(
                            instruction,
                            region.compile_metadata.tier,
                        ) {
                            registers.extend(
                                register_liveness
                                    .transition_live
                                    .get(&instruction.continuation_id)
                                    .into_iter()
                                    .flatten()
                                    .copied(),
                            );
                            stored_registers.extend(
                                register_liveness
                                    .transition_live
                                    .get(&instruction.continuation_id)
                                    .into_iter()
                                    .flatten()
                                    .copied(),
                            );
                        }
                        block_definitions
                            .extend(region_instruction_defined_registers(&instruction.kind));
                    }
                    stored_registers.extend(
                        block
                            .terminator
                            .register_uses()
                            .into_iter()
                            .filter(|register| !block_definitions.contains(register)),
                    );
                    if block_terminator_has_native_transition(block, region.compile_metadata.tier) {
                        registers.extend(
                            register_liveness
                                .transition_live
                                .get(&block.terminator_continuation_id)
                                .into_iter()
                                .flatten()
                                .copied(),
                        );
                        stored_registers.extend(
                            register_liveness
                                .transition_live
                                .get(&block.terminator_continuation_id)
                                .into_iter()
                                .flatten()
                                .copied(),
                        );
                    }
                }
                // Region lowering can synthesize results that do not exist in
                // the source IR (for example the discarded result of a
                // property unset). Declare the executable definitions even
                // when their first use is outside this fragment.
                for block_id in &fragment.blocks {
                    for instruction in &region.blocks[block_id.index()].instructions {
                        registers.extend(region_instruction_defined_registers(&instruction.kind));
                    }
                }
                NativeFragmentLayout {
                    id: fragment.id,
                    blocks: fragment.blocks.iter().copied().collect(),
                    normal_entries: BTreeSet::new(),
                    external_targets: BTreeSet::new(),
                    locals,
                    registers,
                    stored_registers,
                }
            })
            .collect::<Vec<_>>();
        if let Some(owner) = block_owner.get(&BlockId::new(0)).copied() {
            fragments[owner as usize]
                .normal_entries
                .insert(BlockId::new(0));
        }
        let mut shared_registers = BTreeSet::new();
        for block in &region.blocks {
            let source_owner = block_owner[&block.id];
            for target in region_control_targets(block) {
                let target_owner = block_owner.get(&target).copied().ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_UNKNOWN_TARGET",
                        format!(
                            "Region block {} targets missing block {}",
                            block.id.raw(),
                            target.raw()
                        ),
                    )
                })?;
                if source_owner != target_owner {
                    fragments[source_owner as usize]
                        .external_targets
                        .insert(target);
                    fragments[target_owner as usize]
                        .normal_entries
                        .insert(target);
                    shared_registers
                        .extend(register_live_in.get(&target).into_iter().flatten().copied());
                }
                fragments[source_owner as usize]
                    .stored_registers
                    .extend(register_live_in.get(&target).into_iter().flatten().copied());
            }
        }

        let transition_liveness = &register_liveness.transition_live;
        let mut resume_owner = BTreeMap::new();
        let mut insert_resume = |resume_id: i32, block: BlockId| {
            let owner = block_owner[&block];
            match resume_owner.insert(resume_id, owner) {
                Some(previous) if previous != owner => Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_FRAGMENT_RESUME_COLLISION",
                    format!("resume id {resume_id} belongs to fragments {previous} and {owner}"),
                )),
                _ => Ok(()),
            }
        };
        for handler in &region.exception_regions {
            for target in [handler.catch, handler.finally].into_iter().flatten() {
                insert_resume(crate::native_handler_resume_id(target), target)?;
            }
        }
        for block in &region.blocks {
            if region.compile_metadata.tier == NativeCompilerTier::Optimizing {
                insert_resume(
                    crate::native_optimizing_continuation_resume_id(
                        region_block_entry_continuation(block),
                    ),
                    block.id,
                )?;
            }
            if block_terminator_has_native_transition(block, region.compile_metadata.tier)
                && transition_liveness
                    .get(&block.terminator_continuation_id)
                    .is_some_and(|live| live.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
            {
                insert_resume(
                    crate::native_transition_resume_id(block.terminator_continuation_id),
                    block.id,
                )?;
            }
            for instruction in &block.instructions {
                if matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_)) {
                    insert_resume(
                        crate::native_suspension_resume_id(instruction.continuation_id),
                        block.id,
                    )?;
                }
                if instruction_has_native_resume_entry(instruction, region.compile_metadata.tier)
                    && transition_liveness
                        .get(&instruction.continuation_id)
                        .is_some_and(|live| live.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
                {
                    insert_resume(
                        crate::native_transition_resume_id(instruction.continuation_id),
                        block.id,
                    )?;
                }
            }
        }
        for osr in region.osr_entries() {
            insert_resume(
                i32::try_from(osr.id).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_OSR_ID",
                        format!("OSR id {} does not fit the native resume ABI", osr.id),
                    )
                })?,
                osr.block,
            )?;
        }
        let frame = NativeFragmentFrameLayout::for_fragments(region, &fragments, &shared_registers);
        Ok(Self {
            fragments,
            block_owner,
            resume_owner,
            frame,
            register_liveness,
        })
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

fn optimizing_compiled_call_params<'a>(
    call: &RegionNativeCall,
    unit: &'a IrUnit,
    external_function_signatures: &'a [crate::JitExternalFunctionSignature],
) -> Option<&'a [php_ir::IrParam]> {
    if let Some(function) = call
        .direct_compiled_target()
        .or_else(|| call.direct_compiled_unpack_target())
    {
        return unit
            .functions
            .get(function.index())
            .map(|function| function.params.as_slice());
    }
    let (name, link_index) = match &call.target {
        RegionCallTarget::Function {
            name,
            function: None,
        } => (Some(name.as_str()), None),
        RegionCallTarget::Method {
            function: None,
            linked_function: Some(link_index),
            receiver_layout_id: Some(_),
            ..
        } => (None, Some(*link_index)),
        _ => return None,
    };
    external_function_signatures
        .iter()
        .find(|signature| {
            signature.published
                && (name.is_some_and(|name| {
                    signature
                        .name
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case(name.trim_start_matches('\\'))
                }) || link_index == Some(signature.link_index))
        })
        .map(|signature| signature.native_params.as_slice())
}

#[derive(Clone, Copy, Default)]
struct OptimizingCallScalarHelperNeeds {
    numeric_string: bool,
    string_cast: bool,
}

fn direct_fixed_builtin_operand(call: &RegionNativeCall, index: usize) -> Option<RegionOperand> {
    call.args
        .get(index)
        .filter(|argument| argument.name.is_none() && !argument.unpack)
        .and_then(|_| {
            call.operands
                .get(call.argument_operand_offset.saturating_add(index))
                .copied()
                .flatten()
        })
}

fn optimizing_strval_uses_float_handler(
    call: &RegionNativeCall,
    value_flow: &ExecutableValueFlow,
    constants: &[IrConstant],
) -> bool {
    stable_builtin_scalar_consumer(&call.target) == Some(StableScalarConsumerBuiltin::StrVal)
        && call.args.len() == 1
        && direct_fixed_builtin_operand(call, 0).is_some_and(|operand| {
            let fact = lowering_operand_fact(value_flow, constants, operand);
            fact.certainty != crate::region_ir::SsaCertainty::Unknown
                && fact.class == SsaValueClass::Float
        })
}

fn optimizing_call_scalar_helper_needs(
    call: &RegionNativeCall,
    unit: &IrUnit,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    value_flow: &ExecutableValueFlow,
    constants: &[IrConstant],
) -> OptimizingCallScalarHelperNeeds {
    use php_ir::IrReturnType as Type;

    let Some(parameters) =
        optimizing_compiled_call_params(call, unit, external_function_signatures)
    else {
        return OptimizingCallScalarHelperNeeds::default();
    };
    let mut needs = OptimizingCallScalarHelperNeeds::default();
    let mut consider = |parameter: &php_ir::IrParam, operand: Option<RegionOperand>| {
        let Some(type_) = parameter.type_.as_ref() else {
            return;
        };
        if !parameter.by_ref
            && operand.is_some_and(|operand| {
                optimizing_fact_satisfies_type(
                    lowering_operand_fact(value_flow, constants, operand),
                    type_,
                )
            })
        {
            return;
        }
        let scalar_type = match type_ {
            Type::Nullable { inner } => inner.as_ref(),
            type_ => type_,
        };
        match (call.caller_strict_types, scalar_type) {
            (false, Type::Int | Type::Float) => {
                needs.numeric_string = true;
            }
            (false, Type::String) => needs.string_cast = true,
            _ => {}
        }
    };

    if let Some(unpack) = call.trailing_unpack_argument() {
        for (index, parameter) in parameters.iter().enumerate() {
            let operand = (index < unpack)
                .then(|| {
                    call.operands
                        .get(call.argument_operand_offset.saturating_add(index))
                        .copied()
                        .flatten()
                })
                .flatten();
            consider(parameter, operand);
        }
    } else {
        for (index, operand) in call
            .operands
            .iter()
            .skip(call.argument_operand_offset)
            .enumerate()
        {
            let Some(parameter) = parameters
                .get(index)
                .or_else(|| parameters.last().filter(|parameter| parameter.variadic))
            else {
                continue;
            };
            consider(parameter, *operand);
        }
    }
    needs
}

fn optimizing_return_scalar_helper_needs(
    region: &RegionGraph,
    value_flow: &ExecutableValueFlow,
    constants: &[IrConstant],
    strict: bool,
) -> OptimizingCallScalarHelperNeeds {
    use php_ir::IrReturnType as Type;

    let Some(return_type) = region.return_type.as_ref() else {
        return OptimizingCallScalarHelperNeeds::default();
    };
    let scalar_type = match return_type {
        Type::Nullable { inner } => inner.as_ref(),
        return_type => return_type,
    };
    let mut needs = OptimizingCallScalarHelperNeeds::default();
    for block in &region.blocks {
        let requires_scalar_boundary = match &block.terminator {
            RegionTerminator::Return {
                value,
                finally: None,
            } => !optimizing_fact_satisfies_type(
                lowering_operand_fact(value_flow, constants, *value),
                return_type,
            ),
            // The local's SSA fact describes the reference container. Its
            // authoritative payload still requires the declared return
            // coercion and writeback boundary.
            RegionTerminator::ReturnReference { finally: None, .. } => true,
            _ => false,
        };
        if !requires_scalar_boundary {
            continue;
        }
        match (strict, scalar_type) {
            (false, Type::Int | Type::Float) => needs.numeric_string = true,
            (false, Type::String) => needs.string_cast = true,
            _ => {}
        }
    }
    needs
}

fn native_transition_successors(terminator: &RegionTerminator) -> Vec<BlockId> {
    match terminator {
        RegionTerminator::Jump { target } => vec![*target],
        RegionTerminator::JumpIfFalse {
            target,
            fallthrough,
            ..
        }
        | RegionTerminator::JumpIfTrue {
            target,
            fallthrough,
            ..
        } => vec![*target, *fallthrough],
        RegionTerminator::JumpIf {
            if_true, if_false, ..
        } => vec![*if_true, *if_false],
        RegionTerminator::Return { .. }
        | RegionTerminator::ReturnReference { .. }
        | RegionTerminator::Exit { .. } => Vec::new(),
    }
}

pub(super) fn instruction_has_native_transition(
    instruction: &RegionInstruction,
    tier: NativeCompilerTier,
) -> bool {
    if tier == NativeCompilerTier::Optimizing {
        return instruction.optimizer_transition_entry;
    }
    // Baseline must publish the exact entry used by an optimizing island
    // exit, including the first instruction of a baseline-only family. The
    // old hand-maintained allow-list covered direct guards but omitted such
    // island heads (for example a static-local operation), so valid optimized
    // code produced a state the corresponding baseline artifact could not
    // enter.
    if instruction.optimizer_transition_entry {
        return true;
    }
    // Checked binary operations can request a baseline retry. A userland call
    // also needs a caller continuation when its callee suspends (for example a
    // Fiber::suspend nested below the call); throw and exit still unwind
    // terminally through the handler table. These are real resumable
    // safepoints, not instruction-per-resume entries.
    matches!(
        instruction.kind,
        RegionInstructionKind::Binary { .. }
            | RegionInstructionKind::Unary { .. }
            | RegionInstructionKind::LoadLocal { .. }
            | RegionInstructionKind::StoreLocal { .. }
            | RegionInstructionKind::AssignLocalResult { .. }
            | RegionInstructionKind::Discard { .. }
            | RegionInstructionKind::IssetLocal { .. }
            | RegionInstructionKind::EmptyLocal { .. }
            | RegionInstructionKind::UnsetLocal { .. }
            | RegionInstructionKind::NewArray { .. }
            | RegionInstructionKind::ArrayInsert { .. }
            | RegionInstructionKind::AppendDim { .. }
            | RegionInstructionKind::IssetDim { .. }
            | RegionInstructionKind::EmptyDim { .. }
            | RegionInstructionKind::FetchDim {
                mode: php_ir::instruction::DimFetchMode::Read,
                ..
            }
            | RegionInstructionKind::ForeachInit { .. }
            | RegionInstructionKind::ForeachNext { .. }
            | RegionInstructionKind::ForeachCleanup { .. }
            | RegionInstructionKind::FetchProperty { .. }
            | RegionInstructionKind::ArrayCallback(_)
            | RegionInstructionKind::PregCallbackArray(_)
            | RegionInstructionKind::NativeCall(_)
            | RegionInstructionKind::NativeDynamicCode(_)
    )
}

fn instruction_has_sparse_snapshot(
    instruction: &RegionInstruction,
    tier: NativeCompilerTier,
) -> bool {
    instruction_has_native_transition(instruction, tier)
        || matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_))
}

/// Whether this artifact can be entered again at the instruction after it
/// has already started executing. Guard failures in optimizing code exit to
/// the baseline artifact; they are not optimizer resume entries. Conflating
/// those two directions forced the normal optimizing path through a distinct
/// CLIF block for every guardable PHP instruction.
fn instruction_has_native_resume_entry(
    instruction: &RegionInstruction,
    tier: NativeCompilerTier,
) -> bool {
    match tier {
        NativeCompilerTier::Baseline => instruction_has_native_transition(instruction, tier),
        NativeCompilerTier::Optimizing => {
            matches!(
                instruction.kind,
                RegionInstructionKind::ArrayCallback(_)
                    | RegionInstructionKind::PregCallbackArray(_)
                    | RegionInstructionKind::NativeCall(_)
            )
        }
    }
}

fn terminator_has_native_transition(terminator: &RegionTerminator) -> bool {
    !matches!(terminator, RegionTerminator::Jump { .. })
}

fn block_terminator_has_native_transition(
    block: &crate::region_ir::RegionBlock,
    _tier: NativeCompilerTier,
) -> bool {
    terminator_has_native_transition(&block.terminator)
        && !block.instructions.iter().any(|instruction| {
            matches!(instruction.kind, RegionInstructionKind::RuntimeFatal { .. })
        })
}

/// Restore the sparse local portion of a native continuation into the compact
/// streaming frame.  The initialization masks are already part of the
/// transition ABI, so one cold loop can serve every handler, suspension, OSR,
/// and tier-transition entry in the fragment.  Emitting the same local-copy
/// sequence into every resume loader made cold state reconstruction dominate
/// the machine code of large baseline functions.
fn emit_streaming_local_restore_loop(
    builder: &mut FunctionBuilder<'_>,
    pointer_type: ir::Type,
    state: ir::Value,
    frame: ir::Value,
    local_count: u32,
    continuation: ir::Block,
) {
    if local_count == 0 {
        builder.ins().jump(continuation, &[]);
        return;
    }

    let header = builder.create_block();
    let test = builder.create_block();
    let copy = builder.create_block();
    let next = builder.create_block();
    for block in [header, test, copy, next] {
        builder.set_cold_block(block);
    }
    builder.append_block_param(header, types::I64);
    builder.append_block_param(test, types::I64);
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(next, types::I64);

    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(header, &[zero.into()]);

    builder.switch_to_block(header);
    let index = builder.block_params(header)[0];
    let in_range = builder
        .ins()
        .icmp_imm(IntCC::UnsignedLessThan, index, i64::from(local_count));
    builder
        .ins()
        .brif(in_range, test, &[index.into()], continuation, &[]);

    builder.switch_to_block(test);
    let index = builder.block_params(test)[0];
    let word = builder.ins().ushr_imm(index, 6);
    let word_bytes = builder.ins().ishl_imm(word, 3);
    let word_bytes = if pointer_type == types::I64 {
        word_bytes
    } else {
        builder.ins().ireduce(pointer_type, word_bytes)
    };
    let mask_base = builder.ins().iadd_imm(
        state,
        std::mem::offset_of!(crate::JitDeoptState, initialized_mask) as i64,
    );
    let mask_address = builder.ins().iadd(mask_base, word_bytes);
    let mask = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), mask_address, 0);
    let bit_index = builder.ins().band_imm(index, 63);
    let one = builder.ins().iconst(types::I64, 1);
    let bit = builder.ins().ishl(one, bit_index);
    let initialized = builder.ins().band(mask, bit);
    let initialized = builder.ins().icmp_imm(IntCC::NotEqual, initialized, 0);
    builder
        .ins()
        .brif(initialized, copy, &[index.into()], next, &[index.into()]);

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let slot_bytes = builder.ins().ishl_imm(index, 3);
    let slot_bytes = if pointer_type == types::I64 {
        slot_bytes
    } else {
        builder.ins().ireduce(pointer_type, slot_bytes)
    };
    let state_slots = builder.ins().iadd_imm(
        state,
        std::mem::offset_of!(crate::JitDeoptState, slots) as i64,
    );
    let state_slot = builder.ins().iadd(state_slots, slot_bytes);
    let frame_slot = builder.ins().iadd(frame, slot_bytes);
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), state_slot, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), value, frame_slot, 0);
    builder.ins().jump(next, &[index.into()]);

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(header, &[index.into()]);
}

/// Publish sparse baseline locals from the compact fragment frame. Every
/// callsite supplies only its static live mask; one cold loop performs the
/// actual copy for all call side exits in the fragment.
fn emit_streaming_local_snapshot_loop(
    builder: &mut FunctionBuilder<'_>,
    pointer_type: ir::Type,
    state: ir::Value,
    frame: ir::Value,
    local_count: u32,
    continuation: ir::Block,
) {
    if local_count == 0 {
        builder.ins().jump(continuation, &[]);
        return;
    }

    let header = builder.create_block();
    let test = builder.create_block();
    let copy = builder.create_block();
    let next = builder.create_block();
    for block in [header, test, copy, next] {
        builder.set_cold_block(block);
    }
    builder.append_block_param(header, types::I64);
    builder.append_block_param(test, types::I64);
    builder.append_block_param(copy, types::I64);
    builder.append_block_param(next, types::I64);

    let zero = builder.ins().iconst(types::I64, 0);
    builder.ins().jump(header, &[zero.into()]);

    builder.switch_to_block(header);
    let index = builder.block_params(header)[0];
    let in_range = builder
        .ins()
        .icmp_imm(IntCC::UnsignedLessThan, index, i64::from(local_count));
    builder
        .ins()
        .brif(in_range, test, &[index.into()], continuation, &[]);

    builder.switch_to_block(test);
    let index = builder.block_params(test)[0];
    let word = builder.ins().ushr_imm(index, 6);
    let word_bytes = builder.ins().ishl_imm(word, 3);
    let word_bytes = if pointer_type == types::I64 {
        word_bytes
    } else {
        builder.ins().ireduce(pointer_type, word_bytes)
    };
    let mask_base = builder.ins().iadd_imm(
        state,
        std::mem::offset_of!(crate::JitDeoptState, initialized_mask) as i64,
    );
    let mask_address = builder.ins().iadd(mask_base, word_bytes);
    let mask = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), mask_address, 0);
    let bit_index = builder.ins().band_imm(index, 63);
    let one = builder.ins().iconst(types::I64, 1);
    let bit = builder.ins().ishl(one, bit_index);
    let initialized = builder.ins().band(mask, bit);
    let initialized = builder.ins().icmp_imm(IntCC::NotEqual, initialized, 0);
    builder
        .ins()
        .brif(initialized, copy, &[index.into()], next, &[index.into()]);

    builder.switch_to_block(copy);
    let index = builder.block_params(copy)[0];
    let slot_bytes = builder.ins().ishl_imm(index, 3);
    let slot_bytes = if pointer_type == types::I64 {
        slot_bytes
    } else {
        builder.ins().ireduce(pointer_type, slot_bytes)
    };
    let state_slots = builder.ins().iadd_imm(
        state,
        std::mem::offset_of!(crate::JitDeoptState, slots) as i64,
    );
    let state_slot = builder.ins().iadd(state_slots, slot_bytes);
    let frame_slot = builder.ins().iadd(frame, slot_bytes);
    let value = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), frame_slot, 0);
    builder
        .ins()
        .store(MemFlagsData::new(), value, state_slot, 0);
    builder.ins().jump(next, &[index.into()]);

    builder.switch_to_block(next);
    let index = builder.block_params(next)[0];
    let index = builder.ins().iadd_imm(index, 1);
    builder.ins().jump(header, &[index.into()]);
}

/// Classical SSA live-in sets for the small set of actual native transition
/// safepoints. This deliberately does not equate "defined earlier" with
/// "live now": doing so creates cumulative register prefixes and quadratic
/// Cranelift move/alias pressure in large PHP functions.
fn native_register_live_in(region: &RegionGraph) -> BTreeMap<BlockId, BTreeSet<RegId>> {
    let block_indices = region
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect::<BTreeMap<_, _>>();
    let mut live_in = vec![BTreeSet::<RegId>::new(); region.blocks.len()];
    loop {
        let mut changed = false;
        for (index, block) in region.blocks.iter().enumerate().rev() {
            let mut live = native_transition_successors(&block.terminator)
                .into_iter()
                .filter_map(|successor| block_indices.get(&successor).copied())
                .flat_map(|successor| live_in[successor].iter().copied())
                .collect::<BTreeSet<_>>();
            live.extend(block.terminator.register_uses());
            live.extend(block.terminator_live_registers.iter().flatten().copied());
            for instruction in block.instructions.iter().rev() {
                for defined in region_instruction_defined_registers(&instruction.kind) {
                    live.remove(&defined);
                }
                live.extend(instruction.register_uses());
                live.extend(
                    instruction
                        .transition_live_registers
                        .iter()
                        .flatten()
                        .copied(),
                );
            }
            if live != live_in[index] {
                live_in[index] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    region
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, live_in[index].clone()))
        .collect()
}

#[derive(Clone, Debug)]
struct NativeRegisterLiveness {
    block_live_in: BTreeMap<BlockId, BTreeSet<RegId>>,
    transition_live: BTreeMap<u32, Vec<RegId>>,
}

impl NativeRegisterLiveness {
    fn analyze(region: &RegionGraph) -> Self {
        let block_live_in = native_register_live_in(region);
        let mut transition_live = BTreeMap::new();
        for block in &region.blocks {
            let mut live = native_transition_successors(&block.terminator)
                .into_iter()
                .filter_map(|successor| block_live_in.get(&successor))
                .flat_map(|registers| registers.iter().copied())
                .collect::<BTreeSet<_>>();
            live.extend(block.terminator.register_uses());
            if block_terminator_has_native_transition(block, region.compile_metadata.tier) {
                transition_live.insert(
                    block.terminator_continuation_id,
                    block
                        .terminator_live_registers
                        .clone()
                        .unwrap_or_else(|| live.iter().copied().collect()),
                );
            }
            for instruction in block.instructions.iter().rev() {
                for defined in region_instruction_defined_registers(&instruction.kind) {
                    live.remove(&defined);
                }
                live.extend(instruction.register_uses());
                live.extend(
                    instruction
                        .transition_live_registers
                        .iter()
                        .flatten()
                        .copied(),
                );
                if instruction_has_sparse_snapshot(instruction, region.compile_metadata.tier) {
                    transition_live.insert(
                        instruction.continuation_id,
                        instruction
                            .transition_live_registers
                            .clone()
                            .unwrap_or_else(|| live.iter().copied().collect()),
                    );
                }
            }
        }
        Self {
            block_live_in,
            transition_live,
        }
    }
}

fn ir_function_requires_trampoline(function: &php_ir::IrFunction) -> bool {
    function.params.iter().any(|parameter| parameter.by_ref)
        || function.returns_by_ref
        || ir_function_requires_non_reference_trampoline(function)
}

fn ir_function_requires_non_reference_trampoline(function: &php_ir::IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                php_ir::InstructionKind::Yield { .. } | php_ir::InstructionKind::YieldFrom { .. }
            ) || matches!(
                &instruction.kind,
                php_ir::InstructionKind::CallFunction { name, .. }
                    if name.trim_start_matches('\\').eq_ignore_ascii_case("debug_backtrace")
            )
        })
    }) || function.attributes.iter().any(|attribute| {
        attribute
            .resolved_name
            .as_deref()
            .or(attribute.fallback_name.as_deref())
            .unwrap_or(&attribute.name)
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("deprecated")
    })
}

fn ir_function_has_exception_handler(function: &php_ir::IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            matches!(
                instruction.kind,
                php_ir::InstructionKind::EnterTry { catch: Some(_), .. }
                    | php_ir::InstructionKind::EnterTry {
                        finally: Some(_),
                        ..
                    }
            )
        })
    })
}

fn declare_baseline_value_operation(
    module: &mut JITModule,
    symbol: &str,
    arity: u8,
    address: usize,
) -> Result<NativeHelper, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I32));
    for _ in 0..arity {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    declare_native_helper(module, symbol, &signature, address)
}

fn declare_native_helper(
    module: &mut JITModule,
    symbol: &str,
    signature: &ir::Signature,
    address: usize,
) -> Result<NativeHelper, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = signature.clone();
    signature.params.insert(0, AbiParam::new(pointer_type));
    let import_symbol = native_helper_import_symbol(symbol, address);
    let function = module
        .declare_function(&import_symbol, Linkage::Import, &signature)
        .map_err(|error| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_OPERATION",
                format!("failed to declare {symbol}: {error}"),
            )
        })?;
    Ok(NativeHelper {
        function,
        terminal_exit: None,
        inline_runtime_view: false,
        runtime: None,
    })
}

fn declare_native_pure_handler(
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
                format!("failed to declare pure {symbol}: {error}"),
            )
        })?;
    Ok(NativeHelper {
        function,
        terminal_exit: None,
        inline_runtime_view: false,
        runtime: None,
    })
}

fn declare_native_control_handler(
    module: &mut JITModule,
    needed: bool,
    symbol: &str,
    argument_count: usize,
    address: impl FnOnce() -> usize,
) -> Result<Option<NativeHelper>, CraneliftLoweringError> {
    if !needed {
        return Ok(None);
    }
    let mut signature = module.make_signature();
    for _ in 0..argument_count {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.returns.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(types::I64));
    declare_native_helper(module, symbol, &signature, address()).map(Some)
}

pub(super) fn compile_region_graph_native(
    unit: &IrUnit,
    region: RegionGraph,
    plan: NativeCompilePlan,
    runtime_helpers: crate::JitRuntimeHelperAddresses,
    request: &JitCompileRequest,
) -> Result<NativeScalarRegionCompileResult, CraneliftLoweringError> {
    validate_region_native_coverage(&region)?;
    region.verify().map_err(|error| {
        CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_REGION_VERIFY", error.to_string())
    })?;
    let function = region.function;
    let runtime_unit_identity = if request.deployment_runtime_identity == 0 {
        u64::from(unit.id.raw())
    } else {
        request.deployment_runtime_identity
    };
    let mut regions = BTreeMap::from([(function, region)]);
    for candidate in regions.values_mut() {
        select_native_region_tier(candidate, &plan, &unit.constants);
    }
    // Admission can deliberately downgrade an optimizing request when even
    // one instruction family still belongs to the baseline-native runtime.
    // The incoming plan was built for the requested tier and may therefore
    // contain one large whole-region job. Re-plan the resulting graph before
    // any CLIF construction so the downgrade cannot bypass baseline fragment
    // ceilings or fail a valid PHP unit merely because its stale optimizing
    // plan was oversized.
    let replanned = split_oversized_region_blocks(
        regions
            .remove(&function)
            .expect("compile group owns its requested function"),
    );
    regions.insert(function, replanned);
    let plan = NativeCompilePlan::for_region(&regions[&function]);
    if regions[&function].compile_metadata.tier == NativeCompilerTier::Baseline
        && let Some(fragment) = plan
            .fragments
            .iter()
            .find(|fragment| !fragment.is_within_budget())
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_FRAGMENT_BUDGET",
            format!(
                "fragment {} exceeds the pre-Cranelift budget: blocks={} instructions={} estimated_clif_blocks={}",
                fragment.id,
                fragment.blocks.len(),
                fragment.ir_instructions,
                fragment.estimated_clif_blocks
            ),
        ));
    }
    let region = &regions[&function];
    let compilation_mode = crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
        region.compile_metadata.tier,
    )
    .mode();
    let baseline_helper_imports = compilation_mode
        == crate::cranelift_lowering::baseline_streaming::NativeCompilationMode::StreamingBaseline;
    let fragment_layout = (plan.fragments.len() > 1
        || regions
            .values()
            .any(|candidate| candidate.compile_metadata.tier == NativeCompilerTier::Baseline))
    .then(|| NativeFunctionFragmentLayout::for_plan(region, &plan))
    .transpose()?;
    let selected_plan = std::cell::RefCell::new(plan.clone());
    let selected_fragment_layout = std::cell::RefCell::new(fragment_layout.clone());
    // Value-flow and executable SSA describe the PHP function, not one native
    // fragment. Build them exactly once after tier selection and Region-block
    // splitting. Recomputing the complete dominator/phi graph inside every
    // fragment lowering made fragmentation multiply whole-function analysis.
    let value_flows = regions
        .iter()
        .map(|(function, candidate)| {
            let flow = if candidate.compile_metadata.tier == NativeCompilerTier::Optimizing {
                crate::region_ir::analyze_executable_value_flow(candidate, &unit.constants)
            } else {
                crate::region_ir::analyze_baseline_value_ownership(candidate)
            };
            flow.verify_ownership(candidate).map_err(|error| {
                CraneliftLoweringError::new("JIT_CRANELIFT_REJECT_OWNERSHIP", error)
            })?;
            Ok((*function, flow))
        })
        .collect::<Result<BTreeMap<_, _>, CraneliftLoweringError>>()?;
    let ssa_metrics = regions
        .iter()
        .filter(|(_, candidate)| candidate.compile_metadata.tier == NativeCompilerTier::Optimizing)
        .map(|(function, _)| {
            let flow = &value_flows[function];
            (
                flow.promoted_local_count() as u64,
                flow.promoted_register_count() as u64,
                flow.ownership_move_count() as u64,
            )
        })
        .fold((0_u64, 0_u64, 0_u64), |total, metrics| {
            (
                total.0.saturating_add(metrics.0),
                total.1.saturating_add(metrics.1),
                total.2.saturating_add(metrics.2),
            )
        });
    let arity = region_arity(region)?;
    let fast_path_hits = regions
        .values()
        .map(|region| region.fast_path_operations)
        .sum();
    let has_control_flow = regions.values().any(RegionGraph::has_control_flow);
    let mut trampoline_functions = regions
        .iter()
        .filter_map(|(function, region)| {
            (region.params.iter().any(|parameter| parameter.by_ref)
                || region.returns_by_ref
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
    let resolver_target = |target: FunctionId| {
        runtime_helpers.native_function_resolve != 0
            && !regions.contains_key(&target)
            && unit
                .functions
                .get(target.index())
                .is_some_and(|function| !ir_function_requires_trampoline(function))
    };
    let needs_function_resolver = regions.values().any(|region| {
        region_contains(region, |kind| {
            let RegionInstructionKind::NativeCall(call) = kind else {
                return false;
            };
            !matches!(call.result, RegionCallResult::ReferenceLocal(_))
                && call
                    .args
                    .iter()
                    .all(|argument| argument.name.is_none() && !argument.unpack)
                && call.direct_compiled_target().is_some_and(resolver_target)
        })
    });
    let is_direct_linked_call = |call: &RegionNativeCall| {
        direct_linked_signature(call, &request.external_function_signatures).is_some()
    };
    let is_direct_linked_variadic_call = |call: &RegionNativeCall| {
        direct_linked_signature(call, &request.external_function_signatures)
            .and_then(|signature| signature.native_params.last())
            .is_some_and(|parameter| parameter.variadic)
    };
    let needs_call_trampoline = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::NativeCall(call)
                    if call.direct_compiled_target().is_none()
                        && !matches!(call.target, RegionCallTarget::Semantic { .. })
                        && !is_direct_linked_call(call)
            )
        }) || region
            .direct_callees()
            .iter()
            .any(|callee| !regions.contains_key(callee) && !resolver_target(*callee))
            || region_contains(region, |kind| {
                matches!(
                    kind,
                    RegionInstructionKind::NativeCall(call)
                        if !matches!(call.target, RegionCallTarget::Semantic { .. })
                            && (matches!(call.result, RegionCallResult::ReferenceLocal(_))
                            || call.args.iter().any(|argument| {
                                argument.name.is_some() || argument.unpack
                            })
                            || call
                                .direct_compiled_target()
                                .is_some_and(|target| trampoline_functions.contains(&target)))
                )
            })
    });
    let needs_baseline_builtin_dispatch = baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(call)
                    if baseline_builtin_helper_id(&call.target).is_some())
            })
        });
    let needs_exact_symbol_query: [bool; StableSymbolQueryBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_symbol_query(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_pcre: [bool; StablePcreBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_pcre(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_preg_callback = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(
                    kind,
                    RegionInstructionKind::ArrayCallback(call)
                        if call.operation == RegionArrayCallbackOperation::PregReplace
                ) || matches!(kind, RegionInstructionKind::PregCallbackArray(_))
            })
        });
    let needs_exact_json: [bool; StableJsonBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_json(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_format: [bool; StableFormatBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_format(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_hash: [bool; StableHashBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_hash(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_byte_codec: [bool; StableByteCodecBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_byte_codec(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_string_search_compare: [bool; StableStringSearchCompareBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_string_search_compare(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_string_rewrite: [bool; StableStringRewriteBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_string_rewrite(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_html_codec: [bool; StableHtmlCodecBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_html_codec(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_url_query: [bool; StableUrlQueryBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_url_query(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_array_aggregate: [bool; StableArrayAggregateBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.iter().any(|(function, region)| {
                    let value_flow = &value_flows[function];
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_array_aggregate(&call.target)
                            .is_some_and(|builtin| builtin.index() == index)
                            && !(matches!(
                                stable_builtin_array_aggregate(&call.target),
                                Some(
                                    StableArrayAggregateBuiltin::Count
                                        | StableArrayAggregateBuiltin::SizeOf
                                )
                            ) && (call.args.len() == 1
                                || call.args.len() == 2
                                    && direct_fixed_builtin_operand(call, 1).is_some_and(
                                        |mode| value_flow
                                            .operand_fact(&unit.constants, mode)
                                            .integer_range
                                            == Some(
                                                crate::region_ir::SsaIntegerRange::exact(0),
                                            ),
                                    ))))
                    })
                })
        });
    let needs_exact_recursive_array: [bool; StableRecursiveArrayBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_recursive_array(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_array_sort: [bool; StableArraySortBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_array_sort(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_array_multisort = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(call)
                    if stable_builtin_array_multisort(&call.target))
            })
        });
    let needs_exact_object_identity: [bool; StableObjectIdentityBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_object_identity(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_callable_query: [bool; StableCallableQueryBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_callable_query(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_callback_handler: [bool; StableCallbackHandlerBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_callback_handler(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_autoload_callback: [bool; StableAutoloadCallbackBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_autoload_callback(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_shutdown_callback = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(call)
                    if stable_builtin_shutdown_callback(&call.target))
            })
        });
    let needs_exact_serialization: [bool; StableSerializationBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_serialization(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_tokenizer: [bool; StableTokenizerBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_tokenizer(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_mbstring: [bool; StableMbstringBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_mbstring(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_bcmath: [bool; StableBcmathBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_bcmath(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_filter: [bool; StableFilterBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_filter(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_session: [bool; StableSessionBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                    if stable_builtin_session(&call.target)
                        .is_some_and(|builtin| {
                            builtin.index() == index
                                && builtin.accepts_arity(call.args.len())
                        }))
                })
            })
    });
    let needs_exact_object_vars: [bool; StableObjectVarsBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_object_vars(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_class_metadata: [bool; StableClassMetadataBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_class_metadata(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_class_lineage: [bool; StableClassLineageBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_class_lineage(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_extension_query: [bool; StableExtensionQueryBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_extension_query(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_memory_query: [bool; StableMemoryQueryBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_memory_query(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_gc: [bool; StableGcBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_gc(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_resource_query: [bool; StableResourceQueryBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_resource_query(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_error_state: [bool; StableErrorStateBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_error_state(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_settype = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(call)
                    if stable_builtin_settype(&call.target))
            })
        });
    let needs_exact_configuration: [bool; StableConfigurationBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_configuration(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_http_response: [bool; StableHttpResponseBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_http_response(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_cookie: [bool; StableCookieBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_cookie(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_clock: [bool; StableClockBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_clock(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_date: [bool; StableDateBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_date(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_random: [bool; StableRandomBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_random(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_request_query: [bool; StableRequestQueryBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_request_query(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_declaration_inventory: [bool; StableDeclarationInventoryBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_declaration_inventory(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_constant_inventory = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(call)
                    if stable_builtin_constant_inventory(&call.target))
            })
        });
    let needs_exact_compact = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(call)
                    if stable_builtin_compact(&call.target)
                        || stable_builtin_get_defined_vars(&call.target))
            })
        });
    let needs_exact_frame_introspection: [bool; StableFrameIntrospectionBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_frame_introspection(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_base_conversion: [bool; StableBaseConversionBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_base_conversion(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_intval_base = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(call)
                    if stable_builtin_scalar_consumer(&call.target)
                        == Some(StableScalarConsumerBuiltin::IntVal)
                        && call.args.len() == 2)
            })
        });
    let needs_exact_network_address: [bool; StableNetworkAddressBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_network_address(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_compression_codec: [bool; StableCompressionCodecBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_compression_codec(&call.target)
                                .is_some_and(|builtin| builtin.index() == index))
                    })
                })
        });
    let needs_exact_path: [bool; StablePathBuiltin::COUNT] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(kind, RegionInstructionKind::NativeCall(call)
                        if stable_builtin_path(&call.target)
                            .is_some_and(|builtin| builtin.index() == index))
                })
            })
    });
    let needs_exact_output_buffer: [bool; StableOutputBufferBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        matches!(kind, RegionInstructionKind::NativeCall(call)
                            if stable_builtin_output_buffer(&call.target)
                                .is_some_and(|builtin| builtin.index() == index
                                    && builtin.accepts_arity(call.args.len())))
                    })
                })
        });
    let needs_exact_pure_math: [bool; StablePureMathBuiltin::COUNT] =
        std::array::from_fn(|index| {
            !baseline_helper_imports
                && regions.values().any(|region| {
                    region_contains(region, |kind| {
                        if index == StablePureMathBuiltin::Fpow.index()
                            && (matches!(
                                kind,
                                RegionInstructionKind::Binary {
                                    op: RegionBinaryOp::Pow,
                                    ..
                                }
                            ) || matches!(
                                kind,
                                RegionInstructionKind::NativeCall(call)
                                    if stable_builtin_numeric_operator(&call.target)
                                        == Some(StableNumericOperatorBuiltin::Pow)
                            ))
                        {
                            return true;
                        }
                        let RegionInstructionKind::NativeCall(call) = kind else {
                            return false;
                        };
                        stable_builtin_pure_math(&call.target).is_some_and(|builtin| {
                            builtin.index() == index
                                && builtin.accepts_arity(call.args.len())
                                && call.args.iter().enumerate().all(|(argument, metadata)| {
                                    metadata.name.is_none()
                                        && !metadata.unpack
                                        && call
                                            .operands
                                            .get(
                                                call.argument_operand_offset
                                                    .saturating_add(argument),
                                            )
                                            .is_some_and(Option::is_some)
                                })
                        })
                    })
                })
        });
    let needs_semantic_dispatch = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call)
                if matches!(call.target, RegionCallTarget::Semantic { .. }))
        })
    });
    let needs_frame_arena = runtime_helpers.native_frame_alloc != 0
        && runtime_helpers.native_frame_release != 0
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::NativeCall(_))
            })
        });
    if baseline_helper_imports
        && needs_call_trampoline
        && runtime_helpers.baseline_call_dispatch == 0
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_CALL_TRAMPOLINE",
            "dynamic or complex call requires the typed native dispatch trampoline",
        ));
    }
    if baseline_helper_imports
        && needs_baseline_builtin_dispatch
        && runtime_helpers.baseline_builtin_dispatch == 0
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_BUILTIN_DISPATCH",
            "direct builtin call requires the stable-ID native builtin dispatcher",
        ));
    }
    if baseline_helper_imports
        && needs_semantic_dispatch
        && runtime_helpers.baseline_semantic_dispatch == 0
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_SEMANTIC_DISPATCH",
            "typed semantic operation requires the direct semantic dispatcher",
        ));
    }
    let needs_dynamic_code = regions.values().any(RegionGraph::has_native_dynamic_code);
    if baseline_helper_imports && needs_dynamic_code && runtime_helpers.native_dynamic_code == 0 {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_NATIVE_DYNAMIC_CODE",
            "include, eval, or runtime declaration requires the native dynamic-code compiler",
        ));
    }
    let baseline_call_symbol = BASELINE_NATIVE_CALL_DISPATCH_SYMBOL.to_owned();
    let native_builtin_dispatch_symbol = BASELINE_NATIVE_BUILTIN_DISPATCH_SYMBOL.to_owned();
    let baseline_semantic_dispatch_symbol = BASELINE_NATIVE_SEMANTIC_DISPATCH_SYMBOL.to_owned();
    let native_function_resolve_symbol = NATIVE_FUNCTION_RESOLVE_SYMBOL.to_owned();
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
    let mut needs_exact_unary = [false; NATIVE_EXACT_UNARY_COUNT];
    for operation in NATIVE_EXACT_UNARY_OPERATIONS {
        needs_exact_unary[native_exact_unary_index(operation)] = regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(
                    kind,
                    RegionInstructionKind::Unary { op, .. } if *op == operation
                )
            })
        });
    }
    let mut needs_exact_compare = [false; NATIVE_EXACT_COMPARE_COUNT];
    for operation in NATIVE_EXACT_COMPARE_OPERATIONS {
        needs_exact_compare[native_exact_compare_index(operation)] =
            regions.values().any(|region| {
                region_contains(region, |kind| {
                    matches!(
                        kind,
                        RegionInstructionKind::Compare { op, .. } if *op == operation
                    ) || operation == RegionCompareOpCode::Spaceship
                        && matches!(
                            kind,
                            RegionInstructionKind::NativeCall(call)
                                if stable_builtin_extrema(&call.target).is_some()
                        )
                })
            });
    }
    let needs_baseline_binary = baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(kind, RegionInstructionKind::Binary { .. })
            })
        });
    let needs_array_union = !baseline_helper_imports
        && regions.iter().any(|(function, region)| {
            let value_flow = &value_flows[function];
            region_contains(region, |kind| {
                let RegionInstructionKind::Binary {
                    op: RegionBinaryOp::Add,
                    lhs,
                    rhs,
                    ..
                } = kind
                else {
                    return false;
                };
                [*lhs, *rhs].into_iter().all(|operand| {
                    let fact = value_flow.operand_fact(&unit.constants, operand);
                    fact.certainty != crate::region_ir::SsaCertainty::Unknown
                        && fact.class == SsaValueClass::ArrayHandle
                })
            })
        });
    let needs_concat = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(
                    kind,
                    RegionInstructionKind::Binary {
                        op: RegionBinaryOp::Concat,
                        ..
                    }
                )
            })
        });
    let needs_string_bitwise: [bool; 3] = std::array::from_fn(|index| {
        !baseline_helper_imports
            && regions.iter().any(|(function, region)| {
                let value_flow = &value_flows[function];
                region_contains(region, |kind| {
                    let RegionInstructionKind::Binary { op, lhs, rhs, .. } = kind else {
                        return false;
                    };
                    matches!(
                        op,
                        RegionBinaryOp::BitAnd | RegionBinaryOp::BitOr | RegionBinaryOp::BitXor
                    ) && native_string_bitwise_index(*op) == index
                        && [*lhs, *rhs].into_iter().all(|operand| {
                            let fact = value_flow.operand_fact(&unit.constants, operand);
                            fact.certainty != crate::region_ir::SsaCertainty::Unknown
                                && fact.class == SsaValueClass::StringHandle
                        })
                })
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
    let needs_float_to_string = regions.iter().any(|(function, region)| {
        let value_flow = &value_flows[function];
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Echo { .. } | RegionInstructionKind::Compare { .. }
            ) || matches!(kind, RegionInstructionKind::NativeCall(call)
            if stable_builtin_array_lookup(&call.target).is_some()
                || stable_builtin_extrema(&call.target).is_some()
                || optimizing_strval_uses_float_handler(
                    call,
                    value_flow,
                    &unit.constants,
                ))
        })
    });
    let call_scalar_helper_needs = regions.iter().fold(
        OptimizingCallScalarHelperNeeds::default(),
        |mut needs, (function, region)| {
            let value_flow = &value_flows[function];
            let return_needs = optimizing_return_scalar_helper_needs(
                region,
                value_flow,
                &unit.constants,
                region.strict_types,
            );
            needs.numeric_string |= return_needs.numeric_string;
            needs.string_cast |= return_needs.string_cast;
            for call in region
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .filter_map(|instruction| {
                    let RegionInstructionKind::NativeCall(call) = &instruction.kind else {
                        return None;
                    };
                    Some(call)
                })
            {
                let call_needs = optimizing_call_scalar_helper_needs(
                    call,
                    unit,
                    &request.external_function_signatures,
                    value_flow,
                    &unit.constants,
                );
                needs.numeric_string |= call_needs.numeric_string;
                needs.string_cast |= call_needs.string_cast;
            }
            needs
        },
    );
    let needs_numeric_string = call_scalar_helper_needs.numeric_string
        || regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(
                    kind,
                    RegionInstructionKind::Compare { .. }
                        | RegionInstructionKind::Cast {
                            op: RegionCastOp::Int | RegionCastOp::Float,
                            ..
                        }
                ) || matches!(
                kind,
                RegionInstructionKind::NativeCall(call)
                    if stable_builtin_array_lookup(&call.target).is_some()
                        || stable_builtin_extrema(&call.target).is_some()
                        || matches!(
                            stable_builtin_scalar_consumer(&call.target),
                            Some(
                                StableScalarConsumerBuiltin::FloatVal
                                    | StableScalarConsumerBuiltin::IntVal
                                    | StableScalarConsumerBuiltin::StrVal
                            )
                        )
                )
            })
        });
    let needs_fmod_f64 = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call)
                if stable_builtin_scalar_math(&call.target)
                    == Some(StableScalarMathBuiltin::Fmod))
        })
    });
    let needs_round_f64 = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call)
                if stable_builtin_numeric_operator(&call.target)
                    == Some(StableNumericOperatorBuiltin::Round))
        })
    });
    let needs_array_cast = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Cast {
                    op: RegionCastOp::Array,
                    ..
                }
            )
        })
    });
    let needs_int_cast = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Cast {
                    op: RegionCastOp::Int,
                    ..
                }
            ) || matches!(
                kind,
                RegionInstructionKind::NativeCall(call)
                    if stable_builtin_scalar_consumer(&call.target)
                        == Some(StableScalarConsumerBuiltin::IntVal)
            )
        })
    });
    let needs_float_cast = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Cast {
                    op: RegionCastOp::Float,
                    ..
                }
            ) || matches!(
                kind,
                RegionInstructionKind::NativeCall(call)
                    if stable_builtin_scalar_consumer(&call.target)
                        == Some(StableScalarConsumerBuiltin::FloatVal)
            )
        })
    });
    let needs_string_cast = call_scalar_helper_needs.string_cast
        || regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(
                    kind,
                    RegionInstructionKind::Cast {
                        op: RegionCastOp::String,
                        ..
                    }
                ) || matches!(
                    kind,
                    RegionInstructionKind::NativeCall(call)
                        if stable_builtin_scalar_consumer(&call.target)
                            == Some(StableScalarConsumerBuiltin::StrVal)
                            && call.args.len() == 1
                            && direct_fixed_builtin_operand(call, 0).is_some()
                )
            })
        });
    let needs_callback_return_string = regions.values().any(|region| {
        region_contains(region, |kind| match kind {
            RegionInstructionKind::PregCallbackArray(_) => true,
            RegionInstructionKind::ArrayCallback(call) => {
                call.operation == RegionArrayCallbackOperation::PregReplace
            }
            _ => false,
        })
    });
    let needs_object_cast = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Cast {
                    op: RegionCastOp::Object,
                    ..
                }
            )
        })
    });
    let needs_object_class_name = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::FetchObjectClassName {
                    prepared_class: None,
                    ..
                }
            ) || matches!(kind, RegionInstructionKind::NativeCall(call)
            if matches!(
                call.target,
                RegionCallTarget::Semantic {
                    operation: RegionSemanticOp::BoundClosureClass { .. }
                }
            )) || matches!(kind, RegionInstructionKind::NativeCall(call)
                    if stable_builtin_scalar_consumer(&call.target)
                        == Some(StableScalarConsumerBuiltin::GetDebugType))
                || matches!(kind, RegionInstructionKind::NativeCall(call)
                    if stable_builtin_get_class(&call.target))
        })
    });
    let needs_acquire_callable = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(
                    kind,
                    RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Semantic {
                            operation: RegionSemanticOp::AcquireCallable { .. }
                        },
                        ..
                    })
                ) || matches!(
                    kind,
                    RegionInstructionKind::ArrayCallback(call)
                        if matches!(call.callback, RegionArrayCallbackTarget::Runtime(_))
                ) || matches!(
                    kind,
                    RegionInstructionKind::PregCallbackArray(call)
                        if call.entries.iter().any(|entry| matches!(
                            entry.callback,
                            RegionArrayCallbackTarget::Runtime(_)
                        ))
                )
            })
        });
    let needs_resolve_callable = !baseline_helper_imports
        && regions.values().any(|region| {
            region_contains(region, |kind| {
                matches!(
                    kind,
                    RegionInstructionKind::NativeCall(RegionNativeCall {
                        target: RegionCallTarget::Semantic {
                            operation: RegionSemanticOp::ResolveCallable {
                                callable: php_ir::instruction::CallableKind::FunctionName { .. },
                                ..
                            }
                        },
                        ..
                    })
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
                    // Every baseline by-value array operand passes through
                    // the shared reference-payload guard. Even a register or
                    // constant can hold a compatibility reference after a
                    // continuation resume, so these instruction families
                    // need the cold local-fetch boundary as well.
                    | RegionInstructionKind::ArrayInsert { .. }
                    | RegionInstructionKind::ArraySpread { .. }
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
        region
            .exception_regions
            .iter()
            .any(|handler| handler.catch.is_some() && handler.exception_local.is_some())
            || region_contains(region, |kind| {
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
    let needs_value_release = true;
    // Local publication is part of the native frame ABI, not just explicit
    // PHP reference syntax.  Stores, unsets, foreach-by-reference and array
    // root updates can all publish a local through the same helper.  Keep the
    // helper available for every executable region so adding publication to a
    // lowering cannot accidentally make an otherwise supported function
    // uncompilable.
    let needs_reference_bind = true;
    let needs_argument_check = regions.values().any(|region| {
        region
            .params
            .iter()
            .any(|parameter| parameter.type_.is_some())
    }) || (needs_function_resolver
        && unit.functions.iter().any(|function| {
            function
                .params
                .iter()
                .any(|parameter| parameter.type_.is_some())
        }));
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
                || matches!(kind, RegionInstructionKind::NativeCall(call)
                    if call.variadic || is_direct_linked_variadic_call(call))
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
    let needs_dynamic_property_slot = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call)
            if matches!(
                &call.target,
                RegionCallTarget::Semantic {
                    operation: RegionSemanticOp::PropertyFetch {
                        property: RegionPropertyName::Dynamic(_),
                        ..
                    } | RegionSemanticOp::PropertyAssign {
                        property: RegionPropertyName::Dynamic(_),
                        ..
                    } | RegionSemanticOp::PropertyUnset {
                        property: RegionPropertyName::Dynamic(_),
                        ..
                    } | RegionSemanticOp::PropertyDimAssign {
                        property: RegionPropertyName::Dynamic(_),
                        ..
                    } | RegionSemanticOp::PropertyDimUnset {
                        property: RegionPropertyName::Dynamic(_),
                        ..
                    }
                }
            ))
        })
    });
    let needs_dynamic_property_test_slot = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call)
            if matches!(
                &call.target,
                RegionCallTarget::Semantic {
                    operation: RegionSemanticOp::PropertyIsset {
                        property: RegionPropertyName::Dynamic(_),
                        ..
                    } | RegionSemanticOp::PropertyEmpty {
                        property: RegionPropertyName::Dynamic(_),
                        ..
                    } | RegionSemanticOp::PropertyDimIsset {
                        property: RegionPropertyName::Dynamic(_),
                        ..
                    } | RegionSemanticOp::PropertyDimEmpty {
                        property: RegionPropertyName::Dynamic(_),
                        ..
                    }
                }
            ))
        })
    });
    let needs_object_clone = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::CloneObject { .. })
        })
    });
    let needs_plain_object_clone = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::CloneObject { plain: true, .. })
        })
    });
    let needs_prepared_closure_new = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::NativeDynamicCode(
                    RegionNativeDynamicCode::MakeClosure { .. }
                )
            )
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
            ) || matches!(kind, RegionInstructionKind::NativeCall(call)
                if call.variadic || is_direct_linked_variadic_call(call))
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
            ) || matches!(kind, RegionInstructionKind::NativeCall(call)
                if stable_builtin_array_key_exists(&call.target))
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
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::Unary {
                    op: crate::region_ir::RegionUnaryOp::Not,
                    ..
                } | RegionInstructionKind::Cast {
                    op: crate::region_ir::RegionCastOp::Bool,
                    ..
                } | RegionInstructionKind::EmptyDim { .. }
                    | RegionInstructionKind::EmptyLocal { .. }
            )
        }) || region.blocks.iter().any(|block| {
            matches!(
                block.terminator,
                RegionTerminator::JumpIfFalse { .. }
                    | RegionTerminator::JumpIfTrue { .. }
                    | RegionTerminator::JumpIf { .. }
            )
        })
    });
    let needs_type_predicate = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::NativeCall(call)
                    if stable_builtin_type_predicate(&call.target).is_some()
                        && call.argument_operand_offset == 0
                        && call.args.len() == 1
                        && call.args[0].name.is_none()
                        && !call.args[0].unpack
                        && call.operands.len() == 1
                        && call.operands[0].is_some()
                        && !matches!(call.result, RegionCallResult::ReferenceLocal(_))
            )
        })
    });
    let needs_stable_length = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(
                kind,
                RegionInstructionKind::EmptyDim { .. } | RegionInstructionKind::EmptyLocal { .. }
            ) || matches!(kind, RegionInstructionKind::NativeCall(call) if stable_builtin_length(&call.target).is_some())
        })
    });
    let needs_string_predicate = regions.values().any(|region| {
        region_contains(region, |kind| {
            matches!(kind, RegionInstructionKind::NativeCall(call)
                if stable_builtin_string_predicate(&call.target).is_some())
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
    // Shadow every generic-runtime requirement with the tier capability.  In
    // particular, the optimizing closure below never declares a helper and
    // therefore cannot smuggle one into code through an unused wrapper.
    let needs_call_trampoline = baseline_helper_imports && needs_call_trampoline;
    let needs_function_resolver = baseline_helper_imports && needs_function_resolver;
    let needs_semantic_dispatch = baseline_helper_imports && needs_semantic_dispatch;
    let needs_frame_arena = baseline_helper_imports && needs_frame_arena;
    let needs_dynamic_code = baseline_helper_imports && needs_dynamic_code;
    let needs_unary = baseline_helper_imports && needs_unary;
    if baseline_helper_imports {
        needs_exact_unary.fill(false);
    }
    if baseline_helper_imports {
        needs_exact_compare.fill(false);
    }
    let needs_compare = baseline_helper_imports && needs_compare;
    let needs_float_to_string = !baseline_helper_imports && needs_float_to_string;
    let needs_numeric_string = !baseline_helper_imports && needs_numeric_string;
    let needs_fmod_f64 = !baseline_helper_imports && needs_fmod_f64;
    let needs_round_f64 = !baseline_helper_imports && needs_round_f64;
    let needs_array_cast = !baseline_helper_imports && needs_array_cast;
    let needs_int_cast = !baseline_helper_imports && needs_int_cast;
    let needs_float_cast = !baseline_helper_imports && needs_float_cast;
    let needs_string_cast = !baseline_helper_imports && needs_string_cast;
    let needs_object_cast = !baseline_helper_imports && needs_object_cast;
    let needs_object_class_name = !baseline_helper_imports && needs_object_class_name;
    let needs_cast = baseline_helper_imports && needs_cast;
    let needs_direct_echo = !baseline_helper_imports && needs_echo;
    let needs_echo = baseline_helper_imports && needs_echo;
    let needs_local_fetch = baseline_helper_imports && needs_local_fetch;
    let needs_local_store = baseline_helper_imports && needs_local_store;
    let needs_value_release = baseline_helper_imports && needs_value_release;
    let needs_reference_bind = baseline_helper_imports && needs_reference_bind;
    let needs_argument_check = baseline_helper_imports && needs_argument_check;
    let needs_return_check = baseline_helper_imports && needs_return_check;
    let needs_prepared_exception_new = !baseline_helper_imports && needs_exception_new;
    let needs_exception_new = baseline_helper_imports && needs_exception_new;
    let needs_array_new = baseline_helper_imports && needs_array_new;
    let needs_prepared_object_new = !baseline_helper_imports && needs_object_new;
    let needs_prepared_closure_new = !baseline_helper_imports && needs_prepared_closure_new;
    let needs_object_new = baseline_helper_imports && needs_object_new;
    let needs_property_fetch = baseline_helper_imports && needs_property_fetch;
    let needs_property_assign = baseline_helper_imports && needs_property_assign;
    let needs_object_clone = baseline_helper_imports && needs_object_clone;
    let needs_plain_object_clone = !baseline_helper_imports && needs_plain_object_clone;
    let needs_dynamic_property_slot = !baseline_helper_imports && needs_dynamic_property_slot;
    let needs_dynamic_property_test_slot =
        !baseline_helper_imports && needs_dynamic_property_test_slot;
    let needs_object_clone_with = baseline_helper_imports && needs_object_clone_with;
    let needs_array_insert = baseline_helper_imports && needs_array_insert;
    let needs_array_fetch = baseline_helper_imports && needs_array_fetch;
    let needs_array_unset = baseline_helper_imports && needs_array_unset;
    let needs_array_spread = baseline_helper_imports && needs_array_spread;
    let needs_foreach_init = baseline_helper_imports && needs_foreach_init;
    let needs_foreach_next = baseline_helper_imports && needs_foreach_next;
    let needs_foreach_cleanup = baseline_helper_imports && needs_foreach_cleanup;
    let needs_constant_fetch = baseline_helper_imports && needs_constant_fetch;
    let needs_truthy = baseline_helper_imports && needs_truthy;
    let needs_type_predicate = baseline_helper_imports && needs_type_predicate;
    let needs_stable_length = baseline_helper_imports && needs_stable_length;
    let needs_string_predicate = baseline_helper_imports && needs_string_predicate;
    let needs_runtime_fatal = baseline_helper_imports && needs_runtime_fatal;
    let mut imports = vec![(
        "region-runtime-helper-abi".to_owned(),
        region.compile_metadata.helper_abi_hash as usize,
    )];
    if baseline_helper_imports && needs_call_trampoline {
        imports.push((
            baseline_call_symbol.clone(),
            runtime_helpers.baseline_call_dispatch,
        ));
    }
    if needs_baseline_builtin_dispatch {
        imports.push((
            native_builtin_dispatch_symbol.clone(),
            runtime_helpers.baseline_builtin_dispatch,
        ));
    }
    for builtin in StableSymbolQueryBuiltin::all() {
        if !needs_exact_symbol_query[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StableSymbolQueryBuiltin::Define => runtime_helpers.native_define,
            StableSymbolQueryBuiltin::Defined => runtime_helpers.native_defined,
            StableSymbolQueryBuiltin::Constant => runtime_helpers.native_constant,
            StableSymbolQueryBuiltin::FunctionExists => runtime_helpers.native_function_exists,
            StableSymbolQueryBuiltin::ClassExists => runtime_helpers.native_class_exists,
            StableSymbolQueryBuiltin::InterfaceExists => runtime_helpers.native_interface_exists,
            StableSymbolQueryBuiltin::TraitExists => runtime_helpers.native_trait_exists,
            StableSymbolQueryBuiltin::EnumExists => runtime_helpers.native_enum_exists,
            StableSymbolQueryBuiltin::MethodExists => runtime_helpers.native_method_exists,
            StableSymbolQueryBuiltin::PropertyExists => runtime_helpers.native_property_exists,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_NATIVE_SYMBOL_QUERY",
                format!(
                    "prepared symbol query requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StablePcreBuiltin::all() {
        if !needs_exact_pcre[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StablePcreBuiltin::Match => runtime_helpers.native_preg_match,
            StablePcreBuiltin::MatchAll => runtime_helpers.native_preg_match_all,
            StablePcreBuiltin::Replace => runtime_helpers.native_preg_replace,
            StablePcreBuiltin::Filter => runtime_helpers.native_preg_filter,
            StablePcreBuiltin::Split => runtime_helpers.native_preg_split,
            StablePcreBuiltin::Grep => runtime_helpers.native_preg_grep,
            StablePcreBuiltin::Quote => runtime_helpers.native_preg_quote,
            StablePcreBuiltin::LastError => runtime_helpers.native_preg_last_error,
            StablePcreBuiltin::LastErrorMessage => runtime_helpers.native_preg_last_error_msg,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_PCRE",
                format!(
                    "prepared PCRE builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    if needs_preg_callback {
        for (symbol, address) in [
            (
                "phrust_native_preg_callback_plan",
                runtime_helpers.native_preg_callback_plan,
            ),
            (
                "phrust_native_preg_callback_assemble",
                runtime_helpers.native_preg_callback_assemble,
            ),
        ] {
            if address == 0 {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_EXACT_PCRE_CALLBACK",
                    format!("prepared PCRE callback replacement requires {symbol}"),
                ));
            }
            imports.push((symbol.to_owned(), address));
        }
    }
    for builtin in StableJsonBuiltin::all() {
        if !needs_exact_json[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StableJsonBuiltin::Encode => runtime_helpers.native_json_encode,
            StableJsonBuiltin::Decode => runtime_helpers.native_json_decode,
            StableJsonBuiltin::Validate => runtime_helpers.native_json_validate,
            StableJsonBuiltin::LastError => runtime_helpers.native_json_last_error,
            StableJsonBuiltin::LastErrorMessage => runtime_helpers.native_json_last_error_msg,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_JSON",
                format!(
                    "prepared JSON builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableFormatBuiltin::all() {
        if !needs_exact_format[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StableFormatBuiltin::Sprintf => runtime_helpers.native_sprintf,
            StableFormatBuiltin::Printf => runtime_helpers.native_printf,
            StableFormatBuiltin::Vsprintf => runtime_helpers.native_vsprintf,
            StableFormatBuiltin::Vprintf => runtime_helpers.native_vprintf,
            StableFormatBuiltin::NumberFormat => runtime_helpers.native_number_format,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_FORMAT",
                format!(
                    "prepared formatting builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableHashBuiltin::all() {
        if !needs_exact_hash[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StableHashBuiltin::Md5 => runtime_helpers.native_md5,
            StableHashBuiltin::Sha1 => runtime_helpers.native_sha1,
            StableHashBuiltin::Crc32 => runtime_helpers.native_crc32,
            StableHashBuiltin::Hash => runtime_helpers.native_hash,
            StableHashBuiltin::HashHmac => runtime_helpers.native_hash_hmac,
            StableHashBuiltin::HashEquals => runtime_helpers.native_hash_equals,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_HASH",
                format!(
                    "prepared hash builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableByteCodecBuiltin::all() {
        if !needs_exact_byte_codec[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StableByteCodecBuiltin::Base64Encode => runtime_helpers.native_base64_encode,
            StableByteCodecBuiltin::Base64Decode => runtime_helpers.native_base64_decode,
            StableByteCodecBuiltin::Bin2Hex => runtime_helpers.native_bin2hex,
            StableByteCodecBuiltin::Hex2Bin => runtime_helpers.native_hex2bin,
            StableByteCodecBuiltin::QuotedPrintableDecode => {
                runtime_helpers.native_quoted_printable_decode
            }
            StableByteCodecBuiltin::UrlEncode => runtime_helpers.native_urlencode,
            StableByteCodecBuiltin::RawUrlEncode => runtime_helpers.native_rawurlencode,
            StableByteCodecBuiltin::UrlDecode => runtime_helpers.native_urldecode,
            StableByteCodecBuiltin::RawUrlDecode => runtime_helpers.native_rawurldecode,
            StableByteCodecBuiltin::UuEncode => runtime_helpers.native_convert_uuencode,
            StableByteCodecBuiltin::UuDecode => runtime_helpers.native_convert_uudecode,
            StableByteCodecBuiltin::AddCSlashes => runtime_helpers.native_addcslashes,
            StableByteCodecBuiltin::StripCSlashes => runtime_helpers.native_stripcslashes,
            StableByteCodecBuiltin::StripSlashes => runtime_helpers.native_stripslashes,
            StableByteCodecBuiltin::QuoteMeta => runtime_helpers.native_quotemeta,
            StableByteCodecBuiltin::Pack => runtime_helpers.native_pack,
            StableByteCodecBuiltin::Unpack => runtime_helpers.native_unpack,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_BYTE_CODEC",
                format!(
                    "prepared byte-codec builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableStringSearchCompareBuiltin::all() {
        if !needs_exact_string_search_compare[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_string_search_compare[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_STRING_SEARCH_COMPARE",
                format!(
                    "prepared string search/compare builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableStringRewriteBuiltin::all() {
        if !needs_exact_string_rewrite[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_string_rewrite[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_STRING_REWRITE",
                format!(
                    "prepared string rewrite builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableHtmlCodecBuiltin::all() {
        if !needs_exact_html_codec[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_html_codec[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_HTML_CODEC",
                format!(
                    "prepared HTML codec builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableUrlQueryBuiltin::all() {
        if !needs_exact_url_query[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_url_query[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_URL_QUERY",
                format!(
                    "prepared URL/query builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableArrayAggregateBuiltin::all() {
        if !needs_exact_array_aggregate[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_array_aggregate[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_ARRAY_AGGREGATE",
                format!(
                    "prepared array aggregate requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableRecursiveArrayBuiltin::all() {
        if !needs_exact_recursive_array[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_recursive_array[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_RECURSIVE_ARRAY",
                format!(
                    "prepared recursive array operation requires fixed native handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableArraySortBuiltin::all() {
        if !needs_exact_array_sort[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_array_sort[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_ARRAY_PRESERVING_SORT",
                format!(
                    "prepared key-preserving sort requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    if needs_exact_array_multisort {
        let address = runtime_helpers.native_array_multisort;
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_ARRAY_MULTISORT",
                "prepared array_multisort requires its fixed native slice handler",
            ));
        }
        imports.push(("phrust_native_array_multisort".to_owned(), address));
    }
    for builtin in StableObjectIdentityBuiltin::all() {
        if !needs_exact_object_identity[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_object_identity[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_OBJECT_IDENTITY",
                format!(
                    "prepared object-identity builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableCallableQueryBuiltin::all() {
        if !needs_exact_callable_query[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_is_callable;
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_CALLABLE_QUERY",
                format!(
                    "prepared callable-query builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableCallbackHandlerBuiltin::all() {
        if !needs_exact_callback_handler[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_callback_handler[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_CALLBACK_HANDLER",
                format!(
                    "prepared callback-handler builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableAutoloadCallbackBuiltin::all() {
        if !needs_exact_autoload_callback[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_autoload_callback[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_AUTOLOAD_CALLBACK",
                format!(
                    "prepared autoload-callback builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    if needs_exact_shutdown_callback {
        let address = runtime_helpers.native_register_shutdown_function;
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_SHUTDOWN_CALLBACK",
                "prepared register_shutdown_function requires its exact native slice handler",
            ));
        }
        imports.push((
            "phrust_native_register_shutdown_function".to_owned(),
            address,
        ));
    }
    for builtin in StableSerializationBuiltin::all() {
        if !needs_exact_serialization[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_serialization[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_SERIALIZATION",
                format!(
                    "prepared serialization builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableTokenizerBuiltin::all() {
        if !needs_exact_tokenizer[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_tokenizer[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_TOKENIZER",
                format!(
                    "prepared tokenizer builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableMbstringBuiltin::all() {
        if !needs_exact_mbstring[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_mbstring[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_MBSTRING",
                format!(
                    "prepared mbstring builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableBcmathBuiltin::all() {
        if !needs_exact_bcmath[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_bcmath[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_BCMATH",
                format!(
                    "prepared bcmath builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableFilterBuiltin::all() {
        if !needs_exact_filter[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_filter[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_FILTER",
                format!(
                    "prepared filter builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableSessionBuiltin::all() {
        if !needs_exact_session[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_session[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_SESSION",
                format!(
                    "prepared session builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableObjectVarsBuiltin::all() {
        if !needs_exact_object_vars[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_object_vars[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_OBJECT_VARS",
                format!(
                    "prepared object-vars builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableClassMetadataBuiltin::all() {
        if !needs_exact_class_metadata[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_class_metadata[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_CLASS_METADATA",
                format!(
                    "prepared class-metadata builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableClassLineageBuiltin::all() {
        if !needs_exact_class_lineage[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_class_lineage[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_CLASS_LINEAGE",
                format!(
                    "prepared class-lineage builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableExtensionQueryBuiltin::all() {
        if !needs_exact_extension_query[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_extension_query[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_EXTENSION_QUERY",
                format!(
                    "prepared extension-query builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableMemoryQueryBuiltin::all() {
        if !needs_exact_memory_query[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_memory_query[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_MEMORY_QUERY",
                format!(
                    "prepared memory-query builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableGcBuiltin::all() {
        if !needs_exact_gc[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_gc[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_GC",
                format!(
                    "prepared GC builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableResourceQueryBuiltin::all() {
        if !needs_exact_resource_query[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_resource_query[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_RESOURCE_QUERY",
                format!(
                    "prepared resource-query builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableErrorStateBuiltin::all() {
        if !needs_exact_error_state[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_error_state[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_ERROR_STATE",
                format!(
                    "prepared error-state builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    if needs_exact_settype {
        if runtime_helpers.native_settype == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_SETTYPE",
                "prepared settype builtin requires exact handler phrust_native_settype",
            ));
        }
        imports.push((
            "phrust_native_settype".to_owned(),
            runtime_helpers.native_settype,
        ));
    }
    for builtin in StableConfigurationBuiltin::all() {
        if !needs_exact_configuration[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_configuration[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_CONFIGURATION",
                format!(
                    "prepared configuration builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableHttpResponseBuiltin::all() {
        if !needs_exact_http_response[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_http_response[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_HTTP_RESPONSE",
                format!(
                    "prepared HTTP-response builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableCookieBuiltin::all() {
        if !needs_exact_cookie[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_cookie[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_COOKIE",
                format!(
                    "prepared cookie builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableClockBuiltin::all() {
        if !needs_exact_clock[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_clock[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_CLOCK",
                format!(
                    "prepared clock builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableDateBuiltin::all() {
        if !needs_exact_date[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_date[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_DATE",
                format!(
                    "prepared date builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableRandomBuiltin::all() {
        if !needs_exact_random[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_random[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_RANDOM",
                format!(
                    "prepared random builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableRequestQueryBuiltin::all() {
        if !needs_exact_request_query[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_request_query[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_REQUEST_QUERY",
                format!(
                    "prepared request-query builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableDeclarationInventoryBuiltin::all() {
        if !needs_exact_declaration_inventory[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_declaration_inventory[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_DECLARATION_INVENTORY",
                format!(
                    "prepared declaration-inventory builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    if needs_exact_constant_inventory {
        if runtime_helpers.native_constant_inventory == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_CONSTANT_INVENTORY",
                "prepared constant-inventory builtin requires exact handler phrust_native_get_defined_constants",
            ));
        }
        imports.push((
            "phrust_native_get_defined_constants".to_owned(),
            runtime_helpers.native_constant_inventory,
        ));
    }
    if needs_exact_compact {
        if runtime_helpers.native_compact == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_COMPACT",
                "prepared compact builtin requires exact handler phrust_native_compact",
            ));
        }
        imports.push((
            "phrust_native_compact".to_owned(),
            runtime_helpers.native_compact,
        ));
    }
    for builtin in StableFrameIntrospectionBuiltin::all() {
        if !needs_exact_frame_introspection[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_frame_introspection[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_FRAME_INTROSPECTION",
                format!(
                    "prepared frame-introspection builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableBaseConversionBuiltin::all() {
        if !needs_exact_base_conversion[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_base_conversion[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_BASE_CONVERSION",
                format!(
                    "prepared base-conversion builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    if needs_exact_intval_base {
        if runtime_helpers.native_intval_base == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_INTVAL_BASE",
                "prepared two-argument intval requires exact handler phrust_native_intval_base",
            ));
        }
        imports.push((
            "phrust_native_intval_base".to_owned(),
            runtime_helpers.native_intval_base,
        ));
    }
    for builtin in StableNetworkAddressBuiltin::all() {
        if !needs_exact_network_address[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_network_address[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_NETWORK_ADDRESS",
                format!(
                    "prepared network-address builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableCompressionCodecBuiltin::all() {
        if !needs_exact_compression_codec[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_compression_codec[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_COMPRESSION_CODEC",
                format!(
                    "prepared compression-codec builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StablePathBuiltin::all() {
        if !needs_exact_path[builtin.index()] {
            continue;
        }
        let address = match builtin {
            StablePathBuiltin::Basename => runtime_helpers.native_basename,
            StablePathBuiltin::Dirname => runtime_helpers.native_dirname,
            StablePathBuiltin::Realpath => runtime_helpers.native_realpath,
            StablePathBuiltin::FileExists => runtime_helpers.native_file_exists,
            StablePathBuiltin::IsFile => runtime_helpers.native_is_file,
            StablePathBuiltin::IsDir => runtime_helpers.native_is_dir,
            StablePathBuiltin::IsReadable => runtime_helpers.native_is_readable,
            StablePathBuiltin::IsWritable => runtime_helpers.native_is_writable,
            StablePathBuiltin::IsLink => runtime_helpers.native_is_link,
            StablePathBuiltin::FilePerms => runtime_helpers.native_fileperms,
            StablePathBuiltin::FileOwner => runtime_helpers.native_fileowner,
            StablePathBuiltin::FileGroup => runtime_helpers.native_filegroup,
            StablePathBuiltin::FileType => runtime_helpers.native_filetype,
            StablePathBuiltin::DiskFreeSpace => runtime_helpers.native_disk_free_space,
            StablePathBuiltin::DiskTotalSpace => runtime_helpers.native_disk_total_space,
            StablePathBuiltin::Pathinfo => runtime_helpers.native_pathinfo,
            StablePathBuiltin::Stat => runtime_helpers.native_stat,
            StablePathBuiltin::Lstat => runtime_helpers.native_lstat,
            StablePathBuiltin::File => runtime_helpers.native_file,
            StablePathBuiltin::Glob => runtime_helpers.native_glob,
            StablePathBuiltin::OpenDir => runtime_helpers.native_opendir,
            StablePathBuiltin::ReadDir => runtime_helpers.native_readdir,
            StablePathBuiltin::RewindDir => runtime_helpers.native_rewinddir,
            StablePathBuiltin::CloseDir => runtime_helpers.native_closedir,
            StablePathBuiltin::ScanDir => runtime_helpers.native_scandir,
            StablePathBuiltin::StreamGetMetaData => runtime_helpers.native_stream_get_meta_data,
            StablePathBuiltin::StreamGetWrappers => runtime_helpers.native_stream_get_wrappers,
            StablePathBuiltin::StreamIsLocal => runtime_helpers.native_stream_is_local,
            StablePathBuiltin::StreamResolveIncludePath => {
                runtime_helpers.native_stream_resolve_include_path
            }
            StablePathBuiltin::StreamContextCreate => runtime_helpers.native_stream_context_create,
            StablePathBuiltin::StreamContextGetDefault => {
                runtime_helpers.native_stream_context_get_default
            }
            StablePathBuiltin::StreamContextGetOptions => {
                runtime_helpers.native_stream_context_get_options
            }
            StablePathBuiltin::StreamContextSetDefault => {
                runtime_helpers.native_stream_context_set_default
            }
            StablePathBuiltin::StreamContextSetOption => {
                runtime_helpers.native_stream_context_set_option
            }
            StablePathBuiltin::StreamContextSetOptions => {
                runtime_helpers.native_stream_context_set_options
            }
            StablePathBuiltin::StreamFilterAppend => runtime_helpers.native_stream_filter_append,
            StablePathBuiltin::StreamFilterPrepend => runtime_helpers.native_stream_filter_prepend,
            StablePathBuiltin::StreamFilterRemove => runtime_helpers.native_stream_filter_remove,
            StablePathBuiltin::StreamIsAtty => runtime_helpers.native_stream_isatty,
            StablePathBuiltin::StreamSetTimeout => runtime_helpers.native_stream_set_timeout,
            StablePathBuiltin::Chmod => runtime_helpers.native_chmod,
            StablePathBuiltin::Symlink => runtime_helpers.native_symlink,
            StablePathBuiltin::Readfile => runtime_helpers.native_readfile,
            StablePathBuiltin::IsUploadedFile => runtime_helpers.native_is_uploaded_file,
            StablePathBuiltin::Tempnam => runtime_helpers.native_tempnam,
            StablePathBuiltin::Tmpfile => runtime_helpers.native_tmpfile,
            StablePathBuiltin::Filesize => runtime_helpers.native_filesize,
            StablePathBuiltin::Filemtime => runtime_helpers.native_filemtime,
            StablePathBuiltin::FileGetContents => runtime_helpers.native_file_get_contents,
            StablePathBuiltin::FilePutContents => runtime_helpers.native_file_put_contents,
            StablePathBuiltin::Rename => runtime_helpers.native_rename,
            StablePathBuiltin::Unlink => runtime_helpers.native_unlink,
            StablePathBuiltin::Mkdir => runtime_helpers.native_mkdir,
            StablePathBuiltin::Rmdir => runtime_helpers.native_rmdir,
            StablePathBuiltin::Touch => runtime_helpers.native_touch,
            StablePathBuiltin::Fopen => runtime_helpers.native_fopen,
            StablePathBuiltin::Fwrite => runtime_helpers.native_fwrite,
            StablePathBuiltin::Fclose => runtime_helpers.native_fclose,
            StablePathBuiltin::Fread => runtime_helpers.native_fread,
            StablePathBuiltin::Fgets => runtime_helpers.native_fgets,
            StablePathBuiltin::Fgetc => runtime_helpers.native_fgetc,
            StablePathBuiltin::Feof => runtime_helpers.native_feof,
            StablePathBuiltin::Fflush => runtime_helpers.native_fflush,
            StablePathBuiltin::Fseek => runtime_helpers.native_fseek,
            StablePathBuiltin::Ftell => runtime_helpers.native_ftell,
            StablePathBuiltin::Ftruncate => runtime_helpers.native_ftruncate,
            StablePathBuiltin::Rewind => runtime_helpers.native_rewind,
            StablePathBuiltin::StreamGetContents => runtime_helpers.native_stream_get_contents,
            StablePathBuiltin::StreamCopyToStream => runtime_helpers.native_stream_copy_to_stream,
        };
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_PATH",
                format!(
                    "prepared path builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    for builtin in StableOutputBufferBuiltin::all() {
        if !needs_exact_output_buffer[builtin.index()] {
            continue;
        }
        let address = runtime_helpers.native_output_buffer[builtin.index()];
        if address == 0 {
            return Err(CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_EXACT_OUTPUT_BUFFER",
                format!(
                    "prepared output-buffer builtin requires exact handler {}",
                    builtin.symbol()
                ),
            ));
        }
        imports.push((builtin.symbol().to_owned(), address));
    }
    if baseline_helper_imports && needs_semantic_dispatch {
        imports.push((
            baseline_semantic_dispatch_symbol.clone(),
            runtime_helpers.baseline_semantic_dispatch,
        ));
    }
    if baseline_helper_imports && needs_function_resolver {
        imports.push((
            native_function_resolve_symbol.clone(),
            runtime_helpers.native_function_resolve,
        ));
    }
    if baseline_helper_imports && needs_frame_arena {
        imports.push((
            "phrust_native_frame_alloc".to_owned(),
            runtime_helpers.native_frame_alloc,
        ));
        imports.push((
            "phrust_native_frame_release".to_owned(),
            runtime_helpers.native_frame_release,
        ));
    }
    if baseline_helper_imports && needs_dynamic_code {
        imports.push((
            native_dynamic_code_symbol.clone(),
            runtime_helpers.native_dynamic_code,
        ));
    }
    for (needed, configured, fallback, symbol) in [
        (
            needs_unary,
            runtime_helpers.baseline_unary,
            test_native_unary_fallback as *const () as usize,
            "phrust_baseline_native_unary",
        ),
        (
            needs_baseline_binary,
            runtime_helpers.baseline_binary,
            test_baseline_binary_fallback as *const () as usize,
            "phrust_baseline_native_binary",
        ),
        (
            needs_compare,
            runtime_helpers.baseline_compare,
            test_native_compare_fallback as *const () as usize,
            "phrust_baseline_native_compare",
        ),
        (
            needs_cast,
            runtime_helpers.baseline_cast,
            test_native_cast_fallback as *const () as usize,
            "phrust_baseline_native_cast",
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
            needs_value_release,
            runtime_helpers.native_value_release,
            test_native_value_release_fallback as *const () as usize,
            "phrust_native_value_release",
        ),
        (
            needs_reference_bind,
            runtime_helpers.native_reference_bind,
            test_native_reference_bind_fallback as *const () as usize,
            "phrust_native_reference_bind",
        ),
        (
            needs_argument_check,
            runtime_helpers.native_argument_check,
            test_native_argument_check_fallback as *const () as usize,
            "phrust_native_argument_check",
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
            needs_array_insert,
            runtime_helpers.native_array_insert_local,
            test_native_array_insert_fallback as *const () as usize,
            "phrust_native_array_insert_local",
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
            needs_type_predicate,
            runtime_helpers.native_type_predicate,
            test_native_type_predicate_fallback as *const () as usize,
            "phrust_native_type_predicate",
        ),
        (
            needs_stable_length,
            runtime_helpers.native_stable_length,
            test_native_stable_length_fallback as *const () as usize,
            "phrust_native_stable_length",
        ),
        (
            needs_string_predicate,
            runtime_helpers.native_string_predicate,
            test_native_string_predicate_fallback as *const () as usize,
            "phrust_native_string_predicate",
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
        if needed && (baseline_helper_imports || symbol == "phrust_native_execution_poll") {
            let address = if configured == 0 {
                fallback
            } else {
                configured
            };
            imports.push((symbol.to_owned(), address));
        }
    }
    for (needed, configured, symbol) in [
        (
            needs_array_union,
            runtime_helpers.native_array_union,
            "phrust_native_array_union",
        ),
        (
            needs_concat,
            runtime_helpers.native_concat,
            "phrust_native_concat",
        ),
        (
            needs_string_bitwise[0],
            runtime_helpers.native_string_bitwise[0],
            "phrust_native_bit_and",
        ),
        (
            needs_string_bitwise[1],
            runtime_helpers.native_string_bitwise[1],
            "phrust_native_bit_or",
        ),
        (
            needs_string_bitwise[2],
            runtime_helpers.native_string_bitwise[2],
            "phrust_native_bit_xor",
        ),
    ] {
        if !needed {
            continue;
        }
        imports.push((
            symbol.to_owned(),
            if configured == 0 {
                test_total_representation_binary_fallback as *const () as usize
            } else {
                configured
            },
        ));
    }
    for operation in NATIVE_EXACT_UNARY_OPERATIONS {
        let index = native_exact_unary_index(operation);
        if !needs_exact_unary[index] {
            continue;
        }
        let configured = runtime_helpers.native_exact_unary[index];
        imports.push((
            native_exact_unary_symbol(operation).to_owned(),
            if configured == 0 {
                test_native_exact_unary_fallback as *const () as usize
            } else {
                configured
            },
        ));
    }
    for operation in NATIVE_EXACT_COMPARE_OPERATIONS {
        let index = native_exact_compare_index(operation);
        if !needs_exact_compare[index] {
            continue;
        }
        let configured = runtime_helpers.native_exact_compare[index];
        imports.push((
            native_exact_compare_symbol(operation).to_owned(),
            if configured == 0 {
                test_native_exact_compare_fallback as *const () as usize
            } else {
                configured
            },
        ));
    }
    if needs_direct_echo {
        imports.push((
            "phrust_native_echo_bytes".to_owned(),
            if runtime_helpers.native_echo_bytes == 0 {
                test_native_echo_bytes_fallback as *const () as usize
            } else {
                runtime_helpers.native_echo_bytes
            },
        ));
    }
    if needs_float_to_string {
        imports.push((
            "phrust_native_float_to_string".to_owned(),
            if runtime_helpers.native_float_to_string == 0 {
                test_native_float_to_string_fallback as *const () as usize
            } else {
                runtime_helpers.native_float_to_string
            },
        ));
    }
    if needs_numeric_string {
        imports.push((
            "phrust_native_numeric_string".to_owned(),
            if runtime_helpers.native_numeric_string == 0 {
                test_native_numeric_string_fallback as *const () as usize
            } else {
                runtime_helpers.native_numeric_string
            },
        ));
    }
    if needs_fmod_f64 {
        imports.push((
            "phrust_native_fmod_f64".to_owned(),
            if runtime_helpers.native_fmod_f64 == 0 {
                test_native_fmod_f64_fallback as *const () as usize
            } else {
                runtime_helpers.native_fmod_f64
            },
        ));
    }
    if needs_round_f64 {
        imports.push((
            "phrust_native_round_f64".to_owned(),
            if runtime_helpers.native_round_f64 == 0 {
                test_native_round_f64_fallback as *const () as usize
            } else {
                runtime_helpers.native_round_f64
            },
        ));
    }
    for builtin in StablePureMathBuiltin::all() {
        if !needs_exact_pure_math[builtin.index()] {
            continue;
        }
        let configured = runtime_helpers.native_pure_math[builtin.index()];
        imports.push((
            builtin.symbol().to_owned(),
            if configured == 0 {
                test_native_pure_math_fallback(builtin)
            } else {
                configured
            },
        ));
    }
    if needs_array_cast {
        imports.push((
            "phrust_native_array_cast".to_owned(),
            if runtime_helpers.native_array_cast == 0 {
                test_native_array_cast_fallback as *const () as usize
            } else {
                runtime_helpers.native_array_cast
            },
        ));
    }
    if needs_int_cast {
        imports.push((
            "phrust_native_int_cast".to_owned(),
            if runtime_helpers.native_int_cast == 0 {
                test_native_int_cast_fallback as *const () as usize
            } else {
                runtime_helpers.native_int_cast
            },
        ));
    }
    if needs_float_cast {
        imports.push((
            "phrust_native_float_cast".to_owned(),
            if runtime_helpers.native_float_cast == 0 {
                test_native_float_cast_fallback as *const () as usize
            } else {
                runtime_helpers.native_float_cast
            },
        ));
    }
    if needs_string_cast {
        imports.push((
            "phrust_native_string_cast".to_owned(),
            if runtime_helpers.native_string_cast == 0 {
                test_native_string_cast_fallback as *const () as usize
            } else {
                runtime_helpers.native_string_cast
            },
        ));
    }
    if needs_callback_return_string {
        imports.push((
            "phrust_native_callback_return_string".to_owned(),
            if runtime_helpers.native_callback_return_string == 0 {
                test_native_string_cast_fallback as *const () as usize
            } else {
                runtime_helpers.native_callback_return_string
            },
        ));
    }
    if needs_object_class_name {
        imports.push((
            "phrust_native_object_class_name".to_owned(),
            if runtime_helpers.native_object_class_name == 0 {
                test_native_object_class_name_fallback as *const () as usize
            } else {
                runtime_helpers.native_object_class_name
            },
        ));
    }
    if needs_acquire_callable {
        imports.push((
            "phrust_native_acquire_callable".to_owned(),
            if runtime_helpers.native_acquire_callable == 0 {
                test_native_object_class_name_fallback as *const () as usize
            } else {
                runtime_helpers.native_acquire_callable
            },
        ));
    }
    if needs_resolve_callable {
        imports.push((
            "phrust_native_resolve_callable".to_owned(),
            if runtime_helpers.native_resolve_callable == 0 {
                test_native_object_class_name_fallback as *const () as usize
            } else {
                runtime_helpers.native_resolve_callable
            },
        ));
    }
    if needs_object_cast {
        imports.push((
            "phrust_native_object_cast".to_owned(),
            if runtime_helpers.native_object_cast == 0 {
                test_native_object_cast_fallback as *const () as usize
            } else {
                runtime_helpers.native_object_cast
            },
        ));
    }
    if needs_prepared_object_new {
        imports.push((
            "phrust_native_prepared_object_new".to_owned(),
            if runtime_helpers.native_prepared_object_new == 0 {
                test_native_prepared_object_new_fallback as *const () as usize
            } else {
                runtime_helpers.native_prepared_object_new
            },
        ));
    }
    if needs_prepared_exception_new {
        imports.push((
            "phrust_native_prepared_exception_new".to_owned(),
            if runtime_helpers.native_prepared_exception_new == 0 {
                test_native_prepared_exception_new_fallback as *const () as usize
            } else {
                runtime_helpers.native_prepared_exception_new
            },
        ));
    }
    if needs_prepared_closure_new {
        imports.push((
            "phrust_native_prepared_closure_new".to_owned(),
            if runtime_helpers.native_prepared_closure_new == 0 {
                test_native_prepared_closure_new_fallback as *const () as usize
            } else {
                runtime_helpers.native_prepared_closure_new
            },
        ));
    }
    if needs_plain_object_clone {
        imports.push((
            "phrust_native_plain_object_clone".to_owned(),
            if runtime_helpers.native_plain_object_clone == 0 {
                test_native_plain_object_clone_fallback as *const () as usize
            } else {
                runtime_helpers.native_plain_object_clone
            },
        ));
    }
    if needs_dynamic_property_slot {
        imports.push((
            "phrust_native_dynamic_property_slot".to_owned(),
            if runtime_helpers.native_dynamic_property_slot == 0 {
                test_native_dynamic_property_fallback as *const () as usize
            } else {
                runtime_helpers.native_dynamic_property_slot
            },
        ));
    }
    if needs_dynamic_property_test_slot {
        imports.push((
            "phrust_native_dynamic_property_test_slot".to_owned(),
            if runtime_helpers.native_dynamic_property_test_slot == 0 {
                test_native_dynamic_property_fallback as *const () as usize
            } else {
                runtime_helpers.native_dynamic_property_test_slot
            },
        ));
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
    let function_key = native_function_key(
        request
            .deployment_identity
            .clone()
            .unwrap_or_else(|| crate::stable_ir_fingerprint(unit)),
        function.raw(),
        unit.functions[function.index()].params.len(),
        region.local_count,
        request.opt_level >= 2,
        request.invalidation_generation,
    );
    let compiled_clif_blocks = std::cell::Cell::new(None);
    let compiled_maximum_pre_regalloc = std::cell::Cell::new(None);
    let compiled_maximum_temporary_cache_entries = std::cell::Cell::new(None);
    let compiled_pre_regalloc_replans = std::cell::Cell::new(0_usize);
    let compiled = compile_managed_native(
        request,
        function,
        function_key,
        if compilation_mode
            == crate::cranelift_lowering::baseline_streaming::NativeCompilationMode::StreamingBaseline
        {
            crate::code_manager::NativeCompileAdmission::request_critical(
                plan.admission_cost_tokens(),
            )
        } else {
            crate::code_manager::NativeCompileAdmission::background_optimizing(
                plan.admission_cost_tokens(),
            )
        },
        compilation_mode.specialization(),
        &import_refs,
        |module, codegen_context, builder_context, name| {
            let mut active_plan = selected_plan.borrow().clone();
            let mut active_fragment_layout = selected_fragment_layout.borrow().clone();
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
                    &baseline_call_symbol,
                    &signature,
                    helper_address(&baseline_call_symbol),
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
            let mut native_operations = BaselineNativeOperations::default();
            let pointer_type = module.target_config().pointer_type();
            let mut exact_symbol_query = [None; StableSymbolQueryBuiltin::COUNT];
            for builtin in StableSymbolQueryBuiltin::all() {
                if !needs_exact_symbol_query[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_symbol_query[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_pcre = [None; StablePcreBuiltin::COUNT];
            for builtin in StablePcreBuiltin::all() {
                if !needs_exact_pcre[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StablePcreBuiltin::LastError | StablePcreBuiltin::LastErrorMessage => 0,
                    StablePcreBuiltin::Quote => 2,
                    StablePcreBuiltin::Grep => 3,
                    StablePcreBuiltin::Split => 4,
                    StablePcreBuiltin::Match
                    | StablePcreBuiltin::MatchAll
                    | StablePcreBuiltin::Replace
                    | StablePcreBuiltin::Filter => 5,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_pcre[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let preg_callback_plan = if needs_preg_callback {
                let mut signature = module.make_signature();
                for _ in 0..5 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_preg_callback_plan",
                    &signature,
                    helper_address("phrust_native_preg_callback_plan"),
                )?)
            } else {
                None
            };
            let preg_callback_assemble = if needs_preg_callback {
                let mut signature = module.make_signature();
                for _ in 0..3 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_preg_callback_assemble",
                    &signature,
                    helper_address("phrust_native_preg_callback_assemble"),
                )?)
            } else {
                None
            };
            let mut exact_json = [None; StableJsonBuiltin::COUNT];
            for builtin in StableJsonBuiltin::all() {
                if !needs_exact_json[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableJsonBuiltin::LastError | StableJsonBuiltin::LastErrorMessage => 0,
                    StableJsonBuiltin::Encode | StableJsonBuiltin::Validate => 3,
                    StableJsonBuiltin::Decode => 4,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_json[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_format = [None; StableFormatBuiltin::COUNT];
            for builtin in StableFormatBuiltin::all() {
                if !needs_exact_format[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                match builtin {
                    StableFormatBuiltin::Sprintf | StableFormatBuiltin::Printf => {
                        signature.params.push(AbiParam::new(types::I32));
                        signature
                            .params
                            .push(AbiParam::new(module.target_config().pointer_type()));
                    }
                    StableFormatBuiltin::Vsprintf | StableFormatBuiltin::Vprintf => {
                        signature.params.push(AbiParam::new(types::I64));
                        signature.params.push(AbiParam::new(types::I64));
                    }
                    StableFormatBuiltin::NumberFormat => {
                        for _ in 0..4 {
                            signature.params.push(AbiParam::new(types::I64));
                        }
                    }
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_format[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_hash = [None; StableHashBuiltin::COUNT];
            for builtin in StableHashBuiltin::all() {
                if !needs_exact_hash[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableHashBuiltin::Crc32 => 1,
                    StableHashBuiltin::Md5
                    | StableHashBuiltin::Sha1
                    | StableHashBuiltin::HashEquals => 2,
                    StableHashBuiltin::Hash | StableHashBuiltin::HashHmac => 4,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_hash[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_byte_codec = [None; StableByteCodecBuiltin::COUNT];
            for builtin in StableByteCodecBuiltin::all() {
                if !needs_exact_byte_codec[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                match builtin {
                    StableByteCodecBuiltin::Pack => {
                        signature.params.push(AbiParam::new(types::I32));
                        signature
                            .params
                            .push(AbiParam::new(module.target_config().pointer_type()));
                    }
                    StableByteCodecBuiltin::Unpack => {
                        for _ in 0..3 {
                            signature.params.push(AbiParam::new(types::I64));
                        }
                    }
                    StableByteCodecBuiltin::Base64Decode
                    | StableByteCodecBuiltin::AddCSlashes => {
                        for _ in 0..2 {
                            signature.params.push(AbiParam::new(types::I64));
                        }
                    }
                    StableByteCodecBuiltin::Base64Encode
                    | StableByteCodecBuiltin::Bin2Hex
                    | StableByteCodecBuiltin::Hex2Bin
                    | StableByteCodecBuiltin::QuotedPrintableDecode
                    | StableByteCodecBuiltin::UrlEncode
                    | StableByteCodecBuiltin::RawUrlEncode
                    | StableByteCodecBuiltin::UrlDecode
                    | StableByteCodecBuiltin::RawUrlDecode
                    | StableByteCodecBuiltin::UuEncode
                    | StableByteCodecBuiltin::UuDecode
                    | StableByteCodecBuiltin::StripCSlashes
                    | StableByteCodecBuiltin::StripSlashes
                    | StableByteCodecBuiltin::QuoteMeta => {
                        signature.params.push(AbiParam::new(types::I64));
                    }
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_byte_codec[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_string_search_compare =
                [None; StableStringSearchCompareBuiltin::COUNT];
            for builtin in StableStringSearchCompareBuiltin::all() {
                if !needs_exact_string_search_compare[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableStringSearchCompareBuiltin::StrPBrk
                    | StableStringSearchCompareBuiltin::StrNatCmp
                    | StableStringSearchCompareBuiltin::StrNatCaseCmp => 2,
                    StableStringSearchCompareBuiltin::StrStr
                    | StableStringSearchCompareBuiltin::StrIStr
                    | StableStringSearchCompareBuiltin::StrRChr => 3,
                    StableStringSearchCompareBuiltin::SubstrCompare => 5,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_string_search_compare[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_string_rewrite = [None; StableStringRewriteBuiltin::COUNT];
            for builtin in StableStringRewriteBuiltin::all() {
                if !needs_exact_string_rewrite[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableStringRewriteBuiltin::UcWords
                    | StableStringRewriteBuiltin::StripTags
                    | StableStringRewriteBuiltin::StrSplit => 2,
                    StableStringRewriteBuiltin::StrTr
                    | StableStringRewriteBuiltin::VersionCompare => 3,
                    StableStringRewriteBuiltin::StrPad
                    | StableStringRewriteBuiltin::SubstrReplace => 4,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_string_rewrite[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_html_codec = [None; StableHtmlCodecBuiltin::COUNT];
            for builtin in StableHtmlCodecBuiltin::all() {
                if !needs_exact_html_codec[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableHtmlCodecBuiltin::SpecialChars | StableHtmlCodecBuiltin::Entities => 4,
                    StableHtmlCodecBuiltin::EntityDecode => 3,
                    StableHtmlCodecBuiltin::SpecialCharsDecode => 2,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_html_codec[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_url_query = [None; StableUrlQueryBuiltin::COUNT];
            for builtin in StableUrlQueryBuiltin::all() {
                if !needs_exact_url_query[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableUrlQueryBuiltin::ParseUrl | StableUrlQueryBuiltin::ParseStr => 2,
                    StableUrlQueryBuiltin::HttpBuildQuery => 4,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_url_query[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_array_aggregate = [None; StableArrayAggregateBuiltin::COUNT];
            for builtin in StableArrayAggregateBuiltin::all() {
                if !needs_exact_array_aggregate[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableArrayAggregateBuiltin::Sum => 1,
                    StableArrayAggregateBuiltin::Count
                    | StableArrayAggregateBuiltin::SizeOf => 2,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_array_aggregate[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_recursive_array = [None; StableRecursiveArrayBuiltin::COUNT];
            for builtin in StableRecursiveArrayBuiltin::all() {
                if !needs_exact_recursive_array[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_recursive_array[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_array_sort = [None; StableArraySortBuiltin::COUNT];
            for builtin in StableArraySortBuiltin::all() {
                if !needs_exact_array_sort[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_array_sort[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let exact_array_multisort = if needs_exact_array_multisort {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(module.target_config().pointer_type()));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_array_multisort",
                    &signature,
                    helper_address("phrust_native_array_multisort"),
                )?)
            } else {
                None
            };
            let mut exact_frame_introspection =
                [None; StableFrameIntrospectionBuiltin::COUNT];
            let mut exact_object_identity = [None; StableObjectIdentityBuiltin::COUNT];
            for builtin in StableObjectIdentityBuiltin::all() {
                if !needs_exact_object_identity[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_object_identity[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_callable_query = [None; StableCallableQueryBuiltin::COUNT];
            for builtin in StableCallableQueryBuiltin::all() {
                if !needs_exact_callable_query[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                for _ in 0..3 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_callable_query[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_callback_handler = [None; StableCallbackHandlerBuiltin::COUNT];
            for builtin in StableCallbackHandlerBuiltin::all() {
                if !needs_exact_callback_handler[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                for _ in 0..2 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_callback_handler[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_autoload_callback = [None; StableAutoloadCallbackBuiltin::COUNT];
            for builtin in StableAutoloadCallbackBuiltin::all() {
                if !needs_exact_autoload_callback[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                for _ in 0..3 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_autoload_callback[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let exact_shutdown_callback = if needs_exact_shutdown_callback {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                signature
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_register_shutdown_function",
                    &signature,
                    helper_address("phrust_native_register_shutdown_function"),
                )?)
            } else {
                None
            };
            let mut exact_serialization = [None; StableSerializationBuiltin::COUNT];
            for builtin in StableSerializationBuiltin::all() {
                if !needs_exact_serialization[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_serialization[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_tokenizer = [None; StableTokenizerBuiltin::COUNT];
            for builtin in StableTokenizerBuiltin::all() {
                if !needs_exact_tokenizer[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableTokenizerBuiltin::GetAll => 2,
                    StableTokenizerBuiltin::Name => 1,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_tokenizer[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_mbstring = [None; StableMbstringBuiltin::COUNT];
            for builtin in StableMbstringBuiltin::all() {
                if !needs_exact_mbstring[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableMbstringBuiltin::ListEncodings => 0,
                    StableMbstringBuiltin::InternalEncoding
                    | StableMbstringBuiltin::EncodingAliases
                    | StableMbstringBuiltin::SubstituteCharacter => 1,
                    StableMbstringBuiltin::CheckEncoding
                    | StableMbstringBuiltin::Strlen
                    | StableMbstringBuiltin::Strtolower
                    | StableMbstringBuiltin::Strtoupper
                    | StableMbstringBuiltin::Strwidth
                    | StableMbstringBuiltin::Ucfirst
                    | StableMbstringBuiltin::Lcfirst
                    | StableMbstringBuiltin::Ord
                    | StableMbstringBuiltin::Chr
                    | StableMbstringBuiltin::ParseStr => 2,
                    StableMbstringBuiltin::DetectEncoding
                    | StableMbstringBuiltin::ConvertEncoding
                    | StableMbstringBuiltin::SubstrCount
                    | StableMbstringBuiltin::ConvertCase => 3,
                    StableMbstringBuiltin::Stripos
                    | StableMbstringBuiltin::Strpos
                    | StableMbstringBuiltin::Strripos
                    | StableMbstringBuiltin::Strrpos
                    | StableMbstringBuiltin::Substr
                    | StableMbstringBuiltin::Strcut => 4,
                    StableMbstringBuiltin::Strimwidth => 5,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_mbstring[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_bcmath = [None; StableBcmathBuiltin::COUNT];
            for builtin in StableBcmathBuiltin::all() {
                if !needs_exact_bcmath[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableBcmathBuiltin::Scale => 1,
                    StableBcmathBuiltin::Sqrt => 2,
                    StableBcmathBuiltin::Add
                    | StableBcmathBuiltin::Comp
                    | StableBcmathBuiltin::Div
                    | StableBcmathBuiltin::Mod
                    | StableBcmathBuiltin::Mul
                    | StableBcmathBuiltin::Pow
                    | StableBcmathBuiltin::Sub => 3,
                    StableBcmathBuiltin::PowMod => 4,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_bcmath[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_filter = [None; StableFilterBuiltin::COUNT];
            for builtin in StableFilterBuiltin::all() {
                if !needs_exact_filter[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableFilterBuiltin::Input => 4,
                    StableFilterBuiltin::HasVar => 2,
                    StableFilterBuiltin::InputArray
                    | StableFilterBuiltin::VarArray
                    | StableFilterBuiltin::Var => 3,
                    StableFilterBuiltin::List => 0,
                    StableFilterBuiltin::Id => 1,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_filter[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_session = [None; StableSessionBuiltin::COUNT];
            for builtin in StableSessionBuiltin::all() {
                if !needs_exact_session[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableSessionBuiltin::CacheExpire
                    | StableSessionBuiltin::CacheLimiter
                    | StableSessionBuiltin::Decode
                    | StableSessionBuiltin::CreateId
                    | StableSessionBuiltin::Id
                    | StableSessionBuiltin::ModuleName
                    | StableSessionBuiltin::Name
                    | StableSessionBuiltin::RegenerateId
                    | StableSessionBuiltin::SavePath
                    | StableSessionBuiltin::Start => 1,
                    StableSessionBuiltin::SetCookieParams => 5,
                    StableSessionBuiltin::SetSaveHandler => 9,
                    StableSessionBuiltin::Abort
                    | StableSessionBuiltin::Commit
                    | StableSessionBuiltin::Destroy
                    | StableSessionBuiltin::Gc
                    | StableSessionBuiltin::Encode
                    | StableSessionBuiltin::GetCookieParams
                    | StableSessionBuiltin::RegisterShutdown
                    | StableSessionBuiltin::Reset
                    | StableSessionBuiltin::Status
                    | StableSessionBuiltin::Unset
                    | StableSessionBuiltin::WriteClose => 0,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_session[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_object_vars = [None; StableObjectVarsBuiltin::COUNT];
            for builtin in StableObjectVarsBuiltin::all() {
                if !needs_exact_object_vars[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_object_vars[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_class_metadata = [None; StableClassMetadataBuiltin::COUNT];
            for builtin in StableClassMetadataBuiltin::all() {
                if !needs_exact_class_metadata[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_class_metadata[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_class_lineage = [None; StableClassLineageBuiltin::COUNT];
            for builtin in StableClassLineageBuiltin::all() {
                if !needs_exact_class_lineage[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableClassLineageBuiltin::ParentClass => 1,
                    StableClassLineageBuiltin::Implements => 2,
                    StableClassLineageBuiltin::IsSubclassOf | StableClassLineageBuiltin::IsA => 3,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_class_lineage[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_extension_query = [None; StableExtensionQueryBuiltin::COUNT];
            for builtin in StableExtensionQueryBuiltin::all() {
                if !needs_exact_extension_query[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_extension_query[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_memory_query = [None; StableMemoryQueryBuiltin::COUNT];
            for builtin in StableMemoryQueryBuiltin::all() {
                if !needs_exact_memory_query[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_memory_query[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_gc = [None; StableGcBuiltin::COUNT];
            for builtin in StableGcBuiltin::all() {
                if !needs_exact_gc[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_gc[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_resource_query = [None; StableResourceQueryBuiltin::COUNT];
            for builtin in StableResourceQueryBuiltin::all() {
                if !needs_exact_resource_query[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_resource_query[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_error_state = [None; StableErrorStateBuiltin::COUNT];
            for builtin in StableErrorStateBuiltin::all() {
                if !needs_exact_error_state[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_error_state[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let exact_settype = if needs_exact_settype {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_settype",
                    &signature,
                    helper_address("phrust_native_settype"),
                )?)
            } else {
                None
            };
            let mut exact_configuration = [None; StableConfigurationBuiltin::COUNT];
            for builtin in StableConfigurationBuiltin::all() {
                if !needs_exact_configuration[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableConfigurationBuiltin::IncludePath
                    | StableConfigurationBuiltin::TimezoneGet => 0,
                    StableConfigurationBuiltin::IniGet
                    | StableConfigurationBuiltin::CfgVar
                    | StableConfigurationBuiltin::SetIncludePath
                    | StableConfigurationBuiltin::TimezoneSet => 1,
                    StableConfigurationBuiltin::IniGetAll
                    | StableConfigurationBuiltin::IniSet => 2,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_configuration[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_http_response = [None; StableHttpResponseBuiltin::COUNT];
            for builtin in StableHttpResponseBuiltin::all() {
                if !needs_exact_http_response[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableHttpResponseBuiltin::HeadersList
                    | StableHttpResponseBuiltin::HeadersSent => 0,
                    StableHttpResponseBuiltin::HeaderRemove
                    | StableHttpResponseBuiltin::ResponseCode => 1,
                    StableHttpResponseBuiltin::Header => 3,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_http_response[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_cookie = [None; StableCookieBuiltin::COUNT];
            for builtin in StableCookieBuiltin::all() {
                if !needs_exact_cookie[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                for _ in 0..7 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_cookie[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_clock = [None; StableClockBuiltin::COUNT];
            for builtin in StableClockBuiltin::all() {
                if !needs_exact_clock[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableClockBuiltin::Time => 0,
                    StableClockBuiltin::Microtime | StableClockBuiltin::Hrtime => 1,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_clock[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_date = [None; StableDateBuiltin::COUNT];
            for builtin in StableDateBuiltin::all() {
                if !needs_exact_date[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableDateBuiltin::TimezoneIdentifiers => 0,
                    StableDateBuiltin::Date
                    | StableDateBuiltin::Gmdate
                    | StableDateBuiltin::Strtotime => 2,
                    StableDateBuiltin::Checkdate => 3,
                    StableDateBuiltin::Mktime | StableDateBuiltin::Gmmktime => 6,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_date[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_random = [None; StableRandomBuiltin::COUNT];
            for builtin in StableRandomBuiltin::all() {
                if !needs_exact_random[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableRandomBuiltin::GetRandMax | StableRandomBuiltin::MtGetRandMax => 0,
                    StableRandomBuiltin::RandomBytes | StableRandomBuiltin::Shuffle => 1,
                    StableRandomBuiltin::RandomInt
                    | StableRandomBuiltin::Rand
                    | StableRandomBuiltin::MtRand
                    | StableRandomBuiltin::ArrayRand => 2,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_random[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_request_query = [None; StableRequestQueryBuiltin::COUNT];
            for builtin in StableRequestQueryBuiltin::all() {
                if !needs_exact_request_query[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableRequestQueryBuiltin::Environment
                    | StableRequestQueryBuiltin::Uname
                    | StableRequestQueryBuiltin::ChangeDirectory
                    | StableRequestQueryBuiltin::Umask => 1,
                    StableRequestQueryBuiltin::ClearStatCache => 2,
                    StableRequestQueryBuiltin::TempDir
                    | StableRequestQueryBuiltin::CurrentDirectory
                    | StableRequestQueryBuiltin::SapiName
                    | StableRequestQueryBuiltin::CurrentUser
                    | StableRequestQueryBuiltin::IncludedFiles => 0,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_request_query[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_declaration_inventory =
                [None; StableDeclarationInventoryBuiltin::COUNT];
            for builtin in StableDeclarationInventoryBuiltin::all() {
                if !needs_exact_declaration_inventory[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_declaration_inventory[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let exact_constant_inventory = if needs_exact_constant_inventory {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_get_defined_constants",
                    &signature,
                    helper_address("phrust_native_get_defined_constants"),
                )?)
            } else {
                None
            };
            let exact_compact = if needs_exact_compact {
                let mut signature = module.make_signature();
                for _ in 0..5 {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_compact",
                    &signature,
                    helper_address("phrust_native_compact"),
                )?)
            } else {
                None
            };
            for builtin in StableFrameIntrospectionBuiltin::all() {
                if !needs_exact_frame_introspection[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableFrameIntrospectionBuiltin::NumArgs
                    | StableFrameIntrospectionBuiltin::GetArgs => 0,
                    StableFrameIntrospectionBuiltin::GetArg => 1,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_frame_introspection[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_base_conversion = [None; StableBaseConversionBuiltin::COUNT];
            for builtin in StableBaseConversionBuiltin::all() {
                if !needs_exact_base_conversion[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableBaseConversionBuiltin::BaseConvert => 3,
                    StableBaseConversionBuiltin::BinDec
                    | StableBaseConversionBuiltin::DecBin
                    | StableBaseConversionBuiltin::DecHex
                    | StableBaseConversionBuiltin::DecOct
                    | StableBaseConversionBuiltin::HexDec
                    | StableBaseConversionBuiltin::OctDec => 1,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_base_conversion[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let exact_intval_base = if needs_exact_intval_base {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_intval_base",
                    &signature,
                    helper_address("phrust_native_intval_base"),
                )?)
            } else {
                None
            };
            let mut exact_network_address = [None; StableNetworkAddressBuiltin::COUNT];
            for builtin in StableNetworkAddressBuiltin::all() {
                if !needs_exact_network_address[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_network_address[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_compression_codec = [None; StableCompressionCodecBuiltin::COUNT];
            for builtin in StableCompressionCodecBuiltin::all() {
                if !needs_exact_compression_codec[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StableCompressionCodecBuiltin::GzDecode
                    | StableCompressionCodecBuiltin::GzUncompress
                    | StableCompressionCodecBuiltin::GzInflate
                    | StableCompressionCodecBuiltin::ZlibDecode => 2,
                    StableCompressionCodecBuiltin::GzEncode
                    | StableCompressionCodecBuiltin::GzCompress
                    | StableCompressionCodecBuiltin::GzDeflate
                    | StableCompressionCodecBuiltin::ZlibEncode => 3,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_compression_codec[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_path = [None; StablePathBuiltin::COUNT];
            for builtin in StablePathBuiltin::all() {
                if !needs_exact_path[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                let arity = match builtin {
                    StablePathBuiltin::Realpath
                    | StablePathBuiltin::FileExists
                    | StablePathBuiltin::IsFile
                    | StablePathBuiltin::IsDir
                    | StablePathBuiltin::IsReadable
                    | StablePathBuiltin::IsWritable
                    | StablePathBuiltin::IsLink
                    | StablePathBuiltin::FilePerms
                    | StablePathBuiltin::FileOwner
                    | StablePathBuiltin::FileGroup
                    | StablePathBuiltin::FileType
                    | StablePathBuiltin::DiskFreeSpace
                    | StablePathBuiltin::DiskTotalSpace
                    | StablePathBuiltin::Stat
                    | StablePathBuiltin::Lstat
                    | StablePathBuiltin::Filesize
                    | StablePathBuiltin::Filemtime
                    | StablePathBuiltin::Unlink
                    | StablePathBuiltin::Mkdir
                    | StablePathBuiltin::Rmdir
                    | StablePathBuiltin::Touch
                    | StablePathBuiltin::Fclose
                    | StablePathBuiltin::Fgetc
                    | StablePathBuiltin::Feof
                    | StablePathBuiltin::Fflush
                    | StablePathBuiltin::Ftell
                    | StablePathBuiltin::Rewind
                    | StablePathBuiltin::OpenDir
                    | StablePathBuiltin::ReadDir
                    | StablePathBuiltin::RewindDir
                    | StablePathBuiltin::CloseDir
                    | StablePathBuiltin::StreamGetMetaData
                    | StablePathBuiltin::StreamIsLocal
                    | StablePathBuiltin::StreamResolveIncludePath
                    | StablePathBuiltin::StreamContextCreate
                    | StablePathBuiltin::StreamContextGetDefault
                    | StablePathBuiltin::StreamContextGetOptions
                    | StablePathBuiltin::StreamContextSetDefault
                    | StablePathBuiltin::StreamFilterRemove
                    | StablePathBuiltin::StreamIsAtty
                    | StablePathBuiltin::Readfile
                    | StablePathBuiltin::IsUploadedFile => 1,
                    StablePathBuiltin::StreamGetWrappers | StablePathBuiltin::Tmpfile => 0,
                    StablePathBuiltin::Basename
                    | StablePathBuiltin::Dirname
                    | StablePathBuiltin::Pathinfo
                    | StablePathBuiltin::Glob
                    | StablePathBuiltin::Rename
                    | StablePathBuiltin::Fopen
                    | StablePathBuiltin::Fread
                    | StablePathBuiltin::Fgets
                    | StablePathBuiltin::Ftruncate
                    | StablePathBuiltin::ScanDir
                    | StablePathBuiltin::StreamContextSetOptions
                    | StablePathBuiltin::Chmod
                    | StablePathBuiltin::Symlink
                    | StablePathBuiltin::Tempnam => 2,
                    StablePathBuiltin::Fwrite
                    | StablePathBuiltin::Fseek
                    | StablePathBuiltin::File
                    | StablePathBuiltin::StreamGetContents
                    | StablePathBuiltin::StreamSetTimeout => 3,
                    StablePathBuiltin::StreamCopyToStream
                    | StablePathBuiltin::FilePutContents
                    | StablePathBuiltin::StreamContextSetOption
                    | StablePathBuiltin::StreamFilterAppend
                    | StablePathBuiltin::StreamFilterPrepend => 4,
                    StablePathBuiltin::FileGetContents => 5,
                };
                for _ in 0..arity {
                    signature.params.push(AbiParam::new(types::I64));
                }
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_path[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let mut exact_output_buffer = [None; StableOutputBufferBuiltin::COUNT];
            for builtin in StableOutputBufferBuiltin::all() {
                if !needs_exact_output_buffer[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                exact_output_buffer[builtin.index()] = Some(declare_native_helper(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let array_union = declare_native_control_handler(
                module,
                needs_array_union,
                "phrust_native_array_union",
                2,
                || helper_address("phrust_native_array_union"),
            )?;
            let concat = declare_native_control_handler(
                module,
                needs_concat,
                "phrust_native_concat",
                2,
                || helper_address("phrust_native_concat"),
            )?;
            let mut string_bitwise = [None; 3];
            for (index, symbol) in [
                "phrust_native_bit_and",
                "phrust_native_bit_or",
                "phrust_native_bit_xor",
            ]
            .into_iter()
            .enumerate()
            {
                string_bitwise[index] = declare_native_control_handler(
                    module,
                    needs_string_bitwise[index],
                    symbol,
                    2,
                    || helper_address(symbol),
                )?;
            }
            let mut exact_unary = [None; NATIVE_EXACT_UNARY_COUNT];
            for operation in NATIVE_EXACT_UNARY_OPERATIONS {
                let index = native_exact_unary_index(operation);
                exact_unary[index] = declare_native_control_handler(
                    module,
                    needs_exact_unary[index],
                    native_exact_unary_symbol(operation),
                    1,
                    || helper_address(native_exact_unary_symbol(operation)),
                )?;
            }
            let mut exact_compare = [None; NATIVE_EXACT_COMPARE_COUNT];
            for operation in NATIVE_EXACT_COMPARE_OPERATIONS {
                let index = native_exact_compare_index(operation);
                exact_compare[index] = declare_native_control_handler(
                    module,
                    needs_exact_compare[index],
                    native_exact_compare_symbol(operation),
                    2,
                    || helper_address(native_exact_compare_symbol(operation)),
                )?;
            }
            let echo_bytes = if needs_direct_echo {
                let mut bytes_signature = module.make_signature();
                bytes_signature.params.push(AbiParam::new(pointer_type));
                bytes_signature.params.push(AbiParam::new(types::I64));
                let bytes = declare_native_helper(
                    module,
                    "phrust_native_echo_bytes",
                    &bytes_signature,
                    helper_address("phrust_native_echo_bytes"),
                )?;
                Some(bytes)
            } else {
                None
            };
            let float_to_string = if needs_float_to_string {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::F64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_float_to_string",
                    &signature,
                    helper_address("phrust_native_float_to_string"),
                )?)
            } else {
                None
            };
            let numeric_string = if needs_numeric_string {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_pure_handler(
                    module,
                    "phrust_native_numeric_string",
                    &signature,
                    helper_address("phrust_native_numeric_string"),
                )?)
            } else {
                None
            };
            let fmod_f64 = if needs_fmod_f64 {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::F64));
                signature.params.push(AbiParam::new(types::F64));
                signature.returns.push(AbiParam::new(types::F64));
                Some(declare_native_pure_handler(
                    module,
                    "phrust_native_fmod_f64",
                    &signature,
                    helper_address("phrust_native_fmod_f64"),
                )?)
            } else {
                None
            };
            let round_f64 = if needs_round_f64 {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::F64));
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::F64));
                Some(declare_native_pure_handler(
                    module,
                    "phrust_native_round_f64",
                    &signature,
                    helper_address("phrust_native_round_f64"),
                )?)
            } else {
                None
            };
            let mut pure_math = [None; StablePureMathBuiltin::COUNT];
            for builtin in StablePureMathBuiltin::all() {
                if !needs_exact_pure_math[builtin.index()] {
                    continue;
                }
                let mut signature = module.make_signature();
                for _ in 0..if builtin.accepts_arity(2) { 2 } else { 1 } {
                    signature.params.push(AbiParam::new(types::F64));
                }
                signature.returns.push(AbiParam::new(types::F64));
                pure_math[builtin.index()] = Some(declare_native_pure_handler(
                    module,
                    builtin.symbol(),
                    &signature,
                    helper_address(builtin.symbol()),
                )?);
            }
            let array_cast = declare_native_control_handler(
                module,
                needs_array_cast,
                "phrust_native_array_cast",
                1,
                || helper_address("phrust_native_array_cast"),
            )?;
            let int_cast = declare_native_control_handler(
                module,
                needs_int_cast,
                "phrust_native_int_cast",
                1,
                || helper_address("phrust_native_int_cast"),
            )?;
            let float_cast = declare_native_control_handler(
                module,
                needs_float_cast,
                "phrust_native_float_cast",
                1,
                || helper_address("phrust_native_float_cast"),
            )?;
            let string_cast = declare_native_control_handler(
                module,
                needs_string_cast,
                "phrust_native_string_cast",
                1,
                || helper_address("phrust_native_string_cast"),
            )?;
            let callback_return_string = declare_native_control_handler(
                module,
                needs_callback_return_string,
                "phrust_native_callback_return_string",
                1,
                || helper_address("phrust_native_callback_return_string"),
            )?;
            let object_cast = declare_native_control_handler(
                module,
                needs_object_cast,
                "phrust_native_object_cast",
                1,
                || helper_address("phrust_native_object_cast"),
            )?;
            let object_class_name = if needs_object_class_name {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_object_class_name",
                    &signature,
                    helper_address("phrust_native_object_class_name"),
                )?)
            } else {
                None
            };
            let acquire_callable = declare_native_control_handler(
                module,
                needs_acquire_callable,
                "phrust_native_acquire_callable",
                1,
                || helper_address("phrust_native_acquire_callable"),
            )?;
            let resolve_callable = declare_native_control_handler(
                module,
                needs_resolve_callable,
                "phrust_native_resolve_callable",
                5,
                || helper_address("phrust_native_resolve_callable"),
            )?;
            let prepared_object_new = if needs_prepared_object_new {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_prepared_object_new",
                    &signature,
                    helper_address("phrust_native_prepared_object_new"),
                )?)
            } else {
                None
            };
            let prepared_exception_new = if needs_prepared_exception_new {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_prepared_exception_new",
                    &signature,
                    helper_address("phrust_native_prepared_exception_new"),
                )?)
            } else {
                None
            };
            let prepared_closure_new = if needs_prepared_closure_new {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_prepared_closure_new",
                    &signature,
                    helper_address("phrust_native_prepared_closure_new"),
                )?)
            } else {
                None
            };
            let plain_object_clone = if needs_plain_object_clone {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I64));
                Some(declare_native_helper(
                    module,
                    "phrust_native_plain_object_clone",
                    &signature,
                    helper_address("phrust_native_plain_object_clone"),
                )?)
            } else {
                None
            };
            let dynamic_property_slot = declare_native_control_handler(
                module,
                needs_dynamic_property_slot,
                "phrust_native_dynamic_property_slot",
                2,
                || helper_address("phrust_native_dynamic_property_slot"),
            )?;
            let dynamic_property_test_slot = declare_native_control_handler(
                module,
                needs_dynamic_property_test_slot,
                "phrust_native_dynamic_property_test_slot",
                2,
                || helper_address("phrust_native_dynamic_property_test_slot"),
            )?;
            if needs_baseline_builtin_dispatch {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.builtin_dispatch = Some(declare_native_helper(
                    module,
                    &native_builtin_dispatch_symbol,
                    &signature,
                    helper_address(&native_builtin_dispatch_symbol),
                )?);
            }
            if needs_semantic_dispatch {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(types::I32));
                signature.params.push(AbiParam::new(pointer_type));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.semantic_dispatch = Some(declare_native_helper(
                    module,
                    &baseline_semantic_dispatch_symbol,
                    &signature,
                    helper_address(&baseline_semantic_dispatch_symbol),
                )?);
            }
            if needs_function_resolver {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.function_resolve = Some(declare_native_helper(
                    module,
                    &native_function_resolve_symbol,
                    &signature,
                    helper_address(&native_function_resolve_symbol),
                )?);
            }
            if needs_frame_arena {
                let mut alloc_signature = module.make_signature();
                alloc_signature.params.push(AbiParam::new(types::I64));
                alloc_signature.params.push(AbiParam::new(types::I64));
                alloc_signature.params.push(AbiParam::new(types::I64));
                alloc_signature.returns.push(AbiParam::new(pointer_type));
                native_operations.frame_alloc = Some(declare_native_helper(
                    module,
                    "phrust_native_frame_alloc",
                    &alloc_signature,
                    helper_address("phrust_native_frame_alloc"),
                )?);
                let mut release_signature = module.make_signature();
                release_signature.params.push(AbiParam::new(types::I64));
                release_signature.params.push(AbiParam::new(pointer_type));
                release_signature.returns.push(AbiParam::new(types::I32));
                native_operations.frame_release = Some(declare_native_helper(
                    module,
                    "phrust_native_frame_release",
                    &release_signature,
                    helper_address("phrust_native_frame_release"),
                )?);
            }
            if needs_unary {
                native_operations.unary = Some(declare_baseline_value_operation(
                    module,
                    "phrust_baseline_native_unary",
                    1,
                    helper_address("phrust_baseline_native_unary"),
                )?);
            }
            if needs_baseline_binary {
                native_operations.baseline_binary = Some(declare_baseline_value_operation(
                    module,
                    "phrust_baseline_native_binary",
                    4,
                    helper_address("phrust_baseline_native_binary"),
                )?);
            }
            if needs_compare {
                native_operations.compare = Some(declare_baseline_value_operation(
                    module,
                    "phrust_baseline_native_compare",
                    2,
                    helper_address("phrust_baseline_native_compare"),
                )?);
            }
            if needs_cast {
                native_operations.cast = Some(declare_baseline_value_operation(
                    module,
                    "phrust_baseline_native_cast",
                    1,
                    helper_address("phrust_baseline_native_cast"),
                )?);
            }
            if needs_echo {
                let mut signature = module.make_signature();
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
                native_operations.local_fetch = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_local_fetch",
                    5,
                    helper_address("phrust_native_local_fetch"),
                )?);
            }
            if needs_local_store {
                native_operations.local_store = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_local_store",
                    4,
                    helper_address("phrust_native_local_store"),
                )?);
            }
            if needs_value_release {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.value_release = Some(declare_native_helper(
                    module,
                    "phrust_native_value_release",
                    &signature,
                    helper_address("phrust_native_value_release"),
                )?);
            }
            if needs_reference_bind {
                native_operations.reference_bind = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_reference_bind",
                    3,
                    helper_address("phrust_native_reference_bind"),
                )?);
            }
            if needs_argument_check {
                native_operations.argument_check = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_argument_check",
                    5,
                    helper_address("phrust_native_argument_check"),
                )?);
            }
            if needs_return_check {
                native_operations.return_check = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_return_check",
                    2,
                    helper_address("phrust_native_return_check"),
                )?);
            }
            if needs_exception_new {
                native_operations.exception_new = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_exception_new",
                    3,
                    helper_address("phrust_native_exception_new"),
                )?);
            }
            if needs_array_new {
                native_operations.array_new = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_new",
                    0,
                    helper_address("phrust_native_array_new"),
                )?);
            }
            if needs_object_new {
                native_operations.object_new = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_object_new",
                    0,
                    helper_address("phrust_native_object_new"),
                )?);
            }
            if needs_property_fetch {
                native_operations.property_fetch = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_property_fetch",
                    3,
                    helper_address("phrust_native_property_fetch"),
                )?);
            }
            if needs_property_assign {
                native_operations.property_assign = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_property_assign",
                    4,
                    helper_address("phrust_native_property_assign"),
                )?);
            }
            if needs_object_clone {
                native_operations.object_clone = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_object_clone",
                    1,
                    helper_address("phrust_native_object_clone"),
                )?);
            }
            if needs_object_clone_with {
                native_operations.object_clone_with = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_object_clone_with",
                    2,
                    helper_address("phrust_native_object_clone_with"),
                )?);
            }
            if needs_array_insert {
                native_operations.array_insert = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_insert",
                    3,
                    helper_address("phrust_native_array_insert"),
                )?);
                native_operations.array_insert_local = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_insert_local",
                    3,
                    helper_address("phrust_native_array_insert_local"),
                )?);
            }
            if needs_array_fetch {
                native_operations.array_fetch = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_fetch",
                    2,
                    helper_address("phrust_native_array_fetch"),
                )?);
            }
            if needs_array_unset {
                native_operations.array_unset = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_unset",
                    2,
                    helper_address("phrust_native_array_unset"),
                )?);
            }
            if needs_array_spread {
                native_operations.array_spread = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_array_spread",
                    2,
                    helper_address("phrust_native_array_spread"),
                )?);
            }
            if needs_foreach_init {
                native_operations.foreach_init = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_foreach_init",
                    3,
                    helper_address("phrust_native_foreach_init"),
                )?);
            }
            if needs_foreach_next {
                let mut signature = module.make_signature();
                signature.params.push(AbiParam::new(types::I64));
                signature.params.push(AbiParam::new(pointer_type));
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
                signature.returns.push(AbiParam::new(types::I32));
                native_operations.foreach_cleanup = Some(declare_native_helper(
                    module,
                    "phrust_native_foreach_cleanup",
                    &signature,
                    helper_address("phrust_native_foreach_cleanup"),
                )?);
            }
            if needs_constant_fetch {
                native_operations.constant_fetch = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_constant_fetch",
                    2,
                    helper_address("phrust_native_constant_fetch"),
                )?);
            }
            if needs_truthy {
                let mut signature = module.make_signature();
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
            if needs_type_predicate {
                native_operations.type_predicate = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_type_predicate",
                    1,
                    helper_address("phrust_native_type_predicate"),
                )?);
            }
            if needs_stable_length {
                native_operations.stable_length = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_stable_length",
                    3,
                    helper_address("phrust_native_stable_length"),
                )?);
            }
            if needs_string_predicate {
                native_operations.string_predicate = Some(declare_baseline_value_operation(
                    module,
                    "phrust_native_string_predicate",
                    2,
                    helper_address("phrust_native_string_predicate"),
                )?);
            }
            if needs_runtime_fatal {
                let mut signature = module.make_signature();
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
            let synthetic_base = u32::try_from(unit.functions.len()).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                    "source unit function count does not fit the fragment symbol space",
                )
            })?;
            let mut next_synthetic = synthetic_base;
            let tier_operations = if baseline_helper_imports {
                let value_release_commit_symbol = FunctionId::new(next_synthetic);
                next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                        "native baseline value-release symbol id overflowed",
                    )
                })?;
                let symbol = format!("{name}.native.value_release_commit");
                let signature = direct_value_release_signature(module);
                let value_release_commit = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare {symbol}: {error}"),
                        )
                    })?;
                functions.insert(value_release_commit_symbol, value_release_commit);
                NativeTierOperations::Baseline {
                    call: native_call_helper,
                    dynamic_code: native_dynamic_code_helper,
                    operations: native_operations,
                    value_release_commit,
                    value_release_commit_symbol,
                }
            } else {
                let array_ensure_unique_symbol = FunctionId::new(next_synthetic);
                next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                        "native optimizing-operation symbol id overflowed",
                    )
                })?;
                let symbol = format!("{name}.native.array_ensure_unique");
                let signature = direct_array_ensure_unique_signature(module);
                let array_ensure_unique = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare {symbol}: {error}"),
                        )
                    })?;
                functions.insert(array_ensure_unique_symbol, array_ensure_unique);
                let array_child_entry_symbol = FunctionId::new(next_synthetic);
                next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                        "native optimizing-operation symbol id overflowed",
                    )
                })?;
                let symbol = format!("{name}.native.array_child_entry");
                let signature = direct_array_child_entry_signature(module);
                let array_child_entry = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare {symbol}: {error}"),
                        )
                    })?;
                functions.insert(array_child_entry_symbol, array_child_entry);
                let value_release_commit_symbol = FunctionId::new(next_synthetic);
                next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                        "native value-release commit symbol id overflowed",
                    )
                })?;
                let symbol = format!("{name}.native.value_release_commit");
                let signature = direct_value_release_signature(module);
                let value_release_commit = module
                    .declare_function(&symbol, Linkage::Local, &signature)
                    .map_err(|error| {
                        CraneliftLoweringError::new(
                            "JIT_CRANELIFT_REJECT_DECLARE",
                            format!("failed to declare {symbol}: {error}"),
                        )
                    })?;
                functions.insert(value_release_commit_symbol, value_release_commit);
                NativeTierOperations::Optimizing {
                    operations: NativeOptimizingOperations {
                        execution_poll: native_operations.execution_poll,
                        array_union,
                        concat,
                        string_bitwise,
                        exact_unary,
                        exact_compare,
                        echo_bytes,
                        float_to_string,
                        numeric_string,
                        fmod_f64,
                        round_f64,
                        pure_math,
                        array_cast,
                        int_cast,
                        float_cast,
                        string_cast,
                        callback_return_string,
                        object_cast,
                        object_class_name,
                        acquire_callable,
                        resolve_callable,
                        prepared_object_new,
                        prepared_exception_new,
                        prepared_closure_new,
                        plain_object_clone,
                        dynamic_property_slot,
                        dynamic_property_test_slot,
                        exact_symbol_query,
                        exact_pcre,
                        preg_callback_plan,
                        preg_callback_assemble,
                        exact_json,
                        exact_format,
                        exact_hash,
                        exact_byte_codec,
                        exact_string_search_compare,
                        exact_string_rewrite,
                        exact_html_codec,
                        exact_url_query,
                        exact_array_aggregate,
                        exact_recursive_array,
                        exact_array_sort,
                        exact_array_multisort,
                        exact_object_identity,
                        exact_callable_query,
                        exact_callback_handler,
                        exact_autoload_callback,
                        exact_shutdown_callback,
                        exact_serialization,
                        exact_tokenizer,
                        exact_mbstring,
                        exact_bcmath,
                        exact_filter,
                        exact_session,
                        exact_object_vars,
                        exact_class_metadata,
                        exact_class_lineage,
                        exact_extension_query,
                        exact_memory_query,
                        exact_gc,
                        exact_resource_query,
                        exact_error_state,
                        exact_settype,
                        exact_configuration,
                        exact_http_response,
                        exact_cookie,
                        exact_clock,
                        exact_date,
                        exact_random,
                        exact_request_query,
                        exact_declaration_inventory,
                        exact_constant_inventory,
                        exact_compact,
                        exact_frame_introspection,
                        exact_base_conversion,
                        exact_intval_base,
                        exact_network_address,
                        exact_compression_codec,
                        exact_path,
                        exact_output_buffer,
                        array_ensure_unique,
                        array_ensure_unique_symbol,
                        array_child_entry,
                        array_child_entry_symbol,
                        value_release_commit,
                        value_release_commit_symbol,
                    },
                }
            };
            let (mut fragment_functions, mut fragment_symbols) =
                declare_fragment_functions(
                    module,
                    name,
                    region,
                    active_fragment_layout.as_ref(),
                    0,
                    &mut next_synthetic,
                    &mut functions,
                )?;
            let inline_constants = collect_bounded_inline_values(unit, &regions);
            let tail_forwards = regions
                .values()
                .flat_map(|candidate| {
                    candidate.blocks.iter().filter_map(|block| {
                        let (continuation, target) =
                            bounded_tail_forward_target(candidate, block, &regions)?;
                        (!trampoline_functions.contains(&target))
                            .then_some(((candidate.function, continuation), target))
                    })
                })
                .collect::<BTreeMap<_, _>>();

            let mut code_bytes = 0_u64;
            let mut clif_blocks = 0_usize;
            let mut maximum_pre_regalloc = PreRegallocMetrics::default();
            let mut maximum_temporary_cache_entries = 0_usize;
            let mut native_pc_ranges = Vec::new();
            let mut relocatable_bytes = Vec::new();
            let mut relocatable_functions = Vec::new();
            let mut relocatable_relocations = Vec::new();
            let mut emitted_production_lowering = Vec::new();
            let mut function_code_metrics = BTreeMap::new();
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
                    let native_arity =
                        crate::region_ir::native_function_parameter_locals(unit, function_id)?
                            .len();
                    Some((
                        function_id,
                        NativeFunctionMetadata {
                            name: function.name.clone(),
                            params: function.params.clone(),
                            requires_trampoline: ir_function_requires_trampoline(function),
                            native_arity,
                            reference_only_trampoline: (function
                                .params
                                .iter()
                                .any(|parameter| parameter.by_ref)
                                || function.returns_by_ref)
                                && !ir_function_requires_non_reference_trampoline(function),
                            returns_by_reference: function.returns_by_ref,
                            return_type: function.return_type.clone(),
                            has_exception_handlers: ir_function_has_exception_handler(function),
                        },
                    ))
                })
                .collect::<BTreeMap<_, _>>();
            function_params.extend(regions.iter().map(|(function, region)| {
                let ir_function = &unit.functions[function.index()];
                (
                    *function,
                    NativeFunctionMetadata {
                        name: ir_function.name.clone(),
                        params: region.params.clone(),
                        requires_trampoline: trampoline_functions.contains(function),
                        native_arity: region.arity(),
                        reference_only_trampoline: (ir_function
                            .params
                            .iter()
                            .any(|parameter| parameter.by_ref)
                            || ir_function.returns_by_ref)
                            && !ir_function_requires_non_reference_trampoline(ir_function),
                        returns_by_reference: ir_function.returns_by_ref,
                        return_type: ir_function.return_type.clone(),
                        has_exception_handlers: !region.exception_regions.is_empty(),
                    },
                )
            }));
            let mut preflighted_whole = None;
            let mut preflighted_fragments = BTreeMap::<u32, DefinedRegionFunction>::new();
            // A planner-admitted whole optimizing function still needs exact
            // CLIF preflight. Direct calls, ownership, and guards can expand
            // one Region instruction into enough backend state to exceed the
            // whole-function ceiling even when the source estimate is
            // bounded. Keep the ordinary whole-function representation when
            // its exact form fits; otherwise enter the same deterministic
            // fragment refinement used below.
            if active_fragment_layout.is_none() {
                let register_liveness = NativeRegisterLiveness::analyze(region);
                let compiler =
                    crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
                        region.compile_metadata.tier,
                    );
                let preflight = compiler.compile_fragment(&mut |mode| {
                    define_region_graph_function(
                        module,
                        codegen_context,
                        builder_context,
                        region,
                        &unit.constants,
                        &value_flows[&region.function],
                        functions[&region.function],
                        &functions,
                        &inline_constants,
                        &tail_forwards,
                        &function_params,
                        &request.external_function_signatures,
                        tier_operations,
                        &register_liveness,
                        None,
                        runtime_unit_identity,
                        mode,
                        false,
                        true,
                    )
                });
                match preflight {
                    Ok(defined)
                        if defined
                            .pre_regalloc
                            .exceeds_replan_margin(region.compile_metadata.tier) =>
                    {
                        active_plan = NativeCompilePlan::for_bounded_fragments(region);
                        active_fragment_layout =
                            Some(NativeFunctionFragmentLayout::for_plan(region, &active_plan)?);
                        compiled_pre_regalloc_replans
                            .set(compiled_pre_regalloc_replans.get().saturating_add(1));
                        (fragment_functions, fragment_symbols) = declare_fragment_functions(
                            module,
                            name,
                            region,
                            active_fragment_layout.as_ref(),
                            0,
                            &mut next_synthetic,
                            &mut functions,
                        )?;
                    }
                    Ok(defined) => preflighted_whole = Some(defined),
                    Err(error) if error.code == "JIT_CRANELIFT_PRE_REGALLOC_BUDGET" => {
                        active_plan = NativeCompilePlan::for_bounded_fragments(region);
                        active_fragment_layout =
                            Some(NativeFunctionFragmentLayout::for_plan(region, &active_plan)?);
                        compiled_pre_regalloc_replans
                            .set(compiled_pre_regalloc_replans.get().saturating_add(1));
                        (fragment_functions, fragment_symbols) = declare_fragment_functions(
                            module,
                            name,
                            region,
                            active_fragment_layout.as_ref(),
                            0,
                            &mut next_synthetic,
                            &mut functions,
                        )?;
                    }
                    Err(error) => return Err(error),
                }
            }
            // Fragmented optimizing functions and streaming baseline
            // functions use exact preflight for every fragment. The cheap
            // planner estimate intentionally cannot account for the full
            // live-state fanout of direct guards; without this pass, one
            // underestimated fragment rejects the complete artifact only
            // after all preceding fragments have already been compiled.
            if active_fragment_layout.is_some() {
                for replan_attempt in 0..=MAX_PRE_REGALLOC_REPLAN_ATTEMPTS {
                    let mut offending_fragments = Vec::new();
                    let mut round_preflighted = BTreeMap::new();
                    if let Some(layout) = active_fragment_layout.as_ref() {
                        for fragment in &layout.fragments {
                            let compiler = crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
                                region.compile_metadata.tier,
                            );
                            let preflight = compiler.compile_fragment(&mut |mode| {
                                let func_id = if layout.fragments.len() == 1 {
                                    functions[&region.function]
                                } else {
                                    fragment_functions[&fragment.id]
                                };
                                define_region_graph_function(
                                    module,
                                    codegen_context,
                                    builder_context,
                                    region,
                                    &unit.constants,
                                    &value_flows[&region.function],
                                    func_id,
                                    &functions,
                                    &inline_constants,
                                    &tail_forwards,
                                    &function_params,
                                    &request.external_function_signatures,
                                    tier_operations,
                                    &layout.register_liveness,
                                    Some(NativeFragmentDefinition {
                                        layout,
                                        fragment,
                                        functions: &fragment_functions,
                                    }),
                                    runtime_unit_identity,
                                    mode,
                                    layout.fragments.len() == 1,
                                    true,
                                )
                            });
                            match preflight {
                                Ok(defined)
                                    if defined
                                        .pre_regalloc
                                        .exceeds_replan_margin(region.compile_metadata.tier) =>
                                {
                                    offending_fragments.push((
                                        fragment.id,
                                        defined
                                            .pre_regalloc
                                            .minimum_fragment_count(region.compile_metadata.tier),
                                    ));
                                }
                                Ok(defined) => {
                                    round_preflighted.insert(fragment.id, defined);
                                }
                                Err(error) if error.code == "JIT_CRANELIFT_PRE_REGALLOC_BUDGET" => {
                                    // A hard-limit rejection does not expose
                                    // trustworthy metrics. Bisect it and let
                                    // the next exact preflight size both
                                    // children before any regalloc work.
                                    offending_fragments.push((fragment.id, 2));
                                }
                                Err(error) => return Err(error),
                            }
                        }
                    }
                    if offending_fragments.is_empty() {
                        preflighted_fragments = round_preflighted;
                        break;
                    }
                    if replan_attempt == MAX_PRE_REGALLOC_REPLAN_ATTEMPTS {
                        return Err(CraneliftLoweringError::new(
                            "JIT_CRANELIFT_PRE_REGALLOC_REPLAN_LIMIT",
                            format!(
                                "fragments {offending_fragments:?} still exceed the exact pre-regalloc safety margin after {MAX_PRE_REGALLOC_REPLAN_ATTEMPTS} deterministic replan rounds"
                            ),
                        ));
                    }
                    // Refine every exact offender in the same deterministic
                    // round. Splitting only the first offender made the global
                    // attempt limit depend on how many independently large
                    // fragments a function happened to contain. Descending IDs
                    // keep lower fragment IDs stable while each split
                    // re-enumerates the plan.
                    offending_fragments.sort_unstable_by_key(|(fragment_id, _)| *fragment_id);
                    offending_fragments.dedup_by_key(|(fragment_id, _)| *fragment_id);
                    for (fragment_id, pieces) in offending_fragments.into_iter().rev() {
                        let block_shape = active_plan
                            .fragments
                            .iter()
                            .find(|fragment| fragment.id == fragment_id)
                            .map(|fragment| {
                                fragment
                                    .blocks
                                    .iter()
                                    .map(|block| {
                                        let region_block = &region.blocks[block.index()];
                                        let instructions = region_block
                                            .instructions
                                            .iter()
                                            .map(|instruction| {
                                                let manifest =
                                                    crate::region_ir::baseline_instruction_lowering(
                                                        &instruction.source_kind,
                                                    );
                                                format!(
                                                    "{}(uses={},live={})",
                                                    manifest.variant,
                                                    instruction.register_uses().len(),
                                                    instruction.live_locals.len(),
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                            .join("+");
                                        format!(
                                            "{}(source={}):instructions={}:{}:entry-live={}:terminator={}:terminator-live={}:terminator-registers={}",
                                            block.raw(),
                                            region_block.source_block.raw(),
                                            region_block.instructions.len(),
                                            instructions,
                                            region_block.entry_live_locals.len(),
                                            crate::region_ir::baseline_terminator_lowering(
                                                &region_block.source_terminator,
                                            )
                                            .variant,
                                            region_block.terminator_live_locals.len(),
                                            region_block
                                                .terminator_live_registers
                                                .as_ref()
                                                .map_or(0, Vec::len),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join(",")
                            })
                            .unwrap_or_default();
                        active_plan = active_plan.refine_fragment_into(region, fragment_id, pieces).ok_or_else(|| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_PRE_REGALLOC_UNSPLITTABLE",
                                format!(
                                    "function {} fragment {fragment_id} exceeds the exact pre-regalloc safety margin and contains no safe Region-block cut (block:instruction-count={block_shape})",
                                    region.function_name,
                                ),
                            )
                        })?;
                    }
                    compiled_pre_regalloc_replans
                        .set(compiled_pre_regalloc_replans.get().saturating_add(1));
                    active_fragment_layout =
                        Some(NativeFunctionFragmentLayout::for_plan(region, &active_plan)?);
                    (fragment_functions, fragment_symbols) = declare_fragment_functions(
                        module,
                        name,
                        region,
                        active_fragment_layout.as_ref(),
                        replan_attempt + 1,
                        &mut next_synthetic,
                        &mut functions,
                    )?;
                }
            }
            {
                let referenced_internal_functions = std::cell::RefCell::new(BTreeSet::new());
                let mut append_defined = |symbol: FunctionId,
                                      arity: u8,
                                      local_count: u32,
                                      mut defined: DefinedRegionFunction|
             -> Result<(u64, u32), CraneliftLoweringError> {
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
                clif_blocks = clif_blocks.saturating_add(defined.clif_blocks);
                maximum_pre_regalloc.max_assign(defined.pre_regalloc);
                maximum_temporary_cache_entries = maximum_temporary_cache_entries
                    .max(defined.maximum_temporary_cache_entries);
                relocatable_bytes.extend_from_slice(&defined.code);
                for relocation in &mut defined.relocations {
                    if let crate::JitRelocatableTarget::InternalFunction(function) =
                        &relocation.target
                    {
                        referenced_internal_functions
                            .borrow_mut()
                            .insert(*function);
                    }
                    relocation.offset = relocation.offset.saturating_add(code_offset);
                }
                relocatable_relocations.append(&mut defined.relocations);
                emitted_production_lowering.append(&mut defined.production_lowering);
                relocatable_functions.push(crate::JitRelocatableFunction {
                    function: symbol,
                    code_offset,
                    code_len: candidate_bytes,
                    arity,
                    local_count,
                });
                code_bytes = code_bytes.saturating_add(candidate_bytes);
                native_pc_ranges.append(&mut defined.native_pc_ranges);
                Ok((candidate_bytes, defined.native_stack_bytes))
            };
            // A compile group may contain many bounded native fragments. Reuse
            // Cranelift's allocation-heavy translation scratch sequentially;
            // `clear_context` preserves its backing allocations after every
            // fragment while regalloc still sees only one fragment at a time.
            for candidate in regions.values() {
                if let Some(layout) = &active_fragment_layout {
                    let mut function_bytes = 0_u64;
                    let mut maximum_stack = 0_u32;
                    if layout.fragments.len() == 1 {
                        let fragment = &layout.fragments[0];
                        let defined = if let Some(preflighted) =
                            preflighted_fragments.remove(&fragment.id)
                        {
                            compile_preflighted_region_function(
                                module,
                                codegen_context,
                                functions[&candidate.function],
                                candidate,
                                &functions,
                                preflighted,
                            )?
                        } else {
                            let compiler =
                                crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
                                    candidate.compile_metadata.tier,
                                );
                            compiler.compile_fragment(&mut |compilation_mode| {
                                define_region_graph_function(
                                    module,
                                    codegen_context,
                                    builder_context,
                                    candidate,
                                    &unit.constants,
                                    &value_flows[&candidate.function],
                                    functions[&candidate.function],
                                    &functions,
                                    &inline_constants,
                                    &tail_forwards,
                                    &function_params,
                                    &request.external_function_signatures,
                                    tier_operations,
                                    &layout.register_liveness,
                                    Some(NativeFragmentDefinition {
                                        layout,
                                        fragment,
                                        functions: &fragment_functions,
                                    }),
                                    runtime_unit_identity,
                                    compilation_mode,
                                    true,
                                    false,
                                )
                            })?
                        };
                        let metrics = append_defined(
                            candidate.function,
                            region_arity(candidate)?,
                            candidate.local_count,
                            defined,
                        )?;
                        function_code_metrics.insert(candidate.function, metrics);
                        continue;
                    }
                    for fragment in &layout.fragments {
                        let defined = if let Some(preflighted) =
                            preflighted_fragments.remove(&fragment.id)
                        {
                            compile_preflighted_region_function(
                                module,
                                codegen_context,
                                fragment_functions[&fragment.id],
                                candidate,
                                &functions,
                                preflighted,
                            )?
                        } else {
                            let compiler =
                                crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
                                    candidate.compile_metadata.tier,
                                );
                            compiler.compile_fragment(&mut |compilation_mode| {
                                define_region_graph_function(
                                    module,
                                    codegen_context,
                                    builder_context,
                                    candidate,
                                    &unit.constants,
                                    &value_flows[&candidate.function],
                                    fragment_functions[&fragment.id],
                                    &functions,
                                    &inline_constants,
                                    &tail_forwards,
                                    &function_params,
                                    &request.external_function_signatures,
                                    tier_operations,
                                    &layout.register_liveness,
                                    Some(NativeFragmentDefinition {
                                        layout,
                                        fragment,
                                        functions: &fragment_functions,
                                    }),
                                    runtime_unit_identity,
                                    compilation_mode,
                                    false,
                                    false,
                                )
                            })?
                        };
                        let (bytes, stack) = append_defined(
                            fragment_symbols[&fragment.id],
                            0,
                            candidate.local_count,
                            defined,
                        )?;
                        function_bytes = function_bytes.saturating_add(bytes);
                        maximum_stack = maximum_stack.max(stack);
                    }
                    let wrapper = define_region_fragment_wrapper(
                        module,
                        codegen_context,
                        builder_context,
                        candidate,
                        functions[&candidate.function],
                        &fragment_functions,
                        layout,
                        &functions,
                        &value_flows[&candidate.function],
                        tier_operations,
                    )?;
                    let (bytes, stack) = append_defined(
                        candidate.function,
                        region_arity(candidate)?,
                        candidate.local_count,
                        wrapper,
                    )?;
                    function_bytes = function_bytes.saturating_add(bytes);
                    maximum_stack = maximum_stack.max(stack);
                    function_code_metrics
                        .insert(candidate.function, (function_bytes, maximum_stack));
                } else {
                    let register_liveness = NativeRegisterLiveness::analyze(candidate);
                    let defined = if let Some(preflighted) = preflighted_whole.take() {
                        compile_preflighted_region_function(
                            module,
                            codegen_context,
                            functions[&candidate.function],
                            candidate,
                            &functions,
                            preflighted,
                        )
                    } else {
                        let compiler =
                            crate::cranelift_lowering::baseline_streaming::compiler_for_tier(
                                candidate.compile_metadata.tier,
                            );
                        compiler.compile_fragment(&mut |compilation_mode| {
                            define_region_graph_function(
                                module,
                                codegen_context,
                                builder_context,
                                candidate,
                                &unit.constants,
                                &value_flows[&candidate.function],
                                functions[&candidate.function],
                                &functions,
                                &inline_constants,
                                &tail_forwards,
                                &function_params,
                                &request.external_function_signatures,
                                tier_operations,
                                &register_liveness,
                                None,
                                runtime_unit_identity,
                                compilation_mode,
                                false,
                                false,
                            )
                        })
                    }?;
                    let metrics = append_defined(
                        candidate.function,
                        region_arity(candidate)?,
                        candidate.local_count,
                        defined,
                    )?;
                    function_code_metrics.insert(candidate.function, metrics);
                }
            }
            let referenced = referenced_internal_functions.borrow().clone();
            match tier_operations {
                NativeTierOperations::Optimizing { operations } => {
                // Optimizing support functions are part of an artifact only
                // when its emitted CLIF actually relocates to them. The old
                // unconditional bundle compiled and published all five
                // bodies even for pure scalar functions. Keep the exact
                // native dependency closure for the emitted direct paths.
                let needs_ensure =
                    referenced.contains(&operations.array_ensure_unique_symbol);
                let needs_child =
                    referenced.contains(&operations.array_child_entry_symbol);
                let needs_commit =
                    referenced.contains(&operations.value_release_commit_symbol);

                if needs_ensure {
                    let defined = define_direct_array_ensure_unique_function(
                        module,
                        codegen_context,
                        builder_context,
                        operations.array_ensure_unique,
                    )?;
                    let _ = append_defined(
                        operations.array_ensure_unique_symbol,
                        0,
                        0,
                        defined,
                    )?;
                }
                if needs_child {
                    let defined = define_direct_array_child_entry_function(
                        module,
                        codegen_context,
                        builder_context,
                        operations.array_child_entry,
                    )?;
                    let _ = append_defined(
                        operations.array_child_entry_symbol,
                        0,
                        0,
                        defined,
                    )?;
                }
                if needs_commit {
                    let defined = define_direct_value_release_commit_function(
                        module,
                        codegen_context,
                        builder_context,
                        operations.value_release_commit,
                        operations.value_release_commit_symbol,
                    )?;
                    let _ = append_defined(
                        operations.value_release_commit_symbol,
                        0,
                        0,
                        defined,
                    )?;
                }
                },
                NativeTierOperations::Baseline {
                    value_release_commit,
                    value_release_commit_symbol,
                    ..
                } if referenced.contains(&value_release_commit_symbol) => {
                    let defined = define_direct_value_release_commit_function(
                        module,
                        codegen_context,
                        builder_context,
                        value_release_commit,
                        value_release_commit_symbol,
                    )?;
                    let _ = append_defined(value_release_commit_symbol, 0, 0, defined)?;
                },
                NativeTierOperations::Baseline { .. } => {},
            }
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
                    let (function_code_bytes, native_stack_bytes) =
                        function_code_metrics[&candidate.function];
                    Ok(crate::JitNativeFunctionEntryMetadata {
                        function: candidate.function,
                        address: module.get_finalized_function(functions[&candidate.function])
                            as usize,
                        arity: region_arity(candidate)?,
                        code_bytes: function_code_bytes,
                        native_stack_bytes,
                        local_count: candidate.local_count,
                        direct_call_sites: candidate
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .filter(|instruction| {
                                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call)
                                if call.direct_compiled_target().is_some_and(|target| {
                                    (regions.contains_key(&target) || needs_function_resolver)
                                        && function_params.get(&target).is_some_and(
                                            |metadata| {
                                                metadata.native_arity == call.operands.len()
                                                    && (!metadata.requires_trampoline
                                                        || (metadata.reference_only_trampoline
                                                            && metadata.params.iter().enumerate().all(
                                                                |(index, parameter)| {
                                                                        !parameter.by_ref
                                                                        || call.args.get(index).is_some_and(
                                                                            |argument| {
                                                                                argument.by_ref_local.is_some()
                                                                            },
                                                                        )
                                                                },
                                                            )))
                                            },
                                        )
                                        && !matches!(
                                            call.result,
                                            RegionCallResult::ReferenceLocal(_)
                                        )
                                        && call.args.iter().all(|argument| {
                                            argument.name.is_none() && !argument.unpack
                                        })
                                        && !(call.operands.is_empty()
                                            && inline_constants.contains_key(&target))
                                }))
                            })
                            .count() as u64,
                        direct_method_call_sites: candidate
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .filter(|instruction| {
                                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call)
                                if call.argument_operand_offset == 1
                                    && call.direct_compiled_target().is_some_and(|target| {
                                        (regions.contains_key(&target) || needs_function_resolver)
                                            && function_params
                                                .get(&target)
                                                .is_some_and(|metadata| {
                                                    !metadata.requires_trampoline
                                                })
                                            && !matches!(
                                                call.result,
                                                RegionCallResult::ReferenceLocal(_)
                                            )
                                            && call.args.iter().all(|argument| {
                                                argument.name.is_none() && !argument.unpack
                                            })
                                    }))
                            })
                            .count() as u64,
                        inlined_call_sites: candidate
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .filter(|instruction| {
                                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call)
                                if call.direct_compiled_target().is_some_and(|target| {
                                    inline_constants
                                        .get(&target)
                                        .copied()
                                        .and_then(|value| bounded_inline_call_operand(call, value))
                                        .is_some()
                                }))
                            })
                            .count() as u64,
                        inline_bytes_added: candidate
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .filter(|instruction| {
                                matches!(&instruction.kind, RegionInstructionKind::NativeCall(call)
                                if call.direct_compiled_target().is_some_and(|target| {
                                    inline_constants
                                        .get(&target)
                                        .copied()
                                        .and_then(|value| bounded_inline_call_operand(call, value))
                                        .is_some()
                                }))
                            })
                            .count() as u64
                            * 8,
                        tail_call_sites: tail_forwards
                            .keys()
                            .filter(|(function, _)| *function == candidate.function)
                            .count() as u64,
                        inline_rejected_by_reason: inline_rejection_counts(candidate, &regions),
                    })
                })
                .collect::<Result<Vec<_>, CraneliftLoweringError>>()?;
            let root = functions[&function];
            let address = module.get_finalized_function(root) as usize;
            let region_state_metadata = region_graph_metadata(
                function,
                region.local_count,
                regions.values(),
                native_pc_ranges,
                function_entries,
                active_fragment_layout
                    .as_ref()
                    .map(|layout| &layout.register_liveness),
                &value_flows,
                emitted_production_lowering,
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
            if compilation_mode
                == crate::cranelift_lowering::baseline_streaming::NativeCompilationMode::SsaOptimizing
            {
                let forbidden = relocatable_relocations.iter().find_map(|relocation| {
                    match &relocation.target {
                        crate::JitRelocatableTarget::Helper(symbol)
                            if symbol.starts_with("phrust_baseline_") =>
                        {
                            Some(symbol.as_str())
                        }
                        crate::JitRelocatableTarget::Helper(symbol)
                            if matches!(
                                symbol.as_str(),
                                "phrust_native_define"
                                    | "phrust_native_defined"
                                    | "phrust_native_constant"
                                    | "phrust_native_echo_bytes"
                                    | "phrust_native_float_to_string"
                                    | "phrust_native_numeric_string"
                                    | "phrust_native_fmod_f64"
                                    | "phrust_native_round_f64"
                                    | "phrust_native_array_cast"
                                    | "phrust_native_int_cast"
                                    | "phrust_native_float_cast"
                                    | "phrust_native_string_cast"
                                    | "phrust_native_callback_return_string"
                                    | "phrust_native_array_union"
                                    | "phrust_native_concat"
                                    | "phrust_native_bit_and"
                                    | "phrust_native_bit_or"
                                    | "phrust_native_bit_xor"
                                    | "phrust_native_unary_plus"
                                    | "phrust_native_unary_minus"
                                    | "phrust_native_bit_not"
                                    | "phrust_native_equal"
                                    | "phrust_native_not_equal"
                                    | "phrust_native_identical"
                                    | "phrust_native_not_identical"
                                    | "phrust_native_less"
                                    | "phrust_native_less_equal"
                                    | "phrust_native_greater"
                                    | "phrust_native_greater_equal"
                                    | "phrust_native_spaceship"
                                    | "phrust_native_object_cast"
                                    | "phrust_native_object_class_name"
                                    | "phrust_native_acquire_callable"
                                    | "phrust_native_is_callable"
                                    | "phrust_native_resolve_callable"
                                    | "phrust_native_prepared_object_new"
                                    | "phrust_native_prepared_exception_new"
                                    | "phrust_native_prepared_closure_new"
                                    | "phrust_native_plain_object_clone"
                                    | "phrust_native_dynamic_property_slot"
                                    | "phrust_native_dynamic_property_test_slot"
                                    | "phrust_native_function_exists"
                                    | "phrust_native_class_exists"
                                    | "phrust_native_interface_exists"
                                    | "phrust_native_trait_exists"
                                    | "phrust_native_enum_exists"
                                    | "phrust_native_method_exists"
                                    | "phrust_native_property_exists"
                                    | "phrust_native_execution_poll"
                            )
                                || symbol.starts_with("phrust_native_preg_")
                                || symbol.starts_with("phrust_native_json_")
                                || StablePureMathBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableBaseConversionBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || symbol == "phrust_native_intval_base"
                                || StableNetworkAddressBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableCompressionCodecBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableStringSearchCompareBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableStringRewriteBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableHtmlCodecBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableUrlQueryBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableArrayAggregateBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableRecursiveArrayBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableArraySortBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || symbol == "phrust_native_array_multisort"
                                || StableObjectIdentityBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableCallbackHandlerBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableAutoloadCallbackBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || symbol == "phrust_native_register_shutdown_function"
                                || StableSerializationBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableTokenizerBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableMbstringBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableBcmathBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableFilterBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableSessionBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableObjectVarsBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableClassMetadataBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableClassLineageBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableExtensionQueryBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableMemoryQueryBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableGcBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableResourceQueryBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableErrorStateBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || symbol == "phrust_native_settype"
                                || StableConfigurationBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableHttpResponseBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableCookieBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableClockBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableDateBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableRandomBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableRequestQueryBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || StableDeclarationInventoryBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || symbol == "phrust_native_get_defined_constants"
                                || symbol == "phrust_native_compact"
                                || StableFrameIntrospectionBuiltin::all()
                                    .iter()
                                    .any(|builtin| builtin.symbol() == symbol)
                                || matches!(
                                    symbol.as_str(),
                                    "phrust_native_sprintf"
                                        | "phrust_native_printf"
                                        | "phrust_native_vsprintf"
                                        | "phrust_native_vprintf"
                                        | "phrust_native_number_format"
                                        | "phrust_native_md5"
                                        | "phrust_native_sha1"
                                        | "phrust_native_crc32"
                                        | "phrust_native_hash"
                                        | "phrust_native_hash_hmac"
                                        | "phrust_native_hash_equals"
                                        | "phrust_native_base64_encode"
                                        | "phrust_native_base64_decode"
                                        | "phrust_native_bin2hex"
                                        | "phrust_native_hex2bin"
                                        | "phrust_native_quoted_printable_decode"
                                        | "phrust_native_urlencode"
                                        | "phrust_native_rawurlencode"
                                        | "phrust_native_urldecode"
                                        | "phrust_native_rawurldecode"
                                        | "phrust_native_convert_uuencode"
                                        | "phrust_native_convert_uudecode"
                                        | "phrust_native_addcslashes"
                                        | "phrust_native_stripcslashes"
                                        | "phrust_native_stripslashes"
                                        | "phrust_native_quotemeta"
                                        | "phrust_native_pack"
                                        | "phrust_native_unpack"
                                        | "phrust_native_basename"
                                        | "phrust_native_dirname"
                                        | "phrust_native_realpath"
                                        | "phrust_native_file_exists"
                                        | "phrust_native_is_file"
                                        | "phrust_native_is_dir"
                                        | "phrust_native_is_readable"
                                        | "phrust_native_is_writable"
                                        | "phrust_native_is_link"
                                        | "phrust_native_fileperms"
                                        | "phrust_native_fileowner"
                                        | "phrust_native_filegroup"
                                        | "phrust_native_filetype"
                                        | "phrust_native_disk_free_space"
                                        | "phrust_native_disk_total_space"
                                        | "phrust_native_pathinfo"
                                        | "phrust_native_stat"
                                        | "phrust_native_lstat"
                                        | "phrust_native_file"
                                        | "phrust_native_glob"
                                        | "phrust_native_opendir"
                                        | "phrust_native_readdir"
                                        | "phrust_native_rewinddir"
                                        | "phrust_native_closedir"
                                        | "phrust_native_scandir"
                                        | "phrust_native_stream_get_meta_data"
                                        | "phrust_native_stream_get_wrappers"
                                        | "phrust_native_stream_is_local"
                                        | "phrust_native_stream_resolve_include_path"
                                        | "phrust_native_stream_context_create"
                                        | "phrust_native_stream_context_get_default"
                                        | "phrust_native_stream_context_get_options"
                                        | "phrust_native_stream_context_set_default"
                                        | "phrust_native_stream_context_set_option"
                                        | "phrust_native_stream_context_set_options"
                                        | "phrust_native_stream_filter_append"
                                        | "phrust_native_stream_filter_prepend"
                                        | "phrust_native_stream_filter_remove"
                                        | "phrust_native_stream_isatty"
                                        | "phrust_native_stream_set_timeout"
                                        | "phrust_native_chmod"
                                        | "phrust_native_symlink"
                                        | "phrust_native_readfile"
                                        | "phrust_native_is_uploaded_file"
                                        | "phrust_native_tempnam"
                                        | "phrust_native_tmpfile"
                                        | "phrust_native_filesize"
                                        | "phrust_native_filemtime"
                                        | "phrust_native_file_get_contents"
                                        | "phrust_native_file_put_contents"
                                        | "phrust_native_rename"
                                        | "phrust_native_unlink"
                                        | "phrust_native_mkdir"
                                        | "phrust_native_rmdir"
                                        | "phrust_native_touch"
                                        | "phrust_native_fopen"
                                        | "phrust_native_fwrite"
                                        | "phrust_native_fclose"
                                        | "phrust_native_fread"
                                        | "phrust_native_fgets"
                                        | "phrust_native_fgetc"
                                        | "phrust_native_feof"
                                        | "phrust_native_fflush"
                                        | "phrust_native_fseek"
                                        | "phrust_native_ftell"
                                        | "phrust_native_ftruncate"
                                        | "phrust_native_rewind"
                                        | "phrust_native_stream_get_contents"
                                        | "phrust_native_stream_copy_to_stream"
                                        | "phrust_native_ob_start"
                                        | "phrust_native_ob_get_clean"
                                        | "phrust_native_ob_get_contents"
                                        | "phrust_native_ob_get_flush"
                                        | "phrust_native_ob_get_length"
                                        | "phrust_native_ob_get_level"
                                        | "phrust_native_ob_end_flush"
                                        | "phrust_native_ob_end_clean"
                                ) =>
                        {
                            None
                        }
                        crate::JitRelocatableTarget::Helper(symbol) => Some(symbol.as_str()),
                        crate::JitRelocatableTarget::InternalFunction(_) => None,
                    }
                });
                if let Some(symbol) = forbidden {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_OPTIMIZER_HELPER_IMPORT",
                        format!(
                            "optimizing artifact attempted to publish forbidden runtime import {symbol}"
                        ),
                    ));
                }
            }
            handle.bind_relocatable_code(crate::JitRelocatableCode {
                root: function,
                code: relocatable_bytes,
                functions: relocatable_functions,
                relocations: relocatable_relocations,
            });
            compiled_clif_blocks.set(Some(clif_blocks));
            compiled_maximum_pre_regalloc.set(Some(maximum_pre_regalloc));
            compiled_maximum_temporary_cache_entries
                .set(Some(maximum_temporary_cache_entries));
            *selected_plan.borrow_mut() = active_plan;
            *selected_fragment_layout.borrow_mut() = active_fragment_layout;
            Ok((handle, code_bytes))
        },
    )?;
    let plan = selected_plan.into_inner();
    let fragment_layout = selected_fragment_layout.into_inner();
    let fragment_frame_metrics = fragment_layout.as_ref().map_or((0, 0, 0), |layout| {
        (
            layout.frame.value_slots,
            layout.frame.shared_register_slots,
            layout.frame.scratch_register_slots,
        )
    });
    let mut handle = compiled.handle;
    handle.bind_ssa_metrics(ssa_metrics.0, ssa_metrics.1, ssa_metrics.2);
    Ok(NativeScalarRegionCompileResult {
        handle,
        code_bytes: compiled.code_bytes,
        clif_blocks: compiled_clif_blocks.get(),
        maximum_pre_regalloc: compiled_maximum_pre_regalloc.get(),
        maximum_temporary_cache_entries: compiled_maximum_temporary_cache_entries.get(),
        fragment_frame_slots: fragment_frame_metrics.0,
        fragment_shared_register_slots: fragment_frame_metrics.1,
        fragment_scratch_register_slots: fragment_frame_metrics.2,
        pre_regalloc_replans: compiled_pre_regalloc_replans.get(),
        fast_path_hits,
        has_control_flow,
        compilation_mode,
        plan,
    })
}

pub(super) fn select_native_region_tier(
    candidate: &mut RegionGraph,
    _plan: &NativeCompilePlan,
    _constants: &[IrConstant],
) {
    if candidate.compile_metadata.tier == NativeCompilerTier::Optimizing {
        let _ = crate::region_ir::opt::optimize_executable_region(candidate);
        for block in &mut candidate.blocks {
            for instruction in &mut block.instructions {
                // Optimizing publication is now all-or-nothing. Unsupported
                // semantic shapes reject the complete optimizing artifact
                // before publication, so no instruction is a local
                // fast/slow-island entry.
                instruction.optimizer_transition_entry = false;
                instruction.transition_live_registers = None;
            }
        }
    }
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
        RegionInstructionKind::ArrayCallback(call) => Some(call.result),
        RegionInstructionKind::PregCallbackArray(call) => Some(call.result),
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
        | RegionInstructionKind::CompileTimeFatal { .. } => None,
    }
}

fn region_instruction_defined_registers(kind: &RegionInstructionKind) -> Vec<RegId> {
    let mut registers = region_instruction_result_register(kind)
        .into_iter()
        .collect::<Vec<_>>();
    match kind {
        RegionInstructionKind::ArrayInsert { array, .. }
        | RegionInstructionKind::ArraySpread { array, .. } => registers.push(*array),
        RegionInstructionKind::ForeachNext { key, value, .. } => {
            registers.extend(*key);
            registers.push(*value);
        }
        RegionInstructionKind::ForeachNextRef { key, .. } => registers.extend(*key),
        _ => {}
    }
    registers.sort_unstable();
    registers.dedup();
    registers
}

fn region_register_types(region: &RegionGraph) -> BTreeMap<RegId, ir::Type> {
    region
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .flat_map(|instruction| {
            region_instruction_defined_registers(&instruction.kind)
                .into_iter()
                .map(|register| (register, types::I64))
        })
        .collect()
}

/// Deliberately tiny first inlining tier. It handles only a stable zero-arity
/// function whose complete body returns one scalar constant. This preserves a
/// hard code-growth bound and cannot recursively inline a call graph.
fn bounded_inline_return(region: &RegionGraph) -> Option<BoundedInlineValue> {
    if region.return_type.is_some()
        || region.flags.is_method
        || region.flags.is_closure
        || region.flags.is_generator
        || region.blocks.len() != 1
    {
        return None;
    }
    let block = &region.blocks[0];
    let RegionTerminator::Return {
        value,
        finally: None,
    } = block.terminator
    else {
        return None;
    };
    match block.instructions.as_slice() {
        [] if region.params.is_empty()
            && matches!(value, RegionOperand::I64(_) | RegionOperand::Constant(_)) =>
        {
            Some(BoundedInlineValue::Constant(value))
        }
        [
            RegionInstruction {
                kind: RegionInstructionKind::Move { dst, src },
                ..
            },
        ] if value == RegionOperand::Register(*dst)
            && matches!(src, RegionOperand::I64(_) | RegionOperand::Constant(_)) =>
        {
            Some(BoundedInlineValue::Constant(*src))
        }
        [
            RegionInstruction {
                kind:
                    RegionInstructionKind::LoadLocal {
                        dst,
                        local,
                        quiet: false,
                    },
                ..
            },
        ] if value == RegionOperand::Register(*dst)
            && region.params.iter().all(|parameter| {
                parameter.required
                    && parameter.default.is_none()
                    && parameter.type_.is_none()
                    && !parameter.by_ref
                    && !parameter.variadic
            }) =>
        {
            region
                .parameter_locals
                .iter()
                .position(|parameter| parameter == local)
                .map(|index| BoundedInlineValue::Argument {
                    index,
                    arity: region.params.len(),
                })
        }
        _ => None,
    }
}

fn collect_bounded_inline_values(
    unit: &IrUnit,
    roots: &BTreeMap<FunctionId, RegionGraph>,
) -> BTreeMap<FunctionId, BoundedInlineValue> {
    if !roots
        .values()
        .any(|region| region.compile_metadata.tier == NativeCompilerTier::Optimizing)
    {
        return BTreeMap::new();
    }
    roots
        .values()
        .flat_map(RegionGraph::direct_callees)
        .filter(|callee| !roots.contains_key(callee))
        .filter(|callee| {
            unit.functions
                .get(callee.index())
                .is_some_and(|function| !ir_function_requires_trampoline(function))
        })
        .filter_map(|callee| {
            crate::region_ir::build_baseline_region(unit, callee)
                .ok()
                .and_then(|region| bounded_inline_return(&region))
                .map(|value| (callee, value))
        })
        .collect()
}

fn bounded_inline_rejection(region: &RegionGraph) -> &'static str {
    if !region.params.is_empty() {
        "arguments"
    } else if region.flags.is_method || region.flags.is_closure {
        "receiver-or-closure-environment"
    } else if region.flags.is_generator {
        "suspension"
    } else if region.return_type.is_some() {
        "return-type-check"
    } else if region.blocks.len() != 1 {
        "control-flow-complexity"
    } else {
        "not-bounded-wrapper"
    }
}

fn inline_rejection_counts(
    caller: &RegionGraph,
    regions: &BTreeMap<FunctionId, RegionGraph>,
) -> BTreeMap<String, u64> {
    let mut reasons = BTreeMap::new();
    for call in caller
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.kind {
            RegionInstructionKind::NativeCall(call) => Some(call),
            _ => None,
        })
    {
        let Some(target) = call.direct_compiled_target() else {
            continue;
        };
        let Some(callee) = regions.get(&target) else {
            continue;
        };
        if bounded_inline_return(callee)
            .and_then(|value| bounded_inline_call_operand(call, value))
            .is_some()
        {
            continue;
        }
        let reason = if call.operands.is_empty() {
            bounded_inline_rejection(callee)
        } else {
            "arguments-or-receiver"
        };
        let count = reasons.entry(reason.to_owned()).or_insert(0_u64);
        *count = count.saturating_add(1);
    }
    reasons
}

/// Selects the deliberately small tail-call subset whose callee can consume
/// the caller's packed argument buffer directly. This avoids allocating a
/// second arena frame and transfers the caller's argument ownership exactly
/// once. More general tail calls need an owned-frame transfer protocol.
fn bounded_tail_forward_target(
    region: &RegionGraph,
    block: &crate::region_ir::RegionBlock,
    regions: &BTreeMap<FunctionId, RegionGraph>,
) -> Option<(u32, FunctionId)> {
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (region, block, regions);
        return None;
    }

    #[cfg(target_arch = "x86_64")]
    {
        let RegionTerminator::Return {
            value: RegionOperand::Register(returned),
            finally: None,
        } = &block.terminator
        else {
            return None;
        };
        let (last, prefix) = block.instructions.split_last()?;
        let RegionInstructionKind::NativeCall(call) = &last.kind else {
            return None;
        };
        let RegionCallResult::Register(destination) = call.result else {
            return None;
        };
        let target = call.direct_compiled_target()?;
        let callee = regions.get(&target)?;
        if destination != *returned
            || target == region.function
            || call.argument_operand_offset != 0
            || call.variadic
            || call.returns_by_reference
            || region.returns_by_ref
            || callee.returns_by_ref
            || region.params != callee.params
            || region.return_type != callee.return_type
            || !region.exception_regions.is_empty()
            || !callee.exception_regions.is_empty()
            || region.flags.is_generator
            || region.flags.is_closure
            || region.flags.is_method
            || callee.flags.is_generator
            || callee.flags.is_closure
            || callee.flags.is_method
            || prefix.len() != region.parameter_locals.len()
            || call.operands.len() != region.parameter_locals.len()
            || !callee
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .all(|instruction| {
                    matches!(
                        instruction.kind,
                        RegionInstructionKind::Nop
                            | RegionInstructionKind::Move { .. }
                            | RegionInstructionKind::LoadLocal { .. }
                    )
                })
        {
            return None;
        }
        for (((instruction, local), operand), parameter) in prefix
            .iter()
            .zip(&region.parameter_locals)
            .zip(&call.operands)
            .zip(&call.args)
        {
            let RegionInstructionKind::LoadLocal {
                dst,
                local: loaded,
                quiet: false,
            } = &instruction.kind
            else {
                return None;
            };
            if *loaded != *local
                || *operand != Some(RegionOperand::Register(*dst))
                || parameter.name.is_some()
                || parameter.unpack
                || parameter.by_ref_local.is_some()
                || parameter.by_ref_dim.is_some()
                || parameter.by_ref_property.is_some()
                || parameter.by_ref_property_dim.is_some()
            {
                return None;
            }
        }
        Some((last.continuation_id, target))
    }
}

fn region_graph_signature(
    module: &JITModule,
    region: &RegionGraph,
) -> Result<Signature, CraneliftLoweringError> {
    region_arity(region)?;
    Ok(native_php_entry_signature(module))
}

fn direct_array_ensure_unique_signature(module: &JITModule) -> Signature {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I8));
    signature.returns.push(AbiParam::new(types::I32));
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn direct_array_child_entry_signature(module: &JITModule) -> Signature {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(pointer_type));
    signature
}

fn direct_value_release_signature(module: &JITModule) -> Signature {
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(types::I8));
    signature
}

fn region_fragment_signature(
    module: &JITModule,
    region: &RegionGraph,
) -> Result<Signature, CraneliftLoweringError> {
    region_arity(region)?;
    let pointer_type = module.target_config().pointer_type();
    let mut signature = module.make_signature();
    #[cfg(target_arch = "x86_64")]
    {
        signature.call_conv = CallConv::Tail;
    }
    signature.params.push(AbiParam::new(pointer_type));
    signature.params.push(AbiParam::new(pointer_type));
    signature.returns.push(AbiParam::new(types::I32));
    Ok(signature)
}

type DeclaredFragmentFunctions = (BTreeMap<u32, FuncId>, BTreeMap<u32, FunctionId>);

#[allow(clippy::too_many_arguments)]
fn declare_fragment_functions(
    module: &mut JITModule,
    root_symbol: &str,
    region: &RegionGraph,
    layout: Option<&NativeFunctionFragmentLayout>,
    replan_attempt: usize,
    next_synthetic: &mut u32,
    functions: &mut BTreeMap<FunctionId, FuncId>,
) -> Result<DeclaredFragmentFunctions, CraneliftLoweringError> {
    let mut fragment_functions = BTreeMap::new();
    let mut fragment_symbols = BTreeMap::new();
    let Some(layout) = layout else {
        return Ok((fragment_functions, fragment_symbols));
    };
    if layout.fragments.len() == 1 {
        fragment_functions.insert(layout.fragments[0].id, functions[&region.function]);
        return Ok((fragment_functions, fragment_symbols));
    }
    for fragment in &layout.fragments {
        let synthetic = FunctionId::new(*next_synthetic);
        *next_synthetic = next_synthetic.checked_add(1).ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_FRAGMENT_SYMBOL_LIMIT",
                "native fragment symbol id overflowed",
            )
        })?;
        let symbol = if replan_attempt == 0 {
            format!("{root_symbol}.fragment.{}", fragment.id)
        } else {
            format!(
                "{root_symbol}.replan.{replan_attempt}.fragment.{}",
                fragment.id
            )
        };
        let signature = region_fragment_signature(module, region)?;
        let func_id = module
            .declare_function(&symbol, Linkage::Local, &signature)
            .map_err(|error| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_DECLARE_FRAGMENT",
                    format!("failed to declare native fragment {symbol}: {error}"),
                )
            })?;
        fragment_functions.insert(fragment.id, func_id);
        fragment_symbols.insert(fragment.id, synthetic);
        functions.insert(synthetic, func_id);
    }
    Ok((fragment_functions, fragment_symbols))
}

pub(super) struct DefinedRegionFunction {
    lowered_function: Option<ir::Function>,
    code: Vec<u8>,
    clif_blocks: usize,
    alignment: u64,
    relocations: Vec<crate::JitRelocatableRelocation>,
    native_pc_ranges: Vec<crate::JitNativePcRange>,
    native_stack_bytes: u32,
    pre_regalloc: PreRegallocMetrics,
    maximum_temporary_cache_entries: usize,
    production_lowering: Vec<crate::JitProductionLoweringMetadata>,
}

const MAX_NATIVE_SPILL_FRAME_BYTES: u32 = 1024 * 1024;
const MAX_FRAGMENT_CLIF_BLOCKS: usize = 768;
const MAX_OPTIMIZING_CLIF_BLOCKS: usize = 4_096;
const MAX_FRAGMENT_CLIF_VALUES: usize = 16_384;
const MAX_OPTIMIZING_CLIF_VALUES: usize = 65_536;
const MAX_FRAGMENT_CLIF_INSTRUCTIONS: usize = 32_768;
const MAX_OPTIMIZING_CLIF_INSTRUCTIONS: usize = 65_536;
const MAX_FRAGMENT_BLOCK_PARAMETERS: usize = 4_096;
const MAX_OPTIMIZING_BLOCK_PARAMETERS: usize = 16_384;
// Exact CLIF must retain 30% headroom below the absolute backend ceiling.
// This is intentionally stricter than merely avoiding a hard rejection: it
// keeps the admitted regalloc graph away from the nonlinear edge while the
// planner's cheaper estimate remains calibrated independently.
const PRE_REGALLOC_REPLAN_MARGIN_PERCENT: usize = 70;
// The planner admits at most 64 Region blocks per fragment. Six bisection
// rounds are therefore sufficient to reduce every splittable offender to one
// Region block (ceil(log2(64))). A remaining offender is structurally
// unsplittable and is rejected before regalloc; this is a proof-derived bound,
// not a wall-time retry budget.
const MAX_PRE_REGALLOC_REPLAN_ATTEMPTS: usize = 6;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct PreRegallocMetrics {
    pub(super) blocks: usize,
    pub(super) values: usize,
    pub(super) instructions: usize,
    pub(super) block_parameters: usize,
    pub(super) loads: usize,
    pub(super) stores: usize,
    pub(super) loads_per_source_instruction_milli: usize,
    pub(super) stores_per_source_instruction_milli: usize,
}

impl PreRegallocMetrics {
    fn limits(tier: NativeCompilerTier) -> (usize, usize, usize, usize) {
        if tier == NativeCompilerTier::Optimizing {
            (
                MAX_OPTIMIZING_CLIF_BLOCKS,
                MAX_OPTIMIZING_CLIF_VALUES,
                MAX_OPTIMIZING_CLIF_INSTRUCTIONS,
                MAX_OPTIMIZING_BLOCK_PARAMETERS,
            )
        } else {
            (
                MAX_FRAGMENT_CLIF_BLOCKS,
                MAX_FRAGMENT_CLIF_VALUES,
                MAX_FRAGMENT_CLIF_INSTRUCTIONS,
                MAX_FRAGMENT_BLOCK_PARAMETERS,
            )
        }
    }

    fn exceeds_replan_margin(self, tier: NativeCompilerTier) -> bool {
        let (blocks, values, instructions, parameters) = Self::limits(tier);
        self.blocks.saturating_mul(100) > blocks.saturating_mul(PRE_REGALLOC_REPLAN_MARGIN_PERCENT)
            || self.values.saturating_mul(100)
                > values.saturating_mul(PRE_REGALLOC_REPLAN_MARGIN_PERCENT)
            || self.instructions.saturating_mul(100)
                > instructions.saturating_mul(PRE_REGALLOC_REPLAN_MARGIN_PERCENT)
            || self.block_parameters.saturating_mul(100)
                > parameters.saturating_mul(PRE_REGALLOC_REPLAN_MARGIN_PERCENT)
    }

    /// Minimum number of approximately balanced fragments required by the
    /// largest exact CLIF dimension. This is a planning hint only: every
    /// resulting fragment is exact-preflighted again before regalloc.
    fn minimum_fragment_count(self, tier: NativeCompilerTier) -> usize {
        let percent = PRE_REGALLOC_REPLAN_MARGIN_PERCENT;
        let (blocks, values, instructions, parameters) = Self::limits(tier);
        let block_limit = blocks.saturating_mul(percent) / 100;
        let value_limit = values.saturating_mul(percent) / 100;
        let instruction_limit = instructions.saturating_mul(percent) / 100;
        let parameter_limit = parameters.saturating_mul(percent) / 100;
        [
            self.blocks.div_ceil(block_limit.max(1)),
            self.values.div_ceil(value_limit.max(1)),
            self.instructions.div_ceil(instruction_limit.max(1)),
            self.block_parameters.div_ceil(parameter_limit.max(1)),
        ]
        .into_iter()
        .max()
        .unwrap_or(2)
        .max(2)
    }

    fn max_assign(&mut self, other: Self) {
        self.blocks = self.blocks.max(other.blocks);
        self.values = self.values.max(other.values);
        self.instructions = self.instructions.max(other.instructions);
        self.block_parameters = self.block_parameters.max(other.block_parameters);
        self.loads = self.loads.max(other.loads);
        self.stores = self.stores.max(other.stores);
        self.loads_per_source_instruction_milli = self
            .loads_per_source_instruction_milli
            .max(other.loads_per_source_instruction_milli);
        self.stores_per_source_instruction_milli = self
            .stores_per_source_instruction_milli
            .max(other.stores_per_source_instruction_milli);
    }
}

pub(super) fn validate_pre_regalloc_structure(
    function: &ir::Function,
    region: &RegionGraph,
    fragment: Option<u32>,
) -> Result<PreRegallocMetrics, CraneliftLoweringError> {
    let blocks = function.layout.blocks().count();
    let values = function.dfg.num_values();
    let instructions = function
        .layout
        .blocks()
        .map(|block| function.layout.block_insts(block).count())
        .sum::<usize>();
    let block_parameters = function
        .layout
        .blocks()
        .map(|block| function.dfg.block_params(block).len())
        .sum::<usize>();
    let mut loads = 0_usize;
    let mut stores = 0_usize;
    for block in function.layout.blocks() {
        for instruction in function.layout.block_insts(block) {
            match function.dfg.insts[instruction].opcode() {
                ir::Opcode::Load | ir::Opcode::StackLoad => loads = loads.saturating_add(1),
                ir::Opcode::Store | ir::Opcode::StackStore => stores = stores.saturating_add(1),
                _ => {}
            }
        }
    }
    let (maximum_blocks, maximum_values, maximum_instructions, maximum_block_parameters) =
        if region.compile_metadata.tier == NativeCompilerTier::Optimizing {
            (
                MAX_OPTIMIZING_CLIF_BLOCKS,
                MAX_OPTIMIZING_CLIF_VALUES,
                MAX_OPTIMIZING_CLIF_INSTRUCTIONS,
                MAX_OPTIMIZING_BLOCK_PARAMETERS,
            )
        } else {
            (
                MAX_FRAGMENT_CLIF_BLOCKS,
                MAX_FRAGMENT_CLIF_VALUES,
                MAX_FRAGMENT_CLIF_INSTRUCTIONS,
                MAX_FRAGMENT_BLOCK_PARAMETERS,
            )
        };
    if blocks > maximum_blocks
        || values > maximum_values
        || instructions > maximum_instructions
        || block_parameters > maximum_block_parameters
    {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_PRE_REGALLOC_BUDGET",
            format!(
                "function {} fragment={} exceeds the pre-regalloc ceiling: clif_blocks={blocks}/{maximum_blocks} clif_values={values}/{maximum_values} clif_instructions={instructions}/{maximum_instructions} block_parameters={block_parameters}/{maximum_block_parameters}",
                region.function_name,
                fragment.map_or_else(|| "whole".to_owned(), |id| id.to_string()),
            ),
        ));
    }
    Ok(PreRegallocMetrics {
        blocks,
        values,
        instructions,
        block_parameters,
        loads,
        stores,
        loads_per_source_instruction_milli: 0,
        stores_per_source_instruction_milli: 0,
    })
}

fn compile_preflighted_region_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    func_id: FuncId,
    region: &RegionGraph,
    functions: &BTreeMap<FunctionId, FuncId>,
    mut defined: DefinedRegionFunction,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    ctx.func = defined.lowered_function.take().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_MISSING_PREFLIGHT_CLIF",
            "exact preflight did not retain its verified CLIF function",
        )
    })?;
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define preflighted native function: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            "Cranelift returned no compiled machine-code buffer",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |layout| layout.frame_to_fp_offset);
    if native_stack_bytes > MAX_NATIVE_SPILL_FRAME_BYTES {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_NATIVE_STACK_LIMIT",
            format!(
                "function {} requires {native_stack_bytes} native stack bytes; limit is {MAX_NATIVE_SPILL_FRAME_BYTES}",
                region.function_name
            ),
        ));
    }
    defined.code = compiled.code_buffer().to_vec();
    defined.alignment = u64::from(compiled.buffer.alignment)
        .max(module.isa().function_alignment().minimum as u64)
        .max(module.isa().symbol_alignment());
    defined.relocations = compiled
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
    defined.native_pc_ranges = ctx
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
    defined.native_stack_bytes = native_stack_bytes;
    module.clear_context(ctx);
    Ok(defined)
}

#[allow(clippy::too_many_arguments)]
fn define_region_fragment_wrapper(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    region: &RegionGraph,
    func_id: FuncId,
    fragment_functions: &BTreeMap<u32, FuncId>,
    layout: &NativeFunctionFragmentLayout,
    relocation_functions: &BTreeMap<FunctionId, FuncId>,
    value_flow: &ExecutableValueFlow,
    tier_operations: NativeTierOperations,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    ctx.func.signature = region_graph_signature(module, region)?;
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let params = builder.block_params(entry).to_vec();
        let runtime = params[0];
        let arguments = params[1];
        let result_out = params[2];
        let deopt_out = params[3];
        let resume_id = params[4];
        let resume_state = params[5];
        if region.compile_metadata.tier == NativeCompilerTier::Baseline {
            lower_baseline_function_entry(&mut builder, deopt_out, region.function)?;
        }
        let (arguments, resume_id) = if region.compile_metadata.tier == NativeCompilerTier::Baseline
        {
            let NativeTierOperations::Baseline { operations, .. } = tier_operations else {
                return Err(CraneliftLoweringError::new(
                    "JIT_CRANELIFT_NATIVE_ENTRY_BINDING",
                    "baseline fragment wrapper has no baseline operation plane",
                ));
            };
            lower_baseline_bind_packed_arguments(
                module,
                &mut builder,
                operations
                    .argument_check
                    .map(|helper| helper.with_runtime(runtime)),
                &region.params,
                region
                    .parameter_locals
                    .len()
                    .saturating_sub(region.params.len()),
                arguments,
                result_out,
                deopt_out,
                resume_id,
                region.function,
            )?
        } else {
            (arguments, resume_id)
        };
        let frame_layout = &layout.frame;
        let frame_bytes = frame_layout.frame_bytes()?;
        let frame_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            frame_bytes,
            3,
        ));
        let frame = builder.ins().stack_addr(pointer_type, frame_slot, 0);
        let uninitialized = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        for local in frame_layout.local_slots.keys().copied() {
            let initial = if matches!(
                value_flow.local_storage(local),
                crate::region_ir::LocalStorageClass::RequestGlobal
                    | crate::region_ir::LocalStorageClass::Superglobal
            ) {
                lower_trusted_request_local_reference(
                    &mut builder,
                    deopt_out,
                    region.function,
                    local,
                )
            } else {
                uninitialized
            };
            builder.ins().store(
                MemFlagsData::new(),
                initial,
                frame,
                frame_layout.local_offset(local)?,
            );
        }
        for (index, local) in region.parameter_locals.iter().enumerate() {
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                arguments,
                i32::try_from(index.saturating_mul(8)).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_ARITY",
                        "fragment wrapper argument offset does not fit the native ABI",
                    )
                })?,
            );
            if value_flow.owns_parameter_at_entry(*local) {
                lower_optimizing_retain(&mut builder, value, deopt_out);
            }
            builder.ins().store(
                MemFlagsData::new(),
                value,
                frame,
                frame_layout.local_offset(*local)?,
            );
        }
        let continue_status = builder
            .ins()
            .iconst(types::I32, i64::from(crate::JitCallStatus::CONTINUE.0));
        let empty = builder.ins().iconst(types::I64, 0);
        builder.ins().store(
            MemFlagsData::new(),
            continue_status,
            frame,
            frame_layout.pending_status_offset(),
        );
        builder.ins().store(
            MemFlagsData::new(),
            empty,
            frame,
            frame_layout.pending_value_offset(),
        );
        for (value, offset) in [
            (arguments, frame_layout.arguments_offset()),
            (result_out, frame_layout.result_out_offset()),
            (deopt_out, frame_layout.deopt_out_offset()),
            (resume_state, frame_layout.resume_state_offset()),
        ] {
            builder
                .ins()
                .store(MemFlagsData::new(), value, frame, offset);
        }
        builder.ins().store(
            MemFlagsData::new(),
            resume_id,
            frame,
            frame_layout.resume_id_offset(),
        );

        let call_blocks = layout
            .fragments
            .iter()
            .map(|fragment| (fragment.id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let root_fragment = layout.block_owner[&BlockId::new(0)];
        if layout.fragments.len() == 1 {
            builder.ins().jump(call_blocks[&root_fragment], &[]);
        } else {
            // Cranelift lowers a sparse `Switch` to control-flow blocks for
            // every resume id. Large PHP functions have hundreds of precise
            // transition ids, so that representation made this tiny wrapper
            // larger than a bounded fragment. Match all ids owned by a
            // fragment in one straight-line predicate instead. Intermediate
            // compare values die immediately and the wrapper CFG now scales
            // with the number of fragments, not the number of safepoints.
            for fragment in &layout.fragments {
                if fragment.id == root_fragment {
                    continue;
                }
                let mut matches_fragment = None;
                for encoded_resume in
                    layout
                        .resume_owner
                        .iter()
                        .filter_map(|(encoded_resume, owner)| {
                            (*owner == fragment.id).then_some(*encoded_resume)
                        })
                {
                    let matches_resume =
                        builder
                            .ins()
                            .icmp_imm(IntCC::Equal, resume_id, i64::from(encoded_resume));
                    matches_fragment = Some(match matches_fragment {
                        Some(previous) => builder.ins().bor(previous, matches_resume),
                        None => matches_resume,
                    });
                }
                if let Some(matches_fragment) = matches_fragment {
                    let next_fragment = builder.create_block();
                    builder.ins().brif(
                        matches_fragment,
                        call_blocks[&fragment.id],
                        &[],
                        next_fragment,
                        &[],
                    );
                    builder.switch_to_block(next_fragment);
                }
            }
            builder.ins().jump(call_blocks[&root_fragment], &[]);
        }

        for fragment in &layout.fragments {
            builder.switch_to_block(call_blocks[&fragment.id]);
            let callee =
                module.declare_func_in_func(fragment_functions[&fragment.id], builder.func);
            let entry_block = fragment
                .normal_entries
                .iter()
                .next()
                .copied()
                .unwrap_or(BlockId::new(0));
            let entry_id = builder
                .ins()
                .iconst(types::I32, i64::from(entry_block.raw()));
            builder.ins().store(
                MemFlagsData::new(),
                entry_id,
                frame,
                frame_layout.entry_id_offset(),
            );
            let call = builder.ins().call(callee, &[runtime, frame]);
            let status = builder.inst_results(call)[0];
            builder.ins().return_(&[status]);
        }
        builder.seal_all_blocks();
        builder.finalize();
    }
    let pre_regalloc = validate_pre_regalloc_structure(&ctx.func, region, None)?;
    let verifier_flags = settings::Flags::new(settings::builder());
    verify_function(&ctx.func, &verifier_flags).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_FRAGMENT_WRAPPER",
            format!("Cranelift verifier rejected fragment wrapper: {error}"),
        )
    })?;
    let clif_blocks = ctx.func.layout.blocks().count();
    module.define_function(func_id, ctx).map_err(|error| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_FRAGMENT_WRAPPER",
            format!("failed to define native fragment wrapper: {error}"),
        )
    })?;
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_FRAGMENT_WRAPPER",
            "Cranelift returned no fragment-wrapper machine code",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |frame| frame.frame_to_fp_offset);
    if native_stack_bytes > MAX_NATIVE_SPILL_FRAME_BYTES {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_FRAGMENT_WRAPPER_STACK_LIMIT",
            format!(
                "fragment wrapper requires {native_stack_bytes} native stack bytes; limit is {MAX_NATIVE_SPILL_FRAME_BYTES}"
            ),
        ));
    }
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
                relocation_functions,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations,
        native_pc_ranges: Vec::new(),
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries: 0,
        production_lowering: Vec::new(),
    })
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

fn lower_entry_array_source(
    builder: &mut FunctionBuilder<'_>,
    arguments: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    source: NativeEntryArraySource,
) -> Result<ir::Value, CraneliftLoweringError> {
    match source {
        NativeEntryArraySource::Parameter(index) => Ok(builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            arguments,
            i32::try_from(index.saturating_mul(8)).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_ARRAY_ADMISSION_OFFSET",
                    "array admission parameter offset does not fit the native ABI",
                )
            })?,
        )),
        NativeEntryArraySource::TrustedGlobal(continuation_id) => {
            let reference = lower_trusted_global_reference_at_continuation(
                builder,
                function,
                continuation_id,
                deopt_out,
            );
            let slot = lower_optimizing_slot_address(builder, reference, deopt_out);
            Ok(builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
            ))
        }
    }
}

fn emit_optimizing_entry_direct_array_descriptor(
    builder: &mut FunctionBuilder<'_>,
    array: ir::Value,
    deopt_out: ir::Value,
    rejected: ir::Block,
) -> (ir::Value, ir::Value, ir::Value) {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let inspect_index = builder.create_block();
    let inspect_slot = builder.create_block();
    let accepted = builder.create_block();
    builder.append_block_param(inspect_slot, types::I64);
    builder.append_block_param(accepted, pointer_type);
    builder.append_block_param(accepted, types::I64);
    builder.append_block_param(accepted, pointer_type);

    let tagged = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
    let encoded_index = builder.ins().ireduce(types::I32, array);
    let direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        encoded_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(tagged, direct_index);
    builder
        .ins()
        .brif(direct, inspect_index, &[], rejected, &[]);

    builder.switch_to_block(inspect_index);
    let index = builder.ins().iadd_imm(
        encoded_index,
        -i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let in_bounds = builder.ins().icmp_imm(
        IntCC::UnsignedLessThan,
        index,
        crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY as i64,
    );
    builder
        .ins()
        .brif(in_bounds, inspect_slot, &[array.into()], rejected, &[]);

    builder.switch_to_block(inspect_slot);
    let array = builder.block_params(inspect_slot)[0];
    let slot = lower_optimizing_slot_address(builder, array, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let length = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let direct_kind = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
    );
    let abi_mask = (1_i64 << crate::JIT_NATIVE_DIRECT_ARRAY_CURSOR_SHIFT) - 1;
    let abi = builder.ins().band_imm(flags, abi_mask);
    let abi_matches = builder.ins().icmp_imm(
        IntCC::Equal,
        abi,
        i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION),
    );
    let bounded = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        length,
        crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
    );
    let admitted = builder.ins().band(direct_kind, abi_matches);
    let admitted = builder.ins().band(admitted, bounded);
    builder.ins().brif(
        admitted,
        accepted,
        &[slot.into(), length.into(), entries.into()],
        rejected,
        &[],
    );

    builder.switch_to_block(accepted);
    (
        builder.block_params(accepted)[0],
        builder.block_params(accepted)[1],
        builder.block_params(accepted)[2],
    )
}

fn lower_optimizing_entry_array_key(
    builder: &mut FunctionBuilder<'_>,
    key: &NativeEntryArrayKey,
    _deopt_out: ir::Value,
) -> ir::Value {
    match key {
        NativeEntryArrayKey::Integer(value) => builder.ins().iconst(types::I64, *value),
        NativeEntryArrayKey::Constant(index) => builder
            .ins()
            .iconst(types::I64, crate::jit_encode_constant(*index)),
    }
}

fn emit_optimizing_entry_array_probe_paths(
    builder: &mut FunctionBuilder<'_>,
    paths: &[NativeEntryArrayProbeRequirement],
    root_length: ir::Value,
    root_entries: ir::Value,
    deopt_out: ir::Value,
    rejected: ir::Block,
) {
    for path in paths {
        let path_done = builder.create_block();
        let mut length = root_length;
        let mut entries = root_entries;
        for (index, key) in path.keys.iter().enumerate() {
            let key = lower_optimizing_entry_array_key(builder, key, deopt_out);
            let (found, value) = lower_optimizing_direct_array_lookup_optional(
                builder, length, entries, key, deopt_out,
            );
            let found_block = builder.create_block();
            builder.ins().brif(found, found_block, &[], path_done, &[]);
            builder.switch_to_block(found_block);
            if index + 1 < path.keys.len() {
                let (_, child_length, child_entries) =
                    emit_optimizing_entry_direct_array_descriptor(
                        builder, value, deopt_out, rejected,
                    );
                length = child_length;
                entries = child_entries;
            } else {
                if path.leaf == NativeEntryArrayProbeLeaf::PlainValue {
                    let authoritative =
                        lower_optimizing_call_value_is_authoritative(builder, value);
                    let reference =
                        lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
                    let plain = builder.ins().icmp_imm(IntCC::Equal, reference, 0);
                    let admitted = builder.ins().band(authoritative, plain);
                    let value_ready = builder.create_block();
                    builder
                        .ins()
                        .brif(admitted, value_ready, &[], rejected, &[]);
                    builder.switch_to_block(value_ready);
                }
                builder.ins().jump(path_done, &[]);
            }
        }
        builder.switch_to_block(path_done);
    }
}

fn emit_optimizing_entry_array_mutations(
    builder: &mut FunctionBuilder<'_>,
    mutations: &[NativeEntryArrayMutationRequirement],
    root: ir::Value,
    insertion_budget: usize,
    deopt_out: ir::Value,
    rejected: ir::Block,
) {
    for mutation in mutations {
        let (root_slot, root_length, root_entries) =
            emit_optimizing_entry_direct_array_descriptor(builder, root, deopt_out, rejected);
        let root_refcount = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            root_slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
        );
        let root_unique = builder.ins().icmp_imm(IntCC::Equal, root_refcount, 1);
        let root_ready = builder.create_block();
        builder
            .ins()
            .brif(root_unique, root_ready, &[], rejected, &[]);
        builder.switch_to_block(root_ready);

        let parents = match mutation {
            NativeEntryArrayMutationRequirement::Assign { parents, .. }
            | NativeEntryArrayMutationRequirement::Append { parents }
            | NativeEntryArrayMutationRequirement::Unset { parents, .. }
            | NativeEntryArrayMutationRequirement::Reference { parents, .. }
            | NativeEntryArrayMutationRequirement::ReferenceAppend { parents } => parents,
        };
        let mut current_array = root;
        let mut current_slot = root_slot;
        let mut current_length = root_length;
        let mut current_entries = root_entries;
        for parent in parents {
            let parent = builder.ins().iconst(types::I64, *parent);
            let (found, child) = lower_optimizing_direct_array_lookup_optional(
                builder,
                current_length,
                current_entries,
                parent,
                deopt_out,
            );
            let child_found = builder.create_block();
            builder.ins().brif(found, child_found, &[], rejected, &[]);
            builder.switch_to_block(child_found);
            let (child_slot, child_length, child_entries) =
                emit_optimizing_entry_direct_array_descriptor(builder, child, deopt_out, rejected);
            let child_refcount = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                child_slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
            );
            let child_unique = builder.ins().icmp_imm(IntCC::Equal, child_refcount, 1);
            let child_ready = builder.create_block();
            builder
                .ins()
                .brif(child_unique, child_ready, &[], rejected, &[]);
            builder.switch_to_block(child_ready);
            current_array = child;
            current_slot = child_slot;
            current_length = child_length;
            current_entries = child_entries;
        }

        if !matches!(mutation, NativeEntryArrayMutationRequirement::Unset { .. }) {
            let reserved = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                current_slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
            );
            let reserved = builder.ins().uextend(types::I64, reserved);
            let required = builder.ins().iadd_imm(
                current_length,
                i64::try_from(insertion_budget).unwrap_or(i64::MAX),
            );
            let has_capacity =
                builder
                    .ins()
                    .icmp(IntCC::UnsignedGreaterThanOrEqual, reserved, required);
            let capacity_ready = builder.create_block();
            builder
                .ins()
                .brif(has_capacity, capacity_ready, &[], rejected, &[]);
            builder.switch_to_block(capacity_ready);
        }

        match mutation {
            NativeEntryArrayMutationRequirement::Append { .. }
            | NativeEntryArrayMutationRequirement::ReferenceAppend { .. } => {
                let state = lower_direct_array_state_address(builder, current_array, deopt_out);
                let next_key = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    state,
                    std::mem::offset_of!(crate::JitNativeDirectArrayState, next_append_key) as i32,
                );
                let has_next = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    state,
                    std::mem::offset_of!(crate::JitNativeDirectArrayState, has_next_append_key)
                        as i32,
                );
                let maximum = builder.ins().icmp_imm(
                    IntCC::SignedGreaterThan,
                    next_key,
                    i64::MAX.saturating_sub(i64::try_from(insertion_budget).unwrap_or(i64::MAX)),
                );
                let has_next = builder.ins().icmp_imm(IntCC::NotEqual, has_next, 0);
                let overflow = builder.ins().band(maximum, has_next);
                let safe = builder.ins().icmp_imm(IntCC::Equal, overflow, 0);
                let append_ready = builder.create_block();
                builder.ins().brif(safe, append_ready, &[], rejected, &[]);
                builder.switch_to_block(append_ready);
            }
            NativeEntryArrayMutationRequirement::Assign { key, .. }
            | NativeEntryArrayMutationRequirement::Unset { key, .. }
            | NativeEntryArrayMutationRequirement::Reference { key, .. } => {
                let key = builder.ins().iconst(types::I64, *key);
                let (found, old) = lower_optimizing_direct_array_lookup_optional(
                    builder,
                    current_length,
                    current_entries,
                    key,
                    deopt_out,
                );
                let runtime = lower_is_runtime_handle(builder, old);
                let immediate = builder.ins().icmp_imm(IntCC::Equal, runtime, 0);
                let missing = builder.ins().icmp_imm(IntCC::Equal, found, 0);
                let safe = builder.ins().bor(missing, immediate);
                let value_ready = builder.create_block();
                builder.ins().brif(safe, value_ready, &[], rejected, &[]);
                builder.switch_to_block(value_ready);
            }
        }
    }
}

fn emit_optimizing_entry_property_slot(
    builder: &mut FunctionBuilder<'_>,
    object: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    requirement: NativeEntryPropertyRequirement,
    insertion_budget: usize,
    rejected: ir::Block,
) -> ir::Value {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let inspect = builder.create_block();
    let inspect_plan = builder.create_block();
    let accepted = builder.create_block();
    builder.append_block_param(accepted, pointer_type);

    let tagged = lower_value_has_tag(builder, object, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
    let raw_index = builder.ins().ireduce(types::I32, object);
    let direct_index = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        raw_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct = builder.ins().band(tagged, direct_index);
    builder.ins().brif(direct, inspect, &[], rejected, &[]);

    builder.switch_to_block(inspect);
    let object_slot = lower_optimizing_slot_address(builder, object, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        object_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        object_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let layout_id = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        object_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let slot_count = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        object_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
    );
    let property_slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        object_slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let kind_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT),
    );
    let version = builder.ins().band_imm(
        flags,
        i64::from(crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_MASK),
    );
    let version_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        version,
        i64::from(crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION),
    );
    let slots_ok = builder.ins().icmp_imm(IntCC::NotEqual, property_slots, 0);
    let admitted = builder.ins().band(kind_ok, version_ok);
    let admitted = builder.ins().band(admitted, slots_ok);
    builder
        .ins()
        .brif(admitted, inspect_plan, &[], rejected, &[]);

    builder.switch_to_block(inspect_plan);
    let view = lower_active_runtime_view(builder, deopt_out);
    let offsets = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(
            crate::JitNativeRuntimeView,
            trusted_property_function_offsets,
        ) as i32,
    );
    let plans = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_property_slots) as i32,
    );
    let function_entry = builder.ins().iadd_imm(
        offsets,
        i64::try_from(function.index().saturating_mul(4)).unwrap_or(i64::MAX),
    );
    let plan_base = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), function_entry, 0);
    let plan_index = builder
        .ins()
        .iadd_imm(plan_base, i64::from(requirement.continuation_id));
    let wide_plan_index = builder.ins().uextend(pointer_type, plan_index);
    let plan_offset = builder.ins().ishl_imm(wide_plan_index, 4);
    let plan = builder.ins().iadd(plans, plan_offset);
    let state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        plan,
        std::mem::offset_of!(crate::JitNativeTrustedPropertySlot, state) as i32,
    );
    let expected_layout = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        plan,
        std::mem::offset_of!(crate::JitNativeTrustedPropertySlot, layout_id) as i32,
    );
    let property_index = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        plan,
        std::mem::offset_of!(crate::JitNativeTrustedPropertySlot, slot_index) as i32,
    );
    let published =
        builder
            .ins()
            .icmp_imm(IntCC::Equal, state, i64::from(requirement.required_state));
    let family = builder.ins().icmp_imm(IntCC::Equal, expected_layout, 0);
    let exact = builder.ins().icmp(IntCC::Equal, layout_id, expected_layout);
    let layout_ok = builder.ins().bor(family, exact);
    let index_ok = builder
        .ins()
        .icmp(IntCC::UnsignedLessThan, property_index, slot_count);
    let admitted = builder.ins().band(published, layout_ok);
    let admitted = builder.ins().band(admitted, index_ok);
    let property_ready = builder.create_block();
    builder
        .ins()
        .brif(admitted, property_ready, &[], rejected, &[]);

    builder.switch_to_block(property_ready);
    let wide_property_index = builder.ins().uextend(pointer_type, property_index);
    let property_offset = builder.ins().ishl_imm(wide_property_index, 4);
    let property_slot = builder.ins().iadd(property_slots, property_offset);
    let initialized = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        property_slot,
        std::mem::offset_of!(php_runtime::api::NativeDeclaredPropertySlot, initialized) as i32,
    );
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        property_slot,
        std::mem::offset_of!(php_runtime::api::NativeDeclaredPropertySlot, value) as i32,
    );
    let initialized = builder.ins().icmp_imm(IntCC::NotEqual, initialized, 0);
    let reference = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    let plain = builder.ins().icmp_imm(IntCC::Equal, reference, 0);
    let authoritative = lower_optimizing_call_value_is_authoritative(builder, value);
    let reference_shape = if requirement.require_reference {
        reference
    } else if requirement.allow_reference {
        builder.ins().iconst(types::I8, 1)
    } else {
        plain
    };
    let mut value_ok = builder.ins().band(reference_shape, authoritative);
    if requirement.readable {
        let uninitialized = builder.ins().icmp_imm(
            IntCC::Equal,
            value,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        let initialized_value = builder.ins().icmp_imm(IntCC::Equal, uninitialized, 0);
        value_ok = builder.ins().band(value_ok, initialized);
        value_ok = builder.ins().band(value_ok, initialized_value);
    }
    let value_ready = builder.create_block();
    builder
        .ins()
        .brif(value_ok, value_ready, &[], rejected, &[]);
    builder.switch_to_block(value_ready);
    if !requirement.probe_paths.is_empty() {
        let (_, length, entries) =
            emit_optimizing_entry_direct_array_descriptor(builder, value, deopt_out, rejected);
        emit_optimizing_entry_array_probe_paths(
            builder,
            &requirement.probe_paths,
            length,
            entries,
            deopt_out,
            rejected,
        );
    }
    if !requirement.mutations.is_empty() {
        emit_optimizing_entry_array_mutations(
            builder,
            &requirement.mutations,
            value,
            insertion_budget,
            deopt_out,
            rejected,
        );
    }
    if requirement.releasable {
        let runtime = lower_is_runtime_handle(builder, value);
        let inspect_runtime = builder.create_block();
        builder.ins().brif(
            runtime,
            inspect_runtime,
            &[],
            accepted,
            &[property_slot.into()],
        );
        builder.switch_to_block(inspect_runtime);
        let value_slot = lower_optimizing_slot_address(builder, value, deopt_out);
        let refcount = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            value_slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
        );
        let shared = builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, refcount, 1);
        builder
            .ins()
            .brif(shared, accepted, &[property_slot.into()], rejected, &[]);
    } else {
        builder.ins().jump(accepted, &[property_slot.into()]);
    }
    builder.switch_to_block(accepted);
    builder.block_params(accepted)[0]
}

fn emit_optimizing_entry_static_property(
    builder: &mut FunctionBuilder<'_>,
    deopt_out: ir::Value,
    function: FunctionId,
    requirement: &NativeEntryStaticPropertyRequirement,
    insertion_budget: usize,
    rejected: ir::Block,
) {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = lower_active_runtime_view(builder, deopt_out);
    let offsets = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(
            crate::JitNativeRuntimeView,
            trusted_property_function_offsets,
        ) as i32,
    );
    let plans = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_static_property_slots) as i32,
    );
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, static_property_slots) as i32,
    );
    let function_entry = builder.ins().iadd_imm(
        offsets,
        i64::try_from(function.index().saturating_mul(4)).unwrap_or(i64::MAX),
    );
    let plan_base = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), function_entry, 0);
    let plan_index = builder
        .ins()
        .iadd_imm(plan_base, i64::from(requirement.continuation_id));
    let wide_plan_index = builder.ins().uextend(pointer_type, plan_index);
    let plan_offset = builder.ins().ishl_imm(wide_plan_index, 3);
    let plan = builder.ins().iadd(plans, plan_offset);
    let state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        plan,
        std::mem::offset_of!(crate::JitNativeTrustedStaticPropertySlot, state) as i32,
    );
    let slot_index = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        plan,
        std::mem::offset_of!(crate::JitNativeTrustedStaticPropertySlot, slot_index) as i32,
    );
    let state = builder
        .ins()
        .band_imm(state, i64::from(requirement.required_state));
    let published =
        builder
            .ins()
            .icmp_imm(IntCC::Equal, state, i64::from(requirement.required_state));
    let slot_count = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, static_property_slot_count) as i32,
    );
    let in_bounds = builder
        .ins()
        .icmp(IntCC::UnsignedLessThan, slot_index, slot_count);
    let admitted = builder.ins().band(published, in_bounds);
    let plan_ready = builder.create_block();
    builder.ins().brif(admitted, plan_ready, &[], rejected, &[]);
    builder.switch_to_block(plan_ready);

    let wide_slot_index = builder.ins().uextend(pointer_type, slot_index);
    let slot_offset = builder.ins().ishl_imm(wide_slot_index, 4);
    let slot = builder.ins().iadd(slots, slot_offset);
    let initialized = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeStaticPropertySlot, initialized) as i32,
    );
    let initialized = builder.ins().icmp_imm(IntCC::NotEqual, initialized, 0);
    let value = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeStaticPropertySlot, value) as i32,
    );
    let authoritative = lower_optimizing_call_value_is_authoritative(builder, value);
    let reference = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
    let plain = builder.ins().icmp_imm(IntCC::Equal, reference, 0);
    let reference_shape = if requirement.require_reference {
        reference
    } else if requirement.allow_reference {
        builder.ins().iconst(types::I8, 1)
    } else {
        plain
    };
    let mut admitted = builder.ins().band(authoritative, reference_shape);
    if requirement.readable {
        admitted = builder.ins().band(admitted, initialized);
    }
    let value_ready = builder.create_block();
    builder
        .ins()
        .brif(admitted, value_ready, &[], rejected, &[]);
    builder.switch_to_block(value_ready);
    if !requirement.probe_paths.is_empty() {
        let (_, length, entries) =
            emit_optimizing_entry_direct_array_descriptor(builder, value, deopt_out, rejected);
        emit_optimizing_entry_array_probe_paths(
            builder,
            &requirement.probe_paths,
            length,
            entries,
            deopt_out,
            rejected,
        );
    }
    if !requirement.mutations.is_empty() {
        emit_optimizing_entry_array_mutations(
            builder,
            &requirement.mutations,
            value,
            insertion_budget,
            deopt_out,
            rejected,
        );
    }
    if requirement.releasable {
        let runtime = lower_is_runtime_handle(builder, value);
        let inspect_runtime = builder.create_block();
        let done = builder.create_block();
        builder.ins().brif(runtime, inspect_runtime, &[], done, &[]);
        builder.switch_to_block(inspect_runtime);
        let value_slot = lower_optimizing_slot_address(builder, value, deopt_out);
        let refcount = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            value_slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
        );
        let shared = builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, refcount, 1);
        builder.ins().brif(shared, done, &[], rejected, &[]);
        builder.switch_to_block(done);
    }
}

fn emit_optimizing_entry_instanceof(
    builder: &mut FunctionBuilder<'_>,
    arguments: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    requirement: NativeEntryInstanceofRequirement,
    rejected: ir::Block,
) -> Result<(), CraneliftLoweringError> {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let Some(parameter_index) = requirement.parameter_index else {
        let view = lower_active_runtime_view(builder, deopt_out);
        let offsets = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            view,
            std::mem::offset_of!(
                crate::JitNativeRuntimeView,
                trusted_property_function_offsets,
            ) as i32,
        );
        let plans = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            view,
            std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_instanceof_plans) as i32,
        );
        let function_entry = builder.ins().iadd_imm(
            offsets,
            i64::try_from(function.index().saturating_mul(4)).unwrap_or(i64::MAX),
        );
        let plan_base = builder
            .ins()
            .load(types::I32, MemFlagsData::new(), function_entry, 0);
        let plan_index = builder
            .ins()
            .iadd_imm(plan_base, i64::from(requirement.continuation_id));
        let wide_plan_index = builder.ins().uextend(pointer_type, plan_index);
        let plan_offset = builder.ins().ishl_imm(wide_plan_index, 4);
        let plan = builder.ins().iadd(plans, plan_offset);
        let state = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            plan,
            std::mem::offset_of!(crate::JitNativeInstanceOfPlan, state) as i32,
        );
        let published = builder.ins().icmp_imm(
            IntCC::Equal,
            state,
            i64::from(crate::JIT_NATIVE_INSTANCEOF_PLAN_PUBLISHED),
        );
        let accepted = builder.create_block();
        builder.ins().brif(published, accepted, &[], rejected, &[]);
        builder.switch_to_block(accepted);
        return Ok(());
    };
    let object = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        arguments,
        i32::try_from(parameter_index.saturating_mul(8)).map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_INSTANCEOF_OFFSET",
                "instanceof parameter offset does not fit the native ABI",
            )
        })?,
    );
    let object_tag = lower_value_has_tag(builder, object, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
    let index = builder.ins().ireduce(types::I32, object);
    let direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let direct_object = builder.ins().band(object_tag, direct);
    let inspect = builder.create_block();
    builder
        .ins()
        .brif(direct_object, inspect, &[], rejected, &[]);
    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, object, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let layout_id = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
    );
    let kind_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT),
    );
    let abi = builder.ins().band_imm(
        flags,
        i64::from(crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_MASK),
    );
    let abi_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        abi,
        i64::from(crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION),
    );
    let shape_ok = builder.ins().band(kind_ok, abi_ok);
    let inspect_plan = builder.create_block();
    builder
        .ins()
        .brif(shape_ok, inspect_plan, &[], rejected, &[]);
    builder.switch_to_block(inspect_plan);

    let view = lower_active_runtime_view(builder, deopt_out);
    let offsets = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(
            crate::JitNativeRuntimeView,
            trusted_property_function_offsets,
        ) as i32,
    );
    let plans = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_instanceof_plans) as i32,
    );
    let entries = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_instanceof_entries) as i32,
    );
    let function_entry = builder.ins().iadd_imm(
        offsets,
        i64::try_from(function.index().saturating_mul(4)).unwrap_or(i64::MAX),
    );
    let plan_base = builder
        .ins()
        .load(types::I32, MemFlagsData::new(), function_entry, 0);
    let plan_index = builder
        .ins()
        .iadd_imm(plan_base, i64::from(requirement.continuation_id));
    let wide_plan_index = builder.ins().uextend(pointer_type, plan_index);
    let plan_offset = builder.ins().ishl_imm(wide_plan_index, 4);
    let plan = builder.ins().iadd(plans, plan_offset);
    let entry_offset = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        plan,
        std::mem::offset_of!(crate::JitNativeInstanceOfPlan, entry_offset) as i32,
    );
    let mask = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        plan,
        std::mem::offset_of!(crate::JitNativeInstanceOfPlan, mask) as i32,
    );
    let state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        plan,
        std::mem::offset_of!(crate::JitNativeInstanceOfPlan, state) as i32,
    );
    let published = builder.ins().icmp_imm(
        IntCC::Equal,
        state,
        i64::from(crate::JIT_NATIVE_INSTANCEOF_PLAN_PUBLISHED),
    );
    let lookup = builder.create_block();
    builder.append_block_param(lookup, types::I32);
    let hash = builder.ins().ushr_imm(layout_id, 30);
    let hash = builder.ins().bxor(layout_id, hash);
    let hash = builder
        .ins()
        .imul_imm(hash, 0xbf58_476d_1ce4_e5b9_u64 as i64);
    let folded = builder.ins().ushr_imm(hash, 27);
    let hash = builder.ins().bxor(hash, folded);
    let hash = builder
        .ins()
        .imul_imm(hash, 0x94d0_49bb_1331_11eb_u64 as i64);
    let folded = builder.ins().ushr_imm(hash, 31);
    let hash = builder.ins().bxor(hash, folded);
    let hash = builder.ins().ireduce(types::I32, hash);
    let bucket = builder.ins().band(hash, mask);
    builder
        .ins()
        .brif(published, lookup, &[bucket.into()], rejected, &[]);
    builder.switch_to_block(lookup);
    let bucket = builder.block_params(lookup)[0];
    let entry_index = builder.ins().iadd(entry_offset, bucket);
    let wide_entry_index = builder.ins().uextend(pointer_type, entry_index);
    let byte_offset = builder.ins().ishl_imm(wide_entry_index, 4);
    let entry = builder.ins().iadd(entries, byte_offset);
    let candidate = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        entry,
        std::mem::offset_of!(crate::JitNativeInstanceOfEntry, layout_id) as i32,
    );
    let hit = builder.ins().icmp(IntCC::Equal, candidate, layout_id);
    let inspect_empty = builder.create_block();
    let accepted = builder.create_block();
    builder.ins().brif(hit, accepted, &[], inspect_empty, &[]);
    builder.switch_to_block(inspect_empty);
    let empty = builder.ins().icmp_imm(IntCC::Equal, candidate, 0);
    let next_bucket = builder.ins().iadd_imm(bucket, 1);
    let next_bucket = builder.ins().band(next_bucket, mask);
    builder
        .ins()
        .brif(empty, rejected, &[], lookup, &[next_bucket.into()]);
    builder.switch_to_block(accepted);
    Ok(())
}

fn emit_optimizing_entry_object_flags(
    builder: &mut FunctionBuilder<'_>,
    arguments: ir::Value,
    deopt_out: ir::Value,
    parameter_index: usize,
    required_flags: u32,
    rejected: ir::Block,
    family: &str,
) -> Result<ir::Value, CraneliftLoweringError> {
    let object = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        arguments,
        i32::try_from(parameter_index.saturating_mul(8)).map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_OBJECT_REQUIREMENT_OFFSET",
                format!("{family} parameter offset does not fit the native ABI"),
            )
        })?,
    );
    let tagged = lower_value_has_tag(builder, object, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
    let index = builder.ins().ireduce(types::I32, object);
    let direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let inspect = builder.create_block();
    let shape = builder.ins().band(tagged, direct);
    builder.ins().brif(shape, inspect, &[], rejected, &[]);
    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, object, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let kind_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT),
    );
    let abi = builder.ins().band_imm(
        flags,
        i64::from(crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_MASK),
    );
    let abi_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        abi,
        i64::from(crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION),
    );
    let published = builder.ins().band(kind_ok, abi_ok);
    let required = builder.ins().band_imm(flags, i64::from(required_flags));
    let required = builder
        .ins()
        .icmp_imm(IntCC::Equal, required, i64::from(required_flags));
    let admitted = builder.ins().band(published, required);
    let next = builder.create_block();
    builder.ins().brif(admitted, next, &[], rejected, &[]);
    builder.switch_to_block(next);
    Ok(flags)
}

fn emit_optimizing_entry_callable(
    builder: &mut FunctionBuilder<'_>,
    arguments: ir::Value,
    deopt_out: ir::Value,
    requirement: NativeEntryCallableRequirement,
    rejected: ir::Block,
) -> Result<(), CraneliftLoweringError> {
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let callable = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        arguments,
        i32::try_from(requirement.parameter_index.saturating_mul(8)).map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_CALLABLE_REQUIREMENT_OFFSET",
                "callable parameter offset does not fit the native ABI",
            )
        })?,
    );
    let tagged = lower_value_has_tag(builder, callable, crate::JIT_VALUE_RUNTIME_CALLABLE_TAG);
    let index = builder.ins().ireduce(types::I32, callable);
    let direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let inspect = builder.create_block();
    let shape = builder.ins().band(tagged, direct);
    builder.ins().brif(shape, inspect, &[], rejected, &[]);
    builder.switch_to_block(inspect);
    let slot = lower_optimizing_slot_address(builder, callable, deopt_out);
    let kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
    );
    let abi = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
    );
    let view = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
    );
    let kind_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        kind,
        i64::from(crate::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE),
    );
    let abi_ok = builder.ins().icmp_imm(
        IntCC::Equal,
        abi,
        i64::from(crate::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION),
    );
    let has_view = builder.ins().icmp_imm(IntCC::NotEqual, view, 0);
    let descriptor_ok = builder.ins().band(kind_ok, abi_ok);
    let descriptor_ok = builder.ins().band(descriptor_ok, has_view);
    let inspect_view = builder.create_block();
    builder
        .ins()
        .brif(descriptor_ok, inspect_view, &[], rejected, &[]);
    builder.switch_to_block(inspect_view);

    let callable_kind = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, kind) as i32,
    );
    let flags = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, flags) as i32,
    );
    let function_id = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, function_id) as i32,
    );
    let captures = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, captures) as i32,
    );
    let capture_count = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, capture_count) as i32,
    );
    let receiver = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, receiver) as i32,
    );
    let implicit_this = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, implicit_this) as i32,
    );
    let name_bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, name_bytes) as i32,
    );
    let name_length = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, name_length) as i32,
    );
    let method_bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, method_bytes) as i32,
    );
    let method_length = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, method_length) as i32,
    );
    let class_bytes = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, class_bytes) as i32,
    );
    let class_length = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativePreparedCallableView, class_length) as i32,
    );
    let fixed = builder.ins().band_imm(
        flags,
        i64::from(crate::JIT_NATIVE_PREPARED_CALLABLE_FIXED_BINDING),
    );
    let fixed = builder.ins().icmp_imm(IntCC::NotEqual, fixed, 0);
    let target = builder
        .ins()
        .icmp_imm(IntCC::NotEqual, function_id, i64::from(u32::MAX));
    let user = builder.ins().icmp_imm(
        IntCC::Equal,
        callable_kind,
        i64::from(crate::JIT_NATIVE_CALLABLE_KIND_USER_FUNCTION),
    );
    let closure = builder.ins().icmp_imm(
        IntCC::Equal,
        callable_kind,
        i64::from(crate::JIT_NATIVE_CALLABLE_KIND_CLOSURE),
    );
    let bound_object = builder.ins().icmp_imm(
        IntCC::Equal,
        callable_kind,
        i64::from(crate::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD),
    );
    let bound_class = builder.ins().icmp_imm(
        IntCC::Equal,
        callable_kind,
        i64::from(crate::JIT_NATIVE_CALLABLE_KIND_BOUND_CLASS_METHOD),
    );
    let callable_kind_ok = builder.ins().bor(user, closure);
    let callable_kind_ok = builder.ins().bor(callable_kind_ok, bound_object);
    let callable_kind_ok = builder.ins().bor(callable_kind_ok, bound_class);
    let allowed_flags = crate::JIT_NATIVE_PREPARED_CLOSURE_HAS_IMPLICIT_THIS
        | crate::JIT_NATIVE_PREPARED_CALLABLE_HAS_SCOPE
        | crate::JIT_NATIVE_PREPARED_CALLABLE_FIXED_BINDING
        | crate::JIT_NATIVE_PREPARED_CALLABLE_HAS_RECEIVER
        | crate::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_INT
        | crate::JIT_NATIVE_PREPARED_CALLABLE_FIRST_PARAMETER_BY_REFERENCE
        | crate::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_RELEASABLE_SCALAR
        | crate::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_STRING;
    let unknown_flags = builder.ins().band_imm(flags, i64::from(!allowed_flags));
    let flags_known = builder.ins().icmp_imm(IntCC::Equal, unknown_flags, 0);
    let no_captures = builder.ins().icmp_imm(IntCC::Equal, capture_count, 0);
    let captures_present = builder.ins().icmp_imm(IntCC::NotEqual, captures, 0);
    let closure_capture_shape = builder.ins().bor(no_captures, captures_present);
    let no_capture_pointer = builder.ins().icmp_imm(IntCC::Equal, captures, 0);
    let nonclosure_capture_shape = builder.ins().band(no_captures, no_capture_pointer);
    let capture_shape =
        builder
            .ins()
            .select(closure, closure_capture_shape, nonclosure_capture_shape);
    let receiver_tag = lower_value_has_tag(builder, receiver, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
    let receiver_index = builder.ins().ireduce(types::I32, receiver);
    let receiver_direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        receiver_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let receiver_shape = builder.ins().band(receiver_tag, receiver_direct);
    let has_receiver = builder.ins().band_imm(
        flags,
        i64::from(crate::JIT_NATIVE_PREPARED_CALLABLE_HAS_RECEIVER),
    );
    let has_receiver = builder.ins().icmp_imm(IntCC::NotEqual, has_receiver, 0);
    let bound_receiver_shape = builder.ins().band(has_receiver, receiver_shape);
    let no_receiver = builder.ins().icmp_imm(IntCC::Equal, has_receiver, 0);
    let receiver_shape = builder
        .ins()
        .select(bound_object, bound_receiver_shape, no_receiver);
    let has_implicit_this = builder.ins().band_imm(
        flags,
        i64::from(crate::JIT_NATIVE_PREPARED_CLOSURE_HAS_IMPLICIT_THIS),
    );
    let has_implicit_this = builder
        .ins()
        .icmp_imm(IntCC::NotEqual, has_implicit_this, 0);
    let implicit_tag =
        lower_value_has_tag(builder, implicit_this, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
    let implicit_index = builder.ins().ireduce(types::I32, implicit_this);
    let implicit_direct = builder.ins().icmp_imm(
        IntCC::UnsignedGreaterThanOrEqual,
        implicit_index,
        i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
    );
    let implicit_object = builder.ins().band(implicit_tag, implicit_direct);
    let no_implicit_this = builder.ins().icmp_imm(IntCC::Equal, implicit_this, 0);
    let closure_implicit_shape =
        builder
            .ins()
            .select(has_implicit_this, implicit_object, no_implicit_this);
    let nonclosure_implicit_shape = no_implicit_this;
    let implicit_shape =
        builder
            .ins()
            .select(closure, closure_implicit_shape, nonclosure_implicit_shape);
    let name_present = builder.ins().icmp_imm(IntCC::NotEqual, name_bytes, 0);
    let name_nonempty = builder.ins().icmp_imm(IntCC::NotEqual, name_length, 0);
    let user_name_shape = builder.ins().band(name_present, name_nonempty);
    let method_present = builder.ins().icmp_imm(IntCC::NotEqual, method_bytes, 0);
    let method_nonempty = builder.ins().icmp_imm(IntCC::NotEqual, method_length, 0);
    let method_shape = builder.ins().band(method_present, method_nonempty);
    let user_or_closure = builder.ins().bor(user, closure);
    let name_shape = builder.ins().select(user, user_name_shape, method_shape);
    let name_shape = builder.ins().select(closure, flags_known, name_shape);
    let class_present = builder.ins().icmp_imm(IntCC::NotEqual, class_bytes, 0);
    let class_nonempty = builder.ins().icmp_imm(IntCC::NotEqual, class_length, 0);
    let bound_class_shape = builder.ins().band(class_present, class_nonempty);
    let class_not_required = builder.ins().iconst(types::I8, 1);
    let class_shape = builder
        .ins()
        .select(bound_class, bound_class_shape, class_not_required);
    let named_kind = builder.ins().bor(user_or_closure, bound_object);
    let named_kind = builder.ins().bor(named_kind, bound_class);
    let name_shape = builder.ins().band(named_kind, name_shape);
    let admitted = builder.ins().band(fixed, target);
    let admitted = builder.ins().band(admitted, callable_kind_ok);
    let admitted = builder.ins().band(admitted, flags_known);
    let admitted = builder.ins().band(admitted, capture_shape);
    let admitted = builder.ins().band(admitted, receiver_shape);
    let admitted = builder.ins().band(admitted, implicit_shape);
    let admitted = builder.ins().band(admitted, name_shape);
    let admitted = builder.ins().band(admitted, class_shape);
    let accepted = builder.create_block();
    builder.ins().brif(admitted, accepted, &[], rejected, &[]);
    builder.switch_to_block(accepted);
    let scan = builder.create_block();
    let inspect_capture = builder.create_block();
    let captures_accepted = builder.create_block();
    builder.append_block_param(scan, types::I32);
    let zero = builder.ins().iconst(types::I32, 0);
    builder.ins().jump(scan, &[zero.into()]);
    builder.switch_to_block(scan);
    let capture_index = builder.block_params(scan)[0];
    let captures_complete = builder.ins().icmp(
        IntCC::UnsignedGreaterThanOrEqual,
        capture_index,
        capture_count,
    );
    builder.ins().brif(
        captures_complete,
        captures_accepted,
        &[],
        inspect_capture,
        &[],
    );
    builder.switch_to_block(inspect_capture);
    let wide_capture_index = builder.ins().uextend(pointer_type, capture_index);
    let capture_offset = builder.ins().ishl_imm(wide_capture_index, 3);
    let capture_pointer = builder.ins().iadd(captures, capture_offset);
    let capture = builder
        .ins()
        .load(types::I64, MemFlagsData::new(), capture_pointer, 0);
    let authoritative = lower_optimizing_call_value_is_authoritative(builder, capture);
    let next_capture = builder.create_block();
    builder
        .ins()
        .brif(authoritative, next_capture, &[], rejected, &[]);
    builder.switch_to_block(next_capture);
    let capture_index = builder.ins().iadd_imm(capture_index, 1);
    builder.ins().jump(scan, &[capture_index.into()]);
    builder.switch_to_block(captures_accepted);
    Ok(())
}

fn emit_optimizing_entry_admission(
    builder: &mut FunctionBuilder<'_>,
    admission: &NativeOptimizingAdmission,
    arguments: ir::Value,
    deopt_out: ir::Value,
    function: FunctionId,
    accepted: ir::Block,
) -> Result<(), CraneliftLoweringError> {
    if admission.array_requirements.is_empty()
        && admission.initialized_request_locals.is_empty()
        && admission.releasable_request_locals.is_empty()
        && admission.initialized_globals.is_empty()
        && admission.releasable_globals.is_empty()
        && admission.plain_globals.is_empty()
        && admission.reference_source_parameters.is_empty()
        && admission.property_requirements.is_empty()
        && admission.static_property_requirements.is_empty()
        && admission.object_layout_requirements.is_empty()
        && admission.instanceof_requirements.is_empty()
        && admission.callable_requirements.is_empty()
        && admission.clone_requirements.is_empty()
        && admission.dynamic_property_requirements.is_empty()
        && admission.exception_requirements.is_empty()
        && admission.value_class_requirements.is_empty()
        && admission.string_requirements.is_empty()
        && admission.resource_type_requirements.is_empty()
        && admission.integer_requirements.is_empty()
        && admission.float_requirements.is_empty()
        && admission.trusted_constant_requirements.is_empty()
        && admission.trusted_static_local_requirements.is_empty()
        && admission.fixed_value_allocations == 0
        && admission.fixed_array_entries == 0
        && admission.fixed_string_bytes == 0
        && !admission.require_non_fiber_scope
    {
        builder.ins().jump(accepted, &[]);
        return Ok(());
    }
    let rejected = builder.create_block();
    builder.set_cold_block(rejected);
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    if admission.require_non_fiber_scope {
        let view = lower_active_runtime_view(builder, deopt_out);
        let scope = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            view,
            std::mem::offset_of!(crate::JitNativeRuntimeView, fiber_execution_scope) as i32,
        );
        let ordinary = builder.ins().icmp_imm(IntCC::Equal, scope, 0);
        let next = builder.create_block();
        builder.ins().brif(ordinary, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for requirement in &admission.object_layout_requirements {
        let object = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            arguments,
            i32::try_from(requirement.parameter_index.saturating_mul(8)).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_METHOD_RECEIVER_OFFSET",
                    "method receiver parameter offset does not fit the native ABI",
                )
            })?,
        );
        let object_tag = lower_value_has_tag(builder, object, crate::JIT_VALUE_RUNTIME_OBJECT_TAG);
        let index = builder.ins().ireduce(types::I32, object);
        let direct = builder.ins().icmp_imm(
            IntCC::UnsignedGreaterThanOrEqual,
            index,
            i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
        );
        let object_shape = builder.ins().band(object_tag, direct);
        let inspect = builder.create_block();
        builder
            .ins()
            .brif(object_shape, inspect, &[], rejected, &[]);
        builder.switch_to_block(inspect);
        let slot = lower_optimizing_slot_address(builder, object, deopt_out);
        let kind = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
        );
        let flags = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
        );
        let layout = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
        );
        let kind_ok = builder.ins().icmp_imm(
            IntCC::Equal,
            kind,
            i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT),
        );
        let abi = builder.ins().band_imm(
            flags,
            i64::from(crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_MASK),
        );
        let abi_ok = builder.ins().icmp_imm(
            IntCC::Equal,
            abi,
            i64::from(crate::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION),
        );
        let layout_ok = builder
            .ins()
            .icmp_imm(IntCC::Equal, layout, requirement.layout_id as i64);
        let accepted_layout = builder.ins().band(kind_ok, abi_ok);
        let accepted_layout = builder.ins().band(accepted_layout, layout_ok);
        let next = builder.create_block();
        builder
            .ins()
            .brif(accepted_layout, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for requirement in admission.callable_requirements.iter().copied() {
        emit_optimizing_entry_callable(builder, arguments, deopt_out, requirement, rejected)?;
    }
    for requirement in admission.clone_requirements.iter().copied() {
        let _ = emit_optimizing_entry_object_flags(
            builder,
            arguments,
            deopt_out,
            requirement.parameter_index,
            0,
            rejected,
            "plain clone",
        )?;
    }
    for requirement in admission.dynamic_property_requirements.iter().copied() {
        let _ = emit_optimizing_entry_object_flags(
            builder,
            arguments,
            deopt_out,
            requirement.parameter_index,
            crate::JIT_NATIVE_OBJECT_STDCLASS,
            rejected,
            "dynamic property",
        )?;
    }
    for requirement in &admission.value_class_requirements {
        let value = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            arguments,
            i32::try_from(requirement.parameter_index.saturating_mul(8)).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_VALUE_CLASS_OFFSET",
                    "value-class parameter offset does not fit the native ABI",
                )
            })?,
        );
        let direct_kind = |builder: &mut FunctionBuilder<'_>, tag: u64, expected_kind: u32| {
            let runtime = lower_is_runtime_handle(builder, value);
            let tag_matches = lower_value_has_tag(builder, value, tag);
            let index = builder.ins().ireduce(types::I32, value);
            let direct = builder.ins().icmp_imm(
                IntCC::UnsignedGreaterThanOrEqual,
                index,
                i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
            );
            let admitted = builder.ins().band(runtime, tag_matches);
            let admitted = builder.ins().band(admitted, direct);
            let safe = builder.ins().iconst(
                types::I64,
                (tag | u64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)) as i64,
            );
            let safe = builder.ins().select(admitted, value, safe);
            let slot = lower_optimizing_slot_address(builder, safe, deopt_out);
            let kind = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
            );
            let kind_ok = builder
                .ins()
                .icmp_imm(IntCC::Equal, kind, i64::from(expected_kind));
            builder.ins().band(admitted, kind_ok)
        };
        let admitted = match requirement.class {
            SsaValueClass::Int => lower_optimizing_integer_candidate(builder, value, deopt_out).0,
            SsaValueClass::StringHandle => {
                lower_native_string_key_descriptor(builder, value, deopt_out).0
            }
            SsaValueClass::Bool => {
                let yes = builder.ins().icmp_imm(
                    IntCC::Equal,
                    value,
                    crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
                );
                let no = builder.ins().icmp_imm(
                    IntCC::Equal,
                    value,
                    crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
                );
                builder.ins().bor(yes, no)
            }
            SsaValueClass::Null => {
                builder
                    .ins()
                    .icmp_imm(IntCC::Equal, value, crate::jit_encode_constant(u32::MAX))
            }
            SsaValueClass::Float => {
                let runtime = lower_is_runtime_handle(builder, value);
                let tag = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_FLOAT_TAG);
                let index = builder.ins().ireduce(types::I32, value);
                let direct = builder.ins().icmp_imm(
                    IntCC::UnsignedGreaterThanOrEqual,
                    index,
                    i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
                );
                let direct_float = builder.ins().band(runtime, tag);
                let direct_float = builder.ins().band(direct_float, direct);
                let first_direct = builder.ins().iconst(
                    types::I64,
                    (crate::JIT_VALUE_RUNTIME_FLOAT_TAG
                        | u64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
                        as i64,
                );
                let safe = builder.ins().select(direct_float, value, first_direct);
                let slot = lower_optimizing_slot_address(builder, safe, deopt_out);
                let kind = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    slot,
                    std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
                );
                let kind_ok = builder.ins().icmp_imm(
                    IntCC::Equal,
                    kind,
                    i64::from(crate::JIT_NATIVE_VALUE_VIEW_FLOAT),
                );
                builder.ins().band(direct_float, kind_ok)
            }
            SsaValueClass::ArrayHandle => direct_kind(
                builder,
                crate::JIT_VALUE_RUNTIME_ARRAY_TAG,
                crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            ),
            SsaValueClass::ObjectHandle => direct_kind(
                builder,
                crate::JIT_VALUE_RUNTIME_OBJECT_TAG,
                crate::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
            ),
            SsaValueClass::CallableHandle => direct_kind(
                builder,
                crate::JIT_VALUE_RUNTIME_CALLABLE_TAG,
                crate::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE,
            ),
            SsaValueClass::ResourceHandle => direct_kind(
                builder,
                crate::JIT_VALUE_RUNTIME_RESOURCE_TAG,
                crate::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE,
            ),
            SsaValueClass::GeneratorHandle => direct_kind(
                builder,
                crate::JIT_VALUE_RUNTIME_GENERATOR_TAG,
                crate::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR,
            ),
            SsaValueClass::FiberHandle => direct_kind(
                builder,
                crate::JIT_VALUE_RUNTIME_FIBER_TAG,
                crate::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER,
            ),
            SsaValueClass::Uninitialized => builder.ins().icmp_imm(
                IntCC::Equal,
                value,
                crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
            ),
            SsaValueClass::ReferenceHandle | SsaValueClass::MixedHandle => {
                unreachable!("reference/mixed entry classes require a dedicated publication plan")
            }
        };
        let next = builder.create_block();
        builder.ins().brif(admitted, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for requirement in &admission.integer_requirements {
        let encoded = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            arguments,
            i32::try_from(requirement.parameter_index.saturating_mul(8)).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_INTEGER_REQUIREMENT_OFFSET",
                    "integer-requirement parameter offset does not fit the native ABI",
                )
            })?,
        );
        let (integer, value) = lower_optimizing_integer_candidate(builder, encoded, deopt_out);
        let minimum =
            builder
                .ins()
                .icmp_imm(IntCC::SignedGreaterThanOrEqual, value, requirement.minimum);
        let maximum =
            builder
                .ins()
                .icmp_imm(IntCC::SignedLessThanOrEqual, value, requirement.maximum);
        let mut admitted = builder.ins().band(integer, minimum);
        admitted = builder.ins().band(admitted, maximum);
        for forbidden in &requirement.forbidden_values {
            let different = builder.ins().icmp_imm(IntCC::NotEqual, value, *forbidden);
            admitted = builder.ins().band(admitted, different);
        }
        let next = builder.create_block();
        builder.ins().brif(admitted, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for requirement in &admission.float_requirements {
        let encoded = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            arguments,
            i32::try_from(requirement.parameter_index.saturating_mul(8)).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_FLOAT_REQUIREMENT_OFFSET",
                    "float-requirement parameter offset does not fit the native ABI",
                )
            })?,
        );
        let slot = lower_optimizing_slot_address(builder, encoded, deopt_out);
        let bits = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
        );
        let value = builder.ins().bitcast(types::F64, MemFlagsData::new(), bits);
        let admitted = if requirement.forbid_zero {
            let zero = builder
                .ins()
                .f64const(cranelift_codegen::ir::immediates::Ieee64::with_bits(0));
            builder.ins().fcmp(FloatCC::NotEqual, value, zero)
        } else {
            builder.ins().iconst(types::I8, 1)
        };
        let next = builder.create_block();
        builder.ins().brif(admitted, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    let mut required_string_bytes = builder.ins().iconst(
        types::I64,
        i64::try_from(admission.fixed_string_bytes).map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_STRING_ADMISSION_BUDGET",
                "fixed string-byte budget does not fit i64",
            )
        })?,
    );
    for requirement in admission.exception_requirements.iter().copied() {
        let prepared = lower_optimizing_prepared_exception_pointer(
            builder,
            function,
            requirement.continuation_id,
            deopt_out,
        );
        let published = builder.ins().icmp_imm(IntCC::NotEqual, prepared, 0);
        let inspect = builder.create_block();
        builder.ins().brif(published, inspect, &[], rejected, &[]);
        builder.switch_to_block(inspect);
        let property_slots = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            prepared,
            std::mem::offset_of!(crate::JitNativePreparedExceptionView, property_slots) as i32,
        );
        let include_function_frame = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            prepared,
            std::mem::offset_of!(
                crate::JitNativePreparedExceptionView,
                include_function_frame
            ) as i32,
        );
        let property_shape = builder.ins().icmp_imm(IntCC::Equal, property_slots, 6);
        let frame_shape = builder.ins().icmp_imm(
            IntCC::Equal,
            include_function_frame,
            i64::from(u32::from(requirement.include_function_frame)),
        );
        let plan_shape = builder.ins().band(property_shape, frame_shape);
        let accepted_plan = builder.create_block();
        builder
            .ins()
            .brif(plan_shape, accepted_plan, &[], rejected, &[]);
        builder.switch_to_block(accepted_plan);
        let fixed = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            prepared,
            std::mem::offset_of!(crate::JitNativePreparedExceptionView, fixed_string_bytes) as i32,
        );
        required_string_bytes = builder.ins().iadd(required_string_bytes, fixed);
    }
    for requirement in admission.resource_type_requirements.iter().copied() {
        let value = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            arguments,
            i32::try_from(requirement.parameter_index.saturating_mul(8)).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_RESOURCE_TYPE_OFFSET",
                    "resource-type parameter offset does not fit the native ABI",
                )
            })?,
        );
        let slot = lower_optimizing_slot_address(builder, value, deopt_out);
        let length = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
        );
        let length = builder.ins().uextend(types::I64, length);
        let within_limit = builder.ins().icmp_imm(
            IntCC::UnsignedLessThanOrEqual,
            length,
            crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY as i64,
        );
        let next = builder.create_block();
        builder.ins().brif(within_limit, next, &[], rejected, &[]);
        builder.switch_to_block(next);
        let minimum = builder.ins().iconst(
            types::I64,
            i64::from(crate::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY),
        );
        let adjusted = builder.ins().umax(length, minimum);
        let below = builder.ins().iadd_imm(adjusted, -1);
        let leading = builder.ins().clz(below);
        let bit_width = builder.ins().iconst(types::I64, 64);
        let width = builder.ins().isub(bit_width, leading);
        let one = builder.ins().iconst(types::I64, 1);
        let capacity = builder.ins().ishl(one, width);
        required_string_bytes = builder.ins().iadd(required_string_bytes, capacity);
    }
    for requirement in admission.string_requirements.iter().copied() {
        let value = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            arguments,
            i32::try_from(requirement.parameter_index.saturating_mul(8)).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_STRING_REQUIREMENT_OFFSET",
                    "string-requirement parameter offset does not fit the native ABI",
                )
            })?,
        );
        let (valid, length, _) = lower_native_string_key_descriptor(builder, value, deopt_out);
        let minimum = builder.ins().icmp_imm(
            IntCC::UnsignedGreaterThanOrEqual,
            length,
            i64::try_from(requirement.minimum_length).unwrap_or(i64::MAX),
        );
        let accepted_string = builder.ins().band(valid, minimum);
        let next = builder.create_block();
        builder
            .ins()
            .brif(accepted_string, next, &[], rejected, &[]);
        builder.switch_to_block(next);
        if let Some(offset) = requirement.offset {
            let offset = match offset {
                NativeEntryStringOffset::Constant(offset) => {
                    builder.ins().iconst(types::I64, offset)
                }
                NativeEntryStringOffset::Parameter(parameter_index) => {
                    let encoded = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        arguments,
                        i32::try_from(parameter_index.saturating_mul(8)).map_err(|_| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_STRING_OFFSET_PARAMETER",
                                "string-offset parameter does not fit the native ABI",
                            )
                        })?,
                    );
                    lower_optimizing_integer_candidate(builder, encoded, deopt_out).1
                }
            };
            let negative = builder.ins().icmp_imm(IntCC::SignedLessThan, offset, 0);
            let magnitude = builder.ins().ineg(offset);
            let negative_in_range =
                builder
                    .ins()
                    .icmp(IntCC::UnsignedLessThanOrEqual, magnitude, length);
            let positive_in_range =
                builder
                    .ins()
                    .icmp(IntCC::UnsignedLessThanOrEqual, offset, length);
            let in_range = builder
                .ins()
                .select(negative, negative_in_range, positive_in_range);
            let next = builder.create_block();
            builder.ins().brif(in_range, next, &[], rejected, &[]);
            builder.switch_to_block(next);
        }
        if requirement.allocation_multiplier != 0 {
            let allocation_length = builder.ins().imul_imm(
                length,
                i64::try_from(requirement.allocation_multiplier).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_STRING_ADMISSION_BUDGET",
                        "string allocation multiplier does not fit i64",
                    )
                })?,
            );
            let within_limit = builder.ins().icmp_imm(
                IntCC::UnsignedLessThanOrEqual,
                allocation_length,
                crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY as i64,
            );
            let next = builder.create_block();
            builder.ins().brif(within_limit, next, &[], rejected, &[]);
            builder.switch_to_block(next);
            let minimum = builder.ins().iconst(
                types::I64,
                i64::from(crate::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY),
            );
            let adjusted = builder.ins().umax(allocation_length, minimum);
            let below = builder.ins().iadd_imm(adjusted, -1);
            let leading = builder.ins().clz(below);
            let bit_width = builder.ins().iconst(types::I64, 64);
            let width = builder.ins().isub(bit_width, leading);
            let one = builder.ins().iconst(types::I64, 1);
            let capacity = builder.ins().ishl(one, width);
            required_string_bytes = builder.ins().iadd(required_string_bytes, capacity);
        }
    }
    for requirement in admission.instanceof_requirements.iter().copied() {
        emit_optimizing_entry_instanceof(
            builder,
            arguments,
            deopt_out,
            function,
            requirement,
            rejected,
        )?;
    }
    for continuation in admission.trusted_constant_requirements.iter().copied() {
        let view = lower_active_runtime_view(builder, deopt_out);
        let offsets = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            view,
            std::mem::offset_of!(
                crate::JitNativeRuntimeView,
                trusted_property_function_offsets,
            ) as i32,
        );
        let plans = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            view,
            std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_constant_slots) as i32,
        );
        let function_entry = builder.ins().iadd_imm(
            offsets,
            i64::try_from(function.index().saturating_mul(4)).unwrap_or(i64::MAX),
        );
        let plan_base = builder
            .ins()
            .load(types::I32, MemFlagsData::new(), function_entry, 0);
        let plan_index = builder.ins().iadd_imm(plan_base, i64::from(continuation));
        let wide_plan_index = builder.ins().uextend(pointer_type, plan_index);
        let plan_offset = builder.ins().ishl_imm(wide_plan_index, 4);
        let plan = builder.ins().iadd(plans, plan_offset);
        let state = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            plan,
            std::mem::offset_of!(crate::JitNativeTrustedConstantSlot, state) as i32,
        );
        let published = builder.ins().icmp_imm(
            IntCC::Equal,
            state,
            i64::from(crate::JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED),
        );
        let next = builder.create_block();
        builder.ins().brif(published, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for continuation in admission.trusted_static_local_requirements.iter().copied() {
        let view = lower_active_runtime_view(builder, deopt_out);
        let offsets = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            view,
            std::mem::offset_of!(
                crate::JitNativeRuntimeView,
                trusted_property_function_offsets,
            ) as i32,
        );
        let plans = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            view,
            std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_static_local_slots) as i32,
        );
        let function_entry = builder.ins().iadd_imm(
            offsets,
            i64::try_from(function.index().saturating_mul(4)).unwrap_or(i64::MAX),
        );
        let plan_base = builder
            .ins()
            .load(types::I32, MemFlagsData::new(), function_entry, 0);
        let plan_index = builder.ins().iadd_imm(plan_base, i64::from(continuation));
        let wide_plan_index = builder.ins().uextend(pointer_type, plan_index);
        let plan_offset = builder.ins().ishl_imm(wide_plan_index, 4);
        let plan = builder.ins().iadd(plans, plan_offset);
        let state = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            plan,
            std::mem::offset_of!(crate::JitNativeTrustedStaticLocalSlot, state) as i32,
        );
        let published = builder.ins().icmp_imm(
            IntCC::Equal,
            state,
            i64::from(crate::JIT_NATIVE_TRUSTED_STATIC_LOCAL_PUBLISHED),
        );
        let next = builder.create_block();
        builder.ins().brif(published, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for local in admission.initialized_request_locals.iter().copied() {
        let reference = lower_trusted_request_local_reference(builder, deopt_out, function, local);
        let slot = lower_optimizing_slot_address(builder, reference, deopt_out);
        let payload = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
        );
        let initialized = builder.ins().icmp_imm(
            IntCC::NotEqual,
            payload,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        let next = builder.create_block();
        builder.ins().brif(initialized, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for continuation_id in admission.initialized_globals.iter().copied() {
        let reference = lower_trusted_global_reference_at_continuation(
            builder,
            function,
            continuation_id,
            deopt_out,
        );
        let slot = lower_optimizing_slot_address(builder, reference, deopt_out);
        let payload = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
        );
        let initialized = builder.ins().icmp_imm(
            IntCC::NotEqual,
            payload,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        let next = builder.create_block();
        builder.ins().brif(initialized, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for continuation_id in admission.plain_globals.iter().copied() {
        let reference = lower_trusted_global_reference_at_continuation(
            builder,
            function,
            continuation_id,
            deopt_out,
        );
        let slot = lower_optimizing_slot_address(builder, reference, deopt_out);
        let payload = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
        );
        let authoritative = lower_optimizing_call_value_is_authoritative(builder, payload);
        let reference =
            lower_value_has_tag(builder, payload, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
        let plain = builder.ins().icmp_imm(IntCC::Equal, reference, 0);
        let admitted = builder.ins().band(authoritative, plain);
        let next = builder.create_block();
        builder.ins().brif(admitted, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for parameter_index in admission.reference_source_parameters.iter().copied() {
        let value = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            arguments,
            i32::try_from(parameter_index.saturating_mul(8)).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_REFERENCE_SOURCE_OFFSET",
                    "reference source parameter offset does not fit the native ABI",
                )
            })?,
        );
        let authoritative = lower_optimizing_call_value_is_authoritative(builder, value);
        let next = builder.create_block();
        builder.ins().brif(authoritative, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for local in admission.releasable_request_locals.iter().copied() {
        let reference = lower_trusted_request_local_reference(builder, deopt_out, function, local);
        let slot = lower_optimizing_slot_address(builder, reference, deopt_out);
        let payload = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
        );
        let inspect_runtime = builder.create_block();
        let next = builder.create_block();
        let runtime = lower_is_runtime_handle(builder, payload);
        builder.ins().brif(runtime, inspect_runtime, &[], next, &[]);
        builder.switch_to_block(inspect_runtime);
        let payload_slot = lower_optimizing_slot_address(builder, payload, deopt_out);
        let refcount = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            payload_slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
        );
        let shared = builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, refcount, 1);
        builder.ins().brif(shared, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for continuation_id in admission.releasable_globals.iter().copied() {
        let reference = lower_trusted_global_reference_at_continuation(
            builder,
            function,
            continuation_id,
            deopt_out,
        );
        let slot = lower_optimizing_slot_address(builder, reference, deopt_out);
        let payload = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
        );
        let inspect_runtime = builder.create_block();
        let next = builder.create_block();
        let runtime = lower_is_runtime_handle(builder, payload);
        builder.ins().brif(runtime, inspect_runtime, &[], next, &[]);
        builder.switch_to_block(inspect_runtime);
        let payload_slot = lower_optimizing_slot_address(builder, payload, deopt_out);
        let refcount = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            payload_slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
        );
        let shared = builder
            .ins()
            .icmp_imm(IntCC::UnsignedGreaterThan, refcount, 1);
        builder.ins().brif(shared, next, &[], rejected, &[]);
        builder.switch_to_block(next);
    }
    for requirement in admission.property_requirements.iter().cloned() {
        let object = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            arguments,
            i32::try_from(requirement.parameter_index.saturating_mul(8)).map_err(|_| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_REJECT_PROPERTY_ADMISSION_OFFSET",
                    "property admission parameter offset does not fit the native ABI",
                )
            })?,
        );
        let _ = emit_optimizing_entry_property_slot(
            builder,
            object,
            deopt_out,
            function,
            requirement,
            admission.fixed_lvalue_insertions,
            rejected,
        );
    }
    for requirement in &admission.static_property_requirements {
        emit_optimizing_entry_static_property(
            builder,
            deopt_out,
            function,
            requirement,
            admission.fixed_lvalue_insertions,
            rejected,
        );
    }
    if admission.array_requirements.is_empty()
        && admission.fixed_value_allocations == 0
        && admission.fixed_array_entries == 0
        && admission.fixed_string_bytes == 0
        && admission.exception_requirements.is_empty()
        && admission
            .string_requirements
            .iter()
            .all(|requirement| requirement.allocation_multiplier == 0)
    {
        builder.ins().jump(accepted, &[]);
        builder.switch_to_block(rejected);
        let retry_baseline = builder.ins().iconst(
            types::I32,
            i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
        );
        builder.ins().return_(&[retry_baseline]);
        return Ok(());
    }
    let mut required_entries = builder.ins().iconst(
        types::I64,
        i64::try_from(admission.fixed_array_entries).map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ARRAY_ADMISSION_BUDGET",
                "fixed array entry budget does not fit i64",
            )
        })?,
    );
    let fixed_value_allocations = admission.fixed_value_allocations.saturating_add(
        admission
            .array_requirements
            .values()
            .map(|requirement| requirement.projection_allocations)
            .sum::<usize>(),
    );
    let mut required_values = builder.ins().iconst(
        types::I64,
        i64::try_from(fixed_value_allocations).map_err(|_| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_ARRAY_ADMISSION_BUDGET",
                "fixed value-slot budget does not fit i64",
            )
        })?,
    );
    for requirement in admission.exception_requirements.iter().copied() {
        let prepared = lower_optimizing_prepared_exception_pointer(
            builder,
            function,
            requirement.continuation_id,
            deopt_out,
        );
        let fixed_values = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            prepared,
            std::mem::offset_of!(crate::JitNativePreparedExceptionView, fixed_value_slots) as i32,
        );
        let fixed_values = builder.ins().uextend(types::I64, fixed_values);
        required_values = builder.ins().iadd(required_values, fixed_values);
        let fixed_entries = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            prepared,
            std::mem::offset_of!(crate::JitNativePreparedExceptionView, fixed_array_entries) as i32,
        );
        let fixed_entries = builder.ins().uextend(types::I64, fixed_entries);
        required_entries = builder.ins().iadd(required_entries, fixed_entries);
        if requirement.include_function_frame {
            let view = lower_active_runtime_view(builder, deopt_out);
            let argument_count = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                view,
                std::mem::offset_of!(crate::JitNativeRuntimeView, active_call_argument_count)
                    as i32,
            );
            let argument_count = builder.ins().uextend(types::I64, argument_count);
            let argument_capacity = lower_optimizing_direct_array_capacity(builder, argument_count);
            required_entries = builder.ins().iadd(required_entries, argument_capacity);
        }
    }
    let mut admitted_array_lengths = BTreeMap::new();
    for (source, requirement) in &admission.array_requirements {
        let inspect = builder.create_block();
        let inspect_slot = builder.create_block();
        let source_accepted = builder.create_block();
        builder.append_block_param(inspect_slot, types::I64);
        builder.append_block_param(source_accepted, types::I64);
        builder.append_block_param(source_accepted, pointer_type);
        let array = lower_entry_array_source(builder, arguments, deopt_out, function, *source)?;
        let tagged = lower_value_has_tag(builder, array, crate::JIT_VALUE_RUNTIME_ARRAY_TAG);
        let encoded_index = builder.ins().ireduce(types::I32, array);
        let direct_index = builder.ins().icmp_imm(
            IntCC::UnsignedGreaterThanOrEqual,
            encoded_index,
            i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
        );
        let direct = builder.ins().band(tagged, direct_index);
        builder.ins().brif(direct, inspect, &[], rejected, &[]);

        builder.switch_to_block(inspect);
        let index = builder.ins().iadd_imm(
            encoded_index,
            -i64::from(crate::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE),
        );
        let in_bounds = builder.ins().icmp_imm(
            IntCC::UnsignedLessThan,
            index,
            crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY as i64,
        );
        builder
            .ins()
            .brif(in_bounds, inspect_slot, &[array.into()], rejected, &[]);

        builder.switch_to_block(inspect_slot);
        let array = builder.block_params(inspect_slot)[0];
        let slot = lower_optimizing_slot_address(builder, array, deopt_out);
        let kind = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, kind) as i32,
        );
        let length = builder.ins().load(
            types::I64,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, payload) as i32,
        );
        let entries = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, aux) as i32,
        );
        let flags = builder.ins().load(
            types::I32,
            MemFlagsData::new(),
            slot,
            std::mem::offset_of!(crate::JitNativeValueSlot, flags) as i32,
        );
        let direct_kind = builder.ins().icmp_imm(
            IntCC::Equal,
            kind,
            i64::from(crate::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY),
        );
        let abi_mask = (1_i64 << crate::JIT_NATIVE_DIRECT_ARRAY_CURSOR_SHIFT) - 1;
        let abi = builder.ins().band_imm(flags, abi_mask);
        let abi_matches = builder.ins().icmp_imm(
            IntCC::Equal,
            abi,
            i64::from(crate::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION),
        );
        let bounded = builder.ins().icmp_imm(
            IntCC::UnsignedLessThanOrEqual,
            length,
            crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
        );
        let admitted = builder.ins().band(direct_kind, abi_matches);
        let admitted = builder.ins().band(admitted, bounded);
        builder.ins().brif(
            admitted,
            source_accepted,
            &[length.into(), entries.into()],
            rejected,
            &[],
        );

        builder.switch_to_block(source_accepted);
        let length = builder.block_params(source_accepted)[0];
        let entries = builder.block_params(source_accepted)[1];
        admitted_array_lengths.insert(*source, length);
        if requirement.minimum_length != 0 {
            let enough = builder.ins().icmp_imm(
                IntCC::UnsignedGreaterThanOrEqual,
                length,
                i64::try_from(requirement.minimum_length).unwrap_or(i64::MAX),
            );
            let next = builder.create_block();
            builder.ins().brif(enough, next, &[], rejected, &[]);
            builder.switch_to_block(next);
        }
        emit_optimizing_entry_array_probe_paths(
            builder,
            &requirement.probe_paths,
            length,
            entries,
            deopt_out,
            rejected,
        );
        for key in &requirement.required_integer_keys {
            let key = builder.ins().iconst(types::I64, *key);
            let (found, _) = lower_optimizing_direct_array_lookup_optional(
                builder, length, entries, key, deopt_out,
            );
            let next = builder.create_block();
            builder.ins().brif(found, next, &[], rejected, &[]);
            builder.switch_to_block(next);
        }
        for (key, type_) in &requirement.required_value_types {
            let key = builder.ins().iconst(types::I64, *key);
            let (found, value) = lower_optimizing_direct_array_lookup_optional(
                builder, length, entries, key, deopt_out,
            );
            let typed = lower_optimizing_type_guard(builder, value, type_, deopt_out)
                .expect("publication admitted only directly guardable array element types");
            let admitted = builder.ins().band(found, typed);
            let next = builder.create_block();
            builder.ins().brif(admitted, next, &[], rejected, &[]);
            builder.switch_to_block(next);
        }
        if requirement.require_supported_keys
            || requirement.require_plain_values
            || requirement.require_key_values
            || requirement.require_string_values
            || requirement.require_scalar_values
            || !requirement.all_value_types.is_empty()
        {
            let scan = builder.create_block();
            let inspect = builder.create_block();
            let finished = builder.create_block();
            builder.append_block_param(scan, types::I64);
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().jump(scan, &[zero.into()]);
            builder.switch_to_block(scan);
            let index = builder.block_params(scan)[0];
            let done = builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
            builder.ins().brif(done, finished, &[], inspect, &[]);
            builder.switch_to_block(inspect);
            let entry =
                lower_optimizing_direct_array_entry_address(builder, entries, index, pointer_type);
            let key = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                entry,
                std::mem::offset_of!(crate::JitNativeDirectArrayEntry, key) as i32,
            );
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                entry,
                std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
            );
            let authoritative = lower_optimizing_call_value_is_authoritative(builder, value);
            let reference =
                lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
            let plain = builder.ins().icmp_imm(IntCC::Equal, reference, 0);
            let mut admitted = authoritative;
            if requirement.require_plain_values {
                admitted = builder.ins().band(admitted, plain);
            }
            if requirement.require_supported_keys {
                let integer = lower_native_array_key_integer_candidate(builder, key, deopt_out).0;
                let string = lower_native_string_key_descriptor(builder, key, deopt_out).0;
                let supported = builder.ins().bor(integer, string);
                admitted = builder.ins().band(admitted, supported);
            }
            if requirement.require_key_values {
                let integer = lower_native_array_key_integer_candidate(builder, value, deopt_out).0;
                let string = lower_native_string_key_descriptor(builder, value, deopt_out).0;
                let key_value = builder.ins().bor(integer, string);
                admitted = builder.ins().band(admitted, key_value);
            }
            if requirement.require_string_values {
                let string = lower_native_string_key_descriptor(builder, value, deopt_out).0;
                admitted = builder.ins().band(admitted, string);
            }
            if requirement.require_scalar_values {
                let integer = lower_optimizing_integer_candidate(builder, value, deopt_out).0;
                let string = lower_native_string_key_descriptor(builder, value, deopt_out).0;
                let float = lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_FLOAT_TAG);
                let true_value = builder.ins().icmp_imm(
                    IntCC::Equal,
                    value,
                    crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
                );
                let false_value = builder.ins().icmp_imm(
                    IntCC::Equal,
                    value,
                    crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
                );
                let null_value = builder.ins().icmp_imm(
                    IntCC::Equal,
                    value,
                    crate::jit_encode_constant(u32::MAX),
                );
                let scalar = builder.ins().bor(integer, string);
                let scalar = builder.ins().bor(scalar, float);
                let scalar = builder.ins().bor(scalar, true_value);
                let scalar = builder.ins().bor(scalar, false_value);
                let scalar = builder.ins().bor(scalar, null_value);
                admitted = builder.ins().band(admitted, scalar);
            }
            for type_ in &requirement.all_value_types {
                let typed = lower_optimizing_type_guard(builder, value, type_, deopt_out)
                    .expect("publication admitted only directly guardable callback array types");
                admitted = builder.ins().band(admitted, typed);
            }
            let next = builder.create_block();
            builder.ins().brif(admitted, next, &[], rejected, &[]);
            builder.switch_to_block(next);
            let next_index = builder.ins().iadd_imm(index, 1);
            builder.ins().jump(scan, &[next_index.into()]);
            builder.switch_to_block(finished);
        }
        for separator_length in &requirement.implode_separator_lengths {
            let scan = builder.create_block();
            let inspect = builder.create_block();
            let finished = builder.create_block();
            builder.append_block_param(scan, types::I64);
            builder.append_block_param(scan, types::I64);
            builder.append_block_param(finished, types::I64);
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().jump(scan, &[zero.into(), zero.into()]);
            builder.switch_to_block(scan);
            let index = builder.block_params(scan)[0];
            let total = builder.block_params(scan)[1];
            let done = builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
            builder
                .ins()
                .brif(done, finished, &[total.into()], inspect, &[]);
            builder.switch_to_block(inspect);
            let entry =
                lower_optimizing_direct_array_entry_address(builder, entries, index, pointer_type);
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                entry,
                std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
            );
            let (_, value_length, _) =
                lower_native_string_key_descriptor(builder, value, deopt_out);
            let first = builder.ins().icmp_imm(IntCC::Equal, index, 0);
            let separator = builder.ins().iconst(
                types::I64,
                i64::try_from(*separator_length).unwrap_or(i64::MAX),
            );
            let separator = builder.ins().select(first, zero, separator);
            let next_total = builder.ins().iadd(total, separator);
            let next_total = builder.ins().iadd(next_total, value_length);
            let next_index = builder.ins().iadd_imm(index, 1);
            builder
                .ins()
                .jump(scan, &[next_index.into(), next_total.into()]);
            builder.switch_to_block(finished);
            let total = builder.block_params(finished)[0];
            let within_limit = builder.ins().icmp_imm(
                IntCC::UnsignedLessThanOrEqual,
                total,
                crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY as i64,
            );
            let next = builder.create_block();
            builder.ins().brif(within_limit, next, &[], rejected, &[]);
            builder.switch_to_block(next);
            let minimum = builder.ins().iconst(
                types::I64,
                i64::from(crate::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY),
            );
            let adjusted = builder.ins().umax(total, minimum);
            let below = builder.ins().iadd_imm(adjusted, -1);
            let leading = builder.ins().clz(below);
            let bit_width = builder.ins().iconst(types::I64, 64);
            let width = builder.ins().isub(bit_width, leading);
            let one = builder.ins().iconst(types::I64, 1);
            let capacity = builder.ins().ishl(one, width);
            required_string_bytes = builder.ins().iadd(required_string_bytes, capacity);
        }
        for unpack in &requirement.unpack_calls {
            let enough = builder.ins().icmp_imm(
                IntCC::UnsignedGreaterThanOrEqual,
                length,
                i64::try_from(unpack.required_length).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_UNPACK_ARITY",
                        "required unpack length does not fit the native ABI",
                    )
                })?,
            );
            let length_accepted = builder.create_block();
            builder
                .ins()
                .brif(enough, length_accepted, &[], rejected, &[]);
            builder.switch_to_block(length_accepted);
            for (index, type_, by_reference) in &unpack.fixed_parameters {
                let entry =
                    builder.ins().iadd_imm(
                        entries,
                        i64::try_from(index.saturating_mul(std::mem::size_of::<
                            crate::JitNativeDirectArrayEntry,
                        >()))
                        .map_err(|_| {
                            CraneliftLoweringError::new(
                                "JIT_CRANELIFT_REJECT_ARRAY_UNPACK_OFFSET",
                                "fixed unpack entry offset does not fit the native ABI",
                            )
                        })?,
                    );
                let key = builder
                    .ins()
                    .load(types::I64, MemFlagsData::new(), entry, 0);
                let integer_key =
                    lower_native_array_key_integer_candidate(builder, key, deopt_out).0;
                let value = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    entry,
                    std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
                );
                let authoritative = lower_optimizing_call_value_is_authoritative(builder, value);
                let reference =
                    lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
                let reference_shape = if *by_reference {
                    reference
                } else {
                    builder.ins().icmp_imm(IntCC::Equal, reference, 0)
                };
                let mut admitted = builder.ins().band(integer_key, authoritative);
                admitted = builder.ins().band(admitted, reference_shape);
                if let Some(type_) = type_ {
                    let typed = lower_optimizing_type_guard(builder, value, type_, deopt_out)
                        .expect("publication admitted only directly guardable unpack types");
                    admitted = builder.ins().band(admitted, typed);
                }
                let next = builder.create_block();
                builder.ins().brif(admitted, next, &[], rejected, &[]);
                builder.switch_to_block(next);
            }
            let scan = builder.create_block();
            let inspect = builder.create_block();
            let finished = builder.create_block();
            builder.append_block_param(scan, types::I64);
            let start = builder.ins().iconst(
                types::I64,
                i64::try_from(unpack.tail_start).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_UNPACK_OFFSET",
                        "variadic unpack start does not fit the native ABI",
                    )
                })?,
            );
            builder.ins().jump(scan, &[start.into()]);
            builder.switch_to_block(scan);
            let index = builder.block_params(scan)[0];
            let done = builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
            builder.ins().brif(done, finished, &[], inspect, &[]);
            builder.switch_to_block(inspect);
            let entry =
                lower_optimizing_direct_array_entry_address(builder, entries, index, pointer_type);
            let key = builder
                .ins()
                .load(types::I64, MemFlagsData::new(), entry, 0);
            let integer_key = lower_native_array_key_integer_candidate(builder, key, deopt_out).0;
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                entry,
                std::mem::offset_of!(crate::JitNativeDirectArrayEntry, value) as i32,
            );
            let authoritative = lower_optimizing_call_value_is_authoritative(builder, value);
            let reference =
                lower_value_has_tag(builder, value, crate::JIT_VALUE_RUNTIME_REFERENCE_TAG);
            let reference_shape = if unpack.tail_by_reference {
                reference
            } else {
                builder.ins().icmp_imm(IntCC::Equal, reference, 0)
            };
            let mut admitted = builder.ins().band(integer_key, authoritative);
            admitted = builder.ins().band(admitted, reference_shape);
            if let Some(type_) = unpack.tail_type.as_ref() {
                let typed = lower_optimizing_type_guard(builder, value, type_, deopt_out)
                    .expect("publication admitted only directly guardable variadic types");
                admitted = builder.ins().band(admitted, typed);
            }
            let next = builder.create_block();
            builder.ins().brif(admitted, next, &[], rejected, &[]);
            builder.switch_to_block(next);
            let next_index = builder.ins().iadd_imm(index, 1);
            builder.ins().jump(scan, &[next_index.into()]);
            builder.switch_to_block(finished);
        }
        for mutation in &requirement.mutations {
            let refcount = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                slot,
                std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
            );
            let unique = builder.ins().icmp_imm(IntCC::Equal, refcount, 1);
            let root_unique = builder.create_block();
            builder.ins().brif(unique, root_unique, &[], rejected, &[]);
            builder.switch_to_block(root_unique);

            let parents = match mutation {
                NativeEntryArrayMutationRequirement::Assign { parents, .. }
                | NativeEntryArrayMutationRequirement::Append { parents }
                | NativeEntryArrayMutationRequirement::Unset { parents, .. }
                | NativeEntryArrayMutationRequirement::Reference { parents, .. }
                | NativeEntryArrayMutationRequirement::ReferenceAppend { parents } => parents,
            };
            let mut current_array = array;
            let mut current_slot = slot;
            let mut current_length = length;
            let mut current_entries = entries;
            for parent in parents {
                let parent = builder.ins().iconst(types::I64, *parent);
                let (found, child) = lower_optimizing_direct_array_lookup_optional(
                    builder,
                    current_length,
                    current_entries,
                    parent,
                    deopt_out,
                );
                let child_found = builder.create_block();
                builder.ins().brif(found, child_found, &[], rejected, &[]);
                builder.switch_to_block(child_found);
                let (child_slot, child_length, child_entries) =
                    emit_optimizing_entry_direct_array_descriptor(
                        builder, child, deopt_out, rejected,
                    );
                let child_refcount = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    child_slot,
                    std::mem::offset_of!(crate::JitNativeValueSlot, refcount) as i32,
                );
                let child_unique = builder.ins().icmp_imm(IntCC::Equal, child_refcount, 1);
                let child_accepted = builder.create_block();
                builder
                    .ins()
                    .brif(child_unique, child_accepted, &[], rejected, &[]);
                builder.switch_to_block(child_accepted);
                current_array = child;
                current_slot = child_slot;
                current_length = child_length;
                current_entries = child_entries;
            }
            let additional = usize::from(!matches!(
                mutation,
                NativeEntryArrayMutationRequirement::Unset { .. }
            ));
            if additional != 0 {
                let reserved = builder.ins().load(
                    types::I32,
                    MemFlagsData::new(),
                    current_slot,
                    std::mem::offset_of!(crate::JitNativeValueSlot, reserved) as i32,
                );
                let reserved = builder.ins().uextend(types::I64, reserved);
                let required = builder.ins().iadd_imm(
                    current_length,
                    i64::try_from(admission.fixed_lvalue_insertions).unwrap_or(i64::MAX),
                );
                let capacity =
                    builder
                        .ins()
                        .icmp(IntCC::UnsignedGreaterThanOrEqual, reserved, required);
                let capacity_accepted = builder.create_block();
                builder
                    .ins()
                    .brif(capacity, capacity_accepted, &[], rejected, &[]);
                builder.switch_to_block(capacity_accepted);
            }
            match mutation {
                NativeEntryArrayMutationRequirement::Append { .. }
                | NativeEntryArrayMutationRequirement::ReferenceAppend { .. } => {
                    let state = lower_direct_array_state_address(builder, current_array, deopt_out);
                    let next_key = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        state,
                        std::mem::offset_of!(crate::JitNativeDirectArrayState, next_append_key,)
                            as i32,
                    );
                    let has_next = builder.ins().load(
                        types::I32,
                        MemFlagsData::new(),
                        state,
                        std::mem::offset_of!(crate::JitNativeDirectArrayState, has_next_append_key,)
                            as i32,
                    );
                    let maximum = builder.ins().icmp_imm(
                        IntCC::SignedGreaterThan,
                        next_key,
                        i64::MAX.saturating_sub(
                            i64::try_from(admission.fixed_lvalue_insertions).unwrap_or(i64::MAX),
                        ),
                    );
                    let has_next = builder.ins().icmp_imm(IntCC::NotEqual, has_next, 0);
                    let overflow = builder.ins().band(maximum, has_next);
                    let safe = builder.ins().icmp_imm(IntCC::Equal, overflow, 0);
                    let next = builder.create_block();
                    builder.ins().brif(safe, next, &[], rejected, &[]);
                    builder.switch_to_block(next);
                }
                NativeEntryArrayMutationRequirement::Assign { key, .. }
                | NativeEntryArrayMutationRequirement::Unset { key, .. }
                | NativeEntryArrayMutationRequirement::Reference { key, .. } => {
                    let key = builder.ins().iconst(types::I64, *key);
                    let (found, old) = lower_optimizing_direct_array_lookup_optional(
                        builder,
                        current_length,
                        current_entries,
                        key,
                        deopt_out,
                    );
                    let runtime = lower_is_runtime_handle(builder, old);
                    let immediate = builder.ins().icmp_imm(IntCC::Equal, runtime, 0);
                    let missing = builder.ins().icmp_imm(IntCC::Equal, found, 0);
                    let safe = builder.ins().bor(missing, immediate);
                    let next = builder.create_block();
                    builder.ins().brif(safe, next, &[], rejected, &[]);
                    builder.switch_to_block(next);
                }
            }
        }
        if requirement.projection_allocations != 0 {
            let capacity = lower_optimizing_direct_array_capacity(builder, length);
            let capacity = builder.ins().imul_imm(
                capacity,
                i64::try_from(requirement.projection_allocations).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_ADMISSION_BUDGET",
                        "array projection count does not fit the native admission ABI",
                    )
                })?,
            );
            required_entries = builder.ins().iadd(required_entries, capacity);
        }
        if requirement.spread_allocations != 0 {
            let capacity = lower_optimizing_direct_array_capacity(builder, length);
            let capacity = builder.ins().imul_imm(
                capacity,
                i64::try_from(requirement.spread_allocations).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_SPREAD_BUDGET",
                        "array spread count does not fit the native admission ABI",
                    )
                })?,
            );
            required_entries = builder.ins().iadd(required_entries, capacity);
        }
        if requirement.value_allocations_per_entry != 0 {
            let values = builder.ins().imul_imm(
                length,
                i64::try_from(requirement.value_allocations_per_entry).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_ADMISSION_BUDGET",
                        "per-entry value-slot budget does not fit the native admission ABI",
                    )
                })?,
            );
            required_values = builder.ins().iadd(required_values, values);
        }
        if requirement.entry_allocations_per_entry != 0 {
            let entries = builder.ins().imul_imm(
                length,
                i64::try_from(requirement.entry_allocations_per_entry).map_err(|_| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_ARRAY_ADMISSION_BUDGET",
                        "per-entry array budget does not fit the native admission ABI",
                    )
                })?,
            );
            required_entries = builder.ins().iadd(required_entries, entries);
        }
    }
    for group in &admission.equal_array_length_groups {
        let Some((first, rest)) = group.split_first() else {
            continue;
        };
        let first_length = admitted_array_lengths[first];
        for source in rest {
            let same =
                builder
                    .ins()
                    .icmp(IntCC::Equal, first_length, admitted_array_lengths[source]);
            let next = builder.create_block();
            builder.ins().brif(same, next, &[], rejected, &[]);
            builder.switch_to_block(next);
        }
    }

    let requires_resource_budget = fixed_value_allocations != 0
        || admission.fixed_array_entries != 0
        || admission.fixed_string_bytes != 0
        || !admission.exception_requirements.is_empty()
        || admission
            .string_requirements
            .iter()
            .any(|requirement| requirement.allocation_multiplier != 0)
        || admission
            .array_requirements
            .values()
            .any(|requirement| !requirement.implode_separator_lengths.is_empty())
        || admission.array_requirements.values().any(|requirement| {
            requirement.value_allocations_per_entry != 0
                || requirement.entry_allocations_per_entry != 0
        });
    if !requires_resource_budget {
        builder.ins().jump(accepted, &[]);
        builder.switch_to_block(rejected);
        let retry_baseline = builder.ins().iconst(
            types::I32,
            i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
        );
        builder.ins().return_(&[retry_baseline]);
        return Ok(());
    }

    let view = lower_active_runtime_view(builder, deopt_out);
    let direct_value_next = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, direct_value_next) as i32,
    );
    let direct_value_next =
        builder
            .ins()
            .load(types::I32, MemFlagsData::new(), direct_value_next, 0);
    let direct_value_next = builder.ins().uextend(types::I64, direct_value_next);
    let value_end = builder.ins().iadd(direct_value_next, required_values);
    let values_fit = builder.ins().icmp_imm(
        IntCC::UnsignedLessThanOrEqual,
        value_end,
        crate::JIT_NATIVE_DIRECT_VALUE_CAPACITY as i64,
    );
    let requires_array_entries = admission.fixed_array_entries != 0
        || !admission.exception_requirements.is_empty()
        || admission.array_requirements.values().any(|requirement| {
            requirement.projection_allocations != 0
                || requirement.spread_allocations != 0
                || requirement.entry_allocations_per_entry != 0
        });
    let arrays_fit = if requires_array_entries {
        let direct_array_next = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            view,
            std::mem::offset_of!(crate::JitNativeRuntimeView, direct_array_next) as i32,
        );
        let direct_array_next =
            builder
                .ins()
                .load(types::I32, MemFlagsData::new(), direct_array_next, 0);
        let direct_array_next = builder.ins().uextend(types::I64, direct_array_next);
        let array_end = builder.ins().iadd(direct_array_next, required_entries);
        builder.ins().icmp_imm(
            IntCC::UnsignedLessThanOrEqual,
            array_end,
            crate::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY as i64,
        )
    } else {
        builder.ins().iconst(types::I8, 1)
    };
    let requires_string_bytes = admission.fixed_string_bytes != 0
        || !admission.exception_requirements.is_empty()
        || admission
            .string_requirements
            .iter()
            .any(|requirement| requirement.allocation_multiplier != 0)
        || admission
            .array_requirements
            .values()
            .any(|requirement| !requirement.implode_separator_lengths.is_empty());
    let strings_fit = if requires_string_bytes {
        let direct_string_next = builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            view,
            std::mem::offset_of!(crate::JitNativeRuntimeView, direct_string_next) as i32,
        );
        let direct_string_next =
            builder
                .ins()
                .load(types::I32, MemFlagsData::new(), direct_string_next, 0);
        let direct_string_next = builder.ins().uextend(types::I64, direct_string_next);
        let string_end = builder
            .ins()
            .iadd(direct_string_next, required_string_bytes);
        builder.ins().icmp_imm(
            IntCC::UnsignedLessThanOrEqual,
            string_end,
            crate::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY as i64,
        )
    } else {
        builder.ins().iconst(types::I8, 1)
    };
    let resources_fit = builder.ins().band(values_fit, arrays_fit);
    let resources_fit = builder.ins().band(resources_fit, strings_fit);
    builder
        .ins()
        .brif(resources_fit, accepted, &[], rejected, &[]);

    builder.switch_to_block(rejected);
    let retry_baseline = builder.ins().iconst(
        types::I32,
        i64::from(crate::JitCallStatus::RECOMPILE_REQUESTED.0),
    );
    builder.ins().return_(&[retry_baseline]);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn define_region_graph_function(
    module: &mut JITModule,
    ctx: &mut cranelift_codegen::Context,
    builder_context: &mut FunctionBuilderContext,
    region: &RegionGraph,
    constants: &[IrConstant],
    value_flow: &ExecutableValueFlow,
    func_id: FuncId,
    functions: &BTreeMap<FunctionId, FuncId>,
    inline_constants: &BTreeMap<FunctionId, BoundedInlineValue>,
    tail_forwards: &BTreeMap<(FunctionId, u32), FunctionId>,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    tier_operations: NativeTierOperations,
    register_liveness: &NativeRegisterLiveness,
    fragment: Option<NativeFragmentDefinition<'_>>,
    unit_identity: u64,
    compilation_mode: crate::cranelift_lowering::baseline_streaming::NativeCompilationMode,
    inline_fragment_entry: bool,
    preflight_only: bool,
) -> Result<DefinedRegionFunction, CraneliftLoweringError> {
    let pointer_type = module.target_config().pointer_type();
    let optimizing_admission = if region.compile_metadata.tier == NativeCompilerTier::Optimizing {
        optimizing_admission_for_region(
            region,
            constants,
            value_flow,
            function_params,
            external_function_signatures,
        )?
    } else {
        NativeOptimizingAdmission::default()
    };
    let mut maximum_temporary_cache_entries = 0_usize;
    let mut production_lowering = Vec::new();
    ctx.func.signature = if fragment.is_some() && !inline_fragment_entry {
        region_fragment_signature(module, region)?
    } else {
        region_graph_signature(module, region)?
    };
    ctx.func.name = UserFuncName::user(0, func_id.as_u32());
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, builder_context);
        let owned_blocks = region
            .blocks
            .iter()
            .filter(|block| {
                fragment.is_none_or(|fragment| fragment.fragment.blocks.contains(&block.id))
            })
            .collect::<Vec<_>>();
        let blocks = if let Some(fragment) = fragment {
            fragment
                .fragment
                .blocks
                .iter()
                .chain(&fragment.fragment.external_targets)
                .map(|block| (*block, builder.create_block()))
                .collect::<BTreeMap<_, _>>()
        } else {
            create_region_cranelift_blocks(&mut builder, region)?
        };
        // An optimizing guard failure transfers once to the matching
        // baseline-native continuation. Baseline code deliberately remains
        // in that tier until the PHP call returns: instruction/block-level
        // ping-pong required two independently computed sparse-live layouts
        // to share a positional ABI and could silently restore one register
        // into another. It also rebuilt transition state at every CFG edge.
        let terminator_blocks = blocks.clone();
        // Only true resumable native transitions need an instruction-entry
        // block. Ordinary Region instructions are lowered directly into their
        // PHP CFG block (or the continuation block created by a fallible
        // helper). Creating an entry block for every instruction turns a
        // large but ordinary PHP function into a pathological Cranelift CFG
        // before regalloc2 sees it.
        let transition_blocks = owned_blocks
            .iter()
            .flat_map(|block| {
                block
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        instruction_has_native_resume_entry(
                            instruction,
                            region.compile_metadata.tier,
                        )
                    })
                    .map(|instruction| instruction.continuation_id)
                    .chain(
                        block_terminator_has_native_transition(block, region.compile_metadata.tier)
                            .then_some(block.terminator_continuation_id),
                    )
            })
            .map(|continuation| (continuation, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let suspension_blocks = owned_blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .filter(|instruction| {
                matches!(instruction.kind, RegionInstructionKind::NativeSuspend(_))
            })
            .map(|instruction| (instruction.continuation_id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let terminal_exit = builder.create_block();
        builder.set_cold_block(terminal_exit);
        builder.append_block_param(terminal_exit, types::I32);
        builder.append_block_param(terminal_exit, types::I64);
        let normal_entry = blocks.values().next().copied().ok_or_else(|| {
            CraneliftLoweringError::new(
                "JIT_CRANELIFT_REJECT_HELPER_CONTROL_FLOW",
                "executable region requires at least one block",
            )
        })?;
        let native_entry = builder.create_block();
        builder.append_block_params_for_function_params(native_entry);
        builder.switch_to_block(native_entry);
        let params = builder.block_params(native_entry).to_vec();
        let runtime = params[0];
        let frame_layout = fragment.map(|fragment| &fragment.layout.frame);
        let fragment_frame = if fragment.is_some() {
            if inline_fragment_entry {
                let frame_bytes = frame_layout
                    .expect("inline fragment frame layout")
                    .frame_bytes()?;
                let frame_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    frame_bytes,
                    3,
                ));
                Some(builder.ins().stack_addr(pointer_type, frame_slot, 0))
            } else {
                Some(params[1])
            }
        } else {
            None
        };
        let streaming_state_frame = if compilation_mode.streams_cfg_state_through_slots() {
            fragment_frame
        } else {
            None
        };
        let (arguments, result_out, deopt_out, resume_id, resume_state, fragment_entry_id) =
            if let Some(frame) = fragment_frame {
                let layout = frame_layout.expect("fragment frame layout");
                let (arguments, result_out, deopt_out, resume_id, resume_state, entry_id) =
                    if inline_fragment_entry {
                        let entry_id = builder.ins().iconst(types::I32, 0);
                        for (value, offset) in [
                            (params[1], layout.arguments_offset()),
                            (params[2], layout.result_out_offset()),
                            (params[3], layout.deopt_out_offset()),
                            (params[5], layout.resume_state_offset()),
                        ] {
                            builder
                                .ins()
                                .store(MemFlagsData::new(), value, frame, offset);
                        }
                        builder.ins().store(
                            MemFlagsData::new(),
                            params[4],
                            frame,
                            layout.resume_id_offset(),
                        );
                        builder.ins().store(
                            MemFlagsData::new(),
                            entry_id,
                            frame,
                            layout.entry_id_offset(),
                        );
                        (
                            params[1], params[2], params[3], params[4], params[5], entry_id,
                        )
                    } else {
                        (
                            builder.ins().load(
                                pointer_type,
                                MemFlagsData::new(),
                                frame,
                                layout.arguments_offset(),
                            ),
                            builder.ins().load(
                                pointer_type,
                                MemFlagsData::new(),
                                frame,
                                layout.result_out_offset(),
                            ),
                            builder.ins().load(
                                pointer_type,
                                MemFlagsData::new(),
                                frame,
                                layout.deopt_out_offset(),
                            ),
                            builder.ins().load(
                                types::I32,
                                MemFlagsData::new(),
                                frame,
                                layout.resume_id_offset(),
                            ),
                            builder.ins().load(
                                pointer_type,
                                MemFlagsData::new(),
                                frame,
                                layout.resume_state_offset(),
                            ),
                            builder.ins().load(
                                types::I32,
                                MemFlagsData::new(),
                                frame,
                                layout.entry_id_offset(),
                            ),
                        )
                    };
                (
                    arguments,
                    result_out,
                    deopt_out,
                    resume_id,
                    resume_state,
                    Some(entry_id),
                )
            } else {
                (params[1], params[2], params[3], params[4], params[5], None)
            };
        if region.compile_metadata.tier == NativeCompilerTier::Baseline
            && (fragment.is_none() || inline_fragment_entry)
        {
            lower_baseline_function_entry(&mut builder, deopt_out, region.function)?;
        }
        let (
            native_call_helper,
            native_dynamic_code_helper,
            mut baseline_operations,
            baseline_value_release_commit,
            execution_poll,
        ) = match tier_operations {
            NativeTierOperations::Baseline {
                call,
                dynamic_code,
                operations,
                value_release_commit,
                ..
            } => {
                let operations =
                    operations
                        .with_runtime(runtime)
                        .with_terminal_exit(NativeTerminalExit {
                            block: terminal_exit,
                        });
                (
                    call.map(|helper| helper.with_runtime(runtime)),
                    dynamic_code.map(|helper| helper.with_runtime(runtime)),
                    Some(operations),
                    Some(module.declare_func_in_func(value_release_commit, builder.func)),
                    operations.execution_poll,
                )
            }
            NativeTierOperations::Optimizing { .. } => {
                let NativeTierOperations::Optimizing { operations } = tier_operations else {
                    unreachable!("optimizing tier was matched above")
                };
                (
                    None,
                    None,
                    None,
                    None,
                    operations
                        .execution_poll
                        .map(|helper| helper.with_runtime(runtime))
                        .map(|helper| {
                            helper.with_terminal_exit(NativeTerminalExit {
                                block: terminal_exit,
                            })
                        }),
                )
            }
        };
        // These guards read the request-owned runtime view directly and only
        // call Rust for reference, warning, destructor, or unsupported dynamic
        // cases. Baseline code needs the same fast paths: forcing every local,
        // scalar comparison, and retain/release through helpers dominated warm
        // execution long after compilation had finished.
        if let Some(native_operations) = baseline_operations.as_mut() {
            native_operations.value_release = native_operations
                .value_release
                .map(NativeHelper::with_inline_runtime_view);
            native_operations.compare = native_operations
                .compare
                .map(NativeHelper::with_inline_runtime_view);
            native_operations.local_fetch = native_operations
                .local_fetch
                .map(NativeHelper::with_inline_runtime_view);
            native_operations.local_store = native_operations
                .local_store
                .map(NativeHelper::with_inline_runtime_view);
            native_operations.truthy = native_operations
                .truthy
                .map(NativeHelper::with_inline_runtime_view);
            native_operations.type_predicate = native_operations
                .type_predicate
                .map(NativeHelper::with_inline_runtime_view);
            native_operations.stable_length = native_operations
                .stable_length
                .map(NativeHelper::with_inline_runtime_view);
            native_operations.array_fetch = native_operations
                .array_fetch
                .map(NativeHelper::with_inline_runtime_view);
            native_operations.foreach_next = native_operations
                .foreach_next
                .map(NativeHelper::with_inline_runtime_view);
            native_operations.string_predicate = native_operations
                .string_predicate
                .map(NativeHelper::with_inline_runtime_view);
        }
        let (arguments, resume_id) = if region.compile_metadata.tier == NativeCompilerTier::Baseline
            && (fragment.is_none() || inline_fragment_entry)
        {
            lower_baseline_bind_packed_arguments(
                module,
                &mut builder,
                baseline_operations
                    .expect("baseline entry requires baseline operations")
                    .argument_check,
                &region.params,
                region
                    .parameter_locals
                    .len()
                    .saturating_sub(region.params.len()),
                arguments,
                result_out,
                deopt_out,
                resume_id,
                region.function,
            )?
        } else {
            (arguments, resume_id)
        };
        let local_ids = fragment.map_or_else(
            || {
                (0..region.local_count)
                    .map(LocalId::new)
                    .collect::<BTreeSet<_>>()
            },
            |fragment| fragment.fragment.locals.clone(),
        );
        let locals = if let Some(frame) = streaming_state_frame {
            let layout = frame_layout.expect("streaming frame layout");
            local_ids
                .into_iter()
                .map(|local| {
                    Ok((
                        local,
                        NativeLocalStorage::FrameSlot {
                            frame,
                            offset: layout.local_offset(local)?,
                        },
                    ))
                })
                .collect::<Result<NativeLocalMap, CraneliftLoweringError>>()?
        } else {
            local_ids
                .into_iter()
                .map(|local| {
                    (
                        local,
                        NativeLocalStorage::Variable(builder.declare_var(types::I64)),
                    )
                })
                .collect::<NativeLocalMap>()
        };
        let streaming_call_exit = streaming_state_frame
            .filter(|_| {
                owned_blocks.iter().any(|block| {
                    block.instructions.iter().any(|instruction| {
                        matches!(instruction.kind, RegionInstructionKind::NativeCall(_))
                    })
                })
            })
            .map(|_| {
                let block = builder.create_block();
                builder.set_cold_block(block);
                builder.append_block_param(block, types::I32);
                builder.append_block_param(block, types::I64);
                builder.append_block_param(block, types::I32);
                builder.append_block_param(block, types::I64);
                for _ in 0..crate::JIT_DEOPT_LOCAL_MASK_WORDS {
                    builder.append_block_param(block, types::I64);
                }
                NativeStreamingCallExit { block }
            });
        let register_types = region_register_types(region);
        let register_live_in = &register_liveness.block_live_in;
        let transition_register_liveness = &register_liveness.transition_live;
        let register_ids = fragment.map_or_else(
            || {
                (0..region.register_count)
                    .map(RegId::new)
                    .collect::<BTreeSet<_>>()
            },
            |fragment| fragment.fragment.registers.clone(),
        );
        let register_variables = register_ids
            .into_iter()
            .map(|register| {
                let type_ = register_types.get(&register).copied().unwrap_or(types::I64);
                let storage = if let Some(frame) = streaming_state_frame {
                    frame_layout
                        .expect("streaming frame layout")
                        .register_offset_if_present(
                            fragment.expect("streaming fragment definition").fragment.id,
                            register,
                        )
                        .map_or(NativeRegisterStorage::Transient { type_ }, |offset| {
                            NativeRegisterStorage::FrameSlot {
                                frame,
                                offset,
                                type_,
                            }
                        })
                } else {
                    NativeRegisterStorage::Variable(builder.declare_var(type_))
                };
                (register, storage)
            })
            .collect::<NativeRegisterMap>();
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
        if let Some(frame) = fragment_frame
            && !inline_fragment_entry
        {
            let status = builder.ins().load(
                types::I32,
                MemFlagsData::new(),
                frame,
                frame_layout
                    .expect("fragment frame layout")
                    .pending_status_offset(),
            );
            let value = builder.ins().load(
                types::I64,
                MemFlagsData::new(),
                frame,
                frame_layout
                    .expect("fragment frame layout")
                    .pending_value_offset(),
            );
            builder.def_var(pending_status, status);
            builder.def_var(pending_value, value);
            if let Some(frame) = streaming_state_frame {
                builder.ins().store(
                    MemFlagsData::new(),
                    status,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .pending_status_offset(),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    value,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .pending_value_offset(),
                );
            }
        } else if let Some(frame) = fragment_frame {
            let layout = frame_layout.expect("inline fragment frame layout");
            builder.ins().store(
                MemFlagsData::new(),
                continue_status,
                frame,
                layout.pending_status_offset(),
            );
            builder.ins().store(
                MemFlagsData::new(),
                empty_value,
                frame,
                layout.pending_value_offset(),
            );
        }
        let uninitialized_value = builder.ins().iconst(
            types::I64,
            crate::jit_encode_constant(crate::JIT_VALUE_UNINITIALIZED),
        );
        for (local, storage) in &locals {
            if let NativeLocalStorage::Variable(variable) = *storage {
                let initial = if matches!(
                    value_flow.local_storage(*local),
                    crate::region_ir::LocalStorageClass::RequestGlobal
                        | crate::region_ir::LocalStorageClass::Superglobal
                ) {
                    lower_trusted_request_local_reference(
                        &mut builder,
                        deopt_out,
                        region.function,
                        *local,
                    )
                } else {
                    uninitialized_value
                };
                builder.def_var(variable, initial);
            }
        }
        if matches!(tier_operations, NativeTierOperations::Optimizing { .. })
            && (fragment.is_none() || inline_fragment_entry)
            && (!optimizing_admission.array_requirements.is_empty()
                || !optimizing_admission.initialized_request_locals.is_empty()
                || !optimizing_admission.releasable_request_locals.is_empty()
                || !optimizing_admission.initialized_globals.is_empty()
                || !optimizing_admission.releasable_globals.is_empty()
                || !optimizing_admission.plain_globals.is_empty()
                || !optimizing_admission.property_requirements.is_empty()
                || optimizing_admission.fixed_value_allocations != 0
                || optimizing_admission.fixed_array_entries != 0
                || optimizing_admission.require_non_fiber_scope)
        {
            let admission = builder.create_block();
            let bind_parameters = builder.create_block();
            let normal_invocation = builder.ins().icmp_imm(IntCC::Equal, resume_id, -1);
            builder
                .ins()
                .brif(normal_invocation, admission, &[], bind_parameters, &[]);
            builder.switch_to_block(admission);
            emit_optimizing_entry_admission(
                &mut builder,
                &optimizing_admission,
                arguments,
                deopt_out,
                region.function,
                bind_parameters,
            )?;
            builder.switch_to_block(bind_parameters);
        }
        if fragment.is_none() {
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
                if value_flow.owns_parameter_at_entry(*param) {
                    lower_optimizing_retain(&mut builder, value, deopt_out);
                }
                define_local_variable(&mut builder, &locals, *param, value)?;
            }
        } else if inline_fragment_entry {
            let frame = fragment_frame.expect("inline fragment frame");
            let layout = frame_layout.expect("inline fragment frame layout");
            for local in layout.local_slots.keys().copied() {
                let initial = if matches!(
                    value_flow.local_storage(local),
                    crate::region_ir::LocalStorageClass::RequestGlobal
                        | crate::region_ir::LocalStorageClass::Superglobal
                ) {
                    lower_trusted_request_local_reference(
                        &mut builder,
                        deopt_out,
                        region.function,
                        local,
                    )
                } else {
                    uninitialized_value
                };
                builder.ins().store(
                    MemFlagsData::new(),
                    initial,
                    frame,
                    layout.local_offset(local)?,
                );
            }
            for (index, local) in region.parameter_locals.iter().enumerate() {
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
                if value_flow.owns_parameter_at_entry(*local) {
                    lower_optimizing_retain(&mut builder, value, deopt_out);
                }
                builder.ins().store(
                    MemFlagsData::new(),
                    value,
                    frame,
                    layout.local_offset(*local)?,
                );
            }
        }
        let handler_resume_blocks = region
            .exception_regions
            .iter()
            .flat_map(|handler| [handler.catch, handler.finally])
            .flatten()
            .filter(|target| {
                fragment.is_none_or(|fragment| fragment.fragment.blocks.contains(target))
            })
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
        let handler_resume_loaders = handler_resume_blocks
            .iter()
            .map(|target| (*target, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let transition_resume_loaders = owned_blocks
            .iter()
            .flat_map(|block| {
                block
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        instruction_has_native_resume_entry(
                            instruction,
                            region.compile_metadata.tier,
                        )
                    })
                    .map(|instruction| instruction.continuation_id)
                    .chain(
                        block_terminator_has_native_transition(block, region.compile_metadata.tier)
                            .then_some(block.terminator_continuation_id),
                    )
            })
            .filter(|continuation| {
                transition_register_liveness
                    .get(continuation)
                    .is_some_and(|registers| registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
            })
            .map(|continuation| (continuation, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let optimizing_block_resume_loaders =
            if region.compile_metadata.tier == NativeCompilerTier::Optimizing {
                {
                    owned_blocks
                        .iter()
                        .map(|block| (block.id, builder.create_block()))
                        .collect::<BTreeMap<_, _>>()
                }
            } else {
                Default::default()
            };
        let osr_entries = region
            .osr_entries()
            .into_iter()
            .filter(|entry| {
                fragment.is_none_or(|fragment| fragment.fragment.blocks.contains(&entry.block))
            })
            .collect::<Vec<_>>();
        let osr_resume_loaders = osr_entries
            .iter()
            .map(|entry| (entry.id, builder.create_block()))
            .collect::<BTreeMap<_, _>>();
        let has_resume_entries = !handler_resume_loaders.is_empty()
            || !suspension_blocks.is_empty()
            || !transition_resume_loaders.is_empty()
            || !optimizing_block_resume_loaders.is_empty()
            || !osr_resume_loaders.is_empty();
        let resume_default = has_resume_entries.then(|| builder.create_block());
        let mut resume_switch = Switch::new();
        let streaming_resume_restore =
            (has_resume_entries && streaming_state_frame.is_some()).then(|| builder.create_block());
        for (target, loader) in &handler_resume_loaders {
            let resume = u128::from(crate::native_handler_resume_id(*target) as u32);
            resume_switch.set_entry(resume, *loader);
        }
        for (continuation, resume_block) in &suspension_blocks {
            let resume = u128::from(crate::native_suspension_resume_id(*continuation) as u32);
            resume_switch.set_entry(resume, *resume_block);
        }
        for (continuation, loader) in &transition_resume_loaders {
            let resume = u128::from(crate::native_transition_resume_id(*continuation) as u32);
            resume_switch.set_entry(resume, *loader);
        }
        for (block, loader) in &optimizing_block_resume_loaders {
            let continuation = region_block_entry_continuation(&region.blocks[block.index()]);
            let resume =
                u128::from(crate::native_optimizing_continuation_resume_id(continuation) as u32);
            resume_switch.set_entry(resume, *loader);
        }
        for (id, loader) in &osr_resume_loaders {
            let resume = u128::from(*id);
            resume_switch.set_entry(resume, *loader);
        }
        let resume_dispatch = if let Some(resume_default) = resume_default {
            let dispatch = builder.create_block();
            builder.set_cold_block(dispatch);
            if let Some(restore) = streaming_resume_restore {
                let is_normal_entry = builder.ins().icmp_imm(IntCC::Equal, resume_id, -1);
                builder
                    .ins()
                    .brif(is_normal_entry, resume_default, &[], restore, &[]);
                builder.switch_to_block(restore);
                builder.set_cold_block(restore);
                let local_restore_done = builder.create_block();
                builder.set_cold_block(local_restore_done);
                emit_streaming_local_restore_loop(
                    &mut builder,
                    pointer_type,
                    resume_state,
                    streaming_state_frame.expect("streaming resume frame"),
                    region.local_count,
                    local_restore_done,
                );
                builder.switch_to_block(local_restore_done);
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
                let frame = streaming_state_frame.expect("streaming resume frame");
                let layout = frame_layout.expect("streaming resume frame layout");
                builder.ins().store(
                    MemFlagsData::new(),
                    control_status,
                    frame,
                    layout.pending_status_offset(),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    control_value,
                    frame,
                    layout.pending_value_offset(),
                );
                builder.ins().jump(dispatch, &[]);
            } else {
                builder.ins().jump(dispatch, &[]);
            }
            Some(dispatch)
        } else {
            None
        };

        for target in handler_resume_blocks {
            let loader = handler_resume_loaders[&target];
            builder.switch_to_block(loader);
            builder.set_cold_block(loader);
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
            if streaming_state_frame.is_none() {
                restore_native_local_state_values(
                    &mut builder,
                    resume_state,
                    &locals,
                    &resume_locals.into_iter().collect::<Vec<_>>(),
                )?;
            }
            // A call-originated throw reaches a handler through the published
            // control value, not through a pre-existing caller local slot.
            // Install that authoritative native throwable directly into the
            // catch local after restoring the caller frame. Restoring the
            // uninitialized snapshot slot here previously replaced every
            // caught Error with NULL.
            if let Some(exception_locals) = handler_exception_locals.get(&target) {
                if matches!(tier_operations, NativeTierOperations::Optimizing { .. }) {
                    return Err(CraneliftLoweringError::new(
                        "JIT_CRANELIFT_REJECT_HANDLER_LVALUE_BOUNDARY",
                        "optimizing catch-local binding was not rejected during entry admission",
                    ));
                } else {
                    for local in exception_locals {
                        let current = use_local_variable(&mut builder, &locals, *local)?;
                        let function = builder
                            .ins()
                            .iconst(types::I64, i64::from(region.function.raw()));
                        let local_value = builder.ins().iconst(types::I64, i64::from(local.raw()));
                        let stored = lower_native_value_operation(
                            module,
                            &mut builder,
                            baseline_operations
                                .expect("baseline catch binding requires baseline operations")
                                .local_store,
                            crate::JIT_LOCAL_STORE_MOVE_INPUT,
                            &[current, value, function, local_value],
                            result_out,
                        )?;
                        define_local_variable(&mut builder, &locals, *local, stored)?;
                    }
                }
            }
            builder.ins().jump(cranelift_block(&blocks, target)?, &[]);
        }
        for region_block in &owned_blocks {
            for instruction in &region_block.instructions {
                if let Some(live_registers) = transition_register_liveness
                    .get(&instruction.continuation_id)
                    .filter(|_| {
                        instruction_has_native_resume_entry(
                            instruction,
                            region.compile_metadata.tier,
                        )
                    })
                    .filter(|registers| registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
                {
                    let loader = transition_resume_loaders[&instruction.continuation_id];
                    builder.switch_to_block(loader);
                    builder.set_cold_block(loader);
                    if let Some(frame) = streaming_state_frame {
                        let fragment = fragment.expect("streaming transition fragment");
                        let layout = frame_layout.expect("streaming transition frame layout");
                        for (snapshot_slot, register) in live_registers.iter().enumerate() {
                            let source_offset =
                                std::mem::offset_of!(crate::JitDeoptState, registers)
                                    .saturating_add(snapshot_slot.saturating_mul(8));
                            let value = builder.ins().load(
                                types::I64,
                                MemFlagsData::new(),
                                resume_state,
                                source_offset as i32,
                            );
                            builder.ins().store(
                                MemFlagsData::new(),
                                value,
                                frame,
                                layout.register_offset(fragment.fragment.id, *register)?,
                            );
                        }
                        builder
                            .ins()
                            .jump(transition_blocks[&instruction.continuation_id], &[]);
                        continue;
                    }
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
                    restore_native_local_state_values(
                        &mut builder,
                        resume_state,
                        &locals,
                        &instruction.live_locals,
                    )?;
                    let mut restored_registers = register_variables.clone();
                    for (snapshot_slot, register) in live_registers.iter().enumerate() {
                        let type_ = register_types.get(register).copied().unwrap_or(types::I64);
                        let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
                            .saturating_add(snapshot_slot.saturating_mul(8));
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
                        define_region_register(
                            &mut builder,
                            &register_variables,
                            &mut restored_registers,
                            *register,
                            value,
                        )?;
                    }
                    builder
                        .ins()
                        .jump(transition_blocks[&instruction.continuation_id], &[]);
                }
            }
        }
        for region_block in &owned_blocks {
            let continuation = region_block.terminator_continuation_id;
            let Some(live_registers) = transition_register_liveness
                .get(&continuation)
                .filter(|_| {
                    block_terminator_has_native_transition(
                        region_block,
                        region.compile_metadata.tier,
                    )
                })
                .filter(|registers| registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS)
            else {
                continue;
            };
            let loader = transition_resume_loaders[&continuation];
            builder.switch_to_block(loader);
            builder.set_cold_block(loader);
            if let Some(frame) = streaming_state_frame {
                let fragment = fragment.expect("streaming terminator transition fragment");
                let layout = frame_layout.expect("streaming terminator transition frame layout");
                for (snapshot_slot, register) in live_registers.iter().enumerate() {
                    let source_offset = std::mem::offset_of!(crate::JitDeoptState, registers)
                        .saturating_add(snapshot_slot.saturating_mul(8));
                    let value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        resume_state,
                        source_offset as i32,
                    );
                    builder.ins().store(
                        MemFlagsData::new(),
                        value,
                        frame,
                        layout.register_offset(fragment.fragment.id, *register)?,
                    );
                }
            } else {
                restore_native_local_state_values(
                    &mut builder,
                    resume_state,
                    &locals,
                    &region_block.terminator_live_locals,
                )?;
                let mut restored_registers = register_variables.clone();
                for (snapshot_slot, register) in live_registers.iter().enumerate() {
                    let type_ = register_types.get(register).copied().unwrap_or(types::I64);
                    let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
                        .saturating_add(snapshot_slot.saturating_mul(8));
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
                    define_region_register(
                        &mut builder,
                        &register_variables,
                        &mut restored_registers,
                        *register,
                        value,
                    )?;
                }
            }
            builder.ins().jump(transition_blocks[&continuation], &[]);
        }
        for (block_id, loader) in &optimizing_block_resume_loaders {
            let target = region.blocks.get(block_id.index()).ok_or_else(|| {
                CraneliftLoweringError::new(
                    "JIT_CRANELIFT_OPTIMIZING_REENTRY_BLOCK",
                    format!("optimizing re-entry block {} is missing", block_id.raw()),
                )
            })?;
            builder.switch_to_block(*loader);
            builder.set_cold_block(*loader);
            restore_native_local_state_values(
                &mut builder,
                resume_state,
                &locals,
                &target.entry_state_locals,
            )?;
            let mut restored_registers = register_variables.clone();
            for (snapshot_slot, register) in register_live_in
                .get(block_id)
                .into_iter()
                .flatten()
                .enumerate()
            {
                let type_ = register_types.get(register).copied().unwrap_or(types::I64);
                let offset = std::mem::offset_of!(crate::JitDeoptState, registers)
                    .saturating_add(snapshot_slot.saturating_mul(8));
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
                define_region_register(
                    &mut builder,
                    &register_variables,
                    &mut restored_registers,
                    *register,
                    value,
                )?;
            }
            builder
                .ins()
                .jump(cranelift_block(&blocks, *block_id)?, &[]);
        }
        for osr_entry in &osr_entries {
            let loader = osr_resume_loaders[&osr_entry.id];
            builder.switch_to_block(loader);
            builder.set_cold_block(loader);
            if streaming_state_frame.is_none() {
                restore_native_local_state_values(
                    &mut builder,
                    resume_state,
                    &locals,
                    &osr_entry.live_locals,
                )?;
            }
            builder
                .ins()
                .jump(cranelift_block(&blocks, osr_entry.block)?, &[]);
        }
        if let Some(resume_default) = resume_default {
            builder.switch_to_block(resume_default);
        }
        if let Some(fragment) = fragment {
            let frame = fragment_frame.expect("fragment signature has a native frame");
            let entry_id = fragment_entry_id.expect("fragment signature has an entry id");
            let invalid_entry = builder.create_block();
            let entry_loaders = fragment
                .fragment
                .normal_entries
                .iter()
                .map(|entry| (*entry, builder.create_block()))
                .collect::<BTreeMap<_, _>>();
            let mut entry_switch = Switch::new();
            for (entry, loader) in &entry_loaders {
                entry_switch.set_entry(u128::from(entry.raw()), *loader);
            }
            entry_switch.emit(&mut builder, entry_id, invalid_entry);
            for entry in &fragment.fragment.normal_entries {
                let loader = entry_loaders[entry];
                builder.switch_to_block(loader);
                let entry_block = region.blocks.get(entry.index()).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_ENTRY",
                        format!("fragment entry block {} is missing", entry.raw()),
                    )
                })?;
                if streaming_state_frame.is_none() {
                    let mut entry_locals = entry_block
                        .entry_state_locals
                        .iter()
                        .copied()
                        .collect::<BTreeSet<_>>();
                    if entry.raw() == 0 {
                        entry_locals.extend(region.parameter_locals.iter().copied());
                    }
                    for local in entry_locals {
                        let value = builder.ins().load(
                            types::I64,
                            MemFlagsData::new(),
                            frame,
                            frame_layout
                                .expect("streaming frame layout")
                                .local_offset(local)?,
                        );
                        define_local_variable(&mut builder, &locals, local, value)?;
                    }
                    let mut restored_registers = register_variables.clone();
                    for register in register_live_in.get(entry).into_iter().flatten() {
                        let type_ = register_types.get(register).copied().unwrap_or(types::I64);
                        let value = builder.ins().load(
                            types::I64,
                            MemFlagsData::new(),
                            frame,
                            frame_layout
                                .expect("optimizing fragment frame layout")
                                .register_offset(fragment.fragment.id, *register)?,
                        );
                        let value = if type_ == types::I64 {
                            value
                        } else {
                            builder.ins().ireduce(type_, value)
                        };
                        define_region_register(
                            &mut builder,
                            &register_variables,
                            &mut restored_registers,
                            *register,
                            value,
                        )?;
                    }
                }
                builder.ins().jump(cranelift_block(&blocks, *entry)?, &[]);
            }
            builder.switch_to_block(invalid_entry);
            builder.set_cold_block(invalid_entry);
            let invalid_entry_marker = builder.ins().iconst(types::I32, 0x4652_4147);
            builder.ins().store(
                MemFlagsData::new(),
                invalid_entry_marker,
                deopt_out,
                std::mem::offset_of!(crate::JitDeoptState, control_reserved) as i32,
            );
            let invalid_entry_value = builder.ins().sextend(types::I64, entry_id);
            builder.ins().store(
                MemFlagsData::new(),
                invalid_entry_value,
                deopt_out,
                std::mem::offset_of!(crate::JitDeoptState, control_value) as i32,
            );
            let invalid = builder
                .ins()
                .iconst(types::I32, i64::from(crate::JitCallStatus::RUNTIME_ERROR.0));
            builder.ins().return_(&[invalid]);
        } else {
            builder.ins().jump(normal_entry, &[]);
        }

        let loop_headers = region
            .osr_entries()
            .into_iter()
            .filter(|entry| {
                fragment.is_none_or(|fragment| fragment.fragment.blocks.contains(&entry.block))
            })
            .map(|entry| entry.block)
            .collect::<BTreeSet<_>>();
        for region_block in &owned_blocks {
            let mut registers = register_variables.clone();
            builder.switch_to_block(cranelift_block(&blocks, region_block.id)?);
            if let Some(frame) = streaming_state_frame {
                for register in register_live_in.get(&region_block.id).into_iter().flatten() {
                    let type_ = register_types.get(register).copied().unwrap_or(types::I64);
                    let value = builder.ins().load(
                        types::I64,
                        MemFlagsData::new(),
                        frame,
                        frame_layout
                            .expect("streaming frame layout")
                            .register_offset(
                                fragment.expect("streaming fragment definition").fragment.id,
                                *register,
                            )?,
                    );
                    let value = if type_ == types::I64 {
                        value
                    } else {
                        builder.ins().ireduce(type_, value)
                    };
                    // One load per real block live-in is cheaper than
                    // reloading the same slot at every operand use. The frame
                    // remains authoritative; this cache is discarded at the
                    // next real CFG boundary.
                    registers.insert(*register, NativeRegisterStorage::Cached(value));
                }
            }
            if loop_headers.contains(&region_block.id)
                && let Some(helper) = execution_poll
            {
                let count_visits = builder.create_block();
                let poll = builder.create_block();
                let continue_execution = builder.create_block();
                let runtime_view = lower_active_runtime_view(&mut builder, deopt_out);
                let counter_address = builder.ins().load(
                    types::I64,
                    MemFlagsData::new(),
                    runtime_view,
                    std::mem::offset_of!(crate::JitNativeRuntimeView, poll_counter) as i32,
                );
                let pointer_type = module.target_config().pointer_type();
                let counter_address = if pointer_type == types::I64 {
                    counter_address
                } else {
                    builder.ins().ireduce(pointer_type, counter_address)
                };
                let counter_available = builder.ins().icmp_imm(IntCC::NotEqual, counter_address, 0);
                builder
                    .ins()
                    .brif(counter_available, count_visits, &[], poll, &[]);

                builder.switch_to_block(count_visits);
                let counter =
                    builder
                        .ins()
                        .load(types::I32, MemFlagsData::new(), counter_address, 0);
                let counter = builder.ins().iadd_imm(counter, 1);
                let counter = builder.ins().band_imm(counter, 4095);
                builder
                    .ins()
                    .store(MemFlagsData::new(), counter, counter_address, 0);
                let deadline_check = builder.ins().icmp_imm(IntCC::Equal, counter, 0);
                builder
                    .ins()
                    .brif(deadline_check, poll, &[], continue_execution, &[]);

                builder.switch_to_block(poll);
                let call = call_native_helper(module, &mut builder, helper, &[]);
                let status = builder.inst_results(call)[0];
                require_native_operation_ok(&mut builder, status, helper.terminal_exit()?)?;
                builder.ins().jump(continue_execution, &[]);
                builder.switch_to_block(continue_execution);
            }
            let mut terminated = false;
            for instruction in &region_block.instructions {
                let transition_block = transition_blocks.get(&instruction.continuation_id).copied();
                if let Some(transition_block) = transition_block {
                    builder.ins().jump(transition_block, &[]);
                    builder.switch_to_block(transition_block);
                    // A resume loader may enter this instruction without
                    // executing earlier instructions in the Region block.
                    // The compact frame is authoritative at that boundary;
                    // block-local cached SSA values would not dominate the
                    // resume edge.
                    if streaming_state_frame.is_some() {
                        registers = register_variables.clone();
                    }
                }
                builder.set_srcloc(ir::SourceLoc::new(
                    instruction.continuation_id.saturating_add(1),
                ));
                if let Some(target) = tail_forwards
                    .get(&(region.function, instruction.continuation_id))
                    .and_then(|target| functions.get(target))
                {
                    let callee = module.declare_func_in_func(*target, builder.func);
                    builder.ins().return_call(
                        callee,
                        &[
                            runtime,
                            arguments,
                            result_out,
                            deopt_out,
                            resume_id,
                            resume_state,
                        ],
                    );
                    terminated = true;
                    break;
                }
                let transition_live_registers = transition_register_liveness
                    .get(&instruction.continuation_id)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                match tier_operations {
                    NativeTierOperations::Optimizing { operations } => {
                        lower_optimizing_region_instruction(
                            module,
                            &mut builder,
                            &register_variables,
                            &suspension_blocks,
                            &blocks,
                            &locals,
                            &mut registers,
                            instruction,
                            transition_live_registers,
                            optimizing_admission.array_call_is_total(instruction.continuation_id)
                                || optimizing_admission
                                    .fixed_builtin_call_is_total(instruction.continuation_id),
                            optimizing_admission
                                .array_instruction_is_total(instruction.continuation_id),
                            optimizing_admission
                                .binary_instruction_is_total(instruction.continuation_id),
                            optimizing_admission
                                .scalar_control_instruction_is_total(instruction.continuation_id),
                            optimizing_admission
                                .array_instruction_is_fresh(instruction.continuation_id),
                            optimizing_admission.local_load_is_total(instruction.continuation_id),
                            optimizing_admission
                                .request_local_store_is_total(instruction.continuation_id),
                            optimizing_admission
                                .return_reference_store_is_total(instruction.continuation_id),
                            constants,
                            value_flow,
                            inline_constants,
                            function_params,
                            external_function_signatures,
                            runtime,
                            result_out,
                            deopt_out,
                            resume_state,
                            pending_status,
                            pending_value,
                            region.function,
                            region.local_count,
                            region.flags.is_top_level,
                            native_version,
                            unit_identity,
                            operations.with_runtime(runtime),
                        )
                        .map(|emitted| {
                            production_lowering.push(crate::JitProductionLoweringMetadata {
                                function: region.function,
                                continuation_id: instruction.continuation_id,
                                operation: crate::region_ir::baseline_instruction_lowering(
                                    &instruction.source_kind,
                                )
                                .variant
                                .to_owned(),
                                class: emitted.class,
                            });
                        })
                    }
                    NativeTierOperations::Baseline { .. } => lower_baseline_region_instruction(
                        module,
                        &mut builder,
                        functions,
                        inline_constants,
                        function_params,
                        external_function_signatures,
                        native_call_helper,
                        native_dynamic_code_helper,
                        baseline_operations
                            .expect("baseline instruction requires baseline operations"),
                        baseline_value_release_commit,
                        &register_variables,
                        &blocks,
                        &suspension_blocks,
                        &locals,
                        &mut registers,
                        region_block.source_block,
                        instruction,
                        transition_live_registers,
                        constants,
                        value_flow,
                        streaming_call_exit,
                        result_out,
                        deopt_out,
                        resume_state,
                        pending_status,
                        pending_value,
                        region.function,
                        region.return_type.is_some(),
                        region.local_count,
                        native_version,
                        region.flags.is_top_level,
                        &region.locals,
                        unit_identity,
                        pointer_type,
                    ),
                }
                .map_err(|error| {
                    CraneliftLoweringError::new(
                        error.code,
                        format!(
                            "{} in Region block {} continuation {} ({:?})",
                            error.detail,
                            region_block.id.raw(),
                            instruction.continuation_id,
                            instruction.source_kind,
                        ),
                    )
                })?;
                maximum_temporary_cache_entries = maximum_temporary_cache_entries.max(
                    registers
                        .values()
                        .filter(|storage| matches!(storage, NativeRegisterStorage::Cached(_)))
                        .count(),
                );
                if matches!(instruction.kind, RegionInstructionKind::RuntimeFatal { .. }) {
                    terminated = true;
                    break;
                }
            }
            if terminated {
                continue;
            }
            if let Some(transition_block) =
                transition_blocks.get(&region_block.terminator_continuation_id)
            {
                builder.ins().jump(*transition_block, &[]);
                builder.switch_to_block(*transition_block);
                // The normal edge and a resume loader both enter this block.
                // Values cached while lowering the normal predecessor do not
                // dominate the resume edge; the compact frame does.
                if streaming_state_frame.is_some() {
                    registers = register_variables.clone();
                }
            }
            builder.set_srcloc(ir::SourceLoc::new(
                region_block.terminator_continuation_id.saturating_add(1),
            ));
            // Streaming definitions store through to every externally live
            // frame slot immediately. Re-emitting all successor live-ins here
            // duplicated stores on every CFG edge and inflated both baseline
            // code and execution traffic; successor blocks already reload the
            // authoritative slots above.
            match tier_operations {
                NativeTierOperations::Optimizing { operations } => {
                    let value_release_commit =
                        module.declare_func_in_func(operations.value_release_commit, builder.func);
                    debug_assert!(
                        optimizing_admission
                            .terminator_is_total(region_block.terminator_continuation_id),
                        "optimizing terminators are total before lowering"
                    );
                    lower_optimizing_region_terminator(
                        &mut builder,
                        &blocks,
                        &locals,
                        &registers,
                        result_out,
                        deopt_out,
                        value_release_commit,
                        optimizing_admission.return_plan(region_block.terminator_continuation_id),
                        match region_block.terminator {
                            RegionTerminator::ReturnReference { local, .. } => {
                                optimizing_admission.return_reference_is_prebound(local)
                                    || region.params.iter().any(|parameter| {
                                        parameter.local == local && parameter.by_ref
                                    })
                            }
                            _ => false,
                        },
                        &region_block.terminator,
                        constants,
                        value_flow,
                    )
                    .map(|emitted| {
                        production_lowering.push(crate::JitProductionLoweringMetadata {
                            function: region.function,
                            continuation_id: region_block.terminator_continuation_id,
                            operation: crate::region_ir::baseline_terminator_lowering(
                                &region_block.source_terminator,
                            )
                            .variant
                            .to_owned(),
                            class: emitted.class,
                        });
                    })
                }
                NativeTierOperations::Baseline { .. } => lower_region_terminator(
                    &mut builder,
                    &terminator_blocks,
                    &locals,
                    &registers,
                    result_out,
                    deopt_out,
                    pending_status,
                    pending_value,
                    module,
                    baseline_operations.expect("baseline terminator requires baseline operations"),
                    region.function,
                    region.local_count,
                    region_block.terminator_continuation_id,
                    native_version,
                    region.return_type.is_some(),
                    &region_block.terminator,
                    constants,
                    value_flow,
                ),
            }?;
        }
        if let Some(fragment) = fragment {
            let frame = fragment_frame.expect("fragment signature has a native frame");
            for target in &fragment.fragment.external_targets {
                builder.switch_to_block(cranelift_block(&blocks, *target)?);
                let target_block = region.blocks.get(target.index()).ok_or_else(|| {
                    CraneliftLoweringError::new(
                        "JIT_CRANELIFT_FRAGMENT_EXIT_TARGET",
                        format!("fragment exit target {} is missing", target.raw()),
                    )
                })?;
                if streaming_state_frame.is_none() {
                    for local in &target_block.entry_state_locals {
                        let value = use_local_variable(&mut builder, &locals, *local)?;
                        builder.ins().store(
                            MemFlagsData::new(),
                            value,
                            frame,
                            frame_layout
                                .expect("fragment frame layout")
                                .local_offset(*local)?,
                        );
                    }
                }
                if streaming_state_frame.is_none() {
                    for register in register_live_in.get(target).into_iter().flatten() {
                        let value =
                            use_region_register(&mut builder, &register_variables, *register)?;
                        let value = if builder.func.dfg.value_type(value) == types::I64 {
                            value
                        } else {
                            builder.ins().uextend(types::I64, value)
                        };
                        builder.ins().store(
                            MemFlagsData::new(),
                            value,
                            frame,
                            frame_layout
                                .expect("fragment frame layout")
                                .register_offset(fragment.fragment.id, *register)?,
                        );
                    }
                }
                let status = builder.use_var(pending_status);
                let value = builder.use_var(pending_value);
                builder.ins().store(
                    MemFlagsData::new(),
                    status,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .pending_status_offset(),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    value,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .pending_value_offset(),
                );
                let target_fragment = fragment.layout.block_owner[target];
                let callee =
                    module.declare_func_in_func(fragment.functions[&target_fragment], builder.func);
                let no_resume = builder.ins().iconst(types::I32, -1);
                let entry = builder.ins().iconst(types::I32, i64::from(target.raw()));
                builder.ins().store(
                    MemFlagsData::new(),
                    entry,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .entry_id_offset(),
                );
                builder.ins().store(
                    MemFlagsData::new(),
                    no_resume,
                    frame,
                    frame_layout
                        .expect("fragment frame layout")
                        .resume_id_offset(),
                );
                builder.ins().return_call(callee, &[runtime, frame]);
            }
        }
        if let (Some(streaming_call_exit), Some(frame)) =
            (streaming_call_exit, streaming_state_frame)
        {
            builder.switch_to_block(streaming_call_exit.block);
            let params = builder.block_params(streaming_call_exit.block).to_vec();
            let status = params[0];
            let value = params[1];
            let continuation = params[2];
            let suspension_link = params[3];
            let store_i32 = |builder: &mut FunctionBuilder<'_>, offset: usize, value: ir::Value| {
                builder
                    .ins()
                    .store(MemFlagsData::new(), value, deopt_out, offset as i32);
            };
            let function_id = builder
                .ins()
                .iconst(types::I32, i64::from(region.function.raw()));
            let slot_count = builder
                .ins()
                .iconst(types::I32, i64::from(region.local_count));
            let native_version_value = builder.ins().iconst(types::I32, i64::from(native_version));
            store_i32(
                &mut builder,
                std::mem::offset_of!(crate::JitDeoptState, function_id),
                function_id,
            );
            store_i32(
                &mut builder,
                std::mem::offset_of!(crate::JitDeoptState, continuation_id),
                continuation,
            );
            store_i32(
                &mut builder,
                std::mem::offset_of!(crate::JitDeoptState, slot_count),
                slot_count,
            );
            store_i32(
                &mut builder,
                std::mem::offset_of!(crate::JitDeoptState, native_version),
                native_version_value,
            );
            for (word, mask) in params[4..].iter().copied().enumerate() {
                builder.ins().store(
                    MemFlagsData::new(),
                    mask,
                    deopt_out,
                    std::mem::offset_of!(crate::JitDeoptState, initialized_mask)
                        .saturating_add(word.saturating_mul(8)) as i32,
                );
            }
            builder
                .ins()
                .store(MemFlagsData::new(), value, result_out, 0);
            publish_native_fiber_suspension_link(&mut builder, deopt_out, suspension_link);
            let finished = builder.create_block();
            builder.set_cold_block(finished);
            emit_streaming_local_snapshot_loop(
                &mut builder,
                pointer_type,
                deopt_out,
                frame,
                region.local_count,
                finished,
            );
            builder.switch_to_block(finished);
            builder.ins().return_(&[status]);
        }
        if let (Some(dispatch), Some(resume_default)) = (resume_dispatch, resume_default) {
            builder.switch_to_block(dispatch);
            resume_switch.emit(&mut builder, resume_id, resume_default);
        }
        builder.switch_to_block(terminal_exit);
        let terminal_status = builder.block_params(terminal_exit)[0];
        let terminal_value = builder.block_params(terminal_exit)[1];
        builder
            .ins()
            .store(MemFlagsData::new(), terminal_value, result_out, 0);
        builder.ins().return_(&[terminal_status]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    let mut pre_regalloc = match validate_pre_regalloc_structure(
        &ctx.func,
        region,
        fragment.map(|fragment| fragment.fragment.id),
    ) {
        Ok(metrics) => metrics,
        Err(error) => {
            module.clear_context(ctx);
            return Err(error);
        }
    };
    let source_instructions = fragment.map_or_else(
        || {
            region
                .blocks
                .iter()
                .map(|block| block.instructions.len())
                .sum()
        },
        |fragment| {
            fragment
                .fragment
                .blocks
                .iter()
                .map(|block| region.blocks[block.index()].instructions.len())
                .sum::<usize>()
        },
    );
    if source_instructions != 0 {
        pre_regalloc.loads_per_source_instruction_milli = pre_regalloc
            .loads
            .saturating_mul(1_000)
            .div_ceil(source_instructions);
        pre_regalloc.stores_per_source_instruction_milli = pre_regalloc
            .stores
            .saturating_mul(1_000)
            .div_ceil(source_instructions);
    }
    let verifier_flags = settings::Flags::new(settings::builder());
    if let Err(error) = verify_function(&ctx.func, &verifier_flags) {
        let error = CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_VERIFIER",
            format!("Cranelift verifier rejected executable Region IR: {error}"),
        );
        module.clear_context(ctx);
        return Err(error);
    }
    let clif_blocks = ctx.func.layout.blocks().count();
    if preflight_only {
        let lowered_function = std::mem::replace(&mut ctx.func, ir::Function::new());
        module.clear_context(ctx);
        return Ok(DefinedRegionFunction {
            lowered_function: Some(lowered_function),
            code: Vec::new(),
            clif_blocks,
            alignment: 1,
            relocations: Vec::new(),
            native_pc_ranges: Vec::new(),
            native_stack_bytes: 0,
            pre_regalloc,
            maximum_temporary_cache_entries,
            production_lowering,
        });
    }
    if let Err(error) = module.define_function(func_id, ctx) {
        let error = CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_DEFINE",
            format!("failed to define native function: {error}"),
        );
        module.clear_context(ctx);
        return Err(error);
    }
    let compiled = ctx.compiled_code().ok_or_else(|| {
        CraneliftLoweringError::new(
            "JIT_CRANELIFT_REJECT_CACHE_CODE",
            "Cranelift returned no compiled machine-code buffer",
        )
    })?;
    let native_stack_bytes = compiled
        .buffer
        .frame_layout()
        .map_or(0, |layout| layout.frame_to_fp_offset);
    if native_stack_bytes > MAX_NATIVE_SPILL_FRAME_BYTES {
        return Err(CraneliftLoweringError::new(
            "JIT_CRANELIFT_NATIVE_STACK_LIMIT",
            format!(
                "function {} requires {native_stack_bytes} native stack bytes; limit is {MAX_NATIVE_SPILL_FRAME_BYTES}",
                region.function_name
            ),
        ));
    }
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
    module.clear_context(ctx);
    Ok(DefinedRegionFunction {
        lowered_function: None,
        code,
        clif_blocks,
        alignment,
        relocations,
        native_pc_ranges,
        native_stack_bytes,
        pre_regalloc,
        maximum_temporary_cache_entries,
        production_lowering,
    })
}

include!("executable_region/direct_value_support.rs");

#[allow(clippy::too_many_arguments)]
fn region_graph_metadata<'a>(
    root: FunctionId,
    root_local_count: u32,
    regions: impl Iterator<Item = &'a RegionGraph>,
    native_pc_ranges: Vec<crate::JitNativePcRange>,
    function_entries: Vec<crate::JitNativeFunctionEntryMetadata>,
    root_register_liveness: Option<&NativeRegisterLiveness>,
    value_flows: &BTreeMap<FunctionId, ExecutableValueFlow>,
    mut emitted_production_lowering: Vec<crate::JitProductionLoweringMetadata>,
) -> crate::JitRegionStateMetadata {
    let regions = regions.collect::<Vec<_>>();
    emitted_production_lowering.sort_by_key(|entry| (entry.function, entry.continuation_id));
    emitted_production_lowering.dedup_by_key(|entry| (entry.function, entry.continuation_id));
    let transition_liveness = regions
        .iter()
        .map(|region| {
            let liveness = root_register_liveness
                .filter(|_| region.function == root)
                .map_or_else(
                    || NativeRegisterLiveness::analyze(region).transition_live,
                    |liveness| liveness.transition_live.clone(),
                );
            (region.function, liveness)
        })
        .collect::<BTreeMap<_, _>>();
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
    let root_direct_call_sites = function_entries
        .iter()
        .find(|entry| entry.function == root)
        .map_or(0, |entry| entry.direct_call_sites);
    let root_direct_method_call_sites = function_entries
        .iter()
        .find(|entry| entry.function == root)
        .map_or(0, |entry| entry.direct_method_call_sites);
    let root_inlining = function_entries
        .iter()
        .find(|entry| entry.function == root)
        .map(|entry| {
            (
                entry.inlined_call_sites,
                entry.inline_bytes_added,
                entry.tail_call_sites,
                entry.inline_rejected_by_reason.clone(),
            )
        })
        .unwrap_or_default();
    let direct_callees = regions
        .iter()
        .flat_map(|region| region.direct_callees())
        .collect::<BTreeSet<_>>()
        .into_iter()
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
        compiled_to_compiled_call_sites: root_direct_call_sites,
        compiled_to_compiled_method_call_sites: root_direct_method_call_sites,
        inlined_call_sites: root_inlining.0,
        inline_bytes_added: root_inlining.1,
        tail_call_sites: root_inlining.2,
        inline_rejected_by_reason: root_inlining.3,
        direct_callees,
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
                let liveness = &transition_liveness[&region.function];
                let value_flow = &value_flows[&region.function];
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
                        let live_registers = liveness
                            .get(&instruction.continuation_id)
                            .cloned()
                            .unwrap_or_default();
                        let owned_locals = instruction
                            .live_locals
                            .iter()
                            .copied()
                            .filter(|local| {
                                value_flow.local_storage(*local).is_native_frame_local()
                            })
                            .collect();
                        let owned_registers = live_registers
                            .iter()
                            .copied()
                            .filter(|register| {
                                crate::region_ir::value_release_required(
                                    value_flow.register_fact(*register),
                                )
                            })
                            .collect();
                        Some(crate::JitNativeSuspensionMetadata {
                            function: region.function,
                            continuation_id: instruction.continuation_id,
                            resume_id: crate::native_suspension_resume_id(
                                instruction.continuation_id,
                            ),
                            kind,
                            span: instruction.span,
                            live_locals: instruction.live_locals.clone(),
                            owned_locals,
                            live_registers,
                            owned_registers,
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
                let liveness = &transition_liveness[&region.function];
                let value_flow = &value_flows[&region.function];
                let mut transitions = region
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .filter_map(|instruction| {
                        if !instruction_has_native_transition(
                            instruction,
                            region.compile_metadata.tier,
                        ) {
                            return None;
                        }
                        let live_registers = liveness.get(&instruction.continuation_id)?;
                        (live_registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS).then(|| {
                            let owned_locals = instruction
                                .live_locals
                                .iter()
                                .copied()
                                .filter(|local| {
                                    value_flow.local_storage(*local).is_native_frame_local()
                                })
                                .collect();
                            let owned_registers = live_registers
                                .iter()
                                .copied()
                                .filter(|register| {
                                    crate::region_ir::value_release_required(
                                        value_flow.register_fact(*register),
                                    )
                                })
                                .collect();
                            crate::JitNativeTransitionMetadata {
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
                                owned_locals,
                                owned_registers,
                                result_register: region_instruction_result_register(
                                    &instruction.kind,
                                ),
                            }
                        })
                    })
                    .collect::<Vec<_>>();
                transitions.extend(region.blocks.iter().filter_map(|block| {
                    if !block_terminator_has_native_transition(block, region.compile_metadata.tier)
                    {
                        return None;
                    }
                    let continuation_id = block.terminator_continuation_id;
                    let live_registers = liveness.get(&continuation_id)?;
                    (live_registers.len() <= crate::JIT_DEOPT_MAX_REGISTERS).then(|| {
                        let owned_locals = block
                            .terminator_live_locals
                            .iter()
                            .copied()
                            .filter(|local| {
                                value_flow.local_storage(*local).is_native_frame_local()
                            })
                            .collect();
                        let owned_registers = live_registers
                            .iter()
                            .copied()
                            .filter(|register| {
                                crate::region_ir::value_release_required(
                                    value_flow.register_fact(*register),
                                )
                            })
                            .collect();
                        crate::JitNativeTransitionMetadata {
                            function: region.function,
                            native_version: u32::from(
                                region.compile_metadata.tier == NativeCompilerTier::Optimizing,
                            ),
                            continuation_id,
                            resume_id: crate::native_transition_resume_id(continuation_id),
                            span: block.terminator_span,
                            live_locals: block.terminator_live_locals.clone(),
                            live_registers: live_registers.clone(),
                            owned_locals,
                            owned_registers,
                            result_register: None,
                        }
                    })
                }));
                transitions
            })
            .collect(),
        production_lowering: emitted_production_lowering,
        function_entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_native_closure_construction_does_not_require_a_trampoline() {
        let mut builder = php_ir::IrBuilder::new(php_ir::UnitId::new(7_001));
        let file = builder.add_file("direct-native-closure.php");
        let span = php_ir::IrSpan::new(file, 0, 1);

        let closure = builder.start_function(
            "direct_native_closure_body",
            php_ir::FunctionFlags::default(),
            span,
        );
        let closure_block = builder.append_block(closure);
        builder.terminate_return(closure, closure_block, None, span);

        let factory = builder.start_function(
            "direct_native_closure_factory",
            php_ir::FunctionFlags::default(),
            span,
        );
        let factory_block = builder.append_block(factory);
        let result = builder.alloc_register(factory);
        builder.emit(
            factory,
            factory_block,
            php_ir::InstructionKind::MakeClosure {
                dst: result,
                function: closure,
                captures: Vec::new(),
            },
            span,
        );
        builder.terminate_return(
            factory,
            factory_block,
            Some(php_ir::Operand::Register(result)),
            span,
        );

        let unit = builder.finish();
        let function = &unit.functions[factory.index()];
        assert!(!ir_function_requires_non_reference_trampoline(function));
        assert!(!ir_function_requires_trampoline(function));
    }

    #[test]
    fn native_exception_control_does_not_require_a_trampoline() {
        let mut builder = php_ir::IrBuilder::new(php_ir::UnitId::new(7_002));
        let file = builder.add_file("direct-native-exception.php");
        let span = php_ir::IrSpan::new(file, 0, 1);
        let function = builder.start_function(
            "direct_native_exception",
            php_ir::FunctionFlags::default(),
            span,
        );
        let block = builder.append_block(function);
        let message = builder.intern_constant(php_ir::IrConstant::String("native".to_owned()));
        let exception = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            php_ir::InstructionKind::MakeException {
                dst: exception,
                class_name: "runtimeexception".to_owned(),
                message: php_ir::Operand::Constant(message),
            },
            span,
        );
        builder.emit(
            function,
            block,
            php_ir::InstructionKind::Throw {
                value: php_ir::Operand::Register(exception),
            },
            span,
        );
        builder.terminate_return(function, block, None, span);

        let unit = builder.finish();
        let function = &unit.functions[function.index()];
        assert!(!ir_function_requires_non_reference_trampoline(function));
        assert!(!ir_function_requires_trampoline(function));
    }

    #[test]
    fn debug_backtrace_requires_a_complete_native_frame_trampoline() {
        let mut builder = php_ir::IrBuilder::new(php_ir::UnitId::new(7_004));
        let file = builder.add_file("native-debug-backtrace-frame.php");
        let span = php_ir::IrSpan::new(file, 0, 1);
        let function = builder.start_function(
            "native_debug_backtrace_frame",
            php_ir::FunctionFlags::default(),
            span,
        );
        let block = builder.append_block(function);
        let result = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            php_ir::InstructionKind::CallFunction {
                dst: result,
                name: "debug_backtrace".to_owned(),
                args: Vec::new(),
            },
            span,
        );
        builder.terminate_return(
            function,
            block,
            Some(php_ir::Operand::Register(result)),
            span,
        );

        let unit = builder.finish();
        let function = &unit.functions[function.index()];
        assert!(ir_function_requires_non_reference_trampoline(function));
        assert!(ir_function_requires_trampoline(function));
    }

    #[test]
    fn catch_admission_is_distinct_from_finally_only_control() {
        let mut builder = php_ir::IrBuilder::new(php_ir::UnitId::new(7_003));
        let file = builder.add_file("native-catch-admission.php");
        let span = php_ir::IrSpan::new(file, 0, 1);

        let catching = builder.start_function("catching", php_ir::FunctionFlags::default(), span);
        let catching_entry = builder.append_block(catching);
        let catch = builder.append_block(catching);
        let catching_after = builder.append_block(catching);
        let exception_local = builder.intern_local(catching, "exception");
        builder.emit(
            catching,
            catching_entry,
            php_ir::InstructionKind::EnterTry {
                catch: Some(catch),
                catch_types: vec!["throwable".to_owned()],
                finally: None,
                after: catching_after,
                exception_local: Some(exception_local),
            },
            span,
        );
        builder.terminate_jump(catching, catching_entry, catching_after, span);
        builder.terminate_jump(catching, catch, catching_after, span);
        builder.terminate_return(catching, catching_after, None, span);

        let finalizing =
            builder.start_function("finalizing", php_ir::FunctionFlags::default(), span);
        let finalizing_entry = builder.append_block(finalizing);
        let finally = builder.append_block(finalizing);
        let finalizing_after = builder.append_block(finalizing);
        builder.emit(
            finalizing,
            finalizing_entry,
            php_ir::InstructionKind::EnterTry {
                catch: None,
                catch_types: Vec::new(),
                finally: Some(finally),
                after: finalizing_after,
                exception_local: None,
            },
            span,
        );
        builder.terminate_jump(finalizing, finalizing_entry, finally, span);
        builder.emit(
            finalizing,
            finally,
            php_ir::InstructionKind::EndFinally {
                after: finalizing_after,
            },
            span,
        );
        builder.terminate_jump(finalizing, finally, finalizing_after, span);
        builder.terminate_return(finalizing, finalizing_after, None, span);

        let unit = builder.finish();
        assert!(ir_function_has_exception_handler(
            &unit.functions[catching.index()]
        ));
        assert!(ir_function_has_exception_handler(
            &unit.functions[finalizing.index()]
        ));
    }
}
