use crate::ids::BlockId;
use crate::source_map::IrSpan;

use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct LoopTargets {
    pub(super) break_block: BlockId,
    pub(super) continue_block: BlockId,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ConditionTargets {
    pub(super) true_target: BlockId,
    pub(super) false_target: BlockId,
    pub(super) span: IrSpan,
}

impl LoweringContext<'_> {
    pub(super) fn lower_break_or_continue(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        expr: Option<ExprId>,
        is_break: bool,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let level = self.loop_control_level(expr).unwrap_or(1);
        if level == 0 || level > self.loop_stack.len() {
            self.unsupported(
                UnsupportedFeature::DynamicLoopControlLevel,
                self.span_for(SourceMappedId::from(stmt_id)),
                "break/continue level is outside the active loop stack",
            );
            return block;
        }
        let targets = self.loop_stack[self.loop_stack.len() - level];
        let target = if is_break {
            targets.break_block
        } else {
            targets.continue_block
        };
        self.jump_if_open(builder, function, block, target, span);
        block
    }

    pub(super) fn ensure_label_blocks(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        labels: Vec<(String, StmtId)>,
    ) {
        for (name, stmt_id) in labels {
            if self
                .label_blocks
                .get(&function)
                .and_then(|labels| labels.get(&name))
                .is_some()
            {
                continue;
            }
            let block = builder.append_block(function);
            let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
            builder.add_source_map(
                IrSourceMapTarget::Block { function, block },
                format!("hir:stmt:{}:label:{name}", stmt_id.raw()),
                span,
            );
            self.label_blocks
                .entry(function)
                .or_default()
                .insert(name, block);
        }
    }

    pub(super) fn collect_label_statements(&self, statements: &[StmtId]) -> Vec<(String, StmtId)> {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return Vec::new();
        };
        let mut labels = Vec::new();
        self.collect_label_statements_into(module, statements, &mut labels);
        labels
    }

    pub(super) fn collect_label_statements_into(
        &self,
        module: &HirModule,
        statements: &[StmtId],
        labels: &mut Vec<(String, StmtId)>,
    ) {
        for stmt in statements {
            let Some(statement) = module.statements().get(*stmt) else {
                continue;
            };
            match statement.kind() {
                HirStmtKind::Label { name: Some(name) } => {
                    labels.push((name.clone(), *stmt));
                }
                HirStmtKind::Label { name: None } => {}
                HirStmtKind::Block { statements }
                | HirStmtKind::While {
                    body: statements, ..
                }
                | HirStmtKind::DoWhile {
                    body: statements, ..
                }
                | HirStmtKind::Declare {
                    body: statements, ..
                } => self.collect_label_statements_into(module, statements, labels),
                HirStmtKind::If {
                    body,
                    elseifs,
                    else_body,
                    ..
                } => {
                    self.collect_label_statements_into(module, body, labels);
                    for branch in elseifs {
                        self.collect_label_statements_into(module, &branch.body, labels);
                    }
                    self.collect_label_statements_into(module, else_body, labels);
                }
                HirStmtKind::For { body, .. } | HirStmtKind::Foreach { body, .. } => {
                    self.collect_label_statements_into(module, body, labels);
                }
                HirStmtKind::Switch { cases, .. } => {
                    for case in cases {
                        self.collect_label_statements_into(module, &case.body, labels);
                    }
                }
                HirStmtKind::Try {
                    body,
                    catches,
                    finally_body,
                } => {
                    self.collect_label_statements_into(module, body, labels);
                    for catch in catches {
                        self.collect_label_statements_into(module, &catch.body, labels);
                    }
                    self.collect_label_statements_into(module, finally_body, labels);
                }
                _ => {}
            }
        }
    }

    pub(super) fn is_label_stmt(&self, stmt: StmtId) -> bool {
        self.frontend
            .database()
            .module(self.frontend.module().module_id())
            .and_then(|module| module.statements().get(stmt))
            .is_some_and(|statement| matches!(statement.kind(), HirStmtKind::Label { .. }))
    }

    pub(super) fn lower_label_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        name: Option<String>,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let Some(name) = name else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(stmt_id)),
                "label statement is missing its label name",
            );
            return block;
        };
        let Some(target) = self
            .label_blocks
            .get(&function)
            .and_then(|labels| labels.get(&name))
            .copied()
        else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(stmt_id)),
                format!("label `{name}` has no lowered target block"),
            );
            return block;
        };
        if block != target && !builder.is_terminated(function, block) {
            builder.terminate_jump(function, block, target, span);
        }
        builder.add_source_map(
            IrSourceMapTarget::Block {
                function,
                block: target,
            },
            format!("hir:label:{name}"),
            span,
        );
        target
    }

    pub(super) fn lower_goto_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        label: Option<String>,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let Some(label) = label else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(stmt_id)),
                "goto statement is missing its target label",
            );
            return block;
        };
        let Some(target) = self
            .label_blocks
            .get(&function)
            .and_then(|labels| labels.get(&label))
            .copied()
        else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(stmt_id)),
                format!("goto target label `{label}` was not found in this function"),
            );
            return block;
        };
        if !builder.is_terminated(function, block) {
            builder.terminate_jump(function, block, target, span);
            builder.add_source_map(
                IrSourceMapTarget::Terminator { function, block },
                format!("hir:goto:{label}"),
                span,
            );
        }
        block
    }

    pub(super) fn terminate_condition_targets(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        condition: Option<ExprId>,
        targets: ConditionTargets,
    ) {
        let Some(condition) = condition else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                TextRange::new(targets.span.start as usize, targets.span.end as usize),
                "control-flow condition is missing",
            );
            return;
        };
        if let Some(value) = self.lower_expr_to_register(builder, function, block, condition) {
            builder.terminate_jump_if(
                function,
                value.block,
                Operand::Register(value.register),
                targets.true_target,
                targets.false_target,
                targets.span,
            );
        }
    }

    pub(super) fn jump_if_open(
        &self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        target: BlockId,
        span: IrSpan,
    ) {
        if !builder.is_terminated(function, block) {
            builder.terminate_jump(function, block, target, span);
        }
    }

    pub(super) fn loop_control_level(&mut self, expr: Option<ExprId>) -> Option<usize> {
        let Some(expr) = expr else {
            return Some(1);
        };
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Literal { text } => text.trim().parse::<usize>().ok(),
            _ => {
                self.unsupported(
                    UnsupportedFeature::DynamicLoopControlLevel,
                    self.span_for(SourceMappedId::from(expr)),
                    "dynamic break/continue levels are not lowered in the control-flow MVP",
                );
                None
            }
        }
    }

    pub(super) fn add_expr_source_map(
        &self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        instruction: crate::ids::InstrId,
        expr: ExprId,
        span: IrSpan,
    ) {
        builder.add_source_map(
            IrSourceMapTarget::Instruction {
                function,
                block,
                instruction,
            },
            format!("hir:expr:{}", expr.raw()),
            span,
        );
    }

    pub(super) fn unsupported(
        &mut self,
        feature: UnsupportedFeature,
        range: TextRange,
        message: impl Into<String>,
    ) {
        let span = span_from_range(self.file, range);
        self.diagnostics.push(LoweringDiagnostic {
            id: feature.diagnostic_id().to_string(),
            feature,
            span,
            message: message.into(),
        });
    }

    pub(super) fn span_for(&self, id: SourceMappedId) -> TextRange {
        self.frontend
            .database()
            .source_map()
            .span(id)
            .unwrap_or_else(|| TextRange::new(0, self.frontend.module().source_bytes()))
    }
}
