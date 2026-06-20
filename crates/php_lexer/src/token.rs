use php_source::TextRange;
use std::fmt;

/// Normalized token names used by the lexer compatibility surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenName {
    /// PHP `T_INLINE_HTML`.
    InlineHtml,
    /// PHP `T_OPEN_TAG`.
    OpenTag,
    /// PHP `T_OPEN_TAG_WITH_ECHO`.
    OpenTagWithEcho,
    /// PHP `T_CLOSE_TAG`.
    CloseTag,
    /// PHP `T_WHITESPACE`.
    Whitespace,
    /// PHP `T_COMMENT`.
    Comment,
    /// PHP `T_DOC_COMMENT`.
    DocComment,
    /// PHP `T_BAD_CHARACTER`.
    BadCharacter,
    /// PHP `T_STRING`.
    String,
    /// PHP `T_IF`.
    If,
    /// PHP `T_ELSE`.
    Else,
    /// PHP `T_ELSEIF`.
    ElseIf,
    /// PHP `T_ENDIF`.
    EndIf,
    /// PHP `T_FUNCTION`.
    Function,
    /// PHP `T_CLASS`.
    Class,
    /// PHP `T_ABSTRACT`.
    Abstract,
    /// PHP `T_FINAL`.
    Final,
    /// PHP `T_INTERFACE`.
    Interface,
    /// PHP `T_TRAIT`.
    Trait,
    /// PHP `T_ENUM`.
    Enum,
    /// PHP `T_NAMESPACE`.
    Namespace,
    /// PHP `T_USE`.
    Use,
    /// PHP `T_AS`.
    As,
    /// PHP `T_INSTEADOF`.
    InsteadOf,
    /// PHP `T_MATCH`.
    Match,
    /// PHP `T_READONLY`.
    Readonly,
    /// PHP `T_FN`.
    Fn,
    /// PHP `T_YIELD`.
    Yield,
    /// PHP `T_YIELD_FROM`.
    YieldFrom,
    /// PHP `T_CASE`.
    Case,
    /// PHP `T_DEFAULT`.
    Default,
    /// PHP `T_ECHO`.
    Echo,
    /// PHP `T_PRINT`.
    Print,
    /// PHP `T_RETURN`.
    Return,
    /// PHP `T_BREAK`.
    Break,
    /// PHP `T_CONTINUE`.
    Continue,
    /// PHP `T_EXTENDS`.
    Extends,
    /// PHP `T_IMPLEMENTS`.
    Implements,
    /// PHP `T_PUBLIC`.
    Public,
    /// PHP `T_PROTECTED`.
    Protected,
    /// PHP `T_PRIVATE`.
    Private,
    /// PHP `T_STATIC`.
    Static,
    /// PHP `T_CONST`.
    Const,
    /// PHP `T_VAR`.
    Var,
    /// PHP `T_DECLARE`.
    Declare,
    /// PHP `T_ENDDECLARE`.
    EndDeclare,
    /// PHP `T_GLOBAL`.
    Global,
    /// PHP `T_CALLABLE`.
    Callable,
    /// PHP `T_CLONE`.
    Clone,
    /// PHP `T_NEW`.
    New,
    /// PHP `T_WHILE`.
    While,
    /// PHP `T_ENDWHILE`.
    EndWhile,
    /// PHP `T_DO`.
    Do,
    /// PHP `T_FOR`.
    For,
    /// PHP `T_ENDFOR`.
    EndFor,
    /// PHP `T_FOREACH`.
    Foreach,
    /// PHP `T_ENDFOREACH`.
    EndForeach,
    /// PHP `T_SWITCH`.
    Switch,
    /// PHP `T_ENDSWITCH`.
    EndSwitch,
    /// PHP `T_TRY`.
    Try,
    /// PHP `T_THROW`.
    Throw,
    /// PHP `T_CATCH`.
    Catch,
    /// PHP `T_FINALLY`.
    Finally,
    /// PHP `T_INCLUDE`.
    Include,
    /// PHP `T_INCLUDE_ONCE`.
    IncludeOnce,
    /// PHP `T_REQUIRE`.
    Require,
    /// PHP `T_REQUIRE_ONCE`.
    RequireOnce,
    /// PHP `T_EVAL`.
    Eval,
    /// PHP `T_ISSET`.
    Isset,
    /// PHP `T_EMPTY`.
    Empty,
    /// PHP `T_UNSET`.
    Unset,
    /// PHP `T_LIST`.
    List,
    /// PHP `T_ARRAY`.
    Array,
    /// PHP `T_INSTANCEOF`.
    Instanceof,
    /// PHP `T_GOTO`.
    Goto,
    /// PHP `T_EXIT`.
    Exit,
    /// PHP `T_HALT_COMPILER`.
    HaltCompiler,
    /// PHP `T_VARIABLE`.
    Variable,
    /// PHP `T_NAME_FULLY_QUALIFIED`.
    NameFullyQualified,
    /// PHP `T_NAME_QUALIFIED`.
    NameQualified,
    /// PHP `T_NAME_RELATIVE`.
    NameRelative,
    /// PHP `T_LNUMBER`.
    LNumber,
    /// PHP `T_DNUMBER`.
    DNumber,
    /// PHP `T_IS_EQUAL`.
    IsEqual,
    /// PHP `T_IS_NOT_EQUAL`.
    IsNotEqual,
    /// PHP `T_IS_IDENTICAL`.
    IsIdentical,
    /// PHP `T_IS_NOT_IDENTICAL`.
    IsNotIdentical,
    /// PHP `T_IS_SMALLER_OR_EQUAL`.
    IsSmallerOrEqual,
    /// PHP `T_IS_GREATER_OR_EQUAL`.
    IsGreaterOrEqual,
    /// PHP `T_SPACESHIP`.
    Spaceship,
    /// PHP `T_BOOLEAN_AND`.
    BooleanAnd,
    /// PHP `T_BOOLEAN_OR`.
    BooleanOr,
    /// PHP `T_LOGICAL_AND`.
    LogicalAnd,
    /// PHP `T_LOGICAL_OR`.
    LogicalOr,
    /// PHP `T_LOGICAL_XOR`.
    LogicalXor,
    /// PHP `T_SL`.
    Sl,
    /// PHP `T_SR`.
    Sr,
    /// PHP `T_SL_EQUAL`.
    SlEqual,
    /// PHP `T_SR_EQUAL`.
    SrEqual,
    /// PHP `T_PLUS_EQUAL`.
    PlusEqual,
    /// PHP `T_MINUS_EQUAL`.
    MinusEqual,
    /// PHP `T_MUL_EQUAL`.
    MulEqual,
    /// PHP `T_DIV_EQUAL`.
    DivEqual,
    /// PHP `T_MOD_EQUAL`.
    ModEqual,
    /// PHP `T_CONCAT_EQUAL`.
    ConcatEqual,
    /// PHP `T_AND_EQUAL`.
    AndEqual,
    /// PHP `T_OR_EQUAL`.
    OrEqual,
    /// PHP `T_XOR_EQUAL`.
    XorEqual,
    /// PHP `T_COALESCE`.
    Coalesce,
    /// PHP `T_COALESCE_EQUAL`.
    CoalesceEqual,
    /// PHP `T_OBJECT_OPERATOR`.
    ObjectOperator,
    /// PHP `T_NULLSAFE_OBJECT_OPERATOR`.
    NullsafeObjectOperator,
    /// PHP `T_DOUBLE_COLON`.
    DoubleColon,
    /// PHP `T_DOUBLE_ARROW`.
    DoubleArrow,
    /// PHP `T_INC`.
    Inc,
    /// PHP `T_DEC`.
    Dec,
    /// PHP `T_ELLIPSIS`.
    Ellipsis,
    /// PHP `T_POW`.
    Pow,
    /// PHP `T_POW_EQUAL`.
    PowEqual,
    /// PHP `T_ATTRIBUTE`.
    Attribute,
    /// PHP `T_INT_CAST`.
    IntCast,
    /// PHP `T_DOUBLE_CAST`.
    DoubleCast,
    /// PHP `T_STRING_CAST`.
    StringCast,
    /// PHP `T_ARRAY_CAST`.
    ArrayCast,
    /// PHP `T_OBJECT_CAST`.
    ObjectCast,
    /// PHP `T_BOOL_CAST`.
    BoolCast,
    /// PHP `T_UNSET_CAST`.
    UnsetCast,
    /// PHP `T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG`.
    AmpersandFollowedByVarOrVararg,
    /// PHP `T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG`.
    AmpersandNotFollowedByVarOrVararg,
    /// PHP 8.4+ `T_PUBLIC_SET`.
    PublicSet,
    /// PHP 8.4+ `T_PROTECTED_SET`.
    ProtectedSet,
    /// PHP 8.4+ `T_PRIVATE_SET`.
    PrivateSet,
    /// PHP `T_CONSTANT_ENCAPSED_STRING`.
    ConstantEncapsedString,
    /// PHP `T_ENCAPSED_AND_WHITESPACE`.
    EncapsedAndWhitespace,
    /// PHP `T_NUM_STRING`.
    NumString,
    /// PHP `T_STRING_VARNAME`.
    StringVarName,
    /// PHP `T_CURLY_OPEN`.
    CurlyOpen,
    /// PHP `T_DOLLAR_OPEN_CURLY_BRACES`.
    DollarOpenCurlyBraces,
    /// PHP `T_START_HEREDOC`.
    StartHeredoc,
    /// PHP `T_END_HEREDOC`.
    EndHeredoc,
    /// PHP `T_LINE`.
    Line,
    /// PHP `T_FILE`.
    File,
    /// PHP `T_DIR`.
    Dir,
    /// PHP `T_CLASS_C`.
    ClassC,
    /// PHP `T_TRAIT_C`.
    TraitC,
    /// PHP `T_METHOD_C`.
    MethodC,
    /// PHP `T_FUNC_C`.
    FuncC,
    /// PHP `T_NS_C`.
    NamespaceC,
    /// PHP 8.5 `T_PROPERTY_C`.
    PropertyC,
    /// PHP 8.5 `T_PIPE`.
    Pipe,
    /// PHP `T_VOID_CAST`.
    VoidCast,
}

