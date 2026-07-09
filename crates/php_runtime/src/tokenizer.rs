//! Tokenizer extension helpers backed by the Lexer lexer.

use crate::{
    ArrayKey, BuiltinError, PhpArray, PhpReferenceClassification, RuntimeDiagnostic,
    RuntimeSeverity, RuntimeSourceSpan, Value,
};
use php_lexer::{
    LexDiagnosticKind, LexerConfig, TOKENIZER_TOKEN_ID_BASE, TOKENIZER_TOKEN_NAMES, Token,
    TokenKind, TokenName, lex_all,
};
use php_syntax::{ParseDiagnostic, ParseDiagnosticId, parse_source_file};

/// PHP `TOKEN_PARSE` flag value for the tokenizer extension.
pub const TOKEN_PARSE: i64 = 1;

/// One token returned by the tokenizer bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenizerToken {
    /// Local tokenizer token ID, or the byte value for symbol tokens.
    pub id: i64,
    /// Normalized token name, or the symbol text for symbol tokens.
    pub name: String,
    /// Original token text.
    pub text: String,
    /// One-based source line.
    pub line: u32,
    /// Zero-based source byte offset.
    pub pos: u32,
    /// Whether PHP's `token_get_all` returns an array for this token.
    pub named: bool,
    /// Named token kind, when available.
    pub token_name: Option<TokenName>,
}

/// Tokenizer result plus PHP-visible non-fatal diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenizeResult {
    /// Tokens produced from the input source.
    pub tokens: Vec<TokenizerToken>,
    /// Non-fatal diagnostics that callers must route through PHP error handling.
    pub diagnostics: Vec<RuntimeDiagnostic>,
}

/// Returns the local tokenizer ID for a named token.
#[must_use]
pub fn token_name_id(name: TokenName) -> i64 {
    TOKENIZER_TOKEN_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map_or(TOKENIZER_TOKEN_ID_BASE, |index| {
            TOKENIZER_TOKEN_ID_BASE + index as i64
        })
}

/// Resolves a local tokenizer ID to its PHP token name.
#[must_use]
pub fn token_name_for_id(id: i64) -> Option<&'static str> {
    token_name_kind_for_id(id).map(|name| name.as_php_name())
}

/// Resolves a local tokenizer ID to the lexer token kind.
#[must_use]
pub fn token_name_kind_for_id(id: i64) -> Option<TokenName> {
    if id < TOKENIZER_TOKEN_ID_BASE {
        return None;
    }
    let index = (id - TOKENIZER_TOKEN_ID_BASE) as usize;
    TOKENIZER_TOKEN_NAMES.get(index).copied()
}

/// Returns whether a local tokenizer ID is ignorable for `PhpToken::isIgnorable`.
#[must_use]
pub fn is_ignorable_id(id: i64) -> bool {
    is_ignorable_name(token_name_kind_for_id(id))
}

/// Tokenizes source code with the existing Lexer lexer.
pub fn tokenize(source: &str, flags: i64) -> Result<Vec<TokenizerToken>, BuiltinError> {
    tokenize_with_diagnostics(source, flags).map(|result| result.tokens)
}

/// Tokenizes source code and preserves PHP-visible non-fatal diagnostics.
pub fn tokenize_with_diagnostics(source: &str, flags: i64) -> Result<TokenizeResult, BuiltinError> {
    let token_parse = flags & TOKEN_PARSE != 0;
    let config = LexerConfig {
        token_parse,
        ..LexerConfig::default()
    };
    let result = lex_all(source, config);
    if let Some(diagnostic) = result
        .diagnostics
        .iter()
        .find(|diagnostic| !is_recoverable_tokenizer_diagnostic(diagnostic.kind, token_parse))
    {
        if token_parse {
            return Err(token_parse_lex_error(source, diagnostic));
        }
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_TOKENIZER_LEX",
            format!("{diagnostic:?}"),
        ));
    }
    if token_parse {
        validate_token_parse_literals(source, &result.tokens)?;
        validate_token_parse_heredoc_indentation(source, &result.tokens)?;
    }
    let mut tokens = result
        .tokens
        .into_iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| tokenizer_token(source, &token))
        .collect::<Vec<_>>();
    if token_parse {
        validate_token_parse_source(source)?;
        apply_token_parse_reclassification(&mut tokens);
    }
    let diagnostics = if token_parse {
        token_parse_deprecations(&tokens)
    } else {
        Vec::new()
    };
    Ok(TokenizeResult {
        tokens,
        diagnostics,
    })
}

