use std::collections::BTreeMap;

use crate::ids::{BlockId, FunctionId, InstrId};
use crate::instruction::IrDiagnosticSeverity;
use crate::source_map::IrSpan;
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticLocation, DiagnosticPhase, DiagnosticSeverity,
    DiagnosticSpan, DiagnosticSuggestion,
};
use serde::{Deserialize, Serialize};

/// Stable code for an unresolved trait used during declaration composition.
pub const MISSING_TRAIT_DIAGNOSTIC_CODE: &str = "E_PHP_IR_TRAIT_NOT_FOUND";

/// Stable unsupported-feature classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedFeature {
    /// Generator functions and `yield` expressions.
    Generator,
    /// `yield from` delegation has generator semantics beyond runtime.
    YieldFrom,
    /// Fiber construction is runtime-sensitive.
    Fiber,
    /// `eval` is intentionally deferred to a runtime-aware layer.
    Eval,
    /// Autoload registration and autoload-sensitive lookup are deferred.
    Autoload,
    /// Reflection objects and metadata are deferred.
    Reflection,
    /// Trait declarations, uses, and composition are deferred.
    TraitRuntime,
    /// Enum declarations and runtime objects are deferred.
    EnumRuntime,
    /// Property hook execution is deferred.
    PropertyHooks,
    /// Full PHP references and Copy-on-Write semantics are deferred.
    FullReferences,
    /// HIR statement family not yet lowered by runtime.
    HirStatement,
    /// `for` headers with multiple expressions in one section are deferred.
    ForHeaderMultiExpression,
    /// Dynamic or out-of-range `break`/`continue` levels are deferred.
    DynamicLoopControlLevel,
    /// Dynamic function calls are deferred until callable semantics are stable.
    DynamicFunctionCall,
    /// By-reference parameters are outside the call-frame subset.
    ByReferenceParameter,
    /// By-reference returns are recorded by Semantic frontend but not executable yet.
    ByReferenceReturn,
    /// Parameter defaults not proven foldable by Semantic frontend are not executed by
    /// the VM.
    AdvancedParameter,
    /// Array spread/unpack is deferred until array merge semantics are modeled.
    ArraySpread,
    /// By-reference foreach requires reference slots/COW support.
    ByReferenceForeach,
    /// References to array elements require full PHP reference/COW semantics.
    ArrayElementReference,
    /// References to object properties require property slot/lvalue plumbing.
    ObjectPropertyReference,
    /// Method calls require a statically-known object and method target.
    MethodCall,
    /// Static method calls require an explicit class name in the MVP.
    LateStaticBinding,
    /// Static properties are not modeled by the object MVP.
    StaticProperty,
    /// Non-class class-like declarations are outside the object MVP.
    ClassLikeObject,
    /// Method modifiers outside public instance methods are outside the object MVP.
    ObjectMethodModifier,
    /// Property modifiers outside public untyped instance properties are outside the object MVP.
    ObjectPropertyModifier,
    /// Catch types outside `Exception`/`Throwable` are outside the exception MVP.
    CatchType,
}

/// Class-like declaration that owns a missing trait use.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingTraitOwnerKind {
    /// Named class declaration.
    Class,
    /// Interface declaration.
    Interface,
    /// Trait declaration.
    Trait,
    /// Enum declaration.
    Enum,
    /// Anonymous class expression.
    AnonymousClass,
}

/// Structured data for an unresolved trait composition request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MissingTraitDiagnostic {
    /// Stable machine-readable code independent of rendered prose.
    pub code: String,
    /// Fully resolved and normalized trait name used for lookup.
    pub normalized_name: String,
    /// Trait spelling used at the source site, including aliases when present.
    pub display_name: String,
    /// Normalized name of the class-like declaration using the trait.
    pub owner_normalized_name: String,
    /// Source-facing name of the class-like declaration using the trait.
    pub owner_display_name: String,
    /// Kind of class-like declaration using the trait.
    pub owner_kind: MissingTraitOwnerKind,
    /// Source path recorded in the IR file table.
    pub source_name: String,
    /// Source file and byte span of the trait-use declaration.
    pub span: IrSpan,
}

impl MissingTraitDiagnostic {
    /// Creates one structured missing-trait diagnostic payload.
    #[must_use]
    pub fn new(
        normalized_name: impl Into<String>,
        display_name: impl Into<String>,
        owner_normalized_name: impl Into<String>,
        owner_display_name: impl Into<String>,
        owner_kind: MissingTraitOwnerKind,
        source_name: impl Into<String>,
        span: IrSpan,
    ) -> Self {
        Self {
            code: MISSING_TRAIT_DIAGNOSTIC_CODE.to_string(),
            normalized_name: normalized_name.into(),
            display_name: display_name.into(),
            owner_normalized_name: owner_normalized_name.into(),
            owner_display_name: owner_display_name.into(),
            owner_kind,
            source_name: source_name.into(),
            span,
        }
    }
}

