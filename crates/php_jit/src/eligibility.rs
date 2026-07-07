//! Conservative JIT eligibility analysis for performance.
//!
//! The analysis deliberately accepts only a tiny primitive, leaf-function IR
//! subset. Anything with PHP-visible dynamic behavior is rejected or marked
//! unknown before future lowering/codegen can see it.

use php_ir::instruction::{IrCallArg, TerminatorKind};
use php_ir::{
    BinaryOp, CastKind, CompareOp, FunctionId, Instruction, InstructionKind, IrCapture, IrConstant,
    IrFunction, IrParam, IrReturnType, IrUnit, Operand, UnaryOp,
};

/// Stable candidate kind assigned by the performance eligibility analyzer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitCandidateKind {
    /// A conservative leaf function containing only int-local operations.
    IntLeafCandidate,
    /// A typed packed-array `$xs[$i]` read-only fetch candidate.
    PackedArrayFetchCandidate,
    /// A packed-array by-value foreach integer reduction candidate.
    PackedForeachIntSumCandidate,
    /// A guarded fast path for exact known internal calls.
    KnownCallCandidate,
    /// A guarded fast path for two-string concatenation.
    StringConcatCandidate,
    /// A guarded fast path for a monomorphic property load.
    PropertyLoadCandidate,
}

impl JitCandidateKind {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IntLeafCandidate => "IntLeafCandidate",
            Self::PackedArrayFetchCandidate => "PackedArrayFetchCandidate",
            Self::PackedForeachIntSumCandidate => "PackedForeachIntSumCandidate",
            Self::KnownCallCandidate => "KnownCallCandidate",
            Self::StringConcatCandidate => "StringConcatCandidate",
            Self::PropertyLoadCandidate => "PropertyLoadCandidate",
        }
    }
}

/// Eligibility state for one JIT candidate region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JitEligibility {
    /// Region is inside the performance primitive subset.
    Eligible,
    /// Region is outside the subset for a stable, machine-readable reason.
    Rejected { reason: JitEligibilityReason },
    /// Region cannot be classified because the IR metadata is incomplete.
    Unknown { reason: JitEligibilityReason },
}

impl JitEligibility {
    /// Stable status spelling for reports.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Eligible => "eligible",
            Self::Rejected { .. } => "rejected",
            Self::Unknown { .. } => "unknown",
        }
    }
}

/// Stable machine-readable reason attached to a rejection or unknown result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitEligibilityReason {
    /// Stable reason identifier.
    pub code: &'static str,
    /// Human-readable detail for debug output.
    pub detail: String,
    /// Block index when the reason is instruction-local.
    pub block: Option<u32>,
    /// Instruction index when the reason is instruction-local.
    pub instruction: Option<u32>,
}

impl JitEligibilityReason {
    fn function(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            block: None,
            instruction: None,
        }
    }

    fn instruction(
        code: &'static str,
        detail: impl Into<String>,
        block: u32,
        instruction: u32,
    ) -> Self {
        Self {
            code,
            detail: detail.into(),
            block: Some(block),
            instruction: Some(instruction),
        }
    }
}

/// Per-report analysis counters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JitEligibilityStats {
    /// Functions inspected by this report.
    pub functions_analyzed: u64,
    /// Blocks inspected by this report.
    pub blocks_analyzed: u64,
    /// Instructions inspected by this report.
    pub instructions_analyzed: u64,
    /// Eligible regions observed by this report.
    pub eligible: u64,
    /// Rejected regions observed by this report.
    pub rejected: u64,
    /// Unknown regions observed by this report.
    pub unknown: u64,
}

/// Stable eligibility report for logs, tests, and future CLI output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitEligibilityReport {
    /// Function ID requested by the caller.
    pub function: FunctionId,
    /// Function name when the ID resolved.
    pub function_name: Option<String>,
    /// Final eligibility state.
    pub eligibility: JitEligibility,
    /// Candidate kind assigned when the function is eligible.
    pub candidate_kind: Option<JitCandidateKind>,
    /// All collected reasons, with the first reason mirrored in `eligibility`.
    pub reasons: Vec<JitEligibilityReason>,
    /// Analysis counters for this report.
    pub stats: JitEligibilityStats,
    /// Stable debug lines.
    pub debug: Vec<String>,
}

impl JitEligibilityReport {
    /// Returns a stable multi-line debug string.
    #[must_use]
    pub fn debug_output(&self) -> String {
        self.debug.join("\n")
    }

    /// Returns a stable compact JSON report for CLI output and fixtures.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut json = String::new();
        json.push('{');
        json.push_str("\"function_id\":");
        json.push_str(&self.function.raw().to_string());
        json.push_str(",\"function_name\":");
        match &self.function_name {
            Some(name) => {
                json.push('"');
                json.push_str(&escape_json(name));
                json.push('"');
            }
            None => json.push_str("null"),
        }
        json.push_str(",\"status\":\"");
        json.push_str(self.eligibility.as_str());
        json.push('"');
        json.push_str(",\"candidate_kind\":");
        match self.candidate_kind {
            Some(kind) => {
                json.push('"');
                json.push_str(kind.as_str());
                json.push('"');
            }
            None => json.push_str("null"),
        }
        json.push_str(",\"stats\":{");
        json.push_str("\"functions_analyzed\":");
        json.push_str(&self.stats.functions_analyzed.to_string());
        json.push_str(",\"blocks_analyzed\":");
        json.push_str(&self.stats.blocks_analyzed.to_string());
        json.push_str(",\"instructions_analyzed\":");
        json.push_str(&self.stats.instructions_analyzed.to_string());
        json.push_str(",\"eligible\":");
        json.push_str(&self.stats.eligible.to_string());
        json.push_str(",\"rejected\":");
        json.push_str(&self.stats.rejected.to_string());
        json.push_str(",\"unknown\":");
        json.push_str(&self.stats.unknown.to_string());
        json.push_str("},\"reasons\":[");
        for (index, reason) in self.reasons.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push('{');
            json.push_str("\"code\":\"");
            json.push_str(reason.code);
            json.push_str("\",\"detail\":\"");
            json.push_str(&escape_json(&reason.detail));
            json.push_str("\",\"block\":");
            match reason.block {
                Some(block) => json.push_str(&block.to_string()),
                None => json.push_str("null"),
            }
            json.push_str(",\"instruction\":");
            match reason.instruction {
                Some(instruction) => json.push_str(&instruction.to_string()),
                None => json.push_str("null"),
            }
            json.push('}');
        }
        json.push_str("]}");
        json
    }
}

