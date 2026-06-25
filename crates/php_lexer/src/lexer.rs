use crate::cursor::Cursor;
use crate::keywords::{eq_ignore_ascii_case, keyword_or_magic_token};
use crate::strings::{StringScan, scan_constant_encapsed_string};
use crate::{
    LexDiagnostic, LexDiagnosticKind, LexerMode, SymbolKind, TextRange, Token, TokenKind, TokenName,
};

/// Configuration for lexer behavior.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LexerConfig {
    /// Whether `<?` should be treated as an opening tag in future scanner work.
    pub short_open_tag: bool,
    /// Reserved for future `TOKEN_PARSE`-like contextual lexing.
    pub token_parse: bool,
    /// Whether to append a synthetic EOF token.
    pub emit_eof: bool,
}

/// Result returned by whole-source lexing.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LexResult {
    /// Tokens emitted by the lexer.
    pub tokens: Vec<Token>,
    /// Recoverable diagnostics emitted during lexing.
    pub diagnostics: Vec<LexDiagnostic>,
}

/// Byte-oriented PHP lexer.
#[derive(Clone, Debug)]
pub struct Lexer<'src> {
    source: &'src str,
    cursor: Cursor<'src>,
    config: LexerConfig,
    mode: LexerMode,
    mode_start: usize,
    mode_start_line: u32,
    line: u32,
    emitted_placeholder: bool,
    emitted_eof: bool,
    expecting_object_property: bool,
    expecting_string_varname: bool,
    in_string_var_offset: bool,
    in_braced_interpolation: bool,
    last_encapsed_variable: bool,
    halt_compiler_seen: bool,
    heredoc_label: Option<String>,
    heredoc_nowdoc: bool,
    diagnostics: Vec<LexDiagnostic>,
}

impl<'src> Lexer<'src> {
    /// Creates a lexer over `source`.
    #[must_use]
    pub fn new(source: &'src str, config: LexerConfig) -> Self {
        Self {
            source,
            cursor: Cursor::new(source),
            config,
            mode: LexerMode::InlineHtml,
            mode_start: 0,
            mode_start_line: 1,
            line: 1,
            emitted_placeholder: false,
            emitted_eof: false,
            expecting_object_property: false,
            expecting_string_varname: false,
            in_string_var_offset: false,
            in_braced_interpolation: false,
            last_encapsed_variable: false,
            halt_compiler_seen: false,
            heredoc_label: None,
            heredoc_nowdoc: false,
            diagnostics: Vec::new(),
        }
    }

