//! Class-like HIR records.

use crate::hir::{
    AttributeId, ClassLikeId, ConstExprId, ConstId, EnumCaseId, FullyQualifiedName,
    HirNameResolution, MethodId, ModifierSet, PromotedPropertyInfo, PropertyId, TraitUseId, TypeId,
};

/// Class-like declaration families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassLikeKind {
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

impl ClassLikeKind {
    /// Returns stable JSON text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Trait => "trait",
            Self::Enum => "enum",
            Self::AnonymousClass => "anonymous_class",
        }
    }
}

/// Member summary attached to a class-like declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassLikeMember {
    kind: ClassLikeMemberKind,
    name: Option<String>,
    id: Option<ClassLikeMemberId>,
}

impl ClassLikeMember {
    /// Creates a member summary.
    #[must_use]
    pub fn new(kind: ClassLikeMemberKind, name: Option<String>) -> Self {
        Self {
            kind,
            name,
            id: None,
        }
    }

    /// Creates a member summary linked to a typed member HIR record.
    #[must_use]
    pub fn with_id(kind: ClassLikeMemberKind, name: Option<String>, id: ClassLikeMemberId) -> Self {
        Self {
            kind,
            name,
            id: Some(id),
        }
    }

    /// Returns the member family.
    #[must_use]
    pub const fn kind(&self) -> ClassLikeMemberKind {
        self.kind
    }

    /// Returns the member name, when a stable name is visible.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the linked member ID, when this summary has a lowered member record.
    #[must_use]
    pub const fn id(&self) -> Option<ClassLikeMemberId> {
        self.id
    }
}

/// Typed ID union for member summaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassLikeMemberId {
    /// Method HIR ID.
    Method(MethodId),
    /// Property declaration HIR ID.
    Property(PropertyId),
    /// Class constant declaration HIR ID.
    ClassConstant(ConstId),
    /// Trait-use declaration HIR ID.
    TraitUse(TraitUseId),
    /// Enum case HIR ID.
    EnumCase(EnumCaseId),
}

/// Class-like member families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassLikeMemberKind {
    /// Method declaration.
    Method,
    /// Property declaration.
    Property,
    /// Class constant declaration.
    ClassConstant,
    /// Trait use declaration.
    TraitUse,
    /// Enum case declaration.
    EnumCase,
}

impl ClassLikeMemberKind {
    /// Returns stable JSON text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Method => "method",
            Self::Property => "property",
            Self::ClassConstant => "class_constant",
            Self::TraitUse => "trait_use",
            Self::EnumCase => "enum_case",
        }
    }
}

/// Recognized PHP magic method families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MagicMethodKind {
    /// `__construct`.
    Construct,
    /// `__destruct`.
    Destruct,
    /// `__call`.
    Call,
    /// `__callStatic`.
    CallStatic,
    /// `__get`.
    Get,
    /// `__set`.
    Set,
    /// `__isset`.
    Isset,
    /// `__unset`.
    Unset,
    /// `__sleep`.
    Sleep,
    /// `__wakeup`.
    Wakeup,
    /// `__serialize`.
    Serialize,
    /// `__unserialize`.
    Unserialize,
    /// `__toString`.
    ToString,
    /// `__invoke`.
    Invoke,
    /// `__set_state`.
    SetState,
    /// `__clone`.
    Clone,
    /// `__debugInfo`.
    DebugInfo,
}

