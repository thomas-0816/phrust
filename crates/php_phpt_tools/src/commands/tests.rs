use super::*;

fn test_phpt_entry(path: &str) -> PhptEntry {
    PhptEntry {
        path: path.to_string(),
        title: "test".to_string(),
        sections: vec!["TEST".to_string(), "FILE".to_string(), "EXPECT".to_string()],
        module: "zend".to_string(),
        has_skipif: false,
        has_clean: false,
        has_redirecttest: false,
        has_external_files: false,
        uses_http_sections: false,
        uses_stdin_args: false,
        expectation_kind: "expect".to_string(),
        source_hash: "hash".to_string(),
    }
}

#[test]
fn manifest_json_roundtrips() {
    let entry = ManifestEntry {
        path: "ext/standard/tests/file \"x\".phpt".to_string(),
        size: 12,
        sha256: "abc".to_string(),
        kind: FileKind::Phpt,
    };

    assert_eq!(
        ManifestEntry::from_json_line(&entry.to_json_line()).unwrap(),
        entry
    );
}

#[test]
fn baseline_metadata_json_roundtrips() {
    let metadata = BaselineMetadata {
        schema_version: "phpt-full-baseline-v1".to_string(),
        timestamp: "20260624T125543Z".to_string(),
        corpus_count: 21_556,
        pass_count: 1_056,
        skip_count: 64,
        xfail_count: 8,
        fail_count: 19_973,
        bork_count: 455,
        known_failure_count: 20_428,
        failure_manifest: "tests/phpt/manifests/full-known-failures.jsonl".to_string(),
    };

    assert_eq!(
        BaselineMetadata::from_json(&metadata.to_json()).unwrap(),
        metadata
    );
}

