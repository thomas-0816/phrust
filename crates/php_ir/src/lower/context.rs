use std::cell::OnceCell;
use std::collections::HashMap;

use crate::builder::IrBuilder;
use crate::constants::IrConstant;
use crate::ids::{BlockId, FileId, FunctionId, UnitId};
use crate::instruction::{InstructionKind, IrDiagnosticSeverity};
use crate::module::IrUnit;
use crate::source_map::{IrSourceMapTarget, IrSpan};
use crate::verify::VerificationError;
use php_semantics::hir::{ExprId, HirExprKind};
use php_semantics::{FrontendResult, SourceMappedId};
use php_source::{SourceText, TextRange};

use super::control_flow::LoopTargets;
use super::diagnostics::{EarlyDiagnostic, LoweringDiagnostic};

/// Options for the skeleton lowering entrypoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoweringOptions {
    /// IR unit ID to assign.
    pub unit_id: UnitId,
    /// Source path used in the IR file table.
    pub source_path: String,
    /// Source text used for line-sensitive magic constants.
    pub source_text: Option<String>,
}

impl Default for LoweringOptions {
    fn default() -> Self {
        Self {
            unit_id: UnitId::new(0),
            source_path: "<memory>".to_string(),
            source_text: None,
        }
    }
}

/// Lowering context for one frontend result.
#[derive(Debug)]
pub struct LoweringContext<'a> {
    pub(super) frontend: &'a FrontendResult,
    pub(super) options: LoweringOptions,
    pub(super) file: FileId,
    pub(super) diagnostics: Vec<LoweringDiagnostic>,
    pub(super) loop_stack: Vec<LoopTargets>,
    pub(super) label_blocks: HashMap<FunctionId, HashMap<String, BlockId>>,
    pub(super) closure_functions: HashMap<ExprId, FunctionId>,
    pub(super) function_names: HashMap<FunctionId, String>,
    pub(super) namespace_names: HashMap<FunctionId, String>,
    pub(super) conditional_function_declarations: Vec<(TextRange, String, FunctionId)>,
    pub(super) class_names: HashMap<FunctionId, String>,
    pub(super) method_names: HashMap<FunctionId, String>,
    pub(super) variable_spans: HashMap<String, Vec<TextRange>>,
    pub(super) global_constant_initializers: OnceCell<HashMap<String, IrConstant>>,
    pub(super) source_text: SourceText,
    pub(super) early_diagnostics: HashMap<FunctionId, Vec<EarlyDiagnostic>>,
}

impl<'a> LoweringContext<'a> {
    /// Creates a lowering context.
    #[must_use]
    pub fn new(frontend: &'a FrontendResult, options: LoweringOptions, file: FileId) -> Self {
        let source_text = SourceText::new(options.source_text.clone().unwrap_or_default());
        let variable_spans = collect_variable_spans(frontend);
        Self {
            frontend,
            options,
            file,
            diagnostics: Vec::new(),
            loop_stack: Vec::new(),
            label_blocks: HashMap::new(),
            closure_functions: HashMap::new(),
            function_names: HashMap::new(),
            namespace_names: HashMap::new(),
            conditional_function_declarations: Vec::new(),
            class_names: HashMap::new(),
            method_names: HashMap::new(),
            variable_spans,
            global_constant_initializers: OnceCell::new(),
            source_text,
            early_diagnostics: HashMap::new(),
        }
    }

    /// Returns collected diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[LoweringDiagnostic] {
        &self.diagnostics
    }

    pub(super) fn record_early_diagnostic(
        &mut self,
        function: FunctionId,
        expr: ExprId,
        span: IrSpan,
        severity: IrDiagnosticSeverity,
        diagnostic_id: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.early_diagnostics
            .entry(function)
            .or_default()
            .push(EarlyDiagnostic {
                origin: format!("hir:expr:{}", expr.raw()),
                span,
                severity,
                diagnostic_id: diagnostic_id.into(),
                message: message.into(),
            });
    }

    pub(super) fn record_early_diagnostic_origin(
        &mut self,
        function: FunctionId,
        origin: impl Into<String>,
        span: IrSpan,
        severity: IrDiagnosticSeverity,
        diagnostic_id: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.early_diagnostics
            .entry(function)
            .or_default()
            .push(EarlyDiagnostic {
                origin: origin.into(),
                span,
                severity,
                diagnostic_id: diagnostic_id.into(),
                message: message.into(),
            });
    }

    pub(super) fn doc_comment_before(&self, range: TextRange) -> Option<String> {
        let text = self.source_text.as_str();
        let declaration_start = range.start().to_usize().min(text.len());
        let before = &text[..declaration_start];
        let trimmed = before.trim_end_matches(|ch: char| ch.is_whitespace());
        let comment_end = trimmed.len();
        let comment_start = trimmed.rfind("/**")?;
        let comment = &trimmed[comment_start..comment_end];
        if !comment.ends_with("*/") {
            return None;
        }
        Some(comment.to_owned())
    }

    pub(super) fn emit_early_diagnostics(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
    ) {
        let Some(diagnostics) = self.early_diagnostics.remove(&function) else {
            return;
        };
        for (index, diagnostic) in diagnostics.into_iter().enumerate() {
            let instruction = builder.emit(
                function,
                block,
                InstructionKind::EmitDiagnostic {
                    severity: diagnostic.severity,
                    diagnostic_id: diagnostic.diagnostic_id,
                    message: diagnostic.message,
                    leading_newline: index > 0,
                },
                diagnostic.span,
            );
            builder.add_source_map(
                IrSourceMapTarget::Instruction {
                    function,
                    block,
                    instruction,
                },
                diagnostic.origin,
                diagnostic.span,
            );
        }
    }
}

fn collect_variable_spans(frontend: &FrontendResult) -> HashMap<String, Vec<TextRange>> {
    let mut variable_spans: HashMap<String, Vec<TextRange>> = HashMap::new();
    let Some(module) = frontend.database().module(frontend.module().module_id()) else {
        return variable_spans;
    };
    let source_map = frontend.database().source_map();
    for (expr_id, expr) in module.expressions().iter() {
        let HirExprKind::Variable { name, .. } = expr.kind() else {
            continue;
        };
        let Some(span) = source_map.span(SourceMappedId::from(expr_id)) else {
            continue;
        };
        variable_spans.entry(name.clone()).or_default().push(span);
    }
    for spans in variable_spans.values_mut() {
        spans.sort_by_key(|span| (span.start().to_usize(), span.end().to_usize()));
    }
    variable_spans
}

/// Result of lowering one frontend file.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweringResult {
    /// Lowered IR unit.
    pub unit: IrUnit,
    /// Lowering diagnostics.
    pub diagnostics: Vec<LoweringDiagnostic>,
    /// Verifier result for the produced unit.
    pub verification: Result<(), Vec<VerificationError>>,
}
