use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use php_testkit::compatibility::{MismatchCategory, classify_phpt_detail};

use php_phpt_tools::expect::{ExpectationKind, match_expectation};
use php_phpt_tools::phpt::{PhptDocument, PhptSection, parse_phpt};

pub(crate) mod baseline;
mod fingerprint;
pub(crate) mod generate;
pub(crate) mod index;
mod json;
pub(crate) mod lookup;
pub(crate) mod run;
pub(crate) mod source_index;
pub(crate) mod symbol_index;
pub(crate) mod triage;
pub(crate) mod verify;

use fingerprint::{normalize_repository_paths, normalize_work_directory_paths};
use json::*;

const DEFAULT_MANIFEST: &str = "tests/phpt/manifests/php-src-hashes.jsonl";
const DEFAULT_SYMBOLS: &str = "tests/phpt/manifests/php-src-symbols.jsonl";
const DEFAULT_PHPT_CORPUS: &str = "tests/phpt/manifests/phpt-corpus.jsonl";
const DEFAULT_PHPT_REPORT: &str = "target/phpt-work/reports/phpt-corpus-summary.md";
const DEFAULT_PHPT_BASELINE_METADATA: &str = "tests/phpt/manifests/full-baseline-metadata.json";
const DEFAULT_PHPT_BASELINE_MODULE_COUNTS: &str =
    "tests/phpt/manifests/full-baseline-module-counts.jsonl";
const DEFAULT_PHPT_TRIAGE_REPORT: &str = "target/phpt-work/reports/triage.md";
const DEFAULT_PHPT_EXTENSION_POLICY_REPORT: &str = "target/phpt-work/reports/extension-policy.md";
const DEFAULT_PHPT_KNOWN_GAP_REPORT: &str = "target/phpt-work/reports/known-gaps.md";
const DEFAULT_PHPT_KNOWN_GAP_CATALOG: &str = "tests/phpt/manifests/known-gap-catalog.jsonl";
const DEFAULT_PHPT_MODULE_PRIORITY: &str = "tests/phpt/manifests/module-priority.json";
const DEFAULT_PHPT_MODULE_DOCS_DIR: &str = "target/phpt-work/modules";
const DEFAULT_PHPT_MODULE_MANIFESTS_DIR: &str = "tests/phpt/manifests/modules";
const GENERATOR_VERSION: &str = "phpt-generate-v1";
const PHP_RUN_TESTS_INI_DEFAULTS: &[(&str, &str)] = &[
    ("output_handler", ""),
    ("open_basedir", ""),
    ("disable_functions", ""),
    ("output_buffering", "Off"),
    ("error_reporting", "32767"),
    ("fatal_error_backtraces", "Off"),
    ("display_errors", "1"),
    ("display_startup_errors", "1"),
    ("log_errors", "0"),
    ("html_errors", "0"),
    ("report_zend_debug", "0"),
    ("docref_root", ""),
    ("docref_ext", ".html"),
    ("error_prepend_string", ""),
    ("error_append_string", ""),
    ("auto_prepend_file", ""),
    ("auto_append_file", ""),
    ("ignore_repeated_errors", "0"),
    ("precision", "14"),
    ("serialize_precision", "-1"),
    ("memory_limit", "128M"),
    ("expose_php", "1"),
    ("opcache.fast_shutdown", "0"),
    ("opcache.file_update_protection", "0"),
    ("opcache.revalidate_freq", "0"),
    ("opcache.jit_hot_loop", "1"),
    ("opcache.jit_hot_func", "1"),
    ("opcache.jit_hot_return", "1"),
    ("opcache.jit_hot_side_exit", "1"),
    ("opcache.jit_max_root_traces", "100000"),
    ("opcache.jit_max_side_traces", "100000"),
    ("opcache.jit_max_exit_counters", "100000"),
    ("opcache.protect_memory", "1"),
    ("zend.assertions", "1"),
    ("zend.exception_ignore_args", "0"),
    ("zend.exception_string_param_max_len", "15"),
    ("short_open_tag", "0"),
    ("date.timezone", "UTC"),
];

#[derive(Debug)]
struct SourceOptions {
    php_src: PathBuf,
    manifest: PathBuf,
}

#[derive(Debug)]
struct SymbolOptions {
    php_src: PathBuf,
    symbols: PathBuf,
}

impl SymbolOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut php_src = None;
        let mut symbols = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            if let Some(value) = arg.strip_prefix("--php-src=") {
                php_src = Some(PathBuf::from(value));
            } else if arg == "--php-src" {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--php-src requires a path".to_string());
                };
                php_src = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix("--symbols=") {
                symbols = Some(PathBuf::from(value));
            } else if arg == "--symbols" {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--symbols requires a path".to_string());
                };
                symbols = Some(PathBuf::from(value));
            } else {
                return Err(format!("unknown option `{arg}`"));
            }
            index += 1;
        }
        let php_src = php_src
            .or_else(|| env::var_os("PHP_SRC_DIR").map(PathBuf::from))
            .unwrap_or_else(default_php_src_dir);
        if !php_src.is_dir() {
            return Err(format!(
                "php-src checkout not found at {}; set PHP_SRC_DIR or --php-src",
                php_src.display()
            ));
        }
        Ok(Self {
            php_src,
            symbols: symbols.unwrap_or_else(|| PathBuf::from(DEFAULT_SYMBOLS)),
        })
    }
}

#[derive(Debug)]
struct LookupOptions {
    symbols: PathBuf,
    symbol: String,
}

#[derive(Debug)]
struct PhptIndexOptions {
    php_src: PathBuf,
    out: PathBuf,
    report: PathBuf,
}

#[derive(Clone, Debug)]
struct RunOptions {
    target: PathBuf,
    target_mode: TargetMode,
    manifest: PathBuf,
    php_src: PathBuf,
    work_dir: PathBuf,
    out: PathBuf,
    summary: PathBuf,
    reuse_results: Option<PathBuf>,
    dev_reuse_pass: bool,
    cleanup_work: bool,
    timeout: Duration,
    jobs: usize,
}

#[derive(Debug)]
struct RunContext {
    options: RunOptions,
    target_fingerprint: String,
    runner_fingerprint: String,
    cached_results: BTreeMap<String, PhptRunResult>,
}

#[derive(Debug)]
struct RerunManifestOptions {
    results: PathBuf,
    out: PathBuf,
}

#[derive(Debug)]
struct BaselineOptions {
    results: PathBuf,
    corpus: PathBuf,
    known_failures: PathBuf,
    metadata: PathBuf,
    module_counts: PathBuf,
    report: PathBuf,
    previous_known_failures: Option<PathBuf>,
    previous_results: Option<PathBuf>,
    timestamp: String,
}

#[derive(Debug)]
struct VerifyBaselineOptions {
    corpus: PathBuf,
    known_failures: PathBuf,
    metadata: PathBuf,
    module_counts: PathBuf,
    known_gap_catalog: PathBuf,
    report: PathBuf,
}

#[derive(Debug)]
struct TriageOptions {
    corpus: PathBuf,
    known_failures: PathBuf,
    metadata: PathBuf,
    module_counts: PathBuf,
    results: Option<PathBuf>,
    report: PathBuf,
    extension_policy_report: PathBuf,
    known_gap_report: PathBuf,
    known_gap_catalog: PathBuf,
    priority: PathBuf,
    modules_dir: PathBuf,
    module_manifests_dir: PathBuf,
    selected_limit: usize,
}

#[derive(Debug)]
struct GenerateOptions {
    module: String,
    php_src: PathBuf,
    reference: PathBuf,
    corpus: PathBuf,
    known_failures: PathBuf,
    generated_dir: PathBuf,
    module_manifest: PathBuf,
    generated_manifest: PathBuf,
    work_dir: PathBuf,
    timestamp: String,
    smoke_count: usize,
    regression_count: usize,
    timeout: Duration,
}

impl RunOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut target = None;
        let mut manifest = None;
        let mut php_src = None;
        let mut work_dir = None;
        let mut out = None;
        let mut summary = None;
        let mut reuse_results = None;
        let mut target_mode = None;
        let mut timeout = None;
        let mut jobs = None;
        let mut dev_reuse_pass = false;
        let mut cleanup_work = false;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "--target" => {
                    index += 1;
                    target = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--target requires a path".to_string())?,
                    ));
                }
                "--manifest" => {
                    index += 1;
                    manifest = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--manifest requires a path".to_string())?,
                    ));
                }
                "--target-mode" => {
                    index += 1;
                    target_mode =
                        Some(TargetMode::parse(args.get(index).ok_or_else(|| {
                            "--target-mode requires php-cli or php-vm".to_string()
                        })?)?);
                }
                "--php-src" => {
                    index += 1;
                    php_src = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--php-src requires a path".to_string())?,
                    ));
                }
                "--work-dir" => {
                    index += 1;
                    work_dir = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--work-dir requires a path".to_string())?,
                    ));
                }
                "--out" => {
                    index += 1;
                    out = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--out requires a path".to_string())?,
                    ));
                }
                "--summary" => {
                    index += 1;
                    summary = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--summary requires a path".to_string())?,
                    ));
                }
                "--reuse-results" => {
                    index += 1;
                    reuse_results =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--reuse-results requires a path".to_string()
                        })?));
                }
                "--timeout-seconds" => {
                    index += 1;
                    timeout = Some(parse_duration_seconds(
                        args.get(index)
                            .ok_or_else(|| "--timeout-seconds requires a number".to_string())?,
                    )?);
                }
                "--jobs" => {
                    index += 1;
                    jobs = Some(parse_jobs(
                        args.get(index)
                            .ok_or_else(|| "--jobs requires a number".to_string())?,
                    )?);
                }
                "--dev-reuse-pass" => {
                    dev_reuse_pass = true;
                }
                "--cleanup-work" => {
                    cleanup_work = true;
                }
                _ if arg.starts_with("--target=") => {
                    target = Some(PathBuf::from(arg.trim_start_matches("--target=")));
                }
                _ if arg.starts_with("--manifest=") => {
                    manifest = Some(PathBuf::from(arg.trim_start_matches("--manifest=")));
                }
                _ if arg.starts_with("--target-mode=") => {
                    target_mode =
                        Some(TargetMode::parse(arg.trim_start_matches("--target-mode="))?);
                }
                _ if arg.starts_with("--php-src=") => {
                    php_src = Some(PathBuf::from(arg.trim_start_matches("--php-src=")));
                }
                _ if arg.starts_with("--work-dir=") => {
                    work_dir = Some(PathBuf::from(arg.trim_start_matches("--work-dir=")));
                }
                _ if arg.starts_with("--out=") => {
                    out = Some(PathBuf::from(arg.trim_start_matches("--out=")));
                }
                _ if arg.starts_with("--summary=") => {
                    summary = Some(PathBuf::from(arg.trim_start_matches("--summary=")));
                }
                _ if arg.starts_with("--reuse-results=") => {
                    reuse_results = Some(PathBuf::from(arg.trim_start_matches("--reuse-results=")));
                }
                _ if arg.starts_with("--timeout-seconds=") => {
                    timeout = Some(parse_duration_seconds(
                        arg.trim_start_matches("--timeout-seconds="),
                    )?);
                }
                _ if arg.starts_with("--jobs=") => {
                    jobs = Some(parse_jobs(arg.trim_start_matches("--jobs="))?);
                }
                _ if arg.starts_with("--dev-reuse-pass=") => {
                    dev_reuse_pass = parse_bool_flag(
                        arg.trim_start_matches("--dev-reuse-pass="),
                        "--dev-reuse-pass",
                    )?;
                }
                _ if arg.starts_with("--cleanup-work=") => {
                    cleanup_work = parse_bool_flag(
                        arg.trim_start_matches("--cleanup-work="),
                        "--cleanup-work",
                    )?;
                }
                _ => return Err(format!("unknown run option `{arg}`")),
            }
            index += 1;
        }
        let php_src = php_src
            .or_else(|| env::var_os("PHP_SRC_DIR").map(PathBuf::from))
            .unwrap_or_else(default_php_src_dir);
        let target = target
            .or_else(|| env::var_os("TARGET_PHP").map(PathBuf::from))
            .ok_or_else(|| "run requires --target or TARGET_PHP".to_string())?;
        let manifest = manifest.ok_or_else(|| "run requires --manifest".to_string())?;
        Ok(Self {
            target_mode: target_mode
                .or_else(|| {
                    env::var("PHPT_TARGET_MODE")
                        .ok()
                        .and_then(|value| TargetMode::parse(&value).ok())
                })
                .unwrap_or_else(|| infer_target_mode(&target)),
            target,
            manifest,
            php_src,
            work_dir: work_dir
                .or_else(|| env::var_os("PHPT_WORK_DIR").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("target/phpt-work")),
            out: out.unwrap_or_else(|| PathBuf::from("target/phpt-work/module-runs/results.jsonl")),
            summary: summary
                .unwrap_or_else(|| PathBuf::from("target/phpt-work/module-runs/summary.md")),
            reuse_results: reuse_results
                .or_else(|| env::var_os("PHPT_REUSE_RESULTS").map(PathBuf::from)),
            dev_reuse_pass: dev_reuse_pass || env_flag("PHPT_DEV_REUSE_PASS"),
            cleanup_work: cleanup_work || env_flag("PHPT_CLEANUP_WORK"),
            timeout: timeout
                .or_else(|| {
                    env::var("PHPT_TIMEOUT_SECONDS")
                        .ok()
                        .and_then(|value| parse_duration_seconds(&value).ok())
                })
                .unwrap_or_else(|| Duration::from_secs(10)),
            jobs: jobs
                .or_else(|| {
                    env::var("PHPT_JOBS")
                        .ok()
                        .and_then(|value| parse_jobs(&value).ok())
                })
                .unwrap_or_else(default_phpt_jobs),
        })
    }
}

impl RerunManifestOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut results = None;
        let mut out = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "--results" => {
                    index += 1;
                    results = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--results requires a path".to_string())?,
                    ));
                }
                "--out" => {
                    index += 1;
                    out = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--out requires a path".to_string())?,
                    ));
                }
                _ if arg.starts_with("--results=") => {
                    results = Some(PathBuf::from(arg.trim_start_matches("--results=")));
                }
                _ if arg.starts_with("--out=") => {
                    out = Some(PathBuf::from(arg.trim_start_matches("--out=")));
                }
                _ => return Err(format!("unknown rerun-manifest option `{arg}`")),
            }
            index += 1;
        }
        Ok(Self {
            results: results.ok_or_else(|| "rerun-manifest requires --results".to_string())?,
            out: out.ok_or_else(|| "rerun-manifest requires --out".to_string())?,
        })
    }
}

impl BaselineOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut results = None;
        let mut corpus = None;
        let mut known_failures = None;
        let mut metadata = None;
        let mut module_counts = None;
        let mut report = None;
        let mut previous_known_failures = None;
        let mut previous_results = None;
        let mut timestamp = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "--results" => {
                    index += 1;
                    results = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--results requires a path".to_string())?,
                    ));
                }
                "--corpus" => {
                    index += 1;
                    corpus = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--corpus requires a path".to_string())?,
                    ));
                }
                "--known-failures" => {
                    index += 1;
                    known_failures =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--known-failures requires a path".to_string()
                        })?));
                }
                "--metadata" => {
                    index += 1;
                    metadata = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--metadata requires a path".to_string())?,
                    ));
                }
                "--module-counts" => {
                    index += 1;
                    module_counts =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--module-counts requires a path".to_string()
                        })?));
                }
                "--report" => {
                    index += 1;
                    report = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--report requires a path".to_string())?,
                    ));
                }
                "--previous-known-failures" => {
                    index += 1;
                    previous_known_failures =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--previous-known-failures requires a path".to_string()
                        })?));
                }
                "--previous-results" => {
                    index += 1;
                    previous_results =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--previous-results requires a path".to_string()
                        })?));
                }
                "--timestamp" => {
                    index += 1;
                    timestamp = Some(
                        args.get(index)
                            .ok_or_else(|| "--timestamp requires a value".to_string())?
                            .to_string(),
                    );
                }
                _ if arg.starts_with("--results=") => {
                    results = Some(PathBuf::from(arg.trim_start_matches("--results=")));
                }
                _ if arg.starts_with("--corpus=") => {
                    corpus = Some(PathBuf::from(arg.trim_start_matches("--corpus=")));
                }
                _ if arg.starts_with("--known-failures=") => {
                    known_failures =
                        Some(PathBuf::from(arg.trim_start_matches("--known-failures=")));
                }
                _ if arg.starts_with("--metadata=") => {
                    metadata = Some(PathBuf::from(arg.trim_start_matches("--metadata=")));
                }
                _ if arg.starts_with("--module-counts=") => {
                    module_counts = Some(PathBuf::from(arg.trim_start_matches("--module-counts=")));
                }
                _ if arg.starts_with("--report=") => {
                    report = Some(PathBuf::from(arg.trim_start_matches("--report=")));
                }
                _ if arg.starts_with("--previous-known-failures=") => {
                    previous_known_failures = Some(PathBuf::from(
                        arg.trim_start_matches("--previous-known-failures="),
                    ));
                }
                _ if arg.starts_with("--previous-results=") => {
                    previous_results =
                        Some(PathBuf::from(arg.trim_start_matches("--previous-results=")));
                }
                _ if arg.starts_with("--timestamp=") => {
                    timestamp = Some(arg.trim_start_matches("--timestamp=").to_string());
                }
                _ => return Err(format!("unknown baseline option `{arg}`")),
            }
            index += 1;
        }
        Ok(Self {
            results: results.ok_or_else(|| "baseline requires --results".to_string())?,
            corpus: corpus.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_CORPUS)),
            known_failures: known_failures
                .unwrap_or_else(|| PathBuf::from("tests/phpt/manifests/full-known-failures.jsonl")),
            metadata: metadata.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_BASELINE_METADATA)),
            module_counts: module_counts
                .unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_BASELINE_MODULE_COUNTS)),
            report: report
                .unwrap_or_else(|| PathBuf::from("target/phpt-work/reports/full-baseline.md")),
            previous_known_failures,
            previous_results,
            timestamp: timestamp
                .or_else(|| env::var("PHPT_BASELINE_TIMESTAMP").ok())
                .unwrap_or_else(|| "unknown".to_string()),
        })
    }
}