/// Analyzes one function in a unit for performance JIT eligibility.
#[must_use]
pub fn analyze_jit_eligibility(unit: &IrUnit, function: FunctionId) -> JitEligibilityReport {
    let Some(ir_function) = unit.functions.get(function.index()) else {
        let reason = JitEligibilityReason::function(
            "JIT_ELIGIBILITY_UNKNOWN_FUNCTION",
            format!(
                "function id {} is not present in the IR unit",
                function.raw()
            ),
        );
        return unknown_report(function, None, reason);
    };

    analyze_function(unit, function, ir_function, &unit.constants)
}

fn analyze_function(
    unit: &IrUnit,
    function_id: FunctionId,
    function: &IrFunction,
    constants: &[IrConstant],
) -> JitEligibilityReport {
    let mut stats = JitEligibilityStats {
        functions_analyzed: 1,
        blocks_analyzed: function.blocks.len() as u64,
        ..JitEligibilityStats::default()
    };
    stats.instructions_analyzed = function
        .blocks
        .iter()
        .map(|block| block.instructions.len() as u64)
        .sum();

    if packed_foreach_int_sum_candidate_is_eligible(function, constants).is_ok() {
        stats.eligible = 1;
        let eligibility = JitEligibility::Eligible;
        let debug = vec![
            format!(
                "jit-eligibility function={} status={}",
                function.name,
                eligibility.as_str()
            ),
            format!(
                "jit-eligibility stats functions={} blocks={} instructions={}",
                stats.functions_analyzed, stats.blocks_analyzed, stats.instructions_analyzed
            ),
            "jit-eligibility candidate=PackedForeachIntSumCandidate".to_owned(),
        ];
        return JitEligibilityReport {
            function: function_id,
            function_name: Some(function.name.clone()),
            eligibility,
            candidate_kind: Some(JitCandidateKind::PackedForeachIntSumCandidate),
            reasons: Vec::new(),
            stats,
            debug,
        };
    }

    if packed_array_fetch_candidate_is_eligible(function).is_ok() {
        stats.eligible = 1;
        let eligibility = JitEligibility::Eligible;
        let debug = vec![
            format!(
                "jit-eligibility function={} status={}",
                function.name,
                eligibility.as_str()
            ),
            format!(
                "jit-eligibility stats functions={} blocks={} instructions={}",
                stats.functions_analyzed, stats.blocks_analyzed, stats.instructions_analyzed
            ),
            "jit-eligibility candidate=PackedArrayFetchCandidate".to_owned(),
        ];
        return JitEligibilityReport {
            function: function_id,
            function_name: Some(function.name.clone()),
            eligibility,
            candidate_kind: Some(JitCandidateKind::PackedArrayFetchCandidate),
            reasons: Vec::new(),
            stats,
            debug,
        };
    }

    if known_call_candidate_is_eligible(unit, function).is_ok() {
        stats.eligible = 1;
        let eligibility = JitEligibility::Eligible;
        let debug = vec![
            format!(
                "jit-eligibility function={} status={}",
                function.name,
                eligibility.as_str()
            ),
            format!(
                "jit-eligibility stats functions={} blocks={} instructions={}",
                stats.functions_analyzed, stats.blocks_analyzed, stats.instructions_analyzed
            ),
            "jit-eligibility candidate=KnownCallCandidate".to_owned(),
        ];
        return JitEligibilityReport {
            function: function_id,
            function_name: Some(function.name.clone()),
            eligibility,
            candidate_kind: Some(JitCandidateKind::KnownCallCandidate),
            reasons: Vec::new(),
            stats,
            debug,
        };
    }

    if string_concat_candidate_is_eligible(function).is_ok() {
        stats.eligible = 1;
        let eligibility = JitEligibility::Eligible;
        let debug = vec![
            format!(
                "jit-eligibility function={} status={}",
                function.name,
                eligibility.as_str()
            ),
            format!(
                "jit-eligibility stats functions={} blocks={} instructions={}",
                stats.functions_analyzed, stats.blocks_analyzed, stats.instructions_analyzed
            ),
            "jit-eligibility candidate=StringConcatCandidate".to_owned(),
        ];
        return JitEligibilityReport {
            function: function_id,
            function_name: Some(function.name.clone()),
            eligibility,
            candidate_kind: Some(JitCandidateKind::StringConcatCandidate),
            reasons: Vec::new(),
            stats,
            debug,
        };
    }

    if property_load_candidate_is_eligible(unit, function).is_ok() {
        stats.eligible = 1;
        let eligibility = JitEligibility::Eligible;
        let debug = vec![
            format!(
                "jit-eligibility function={} status={}",
                function.name,
                eligibility.as_str()
            ),
            format!(
                "jit-eligibility stats functions={} blocks={} instructions={}",
                stats.functions_analyzed, stats.blocks_analyzed, stats.instructions_analyzed
            ),
            "jit-eligibility candidate=PropertyLoadCandidate".to_owned(),
        ];
        return JitEligibilityReport {
            function: function_id,
            function_name: Some(function.name.clone()),
            eligibility,
            candidate_kind: Some(JitCandidateKind::PropertyLoadCandidate),
            reasons: Vec::new(),
            stats,
            debug,
        };
    }

    let mut rejected = Vec::new();
    let mut unknown = Vec::new();

    check_function_shape(function, &mut rejected, &mut unknown);

    for block in &function.blocks {
        let block_index = block.id.raw();
        for instruction in &block.instructions {
            check_instruction(
                instruction,
                block_index,
                constants,
                &mut rejected,
                &mut unknown,
            );
        }

        match &block.terminator {
            Some(terminator) => {
                check_terminator(
                    &terminator.kind,
                    block_index,
                    constants,
                    &mut rejected,
                    &mut unknown,
                );
            }
            None => unknown.push(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_UNKNOWN_MISSING_TERMINATOR",
                format!("block {} has no terminator", block.id.raw()),
            )),
        }
    }

    let (eligibility, reasons) = if let Some(reason) = rejected.first().cloned() {
        stats.rejected = 1;
        let mut reasons = rejected;
        reasons.extend(unknown);
        (JitEligibility::Rejected { reason }, reasons)
    } else if let Some(reason) = unknown.first().cloned() {
        stats.unknown = 1;
        (JitEligibility::Unknown { reason }, unknown)
    } else {
        stats.eligible = 1;
        (JitEligibility::Eligible, Vec::new())
    };

    let mut debug = Vec::new();
    debug.push(format!(
        "jit-eligibility function={} status={}",
        function.name,
        eligibility.as_str()
    ));
    debug.push(format!(
        "jit-eligibility stats functions={} blocks={} instructions={}",
        stats.functions_analyzed, stats.blocks_analyzed, stats.instructions_analyzed
    ));
    for reason in &reasons {
        match (reason.block, reason.instruction) {
            (Some(block), Some(instruction)) => debug.push(format!(
                "jit-eligibility reason code={} block={} instruction={} detail={}",
                reason.code, block, instruction, reason.detail
            )),
            _ => debug.push(format!(
                "jit-eligibility reason code={} detail={}",
                reason.code, reason.detail
            )),
        }
    }

    JitEligibilityReport {
        function: function_id,
        function_name: Some(function.name.clone()),
        candidate_kind: matches!(eligibility, JitEligibility::Eligible)
            .then_some(JitCandidateKind::IntLeafCandidate),
        eligibility,
        reasons,
        stats,
        debug,
    }
}

