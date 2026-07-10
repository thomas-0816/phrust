use std::collections::HashMap;

use crate::ids::FunctionId;
use crate::module::ClassMethodFlags;
use php_semantics::hir::{ClassLikeId, ConstExprId, FunctionSignature, HirModule, HirProperty};

use super::expressions::*;
use super::*;

pub(super) type ClassConstantInitializerMap = HashMap<String, HashMap<String, ConstExprId>>;
pub(super) type ClassParentMap = HashMap<String, Option<String>>;

#[derive(Clone, Copy, Debug)]
pub(super) struct MethodFunctionInput<'a> {
    pub(super) class_name: &'a str,
    pub(super) method_name: &'a str,
    pub(super) display_class_name: &'a str,
    pub(super) display_method_name: &'a str,
    pub(super) signature: &'a FunctionSignature,
    pub(super) class_constant_initializers: &'a ClassConstantInitializerMap,
    pub(super) class_parents: &'a ClassParentMap,
    pub(super) main_function: FunctionId,
}

#[derive(Clone, Debug)]
pub(super) struct TraitMethodCandidate {
    pub(super) trait_name: String,
    pub(super) display_trait_name: String,
    pub(super) method_name: String,
    pub(super) display_method_name: String,
    pub(super) signature: FunctionSignature,
    pub(super) flags: ClassMethodFlags,
}

#[derive(Clone, Debug)]
pub(super) struct TraitAliasSpec {
    pub(super) trait_name: Option<String>,
    pub(super) method_name: String,
    pub(super) alias: Option<String>,
    pub(super) visibility: Option<TraitVisibility>,
}

pub(super) struct TraitCompositionInput<'a> {
    pub(super) module: &'a HirModule,
    pub(super) trait_class_likes:
        &'a HashMap<String, (ClassLikeId, php_semantics::hir::HirClassLike)>,
    pub(super) main_function: FunctionId,
    pub(super) class_like_id: ClassLikeId,
    pub(super) class_like: &'a php_semantics::hir::HirClassLike,
    pub(super) class_name: &'a str,
    pub(super) display_class_name: &'a str,
    pub(super) class_constant_initializers: &'a ClassConstantInitializerMap,
    pub(super) class_parents: &'a ClassParentMap,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PropertyEntriesInput<'a> {
    pub(super) class_name: &'a str,
    pub(super) display_class_name: &'a str,
    pub(super) property: &'a HirProperty,
    pub(super) class_constant_initializers: &'a ClassConstantInitializerMap,
    pub(super) class_parents: &'a ClassParentMap,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TraitCompositionMembersInput<'a> {
    pub(super) module: &'a HirModule,
    pub(super) trait_class_like: &'a php_semantics::hir::HirClassLike,
    pub(super) trait_name: &'a str,
    pub(super) class_name: &'a str,
    pub(super) display_class_name: &'a str,
    pub(super) class_constant_initializers: &'a ClassConstantInitializerMap,
    pub(super) class_parents: &'a ClassParentMap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TraitVisibility {
    Public,
    Protected,
    Private,
}

impl TraitVisibility {
    pub(super) fn from_text(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "public" => Some(Self::Public),
            "protected" => Some(Self::Protected),
            "private" => Some(Self::Private),
            _ => None,
        }
    }

    pub(super) fn apply(self, flags: &mut ClassMethodFlags) {
        flags.is_private = self == Self::Private;
        flags.is_protected = self == Self::Protected;
    }
}

