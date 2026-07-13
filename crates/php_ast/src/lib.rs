//! Typed AST views over the lossless PHP CST.
//!
//! This crate is intentionally thin. It wraps `php_syntax` nodes and tokens
//! without reparsing, re-lexing, evaluating source, or adding semantic behavior
//! to the parser layer.

use php_source::TextRange;
use php_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxNodeKind, SyntaxToken};
use std::slice;

pub mod ast_node;
pub mod ast_token;
pub mod attributes;
pub mod classes;
pub mod declarations;
pub mod expressions;
pub mod names;
pub mod statements;
pub mod support;
pub mod types;
pub mod validation;

/// Common behavior for typed AST node views.
pub trait AstNode<'tree>: Clone {
    /// Attempts to cast a CST node into this AST view.
    fn cast(node: &'tree SyntaxNode) -> Option<Self>
    where
        Self: Sized;

    /// Returns true when a CST node kind can be viewed as this AST node.
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized;

    /// Returns the wrapped CST node.
    fn syntax(&self) -> &'tree SyntaxNode;

    /// Returns the CST node kind.
    #[must_use]
    fn kind(&self) -> SyntaxKind {
        *self.syntax().kind()
    }

    /// Returns the byte range covered by this view.
    #[must_use]
    fn text_range(&self) -> TextRange {
        self.syntax().text_range()
    }

    /// Returns a stable source-local pointer for this AST node.
    #[must_use]
    fn ast_ptr(&self) -> AstPtr {
        AstPtr::new(self.kind(), self.text_range())
    }
}

/// Common behavior for typed AST token views.
pub trait AstToken<'tree>: Clone {
    /// Attempts to cast a CST token into this AST token view.
    fn cast(token: &'tree SyntaxToken) -> Option<Self>
    where
        Self: Sized;

    /// Returns true when a CST token kind can be viewed as this AST token.
    fn can_cast(kind: SyntaxKind) -> bool
    where
        Self: Sized;

    /// Returns the wrapped CST token.
    fn syntax(&self) -> &'tree SyntaxToken;

    /// Returns the CST token kind.
    #[must_use]
    fn kind(&self) -> SyntaxKind {
        *self.syntax().kind()
    }

    /// Returns the token text.
    #[must_use]
    fn text(&self) -> &'tree str {
        self.syntax().text()
    }

    /// Returns the token byte range.
    #[must_use]
    fn text_range(&self) -> TextRange {
        self.syntax().text_range()
    }
}

/// Iterator over typed direct AST child nodes.
pub struct AstChildren<'tree, N> {
    inner: slice::Iter<'tree, SyntaxElement>,
    _marker: std::marker::PhantomData<N>,
}

impl<'tree, N> AstChildren<'tree, N> {
    /// Creates a typed child iterator for one CST node.
    #[must_use]
    pub fn new(node: &'tree SyntaxNode) -> Self {
        Self {
            inner: node.children().iter(),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'tree, N> Iterator for AstChildren<'tree, N>
where
    N: AstNode<'tree>,
{
    type Item = N;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.find_map(|child| match child {
            SyntaxElement::Node(node) => N::cast(node),
            SyntaxElement::Token(_) => None,
        })
    }
}

/// Convenience helpers for finding typed AST views from a CST node.
pub trait SyntaxNodeExt {
    /// Returns typed direct child nodes.
    fn ast_children<'tree, N>(&'tree self) -> AstChildren<'tree, N>
    where
        N: AstNode<'tree>;

    /// Returns the first typed direct child node.
    fn ast_child<'tree, N>(&'tree self) -> Option<N>
    where
        N: AstNode<'tree>;

    /// Returns typed direct child tokens.
    fn ast_tokens<'tree, T>(&'tree self) -> impl Iterator<Item = T> + 'tree
    where
        T: AstToken<'tree> + 'tree;
}

impl SyntaxNodeExt for SyntaxNode {
    fn ast_children<'tree, N>(&'tree self) -> AstChildren<'tree, N>
    where
        N: AstNode<'tree>,
    {
        AstChildren::new(self)
    }

    fn ast_child<'tree, N>(&'tree self) -> Option<N>
    where
        N: AstNode<'tree>,
    {
        self.ast_children().next()
    }

    fn ast_tokens<'tree, T>(&'tree self) -> impl Iterator<Item = T> + 'tree
    where
        T: AstToken<'tree> + 'tree,
    {
        child_tokens(self)
    }
}

/// Source-local AST identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceAstId {
    ordinal: u32,
}

impl SourceAstId {
    /// Creates a source-local ID from a caller-assigned ordinal.
    #[must_use]
    pub const fn new(ordinal: u32) -> Self {
        Self { ordinal }
    }

    /// Returns the source-local ordinal.
    #[must_use]
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }
}

/// Stable pointer to a CST-backed AST node within one source file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AstPtr {
    kind: SyntaxKind,
    range: TextRange,
}

impl AstPtr {
    /// Creates a pointer from CST kind and range.
    #[must_use]
    pub const fn new(kind: SyntaxKind, range: TextRange) -> Self {
        Self { kind, range }
    }

    /// Returns the pointed node kind.
    #[must_use]
    pub const fn kind(self) -> SyntaxKind {
        self.kind
    }

    /// Returns the pointed byte range.
    #[must_use]
    pub const fn text_range(self) -> TextRange {
        self.range
    }
}

macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[doc = concat!("Typed AST view for `", stringify!($kind), "`.")]
        #[derive(Clone, Copy, Debug)]
        pub struct $name<'tree> {
            syntax: &'tree SyntaxNode,
        }

        impl<'tree> $name<'tree> {
            /// Wraps a CST node if its kind matches this view.
            #[must_use]
            pub fn new(syntax: &'tree SyntaxNode) -> Option<Self> {
                Self::cast(syntax)
            }
        }

        impl<'tree> AstNode<'tree> for $name<'tree> {
            fn cast(node: &'tree SyntaxNode) -> Option<Self> {
                Self::can_cast(*node.kind()).then_some(Self { syntax: node })
            }

            fn can_cast(kind: SyntaxKind) -> bool {
                matches!(kind, SyntaxKind::Node(SyntaxNodeKind::$kind))
            }

            fn syntax(&self) -> &'tree SyntaxNode {
                self.syntax
            }
        }
    };
}

ast_node!(SourceFile, SourceFile);
ast_node!(NamespaceDecl, NamespaceStmt);
ast_node!(UseDecl, UseDecl);
ast_node!(ConstDecl, ConstDecl);
ast_node!(FunctionDecl, FunctionDecl);
ast_node!(ClassDecl, ClassDecl);
ast_node!(InterfaceDecl, InterfaceDecl);
ast_node!(TraitDecl, TraitDecl);
ast_node!(EnumDecl, EnumDecl);
ast_node!(MethodDecl, MethodDecl);
ast_node!(PropertyDecl, PropertyDecl);
ast_node!(ClassConstDecl, ClassConstDecl);
ast_node!(ParamList, ParamList);
ast_node!(Param, Param);
ast_node!(AttributeGroup, AttributeGroup);
ast_node!(Attribute, Attribute);
ast_node!(ClassMemberList, ClassMemberList);
ast_node!(TraitUseDecl, TraitUseDecl);
ast_node!(TypeNode, Type);
ast_node!(UnionType, UnionType);
ast_node!(IntersectionType, IntersectionType);
ast_node!(NullableType, NullableType);
ast_node!(DnfType, DnfType);
ast_node!(StatementList, StatementList);
ast_node!(InlineHtmlStmt, InlineHtml);
ast_node!(EmptyStmt, EmptyStmt);
ast_node!(ExprStmt, ExprStmt);
ast_node!(EchoStmt, EchoStmt);
ast_node!(ReturnStmt, ReturnStmt);
ast_node!(ThrowStmt, ThrowStmt);
ast_node!(BreakStmt, BreakStmt);
ast_node!(ContinueStmt, ContinueStmt);
ast_node!(BlockStmt, BlockStmt);
ast_node!(IfStmt, IfStmt);
ast_node!(WhileStmt, WhileStmt);
ast_node!(DoWhileStmt, DoWhileStmt);
ast_node!(ForStmt, ForStmt);
ast_node!(ForeachStmt, ForeachStmt);
ast_node!(SwitchStmt, SwitchStmt);
ast_node!(TryStmt, TryStmt);
ast_node!(CatchClause, CatchClause);
ast_node!(FinallyClause, FinallyClause);
ast_node!(DeclareStmt, DeclareStmt);
ast_node!(GlobalStmt, GlobalStmt);
ast_node!(StaticStmt, StaticStmt);
ast_node!(UnsetStmt, UnsetStmt);
ast_node!(GotoStmt, GotoStmt);
ast_node!(LabelStmt, LabelStmt);
ast_node!(Expr, Expr);
ast_node!(Literal, Literal);
ast_node!(Name, Name);
ast_node!(Variable, Variable);
ast_node!(ParenthesizedExpr, ParenthesizedExpr);
ast_node!(PrefixExpr, PrefixExpr);
ast_node!(PostfixExpr, PostfixExpr);
ast_node!(VoidCastExpr, VoidCastExpr);
ast_node!(BinaryExpr, BinaryExpr);
ast_node!(AssignExpr, AssignExpr);
ast_node!(TernaryExpr, TernaryExpr);
ast_node!(CallExpr, CallExpr);
ast_node!(ArrayDimFetchExpr, ArrayDimFetchExpr);
ast_node!(PropertyFetchExpr, PropertyFetchExpr);
ast_node!(StaticAccessExpr, StaticAccessExpr);
ast_node!(ArrayExpr, ArrayExpr);
ast_node!(ArrayPair, ArrayPair);
ast_node!(MatchExpr, MatchExpr);
ast_node!(ThrowExpr, ThrowExpr);
ast_node!(ConstructExpr, ConstructExpr);
ast_node!(YieldExpr, YieldExpr);
ast_node!(ClosureExpr, ClosureExpr);
ast_node!(ArrowFunctionExpr, ArrowFunctionExpr);
ast_node!(NewExpr, NewExpr);
ast_node!(CloneExpr, CloneExpr);
ast_node!(CloneWithExpr, CloneWithExpr);
ast_node!(PipeExpr, PipeExpr);
ast_node!(StringNode, String);
ast_node!(Encapsed, Encapsed);
ast_node!(Heredoc, Heredoc);