fn packed_foreach_int_sum_candidate_is_eligible(
    function: &IrFunction,
    constants: &[IrConstant],
) -> Result<(), JitEligibilityReason> {
    if function.flags.is_top_level
        || function.flags.is_closure
        || function.flags.is_method
        || function.flags.is_generator
        || function.returns_by_ref
        || !function.captures.is_empty()
    {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_SHAPE",
            "packed foreach sum candidate requires an ordinary leaf function",
        ));
    }
    if function.return_type.as_ref() != Some(&IrReturnType::Int) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_RETURN",
            "packed foreach sum candidate requires declared int return",
        ));
    }
    let [array_param] = function.params.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_PARAMS",
            "packed foreach sum candidate requires one array param",
        ));
    };
    if array_param.by_ref || array_param.variadic || array_param.default.is_some() {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_PARAMS",
            "packed foreach sum array parameter must be required by-value",
        ));
    }
    if array_param.type_.as_ref() != Some(&IrReturnType::Array) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_PARAMS",
            "packed foreach sum parameter must be declared array",
        ));
    }
    let [entry, condition, body, after] = function.blocks.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONTROL_FLOW",
            "packed foreach sum requires entry, condition, body, and return blocks",
        ));
    };

    let [
        init_value,
        store_sum,
        discard_init,
        load_array,
        foreach_init,
    ] = entry.instructions.as_slice()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum entry block has unexpected instructions",
        ));
    };
    let InstructionKind::LoadConst {
        dst: zero_reg,
        constant,
    } = init_value.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum must initialize accumulator with a constant",
        ));
    };
    if constants.get(constant.index()) != Some(&IrConstant::Int(0)) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum accumulator must start at integer zero",
        ));
    }
    let InstructionKind::StoreLocal {
        local: sum_local,
        src: Operand::Register(store_reg),
    } = store_sum.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum must store zero into an accumulator local",
        ));
    };
    if store_reg != zero_reg {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach accumulator store must use initialized zero register",
        ));
    }
    match discard_init.kind.clone() {
        InstructionKind::Discard {
            src: Operand::Register(reg),
        } if reg == zero_reg => {}
        _ => {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
                "packed foreach sum entry must discard the initializer result",
            ));
        }
    }
    let InstructionKind::LoadLocal {
        dst: array_reg,
        local,
    } = load_array.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum must load the array parameter",
        ));
    };
    if local != array_param.local {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum source must be the array parameter",
        ));
    }
    let InstructionKind::ForeachInit {
        iterator,
        source: Operand::Register(source_reg),
    } = foreach_init.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach sum must use by-value foreach init",
        ));
    };
    if source_reg != array_reg {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_ENTRY",
            "packed foreach init source must be the array parameter load",
        ));
    }
    match &entry.terminator {
        Some(terminator) => match terminator.kind.clone() {
            TerminatorKind::Jump { target } if target == condition.id => {}
            _ => {
                return Err(JitEligibilityReason::function(
                    "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                    "packed foreach entry must jump to the condition block",
                ));
            }
        },
        None => {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                "packed foreach entry requires a terminator",
            ));
        }
    }

    let [foreach_next] = condition.instructions.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONDITION",
            "packed foreach condition must contain one foreach_next",
        ));
    };
    let InstructionKind::ForeachNext {
        has_value,
        iterator: next_iterator,
        key: None,
        value,
    } = foreach_next.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONDITION",
            "packed foreach sum must be by-value without key binding",
        ));
    };
    if next_iterator != iterator {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONDITION",
            "foreach_next iterator does not match foreach_init",
        ));
    }
    match &condition.terminator {
        Some(terminator) => match terminator.kind.clone() {
            TerminatorKind::JumpIf {
                condition: Operand::Register(condition_reg),
                if_true,
                if_false,
            } if condition_reg == has_value && if_true == body.id && if_false == after.id => {}
            _ => {
                return Err(JitEligibilityReason::function(
                    "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                    "packed foreach condition must branch to body or return block",
                ));
            }
        },
        None => {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                "packed foreach condition requires a terminator",
            ));
        }
    }

    let [
        store_value,
        load_sum,
        load_value,
        add,
        store_accumulator,
        discard_add,
    ] = body.instructions.as_slice()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body must contain only element store and int accumulation",
        ));
    };
    let InstructionKind::StoreLocal {
        local: value_local,
        src: Operand::Register(stored_value_reg),
    } = store_value.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body must store the current element local",
        ));
    };
    if stored_value_reg != value {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body value local must come from foreach_next",
        ));
    }
    let InstructionKind::LoadLocal {
        dst: loaded_sum,
        local: loaded_sum_local,
    } = load_sum.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body must load the accumulator",
        ));
    };
    if loaded_sum_local != sum_local {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body must accumulate into the initialized local",
        ));
    }
    let InstructionKind::LoadLocal {
        dst: loaded_value,
        local: loaded_value_local,
    } = load_value.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body must load the current element",
        ));
    };
    if loaded_value_local != value_local {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body must add the current element local",
        ));
    }
    let InstructionKind::Binary {
        dst: add_result,
        op: BinaryOp::Add,
        lhs: Operand::Register(add_lhs),
        rhs: Operand::Register(add_rhs),
    } = add.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body must be a single addition",
        ));
    };
    if add_lhs != loaded_sum || add_rhs != loaded_value {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
            "packed foreach body addition must use accumulator plus current element",
        ));
    }
    match store_accumulator.kind.clone() {
        InstructionKind::StoreLocal {
            local,
            src: Operand::Register(reg),
        } if local == sum_local && reg == add_result => {}
        _ => {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
                "packed foreach body must store the addition result to the accumulator",
            ));
        }
    }
    match discard_add.kind.clone() {
        InstructionKind::Discard {
            src: Operand::Register(reg),
        } if reg == add_result => {}
        _ => {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_BODY",
                "packed foreach body must discard the addition result",
            ));
        }
    }
    match &body.terminator {
        Some(terminator) => match terminator.kind.clone() {
            TerminatorKind::Jump { target } if target == condition.id => {}
            _ => {
                return Err(JitEligibilityReason::function(
                    "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                    "packed foreach body must loop back to the condition block",
                ));
            }
        },
        None => {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_CONTROL_FLOW",
                "packed foreach body requires a terminator",
            ));
        }
    }

    let return_load = match after.instructions.as_slice() {
        [return_load] => return_load,
        [cleanup, return_load] => {
            match cleanup.kind {
                InstructionKind::ForeachCleanup {
                    iterator: cleanup_iterator,
                } if cleanup_iterator == iterator => {}
                _ => {
                    return Err(JitEligibilityReason::function(
                        "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_RETURN",
                        "packed foreach return block cleanup must match the foreach iterator",
                    ));
                }
            }
            return_load
        }
        _ => {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_RETURN",
                "packed foreach return block must optionally cleanup foreach then load the accumulator",
            ));
        }
    };
    let InstructionKind::LoadLocal {
        dst: return_reg,
        local: return_local,
    } = return_load.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_RETURN",
            "packed foreach return block must load the accumulator",
        ));
    };
    if return_local != sum_local {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_RETURN",
            "packed foreach return must read the accumulator local",
        ));
    }
    match &after.terminator {
        Some(terminator) => match terminator.kind.clone() {
            TerminatorKind::Return {
                value: Some(Operand::Register(reg)),
                by_ref_local: None,
            } if reg == return_reg => Ok(()),
            _ => Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_RETURN",
                "packed foreach sum must return the accumulator by value",
            )),
        },
        None => Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FOREACH_RETURN",
            "packed foreach return block requires a terminator",
        )),
    }
}

