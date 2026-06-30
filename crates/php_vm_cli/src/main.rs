//! VM CLI process entry point.
//!
//! Command parsing and debug/report adapters live in `commands`; reusable
//! library entrypoints live in `php_vm_cli`.

mod commands;

use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticOutputFormat, DiagnosticPhase,
    DiagnosticSeverity, install_panic_diagnostic_hook,
};
use std::io::{self, Write};
use std::process;
use std::str::FromStr;
use std::thread;

const PHP_VM_STACK_SIZE: usize = 128 * 1024 * 1024;

fn main() {
    install_panic_diagnostic_hook("php-vm", env_error_format());
    let handle = match thread::Builder::new()
        .name("php-vm-runtime".to_owned())
        .stack_size(PHP_VM_STACK_SIZE)
        .spawn(commands::main_entry)
    {
        Ok(handle) => handle,
        Err(error) => {
            write_spawn_failure(error, env_error_format());
            process::exit(1);
        }
    };
    handle
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
}

fn write_spawn_failure(error: io::Error, format: DiagnosticOutputFormat) {
    let diagnostic = DiagnosticEnvelope::new(
        "E_PHRUST_CLI_THREAD_SPAWN_FAILED",
        DiagnosticLayer::infrastructure(),
        DiagnosticPhase::new("startup"),
        DiagnosticSeverity::FatalError,
        "failed to spawn php-vm runtime thread",
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
    std::env::var("PHRUST_ERROR_FORMAT")
        .ok()
        .and_then(|value| DiagnosticOutputFormat::from_str(&value).ok())
        .unwrap_or(DiagnosticOutputFormat::Text)
}
