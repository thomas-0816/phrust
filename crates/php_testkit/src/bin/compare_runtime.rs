//! Runtime fixture differential runner for runtime.

use php_testkit::compatibility::{MismatchCategory, first_differing_line, summarize_output};
use php_testkit::normalize_output::normalize_runtime_stderr;
use php_testkit::runtime_fixture::{
    RuntimeComparisonResult, RuntimeComparisonStatus, RuntimeFixture, RuntimeFixtureExpectation,
    RuntimeFixtureKind, RuntimeOutputSummary, RuntimeSideResult,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Debug)]
struct Options {
    fixtures_root: PathBuf,
    out_dir: PathBuf,
    rust_vm: Option<PathBuf>,
}

#[derive(Serialize)]
struct RuntimeReport {
    fixtures_root: String,
    total: usize,
    pass: usize,
    fail: usize,
    skipped: usize,
    known_gap: usize,
    unexpected_pass: usize,
    categories: BTreeMap<String, usize>,
    feature_areas: BTreeMap<String, usize>,
    diagnostic_ids: BTreeMap<String, usize>,
    owner_areas: BTreeMap<String, usize>,
    results: Vec<RuntimeComparisonResult>,
}

#[derive(Clone, Debug, Deserialize)]
struct KnownGapEntry {
    id: String,
    feature: String,
    status: String,
    layer: String,
    fixtures: Vec<String>,
    owner_area: String,
}

#[derive(Clone, Debug)]
struct KnownGapMatch {
    id: String,
    confidence: &'static str,
    feature: Option<String>,
    owner_area: Option<String>,
}

#[derive(Default)]
struct KnownGapCatalog {
    by_id: BTreeMap<String, KnownGapEntry>,
    by_fixture: BTreeMap<String, String>,
}

fn main() {
    let code = match run() {
        Ok(report) => {
            if report.fail == 0 {
                0
            } else {
                eprintln!("runtime comparison failed for {} fixture(s)", report.fail);
                1
            }
        }
        Err(error) => {
            eprintln!("{error}");
            2
        }
    };
    if code != 0 {
        std::process::exit(code);
    }
}

