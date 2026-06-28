use super::*;

pub(crate) fn baseline_results<W: Write, E: Write>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String> {
    let options = BaselineOptions::parse(args)?;
    let results = read_run_results(&options.results)?;
    let corpus_entries = read_phpt_entries(&options.corpus)?;
    let corpus = corpus_entries
        .iter()
        .map(|entry| (entry.path.clone(), entry.module.clone()))
        .collect::<BTreeMap<_, _>>();
    let accepting_baseline = env::var("PHPT_ACCEPT_BASELINE").as_deref() == Ok("1");
    let previous_failures = if let Some(previous) = &options.previous_known_failures {
        if previous.is_file() {
            read_known_failures(previous)?
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    let previous_results = if let Some(previous) = &options.previous_results {
        if previous.is_file() {
            read_run_results(previous)?
                .into_iter()
                .filter(|result| !matches!(result.outcome.as_str(), "PASS" | "SKIP" | "XFAIL"))
                .map(|result| (result.path.clone(), result))
                .collect::<BTreeMap<_, _>>()
        } else {
            BTreeMap::new()
        }
    } else {
        BTreeMap::new()
    };
    let current_results = results
        .iter()
        .filter(|result| !matches!(result.outcome.as_str(), "PASS" | "SKIP" | "XFAIL"))
        .map(|result| (result.path.clone(), result))
        .collect::<BTreeMap<_, _>>();
    let previous_first_seen = previous_failures
        .iter()
        .map(|failure| {
            (
                (
                    failure.path.clone(),
                    failure.outcome.clone(),
                    failure.failure_fingerprint.clone(),
                ),
                failure.first_seen_timestamp.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let previous_path_outcome_first_seen = previous_failures
        .iter()
        .map(|failure| {
            (
                (failure.path.clone(), failure.outcome.clone()),
                failure.first_seen_timestamp.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let previous_path_first_seen = previous_failures
        .iter()
        .map(|failure| (failure.path.clone(), failure.first_seen_timestamp.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut failures = results
        .iter()
        .filter(|result| !matches!(result.outcome.as_str(), "PASS" | "SKIP" | "XFAIL"))
        .map(|result| {
            let module = corpus
                .get(&result.path)
                .cloned()
                .unwrap_or_else(|| module_guess(&result.path));
            let fingerprint = failure_fingerprint(result);
            let first_seen = previous_first_seen
                .get(&(
                    result.path.clone(),
                    result.outcome.clone(),
                    fingerprint.clone(),
                ))
                .cloned()
                .or_else(|| {
                    previous_path_outcome_first_seen
                        .get(&(result.path.clone(), result.outcome.clone()))
                        .filter(|_| {
                            is_related_known_failure_evolution(
                                previous_results.get(&result.path),
                                current_results.get(&result.path).copied(),
                            )
                        })
                        .cloned()
                })
                .or_else(|| {
                    previous_path_first_seen
                        .get(&result.path)
                        .filter(|_| {
                            is_related_known_failure_evolution(
                                previous_results.get(&result.path),
                                current_results.get(&result.path).copied(),
                            )
                        })
                        .cloned()
                })
                .unwrap_or_else(|| options.timestamp.clone());
            KnownFailure {
                path: result.path.clone(),
                module_tag: module.clone(),
                outcome: result.outcome.clone(),
                failure_fingerprint: fingerprint,
                primary_missing_feature_guess: missing_feature_guess(result),
                owner_module: module,
                first_seen_timestamp: first_seen,
            }
        })
        .collect::<Vec<_>>();
    failures.sort_by(|left, right| left.path.cmp(&right.path));

    if previous_failures.is_empty() && !failures.is_empty() && !accepting_baseline {
        writeln!(
            stderr,
            "refusing to create a non-green PHPT baseline without PHPT_ACCEPT_BASELINE=1"
        )
        .map_err(|error| error.to_string())?;
        return Ok(1);
    }

    if !previous_failures.is_empty() {
        let mut previous_keys = previous_failures
            .iter()
            .map(|failure| {
                (
                    failure.path.clone(),
                    failure.outcome.clone(),
                    failure.failure_fingerprint.clone(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        for result in previous_results.values() {
            previous_keys.insert((
                result.path.clone(),
                result.outcome.clone(),
                failure_fingerprint(result),
            ));
        }
        let regressions = failures
            .iter()
            .filter(|failure| {
                !previous_keys.contains(&(
                    failure.path.clone(),
                    failure.outcome.clone(),
                    failure.failure_fingerprint.clone(),
                )) && !is_related_known_failure_evolution(
                    previous_results.get(&failure.path),
                    current_results.get(&failure.path).copied(),
                )
            })
            .collect::<Vec<_>>();
        if !regressions.is_empty() {
            writeln!(
                stderr,
                "PHPT full regression detected {} new or changed failure fingerprints",
                regressions.len()
            )
            .map_err(|error| error.to_string())?;
            for failure in regressions.iter().take(25) {
                writeln!(
                    stderr,
                    "{} {} {}",
                    failure.path, failure.outcome, failure.failure_fingerprint
                )
                .map_err(|error| error.to_string())?;
            }
            return Ok(1);
        }
    }

    if !accepting_baseline {
        writeln!(
            stdout,
            "[ok] PHPT full regression matched accepted baseline; set PHPT_ACCEPT_BASELINE=1 to update committed baseline files"
        )
        .map_err(|error| error.to_string())?;
        return Ok(0);
    }

    if let Some(parent) = options.known_failures.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.metadata.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.module_counts.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.report.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let mut jsonl = String::new();
    for failure in &failures {
        jsonl.push_str(&failure.to_json_line());
        jsonl.push('\n');
    }
    fs::write(&options.known_failures, jsonl)
        .map_err(|error| format!("{}: {error}", options.known_failures.display()))?;
    let metadata = BaselineMetadata::from_results(
        &results,
        failures.len(),
        &options.timestamp,
        &options.known_failures,
    );
    let triage = build_triage(&corpus_entries, &failures, &results);
    fs::write(&options.metadata, metadata.to_json())
        .map_err(|error| format!("{}: {error}", options.metadata.display()))?;
    fs::write(
        &options.module_counts,
        render_baseline_module_counts(&triage),
    )
    .map_err(|error| format!("{}: {error}", options.module_counts.display()))?;
    fs::write(
        &options.report,
        render_baseline_report(&results, &failures, &options.timestamp),
    )
    .map_err(|error| format!("{}: {error}", options.report.display()))?;
    writeln!(
        stdout,
        "[ok] wrote {} known failures to {}, metadata {}, module counts {}, and report {}",
        failures.len(),
        options.known_failures.display(),
        options.metadata.display(),
        options.module_counts.display(),
        options.report.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}

pub(crate) fn verify_baseline<W: Write, E: Write>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String> {
    let options = VerifyBaselineOptions::parse(args)?;
    let corpus = super::run::read_manifest_paths(&options.corpus)?;
    let failures = read_known_failures(&options.known_failures)?;
    let metadata = read_baseline_metadata(&options.metadata)?;
    let module_counts = read_baseline_module_counts(&options.module_counts)?;
    let known_gap_catalog = read_known_gap_catalog(&options.known_gap_catalog)?;
    let report = read_baseline_report_totals(&options.report)?;

    let mut errors = Vec::new();
    if metadata.schema_version != "phpt-full-baseline-v1" {
        errors.push(format!(
            "{}: unsupported schema_version `{}`",
            options.metadata.display(),
            metadata.schema_version
        ));
    }
    if metadata.corpus_count != corpus.len() {
        errors.push(format!(
            "baseline corpus_count mismatch: metadata={} corpus={}",
            metadata.corpus_count,
            corpus.len()
        ));
    }
    if metadata.known_failure_count != failures.len() {
        errors.push(format!(
            "known_failure_count mismatch: metadata={} manifest={}",
            metadata.known_failure_count,
            failures.len()
        ));
    }

    let failure_counts = count_known_failure_outcomes(&failures);
    let manifest_fail = *failure_counts.get("FAIL").unwrap_or(&0);
    let manifest_bork = *failure_counts.get("BORK").unwrap_or(&0);
    if metadata.fail_count != manifest_fail {
        errors.push(format!(
            "FAIL count mismatch: metadata={} manifest={manifest_fail}",
            metadata.fail_count
        ));
    }
    if metadata.bork_count != manifest_bork {
        errors.push(format!(
            "BORK count mismatch: metadata={} manifest={manifest_bork}",
            metadata.bork_count
        ));
    }

    compare_report_total("PASS", metadata.pass_count, &report, &mut errors);
    compare_report_total("SKIP", metadata.skip_count, &report, &mut errors);
    compare_report_total("FAIL", metadata.fail_count, &report, &mut errors);
    compare_report_total("BORK", metadata.bork_count, &report, &mut errors);
    if metadata.timestamp != report.timestamp {
        errors.push(format!(
            "timestamp mismatch: metadata={} report={}",
            metadata.timestamp, report.timestamp
        ));
    }

    let non_green = report
        .outcomes
        .iter()
        .filter(|(outcome, _)| !matches!(outcome.as_str(), "PASS" | "SKIP" | "XFAIL"))
        .map(|(_, count)| *count)
        .sum::<usize>();
    if non_green > 0 && failures.is_empty() {
        errors.push(format!(
            "{} reports {non_green} non-green outcomes but {} is empty",
            options.report.display(),
            options.known_failures.display()
        ));
    }
    if metadata.fail_count + metadata.bork_count != metadata.known_failure_count {
        errors.push(format!(
            "known_failure_count must equal fail_count + bork_count: {} != {} + {}",
            metadata.known_failure_count, metadata.fail_count, metadata.bork_count
        ));
    }
    if metadata.corpus_count
        != metadata.pass_count + metadata.skip_count + metadata.fail_count + metadata.bork_count
    {
        errors.push(format!(
            "corpus_count must equal PASS + SKIP + FAIL + BORK: {} != {} + {} + {} + {}",
            metadata.corpus_count,
            metadata.pass_count,
            metadata.skip_count,
            metadata.fail_count,
            metadata.bork_count
        ));
    }
    verify_baseline_module_counts(&module_counts, &metadata, &mut errors);
    verify_known_gap_catalog(
        &known_gap_catalog,
        &failures,
        &module_counts,
        &metadata,
        &mut errors,
    );

    for (index, failure) in failures.iter().enumerate() {
        if failure.path.is_empty()
            || failure.module_tag.is_empty()
            || failure.outcome.is_empty()
            || failure.failure_fingerprint.is_empty()
            || failure.primary_missing_feature_guess.is_empty()
            || failure.owner_module.is_empty()
            || failure.first_seen_timestamp.is_empty()
        {
            errors.push(format!(
                "{}:{}: known failure has an empty required field",
                options.known_failures.display(),
                index + 1
            ));
            break;
        }
    }

    if !errors.is_empty() {
        for error in &errors {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
        }
        return Ok(1);
    }

    writeln!(
        stdout,
        "[ok] verified PHPT baseline: {} corpus entries, {} known non-green fingerprints",
        metadata.corpus_count, metadata.known_failure_count
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}