impl VerifyBaselineOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut corpus = None;
        let mut known_failures = None;
        let mut metadata = None;
        let mut module_counts = None;
        let mut known_gap_catalog = None;
        let mut report = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "--corpus" => {
                    index += 1;
                    corpus = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--corpus requires a path".to_string())?,
                    ));
                }
                "--known-failures" => {
                    index += 1;
                    known_failures =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--known-failures requires a path".to_string()
                        })?));
                }
                "--metadata" => {
                    index += 1;
                    metadata = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--metadata requires a path".to_string())?,
                    ));
                }
                "--module-counts" => {
                    index += 1;
                    module_counts =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--module-counts requires a path".to_string()
                        })?));
                }
                "--known-gap-catalog" => {
                    index += 1;
                    known_gap_catalog =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--known-gap-catalog requires a path".to_string()
                        })?));
                }
                "--report" => {
                    index += 1;
                    report = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--report requires a path".to_string())?,
                    ));
                }
                _ if arg.starts_with("--corpus=") => {
                    corpus = Some(PathBuf::from(arg.trim_start_matches("--corpus=")));
                }
                _ if arg.starts_with("--known-failures=") => {
                    known_failures =
                        Some(PathBuf::from(arg.trim_start_matches("--known-failures=")));
                }
                _ if arg.starts_with("--metadata=") => {
                    metadata = Some(PathBuf::from(arg.trim_start_matches("--metadata=")));
                }
                _ if arg.starts_with("--module-counts=") => {
                    module_counts = Some(PathBuf::from(arg.trim_start_matches("--module-counts=")));
                }
                _ if arg.starts_with("--known-gap-catalog=") => {
                    known_gap_catalog = Some(PathBuf::from(
                        arg.trim_start_matches("--known-gap-catalog="),
                    ));
                }
                _ if arg.starts_with("--report=") => {
                    report = Some(PathBuf::from(arg.trim_start_matches("--report=")));
                }
                _ => return Err(format!("unknown verify-baseline option `{arg}`")),
            }
            index += 1;
        }
        Ok(Self {
            corpus: corpus.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_CORPUS)),
            known_failures: known_failures
                .unwrap_or_else(|| PathBuf::from("tests/phpt/manifests/full-known-failures.jsonl")),
            metadata: metadata.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_BASELINE_METADATA)),
            module_counts: module_counts
                .unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_BASELINE_MODULE_COUNTS)),
            known_gap_catalog: known_gap_catalog
                .unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_KNOWN_GAP_CATALOG)),
            report: report
                .unwrap_or_else(|| PathBuf::from("target/phpt-work/reports/full-baseline.md")),
        })
    }
}

impl TriageOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut corpus = None;
        let mut known_failures = None;
        let mut metadata = None;
        let mut module_counts = None;
        let mut results = None;
        let mut report = None;
        let mut extension_policy_report = None;
        let mut known_gap_report = None;
        let mut known_gap_catalog = None;
        let mut priority = None;
        let mut modules_dir = None;
        let mut module_manifests_dir = None;
        let mut selected_limit = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "--corpus" => {
                    index += 1;
                    corpus = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--corpus requires a path".to_string())?,
                    ));
                }
                "--known-failures" => {
                    index += 1;
                    known_failures =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--known-failures requires a path".to_string()
                        })?));
                }
                "--metadata" => {
                    index += 1;
                    metadata = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--metadata requires a path".to_string())?,
                    ));
                }
                "--module-counts" => {
                    index += 1;
                    module_counts =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--module-counts requires a path".to_string()
                        })?));
                }
                "--results" => {
                    index += 1;
                    results = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--results requires a path".to_string())?,
                    ));
                }
                "--report" => {
                    index += 1;
                    report = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--report requires a path".to_string())?,
                    ));
                }
                "--extension-policy-report" => {
                    index += 1;
                    extension_policy_report =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--extension-policy-report requires a path".to_string()
                        })?));
                }
                "--known-gap-report" => {
                    index += 1;
                    known_gap_report =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--known-gap-report requires a path".to_string()
                        })?));
                }
                "--known-gap-catalog" => {
                    index += 1;
                    known_gap_catalog =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--known-gap-catalog requires a path".to_string()
                        })?));
                }
                "--priority" => {
                    index += 1;
                    priority = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--priority requires a path".to_string())?,
                    ));
                }
                "--modules-dir" => {
                    index += 1;
                    modules_dir =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--modules-dir requires a path".to_string()
                        })?));
                }
                "--module-manifests-dir" => {
                    index += 1;
                    module_manifests_dir =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--module-manifests-dir requires a path".to_string()
                        })?));
                }
                "--selected-limit" => {
                    index += 1;
                    selected_limit = Some(parse_usize(
                        args.get(index)
                            .ok_or_else(|| "--selected-limit requires a number".to_string())?,
                        "--selected-limit",
                    )?);
                }
                _ if arg.starts_with("--corpus=") => {
                    corpus = Some(PathBuf::from(arg.trim_start_matches("--corpus=")));
                }
                _ if arg.starts_with("--known-failures=") => {
                    known_failures =
                        Some(PathBuf::from(arg.trim_start_matches("--known-failures=")));
                }
                _ if arg.starts_with("--metadata=") => {
                    metadata = Some(PathBuf::from(arg.trim_start_matches("--metadata=")));
                }
                _ if arg.starts_with("--module-counts=") => {
                    module_counts = Some(PathBuf::from(arg.trim_start_matches("--module-counts=")));
                }
                _ if arg.starts_with("--results=") => {
                    results = Some(PathBuf::from(arg.trim_start_matches("--results=")));
                }
                _ if arg.starts_with("--report=") => {
                    report = Some(PathBuf::from(arg.trim_start_matches("--report=")));
                }
                _ if arg.starts_with("--extension-policy-report=") => {
                    extension_policy_report = Some(PathBuf::from(
                        arg.trim_start_matches("--extension-policy-report="),
                    ));
                }
                _ if arg.starts_with("--known-gap-report=") => {
                    known_gap_report =
                        Some(PathBuf::from(arg.trim_start_matches("--known-gap-report=")));
                }
                _ if arg.starts_with("--known-gap-catalog=") => {
                    known_gap_catalog = Some(PathBuf::from(
                        arg.trim_start_matches("--known-gap-catalog="),
                    ));
                }
                _ if arg.starts_with("--priority=") => {
                    priority = Some(PathBuf::from(arg.trim_start_matches("--priority=")));
                }
                _ if arg.starts_with("--modules-dir=") => {
                    modules_dir = Some(PathBuf::from(arg.trim_start_matches("--modules-dir=")));
                }
                _ if arg.starts_with("--module-manifests-dir=") => {
                    module_manifests_dir = Some(PathBuf::from(
                        arg.trim_start_matches("--module-manifests-dir="),
                    ));
                }
                _ if arg.starts_with("--selected-limit=") => {
                    selected_limit = Some(parse_usize(
                        arg.trim_start_matches("--selected-limit="),
                        "--selected-limit",
                    )?);
                }
                _ => return Err(format!("unknown triage option `{arg}`")),
            }
            index += 1;
        }
        let results = results.or_else(|| env::var_os("PHPT_RESULTS").map(PathBuf::from));
        Ok(Self {
            corpus: corpus.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_CORPUS)),
            known_failures: known_failures
                .unwrap_or_else(|| PathBuf::from("tests/phpt/manifests/full-known-failures.jsonl")),
            metadata: metadata.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_BASELINE_METADATA)),
            module_counts: module_counts
                .unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_BASELINE_MODULE_COUNTS)),
            results,
            report: report.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_TRIAGE_REPORT)),
            extension_policy_report: extension_policy_report
                .unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_EXTENSION_POLICY_REPORT)),
            known_gap_report: known_gap_report
                .unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_KNOWN_GAP_REPORT)),
            known_gap_catalog: known_gap_catalog
                .unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_KNOWN_GAP_CATALOG)),
            priority: priority.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_MODULE_PRIORITY)),
            modules_dir: modules_dir.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_MODULE_DOCS_DIR)),
            module_manifests_dir: module_manifests_dir
                .unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_MODULE_MANIFESTS_DIR)),
            selected_limit: selected_limit.unwrap_or(200),
        })
    }
}

impl GenerateOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut module = None;
        let mut php_src = None;
        let mut reference = None;
        let mut corpus = None;
        let mut known_failures = None;
        let mut generated_dir = None;
        let mut module_manifest = None;
        let mut generated_manifest = None;
        let mut work_dir = None;
        let mut timestamp = None;
        let mut smoke_count = None;
        let mut regression_count = None;
        let mut timeout = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "--module" => {
                    index += 1;
                    module = Some(
                        args.get(index)
                            .ok_or_else(|| "--module requires a value".to_string())?
                            .to_string(),
                    );
                }
                "--php-src" => {
                    index += 1;
                    php_src = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--php-src requires a path".to_string())?,
                    ));
                }
                "--reference" => {
                    index += 1;
                    reference = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--reference requires a path".to_string())?,
                    ));
                }
                "--corpus" => {
                    index += 1;
                    corpus = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--corpus requires a path".to_string())?,
                    ));
                }
                "--known-failures" => {
                    index += 1;
                    known_failures =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--known-failures requires a path".to_string()
                        })?));
                }
                "--generated-dir" => {
                    index += 1;
                    generated_dir =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--generated-dir requires a path".to_string()
                        })?));
                }
                "--module-manifest" => {
                    index += 1;
                    module_manifest =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--module-manifest requires a path".to_string()
                        })?));
                }
                "--generated-manifest" => {
                    index += 1;
                    generated_manifest =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--generated-manifest requires a path".to_string()
                        })?));
                }
                "--work-dir" => {
                    index += 1;
                    work_dir = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--work-dir requires a path".to_string())?,
                    ));
                }
                "--timestamp" => {
                    index += 1;
                    timestamp = Some(
                        args.get(index)
                            .ok_or_else(|| "--timestamp requires a value".to_string())?
                            .to_string(),
                    );
                }
                "--smoke-count" => {
                    index += 1;
                    smoke_count = Some(parse_usize(
                        args.get(index)
                            .ok_or_else(|| "--smoke-count requires a number".to_string())?,
                        "--smoke-count",
                    )?);
                }
                "--regression-count" => {
                    index += 1;
                    regression_count = Some(parse_usize(
                        args.get(index)
                            .ok_or_else(|| "--regression-count requires a number".to_string())?,
                        "--regression-count",
                    )?);
                }
                "--timeout-seconds" => {
                    index += 1;
                    timeout = Some(parse_duration_seconds(
                        args.get(index)
                            .ok_or_else(|| "--timeout-seconds requires a number".to_string())?,
                    )?);
                }
                _ if arg.starts_with("MODULE=") => {
                    module = Some(arg.trim_start_matches("MODULE=").to_string());
                }
                _ if arg.starts_with("--module=") => {
                    module = Some(arg.trim_start_matches("--module=").to_string());
                }
                _ if arg.starts_with("--php-src=") => {
                    php_src = Some(PathBuf::from(arg.trim_start_matches("--php-src=")));
                }
                _ if arg.starts_with("--reference=") => {
                    reference = Some(PathBuf::from(arg.trim_start_matches("--reference=")));
                }
                _ if arg.starts_with("--corpus=") => {
                    corpus = Some(PathBuf::from(arg.trim_start_matches("--corpus=")));
                }
                _ if arg.starts_with("--known-failures=") => {
                    known_failures =
                        Some(PathBuf::from(arg.trim_start_matches("--known-failures=")));
                }
                _ if arg.starts_with("--generated-dir=") => {
                    generated_dir = Some(PathBuf::from(arg.trim_start_matches("--generated-dir=")));
                }
                _ if arg.starts_with("--module-manifest=") => {
                    module_manifest =
                        Some(PathBuf::from(arg.trim_start_matches("--module-manifest=")));
                }
                _ if arg.starts_with("--generated-manifest=") => {
                    generated_manifest = Some(PathBuf::from(
                        arg.trim_start_matches("--generated-manifest="),
                    ));
                }
                _ if arg.starts_with("--work-dir=") => {
                    work_dir = Some(PathBuf::from(arg.trim_start_matches("--work-dir=")));
                }
                _ if arg.starts_with("--timestamp=") => {
                    timestamp = Some(arg.trim_start_matches("--timestamp=").to_string());
                }
                _ if arg.starts_with("--smoke-count=") => {
                    smoke_count = Some(parse_usize(
                        arg.trim_start_matches("--smoke-count="),
                        "--smoke-count",
                    )?);
                }
                _ if arg.starts_with("--regression-count=") => {
                    regression_count = Some(parse_usize(
                        arg.trim_start_matches("--regression-count="),
                        "--regression-count",
                    )?);
                }
                _ if arg.starts_with("--timeout-seconds=") => {
                    timeout = Some(parse_duration_seconds(
                        arg.trim_start_matches("--timeout-seconds="),
                    )?);
                }
                _ => return Err(format!("unknown generate option `{arg}`")),
            }
            index += 1;
        }
        let module = module
            .or_else(|| env::var("MODULE").ok())
            .ok_or_else(|| "generate requires --module or MODULE".to_string())?;
        let safe_module = safe_path_component(&module);
        let php_src = php_src
            .or_else(|| env::var_os("PHP_SRC_DIR").map(PathBuf::from))
            .unwrap_or_else(default_php_src_dir);
        let reference = reference
            .or_else(|| env::var_os("REFERENCE_PHP").map(PathBuf::from))
            .unwrap_or_else(|| php_src.join("sapi/cli/php"));
        if !reference.is_file() {
            return Err(format!(
                "reference PHP CLI is not built: {}; set REFERENCE_PHP",
                reference.display()
            ));
        }
        Ok(Self {
            module,
            php_src,
            reference,
            corpus: corpus.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_CORPUS)),
            known_failures: known_failures
                .unwrap_or_else(|| PathBuf::from("tests/phpt/manifests/full-known-failures.jsonl")),
            generated_dir: generated_dir
                .unwrap_or_else(|| PathBuf::from("tests/phpt/generated").join(&safe_module)),
            module_manifest: module_manifest.unwrap_or_else(|| {
                PathBuf::from("tests/phpt/manifests").join(format!("{safe_module}-originals.jsonl"))
            }),
            generated_manifest: generated_manifest.unwrap_or_else(|| {
                PathBuf::from("tests/phpt/manifests").join(format!("{safe_module}-generated.jsonl"))
            }),
            work_dir: work_dir.unwrap_or_else(|| {
                PathBuf::from("target/phpt-work")
                    .join("generate")
                    .join(&safe_module)
            }),
            timestamp: timestamp
                .or_else(|| env::var("PHPT_GENERATED_TIMESTAMP").ok())
                .unwrap_or_else(|| "unknown".to_string()),
            smoke_count: smoke_count.unwrap_or(3),
            regression_count: regression_count.unwrap_or(2),
            timeout: timeout
                .or_else(|| {
                    env::var("PHPT_TIMEOUT_SECONDS")
                        .ok()
                        .and_then(|value| parse_duration_seconds(&value).ok())
                })
                .unwrap_or_else(|| Duration::from_secs(10)),
        })
    }
}

