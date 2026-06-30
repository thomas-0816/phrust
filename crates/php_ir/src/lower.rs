//! Semantic frontend frontend to runtime IR lowering skeleton.

use crate::builder::IrBuilder;
use crate::constants::{IrConstant, IrConstantArrayEntry};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::function::{FunctionFlags, IrCapture, IrParam, IrReturnType};
use crate::ids::{BlockId, FileId, FunctionId, InstrId, LocalId, RegId, UnitId};
use crate::instruction::{
    BinaryOp, CallableKind, CastKind, ClosureCaptureArg, CompareOp, IncludeKind, InstructionKind,
    IrCallArg, IrCallArgValueKind, IrCallDimTarget, IrCallPropertyTarget, IrDiagnosticSeverity,
    UnaryOp,
};
use crate::literal_text::{
    InterpolatedDim, InterpolatedPart, heredoc_literal_body, interpolated_literal_parts,
    quoted_literal_body,
};
use crate::module::{
    AttributeEntry, ClassConstantEntry, ClassConstantFlags, ClassConstantReference, ClassEntry,
    ClassEnumBackingType, ClassEnumCaseEntry, ClassFlags, ClassMethodEntry, ClassMethodFlags,
    ClassPropertyEntry, ClassPropertyFlags, ClassPropertyHooks, DeferredConstArrayEntry,
    DeferredConstExpr, IrUnit, NamedConstantReference, display_class_name, normalize_class_name,
};
use crate::operand::Operand;
use crate::source_map::{IrSourceMapTarget, IrSpan};
use crate::verify::{VerificationError, verify_unit};
use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticLocation, DiagnosticPhase, DiagnosticSeverity,
    DiagnosticSpan, DiagnosticSuggestion,
};
use php_semantics::hir::{
    AttributeId, AttributeTarget, BuiltinType, ClassLikeId, ClassLikeKind, ClassLikeMemberId,
    ConstExprContext, ConstExprId, ConstValue, DeclareValue, DefaultValueRef, ExprId,
    FunctionSignature, HirCallArg, HirCatchClause, HirClassLike, HirExprKind, HirIfBranch,
    HirMatchArm, HirModule, HirNameResolution, HirProperty, HirPropertyHookBody, HirStmtKind,
    HirSwitchCase, HirTraitAdaptationKind, HirTypeKind, MagicMethodKind, ModifierSet, NameKind,
    Parameter, ParameterAttribute, ReturnType, SignatureKind, StmtId, TopLevelItemKind, TypeId,
    Visibility,
};
use php_semantics::scopes::CaptureMode;
use php_semantics::symbols::declarations::DeclarationKind;
use php_semantics::{FrontendResult, SourceMappedId};
use php_source::{BytePos, SourceText, TextRange};
use serde::{Deserialize, Serialize};

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

struct HirTryParts {
    body: Vec<StmtId>,
    catches: Vec<HirCatchClause>,
    finally_body: Vec<StmtId>,
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

/// Options for the skeleton lowering entrypoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoweringOptions {
    /// IR unit ID to assign.
    pub unit_id: UnitId,
    /// Source path used in the IR file table.
    pub source_path: String,
    /// Source text used for line-sensitive magic constants.
    pub source_text: Option<String>,
    /// Whether unsupported HIR features should also produce IR marker
    /// instructions in the top-level block.
    pub emit_unsupported_instructions: bool,
}

impl Default for LoweringOptions {
    fn default() -> Self {
        Self {
            unit_id: UnitId::new(0),
            source_path: "<memory>".to_string(),
            source_text: None,
            emit_unsupported_instructions: true,
        }
    }
}

/// Lowering context for one frontend result.
#[derive(Debug)]
pub struct LoweringContext<'a> {
    frontend: &'a FrontendResult,
    options: LoweringOptions,
    file: FileId,
    diagnostics: Vec<LoweringDiagnostic>,
    loop_stack: Vec<LoopTargets>,
    label_blocks: HashMap<FunctionId, HashMap<String, BlockId>>,
    closure_functions: HashMap<ExprId, FunctionId>,
    function_names: HashMap<FunctionId, String>,
    class_names: HashMap<FunctionId, String>,
    method_names: HashMap<FunctionId, String>,
    source_text: SourceText,
    early_diagnostics: HashMap<FunctionId, Vec<EarlyDiagnostic>>,
}

impl<'a> LoweringContext<'a> {
    /// Creates a lowering context.
    #[must_use]
    pub fn new(frontend: &'a FrontendResult, options: LoweringOptions, file: FileId) -> Self {
        let source_text = SourceText::new(options.source_text.clone().unwrap_or_default());
        Self {
            frontend,
            options,
            file,
            diagnostics: Vec::new(),
            loop_stack: Vec::new(),
            label_blocks: HashMap::new(),
            closure_functions: HashMap::new(),
            function_names: HashMap::new(),
            class_names: HashMap::new(),
            method_names: HashMap::new(),
            source_text,
            early_diagnostics: HashMap::new(),
        }
    }