/// Compatibility name used by the semantic frontend.
pub type ParameterList<'tree> = ParamList<'tree>;
/// Compatibility name used by the semantic frontend.
pub type Parameter<'tree> = Param<'tree>;
/// Compatibility name for parser attribute groups.
pub type AttributeList<'tree> = AttributeGroup<'tree>;
/// Compatibility name for use items. The current CST stores item structure
/// inside `USE_DECL`; the declarative API expands around that shape.
pub type UseItem<'tree> = UseDecl<'tree>;
/// Compatibility name for anonymous class expressions.
pub type AnonymousClassExpr<'tree> = AnonymousClassDecl<'tree>;
/// Compatibility name for trait-use declarations.
pub type TraitUse<'tree> = TraitUseDecl<'tree>;
/// Compatibility name for property items. The current CST stores property
/// items inside `PROPERTY_DECL`.
pub type PropertyItem<'tree> = PropertyDecl<'tree>;
/// Compatibility name for generic type syntax.
pub type TypeSyntax<'tree> = TypeNode<'tree>;
/// Compatibility name for expression statements.
pub type ExpressionStmt<'tree> = ExprStmt<'tree>;
/// Compatibility name for literal expressions.
pub type LiteralExpr<'tree> = Literal<'tree>;
/// Compatibility name for variable expressions.
pub type VariableExpr<'tree> = Variable<'tree>;
/// Compatibility name for name expressions.
pub type NameExpr<'tree> = Name<'tree>;
/// Compatibility name for prefix/unary expressions.
pub type UnaryExpr<'tree> = PrefixExpr<'tree>;
/// Compatibility name for coalesce expressions, represented by `BINARY_EXPR`.
pub type CoalesceExpr<'tree> = BinaryExpr<'tree>;
/// Compatibility name for list expressions, represented by `ARRAY_EXPR`.
pub type ListExpr<'tree> = ArrayExpr<'tree>;
/// Compatibility name for object method-call syntax.
pub type MethodCallExpr<'tree> = PropertyFetchExpr<'tree>;
/// Compatibility name for nullsafe method-call syntax.
pub type NullsafeMethodCallExpr<'tree> = PropertyFetchExpr<'tree>;
/// Compatibility name for nullsafe property-fetch syntax.
pub type NullsafePropertyFetchExpr<'tree> = PropertyFetchExpr<'tree>;
/// Compatibility name for dimension fetches.
pub type DimFetchExpr<'tree> = ArrayDimFetchExpr<'tree>;
/// Compatibility name for first-class callable syntax, represented by calls.
pub type FirstClassCallableExpr<'tree> = CallExpr<'tree>;
/// Compatibility name for yield-from syntax, represented by `YIELD_EXPR`.
pub type YieldFromExpr<'tree> = YieldExpr<'tree>;
/// Compatibility name for include/require expressions.
pub type IncludeExpr<'tree> = ConstructExpr<'tree>;
/// Compatibility name for eval expressions.
pub type EvalExpr<'tree> = ConstructExpr<'tree>;
/// Compatibility name for exit/die expressions.
pub type ExitExpr<'tree> = ConstructExpr<'tree>;
/// Compatibility name for cast expressions.
pub type CastExpr<'tree> = PrefixExpr<'tree>;
/// Compatibility name for named type syntax.
pub type NamedType<'tree> = TypeNode<'tree>;
/// Compatibility name for parenthesized type syntax in the current CST.
pub type ParenthesizedType<'tree> = DnfType<'tree>;
/// Compatibility names for keyword type atoms represented by `TYPE`.
pub type VoidType<'tree> = TypeNode<'tree>;
pub type NeverType<'tree> = TypeNode<'tree>;
pub type StaticType<'tree> = TypeNode<'tree>;
pub type SelfType<'tree> = TypeNode<'tree>;
pub type ParentType<'tree> = TypeNode<'tree>;
pub type FalseType<'tree> = TypeNode<'tree>;
pub type TrueType<'tree> = TypeNode<'tree>;
pub type NullType<'tree> = TypeNode<'tree>;
pub type MixedType<'tree> = TypeNode<'tree>;
pub type IterableType<'tree> = TypeNode<'tree>;
pub type ObjectType<'tree> = TypeNode<'tree>;
pub type CallableType<'tree> = TypeNode<'tree>;

/// PHP cast operators visible in expression syntax.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CastKind {
    /// `(int)` cast.
    Int,
    /// `(float)` or `(double)` cast.
    Float,
    /// `(string)` cast.
    String,
    /// `(array)` cast.
    Array,
    /// `(object)` cast.
    Object,
    /// `(bool)` cast.
    Bool,
    /// `(unset)` cast.
    Unset,
    /// PHP 8.5 `(void)` cast.
    Void,
}

/// One entry in a comma-separated expression list.
#[derive(Clone, Debug)]
pub enum ExprListItem<'tree> {
    /// A parsed expression entry.
    Expression(ExprNode<'tree>),
    /// A parser recovery node occupying an entry position.
    Error(&'tree SyntaxNode),
}

impl ExprListItem<'_> {
    /// Returns the exact byte range occupied by this list entry.
    #[must_use]
    pub fn text_range(&self) -> TextRange {
        match self {
            Self::Expression(expression) => expression.syntax().text_range(),
            Self::Error(node) => node.text_range(),
        }
    }
}

/// Built-in construct expression family represented by `CONSTRUCT_EXPR`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructKind {
    /// `include`.
    Include,
    /// `include_once`.
    IncludeOnce,
    /// `require`.
    Require,
    /// `require_once`.
    RequireOnce,
    /// `print`.
    Print,
    /// `isset`.
    Isset,
    /// `empty`.
    Empty,
    /// `eval`.
    Eval,
    /// `exit` or `die`.
    Exit,
}

/// PHP keyword type atoms visible inside type syntax.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeKeyword {
    /// `void`.
    Void,
    /// `never`.
    Never,
    /// `static`.
    Static,
    /// `self`.
    Self_,
    /// `parent`.
    Parent,
    /// `false`.
    False,
    /// `true`.
    True,
    /// `null`.
    Null,
    /// `mixed`.
    Mixed,
    /// `iterable`.
    Iterable,
    /// `object`.
    Object,
    /// `callable`.
    Callable,
    /// `array`.
    Array,
}

/// Extends-clause view over class/interface declarations that contain a raw
/// `T_EXTENDS` token. The current CST does not create a separate clause node.
#[derive(Clone, Copy, Debug)]
pub struct ExtendsClause<'tree> {
    syntax: &'tree SyntaxNode,
}

impl<'tree> ExtendsClause<'tree> {
    /// Wraps a declaration node when it contains an extends clause.
    #[must_use]
    pub fn new(syntax: &'tree SyntaxNode) -> Option<Self> {
        Self::cast(syntax)
    }
}

impl<'tree> AstNode<'tree> for ExtendsClause<'tree> {
    fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        Self::can_cast(*node.kind()).then_some(()).and_then(|()| {
            has_direct_token_name(node, "T_EXTENDS").then_some(Self { syntax: node })
        })
    }

    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::Node(SyntaxNodeKind::ClassDecl | SyntaxNodeKind::InterfaceDecl)
        )
    }

    fn syntax(&self) -> &'tree SyntaxNode {
        self.syntax
    }
}

/// Implements-clause view over class/enum declarations that contain a raw
/// `T_IMPLEMENTS` token. The current CST does not create a separate clause node.
#[derive(Clone, Copy, Debug)]
pub struct ImplementsClause<'tree> {
    syntax: &'tree SyntaxNode,
}

impl<'tree> ImplementsClause<'tree> {
    /// Wraps a declaration node when it contains an implements clause.
    #[must_use]
    pub fn new(syntax: &'tree SyntaxNode) -> Option<Self> {
        Self::cast(syntax)
    }
}

impl<'tree> AstNode<'tree> for ImplementsClause<'tree> {
    fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        Self::can_cast(*node.kind()).then_some(()).and_then(|()| {
            has_direct_token_name(node, "T_IMPLEMENTS").then_some(Self { syntax: node })
        })
    }

    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::Node(SyntaxNodeKind::ClassDecl | SyntaxNodeKind::EnumDecl)
        )
    }

    fn syntax(&self) -> &'tree SyntaxNode {
        self.syntax
    }
}

/// Trait-adaptation view over trait-use declarations with an adaptation block.
#[derive(Clone, Copy, Debug)]
pub struct TraitAdaptation<'tree> {
    syntax: &'tree SyntaxNode,
}

impl<'tree> TraitAdaptation<'tree> {
    /// Wraps a trait-use declaration when it has an adaptation block.
    #[must_use]
    pub fn new(syntax: &'tree SyntaxNode) -> Option<Self> {
        Self::cast(syntax)
    }
}

impl<'tree> EchoStmt<'tree> {
    /// Returns echoed expressions in source order.
    pub fn expressions(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }

    /// Returns parsed and recovered list entries in source order.
    pub fn expression_items(&self) -> impl Iterator<Item = ExprListItem<'tree>> + 'tree {
        direct_expr_list_items(self.syntax).into_iter()
    }

    /// Returns the byte ranges of direct comma separators.
    pub fn comma_ranges(&self) -> impl Iterator<Item = TextRange> + 'tree {
        direct_comma_ranges(self.syntax).into_iter()
    }
}

impl<'tree> DeclareStmt<'tree> {
    /// Returns directive assignments in source order.
    pub fn directives(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }

    /// Returns parsed and recovered directive entries in source order.
    pub fn expression_items(&self) -> impl Iterator<Item = ExprListItem<'tree>> + 'tree {
        direct_expr_list_items(self.syntax).into_iter()
    }

    /// Returns the byte ranges of direct comma separators.
    pub fn comma_ranges(&self) -> impl Iterator<Item = TextRange> + 'tree {
        direct_comma_ranges(self.syntax).into_iter()
    }
}

impl<'tree> GlobalStmt<'tree> {
    /// Returns global variable expressions in source order.
    pub fn variables(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }

    /// Returns parsed and recovered global entries in source order.
    pub fn expression_items(&self) -> impl Iterator<Item = ExprListItem<'tree>> + 'tree {
        direct_expr_list_items(self.syntax).into_iter()
    }

    /// Returns the byte ranges of direct comma separators.
    pub fn comma_ranges(&self) -> impl Iterator<Item = TextRange> + 'tree {
        direct_comma_ranges(self.syntax).into_iter()
    }
}

impl<'tree> StaticStmt<'tree> {
    /// Returns static-local declarations in source order.
    pub fn locals(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }

    /// Returns parsed and recovered static-local entries in source order.
    pub fn expression_items(&self) -> impl Iterator<Item = ExprListItem<'tree>> + 'tree {
        direct_expr_list_items(self.syntax).into_iter()
    }

    /// Returns the byte ranges of direct comma separators.
    pub fn comma_ranges(&self) -> impl Iterator<Item = TextRange> + 'tree {
        direct_comma_ranges(self.syntax).into_iter()
    }
}

impl<'tree> AssignExpr<'tree> {
    /// Returns the assignment target expression.
    #[must_use]
    pub fn left(&self) -> Option<ExprNode<'tree>> {
        direct_expr_nodes(self.syntax).into_iter().next()
    }

    /// Returns the assigned value expression.
    #[must_use]
    pub fn right(&self) -> Option<ExprNode<'tree>> {
        direct_expr_nodes(self.syntax).into_iter().nth(1)
    }
}

impl<'tree> UnsetStmt<'tree> {
    /// Returns unset targets in source order.
    pub fn expressions(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }

    /// Returns parsed and recovered unset entries in source order.
    pub fn expression_items(&self) -> impl Iterator<Item = ExprListItem<'tree>> + 'tree {
        direct_expr_list_items(self.syntax).into_iter()
    }

    /// Returns the byte ranges of direct comma separators.
    pub fn comma_ranges(&self) -> impl Iterator<Item = TextRange> + 'tree {
        direct_comma_ranges(self.syntax).into_iter()
    }
}

impl<'tree> AstNode<'tree> for TraitAdaptation<'tree> {
    fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        let trait_use = TraitUseDecl::cast(node)?;
        trait_use
            .has_adaptation_block()
            .then_some(Self { syntax: node })
    }

    fn can_cast(kind: SyntaxKind) -> bool {
        TraitUseDecl::can_cast(kind)
    }

    fn syntax(&self) -> &'tree SyntaxNode {
        self.syntax
    }
}

/// Enum-case view over `PROPERTY_DECL` nodes beginning with `T_CASE`.
#[derive(Clone, Copy, Debug)]
pub struct EnumCase<'tree> {
    syntax: &'tree SyntaxNode,
}

impl<'tree> EnumCase<'tree> {
    /// Wraps an enum case represented by the current CST.
    #[must_use]
    pub fn new(syntax: &'tree SyntaxNode) -> Option<Self> {
        Self::cast(syntax)
    }
}

impl<'tree> AstNode<'tree> for EnumCase<'tree> {
    fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        let property = PropertyDecl::cast(node)?;
        property.is_enum_case().then_some(Self { syntax: node })
    }

    fn can_cast(kind: SyntaxKind) -> bool {
        PropertyDecl::can_cast(kind)
    }

    fn syntax(&self) -> &'tree SyntaxNode {
        self.syntax
    }
}