impl RunContext {
    fn new(options: RunOptions) -> Result<Self, String> {
        let target_fingerprint = file_fingerprint(&options.target)?;
        let runner_fingerprint = env::current_exe()
            .ok()
            .and_then(|path| file_fingerprint(&path).ok())
            .unwrap_or_else(|| "runner=unknown".to_string());
        let cached_results = match &options.reuse_results {
            Some(path) if path.is_file() => read_run_results(path)?
                .into_iter()
                .filter(|result| result.cache_key.is_some())
                .map(|mut result| {
                    result.cache_status = None;
                    (result.path.clone(), result)
                })
                .collect(),
            Some(path) => {
                return Err(format!(
                    "PHPT reuse result file does not exist: {}",
                    path.display()
                ));
            }
            None => BTreeMap::new(),
        };
        Ok(Self {
            options,
            target_fingerprint,
            runner_fingerprint,
            cached_results,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetMode {
    PhpCli,
    PhpVm,
}

impl TargetMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::PhpCli => "php-cli",
            Self::PhpVm => "php-vm",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "php-cli" => Ok(Self::PhpCli),
            "php-vm" => Ok(Self::PhpVm),
            _ => Err(format!(
                "unknown target mode `{value}`; expected php-cli or php-vm"
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PhptRunResult {
    path: String,
    outcome: String,
    detail: String,
    mismatch_category: Option<MismatchCategory>,
    cache_key: Option<String>,
    input_cache_key: Option<String>,
    cache_status: Option<String>,
}

impl PhptRunResult {
    fn new(path: impl Into<String>, outcome: impl Into<String>, detail: impl Into<String>) -> Self {
        let outcome = outcome.into();
        let detail = detail.into();
        Self {
            path: path.into(),
            mismatch_category: classify_phpt_detail(&outcome, &detail),
            outcome,
            detail,
            cache_key: None,
            input_cache_key: None,
            cache_status: None,
        }
    }

    fn with_cache_keys(mut self, cache_key: String, input_cache_key: String) -> Self {
        self.cache_key = Some(cache_key);
        self.input_cache_key = Some(input_cache_key);
        self.cache_status = Some("miss".to_string());
        self
    }

    fn mark_cache_hit(mut self) -> Self {
        self.cache_status = Some("hit".to_string());
        self
    }

    fn mark_dev_pass_cache_hit(mut self) -> Self {
        self.cache_status = Some("dev-pass-hit".to_string());
        self
    }

    fn to_json_line(&self) -> String {
        let mut line = format!(
            "{{\"path\":\"{}\",\"outcome\":\"{}\",\"detail\":\"{}\"",
            escape_json(&self.path),
            escape_json(&self.outcome),
            escape_json(&self.detail)
        );
        if let Some(category) = self.mismatch_category {
            line.push_str(&format!(",\"mismatch_category\":\"{}\"", category.as_str()));
        }
        if let Some(cache_key) = &self.cache_key {
            line.push_str(&format!(",\"cache_key\":\"{}\"", escape_json(cache_key)));
        }
        if let Some(input_cache_key) = &self.input_cache_key {
            line.push_str(&format!(
                ",\"input_cache_key\":\"{}\"",
                escape_json(input_cache_key)
            ));
        }
        if let Some(cache_status) = &self.cache_status {
            line.push_str(&format!(
                ",\"cache_status\":\"{}\"",
                escape_json(cache_status)
            ));
        }
        line.push('}');
        line
    }

    fn from_json_line(line: &str) -> Result<Self, String> {
        let category = extract_optional_json_string(line, "mismatch_category")?
            .as_deref()
            .map(parse_mismatch_category)
            .transpose()?;
        Ok(Self {
            path: extract_json_string(line, "path")?,
            outcome: extract_json_string(line, "outcome")?,
            detail: extract_json_string(line, "detail")?,
            mismatch_category: category,
            cache_key: extract_optional_json_string(line, "cache_key")?,
            input_cache_key: extract_optional_json_string(line, "input_cache_key")?,
            cache_status: extract_optional_json_string(line, "cache_status")?,
        })
    }
}

fn parse_mismatch_category(value: &str) -> Result<MismatchCategory, String> {
    MismatchCategory::parse(value).ok_or_else(|| format!("unknown mismatch_category `{value}`"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct KnownFailure {
    path: String,
    module_tag: String,
    outcome: String,
    failure_fingerprint: String,
    primary_missing_feature_guess: String,
    owner_module: String,
    first_seen_timestamp: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineMetadata {
    schema_version: String,
    timestamp: String,
    corpus_count: usize,
    pass_count: usize,
    skip_count: usize,
    xfail_count: usize,
    fail_count: usize,
    bork_count: usize,
    known_failure_count: usize,
    failure_manifest: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BaselineModuleCount {
    kind: String,
    module: String,
    corpus_count: usize,
    pass_count: usize,
    skip_count: usize,
    fail_count: usize,
    bork_count: usize,
    known_failure_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct KnownGapCatalogEntry {
    id: String,
    title: String,
    reference_behavior: String,
    current_rust_behavior: String,
    fixture_or_phpt_example: String,
    planned_solution_layer: String,
    baseline_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GeneratedCase {
    path: PathBuf,
    manifest_path: String,
    module: String,
    kind: String,
    original_path: String,
    original_source_hash: String,
    generated_timestamp: String,
    generator_version: String,
    reason: String,
    source: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReductionMode {
    LineRemoval,
}

mod policy;
use policy::{EXTENSION_POLICY, KNOWN_GAP_CATALOG, MODULE_PLAN, ModulePlanSpec};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ModuleTriageStats {
    corpus_count: usize,
    pass_count: usize,
    skip_count: usize,
    fail_count: usize,
    bork_count: usize,
    known_failure_count: usize,
    failure_clusters: BTreeMap<String, usize>,
    bork_subclasses: BTreeMap<String, usize>,
    relevant_paths: Vec<String>,
    selected_paths: Vec<String>,
}

impl ModuleTriageStats {
    fn non_green(&self) -> usize {
        self.fail_count + self.bork_count
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PhptTriage {
    modules: BTreeMap<String, ModuleTriageStats>,
    raw_modules: BTreeMap<String, ModuleTriageStats>,
    failure_clusters: BTreeMap<String, usize>,
    unsupported_guesses: BTreeMap<String, usize>,
    bork_subclasses: BTreeMap<String, usize>,
    has_result_counts: bool,
    count_source: String,
}

impl KnownFailure {
    fn to_json_line(&self) -> String {
        format!(
            "{{\"path\":\"{}\",\"module_tag\":\"{}\",\"outcome\":\"{}\",\"failure_fingerprint\":\"{}\",\"primary_missing_feature_guess\":\"{}\",\"owner_module\":\"{}\",\"first_seen_timestamp\":\"{}\"}}",
            escape_json(&self.path),
            escape_json(&self.module_tag),
            escape_json(&self.outcome),
            escape_json(&self.failure_fingerprint),
            escape_json(&self.primary_missing_feature_guess),
            escape_json(&self.owner_module),
            escape_json(&self.first_seen_timestamp)
        )
    }

    fn from_json_line(line: &str) -> Result<Self, String> {
        Ok(Self {
            path: extract_json_string(line, "path")?,
            module_tag: extract_json_string(line, "module_tag")?,
            outcome: extract_json_string(line, "outcome")?,
            failure_fingerprint: extract_json_string(line, "failure_fingerprint")?,
            primary_missing_feature_guess: extract_json_string(
                line,
                "primary_missing_feature_guess",
            )?,
            owner_module: extract_json_string(line, "owner_module")?,
            first_seen_timestamp: extract_json_string(line, "first_seen_timestamp")?,
        })
    }
}

impl BaselineMetadata {
    fn from_results(
        results: &[PhptRunResult],
        known_failure_count: usize,
        timestamp: &str,
        failure_manifest: &Path,
    ) -> Self {
        let mut outcomes = BTreeMap::<String, usize>::new();
        for result in results {
            *outcomes.entry(result.outcome.clone()).or_default() += 1;
        }
        Self {
            schema_version: "phpt-full-baseline-v1".to_string(),
            timestamp: timestamp.to_string(),
            corpus_count: results.len(),
            pass_count: *outcomes.get("PASS").unwrap_or(&0),
            skip_count: *outcomes.get("SKIP").unwrap_or(&0),
            xfail_count: *outcomes.get("XFAIL").unwrap_or(&0),
            fail_count: *outcomes.get("FAIL").unwrap_or(&0),
            bork_count: *outcomes.get("BORK").unwrap_or(&0),
            known_failure_count,
            failure_manifest: failure_manifest.to_string_lossy().replace('\\', "/"),
        }
    }

    fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"schema_version\":\"{}\",\n",
                "  \"timestamp\":\"{}\",\n",
                "  \"corpus_count\":{},\n",
                "  \"pass_count\":{},\n",
                "  \"skip_count\":{},\n",
                "  \"xfail_count\":{},\n",
                "  \"fail_count\":{},\n",
                "  \"bork_count\":{},\n",
                "  \"known_failure_count\":{},\n",
                "  \"failure_manifest\":\"{}\"\n",
                "}}\n"
            ),
            escape_json(&self.schema_version),
            escape_json(&self.timestamp),
            self.corpus_count,
            self.pass_count,
            self.skip_count,
            self.xfail_count,
            self.fail_count,
            self.bork_count,
            self.known_failure_count,
            escape_json(&self.failure_manifest)
        )
    }

    fn from_json(source: &str) -> Result<Self, String> {
        Ok(Self {
            schema_version: extract_json_string(source, "schema_version")?,
            timestamp: extract_json_string(source, "timestamp")?,
            corpus_count: extract_json_usize(source, "corpus_count")?,
            pass_count: extract_json_usize(source, "pass_count")?,
            skip_count: extract_json_usize(source, "skip_count")?,
            // Older baselines predate XFAIL bookkeeping; absent means zero.
            xfail_count: if source.contains("\"xfail_count\":") {
                extract_json_usize(source, "xfail_count")?
            } else {
                0
            },
            fail_count: extract_json_usize(source, "fail_count")?,
            bork_count: extract_json_usize(source, "bork_count")?,
            known_failure_count: extract_json_usize(source, "known_failure_count")?,
            failure_manifest: extract_json_string(source, "failure_manifest")?,
        })
    }
}

impl BaselineModuleCount {
    fn from_json_line(line: &str) -> Result<Self, String> {
        Ok(Self {
            kind: extract_json_string(line, "kind")?,
            module: extract_json_string(line, "module")?,
            corpus_count: extract_json_usize(line, "corpus_count")?,
            pass_count: extract_json_usize(line, "pass_count")?,
            skip_count: extract_json_usize(line, "skip_count")?,
            fail_count: extract_json_usize(line, "fail_count")?,
            bork_count: extract_json_usize(line, "bork_count")?,
            known_failure_count: extract_json_usize(line, "known_failure_count")?,
        })
    }
}

impl KnownGapCatalogEntry {
    fn from_json_line(line: &str) -> Result<Self, String> {
        Ok(Self {
            id: extract_json_string(line, "id")?,
            title: extract_json_string(line, "title")?,
            reference_behavior: extract_json_string(line, "reference_behavior")?,
            current_rust_behavior: extract_json_string(line, "current_rust_behavior")?,
            fixture_or_phpt_example: extract_json_string(line, "fixture_or_phpt_example")?,
            planned_solution_layer: extract_json_string(line, "planned_solution_layer")?,
            baseline_count: extract_json_usize(line, "baseline_count")?,
        })
    }

    fn to_json_line(&self) -> String {
        format!(
            "{{\"schema_version\":\"phpt-known-gap-v1\",\"id\":\"{}\",\"title\":\"{}\",\"reference_behavior\":\"{}\",\"current_rust_behavior\":\"{}\",\"fixture_or_phpt_example\":\"{}\",\"planned_solution_layer\":\"{}\",\"baseline_count\":{}}}",
            escape_json(&self.id),
            escape_json(&self.title),
            escape_json(&self.reference_behavior),
            escape_json(&self.current_rust_behavior),
            escape_json(&self.fixture_or_phpt_example),
            escape_json(&self.planned_solution_layer),
            self.baseline_count
        )
    }
}

impl GeneratedCase {
    fn to_json_line(&self) -> String {
        format!(
            "{{\"path\":\"{}\",\"module\":\"{}\",\"kind\":\"{}\",\"original_path\":\"{}\",\"original_source_hash\":\"{}\",\"generated_timestamp\":\"{}\",\"generator_version\":\"{}\",\"reason\":\"{}\"}}",
            escape_json(&self.manifest_path),
            escape_json(&self.module),
            escape_json(&self.kind),
            escape_json(&self.original_path),
            escape_json(&self.original_source_hash),
            escape_json(&self.generated_timestamp),
            escape_json(&self.generator_version),
            escape_json(&self.reason)
        )
    }
}

impl PhptIndexOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut php_src = None;
        let mut out = None;
        let mut report = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            if let Some(value) = arg.strip_prefix("--php-src=") {
                php_src = Some(PathBuf::from(value));
            } else if arg == "--php-src" {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--php-src requires a path".to_string());
                };
                php_src = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix("--out=") {
                out = Some(PathBuf::from(value));
            } else if arg == "--out" {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--out requires a path".to_string());
                };
                out = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix("--report=") {
                report = Some(PathBuf::from(value));
            } else if arg == "--report" {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--report requires a path".to_string());
                };
                report = Some(PathBuf::from(value));
            } else {
                return Err(format!("unknown option `{arg}`"));
            }
            index += 1;
        }
        let php_src = php_src
            .or_else(|| env::var_os("PHP_SRC_DIR").map(PathBuf::from))
            .unwrap_or_else(default_php_src_dir);
        if !php_src.is_dir() {
            return Err(format!(
                "php-src checkout not found at {}; set PHP_SRC_DIR or --php-src",
                php_src.display()
            ));
        }
        Ok(Self {
            php_src,
            out: out.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_CORPUS)),
            report: report.unwrap_or_else(|| PathBuf::from(DEFAULT_PHPT_REPORT)),
        })
    }
}

impl LookupOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut symbols = None;
        let mut symbol = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            if let Some(value) = arg.strip_prefix("--symbols=") {
                symbols = Some(PathBuf::from(value));
            } else if arg == "--symbols" {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--symbols requires a path".to_string());
                };
                symbols = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix("--symbol=") {
                symbol = Some(value.to_string());
            } else if arg == "--symbol" {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--symbol requires a value".to_string());
                };
                symbol = Some(value.to_string());
            } else if let Some(value) = arg.strip_prefix("SYMBOL=") {
                symbol = Some(value.to_string());
            } else if symbol.is_none() {
                symbol = Some(arg.to_string());
            } else {
                return Err(format!("unknown option `{arg}`"));
            }
            index += 1;
        }
        let Some(symbol) = symbol else {
            return Err("lookup-symbol requires SYMBOL=<name> or --symbol <name>".to_string());
        };
        Ok(Self {
            symbols: symbols.unwrap_or_else(|| PathBuf::from(DEFAULT_SYMBOLS)),
            symbol,
        })
    }
}

impl SourceOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut php_src = None;
        let mut manifest = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            if let Some(value) = arg.strip_prefix("--php-src=") {
                php_src = Some(PathBuf::from(value));
            } else if arg == "--php-src" {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--php-src requires a path".to_string());
                };
                php_src = Some(PathBuf::from(value));
            } else if let Some(value) = arg.strip_prefix("--manifest=") {
                manifest = Some(PathBuf::from(value));
            } else if arg == "--manifest" {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("--manifest requires a path".to_string());
                };
                manifest = Some(PathBuf::from(value));
            } else {
                return Err(format!("unknown option `{arg}`"));
            }
            index += 1;
        }
        let php_src = php_src
            .or_else(|| env::var_os("PHP_SRC_DIR").map(PathBuf::from))
            .unwrap_or_else(default_php_src_dir);
        if !php_src.is_dir() {
            return Err(format!(
                "php-src checkout not found at {}; set PHP_SRC_DIR or --php-src",
                php_src.display()
            ));
        }
        Ok(Self {
            php_src,
            manifest: manifest.unwrap_or_else(|| PathBuf::from(DEFAULT_MANIFEST)),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ManifestEntry {
    path: String,
    size: u64,
    sha256: String,
    kind: FileKind,
}

impl ManifestEntry {
    fn to_json_line(&self) -> String {
        format!(
            "{{\"path\":\"{}\",\"size\":{},\"sha256\":\"{}\",\"kind\":\"{}\"}}",
            escape_json(&self.path),
            self.size,
            self.sha256,
            self.kind.as_str()
        )
    }

    fn from_json_line(line: &str) -> Result<Self, String> {
        Ok(Self {
            path: extract_json_string(line, "path")?,
            size: extract_json_u64(line, "size")?,
            sha256: extract_json_string(line, "sha256")?,
            kind: FileKind::parse(&extract_json_string(line, "kind")?)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FileKind {
    Phpt,
    CSource,
    Header,
    ZendSource,
    RunTests,
    FixtureSupport,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SymbolEntry {
    kind: String,
    php_name: String,
    c_name: String,
    path: String,
    line: u64,
    module: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PhptEntry {
    path: String,
    title: String,
    sections: Vec<String>,
    module: String,
    has_skipif: bool,
    has_clean: bool,
    has_redirecttest: bool,
    has_external_files: bool,
    uses_http_sections: bool,
    uses_stdin_args: bool,
    expectation_kind: String,
    source_hash: String,
}

impl PhptEntry {
    fn to_json_line(&self) -> String {
        format!(
            "{{\"path\":\"{}\",\"title\":\"{}\",\"sections\":{},\"module\":\"{}\",\"has_skipif\":{},\"has_clean\":{},\"has_redirecttest\":{},\"has_external_files\":{},\"uses_http_sections\":{},\"uses_stdin_args\":{},\"expectation_kind\":\"{}\",\"source_hash\":\"{}\"}}",
            escape_json(&self.path),
            escape_json(&self.title),
            json_string_array(&self.sections),
            escape_json(&self.module),
            self.has_skipif,
            self.has_clean,
            self.has_redirecttest,
            self.has_external_files,
            self.uses_http_sections,
            self.uses_stdin_args,
            escape_json(&self.expectation_kind),
            self.source_hash
        )
    }

    fn from_json_line(line: &str) -> Result<Self, String> {
        Ok(Self {
            path: extract_json_string(line, "path")?,
            title: extract_json_string(line, "title")?,
            sections: extract_json_string_array(line, "sections")?,
            module: extract_json_string(line, "module")?,
            has_skipif: extract_json_bool(line, "has_skipif")?,
            has_clean: extract_json_bool(line, "has_clean")?,
            has_redirecttest: extract_json_bool(line, "has_redirecttest")?,
            has_external_files: extract_json_bool(line, "has_external_files")?,
            uses_http_sections: extract_json_bool(line, "uses_http_sections")?,
            uses_stdin_args: extract_json_bool(line, "uses_stdin_args")?,
            expectation_kind: extract_json_string(line, "expectation_kind")?,
            source_hash: extract_json_string(line, "source_hash")?,
        })
    }
}

impl SymbolEntry {
    fn to_json_line(&self) -> String {
        format!(
            "{{\"kind\":\"{}\",\"php_name\":\"{}\",\"c_name\":\"{}\",\"path\":\"{}\",\"line\":{},\"module\":\"{}\"}}",
            escape_json(&self.kind),
            escape_json(&self.php_name),
            escape_json(&self.c_name),
            escape_json(&self.path),
            self.line,
            escape_json(&self.module)
        )
    }

    fn from_json_line(line: &str) -> Result<Self, String> {
        Ok(Self {
            kind: extract_json_string(line, "kind")?,
            php_name: extract_json_string(line, "php_name")?,
            c_name: extract_json_string(line, "c_name")?,
            path: extract_json_string(line, "path")?,
            line: extract_json_u64(line, "line")?,
            module: extract_json_string(line, "module")?,
        })
    }

    fn matches(&self, query: &str) -> bool {
        self.php_name.to_ascii_lowercase() == query
            || self.c_name.to_ascii_lowercase() == query
            || self.path.to_ascii_lowercase().contains(query)
            || self
                .php_name
                .to_ascii_lowercase()
                .contains(&format!("::{query}"))
    }
}

impl FileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Phpt => "phpt",
            Self::CSource => "c_source",
            Self::Header => "header",
            Self::ZendSource => "zend_source",
            Self::RunTests => "run_tests",
            Self::FixtureSupport => "fixture_support",
            Self::Other => "other",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "phpt" => Ok(Self::Phpt),
            "c_source" => Ok(Self::CSource),
            "header" => Ok(Self::Header),
            "zend_source" => Ok(Self::ZendSource),
            "run_tests" => Ok(Self::RunTests),
            "fixture_support" => Ok(Self::FixtureSupport),
            "other" => Ok(Self::Other),
            _ => Err(format!("unknown file kind `{value}`")),
        }
    }
}

fn collect_manifest_entries(php_src: &Path) -> Result<Vec<ManifestEntry>, String> {
    let mut entries = Vec::new();
    collect_recursive(php_src, php_src, &mut entries)?;
    Ok(entries)
}

fn collect_symbol_entries(php_src: &Path) -> Result<Vec<SymbolEntry>, String> {
    let mut source_files = Vec::new();
    collect_symbol_source_files(php_src, php_src, &mut source_files)?;
    source_files.sort();
    let mut entries = Vec::new();
    for rel in source_files {
        let path = php_src.join(&rel);
        if rel.starts_with("Zend/") && is_c_or_header(&rel) {
            entries.push(SymbolEntry {
                kind: "zend_source_file".to_string(),
                php_name: String::new(),
                c_name: source_stem(&rel),
                path: rel.clone(),
                line: 1,
                module: module_guess(&rel),
            });
        }
        scan_symbol_file(&path, &rel, &mut entries)?;
    }
    Ok(entries)
}

fn collect_phpt_entries(php_src: &Path) -> Result<Vec<PhptEntry>, String> {
    let mut files = Vec::new();
    collect_phpt_files(php_src, php_src, &mut files)?;
    files.sort();
    let mut entries = Vec::new();
    for rel in files {
        let path = php_src.join(&rel);
        let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let source = String::from_utf8_lossy(&bytes);
        let document = parse_phpt(&source);
        let sections = document.sections;
        let section_names = sections
            .iter()
            .map(|section| section.name.clone())
            .collect::<Vec<_>>();
        let title = sections
            .iter()
            .find(|section| section.name == "TEST")
            .map(|section| first_non_empty_line(&section.body))
            .unwrap_or_default();
        let (_, source_hash) = hash_file(&path)?;
        entries.push(PhptEntry {
            path: rel.clone(),
            title,
            module: phpt_module_tag(&rel, &sections),
            has_skipif: has_section(&sections, "SKIPIF"),
            has_clean: has_section(&sections, "CLEAN"),
            has_redirecttest: has_section(&sections, "REDIRECTTEST"),
            has_external_files: sections
                .iter()
                .any(|section| section.name.ends_with("_EXTERNAL")),
            uses_http_sections: sections.iter().any(|section| {
                matches!(
                    section.name.as_str(),
                    "GET" | "POST" | "POST_RAW" | "PUT" | "COOKIE" | "EXPECTHEADERS"
                )
            }),
            uses_stdin_args: sections
                .iter()
                .any(|section| matches!(section.name.as_str(), "STDIN" | "ARGS")),
            expectation_kind: expectation_kind(&sections),
            source_hash,
            sections: section_names,
        });
    }
    Ok(entries)
}

fn collect_phpt_files(
    php_src: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> Result<(), String> {
    let mut children = fs::read_dir(current)
        .map_err(|error| format!("{}: {error}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", current.display()))?;
    children.sort_by_key(|entry| entry.path());
    for child in children {
        let path = child.path();
        let file_type = child
            .file_type()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if file_type.is_dir() {
            if should_skip_dir(php_src, &path) {
                continue;
            }
            collect_phpt_files(php_src, &path, files)?;
        } else if file_type.is_file()
            && path.extension().and_then(|ext| ext.to_str()) == Some("phpt")
        {
            files.push(relative_path(php_src, &path)?);
        }
    }
    Ok(())
}

fn resolve_phpt_path(php_src: &Path, manifest_path: &str) -> PathBuf {
    let path = PathBuf::from(manifest_path);
    if path.is_file() {
        path
    } else {
        php_src.join(manifest_path)
    }
}

fn section<'a>(sections: &'a [PhptSection], name: &str) -> Option<&'a PhptSection> {
    sections.iter().find(|section| section.name == name)
}

fn file_body(sections: &[PhptSection], phpt_path: &Path) -> Result<Option<String>, String> {
    if let Some(section) = section(sections, "FILE").or_else(|| section(sections, "FILEEOF")) {
        return Ok(Some(section.body.clone()));
    }
    if let Some(section) = section(sections, "FILE_EXTERNAL") {
        let external = resolve_external_phpt_path(phpt_path, "FILE_EXTERNAL", &section.body)?;
        return fs::read_to_string(&external)
            .map(Some)
            .map_err(|error| format!("{}: {error}", external.display()));
    }
    Ok(None)
}

fn resolve_external_phpt_path(
    phpt_path: &Path,
    section_name: &str,
    body: &str,
) -> Result<PathBuf, String> {
    let raw = first_non_empty_line(body);
    if raw.is_empty() {
        return Err(format!("{section_name} path is empty"));
    }
    let external = Path::new(&raw);
    if external.is_absolute()
        || external.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "{section_name} path must be a relative local path: {raw}"
        ));
    }
    let base = phpt_path.parent().unwrap_or_else(|| Path::new("."));
    let base = fs::canonicalize(base).map_err(|error| format!("{}: {error}", base.display()))?;
    let candidate = base.join(external);
    let resolved = fs::canonicalize(&candidate)
        .map_err(|error| format!("{section_name} {}: {error}", candidate.display()))?;
    if !resolved.starts_with(&base) {
        return Err(format!("{section_name} path escapes PHPT directory: {raw}"));
    }
    Ok(resolved)
}

fn phpt_result_cache_key(
    context: &RunContext,
    manifest_path: &str,
    phpt_source: &str,
    document: &PhptDocument,
    phpt_path: &Path,
    timeout: Duration,
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(b"phpt-run-cache-v1\0");
    hasher.update(manifest_path.as_bytes());
    hasher.update(b"\0target-mode=");
    hasher.update(context.options.target_mode.as_str().as_bytes());
    hasher.update(b"\0timeout=");
    hasher.update(timeout.as_secs().to_string().as_bytes());
    hasher.update(b"\0target=");
    hasher.update(context.target_fingerprint.as_bytes());
    hasher.update(b"\0runner=");
    hasher.update(context.runner_fingerprint.as_bytes());
    hasher.update(b"\0phpt=");
    hasher.update(phpt_source.as_bytes());
    if let Some(file_body) = file_body(&document.sections, phpt_path)? {
        hasher.update(b"\0file-body=");
        hasher.update(file_body.as_bytes());
    }
    hash_phpt_support_files(&mut hasher, phpt_path)?;
    if let Some((kind, expected)) = expectation(&document.sections, phpt_path)? {
        hasher.update(b"\0expectation-kind=");
        hasher.update(format!("{kind:?}").as_bytes());
        hasher.update(b"\0expectation=");
        hasher.update(expected.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn phpt_result_input_cache_key(
    context: &RunContext,
    manifest_path: &str,
    phpt_source: &str,
    document: &PhptDocument,
    phpt_path: &Path,
    timeout: Duration,
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(b"phpt-run-input-cache-v1\0");
    hasher.update(manifest_path.as_bytes());
    hasher.update(b"\0target-mode=");
    hasher.update(context.options.target_mode.as_str().as_bytes());
    hasher.update(b"\0timeout=");
    hasher.update(timeout.as_secs().to_string().as_bytes());
    hasher.update(b"\0phpt=");
    hasher.update(phpt_source.as_bytes());
    if let Some(file_body) = file_body(&document.sections, phpt_path)? {
        hasher.update(b"\0file-body=");
        hasher.update(file_body.as_bytes());
    }
    hash_phpt_support_files(&mut hasher, phpt_path)?;
    if let Some((kind, expected)) = expectation(&document.sections, phpt_path)? {
        hasher.update(b"\0expectation-kind=");
        hasher.update(format!("{kind:?}").as_bytes());
        hasher.update(b"\0expectation=");
        hasher.update(expected.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn expectation(
    sections: &[PhptSection],
    phpt_path: &Path,
) -> Result<Option<(ExpectationKind, String)>, String> {
    for (name, kind) in [
        ("EXPECT", ExpectationKind::Expect),
        ("EXPECTF", ExpectationKind::ExpectF),
        ("EXPECTREGEX", ExpectationKind::ExpectRegex),
    ] {
        if let Some(section) = section(sections, name) {
            return Ok(Some((kind, section.body.clone())));
        }
    }
    for (name, kind) in [
        ("EXPECT_EXTERNAL", ExpectationKind::Expect),
        ("EXPECTF_EXTERNAL", ExpectationKind::ExpectF),
        ("EXPECTREGEX_EXTERNAL", ExpectationKind::ExpectRegex),
    ] {
        if let Some(section) = section(sections, name) {
            let external = resolve_external_phpt_path(phpt_path, name, &section.body)?;
            let expected = fs::read_to_string(&external)
                .map_err(|error| format!("{}: {error}", external.display()))?;
            return Ok(Some((kind, expected)));
        }
    }
    Ok(None)
}

fn copy_phpt_support_files(phpt_path: &Path, work_dir: &Path) -> Result<(), String> {
    let Some(source_dir) = phpt_path.parent() else {
        return Ok(());
    };
    if let Some(file_name) = phpt_path.file_name() {
        let destination = work_dir.join(file_name);
        fs::copy(phpt_path, &destination).map_err(|error| {
            format!(
                "{} -> {}: {error}",
                phpt_path.display(),
                destination.display()
            )
        })?;
    }
    for entry in sorted_dir_entries(source_dir)? {
        let source = entry.path();
        if source == phpt_path || is_phpt_file(&source) {
            continue;
        }
        let destination = work_dir.join(entry.file_name());
        copy_phpt_support_entry(&source, &destination)?;
    }
    Ok(())
}

fn copy_phpt_support_entry(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(source).map_err(|error| format!("{}: {error}", source.display()))?;
    if metadata.is_dir() {
        fs::create_dir_all(destination)
            .map_err(|error| format!("{}: {error}", destination.display()))?;
        for entry in sorted_dir_entries(source)? {
            let child_source = entry.path();
            if is_phpt_file(&child_source) {
                continue;
            }
            copy_phpt_support_entry(&child_source, &destination.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        fs::copy(source, destination).map_err(|error| {
            format!("{} -> {}: {error}", source.display(), destination.display())
        })?;
    }
    Ok(())
}

fn hash_phpt_support_files(hasher: &mut Sha256, phpt_path: &Path) -> Result<(), String> {
    let Some(source_dir) = phpt_path.parent() else {
        return Ok(());
    };
    hasher.update(b"\0support-files=");
    hash_phpt_support_dir(hasher, source_dir, source_dir, phpt_path)
}

fn hash_phpt_support_dir(
    hasher: &mut Sha256,
    root: &Path,
    current: &Path,
    phpt_path: &Path,
) -> Result<(), String> {
    for entry in sorted_dir_entries(current)? {
        let path = entry.path();
        if path == phpt_path || is_phpt_file(&path) {
            continue;
        }
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if metadata.is_dir() {
            hasher.update(b"dir:");
            hasher.update(relative.to_string_lossy().as_bytes());
            hasher.update(b"\0");
            hash_phpt_support_dir(hasher, root, &path, phpt_path)?;
        } else if metadata.is_file() {
            hasher.update(b"file:");
            hasher.update(relative.to_string_lossy().as_bytes());
            hasher.update(b"\0");
            let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            hasher.update(bytes);
            hasher.update(b"\0");
        }
    }
    Ok(())
}

fn sorted_dir_entries(dir: &Path) -> Result<Vec<fs::DirEntry>, String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    Ok(entries)
}

fn is_phpt_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("phpt"))
}

fn ini_args(sections: &[PhptSection]) -> Vec<(String, String)> {
    let Some(section) = section(sections, "INI") else {
        return Vec::new();
    };
    section
        .body
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect()
}

fn env_args(sections: &[PhptSection]) -> Vec<(String, String)> {
    let mut env = Vec::new();
    for section_name in ["ENV", "GET", "POST", "POST_RAW", "PUT", "COOKIE"] {
        if let Some(section) = section(sections, section_name) {
            match section_name {
                "GET" => env.push(("QUERY_STRING".to_string(), section.body.trim().to_string())),
                "POST" | "POST_RAW" | "PUT" => {
                    env.push((
                        "REQUEST_METHOD".to_string(),
                        section_name.replace("_RAW", ""),
                    ));
                    let body = if section_name == "POST" {
                        section.body.trim_end_matches(['\r', '\n']).to_string()
                    } else {
                        section.body.clone()
                    };
                    env.push(("PHPT_REQUEST_BODY".to_string(), body));
                }
                "COOKIE" => env.push(("HTTP_COOKIE".to_string(), section.body.trim().to_string())),
                "ENV" => {
                    for line in section.body.lines() {
                        if let Some((name, value)) = line.split_once('=') {
                            env.push((name.trim().to_string(), value.trim().to_string()));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    env
}

#[derive(Debug)]
struct PhptExecutionContext<'a> {
    ini: Vec<(String, String)>,
    env: Vec<(String, String)>,
    args: Vec<String>,
    stdin: Option<&'a str>,
}

fn context_from_sections(sections: &[PhptSection]) -> PhptExecutionContext<'_> {
    let capture_stdio = capture_stdio(sections);
    PhptExecutionContext {
        ini: ini_args(sections),
        env: env_args(sections),
        args: section(sections, "ARGS")
            .map(|section| split_phpt_args(&section.body))
            .unwrap_or_default(),
        stdin: stdin_from_sections(sections, capture_stdio),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CaptureStdio {
    stdin: bool,
    stdout: bool,
    stderr: bool,
}

impl CaptureStdio {
    const ALL: Self = Self {
        stdin: true,
        stdout: true,
        stderr: true,
    };
}

fn capture_stdio(sections: &[PhptSection]) -> CaptureStdio {
    let Some(section) = section(sections, "CAPTURE_STDIO") else {
        return CaptureStdio::ALL;
    };
    let tokens = section
        .body
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>();
    CaptureStdio {
        stdin: tokens.iter().any(|token| token == "STDIN"),
        stdout: tokens.iter().any(|token| token == "STDOUT"),
        stderr: tokens.iter().any(|token| token == "STDERR"),
    }
}

fn stdin_from_sections(sections: &[PhptSection], capture_stdio: CaptureStdio) -> Option<&str> {
    section(sections, "STDIN")
        .map(|section| section.body.as_str())
        .or_else(|| capture_stdio.stdin.then_some(""))
}

fn captured_output(output: &ProcessOutput, capture_stdio: CaptureStdio) -> String {
    match (capture_stdio.stdout, capture_stdio.stderr) {
        (true, true) => {
            let mut combined = String::with_capacity(output.stdout.len() + output.stderr.len());
            combined.push_str(&output.stdout);
            combined.push_str(&output.stderr);
            combined
        }
        (true, false) => output.stdout.clone(),
        (false, true) => output.stderr.clone(),
        (false, false) => String::new(),
    }
}

fn skipif_env_args(sections: &[PhptSection]) -> Vec<(String, String)> {
    skipif_env_args_for_stdio(sections, host_stdio_is_fully_terminal())
}

fn skipif_env_args_for_stdio(
    sections: &[PhptSection],
    stdio_is_fully_terminal: bool,
) -> Vec<(String, String)> {
    if capture_stdio_needs_io_capture_skip(sections, stdio_is_fully_terminal) {
        vec![("SKIP_IO_CAPTURE_TESTS".to_string(), "1".to_string())]
    } else {
        Vec::new()
    }
}

fn capture_stdio_needs_io_capture_skip(
    sections: &[PhptSection],
    stdio_is_fully_terminal: bool,
) -> bool {
    section(sections, "CAPTURE_STDIO").is_some() && !stdio_is_fully_terminal
}

fn host_stdio_is_fully_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal()
}

fn target_cli_skip_reason(
    manifest_path: &str,
    target_mode: TargetMode,
    sections: &[PhptSection],
    source: &str,
) -> Option<&'static str> {
    if target_mode == TargetMode::PhpCli {
        if manifest_path.starts_with("sapi/phpdbg/") {
            return Some("phpdbg not available in php-cli target mode");
        }
        if manifest_path.starts_with("sapi/fpm/") {
            return Some("FPM not available in php-cli target mode");
        }
        if manifest_path.starts_with("sapi/cgi/") {
            return Some("CGI not available in php-cli target mode");
        }
        if manifest_path.starts_with("sapi/apache") {
            return Some("Apache module not available in php-cli target mode");
        }
        if source.contains("php_cli_server_start")
            || source.contains("PHP_CLI_SERVER_ADDRESS")
            || source.contains(" -S ")
        {
            return Some("CLI built-in web server not available in php-cli target mode");
        }
        if source.contains("cli_set_process_title")
            || source.contains("cli_get_process_title")
            || source.contains("sapi_windows_set_ctrl_handler")
            || manifest_path.contains("cli_process_title")
        {
            return Some("CLI process-control APIs not available in php-cli target mode");
        }
        if source.contains("fclose(STDIN)")
            || source.contains("fclose(STDOUT)")
            || source.contains("fclose(STDERR)")
            || source.contains("php://fd/")
        {
            return Some("CLI stdio descriptor rebinding not available in php-cli target mode");
        }
        if source.contains(" -R ") {
            return Some("CLI -R line-processing mode not available in php-cli target mode");
        }
        if source.contains(" --ini") {
            return Some("CLI --ini introspection not available in php-cli target mode");
        }
        if manifest_path == "ext/standard/tests/hrtime/hrtime.phpt" {
            return Some("flaky hrtime busy-loop exceeds target VM step limit");
        }
        if php_source_contains_function_call(source, "passthru")
            || source.contains("proc_open(")
            || source.contains("shell_exec(")
        {
            return Some("process-control functions are outside the php-cli target contract");
        }
        if section(sections, "SKIPIF").is_some_and(|skipif| {
            ["exec", "system", "passthru", "proc_open", "shell_exec"]
                .iter()
                .any(|name| php_source_contains_function_call(&skipif.body, name))
        }) {
            return Some("SKIPIF process-control probe is outside the php-cli target contract");
        }
        if manifest_path == "sapi/cli/tests/bug77561.phpt" {
            return Some("include-path expression runtime gap outside the php-cli target contract");
        }
        if manifest_path == "sapi/cli/tests/bug70006.phpt" {
            return Some(
                "STDOUT default-parameter lowering is outside the php-cli target contract",
            );
        }
    }
    if section(sections, "PHPDBG").is_some() {
        Some("phpdbg not available")
    } else if section(sections, "CGI").is_some()
        || section(sections, "GZIP_POST").is_some()
        || section(sections, "DEFLATE_POST").is_some()
    {
        Some("CGI not available")
    } else {
        None
    }
}

fn php_source_contains_function_call(source: &str, name: &str) -> bool {
    let source_bytes = source.as_bytes();
    let name_len = name.len();
    let mut offset = 0;
    while let Some(index) = source[offset..].find(name) {
        let start = offset + index;
        let before = start.checked_sub(1).and_then(|idx| source_bytes.get(idx));
        if before.is_some_and(|byte| is_php_identifier_byte(*byte) || matches!(*byte, b'$' | b'>'))
        {
            offset = start + name_len;
            continue;
        }
        let mut cursor = start + name_len;
        while source_bytes
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            cursor += 1;
        }
        if source_bytes.get(cursor) == Some(&b'(') {
            return true;
        }
        offset = start + name_len;
    }
    false
}

fn is_php_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

fn required_extensions(sections: &[PhptSection]) -> Vec<String> {
    let Some(section) = section(sections, "EXTENSIONS") else {
        return Vec::new();
    };
    section
        .body
        .lines()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with(';'))
        .map(str::to_string)
        .collect()
}

fn required_extensions_skip_reason(
    options: &RunOptions,
    sections: &[PhptSection],
    work_dir: &Path,
) -> Result<Option<String>, String> {
    let required = required_extensions(sections);
    if required.is_empty() {
        return Ok(None);
    }

    let check_path = work_dir.join("required_extensions.php");
    fs::write(&check_path, extension_check_source(&required))
        .map_err(|error| format!("{}: {error}", check_path.display()))?;
    let output = run_php(options, &check_path, work_dir, &[], &[], &[], None)?;
    let missing = output
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix("missing:"))
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!(
            "required extension(s) not loaded: {}",
            missing.join(", ")
        )))
    }
}

fn extension_check_source(required: &[String]) -> String {
    let extensions = required
        .iter()
        .map(|extension| format!("'{}'", php_single_quoted_literal(extension)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "<?php\n$missing = [];\nforeach ([{extensions}] as $extension) {{\n    if (!extension_loaded($extension)) {{\n        $missing[] = $extension;\n    }}\n}}\nif ($missing) {{\n    echo 'missing:', implode(',', $missing), \"\\n\";\n}}\n"
    )
}

fn php_single_quoted_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn phpt_execution_filename(phpt_path: &Path) -> String {
    let stem = phpt_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("test");
    format!("{stem}.php")
}

fn split_phpt_args(args: &str) -> Vec<String> {
    args.split_whitespace().map(str::to_string).collect()
}

fn run_clean_if_present(
    options: &RunOptions,
    sections: &[PhptSection],
    work_dir: &Path,
) -> Result<(), String> {
    let Some(clean) = section(sections, "CLEAN") else {
        return Ok(());
    };
    let clean_path = work_dir.join("clean.php");
    fs::write(&clean_path, &clean.body)
        .map_err(|error| format!("{}: {error}", clean_path.display()))?;
    let _ = run_php(options, &clean_path, work_dir, &[], &[], &[], None)?;
    Ok(())
}

#[derive(Debug)]
struct ProcessOutput {
    status: i32,
    stdout: String,
    stderr: String,
}

fn run_php(
    options: &RunOptions,
    script: &Path,
    cwd: &Path,
    ini: &[(String, String)],
    envs: &[(String, String)],
    script_args: &[String],
    stdin: Option<&str>,
) -> Result<ProcessOutput, String> {
    let target = fs::canonicalize(&options.target)
        .map_err(|error| format!("{}: {error}", options.target.display()))?;
    let script =
        fs::canonicalize(script).map_err(|error| format!("{}: {error}", script.display()))?;
    let mut command = Command::new(&target);
    command.current_dir(cwd);
    command.env("TEST_PHP_EXECUTABLE", &target);
    command.env("TEST_PHP_CLI_EXECUTABLE", &target);
    command.env(
        "TEST_PHP_EXECUTABLE_ESCAPED",
        shell_escape(&target.to_string_lossy()),
    );
    let ini = php_run_tests_ini_args(ini);
    match options.target_mode {
        TargetMode::PhpCli => {
            command.arg("-n");
            for (name, value) in &ini {
                command.arg("-d").arg(format!("{name}={value}"));
            }
            command.arg(script);
            command.args(script_args);
        }
        TargetMode::PhpVm => {
            command.arg("run");
            for (name, value) in envs {
                command.arg("--env").arg(format!("{name}={value}"));
            }
            for (name, value) in &ini {
                command
                    .arg("--env")
                    .arg(format!("PHPT_INI_{}={value}", sanitize_env_name(name)));
            }
            command.arg(script);
            if !script_args.is_empty() {
                command.arg("--");
                command.args(script_args);
            }
        }
    }
    if options.target_mode == TargetMode::PhpCli {
        for (name, value) in envs {
            command.env(name, value);
        }
    }
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("{}: {error}", target.display()))?;
    if let Some(stdin) = stdin
        && let Some(mut child_stdin) = child.stdin.take()
    {
        child_stdin
            .write_all(stdin.as_bytes())
            .map_err(|error| format!("stdin: {error}"))?;
    }
    let start = Instant::now();
    let output = loop {
        if child
            .try_wait()
            .map_err(|error| format!("{}: {error}", target.display()))?
            .is_some()
        {
            break child
                .wait_with_output()
                .map_err(|error| format!("{}: {error}", target.display()))?;
        }
        if start.elapsed() > options.timeout {
            let _ = child.kill();
            let output = child
                .wait_with_output()
                .map_err(|error| format!("{}: {error}", target.display()))?;
            return Ok(ProcessOutput {
                status: 124,
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: format!(
                    "PHPT_TIMEOUT after {}s\n{}",
                    options.timeout.as_secs(),
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(ProcessOutput {
        status: output.status.code().unwrap_or(255),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn php_run_tests_ini_args(test_ini: &[(String, String)]) -> Vec<(String, String)> {
    let mut ini = PHP_RUN_TESTS_INI_DEFAULTS
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect::<Vec<_>>();
    ini.extend(test_ini.iter().cloned());
    ini
}

fn normalize_expected_output(value: &str) -> String {
    normalize_output(value, false)
}

fn normalize_actual_output(value: &str) -> String {
    normalize_output(value, true)
}

fn normalize_output(value: &str, strip_php_cli_diagnostic_prefix: bool) -> String {
    let mut normalized = value.replace("\r\n", "\n");
    if strip_php_cli_diagnostic_prefix
        && normalized.starts_with('\n')
        && starts_with_php_cli_diagnostic(&normalized[1..])
    {
        normalized.remove(0);
    }
    php_run_tests_trim(&normalized).to_string()
}

fn php_run_tests_trim(value: &str) -> &str {
    value.trim_matches(|ch| matches!(ch, '\0' | ' ' | '\n' | '\r' | '\t' | '\u{000B}'))
}

fn starts_with_php_cli_diagnostic(value: &str) -> bool {
    [
        "Deprecated:",
        "Fatal error:",
        "Notice:",
        "Parse error:",
        "Recoverable fatal error:",
        "Strict Standards:",
        "Warning:",
    ]
    .iter()
    .any(|prefix| value.starts_with(prefix))
}

fn read_phpt_corpus(path: &Path) -> Result<Vec<PhptEntry>, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut entries = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        entries.push(
            PhptEntry::from_json_line(line)
                .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?,
        );
    }
    Ok(entries)
}

fn build_generated_case(
    options: &GenerateOptions,
    reference_options: &RunOptions,
    entry: &PhptEntry,
    kind: &str,
    reason: &str,
    reduction: Option<ReductionMode>,
    index: usize,
) -> Result<Option<GeneratedCase>, String> {
    let phpt_path = options.php_src.join(&entry.path);
    let source = fs::read_to_string(&phpt_path)
        .map_err(|error| format!("{}: {error}", phpt_path.display()))?;
    let document = parse_phpt(&source);
    let Some(mut body) = file_body(&document.sections, &phpt_path)? else {
        return Ok(None);
    };
    let base = run_reference_body(
        reference_options,
        &document.sections,
        &body,
        &options.work_dir.join(format!("candidate-{index}")),
    )?;
    if base.status != 0 {
        return Ok(None);
    }
    if matches!(reduction, Some(ReductionMode::LineRemoval)) {
        body = reduce_body_by_reference_equivalence(
            reference_options,
            &document.sections,
            &body,
            &base,
            &options.work_dir.join(format!("reduce-{index}")),
        )?;
    }
    let final_output = run_reference_body(
        reference_options,
        &document.sections,
        &body,
        &options.work_dir.join(format!("final-{index}")),
    )?;
    if final_output.status != 0 {
        return Ok(None);
    }

    let stem = entry
        .path
        .rsplit('/')
        .next()
        .unwrap_or("generated.phpt")
        .trim_end_matches(".phpt");
    let file_name = format!(
        "{}-{}-{}.phpt",
        kind,
        safe_path_component(stem),
        &entry.source_hash[..12.min(entry.source_hash.len())]
    );
    let path = options.generated_dir.join(file_name);
    let manifest_path = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let generated_source = render_generated_phpt(
        options,
        entry,
        kind,
        reason,
        &body,
        &final_output.stdout,
        &document.sections,
    );
    Ok(Some(GeneratedCase {
        path,
        manifest_path,
        module: options.module.clone(),
        kind: kind.to_string(),
        original_path: entry.path.clone(),
        original_source_hash: entry.source_hash.clone(),
        generated_timestamp: options.timestamp.clone(),
        generator_version: GENERATOR_VERSION.to_string(),
        reason: reason.to_string(),
        source: generated_source,
    }))
}

fn write_generated_case(case: &GeneratedCase) -> Result<(), String> {
    if let Some(parent) = case.path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(&case.path, &case.source).map_err(|error| format!("{}: {error}", case.path.display()))
}

fn clear_generated_phpts(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", dir.display()))?
    {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("phpt") {
            fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    Ok(())
}

fn run_reference_body(
    options: &RunOptions,
    sections: &[PhptSection],
    body: &str,
    work_dir: &Path,
) -> Result<ProcessOutput, String> {
    let _ = fs::remove_dir_all(work_dir);
    fs::create_dir_all(work_dir).map_err(|error| format!("{}: {error}", work_dir.display()))?;
    let script = work_dir.join("test.php");
    fs::write(&script, body).map_err(|error| format!("{}: {error}", script.display()))?;
    let context = context_from_sections(sections);
    run_php(
        options,
        &script,
        work_dir,
        &context.ini,
        &context.env,
        &context.args,
        context.stdin,
    )
}

fn reduce_body_by_reference_equivalence(
    options: &RunOptions,
    sections: &[PhptSection],
    body: &str,
    expected: &ProcessOutput,
    work_dir: &Path,
) -> Result<String, String> {
    let mut lines = body
        .split_inclusive('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.len() > 80 {
        return Ok(body.to_string());
    }
    let mut index = 0usize;
    let mut attempts = 0usize;
    while index < lines.len() && attempts < 200 {
        let line = &lines[index];
        if line.trim_start().starts_with("<?php") {
            index += 1;
            continue;
        }
        let mut candidate = lines.clone();
        candidate.remove(index);
        let candidate_body = candidate.concat();
        attempts += 1;
        let output = run_reference_body(
            options,
            sections,
            &candidate_body,
            &work_dir.join(format!("attempt-{attempts}")),
        )?;
        if output.status == expected.status
            && output.stdout == expected.stdout
            && output.stderr == expected.stderr
        {
            lines = candidate;
        } else {
            index += 1;
        }
    }
    Ok(lines.concat())
}

fn render_generated_phpt(
    options: &GenerateOptions,
    entry: &PhptEntry,
    kind: &str,
    reason: &str,
    body: &str,
    expected_stdout: &str,
    sections: &[PhptSection],
) -> String {
    let mut out = String::new();
    out.push_str("--TEST--\n");
    out.push_str(&format!(
        "PHPT generated {kind}: {}\n",
        first_non_empty_line(&entry.title)
    ));
    out.push_str("--DESCRIPTION--\n");
    out.push_str(&format!("original php-src path: {}\n", entry.path));
    out.push_str(&format!("original source hash: {}\n", entry.source_hash));
    out.push_str(&format!("generated timestamp: {}\n", options.timestamp));
    out.push_str(&format!("generator version: {GENERATOR_VERSION}\n"));
    out.push_str(&format!("reason: {reason}\n"));
    if let Some(ini) = section(sections, "INI") {
        out.push_str("--INI--\n");
        out.push_str(&ini.body);
        ensure_trailing_newline(&mut out);
    }
    if let Some(env) = section(sections, "ENV") {
        out.push_str("--ENV--\n");
        out.push_str(&env.body);
        ensure_trailing_newline(&mut out);
    }
    if let Some(args) = section(sections, "ARGS") {
        out.push_str("--ARGS--\n");
        out.push_str(&args.body);
        ensure_trailing_newline(&mut out);
    }
    if let Some(stdin) = section(sections, "STDIN") {
        out.push_str("--STDIN--\n");
        out.push_str(&stdin.body);
        ensure_trailing_newline(&mut out);
    }
    if let Some(capture_stdio) = section(sections, "CAPTURE_STDIO") {
        out.push_str("--CAPTURE_STDIO--\n");
        out.push_str(&capture_stdio.body);
        ensure_trailing_newline(&mut out);
    }
    out.push_str("--FILE--\n");
    out.push_str(body);
    ensure_trailing_newline(&mut out);
    out.push_str("--EXPECT--\n");
    out.push_str(expected_stdout);
    ensure_trailing_newline(&mut out);
    out
}

fn ensure_trailing_newline(value: &mut String) {
    if !value.ends_with('\n') {
        value.push('\n');
    }
}

fn matches_module_selector(entry: &PhptEntry, selector: &str) -> bool {
    if entry.module == selector {
        return true;
    }
    match selector {
        "zend.basic" => {
            entry.path.starts_with("Zend/tests/")
                && entry.path["Zend/tests/".len()..].matches('/').count() == 0
        }
        _ if selector.starts_with("zend.") => {
            let subdir = selector
                .trim_start_matches("zend.")
                .replace('.', "/")
                .replace('_', "-");
            entry.path.starts_with(&format!("Zend/tests/{subdir}/"))
        }
        _ if selector.starts_with("ext.") => {
            let extension = selector.trim_start_matches("ext.");
            entry.path.starts_with(&format!("ext/{extension}/"))
        }
        _ => false,
    }
}

fn is_simple_generation_candidate(entry: &PhptEntry) -> bool {
    !entry.has_skipif
        && !entry.has_clean
        && !entry.has_redirecttest
        && !entry.has_external_files
        && !entry.uses_http_sections
        && !entry.uses_stdin_args
        && entry.expectation_kind == "expect"
        && entry
            .sections
            .iter()
            .any(|section| section == "FILE" || section == "FILEEOF")
}

fn source_len(path: &Path) -> u64 {
    path.metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(u64::MAX)
}

fn safe_path_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('-').to_string()
}

fn read_run_results(path: &Path) -> Result<Vec<PhptRunResult>, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut results = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        results.push(
            PhptRunResult::from_json_line(line)
                .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?,
        );
    }
    Ok(results)
}

fn read_phpt_entries(path: &Path) -> Result<Vec<PhptEntry>, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut entries = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        entries.push(
            PhptEntry::from_json_line(line)
                .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?,
        );
    }
    Ok(entries)
}

fn read_known_failures(path: &Path) -> Result<Vec<KnownFailure>, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut failures = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        failures.push(
            KnownFailure::from_json_line(line)
                .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?,
        );
    }
    Ok(failures)
}

fn read_baseline_module_counts(path: &Path) -> Result<Vec<BaselineModuleCount>, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut counts = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        counts.push(
            BaselineModuleCount::from_json_line(line)
                .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?,
        );
    }
    Ok(counts)
}

fn read_known_gap_catalog(path: &Path) -> Result<Vec<KnownGapCatalogEntry>, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut entries = Vec::new();
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        entries.push(
            KnownGapCatalogEntry::from_json_line(line)
                .map_err(|error| format!("{}:{}: {error}", path.display(), index + 1))?,
        );
    }
    Ok(entries)
}

fn read_baseline_metadata(path: &Path) -> Result<BaselineMetadata, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    BaselineMetadata::from_json(&source).map_err(|error| format!("{}: {error}", path.display()))
}

