use php_lexer::{SymbolKind, TokenKind, TokenName};

/// Unified CST kind wrapper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxKind {
    /// Token leaf kind.
    Token(SyntaxTokenKind),
    /// Node kind.
    Node(SyntaxNodeKind),
}

impl SyntaxKind {
    /// Root source-file node.
    pub const SOURCE_FILE: Self = Self::Node(SyntaxNodeKind::SourceFile);
    /// Error node.
    pub const ERROR: Self = Self::Node(SyntaxNodeKind::Error);

    /// Converts a lexer token kind to a syntax token kind.
    #[must_use]
    pub const fn from_token_kind(kind: TokenKind) -> Self {
        match kind {
            TokenKind::Named(name) => Self::Token(SyntaxTokenKind::Named(name)),
            TokenKind::Symbol(symbol) => Self::Token(SyntaxTokenKind::Symbol(symbol)),
            TokenKind::Eof => Self::Token(SyntaxTokenKind::Eof),
        }
    }

    /// Returns true for trivia token leaves.
    #[must_use]
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::Token(SyntaxTokenKind::Named(
                TokenName::Whitespace | TokenName::Comment | TokenName::DocComment
            ))
        )
    }

    /// Returns true for token leaves.
    #[must_use]
    pub const fn is_token(self) -> bool {
        matches!(self, Self::Token(_))
    }

    /// Returns true for nodes.
    #[must_use]
    pub const fn is_node(self) -> bool {
        matches!(self, Self::Node(_))
    }

    /// Returns a stable display name.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Token(token) => token.name(),
            Self::Node(node) => node.name().to_owned(),
        }
    }
}

/// Token categories in the CST.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxTokenKind {
    /// Named PHP lexer token.
    Named(TokenName),
    /// Single-byte PHP symbol.
    Symbol(SymbolKind),
    /// Synthetic EOF marker when requested from the lexer.
    Eof,
}

impl SyntaxTokenKind {
    /// Returns a stable display name.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Named(name) => name.as_php_name().to_owned(),
            Self::Symbol(symbol) => symbol.reference_name(),
            Self::Eof => "EOF".to_owned(),
        }
    }
}

