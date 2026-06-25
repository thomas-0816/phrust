//! performance conservative optimizer framework.
//!
//! The optimizer pass pipeline supports the first
//! conservative optimization pass.

use php_ir::instruction::TerminatorKind;
use php_ir::{
    BinaryOp, BlockId, ConstId, InstrId, InstructionKind, IrConstant, IrFunction, IrUnit, Operand,
    RegId, UnaryOp, VerificationError, verify_unit,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

/// Optimization level accepted by the performance CLI.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum OptimizationLevel {
    /// Exact legacy execution path: no optimizer pipeline is run.
    #[default]
    O0,
    /// Conservative performance optimizer pipeline.
    O1,
    /// Reserved higher conservative pipeline.
    O2,
}

impl OptimizationLevel {
    /// Stable CLI spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::O0 => "0",
            Self::O1 => "1",
            Self::O2 => "2",
        }
    }

    /// True when the optimizer pipeline should run.
    #[must_use]
    pub const fn runs_pipeline(self) -> bool {
        !matches!(self, Self::O0)
    }
}

impl fmt::Display for OptimizationLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for OptimizationLevel {
    type Err = ParseOptimizationLevelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "0" => Ok(Self::O0),
            "1" => Ok(Self::O1),
            "2" => Ok(Self::O2),
            _ => Err(ParseOptimizationLevelError {
                value: value.to_string(),
            }),
        }
    }
}

/// Invalid optimization level.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseOptimizationLevelError {
    value: String,
}

impl fmt::Display for ParseOptimizationLevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported optimization level `{}`; expected 0, 1, or 2",
            self.value
        )
    }
}

impl std::error::Error for ParseOptimizationLevelError {}

/// Position where a pass runs relative to the verifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PassPhase {
    /// Runs before the verifier check.
    PreVerify,
    /// Runs after a verifier check.
    PostVerify,
}

impl PassPhase {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreVerify => "pre_verify",
            Self::PostVerify => "post_verify",
        }
    }
}

/// Per-pass execution context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassContext {
    level: OptimizationLevel,
    enabled_only: Option<BTreeSet<&'static str>>,
    disabled: BTreeSet<&'static str>,
}

impl PassContext {
    /// Creates a context for one optimizer run.
    #[must_use]
    pub fn new(level: OptimizationLevel) -> Self {
        Self {
            level,
            enabled_only: None,
            disabled: BTreeSet::new(),
        }
    }

    /// Enables only the named passes.
    #[must_use]
    pub fn with_enabled_only(mut self, pass_names: impl IntoIterator<Item = &'static str>) -> Self {
        self.enabled_only = Some(pass_names.into_iter().collect());
        self
    }

    /// Disables the named passes.
    #[must_use]
    pub fn with_disabled(mut self, pass_names: impl IntoIterator<Item = &'static str>) -> Self {
        self.disabled = pass_names.into_iter().collect();
        self
    }

    /// Optimization level for this run.
    #[must_use]
    pub const fn level(&self) -> OptimizationLevel {
        self.level
    }

    /// True when a pass should run for this context.
    #[must_use]
    pub fn is_pass_enabled(&self, pass_name: &'static str) -> bool {
        if self.disabled.contains(pass_name) {
            return false;
        }
        self.enabled_only
            .as_ref()
            .is_none_or(|enabled| enabled.contains(pass_name))
    }
}

/// One optimizer pass.
pub trait OptimizerPass {
    /// Stable pass name.
    fn name(&self) -> &'static str;

    /// Relative verifier phase.
    fn phase(&self) -> PassPhase;

    /// Minimum optimization level required to run the pass.
    fn min_level(&self) -> OptimizationLevel {
        OptimizationLevel::O1
    }

    /// Runs the pass.
    fn run(&self, unit: &mut IrUnit, context: &PassContext) -> Result<PassReport, PassError>;
}

/// One pass report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassReport {
    /// Stable pass name.
    pub name: &'static str,
    /// Relative verifier phase.
    pub phase: PassPhase,
    /// Whether the pass ran instead of being disabled or below level.
    pub enabled: bool,
    /// Whether the pass changed the unit.
    pub changed: bool,
    /// Whether source mapping and file-span counts were preserved.
    pub source_spans_preserved: bool,
    /// Deterministic pass statistics.
    pub stats: BTreeMap<&'static str, u64>,
}

impl PassReport {
    fn skipped(name: &'static str, phase: PassPhase) -> Self {
        Self {
            name,
            phase,
            enabled: false,
            changed: false,
            source_spans_preserved: true,
            stats: BTreeMap::new(),
        }
    }
}

/// Full optimizer report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptimizationReport {
    /// Requested optimization level.
    pub level: OptimizationLevel,
    /// Reports in pass execution order.
    pub passes: Vec<PassReport>,
}

impl OptimizationReport {
    /// Number of passes that actually ran.
    #[must_use]
    pub fn enabled_pass_count(&self) -> usize {
        self.passes.iter().filter(|pass| pass.enabled).count()
    }
}

/// Optimizer failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PassError {
    /// A pass rejected the current unit.
    PassFailed {
        /// Stable pass name.
        pass: &'static str,
        /// Human-readable reason.
        message: String,
    },
    /// Verifier failed before or after a pass phase.
    Verification {
        /// Relative verifier phase.
        phase: PassPhase,
        /// Verifier errors.
        errors: Vec<VerificationError>,
    },
}

impl fmt::Display for PassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PassFailed { pass, message } => write!(f, "{pass}: {message}"),
            Self::Verification { phase, errors } => write!(
                f,
                "optimizer {} verifier failed with {} error(s)",
                phase.as_str(),
                errors.len()
            ),
        }
    }
}

impl std::error::Error for PassError {}

/// Ordered pass pipeline.
pub struct PassPipeline {
    passes: Vec<Box<dyn OptimizerPass>>,
}

impl PassPipeline {
    /// Creates a pipeline from explicit passes.
    #[must_use]
    pub fn new(passes: Vec<Box<dyn OptimizerPass>>) -> Self {
        Self { passes }
    }

    /// conservative no-op optimizer pipeline.
    #[must_use]
    pub fn noop() -> Self {
        Self::new(vec![
            Box::new(NoopPass::new("perf_pre_verify_noop", PassPhase::PreVerify)),
            Box::new(NoopPass::new(
                "perf_post_verify_noop",
                PassPhase::PostVerify,
            )),
        ])
    }

    /// Current performance optimizer pipeline.
    #[must_use]
    pub fn performance() -> Self {
        Self::new(vec![
            Box::new(NoopPass::new("perf_pre_verify_noop", PassPhase::PreVerify)),
            Box::new(ConstantFoldingPass),
            Box::new(PeepholeSimplify),
            Box::new(BranchSimplify),
            Box::new(NoopPass::new(
                "perf_post_verify_noop",
                PassPhase::PostVerify,
            )),
        ])
    }