fn is_recoverable_tokenizer_diagnostic(kind: LexDiagnosticKind, token_parse: bool) -> bool {
    matches!(kind, LexDiagnosticKind::BadCharacter)
        || (!token_parse && matches!(kind, LexDiagnosticKind::UnterminatedHeredoc))
}

/// Converts tokenizer output to `token_get_all` array/string shape.
#[must_use]
pub fn token_get_all_value(tokens: Vec<TokenizerToken>) -> Value {
    Value::packed_array(tokens.into_iter().map(token_get_all_entry).collect())
}

/// Returns whether a token is ignorable for `PhpToken::isIgnorable`.
#[must_use]
pub fn is_ignorable_name(name: Option<TokenName>) -> bool {
    matches!(
        name,
        Some(
            TokenName::OpenTag | TokenName::Whitespace | TokenName::Comment | TokenName::DocComment
        )
    )
}

fn tokenizer_token(source: &str, token: &Token) -> TokenizerToken {
    let text = token.text(source).unwrap_or_default().to_owned();
    let pos = token.range.start().to_usize() as u32;
    match token.kind {
        TokenKind::Named(name) => TokenizerToken {
            id: token_name_id(name),
            name: name.as_php_name().to_owned(),
            text,
            line: token.line,
            pos,
            named: true,
            token_name: Some(name),
        },
        TokenKind::Symbol(symbol) => {
            let name = symbol.reference_name();
            let id = text.as_bytes().first().copied().map_or(0, i64::from);
            TokenizerToken {
                id,
                name,
                text,
                line: token.line,
                pos,
                named: false,
                token_name: None,
            }
        }
        TokenKind::Eof => TokenizerToken {
            id: 0,
            name: "EOF".to_owned(),
            text,
            line: token.line,
            pos,
            named: false,
            token_name: None,
        },
    }
}

fn apply_token_parse_reclassification(tokens: &mut [TokenizerToken]) {
    let mut expect_member_name = false;
    let mut expect_const_name = false;
    let mut const_declaration_name = false;

    for index in 0..tokens.len() {
        let name = tokens[index].token_name;

        if matches!(
            name,
            Some(
                TokenName::DoubleColon
                    | TokenName::ObjectOperator
                    | TokenName::NullsafeObjectOperator
            )
        ) {
            expect_member_name = true;
            continue;
        }

        if expect_member_name {
            if is_trivia_name(name) {
                continue;
            }
            if is_identifier_like_token(&tokens[index]) {
                reclassify_as_string(&mut tokens[index]);
            }
            expect_member_name = false;
            continue;
        }

        if expect_const_name {
            if is_trivia_name(name) {
                continue;
            }
            if is_identifier_like_token(&tokens[index]) {
                reclassify_as_string(&mut tokens[index]);
            }
            expect_const_name = false;
            continue;
        }

        if name == Some(TokenName::Const) {
            expect_const_name = true;
            const_declaration_name = true;
            continue;
        }

        if const_declaration_name {
            if tokens[index].text == "=" || tokens[index].text == ";" {
                const_declaration_name = false;
            } else if tokens[index].text == "," {
                expect_const_name = true;
            }
        }

        if name == Some(TokenName::Namespace)
            && next_significant_name(tokens, index) == Some(TokenName::As)
        {
            reclassify_as_string(&mut tokens[index]);
        }
    }
}

