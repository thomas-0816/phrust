//! Semantic validation for declaration modifiers.

use std::collections::HashMap;

use crate::diagnostics::{
    DiagnosticId, DiagnosticLabel, DiagnosticPhase, DiagnosticSeverity, SemanticDiagnostic,
};
use crate::hir::{Modifier, ModifierOccurrence, ModifierSet, Visibility};
use php_ast::{
    AstNode, AstToken, ClassConstDecl, ClassDecl, EnumDecl, FunctionDecl, InterfaceDecl,
    MethodDecl, Param, PropertyDecl, TokenView, TraitDecl, descendant_nodes, modifier_tokens,
    syntax_child_tokens,
};
use php_source::TextRange;
use php_syntax::SyntaxNode;

/// Returns modifier diagnostics for one source file.
#[must_use]
pub fn check_source_file(source_file: &SyntaxNode) -> Vec<SemanticDiagnostic> {
    let mut diagnostics = Vec::new();

    for class_decl in descendant_nodes::<ClassDecl<'_>>(source_file) {
        check_modifier_target(
            "class",
            class_decl.text_range(),
            collect_named_modifiers(class_decl.syntax()),
            ModifierRules::class_like(ClassLikeKind::Class),
            &mut diagnostics,
        );
    }
    for interface_decl in descendant_nodes::<InterfaceDecl<'_>>(source_file) {
        check_modifier_target(
            "interface",
            interface_decl.text_range(),
            collect_named_modifiers(interface_decl.syntax()),
            ModifierRules::class_like(ClassLikeKind::Interface),
            &mut diagnostics,
        );
    }
    for trait_decl in descendant_nodes::<TraitDecl<'_>>(source_file) {
        check_modifier_target(
            "trait",
            trait_decl.text_range(),
            collect_named_modifiers(trait_decl.syntax()),
            ModifierRules::class_like(ClassLikeKind::Trait),
            &mut diagnostics,
        );
    }
    for enum_decl in descendant_nodes::<EnumDecl<'_>>(source_file) {
        check_modifier_target(
            "enum",
            enum_decl.text_range(),
            collect_named_modifiers(enum_decl.syntax()),
            ModifierRules::class_like(ClassLikeKind::Enum),
            &mut diagnostics,
        );
    }
    for function_decl in descendant_nodes::<FunctionDecl<'_>>(source_file) {
        check_modifier_target(
            "function",
            function_decl.text_range(),
            collect_named_modifiers(function_decl.syntax()),
            ModifierRules::function(),
            &mut diagnostics,
        );
    }
    for method_decl in descendant_nodes::<MethodDecl<'_>>(source_file) {
        check_modifier_target(
            "method",
            method_decl.text_range(),
            collect_named_modifiers(method_decl.syntax()),
            ModifierRules::method(),
            &mut diagnostics,
        );
    }
    for property_decl in descendant_nodes::<PropertyDecl<'_>>(source_file) {
        if property_decl.is_enum_case() {
            continue;
        }
        let mut modifiers = collect_named_modifiers(property_decl.syntax());
        let hooked = has_direct_token(property_decl.syntax(), "{");
        if hooked {
            modifiers.push(ModifierOccurrence::new(
                Modifier::HookRelated,
                property_decl.text_range(),
            ));
        }
        check_modifier_target(
            "property",
            property_decl.text_range(),
            modifiers,
            if hooked {
                ModifierRules::hooked_property()
            } else {
                ModifierRules::property()
            },
            &mut diagnostics,
        );
    }
    for class_const_decl in descendant_nodes::<ClassConstDecl<'_>>(source_file) {
        check_modifier_target(
            "class constant",
            class_const_decl.text_range(),
            collect_named_modifiers(class_const_decl.syntax()),
            ModifierRules::class_const(),
            &mut diagnostics,
        );
    }
    for param in descendant_nodes::<Param<'_>>(source_file) {
        check_modifier_target(
            "parameter",
            param.text_range(),
            collect_parameter_modifiers(param.syntax()),
            ModifierRules::parameter(),
            &mut diagnostics,
        );
    }

    diagnostics
}

fn check_modifier_target(
    target: &str,
    target_span: TextRange,
    modifiers: Vec<ModifierOccurrence>,
    rules: ModifierRules,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) {
    if modifiers.is_empty() {
        return;
    }

    let mut set = ModifierSet::new();
    for modifier in modifiers {
        set.push(modifier);
    }

    check_duplicates(target, &set, diagnostics);
    check_multiple_visibility(target, &set, diagnostics);
    check_allowed_modifiers(target, &set, &rules, diagnostics);
    check_incompatible_combinations(target, target_span, &set, &rules, diagnostics);
}

