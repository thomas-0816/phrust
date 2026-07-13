//! Stable semantic diagnostic identifiers.

/// Stable semantic diagnostic IDs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticId {
    /// Reserved marker for future semantic diagnostics.
    Reserved,
    /// Duplicate function or method parameter.
    DuplicateParameter,
    /// Variadic parameter is not the final parameter.
    VariadicParameterNotLast,
    /// Parameter default value is not allowed in this position.
    InvalidParameterDefault,
    /// `$this` is used as a parameter name.
    ThisParameter,
    /// `$this` is used as an assignment target.
    ThisReassignment,
    /// Constructor property promotion appears in an invalid context.
    InvalidPropertyPromotion,
    /// Closure use captures duplicate a parameter name.
    ClosureUseDuplicatesParameter,
    /// Closure use captures an auto-global variable.
    ClosureUseAutoGlobal,
    /// Closure use captures the same variable more than once.
    DuplicateClosureUseVariable,
    /// HIR lowering expected a child node that recovery syntax omitted.
    HirMissingChild,
    /// Duplicate import alias in one import scope.
    DuplicateUseAlias,
    /// Duplicate declaration in one source file.
    DuplicateDeclaration,
    /// Mixed braced and unbraced namespace declarations.
    MixedNamespaceDeclarations,
    /// Namespace declaration appears after an invalid statement.
    NamespaceMustBeFirstStatement,
    /// `void` type used in a disallowed context.
    InvalidTypeVoidContext,
    /// `never` type used in a disallowed context.
    InvalidTypeNeverContext,
    /// `static` type used in a disallowed context.
    InvalidTypeStaticContext,
    /// `self` type used in a disallowed context.
    InvalidTypeSelfContext,
    /// `parent` type used in a disallowed context.
    InvalidTypeParentContext,
    /// `callable` type used in a disallowed context.
    InvalidTypeCallableContext,
    /// Non-class type used as an intersection member.
    InvalidIntersectionMember,
    /// Duplicate type alternative in a union or intersection.
    DuplicateTypeAlternative,
    /// Duplicate declaration modifier.
    DuplicateModifier,
    /// Incompatible declaration modifiers.
    IncompatibleModifiers,
    /// `break` outside loop or switch context.
    BreakNotInLoopOrSwitch,
    /// `continue` outside loop or switch context.
    ContinueNotInLoopOrSwitch,
    /// `break`/`continue` level exceeds available loop or switch contexts.
    InvalidBreakContinueLevel,
    /// `return` appears in a context where PHP does not allow it.
    ReturnOutsideAllowedContext,
    /// Void function returns a value.
    ReturnValueFromVoidFunction,
    /// Never-returning function contains a return statement.
    ReturnFromNeverFunction,
    /// `yield` outside function context.
    YieldOutsideFunction,
    /// `goto` target label was not found in the same control unit.
    GotoLabelNotFound,
    /// Invalid constant expression.
    InvalidConstExpr,
    /// Class constant appears in a write/reference target position.
    InvalidClassConstantWrite,
    /// Attribute argument is not a constant expression.
    AttributeArgumentNotConstExpr,
    /// Duplicate class member.
    DuplicateClassMember,
    /// Unit enum case has a value.
    EnumCaseValueOnUnitEnum,
    /// Backed enum case is missing a value.
    EnumCaseMissingValueOnBackedEnum,
    /// Trait adaptation syntax has an invalid semantic shape.
    TraitAdaptationInvalidShape,
    /// Class context keyword used outside a valid class-like context.
    InvalidClassContextName,
    /// Magic method declaration violates a reference-confirmed compile-time rule.
    InvalidMagicMethodSignature,
    /// `declare(strict_types=...)` has an invalid value.
    InvalidStrictTypesDeclare,
    /// `declare(strict_types=...)` is not the first statement in the file.
    StrictTypesDeclareNotFirst,
    /// Reference behavior is intentionally deferred.
    ReferenceBehaviorDeferred,
    /// Runtime-only check is intentionally deferred.
    RuntimeCheckDeferred,
    /// `(void)` cast is rejected by the pinned PHP reference.
    InvalidVoidCast,
}