fn next_significant_name(tokens: &[TokenizerToken], index: usize) -> Option<TokenName> {
    tokens
        .iter()
        .skip(index + 1)
        .find(|token| !is_trivia_name(token.token_name))
        .and_then(|token| token.token_name)
}

fn is_trivia_name(name: Option<TokenName>) -> bool {
    matches!(
        name,
        Some(TokenName::Whitespace | TokenName::Comment | TokenName::DocComment)
    )
}

fn is_identifier_like_token(token: &TokenizerToken) -> bool {
    token.named
        && token
            .text
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_' || *byte >= 0x80)
}

fn reclassify_as_string(token: &mut TokenizerToken) {
    token.id = token_name_id(TokenName::String);
    token.name = TokenName::String.as_php_name().to_owned();
    token.token_name = Some(TokenName::String);
}

fn token_parse_deprecations(tokens: &[TokenizerToken]) -> Vec<RuntimeDiagnostic> {
    tokens
        .iter()
        .filter_map(non_canonical_cast_deprecation)
        .collect()
}

fn non_canonical_cast_deprecation(token: &TokenizerToken) -> Option<RuntimeDiagnostic> {
    let cast = token_text_cast_name(&token.text)?;
    let replacement = match (token.token_name, cast.as_str()) {
        (Some(TokenName::IntCast), "integer") => "int",
        (Some(TokenName::DoubleCast), "double") => "float",
        (Some(TokenName::BoolCast), "boolean") => "bool",
        _ => return None,
    };
    Some(RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_TOKENIZER_NON_CANONICAL_CAST_DEPRECATED",
        RuntimeSeverity::Deprecation,
        format!("Non-canonical cast ({cast}) is deprecated, use the ({replacement}) cast instead"),
        RuntimeSourceSpan {
            file: None,
            start: token.line,
            end: token.line,
        },
        Vec::new(),
        Some(PhpReferenceClassification::Deprecation),
    ))
}

fn token_text_cast_name(text: &str) -> Option<String> {
    let inner = text.trim().strip_prefix('(')?.strip_suffix(')')?.trim();
    inner
        .bytes()
        .all(|byte| byte.is_ascii_alphabetic())
        .then(|| inner.to_ascii_lowercase())
}

fn validate_token_parse_source(source: &str) -> Result<(), BuiltinError> {
    let parse = parse_source_file(source);
    if let Some(diagnostic) = parse.diagnostics().first() {
        return Err(token_parse_error(token_parse_diagnostic_message(
            source, diagnostic,
        )));
    }
    Ok(())
}

fn validate_token_parse_literals(source: &str, tokens: &[Token]) -> Result<(), BuiltinError> {
    for token in tokens {
        let TokenKind::Named(name) = token.kind else {
            continue;
        };
        let text = token.text(source).unwrap_or_default();
        match name {
            TokenName::LNumber if is_invalid_legacy_octal_literal(text) => {
                return Err(token_parse_error("Invalid numeric literal"));
            }
            TokenName::ConstantEncapsedString => validate_unicode_codepoint_escapes(text)?,
            _ => {}
        }
    }
    Ok(())
}

fn validate_token_parse_heredoc_indentation(
    source: &str,
    tokens: &[Token],
) -> Result<(), BuiltinError> {
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].kind != TokenKind::Named(TokenName::StartHeredoc) {
            index += 1;
            continue;
        }

        let heredoc_start = index;
        let Some(end_index) =
            tokens
                .iter()
                .enumerate()
                .skip(index + 1)
                .find_map(|(candidate, token)| {
                    (token.kind == TokenKind::Named(TokenName::EndHeredoc)).then_some(candidate)
                })
        else {
            index += 1;
            continue;
        };

        let end_text = tokens[end_index].text(source).unwrap_or_default();
        let indent = leading_heredoc_indent(end_text.as_bytes());
        if heredoc_indent_is_mixed(indent) {
            return Err(token_parse_error_at_line(
                "Invalid indentation - tabs and spaces cannot be mixed",
                first_heredoc_body_line(tokens, heredoc_start, end_index),
            ));
        }
        if !indent.is_empty() {
            for token in &tokens[heredoc_start + 1..end_index] {
                if token.kind != TokenKind::Named(TokenName::EncapsedAndWhitespace) {
                    continue;
                }
                let text = token.text(source).unwrap_or_default();
                if let Some(line_offset) =
                    first_underindented_heredoc_body_line(text.as_bytes(), indent)
                {
                    return Err(token_parse_error_at_line(
                        format!(
                            "Invalid body indentation level (expecting an indentation level of at least {})",
                            indent.len()
                        ),
                        i64::from(token.line) + line_offset as i64,
                    ));
                }
            }
        }
        index = end_index + 1;
    }
    Ok(())
}