fn run() -> Result<RuntimeReport, String> {
    let options = parse_args(env::args().skip(1))?;
    fs::create_dir_all(&options.out_dir).map_err(|error| {
        format!(
            "failed to create report directory {}: {error}",
            options.out_dir.display()
        )
    })?;
    let known_gaps = KnownGapCatalog::load(Path::new("docs/known_gaps/runtime.jsonl"))?;
    let fixtures = discover_fixtures(&options.fixtures_root)?;
    let mut results = Vec::new();
    for fixture in fixtures {
        let result = compare_fixture(&fixture, &options, &known_gaps);
        write_result(&options.out_dir, &result)?;
        results.push(result);
    }
    write_results_jsonl(&options.out_dir, &results)?;
    let report = RuntimeReport {
        fixtures_root: options.fixtures_root.display().to_string(),
        total: results.len(),
        pass: results
            .iter()
            .filter(|result| result.status == RuntimeComparisonStatus::Pass)
            .count(),
        fail: results
            .iter()
            .filter(|result| result.status == RuntimeComparisonStatus::Fail)
            .count(),
        skipped: results
            .iter()
            .filter(|result| result.status == RuntimeComparisonStatus::Skipped)
            .count(),
        known_gap: results
            .iter()
            .filter(|result| result.status == RuntimeComparisonStatus::KnownGap)
            .count(),
        unexpected_pass: results
            .iter()
            .filter(|result| result.status == RuntimeComparisonStatus::UnexpectedPass)
            .count(),
        categories: count_by(results.iter().filter_map(|result| {
            result
                .category
                .map(|category| category.as_str().to_string())
        })),
        feature_areas: count_by(
            results
                .iter()
                .filter_map(|result| result.feature_area.clone()),
        ),
        diagnostic_ids: count_by(
            results
                .iter()
                .flat_map(|result| result.diagnostic_ids.clone()),
        ),
        owner_areas: count_by(
            results
                .iter()
                .filter_map(|result| result.owner_area.clone()),
        ),
        results,
    };
    let report_json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    fs::write(options.out_dir.join("runtime-report.json"), report_json)
        .map_err(|error| format!("failed to write runtime report: {error}"))?;
    fs::write(
        options.out_dir.join("runtime-report.md"),
        render_markdown_report(&report),
    )
    .map_err(|error| format!("failed to write runtime markdown report: {error}"))?;
    if options.out_dir == Path::new("target/runtime/runtime-diff") {
        write_docs_reports(&report)?;
    }
    println!(
        "[ok] runtime comparison report: total={} pass={} fail={} skip={} known_gap={} unexpected_pass={} top_categories={} path={}",
        report.total,
        report.pass,
        report.fail,
        report.skipped,
        report.known_gap,
        report.unexpected_pass,
        summarize_counts(&report.categories),
        options.out_dir.join("runtime-report.json").display()
    );
    Ok(report)
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut fixtures_root = PathBuf::from("fixtures/runtime");
    let mut out_dir = PathBuf::from("target/runtime/runtime-diff");
    let mut rust_vm = env::var_os("PHP_VM_CLI").map(PathBuf::from);
    let args = args.into_iter().collect::<Vec<_>>();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--fixtures" => {
                index += 1;
                fixtures_root = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--fixtures requires a path".to_string())?,
                );
            }
            "--out" => {
                index += 1;
                out_dir = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--out requires a path".to_string())?,
                );
            }
            "--rust-vm" => {
                index += 1;
                rust_vm = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--rust-vm requires a path".to_string())?,
                ));
            }
            "--help" | "-h" => {
                return Err(
                    "Usage: compare-runtime [--fixtures fixtures/runtime] [--out target/runtime/runtime-diff] [--rust-vm target/debug/php-vm]"
                        .to_string(),
                );
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
        index += 1;
    }
    Ok(Options {
        fixtures_root,
        out_dir,
        rust_vm,
    })
}

fn discover_fixtures(root: &Path) -> Result<Vec<RuntimeFixture>, String> {
    let mut paths = Vec::new();
    collect_php_files(root, &mut paths)?;
    paths.sort();
    Ok(paths
        .into_iter()
        .map(RuntimeFixture::new)
        .map(|mut fixture| {
            if fixture
                .path
                .to_string_lossy()
                .contains("/valid/includes/lib/")
            {
                fixture.expect = RuntimeFixtureExpectation::Skip;
            }
            fixture
        })
        .collect())
}

fn collect_php_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_php_files(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("php") {
            out.push(path);
        }
    }
    Ok(())
}