fn packed_array_fetch_candidate_is_eligible(
    function: &IrFunction,
) -> Result<(), JitEligibilityReason> {
    if function.flags.is_top_level
        || function.flags.is_closure
        || function.flags.is_method
        || function.flags.is_generator
        || function.returns_by_ref
        || !function.captures.is_empty()
    {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_SHAPE",
            "packed-array fetch candidate requires an ordinary leaf function",
        ));
    }
    if function.return_type.as_ref() != Some(&IrReturnType::Int) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_RETURN",
            "packed-array fetch candidate requires declared int return",
        ));
    }
    let [array_param, index_param] = function.params.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_PARAMS",
            "packed-array fetch candidate requires array and int params",
        ));
    };
    check_packed_fetch_param_shape(array_param, IrReturnType::Array, "array")?;
    check_packed_fetch_param_shape(index_param, IrReturnType::Int, "index")?;
    let [block] = function.blocks.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_CONTROL_FLOW",
            "packed-array fetch candidate requires one basic block",
        ));
    };

    let mut array_reg = None;
    let mut index_reg = None;
    let mut fetch_reg = None;
    for instruction in &block.instructions {
        match &instruction.kind {
            InstructionKind::LoadLocal { dst, local } if *local == array_param.local => {
                array_reg = Some(*dst);
            }
            InstructionKind::LoadLocal { dst, local } if *local == index_param.local => {
                index_reg = Some(*dst);
            }
            InstructionKind::FetchDim {
                dst,
                array,
                key,
                quiet: false,
            } => {
                if *array
                    != Operand::Register(array_reg.ok_or_else(|| {
                        JitEligibilityReason::function(
                            "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_SHAPE",
                            "fetch_dim appears before array param load",
                        )
                    })?)
                    || *key
                        != Operand::Register(index_reg.ok_or_else(|| {
                            JitEligibilityReason::function(
                                "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_SHAPE",
                                "fetch_dim appears before index param load",
                            )
                        })?)
                {
                    return Err(JitEligibilityReason::function(
                        "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_SHAPE",
                        "fetch_dim operands do not match array and index params",
                    ));
                }
                fetch_reg = Some(*dst);
            }
            _ => {
                return Err(JitEligibilityReason::function(
                    "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_OPCODE",
                    "instruction is outside the packed-array fetch subset",
                ));
            }
        }
    }

    match &block.terminator {
        Some(terminator) => match &terminator.kind {
            TerminatorKind::Return {
                value: Some(Operand::Register(return_reg)),
                by_ref_local: None,
            } if Some(*return_reg) == fetch_reg => Ok(()),
            _ => Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_TERMINATOR",
                "packed-array fetch candidate must return the fetched value by value",
            )),
        },
        None => Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_TERMINATOR",
            "packed-array fetch candidate requires a return terminator",
        )),
    }
}

fn check_packed_fetch_param_shape(
    param: &IrParam,
    expected: IrReturnType,
    role: &'static str,
) -> Result<(), JitEligibilityReason> {
    if param.by_ref || param.variadic || param.default.is_some() {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_PARAMS",
            format!("packed-array fetch {role} parameter must be required by-value"),
        ));
    }
    if param.type_.as_ref() != Some(&expected) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PACKED_FETCH_PARAMS",
            format!("packed-array fetch {role} parameter has wrong type"),
        ));
    }
    Ok(())
}