/// Node categories in the CST.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxNodeKind {
    /// Complete file.
    SourceFile,
    /// PHP section between open and close tags.
    PhpBlock,
    /// Inline HTML section outside PHP mode.
    InlineHtml,
    /// Sequence of statements.
    StatementList,
    /// Error recovery node.
    Error,
    /// Empty statement.
    EmptyStmt,
    /// Expression statement.
    ExprStmt,
    /// Echo statement or short echo statement.
    EchoStmt,
    /// Return statement.
    ReturnStmt,
    /// Throw statement.
    ThrowStmt,
    /// Break statement.
    BreakStmt,
    /// Continue statement.
    ContinueStmt,
    /// Block statement.
    BlockStmt,
    /// If statement.
    IfStmt,
    /// While statement.
    WhileStmt,
    /// Do/while statement.
    DoWhileStmt,
    /// For statement.
    ForStmt,
    /// Foreach statement.
    ForeachStmt,
    /// Switch statement.
    SwitchStmt,
    /// Try statement.
    TryStmt,
    /// Catch clause.
    CatchClause,
    /// Finally clause.
    FinallyClause,
    /// Declare statement.
    DeclareStmt,
    /// Global statement.
    GlobalStmt,
    /// Static local-variable statement.
    StaticStmt,
    /// Unset statement.
    UnsetStmt,
    /// Goto statement.
    GotoStmt,
    /// Label statement.
    LabelStmt,
    /// Namespace statement.
    NamespaceStmt,
    /// Use declaration.
    UseDecl,
    /// Function declaration.
    FunctionDecl,
    /// Parameter list.
    ParamList,
    /// Parameter.
    Param,
    /// Class declaration.
    ClassDecl,
    /// Interface declaration.
    InterfaceDecl,
    /// Trait declaration.
    TraitDecl,
    /// Enum declaration.
    EnumDecl,
    /// Class member list.
    ClassMemberList,
    /// Method declaration.
    MethodDecl,
    /// Property declaration.
    PropertyDecl,
    /// Class constant declaration.
    ClassConstDecl,
    /// Trait use declaration.
    TraitUseDecl,
    /// Attribute group.
    AttributeGroup,
    /// Attribute.
    Attribute,
    /// Generic type node.
    Type,
    /// Union type.
    UnionType,
    /// Intersection type.
    IntersectionType,
    /// Nullable type.
    NullableType,
    /// Disjunctive normal form type.
    DnfType,
    /// Generic expression node.
    Expr,
    /// Literal expression.
    Literal,
    /// Name expression.
    Name,
    /// Variable expression.
    Variable,
    /// Parenthesized expression.
    ParenthesizedExpr,
    /// Prefix expression.
    PrefixExpr,
    /// PHP 8.5 void-cast expression.
    VoidCastExpr,
    /// Binary expression.
    BinaryExpr,
    /// Assignment expression.
    AssignExpr,
    /// Ternary or elvis expression.
    TernaryExpr,
    /// Call expression.
    CallExpr,
    /// Array or dimension fetch expression.
    ArrayDimFetchExpr,
    /// Object property or method fetch expression.
    PropertyFetchExpr,
    /// Static property, constant, or method access expression.
    StaticAccessExpr,
    /// Array expression.
    ArrayExpr,
    /// Array element or key/value pair.
    ArrayPair,
    /// Match expression.
    MatchExpr,
    /// Throw expression.
    ThrowExpr,
    /// Include/require/print/isset/empty/eval/exit construct expression.
    ConstructExpr,
    /// Yield expression.
    YieldExpr,
    /// Closure expression.
    ClosureExpr,
    /// Arrow function expression.
    ArrowFunctionExpr,
    /// New expression.
    NewExpr,
    /// Clone expression.
    CloneExpr,
    /// Clone-with expression.
    CloneWithExpr,
    /// Pipe expression.
    PipeExpr,
    /// String node.
    String,
    /// Encapsed string node.
    Encapsed,
    /// Heredoc node.
    Heredoc,
}

