use php_syntax::parse_source_file;
use std::fs;
use std::path::{Path, PathBuf};

const CASES: &[(&str, &str)] = &[
    ("missing_semicolon", "invalid/missing_semicolon.php"),
    ("arrays_invalid", "invalid/arrays_invalid.php"),
    ("attributes_invalid", "invalid/attributes_invalid.php"),
    (
        "expressions_basic_invalid",
        "invalid/expressions_basic_invalid.php",
    ),
    ("functions_invalid", "invalid/functions_invalid.php"),
    ("types_invalid", "invalid/types_invalid.php"),
    ("recovery_bad_attribute", "recovery/bad_attribute.php"),
    (
        "recovery_missing_expression",
        "recovery/missing_expression.php",
    ),
    (
        "recovery_unclosed_delimiter",
        "recovery/unclosed_delimiter.php",
    ),
];

#[test]
fn diagnostic_snapshots_are_stable() {
    for (name, fixture) in CASES {
        let source = fs::read_to_string(fixture_path(fixture)).expect("fixture source");
        let parse = parse_source_file(&source);
        let content = render_diagnostic_snapshot(fixture, &source, &parse);

        assert_snapshot("diagnostics", name, &content);
    }
}

fn render_diagnostic_snapshot(fixture: &str, source: &str, parse: &php_syntax::Parse) -> String {
    let mut out = String::new();
    out.push_str("fixture: ");
    out.push_str(fixture);
    out.push('\n');
    out.push_str("roundtrip_ok: ");
    out.push_str(if parse.reconstructed_text() == source {
        "true"
    } else {
        "false"
    });
    out.push('\n');
    out.push_str("diagnostics:\n");
    for diagnostic in parse.diagnostics() {
        out.push_str("- id: ");
        out.push_str(diagnostic.id.as_str());
        out.push('\n');
        out.push_str("  span: ");
        out.push_str(&diagnostic.span.start().to_usize().to_string());
        out.push_str("..");
        out.push_str(&diagnostic.span.end().to_usize().to_string());
        out.push('\n');
        out.push_str("  expected: ");
        out.push_str(&format!("{:?}", diagnostic.expected));
        out.push('\n');
        out.push_str("  message: ");
        out.push_str(&format!("{:?}", diagnostic.message));
        out.push('\n');
    }
    out
}

fn assert_snapshot(kind: &str, name: &str, actual: &str) {
    let path = snapshot_path(kind, name);
    if std::env::var_os("UPDATE_PARSER_SNAPSHOTS").is_some() {
        fs::create_dir_all(path.parent().expect("snapshot parent")).expect("snapshot dir");
        fs::write(&path, actual).expect("write snapshot");
    }

    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    assert_eq!(expected, actual, "snapshot changed: {}", path.display());
}

fn fixture_path(fixture: &str) -> PathBuf {
    repo_root().join("fixtures/parser").join(fixture)
}

fn snapshot_path(kind: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(kind)
        .join(format!("{name}.snap"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}