fn known_call_candidate_is_eligible(
    unit: &IrUnit,
    function: &IrFunction,
) -> Result<(), JitEligibilityReason> {
    if function.flags.is_top_level
        || function.flags.is_closure
        || function.flags.is_method
        || function.flags.is_generator
        || function.returns_by_ref
        || !function.captures.is_empty()
    {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_SHAPE",
            "known-call candidate requires an ordinary leaf function",
        ));
    }
    if function.return_type.as_ref() != Some(&IrReturnType::Int) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_RETURN",
            "known-call candidate requires declared int return",
        ));
    }
    let [param] = function.params.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_PARAMS",
            "known-call candidate requires exactly one parameter",
        ));
    };
    if param.by_ref || param.variadic || param.default.is_some() {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_PARAMS",
            "known-call parameter must be required by-value",
        ));
    }
    let [block] = function.blocks.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_CONTROL_FLOW",
            "known-call candidate requires one basic block",
        ));
    };
    let [load, call] = block.instructions.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_INSTRUCTIONS",
            "known-call candidate expects load-local then call",
        ));
    };
    let InstructionKind::LoadLocal {
        dst: loaded,
        local: loaded_local,
    } = load.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_LOAD",
            "known-call candidate must load the sole parameter",
        ));
    };
    if loaded_local != param.local {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_LOAD",
            "known-call load must read the sole parameter local",
        ));
    }
    let InstructionKind::CallFunction { dst, name, args } = &call.kind else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_OPCODE",
            "known-call candidate expects a direct function call",
        ));
    };
    let expected_type = match name.as_str() {
        "strlen" => IrReturnType::String,
        "count" => IrReturnType::Array,
        _ => {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_TARGET",
                "known-call candidate only supports strlen and count",
            ));
        }
    };
    if unit
        .function_table
        .iter()
        .any(|entry| entry.name.eq_ignore_ascii_case(name))
    {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_OVERRIDE",
            "known-call candidate rejected a user function override ambiguity",
        ));
    }
    let [arg] = args.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_ARITY",
            "known-call candidate requires exactly one call argument",
        ));
    };
    if arg.name.is_some() || arg.unpack {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_ARGUMENT_MODE",
            "known-call candidate rejects named and unpacked arguments",
        ));
    }
    if arg.value != Operand::Register(loaded) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_ARGUMENT",
            "known-call argument must be the loaded parameter",
        ));
    }
    let param_type_supported = match param.type_.as_ref() {
        None => true,
        Some(type_) => type_ == &expected_type,
    };
    if !param_type_supported {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_PARAM_TYPE",
            "known-call parameter type is incompatible with the builtin guard",
        ));
    }
    match &block.terminator {
        Some(terminator) => match &terminator.kind {
            TerminatorKind::Return {
                value: Some(Operand::Register(return_reg)),
                by_ref_local: None,
            } if return_reg == dst => Ok(()),
            _ => Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_RETURN",
                "known-call candidate must return the call result by value",
            )),
        },
        None => Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_KNOWN_CALL_RETURN",
            "known-call candidate requires a return terminator",
        )),
    }
}

fn string_concat_candidate_is_eligible(function: &IrFunction) -> Result<(), JitEligibilityReason> {
    if function.flags.is_top_level
        || function.flags.is_closure
        || function.flags.is_method
        || function.flags.is_generator
        || function.returns_by_ref
        || !function.captures.is_empty()
    {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_SHAPE",
            "string-concat candidate requires an ordinary leaf function",
        ));
    }
    if function.return_type.as_ref() != Some(&IrReturnType::String) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_RETURN",
            "string-concat candidate requires declared string return",
        ));
    }
    let [lhs_param, rhs_param] = function.params.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_PARAMS",
            "string-concat candidate requires exactly two parameters",
        ));
    };
    for param in [lhs_param, rhs_param] {
        if param.by_ref || param.variadic || param.default.is_some() {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_PARAMS",
                "string-concat parameters must be required by-value",
            ));
        }
        if param.type_.as_ref() != Some(&IrReturnType::String) {
            return Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_PARAM_TYPE",
                "string-concat operands must be declared strings",
            ));
        }
    }
    let [block] = function.blocks.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_CONTROL_FLOW",
            "string-concat candidate requires one basic block",
        ));
    };
    let [load_lhs, load_rhs, concat] = block.instructions.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_INSTRUCTIONS",
            "string-concat candidate expects load, load, concat",
        ));
    };
    let InstructionKind::LoadLocal {
        dst: loaded_lhs,
        local: loaded_lhs_local,
    } = load_lhs.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_LOAD",
            "string-concat candidate must load the left parameter",
        ));
    };
    if loaded_lhs_local != lhs_param.local {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_LOAD",
            "left concat operand must load the left parameter",
        ));
    }
    let InstructionKind::LoadLocal {
        dst: loaded_rhs,
        local: loaded_rhs_local,
    } = load_rhs.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_LOAD",
            "string-concat candidate must load the right parameter",
        ));
    };
    if loaded_rhs_local != rhs_param.local {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_LOAD",
            "right concat operand must load the right parameter",
        ));
    }
    let InstructionKind::Binary {
        dst,
        op: BinaryOp::Concat,
        lhs: Operand::Register(lhs_reg),
        rhs: Operand::Register(rhs_reg),
    } = concat.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_OPCODE",
            "string-concat candidate expects a binary concat",
        ));
    };
    if lhs_reg != loaded_lhs || rhs_reg != loaded_rhs {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_OPERANDS",
            "string-concat operands must be the loaded parameters",
        ));
    }
    match &block.terminator {
        Some(terminator) => match &terminator.kind {
            TerminatorKind::Return {
                value: Some(Operand::Register(return_reg)),
                by_ref_local: None,
            } if *return_reg == dst => Ok(()),
            _ => Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_RETURN",
                "string-concat candidate must return the concat result",
            )),
        },
        None => Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_STRING_CONCAT_RETURN",
            "string-concat candidate requires a return terminator",
        )),
    }
}

