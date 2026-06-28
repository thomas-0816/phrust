use std::{
    env, fmt,
    net::SocketAddr,
    path::{Path, PathBuf},
};

const DEFAULT_LISTEN: &str = "127.0.0.1:8080";
const DEFAULT_INDEX: &str = "index.php";
const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576;
const DEFAULT_MAX_UPLOAD_FILES: usize = 32;
const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_SCRIPT_CACHE_SHARDS: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    pub listen: SocketAddr,
    pub docroot: PathBuf,
    pub index: String,
    pub front_controller: Option<PathBuf>,
    pub max_body_bytes: usize,
    pub upload_temp_dir: PathBuf,
    pub max_upload_files: usize,
    pub max_upload_file_bytes: usize,
    pub max_in_flight: usize,
    pub request_timeout_ms: u64,
    pub metrics_endpoint_enabled: bool,
    pub script_cache_enabled: bool,
    pub script_cache_shards: usize,
    pub help: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigError {
    message: String,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConfigError {}

impl ServerConfig {
    pub fn parse_env() -> Result<Self, ConfigError> {
        Self::parse_from(env::args().skip(1))
    }

    pub fn parse_from<I, S>(args: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut listen = parse_listen(DEFAULT_LISTEN)?;
        let mut docroot = None;
        let mut index = DEFAULT_INDEX.to_string();
        let mut front_controller = None;
        let mut max_body_bytes = DEFAULT_MAX_BODY_BYTES;
        let mut upload_temp_dir = std::env::temp_dir().join("phrust-uploads");
        let mut max_upload_files = DEFAULT_MAX_UPLOAD_FILES;
        let mut max_upload_file_bytes = None;
        let mut max_in_flight = default_max_in_flight();
        let mut request_timeout_ms = DEFAULT_REQUEST_TIMEOUT_MS;
        let mut metrics_endpoint_enabled = true;
        let mut script_cache_enabled = true;
        let mut script_cache_shards = DEFAULT_SCRIPT_CACHE_SHARDS;
        let mut help = false;
        let mut args = args.into_iter().map(Into::into);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => help = true,
                "--listen" => listen = parse_listen(&required_value(&arg, &mut args)?)?,
                "--docroot" => docroot = Some(PathBuf::from(required_value(&arg, &mut args)?)),
                "--index" => {
                    index = required_value(&arg, &mut args)?;
                    validate_index(&index)?;
                }
                "--front-controller" => {
                    let value = required_value(&arg, &mut args)?;
                    let path = PathBuf::from(value);
                    validate_relative_path("--front-controller", &path)?;
                    front_controller = Some(path);
                }
                "--max-body-bytes" => {
                    max_body_bytes = parse_positive_usize(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--upload-temp-dir" => {
                    upload_temp_dir = PathBuf::from(required_value(&arg, &mut args)?);
                }
                "--max-upload-files" => {
                    max_upload_files =
                        parse_positive_usize(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--max-upload-file-bytes" => {
                    max_upload_file_bytes = Some(parse_positive_usize(
                        &arg,
                        &required_value(&arg, &mut args)?,
                    )?);
                }
                "--max-in-flight" => {
                    max_in_flight = parse_positive_usize(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--request-timeout-ms" => {
                    request_timeout_ms =
                        parse_positive_u64(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--disable-metrics-endpoint" => metrics_endpoint_enabled = false,
                "--no-script-cache" => script_cache_enabled = false,
                "--script-cache-shards" => {
                    script_cache_shards =
                        parse_positive_usize(&arg, &required_value(&arg, &mut args)?)?;
                }
                _ if arg.starts_with('-') => {
                    return Err(ConfigError::new(format!("unknown flag `{arg}`")));
                }
                _ => return Err(ConfigError::new(format!("unexpected argument `{arg}`"))),
            }
        }

        if help {
            return Ok(Self {
                listen,
                docroot: docroot.unwrap_or_default(),
                index,
                front_controller,
                max_body_bytes,
                upload_temp_dir,
                max_upload_files,
                max_upload_file_bytes: max_upload_file_bytes.unwrap_or(max_body_bytes),
                max_in_flight,
                request_timeout_ms,
                metrics_endpoint_enabled,
                script_cache_enabled,
                script_cache_shards,
                help,
            });
        }

        let docroot = docroot.ok_or_else(|| ConfigError::new("--docroot is required"))?;
        Ok(Self {
            listen,
            docroot,
            index,
            front_controller,
            max_body_bytes,
            upload_temp_dir,
            max_upload_files,
            max_upload_file_bytes: max_upload_file_bytes.unwrap_or(max_body_bytes),
            max_in_flight,
            request_timeout_ms,
            metrics_endpoint_enabled,
            script_cache_enabled,
            script_cache_shards,
            help,
        })
    }

    pub fn help_text() -> &'static str {
        "Usage: phrust-server --docroot <path> [options]\n\
\n\
Options:\n\
  --listen <addr>              TCP listen address (default: 127.0.0.1:8080)\n\
  --docroot <path>             document root (required unless --help)\n\
  --index <name>               directory index file name (default: index.php)\n\
  --front-controller <path>    optional front controller, relative to docroot\n\
  --max-body-bytes <n>         maximum request body bytes (default: 1048576)\n\
  --upload-temp-dir <path>     upload temp directory (default: OS temp/phrust-uploads)\n\
  --max-upload-files <n>       maximum uploaded files per request (default: 32)\n\
  --max-upload-file-bytes <n>  maximum bytes per uploaded file (default: max body bytes)\n\
  --max-in-flight <n>          maximum concurrent in-flight requests\n\
  --request-timeout-ms <n>     body read timeout in milliseconds (default: 30000)\n\
  --disable-metrics-endpoint   disable GET /__phrust/metrics\n\
  --no-script-cache            disable process-local compiled script cache\n\
  --script-cache-shards <n>    compiled script cache shard count (default: 16)\n\
  --help                       show this help\n"
    }

    pub fn validated_docroot(&self) -> Result<PathBuf, ConfigError> {
        let docroot = self.docroot.canonicalize().map_err(|error| {
            ConfigError::new(format!(
                "docroot `{}` cannot be canonicalized: {error}",
                self.docroot.display()
            ))
        })?;
        if !docroot.is_dir() {
            return Err(ConfigError::new(format!(
                "docroot `{}` is not a directory",
                docroot.display()
            )));
        }
        Ok(docroot)
    }
}

fn required_value(
    flag: &str,
    args: &mut impl Iterator<Item = String>,
) -> Result<String, ConfigError> {
    args.next()
        .ok_or_else(|| ConfigError::new(format!("{flag} requires a value")))
}

fn parse_listen(value: &str) -> Result<SocketAddr, ConfigError> {
    value
        .parse()
        .map_err(|error| ConfigError::new(format!("invalid --listen `{value}`: {error}")))
}

fn parse_positive_usize(flag: &str, value: &str) -> Result<usize, ConfigError> {
    let parsed = value
        .parse::<usize>()
        .map_err(|error| ConfigError::new(format!("invalid {flag} `{value}`: {error}")))?;
    if parsed == 0 {
        return Err(ConfigError::new(format!(
            "{flag} must be greater than zero"
        )));
    }
    Ok(parsed)
}

fn parse_positive_u64(flag: &str, value: &str) -> Result<u64, ConfigError> {
    let parsed = value
        .parse::<u64>()
        .map_err(|error| ConfigError::new(format!("invalid {flag} `{value}`: {error}")))?;
    if parsed == 0 {
        return Err(ConfigError::new(format!(
            "{flag} must be greater than zero"
        )));
    }
    Ok(parsed)
}

fn validate_index(index: &str) -> Result<(), ConfigError> {
    if index.is_empty() || index.contains('/') || index.contains('\\') || index.contains('\0') {
        return Err(ConfigError::new("--index must be a file name"));
    }
    Ok(())
}

fn validate_relative_path(flag: &str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(ConfigError::new(format!(
            "{flag} must be relative to docroot"
        )));
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ConfigError::new(format!("{flag} must not contain `..`")));
    }
    Ok(())
}

fn default_max_in_flight() -> usize {
    std::thread::available_parallelism().map_or(256, |count| count.get().saturating_mul(256))
}

#[cfg(test)]
mod tests {
    use super::ServerConfig;
    use std::{net::SocketAddr, path::PathBuf};

    #[test]
    fn parses_required_docroot_and_defaults() {
        let config = ServerConfig::parse_from(["--docroot", "public"]).unwrap();

        assert_eq!(
            config.listen,
            "127.0.0.1:8080".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(config.docroot, PathBuf::from("public"));
        assert_eq!(config.index, "index.php");
        assert_eq!(config.max_body_bytes, 1_048_576);
        assert_eq!(
            config.upload_temp_dir,
            std::env::temp_dir().join("phrust-uploads")
        );
        assert_eq!(config.max_upload_files, 32);
        assert_eq!(config.max_upload_file_bytes, 1_048_576);
        assert_eq!(config.request_timeout_ms, 30_000);
        assert!(config.metrics_endpoint_enabled);
        assert!(config.script_cache_enabled);
        assert_eq!(config.script_cache_shards, 16);
        assert!(config.front_controller.is_none());
        assert!(!config.help);
        assert!(config.max_in_flight > 0);
    }

    #[test]
    fn parses_all_options() {
        let config = ServerConfig::parse_from([
            "--listen",
            "127.0.0.1:0",
            "--docroot",
            "public",
            "--index",
            "home.php",
            "--front-controller",
            "index.php",
            "--max-body-bytes",
            "64",
            "--upload-temp-dir",
            "uploads",
            "--max-upload-files",
            "4",
            "--max-upload-file-bytes",
            "32",
            "--max-in-flight",
            "2",
            "--request-timeout-ms",
            "250",
            "--disable-metrics-endpoint",
            "--no-script-cache",
            "--script-cache-shards",
            "3",
        ])
        .unwrap();

        assert_eq!(config.listen, "127.0.0.1:0".parse::<SocketAddr>().unwrap());
        assert_eq!(config.index, "home.php");
        assert_eq!(config.front_controller, Some(PathBuf::from("index.php")));
        assert_eq!(config.max_body_bytes, 64);
        assert_eq!(config.upload_temp_dir, PathBuf::from("uploads"));
        assert_eq!(config.max_upload_files, 4);
        assert_eq!(config.max_upload_file_bytes, 32);
        assert_eq!(config.max_in_flight, 2);
        assert_eq!(config.request_timeout_ms, 250);
        assert!(!config.metrics_endpoint_enabled);
        assert!(!config.script_cache_enabled);
        assert_eq!(config.script_cache_shards, 3);
    }

    #[test]
    fn help_does_not_require_docroot() {
        let config = ServerConfig::parse_from(["--help"]).unwrap();

        assert!(config.help);
    }

    #[test]
    fn rejects_missing_docroot_without_help() {
        let error = ServerConfig::parse_from(["--listen", "127.0.0.1:0"]).unwrap_err();

        assert_eq!(error.to_string(), "--docroot is required");
    }

    #[test]
    fn rejects_unknown_flag() {
        let error = ServerConfig::parse_from(["--docroot", "public", "--wat"]).unwrap_err();

        assert_eq!(error.to_string(), "unknown flag `--wat`");
    }

    #[test]
    fn rejects_zero_limits() {
        let error =
            ServerConfig::parse_from(["--docroot", "public", "--max-in-flight", "0"]).unwrap_err();

        assert_eq!(
            error.to_string(),
            "--max-in-flight must be greater than zero"
        );

        let error =
            ServerConfig::parse_from(["--docroot", "public", "--max-body-bytes", "0"]).unwrap_err();

        assert_eq!(
            error.to_string(),
            "--max-body-bytes must be greater than zero"
        );

        let error = ServerConfig::parse_from(["--docroot", "public", "--max-upload-files", "0"])
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--max-upload-files must be greater than zero"
        );

        let error =
            ServerConfig::parse_from(["--docroot", "public", "--max-upload-file-bytes", "0"])
                .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--max-upload-file-bytes must be greater than zero"
        );

        let error = ServerConfig::parse_from(["--docroot", "public", "--request-timeout-ms", "0"])
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--request-timeout-ms must be greater than zero"
        );
    }
}
