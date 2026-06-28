use std::io::Write;

use crate::commands;

pub(crate) fn run<I, W, E>(args: I, stdout: &mut W, stderr: &mut E) -> Result<i32, String>
where
    I: IntoIterator<Item = String>,
    W: Write,
    E: Write,
{
    let args: Vec<String> = args.into_iter().collect();
    let Some(command) = args.first().map(String::as_str) else {
        print_usage(stdout)?;
        return Ok(0);
    };
    match command {
        "source-index" => commands::source_index::source_index(&args[1..], stdout),
        "symbol-index" => commands::symbol_index::symbol_index(&args[1..], stdout),
        "lookup-symbol" => commands::lookup::lookup_symbol(&args[1..], stdout, stderr),
        "phpt-index" => commands::index::phpt_index(&args[1..], stdout),
        "run" => commands::run::run_phpt_manifest(&args[1..], stdout),
        "rerun-manifest" => commands::run::rerun_manifest(&args[1..], stdout),
        "baseline" => commands::baseline::baseline_results(&args[1..], stdout, stderr),
        "verify-baseline" => commands::baseline::verify_baseline(&args[1..], stdout, stderr),
        "triage" => commands::triage::triage_phpt_baseline(&args[1..], stdout),
        "generate" => commands::generate::generate_module_tests(&args[1..], stdout),
        "verify-source" => commands::verify::verify_source(&args[1..], stdout, stderr),
        "--help" | "-h" | "help" => {
            print_usage(stdout)?;
            Ok(0)
        }
        _ => Err(format!("unknown php-phpt-tools command `{command}`")),
    }
}

fn print_usage<W: Write>(stdout: &mut W) -> Result<(), String> {
    writeln!(
        stdout,
        "usage: php-phpt-tools <source-index|symbol-index|lookup-symbol|phpt-index|run|rerun-manifest|baseline|verify-baseline|triage|generate|verify-source> [options]"
    )
    .map_err(|error| error.to_string())
}