impl TokenName {
    /// Returns the compatibility name used for reference comparisons.
    #[must_use]
    pub const fn as_php_name(self) -> &'static str {
        match self {
            Self::InlineHtml => "T_INLINE_HTML",
            Self::OpenTag => "T_OPEN_TAG",
            Self::OpenTagWithEcho => "T_OPEN_TAG_WITH_ECHO",
            Self::CloseTag => "T_CLOSE_TAG",
            Self::Whitespace => "T_WHITESPACE",
            Self::Comment => "T_COMMENT",
            Self::DocComment => "T_DOC_COMMENT",
            Self::BadCharacter => "T_BAD_CHARACTER",
            Self::String => "T_STRING",
            Self::If => "T_IF",
            Self::Else => "T_ELSE",
            Self::ElseIf => "T_ELSEIF",
            Self::EndIf => "T_ENDIF",
            Self::Function => "T_FUNCTION",
            Self::Class => "T_CLASS",
            Self::Abstract => "T_ABSTRACT",
            Self::Final => "T_FINAL",
            Self::Interface => "T_INTERFACE",
            Self::Trait => "T_TRAIT",
            Self::Enum => "T_ENUM",
            Self::Namespace => "T_NAMESPACE",
            Self::Use => "T_USE",
            Self::As => "T_AS",
            Self::InsteadOf => "T_INSTEADOF",
            Self::Match => "T_MATCH",
            Self::Readonly => "T_READONLY",
            Self::Fn => "T_FN",
            Self::Yield => "T_YIELD",
            Self::YieldFrom => "T_YIELD_FROM",
            Self::Case => "T_CASE",
            Self::Default => "T_DEFAULT",
            Self::Echo => "T_ECHO",
            Self::Print => "T_PRINT",
            Self::Return => "T_RETURN",
            Self::Break => "T_BREAK",
            Self::Continue => "T_CONTINUE",
            Self::Extends => "T_EXTENDS",
            Self::Implements => "T_IMPLEMENTS",
            Self::Public => "T_PUBLIC",
            Self::Protected => "T_PROTECTED",
            Self::Private => "T_PRIVATE",
            Self::Static => "T_STATIC",
            Self::Const => "T_CONST",
            Self::Var => "T_VAR",
            Self::Declare => "T_DECLARE",
            Self::EndDeclare => "T_ENDDECLARE",
            Self::Global => "T_GLOBAL",
            Self::Callable => "T_CALLABLE",
            Self::Clone => "T_CLONE",
            Self::New => "T_NEW",
            Self::While => "T_WHILE",
            Self::EndWhile => "T_ENDWHILE",
            Self::Do => "T_DO",
            Self::For => "T_FOR",
            Self::EndFor => "T_ENDFOR",
            Self::Foreach => "T_FOREACH",
            Self::EndForeach => "T_ENDFOREACH",
            Self::Switch => "T_SWITCH",
            Self::EndSwitch => "T_ENDSWITCH",
            Self::Try => "T_TRY",
            Self::Throw => "T_THROW",
            Self::Catch => "T_CATCH",
            Self::Finally => "T_FINALLY",
            Self::Include => "T_INCLUDE",
            Self::IncludeOnce => "T_INCLUDE_ONCE",
            Self::Require => "T_REQUIRE",
            Self::RequireOnce => "T_REQUIRE_ONCE",
            Self::Eval => "T_EVAL",
            Self::Isset => "T_ISSET",
            Self::Empty => "T_EMPTY",
            Self::Unset => "T_UNSET",
            Self::List => "T_LIST",
            Self::Array => "T_ARRAY",
            Self::Instanceof => "T_INSTANCEOF",
            Self::Goto => "T_GOTO",
            Self::Exit => "T_EXIT",
            Self::HaltCompiler => "T_HALT_COMPILER",
            Self::Variable => "T_VARIABLE",
            Self::NameFullyQualified => "T_NAME_FULLY_QUALIFIED",
            Self::NameQualified => "T_NAME_QUALIFIED",
            Self::NameRelative => "T_NAME_RELATIVE",
            Self::LNumber => "T_LNUMBER",
            Self::DNumber => "T_DNUMBER",
            Self::IsEqual => "T_IS_EQUAL",
            Self::IsNotEqual => "T_IS_NOT_EQUAL",
            Self::IsIdentical => "T_IS_IDENTICAL",
            Self::IsNotIdentical => "T_IS_NOT_IDENTICAL",
            Self::IsSmallerOrEqual => "T_IS_SMALLER_OR_EQUAL",
            Self::IsGreaterOrEqual => "T_IS_GREATER_OR_EQUAL",
            Self::Spaceship => "T_SPACESHIP",
            Self::BooleanAnd => "T_BOOLEAN_AND",
            Self::BooleanOr => "T_BOOLEAN_OR",
            Self::LogicalAnd => "T_LOGICAL_AND",
            Self::LogicalOr => "T_LOGICAL_OR",
            Self::LogicalXor => "T_LOGICAL_XOR",
            Self::Sl => "T_SL",
            Self::Sr => "T_SR",
            Self::SlEqual => "T_SL_EQUAL",
            Self::SrEqual => "T_SR_EQUAL",
            Self::PlusEqual => "T_PLUS_EQUAL",
            Self::MinusEqual => "T_MINUS_EQUAL",
            Self::MulEqual => "T_MUL_EQUAL",
            Self::DivEqual => "T_DIV_EQUAL",
            Self::ModEqual => "T_MOD_EQUAL",
            Self::ConcatEqual => "T_CONCAT_EQUAL",
            Self::AndEqual => "T_AND_EQUAL",
            Self::OrEqual => "T_OR_EQUAL",
            Self::XorEqual => "T_XOR_EQUAL",
            Self::Coalesce => "T_COALESCE",
            Self::CoalesceEqual => "T_COALESCE_EQUAL",
            Self::ObjectOperator => "T_OBJECT_OPERATOR",
            Self::NullsafeObjectOperator => "T_NULLSAFE_OBJECT_OPERATOR",
            Self::DoubleColon => "T_DOUBLE_COLON",
            Self::DoubleArrow => "T_DOUBLE_ARROW",
            Self::Inc => "T_INC",
            Self::Dec => "T_DEC",
            Self::Ellipsis => "T_ELLIPSIS",
            Self::Pow => "T_POW",
            Self::PowEqual => "T_POW_EQUAL",
            Self::Attribute => "T_ATTRIBUTE",
            Self::IntCast => "T_INT_CAST",
            Self::DoubleCast => "T_DOUBLE_CAST",
            Self::StringCast => "T_STRING_CAST",
            Self::ArrayCast => "T_ARRAY_CAST",
            Self::ObjectCast => "T_OBJECT_CAST",
            Self::BoolCast => "T_BOOL_CAST",
            Self::UnsetCast => "T_UNSET_CAST",
            Self::AmpersandFollowedByVarOrVararg => "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG",
            Self::AmpersandNotFollowedByVarOrVararg => "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG",
            Self::PublicSet => "T_PUBLIC_SET",
            Self::ProtectedSet => "T_PROTECTED_SET",
            Self::PrivateSet => "T_PRIVATE_SET",
            Self::ConstantEncapsedString => "T_CONSTANT_ENCAPSED_STRING",
            Self::EncapsedAndWhitespace => "T_ENCAPSED_AND_WHITESPACE",
            Self::NumString => "T_NUM_STRING",
            Self::StringVarName => "T_STRING_VARNAME",
            Self::CurlyOpen => "T_CURLY_OPEN",
            Self::DollarOpenCurlyBraces => "T_DOLLAR_OPEN_CURLY_BRACES",
            Self::StartHeredoc => "T_START_HEREDOC",
            Self::EndHeredoc => "T_END_HEREDOC",
            Self::Line => "T_LINE",
            Self::File => "T_FILE",
            Self::Dir => "T_DIR",
            Self::ClassC => "T_CLASS_C",
            Self::TraitC => "T_TRAIT_C",
            Self::MethodC => "T_METHOD_C",
            Self::FuncC => "T_FUNC_C",
            Self::NamespaceC => "T_NS_C",
            Self::PropertyC => "T_PROPERTY_C",
            Self::Pipe => "T_PIPE",
            Self::VoidCast => "T_VOID_CAST",
        }
    }
}