impl DiagnosticId {
    /// Returns the stable string code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reserved => "PHS0000",
            Self::DuplicateParameter => "E_PHP_DUPLICATE_PARAMETER",
            Self::VariadicParameterNotLast => "E_PHP_VARIADIC_PARAMETER_NOT_LAST",
            Self::InvalidParameterDefault => "E_PHP_INVALID_PARAMETER_DEFAULT",
            Self::ThisParameter => "E_PHP_THIS_PARAMETER",
            Self::ThisReassignment => "E_PHP_THIS_REASSIGNMENT",
            Self::InvalidPropertyPromotion => "E_PHP_INVALID_PROPERTY_PROMOTION",
            Self::ClosureUseDuplicatesParameter => "E_PHP_CLOSURE_USE_DUPLICATES_PARAMETER",
            Self::ClosureUseAutoGlobal => "E_PHP_CLOSURE_USE_AUTO_GLOBAL",
            Self::DuplicateClosureUseVariable => "E_PHP_DUPLICATE_CLOSURE_USE_VARIABLE",
            Self::HirMissingChild => "E_PHP_HIR_MISSING_CHILD",
            Self::DuplicateUseAlias => "E_PHP_DUPLICATE_USE_ALIAS",
            Self::DuplicateDeclaration => "E_PHP_DUPLICATE_DECLARATION",
            Self::MixedNamespaceDeclarations => "E_PHP_MIXED_NAMESPACE_DECLARATIONS",
            Self::NamespaceMustBeFirstStatement => "E_PHP_NAMESPACE_MUST_BE_FIRST_STATEMENT",
            Self::InvalidTypeVoidContext => "E_PHP_INVALID_TYPE_VOID_CONTEXT",
            Self::InvalidTypeNeverContext => "E_PHP_INVALID_TYPE_NEVER_CONTEXT",
            Self::InvalidTypeStaticContext => "E_PHP_INVALID_TYPE_STATIC_CONTEXT",
            Self::InvalidTypeSelfContext => "E_PHP_INVALID_TYPE_SELF_CONTEXT",
            Self::InvalidTypeParentContext => "E_PHP_INVALID_TYPE_PARENT_CONTEXT",
            Self::InvalidTypeCallableContext => "E_PHP_INVALID_TYPE_CALLABLE_CONTEXT",
            Self::InvalidIntersectionMember => "E_PHP_INVALID_INTERSECTION_MEMBER",
            Self::DuplicateTypeAlternative => "E_PHP_DUPLICATE_TYPE_ALTERNATIVE",
            Self::DuplicateModifier => "E_PHP_DUPLICATE_MODIFIER",
            Self::IncompatibleModifiers => "E_PHP_INCOMPATIBLE_MODIFIERS",
            Self::BreakNotInLoopOrSwitch => "E_PHP_BREAK_NOT_IN_LOOP_OR_SWITCH",
            Self::ContinueNotInLoopOrSwitch => "E_PHP_CONTINUE_NOT_IN_LOOP_OR_SWITCH",
            Self::InvalidBreakContinueLevel => "E_PHP_INVALID_BREAK_CONTINUE_LEVEL",
            Self::ReturnOutsideAllowedContext => "E_PHP_RETURN_OUTSIDE_ALLOWED_CONTEXT",
            Self::ReturnValueFromVoidFunction => "E_PHP_RETURN_VALUE_FROM_VOID_FUNCTION",
            Self::ReturnFromNeverFunction => "E_PHP_RETURN_FROM_NEVER_FUNCTION",
            Self::YieldOutsideFunction => "E_PHP_YIELD_OUTSIDE_FUNCTION",
            Self::GotoLabelNotFound => "E_PHP_GOTO_LABEL_NOT_FOUND",
            Self::InvalidConstExpr => "E_PHP_INVALID_CONST_EXPR",
            Self::InvalidClassConstantWrite => "E_PHP_INVALID_CLASS_CONSTANT_WRITE",
            Self::AttributeArgumentNotConstExpr => "E_PHP_ATTRIBUTE_ARGUMENT_NOT_CONST_EXPR",
            Self::DuplicateClassMember => "E_PHP_DUPLICATE_CLASS_MEMBER",
            Self::EnumCaseValueOnUnitEnum => "E_PHP_ENUM_CASE_VALUE_ON_UNIT_ENUM",
            Self::EnumCaseMissingValueOnBackedEnum => {
                "E_PHP_ENUM_CASE_MISSING_VALUE_ON_BACKED_ENUM"
            }
            Self::TraitAdaptationInvalidShape => "E_PHP_TRAIT_ADAPTATION_INVALID_SHAPE",
            Self::InvalidClassContextName => "E_PHP_INVALID_CLASS_CONTEXT_NAME",
            Self::InvalidMagicMethodSignature => "E_PHP_INVALID_MAGIC_METHOD_SIGNATURE",
            Self::InvalidStrictTypesDeclare => "E_PHP_INVALID_STRICT_TYPES_DECLARE",
            Self::StrictTypesDeclareNotFirst => "E_PHP_STRICT_TYPES_DECLARE_NOT_FIRST",
            Self::ReferenceBehaviorDeferred => "W_PHP_REFERENCE_BEHAVIOR_DEFERRED",
            Self::RuntimeCheckDeferred => "N_PHP_RUNTIME_CHECK_DEFERRED",
            Self::InvalidVoidCast => "E_PHP_INVALID_VOID_CAST",
        }
    }
}