fn check_duplicates(target: &str, set: &ModifierSet, diagnostics: &mut Vec<SemanticDiagnostic>) {
    let mut seen: HashMap<Modifier, TextRange> = HashMap::new();
    for occurrence in set.occurrences() {
        let modifier = occurrence.modifier();
        if matches!(modifier, Modifier::Promoted | Modifier::HookRelated) {
            continue;
        }
        if let Some(previous) = seen.insert(modifier, occurrence.span()) {
            diagnostics.push(
                SemanticDiagnostic::with_span(
                    DiagnosticId::DuplicateModifier,
                    DiagnosticSeverity::Error,
                    DiagnosticPhase::ModifierValidation,
                    format!(
                        "duplicate `{}` modifier on {target}",
                        occurrence.modifier().as_str()
                    ),
                    occurrence.span(),
                )
                .with_label(DiagnosticLabel::new(previous, "first modifier is here")),
            );
        }
    }
}

fn check_multiple_visibility(
    target: &str,
    set: &ModifierSet,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) {
    let normal: Vec<_> = set
        .occurrences()
        .iter()
        .copied()
        .filter(|occurrence| occurrence.modifier().visibility().is_some())
        .collect();
    if normal.len() > 1 {
        diagnostics.push(
            SemanticDiagnostic::with_span(
                DiagnosticId::IncompatibleModifiers,
                DiagnosticSeverity::Error,
                DiagnosticPhase::ModifierValidation,
                format!("multiple visibility modifiers on {target}"),
                normal[1].span(),
            )
            .with_label(DiagnosticLabel::new(
                normal[0].span(),
                "first visibility is here",
            )),
        );
    }

    let set_visibility: Vec<_> = set
        .occurrences()
        .iter()
        .copied()
        .filter(|occurrence| occurrence.modifier().set_visibility().is_some())
        .collect();
    if set_visibility.len() > 1 {
        diagnostics.push(
            SemanticDiagnostic::with_span(
                DiagnosticId::IncompatibleModifiers,
                DiagnosticSeverity::Error,
                DiagnosticPhase::ModifierValidation,
                format!("multiple property set-visibility modifiers on {target}"),
                set_visibility[1].span(),
            )
            .with_label(DiagnosticLabel::new(
                set_visibility[0].span(),
                "first set visibility is here",
            )),
        );
    }
}

fn check_allowed_modifiers(
    target: &str,
    set: &ModifierSet,
    rules: &ModifierRules,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) {
    for occurrence in set.occurrences() {
        let modifier = occurrence.modifier();
        if matches!(modifier, Modifier::Promoted | Modifier::HookRelated) {
            continue;
        }
        if !rules.allows(modifier) {
            diagnostics.push(SemanticDiagnostic::with_span(
                DiagnosticId::IncompatibleModifiers,
                DiagnosticSeverity::Error,
                DiagnosticPhase::ModifierValidation,
                format!(
                    "`{}` modifier is not allowed on {target}",
                    modifier.as_str()
                ),
                occurrence.span(),
            ));
        }
    }
}