impl SyntaxNodeKind {
    /// Returns a stable display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SourceFile => "SOURCE_FILE",
            Self::PhpBlock => "PHP_BLOCK",
            Self::InlineHtml => "INLINE_HTML",
            Self::StatementList => "STATEMENT_LIST",
            Self::Error => "ERROR",
            Self::EmptyStmt => "EMPTY_STMT",
            Self::ExprStmt => "EXPR_STMT",
            Self::EchoStmt => "ECHO_STMT",
            Self::ReturnStmt => "RETURN_STMT",
            Self::ThrowStmt => "THROW_STMT",
            Self::BreakStmt => "BREAK_STMT",
            Self::ContinueStmt => "CONTINUE_STMT",
            Self::BlockStmt => "BLOCK_STMT",
            Self::IfStmt => "IF_STMT",
            Self::WhileStmt => "WHILE_STMT",
            Self::DoWhileStmt => "DO_WHILE_STMT",
            Self::ForStmt => "FOR_STMT",
            Self::ForeachStmt => "FOREACH_STMT",
            Self::SwitchStmt => "SWITCH_STMT",
            Self::TryStmt => "TRY_STMT",
            Self::CatchClause => "CATCH_CLAUSE",
            Self::FinallyClause => "FINALLY_CLAUSE",
            Self::DeclareStmt => "DECLARE_STMT",
            Self::GlobalStmt => "GLOBAL_STMT",
            Self::StaticStmt => "STATIC_STMT",
            Self::UnsetStmt => "UNSET_STMT",
            Self::GotoStmt => "GOTO_STMT",
            Self::LabelStmt => "LABEL_STMT",
            Self::NamespaceStmt => "NAMESPACE_STMT",
            Self::UseDecl => "USE_DECL",
            Self::FunctionDecl => "FUNCTION_DECL",
            Self::ParamList => "PARAM_LIST",
            Self::Param => "PARAM",
            Self::ClassDecl => "CLASS_DECL",
            Self::InterfaceDecl => "INTERFACE_DECL",
            Self::TraitDecl => "TRAIT_DECL",
            Self::EnumDecl => "ENUM_DECL",
            Self::ClassMemberList => "CLASS_MEMBER_LIST",
            Self::MethodDecl => "METHOD_DECL",
            Self::PropertyDecl => "PROPERTY_DECL",
            Self::ClassConstDecl => "CLASS_CONST_DECL",
            Self::TraitUseDecl => "TRAIT_USE_DECL",
            Self::AttributeGroup => "ATTRIBUTE_GROUP",
            Self::Attribute => "ATTRIBUTE",
            Self::Type => "TYPE",
            Self::UnionType => "UNION_TYPE",
            Self::IntersectionType => "INTERSECTION_TYPE",
            Self::NullableType => "NULLABLE_TYPE",
            Self::DnfType => "DNF_TYPE",
            Self::Expr => "EXPR",
            Self::Literal => "LITERAL",
            Self::Name => "NAME",
            Self::Variable => "VARIABLE",
            Self::ParenthesizedExpr => "PARENTHESIZED_EXPR",
            Self::PrefixExpr => "PREFIX_EXPR",
            Self::VoidCastExpr => "VOID_CAST_EXPR",
            Self::BinaryExpr => "BINARY_EXPR",
            Self::AssignExpr => "ASSIGN_EXPR",
            Self::TernaryExpr => "TERNARY_EXPR",
            Self::CallExpr => "CALL_EXPR",
            Self::ArrayDimFetchExpr => "ARRAY_DIM_FETCH_EXPR",
            Self::PropertyFetchExpr => "PROPERTY_FETCH_EXPR",
            Self::StaticAccessExpr => "STATIC_ACCESS_EXPR",
            Self::ArrayExpr => "ARRAY_EXPR",
            Self::ArrayPair => "ARRAY_PAIR",
            Self::MatchExpr => "MATCH_EXPR",
            Self::ThrowExpr => "THROW_EXPR",
            Self::ConstructExpr => "CONSTRUCT_EXPR",
            Self::YieldExpr => "YIELD_EXPR",
            Self::ClosureExpr => "CLOSURE_EXPR",
            Self::ArrowFunctionExpr => "ARROW_FUNCTION_EXPR",
            Self::NewExpr => "NEW_EXPR",
            Self::CloneExpr => "CLONE_EXPR",
            Self::CloneWithExpr => "CLONE_WITH_EXPR",
            Self::PipeExpr => "PIPE_EXPR",
            Self::String => "STRING",
            Self::Encapsed => "ENCAPSED",
            Self::Heredoc => "HEREDOC",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SyntaxKind, SyntaxNodeKind, SyntaxTokenKind};
    use php_lexer::{SymbolKind, TokenKind, TokenName};

    #[test]
    fn maps_named_lexer_tokens() {
        assert_eq!(
            SyntaxKind::from_token_kind(TokenKind::Named(TokenName::OpenTag)),
            SyntaxKind::Token(SyntaxTokenKind::Named(TokenName::OpenTag))
        );
        assert_eq!(
            SyntaxKind::from_token_kind(TokenKind::Named(TokenName::Pipe)).name(),
            "T_PIPE"
        );
        assert_eq!(
            SyntaxKind::from_token_kind(TokenKind::Named(TokenName::VoidCast)).name(),
            "T_VOID_CAST"
        );
    }

    #[test]
    fn maps_symbol_and_eof_tokens() {
        assert_eq!(
            SyntaxKind::from_token_kind(TokenKind::Symbol(SymbolKind::Char(b';'))).name(),
            ";"
        );
        assert_eq!(SyntaxKind::from_token_kind(TokenKind::Eof).name(), "EOF");
    }

    #[test]
    fn classifies_tokens_nodes_and_trivia() {
        let whitespace = SyntaxKind::from_token_kind(TokenKind::Named(TokenName::Whitespace));
        assert!(whitespace.is_token());
        assert!(whitespace.is_trivia());
        assert!(SyntaxKind::Node(SyntaxNodeKind::SourceFile).is_node());
        assert!(!SyntaxKind::Node(SyntaxNodeKind::SourceFile).is_token());
    }
}