fn leading_heredoc_indent(text: &[u8]) -> &[u8] {
    let len = text
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    &text[..len]
}

fn heredoc_indent_is_mixed(indent: &[u8]) -> bool {
    indent.contains(&b' ') && indent.contains(&b'\t')
}

fn first_heredoc_body_line(tokens: &[Token], start_index: usize, end_index: usize) -> i64 {
    tokens[start_index + 1..end_index]
        .iter()
        .find(|token| token.kind == TokenKind::Named(TokenName::EncapsedAndWhitespace))
        .map_or_else(
            || i64::from(tokens[start_index].line) + 1,
            |token| i64::from(token.line),
        )
}

fn first_underindented_heredoc_body_line(text: &[u8], indent: &[u8]) -> Option<usize> {
    let mut line_offset = 0;
    let mut line_start = 0;
    while line_start < text.len() {
        let line_end = text[line_start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(text.len(), |offset| line_start + offset + 1);
        let line = &text[line_start..line_end];
        if !heredoc_body_line_is_blank(line) && !line.starts_with(indent) {
            return Some(line_offset);
        }
        line_start = line_end;
        line_offset += 1;
    }
    None
}

fn heredoc_body_line_is_blank(line: &[u8]) -> bool {
    line.iter()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
}

fn is_invalid_legacy_octal_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() <= 1 || bytes.first() != Some(&b'0') {
        return false;
    }
    if matches!(bytes.get(1), Some(b'x' | b'X' | b'b' | b'B' | b'o' | b'O')) {
        return false;
    }
    bytes.iter().skip(1).any(|byte| matches!(byte, b'8' | b'9'))
}

fn validate_unicode_codepoint_escapes(text: &str) -> Result<(), BuiltinError> {
    let bytes = text.as_bytes();
    let mut offset = 0;
    while let Some(relative) = find_unicode_escape_start(&bytes[offset..]) {
        let start = offset + relative + 3;
        let Some(close_relative) = bytes[start..].iter().position(|byte| *byte == b'}') else {
            return Err(token_parse_error("Invalid UTF-8 codepoint escape sequence"));
        };
        let body = &bytes[start..start + close_relative];
        let Some(codepoint) = parse_unicode_escape_body(body) else {
            return Err(token_parse_error("Invalid UTF-8 codepoint escape sequence"));
        };
        if codepoint > 0x10_FFFF {
            return Err(token_parse_error(
                "Invalid UTF-8 codepoint escape sequence: Codepoint too large",
            ));
        }
        offset = start + close_relative + 1;
    }
    Ok(())
}

fn find_unicode_escape_start(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(3)
        .position(|window| window == [b'\\', b'u', b'{'])
}

fn parse_unicode_escape_body(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0_u32;
    for byte in bytes {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte - b'0'),
            b'a'..=b'f' => u32::from(byte - b'a' + 10),
            b'A'..=b'F' => u32::from(byte - b'A' + 10),
            _ => return None,
        };
        value = value.saturating_mul(16).saturating_add(digit);
    }
    Some(value)
}

