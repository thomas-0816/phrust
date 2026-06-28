use super::*;

pub(crate) fn lookup_symbol<W: Write, E: Write>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String> {
    let options = LookupOptions::parse(args)?;
    if !options.symbols.is_file() {
        return Err(format!(
            "{}: source symbol index does not exist; run `just phpt-source-index`",
            options.symbols.display()
        ));
    }
    let query = options.symbol.to_ascii_lowercase();
    let index = fs::read_to_string(&options.symbols)
        .map_err(|error| format!("{}: {error}", options.symbols.display()))?;
    let mut matches = Vec::new();
    for (line_index, line) in index.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry = match SymbolEntry::from_json_line(line) {
            Ok(entry) => entry,
            Err(error) => {
                writeln!(
                    stderr,
                    "{}:{}: {error}",
                    options.symbols.display(),
                    line_index + 1
                )
                .map_err(|io| io.to_string())?;
                continue;
            }
        };
        if entry.matches(&query) {
            matches.push(entry);
        }
    }
    if matches.is_empty() {
        writeln!(stderr, "no php-src symbol matches for `{}`", options.symbol)
            .map_err(|error| error.to_string())?;
        return Ok(1);
    }
    for entry in matches {
        writeln!(
            stdout,
            "{}\t{}\t{}\t{}:{}\t{}",
            entry.kind, entry.php_name, entry.c_name, entry.path, entry.line, entry.module
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(0)
}