fn check_incompatible_combinations(
    target: &str,
    target_span: TextRange,
    set: &ModifierSet,
    rules: &ModifierRules,
    diagnostics: &mut Vec<SemanticDiagnostic>,
) {
    if set.is_abstract()
        && set.is_final()
        && let Some(span) = first_span(set, Modifier::Final)
    {
        diagnostics.push(SemanticDiagnostic::with_span(
            DiagnosticId::IncompatibleModifiers,
            DiagnosticSeverity::Error,
            DiagnosticPhase::ModifierValidation,
            format!("{target} cannot be both abstract and final"),
            span,
        ));
    }

    if rules.abstract_private_invalid
        && set.is_abstract()
        && set.visibility() == Some(Visibility::Private)
        && let Some(span) = first_span(set, Modifier::Private)
    {
        diagnostics.push(SemanticDiagnostic::with_span(
            DiagnosticId::IncompatibleModifiers,
            DiagnosticSeverity::Error,
            DiagnosticPhase::ModifierValidation,
            "abstract method cannot be private",
            span,
        ));
    }

    if rules.static_readonly_invalid
        && set.is_static()
        && set.is_readonly()
        && let Some(span) = first_span(set, Modifier::Readonly)
    {
        diagnostics.push(SemanticDiagnostic::with_span(
            DiagnosticId::IncompatibleModifiers,
            DiagnosticSeverity::Error,
            DiagnosticPhase::ModifierValidation,
            "static property cannot be readonly",
            span,
        ));
    }

    if set.set_visibility().is_some() {
        if !rules.allow_set_visibility {
            if let Some(span) = first_set_visibility_span(set) {
                diagnostics.push(SemanticDiagnostic::with_span(
                    DiagnosticId::IncompatibleModifiers,
                    DiagnosticSeverity::Error,
                    DiagnosticPhase::ModifierValidation,
                    format!("property set visibility is not allowed on {target}"),
                    span,
                ));
            }
        } else if set.visibility().is_none() && !set.uses_var() {
            if let Some(span) = first_set_visibility_span(set) {
                diagnostics.push(SemanticDiagnostic::with_span(
                    DiagnosticId::IncompatibleModifiers,
                    DiagnosticSeverity::Error,
                    DiagnosticPhase::ModifierValidation,
                    "property set visibility requires an explicit property visibility",
                    span,
                ));
            }
        } else if !valid_set_visibility_order(set.visibility(), set.set_visibility())
            && let Some(span) = first_set_visibility_span(set)
        {
            diagnostics.push(SemanticDiagnostic::with_span(
                DiagnosticId::IncompatibleModifiers,
                DiagnosticSeverity::Error,
                DiagnosticPhase::ModifierValidation,
                "property set visibility must not be less restrictive than get visibility",
                span,
            ));
        }
    }

    if rules.require_promoted_visibility_for_readonly
        && set.is_readonly()
        && set.visibility().is_none()
        && let Some(span) = first_span(set, Modifier::Readonly)
    {
        diagnostics.push(SemanticDiagnostic::with_span(
            DiagnosticId::IncompatibleModifiers,
            DiagnosticSeverity::Error,
            DiagnosticPhase::ModifierValidation,
            "readonly promoted parameter requires property visibility",
            span,
        ));
    }

    if set.is_hook_related() && !rules.allow_hook_related {
        diagnostics.push(SemanticDiagnostic::with_span(
            DiagnosticId::IncompatibleModifiers,
            DiagnosticSeverity::Error,
            DiagnosticPhase::ModifierValidation,
            format!("property hook body is not allowed on {target}"),
            target_span,
        ));
    }
}

fn collect_named_modifiers(node: &SyntaxNode) -> Vec<ModifierOccurrence> {
    modifier_tokens(node)
        .filter_map(|token| occurrence_from_token(token))
        .collect()
}

fn collect_parameter_modifiers(node: &SyntaxNode) -> Vec<ModifierOccurrence> {
    let mut modifiers = collect_named_modifiers(node);
    if modifiers
        .iter()
        .any(|occurrence| occurrence.modifier().visibility().is_some())
    {
        modifiers.push(ModifierOccurrence::new(
            Modifier::Promoted,
            node.text_range(),
        ));
    }
    modifiers.extend(syntax_child_tokens(node).filter_map(|token| {
        let name = token.kind().name();
        if name == "&" || name == "T_ELLIPSIS" {
            occurrence_from_syntax_token(token)
        } else {
            None
        }
    }));
    modifiers
}

fn occurrence_from_token(token: TokenView<'_>) -> Option<ModifierOccurrence> {
    let modifier = Modifier::from_token_name(&token.kind().name())?;
    Some(ModifierOccurrence::new(modifier, token.text_range()))
}

fn occurrence_from_syntax_token(token: &php_syntax::SyntaxToken) -> Option<ModifierOccurrence> {
    let modifier = Modifier::from_token_name(&token.kind().name())?;
    Some(ModifierOccurrence::new(modifier, token.text_range()))
}

fn has_direct_token(node: &SyntaxNode, token_name: &str) -> bool {
    syntax_child_tokens(node).any(|token| token.kind().name() == token_name)
}

fn first_span(set: &ModifierSet, modifier: Modifier) -> Option<TextRange> {
    set.occurrences()
        .iter()
        .find(|occurrence| occurrence.modifier() == modifier)
        .map(|occurrence| occurrence.span())
}

fn first_set_visibility_span(set: &ModifierSet) -> Option<TextRange> {
    set.occurrences()
        .iter()
        .find(|occurrence| occurrence.modifier().set_visibility().is_some())
        .map(|occurrence| occurrence.span())
}

fn valid_set_visibility_order(
    visibility: Option<Visibility>,
    set_visibility: Option<Visibility>,
) -> bool {
    match (visibility, set_visibility) {
        (_, None) | (None, _) => true,
        (Some(get), Some(set)) => visibility_rank(set) >= visibility_rank(get),
    }
}

