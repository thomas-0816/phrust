use super::*;

pub(crate) fn symbol_index<W: Write>(args: &[String], stdout: &mut W) -> Result<i32, String> {
    let options = SymbolOptions::parse(args)?;
    let mut entries = collect_symbol_entries(&options.php_src)?;
    entries.sort_by(|left, right| {
        (
            &left.path,
            left.line,
            &left.kind,
            &left.c_name,
            &left.php_name,
        )
            .cmp(&(
                &right.path,
                right.line,
                &right.kind,
                &right.c_name,
                &right.php_name,
            ))
    });
    if let Some(parent) = options.symbols.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let mut out = String::new();
    for entry in &entries {
        out.push_str(&entry.to_json_line());
        out.push('\n');
    }
    fs::write(&options.symbols, out)
        .map_err(|error| format!("{}: {error}", options.symbols.display()))?;
    writeln!(
        stdout,
        "[ok] wrote {} symbol entries to {}",
        entries.len(),
        options.symbols.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}
