use super::*;

pub(crate) fn verify_source<W: Write, E: Write>(
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String> {
    let options = SourceOptions::parse(args)?;
    if !options.manifest.is_file() {
        return Err(format!(
            "{}: source hash manifest does not exist; run `just phpt-source-index`",
            options.manifest.display()
        ));
    }
    let manifest = fs::read_to_string(&options.manifest)
        .map_err(|error| format!("{}: {error}", options.manifest.display()))?;
    let mut checked = 0usize;
    let mut errors = Vec::new();
    for (line_index, line) in manifest.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let entry = match ManifestEntry::from_json_line(line) {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!(
                    "{}:{}: {error}",
                    options.manifest.display(),
                    line_index + 1
                ));
                continue;
            }
        };
        checked += 1;
        let path = options.php_src.join(&entry.path);
        match hash_file(&path) {
            Ok((size, sha256)) => {
                if size != entry.size {
                    errors.push(format!(
                        "{}: size mismatch manifest={} actual={}",
                        entry.path, entry.size, size
                    ));
                }
                if sha256 != entry.sha256 {
                    errors.push(format!("{}: sha256 mismatch", entry.path));
                }
            }
            Err(error) => errors.push(format!("{}: {error}", entry.path)),
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
        "[ok] verified {checked} php-src manifest entries from {}",
        options.manifest.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}
