#![cfg_attr(target_family = "wasm", allow(dead_code, unused_imports))]

use std::env;
use std::io::{self, IsTerminal, Write};
use std::process;
use std::str::FromStr;
use std::thread;

use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticOutputFormat, DiagnosticPhase,
    DiagnosticSeverity, install_panic_diagnostic_hook,
};

const PHP_CLI_STACK_SIZE: usize = 128 * 1024 * 1024;

fn main() {
    install_panic_diagnostic_hook("phrust-php", env_error_format());
    let args: Vec<String> = env::args().skip(1).collect();
    let run = |args: Vec<String>| {
        let mut stdin = io::stdin();
        let stdin_is_terminal = stdin.is_terminal();
        php_vm_cli::php_cli::run_with_terminal(
            args,
            &mut stdin,
            stdin_is_terminal,
            &mut io::stdout(),
            &mut io::stderr(),
        )
    };
    #[cfg(not(target_family = "wasm"))]
    let code = {
        let handle = match thread::Builder::new()
            .name("phrust-php-runtime".to_owned())
            .stack_size(PHP_CLI_STACK_SIZE)
            .spawn(move || run(args))
        {
            Ok(handle) => handle,
            Err(error) => {
                write_spawn_failure(error, env_error_format());
                process::exit(1);
            }
        };
        handle
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    };
    #[cfg(target_family = "wasm")]
    let code = run(args);
    if code != 0 {
        std::process::exit(code);
    }
}

fn write_spawn_failure(error: io::Error, format: DiagnosticOutputFormat) {
    let diagnostic = DiagnosticEnvelope::new(
        "E_PHRUST_CLI_THREAD_SPAWN_FAILED",
        DiagnosticLayer::infrastructure(),
        DiagnosticPhase::new("startup"),
        DiagnosticSeverity::FatalError,
        "failed to spawn phrust-php runtime thread",
    );
    let rendered = match format {
        DiagnosticOutputFormat::Text => {
            let mut line = diagnostic.text_line();
            line.push_str("; cause=");
            line.push_str(&error.to_string());
            line.push('\n');
            line
        }
        DiagnosticOutputFormat::Json => diagnostic.json_line().unwrap_or_else(|_| {
            let mut line = diagnostic.text_line();
            line.push('\n');
            line
        }),
    };
    let _ = io::stderr().lock().write_all(rendered.as_bytes());
}

fn env_error_format() -> DiagnosticOutputFormat {
    env::var("PHRUST_ERROR_FORMAT")
        .ok()
        .and_then(|value| DiagnosticOutputFormat::from_str(&value).ok())
        .unwrap_or(DiagnosticOutputFormat::Text)
}