fn property_load_candidate_is_eligible(
    unit: &IrUnit,
    function: &IrFunction,
) -> Result<(), JitEligibilityReason> {
    if function.flags.is_top_level
        || function.flags.is_closure
        || function.flags.is_method
        || function.flags.is_generator
        || function.returns_by_ref
        || !function.captures.is_empty()
    {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_SHAPE",
            "property-load candidate requires an ordinary leaf function",
        ));
    }
    if matches!(
        function.return_type.as_ref(),
        None | Some(IrReturnType::Void | IrReturnType::Never)
    ) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_RETURN",
            "property-load candidate requires a value return type",
        ));
    }
    let [param] = function.params.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_PARAMS",
            "property-load candidate requires exactly one object parameter",
        ));
    };
    if param.by_ref || param.variadic || param.default.is_some() {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_PARAMS",
            "property-load parameter must be required by-value",
        ));
    }
    let Some(IrReturnType::Class { name, .. }) = param.type_.as_ref() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_PARAM_TYPE",
            "property-load parameter must have a class type",
        ));
    };
    let Some(class) = unit
        .classes
        .iter()
        .find(|class| normalize_class_name(&class.name) == normalize_class_name(name))
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_CLASS",
            "property-load parameter class is not present in the IR unit",
        ));
    };
    let [block] = function.blocks.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_CONTROL_FLOW",
            "property-load candidate requires one straight-line block",
        ));
    };
    let [load, fetch] = block.instructions.as_slice() else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_INSTRUCTIONS",
            "property-load candidate expects load-local then fetch-property",
        ));
    };
    let InstructionKind::LoadLocal {
        dst: loaded,
        local: loaded_local,
    } = load.kind.clone()
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_LOAD",
            "property-load candidate must load the object parameter",
        ));
    };
    if loaded_local != param.local {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_LOAD",
            "property-load load must read the object parameter local",
        ));
    }
    let InstructionKind::FetchProperty {
        dst,
        object: Operand::Register(object_reg),
        property,
    } = &fetch.kind
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_OPCODE",
            "property-load candidate expects a direct property fetch",
        ));
    };
    if *object_reg != loaded {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_OBJECT",
            "property-load fetch must use the loaded object parameter",
        ));
    }
    let Some(property_entry) = class
        .properties
        .iter()
        .find(|entry| entry.name == *property)
    else {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_DECLARED",
            "property-load fast path requires a declared property",
        ));
    };
    if property_entry.flags.is_static
        || property_entry.flags.is_private
        || property_entry.flags.is_protected
    {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_VISIBILITY",
            "property-load fast path requires a visible instance property",
        ));
    }
    if property_entry.hooks.get.is_some() || property_entry.hooks.set.is_some() {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_HOOK",
            "property-load fast path rejects property hooks",
        ));
    }
    if class.methods.iter().any(|method| {
        method.name.eq_ignore_ascii_case("__get")
            && !method.flags.is_static
            && !method.flags.is_private
            && !method.flags.is_protected
    }) {
        return Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_MAGIC_GET",
            "property-load fast path rejects public __get",
        ));
    }
    match &block.terminator {
        Some(terminator) => match &terminator.kind {
            TerminatorKind::Return {
                value: Some(Operand::Register(return_reg)),
                by_ref_local: None,
            } if return_reg == dst => Ok(()),
            _ => Err(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_RETURN",
                "property-load candidate must return the fetched property by value",
            )),
        },
        None => Err(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_PROPERTY_LOAD_RETURN",
            "property-load candidate requires a return terminator",
        )),
    }
}

fn normalize_class_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

fn check_function_shape(
    function: &IrFunction,
    rejected: &mut Vec<JitEligibilityReason>,
    unknown: &mut Vec<JitEligibilityReason>,
) {
    if function.flags.is_generator {
        rejected.push(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_GENERATOR",
            "generators are outside the performance JIT subset",
        ));
    }
    if function.flags.is_closure && !function.captures.is_empty() {
        rejected.push(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_CLOSURE_CAPTURE",
            "capturing closures may observe reference and lifetime behavior; alias_state=unknown_aliasing",
        ));
    }
    if function.returns_by_ref {
        rejected.push(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_BY_REF_RETURN",
            "by-reference returns are outside the performance JIT subset; alias_state=escaped_reference",
        ));
    }
    if function.blocks.is_empty() {
        unknown.push(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_UNKNOWN_EMPTY_BODY",
            "function has no basic blocks",
        ));
    }

    for param in &function.params {
        check_param(param, rejected);
    }
    for capture in &function.captures {
        check_capture(capture, rejected);
    }
    if let Some(return_type) = &function.return_type {
        check_type(return_type, "return type", rejected);
    }
}

fn check_param(param: &IrParam, rejected: &mut Vec<JitEligibilityReason>) {
    if param.by_ref {
        rejected.push(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_BY_REF_PARAM",
            format!(
                "parameter `${}` is by-reference; alias_state=escaped_reference",
                param.name
            ),
        ));
    }
    if param.variadic {
        rejected.push(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_VARIADIC_PARAM",
            format!("parameter `${}` is variadic", param.name),
        ));
    }
    match &param.type_ {
        Some(type_) => check_type(type_, "parameter type", rejected),
        None => rejected.push(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_UNTYPED_PARAM",
            format!(
                "parameter `${}` is not declared int and no stable int profile is available",
                param.name
            ),
        )),
    }
}

fn check_capture(capture: &IrCapture, rejected: &mut Vec<JitEligibilityReason>) {
    if capture.by_ref {
        rejected.push(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_BY_REF_CAPTURE",
            format!(
                "capture `${}` is by-reference; alias_state=escaped_reference",
                capture.name
            ),
        ));
    }
}

fn check_type(
    type_: &IrReturnType,
    context: &'static str,
    rejected: &mut Vec<JitEligibilityReason>,
) {
    if !matches!(type_, IrReturnType::Int) {
        rejected.push(JitEligibilityReason::function(
            "JIT_ELIGIBILITY_REJECT_NON_PRIMITIVE_TYPE",
            format!("{context} is not an int"),
        ));
    }
}

