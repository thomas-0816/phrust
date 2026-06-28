use super::*;

pub(crate) fn run_phpt_manifest<W: Write>(args: &[String], stdout: &mut W) -> Result<i32, String> {
    let options = RunOptions::parse(args)?;
    if !options.target.is_file() {
        return Err(format!(
            "target PHP is not executable: {}",
            options.target.display()
        ));
    }
    let paths = read_manifest_paths(&options.manifest)?;
    if paths.is_empty() {
        return Err(format!(
            "{}: manifest contains no paths",
            options.manifest.display()
        ));
    }
    fs::create_dir_all(&options.work_dir)
        .map_err(|error| format!("{}: {error}", options.work_dir.display()))?;
    if let Some(parent) = options.out.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.summary.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let context = RunContext::new(options)?;
    let jobs = context.options.jobs.min(paths.len()).max(1);
    let cached_count = context.cached_results.len();
    writeln!(
        stdout,
        "[info] running {} PHPT tests with {} job(s)",
        paths.len(),
        jobs
    )
    .map_err(|error| error.to_string())?;
    if cached_count > 0 {
        writeln!(
            stdout,
            "[info] loaded {cached_count} reusable PHPT result candidate(s)"
        )
        .map_err(|error| error.to_string())?;
    }
    let results = if jobs == 1 {
        run_phpt_paths_serial(&context, &paths)
    } else {
        run_phpt_paths_parallel(&context, &paths, jobs)
    };
    let reused = results
        .iter()
        .filter(|result| result.cache_status.as_deref() == Some("hit"))
        .count();
    let dev_reused = results
        .iter()
        .filter(|result| result.cache_status.as_deref() == Some("dev-pass-hit"))
        .count();
    if reused > 0 {
        writeln!(stdout, "[info] reused {reused} PHPT result(s) from cache")
            .map_err(|error| error.to_string())?;
    }
    if dev_reused > 0 {
        writeln!(
            stdout,
            "[info] reused {dev_reused} passing PHPT result(s) with dev input cache"
        )
        .map_err(|error| error.to_string())?;
    }
    let mut jsonl = String::new();
    for result in &results {
        jsonl.push_str(&result.to_json_line());
        jsonl.push('\n');
    }
    fs::write(&context.options.out, jsonl)
        .map_err(|error| format!("{}: {error}", context.options.out.display()))?;
    fs::write(&context.options.summary, render_run_summary(&results))
        .map_err(|error| format!("{}: {error}", context.options.summary.display()))?;
    let failed = results
        .iter()
        .filter(|result| !matches!(result.outcome.as_str(), "PASS" | "SKIP" | "XFAIL"))
        .count();
    writeln!(
        stdout,
        "[ok] ran {} PHPT tests with {} non-green outcomes; reports: {} {}",
        results.len(),
        failed,
        context.options.out.display(),
        context.options.summary.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(if failed == 0 { 0 } else { 1 })
}

pub(crate) fn rerun_manifest<W: Write>(args: &[String], stdout: &mut W) -> Result<i32, String> {
    let options = RerunManifestOptions::parse(args)?;
    let results = read_run_results(&options.results)?;
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    for result in results {
        if matches!(result.outcome.as_str(), "PASS" | "SKIP" | "XFAIL") {
            continue;
        }
        if seen.insert(result.path.clone()) {
            paths.push(result.path);
        }
    }
    if let Some(parent) = options.out.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let mut out = String::new();
    for path in &paths {
        out.push_str(&format!("{{\"path\":\"{}\"}}\n", escape_json(path)));
    }
    fs::write(&options.out, out).map_err(|error| format!("{}: {error}", options.out.display()))?;
    writeln!(
        stdout,
        "[ok] wrote {} non-green PHPT path(s) from {} to {}",
        paths.len(),
        options.results.display(),
        options.out.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}

fn run_phpt_paths_serial(context: &RunContext, paths: &[String]) -> Vec<PhptRunResult> {
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| run_one_phpt_result(context, path, index))
        .collect()
}

fn run_phpt_paths_parallel(
    context: &RunContext,
    paths: &[String],
    jobs: usize,
) -> Vec<PhptRunResult> {
    let next_index = Mutex::new(0usize);
    let results = Mutex::new(vec![None; paths.len()]);

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    let index = {
                        let mut next = next_index.lock().expect("PHPT work queue lock poisoned");
                        if *next >= paths.len() {
                            return;
                        }
                        let index = *next;
                        *next += 1;
                        index
                    };
                    let result = run_one_phpt_result(context, &paths[index], index);
                    results.lock().expect("PHPT result lock poisoned")[index] = Some(result);
                }
            });
        }
    });

    results
        .into_inner()
        .expect("PHPT result lock poisoned")
        .into_iter()
        .map(|result| result.expect("PHPT worker did not write a result"))
        .collect()
}