#[derive(Debug)]
struct BaselineReportTotals {
    timestamp: String,
    outcomes: BTreeMap<String, usize>,
}

fn read_baseline_report_totals(path: &Path) -> Result<BaselineReportTotals, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let timestamp = source
        .lines()
        .find_map(|line| {
            line.strip_prefix("Generated: `")
                .and_then(|rest| rest.strip_suffix('`'))
                .map(str::to_string)
        })
        .ok_or_else(|| format!("{}: missing Generated timestamp", path.display()))?;
    let mut outcomes = BTreeMap::new();
    let mut in_totals = false;
    for line in source.lines() {
        if line == "## Totals" {
            in_totals = true;
            continue;
        }
        if in_totals && line.starts_with("## ") {
            break;
        }
        if !in_totals || !line.starts_with('|') {
            continue;
        }
        let cells = line
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        if cells.len() != 2 || cells[0] == "Outcome" || cells[0].starts_with("---") {
            continue;
        }
        let count = cells[1].parse::<usize>().map_err(|error| {
            format!(
                "{}: invalid outcome count `{}` for `{}`: {error}",
                path.display(),
                cells[1],
                cells[0]
            )
        })?;
        outcomes.insert(cells[0].to_string(), count);
    }
    if outcomes.is_empty() {
        return Err(format!("{}: missing Totals outcome table", path.display()));
    }
    Ok(BaselineReportTotals {
        timestamp,
        outcomes,
    })
}

