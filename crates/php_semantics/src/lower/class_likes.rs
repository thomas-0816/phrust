//! Structural class-like lowering.

use std::collections::HashSet;

use crate::FrontendDatabase;
use crate::diagnostics::{
    DiagnosticId, DiagnosticPhase, DiagnosticReporter, DiagnosticSeverity, SemanticDiagnostic,
};
use crate::hir::{
    AttributeId, ClassLikeId, ClassLikeKind, ClassLikeMember, ClassLikeMemberId,
    ClassLikeMemberKind, ConstExprContext, ConstExprId, ConstId, EnumCaseId, FullyQualifiedName,
    HirClassConst, HirClassLike, HirEnumCase, HirMethod, HirNameResolution, HirProperty,
    HirPropertyHook, HirPropertyHookBody, HirPropertyItem, HirTraitAdaptation,
    HirTraitAdaptationKind, HirTraitMethodRef, HirTraitUse, MagicMethodKind, MethodId, Modifier,
    ModifierOccurrence, ModifierSet, ModuleId, NamePart, NamespaceName, PropertyId, QualifiedName,
    SignatureKind, TraitUseId, TypeContext, TypeId,
};
use crate::lower::types::{
    TypeLoweringScope, TypeToken, enum_backing_type_tokens, lower_type_tokens,
    non_empty_type_tokens, type_token_from_syntax_token,
};
use crate::symbols::resolution::{NameResolver, ResolveContext, ResolvedName};
use php_ast::{
    AstNode, AstToken, ClassConstDecl, ClassDecl, EnumCase, EnumDecl, ExprNode, InterfaceDecl,
    MemberDecl, MethodDecl, PropertyDecl, TokenView, TraitDecl, TraitUseDecl, descendant_nodes,
    descendant_tokens, modifier_tokens, syntax_child_nodes, syntax_child_tokens,
};
use php_source::TextRange;
use php_syntax::{SyntaxElement, SyntaxNode};

/// Collects structural class-like HIR inside one top-level node.
pub fn collect_class_likes_in_node(
    node: &SyntaxNode,
    database: &mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &mut DiagnosticReporter,
    scope: TypeLoweringScope,
) {
    let mut lowerer = ClassLikeLowerer {
        database,
        module_id,
        reporter,
        scope,
        anonymous_count: 0,
    };
    lowerer.collect_node(node);
}

struct ClassLikeLowerer<'a> {
    database: &'a mut FrontendDatabase,
    module_id: ModuleId,
    reporter: &'a mut DiagnosticReporter,
    scope: TypeLoweringScope,
    anonymous_count: usize,
}

