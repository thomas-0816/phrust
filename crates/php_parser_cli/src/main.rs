use php_syntax::{ParseDiagnosticId, ParseSeverity, parse_source_file};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Debug, Default)]
struct Args {
    file: Option<PathBuf>,
    json: bool,
    debug_tree: bool,
    roundtrip_check: bool,
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
        .ok_or_else(|| CliError::usage("a PHP source file is required"))?;
    let source = fs::read_to_string(&file)
        .map_err(|error| CliError::read(format!("failed to read {}: {error}", file.display())))?;
    let parse = parse_source_file(&source);

    if args.json {
        println!("{}", render_json(&file, &parse));
    } else if args.debug_tree {
        print!("{}", parse.debug_tree());
    } else if args.roundtrip_check {
        if parse.reconstructed_text() != source {
            return Err(CliError::roundtrip(format!(
                "{}: roundtrip mismatch",
                file.display()
            )));
        }
        println!("{}: roundtrip ok", file.display());
    } else {
        println!(
            "{}: parsed CST with {} diagnostic(s)",
            file.display(),
            parse.diagnostics().len()
        );
    }

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
            "--json" => parsed.json = true,
            "--debug-tree" => parsed.debug_tree = true,
            "--roundtrip-check" => parsed.roundtrip_check = true,
            "--file" => {
                let value = iter
                    .next()
                    .ok_or_else(|| CliError::usage("--file requires a path"))?;
                parsed.file = Some(PathBuf::from(value));
            }
            _ if arg.starts_with('-') => {
                return Err(CliError::usage(format!("unknown argument: {arg}")));
            }
            _ => {
                if parsed.file.is_some() {
                    return Err(CliError::usage(format!("unexpected argument: {arg}")));
                }
                parsed.file = Some(PathBuf::from(arg));
            }
        }
    }

    Ok(parsed)
}

fn render_json(file: &std::path::Path, parse: &php_syntax::Parse) -> String {
    let source = fs::read_to_string(file).unwrap_or_default();
    let roundtrip_ok = parse.reconstructed_text() == source;
    let mut out = String::new();
    out.push_str("{\"engine\":\"rust-php-parser\",\"file\":\"");
    out.push_str(&escape_json(&file.display().to_string()));
    out.push_str("\",\"ok\":");
    out.push_str(if !parse.has_errors() && roundtrip_ok {
        "true"
    } else {
        "false"
    });
    out.push_str(",\"has_errors\":");
    out.push_str(if parse.has_errors() { "true" } else { "false" });
    out.push_str(",\"roundtrip_ok\":");
    out.push_str(if roundtrip_ok { "true" } else { "false" });
    out.push_str(",\"diagnostics\":[");
    for (index, diagnostic) in parse.diagnostics().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":\"");
        out.push_str(diagnostic_id(diagnostic.id));
        out.push_str("\",\"message\":\"");
        out.push_str(&escape_json(&diagnostic.message));
        out.push_str("\",\"severity\":\"");
        out.push_str(severity_name(diagnostic.severity));
        out.push_str("\",\"expected\":[");
        for (expected_index, expected) in diagnostic.expected.iter().enumerate() {
            if expected_index > 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(&escape_json(expected));
            out.push('"');
        }
        out.push(']');
        out.push_str(",\"start\":");
        out.push_str(&diagnostic.span.start().to_usize().to_string());
        out.push_str(",\"end\":");
        out.push_str(&diagnostic.span.end().to_usize().to_string());
        out.push('}');
    }
    out.push_str("]}");
    out
}

fn diagnostic_id(id: ParseDiagnosticId) -> &'static str {
    id.as_str()
}

fn severity_name(severity: ParseSeverity) -> &'static str {
    match severity {
        ParseSeverity::Error => "error",
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
        "Usage:\n  php-parse path/to/file.php [--json|--debug-tree|--roundtrip-check]\n  php-parse --file path/to/file.php [--json|--debug-tree|--roundtrip-check]\n\nOptions:\n  --json             Print normalized JSON.\n  --debug-tree       Print the CST debug tree.\n  --roundtrip-check  Verify exact source reconstruction.\n  --file PATH        PHP source file to parse.\n  --help             Show this help."
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

    fn roundtrip(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 1,
        }
    }
}