fn compare_fixture(
    fixture: &RuntimeFixture,
    options: &Options,
    known_gaps: &KnownGapCatalog,
) -> RuntimeComparisonResult {
    if fixture.expect == RuntimeFixtureExpectation::Skip {
        return result(
            fixture,
            RuntimeComparisonParts {
                reference: None,
                rust: None,
                status: RuntimeComparisonStatus::Skipped,
                category: None,
                diagnostic_ids: Vec::new(),
                known_gap_id: None,
                known_gap_match: known_gaps.find_for_fixture_metadata(fixture),
                message: Some("fixture metadata requested skip".to_string()),
            },
        );
    }

    let rust = run_rust_vm(fixture, options);
    let rust_side = rust
        .as_ref()
        .ok()
        .map(|output| side_result(output, &fixture.path));
    let diagnostic_ids = rust
        .as_ref()
        .ok()
        .map(|output| extract_diagnostic_ids(&String::from_utf8_lossy(&output.stderr)))
        .unwrap_or_default();

    if fixture.expect == RuntimeFixtureExpectation::KnownGap
        || fixture.kind == RuntimeFixtureKind::KnownGap
    {
        if rust.is_err() {
            return result(
                fixture,
                RuntimeComparisonParts {
                    reference: run_reference_side(fixture).ok().flatten(),
                    rust: rust_side,
                    status: RuntimeComparisonStatus::Fail,
                    category: Some(MismatchCategory::HarnessError),
                    diagnostic_ids,
                    known_gap_id: None,
                    known_gap_match: known_gaps.find_for_fixture_metadata(fixture),
                    message: rust.err(),
                },
            );
        }
        let reference = run_reference_side(fixture).ok().flatten();
        let gap_match = known_gaps.find(fixture, &diagnostic_ids);
        let status = match (&reference, &rust_side) {
            (Some(reference), Some(rust)) if same_side(reference, rust) => {
                RuntimeComparisonStatus::UnexpectedPass
            }
            _ => RuntimeComparisonStatus::KnownGap,
        };
        let category = match status {
            RuntimeComparisonStatus::UnexpectedPass => Some(MismatchCategory::UnexpectedPass),
            _ => fixture
                .category
                .or(Some(MismatchCategory::ExpectedKnownGap)),
        };
        let message = if status == RuntimeComparisonStatus::UnexpectedPass {
            Some(
                "known-gap fixture now matches the PHP reference; retire or reclassify the gap"
                    .to_string(),
            )
        } else {
            rust.err()
        };
        return result(
            fixture,
            RuntimeComparisonParts {
                reference,
                rust: rust_side,
                status,
                category,
                diagnostic_ids,
                known_gap_id: None,
                known_gap_match: gap_match,
                message,
            },
        );
    }

    if fixture.expect == RuntimeFixtureExpectation::Fail
        || fixture.kind == RuntimeFixtureKind::Invalid
    {
        let status = match rust.as_ref() {
            Ok(output) if !output.status.success() => RuntimeComparisonStatus::Pass,
            Ok(_) => RuntimeComparisonStatus::Fail,
            Err(_) => RuntimeComparisonStatus::Fail,
        };
        let message = if status == RuntimeComparisonStatus::Fail {
            Some("fixture was expected to fail on the Rust runtime".to_string())
        } else {
            None
        };
        return result(
            fixture,
            RuntimeComparisonParts {
                reference: run_reference_side(fixture).ok().flatten(),
                rust: rust_side,
                status,
                category: (status == RuntimeComparisonStatus::Fail)
                    .then_some(MismatchCategory::RuntimeExitMismatch),
                diagnostic_ids,
                known_gap_id: fixture.known_gap_id.clone(),
                known_gap_match: known_gaps.find_for_fixture_metadata(fixture),
                message: message.or_else(|| rust.err()),
            },
        );
    }

    let reference = match run_reference_side(fixture) {
        Ok(reference) => reference,
        Err(message) => {
            return result(
                fixture,
                RuntimeComparisonParts {
                    reference: None,
                    rust: rust_side,
                    status: RuntimeComparisonStatus::Fail,
                    category: Some(MismatchCategory::HarnessError),
                    diagnostic_ids,
                    known_gap_id: None,
                    known_gap_match: None,
                    message: Some(message),
                },
            );
        }
    };
    let Some(reference) = reference else {
        let status = if fixture.php_ref_required {
            RuntimeComparisonStatus::Fail
        } else {
            RuntimeComparisonStatus::Skipped
        };
        return result(
            fixture,
            RuntimeComparisonParts {
                reference: None,
                rust: rust_side,
                status,
                category: (status == RuntimeComparisonStatus::Fail)
                    .then_some(MismatchCategory::HarnessError),
                diagnostic_ids,
                known_gap_id: None,
                known_gap_match: None,
                message: Some("REFERENCE_PHP is not set".to_string()),
            },
        );
    };
    let gap_match = known_gaps.find(fixture, &diagnostic_ids);
    let sides_match =
        matches!((&reference, &rust_side), (reference, Some(rust)) if same_side(reference, rust));
    let status = if sides_match {
        if gap_match.is_some() {
            RuntimeComparisonStatus::UnexpectedPass
        } else {
            RuntimeComparisonStatus::Pass
        }
    } else if gap_match.is_some() {
        RuntimeComparisonStatus::KnownGap
    } else {
        RuntimeComparisonStatus::Fail
    };
    let category = match status {
        RuntimeComparisonStatus::Fail => Some(classify_runtime_mismatch(
            &reference,
            rust_side.as_ref(),
            &diagnostic_ids,
            rust.as_ref().err().map(String::as_str),
        )),
        RuntimeComparisonStatus::KnownGap => fixture
            .category
            .or(Some(MismatchCategory::ExpectedKnownGap)),
        RuntimeComparisonStatus::UnexpectedPass => Some(MismatchCategory::UnexpectedPass),
        RuntimeComparisonStatus::Pass | RuntimeComparisonStatus::Skipped => None,
    };
    let message = match status {
        RuntimeComparisonStatus::Fail | RuntimeComparisonStatus::KnownGap => {
            Some(diff_message(&reference, rust_side.as_ref()))
        }
        RuntimeComparisonStatus::UnexpectedPass => Some(
            "known-gap fixture now matches the PHP reference; retire or reclassify the gap"
                .to_string(),
        ),
        RuntimeComparisonStatus::Pass | RuntimeComparisonStatus::Skipped => None,
    };
    result(
        fixture,
        RuntimeComparisonParts {
            reference: Some(reference),
            rust: rust_side,
            status,
            category,
            diagnostic_ids,
            known_gap_id: None,
            known_gap_match: gap_match,
            message: message.or_else(|| rust.err()),
        },
    )
}

