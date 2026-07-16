//! Semantic frontend frontend to runtime IR lowering skeleton.

use crate::builder::IrBuilder;
use crate::constants::{IrConstant, IrConstantArrayEntry};
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::function::{FunctionFlags, IrCapture, IrParam, IrReturnType};
use crate::ids::{BlockId, FileId, FunctionId, LocalId, RegId};
use crate::instruction::{
    CallableKind, CastKind, ClosureCaptureArg, CompareOp, InstructionKind, IrCallArgValueKind,
    IrCallDimTarget, IrCallPropertyDimTarget, IrCallPropertyTarget, IrDiagnosticSeverity,
};
use crate::literal_text::{
    InterpolatedDim, InterpolatedPart, heredoc_literal_body, interpolated_literal_parts,
    quoted_literal_body,
};
use crate::module::{
    AttributeEntry, ClassConstantEntry, ClassConstantFlags, ClassConstantReference, ClassEntry,
    ClassEnumBackingType, ClassEnumCaseEntry, ClassFlags, ClassMethodEntry, ClassMethodFlags,
    ClassPropertyEntry, ClassPropertyFlags, ClassPropertyHooks, DeferredConstArrayEntry,
    DeferredConstExpr, NamedConstantReference, display_class_name, normalize_class_name,
};
use crate::operand::Operand;
use crate::source_map::{IrSourceMapTarget, IrSpan};
use crate::verify::verify_unit;
use php_semantics::hir::{
    AttributeId, AttributeTarget, BuiltinType, ClassLikeId, ClassLikeKind, ClassLikeMemberId,
    ConstExprContext, ConstExprId, ConstValue, DeclareValue, DefaultValueRef, ExprId,
    FunctionSignature, HirCallArg, HirClassLike, HirExprKind, HirMatchArm, HirModule,
    HirNameResolution, HirPropertyHookBody, HirStmtKind, HirSwitchCase, HirTraitAdaptationKind,
    HirTypeKind, MagicMethodKind, ModifierSet, NameKind, Parameter, ParameterAttribute, ReturnType,
    SignatureKind, StmtId, TopLevelItemKind, TypeId, Visibility,
};
use php_semantics::scopes::CaptureMode;
use php_semantics::symbols::declarations::DeclarationKind;
use php_semantics::symbols::imports::ImportKind;
use php_semantics::symbols::resolution::{ResolveContext, ResolvedName};
use php_semantics::{FrontendResult, SourceMappedId};
use php_source::{BytePos, TextRange};

mod consts;
mod context;
mod control_flow;
mod declarations;
mod diagnostics;
mod expression_targets;
mod expressions;
mod session;
mod statements;

pub use context::{LoweringContext, LoweringOptions, LoweringResult};
pub use diagnostics::*;
use expressions::cast_kind;
pub use session::{lower_compilation_session, lower_frontend_result};

const AUTO_GLOBAL_NAMES: &[&str] = &[
    "argc", "argv", "_SERVER", "_ENV", "_GET", "_POST", "_COOKIE", "_FILES", "_REQUEST", "_SESSION",
];

fn local_name(name: &str) -> &str {
    if let Some(inner) = name
        .strip_prefix("${")
        .and_then(|name| name.strip_suffix('}'))
        && !inner.is_empty()
        && inner.bytes().all(|byte| byte.is_ascii_digit())
    {
        return inner;
    }
    name.strip_prefix('$').unwrap_or(name)
}

fn zero_arg_variable_call_name(name: &str) -> Option<&str> {
    let name = local_name(name);
    let callable_name = name.strip_suffix("()")?;
    if callable_name.is_empty() || callable_name.contains('(') || callable_name.contains(')') {
        return None;
    }
    Some(callable_name)
}

fn trait_resolution_name(name: &HirNameResolution) -> String {
    normalize_class_name(
        name.resolved()
            .or_else(|| name.fallback())
            .unwrap_or_else(|| name.source()),
    )
}

fn interface_resolution_name(name: &HirNameResolution) -> String {
    normalize_class_name(
        name.resolved()
            .or_else(|| name.fallback())
            .unwrap_or_else(|| name.source()),
    )
}

fn class_resolution_display_name(
    module: &HirModule,
    resolution: &HirNameResolution,
    class_range: TextRange,
    class_likes: &[(ClassLikeId, HirClassLike)],
    source_path: &str,
) -> String {
    source_imported_class_resolution_display_name(module, resolution)
        .or_else(|| imported_class_resolution_display_name(module, resolution))
        .or_else(|| declared_class_resolution_display_name(class_likes, source_path, resolution))
        .or_else(|| source_qualified_class_resolution_display_name(resolution))
        .or_else(|| {
            source_namespaced_class_resolution_display_name(module, resolution, class_range)
        })
        .or_else(|| source_unqualified_class_resolution_display_name(resolution))
        .or_else(|| {
            resolution
                .resolved()
                .or_else(|| resolution.fallback())
                .map(display_class_name)
        })
        .unwrap_or_else(|| display_class_name(resolution.source()))
}

/// Preserves the source spelling for a plain unqualified reference whose
/// resolution is just its normalization (for example an undeclared global
/// parent class that autoloading must observe with its PHP-visible casing).
fn source_unqualified_class_resolution_display_name(
    resolution: &HirNameResolution,
) -> Option<String> {
    let source = resolution.source().trim_start_matches('\\');
    if source.is_empty() || source.contains('\\') {
        return None;
    }
    let resolved = resolution.resolved().or_else(|| resolution.fallback())?;
    (normalize_class_name(source) == normalize_class_name(resolved))
        .then(|| display_class_name(source))
}

fn source_qualified_class_resolution_display_name(
    resolution: &HirNameResolution,
) -> Option<String> {
    let source = resolution.source().trim_start_matches('\\');
    if !source.contains('\\') {
        return None;
    }
    let resolved = resolution.resolved().or_else(|| resolution.fallback())?;
    (normalize_class_name(source) == normalize_class_name(resolved))
        .then(|| display_class_name(resolution.source()))
}

fn source_namespaced_class_resolution_display_name(
    module: &HirModule,
    resolution: &HirNameResolution,
    class_range: TextRange,
) -> Option<String> {
    let source = resolution.source().trim_start_matches('\\');
    if source.is_empty() || source.contains('\\') {
        return None;
    }
    let resolved = resolution.resolved().or_else(|| resolution.fallback())?;
    module.namespaces().values().find_map(|namespace| {
        if !range_contains(namespace.span(), class_range) {
            return None;
        }
        let namespace_name = namespace.name()?.text();
        if namespace_name.is_empty() {
            return None;
        }
        let display = format!("{namespace_name}\\{source}");
        (normalize_class_name(&display) == normalize_class_name(resolved)).then_some(display)
    })
}

fn source_imported_class_resolution_display_name(
    module: &HirModule,
    resolution: &HirNameResolution,
) -> Option<String> {
    let source = resolution.source().trim_start_matches('\\');
    let first_part = source.split('\\').next().unwrap_or_default();
    if first_part.is_empty() {
        return None;
    }
    module.namespaces().values().find_map(|namespace| {
        let import = namespace
            .imports()
            .lookup(ImportKind::ClassLike, first_part)?;
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
    })
}