#[test]
fn file_external_reads_relative_local_file() {
    let dir =
        std::env::temp_dir().join(format!("phrust-file-external-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let phpt_path = dir.join("case.phpt");
    let external_path = dir.join("payload.inc");
    fs::write(&phpt_path, "").unwrap();
    fs::write(&external_path, "<?php echo \"external\\n\";").unwrap();
    let sections =
        parse_phpt("--TEST--\nt\n--FILE_EXTERNAL--\npayload.inc\n--EXPECT--\nexternal\n").sections;

    assert_eq!(
        file_body(&sections, &phpt_path).unwrap(),
        Some("<?php echo \"external\\n\";".to_string())
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn file_external_rejects_absolute_or_escaping_paths() {
    let dir = std::env::temp_dir().join(format!(
        "phrust-file-external-reject-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let phpt_path = dir.join("case.phpt");
    fs::write(&phpt_path, "").unwrap();

    let absolute = parse_phpt(&format!(
        "--TEST--\nt\n--FILE_EXTERNAL--\n{}\n--EXPECT--\n",
        phpt_path.display()
    ))
    .sections;
    assert!(
        file_body(&absolute, &phpt_path)
            .unwrap_err()
            .contains("FILE_EXTERNAL path must be a relative local path")
    );

    let escaping =
        parse_phpt("--TEST--\nt\n--FILE_EXTERNAL--\n../payload.inc\n--EXPECT--\n").sections;
    assert!(
        file_body(&escaping, &phpt_path)
            .unwrap_err()
            .contains("FILE_EXTERNAL path must be a relative local path")
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn parses_baseline_report_totals() {
    let report = "# PHPT Full PHPT Baseline\n\nGenerated: `20260624T125543Z`\n\n## Totals\n\n| Outcome | Count |\n| --- | ---: |\n| BORK | 455 |\n| FAIL | 19973 |\n| PASS | 1056 |\n| SKIP | 64 |\n\n## Top Failure Clusters\n";
    let path = std::env::temp_dir().join(format!(
        "phrust-baseline-report-test-{}.md",
        std::process::id()
    ));
    fs::write(&path, report).unwrap();
    let totals = read_baseline_report_totals(&path).unwrap();
    fs::remove_file(&path).unwrap();

    assert_eq!(totals.timestamp, "20260624T125543Z");
    assert_eq!(totals.outcomes.get("BORK"), Some(&455));
    assert_eq!(totals.outcomes.get("FAIL"), Some(&19_973));
    assert_eq!(totals.outcomes.get("PASS"), Some(&1_056));
    assert_eq!(totals.outcomes.get("SKIP"), Some(&64));
}

#[test]
fn parses_run_jobs() {
    assert_eq!(parse_jobs("1").unwrap(), 1);
    assert_eq!(parse_jobs("8").unwrap(), 8);
    assert!(parse_jobs("0").is_err());
    assert!(parse_jobs("many").is_err());
}

#[test]
fn default_run_jobs_uses_bounded_parallelism() {
    let jobs = default_phpt_jobs();
    assert!((1..=8).contains(&jobs));
}

#[test]
fn shell_escape_quotes_cli_paths_for_php_env() {
    assert_eq!(shell_escape("/tmp/php cli"), "'/tmp/php cli'");
    assert_eq!(shell_escape("/tmp/php'cli"), "'/tmp/php'\\''cli'");
}

#[test]
fn parses_run_reuse_results() {
    let options = RunOptions::parse(&[
        "--target".to_string(),
        "target/debug/php-vm".to_string(),
        "--manifest".to_string(),
        "tests/phpt/manifests/runner-smoke.jsonl".to_string(),
        "--reuse-results".to_string(),
        "target/phpt-work/full-runs/previous/results.jsonl".to_string(),
    ])
    .unwrap();

    assert_eq!(
        options.reuse_results.as_deref(),
        Some(Path::new(
            "target/phpt-work/full-runs/previous/results.jsonl"
        ))
    );
    assert!(!options.dev_reuse_pass);
}

#[test]
fn parses_run_dev_reuse_pass() {
    let options = RunOptions::parse(&[
        "--target".to_string(),
        "target/debug/php-vm".to_string(),
        "--manifest".to_string(),
        "tests/phpt/manifests/runner-smoke.jsonl".to_string(),
        "--dev-reuse-pass".to_string(),
    ])
    .unwrap();

    assert!(options.dev_reuse_pass);
}

#[test]
fn parses_run_cleanup_work() {
    let options = RunOptions::parse(&[
        "--target".to_string(),
        "target/debug/php-vm".to_string(),
        "--manifest".to_string(),
        "tests/phpt/manifests/runner-smoke.jsonl".to_string(),
        "--cleanup-work".to_string(),
    ])
    .unwrap();

    assert!(options.cleanup_work);
}

#[test]
fn parses_source_index_options_with_defaults_and_overrides() {
    let php_src = env::temp_dir().join(format!("phrust-phpt-source-{}", std::process::id()));
    fs::create_dir_all(&php_src).unwrap();

    let defaulted =
        SourceOptions::parse(&["--php-src".to_string(), php_src.display().to_string()]).unwrap();
    assert_eq!(defaulted.php_src, php_src);
    assert_eq!(defaulted.manifest, PathBuf::from(DEFAULT_MANIFEST));

    let overridden = SourceOptions::parse(&[
        format!("--php-src={}", php_src.display()),
        "--manifest=target/source.jsonl".to_string(),
    ])
    .unwrap();
    assert_eq!(overridden.manifest, PathBuf::from("target/source.jsonl"));

    fs::remove_dir_all(&php_src).unwrap();
}

#[test]
fn verify_source_skips_missing_host_generated_artifact() {
    let dir = env::temp_dir().join(format!("phrust-phpt-verify-source-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let php_src = dir.join("php-src");
    let manifest = dir.join("php-src-hashes.jsonl");
    fs::create_dir_all(&php_src).unwrap();
    let entry = ManifestEntry {
        path: "ext/opcache/jit/ir/ir_emit_aarch64.h".to_string(),
        size: 1,
        sha256: "unused-on-this-platform".to_string(),
        kind: FileKind::Header,
    };
    fs::write(&manifest, format!("{}\n", entry.to_json_line())).unwrap();
    let args = [
        format!("--php-src={}", php_src.display()),
        format!("--manifest={}", manifest.display()),
    ];
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let status = verify::verify_source(&args, &mut stdout, &mut stderr).unwrap();

    assert_eq!(status, 0);
    assert!(stderr.is_empty());
    assert!(
        String::from_utf8(stdout)
            .unwrap()
            .contains("host-generated artifact is absent on this platform")
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn parses_symbol_and_lookup_options() {
    let php_src = env::temp_dir().join(format!("phrust-phpt-symbol-{}", std::process::id()));
    fs::create_dir_all(&php_src).unwrap();

    let symbols = SymbolOptions::parse(&[
        format!("--php-src={}", php_src.display()),
        "--symbols=target/symbols.jsonl".to_string(),
    ])
    .unwrap();
    assert_eq!(symbols.php_src, php_src);
    assert_eq!(symbols.symbols, PathBuf::from("target/symbols.jsonl"));

    let lookup = LookupOptions::parse(&[
        "--symbols".to_string(),
        "target/symbols.jsonl".to_string(),
        "zif_strlen".to_string(),
    ])
    .unwrap();
    assert_eq!(lookup.symbols, PathBuf::from("target/symbols.jsonl"));
    assert_eq!(lookup.symbol, "zif_strlen");

    fs::remove_dir_all(&php_src).unwrap();
}

#[test]
fn parses_generate_options_with_module_specific_defaults() {
    let php_src = env::temp_dir().join(format!("phrust-phpt-generate-{}", std::process::id()));
    fs::create_dir_all(&php_src).unwrap();
    let reference = env::current_exe().unwrap();

    let options = GenerateOptions::parse(&[
        "--module=zend.functions".to_string(),
        format!("--php-src={}", php_src.display()),
        format!("--reference={}", reference.display()),
        "--smoke-count=4".to_string(),
        "--regression-count".to_string(),
        "5".to_string(),
        "--timeout-seconds=3".to_string(),
    ])
    .unwrap();

    assert_eq!(options.module, "zend.functions");
    assert_eq!(
        options.generated_dir,
        PathBuf::from("tests/phpt/generated/zend.functions")
    );
    assert_eq!(
        options.module_manifest,
        PathBuf::from("tests/phpt/manifests/zend.functions-originals.jsonl")
    );
    assert_eq!(options.smoke_count, 4);
    assert_eq!(options.regression_count, 5);
    assert_eq!(options.timeout, Duration::from_secs(3));

    fs::remove_dir_all(&php_src).unwrap();
}

#[test]
fn normalizes_actual_php_cli_diagnostic_leading_blank_line() {
    assert_eq!(
        normalize_actual_output("\r\nWarning: example\n"),
        "Warning: example"
    );
    assert_eq!(
        normalize_actual_output("\nDeprecated: example\n"),
        "Deprecated: example"
    );
    assert_eq!(normalize_actual_output("\nuser output\n"), "user output");
    assert_eq!(
        normalize_expected_output("\nWarning: example\n"),
        "Warning: example"
    );
    assert_eq!(php_run_tests_trim("\0\t\n out \r\n"), "out");
}

#[test]
fn php_run_tests_ini_defaults_precede_test_ini_overrides() {
    let ini = php_run_tests_ini_args(&[
        ("memory_limit".to_string(), "64M".to_string()),
        ("include_path".to_string(), "fixtures".to_string()),
    ]);

    assert!(ini.contains(&("report_zend_debug".to_string(), "0".to_string())));
    assert!(
        ini.iter()
            .position(|entry| entry == &("memory_limit".to_string(), "128M".to_string()))
            .unwrap()
            < ini
                .iter()
                .position(|entry| entry == &("memory_limit".to_string(), "64M".to_string()))
                .unwrap()
    );
    assert_eq!(
        ini.last(),
        Some(&("include_path".to_string(), "fixtures".to_string()))
    );
}

#[test]
fn phpt_run_result_json_preserves_optional_cache_fields() {
    let result = PhptRunResult::new("Zend/tests/example.phpt", "PASS", "")
        .with_cache_keys("abc".into(), "input-abc".into());
    let parsed = PhptRunResult::from_json_line(&result.to_json_line()).unwrap();

    assert_eq!(parsed.path, "Zend/tests/example.phpt");
    assert_eq!(parsed.cache_key.as_deref(), Some("abc"));
    assert_eq!(parsed.input_cache_key.as_deref(), Some("input-abc"));
    assert_eq!(parsed.cache_status.as_deref(), Some("miss"));
    assert_eq!(parsed.mismatch_category, None);

    let failed = PhptRunResult::new(
        "Zend/tests/fail.phpt",
        "FAIL",
        "target exited with status 255",
    );
    let failed_json = failed.to_json_line();
    assert!(failed_json.contains("\"mismatch_category\":\"RuntimeExitMismatch\""));
    let parsed_failed = PhptRunResult::from_json_line(&failed_json).unwrap();
    assert_eq!(
        parsed_failed.mismatch_category,
        Some(MismatchCategory::RuntimeExitMismatch)
    );

    let legacy = PhptRunResult::from_json_line(
        "{\"path\":\"Zend/tests/legacy.phpt\",\"outcome\":\"FAIL\",\"detail\":\"old\"}",
    )
    .unwrap();
    assert_eq!(legacy.mismatch_category, None);
    assert_eq!(legacy.cache_key, None);
    assert_eq!(legacy.input_cache_key, None);
    assert_eq!(legacy.cache_status, None);
}

#[test]
fn phpt_run_result_escapes_json_control_characters() {
    let detail = "nul=\0 unit-separator=\u{1f} backspace=\u{08} form-feed=\u{0c}";
    let result = PhptRunResult::new("Zend/tests/control.phpt", "FAIL", detail);

    let json = result.to_json_line();
    assert!(!json.chars().any(|ch| ch <= '\u{1f}'));
    assert!(json.contains("nul=\\u0000"));
    assert!(json.contains("unit-separator=\\u001f"));
    assert!(json.contains("backspace=\\b"));
    assert!(json.contains("form-feed=\\f"));

    let parsed = PhptRunResult::from_json_line(&json).unwrap();
    assert_eq!(parsed.detail, detail);
}

#[test]
fn rerun_manifest_keeps_only_non_green_paths() {
    let dir =
        std::env::temp_dir().join(format!("phrust-rerun-manifest-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let results = dir.join("results.jsonl");
    let out = dir.join("rerun.jsonl");
    fs::write(
        &results,
        [
            PhptRunResult::new("a.phpt", "PASS", "").to_json_line(),
            PhptRunResult::new("b.phpt", "FAIL", "x").to_json_line(),
            PhptRunResult::new("c.phpt", "BORK", "x").to_json_line(),
            PhptRunResult::new("b.phpt", "FAIL", "x").to_json_line(),
            PhptRunResult::new("d.phpt", "SKIP", "x").to_json_line(),
        ]
        .join("\n"),
    )
    .unwrap();

    let status = run::rerun_manifest(
        &[
            "--results".to_string(),
            results.display().to_string(),
            "--out".to_string(),
            out.display().to_string(),
        ],
        &mut Vec::new(),
    )
    .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        fs::read_to_string(&out).unwrap(),
        "{\"path\":\"b.phpt\"}\n{\"path\":\"c.phpt\"}\n"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn triage_keeps_array_builtins_separate_from_reference_core() {
    assert_eq!(
        plan_module_for_path(
            "ext/standard/tests/array/array_chunk.phpt",
            "standard.arrays",
            None
        ),
        "standard.arrays"
    );
    assert_eq!(
        plan_module_for_path("Zend/tests/foreach/foreach_by_ref.phpt", "zend", None),
        "arrays.references"
    );
    assert_eq!(
        plan_module_for_path("ext/spl/tests/arrayAccess_001.phpt", "spl", None),
        "arrays.references"
    );
}

#[test]
fn triage_routes_function_and_callable_paths_to_zend_functions() {
    assert_eq!(
        plan_module_for_path(
            "Zend/tests/call_user_functions/call_user_func_001.phpt",
            "zend",
            None
        ),
        "zend.functions"
    );
    assert_eq!(
        plan_module_for_path("Zend/tests/closures/closure_001.phpt", "zend", None),
        "zend.functions"
    );
    assert_eq!(
        plan_module_for_path(
            "ext/standard/tests/general_functions/call_user_func.phpt",
            "standard",
            None
        ),
        "zend.functions"
    );
}

#[test]
fn zend_functions_gate_keeps_core_zend_callable_paths() {
    let entry = test_phpt_entry("Zend/tests/call_user_functions/call_user_func_001.phpt");
    assert!(is_module_gate_candidate_for_module(
        "zend.functions",
        &entry
    ));

    let entry = test_phpt_entry("Zend/tests/closures/closure_001.phpt");
    assert!(is_module_gate_candidate_for_module(
        "zend.functions",
        &entry
    ));

    let entry = test_phpt_entry("Zend/tests/arrow_functions/001.phpt");
    assert!(is_module_gate_candidate_for_module(
        "zend.functions",
        &entry
    ));
}

#[test]
fn zend_functions_gate_excludes_extension_and_general_function_paths() {
    let standard_entry =
        test_phpt_entry("ext/standard/tests/general_functions/call_user_func.phpt");
    assert!(!is_module_gate_candidate_for_module(
        "zend.functions",
        &standard_entry
    ));

    let mut extension_entry = test_phpt_entry("ext/xsl/tests/XSLTProcessor_callables.phpt");
    extension_entry.sections.push("EXTENSIONS".to_string());
    assert!(!is_module_gate_candidate_for_module(
        "zend.functions",
        &extension_entry
    ));

    assert!(is_module_gate_candidate_for_module(
        "standard",
        &standard_entry
    ));
}

#[test]
fn zend_functions_gate_excludes_reference_unstable_trace_paths() {
    for path in [
        "Zend/tests/function_arguments/sensitive_parameter.phpt",
        "Zend/tests/function_arguments/function_arguments_001.phpt",
        "Zend/tests/function_arguments/function_arguments_002.phpt",
        "Zend/tests/closures/closure_005.phpt",
        "Zend/tests/closures/closure_018.phpt",
        "Zend/tests/closures/closure_019.phpt",
        "Zend/tests/closures/closure_022.phpt",
        "Zend/tests/closures/closure_033.phpt",
        "Zend/tests/closures/closure_065.phpt",
    ] {
        assert!(
            !is_module_gate_candidate_for_module("zend.functions", &test_phpt_entry(path)),
            "{path}"
        );
    }
}

#[test]
fn triage_classifies_common_bork_subclasses() {
    assert_eq!(
        classify_bork(Some("unsupported section --POST--")),
        "unsupported-section"
    );
    assert_eq!(
        classify_bork(Some("unsupported PHPT section `PHPDBG`")),
        "missing-target-cli-capability"
    );
    assert_eq!(
        classify_bork(Some("FILE_EXTERNAL is not supported")),
        "unsupported-file-external"
    );
    assert_eq!(
        classify_bork(Some("test.phpt: stream did not contain valid UTF-8")),
        "malformed-or-non-utf8-phpt"
    );
    assert_eq!(
        classify_bork(Some("malformed PHPT: missing --FILE--")),
        "malformed-or-incomplete-phpt"
    );
    assert_eq!(
        classify_bork(Some("STDIN and ARGS are unsupported")),
        "unsupported-runner-io"
    );
}

#[test]
fn triage_renders_extension_policy_without_hiding_counts() {
    let mut triage = PhptTriage::default();
    triage.raw_modules.insert(
        "phar".to_string(),
        ModuleTriageStats {
            corpus_count: 3,
            pass_count: 1,
            skip_count: 0,
            fail_count: 2,
            bork_count: 0,
            known_failure_count: 2,
            failure_clusters: BTreeMap::from([("runtime-output-mismatch".to_string(), 2)]),
            ..ModuleTriageStats::default()
        },
    );
    triage.raw_modules.insert(
        "pdo".to_string(),
        ModuleTriageStats {
            corpus_count: 2,
            pass_count: 0,
            skip_count: 1,
            fail_count: 1,
            bork_count: 0,
            known_failure_count: 2,
            ..ModuleTriageStats::default()
        },
    );
    triage.raw_modules.insert(
        "pdo_sqlite".to_string(),
        ModuleTriageStats {
            corpus_count: 4,
            pass_count: 0,
            skip_count: 1,
            fail_count: 3,
            bork_count: 0,
            known_failure_count: 4,
            ..ModuleTriageStats::default()
        },
    );

    let report = render_extension_policy_report(
        &BaselineMetadata {
            schema_version: "phpt-full-baseline-v1".to_string(),
            timestamp: "20260624T210848Z".to_string(),
            corpus_count: 8,
            pass_count: 1,
            skip_count: 1,
            xfail_count: 0,
            fail_count: 6,
            bork_count: 0,
            known_failure_count: 7,
            failure_manifest: "tests/phpt/manifests/full-known-failures.jsonl".to_string(),
        },
        &triage,
    );

    assert!(report.contains("Extension PHPTs remain in the corpus"));
    assert!(
            report.contains(
                "| phar | required-composer | 3 | 1 | 0 | 2 | 0 | `runtime-output-mismatch` 2 | no | yes | yes | real-implementation-required |"
            )
        );
    assert!(report.contains(
        "| pdo | optional | 2 | 0 | 1 | 1 | 0 | none | no | no | yes | partial-implementation |"
    ));
    assert!(report.contains(
            "| pdo_sqlite | required-framework | 4 | 0 | 1 | 3 | 0 | none | no | no | yes | real-implementation-required |"
        ));
}

#[test]
fn triage_applies_committed_baseline_module_counts() {
    let counts = vec![BaselineModuleCount::from_json_line(
            "{\"kind\":\"plan\",\"module\":\"standard.arrays\",\"corpus_count\":2,\"pass_count\":1,\"skip_count\":0,\"fail_count\":1,\"bork_count\":0,\"known_failure_count\":1}",
        )
        .unwrap()];
    let mut triage = PhptTriage::default();
    triage
        .modules
        .insert("standard.arrays".to_string(), ModuleTriageStats::default());

    apply_baseline_module_counts(&mut triage, &counts);

    let stats = triage.modules.get("standard.arrays").unwrap();
    assert_eq!(stats.corpus_count, 2);
    assert_eq!(stats.pass_count, 1);
    assert_eq!(stats.fail_count, 1);
    assert_eq!(triage.count_source, "baseline-module-counts");
    assert!(triage.has_result_counts);
}

#[test]
fn baseline_module_counts_render_plan_raw_and_bork_rows() {
    let mut triage = PhptTriage::default();
    triage.modules.insert(
        "standard.arrays".to_string(),
        ModuleTriageStats {
            corpus_count: 2,
            pass_count: 1,
            fail_count: 1,
            known_failure_count: 1,
            ..ModuleTriageStats::default()
        },
    );
    triage.raw_modules.insert(
        "standard".to_string(),
        ModuleTriageStats {
            corpus_count: 3,
            pass_count: 1,
            skip_count: 1,
            fail_count: 1,
            known_failure_count: 1,
            ..ModuleTriageStats::default()
        },
    );
    triage
        .bork_subclasses
        .insert("unsupported-section".to_string(), 4);

    let rendered = render_baseline_module_counts(&triage);

    assert!(
        rendered.contains("\"kind\":\"plan\",\"module\":\"standard.arrays\",\"corpus_count\":2")
    );
    assert!(rendered.contains("\"kind\":\"raw\",\"module\":\"standard\",\"corpus_count\":3"));
    assert!(rendered.contains(
        "\"kind\":\"bork_subclass\",\"module\":\"unsupported-section\",\"corpus_count\":0"
    ));
}

#[test]
fn known_gap_catalog_renders_required_hard_rule_fields() {
    let failures = vec![KnownFailure {
        path: "Zend/tests/basic/002.phpt".to_string(),
        module_tag: "zend".to_string(),
        outcome: "FAIL".to_string(),
        failure_fingerprint: "abc".to_string(),
        primary_missing_feature_guess: "runtime-output-mismatch".to_string(),
        owner_module: "zend.basic".to_string(),
        first_seen_timestamp: "20260624T125543Z".to_string(),
    }];
    let rows = build_known_gap_rows(&failures, &[]);
    let rendered = render_known_gap_catalog(&rows);
    let markdown = render_known_gap_report(
        &BaselineMetadata {
            schema_version: "phpt-full-baseline-v1".to_string(),
            timestamp: "20260624T125543Z".to_string(),
            corpus_count: 1,
            pass_count: 0,
            skip_count: 0,
            xfail_count: 0,
            fail_count: 1,
            bork_count: 0,
            known_failure_count: 1,
            failure_manifest: "tests/phpt/manifests/full-known-failures.jsonl".to_string(),
        },
        &rows,
    );

    assert!(rendered.contains("\"schema_version\":\"phpt-known-gap-v1\""));
    assert!(rendered.contains("\"id\":\"runtime-output-mismatch\""));
    assert!(rendered.contains("\"baseline_count\":1"));
    assert!(rendered.contains("\"reference_behavior\":\""));
    assert!(rendered.contains("\"current_rust_behavior\":\""));
    assert!(rendered.contains("\"fixture_or_phpt_example\":\"Zend/tests/basic/002.phpt\""));
    assert!(rendered.contains("\"planned_solution_layer\":\""));
    assert!(markdown.contains("| `runtime-output-mismatch` | 1 |"));
}

#[test]
fn known_gap_catalog_verifier_rejects_missing_ids_and_count_drift() {
    let failures = vec![KnownFailure {
        path: "Zend/tests/basic/002.phpt".to_string(),
        module_tag: "zend".to_string(),
        outcome: "FAIL".to_string(),
        failure_fingerprint: "abc".to_string(),
        primary_missing_feature_guess: "runtime-output-mismatch".to_string(),
        owner_module: "zend.basic".to_string(),
        first_seen_timestamp: "20260624T125543Z".to_string(),
    }];
    let module_counts = vec![BaselineModuleCount::from_json_line(
            "{\"kind\":\"bork_subclass\",\"module\":\"unsupported-section\",\"corpus_count\":0,\"pass_count\":0,\"skip_count\":0,\"fail_count\":0,\"bork_count\":2,\"known_failure_count\":2}",
        )
        .unwrap()];
    let mut catalog = build_known_gap_rows(&failures, &module_counts);
    catalog.retain(|entry| entry.id != "unsupported-section");
    if let Some(entry) = catalog
        .iter_mut()
        .find(|entry| entry.id == "runtime-output-mismatch")
    {
        entry.baseline_count = 99;
    }
    let mut errors = Vec::new();

    verify_known_gap_catalog(
        &catalog,
        &failures,
        &module_counts,
        &BaselineMetadata {
            schema_version: "phpt-full-baseline-v1".to_string(),
            timestamp: "20260624T125543Z".to_string(),
            corpus_count: 3,
            pass_count: 0,
            skip_count: 0,
            xfail_count: 0,
            fail_count: 1,
            bork_count: 2,
            known_failure_count: 3,
            failure_manifest: "tests/phpt/manifests/full-known-failures.jsonl".to_string(),
        },
        &mut errors,
    );

    assert!(
        errors
            .iter()
            .any(|error| error.contains("unsupported-section"))
    );
    assert!(errors.iter().any(|error| {
        error.contains("runtime-output-mismatch") && error.contains("baseline_count mismatch")
    }));
}

#[test]
fn triage_preserves_curated_generated_selected_manifests() {
    let dir = env::temp_dir().join(format!("phrust-curated-selected-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let manifest = dir.join("filesystem.streams.selected.jsonl");
    let existing = "{\"path\":\"tests/phpt/generated/filesystem.streams/local-file-roundtrip.phpt\",\"module\":\"filesystem.streams\",\"kind\":\"generated\"}\n";
    fs::write(&manifest, existing).unwrap();

    let stats = ModuleTriageStats {
        selected_paths: vec!["ext/standard/tests/file/new-broad-path.phpt".to_string()],
        ..ModuleTriageStats::default()
    };

    assert!(has_curated_generated_manifest(&manifest));
    assert_eq!(render_selected_manifest(&manifest, &stats, 200), existing);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn triage_refreshes_plan_counts_for_curated_modules() {
    let dir = env::temp_dir().join(format!("phrust-curated-plan-{}", std::process::id()));
    let modules_dir = dir.join("docs");
    let manifests_dir = dir.join("manifests");
    fs::create_dir_all(&manifests_dir).unwrap();
    let selected = manifests_dir.join("zend.basic.selected.jsonl");
    let curated = "{\"path\":\"tests/phpt/generated/zend.basic/regression-example.phpt\",\"module\":\"zend.basic\",\"kind\":\"regression\"}\n";
    fs::write(&selected, curated).unwrap();

    let mut triage = PhptTriage::default();
    triage.modules.insert(
        "zend.basic".to_string(),
        ModuleTriageStats {
            corpus_count: 3509,
            pass_count: 987,
            skip_count: 89,
            fail_count: 2432,
            known_failure_count: 2432,
            ..ModuleTriageStats::default()
        },
    );
    let options = TriageOptions {
        corpus: dir.join("corpus.jsonl"),
        known_failures: dir.join("failures.jsonl"),
        metadata: dir.join("metadata.json"),
        module_counts: dir.join("module-counts.jsonl"),
        results: None,
        report: dir.join("report.md"),
        extension_policy_report: dir.join("extension-policy.md"),
        known_gap_report: dir.join("known-gaps.md"),
        known_gap_catalog: dir.join("known-gap-catalog.jsonl"),
        priority: dir.join("priority.json"),
        modules_dir: modules_dir.clone(),
        module_manifests_dir: manifests_dir.clone(),
        selected_limit: 200,
    };
    let metadata = BaselineMetadata {
        schema_version: "phpt-full-baseline-v1".to_string(),
        timestamp: "20260712T101615Z".to_string(),
        corpus_count: 3509,
        pass_count: 987,
        skip_count: 89,
        xfail_count: 1,
        fail_count: 2432,
        bork_count: 0,
        known_failure_count: 2432,
        failure_manifest: "failures.jsonl".to_string(),
    };

    write_triage_outputs(&options, &metadata, &triage, &[]).unwrap();

    let plan = fs::read_to_string(manifests_dir.join("zend.basic.json")).unwrap();
    assert!(
        plan.contains("\"pass_count\":987") && plan.contains("\"fail_count\":2432"),
        "curated module plan must track current baseline counts: {plan}"
    );
    assert_eq!(
        fs::read_to_string(&selected).unwrap(),
        curated,
        "curated selected manifest must be preserved verbatim"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn phpt_runner_reads_non_utf8_sources_lossily() {
    let path = env::temp_dir().join(format!(
        "phrust-non-utf8-phpt-{}-{}.phpt",
        std::process::id(),
        "locale"
    ));
    fs::write(
        &path,
        b"--TEST--\nlocale \xE9\n--FILE--\n<?php echo 'ok';\n--EXPECT--\nok\n",
    )
    .unwrap();
    let (source, has_invalid_utf8) = run::read_phpt_source_lossy_with_invalid_utf8(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(has_invalid_utf8);
    assert!(source.contains("--FILE--"));
    assert!(source.contains('\u{fffd}'));
}

#[test]
fn capture_stdio_defaults_to_all_streams() {
    let sections = parse_phpt("--TEST--\nt\n--FILE--\n<?php\n--EXPECT--\n").sections;
    assert_eq!(capture_stdio(&sections), CaptureStdio::ALL);
    assert_eq!(stdin_from_sections(&sections, CaptureStdio::ALL), Some(""));
}

#[test]
fn capture_stdio_parses_stream_tokens_case_insensitively() {
    let sections =
        parse_phpt("--TEST--\nt\n--CAPTURE_STDIO--\nstdin, stderr\n--FILE--\n<?php\n--EXPECT--\n")
            .sections;
    assert_eq!(
        capture_stdio(&sections),
        CaptureStdio {
            stdin: true,
            stdout: false,
            stderr: true
        }
    );
}

#[test]
fn captured_output_respects_capture_stdio_stream_mask() {
    let output = ProcessOutput {
        status: 0,
        stdout: "out\n".to_string(),
        stderr: "err\n".to_string(),
    };
    assert_eq!(
        captured_output(
            &output,
            CaptureStdio {
                stdin: true,
                stdout: true,
                stderr: true
            }
        ),
        "out\nerr\n"
    );
    assert_eq!(
        captured_output(
            &output,
            CaptureStdio {
                stdin: true,
                stdout: false,
                stderr: true
            }
        ),
        "err\n"
    );
    assert_eq!(
        captured_output(
            &output,
            CaptureStdio {
                stdin: true,
                stdout: false,
                stderr: false
            }
        ),
        ""
    );
}

#[test]
fn capture_stdio_skipif_env_only_applies_without_tty() {
    let sections = parse_phpt(
        "--TEST--\nt\n--SKIPIF--\n<?php\n--CAPTURE_STDIO--\nSTDOUT\n--FILE--\n<?php\n--EXPECT--\n",
    )
    .sections;
    assert_eq!(
        skipif_env_args_for_stdio(&sections, false),
        vec![("SKIP_IO_CAPTURE_TESTS".to_string(), "1".to_string())]
    );
    assert!(skipif_env_args_for_stdio(&sections, true).is_empty());
}

#[test]
fn capture_stdio_skipif_env_does_not_apply_without_capture_section() {
    let sections =
        parse_phpt("--TEST--\nt\n--SKIPIF--\n<?php\n--FILE--\n<?php\n--EXPECT--\n").sections;
    assert!(skipif_env_args_for_stdio(&sections, false).is_empty());
}

#[test]
fn env_args_trim_urlencoded_post_section_trailing_newline() {
    let sections =
        parse_phpt("--TEST--\nt\n--POST--\nd=4&e=5\n--FILE--\n<?php\n--EXPECT--\n").sections;

    assert_eq!(
        env_args(&sections),
        vec![
            ("REQUEST_METHOD".to_string(), "POST".to_string()),
            ("PHPT_REQUEST_BODY".to_string(), "d=4&e=5".to_string())
        ]
    );
}

#[test]
fn target_sapi_sections_skip_without_sapi_binary() {
    let cgi_sections = parse_phpt("--TEST--\nt\n--CGI--\n--FILE--\n<?php\n--EXPECT--\n").sections;
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/example.phpt",
            TargetMode::PhpCli,
            &cgi_sections,
            ""
        ),
        Some("CGI not available")
    );

    let phpdbg_sections =
        parse_phpt("--TEST--\nt\n--PHPDBG--\nr\n--FILE--\n<?php\n--EXPECT--\n").sections;
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/example.phpt",
            TargetMode::PhpCli,
            &phpdbg_sections,
            ""
        ),
        Some("phpdbg not available")
    );

    let gzip_sections =
        parse_phpt("--TEST--\nt\n--GZIP_POST--\na=1\n--FILE--\n<?php\n--EXPECT--\n").sections;
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/example.phpt",
            TargetMode::PhpCli,
            &gzip_sections,
            ""
        ),
        Some("CGI not available")
    );

    let deflate_sections =
        parse_phpt("--TEST--\nt\n--DEFLATE_POST--\na=1\n--FILE--\n<?php\n--EXPECT--\n").sections;
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/example.phpt",
            TargetMode::PhpCli,
            &deflate_sections,
            ""
        ),
        Some("CGI not available")
    );
}

#[test]
fn target_php_cli_mode_skips_sapi_paths_only() {
    let sections = parse_phpt("--TEST--\nt\n--FILE--\n<?php\n--EXPECT--\n").sections;
    assert_eq!(
        target_cli_skip_reason(
            "sapi/phpdbg/tests/example.phpt",
            TargetMode::PhpCli,
            &sections,
            ""
        ),
        Some("phpdbg not available in php-cli target mode")
    );
    assert_eq!(
        target_cli_skip_reason(
            "sapi/fpm/tests/example.phpt",
            TargetMode::PhpCli,
            &sections,
            ""
        ),
        Some("FPM not available in php-cli target mode")
    );
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cgi/tests/example.phpt",
            TargetMode::PhpCli,
            &sections,
            ""
        ),
        Some("CGI not available in php-cli target mode")
    );
    assert_eq!(
        target_cli_skip_reason(
            "sapi/apache2handler/tests/example.phpt",
            TargetMode::PhpCli,
            &sections,
            ""
        ),
        Some("Apache module not available in php-cli target mode")
    );
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/example.phpt",
            TargetMode::PhpCli,
            &sections,
            ""
        ),
        None
    );
}

#[test]
fn target_php_cli_mode_skips_concrete_non_scope_cli_features() {
    let sections = parse_phpt("--TEST--\nt\n--FILE--\n<?php\n--EXPECT--\n").sections;
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/php_cli_server_001.phpt",
            TargetMode::PhpCli,
            &sections,
            "<?php php_cli_server_start('echo 1;');"
        ),
        Some("CLI built-in web server not available in php-cli target mode")
    );
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/cli_set_process_title_basic.phpt",
            TargetMode::PhpCli,
            &sections,
            "<?php cli_set_process_title('x');"
        ),
        Some("CLI process-control APIs not available in php-cli target mode")
    );
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/gh8827-001.phpt",
            TargetMode::PhpCli,
            &sections,
            "<?php fclose(STDOUT); file_put_contents('php://fd/1', 'x');"
        ),
        Some("CLI stdio descriptor rebinding not available in php-cli target mode")
    );
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/bug71624.phpt",
            TargetMode::PhpCli,
            &sections,
            "<?php shell_exec(\"cat input | php -n -R 'echo $argn;'\");"
        ),
        Some("CLI -R line-processing mode not available in php-cli target mode")
    );
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/gh21901.phpt",
            TargetMode::PhpCli,
            &sections,
            "<?php echo shell_exec($php . ' -n --ini');"
        ),
        Some("CLI --ini introspection not available in php-cli target mode")
    );
    assert_eq!(
        target_cli_skip_reason(
            "ext/standard/tests/general_functions/passthru_basic.phpt",
            TargetMode::PhpCli,
            &sections,
            "<?php passthru('echo x');"
        ),
        Some("process-control functions are outside the php-cli target contract")
    );
    assert_eq!(
        target_cli_skip_reason(
            "tests/phpt/generated/zlib/gzip-stream-helpers.phpt",
            TargetMode::PhpCli,
            &sections,
            "<?php gzpassthru($handle);"
        ),
        None
    );
    assert_eq!(
        target_cli_skip_reason(
            "sapi/cli/tests/bug77561.phpt",
            TargetMode::PhpCli,
            &sections,
            "<?php require __DIR__ . '/bug77561.inc';"
        ),
        Some("include-path expression runtime gap outside the php-cli target contract")
    );
    assert_eq!(
        target_cli_skip_reason(
            "ext/standard/tests/hrtime/hrtime.phpt",
            TargetMode::PhpCli,
            &sections,
            "--FLAKY--\n<?php hrtime(true); for ($i = 0; $i < 1024*1024; $i++);"
        ),
        Some("flaky hrtime busy-loop exceeds target VM step limit")
    );
}