    /// Runs passes around verifier boundaries.
    pub fn run(
        &self,
        unit: &mut IrUnit,
        context: &PassContext,
    ) -> Result<OptimizationReport, PassError> {
        let mut reports = Vec::new();
        self.run_phase(PassPhase::PreVerify, unit, context, &mut reports)?;
        verify_unit(unit).map_err(|errors| PassError::Verification {
            phase: PassPhase::PreVerify,
            errors,
        })?;
        self.run_phase(PassPhase::PostVerify, unit, context, &mut reports)?;
        verify_unit(unit).map_err(|errors| PassError::Verification {
            phase: PassPhase::PostVerify,
            errors,
        })?;
        Ok(OptimizationReport {
            level: context.level(),
            passes: reports,
        })
    }

    fn run_phase(
        &self,
        phase: PassPhase,
        unit: &mut IrUnit,
        context: &PassContext,
        reports: &mut Vec<PassReport>,
    ) -> Result<(), PassError> {
        for pass in self.passes.iter().filter(|pass| pass.phase() == phase) {
            if context.level() < pass.min_level() || !context.is_pass_enabled(pass.name()) {
                reports.push(PassReport::skipped(pass.name(), pass.phase()));
                continue;
            }
            reports.push(pass.run(unit, context)?);
        }
        Ok(())
    }
}

/// No-op pass used until real optimizations land.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoopPass {
    name: &'static str,
    phase: PassPhase,
}

impl NoopPass {
    /// Creates a named no-op pass.
    #[must_use]
    pub const fn new(name: &'static str, phase: PassPhase) -> Self {
        Self { name, phase }
    }
}

impl OptimizerPass for NoopPass {
    fn name(&self) -> &'static str {
        self.name
    }

    fn phase(&self) -> PassPhase {
        self.phase
    }

    fn run(&self, unit: &mut IrUnit, _context: &PassContext) -> Result<PassReport, PassError> {
        let before = unit.clone();
        let mut stats = BTreeMap::new();
        stats.insert("functions", unit.functions.len() as u64);
        stats.insert(
            "blocks",
            unit.functions
                .iter()
                .map(|function| function.blocks.len() as u64)
                .sum(),
        );
        stats.insert(
            "instructions",
            unit.functions
                .iter()
                .flat_map(|function| &function.blocks)
                .map(|block| block.instructions.len() as u64)
                .sum(),
        );
        stats.insert("source_map_entries", unit.source_map.entries().len() as u64);

        Ok(PassReport {
            name: self.name,
            phase: self.phase,
            enabled: true,
            changed: before != *unit,
            source_spans_preserved: before.files == unit.files
                && before.source_map == unit.source_map,
            stats,
        })
    }
}

/// Conservative constant folding for operations with no observable diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstantFoldingPass;