/// Single-byte symbol tokens that PHP reports as string tokens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    /// A single-byte PHP symbol such as `;`, `{`, or `=`.
    Char(u8),
}

impl SymbolKind {
    /// Returns the normalized reference name.
    #[must_use]
    pub fn reference_name(self) -> String {
        match self {
            Self::Char(byte) if byte.is_ascii() => char::from(byte).to_string(),
            Self::Char(byte) => format!("\\x{byte:02X}"),
        }
    }
}

/// Complete lexer token kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    /// Named PHP `T_*` token.
    Named(TokenName),
    /// Single-byte symbol token.
    Symbol(SymbolKind),
    /// Synthetic end-of-file marker, emitted only when configured.
    Eof,
}

impl TokenKind {
    /// Returns the normalized reference name for comparisons.
    #[must_use]
    pub fn reference_name(self) -> String {
        match self {
            Self::Named(name) => name.as_php_name().to_owned(),
            Self::Symbol(symbol) => symbol.reference_name(),
            Self::Eof => "EOF".to_owned(),
        }
    }
}

/// A lossless token reference into the original source.
#[derive(Clone, Eq, PartialEq)]
pub struct Token {
    /// Token kind.
    pub kind: TokenKind,
    /// Byte range in the original source.
    pub range: TextRange,
    /// One-based start line.
    pub line: u32,
}