#[test]
fn target_php_cli_mode_skips_unavailable_process_probe() {
    let sections = parse_phpt(
        "--TEST--\nt\n--SKIPIF--\n<?php exec('probe', $output, $status);\n--FILE--\n<?php echo 'runnable';\n--EXPECT--\nrunnable\n",
    )
    .sections;
    assert_eq!(
        target_cli_skip_reason(
            "ext/standard/tests/network/probe.phpt",
            TargetMode::PhpCli,
            &sections,
            "<?php echo 'runnable';"
        ),
        Some("SKIPIF process-control probe is outside the php-cli target contract")
    );
}

#[test]
fn target_vm_mode_does_not_skip_by_sapi_path() {
    let sections = parse_phpt("--TEST--\nt\n--FILE--\n<?php\n--EXPECT--\n").sections;
    assert_eq!(
        target_cli_skip_reason(
            "sapi/fpm/tests/example.phpt",
            TargetMode::PhpVm,
            &sections,
            "<?php php_cli_server_start('echo 1;');"
        ),
        None
    );
}

#[test]
fn required_extensions_parse_lines_and_commas() {
    let sections = parse_phpt(
            "--TEST--\nt\n--EXTENSIONS--\nzend_test, session\n# comment\n; another\nstandard\n--FILE--\n<?php\n--EXPECT--\n",
        )
        .sections;
    assert_eq!(
        required_extensions(&sections),
        vec![
            "zend_test".to_string(),
            "session".to_string(),
            "standard".to_string()
        ]
    );
}