    /// Returns the source being lexed.
    #[must_use]
    pub const fn source(&self) -> &'src str {
        self.source
    }

    /// Returns the current scanner mode.
    #[must_use]
    pub const fn mode(&self) -> LexerMode {
        self.mode
    }

    /// Returns the current byte offset.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.cursor.position()
    }

    /// Emits the next token.
    pub fn next_token(&mut self) -> Option<Token> {
        if !self.emitted_placeholder {
            self.emitted_placeholder = true;
        }

        let token = match self.mode {
            LexerMode::InlineHtml => self.next_inline_html_token(),
            LexerMode::Scripting => self.next_scripting_token(),
            LexerMode::DoubleQuote => self.next_encapsed_token(b'"'),
            LexerMode::Backtick => self.next_encapsed_token(b'`'),
            LexerMode::Heredoc | LexerMode::Nowdoc => self.next_heredoc_token(),
            _ => self.next_scripting_token(),
        };

        if token.is_some() {
            return token;
        }

        if self.config.emit_eof && !self.emitted_eof {
            self.emitted_eof = true;
            let offset = self.cursor.position();
            return Some(Token::new(
                TokenKind::Eof,
                TextRange::new(offset, offset),
                1,
            ));
        }

        None
    }

    fn next_inline_html_token(&mut self) -> Option<Token> {
        if self.cursor.is_eof() {
            return None;
        }

        if self.cursor.position() == 0
            && self.cursor.starts_with(b"\xEF\xBB\xBF")
            && self.open_tag_at_offset(3).is_some()
        {
            self.consume_len(3);
        }

        if let Some((kind, len)) = self.open_tag_at_cursor() {
            let start = self.cursor.position();
            let line = self.line;
            self.consume_len(len);
            self.mode = LexerMode::Scripting;
            return Some(Token::new(
                kind,
                TextRange::new(start, self.cursor.position()),
                line,
            ));
        }

        let start = self.cursor.position();
        let line = self.line;
        while !self.cursor.is_eof() && self.open_tag_at_cursor().is_none() {
            self.consume_len(1);
        }

        Some(Token::new(
            TokenKind::Named(TokenName::InlineHtml),
            TextRange::new(start, self.cursor.position()),
            line,
        ))
    }

    fn next_scripting_token(&mut self) -> Option<Token> {
        if self.cursor.is_eof() {
            return None;
        }

        if self.halt_compiler_seen && self.cursor.peek() == Some(b';') {
            let token = self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(b';')), 1);
            self.mode = LexerMode::InlineHtml;
            self.halt_compiler_seen = false;
            return Some(token);
        }

        if self.cursor.starts_with(b"?>") {
            let start = self.cursor.position();
            let line = self.line;
            let len = self.close_tag_len();
            self.consume_len(len);
            self.mode = LexerMode::InlineHtml;
            return Some(Token::new(
                TokenKind::Named(TokenName::CloseTag),
                TextRange::new(start, self.cursor.position()),
                line,
            ));
        }

        if is_php_whitespace(self.cursor.peek().unwrap_or_default()) {
            return Some(
                self.consume_while_token(TokenKind::Named(TokenName::Whitespace), |lexer| {
                    lexer.cursor.peek().is_some_and(is_php_whitespace)
                }),
            );
        }

        if self.cursor.starts_with(b"#[") {
            return Some(self.consume_fixed_token(TokenKind::Named(TokenName::Attribute), 2));
        }

        if self.cursor.starts_with(b"//") {
            return Some(self.consume_line_comment());
        }

        if self.cursor.starts_with(b"#") {
            return Some(self.consume_line_comment());
        }

        if self.cursor.starts_with(b"/*") {
            return Some(self.consume_block_comment());
        }

        if is_bad_character(self.cursor.peek().unwrap_or_default()) {
            let start = self.cursor.position();
            let line = self.line;
            self.consume_len(1);
            let range = TextRange::new(start, self.cursor.position());
            self.diagnostics.push(LexDiagnostic::new(
                LexDiagnosticKind::BadCharacter,
                "bad control character in scripting mode",
                range,
                line as usize,
            ));
            return Some(Token::new(
                TokenKind::Named(TokenName::BadCharacter),
                range,
                line,
            ));
        }

        if self.cursor.peek().is_some_and(|byte| byte.is_ascii_digit())
            || (self.cursor.peek() == Some(b'.')
                && self
                    .cursor
                    .peek_n(1)
                    .is_some_and(|byte| byte.is_ascii_digit()))
        {
            return Some(self.consume_number());
        }

        if let Some(token) = self.consume_constant_string_if_available() {
            return Some(token);
        }

        if let Some(token) = self.consume_heredoc_start_if_available() {
            return Some(token);
        }

        if self.cursor.peek() == Some(b'`') {
            let start = self.cursor.position();
            let line = self.line;
            self.enter_encapsed_mode(LexerMode::Backtick, start, line);
            return Some(self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(b'`')), 1));
        }

        if let Some((kind, len)) = self.cast_at_cursor() {
            return Some(self.consume_fixed_token(kind, len));
        }

        if let Some(token) = self.consume_yield_from() {
            return Some(token);
        }

        if self.cursor.peek() == Some(b'$') {
            return Some(self.consume_variable_or_dollar());
        }

        if let Some((kind, len)) = self.property_hook_visibility_at_cursor() {
            return Some(self.consume_fixed_token(kind, len));
        }

        if self.cursor.peek() == Some(b'\\')
            && self.cursor.peek_n(1).is_some_and(is_identifier_start)
        {
            return Some(
                self.consume_qualified_name(TokenKind::Named(TokenName::NameFullyQualified), true),
            );
        }

        if self.cursor.peek().is_some_and(is_identifier_start) {
            return Some(self.consume_identifier_or_name());
        }

        if let Some((kind, len)) = self.operator_at_cursor() {
            return Some(self.consume_fixed_token(kind, len));
        }

        if self.cursor.peek() == Some(b'&') {
            return Some(self.consume_ampersand());
        }

        let start = self.cursor.position();
        let line = self.line;
        let byte = self.cursor.peek().unwrap_or_default();
        self.consume_len(1);
        Some(Token::new(
            TokenKind::Symbol(SymbolKind::Char(byte)),
            TextRange::new(start, self.cursor.position()),
            line,
        ))
    }

    fn next_encapsed_token(&mut self, delimiter: u8) -> Option<Token> {
        if self.cursor.is_eof() {
            let range = TextRange::new(self.mode_start, self.cursor.position());
            self.diagnostics.push(LexDiagnostic::new(
                LexDiagnosticKind::UnterminatedString,
                "unterminated encapsed string literal",
                range,
                self.mode_start_line as usize,
            ));
            self.mode = LexerMode::Scripting;
            self.clear_encapsed_state();
            return None;
        }

        if self.cursor.peek() == Some(delimiter) {
            self.mode = LexerMode::Scripting;
            self.clear_encapsed_state();
            return Some(
                self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(delimiter)), 1),
            );
        }

        if self.expecting_object_property && self.cursor.peek().is_some_and(is_identifier_start) {
            self.expecting_object_property = false;
            self.last_encapsed_variable = false;
            return Some(self.consume_identifier_token(TokenName::String));
        }

        if self.expecting_string_varname && self.cursor.peek().is_some_and(is_identifier_start) {
            self.expecting_string_varname = false;
            self.last_encapsed_variable = false;
            return Some(self.consume_identifier_token(TokenName::StringVarName));
        }

        if self.in_string_var_offset {
            if self.cursor.peek().is_some_and(is_decimal_digit) {
                return Some(self.consume_num_string());
            }
            if self.cursor.peek() == Some(b']') {
                self.in_string_var_offset = false;
                return Some(
                    self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(b']')), 1),
                );
            }
        }

        if self.in_braced_interpolation && self.cursor.peek() == Some(b'}') {
            self.in_braced_interpolation = false;
            self.last_encapsed_variable = false;
            return Some(self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(b'}')), 1));
        }

        if self.cursor.starts_with(b"{$") {
            self.in_braced_interpolation = true;
            self.last_encapsed_variable = false;
            return Some(self.consume_fixed_token(TokenKind::Named(TokenName::CurlyOpen), 1));
        }

        if self.cursor.starts_with(b"${") {
            self.expecting_string_varname = true;
            self.in_braced_interpolation = true;
            self.last_encapsed_variable = false;
            return Some(
                self.consume_fixed_token(TokenKind::Named(TokenName::DollarOpenCurlyBraces), 2),
            );
        }

        if self.cursor.peek() == Some(b'$')
            && self.cursor.peek_n(1).is_some_and(is_identifier_start)
        {
            let token = self.consume_variable_or_dollar();
            self.last_encapsed_variable = token.kind == TokenKind::Named(TokenName::Variable);
            return Some(token);
        }

        if self.last_encapsed_variable && self.cursor.starts_with(b"->") {
            self.expecting_object_property = true;
            self.last_encapsed_variable = false;
            return Some(self.consume_fixed_token(TokenKind::Named(TokenName::ObjectOperator), 2));
        }

        if self.last_encapsed_variable && self.cursor.peek() == Some(b'[') {
            self.in_string_var_offset = true;
            self.last_encapsed_variable = false;
            return Some(self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(b'[')), 1));
        }

        Some(self.consume_encapsed_text(delimiter))
    }

    fn next_heredoc_token(&mut self) -> Option<Token> {
        if self.cursor.is_eof() {
            let range = TextRange::new(self.mode_start, self.cursor.position());
            self.diagnostics.push(LexDiagnostic::new(
                LexDiagnosticKind::UnterminatedHeredoc,
                "unterminated heredoc/nowdoc literal",
                range,
                self.mode_start_line as usize,
            ));
            self.mode = LexerMode::Scripting;
            self.clear_heredoc_state();
            self.clear_encapsed_state();
            return None;
        }

        if let Some(len) = self.heredoc_end_len_at_cursor() {
            self.mode = LexerMode::Scripting;
            self.clear_heredoc_state();
            self.clear_encapsed_state();
            return Some(self.consume_fixed_token(TokenKind::Named(TokenName::EndHeredoc), len));
        }

        if !self.heredoc_nowdoc
            && let Some(token) = self.next_interpolation_token_without_delimiter()
        {
            return Some(token);
        }

        Some(self.consume_heredoc_text())
    }

    fn consume_fixed_token(&mut self, kind: TokenKind, len: usize) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        self.consume_len(len);
        Token::new(kind, TextRange::new(start, self.cursor.position()), line)
    }

    fn consume_while_token(
        &mut self,
        kind: TokenKind,
        mut predicate: impl FnMut(&Self) -> bool,
    ) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        while !self.cursor.is_eof() && predicate(self) {
            self.consume_len(1);
        }
        Token::new(kind, TextRange::new(start, self.cursor.position()), line)
    }

    fn consume_line_comment(&mut self) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        while !self.cursor.is_eof()
            && !self.cursor.starts_with(b"?>")
            && !matches!(self.cursor.peek(), Some(b'\r' | b'\n'))
        {
            self.consume_len(1);
        }
        Token::new(
            TokenKind::Named(TokenName::Comment),
            TextRange::new(start, self.cursor.position()),
            line,
        )
    }

    fn consume_block_comment(&mut self) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        let kind = if self.cursor.starts_with(b"/**") {
            TokenKind::Named(TokenName::DocComment)
        } else {
            TokenKind::Named(TokenName::Comment)
        };

        self.consume_len(2);
        while !self.cursor.is_eof() {
            if self.cursor.starts_with(b"*/") {
                self.consume_len(2);
                return Token::new(kind, TextRange::new(start, self.cursor.position()), line);
            }
            self.consume_len(1);
        }

        let range = TextRange::new(start, self.cursor.position());
        self.diagnostics.push(LexDiagnostic::new(
            LexDiagnosticKind::UnterminatedBlockComment,
            "unterminated block comment",
            range,
            line as usize,
        ));
        Token::new(kind, range, line)
    }

    fn consume_yield_from(&mut self) -> Option<Token> {
        if !self.word_at_cursor(b"yield") {
            return None;
        }

        let mut offset = 5;
        let mut saw_whitespace = false;
        while self.cursor.peek_n(offset).is_some_and(is_php_whitespace) {
            saw_whitespace = true;
            offset += 1;
        }

        if !saw_whitespace || !self.word_at_offset(offset, b"from") {
            return None;
        }

        let len = offset + 4;
        if self.cursor.peek_n(len).is_some_and(is_identifier_continue) {
            return None;
        }

        let start = self.cursor.position();
        let line = self.line;
        self.consume_len(len);
        Some(Token::new(
            TokenKind::Named(TokenName::YieldFrom),
            TextRange::new(start, self.cursor.position()),
            line,
        ))
    }

    fn consume_variable_or_dollar(&mut self) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        self.consume_len(1);

        if self.cursor.peek().is_some_and(is_identifier_start) {
            while self.cursor.peek().is_some_and(is_identifier_continue) {
                self.consume_len(1);
            }
            return Token::new(
                TokenKind::Named(TokenName::Variable),
                TextRange::new(start, self.cursor.position()),
                line,
            );
        }

        Token::new(
            TokenKind::Symbol(SymbolKind::Char(b'$')),
            TextRange::new(start, self.cursor.position()),
            line,
        )
    }

    fn consume_ampersand(&mut self) -> Token {
        let kind = if self.ampersand_is_followed_by_var_or_vararg() {
            TokenName::AmpersandFollowedByVarOrVararg
        } else {
            TokenName::AmpersandNotFollowedByVarOrVararg
        };
        self.consume_fixed_token(TokenKind::Named(kind), 1)
    }

    fn consume_constant_string_if_available(&mut self) -> Option<Token> {
        match scan_constant_encapsed_string(self.source, self.cursor.position())? {
            StringScan::Interpolated => {
                let start = self.cursor.position();
                let line = self.line;
                self.enter_encapsed_mode(LexerMode::DoubleQuote, start, line);
                Some(self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(b'"')), 1))
            }
            StringScan::Constant { len, terminated } => {
                let start = self.cursor.position();
                let line = self.line;
                self.consume_len(len);
                let range = TextRange::new(start, self.cursor.position());
                if !terminated {
                    self.diagnostics.push(LexDiagnostic::new(
                        LexDiagnosticKind::UnterminatedString,
                        "unterminated string literal",
                        range,
                        line as usize,
                    ));
                }
                Some(Token::new(
                    TokenKind::Named(TokenName::ConstantEncapsedString),
                    range,
                    line,
                ))
            }
        }
    }

    fn consume_heredoc_start_if_available(&mut self) -> Option<Token> {
        let (len, label, nowdoc) = self.heredoc_start_at_cursor()?;
        let start = self.cursor.position();
        let line = self.line;
        self.heredoc_label = Some(label);
        self.heredoc_nowdoc = nowdoc;
        self.enter_encapsed_mode(
            if nowdoc {
                LexerMode::Nowdoc
            } else {
                LexerMode::Heredoc
            },
            start,
            line,
        );
        self.heredoc_nowdoc = nowdoc;
        Some(self.consume_fixed_token(TokenKind::Named(TokenName::StartHeredoc), len))
    }

    fn next_interpolation_token_without_delimiter(&mut self) -> Option<Token> {
        if self.expecting_object_property && self.cursor.peek().is_some_and(is_identifier_start) {
            self.expecting_object_property = false;
            self.last_encapsed_variable = false;
            return Some(self.consume_identifier_token(TokenName::String));
        }

        if self.expecting_string_varname && self.cursor.peek().is_some_and(is_identifier_start) {
            self.expecting_string_varname = false;
            self.last_encapsed_variable = false;
            return Some(self.consume_identifier_token(TokenName::StringVarName));
        }

        if self.in_string_var_offset {
            if self.cursor.peek().is_some_and(is_decimal_digit) {
                return Some(self.consume_num_string());
            }
            if self.cursor.peek() == Some(b']') {
                self.in_string_var_offset = false;
                return Some(
                    self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(b']')), 1),
                );
            }
        }

        if self.in_braced_interpolation && self.cursor.peek() == Some(b'}') {
            self.in_braced_interpolation = false;
            self.last_encapsed_variable = false;
            return Some(self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(b'}')), 1));
        }

        if self.cursor.starts_with(b"{$") {
            self.in_braced_interpolation = true;
            self.last_encapsed_variable = false;
            return Some(self.consume_fixed_token(TokenKind::Named(TokenName::CurlyOpen), 1));
        }

        if self.cursor.starts_with(b"${") {
            self.expecting_string_varname = true;
            self.in_braced_interpolation = true;
            self.last_encapsed_variable = false;
            return Some(
                self.consume_fixed_token(TokenKind::Named(TokenName::DollarOpenCurlyBraces), 2),
            );
        }

        if self.cursor.peek() == Some(b'$')
            && self.cursor.peek_n(1).is_some_and(is_identifier_start)
        {
            let token = self.consume_variable_or_dollar();
            self.last_encapsed_variable = token.kind == TokenKind::Named(TokenName::Variable);
            return Some(token);
        }

        if self.last_encapsed_variable && self.cursor.starts_with(b"->") {
            self.expecting_object_property = true;
            self.last_encapsed_variable = false;
            return Some(self.consume_fixed_token(TokenKind::Named(TokenName::ObjectOperator), 2));
        }

        if self.last_encapsed_variable && self.cursor.peek() == Some(b'[') {
            self.in_string_var_offset = true;
            self.last_encapsed_variable = false;
            return Some(self.consume_fixed_token(TokenKind::Symbol(SymbolKind::Char(b'[')), 1));
        }

        None
    }

    fn consume_identifier_token(&mut self, name: TokenName) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        self.consume_identifier_bytes();
        Token::new(
            TokenKind::Named(name),
            TextRange::new(start, self.cursor.position()),
            line,
        )
    }

    fn consume_num_string(&mut self) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        while self.cursor.peek().is_some_and(is_decimal_digit) {
            self.consume_len(1);
        }
        Token::new(
            TokenKind::Named(TokenName::NumString),
            TextRange::new(start, self.cursor.position()),
            line,
        )
    }

    fn consume_encapsed_text(&mut self, delimiter: u8) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        while !self.cursor.is_eof() {
            if self.cursor.peek() == Some(delimiter)
                || self.cursor.starts_with(b"{$")
                || self.cursor.starts_with(b"${")
                || (self.cursor.peek() == Some(b'$')
                    && self.cursor.peek_n(1).is_some_and(is_identifier_start))
                || (self.in_braced_interpolation && self.cursor.peek() == Some(b'}'))
            {
                break;
            }

            if self.cursor.peek() == Some(b'\\') {
                self.consume_len(1);
                if !self.cursor.is_eof() {
                    self.consume_len(1);
                }
                continue;
            }

            self.consume_len(1);
        }
        self.last_encapsed_variable = false;
        Token::new(
            TokenKind::Named(TokenName::EncapsedAndWhitespace),
            TextRange::new(start, self.cursor.position()),
            line,
        )
    }

    fn consume_heredoc_text(&mut self) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        while !self.cursor.is_eof() {
            if self.heredoc_end_len_at_cursor().is_some()
                || (!self.heredoc_nowdoc
                    && (self.cursor.starts_with(b"{$")
                        || self.cursor.starts_with(b"${")
                        || (self.cursor.peek() == Some(b'$')
                            && self.cursor.peek_n(1).is_some_and(is_identifier_start))
                        || (self.in_braced_interpolation && self.cursor.peek() == Some(b'}'))))
            {
                break;
            }

            if !self.heredoc_nowdoc && self.cursor.peek() == Some(b'\\') {
                self.consume_len(1);
                if !self.cursor.is_eof() {
                    self.consume_len(1);
                }
                continue;
            }

            self.consume_len(1);
        }
        self.last_encapsed_variable = false;
        Token::new(
            TokenKind::Named(TokenName::EncapsedAndWhitespace),
            TextRange::new(start, self.cursor.position()),
            line,
        )
    }

    fn consume_identifier_or_name(&mut self) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        self.consume_identifier_bytes();

        if eq_ignore_ascii_case(
            &self.source.as_bytes()[start..self.cursor.position()],
            b"namespace",
        ) && self.cursor.peek() == Some(b'\\')
            && self.cursor.peek_n(1).is_some_and(is_identifier_start)
        {
            while self.cursor.peek() == Some(b'\\')
                && self.cursor.peek_n(1).is_some_and(is_identifier_start)
            {
                self.consume_len(1);
                self.consume_identifier_bytes();
            }
            return Token::new(
                TokenKind::Named(TokenName::NameRelative),
                TextRange::new(start, self.cursor.position()),
                line,
            );
        }

        let mut qualified = false;
        while self.cursor.peek() == Some(b'\\')
            && self.cursor.peek_n(1).is_some_and(is_identifier_start)
        {
            qualified = true;
            self.consume_len(1);
            self.consume_identifier_bytes();
        }

        let range = TextRange::new(start, self.cursor.position());
        if qualified {
            return Token::new(TokenKind::Named(TokenName::NameQualified), range, line);
        }

        let text = &self.source.as_bytes()[range.start().to_usize()..range.end().to_usize()];
        let name = keyword_or_magic_token(text).unwrap_or(TokenName::String);
        if name == TokenName::HaltCompiler {
            self.halt_compiler_seen = true;
        }
        Token::new(TokenKind::Named(name), range, line)
    }

    fn consume_qualified_name(&mut self, kind: TokenKind, leading_backslash: bool) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        if leading_backslash {
            self.consume_len(1);
        }
        self.consume_identifier_bytes();
        while self.cursor.peek() == Some(b'\\')
            && self.cursor.peek_n(1).is_some_and(is_identifier_start)
        {
            self.consume_len(1);
            self.consume_identifier_bytes();
        }
        Token::new(kind, TextRange::new(start, self.cursor.position()), line)
    }

    fn consume_identifier_bytes(&mut self) {
        if self.cursor.peek().is_some_and(is_identifier_start) {
            self.consume_len(1);
        }
        while self.cursor.peek().is_some_and(is_identifier_continue) {
            self.consume_len(1);
        }
    }

    fn consume_number(&mut self) -> Token {
        let start = self.cursor.position();
        let line = self.line;
        let mut kind = TokenName::LNumber;

        if self.cursor.peek() == Some(b'.') {
            kind = TokenName::DNumber;
            self.consume_len(1);
            self.consume_digit_sequence(is_decimal_digit);
            self.consume_exponent_if_present();
            return Token::new(
                TokenKind::Named(kind),
                TextRange::new(start, self.cursor.position()),
                line,
            );
        }

        if self.cursor.peek() == Some(b'0') {
            match self.cursor.peek_n(1) {
                Some(b'x' | b'X') if self.cursor.peek_n(2).is_some_and(is_hex_digit) => {
                    self.consume_len(2);
                    self.consume_digit_sequence(is_hex_digit);
                    return Token::new(
                        TokenKind::Named(kind),
                        TextRange::new(start, self.cursor.position()),
                        line,
                    );
                }
                Some(b'b' | b'B') if self.cursor.peek_n(2).is_some_and(is_binary_digit) => {
                    self.consume_len(2);
                    self.consume_digit_sequence(is_binary_digit);
                    return Token::new(
                        TokenKind::Named(kind),
                        TextRange::new(start, self.cursor.position()),
                        line,
                    );
                }
                Some(b'o' | b'O') if self.cursor.peek_n(2).is_some_and(is_octal_digit) => {
                    self.consume_len(2);
                    self.consume_digit_sequence(is_octal_digit);
                    return Token::new(
                        TokenKind::Named(kind),
                        TextRange::new(start, self.cursor.position()),
                        line,
                    );
                }
                _ => {}
            }
        }

        self.consume_digit_sequence(is_decimal_digit);

        if self.cursor.peek() == Some(b'.') {
            kind = TokenName::DNumber;
            self.consume_len(1);
            self.consume_digit_sequence(is_decimal_digit);
        }

        if self.consume_exponent_if_present() {
            kind = TokenName::DNumber;
        }

        Token::new(
            TokenKind::Named(kind),
            TextRange::new(start, self.cursor.position()),
            line,
        )
    }

    fn consume_digit_sequence(&mut self, predicate: fn(u8) -> bool) {
        if self.cursor.peek().is_some_and(predicate) {
            self.consume_len(1);
        }

        while let Some(byte) = self.cursor.peek() {
            if predicate(byte) {
                self.consume_len(1);
            } else if byte == b'_' && self.cursor.peek_n(1).is_some_and(predicate) {
                self.consume_len(2);
            } else {
                break;
            }
        }
    }

    fn consume_exponent_if_present(&mut self) -> bool {
        if !matches!(self.cursor.peek(), Some(b'e' | b'E')) {
            return false;
        }

        let digit_offset = match self.cursor.peek_n(1) {
            Some(b'+' | b'-') => 2,
            _ => 1,
        };

        if !self
            .cursor
            .peek_n(digit_offset)
            .is_some_and(is_decimal_digit)
        {
            return false;
        }

        self.consume_len(digit_offset);
        self.consume_digit_sequence(is_decimal_digit);
        true
    }

    fn cast_at_cursor(&self) -> Option<(TokenKind, usize)> {
        if self.cursor.peek() != Some(b'(') {
            return None;
        }

        let bytes = self.source.as_bytes();
        let mut offset = self.cursor.position() + 1;
        while bytes
            .get(offset)
            .is_some_and(|byte| is_php_whitespace(*byte))
        {
            offset += 1;
        }

        let name_start = offset;
        while bytes
            .get(offset)
            .is_some_and(|byte| byte.is_ascii_alphabetic())
        {
            offset += 1;
        }
        if offset == name_start {
            return None;
        }

        let name = &bytes[name_start..offset];
        while bytes
            .get(offset)
            .is_some_and(|byte| is_php_whitespace(*byte))
        {
            offset += 1;
        }

        if bytes.get(offset) != Some(&b')') {
            return None;
        }

        let name = if eq_ignore_ascii_case(name, b"int") || eq_ignore_ascii_case(name, b"integer") {
            TokenName::IntCast
        } else if eq_ignore_ascii_case(name, b"float")
            || eq_ignore_ascii_case(name, b"double")
            || eq_ignore_ascii_case(name, b"real")
        {
            TokenName::DoubleCast
        } else if eq_ignore_ascii_case(name, b"string") {
            TokenName::StringCast
        } else if eq_ignore_ascii_case(name, b"array") {
            TokenName::ArrayCast
        } else if eq_ignore_ascii_case(name, b"object") {
            TokenName::ObjectCast
        } else if eq_ignore_ascii_case(name, b"bool") || eq_ignore_ascii_case(name, b"boolean") {
            TokenName::BoolCast
        } else if eq_ignore_ascii_case(name, b"unset") {
            TokenName::UnsetCast
        } else if eq_ignore_ascii_case(name, b"void") {
            TokenName::VoidCast
        } else {
            return None;
        };

        Some((TokenKind::Named(name), offset + 1 - self.cursor.position()))
    }

    fn property_hook_visibility_at_cursor(&self) -> Option<(TokenKind, usize)> {
        const VISIBILITIES: &[(&[u8], TokenName)] = &[
            (b"protected(set)", TokenName::ProtectedSet),
            (b"private(set)", TokenName::PrivateSet),
            (b"public(set)", TokenName::PublicSet),
        ];

        for (text, name) in VISIBILITIES {
            if self.bytes_at_cursor_eq_ignore_ascii_case(text) {
                return Some((TokenKind::Named(*name), text.len()));
            }
        }

        None
    }

    fn ampersand_is_followed_by_var_or_vararg(&self) -> bool {
        let mut offset = 1;
        while self.cursor.peek_n(offset).is_some_and(is_php_whitespace) {
            offset += 1;
        }
        self.cursor.peek_n(offset) == Some(b'$')
            || (self.cursor.peek_n(offset) == Some(b'.')
                && self.cursor.peek_n(offset + 1) == Some(b'.')
                && self.cursor.peek_n(offset + 2) == Some(b'.'))
    }

    fn operator_at_cursor(&self) -> Option<(TokenKind, usize)> {
        const OPERATORS: &[(&[u8], TokenName)] = &[
            (b"??=", TokenName::CoalesceEqual),
            (b"?->", TokenName::NullsafeObjectOperator),
            (b"===", TokenName::IsIdentical),
            (b"!==", TokenName::IsNotIdentical),
            (b"<=>", TokenName::Spaceship),
            (b"<<=", TokenName::SlEqual),
            (b">>=", TokenName::SrEqual),
            (b"**=", TokenName::PowEqual),
            (b"|>", TokenName::Pipe),
            (b"==", TokenName::IsEqual),
            (b"!=", TokenName::IsNotEqual),
            (b"<>", TokenName::IsNotEqual),
            (b"<=", TokenName::IsSmallerOrEqual),
            (b">=", TokenName::IsGreaterOrEqual),
            (b"&&", TokenName::BooleanAnd),
            (b"||", TokenName::BooleanOr),
            (b"<<", TokenName::Sl),
            (b">>", TokenName::Sr),
            (b"+=", TokenName::PlusEqual),
            (b"-=", TokenName::MinusEqual),
            (b"*=", TokenName::MulEqual),
            (b"/=", TokenName::DivEqual),
            (b"%=", TokenName::ModEqual),
            (b".=", TokenName::ConcatEqual),
            (b"&=", TokenName::AndEqual),
            (b"|=", TokenName::OrEqual),
            (b"^=", TokenName::XorEqual),
            (b"??", TokenName::Coalesce),
            (b"->", TokenName::ObjectOperator),
            (b"::", TokenName::DoubleColon),
            (b"=>", TokenName::DoubleArrow),
            (b"++", TokenName::Inc),
            (b"--", TokenName::Dec),
            (b"...", TokenName::Ellipsis),
            (b"**", TokenName::Pow),
        ];

        for (text, name) in OPERATORS {
            if self.cursor.starts_with(text) {
                return Some((TokenKind::Named(*name), text.len()));
            }
        }

        None
    }

    fn heredoc_start_at_cursor(&self) -> Option<(usize, String, bool)> {
        if !self.cursor.starts_with(b"<<<") {
            return None;
        }

        let bytes = self.source.as_bytes();
        let start = self.cursor.position();
        let mut offset = start + 3;
        while matches!(bytes.get(offset), Some(b' ' | b'\t')) {
            offset += 1;
        }

        let mut nowdoc = false;
        let quote = match bytes.get(offset) {
            Some(b'\'' | b'"') => {
                nowdoc = bytes[offset] == b'\'';
                let quote = bytes[offset];
                offset += 1;
                Some(quote)
            }
            _ => None,
        };

        let label_start = offset;
        if !bytes
            .get(offset)
            .is_some_and(|byte| is_identifier_start(*byte))
        {
            return None;
        }
        offset += 1;
        while bytes
            .get(offset)
            .is_some_and(|byte| is_identifier_continue(*byte))
        {
            offset += 1;
        }
        let label_end = offset;

        if let Some(quote) = quote {
            if bytes.get(offset) != Some(&quote) {
                return None;
            }
            offset += 1;
        }

        let newline_len = match (bytes.get(offset), bytes.get(offset + 1)) {
            (Some(b'\r'), Some(b'\n')) => 2,
            (Some(b'\n' | b'\r'), _) => 1,
            _ => return None,
        };

        let label = std::str::from_utf8(&bytes[label_start..label_end])
            .ok()?
            .to_owned();
        Some((offset + newline_len - start, label, nowdoc))
    }

    fn heredoc_end_len_at_cursor(&self) -> Option<usize> {
        let label = self.heredoc_label.as_ref()?;
        let bytes = self.source.as_bytes();
        let start = self.cursor.position();
        if start > 0 && !matches!(bytes.get(start - 1), Some(b'\n' | b'\r')) {
            return None;
        }

        let mut offset = start;
        while matches!(bytes.get(offset), Some(b' ' | b'\t')) {
            offset += 1;
        }

        if bytes.get(offset..offset + label.len()) != Some(label.as_bytes()) {
            return None;
        }
        offset += label.len();

        match bytes.get(offset) {
            None | Some(b';' | b'\n' | b'\r') => Some(offset - start),
            _ => None,
        }
    }

    fn enter_encapsed_mode(&mut self, mode: LexerMode, start: usize, line: u32) {
        self.mode = mode;
        self.mode_start = start;
        self.mode_start_line = line;
        self.clear_encapsed_state();
    }

    fn clear_encapsed_state(&mut self) {
        self.expecting_object_property = false;
        self.expecting_string_varname = false;
        self.in_string_var_offset = false;
        self.in_braced_interpolation = false;
        self.last_encapsed_variable = false;
    }

    fn clear_heredoc_state(&mut self) {
        self.heredoc_label = None;
        self.heredoc_nowdoc = false;
    }

    fn open_tag_at_cursor(&self) -> Option<(TokenKind, usize)> {
        self.open_tag_at_offset(0)
    }

    fn open_tag_at_offset(&self, offset: usize) -> Option<(TokenKind, usize)> {
        if self.cursor_starts_with_at(offset, b"<?=") {
            return Some((TokenKind::Named(TokenName::OpenTagWithEcho), 3));
        }

        if self.cursor_starts_with_at(offset, b"<?php")
            || self.cursor_starts_with_at(offset, b"<?PHP")
        {
            let next = self.cursor.peek_n(offset + 5);
            if next.is_none_or(|byte| byte.is_ascii_whitespace()) {
                return Some((
                    TokenKind::Named(TokenName::OpenTag),
                    5 + php_tag_space_len(&self.cursor, offset + 5),
                ));
            }
        }

        if self.config.short_open_tag && self.cursor_starts_with_at(offset, b"<?") {
            return Some((
                TokenKind::Named(TokenName::OpenTag),
                2 + php_tag_space_len(&self.cursor, offset + 2),
            ));
        }

        None
    }

    fn cursor_starts_with_at(&self, offset: usize, needle: &[u8]) -> bool {
        needle
            .iter()
            .enumerate()
            .all(|(index, expected)| self.cursor.peek_n(offset + index) == Some(*expected))
    }

    fn word_at_cursor(&self, word: &[u8]) -> bool {
        self.word_at_offset(0, word)
    }

    fn word_at_offset(&self, offset: usize, word: &[u8]) -> bool {
        for (index, expected) in word.iter().enumerate() {
            let Some(actual) = self.cursor.peek_n(offset + index) else {
                return false;
            };
            if !actual.eq_ignore_ascii_case(expected) {
                return false;
            }
        }
        !self
            .cursor
            .peek_n(offset + word.len())
            .is_some_and(is_identifier_continue)
    }

    fn bytes_at_cursor_eq_ignore_ascii_case(&self, expected: &[u8]) -> bool {
        for (index, byte) in expected.iter().enumerate() {
            let Some(actual) = self.cursor.peek_n(index) else {
                return false;
            };
            if !actual.eq_ignore_ascii_case(byte) {
                return false;
            }
        }
        true
    }

    fn close_tag_len(&self) -> usize {
        match (self.cursor.peek_n(2), self.cursor.peek_n(3)) {
            (Some(b'\r'), Some(b'\n')) => 4,
            (Some(b'\n' | b'\r'), _) => 3,
            _ => 2,
        }
    }

    fn consume_len(&mut self, len: usize) {
        let start = self.cursor.position();
        let end = start.saturating_add(len).min(self.cursor.len());
        self.advance_line_for_range(start, end);
        for _ in start..end {
            let _ = self.cursor.bump();
        }
    }

    fn advance_line_for_range(&mut self, start: usize, end: usize) {
        let bytes = self.source.as_bytes();
        let mut index = start;
        while index < end {
            match bytes[index] {
                b'\r' => {
                    self.line += 1;
                    index += 1;
                }
                b'\n' if index == 0 || bytes[index - 1] != b'\r' => {
                    self.line += 1;
                    index += 1;
                }
                b'\n' => {
                    index += 1;
                }
                _ => {
                    index += 1;
                }
            }
        }
    }
}