fn token_parse_diagnostic_message(source: &str, diagnostic: &ParseDiagnostic) -> String {
    if diagnostic.id == ParseDiagnosticId::ExpectedToken
        && diagnostic.expected.iter().any(|expected| expected == ";")
        && let Some(identifier) = diagnostic_identifier_text(source, diagnostic)
    {
        return format!("syntax error, unexpected identifier \"{identifier}\"");
    }
    diagnostic.message.clone()
}

fn diagnostic_identifier_text<'source>(
    source: &'source str,
    diagnostic: &ParseDiagnostic,
) -> Option<&'source str> {
    let start = diagnostic.span.start().to_usize();
    let end = diagnostic.span.end().to_usize();
    let text = source.get(start..end)?;
    let mut chars = text.chars();
    let first = chars.next()?;
    if !(first == '_' || first.is_ascii_alphabetic() || !first.is_ascii()) {
        return None;
    }
    chars
        .all(|ch| ch == '_' || ch.is_ascii_alphanumeric() || !ch.is_ascii())
        .then_some(text)
}

fn token_parse_error(message: impl Into<String>) -> BuiltinError {
    BuiltinError::new("E_PHP_RUNTIME_TOKENIZER_PARSE", message)
}

fn token_parse_error_at_line(message: impl Into<String>, line: i64) -> BuiltinError {
    token_parse_error(message).with_tokenizer_parse_line(line)
}

fn token_parse_lex_error(source: &str, diagnostic: &php_lexer::LexDiagnostic) -> BuiltinError {
    if diagnostic.kind == LexDiagnosticKind::UnterminatedHeredoc {
        return token_parse_error_at_line(
            "syntax error, unexpected end of file, expecting variable or heredoc end or \"${\" or \"{$\"",
            source_line_count(source),
        );
    }
    token_parse_error_at_line(diagnostic.message.clone(), diagnostic.line as i64)
}

fn source_line_count(source: &str) -> i64 {
    1 + source
        .as_bytes()
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as i64
}

fn token_get_all_entry(token: TokenizerToken) -> Value {
    if !token.named {
        return Value::string(token.text.into_bytes());
    }
    let mut entry = PhpArray::new();
    entry.insert(ArrayKey::Int(0), Value::Int(token.id));
    entry.insert(ArrayKey::Int(1), Value::string(token.text.into_bytes()));
    entry.insert(ArrayKey::Int(2), Value::Int(i64::from(token.line)));
    Value::Array(entry)
}

#[cfg(test)]
mod tests {
    use super::{TOKEN_PARSE, tokenize};
    use php_lexer::TokenName;

    fn significant_names_and_text(source: &str) -> Vec<(TokenName, String)> {
        tokenize(source, TOKEN_PARSE)
            .expect("tokenize source")
            .into_iter()
            .filter_map(|token| {
                let name = token.token_name?;
                (name != TokenName::Whitespace).then_some((name, token.text))
            })
            .collect()
    }

    #[test]
    fn token_parse_reclassifies_member_names_as_strings() {
        let tokens = significant_names_and_text("<?php X::continue; $x->__halt_compiler();");

        assert!(tokens.windows(3).any(|window| window
            == [
                (TokenName::String, "X".to_owned()),
                (TokenName::DoubleColon, "::".to_owned()),
                (TokenName::String, "continue".to_owned()),
            ]));
        assert!(tokens.windows(3).any(|window| window
            == [
                (TokenName::Variable, "$x".to_owned()),
                (TokenName::ObjectOperator, "->".to_owned()),
                (TokenName::String, "__halt_compiler".to_owned()),
            ]));
    }

    #[test]
    fn token_parse_reclassifies_const_names_as_strings() {
        let tokens = significant_names_and_text(
            "<?php class C { const CONST = 1; const CONTINUE = self::CONST; const ARRAY = []; }",
        );

        assert!(tokens.contains(&(TokenName::String, "CONST".to_owned())));
        assert!(tokens.contains(&(TokenName::String, "CONTINUE".to_owned())));
        assert!(tokens.contains(&(TokenName::String, "ARRAY".to_owned())));
        assert!(tokens.windows(3).any(|window| window
            == [
                (TokenName::String, "self".to_owned()),
                (TokenName::DoubleColon, "::".to_owned()),
                (TokenName::String, "CONST".to_owned()),
            ]));
    }

