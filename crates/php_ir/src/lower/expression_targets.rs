use super::expressions::*;
use super::*;

impl LoweringContext<'_> {
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
            HirExprKind::Variable { name, .. } => {
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
            && self
                .comparison_assignment_target(builder, function, right)
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
                let relative_name = normalized_source
                    .rsplit('\\')
                    .next()
                    .filter(|name| matches!(*name, "self" | "static" | "parent"));
                if let Some(relative_name) = relative_name {
                    return Some(relative_name.to_owned());
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
                .or_else(|| {
                    resolution
                        .resolved()
                        .or_else(|| resolution.fallback())
                        .map(display_class_name)
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
            HirExprKind::Literal { text } => quoted_literal_body(text)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .or_else(|| Some(local_name(text).to_owned())),
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
            HirExprKind::Variable { name, .. } => Some(local_name(name).to_owned()),
            _ => None,
        }
    }

    pub(super) fn static_property_display_name(&self, expr: ExprId) -> Option<String> {
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
        let target = (*target)?;
        let normalized_class_name = self.static_class_name(target)?;
        let class_name = if matches!(normalized_class_name.as_str(), "self" | "static" | "parent") {
            normalized_class_name
        } else {
            self.static_class_display_name(target)
                .unwrap_or(normalized_class_name)
        };
        Some(StaticPropertyTarget {
            class_name,
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
        self.static_property_target(expr)
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
            HirExprKind::Literal { text } => {
                text.starts_with('$') || interpolated_literal_parts(text).is_some()
            }
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
            HirExprKind::Literal { text } => {
                text.starts_with('$') || interpolated_literal_parts(text).is_some()
            }
            HirExprKind::Name { resolution } => resolution.source().starts_with('$'),
            HirExprKind::PropertyFetch { .. } => false,
            _ => true,
        }
    }

    pub(super) fn static_access_uses_dynamic_member(&self, expr: ExprId) -> bool {
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
        let HirExprKind::StaticAccess { member, .. } = expression.kind() else {
            return false;
        };
        let Some(member) = member.and_then(|member| module.expressions().get(member)) else {
            return false;
        };
        !matches!(
            member.kind(),
            HirExprKind::Literal { .. } | HirExprKind::Name { .. }
        )
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