fn run_reference_side(fixture: &RuntimeFixture) -> Result<Option<RuntimeSideResult>, String> {
    let Some(php_bin) = env::var_os("REFERENCE_PHP").map(PathBuf::from) else {
        return Ok(None);
    };
    if !php_bin.is_file() {
        return Err(format!(
            "REFERENCE_PHP is not a file: {}",
            php_bin.display()
        ));
    }
    let output = Command::new(&php_bin)
        .arg(&fixture.path)
        .args(&fixture.args)
        .env_clear()
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("NO_COLOR", "1")
        .env("PHP_INI_SCAN_DIR", "")
        .output()
        .map_err(|error| format!("failed to execute {}: {error}", php_bin.display()))?;
    Ok(Some(side_result_with_php(
        &output,
        &fixture.path,
        Some(&php_bin),
    )))
}

fn run_rust_vm(fixture: &RuntimeFixture, options: &Options) -> Result<Output, String> {
    if let Some(path) = &options.rust_vm
        && path.is_file()
    {
        let mut command = Command::new(path);
        command.arg("run").arg(&fixture.path);
        if !fixture.args.is_empty() {
            command.arg("--").args(&fixture.args);
        }
        return command
            .output()
            .map_err(|error| format!("failed to execute {}: {error}", path.display()));
    }
    let default_vm = Path::new("target/debug/php-vm");
    if default_vm.is_file() {
        let mut command = Command::new(default_vm);
        command.arg("run").arg(&fixture.path);
        if !fixture.args.is_empty() {
            command.arg("--").args(&fixture.args);
        }
        return command
            .output()
            .map_err(|error| format!("failed to execute {}: {error}", default_vm.display()));
    }
    let mut command = Command::new("cargo");
    command.args(["run", "-p", "php_vm_cli", "--", "run"]);
    command.arg(&fixture.path);
    if !fixture.args.is_empty() {
        command.arg("--").args(&fixture.args);
    }
    command
        .output()
        .map_err(|error| format!("failed to execute cargo run -p php_vm_cli: {error}"))
}

fn side_result(output: &Output, file: &Path) -> RuntimeSideResult {
    side_result_with_php(output, file, None)
}

fn side_result_with_php(output: &Output, file: &Path, php_bin: Option<&Path>) -> RuntimeSideResult {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    RuntimeSideResult {
        exit_code: output.status.code(),
        stdout,
        stderr_normalized: normalize_runtime_stderr(&stderr, file, php_bin),
    }
}

fn same_side(reference: &RuntimeSideResult, rust: &RuntimeSideResult) -> bool {
    reference.exit_code == rust.exit_code
        && reference.stdout == rust.stdout
        && reference.stderr_normalized == rust.stderr_normalized
}

