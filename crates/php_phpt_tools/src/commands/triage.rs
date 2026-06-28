use super::*;

pub(crate) fn triage_phpt_baseline<W: Write>(
    args: &[String],
    stdout: &mut W,
) -> Result<i32, String> {
    let options = TriageOptions::parse(args)?;
    let corpus = read_phpt_entries(&options.corpus)?;
    let failures = read_known_failures(&options.known_failures)?;
    let metadata = read_baseline_metadata(&options.metadata)?;
    let results = match &options.results {
        Some(path) if path.is_file() => read_run_results(path)?,
        Some(path) => {
            return Err(format!(
                "{}: PHPT result file does not exist",
                path.display()
            ));
        }
        None => Vec::new(),
    };
    let mut triage = build_triage(&corpus, &failures, &results);
    if results.is_empty() && options.module_counts.is_file() {
        let module_counts = read_baseline_module_counts(&options.module_counts)?;
        apply_baseline_module_counts(&mut triage, &module_counts);
    }
    let module_counts = if options.module_counts.is_file() {
        read_baseline_module_counts(&options.module_counts)?
    } else {
        Vec::new()
    };
    let known_gap_rows = build_known_gap_rows(&failures, &module_counts);

    write_triage_outputs(&options, &metadata, &triage, &known_gap_rows)?;
    writeln!(
        stdout,
        "[ok] wrote PHPT triage report {}, extension policy {}, known gaps {}, priority manifest {}, and {} module plans",
        options.report.display(),
        options.extension_policy_report.display(),
        options.known_gap_report.display(),
        options.priority.display(),
        MODULE_PLAN.len()
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}