#[test]
fn extension_check_source_escapes_single_quoted_literals() {
    let source = extension_check_source(&["weird\\ext'name".to_string()]);
    assert!(source.contains("'weird\\\\ext\\'name'"));
    assert!(source.contains("extension_loaded($extension)"));
}

#[test]
fn phpt_execution_filename_uses_original_basename() {
    assert_eq!(
        phpt_execution_filename(Path::new("Zend/tests/unset/this_in_unset.phpt")),
        "this_in_unset.php"
    );
}

#[test]
fn copy_support_files_mirrors_active_phpt_but_not_sibling_phpts() {
    let base = env::temp_dir().join(format!("phrust-phpt-support-copy-{}", std::process::id()));
    let source = base.join("source");
    let work = base.join("work");
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&source).expect("source dir");
    fs::create_dir_all(&work).expect("work dir");
    fs::write(source.join("003.phpt"), b"--TEST--\nactive\n").expect("active phpt");
    fs::write(source.join("004.phpt"), b"--TEST--\nsibling\n").expect("sibling phpt");
    fs::write(source.join("payload.inc"), b"payload").expect("payload");

    copy_phpt_support_files(&source.join("003.phpt"), &work).expect("copy support files");

    assert_eq!(
        fs::read_to_string(work.join("003.phpt")).expect("active copied"),
        "--TEST--\nactive\n"
    );
    assert!(!work.join("004.phpt").exists());
    assert_eq!(
        fs::read_to_string(work.join("payload.inc")).expect("payload copied"),
        "payload"
    );
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn classifies_required_paths() {
    assert_eq!(
        classify_relevant_file("run-tests.php"),
        Some(FileKind::RunTests)
    );
    assert_eq!(
        classify_relevant_file("Zend/zend_execute.c"),
        Some(FileKind::ZendSource)
    );
    assert_eq!(
        classify_relevant_file("main/main.c"),
        Some(FileKind::CSource)
    );
    assert_eq!(
        classify_relevant_file("ext/standard/php_string.h"),
        Some(FileKind::Header)
    );
    assert_eq!(
        classify_relevant_file("tests/basic/001.phpt"),
        Some(FileKind::Phpt)
    );
    assert_eq!(classify_relevant_file("README.md"), None);
}

