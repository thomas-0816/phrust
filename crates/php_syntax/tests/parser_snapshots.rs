use php_syntax::parse_source_file;
use std::fs;
use std::path::{Path, PathBuf};

const CASES: &[(&str, &str)] = &[
    ("basic_echo", "valid/basic_echo.php"),
    ("pure_html", "valid/pure_html.php"),
    ("php_html_modes", "valid/php_html_modes.php"),
    ("statements_basic", "valid/statements_basic.php"),
    ("control_flow", "valid/control_flow.php"),
    ("alternative_syntax", "valid/alternative_syntax.php"),
    ("operator_groups", "valid/operator_groups.php"),
    ("expressions_postfix", "valid/expressions_postfix.php"),
    ("functions", "valid/functions.php"),
    ("closures", "valid/closures.php"),
    ("class_members", "valid/class_members.php"),
    ("attributes", "valid/attributes.php"),
    ("dnf_types", "valid/dnf_types.php"),
    ("encapsed_strings", "valid/encapsed_strings.php"),
    ("php85_syntax_matrix", "php85/syntax_matrix.php"),
];

#[test]
fn parser_debug_tree_snapshots_are_stable() {
    for (name, fixture) in CASES {
        let source = fs::read_to_string(fixture_path(fixture)).expect("fixture source");
        let parse = parse_source_file(&source);
        let content = render_parser_snapshot(fixture, &source, &parse);

        assert_snapshot("parser", name, &content);
    }
}

fn render_parser_snapshot(fixture: &str, source: &str, parse: &php_syntax::Parse) -> String {
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
    out.push_str("diagnostics: ");
    out.push_str(&parse.diagnostics().len().to_string());
    out.push_str("\n\n");
    out.push_str(&parse.debug_tree());
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