fn visibility_rank(visibility: Visibility) -> u8 {
    match visibility {
        Visibility::Public => 0,
        Visibility::Protected => 1,
        Visibility::Private => 2,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClassLikeKind {
    Class,
    Interface,
    Trait,
    Enum,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ModifierRules {
    allow_abstract: bool,
    allow_final: bool,
    allow_static: bool,
    allow_readonly: bool,
    allow_visibility: bool,
    allow_set_visibility: bool,
    allow_var: bool,
    allow_by_ref: bool,
    allow_variadic: bool,
    allow_hook_related: bool,
    abstract_private_invalid: bool,
    static_readonly_invalid: bool,
    require_promoted_visibility_for_readonly: bool,
}

impl ModifierRules {
    const fn class_like(kind: ClassLikeKind) -> Self {
        Self {
            allow_abstract: matches!(kind, ClassLikeKind::Class),
            allow_final: matches!(kind, ClassLikeKind::Class),
            allow_static: false,
            allow_readonly: matches!(kind, ClassLikeKind::Class),
            allow_visibility: false,
            allow_set_visibility: false,
            allow_var: false,
            allow_by_ref: false,
            allow_variadic: false,
            allow_hook_related: false,
            abstract_private_invalid: false,
            static_readonly_invalid: false,
            require_promoted_visibility_for_readonly: false,
        }
    }

    const fn function() -> Self {
        Self {
            allow_abstract: false,
            allow_final: false,
            allow_static: false,
            allow_readonly: false,
            allow_visibility: false,
            allow_set_visibility: false,
            allow_var: false,
            allow_by_ref: true,
            allow_variadic: false,
            allow_hook_related: false,
            abstract_private_invalid: false,
            static_readonly_invalid: false,
            require_promoted_visibility_for_readonly: false,
        }
    }

    const fn method() -> Self {
        Self {
            allow_abstract: true,
            allow_final: true,
            allow_static: true,
            allow_readonly: false,
            allow_visibility: true,
            allow_set_visibility: false,
            allow_var: false,
            allow_by_ref: true,
            allow_variadic: false,
            allow_hook_related: false,
            abstract_private_invalid: true,
            static_readonly_invalid: false,
            require_promoted_visibility_for_readonly: false,
        }
    }

    const fn property() -> Self {
        Self {
            allow_abstract: false,
            // Final properties are legal since PHP 8.4.
            allow_final: true,
            allow_static: true,
            allow_readonly: true,
            allow_visibility: true,
            allow_set_visibility: true,
            allow_var: true,
            allow_by_ref: false,
            allow_variadic: false,
            allow_hook_related: true,
            abstract_private_invalid: false,
            static_readonly_invalid: true,
            require_promoted_visibility_for_readonly: false,
        }
    }

    /// Hooked properties (PHP 8.4) may additionally be declared abstract.
    const fn hooked_property() -> Self {
        Self {
            allow_abstract: true,
            ..Self::property()
        }
    }

    const fn class_const() -> Self {
        Self {
            allow_abstract: false,
            allow_final: true,
            allow_static: false,
            allow_readonly: false,
            allow_visibility: true,
            allow_set_visibility: false,
            allow_var: false,
            allow_by_ref: false,
            allow_variadic: false,
            allow_hook_related: false,
            abstract_private_invalid: false,
            static_readonly_invalid: false,
            require_promoted_visibility_for_readonly: false,
        }
    }

    const fn parameter() -> Self {
        Self {
            allow_abstract: false,
            allow_final: false,
            allow_static: false,
            allow_readonly: true,
            allow_visibility: true,
            allow_set_visibility: true,
            allow_var: false,
            allow_by_ref: true,
            allow_variadic: true,
            allow_hook_related: false,
            abstract_private_invalid: false,
            static_readonly_invalid: false,
            require_promoted_visibility_for_readonly: true,
        }
    }

    const fn allows(self, modifier: Modifier) -> bool {
        match modifier {
            Modifier::Abstract => self.allow_abstract,
            Modifier::Final => self.allow_final,
            Modifier::Static => self.allow_static,
            Modifier::Readonly => self.allow_readonly,
            Modifier::Public | Modifier::Protected | Modifier::Private => self.allow_visibility,
            Modifier::PublicSet | Modifier::ProtectedSet | Modifier::PrivateSet => {
                self.allow_set_visibility
            }
            Modifier::Var => self.allow_var,
            Modifier::ByRef => self.allow_by_ref,
            Modifier::Variadic => self.allow_variadic,
            Modifier::Promoted => true,
            Modifier::HookRelated => self.allow_hook_related,
        }
    }
}