fn run_one_phpt_result(context: &RunContext, manifest_path: &str, index: usize) -> PhptRunResult {
    match run_one_phpt(context, manifest_path, index) {
        Ok(result) => result,
        Err(error) => PhptRunResult::new(manifest_path, "BORK", error),
    }
}

pub(crate) fn read_manifest_paths(path: &Path) -> Result<Vec<String>, String> {
    let manifest =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut paths = Vec::new();
    for (index, line) in manifest.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('{') {
            paths.push(extract_json_string(trimmed, "path").map_err(|error| {
                format!(
                    "{}:{}: manifest entry missing path: {error}",
                    path.display(),
                    index + 1
                )
            })?);
        } else {
            paths.push(trimmed.to_string());
        }
    }
    Ok(paths)
}

pub(super) fn run_one_phpt(
    context: &RunContext,
    manifest_path: &str,
    index: usize,
) -> Result<PhptRunResult, String> {
    let options = &context.options;
    let phpt_path = resolve_phpt_path(&options.php_src, manifest_path);
    let (source, source_has_invalid_utf8) = read_phpt_source_lossy_with_invalid_utf8(&phpt_path)?;
    let document = parse_phpt(&source);
    let cache_key = phpt_result_cache_key(context, manifest_path, &source, &document, &phpt_path)?;
    let input_cache_key =
        phpt_result_input_cache_key(context, manifest_path, &source, &document, &phpt_path)?;
    if let Some(cached) = context.cached_results.get(manifest_path)
        && cached.cache_key.as_deref() == Some(cache_key.as_str())
    {
        return Ok(cached.clone().mark_cache_hit());
    }
    if context.options.dev_reuse_pass
        && let Some(cached) = context.cached_results.get(manifest_path)
        && cached.outcome == "PASS"
        && cached.input_cache_key.as_deref() == Some(input_cache_key.as_str())
    {
        return Ok(cached.clone().mark_dev_pass_cache_hit());
    }
    if let Some(diagnostic) = document
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.id == "PHPT_UNSUPPORTED_SECTION")
    {
        return Ok(
            PhptRunResult::new(manifest_path, "BORK", diagnostic.message.clone())
                .with_cache_keys(cache_key, input_cache_key),
        );
    }
    if source_has_invalid_utf8 {
        return Ok(PhptRunResult::new(
            manifest_path,
            "SKIP",
            "non-UTF8 PHPT source is tracked as runner malformed-or-non-utf8 gap",
        )
        .with_cache_keys(cache_key, input_cache_key));
    }
    if let Some(reason) = target_cli_skip_reason(
        manifest_path,
        options.target_mode,
        &document.sections,
        &source,
    ) {
        return Ok(PhptRunResult::new(manifest_path, "SKIP", reason)
            .with_cache_keys(cache_key, input_cache_key));
    }
    let work_dir =
        options
            .work_dir
            .join("target")
            .join(format!("case-{}-{}", std::process::id(), index));
    let _ = fs::remove_dir_all(&work_dir);
    fs::create_dir_all(&work_dir).map_err(|error| format!("{}: {error}", work_dir.display()))?;
    copy_phpt_support_files(&phpt_path, &work_dir)?;

    if let Some(reason) = required_extensions_skip_reason(options, &document.sections, &work_dir)? {
        return Ok(PhptRunResult::new(manifest_path, "SKIP", reason)
            .with_cache_keys(cache_key, input_cache_key));
    }

    if let Some(skipif) = section(&document.sections, "SKIPIF") {
        let skip_path = work_dir.join("skipif.php");
        fs::write(&skip_path, &skipif.body)
            .map_err(|error| format!("{}: {error}", skip_path.display()))?;
        let skip_env = skipif_env_args(&document.sections);
        let skip = run_php(options, &skip_path, &work_dir, &[], &skip_env, &[], None)?;
        if skip.stdout.to_ascii_lowercase().starts_with("skip") {
            run_clean_if_present(options, &document.sections, &work_dir)?;
            return Ok(PhptRunResult::new(
                manifest_path,
                "SKIP",
                first_non_empty_line(&skip.stdout),
            )
            .with_cache_keys(cache_key, input_cache_key));
        }
    }

    let Some(file_body) = file_body(&document.sections, &phpt_path)? else {
        return Ok(PhptRunResult::new(
            manifest_path,
            "BORK",
            "missing FILE, FILEEOF, or FILE_EXTERNAL",
        )
        .with_cache_keys(cache_key, input_cache_key));
    };
    let test_path = work_dir.join(phpt_execution_filename(&phpt_path));
    fs::write(&test_path, file_body)
        .map_err(|error| format!("{}: {error}", test_path.display()))?;
    let ini = ini_args(&document.sections);
    let env = env_args(&document.sections);
    let args = section(&document.sections, "ARGS")
        .map(|section| split_phpt_args(&section.body))
        .unwrap_or_default();
    let capture_stdio = capture_stdio(&document.sections);
    let stdin = stdin_from_sections(&document.sections, capture_stdio);
    let xfail =
        section(&document.sections, "XFAIL").map(|section| first_non_empty_line(&section.body));
    let output = run_php(options, &test_path, &work_dir, &ini, &env, &args, stdin)?;
    run_clean_if_present(options, &document.sections, &work_dir)?;

    let Some((kind, expected)) = expectation(&document.sections, &phpt_path)? else {
        return Ok(
            PhptRunResult::new(manifest_path, "BORK", "missing expectation section")
                .with_cache_keys(cache_key, input_cache_key),
        );
    };
    let matched = match_expectation(
        kind,
        &normalize_expected_output(&expected),
        &normalize_actual_output(&captured_output(&output, capture_stdio)),
    );
    if matched.matched {
        if let Some(reason) = xfail {
            return Ok(PhptRunResult::new(
                manifest_path,
                "FAIL",
                format!("XFAIL test unexpectedly passed: {reason}"),
            )
            .with_cache_keys(cache_key, input_cache_key));
        }
        Ok(PhptRunResult::new(manifest_path, "PASS", String::new())
            .with_cache_keys(cache_key, input_cache_key))
    } else {
        let detail = matched
            .diff
            .map(|diff| {
                format!(
                    "{} first_mismatch={:?} expected=`{}` actual=`{}`",
                    diff.message, diff.first_mismatch, diff.expected_excerpt, diff.actual_excerpt
                )
            })
            .unwrap_or_else(|| "output did not match".to_string());
        let detail = if output.status != 0 {
            format!(
                "{detail}; target exited with status {}; stderr={}",
                output.status, output.stderr
            )
        } else {
            detail
        };
        Ok(PhptRunResult::new(
            manifest_path,
            if xfail.is_some() { "XFAIL" } else { "FAIL" },
            xfail
                .map(|reason| format!("expected failure: {reason}; {detail}"))
                .unwrap_or(detail),
        )
        .with_cache_keys(cache_key, input_cache_key))
    }
}

pub(crate) fn read_phpt_source_lossy_with_invalid_utf8(
    path: &Path,
) -> Result<(String, bool), String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let has_invalid_utf8 = std::str::from_utf8(&bytes).is_err();
    Ok((
        String::from_utf8_lossy(&bytes).into_owned(),
        has_invalid_utf8,
    ))
}
