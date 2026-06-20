/// Scanner modes required for PHP tokenization.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LexerMode {
    /// Bytes outside a PHP opening tag.
    #[default]
    InlineHtml,
    /// Normal PHP script tokenization.
    Scripting,
    /// Interpolated double-quoted string body.
    DoubleQuote,
    /// Interpolated shell execution string body.
    Backtick,
    /// Heredoc body.
    Heredoc,
    /// Nowdoc body.
    Nowdoc,
    /// Variable offset inside an interpolated string.
    StringVarOffset,
    /// Scanner is resolving a variable name in interpolation.
    LookingForVarName,
}
