use super::*;

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
    fn run(
        &self,
        transaction: &mut PassTransaction<'_>,
        context: &PassContext,
    ) -> Result<PassReport, PassError>;
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
    /// Whether the pass result failed verification and was rolled back.
    pub rolled_back: bool,
    /// Exact mutation scope observed by the pipeline transaction.
    pub scope: PassScopeReport,
    /// Deterministic pass statistics.
    pub stats: BTreeMap<&'static str, u64>,
}

/// Deterministic mutation footprint for one optimizer pass.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PassScopeReport {
    /// Function indexes written by the pass.
    pub functions: Vec<usize>,
    /// Function/block index pairs written by the pass.
    pub blocks: Vec<(usize, usize)>,
    /// Whether the constant pool was written.
    pub constants: bool,
    /// Unit metadata tables written by the pass.
    pub metadata: Vec<&'static str>,
    /// Whether the pass API allowed source mappings to change.
    pub source_mappings_may_change: bool,
}

impl PassReport {
    pub(crate) fn skipped(name: &'static str, phase: PassPhase) -> Self {
        Self {
            name,
            phase,
            enabled: false,
            changed: false,
            source_spans_preserved: true,
            rolled_back: false,
            scope: PassScopeReport::default(),
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

impl PassError {
    /// Converts an optimizer failure to shared diagnostic envelope(s).
    #[must_use]
    pub fn to_diagnostic_envelopes(
        &self,
        optimization_level: OptimizationLevel,
        unit_name: Option<&str>,
        function_name: Option<&str>,
    ) -> Vec<DiagnosticEnvelope> {
        match self {
            Self::PassFailed { pass, message } => {
                let mut context = optimizer_context(optimization_level, unit_name, function_name);
                context.insert("pass".to_string(), (*pass).to_string());
                vec![
                    DiagnosticEnvelope::new(
                        "E_PHP_OPTIMIZER_PASS_FAILED",
                        DiagnosticLayer::optimizer(),
                        DiagnosticPhase::new("pass"),
                        DiagnosticSeverity::Error,
                        message.clone(),
                    )
                    .with_context(context),
                ]
            }
            Self::Verification { phase, errors } => errors
                .iter()
                .map(|error| {
                    let mut envelope =
                        error.to_diagnostic_envelope(&VerificationDiagnosticContext::default());
                    envelope.layer = DiagnosticLayer::optimizer();
                    envelope.phase = DiagnosticPhase::new(format!("verify_{}", phase.as_str()));
                    envelope.context.extend(optimizer_context(
                        optimization_level,
                        unit_name,
                        function_name,
                    ));
                    envelope
                        .context
                        .insert("optimizer_phase".to_string(), phase.as_str().to_string());
                    envelope
                })
                .collect(),
        }
    }
}

fn optimizer_context(
    optimization_level: OptimizationLevel,
    unit_name: Option<&str>,
    function_name: Option<&str>,
) -> BTreeMap<String, String> {
    let mut context = BTreeMap::new();
    context.insert(
        "optimization_level".to_string(),
        optimization_level.as_str().to_string(),
    );
    if let Some(unit_name) = unit_name {
        context.insert("unit".to_string(), unit_name.to_string());
    }
    if let Some(function_name) = function_name {
        context.insert("function".to_string(), function_name.to_string());
    }
    context
}