fn count_known_failure_outcomes(failures: &[KnownFailure]) -> BTreeMap<String, usize> {
    let mut outcomes = BTreeMap::new();
    for failure in failures {
        *outcomes.entry(failure.outcome.clone()).or_default() += 1;
    }
    outcomes
}

fn build_triage(
    corpus: &[PhptEntry],
    failures: &[KnownFailure],
    results: &[PhptRunResult],
) -> PhptTriage {
    let mut triage = PhptTriage {
        has_result_counts: !results.is_empty(),
        count_source: if results.is_empty() {
            "known-failures".to_string()
        } else {
            "results".to_string()
        },
        ..PhptTriage::default()
    };
    let corpus_by_path = corpus
        .iter()
        .map(|entry| (entry.path.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let result_by_path = results
        .iter()
        .map(|result| (result.path.clone(), result))
        .collect::<BTreeMap<_, _>>();

    for entry in corpus {
        let module = plan_module_for_entry(entry, None);
        let stats = triage.modules.entry(module.to_string()).or_default();
        stats.corpus_count += 1;
        remember_relevant_path(stats, &entry.path);
        remember_selected_path(stats, module, entry);

        let raw_stats = triage.raw_modules.entry(entry.module.clone()).or_default();
        raw_stats.corpus_count += 1;
        remember_relevant_path(raw_stats, &entry.path);
        remember_selected_path(raw_stats, &entry.module, entry);
    }

    for result in results {
        let entry = corpus_by_path.get(&result.path).copied();
        let module = entry
            .map(|entry| plan_module_for_entry(entry, Some(result)))
            .unwrap_or_else(|| plan_module_for_path(&result.path, "unknown", Some(result)));
        add_outcome(&mut triage.modules, module, &result.outcome);
        let raw_module = entry
            .map(|entry| entry.module.as_str())
            .unwrap_or("unknown");
        add_outcome(&mut triage.raw_modules, raw_module, &result.outcome);
    }

    for failure in failures {
        let result = result_by_path.get(&failure.path).copied();
        let corpus_entry = corpus_by_path.get(&failure.path).copied();
        let module = if result.is_none() && failure.outcome == "BORK" {
            "phpt.runner"
        } else {
            corpus_entry
                .map(|entry| plan_module_for_entry(entry, result))
                .unwrap_or_else(|| plan_module_for_path(&failure.path, &failure.module_tag, result))
        };
        let stats = triage.modules.entry(module.to_string()).or_default();
        stats.known_failure_count += 1;
        remember_priority_path(stats, &failure.path);
        remember_priority_selected_path(stats, module, corpus_entry, &failure.path);
        *stats
            .failure_clusters
            .entry(failure.primary_missing_feature_guess.clone())
            .or_default() += 1;
        if failure.outcome == "BORK" {
            *stats
                .bork_subclasses
                .entry(classify_bork(result.map(|result| result.detail.as_str())))
                .or_default() += 1;
        }

        let raw_stats = triage
            .raw_modules
            .entry(failure.owner_module.clone())
            .or_default();
        raw_stats.known_failure_count += 1;
        remember_priority_path(raw_stats, &failure.path);
        remember_priority_selected_path(
            raw_stats,
            &failure.owner_module,
            corpus_entry,
            &failure.path,
        );
        *raw_stats
            .failure_clusters
            .entry(failure.primary_missing_feature_guess.clone())
            .or_default() += 1;

        *triage
            .failure_clusters
            .entry(failure.primary_missing_feature_guess.clone())
            .or_default() += 1;
        if failure
            .primary_missing_feature_guess
            .contains("unsupported")
        {
            *triage
                .unsupported_guesses
                .entry(failure.primary_missing_feature_guess.clone())
                .or_default() += 1;
        }
        if failure.outcome == "BORK" {
            *triage
                .bork_subclasses
                .entry(classify_bork(result.map(|result| result.detail.as_str())))
                .or_default() += 1;
        }

        if !triage.has_result_counts {
            add_outcome(&mut triage.modules, module, &failure.outcome);
            add_outcome(
                &mut triage.raw_modules,
                &failure.owner_module,
                &failure.outcome,
            );
        }
    }

    triage
}

fn apply_baseline_module_counts(triage: &mut PhptTriage, counts: &[BaselineModuleCount]) {
    if counts.iter().any(|count| count.kind == "bork_subclass") {
        triage.bork_subclasses.clear();
    }
    for count in counts {
        if count.kind == "bork_subclass" {
            triage
                .bork_subclasses
                .insert(count.module.clone(), count.known_failure_count);
            continue;
        }
        let target = match count.kind.as_str() {
            "plan" => &mut triage.modules,
            "raw" => &mut triage.raw_modules,
            _ => continue,
        };
        let stats = target.entry(count.module.clone()).or_default();
        stats.corpus_count = count.corpus_count;
        stats.pass_count = count.pass_count;
        stats.skip_count = count.skip_count;
        stats.fail_count = count.fail_count;
        stats.bork_count = count.bork_count;
        stats.known_failure_count = count.known_failure_count;
    }
    if !counts.is_empty() {
        triage.has_result_counts = true;
        triage.count_source = "baseline-module-counts".to_string();
    }
}

fn remember_relevant_path(stats: &mut ModuleTriageStats, path: &str) {
    if stats.relevant_paths.iter().any(|known| known == path) {
        return;
    }
    if stats.relevant_paths.len() < 500 {
        stats.relevant_paths.push(path.to_string());
    }
}

fn remember_selected_path(stats: &mut ModuleTriageStats, module: &str, entry: &PhptEntry) {
    if !is_module_gate_candidate_for_module(module, entry)
        || stats
            .selected_paths
            .iter()
            .any(|known| known == &entry.path)
    {
        return;
    }
    if stats.selected_paths.len() < 500 {
        stats.selected_paths.push(entry.path.clone());
    }
}

fn remember_priority_path(stats: &mut ModuleTriageStats, path: &str) {
    if let Some(index) = stats.relevant_paths.iter().position(|known| known == path) {
        let path = stats.relevant_paths.remove(index);
        stats.relevant_paths.insert(0, path);
        return;
    }
    stats.relevant_paths.insert(0, path.to_string());
    if stats.relevant_paths.len() > 500 {
        stats.relevant_paths.pop();
    }
}

fn remember_priority_selected_path(
    stats: &mut ModuleTriageStats,
    module: &str,
    entry: Option<&PhptEntry>,
    path: &str,
) {
    let Some(entry) = entry else {
        return;
    };
    if !is_module_gate_candidate_for_module(module, entry) {
        return;
    }
    if let Some(index) = stats.selected_paths.iter().position(|known| known == path) {
        let path = stats.selected_paths.remove(index);
        stats.selected_paths.insert(0, path);
        return;
    }
    stats.selected_paths.insert(0, path.to_string());
    if stats.selected_paths.len() > 500 {
        stats.selected_paths.pop();
    }
}

fn is_module_gate_candidate(entry: &PhptEntry) -> bool {
    !entry.uses_http_sections
}

fn is_module_gate_candidate_for_module(module: &str, entry: &PhptEntry) -> bool {
    if !is_module_gate_candidate(entry) {
        return false;
    }
    if module != "zend.functions" {
        return true;
    }
    if entry
        .sections
        .iter()
        .any(|section| section.eq_ignore_ascii_case("EXTENSIONS"))
    {
        return false;
    }
    is_zend_functions_core_gate_path(&entry.path)
}

fn is_zend_functions_core_gate_path(path: &str) -> bool {
    if is_zend_functions_nonportable_gate_path(path) {
        return false;
    }
    [
        "Zend/tests/arrow_functions/",
        "Zend/tests/call_user_functions/",
        "Zend/tests/closures/",
        "Zend/tests/first_class_callable/",
        "Zend/tests/function_arguments/",
        "Zend/tests/type_declarations/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}

fn is_zend_functions_nonportable_gate_path(path: &str) -> bool {
    path.contains("/sensitive_parameter")
        || path.ends_with("/function_arguments_001.phpt")
        || path.ends_with("/function_arguments_002.phpt")
        || path.ends_with("/closure_005.phpt")
        || path.ends_with("/closure_018.phpt")
        || path.ends_with("/closure_019.phpt")
        || path.ends_with("/closure_022.phpt")
        || path.ends_with("/closure_033.phpt")
        || path.ends_with("/closure_065.phpt")
}

fn add_outcome(modules: &mut BTreeMap<String, ModuleTriageStats>, module: &str, outcome: &str) {
    let stats = modules.entry(module.to_string()).or_default();
    match outcome {
        "PASS" => stats.pass_count += 1,
        "SKIP" => stats.skip_count += 1,
        "FAIL" => stats.fail_count += 1,
        "BORK" => stats.bork_count += 1,
        _ => {}
    }
}

fn classify_bork(detail: Option<&str>) -> String {
    let Some(detail) = detail else {
        return "unknown-bork".to_string();
    };
    let lower = detail.to_ascii_lowercase();
    if lower.contains("unsupported phpt section `phpdbg`")
        || lower.contains("unsupported phpt section `cgi`")
    {
        "missing-target-cli-capability".to_string()
    } else if lower.contains("unsupported section") || lower.contains("unsupported phpt section") {
        "unsupported-section".to_string()
    } else if lower.contains("file_external") {
        "unsupported-file-external".to_string()
    } else if lower.contains("expect") {
        "unsupported-expectation".to_string()
    } else if lower.contains("stdin")
        || lower.contains("args")
        || lower.contains("env")
        || lower.contains("ini")
        || lower.contains("clean")
    {
        "unsupported-runner-io".to_string()
    } else if lower.contains("stream did not contain valid utf-8") {
        "malformed-or-non-utf8-phpt".to_string()
    } else if lower.contains("malformed") || lower.contains("missing") {
        "malformed-or-incomplete-phpt".to_string()
    } else if lower.contains("extension") {
        "extension-policy".to_string()
    } else {
        "other-bork".to_string()
    }
}

fn plan_module_for_entry(entry: &PhptEntry, result: Option<&PhptRunResult>) -> &'static str {
    plan_module_for_path(&entry.path, &entry.module, result)
}

fn plan_module_for_path(
    path: &str,
    corpus_module: &str,
    result: Option<&PhptRunResult>,
) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if result
        .map(|result| result.outcome == "BORK")
        .unwrap_or(false)
    {
        return "phpt.runner";
    }
    if lower.starts_with("sapi/") || lower.contains("argv") || lower.contains("stdin") {
        return "phpt.cli";
    }
    if lower.contains("arrayaccess")
        || lower.contains("reference")
        || lower.contains("foreach")
        || lower.contains("cow")
    {
        return "arrays.references";
    }
    if lower.contains("callable")
        || lower.contains("closure")
        || lower.contains("function")
        || lower.contains("variadic")
    {
        return "zend.functions";
    }
    if lower.contains("class")
        || lower.contains("object")
        || lower.contains("trait")
        || lower.contains("enum")
        || lower.contains("magic")
    {
        return "objects.classes";
    }
    if corpus_module == "filesystem" || corpus_module == "streams" {
        return "filesystem.streams";
    }
    match corpus_module {
        "standard.arrays" => "standard.arrays",
        "standard.strings" => "standard.strings",
        "json" => "json",
        "pcre" => "pcre",
        "date" => "date",
        "spl" => "spl",
        "reflection" => "reflection",
        "zend"
            if lower.contains("concat")
                || lower.contains("compare")
                || lower.contains("operator")
                || lower.contains("add_")
                || lower.contains("sub_")
                || lower.contains("mul_")
                || lower.contains("div_") =>
        {
            "operators.conversions"
        }
        "zend" => "zend.basic",
        "standard" if lower.contains("math") || lower.contains("round") => "standard.math",
        "standard" if lower.contains("serialize") => "standard.serialization",
        "standard" if lower.contains("string") || lower.contains("/strings/") => "standard.strings",
        "standard" => "standard.variables",
        "unknown" if lower.contains("/strings/") || lower.starts_with("tests/strings/") => {
            "strings.literals"
        }
        "unknown" => "extension.policy",
        _ => "extension.policy",
    }
}

fn write_triage_outputs(
    options: &TriageOptions,
    metadata: &BaselineMetadata,
    triage: &PhptTriage,
    known_gap_rows: &[KnownGapCatalogEntry],
) -> Result<(), String> {
    if let Some(parent) = options.report.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.extension_policy_report.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.known_gap_report.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.known_gap_catalog.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.priority.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::create_dir_all(&options.modules_dir)
        .map_err(|error| format!("{}: {error}", options.modules_dir.display()))?;
    fs::create_dir_all(&options.module_manifests_dir)
        .map_err(|error| format!("{}: {error}", options.module_manifests_dir.display()))?;

    fs::write(&options.report, render_triage_report(metadata, triage))
        .map_err(|error| format!("{}: {error}", options.report.display()))?;
    fs::write(
        &options.extension_policy_report,
        render_extension_policy_report(metadata, triage),
    )
    .map_err(|error| format!("{}: {error}", options.extension_policy_report.display()))?;
    fs::write(
        &options.known_gap_report,
        render_known_gap_report(metadata, known_gap_rows),
    )
    .map_err(|error| format!("{}: {error}", options.known_gap_report.display()))?;
    fs::write(
        &options.known_gap_catalog,
        render_known_gap_catalog(known_gap_rows),
    )
    .map_err(|error| format!("{}: {error}", options.known_gap_catalog.display()))?;
    fs::write(&options.priority, render_module_priority_json(triage))
        .map_err(|error| format!("{}: {error}", options.priority.display()))?;
    fs::write(
        options.modules_dir.join("README.md"),
        render_modules_readme(triage),
    )
    .map_err(|error| {
        format!(
            "{}: {error}",
            options.modules_dir.join("README.md").display()
        )
    })?;

    for (index, spec) in MODULE_PLAN.iter().enumerate() {
        let stats = triage.modules.get(spec.name).cloned().unwrap_or_default();
        let safe_module = safe_path_component(spec.name);
        let doc_path = options.modules_dir.join(format!("{safe_module}.md"));
        let manifest_path = options
            .module_manifests_dir
            .join(format!("{safe_module}.json"));
        let selected_manifest_path = options
            .module_manifests_dir
            .join(format!("{safe_module}.selected.jsonl"));
        // The curated selected manifest preserves itself inside
        // render_selected_manifest; the plan JSON and doc are derived data and
        // must always be re-rendered so their counts track the current
        // baseline instead of freezing at the curation timestamp.
        fs::write(
            &doc_path,
            render_module_doc(spec, index + 1, &stats, &selected_manifest_path),
        )
        .map_err(|error| format!("{}: {error}", doc_path.display()))?;
        fs::write(
            &manifest_path,
            render_module_manifest(spec, index + 1, &stats, &selected_manifest_path),
        )
        .map_err(|error| format!("{}: {error}", manifest_path.display()))?;
        fs::write(
            &selected_manifest_path,
            render_selected_manifest(&selected_manifest_path, &stats, options.selected_limit),
        )
        .map_err(|error| format!("{}: {error}", selected_manifest_path.display()))?;
    }
    Ok(())
}

fn has_curated_generated_manifest(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|existing| existing.contains("tests/phpt/generated/"))
        .unwrap_or(false)
}

fn render_triage_report(metadata: &BaselineMetadata, triage: &PhptTriage) -> String {
    let mut out = String::new();
    out.push_str("# PHPT Triage\n\n");
    out.push_str(&format!(
        "Baseline `{}` covers {} PHPTs: {} PASS, {} SKIP, {} FAIL, {} BORK.\n\n",
        metadata.timestamp,
        metadata.corpus_count,
        metadata.pass_count,
        metadata.skip_count,
        metadata.fail_count,
        metadata.bork_count
    ));
    if triage.count_source == "results" {
        out.push_str(
            "Per-module PASS/SKIP counts are based on the explicitly provided full-run results.\n\n",
        );
    } else if triage.count_source == "baseline-module-counts" {
        out.push_str(
            "Per-module PASS/SKIP counts are based on the committed baseline module-count manifest.\n\n",
        );
    } else {
        out.push_str("Per-module PASS/SKIP counts are unavailable because no full-run results were provided; FAIL/BORK counts come from the committed known-failure baseline.\n\n");
    }

    out.push_str("## Top Failing Modules\n\n");
    out.push_str("| Module | Priority | Corpus | PASS | SKIP | FAIL | BORK | Known non-green |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for (priority, spec, stats) in prioritized_modules(triage).into_iter().take(20) {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            spec.name,
            priority,
            stats.corpus_count,
            stats.pass_count,
            stats.skip_count,
            stats.fail_count,
            stats.bork_count,
            stats.known_failure_count
        ));
    }

    out.push_str("\n## Top Failure Clusters\n\n");
    render_count_table(&mut out, "Cluster", &triage.failure_clusters, 20);

    out.push_str("\n## Top Unsupported Feature Guesses\n\n");
    render_count_table(&mut out, "Guess", &triage.unsupported_guesses, 20);

    out.push_str("\n## BORK Subclasses\n\n");
    render_count_table(&mut out, "Subclass", &triage.bork_subclasses, 20);

    out.push_str("\n## Next Module Candidates\n\n");
    out.push_str("| Rank | Module | Reason |\n| ---: | --- | --- |\n");
    for (rank, (_priority, spec, stats)) in prioritized_modules(triage)
        .into_iter()
        .filter(|(_, _, stats)| stats.non_green() > 0)
        .take(10)
        .enumerate()
    {
        out.push_str(&format!(
            "| {} | {} | {} non-green, leverage {} |\n",
            rank + 1,
            spec.name,
            stats.non_green(),
            spec.leverage
        ));
    }

    out.push_str("\n## Extension Policy\n\n");
    out.push_str("Extension PHPTs remain in the corpus and full-regression baseline; this table classifies ownership instead of hiding failures.\n\n");
    render_extension_policy_table(&mut out, triage);

    out.push_str("\n## Raw Corpus Module Counts\n\n");
    out.push_str("| Module | Corpus | PASS | SKIP | FAIL | BORK | Known non-green |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    let mut raw = triage.raw_modules.iter().collect::<Vec<_>>();
    raw.sort_by(|left, right| {
        right
            .1
            .known_failure_count
            .cmp(&left.1.known_failure_count)
            .then_with(|| left.0.cmp(right.0))
    });
    for (module, stats) in raw.into_iter().take(40) {
        out.push_str(&format!(
            "| {module} | {} | {} | {} | {} | {} | {} |\n",
            stats.corpus_count,
            stats.pass_count,
            stats.skip_count,
            stats.fail_count,
            stats.bork_count,
            stats.known_failure_count
        ));
    }
    out
}