impl MagicMethodKind {
    /// Classifies a method name as a PHP magic method.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "__construct" => Some(Self::Construct),
            "__destruct" => Some(Self::Destruct),
            "__call" => Some(Self::Call),
            "__callstatic" => Some(Self::CallStatic),
            "__get" => Some(Self::Get),
            "__set" => Some(Self::Set),
            "__isset" => Some(Self::Isset),
            "__unset" => Some(Self::Unset),
            "__sleep" => Some(Self::Sleep),
            "__wakeup" => Some(Self::Wakeup),
            "__serialize" => Some(Self::Serialize),
            "__unserialize" => Some(Self::Unserialize),
            "__tostring" => Some(Self::ToString),
            "__invoke" => Some(Self::Invoke),
            "__set_state" => Some(Self::SetState),
            "__clone" => Some(Self::Clone),
            "__debuginfo" => Some(Self::DebugInfo),
            _ => None,
        }
    }

    /// Returns stable JSON text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Construct => "__construct",
            Self::Destruct => "__destruct",
            Self::Call => "__call",
            Self::CallStatic => "__callStatic",
            Self::Get => "__get",
            Self::Set => "__set",
            Self::Isset => "__isset",
            Self::Unset => "__unset",
            Self::Sleep => "__sleep",
            Self::Wakeup => "__wakeup",
            Self::Serialize => "__serialize",
            Self::Unserialize => "__unserialize",
            Self::ToString => "__toString",
            Self::Invoke => "__invoke",
            Self::SetState => "__set_state",
            Self::Clone => "__clone",
            Self::DebugInfo => "__debugInfo",
        }
    }
}

/// Structural HIR for a PHP class, interface, trait, enum, or anonymous class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirClassLike {
    kind: ClassLikeKind,
    name: Option<String>,
    fqn: Option<FullyQualifiedName>,
    anonymous_id: Option<String>,
    modifiers: ModifierSet,
    extends: Vec<HirNameResolution>,
    implements: Vec<HirNameResolution>,
    trait_uses: Vec<HirNameResolution>,
    members: Vec<ClassLikeMember>,
    attributes: Vec<AttributeId>,
    backing_type: Option<TypeId>,
}

/// Lowered enum case record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirEnumCase {
    class_like: ClassLikeId,
    name: Option<String>,
    value: Option<ConstExprId>,
    attributes: Vec<AttributeId>,
}

impl HirEnumCase {
    /// Creates an enum-case record.
    #[must_use]
    pub fn new(
        class_like: ClassLikeId,
        name: Option<String>,
        value: Option<ConstExprId>,
        attributes: Vec<AttributeId>,
    ) -> Self {
        Self {
            class_like,
            name,
            value,
            attributes,
        }
    }

    /// Returns owning enum class-like ID.
    #[must_use]
    pub const fn class_like(&self) -> ClassLikeId {
        self.class_like
    }

    /// Returns enum case name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns backing value candidate for backed enum cases.
    #[must_use]
    pub const fn value(&self) -> Option<ConstExprId> {
        self.value
    }

    /// Returns attached attribute IDs.
    #[must_use]
    pub fn attributes(&self) -> &[AttributeId] {
        &self.attributes
    }
}

/// Lowered trait-use declaration record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirTraitUse {
    class_like: ClassLikeId,
    traits: Vec<HirNameResolution>,
    adaptations: Vec<HirTraitAdaptation>,
}

impl HirTraitUse {
    /// Creates a trait-use record.
    #[must_use]
    pub fn new(
        class_like: ClassLikeId,
        traits: Vec<HirNameResolution>,
        adaptations: Vec<HirTraitAdaptation>,
    ) -> Self {
        Self {
            class_like,
            traits,
            adaptations,
        }
    }

    /// Returns the owning class-like ID.
    #[must_use]
    pub const fn class_like(&self) -> ClassLikeId {
        self.class_like
    }

    /// Returns resolved trait names in the use clause.
    #[must_use]
    pub fn traits(&self) -> &[HirNameResolution] {
        &self.traits
    }

    /// Returns adaptation entries in source order.
    #[must_use]
    pub fn adaptations(&self) -> &[HirTraitAdaptation] {
        &self.adaptations
    }
}

/// Trait-use adaptation entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirTraitAdaptation {
    kind: HirTraitAdaptationKind,
    method: HirTraitMethodRef,
    span: php_source::TextRange,
}