fn diff_message(reference: &RuntimeSideResult, rust: Option<&RuntimeSideResult>) -> String {
    let Some(rust) = rust else {
        return "Rust runtime did not produce output".to_string();
    };
    let mut parts = Vec::new();
    if reference.exit_code != rust.exit_code {
        parts.push(format!(
            "exit_code reference={:?} rust={:?}",
            reference.exit_code, rust.exit_code
        ));
    }
    if reference.stdout != rust.stdout {
        parts.push(format!(
            "stdout reference={:?} rust={:?}",
            reference.stdout, rust.stdout
        ));
    }
    if reference.stderr_normalized != rust.stderr_normalized {
        parts.push(format!(
            "stderr reference={:?} rust={:?}",
            reference.stderr_normalized, rust.stderr_normalized
        ));
    }
    parts.join("; ")
}

struct RuntimeComparisonParts {
    reference: Option<RuntimeSideResult>,
    rust: Option<RuntimeSideResult>,
    status: RuntimeComparisonStatus,
    category: Option<MismatchCategory>,
    diagnostic_ids: Vec<String>,
    known_gap_id: Option<String>,
    known_gap_match: Option<KnownGapMatch>,
    message: Option<String>,
}

fn result(fixture: &RuntimeFixture, parts: RuntimeComparisonParts) -> RuntimeComparisonResult {
    let RuntimeComparisonParts {
        reference,
        rust,
        status,
        category,
        diagnostic_ids,
        known_gap_id,
        known_gap_match,
        message,
    } = parts;
    let known_gap_id =
        known_gap_id.or_else(|| known_gap_match.as_ref().map(|matched| matched.id.clone()));
    let feature_area = known_gap_match
        .as_ref()
        .and_then(|matched| matched.feature.clone())
        .or_else(|| infer_feature_area(fixture));
    let owner_area = known_gap_match
        .as_ref()
        .and_then(|matched| matched.owner_area.clone());
    let reference_summary = reference.as_ref().map(output_summary);
    let phrust_summary = rust.as_ref().map(output_summary);
    let first_differing_line = compare_first_differing_line(reference.as_ref(), rust.as_ref());
    RuntimeComparisonResult {
        file: fixture.display_path(),
        mode: "compare-runtime".to_string(),
        reference,
        rust,
        status,
        category,
        diagnostic_ids,
        known_gap_id,
        known_gap_match_confidence: known_gap_match.map(|matched| matched.confidence.to_string()),
        feature_area,
        owner_area,
        reference_summary,
        phrust_summary,
        first_differing_line,
        message,
    }
}

fn write_result(out_dir: &Path, result: &RuntimeComparisonResult) -> Result<(), String> {
    let filename = result
        .file
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let json = result.to_pretty_json().map_err(|error| error.to_string())?;
    fs::write(out_dir.join(format!("{filename}.json")), json)
        .map_err(|error| format!("failed to write fixture result: {error}"))
}

fn write_results_jsonl(out_dir: &Path, results: &[RuntimeComparisonResult]) -> Result<(), String> {
    let mut lines = String::new();
    for result in results {
        let line = serde_json::to_string(result).map_err(|error| error.to_string())?;
        lines.push_str(&line);
        lines.push('\n');
    }
    fs::write(out_dir.join("runtime-results.jsonl"), lines)
        .map_err(|error| format!("failed to write runtime JSONL report: {error}"))
}

fn write_docs_reports(report: &RuntimeReport) -> Result<(), String> {
    let docs_dir = Path::new("target/runtime/reports");
    fs::create_dir_all(docs_dir)
        .map_err(|error| format!("failed to create runtime report directory: {error}"))?;
    fs::write(
        docs_dir.join("runtime-diff-report.md"),
        render_markdown_report(report),
    )
    .map_err(|error| format!("failed to write runtime markdown report: {error}"))?;

    let mut lines = String::new();
    for result in &report.results {
        let line = serde_json::to_string(result).map_err(|error| error.to_string())?;
        lines.push_str(&line);
        lines.push('\n');
    }
    fs::write(docs_dir.join("runtime-diff-results.jsonl"), lines)
        .map_err(|error| format!("failed to write runtime JSONL report: {error}"))
}