impl ClassLikeLowerer<'_> {
    fn collect_node(&mut self, node: &SyntaxNode) {
        if let Some(class_decl) = ClassDecl::cast(node) {
            self.collect_class(class_decl);
        } else if let Some(interface_decl) = InterfaceDecl::cast(node) {
            self.collect_named_class_like(
                ClassLikeKind::Interface,
                interface_decl.name_text(),
                interface_decl.syntax(),
            );
        } else if let Some(trait_decl) = TraitDecl::cast(node) {
            self.collect_named_class_like(
                ClassLikeKind::Trait,
                trait_decl.name_text(),
                trait_decl.syntax(),
            );
        } else if let Some(enum_decl) = EnumDecl::cast(node) {
            self.collect_enum(enum_decl);
        }

        for child in php_ast::syntax_child_nodes(node) {
            self.collect_node(child);
        }
    }

    fn collect_class(&mut self, class_decl: ClassDecl<'_>) {
        if class_decl.is_anonymous() {
            let anonymous_id = format!("anonymous#{}", self.anonymous_count);
            self.anonymous_count += 1;
            let mut class_like = HirClassLike::new(
                ClassLikeKind::AnonymousClass,
                None,
                None,
                Some(anonymous_id),
                collect_modifier_set(class_decl.syntax()),
            );
            class_like.set_extends(self.collect_clause_names(class_decl.syntax(), "T_EXTENDS"));
            class_like
                .set_implements(self.collect_clause_names(class_decl.syntax(), "T_IMPLEMENTS"));
            class_like.set_trait_uses(self.collect_trait_use_names(class_decl.syntax()));
            class_like.set_attributes(self.collect_attribute_ids(class_decl.syntax()));
            self.alloc_class_like(class_like, class_decl.syntax());
        } else {
            self.collect_named_class_like(
                ClassLikeKind::Class,
                class_decl.name_text(),
                class_decl.syntax(),
            );
        }
    }

    fn collect_enum(&mut self, enum_decl: EnumDecl<'_>) {
        let mut class_like = self.build_named_class_like(
            ClassLikeKind::Enum,
            enum_decl.name_text(),
            enum_decl.syntax(),
        );
        if let Some(tokens) = enum_backing_type_tokens(enum_decl.syntax()) {
            class_like.set_backing_type(lower_type_tokens(
                &tokens,
                self.database,
                self.module_id,
                self.reporter,
                self.scope.clone(),
                TypeContext::EnumBacking,
                true,
            ));
        }
        self.alloc_class_like(class_like, enum_decl.syntax());
    }

    fn collect_named_class_like(
        &mut self,
        kind: ClassLikeKind,
        name: Option<&str>,
        node: &SyntaxNode,
    ) {
        let class_like = self.build_named_class_like(kind, name, node);
        self.alloc_class_like(class_like, node);
    }

    fn build_named_class_like(
        &mut self,
        kind: ClassLikeKind,
        name: Option<&str>,
        node: &SyntaxNode,
    ) -> HirClassLike {
        let name = name.map(str::to_owned);
        let fqn = name
            .as_deref()
            .map(|name| declaration_fqn(self.scope.namespace_name(), name));
        let mut class_like = HirClassLike::new(kind, name, fqn, None, collect_modifier_set(node));
        class_like.set_extends(self.collect_clause_names(node, "T_EXTENDS"));
        class_like.set_implements(self.collect_clause_names(node, "T_IMPLEMENTS"));
        class_like.set_trait_uses(self.collect_trait_use_names(node));
        class_like.set_attributes(self.collect_attribute_ids(node));
        self.check_structural_rules(kind, node, &class_like);
        class_like
    }

    fn alloc_class_like(&mut self, mut class_like: HirClassLike, node: &SyntaxNode) {
        if class_like.attributes().is_empty() {
            class_like.set_attributes(self.collect_attribute_ids(node));
        }
        let id = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before class-like lowering")
            .class_likes_mut()
            .alloc(class_like);
        self.database.source_map_mut().insert(id, node.text_range());
        let (kind, is_backed_enum) = {
            let module = self
                .database
                .module(self.module_id)
                .expect("module allocated before class-like member lowering");
            let class_like = module
                .class_likes()
                .get(id)
                .expect("class-like allocated above");
            (
                class_like.kind(),
                class_like.kind() == ClassLikeKind::Enum && class_like.backing_type().is_some(),
            )
        };
        let members = self.collect_member_records(id, kind, is_backed_enum, node);
        self.database
            .module_mut(self.module_id)
            .expect("module allocated before class-like member lowering")
            .class_likes_mut()
            .get_mut(id)
            .expect("class-like allocated above")
            .set_members(members);
    }

    fn collect_member_records(
        &mut self,
        class_like_id: ClassLikeId,
        class_like_kind: ClassLikeKind,
        is_backed_enum: bool,
        node: &SyntaxNode,
    ) -> Vec<ClassLikeMember> {
        let mut members = Vec::new();
        let mut seen_methods = HashSet::<String>::new();
        let mut seen_properties = HashSet::<String>::new();
        let mut seen_consts = HashSet::<String>::new();
        let mut seen_enum_cases = HashSet::<String>::new();

        for member_node in class_member_nodes(node) {
            match MemberDecl::cast(member_node) {
                Some(MemberDecl::Method(method)) => {
                    let name = method.name_text().map(str::to_owned);
                    if let Some(name) = &name
                        && !seen_methods.insert(name.to_ascii_lowercase())
                    {
                        self.error(
                            DiagnosticId::DuplicateClassMember,
                            format!("duplicate method `{name}`"),
                            method.text_range(),
                        );
                    }
                    let id = self.alloc_method(class_like_id, method);
                    members.push(ClassLikeMember::with_id(
                        ClassLikeMemberKind::Method,
                        name,
                        ClassLikeMemberId::Method(id),
                    ));
                }
                Some(MemberDecl::Property(property)) if property.is_enum_case() => {
                    let name = property.name_text().map(str::to_owned);
                    if let Some(name) = &name
                        && !seen_enum_cases.insert(name.clone())
                    {
                        self.error(
                            DiagnosticId::DuplicateClassMember,
                            format!("duplicate enum case `{name}`"),
                            property.text_range(),
                        );
                    }
                    let id = self.alloc_enum_case(
                        class_like_id,
                        class_like_kind,
                        is_backed_enum,
                        property,
                    );
                    members.push(ClassLikeMember::with_id(
                        ClassLikeMemberKind::EnumCase,
                        name,
                        ClassLikeMemberId::EnumCase(id),
                    ));
                }
                Some(MemberDecl::Property(property)) => {
                    let names = property_item_names(property.syntax());
                    for name in &names {
                        if !seen_properties.insert(name.clone()) {
                            self.error(
                                DiagnosticId::DuplicateClassMember,
                                format!("duplicate property `{name}`"),
                                property.text_range(),
                            );
                        }
                    }
                    let id = self.alloc_property(class_like_id, property);
                    members.push(ClassLikeMember::with_id(
                        ClassLikeMemberKind::Property,
                        names.first().cloned(),
                        ClassLikeMemberId::Property(id),
                    ));
                }
                Some(MemberDecl::ClassConst(class_const)) => {
                    let items = class_const_items(class_const.syntax());
                    let items = if items.is_empty() {
                        vec![ClassConstItem {
                            name: class_const.name_text().map(str::to_owned),
                            value_range: None,
                        }]
                    } else {
                        items
                    };
                    for item in items {
                        if let Some(name) = &item.name
                            && !seen_consts.insert(name.clone())
                        {
                            self.error(
                                DiagnosticId::DuplicateClassMember,
                                format!("duplicate class constant `{name}`"),
                                class_const.text_range(),
                            );
                        }
                        let id = self.alloc_class_const(class_like_id, class_const, &item);
                        members.push(ClassLikeMember::with_id(
                            ClassLikeMemberKind::ClassConstant,
                            item.name,
                            ClassLikeMemberId::ClassConstant(id),
                        ));
                    }
                }
                Some(MemberDecl::TraitUse(trait_use)) => {
                    let id = self.alloc_trait_use(class_like_id, trait_use);
                    members.push(ClassLikeMember::with_id(
                        ClassLikeMemberKind::TraitUse,
                        None,
                        ClassLikeMemberId::TraitUse(id),
                    ));
                }
                None => {
                    if let Some(enum_case) = EnumCase::cast(member_node) {
                        members.push(ClassLikeMember::new(
                            ClassLikeMemberKind::EnumCase,
                            first_name_after_token(enum_case.syntax(), "T_CASE"),
                        ));
                    }
                }
            }
        }
        members
    }

    fn alloc_enum_case(
        &mut self,
        class_like_id: ClassLikeId,
        class_like_kind: ClassLikeKind,
        is_backed_enum: bool,
        property: PropertyDecl<'_>,
    ) -> EnumCaseId {
        let value =
            self.const_expr_id_in_node(property.syntax(), ConstExprContext::EnumCaseBackingValue);
        if class_like_kind == ClassLikeKind::Enum {
            match (is_backed_enum, value.is_some()) {
                (false, true) => self.error(
                    DiagnosticId::EnumCaseValueOnUnitEnum,
                    "unit enum case must not declare a backing value",
                    property.text_range(),
                ),
                (true, false) => self.error(
                    DiagnosticId::EnumCaseMissingValueOnBackedEnum,
                    "backed enum case must declare a backing value",
                    property.text_range(),
                ),
                _ => {}
            }
        }
        let enum_case = HirEnumCase::new(
            class_like_id,
            property.name_text().map(str::to_owned),
            value,
            self.collect_attribute_ids(property.syntax()),
        );
        let id = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before enum-case lowering")
            .enum_cases_mut()
            .alloc(enum_case);
        self.database
            .source_map_mut()
            .insert(id, property.syntax().text_range());
        id
    }

    fn alloc_method(&mut self, class_like_id: ClassLikeId, method: MethodDecl<'_>) -> MethodId {
        let name = method.name_text().map(str::to_owned);
        let signature_index = self.method_signature_index(method.syntax());
        let method_hir = HirMethod::new(
            class_like_id,
            name.clone(),
            name.as_deref().and_then(MagicMethodKind::from_name),
            collect_modifier_set(method.syntax()),
            method.body().is_some(),
            self.collect_attribute_ids(method.syntax()),
            signature_index,
        );
        let id = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before method lowering")
            .methods_mut()
            .alloc(method_hir);
        self.database
            .source_map_mut()
            .insert(id, method.syntax().text_range());
        id
    }

    fn alloc_trait_use(
        &mut self,
        class_like_id: ClassLikeId,
        trait_use: TraitUseDecl<'_>,
    ) -> TraitUseId {
        let traits = collect_trait_use_name_texts(trait_use.syntax())
            .into_iter()
            .map(|name| self.resolve_class_like_name(&name))
            .collect();
        let adaptations = self.collect_trait_adaptations(trait_use.syntax());
        let hir = HirTraitUse::new(class_like_id, traits, adaptations);
        let id = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before trait-use lowering")
            .trait_uses_mut()
            .alloc(hir);
        self.database
            .source_map_mut()
            .insert(id, trait_use.syntax().text_range());
        id
    }

    fn collect_trait_adaptations(&mut self, node: &SyntaxNode) -> Vec<HirTraitAdaptation> {
        adaptation_statement_tokens(node)
            .into_iter()
            .filter_map(|tokens| self.lower_trait_adaptation(&tokens))
            .collect()
    }

    fn lower_trait_adaptation(&mut self, tokens: &[AdaptationToken]) -> Option<HirTraitAdaptation> {
        let span = token_record_range(tokens)?;
        let instead_of = tokens.iter().position(|token| {
            token.kind == "T_INSTEADOF" || token.text.eq_ignore_ascii_case("insteadof")
        });
        let alias = tokens
            .iter()
            .position(|token| token.kind == "T_AS" || token.text.eq_ignore_ascii_case("as"));

        match (instead_of, alias) {
            (Some(index), None) => self.lower_trait_precedence(tokens, index, span),
            (None, Some(index)) => self.lower_trait_alias(tokens, index, span),
            _ => {
                self.error(
                    DiagnosticId::TraitAdaptationInvalidShape,
                    "trait adaptation must contain exactly one `as` or `insteadof` operator",
                    span,
                );
                None
            }
        }
    }

    fn lower_trait_precedence(
        &mut self,
        tokens: &[AdaptationToken],
        operator: usize,
        span: TextRange,
    ) -> Option<HirTraitAdaptation> {
        let method = self.trait_method_ref(&tokens[..operator], span)?;
        let names = trait_name_texts_from_tokens(&tokens[operator + 1..]);
        if names.is_empty() {
            self.error(
                DiagnosticId::TraitAdaptationInvalidShape,
                "`insteadof` adaptation requires at least one excluded trait",
                span,
            );
            return None;
        }
        Some(HirTraitAdaptation::new(
            HirTraitAdaptationKind::Precedence {
                instead_of: names
                    .into_iter()
                    .map(|name| self.resolve_class_like_name(&name))
                    .collect(),
            },
            method,
            span,
        ))
    }

    fn lower_trait_alias(
        &mut self,
        tokens: &[AdaptationToken],
        operator: usize,
        span: TextRange,
    ) -> Option<HirTraitAdaptation> {
        let method = self.trait_method_ref(&tokens[..operator], span)?;
        let mut visibility = None;
        let mut alias = None;
        for token in &tokens[operator + 1..] {
            if is_visibility_adaptation_token(token) {
                visibility = Some(token.text.to_ascii_lowercase());
            } else if token.kind == "T_STRING" {
                alias = Some(token.text.clone());
            }
        }
        if visibility.is_none() && alias.is_none() {
            self.error(
                DiagnosticId::TraitAdaptationInvalidShape,
                "`as` adaptation requires a visibility change or alias",
                span,
            );
            return None;
        }
        Some(HirTraitAdaptation::new(
            HirTraitAdaptationKind::Alias { alias, visibility },
            method,
            span,
        ))
    }

    fn trait_method_ref(
        &mut self,
        tokens: &[AdaptationToken],
        span: TextRange,
    ) -> Option<HirTraitMethodRef> {
        let significant: Vec<_> = tokens
            .iter()
            .filter(|token| token.text != "," && token.text != ";")
            .collect();
        if significant.is_empty() {
            self.error(
                DiagnosticId::TraitAdaptationInvalidShape,
                "trait adaptation is missing a method reference",
                span,
            );
            return None;
        }
        let separator = significant.iter().position(|token| token.text == "::");
        if let Some(separator) = separator {
            let trait_name = significant[..separator]
                .iter()
                .filter(|token| is_name_adaptation_token(token))
                .map(|token| token.text.as_str())
                .collect::<String>();
            let method = significant
                .iter()
                .skip(separator + 1)
                .find(|token| token.kind == "T_STRING")
                .map(|token| token.text.clone());
            let Some(method) = method else {
                self.error(
                    DiagnosticId::TraitAdaptationInvalidShape,
                    "qualified trait adaptation is missing a method name",
                    span,
                );
                return None;
            };
            let trait_name =
                (!trait_name.is_empty()).then(|| self.resolve_class_like_name(&trait_name));
            Some(HirTraitMethodRef::new(trait_name, method))
        } else {
            significant
                .iter()
                .find(|token| token.kind == "T_STRING")
                .map(|token| HirTraitMethodRef::new(None, token.text.clone()))
                .or_else(|| {
                    self.error(
                        DiagnosticId::TraitAdaptationInvalidShape,
                        "trait adaptation is missing a method name",
                        span,
                    );
                    None
                })
        }
    }

    fn alloc_property(
        &mut self,
        class_like_id: ClassLikeId,
        property: PropertyDecl<'_>,
    ) -> PropertyId {
        let items = property_item_names(property.syntax())
            .into_iter()
            .map(|name| {
                HirPropertyItem::new(
                    name,
                    self.const_expr_id_in_node(
                        property.syntax(),
                        ConstExprContext::PropertyDefault,
                    ),
                    None,
                )
            })
            .collect();
        let property_hir = HirProperty::new(
            class_like_id,
            collect_modifier_set(property.syntax()),
            self.type_id_in_node(property.syntax(), TypeContext::Property)
                .or_else(|| {
                    property_type_tokens(property.syntax()).and_then(|tokens| {
                        lower_type_tokens(
                            &tokens,
                            self.database,
                            self.module_id,
                            self.reporter,
                            self.scope.clone(),
                            TypeContext::Property,
                            true,
                        )
                    })
                }),
            items,
            property_hooks(property.syntax()),
            self.collect_attribute_ids(property.syntax()),
        );
        let id = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before property lowering")
            .properties_mut()
            .alloc(property_hir);
        self.database
            .source_map_mut()
            .insert(id, property.syntax().text_range());
        id
    }

    fn alloc_class_const(
        &mut self,
        class_like_id: ClassLikeId,
        class_const: ClassConstDecl<'_>,
        item: &ClassConstItem,
    ) -> ConstId {
        let const_hir = HirClassConst::new(
            class_like_id,
            item.name.clone(),
            collect_modifier_set(class_const.syntax()),
            self.type_id_in_node(class_const.syntax(), TypeContext::ClassConstant)
                .or_else(|| {
                    class_const_type_tokens(class_const.syntax()).and_then(|tokens| {
                        lower_type_tokens(
                            &tokens,
                            self.database,
                            self.module_id,
                            self.reporter,
                            self.scope.clone(),
                            TypeContext::ClassConstant,
                            true,
                        )
                    })
                }),
            if let Some(range) = item.value_range {
                self.const_expr_id_for_range(range, ConstExprContext::ClassConstInitializer)
            } else {
                self.const_expr_id_in_node(
                    class_const.syntax(),
                    ConstExprContext::ClassConstInitializer,
                )
            },
            self.collect_attribute_ids(class_const.syntax()),
        );
        let id = self
            .database
            .module_mut(self.module_id)
            .expect("module allocated before class-constant lowering")
            .class_consts_mut()
            .alloc(const_hir);
        self.database
            .source_map_mut()
            .insert(id, class_const.syntax().text_range());
        id
    }

    fn method_signature_index(&self, node: &SyntaxNode) -> Option<usize> {
        let module = self
            .database
            .module(self.module_id)
            .expect("module allocated before class-like lowering");
        module.signatures().iter().position(|signature| {
            signature.kind() == SignatureKind::Method && signature.span() == node.text_range()
        })
    }

    fn type_id_in_node(&self, node: &SyntaxNode, context: TypeContext) -> Option<TypeId> {
        let module = self
            .database
            .module(self.module_id)
            .expect("module allocated before class-like lowering");
        module
            .types()
            .iter()
            .filter_map(|(id, ty)| {
                let span = self.database.source_map().span(id)?;
                (ty.context() == context && contains_range(node.text_range(), span))
                    .then_some((id, span))
            })
            .max_by_key(|(_, span)| span.end().to_usize() - span.start().to_usize())
            .map(|(id, _)| id)
    }

    fn const_expr_id_in_node(
        &self,
        node: &SyntaxNode,
        context: ConstExprContext,
    ) -> Option<ConstExprId> {
        let module = self
            .database
            .module(self.module_id)
            .expect("module allocated before class-like lowering");
        module.const_exprs().iter().find_map(|(id, expr)| {
            let span = self.database.source_map().span(id)?;
            (expr.context() == context && contains_range(node.text_range(), span)).then_some(id)
        })
    }

    fn const_expr_id_for_range(
        &self,
        range: TextRange,
        context: ConstExprContext,
    ) -> Option<ConstExprId> {
        let module = self
            .database
            .module(self.module_id)
            .expect("module allocated before class-like lowering");
        module.const_exprs().iter().find_map(|(id, expr)| {
            let span = self.database.source_map().span(id)?;
            (expr.context() == context && span == range).then_some(id)
        })
    }

    fn collect_clause_names(&self, node: &SyntaxNode, marker: &str) -> Vec<HirNameResolution> {
        let Some((start, end)) = clause_token_range(node, marker) else {
            return Vec::new();
        };
        collect_name_texts_between(node, start, end)
            .into_iter()
            .map(|name| self.resolve_class_like_name(&name))
            .collect()
    }

    fn collect_trait_use_names(&self, node: &SyntaxNode) -> Vec<HirNameResolution> {
        descendant_nodes::<TraitUseDecl<'_>>(node)
            .flat_map(|trait_use| {
                collect_trait_use_name_texts(trait_use.syntax())
                    .into_iter()
                    .map(|name| self.resolve_class_like_name(&name))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn resolve_class_like_name(&self, source: &str) -> HirNameResolution {
        let qualified = QualifiedName::parse(source);
        let result = self
            .scope
            .resolver()
            .resolve(&qualified, ResolveContext::ClassLike);
        let name_kind = NameResolver::name_kind(ResolveContext::ClassLike);
        let (resolved, resolved_display) = match &result {
            ResolvedName::FullyQualified(name) => {
                (Some(name.canonical(name_kind)), Some(name.display()))
            }
            ResolvedName::MaybeRuntimeFallback { namespaced, .. } => (
                Some(namespaced.canonical(name_kind)),
                Some(namespaced.display()),
            ),
            ResolvedName::Dynamic | ResolvedName::Unresolved => (None, None),
        };
        HirNameResolution::new_with_display(
            source,
            ResolveContext::ClassLike.as_str(),
            result.classification(),
            resolved,
            resolved_display,
            None,
        )
    }

    fn collect_attribute_ids(&self, node: &SyntaxNode) -> Vec<AttributeId> {
        let direct_attribute_span = direct_attribute_span(node);
        let Some(attribute_span) = direct_attribute_span else {
            return Vec::new();
        };
        let module = self
            .database
            .module(self.module_id)
            .expect("module allocated before class-like lowering");
        module
            .attributes()
            .iter()
            .filter_map(|(id, _)| {
                let span = self.database.source_map().span(id)?;
                (span.start().to_usize() >= attribute_span.start().to_usize()
                    && span.end().to_usize() <= attribute_span.end().to_usize())
                .then_some(id)
            })
            .collect()
    }

    fn check_structural_rules(
        &mut self,
        kind: ClassLikeKind,
        node: &SyntaxNode,
        class_like: &HirClassLike,
    ) {
        if kind == ClassLikeKind::Class && class_like.extends().len() > 1 {
            self.note("class may extend at most one class", node.text_range());
        }
        if kind == ClassLikeKind::Trait
            && (!class_like.extends().is_empty() || !class_like.implements().is_empty())
        {
            self.note(
                "trait cannot declare extends or implements clauses",
                node.text_range(),
            );
        }
    }

    fn note(&mut self, message: impl Into<String>, span: TextRange) {
        self.reporter.report(SemanticDiagnostic::with_span(
            DiagnosticId::RuntimeCheckDeferred,
            DiagnosticSeverity::Note,
            DiagnosticPhase::ClassLikeValidation,
            message,
            span,
        ));
    }

    fn error(&mut self, id: DiagnosticId, message: impl Into<String>, span: TextRange) {
        self.reporter.report(SemanticDiagnostic::with_span(
            id,
            DiagnosticSeverity::Error,
            DiagnosticPhase::ClassLikeValidation,
            message,
            span,
        ));
    }
}

fn collect_modifier_set(node: &SyntaxNode) -> ModifierSet {
    let mut set = ModifierSet::new();
    for token in modifier_tokens(node) {
        if let Some(modifier) = Modifier::from_token_name(&token.kind().name()) {
            set.push(ModifierOccurrence::new(modifier, token.text_range()));
        }
    }
    set
}

fn class_member_nodes(node: &SyntaxNode) -> Vec<&SyntaxNode> {
    php_ast::syntax_child_nodes(node)
        .find(|child| child.kind().name() == "CLASS_MEMBER_LIST")
        .map(|members| php_ast::syntax_child_nodes(members).collect())
        .unwrap_or_default()
}

fn clause_token_range(node: &SyntaxNode, marker: &str) -> Option<(usize, usize)> {
    let tokens: Vec<_> = syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let marker_index = tokens
        .iter()
        .position(|token| token.kind().name() == marker)?;
    let end = tokens
        .iter()
        .enumerate()
        .skip(marker_index + 1)
        .find(|(_, token)| {
            matches!(
                token.kind().name().as_str(),
                "T_IMPLEMENTS" | "T_EXTENDS" | "{"
            )
        })
        .map(|(index, _)| index)
        .unwrap_or(tokens.len());
    Some((marker_index + 1, end))
}

fn collect_name_texts_between(node: &SyntaxNode, start: usize, end: usize) -> Vec<String> {
    let tokens: Vec<_> = syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let mut names = Vec::new();
    let mut current = String::new();
    for token in tokens.iter().take(end).skip(start) {
        if token.text() == "," {
            if !current.is_empty() {
                names.push(std::mem::take(&mut current));
            }
            continue;
        }
        if is_name_token(token) || token.text() == "\\" {
            current.push_str(token.text());
        }
    }
    if !current.is_empty() {
        names.push(current);
    }
    names
}

fn collect_trait_use_name_texts(node: &SyntaxNode) -> Vec<String> {
    let tokens: Vec<_> = syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .collect();
    let start = tokens
        .iter()
        .position(|token| token.kind().name() == "T_USE")
        .map(|index| index + 1)
        .unwrap_or(0);
    let end = tokens
        .iter()
        .position(|token| token.text() == ";" || token.text() == "{")
        .unwrap_or(tokens.len());
    collect_name_texts_between(node, start, end)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AdaptationToken {
    text: String,
    kind: String,
    range: TextRange,
}

fn adaptation_statement_tokens(node: &SyntaxNode) -> Vec<Vec<AdaptationToken>> {
    let mut in_block = false;
    let mut current = Vec::new();
    let mut statements = Vec::new();
    for token in syntax_child_tokens(node).filter(|token| !token.kind().is_trivia()) {
        if token.text() == "{" {
            in_block = true;
            continue;
        }
        if token.text() == "}" {
            if !current.is_empty() {
                statements.push(std::mem::take(&mut current));
            }
            break;
        }
        if !in_block {
            continue;
        }
        if token.text() == ";" {
            if !current.is_empty() {
                statements.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(AdaptationToken {
            text: token.text().to_owned(),
            kind: token.kind().name(),
            range: token.text_range(),
        });
    }
    statements
}

fn trait_name_texts_from_tokens(tokens: &[AdaptationToken]) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = String::new();
    for token in tokens {
        if token.text == "," {
            if !current.is_empty() {
                names.push(std::mem::take(&mut current));
            }
            continue;
        }
        if is_name_adaptation_token(token) || token.text == "\\" {
            current.push_str(&token.text);
        }
    }
    if !current.is_empty() {
        names.push(current);
    }
    names
}

fn token_record_range(tokens: &[AdaptationToken]) -> Option<TextRange> {
    let first = tokens.first()?;
    let last = tokens.last()?;
    Some(TextRange::new(
        first.range.start().to_usize(),
        last.range.end().to_usize(),
    ))
}

fn is_name_adaptation_token(token: &AdaptationToken) -> bool {
    matches!(
        token.kind.as_str(),
        "T_STRING" | "T_NAME_QUALIFIED" | "T_NAME_FULLY_QUALIFIED" | "T_NAME_RELATIVE"
    )
}

fn is_visibility_adaptation_token(token: &AdaptationToken) -> bool {
    matches!(
        token.kind.as_str(),
        "T_PUBLIC" | "T_PROTECTED" | "T_PRIVATE"
    )
}

fn property_item_names(node: &SyntaxNode) -> Vec<String> {
    syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .take_while(|token| token.text() != "{")
        .filter(|token| token.kind().name() == "T_VARIABLE")
        .map(|token| token.text().to_owned())
        .collect()
}

#[derive(Clone, Debug)]
struct ClassConstItem {
    name: Option<String>,
    value_range: Option<TextRange>,
}

fn class_const_items(node: &SyntaxNode) -> Vec<ClassConstItem> {
    let mut items = Vec::new();
    let mut last_string: Option<String> = None;
    let mut pending_name: Option<String> = None;

    for child in node.children() {
        match child {
            SyntaxElement::Token(token) => {
                if token.kind().is_trivia() {
                    continue;
                }
                match token.kind().name().as_str() {
                    "T_STRING" => last_string = Some(token.text().to_owned()),
                    _ if token.text() == "=" => {
                        pending_name = last_string.take();
                    }
                    _ if token.text() == "," => {
                        pending_name = None;
                        last_string = None;
                    }
                    _ => {}
                }
            }
            SyntaxElement::Node(expr_node) if ExprNode::cast(expr_node).is_some() => {
                items.push(ClassConstItem {
                    name: pending_name.take(),
                    value_range: Some(expr_node.text_range()),
                });
                last_string = None;
            }
            SyntaxElement::Node(_) => {}
        }
    }

    items
}

fn property_type_tokens(node: &SyntaxNode) -> Option<Vec<TypeToken>> {
    let tokens: Vec<TypeToken> = syntax_child_tokens(node)
        .filter(|token| !token.kind().is_trivia())
        .take_while(|token| token.kind().name() != "T_VARIABLE")
        .map(type_token_from_syntax_token)
        .filter(|token| !is_modifier_type_token(token))
        .collect();
    non_empty_type_tokens(tokens)
}

fn class_const_type_tokens(node: &SyntaxNode) -> Option<Vec<TypeToken>> {
    let tokens = descendant_type_tokens(node);
    let const_index = tokens.iter().position(|token| token.kind == "T_CONST")?;
    let first_string_after_const = tokens
        .iter()
        .enumerate()
        .skip(const_index + 1)
        .find(|(_, token)| token.kind == "T_STRING")
        .map(|(index, _)| index)?;
    let second_string_after_const = tokens
        .iter()
        .enumerate()
        .skip(first_string_after_const + 1)
        .find(|(_, token)| token.kind == "T_STRING")
        .map(|(index, _)| index);
    let end = second_string_after_const?;
    non_empty_type_tokens(tokens[const_index + 1..end].to_vec())
}

fn descendant_type_tokens(node: &SyntaxNode) -> Vec<TypeToken> {
    descendant_tokens::<TokenView<'_>>(node)
        .filter(|token| !token.kind().is_trivia())
        .map(|token| TypeToken {
            text: token.text().to_owned(),
            kind: token.kind().name(),
            range: token.text_range(),
        })
        .collect()
}

fn is_modifier_type_token(token: &TypeToken) -> bool {
    matches!(
        token.kind.as_str(),
        "T_PUBLIC"
            | "T_PROTECTED"
            | "T_PRIVATE"
            | "T_STATIC"
            | "T_READONLY"
            | "T_VAR"
            | "T_PUBLIC_SET"
            | "T_PROTECTED_SET"
            | "T_PRIVATE_SET"
    )
}

fn property_hooks(node: &SyntaxNode) -> Vec<HirPropertyHook> {
    let mut hooks = Vec::new();
    let property_name = syntax_child_tokens(node)
        .find(|token| token.kind().name() == "T_VARIABLE")
        .map(|token| token.text().trim_start_matches('$').to_owned());
    for hook in syntax_child_nodes(node).filter(|node| node.kind().name() == "PROPERTY_HOOK_DECL") {
        let mut kind = None;
        let mut body = HirPropertyHookBody::Block;
        for token in syntax_child_tokens(hook).filter(|token| !token.kind().is_trivia()) {
            match token.text() {
                "get" | "set" if kind.is_none() => kind = Some(token.text().to_owned()),
                "=>" => body = HirPropertyHookBody::Expression,
                _ => {}
            }
        }
        if let Some(kind) = kind {
            let tokens = descendant_tokens::<TokenView<'_>>(hook)
                .filter(|token| !token.kind().is_trivia())
                .collect::<Vec<_>>();
            let uses_backing_storage = property_name.as_deref().is_some_and(|property_name| {
                tokens.windows(3).any(|window| {
                    window[0].text() == "$this"
                        && window[1].text() == "->"
                        && window[2].text().trim_start_matches('$') == property_name
                })
            });
            hooks.push(HirPropertyHook::new(
                kind,
                hook.text_range(),
                body,
                uses_backing_storage,
            ));
        }
    }
    hooks
}

fn contains_range(outer: TextRange, inner: TextRange) -> bool {
    inner.start().to_usize() >= outer.start().to_usize()
        && inner.end().to_usize() <= outer.end().to_usize()
}

fn is_name_token(token: &&php_syntax::SyntaxToken) -> bool {
    matches!(
        token.kind().name().as_str(),
        "T_STRING" | "T_NAME_QUALIFIED" | "T_NAME_FULLY_QUALIFIED" | "T_NAME_RELATIVE"
    )
}

fn direct_attribute_span(node: &SyntaxNode) -> Option<TextRange> {
    let attrs: Vec<_> = php_ast::syntax_child_nodes(node)
        .filter(|child| child.kind().name() == "ATTRIBUTE_GROUP")
        .collect();
    let first = attrs.first()?;
    let last = attrs.last()?;
    Some(TextRange::new(
        first.text_range().start().to_usize(),
        last.text_range().end().to_usize(),
    ))
}

fn first_name_after_token(node: &SyntaxNode, marker: &str) -> Option<String> {
    let mut after = false;
    for token in syntax_child_tokens(node).filter(|token| !token.kind().is_trivia()) {
        if after && token.kind().name() == "T_STRING" {
            return Some(token.text().to_owned());
        }
        if token.kind().name() == marker {
            after = true;
        }
    }
    None
}

fn declaration_fqn(namespace_name: Option<&NamespaceName>, name: &str) -> FullyQualifiedName {
    let mut parts: Vec<NamePart> = namespace_name
        .map(|namespace| QualifiedName::parse(namespace.text()).parts().to_vec())
        .unwrap_or_default();
    parts.push(NamePart::new(name));
    FullyQualifiedName::from_parts(parts)
}
