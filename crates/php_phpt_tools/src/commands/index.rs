use super::*;

pub(crate) fn phpt_index<W: Write>(args: &[String], stdout: &mut W) -> Result<i32, String> {
    let options = PhptIndexOptions::parse(args)?;
    let mut entries = collect_phpt_entries(&options.php_src)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    if let Some(parent) = options.out.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    if let Some(parent) = options.report.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let mut jsonl = String::new();
    for entry in &entries {
        jsonl.push_str(&entry.to_json_line());
        jsonl.push('\n');
    }
    fs::write(&options.out, jsonl)
        .map_err(|error| format!("{}: {error}", options.out.display()))?;
    fs::write(&options.report, render_phpt_summary(&entries))
        .map_err(|error| format!("{}: {error}", options.report.display()))?;
    writeln!(
        stdout,
        "[ok] indexed {} PHPT files to {} and {}",
        entries.len(),
        options.out.display(),
        options.report.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}