/// A grouped use declaration view. The current CST represents grouped and
/// ungrouped imports with `USE_DECL`; this view gives semantic code a distinct
/// API surface without inventing a second parser node.
#[derive(Clone, Copy, Debug)]
pub struct UseGroup<'tree> {
    syntax: &'tree SyntaxNode,
}

impl<'tree> UseGroup<'tree> {
    /// Wraps a use declaration as a use group view.
    #[must_use]
    pub fn new(syntax: &'tree SyntaxNode) -> Option<Self> {
        Self::cast(syntax)
    }
}

impl<'tree> AstNode<'tree> for UseGroup<'tree> {
    fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        let use_decl = UseDecl::cast(node)?;
        use_decl.is_grouped().then_some(Self { syntax: node })
    }

    fn can_cast(kind: SyntaxKind) -> bool {
        UseDecl::can_cast(kind)
    }

    fn syntax(&self) -> &'tree SyntaxNode {
        self.syntax
    }
}

/// Anonymous class view over `CLASS_DECL` nodes that use anonymous-class
/// surface syntax.
#[derive(Clone, Copy, Debug)]
pub struct AnonymousClassDecl<'tree> {
    syntax: &'tree SyntaxNode,
}

impl<'tree> AnonymousClassDecl<'tree> {
    /// Wraps a class declaration when it has anonymous-class syntax.
    #[must_use]
    pub fn new(syntax: &'tree SyntaxNode) -> Option<Self> {
        Self::cast(syntax)
    }
}

impl<'tree> AstNode<'tree> for AnonymousClassDecl<'tree> {
    fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        let class = ClassDecl::cast(node)?;
        class.is_anonymous().then_some(Self { syntax: node })
    }

    fn can_cast(kind: SyntaxKind) -> bool {
        ClassDecl::can_cast(kind)
    }

    fn syntax(&self) -> &'tree SyntaxNode {
        self.syntax
    }
}

/// A class-like declaration view.
#[derive(Clone, Copy, Debug)]
pub enum ClassLikeDecl<'tree> {
    /// Class declaration.
    Class(ClassDecl<'tree>),
    /// Interface declaration.
    Interface(InterfaceDecl<'tree>),
    /// Trait declaration.
    Trait(TraitDecl<'tree>),
    /// Enum declaration.
    Enum(EnumDecl<'tree>),
}

impl<'tree> ClassLikeDecl<'tree> {
    /// Attempts to cast a CST node to any class-like declaration.
    #[must_use]
    pub fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        ClassDecl::cast(node)
            .map(Self::Class)
            .or_else(|| InterfaceDecl::cast(node).map(Self::Interface))
            .or_else(|| TraitDecl::cast(node).map(Self::Trait))
            .or_else(|| EnumDecl::cast(node).map(Self::Enum))
    }

    /// Returns the wrapped CST node.
    #[must_use]
    pub fn syntax(&self) -> &'tree SyntaxNode {
        match self {
            Self::Class(node) => node.syntax(),
            Self::Interface(node) => node.syntax(),
            Self::Trait(node) => node.syntax(),
            Self::Enum(node) => node.syntax(),
        }
    }

    /// Returns the wrapped CST node kind.
    #[must_use]
    pub fn kind(&self) -> SyntaxKind {
        *self.syntax().kind()
    }

    /// Returns the class-like member list, if present.
    #[must_use]
    pub fn member_list(&self) -> Option<ClassMemberList<'tree>> {
        child_node(self.syntax())
    }

    /// Returns class-like members in source order.
    pub fn members(&self) -> impl Iterator<Item = MemberDecl<'tree>> + 'tree {
        let mut out = Vec::new();
        if let Some(member_list) = self.member_list() {
            out.extend(member_list.members());
        }
        out.into_iter()
    }
}