    /// Returns collected diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &[LoweringDiagnostic] {
        &self.diagnostics
    }

    fn record_early_diagnostic(
        &mut self,
        function: FunctionId,
        expr: ExprId,
        span: IrSpan,
        severity: IrDiagnosticSeverity,
        diagnostic_id: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.early_diagnostics
            .entry(function)
            .or_default()
            .push(EarlyDiagnostic {
                origin: format!("hir:expr:{}", expr.raw()),
                span,
                severity,
                diagnostic_id: diagnostic_id.into(),
                message: message.into(),
            });
    }

    fn record_early_diagnostic_origin(
        &mut self,
        function: FunctionId,
        origin: impl Into<String>,
        span: IrSpan,
        severity: IrDiagnosticSeverity,
        diagnostic_id: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.early_diagnostics
            .entry(function)
            .or_default()
            .push(EarlyDiagnostic {
                origin: origin.into(),
                span,
                severity,
                diagnostic_id: diagnostic_id.into(),
                message: message.into(),
            });
    }

    fn doc_comment_before(&self, range: TextRange) -> Option<String> {
        let text = self.source_text.as_str();
        let declaration_start = range.start().to_usize().min(text.len());
        let before = &text[..declaration_start];
        let trimmed = before.trim_end_matches(|ch: char| ch.is_whitespace());
        let comment_end = trimmed.len();
        let comment_start = trimmed.rfind("/**")?;
        let comment = &trimmed[comment_start..comment_end];
        if !comment.ends_with("*/") {
            return None;
        }
        Some(comment.to_owned())
    }

    fn emit_early_diagnostics(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
    ) {
        let Some(diagnostics) = self.early_diagnostics.remove(&function) else {
            return;
        };
        for (index, diagnostic) in diagnostics.into_iter().enumerate() {
            let instruction = builder.emit(
                function,
                block,
                InstructionKind::EmitDiagnostic {
                    severity: diagnostic.severity,
                    diagnostic_id: diagnostic.diagnostic_id,
                    message: diagnostic.message,
                    leading_newline: index > 0,
                },
                diagnostic.span,
            );
            builder.add_source_map(
                IrSourceMapTarget::Instruction {
                    function,
                    block,
                    instruction,
                },
                diagnostic.origin,
                diagnostic.span,
            );
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

/// Result of lowering one frontend file.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweringResult {
    /// Lowered IR unit.
    pub unit: IrUnit,
    /// Lowering diagnostics.
    pub diagnostics: Vec<LoweringDiagnostic>,
    /// Verifier result for the produced unit.
    pub verification: Result<(), Vec<VerificationError>>,
}

#[derive(Clone, Copy, Debug)]
struct LowerSite {
    function: FunctionId,
    block: BlockId,
    expr: ExprId,
    span: IrSpan,
    range: TextRange,
}

#[derive(Clone, Copy, Debug)]
struct LoopTargets {
    break_block: BlockId,
    continue_block: BlockId,
}

#[derive(Clone, Copy, Debug)]
struct ConditionTargets {
    true_target: BlockId,
    false_target: BlockId,
    span: IrSpan,
}

#[derive(Clone, Copy, Debug)]
struct MethodFunctionInput<'a> {
    class_name: &'a str,
    method_name: &'a str,
    display_class_name: &'a str,
    display_method_name: &'a str,
    signature: &'a FunctionSignature,
    class_constant_initializers: &'a ClassConstantInitializerMap,
    class_parents: &'a ClassParentMap,
    main_function: FunctionId,
}

struct DeferredConstExprLoweringInput<'a> {
    module: &'a HirModule,
    named_constants: &'a HashMap<String, IrConstant>,
    current_class: Option<&'a str>,
    class_constants: &'a ClassConstantInitializerMap,
    class_parents: &'a ClassParentMap,
    visiting_class_constants: &'a mut Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug)]
struct LoweredExpr {
    register: crate::ids::RegId,
    block: BlockId,
}

#[derive(Clone, Debug)]
struct EarlyDiagnostic {
    origin: String,
    span: IrSpan,
    severity: IrDiagnosticSeverity,
    diagnostic_id: String,
    message: String,
}

#[derive(Clone, Debug)]
struct IfParts {
    condition: Option<ExprId>,
    body: Vec<StmtId>,
    elseifs: Vec<HirIfBranch>,
    else_body: Vec<StmtId>,
}

#[derive(Clone, Debug)]
struct ForParts {
    init: Vec<ExprId>,
    condition: Vec<ExprId>,
    update: Vec<ExprId>,
    body: Vec<StmtId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CaptureSpec {
    name: String,
    by_ref: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StaticLocalSpec {
    name: String,
    initializer: Option<ExprId>,
}

#[derive(Clone, Debug)]
struct DimAssignmentTarget {
    local: LocalId,
    dims: Vec<ExprId>,
    append: bool,
}

#[derive(Clone, Debug)]
struct PropertyAssignmentTarget {
    receiver: ExprId,
    property: String,
}

#[derive(Clone, Debug)]
struct DynamicPropertyTarget {
    receiver: ExprId,
    property: ExprId,
}

#[derive(Clone, Debug)]
struct DynamicPropertyDimTarget {
    receiver: ExprId,
    property: ExprId,
    dims: Vec<ExprId>,
    append: bool,
}

#[derive(Clone, Debug)]
struct DynamicMethodCallTarget {
    receiver: ExprId,
    method: ExprId,
}

#[derive(Clone, Debug)]
struct PropertyDimTarget {
    receiver: ExprId,
    property: String,
    dims: Vec<ExprId>,
    append: bool,
}

#[derive(Clone, Debug)]
struct StaticPropertyTarget {
    class_name: String,
    property: String,
}

#[derive(Clone, Debug)]
struct StaticPropertyDimTarget {
    class_name: String,
    property: String,
    dims: Vec<ExprId>,
    append: bool,
}

#[derive(Clone, Debug)]
struct ClassConstantTarget {
    class_name: String,
    display_class_name: Option<String>,
    constant: String,
}

type ClassConstantInitializerMap = HashMap<String, HashMap<String, ConstExprId>>;
type ClassParentMap = HashMap<String, Option<String>>;

#[derive(Clone, Debug)]
struct ObjectClassNameTarget {
    object: ExprId,
}

#[derive(Clone, Debug)]
struct MethodCallTarget {
    receiver: ExprId,
    method: String,
}

#[derive(Clone, Debug)]
struct StaticMethodCallTarget {
    class_name: String,
    method: String,
}

#[derive(Clone, Debug)]
enum CallableComponent {
    Expr(ExprId),
    String(String),
}

#[derive(Clone, Debug)]
struct TraitMethodCandidate {
    trait_name: String,
    display_trait_name: String,
    method_name: String,
    display_method_name: String,
    signature: FunctionSignature,
    flags: ClassMethodFlags,
}

#[derive(Clone, Debug)]
struct TraitAliasSpec {
    trait_name: Option<String>,
    method_name: String,
    alias: Option<String>,
    visibility: Option<TraitVisibility>,
}

struct TraitCompositionInput<'a> {
    module: &'a HirModule,
    trait_class_likes: &'a HashMap<String, (ClassLikeId, php_semantics::hir::HirClassLike)>,
    main_function: FunctionId,
    class_like_id: ClassLikeId,
    class_like: &'a php_semantics::hir::HirClassLike,
    class_name: &'a str,
    display_class_name: &'a str,
    class_constant_initializers: &'a ClassConstantInitializerMap,
    class_parents: &'a ClassParentMap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TraitVisibility {
    Public,
    Protected,
    Private,
}

impl TraitVisibility {
    fn from_text(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "public" => Some(Self::Public),
            "protected" => Some(Self::Protected),
            "private" => Some(Self::Private),
            _ => None,
        }
    }

    fn apply(self, flags: &mut ClassMethodFlags) {
        flags.is_private = self == Self::Private;
        flags.is_protected = self == Self::Protected;
    }
}

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

