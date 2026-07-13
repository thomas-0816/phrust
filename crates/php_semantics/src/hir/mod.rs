//! Semantic frontend HIR skeleton.

pub mod arena;
pub mod attributes;
pub mod class_like;
pub mod const_expr;
pub mod decl;
pub mod declare;
pub mod expr;
pub mod ids;
pub mod modifiers;
pub mod module;
pub mod names;
pub mod signatures;
pub mod stmt;
pub mod types;

pub use arena::Arena;
pub use attributes::{AttributeTarget, HirAttribute};
pub use class_like::{
    ClassLikeKind, ClassLikeMember, ClassLikeMemberId, ClassLikeMemberKind, HirClassConst,
    HirClassLike, HirEnumCase, HirMethod, HirProperty, HirPropertyHook, HirPropertyHookBody,
    HirPropertyItem, HirTraitAdaptation, HirTraitAdaptationKind, HirTraitMethodRef, HirTraitUse,
    MagicMethodKind,
};
pub use const_expr::{ConstExpr, ConstExprContext, ConstExprKind, ConstValue};
pub use decl::{HirDecl, HirDeclKind};
pub use declare::{DeclareDirective, DeclareValue, FileDirectives, HirDeclare};
pub use expr::{DeferredEffects, HirCallArg, HirExpr, HirExprKind, HirMatchArm, HirNameResolution};
pub use ids::{
    AttributeId, ClassLikeId, ConstExprId, ConstId, DeclId, EnumCaseId, ExprId, FunctionId, HirId,
    MethodId, ModuleId, NameId, NamespaceId, ParamId, PropertyId, ScopeId, StmtId, SymbolId,
    TraitUseId, TypeId,
};
pub use modifiers::{Modifier, ModifierOccurrence, ModifierSet};
pub use module::{
    HirModule, HirNamespaceBlock, NamespaceForm, NamespaceName, TopLevelItem, TopLevelItemKind,
};
pub use names::{FullyQualifiedName, HirName, NameKind, NamePart, QualifiedName, RawName};
pub use signatures::{
    DefaultValueRef, FunctionLikeFlags, FunctionSignature, Parameter, ParameterAttribute,
    ParameterFlags, PromotedPropertyInfo, ReturnType, SignatureKind, Visibility,
};
pub use stmt::{HirCatchClause, HirIfBranch, HirStaticLocal, HirStmt, HirStmtKind, HirSwitchCase};
pub use types::{BuiltinType, HirType, HirTypeKind, TypeContext};