impl HirTraitAdaptation {
    /// Creates a trait adaptation.
    #[must_use]
    pub const fn new(
        kind: HirTraitAdaptationKind,
        method: HirTraitMethodRef,
        span: php_source::TextRange,
    ) -> Self {
        Self { kind, method, span }
    }

    /// Returns adaptation kind.
    #[must_use]
    pub const fn kind(&self) -> &HirTraitAdaptationKind {
        &self.kind
    }

    /// Returns adapted method reference.
    #[must_use]
    pub const fn method(&self) -> &HirTraitMethodRef {
        &self.method
    }

    /// Returns source span.
    #[must_use]
    pub const fn span(&self) -> php_source::TextRange {
        self.span
    }
}

/// Trait-use adaptation kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HirTraitAdaptationKind {
    /// `A::m insteadof B`.
    Precedence { instead_of: Vec<HirNameResolution> },
    /// `A::m as alias` or `A::m as private alias`.
    Alias {
        alias: Option<String>,
        visibility: Option<String>,
    },
}

impl HirTraitAdaptationKind {
    /// Returns stable JSON text.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Precedence { .. } => "precedence",
            Self::Alias { .. } => "alias",
        }
    }
}

/// Trait method reference inside an adaptation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirTraitMethodRef {
    trait_name: Option<HirNameResolution>,
    method: String,
}

impl HirTraitMethodRef {
    /// Creates a method reference.
    #[must_use]
    pub fn new(trait_name: Option<HirNameResolution>, method: impl Into<String>) -> Self {
        Self {
            trait_name,
            method: method.into(),
        }
    }

    /// Returns the optional trait qualifier.
    #[must_use]
    pub const fn trait_name(&self) -> Option<&HirNameResolution> {
        self.trait_name.as_ref()
    }

    /// Returns the method name.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }
}

/// Lowered class method record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirMethod {
    class_like: ClassLikeId,
    name: Option<String>,
    magic_kind: Option<MagicMethodKind>,
    modifiers: ModifierSet,
    has_body: bool,
    attributes: Vec<AttributeId>,
    signature_index: Option<usize>,
}

impl HirMethod {
    /// Creates a method record.
    #[must_use]
    pub fn new(
        class_like: ClassLikeId,
        name: Option<String>,
        magic_kind: Option<MagicMethodKind>,
        modifiers: ModifierSet,
        has_body: bool,
        attributes: Vec<AttributeId>,
        signature_index: Option<usize>,
    ) -> Self {
        Self {
            class_like,
            name,
            magic_kind,
            modifiers,
            has_body,
            attributes,
            signature_index,
        }
    }

    /// Returns the owning class-like ID.
    #[must_use]
    pub const fn class_like(&self) -> ClassLikeId {
        self.class_like
    }

    /// Returns the method name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the recognized magic method kind, if any.
    #[must_use]
    pub const fn magic_kind(&self) -> Option<MagicMethodKind> {
        self.magic_kind
    }

    /// Returns method modifiers.
    #[must_use]
    pub const fn modifiers(&self) -> &ModifierSet {
        &self.modifiers
    }

    /// Returns true when a body block is present.
    #[must_use]
    pub const fn has_body(&self) -> bool {
        self.has_body
    }

    /// Returns attached attribute IDs.
    #[must_use]
    pub fn attributes(&self) -> &[AttributeId] {
        &self.attributes
    }

    /// Returns the matching function-signature vector index.
    #[must_use]
    pub const fn signature_index(&self) -> Option<usize> {
        self.signature_index
    }
}

/// Lowered property declaration record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirProperty {
    class_like: ClassLikeId,
    modifiers: ModifierSet,
    type_id: Option<TypeId>,
    items: Vec<HirPropertyItem>,
    hooks: Vec<HirPropertyHook>,
    attributes: Vec<AttributeId>,
}

