use php_syntax::parse_source_file;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn parser_fixtures_roundtrip_exactly() {
    let fixtures = parser_fixtures();
    assert!(!fixtures.is_empty(), "parser fixtures should be present");

    for fixture in fixtures {
        let source = fs::read_to_string(&fixture)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", fixture.display()));
        let parse = parse_source_file(&source);
        assert_eq!(
            parse.reconstructed_text(),
            source,
            "roundtrip mismatch for {}",
            fixture.display()
        );
    }
}

fn parser_fixtures() -> Vec<PathBuf> {
    let root = repo_root().join("fixtures/parser");
    let mut out = Vec::new();
    collect_php_files(&root, &mut out);
    out.sort();
    out
}

fn collect_php_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read fixture dir {}: {error}", dir.display()));
    for entry in entries {
        let entry = entry.expect("fixture dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_php_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "php") {
            out.push(path);
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}