fn output_summary(side: &RuntimeSideResult) -> RuntimeOutputSummary {
    RuntimeOutputSummary {
        exit_code: side.exit_code,
        stdout: summarize_output(&side.stdout),
        stderr: summarize_output(&side.stderr_normalized),
    }
}

fn compare_first_differing_line(
    reference: Option<&RuntimeSideResult>,
    rust: Option<&RuntimeSideResult>,
) -> Option<usize> {
    let reference = reference?;
    let rust = rust?;
    if reference.stdout != rust.stdout {
        first_differing_line(&reference.stdout, &rust.stdout)
    } else if reference.stderr_normalized != rust.stderr_normalized {
        first_differing_line(&reference.stderr_normalized, &rust.stderr_normalized)
    } else {
        None
    }
}

fn classify_runtime_mismatch(
    reference: &RuntimeSideResult,
    rust: Option<&RuntimeSideResult>,
    diagnostic_ids: &[String],
    harness_error: Option<&str>,
) -> MismatchCategory {
    if let Some(error) = harness_error {
        let lower = error.to_ascii_lowercase();
        if lower.contains("timeout") {
            return MismatchCategory::TimeoutOrNontermination;
        }
        return MismatchCategory::HarnessError;
    }
    let Some(rust) = rust else {
        return MismatchCategory::HarnessError;
    };
    if diagnostic_ids.iter().any(|id| id.contains("UNSUPPORTED")) {
        return MismatchCategory::UnsupportedFeature;
    }
    let stderr_lower = rust.stderr_normalized.to_ascii_lowercase();
    if stderr_lower.contains("timeout") || stderr_lower.contains("step limit") {
        return MismatchCategory::TimeoutOrNontermination;
    }
    if stderr_lower.contains("parse") || stderr_lower.contains("syntax") {
        return MismatchCategory::PhrustParseMismatch;
    }
    if stderr_lower.contains("compile") || stderr_lower.contains("lower") {
        return MismatchCategory::CompileMismatch;
    }
    if reference.exit_code != rust.exit_code {
        return MismatchCategory::RuntimeExitMismatch;
    }
    if reference.stdout != rust.stdout {
        return MismatchCategory::StdoutMismatch;
    }
    if !diagnostic_ids.is_empty() {
        return MismatchCategory::DiagnosticMismatch;
    }
    if reference.stderr_normalized != rust.stderr_normalized {
        return MismatchCategory::StderrMismatch;
    }
    MismatchCategory::HarnessError
}

fn infer_feature_area(fixture: &RuntimeFixture) -> Option<String> {
    let mut parts = fixture.path.components().filter_map(|component| {
        let text = component.as_os_str().to_str()?;
        (!matches!(
            text,
            "fixtures" | "runtime" | "valid" | "invalid" | "known_gaps"
        ))
        .then_some(text.to_string())
    });
    parts.next()
}

fn count_by(values: impl IntoIterator<Item = String>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_insert(0) += 1;
    }
    counts
}

fn summarize_counts(counts: &BTreeMap<String, usize>) -> String {
    let mut entries = counts
        .iter()
        .map(|(key, count)| format!("{key}:{count}"))
        .collect::<Vec<_>>();
    entries.truncate(5);
    if entries.is_empty() {
        "none".to_string()
    } else {
        entries.join(",")
    }
}