impl OptimizerPass for ConstantFoldingPass {
    fn name(&self) -> &'static str {
        "constant_folding_safe_subset"
    }

    fn phase(&self) -> PassPhase {
        PassPhase::PreVerify
    }

    fn run(&self, unit: &mut IrUnit, _context: &PassContext) -> Result<PassReport, PassError> {
        let before_files = unit.files.clone();
        let before_source_map = unit.source_map.clone();
        let mut constants = unit.constants.clone();
        let mut stats = ConstantFoldingStats::default();

        for function in &mut unit.functions {
            for block in &mut function.blocks {
                let mut known_constants = BTreeMap::<RegId, ConstId>::new();
                for instruction in &mut block.instructions {
                    match &instruction.kind {
                        InstructionKind::LoadConst { dst, constant } => {
                            known_constants.insert(*dst, *constant);
                            continue;
                        }
                        InstructionKind::Binary { dst, op, lhs, rhs } => {
                            let dst = *dst;
                            known_constants.remove(&dst);
                            let lhs = resolve_constant(*lhs, &known_constants);
                            let rhs = resolve_constant(*rhs, &known_constants);
                            if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                                match fold_binary(*op, lhs, rhs, &mut constants) {
                                    Some((constant, kind)) => {
                                        instruction.kind =
                                            InstructionKind::LoadConst { dst, constant };
                                        known_constants.insert(dst, constant);
                                        stats.record(kind);
                                    }
                                    None => stats.skipped_unsafe += 1,
                                }
                            }
                            continue;
                        }
                        InstructionKind::Unary { dst, op, src } => {
                            let dst = *dst;
                            known_constants.remove(&dst);
                            if let Some(src) = resolve_constant(*src, &known_constants) {
                                match fold_unary(*op, src, &mut constants) {
                                    Some((constant, kind)) => {
                                        instruction.kind =
                                            InstructionKind::LoadConst { dst, constant };
                                        known_constants.insert(dst, constant);
                                        stats.record(kind);
                                    }
                                    None => stats.skipped_unsafe += 1,
                                }
                            }
                            continue;
                        }
                        _ => {}
                    }

                    for register in defined_registers(&instruction.kind) {
                        known_constants.remove(&register);
                    }
                }
            }
        }

        unit.constants = constants;
        let total_folded = stats.total_folded();
        Ok(PassReport {
            name: self.name(),
            phase: self.phase(),
            enabled: true,
            changed: total_folded > 0,
            source_spans_preserved: before_files == unit.files
                && before_source_map == unit.source_map,
            stats: stats.into_report_stats(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FoldKind {
    IntegerBinary,
    BoolNot,
    StringConcat,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ConstantFoldingStats {
    integer_binary_folded: u64,
    bool_not_folded: u64,
    string_concat_folded: u64,
    skipped_unsafe: u64,
}

impl ConstantFoldingStats {
    fn record(&mut self, kind: FoldKind) {
        match kind {
            FoldKind::IntegerBinary => self.integer_binary_folded += 1,
            FoldKind::BoolNot => self.bool_not_folded += 1,
            FoldKind::StringConcat => self.string_concat_folded += 1,
        }
    }

    fn total_folded(&self) -> u64 {
        self.integer_binary_folded + self.bool_not_folded + self.string_concat_folded
    }

    fn into_report_stats(self) -> BTreeMap<&'static str, u64> {
        BTreeMap::from([
            ("bool_not_folded", self.bool_not_folded),
            ("integer_binary_folded", self.integer_binary_folded),
            ("skipped_unsafe", self.skipped_unsafe),
            ("string_concat_folded", self.string_concat_folded),
            ("total_folded", self.total_folded()),
        ])
    }
}

fn resolve_constant(
    operand: Operand,
    known_constants: &BTreeMap<RegId, ConstId>,
) -> Option<ConstId> {
    match operand {
        Operand::Constant(constant) => Some(constant),
        Operand::Register(register) => known_constants.get(&register).copied(),
        Operand::Local(_) => None,
    }
}

fn fold_binary(
    op: BinaryOp,
    lhs: ConstId,
    rhs: ConstId,
    constants: &mut Vec<IrConstant>,
) -> Option<(ConstId, FoldKind)> {
    let folded = match (op, constants.get(lhs.index())?, constants.get(rhs.index())?) {
        (BinaryOp::Add, IrConstant::Int(lhs), IrConstant::Int(rhs)) => {
            IrConstant::Int(lhs.checked_add(*rhs)?)
        }
        (BinaryOp::Sub, IrConstant::Int(lhs), IrConstant::Int(rhs)) => {
            IrConstant::Int(lhs.checked_sub(*rhs)?)
        }
        (BinaryOp::Mul, IrConstant::Int(lhs), IrConstant::Int(rhs)) => {
            IrConstant::Int(lhs.checked_mul(*rhs)?)
        }
        (BinaryOp::Concat, IrConstant::String(lhs), IrConstant::String(rhs)) => {
            let mut value = String::with_capacity(lhs.len().checked_add(rhs.len())?);
            value.push_str(lhs);
            value.push_str(rhs);
            let constant = append_constant(constants, IrConstant::String(value))?;
            return Some((constant, FoldKind::StringConcat));
        }
        _ => return None,
    };

    let constant = append_constant(constants, folded)?;
    Some((constant, FoldKind::IntegerBinary))
}

fn fold_unary(
    op: UnaryOp,
    src: ConstId,
    constants: &mut Vec<IrConstant>,
) -> Option<(ConstId, FoldKind)> {
    match (op, constants.get(src.index())?) {
        (UnaryOp::Not, IrConstant::Bool(value)) => {
            let constant = append_constant(constants, IrConstant::Bool(!value))?;
            Some((constant, FoldKind::BoolNot))
        }
        _ => None,
    }
}

fn append_constant(constants: &mut Vec<IrConstant>, constant: IrConstant) -> Option<ConstId> {
    let index = u32::try_from(constants.len()).ok()?;
    constants.push(constant);
    Some(ConstId::new(index))
}

fn defined_registers(kind: &InstructionKind) -> Vec<RegId> {
    match kind {
        InstructionKind::LoadConst { dst, .. }
        | InstructionKind::FetchConst { dst, .. }
        | InstructionKind::Move { dst, .. }
        | InstructionKind::LoadLocal { dst, .. }
        | InstructionKind::LoadLocalQuiet { dst, .. }
        | InstructionKind::Binary { dst, .. }
        | InstructionKind::Compare { dst, .. }
        | InstructionKind::InstanceOf { dst, .. }
        | InstructionKind::Unary { dst, .. }
        | InstructionKind::Cast { dst, .. }
        | InstructionKind::Yield { dst, .. }
        | InstructionKind::YieldFrom { dst, .. }
        | InstructionKind::CallFunction { dst, .. }
        | InstructionKind::CallMethod { dst, .. }
        | InstructionKind::CallStaticMethod { dst, .. }
        | InstructionKind::CloneObject { dst, .. }
        | InstructionKind::CloneWith { dst, .. }
        | InstructionKind::MakeException { dst, .. }
        | InstructionKind::MakeClosure { dst, .. }
        | InstructionKind::CallClosure { dst, .. }
        | InstructionKind::ResolveCallable { dst, .. }
        | InstructionKind::CallCallable { dst, .. }
        | InstructionKind::Pipe { dst, .. }
        | InstructionKind::Include { dst, .. }
        | InstructionKind::Eval { dst, .. }
        | InstructionKind::NewObject { dst, .. }
        | InstructionKind::DynamicNewObject { dst, .. }
        | InstructionKind::FetchProperty { dst, .. }
        | InstructionKind::IssetProperty { dst, .. }
        | InstructionKind::EmptyProperty { dst, .. }
        | InstructionKind::FetchStaticProperty { dst, .. }
        | InstructionKind::FetchClassConstant { dst, .. }
        | InstructionKind::AssignProperty { dst, .. }
        | InstructionKind::AssignStaticProperty { dst, .. }
        | InstructionKind::NewArray { dst }
        | InstructionKind::FetchDim { dst, .. }
        | InstructionKind::AssignDim { dst, .. }
        | InstructionKind::AppendDim { dst, .. }
        | InstructionKind::IssetLocal { dst, .. }
        | InstructionKind::EmptyLocal { dst, .. }
        | InstructionKind::IssetDim { dst, .. }
        | InstructionKind::EmptyDim { dst, .. }
        | InstructionKind::ArrayGet { dst, .. } => vec![*dst],
        InstructionKind::ArrayInsert { array, .. } => vec![*array],
        InstructionKind::ForeachInit { iterator, .. }
        | InstructionKind::ForeachInitRef { iterator, .. } => vec![*iterator],
        InstructionKind::ForeachNext {
            has_value,
            iterator,
            key,
            value,
        } => {
            let mut registers = vec![*has_value, *iterator, *value];
            if let Some(key) = key {
                registers.push(*key);
            }
            registers
        }
        InstructionKind::ForeachNextRef {
            has_value,
            iterator,
            key,
            ..
        } => {
            let mut registers = vec![*has_value, *iterator];
            if let Some(key) = key {
                registers.push(*key);
            }
            registers
        }
        InstructionKind::Nop
        | InstructionKind::StoreLocal { .. }
        | InstructionKind::BindReference { .. }
        | InstructionKind::BindGlobal { .. }
        | InstructionKind::BindReferenceDim { .. }
        | InstructionKind::BindReferenceFromDim { .. }
        | InstructionKind::BindReferenceFromCall { .. }
        | InstructionKind::InitStaticLocal { .. }
        | InstructionKind::Discard { .. }
        | InstructionKind::Echo { .. }
        | InstructionKind::EmitDiagnostic { .. }
        | InstructionKind::EnterTry { .. }
        | InstructionKind::LeaveTry
        | InstructionKind::EndFinally { .. }
        | InstructionKind::Throw { .. }
        | InstructionKind::UnsetProperty { .. }
        | InstructionKind::UnsetLocal { .. }
        | InstructionKind::UnsetDim { .. }
        | InstructionKind::Unsupported { .. }
        | InstructionKind::RuntimeError { .. } => Vec::new(),
    }
}

/// Peephole simplification for trivially side-effect-free IR patterns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeepholeSimplify;

impl OptimizerPass for PeepholeSimplify {
    fn name(&self) -> &'static str {
        "peephole_simplify"
    }

    fn phase(&self) -> PassPhase {
        PassPhase::PreVerify
    }

    fn run(&self, unit: &mut IrUnit, _context: &PassContext) -> Result<PassReport, PassError> {
        let before_files = unit.files.clone();
        let before_source_map = unit.source_map.clone();
        let mut stats = PeepholeStats::default();

        while let Some(peephole) = find_peephole(unit) {
            let before_transform = unit.clone();
            apply_peephole(unit, peephole);
            if let Err(errors) = verify_unit(unit) {
                *unit = before_transform;
                return Err(PassError::Verification {
                    phase: self.phase(),
                    errors,
                });
            }
            stats.record(peephole);
        }

        let total = stats.total_transformations();
        Ok(PassReport {
            name: self.name(),
            phase: self.phase(),
            enabled: true,
            changed: total > 0,
            source_spans_preserved: before_files == unit.files
                && before_source_map == unit.source_map,
            stats: stats.into_report_stats(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Peephole {
    RemoveNop {
        function: usize,
        block: usize,
        instruction: usize,
    },
    RemoveSelfMove {
        function: usize,
        block: usize,
        instruction: usize,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PeepholeStats {
    noops_removed: u64,
    self_moves_removed: u64,
}

impl PeepholeStats {
    fn record(&mut self, peephole: Peephole) {
        match peephole {
            Peephole::RemoveNop { .. } => self.noops_removed += 1,
            Peephole::RemoveSelfMove { .. } => self.self_moves_removed += 1,
        }
    }

    fn total_transformations(&self) -> u64 {
        self.noops_removed + self.self_moves_removed
    }

    fn into_report_stats(self) -> BTreeMap<&'static str, u64> {
        BTreeMap::from([
            ("noops_removed", self.noops_removed),
            ("self_moves_removed", self.self_moves_removed),
            ("total_transformations", self.total_transformations()),
        ])
    }
}

fn find_peephole(unit: &IrUnit) -> Option<Peephole> {
    for (function_index, function) in unit.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (instruction_index, instruction) in block.instructions.iter().enumerate() {
                match instruction.kind {
                    InstructionKind::Nop => {
                        return Some(Peephole::RemoveNop {
                            function: function_index,
                            block: block_index,
                            instruction: instruction_index,
                        });
                    }
                    InstructionKind::Move {
                        dst,
                        src: Operand::Register(src),
                    } if dst == src => {
                        return Some(Peephole::RemoveSelfMove {
                            function: function_index,
                            block: block_index,
                            instruction: instruction_index,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

fn apply_peephole(unit: &mut IrUnit, peephole: Peephole) {
    let (function, block, instruction) = match peephole {
        Peephole::RemoveNop {
            function,
            block,
            instruction,
        }
        | Peephole::RemoveSelfMove {
            function,
            block,
            instruction,
        } => (function, block, instruction),
    };
    let block = &mut unit.functions[function].blocks[block];
    block.instructions.remove(instruction);
    renumber_instructions(block);
}

fn renumber_instructions(block: &mut php_ir::BasicBlock) {
    for (index, instruction) in block.instructions.iter_mut().enumerate() {
        instruction.id = InstrId::new(index as u32);
    }
}

/// Conservative branch simplification backed by a minimal CFG view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchSimplify;

impl OptimizerPass for BranchSimplify {
    fn name(&self) -> &'static str {
        "branch_simplify"
    }

    fn phase(&self) -> PassPhase {
        PassPhase::PreVerify
    }

    fn run(&self, unit: &mut IrUnit, _context: &PassContext) -> Result<PassReport, PassError> {
        let before_files = unit.files.clone();
        let before_source_map = unit.source_map.clone();
        let mut stats = BranchSimplifyStats::default();

        while let Some(simplification) = find_branch_simplification(unit) {
            let before_transform = unit.clone();
            apply_branch_simplification(unit, simplification);
            if let Err(errors) = verify_unit(unit) {
                *unit = before_transform;
                return Err(PassError::Verification {
                    phase: self.phase(),
                    errors,
                });
            }
            stats.record(simplification);
        }

        let total = stats.total_transformations();
        Ok(PassReport {
            name: self.name(),
            phase: self.phase(),
            enabled: true,
            changed: total > 0,
            source_spans_preserved: before_files == unit.files
                && before_source_map == unit.source_map,
            stats: stats.into_report_stats(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CfgView {
    successors: Vec<Vec<BlockId>>,
    predecessors: Vec<Vec<BlockId>>,
    reachable: Vec<bool>,
}

impl CfgView {
    fn new(function: &IrFunction) -> Self {
        let successors: Vec<Vec<BlockId>> = (0..function.blocks.len())
            .map(|index| block_successors(function, index))
            .collect();
        let mut predecessors = vec![Vec::new(); function.blocks.len()];
        for (source, targets) in successors.iter().enumerate() {
            for target in targets {
                if target.index() < predecessors.len() {
                    predecessors[target.index()].push(BlockId::new(source as u32));
                }
            }
        }
        let mut reachable = vec![false; function.blocks.len()];
        let mut stack = if function.blocks.is_empty() {
            Vec::new()
        } else {
            vec![BlockId::new(0)]
        };
        while let Some(block) = stack.pop() {
            let index = block.index();
            if index >= reachable.len() || reachable[index] {
                continue;
            }
            reachable[index] = true;
            for successor in &successors[index] {
                stack.push(*successor);
            }
        }
        Self {
            successors,
            predecessors,
            reachable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BranchSimplification {
    ConstantBranch {
        function: usize,
        block: usize,
        target: BlockId,
    },
    ForwardEmptyBlock {
        function: usize,
        block: usize,
        old_target: BlockId,
        new_target: BlockId,
    },
    RemoveUnreachableEmptyTail {
        function: usize,
        new_len: usize,
        removed: usize,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct BranchSimplifyStats {
    constant_branches: u64,
    empty_block_forwards: u64,
    unreachable_empty_tail_blocks_removed: u64,
}

impl BranchSimplifyStats {
    fn record(&mut self, simplification: BranchSimplification) {
        match simplification {
            BranchSimplification::ConstantBranch { .. } => self.constant_branches += 1,
            BranchSimplification::ForwardEmptyBlock { .. } => self.empty_block_forwards += 1,
            BranchSimplification::RemoveUnreachableEmptyTail { removed, .. } => {
                self.unreachable_empty_tail_blocks_removed += removed as u64;
            }
        }
    }

    fn total_transformations(&self) -> u64 {
        self.constant_branches
            + self.empty_block_forwards
            + self.unreachable_empty_tail_blocks_removed
    }

    fn into_report_stats(self) -> BTreeMap<&'static str, u64> {
        BTreeMap::from([
            ("constant_branches", self.constant_branches),
            ("empty_block_forwards", self.empty_block_forwards),
            (
                "unreachable_empty_tail_blocks_removed",
                self.unreachable_empty_tail_blocks_removed,
            ),
            ("total_transformations", self.total_transformations()),
        ])
    }
}

fn find_branch_simplification(unit: &IrUnit) -> Option<BranchSimplification> {
    for (function_index, function) in unit.functions.iter().enumerate() {
        let cfg = CfgView::new(function);
        for (block_index, block) in function.blocks.iter().enumerate() {
            if block_has_exception_boundary(block) {
                continue;
            }
            if let Some(terminator) = &block.terminator
                && let Some(target) =
                    constant_branch_target(function, block_index, &terminator.kind, &unit.constants)
            {
                return Some(BranchSimplification::ConstantBranch {
                    function: function_index,
                    block: block_index,
                    target,
                });
            }
        }

        for (block_index, block) in function.blocks.iter().enumerate() {
            let Some(terminator) = &block.terminator else {
                continue;
            };
            for target in terminator_explicit_targets(&terminator.kind) {
                let target_index = target.index();
                if target_index >= function.blocks.len() {
                    continue;
                }
                let target_block = &function.blocks[target_index];
                if block_has_exception_boundary(block) || block_has_exception_boundary(target_block)
                {
                    continue;
                }
                if target_block.instructions.is_empty()
                    && let Some(target_terminator) = &target_block.terminator
                    && let TerminatorKind::Jump { target: new_target } = target_terminator.kind
                    && new_target != target
                {
                    return Some(BranchSimplification::ForwardEmptyBlock {
                        function: function_index,
                        block: block_index,
                        old_target: target,
                        new_target,
                    });
                }
            }
        }

        if let Some(simplification) =
            unreachable_empty_tail_simplification(function_index, function, &cfg)
        {
            return Some(simplification);
        }
    }
    None
}

fn apply_branch_simplification(unit: &mut IrUnit, simplification: BranchSimplification) {
    match simplification {
        BranchSimplification::ConstantBranch {
            function,
            block,
            target,
        } => {
            let block = &mut unit.functions[function].blocks[block];
            let span = block
                .terminator
                .as_ref()
                .map(|terminator| terminator.span)
                .expect("branch simplification requires a terminator");
            block.terminator = Some(php_ir::Terminator {
                span,
                kind: TerminatorKind::Jump { target },
            });
        }
        BranchSimplification::ForwardEmptyBlock {
            function,
            block,
            old_target,
            new_target,
        } => {
            let terminator = unit.functions[function].blocks[block]
                .terminator
                .as_mut()
                .expect("forwarding simplification requires a terminator");
            replace_terminator_target(&mut terminator.kind, old_target, new_target);
        }
        BranchSimplification::RemoveUnreachableEmptyTail {
            function, new_len, ..
        } => {
            unit.functions[function].blocks.truncate(new_len);
        }
    }
}

fn block_successors(function: &IrFunction, block_index: usize) -> Vec<BlockId> {
    let Some(terminator) = &function.blocks[block_index].terminator else {
        return Vec::new();
    };
    let next = || {
        let next_index = block_index + 1;
        (next_index < function.blocks.len()).then(|| BlockId::new(next_index as u32))
    };
    match terminator.kind {
        TerminatorKind::Jump { target } => vec![target],
        TerminatorKind::JumpIfFalse { target, .. } | TerminatorKind::JumpIfTrue { target, .. } => {
            let mut targets = vec![target];
            if let Some(next) = next() {
                targets.push(next);
            }
            targets
        }
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => vec![if_true, if_false],
        TerminatorKind::Return { .. } => Vec::new(),
    }
}

fn terminator_explicit_targets(kind: &TerminatorKind) -> Vec<BlockId> {
    match kind {
        TerminatorKind::Jump { target }
        | TerminatorKind::JumpIfFalse { target, .. }
        | TerminatorKind::JumpIfTrue { target, .. } => vec![*target],
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => vec![*if_true, *if_false],
        TerminatorKind::Return { .. } => Vec::new(),
    }
}

fn constant_branch_target(
    function: &IrFunction,
    block_index: usize,
    kind: &TerminatorKind,
    constants: &[IrConstant],
) -> Option<BlockId> {
    let bool_value = match kind {
        TerminatorKind::JumpIfFalse { condition, .. }
        | TerminatorKind::JumpIfTrue { condition, .. }
        | TerminatorKind::JumpIf { condition, .. } => {
            condition_bool_value(function, block_index, *condition, constants)?
        }
        TerminatorKind::Jump { .. } | TerminatorKind::Return { .. } => return None,
    };
    match kind {
        TerminatorKind::JumpIfFalse { target, .. } => {
            if bool_value {
                next_block(function, block_index)
            } else {
                Some(*target)
            }
        }
        TerminatorKind::JumpIfTrue { target, .. } => {
            if bool_value {
                Some(*target)
            } else {
                next_block(function, block_index)
            }
        }
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => Some(if bool_value { *if_true } else { *if_false }),
        TerminatorKind::Jump { .. } | TerminatorKind::Return { .. } => None,
    }
}

fn condition_bool_value(
    function: &IrFunction,
    block_index: usize,
    condition: Operand,
    constants: &[IrConstant],
) -> Option<bool> {
    let constant = match condition {
        Operand::Constant(constant) => constant,
        Operand::Register(register) => {
            block_register_bool_constant(&function.blocks[block_index], register)?
        }
        Operand::Local(_) => return None,
    };
    match constants.get(constant.index())? {
        IrConstant::Bool(value) => Some(*value),
        _ => None,
    }
}

fn block_register_bool_constant(block: &php_ir::BasicBlock, register: RegId) -> Option<ConstId> {
    for instruction in block.instructions.iter().rev() {
        match instruction.kind {
            InstructionKind::LoadConst { dst, constant } if dst == register => {
                return Some(constant);
            }
            _ if defined_registers(&instruction.kind).contains(&register) => return None,
            _ => continue,
        }
    }
    None
}

fn next_block(function: &IrFunction, block_index: usize) -> Option<BlockId> {
    let next_index = block_index + 1;
    (next_index < function.blocks.len()).then(|| BlockId::new(next_index as u32))
}

fn replace_terminator_target(kind: &mut TerminatorKind, old_target: BlockId, new_target: BlockId) {
    match kind {
        TerminatorKind::Jump { target }
        | TerminatorKind::JumpIfFalse { target, .. }
        | TerminatorKind::JumpIfTrue { target, .. } => {
            if *target == old_target {
                *target = new_target;
            }
        }
        TerminatorKind::JumpIf {
            if_true, if_false, ..
        } => {
            if *if_true == old_target {
                *if_true = new_target;
            }
            if *if_false == old_target {
                *if_false = new_target;
            }
        }
        TerminatorKind::Return { .. } => {}
    }
}

fn unreachable_empty_tail_simplification(
    function_index: usize,
    function: &IrFunction,
    cfg: &CfgView,
) -> Option<BranchSimplification> {
    let last_reachable = cfg
        .reachable
        .iter()
        .rposition(|reachable| *reachable)
        .unwrap_or(0);
    let new_len = last_reachable + 1;
    if new_len >= function.blocks.len() {
        return None;
    }
    if kept_blocks_reference_removed_tail(function, new_len) {
        return None;
    }
    let tail = &function.blocks[new_len..];
    if tail.iter().all(|block| {
        block.instructions.is_empty()
            && block.terminator.is_some()
            && !block_has_exception_boundary(block)
    }) {
        return Some(BranchSimplification::RemoveUnreachableEmptyTail {
            function: function_index,
            new_len,
            removed: tail.len(),
        });
    }
    None
}

fn kept_blocks_reference_removed_tail(function: &IrFunction, new_len: usize) -> bool {
    function.blocks[..new_len]
        .iter()
        .flat_map(|block| &block.instructions)
        .flat_map(instruction_metadata_targets)
        .any(|target| target.index() >= new_len)
}

fn instruction_metadata_targets(instruction: &php_ir::Instruction) -> Vec<BlockId> {
    match &instruction.kind {
        InstructionKind::EnterTry {
            catch,
            finally,
            after,
            ..
        } => {
            let mut targets = vec![*after];
            if let Some(catch) = catch {
                targets.push(*catch);
            }
            if let Some(finally) = finally {
                targets.push(*finally);
            }
            targets
        }
        InstructionKind::EndFinally { after } => vec![*after],
        _ => Vec::new(),
    }
}

fn block_has_exception_boundary(block: &php_ir::BasicBlock) -> bool {
    block.instructions.iter().any(|instruction| {
        matches!(
            instruction.kind,
            InstructionKind::EnterTry { .. }
                | InstructionKind::LeaveTry
                | InstructionKind::EndFinally { .. }
                | InstructionKind::Throw { .. }
                | InstructionKind::MakeException { .. }
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ConstantFoldingPass, NoopPass, OptimizationLevel, OptimizerPass, PassContext, PassPhase,
        PassPipeline, PeepholeSimplify,
    };
    use php_ir::instruction::TerminatorKind;
    use php_ir::{
        BinaryOp, FunctionFlags, InstructionKind, IrBuilder, IrConstant, IrSpan, Operand, UnaryOp,
        UnitId,
    };

    fn simple_unit() -> php_ir::IrUnit {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("optimizer/noop.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let block = builder.append_block(function);
        let constant = builder.add_constant(IrConstant::String("noop".to_string()));
        let register = builder.alloc_register(function);
        builder.emit_load_const(
            function,
            block,
            register,
            constant,
            IrSpan::new(file, 6, 12),
        );
        builder.terminate_return(
            function,
            block,
            Some(Operand::Register(register)),
            IrSpan::new(file, 6, 12),
        );
        builder.set_entry(function);
        builder.finish()
    }

    fn folding_unit(kind: InstructionKind) -> php_ir::IrUnit {
        let mut builder = IrBuilder::new(UnitId::new(1));
        let file = builder.add_file("optimizer/folding.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let block = builder.append_block(function);
        builder.emit(function, block, kind, IrSpan::new(file, 6, 12));
        builder.terminate_return(function, block, None, IrSpan::new(file, 13, 14));
        builder.set_entry(function);
        builder.finish()
    }

    fn constant(unit: &php_ir::IrUnit, index: usize) -> &IrConstant {
        &unit.constants[index]
    }

    #[test]
    fn optimization_levels_parse_stable_cli_values() {
        assert_eq!("0".parse(), Ok(OptimizationLevel::O0));
        assert_eq!("1".parse(), Ok(OptimizationLevel::O1));
        assert_eq!("2".parse(), Ok(OptimizationLevel::O2));
        assert!("3".parse::<OptimizationLevel>().is_err());
        assert_eq!(OptimizationLevel::O1.as_str(), "1");
        assert!(OptimizationLevel::O0 < OptimizationLevel::O1);
    }

    #[test]
    fn noop_pipeline_reports_without_changing_ir_or_spans() {
        let mut unit = simple_unit();
        let before = unit.clone();
        let report = PassPipeline::noop()
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("noop pipeline should pass");

        assert_eq!(unit, before);
        assert_eq!(report.level, OptimizationLevel::O1);
        assert_eq!(report.enabled_pass_count(), 2);
        assert_eq!(report.passes.len(), 2);
        assert!(report.passes.iter().all(|pass| !pass.changed));
        assert!(report.passes.iter().all(|pass| pass.source_spans_preserved));
        assert_eq!(report.passes[0].phase, PassPhase::PreVerify);
        assert_eq!(report.passes[1].phase, PassPhase::PostVerify);
        assert_eq!(report.passes[0].stats["functions"], 1);
    }

    #[test]
    fn passes_can_be_individually_disabled_or_enabled() {
        let mut unit = simple_unit();
        let report = PassPipeline::noop()
            .run(
                &mut unit,
                &PassContext::new(OptimizationLevel::O1).with_disabled(["perf_post_verify_noop"]),
            )
            .expect("disabled pass should be skipped");

        assert_eq!(report.enabled_pass_count(), 1);
        assert!(report.passes[0].enabled);
        assert!(!report.passes[1].enabled);

        let mut unit = simple_unit();
        let report = PassPipeline::noop()
            .run(
                &mut unit,
                &PassContext::new(OptimizationLevel::O1)
                    .with_enabled_only(["perf_post_verify_noop"]),
            )
            .expect("enabled-only pass should run");

        assert_eq!(report.enabled_pass_count(), 1);
        assert!(!report.passes[0].enabled);
        assert!(report.passes[1].enabled);
    }

    #[test]
    fn level_zero_context_skips_noop_passes() {
        let mut unit = simple_unit();
        let report = PassPipeline::noop()
            .run(&mut unit, &PassContext::new(OptimizationLevel::O0))
            .expect("level zero still verifies");

        assert_eq!(report.enabled_pass_count(), 0);
        assert_eq!(report.passes.len(), 2);
    }

    #[test]
    fn direct_noop_pass_preserves_unit() {
        let mut unit = simple_unit();
        let before = unit.clone();
        let report = NoopPass::new("direct_noop", PassPhase::PreVerify)
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("noop pass should pass");

        assert_eq!(unit, before);
        assert!(report.enabled);
        assert!(!report.changed);
        assert!(report.source_spans_preserved);
    }

    #[test]
    fn perf_pipeline_runs_constant_folding_between_verifiers() {
        let mut unit = simple_unit();
        let report = PassPipeline::performance()
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("performance pipeline should pass");

        assert_eq!(report.enabled_pass_count(), 5);
        assert_eq!(report.passes[1].name, "constant_folding_safe_subset");
        assert_eq!(report.passes[1].phase, PassPhase::PreVerify);
        assert_eq!(report.passes[1].stats["total_folded"], 0);
        assert_eq!(report.passes[2].name, "peephole_simplify");
        assert_eq!(report.passes[2].stats["total_transformations"], 0);
        assert_eq!(report.passes[3].name, "branch_simplify");
        assert_eq!(report.passes[3].stats["total_transformations"], 0);
    }

    #[test]
    fn folds_safe_integer_binary_without_overflow() {
        let mut unit = folding_unit(InstructionKind::Binary {
            dst: php_ir::RegId::new(0),
            op: BinaryOp::Mul,
            lhs: Operand::Constant(php_ir::ConstId::new(0)),
            rhs: Operand::Constant(php_ir::ConstId::new(1)),
        });
        unit.constants = vec![IrConstant::Int(6), IrConstant::Int(7)];

        let report = ConstantFoldingPass
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("folding should pass");

        assert!(report.changed);
        assert_eq!(report.stats["integer_binary_folded"], 1);
        assert_eq!(constant(&unit, 2), &IrConstant::Int(42));
        assert!(matches!(
            unit.functions[0].blocks[0].instructions[0].kind,
            InstructionKind::LoadConst {
                constant,
                ..
            } if constant == php_ir::ConstId::new(2)
        ));
    }

    #[test]
    fn folds_bool_not_and_string_concat() {
        let mut unit = folding_unit(InstructionKind::Unary {
            dst: php_ir::RegId::new(0),
            op: UnaryOp::Not,
            src: Operand::Constant(php_ir::ConstId::new(0)),
        });
        unit.constants = vec![IrConstant::Bool(false)];

        let report = ConstantFoldingPass
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("bool not should fold");
        assert_eq!(report.stats["bool_not_folded"], 1);
        assert_eq!(constant(&unit, 1), &IrConstant::Bool(true));

        let mut unit = folding_unit(InstructionKind::Binary {
            dst: php_ir::RegId::new(0),
            op: BinaryOp::Concat,
            lhs: Operand::Constant(php_ir::ConstId::new(0)),
            rhs: Operand::Constant(php_ir::ConstId::new(1)),
        });
        unit.constants = vec![
            IrConstant::String("php".to_string()),
            IrConstant::String("-vm".to_string()),
        ];

        let report = ConstantFoldingPass
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("string concat should fold");
        assert_eq!(report.stats["string_concat_folded"], 1);
        assert_eq!(
            constant(&unit, 2),
            &IrConstant::String("php-vm".to_string())
        );
    }

    #[test]
    fn refuses_unsafe_or_observable_folds() {
        for (op, lhs, rhs) in [
            (BinaryOp::Add, IrConstant::Int(i64::MAX), IrConstant::Int(1)),
            (BinaryOp::Div, IrConstant::Int(6), IrConstant::Int(3)),
            (BinaryOp::Mod, IrConstant::Int(6), IrConstant::Int(3)),
            (
                BinaryOp::Add,
                IrConstant::String("1".to_string()),
                IrConstant::Int(2),
            ),
        ] {
            let mut unit = folding_unit(InstructionKind::Binary {
                dst: php_ir::RegId::new(0),
                op,
                lhs: Operand::Constant(php_ir::ConstId::new(0)),
                rhs: Operand::Constant(php_ir::ConstId::new(1)),
            });
            unit.constants = vec![lhs, rhs];
            let before = unit.clone();

            let report = ConstantFoldingPass
                .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
                .expect("unsafe fold should be skipped");

            assert_eq!(unit, before);
            assert!(!report.changed);
            assert_eq!(report.stats["total_folded"], 0);
            assert_eq!(report.stats["skipped_unsafe"], 1);
        }
    }

    #[test]
    fn preserves_source_maps_and_does_not_fold_non_bool_not() {
        let mut unit = folding_unit(InstructionKind::Unary {
            dst: php_ir::RegId::new(0),
            op: UnaryOp::Not,
            src: Operand::Constant(php_ir::ConstId::new(0)),
        });
        unit.constants = vec![IrConstant::Int(0)];
        let before_files = unit.files.clone();
        let before_source_map = unit.source_map.clone();

        let report = ConstantFoldingPass
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("non-bool not should be skipped");

        assert!(!report.changed);
        assert!(report.source_spans_preserved);
        assert_eq!(unit.files, before_files);
        assert_eq!(unit.source_map, before_source_map);
        assert_eq!(report.stats["skipped_unsafe"], 1);
    }

    #[test]
    fn peephole_removes_nop_and_self_move_with_snapshot() {
        let mut builder = IrBuilder::new(UnitId::new(2));
        let file = builder.add_file("optimizer/peephole.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let block = builder.append_block(function);
        let constant = builder.add_constant(IrConstant::Int(1));
        let register = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::Nop,
            IrSpan::new(file, 6, 7),
        );
        builder.emit_load_const(function, block, register, constant, IrSpan::new(file, 8, 9));
        builder.emit(
            function,
            block,
            InstructionKind::Move {
                dst: register,
                src: Operand::Register(register),
            },
            IrSpan::new(file, 10, 11),
        );
        builder.terminate_return(
            function,
            block,
            Some(Operand::Register(register)),
            IrSpan::new(file, 12, 13),
        );
        builder.set_entry(function);
        let mut unit = builder.finish();
        let before = format!("{unit}");

        let report = PeepholeSimplify
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("peephole pass should verify after each transform");
        let after = format!("{unit}");

        assert!(before.contains("nop"));
        assert!(before.contains("move"));
        assert!(!after.contains("nop"));
        assert!(!after.contains("move"));
        assert_eq!(report.stats["noops_removed"], 1);
        assert_eq!(report.stats["self_moves_removed"], 1);
        assert_eq!(report.stats["total_transformations"], 2);
        assert!(report.source_spans_preserved);
    }

    #[test]
    fn peephole_keeps_effectful_and_register_defining_moves() {
        let mut builder = IrBuilder::new(UnitId::new(3));
        let file = builder.add_file("optimizer/no-peephole.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let block = builder.append_block(function);
        let constant = builder.add_constant(IrConstant::Int(1));
        let source = builder.alloc_register(function);
        let target = builder.alloc_register(function);
        builder.emit_load_const(function, block, source, constant, IrSpan::new(file, 6, 7));
        builder.emit(
            function,
            block,
            InstructionKind::Move {
                dst: target,
                src: Operand::Register(source),
            },
            IrSpan::new(file, 8, 9),
        );
        builder.emit(
            function,
            block,
            InstructionKind::CallFunction {
                dst: source,
                name: "side_effect".to_string(),
                args: Vec::new(),
            },
            IrSpan::new(file, 10, 11),
        );
        builder.terminate_return(
            function,
            block,
            Some(Operand::Register(target)),
            IrSpan::new(file, 12, 13),
        );
        builder.set_entry(function);
        let mut unit = builder.finish();
        let before = unit.clone();

        let report = PeepholeSimplify
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("negative peepholes should pass");

        assert_eq!(unit, before);
        assert!(!report.changed);
        assert_eq!(report.stats["total_transformations"], 0);
    }

    #[test]
    fn branch_simplify_rewrites_constant_jump_if_snapshot() {
        let mut builder = IrBuilder::new(UnitId::new(4));
        let file = builder.add_file("optimizer/branch.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let entry = builder.append_block(function);
        let true_block = builder.append_block(function);
        let false_block = builder.append_block(function);
        let condition = builder.add_constant(IrConstant::Bool(true));
        builder.terminate_jump_if(
            function,
            entry,
            Operand::Constant(condition),
            true_block,
            false_block,
            IrSpan::new(file, 6, 10),
        );
        builder.terminate_return(function, true_block, None, IrSpan::new(file, 11, 12));
        builder.terminate_return(function, false_block, None, IrSpan::new(file, 13, 14));
        builder.set_entry(function);
        let mut unit = builder.finish();
        let before = format!("{unit}");

        let report = super::BranchSimplify
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("constant branch should simplify");
        let after = format!("{unit}");

        assert!(before.contains("jump_if"));
        assert!(after.contains("jump block:1"));
        assert_eq!(report.stats["constant_branches"], 1);
        assert_eq!(report.stats["unreachable_empty_tail_blocks_removed"], 1);
        assert_eq!(report.stats["total_transformations"], 2);
        assert!(report.source_spans_preserved);
    }

    #[test]
    fn branch_simplify_uses_cfg_fallthrough_for_loaded_bool_conditions() {
        let mut builder = IrBuilder::new(UnitId::new(5));
        let file = builder.add_file("optimizer/fallthrough.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let entry = builder.append_block(function);
        let fallthrough = builder.append_block(function);
        let false_target = builder.append_block(function);
        let condition = builder.add_constant(IrConstant::Bool(true));
        let register = builder.alloc_register(function);
        builder.emit_load_const(
            function,
            entry,
            register,
            condition,
            IrSpan::new(file, 6, 7),
        );
        builder.terminate_jump_if_false(
            function,
            entry,
            Operand::Register(register),
            false_target,
            IrSpan::new(file, 8, 9),
        );
        builder.terminate_return(function, fallthrough, None, IrSpan::new(file, 10, 11));
        builder.terminate_return(function, false_target, None, IrSpan::new(file, 12, 13));
        builder.set_entry(function);
        let mut unit = builder.finish();

        let report = super::BranchSimplify
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("loaded bool branch should simplify to fallthrough jump");

        assert!(matches!(
            unit.functions[0].blocks[0].terminator.as_ref().unwrap().kind,
            TerminatorKind::Jump { target } if target == fallthrough
        ));
        assert_eq!(report.stats["constant_branches"], 1);
    }

    #[test]
    fn branch_simplify_forwards_empty_blocks_and_truncates_empty_unreachable_tail() {
        let mut builder = IrBuilder::new(UnitId::new(6));
        let file = builder.add_file("optimizer/empty-block.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let entry = builder.append_block(function);
        let forwarding = builder.append_block(function);
        let target = builder.append_block(function);
        let tail = builder.append_block(function);
        builder.terminate_jump(function, entry, forwarding, IrSpan::new(file, 6, 7));
        builder.terminate_jump(function, forwarding, target, IrSpan::new(file, 8, 9));
        builder.terminate_return(function, target, None, IrSpan::new(file, 10, 11));
        builder.terminate_return(function, tail, None, IrSpan::new(file, 12, 13));
        builder.set_entry(function);
        let mut unit = builder.finish();

        let report = super::BranchSimplify
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("empty block CFG simplifications should verify");

        assert!(matches!(
            unit.functions[0].blocks[0].terminator.as_ref().unwrap().kind,
            TerminatorKind::Jump { target: rewritten } if rewritten == target
        ));
        assert_eq!(unit.functions[0].blocks.len(), 3);
        assert_eq!(report.stats["empty_block_forwards"], 1);
        assert_eq!(report.stats["unreachable_empty_tail_blocks_removed"], 1);
    }

    #[test]
    fn branch_simplify_keeps_non_bool_and_exception_boundary_blocks() {
        let mut builder = IrBuilder::new(UnitId::new(7));
        let file = builder.add_file("optimizer/no-branch.php");
        let function = builder.start_function(
            "main",
            FunctionFlags {
                is_top_level: true,
                ..FunctionFlags::default()
            },
            IrSpan::new(file, 0, 5),
        );
        let entry = builder.append_block(function);
        let target = builder.append_block(function);
        let fallback = builder.append_block(function);
        let after = builder.append_block(function);
        let condition = builder.add_constant(IrConstant::Int(1));
        let register = builder.alloc_register(function);
        builder.emit(
            function,
            entry,
            InstructionKind::EnterTry {
                catch: None,
                catch_types: Vec::new(),
                finally: None,
                after,
                exception_local: None,
            },
            IrSpan::new(file, 6, 7),
        );
        builder.emit_load_const(
            function,
            entry,
            register,
            condition,
            IrSpan::new(file, 8, 9),
        );
        builder.terminate_jump_if(
            function,
            entry,
            Operand::Register(register),
            target,
            fallback,
            IrSpan::new(file, 10, 11),
        );
        builder.terminate_return(function, target, None, IrSpan::new(file, 12, 13));
        builder.terminate_return(function, fallback, None, IrSpan::new(file, 14, 15));
        builder.terminate_return(function, after, None, IrSpan::new(file, 16, 17));
        builder.set_entry(function);
        let mut unit = builder.finish();
        let before = unit.clone();

        let report = super::BranchSimplify
            .run(&mut unit, &PassContext::new(OptimizationLevel::O1))
            .expect("unsafe branch simplifications should be skipped");

        assert_eq!(unit, before);
        assert!(!report.changed);
        assert_eq!(report.stats["total_transformations"], 0);
    }
}