fn render_extension_policy_report(metadata: &BaselineMetadata, triage: &PhptTriage) -> String {
    let mut out = String::new();
    out.push_str("# PHPT Extension Policy\n\n");
    out.push_str(&format!(
        "Generated from baseline `{}` with {} PHPT corpus entries and {} known non-green fingerprints.\n\n",
        metadata.timestamp, metadata.corpus_count, metadata.known_failure_count
    ));
    out.push_str("Extension PHPTs remain in the corpus and full-regression baseline. Policy classification uses `required-core`, `required-composer`, `required-framework`, `optional`, and `out-of-scope`; implementation class uses `stub-only`, `MVP`, `real-implementation-required`, or `already-implemented`. Classification does not remove tests from accounting.\n\n");
    out.push_str("## Policy Table\n\n");
    render_extension_policy_table(&mut out, triage);
    out.push_str("\n## Invariants\n\n");
    out.push_str("- Extension PHPT counts come from `tests/phpt/manifests/phpt-corpus.jsonl` and the committed known-failure baseline.\n");
    out.push_str("- Extension failures are still present in `target/phpt-work/reports/triage.md` and `target/phpt-work/reports/full-baseline.md`.\n");
    out.push_str("- Out-of-scope means not required for strict core progress; it does not mean silently skipped or deleted.\n");
    out.push_str("- Stub or implementation work must be added in the owning functional module, not as generated implementation-history artifacts.\n");
    out
}

fn render_extension_policy_table(out: &mut String, triage: &PhptTriage) {
    out.push_str("| Extension | Policy | PHPT count | PASS | SKIP | FAIL | BORK | Top failure clusters | Required for Core | Required for Composer | Framework relevant | Implementation class | Next action |\n");
    out.push_str(
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- | --- | --- | --- | --- |\n",
    );
    for spec in EXTENSION_POLICY {
        let stats = extension_policy_stats(triage, spec.extension);
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            spec.extension,
            spec.policy,
            stats.corpus_count,
            stats.pass_count,
            stats.skip_count,
            stats.fail_count,
            stats.bork_count,
            format_top_clusters(&stats),
            yes_no(spec.required_for_core),
            yes_no(spec.required_for_composer),
            yes_no(spec.required_for_framework),
            spec.implementation_class,
            spec.next_action
        ));
    }
}

fn extension_policy_stats(triage: &PhptTriage, extension: &str) -> ModuleTriageStats {
    triage
        .raw_modules
        .get(extension)
        .cloned()
        .unwrap_or_default()
}

fn format_top_clusters(stats: &ModuleTriageStats) -> String {
    if stats.failure_clusters.is_empty() {
        return "none".to_string();
    }
    let mut clusters = stats.failure_clusters.iter().collect::<Vec<_>>();
    clusters.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    clusters
        .into_iter()
        .take(3)
        .map(|(cluster, count)| format!("`{cluster}` {count}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn build_known_gap_rows(
    failures: &[KnownFailure],
    module_counts: &[BaselineModuleCount],
) -> Vec<KnownGapCatalogEntry> {
    let mut counts = BTreeMap::<String, usize>::new();
    let mut examples = BTreeMap::<String, String>::new();
    let mut extension_counts = BTreeMap::<String, usize>::new();
    let mut extension_examples = BTreeMap::<String, String>::new();
    for failure in failures {
        let id = failure.primary_missing_feature_guess.as_str();
        if id.is_empty() {
            continue;
        }
        *counts.entry(id.to_string()).or_default() += 1;
        examples
            .entry(id.to_string())
            .or_insert_with(|| failure.path.clone());
        *extension_counts
            .entry(failure.module_tag.clone())
            .or_default() += 1;
        extension_examples
            .entry(failure.module_tag.clone())
            .or_insert_with(|| failure.path.clone());
    }
    for count in module_counts {
        if count.kind == "bork_subclass" {
            *counts.entry(count.module.clone()).or_default() += count.known_failure_count;
        } else if count.kind == "raw" {
            extension_counts.insert(count.module.clone(), count.known_failure_count);
        }
    }

    let mut rows = KNOWN_GAP_CATALOG
        .iter()
        .map(|spec| KnownGapCatalogEntry {
            id: spec.id.to_string(),
            title: spec.title.to_string(),
            reference_behavior: spec.reference_behavior.to_string(),
            current_rust_behavior: spec.current_rust_behavior.to_string(),
            fixture_or_phpt_example: examples
                .get(spec.id)
                .cloned()
                .unwrap_or_else(|| spec.fixture_or_phpt_example.to_string()),
            planned_solution_layer: spec.planned_solution_layer.to_string(),
            baseline_count: *counts.get(spec.id).unwrap_or(&0),
        })
        .collect::<Vec<_>>();
    rows.extend(EXTENSION_POLICY.iter().map(|spec| {
        let id = format!("extension-policy-{}", spec.extension);
        KnownGapCatalogEntry {
            id,
            title: format!("Extension policy for {}", spec.extension),
            reference_behavior: format!(
                "Reference PHP provides the {} extension behavior covered by its PHPT corpus when the extension is enabled.",
                spec.extension
            ),
            current_rust_behavior: format!(
                "phrust classifies {} as {} with implementation class {}; non-green PHPTs stay visible in full-regression accounting.",
                spec.extension, spec.policy, spec.implementation_class
            ),
            fixture_or_phpt_example: extension_examples
                .get(spec.extension)
                .cloned()
                .unwrap_or_else(|| spec.fixture_or_phpt_example.to_string()),
            planned_solution_layer: spec.planned_solution_layer.to_string(),
            baseline_count: *extension_counts.get(spec.extension).unwrap_or(&0),
        }
    }));
    rows
}

fn render_known_gap_catalog(entries: &[KnownGapCatalogEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        out.push_str(&entry.to_json_line());
        out.push('\n');
    }
    out
}

fn render_known_gap_report(
    metadata: &BaselineMetadata,
    entries: &[KnownGapCatalogEntry],
) -> String {
    let mut out = String::new();
    out.push_str("# PHPT Known Gaps\n\n");
    out.push_str(&format!(
        "Generated from baseline `{}` with {} known non-green fingerprints. This catalog is the stable owner map for PHPT failures that are accepted in the committed full baseline.\n\n",
        metadata.timestamp, metadata.known_failure_count
    ));
    out.push_str("Each row carries the hard-rule fields required for a known gap: ID, reference behavior, current Rust behavior, fixture or PHPT example, and planned solution layer.\n\n");
    out.push_str("| ID | Baseline count | Reference behavior | Current Rust behavior | Fixture or PHPT example | Planned solution layer |\n");
    out.push_str("| --- | ---: | --- | --- | --- | --- |\n");
    for entry in entries {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | `{}` | {} |\n",
            entry.id,
            entry.baseline_count,
            entry.reference_behavior,
            entry.current_rust_behavior,
            entry.fixture_or_phpt_example,
            entry.planned_solution_layer
        ));
    }
    out.push_str("\n## Invariants\n\n");
    out.push_str("- `tests/phpt/manifests/known-gap-catalog.jsonl` is the machine-readable form of this catalog.\n");
    out.push_str("- `just phpt-verify-baseline` rejects a known failure whose `primary_missing_feature_guess` is missing here.\n");
    out.push_str(
        "- BORK subclasses from `full-baseline-module-counts.jsonl` must also have catalog rows.\n",
    );
    out.push_str("- The catalog documents accepted baseline gaps only; it does not make new failures acceptable without `PHPT_ACCEPT_BASELINE=1`.\n");
    out
}

