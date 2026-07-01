use php_diagnostics::{DiagnosticOutputFormat, install_panic_diagnostic_hook};
use php_server::{config::ServerConfig, server};
use std::str::FromStr;
use tracing_subscriber::{EnvFilter, fmt};

const TOKIO_WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

fn main() {
    let error_format = env_error_format();
    install_panic_diagnostic_hook("phrust-server", error_format);
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(TOKIO_WORKER_STACK_BYTES)
        .build()
        .expect("tokio runtime should initialize");
    runtime.block_on(async_main(error_format));
}

async fn async_main(error_format: DiagnosticOutputFormat) {
    match ServerConfig::parse_env() {
        Ok(config) => {
            if config.help {
                print!("{}", ServerConfig::help_text());
                return;
            }
            if let Err(error) = server::run(config).await {
                eprint!("{}", render_server_error(&error, error_format));
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprint!("{}", render_config_error(&error, error_format));
            eprintln!();
            eprintln!("{}", ServerConfig::help_text());
            std::process::exit(2);
        }
    }
}

fn env_error_format() -> DiagnosticOutputFormat {
    std::env::var("PHRUST_SERVER_ERROR_FORMAT")
        .ok()
        .and_then(|value| DiagnosticOutputFormat::from_str(&value).ok())
        .unwrap_or(DiagnosticOutputFormat::Text)
}

fn render_config_error(
    error: &php_server::config::ConfigError,
    format: DiagnosticOutputFormat,
) -> String {
    render_envelope(error.diagnostic(), format)
}

fn render_server_error(error: &server::ServerError, format: DiagnosticOutputFormat) -> String {
    render_envelope(&error.diagnostic(), format)
}

fn render_envelope(
    envelope: &php_diagnostics::DiagnosticEnvelope,
    format: DiagnosticOutputFormat,
) -> String {
    match format {
        DiagnosticOutputFormat::Text => {
            let mut line = envelope.text_line();
            line.push('\n');
            line
        }
        DiagnosticOutputFormat::Json => envelope.json_line().unwrap_or_else(|error| {
            format!(
                "E_PHRUST_SERVER_DIAGNOSTIC_RENDER layer=server phase=render severity=error: failed to render server diagnostic; cause={error}\n"
            )
        }),
    }
}