fn check_instruction(
    instruction: &Instruction,
    block: u32,
    constants: &[IrConstant],
    rejected: &mut Vec<JitEligibilityReason>,
    unknown: &mut Vec<JitEligibilityReason>,
) {
    let id = instruction.id.raw();
    match &instruction.kind {
        InstructionKind::Nop
        | InstructionKind::LoadLocal { .. }
        | InstructionKind::StoreLocal { .. }
        | InstructionKind::Move { .. }
        | InstructionKind::Discard { .. } => {
            check_instruction_operands(instruction, block, constants, rejected, unknown);
        }
        InstructionKind::LoadLocalQuiet { .. }
        | InstructionKind::IssetLocal { .. }
        | InstructionKind::EmptyLocal { .. } => rejected.push(JitEligibilityReason::instruction(
            "JIT_ELIGIBILITY_REJECT_DYNAMIC_LOCAL_OPCODE",
            "dynamic local existence checks are outside the int leaf subset",
            block,
            id,
        )),
        InstructionKind::LoadConst { constant, .. } => {
            check_constant(*constant, block, id, constants, rejected, unknown);
        }
        InstructionKind::Binary { op, .. } => {
            if !is_allowed_binary(*op) {
                rejected.push(JitEligibilityReason::instruction(
                    "JIT_ELIGIBILITY_REJECT_NON_PRIMITIVE_BINARY_OP",
                    format!("binary op {op:?} is outside the primitive int subset"),
                    block,
                    id,
                ));
            }
            check_instruction_operands(instruction, block, constants, rejected, unknown);
        }
        InstructionKind::Compare { op, .. } => {
            if !is_allowed_compare(*op) {
                rejected.push(JitEligibilityReason::instruction(
                    "JIT_ELIGIBILITY_REJECT_NON_PRIMITIVE_COMPARE_OP",
                    format!("compare op {op:?} is outside the primitive subset"),
                    block,
                    id,
                ));
            }
            check_instruction_operands(instruction, block, constants, rejected, unknown);
        }
        InstructionKind::Unary { op, .. } => {
            if !is_allowed_unary(*op) {
                rejected.push(JitEligibilityReason::instruction(
                    "JIT_ELIGIBILITY_REJECT_NON_PRIMITIVE_UNARY_OP",
                    format!("unary op {op:?} is outside the primitive int/bool subset"),
                    block,
                    id,
                ));
            }
            check_instruction_operands(instruction, block, constants, rejected, unknown);
        }
        InstructionKind::Cast { kind, .. } => {
            if !matches!(kind, CastKind::Bool | CastKind::Int) {
                rejected.push(JitEligibilityReason::instruction(
                    "JIT_ELIGIBILITY_REJECT_NON_PRIMITIVE_CAST",
                    format!("cast {kind:?} is outside the primitive int/bool subset"),
                    block,
                    id,
                ));
            }
            check_instruction_operands(instruction, block, constants, rejected, unknown);
        }
        InstructionKind::BindReference { .. }
        | InstructionKind::BindGlobal { .. }
        | InstructionKind::BindReferenceDim { .. }
        | InstructionKind::BindReferenceProperty { .. }
        | InstructionKind::BindReferencePropertyDim { .. }
        | InstructionKind::BindReferenceDimFromProperty { .. }
        | InstructionKind::BindReferenceFromProperty { .. }
        | InstructionKind::BindReferenceFromPropertyDim { .. }
        | InstructionKind::BindReferenceFromDim { .. }
        | InstructionKind::BindReferenceFromStaticPropertyDim { .. }
        | InstructionKind::BindReferenceStaticProperty { .. }
        | InstructionKind::BindReferenceFromCall { .. }
        | InstructionKind::BindReferenceFromMethodCall { .. } => {
            rejected.push(JitEligibilityReason::instruction(
                "JIT_ELIGIBILITY_REJECT_REFERENCE_OPCODE",
                "reference-producing opcodes are outside the JIT subset; alias_state=unknown_aliasing",
                block,
                id,
            ))
        }
        InstructionKind::CallFunction { .. }
        | InstructionKind::CallMethod { .. }
        | InstructionKind::CallStaticMethod { .. }
        | InstructionKind::CallClosure { .. }
        | InstructionKind::CallCallable { .. }
        | InstructionKind::Pipe { .. }
        | InstructionKind::ResolveCallable { .. }
        | InstructionKind::AcquireCallable { .. }
        | InstructionKind::MakeClosure { .. } => rejected.push(JitEligibilityReason::instruction(
            "JIT_ELIGIBILITY_REJECT_CALL_OPCODE",
            "calls and callable resolution are outside the default JIT subset",
            block,
            id,
        )),
        InstructionKind::EnterTry { .. }
        | InstructionKind::LeaveTry
        | InstructionKind::EndFinally { .. }
        | InstructionKind::Throw { .. }
        | InstructionKind::MakeException { .. } => {
            rejected.push(JitEligibilityReason::instruction(
                "JIT_ELIGIBILITY_REJECT_EXCEPTION_OPCODE",
                "exception control-flow is outside the JIT subset",
                block,
                id,
            ))
        }
        InstructionKind::Yield { .. } | InstructionKind::YieldFrom { .. } => {
            rejected.push(JitEligibilityReason::instruction(
                "JIT_ELIGIBILITY_REJECT_GENERATOR_OPCODE",
                "generator opcodes are outside the JIT subset",
                block,
                id,
            ))
        }
        InstructionKind::Include { .. } | InstructionKind::Eval { .. } => {
            rejected.push(JitEligibilityReason::instruction(
                "JIT_ELIGIBILITY_REJECT_INCLUDE_EVAL_OPCODE",
                "include/eval/autoload-sensitive opcodes are outside the JIT subset",
                block,
                id,
            ))
        }
        InstructionKind::NewArray { .. }
        | InstructionKind::ArrayInsert { .. }
        | InstructionKind::ArraySpread { .. }
        | InstructionKind::FetchDim { .. }
        | InstructionKind::AssignDim { .. }
        | InstructionKind::AppendDim { .. }
        | InstructionKind::IssetDim { .. }
        | InstructionKind::EmptyDim { .. }
        | InstructionKind::UnsetDim { .. }
        | InstructionKind::ForeachInit { .. }
        | InstructionKind::ForeachNext { .. }
        | InstructionKind::ForeachCleanup { .. }
        | InstructionKind::ForeachInitRef { .. }
        | InstructionKind::ForeachNextRef { .. }
        | InstructionKind::ArrayGet { .. } => rejected.push(JitEligibilityReason::instruction(
            "JIT_ELIGIBILITY_REJECT_ARRAY_OPCODE",
            "array and foreach opcodes are outside the JIT subset",
            block,
            id,
        )),
        InstructionKind::NewObject { .. }
        | InstructionKind::DynamicNewObject { .. }
        | InstructionKind::DeclareClass { .. }
        | InstructionKind::CloneObject { .. }
        | InstructionKind::CloneWith { .. }
        | InstructionKind::InstanceOf { .. }
        | InstructionKind::DynamicInstanceOf { .. }
        | InstructionKind::FetchProperty { .. }
        | InstructionKind::FetchDynamicProperty { .. }
        | InstructionKind::IssetProperty { .. }
        | InstructionKind::IssetDynamicProperty { .. }
        | InstructionKind::EmptyProperty { .. }
        | InstructionKind::EmptyDynamicProperty { .. }
        | InstructionKind::IssetPropertyDim { .. }
        | InstructionKind::IssetDynamicPropertyDim { .. }
        | InstructionKind::EmptyPropertyDim { .. }
        | InstructionKind::EmptyDynamicPropertyDim { .. }
        | InstructionKind::UnsetProperty { .. }
        | InstructionKind::UnsetPropertyDim { .. }
        | InstructionKind::UnsetDynamicProperty { .. }
        | InstructionKind::FetchStaticProperty { .. }
        | InstructionKind::FetchDynamicStaticProperty { .. }
        | InstructionKind::IssetStaticProperty { .. }
        | InstructionKind::IssetStaticPropertyDim { .. }
        | InstructionKind::EmptyStaticProperty { .. }
        | InstructionKind::EmptyStaticPropertyDim { .. }
        | InstructionKind::UnsetStaticPropertyDim { .. }
        | InstructionKind::FetchClassConstant { .. }
        | InstructionKind::FetchObjectClassName { .. }
        | InstructionKind::AssignProperty { .. }
        | InstructionKind::AssignPropertyDim { .. }
        | InstructionKind::AssignDynamicProperty { .. }
        | InstructionKind::AssignStaticProperty { .. }
        | InstructionKind::AssignDynamicStaticProperty { .. } => {
            rejected.push(JitEligibilityReason::instruction(
                "JIT_ELIGIBILITY_REJECT_OBJECT_OPCODE",
                "objects, properties, classes, and destructors are outside the JIT subset",
                block,
                id,
            ))
        }
        InstructionKind::FetchConst { .. }
        | InstructionKind::RegisterConstant { .. }
        | InstructionKind::DeclareFunction { .. }
        | InstructionKind::InitStaticLocal { .. }
        | InstructionKind::UnsetLocal { .. }
        | InstructionKind::Echo { .. }
        | InstructionKind::EmitDiagnostic { .. }
        | InstructionKind::Unsupported { .. }
        | InstructionKind::RuntimeError { .. } => rejected.push(JitEligibilityReason::instruction(
            "JIT_ELIGIBILITY_REJECT_OBSERVABLE_OPCODE",
            "observable or dynamic VM behavior is outside the JIT subset",
            block,
            id,
        )),
    }
}