/// Typed lowering data used by compiler control flow.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoweringDiagnosticPayload {
    /// A trait-use declaration refers to a trait absent from this compilation.
    MissingTrait(MissingTraitDiagnostic),
}

impl UnsupportedFeature {
    /// Stable diagnostic ID for this unsupported feature.
    #[must_use]
    pub const fn diagnostic_id(self) -> &'static str {
        match self {
            Self::Generator => "E_PHP_IR_UNSUPPORTED_GENERATOR",
            Self::YieldFrom => "E_PHP_IR_UNSUPPORTED_YIELD_FROM",
            Self::Fiber => "E_PHP_IR_UNSUPPORTED_FIBER",
            Self::Eval => "E_PHP_IR_UNSUPPORTED_EVAL",
            Self::Autoload => "E_PHP_IR_UNSUPPORTED_AUTOLOAD",
            Self::Reflection => "E_PHP_IR_UNSUPPORTED_REFLECTION",
            Self::TraitRuntime => "E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME",
            Self::EnumRuntime => "E_PHP_IR_UNSUPPORTED_ENUM_RUNTIME",
            Self::PropertyHooks => "E_PHP_IR_UNSUPPORTED_PROPERTY_HOOKS",
            Self::FullReferences => "E_PHP_IR_UNSUPPORTED_REFERENCE_SEMANTICS",
            Self::HirStatement => "E_PHP_IR_UNSUPPORTED_HIR_STATEMENT",
            Self::ForHeaderMultiExpression => "E_PHP_IR_UNSUPPORTED_FOR_HEADER_MULTI_EXPR",
            Self::DynamicLoopControlLevel => "E_PHP_IR_UNSUPPORTED_DYNAMIC_LOOP_CONTROL_LEVEL",
            Self::DynamicFunctionCall => "E_PHP_IR_UNSUPPORTED_DYNAMIC_FUNCTION_CALL",
            Self::ByReferenceParameter => "E_PHP_IR_UNSUPPORTED_BY_REF_PARAMETER",
            Self::ByReferenceReturn => "E_PHP_IR_UNSUPPORTED_BY_REF_RETURN",
            Self::AdvancedParameter => "E_PHP_IR_UNSUPPORTED_ADVANCED_PARAMETER",
            Self::ArraySpread => "E_PHP_IR_UNSUPPORTED_ARRAY_SPREAD",
            Self::ByReferenceForeach => "E_PHP_IR_UNSUPPORTED_BY_REF_FOREACH",
            Self::ArrayElementReference => "E_PHP_IR_UNSUPPORTED_ARRAY_ELEMENT_REFERENCE",
            Self::ObjectPropertyReference => "E_PHP_IR_UNSUPPORTED_PROPERTY_REFERENCE",
            Self::MethodCall => "E_PHP_IR_UNSUPPORTED_METHOD_CALL",
            Self::LateStaticBinding => "E_PHP_IR_UNSUPPORTED_LATE_STATIC_BINDING",
            Self::StaticProperty => "E_PHP_IR_UNSUPPORTED_STATIC_PROPERTY",
            Self::ClassLikeObject => "E_PHP_IR_UNSUPPORTED_CLASSLIKE_OBJECT",
            Self::ObjectMethodModifier => "E_PHP_IR_UNSUPPORTED_OBJECT_METHOD_MODIFIER",
            Self::ObjectPropertyModifier => "E_PHP_IR_UNSUPPORTED_OBJECT_PROPERTY_MODIFIER",
            Self::CatchType => "E_PHP_IR_UNSUPPORTED_CATCH_TYPE",
        }
    }

    /// Stable feature spelling for diagnostic context.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generator => "generator",
            Self::YieldFrom => "yield_from",
            Self::Fiber => "fiber",
            Self::Eval => "eval",
            Self::Autoload => "autoload",
            Self::Reflection => "reflection",
            Self::TraitRuntime => "trait_runtime",
            Self::EnumRuntime => "enum_runtime",
            Self::PropertyHooks => "property_hooks",
            Self::FullReferences => "full_references",
            Self::HirStatement => "hir_statement",
            Self::ForHeaderMultiExpression => "for_header_multi_expression",
            Self::DynamicLoopControlLevel => "dynamic_loop_control_level",
            Self::DynamicFunctionCall => "dynamic_function_call",
            Self::ByReferenceParameter => "by_reference_parameter",
            Self::ByReferenceReturn => "by_reference_return",
            Self::AdvancedParameter => "advanced_parameter",
            Self::ArraySpread => "array_spread",
            Self::ByReferenceForeach => "by_reference_foreach",
            Self::ArrayElementReference => "array_element_reference",
            Self::ObjectPropertyReference => "object_property_reference",
            Self::MethodCall => "method_call",
            Self::LateStaticBinding => "late_static_binding",
            Self::StaticProperty => "static_property",
            Self::ClassLikeObject => "class_like_object",
            Self::ObjectMethodModifier => "object_method_modifier",
            Self::ObjectPropertyModifier => "object_property_modifier",
            Self::CatchType => "catch_type",
        }
    }

    fn suggestion(self) -> &'static str {
        match self {
            Self::Eval => {
                "avoid eval or defer this script to a runtime path that supports dynamic compilation"
            }
            Self::Autoload => {
                "preload the required declarations or record the lookup as deferred metadata"
            }
            Self::Reflection => {
                "avoid reflection-dependent execution in lowered IR until reflection metadata is modeled"
            }
            Self::FullReferences
            | Self::ByReferenceParameter
            | Self::ByReferenceReturn
            | Self::ByReferenceForeach
            | Self::ArrayElementReference
            | Self::ObjectPropertyReference => {
                "rewrite this construct without PHP references or keep it as a known runtime gap"
            }
            _ => "rewrite this construct to the supported runtime subset or keep it as a known gap",
        }
    }
}

