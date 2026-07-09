use super::*;

pub(crate) fn generate_module_tests<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<i32, String> {
    let options = GenerateOptions::parse(args)?;
    let corpus = read_phpt_corpus(&options.corpus)?;
    let mut selected = corpus
        .iter()
        .filter(|entry| matches_module_selector(entry, &options.module))
        .cloned()
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| left.path.cmp(&right.path));
    if selected.is_empty() {
        return Err(format!(
            "{}: no PHPT corpus entries match module selector `{}`",
            options.corpus.display(),
            options.module
        ));
    }

    fs::create_dir_all(&options.generated_dir)
        .map_err(|error| format!("{}: {error}", options.generated_dir.display()))?;
    clear_generated_phpts(&options.generated_dir)?;
    if let Some(parent) = options.module_manifest.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.generated_manifest.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::create_dir_all(&options.work_dir)
        .map_err(|error| format!("{}: {error}", options.work_dir.display()))?;

    let mut module_manifest = String::new();
    for entry in &selected {
        module_manifest.push_str(&entry.to_json_line());
        module_manifest.push('\n');
    }
    fs::write(&options.module_manifest, module_manifest)
        .map_err(|error| format!("{}: {error}", options.module_manifest.display()))?;

    let reference_options = RunOptions {
        target: options.reference.clone(),
        target_mode: TargetMode::PhpCli,
        manifest: options.module_manifest.clone(),
        php_src: options.php_src.clone(),
        work_dir: options.work_dir.join("reference"),
        out: options.work_dir.join("unused-results.jsonl"),
        summary: options.work_dir.join("unused-summary.md"),
        reuse_results: None,
        dev_reuse_pass: false,
        timeout: options.timeout,
        jobs: 1,
    };
    let reference_context = RunContext::new(reference_options.clone())?;

    let mut generated = Vec::new();
    let mut smoke_candidates = selected
        .iter()
        .filter(|entry| is_simple_generation_candidate(entry))
        .cloned()
        .collect::<Vec<_>>();
    smoke_candidates.sort_by_key(|entry| source_len(&options.php_src.join(&entry.path)));
    for entry in smoke_candidates {
        if generated
            .iter()
            .filter(|case: &&GeneratedCase| case.kind == "smoke")
            .count()
            >= options.smoke_count
        {
            break;
        }
        if run::run_one_phpt(
            &reference_context,
            &run::PhptManifestEntry::path(entry.path.clone()),
            generated.len(),
        )?
        .outcome
            != "PASS"
        {
            continue;
        }
        if let Some(case) = build_generated_case(
            &options,
            &reference_options,
            &entry,
            "smoke",
            "smallest reference-passing example",
            None,
            generated.len(),
        )? {
            write_generated_case(&case)?;
            generated.push(case);
        }
    }

    if options.known_failures.is_file() {
        let smoke_originals = generated
            .iter()
            .filter(|case| case.kind == "smoke")
            .map(|case| case.original_path.clone())
            .collect::<BTreeSet<_>>();
        let selected_by_path = selected
            .iter()
            .map(|entry| (entry.path.clone(), entry.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut failure_candidates = read_known_failures(&options.known_failures)?
            .into_iter()
            .filter_map(|failure| selected_by_path.get(&failure.path).cloned())
            .filter(|entry| !smoke_originals.contains(&entry.path))
            .filter(is_simple_generation_candidate)
            .collect::<Vec<_>>();
        failure_candidates.sort_by_key(|entry| source_len(&options.php_src.join(&entry.path)));
        for entry in failure_candidates {
            if generated
                .iter()
                .filter(|case: &&GeneratedCase| case.kind == "regression")
                .count()
                >= options.regression_count
            {
                break;
            }
            if let Some(case) = build_generated_case(
                &options,
                &reference_options,
                &entry,
                "regression",
                "known target failure minimized against reference output",
                Some(ReductionMode::LineRemoval),
                generated.len(),
            )? {
                write_generated_case(&case)?;
                generated.push(case);
            }
        }
    }

    if generated.is_empty() {
        return Err(format!(
            "module selector `{}` produced no generated PHPTs",
            options.module
        ));
    }
    let mut generated_manifest = String::new();
    for case in &generated {
        generated_manifest.push_str(&case.to_json_line());
        generated_manifest.push('\n');
    }
    fs::write(&options.generated_manifest, generated_manifest)
        .map_err(|error| format!("{}: {error}", options.generated_manifest.display()))?;

    writeln!(
        stdout,
        "[ok] wrote {} original entries to {}",
        selected.len(),
        options.module_manifest.display()
    )
    .map_err(|error| error.to_string())?;
    writeln!(
        stdout,
        "[ok] generated {} PHPTs under {} and manifest {}",
        generated.len(),
        options.generated_dir.display(),
        options.generated_manifest.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}