fn render_markdown_report(report: &RuntimeReport) -> String {
    let mut out = String::new();
    out.push_str("# Runtime Compatibility Report\n\n");
    out.push_str(&format!(
        "- Fixtures: {}\n- Pass: {}\n- Unexpected failures: {}\n- Skipped: {}\n- Expected known gaps: {}\n- Unexpected passes: {}\n\n",
        report.total,
        report.pass,
        report.fail,
        report.skipped,
        report.known_gap,
        report.unexpected_pass
    ));
    render_count_section(&mut out, "Categories", &report.categories);
    render_count_section(&mut out, "Feature Areas", &report.feature_areas);
    render_count_section(&mut out, "Diagnostic IDs", &report.diagnostic_ids);
    render_count_section(&mut out, "Owner Streams", &report.owner_areas);
    out.push_str("## Non-Pass Fixtures\n\n");
    out.push_str("| Fixture | Status | Category | Known gap | Feature area | Owner | First differing line | Message |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for result in &report.results {
        if result.status == RuntimeComparisonStatus::Pass {
            continue;
        }
        out.push_str(&format!(
            "| `{}` | `{:?}` | {} | {} | {} | {} | {} | {} |\n",
            markdown_escape(&result.file),
            result.status,
            result
                .category
                .map(|category| format!("`{}`", category.as_str()))
                .unwrap_or_else(|| "-".to_string()),
            result
                .known_gap_id
                .as_deref()
                .map(|id| format!("`{}`", markdown_escape(id)))
                .unwrap_or_else(|| "-".to_string()),
            result
                .feature_area
                .as_deref()
                .map(markdown_escape)
                .unwrap_or_else(|| "-".to_string()),
            result
                .owner_area
                .as_deref()
                .map(markdown_escape)
                .unwrap_or_else(|| "-".to_string()),
            result
                .first_differing_line
                .map(|line| line.to_string())
                .unwrap_or_else(|| "-".to_string()),
            result
                .message
                .as_deref()
                .map(markdown_escape)
                .unwrap_or_else(|| "-".to_string())
        ));
    }
    out
}

fn render_count_section(out: &mut String, title: &str, counts: &BTreeMap<String, usize>) {
    out.push_str(&format!("## {title}\n\n"));
    if counts.is_empty() {
        out.push_str("- none\n\n");
        return;
    }
    for (key, count) in counts {
        out.push_str(&format!("- `{}`: {}\n", markdown_escape(key), count));
    }
    out.push('\n');
}

fn markdown_escape(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

impl KnownGapCatalog {
    fn load(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let mut catalog = Self::default();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let entry: KnownGapEntry = serde_json::from_str(line)
                .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?;
            for fixture in &entry.fixtures {
                catalog.by_fixture.insert(fixture.clone(), entry.id.clone());
            }
            catalog.by_id.insert(entry.id.clone(), entry);
        }
        Ok(catalog)
    }

    fn find_for_fixture_metadata(&self, fixture: &RuntimeFixture) -> Option<KnownGapMatch> {
        fixture
            .known_gap_id
            .as_ref()
            .and_then(|id| self.match_by_id(id, "fixture-metadata"))
    }

    fn find(&self, fixture: &RuntimeFixture, diagnostic_ids: &[String]) -> Option<KnownGapMatch> {
        if let Some(matched) = self.find_for_fixture_metadata(fixture) {
            return Some(matched);
        }
        for diagnostic_id in diagnostic_ids {
            if let Some(matched) = self.match_by_id(diagnostic_id, "diagnostic-id") {
                return Some(matched);
            }
        }
        self.by_fixture
            .get(&fixture.display_path())
            .and_then(|id| self.match_by_id(id, "fixture-path"))
    }

    fn match_by_id(&self, id: &str, confidence: &'static str) -> Option<KnownGapMatch> {
        let entry = self.by_id.get(id)?;
        if entry.status == "implemented" && confidence != "fixture-metadata" {
            return None;
        }
        Some(KnownGapMatch {
            id: entry.id.clone(),
            confidence,
            feature: Some(entry.feature.clone()),
            owner_area: Some(if entry.owner_area.is_empty() {
                entry.layer.clone()
            } else {
                entry.owner_area.clone()
            }),
        })
    }
}

fn extract_diagnostic_ids(stderr: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = stderr;
    while let Some(index) = rest.find("\"id\":\"") {
        let after = &rest[index + "\"id\":\"".len()..];
        let Some(end) = after.find('"') else {
            break;
        };
        let id = after[..end].to_string();
        if !ids.contains(&id) {
            ids.push(id);
        }
        rest = &after[end + 1..];
    }
    for token in stderr.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')) {
        if token.starts_with("E_") && !ids.iter().any(|id| id == token) {
            ids.push(token.to_string());
        }
    }
    ids
}