fn imported_class_resolution_display_name(
    module: &HirModule,
    resolution: &HirNameResolution,
) -> Option<String> {
    let canonical = normalize_class_name(resolution.resolved().or_else(|| resolution.fallback())?);
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

fn declared_class_resolution_display_name(
    class_likes: &[(ClassLikeId, HirClassLike)],
    source_path: &str,
    resolution: &HirNameResolution,
) -> Option<String> {
    let normalized = normalize_class_name(resolution.resolved().or_else(|| resolution.fallback())?);
    class_likes.iter().find_map(|(_, class_like)| {
        (class_like_normalized_name(class_like, source_path)? == normalized)
            .then(|| class_like_display_name(class_like, &normalized))
    })
}

fn trait_alias_matches(
    alias: &declarations::TraitAliasSpec,
    candidate: &declarations::TraitMethodCandidate,
) -> bool {
    normalize_method_name(&alias.method_name) == normalize_method_name(&candidate.method_name)
        && alias
            .trait_name
            .as_deref()
            .is_none_or(|trait_name| normalize_class_name(trait_name) == candidate.trait_name)
}

fn class_method_flags_from_modifiers(modifiers: &ModifierSet) -> ClassMethodFlags {
    ClassMethodFlags {
        is_static: modifiers.is_static(),
        is_private: modifiers
            .visibility()
            .is_some_and(|visibility| visibility == Visibility::Private),
        is_protected: modifiers
            .visibility()
            .is_some_and(|visibility| visibility == Visibility::Protected),
        is_abstract: modifiers.is_abstract(),
        has_body: true,
        is_final: modifiers.is_final(),
    }
}

fn normalize_function_name(name: &str) -> String {
    name.trim_start_matches('\\').to_ascii_lowercase()
}

fn normalized_function_basename(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

fn namespace_prefix(name: &str) -> String {
    name.trim_start_matches('\\')
        .rsplit_once('\\')
        .map_or_else(String::new, |(namespace, _)| namespace.to_owned())
}

fn namespace_name_for_span(module: &HirModule, span: TextRange) -> Option<String> {
    module.namespaces().values().find_map(|namespace| {
        range_contains(namespace.span(), span).then(|| {
            namespace
                .name()
                .map_or_else(String::new, |name| name.text().to_owned())
        })
    })
}

fn qualified_function_name(
    module: &HirModule,
    signature: &FunctionSignature,
    short_name: &str,
) -> String {
    for namespace in module.namespaces().values() {
        let owns_signature = namespace.items().iter().any(|item| {
            item.kind() == TopLevelItemKind::Function && item.span() == signature.span()
        });
        if !owns_signature {
            continue;
        }
        return namespace.name().map_or_else(
            || short_name.to_owned(),
            |name| format!("{}\\{short_name}", name.text()),
        );
    }
    short_name.to_owned()
}

fn is_top_level_function_signature(module: &HirModule, signature: &FunctionSignature) -> bool {
    module.namespaces().values().any(|namespace| {
        namespace.items().iter().any(|item| {
            item.kind() == TopLevelItemKind::Function && item.span() == signature.span()
        })
    })
}

fn function_declaration_metadata(
    module: &HirModule,
    signature: &FunctionSignature,
) -> Option<(String, DeclarationKind)> {
    let short_name = signature.name()?;
    let expected_basename = normalize_function_name(short_name);
    let mut matching_entries = module.declaration_table().entries().iter().filter(|entry| {
        normalized_function_basename(&entry.fqn().canonical(NameKind::Function))
            == expected_basename
            && matches!(
                entry.kind(),
                DeclarationKind::Function | DeclarationKind::ConditionalFunction
            )
            && range_contains(signature.span(), entry.span())
    });
    let is_top_level = is_top_level_function_signature(module, signature);
    matching_entries
        .find(|entry| entry.kind() == DeclarationKind::ConditionalFunction)
        .or_else(|| matching_entries.next())
        .map(|entry| {
            let kind = if is_top_level {
                entry.kind()
            } else {
                DeclarationKind::ConditionalFunction
            };
            (entry.fqn().canonical(NameKind::Function), kind)
        })
}

fn class_declaration_kind(
    module: &HirModule,
    class_like: &HirClassLike,
    class_span: TextRange,
    normalized_name: &str,
) -> Option<DeclarationKind> {
    let name = class_like
        .fqn()
        .map(|name| name.canonical(NameKind::ClassLike))
        .or_else(|| class_like.name().map(normalize_class_name))
        .map(|name| normalize_class_name(&name))
        .unwrap_or_else(|| normalized_name.to_owned());
    module
        .declaration_table()
        .entries()
        .iter()
        .find(|entry| {
            matches!(
                entry.kind(),
                DeclarationKind::Class
                    | DeclarationKind::Interface
                    | DeclarationKind::Trait
                    | DeclarationKind::Enum
                    | DeclarationKind::ConditionalClassLike
            ) && entry.fqn().canonical(NameKind::ClassLike) == name
                && range_contains(class_span, entry.span())
        })
        .map(|entry| entry.kind())
}

fn conditional_class_declaration_name_for_span(
    module: &HirModule,
    span: TextRange,
) -> Option<String> {
    module
        .declaration_table()
        .entries()
        .iter()
        .find(|entry| {
            entry.kind() == DeclarationKind::ConditionalClassLike
                && range_contains(span, entry.span())
        })
        .map(|entry| normalize_class_name(&entry.fqn().canonical(NameKind::ClassLike)))
}

fn is_internal_throwable_class(normalized: &str) -> bool {
    matches!(
        normalized,
        "throwable"
            | "exception"
            | "error"
            | "typeerror"
            | "valueerror"
            | "argumentcounterror"
            | "fibererror"
            | "jsonexception"
            | "pdoexception"
            | "logicexception"
            | "badfunctioncallexception"
            | "badmethodcallexception"
            | "domainexception"
            | "invalidargumentexception"
            | "lengthexception"
            | "outofrangeexception"
            | "runtimeexception"
            | "outofboundsexception"
            | "overflowexception"
            | "rangeexception"
            | "underflowexception"
            | "unexpectedvalueexception"
    )
}

fn normalize_method_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn language_constant(name: &str) -> Option<IrConstant> {
    let normalized = name.trim_start_matches('\\');
    if normalized.eq_ignore_ascii_case("null") {
        Some(IrConstant::Null)
    } else if normalized.eq_ignore_ascii_case("true") {
        Some(IrConstant::Bool(true))
    } else if normalized.eq_ignore_ascii_case("false") {
        Some(IrConstant::Bool(false))
    } else if normalized.eq_ignore_ascii_case("PHP_INT_MAX") {
        Some(IrConstant::Int(isize::MAX as i64))
    } else if normalized.eq_ignore_ascii_case("PHP_INT_MIN") {
        Some(IrConstant::Int(isize::MIN as i64))
    } else if normalized.eq_ignore_ascii_case("PHP_INT_SIZE") {
        Some(IrConstant::Int(std::mem::size_of::<isize>() as i64))
    } else if normalized.eq_ignore_ascii_case("E_ERROR") {
        Some(IrConstant::Int(php_std::constants::E_ERROR))
    } else if normalized.eq_ignore_ascii_case("E_WARNING") {
        Some(IrConstant::Int(php_std::constants::E_WARNING))
    } else if normalized.eq_ignore_ascii_case("E_PARSE") {
        Some(IrConstant::Int(php_std::constants::E_PARSE))
    } else if normalized.eq_ignore_ascii_case("E_NOTICE") {
        Some(IrConstant::Int(php_std::constants::E_NOTICE))
    } else if normalized.eq_ignore_ascii_case("E_CORE_ERROR") {
        Some(IrConstant::Int(php_std::constants::E_CORE_ERROR))
    } else if normalized.eq_ignore_ascii_case("E_CORE_WARNING") {
        Some(IrConstant::Int(php_std::constants::E_CORE_WARNING))
    } else if normalized.eq_ignore_ascii_case("E_COMPILE_ERROR") {
        Some(IrConstant::Int(php_std::constants::E_COMPILE_ERROR))
    } else if normalized.eq_ignore_ascii_case("E_COMPILE_WARNING") {
        Some(IrConstant::Int(php_std::constants::E_COMPILE_WARNING))
    } else if normalized.eq_ignore_ascii_case("E_USER_ERROR") {
        Some(IrConstant::Int(php_std::constants::E_USER_ERROR))
    } else if normalized.eq_ignore_ascii_case("E_USER_WARNING") {
        Some(IrConstant::Int(php_std::constants::E_USER_WARNING))
    } else if normalized.eq_ignore_ascii_case("E_USER_NOTICE") {
        Some(IrConstant::Int(php_std::constants::E_USER_NOTICE))
    } else if normalized.eq_ignore_ascii_case("E_STRICT") {
        Some(IrConstant::Int(php_std::constants::E_STRICT))
    } else if normalized.eq_ignore_ascii_case("E_RECOVERABLE_ERROR") {
        Some(IrConstant::Int(php_std::constants::E_RECOVERABLE_ERROR))
    } else if normalized.eq_ignore_ascii_case("E_DEPRECATED") {
        Some(IrConstant::Int(php_std::constants::E_DEPRECATED))
    } else if normalized.eq_ignore_ascii_case("E_USER_DEPRECATED") {
        Some(IrConstant::Int(php_std::constants::E_USER_DEPRECATED))
    } else if normalized.eq_ignore_ascii_case("E_ALL") {
        Some(IrConstant::Int(php_std::constants::E_ALL))
    } else {
        None
    }
}

fn source_dir(path: &str) -> String {
    std::path::Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn span_from_range(file: FileId, range: TextRange) -> IrSpan {
    IrSpan::from_text_range(file, range)
}

fn expr_stmt_is_side_effect_free_bare_variable(module: &HirModule, expr: ExprId) -> bool {
    let Some(expression) = module.expressions().get(expr) else {
        return false;
    };
    matches!(expression.kind(), HirExprKind::Variable { .. })
}

fn range_contains(outer: TextRange, inner: TextRange) -> bool {
    outer.start().to_usize() <= inner.start().to_usize()
        && outer.end().to_usize() >= inner.end().to_usize()
}

fn ranges_overlap(lhs: TextRange, rhs: TextRange) -> bool {
    lhs.start().to_usize() < rhs.end().to_usize() && rhs.start().to_usize() < lhs.end().to_usize()
}

fn range_overlap_len(lhs: TextRange, rhs: TextRange) -> usize {
    let start = lhs.start().to_usize().max(rhs.start().to_usize());
    let end = lhs.end().to_usize().min(rhs.end().to_usize());
    end.saturating_sub(start)
}

fn collect_class_constant_initializers(
    module: &HirModule,
    class_likes: &[(ClassLikeId, HirClassLike)],
    source_path: &str,
) -> declarations::ClassConstantInitializerMap {
    class_likes
        .iter()
        .filter_map(|(_, class_like)| {
            let class_name = class_like_normalized_name(class_like, source_path)?;
            let constants = class_like
                .members()
                .iter()
                .filter_map(|member| {
                    let Some(ClassLikeMemberId::ClassConstant(const_id)) = member.id() else {
                        return None;
                    };
                    let constant = module.class_consts().get(const_id)?;
                    Some((constant.name()?.to_owned(), constant.value()?))
                })
                .collect::<HashMap<_, _>>();
            Some((class_name, constants))
        })
        .collect()
}

fn collect_class_parents(
    class_likes: &[(ClassLikeId, HirClassLike)],
    source_path: &str,
) -> declarations::ClassParentMap {
    class_likes
        .iter()
        .filter_map(|(_, class_like)| {
            let class_name = class_like_normalized_name(class_like, source_path)?;
            let parent = matches!(
                class_like.kind(),
                ClassLikeKind::Class | ClassLikeKind::AnonymousClass
            )
            .then(|| {
                class_like.extends().first().map(|name| {
                    normalize_class_name(
                        name.resolved()
                            .or_else(|| name.fallback())
                            .unwrap_or_else(|| name.source()),
                    )
                })
            })
            .flatten();
            Some((class_name, parent))
        })
        .collect()
}

fn class_like_normalized_name(class_like: &HirClassLike, source_path: &str) -> Option<String> {
    class_like
        .fqn()
        .map(|name| name.canonical(NameKind::ClassLike))
        .or_else(|| class_like.name().map(normalize_class_name))
        .or_else(|| {
            class_like
                .anonymous_id()
                .map(|anonymous_id| anonymous_class_ir_name(anonymous_id, source_path))
        })
        .map(|name| normalize_class_name(&name))
}

fn class_like_display_name(class_like: &HirClassLike, fallback: &str) -> String {
    class_like
        .fqn()
        .map(|name| {
            name.parts()
                .iter()
                .map(|part| part.original())
                .collect::<Vec<_>>()
                .join("\\")
        })
        .or_else(|| class_like.name().map(ToOwned::to_owned))
        .or_else(|| class_like.anonymous_id().map(ToOwned::to_owned))
        .unwrap_or_else(|| display_class_name(fallback))
}

fn anonymous_class_ir_name(anonymous_id: &str, source_path: &str) -> String {
    let suffix = anonymous_id
        .chars()
        .map(|ch| {
            if ch == '_' || ch.is_ascii_alphanumeric() {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!(
        "__phrust_anonymous_{:016x}_{suffix}",
        stable_source_hash(source_path)
    )
}

fn stable_source_hash(source_path: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in source_path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn constant_from_expr_with_names(
    module: &HirModule,
    expr_id: ExprId,
    named_constants: &HashMap<String, IrConstant>,
) -> Option<IrConstant> {
    constant_from_expr_with_class_constants(
        module,
        expr_id,
        named_constants,
        None,
        &HashMap::new(),
        &HashMap::new(),
        &mut Vec::new(),
    )
}

fn define_constant_initializers_from_hir(
    module: &HirModule,
    named_constants: &HashMap<String, IrConstant>,
) -> HashMap<String, IrConstant> {
    let definitions = module
        .expressions()
        .iter()
        .filter_map(|(_, expression)| {
            let HirExprKind::Call {
                callee: Some(callee),
                args,
            } = expression.kind()
            else {
                return None;
            };
            let callee = module.expressions().get(*callee)?;
            let HirExprKind::Name { resolution } = callee.kind() else {
                return None;
            };
            if !resolution.source().eq_ignore_ascii_case("define") || args.len() < 2 {
                return None;
            }
            Some((args[0].value, args[1].value))
        })
        .collect::<Vec<_>>();

    let mut resolved = HashMap::new();
    loop {
        let mut available = named_constants.clone();
        available.extend(resolved.clone());
        let before = resolved.len();
        for (name, value) in &definitions {
            let Some(IrConstant::String(name)) =
                constant_from_expr_with_names(module, *name, &available)
            else {
                continue;
            };
            let Some(value) = constant_from_expr_with_names(module, *value, &available) else {
                continue;
            };
            resolved.insert(name, value);
        }
        if resolved.len() == before {
            return resolved;
        }
    }
}

fn constant_from_overlapping_default_expr(
    frontend: &FrontendResult,
    module: &HirModule,
    default: &DefaultValueRef,
    named_constants: &HashMap<String, IrConstant>,
    current_class: Option<&str>,
    class_constants: &declarations::ClassConstantInitializerMap,
    class_parents: &declarations::ClassParentMap,
) -> Option<IrConstant> {
    module
        .expressions()
        .iter()
        .filter_map(|(expr_id, _)| {
            let span = frontend.database().source_map().span(expr_id)?;
            if !ranges_overlap(default.span(), span) {
                return None;
            }
            Some((span, expr_id))
        })
        .max_by_key(|(span, _)| {
            (
                range_overlap_len(default.span(), *span),
                span.end()
                    .to_usize()
                    .saturating_sub(span.start().to_usize()),
            )
        })
        .and_then(|(_, expr_id)| {
            constant_from_expr_with_runtime_constants(
                module,
                expr_id,
                RuntimeConstantInputs {
                    named_constants,
                    current_class,
                    class_constants,
                    class_parents,
                },
                &|_| None,
                &mut Vec::new(),
            )
        })
}

fn constant_from_expr_with_class_constants(
    module: &HirModule,
    expr_id: ExprId,
    named_constants: &HashMap<String, IrConstant>,
    current_class: Option<&str>,
    class_constants: &declarations::ClassConstantInitializerMap,
    class_parents: &declarations::ClassParentMap,
    visiting_class_constants: &mut Vec<(String, String)>,
) -> Option<IrConstant> {
    let expr = module.expressions().get(expr_id)?;
    match expr.kind() {
        HirExprKind::Literal { text } => literal_constant(text),
        HirExprKind::Name { resolution } => language_constant(resolution.source())
            .or_else(|| named_constant_value(named_constants, resolution)),
        HirExprKind::Unary { operator, expr } => {
            let value = constant_from_expr_with_class_constants(
                module,
                (*expr)?,
                named_constants,
                current_class,
                class_constants,
                class_parents,
                visiting_class_constants,
            )?;
            match operator.as_str() {
                "parenthesized" | "+" => Some(value),
                "-" => negate_ir_constant(value),
                "~" => bitnot_ir_constant(value),
                _ => cast_kind(operator).and_then(|kind| ir_constant_cast(kind, value)),
            }
        }
        HirExprKind::Cast { kind, expr } => {
            let value = constant_from_expr_with_class_constants(
                module,
                (*expr)?,
                named_constants,
                current_class,
                class_constants,
                class_parents,
                visiting_class_constants,
            )?;
            ir_constant_cast(cast_kind(kind)?, value)
        }
        HirExprKind::Binary {
            operator,
            left,
            right,
        } => {
            let left = constant_from_expr_with_class_constants(
                module,
                (*left)?,
                named_constants,
                current_class,
                class_constants,
                class_parents,
                visiting_class_constants,
            )?;
            let right = constant_from_expr_with_class_constants(
                module,
                (*right)?,
                named_constants,
                current_class,
                class_constants,
                class_parents,
                visiting_class_constants,
            )?;
            binary_ir_constant(operator, left, right)
        }
        HirExprKind::Ternary {
            condition,
            if_true,
            if_false,
        } => {
            let condition_value = constant_from_expr_with_class_constants(
                module,
                (*condition)?,
                named_constants,
                current_class,
                class_constants,
                class_parents,
                visiting_class_constants,
            )?;
            if ir_constant_truthy(&condition_value)? {
                let selected = if_true.unwrap_or((*condition)?);
                constant_from_expr_with_class_constants(
                    module,
                    selected,
                    named_constants,
                    current_class,
                    class_constants,
                    class_parents,
                    visiting_class_constants,
                )
            } else {
                constant_from_expr_with_class_constants(
                    module,
                    (*if_false)?,
                    named_constants,
                    current_class,
                    class_constants,
                    class_parents,
                    visiting_class_constants,
                )
            }
        }
        HirExprKind::Array { elements } => {
            let mut entries = Vec::with_capacity(elements.len());
            for element_id in elements {
                let element = module.expressions().get(*element_id)?;
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
                        entries.push(IrConstantArrayEntry {
                            key: key.and_then(|key| {
                                constant_from_expr_with_class_constants(
                                    module,
                                    key,
                                    named_constants,
                                    current_class,
                                    class_constants,
                                    class_parents,
                                    visiting_class_constants,
                                )
                            }),
                            value: constant_from_expr_with_class_constants(
                                module,
                                (*value)?,
                                named_constants,
                                current_class,
                                class_constants,
                                class_parents,
                                visiting_class_constants,
                            )?,
                        });
                    }
                    _ => entries.push(IrConstantArrayEntry {
                        key: None,
                        value: constant_from_expr_with_class_constants(
                            module,
                            *element_id,
                            named_constants,
                            current_class,
                            class_constants,
                            class_parents,
                            visiting_class_constants,
                        )?,
                    }),
                }
            }
            Some(IrConstant::Array(entries))
        }
        HirExprKind::StaticAccess { target, member } => {
            let target_class = class_constant_initializer_target_class(
                module,
                (*target)?,
                current_class,
                class_parents,
            )?;
            let member = class_constant_initializer_member_name(module, (*member)?)?;
            if member.eq_ignore_ascii_case("class") {
                return Some(IrConstant::String(
                    class_constant_initializer_target_display_class(
                        module,
                        (*target)?,
                        current_class,
                        class_parents,
                    )
                    .unwrap_or_else(|| target_class.clone()),
                ));
            }
            resolve_class_constant_initializer(
                module,
                &target_class,
                &member,
                named_constants,
                class_constants,
                class_parents,
                visiting_class_constants,
            )
        }
        HirExprKind::DimFetch { receiver, dim } => {
            let receiver = constant_from_expr_with_class_constants(
                module,
                (*receiver)?,
                named_constants,
                current_class,
                class_constants,
                class_parents,
                visiting_class_constants,
            )?;
            let dim = constant_from_expr_with_class_constants(
                module,
                (*dim)?,
                named_constants,
                current_class,
                class_constants,
                class_parents,
                visiting_class_constants,
            )?;
            ir_constant_dim_fetch(receiver, &dim)
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct RuntimeConstantInputs<'a> {
    named_constants: &'a HashMap<String, IrConstant>,
    current_class: Option<&'a str>,
    class_constants: &'a declarations::ClassConstantInitializerMap,
    class_parents: &'a declarations::ClassParentMap,
}

fn rt_inputs<'a>(
    names: (&'a HashMap<String, IrConstant>, Option<&'a str>),
    class_maps: (
        &'a declarations::ClassConstantInitializerMap,
        &'a declarations::ClassParentMap,
    ),
) -> RuntimeConstantInputs<'a> {
    RuntimeConstantInputs {
        named_constants: names.0,
        current_class: names.1,
        class_constants: class_maps.0,
        class_parents: class_maps.1,
    }
}

fn constant_from_expr_with_runtime_constants(
    module: &HirModule,
    expr_id: ExprId,
    inputs: RuntimeConstantInputs<'_>,
    magic_constant: &impl Fn(ExprId) -> Option<IrConstant>,
    visiting_class_constants: &mut Vec<(String, String)>,
) -> Option<IrConstant> {
    if let Some(value) = magic_constant(expr_id) {
        return Some(value);
    }
    let expr = module.expressions().get(expr_id)?;
    match expr.kind() {
        HirExprKind::Literal { text } => literal_constant(text),
        HirExprKind::Name { resolution } => language_constant(resolution.source())
            .or_else(|| named_constant_value(inputs.named_constants, resolution))
            .or_else(|| runtime_named_constant_name(resolution).map(IrConstant::NamedConstant)),
        HirExprKind::Unary { operator, expr } => {
            let value = constant_from_expr_with_runtime_constants(
                module,
                (*expr)?,
                inputs,
                magic_constant,
                visiting_class_constants,
            )?;
            match operator.as_str() {
                "parenthesized" | "+" => Some(value),
                "-" => negate_ir_constant(value),
                "~" => bitnot_ir_constant(value),
                _ => cast_kind(operator).and_then(|kind| ir_constant_cast(kind, value)),
            }
        }
        HirExprKind::Cast { kind, expr } => {
            let value = constant_from_expr_with_runtime_constants(
                module,
                (*expr)?,
                inputs,
                magic_constant,
                visiting_class_constants,
            )?;
            ir_constant_cast(cast_kind(kind)?, value)
        }
        HirExprKind::Binary {
            operator,
            left,
            right,
        } => {
            let left = constant_from_expr_with_runtime_constants(
                module,
                (*left)?,
                inputs,
                magic_constant,
                visiting_class_constants,
            )?;
            let right = constant_from_expr_with_runtime_constants(
                module,
                (*right)?,
                inputs,
                magic_constant,
                visiting_class_constants,
            )?;
            binary_ir_constant(operator, left, right)
        }
        HirExprKind::Ternary {
            condition,
            if_true,
            if_false,
        } => {
            let condition_value = constant_from_expr_with_runtime_constants(
                module,
                (*condition)?,
                inputs,
                magic_constant,
                visiting_class_constants,
            )?;
            if ir_constant_truthy(&condition_value)? {
                let selected = if_true.unwrap_or((*condition)?);
                constant_from_expr_with_runtime_constants(
                    module,
                    selected,
                    inputs,
                    magic_constant,
                    visiting_class_constants,
                )
            } else {
                constant_from_expr_with_runtime_constants(
                    module,
                    (*if_false)?,
                    inputs,
                    magic_constant,
                    visiting_class_constants,
                )
            }
        }
        HirExprKind::Array { elements } => {
            let mut entries = Vec::with_capacity(elements.len());
            for element_id in elements {
                let element = module.expressions().get(*element_id)?;
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
                        entries.push(IrConstantArrayEntry {
                            key: key.and_then(|key| {
                                constant_from_expr_with_runtime_constants(
                                    module,
                                    key,
                                    inputs,
                                    magic_constant,
                                    visiting_class_constants,
                                )
                            }),
                            value: constant_from_expr_with_runtime_constants(
                                module,
                                (*value)?,
                                inputs,
                                magic_constant,
                                visiting_class_constants,
                            )?,
                        });
                    }
                    _ => entries.push(IrConstantArrayEntry {
                        key: None,
                        value: constant_from_expr_with_runtime_constants(
                            module,
                            *element_id,
                            inputs,
                            magic_constant,
                            visiting_class_constants,
                        )?,
                    }),
                }
            }
            Some(IrConstant::Array(entries))
        }
        HirExprKind::StaticAccess { target, member } => {
            let target_class = class_constant_initializer_target_class(
                module,
                (*target)?,
                inputs.current_class,
                inputs.class_parents,
            )?;
            let member = class_constant_initializer_member_name(module, (*member)?)?;
            let display_class_name = class_constant_initializer_target_display_class(
                module,
                (*target)?,
                inputs.current_class,
                inputs.class_parents,
            )
            .unwrap_or_else(|| target_class.clone());
            if member.eq_ignore_ascii_case("class") {
                return Some(IrConstant::String(display_class_name));
            }
            resolve_class_constant_initializer(
                module,
                &target_class,
                &member,
                inputs.named_constants,
                inputs.class_constants,
                inputs.class_parents,
                visiting_class_constants,
            )
            .or(Some(IrConstant::ClassConstant {
                class_name: target_class,
                display_class_name,
                constant_name: member,
            }))
        }
        HirExprKind::DimFetch { receiver, dim } => {
            let receiver = constant_from_expr_with_runtime_constants(
                module,
                (*receiver)?,
                inputs,
                magic_constant,
                visiting_class_constants,
            )?;
            let dim = constant_from_expr_with_runtime_constants(
                module,
                (*dim)?,
                inputs,
                magic_constant,
                visiting_class_constants,
            )?;
            ir_constant_dim_fetch(receiver, &dim)
        }
        _ => None,
    }
}

fn class_constant_initializer_target_class(
    module: &HirModule,
    expr_id: ExprId,
    current_class: Option<&str>,
    class_parents: &declarations::ClassParentMap,
) -> Option<String> {
    let expr = module.expressions().get(expr_id)?;
    let HirExprKind::Name { resolution } = expr.kind() else {
        return None;
    };
    let source = resolution.source();
    if source.eq_ignore_ascii_case("self") || source.eq_ignore_ascii_case("static") {
        return current_class.map(normalize_class_name);
    }
    if source.eq_ignore_ascii_case("parent") {
        return current_class
            .map(normalize_class_name)
            .and_then(|class| class_parents.get(&class).cloned().flatten());
    }
    Some(normalize_class_name(
        resolution
            .resolved()
            .or_else(|| resolution.fallback())
            .unwrap_or(source),
    ))
}

fn class_constant_initializer_target_display_class(
    module: &HirModule,
    expr_id: ExprId,
    current_class: Option<&str>,
    class_parents: &declarations::ClassParentMap,
) -> Option<String> {
    let expr = module.expressions().get(expr_id)?;
    let HirExprKind::Name { resolution } = expr.kind() else {
        return None;
    };
    let source = resolution.source();
    if source.eq_ignore_ascii_case("self") || source.eq_ignore_ascii_case("static") {
        return current_class.map(ToOwned::to_owned);
    }
    if source.eq_ignore_ascii_case("parent") {
        return current_class
            .map(normalize_class_name)
            .and_then(|class| class_parents.get(&class).cloned().flatten());
    }
    resolved_class_like_display_name(module, resolution)
        .or_else(|| Some(source.trim_start_matches('\\').to_owned()))
}

fn resolved_class_like_display_name(
    module: &HirModule,
    resolution: &HirNameResolution,
) -> Option<String> {
    let resolved = normalize_class_name(resolution.resolved()?);
    module.namespaces().values().find_map(|namespace| {
        namespace.resolved_names().iter().find_map(|record| {
            if record.context() != ResolveContext::ClassLike
                || record.source().original() != resolution.source()
            {
                return None;
            }
            let ResolvedName::FullyQualified(name) = record.result() else {
                return None;
            };
            (name.canonical(NameKind::ClassLike) == resolved).then(|| {
                name.parts()
                    .iter()
                    .map(|part| part.original())
                    .collect::<Vec<_>>()
                    .join("\\")
            })
        })
    })
}

fn runtime_named_constant_name(resolution: &HirNameResolution) -> Option<String> {
    [
        resolution.resolved(),
        resolution.fallback(),
        Some(resolution.source()),
        resolution.source().strip_prefix('\\'),
    ]
    .into_iter()
    .flatten()
    .find(|name| !name.is_empty())
    .map(ToOwned::to_owned)
}

fn class_constant_initializer_member_name(module: &HirModule, expr_id: ExprId) -> Option<String> {
    let expr = module.expressions().get(expr_id)?;
    match expr.kind() {
        HirExprKind::Literal { text } if !text.starts_with('$') => {
            Some(local_name(text).to_owned())
        }
        HirExprKind::Name { resolution } if !resolution.source().starts_with('$') => {
            Some(local_name(resolution.source()).to_owned())
        }
        _ => None,
    }
}

fn resolve_class_constant_initializer(
    module: &HirModule,
    class_name: &str,
    constant_name: &str,
    named_constants: &HashMap<String, IrConstant>,
    class_constants: &declarations::ClassConstantInitializerMap,
    class_parents: &declarations::ClassParentMap,
    visiting_class_constants: &mut Vec<(String, String)>,
) -> Option<IrConstant> {
    let mut class_name = Some(normalize_class_name(class_name));
    let mut seen_classes = Vec::new();
    while let Some(search_class) = class_name {
        if seen_classes.iter().any(|class| class == &search_class) {
            return None;
        }
        seen_classes.push(search_class.clone());
        if let Some(const_expr_id) = class_constants
            .get(&search_class)
            .and_then(|constants| constants.get(constant_name))
            .copied()
        {
            let key = (search_class.clone(), constant_name.to_owned());
            if visiting_class_constants.iter().any(|entry| entry == &key) {
                return None;
            }
            let const_expr = module.const_exprs().get(const_expr_id)?;
            if const_expr.context() != ConstExprContext::ClassConstInitializer
                || !const_expr.is_allowed()
            {
                return None;
            }
            visiting_class_constants.push(key);
            let result = constant_from_expr_with_class_constants(
                module,
                const_expr.expr_id(),
                named_constants,
                Some(&search_class),
                class_constants,
                class_parents,
                visiting_class_constants,
            )
            .or_else(|| {
                const_expr
                    .folded_value()
                    .and_then(ir_constant_from_const_value)
            });
            visiting_class_constants.pop();
            return result;
        }
        class_name = class_parents.get(&search_class).cloned().flatten();
    }
    None
}

fn named_constant_value(
    named_constants: &HashMap<String, IrConstant>,
    resolution: &HirNameResolution,
) -> Option<IrConstant> {
    let candidates = [
        resolution.resolved(),
        resolution.fallback(),
        Some(resolution.source()),
        resolution.source().strip_prefix('\\'),
    ];
    candidates
        .into_iter()
        .flatten()
        .find_map(|name| named_constants.get(name).cloned())
}

fn named_constant_reference_from_resolution(
    resolution: &HirNameResolution,
) -> Option<NamedConstantReference> {
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

fn is_quiet_by_ref_internal_builtin_arg(function: &str, index: usize, arg: &HirCallArg) -> bool {
    if arg.unpack {
        return false;
    }

    let function = normalize_function_name(function);
    if generated_internal_builtin_param_is_by_ref(&function, index, arg.name.as_deref()) {
        return true;
    }

    match function.as_str() {
        "apcu_fetch" => index == 1 || arg.name.as_deref() == Some("success"),
        "apcu_dec" | "apcu_inc" => index == 2 || arg.name.as_deref() == Some("success"),
        "exif_thumbnail" => {
            (1..=3).contains(&index)
                || matches!(arg.name.as_deref(), Some("width" | "height" | "image_type"))
        }
        "getimagesize" => index == 1 || arg.name.as_deref() == Some("image_info"),
        "is_callable" => index == 2 || arg.name.as_deref() == Some("callable_name"),
        "msg_send" => index == 5 || arg.name.as_deref() == Some("error_code"),
        "msg_receive" => {
            index == 2
                || index == 4
                || index == 7
                || matches!(
                    arg.name.as_deref(),
                    Some("received_message_type" | "message" | "error_code")
                )
        }
        "openssl_random_pseudo_bytes" => index == 1 || arg.name.as_deref() == Some("strong_result"),
        "pcntl_wait" => index == 0 || arg.name.as_deref() == Some("status"),
        "pcntl_waitpid" => {
            index == 1
                || arg.name.as_deref() == Some("status")
                || index == 3
                || arg.name.as_deref() == Some("resource_usage")
        }
        "preg_filter" | "preg_replace" | "preg_replace_callback" => {
            index == 4 || arg.name.as_deref() == Some("count")
        }
        "preg_replace_callback_array" => index == 3 || arg.name.as_deref() == Some("count"),
        "preg_match" | "preg_match_all" => index == 2 || arg.name.as_deref() == Some("matches"),
        "socket_getpeername" | "socket_getsockname" => {
            index == 1 || index == 2 || matches!(arg.name.as_deref(), Some("address" | "port"))
        }
        "socket_recv" => index == 1 || arg.name.as_deref() == Some("data"),
        _ => false,
    }
}

fn generated_internal_builtin_param_is_by_ref(
    function: &str,
    index: usize,
    arg_name: Option<&str>,
) -> bool {
    let Some(metadata) = php_std::arginfo::function_metadata_indexed(function) else {
        return false;
    };
    let param = if let Some(name) = arg_name {
        metadata.params.iter().find(|param| param.name == name)
    } else {
        metadata.params.get(index)
    };
    param.is_some_and(|param| param.by_ref)
}

/// Predefined stdlib constant initializers, built once per process.
///
/// The registry exposes ~4.8k constants; rebuilding this owned-key map on
/// every compile was a measurable fixed cost per CLI invocation.
static PREDEFINED_CONSTANT_INITIALIZERS: std::sync::LazyLock<HashMap<String, IrConstant>> =
    std::sync::LazyLock::new(|| {
        let registry = php_std::ExtensionRegistry::standard_library();
        registry
            .enabled_constants()
            .into_iter()
            .filter(|constant| constant.deprecation().is_none())
            .filter_map(|constant| {
                Some((
                    constant.name().to_owned(),
                    ir_constant_from_std_value(constant.value()?)?,
                ))
            })
            .collect()
    });

fn predefined_constant_initializers() -> &'static HashMap<String, IrConstant> {
    &PREDEFINED_CONSTANT_INITIALIZERS
}

fn ir_constant_from_std_value(value: php_std::ConstantValue) -> Option<IrConstant> {
    match value {
        php_std::ConstantValue::Null => Some(IrConstant::Null),
        php_std::ConstantValue::Bool(value) => Some(IrConstant::Bool(value)),
        php_std::ConstantValue::Int(value) => Some(IrConstant::Int(value)),
        php_std::ConstantValue::Float(value) => Some(IrConstant::Float(value.to_f64())),
        php_std::ConstantValue::String(value) => Some(IrConstant::String(value.to_owned())),
        php_std::ConstantValue::Array(values) => values
            .iter()
            .copied()
            .map(ir_constant_from_std_value)
            .map(|value| value.map(|value| IrConstantArrayEntry { key: None, value }))
            .collect::<Option<Vec<_>>>()
            .map(IrConstant::Array),
    }
}

fn destructuring_key(
    module: &HirModule,
    index: usize,
    source_index: Option<usize>,
    key: Option<ExprId>,
) -> Option<IrConstant> {
    let Some(key) = key else {
        return Some(IrConstant::Int(
            source_index.unwrap_or(index).try_into().ok()?,
        ));
    };
    let expression = module.expressions().get(key)?;
    match expression.kind() {
        HirExprKind::Literal { text } => literal_constant(text),
        _ => None,
    }
}

fn destructuring_patterns(
    module: &HirModule,
    expr: ExprId,
) -> Option<Vec<(IrConstant, expressions::DestructuringPattern)>> {
    let expression = module.expressions().get(expr)?;
    let elements = match expression.kind().clone() {
        HirExprKind::Array { elements } | HirExprKind::List { elements } => elements,
        _ => return None,
    };
    let mut patterns = Vec::new();
    for (index, element) in elements.into_iter().enumerate() {
        let element_expression = module.expressions().get(element)?;
        if matches!(element_expression.kind(), HirExprKind::Missing) {
            continue;
        }
        let (key, value) = match element_expression.kind().clone() {
            HirExprKind::ArrayPair {
                key,
                value: Some(value),
                unpack: false,
                by_ref: false,
            } => (destructuring_key(module, index, None, key)?, value),
            HirExprKind::ArrayPair { .. } => return None,
            _ => (IrConstant::Int(index.try_into().ok()?), element),
        };
        let pattern = if let Some(children) = destructuring_patterns(module, value) {
            expressions::DestructuringPattern::Nested(children)
        } else {
            expressions::DestructuringPattern::Expr(value)
        };
        patterns.push((key, pattern));
    }
    Some(patterns)
}

fn negate_ir_constant(value: IrConstant) -> Option<IrConstant> {
    match value {
        IrConstant::Int(value) => value.checked_neg().map(IrConstant::Int),
        IrConstant::Float(value) => Some(IrConstant::Float(-value)),
        _ => None,
    }
}

fn binary_ir_constant(operator: &str, left: IrConstant, right: IrConstant) -> Option<IrConstant> {
    if operator == "??" {
        return if matches!(left, IrConstant::Null) {
            Some(right)
        } else {
            Some(left)
        };
    }
    match (operator, left, right) {
        ("+", IrConstant::Int(left), IrConstant::Int(right)) => {
            left.checked_add(right).map(IrConstant::Int)
        }
        ("-", IrConstant::Int(left), IrConstant::Int(right)) => {
            left.checked_sub(right).map(IrConstant::Int)
        }
        ("*", IrConstant::Int(left), IrConstant::Int(right)) => {
            left.checked_mul(right).map(IrConstant::Int)
        }
        ("&", IrConstant::Int(left), IrConstant::Int(right)) => Some(IrConstant::Int(left & right)),
        ("|", IrConstant::Int(left), IrConstant::Int(right)) => Some(IrConstant::Int(left | right)),
        ("^", IrConstant::Int(left), IrConstant::Int(right)) => Some(IrConstant::Int(left ^ right)),
        ("<<", IrConstant::Int(left), IrConstant::Int(right)) => u32::try_from(right)
            .ok()
            .and_then(|shift| left.checked_shl(shift))
            .map(IrConstant::Int),
        (">>", IrConstant::Int(left), IrConstant::Int(right)) => u32::try_from(right)
            .ok()
            .and_then(|shift| left.checked_shr(shift))
            .map(IrConstant::Int),
        (".", IrConstant::String(left), IrConstant::String(right)) => {
            Some(IrConstant::String(format!("{left}{right}")))
        }
        (".", IrConstant::StringBytes(mut left), IrConstant::StringBytes(right)) => {
            left.extend(right);
            Some(IrConstant::StringBytes(left))
        }
        _ => None,
    }
}

fn ir_constant_truthy(value: &IrConstant) -> Option<bool> {
    match value {
        IrConstant::Null => Some(false),
        IrConstant::Bool(value) => Some(*value),
        IrConstant::Int(value) => Some(*value != 0),
        IrConstant::Float(value) => Some(*value != 0.0 && !value.is_nan()),
        IrConstant::String(value) => Some(!value.is_empty() && value != "0"),
        IrConstant::StringBytes(value) => Some(!value.is_empty() && value.as_slice() != b"0"),
        IrConstant::Array(entries) => Some(!entries.is_empty()),
        IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => None,
    }
}

fn ir_constant_cast(kind: CastKind, value: IrConstant) -> Option<IrConstant> {
    match kind {
        CastKind::Bool => ir_constant_truthy(&value).map(IrConstant::Bool),
        CastKind::Int => ir_constant_to_int(&value).map(IrConstant::Int),
        CastKind::Float => ir_constant_to_float(&value).map(IrConstant::Float),
        CastKind::String => ir_constant_to_string(&value).map(IrConstant::String),
        CastKind::Array | CastKind::Object | CastKind::Void => None,
    }
}

fn ir_constant_to_int(value: &IrConstant) -> Option<i64> {
    match value {
        IrConstant::Null => Some(0),
        IrConstant::Bool(value) => Some(i64::from(*value)),
        IrConstant::Int(value) => Some(*value),
        IrConstant::Float(value) => Some(*value as i64),
        IrConstant::String(value) => Some(ir_constant_bytes_to_int(value.as_bytes())),
        IrConstant::StringBytes(value) => Some(ir_constant_bytes_to_int(value)),
        IrConstant::Array(entries) => Some(i64::from(!entries.is_empty())),
        IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => None,
    }
}

fn ir_constant_to_float(value: &IrConstant) -> Option<f64> {
    match value {
        IrConstant::Null => Some(0.0),
        IrConstant::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
        IrConstant::Int(value) => Some(*value as f64),
        IrConstant::Float(value) => Some(*value),
        IrConstant::String(value) => value.trim_start().parse::<f64>().ok().or(Some(0.0)),
        IrConstant::StringBytes(value) => String::from_utf8_lossy(value)
            .trim_start()
            .parse::<f64>()
            .ok()
            .or(Some(0.0)),
        IrConstant::Array(entries) => Some(if entries.is_empty() { 0.0 } else { 1.0 }),
        IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => None,
    }
}

fn ir_constant_to_string(value: &IrConstant) -> Option<String> {
    match value {
        IrConstant::Null => Some(String::new()),
        IrConstant::Bool(value) => Some(if *value { "1" } else { "" }.to_owned()),
        IrConstant::Int(value) => Some(value.to_string()),
        IrConstant::Float(value) => Some(value.to_string()),
        IrConstant::String(value) => Some(value.clone()),
        IrConstant::StringBytes(value) => Some(String::from_utf8_lossy(value).into_owned()),
        IrConstant::Array(_) | IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => {
            None
        }
    }
}

fn ir_constant_bytes_to_int(bytes: &[u8]) -> i64 {
    let mut index = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let negative = match bytes.get(index) {
        Some(b'-') => {
            index += 1;
            true
        }
        Some(b'+') => {
            index += 1;
            false
        }
        _ => false,
    };
    let mut value = 0_i64;
    let mut saw_digit = false;
    while let Some(byte) = bytes.get(index).filter(|byte| byte.is_ascii_digit()) {
        saw_digit = true;
        let digit = i64::from(byte - b'0');
        let Some(next) = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(digit))
        else {
            return if negative { i64::MIN } else { i64::MAX };
        };
        value = next;
        index += 1;
    }
    if !saw_digit {
        return 0;
    }
    if negative {
        value.checked_neg().unwrap_or(i64::MIN)
    } else {
        value
    }
}

fn ir_constant_dim_fetch(receiver: IrConstant, dim: &IrConstant) -> Option<IrConstant> {
    match receiver {
        IrConstant::Array(entries) => {
            let mut next_index = 0_i64;
            for entry in entries {
                let key = entry.key.unwrap_or_else(|| {
                    let key = IrConstant::Int(next_index);
                    next_index += 1;
                    key
                });
                if let IrConstant::Int(index) = key {
                    next_index = next_index.max(index.saturating_add(1));
                    if ir_constant_array_key_matches_int(index, dim) {
                        return Some(entry.value);
                    }
                } else if ir_constant_array_key_matches(&key, dim) {
                    return Some(entry.value);
                }
            }
            None
        }
        IrConstant::String(value) => {
            let IrConstant::Int(index) = dim else {
                return None;
            };
            let bytes = value.as_bytes();
            let length = bytes.len() as i64;
            let resolved = if *index < 0 { *index + length } else { *index };
            if resolved < 0 || resolved >= length {
                None
            } else {
                Some(IrConstant::String(
                    char::from(bytes[resolved as usize]).to_string(),
                ))
            }
        }
        IrConstant::StringBytes(value) => {
            let IrConstant::Int(index) = dim else {
                return None;
            };
            let length = value.len() as i64;
            let resolved = if *index < 0 { *index + length } else { *index };
            if resolved < 0 || resolved >= length {
                None
            } else {
                Some(IrConstant::StringBytes(vec![value[resolved as usize]]))
            }
        }
        _ => None,
    }
}

fn ir_constant_array_key_matches(key: &IrConstant, dim: &IrConstant) -> bool {
    match (key, dim) {
        (IrConstant::Int(left), _) => ir_constant_array_key_matches_int(*left, dim),
        (IrConstant::String(left), IrConstant::String(right)) => left == right,
        (IrConstant::StringBytes(left), IrConstant::StringBytes(right)) => left == right,
        (IrConstant::Bool(left), _) => ir_constant_array_key_matches_int(i64::from(*left), dim),
        (IrConstant::Null, IrConstant::String(right)) => right.is_empty(),
        _ => key == dim,
    }
}

fn ir_constant_array_key_matches_int(index: i64, dim: &IrConstant) -> bool {
    match dim {
        IrConstant::Int(value) => index == *value,
        IrConstant::Bool(value) => index == i64::from(*value),
        _ => false,
    }
}

fn bitnot_ir_constant(value: IrConstant) -> Option<IrConstant> {
    match value {
        IrConstant::Int(value) => Some(IrConstant::Int(!value)),
        _ => None,
    }
}

fn ir_constant_from_const_value(value: &ConstValue) -> Option<IrConstant> {
    match value {
        ConstValue::Null => Some(IrConstant::Null),
        ConstValue::Bool(value) => Some(IrConstant::Bool(*value)),
        ConstValue::Int(value) => Some(IrConstant::Int(*value)),
        ConstValue::String(value) => Some(IrConstant::String(value.clone())),
        ConstValue::UnresolvedRef(_) | ConstValue::ClosureConst | ConstValue::CallableConst => None,
    }
}

fn literal_constant(text: &str) -> Option<IrConstant> {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("null") {
        return Some(IrConstant::Null);
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return Some(IrConstant::Bool(true));
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Some(IrConstant::Bool(false));
    }
    if let Some(bytes) = quoted_literal_body(trimmed) {
        return Some(ir_string_constant(bytes));
    }
    if let Some(bytes) = heredoc_literal_body(trimmed) {
        return Some(ir_string_constant(bytes));
    }

    let numeric = trimmed.replace('_', "");
    if is_php_float_literal_candidate(&numeric) {
        return numeric.parse::<f64>().ok().map(IrConstant::Float);
    }
    parse_php_int_literal(&numeric)
        .map(IrConstant::Int)
        .or_else(|| {
            decimal_integer_literal(&numeric)?
                .parse::<f64>()
                .ok()
                .map(IrConstant::Float)
        })
}

fn decimal_integer_literal(text: &str) -> Option<&str> {
    let body = text
        .strip_prefix('-')
        .or_else(|| text.strip_prefix('+'))
        .unwrap_or(text);
    (!body.is_empty() && body.chars().all(|ch| ch.is_ascii_digit())).then_some(text)
}

fn is_php_float_literal_candidate(text: &str) -> bool {
    let body = text
        .strip_prefix('-')
        .or_else(|| text.strip_prefix('+'))
        .unwrap_or(text);
    let lower = body.to_ascii_lowercase();
    if lower.starts_with("0x") || lower.starts_with("0b") {
        return false;
    }
    body.contains('.') || body.contains('e') || body.contains('E')
}

fn parse_php_int_literal(text: &str) -> Option<i64> {
    let (negative, body) = text
        .strip_prefix('-')
        .map(|body| (true, body))
        .or_else(|| text.strip_prefix('+').map(|body| (false, body)))
        .unwrap_or((false, text));
    if body.is_empty() {
        return None;
    }
    let lower = body.to_ascii_lowercase();
    let parsed = if let Some(digits) = lower.strip_prefix("0x") {
        i64::from_str_radix(digits, 16).ok()?
    } else if let Some(digits) = lower.strip_prefix("0b") {
        i64::from_str_radix(digits, 2).ok()?
    } else if body.len() > 1
        && body.starts_with('0')
        && body.chars().all(|ch| matches!(ch, '0'..='7'))
    {
        i64::from_str_radix(body, 8).ok()?
    } else {
        body.parse::<i64>().ok()?
    };
    Some(if negative { -parsed } else { parsed })
}

fn ir_string_constant(bytes: Vec<u8>) -> IrConstant {
    match String::from_utf8(bytes) {
        Ok(value) => IrConstant::String(value),
        Err(error) => IrConstant::StringBytes(error.into_bytes()),
    }
}

#[cfg(test)]
mod tests;
