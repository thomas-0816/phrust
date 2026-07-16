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
    let mut host_generated_skips = Vec::new();
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
                let size_mismatch = size != entry.size;
                let hash_mismatch = sha256 != entry.sha256;
                if size_mismatch || hash_mismatch {
                    if is_host_generated_source_artifact(&entry.path) {
                        host_generated_skips.push(format!(
                            "{}: host-generated artifact differs from manifest on this platform",
                            entry.path
                        ));
                    } else {
                        if size_mismatch {
                            errors.push(format!(
                                "{}: size mismatch manifest={} actual={}",
                                entry.path, entry.size, size
                            ));
                        }
                        if hash_mismatch {
                            errors.push(format!("{}: sha256 mismatch", entry.path));
                        }
                    }
                }
            }
            Err(error) => {
                if is_host_generated_source_artifact(&entry.path) && !path.exists() {
                    host_generated_skips.push(format!(
                        "{}: host-generated artifact is absent on this platform",
                        entry.path
                    ));
                } else {
                    errors.push(format!("{}: {error}", entry.path));
                }
            }
        }
    }
    if !errors.is_empty() {
        for error in &errors {
            writeln!(stderr, "{error}").map_err(|io| io.to_string())?;
        }
        return Ok(1);
    }
    for skip in &host_generated_skips {
        writeln!(stdout, "[skip] {skip}").map_err(|io| io.to_string())?;
    }
    let verified = checked.saturating_sub(host_generated_skips.len());
    writeln!(
        stdout,
        "[ok] verified {verified} php-src manifest entries from {}; skipped {} host-generated entries",
        options.manifest.display(),
        host_generated_skips.len()
    )
    .map_err(|error| error.to_string())?;
    Ok(0)
}

fn is_host_generated_source_artifact(path: &str) -> bool {
    matches!(
        path,
        "Zend/zend_ini_parser.c"
            | "Zend/zend_ini_parser.h"
            | "Zend/zend_language_parser.c"
            | "Zend/zend_language_parser.h"
            | "ext/json/json_parser.tab.h"
            | "ext/opcache/jit/ir/ir_emit_aarch64.h"
            | "main/build-defs.h"
            | "main/php_config.h"
    )
}