/// One runtime lowering diagnostic.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LoweringDiagnostic {
    /// Stable machine-readable diagnostic ID.
    pub id: String,
    /// Unsupported feature category.
    pub feature: UnsupportedFeature,
    /// Source span in the IR source file table.
    pub span: IrSpan,
    /// Human-readable message.
    pub message: String,
    /// Optional typed data for consumers that need to act on the diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<LoweringDiagnosticPayload>,
}

/// Optional context for rendering a lowering diagnostic as a shared envelope.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LoweringDiagnosticContext {
    /// Stable source identifier, if different from the path.
    pub source_id: Option<String>,
    /// HIR or source-map origin that produced the diagnostic.
    pub origin: Option<String>,
    /// Function currently being lowered.
    pub function: Option<FunctionId>,
    /// Basic block currently being emitted.
    pub block: Option<BlockId>,
    /// Instruction mapped from this diagnostic.
    pub instruction: Option<InstrId>,
    /// Class context, if available.
    pub class_name: Option<String>,
    /// Method context, if available.
    pub method_name: Option<String>,
}

impl LoweringDiagnostic {
    /// Creates a generic unsupported-feature diagnostic without typed payload.
    #[must_use]
    pub fn unsupported(
        feature: UnsupportedFeature,
        span: IrSpan,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: feature.diagnostic_id().to_string(),
            feature,
            span,
            message: message.into(),
            payload: None,
        }
    }

    /// Returns structured missing-trait data when this diagnostic carries it.
    #[must_use]
    pub fn missing_trait(&self) -> Option<&MissingTraitDiagnostic> {
        match self.payload.as_ref() {
            Some(LoweringDiagnosticPayload::MissingTrait(payload)) => Some(payload),
            None => None,
        }
    }

    /// Converts this lowering diagnostic to the shared diagnostic envelope.
    #[must_use]
    pub fn to_diagnostic_envelope(
        &self,
        source_path: Option<&str>,
        context: &LoweringDiagnosticContext,
    ) -> DiagnosticEnvelope {
        let mut metadata = BTreeMap::new();
        metadata.insert("feature".to_string(), self.feature.as_str().to_string());
        metadata.insert("file_id".to_string(), self.span.file.raw().to_string());
        if let Some(missing_trait) = self.missing_trait() {
            metadata.insert("typed_code".to_string(), missing_trait.code.clone());
            metadata.insert(
                "missing_trait".to_string(),
                missing_trait.normalized_name.clone(),
            );
            metadata.insert(
                "missing_trait_display".to_string(),
                missing_trait.display_name.clone(),
            );
            metadata.insert(
                "owner".to_string(),
                missing_trait.owner_normalized_name.clone(),
            );
            metadata.insert(
                "owner_display".to_string(),
                missing_trait.owner_display_name.clone(),
            );
        }
        if let Some(origin) = &context.origin {
            metadata.insert("origin".to_string(), origin.clone());
        }
        if let Some(function) = context.function {
            metadata.insert("function_id".to_string(), function.raw().to_string());
        }
        if let Some(block) = context.block {
            metadata.insert("block_id".to_string(), block.raw().to_string());
        }
        if let Some(instruction) = context.instruction {
            metadata.insert("instruction_id".to_string(), instruction.raw().to_string());
        }
        if let Some(class_name) = &context.class_name {
            metadata.insert("class".to_string(), class_name.clone());
        }
        if let Some(method_name) = &context.method_name {
            metadata.insert("method".to_string(), method_name.clone());
        }

        let mut envelope = DiagnosticEnvelope::new(
            self.id.clone(),
            DiagnosticLayer::ir(),
            DiagnosticPhase::new("lower"),
            DiagnosticSeverity::UnsupportedFeature,
            self.message.clone(),
        )
        .with_location(DiagnosticLocation::new(
            source_path,
            context.source_id.as_deref(),
            Some(DiagnosticSpan::new(
                self.span.start as usize,
                self.span.end as usize,
            )),
        ))
        .with_context(metadata);
        envelope.suggestion = Some(DiagnosticSuggestion::new(self.feature.suggestion()));
        envelope.php_visible = false;
        envelope
    }
}