#[test]
fn extracts_common_symbol_macros() {
    assert_eq!(
        macro_args("PHP_FUNCTION(strlen)", "PHP_FUNCTION").unwrap(),
        "strlen"
    );
    assert_eq!(
        macro_args("PHP_METHOD(DateTime, __construct)", "PHP_METHOD").unwrap(),
        "DateTime, __construct"
    );
    assert_eq!(
        init_class_entry_name("INIT_CLASS_ENTRY(ce, \"ArrayObject\", methods)").unwrap(),
        "ArrayObject"
    );
    assert_eq!(
        module_entry_name("zend_module_entry json_module_entry = {").unwrap(),
        "json"
    );
}

#[test]
fn classifies_known_failure_evolution_as_related_changes() {
    let previous = PhptRunResult::new(
        "Zend/tests/example.phpt",
        "FAIL",
        format!("target exited with status 2; stderr={LITERAL_KIND_UNSUPPORTED_DIAGNOSTIC}"),
    );
    let current = PhptRunResult::new(
        "Zend/tests/example.phpt",
        "FAIL",
        "target exited with status 3; stderr=runtime_error: undefined function getdate",
    );
    let unrelated_path =
        PhptRunResult::new("Zend/tests/other.phpt", "FAIL", current.detail.clone());
    let passing_current = PhptRunResult::new(previous.path.clone(), "PASS", String::new());
    let previous_advanced_parameter = PhptRunResult::new(
        previous.path.clone(),
        "FAIL",
        format!("target exited with status 2; stderr={ADVANCED_PARAMETER_UNFOLDED_DIAGNOSTIC}"),
    );
    let previous_output = PhptRunResult::new(
        previous.path.clone(),
        "FAIL",
        "output did not match expectation first_mismatch=Some(100)",
    );
    let current_output = PhptRunResult::new(
        previous.path.clone(),
        "FAIL",
        "output did not match expectation first_mismatch=Some(200)",
    );
    let previous_step_limit = PhptRunResult::new(
        previous.path.clone(),
        "FAIL",
        "target exited with status 3; stderr=runtime_error: VM step limit exceeded",
    );
    let current_timeout = PhptRunResult::new(
        previous.path.clone(),
        "FAIL",
        "target exited with status 124; stderr=PHPT_TIMEOUT after 30s",
    );
    let previous_bork = PhptRunResult::new(
        previous.path.clone(),
        "BORK",
        "unsupported PHPT section `FLAKY`",
    );
    let current_after_runner_support = PhptRunResult::new(
        previous.path.clone(),
        "FAIL",
        "output did not match expectation",
    );
    let previous_target_exit = PhptRunResult::new(
        previous.path.clone(),
        "FAIL",
        "target exited with status 3; stderr=/tmp/repo/target/phpt-work/full-runs/a/work/target/case-1-2/test.php: runtime_error: undefined function highlight_string",
    );
    let current_expectation_then_exit = PhptRunResult::new(
        previous.path.clone(),
        "FAIL",
        "output did not match expectation first_mismatch=Some(0) expected=`done` actual=``; target exited with status 255; stderr=/tmp/repo/target/phpt-work/full-runs/b/work/target/case-9-8/test.php: runtime_error: undefined function highlight_string",
    );
    let current_changed_stderr = PhptRunResult::new(
        previous.path.clone(),
        "FAIL",
        "output did not match expectation first_mismatch=Some(0) expected=`done` actual=``; target exited with status 255; stderr=/tmp/repo/target/phpt-work/full-runs/b/work/target/case-9-8/test.php: runtime_error: undefined function different",
    );

    assert!(is_related_known_failure_evolution(
        Some(&previous),
        Some(&current)
    ));
    assert!(is_related_known_failure_evolution(
        Some(&previous_advanced_parameter),
        Some(&current)
    ));
    assert!(is_related_known_failure_evolution(
        Some(&previous_output),
        Some(&current_output)
    ));
    assert!(is_related_known_failure_evolution(
        Some(&previous_step_limit),
        Some(&current_timeout)
    ));
    assert!(is_related_known_failure_evolution(
        Some(&previous_bork),
        Some(&current_after_runner_support)
    ));
    assert!(is_related_known_failure_evolution(
        Some(&previous_target_exit),
        Some(&current_expectation_then_exit)
    ));
    assert!(!is_related_known_failure_evolution(
        Some(&previous_target_exit),
        Some(&current_changed_stderr)
    ));
    assert!(!is_related_known_failure_evolution(
        Some(&previous),
        Some(&unrelated_path)
    ));
    assert!(!is_related_known_failure_evolution(
        Some(&previous),
        Some(&passing_current)
    ));
}