    #[test]
    fn token_parse_reclassifies_namespace_trait_alias_as_string() {
        let tokens = significant_names_and_text("<?php class C { use A { namespace as bar; } }");

        assert!(tokens.windows(3).any(|window| window
            == [
                (TokenName::String, "namespace".to_owned()),
                (TokenName::As, "as".to_owned()),
                (TokenName::String, "bar".to_owned()),
            ]));
    }

    #[test]
    fn token_parse_reports_parser_diagnostics_as_parse_errors() {
        let error = tokenize("<?php invalid code;", TOKEN_PARSE).expect_err("parse error");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_TOKENIZER_PARSE");
        assert_eq!(
            error.message(),
            "syntax error, unexpected identifier \"code\""
        );
    }

    #[test]
    fn token_parse_reports_invalid_legacy_octal_literals() {
        let error = tokenize("<?php var_dump(078);", TOKEN_PARSE).expect_err("parse error");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_TOKENIZER_PARSE");
        assert_eq!(error.message(), "Invalid numeric literal");
    }

    #[test]
    fn token_parse_reports_invalid_unicode_codepoint_escapes() {
        let error =
            tokenize("<?php var_dump(\"\\u{xyz}\");", TOKEN_PARSE).expect_err("parse error");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_TOKENIZER_PARSE");
        assert_eq!(error.message(), "Invalid UTF-8 codepoint escape sequence");
    }

    #[test]
    fn token_parse_reports_too_large_unicode_codepoint_escapes() {
        let error =
            tokenize("<?php var_dump(\"\\u{ffffff}\");", TOKEN_PARSE).expect_err("parse error");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_TOKENIZER_PARSE");
        assert_eq!(
            error.message(),
            "Invalid UTF-8 codepoint escape sequence: Codepoint too large"
        );
    }

    #[test]
    fn tokenizer_recovers_unterminated_heredoc_without_token_parse() {
        let tokens = tokenize("<?php <<<TXT\nhello", 0).expect("partial heredoc tokens");

        assert!(
            tokens
                .iter()
                .any(|token| token.token_name == Some(TokenName::StartHeredoc))
        );
        assert!(
            tokens
                .iter()
                .any(|token| token.token_name == Some(TokenName::EncapsedAndWhitespace))
        );
    }

    #[test]
    fn token_parse_reports_unterminated_heredoc_as_parse_error() {
        let error = tokenize("<?php <<<TXT\nhello", TOKEN_PARSE).expect_err("parse error");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_TOKENIZER_PARSE");
        assert_eq!(
            error.message(),
            "syntax error, unexpected end of file, expecting variable or heredoc end or \"${\" or \"{$\""
        );
        assert_eq!(
            error
                .context()
                .and_then(|context| context.tokenizer_parse_line),
            Some(2)
        );
    }

    #[test]
    fn token_parse_reports_mixed_heredoc_indentation() {
        let error =
            tokenize("<?php\n \t<<<'DOC'\n \tXXX\n \tDOC;", TOKEN_PARSE).expect_err("parse error");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_TOKENIZER_PARSE");
        assert_eq!(
            error.message(),
            "Invalid indentation - tabs and spaces cannot be mixed"
        );
        assert_eq!(
            error
                .context()
                .and_then(|context| context.tokenizer_parse_line),
            Some(3)
        );
    }

    #[test]
    fn token_parse_reports_underindented_heredoc_body() {
        let error = tokenize("<?php <<<TXT\nabc\n   TXT;", TOKEN_PARSE).expect_err("parse error");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_TOKENIZER_PARSE");
        assert_eq!(
            error.message(),
            "Invalid body indentation level (expecting an indentation level of at least 3)"
        );
        assert_eq!(
            error
                .context()
                .and_then(|context| context.tokenizer_parse_line),
            Some(2)
        );
    }
}