fn render_count_table(
    out: &mut String,
    label: &str,
    counts: &BTreeMap<String, usize>,
    limit: usize,
) {
    out.push_str(&format!("| {label} | Count |\n| --- | ---: |\n"));
    let mut rows = counts.iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    for (name, count) in rows.into_iter().take(limit) {
        out.push_str(&format!("| {name} | {count} |\n"));
    }
    if counts.is_empty() {
        out.push_str("| none | 0 |\n");
    }
}

fn prioritized_modules(
    triage: &PhptTriage,
) -> Vec<(usize, &'static ModulePlanSpec, ModuleTriageStats)> {
    let mut rows = MODULE_PLAN
        .iter()
        .enumerate()
        .map(|(index, spec)| {
            (
                index + 1,
                spec,
                triage.modules.get(spec.name).cloned().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| right.2.non_green().cmp(&left.2.non_green()))
            .then_with(|| right.1.leverage.cmp(&left.1.leverage))
            .then_with(|| left.1.name.cmp(right.1.name))
    });
    rows
}

fn render_module_priority_json(triage: &PhptTriage) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"schema_version\":\"phpt-module-priority-v1\",\n");
    out.push_str(&format!(
        "  \"has_result_counts\":{},\n",
        if triage.has_result_counts {
            "true"
        } else {
            "false"
        }
    ));
    out.push_str(&format!(
        "  \"count_source\":\"{}\",\n",
        escape_json(&triage.count_source)
    ));
    out.push_str("  \"modules\":[\n");
    for (row_index, (priority, spec, stats)) in prioritized_modules(triage).into_iter().enumerate()
    {
        if row_index > 0 {
            out.push_str(",\n");
        }
        out.push_str("    {\n");
        out.push_str(&format!("      \"priority\":{},\n", priority));
        out.push_str(&format!(
            "      \"module\":\"{}\",\n",
            escape_json(spec.name)
        ));
        out.push_str(&format!("      \"leverage\":{},\n", spec.leverage));
        out.push_str(&format!("      \"corpus_count\":{},\n", stats.corpus_count));
        out.push_str(&format!("      \"pass_count\":{},\n", stats.pass_count));
        out.push_str(&format!("      \"skip_count\":{},\n", stats.skip_count));
        out.push_str(&format!("      \"fail_count\":{},\n", stats.fail_count));
        out.push_str(&format!("      \"bork_count\":{},\n", stats.bork_count));
        out.push_str(&format!(
            "      \"known_failure_count\":{},\n",
            stats.known_failure_count
        ));
        out.push_str(&format!(
            "      \"next_step\":\"{}\"\n",
            escape_json(spec.next_step)
        ));
        out.push_str("    }");
    }
    out.push_str("\n  ]\n}\n");
    out
}

fn render_baseline_module_counts(triage: &PhptTriage) -> String {
    let mut out = String::new();
    for spec in MODULE_PLAN {
        let stats = triage.modules.get(spec.name).cloned().unwrap_or_default();
        push_baseline_module_count(&mut out, "plan", spec.name, &stats);
    }

    let mut raw_modules = triage.raw_modules.iter().collect::<Vec<_>>();
    raw_modules.sort_by(|left, right| {
        right
            .1
            .known_failure_count
            .cmp(&left.1.known_failure_count)
            .then_with(|| right.1.corpus_count.cmp(&left.1.corpus_count))
            .then_with(|| left.0.cmp(right.0))
    });
    for (module, stats) in raw_modules {
        push_baseline_module_count(&mut out, "raw", module, stats);
    }

    let mut bork_subclasses = triage.bork_subclasses.iter().collect::<Vec<_>>();
    bork_subclasses.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    for (subclass, count) in bork_subclasses {
        let stats = ModuleTriageStats {
            bork_count: *count,
            known_failure_count: *count,
            ..ModuleTriageStats::default()
        };
        push_baseline_module_count(&mut out, "bork_subclass", subclass, &stats);
    }
    out
}

fn push_baseline_module_count(
    out: &mut String,
    kind: &str,
    module: &str,
    stats: &ModuleTriageStats,
) {
    out.push_str(&format!(
        "{{\"kind\":\"{}\",\"module\":\"{}\",\"corpus_count\":{},\"pass_count\":{},\"skip_count\":{},\"fail_count\":{},\"bork_count\":{},\"known_failure_count\":{}}}\n",
        escape_json(kind),
        escape_json(module),
        stats.corpus_count,
        stats.pass_count,
        stats.skip_count,
        stats.fail_count,
        stats.bork_count,
        stats.known_failure_count
    ));
}

fn render_modules_readme(triage: &PhptTriage) -> String {
    let mut out = String::new();
    out.push_str("# PHPT Module Plan\n\n");
    out.push_str("This directory contains the functional module plan for PHPT-driven runtime completion. The order is based on core language dependencies, failure volume, and expected leverage across later modules.\n\n");
    out.push_str("| Priority | Module | Corpus | PASS | SKIP | FAIL | BORK | Next step |\n");
    out.push_str("| ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |\n");
    for (priority, spec, stats) in prioritized_modules(triage) {
        out.push_str(&format!(
            "| {} | [{}]({}.md) | {} | {} | {} | {} | {} | {} |\n",
            priority,
            spec.name,
            safe_path_component(spec.name),
            stats.corpus_count,
            stats.pass_count,
            stats.skip_count,
            stats.fail_count,
            stats.bork_count,
            spec.next_step
        ));
    }
    out.push_str("\nThe `extension.policy` module is the merged projection for `phpt/ext-policy-orchestration`, `phpt/ext-text-i18n`, `phpt/ext-xml-soap`, and `phpt/ext-data-platform`. Its counts stay tied to the committed full baseline; module docs and manifests classify extension ownership without removing failures from PHPT bookkeeping.\n");
    out
}

fn render_module_doc(
    spec: &ModulePlanSpec,
    priority: usize,
    stats: &ModuleTriageStats,
    selected_manifest: &Path,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", spec.name));
    out.push_str(&format!("- Priority: {priority}\n"));
    out.push_str(&format!(
        "- Selected manifest: `{}`\n",
        selected_manifest.display()
    ));
    out.push_str(&format!(
        "- Current counts: {} PASS, {} SKIP, {} FAIL, {} BORK from {} corpus candidates\n",
        stats.pass_count, stats.skip_count, stats.fail_count, stats.bork_count, stats.corpus_count
    ));
    out.push_str("\n## Scope\n\n");
    for item in spec.scope {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("\n## Non-Scope\n\n");
    for item in spec.non_scope {
        out.push_str(&format!("- {item}\n"));
    }
    out.push_str("\n## Relevant PHPT Paths\n\n");
    for path in stats.relevant_paths.iter().take(40) {
        out.push_str(&format!("- `{path}`\n"));
    }
    if stats.relevant_paths.is_empty() {
        out.push_str("- none identified yet\n");
    }
    out.push_str("\n## Relevant php-src Source Areas\n\n");
    for item in spec.source_places {
        out.push_str(&format!("- `{item}`\n"));
    }
    out.push_str("\n## Target Gates\n\n");
    for gate in spec.target_gates {
        out.push_str(&format!("- `{gate}`\n"));
    }
    out.push_str("\n## Known Gaps\n\n");
    if stats.failure_clusters.is_empty() {
        out.push_str("- no known non-green fingerprints assigned in the current baseline\n");
    } else {
        let mut clusters = stats.failure_clusters.iter().collect::<Vec<_>>();
        clusters.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
        for (cluster, count) in clusters {
            out.push_str(&format!("- `{cluster}`: {count}\n"));
        }
    }
    out.push_str("\n## Next Step\n\n");
    out.push_str(spec.next_step);
    out.push('\n');
    out
}

fn render_module_manifest(
    spec: &ModulePlanSpec,
    priority: usize,
    stats: &ModuleTriageStats,
    selected_manifest: &Path,
) -> String {
    format!(
        concat!(
            "{{\n",
            "  \"schema_version\":\"phpt-module-plan-v1\",\n",
            "  \"module\":\"{}\",\n",
            "  \"priority\":{},\n",
            "  \"selected_manifest\":\"{}\",\n",
            "  \"corpus_count\":{},\n",
            "  \"pass_count\":{},\n",
            "  \"skip_count\":{},\n",
            "  \"fail_count\":{},\n",
            "  \"bork_count\":{},\n",
            "  \"known_failure_count\":{},\n",
            "  \"scope\":{},\n",
            "  \"non_scope\":{},\n",
            "  \"target_gates\":{},\n",
            "  \"next_step\":\"{}\"\n",
            "}}\n"
        ),
        escape_json(spec.name),
        priority,
        escape_json(&selected_manifest.to_string_lossy().replace('\\', "/")),
        stats.corpus_count,
        stats.pass_count,
        stats.skip_count,
        stats.fail_count,
        stats.bork_count,
        stats.known_failure_count,
        json_str_array(spec.scope),
        json_str_array(spec.non_scope),
        json_str_array(spec.target_gates),
        escape_json(spec.next_step)
    )
}

fn render_selected_manifest(path: &Path, stats: &ModuleTriageStats, limit: usize) -> String {
    if has_curated_generated_manifest(path)
        && let Ok(existing) = fs::read_to_string(path)
    {
        return existing;
    }
    let mut out = String::new();
    let paths = if stats.selected_paths.is_empty() {
        &stats.relevant_paths
    } else {
        &stats.selected_paths
    };
    for path in paths.iter().take(limit) {
        out.push_str(&format!("{{\"path\":\"{}\"}}\n", escape_json(path)));
    }
    out
}

fn json_str_array(values: &[&str]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&escape_json(value));
        out.push('"');
    }
    out.push(']');
    out
}

fn compare_report_total(
    outcome: &str,
    expected: usize,
    report: &BaselineReportTotals,
    errors: &mut Vec<String>,
) {
    let actual = *report.outcomes.get(outcome).unwrap_or(&0);
    if expected != actual {
        errors.push(format!(
            "{outcome} count mismatch: metadata={expected} report={actual}"
        ));
    }
}

fn verify_baseline_module_counts(
    counts: &[BaselineModuleCount],
    metadata: &BaselineMetadata,
    errors: &mut Vec<String>,
) {
    if counts.is_empty() {
        errors.push("baseline module-count manifest is empty".to_string());
        return;
    }

    let mut plan_corpus = 0usize;
    let mut plan_known = 0usize;
    let mut seen_plan_modules = std::collections::BTreeSet::new();
    let mut bork_subclasses = 0usize;
    for count in counts {
        match count.kind.as_str() {
            "plan" => {
                plan_corpus += count.corpus_count;
                plan_known += count.known_failure_count;
                seen_plan_modules.insert(count.module.as_str());
            }
            "bork_subclass" => {
                bork_subclasses += count.known_failure_count;
            }
            "raw" => {}
            other => errors.push(format!(
                "baseline module-count manifest contains unknown kind `{other}` for `{}`",
                count.module
            )),
        }
    }

    for spec in MODULE_PLAN {
        if !seen_plan_modules.contains(spec.name) {
            errors.push(format!(
                "baseline module-count manifest is missing plan module `{}`",
                spec.name
            ));
        }
    }
    if plan_corpus != metadata.corpus_count {
        errors.push(format!(
            "plan module corpus_count sum mismatch: metadata={} module_counts={plan_corpus}",
            metadata.corpus_count
        ));
    }
    if plan_known != metadata.known_failure_count {
        errors.push(format!(
            "plan module known_failure_count sum mismatch: metadata={} module_counts={plan_known}",
            metadata.known_failure_count
        ));
    }
    if bork_subclasses != metadata.bork_count {
        errors.push(format!(
            "BORK subclass count sum mismatch: metadata={} module_counts={bork_subclasses}",
            metadata.bork_count
        ));
    }
}

fn verify_known_gap_catalog(
    catalog: &[KnownGapCatalogEntry],
    failures: &[KnownFailure],
    module_counts: &[BaselineModuleCount],
    metadata: &BaselineMetadata,
    errors: &mut Vec<String>,
) {
    if metadata.known_failure_count > 0 && catalog.is_empty() {
        errors.push("PHPT known-gap catalog is empty while known failures exist".to_string());
        return;
    }

    let mut ids = BTreeSet::new();
    let mut rows = BTreeMap::<String, &KnownGapCatalogEntry>::new();
    for entry in catalog {
        if entry.id.is_empty()
            || entry.title.is_empty()
            || entry.reference_behavior.is_empty()
            || entry.current_rust_behavior.is_empty()
            || entry.fixture_or_phpt_example.is_empty()
            || entry.planned_solution_layer.is_empty()
        {
            errors.push(format!(
                "PHPT known-gap catalog row `{}` has an empty required field",
                entry.id
            ));
        }
        if !ids.insert(entry.id.as_str()) {
            errors.push(format!(
                "PHPT known-gap catalog contains duplicate id `{}`",
                entry.id
            ));
        }
        rows.insert(entry.id.clone(), entry);
    }

    for spec in KNOWN_GAP_CATALOG {
        if !rows.contains_key(spec.id) {
            errors.push(format!(
                "PHPT known-gap catalog is missing required id `{}`",
                spec.id
            ));
        }
    }

    let expected = build_known_gap_rows(failures, module_counts);
    for expected_row in expected {
        let Some(actual) = rows.get(&expected_row.id) else {
            continue;
        };
        if actual.baseline_count != expected_row.baseline_count {
            errors.push(format!(
                "PHPT known-gap `{}` baseline_count mismatch: catalog={} expected={}",
                expected_row.id, actual.baseline_count, expected_row.baseline_count
            ));
        }
    }

    for failure in failures {
        if !rows.contains_key(&failure.primary_missing_feature_guess) {
            errors.push(format!(
                "PHPT known-gap catalog is missing primary_missing_feature_guess `{}` for `{}`",
                failure.primary_missing_feature_guess, failure.path
            ));
            break;
        }
    }
    for count in module_counts {
        if count.kind == "bork_subclass" && !rows.contains_key(&count.module) {
            errors.push(format!(
                "PHPT known-gap catalog is missing BORK subclass `{}`",
                count.module
            ));
        }
    }
}

fn failure_fingerprint(result: &PhptRunResult) -> String {
    let mut hasher = Sha256::new();
    hasher.update(result.outcome.as_bytes());
    hasher.update(b"\0");
    hasher.update(normalize_failure_detail_for_fingerprint(&result.detail).as_bytes());
    format!("{:x}", hasher.finalize())
}