impl LoweringContext<'_> {
    fn lower_auto_global_bindings(
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

    fn lower_global_constant_declarations(
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

    fn lower_class_declarations(&mut self, builder: &mut IrBuilder, main_function: FunctionId) {
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
        let class_constant_initializers = collect_class_constant_initializers(module, &class_likes);
        let class_parents = collect_class_parents(&class_likes);
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
        self.push_internal_interfaces(builder, &declared_class_likes);
        for (class_like_id, class_like) in class_likes {
            if !matches!(
                class_like.kind(),
                ClassLikeKind::Class
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
            let Some(name) = class_like
                .fqn()
                .map(|name| name.canonical(NameKind::ClassLike))
                .or_else(|| class_like.name().map(normalize_class_name))
            else {
                continue;
            };
            let display_class_name = class_like_display_name(&class_like, &name);
            let name = normalize_class_name(&name);
            let span = span_from_range(
                self.file,
                self.span_for(SourceMappedId::from(class_like_id)),
            );
            let parent = class_like.extends().first().map(|name| {
                normalize_class_name(
                    name.resolved()
                        .or_else(|| name.fallback())
                        .unwrap_or_else(|| name.source()),
                )
            });
            let parent = (class_like.kind() == ClassLikeKind::Class)
                .then_some(parent)
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
                        if signature.flags().is_generator() {
                            self.unsupported(
                                UnsupportedFeature::Generator,
                                signature.span(),
                                "generator methods are not executable in the object-runtime object MVP",
                            );
                            continue;
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
                        let property_type = self.lower_runtime_type(property.type_id());
                        let hooks = self.lower_property_hooks(
                            builder,
                            &name,
                            &display_class_name,
                            property,
                        );
                        let set_visibility = property.modifiers().set_visibility();
                        for item in property.items() {
                            let default = self
                                .lower_property_default(
                                    item.default(),
                                    Some(&name),
                                    Some(&display_class_name),
                                    &class_constant_initializers,
                                    &class_parents,
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
                                    Some(&name),
                                    &class_parents,
                                )
                            } else {
                                None
                            };
                            let default_named_constant =
                                if default.is_none() && default_class_constant.is_none() {
                                    self.lower_const_expr_named_constant_reference(
                                        item.default(),
                                        |context| {
                                            matches!(
                                                context,
                                                ConstExprContext::PropertyDefault
                                                    | ConstExprContext::PromotedPropertyDefault
                                            )
                                        },
                                    )
                                } else {
                                    None
                                };
                            let default_expr = if default.is_none()
                                && default_class_constant.is_none()
                                && default_named_constant.is_none()
                            {
                                self.lower_deferred_property_default(
                                    item.default(),
                                    Some(&name),
                                    Some(&display_class_name),
                                    &class_constant_initializers,
                                    &class_parents,
                                )
                            } else {
                                None
                            };
                            properties.push(ClassPropertyEntry {
                                name: local_name(item.name()).to_owned(),
                                default,
                                default_class_constant,
                                default_named_constant,
                                default_expr,
                                type_: property_type.clone(),
                                flags: ClassPropertyFlags {
                                    is_static: property.modifiers().is_static(),
                                    is_private: property.modifiers().visibility().is_some_and(
                                        |visibility| visibility == Visibility::Private,
                                    ),
                                    is_protected: property.modifiers().visibility().is_some_and(
                                        |visibility| visibility == Visibility::Protected,
                                    ),
                                    set_is_private: set_visibility.is_some_and(|visibility| {
                                        visibility == Visibility::Private
                                    }),
                                    set_is_protected: set_visibility.is_some_and(|visibility| {
                                        visibility == Visibility::Protected
                                    }),
                                    is_readonly: property.modifiers().is_readonly(),
                                    is_typed: property.type_id().is_some(),
                                },
                                hooks: hooks.clone(),
                                attributes: self
                                    .lower_attribute_ids(builder, property.attributes()),
                            });
                        }
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
                },
                span,
            });
        }
    }

    fn push_promoted_constructor_properties(
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

    fn push_internal_interfaces(&mut self, builder: &mut IrBuilder, declared: &HashSet<String>) {
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
                },
                span: IrSpan::default(),
            });
        }
    }

    fn lower_method_function(
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

    fn lower_constructor_property_promotions(
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

    fn lower_property_hooks(
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

    fn compose_trait_methods(
        &mut self,
        builder: &mut IrBuilder,
        input: TraitCompositionInput<'_>,
        methods: &mut Vec<ClassMethodEntry>,
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
                let trait_name = trait_resolution_name(trait_name);
                let Some((_trait_id, trait_class_like)) = trait_class_likes.get(&trait_name) else {
                    self.unsupported(
                        UnsupportedFeature::TraitRuntime,
                        self.span_for(SourceMappedId::from(trait_use_id)),
                        format!(
                            "E_PHP_IR_TRAIT_NOT_FOUND: trait {trait_name} used by {class_name} is not declared"
                        ),
                    );
                    continue;
                };
                self.collect_trait_method_candidates(
                    module,
                    trait_class_like,
                    &trait_name,
                    &mut candidates,
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

    fn collect_trait_method_candidates(
        &mut self,
        module: &HirModule,
        trait_class_like: &php_semantics::hir::HirClassLike,
        trait_name: &str,
        candidates: &mut Vec<TraitMethodCandidate>,
    ) {
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
                    if signature.flags().is_generator() {
                        self.unsupported(
                            UnsupportedFeature::Generator,
                            signature.span(),
                            "generator trait methods are not executable in the trait-composition trait MVP",
                        );
                        continue;
                    }
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
                    self.unsupported(
                        UnsupportedFeature::TraitRuntime,
                        self.span_for(SourceMappedId::from(property_id)),
                        "trait properties are not executable in the trait-composition trait-method composition layer",
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

    fn global_const_initializers(&self) -> Vec<(ExprId, Option<IrConstant>)> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id());
        let Some(module) = module else {
            return Vec::new();
        };
        let named_constants = predefined_constant_initializer_map();
        let class_likes = module
            .class_likes()
            .iter()
            .map(|(id, class_like)| (id, class_like.clone()))
            .collect::<Vec<_>>();
        let class_constant_initializers = collect_class_constant_initializers(module, &class_likes);
        let class_parents = collect_class_parents(&class_likes);
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
                    &named_constants,
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

    fn global_constant_initializer_map(&self) -> HashMap<String, IrConstant> {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return HashMap::new();
        };
        let mut map = predefined_constant_initializer_map();
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

    fn lower_function_declarations(&mut self, builder: &mut IrBuilder, main_function: FunctionId) {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return;
        };
        let signatures = module.signatures().to_vec();
        let mut registered_conditional_names = HashSet::new();
        for signature in signatures {
            if signature.kind() != SignatureKind::Function {
                continue;
            }
            let Some(name) = signature.name() else {
                continue;
            };
            let registered_name = qualified_function_name(module, &signature, name);
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
            self.function_names.insert(function, name.to_string());
            let normalized_name = normalize_function_name(&registered_name);
            let declaration_kind = function_declaration_kind(module, &signature, &normalized_name);
            if declaration_kind != Some(DeclarationKind::ConditionalFunction)
                || registered_conditional_names.insert(normalized_name.clone())
            {
                builder.register_function_name(normalized_name, function);
            }
            builder.set_return_type(function, self.lower_return_type(signature.return_type()));
            builder.set_returns_by_ref(function, signature.by_ref_return());
            builder.add_source_map(
                IrSourceMapTarget::Function { function },
                format!("hir:function:{name}"),
                span,
            );
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
                            name,
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

    fn lower_param_default(&self, param: &Parameter) -> Option<IrConstant> {
        self.lower_param_default_with_class_constants(param, None, &HashMap::new(), &HashMap::new())
    }

    fn lower_param_default_with_class_constants(
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
                constant_from_expr_with_class_constants(
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

    fn lower_param_runtime_type(
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

    fn param_default_triggers_implicit_nullable_deprecation(
        &self,
        param: &Parameter,
        default: &Option<IrConstant>,
    ) -> bool {
        default == &Some(IrConstant::Null)
            && self.param_type_triggers_implicit_nullable_deprecation(param)
    }

    fn param_type_triggers_implicit_nullable_deprecation(&self, param: &Parameter) -> bool {
        let Some(type_id) = param.type_id() else {
            return false;
        };
        !self.type_accepts_null(type_id)
    }

    fn type_accepts_null(&self, type_id: TypeId) -> bool {
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

    fn lower_property_default(
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

    fn lower_deferred_property_default(
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

    fn lower_deferred_const_expr(
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

    fn lower_deferred_class_constant_reference(
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

    fn lower_class_constant_value(
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

    fn lower_const_expr_magic_constant(
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
            "__METHOD__" | "__FUNCTION__" | "__NAMESPACE__" => {
                Some(IrConstant::String(String::new()))
            }
            _ => None,
        }
    }

    fn lower_enum_case_value(&self, value: Option<ConstExprId>) -> Option<IrConstant> {
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

    fn lower_enum_backing_type(
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

    fn lower_attribute_ids(
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

    fn lower_parameter_attributes(
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

    fn lower_attributes_for_target_span(
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

    fn lower_attribute_argument(&self, expr_id: ExprId) -> Option<IrConstant> {
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

    fn lower_const_expr_value(
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

    fn lower_const_expr_class_constant_reference(
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

    fn lower_const_expr_named_constant_reference(
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

    fn lower_return_type(&self, return_type: Option<&ReturnType>) -> Option<IrReturnType> {
        self.lower_runtime_type(return_type.map(|return_type| return_type.type_id()))
    }

    fn lower_runtime_type(&self, type_id: Option<TypeId>) -> Option<IrReturnType> {
        let type_id = type_id?;
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let ty = module.types().get(type_id)?;
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
            HirTypeKind::Named { name, .. } => Some(IrReturnType::Class {
                name: name.original().to_owned(),
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

    fn lower_runtime_type_members(&self, members: &[TypeId]) -> Option<Vec<IrReturnType>> {
        members
            .iter()
            .map(|member| self.lower_runtime_type(Some(*member)))
            .collect()
    }

    fn lower_top_level(
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

        let mut statements = Vec::new();
        for namespace in module.namespaces().values() {
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
        }

        self.lower_stmt_list(builder, function, block, statements)
    }

    fn lower_stmt(
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
                self.unsupported(
                    UnsupportedFeature::HirStatement,
                    span,
                    format!("HIR statement `{}` is not lowered to IR yet", kind.as_str()),
                );
                block
            }
        }
    }

    fn lower_global_stmt(
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

    fn global_names_from_stmt_source(&mut self, stmt_id: StmtId) -> Vec<String> {
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

    fn lower_static_stmt(
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

    fn lower_echo_expr(
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

    fn lower_inline_html_stmt(
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

    fn lower_if_stmt(
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

    fn lower_while_stmt(
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

    fn lower_do_while_stmt(
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

    fn lower_for_stmt(
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
    fn lower_foreach_stmt(
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
    fn lower_foreach_ref_local(
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

    fn foreach_destructuring_targets(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        value_target: ExprId,
    ) -> Option<Vec<(i64, LocalId)>> {
        let target_exprs = {
            let module = self
                .frontend
                .database()
                .module(self.frontend.module().module_id())?;
            let expression = module.expressions().get(value_target)?;
            let elements = match expression.kind().clone() {
                HirExprKind::Array { elements } | HirExprKind::List { elements } => elements,
                _ => return None,
            };
            let mut target_exprs = Vec::new();
            for (index, element) in elements.into_iter().enumerate() {
                let element_expression = module.expressions().get(element)?;
                let target = match element_expression.kind().clone() {
                    HirExprKind::ArrayPair {
                        key: None,
                        value: Some(value),
                        unpack: false,
                        by_ref: false,
                    } => value,
                    HirExprKind::ArrayPair { .. } => return None,
                    _ => element,
                };
                target_exprs.push((index.try_into().ok()?, target));
            }
            target_exprs
        };
        let mut targets = Vec::new();
        for (index, target) in target_exprs {
            let local = self.variable_local(builder, function, target)?;
            targets.push((index, local));
        }
        Some(targets)
    }

    fn lower_foreach_value_destructure(
        &mut self,
        builder: &mut IrBuilder,
        function: FunctionId,
        block: BlockId,
        value: RegId,
        targets: Vec<(i64, LocalId)>,
        span: IrSpan,
    ) -> BlockId {
        for (index, local) in targets {
            let key = builder.intern_constant(IrConstant::Int(index));
            let fetched = builder.alloc_register(function);
            builder.emit(
                function,
                block,
                InstructionKind::FetchDim {
                    dst: fetched,
                    array: Operand::Register(value),
                    key: Operand::Constant(key),
                    quiet: false,
                },
                span,
            );
            builder.emit(
                function,
                block,
                InstructionKind::StoreLocal {
                    local,
                    src: Operand::Register(fetched),
                },
                span,
            );
        }
        block
    }

    fn lower_break_or_continue(
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

    fn lower_exit_stmt(
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

    fn lower_return_stmt(
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

    fn lower_throw_stmt(
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

    fn lower_try_stmt(
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
                .map(|ty| normalize_class_name(ty))
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

    fn lower_switch_stmt(
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
            .or_else(|| case_blocks.first().copied())
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

    fn lower_stmt_list(
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

    fn ensure_label_blocks(
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

    fn collect_label_statements(&self, statements: &[StmtId]) -> Vec<(String, StmtId)> {
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

    fn collect_label_statements_into(
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

    fn is_label_stmt(&self, stmt: StmtId) -> bool {
        self.frontend
            .database()
            .module(self.frontend.module().module_id())
            .and_then(|module| module.statements().get(stmt))
            .is_some_and(|statement| matches!(statement.kind(), HirStmtKind::Label { .. }))
    }

    fn lower_label_stmt(
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

    fn lower_goto_stmt(
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

    fn lower_expr_stmt(
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

    fn lower_unset_stmt(
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
                let Some(property) =
                    self.lower_expr_to_register(builder, function, current, target.property)
                else {
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

    fn dim_assignment_target(
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
                if target.append {
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
    fn terminate_condition_targets(
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

    fn jump_if_open(
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

    fn lower_expr_to_register(
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

    fn lower_yield_to_register(
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

    fn lower_yield_from_to_register(
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

    fn lower_new_object_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        class: Option<ExprId>,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        let Some(class) = class else {
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
        let display_class_name = self
            .static_class_source_name(class)
            .unwrap_or_else(|| class_name.clone());
        let normalized_class_name = normalize_class_name(&class_name);
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
                class_name: display_class_name,
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

    fn lower_property_fetch_to_register(
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
                let property_value =
                    self.lower_expr_to_register(builder, site.function, value_block, property)?;
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
            self.lower_expr_to_register(builder, site.function, object.block, property)?;
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

    fn lower_dynamic_member_name_to_register(
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
            HirExprKind::Literal { text } if text.starts_with('$') => Some(local_name(text)),
            HirExprKind::Name { resolution } if resolution.source().starts_with('$') => {
                Some(local_name(resolution.source()))
            }
            _ => None,
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

    fn lower_static_access_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
    ) -> Option<LoweredExpr> {
        if let Some(target) = self.static_property_target(site.expr) {
            return self.lower_static_property_fetch_to_register(builder, site, target);
        }
        if let Some(target) = self.class_constant_target(site.expr) {
            let normalized_class_name = normalize_class_name(&target.class_name);
            if target.constant.eq_ignore_ascii_case("class")
                && !matches!(normalized_class_name.as_str(), "self" | "static" | "parent")
            {
                let dst = builder.alloc_register(site.function);
                let constant = builder.intern_constant(IrConstant::String(
                    target.display_class_name.unwrap_or(target.class_name),
                ));
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
                    class_name: target.class_name,
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

    fn lower_static_property_fetch_to_register(
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

    fn lower_dim_fetch_to_register(
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

    fn lower_array_to_register(
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
            let by_ref_local = if by_ref {
                match self.variable_local(builder, site.function, value) {
                    Some(local) => Some(local),
                    None => {
                        self.unsupported(
                            UnsupportedFeature::ArrayElementReference,
                            self.span_for(SourceMappedId::from(element)),
                            "array literal by-reference elements require a simple local variable",
                        );
                        continue;
                    }
                }
            } else {
                None
            };
            let value = self.lower_expr_to_register(builder, site.function, current, value)?;
            current = value.block;
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

    fn lower_call_args(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        args: &[HirCallArg],
    ) -> Option<(Vec<IrCallArg>, BlockId)> {
        self.lower_call_args_with_value_policy(builder, site, args, |_, _| false)
    }

    fn lower_call_args_for_function(
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

    fn lower_call_args_with_value_policy(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        args: &[HirCallArg],
        mut use_null_placeholder: impl FnMut(usize, &HirCallArg) -> bool,
    ) -> Option<(Vec<IrCallArg>, BlockId)> {
        let mut current = site.block;
        let mut operands = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            let by_ref_local = (!arg.unpack)
                .then(|| self.variable_local(builder, site.function, arg.value))
                .flatten();
            let dim_target = (!arg.unpack)
                .then(|| self.dim_assignment_target(builder, site.function, arg.value))
                .flatten()
                .filter(|target| !target.append && !target.dims.is_empty());
            let property_target = (!arg.unpack)
                .then(|| self.property_assignment_target(arg.value))
                .flatten();
            let (value, by_ref_dim, by_ref_property) = if by_ref_local.is_some()
                && use_null_placeholder(index, arg)
            {
                (
                    Operand::Constant(builder.intern_constant(IrConstant::Null)),
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
                    Operand::Register(last.expect("dimension target has at least one dimension")),
                    Some(IrCallDimTarget {
                        local: target.local,
                        dims,
                    }),
                    None,
                )
            } else if let Some(target) = property_target {
                let object =
                    self.lower_expr_to_register(builder, site.function, current, target.receiver)?;
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
                    Some(IrCallPropertyTarget {
                        object: Operand::Register(object.register),
                        property: target.property,
                    }),
                )
            } else {
                let value =
                    self.lower_expr_to_register(builder, site.function, current, arg.value)?;
                current = value.block;
                (Operand::Register(value.register), None, None)
            };
            operands.push(IrCallArg {
                name: arg.name.clone(),
                value,
                unpack: arg.unpack,
                value_kind: self.call_arg_value_kind(arg.value),
                by_ref_local,
                by_ref_dim,
                by_ref_property,
            });
        }
        Some((operands, current))
    }

    fn call_arg_value_kind(&self, expr: ExprId) -> IrCallArgValueKind {
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

    fn lower_call_to_register(
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
            let target = self.static_method_call_target(callee)?;
            return self.lower_static_method_call_to_register(builder, site, target, args);
        }
        let dst = builder.alloc_register(site.function);
        let (kind, current) =
            if let Some(name) = callee.and_then(|callee| self.static_function_call_name(callee)) {
                let normalized_name = normalize_function_name(&name);
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

    fn lower_method_call_to_register(
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

    fn lower_dynamic_method_call_to_register(
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

    fn lower_static_method_call_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: StaticMethodCallTarget,
        args: Vec<HirCallArg>,
    ) -> Option<LoweredExpr> {
        let (operands, current) = self.lower_call_args(builder, site, &args)?;
        let dst = builder.alloc_register(site.function);
        let instruction = builder.emit(
            site.function,
            current,
            InstructionKind::CallStaticMethod {
                dst,
                class_name: normalize_class_name(&target.class_name),
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

    fn lower_dynamic_static_method_call_to_register(
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
        let class_name = self.static_class_name((*target)?)?;
        let method = (*member)?;
        let method_value =
            self.lower_expr_to_register(builder, site.function, site.block, method)?;
        let class_const = builder.intern_constant(IrConstant::String(class_name));
        self.lower_callable_pair_call_to_register(
            builder,
            LowerSite {
                block: method_value.block,
                ..site
            },
            Operand::Constant(class_const),
            Operand::Register(method_value.register),
            args,
        )
    }

    fn lower_callable_pair_call_to_register(
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

    fn lower_clone_object_to_register(
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

    fn lower_clone_with_to_register(
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

    fn lower_builtin_call_to_register(
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

    fn lower_isset_empty_to_register(
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

    fn lower_multi_isset_to_register(
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

    fn lower_isset_empty_operand_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        name: &str,
        arg: ExprId,
    ) -> Option<LoweredExpr> {
        let dst = builder.alloc_register(site.function);
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
            let property =
                self.lower_expr_to_register(builder, site.function, object.block, target.property)?;
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
            let property =
                self.lower_expr_to_register(builder, site.function, object.block, target.property)?;
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

    fn lower_pipe_to_register(
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

    fn lower_include_to_register(
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

    fn lower_eval_to_register(
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

    fn lower_pipe_callable_to_register(
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

    fn lower_callable_expr_to_register(
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

    fn lower_acquire_callable_value(
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
    fn lower_method_first_class_callable(
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

    fn first_class_callable_runtime_value(&self, expr: ExprId) -> bool {
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

    fn callable_static_target_component(&self, expr: ExprId) -> Option<CallableComponent> {
        if let Some(class_name) = self.static_class_name(expr) {
            return Some(CallableComponent::String(class_name));
        }
        Some(CallableComponent::Expr(expr))
    }

    fn callable_member_component(&self, expr: ExprId) -> Option<CallableComponent> {
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

    fn lower_callable_component_to_register(
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

    fn emit_callable_array_insert(
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

    fn lower_closure_to_register(
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

    fn lower_signatureless_closure_to_register(
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

    fn lower_signatureless_closure_function(
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

    fn lower_closure_function(
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

    fn lower_literal_to_register(
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

    fn lower_interpolated_literal_to_register(
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
                InterpolatedPart::Property { receiver, property } => {
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
                    register
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

    fn emit_constant_to_register(
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

    fn magic_constant(&self, text: &str, site: LowerSite) -> Option<IrConstant> {
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
            "__NAMESPACE__" => Some(IrConstant::String(String::new())),
            "__METHOD__" => Some(IrConstant::String(
                self.function_names
                    .get(&site.function)
                    .cloned()
                    .unwrap_or_default(),
            )),
            _ => None,
        }
    }

    fn lower_short_circuit_to_register(
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

    fn lower_ternary_to_register(
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

    fn lower_match_to_register(
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

    fn lower_coalesce_left_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        left: ExprId,
    ) -> Option<LoweredExpr> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(left)?;
        match expression.kind() {
            HirExprKind::Variable { name } => {
                let local = builder.intern_local(site.function, local_name(name));
                let dst = builder.alloc_register(site.function);
                let range = self.span_for(SourceMappedId::from(left));
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
                    left,
                    span,
                );
                Some(LoweredExpr {
                    register: dst,
                    block: site.block,
                })
            }
            _ => self.lower_expr_to_register(builder, site.function, site.block, left),
        }
    }

    fn lower_error_suppression_to_register(
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

    fn emit_bool_move(
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

    fn emit_bool_cast(
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

    fn lower_cast_to_register(
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

    fn lower_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        operator: &str,
        left: Option<ExprId>,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
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

    fn lower_property_assign_to_register(
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

    fn lower_property_compound_assign_to_register(
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

    fn lower_property_dim_assign_to_register(
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

    fn lower_dynamic_property_assign_to_register(
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
        let property =
            self.lower_expr_to_register(builder, site.function, object.block, target.property)?;
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

    fn lower_dynamic_property_dim_assign_to_register(
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
        let property =
            self.lower_expr_to_register(builder, site.function, object.block, target.property)?;
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

    fn lower_static_property_assign_to_register(
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

    fn lower_static_property_compound_assign_to_register(
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

    fn lower_static_property_dim_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: StaticPropertyDimTarget,
        right: Option<ExprId>,
    ) -> Option<LoweredExpr> {
        if target.dims.len() > 1 || (target.append && !target.dims.is_empty()) {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "only top-level static property array dimension assignment is lowered to IR",
            );
            return None;
        }
        let Some(right) = right else {
            self.unsupported(
                UnsupportedFeature::HirStatement,
                site.range,
                "static property array assignment is missing its right operand",
            );
            return None;
        };
        let property = StaticPropertyTarget {
            class_name: target.class_name,
            property: target.property,
        };
        let array =
            self.lower_static_property_fetch_to_register(builder, site, property.clone())?;
        let mut current = array.block;
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
                array: array.register,
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
        self.emit_static_property_assign_from_register(
            builder,
            site,
            value.block,
            &property,
            array.register,
        )?;
        Some(LoweredExpr {
            register: value.register,
            block: value.block,
        })
    }

    fn emit_static_property_assign_from_register(
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

    fn lower_reference_assign_to_register(
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
        if let Some(left) = left
            && let Some(target) = self.dim_assignment_target(builder, site.function, left)
            && let Some(source) = right.and_then(|right| self.property_assignment_target(right))
        {
            return self
                .lower_dim_reference_from_property_to_register(builder, site, target, source);
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
            (Some(_), Some(_)) => {
                self.unsupported(
                    UnsupportedFeature::ArrayElementReference,
                    site.range,
                    "array-dimension to array-dimension reference binding is not implemented yet",
                );
                return None;
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

    fn lower_property_reference_assign_to_register(
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

    fn lower_property_dim_reference_assign_to_register(
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

    fn lower_dim_reference_from_property_to_register(
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

    fn contains_dim_fetch_expr(&self, expr: ExprId) -> bool {
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

    fn contains_property_fetch_expr(&self, expr: ExprId) -> bool {
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

    fn instanceof_class_name(&self, expr: ExprId) -> Option<String> {
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

    fn expr_contains(
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

    fn lower_destructuring_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        targets: Vec<(i64, LocalId)>,
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

    fn lower_dim_compound_assign_to_register(
        &mut self,
        builder: &mut IrBuilder,
        site: LowerSite,
        target: DimAssignmentTarget,
        operator: &str,
        left: ExprId,
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
        let old = self.lower_expr_to_register(builder, site.function, site.block, left)?;
        let rhs = self.lower_expr_to_register(builder, site.function, old.block, right)?;
        let value = builder.alloc_register(site.function);
        let instruction = builder.emit(
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
            instruction,
            site.expr,
            site.span,
        );
        self.lower_dim_assign_value_to_register(
            builder,
            site,
            rhs.block,
            target,
            Operand::Register(value),
            value,
        )
    }

    fn lower_dim_assign_to_register(
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
        self.lower_dim_assign_value_to_register(
            builder,
            site,
            value.block,
            target,
            Operand::Register(value.register),
            value.register,
        )
    }

    fn lower_dim_assign_value_to_register(
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

    fn lower_inc_dec_to_register(
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

    fn variable_local(
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

    fn static_function_call_name(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Name { resolution } => resolution
                .resolved()
                .or_else(|| resolution.fallback())
                .or_else(|| Some(resolution.source()))
                .map(ToOwned::to_owned),
            _ => None,
        }
    }

    fn direct_function_call_parts(&self, expr: ExprId) -> Option<(String, Vec<HirCallArg>)> {
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

    fn static_class_name(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Name { resolution } => resolution
                .resolved()
                .or_else(|| resolution.fallback())
                .or_else(|| Some(resolution.source()))
                .map(ToOwned::to_owned),
            _ => None,
        }
    }

    fn static_class_display_name(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Name { resolution } => self
                .declared_class_display_name(
                    resolution
                        .resolved()
                        .or_else(|| resolution.fallback())
                        .unwrap_or_else(|| resolution.source()),
                )
                .or_else(|| Some(display_class_name(resolution.source()))),
            _ => None,
        }
    }

    fn static_class_source_name(&self, expr: ExprId) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let expression = module.expressions().get(expr)?;
        match expression.kind() {
            HirExprKind::Name { resolution } => Some(resolution.source().to_owned()),
            _ => None,
        }
    }

    fn declared_class_display_name(&self, class_name: &str) -> Option<String> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        let normalized = normalize_class_name(class_name);
        module.class_likes().iter().find_map(|(_, class_like)| {
            (class_like_normalized_name(class_like)? == normalized)
                .then(|| class_like_display_name(class_like, class_name))
        })
    }

    fn static_property_name(&self, expr: ExprId) -> Option<String> {
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

    fn static_property_member_name(&self, expr: ExprId) -> Option<String> {
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

    fn static_property_display_name(&self, expr: ExprId) -> Option<String> {
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

    fn static_property_target(&self, expr: ExprId) -> Option<StaticPropertyTarget> {
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

    fn static_property_dim_target(&self, expr: ExprId) -> Option<StaticPropertyDimTarget> {
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

    fn static_property_test_target(&self, expr: ExprId) -> Option<StaticPropertyTarget> {
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

    fn class_constant_target(&self, expr: ExprId) -> Option<ClassConstantTarget> {
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
        })
    }

    fn object_class_name_target(&self, expr: ExprId) -> Option<ObjectClassNameTarget> {
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

    fn static_member_is_property(&self, expr: ExprId) -> bool {
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

    fn method_call_target(
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

    fn dynamic_method_call_target(
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

    fn static_method_call_target(&mut self, expr: ExprId) -> Option<StaticMethodCallTarget> {
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
        Some(StaticMethodCallTarget {
            class_name,
            method: self.static_property_name(member)?,
        })
    }

    fn is_static_access_expr(&self, expr: ExprId) -> bool {
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

    fn parenthesized_clone_operand(
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

    fn clone_with_operands(
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

    fn property_assignment_target(&self, expr: ExprId) -> Option<PropertyAssignmentTarget> {
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

    fn dynamic_property_target(&self, expr: ExprId) -> Option<DynamicPropertyTarget> {
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

    fn dynamic_property_dim_target(&self, expr: ExprId) -> Option<DynamicPropertyDimTarget> {
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

    fn property_fetch_uses_dynamic_member(&self, expr: ExprId) -> bool {
        let range = self.span_for(SourceMappedId::from(expr));
        self.source_text
            .slice(range)
            .is_some_and(|source| source.contains("->$") || source.contains("->{"))
    }

    fn method_call_uses_dynamic_member(&self, expr: ExprId) -> bool {
        let range = self.span_for(SourceMappedId::from(expr));
        self.source_text.slice(range).is_some_and(|source| {
            source.contains("->$")
                || source.contains("->{")
                || source.contains("?->$")
                || source.contains("?->{")
        })
    }

    fn static_access_uses_dynamic_member(&self, expr: ExprId) -> bool {
        let range = self.span_for(SourceMappedId::from(expr));
        self.source_text
            .slice(range)
            .is_some_and(|source| source.contains("::$"))
    }

    fn property_dim_target(&self, expr: ExprId) -> Option<PropertyDimTarget> {
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

    fn statement_id_for_span(&self, span: TextRange) -> Option<StmtId> {
        let module = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())?;
        module.statements().iter().find_map(|(stmt_id, _)| {
            (self.span_for(SourceMappedId::from(stmt_id)) == span).then_some(stmt_id)
        })
    }

    fn statement_ids_inside(&self, span: TextRange) -> Vec<StmtId> {
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

    fn method_body_statement_ids(&self, signature: &FunctionSignature) -> Vec<StmtId> {
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

    fn expr_id_for_span(&self, span: TextRange) -> Option<ExprId> {
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

    fn outermost_expr_inside(&self, span: TextRange) -> Option<ExprId> {
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

    fn property_hooks_use_backing_storage(&self, property: &HirProperty) -> bool {
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

    fn signature_for_expr(
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

    fn function_like_uses_variable(&self, span: TextRange, variable_name: &str) -> bool {
        let Some(module) = self
            .frontend
            .database()
            .module(self.frontend.module().module_id())
        else {
            return false;
        };
        module.expressions().iter().any(|(expr_id, expr)| {
            let expr_span = self.span_for(SourceMappedId::from(expr_id));
            range_contains(span, expr_span)
                && matches!(expr.kind(), HirExprKind::Variable { name } if name == variable_name)
        })
    }

    fn explicit_capture_specs(&self, span: TextRange) -> Vec<CaptureSpec> {
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

    fn implicit_arrow_capture_specs(
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

    fn static_local_specs(&self, stmt_id: StmtId, initializers: &[ExprId]) -> Vec<StaticLocalSpec> {
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

    fn loop_control_level(&mut self, expr: Option<ExprId>) -> Option<usize> {
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

    fn add_expr_source_map(
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

    fn unsupported(
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

    fn span_for(&self, id: SourceMappedId) -> TextRange {
        self.frontend
            .database()
            .source_map()
            .span(id)
            .unwrap_or_else(|| TextRange::new(0, self.frontend.module().source_bytes()))
    }

    fn is_reflection_function_name(&self, expr: Option<php_semantics::hir::ExprId>) -> bool {
        self.static_source_or_resolved_name(expr)
            .is_some_and(|name| name.to_ascii_lowercase().starts_with("reflection"))
    }

    fn static_source_or_resolved_name(
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

fn unary_op(operator: &str) -> Option<UnaryOp> {
    match operator {
        "+" => Some(UnaryOp::Plus),
        "-" => Some(UnaryOp::Minus),
        "!" => Some(UnaryOp::Not),
        "~" => Some(UnaryOp::BitNot),
        _ => None,
    }
}

fn binary_op(operator: &str) -> Option<BinaryOp> {
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

fn assignment_binary_op(operator: &str) -> Option<BinaryOp> {
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

fn compare_op(operator: &str) -> Option<CompareOp> {
    match operator {
        "==" => Some(CompareOp::Equal),
        "!=" | "<>" => Some(CompareOp::NotEqual),
        "===" => Some(CompareOp::Identical),
        "!==" => Some(CompareOp::NotIdentical),
        "<" => Some(CompareOp::Less),
        "<=" => Some(CompareOp::LessEqual),
        ">" => Some(CompareOp::Greater),
        ">=" => Some(CompareOp::GreaterEqual),
        "<=>" => Some(CompareOp::Spaceship),
        _ => None,
    }
}

fn cast_kind(kind: &str) -> Option<CastKind> {
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

fn include_kind(kind: &str) -> Option<IncludeKind> {
    match kind.to_ascii_lowercase().as_str() {
        "include" => Some(IncludeKind::Include),
        "include_once" => Some(IncludeKind::IncludeOnce),
        "require" => Some(IncludeKind::Require),
        "require_once" => Some(IncludeKind::RequireOnce),
        _ => None,
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

fn trait_alias_matches(alias: &TraitAliasSpec, candidate: &TraitMethodCandidate) -> bool {
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

fn function_declaration_kind(
    module: &HirModule,
    signature: &FunctionSignature,
    normalized_name: &str,
) -> Option<DeclarationKind> {
    module
        .declaration_table()
        .entries()
        .iter()
        .find(|entry| {
            matches!(
                entry.kind(),
                DeclarationKind::Function | DeclarationKind::ConditionalFunction
            ) && entry.fqn().canonical(NameKind::Function) == normalized_name
                && range_contains(signature.span(), entry.span())
        })
        .map(|entry| entry.kind())
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
) -> ClassConstantInitializerMap {
    class_likes
        .iter()
        .filter_map(|(_, class_like)| {
            let class_name = class_like_normalized_name(class_like)?;
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

fn collect_class_parents(class_likes: &[(ClassLikeId, HirClassLike)]) -> ClassParentMap {
    class_likes
        .iter()
        .filter_map(|(_, class_like)| {
            let class_name = class_like_normalized_name(class_like)?;
            let parent = (class_like.kind() == ClassLikeKind::Class)
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

fn class_like_normalized_name(class_like: &HirClassLike) -> Option<String> {
    class_like
        .fqn()
        .map(|name| name.canonical(NameKind::ClassLike))
        .or_else(|| class_like.name().map(normalize_class_name))
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
        .unwrap_or_else(|| display_class_name(fallback))
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
    class_constants: &ClassConstantInitializerMap,
    class_parents: &ClassParentMap,
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
    class_constants: &ClassConstantInitializerMap,
    class_parents: &ClassParentMap,
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
            constant_from_expr_with_class_constants(
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
    class_constants: &ClassConstantInitializerMap,
    class_parents: &ClassParentMap,
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
        _ => None,
    }
}

fn constant_from_expr_with_runtime_constants(
    module: &HirModule,
    expr_id: ExprId,
    named_constants: &HashMap<String, IrConstant>,
    current_class: Option<&str>,
    class_constants: &ClassConstantInitializerMap,
    class_parents: &ClassParentMap,
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
        _ => None,
    }
}

fn class_constant_initializer_target_class(
    module: &HirModule,
    expr_id: ExprId,
    current_class: Option<&str>,
    class_parents: &ClassParentMap,
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
    class_parents: &ClassParentMap,
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
    Some(source.trim_start_matches('\\').to_owned())
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
    class_constants: &ClassConstantInitializerMap,
    class_parents: &ClassParentMap,
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

fn negate_ir_constant(value: IrConstant) -> Option<IrConstant> {
    match value {
        IrConstant::Int(value) => value.checked_neg().map(IrConstant::Int),
        IrConstant::Float(value) => Some(IrConstant::Float(-value)),
        _ => None,
    }
}

fn binary_ir_constant(operator: &str, left: IrConstant, right: IrConstant) -> Option<IrConstant> {
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
    fn static_property_append_lowers_through_fetch_insert_and_assign() {
        let frontend = analyze_source("<?php class C { static public $p = array(); } C::$p[] = 1;");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        let snapshot = result.unit.to_snapshot_text();
        assert!(snapshot.contains("fetch_static_property r"), "{snapshot}");
        assert!(snapshot.contains("array_insert r"), "{snapshot}");
        assert!(snapshot.contains("assign_static_property r"), "{snapshot}");
        assert!(snapshot.contains("c::$p"), "{snapshot}");
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
        assert_eq!(result.unit.function_table.len(), 1);
        assert_eq!(result.unit.function_table[0].name, "branch_dup");
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
        assert_eq!(snapshot.matches("function_name \"branch_dup\"").count(), 1);
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
    fn lower_yield_from_to_ir_instruction() {
        let frontend = analyze_source("<?php function gen($items) { yield from $items; }");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(result.unit.to_snapshot_text().contains("yield_from r"));
    }

    #[test]
    fn lower_eval_to_ir_instruction() {
        let frontend = analyze_source("<?php eval('echo 1;');");
        let result = lower_frontend_result(&frontend, LoweringOptions::default());

        assert!(result.verification.is_ok(), "{:#?}", result.verification);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(result.unit.to_snapshot_text().contains("eval r"));
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
