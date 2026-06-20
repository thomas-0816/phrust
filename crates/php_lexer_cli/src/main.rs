use php_lexer::{LexDiagnosticKind, LexerConfig, TokenKind, lex_all};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Default)]
struct Args {
    file: Option<PathBuf>,
    short_open_tag: bool,
    pretty: bool,
    help: bool,
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(error) => {
            let _ = writeln!(io::stderr(), "{}", error.message);
            std::process::exit(error.code);
        }
    }
}

fn run() -> Result<(), CliError> {
    let args = parse_args(env::args().skip(1))?;
    if args.help {
        print_usage();
        return Ok(());
    }

    let file = args
        .file
        .ok_or_else(|| CliError::usage("--file is required"))?;
    let source = fs::read_to_string(&file)
        .map_err(|error| CliError::read(format!("failed to read {}: {error}", file.display())))?;

    let result = lex_all(
        &source,
        LexerConfig {
            short_open_tag: args.short_open_tag,
            token_parse: false,
            emit_eof: false,
        },
    );

    let json = render_json(&source, &result, args.pretty);
    println!("{json}");
    Ok(())
}

fn parse_args<I>(args: I) -> Result<Args, CliError>
where
    I: IntoIterator<Item = String>,
{
    let mut parsed = Args::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => parsed.help = true,
            "--short-open-tag" => parsed.short_open_tag = true,
            "--pretty" => parsed.pretty = true,
            "--file" => {
                let value = iter
                    .next()
                    .ok_or_else(|| CliError::usage("--file requires a path"))?;
                parsed.file = Some(PathBuf::from(value));
            }
            _ => return Err(CliError::usage(format!("unknown argument: {arg}"))),
        }
    }

    Ok(parsed)
}

fn render_json(source: &str, result: &php_lexer::LexResult, pretty: bool) -> String {
    if pretty {
        render_pretty_json(source, result)
    } else {
        render_compact_json(source, result)
    }
}

fn render_compact_json(source: &str, result: &php_lexer::LexResult) -> String {
    let mut out = String::from("{\"engine\":\"rust-php-lexer\",\"tokens\":[");
    for (index, token) in result.tokens.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let text = token.text(source).unwrap_or_default();
        out.push_str(&format!(
            "{{\"index\":{},\"kind\":\"{}\",\"text\":\"{}\",\"line\":{},\"start\":{},\"end\":{}}}",
            index,
            escape_json(&token_kind_name(token.kind)),
            escape_json(text),
            token.line,
            token.range.start().to_usize(),
            token.range.end().to_usize()
        ));
    }
    out.push_str("],\"diagnostics\":[");
    for (index, diagnostic) in result.diagnostics.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"index\":{},\"id\":\"{}\",\"message\":\"{}\",\"line\":{},\"start\":{},\"end\":{}}}",
            index,
            diagnostic_id(diagnostic.kind),
            escape_json(&diagnostic.message),
            diagnostic.line,
            diagnostic.span.start().to_usize(),
            diagnostic.span.end().to_usize()
        ));
    }
    out.push_str("]}");
    out
}

fn render_pretty_json(source: &str, result: &php_lexer::LexResult) -> String {
    let mut out = String::from("{\n  \"engine\": \"rust-php-lexer\",\n  \"tokens\": [");
    for (index, token) in result.tokens.iter().enumerate() {
        if index == 0 {
            out.push('\n');
        } else {
            out.push_str(",\n");
        }
        let text = token.text(source).unwrap_or_default();
        out.push_str(&format!(
            "    {{\"index\": {}, \"kind\": \"{}\", \"text\": \"{}\", \"line\": {}, \"start\": {}, \"end\": {}}}",
            index,
            escape_json(&token_kind_name(token.kind)),
            escape_json(text),
            token.line,
            token.range.start().to_usize(),
            token.range.end().to_usize()
        ));
    }
    if !result.tokens.is_empty() {
        out.push('\n');
    }
    out.push_str("  ],\n  \"diagnostics\": [");
    for (index, diagnostic) in result.diagnostics.iter().enumerate() {
        if index == 0 {
            out.push('\n');
        } else {
            out.push_str(",\n");
        }
        out.push_str(&format!(
            "    {{\"index\": {}, \"id\": \"{}\", \"message\": \"{}\", \"line\": {}, \"start\": {}, \"end\": {}}}",
            index,
            diagnostic_id(diagnostic.kind),
            escape_json(&diagnostic.message),
            diagnostic.line,
            diagnostic.span.start().to_usize(),
            diagnostic.span.end().to_usize()
        ));
    }
    if !result.diagnostics.is_empty() {
        out.push('\n');
    }
    out.push_str("  ]\n}");
    out
}

fn token_kind_name(kind: TokenKind) -> String {
    kind.reference_name()
}

fn diagnostic_id(kind: LexDiagnosticKind) -> &'static str {
    match kind {
        LexDiagnosticKind::InvalidInput => "invalid-input",
        LexDiagnosticKind::UnterminatedBlockComment => "unterminated-block-comment",
        LexDiagnosticKind::UnterminatedString => "unterminated-string",
        LexDiagnosticKind::UnterminatedHeredoc => "unterminated-heredoc",
        LexDiagnosticKind::BadCharacter => "bad-character",
    }
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0C}' => escaped.push_str("\\f"),
            ch if ch <= '\u{1F}' => escaped.push_str(&format!("\\u{:04X}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn print_usage() {
    println!(
        "Usage:\n  php-lex --file path/to/file.php [--short-open-tag] [--pretty]\n\nOptions:\n  --file PATH         PHP source file to tokenize.\n  --short-open-tag   Treat <? as an opening tag.\n  --pretty           Pretty-print JSON.\n  --help             Show this help."
    );
}

struct CliError {
    message: String,
    code: i32,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 2,
        }
    }

    fn read(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 1,
        }
    }
}