/// Class-like member declaration view.
#[derive(Clone, Copy, Debug)]
pub enum MemberDecl<'tree> {
    /// Method declaration.
    Method(MethodDecl<'tree>),
    /// Property declaration.
    Property(PropertyDecl<'tree>),
    /// Class constant declaration.
    ClassConst(ClassConstDecl<'tree>),
    /// Trait use declaration.
    TraitUse(TraitUseDecl<'tree>),
}

impl<'tree> MemberDecl<'tree> {
    /// Attempts to cast a CST node to a member declaration.
    #[must_use]
    pub fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        MethodDecl::cast(node)
            .map(Self::Method)
            .or_else(|| PropertyDecl::cast(node).map(Self::Property))
            .or_else(|| ClassConstDecl::cast(node).map(Self::ClassConst))
            .or_else(|| TraitUseDecl::cast(node).map(Self::TraitUse))
    }

    /// Returns the wrapped CST node.
    #[must_use]
    pub fn syntax(&self) -> &'tree SyntaxNode {
        match self {
            Self::Method(node) => node.syntax(),
            Self::Property(node) => node.syntax(),
            Self::ClassConst(node) => node.syntax(),
            Self::TraitUse(node) => node.syntax(),
        }
    }
}

/// Top-level declaration view.
#[derive(Clone, Copy, Debug)]
pub enum Decl<'tree> {
    /// Namespace declaration.
    Namespace(NamespaceDecl<'tree>),
    /// Use declaration.
    Use(UseDecl<'tree>),
    /// Namespace-level constant declaration.
    Const(ConstDecl<'tree>),
    /// Function declaration.
    Function(FunctionDecl<'tree>),
    /// Class-like declaration.
    ClassLike(ClassLikeDecl<'tree>),
}

/// Statement or statement-like clause view.
#[derive(Clone, Copy, Debug)]
pub enum Stmt<'tree> {
    /// Inline HTML outside PHP mode.
    InlineHtml(InlineHtmlStmt<'tree>),
    /// Empty statement.
    Empty(EmptyStmt<'tree>),
    /// Expression statement.
    Expr(ExprStmt<'tree>),
    /// Echo statement.
    Echo(EchoStmt<'tree>),
    /// Return statement.
    Return(ReturnStmt<'tree>),
    /// Throw statement.
    Throw(ThrowStmt<'tree>),
    /// Break statement.
    Break(BreakStmt<'tree>),
    /// Continue statement.
    Continue(ContinueStmt<'tree>),
    /// Block statement.
    Block(BlockStmt<'tree>),
    /// If statement.
    If(IfStmt<'tree>),
    /// While statement.
    While(WhileStmt<'tree>),
    /// Do/while statement.
    DoWhile(DoWhileStmt<'tree>),
    /// For statement.
    For(ForStmt<'tree>),
    /// Foreach statement.
    Foreach(ForeachStmt<'tree>),
    /// Switch statement.
    Switch(SwitchStmt<'tree>),
    /// Try statement.
    Try(TryStmt<'tree>),
    /// Catch clause.
    Catch(CatchClause<'tree>),
    /// Finally clause.
    Finally(FinallyClause<'tree>),
    /// Declare statement.
    Declare(DeclareStmt<'tree>),
    /// Global statement.
    Global(GlobalStmt<'tree>),
    /// Static local statement.
    Static(StaticStmt<'tree>),
    /// Unset statement.
    Unset(UnsetStmt<'tree>),
    /// Goto statement.
    Goto(GotoStmt<'tree>),
    /// Label statement.
    Label(LabelStmt<'tree>),
}

impl<'tree> Stmt<'tree> {
    /// Attempts to cast a CST node to a statement view.
    #[must_use]
    pub fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        InlineHtmlStmt::cast(node)
            .map(Self::InlineHtml)
            .or_else(|| EmptyStmt::cast(node).map(Self::Empty))
            .or_else(|| ExprStmt::cast(node).map(Self::Expr))
            .or_else(|| EchoStmt::cast(node).map(Self::Echo))
            .or_else(|| ReturnStmt::cast(node).map(Self::Return))
            .or_else(|| ThrowStmt::cast(node).map(Self::Throw))
            .or_else(|| BreakStmt::cast(node).map(Self::Break))
            .or_else(|| ContinueStmt::cast(node).map(Self::Continue))
            .or_else(|| BlockStmt::cast(node).map(Self::Block))
            .or_else(|| IfStmt::cast(node).map(Self::If))
            .or_else(|| WhileStmt::cast(node).map(Self::While))
            .or_else(|| DoWhileStmt::cast(node).map(Self::DoWhile))
            .or_else(|| ForStmt::cast(node).map(Self::For))
            .or_else(|| ForeachStmt::cast(node).map(Self::Foreach))
            .or_else(|| SwitchStmt::cast(node).map(Self::Switch))
            .or_else(|| TryStmt::cast(node).map(Self::Try))
            .or_else(|| CatchClause::cast(node).map(Self::Catch))
            .or_else(|| FinallyClause::cast(node).map(Self::Finally))
            .or_else(|| DeclareStmt::cast(node).map(Self::Declare))
            .or_else(|| GlobalStmt::cast(node).map(Self::Global))
            .or_else(|| StaticStmt::cast(node).map(Self::Static))
            .or_else(|| UnsetStmt::cast(node).map(Self::Unset))
            .or_else(|| GotoStmt::cast(node).map(Self::Goto))
            .or_else(|| LabelStmt::cast(node).map(Self::Label))
    }

    /// Returns the wrapped CST node.
    #[must_use]
    pub fn syntax(&self) -> &'tree SyntaxNode {
        match self {
            Self::InlineHtml(node) => node.syntax(),
            Self::Empty(node) => node.syntax(),
            Self::Expr(node) => node.syntax(),
            Self::Echo(node) => node.syntax(),
            Self::Return(node) => node.syntax(),
            Self::Throw(node) => node.syntax(),
            Self::Break(node) => node.syntax(),
            Self::Continue(node) => node.syntax(),
            Self::Block(node) => node.syntax(),
            Self::If(node) => node.syntax(),
            Self::While(node) => node.syntax(),
            Self::DoWhile(node) => node.syntax(),
            Self::For(node) => node.syntax(),
            Self::Foreach(node) => node.syntax(),
            Self::Switch(node) => node.syntax(),
            Self::Try(node) => node.syntax(),
            Self::Catch(node) => node.syntax(),
            Self::Finally(node) => node.syntax(),
            Self::Declare(node) => node.syntax(),
            Self::Global(node) => node.syntax(),
            Self::Static(node) => node.syntax(),
            Self::Unset(node) => node.syntax(),
            Self::Goto(node) => node.syntax(),
            Self::Label(node) => node.syntax(),
        }
    }
}

/// Expression view.
#[derive(Clone, Copy, Debug)]
pub enum ExprNode<'tree> {
    /// Generic expression wrapper.
    Expr(Expr<'tree>),
    /// Literal expression.
    Literal(Literal<'tree>),
    /// Name expression.
    Name(Name<'tree>),
    /// Variable expression.
    Variable(Variable<'tree>),
    /// Parenthesized expression.
    Parenthesized(ParenthesizedExpr<'tree>),
    /// Prefix expression.
    Prefix(PrefixExpr<'tree>),
    /// Postfix expression.
    Postfix(PostfixExpr<'tree>),
    /// PHP 8.5 void cast.
    VoidCast(VoidCastExpr<'tree>),
    /// Binary expression.
    Binary(BinaryExpr<'tree>),
    /// Assignment expression.
    Assign(AssignExpr<'tree>),
    /// Ternary expression.
    Ternary(TernaryExpr<'tree>),
    /// Call expression.
    Call(CallExpr<'tree>),
    /// Array/dimension fetch expression.
    ArrayDimFetch(ArrayDimFetchExpr<'tree>),
    /// Property fetch expression.
    PropertyFetch(PropertyFetchExpr<'tree>),
    /// Static access expression.
    StaticAccess(StaticAccessExpr<'tree>),
    /// Array expression.
    Array(ArrayExpr<'tree>),
    /// Array pair.
    ArrayPair(ArrayPair<'tree>),
    /// Match expression.
    Match(MatchExpr<'tree>),
    /// Throw expression.
    Throw(ThrowExpr<'tree>),
    /// Construct expression.
    Construct(ConstructExpr<'tree>),
    /// Yield expression.
    Yield(YieldExpr<'tree>),
    /// Closure expression.
    Closure(ClosureExpr<'tree>),
    /// Arrow function expression.
    ArrowFunction(ArrowFunctionExpr<'tree>),
    /// New expression.
    New(NewExpr<'tree>),
    /// Clone expression.
    Clone(CloneExpr<'tree>),
    /// PHP 8.5 clone-with expression.
    CloneWith(CloneWithExpr<'tree>),
    /// PHP 8.5 pipe expression.
    Pipe(PipeExpr<'tree>),
    /// String node.
    String(StringNode<'tree>),
    /// Encapsed string node.
    Encapsed(Encapsed<'tree>),
    /// Heredoc node.
    Heredoc(Heredoc<'tree>),
}

impl<'tree> ExprNode<'tree> {
    /// Attempts to cast a CST node to an expression view.
    #[must_use]
    pub fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        Expr::cast(node)
            .map(Self::Expr)
            .or_else(|| Literal::cast(node).map(Self::Literal))
            .or_else(|| Name::cast(node).map(Self::Name))
            .or_else(|| Variable::cast(node).map(Self::Variable))
            .or_else(|| ParenthesizedExpr::cast(node).map(Self::Parenthesized))
            .or_else(|| PrefixExpr::cast(node).map(Self::Prefix))
            .or_else(|| PostfixExpr::cast(node).map(Self::Postfix))
            .or_else(|| VoidCastExpr::cast(node).map(Self::VoidCast))
            .or_else(|| BinaryExpr::cast(node).map(Self::Binary))
            .or_else(|| AssignExpr::cast(node).map(Self::Assign))
            .or_else(|| TernaryExpr::cast(node).map(Self::Ternary))
            .or_else(|| CallExpr::cast(node).map(Self::Call))
            .or_else(|| ArrayDimFetchExpr::cast(node).map(Self::ArrayDimFetch))
            .or_else(|| PropertyFetchExpr::cast(node).map(Self::PropertyFetch))
            .or_else(|| StaticAccessExpr::cast(node).map(Self::StaticAccess))
            .or_else(|| ArrayExpr::cast(node).map(Self::Array))
            .or_else(|| ArrayPair::cast(node).map(Self::ArrayPair))
            .or_else(|| MatchExpr::cast(node).map(Self::Match))
            .or_else(|| ThrowExpr::cast(node).map(Self::Throw))
            .or_else(|| ConstructExpr::cast(node).map(Self::Construct))
            .or_else(|| YieldExpr::cast(node).map(Self::Yield))
            .or_else(|| ClosureExpr::cast(node).map(Self::Closure))
            .or_else(|| ArrowFunctionExpr::cast(node).map(Self::ArrowFunction))
            .or_else(|| NewExpr::cast(node).map(Self::New))
            .or_else(|| CloneExpr::cast(node).map(Self::Clone))
            .or_else(|| CloneWithExpr::cast(node).map(Self::CloneWith))
            .or_else(|| PipeExpr::cast(node).map(Self::Pipe))
            .or_else(|| StringNode::cast(node).map(Self::String))
            .or_else(|| Encapsed::cast(node).map(Self::Encapsed))
            .or_else(|| Heredoc::cast(node).map(Self::Heredoc))
    }

    /// Returns the wrapped CST node.
    #[must_use]
    pub fn syntax(&self) -> &'tree SyntaxNode {
        match self {
            Self::Expr(node) => node.syntax(),
            Self::Literal(node) => node.syntax(),
            Self::Name(node) => node.syntax(),
            Self::Variable(node) => node.syntax(),
            Self::Parenthesized(node) => node.syntax(),
            Self::Prefix(node) => node.syntax(),
            Self::Postfix(node) => node.syntax(),
            Self::VoidCast(node) => node.syntax(),
            Self::Binary(node) => node.syntax(),
            Self::Assign(node) => node.syntax(),
            Self::Ternary(node) => node.syntax(),
            Self::Call(node) => node.syntax(),
            Self::ArrayDimFetch(node) => node.syntax(),
            Self::PropertyFetch(node) => node.syntax(),
            Self::StaticAccess(node) => node.syntax(),
            Self::Array(node) => node.syntax(),
            Self::ArrayPair(node) => node.syntax(),
            Self::Match(node) => node.syntax(),
            Self::Throw(node) => node.syntax(),
            Self::Construct(node) => node.syntax(),
            Self::Yield(node) => node.syntax(),
            Self::Closure(node) => node.syntax(),
            Self::ArrowFunction(node) => node.syntax(),
            Self::New(node) => node.syntax(),
            Self::Clone(node) => node.syntax(),
            Self::CloneWith(node) => node.syntax(),
            Self::Pipe(node) => node.syntax(),
            Self::String(node) => node.syntax(),
            Self::Encapsed(node) => node.syntax(),
            Self::Heredoc(node) => node.syntax(),
        }
    }
}

/// Type syntax view.
#[derive(Clone, Copy, Debug)]
pub enum TypeView<'tree> {
    /// Generic type syntax.
    Type(TypeNode<'tree>),
    /// Union type.
    Union(UnionType<'tree>),
    /// Intersection type.
    Intersection(IntersectionType<'tree>),
    /// Nullable type.
    Nullable(NullableType<'tree>),
    /// DNF type.
    Dnf(DnfType<'tree>),
}

impl<'tree> TypeView<'tree> {
    /// Attempts to cast a CST node to a type syntax view.
    #[must_use]
    pub fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        TypeNode::cast(node)
            .map(Self::Type)
            .or_else(|| UnionType::cast(node).map(Self::Union))
            .or_else(|| IntersectionType::cast(node).map(Self::Intersection))
            .or_else(|| NullableType::cast(node).map(Self::Nullable))
            .or_else(|| DnfType::cast(node).map(Self::Dnf))
    }

    /// Returns the wrapped CST node.
    #[must_use]
    pub fn syntax(&self) -> &'tree SyntaxNode {
        match self {
            Self::Type(node) => node.syntax(),
            Self::Union(node) => node.syntax(),
            Self::Intersection(node) => node.syntax(),
            Self::Nullable(node) => node.syntax(),
            Self::Dnf(node) => node.syntax(),
        }
    }
}

impl<'tree> Decl<'tree> {
    /// Attempts to cast a CST node to a declaration view.
    #[must_use]
    pub fn cast(node: &'tree SyntaxNode) -> Option<Self> {
        NamespaceDecl::cast(node)
            .map(Self::Namespace)
            .or_else(|| UseDecl::cast(node).map(Self::Use))
            .or_else(|| ConstDecl::cast(node).map(Self::Const))
            .or_else(|| FunctionDecl::cast(node).map(Self::Function))
            .or_else(|| ClassLikeDecl::cast(node).map(Self::ClassLike))
    }

    /// Returns the wrapped CST node.
    #[must_use]
    pub fn syntax(&self) -> &'tree SyntaxNode {
        match self {
            Self::Namespace(node) => node.syntax(),
            Self::Use(node) => node.syntax(),
            Self::Const(node) => node.syntax(),
            Self::Function(node) => node.syntax(),
            Self::ClassLike(node) => node.syntax(),
        }
    }
}

/// Untyped token view for callers that need stable token helpers before a
/// narrower token family exists.
#[derive(Clone, Copy, Debug)]
pub struct TokenView<'tree> {
    syntax: &'tree SyntaxToken,
}

impl<'tree> AstToken<'tree> for TokenView<'tree> {
    fn cast(token: &'tree SyntaxToken) -> Option<Self> {
        Self::can_cast(*token.kind()).then_some(Self { syntax: token })
    }

    fn can_cast(kind: SyntaxKind) -> bool {
        kind.is_token()
    }

    fn syntax(&self) -> &'tree SyntaxToken {
        self.syntax
    }
}

/// Returns typed direct child nodes.
pub fn child_nodes<'tree, N>(node: &'tree SyntaxNode) -> impl Iterator<Item = N> + 'tree
where
    N: AstNode<'tree> + 'tree,
{
    AstChildren::new(node)
}

/// Returns the first typed direct child node.
#[must_use]
pub fn child_node<'tree, N>(node: &'tree SyntaxNode) -> Option<N>
where
    N: AstNode<'tree> + 'tree,
{
    child_nodes(node).next()
}

/// Returns typed direct child tokens.
pub fn child_tokens<'tree, T>(node: &'tree SyntaxNode) -> impl Iterator<Item = T> + 'tree
where
    T: AstToken<'tree> + 'tree,
{
    node.children().iter().filter_map(|child| match child {
        SyntaxElement::Node(_) => None,
        SyntaxElement::Token(token) => T::cast(token),
    })
}

/// Returns all direct CST child nodes without requiring a typed view.
pub fn syntax_child_nodes(node: &SyntaxNode) -> impl Iterator<Item = &SyntaxNode> {
    node.children().iter().filter_map(|child| match child {
        SyntaxElement::Node(node) => Some(node),
        SyntaxElement::Token(_) => None,
    })
}

fn direct_expr_nodes<'tree>(node: &'tree SyntaxNode) -> Vec<ExprNode<'tree>> {
    syntax_child_nodes(node)
        .filter_map(ExprNode::cast)
        .collect()
}

fn direct_expr_list_items<'tree>(node: &'tree SyntaxNode) -> Vec<ExprListItem<'tree>> {
    syntax_child_nodes(node)
        .filter_map(|child| {
            ExprNode::cast(child)
                .map(ExprListItem::Expression)
                .or_else(|| (child.kind().name() == "ERROR").then_some(ExprListItem::Error(child)))
        })
        .collect()
}

fn direct_comma_ranges(node: &SyntaxNode) -> Vec<TextRange> {
    let mut ranges = Vec::new();
    for child in node.children() {
        match child {
            SyntaxElement::Token(token) if token.text() == "," => {
                ranges.push(token.text_range());
            }
            SyntaxElement::Node(child) if child.kind().name() == "ERROR" => {
                ranges.extend(
                    syntax_child_tokens(child)
                        .filter(|token| token.text() == ",")
                        .map(SyntaxToken::text_range),
                );
            }
            _ => {}
        }
    }
    ranges
}

/// Returns all direct CST child tokens without requiring a typed view.
pub fn syntax_child_tokens(node: &SyntaxNode) -> impl Iterator<Item = &SyntaxToken> {
    node.children().iter().filter_map(|child| match child {
        SyntaxElement::Node(_) => None,
        SyntaxElement::Token(token) => Some(token),
    })
}

/// Returns typed descendant nodes in source order.
pub fn descendant_nodes<'tree, N>(node: &'tree SyntaxNode) -> impl Iterator<Item = N> + 'tree
where
    N: AstNode<'tree> + 'tree,
{
    let mut out = Vec::new();
    collect_descendant_nodes(node, &mut out);
    out.into_iter()
}

fn collect_descendant_nodes<'tree, N>(node: &'tree SyntaxNode, out: &mut Vec<N>)
where
    N: AstNode<'tree> + 'tree,
{
    for child in node.children() {
        if let SyntaxElement::Node(child_node) = child {
            if let Some(view) = N::cast(child_node) {
                out.push(view);
            }
            collect_descendant_nodes(child_node, out);
        }
    }
}

/// Returns typed descendant tokens in source order.
pub fn descendant_tokens<'tree, T>(node: &'tree SyntaxNode) -> impl Iterator<Item = T> + 'tree
where
    T: AstToken<'tree> + 'tree,
{
    let mut out = Vec::new();
    collect_descendant_tokens(node, &mut out);
    out.into_iter()
}

fn collect_descendant_tokens<'tree, T>(node: &'tree SyntaxNode, out: &mut Vec<T>)
where
    T: AstToken<'tree> + 'tree,
{
    for child in node.children() {
        match child {
            SyntaxElement::Node(child_node) => collect_descendant_tokens(child_node, out),
            SyntaxElement::Token(token) => {
                if let Some(view) = T::cast(token) {
                    out.push(view);
                }
            }
        }
    }
}

/// Returns the text of the first descendant token whose display name matches.
#[must_use]
pub fn token_text_by_name<'tree>(node: &'tree SyntaxNode, token_name: &str) -> Option<&'tree str> {
    descendant_tokens::<TokenView<'tree>>(node)
        .find(|token| token.kind().name() == token_name)
        .map(|token| token.text())
}

/// Returns raw modifier tokens that are direct children of a declaration node.
pub fn modifier_tokens(node: &SyntaxNode) -> impl Iterator<Item = TokenView<'_>> {
    child_tokens::<TokenView<'_>>(node).filter(|token| is_modifier_token_name(&token.kind().name()))
}

/// Returns direct attribute-list views attached to a declaration node.
pub fn attribute_lists<'tree>(
    node: &'tree SyntaxNode,
) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
    child_nodes::<AttributeList<'tree>>(node)
}

fn has_direct_token_name(node: &SyntaxNode, token_name: &str) -> bool {
    syntax_child_tokens(node).any(|token| token.kind().name() == token_name)
}

fn is_modifier_token_name(token_name: &str) -> bool {
    matches!(
        token_name,
        "T_ABSTRACT"
            | "T_FINAL"
            | "T_READONLY"
            | "T_PUBLIC"
            | "T_PROTECTED"
            | "T_PRIVATE"
            | "T_PUBLIC_SET"
            | "T_PROTECTED_SET"
            | "T_PRIVATE_SET"
            | "T_STATIC"
            | "T_VAR"
    )
}

fn first_direct_token_text_by_name<'tree>(
    node: &'tree SyntaxNode,
    token_name: &str,
) -> Option<&'tree str> {
    syntax_child_tokens(node)
        .find(|token| token.kind().name() == token_name)
        .map(SyntaxToken::text)
}

fn name_text_after_token<'tree>(
    node: &'tree SyntaxNode,
    marker_token_name: &str,
) -> Option<&'tree str> {
    let mut after_marker = false;
    for token in syntax_child_tokens(node) {
        let name = token.kind().name();
        if name == marker_token_name {
            after_marker = true;
            continue;
        }
        if after_marker && name == "T_STRING" {
            return Some(token.text());
        }
    }
    None
}

impl<'tree> SourceFile<'tree> {
    /// Returns declarations in source order.
    pub fn declarations(&self) -> impl Iterator<Item = Decl<'tree>> + 'tree {
        let mut out = Vec::new();
        collect_declarations(self.syntax, &mut out);
        out.into_iter()
    }

    /// Returns direct namespace declarations.
    pub fn namespaces(&self) -> impl Iterator<Item = NamespaceDecl<'tree>> + 'tree {
        descendant_nodes(self.syntax)
    }

    /// Returns direct function declarations.
    pub fn functions(&self) -> impl Iterator<Item = FunctionDecl<'tree>> + 'tree {
        descendant_nodes(self.syntax)
    }

    /// Returns class-like declarations in source order.
    pub fn class_likes(&self) -> impl Iterator<Item = ClassLikeDecl<'tree>> + 'tree {
        let mut out = Vec::new();
        collect_class_likes(self.syntax, &mut out);
        out.into_iter()
    }
}

impl<'tree> NamespaceDecl<'tree> {
    /// Returns the namespace name, if present.
    #[must_use]
    pub fn name(&self) -> Option<Name<'tree>> {
        child_node(self.syntax)
    }
}

impl<'tree> UseDecl<'tree> {
    /// Returns true when this import uses grouped `{ ... }` syntax.
    #[must_use]
    pub fn is_grouped(&self) -> bool {
        has_direct_token_name(self.syntax, "{")
    }

    /// Returns use-item views. The current CST has one `USE_DECL` node per
    /// import statement, so this iterator yields the declaration itself.
    pub fn items(&self) -> impl Iterator<Item = UseItem<'tree>> + 'tree {
        std::iter::once(*self)
    }

    /// Returns name nodes inside the use declaration in source order.
    pub fn names(&self) -> impl Iterator<Item = Name<'tree>> + 'tree {
        child_nodes(self.syntax)
    }
}

impl<'tree> ConstDecl<'tree> {
    /// Returns modifier tokens as raw AST data.
    pub fn modifier_tokens(&self) -> impl Iterator<Item = TokenView<'tree>> + 'tree {
        modifier_tokens(self.syntax)
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }
}

fn collect_declarations<'tree>(node: &'tree SyntaxNode, out: &mut Vec<Decl<'tree>>) {
    for child in node.children() {
        if let SyntaxElement::Node(child_node) = child {
            if let Some(decl) = Decl::cast(child_node) {
                out.push(decl);
            }
            collect_declarations(child_node, out);
        }
    }
}

fn collect_class_likes<'tree>(node: &'tree SyntaxNode, out: &mut Vec<ClassLikeDecl<'tree>>) {
    for child in node.children() {
        if let SyntaxElement::Node(child_node) = child {
            if let Some(class_like) = ClassLikeDecl::cast(child_node) {
                out.push(class_like);
            }
            collect_class_likes(child_node, out);
        }
    }
}

impl<'tree> FunctionDecl<'tree> {
    /// Returns the function name node, if present.
    #[must_use]
    pub fn name(&self) -> Option<Name<'tree>> {
        child_node(self.syntax)
    }

    /// Returns the raw function identifier token text, if present.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        name_text_after_token(self.syntax, "T_FUNCTION")
    }

    /// Returns the function parameter list.
    #[must_use]
    pub fn parameter_list(&self) -> Option<ParameterList<'tree>> {
        child_node(self.syntax)
    }

    /// Returns function parameters.
    pub fn parameters(&self) -> impl Iterator<Item = Param<'tree>> + 'tree {
        let mut out = Vec::new();
        if let Some(parameter_list) = self.parameter_list() {
            out.extend(child_nodes::<Param<'tree>>(parameter_list.syntax()));
        }
        out.into_iter()
    }

    /// Returns the declared return type syntax, if present.
    #[must_use]
    pub fn return_type(&self) -> Option<TypeView<'tree>> {
        syntax_child_nodes(self.syntax).find_map(TypeView::cast)
    }

    /// Returns the function body block, if present.
    #[must_use]
    pub fn body(&self) -> Option<BlockStmt<'tree>> {
        child_node(self.syntax)
    }

    /// Returns attribute groups attached inside this declaration node.
    pub fn attribute_groups(&self) -> impl Iterator<Item = AttributeGroup<'tree>> + 'tree {
        child_nodes(self.syntax)
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }

    /// Returns modifier tokens as raw AST data.
    pub fn modifier_tokens(&self) -> impl Iterator<Item = TokenView<'tree>> + 'tree {
        modifier_tokens(self.syntax)
    }
}

impl<'tree> ClassDecl<'tree> {
    /// Returns true when this `CLASS_DECL` uses anonymous-class syntax.
    #[must_use]
    pub fn is_anonymous(&self) -> bool {
        let mut saw_class = false;
        for token in descendant_tokens::<TokenView<'tree>>(self.syntax) {
            match token.kind().name().as_str() {
                "T_WHITESPACE" | "T_COMMENT" | "T_DOC_COMMENT" => {}
                "T_CLASS" => saw_class = true,
                "T_STRING" if saw_class => return false,
                "(" | "T_EXTENDS" | "T_IMPLEMENTS" | "{" if saw_class => return true,
                _ if saw_class => return true,
                _ => {}
            }
        }
        false
    }

    /// Returns the class identifier token text for named classes.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        if self.is_anonymous() {
            return None;
        }
        name_text_after_token(self.syntax, "T_CLASS")
    }

    /// Returns the member list, if present.
    #[must_use]
    pub fn member_list(&self) -> Option<ClassMemberList<'tree>> {
        child_node(self.syntax)
    }

    /// Returns class members in source order.
    pub fn members(&self) -> impl Iterator<Item = MemberDecl<'tree>> + 'tree {
        self.member_list()
            .map(|members| members.members().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    /// Returns raw modifier tokens.
    pub fn modifier_tokens(&self) -> impl Iterator<Item = TokenView<'tree>> + 'tree {
        modifier_tokens(self.syntax)
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }

    /// Returns an extends-clause view, if present in the current CST.
    #[must_use]
    pub fn extends_clause(&self) -> Option<ExtendsClause<'tree>> {
        ExtendsClause::cast(self.syntax)
    }

    /// Returns an implements-clause view, if present in the current CST.
    #[must_use]
    pub fn implements_clause(&self) -> Option<ImplementsClause<'tree>> {
        ImplementsClause::cast(self.syntax)
    }
}

impl<'tree> InterfaceDecl<'tree> {
    /// Returns the interface identifier token text.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        name_text_after_token(self.syntax, "T_INTERFACE")
    }

    /// Returns the member list, if present.
    #[must_use]
    pub fn member_list(&self) -> Option<ClassMemberList<'tree>> {
        child_node(self.syntax)
    }

    /// Returns interface members in source order.
    pub fn members(&self) -> impl Iterator<Item = MemberDecl<'tree>> + 'tree {
        self.member_list()
            .map(|members| members.members().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }

    /// Returns an extends-clause view, if present in the current CST.
    #[must_use]
    pub fn extends_clause(&self) -> Option<ExtendsClause<'tree>> {
        ExtendsClause::cast(self.syntax)
    }
}

impl<'tree> TraitDecl<'tree> {
    /// Returns the trait identifier token text.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        name_text_after_token(self.syntax, "T_TRAIT")
    }

    /// Returns the member list, if present.
    #[must_use]
    pub fn member_list(&self) -> Option<ClassMemberList<'tree>> {
        child_node(self.syntax)
    }

    /// Returns trait members in source order.
    pub fn members(&self) -> impl Iterator<Item = MemberDecl<'tree>> + 'tree {
        self.member_list()
            .map(|members| members.members().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }
}

impl<'tree> EnumDecl<'tree> {
    /// Returns the enum identifier token text.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        name_text_after_token(self.syntax, "T_ENUM")
    }

    /// Returns the member list, if present.
    #[must_use]
    pub fn member_list(&self) -> Option<ClassMemberList<'tree>> {
        child_node(self.syntax)
    }

    /// Returns enum members in source order.
    pub fn members(&self) -> impl Iterator<Item = MemberDecl<'tree>> + 'tree {
        self.member_list()
            .map(|members| members.members().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    /// Returns enum cases represented by current parser property nodes.
    pub fn cases(&self) -> impl Iterator<Item = EnumCase<'tree>> + 'tree {
        self.member_list()
            .map(|members| {
                descendant_nodes::<EnumCase<'tree>>(members.syntax()).collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter()
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }

    /// Returns an implements-clause view, if present in the current CST.
    #[must_use]
    pub fn implements_clause(&self) -> Option<ImplementsClause<'tree>> {
        ImplementsClause::cast(self.syntax)
    }
}

impl<'tree> ClassMemberList<'tree> {
    /// Returns direct member declarations.
    pub fn members(&self) -> impl Iterator<Item = MemberDecl<'tree>> + 'tree {
        self.syntax
            .children()
            .iter()
            .filter_map(|child| match child {
                SyntaxElement::Node(node) => MemberDecl::cast(node),
                SyntaxElement::Token(_) => None,
            })
    }
}

impl<'tree> MethodDecl<'tree> {
    /// Returns the method identifier token text, if present.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        name_text_after_token(self.syntax, "T_FUNCTION")
    }

    /// Returns the method parameter list.
    #[must_use]
    pub fn parameter_list(&self) -> Option<ParameterList<'tree>> {
        child_node(self.syntax)
    }

    /// Returns method parameters.
    pub fn parameters(&self) -> impl Iterator<Item = Param<'tree>> + 'tree {
        let mut out = Vec::new();
        if let Some(parameter_list) = self.parameter_list() {
            out.extend(child_nodes::<Param<'tree>>(parameter_list.syntax()));
        }
        out.into_iter()
    }

    /// Returns the declared return type syntax, if present.
    #[must_use]
    pub fn return_type(&self) -> Option<TypeView<'tree>> {
        syntax_child_nodes(self.syntax).find_map(TypeView::cast)
    }

    /// Returns the method body block, if present.
    #[must_use]
    pub fn body(&self) -> Option<BlockStmt<'tree>> {
        child_node(self.syntax)
    }

    /// Returns raw modifier tokens.
    pub fn modifier_tokens(&self) -> impl Iterator<Item = TokenView<'tree>> + 'tree {
        modifier_tokens(self.syntax)
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }
}

impl<'tree> PropertyDecl<'tree> {
    /// Returns true when this property-shaped node represents an enum case.
    #[must_use]
    pub fn is_enum_case(&self) -> bool {
        has_direct_token_name(self.syntax, "T_CASE")
    }

    /// Returns the first property variable token text, if present.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        first_direct_token_text_by_name(self.syntax, "T_VARIABLE")
            .or_else(|| name_text_after_token(self.syntax, "T_CASE"))
    }

    /// Returns raw modifier tokens.
    pub fn modifier_tokens(&self) -> impl Iterator<Item = TokenView<'tree>> + 'tree {
        modifier_tokens(self.syntax)
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }
}

impl<'tree> ClassConstDecl<'tree> {
    /// Returns the first constant identifier token text after `const`, if present.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        let mut after_const = false;
        let mut after_type = false;
        for token in syntax_child_tokens(self.syntax) {
            let name = token.kind().name();
            if name == "T_CONST" {
                after_const = true;
                continue;
            }
            if after_const && name == "T_STRING" && !after_type {
                after_type = true;
                continue;
            }
            if after_const && name == "T_STRING" {
                return Some(token.text());
            }
        }
        name_text_after_token(self.syntax, "T_CONST")
    }

    /// Returns raw modifier tokens.
    pub fn modifier_tokens(&self) -> impl Iterator<Item = TokenView<'tree>> + 'tree {
        modifier_tokens(self.syntax)
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }
}

impl<'tree> Param<'tree> {
    /// Returns the parameter variable token text, if present.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        first_direct_token_text_by_name(self.syntax, "T_VARIABLE")
    }

    /// Returns attached attribute lists.
    pub fn attribute_lists(&self) -> impl Iterator<Item = AttributeList<'tree>> + 'tree {
        attribute_lists(self.syntax)
    }
}

impl<'tree> EnumCase<'tree> {
    /// Returns the enum case identifier token text, if present.
    #[must_use]
    pub fn name_text(&self) -> Option<&'tree str> {
        name_text_after_token(self.syntax, "T_CASE")
    }
}

impl<'tree> TraitUseDecl<'tree> {
    /// Returns trait names used by this declaration.
    pub fn names(&self) -> impl Iterator<Item = Name<'tree>> + 'tree {
        child_nodes(self.syntax)
    }

    /// Returns true when the trait use has an adaptation block.
    #[must_use]
    pub fn has_adaptation_block(&self) -> bool {
        syntax_child_tokens(self.syntax).any(|token| token.text() == "{")
    }

    /// Returns a trait-adaptation view when an adaptation block is present.
    #[must_use]
    pub fn adaptation(&self) -> Option<TraitAdaptation<'tree>> {
        TraitAdaptation::cast(self.syntax)
    }

    /// Returns significant token text from the adaptation block. The current
    /// CST does not expose separate adaptation nodes.
    pub fn adaptation_token_texts(&self) -> impl Iterator<Item = &'tree str> + 'tree {
        let mut in_block = false;
        let mut out = Vec::new();
        for token in syntax_child_tokens(self.syntax) {
            if token.text() == "{" {
                in_block = true;
                continue;
            }
            if token.text() == "}" {
                break;
            }
            if in_block && !token.kind().is_trivia() {
                out.push(token.text());
            }
        }
        out.into_iter()
    }
}

impl<'tree> TraitAdaptation<'tree> {
    /// Returns significant token text from the adaptation block.
    pub fn token_texts(&self) -> impl Iterator<Item = &'tree str> + 'tree {
        TraitUseDecl::cast(self.syntax)
            .map(|trait_use| trait_use.adaptation_token_texts().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }
}

impl<'tree> PrefixExpr<'tree> {
    /// Returns the cast operator when this prefix expression is a cast.
    #[must_use]
    pub fn cast_kind(&self) -> Option<CastKind> {
        cast_kind_from_first_token(self.syntax)
    }
}

impl<'tree> VoidCastExpr<'tree> {
    /// Returns the PHP 8.5 void-cast operator.
    #[must_use]
    pub const fn cast_kind(&self) -> CastKind {
        CastKind::Void
    }
}

impl<'tree> BinaryExpr<'tree> {
    /// Returns true when this binary expression uses `??`.
    #[must_use]
    pub fn is_coalesce(&self) -> bool {
        descendant_tokens::<TokenView<'tree>>(self.syntax)
            .any(|token| token.kind().name() == "T_COALESCE")
    }
}

impl<'tree> ArrayExpr<'tree> {
    /// Returns true when this array expression uses `list(...)` syntax.
    #[must_use]
    pub fn is_list_syntax(&self) -> bool {
        first_significant_token(self.syntax).is_some_and(|token| token.kind().name() == "T_LIST")
    }
}

impl<'tree> CallExpr<'tree> {
    /// Returns call argument expressions in source order.
    pub fn arguments(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }

    /// Returns true for first-class callable syntax such as `strlen(...)`.
    #[must_use]
    pub fn is_first_class_callable(&self) -> bool {
        descendant_tokens::<TokenView<'tree>>(self.syntax)
            .any(|token| token.kind().name() == "T_ELLIPSIS")
    }
}

impl<'tree> Variable<'tree> {
    /// Returns the number of direct variable-variable sigils.
    #[must_use]
    pub fn sigil_count(&self) -> usize {
        syntax_child_tokens(self.syntax)
            .filter(|token| !token.kind().is_trivia())
            .map(|token| token.text().chars().take_while(|ch| *ch == '$').count())
            .sum()
    }

    /// Returns the nested expression for a dynamic variable, when present.
    #[must_use]
    pub fn dynamic_expression(&self) -> Option<ExprNode<'tree>> {
        direct_expr_nodes(self.syntax).into_iter().next()
    }
}

impl<'tree> ArrayDimFetchExpr<'tree> {
    /// Returns the receiver expression followed by the optional dimension.
    pub fn expressions(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }
}

impl<'tree> PropertyFetchExpr<'tree> {
    /// Returns the receiver and property expressions in source order.
    pub fn expressions(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }

    /// Returns true when this fetch uses the nullsafe `?->` operator.
    #[must_use]
    pub fn is_nullsafe(&self) -> bool {
        descendant_tokens::<TokenView<'tree>>(self.syntax)
            .any(|token| token.kind().name() == "T_NULLSAFE_OBJECT_OPERATOR")
    }
}

impl<'tree> StaticAccessExpr<'tree> {
    /// Returns the target and member expressions in source order.
    pub fn expressions(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }
}

impl<'tree> ConstructExpr<'tree> {
    /// Returns the built-in construct represented by this expression.
    #[must_use]
    pub fn construct_kind(&self) -> Option<ConstructKind> {
        let token = first_significant_token(self.syntax)?;
        match token.kind().name().as_str() {
            "T_INCLUDE" => Some(ConstructKind::Include),
            "T_INCLUDE_ONCE" => Some(ConstructKind::IncludeOnce),
            "T_REQUIRE" => Some(ConstructKind::Require),
            "T_REQUIRE_ONCE" => Some(ConstructKind::RequireOnce),
            "T_PRINT" => Some(ConstructKind::Print),
            "T_ISSET" => Some(ConstructKind::Isset),
            "T_EMPTY" => Some(ConstructKind::Empty),
            "T_EVAL" => Some(ConstructKind::Eval),
            "T_EXIT" => Some(ConstructKind::Exit),
            _ => None,
        }
    }

    /// Returns construct operand expressions in source order.
    pub fn operands(&self) -> impl Iterator<Item = ExprNode<'tree>> + 'tree {
        direct_expr_nodes(self.syntax).into_iter()
    }

    /// Returns parsed and recovered operand entries in source order.
    pub fn expression_items(&self) -> impl Iterator<Item = ExprListItem<'tree>> + 'tree {
        direct_expr_list_items(self.syntax).into_iter()
    }

    /// Returns the byte ranges of direct comma separators.
    pub fn comma_ranges(&self) -> impl Iterator<Item = TextRange> + 'tree {
        direct_comma_ranges(self.syntax).into_iter()
    }
}

impl<'tree> YieldExpr<'tree> {
    /// Returns true when this is `yield from`.
    #[must_use]
    pub fn is_yield_from(&self) -> bool {
        first_significant_token(self.syntax)
            .is_some_and(|token| token.kind().name() == "T_YIELD_FROM")
    }
}

impl<'tree> TypeNode<'tree> {
    /// Returns the leading keyword type atom, when this type is keyword-shaped.
    #[must_use]
    pub fn keyword(&self) -> Option<TypeKeyword> {
        type_keyword_from_first_token(self.syntax)
    }
}

impl<'tree> TypeView<'tree> {
    /// Returns a keyword type atom for simple keyword-shaped type syntax.
    #[must_use]
    pub fn keyword(&self) -> Option<TypeKeyword> {
        type_keyword_from_first_token(self.syntax())
    }
}

fn cast_kind_from_first_token(node: &SyntaxNode) -> Option<CastKind> {
    let token = first_significant_token(node)?;
    match token.kind().name().as_str() {
        "T_INT_CAST" => Some(CastKind::Int),
        "T_DOUBLE_CAST" => Some(CastKind::Float),
        "T_STRING_CAST" => Some(CastKind::String),
        "T_ARRAY_CAST" => Some(CastKind::Array),
        "T_OBJECT_CAST" => Some(CastKind::Object),
        "T_BOOL_CAST" => Some(CastKind::Bool),
        "T_UNSET_CAST" => Some(CastKind::Unset),
        "T_VOID_CAST" => Some(CastKind::Void),
        _ => None,
    }
}

fn type_keyword_from_first_token(node: &SyntaxNode) -> Option<TypeKeyword> {
    let token = first_significant_token(node)?;
    match token.text().to_ascii_lowercase().as_str() {
        "void" => Some(TypeKeyword::Void),
        "never" => Some(TypeKeyword::Never),
        "static" => Some(TypeKeyword::Static),
        "self" => Some(TypeKeyword::Self_),
        "parent" => Some(TypeKeyword::Parent),
        "false" => Some(TypeKeyword::False),
        "true" => Some(TypeKeyword::True),
        "null" => Some(TypeKeyword::Null),
        "mixed" => Some(TypeKeyword::Mixed),
        "iterable" => Some(TypeKeyword::Iterable),
        "object" => Some(TypeKeyword::Object),
        "callable" => Some(TypeKeyword::Callable),
        "array" => Some(TypeKeyword::Array),
        _ => None,
    }
}

fn first_significant_token<'tree>(node: &'tree SyntaxNode) -> Option<TokenView<'tree>> {
    descendant_tokens::<TokenView<'tree>>(node).find(|token| !token.kind().is_trivia())
}

/// Returns the root source-file view for a parsed CST.
#[must_use]
pub fn source_file(root: &SyntaxNode) -> Option<SourceFile<'_>> {
    SourceFile::new(root)
}

#[cfg(test)]
mod tests {
    use super::{
        AnonymousClassDecl, ArrayExpr, AstChildren, AstNode, AstToken, BinaryExpr, CallExpr,
        CastKind, ClassDecl, ClassLikeDecl::Class, ClassLikeDecl::Trait, CloneExpr, CloneWithExpr,
        ConstructExpr, ConstructKind, Decl, DnfType, EnumCase, EnumDecl, ExprListItem, ExprNode,
        FunctionDecl, InlineHtmlStmt, InterfaceDecl, IntersectionType, MatchExpr, MemberDecl, Name,
        Parameter, PipeExpr, PropertyFetchExpr, SourceAstId, SourceFile, StatementList, Stmt,
        SyntaxNodeExt, TokenView, TraitUseDecl, TypeKeyword, TypeNode, TypeView, UseDecl, UseGroup,
        VoidCastExpr, descendant_nodes, descendant_tokens, source_file, syntax_child_nodes,
        token_text_by_name,
    };
    use php_syntax::{SyntaxNodeKind, parse_source_file};

    #[test]
    fn source_file_wraps_parser_root() {
        let parse = parse_source_file("<?php echo 1;\n");
        let root = SourceFile::new(parse.root()).expect("root should cast");

        assert_eq!(
            root.syntax().kind().name(),
            SyntaxNodeKind::SourceFile.name()
        );
        assert!(root.text_range().end().to_usize() > 0);
    }

    #[test]
    fn typed_children_find_declarations() {
        let parse = parse_source_file("<?php function f() {} class C {}\n");
        let root = source_file(parse.root()).expect("source file");
        let functions: Vec<_> = descendant_nodes::<FunctionDecl<'_>>(root.syntax()).collect();
        let classes: Vec<_> = root
            .class_likes()
            .filter(|class_like| matches!(class_like, Class(_)))
            .collect();

        assert_eq!(functions.len(), 1);
        assert_eq!(classes.len(), 1);
        assert_eq!(functions[0].kind().name(), "FUNCTION_DECL");
        assert_eq!(classes[0].kind().name(), "CLASS_DECL");
    }

    #[test]
    fn token_helpers_expose_direct_tokens() {
        let parse = parse_source_file("<?php function f() {}\n");
        let root = source_file(parse.root()).expect("source file");
        let tokens: Vec<_> = descendant_tokens::<TokenView<'_>>(root.syntax()).collect();

        assert!(tokens.iter().any(|token| token.text() == "<?php "));
        assert_eq!(
            token_text_by_name(root.syntax(), "T_OPEN_TAG"),
            Some("<?php ")
        );
    }

    #[test]
    fn source_ids_and_ast_ptrs_are_source_local() {
        let parse = parse_source_file("<?php echo 1;\n");
        let root = source_file(parse.root()).expect("source file");
        let id = SourceAstId::new(7);
        let ptr = root.ast_ptr();

        assert_eq!(id.ordinal(), 7);
        assert_eq!(ptr.kind().name(), "SOURCE_FILE");
        assert_eq!(ptr.text_range(), root.text_range());
    }

    #[test]
    fn function_views_keep_source_spans() {
        let parse = parse_source_file("<?php function f(int $x): string { return \"x\"; }");
        assert!(!parse.has_errors());
        let root = source_file(parse.root()).expect("source file");
        let statement_lists: Vec<StatementList<'_>> = descendant_nodes(root.syntax()).collect();
        let functions: Vec<FunctionDecl<'_>> = descendant_nodes(root.syntax()).collect();

        assert_eq!(statement_lists.len(), 1);
        assert_eq!(functions.len(), 1);

        let function = functions[0];
        let parameter_list = function.parameter_list().expect("parameter list view");
        let _: AstChildren<'_, Parameter<'_>> = parameter_list.syntax().ast_children();
        let params: Vec<Parameter<'_>> = function.parameters().collect();
        let return_type = function.return_type().expect("return type view");
        let body = function.body().expect("function body block");
        let names: Vec<Name<'_>> = descendant_nodes(function.syntax()).collect();
        let param_type = descendant_nodes::<TypeNode<'_>>(params[0].syntax())
            .next()
            .expect("parameter type view");

        assert_eq!(params.len(), 1);
        assert!(param_type.text_range().start() < param_type.text_range().end());
        assert!(matches!(return_type, TypeView::Type(_)));
        assert!(
            return_type.syntax().text_range().start() < return_type.syntax().text_range().end()
        );
        assert!(body.text_range().start() < body.text_range().end());
        assert!(function.text_range().start() <= return_type.syntax().text_range().start());
        assert!(body.text_range().end() <= function.text_range().end());
        assert_eq!(function.ast_ptr().text_range(), function.text_range());
        assert!(names.len() >= 2);
    }

    #[test]
    fn source_file_declarations_cover_decl_families() {
        let parse = parse_source_file(
            "<?php namespace A; use B\\C; const X = 1; function f() {} trait T {} class C {}\n",
        );
        let root = source_file(parse.root()).expect("source file");
        let declarations: Vec<_> = root.declarations().collect();

        assert!(
            declarations
                .iter()
                .any(|decl| matches!(decl, Decl::Namespace(_)))
        );
        assert!(declarations.iter().any(|decl| matches!(decl, Decl::Use(_))));
        assert!(
            declarations
                .iter()
                .any(|decl| matches!(decl, Decl::Const(_)))
        );
        assert!(
            declarations
                .iter()
                .any(|decl| matches!(decl, Decl::Function(_)))
        );
        assert!(
            declarations
                .iter()
                .any(|decl| matches!(decl, Decl::ClassLike(Trait(_))))
        );
        assert!(
            declarations
                .iter()
                .any(|decl| matches!(decl, Decl::ClassLike(Class(_))))
        );
    }

    #[test]
    fn class_like_members_are_structured_views() {
        let parse = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/class_members.php"
        ));
        let root = source_file(parse.root()).expect("source file");
        let class = root
            .class_likes()
            .find(|class_like| matches!(class_like, Class(_)))
            .expect("class declaration");
        let members: Vec<_> = class.members().collect();

        assert!(
            members
                .iter()
                .any(|member| matches!(member, MemberDecl::ClassConst(_)))
        );
        assert!(
            members
                .iter()
                .any(|member| matches!(member, MemberDecl::Property(_)))
        );
        assert!(
            members
                .iter()
                .any(|member| matches!(member, MemberDecl::TraitUse(_)))
        );
        assert!(
            members
                .iter()
                .any(|member| matches!(member, MemberDecl::Method(_)))
        );

        let trait_use = members
            .iter()
            .find_map(|member| match member {
                MemberDecl::TraitUse(trait_use) => Some(*trait_use),
                _ => None,
            })
            .expect("trait use");
        let adaptation_tokens: Vec<_> = trait_use.adaptation_token_texts().collect();
        assert!(trait_use.has_adaptation_block());
        assert!(adaptation_tokens.contains(&"insteadof"));
        assert!(adaptation_tokens.contains(&"as"));
    }

    #[test]
    fn declaration_and_member_views_cover_fixtures() {
        let uses = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/use_declarations.php"
        ));
        assert!(!uses.has_errors());
        let uses_root = source_file(uses.root()).expect("source file");
        let use_decls: Vec<UseDecl<'_>> = descendant_nodes(uses_root.syntax()).collect();
        let use_groups: Vec<UseGroup<'_>> = descendant_nodes(uses_root.syntax()).collect();
        assert_eq!(use_decls.len(), 5);
        assert_eq!(use_groups.len(), 2);
        assert!(
            use_decls
                .iter()
                .all(|use_decl| use_decl.items().count() == 1)
        );
        assert!(
            use_decls
                .iter()
                .map(|use_decl| use_decl.names().count())
                .sum::<usize>()
                >= 7
        );

        let interfaces_traits = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/interfaces_traits.php"
        ));
        assert!(!interfaces_traits.has_errors());
        let it_root = source_file(interfaces_traits.root()).expect("source file");
        let interface = descendant_nodes::<InterfaceDecl<'_>>(it_root.syntax())
            .find(|interface| interface.name_text() == Some("Reader"))
            .expect("interface");
        let trait_user = descendant_nodes::<ClassDecl<'_>>(it_root.syntax())
            .find(|class| class.name_text() == Some("UsesTrait"))
            .expect("class using trait");
        assert!(interface.extends_clause().is_some());
        assert!(
            interface
                .members()
                .any(|member| matches!(member, MemberDecl::Method(_)))
        );
        assert!(
            trait_user
                .members()
                .any(|member| matches!(member, MemberDecl::TraitUse(_)))
        );

        let class_members = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/class_members.php"
        ));
        assert!(!class_members.has_errors());
        let members_root = source_file(class_members.root()).expect("source file");
        let class = descendant_nodes::<ClassDecl<'_>>(members_root.syntax())
            .find(|class| class.name_text() == Some("MemberExamples"))
            .expect("member class");
        let members: Vec<_> = class.members().collect();
        assert!(
            class
                .modifier_tokens()
                .any(|token| token.kind().name() == "T_ABSTRACT")
        );
        assert!(
            members
                .iter()
                .any(|member| matches!(member, MemberDecl::ClassConst(_)))
        );
        assert!(
            members
                .iter()
                .any(|member| matches!(member, MemberDecl::Property(_)))
        );
        assert!(
            members
                .iter()
                .any(|member| matches!(member, MemberDecl::Method(_)))
        );
        let trait_use = descendant_nodes::<TraitUseDecl<'_>>(members_root.syntax())
            .find(|trait_use| trait_use.has_adaptation_block())
            .expect("trait adaptation");
        let adaptation = trait_use.adaptation().expect("adaptation view");
        let adaptation_tokens: Vec<_> = adaptation.token_texts().collect();
        assert!(adaptation_tokens.contains(&"insteadof"));
        assert!(adaptation_tokens.contains(&"as"));

        let enums = parse_source_file(include_str!("../../../fixtures/parser/valid/enums.php"));
        assert!(!enums.has_errors());
        let enum_root = source_file(enums.root()).expect("source file");
        let status = descendant_nodes::<EnumDecl<'_>>(enum_root.syntax())
            .find(|enum_decl| enum_decl.name_text() == Some("Status"))
            .expect("backed enum");
        let enum_cases: Vec<EnumCase<'_>> = descendant_nodes(enum_root.syntax()).collect();
        assert!(status.implements_clause().is_some());
        assert_eq!(status.cases().count(), 2);
        assert_eq!(enum_cases.len(), 4);
        assert!(
            enum_cases
                .iter()
                .any(|case| case.name_text() == Some("Draft"))
        );

        let anonymous = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/classes_basic.php"
        ));
        let anonymous_root = source_file(anonymous.root()).expect("source file");
        assert_eq!(
            descendant_nodes::<super::AnonymousClassExpr<'_>>(anonymous_root.syntax()).count(),
            1
        );

        let attributes = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/attributes.php"
        ));
        let attr_root = source_file(attributes.root()).expect("source file");
        assert!(
            descendant_nodes::<FunctionDecl<'_>>(attr_root.syntax())
                .any(|function| function.attribute_lists().count() > 0)
        );

        let broken = parse_source_file("<?php class Broken { public function ( }");
        let broken_root = source_file(broken.root()).expect("source file");
        let _classes: Vec<ClassDecl<'_>> = descendant_nodes(broken_root.syntax()).collect();
        assert!(broken.has_errors());
    }

    #[test]
    fn anonymous_classes_cast_separately_from_named_classes() {
        let parse = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/classes_basic.php"
        ));
        let root = source_file(parse.root()).expect("source file");
        let anonymous: Vec<_> = descendant_nodes::<AnonymousClassDecl<'_>>(root.syntax()).collect();
        let class_like_count = root.class_likes().count();

        assert_eq!(anonymous.len(), 1);
        assert!(
            anonymous[0].text_range().end().to_usize()
                > anonymous[0].text_range().start().to_usize()
        );
        assert!(class_like_count >= 3);
    }

    #[test]
    fn statement_views_cover_control_misc_and_inline_html() {
        let parse = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/control_flow.php"
        ));
        assert!(!parse.has_errors());
        let root = source_file(parse.root()).expect("source file");
        let statements = stmt_views(root.syntax());

        assert!(statements.iter().any(|stmt| matches!(stmt, Stmt::If(_))));
        assert!(statements.iter().any(|stmt| matches!(stmt, Stmt::While(_))));
        assert!(
            statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::DoWhile(_)))
        );
        assert!(statements.iter().any(|stmt| matches!(stmt, Stmt::For(_))));
        assert!(
            statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Foreach(_)))
        );
        assert!(
            statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Switch(_)))
        );
        assert!(
            statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Return(_)))
        );
        assert!(statements.iter().any(|stmt| matches!(stmt, Stmt::Throw(_))));
        assert!(statements.iter().any(|stmt| matches!(stmt, Stmt::Break(_))));
        assert!(
            statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Continue(_)))
        );

        let misc = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/statements_misc.php"
        ));
        assert!(!misc.has_errors());
        let misc_root = source_file(misc.root()).expect("source file");
        let misc_statements = stmt_views(misc_root.syntax());
        assert!(
            misc_statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Global(_)))
        );
        assert!(
            misc_statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Static(_)))
        );
        assert!(
            misc_statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Unset(_)))
        );
        assert!(
            misc_statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Goto(_)))
        );
        assert!(
            misc_statements
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Label(_)))
        );

        let inline = parse_source_file("<h1>Title</h1><?php echo 1; ?>tail");
        let inline_root = source_file(inline.root()).expect("source file");
        assert!(descendant_nodes::<InlineHtmlStmt<'_>>(inline_root.syntax()).count() >= 2);
        assert!(
            stmt_views(inline_root.syntax())
                .iter()
                .any(|stmt| matches!(stmt, Stmt::InlineHtml(_)))
        );
    }

    #[test]
    fn statement_and_construct_lists_expose_typed_expressions() {
        let parse = parse_source_file(
            "<?php global $first, $$dynamic; static $cached = 1, $empty; declare(ticks = 1); echo $first, $cached; unset($first, $cached); isset($first, $cached + load());",
        );
        assert!(!parse.has_errors());
        let root = source_file(parse.root()).expect("source file");
        let statements = stmt_views(root.syntax());

        let global = statements
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::Global(stmt) => Some(stmt),
                _ => None,
            })
            .expect("global statement");
        assert_eq!(global.variables().count(), 2);
        assert_eq!(global.expression_items().count(), 2);
        assert_eq!(global.comma_ranges().count(), 1);

        let static_locals = statements
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::Static(stmt) => Some(stmt),
                _ => None,
            })
            .expect("static statement");
        assert_eq!(static_locals.locals().count(), 2);
        assert_eq!(static_locals.comma_ranges().count(), 1);
        assert!(matches!(
            static_locals.locals().next(),
            Some(ExprNode::Assign(_))
        ));

        let declare = statements
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::Declare(stmt) => Some(stmt),
                _ => None,
            })
            .expect("declare statement");
        assert!(matches!(
            declare.directives().next(),
            Some(ExprNode::Assign(_))
        ));

        let echo = statements
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::Echo(stmt) => Some(stmt),
                _ => None,
            })
            .expect("echo statement");
        assert_eq!(echo.expressions().count(), 2);

        let unset = statements
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::Unset(stmt) => Some(stmt),
                _ => None,
            })
            .expect("unset statement");
        assert_eq!(unset.expressions().count(), 2);

        let isset = descendant_nodes::<ConstructExpr<'_>>(root.syntax())
            .find(|construct| construct.construct_kind() == Some(ConstructKind::Isset))
            .expect("isset construct");
        let operands = isset.operands().collect::<Vec<_>>();
        assert_eq!(operands.len(), 2);
        assert_eq!(isset.comma_ranges().count(), 1);
        assert!(matches!(operands[1], ExprNode::Binary(_)));
    }

    #[test]
    fn expression_lists_preserve_recovery_items_and_separator_spans() {
        let source = "<?php function f() { global /* one */ $first,  $$dynamic, , $last; }";
        let parse = parse_source_file(source);
        assert!(parse.has_errors());
        let root = source_file(parse.root()).expect("source file");
        let global = stmt_views(root.syntax())
            .into_iter()
            .find_map(|statement| match statement {
                Stmt::Global(global) => Some(global),
                _ => None,
            })
            .expect("global statement");
        let items = global.expression_items().collect::<Vec<_>>();

        assert_eq!(items.len(), 4);
        assert!(matches!(items[0], ExprListItem::Expression(_)));
        assert!(matches!(items[1], ExprListItem::Expression(_)));
        assert!(matches!(items[2], ExprListItem::Error(_)));
        assert!(matches!(items[3], ExprListItem::Expression(_)));
        assert_eq!(global.comma_ranges().count(), 3);
        let first_span = items[0].text_range();
        let error_span = items[2].text_range();
        assert_eq!(
            &source[first_span.start().to_usize()..first_span.end().to_usize()],
            "$first"
        );
        assert_eq!(
            &source[error_span.start().to_usize()..error_span.end().to_usize()],
            ","
        );
    }

    #[test]
    fn expression_views_cover_postfix_constructs_and_php85_nodes() {
        let postfix = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/expressions_postfix.php"
        ));
        assert!(!postfix.has_errors());
        let postfix_root = source_file(postfix.root()).expect("source file");
        assert!(descendant_nodes::<CallExpr<'_>>(postfix_root.syntax()).count() > 0);
        assert!(descendant_nodes::<PropertyFetchExpr<'_>>(postfix_root.syntax()).count() > 0);
        assert!(
            descendant_nodes::<PropertyFetchExpr<'_>>(postfix_root.syntax())
                .any(|fetch| fetch.is_nullsafe())
        );

        let constructs = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/statements_misc.php"
        ));
        let construct_root = source_file(constructs.root()).expect("source file");
        let construct_kinds: Vec<_> =
            descendant_nodes::<ConstructExpr<'_>>(construct_root.syntax())
                .filter_map(|expr| expr.construct_kind())
                .collect();
        assert!(construct_kinds.contains(&ConstructKind::Include));
        assert!(construct_kinds.contains(&ConstructKind::RequireOnce));
        assert!(construct_kinds.contains(&ConstructKind::Eval));
        assert!(construct_kinds.contains(&ConstructKind::Exit));

        let pipe = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/php85/pipe_operator.php"
        ));
        let pipe_root = source_file(pipe.root()).expect("source file");
        assert!(descendant_nodes::<PipeExpr<'_>>(pipe_root.syntax()).count() >= 2);
        assert!(
            descendant_nodes::<CallExpr<'_>>(pipe_root.syntax())
                .any(|call| call.is_first_class_callable())
        );

        let void_cast = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/php85/void_cast.php"
        ));
        let void_root = source_file(void_cast.root()).expect("source file");
        let void_casts: Vec<_> = descendant_nodes::<VoidCastExpr<'_>>(void_root.syntax()).collect();
        assert_eq!(void_casts.len(), 2);
        assert!(
            void_casts
                .iter()
                .all(|cast| cast.cast_kind() == CastKind::Void)
        );

        let clone_with = parse_source_file(include_str!(
            "../../../fixtures/parser/valid/php85/clone_with.php"
        ));
        let clone_root = source_file(clone_with.root()).expect("source file");
        assert_eq!(
            descendant_nodes::<CloneWithExpr<'_>>(clone_root.syntax()).count(),
            1
        );
        assert_eq!(
            descendant_nodes::<CloneExpr<'_>>(clone_root.syntax()).count(),
            1
        );
    }

    #[test]
    fn expression_views_cover_match_yield_coalesce_list_and_casts() {
        let parse = parse_source_file(
            "<?php
            $a = [1, 2];
            list($x, $y) = $a;
            $value = $a[0] ?? null;
            $value = (int) $value;
            $matched = match ($value) { 1 => 'one', default => 'many' };
            function gen($xs) { yield from $xs; }
            ",
        );
        assert!(!parse.has_errors());
        let root = source_file(parse.root()).expect("source file");

        assert!(
            descendant_nodes::<ArrayExpr<'_>>(root.syntax()).any(|array| array.is_list_syntax())
        );
        assert!(descendant_nodes::<BinaryExpr<'_>>(root.syntax()).any(|expr| expr.is_coalesce()));
        assert!(
            descendant_nodes::<super::PrefixExpr<'_>>(root.syntax())
                .any(|expr| expr.cast_kind() == Some(CastKind::Int))
        );
        assert!(descendant_nodes::<MatchExpr<'_>>(root.syntax()).count() >= 1);
        assert!(
            descendant_nodes::<super::YieldExpr<'_>>(root.syntax())
                .any(|expr| expr.is_yield_from())
        );

        let expression_views = expr_views(root.syntax());
        assert!(
            expression_views
                .iter()
                .any(|expr| matches!(expr, ExprNode::Array(_)))
        );
        assert!(
            expression_views
                .iter()
                .any(|expr| matches!(expr, ExprNode::Match(_)))
        );
    }

    #[test]
    fn type_views_cover_dnf_intersection_and_keyword_atoms() {
        let parse = parse_source_file(include_str!("../../../fixtures/parser/valid/dnf_types.php"));
        assert!(!parse.has_errors());
        let root = source_file(parse.root()).expect("source file");
        assert!(descendant_nodes::<DnfType<'_>>(root.syntax()).count() >= 3);
        assert!(descendant_nodes::<IntersectionType<'_>>(root.syntax()).count() >= 1);
        assert!(
            descendant_nodes::<DnfType<'_>>(root.syntax())
                .map(|ty| TypeView::cast(ty.syntax()).expect("type view"))
                .all(|ty| ty.keyword().is_none())
        );

        let keywords = parse_source_file(
            "<?php function f(callable $c, object $o, mixed $m): void {}
            function g(): static {}
            ",
        );
        assert!(!keywords.has_errors());
        let keyword_root = source_file(keywords.root()).expect("source file");
        let keyword_atoms: Vec<_> = descendant_nodes::<TypeNode<'_>>(keyword_root.syntax())
            .filter_map(|ty| ty.keyword())
            .collect();

        assert!(keyword_atoms.contains(&TypeKeyword::Callable));
        assert!(keyword_atoms.contains(&TypeKeyword::Object));
        assert!(keyword_atoms.contains(&TypeKeyword::Mixed));
        assert!(keyword_atoms.contains(&TypeKeyword::Void));
        assert!(keyword_atoms.contains(&TypeKeyword::Static));
    }

    #[test]
    fn expression_views_tolerate_broken_syntax() {
        let parse = parse_source_file("<?php $x = (1 + ; echo 2;");
        let root = source_file(parse.root()).expect("source file");
        let _views = expr_views(root.syntax());
        let _statements = stmt_views(root.syntax());
        assert!(parse.has_errors());
    }

    fn stmt_views<'tree>(node: &'tree php_syntax::SyntaxNode) -> Vec<Stmt<'tree>> {
        let mut out = Vec::new();
        collect_stmt_views(node, &mut out);
        out
    }

    fn collect_stmt_views<'tree>(node: &'tree php_syntax::SyntaxNode, out: &mut Vec<Stmt<'tree>>) {
        for child in syntax_child_nodes(node) {
            if let Some(statement) = Stmt::cast(child) {
                out.push(statement);
            }
            collect_stmt_views(child, out);
        }
    }

    fn expr_views<'tree>(node: &'tree php_syntax::SyntaxNode) -> Vec<ExprNode<'tree>> {
        let mut out = Vec::new();
        collect_expr_views(node, &mut out);
        out
    }

    fn collect_expr_views<'tree>(
        node: &'tree php_syntax::SyntaxNode,
        out: &mut Vec<ExprNode<'tree>>,
    ) {
        for child in syntax_child_nodes(node) {
            if let Some(expression) = ExprNode::cast(child) {
                out.push(expression);
            }
            collect_expr_views(child, out);
        }
    }
}