#[test]
fn normalizes_run_specific_paths_for_failure_fingerprints() {
    let left = "stderr=/tmp/repo/target/phpt-work/full-runs/a/work/target/case-1-2/test.php: E\nthread 'main' (123) panicked";
    let right = "stderr=/tmp/repo/target/phpt-work/full-runs/b/work/target/case-9-8/test.php: E\nthread 'main' (456) panicked";

    assert_eq!(
        normalize_failure_detail_for_fingerprint(left),
        normalize_failure_detail_for_fingerprint(right)
    );

    let isolated = "stderr=/tmp/repo/target/phpt-work-one-worker/full-runs/c/work/target/case-3-4/test.php: E\nthread 'main' (789) panicked";

    assert_eq!(
        normalize_failure_detail_for_fingerprint(left),
        normalize_failure_detail_for_fingerprint(isolated)
    );

    let left = "thread 'main' (123) panicked at crates/php_vm/src/vm.rs:7824:37:\n             at /rustc/hash/library/std/src/panicking.rs:689:5";
    let right = "thread 'main' (456) panicked at crates/php_vm/src/vm.rs:7827:37:\n             at /rustc/hash/library/std/src/panicking.rs:701:5";

    assert_eq!(
        normalize_failure_detail_for_fingerprint(left),
        normalize_failure_detail_for_fingerprint(right)
    );

    let left = "message=\"/tmp/repo/target/phpt-work/full-runs/a/work/target/case-1-2: Is a directory\" actual=`/tmp/repo/target/phpt-work/full-runs/a/wor`";
    let right = "message=\"/tmp/repo/target/phpt-work/full-runs/b/work/target/case-9-8: Is a directory\" actual=`/tmp/repo/target/phpt-work/full-runs/b/wor`";

    assert_eq!(
        normalize_failure_detail_for_fingerprint(left),
        normalize_failure_detail_for_fingerprint(right)
    );

    let left = "output did not match expectation first_mismatch=Some(16) expected=`bool(true)` actual=`int(1782285176)`";
    let right = "output did not match expectation first_mismatch=Some(16) expected=`bool(true)` actual=`int(1782289747)`";

    assert_eq!(
        normalize_failure_detail_for_fingerprint(left),
        normalize_failure_detail_for_fingerprint(right)
    );

    assert_eq!(
        normalize_failure_detail_for_fingerprint(
            "target exited; stderr=PHPT_TIMEOUT after 10s\npartial"
        ),
        "PHPT_TIMEOUT"
    );

    let left = "stderr=<phpt-test.php>:1: E_PHP_IR_TRAIT_METHOD_CONFLICT: method b\n<phpt-test.php>:1: E_PHP_IR_TRAIT_METHOD_CONFLICT: method a";
    let right = "stderr=<phpt-test.php>:1: E_PHP_IR_TRAIT_METHOD_CONFLICT: method a\n<phpt-test.php>:1: E_PHP_IR_TRAIT_METHOD_CONFLICT: method b";

    assert_eq!(
        normalize_failure_detail_for_fingerprint(left),
        normalize_failure_detail_for_fingerprint(right)
    );
}