impl Token {
    /// Creates a token.
    #[must_use]
    pub const fn new(kind: TokenKind, range: TextRange, line: u32) -> Self {
        Self { kind, range, line }
    }

    /// Returns the normalized reference name for this token.
    #[must_use]
    pub fn reference_name(&self) -> String {
        self.kind.reference_name()
    }

    /// Returns the token text when the range is valid for `source`.
    #[must_use]
    pub fn text<'src>(&self, source: &'src str) -> Option<&'src str> {
        source.get(self.range.start().to_usize()..self.range.end().to_usize())
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Token")
            .field("kind", &self.kind)
            .field("range", &self.range)
            .field("line", &self.line)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{SymbolKind, Token, TokenKind, TokenName};
    use php_source::TextRange;

    #[test]
    fn token_names_use_reference_strings() {
        assert_eq!(TokenName::InlineHtml.as_php_name(), "T_INLINE_HTML");
        assert_eq!(TokenName::OpenTag.as_php_name(), "T_OPEN_TAG");
        assert_eq!(
            TokenName::OpenTagWithEcho.as_php_name(),
            "T_OPEN_TAG_WITH_ECHO"
        );
        assert_eq!(TokenName::CloseTag.as_php_name(), "T_CLOSE_TAG");
        assert_eq!(TokenName::Whitespace.as_php_name(), "T_WHITESPACE");
        assert_eq!(TokenName::Comment.as_php_name(), "T_COMMENT");
        assert_eq!(TokenName::DocComment.as_php_name(), "T_DOC_COMMENT");
        assert_eq!(TokenName::BadCharacter.as_php_name(), "T_BAD_CHARACTER");
        assert_eq!(TokenName::String.as_php_name(), "T_STRING");
        assert_eq!(TokenName::Variable.as_php_name(), "T_VARIABLE");
        assert_eq!(TokenName::LNumber.as_php_name(), "T_LNUMBER");
        assert_eq!(TokenName::DNumber.as_php_name(), "T_DNUMBER");
        assert_eq!(TokenName::CoalesceEqual.as_php_name(), "T_COALESCE_EQUAL");
        assert_eq!(
            TokenName::NullsafeObjectOperator.as_php_name(),
            "T_NULLSAFE_OBJECT_OPERATOR"
        );
        assert_eq!(TokenName::Attribute.as_php_name(), "T_ATTRIBUTE");
        assert_eq!(TokenName::IntCast.as_php_name(), "T_INT_CAST");
        assert_eq!(
            TokenName::AmpersandFollowedByVarOrVararg.as_php_name(),
            "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG"
        );
        assert_eq!(TokenName::PublicSet.as_php_name(), "T_PUBLIC_SET");
        assert_eq!(
            TokenName::ConstantEncapsedString.as_php_name(),
            "T_CONSTANT_ENCAPSED_STRING"
        );
        assert_eq!(
            TokenName::EncapsedAndWhitespace.as_php_name(),
            "T_ENCAPSED_AND_WHITESPACE"
        );
        assert_eq!(TokenName::NumString.as_php_name(), "T_NUM_STRING");
        assert_eq!(TokenName::StringVarName.as_php_name(), "T_STRING_VARNAME");
        assert_eq!(TokenName::CurlyOpen.as_php_name(), "T_CURLY_OPEN");
        assert_eq!(
            TokenName::DollarOpenCurlyBraces.as_php_name(),
            "T_DOLLAR_OPEN_CURLY_BRACES"
        );
        assert_eq!(TokenName::StartHeredoc.as_php_name(), "T_START_HEREDOC");
        assert_eq!(TokenName::EndHeredoc.as_php_name(), "T_END_HEREDOC");
        assert_eq!(TokenName::Pipe.as_php_name(), "T_PIPE");
        assert_eq!(TokenName::VoidCast.as_php_name(), "T_VOID_CAST");
    }

    #[test]
    fn token_kind_reference_names_are_stable() {
        assert_eq!(
            TokenKind::Named(TokenName::Variable).reference_name(),
            "T_VARIABLE"
        );
        assert_eq!(
            TokenKind::Symbol(SymbolKind::Char(b';')).reference_name(),
            ";"
        );
        assert_eq!(TokenKind::Eof.reference_name(), "EOF");
    }

    #[test]
    fn token_text_is_span_derived() {
        let token = Token::new(
            TokenKind::Named(TokenName::OpenTag),
            TextRange::new(0, 5),
            1,
        );
        assert_eq!(token.text("<?php echo"), Some("<?php"));
        assert_eq!(token.reference_name(), "T_OPEN_TAG");
    }

    #[test]
    fn malformed_ranges_do_not_panic() {
        let range = TextRange::new(10, 2);
        assert_eq!(range.len(), 0);
        assert!(range.is_empty());
    }
}
