use crate::constants::IrConstant;
use crate::ids::{BlockId, FunctionId, LocalId, RegId};
use crate::instruction::{BinaryOp, CastKind, CompareOp, IncludeKind, UnaryOp};
use crate::source_map::IrSpan;
use php_semantics::hir::ExprId;
use php_source::TextRange;

use super::consts::*;
use super::declarations::*;
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct LowerSite {
    pub(super) function: FunctionId,
    pub(super) block: BlockId,
    pub(super) expr: ExprId,
    pub(super) span: IrSpan,
    pub(super) range: TextRange,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LoweredExpr {
    pub(super) register: RegId,
    pub(super) block: BlockId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CaptureSpec {
    pub(super) name: String,
    pub(super) by_ref: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct StaticLocalSpec {
    pub(super) name: String,
    pub(super) initializer: Option<ExprId>,
}

#[derive(Clone, Debug)]
pub(super) struct DimAssignmentTarget {
    pub(super) local: LocalId,
    pub(super) dims: Vec<ExprId>,
    pub(super) append: bool,
}

#[derive(Clone, Debug)]
pub(super) struct PropertyAssignmentTarget {
    pub(super) receiver: ExprId,
    pub(super) property: String,
}

#[derive(Clone, Debug)]
pub(super) enum DestructuringTarget {
    Local(LocalId),
    Property(PropertyAssignmentTarget),
    Dim(DimAssignmentTarget),
    Nested(Vec<(IrConstant, DestructuringTarget)>),
}

#[derive(Clone, Debug)]
pub(super) enum DestructuringPattern {
    Expr(ExprId),
    Nested(Vec<(IrConstant, DestructuringPattern)>),
}

#[derive(Clone, Debug)]
pub(super) struct DynamicPropertyTarget {
    pub(super) receiver: ExprId,
    pub(super) property: ExprId,
}

#[derive(Clone, Debug)]
pub(super) struct DynamicPropertyDimTarget {
    pub(super) receiver: ExprId,
    pub(super) property: ExprId,
    pub(super) dims: Vec<ExprId>,
    pub(super) append: bool,
}

#[derive(Clone, Debug)]
pub(super) struct DynamicMethodCallTarget {
    pub(super) receiver: ExprId,
    pub(super) method: ExprId,
}

#[derive(Clone, Debug)]
pub(super) struct PropertyDimTarget {
    pub(super) receiver: ExprId,
    pub(super) property: String,
    pub(super) dims: Vec<ExprId>,
    pub(super) append: bool,
}

#[derive(Clone, Debug)]
pub(super) struct StaticPropertyTarget {
    pub(super) class_name: String,
    pub(super) property: String,
}

#[derive(Clone, Debug)]
pub(super) struct DynamicStaticPropertyTarget {
    pub(super) class_name: ExprId,
    pub(super) property: String,
}

#[derive(Clone, Debug)]
pub(super) struct StaticPropertyDimTarget {
    pub(super) class_name: String,
    pub(super) property: String,
    pub(super) dims: Vec<ExprId>,
    pub(super) append: bool,
}

#[derive(Clone, Debug)]
pub(super) struct ClassConstantDimTarget {
    pub(super) class_name: String,
    pub(super) display_class_name: Option<String>,
    pub(super) constant: String,
    pub(super) dims: Vec<ExprId>,
    pub(super) append: bool,
}

#[derive(Clone, Debug)]
pub(super) struct ClassConstantTarget {
    pub(super) class_name: String,
    pub(super) display_class_name: Option<String>,
    pub(super) constant: String,
    pub(super) target_expr: ExprId,
}

#[derive(Clone, Debug)]
pub(super) struct ObjectClassNameTarget {
    pub(super) object: ExprId,
}

#[derive(Clone, Debug)]
pub(super) struct MethodCallTarget {
    pub(super) receiver: ExprId,
    pub(super) method: String,
}

#[derive(Clone, Debug)]
pub(super) struct StaticMethodCallTarget {
    pub(super) class_name: String,
    pub(super) display_class_name: Option<String>,
    pub(super) method: String,
}

#[derive(Clone, Debug)]
pub(super) enum CallableComponent {
    Expr(ExprId),
    String(String),
}

pub(super) fn unary_op(operator: &str) -> Option<UnaryOp> {
    match operator {
        "+" => Some(UnaryOp::Plus),
        "-" => Some(UnaryOp::Minus),
        "!" => Some(UnaryOp::Not),
        "~" => Some(UnaryOp::BitNot),
        _ => None,
    }
}

pub(super) fn binary_op(operator: &str) -> Option<BinaryOp> {
    match operator {
        "+" => Some(BinaryOp::Add),
        "-" => Some(BinaryOp::Sub),
        "*" => Some(BinaryOp::Mul),
        "/" => Some(BinaryOp::Div),
        "%" => Some(BinaryOp::Mod),
        "**" => Some(BinaryOp::Pow),
        "." => Some(BinaryOp::Concat),
        "&" => Some(BinaryOp::BitAnd),
        "|" => Some(BinaryOp::BitOr),
        "^" => Some(BinaryOp::BitXor),
        "<<" => Some(BinaryOp::ShiftLeft),
        ">>" => Some(BinaryOp::ShiftRight),
        _ => None,
    }
}

pub(super) fn assignment_binary_op(operator: &str) -> Option<BinaryOp> {
    match operator {
        "+=" => Some(BinaryOp::Add),
        "-=" => Some(BinaryOp::Sub),
        "*=" => Some(BinaryOp::Mul),
        "/=" => Some(BinaryOp::Div),
        "%=" => Some(BinaryOp::Mod),
        "**=" => Some(BinaryOp::Pow),
        ".=" => Some(BinaryOp::Concat),
        "&=" => Some(BinaryOp::BitAnd),
        "|=" => Some(BinaryOp::BitOr),
        "^=" => Some(BinaryOp::BitXor),
        "<<=" => Some(BinaryOp::ShiftLeft),
        ">>=" => Some(BinaryOp::ShiftRight),
        _ => None,
    }
}

pub(super) fn compare_op(operator: &str) -> Option<CompareOp> {
    match operator {
        "==" => Some(CompareOp::Equal),
        "===" => Some(CompareOp::Identical),
        "!=" | "<>" => Some(CompareOp::NotEqual),
        "!==" => Some(CompareOp::NotIdentical),
        "<" => Some(CompareOp::Less),
        "<=" => Some(CompareOp::LessEqual),
        ">" => Some(CompareOp::Greater),
        ">=" => Some(CompareOp::GreaterEqual),
        "<=>" => Some(CompareOp::Spaceship),
        _ => None,
    }
}

pub(super) fn cast_kind(kind: &str) -> Option<CastKind> {
    let normalized = kind
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .to_ascii_lowercase();
    match normalized.as_str() {
        "bool" | "boolean" => Some(CastKind::Bool),
        "int" | "integer" => Some(CastKind::Int),
        "float" | "double" | "real" => Some(CastKind::Float),
        "string" => Some(CastKind::String),
        "array" => Some(CastKind::Array),
        "object" => Some(CastKind::Object),
        "void" => Some(CastKind::Void),
        _ => None,
    }
}

pub(super) fn include_kind(kind: &str) -> Option<IncludeKind> {
    match kind.to_ascii_lowercase().as_str() {
        "include" => Some(IncludeKind::Include),
        "include_once" => Some(IncludeKind::IncludeOnce),
        "require" => Some(IncludeKind::Require),
        "require_once" => Some(IncludeKind::RequireOnce),
        _ => None,
    }
}

impl LoweringContext<'_> {
    pub(super) fn lower_param_default(&self, param: &Parameter) -> Option<IrConstant> {
        self.lower_param_default_with_class_constants(param, None, &HashMap::new(), &HashMap::new())
    }

    pub(super) fn lower_param_default_with_class_constants(
        &self,
        param: &Parameter,
        current_class: Option<&str>,
        class_constants: &ClassConstantInitializerMap,
        class_parents: &ClassParentMap,
    ) -> Option<IrConstant> {
        let default = param.default()?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let named_constants = self.global_constant_initializer_map();
        if !default.is_const_expr_candidate() {
            return self
                .source_text
                .as_str()
                .get(default.span().start().to_usize()..default.span().end().to_usize())
                .and_then(|source| {
                    named_constant_from_default_source(source, &named_constants).or_else(|| {
                        class_constant_from_default_source(
                            module,
                            source,
                            current_class,
                            &named_constants,
                            class_constants,
                            class_parents,
                        )
                    })
                });
        }
        module
            .const_exprs()
            .iter()
            .filter_map(|(id, const_expr)| {
                if const_expr.context() != ConstExprContext::ParameterDefault
                    || !const_expr.is_allowed()
                {
                    return None;
                }
                let span = self.frontend.database().source_map().span(id)?;
                if !ranges_overlap(default.span(), span) {
                    return None;
                }
                Some((span, const_expr))
            })
            .max_by_key(|(span, _)| {
                (
                    range_overlap_len(default.span(), *span),
                    span.end()
                        .to_usize()
                        .saturating_sub(span.start().to_usize()),
                )
            })
            .and_then(|(_, const_expr)| {
                if let Some(value) = const_expr
                    .folded_value()
                    .and_then(ir_constant_from_const_value)
                {
                    return Some(value);
                }
                constant_from_expr_with_runtime_constants(
                    module,
                    const_expr.expr_id(),
                    &named_constants,
                    current_class,
                    class_constants,
                    class_parents,
                    &mut Vec::new(),
                )
            })
            .or_else(|| {
                constant_from_overlapping_default_expr(
                    self.frontend,
                    module,
                    default,
                    &named_constants,
                    current_class,
                    class_constants,
                    class_parents,
                )
            })
            .or_else(|| {
                self.source_text
                    .as_str()
                    .get(default.span().start().to_usize()..default.span().end().to_usize())
                    .and_then(|source| {
                        named_constant_from_default_source(source, &named_constants)
                            .or_else(|| {
                                class_constant_from_default_source(
                                    module,
                                    source,
                                    current_class,
                                    &named_constants,
                                    class_constants,
                                    class_parents,
                                )
                            })
                            .or_else(|| {
                                source_constant_from_default_source(source, &named_constants)
                            })
                    })
            })
    }

    pub(super) fn lower_param_runtime_type(
        &self,
        param: &Parameter,
        default: &Option<IrConstant>,
    ) -> Option<IrReturnType> {
        let type_ = self.lower_runtime_type(param.type_id())?;
        if self.param_default_triggers_implicit_nullable_deprecation(param, default) {
            return Some(IrReturnType::Nullable {
                inner: Box::new(type_),
            });
        }
        Some(type_)
    }

    pub(super) fn param_default_triggers_implicit_nullable_deprecation(
        &self,
        param: &Parameter,
        default: &Option<IrConstant>,
    ) -> bool {
        default == &Some(IrConstant::Null)
            && self.param_type_triggers_implicit_nullable_deprecation(param)
    }

    pub(super) fn param_type_triggers_implicit_nullable_deprecation(
        &self,
        param: &Parameter,
    ) -> bool {
        let Some(type_id) = param.type_id() else {
            return false;
        };
        !self.type_accepts_null(type_id)
    }

    pub(super) fn lower_deferred_property_default(
        &self,
        default: Option<ConstExprId>,
        current_class: Option<&str>,
        current_class_display: Option<&str>,
        class_constants: &ClassConstantInitializerMap,
        class_parents: &ClassParentMap,
    ) -> Option<DeferredConstExpr> {
        let const_expr_id = default?;
        if let Some(value) =
            self.lower_const_expr_magic_constant(const_expr_id, current_class_display)
        {
            return Some(DeferredConstExpr::Literal(value));
        }
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let const_expr = module.const_exprs().get(const_expr_id)?;
        if !matches!(
            const_expr.context(),
            ConstExprContext::PropertyDefault | ConstExprContext::PromotedPropertyDefault
        ) || !const_expr.is_allowed()
        {
            return None;
        }
        let named_constants = self.global_constant_initializer_map();
        let mut visiting_class_constants = Vec::new();
        let mut input = DeferredConstExprLoweringInput {
            module,
            named_constants: &named_constants,
            current_class,
            class_constants,
            class_parents,
            visiting_class_constants: &mut visiting_class_constants,
        };
        self.lower_deferred_const_expr(&mut input, const_expr.expr_id())
    }

    pub(super) fn lower_deferred_const_expr(
        &self,
        input: &mut DeferredConstExprLoweringInput<'_>,
        expr_id: ExprId,
    ) -> Option<DeferredConstExpr> {
        if let Some(value) = constant_from_expr_with_class_constants(
            input.module,
            expr_id,
            input.named_constants,
            input.current_class,
            input.class_constants,
            input.class_parents,
            input.visiting_class_constants,
        ) {
            return Some(DeferredConstExpr::Literal(value));
        }

        let expr = input.module.expressions().get(expr_id)?;
        match expr.kind() {
            HirExprKind::Literal { text } => literal_constant(text).map(DeferredConstExpr::Literal),
            HirExprKind::Name { resolution } => language_constant(resolution.source())
                .or_else(|| named_constant_value(input.named_constants, resolution))
                .map(DeferredConstExpr::Literal)
                .or_else(|| {
                    named_constant_reference_from_resolution(resolution)
                        .map(DeferredConstExpr::NamedConstant)
                }),
            HirExprKind::StaticAccess { target, member } => self
                .lower_deferred_class_constant_reference(
                    input.module,
                    *target,
                    *member,
                    input.current_class,
                    input.class_parents,
                )
                .map(DeferredConstExpr::ClassConstant),
            HirExprKind::Array { elements } => {
                let mut entries = Vec::with_capacity(elements.len());
                for element_id in elements {
                    let element = input.module.expressions().get(*element_id)?;
                    match element.kind() {
                        HirExprKind::ArrayPair {
                            key,
                            value,
                            unpack,
                            by_ref,
                        } => {
                            if *unpack || *by_ref {
                                return None;
                            }
                            let key = match key {
                                Some(key) => Some(self.lower_deferred_const_expr(input, *key)?),
                                None => None,
                            };
                            let value = self.lower_deferred_const_expr(input, (*value)?)?;
                            entries.push(DeferredConstArrayEntry { key, value });
                        }
                        _ => {
                            let value = self.lower_deferred_const_expr(input, *element_id)?;
                            entries.push(DeferredConstArrayEntry { key: None, value });
                        }
                    }
                }
                Some(DeferredConstExpr::Array(entries))
            }
            _ => None,
        }
    }

    pub(super) fn lower_deferred_class_constant_reference(
        &self,
        module: &HirModule,
        target: Option<ExprId>,
        member: Option<ExprId>,
        current_class: Option<&str>,
        class_parents: &ClassParentMap,
    ) -> Option<ClassConstantReference> {
        Some(ClassConstantReference {
            class_name: class_constant_initializer_target_class(
                module,
                target?,
                current_class,
                class_parents,
            )?,
            display_class_name: class_constant_initializer_target_display_class(
                module,
                target?,
                current_class,
                class_parents,
            )?,
            constant_name: class_constant_initializer_member_name(module, member?)?,
        })
    }

    pub(super) fn lower_const_expr_magic_constant(
        &self,
        const_expr_id: ConstExprId,
        current_class_display: Option<&str>,
    ) -> Option<IrConstant> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let const_expr = module.const_exprs().get(const_expr_id)?;
        let expr = module.expressions().get(const_expr.expr_id())?;
        let text = match expr.kind() {
            HirExprKind::Name { resolution } => resolution.source(),
            HirExprKind::Literal { text } => text,
            _ => return None,
        };
        let span = self.span_for(SourceMappedId::from(const_expr_id));
        match text.to_ascii_uppercase().as_str() {
            "__FILE__" => Some(IrConstant::String(self.options.source_path.clone())),
            "__DIR__" => Some(IrConstant::String(source_dir(&self.options.source_path))),
            "__LINE__" => Some(IrConstant::Int(
                self.source_text
                    .line_col(BytePos::new(span.start().to_usize()))
                    .line as i64,
            )),
            "__CLASS__" => Some(IrConstant::String(
                current_class_display.unwrap_or_default().to_owned(),
            )),
            "__METHOD__" | "__FUNCTION__" => Some(IrConstant::String(String::new())),
            "__NAMESPACE__" => Some(IrConstant::String(
                namespace_name_for_span(module, span).unwrap_or_default(),
            )),
            _ => None,
        }
    }

    pub(super) fn lower_enum_case_value(&self, value: Option<ConstExprId>) -> Option<IrConstant> {
        let value = value?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let const_expr = module.const_exprs().get(value)?;
        if const_expr.context() != ConstExprContext::EnumCaseBackingValue
            || !const_expr.is_allowed()
        {
            return None;
        }
        let named_constants = self.global_constant_initializer_map();
        constant_from_expr_with_names(module, const_expr.expr_id(), &named_constants).or_else(
            || {
                const_expr
                    .folded_value()
                    .and_then(ir_constant_from_const_value)
            },
        )
    }

    pub(super) fn lower_enum_backing_type(
        &self,
        class_like: &php_semantics::hir::HirClassLike,
    ) -> Option<ClassEnumBackingType> {
        let type_id = class_like.backing_type()?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let ty = module.types().get(type_id)?;
        match ty.kind() {
            HirTypeKind::Builtin(BuiltinType::Int) => Some(ClassEnumBackingType::Int),
            HirTypeKind::Builtin(BuiltinType::String) => Some(ClassEnumBackingType::String),
            _ => None,
        }
    }

    pub(super) fn lower_attribute_ids(
        &self,
        builder: &mut IrBuilder,
        ids: &[AttributeId],
    ) -> Vec<AttributeEntry> {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|id| {
                let attribute = module.attributes().get(*id)?;
                let span = span_from_range(self.file, self.span_for(SourceMappedId::from(*id)));
                let arguments = attribute
                    .args()
                    .iter()
                    .filter_map(|expr| self.lower_attribute_argument(*expr))
                    .map(|constant| builder.intern_constant(constant))
                    .collect();
                Some(AttributeEntry {
                    name: attribute.name().source().to_owned(),
                    resolved_name: attribute.name().resolved().map(ToOwned::to_owned),
                    fallback_name: attribute.name().fallback().map(ToOwned::to_owned),
                    arguments,
                    repeated_on_target: attribute.is_repeated_on_target(),
                    span,
                })
            })
            .collect()
    }

    pub(super) fn lower_attribute_argument(&self, expr_id: ExprId) -> Option<IrConstant> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        module.const_exprs().iter().find_map(|(_, const_expr)| {
            if const_expr.context() != ConstExprContext::AttributeArgument
                || const_expr.expr_id() != expr_id
                || !const_expr.is_allowed()
            {
                return None;
            }
            let named_constants = self.global_constant_initializer_map();
            if let Some(value) = constant_from_expr_with_names(module, expr_id, &named_constants) {
                return Some(value);
            }
            if let Some(value) = const_expr.folded_value() {
                return ir_constant_from_const_value(value);
            }
            let expr = module.expressions().get(expr_id)?;
            match expr.kind() {
                HirExprKind::Literal { text } => literal_constant(text),
                _ => None,
            }
        })
    }

    pub(super) fn lower_const_expr_value(
        &self,
        const_expr_id: ConstExprId,
        accepts_context: impl Fn(ConstExprContext) -> bool,
        current_class: Option<&str>,
        class_constants: &ClassConstantInitializerMap,
        class_parents: &ClassParentMap,
    ) -> Option<IrConstant> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let const_expr = module.const_exprs().get(const_expr_id)?;
        if !accepts_context(const_expr.context()) || !const_expr.is_allowed() {
            return None;
        }
        let mut visiting = Vec::new();
        let named_constants = self.global_constant_initializer_map();
        constant_from_expr_with_runtime_constants(
            module,
            const_expr.expr_id(),
            &named_constants,
            current_class,
            class_constants,
            class_parents,
            &mut visiting,
        )
        .or_else(|| {
            const_expr
                .folded_value()
                .and_then(ir_constant_from_const_value)
        })
    }

    pub(super) fn lower_const_expr_class_constant_reference(
        &self,
        const_expr_id: Option<ConstExprId>,
        accepts_context: impl Fn(ConstExprContext) -> bool,
        current_class: Option<&str>,
        class_parents: &ClassParentMap,
    ) -> Option<ClassConstantReference> {
        let const_expr_id = const_expr_id?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let const_expr = module.const_exprs().get(const_expr_id)?;
        if !accepts_context(const_expr.context()) || !const_expr.is_allowed() {
            return None;
        }
        let expr = module.expressions().get(const_expr.expr_id())?;
        let HirExprKind::StaticAccess { target, member } = expr.kind() else {
            return None;
        };
        Some(ClassConstantReference {
            class_name: class_constant_initializer_target_class(
                module,
                (*target)?,
                current_class,
                class_parents,
            )?,
            display_class_name: class_constant_initializer_target_display_class(
                module,
                (*target)?,
                current_class,
                class_parents,
            )?,
            constant_name: class_constant_initializer_member_name(module, (*member)?)?,
        })
    }

    pub(super) fn lower_const_expr_named_constant_reference(
        &self,
        const_expr_id: Option<ConstExprId>,
        accepts_context: impl Fn(ConstExprContext) -> bool,
    ) -> Option<NamedConstantReference> {
        let const_expr_id = const_expr_id?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let const_expr = module.const_exprs().get(const_expr_id)?;
        if !accepts_context(const_expr.context()) || !const_expr.is_allowed() {
            return None;
        }
        let expr = module.expressions().get(const_expr.expr_id())?;
        let HirExprKind::Name { resolution } = expr.kind() else {
            return None;
        };
        if language_constant(resolution.source()).is_some()
            || self
                .lower_const_expr_magic_constant(const_expr_id, None)
                .is_some()
        {
            return None;
        }
        let mut names = Vec::new();
        for candidate in [
            resolution.resolved(),
            resolution.fallback(),
            Some(resolution.source()),
            resolution.source().strip_prefix('\\'),
        ]
        .into_iter()
        .flatten()
        {
            let name = candidate.trim_start_matches('\\').to_owned();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        (!names.is_empty()).then(|| NamedConstantReference {
            display_name: resolution.source().trim_start_matches('\\').to_owned(),
            names,
        })
    }

    pub(super) fn dim_assignment_target(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        expr: ExprId,
    ) -> Option<DimAssignmentTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind().clone() {
            HirExprKind::Variable { name } => Some(DimAssignmentTarget {
                local: builder.intern_local(function, local_name(&name)),
                dims: Vec::new(),
                append: false,
            }),
            HirExprKind::DimFetch { receiver, dim } => {
                let receiver = receiver?;
                let mut target = self.dim_assignment_target(builder, function, receiver)?;
                if target.append && dim.is_none() {
                    return None;
                }
                if let Some(dim) = dim {
                    target.dims.push(dim);
                } else {
                    target.append = true;
                }
                Some(target)
            }
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_expr_to_register(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        expr: ExprId,
    ) -> Option<LoweredExpr> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let range = self.span_for(SourceMappedId::from(expr));
        let span = span_from_range(self.file, range);
        let site = LowerSite {
            function,
            block,
            expr,
            span,
            range,
        };
        let kind = expression.kind().clone();
        match kind {
            HirExprKind::Literal { text } => {
                if let Some(callable_name) = zero_arg_variable_call_name(&text) {
                    let callee_local = builder.intern_local(function, callable_name);
                    let callee = builder.alloc_register(function);
                    let load = builder.emit(
                        function,
                        block,
                        InstructionKind::LoadLocal {
                            dst: callee,
                            local: callee_local,
                        },
                        span,
                    );
                    self.add_expr_source_map(builder, function, block, load, expr, span);
                    let dst = builder.alloc_register(function);
                    let call = builder.emit(
                        function,
                        block,
                        InstructionKind::CallCallable {
                            dst,
                            callee: Operand::Register(callee),
                            args: Vec::new(),
                        },
                        span,
                    );
                    self.add_expr_source_map(builder, function, block, call, expr, span);
                    return Some(LoweredExpr {
                        register: dst,
                        block,
                    });
                }
                if text.starts_with('$') {
                    let local = builder.intern_local(function, local_name(&text));
                    let dst = builder.alloc_register(function);
                    let instruction = builder.emit(
                        function,
                        block,
                        InstructionKind::LoadLocal { dst, local },
                        span,
                    );
                    self.add_expr_source_map(builder, function, block, instruction, expr, span);
                    return Some(LoweredExpr {
                        register: dst,
                        block,
                    });
                }
                if let Some(constant) = self.magic_constant(&text, site) {
                    return Some(self.emit_constant_to_register(builder, site, constant));
                }
                self.lower_literal_to_register(builder, site, &text)
            }
            HirExprKind::Name { resolution } => {
                if let Some(constant) = language_constant(resolution.source()) {
                    return Some(self.emit_constant_to_register(builder, site, constant));
                }
                let name = resolution
                    .resolved()
                    .or_else(|| resolution.fallback())
                    .unwrap_or_else(|| resolution.source());
                let dst = builder.alloc_register(function);
                let instruction = builder.emit(
                    function,
                    block,
                    InstructionKind::FetchConst {
                        dst,
                        name: name.trim_start_matches('\\').to_string(),
                    },
                    span,
                );
                self.add_expr_source_map(builder, function, block, instruction, expr, span);
                Some(LoweredExpr {
                    register: dst,
                    block,
                })
            }
            HirExprKind::Variable { name } => {
                if let Some(callable_name) = zero_arg_variable_call_name(&name) {
                    let callee_local = builder.intern_local(function, callable_name);
                    let callee = builder.alloc_register(function);
                    let load = builder.emit(
                        function,
                        block,
                        InstructionKind::LoadLocal {
                            dst: callee,
                            local: callee_local,
                        },
                        span,
                    );
                    self.add_expr_source_map(builder, function, block, load, expr, span);
                    let dst = builder.alloc_register(function);
                    let call = builder.emit(
                        function,
                        block,
                        InstructionKind::CallCallable {
                            dst,
                            callee: Operand::Register(callee),
                            args: Vec::new(),
                        },
                        span,
                    );
                    self.add_expr_source_map(builder, function, block, call, expr, span);
                    return Some(LoweredExpr {
                        register: dst,
                        block,
                    });
                }
                let local = builder.intern_local(function, local_name(&name));
                let dst = builder.alloc_register(function);
                let instruction = builder.emit(
                    function,
                    block,
                    InstructionKind::LoadLocal { dst, local },
                    span,
                );
                self.add_expr_source_map(builder, function, block, instruction, expr, span);
                Some(LoweredExpr {
                    register: dst,
                    block,
                })
            }
            HirExprKind::Unary {
                operator,
                expr: inner,
            } if operator == "parenthesized" => {
                inner.and_then(|inner| self.lower_expr_to_register(builder, function, block, inner))
            }
            HirExprKind::Unary {
                operator,
                expr: inner,
            } => {
                if operator == "@" {
                    return self.lower_error_suppression_to_register(builder, site, inner);
                }
                if let Some(cast) = cast_kind(&operator) {
                    return self.lower_cast_to_register(builder, site, inner, cast);
                }
                if matches!(operator.as_str(), "++" | "--") {
                    return self.lower_inc_dec_to_register(builder, site, inner, &operator);
                }
                let Some(op) = unary_op(&operator) else {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        range,
                        format!("unary operator `{operator}` is not lowered to IR yet"),
                    );
                    return None;
                };
                let Some(inner) = inner else {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        range,
                        "unary expression is missing its operand",
                    );
                    return None;
                };
                let src = self.lower_expr_to_register(builder, function, block, inner)?;
                let dst = builder.alloc_register(function);
                let instruction = builder.emit(
                    function,
                    src.block,
                    InstructionKind::Unary {
                        dst,
                        op,
                        src: Operand::Register(src.register),
                    },
                    span,
                );
                self.add_expr_source_map(builder, function, src.block, instruction, expr, span);
                Some(LoweredExpr {
                    register: dst,
                    block: src.block,
                })
            }
            HirExprKind::Binary {
                operator,
                left,
                right,
            } => {
                if operator == "xor" {
                    return self.lower_logical_xor_to_register(builder, site, left, right);
                }
                if matches!(operator.as_str(), "&&" | "and" | "||" | "or" | "??") {
                    return self
                        .lower_short_circuit_to_register(builder, site, &operator, left, right);
                }
                let Some(left) = left else {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        range,
                        "binary expression is missing its left operand",
                    );
                    return None;
                };
                let Some(right) = right else {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        range,
                        "binary expression is missing its right operand",
                    );
                    return None;
                };
                let lhs = self.lower_expr_to_register(builder, function, block, left)?;
                if operator == "instanceof" {
                    if let Some(class_name) = self.instanceof_class_name(right) {
                        let dst = builder.alloc_register(function);
                        let instruction = builder.emit(
                            function,
                            lhs.block,
                            InstructionKind::InstanceOf {
                                dst,
                                object: Operand::Register(lhs.register),
                                class_name,
                            },
                            span,
                        );
                        self.add_expr_source_map(
                            builder,
                            function,
                            lhs.block,
                            instruction,
                            expr,
                            span,
                        );
                        return Some(LoweredExpr {
                            register: dst,
                            block: lhs.block,
                        });
                    };
                    let rhs = self.lower_expr_to_register(builder, function, lhs.block, right)?;
                    let dst = builder.alloc_register(function);
                    let instruction = builder.emit(
                        function,
                        rhs.block,
                        InstructionKind::DynamicInstanceOf {
                            dst,
                            object: Operand::Register(lhs.register),
                            target: Operand::Register(rhs.register),
                        },
                        span,
                    );
                    self.add_expr_source_map(builder, function, rhs.block, instruction, expr, span);
                    return Some(LoweredExpr {
                        register: dst,
                        block: rhs.block,
                    });
                }
                let rhs = self.lower_expr_to_register(builder, function, lhs.block, right)?;
                let dst = builder.alloc_register(function);
                let kind = if let Some(op) = binary_op(&operator) {
                    InstructionKind::Binary {
                        dst,
                        op,
                        lhs: Operand::Register(lhs.register),
                        rhs: Operand::Register(rhs.register),
                    }
                } else if let Some(op) = compare_op(&operator) {
                    InstructionKind::Compare {
                        dst,
                        op,
                        lhs: Operand::Register(lhs.register),
                        rhs: Operand::Register(rhs.register),
                    }
                } else {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        range,
                        format!("binary operator `{operator}` is not lowered to IR yet"),
                    );
                    return None;
                };
                let instruction = builder.emit(function, rhs.block, kind, span);
                self.add_expr_source_map(builder, function, rhs.block, instruction, expr, span);
                Some(LoweredExpr {
                    register: dst,
                    block: rhs.block,
                })
            }
            HirExprKind::Cast { kind, expr: inner } => {
                let Some(cast) = cast_kind(&kind) else {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        range,
                        format!("cast `{kind}` is not lowered to IR yet"),
                    );
                    return None;
                };
                self.lower_cast_to_register(builder, site, inner, cast)
            }
            HirExprKind::Assign {
                operator,
                left,
                right,
            } => self.lower_assign_to_register(builder, site, &operator, left, right),
            HirExprKind::Ternary {
                condition,
                if_true,
                if_false,
            } => self.lower_ternary_to_register(builder, site, condition, if_true, if_false),
            HirExprKind::Match { subject, arms } => {
                self.lower_match_to_register(builder, site, subject, arms)
            }
            HirExprKind::Array { elements } => {
                self.lower_array_to_register(builder, site, elements)
            }
            HirExprKind::ArrayPair { .. } => {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    range,
                    "array pair expression cannot be lowered outside an array literal",
                );
                None
            }
            HirExprKind::Call { callee, args } => {
                self.lower_call_to_register(builder, site, callee, args)
            }
            HirExprKind::BuiltinCall { name, args } => {
                self.lower_builtin_call_to_register(builder, site, &name, args)
            }
            HirExprKind::Pipe { input, callable } => {
                self.lower_pipe_to_register(builder, site, input, callable)
            }
            HirExprKind::Include { kind, expr, .. } => {
                self.lower_include_to_register(builder, site, &kind, expr)
            }
            HirExprKind::Eval { expr, .. } => self.lower_eval_to_register(builder, site, expr),
            HirExprKind::FirstClassCallable { callee } => {
                self.lower_callable_expr_to_register(builder, site, callee)
            }
            HirExprKind::Closure { .. } => {
                self.lower_closure_to_register(builder, site, SignatureKind::Closure, None)
            }
            HirExprKind::ArrowFunction { expr: body } => {
                self.lower_closure_to_register(builder, site, SignatureKind::ArrowFunction, body)
            }
            HirExprKind::DimFetch { receiver, dim } => {
                self.lower_dim_fetch_to_register(builder, site, receiver, dim)
            }
            HirExprKind::New { class, args } => {
                self.lower_new_object_to_register(builder, site, class, args)
            }
            HirExprKind::Clone { expr: inner } => {
                self.lower_clone_object_to_register(builder, site, inner)
            }
            HirExprKind::CloneWith {
                expr: inner,
                replacements,
            } => self.lower_clone_with_to_register(builder, site, inner, replacements),
            HirExprKind::PropertyFetch {
                receiver,
                property,
                nullsafe,
            } => self.lower_property_fetch_to_register(builder, site, receiver, property, nullsafe),
            HirExprKind::MethodCall {
                receiver,
                method,
                args,
                nullsafe,
            } => {
                self.lower_method_call_to_register(builder, site, receiver, method, args, nullsafe)
            }
            HirExprKind::StaticAccess { .. } => self.lower_static_access_to_register(builder, site),
            HirExprKind::Yield { key, value } => {
                self.lower_yield_to_register(builder, site, key, value)
            }
            HirExprKind::YieldFrom { expr } => {
                self.lower_yield_from_to_register(builder, site, expr)
            }
            HirExprKind::Missing => {
                if let Some((function_name, property_local)) =
                    self.call_dynamic_property_target_from_source_range(range)
                {
                    let object = builder.alloc_register(function);
                    let call = builder.emit(
                        function,
                        block,
                        InstructionKind::CallFunction {
                            dst: object,
                            name: normalize_function_name(&function_name),
                            args: Vec::new(),
                        },
                        span,
                    );
                    self.add_expr_source_map(builder, function, block, call, expr, span);
                    let property = builder.alloc_register(function);
                    let local = builder.intern_local(function, property_local);
                    let load_property = builder.emit(
                        function,
                        block,
                        InstructionKind::LoadLocal {
                            dst: property,
                            local,
                        },
                        span,
                    );
                    self.add_expr_source_map(builder, function, block, load_property, expr, span);
                    let dst = builder.alloc_register(function);
                    let fetch = builder.emit(
                        function,
                        block,
                        InstructionKind::FetchDynamicProperty {
                            dst,
                            object: Operand::Register(object),
                            property: Operand::Register(property),
                        },
                        span,
                    );
                    self.add_expr_source_map(builder, function, block, fetch, expr, span);
                    return Some(LoweredExpr {
                        register: dst,
                        block,
                    });
                }
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    range,
                    "HIR expression `missing` is not lowered to IR yet",
                );
                None
            }
            kind => {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    range,
                    format!(
                        "HIR expression `{}` is not lowered to IR yet",
                        kind.as_str()
                    ),
                );
                None
            }
        }
    }

    pub(super) fn lower_yield_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        mut key: Option<ExprId>,
        mut value: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        if value.is_none() {
            value = key.take();
        }
        let mut current = site.block;
        let key = if let Some(key) = key {
            let lowered = self.lower_expr_to_register(builder, site.function, current, key)?;
            current = lowered.block;
            Some(Operand::Register(lowered.register))
        } else {
            None
        };
        let value = if let Some(value) = value {
            let lowered = self.lower_expr_to_register(builder, site.function, current, value)?;
            current = lowered.block;
            Some(Operand::Register(lowered.register))
        } else {
            None
        };
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            current,
            InstructionKind::Yield { dst, key, value },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn lower_yield_from_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        expr: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(expr) = expr else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                self.span_for(SourceMappedId::from(site.expr)),
                "yield from source expression is missing",
            );
            return None;
        };
        let source = self.lower_expr_to_register(builder, site.function, site.block, expr)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            source.block,
            InstructionKind::YieldFrom {
                dst,
                source: Operand::Register(source.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            source.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: source.block,
        })
    }

    pub(super) fn lower_new_object_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        class: Option<ExprId>,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        let Some(class) = class else {
            if let Some((class_name, display_class_name)) =
                self.anonymous_class_name_for_new(site.range)
            {
                let (operands, current) = self.lower_call_args(builder, site, &args)?;
                let dst = builder.alloc_register(site.function);
                let instruction = builder.emit(
                    site.function,
                    current,
                    InstructionKind::NewObject {
                        dst,
                        display_class_name,
                        class_name,
                        args: operands,
                    },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    instruction,
                    site.expr,
                    site.span,
                );
                return Some(LoweredExpr {
                    register: dst,
                    block: current,
                });
            }
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "new expression is missing its class operand",
            );
            return None;
        };
        let Some(class_name) = self.static_class_name(class) else {
            let class_name =
                self.lower_expr_to_register(builder, site.function, site.block, class)?;
            let dynamic_site = LowerSite {
                block: class_name.block,
                ..site
            };
            let (operands, current) = self.lower_call_args(builder, dynamic_site, &args)?;
            let dst = builder.alloc_register(site.function);
            let instruction = builder.emit(
                site.function,
                current,
                InstructionKind::DynamicNewObject {
                    dst,
                    class_name: Operand::Register(class_name.register),
                    args: operands,
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                instruction,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        };
        let source_display_class_name = self
            .new_object_display_class_name(site.function, class, &class_name)
            .unwrap_or_else(|| display_class_name(&class_name));
        let normalized_class_name = self
            .new_object_class_name(site.function, &class_name)
            .unwrap_or_else(|| normalize_class_name(&class_name));
        if is_internal_throwable_class(&normalized_class_name) {
            let message = args.first().map(|arg| arg.value);
            let (current, message) = if let Some(message) = message {
                let value =
                    self.lower_expr_to_register(builder, site.function, site.block, message)?;
                (value.block, Operand::Register(value.register))
            } else {
                (
                    site.block,
                    Operand::Constant(builder.intern_constant(IrConstant::String(String::new()))),
                )
            };
            let dst = builder.alloc_register(site.function);
            let instruction = builder.emit(
                site.function,
                current,
                InstructionKind::MakeException {
                    dst,
                    class_name: normalized_class_name,
                    message,
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                instruction,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        }
        let (operands, current) = self.lower_call_args(builder, site, &args)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            current,
            InstructionKind::NewObject {
                dst,
                display_class_name: source_display_class_name,
                class_name: normalized_class_name,
                args: operands,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn anonymous_class_name_for_new(
        &self,
        range: TextRange,
    ) -> Option<(String, String)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        module
            .class_likes()
            .iter()
            .filter(|(_, class_like)| class_like.kind() == ClassLikeKind::AnonymousClass)
            .find_map(|(class_like_id, class_like)| {
                let span = self.span_for(SourceMappedId::from(class_like_id));
                if !range_contains(range, span) {
                    return None;
                }
                let name = class_like_normalized_name(class_like, &self.options.source_path)?;
                Some((name.clone(), class_like_display_name(class_like, &name)))
            })
    }

    pub(super) fn lower_property_fetch_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        receiver: Option<ExprId>,
        property: Option<ExprId>,
        nullsafe: bool,
    ) -> Option<LoweredExpr> {
        let receiver = receiver?;
        let property = property?;
        let object = self.lower_expr_to_register(builder, site.function, site.block, receiver)?;
        if nullsafe {
            let dst = builder.alloc_register(site.function);
            let is_null = builder.alloc_register(site.function);
            let null_const = builder.intern_constant(IrConstant::Null);
            let null_block = builder.append_block(site.function);
            let value_block = builder.append_block(site.function);
            let after_block = builder.append_block(site.function);
            builder.emit(
                site.function,
                object.block,
                InstructionKind::Compare {
                    dst: is_null,
                    op: CompareOp::Identical,
                    lhs: Operand::Register(object.register),
                    rhs: Operand::Constant(null_const),
                },
                site.span,
            );
            builder.terminate_jump_if(
                site.function,
                object.block,
                Operand::Register(is_null),
                null_block,
                value_block,
                site.span,
            );
            builder.emit(
                site.function,
                null_block,
                InstructionKind::Move {
                    dst,
                    src: Operand::Constant(null_const),
                },
                site.span,
            );
            self.jump_if_open(builder, site.function, null_block, after_block, site.span);
            let value = if !self.property_fetch_uses_dynamic_member(site.expr)
                && let Some(property) = self.static_property_name(property)
            {
                let property_dst = builder.alloc_register(site.function);
                let instruction = builder.emit(
                    site.function,
                    value_block,
                    InstructionKind::FetchProperty {
                        dst: property_dst,
                        object: Operand::Register(object.register),
                        property,
                    },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    value_block,
                    instruction,
                    site.expr,
                    site.span,
                );
                LoweredExpr {
                    register: property_dst,
                    block: value_block,
                }
            } else {
                let property_value = self.lower_dynamic_member_name_to_register(
                    builder,
                    site,
                    value_block,
                    property,
                )?;
                let property_dst = builder.alloc_register(site.function);
                let instruction = builder.emit(
                    site.function,
                    property_value.block,
                    InstructionKind::FetchDynamicProperty {
                        dst: property_dst,
                        object: Operand::Register(object.register),
                        property: Operand::Register(property_value.register),
                    },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    property_value.block,
                    instruction,
                    site.expr,
                    site.span,
                );
                LoweredExpr {
                    register: property_dst,
                    block: property_value.block,
                }
            };
            builder.emit(
                site.function,
                value.block,
                InstructionKind::Move {
                    dst,
                    src: Operand::Register(value.register),
                },
                site.span,
            );
            self.jump_if_open(builder, site.function, value.block, after_block, site.span);
            return Some(LoweredExpr {
                register: dst,
                block: after_block,
            });
        }
        if !self.property_fetch_uses_dynamic_member(site.expr)
            && let Some(property) = self.static_property_name(property)
        {
            let dst = builder.alloc_register(site.function);
            let instruction = builder.emit(
                site.function,
                object.block,
                InstructionKind::FetchProperty {
                    dst,
                    object: Operand::Register(object.register),
                    property,
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                object.block,
                instruction,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: object.block,
            });
        }
        let property_value =
            self.lower_dynamic_member_name_to_register(builder, site, object.block, property)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            property_value.block,
            InstructionKind::FetchDynamicProperty {
                dst,
                object: Operand::Register(object.register),
                property: Operand::Register(property_value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            property_value.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: property_value.block,
        })
    }

    pub(super) fn lower_dynamic_member_name_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        block: BlockId,
        member: ExprId,
    ) -> Option<LoweredExpr> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(member)?;
        let variable_name = match expression.kind() {
            HirExprKind::Literal { text } if text.starts_with('$') => {
                Some(local_name(text).to_owned())
            }
            HirExprKind::Variable { name } if name.starts_with('$') => {
                Some(local_name(name).to_owned())
            }
            HirExprKind::Name { resolution } if resolution.source().starts_with('$') => {
                Some(local_name(resolution.source()).to_owned())
            }
            _ => self.dynamic_member_variable_name_from_source(member),
        };
        if let Some(variable_name) = variable_name {
            let local = builder.intern_local(site.function, variable_name);
            let dst = builder.alloc_register(site.function);
            let range = self.span_for(SourceMappedId::from(member));
            let span = span_from_range(self.file, range);
            let instruction = builder.emit(
                site.function,
                block,
                InstructionKind::LoadLocal { dst, local },
                span,
            );
            self.add_expr_source_map(builder, site.function, block, instruction, member, span);
            return Some(LoweredExpr {
                register: dst,
                block,
            });
        }
        self.lower_expr_to_register(builder, site.function, block, member)
    }

    pub(super) fn dynamic_member_variable_name_from_source(&self, expr: ExprId) -> Option<String> {
        let range = self.span_for(SourceMappedId::from(expr));
        let source = self.source_text.slice(range)?.trim();
        let marker = source
            .find("->$")
            .map(|index| (index, "->$".len()))
            .or_else(|| source.find("?->$").map(|index| (index, "?->$".len())))?;
        let rest = &source[marker.0 + marker.1..];
        let end = rest
            .bytes()
            .position(|byte| !(byte == b'_' || byte.is_ascii_alphanumeric()))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        (!name.is_empty()).then(|| name.to_owned())
    }

    pub(super) fn missing_call_dynamic_property_target_from_source(
        &self,
        expr: ExprId,
    ) -> Option<(String, String)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        if !matches!(
            expression.kind(),
            HirExprKind::Missing | HirExprKind::BuiltinCall { .. }
        ) {
            return None;
        }
        let range = self.span_for(SourceMappedId::from(expr));
        self.call_dynamic_property_target_from_source_range(range)
    }

    pub(super) fn call_dynamic_property_target_from_source_range(
        &self,
        range: TextRange,
    ) -> Option<(String, String)> {
        let mut source = self.source_text.slice(range)?.trim();
        if (source.starts_with("empty(") || source.starts_with("isset("))
            && source.ends_with(')')
            && let Some(open) = source.find('(')
        {
            source = source[open + 1..source.len() - 1].trim();
        }
        let marker = source
            .find("()->$")
            .map(|index| (index, "()->$".len()))
            .or_else(|| source.find("() ->$").map(|index| (index, "() ->$".len())))?;
        let function_name = source[..marker.0]
            .trim_end()
            .rsplit(|ch: char| !(ch == '_' || ch == '\\' || ch.is_ascii_alphanumeric()))
            .next()
            .unwrap_or("");
        if function_name.is_empty()
            || !function_name
                .bytes()
                .all(|byte| byte == b'_' || byte == b'\\' || byte.is_ascii_alphanumeric())
        {
            return None;
        }
        let rest = &source[marker.0 + marker.1..];
        let end = rest
            .bytes()
            .position(|byte| !(byte == b'_' || byte.is_ascii_alphanumeric()))
            .unwrap_or(rest.len());
        let property_local = &rest[..end];
        (!property_local.is_empty()).then(|| (function_name.to_owned(), property_local.to_owned()))
    }

    pub(super) fn lower_static_access_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
    ) -> Option<LoweredExpr> {
        if let Some(target) = self.static_property_target(site.expr) {
            return self.lower_static_property_fetch_to_register(builder, site, target);
        }
        if let Some(target) = self.dynamic_static_property_target(site.expr) {
            return self.lower_dynamic_static_property_fetch_to_register(builder, site, target);
        }
        if let Some(target) = self.class_constant_target(site.expr) {
            let normalized_class_name = normalize_class_name(&target.class_name);
            let normalized_display_class_name = target
                .display_class_name
                .as_deref()
                .map(normalize_class_name)
                .unwrap_or_else(|| normalized_class_name.clone());
            let relative_class_name =
                matches!(normalized_class_name.as_str(), "self" | "static" | "parent")
                    || matches!(
                        normalized_display_class_name.as_str(),
                        "self" | "static" | "parent"
                    );
            if target.constant.eq_ignore_ascii_case("class")
                && (normalized_class_name == "self" || normalized_display_class_name == "self")
                && let Some(class_name) = self.class_names.get(&site.function)
            {
                let dst = builder.alloc_register(site.function);
                let constant = builder.intern_constant(IrConstant::String(class_name.clone()));
                let instruction = builder.emit(
                    site.function,
                    site.block,
                    InstructionKind::LoadConst { dst, constant },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    site.block,
                    instruction,
                    site.expr,
                    site.span,
                );
                return Some(LoweredExpr {
                    register: dst,
                    block: site.block,
                });
            }
            if target.constant.eq_ignore_ascii_case("class") && !relative_class_name {
                let dst = builder.alloc_register(site.function);
                let class_name = self
                    .class_name_constant_value(target.target_expr)
                    .unwrap_or_else(|| target.display_class_name.unwrap_or(target.class_name));
                let constant = builder.intern_constant(IrConstant::String(class_name));
                let instruction = builder.emit(
                    site.function,
                    site.block,
                    InstructionKind::LoadConst { dst, constant },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    site.block,
                    instruction,
                    site.expr,
                    site.span,
                );
                return Some(LoweredExpr {
                    register: dst,
                    block: site.block,
                });
            }
            let dst = builder.alloc_register(site.function);
            let instruction = builder.emit(
                site.function,
                site.block,
                InstructionKind::FetchClassConstant {
                    dst,
                    class_name: class_constant_fetch_class_name(
                        target.class_name,
                        target.display_class_name,
                    ),
                    constant: target.constant,
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                site.block,
                instruction,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: site.block,
            });
        }
        if let Some(target) = self.object_class_name_target(site.expr) {
            let object =
                self.lower_expr_to_register(builder, site.function, site.block, target.object)?;
            let dst = builder.alloc_register(site.function);
            let instruction = builder.emit(
                site.function,
                object.block,
                InstructionKind::FetchObjectClassName {
                    dst,
                    object: Operand::Register(object.register),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                object.block,
                instruction,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: object.block,
            });
        }
        self.unsupported(
            UnsupportedFeature::StaticProperty,
            site.range,
            "static access target or member is not statically known",
        );
        None
    }

    pub(super) fn lower_static_property_fetch_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: StaticPropertyTarget,
    ) -> Option<LoweredExpr> {
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            site.block,
            InstructionKind::FetchStaticProperty {
                dst,
                class_name: target.class_name,
                property: target.property,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: site.block,
        })
    }

    pub(super) fn lower_dynamic_static_property_fetch_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DynamicStaticPropertyTarget,
    ) -> Option<LoweredExpr> {
        let class_name =
            self.lower_expr_to_register(builder, site.function, site.block, target.class_name)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            class_name.block,
            InstructionKind::FetchDynamicStaticProperty {
                dst,
                class_name: Operand::Register(class_name.register),
                property: target.property,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            class_name.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: class_name.block,
        })
    }

    pub(super) fn lower_dim_fetch_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        receiver: Option<ExprId>,
        dim: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let receiver = receiver?;
        let dim = dim?;
        let array = self.lower_expr_to_register(builder, site.function, site.block, receiver)?;
        let index = self.lower_expr_to_register(builder, site.function, array.block, dim)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            index.block,
            InstructionKind::FetchDim {
                dst,
                array: Operand::Register(array.register),
                key: Operand::Register(index.register),
                quiet: false,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            index.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: index.block,
        })
    }

    pub(super) fn lower_array_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        elements: Vec<ExprId>,
    ) -> Option<LoweredExpr> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let dst = builder.alloc_register(site.function);
        let new_array = builder.emit(
            site.function,
            site.block,
            InstructionKind::NewArray { dst },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            new_array,
            site.expr,
            site.span,
        );
        let mut current = site.block;

        for element in elements {
            let Some(expression) = module.expressions().get(element) else {
                continue;
            };
            let (key, value, unpack, by_ref) = match expression.kind() {
                HirExprKind::ArrayPair {
                    key,
                    value,
                    unpack,
                    by_ref,
                } => (*key, *value, *unpack, *by_ref),
                _ => (None, Some(element), false, false),
            };
            if unpack {
                let Some(value) = value else {
                    self.unsupported(
                        UnsupportedFeature::ArraySpread,
                        self.span_for(SourceMappedId::from(element)),
                        "array spread element is missing its value",
                    );
                    continue;
                };
                let source = self.lower_expr_to_register(builder, site.function, current, value)?;
                current = source.block;
                let instruction = builder.emit(
                    site.function,
                    current,
                    InstructionKind::ArraySpread {
                        array: dst,
                        source: Operand::Register(source.register),
                    },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    instruction,
                    element,
                    site.span,
                );
                continue;
            }
            let key = if let Some(key) = key {
                let key_value =
                    self.lower_expr_to_register(builder, site.function, current, key)?;
                current = key_value.block;
                Some(Operand::Register(key_value.register))
            } else {
                None
            };
            let Some(value) = value else {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    self.span_for(SourceMappedId::from(element)),
                    "array element is missing its value",
                );
                continue;
            };
            let (value, by_ref_local) = if by_ref {
                if let Some(local) = self.variable_local(builder, site.function, value) {
                    let value =
                        self.lower_expr_to_register(builder, site.function, current, value)?;
                    current = value.block;
                    (value, Some(local))
                } else if let Some(target) =
                    self.dim_assignment_target(builder, site.function, value)
                {
                    if target.append || target.dims.is_empty() {
                        self.unsupported(
                            UnsupportedFeature::ArrayElementReference,
                            self.span_for(SourceMappedId::from(element)),
                            "array literal by-reference dimension elements require an existing array element",
                        );
                        continue;
                    }
                    let mut dims = Vec::with_capacity(target.dims.len());
                    for dim in target.dims {
                        let dim_value =
                            self.lower_expr_to_register(builder, site.function, current, dim)?;
                        current = dim_value.block;
                        dims.push(Operand::Register(dim_value.register));
                    }
                    let local = builder.intern_local(
                        site.function,
                        format!("__phrust:array-ref-dim:{}", element.raw()),
                    );
                    let bind = builder.emit(
                        site.function,
                        current,
                        InstructionKind::BindReferenceFromDim {
                            target: local,
                            local: target.local,
                            dims,
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        bind,
                        element,
                        site.span,
                    );
                    let register = builder.alloc_register(site.function);
                    let load = builder.emit(
                        site.function,
                        current,
                        InstructionKind::LoadLocal {
                            dst: register,
                            local,
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        load,
                        element,
                        site.span,
                    );
                    (
                        LoweredExpr {
                            register,
                            block: current,
                        },
                        Some(local),
                    )
                } else if let Some(target) = self.property_assignment_target(value) {
                    let object = self.lower_expr_to_register(
                        builder,
                        site.function,
                        current,
                        target.receiver,
                    )?;
                    current = object.block;
                    let local = builder.intern_local(
                        site.function,
                        format!("__phrust:array-ref-property:{}", element.raw()),
                    );
                    let bind = builder.emit(
                        site.function,
                        current,
                        InstructionKind::BindReferenceFromProperty {
                            target: local,
                            object: Operand::Register(object.register),
                            property: target.property,
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        bind,
                        element,
                        site.span,
                    );
                    let register = builder.alloc_register(site.function);
                    let load = builder.emit(
                        site.function,
                        current,
                        InstructionKind::LoadLocal {
                            dst: register,
                            local,
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        load,
                        element,
                        site.span,
                    );
                    (
                        LoweredExpr {
                            register,
                            block: current,
                        },
                        Some(local),
                    )
                } else {
                    self.unsupported(
                        UnsupportedFeature::ArrayElementReference,
                        self.span_for(SourceMappedId::from(element)),
                        "array literal by-reference elements require a simple local variable or local array dimension",
                    );
                    continue;
                }
            } else {
                let value = self.lower_expr_to_register(builder, site.function, current, value)?;
                current = value.block;
                (value, None)
            };
            let instruction = builder.emit(
                site.function,
                current,
                InstructionKind::ArrayInsert {
                    array: dst,
                    key,
                    value: Operand::Register(value.register),
                    by_ref_local,
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                instruction,
                element,
                site.span,
            );
        }

        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn lower_call_args(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        args: &[HirCallArg],
    ) -> Option<(Vec<IrCallArg>, BlockId)> {
        self.lower_call_args_with_value_policy(builder, site, args, |_, _| false)
    }

    pub(super) fn lower_call_args_for_function(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        function: &str,
        args: &[HirCallArg],
    ) -> Option<(Vec<IrCallArg>, BlockId)> {
        self.lower_call_args_with_value_policy(builder, site, args, |index, arg| {
            is_quiet_by_ref_internal_builtin_arg(function, index, arg)
        })
    }

    pub(super) fn lower_call_args_with_value_policy(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        args: &[HirCallArg],
        mut use_null_placeholder: impl FnMut(usize, &HirCallArg) -> bool,
    ) -> Option<(Vec<IrCallArg>, BlockId)> {
        let mut current = site.block;
        let mut operands = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            let direct_by_ref_local = (!arg.unpack)
                .then(|| self.variable_local(builder, site.function, arg.value))
                .flatten();
            let dim_target = (!arg.unpack)
                .then(|| self.dim_assignment_target(builder, site.function, arg.value))
                .flatten()
                .filter(|target| !target.append && !target.dims.is_empty());
            let property_target = (!arg.unpack)
                .then(|| self.property_assignment_target(arg.value))
                .flatten();
            let property_dim_target = (!arg.unpack)
                .then(|| self.property_dim_target(arg.value))
                .flatten()
                .filter(|target| !target.append && !target.dims.is_empty());
            let static_property_target = (!arg.unpack)
                .then(|| self.static_property_target(arg.value))
                .flatten();
            let (value, by_ref_local, by_ref_dim, by_ref_property, by_ref_property_dim) =
                if direct_by_ref_local.is_some() && use_null_placeholder(index, arg) {
                    (
                        Operand::Constant(builder.intern_constant(IrConstant::Null)),
                        direct_by_ref_local,
                        None,
                        None,
                        None,
                    )
                } else if let Some(target) = dim_target {
                    let mut dims = Vec::with_capacity(target.dims.len());
                    for dim in &target.dims {
                        let dim_value =
                            self.lower_expr_to_register(builder, site.function, current, *dim)?;
                        current = dim_value.block;
                        dims.push(Operand::Register(dim_value.register));
                    }
                    let mut array = Operand::Local(target.local);
                    let mut last = None;
                    for dim in &dims {
                        let dst = builder.alloc_register(site.function);
                        let instruction = builder.emit(
                            site.function,
                            current,
                            InstructionKind::FetchDim {
                                dst,
                                array,
                                key: *dim,
                                quiet: false,
                            },
                            site.span,
                        );
                        self.add_expr_source_map(
                            builder,
                            site.function,
                            current,
                            instruction,
                            arg.value,
                            site.span,
                        );
                        array = Operand::Register(dst);
                        last = Some(dst);
                    }
                    (
                        Operand::Register(
                            last.expect("dimension target has at least one dimension"),
                        ),
                        None,
                        Some(IrCallDimTarget {
                            local: target.local,
                            dims,
                        }),
                        None,
                        None,
                    )
                } else if let Some(target) = property_dim_target {
                    let object = self.lower_expr_to_register(
                        builder,
                        site.function,
                        current,
                        target.receiver,
                    )?;
                    current = object.block;
                    let object_operand = Operand::Register(object.register);
                    let mut dims = Vec::with_capacity(target.dims.len());
                    for dim in &target.dims {
                        let dim_value =
                            self.lower_expr_to_register(builder, site.function, current, *dim)?;
                        current = dim_value.block;
                        dims.push(Operand::Register(dim_value.register));
                    }
                    let dst = builder.alloc_register(site.function);
                    let instruction = builder.emit(
                        site.function,
                        current,
                        InstructionKind::FetchProperty {
                            dst,
                            object: object_operand,
                            property: target.property.clone(),
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        instruction,
                        arg.value,
                        site.span,
                    );
                    let mut array = Operand::Register(dst);
                    let mut last = None;
                    for dim in &dims {
                        let dst = builder.alloc_register(site.function);
                        let instruction = builder.emit(
                            site.function,
                            current,
                            InstructionKind::FetchDim {
                                dst,
                                array,
                                key: *dim,
                                quiet: false,
                            },
                            site.span,
                        );
                        self.add_expr_source_map(
                            builder,
                            site.function,
                            current,
                            instruction,
                            arg.value,
                            site.span,
                        );
                        array = Operand::Register(dst);
                        last = Some(dst);
                    }
                    (
                        Operand::Register(
                            last.expect("property dimension target has at least one dimension"),
                        ),
                        None,
                        None,
                        None,
                        Some(IrCallPropertyDimTarget {
                            object: object_operand,
                            property: target.property,
                            dims,
                        }),
                    )
                } else if let Some(target) = property_target {
                    let object = self.lower_expr_to_register(
                        builder,
                        site.function,
                        current,
                        target.receiver,
                    )?;
                    current = object.block;
                    let dst = builder.alloc_register(site.function);
                    let instruction = builder.emit(
                        site.function,
                        current,
                        InstructionKind::FetchProperty {
                            dst,
                            object: Operand::Register(object.register),
                            property: target.property.clone(),
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        instruction,
                        arg.value,
                        site.span,
                    );
                    (
                        Operand::Register(dst),
                        None,
                        None,
                        Some(IrCallPropertyTarget {
                            object: Operand::Register(object.register),
                            property: target.property,
                        }),
                        None,
                    )
                } else if let Some(target) = static_property_target {
                    let local = builder.intern_local(
                        site.function,
                        format!("__phrust:by-ref-static-property:{}", arg.value.raw()),
                    );
                    let bind = builder.emit(
                        site.function,
                        current,
                        InstructionKind::BindReferenceFromStaticPropertyDim {
                            target: local,
                            class_name: target.class_name,
                            property: target.property,
                            dims: Vec::new(),
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        bind,
                        arg.value,
                        site.span,
                    );
                    let dst = builder.alloc_register(site.function);
                    let load = builder.emit(
                        site.function,
                        current,
                        InstructionKind::LoadLocal { dst, local },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        load,
                        arg.value,
                        site.span,
                    );
                    (Operand::Register(dst), Some(local), None, None, None)
                } else {
                    let value =
                        self.lower_expr_to_register(builder, site.function, current, arg.value)?;
                    current = value.block;
                    (
                        Operand::Register(value.register),
                        direct_by_ref_local,
                        None,
                        None,
                        None,
                    )
                };
            operands.push(IrCallArg {
                name: arg.name.clone(),
                value,
                unpack: arg.unpack,
                value_kind: self.call_arg_value_kind(arg.value),
                by_ref_local,
                by_ref_dim,
                by_ref_property,
                by_ref_property_dim,
            });
        }
        Some((operands, current))
    }

    pub(super) fn call_arg_value_kind(&self, expr: ExprId) -> IrCallArgValueKind {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return IrCallArgValueKind::Direct;
        };
        let Some(expression) = module.expressions().get(expr) else {
            return IrCallArgValueKind::Direct;
        };
        match expression.kind() {
            HirExprKind::Call { .. }
            | HirExprKind::MethodCall { .. }
            | HirExprKind::New { .. }
            | HirExprKind::Clone { .. }
            | HirExprKind::Include { .. }
            | HirExprKind::Eval { .. } => IrCallArgValueKind::IndirectTemporary,
            _ => IrCallArgValueKind::Direct,
        }
    }

    pub(super) fn lower_call_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        callee: Option<ExprId>,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        if self.is_reflection_function_name(callee) {
            self.unsupported(
                UnsupportedFeature::Reflection,
                site.range,
                "reflection functions are not executable in the known-gap known-gap layer",
            );
            return None;
        }
        if let Some(callee) = callee
            && self.is_static_access_expr(callee)
        {
            if self.static_access_uses_dynamic_member(callee) {
                return self
                    .lower_dynamic_static_method_call_to_register(builder, site, callee, args);
            }
            if let Some(target) = self.static_method_call_target(callee) {
                return self.lower_static_method_call_to_register(builder, site, target, args);
            }
            return self.lower_dynamic_static_method_call_to_register(builder, site, callee, args);
        }
        let dst = builder.alloc_register(site.function);
        let (kind, current) =
            if let Some(name) = callee.and_then(|callee| self.static_function_call_name(callee)) {
                let normalized_name = normalize_function_name(&name);
                if let Some(lowered) = self.lower_static_property_first_arg_by_ref_call(
                    builder,
                    site,
                    &normalized_name,
                    &args,
                    dst,
                ) {
                    return Some(lowered);
                }
                let (operands, current) =
                    self.lower_call_args_for_function(builder, site, &normalized_name, &args)?;
                (
                    InstructionKind::CallFunction {
                        dst,
                        name: normalized_name,
                        args: operands,
                    },
                    current,
                )
            } else if let Some(callee) = callee {
                let (operands, mut current) = self.lower_call_args(builder, site, &args)?;
                let callee_value =
                    self.lower_expr_to_register(builder, site.function, current, callee)?;
                current = callee_value.block;
                (
                    InstructionKind::CallCallable {
                        dst,
                        callee: Operand::Register(callee_value.register),
                        args: operands,
                    },
                    current,
                )
            } else {
                self.unsupported(
                    UnsupportedFeature::DynamicFunctionCall,
                    site.range,
                    "call expression is missing a callable target",
                );
                return None;
            };
        let instruction = builder.emit(site.function, current, kind, site.span);
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn lower_static_property_first_arg_by_ref_call(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        normalized_name: &str,
        args: &[HirCallArg],
        dst: RegId,
    ) -> Option<LoweredExpr> {
        if normalized_function_basename(normalized_name) != "array_unshift" {
            return None;
        }
        let (first, rest) = args.split_first()?;
        if first.unpack {
            return None;
        }
        let target = self.static_property_target(first.value)?;
        let mut current = site.block;
        let property_value = builder.alloc_register(site.function);
        let fetch = builder.emit(
            site.function,
            current,
            InstructionKind::FetchStaticProperty {
                dst: property_value,
                class_name: target.class_name.clone(),
                property: target.property.clone(),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            fetch,
            first.value,
            site.span,
        );

        let local = builder.intern_local(
            site.function,
            format!(
                "__phrust:{normalized_name}-static-property:{}",
                first.value.raw()
            ),
        );
        builder.emit(
            site.function,
            current,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(property_value),
            },
            site.span,
        );

        let rest_site = LowerSite {
            block: current,
            ..site
        };
        let (rest_args, rest_current) =
            self.lower_call_args_for_function(builder, rest_site, normalized_name, rest)?;
        current = rest_current;
        let mut operands = Vec::with_capacity(args.len());
        operands.push(IrCallArg {
            name: first.name.clone(),
            value: Operand::Local(local),
            unpack: false,
            value_kind: IrCallArgValueKind::Direct,
            by_ref_local: Some(local),
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        });
        operands.extend(rest_args);

        let call = builder.emit(
            site.function,
            current,
            InstructionKind::CallFunction {
                dst,
                name: normalized_name.to_owned(),
                args: operands,
            },
            site.span,
        );
        self.add_expr_source_map(builder, site.function, current, call, site.expr, site.span);
        let writeback = builder.alloc_register(site.function);
        let assign = builder.emit(
            site.function,
            current,
            InstructionKind::AssignStaticProperty {
                dst: writeback,
                class_name: target.class_name,
                property: target.property,
                value: Operand::Local(local),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            assign,
            first.value,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn lower_method_call_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        receiver: Option<ExprId>,
        method: Option<ExprId>,
        args: Vec<HirCallArg>,
        nullsafe: bool,
    ) -> Option<LoweredExpr> {
        if self.method_call_uses_dynamic_member(site.expr) {
            return self.lower_dynamic_method_call_to_register(
                builder, site, receiver, method, args, nullsafe,
            );
        }
        let Some(target) = self.method_call_target(receiver, method) else {
            self.unsupported(
                UnsupportedFeature::MethodCall,
                site.range,
                "method call target is dynamic or missing in the method-runtime object MVP",
            );
            return None;
        };
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        if nullsafe {
            let dst = builder.alloc_register(site.function);
            let is_null = builder.alloc_register(site.function);
            let null_const = builder.intern_constant(IrConstant::Null);
            let null_block = builder.append_block(site.function);
            let call_block = builder.append_block(site.function);
            let after_block = builder.append_block(site.function);
            builder.emit(
                site.function,
                object.block,
                InstructionKind::Compare {
                    dst: is_null,
                    op: CompareOp::Identical,
                    lhs: Operand::Register(object.register),
                    rhs: Operand::Constant(null_const),
                },
                site.span,
            );
            builder.terminate_jump_if(
                site.function,
                object.block,
                Operand::Register(is_null),
                null_block,
                call_block,
                site.span,
            );
            builder.emit(
                site.function,
                null_block,
                InstructionKind::Move {
                    dst,
                    src: Operand::Constant(null_const),
                },
                site.span,
            );
            self.jump_if_open(builder, site.function, null_block, after_block, site.span);
            let call_site = LowerSite {
                block: call_block,
                ..site
            };
            let (operands, current) = self.lower_call_args(builder, call_site, &args)?;
            let call_result = builder.alloc_register(site.function);
            let instruction = builder.emit(
                site.function,
                current,
                InstructionKind::CallMethod {
                    dst: call_result,
                    object: Operand::Register(object.register),
                    method: normalize_method_name(&target.method),
                    args: operands,
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                instruction,
                site.expr,
                site.span,
            );
            builder.emit(
                site.function,
                current,
                InstructionKind::Move {
                    dst,
                    src: Operand::Register(call_result),
                },
                site.span,
            );
            self.jump_if_open(builder, site.function, current, after_block, site.span);
            return Some(LoweredExpr {
                register: dst,
                block: after_block,
            });
        }
        let site = LowerSite {
            block: object.block,
            ..site
        };
        let (operands, current) = self.lower_call_args(builder, site, &args)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            current,
            InstructionKind::CallMethod {
                dst,
                object: Operand::Register(object.register),
                method: normalize_method_name(&target.method),
                args: operands,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn lower_dynamic_method_call_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        receiver: Option<ExprId>,
        method: Option<ExprId>,
        args: Vec<HirCallArg>,
        nullsafe: bool,
    ) -> Option<LoweredExpr> {
        let target = self.dynamic_method_call_target(receiver, method)?;
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        if nullsafe {
            let dst = builder.alloc_register(site.function);
            let is_null = builder.alloc_register(site.function);
            let null_const = builder.intern_constant(IrConstant::Null);
            let null_block = builder.append_block(site.function);
            let call_block = builder.append_block(site.function);
            let after_block = builder.append_block(site.function);
            builder.emit(
                site.function,
                object.block,
                InstructionKind::Compare {
                    dst: is_null,
                    op: CompareOp::Identical,
                    lhs: Operand::Register(object.register),
                    rhs: Operand::Constant(null_const),
                },
                site.span,
            );
            builder.terminate_jump_if(
                site.function,
                object.block,
                Operand::Register(is_null),
                null_block,
                call_block,
                site.span,
            );
            builder.emit(
                site.function,
                null_block,
                InstructionKind::Move {
                    dst,
                    src: Operand::Constant(null_const),
                },
                site.span,
            );
            self.jump_if_open(builder, site.function, null_block, after_block, site.span);
            let method_value = self.lower_dynamic_member_name_to_register(
                builder,
                site,
                call_block,
                target.method,
            )?;
            let call = self.lower_callable_pair_call_to_register(
                builder,
                LowerSite {
                    block: method_value.block,
                    ..site
                },
                Operand::Register(object.register),
                Operand::Register(method_value.register),
                args,
            )?;
            builder.emit(
                site.function,
                call.block,
                InstructionKind::Move {
                    dst,
                    src: Operand::Register(call.register),
                },
                site.span,
            );
            self.jump_if_open(builder, site.function, call.block, after_block, site.span);
            return Some(LoweredExpr {
                register: dst,
                block: after_block,
            });
        }
        let method_value =
            self.lower_dynamic_member_name_to_register(builder, site, object.block, target.method)?;
        self.lower_callable_pair_call_to_register(
            builder,
            LowerSite {
                block: method_value.block,
                ..site
            },
            Operand::Register(object.register),
            Operand::Register(method_value.register),
            args,
        )
    }

    pub(super) fn lower_static_method_call_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: StaticMethodCallTarget,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        let (operands, current) = self.lower_call_args(builder, site, &args)?;
        let dst = builder.alloc_register(site.function);
        let class_name = if matches!(
            normalize_class_name(&target.class_name).as_str(),
            "self" | "static" | "parent"
        ) {
            target.class_name
        } else {
            target.display_class_name.unwrap_or(target.class_name)
        };
        let instruction = builder.emit(
            site.function,
            current,
            InstructionKind::CallStaticMethod {
                dst,
                class_name,
                method: normalize_method_name(&target.method),
                args: operands,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn lower_dynamic_static_method_call_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        callee: ExprId,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(callee)?;
        let HirExprKind::StaticAccess { target, member } = expression.kind() else {
            return None;
        };
        let target = (*target)?;
        let method = (*member)?;
        let (class_operand, block) = if let Some(class_name) = self.static_class_name(target) {
            (
                Operand::Constant(builder.intern_constant(IrConstant::String(class_name))),
                site.block,
            )
        } else {
            let class_value =
                self.lower_expr_to_register(builder, site.function, site.block, target)?;
            (Operand::Register(class_value.register), class_value.block)
        };
        let dynamic_member = self.static_access_uses_dynamic_member(callee);
        let (method_operand, block) =
            if !dynamic_member && let Some(method_name) = self.static_property_name(method) {
                (
                    Operand::Constant(builder.intern_constant(IrConstant::String(method_name))),
                    block,
                )
            } else {
                let method_value =
                    self.lower_expr_to_register(builder, site.function, block, method)?;
                (Operand::Register(method_value.register), method_value.block)
            };
        self.lower_callable_pair_call_to_register(
            builder,
            LowerSite { block, ..site },
            class_operand,
            method_operand,
            args,
        )
    }

    pub(super) fn lower_callable_pair_call_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: Operand,
        method: Operand,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        let callable = builder.alloc_register(site.function);
        builder.emit(
            site.function,
            site.block,
            InstructionKind::NewArray { dst: callable },
            site.span,
        );
        builder.emit(
            site.function,
            site.block,
            InstructionKind::ArrayInsert {
                array: callable,
                key: None,
                value: target,
                by_ref_local: None,
            },
            site.span,
        );
        builder.emit(
            site.function,
            site.block,
            InstructionKind::ArrayInsert {
                array: callable,
                key: None,
                value: method,
                by_ref_local: None,
            },
            site.span,
        );
        let (operands, current) = self.lower_call_args(builder, site, &args)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            current,
            InstructionKind::CallCallable {
                dst,
                callee: Operand::Register(callable),
                args: operands,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn new_object_class_name(
        &self,
        function: FunctionId,
        class_name: &str,
    ) -> Option<String> {
        let normalized = normalize_class_name(class_name);
        if normalized == "self" {
            return self
                .class_names
                .get(&function)
                .map(|name| normalize_class_name(name));
        }
        Some(normalized)
    }

    pub(super) fn new_object_display_class_name(
        &self,
        function: FunctionId,
        class: ExprId,
        class_name: &str,
    ) -> Option<String> {
        let normalized = normalize_class_name(class_name);
        if normalized == "self" {
            return self.class_names.get(&function).cloned();
        }
        self.static_class_display_name(class)
    }

    pub(super) fn lower_clone_object_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        object: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(object) = object else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "clone expression is missing its object operand",
            );
            return None;
        };
        let object = self.lower_expr_to_register(builder, site.function, site.block, object)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            object.block,
            InstructionKind::CloneObject {
                dst,
                object: Operand::Register(object.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            object.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: object.block,
        })
    }

    pub(super) fn lower_clone_with_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        expr: Option<ExprId>,
        replacements: Vec<ExprId>,
    ) -> Option<LoweredExpr> {
        if let Some(object) = self.parenthesized_clone_operand(expr, replacements.as_slice()) {
            return self.lower_clone_object_to_register(builder, site, Some(object));
        }
        let Some((object_expr, replacements_expr)) =
            self.clone_with_operands(expr, replacements.as_slice())
        else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "clone-with requires an object expression and replacement array in the reflection-clone MVP",
            );
            return None;
        };
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, object_expr)?;
        let replacements =
            self.lower_expr_to_register(builder, site.function, object.block, replacements_expr)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            replacements.block,
            InstructionKind::CloneWith {
                dst,
                object: Operand::Register(object.register),
                replacements: Operand::Register(replacements.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            replacements.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: replacements.block,
        })
    }

    pub(super) fn lower_builtin_call_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        name: &str,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        if matches!(name, "isset" | "empty") {
            return self.lower_isset_empty_to_register(builder, site, name, args);
        }
        let (operands, current) = self.lower_call_args(builder, site, &args)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            current,
            InstructionKind::CallFunction {
                dst,
                name: normalize_function_name(name),
                args: operands,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn lower_isset_empty_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        name: &str,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        if args.is_empty() {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!("{name} requires at least one operand"),
            );
            return None;
        }
        if name != "isset" && args.len() != 1 {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!("{name} supports exactly one operand"),
            );
            return None;
        }
        if name == "isset" && args.len() > 1 {
            return self.lower_multi_isset_to_register(builder, site, args);
        }
        self.lower_isset_empty_operand_to_register(builder, site, name, args[0].value)
    }

    pub(super) fn lower_multi_isset_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        let dst = builder.alloc_register(site.function);
        let mut current = site.block;
        let mut false_blocks = Vec::new();
        let last = args.len().saturating_sub(1);

        for (index, arg) in args.into_iter().enumerate() {
            let value = self.lower_isset_empty_operand_to_register(
                builder,
                LowerSite {
                    function: site.function,
                    block: current,
                    expr: site.expr,
                    range: site.range,
                    span: site.span,
                },
                "isset",
                arg.value,
            )?;
            if index == last {
                self.emit_bool_cast(
                    builder,
                    site.function,
                    value.block,
                    dst,
                    value.register,
                    site.span,
                );
                let after = builder.append_block(site.function);
                self.jump_if_open(builder, site.function, value.block, after, site.span);
                for false_block in false_blocks {
                    self.jump_if_open(builder, site.function, false_block, after, site.span);
                }
                return Some(LoweredExpr {
                    register: dst,
                    block: after,
                });
            }

            let false_block = builder.append_block(site.function);
            let true_block = builder.append_block(site.function);
            builder.terminate_jump_if(
                site.function,
                value.block,
                Operand::Register(value.register),
                true_block,
                false_block,
                site.span,
            );
            self.emit_bool_move(builder, site.function, false_block, dst, false, site.span);
            false_blocks.push(false_block);
            current = true_block;
        }

        None
    }

    pub(super) fn lower_isset_empty_operand_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        name: &str,
        arg: ExprId,
    ) -> Option<LoweredExpr> {
        let dst = builder.alloc_register(site.function);
        if let Some((function_name, property_local)) = self
            .missing_call_dynamic_property_target_from_source(arg)
            .or_else(|| self.missing_call_dynamic_property_target_from_source(site.expr))
            .or_else(|| self.call_dynamic_property_target_from_source_range(site.range))
        {
            let object = builder.alloc_register(site.function);
            let call = builder.emit(
                site.function,
                site.block,
                InstructionKind::CallFunction {
                    dst: object,
                    name: normalize_function_name(&function_name),
                    args: Vec::new(),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                site.block,
                call,
                site.expr,
                site.span,
            );
            let property = builder.alloc_register(site.function);
            let local = builder.intern_local(site.function, property_local);
            let load_property = builder.emit(
                site.function,
                site.block,
                InstructionKind::LoadLocal {
                    dst: property,
                    local,
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                site.block,
                load_property,
                site.expr,
                site.span,
            );
            let instruction = if name == "isset" {
                InstructionKind::IssetDynamicProperty {
                    dst,
                    object: Operand::Register(object),
                    property: Operand::Register(property),
                }
            } else {
                InstructionKind::EmptyDynamicProperty {
                    dst,
                    object: Operand::Register(object),
                    property: Operand::Register(property),
                }
            };
            let emitted = builder.emit(site.function, site.block, instruction, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                site.block,
                emitted,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: site.block,
            });
        }
        let kind = if let Some(local) = self.variable_local(builder, site.function, arg) {
            if name == "isset" {
                InstructionKind::IssetLocal { dst, local }
            } else {
                InstructionKind::EmptyLocal { dst, local }
            }
        } else if let Some(target) = self.dim_assignment_target(builder, site.function, arg) {
            if target.append || target.dims.is_empty() {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    self.span_for(SourceMappedId::from(arg)),
                    format!("{name} append dimensions are outside the runtime MVP"),
                );
                return None;
            }
            let mut current = site.block;
            let mut dims = Vec::with_capacity(target.dims.len());
            for dim in target.dims {
                let dim_value =
                    self.lower_expr_to_register(builder, site.function, current, dim)?;
                current = dim_value.block;
                dims.push(Operand::Register(dim_value.register));
            }
            let instruction = if name == "isset" {
                InstructionKind::IssetDim {
                    dst,
                    local: target.local,
                    dims,
                }
            } else {
                InstructionKind::EmptyDim {
                    dst,
                    local: target.local,
                    dims,
                }
            };
            let emitted = builder.emit(site.function, current, instruction, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                emitted,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        } else if let Some(target) = self.property_dim_target(arg) {
            if target.append || target.dims.is_empty() {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    self.span_for(SourceMappedId::from(arg)),
                    format!("{name} append property dimensions are outside the runtime MVP"),
                );
                return None;
            }
            let object =
                self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
            let mut current = object.block;
            let mut dims = Vec::with_capacity(target.dims.len());
            for dim in target.dims {
                let dim_value =
                    self.lower_expr_to_register(builder, site.function, current, dim)?;
                current = dim_value.block;
                dims.push(Operand::Register(dim_value.register));
            }
            let instruction = if name == "isset" {
                InstructionKind::IssetPropertyDim {
                    dst,
                    object: Operand::Register(object.register),
                    property: target.property,
                    dims,
                }
            } else {
                InstructionKind::EmptyPropertyDim {
                    dst,
                    object: Operand::Register(object.register),
                    property: target.property,
                    dims,
                }
            };
            let emitted = builder.emit(site.function, current, instruction, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                emitted,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        } else if let Some(target) = self.dynamic_property_dim_target(arg) {
            if target.append || target.dims.is_empty() {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    self.span_for(SourceMappedId::from(arg)),
                    format!(
                        "{name} append dynamic-property dimensions are outside the runtime MVP"
                    ),
                );
                return None;
            }
            let object =
                self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
            let property = self.lower_dynamic_member_name_to_register(
                builder,
                site,
                object.block,
                target.property,
            )?;
            let mut current = property.block;
            let mut dims = Vec::with_capacity(target.dims.len());
            for dim in target.dims {
                let dim_value =
                    self.lower_expr_to_register(builder, site.function, current, dim)?;
                current = dim_value.block;
                dims.push(Operand::Register(dim_value.register));
            }
            let instruction = if name == "isset" {
                InstructionKind::IssetDynamicPropertyDim {
                    dst,
                    object: Operand::Register(object.register),
                    property: Operand::Register(property.register),
                    dims,
                }
            } else {
                InstructionKind::EmptyDynamicPropertyDim {
                    dst,
                    object: Operand::Register(object.register),
                    property: Operand::Register(property.register),
                    dims,
                }
            };
            let emitted = builder.emit(site.function, current, instruction, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                emitted,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        } else if let Some(target) = self.property_assignment_target(arg) {
            let object =
                self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
            let instruction = if name == "isset" {
                InstructionKind::IssetProperty {
                    dst,
                    object: Operand::Register(object.register),
                    property: target.property,
                }
            } else {
                InstructionKind::EmptyProperty {
                    dst,
                    object: Operand::Register(object.register),
                    property: target.property,
                }
            };
            let emitted = builder.emit(site.function, object.block, instruction, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                object.block,
                emitted,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: object.block,
            });
        } else if let Some(target) = self.dynamic_property_target(arg) {
            let object =
                self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
            let property = self.lower_dynamic_member_name_to_register(
                builder,
                site,
                object.block,
                target.property,
            )?;
            let instruction = if name == "isset" {
                InstructionKind::IssetDynamicProperty {
                    dst,
                    object: Operand::Register(object.register),
                    property: Operand::Register(property.register),
                }
            } else {
                InstructionKind::EmptyDynamicProperty {
                    dst,
                    object: Operand::Register(object.register),
                    property: Operand::Register(property.register),
                }
            };
            let emitted = builder.emit(site.function, property.block, instruction, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                property.block,
                emitted,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: property.block,
            });
        } else if let Some(target) = self.static_property_dim_target(arg) {
            if target.append || target.dims.is_empty() {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    self.span_for(SourceMappedId::from(arg)),
                    format!("{name} append static-property dimensions are outside the runtime MVP"),
                );
                return None;
            }
            let mut current = site.block;
            let mut dims = Vec::with_capacity(target.dims.len());
            for dim in target.dims {
                let dim_value =
                    self.lower_expr_to_register(builder, site.function, current, dim)?;
                current = dim_value.block;
                dims.push(Operand::Register(dim_value.register));
            }
            let instruction = if name == "isset" {
                InstructionKind::IssetStaticPropertyDim {
                    dst,
                    class_name: target.class_name,
                    property: target.property,
                    dims,
                }
            } else {
                InstructionKind::EmptyStaticPropertyDim {
                    dst,
                    class_name: target.class_name,
                    property: target.property,
                    dims,
                }
            };
            let emitted = builder.emit(site.function, current, instruction, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                emitted,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        } else if let Some(target) = self.class_constant_dim_target(arg) {
            if target.append || target.dims.is_empty() {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    self.span_for(SourceMappedId::from(arg)),
                    format!("{name} append class-constant dimensions are outside the runtime MVP"),
                );
                return None;
            }
            let mut current = site.block;
            let mut dims = Vec::with_capacity(target.dims.len());
            for dim in target.dims {
                let dim_value =
                    self.lower_expr_to_register(builder, site.function, current, dim)?;
                current = dim_value.block;
                dims.push(Operand::Register(dim_value.register));
            }
            let local = builder.intern_local(
                site.function,
                format!("__phrust:{name}-class-constant-dim:{}", arg.raw()),
            );
            let value = builder.alloc_register(site.function);
            builder.emit(
                site.function,
                current,
                InstructionKind::FetchClassConstant {
                    dst: value,
                    class_name: class_constant_fetch_class_name(
                        target.class_name,
                        target.display_class_name,
                    ),
                    constant: target.constant,
                },
                site.span,
            );
            builder.emit(
                site.function,
                current,
                InstructionKind::StoreLocal {
                    local,
                    src: Operand::Register(value),
                },
                site.span,
            );
            let instruction = if name == "isset" {
                InstructionKind::IssetDim { dst, local, dims }
            } else {
                InstructionKind::EmptyDim { dst, local, dims }
            };
            let emitted = builder.emit(site.function, current, instruction, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                emitted,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        } else if let Some(target) = self.static_property_test_target(arg) {
            if name == "isset" {
                InstructionKind::IssetStaticProperty {
                    dst,
                    class_name: target.class_name,
                    property: target.property,
                }
            } else {
                InstructionKind::EmptyStaticProperty {
                    dst,
                    class_name: target.class_name,
                    property: target.property,
                }
            }
        } else if name == "empty" {
            let value = self.lower_expr_to_register(builder, site.function, site.block, arg)?;
            let instruction = builder.emit(
                site.function,
                value.block,
                InstructionKind::Unary {
                    dst,
                    op: UnaryOp::Not,
                    src: Operand::Register(value.register),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                value.block,
                instruction,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: value.block,
            });
        } else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!(
                    "{name} only supports locals, properties, static properties, and local array dimensions in runtime-semantics"
                ),
            );
            return None;
        };
        let instruction = builder.emit(site.function, site.block, kind, site.span);
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: site.block,
        })
    }

    pub(super) fn lower_pipe_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        input: Option<ExprId>,
        callable: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let input = input?;
        let callable = callable?;
        let input_value = self.lower_expr_to_register(builder, site.function, site.block, input)?;
        let callable_value = self.lower_pipe_callable_to_register(
            builder,
            LowerSite {
                function: site.function,
                block: input_value.block,
                expr: callable,
                range: site.range,
                span: site.span,
            },
            callable,
        )?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            callable_value.block,
            InstructionKind::Pipe {
                dst,
                input: Operand::Register(input_value.register),
                callable: Operand::Register(callable_value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            callable_value.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: callable_value.block,
        })
    }

    pub(super) fn lower_include_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        kind: &str,
        expr: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(path_expr) = expr else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "include/require expression is missing its path operand",
            );
            return None;
        };
        let Some(kind) = include_kind(kind) else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!("include-like construct `{kind}` is not recognized"),
            );
            return None;
        };
        let path = self.lower_expr_to_register(builder, site.function, site.block, path_expr)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            path.block,
            InstructionKind::Include {
                dst,
                kind,
                path: Operand::Register(path.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            path.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: path.block,
        })
    }

    pub(super) fn lower_eval_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        expr: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(code_expr) = expr else {
            self.unsupported(
                UnsupportedFeature::Eval,
                site.range,
                "eval expression is missing its code operand",
            );
            return None;
        };
        let code = self.lower_expr_to_register(builder, site.function, site.block, code_expr)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            code.block,
            InstructionKind::Eval {
                dst,
                code: Operand::Register(code.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            code.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: code.block,
        })
    }

    pub(super) fn lower_pipe_callable_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        callable: ExprId,
    ) -> Option<LoweredExpr> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(callable)?;
        match expression.kind() {
            HirExprKind::FirstClassCallable { callee } => {
                self.lower_callable_expr_to_register(builder, site, *callee)
            }
            _ => self.lower_expr_to_register(builder, site.function, site.block, callable),
        }
    }

    pub(super) fn lower_callable_expr_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        callee: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let callable = if let Some(name) =
            callee.and_then(|callee| self.static_function_call_name(callee))
        {
            CallableKind::FunctionName {
                name: normalize_function_name(&name),
            }
        } else {
            // A method or static-method first-class callable (`$obj->m(...)`,
            // `Cls::m(...)`) lowers to the equivalent `[receiver, 'm']` array
            // callable, which the runtime already dispatches.
            if let Some(callee) = callee
                && let Some(lowered) = self.lower_method_first_class_callable(builder, site, callee)
            {
                return Some(self.lower_acquire_callable_value(builder, site, lowered));
            }
            if let Some(callee) = callee
                && self.first_class_callable_runtime_value(callee)
            {
                let lowered =
                    self.lower_expr_to_register(builder, site.function, site.block, callee)?;
                return Some(self.lower_acquire_callable_value(builder, site, lowered));
            }
            CallableKind::UnresolvedDynamic {
                target: "first-class callable target is not a simple function name".to_owned(),
            }
        };
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            site.block,
            InstructionKind::ResolveCallable { dst, callable },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: site.block,
        })
    }

    pub(super) fn lower_acquire_callable_value(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        value: LoweredExpr,
    ) -> LoweredExpr {
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            value.block,
            InstructionKind::AcquireCallable {
                dst,
                value: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            instruction,
            site.expr,
            site.span,
        );
        LoweredExpr {
            register: dst,
            block: value.block,
        }
    }

    /// Lowers a method or static-method first-class callable (`$obj->m(...)`,
    /// `Cls::m(...)`) to a `[receiver, 'm']` array callable value.
    pub(super) fn lower_method_first_class_callable(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        callee: ExprId,
    ) -> Option<LoweredExpr> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expr = module.expressions().get(callee)?;
        let (receiver, method) = match expr.kind() {
            HirExprKind::MethodCall {
                receiver, method, ..
            } => {
                let target = self.method_call_target(*receiver, *method)?;
                (
                    CallableComponent::Expr(target.receiver),
                    CallableComponent::String(target.method),
                )
            }
            HirExprKind::PropertyFetch {
                receiver: Some(receiver),
                property: Some(property),
                nullsafe: false,
            } => (
                CallableComponent::Expr(*receiver),
                self.callable_member_component(*property)?,
            ),
            HirExprKind::StaticAccess { .. } => {
                let HirExprKind::StaticAccess { target, member } = expr.kind() else {
                    return None;
                };
                let target = self.callable_static_target_component((*target)?)?;
                let method = self.callable_member_component((*member)?)?;
                (target, method)
            }
            _ => return None,
        };
        let dst = builder.alloc_register(site.function);
        let new_array = builder.emit(
            site.function,
            site.block,
            InstructionKind::NewArray { dst },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            new_array,
            site.expr,
            site.span,
        );
        let mut current = site.block;
        let receiver_register =
            self.lower_callable_component_to_register(builder, site, current, receiver)?;
        current = receiver_register.block;
        self.emit_callable_array_insert(
            builder,
            site,
            current,
            dst,
            Operand::Register(receiver_register.register),
        );
        let method_value =
            self.lower_callable_component_to_register(builder, site, current, method)?;
        current = method_value.block;
        self.emit_callable_array_insert(
            builder,
            site,
            current,
            dst,
            Operand::Register(method_value.register),
        );
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn first_class_callable_runtime_value(&self, expr: ExprId) -> bool {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return false;
        };
        let Some(expression) = module.expressions().get(expr) else {
            return false;
        };
        matches!(
            expression.kind(),
            HirExprKind::Array { .. } | HirExprKind::New { .. } | HirExprKind::Variable { .. }
        ) || matches!(
            expression.kind(),
            HirExprKind::Unary {
                operator,
                expr: Some(inner),
            } if operator == "parenthesized" && self.first_class_callable_runtime_value(*inner)
        )
    }

    pub(super) fn callable_static_target_component(
        &self,
        expr: ExprId,
    ) -> Option<CallableComponent> {
        if let Some(class_name) = self.static_class_name(expr) {
            return Some(CallableComponent::String(class_name));
        }
        Some(CallableComponent::Expr(expr))
    }

    pub(super) fn callable_member_component(&self, expr: ExprId) -> Option<CallableComponent> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Literal { text } if text.starts_with('$') => {
                Some(CallableComponent::Expr(expr))
            }
            HirExprKind::Name { resolution } if resolution.source().starts_with('$') => {
                Some(CallableComponent::Expr(expr))
            }
            HirExprKind::Variable { .. } => Some(CallableComponent::Expr(expr)),
            _ => self
                .static_property_display_name(expr)
                .map(CallableComponent::String)
                .or(Some(CallableComponent::Expr(expr))),
        }
    }

    pub(super) fn lower_callable_component_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        block: BlockId,
        component: CallableComponent,
    ) -> Option<LoweredExpr> {
        match component {
            CallableComponent::Expr(expr) => {
                self.lower_expr_to_register(builder, site.function, block, expr)
            }
            CallableComponent::String(value) => Some(self.emit_constant_to_register(
                builder,
                LowerSite { block, ..site },
                IrConstant::String(value),
            )),
        }
    }

    pub(super) fn emit_callable_array_insert(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        block: BlockId,
        array: RegId,
        value: Operand,
    ) {
        let instruction = builder.emit(
            site.function,
            block,
            InstructionKind::ArrayInsert {
                array,
                key: None,
                value,
                by_ref_local: None,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            block,
            instruction,
            site.expr,
            site.span,
        );
    }

    pub(super) fn lower_closure_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        kind: SignatureKind,
        arrow_body: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(signature) = self.signature_for_expr(site.range, kind).cloned() else {
            if kind == SignatureKind::Closure {
                return self.lower_signatureless_closure_to_register(builder, site);
            }
            return None;
        };
        let mut captures = match kind {
            SignatureKind::Closure => self.explicit_capture_specs(signature.span()),
            SignatureKind::ArrowFunction => self.implicit_arrow_capture_specs(
                arrow_body.or_else(|| self.expr_id_for_span(signature.arrow_body()?)),
                signature.parameters(),
            ),
            _ => Vec::new(),
        };
        if matches!(kind, SignatureKind::Closure | SignatureKind::ArrowFunction)
            && !signature.flags().is_static()
            && builder.local_id(site.function, "this").is_some()
            && self.function_like_uses_variable(signature.span(), "$this")
            && !captures.iter().any(|capture| capture.name == "this")
        {
            captures.push(CaptureSpec {
                name: "this".to_owned(),
                by_ref: false,
            });
        }
        let closure_function =
            self.lower_closure_function(builder, site.expr, &signature, arrow_body, &captures);
        if !signature.flags().is_static() && builder.local_id(site.function, "this").is_some() {
            builder.intern_local(closure_function, "this");
        }
        let dst = builder.alloc_register(site.function);
        let capture_args = captures
            .iter()
            .map(|capture| {
                let local = builder.intern_local(site.function, &capture.name);
                ClosureCaptureArg {
                    name: capture.name.clone(),
                    src: Operand::Local(local),
                    by_ref: capture.by_ref,
                }
            })
            .collect();
        let instruction = builder.emit(
            site.function,
            site.block,
            InstructionKind::MakeClosure {
                dst,
                function: closure_function,
                captures: capture_args,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: site.block,
        })
    }

    pub(super) fn lower_signatureless_closure_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
    ) -> Option<LoweredExpr> {
        let mut captures = self.explicit_capture_specs(site.range);
        if builder.local_id(site.function, "this").is_some()
            && self.function_like_uses_variable(site.range, "$this")
            && !captures.iter().any(|capture| capture.name == "this")
        {
            captures.push(CaptureSpec {
                name: "this".to_owned(),
                by_ref: false,
            });
        }
        let closure_function =
            self.lower_signatureless_closure_function(builder, site.expr, site.range, &captures);
        if builder.local_id(site.function, "this").is_some() {
            builder.intern_local(closure_function, "this");
        }
        let dst = builder.alloc_register(site.function);
        let capture_args = captures
            .iter()
            .map(|capture| {
                let local = builder.intern_local(site.function, &capture.name);
                ClosureCaptureArg {
                    name: capture.name.clone(),
                    src: Operand::Local(local),
                    by_ref: capture.by_ref,
                }
            })
            .collect();
        let instruction = builder.emit(
            site.function,
            site.block,
            InstructionKind::MakeClosure {
                dst,
                function: closure_function,
                captures: capture_args,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: site.block,
        })
    }

    pub(super) fn lower_signatureless_closure_function(
        &mut self,
        builder: &mut IrBuilder,
        expr: ExprId,
        range: TextRange,
        captures: &[CaptureSpec],
    ) -> FunctionId {
        if let Some(function) = self.closure_functions.get(&expr) {
            return *function;
        }
        let span = span_from_range(self.file, range);
        let function = builder.start_function(
            format!("closure@{}", range.start().to_usize()),
            FunctionFlags {
                is_closure: true,
                ..FunctionFlags::default()
            },
            span,
        );
        self.closure_functions.insert(expr, function);
        builder.add_source_map(
            IrSourceMapTarget::Function { function },
            format!("hir:closure:{}", range.start().to_usize()),
            span,
        );
        for capture in captures {
            let local = builder.intern_local(function, &capture.name);
            builder.push_capture(
                function,
                IrCapture {
                    name: capture.name.clone(),
                    local,
                    by_ref: capture.by_ref,
                },
            );
        }
        let block = builder.append_block(function);
        builder.add_source_map(
            IrSourceMapTarget::Block { function, block },
            format!("hir:closure:{}:body", function.raw()),
            span,
        );
        let block = self.lower_auto_global_bindings(builder, function, block, range, span);
        let current =
            self.lower_stmt_list(builder, function, block, self.statement_ids_inside(range));
        if !builder.is_terminated(function, current) {
            builder.terminate_return(function, current, None, span);
        }
        function
    }

    pub(super) fn lower_closure_function(
        &mut self,
        builder: &mut IrBuilder,
        expr: ExprId,
        signature: &FunctionSignature,
        arrow_body: Option<ExprId>,
        captures: &[CaptureSpec],
    ) -> FunctionId {
        if let Some(function) = self.closure_functions.get(&expr) {
            return *function;
        }
        let span = span_from_range(self.file, signature.span());
        let name = match signature.kind() {
            SignatureKind::ArrowFunction => {
                format!("arrow@{}", signature.span().start().to_usize())
            }
            _ => format!("closure@{}", signature.span().start().to_usize()),
        };
        let function = builder.start_function(
            name,
            FunctionFlags {
                is_closure: true,
                is_static: signature.flags().is_static(),
                ..FunctionFlags::default()
            },
            span,
        );
        let attributes = self.lower_attributes_for_target_span(
            builder,
            AttributeTarget::Closure,
            signature.span(),
        );
        builder.set_function_attributes(function, attributes);
        self.closure_functions.insert(expr, function);
        builder.set_return_type(function, self.lower_return_type(signature.return_type()));
        builder.add_source_map(
            IrSourceMapTarget::Function { function },
            format!(
                "hir:{}:{}",
                signature.kind().as_str(),
                signature.span().start().to_usize()
            ),
            span,
        );
        for capture in captures {
            let local = builder.intern_local(function, &capture.name);
            builder.push_capture(
                function,
                IrCapture {
                    name: capture.name.clone(),
                    local,
                    by_ref: capture.by_ref,
                },
            );
        }
        for param in signature.parameters() {
            let local_name = local_name(param.name()).to_owned();
            let local = builder.intern_local(function, &local_name);
            let default = self.lower_param_default(param);
            if param.default().is_some() && default.is_none() {
                self.unsupported(
                    UnsupportedFeature::AdvancedParameter,
                    param.span(),
                    "parameter default is not a folded Semantic frontend constant expression",
                );
            }
            let attributes = self.lower_parameter_attributes(builder, param.attributes());
            let type_ = self.lower_param_runtime_type(param, &default);
            builder.push_param(
                function,
                IrParam {
                    name: local_name,
                    local,
                    required: param.default().is_none() && !param.flags().is_variadic(),
                    default,
                    type_,
                    by_ref: param.flags().is_by_ref(),
                    variadic: param.flags().is_variadic(),
                    attributes,
                },
            );
        }

        let block = builder.append_block(function);
        builder.add_source_map(
            IrSourceMapTarget::Block { function, block },
            format!("hir:{}:{}:body", signature.kind().as_str(), function.raw()),
            span,
        );
        let block =
            self.lower_auto_global_bindings(builder, function, block, signature.span(), span);
        match signature.kind() {
            SignatureKind::ArrowFunction => {
                let Some(body) = arrow_body.or_else(|| {
                    self.expr_id_for_span(signature.arrow_body().unwrap_or(signature.span()))
                }) else {
                    builder.terminate_return(function, block, None, span);
                    return function;
                };
                if let Some(value) = self.lower_expr_to_register(builder, function, block, body) {
                    builder.terminate_return(
                        function,
                        value.block,
                        Some(Operand::Register(value.register)),
                        span,
                    );
                } else {
                    builder.terminate_return(function, block, None, span);
                }
            }
            SignatureKind::Closure => {
                let body = self.statement_ids_inside(signature.span());
                let current = self.lower_stmt_list(builder, function, block, body);
                if !builder.is_terminated(function, current) {
                    builder.terminate_return(function, current, None, span);
                }
            }
            _ => {}
        }
        function
    }

    pub(super) fn lower_literal_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        text: &str,
    ) -> Option<LoweredExpr> {
        if let Some(parts) = interpolated_literal_parts(text) {
            return self.lower_interpolated_literal_to_register(builder, site, parts);
        }
        let Some(constant) = literal_constant(text) else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "literal kind is not lowered to IR in literal-lowering",
            );
            return None;
        };

        let constant = builder.intern_constant(constant);
        let register = builder.alloc_register(site.function);
        let load =
            builder.emit_load_const(site.function, site.block, register, constant, site.span);
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            load,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register,
            block: site.block,
        })
    }

    pub(super) fn lower_interpolated_literal_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        parts: Vec<InterpolatedPart>,
    ) -> Option<LoweredExpr> {
        let current = site.block;
        let mut value = None::<RegId>;
        for part in parts {
            let part_register = match part {
                InterpolatedPart::Bytes(bytes) => {
                    if bytes.is_empty() {
                        continue;
                    }
                    let register = builder.alloc_register(site.function);
                    let constant = builder.intern_constant(ir_string_constant(bytes));
                    let instruction = builder.emit_load_const(
                        site.function,
                        current,
                        register,
                        constant,
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        instruction,
                        site.expr,
                        site.span,
                    );
                    register
                }
                InterpolatedPart::Variable {
                    name,
                    dim,
                    deprecated_dollar_brace,
                } => {
                    if deprecated_dollar_brace {
                        if builder.function_flags(site.function).is_top_level {
                            self.record_early_diagnostic(
                                site.function,
                                site.expr,
                                site.span,
                                IrDiagnosticSeverity::Deprecation,
                                "E_PHP_RUNTIME_DEPRECATED_DOLLAR_BRACE_INTERPOLATION",
                                "Using ${var} in strings is deprecated, use {$var} instead",
                            );
                        } else {
                            let instruction = builder.emit(
                                site.function,
                                current,
                                InstructionKind::EmitDiagnostic {
                                    severity: IrDiagnosticSeverity::Deprecation,
                                    diagnostic_id:
                                        "E_PHP_RUNTIME_DEPRECATED_DOLLAR_BRACE_INTERPOLATION"
                                            .to_owned(),
                                    message:
                                        "Using ${var} in strings is deprecated, use {$var} instead"
                                            .to_owned(),
                                    leading_newline: true,
                                },
                                site.span,
                            );
                            self.add_expr_source_map(
                                builder,
                                site.function,
                                current,
                                instruction,
                                site.expr,
                                site.span,
                            );
                        }
                    }
                    let base_register = builder.alloc_register(site.function);
                    let local = builder.intern_local(site.function, name);
                    let instruction = builder.emit(
                        site.function,
                        current,
                        InstructionKind::LoadLocal {
                            dst: base_register,
                            local,
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        instruction,
                        site.expr,
                        site.span,
                    );
                    if let Some(dim) = dim {
                        let key_register = builder.alloc_register(site.function);
                        let key_constant = match dim {
                            InterpolatedDim::Variable(name) => {
                                let local = builder.intern_local(site.function, name);
                                let instruction = builder.emit(
                                    site.function,
                                    current,
                                    InstructionKind::LoadLocal {
                                        dst: key_register,
                                        local,
                                    },
                                    site.span,
                                );
                                self.add_expr_source_map(
                                    builder,
                                    site.function,
                                    current,
                                    instruction,
                                    site.expr,
                                    site.span,
                                );
                                None
                            }
                            InterpolatedDim::Int(value) => Some(IrConstant::Int(value)),
                            InterpolatedDim::String(value) => Some(IrConstant::String(value)),
                        };
                        if let Some(constant) = key_constant {
                            let constant = builder.intern_constant(constant);
                            let instruction = builder.emit_load_const(
                                site.function,
                                current,
                                key_register,
                                constant,
                                site.span,
                            );
                            self.add_expr_source_map(
                                builder,
                                site.function,
                                current,
                                instruction,
                                site.expr,
                                site.span,
                            );
                        }
                        let register = builder.alloc_register(site.function);
                        let instruction = builder.emit(
                            site.function,
                            current,
                            InstructionKind::FetchDim {
                                dst: register,
                                array: Operand::Register(base_register),
                                key: Operand::Register(key_register),
                                quiet: false,
                            },
                            site.span,
                        );
                        self.add_expr_source_map(
                            builder,
                            site.function,
                            current,
                            instruction,
                            site.expr,
                            site.span,
                        );
                        register
                    } else {
                        base_register
                    }
                }
                InterpolatedPart::MethodCall { receiver, method } => {
                    let object_register = builder.alloc_register(site.function);
                    let local = builder.intern_local(site.function, receiver);
                    let instruction = builder.emit(
                        site.function,
                        current,
                        InstructionKind::LoadLocal {
                            dst: object_register,
                            local,
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        instruction,
                        site.expr,
                        site.span,
                    );
                    let register = builder.alloc_register(site.function);
                    let instruction = builder.emit(
                        site.function,
                        current,
                        InstructionKind::CallMethod {
                            dst: register,
                            object: Operand::Register(object_register),
                            method: normalize_method_name(&method),
                            args: Vec::new(),
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        instruction,
                        site.expr,
                        site.span,
                    );
                    register
                }
                InterpolatedPart::Property {
                    receiver,
                    property,
                    dim,
                } => {
                    let object_register = builder.alloc_register(site.function);
                    let local = builder.intern_local(site.function, receiver);
                    let instruction = builder.emit(
                        site.function,
                        current,
                        InstructionKind::LoadLocal {
                            dst: object_register,
                            local,
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        instruction,
                        site.expr,
                        site.span,
                    );
                    let register = builder.alloc_register(site.function);
                    let instruction = builder.emit(
                        site.function,
                        current,
                        InstructionKind::FetchProperty {
                            dst: register,
                            object: Operand::Register(object_register),
                            property,
                        },
                        site.span,
                    );
                    self.add_expr_source_map(
                        builder,
                        site.function,
                        current,
                        instruction,
                        site.expr,
                        site.span,
                    );
                    if let Some(dim) = dim {
                        let key_register = builder.alloc_register(site.function);
                        let key_constant = match dim {
                            InterpolatedDim::Variable(name) => {
                                let local = builder.intern_local(site.function, name);
                                let instruction = builder.emit(
                                    site.function,
                                    current,
                                    InstructionKind::LoadLocal {
                                        dst: key_register,
                                        local,
                                    },
                                    site.span,
                                );
                                self.add_expr_source_map(
                                    builder,
                                    site.function,
                                    current,
                                    instruction,
                                    site.expr,
                                    site.span,
                                );
                                None
                            }
                            InterpolatedDim::Int(value) => Some(IrConstant::Int(value)),
                            InterpolatedDim::String(value) => Some(IrConstant::String(value)),
                        };
                        if let Some(constant) = key_constant {
                            let constant = builder.intern_constant(constant);
                            let instruction = builder.emit_load_const(
                                site.function,
                                current,
                                key_register,
                                constant,
                                site.span,
                            );
                            self.add_expr_source_map(
                                builder,
                                site.function,
                                current,
                                instruction,
                                site.expr,
                                site.span,
                            );
                        }
                        let dim_register = builder.alloc_register(site.function);
                        let instruction = builder.emit(
                            site.function,
                            current,
                            InstructionKind::FetchDim {
                                dst: dim_register,
                                array: Operand::Register(register),
                                key: Operand::Register(key_register),
                                quiet: false,
                            },
                            site.span,
                        );
                        self.add_expr_source_map(
                            builder,
                            site.function,
                            current,
                            instruction,
                            site.expr,
                            site.span,
                        );
                        dim_register
                    } else {
                        register
                    }
                }
            };

            value = Some(if let Some(left) = value {
                let dst = builder.alloc_register(site.function);
                let instruction = builder.emit(
                    site.function,
                    current,
                    InstructionKind::Binary {
                        dst,
                        op: BinaryOp::Concat,
                        lhs: Operand::Register(left),
                        rhs: Operand::Register(part_register),
                    },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    instruction,
                    site.expr,
                    site.span,
                );
                dst
            } else {
                part_register
            });
        }

        let register = if let Some(register) = value {
            let dst = builder.alloc_register(site.function);
            let instruction = builder.emit(
                site.function,
                current,
                InstructionKind::Cast {
                    dst,
                    kind: CastKind::String,
                    src: Operand::Register(register),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                instruction,
                site.expr,
                site.span,
            );
            dst
        } else {
            let register = builder.alloc_register(site.function);
            let constant = builder.intern_constant(IrConstant::String(String::new()));
            let instruction =
                builder.emit_load_const(site.function, current, register, constant, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                instruction,
                site.expr,
                site.span,
            );
            register
        };
        Some(LoweredExpr {
            register,
            block: current,
        })
    }

    pub(super) fn emit_constant_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        constant: IrConstant,
    ) -> LoweredExpr {
        let constant = builder.intern_constant(constant);
        let register = builder.alloc_register(site.function);
        let instruction =
            builder.emit_load_const(site.function, site.block, register, constant, site.span);
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            instruction,
            site.expr,
            site.span,
        );
        LoweredExpr {
            register,
            block: site.block,
        }
    }

    pub(super) fn magic_constant(&self, text: &str, site: LowerSite) -> Option<IrConstant> {
        let normalized = text.trim().to_ascii_uppercase();
        match normalized.as_str() {
            "__FILE__" => Some(IrConstant::String(self.options.source_path.clone())),
            "__DIR__" => Some(IrConstant::String(source_dir(&self.options.source_path))),
            "__LINE__" => Some(IrConstant::Int(
                self.source_text
                    .line_col(BytePos::new(site.range.start().to_usize()))
                    .line as i64,
            )),
            "__FUNCTION__" => Some(IrConstant::String(
                self.method_names
                    .get(&site.function)
                    .or_else(|| self.function_names.get(&site.function))
                    .cloned()
                    .unwrap_or_default(),
            )),
            "__CLASS__" => Some(IrConstant::String(
                self.class_names
                    .get(&site.function)
                    .cloned()
                    .unwrap_or_default(),
            )),
            "__NAMESPACE__" => Some(IrConstant::String(
                self.namespace_names
                    .get(&site.function)
                    .cloned()
                    .unwrap_or_default(),
            )),
            "__METHOD__" => Some(IrConstant::String(
                self.function_names
                    .get(&site.function)
                    .cloned()
                    .unwrap_or_default(),
            )),
            _ => None,
        }
    }

    pub(super) fn lower_short_circuit_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        operator: &str,
        left: Option<ExprId>,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let left = left?;
        let right = right?;
        let left_value = if operator == "??" {
            self.lower_coalesce_left_to_register(builder, site, left)?
        } else {
            self.lower_expr_to_register(builder, site.function, site.block, left)?
        };
        let dst = builder.alloc_register(site.function);
        let false_block = builder.append_block(site.function);
        let true_block = builder.append_block(site.function);
        let after_block = builder.append_block(site.function);

        match operator {
            "&&" | "and" => {
                builder.terminate_jump_if(
                    site.function,
                    left_value.block,
                    Operand::Register(left_value.register),
                    true_block,
                    false_block,
                    site.span,
                );
                self.emit_bool_move(builder, site.function, false_block, dst, false, site.span);
                self.jump_if_open(builder, site.function, false_block, after_block, site.span);
                let right_value =
                    self.lower_expr_to_register(builder, site.function, true_block, right)?;
                self.emit_bool_cast(
                    builder,
                    site.function,
                    right_value.block,
                    dst,
                    right_value.register,
                    site.span,
                );
                self.jump_if_open(
                    builder,
                    site.function,
                    right_value.block,
                    after_block,
                    site.span,
                );
            }
            "||" | "or" => {
                builder.terminate_jump_if(
                    site.function,
                    left_value.block,
                    Operand::Register(left_value.register),
                    true_block,
                    false_block,
                    site.span,
                );
                let right_value =
                    self.lower_expr_to_register(builder, site.function, false_block, right)?;
                self.emit_bool_cast(
                    builder,
                    site.function,
                    right_value.block,
                    dst,
                    right_value.register,
                    site.span,
                );
                self.jump_if_open(
                    builder,
                    site.function,
                    right_value.block,
                    after_block,
                    site.span,
                );
                self.emit_bool_move(builder, site.function, true_block, dst, true, site.span);
                self.jump_if_open(builder, site.function, true_block, after_block, site.span);
            }
            "??" => {
                let is_null = builder.alloc_register(site.function);
                let null = builder.intern_constant(IrConstant::Null);
                builder.emit(
                    site.function,
                    left_value.block,
                    InstructionKind::Compare {
                        dst: is_null,
                        op: CompareOp::Identical,
                        lhs: Operand::Register(left_value.register),
                        rhs: Operand::Constant(null),
                    },
                    site.span,
                );
                builder.terminate_jump_if(
                    site.function,
                    left_value.block,
                    Operand::Register(is_null),
                    true_block,
                    false_block,
                    site.span,
                );
                builder.emit(
                    site.function,
                    false_block,
                    InstructionKind::Move {
                        dst,
                        src: Operand::Register(left_value.register),
                    },
                    site.span,
                );
                self.jump_if_open(builder, site.function, false_block, after_block, site.span);
                let right_value =
                    self.lower_expr_to_register(builder, site.function, true_block, right)?;
                builder.emit(
                    site.function,
                    right_value.block,
                    InstructionKind::Move {
                        dst,
                        src: Operand::Register(right_value.register),
                    },
                    site.span,
                );
                self.jump_if_open(
                    builder,
                    site.function,
                    right_value.block,
                    after_block,
                    site.span,
                );
            }
            _ => return None,
        }

        Some(LoweredExpr {
            register: dst,
            block: after_block,
        })
    }

    pub(super) fn lower_logical_xor_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        left: Option<ExprId>,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(left) = left else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "logical xor expression is missing its left operand",
            );
            return None;
        };
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "logical xor expression is missing its right operand",
            );
            return None;
        };
        let left_value = self.lower_expr_to_register(builder, site.function, site.block, left)?;
        let left_bool = builder.alloc_register(site.function);
        self.emit_bool_cast(
            builder,
            site.function,
            left_value.block,
            left_bool,
            left_value.register,
            site.span,
        );
        let right_value =
            self.lower_expr_to_register(builder, site.function, left_value.block, right)?;
        let right_bool = builder.alloc_register(site.function);
        self.emit_bool_cast(
            builder,
            site.function,
            right_value.block,
            right_bool,
            right_value.register,
            site.span,
        );
        let dst = builder.alloc_register(site.function);
        let compare = builder.emit(
            site.function,
            right_value.block,
            InstructionKind::Compare {
                dst,
                op: CompareOp::NotIdentical,
                lhs: Operand::Register(left_bool),
                rhs: Operand::Register(right_bool),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            right_value.block,
            compare,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: right_value.block,
        })
    }

    pub(super) fn lower_ternary_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        condition: Option<ExprId>,
        if_true: Option<ExprId>,
        if_false: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let condition = condition?;
        let condition_value =
            self.lower_expr_to_register(builder, site.function, site.block, condition)?;
        let false_block = builder.append_block(site.function);
        let true_block = builder.append_block(site.function);
        let after_block = builder.append_block(site.function);
        let dst = builder.alloc_register(site.function);
        builder.terminate_jump_if(
            site.function,
            condition_value.block,
            Operand::Register(condition_value.register),
            true_block,
            false_block,
            site.span,
        );

        let false_expr = if_false?;
        let false_value =
            self.lower_expr_to_register(builder, site.function, false_block, false_expr)?;
        builder.emit(
            site.function,
            false_value.block,
            InstructionKind::Move {
                dst,
                src: Operand::Register(false_value.register),
            },
            site.span,
        );
        self.jump_if_open(
            builder,
            site.function,
            false_value.block,
            after_block,
            site.span,
        );

        let true_value = if let Some(if_true) = if_true {
            self.lower_expr_to_register(builder, site.function, true_block, if_true)?
        } else {
            LoweredExpr {
                register: condition_value.register,
                block: true_block,
            }
        };
        builder.emit(
            site.function,
            true_value.block,
            InstructionKind::Move {
                dst,
                src: Operand::Register(true_value.register),
            },
            site.span,
        );
        self.jump_if_open(
            builder,
            site.function,
            true_value.block,
            after_block,
            site.span,
        );

        Some(LoweredExpr {
            register: dst,
            block: after_block,
        })
    }

    pub(super) fn lower_match_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        subject: Option<ExprId>,
        arms: Vec<HirMatchArm>,
    ) -> Option<LoweredExpr> {
        let subject = subject?;
        let subject_value =
            self.lower_expr_to_register(builder, site.function, site.block, subject)?;
        let dst = builder.alloc_register(site.function);
        let after_block = builder.append_block(site.function);
        let result_blocks = arms
            .iter()
            .map(|_| builder.append_block(site.function))
            .collect::<Vec<_>>();
        let default_index = arms.iter().position(|arm| arm.is_default);
        let error_block = (default_index.is_none()).then(|| builder.append_block(site.function));
        let fallback = default_index
            .map(|index| result_blocks[index])
            .or(error_block)
            .unwrap_or(after_block);
        let conditions = arms
            .iter()
            .enumerate()
            .flat_map(|(arm_index, arm)| {
                arm.conditions
                    .iter()
                    .copied()
                    .map(move |condition| (arm_index, condition))
            })
            .collect::<Vec<_>>();
        let mut current_check = subject_value.block;

        for (position, (arm_index, condition)) in conditions.iter().copied().enumerate() {
            let false_target = if position + 1 == conditions.len() {
                fallback
            } else {
                builder.append_block(site.function)
            };
            let condition_value =
                self.lower_expr_to_register(builder, site.function, current_check, condition)?;
            let matched = builder.alloc_register(site.function);
            builder.emit(
                site.function,
                condition_value.block,
                InstructionKind::Compare {
                    dst: matched,
                    op: CompareOp::Identical,
                    lhs: Operand::Register(subject_value.register),
                    rhs: Operand::Register(condition_value.register),
                },
                site.span,
            );
            builder.terminate_jump_if(
                site.function,
                condition_value.block,
                Operand::Register(matched),
                result_blocks[arm_index],
                false_target,
                site.span,
            );
            current_check = false_target;
        }
        if conditions.is_empty() {
            self.jump_if_open(builder, site.function, current_check, fallback, site.span);
        }

        for (index, arm) in arms.into_iter().enumerate() {
            let Some(result) = arm.result else {
                continue;
            };
            let result_value =
                self.lower_expr_to_register(builder, site.function, result_blocks[index], result)?;
            builder.emit(
                site.function,
                result_value.block,
                InstructionKind::Move {
                    dst,
                    src: Operand::Register(result_value.register),
                },
                site.span,
            );
            self.jump_if_open(
                builder,
                site.function,
                result_value.block,
                after_block,
                site.span,
            );
        }
        if let Some(error_block) = error_block {
            builder.emit(
                site.function,
                error_block,
                InstructionKind::RuntimeError {
                    diagnostic_id: "E_PHP_VM_UNHANDLED_MATCH".to_owned(),
                    message: "match expression did not match any arm".to_owned(),
                },
                site.span,
            );
            let null = builder.intern_constant(IrConstant::Null);
            builder.terminate_return(
                site.function,
                error_block,
                Some(Operand::Constant(null)),
                site.span,
            );
        }
        Some(LoweredExpr {
            register: dst,
            block: after_block,
        })
    }

    pub(super) fn lower_coalesce_left_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        left: ExprId,
    ) -> Option<LoweredExpr> {
        let variable_name = {
            let module = self
                .frontend
                .database()
                .module(self.frontend.module().module_id())?;
            let expression = module.expressions().get(left)?;
            match expression.kind() {
                HirExprKind::Variable { name } => Some(local_name(name).to_owned()),
                _ => None,
            }
        };
        if let Some(name) = variable_name {
            let local = builder.intern_local(site.function, name);
            let dst = builder.alloc_register(site.function);
            let range = self.span_for(SourceMappedId::from(left));
            let span = span_from_range(self.file, range);
            let instruction = builder.emit(
                site.function,
                site.block,
                InstructionKind::LoadLocalQuiet { dst, local },
                span,
            );
            self.add_expr_source_map(builder, site.function, site.block, instruction, left, span);
            return Some(LoweredExpr {
                register: dst,
                block: site.block,
            });
        }
        if let Some(target) = self.dim_assignment_target(builder, site.function, left)
            && !target.append
            && !target.dims.is_empty()
        {
            return self.lower_quiet_dim_target_to_register(builder, site, target, left);
        }
        if let Some(target) = self.property_dim_target(left)
            && !target.append
            && !target.dims.is_empty()
        {
            return self.lower_quiet_property_dim_target_to_register(builder, site, target, left);
        }
        self.lower_expr_to_register(builder, site.function, site.block, left)
    }

    pub(super) fn lower_quiet_dim_target_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DimAssignmentTarget,
        expr: ExprId,
    ) -> Option<LoweredExpr> {
        let range = self.span_for(SourceMappedId::from(expr));
        let span = span_from_range(self.file, range);
        let mut current = site.block;
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim_value = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim_value.block;
            dims.push(Operand::Register(dim_value.register));
        }
        let mut value = builder.alloc_register(site.function);
        let load = builder.emit(
            site.function,
            current,
            InstructionKind::LoadLocalQuiet {
                dst: value,
                local: target.local,
            },
            span,
        );
        self.add_expr_source_map(builder, site.function, current, load, expr, span);
        for dim in dims {
            let fetched = builder.alloc_register(site.function);
            let fetch = builder.emit(
                site.function,
                current,
                InstructionKind::FetchDim {
                    dst: fetched,
                    array: Operand::Register(value),
                    key: dim,
                    quiet: true,
                },
                span,
            );
            self.add_expr_source_map(builder, site.function, current, fetch, expr, span);
            value = fetched;
        }
        Some(LoweredExpr {
            register: value,
            block: current,
        })
    }

    pub(super) fn lower_quiet_property_dim_target_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: PropertyDimTarget,
        expr: ExprId,
    ) -> Option<LoweredExpr> {
        let range = self.span_for(SourceMappedId::from(expr));
        let span = span_from_range(self.file, range);
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        let mut current = object.block;
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim_value = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim_value.block;
            dims.push(Operand::Register(dim_value.register));
        }
        let mut value = builder.alloc_register(site.function);
        let fetch_property = builder.emit(
            site.function,
            current,
            InstructionKind::FetchProperty {
                dst: value,
                object: Operand::Register(object.register),
                property: target.property,
            },
            span,
        );
        self.add_expr_source_map(builder, site.function, current, fetch_property, expr, span);
        for dim in dims {
            let fetched = builder.alloc_register(site.function);
            let fetch = builder.emit(
                site.function,
                current,
                InstructionKind::FetchDim {
                    dst: fetched,
                    array: Operand::Register(value),
                    key: dim,
                    quiet: true,
                },
                span,
            );
            self.add_expr_source_map(builder, site.function, current, fetch, expr, span);
            value = fetched;
        }
        Some(LoweredExpr {
            register: value,
            block: current,
        })
    }

    pub(super) fn lower_error_suppression_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        inner: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(inner) = inner else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "error suppression expression is missing its operand",
            );
            return None;
        };
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(inner)?;
        match expression.kind() {
            HirExprKind::Variable { name } => {
                let local = builder.intern_local(site.function, local_name(name));
                let dst = builder.alloc_register(site.function);
                let range = self.span_for(SourceMappedId::from(inner));
                let span = span_from_range(self.file, range);
                let instruction = builder.emit(
                    site.function,
                    site.block,
                    InstructionKind::LoadLocalQuiet { dst, local },
                    span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    site.block,
                    instruction,
                    inner,
                    span,
                );
                Some(LoweredExpr {
                    register: dst,
                    block: site.block,
                })
            }
            _ => self.lower_expr_to_register(builder, site.function, site.block, inner),
        }
    }

    pub(super) fn emit_bool_move(
        &self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        dst: crate::ids::RegId,
        value: bool,
        span: IrSpan,
    ) {
        let constant = builder.intern_constant(IrConstant::Bool(value));
        builder.emit(
            function,
            block,
            InstructionKind::Move {
                dst,
                src: Operand::Constant(constant),
            },
            span,
        );
    }

    pub(super) fn emit_bool_cast(
        &self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        dst: crate::ids::RegId,
        src: crate::ids::RegId,
        span: IrSpan,
    ) {
        builder.emit(
            function,
            block,
            InstructionKind::Cast {
                dst,
                kind: CastKind::Bool,
                src: Operand::Register(src),
            },
            span,
        );
    }

    pub(super) fn lower_cast_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        inner: Option<ExprId>,
        cast: CastKind,
    ) -> Option<LoweredExpr> {
        let Some(inner) = inner else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "cast expression is missing its operand",
            );
            return None;
        };
        let src = self.lower_expr_to_register(builder, site.function, site.block, inner)?;
        if cast == CastKind::Void {
            let discard = builder.emit(
                site.function,
                src.block,
                InstructionKind::Discard {
                    src: Operand::Register(src.register),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                src.block,
                discard,
                site.expr,
                site.span,
            );
            return self.lower_literal_to_register(
                builder,
                LowerSite {
                    block: src.block,
                    ..site
                },
                "null",
            );
        }
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            src.block,
            InstructionKind::Cast {
                dst,
                kind: cast,
                src: Operand::Register(src.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            src.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: src.block,
        })
    }

    pub(super) fn lower_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        operator: &str,
        left: Option<ExprId>,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        if operator == "??=" {
            return self.lower_coalesce_assign_to_register(builder, site, left, right);
        }
        if operator == "=&" {
            return self.lower_reference_assign_to_register(builder, site, left, right);
        }
        if operator == "="
            && let Some(left) = left
            && let Some(target) = self.property_assignment_target(left)
        {
            return self.lower_property_assign_to_register(builder, site, target, right);
        }
        if operator != "="
            && let Some(left) = left
            && let Some(target) = self.property_assignment_target(left)
        {
            return self.lower_property_compound_assign_to_register(
                builder, site, target, operator, right,
            );
        }
        if operator == "="
            && let Some(left) = left
            && let Some(target) = self.property_dim_target(left)
        {
            return self.lower_property_dim_assign_to_register(builder, site, target, right);
        }
        if operator != "="
            && let Some(left) = left
            && let Some(target) = self.property_dim_target(left)
            && !target.append
            && !target.dims.is_empty()
        {
            return self.lower_property_dim_compound_assign_to_register(
                builder, site, target, operator, left, right,
            );
        }
        if operator == "="
            && let Some(left) = left
            && let Some(target) = self.dynamic_property_dim_target(left)
        {
            return self
                .lower_dynamic_property_dim_assign_to_register(builder, site, target, right);
        }
        if operator == "="
            && let Some(left) = left
            && let Some(target) = self.property_dim_target(left)
        {
            return self.lower_property_dim_assign_to_register(builder, site, target, right);
        }
        if operator == "="
            && let Some(left) = left
            && let Some(target) = self.dynamic_property_target(left)
        {
            return self.lower_dynamic_property_assign_to_register(builder, site, target, right);
        }
        if operator == "="
            && let Some(left) = left
            && let Some(target) = self.static_property_target(left)
        {
            return self.lower_static_property_assign_to_register(builder, site, target, right);
        }
        if operator == "="
            && let Some(left) = left
            && let Some(target) = self.dynamic_static_property_target(left)
        {
            return self
                .lower_dynamic_static_property_assign_to_register(builder, site, target, right);
        }
        if operator == "="
            && let Some(left) = left
            && let Some(target) = self.static_property_dim_target(left)
        {
            return self.lower_static_property_dim_assign_to_register(builder, site, target, right);
        }
        if operator != "="
            && let Some(left) = left
            && let Some(target) = self.static_property_target(left)
        {
            return self.lower_static_property_compound_assign_to_register(
                builder, site, target, operator, right,
            );
        }
        if operator == "="
            && let Some(left) = left
            && let Some(targets) = self.foreach_destructuring_targets(builder, site.function, left)
        {
            return self.lower_destructuring_assign_to_register(builder, site, targets, right);
        }
        if operator != "="
            && let Some(left) = left
            && let Some(target) = self.dim_assignment_target(builder, site.function, left)
            && !target.append
            && !target.dims.is_empty()
        {
            return self.lower_dim_compound_assign_to_register(
                builder, site, target, operator, left, right,
            );
        }
        if operator == "="
            && let Some(left) = left
            && let Some(target) = self.dim_assignment_target(builder, site.function, left)
            && (target.append || !target.dims.is_empty())
        {
            return self.lower_dim_assign_to_register(builder, site, target, right);
        }
        if operator == "="
            && let Some(left) = left
            && let Some((coalesce_left, target)) =
                self.coalesce_assignment_target(builder, site.function, left)
        {
            return self.lower_coalesce_expression_assignment_to_register(
                builder,
                site,
                coalesce_left,
                target,
                right,
            );
        }
        if operator == "="
            && let Some(left) = left
            && let Some((logical_operator, logical_left, logical_right)) =
                self.logical_assignment_target(builder, site.function, left)
        {
            return self.lower_logical_assignment_to_register(
                builder,
                site,
                &logical_operator,
                logical_left,
                logical_right,
                right,
            );
        }
        if operator == "="
            && let Some(left) = left
            && let Some((compare_operator, compare_left, target)) =
                self.comparison_assignment_target(builder, site.function, left)
        {
            return self.lower_comparison_assignment_to_register(
                builder,
                site,
                &compare_operator,
                compare_left,
                target,
                right,
            );
        }
        if operator == "="
            && let Some(left) = left
            && let Some((unary_operator, target)) =
                self.unary_assignment_target(builder, site.function, left)
        {
            return self.lower_unary_assignment_to_register(
                builder,
                site,
                &unary_operator,
                target,
                right,
            );
        }
        let Some(local) = left.and_then(|left| self.variable_local(builder, site.function, left))
        else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "only simple variable assignment is lowered to IR in local-variable",
            );
            return None;
        };
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "assignment expression is missing its right operand",
            );
            return None;
        };
        let value = if operator == "=" {
            self.lower_expr_to_register(builder, site.function, site.block, right)?
        } else {
            let Some(binary) = assignment_binary_op(operator) else {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    site.range,
                    format!("assignment operator `{operator}` is not lowered to IR yet"),
                );
                return None;
            };
            let lhs = builder.alloc_register(site.function);
            let load = builder.emit(
                site.function,
                site.block,
                InstructionKind::LoadLocal { dst: lhs, local },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                site.block,
                load,
                site.expr,
                site.span,
            );
            let rhs = self.lower_expr_to_register(builder, site.function, site.block, right)?;
            let dst = builder.alloc_register(site.function);
            let instruction = builder.emit(
                site.function,
                rhs.block,
                InstructionKind::Binary {
                    dst,
                    op: binary,
                    lhs: Operand::Register(lhs),
                    rhs: Operand::Register(rhs.register),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                rhs.block,
                instruction,
                site.expr,
                site.span,
            );
            LoweredExpr {
                register: dst,
                block: rhs.block,
            }
        };
        let store = builder.emit(
            site.function,
            value.block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            store,
            site.expr,
            site.span,
        );
        Some(value)
    }

    pub(super) fn lower_coalesce_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        left: Option<ExprId>,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        if let Some(left) = left
            && let Some(target) = self.property_assignment_target(left)
        {
            return self.lower_property_coalesce_assign_to_register(builder, site, target, right);
        }
        if let Some(left) = left
            && let Some(target) = self.dim_assignment_target(builder, site.function, left)
            && !target.append
            && !target.dims.is_empty()
        {
            return self.lower_dim_coalesce_assign_to_register(builder, site, target, right);
        }

        let Some(local) = left.and_then(|left| self.variable_local(builder, site.function, left))
        else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "only local and array-dimension null coalescing assignment is lowered to IR",
            );
            return None;
        };
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "null coalescing assignment is missing its right operand",
            );
            return None;
        };

        let dst = builder.alloc_register(site.function);
        let present = builder.alloc_register(site.function);
        builder.emit(
            site.function,
            site.block,
            InstructionKind::IssetLocal {
                dst: present,
                local,
            },
            site.span,
        );

        let existing_block = builder.append_block(site.function);
        let assign_block = builder.append_block(site.function);
        let after_block = builder.append_block(site.function);
        builder.terminate_jump_if(
            site.function,
            site.block,
            Operand::Register(present),
            existing_block,
            assign_block,
            site.span,
        );

        builder.emit(
            site.function,
            existing_block,
            InstructionKind::LoadLocal { dst, local },
            site.span,
        );
        self.jump_if_open(
            builder,
            site.function,
            existing_block,
            after_block,
            site.span,
        );

        let value = self.lower_expr_to_register(builder, site.function, assign_block, right)?;
        let store = builder.emit(
            site.function,
            value.block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            store,
            site.expr,
            site.span,
        );
        builder.emit(
            site.function,
            value.block,
            InstructionKind::Move {
                dst,
                src: Operand::Register(value.register),
            },
            site.span,
        );
        self.jump_if_open(builder, site.function, value.block, after_block, site.span);

        Some(LoweredExpr {
            register: dst,
            block: after_block,
        })
    }

    pub(super) fn lower_property_coalesce_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: PropertyAssignmentTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "property null coalescing assignment is missing its right operand",
            );
            return None;
        };

        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        let dst = builder.alloc_register(site.function);
        let present = builder.alloc_register(site.function);
        builder.emit(
            site.function,
            object.block,
            InstructionKind::IssetProperty {
                dst: present,
                object: Operand::Register(object.register),
                property: target.property.clone(),
            },
            site.span,
        );

        let existing_block = builder.append_block(site.function);
        let assign_block = builder.append_block(site.function);
        let after_block = builder.append_block(site.function);
        builder.terminate_jump_if(
            site.function,
            object.block,
            Operand::Register(present),
            existing_block,
            assign_block,
            site.span,
        );

        builder.emit(
            site.function,
            existing_block,
            InstructionKind::FetchProperty {
                dst,
                object: Operand::Register(object.register),
                property: target.property.clone(),
            },
            site.span,
        );
        self.jump_if_open(
            builder,
            site.function,
            existing_block,
            after_block,
            site.span,
        );

        let value = self.lower_expr_to_register(builder, site.function, assign_block, right)?;
        let assign = builder.emit(
            site.function,
            value.block,
            InstructionKind::AssignProperty {
                dst,
                object: Operand::Register(object.register),
                property: target.property,
                value: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            assign,
            site.expr,
            site.span,
        );
        self.jump_if_open(builder, site.function, value.block, after_block, site.span);

        Some(LoweredExpr {
            register: dst,
            block: after_block,
        })
    }

    pub(super) fn lower_coalesce_expression_assignment_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        coalesce_left: ExprId,
        target: LocalId,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "null coalescing assignment expression is missing its right operand",
            );
            return None;
        };

        let left_value = self.lower_coalesce_left_to_register(builder, site, coalesce_left)?;
        let dst = builder.alloc_register(site.function);
        let is_null = builder.alloc_register(site.function);
        let null = builder.intern_constant(IrConstant::Null);
        builder.emit(
            site.function,
            left_value.block,
            InstructionKind::Compare {
                dst: is_null,
                op: CompareOp::Identical,
                lhs: Operand::Register(left_value.register),
                rhs: Operand::Constant(null),
            },
            site.span,
        );

        let existing_block = builder.append_block(site.function);
        let assign_block = builder.append_block(site.function);
        let after_block = builder.append_block(site.function);
        builder.terminate_jump_if(
            site.function,
            left_value.block,
            Operand::Register(is_null),
            assign_block,
            existing_block,
            site.span,
        );

        builder.emit(
            site.function,
            existing_block,
            InstructionKind::Move {
                dst,
                src: Operand::Register(left_value.register),
            },
            site.span,
        );
        self.jump_if_open(
            builder,
            site.function,
            existing_block,
            after_block,
            site.span,
        );

        let value = self.lower_expr_to_register(builder, site.function, assign_block, right)?;
        let store = builder.emit(
            site.function,
            value.block,
            InstructionKind::StoreLocal {
                local: target,
                src: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            store,
            site.expr,
            site.span,
        );
        builder.emit(
            site.function,
            value.block,
            InstructionKind::Move {
                dst,
                src: Operand::Register(value.register),
            },
            site.span,
        );
        self.jump_if_open(builder, site.function, value.block, after_block, site.span);

        Some(LoweredExpr {
            register: dst,
            block: after_block,
        })
    }

    pub(super) fn lower_property_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: PropertyAssignmentTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "property assignment is missing its right operand",
            );
            return None;
        };
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        let value = self.lower_expr_to_register(builder, site.function, object.block, right)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            value.block,
            InstructionKind::AssignProperty {
                dst,
                object: Operand::Register(object.register),
                property: target.property,
                value: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: value.block,
        })
    }

    pub(super) fn lower_property_compound_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: PropertyAssignmentTarget,
        operator: &str,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(binary) = assignment_binary_op(operator) else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!("assignment operator `{operator}` is not lowered to IR yet"),
            );
            return None;
        };
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "property compound assignment is missing its right operand",
            );
            return None;
        };
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        let old = builder.alloc_register(site.function);
        let fetch = builder.emit(
            site.function,
            object.block,
            InstructionKind::FetchProperty {
                dst: old,
                object: Operand::Register(object.register),
                property: target.property.clone(),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            object.block,
            fetch,
            site.expr,
            site.span,
        );
        let rhs = self.lower_expr_to_register(builder, site.function, object.block, right)?;
        let value = builder.alloc_register(site.function);
        let arithmetic = builder.emit(
            site.function,
            rhs.block,
            InstructionKind::Binary {
                dst: value,
                op: binary,
                lhs: Operand::Register(old),
                rhs: Operand::Register(rhs.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            rhs.block,
            arithmetic,
            site.expr,
            site.span,
        );
        let assign_result = builder.alloc_register(site.function);
        let assign = builder.emit(
            site.function,
            rhs.block,
            InstructionKind::AssignProperty {
                dst: assign_result,
                object: Operand::Register(object.register),
                property: target.property,
                value: Operand::Register(value),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            rhs.block,
            assign,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: value,
            block: rhs.block,
        })
    }

    pub(super) fn lower_property_dim_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: PropertyDimTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "property array assignment is missing its right operand",
            );
            return None;
        };
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        let mut current = object.block;
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim.block;
            dims.push(Operand::Register(dim.register));
        }
        let value = self.lower_expr_to_register(builder, site.function, current, right)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            value.block,
            InstructionKind::AssignPropertyDim {
                dst,
                object: Operand::Register(object.register),
                property: target.property,
                dims,
                value: Operand::Register(value.register),
                append: target.append,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: value.block,
        })
    }

    pub(super) fn lower_property_dim_compound_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: PropertyDimTarget,
        operator: &str,
        left: ExprId,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "property array dimension compound assignment is missing its right operand",
            );
            return None;
        };
        let Some(binary) = assignment_binary_op(operator) else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!("assignment operator `{operator}` is not lowered to IR yet"),
            );
            return None;
        };
        let old = self.lower_expr_to_register(builder, site.function, site.block, left)?;
        let rhs = self.lower_expr_to_register(builder, site.function, old.block, right)?;
        let value = builder.alloc_register(site.function);
        let arithmetic = builder.emit(
            site.function,
            rhs.block,
            InstructionKind::Binary {
                dst: value,
                op: binary,
                lhs: Operand::Register(old.register),
                rhs: Operand::Register(rhs.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            rhs.block,
            arithmetic,
            site.expr,
            site.span,
        );
        let object =
            self.lower_expr_to_register(builder, site.function, rhs.block, target.receiver)?;
        let mut current = object.block;
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim.block;
            dims.push(Operand::Register(dim.register));
        }
        let dst = builder.alloc_register(site.function);
        let assign = builder.emit(
            site.function,
            current,
            InstructionKind::AssignPropertyDim {
                dst,
                object: Operand::Register(object.register),
                property: target.property,
                dims,
                value: Operand::Register(value),
                append: target.append,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            assign,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: value,
            block: current,
        })
    }

    pub(super) fn lower_dynamic_property_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DynamicPropertyTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "property assignment is missing its right operand",
            );
            return None;
        };
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        let property = self.lower_dynamic_member_name_to_register(
            builder,
            site,
            object.block,
            target.property,
        )?;
        let value = self.lower_expr_to_register(builder, site.function, property.block, right)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            value.block,
            InstructionKind::AssignDynamicProperty {
                dst,
                object: Operand::Register(object.register),
                property: Operand::Register(property.register),
                value: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: value.block,
        })
    }

    pub(super) fn lower_dynamic_property_dim_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DynamicPropertyDimTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        if target.dims.len() > 1 || (target.append && !target.dims.is_empty()) {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "only top-level dynamic property array dimension assignment is lowered to IR",
            );
            return None;
        }
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "dynamic property array assignment is missing its right operand",
            );
            return None;
        };
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        let property = self.lower_dynamic_member_name_to_register(
            builder,
            site,
            object.block,
            target.property,
        )?;
        let array = builder.alloc_register(site.function);
        let fetch = builder.emit(
            site.function,
            property.block,
            InstructionKind::FetchDynamicProperty {
                dst: array,
                object: Operand::Register(object.register),
                property: Operand::Register(property.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            property.block,
            fetch,
            site.expr,
            site.span,
        );
        let mut current = property.block;
        let key = if let Some(dim) = target.dims.first().copied() {
            let dim = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim.block;
            Some(Operand::Register(dim.register))
        } else {
            None
        };
        let value = self.lower_expr_to_register(builder, site.function, current, right)?;
        let insert = builder.emit(
            site.function,
            value.block,
            InstructionKind::ArrayInsert {
                array,
                key,
                value: Operand::Register(value.register),
                by_ref_local: None,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            insert,
            site.expr,
            site.span,
        );
        let dst = builder.alloc_register(site.function);
        let assign = builder.emit(
            site.function,
            value.block,
            InstructionKind::AssignDynamicProperty {
                dst,
                object: Operand::Register(object.register),
                property: Operand::Register(property.register),
                value: Operand::Register(array),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            assign,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: value.register,
            block: value.block,
        })
    }

    pub(super) fn lower_static_property_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: StaticPropertyTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "static property assignment is missing its right operand",
            );
            return None;
        };
        let value = self.lower_expr_to_register(builder, site.function, site.block, right)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            value.block,
            InstructionKind::AssignStaticProperty {
                dst,
                class_name: target.class_name,
                property: target.property,
                value: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: value.block,
        })
    }

    pub(super) fn lower_dynamic_static_property_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DynamicStaticPropertyTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "dynamic static property assignment is missing its right operand",
            );
            return None;
        };
        let class_name =
            self.lower_expr_to_register(builder, site.function, site.block, target.class_name)?;
        let value = self.lower_expr_to_register(builder, site.function, class_name.block, right)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            value.block,
            InstructionKind::AssignDynamicStaticProperty {
                dst,
                class_name: Operand::Register(class_name.register),
                property: target.property,
                value: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: value.block,
        })
    }

    pub(super) fn lower_static_property_compound_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: StaticPropertyTarget,
        operator: &str,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(binary) = assignment_binary_op(operator) else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!("assignment operator `{operator}` is not lowered to IR yet"),
            );
            return None;
        };
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "static property compound assignment is missing its right operand",
            );
            return None;
        };
        let old = self.lower_static_property_fetch_to_register(builder, site, target.clone())?;
        let rhs = self.lower_expr_to_register(builder, site.function, old.block, right)?;
        let dst = builder.alloc_register(site.function);
        let arithmetic = builder.emit(
            site.function,
            rhs.block,
            InstructionKind::Binary {
                dst,
                op: binary,
                lhs: Operand::Register(old.register),
                rhs: Operand::Register(rhs.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            rhs.block,
            arithmetic,
            site.expr,
            site.span,
        );
        self.emit_static_property_assign_from_register(builder, site, rhs.block, &target, dst)?;
        Some(LoweredExpr {
            register: dst,
            block: rhs.block,
        })
    }

    pub(super) fn lower_static_property_dim_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: StaticPropertyDimTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "static property array assignment is missing its right operand",
            );
            return None;
        };
        let value = self.lower_expr_to_register(builder, site.function, site.block, right)?;
        self.lower_static_property_dim_assign_operand_to_register(
            builder,
            site,
            value.block,
            target,
            Operand::Register(value.register),
            value.register,
        )
    }

    pub(super) fn lower_static_property_dim_assign_operand_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        block: BlockId,
        target: StaticPropertyDimTarget,
        value: Operand,
        result: RegId,
    ) -> Option<LoweredExpr> {
        let property = StaticPropertyTarget {
            class_name: target.class_name,
            property: target.property,
        };
        let array = self.lower_static_property_fetch_to_register(
            builder,
            LowerSite { block, ..site },
            property.clone(),
        )?;
        let mut current = array.block;
        let local = builder.intern_local(
            site.function,
            format!("__phrust:static-property-dim:{}", site.expr.raw()),
        );
        builder.emit(
            site.function,
            current,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(array.register),
            },
            site.span,
        );
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim.block;
            dims.push(Operand::Register(dim.register));
        }
        let dst = builder.alloc_register(site.function);
        let kind = if target.append {
            InstructionKind::AppendDim {
                dst,
                local,
                dims,
                value,
            }
        } else {
            InstructionKind::AssignDim {
                dst,
                local,
                dims,
                value,
            }
        };
        let assign = builder.emit(site.function, current, kind, site.span);
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            assign,
            site.expr,
            site.span,
        );
        let updated = builder.alloc_register(site.function);
        builder.emit(
            site.function,
            current,
            InstructionKind::LoadLocal {
                dst: updated,
                local,
            },
            site.span,
        );
        self.emit_static_property_assign_from_register(builder, site, current, &property, updated)?;
        Some(LoweredExpr {
            register: result,
            block: current,
        })
    }

    pub(super) fn emit_static_property_assign_from_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        block: BlockId,
        target: &StaticPropertyTarget,
        value: RegId,
    ) -> Option<RegId> {
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            block,
            InstructionKind::AssignStaticProperty {
                dst,
                class_name: target.class_name.clone(),
                property: target.property.clone(),
                value: Operand::Register(value),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            block,
            instruction,
            site.expr,
            site.span,
        );
        Some(dst)
    }

    pub(super) fn lower_reference_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        left: Option<ExprId>,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        if let Some(target) = left.and_then(|left| self.static_property_target(left)) {
            let Some(source) =
                right.and_then(|right| self.variable_local(builder, site.function, right))
            else {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    site.range,
                    "static property by-reference assignment source must be a simple local variable",
                );
                return None;
            };
            let bind = builder.emit(
                site.function,
                site.block,
                InstructionKind::BindReferenceStaticProperty {
                    class_name: target.class_name,
                    property: target.property,
                    source,
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                site.block,
                bind,
                site.expr,
                site.span,
            );
            let dst = builder.alloc_register(site.function);
            let load = builder.emit(
                site.function,
                site.block,
                InstructionKind::LoadLocal { dst, local: source },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                site.block,
                load,
                site.expr,
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: site.block,
            });
        }
        if let Some(left) = left
            && let Some(target) = self.property_dim_target(left)
        {
            return self
                .lower_property_dim_reference_assign_to_register(builder, site, target, right);
        }
        if let Some(left) = left
            && let Some(target) = self.property_assignment_target(left)
        {
            return self.lower_property_reference_assign_to_register(builder, site, target, right);
        }
        if let Some(target) =
            left.and_then(|left| self.variable_local(builder, site.function, left))
            && let Some((receiver, method, args)) =
                right.and_then(|right| self.direct_method_call_parts(right))
        {
            let object =
                self.lower_expr_to_register(builder, site.function, site.block, receiver)?;
            let site = LowerSite {
                block: object.block,
                ..site
            };
            let (operands, current) = self.lower_call_args(builder, site, &args)?;
            let bind = builder.emit(
                site.function,
                current,
                InstructionKind::BindReferenceFromMethodCall {
                    target,
                    object: Operand::Register(object.register),
                    method: normalize_method_name(&method),
                    args: operands,
                },
                site.span,
            );
            self.add_expr_source_map(builder, site.function, current, bind, site.expr, site.span);
            let dst = builder.alloc_register(site.function);
            builder.emit(
                site.function,
                current,
                InstructionKind::LoadLocal { dst, local: target },
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        }
        if let Some(left) = left
            && let Some(target) = self.dim_assignment_target(builder, site.function, left)
            && let Some(source) = right.and_then(|right| self.property_assignment_target(right))
        {
            return self
                .lower_dim_reference_from_property_to_register(builder, site, target, source);
        }
        if let Some(target) =
            left.and_then(|left| self.variable_local(builder, site.function, left))
            && let Some(source_target) =
                right.and_then(|right| self.static_property_dim_target(right))
        {
            if source_target.append {
                self.unsupported(
                    UnsupportedFeature::ArrayElementReference,
                    site.range,
                    "append static property dimension cannot be used as a by-reference source",
                );
                return None;
            }
            let mut current = site.block;
            let mut dims = Vec::with_capacity(source_target.dims.len());
            for dim in source_target.dims {
                let dim_value =
                    self.lower_expr_to_register(builder, site.function, current, dim)?;
                current = dim_value.block;
                dims.push(Operand::Register(dim_value.register));
            }
            let bind = builder.emit(
                site.function,
                current,
                InstructionKind::BindReferenceFromStaticPropertyDim {
                    target,
                    class_name: source_target.class_name,
                    property: source_target.property,
                    dims,
                },
                site.span,
            );
            self.add_expr_source_map(builder, site.function, current, bind, site.expr, site.span);
            let dst = builder.alloc_register(site.function);
            let load = builder.emit(
                site.function,
                current,
                InstructionKind::LoadLocal { dst, local: target },
                site.span,
            );
            self.add_expr_source_map(builder, site.function, current, load, site.expr, site.span);
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        }
        let left_dim = left
            .and_then(|left| self.dim_assignment_target(builder, site.function, left))
            .filter(|target| target.append || !target.dims.is_empty());
        let right_dim = right
            .and_then(|right| self.dim_assignment_target(builder, site.function, right))
            .filter(|target| target.append || !target.dims.is_empty());
        match (left_dim, right_dim) {
            (Some(target), None) if target.append || !target.dims.is_empty() => {
                let Some(source) =
                    right.and_then(|right| self.variable_local(builder, site.function, right))
                else {
                    self.unsupported(
                        UnsupportedFeature::HirStatement,
                        site.range,
                        "array-dimension by-reference assignment source must be a simple local variable",
                    );
                    return None;
                };
                let mut current = site.block;
                let mut dims = Vec::with_capacity(target.dims.len());
                for dim in target.dims {
                    let dim_value =
                        self.lower_expr_to_register(builder, site.function, current, dim)?;
                    current = dim_value.block;
                    dims.push(Operand::Register(dim_value.register));
                }
                let bind = builder.emit(
                    site.function,
                    current,
                    InstructionKind::BindReferenceDim {
                        local: target.local,
                        dims,
                        append: target.append,
                        source,
                    },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    bind,
                    site.expr,
                    site.span,
                );
                let dst = builder.alloc_register(site.function);
                let load = builder.emit(
                    site.function,
                    current,
                    InstructionKind::LoadLocal { dst, local: source },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    load,
                    site.expr,
                    site.span,
                );
                return Some(LoweredExpr {
                    register: dst,
                    block: current,
                });
            }
            (None, Some(source_target))
                if !source_target.append
                    && left
                        .and_then(|left| self.variable_local(builder, site.function, left))
                        .is_some() =>
            {
                let target =
                    left.and_then(|left| self.variable_local(builder, site.function, left))?;
                let mut current = site.block;
                let mut dims = Vec::with_capacity(source_target.dims.len());
                for dim in source_target.dims {
                    let dim_value =
                        self.lower_expr_to_register(builder, site.function, current, dim)?;
                    current = dim_value.block;
                    dims.push(Operand::Register(dim_value.register));
                }
                let bind = builder.emit(
                    site.function,
                    current,
                    InstructionKind::BindReferenceFromDim {
                        target,
                        local: source_target.local,
                        dims,
                    },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    bind,
                    site.expr,
                    site.span,
                );
                let dst = builder.alloc_register(site.function);
                let load = builder.emit(
                    site.function,
                    current,
                    InstructionKind::LoadLocal { dst, local: target },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    load,
                    site.expr,
                    site.span,
                );
                return Some(LoweredExpr {
                    register: dst,
                    block: current,
                });
            }
            (Some(target), Some(source_target)) if !source_target.append => {
                let mut current = site.block;
                let mut source_dims = Vec::with_capacity(source_target.dims.len());
                for dim in source_target.dims {
                    let dim_value =
                        self.lower_expr_to_register(builder, site.function, current, dim)?;
                    current = dim_value.block;
                    source_dims.push(Operand::Register(dim_value.register));
                }
                let source = builder.intern_local(
                    site.function,
                    format!("__phrust:dim-ref-source:{}", site.expr.raw()),
                );
                let bind_source = builder.emit(
                    site.function,
                    current,
                    InstructionKind::BindReferenceFromDim {
                        target: source,
                        local: source_target.local,
                        dims: source_dims,
                    },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    bind_source,
                    site.expr,
                    site.span,
                );
                let mut target_dims = Vec::with_capacity(target.dims.len());
                for dim in target.dims {
                    let dim_value =
                        self.lower_expr_to_register(builder, site.function, current, dim)?;
                    current = dim_value.block;
                    target_dims.push(Operand::Register(dim_value.register));
                }
                let bind_target = builder.emit(
                    site.function,
                    current,
                    InstructionKind::BindReferenceDim {
                        local: target.local,
                        dims: target_dims,
                        append: target.append,
                        source,
                    },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    bind_target,
                    site.expr,
                    site.span,
                );
                let dst = builder.alloc_register(site.function);
                let load = builder.emit(
                    site.function,
                    current,
                    InstructionKind::LoadLocal { dst, local: source },
                    site.span,
                );
                self.add_expr_source_map(
                    builder,
                    site.function,
                    current,
                    load,
                    site.expr,
                    site.span,
                );
                return Some(LoweredExpr {
                    register: dst,
                    block: current,
                });
            }
            (_, Some(source_target)) if source_target.append => {
                self.unsupported(
                    UnsupportedFeature::ArrayElementReference,
                    site.range,
                    "append dimension cannot be used as a by-reference source",
                );
                return None;
            }
            _ => {}
        }
        if left.is_some_and(|left| self.contains_property_fetch_expr(left))
            || right.is_some_and(|right| self.contains_property_fetch_expr(right))
        {
            self.unsupported(
                UnsupportedFeature::ObjectPropertyReference,
                site.range,
                "object-property references are a known gap until property slots participate in reference/COW semantics",
            );
            return None;
        }
        let Some(target) = left.and_then(|left| self.variable_local(builder, site.function, left))
        else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "by-reference assignment target must be a simple local variable in the reference-assignment MVP",
            );
            return None;
        };
        if let Some((name, args)) = right.and_then(|right| self.direct_function_call_parts(right)) {
            let (operands, current) = self.lower_call_args(builder, site, &args)?;
            let bind = builder.emit(
                site.function,
                current,
                InstructionKind::BindReferenceFromCall {
                    target,
                    name,
                    args: operands,
                },
                site.span,
            );
            self.add_expr_source_map(builder, site.function, current, bind, site.expr, site.span);
            let dst = builder.alloc_register(site.function);
            builder.emit(
                site.function,
                current,
                InstructionKind::LoadLocal { dst, local: target },
                site.span,
            );
            return Some(LoweredExpr {
                register: dst,
                block: current,
            });
        }
        let Some(source) =
            right.and_then(|right| self.variable_local(builder, site.function, right))
        else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "by-reference assignment source must be a simple local variable in the reference-assignment MVP",
            );
            return None;
        };
        let bind = builder.emit(
            site.function,
            site.block,
            InstructionKind::BindReference { target, source },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            bind,
            site.expr,
            site.span,
        );
        let dst = builder.alloc_register(site.function);
        let load = builder.emit(
            site.function,
            site.block,
            InstructionKind::LoadLocal { dst, local: target },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            load,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: site.block,
        })
    }

    pub(super) fn lower_property_reference_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: PropertyAssignmentTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(source) =
            right.and_then(|right| self.variable_local(builder, site.function, right))
        else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "property by-reference assignment source must be a simple local variable",
            );
            return None;
        };
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        let bind = builder.emit(
            site.function,
            object.block,
            InstructionKind::BindReferenceProperty {
                object: Operand::Register(object.register),
                property: target.property,
                source,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            object.block,
            bind,
            site.expr,
            site.span,
        );
        let dst = builder.alloc_register(site.function);
        let load = builder.emit(
            site.function,
            object.block,
            InstructionKind::LoadLocal { dst, local: source },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            object.block,
            load,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: object.block,
        })
    }

    pub(super) fn lower_property_dim_reference_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: PropertyDimTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(source) =
            right.and_then(|right| self.variable_local(builder, site.function, right))
        else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "property-dimension by-reference assignment source must be a simple local variable",
            );
            return None;
        };
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
        let mut current = object.block;
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim_value = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim_value.block;
            dims.push(Operand::Register(dim_value.register));
        }
        let bind = builder.emit(
            site.function,
            current,
            InstructionKind::BindReferencePropertyDim {
                object: Operand::Register(object.register),
                property: target.property,
                dims,
                append: target.append,
                source,
            },
            site.span,
        );
        self.add_expr_source_map(builder, site.function, current, bind, site.expr, site.span);
        let dst = builder.alloc_register(site.function);
        let load = builder.emit(
            site.function,
            current,
            InstructionKind::LoadLocal { dst, local: source },
            site.span,
        );
        self.add_expr_source_map(builder, site.function, current, load, site.expr, site.span);
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn lower_dim_reference_from_property_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DimAssignmentTarget,
        source: PropertyAssignmentTarget,
    ) -> Option<LoweredExpr> {
        let object =
            self.lower_expr_to_register(builder, site.function, site.block, source.receiver)?;
        let mut current = object.block;
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim_value = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim_value.block;
            dims.push(Operand::Register(dim_value.register));
        }
        let bind = builder.emit(
            site.function,
            current,
            InstructionKind::BindReferenceDimFromProperty {
                local: target.local,
                dims,
                append: target.append,
                object: Operand::Register(object.register),
                property: source.property,
            },
            site.span,
        );
        self.add_expr_source_map(builder, site.function, current, bind, site.expr, site.span);
        let dst = builder.alloc_register(site.function);
        let load = builder.emit(
            site.function,
            current,
            InstructionKind::LoadLocal {
                dst,
                local: target.local,
            },
            site.span,
        );
        self.add_expr_source_map(builder, site.function, current, load, site.expr, site.span);
        Some(LoweredExpr {
            register: dst,
            block: current,
        })
    }

    pub(super) fn contains_dim_fetch_expr(&self, expr: ExprId) -> bool {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return false;
        };
        self.expr_contains(module, expr, |kind| {
            matches!(kind, HirExprKind::DimFetch { .. })
        })
    }

    pub(super) fn contains_property_fetch_expr(&self, expr: ExprId) -> bool {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return false;
        };
        self.expr_contains(module, expr, |kind| {
            matches!(kind, HirExprKind::PropertyFetch { .. })
        })
    }

    pub(super) fn instanceof_class_name(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Name { resolution } => Some(interface_resolution_name(resolution)),
            HirExprKind::Variable { .. } => None,
            HirExprKind::Unary { operator, expr } if operator == "parenthesized" => {
                expr.and_then(|expr| self.instanceof_class_name(expr))
            }
            _ => None,
        }
    }

    pub(super) fn expr_contains(
        &self,
        module: &php_semantics::hir::HirModule,
        expr: ExprId,
        predicate: impl Copy + Fn(&HirExprKind) -> bool,
    ) -> bool {
        let Some(expression) = module.expressions().get(expr) else {
            return false;
        };
        let kind = expression.kind();
        if predicate(kind) {
            return true;
        }
        match kind {
            HirExprKind::Array { elements } | HirExprKind::List { elements } => elements
                .iter()
                .copied()
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::Closure { body } => body
                .iter()
                .copied()
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::CloneWith { replacements, .. } => replacements
                .iter()
                .copied()
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::ArrayPair { key, value, .. } => [*key, *value]
                .into_iter()
                .flatten()
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::Unary { expr, .. }
            | HirExprKind::ArrowFunction { expr }
            | HirExprKind::Clone { expr }
            | HirExprKind::YieldFrom { expr }
            | HirExprKind::Include { expr, .. }
            | HirExprKind::Eval { expr, .. }
            | HirExprKind::Exit { expr }
            | HirExprKind::Cast { expr, .. }
            | HirExprKind::FirstClassCallable { callee: expr } => {
                expr.is_some_and(|child| self.expr_contains(module, child, predicate))
            }
            HirExprKind::Binary { left, right, .. }
            | HirExprKind::Assign { left, right, .. }
            | HirExprKind::StaticAccess {
                target: left,
                member: right,
            }
            | HirExprKind::DimFetch {
                receiver: left,
                dim: right,
            }
            | HirExprKind::PropertyFetch {
                receiver: left,
                property: right,
                ..
            }
            | HirExprKind::Pipe {
                input: left,
                callable: right,
            } => [*left, *right]
                .into_iter()
                .flatten()
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::Ternary {
                condition,
                if_true,
                if_false,
            } => [*condition, *if_true, *if_false]
                .into_iter()
                .flatten()
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::Call { callee, args } => callee
                .iter()
                .copied()
                .chain(args.iter().map(|arg| arg.value))
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::BuiltinCall { args, .. } => args
                .iter()
                .map(|arg| arg.value)
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => receiver
                .iter()
                .copied()
                .chain(method.iter().copied())
                .chain(args.iter().map(|arg| arg.value))
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::New { class, args } => class
                .iter()
                .copied()
                .chain(args.iter().map(|arg| arg.value))
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::Match { subject, arms } => subject
                .iter()
                .copied()
                .chain(arms.iter().flat_map(|arm| {
                    arm.conditions
                        .iter()
                        .copied()
                        .chain(arm.result.iter().copied())
                }))
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::Yield { key, value } => [*key, *value]
                .into_iter()
                .flatten()
                .any(|child| self.expr_contains(module, child, predicate)),
            HirExprKind::Missing
            | HirExprKind::Literal { .. }
            | HirExprKind::Variable { .. }
            | HirExprKind::Name { .. }
            | HirExprKind::Unlowered { .. } => false,
        }
    }

    pub(super) fn lower_destructuring_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        targets: Vec<(IrConstant, DestructuringTarget)>,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "list assignment is missing its right operand",
            );
            return None;
        };
        let value = self.lower_expr_to_register(builder, site.function, site.block, right)?;
        self.lower_foreach_value_destructure(
            builder,
            site.function,
            value.block,
            value.register,
            targets,
            site.span,
        );
        Some(value)
    }

    pub(super) fn lower_dim_compound_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DimAssignmentTarget,
        operator: &str,
        _left: ExprId,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "array dimension compound assignment is missing its right operand",
            );
            return None;
        };
        let Some(binary) = assignment_binary_op(operator) else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!("assignment operator `{operator}` is not lowered to IR yet"),
            );
            return None;
        };
        let mut current = site.block;
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim_value = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim_value.block;
            dims.push(Operand::Register(dim_value.register));
        }
        let array = builder.alloc_register(site.function);
        let load = builder.emit(
            site.function,
            current,
            InstructionKind::LoadLocal {
                dst: array,
                local: target.local,
            },
            site.span,
        );
        self.add_expr_source_map(builder, site.function, current, load, site.expr, site.span);
        let mut old = array;
        for dim in &dims {
            let fetched = builder.alloc_register(site.function);
            let fetch = builder.emit(
                site.function,
                current,
                InstructionKind::FetchDim {
                    dst: fetched,
                    array: Operand::Register(old),
                    key: *dim,
                    quiet: false,
                },
                site.span,
            );
            self.add_expr_source_map(builder, site.function, current, fetch, site.expr, site.span);
            old = fetched;
        }
        let rhs = self.lower_expr_to_register(builder, site.function, current, right)?;
        let value = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            rhs.block,
            InstructionKind::Binary {
                dst: value,
                op: binary,
                lhs: Operand::Register(old),
                rhs: Operand::Register(rhs.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            rhs.block,
            instruction,
            site.expr,
            site.span,
        );
        let dst = builder.alloc_register(site.function);
        let assign = builder.emit(
            site.function,
            rhs.block,
            InstructionKind::AssignDim {
                dst,
                local: target.local,
                dims,
                value: Operand::Register(value),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            rhs.block,
            assign,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: value,
            block: rhs.block,
        })
    }

    pub(super) fn lower_dim_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DimAssignmentTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "array dimension assignment is missing its right operand",
            );
            return None;
        };
        let value = self.lower_expr_to_register(builder, site.function, site.block, right)?;
        if target.append && !target.dims.is_empty() {
            return self.lower_append_nested_dim_assign_value_to_register(
                builder,
                site,
                value.block,
                target,
                Operand::Register(value.register),
                value.register,
            );
        }
        self.lower_dim_assign_value_to_register(
            builder,
            site,
            value.block,
            target,
            Operand::Register(value.register),
            value.register,
        )
    }

    pub(super) fn lower_append_nested_dim_assign_value_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        block: BlockId,
        target: DimAssignmentTarget,
        value: Operand,
        value_register: RegId,
    ) -> Option<LoweredExpr> {
        let mut current = block;
        let temp_local = builder.intern_local(
            site.function,
            format!("__phrust:append-nested-dim:{}", site.expr.raw()),
        );
        let temp_array = builder.alloc_register(site.function);
        let new_array = builder.emit(
            site.function,
            current,
            InstructionKind::NewArray { dst: temp_array },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            new_array,
            site.expr,
            site.span,
        );
        builder.emit(
            site.function,
            current,
            InstructionKind::StoreLocal {
                local: temp_local,
                src: Operand::Register(temp_array),
            },
            site.span,
        );
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim.block;
            dims.push(Operand::Register(dim.register));
        }
        let nested_value = builder.alloc_register(site.function);
        let assign_nested = builder.emit(
            site.function,
            current,
            InstructionKind::AssignDim {
                dst: nested_value,
                local: temp_local,
                dims,
                value,
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            assign_nested,
            site.expr,
            site.span,
        );
        let append_value = builder.alloc_register(site.function);
        builder.emit(
            site.function,
            current,
            InstructionKind::LoadLocal {
                dst: append_value,
                local: temp_local,
            },
            site.span,
        );
        let append_dst = builder.alloc_register(site.function);
        let append = builder.emit(
            site.function,
            current,
            InstructionKind::AppendDim {
                dst: append_dst,
                local: target.local,
                dims: Vec::new(),
                value: Operand::Register(append_value),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            append,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: value_register,
            block: current,
        })
    }

    pub(super) fn lower_comparison_assignment_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        compare_operator: &str,
        compare_left: ExprId,
        target: LocalId,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "comparison assignment is missing its right operand",
            );
            return None;
        };
        let Some(op) = compare_op(compare_operator) else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!("comparison operator `{compare_operator}` is not lowered to IR yet"),
            );
            return None;
        };
        let lhs = self.lower_expr_to_register(builder, site.function, site.block, compare_left)?;
        let assigned = self.lower_expr_to_register(builder, site.function, lhs.block, right)?;
        let store = builder.emit(
            site.function,
            assigned.block,
            InstructionKind::StoreLocal {
                local: target,
                src: Operand::Register(assigned.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            assigned.block,
            store,
            site.expr,
            site.span,
        );
        let dst = builder.alloc_register(site.function);
        let compare = builder.emit(
            site.function,
            assigned.block,
            InstructionKind::Compare {
                dst,
                op,
                lhs: Operand::Register(lhs.register),
                rhs: Operand::Register(assigned.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            assigned.block,
            compare,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: assigned.block,
        })
    }

    pub(super) fn lower_unary_assignment_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        unary_operator: &str,
        target: LocalId,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "unary assignment is missing its right operand",
            );
            return None;
        };
        let Some(op) = unary_op(unary_operator) else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                format!("unary operator `{unary_operator}` is not lowered to IR yet"),
            );
            return None;
        };
        let assigned = self.lower_expr_to_register(builder, site.function, site.block, right)?;
        let store = builder.emit(
            site.function,
            assigned.block,
            InstructionKind::StoreLocal {
                local: target,
                src: Operand::Register(assigned.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            assigned.block,
            store,
            site.expr,
            site.span,
        );
        let dst = builder.alloc_register(site.function);
        let unary = builder.emit(
            site.function,
            assigned.block,
            InstructionKind::Unary {
                dst,
                op,
                src: Operand::Register(assigned.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            assigned.block,
            unary,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: dst,
            block: assigned.block,
        })
    }

    pub(super) fn lower_logical_assignment_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        logical_operator: &str,
        logical_left: ExprId,
        logical_right: ExprId,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let left_value =
            self.lower_expr_to_register(builder, site.function, site.block, logical_left)?;
        let dst = builder.alloc_register(site.function);
        let false_block = builder.append_block(site.function);
        let true_block = builder.append_block(site.function);
        let after_block = builder.append_block(site.function);

        match logical_operator {
            "&&" | "and" => {
                builder.terminate_jump_if(
                    site.function,
                    left_value.block,
                    Operand::Register(left_value.register),
                    true_block,
                    false_block,
                    site.span,
                );
                self.emit_bool_move(builder, site.function, false_block, dst, false, site.span);
                self.jump_if_open(builder, site.function, false_block, after_block, site.span);

                let right_value = self.lower_assign_to_register(
                    builder,
                    LowerSite {
                        block: true_block,
                        ..site
                    },
                    "=",
                    Some(logical_right),
                    right,
                )?;
                self.emit_bool_cast(
                    builder,
                    site.function,
                    right_value.block,
                    dst,
                    right_value.register,
                    site.span,
                );
                self.jump_if_open(
                    builder,
                    site.function,
                    right_value.block,
                    after_block,
                    site.span,
                );
            }
            "||" | "or" => {
                builder.terminate_jump_if(
                    site.function,
                    left_value.block,
                    Operand::Register(left_value.register),
                    true_block,
                    false_block,
                    site.span,
                );
                let right_value = self.lower_assign_to_register(
                    builder,
                    LowerSite {
                        block: false_block,
                        ..site
                    },
                    "=",
                    Some(logical_right),
                    right,
                )?;
                self.emit_bool_cast(
                    builder,
                    site.function,
                    right_value.block,
                    dst,
                    right_value.register,
                    site.span,
                );
                self.jump_if_open(
                    builder,
                    site.function,
                    right_value.block,
                    after_block,
                    site.span,
                );
                self.emit_bool_move(builder, site.function, true_block, dst, true, site.span);
                self.jump_if_open(builder, site.function, true_block, after_block, site.span);
            }
            _ => return None,
        }

        Some(LoweredExpr {
            register: dst,
            block: after_block,
        })
    }

    pub(super) fn lower_dim_coalesce_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DimAssignmentTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "array dimension null coalescing assignment is missing its right operand",
            );
            return None;
        };

        let mut current = site.block;
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim_value = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim_value.block;
            dims.push(Operand::Register(dim_value.register));
        }

        let dst = builder.alloc_register(site.function);
        let present = builder.alloc_register(site.function);
        let isset = builder.emit(
            site.function,
            current,
            InstructionKind::IssetDim {
                dst: present,
                local: target.local,
                dims: dims.clone(),
            },
            site.span,
        );
        self.add_expr_source_map(builder, site.function, current, isset, site.expr, site.span);

        let existing_block = builder.append_block(site.function);
        let assign_block = builder.append_block(site.function);
        let after_block = builder.append_block(site.function);
        builder.terminate_jump_if(
            site.function,
            current,
            Operand::Register(present),
            existing_block,
            assign_block,
            site.span,
        );

        let mut fetched = builder.alloc_register(site.function);
        builder.emit(
            site.function,
            existing_block,
            InstructionKind::LoadLocal {
                dst: fetched,
                local: target.local,
            },
            site.span,
        );
        for dim in dims.iter().cloned() {
            let next = builder.alloc_register(site.function);
            builder.emit(
                site.function,
                existing_block,
                InstructionKind::FetchDim {
                    dst: next,
                    array: Operand::Register(fetched),
                    key: dim,
                    quiet: false,
                },
                site.span,
            );
            fetched = next;
        }
        builder.emit(
            site.function,
            existing_block,
            InstructionKind::Move {
                dst,
                src: Operand::Register(fetched),
            },
            site.span,
        );
        self.jump_if_open(
            builder,
            site.function,
            existing_block,
            after_block,
            site.span,
        );

        let value = self.lower_expr_to_register(builder, site.function, assign_block, right)?;
        let assign_dst = builder.alloc_register(site.function);
        let assign = builder.emit(
            site.function,
            value.block,
            InstructionKind::AssignDim {
                dst: assign_dst,
                local: target.local,
                dims,
                value: Operand::Register(value.register),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            value.block,
            assign,
            site.expr,
            site.span,
        );
        builder.emit(
            site.function,
            value.block,
            InstructionKind::Move {
                dst,
                src: Operand::Register(value.register),
            },
            site.span,
        );
        self.jump_if_open(builder, site.function, value.block, after_block, site.span);

        Some(LoweredExpr {
            register: dst,
            block: after_block,
        })
    }

    pub(super) fn lower_dim_assign_value_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        block: BlockId,
        target: DimAssignmentTarget,
        value: Operand,
        result: RegId,
    ) -> Option<LoweredExpr> {
        let mut current = block;
        let mut dims = Vec::with_capacity(target.dims.len());
        for dim in target.dims {
            let dim_value = self.lower_expr_to_register(builder, site.function, current, dim)?;
            current = dim_value.block;
            dims.push(Operand::Register(dim_value.register));
        }
        let dst = builder.alloc_register(site.function);
        let kind = if target.append {
            InstructionKind::AppendDim {
                dst,
                local: target.local,
                dims,
                value,
            }
        } else {
            InstructionKind::AssignDim {
                dst,
                local: target.local,
                dims,
                value,
            }
        };
        let instruction = builder.emit(site.function, current, kind, site.span);
        self.add_expr_source_map(
            builder,
            site.function,
            current,
            instruction,
            site.expr,
            site.span,
        );
        Some(LoweredExpr {
            register: result,
            block: current,
        })
    }

    pub(super) fn lower_inc_dec_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        inner: Option<ExprId>,
        operator: &str,
    ) -> Option<LoweredExpr> {
        let Some(inner) = inner else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "increment/decrement expression is missing its operand",
            );
            return None;
        };
        if let Some(target) = self.dim_assignment_target(builder, site.function, inner)
            && !target.append
            && !target.dims.is_empty()
        {
            let old = self.lower_expr_to_register(builder, site.function, site.block, inner)?;
            let one = builder.intern_constant(IrConstant::Int(1));
            let one_reg = builder.alloc_register(site.function);
            let load_one =
                builder.emit_load_const(site.function, old.block, one_reg, one, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                old.block,
                load_one,
                site.expr,
                site.span,
            );
            let new = builder.alloc_register(site.function);
            let op = if operator == "++" {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            let arithmetic = builder.emit(
                site.function,
                old.block,
                InstructionKind::Binary {
                    dst: new,
                    op,
                    lhs: Operand::Register(old.register),
                    rhs: Operand::Register(one_reg),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                old.block,
                arithmetic,
                site.expr,
                site.span,
            );
            let mut current = old.block;
            let mut dims = Vec::with_capacity(target.dims.len());
            for dim in target.dims {
                let dim_value =
                    self.lower_expr_to_register(builder, site.function, current, dim)?;
                current = dim_value.block;
                dims.push(Operand::Register(dim_value.register));
            }
            let assign_result = builder.alloc_register(site.function);
            let assign = builder.emit(
                site.function,
                current,
                InstructionKind::AssignDim {
                    dst: assign_result,
                    local: target.local,
                    dims,
                    value: Operand::Register(new),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                current,
                assign,
                site.expr,
                site.span,
            );

            let inner_range = self.span_for(SourceMappedId::from(inner));
            let is_prefix = inner_range.end() == site.range.end();
            return Some(LoweredExpr {
                register: if is_prefix { new } else { old.register },
                block: current,
            });
        }
        if let Some(target) = self.property_assignment_target(inner) {
            let object =
                self.lower_expr_to_register(builder, site.function, site.block, target.receiver)?;
            let old = builder.alloc_register(site.function);
            let fetch = builder.emit(
                site.function,
                object.block,
                InstructionKind::FetchProperty {
                    dst: old,
                    object: Operand::Register(object.register),
                    property: target.property.clone(),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                object.block,
                fetch,
                site.expr,
                site.span,
            );
            let one = builder.intern_constant(IrConstant::Int(1));
            let one_reg = builder.alloc_register(site.function);
            let load_one =
                builder.emit_load_const(site.function, object.block, one_reg, one, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                object.block,
                load_one,
                site.expr,
                site.span,
            );
            let new = builder.alloc_register(site.function);
            let op = if operator == "++" {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            let arithmetic = builder.emit(
                site.function,
                object.block,
                InstructionKind::Binary {
                    dst: new,
                    op,
                    lhs: Operand::Register(old),
                    rhs: Operand::Register(one_reg),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                object.block,
                arithmetic,
                site.expr,
                site.span,
            );
            let assign_result = builder.alloc_register(site.function);
            let assign = builder.emit(
                site.function,
                object.block,
                InstructionKind::AssignProperty {
                    dst: assign_result,
                    object: Operand::Register(object.register),
                    property: target.property,
                    value: Operand::Register(new),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                object.block,
                assign,
                site.expr,
                site.span,
            );

            let inner_range = self.span_for(SourceMappedId::from(inner));
            let is_prefix = inner_range.end() == site.range.end();
            return Some(LoweredExpr {
                register: if is_prefix { new } else { old },
                block: object.block,
            });
        }
        if let Some(target) = self.static_property_dim_target(inner)
            && !target.append
            && !target.dims.is_empty()
        {
            let old = self.lower_expr_to_register(builder, site.function, site.block, inner)?;
            let one = builder.intern_constant(IrConstant::Int(1));
            let one_reg = builder.alloc_register(site.function);
            let load_one =
                builder.emit_load_const(site.function, old.block, one_reg, one, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                old.block,
                load_one,
                site.expr,
                site.span,
            );
            let new = builder.alloc_register(site.function);
            let op = if operator == "++" {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            let arithmetic = builder.emit(
                site.function,
                old.block,
                InstructionKind::Binary {
                    dst: new,
                    op,
                    lhs: Operand::Register(old.register),
                    rhs: Operand::Register(one_reg),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                old.block,
                arithmetic,
                site.expr,
                site.span,
            );

            let inner_range = self.span_for(SourceMappedId::from(inner));
            let is_prefix = inner_range.end() == site.range.end();
            return self.lower_static_property_dim_assign_operand_to_register(
                builder,
                site,
                old.block,
                target,
                Operand::Register(new),
                if is_prefix { new } else { old.register },
            );
        }
        if let Some(target) = self.static_property_target(inner) {
            let old =
                self.lower_static_property_fetch_to_register(builder, site, target.clone())?;
            let one = builder.intern_constant(IrConstant::Int(1));
            let one_reg = builder.alloc_register(site.function);
            let load_one =
                builder.emit_load_const(site.function, old.block, one_reg, one, site.span);
            self.add_expr_source_map(
                builder,
                site.function,
                old.block,
                load_one,
                site.expr,
                site.span,
            );
            let new = builder.alloc_register(site.function);
            let op = if operator == "++" {
                BinaryOp::Add
            } else {
                BinaryOp::Sub
            };
            let arithmetic = builder.emit(
                site.function,
                old.block,
                InstructionKind::Binary {
                    dst: new,
                    op,
                    lhs: Operand::Register(old.register),
                    rhs: Operand::Register(one_reg),
                },
                site.span,
            );
            self.add_expr_source_map(
                builder,
                site.function,
                old.block,
                arithmetic,
                site.expr,
                site.span,
            );
            self.emit_static_property_assign_from_register(builder, site, old.block, &target, new)?;

            let inner_range = self.span_for(SourceMappedId::from(inner));
            let is_prefix = inner_range.end() == site.range.end();
            return Some(LoweredExpr {
                register: if is_prefix { new } else { old.register },
                block: old.block,
            });
        }
        let Some(local) = self.variable_local(builder, site.function, inner) else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "only simple variable increment/decrement is lowered to IR in local-variable",
            );
            return None;
        };
        let old = builder.alloc_register(site.function);
        let load = builder.emit(
            site.function,
            site.block,
            InstructionKind::LoadLocal { dst: old, local },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            load,
            site.expr,
            site.span,
        );
        let one = builder.intern_constant(IrConstant::Int(1));
        let one_reg = builder.alloc_register(site.function);
        let load_one = builder.emit_load_const(site.function, site.block, one_reg, one, site.span);
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            load_one,
            site.expr,
            site.span,
        );
        let new = builder.alloc_register(site.function);
        let op = if operator == "++" {
            BinaryOp::Add
        } else {
            BinaryOp::Sub
        };
        let arithmetic = builder.emit(
            site.function,
            site.block,
            InstructionKind::Binary {
                dst: new,
                op,
                lhs: Operand::Register(old),
                rhs: Operand::Register(one_reg),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            arithmetic,
            site.expr,
            site.span,
        );
        let store = builder.emit(
            site.function,
            site.block,
            InstructionKind::StoreLocal {
                local,
                src: Operand::Register(new),
            },
            site.span,
        );
        self.add_expr_source_map(
            builder,
            site.function,
            site.block,
            store,
            site.expr,
            site.span,
        );

        let inner_range = self.span_for(SourceMappedId::from(inner));
        let is_prefix = inner_range.end() == site.range.end();
        Some(LoweredExpr {
            register: if is_prefix { new } else { old },
            block: site.block,
        })
    }

    pub(super) fn variable_local(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        expr: ExprId,
    ) -> Option<crate::ids::LocalId> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Variable { name } => {
                Some(builder.intern_local(function, local_name(name)))
            }
            _ => None,
        }
    }

    pub(super) fn comparison_assignment_target(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        expr: ExprId,
    ) -> Option<(String, ExprId, LocalId)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let (operator, compare_left, compare_right) = match expression.kind() {
            HirExprKind::Binary {
                operator,
                left,
                right,
            } if compare_op(operator).is_some() => (operator.clone(), *left, *right),
            _ => return None,
        };
        let compare_left = compare_left?;
        let target = self.variable_local(builder, function, compare_right?)?;
        Some((operator, compare_left, target))
    }

    pub(super) fn logical_assignment_target(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        expr: ExprId,
    ) -> Option<(String, ExprId, ExprId)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let (operator, left, right) = match expression.kind() {
            HirExprKind::Binary {
                operator,
                left,
                right,
            } if matches!(operator.as_str(), "&&" | "and" | "||" | "or") => {
                (operator.clone(), *left, *right)
            }
            _ => return None,
        };
        let left = left?;
        let right = right?;
        if self.variable_local(builder, function, right).is_none()
            && self
                .unary_assignment_target(builder, function, right)
                .is_none()
        {
            return None;
        }
        Some((operator, left, right))
    }

    pub(super) fn coalesce_assignment_target(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        expr: ExprId,
    ) -> Option<(ExprId, LocalId)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let (left, right) = match expression.kind() {
            HirExprKind::Binary {
                operator,
                left,
                right,
            } if operator == "??" => (*left, *right),
            _ => return None,
        };
        let target = self.variable_local(builder, function, right?)?;
        Some((left?, target))
    }

    pub(super) fn unary_assignment_target(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        expr: ExprId,
    ) -> Option<(String, LocalId)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let (operator, inner) = match expression.kind() {
            HirExprKind::Unary { operator, expr } if operator == "!" => (operator.clone(), *expr),
            _ => return None,
        };
        let target = self.variable_local(builder, function, inner?)?;
        Some((operator, target))
    }

    pub(super) fn static_function_call_name(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Name { resolution } => {
                if let Some(display_name) =
                    self.imported_function_display_name(module, expr, resolution)
                {
                    return Some(display_name);
                }
                let source = display_class_name(resolution.source());
                let normalized_source = normalize_class_name(&source);
                if matches!(normalized_source.as_str(), "self" | "static" | "parent") {
                    return Some(normalized_source);
                }
                self.class_name_constant_value(expr).or_else(|| {
                    resolution
                        .resolved()
                        .or_else(|| resolution.fallback())
                        .or_else(|| Some(resolution.source()))
                        .map(ToOwned::to_owned)
                })
            }
            _ => None,
        }
    }

    pub(super) fn imported_function_display_name(
        &self,
        module: &HirModule,
        expr: ExprId,
        resolution: &HirNameResolution,
    ) -> Option<String> {
        if resolution.source().starts_with('\\') {
            return None;
        }
        let source = resolution.source().trim_start_matches('\\');
        let range = self.span_for(SourceMappedId::from(expr));
        let namespace = module
            .namespaces()
            .values()
            .filter(|namespace| range_contains(namespace.span(), range))
            .min_by_key(|namespace| {
                namespace
                    .span()
                    .end()
                    .to_usize()
                    .saturating_sub(namespace.span().start().to_usize())
            });
        let first_part = source.split('\\').next().unwrap_or_default();
        let import = namespace
            .and_then(|namespace| namespace.imports().lookup(ImportKind::Function, first_part))?;
        let mut parts = import
            .name()
            .parts()
            .iter()
            .map(|part| part.original().to_owned())
            .collect::<Vec<_>>();
        parts.extend(
            source
                .split('\\')
                .skip(1)
                .filter(|part| !part.is_empty())
                .map(ToOwned::to_owned),
        );
        Some(parts.join("\\"))
    }

    pub(super) fn direct_function_call_parts(
        &self,
        expr: ExprId,
    ) -> Option<(String, Vec<HirCallArg>)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let HirExprKind::Call {
            callee: Some(callee),
            args,
        } = expression.kind()
        else {
            return None;
        };
        let name = self.static_function_call_name(*callee)?;
        Some((normalize_function_name(&name), args.clone()))
    }

    pub(super) fn direct_method_call_parts(
        &self,
        expr: ExprId,
    ) -> Option<(ExprId, String, Vec<HirCallArg>)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let HirExprKind::MethodCall {
            receiver,
            method,
            args,
            nullsafe: false,
        } = expression.kind()
        else {
            return None;
        };
        if self.method_call_uses_dynamic_member(expr) {
            return None;
        }
        let target = self.method_call_target(*receiver, *method)?;
        Some((target.receiver, target.method, args.clone()))
    }

    pub(super) fn static_class_name(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Name { resolution } => {
                let source = display_class_name(resolution.source());
                let normalized_source = normalize_class_name(&source);
                if matches!(normalized_source.as_str(), "self" | "static" | "parent") {
                    return Some(normalized_source);
                }
                resolution
                    .resolved()
                    .or_else(|| resolution.fallback())
                    .or_else(|| Some(resolution.source()))
                    .map(ToOwned::to_owned)
            }
            _ => None,
        }
    }

    pub(super) fn static_class_display_name(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Name { resolution } => self
                .imported_class_display_name(module, resolution)
                .or_else(|| self.class_name_constant_value(expr))
                .or_else(|| {
                    self.declared_class_display_name(
                        resolution
                            .resolved()
                            .or_else(|| resolution.fallback())
                            .unwrap_or_else(|| resolution.source()),
                    )
                })
                .or_else(|| Some(display_class_name(resolution.source()))),
            _ => None,
        }
    }

    pub(super) fn imported_class_display_name(
        &self,
        module: &HirModule,
        resolution: &HirNameResolution,
    ) -> Option<String> {
        let canonical =
            normalize_class_name(resolution.resolved().or_else(|| resolution.fallback())?);
        for namespace in module.namespaces().values() {
            for import in namespace.imports().entries() {
                if import.kind() != ImportKind::ClassLike
                    || import.name().canonical(NameKind::ClassLike) != canonical
                {
                    continue;
                }
                return Some(
                    import
                        .name()
                        .parts()
                        .iter()
                        .map(|part| part.original())
                        .collect::<Vec<_>>()
                        .join("\\"),
                );
            }
        }
        None
    }

    pub(super) fn declared_class_display_name(&self, class_name: &str) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let normalized = normalize_class_name(class_name);
        module.class_likes().iter().find_map(|(_, class_like)| {
            (class_like_normalized_name(class_like, &self.options.source_path)? == normalized)
                .then(|| class_like_display_name(class_like, class_name))
        })
    }

    pub(super) fn static_property_name(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Literal { text } => Some(local_name(text).to_owned()),
            HirExprKind::Name { resolution } => Some(local_name(resolution.source()).to_owned()),
            _ => None,
        }
    }

    pub(super) fn static_property_member_name(&self, expr: ExprId) -> Option<String> {
        if let Some(name) = self.static_property_name(expr) {
            return Some(name);
        }
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Variable { name } => Some(local_name(name).to_owned()),
            _ => None,
        }
    }

    pub(super) fn static_property_display_name(&self, expr: ExprId) -> Option<String> {
        let range = self.span_for(SourceMappedId::from(expr));
        if let Some(source) = self.source_text.slice(range) {
            let source = source.trim();
            if !source.is_empty()
                && !source.starts_with('$')
                && source
                    .bytes()
                    .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
            {
                return Some(local_name(source).to_owned());
            }
        }
        self.static_property_name(expr)
    }

    pub(super) fn static_property_target(&self, expr: ExprId) -> Option<StaticPropertyTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let HirExprKind::StaticAccess { target, member } = expression.kind() else {
            return None;
        };
        let member_expr = (*member)?;
        if !self.static_member_is_property(member_expr) {
            return None;
        }
        Some(StaticPropertyTarget {
            class_name: self.static_class_name((*target)?)?,
            property: self.static_property_member_name(member_expr)?,
        })
    }

    pub(super) fn dynamic_static_property_target(
        &self,
        expr: ExprId,
    ) -> Option<DynamicStaticPropertyTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let HirExprKind::StaticAccess { target, member } = expression.kind() else {
            return None;
        };
        let target_expr = (*target)?;
        if self.static_class_name(target_expr).is_some() {
            return None;
        }
        let member_expr = (*member)?;
        if !self.static_member_is_property(member_expr) {
            return None;
        }
        Some(DynamicStaticPropertyTarget {
            class_name: target_expr,
            property: self.static_property_member_name(member_expr)?,
        })
    }

    pub(super) fn static_property_dim_target(
        &self,
        expr: ExprId,
    ) -> Option<StaticPropertyDimTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::DimFetch { receiver, dim } => {
                let receiver = (*receiver)?;
                let mut target = if let Some(property) = self.static_property_target(receiver) {
                    StaticPropertyDimTarget {
                        class_name: property.class_name,
                        property: property.property,
                        dims: Vec::new(),
                        append: false,
                    }
                } else {
                    self.static_property_dim_target(receiver)?
                };
                if target.append {
                    return None;
                }
                if let Some(dim) = dim {
                    target.dims.push(*dim);
                } else {
                    target.append = true;
                }
                Some(target)
            }
            _ => None,
        }
    }

    pub(super) fn class_constant_dim_target(&self, expr: ExprId) -> Option<ClassConstantDimTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::DimFetch { receiver, dim } => {
                let receiver = (*receiver)?;
                let mut target = if let Some(constant) = self.class_constant_target(receiver) {
                    ClassConstantDimTarget {
                        class_name: constant.class_name,
                        display_class_name: constant.display_class_name,
                        constant: constant.constant,
                        dims: Vec::new(),
                        append: false,
                    }
                } else {
                    self.class_constant_dim_target(receiver)?
                };
                if target.append {
                    return None;
                }
                if let Some(dim) = dim {
                    target.dims.push(*dim);
                } else {
                    target.append = true;
                }
                Some(target)
            }
            _ => None,
        }
    }

    pub(super) fn static_property_test_target(&self, expr: ExprId) -> Option<StaticPropertyTarget> {
        if let Some(target) = self.static_property_target(expr) {
            return Some(target);
        }
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let HirExprKind::StaticAccess { target, member } = expression.kind() else {
            return None;
        };
        let source = self
            .source_text
            .slice(self.span_for(SourceMappedId::from(expr)))?;
        if !source.contains("::$") {
            return None;
        }
        let member_expr = (*member)?;
        Some(StaticPropertyTarget {
            class_name: self.static_class_name((*target)?)?,
            property: self.static_property_member_name(member_expr)?,
        })
    }

    pub(super) fn class_constant_target(&self, expr: ExprId) -> Option<ClassConstantTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let HirExprKind::StaticAccess { target, member } = expression.kind() else {
            return None;
        };
        let member_expr = (*member)?;
        if self.static_member_is_property(member_expr) {
            return None;
        }
        Some(ClassConstantTarget {
            class_name: self.static_class_name((*target)?)?,
            display_class_name: self.static_class_display_name((*target)?),
            constant: self.static_property_name(member_expr)?,
            target_expr: (*target)?,
        })
    }

    pub(super) fn class_name_constant_value(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let HirExprKind::Name { resolution } = expression.kind() else {
            return None;
        };
        let source = display_class_name(resolution.source());
        let source_without_namespace = source
            .strip_prefix("namespace\\")
            .or_else(|| source.strip_prefix("NAMESPACE\\"))
            .unwrap_or(&source);
        if resolution.source().starts_with('\\') {
            return Some(source_without_namespace.to_owned());
        }

        let range = self.span_for(SourceMappedId::from(expr));
        let namespace = module
            .namespaces()
            .values()
            .filter(|namespace| range_contains(namespace.span(), range))
            .min_by_key(|namespace| {
                namespace
                    .span()
                    .end()
                    .to_usize()
                    .saturating_sub(namespace.span().start().to_usize())
            });
        if let Some(display_name) = resolved_class_like_display_name(module, resolution) {
            return Some(display_name);
        }
        let first_part = source_without_namespace
            .split('\\')
            .next()
            .unwrap_or_default();
        if let Some(import) = namespace.and_then(|namespace| {
            namespace
                .imports()
                .lookup(ImportKind::ClassLike, first_part)
        }) {
            let mut parts = import
                .name()
                .parts()
                .iter()
                .map(|part| part.original().to_owned())
                .collect::<Vec<_>>();
            parts.extend(
                source_without_namespace
                    .split('\\')
                    .skip(1)
                    .filter(|part| !part.is_empty())
                    .map(ToOwned::to_owned),
            );
            return Some(parts.join("\\"));
        }

        if let Some(namespace_name) = namespace.and_then(|namespace| namespace.name()) {
            if source_without_namespace.is_empty() {
                return Some(namespace_name.text().to_owned());
            }
            return Some(format!(
                "{}\\{}",
                namespace_name.text(),
                source_without_namespace
            ));
        }

        Some(source_without_namespace.to_owned())
    }

    pub(super) fn object_class_name_target(&self, expr: ExprId) -> Option<ObjectClassNameTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let HirExprKind::StaticAccess { target, member } = expression.kind() else {
            return None;
        };
        let member_expr = (*member)?;
        if self.static_member_is_property(member_expr) {
            return None;
        }
        if !self
            .static_property_name(member_expr)?
            .eq_ignore_ascii_case("class")
        {
            return None;
        }
        let object = (*target)?;
        if self.static_class_name(object).is_some() {
            return None;
        }
        Some(ObjectClassNameTarget { object })
    }

    pub(super) fn static_member_is_property(&self, expr: ExprId) -> bool {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return false;
        };
        let Some(expression) = module.expressions().get(expr) else {
            return false;
        };
        match expression.kind() {
            HirExprKind::Variable { .. } => true,
            HirExprKind::Literal { text } => text.starts_with('$'),
            HirExprKind::Name { resolution } => resolution.source().starts_with('$'),
            _ => false,
        }
    }

    pub(super) fn method_call_target(
        &self,
        receiver: Option<ExprId>,
        method: Option<ExprId>,
    ) -> Option<MethodCallTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        if let (Some(receiver), Some(method)) = (receiver, method) {
            return Some(MethodCallTarget {
                receiver,
                method: self.static_property_name(method)?,
            });
        }
        let method = method?;
        let expression = module.expressions().get(method)?;
        match expression.kind() {
            HirExprKind::PropertyFetch {
                receiver: Some(receiver),
                property: Some(property),
                ..
            } => Some(MethodCallTarget {
                receiver: *receiver,
                method: self.static_property_name(*property)?,
            }),
            _ => None,
        }
    }

    pub(super) fn dynamic_method_call_target(
        &self,
        receiver: Option<ExprId>,
        method: Option<ExprId>,
    ) -> Option<DynamicMethodCallTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        if let (Some(receiver), Some(method)) = (receiver, method) {
            return Some(DynamicMethodCallTarget { receiver, method });
        }
        let method = method?;
        let expression = module.expressions().get(method)?;
        match expression.kind() {
            HirExprKind::PropertyFetch {
                receiver: Some(receiver),
                property: Some(property),
                ..
            } => Some(DynamicMethodCallTarget {
                receiver: *receiver,
                method: *property,
            }),
            _ => None,
        }
    }

    pub(super) fn static_method_call_target(
        &mut self,
        expr: ExprId,
    ) -> Option<StaticMethodCallTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        let HirExprKind::StaticAccess { target, member } = expression.kind() else {
            return None;
        };
        let (Some(target), Some(member)) = (*target, *member) else {
            self.unsupported(
                UnsupportedFeature::StaticProperty,
                self.span_for(SourceMappedId::from(expr)),
                "static access target or member is missing in the method-runtime object MVP",
            );
            return None;
        };
        let class_name = self.static_class_name(target)?;
        let display_class_name = self.static_class_display_name(target);
        Some(StaticMethodCallTarget {
            class_name,
            display_class_name,
            method: self.static_property_name(member)?,
        })
    }

    pub(super) fn is_static_access_expr(&self, expr: ExprId) -> bool {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return false;
        };
        module
            .expressions()
            .get(expr)
            .is_some_and(|expression| matches!(expression.kind(), HirExprKind::StaticAccess { .. }))
    }

    pub(super) fn parenthesized_clone_operand(
        &self,
        expr: Option<ExprId>,
        replacements: &[ExprId],
    ) -> Option<ExprId> {
        if !replacements.is_empty() {
            return None;
        }
        let expr = expr?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        if let HirExprKind::Call { callee: None, args } = expression.kind()
            && let [object] = args.as_slice()
        {
            return Some(object.value);
        }
        None
    }

    pub(super) fn clone_with_operands(
        &self,
        expr: Option<ExprId>,
        replacements: &[ExprId],
    ) -> Option<(ExprId, ExprId)> {
        if let Some(object) = expr
            && replacements.len() == 1
        {
            return Some((object, replacements[0]));
        }
        let expr = expr?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        if let HirExprKind::Call { callee: None, args } = expression.kind()
            && let [object, replacements] = args.as_slice()
        {
            return Some((object.value, replacements.value));
        }
        None
    }

    pub(super) fn property_assignment_target(
        &self,
        expr: ExprId,
    ) -> Option<PropertyAssignmentTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::PropertyFetch {
                receiver: Some(receiver),
                property: Some(property),
                nullsafe: false,
            } if !self.property_fetch_uses_dynamic_member(expr) => Some(PropertyAssignmentTarget {
                receiver: *receiver,
                property: self.static_property_name(*property)?,
            }),
            _ => None,
        }
    }

    pub(super) fn dynamic_property_target(&self, expr: ExprId) -> Option<DynamicPropertyTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::PropertyFetch {
                receiver: Some(receiver),
                property: Some(property),
                nullsafe: false,
            } if self.property_fetch_uses_dynamic_member(expr)
                || self.static_property_name(*property).is_none() =>
            {
                Some(DynamicPropertyTarget {
                    receiver: *receiver,
                    property: *property,
                })
            }
            _ => None,
        }
    }

    pub(super) fn dynamic_property_dim_target(
        &self,
        expr: ExprId,
    ) -> Option<DynamicPropertyDimTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::DimFetch { receiver, dim } => {
                let receiver = (*receiver)?;
                let mut target = if let Some(property) = self.dynamic_property_target(receiver) {
                    DynamicPropertyDimTarget {
                        receiver: property.receiver,
                        property: property.property,
                        dims: Vec::new(),
                        append: false,
                    }
                } else {
                    self.dynamic_property_dim_target(receiver)?
                };
                if target.append {
                    return None;
                }
                if let Some(dim) = dim {
                    target.dims.push(*dim);
                } else {
                    target.append = true;
                }
                Some(target)
            }
            _ => None,
        }
    }

    pub(super) fn property_fetch_uses_dynamic_member(&self, expr: ExprId) -> bool {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return false;
        };
        let Some(expression) = module.expressions().get(expr) else {
            return false;
        };
        let HirExprKind::PropertyFetch {
            property: Some(property_id),
            ..
        } = expression.kind()
        else {
            return false;
        };
        let Some(property) = module.expressions().get(*property_id) else {
            return false;
        };
        match property.kind() {
            HirExprKind::Variable { .. } => true,
            HirExprKind::Literal { text } => text.starts_with('$'),
            HirExprKind::Name { resolution } => resolution.source().starts_with('$'),
            _ => self.static_property_name(*property_id).is_none(),
        }
    }

    pub(super) fn method_call_uses_dynamic_member(&self, expr: ExprId) -> bool {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return false;
        };
        let Some(expression) = module.expressions().get(expr) else {
            return false;
        };
        let HirExprKind::MethodCall { method, .. } = expression.kind() else {
            return false;
        };
        let Some(method) = *method else {
            return false;
        };
        if self.property_fetch_uses_dynamic_member(method) {
            return true;
        }
        let Some(method_expr) = module.expressions().get(method) else {
            return false;
        };
        match method_expr.kind() {
            HirExprKind::Variable { .. } => true,
            HirExprKind::Literal { text } => text.starts_with('$'),
            HirExprKind::Name { resolution } => resolution.source().starts_with('$'),
            HirExprKind::PropertyFetch { .. } => false,
            _ => {
                let range = self.span_for(SourceMappedId::from(method));
                self.source_text.slice(range).is_some_and(|source| {
                    let source = source.trim();
                    source.starts_with('$')
                        || source.contains("->$")
                        || source.contains("->{")
                        || source.contains("?->$")
                        || source.contains("?->{")
                })
            }
        }
    }

    pub(super) fn static_access_uses_dynamic_member(&self, expr: ExprId) -> bool {
        let range = self.span_for(SourceMappedId::from(expr));
        self.source_text
            .slice(range)
            .is_some_and(|source| source.contains("::$"))
    }

    pub(super) fn property_dim_target(&self, expr: ExprId) -> Option<PropertyDimTarget> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::DimFetch { receiver, dim } => {
                let receiver = (*receiver)?;
                let mut target = if let Some(property) = self.property_assignment_target(receiver) {
                    PropertyDimTarget {
                        receiver: property.receiver,
                        property: property.property,
                        dims: Vec::new(),
                        append: false,
                    }
                } else {
                    self.property_dim_target(receiver)?
                };
                if target.append {
                    return None;
                }
                if let Some(dim) = dim {
                    target.dims.push(*dim);
                } else {
                    target.append = true;
                }
                Some(target)
            }
            _ => None,
        }
    }

    pub(super) fn expr_id_for_span(&self, span: TextRange) -> Option<ExprId> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        module
            .expressions()
            .iter()
            .filter_map(|(expr_id, _)| {
                let expr_span = self.span_for(SourceMappedId::from(expr_id));
                if expr_span == span || range_contains(span, expr_span) {
                    Some((
                        expr_span.end().to_usize() - expr_span.start().to_usize(),
                        expr_id,
                    ))
                } else {
                    None
                }
            })
            .min_by_key(|(width, _)| *width)
            .map(|(_, expr_id)| expr_id)
    }

    pub(super) fn outermost_expr_inside(&self, span: TextRange) -> Option<ExprId> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        module
            .expressions()
            .iter()
            .filter_map(|(expr_id, _)| {
                let expr_span = self.span_for(SourceMappedId::from(expr_id));
                (expr_span != span && range_contains(span, expr_span)).then_some((
                    expr_span.end().to_usize() - expr_span.start().to_usize(),
                    expr_id,
                ))
            })
            .max_by_key(|(width, _)| *width)
            .map(|(_, expr_id)| expr_id)
    }

    pub(super) fn is_reflection_function_name(
        &self,
        expr: Option<php_semantics::hir::ExprId>,
    ) -> bool {
        self.static_source_or_resolved_name(expr)
            .is_some_and(|name| name.to_ascii_lowercase().starts_with("reflection"))
    }

    pub(super) fn static_source_or_resolved_name(
        &self,
        expr: Option<php_semantics::hir::ExprId>,
    ) -> Option<String> {
        let expr = expr?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Name { resolution } => {
                let source = resolution.source().trim_start_matches('\\');
                let resolved = resolution
                    .resolved()
                    .unwrap_or(source)
                    .trim_start_matches('\\');
                Some(resolved.to_owned())
            }
            _ => None,
        }
    }
}

fn class_constant_fetch_class_name(
    class_name: String,
    display_class_name: Option<String>,
) -> String {
    if matches!(
        normalize_class_name(&class_name).as_str(),
        "self" | "static" | "parent"
    ) {
        class_name
    } else {
        display_class_name.unwrap_or(class_name)
    }
}
