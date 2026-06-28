use super::*;

pub(crate) fn source_index<W: Write>(args: &[String], stdout: &mut W) -> Result<i32, String> {
    let options = SourceOptions::parse(args)?;
    let mut entries = collect_manifest_entries(&options.php_src)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    if let Some(parent) = options.manifest.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let mut out = String::new();
    for entry in &entries {
        out.push_str(&entry.to_json_line());
        out.push('\n');
    }
    fs::write(&options.manifest, out)
        .map_err(|error| format!("{}: {error}", options.manifest.display()))?;
    writeln!(
        stdout,
        "[ok] wrote {} entries to {}",
        entries.len(),
        options.manifest.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}