fn normalize_failure_detail_for_fingerprint(detail: &str) -> String {
    let mut normalized = detail.to_string();
    normalize_repository_paths(&mut normalized);
    normalize_work_directory_paths(&mut normalized);
    let thread_marker = "thread 'main' (";
    while let Some(start) = normalized.find(thread_marker) {
        let digits_start = start + thread_marker.len();
        let Some(close_offset) = normalized[digits_start..].find(')') else {
            break;
        };
        let digits_end = digits_start + close_offset;
        if normalized[digits_start..digits_end]
            .chars()
            .all(|ch| ch.is_ascii_digit())
        {
            normalized.replace_range(digits_start..digits_end, "<thread-id>");
        } else {
            break;
        }
    }
    normalized = normalize_rust_source_locations(&normalized);
    if normalized.contains("PHPT_TIMEOUT after") {
        return "PHPT_TIMEOUT".to_string();
    }
    if normalized.starts_with("output did not match expectation")
        && let Some(excerpt_start) = normalized.find(" expected=`")
    {
        normalized.truncate(excerpt_start);
        normalized.push_str(" expected=<excerpt> actual=<excerpt>");
    }
    if normalized.contains("E_PHP_IR_TRAIT_METHOD_CONFLICT") {
        let mut lines = normalized
            .lines()
            .map(|line| {
                if let Some(rest) = line.strip_prefix("stderr=") {
                    rest.to_string()
                } else if line.starts_with("target exited with status ") {
                    line.find("; stderr=")
                        .map(|offset| line[offset + "; stderr=".len()..].to_string())
                        .unwrap_or_else(|| line.to_string())
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>();
        lines.sort_unstable();
        normalized = lines.join("\n");
    }
    normalized
}

fn normalize_rust_source_locations(detail: &str) -> String {
    let mut normalized = detail.to_string();
    let mut search_start = 0;
    while let Some(marker_offset) = normalized[search_start..].find(".rs:") {
        let marker_start = search_start + marker_offset;
        let line_start = marker_start + ".rs:".len();
        let Some(line_end_offset) = normalized[line_start..].find(':') else {
            break;
        };
        let line_end = line_start + line_end_offset;
        if line_start == line_end
            || !normalized[line_start..line_end]
                .chars()
                .all(|ch| ch.is_ascii_digit())
        {
            search_start = line_start;
            continue;
        }

        let col_start = line_end + 1;
        let col_end = normalized[col_start..]
            .find(|ch: char| !ch.is_ascii_digit())
            .map(|offset| col_start + offset)
            .unwrap_or(normalized.len());
        if col_start == col_end {
            search_start = col_start;
            continue;
        }

        normalized.replace_range(line_start..col_end, "<line>:<col>");
        search_start = line_start + "<line>:<col>".len();
    }
    normalized
}

fn missing_feature_guess(result: &PhptRunResult) -> String {
    let detail = result.detail.to_ascii_lowercase();
    if result.outcome == "BORK" && detail.contains("unsupported section") {
        "phpt-runner-section".to_string()
    } else if detail.contains("phpt_timeout") {
        "runtime-timeout".to_string()
    } else if detail.contains("parse") || detail.contains("syntax") {
        "frontend-parse-or-compile".to_string()
    } else if detail.contains("unsupported") || detail.contains("not implemented") {
        "runtime-unsupported-feature".to_string()
    } else if detail.contains("target exited") {
        "runtime-error-or-diagnostic".to_string()
    } else if detail.contains("expected") || detail.contains("actual") {
        "runtime-output-mismatch".to_string()
    } else {
        "needs-triage".to_string()
    }
}

const LITERAL_KIND_UNSUPPORTED_DIAGNOSTIC: &str =
    "E_PHP_IR_UNSUPPORTED_HIR_STATEMENT: literal kind is not lowered to IR";
const ADVANCED_PARAMETER_UNFOLDED_DIAGNOSTIC: &str =
    "parameter default is not a folded Semantic frontend constant expression";
const VM_STEP_LIMIT_DIAGNOSTIC: &str = "VM step limit exceeded";
const PHPT_TIMEOUT_DIAGNOSTIC: &str = "PHPT_TIMEOUT after";

fn is_related_known_failure_evolution(
    previous: Option<&PhptRunResult>,
    current: Option<&PhptRunResult>,
) -> bool {
    let (Some(previous), Some(current)) = (previous, current) else {
        return false;
    };
    if previous.path != current.path
        || matches!(current.outcome.as_str(), "PASS" | "SKIP" | "XFAIL")
    {
        return false;
    }
    if previous.outcome == "BORK" && current.outcome == "FAIL" {
        return true;
    }
    previous
        .detail
        .contains(LITERAL_KIND_UNSUPPORTED_DIAGNOSTIC)
        || previous
            .detail
            .contains(ADVANCED_PARAMETER_UNFOLDED_DIAGNOSTIC)
        || related_runtime_limit_failure(previous, current)
        || related_target_exit_expectation_detail(previous, current)
        || (previous
            .detail
            .starts_with("output did not match expectation")
            && current
                .detail
                .starts_with("output did not match expectation"))
}

fn related_runtime_limit_failure(previous: &PhptRunResult, current: &PhptRunResult) -> bool {
    let previous_limited = previous.detail.contains(VM_STEP_LIMIT_DIAGNOSTIC)
        || previous.detail.contains(PHPT_TIMEOUT_DIAGNOSTIC);
    let current_limited = current.detail.contains(VM_STEP_LIMIT_DIAGNOSTIC)
        || current.detail.contains(PHPT_TIMEOUT_DIAGNOSTIC);
    previous_limited && current_limited
}

fn related_target_exit_expectation_detail(
    previous: &PhptRunResult,
    current: &PhptRunResult,
) -> bool {
    if !previous.detail.starts_with("target exited with status ")
        || !current
            .detail
            .starts_with("output did not match expectation")
        || !current.detail.contains("; target exited with status ")
    {
        return false;
    }
    let Some(previous_stderr) = stderr_payload(&previous.detail) else {
        return false;
    };
    let Some(current_stderr) = stderr_payload(&current.detail) else {
        return false;
    };
    normalize_failure_detail_for_fingerprint(previous_stderr)
        == normalize_failure_detail_for_fingerprint(current_stderr)
}

fn stderr_payload(detail: &str) -> Option<&str> {
    detail
        .find("; stderr=")
        .map(|offset| &detail[offset + "; stderr=".len()..])
        .or_else(|| {
            detail
                .find("stderr=")
                .map(|offset| &detail[offset + "stderr=".len()..])
        })
}

fn render_baseline_report(
    results: &[PhptRunResult],
    failures: &[KnownFailure],
    timestamp: &str,
) -> String {
    let mut outcomes = BTreeMap::<String, usize>::new();
    for result in results {
        *outcomes.entry(result.outcome.clone()).or_default() += 1;
    }
    let mut clusters = BTreeMap::<String, usize>::new();
    for failure in failures {
        *clusters
            .entry(failure.primary_missing_feature_guess.clone())
            .or_default() += 1;
    }
    let mut modules = BTreeMap::<String, usize>::new();
    for failure in failures {
        *modules.entry(failure.module_tag.clone()).or_default() += 1;
    }

    let mut out = String::new();
    out.push_str("# PHPT Full PHPT Baseline\n\n");
    out.push_str(&format!("Generated: `{timestamp}`\n\n"));
    out.push_str("## Totals\n\n");
    out.push_str("| Outcome | Count |\n| --- | ---: |\n");
    for (outcome, count) in outcomes {
        out.push_str(&format!("| {outcome} | {count} |\n"));
    }
    out.push_str("\n## Top Failure Clusters\n\n");
    let mut cluster_counts = clusters.into_iter().collect::<Vec<_>>();
    cluster_counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    out.push_str("| Cluster | Count |\n| --- | ---: |\n");
    for (cluster, count) in cluster_counts.iter().take(20) {
        out.push_str(&format!("| {cluster} | {count} |\n"));
    }
    out.push_str("\n## Top Failing Modules\n\n");
    let mut module_counts = modules.into_iter().collect::<Vec<_>>();
    module_counts.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    out.push_str("| Module | Count |\n| --- | ---: |\n");
    for (module, count) in module_counts.iter().take(20) {
        out.push_str(&format!("| {module} | {count} |\n"));
    }
    out.push_str("\n## Policy\n\n");
    out.push_str(
        "Module work may reduce known failures, but must not add new failures or mutate unrelated fingerprints without explanation.\n",
    );
    out
}

fn parse_duration_seconds(value: &str) -> Result<Duration, String> {
    let seconds = value
        .parse::<u64>()
        .map_err(|_| format!("invalid duration seconds `{value}`"))?;
    if seconds == 0 {
        return Err("timeout must be greater than zero".to_string());
    }
    Ok(Duration::from_secs(seconds))
}

fn parse_jobs(value: &str) -> Result<usize, String> {
    let jobs = parse_usize(value, "jobs")?;
    if jobs == 0 {
        return Err("jobs must be greater than zero".to_string());
    }
    Ok(jobs)
}

fn env_flag(name: &str) -> bool {
    env::var(name).ok().is_some_and(|value| {
        matches!(
            value.as_str(),
            "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
        )
    })
}

fn parse_bool_flag(value: &str, name: &str) -> Result<bool, String> {
    match value {
        "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON" => Ok(true),
        "0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF" => Ok(false),
        _ => Err(format!(
            "invalid {name} value `{value}`; expected true or false"
        )),
    }
}

fn default_phpt_jobs() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .clamp(1, 8)
}

fn parse_usize(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("invalid {name} value `{value}`"))
}

fn infer_target_mode(target: &Path) -> TargetMode {
    if target
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "php-vm")
    {
        TargetMode::PhpVm
    } else {
        TargetMode::PhpCli
    }
}

fn sanitize_env_name(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn render_run_summary(results: &[PhptRunResult]) -> String {
    let mut counts = BTreeMap::<String, usize>::new();
    for result in results {
        *counts.entry(result.outcome.clone()).or_default() += 1;
    }
    let mut out = String::new();
    out.push_str("# PHPT Run Summary\n\n");
    out.push_str("| Outcome | Count |\n| --- | ---: |\n");
    for (outcome, count) in counts {
        out.push_str(&format!("| {outcome} | {count} |\n"));
    }
    out.push_str("\n## Non-green Results\n\n");
    for result in results {
        if !matches!(result.outcome.as_str(), "PASS" | "SKIP" | "XFAIL") {
            out.push_str(&format!(
                "- `{}`: {} - {}\n",
                result.path, result.outcome, result.detail
            ));
        }
    }
    out
}

fn collect_symbol_source_files(
    php_src: &Path,
    current: &Path,
    files: &mut Vec<String>,
) -> Result<(), String> {
    let mut children = fs::read_dir(current)
        .map_err(|error| format!("{}: {error}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", current.display()))?;
    children.sort_by_key(|entry| entry.path());
    for child in children {
        let path = child.path();
        let file_type = child
            .file_type()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if file_type.is_dir() {
            if should_skip_dir(php_src, &path) {
                continue;
            }
            collect_symbol_source_files(php_src, &path, files)?;
        } else if file_type.is_file() {
            let rel = relative_path(php_src, &path)?;
            if is_core_source_path(&rel) && is_symbol_source_file(&rel) {
                files.push(rel);
            }
        }
    }
    Ok(())
}

fn scan_symbol_file(path: &Path, rel: &str, entries: &mut Vec<SymbolEntry>) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let source = String::from_utf8_lossy(&bytes);
    let module = module_guess(rel);
    for (index, line) in source.lines().enumerate() {
        let line_number = index as u64 + 1;
        for (macro_name, kind) in [
            ("PHP_FUNCTION", "php_function"),
            ("ZEND_FUNCTION", "zend_function"),
        ] {
            if let Some(args) = macro_args(line, macro_name) {
                let name = args.trim().to_string();
                if !name.is_empty() {
                    entries.push(SymbolEntry {
                        kind: kind.to_string(),
                        php_name: name.clone(),
                        c_name: format!("{macro_name}({name})"),
                        path: rel.to_string(),
                        line: line_number,
                        module: module.clone(),
                    });
                }
            }
        }
        for (macro_name, kind) in [("PHP_METHOD", "php_method"), ("ZEND_METHOD", "zend_method")] {
            if let Some(args) = macro_args(line, macro_name) {
                let parts = args
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>();
                if parts.len() >= 2 {
                    entries.push(SymbolEntry {
                        kind: kind.to_string(),
                        php_name: format!("{}::{}", parts[0], parts[1]),
                        c_name: format!("{macro_name}({}, {})", parts[0], parts[1]),
                        path: rel.to_string(),
                        line: line_number,
                        module: module.clone(),
                    });
                }
            }
        }
        if let Some(class_name) = init_class_entry_name(line) {
            entries.push(SymbolEntry {
                kind: "class_entry".to_string(),
                php_name: class_name.clone(),
                c_name: "INIT_CLASS_ENTRY".to_string(),
                path: rel.to_string(),
                line: line_number,
                module: module.clone(),
            });
        }
        if let Some(module_name) = module_entry_name(line) {
            entries.push(SymbolEntry {
                kind: "module_entry".to_string(),
                php_name: module_name.clone(),
                c_name: format!("{module_name}_module_entry"),
                path: rel.to_string(),
                line: line_number,
                module: module.clone(),
            });
        }
    }
    Ok(())
}

fn first_non_empty_line(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

fn has_section(sections: &[PhptSection], name: &str) -> bool {
    sections.iter().any(|section| section.name == name)
}

fn expectation_kind(sections: &[PhptSection]) -> String {
    for name in [
        "EXPECT",
        "EXPECTF",
        "EXPECTREGEX",
        "EXPECT_EXTERNAL",
        "EXPECTF_EXTERNAL",
        "EXPECTREGEX_EXTERNAL",
    ] {
        if has_section(sections, name) {
            return name.to_ascii_lowercase();
        }
    }
    "none".to_string()
}

fn phpt_module_tag(rel: &str, sections: &[PhptSection]) -> String {
    if rel.starts_with("Zend/") {
        return "zend".to_string();
    }
    if rel.starts_with("sapi/") {
        return "sapi".to_string();
    }
    if rel.contains("/streams/") || rel.contains("stream") {
        return "streams".to_string();
    }
    if rel.contains("filesystem") || rel.contains("/file/") || rel.contains("file_") {
        return "filesystem".to_string();
    }
    if rel.starts_with("ext/json/") {
        return "json".to_string();
    }
    if rel.starts_with("ext/pcre/") {
        return "pcre".to_string();
    }
    if rel.starts_with("ext/date/") {
        return "date".to_string();
    }
    if rel.starts_with("ext/spl/") {
        return "spl".to_string();
    }
    if rel.starts_with("ext/reflection/") {
        return "reflection".to_string();
    }
    if rel.starts_with("ext/tokenizer/") {
        return "tokenizer".to_string();
    }
    if rel.starts_with("ext/standard/") {
        let lower = rel.to_ascii_lowercase();
        if lower.contains("array") {
            return "standard.arrays".to_string();
        }
        if lower.contains("string") || lower.contains("str_") {
            return "standard.strings".to_string();
        }
        return "standard".to_string();
    }
    for section in sections {
        if section.name == "EXTENSIONS" {
            let first = section
                .body
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_ascii_lowercase();
            if !first.is_empty() {
                return first;
            }
        }
    }
    "unknown".to_string()
}

fn render_phpt_summary(entries: &[PhptEntry]) -> String {
    let mut by_module = BTreeMap::<String, usize>::new();
    let mut by_expectation = BTreeMap::<String, usize>::new();
    let mut section_counts = BTreeMap::<String, usize>::new();
    let mut skipif = 0usize;
    let mut clean = 0usize;
    let mut redirect = 0usize;
    let mut external = 0usize;
    let mut http = 0usize;
    let mut stdin_args = 0usize;

    for entry in entries {
        *by_module.entry(entry.module.clone()).or_default() += 1;
        *by_expectation
            .entry(entry.expectation_kind.clone())
            .or_default() += 1;
        for section in &entry.sections {
            *section_counts.entry(section.clone()).or_default() += 1;
        }
        skipif += usize::from(entry.has_skipif);
        clean += usize::from(entry.has_clean);
        redirect += usize::from(entry.has_redirecttest);
        external += usize::from(entry.has_external_files);
        http += usize::from(entry.uses_http_sections);
        stdin_args += usize::from(entry.uses_stdin_args);
    }

    let mut out = String::new();
    out.push_str("# PHPT Corpus Summary\n\n");
    out.push_str("Generated by `just phpt-index` from the pinned php-src checkout.\n\n");
    out.push_str(&format!("- Total PHPT files: {}\n", entries.len()));
    out.push_str(&format!("- Tests with SKIPIF: {skipif}\n"));
    out.push_str(&format!("- Tests with CLEAN: {clean}\n"));
    out.push_str(&format!("- Tests with REDIRECTTEST: {redirect}\n"));
    out.push_str(&format!("- Tests with external files: {external}\n"));
    out.push_str(&format!("- Tests using HTTP-like sections: {http}\n"));
    out.push_str(&format!("- Tests using STDIN or ARGS: {stdin_args}\n\n"));
    out.push_str("## Module Tags\n\n");
    out.push_str("| Module | PHPT files |\n| --- | ---: |\n");
    for (module, count) in by_module {
        out.push_str(&format!("| {module} | {count} |\n"));
    }
    out.push_str("\n## Expectation Kinds\n\n");
    out.push_str("| Expectation | PHPT files |\n| --- | ---: |\n");
    for (kind, count) in by_expectation {
        out.push_str(&format!("| {kind} | {count} |\n"));
    }
    out.push_str("\n## Section Counts\n\n");
    out.push_str("| Section | PHPT files |\n| --- | ---: |\n");
    for (section, count) in section_counts {
        out.push_str(&format!("| {section} | {count} |\n"));
    }
    out
}

fn json_string_array(values: &[String]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&escape_json(value));
        out.push('"');
    }
    out.push(']');
    out
}

fn collect_recursive(
    php_src: &Path,
    current: &Path,
    entries: &mut Vec<ManifestEntry>,
) -> Result<(), String> {
    let mut children = fs::read_dir(current)
        .map_err(|error| format!("{}: {error}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("{}: {error}", current.display()))?;
    children.sort_by_key(|entry| entry.path());
    for child in children {
        let path = child.path();
        let file_type = child
            .file_type()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if file_type.is_dir() {
            if should_skip_dir(php_src, &path) {
                continue;
            }
            collect_recursive(php_src, &path, entries)?;
        } else if file_type.is_file() {
            let rel = relative_path(php_src, &path)?;
            if let Some(kind) = classify_relevant_file(&rel) {
                let (size, sha256) = hash_file(&path)?;
                entries.push(ManifestEntry {
                    path: rel,
                    size,
                    sha256,
                    kind,
                });
            }
        }
    }
    Ok(())
}

fn should_skip_dir(php_src: &Path, path: &Path) -> bool {
    let Ok(rel) = relative_path(php_src, path) else {
        return true;
    };
    rel == ".git"
        || rel == "autom4te.cache"
        || rel == "modules"
        || rel == "libs"
        || rel.ends_with("/.libs")
        || rel.ends_with("/autom4te.cache")
}

fn classify_relevant_file(rel: &str) -> Option<FileKind> {
    if rel == "run-tests.php" {
        return Some(FileKind::RunTests);
    }
    if rel.ends_with(".phpt") {
        return Some(FileKind::Phpt);
    }
    if !is_core_source_path(rel) {
        return None;
    }
    if rel.ends_with(".c") || rel.ends_with(".cc") {
        if rel.starts_with("Zend/") {
            Some(FileKind::ZendSource)
        } else {
            Some(FileKind::CSource)
        }
    } else if rel.ends_with(".h") {
        Some(FileKind::Header)
    } else if rel.ends_with(".inc")
        || rel.ends_with(".stub.php")
        || rel.ends_with(".php")
        || rel.ends_with(".phtml")
        || rel.ends_with(".exp")
    {
        Some(FileKind::FixtureSupport)
    } else if rel.ends_with(".re")
        || rel.ends_with(".y")
        || rel.ends_with(".l")
        || rel.ends_with(".m4")
        || rel.ends_with(".w32")
        || rel.ends_with(".md")
        || rel.ends_with(".txt")
    {
        Some(FileKind::Other)
    } else {
        None
    }
}

fn is_core_source_path(rel: &str) -> bool {
    rel.starts_with("Zend/")
        || rel.starts_with("main/")
        || rel.starts_with("ext/")
        || rel.starts_with("sapi/cli/")
}

fn is_symbol_source_file(rel: &str) -> bool {
    is_c_or_header(rel) || rel.ends_with(".stub.php")
}

fn is_c_or_header(rel: &str) -> bool {
    rel.ends_with(".c") || rel.ends_with(".h") || rel.ends_with(".cc")
}

fn macro_args(line: &str, macro_name: &str) -> Option<String> {
    let start = line.find(macro_name)?;
    let after_macro = &line[start + macro_name.len()..];
    let open = after_macro.find('(')?;
    let mut depth = 0usize;
    let mut out = String::new();
    for ch in after_macro[open..].chars() {
        if ch == '(' {
            if depth > 0 {
                out.push(ch);
            }
            depth += 1;
        } else if ch == ')' {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(out);
            }
            out.push(ch);
        } else if depth > 0 {
            out.push(ch);
        }
    }
    None
}

fn init_class_entry_name(line: &str) -> Option<String> {
    let args = macro_args(line, "INIT_CLASS_ENTRY")?;
    let first_quote = args.find('"')?;
    let rest = &args[first_quote + 1..];
    let second_quote = rest.find('"')?;
    Some(rest[..second_quote].to_string())
}

fn module_entry_name(line: &str) -> Option<String> {
    let needle = "zend_module_entry ";
    let start = line.find(needle)? + needle.len();
    let rest = &line[start..];
    let name = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();
    name.strip_suffix("_module_entry")
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn module_guess(rel: &str) -> String {
    if rel.starts_with("Zend/") {
        "zend".to_string()
    } else if rel.starts_with("main/") {
        "main".to_string()
    } else if rel.starts_with("sapi/cli/") {
        "sapi.cli".to_string()
    } else if let Some(rest) = rel.strip_prefix("ext/") {
        rest.split('/').next().unwrap_or("ext").to_string()
    } else {
        "unknown".to_string()
    }
}

fn source_stem(rel: &str) -> String {
    rel.rsplit('/')
        .next()
        .unwrap_or(rel)
        .split('.')
        .next()
        .unwrap_or(rel)
        .to_string()
}

fn hash_file(path: &Path) -> Result<(u64, String), String> {
    let mut file = fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}

fn file_fingerprint(path: &Path) -> Result<String, String> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (size, sha256) = hash_file(path)?;
    Ok(format!(
        "{}:{}:{}",
        canonical.to_string_lossy().replace('\\', "/"),
        size,
        sha256
    ))
}

fn default_php_src_dir() -> PathBuf {
    let preferred = PathBuf::from("third_party/php-src-8.5.7");
    if preferred.is_dir() {
        preferred
    } else {
        PathBuf::from("third_party/php-src")
    }
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let rel = path
        .strip_prefix(root)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(rel
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

#[cfg(test)]
mod tests;