fn check_terminator(
    terminator: &TerminatorKind,
    block: u32,
    constants: &[IrConstant],
    rejected: &mut Vec<JitEligibilityReason>,
    unknown: &mut Vec<JitEligibilityReason>,
) {
    match terminator {
        TerminatorKind::Jump { .. } => {}
        TerminatorKind::JumpIfFalse { condition, .. }
        | TerminatorKind::JumpIfTrue { condition, .. }
        | TerminatorKind::JumpIf { condition, .. } => {
            check_operand(*condition, block, u32::MAX, constants, rejected, unknown);
        }
        TerminatorKind::Return {
            value,
            by_ref_local,
        } => {
            if by_ref_local.is_some() {
                rejected.push(JitEligibilityReason::function(
                    "JIT_ELIGIBILITY_REJECT_BY_REF_RETURN",
                    "return terminator returns a local by reference; alias_state=escaped_reference",
                ));
            }
            if let Some(value) = value {
                check_operand(*value, block, u32::MAX, constants, rejected, unknown);
            }
        }
        TerminatorKind::Exit { value } => {
            rejected.push(JitEligibilityReason::function(
                "JIT_ELIGIBILITY_REJECT_EXIT_TERMINATOR",
                "exit terminator changes request control flow and is outside the JIT subset",
            ));
            if let Some(value) = value {
                check_operand(*value, block, u32::MAX, constants, rejected, unknown);
            }
        }
    }
}

fn check_instruction_operands(
    instruction: &Instruction,
    block: u32,
    constants: &[IrConstant],
    rejected: &mut Vec<JitEligibilityReason>,
    unknown: &mut Vec<JitEligibilityReason>,
) {
    let id = instruction.id.raw();
    match &instruction.kind {
        InstructionKind::Move { src, .. }
        | InstructionKind::StoreLocal { src, .. }
        | InstructionKind::Discard { src }
        | InstructionKind::Cast { src, .. }
        | InstructionKind::Unary { src, .. } => {
            check_operand(*src, block, id, constants, rejected, unknown);
        }
        InstructionKind::Binary { lhs, rhs, .. } | InstructionKind::Compare { lhs, rhs, .. } => {
            check_operand(*lhs, block, id, constants, rejected, unknown);
            check_operand(*rhs, block, id, constants, rejected, unknown);
        }
        _ => {}
    }
}

fn check_operand(
    operand: Operand,
    block: u32,
    instruction: u32,
    constants: &[IrConstant],
    rejected: &mut Vec<JitEligibilityReason>,
    unknown: &mut Vec<JitEligibilityReason>,
) {
    if let Operand::Constant(constant) = operand {
        check_constant(constant, block, instruction, constants, rejected, unknown);
    }
}

fn check_constant(
    constant: php_ir::ConstId,
    block: u32,
    instruction: u32,
    constants: &[IrConstant],
    rejected: &mut Vec<JitEligibilityReason>,
    unknown: &mut Vec<JitEligibilityReason>,
) {
    let Some(value) = constants.get(constant.index()) else {
        unknown.push(JitEligibilityReason::instruction(
            "JIT_ELIGIBILITY_UNKNOWN_CONSTANT",
            format!(
                "constant id {} is not present in the IR unit",
                constant.raw()
            ),
            block,
            instruction,
        ));
        return;
    };

    if !matches!(value, IrConstant::Int(_) | IrConstant::Bool(_)) {
        rejected.push(JitEligibilityReason::instruction(
            "JIT_ELIGIBILITY_REJECT_NON_PRIMITIVE_CONSTANT",
            format!("constant {value:?} is not an int/bool primitive"),
            block,
            instruction,
        ));
    }
}

fn is_allowed_binary(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Mod
    )
}

fn is_allowed_compare(op: CompareOp) -> bool {
    matches!(
        op,
        CompareOp::Identical
            | CompareOp::NotIdentical
            | CompareOp::Less
            | CompareOp::LessEqual
            | CompareOp::Greater
            | CompareOp::GreaterEqual
    )
}

fn is_allowed_unary(op: UnaryOp) -> bool {
    matches!(op, UnaryOp::Plus | UnaryOp::Minus | UnaryOp::Not)
}

fn unknown_report(
    function: FunctionId,
    function_name: Option<String>,
    reason: JitEligibilityReason,
) -> JitEligibilityReport {
    JitEligibilityReport {
        function,
        function_name,
        eligibility: JitEligibility::Unknown {
            reason: reason.clone(),
        },
        candidate_kind: None,
        reasons: vec![reason.clone()],
        stats: JitEligibilityStats {
            functions_analyzed: 0,
            unknown: 1,
            ..JitEligibilityStats::default()
        },
        debug: vec![
            "jit-eligibility function=<unknown> status=unknown".to_owned(),
            format!(
                "jit-eligibility reason code={} detail={}",
                reason.code, reason.detail
            ),
        ],
    }
}

/// Returns true when all call arguments stay in the future primitive intrinsic subset.
#[must_use]
pub fn call_args_are_jit_primitive(args: &[IrCallArg]) -> bool {
    args.iter()
        .all(|arg| arg.name.is_none() && !arg.unpack && arg.by_ref_local.is_none())
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped
}