impl LoweringContext<'_> {
    pub(super) fn lower_global_constant_declarations(
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
        let entries = module.declaration_table().entries().to_vec();
        let mut initializers = self.global_const_initializers().into_iter();
        let mut current = block;
        for declaration in entries
            .iter()
            .filter(|entry| entry.kind() == DeclarationKind::Constant)
        {
            let span = span_from_range(self.file, declaration.span());
            let name = declaration.fqn().canonical(NameKind::Constant);
            let Some((expr_id, constant)) = initializers.next() else {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    declaration.span(),
                    "global const initializer is missing from the Semantic frontend",
                );
                continue;
            };
            if let Some(constant) = constant {
                let value = builder.intern_constant(constant);
                builder.register_constant_name(name, value, span);
                continue;
            }
            let Some(value) = self.lower_expr_to_register(builder, function, current, expr_id)
            else {
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    declaration.span(),
                    "global const initializer is not a lowerable constant expression",
                );
                continue;
            };
            current = value.block;
            builder.emit(
                function,
                current,
                InstructionKind::RegisterConstant {
                    name,
                    value: Operand::Register(value.register),
                },
                span,
            );
        }
        current
    }

    pub(super) fn lower_class_declarations(
        &mut self,
        builder: &mut IrBuilder,
        main_function: FunctionId,
    ) {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return;
        };
        let class_likes = module
            .class_likes()
            .iter()
            .map(|(id, class_like)| (id, class_like.clone()))
            .collect::<Vec<_>>();
        let class_constant_initializers =
            collect_class_constant_initializers(module, &class_likes, &self.options.source_path);
        let class_parents = collect_class_parents(&class_likes, &self.options.source_path);
        let trait_class_likes = class_likes
            .iter()
            .filter(|(_, class_like)| class_like.kind() == ClassLikeKind::Trait)
            .filter_map(|(id, class_like)| {
                let name = class_like
                    .fqn()
                    .map(|name| name.canonical(NameKind::ClassLike))
                    .or_else(|| class_like.name().map(normalize_class_name))?;
                Some((name, (*id, class_like.clone())))
            })
            .collect::<HashMap<_, _>>();
        let declared_class_likes = class_likes
            .iter()
            .filter_map(|(_, class_like)| {
                class_like
                    .fqn()
                    .map(|name| name.canonical(NameKind::ClassLike))
                    .or_else(|| class_like.name().map(normalize_class_name))
                    .map(|name| normalize_class_name(&name))
            })
            .collect::<HashSet<_>>();
        let class_likes_snapshot = class_likes.clone();
        self.push_internal_interfaces(builder, &declared_class_likes);
        for (class_like_id, class_like) in class_likes {
            if !matches!(
                class_like.kind(),
                ClassLikeKind::Class
                    | ClassLikeKind::AnonymousClass
                    | ClassLikeKind::Interface
                    | ClassLikeKind::Trait
                    | ClassLikeKind::Enum
            ) {
                let feature = match class_like.kind() {
                    ClassLikeKind::Enum => UnsupportedFeature::EnumRuntime,
                    _ => UnsupportedFeature::ClassLikeObject,
                };
                self.unsupported(
                    feature,
                    self.span_for(SourceMappedId::from(class_like_id)),
                    format!(
                        "class-like kind `{}` is not executable in the known-gap known-gap layer",
                        class_like.kind().as_str()
                    ),
                );
                continue;
            }
            let Some(name) = class_like_normalized_name(&class_like, &self.options.source_path)
            else {
                continue;
            };
            let display_class_name = class_like_display_name(&class_like, &name);
            let class_range = self.span_for(SourceMappedId::from(class_like_id));
            let span = span_from_range(self.file, class_range);
            let declaration_kind = class_declaration_kind(module, &class_like, class_range, &name);
            let parent = class_like.extends().first().map(|name| {
                normalize_class_name(
                    name.resolved()
                        .or_else(|| name.fallback())
                        .unwrap_or_else(|| name.source()),
                )
            });
            let parent_display_name = class_like.extends().first().map(|name| {
                class_resolution_display_name(
                    module,
                    name,
                    class_range,
                    &class_likes_snapshot,
                    &self.options.source_path,
                )
            });
            let parent = matches!(
                class_like.kind(),
                ClassLikeKind::Class | ClassLikeKind::AnonymousClass
            )
            .then_some(parent)
            .flatten();
            let parent_display_name = matches!(
                class_like.kind(),
                ClassLikeKind::Class | ClassLikeKind::AnonymousClass
            )
            .then_some(parent_display_name)
            .flatten();
            let mut interfaces: Vec<String> = if class_like.kind() == ClassLikeKind::Interface {
                class_like
                    .extends()
                    .iter()
                    .map(interface_resolution_name)
                    .collect()
            } else {
                class_like
                    .implements()
                    .iter()
                    .map(interface_resolution_name)
                    .collect()
            };
            if class_like.kind() == ClassLikeKind::Enum {
                interfaces.push(normalize_class_name("UnitEnum"));
                if class_like.backing_type().is_some() {
                    interfaces.push(normalize_class_name("BackedEnum"));
                }
            }
            let mut methods = Vec::new();
            let mut properties = Vec::new();
            let mut constants = Vec::new();
            let mut enum_cases = Vec::new();
            let enum_backing_type = self.lower_enum_backing_type(&class_like);
            if class_like.kind() == ClassLikeKind::Enum {
                properties.push(ClassPropertyEntry {
                    name: "name".to_owned(),
                    default: None,
                    default_class_constant: None,
                    default_named_constant: None,
                    default_expr: None,
                    type_: Some(IrReturnType::String),
                    flags: ClassPropertyFlags {
                        is_readonly: true,
                        is_typed: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                });
                if let Some(backing_type) = enum_backing_type {
                    properties.push(ClassPropertyEntry {
                        name: "value".to_owned(),
                        default: None,
                        default_class_constant: None,
                        default_named_constant: None,
                        default_expr: None,
                        type_: Some(match backing_type {
                            ClassEnumBackingType::Int => IrReturnType::Int,
                            ClassEnumBackingType::String => IrReturnType::String,
                        }),
                        flags: ClassPropertyFlags {
                            is_readonly: true,
                            is_typed: true,
                            ..ClassPropertyFlags::default()
                        },
                        hooks: ClassPropertyHooks::default(),
                        attributes: Vec::new(),
                    });
                }
            }
            let mut constructor = None;
            self.compose_trait_methods(
                builder,
                TraitCompositionInput {
                    module,
                    trait_class_likes: &trait_class_likes,
                    main_function,
                    class_like_id,
                    class_like: &class_like,
                    class_name: &name,
                    display_class_name: &display_class_name,
                    class_constant_initializers: &class_constant_initializers,
                    class_parents: &class_parents,
                },
                &mut methods,
                &mut properties,
            );
            for member in class_like.members() {
                match member.id() {
                    Some(ClassLikeMemberId::Method(method_id)) => {
                        let Some(method) = module.methods().get(method_id).cloned() else {
                            continue;
                        };
                        let Some(method_name) = method.name().map(normalize_method_name) else {
                            continue;
                        };
                        let display_method_name = method
                            .name()
                            .map(ToOwned::to_owned)
                            .unwrap_or_else(|| method_name.clone());
                        let Some(signature) = method
                            .signature_index()
                            .and_then(|index| module.signatures().get(index))
                            .cloned()
                        else {
                            continue;
                        };
                        if method.magic_kind() == Some(MagicMethodKind::Construct) {
                            self.push_promoted_constructor_properties(
                                builder,
                                &mut properties,
                                &signature,
                            );
                        }
                        let function = self.lower_method_function(
                            builder,
                            MethodFunctionInput {
                                class_name: &name,
                                method_name: &method_name,
                                display_class_name: &display_class_name,
                                display_method_name: &display_method_name,
                                signature: &signature,
                                class_constant_initializers: &class_constant_initializers,
                                class_parents: &class_parents,
                                main_function,
                            },
                        );
                        if method.magic_kind() == Some(MagicMethodKind::Construct) {
                            constructor = Some(function);
                        }
                        methods.retain(|entry| normalize_method_name(&entry.name) != method_name);
                        methods.push(ClassMethodEntry {
                            name: method_name,
                            origin_class: name.clone(),
                            function,
                            flags: ClassMethodFlags {
                                is_static: method.modifiers().is_static(),
                                is_private: method
                                    .modifiers()
                                    .visibility()
                                    .is_some_and(|visibility| visibility == Visibility::Private),
                                is_protected: method
                                    .modifiers()
                                    .visibility()
                                    .is_some_and(|visibility| visibility == Visibility::Protected),
                                is_abstract: method.modifiers().is_abstract()
                                    || (class_like.kind() == ClassLikeKind::Interface
                                        && signature.body().is_empty()),
                                has_body: method.has_body(),
                                is_final: method.modifiers().is_final(),
                            },
                            attributes: self.lower_attribute_ids(builder, method.attributes()),
                        });
                    }
                    Some(ClassLikeMemberId::Property(property_id)) => {
                        let Some(property) = module.properties().get(property_id) else {
                            continue;
                        };
                        self.push_lowered_property_entries(
                            builder,
                            &mut properties,
                            PropertyEntriesInput {
                                class_name: &name,
                                display_class_name: &display_class_name,
                                property,
                                class_constant_initializers: &class_constant_initializers,
                                class_parents: &class_parents,
                            },
                        );
                    }
                    Some(ClassLikeMemberId::ClassConstant(const_id)) => {
                        let Some(constant) = module.class_consts().get(const_id) else {
                            continue;
                        };
                        let Some(constant_name) = constant.name().map(ToOwned::to_owned) else {
                            continue;
                        };
                        let value = self
                            .lower_class_constant_value(
                                constant.value(),
                                &name,
                                &display_class_name,
                                &class_constant_initializers,
                                &class_parents,
                            )
                            .map(|constant| builder.intern_constant(constant));
                        let value_class_constant = if value.is_none() {
                            self.lower_const_expr_class_constant_reference(
                                constant.value(),
                                |context| {
                                    matches!(context, ConstExprContext::ClassConstInitializer)
                                },
                                Some(&name),
                                &class_parents,
                            )
                        } else {
                            None
                        };
                        let value_named_constant =
                            if value.is_none() && value_class_constant.is_none() {
                                self.lower_const_expr_named_constant_reference(
                                    constant.value(),
                                    |context| {
                                        matches!(context, ConstExprContext::ClassConstInitializer)
                                    },
                                )
                            } else {
                                None
                            };
                        constants.push(ClassConstantEntry {
                            name: constant_name,
                            value,
                            value_class_constant,
                            value_named_constant,
                            doc_comment: self
                                .doc_comment_before(self.span_for(SourceMappedId::from(const_id))),
                            flags: ClassConstantFlags {
                                is_private: constant
                                    .modifiers()
                                    .visibility()
                                    .is_some_and(|visibility| visibility == Visibility::Private),
                                is_protected: constant
                                    .modifiers()
                                    .visibility()
                                    .is_some_and(|visibility| visibility == Visibility::Protected),
                            },
                            attributes: self.lower_attribute_ids(builder, constant.attributes()),
                            span: span_from_range(
                                self.file,
                                self.span_for(SourceMappedId::from(const_id)),
                            ),
                        });
                    }
                    Some(ClassLikeMemberId::TraitUse(_trait_use_id)) => {}
                    Some(ClassLikeMemberId::EnumCase(enum_case_id)) => {
                        let Some(enum_case) = module.enum_cases().get(enum_case_id) else {
                            continue;
                        };
                        let Some(case_name) = enum_case.name().map(ToOwned::to_owned) else {
                            continue;
                        };
                        let value = self
                            .lower_enum_case_value(enum_case.value())
                            .map(|constant| builder.intern_constant(constant));
                        enum_cases.push(ClassEnumCaseEntry {
                            name: case_name,
                            value,
                            attributes: self.lower_attribute_ids(builder, enum_case.attributes()),
                        });
                    }
                    _ => {}
                }
            }
            let attributes = self.lower_attribute_ids(builder, class_like.attributes());
            builder.push_class(ClassEntry {
                id: crate::ids::ClassId::new(0),
                name,
                display_name: display_class_name,
                parent,
                parent_display_name,
                interfaces,
                methods,
                properties,
                constants,
                enum_cases,
                attributes,
                enum_backing_type,
                constructor,
                flags: ClassFlags {
                    is_abstract: class_like.modifiers().is_abstract(),
                    is_final: class_like.modifiers().is_final()
                        || class_like.kind() == ClassLikeKind::Enum,
                    is_readonly: class_like.modifiers().is_readonly(),
                    is_interface: class_like.kind() == ClassLikeKind::Interface,
                    is_enum: class_like.kind() == ClassLikeKind::Enum,
                    is_trait: class_like.kind() == ClassLikeKind::Trait,
                    is_conditional: declaration_kind == Some(DeclarationKind::ConditionalClassLike),
                },
                span,
            });
        }
    }

    pub(super) fn push_promoted_constructor_properties(
        &self,
        builder: &mut IrBuilder,
        properties: &mut Vec<ClassPropertyEntry>,
        signature: &FunctionSignature,
    ) {
        for param in signature.parameters() {
            let Some(promotion) = param.flags().promoted_property() else {
                continue;
            };
            let property_name = local_name(param.name()).to_owned();
            if properties
                .iter()
                .any(|property| property.name == property_name)
            {
                continue;
            }
            let set_visibility = promotion.set_visibility();
            properties.push(ClassPropertyEntry {
                name: property_name,
                default: None,
                default_class_constant: None,
                default_named_constant: None,
                default_expr: None,
                type_: self.lower_runtime_type(param.type_id()),
                flags: ClassPropertyFlags {
                    is_private: promotion.visibility() == Visibility::Private,
                    is_protected: promotion.visibility() == Visibility::Protected,
                    set_is_private: set_visibility
                        .is_some_and(|visibility| visibility == Visibility::Private),
                    set_is_protected: set_visibility
                        .is_some_and(|visibility| visibility == Visibility::Protected),
                    is_readonly: promotion.is_readonly(),
                    is_typed: param.type_id().is_some(),
                    ..ClassPropertyFlags::default()
                },
                hooks: ClassPropertyHooks::default(),
                attributes: self.lower_parameter_attributes(builder, param.attributes()),
            });
        }
    }

    pub(super) fn push_lowered_property_entries(
        &mut self,
        builder: &mut IrBuilder,
        properties: &mut Vec<ClassPropertyEntry>,
        input: PropertyEntriesInput<'_>,
    ) {
        let class_name = input.class_name;
        let display_class_name = input.display_class_name;
        let property = input.property;
        let property_type = self.lower_runtime_type(property.type_id());
        let hooks = self.lower_property_hooks(builder, class_name, display_class_name, property);
        let set_visibility = property.modifiers().set_visibility();
        let attributes = self.lower_attribute_ids(builder, property.attributes());
        for item in property.items() {
            let property_name = local_name(item.name()).to_owned();
            if properties
                .iter()
                .any(|existing| existing.name == property_name)
            {
                continue;
            }
            let default = self
                .lower_property_default(
                    item.default(),
                    Some(class_name),
                    Some(display_class_name),
                    input.class_constant_initializers,
                    input.class_parents,
                )
                .map(|constant| builder.intern_constant(constant));
            let default_class_constant = if default.is_none() {
                self.lower_const_expr_class_constant_reference(
                    item.default(),
                    |context| {
                        matches!(
                            context,
                            ConstExprContext::PropertyDefault
                                | ConstExprContext::PromotedPropertyDefault
                        )
                    },
                    Some(class_name),
                    input.class_parents,
                )
            } else {
                None
            };
            let default_named_constant = if default.is_none() && default_class_constant.is_none() {
                self.lower_const_expr_named_constant_reference(item.default(), |context| {
                    matches!(
                        context,
                        ConstExprContext::PropertyDefault
                            | ConstExprContext::PromotedPropertyDefault
                    )
                })
            } else {
                None
            };
            let default_expr = if default.is_none()
                && default_class_constant.is_none()
                && default_named_constant.is_none()
            {
                self.lower_deferred_property_default(
                    item.default(),
                    Some(class_name),
                    Some(display_class_name),
                    input.class_constant_initializers,
                    input.class_parents,
                )
            } else {
                None
            };
            properties.push(ClassPropertyEntry {
                name: property_name,
                default,
                default_class_constant,
                default_named_constant,
                default_expr,
                type_: property_type.clone(),
                flags: ClassPropertyFlags {
                    is_static: property.modifiers().is_static(),
                    is_private: property
                        .modifiers()
                        .visibility()
                        .is_some_and(|visibility| visibility == Visibility::Private),
                    is_protected: property
                        .modifiers()
                        .visibility()
                        .is_some_and(|visibility| visibility == Visibility::Protected),
                    set_is_private: set_visibility
                        .is_some_and(|visibility| visibility == Visibility::Private),
                    set_is_protected: set_visibility
                        .is_some_and(|visibility| visibility == Visibility::Protected),
                    is_readonly: property.modifiers().is_readonly(),
                    is_typed: property.type_id().is_some(),
                },
                hooks: hooks.clone(),
                attributes: attributes.clone(),
            });
        }
    }

    pub(super) fn push_internal_interfaces(
        &mut self,
        builder: &mut IrBuilder,
        declared: &HashSet<String>,
    ) {
        for (name, interfaces) in [
            ("Traversable", Vec::new()),
            ("Iterator", vec!["traversable".to_owned()]),
            ("IteratorAggregate", vec!["traversable".to_owned()]),
            ("ArrayAccess", Vec::new()),
            ("Throwable", Vec::new()),
            ("UnitEnum", Vec::new()),
            ("BackedEnum", Vec::new()),
            ("Stringable", Vec::new()),
        ] {
            let normalized = normalize_class_name(name);
            if declared.contains(&normalized) {
                continue;
            }
            builder.push_class(ClassEntry {
                id: crate::ids::ClassId::new(0),
                name: normalized,
                display_name: name.to_owned(),
                parent: None,
                parent_display_name: None,
                interfaces,
                methods: Vec::new(),
                properties: Vec::new(),
                constants: Vec::new(),
                enum_cases: Vec::new(),
                attributes: Vec::new(),
                enum_backing_type: None,
                constructor: None,
                flags: ClassFlags {
                    is_abstract: true,
                    is_final: false,
                    is_readonly: false,
                    is_interface: true,
                    is_enum: false,
                    is_trait: false,
                    is_conditional: false,
                },
                span: IrSpan::default(),
            });
        }
    }

    pub(super) fn lower_method_function(
        &mut self,
        builder: &mut IrBuilder,
        input: MethodFunctionInput<'_>,
    ) -> FunctionId {
        let span = span_from_range(self.file, input.signature.span());
        let function = builder.start_function(
            format!(
                "{}::{}",
                input.display_class_name, input.display_method_name
            ),
            FunctionFlags {
                is_method: true,
                is_generator: input.signature.flags().is_generator(),
                ..FunctionFlags::default()
            },
            span,
        );
        let attributes = self.lower_attributes_for_target_span(
            builder,
            AttributeTarget::Method,
            input.signature.span(),
        );
        builder.set_function_attributes(function, attributes);
        self.class_names
            .insert(function, input.display_class_name.to_owned());
        self.method_names
            .insert(function, input.display_method_name.to_owned());
        self.function_names.insert(
            function,
            format!(
                "{}::{}",
                input.display_class_name, input.display_method_name
            ),
        );
        self.namespace_names
            .insert(function, namespace_prefix(input.display_class_name));
        builder.set_return_type(
            function,
            self.lower_return_type(input.signature.return_type()),
        );
        builder.set_returns_by_ref(function, input.signature.by_ref_return());
        builder.intern_local(function, "this");
        builder.add_source_map(
            IrSourceMapTarget::Function { function },
            format!("hir:method:{}::{}", input.class_name, input.method_name),
            span,
        );
        for param in input.signature.parameters() {
            let local_name = local_name(param.name()).to_owned();
            let local = builder.intern_local(function, &local_name);
            let default = self.lower_param_default_with_class_constants(
                param,
                Some(input.class_name),
                input.class_constant_initializers,
                input.class_parents,
            );
            if param.default().is_some() && default.is_none() {
                self.unsupported(
                    UnsupportedFeature::AdvancedParameter,
                    param.span(),
                    "method parameter default is not a folded Semantic frontend constant expression",
                );
            }
            if self.param_default_triggers_implicit_nullable_deprecation(param, &default) {
                let span = span_from_range(self.file, param.span());
                self.record_early_diagnostic_origin(
                    input.main_function,
                    format!(
                        "hir:method:{}::{}:parameter:{}",
                        input.class_name,
                        input.method_name,
                        param.name()
                    ),
                    span,
                    IrDiagnosticSeverity::Deprecation,
                    "E_PHP_RUNTIME_IMPLICIT_NULLABLE_PARAMETER",
                    format!(
                        "{}::{}(): Implicitly marking parameter {} as nullable is deprecated, the explicit nullable type must be used instead",
                        input.display_class_name,
                        input.display_method_name,
                        param.name()
                    ),
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
            format!(
                "hir:method:{}::{}:body",
                input.class_name, input.method_name
            ),
            span,
        );
        let block =
            self.lower_auto_global_bindings(builder, function, block, input.signature.span(), span);
        let current = self.lower_constructor_property_promotions(
            builder,
            function,
            block,
            input.signature,
            input.class_name,
            input.method_name,
        );
        let current = self.lower_stmt_list(
            builder,
            function,
            current,
            self.method_body_statement_ids(input.signature),
        );
        if !builder.is_terminated(function, current) {
            builder.terminate_return(function, current, None, span);
        }
        function
    }

    pub(super) fn lower_constructor_property_promotions(
        &self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        signature: &FunctionSignature,
        class_name: &str,
        method_name: &str,
    ) -> BlockId {
        if method_name != "__construct" {
            return block;
        }
        let span = span_from_range(self.file, signature.span());
        let this_local = builder.intern_local(function, "this");
        let current = block;
        for param in signature.parameters() {
            if param.flags().promoted_property().is_none() {
                continue;
            }
            let property = local_name(param.name()).to_owned();
            let this = builder.alloc_register(function);
            let load_this = builder.emit(
                function,
                current,
                InstructionKind::LoadLocal {
                    dst: this,
                    local: this_local,
                },
                span,
            );
            builder.add_source_map(
                IrSourceMapTarget::Instruction {
                    function,
                    block: current,
                    instruction: load_this,
                },
                format!("hir:method:{class_name}::{method_name}:promotion:this"),
                span,
            );
            let param_local = builder.intern_local(function, local_name(param.name()));
            let value = builder.alloc_register(function);
            let load_value = builder.emit(
                function,
                current,
                InstructionKind::LoadLocal {
                    dst: value,
                    local: param_local,
                },
                span,
            );
            builder.add_source_map(
                IrSourceMapTarget::Instruction {
                    function,
                    block: current,
                    instruction: load_value,
                },
                format!(
                    "hir:method:{class_name}::{method_name}:promotion:{}",
                    param.name()
                ),
                span,
            );
            let dst = builder.alloc_register(function);
            let assign = builder.emit(
                function,
                current,
                InstructionKind::AssignProperty {
                    dst,
                    object: Operand::Register(this),
                    property,
                    value: Operand::Register(value),
                },
                span,
            );
            builder.add_source_map(
                IrSourceMapTarget::Instruction {
                    function,
                    block: current,
                    instruction: assign,
                },
                format!(
                    "hir:method:{class_name}::{method_name}:promotion:{}:assign",
                    param.name()
                ),
                span,
            );
        }
        current
    }

    pub(super) fn lower_property_hooks(
        &mut self,
        builder: &mut IrBuilder,
        class_name: &str,
        display_class_name: &str,
        property: &HirProperty,
    ) -> ClassPropertyHooks {
        let mut hooks = ClassPropertyHooks {
            backed: self.property_hooks_use_backing_storage(property),
            ..ClassPropertyHooks::default()
        };
        for hook in property.hooks() {
            let span = span_from_range(self.file, hook.span());
            let function = builder.start_function(
                format!(
                    "{class_name}::${}::{}",
                    property.items()[0].name(),
                    hook.kind()
                ),
                FunctionFlags {
                    is_method: true,
                    ..FunctionFlags::default()
                },
                span,
            );
            self.class_names
                .insert(function, display_class_name.to_owned());
            self.method_names.insert(
                function,
                format!("${}::{}", property.items()[0].name(), hook.kind()),
            );
            self.function_names.insert(
                function,
                format!(
                    "{display_class_name}::${}::{}",
                    property.items()[0].name(),
                    hook.kind()
                ),
            );
            self.namespace_names
                .insert(function, namespace_prefix(display_class_name));
            builder.intern_local(function, "this");
            if hook.kind() == "set" {
                let local = builder.intern_local(function, "value");
                builder.push_param(
                    function,
                    IrParam {
                        name: "value".to_owned(),
                        local,
                        required: true,
                        default: None,
                        type_: self.lower_runtime_type(property.type_id()),
                        by_ref: false,
                        variadic: false,
                        attributes: Vec::new(),
                    },
                );
            } else {
                builder.set_return_type(function, self.lower_runtime_type(property.type_id()));
            }
            builder.add_source_map(
                IrSourceMapTarget::Function { function },
                format!(
                    "hir:property-hook:{class_name}::${}:{}",
                    property.items()[0].name(),
                    hook.kind()
                ),
                span,
            );
            let block = builder.append_block(function);
            builder.add_source_map(
                IrSourceMapTarget::Block { function, block },
                format!(
                    "hir:property-hook:{class_name}::${}:{}:body",
                    property.items()[0].name(),
                    hook.kind()
                ),
                span,
            );
            let block =
                self.lower_auto_global_bindings(builder, function, block, hook.span(), span);
            let current = match hook.body() {
                HirPropertyHookBody::Expression => {
                    if let Some(expr) = self.outermost_expr_inside(hook.span()) {
                        if hook.kind() == "get" {
                            if let Some(value) =
                                self.lower_expr_to_register(builder, function, block, expr)
                            {
                                builder.terminate_return(
                                    function,
                                    value.block,
                                    Some(Operand::Register(value.register)),
                                    span,
                                );
                                value.block
                            } else {
                                block
                            }
                        } else {
                            self.lower_expr_stmt(builder, function, block, expr)
                        }
                    } else {
                        block
                    }
                }
                HirPropertyHookBody::Block => self.lower_stmt_list(
                    builder,
                    function,
                    block,
                    self.statement_ids_inside(hook.span()),
                ),
            };
            if !builder.is_terminated(function, current) {
                builder.terminate_return(function, current, None, span);
            }
            match hook.kind() {
                "get" => hooks.get = Some(function),
                "set" => hooks.set = Some(function),
                _ => {}
            }
        }
        hooks
    }

    pub(super) fn compose_trait_methods(
        &mut self,
        builder: &mut IrBuilder,
        input: TraitCompositionInput<'_>,
        methods: &mut Vec<ClassMethodEntry>,
        properties: &mut Vec<ClassPropertyEntry>,
    ) {
        let TraitCompositionInput {
            module,
            trait_class_likes,
            main_function,
            class_like_id,
            class_like,
            class_name,
            display_class_name,
            class_constant_initializers,
            class_parents,
        } = input;
        let mut candidates = Vec::<TraitMethodCandidate>::new();
        let mut removed = HashSet::<(String, String)>::new();
        let mut aliases = Vec::<TraitAliasSpec>::new();

        for member in class_like.members() {
            let Some(ClassLikeMemberId::TraitUse(trait_use_id)) = member.id() else {
                continue;
            };
            let Some(trait_use) = module.trait_uses().get(trait_use_id) else {
                continue;
            };
            for trait_name in trait_use.traits() {
                let display_trait_name = trait_name.source().to_owned();
                let trait_name = trait_resolution_name(trait_name);
                let Some((_trait_id, trait_class_like)) = trait_class_likes.get(&trait_name) else {
                    let span = span_from_range(
                        self.file,
                        self.span_for(SourceMappedId::from(trait_use_id)),
                    );
                    let owner_kind = match class_like.kind() {
                        ClassLikeKind::Class => MissingTraitOwnerKind::Class,
                        ClassLikeKind::Interface => MissingTraitOwnerKind::Interface,
                        ClassLikeKind::Trait => MissingTraitOwnerKind::Trait,
                        ClassLikeKind::Enum => MissingTraitOwnerKind::Enum,
                        ClassLikeKind::AnonymousClass => MissingTraitOwnerKind::AnonymousClass,
                    };
                    self.diagnostics.push(LoweringDiagnostic {
                        id: UnsupportedFeature::TraitRuntime.diagnostic_id().to_string(),
                        feature: UnsupportedFeature::TraitRuntime,
                        span,
                        message: format!(
                            "E_PHP_IR_TRAIT_NOT_FOUND: trait {trait_name} used by {class_name} is not declared"
                        ),
                        payload: Some(LoweringDiagnosticPayload::MissingTrait(
                            MissingTraitDiagnostic::new(
                                &trait_name,
                                display_trait_name,
                                class_name,
                                display_class_name,
                                owner_kind,
                                &self.options.source_path,
                                span,
                            ),
                        )),
                    });
                    continue;
                };
                self.collect_trait_composition_members(
                    builder,
                    TraitCompositionMembersInput {
                        module,
                        trait_class_like,
                        trait_name: &trait_name,
                        class_name,
                        display_class_name,
                        class_constant_initializers,
                        class_parents,
                    },
                    &mut candidates,
                    properties,
                );
            }
            for adaptation in trait_use.adaptations() {
                let method_name = normalize_method_name(adaptation.method().method());
                let trait_name = adaptation.method().trait_name().map(trait_resolution_name);
                match adaptation.kind() {
                    HirTraitAdaptationKind::Precedence { instead_of } => {
                        for excluded in instead_of {
                            removed.insert((trait_resolution_name(excluded), method_name.clone()));
                        }
                    }
                    HirTraitAdaptationKind::Alias { alias, visibility } => {
                        aliases.push(TraitAliasSpec {
                            trait_name,
                            method_name,
                            alias: alias.clone(),
                            visibility: visibility.as_deref().and_then(TraitVisibility::from_text),
                        });
                    }
                }
            }
        }

        for alias in &aliases {
            if alias.alias.is_none() {
                for candidate in &mut candidates {
                    if trait_alias_matches(alias, candidate)
                        && !removed.contains(&(
                            normalize_class_name(&candidate.trait_name),
                            normalize_method_name(&candidate.method_name),
                        ))
                        && let Some(visibility) = alias.visibility
                    {
                        visibility.apply(&mut candidate.flags);
                    }
                }
            }
        }

        let mut composed = candidates
            .into_iter()
            .filter(|candidate| {
                !removed.contains(&(
                    normalize_class_name(&candidate.trait_name),
                    normalize_method_name(&candidate.method_name),
                ))
            })
            .collect::<Vec<_>>();

        for alias in aliases.into_iter().filter(|alias| alias.alias.is_some()) {
            let alias_name = alias.alias.clone().unwrap_or_default();
            let matching = composed
                .iter()
                .filter(|candidate| trait_alias_matches(&alias, candidate))
                .cloned()
                .collect::<Vec<_>>();
            for mut candidate in matching {
                candidate.method_name = normalize_method_name(&alias_name);
                candidate.display_method_name = alias_name.clone();
                if let Some(visibility) = alias.visibility {
                    visibility.apply(&mut candidate.flags);
                }
                composed.push(candidate);
            }
        }

        let mut method_to_origins = HashMap::<String, Vec<String>>::new();
        for candidate in &composed {
            method_to_origins
                .entry(normalize_method_name(&candidate.method_name))
                .or_default()
                .push(candidate.trait_name.clone());
        }
        for (method, origins) in method_to_origins {
            let unique_origins = origins
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if unique_origins.len() > 1 {
                self.unsupported(
                    UnsupportedFeature::TraitRuntime,
                    self.span_for(SourceMappedId::from(class_like_id)),
                    format!(
                        "E_PHP_IR_TRAIT_METHOD_CONFLICT: method {method} is provided by {}",
                        unique_origins.join(", ")
                    ),
                );
                composed
                    .retain(|candidate| normalize_method_name(&candidate.method_name) != method);
            }
        }

        for candidate in composed {
            let function = self.lower_method_function(
                builder,
                MethodFunctionInput {
                    class_name,
                    method_name: &candidate.method_name,
                    display_class_name,
                    display_method_name: &candidate.display_method_name,
                    signature: &candidate.signature,
                    class_constant_initializers,
                    class_parents,
                    main_function,
                },
            );
            let attributes = self.lower_attributes_for_target_span(
                builder,
                AttributeTarget::Method,
                candidate.signature.span(),
            );
            methods.push(ClassMethodEntry {
                name: candidate.method_name,
                origin_class: candidate.display_trait_name,
                function,
                flags: candidate.flags,
                attributes,
            });
        }
    }

    pub(super) fn collect_trait_composition_members(
        &mut self,
        builder: &mut IrBuilder,
        input: TraitCompositionMembersInput<'_>,
        candidates: &mut Vec<TraitMethodCandidate>,
        properties: &mut Vec<ClassPropertyEntry>,
    ) {
        let module = input.module;
        let trait_class_like = input.trait_class_like;
        let trait_name = input.trait_name;
        let class_name = input.class_name;
        let display_class_name = input.display_class_name;
        let class_constant_initializers = input.class_constant_initializers;
        let class_parents = input.class_parents;
        let display_trait_name = trait_class_like
            .name()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| trait_name.to_owned());
        for member in trait_class_like.members() {
            match member.id() {
                Some(ClassLikeMemberId::Method(method_id)) => {
                    let Some(method) = module.methods().get(method_id).cloned() else {
                        continue;
                    };
                    let Some(method_name) = method.name().map(normalize_method_name) else {
                        continue;
                    };
                    let Some(signature) = method
                        .signature_index()
                        .and_then(|index| module.signatures().get(index))
                        .cloned()
                    else {
                        continue;
                    };
                    candidates.push(TraitMethodCandidate {
                        trait_name: normalize_class_name(trait_name),
                        display_trait_name: display_trait_name.clone(),
                        method_name,
                        display_method_name: method
                            .name()
                            .map(ToOwned::to_owned)
                            .unwrap_or_else(|| member.name().unwrap_or("method").to_owned()),
                        signature,
                        flags: class_method_flags_from_modifiers(method.modifiers()),
                    });
                }
                Some(ClassLikeMemberId::Property(property_id)) => {
                    let Some(property) = module.properties().get(property_id) else {
                        continue;
                    };
                    self.push_lowered_property_entries(
                        builder,
                        properties,
                        PropertyEntriesInput {
                            class_name,
                            display_class_name,
                            property,
                            class_constant_initializers,
                            class_parents,
                        },
                    );
                }
                Some(ClassLikeMemberId::ClassConstant(const_id)) => {
                    self.unsupported(
                        UnsupportedFeature::TraitRuntime,
                        self.span_for(SourceMappedId::from(const_id)),
                        "trait constants are not executable in the trait-composition trait-method composition layer",
                    );
                }
                Some(ClassLikeMemberId::TraitUse(trait_use_id)) => {
                    self.unsupported(
                        UnsupportedFeature::TraitRuntime,
                        self.span_for(SourceMappedId::from(trait_use_id)),
                        "nested trait uses are not executable in the trait-composition trait-method composition layer",
                    );
                }
                _ => {}
            }
        }
    }

    pub(super) fn global_const_initializers(&self) -> Vec<(ExprId, Option<IrConstant>)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id());
        let Some(module) = module else {
            return Vec::new();
        };
        let named_constants = predefined_constant_initializers();
        let class_likes = module
            .class_likes()
            .iter()
            .map(|(id, class_like)| (id, class_like.clone()))
            .collect::<Vec<_>>();
        let class_constant_initializers =
            collect_class_constant_initializers(module, &class_likes, &self.options.source_path);
        let class_parents = collect_class_parents(&class_likes, &self.options.source_path);
        module
            .const_exprs()
            .iter()
            .filter(|(_, const_expr)| {
                const_expr.context() == ConstExprContext::GlobalConstInitializer
                    && const_expr.is_allowed()
            })
            .map(|(_, const_expr)| {
                let value = constant_from_expr_with_class_constants(
                    module,
                    const_expr.expr_id(),
                    named_constants,
                    None,
                    &class_constant_initializers,
                    &class_parents,
                    &mut Vec::new(),
                )
                .or_else(|| {
                    const_expr
                        .folded_value()
                        .and_then(ir_constant_from_const_value)
                });
                (const_expr.expr_id(), value)
            })
            .collect()
    }

    pub(super) fn global_constant_initializer_map(&self) -> &HashMap<String, IrConstant> {
        self.global_constant_initializers
            .get_or_init(|| self.build_global_constant_initializer_map())
    }

    fn build_global_constant_initializer_map(&self) -> HashMap<String, IrConstant> {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return HashMap::new();
        };
        // One bulk clone of the shared predefined map; user-declared constants
        // and define() initializers are layered on top below.
        let mut map = predefined_constant_initializers().clone();
        let mut values = self.global_const_initializers().into_iter();
        for (name, value) in module
            .declaration_table()
            .entries()
            .iter()
            .filter(|entry| entry.kind() == DeclarationKind::Constant)
            .filter_map(|entry| {
                values
                    .next()
                    .and_then(|(_, value)| value.map(|value| (entry, value)))
            })
            .flat_map(|(entry, value)| {
                [
                    (entry.name().to_owned(), value.clone()),
                    (entry.fqn().canonical(NameKind::Constant), value),
                ]
            })
        {
            map.insert(name, value);
        }
        map.extend(define_constant_initializers_from_source(
            self.source_text.as_str(),
            &map,
        ));
        map
    }

    pub(super) fn lower_function_declarations(
        &mut self,
        builder: &mut IrBuilder,
        main_function: FunctionId,
    ) {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return;
        };
        let signatures = module.signatures().to_vec();
        let class_likes = module
            .class_likes()
            .iter()
            .map(|(id, class_like)| (id, class_like.clone()))
            .collect::<Vec<_>>();
        let class_constant_initializers =
            collect_class_constant_initializers(module, &class_likes, &self.options.source_path);
        let class_parents = collect_class_parents(&class_likes, &self.options.source_path);
        let mut pending_bodies = Vec::new();
        for signature in signatures {
            if signature.kind() != SignatureKind::Function {
                continue;
            }
            let Some(name) = signature.name() else {
                continue;
            };
            let source_name = name.to_string();
            let display_registered_name = qualified_function_name(module, &signature, name);
            let declaration_metadata = function_declaration_metadata(module, &signature);
            let registered_name = declaration_metadata
                .as_ref()
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| display_registered_name.clone());
            let span = span_from_range(self.file, signature.span());
            let function = builder.start_function(
                name,
                FunctionFlags {
                    is_generator: signature.flags().is_generator(),
                    ..FunctionFlags::default()
                },
                span,
            );
            let attributes = self.lower_attributes_for_target_span(
                builder,
                AttributeTarget::Function,
                signature.span(),
            );
            builder.set_function_attributes(function, attributes);
            self.function_names.insert(function, source_name.clone());
            self.namespace_names
                .insert(function, namespace_prefix(&display_registered_name));
            let normalized_name = normalize_function_name(&registered_name);
            let declaration_kind = declaration_metadata.map(|(_, kind)| kind);
            if declaration_kind == Some(DeclarationKind::ConditionalFunction) {
                self.conditional_function_declarations.push((
                    signature.span(),
                    normalized_name,
                    function,
                ));
            } else {
                builder.register_function_name(normalized_name, function);
            }
            builder.set_return_type(function, self.lower_return_type(signature.return_type()));
            builder.set_returns_by_ref(function, signature.by_ref_return());
            builder.add_source_map(
                IrSourceMapTarget::Function { function },
                format!("hir:function:{source_name}"),
                span,
            );
            for param in signature.parameters() {
                let local_name = local_name(param.name()).to_owned();
                let local = builder.intern_local(function, &local_name);
                let default = self.lower_param_default_with_class_constants(
                    param,
                    None,
                    &class_constant_initializers,
                    &class_parents,
                );
                if param.default().is_some() && default.is_none() {
                    self.unsupported(
                        UnsupportedFeature::AdvancedParameter,
                        param.span(),
                        "parameter default is not a folded Semantic frontend constant expression",
                    );
                }
                if default == Some(IrConstant::Null)
                    && self.param_type_triggers_implicit_nullable_deprecation(param)
                {
                    let span = span_from_range(self.file, param.span());
                    self.record_early_diagnostic_origin(
                        main_function,
                        format!("hir:function:{name}:parameter:{}", param.name()),
                        span,
                        IrDiagnosticSeverity::Deprecation,
                        "E_PHP_RUNTIME_IMPLICIT_NULLABLE_PARAMETER",
                        format!(
                            "{}(): Implicitly marking parameter {} as nullable is deprecated, the explicit nullable type must be used instead",
                            source_name,
                            param.name()
                        ),
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
            pending_bodies.push((function, signature, source_name, span));
        }
        for (function, signature, name, span) in pending_bodies {
            let block = builder.append_block(function);
            builder.add_source_map(
                IrSourceMapTarget::Block { function, block },
                format!("hir:function:{name}:body"),
                span,
            );
            let block =
                self.lower_auto_global_bindings(builder, function, block, signature.span(), span);
            let current = self.lower_stmt_list(builder, function, block, signature.body().to_vec());
            if !builder.is_terminated(function, current) {
                builder.terminate_return(function, current, None, span);
            }
        }
    }

    pub(super) fn type_accepts_null(&self, type_id: TypeId) -> bool {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return false;
        };
        let Some(ty) = module.types().get(type_id) else {
            return false;
        };
        match ty.kind() {
            HirTypeKind::Nullable { .. } | HirTypeKind::Null | HirTypeKind::Mixed => true,
            HirTypeKind::Union { members, .. } => {
                members.iter().any(|member| self.type_accepts_null(*member))
            }
            HirTypeKind::Dnf { members } => {
                members.iter().any(|member| self.type_accepts_null(*member))
            }
            _ => false,
        }
    }

    pub(super) fn lower_property_default(
        &self,
        default: Option<ConstExprId>,
        current_class: Option<&str>,
        current_class_display: Option<&str>,
        class_constants: &ClassConstantInitializerMap,
        class_parents: &ClassParentMap,
    ) -> Option<IrConstant> {
        let default = default?;
        if let Some(value) = self.lower_const_expr_magic_constant(default, current_class_display) {
            return Some(value);
        }
        let value = self.lower_const_expr_value(
            default,
            |context| {
                matches!(
                    context,
                    ConstExprContext::PropertyDefault | ConstExprContext::PromotedPropertyDefault
                )
            },
            current_class,
            class_constants,
            class_parents,
        );
        match value {
            Some(IrConstant::NamedConstant(_)) | Some(IrConstant::ClassConstant { .. }) => None,
            other => other,
        }
    }

    pub(super) fn lower_class_constant_value(
        &self,
        value: Option<ConstExprId>,
        current_class: &str,
        current_class_display: &str,
        class_constants: &ClassConstantInitializerMap,
        class_parents: &ClassParentMap,
    ) -> Option<IrConstant> {
        let value = value?;
        if let Some(value) =
            self.lower_const_expr_magic_constant(value, Some(current_class_display))
        {
            return Some(value);
        }
        let value = self.lower_const_expr_value(
            value,
            |context| matches!(context, ConstExprContext::ClassConstInitializer),
            Some(current_class),
            class_constants,
            class_parents,
        );
        match value {
            Some(IrConstant::NamedConstant(_)) | Some(IrConstant::ClassConstant { .. }) => None,
            other => other,
        }
    }

    pub(super) fn lower_parameter_attributes(
        &self,
        builder: &mut IrBuilder,
        parameter_attributes: &[ParameterAttribute],
    ) -> Vec<AttributeEntry> {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return Vec::new();
        };
        let ids: Vec<_> = module
            .attributes()
            .iter()
            .filter_map(|(id, attribute)| {
                if attribute.target() != AttributeTarget::Parameter {
                    return None;
                }
                let span = self.frontend.database().source_map().span(id)?;
                parameter_attributes
                    .iter()
                    .any(|parameter_attribute| range_contains(parameter_attribute.span(), span))
                    .then_some(id)
            })
            .collect();
        self.lower_attribute_ids(builder, &ids)
    }

    pub(super) fn lower_attributes_for_target_span(
        &self,
        builder: &mut IrBuilder,
        target: AttributeTarget,
        span: TextRange,
    ) -> Vec<AttributeEntry> {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return Vec::new();
        };
        let ids: Vec<_> = module
            .attributes()
            .iter()
            .filter_map(|(id, attribute)| {
                if attribute.target() != target {
                    return None;
                }
                let attribute_span = self.frontend.database().source_map().span(id)?;
                range_contains(span, attribute_span).then_some(id)
            })
            .collect();
        self.lower_attribute_ids(builder, &ids)
    }

    pub(super) fn lower_return_type(
        &self,
        return_type: Option<&ReturnType>,
    ) -> Option<IrReturnType> {
        self.lower_runtime_type(return_type.map(|return_type| return_type.type_id()))
    }

    pub(super) fn lower_runtime_type(&self, type_id: Option<TypeId>) -> Option<IrReturnType> {
        let type_id = type_id?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let ty = module.types().get(type_id)?;
        let source_form = ty.source_form();
        match ty.kind() {
            HirTypeKind::Builtin(BuiltinType::Int) => Some(IrReturnType::Int),
            HirTypeKind::Builtin(BuiltinType::Float) => Some(IrReturnType::Float),
            HirTypeKind::Builtin(BuiltinType::String) => Some(IrReturnType::String),
            HirTypeKind::Builtin(BuiltinType::Bool) => Some(IrReturnType::Bool),
            HirTypeKind::Builtin(BuiltinType::Array) => Some(IrReturnType::Array),
            HirTypeKind::Builtin(BuiltinType::Callable) => Some(IrReturnType::Callable),
            HirTypeKind::Builtin(BuiltinType::Iterable) => Some(IrReturnType::Iterable),
            HirTypeKind::Builtin(BuiltinType::Object) => Some(IrReturnType::Object),
            HirTypeKind::Null => Some(IrReturnType::Null),
            HirTypeKind::Void => Some(IrReturnType::Void),
            HirTypeKind::Mixed => Some(IrReturnType::Mixed),
            HirTypeKind::Never => Some(IrReturnType::Never),
            HirTypeKind::False => Some(IrReturnType::False),
            HirTypeKind::True => Some(IrReturnType::True),
            HirTypeKind::Named { name, resolved } => Some(IrReturnType::Class {
                name: resolved
                    .as_ref()
                    .map(|resolved| resolved.canonical(NameKind::ClassLike))
                    .unwrap_or_else(|| name.original().to_owned()),
                display_name: (!source_form.is_empty()).then(|| source_form.to_owned()),
            }),
            HirTypeKind::Nullable { inner, .. } => {
                let inner = self.lower_runtime_type(Some(*inner))?;
                Some(IrReturnType::Nullable {
                    inner: Box::new(inner),
                })
            }
            HirTypeKind::Union {
                members,
                normalized_from_nullable,
            } if *normalized_from_nullable => {
                let mut non_null = None;
                for member in members {
                    let ty = self.lower_runtime_type(Some(*member))?;
                    if ty == IrReturnType::Null {
                        continue;
                    }
                    if non_null.replace(ty).is_some() {
                        return None;
                    }
                }
                non_null.map(|inner| IrReturnType::Nullable {
                    inner: Box::new(inner),
                })
            }
            HirTypeKind::Union { members, .. } => Some(IrReturnType::Union {
                members: self.lower_runtime_type_members(members)?,
            }),
            HirTypeKind::Intersection { members } => Some(IrReturnType::Intersection {
                members: self.lower_runtime_type_members(members)?,
            }),
            HirTypeKind::Dnf { members } => Some(IrReturnType::Dnf {
                members: self.lower_runtime_type_members(members)?,
            }),
            _ => None,
        }
    }

    pub(super) fn lower_runtime_type_members(
        &self,
        members: &[TypeId],
    ) -> Option<Vec<IrReturnType>> {
        members
            .iter()
            .map(|member| self.lower_runtime_type(Some(*member)))
            .collect()
    }

    pub(super) fn statement_id_for_span(&self, span: TextRange) -> Option<StmtId> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        module.statements().iter().find_map(|(stmt_id, _)| {
            (self.span_for(SourceMappedId::from(stmt_id)) == span).then_some(stmt_id)
        })
    }

    pub(super) fn statement_ids_inside(&self, span: TextRange) -> Vec<StmtId> {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return Vec::new();
        };
        let mut statements = module
            .statements()
            .iter()
            .filter_map(|(stmt_id, _)| {
                let stmt_span = self.span_for(SourceMappedId::from(stmt_id));
                (stmt_span != span && range_contains(span, stmt_span))
                    .then_some((stmt_span, stmt_id))
            })
            .collect::<Vec<_>>();
        statements.sort_by_key(|(stmt_span, _)| {
            (stmt_span.start().to_usize(), stmt_span.end().to_usize())
        });
        let mut outermost = Vec::new();
        for (stmt_span, stmt_id) in statements {
            if outermost
                .iter()
                .any(|(outer_span, _)| range_contains(*outer_span, stmt_span))
            {
                continue;
            }
            outermost.push((stmt_span, stmt_id));
        }
        outermost.into_iter().map(|(_, stmt_id)| stmt_id).collect()
    }

    pub(super) fn method_body_statement_ids(&self, signature: &FunctionSignature) -> Vec<StmtId> {
        if !signature.body().is_empty() {
            return signature.body().to_vec();
        }
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return Vec::new();
        };
        module
            .statements()
            .iter()
            .filter_map(|(stmt_id, statement)| {
                let stmt_span = self.span_for(SourceMappedId::from(stmt_id));
                match statement.kind() {
                    HirStmtKind::Block { statements }
                        if stmt_span != signature.span()
                            && range_contains(signature.span(), stmt_span) =>
                    {
                        Some((
                            stmt_span.end().to_usize() - stmt_span.start().to_usize(),
                            statements.clone(),
                        ))
                    }
                    _ => None,
                }
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, statements)| statements)
            .unwrap_or_else(|| self.statement_ids_inside(signature.span()))
    }

    pub(super) fn property_hooks_use_backing_storage(&self, property: &HirProperty) -> bool {
        let Some(item) = property.items().first() else {
            return false;
        };
        let needle = format!("->{}", local_name(item.name()));
        property.hooks().iter().any(|hook| {
            self.source_text
                .slice(hook.span())
                .is_some_and(|source| source.contains(&needle))
        })
    }

    pub(super) fn signature_for_expr(
        &self,
        span: TextRange,
        kind: SignatureKind,
    ) -> Option<&FunctionSignature> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        module
            .signatures()
            .iter()
            .find(|signature| signature.kind() == kind && signature.span() == span)
            .or_else(|| {
                module
                    .signatures()
                    .iter()
                    .filter(|signature| {
                        signature.kind() == kind
                            && (range_contains(span, signature.span())
                                || range_contains(signature.span(), span)
                                || ranges_overlap(span, signature.span()))
                    })
                    .min_by_key(|signature| {
                        signature.span().end().to_usize() - signature.span().start().to_usize()
                    })
            })
    }

    pub(super) fn function_like_uses_variable(&self, span: TextRange, variable_name: &str) -> bool {
        self.variable_spans.get(variable_name).is_some_and(|spans| {
            spans
                .iter()
                .any(|expr_span| range_contains(span, *expr_span))
        })
    }

    pub(super) fn explicit_capture_specs(&self, span: TextRange) -> Vec<CaptureSpec> {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return Vec::new();
        };
        module
            .scopes()
            .iter()
            .find_map(|(_, scope)| {
                (scope.span() == span).then(|| {
                    scope
                        .function_like()
                        .map(|function_like| {
                            function_like
                                .captures()
                                .iter()
                                .map(|capture| CaptureSpec {
                                    name: local_name(capture.name()).to_owned(),
                                    by_ref: capture.mode() == CaptureMode::ExplicitByReference,
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
            })
            .unwrap_or_default()
    }

    pub(super) fn implicit_arrow_capture_specs(
        &self,
        body: Option<ExprId>,
        params: &[Parameter],
    ) -> Vec<CaptureSpec> {
        let Some(body) = body else {
            return Vec::new();
        };
        let body_span = self.span_for(SourceMappedId::from(body));
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return Vec::new();
        };
        let params = params
            .iter()
            .map(|param| local_name(param.name()).to_owned())
            .collect::<BTreeSet<_>>();
        let names = module
            .expressions()
            .iter()
            .filter_map(|(expr_id, expr)| {
                let span = self.span_for(SourceMappedId::from(expr_id));
                if !range_contains(body_span, span) {
                    return None;
                }
                match expr.kind() {
                    HirExprKind::Variable { name } => {
                        let name = local_name(name).to_owned();
                        (!params.contains(&name)).then_some(name)
                    }
                    _ => None,
                }
            })
            .collect::<BTreeSet<_>>();
        names
            .into_iter()
            .map(|name| CaptureSpec {
                name,
                by_ref: false,
            })
            .collect()
    }

    pub(super) fn static_local_specs(
        &self,
        stmt_id: StmtId,
        initializers: &[ExprId],
    ) -> Vec<StaticLocalSpec> {
        let stmt_span = self.span_for(SourceMappedId::from(stmt_id));
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return Vec::new();
        };
        let mut variables = module
            .scopes()
            .iter()
            .flat_map(|(_, scope)| scope.statics().iter())
            .filter_map(|binding| {
                let variable = binding.variable();
                range_contains(stmt_span, variable.span()).then(|| {
                    (
                        local_name(variable.name()).to_owned(),
                        variable.span().start().to_usize(),
                        variable.span().end().to_usize(),
                    )
                })
            })
            .collect::<Vec<_>>();
        variables.sort_by_key(|(_, start, _)| *start);
        variables
            .iter()
            .enumerate()
            .map(|(index, (name, _, end))| {
                let next_start = variables
                    .get(index + 1)
                    .map(|(_, start, _)| *start)
                    .unwrap_or_else(|| stmt_span.end().to_usize());
                let initializer = initializers.iter().copied().find(|expr| {
                    let span = self.span_for(SourceMappedId::from(*expr));
                    let start = span.start().to_usize();
                    start >= *end && start < next_start
                });
                StaticLocalSpec {
                    name: name.clone(),
                    initializer,
                }
            })
            .collect()
    }
}