#[derive(Clone, Debug)]
pub(super) struct EarlyDiagnostic {
    pub(super) origin: String,
    pub(super) span: IrSpan,
    pub(super) severity: IrDiagnosticSeverity,
    pub(super) diagnostic_id: String,
    pub(super) message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LoweringOptions, lower_frontend_result};
    use php_semantics::analyze_source;

    #[test]
    fn missing_trait_diagnostic_preserves_rendering_and_structured_identity() {
        let source = concat!(
            "<?php namespace App\\Feature; ",
            "use Vendor\\Package\\MiXeD_Trait_used_by_Name as Alias_9; ",
            "class OwnerThing { use Alias_9; }"
        );
        let frontend = analyze_source(source);
        let result = lower_frontend_result(
            &frontend,
            LoweringOptions {
                source_path: "fixtures/odd-trait.php".to_string(),
                source_text: Some(source.to_string()),
                ..LoweringOptions::default()
            },
        );
        let diagnostic = result
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.missing_trait().is_some())
            .expect("missing-trait diagnostic");
        let payload = diagnostic.missing_trait().expect("typed payload");

        assert_eq!(diagnostic.id, "E_PHP_IR_UNSUPPORTED_TRAIT_RUNTIME");
        assert_eq!(
            diagnostic.message,
            concat!(
                "E_PHP_IR_TRAIT_NOT_FOUND: trait ",
                "vendor\\package\\mixed_trait_used_by_name used by ",
                "app\\feature\\ownerthing is not declared"
            )
        );
        assert_eq!(payload.code, MISSING_TRAIT_DIAGNOSTIC_CODE);
        assert_eq!(
            payload.normalized_name,
            "vendor\\package\\mixed_trait_used_by_name"
        );
        assert_eq!(payload.display_name, "Alias_9");
        assert_eq!(payload.owner_normalized_name, "app\\feature\\ownerthing");
        assert_eq!(payload.owner_display_name, "App\\Feature\\OwnerThing");
        assert_eq!(payload.owner_kind, MissingTraitOwnerKind::Class);
        assert_eq!(payload.source_name, "fixtures/odd-trait.php");
        assert_eq!(payload.span, diagnostic.span);
    }

    #[test]
    fn missing_trait_payload_round_trips_deterministically() {
        let source = "<?php namespace Demo; trait OwnerTrait { use \\Vendor\\Qualified_Trait; }";
        let frontend = analyze_source(source);
        let result = lower_frontend_result(
            &frontend,
            LoweringOptions {
                source_path: "fixtures/qualified.php".to_string(),
                source_text: Some(source.to_string()),
                ..LoweringOptions::default()
            },
        );
        let diagnostic = result
            .diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.missing_trait().is_some())
            .expect("missing-trait diagnostic");

        let json = serde_json::to_string(&diagnostic).expect("serialize diagnostic");
        let decoded: LoweringDiagnostic =
            serde_json::from_str(&json).expect("deserialize diagnostic");
        assert_eq!(decoded, diagnostic);
        assert_eq!(
            serde_json::to_string(&decoded).expect("serialize decoded diagnostic"),
            json
        );
        let payload = decoded.missing_trait().expect("typed payload");
        assert_eq!(payload.normalized_name, "vendor\\qualified_trait");
        assert_eq!(payload.display_name, "\\Vendor\\Qualified_Trait");
        assert_eq!(payload.owner_normalized_name, "demo\\ownertrait");
        assert_eq!(payload.owner_kind, MissingTraitOwnerKind::Trait);
    }
}
