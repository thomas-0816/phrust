//! Semantic frontend frontend to runtime IR lowering skeleton.

use crate::builder::IrBuilder;
use crate::constants::{IrConstant, IrConstantArrayEntry};
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::function::{FunctionFlags, IrCapture, IrParam, IrReturnType};
use crate::ids::{BlockId, FileId, FunctionId, LocalId, RegId};
use crate::instruction::{
    CallableKind, ClosureCaptureArg, CompareOp, InstructionKind, IrCallArg, IrCallArgValueKind,
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
    HirNameResolution, HirProperty, HirPropertyHookBody, HirStmtKind, HirSwitchCase,
    HirTraitAdaptationKind, HirTypeKind, MagicMethodKind, ModifierSet, NameKind, Parameter,
    ParameterAttribute, ReturnType, SignatureKind, StmtId, TopLevelItemKind, TypeId, Visibility,
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
mod expressions;
mod statements;

pub use context::{LoweringContext, LoweringOptions, LoweringResult};
pub use diagnostics::{LoweringDiagnostic, LoweringDiagnosticContext, UnsupportedFeature};

const AUTO_GLOBAL_NAMES: &[&str] = &[
    "argc", "argv", "_SERVER", "_ENV", "_GET", "_POST", "_COOKIE", "_FILES", "_REQUEST", "_SESSION",
];

/// Lowers a Semantic frontend frontend result into a minimal runtime IR unit.
#[must_use]
pub fn lower_frontend_result(
    frontend: &FrontendResult,
    options: LoweringOptions,
) -> LoweringResult {
    let mut builder = IrBuilder::new(options.unit_id);
    let strict_types = frontend
        .database()
        .module(frontend.module().module_id())
        .and_then(|module| module.file_directives().strict_types())
        .is_some_and(|directive| matches!(directive.value(), DeclareValue::Int(1)));
    builder.set_strict_types(strict_types);
    let file = builder.add_file(options.source_path.clone());
    let module_span = frontend
        .database()
        .source_map()
        .span(frontend.module().module_id())
        .unwrap_or_else(|| TextRange::new(0, frontend.module().source_bytes()));
    let function = builder.start_function(
        "main",
        FunctionFlags {
            is_top_level: true,
            ..FunctionFlags::default()
        },
        span_from_range(file, module_span),
    );
    let prelude_block = builder.append_block(function);
    let block = builder.append_block(function);
    let null_const = builder.intern_constant(IrConstant::Null);
    let module_ir_span = span_from_range(file, module_span);
    let module_origin = format!("hir:module:{}", frontend.module().module_id().raw());
    builder.add_source_map(
        IrSourceMapTarget::Function { function },
        module_origin.clone(),
        module_ir_span,
    );
    builder.add_source_map(
        IrSourceMapTarget::Block {
            function,
            block: prelude_block,
        },
        module_origin.clone(),
        module_ir_span,
    );
    builder.add_source_map(
        IrSourceMapTarget::Block { function, block },
        module_origin.clone(),
        module_ir_span,
    );

    let mut context = LoweringContext::new(frontend, options, file);
    context.function_names.insert(function, String::new());
    let block = context.lower_global_constant_declarations(&mut builder, function, block);
    context.lower_function_declarations(&mut builder, function);
    context.lower_class_declarations(&mut builder, function);
    let current_block = context.lower_top_level(&mut builder, function, block);
    if context.options.emit_unsupported_instructions
        && !builder.is_terminated(function, current_block)
    {
        for diagnostic in &context.diagnostics {
            let instruction = builder.emit(
                function,
                current_block,
                InstructionKind::Unsupported {
                    diagnostic_id: diagnostic.id.clone(),
                },
                diagnostic.span,
            );
            builder.add_source_map(
                IrSourceMapTarget::Instruction {
                    function,
                    block: current_block,
                    instruction,
                },
                diagnostic.id.clone(),
                diagnostic.span,
            );
        }
    }
    if !builder.is_terminated(function, current_block) {
        builder.terminate_return(
            function,
            current_block,
            Some(Operand::Constant(null_const)),
            span_from_range(file, module_span),
        );
        builder.add_source_map(
            IrSourceMapTarget::Terminator {
                function,
                block: current_block,
            },
            module_origin.clone(),
            module_ir_span,
        );
    }
    context.emit_early_diagnostics(&mut builder, function, prelude_block);
    builder.terminate_jump(function, prelude_block, block, module_ir_span);
    builder.add_source_map(
        IrSourceMapTarget::Terminator {
            function,
            block: prelude_block,
        },
        module_origin,
        module_ir_span,
    );
    builder.set_entry(function);
    let unit = builder.finish();
    let verification = verify_unit(&unit);

    LoweringResult {
        unit,
        diagnostics: context.diagnostics,
        verification,
    }
}

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
    class_likes: &[(ClassLikeId, HirClassLike)],
    source_path: &str,
) -> String {
    imported_class_resolution_display_name(module, resolution)
        .or_else(|| declared_class_resolution_display_name(class_likes, source_path, resolution))
        .unwrap_or_else(|| display_class_name(resolution.source()))
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

fn function_declaration_metadata(
    module: &HirModule,
    signature: &FunctionSignature,
) -> Option<(String, DeclarationKind)> {
    module
        .declaration_table()
        .entries()
        .iter()
        .find(|entry| {
            matches!(
                entry.kind(),
                DeclarationKind::Function | DeclarationKind::ConditionalFunction
            ) && range_contains(signature.span(), entry.span())
        })
        .map(|entry| (entry.fqn().canonical(NameKind::Function), entry.kind()))
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

fn named_constant_from_default_source(
    source: &str,
    named_constants: &HashMap<String, IrConstant>,
) -> Option<IrConstant> {
    let value = source
        .split_once('=')
        .map_or(source, |(_, value)| value)
        .trim();
    named_constants.get(value).cloned()
}

fn source_constant_from_default_source(
    source: &str,
    named_constants: &HashMap<String, IrConstant>,
) -> Option<IrConstant> {
    let value = source
        .split_once('=')
        .map_or(source, |(_, value)| value)
        .trim();
    legacy_array_constant_from_source(value, named_constants)
        .or_else(|| named_constants.get(value).cloned())
        .or_else(|| literal_constant(value))
}

fn define_constant_initializers_from_source(
    source: &str,
    named_constants: &HashMap<String, IrConstant>,
) -> HashMap<String, IrConstant> {
    let mut map = HashMap::new();
    for line in source.lines().map(str::trim_start) {
        let line = line.strip_prefix("<?php").map_or(line, str::trim_start);
        let Some(rest) = line.strip_prefix("define") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('(') else {
            continue;
        };
        let Some(end) = matching_top_level_close_paren(rest) else {
            continue;
        };
        let args = &rest[..end];
        let Some(args) = split_top_level_commas(args) else {
            continue;
        };
        let [name, value, ..] = args.as_slice() else {
            continue;
        };
        let Some(name) = source_constant_from_default_source(name, named_constants) else {
            continue;
        };
        let IrConstant::String(name) = name else {
            continue;
        };
        let Some(value) = source_constant_from_default_source(value, named_constants) else {
            continue;
        };
        map.insert(name, value);
    }
    map
}

fn matching_top_level_close_paren(source: &str) -> Option<usize> {
    let mut depth = 0_u32;
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in source.char_indices() {
        if let Some(quoted) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quoted {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth = depth.checked_add(1)?,
            ')' if depth == 0 => return Some(index),
            ')' => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    None
}

fn legacy_array_constant_from_source(
    source: &str,
    named_constants: &HashMap<String, IrConstant>,
) -> Option<IrConstant> {
    let source = source.trim();
    let (head, tail) = source.split_at(source.len().min(5));
    if !head.eq_ignore_ascii_case("array") {
        return None;
    }
    let tail = tail.trim_start();
    let inner = tail.strip_prefix('(')?.strip_suffix(')')?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Some(IrConstant::Array(Vec::new()));
    }
    let mut entries = Vec::new();
    for entry in split_top_level_commas(inner)? {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (key, value) = split_top_level_arrow(entry).map_or((None, entry), |(key, value)| {
            (Some(key.trim()), value.trim())
        });
        let key = match key {
            Some(key) => Some(source_constant_from_default_source(key, named_constants)?),
            None => None,
        };
        entries.push(IrConstantArrayEntry {
            key,
            value: source_constant_from_default_source(value, named_constants)?,
        });
    }
    Some(IrConstant::Array(entries))
}

fn split_top_level_commas(source: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0_u32;
    let mut quote = None;
    let mut escaped = false;
    for (index, ch) in source.char_indices() {
        if let Some(quoted) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quoted {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' | '[' => depth = depth.checked_add(1)?,
            ')' | ']' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                parts.push(&source[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    parts.push(&source[start..]);
    Some(parts)
}

fn split_top_level_arrow(source: &str) -> Option<(&str, &str)> {
    let mut depth = 0_u32;
    let mut quote = None;
    let mut escaped = false;
    let mut iter = source.char_indices().peekable();
    while let Some((index, ch)) = iter.next() {
        if let Some(quoted) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quoted {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' | '[' => depth = depth.checked_add(1)?,
            ')' | ']' => depth = depth.checked_sub(1)?,
            '=' if depth == 0 && iter.peek().is_some_and(|(_, next)| *next == '>') => {
                return Some((&source[..index], &source[index + 2..]));
            }
            _ => {}
        }
    }
    None
}

fn class_constant_from_default_source(
    module: &HirModule,
    source: &str,
    current_class: Option<&str>,
    named_constants: &HashMap<String, IrConstant>,
    class_constants: &declarations::ClassConstantInitializerMap,
    class_parents: &declarations::ClassParentMap,
) -> Option<IrConstant> {
    let value = source
        .split_once('=')
        .map_or(source, |(_, value)| value)
        .trim();
    let (target, member) = value.split_once("::")?;
    let member = member.trim();
    if member.is_empty() {
        return None;
    }
    let target_class =
        if target.eq_ignore_ascii_case("self") || target.eq_ignore_ascii_case("static") {
            current_class.map(normalize_class_name)?
        } else if target.eq_ignore_ascii_case("parent") {
            current_class
                .map(normalize_class_name)
                .and_then(|class| class_parents.get(&class).cloned().flatten())?
        } else {
            normalize_class_name(target.trim_start_matches('\\'))
        };
    resolve_class_constant_initializer(
        module,
        &target_class,
        member,
        named_constants,
        class_constants,
        class_parents,
        &mut Vec::new(),
    )
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
                named_constants,
                current_class,
                class_constants,
                class_parents,
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
                _ => None,
            }
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

fn constant_from_expr_with_runtime_constants(
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
            .or_else(|| named_constant_value(named_constants, resolution))
            .or_else(|| runtime_named_constant_name(resolution).map(IrConstant::NamedConstant)),
        HirExprKind::Unary { operator, expr } => {
            let value = constant_from_expr_with_runtime_constants(
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
                _ => None,
            }
        }
        HirExprKind::Binary {
            operator,
            left,
            right,
        } => {
            let left = constant_from_expr_with_runtime_constants(
                module,
                (*left)?,
                named_constants,
                current_class,
                class_constants,
                class_parents,
                visiting_class_constants,
            )?;
            let right = constant_from_expr_with_runtime_constants(
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
            let condition_value = constant_from_expr_with_runtime_constants(
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
                constant_from_expr_with_runtime_constants(
                    module,
                    selected,
                    named_constants,
                    current_class,
                    class_constants,
                    class_parents,
                    visiting_class_constants,
                )
            } else {
                constant_from_expr_with_runtime_constants(
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
                                constant_from_expr_with_runtime_constants(
                                    module,
                                    key,
                                    named_constants,
                                    current_class,
                                    class_constants,
                                    class_parents,
                                    visiting_class_constants,
                                )
                            }),
                            value: constant_from_expr_with_runtime_constants(
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
                        value: constant_from_expr_with_runtime_constants(
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
            .or(Some(IrConstant::ClassConstant {
                class_name: target_class,
                constant_name: member,
            }))
        }
        HirExprKind::DimFetch { receiver, dim } => {
            let receiver = constant_from_expr_with_runtime_constants(
                module,
                (*receiver)?,
                named_constants,
                current_class,
                class_constants,
                class_parents,
                visiting_class_constants,
            )?;
            let dim = constant_from_expr_with_runtime_constants(
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
    normalize_function_name(function) == "is_callable"
        && !arg.unpack
        && (index == 2 || arg.name.as_deref() == Some("callable_name"))
}

fn predefined_constant_initializer_map() -> HashMap<String, IrConstant> {
    let registry = php_std::ExtensionRegistry::standard_library();
    registry
        .enabled_constants()
        .into_iter()
        .filter_map(|constant| {
            Some((
                constant.name().to_owned(),
                ir_constant_from_std_value(constant.value()?)?,
            ))
        })
        .collect()
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

fn destructuring_key(module: &HirModule, index: usize, key: Option<ExprId>) -> Option<IrConstant> {
    let Some(key) = key else {
        return Some(IrConstant::Int(index.try_into().ok()?));
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
        let (key, value) = match element_expression.kind().clone() {
            HirExprKind::ArrayPair {
                key,
                value: Some(value),
                unpack: false,
                by_ref: false,
            } => (destructuring_key(module, index, key)?, value),
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
mod tests {
    use super::*;
    use php_semantics::analyze_source;

    #[test]
    fn lower_empty_file_to_top_level_return_null() {
        let frontend = analyze_source("");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.unit.constants, vec![IrConstant::Null]);
        assert!(result.unit.to_snapshot_text().contains("return const:0"));
    }

    #[test]
    fn lower_open_tag_minimal_program() {
        let frontend = analyze_source("<?php");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn unsupported_feature_diagnostic_has_shared_envelope() {
        let diagnostic = LoweringDiagnostic {
            id: UnsupportedFeature::Eval.diagnostic_id().to_string(),
            feature: UnsupportedFeature::Eval,
            span: IrSpan::new(FileId::new(0), 10, 14),
            message: "eval is not supported by IR lowering".to_string(),
        };
        let context = LoweringDiagnosticContext {
            source_id: Some("source:0".to_string()),
            origin: Some("hir:expr:2".to_string()),
            function: Some(FunctionId::new(1)),
            block: Some(BlockId::new(2)),
            instruction: None,
            class_name: Some("C".to_string()),
            method_name: Some("m".to_string()),
        };

        let envelope = diagnostic.to_diagnostic_envelope(Some("demo.php"), &context);
        let json: serde_json::Value =
            serde_json::from_str(&envelope.compact_json().expect("json")).expect("parse json");

        assert_eq!(json["code"], "E_PHP_IR_UNSUPPORTED_EVAL");
        assert_eq!(json["layer"], "ir");
        assert_eq!(json["phase"], "lower");
        assert_eq!(json["severity"], "unsupported_feature");
        assert_eq!(json["location"]["path"], "demo.php");
        assert_eq!(json["location"]["span"]["start"], 10);
        assert_eq!(json["context"]["feature"], "eval");
        assert_eq!(json["context"]["function_id"], "1");
        assert_eq!(json["context"]["block_id"], "2");
        assert_eq!(json["context"]["origin"], "hir:expr:2");
    }

    #[test]
    fn global_array_const_initializers_lower_to_ir_constants() {
        let frontend = analyze_source(r#"<?php const EXPECTED = ["x" => "y", 2 => "z"];"#);
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(result.unit.constant_table.len(), 1);
        let value = &result.unit.constants[result.unit.constant_table[0].value.index()];
        assert_eq!(
            value,
            &IrConstant::Array(vec![
                IrConstantArrayEntry {
                    key: Some(IrConstant::String("x".to_string())),
                    value: IrConstant::String("y".to_string()),
                },
                IrConstantArrayEntry {
                    key: Some(IrConstant::Int(2)),
                    value: IrConstant::String("z".to_string()),
                },
            ])
        );
    }

    #[test]
    fn global_const_initializers_can_alias_class_constants() {
        let frontend = analyze_source(
            "<?php namespace Sodium; class Compat { const KEYBYTES = 32; } const CRYPTO_KEYBYTES = Compat::KEYBYTES;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(result.unit.constant_table.len(), 1);
        let value = &result.unit.constants[result.unit.constant_table[0].value.index()];
        assert_eq!(value, &IrConstant::Int(32));
    }

    #[test]
    fn global_const_initializers_can_register_external_class_constants_at_runtime() {
        let frontend =
            analyze_source("<?php namespace Sodium; const CRYPTO_KEYBYTES = Compat::KEYBYTES;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(result.unit.constant_table.is_empty());
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("fetch_class_constant r0 sodium\\compat::KEYBYTES"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("register_constant \"Sodium\\\\CRYPTO_KEYBYTES\" r0"),
            "{snapshot}"
        );
    }

    #[test]
    fn static_class_constant_targets_use_class_import_resolution() {
        let frontend = analyze_source(
            "<?php namespace Sodium; use ParagonIE_Sodium_Compat; const CRYPTO_KEYBYTES = ParagonIE_Sodium_Compat::KEYBYTES;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("fetch_class_constant r0 paragonie_sodium_compat::KEYBYTES"),
            "{snapshot}"
        );
        assert!(
            !snapshot.contains("Sodium\\ParagonIE_Sodium_Compat::KEYBYTES"),
            "{snapshot}"
        );
        assert!(
            !snapshot.contains("Sodium\\paragonie_sodium_compat::KEYBYTES"),
            "{snapshot}"
        );
    }

    #[test]
    fn class_name_constant_preserves_source_spelling() {
        let frontend = analyze_source("<?php class ClassNameBase {} echo ClassNameBase::class;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("string \"ClassNameBase\""), "{snapshot}");
        assert!(!snapshot.contains("string \"classnamebase\""), "{snapshot}");
    }

    #[test]
    fn namespaced_class_name_constant_uses_declared_fqn_display() {
        let frontend = analyze_source("<?php namespace P21\\Ns; class Child {} echo Child::class;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("string \"P21\\\\Ns\\\\Child\""),
            "{snapshot}"
        );
    }

    #[test]
    fn namespaced_external_class_name_constant_uses_resolved_fqn_display() {
        let frontend = analyze_source(
            "<?php namespace WordPress\\AiClientDependencies\\Http\\Discovery; echo Strategy\\GeneratedDiscoveryStrategy::class;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains(
                "string \"WordPress\\\\AiClientDependencies\\\\Http\\\\Discovery\\\\Strategy\\\\GeneratedDiscoveryStrategy\""
            ),
            "{snapshot}"
        );
        assert!(!snapshot.contains("fetch_class_constant"), "{snapshot}");
    }

    #[test]
    fn imported_qualified_class_name_constant_expands_alias_prefix() {
        let frontend = analyze_source(
            "<?php namespace Foo; use Vendor\\Package as PackageAlias; echo PackageAlias\\Generated::class;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("string \"Vendor\\\\Package\\\\Generated\""),
            "{snapshot}"
        );
        assert!(!snapshot.contains("fetch_class_constant"), "{snapshot}");
    }

    #[test]
    fn static_property_class_name_constant_initializer_lowers_to_string() {
        let frontend = analyze_source(
            "<?php namespace WordPress\\AiClientDependencies\\Http\\Discovery; abstract class ClassDiscovery { private static $strategies = [Strategy\\GeneratedDiscoveryStrategy::class]; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains(
                "array [append=>string \"WordPress\\\\AiClientDependencies\\\\Http\\\\Discovery\\\\Strategy\\\\GeneratedDiscoveryStrategy\"]"
            ),
            "{snapshot}"
        );
        assert!(!snapshot.contains("class_const"), "{snapshot}");
    }

    #[test]
    fn class_constant_forward_references_lower_to_ir_constants() {
        let frontend = analyze_source(
            "<?php class C { const CONST_2 = self::CONST_1; const CONST_1 = self::BASE_CONST; const BASE_CONST = 'hello'; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let class = result
            .unit
            .classes
            .iter()
            .find(|class| class.name == "c")
            .expect("class C");
        let values = class
            .constants
            .iter()
            .map(|constant| {
                let value = constant.value.expect("constant should have folded value");
                (
                    constant.name.as_str(),
                    result.unit.constants[value.index()].clone(),
                )
            })
            .collect::<HashMap<_, _>>();

        assert_eq!(
            values.get("CONST_1"),
            Some(&IrConstant::String("hello".into()))
        );
        assert_eq!(
            values.get("CONST_2"),
            Some(&IrConstant::String("hello".into()))
        );
        assert_eq!(
            values.get("BASE_CONST"),
            Some(&IrConstant::String("hello".into()))
        );
    }

    #[test]
    fn method_parameter_defaults_can_use_class_constants() {
        let frontend = analyze_source(
            "<?php class C { const LIMIT = 32; public static function f($limit = self::LIMIT) { return $limit; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let method = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "C::f")
            .expect("method function");
        assert_eq!(method.params[0].default, Some(IrConstant::Int(32)));
    }

    #[test]
    fn custom_typed_catches_and_by_ref_method_parameters_lower_to_ir() {
        let frontend = analyze_source(
            "<?php class MyEx extends Exception {} class C { public function fill(&$value) { try { throw new MyEx('x'); } catch (MyEx $e) { $value = $e->getMessage(); } } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let method = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "C::fill")
            .expect("method function");
        assert!(method.params[0].by_ref);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("catch_types=[myex]"), "{snapshot}");
    }

    #[test]
    fn by_ref_method_returns_lower_to_reference_ir() {
        let frontend = analyze_source(
            "<?php class C { public function &counter() { static $x = 0; return $x; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let method = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "C::counter")
            .expect("method function");
        assert!(method.returns_by_ref);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("function \"C::counter\""), "{snapshot}");
        assert!(snapshot.contains("return_ref local:"), "{snapshot}");
        assert!(
            !snapshot.contains("E_PHP_IR_UNSUPPORTED_BY_REF_RETURN"),
            "{snapshot}"
        );
    }

    #[test]
    fn class_constant_doc_comments_lower_to_ir_metadata() {
        let source = "<?php class C { /** label */ const LABEL = 'items'; const PLAIN = 1; }";
        let frontend = analyze_source(source);
        let result = lower_frontend_result(
            &frontend,
            LoweringOptions {
                source_text: Some(source.to_owned()),
                ..LoweringOptions::default()
            },
        );

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let class = result
            .unit
            .classes
            .iter()
            .find(|class| class.name == "c")
            .expect("class C");
        let doc_comments = class
            .constants
            .iter()
            .map(|constant| (constant.name.as_str(), constant.doc_comment.as_deref()))
            .collect::<HashMap<_, _>>();

        assert_eq!(doc_comments.get("LABEL"), Some(&Some("/** label */")));
        assert_eq!(doc_comments.get("PLAIN"), Some(&None));
    }

    #[test]
    fn method_array_parameter_defaults_lower_to_ir_constants() {
        let frontend = analyze_source(
            "<?php class Test { static function f3(array $ar = array()) {} static function f4(array $ar = array(25)) {} }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let f3 = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "Test::f3")
            .expect("Test::f3 function");
        let f4 = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "Test::f4")
            .expect("Test::f4 function");

        assert_eq!(f3.params[0].default, Some(IrConstant::Array(Vec::new())));
        assert_eq!(
            f4.params[0].default,
            Some(IrConstant::Array(vec![IrConstantArrayEntry {
                key: None,
                value: IrConstant::Int(25),
            }]))
        );
    }

    #[test]
    fn parameter_default_expression_matrix_lowers_to_ir_constants() {
        let frontend = analyze_source(
            "<?php const LABEL = 'B'; class Source { const FIRST = 'A'; } function f($items = ['left' => Source::FIRST, 'right' => LABEL], $selected = ['x', 'y'][1], $fallback = null ?? 'fallback', $conditional = true ? 'yes' : 'no') {}",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let function = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "f")
            .expect("function f");

        assert_eq!(
            function.params[0].default,
            Some(IrConstant::Array(vec![
                IrConstantArrayEntry {
                    key: Some(IrConstant::String("left".to_owned())),
                    value: IrConstant::String("A".to_owned()),
                },
                IrConstantArrayEntry {
                    key: Some(IrConstant::String("right".to_owned())),
                    value: IrConstant::String("B".to_owned()),
                },
            ]))
        );
        assert_eq!(
            function.params[1].default,
            Some(IrConstant::String("y".to_owned()))
        );
        assert_eq!(
            function.params[2].default,
            Some(IrConstant::String("fallback".to_owned()))
        );
        assert_eq!(
            function.params[3].default,
            Some(IrConstant::String("yes".to_owned()))
        );
    }

    #[test]
    fn parameter_default_array_preserves_external_class_constant() {
        let frontend = analyze_source(
            "<?php class Test { public function __construct($data = array('version' => External::LATEST_SCHEMA)) {} }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let function = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "Test::__construct")
            .expect("Test::__construct function");

        assert_eq!(
            function.params[0].default,
            Some(IrConstant::Array(vec![IrConstantArrayEntry {
                key: Some(IrConstant::String("version".to_owned())),
                value: IrConstant::ClassConstant {
                    class_name: "external".to_owned(),
                    constant_name: "LATEST_SCHEMA".to_owned(),
                },
            }]))
        );
    }

    #[test]
    fn conditional_method_array_parameter_defaults_lower_to_ir_constants() {
        let source = "<?php if (!class_exists('Test', false)) : class Test { static function f3($ar = array()) {} static function f4($ar = array(25)) {} } endif;";
        let frontend = analyze_source(source);
        let result = lower_frontend_result(
            &frontend,
            LoweringOptions {
                source_text: Some(source.to_owned()),
                ..LoweringOptions::default()
            },
        );

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let f3 = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "Test::f3")
            .expect("Test::f3 function");
        let f4 = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "Test::f4")
            .expect("Test::f4 function");

        assert_eq!(f3.params[0].default, Some(IrConstant::Array(Vec::new())));
        assert_eq!(
            f4.params[0].default,
            Some(IrConstant::Array(vec![IrConstantArrayEntry {
                key: None,
                value: IrConstant::Int(25),
            }]))
        );
    }

    #[test]
    fn source_define_parameter_defaults_lower_to_ir_constants() {
        let source = "<?php define('OBJECT', 'OBJECT'); class Test { public function get($output = OBJECT) {} }";
        let frontend = analyze_source(source);
        let result = lower_frontend_result(
            &frontend,
            LoweringOptions {
                source_text: Some(source.to_owned()),
                ..LoweringOptions::default()
            },
        );

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let method = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "Test::get")
            .expect("Test::get function");

        assert_eq!(
            method.params[0].default,
            Some(IrConstant::String("OBJECT".to_owned()))
        );
    }

    #[test]
    fn core_integer_constant_parameter_defaults_lower_to_ir_constants() {
        let frontend = analyze_source(
            "<?php function bounds(?int $max = PHP_INT_MAX, ?int $min = PHP_INT_MIN, int $size = PHP_INT_SIZE, int $level = E_USER_NOTICE) {}",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let function = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "bounds")
            .expect("bounds function");

        assert_eq!(
            function.params[0].default,
            Some(IrConstant::Int(isize::MAX as i64))
        );
        assert_eq!(
            function.params[1].default,
            Some(IrConstant::Int(isize::MIN as i64))
        );
        assert_eq!(
            function.params[2].default,
            Some(IrConstant::Int(std::mem::size_of::<isize>() as i64))
        );
        assert_eq!(function.params[3].default, Some(IrConstant::Int(1024)));
    }

    #[test]
    fn static_property_isset_empty_lower_to_static_property_instructions() {
        let frontend = analyze_source("<?php class C {} var_dump(isset(C::$p), empty(C::$p));");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("isset_static_property r"), "{snapshot}");
        assert!(snapshot.contains("empty_static_property r"), "{snapshot}");
        assert!(snapshot.contains("C::$p"), "{snapshot}");
    }

    #[test]
    fn static_property_dimension_isset_and_unset_lower_to_static_property_dim_instructions() {
        let frontend = analyze_source(
            "<?php class C { private static $map = ['id' => 'ID']; function f($key) { var_dump(isset(self::$map[$key])); unset(self::$map[$key]); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("isset_static_property_dim r"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("unset_static_property_dim self::$map"),
            "{snapshot}"
        );
        assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
    }

    #[test]
    fn class_constant_dimension_isset_empty_lower_through_hidden_local() {
        let frontend = analyze_source(
            "<?php class C { const MAP = ['id' => 'ID']; function f($key) { var_dump(isset(self::MAP[$key]), empty(self::MAP[$key])); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_class_constant r"), "{snapshot}");
        assert!(snapshot.contains("isset_dim r"), "{snapshot}");
        assert!(snapshot.contains("empty_dim r"), "{snapshot}");
        assert!(
            snapshot.contains("__phrust:isset-class-constant-dim"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("__phrust:empty-class-constant-dim"),
            "{snapshot}"
        );
    }

    #[test]
    fn construct_empty_superglobal_dim_lowers_to_empty_dim_instruction() {
        let frontend = analyze_source(
            "<?php const RECOVERY_MODE_COOKIE = 'wordpress_rec'; if (empty($_COOKIE[RECOVERY_MODE_COOKIE])) { echo 'missing'; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("empty_dim r"), "{snapshot}");
        assert!(snapshot.contains("RECOVERY_MODE_COOKIE"), "{snapshot}");
    }

    #[test]
    fn construct_empty_method_call_lowers_to_unary_not() {
        let frontend = analyze_source(
            "<?php class C { function get($name) { return $name; } } $c = new C(); var_dump(empty($c->get('RequiresWP')));",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("call_method r"), "{snapshot}");
        assert!(snapshot.contains("unary r"), "{snapshot}");
        assert!(snapshot.contains("not"), "{snapshot}");
    }

    #[test]
    fn construct_empty_static_method_call_lowers_to_static_call_and_not() {
        let frontend = analyze_source("<?php var_dump(empty(Imagick::queryFormats('WEBP')));");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("call_static_method r"), "{snapshot}");
        assert!(
            snapshot.contains("\"Imagick\"::\"queryformats\""),
            "{snapshot}"
        );
        assert!(snapshot.contains("unary r"), "{snapshot}");
        assert!(snapshot.contains("not"), "{snapshot}");
    }

    #[test]
    fn static_method_call_uses_import_display_name_for_autoload() {
        let frontend = analyze_source(
            "<?php namespace WpOrg\\Requests; use WpOrg\\Requests\\Utility\\InputValidator; final class Requests { public static function set_certificate_path($path) { return InputValidator::is_string_or_stringable($path); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains(
                "\"WpOrg\\\\Requests\\\\Utility\\\\InputValidator\"::\"is_string_or_stringable\""
            ),
            "{snapshot}"
        );
        assert!(!snapshot.contains("\"InputValidator\"::"), "{snapshot}");
        assert!(
            !snapshot.contains("\"wporg\\\\requests\\\\utility\\\\inputvalidator\"::"),
            "{snapshot}"
        );
    }

    #[test]
    fn construct_isset_braced_dynamic_property_lowers_to_dynamic_property_instruction() {
        let frontend = analyze_source(
            "<?php function matches($obj, $m_key) { return isset($obj->{$m_key}); }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("isset_dynamic_property r"), "{snapshot}");
        assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
    }

    #[test]
    fn construct_empty_unbraced_dynamic_property_lowers_to_dynamic_property_instruction() {
        let frontend = analyze_source(
            "<?php function active($kind) { return ! empty( get_queried_object()->$kind ); }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("empty_dynamic_property r")
                || snapshot.contains("fetch_dynamic_property r"),
            "{snapshot}"
        );
        assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
    }

    #[test]
    fn dynamic_property_variable_member_isset_and_unset_lower_without_literal_diagnostics() {
        let frontend = analyze_source(
            "<?php class C { public $data; function has($key) { var_dump(isset($this->data->$key)); unset($this->data->$key); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("isset_dynamic_property r"), "{snapshot}");
        assert!(snapshot.contains("unset_dynamic_property"), "{snapshot}");
        assert!(
            !snapshot.contains("E_PHP_IR_UNSUPPORTED_LITERAL"),
            "{snapshot}"
        );
    }

    #[test]
    fn construct_isset_concat_dim_key_lowers_to_isset_dim_instruction() {
        let frontend = analyze_source(
            "<?php function cookie_exists($user_id) { return isset($_COOKIE['wp-settings-' . $user_id]); }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("binary r"), "{snapshot}");
        assert!(snapshot.contains("isset_dim r"), "{snapshot}");
        assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
    }

    #[test]
    fn construct_isset_concat_constant_dim_key_lowers_to_isset_dim_instruction() {
        let frontend = analyze_source(
            "<?php function postpass_cookie_exists() { return isset($_COOKIE['wp-postpass_' . COOKIEHASH]); }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_const"), "{snapshot}");
        assert!(snapshot.contains("binary r"), "{snapshot}");
        assert!(snapshot.contains("isset_dim r"), "{snapshot}");
        assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
    }

    #[test]
    fn construct_isset_interpolated_dim_lowers_to_isset_dim_instruction() {
        let frontend = analyze_source(
            r#"<?php function plugin($plugins, $extension) { if (isset($plugins["{$extension['slug']}/{$extension['slug']}.php"])) { return $plugins["{$extension['slug']}/{$extension['slug']}.php"]; } }"#,
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("isset_dim r"), "{snapshot}");
        assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    }

    #[test]
    fn construct_isset_nested_dim_key_lowers_to_isset_dim_instruction() {
        let frontend = analyze_source(
            "<?php function error_name($core_errors, $error) { if (isset($core_errors[$error['type']])) { echo $core_errors[$error['type']]; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("isset_dim r"), "{snapshot}");
        assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    }

    #[test]
    fn static_property_append_lowers_through_hidden_local_and_assign() {
        let frontend = analyze_source("<?php class C { static public $p = array(); } C::$p[] = 1;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
        assert!(snapshot.contains("append_dim r"), "{snapshot}");
        assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
        assert!(
            snapshot.contains("__phrust:static-property-dim"),
            "{snapshot}"
        );
        assert!(snapshot.contains("c::$p"), "{snapshot}");
    }

    #[test]
    fn array_unshift_static_property_arg_lowers_through_hidden_local_and_assign() {
        let frontend = analyze_source(
            "<?php class C { private static $items = array(); function f($value) { array_unshift(self::$items, $value); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
        assert!(
            snapshot.contains("__phrust:array_unshift-static-property"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("call_function r") && snapshot.contains("array_unshift"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("by_ref=local") || snapshot.contains("by_ref=local:"),
            "{snapshot}"
        );
        assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
        assert!(snapshot.contains("self::$items"), "{snapshot}");
    }

    #[test]
    fn namespaced_array_unshift_static_property_arg_lowers_through_hidden_local_and_assign() {
        let frontend = analyze_source(
            "<?php namespace WordPress\\AiClientDependencies\\Http\\Discovery; abstract class ClassDiscovery { private static $strategies = array(); function f($strategy) { array_unshift(self::$strategies, $strategy); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
        assert!(
            snapshot.contains("__phrust:wordpress\\aiclientdependencies\\http\\discovery\\array_unshift-static-property"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("call_function r")
                && snapshot
                    .contains("wordpress\\aiclientdependencies\\http\\discovery\\array_unshift"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("by_ref=local") || snapshot.contains("by_ref=local:"),
            "{snapshot}"
        );
        assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
        assert!(snapshot.contains("self::$strategies"), "{snapshot}");
    }

    #[test]
    fn imported_nullable_parameter_type_lowers_to_resolved_class_name() {
        let frontend = analyze_source(
            "<?php namespace App; use Vendor\\Contracts\\CacheInterface; function set_cache(?CacheInterface $cache): void {}",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains(
                "param \"cache\" local:0 required=true variadic=false by_ref=false type=?class \"vendor\\\\contracts\\\\cacheinterface\""
            ),
            "{snapshot}"
        );
    }

    #[test]
    fn nested_static_property_dimension_assignment_lowers_through_hidden_local() {
        let frontend = analyze_source(
            "<?php class C { static public $p = array(); function f($outer, $inner) { self::$p[$outer][$inner] = 1; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
        assert!(snapshot.contains("assign_dim r"), "{snapshot}");
        assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
        assert!(
            snapshot.contains("__phrust:static-property-dim"),
            "{snapshot}"
        );
        assert!(snapshot.contains("self::$p"), "{snapshot}");
    }

    #[test]
    fn static_property_dimension_increment_lowers_through_hidden_local() {
        let frontend = analyze_source(
            "<?php class C { private static $seen = array(); function f($name) { ++static::$seen[$name]; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
        assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
        assert!(snapshot.contains("assign_dim r"), "{snapshot}");
        assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
        assert!(snapshot.contains("static::$seen"), "{snapshot}");
    }

    #[test]
    fn namespaced_self_static_property_keeps_relative_class_name() {
        let frontend = analyze_source(
            "<?php namespace WpOrg\\Requests; final class Requests { protected static $certificate_path = ''; public static function set_certificate_path($path) { self::$certificate_path = $path; return self::$certificate_path; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("assign_static_property r")
                && snapshot.contains("self::$certificate_path"),
            "{snapshot}"
        );
        assert!(
            !snapshot.contains("WpOrg\\\\Requests\\\\self::$certificate_path"),
            "{snapshot}"
        );
    }

    #[test]
    fn anonymous_class_new_lowers_to_synthetic_class_instantiation() {
        let frontend = analyze_source(
            "<?php class Base { public function __construct($value) {} } function f($value) { return new class($value) extends Base { public function m() {} }; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("__phrust_anonymous_"), "{snapshot}");
        assert!(snapshot.contains("_anonymous_0"), "{snapshot}");
        assert!(snapshot.contains("new_object r"), "{snapshot}");
        assert!(snapshot.contains("anonymous#0"), "{snapshot}");
    }

    #[test]
    fn static_property_compound_assign_and_increment_fetch_before_write() {
        let frontend = analyze_source("<?php class C {} C::$p += 1; C::$p++;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert_eq!(
            snapshot.matches("fetch_static_property r").count(),
            2,
            "{snapshot}"
        );
        assert_eq!(
            snapshot.matches("assign_static_property r").count(),
            2,
            "{snapshot}"
        );
        assert!(snapshot.contains("binary r"), "{snapshot}");
        assert!(snapshot.contains("c::$p"), "{snapshot}");
    }

    #[test]
    fn property_increment_lowers_through_fetch_and_assign_property() {
        let frontend = analyze_source("<?php class C {} $c = new C; $c->p++; ++$c->p;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert_eq!(
            snapshot.matches("fetch_property r").count(),
            2,
            "{snapshot}"
        );
        assert_eq!(
            snapshot.matches("assign_property r").count(),
            2,
            "{snapshot}"
        );
        assert!(snapshot.contains("binary r"), "{snapshot}");
    }

    #[test]
    fn property_compound_assign_lowers_through_fetch_binary_and_assign_property() {
        let frontend =
            analyze_source("<?php class C { public $s = ''; } $c = new C; $c->s .= 'x';");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_property r"), "{snapshot}");
        assert!(snapshot.contains("binary r"), "{snapshot}");
        assert!(snapshot.contains("assign_property r"), "{snapshot}");
        assert!(snapshot.contains("$s"), "{snapshot}");
    }

    #[test]
    fn property_dimensions_assignment_append_and_unset_lower_to_dedicated_ir() {
        let frontend = analyze_source(
            "<?php class C { private $callbacks = array(); public function run($priority, $idx) { $this->callbacks[$priority][$idx] = array('function' => 'f'); $this->callbacks[] = 'tail'; unset($this->callbacks[$priority][$idx]); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("assign_property_dim r"), "{snapshot}");
        assert!(snapshot.contains("append_property_dim r"), "{snapshot}");
        assert!(snapshot.contains("unset_property_dim r"), "{snapshot}");
        assert!(snapshot.contains("$callbacks"), "{snapshot}");
    }

    #[test]
    fn property_dimension_compound_assignment_lowers_through_fetch_binary_and_writeback() {
        let frontend = analyze_source(
            "<?php class C { private $cache = []; public function run($group, $key, $offset) { $this->cache[$group][$key] += $offset; $this->cache[$group][$key] -= $offset; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_property r"), "{snapshot}");
        assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
        assert!(snapshot.contains("binary r"), "{snapshot}");
        assert_eq!(
            snapshot.matches("assign_property_dim r").count(),
            2,
            "{snapshot}"
        );
        assert!(snapshot.contains("$cache"), "{snapshot}");
    }

    #[test]
    fn property_reference_assignments_lower_to_reference_ir() {
        let frontend = analyze_source(
            "<?php class C { public $extra; public function bind(&$value, $key, $source) { $this->extra = & $value; $GLOBALS[$key] = & $source->extra; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("bind_reference_property r"), "{snapshot}");
        assert!(
            snapshot.contains("bind_reference_dim_from_property"),
            "{snapshot}"
        );
        assert!(
            !snapshot.contains("E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE"),
            "{snapshot}"
        );
    }

    #[test]
    fn by_ref_foreach_over_property_lowers_through_hidden_local_writeback() {
        let frontend = analyze_source(
            "<?php class C { private $iterations = array(1); public function run() { foreach ($this->iterations as &$iteration) { $iteration = $iteration + 1; } unset($iteration); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_property r"), "{snapshot}");
        assert!(
            snapshot.contains("__phrust:foreach-ref-property"),
            "{snapshot}"
        );
        assert!(snapshot.contains("foreach_init_ref iter"), "{snapshot}");
        assert!(snapshot.contains("assign_property r"), "{snapshot}");
        assert!(
            !snapshot.contains("E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH"),
            "{snapshot}"
        );
    }

    #[test]
    fn by_ref_foreach_over_local_dim_lowers_through_hidden_local_writeback() {
        let frontend = analyze_source(
            "<?php function rename_blocks($settings) { foreach ($settings['blocks'] as &$block_settings) { $block_settings['x'] = 1; } unset($block_settings); return $settings; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
        assert!(snapshot.contains("__phrust:foreach-ref-dim"), "{snapshot}");
        assert!(snapshot.contains("foreach_init_ref iter"), "{snapshot}");
        assert!(snapshot.contains("assign_dim r"), "{snapshot}");
        assert!(
            !snapshot.contains("E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH"),
            "{snapshot}"
        );
    }

    #[test]
    fn constructor_promoted_properties_lower_to_property_and_assignment() {
        let frontend = analyze_source(
            "<?php class Name { function __construct(public string $name) {} function display() { echo $this->name; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let class = result
            .unit
            .classes
            .iter()
            .find(|class| class.name == "name")
            .expect("lowered Name class");
        let property = class
            .properties
            .iter()
            .find(|property| property.name == "name")
            .expect("promoted name property");
        assert!(property.flags.is_typed, "{property:#?}");
        assert!(!property.flags.is_private, "{property:#?}");
        assert!(!property.flags.is_protected, "{property:#?}");
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("assign_property r"), "{snapshot}");
        assert!(snapshot.contains("Name::__construct"), "{snapshot}");
    }

    #[test]
    fn lower_echo_literal_statement_emits_load_const_and_echo() {
        let frontend = analyze_source("<?php echo 1;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("load_const r0 const:1"));
        assert!(snapshot.contains("echo r0"));
        assert!(snapshot.contains("source_map:"));
        assert!(snapshot.contains("instr function:0 block:1 instr:0 <= hir:expr:0"));
    }

    #[test]
    fn lower_top_level_exit_statement_terminates_script() {
        let frontend = analyze_source("<?php echo 'before'; exit; echo 'after';");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("echo r0"), "{snapshot}");
        assert!(snapshot.contains("exit"), "{snapshot}");
        assert!(!snapshot.contains("after"), "{snapshot}");
    }

    #[test]
    fn lower_top_level_exit_message_emits_before_terminating_script() {
        let frontend = analyze_source("<?php die('skip platform'); echo 'after';");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("string \"skip platform\""), "{snapshot}");
        assert!(snapshot.contains("exit r"), "{snapshot}");
        assert!(!snapshot.contains("after"), "{snapshot}");
    }

    #[test]
    fn lower_zero_arg_die_statement_terminates_without_operand() {
        let frontend = analyze_source("<?php function stop_now() { die(); echo 'after'; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("exit"), "{snapshot}");
        assert!(!snapshot.contains("unsupported"), "{snapshot}");
        assert!(!snapshot.contains("missing"), "{snapshot}");
    }

    #[test]
    fn lower_casted_die_operand_terminates_script() {
        let frontend = analyze_source(
            "<?php function stop_now($message) { die( (string) $message ); echo 'after'; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("cast r"), "{snapshot}");
        assert!(snapshot.contains(" string "), "{snapshot}");
        assert!(snapshot.contains("exit r"), "{snapshot}");
        assert!(!snapshot.contains("unsupported"), "{snapshot}");
        assert!(!snapshot.contains("missing"), "{snapshot}");
    }

    #[test]
    fn lower_wordpress_style_die_concat_terminates_script() {
        let frontend = analyze_source(
            "<?php die( '<h1>' . __( 'Requirements Not Met' ) . '</h1><p>' . $compat . '</p></body></html>' ); echo 'after';",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("call_function r"), "{snapshot}");
        assert!(snapshot.contains("\"__\""), "{snapshot}");
        assert!(snapshot.contains("binary r"), "{snapshot}");
        assert!(snapshot.contains("exit r"), "{snapshot}");
        assert!(!snapshot.contains("after"), "{snapshot}");
        assert!(!snapshot.contains("unsupported"), "{snapshot}");
        assert!(!snapshot.contains("missing"), "{snapshot}");
    }

    #[test]
    fn include_construct_operand_keeps_full_concat_expression() {
        let frontend = analyze_source("<?php include __DIR__ . '/_data/child.php';");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert_eq!(
            snapshot.matches("include r3 include r2").count(),
            1,
            "{snapshot}"
        );
        assert!(snapshot.contains(" concat "), "{snapshot}");
        assert!(!snapshot.contains("E_PHP_IR_UNSUPPORTED"), "{snapshot}");
    }

    #[test]
    fn label_and_goto_lower_to_jumps_without_unsupported_hir() {
        let frontend = analyze_source(
            "<?php $i = 0; start: $i++; if ($i < 3) { goto start; } goto done; echo 'skip'; done: echo $i;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("hir:label:start"), "{snapshot}");
        assert!(snapshot.contains("hir:label:done"), "{snapshot}");
        assert!(snapshot.contains("hir:goto:start"), "{snapshot}");
        assert!(snapshot.contains("hir:goto:done"), "{snapshot}");
        assert!(snapshot.matches("jump block:").count() >= 3, "{snapshot}");
        assert!(
            !snapshot.contains("E_PHP_IR_UNSUPPORTED_HIR_STATEMENT"),
            "{snapshot}"
        );
    }

    #[test]
    fn predefined_constants_fold_in_compile_time_contexts() {
        let source = "<?php
            #[Attr(PHP_INT_MAX)]
            class C {
                public const MASK = E_ALL & ~E_DEPRECATED;
                public const ROOT = DIRECTORY_SEPARATOR . 'wp';
                public string $eol = PHP_EOL;
            }
            function boot($limit = PHP_INT_MAX, $path = DEFAULT_INCLUDE_PATH) {}
            ";
        let frontend = analyze_source(source);
        let result = lower_frontend_result(
            &frontend,
            LoweringOptions {
                source_text: Some(source.to_owned()),
                ..LoweringOptions::default()
            },
        );

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::Int(php_std::constants::PHP_INT_MAX)),
            "{:#?}",
            result.unit.constants
        );
        assert!(
            result.unit.constants.contains(&IrConstant::Int(
                php_std::constants::E_ALL & !php_std::constants::E_DEPRECATED
            )),
            "{:#?}",
            result.unit.constants
        );
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::String(php_std::constants::PHP_EOL.to_string())),
            "{:#?}",
            result.unit.constants
        );
        assert!(
            result.unit.constants.contains(&IrConstant::String(format!(
                "{}wp",
                php_std::constants::DIRECTORY_SEPARATOR
            ))),
            "{:#?}",
            result.unit.constants
        );

        let class = result
            .unit
            .classes
            .iter()
            .find(|class| class.name == "c")
            .expect("class C");
        assert_eq!(class.attributes[0].arguments.len(), 1);
        let function = result
            .unit
            .functions
            .iter()
            .find(|function| function.name == "boot")
            .expect("boot function");
        assert_eq!(
            function.params[0].default,
            Some(IrConstant::Int(php_std::constants::PHP_INT_MAX))
        );
        assert_eq!(
            function.params[1].default,
            Some(IrConstant::String(
                php_std::constants::DEFAULT_INCLUDE_PATH.to_string()
            ))
        );
    }

    #[test]
    fn error_suppressed_variable_load_lowers_quietly() {
        let frontend = analyze_source("<?php echo @$missing;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("load_local_quiet"), "{snapshot}");
        assert!(!snapshot.contains("unsupported"), "{snapshot}");
    }

    #[test]
    fn literals_are_interned_in_first_use_order() {
        let frontend = analyze_source("<?php echo 1, 1, \"x\", null, true, 1.5;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(
            result.unit.constants,
            vec![
                IrConstant::Null,
                IrConstant::Int(1),
                IrConstant::String("x".to_string()),
                IrConstant::Bool(true),
                IrConstant::Float(1.5)
            ]
        );
        assert!(
            result
                .unit
                .source_map
                .entries()
                .iter()
                .any(|entry| matches!(
                    entry.target,
                    crate::source_map::IrSourceMapTarget::Instruction { .. }
                ) && entry.origin.starts_with("hir:expr:"))
        );
    }

    #[test]
    fn numeric_literal_separators_and_prefixes_lower_to_constants() {
        let frontend = analyze_source(
            "<?php echo 299_792_458, '|', 0xCAFE_F00D, '|', 0b0101_1111, '|', 0137_041, '|', 0_124;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::Int(299_792_458))
        );
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::Int(0xCAFE_F00D))
        );
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::Int(0b0101_1111))
        );
        assert!(result.unit.constants.contains(&IrConstant::Int(0o137_041)));
        assert!(result.unit.constants.contains(&IrConstant::Int(0o124)));
    }

    #[test]
    fn oversized_decimal_integer_literals_lower_to_float_constants() {
        let frontend = analyze_source("<?php echo 18446744073709551616;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(
            result
                .unit
                .constants
                .iter()
                .any(|constant| matches!(constant, IrConstant::Float(value) if *value == 18446744073709551616_f64))
        );
    }

    #[test]
    fn literals_unescape_php_string_bytes_without_unicode_normalization() {
        let frontend = analyze_source("<?php echo \"a\\n\", 'b\\\\c';");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::String("a\n".to_string()))
        );
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::String("b\\c".to_string()))
        );
        assert_eq!(
            quoted_literal_body(r#""\0\x0n\141""#),
            Some(b"\0\0na".to_vec())
        );
        assert_eq!(
            quoted_literal_body(r#""\u{41}\xFF""#),
            Some(vec![b'A', 0xff])
        );
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::String("a\n".to_string()))
        );
    }

    #[test]
    fn literals_keep_binary_php_string_bytes() {
        let frontend = analyze_source("<?php echo \"\\xFF\\0\";");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::StringBytes(vec![0xff, 0]))
        );
    }

    #[test]
    fn literals_lower_heredoc_and_nowdoc_bodies() {
        let frontend = analyze_source(
            "<?php $a = <<<TXT\nhello\\n\nTXT; $b = <<<'TXT'\nhello\\n\nTXT; echo $a, $b;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::String("hello\n".to_string()))
        );
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::String("hello\\n".to_string()))
        );

        let frontend = analyze_source("<?php $a = <<<TXT\n\\\"quotes\nTXT; $b = \"\\\"quotes\";");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::String("\\\"quotes".to_string()))
        );
        assert!(
            result
                .unit
                .constants
                .contains(&IrConstant::String("\"quotes".to_string()))
        );
    }

    #[test]
    fn literals_lower_simple_interpolation_to_concat() {
        let frontend = analyze_source("<?php $counter = 3; echo \"-- Iteration $counter --\\n\";");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains(" concat "), "{snapshot}");
        assert!(snapshot.contains("cast r"), "{snapshot}");
        assert!(snapshot.contains(" string "), "{snapshot}");
        assert!(snapshot.contains("local:0 $counter"), "{snapshot}");
        assert!(
            interpolated_literal_parts("\"a {$counter} b\"").is_some(),
            "braced simple interpolation should be recognized"
        );
    }

    #[test]
    fn integer_braced_variable_names_lower_to_stable_local_slot() {
        let frontend = analyze_source("<?php ${10} = 42; echo ${10};");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("local:0 $10"), "{snapshot}");
        assert_eq!(snapshot.matches("local:0 $10").count(), 1, "{snapshot}");
    }

    #[test]
    fn deprecated_dollar_brace_interpolation_lowers_diagnostic() {
        let frontend =
            analyze_source("<?php $counter = 3; echo \"-- Iteration ${counter} --\\n\";");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("emit_diagnostic Deprecation"),
            "{snapshot}"
        );
        assert!(
            snapshot.contains("E_PHP_RUNTIME_DEPRECATED_DOLLAR_BRACE_INTERPOLATION"),
            "{snapshot}"
        );
        assert!(snapshot.contains(" concat "), "{snapshot}");
        assert!(snapshot.contains("local:0 $counter"), "{snapshot}");

        let parts = interpolated_literal_parts("\"a {$counter} ${counter} b\"")
            .expect("interpolated parts");
        assert!(matches!(
            &parts[1],
            InterpolatedPart::Variable {
                deprecated_dollar_brace: false,
                ..
            }
        ));
        assert!(matches!(
            &parts[3],
            InterpolatedPart::Variable {
                deprecated_dollar_brace: true,
                ..
            }
        ));
    }

    #[test]
    fn simple_array_dim_interpolation_lowers_fetch_dim() {
        let frontend = analyze_source(
            "<?php $needles = ['Hello world']; $i = 0; echo \"Position of '$needles[$i]'\\n\";",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
        assert!(snapshot.contains("local:0 $needles"), "{snapshot}");
        assert!(snapshot.contains("local:1 $i"), "{snapshot}");

        let parts = interpolated_literal_parts("\"Position of '$needles[$i]'\"").expect("parts");
        assert!(matches!(
            &parts[1],
            InterpolatedPart::Variable {
                name,
                dim: Some(InterpolatedDim::Variable(dim)),
                ..
            } if name == "needles" && dim == "i"
        ));
    }

    #[test]
    fn braced_method_call_interpolation_lowers_call_method() {
        let frontend = analyze_source(
            "<?php try { throw new Error('bad'); } catch (Error $ex) { echo \"{$ex->getCode()}: {$ex->getMessage()}\"; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("call_method r"), "{snapshot}");
        assert!(snapshot.contains("\"getcode\""), "{snapshot}");
        assert!(snapshot.contains("\"getmessage\""), "{snapshot}");

        let parts =
            interpolated_literal_parts("\"{$ex->getCode()}: {$ex->getMessage()}\"").expect("parts");
        assert!(matches!(
            &parts[1],
            InterpolatedPart::MethodCall { receiver, method }
                if receiver == "ex" && method == "getCode"
        ));
        assert!(matches!(
            &parts[3],
            InterpolatedPart::MethodCall { receiver, method }
                if receiver == "ex" && method == "getMessage"
        ));
    }

    #[test]
    fn simple_property_interpolation_lowers_fetch_property() {
        let frontend = analyze_source(
            "<?php class D { private $counter = 2; function f() { echo \"($this->counter)\"; echo \"({$this->counter})\"; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_property r"), "{snapshot}");
        assert!(snapshot.contains("local:0 $this"), "{snapshot}");
        assert!(snapshot.contains("$counter"), "{snapshot}");

        let parts = interpolated_literal_parts("\"($this->counter)\"").expect("parts");
        assert!(matches!(
            &parts[1],
            InterpolatedPart::Property { receiver, property }
                if receiver == "this" && property == "counter"
        ));
        let parts = interpolated_literal_parts("\"({$this->counter})\"").expect("parts");
        assert!(matches!(
            &parts[1],
            InterpolatedPart::Property { receiver, property }
                if receiver == "this" && property == "counter"
        ));
    }

    #[test]
    fn static_new_object_preserves_display_class_name_for_autoload() {
        let frontend = analyze_source("<?php new TestX;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("new_object r"), "{snapshot}");
        assert!(
            snapshot.contains("\"testx\" display=\"TestX\""),
            "{snapshot}"
        );
    }

    #[test]
    fn new_self_lowers_to_declaring_class_name() {
        let frontend = analyze_source(
            "<?php class C { private static $instance = null; public static function get_instance() { if ( null === self::$instance ) { self::$instance = new self(); } return self::$instance; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("new_object r"), "{snapshot}");
        assert!(snapshot.contains("\"c\" display=\"C\""), "{snapshot}");
        assert!(!snapshot.contains("\"self\""), "{snapshot}");
    }

    #[test]
    fn self_class_constant_lowers_to_declaring_class_name() {
        let frontend = analyze_source(
            "<?php namespace WpOrg\\Requests; final class Autoload { public static function register() { spl_autoload_register([self::class, 'load'], true); } public static function load($class) {} }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("string \"WpOrg\\\\Requests\\\\Autoload\""),
            "{snapshot}"
        );
        assert!(!snapshot.contains("string \"self\""), "{snapshot}");
    }

    #[test]
    fn locals_lower_variable_assignment_fetch_and_compound_ops() {
        let frontend = analyze_source("<?php $a = 1; $a += 2; echo $a;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let function = &result.unit.functions[0];
        assert_eq!(function.locals, vec!["a"]);
        assert_eq!(function.local_count, 1);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("local:0 $a"));
        assert!(snapshot.contains("store_local local:0"));
        assert!(snapshot.contains("load_local r"));
        assert!(snapshot.contains("binary r"));
    }

    #[test]
    fn null_coalescing_assignment_lowers_for_locals_and_dimensions() {
        let frontend = analyze_source(
            "<?php $value ??= 'fallback'; $url = []; $url['path'] ??= ''; echo $url['path'];",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("isset_local r"), "{snapshot}");
        assert!(snapshot.contains("isset_dim r"), "{snapshot}");
        assert!(snapshot.contains("assign_dim r"), "{snapshot}");
        assert!(snapshot.contains("store_local local:0"), "{snapshot}");
    }

    #[test]
    fn null_coalescing_expression_assignment_lowers_for_static_local_cache() {
        let frontend = analyze_source(
            "<?php function f() { static $skipStrategy; $skipStrategy ?? $skipStrategy = class_exists('A') ? false : 'A'; return $skipStrategy; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("load_local_quiet r"), "{snapshot}");
        assert!(snapshot.contains("compare r"), "{snapshot}");
        assert!(snapshot.contains("store_local local:"), "{snapshot}");
    }

    #[test]
    fn dim_fetch_lowers_binary_index_expression() {
        let frontend = analyze_source(
            "<?php $args_array = array(array(0), array(-1, 1)); $counter = 1; var_dump($args_array[$counter - 1]);",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("local:0 $args_array"), "{snapshot}");
        assert!(snapshot.contains("local:1 $counter"), "{snapshot}");
        assert!(snapshot.contains("binary r"), "{snapshot}");
        assert!(snapshot.contains("fetch_dim r"), "{snapshot}");
    }

    #[test]
    fn array_literal_preserves_nested_keyed_array_as_append_value() {
        let frontend = analyze_source("<?php $xs = array(array(12 => \"12twelve\"));");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("array_insert"), "{snapshot}");
        assert!(
            !snapshot.contains("array element is missing its value"),
            "{snapshot}"
        );
    }

    #[test]
    fn locals_lower_pre_and_post_increment_with_distinct_return_registers() {
        let frontend = analyze_source("<?php $a = 1; echo $a++; echo ++$a;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert_eq!(result.unit.functions[0].locals, vec!["a"]);
        assert!(snapshot.contains("local:0 $a"));
        assert!(snapshot.matches("store_local local:0").count() >= 3);
    }

    #[test]
    fn control_flow_lowers_if_else_to_readable_blocks() {
        let frontend = analyze_source("<?php if (true) { echo \"t\"; } else { echo \"f\"; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("jump_if r"));
        assert!(snapshot.contains("block:1"));
        assert!(snapshot.contains("block:2"));
        assert!(snapshot.contains("string \"t\""));
        assert!(snapshot.contains("string \"f\""));
    }

    #[test]
    fn ternary_after_if_uses_explicit_false_target() {
        let frontend = analyze_source(
            "<?php function cmp($a, $b) { if ($a == $b) { return 0; } return ($a < $b) ? -1 : 1; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("jump_if r"));
        assert!(snapshot.contains(" block:"));
    }

    #[test]
    fn control_flow_lowers_loops_and_break_continue_targets() {
        let frontend = analyze_source(
            "<?php $i = 0; while ($i < 4) { $i++; if ($i == 2) { continue; } if ($i == 3) { break; } echo $i; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("jump_if r"));
        assert!(snapshot.matches("jump block:").count() >= 3);
        assert!(snapshot.contains("compare r"));
    }

    #[test]
    fn control_flow_lowers_goto_to_label_blocks() {
        let frontend = analyze_source(
            "<?php function scan($i) { if ($i > 0) { goto found; } echo \"skip\"; found: return $i; } echo scan(1);",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("jump block:"), "{snapshot}");
        assert!(snapshot.contains("string \"skip\""), "{snapshot}");
        assert!(snapshot.contains("function \"scan\""), "{snapshot}");
    }

    #[test]
    fn for_loop_lowers_two_initializer_expressions() {
        let frontend = analyze_source("<?php for ($x = 0, $count = 0; $x < 3; $x++) { $count++; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("local:0 $x"), "{snapshot}");
        assert!(snapshot.contains("local:1 $count"), "{snapshot}");
        assert!(snapshot.matches("store_local").count() >= 2, "{snapshot}");
        assert!(
            !snapshot.contains("E_PHP_IR_UNSUPPORTED_FOR_HEADER_MULTI_EXPR"),
            "{snapshot}"
        );
    }

    #[test]
    fn for_loop_lowers_multi_expression_header_sections() {
        let frontend =
            analyze_source("<?php for ($i = 0, $j = 3; $i < 3; $i++, $j--) { echo $i; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("local:0 $i"), "{snapshot}");
        assert!(snapshot.contains("local:1 $j"), "{snapshot}");
        assert!(snapshot.matches("store_local").count() >= 2, "{snapshot}");
        assert!(
            !snapshot.contains("E_PHP_IR_UNSUPPORTED_FOR_HEADER_MULTI_EXPR"),
            "{snapshot}"
        );
    }

    #[test]
    fn foreach_lowers_keyless_list_destructuring_value_target() {
        let frontend =
            analyze_source("<?php foreach ([[1, 2]] as [$val, $precision]) { echo $val; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("$val"), "{snapshot}");
        assert!(snapshot.contains("$precision"), "{snapshot}");
        assert!(snapshot.contains("fetch_dim"), "{snapshot}");
        assert!(snapshot.matches("store_local").count() >= 2, "{snapshot}");
        assert!(
            !snapshot.contains("foreach value target must be a simple local variable"),
            "{snapshot}"
        );
    }

    #[test]
    fn list_assignment_lowers_property_targets() {
        let frontend = analyze_source(
            "<?php class D { public function __construct(...$args) { list($this->handle, $this->src) = $args; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_dim"), "{snapshot}");
        assert!(snapshot.contains("assign_property"), "{snapshot}");
        assert!(
            !snapshot.contains("only simple variable assignment"),
            "{snapshot}"
        );
    }

    #[test]
    fn list_assignment_lowers_array_dimension_targets() {
        let frontend = analyze_source(
            "<?php $data = []; list($data['width'], $data['height']) = image_constrain_size_for_editor($data['width'], $data['height'], $size);",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_dim"), "{snapshot}");
        assert!(snapshot.contains("assign_dim"), "{snapshot}");
        assert!(
            !snapshot.contains("only simple variable assignment"),
            "{snapshot}"
        );
    }

    #[test]
    fn array_destructuring_assignment_lowers_string_keys() {
        let frontend = analyze_source(
            "<?php ['namespace' => $ns, 'value' => $path] = $entry; echo $ns, $path;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("$ns"), "{snapshot}");
        assert!(snapshot.contains("$path"), "{snapshot}");
        assert!(snapshot.contains("string \"namespace\""), "{snapshot}");
        assert!(snapshot.contains("string \"value\""), "{snapshot}");
        assert!(snapshot.contains("fetch_dim"), "{snapshot}");
        assert!(snapshot.matches("store_local").count() >= 2, "{snapshot}");
        assert!(
            !snapshot.contains("only simple variable assignment"),
            "{snapshot}"
        );
    }

    #[test]
    fn list_assignment_lowers_string_keyed_array_destructuring() {
        let frontend = analyze_source(
            "<?php [ 'prefix' => $attr_prefix, 'suffix' => $suffix, 'unique_id' => $unique_id] = $parts;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("$attr_prefix"), "{snapshot}");
        assert!(snapshot.contains("$suffix"), "{snapshot}");
        assert!(snapshot.contains("$unique_id"), "{snapshot}");
        assert!(snapshot.contains("string \"prefix\""), "{snapshot}");
        assert!(snapshot.contains("string \"suffix\""), "{snapshot}");
        assert!(snapshot.contains("string \"unique_id\""), "{snapshot}");
        assert!(snapshot.matches("fetch_dim").count() >= 3, "{snapshot}");
        assert!(
            !snapshot.contains("only simple variable assignment"),
            "{snapshot}"
        );
    }

    #[test]
    fn switch_match_lowers_switch_fallthrough_and_match_error() {
        let frontend = analyze_source(
            "<?php $x = 1; switch ($x) { case 0: echo \"zero\"; case 1: echo \"one\"; break; default: echo \"default\"; } echo match ($x) { 0 => \"zero\" };",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("jump_if r"));
        assert!(snapshot.contains("equal"));
        assert!(snapshot.contains("identical"));
        assert!(snapshot.contains("runtime_error \"E_PHP_VM_UNHANDLED_MATCH\""));
        assert!(snapshot.matches("jump block:").count() >= 2);
        assert!(snapshot.contains("string \"zero\""));
        assert!(snapshot.contains("string \"one\""));
    }

    #[test]
    fn functions_lower_named_declaration_table_params_and_call() {
        let frontend =
            analyze_source("<?php function add($a, $b) { return $a + $b; } echo add(2, 3);");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(result.unit.functions.len(), 2);
        assert_eq!(result.unit.function_table.len(), 1);
        assert_eq!(result.unit.function_table[0].name, "add");
        assert_eq!(result.unit.functions[1].params.len(), 2);
        assert_eq!(result.unit.functions[1].locals, vec!["a", "b"]);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("function_name \"add\" => function:1"));
        assert!(snapshot.contains("call_function r"));
        assert!(snapshot.contains("\"add\""));
    }

    #[test]
    fn functions_lower_namespaced_declaration_table_and_call() {
        let frontend = analyze_source(
            "<?php namespace PerformanceIC; function hot() { return 2; } echo hot();",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(result.unit.function_table.len(), 1);
        assert_eq!(result.unit.function_table[0].name, "performanceic\\hot");
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("function_name \"performanceic\\\\hot\" => function:1"));
        assert!(snapshot.contains("\"performanceic\\\\hot\""));
    }

    #[test]
    fn conditional_duplicate_functions_keep_bodies_without_duplicate_lookup_entries() {
        let frontend = analyze_source(
            "<?php if (false) : function branch_dup() { return 'no'; } else : function branch_dup() { return 'yes'; } endif;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(result.unit.functions.len(), 3);
        assert_eq!(result.unit.function_table.len(), 0);
        assert_eq!(
            result
                .unit
                .functions
                .iter()
                .filter(|function| function.name == "branch_dup")
                .count(),
            2
        );
        let snapshot = result.unit.to_snapshot_text();
        assert_eq!(snapshot.matches("function_name \"branch_dup\"").count(), 0);
    }

    #[test]
    fn conditional_function_declaration_emits_runtime_declare() {
        let frontend = analyze_source(
            "<?php if (true) { function branch_runtime() { return 'yes'; } } echo branch_runtime();",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(result.unit.function_table.len(), 0);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("declare_function \"branch_runtime\""));
    }

    #[test]
    fn namespaced_conditional_function_declaration_emits_runtime_declare() {
        let frontend = analyze_source(
            "<?php namespace Sodium; if (!is_callable('\\\\Sodium\\\\bin2hex')) { function bin2hex($string) { return ParagonIE_Sodium_Compat::bin2hex($string); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(result.unit.function_table.len(), 0);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("declare_function \"sodium\\\\bin2hex\""),
            "{snapshot}"
        );
    }

    #[test]
    fn call_arg_property_dimension_emits_by_ref_metadata() {
        let frontend = analyze_source(
            "<?php class C { public $iterations = [[1, 2]]; function run($i) { next($this->iterations[$i]); } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("by_ref_property_dim="), "{snapshot}");
    }

    #[test]
    fn array_literal_by_ref_local_dimension_lowers_through_hidden_local() {
        let frontend = analyze_source(
            "<?php $credentials = ['user_login' => 'u']; $args = array(&$credentials['user_login']);",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("__phrust:array-ref-dim"), "{snapshot}");
        assert!(snapshot.contains("bind_reference_from_dim"), "{snapshot}");
        assert!(snapshot.contains("array_insert"), "{snapshot}");
        assert!(snapshot.contains("by_ref=local:"), "{snapshot}");
    }

    #[test]
    fn comparison_assignment_idiom_lowers_assignment_then_compare() {
        let frontend =
            analyze_source("<?php while ( false !== $file = readdir( $dh ) ) { echo $file; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("call_function r"), "{snapshot}");
        assert!(snapshot.contains("store_local local:"), "{snapshot}");
        assert!(snapshot.contains("compare r"), "{snapshot}");
        assert!(snapshot.contains("not_identical"), "{snapshot}");
    }

    #[test]
    fn unary_not_assignment_idiom_lowers_assignment_then_not() {
        let frontend = analyze_source(
            "<?php function maybe_post($id) { if ( !$post = get_post($id) ) { return false; } return $post; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("call_function r"), "{snapshot}");
        assert!(snapshot.contains("store_local local:"), "{snapshot}");
        assert!(snapshot.contains("unary r"), "{snapshot}");
        assert!(snapshot.contains("not"), "{snapshot}");
    }

    #[test]
    fn logical_or_not_assignment_idiom_lowers_with_short_circuit() {
        let frontend = analyze_source(
            "<?php if ( ('attachment' != $_post->post_type) || !$url = wp_get_attachment_url($_post->ID) ) { return false; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("not_equal"), "{snapshot}");
        assert!(snapshot.contains("jump_if r"), "{snapshot}");
        assert!(snapshot.contains("store_local local:"), "{snapshot}");
        assert!(snapshot.contains("unary r"), "{snapshot}");
    }

    #[test]
    fn logical_and_assignment_idiom_lowers_with_short_circuit() {
        let frontend = analyze_source(
            "<?php if ( !$fullsize && $src = wp_get_attachment_thumb_url($post->ID) ) { return $src; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("jump_if r"), "{snapshot}");
        assert!(snapshot.contains("store_local local:"), "{snapshot}");
        assert!(snapshot.contains("call_function r"), "{snapshot}");
    }

    #[test]
    fn logical_xor_lowers_to_bool_casts_and_not_identical_compare() {
        let frontend = analyze_source(
            "<?php function f($noopen, $noclose) { if ($noopen xor $noclose) { return 'one'; } return 'both-or-none'; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("cast r"), "{snapshot}");
        assert!(snapshot.contains("bool"), "{snapshot}");
        assert!(snapshot.contains("compare r"), "{snapshot}");
        assert!(snapshot.contains("not_identical"), "{snapshot}");
    }

    #[test]
    fn append_then_keyed_dimension_assignment_lowers_through_temp_array() {
        let frontend = analyze_source(
            "<?php $patternses = array(); $type = 'x'; $regex = 'r'; $patternses[][ $type ] = $regex;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("__phrust:append-nested-dim"),
            "{snapshot}"
        );
        assert!(snapshot.contains("assign_dim"), "{snapshot}");
        assert!(snapshot.contains("append_dim"), "{snapshot}");
    }

    #[test]
    fn dim_to_dim_reference_assignment_lowers_through_hidden_source() {
        let frontend =
            analyze_source("<?php $types[$name] =& $icon_files[$file]; $icon_files[$file] = 'x';");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("__phrust:dim-ref-source"), "{snapshot}");
        assert!(snapshot.contains("bind_reference_from_dim"), "{snapshot}");
        assert!(snapshot.contains("bind_reference_dim"), "{snapshot}");
    }

    #[test]
    fn dim_reference_assignment_allows_property_fetch_dimension_keys() {
        let frontend = analyze_source(
            "<?php foreach ((array) $terms as $key => $term) { $terms_by_id[$term->term_id] =& $terms[$key]; }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("__phrust:dim-ref-source"), "{snapshot}");
        assert!(snapshot.contains("fetch_property"), "{snapshot}");
        assert!(snapshot.contains("bind_reference_from_dim"), "{snapshot}");
        assert!(snapshot.contains("bind_reference_dim"), "{snapshot}");
    }

    #[test]
    fn property_dimension_reference_assignment_allows_method_call_keys() {
        let frontend = analyze_source(
            "<?php class MO { public array $entries = []; function add($entry) { $this->entries[$entry->key()] = &$entry; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("call_method"), "{snapshot}");
        assert!(
            snapshot.contains("bind_reference_property_dim"),
            "{snapshot}"
        );
        assert!(
            !snapshot.contains("object-property references are a known gap"),
            "{snapshot}"
        );
    }

    #[test]
    fn local_reference_assignment_lowers_method_return_reference() {
        let frontend = analyze_source(
            "<?php class MO { function add($original, $translation) { $entry = &$this->make_entry($original, $translation); return $entry; } public function &make_entry($original, $translation) { $entry = new stdClass(); return $entry; } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(
            snapshot.contains("bind_reference_method_call"),
            "{snapshot}"
        );
        assert!(
            !snapshot.contains("object-property references are a known gap"),
            "{snapshot}"
        );
    }

    #[test]
    fn nested_conditional_function_declarations_emit_once_per_branch() {
        let frontend = analyze_source(
            "<?php if (!function_exists('utf8_encode')) : if (extension_loaded('mbstring')) : function utf8_encode($value) { return 'mb'; } else : function utf8_encode($value) { return 'fallback'; } endif; endif;",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(result.unit.function_table.len(), 0);
        let snapshot = result.unit.to_snapshot_text();
        assert_eq!(
            snapshot.matches("declare_function \"utf8_encode\"").count(),
            2,
            "{snapshot}"
        );
    }

    #[test]
    fn nested_conditional_function_inside_function_emits_runtime_declare() {
        let frontend = analyze_source(
            "<?php function outer($flag) { if ($flag) { if (!function_exists('lowercase_octets')) { function lowercase_octets($matches) { return strtolower($matches[0]); } } } }",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert_eq!(result.unit.function_table.len(), 1);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("function_name \"outer\""), "{snapshot}");
        assert!(
            snapshot.contains("declare_function \"lowercase_octets\""),
            "{snapshot}"
        );
        assert_eq!(
            snapshot
                .matches("function_name \"lowercase_octets\"")
                .count(),
            0,
            "{snapshot}"
        );
    }

    #[test]
    fn closures_lower_with_stable_function_id_and_capture_dump() {
        let frontend = analyze_source(
            "<?php $x = 2; $f = function($y) use ($x) { return $x + $y; }; echo $f(3);",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("make_closure r"));
        assert!(snapshot.contains("function:1"));
        assert!(snapshot.contains("\"x\"=local:0 by_ref=false"));
        assert!(snapshot.contains("capture \"x\" local:0 by_ref=false"));
        assert!(snapshot.contains("call_callable r"));
    }

    #[test]
    fn pipe_lowers_first_class_callable_to_stable_callable_ir() {
        let frontend = analyze_source(
            "<?php function plus1($x) { return $x + 1; } echo 2 |> plus1(...); echo \" a \" |> trim(...);",
        );
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("resolve_callable"));
        assert!(snapshot.contains("function_name \"plus1\""));
        assert!(snapshot.contains("function_name \"trim\""));
        assert!(snapshot.contains("pipe r"));
    }

    #[test]
    fn lower_generator_known_gap_is_machine_readable() {
        let frontend = analyze_source("<?php function gen() { yield 1; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(result.unit.to_snapshot_text().contains("yield r"));
    }

    #[test]
    fn lower_generator_method_to_ir_instruction() {
        let frontend =
            analyze_source("<?php class C { public function gen() { yield $this->x; } }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("function \"C::gen\""), "{snapshot}");
        assert!(snapshot.contains("yield r"), "{snapshot}");
    }

    #[test]
    fn lower_yield_from_to_ir_instruction() {
        let frontend = analyze_source("<?php function gen($items) { yield from $items; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(result.unit.to_snapshot_text().contains("yield_from r"));
    }

    #[test]
    fn lower_eval_to_ir_instruction() {
        let frontend = analyze_source("<?php $code = 'echo '; eval($code . '1;');");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains(" concat "), "{snapshot}");
        assert!(snapshot.contains("eval r"), "{snapshot}");
    }

    #[test]
    fn unsupported_feature_ids_are_machine_readable() {
        let expected = [
            (
                UnsupportedFeature::Generator,
                "E_PHP_IR_UNSUPPORTED_GENERATOR",
            ),
            (
                UnsupportedFeature::YieldFrom,
                "E_PHP_IR_UNSUPPORTED_YIELD_FROM",
            ),
            (UnsupportedFeature::Fiber, "E_PHP_IR_UNSUPPORTED_FIBER"),
            (UnsupportedFeature::Eval, "E_PHP_IR_UNSUPPORTED_EVAL"),
            (
                UnsupportedFeature::Autoload,
                "E_PHP_IR_UNSUPPORTED_AUTOLOAD",
            ),
            (
                UnsupportedFeature::Reflection,
                "E_PHP_IR_UNSUPPORTED_REFLECTION",
            ),
            (
                UnsupportedFeature::TraitRuntime,
                "E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME",
            ),
            (
                UnsupportedFeature::EnumRuntime,
                "E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME",
            ),
            (
                UnsupportedFeature::PropertyHooks,
                "E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS",
            ),
            (
                UnsupportedFeature::FullReferences,
                "E_PHP_IR_UNSUPPORTED_REFERENCE_SEMANTICS",
            ),
        ];

        for (feature, diagnostic_id) in expected {
            assert_eq!(feature.diagnostic_id(), diagnostic_id);
        }
    }

    #[test]
    fn formerly_unsupported_constructs_lower_without_unsupported_diagnostics() {
        let cases = [
            "<?php function gen() { yield from []; }",
            "<?php spl_autoload_register(function ($class) {});",
            "<?php trait T { public function f() {} } class C { use T; }",
            "<?php class C { public string $name { get { return 'x'; } } }",
        ];

        for source in cases {
            let frontend = analyze_source(source);
            let result = lower_frontend_result(&frontend, LoweringOptions::default());

            assert!(result.verification.is_ok(), "{:#?}", result.verification);
            assert!(
                result
                    .diagnostics
                    .iter()
                    .all(|diagnostic| !diagnostic.id.starts_with("E_PHP_IR_UNSUPPORTED_")),
                "{source}: {:#?}",
                result.diagnostics
            );
        }
    }

    #[test]
    fn enums_lower_runtime_metadata_and_case_table() {
        let frontend = analyze_source("<?php enum Priority: string { case High = 'H'; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let class = result
            .unit
            .classes
            .iter()
            .find(|class| class.name == "priority")
            .expect("enum class entry");
        assert_eq!(class.display_name, "Priority");
        assert!(class.flags.is_enum);
        assert!(class.flags.is_final);
        assert_eq!(class.enum_backing_type, Some(ClassEnumBackingType::String));
        assert_eq!(class.enum_cases.len(), 1);
        assert_eq!(class.enum_cases[0].name, "High");
        assert!(class.enum_cases[0].value.is_some());
        assert!(class.interfaces.iter().any(|name| name == "unitenum"));
        assert!(class.interfaces.iter().any(|name| name == "backedenum"));
    }
}