impl HirProperty {
    /// Creates a property declaration record.
    #[must_use]
    pub fn new(
        class_like: ClassLikeId,
        modifiers: ModifierSet,
        type_id: Option<TypeId>,
        items: Vec<HirPropertyItem>,
        hooks: Vec<HirPropertyHook>,
        attributes: Vec<AttributeId>,
    ) -> Self {
        Self {
            class_like,
            modifiers,
            type_id,
            items,
            hooks,
            attributes,
        }
    }

    /// Returns the owning class-like ID.
    #[must_use]
    pub const fn class_like(&self) -> ClassLikeId {
        self.class_like
    }

    /// Returns property modifiers.
    #[must_use]
    pub const fn modifiers(&self) -> &ModifierSet {
        &self.modifiers
    }

    /// Returns the property type ID.
    #[must_use]
    pub const fn type_id(&self) -> Option<TypeId> {
        self.type_id
    }

    /// Returns declared property items.
    #[must_use]
    pub fn items(&self) -> &[HirPropertyItem] {
        &self.items
    }

    /// Returns property hook summaries.
    #[must_use]
    pub fn hooks(&self) -> &[HirPropertyHook] {
        &self.hooks
    }

    /// Returns attached attribute IDs.
    #[must_use]
    pub fn attributes(&self) -> &[AttributeId] {
        &self.attributes
    }
}

/// One variable declared by a property declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirPropertyItem {
    name: String,
    default: Option<ConstExprId>,
    promoted: Option<PromotedPropertyInfo>,
}

impl HirPropertyItem {
    /// Creates a property item.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        default: Option<ConstExprId>,
        promoted: Option<PromotedPropertyInfo>,
    ) -> Self {
        Self {
            name: name.into(),
            default,
            promoted,
        }
    }

    /// Returns the variable name, including `$`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the lowered default constant-expression candidate.
    #[must_use]
    pub const fn default(&self) -> Option<ConstExprId> {
        self.default
    }

    /// Returns constructor-promotion metadata.
    #[must_use]
    pub const fn promoted(&self) -> Option<&PromotedPropertyInfo> {
        self.promoted.as_ref()
    }
}

/// Property hook summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirPropertyHook {
    kind: String,
    span: php_source::TextRange,
    body: HirPropertyHookBody,
    uses_backing_storage: bool,
}

impl HirPropertyHook {
    /// Creates a property hook summary.
    #[must_use]
    pub fn new(
        kind: impl Into<String>,
        span: php_source::TextRange,
        body: HirPropertyHookBody,
        uses_backing_storage: bool,
    ) -> Self {
        Self {
            kind: kind.into(),
            span,
            body,
            uses_backing_storage,
        }
    }

    /// Returns the hook kind.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the hook declaration span.
    #[must_use]
    pub const fn span(&self) -> php_source::TextRange {
        self.span
    }

    /// Returns the hook body shape.
    #[must_use]
    pub const fn body(&self) -> HirPropertyHookBody {
        self.body
    }

    /// Returns whether the hook accesses its property's backing storage.
    #[must_use]
    pub const fn uses_backing_storage(&self) -> bool {
        self.uses_backing_storage
    }
}

/// Property hook body shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HirPropertyHookBody {
    /// Hook uses `=> expr;`.
    Expression,
    /// Hook uses `{ statements }`.
    Block,
}

/// Lowered class-constant declaration record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HirClassConst {
    class_like: ClassLikeId,
    name: Option<String>,
    modifiers: ModifierSet,
    type_id: Option<TypeId>,
    value: Option<ConstExprId>,
    attributes: Vec<AttributeId>,
}

impl HirClassConst {
    /// Creates a class-constant declaration record.
    #[must_use]
    pub fn new(
        class_like: ClassLikeId,
        name: Option<String>,
        modifiers: ModifierSet,
        type_id: Option<TypeId>,
        value: Option<ConstExprId>,
        attributes: Vec<AttributeId>,
    ) -> Self {
        Self {
            class_like,
            name,
            modifiers,
            type_id,
            value,
            attributes,
        }
    }

