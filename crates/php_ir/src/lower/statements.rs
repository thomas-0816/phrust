use php_semantics::hir::{ExprId, HirCatchClause, HirIfBranch, StmtId};

use super::control_flow::*;
use super::expressions::*;
use super::*;

pub(super) struct HirTryParts {
    pub(super) body: Vec<StmtId>,
    pub(super) catches: Vec<HirCatchClause>,
    pub(super) finally_body: Vec<StmtId>,
}

#[derive(Clone, Debug)]
pub(super) struct IfParts {
    pub(super) condition: Option<ExprId>,
    pub(super) body: Vec<StmtId>,
    pub(super) elseifs: Vec<HirIfBranch>,
    pub(super) else_body: Vec<StmtId>,
}

#[derive(Clone, Debug)]
pub(super) struct ForParts {
    pub(super) init: Vec<ExprId>,
    pub(super) condition: Vec<ExprId>,
    pub(super) update: Vec<ExprId>,
    pub(super) body: Vec<StmtId>,
}

impl LoweringContext<'_> {
    pub(super) fn lower_auto_global_bindings(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        use_span: TextRange,
        span: IrSpan,
    ) -> BlockId {
        for name in AUTO_GLOBAL_NAMES {
            let variable_name = format!("${name}");
            if !self.function_like_uses_variable(use_span, &variable_name) {
                continue;
            }
            let local = builder.intern_local(function, *name);
            builder.emit(
                function,
                block,
                InstructionKind::BindGlobal {
                    local,
                    name: (*name).to_owned(),
                },
                span,
            );
        }
        block
    }

    pub(super) fn lower_top_level(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
    ) -> BlockId {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return block;
        };

        let previous_namespace = self.namespace_names.get(&function).cloned();
        let mut current = block;
        for namespace in module.namespaces().values() {
            self.namespace_names.insert(
                function,
                namespace
                    .name()
                    .map_or_else(String::new, |name| name.text().to_owned()),
            );
            let mut statements = Vec::new();
            for item in namespace.items() {
                if item.kind() != TopLevelItemKind::Statement
                    && item.kind() != TopLevelItemKind::InlineHtml
                {
                    continue;
                }
                if let Some(stmt_id) = self.statement_id_for_span(item.span()) {
                    statements.push(stmt_id);
                }
            }
            current = self.lower_stmt_list(builder, function, current, statements);
        }

        match previous_namespace {
            Some(name) => {
                self.namespace_names.insert(function, name);
            }
            None => {
                self.namespace_names.remove(&function);
            }
        }

        current
    }

    pub(super) fn lower_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
    ) -> BlockId {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return block;
        };
        let Some(statement) = module.statements().get(stmt_id) else {
            return block;
        };
        let kind = statement.kind().clone();
        match kind {
            HirStmtKind::Missing => block,
            HirStmtKind::InlineHtml { text } => {
                self.lower_inline_html_stmt(builder, function, block, stmt_id, text)
            }
            HirStmtKind::Block { statements } => {
                self.lower_stmt_list(builder, function, block, statements)
            }
            HirStmtKind::Expr { expr } => {
                if let Some(expr) = expr {
                    if expr_stmt_is_side_effect_free_bare_variable(module, expr) {
                        return block;
                    }
                    if self.lower_exit_stmt(builder, function, block, expr, module) {
                        return block;
                    }
                    if let Some(next_block) =
                        self.lower_short_circuit_exit_stmt(builder, function, block, expr, module)
                    {
                        return next_block;
                    }
                    if let Some(value) = self.lower_expr_to_register(builder, function, block, expr)
                    {
                        let span =
                            span_from_range(self.file, self.span_for(SourceMappedId::from(expr)));
                        let discard = builder.emit(
                            function,
                            value.block,
                            InstructionKind::Discard {
                                src: Operand::Register(value.register),
                            },
                            span,
                        );
                        self.add_expr_source_map(
                            builder,
                            function,
                            value.block,
                            discard,
                            expr,
                            span,
                        );
                        return value.block;
                    }
                }
                block
            }
            HirStmtKind::Echo { expressions } => {
                let mut current = block;
                for expr in expressions {
                    current = self.lower_echo_expr(builder, function, current, expr);
                }
                current
            }
            HirStmtKind::If {
                condition,
                body,
                elseifs,
                else_body,
            } => self.lower_if_stmt(
                builder,
                function,
                block,
                stmt_id,
                IfParts {
                    condition,
                    body,
                    elseifs,
                    else_body,
                },
            ),
            HirStmtKind::While { condition, body } => {
                self.lower_while_stmt(builder, function, block, stmt_id, condition, body)
            }
            HirStmtKind::DoWhile { condition, body } => {
                self.lower_do_while_stmt(builder, function, block, stmt_id, condition, body)
            }
            HirStmtKind::For {
                init,
                condition,
                update,
                body,
            } => self.lower_for_stmt(
                builder,
                function,
                block,
                stmt_id,
                ForParts {
                    init,
                    condition,
                    update,
                    body,
                },
            ),
            HirStmtKind::Foreach {
                source,
                key_target,
                value_target,
                by_ref,
                body,
            } => self.lower_foreach_stmt(
                builder,
                function,
                block,
                stmt_id,
                source,
                key_target,
                value_target,
                by_ref,
                body,
            ),
            HirStmtKind::Break { expr } => {
                self.lower_break_or_continue(builder, function, block, stmt_id, expr, true)
            }
            HirStmtKind::Continue { expr } => {
                self.lower_break_or_continue(builder, function, block, stmt_id, expr, false)
            }
            HirStmtKind::Switch {
                condition,
                body: _,
                cases,
            } => self.lower_switch_stmt(builder, function, block, stmt_id, condition, cases),
            HirStmtKind::Try {
                body,
                catches,
                finally_body,
            } => self.lower_try_stmt(
                builder,
                function,
                block,
                stmt_id,
                HirTryParts {
                    body,
                    catches,
                    finally_body,
                },
            ),
            HirStmtKind::Return { expr } => {
                self.lower_return_stmt(builder, function, block, stmt_id, expr)
            }
            HirStmtKind::Throw { expr } => {
                self.lower_throw_stmt(builder, function, block, stmt_id, expr)
            }
            HirStmtKind::Unset { expressions } => {
                self.lower_unset_stmt(builder, function, block, stmt_id, expressions)
            }
            HirStmtKind::Static { variables } => {
                self.lower_static_stmt(builder, function, block, stmt_id, variables)
            }
            HirStmtKind::Global { variables } => {
                self.lower_global_stmt(builder, function, block, stmt_id, variables)
            }
            HirStmtKind::Label { name } => {
                self.lower_label_stmt(builder, function, block, stmt_id, name)
            }
            HirStmtKind::Goto { label } => {
                self.lower_goto_stmt(builder, function, block, stmt_id, label)
            }
            kind => {
                let span = self.span_for(SourceMappedId::from(stmt_id));
                if let Some((name, declared_function)) =
                    self.conditional_function_declaration_for_span(span)
                {
                    builder.emit(
                        function,
                        block,
                        InstructionKind::DeclareFunction {
                            name,
                            function: declared_function,
                        },
                        span_from_range(self.file, span),
                    );
                    return block;
                }
                if let Some(name) = conditional_class_declaration_name_for_span(module, span) {
                    builder.emit(
                        function,
                        block,
                        InstructionKind::DeclareClass { name },
                        span_from_range(self.file, span),
                    );
                    return block;
                }
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    span,
                    format!("HIR statement `{}` is not lowered to IR yet", kind.as_str()),
                );
                block
            }
        }
    }

    pub(super) fn lower_global_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        variables: Vec<ExprId>,
    ) -> BlockId {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return block;
        };
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let names = if variables.is_empty() {
            self.global_names_from_stmt_source(stmt_id)
        } else {
            variables
                .into_iter()
                .filter_map(|variable| {
                    let expression = module.expressions().get(variable)?;
                    let HirExprKind::Variable { name } = expression.kind() else {
                        self.unsupported(
                            UnsupportedFeature::HirStatement,
                            self.span_for(SourceMappedId::from(variable)),
                            "dynamic global variables are not lowered to IR in runtime-semantics",
                        );
                        return None;
                    };
                    Some(local_name(name).to_owned())
                })
                .collect()
        };
        for name in names {
            let local = builder.intern_local(function, &name);
            builder.emit(
                function,
                block,
                InstructionKind::BindGlobal { local, name },
                span,
            );
        }
        block
    }

    pub(super) fn conditional_function_declaration_for_span(
        &self,
        span: TextRange,
    ) -> Option<(String, FunctionId)> {
        self.conditional_function_declarations
            .iter()
            .find(|(declaration_span, _, _)| {
                range_contains(span, *declaration_span)
                    || range_contains(*declaration_span, span)
                    || ranges_overlap(span, *declaration_span)
            })
            .map(|(_, name, function)| (name.clone(), *function))
    }

    pub(super) fn global_names_from_stmt_source(&mut self, stmt_id: StmtId) -> Vec<String> {
        let range = self.span_for(SourceMappedId::from(stmt_id));
        let Some(source) = self.source_text.slice(range) else {
            return Vec::new();
        };
        let source = source.to_owned();
        let Some(rest) = source.trim().strip_prefix("global") else {
            return Vec::new();
        };
        rest.trim_end_matches(';')
            .split(',')
            .filter_map(|item| {
                let name = item.trim();
                let name = name.strip_prefix('$')?;
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
                {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        range,
                        "dynamic global variables are not lowered to IR in runtime-semantics",
                    );
                    return None;
                }
                Some(name.to_owned())
            })
            .collect()
    }

    pub(super) fn lower_static_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        variables: Vec<ExprId>,
    ) -> BlockId {
        let specs = self.static_local_specs(stmt_id, &variables);
        let mut current = block;
        for spec in specs {
            let local = builder.intern_local(function, &spec.name);
            let (default, next_block) = if let Some(initializer) = spec.initializer {
                if let Some(value) =
                    self.lower_expr_to_register(builder, function, current, initializer)
                {
                    (Operand::Register(value.register), value.block)
                } else {
                    (
                        Operand::Constant(builder.intern_constant(IrConstant::Null)),
                        current,
                    )
                }
            } else {
                (
                    Operand::Constant(builder.intern_constant(IrConstant::Null)),
                    current,
                )
            };
            current = next_block;
            let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
            builder.emit(
                function,
                current,
                InstructionKind::InitStaticLocal {
                    local,
                    name: spec.name,
                    default,
                },
                span,
            );
        }
        current
    }

    pub(super) fn lower_echo_expr(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        expr: ExprId,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(expr)));
        let Some(value) = self.lower_expr_to_register(builder, function, block, expr) else {
            return block;
        };
        let echo = builder.emit(
            function,
            value.block,
            InstructionKind::Echo {
                src: Operand::Register(value.register),
            },
            span,
        );
        self.add_expr_source_map(builder, function, value.block, echo, expr, span);
        value.block
    }

    pub(super) fn lower_inline_html_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        text: String,
    ) -> BlockId {
        if text.is_empty() {
            return block;
        }
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let constant = builder.intern_constant(ir_string_constant(text.into_bytes()));
        let instruction = builder.emit(
            function,
            block,
            InstructionKind::Echo {
                src: Operand::Constant(constant),
            },
            span,
        );
        builder.add_source_map(
            IrSourceMapTarget::Instruction {
                function,
                block,
                instruction,
            },
            format!("hir:stmt:{}", stmt_id.raw()),
            span,
        );
        block
    }

    pub(super) fn lower_if_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        parts: IfParts,
    ) -> BlockId {
        let range = self.span_for(SourceMappedId::from(stmt_id));
        let span = span_from_range(self.file, range);
        let IfParts {
            condition,
            body,
            elseifs,
            else_body,
        } = parts;
        let condition_block = builder.append_block(function);
        let elseif_condition_blocks = elseifs
            .iter()
            .map(|_| builder.append_block(function))
            .collect::<Vec<_>>();
        let else_block = if else_body.is_empty() {
            None
        } else {
            Some(builder.append_block(function))
        };
        let after_block = builder.append_block(function);
        let then_block = builder.append_block(function);
        let elseif_body_blocks = elseifs
            .iter()
            .map(|_| builder.append_block(function))
            .collect::<Vec<_>>();

        self.jump_if_open(builder, function, block, condition_block, span);
        let first_false_target = elseif_condition_blocks
            .first()
            .copied()
            .or(else_block)
            .unwrap_or(after_block);
        self.terminate_condition_targets(
            builder,
            function,
            condition_block,
            condition,
            ConditionTargets {
                true_target: then_block,
                false_target: first_false_target,
                span,
            },
        );

        let then_end = self.lower_stmt_list(builder, function, then_block, body);
        self.jump_if_open(builder, function, then_end, after_block, span);

        for (index, branch) in elseifs.into_iter().enumerate() {
            let condition_block = elseif_condition_blocks[index];
            let body_block = elseif_body_blocks[index];
            let false_target = elseif_condition_blocks
                .get(index + 1)
                .copied()
                .or(else_block)
                .unwrap_or(after_block);
            self.terminate_condition_targets(
                builder,
                function,
                condition_block,
                branch.condition,
                ConditionTargets {
                    true_target: body_block,
                    false_target,
                    span,
                },
            );
            let body_end = self.lower_stmt_list(builder, function, body_block, branch.body);
            self.jump_if_open(builder, function, body_end, after_block, span);
        }

        if let Some(else_block) = else_block {
            let else_end = self.lower_stmt_list(builder, function, else_block, else_body);
            self.jump_if_open(builder, function, else_end, after_block, span);
        }

        after_block
    }

    pub(super) fn lower_while_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        condition: Option<ExprId>,
        body: Vec<StmtId>,
    ) -> BlockId {
        let range = self.span_for(SourceMappedId::from(stmt_id));
        let span = span_from_range(self.file, range);
        let condition_block = builder.append_block(function);
        let after_block = builder.append_block(function);
        let body_block = builder.append_block(function);
        self.jump_if_open(builder, function, block, condition_block, span);
        self.terminate_condition_targets(
            builder,
            function,
            condition_block,
            condition,
            ConditionTargets {
                true_target: body_block,
                false_target: after_block,
                span,
            },
        );
        self.loop_stack.push(LoopTargets {
            break_block: after_block,
            continue_block: condition_block,
        });
        let body_end = self.lower_stmt_list(builder, function, body_block, body);
        self.loop_stack.pop();
        self.jump_if_open(builder, function, body_end, condition_block, span);
        after_block
    }

    pub(super) fn lower_do_while_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        condition: Option<ExprId>,
        body: Vec<StmtId>,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let body_block = builder.append_block(function);
        let condition_block = builder.append_block(function);
        let after_block = builder.append_block(function);
        self.jump_if_open(builder, function, block, body_block, span);
        self.loop_stack.push(LoopTargets {
            break_block: after_block,
            continue_block: condition_block,
        });
        let body_end = self.lower_stmt_list(builder, function, body_block, body);
        self.loop_stack.pop();
        self.jump_if_open(builder, function, body_end, condition_block, span);
        let Some(condition) = condition else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(stmt_id)),
                "do/while condition is missing",
            );
            self.jump_if_open(builder, function, condition_block, after_block, span);
            return after_block;
        };
        if let Some(value) =
            self.lower_expr_to_register(builder, function, condition_block, condition)
        {
            builder.terminate_jump_if(
                function,
                value.block,
                Operand::Register(value.register),
                body_block,
                after_block,
                span,
            );
        } else {
            self.jump_if_open(builder, function, condition_block, after_block, span);
        }
        after_block
    }

    pub(super) fn lower_for_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        parts: ForParts,
    ) -> BlockId {
        let ForParts {
            init,
            condition,
            update,
            body,
        } = parts;
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let mut current = block;
        for init in init {
            current = self.lower_expr_stmt(builder, function, current, init);
        }
        let condition_block = builder.append_block(function);
        let after_block = builder.append_block(function);
        let body_block = builder.append_block(function);
        let update_block = builder.append_block(function);
        self.jump_if_open(builder, function, current, condition_block, span);
        if let Some((last_condition, leading_conditions)) = condition.split_last() {
            let mut current_condition = condition_block;
            for condition in leading_conditions {
                current_condition =
                    self.lower_expr_stmt(builder, function, current_condition, *condition);
            }
            self.terminate_condition_targets(
                builder,
                function,
                current_condition,
                Some(*last_condition),
                ConditionTargets {
                    true_target: body_block,
                    false_target: after_block,
                    span,
                },
            );
        } else {
            self.jump_if_open(builder, function, condition_block, body_block, span);
        }
        self.loop_stack.push(LoopTargets {
            break_block: after_block,
            continue_block: update_block,
        });
        let body_end = self.lower_stmt_list(builder, function, body_block, body);
        self.loop_stack.pop();
        self.jump_if_open(builder, function, body_end, update_block, span);
        let mut current_update = update_block;
        for update in update {
            current_update = self.lower_expr_stmt(builder, function, current_update, update);
        }
        self.jump_if_open(builder, function, current_update, condition_block, span);
        after_block
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_foreach_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        source: Option<ExprId>,
        key_target: Option<ExprId>,
        value_target: Option<ExprId>,
        by_ref: bool,
        body: Vec<StmtId>,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let Some(source) = source else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(stmt_id)),
                "foreach source expression is missing",
            );
            return block;
        };
        let Some(value_target) = value_target else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(stmt_id)),
                "foreach value target is missing",
            );
            return block;
        };
        let value_local = self.variable_local(builder, function, value_target);
        let value_destructure = if value_local.is_none() {
            self.foreach_destructuring_targets(builder, function, value_target)
        } else {
            None
        };
        if value_local.is_none() && value_destructure.is_none() {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(value_target)),
                "foreach value target must be a simple local variable in runtime",
            );
            return block;
        }
        let key_local = if let Some(key_target) = key_target {
            let Some(key_local) = self.variable_local(builder, function, key_target) else {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    self.span_for(SourceMappedId::from(key_target)),
                    "foreach key target must be a simple local variable in runtime",
                );
                return block;
            };
            Some(key_local)
        } else {
            None
        };

        if by_ref {
            let Some(value_local) = value_local else {
                self.unsupported(
                    UnsupportedFeature::ByReferenceForeach,
                    self.span_for(SourceMappedId::from(value_target)),
                    "by-reference foreach value destructuring is outside the reference MVP",
                );
                return block;
            };
            if let Some(source_local) = self.variable_local(builder, function, source) {
                return self.lower_foreach_ref_local(
                    builder,
                    function,
                    block,
                    source_local,
                    key_local,
                    value_local,
                    body,
                    span,
                );
            }
            if let Some(target) = self.dim_assignment_target(builder, function, source) {
                if target.append || target.dims.is_empty() {
                    self.unsupported(
                        UnsupportedFeature::ByReferenceForeach,
                        self.span_for(SourceMappedId::from(source)),
                        "by-reference foreach source append dimensions are outside the reference MVP",
                    );
                    return block;
                }
                let mut current = block;
                let mut dims = Vec::with_capacity(target.dims.len());
                for dim in target.dims {
                    let Some(dim_value) =
                        self.lower_expr_to_register(builder, function, current, dim)
                    else {
                        return block;
                    };
                    current = dim_value.block;
                    dims.push(Operand::Register(dim_value.register));
                }
                let mut source_value = builder.alloc_register(function);
                builder.emit(
                    function,
                    current,
                    InstructionKind::LoadLocal {
                        dst: source_value,
                        local: target.local,
                    },
                    span,
                );
                for dim in dims.iter().cloned() {
                    let next = builder.alloc_register(function);
                    let fetch = builder.emit(
                        function,
                        current,
                        InstructionKind::FetchDim {
                            dst: next,
                            array: Operand::Register(source_value),
                            key: dim,
                            quiet: false,
                        },
                        span,
                    );
                    self.add_expr_source_map(builder, function, current, fetch, source, span);
                    source_value = next;
                }
                let source_local = builder.intern_local(
                    function,
                    format!("__phrust:foreach-ref-dim:{}", stmt_id.raw()),
                );
                builder.emit(
                    function,
                    current,
                    InstructionKind::StoreLocal {
                        local: source_local,
                        src: Operand::Register(source_value),
                    },
                    span,
                );
                let after_block = self.lower_foreach_ref_local(
                    builder,
                    function,
                    current,
                    source_local,
                    key_local,
                    value_local,
                    body,
                    span,
                );
                let writeback = builder.alloc_register(function);
                builder.emit(
                    function,
                    after_block,
                    InstructionKind::LoadLocal {
                        dst: writeback,
                        local: source_local,
                    },
                    span,
                );
                let dst = builder.alloc_register(function);
                builder.emit(
                    function,
                    after_block,
                    InstructionKind::AssignDim {
                        dst,
                        local: target.local,
                        dims,
                        value: Operand::Register(writeback),
                    },
                    span,
                );
                return after_block;
            }
            if let Some(target) = self.property_assignment_target(source) {
                let Some(object) =
                    self.lower_expr_to_register(builder, function, block, target.receiver)
                else {
                    return block;
                };
                let property_value = builder.alloc_register(function);
                let fetch = builder.emit(
                    function,
                    object.block,
                    InstructionKind::FetchProperty {
                        dst: property_value,
                        object: Operand::Register(object.register),
                        property: target.property.clone(),
                    },
                    span,
                );
                self.add_expr_source_map(builder, function, object.block, fetch, source, span);
                let source_local = builder.intern_local(
                    function,
                    format!("__phrust:foreach-ref-property:{}", stmt_id.raw()),
                );
                builder.emit(
                    function,
                    object.block,
                    InstructionKind::StoreLocal {
                        local: source_local,
                        src: Operand::Register(property_value),
                    },
                    span,
                );
                let after_block = self.lower_foreach_ref_local(
                    builder,
                    function,
                    object.block,
                    source_local,
                    key_local,
                    value_local,
                    body,
                    span,
                );
                let writeback = builder.alloc_register(function);
                builder.emit(
                    function,
                    after_block,
                    InstructionKind::LoadLocal {
                        dst: writeback,
                        local: source_local,
                    },
                    span,
                );
                let dst = builder.alloc_register(function);
                builder.emit(
                    function,
                    after_block,
                    InstructionKind::AssignProperty {
                        dst,
                        object: Operand::Register(object.register),
                        property: target.property,
                        value: Operand::Register(writeback),
                    },
                    span,
                );
                return after_block;
            }
            self.unsupported(
                UnsupportedFeature::ByReferenceForeach,
                self.span_for(SourceMappedId::from(source)),
                "by-reference foreach source must be a simple local array variable",
            );
            return block;
        }

        let Some(source_value) = self.lower_expr_to_register(builder, function, block, source)
        else {
            return block;
        };
        let iterator = builder.alloc_register(function);
        builder.emit(
            function,
            source_value.block,
            InstructionKind::ForeachInit {
                iterator,
                source: Operand::Register(source_value.register),
            },
            span,
        );

        let condition_block = builder.append_block(function);
        let body_block = builder.append_block(function);
        let after_block = builder.append_block(function);
        self.jump_if_open(builder, function, source_value.block, condition_block, span);

        let has_value = builder.alloc_register(function);
        let key_reg = key_local.map(|_| builder.alloc_register(function));
        let value_reg = builder.alloc_register(function);
        builder.emit(
            function,
            condition_block,
            InstructionKind::ForeachNext {
                has_value,
                iterator,
                key: key_reg,
                value: value_reg,
            },
            span,
        );
        builder.terminate_jump_if(
            function,
            condition_block,
            Operand::Register(has_value),
            body_block,
            after_block,
            span,
        );

        if let (Some(key_local), Some(key_reg)) = (key_local, key_reg) {
            builder.emit(
                function,
                body_block,
                InstructionKind::StoreLocal {
                    local: key_local,
                    src: Operand::Register(key_reg),
                },
                span,
            );
        }
        let body_entry = if let Some(value_local) = value_local {
            builder.emit(
                function,
                body_block,
                InstructionKind::StoreLocal {
                    local: value_local,
                    src: Operand::Register(value_reg),
                },
                span,
            );
            body_block
        } else {
            self.lower_foreach_value_destructure(
                builder,
                function,
                body_block,
                value_reg,
                value_destructure.unwrap_or_default(),
                span,
            )
        };
        self.loop_stack.push(LoopTargets {
            break_block: after_block,
            continue_block: condition_block,
        });
        let body_end = self.lower_stmt_list(builder, function, body_entry, body);
        self.loop_stack.pop();
        self.jump_if_open(builder, function, body_end, condition_block, span);
        builder.emit(
            function,
            after_block,
            InstructionKind::ForeachCleanup { iterator },
            span,
        );
        after_block
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_foreach_ref_local(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        source_local: LocalId,
        key_local: Option<LocalId>,
        value_local: LocalId,
        body: Vec<StmtId>,
        span: IrSpan,
    ) -> BlockId {
        let iterator = builder.alloc_register(function);
        builder.emit(
            function,
            block,
            InstructionKind::ForeachInitRef {
                iterator,
                local: source_local,
            },
            span,
        );

        let condition_block = builder.append_block(function);
        let body_block = builder.append_block(function);
        let after_block = builder.append_block(function);
        self.jump_if_open(builder, function, block, condition_block, span);

        let has_value = builder.alloc_register(function);
        let key_reg = key_local.map(|_| builder.alloc_register(function));
        builder.emit(
            function,
            condition_block,
            InstructionKind::ForeachNextRef {
                has_value,
                iterator,
                key: key_reg,
                value_local,
            },
            span,
        );
        builder.terminate_jump_if(
            function,
            condition_block,
            Operand::Register(has_value),
            body_block,
            after_block,
            span,
        );

        if let (Some(key_local), Some(key_reg)) = (key_local, key_reg) {
            builder.emit(
                function,
                body_block,
                InstructionKind::StoreLocal {
                    local: key_local,
                    src: Operand::Register(key_reg),
                },
                span,
            );
        }
        self.loop_stack.push(LoopTargets {
            break_block: after_block,
            continue_block: condition_block,
        });
        let body_end = self.lower_stmt_list(builder, function, body_block, body);
        self.loop_stack.pop();
        self.jump_if_open(builder, function, body_end, condition_block, span);
        after_block
    }

    pub(super) fn foreach_destructuring_targets(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        value_target: ExprId,
    ) -> Option<Vec<(IrConstant, DestructuringTarget)>> {
        let patterns = {
            let module = self
                .frontend
                .database()
                .module(self.frontend.module().module_id())?;
            destructuring_patterns(module, value_target, &|parent, fallback, element| {
                self.destructuring_source_index(parent, fallback, element)
            })?
        };
        self.destructuring_targets_from_patterns(builder, function, patterns)
    }

    fn destructuring_source_index(
        &self,
        parent: ExprId,
        fallback: usize,
        element: ExprId,
    ) -> Option<usize> {
        let parent_range = self.span_for(SourceMappedId::from(parent));
        let element_range = self.span_for(SourceMappedId::from(element));
        if element_range.start() <= parent_range.start()
            || element_range.start() > parent_range.end()
        {
            return Some(fallback);
        }
        let source = self.source_text.slice(php_source::TextRange::new(
            parent_range.start().to_usize(),
            element_range.start().to_usize(),
        ))?;
        let index = source.bytes().filter(|byte| *byte == b',').count();
        Some(index)
    }

    pub(super) fn destructuring_targets_from_patterns(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        patterns: Vec<(IrConstant, DestructuringPattern)>,
    ) -> Option<Vec<(IrConstant, DestructuringTarget)>> {
        let mut targets = Vec::new();
        for (key, pattern) in patterns {
            let target = match pattern {
                DestructuringPattern::Expr(expr) => {
                    if let Some(local) = self.variable_local(builder, function, expr) {
                        DestructuringTarget::Local(local)
                    } else if let Some(property) = self.property_assignment_target(expr) {
                        DestructuringTarget::Property(property)
                    } else if let Some(dim) = self.dim_assignment_target(builder, function, expr) {
                        DestructuringTarget::Dim(dim)
                    } else {
                        return None;
                    }
                }
                DestructuringPattern::Nested(children) => DestructuringTarget::Nested(
                    self.destructuring_targets_from_patterns(builder, function, children)?,
                ),
            };
            targets.push((key, target));
        }
        Some(targets)
    }

    pub(super) fn lower_foreach_value_destructure(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        value: RegId,
        targets: Vec<(IrConstant, DestructuringTarget)>,
        span: IrSpan,
    ) -> BlockId {
        let mut current = block;
        for (key, target) in targets {
            let key = builder.intern_constant(key);
            let fetched = builder.alloc_register(function);
            builder.emit(
                function,
                current,
                InstructionKind::FetchDim {
                    dst: fetched,
                    array: Operand::Register(value),
                    key: Operand::Constant(key),
                    quiet: false,
                },
                span,
            );
            match target {
                DestructuringTarget::Local(local) => {
                    builder.emit(
                        function,
                        current,
                        InstructionKind::StoreLocal {
                            local,
                            src: Operand::Register(fetched),
                        },
                        span,
                    );
                }
                DestructuringTarget::Property(target) => {
                    let Some(object) =
                        self.lower_expr_to_register(builder, function, current, target.receiver)
                    else {
                        return current;
                    };
                    current = object.block;
                    let dst = builder.alloc_register(function);
                    builder.emit(
                        function,
                        current,
                        InstructionKind::AssignProperty {
                            dst,
                            object: Operand::Register(object.register),
                            property: target.property,
                            value: Operand::Register(fetched),
                        },
                        span,
                    );
                }
                DestructuringTarget::Dim(target) => {
                    let mut dims = Vec::with_capacity(target.dims.len());
                    for dim in target.dims {
                        let Some(dim_value) =
                            self.lower_expr_to_register(builder, function, current, dim)
                        else {
                            return current;
                        };
                        current = dim_value.block;
                        dims.push(Operand::Register(dim_value.register));
                    }
                    let dst = builder.alloc_register(function);
                    let kind = if target.append {
                        InstructionKind::AppendDim {
                            dst,
                            local: target.local,
                            dims,
                            value: Operand::Register(fetched),
                        }
                    } else {
                        InstructionKind::AssignDim {
                            dst,
                            local: target.local,
                            dims,
                            value: Operand::Register(fetched),
                        }
                    };
                    builder.emit(function, current, kind, span);
                }
                DestructuringTarget::Nested(targets) => {
                    current = self.lower_foreach_value_destructure(
                        builder, function, current, fetched, targets, span,
                    );
                }
            }
        }
        current
    }

    pub(super) fn lower_exit_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        expr: ExprId,
        module: &php_semantics::hir::HirModule,
    ) -> bool {
        let Some(expression) = module.expressions().get(expr) else {
            return false;
        };
        let HirExprKind::Exit { expr: exit_expr } = expression.kind() else {
            return false;
        };
        let range = self.span_for(SourceMappedId::from(expr));
        let span = span_from_range(self.file, range);
        let mut exit_block = block;
        let mut exit_value = None;
        if let Some(exit_expr) = *exit_expr {
            let Some(value) = self.lower_expr_to_register(builder, function, block, exit_expr)
            else {
                return false;
            };
            exit_block = value.block;
            exit_value = Some(Operand::Register(value.register));
        }
        builder.terminate_exit(function, exit_block, exit_value, span);
        builder.add_source_map(
            IrSourceMapTarget::Terminator {
                function,
                block: exit_block,
            },
            format!("hir:expr:{}", expr.raw()),
            span,
        );
        true
    }

    fn lower_short_circuit_exit_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        expr: ExprId,
        module: &php_semantics::hir::HirModule,
    ) -> Option<BlockId> {
        let expression = module.expressions().get(expr)?;
        let HirExprKind::Binary {
            operator,
            left,
            right,
        } = expression.kind()
        else {
            return None;
        };
        if !matches!(operator.as_str(), "&&" | "and" | "||" | "or") {
            return None;
        }
        let left = (*left)?;
        let right = (*right)?;
        if !matches!(
            module.expressions().get(right).map(|expr| expr.kind()),
            Some(HirExprKind::Exit { .. })
        ) {
            return None;
        }

        let range = self.span_for(SourceMappedId::from(expr));
        let span = span_from_range(self.file, range);
        let left_value = self.lower_expr_to_register(builder, function, block, left)?;
        let exit_block = builder.append_block(function);
        let after_block = builder.append_block(function);
        match operator.as_str() {
            "&&" | "and" => builder.terminate_jump_if(
                function,
                left_value.block,
                Operand::Register(left_value.register),
                exit_block,
                after_block,
                span,
            ),
            "||" | "or" => builder.terminate_jump_if(
                function,
                left_value.block,
                Operand::Register(left_value.register),
                after_block,
                exit_block,
                span,
            ),
            _ => return None,
        }
        if !self.lower_exit_stmt(builder, function, exit_block, right, module) {
            return None;
        }
        Some(after_block)
    }

    pub(super) fn lower_return_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        expr: Option<ExprId>,
    ) -> BlockId {
        let range = self.span_for(SourceMappedId::from(stmt_id));
        let span = span_from_range(self.file, range);
        let Some(expr) = expr else {
            builder.terminate_return(function, block, None, span);
            return block;
        };
        if builder.returns_by_ref(function)
            && let Some(local) = self.variable_local(builder, function, expr)
        {
            builder.terminate_return_ref(function, block, local, span);
            return block;
        }
        if builder.returns_by_ref(function)
            && let Some(target) = self.dim_assignment_target(builder, function, expr)
        {
            if target.append || target.dims.is_empty() {
                self.unsupported(
                    UnsupportedFeature::ArrayElementReference,
                    range,
                    "array-element by-reference returns require an existing array element",
                );
                builder.terminate_return(function, block, None, span);
                return block;
            }
            let mut current = block;
            let mut dims = Vec::with_capacity(target.dims.len());
            for dim in target.dims {
                let Some(dim_value) = self.lower_expr_to_register(builder, function, current, dim)
                else {
                    builder.terminate_return(function, current, None, span);
                    return current;
                };
                current = dim_value.block;
                dims.push(Operand::Register(dim_value.register));
            }
            let local = builder.intern_local(
                function,
                format!("__phrust:return-ref-dim:{}", stmt_id.raw()),
            );
            let bind = builder.emit(
                function,
                current,
                InstructionKind::BindReferenceFromDim {
                    target: local,
                    local: target.local,
                    dims,
                },
                span,
            );
            builder.add_source_map(
                IrSourceMapTarget::Instruction {
                    function,
                    block: current,
                    instruction: bind,
                },
                format!("hir:stmt:{}", stmt_id.raw()),
                span,
            );
            builder.terminate_return_ref(function, current, local, span);
            return current;
        }
        if builder.returns_by_ref(function)
            && let Some(target) = self.property_assignment_target(expr)
        {
            let Some(object) =
                self.lower_expr_to_register(builder, function, block, target.receiver)
            else {
                builder.terminate_return(function, block, None, span);
                return block;
            };
            let local = builder.intern_local(
                function,
                format!("__phrust:return-ref-property:{}", stmt_id.raw()),
            );
            let bind = builder.emit(
                function,
                object.block,
                InstructionKind::BindReferenceFromProperty {
                    target: local,
                    object: Operand::Register(object.register),
                    property: target.property,
                },
                span,
            );
            builder.add_source_map(
                IrSourceMapTarget::Instruction {
                    function,
                    block: object.block,
                    instruction: bind,
                },
                format!("hir:stmt:{}", stmt_id.raw()),
                span,
            );
            builder.terminate_return_ref(function, object.block, local, span);
            return object.block;
        }
        if builder.returns_by_ref(function) && self.contains_dim_fetch_expr(expr) {
            self.unsupported(
                UnsupportedFeature::ArrayElementReference,
                range,
                "array-element by-reference returns are a known gap until full reference/COW semantics exist",
            );
            builder.terminate_return(function, block, None, span);
            return block;
        }
        if builder.returns_by_ref(function) && self.contains_property_fetch_expr(expr) {
            self.unsupported(
                UnsupportedFeature::ObjectPropertyReference,
                range,
                "object-property by-reference returns are a known gap until property slots participate in reference/COW semantics",
            );
            builder.terminate_return(function, block, None, span);
            return block;
        }
        let Some(value) = self.lower_expr_to_register(builder, function, block, expr) else {
            builder.terminate_return(function, block, None, span);
            return block;
        };
        builder.terminate_return(
            function,
            value.block,
            Some(Operand::Register(value.register)),
            span,
        );
        block
    }

    pub(super) fn lower_throw_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        expr: Option<ExprId>,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let Some(expr) = expr else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(stmt_id)),
                "throw expression is missing",
            );
            return block;
        };
        let Some(value) = self.lower_expr_to_register(builder, function, block, expr) else {
            return block;
        };
        builder.emit(
            function,
            value.block,
            InstructionKind::Throw {
                value: Operand::Register(value.register),
            },
            span,
        );
        value.block
    }

    pub(super) fn lower_try_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        parts: HirTryParts,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let after_block = builder.append_block(function);
        let body_block = builder.append_block(function);
        let catch_blocks = parts
            .catches
            .iter()
            .map(|_| builder.append_block(function))
            .collect::<Vec<_>>();
        let finally_block =
            (!parts.finally_body.is_empty()).then(|| builder.append_block(function));
        let catch_locals = parts
            .catches
            .iter()
            .map(|catch| {
                catch
                    .variable
                    .as_deref()
                    .map(|name| builder.intern_local(function, name))
            })
            .collect::<Vec<_>>();

        if let Some(finally) = finally_block {
            builder.emit(
                function,
                block,
                InstructionKind::EnterTry {
                    catch: None,
                    catch_types: Vec::new(),
                    finally: Some(finally),
                    after: after_block,
                    exception_local: None,
                },
                span,
            );
        }
        for (index, catch) in parts.catches.iter().enumerate().rev() {
            let catch_types = catch
                .types
                .iter()
                .map(|ty| {
                    normalize_class_name(
                        ty.resolved()
                            .or_else(|| ty.fallback())
                            .unwrap_or_else(|| ty.source()),
                    )
                })
                .collect::<Vec<_>>();
            builder.emit(
                function,
                block,
                InstructionKind::EnterTry {
                    catch: Some(catch_blocks[index]),
                    catch_types,
                    finally: None,
                    after: after_block,
                    exception_local: catch_locals[index],
                },
                span,
            );
        }
        self.jump_if_open(builder, function, block, body_block, span);

        let body_end = self.lower_stmt_list(builder, function, body_block, parts.body);
        if !builder.is_terminated(function, body_end) {
            for _ in 0..parts.catches.len() {
                builder.emit(function, body_end, InstructionKind::LeaveTry, span);
            }
            if finally_block.is_some() {
                builder.emit(function, body_end, InstructionKind::LeaveTry, span);
            }
            self.jump_if_open(
                builder,
                function,
                body_end,
                finally_block.unwrap_or(after_block),
                span,
            );
        }

        let catch_count = parts.catches.len();
        for (index, (catch_block, catch)) in catch_blocks.into_iter().zip(parts.catches).enumerate()
        {
            for _ in 0..catch_count.saturating_sub(index + 1) {
                builder.emit(function, catch_block, InstructionKind::LeaveTry, span);
            }
            let catch_body = catch.body;
            let catch_end = self.lower_stmt_list(builder, function, catch_block, catch_body);
            if !builder.is_terminated(function, catch_end) {
                if finally_block.is_some() {
                    builder.emit(function, catch_end, InstructionKind::LeaveTry, span);
                }
                self.jump_if_open(
                    builder,
                    function,
                    catch_end,
                    finally_block.unwrap_or(after_block),
                    span,
                );
            }
        }

        if let Some(finally_block) = finally_block {
            let finally_end =
                self.lower_stmt_list(builder, function, finally_block, parts.finally_body);
            if !builder.is_terminated(function, finally_end) {
                builder.emit(
                    function,
                    finally_end,
                    InstructionKind::EndFinally { after: after_block },
                    span,
                );
                self.jump_if_open(builder, function, finally_end, after_block, span);
            }
        }

        after_block
    }

    pub(super) fn lower_switch_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        condition: Option<ExprId>,
        cases: Vec<HirSwitchCase>,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let Some(condition) = condition else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(stmt_id)),
                "switch condition is missing",
            );
            return block;
        };
        let Some(subject) = self.lower_expr_to_register(builder, function, block, condition) else {
            return block;
        };
        let after_block = builder.append_block(function);
        let case_blocks = cases
            .iter()
            .map(|_| builder.append_block(function))
            .collect::<Vec<_>>();
        let default_index = cases.iter().position(|case| case.is_default);
        let fallback = default_index
            .map(|index| case_blocks[index])
            .unwrap_or(after_block);
        let conditional_cases = cases
            .iter()
            .enumerate()
            .filter(|(_, case)| !case.is_default)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let mut current_check = subject.block;
        for (position, index) in conditional_cases.iter().copied().enumerate() {
            let case = &cases[index];
            let false_target = if position + 1 == conditional_cases.len() {
                fallback
            } else {
                builder.append_block(function)
            };
            if let Some(condition) = case.condition
                && let Some(case_value) =
                    self.lower_expr_to_register(builder, function, current_check, condition)
            {
                let compare = builder.alloc_register(function);
                builder.emit(
                    function,
                    case_value.block,
                    InstructionKind::Compare {
                        dst: compare,
                        op: CompareOp::Equal,
                        lhs: Operand::Register(subject.register),
                        rhs: Operand::Register(case_value.register),
                    },
                    span,
                );
                builder.terminate_jump_if(
                    function,
                    case_value.block,
                    Operand::Register(compare),
                    case_blocks[index],
                    false_target,
                    span,
                );
            }
            current_check = false_target;
        }
        if conditional_cases.is_empty() {
            self.jump_if_open(builder, function, current_check, fallback, span);
        }

        self.loop_stack.push(LoopTargets {
            break_block: after_block,
            continue_block: after_block,
        });
        for (index, case) in cases.into_iter().enumerate() {
            let body_end = self.lower_stmt_list(builder, function, case_blocks[index], case.body);
            let fallthrough = case_blocks.get(index + 1).copied().unwrap_or(after_block);
            self.jump_if_open(builder, function, body_end, fallthrough, span);
        }
        self.loop_stack.pop();
        after_block
    }

    pub(super) fn lower_stmt_list(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        statements: Vec<StmtId>,
    ) -> BlockId {
        let labels = self.collect_label_statements(&statements);
        let has_labels = !labels.is_empty();
        self.ensure_label_blocks(builder, function, labels);
        let mut current = block;
        for stmt in statements {
            if builder.is_terminated(function, current) && !self.is_label_stmt(stmt) {
                if has_labels {
                    continue;
                }
                break;
            }
            current = self.lower_stmt(builder, function, current, stmt);
        }
        current
    }

    pub(super) fn lower_expr_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        expr: ExprId,
    ) -> BlockId {
        if let Some(value) = self.lower_expr_to_register(builder, function, block, expr) {
            let span = span_from_range(self.file, self.span_for(SourceMappedId::from(expr)));
            let discard = builder.emit(
                function,
                value.block,
                InstructionKind::Discard {
                    src: Operand::Register(value.register),
                },
                span,
            );
            self.add_expr_source_map(builder, function, value.block, discard, expr, span);
            return value.block;
        }
        block
    }

    pub(super) fn lower_unset_stmt(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        stmt_id: StmtId,
        expressions: Vec<ExprId>,
    ) -> BlockId {
        let span = span_from_range(self.file, self.span_for(SourceMappedId::from(stmt_id)));
        let mut current = block;
        for expr in expressions {
            if let Some(local) = self.variable_local(builder, function, expr) {
                builder.emit(
                    function,
                    current,
                    InstructionKind::UnsetLocal { local },
                    span,
                );
                continue;
            }
            if let Some(target) = self.property_assignment_target(expr) {
                let Some(object) =
                    self.lower_expr_to_register(builder, function, current, target.receiver)
                else {
                    continue;
                };
                current = object.block;
                builder.emit(
                    function,
                    current,
                    InstructionKind::UnsetProperty {
                        object: Operand::Register(object.register),
                        property: target.property,
                    },
                    span,
                );
                continue;
            }
            if let Some(target) = self.dynamic_property_target(expr) {
                let Some(object) =
                    self.lower_expr_to_register(builder, function, current, target.receiver)
                else {
                    continue;
                };
                current = object.block;
                let property_range = self.span_for(SourceMappedId::from(target.property));
                let property_site = LowerSite {
                    function,
                    block: current,
                    expr: target.property,
                    span: span_from_range(self.file, property_range),
                    range: property_range,
                };
                let Some(property) = self.lower_dynamic_member_name_to_register(
                    builder,
                    property_site,
                    current,
                    target.property,
                ) else {
                    continue;
                };
                current = property.block;
                builder.emit(
                    function,
                    current,
                    InstructionKind::UnsetDynamicProperty {
                        object: Operand::Register(object.register),
                        property: Operand::Register(property.register),
                    },
                    span,
                );
                continue;
            }
            if let Some(target) = self.property_dim_target(expr) {
                if target.append {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        self.span_for(SourceMappedId::from(expr)),
                        "unset of append dimension is invalid for the runtime MVP",
                    );
                    continue;
                }
                let Some(object) =
                    self.lower_expr_to_register(builder, function, current, target.receiver)
                else {
                    continue;
                };
                current = object.block;
                let mut dims = Vec::with_capacity(target.dims.len());
                for dim in target.dims {
                    let Some(dim_value) =
                        self.lower_expr_to_register(builder, function, current, dim)
                    else {
                        continue;
                    };
                    current = dim_value.block;
                    dims.push(Operand::Register(dim_value.register));
                }
                builder.emit(
                    function,
                    current,
                    InstructionKind::UnsetPropertyDim {
                        object: Operand::Register(object.register),
                        property: target.property,
                        dims,
                    },
                    span,
                );
                continue;
            }
            if let Some(target) = self.static_property_dim_target(expr) {
                if target.append {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        self.span_for(SourceMappedId::from(expr)),
                        "unset of append static-property dimension is invalid for the runtime MVP",
                    );
                    continue;
                }
                let mut dims = Vec::with_capacity(target.dims.len());
                for dim in target.dims {
                    let Some(dim_value) =
                        self.lower_expr_to_register(builder, function, current, dim)
                    else {
                        continue;
                    };
                    current = dim_value.block;
                    dims.push(Operand::Register(dim_value.register));
                }
                builder.emit(
                    function,
                    current,
                    InstructionKind::UnsetStaticPropertyDim {
                        class_name: target.class_name,
                        property: target.property,
                        dims,
                    },
                    span,
                );
                continue;
            }
            let Some(target) = self.dim_assignment_target(builder, function, expr) else {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    self.span_for(SourceMappedId::from(expr)),
                    "unset only supports locals, properties, and local array dimensions in runtime-semantics",
                );
                continue;
            };
            if target.append {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    self.span_for(SourceMappedId::from(expr)),
                    "unset of append dimension is invalid for the runtime MVP",
                );
                continue;
            }
            let mut dims = Vec::with_capacity(target.dims.len());
            for dim in target.dims {
                let Some(dim_value) = self.lower_expr_to_register(builder, function, current, dim)
                else {
                    continue;
                };
                current = dim_value.block;
                dims.push(Operand::Register(dim_value.register));
            }
            builder.emit(
                function,
                current,
                InstructionKind::UnsetDim {
                    local: target.local,
                    dims,
                },
                span,
            );
        }
        current
    }
}