fn php_tag_space_len(cursor: &crate::cursor::Cursor<'_>, offset: usize) -> usize {
    match (cursor.peek_n(offset), cursor.peek_n(offset + 1)) {
        (Some(b'\r'), Some(b'\n')) => 2,
        (Some(byte), _) if byte.is_ascii_whitespace() => 1,
        _ => 0,
    }
}

/// Lexes an entire source string with placeholder Lexer behavior.
#[must_use]
pub fn lex_all(source: &str, config: LexerConfig) -> LexResult {
    let mut lexer = Lexer::new(source, config);
    let mut tokens = Vec::new();

    while let Some(token) = lexer.next_token() {
        tokens.push(token);
    }

    LexResult {
        tokens,
        diagnostics: lexer.diagnostics,
    }
}

fn is_php_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn is_bad_character(byte: u8) -> bool {
    matches!(byte, 0x00..=0x08 | 0x0b | 0x0e..=0x1f | 0x7f)
}

fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic() || byte >= 0x80
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn is_decimal_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

fn is_hex_digit(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn is_binary_digit(byte: u8) -> bool {
    matches!(byte, b'0' | b'1')
}

fn is_octal_digit(byte: u8) -> bool {
    matches!(byte, b'0'..=b'7')
}

#[cfg(test)]
mod tests {
    use super::{Lexer, LexerConfig, lex_all};
    use crate::{LexDiagnosticKind, LexerMode, SymbolKind, TextRange, TokenKind, TokenName};

    #[test]
    fn empty_source_emits_no_tokens_by_default() {
        let result = lex_all("", LexerConfig::default());
        assert!(result.tokens.is_empty());
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn non_empty_source_emits_inline_html_placeholder() {
        let result = lex_all("hello\nworld", LexerConfig::default());
        assert_eq!(result.tokens.len(), 1);
        assert_eq!(
            result.tokens[0].kind,
            TokenKind::Named(TokenName::InlineHtml)
        );
        assert_eq!(result.tokens[0].range, TextRange::new(0, 11));
        assert_eq!(result.tokens[0].line, 1);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn html_with_special_characters_is_one_inline_html_token() {
        let source = "<div data-x=\"&amp;\">plain text</div>";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens.len(), 1);
        assert_eq!(
            result.tokens[0].kind,
            TokenKind::Named(TokenName::InlineHtml)
        );
        assert_eq!(result.tokens[0].text(source), Some(source));
    }

    #[test]
    fn php_open_tag_starts_scripting_mode() {
        let source = "<?php echo 1; ?>";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens[0].kind, TokenKind::Named(TokenName::OpenTag));
        assert_eq!(result.tokens[0].text(source), Some("<?php "));
        assert_eq!(result.tokens[0].line, 1);
    }

    #[test]
    fn utf8_bom_before_open_tag_does_not_emit_inline_html() {
        let source = "\u{feff}<?php echo 'ok';";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens[0].kind, TokenKind::Named(TokenName::OpenTag));
        assert_eq!(result.tokens[0].text(source), Some("<?php "));
        assert_eq!(result.tokens[0].range, TextRange::new(3, 9));
    }

    #[test]
    fn echo_open_tag_is_named_token() {
        let source = "<?= $x ?>";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(
            result.tokens[0].kind,
            TokenKind::Named(TokenName::OpenTagWithEcho)
        );
        assert_eq!(result.tokens[0].text(source), Some("<?="));
    }

    #[test]
    fn text_before_open_tag_is_inline_html() {
        let source = "before <?php ?>";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(
            result.tokens[0].kind,
            TokenKind::Named(TokenName::InlineHtml)
        );
        assert_eq!(result.tokens[0].text(source), Some("before "));
        assert_eq!(result.tokens[1].kind, TokenKind::Named(TokenName::OpenTag));
    }

    #[test]
    fn text_after_close_tag_returns_to_inline_html() {
        let source = "before <?php ?>after";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(
            result.tokens[0].kind,
            TokenKind::Named(TokenName::InlineHtml)
        );
        assert_eq!(result.tokens[1].kind, TokenKind::Named(TokenName::OpenTag));
        assert_eq!(result.tokens[2].kind, TokenKind::Named(TokenName::CloseTag));
        assert_eq!(
            result.tokens[3].kind,
            TokenKind::Named(TokenName::InlineHtml)
        );
        assert_eq!(result.tokens[3].text(source), Some("after"));
    }

    #[test]
    fn short_open_tag_requires_config() {
        let source = "<? echo 1; ?>";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens.len(), 1);
        assert_eq!(
            result.tokens[0].kind,
            TokenKind::Named(TokenName::InlineHtml)
        );

        let result = lex_all(
            source,
            LexerConfig {
                short_open_tag: true,
                ..LexerConfig::default()
            },
        );
        assert_eq!(result.tokens[0].kind, TokenKind::Named(TokenName::OpenTag));
    }

    #[test]
    fn tag_tokens_track_start_lines() {
        let source = "a\n<?php\r\n?>\nb";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens[0].line, 1);
        assert_eq!(result.tokens[1].line, 2);
        assert_eq!(result.tokens[1].text(source), Some("<?php\r\n"));
        assert_eq!(result.tokens[2].line, 3);
        assert_eq!(result.tokens[2].text(source), Some("?>\n"));
        assert_eq!(result.tokens[3].line, 4);
    }

    #[test]
    fn scripting_whitespace_is_grouped() {
        let source = "<?php \t\r\n;";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(
            result.tokens[1].kind,
            TokenKind::Named(TokenName::Whitespace)
        );
        assert_eq!(result.tokens[1].text(source), Some("\t\r\n"));
        assert_eq!(result.tokens[2].line, 2);
    }

    #[test]
    fn line_comment_stops_before_close_tag() {
        let source = "<?php // comment ?>\nafter";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens[1].kind, TokenKind::Named(TokenName::Comment));
        assert_eq!(result.tokens[1].text(source), Some("// comment "));
        assert_eq!(result.tokens[2].kind, TokenKind::Named(TokenName::CloseTag));
        assert_eq!(
            result.tokens[3].kind,
            TokenKind::Named(TokenName::InlineHtml)
        );
    }

    #[test]
    fn hash_comment_stops_before_newline() {
        let source = "<?php # hash\n;";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens[1].kind, TokenKind::Named(TokenName::Comment));
        assert_eq!(result.tokens[1].text(source), Some("# hash"));
        assert_eq!(
            result.tokens[2].kind,
            TokenKind::Named(TokenName::Whitespace)
        );
    }

    #[test]
    fn block_and_doc_comments_are_named() {
        let source = "<?php /** doc */ /* block */";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(
            result.tokens[1].kind,
            TokenKind::Named(TokenName::DocComment)
        );
        assert_eq!(result.tokens[1].text(source), Some("/** doc */"));
        assert_eq!(result.tokens[3].kind, TokenKind::Named(TokenName::Comment));
        assert_eq!(result.tokens[3].text(source), Some("/* block */"));
    }

    #[test]
    fn bad_control_character_emits_token_and_diagnostic() {
        let source = "<?php \u{0001};";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(
            result.tokens[1].kind,
            TokenKind::Named(TokenName::BadCharacter)
        );
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].kind, LexDiagnosticKind::BadCharacter);
    }

    #[test]
    fn unterminated_block_comment_recovers_to_eof() {
        let source = "<?php /* unterminated";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens[1].kind, TokenKind::Named(TokenName::Comment));
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].kind,
            LexDiagnosticKind::UnterminatedBlockComment
        );
    }

    #[test]
    fn keywords_are_case_insensitive() {
        let source = "<?php IF else Function CLASS namespace use match enum readonly fn yield";
        let result = lex_all(source, LexerConfig::default());
        let kinds: Vec<TokenKind> = result.tokens.iter().map(|token| token.kind).collect();
        assert!(kinds.contains(&TokenKind::Named(TokenName::If)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Else)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Function)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Class)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Namespace)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Use)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Match)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Enum)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Readonly)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Fn)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Yield)));
    }

    #[test]
    fn yield_from_is_combined_token() {
        let source = "<?php yield from $x;";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(
            result.tokens[1].kind,
            TokenKind::Named(TokenName::YieldFrom)
        );
        assert_eq!(result.tokens[1].text(source), Some("yield from"));
    }

    #[test]
    fn variables_and_invalid_dollar_are_robust() {
        let source = "<?php $name $ = 1;";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens[1].kind, TokenKind::Named(TokenName::Variable));
        assert_eq!(result.tokens[1].text(source), Some("$name"));
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Symbol(SymbolKind::Char(b'$')))
        );
    }

    #[test]
    fn namespace_names_are_lexical_tokens() {
        let source = "<?php \\A\\B Foo\\Bar namespace\\Thing";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(
            result.tokens[1].kind,
            TokenKind::Named(TokenName::NameFullyQualified)
        );
        assert_eq!(
            result.tokens[3].kind,
            TokenKind::Named(TokenName::NameQualified)
        );
        assert_eq!(
            result.tokens[5].kind,
            TokenKind::Named(TokenName::NameRelative)
        );
    }

    #[test]
    fn magic_constants_are_named_tokens() {
        let source = "<?php __LINE__ __FILE__ __DIR__ __CLASS__ __TRAIT__ __METHOD__ __FUNCTION__ __NAMESPACE__ __PROPERTY__";
        let result = lex_all(source, LexerConfig::default());
        let kinds: Vec<TokenKind> = result.tokens.iter().map(|token| token.kind).collect();
        assert!(kinds.contains(&TokenKind::Named(TokenName::Line)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::File)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::Dir)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::ClassC)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::TraitC)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::MethodC)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::FuncC)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::NamespaceC)));
        assert!(kinds.contains(&TokenKind::Named(TokenName::PropertyC)));
    }

    #[test]
    fn non_ascii_identifier_bytes_are_string_token() {
        let source = "<?php café";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(result.tokens[1].kind, TokenKind::Named(TokenName::String));
        assert_eq!(result.tokens[1].text(source), Some("café"));
    }

    #[test]
    fn integer_literals_include_prefixes_and_separators() {
        let source = "<?php 123 1_000 0x1f 0b1010 0o755 0123";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<&str> = result
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Named(TokenName::LNumber))
            .filter_map(|token| token.text(source))
            .collect();
        assert_eq!(
            texts,
            vec!["123", "1_000", "0x1f", "0b1010", "0o755", "0123"]
        );
    }

    #[test]
    fn float_literals_cover_dot_and_exponent_forms() {
        let source = "<?php 1. 1.0 .5 1e10 1e+10 1e-10 1_2.3_4";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<&str> = result
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Named(TokenName::DNumber))
            .filter_map(|token| token.text(source))
            .collect();
        assert_eq!(
            texts,
            vec!["1.", "1.0", ".5", "1e10", "1e+10", "1e-10", "1_2.3_4"]
        );
    }

    #[test]
    fn invalid_numeric_forms_split_like_reference_boundaries() {
        let source = "<?php 1e 0x 0b2 1__2";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<(&str, String)> = result
            .tokens
            .iter()
            .skip(1)
            .filter(|token| token.kind != TokenKind::Named(TokenName::Whitespace))
            .filter_map(|token| Some((token.text(source)?, token.reference_name())))
            .collect();
        assert_eq!(
            texts,
            vec![
                ("1", "T_LNUMBER".to_owned()),
                ("e", "T_STRING".to_owned()),
                ("0", "T_LNUMBER".to_owned()),
                ("x", "T_STRING".to_owned()),
                ("0", "T_LNUMBER".to_owned()),
                ("b2", "T_STRING".to_owned()),
                ("1", "T_LNUMBER".to_owned()),
                ("__2", "T_STRING".to_owned()),
            ]
        );
    }

    #[test]
    fn operators_use_longest_match_tokens() {
        let source = "<?php $a ??= $b ?? $c ? $d : $e; $x ?->y(); $f |> $g; $x === $y !== $z <=> $w; $x += 1; $x **= 2; $x->y; A::b(); ['x' => 1]; ++$i; --$i; foo(...$a); $a &= $b && $c;";
        let result = lex_all(source, LexerConfig::default());
        let kinds: Vec<String> = result
            .tokens
            .iter()
            .filter(|token| token.kind != TokenKind::Named(TokenName::Whitespace))
            .map(|token| token.reference_name())
            .collect();

        assert!(kinds.contains(&"T_COALESCE_EQUAL".to_owned()));
        assert!(kinds.contains(&"T_COALESCE".to_owned()));
        assert!(kinds.contains(&"?".to_owned()));
        assert!(kinds.contains(&":".to_owned()));
        assert!(kinds.contains(&"T_NULLSAFE_OBJECT_OPERATOR".to_owned()));
        assert!(kinds.contains(&"T_PIPE".to_owned()));
        assert!(kinds.contains(&"T_IS_IDENTICAL".to_owned()));
        assert!(kinds.contains(&"T_IS_NOT_IDENTICAL".to_owned()));
        assert!(kinds.contains(&"T_SPACESHIP".to_owned()));
        assert!(kinds.contains(&"T_PLUS_EQUAL".to_owned()));
        assert!(kinds.contains(&"T_POW_EQUAL".to_owned()));
        assert!(kinds.contains(&"T_OBJECT_OPERATOR".to_owned()));
        assert!(kinds.contains(&"T_DOUBLE_COLON".to_owned()));
        assert!(kinds.contains(&"T_DOUBLE_ARROW".to_owned()));
        assert!(kinds.contains(&"T_INC".to_owned()));
        assert!(kinds.contains(&"T_DEC".to_owned()));
        assert!(kinds.contains(&"T_ELLIPSIS".to_owned()));
        assert!(kinds.contains(&"T_AND_EQUAL".to_owned()));
        assert!(kinds.contains(&"T_BOOLEAN_AND".to_owned()));
    }

    #[test]
    fn casts_attributes_ampersands_and_property_hook_tokens_are_named() {
        let source = "<?php (void) $x; ( int ) $y; (boolean)$z; #[Attr] &$x; &foo(); public(set) protected(set) private(set); and or xor";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<(&str, String)> = result
            .tokens
            .iter()
            .filter(|token| token.kind != TokenKind::Named(TokenName::Whitespace))
            .filter_map(|token| Some((token.text(source)?, token.reference_name())))
            .collect();

        assert!(texts.contains(&("(void)", "T_VOID_CAST".to_owned())));
        assert!(texts.contains(&("( int )", "T_INT_CAST".to_owned())));
        assert!(texts.contains(&("(boolean)", "T_BOOL_CAST".to_owned())));
        assert!(texts.contains(&("#[", "T_ATTRIBUTE".to_owned())));
        assert!(texts.contains(&("&", "T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG".to_owned())));
        assert!(texts.contains(&("&", "T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG".to_owned())));
        assert!(texts.contains(&("public(set)", "T_PUBLIC_SET".to_owned())));
        assert!(texts.contains(&("protected(set)", "T_PROTECTED_SET".to_owned())));
        assert!(texts.contains(&("private(set)", "T_PRIVATE_SET".to_owned())));
        assert!(texts.contains(&("and", "T_LOGICAL_AND".to_owned())));
        assert!(texts.contains(&("or", "T_LOGICAL_OR".to_owned())));
        assert!(texts.contains(&("xor", "T_LOGICAL_XOR".to_owned())));
    }

    #[test]
    fn constant_strings_include_quotes_in_token_text() {
        let source = "<?php 'abc' 'it\\'s' '\\\\' \"abc\" \"\\n\"";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<&str> = result
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Named(TokenName::ConstantEncapsedString))
            .filter_map(|token| token.text(source))
            .collect();
        assert_eq!(
            texts,
            vec!["'abc'", "'it\\'s'", "'\\\\'", "\"abc\"", "\"\\n\""]
        );
    }

    #[test]
    fn double_quoted_interpolation_is_not_constant_string_yet() {
        let source = "<?php \"$x\"";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<(&str, String)> = result
            .tokens
            .iter()
            .skip(1)
            .filter_map(|token| Some((token.text(source)?, token.reference_name())))
            .collect();
        assert_eq!(
            texts,
            vec![
                ("\"", "\"".to_owned()),
                ("$x", "T_VARIABLE".to_owned()),
                ("\"", "\"".to_owned()),
            ]
        );
        assert!(
            !result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Named(TokenName::ConstantEncapsedString))
        );
    }

    #[test]
    fn string_newlines_update_following_token_line() {
        let source = "<?php 'a\nb'\n$x";
        let result = lex_all(source, LexerConfig::default());
        let string = result
            .tokens
            .iter()
            .find(|token| token.kind == TokenKind::Named(TokenName::ConstantEncapsedString))
            .unwrap();
        let variable = result
            .tokens
            .iter()
            .find(|token| token.kind == TokenKind::Named(TokenName::Variable))
            .unwrap();
        assert_eq!(string.line, 1);
        assert_eq!(variable.line, 3);
    }

    #[test]
    fn unterminated_string_emits_diagnostic_and_terminates() {
        let source = "<?php 'unterminated";
        let result = lex_all(source, LexerConfig::default());
        assert_eq!(
            result.tokens[1].kind,
            TokenKind::Named(TokenName::ConstantEncapsedString)
        );
        assert_eq!(result.tokens[1].text(source), Some("'unterminated"));
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].kind,
            LexDiagnosticKind::UnterminatedString
        );
    }

    #[test]
    fn double_quoted_encapsed_strings_emit_simple_interpolation_tokens() {
        let source = "<?php \"hello $name\" \"$obj->prop\" \"$arr[0]\" \"{$name}\" \"${name}\"";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<(&str, String)> = result
            .tokens
            .iter()
            .filter(|token| token.kind != TokenKind::Named(TokenName::Whitespace))
            .filter_map(|token| Some((token.text(source)?, token.reference_name())))
            .collect();

        assert!(texts.contains(&("hello ", "T_ENCAPSED_AND_WHITESPACE".to_owned())));
        assert!(texts.contains(&("$name", "T_VARIABLE".to_owned())));
        assert!(texts.contains(&("->", "T_OBJECT_OPERATOR".to_owned())));
        assert!(texts.contains(&("prop", "T_STRING".to_owned())));
        assert!(texts.contains(&("[", "[".to_owned())));
        assert!(texts.contains(&("0", "T_NUM_STRING".to_owned())));
        assert!(texts.contains(&("]", "]".to_owned())));
        assert!(texts.contains(&("{", "T_CURLY_OPEN".to_owned())));
        assert!(texts.contains(&("${", "T_DOLLAR_OPEN_CURLY_BRACES".to_owned())));
        assert!(texts.contains(&("name", "T_STRING_VARNAME".to_owned())));
    }

    #[test]
    fn backtick_encapsed_strings_use_same_simple_interpolation() {
        let source = "<?php `echo $name`";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<(&str, String)> = result
            .tokens
            .iter()
            .skip(1)
            .filter_map(|token| Some((token.text(source)?, token.reference_name())))
            .collect();
        assert_eq!(
            texts,
            vec![
                ("`", "`".to_owned()),
                ("echo ", "T_ENCAPSED_AND_WHITESPACE".to_owned()),
                ("$name", "T_VARIABLE".to_owned()),
                ("`", "`".to_owned()),
            ]
        );
    }

    #[test]
    fn unterminated_encapsed_string_emits_diagnostic() {
        let source = "<?php \"hello $name";
        let result = lex_all(source, LexerConfig::default());
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Named(TokenName::EncapsedAndWhitespace))
        );
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Named(TokenName::Variable))
        );
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].kind,
            LexDiagnosticKind::UnterminatedString
        );
    }

    #[test]
    fn encapsed_scanner_makes_progress_on_punctuation_and_escapes() {
        let source = "<?php \"\\$not_var { [ text $yes\"";
        let result = lex_all(source, LexerConfig::default());
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Named(TokenName::Variable))
        );
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn heredoc_emits_start_content_interpolation_and_end_tokens() {
        let source = "<?php\n$a = <<<TXT\nhello $name\nTXT;\n";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<(&str, String)> = result
            .tokens
            .iter()
            .filter(|token| token.kind != TokenKind::Named(TokenName::Whitespace))
            .filter_map(|token| Some((token.text(source)?, token.reference_name())))
            .collect();

        assert!(texts.contains(&("<<<TXT\n", "T_START_HEREDOC".to_owned())));
        assert!(texts.contains(&("hello ", "T_ENCAPSED_AND_WHITESPACE".to_owned())));
        assert!(texts.contains(&("$name", "T_VARIABLE".to_owned())));
        assert!(texts.contains(&("\n", "T_ENCAPSED_AND_WHITESPACE".to_owned())));
        assert!(texts.contains(&("TXT", "T_END_HEREDOC".to_owned())));
        assert!(texts.contains(&(";", ";".to_owned())));
    }

    #[test]
    fn nowdoc_does_not_interpolate_variables() {
        let source = "<?php\n$a = <<<'TXT'\nhello $name\nTXT;\n";
        let result = lex_all(source, LexerConfig::default());
        let texts: Vec<(&str, String)> = result
            .tokens
            .iter()
            .filter(|token| token.kind != TokenKind::Named(TokenName::Whitespace))
            .filter_map(|token| Some((token.text(source)?, token.reference_name())))
            .collect();

        assert!(texts.contains(&("<<<'TXT'\n", "T_START_HEREDOC".to_owned())));
        assert!(texts.contains(&("hello $name\n", "T_ENCAPSED_AND_WHITESPACE".to_owned())));
        assert!(!texts.contains(&("$name", "T_VARIABLE".to_owned())));
        assert!(texts.contains(&("TXT", "T_END_HEREDOC".to_owned())));
    }

    #[test]
    fn indented_heredoc_end_marker_includes_indent() {
        let source = "<?php\n$a = <<<TXT\n    indented\n    TXT;\n";
        let result = lex_all(source, LexerConfig::default());
        assert!(result.tokens.iter().any(|token| {
            token.kind == TokenKind::Named(TokenName::EndHeredoc)
                && token.text(source) == Some("    TXT")
        }));
    }

    #[test]
    fn unterminated_heredoc_emits_diagnostic() {
        let source = "<?php\n$a = <<<TXT\nhello $name\n";
        let result = lex_all(source, LexerConfig::default());
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Named(TokenName::StartHeredoc))
        );
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].kind,
            LexDiagnosticKind::UnterminatedHeredoc
        );
    }

    #[test]
    fn very_long_number_does_not_panic() {
        let source = format!("<?php {}", "1".repeat(10_000));
        let result = lex_all(&source, LexerConfig::default());
        assert_eq!(result.tokens[1].kind, TokenKind::Named(TokenName::LNumber));
        assert_eq!(result.tokens[1].range.len(), 10_000);
    }

    #[test]
    fn non_ascii_source_uses_byte_spans() {
        let result = lex_all("é", LexerConfig::default());
        assert_eq!(result.tokens[0].range, TextRange::new(0, 2));
    }

    #[test]
    fn eof_token_is_optional() {
        let result = lex_all(
            "",
            LexerConfig {
                emit_eof: true,
                ..LexerConfig::default()
            },
        );
        assert_eq!(result.tokens.len(), 1);
        assert_eq!(result.tokens[0].kind, TokenKind::Eof);
        assert_eq!(result.tokens[0].range, TextRange::new(0, 0));
    }

    #[test]
    fn lexer_exposes_mode_and_offset() {
        let mut lexer = Lexer::new("abc", LexerConfig::default());
        assert_eq!(lexer.mode(), LexerMode::InlineHtml);
        assert_eq!(lexer.offset(), 0);
        assert_eq!(
            lexer.next_token().map(|token| token.kind),
            Some(TokenKind::Named(TokenName::InlineHtml))
        );
        assert_eq!(lexer.offset(), 3);
        assert_eq!(lexer.next_token(), None);
    }
}
