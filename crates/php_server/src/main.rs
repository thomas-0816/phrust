use php_server::{config::ServerConfig, server};
use tracing_subscriber::{EnvFilter, fmt};

const TOKIO_WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;

fn main() {
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
    runtime.block_on(async_main());
}

async fn async_main() {
    match ServerConfig::parse_env() {
        Ok(config) => {
            if config.help {
                print!("{}", ServerConfig::help_text());
                return;
            }
            if let Err(error) = server::run(config).await {
                eprintln!("phrust-server: {error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("phrust-server: {error}");
            eprintln!();
            eprintln!("{}", ServerConfig::help_text());
            std::process::exit(2);
        }
    }
}