    /// Returns the owning class-like ID.
    #[must_use]
    pub const fn class_like(&self) -> ClassLikeId {
        self.class_like
    }

    /// Returns the constant name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns class-constant modifiers.
    #[must_use]
    pub const fn modifiers(&self) -> &ModifierSet {
        &self.modifiers
    }

    /// Returns the constant type ID.
    #[must_use]
    pub const fn type_id(&self) -> Option<TypeId> {
        self.type_id
    }

    /// Returns the initializer constant-expression candidate.
    #[must_use]
    pub const fn value(&self) -> Option<ConstExprId> {
        self.value
    }

    /// Returns attached attribute IDs.
    #[must_use]
    pub fn attributes(&self) -> &[AttributeId] {
        &self.attributes
    }
}

impl HirClassLike {
    /// Creates a class-like HIR record.
    #[must_use]
    pub fn new(
        kind: ClassLikeKind,
        name: Option<String>,
        fqn: Option<FullyQualifiedName>,
        anonymous_id: Option<String>,
        modifiers: ModifierSet,
    ) -> Self {
        Self {
            kind,
            name,
            fqn,
            anonymous_id,
            modifiers,
            extends: Vec::new(),
            implements: Vec::new(),
            trait_uses: Vec::new(),
            members: Vec::new(),
            attributes: Vec::new(),
            backing_type: None,
        }
    }

    /// Returns class-like family.
    #[must_use]
    pub const fn kind(&self) -> ClassLikeKind {
        self.kind
    }

    /// Returns source name for named class-likes.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns global FQN for named class-likes.
    #[must_use]
    pub const fn fqn(&self) -> Option<&FullyQualifiedName> {
        self.fqn.as_ref()
    }

    /// Returns stable local anonymous class ID.
    #[must_use]
    pub fn anonymous_id(&self) -> Option<&str> {
        self.anonymous_id.as_deref()
    }

    /// Returns modifier flags.
    #[must_use]
    pub const fn modifiers(&self) -> &ModifierSet {
        &self.modifiers
    }

    /// Returns extends references.
    #[must_use]
    pub fn extends(&self) -> &[HirNameResolution] {
        &self.extends
    }

    /// Replaces extends references.
    pub fn set_extends(&mut self, extends: Vec<HirNameResolution>) {
        self.extends = extends;
    }

    /// Returns implements references.
    #[must_use]
    pub fn implements(&self) -> &[HirNameResolution] {
        &self.implements
    }

    /// Replaces implements references.
    pub fn set_implements(&mut self, implements: Vec<HirNameResolution>) {
        self.implements = implements;
    }

    /// Returns trait-use references.
    #[must_use]
    pub fn trait_uses(&self) -> &[HirNameResolution] {
        &self.trait_uses
    }

    /// Replaces trait-use references.
    pub fn set_trait_uses(&mut self, trait_uses: Vec<HirNameResolution>) {
        self.trait_uses = trait_uses;
    }

    /// Returns structural member summaries.
    #[must_use]
    pub fn members(&self) -> &[ClassLikeMember] {
        &self.members
    }

    /// Replaces member summaries.
    pub fn set_members(&mut self, members: Vec<ClassLikeMember>) {
        self.members = members;
    }

    /// Returns attached attribute IDs.
    #[must_use]
    pub fn attributes(&self) -> &[AttributeId] {
        &self.attributes
    }

    /// Replaces attached attribute IDs.
    pub fn set_attributes(&mut self, attributes: Vec<AttributeId>) {
        self.attributes = attributes;
    }

    /// Returns enum backing type ID.
    #[must_use]
    pub const fn backing_type(&self) -> Option<TypeId> {
        self.backing_type
    }

    /// Sets enum backing type ID.
    pub fn set_backing_type(&mut self, backing_type: Option<TypeId>) {
        self.backing_type = backing_type;
    }
}
