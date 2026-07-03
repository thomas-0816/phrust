use std::{
    collections::HashMap,
    env, fmt, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use php_diagnostics::{
    DiagnosticEnvelope, DiagnosticLayer, DiagnosticOutputFormat, DiagnosticPhase,
    DiagnosticSeverity, DiagnosticSuggestion,
};
use php_executor::EngineProfileName;

const DEFAULT_LISTEN: &str = "127.0.0.1:8080";
const DEFAULT_INDEX: &str = "index.php";
const DEFAULT_MAX_BODY_BYTES: usize = 1_048_576;
const DEFAULT_MAX_UPLOAD_FILES: usize = 32;
const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_MAX_EXECUTION_MS: u64 = 30_000;
const DEFAULT_MAX_VM_STEPS: usize = 100_000;
const DEFAULT_SCRIPT_CACHE_SHARDS: usize = 16;
const DEFAULT_SCRIPT_CACHE_MAX_ENTRIES: usize = 4096;
const DEFAULT_SCRIPT_CACHE_CHECK_INTERVAL_MS: u64 = 0;
const DEFAULT_SESSION_COOKIE_NAME: &str = "PHPSESSID";
const DEFAULT_SESSION_COOKIE_PATH: &str = "/";
const DEFAULT_MAX_IN_FLIGHT: usize = 200;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    pub listen: SocketAddr,
    pub docroot: PathBuf,
    pub debug: bool,
    pub error_format: DiagnosticOutputFormat,
    pub debug_log: Option<PathBuf>,
    pub index: String,
    pub front_controller: Option<PathBuf>,
    pub max_body_bytes: usize,
    pub upload_temp_dir: PathBuf,
    pub max_upload_files: usize,
    pub max_upload_file_bytes: usize,
    pub session_save_path: PathBuf,
    pub session_cookie_name: String,
    pub session_cookie_path: String,
    pub sessions_enabled: bool,
    pub max_in_flight: usize,
    pub request_timeout_ms: u64,
    pub max_execution_ms: u64,
    pub max_vm_steps: usize,
    pub execution_deadline_enabled: bool,
    pub engine_preset: EngineProfileName,
    pub metrics_endpoint_enabled: bool,
    pub metrics_token: Option<String>,
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
    pub script_cache_enabled: bool,
    pub script_cache_shards: usize,
    pub script_cache_max_entries: usize,
    pub script_cache_preload: Option<PathBuf>,
    pub script_cache_check_interval_ms: u64,
    pub strict_preload: bool,
    pub cache_clear_endpoint_enabled: bool,
    pub access_log: Option<String>,
    pub perf_trace: Option<PathBuf>,
    pub perf_trace_vm_counters: bool,
    pub help: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigError {
    message: String,
    diagnostic: Box<DiagnosticEnvelope>,
}

impl ConfigError {
    fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        let mut diagnostic = DiagnosticEnvelope::new(
            "E_PHRUST_SERVER_CONFIG",
            DiagnosticLayer::server(),
            DiagnosticPhase::new("config"),
            DiagnosticSeverity::Error,
            message.clone(),
        );
        diagnostic.suggestion = Some(DiagnosticSuggestion::new(
            "run phrust-server --help and check the configured flag or path",
        ));
        Self {
            message,
            diagnostic: Box::new(diagnostic),
        }
    }

    pub fn diagnostic(&self) -> &DiagnosticEnvelope {
        self.diagnostic.as_ref()
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
        Self::parse_from_with_env(args, |name| env::var(name).ok())
    }

    fn parse_from_with_env<I, S, F>(args: I, env_value: F) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        F: Fn(&str) -> Option<String>,
    {
        let raw_args = args.into_iter().map(Into::into).collect::<Vec<_>>();
        let help_requested = raw_args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--help" | "-h"));
        let file_config = if help_requested {
            FileConfig::default()
        } else if let Some(path) = config_path_from_args(&raw_args)? {
            FileConfig::load(&path)?
        } else {
            FileConfig::default()
        };

        let mut listen = file_config
            .parse_listen("listen")?
            .unwrap_or(parse_listen(DEFAULT_LISTEN)?);
        let mut docroot = file_config.path("docroot");
        let mut index = file_config
            .string("index")
            .unwrap_or_else(|| DEFAULT_INDEX.to_string());
        let mut front_controller = file_config.path("front_controller");
        let mut max_body_bytes = file_config
            .positive_usize("max_body_bytes")?
            .unwrap_or(DEFAULT_MAX_BODY_BYTES);
        let mut upload_temp_dir = file_config
            .path("upload_temp_dir")
            .unwrap_or_else(|| std::env::temp_dir().join("phrust-uploads"));
        let mut max_upload_files = file_config
            .positive_usize("max_upload_files")?
            .unwrap_or(DEFAULT_MAX_UPLOAD_FILES);
        let mut max_upload_file_bytes = file_config.positive_usize("max_upload_file_bytes")?;
        let mut session_save_path = file_config
            .path("session_save_path")
            .unwrap_or_else(|| std::env::temp_dir().join("phrust-sessions"));
        let mut session_cookie_name = file_config
            .string("session_cookie_name")
            .unwrap_or_else(|| DEFAULT_SESSION_COOKIE_NAME.to_string());
        let mut session_cookie_path = file_config
            .string("session_cookie_path")
            .unwrap_or_else(|| DEFAULT_SESSION_COOKIE_PATH.to_string());
        let mut sessions_enabled = file_config.bool("sessions_enabled")?.unwrap_or(true);
        let mut max_in_flight = file_config
            .positive_usize("max_in_flight")?
            .unwrap_or_else(default_max_in_flight);
        let mut request_timeout_ms = file_config
            .positive_u64("request_timeout_ms")?
            .unwrap_or(DEFAULT_REQUEST_TIMEOUT_MS);
        let mut max_execution_ms = file_config
            .positive_u64("max_execution_ms")?
            .unwrap_or(DEFAULT_MAX_EXECUTION_MS);
        let mut max_vm_steps = file_config
            .positive_usize("max_vm_steps")?
            .unwrap_or(DEFAULT_MAX_VM_STEPS);
        let mut execution_deadline_enabled = file_config
            .bool("execution_deadline_enabled")?
            .unwrap_or(true);
        let mut engine_preset = file_config
            .string("engine_preset")
            .map(|value| parse_engine_preset("engine_preset", &value))
            .transpose()?
            .unwrap_or_default();
        let mut metrics_endpoint_enabled = file_config
            .bool("metrics_endpoint_enabled")?
            .unwrap_or(true);
        let mut metrics_token = file_config.string("metrics_token");
        let mut tls_cert = file_config.path("tls_cert");
        let mut tls_key = file_config.path("tls_key");
        let mut script_cache_enabled = file_config.bool("script_cache_enabled")?.unwrap_or(true);
        let mut script_cache_shards = file_config
            .positive_usize("script_cache_shards")?
            .unwrap_or(DEFAULT_SCRIPT_CACHE_SHARDS);
        let mut script_cache_max_entries = file_config
            .positive_usize("script_cache_max_entries")?
            .unwrap_or(DEFAULT_SCRIPT_CACHE_MAX_ENTRIES);
        let mut script_cache_preload = file_config.path("script_cache_preload");
        let mut script_cache_check_interval_ms = file_config
            .nonnegative_u64("script_cache_check_interval_ms")?
            .unwrap_or(DEFAULT_SCRIPT_CACHE_CHECK_INTERVAL_MS);
        let mut strict_preload = file_config.bool("strict_preload")?.unwrap_or(false);
        let mut cache_clear_endpoint_enabled = file_config
            .bool("cache_clear_endpoint_enabled")?
            .unwrap_or(false);
        let mut access_log = file_config.string("access_log");
        let mut perf_trace = file_config
            .path("perf_trace")
            .or_else(|| env_perf_trace_path(&env_value));
        let mut perf_trace_vm_counters = file_config
            .bool("perf_trace_vm_counters")?
            .unwrap_or_else(|| env_bool(&env_value, "PHRUST_SERVER_PERF_TRACE_VM_COUNTERS"));
        let mut debug = env_bool(&env_value, "PHRUST_SERVER_DEBUG");
        let mut error_format = env_output_format(&env_value, "PHRUST_SERVER_ERROR_FORMAT")?;
        let mut debug_log = env_value("PHRUST_SERVER_DEBUG_LOG")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let mut help = false;
        let mut args = raw_args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => help = true,
                "--config" => {
                    let _ = required_value(&arg, &mut args)?;
                }
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
                "--session-save-path" => {
                    session_save_path = PathBuf::from(required_value(&arg, &mut args)?);
                }
                "--session-cookie-name" => {
                    session_cookie_name = required_value(&arg, &mut args)?;
                    validate_cookie_name("--session-cookie-name", &session_cookie_name)?;
                }
                "--session-cookie-path" => {
                    session_cookie_path = required_value(&arg, &mut args)?;
                    validate_cookie_path("--session-cookie-path", &session_cookie_path)?;
                }
                "--disable-sessions" => sessions_enabled = false,
                "--max-in-flight" => {
                    max_in_flight = parse_positive_usize(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--request-timeout-ms" => {
                    request_timeout_ms =
                        parse_positive_u64(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--max-execution-ms" => {
                    max_execution_ms = parse_positive_u64(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--max-vm-steps" => {
                    max_vm_steps = parse_positive_usize(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--disable-execution-deadline" => execution_deadline_enabled = false,
                "--engine-preset" => {
                    engine_preset = parse_engine_preset(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--disable-metrics-endpoint" => metrics_endpoint_enabled = false,
                "--metrics-token" => {
                    metrics_token = Some(required_value(&arg, &mut args)?);
                }
                "--tls-cert" => tls_cert = Some(PathBuf::from(required_value(&arg, &mut args)?)),
                "--tls-key" => tls_key = Some(PathBuf::from(required_value(&arg, &mut args)?)),
                "--no-script-cache" => script_cache_enabled = false,
                "--script-cache-shards" => {
                    script_cache_shards =
                        parse_positive_usize(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--script-cache-max-entries" => {
                    script_cache_max_entries =
                        parse_positive_usize(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--script-cache-preload" => {
                    script_cache_preload = Some(PathBuf::from(required_value(&arg, &mut args)?));
                }
                "--script-cache-check-interval-ms" => {
                    script_cache_check_interval_ms =
                        parse_nonnegative_u64(&arg, &required_value(&arg, &mut args)?)?;
                }
                "--strict-preload" => strict_preload = true,
                "--enable-cache-clear-endpoint" => cache_clear_endpoint_enabled = true,
                "--access-log" => access_log = Some(required_value(&arg, &mut args)?),
                "--perf-trace" => {
                    perf_trace = Some(PathBuf::from(required_value(&arg, &mut args)?))
                }
                "--perf-trace-vm-counters" => perf_trace_vm_counters = true,
                "--debug" => debug = true,
                "--error-format" => {
                    error_format = parse_output_format(&required_value(&arg, &mut args)?)?;
                }
                "--debug-log" => debug_log = Some(PathBuf::from(required_value(&arg, &mut args)?)),
                _ if arg.starts_with('-') => {
                    return Err(ConfigError::new(format!(
                        "unknown flag `{arg}`; accepted flags include --docroot, --listen, --debug, --error-format, and --help"
                    )));
                }
                _ => return Err(ConfigError::new(format!("unexpected argument `{arg}`"))),
            }
        }

        validate_index(&index)?;
        if let Some(path) = &front_controller {
            validate_relative_path("front_controller", path)?;
        }
        validate_cookie_name("session_cookie_name", &session_cookie_name)?;
        validate_cookie_path("session_cookie_path", &session_cookie_path)?;
        if tls_cert.is_some() != tls_key.is_some() {
            return Err(ConfigError::new(
                "TLS configuration requires both --tls-cert <path> and --tls-key <path>; provide both flags or neither",
            ));
        }

        if help {
            return Ok(Self {
                listen,
                docroot: docroot.unwrap_or_default(),
                debug,
                error_format,
                debug_log,
                index,
                front_controller,
                max_body_bytes,
                upload_temp_dir,
                max_upload_files,
                max_upload_file_bytes: max_upload_file_bytes.unwrap_or(max_body_bytes),
                session_save_path,
                session_cookie_name,
                session_cookie_path,
                sessions_enabled,
                max_in_flight,
                request_timeout_ms,
                max_execution_ms,
                max_vm_steps,
                execution_deadline_enabled,
                engine_preset,
                metrics_endpoint_enabled,
                metrics_token,
                tls_cert,
                tls_key,
                script_cache_enabled,
                script_cache_shards,
                script_cache_max_entries,
                script_cache_preload,
                script_cache_check_interval_ms,
                strict_preload,
                cache_clear_endpoint_enabled,
                access_log,
                perf_trace,
                perf_trace_vm_counters,
                help,
            });
        }

        let docroot = docroot.ok_or_else(|| {
            ConfigError::new("--docroot is required; example: phrust-server --docroot public")
        })?;
        Ok(Self {
            listen,
            docroot,
            debug,
            error_format,
            debug_log,
            index,
            front_controller,
            max_body_bytes,
            upload_temp_dir,
            max_upload_files,
            max_upload_file_bytes: max_upload_file_bytes.unwrap_or(max_body_bytes),
            session_save_path,
            session_cookie_name,
            session_cookie_path,
            sessions_enabled,
            max_in_flight,
            request_timeout_ms,
            max_execution_ms,
            max_vm_steps,
            execution_deadline_enabled,
            engine_preset,
            metrics_endpoint_enabled,
            metrics_token,
            tls_cert,
            tls_key,
            script_cache_enabled,
            script_cache_shards,
            script_cache_max_entries,
            script_cache_preload,
            script_cache_check_interval_ms,
            strict_preload,
            cache_clear_endpoint_enabled,
            access_log,
            perf_trace,
            perf_trace_vm_counters,
            help,
        })
    }

    pub fn help_text() -> &'static str {
        "Usage: phrust-server --docroot <path> [options]\n\
\n\
Options:\n\
  --listen <addr>              TCP listen address (default: 127.0.0.1:8080)\n\
  --config <path>              read simple TOML-style server config\n\
  --docroot <path>             document root (required unless --help)\n\
  --index <name>               directory index file name (default: index.php)\n\
  --front-controller <path>    optional front controller, relative to docroot\n\
  --max-body-bytes <n>         maximum request body bytes (default: 1048576)\n\
  --upload-temp-dir <path>     upload temp directory (default: OS temp/phrust-uploads)\n\
  --max-upload-files <n>       maximum uploaded files per request (default: 32)\n\
  --max-upload-file-bytes <n>  maximum bytes per uploaded file (default: max body bytes)\n\
  --session-save-path <path>   compatibility path for session config\n\
  --session-cookie-name <name> session cookie name (default: PHPSESSID)\n\
  --session-cookie-path <path> session cookie path (default: /)\n\
  --disable-sessions           disable persistent web sessions\n\
  --max-in-flight <n>          maximum concurrent in-flight requests\n\
  --request-timeout-ms <n>     body read timeout in milliseconds (default: 30000)\n\
  --max-execution-ms <n>       PHP execution deadline in milliseconds (default: 30000)\n\
  --max-vm-steps <n>           maximum VM dispatches per request (default: 100000)\n\
  --disable-execution-deadline disable cooperative PHP execution deadline\n\
  --engine-preset <name>       default managed-fast, baseline oracle, fast alias, or experimental-jit diagnostics\n\
  --disable-metrics-endpoint   disable GET /__phrust/metrics\n\
  --metrics-token <token>      require Authorization: Bearer token for metrics\n\
  --tls-cert <path>            PEM certificate chain for HTTPS\n\
  --tls-key <path>             PEM private key for HTTPS\n\
  --access-log <path|->        write compact access logs to file or stdout\n\
  --perf-trace <path>          append per-PHP-request performance trace JSONL\n\
  --perf-trace-vm-counters     include heavy VM counters in perf trace rows\n\
  --debug                      emit structured server debug events to stderr\n\
  --error-format <text|json>   render server diagnostics/debug events as text or JSON\n\
  --debug-log <path>           append server debug events to a file instead of stderr\n\
  --no-script-cache            disable process-local compiled script cache\n\
  --script-cache-shards <n>    compiled script cache shard count (default: 16)\n\
  --script-cache-max-entries <n> maximum compiled script cache entries (default: 4096)\n\
  --script-cache-preload <file> preload newline-delimited script paths at startup\n\
  --script-cache-check-interval-ms <n> skip stat checks for this many milliseconds (default: 0)\n\
  --strict-preload             fail startup when preload entries cannot compile\n\
  --enable-cache-clear-endpoint enable loopback-only POST /__phrust/cache/clear\n\
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
    args.next().ok_or_else(|| {
        ConfigError::new(format!(
            "{flag} requires a value placeholder, for example {flag} <value>"
        ))
    })
}

fn config_path_from_args(args: &[String]) -> Result<Option<PathBuf>, ConfigError> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--config" {
            let Some(path) = iter.next() else {
                return Err(ConfigError::new(
                    "--config requires a value placeholder, for example --config <path>",
                ));
            };
            return Ok(Some(PathBuf::from(path)));
        }
    }
    Ok(None)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct FileConfig {
    values: HashMap<String, String>,
}

impl FileConfig {
    fn load(path: &Path) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path).map_err(|error| {
            ConfigError::new(format!(
                "config `{}` cannot be read: {error}",
                path.display()
            ))
        })?;
        let mut values = HashMap::new();
        for (line_index, line) in contents.lines().enumerate() {
            let line = strip_config_comment(line).trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ConfigError::new(format!(
                    "config `{}` line {} must use key = value",
                    path.display(),
                    line_index + 1
                )));
            };
            let key = normalize_config_key(key.trim());
            let value = parse_config_value(value.trim()).map_err(|message| {
                ConfigError::new(format!(
                    "config `{}` line {} {message}",
                    path.display(),
                    line_index + 1
                ))
            })?;
            values.insert(key, value);
        }
        Ok(Self { values })
    }

    fn string(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn path(&self, key: &str) -> Option<PathBuf> {
        self.string(key).map(PathBuf::from)
    }

    fn parse_listen(&self, key: &str) -> Result<Option<SocketAddr>, ConfigError> {
        self.values
            .get(key)
            .map(|value| parse_listen(value))
            .transpose()
    }

    fn positive_usize(&self, key: &str) -> Result<Option<usize>, ConfigError> {
        self.values
            .get(key)
            .map(|value| parse_positive_usize(key, value))
            .transpose()
    }

    fn positive_u64(&self, key: &str) -> Result<Option<u64>, ConfigError> {
        self.values
            .get(key)
            .map(|value| parse_positive_u64(key, value))
            .transpose()
    }

    fn nonnegative_u64(&self, key: &str) -> Result<Option<u64>, ConfigError> {
        self.values
            .get(key)
            .map(|value| parse_nonnegative_u64(key, value))
            .transpose()
    }

    fn bool(&self, key: &str) -> Result<Option<bool>, ConfigError> {
        self.values
            .get(key)
            .map(|value| match value.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(ConfigError::new(format!(
                    "{key} must be true or false in config"
                ))),
            })
            .transpose()
    }
}

fn normalize_config_key(key: &str) -> String {
    key.replace('-', "_")
}

fn strip_config_comment(line: &str) -> &str {
    let mut in_quote = false;
    for (index, byte) in line.bytes().enumerate() {
        match byte {
            b'"' => in_quote = !in_quote,
            b'#' if !in_quote => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_config_value(value: &str) -> Result<String, &'static str> {
    if let Some(value) = value.strip_prefix('"') {
        let Some(value) = value.strip_suffix('"') else {
            return Err("has an unterminated quoted value");
        };
        return Ok(value.replace("\\\"", "\"").replace("\\\\", "\\"));
    }
    if value.is_empty() {
        return Err("has an empty value");
    }
    Ok(value.to_string())
}

fn parse_listen(value: &str) -> Result<SocketAddr, ConfigError> {
    value.parse().map_err(|error| {
        ConfigError::new(format!(
            "invalid --listen `{value}`: {error}; expected host:port such as 127.0.0.1:8080"
        ))
    })
}

fn env_bool(env_value: &impl Fn(&str) -> Option<String>, name: &str) -> bool {
    env_value(name)
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "on"))
}

fn env_perf_trace_path(env_value: &impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let value = env_value("PHRUST_PERF_TRACE")?;
    let value = value.trim();
    if value.is_empty() || matches!(value, "0" | "false" | "FALSE" | "off") {
        None
    } else if matches!(value, "1" | "true" | "TRUE" | "yes" | "on") {
        Some(PathBuf::from("target/performance/server/perf-trace.jsonl"))
    } else {
        Some(PathBuf::from(value))
    }
}

fn env_output_format(
    env_value: &impl Fn(&str) -> Option<String>,
    name: &str,
) -> Result<DiagnosticOutputFormat, ConfigError> {
    env_value(name)
        .map(|value| parse_output_format(&value))
        .transpose()
        .map(|value| value.unwrap_or(DiagnosticOutputFormat::Text))
}

fn parse_output_format(value: &str) -> Result<DiagnosticOutputFormat, ConfigError> {
    match value {
        "text" => Ok(DiagnosticOutputFormat::Text),
        "json" | "jsonl" => Ok(DiagnosticOutputFormat::Json),
        _ => Err(ConfigError::new(format!(
            "invalid error format `{value}`; expected text or json"
        ))),
    }
}

fn parse_engine_preset(flag: &str, value: &str) -> Result<EngineProfileName, ConfigError> {
    EngineProfileName::parse(value)
        .map_err(|error| ConfigError::new(format!("invalid {flag}: {error}")))
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

fn parse_nonnegative_u64(flag: &str, value: &str) -> Result<u64, ConfigError> {
    value
        .parse::<u64>()
        .map_err(|error| ConfigError::new(format!("invalid {flag} `{value}`: {error}")))
}

fn validate_index(index: &str) -> Result<(), ConfigError> {
    if index.is_empty() || index.contains('/') || index.contains('\\') || index.contains('\0') {
        return Err(ConfigError::new(
            "--index must be a single file name relative to each directory, not an absolute path or nested path",
        ));
    }
    Ok(())
}

fn validate_cookie_name(flag: &str, name: &str) -> Result<(), ConfigError> {
    if name.is_empty()
        || !name.bytes().all(
            |byte| matches!(byte, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e),
        )
    {
        return Err(ConfigError::new(format!(
            "{flag} must be a valid cookie name"
        )));
    }
    Ok(())
}

fn validate_cookie_path(flag: &str, path: &str) -> Result<(), ConfigError> {
    if path.is_empty() || path.contains(['\r', '\n', ';']) {
        return Err(ConfigError::new(format!(
            "{flag} must be a non-empty cookie path without response separators"
        )));
    }
    Ok(())
}

fn validate_relative_path(flag: &str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(ConfigError::new(format!(
            "{flag} must be a non-empty relative path inside docroot, not an absolute path"
        )));
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ConfigError::new(format!(
            "{flag} must stay inside docroot and must not contain `..`"
        )));
    }
    Ok(())
}

fn default_max_in_flight() -> usize {
    DEFAULT_MAX_IN_FLIGHT
}

#[cfg(test)]
mod tests {
    use super::ServerConfig;
    use php_diagnostics::DiagnosticOutputFormat;
    use php_executor::EngineProfileName;
    use std::{
        collections::HashMap,
        fs,
        net::SocketAddr,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEMP_CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn parses_required_docroot_and_defaults() {
        let config = ServerConfig::parse_from(["--docroot", "public"]).unwrap();

        assert_eq!(
            config.listen,
            "127.0.0.1:8080".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(config.docroot, PathBuf::from("public"));
        assert!(!config.debug);
        assert_eq!(config.error_format, DiagnosticOutputFormat::Text);
        assert_eq!(config.debug_log, None);
        assert_eq!(config.index, "index.php");
        assert_eq!(config.max_body_bytes, 1_048_576);
        assert_eq!(
            config.upload_temp_dir,
            std::env::temp_dir().join("phrust-uploads")
        );
        assert_eq!(config.max_upload_files, 32);
        assert_eq!(config.max_upload_file_bytes, 1_048_576);
        assert_eq!(
            config.session_save_path,
            std::env::temp_dir().join("phrust-sessions")
        );
        assert_eq!(config.session_cookie_name, "PHPSESSID");
        assert_eq!(config.session_cookie_path, "/");
        assert!(config.sessions_enabled);
        assert_eq!(config.request_timeout_ms, 30_000);
        assert_eq!(config.max_execution_ms, 30_000);
        assert_eq!(config.max_vm_steps, 100_000);
        assert!(config.execution_deadline_enabled);
        assert_eq!(config.engine_preset, EngineProfileName::Default);
        assert!(config.metrics_endpoint_enabled);
        assert_eq!(config.metrics_token, None);
        assert_eq!(config.tls_cert, None);
        assert_eq!(config.tls_key, None);
        assert_eq!(config.access_log, None);
        assert_eq!(config.perf_trace, None);
        assert!(!config.perf_trace_vm_counters);
        assert!(config.script_cache_enabled);
        assert_eq!(config.script_cache_shards, 16);
        assert_eq!(config.script_cache_max_entries, 4096);
        assert_eq!(config.script_cache_preload, None);
        assert_eq!(config.script_cache_check_interval_ms, 0);
        assert!(!config.strict_preload);
        assert!(!config.cache_clear_endpoint_enabled);
        assert!(config.front_controller.is_none());
        assert!(!config.help);
        assert_eq!(config.max_in_flight, 200);
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
            "--session-save-path",
            "sessions",
            "--session-cookie-name",
            "APPSESSID",
            "--session-cookie-path",
            "/app",
            "--disable-sessions",
            "--max-in-flight",
            "2",
            "--request-timeout-ms",
            "250",
            "--max-execution-ms",
            "125",
            "--max-vm-steps",
            "250000",
            "--disable-execution-deadline",
            "--engine-preset",
            "experimental-jit",
            "--disable-metrics-endpoint",
            "--metrics-token",
            "secret",
            "--tls-cert",
            "tls/cert.pem",
            "--tls-key",
            "tls/key.pem",
            "--access-log",
            "-",
            "--perf-trace",
            "perf.jsonl",
            "--perf-trace-vm-counters",
            "--debug",
            "--error-format",
            "json",
            "--debug-log",
            "debug.log",
            "--no-script-cache",
            "--script-cache-shards",
            "3",
            "--script-cache-max-entries",
            "64",
            "--script-cache-preload",
            "preload.txt",
            "--script-cache-check-interval-ms",
            "25",
            "--strict-preload",
            "--enable-cache-clear-endpoint",
        ])
        .unwrap();

        assert_eq!(config.listen, "127.0.0.1:0".parse::<SocketAddr>().unwrap());
        assert_eq!(config.index, "home.php");
        assert_eq!(config.front_controller, Some(PathBuf::from("index.php")));
        assert_eq!(config.max_body_bytes, 64);
        assert_eq!(config.upload_temp_dir, PathBuf::from("uploads"));
        assert_eq!(config.max_upload_files, 4);
        assert_eq!(config.max_upload_file_bytes, 32);
        assert_eq!(config.session_save_path, PathBuf::from("sessions"));
        assert_eq!(config.session_cookie_name, "APPSESSID");
        assert_eq!(config.session_cookie_path, "/app");
        assert!(!config.sessions_enabled);
        assert_eq!(config.max_in_flight, 2);
        assert_eq!(config.request_timeout_ms, 250);
        assert_eq!(config.max_execution_ms, 125);
        assert_eq!(config.max_vm_steps, 250_000);
        assert!(!config.execution_deadline_enabled);
        assert_eq!(config.engine_preset, EngineProfileName::ExperimentalJit);
        assert!(!config.metrics_endpoint_enabled);
        assert_eq!(config.metrics_token, Some("secret".to_string()));
        assert_eq!(config.tls_cert, Some(PathBuf::from("tls/cert.pem")));
        assert_eq!(config.tls_key, Some(PathBuf::from("tls/key.pem")));
        assert_eq!(config.access_log, Some("-".to_string()));
        assert_eq!(config.perf_trace, Some(PathBuf::from("perf.jsonl")));
        assert!(config.perf_trace_vm_counters);
        assert!(config.debug);
        assert_eq!(config.error_format, DiagnosticOutputFormat::Json);
        assert_eq!(config.debug_log, Some(PathBuf::from("debug.log")));
        assert!(!config.script_cache_enabled);
        assert_eq!(config.script_cache_shards, 3);
        assert_eq!(config.script_cache_max_entries, 64);
        assert_eq!(
            config.script_cache_preload,
            Some(PathBuf::from("preload.txt"))
        );
        assert_eq!(config.script_cache_check_interval_ms, 25);
        assert!(config.strict_preload);
        assert!(config.cache_clear_endpoint_enabled);
    }

    #[test]
    fn help_does_not_require_docroot() {
        let config = ServerConfig::parse_from(["--help"]).unwrap();

        assert!(config.help);
    }

    #[test]
    fn parses_config_file_and_cli_overrides_values() {
        let path = temp_config(
            r#"
listen = "127.0.0.1:9000"
docroot = "from-file"
index = "home.php"
max_body_bytes = 64
metrics_token = "from-file-token"
access_log = "access.log"
tls_cert = "cert.pem"
tls_key = "key.pem"
script_cache_max_entries = 12
strict_preload = true
engine_preset = "baseline"
max_vm_steps = 333000
"#,
        );

        let config = ServerConfig::parse_from([
            "--config",
            path.to_str().unwrap(),
            "--docroot",
            "from-cli",
            "--max-body-bytes",
            "128",
            "--metrics-token",
            "from-cli-token",
            "--engine-preset",
            "fast",
        ])
        .unwrap();

        fs::remove_file(path).expect("remove config");

        assert_eq!(
            config.listen,
            "127.0.0.1:9000".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(config.docroot, PathBuf::from("from-cli"));
        assert_eq!(config.index, "home.php");
        assert_eq!(config.max_body_bytes, 128);
        assert_eq!(config.metrics_token, Some("from-cli-token".to_string()));
        assert_eq!(config.access_log, Some("access.log".to_string()));
        assert_eq!(config.tls_cert, Some(PathBuf::from("cert.pem")));
        assert_eq!(config.tls_key, Some(PathBuf::from("key.pem")));
        assert_eq!(config.script_cache_max_entries, 12);
        assert!(config.strict_preload);
        assert_eq!(config.engine_preset, EngineProfileName::Default);
        assert_eq!(config.max_vm_steps, 333_000);
    }

    #[test]
    fn cli_override_replaces_invalid_file_value_before_final_validation() {
        let path = temp_config(
            r#"
docroot = "from-file"
index = "../bad.php"
"#,
        );

        let config =
            ServerConfig::parse_from(["--config", path.to_str().unwrap(), "--index", "index.php"])
                .unwrap();

        fs::remove_file(path).expect("remove config");

        assert_eq!(config.index, "index.php");
    }

    #[test]
    fn rejects_missing_docroot_without_help() {
        let error = ServerConfig::parse_from(["--listen", "127.0.0.1:0"]).unwrap_err();

        assert_eq!(
            error.to_string(),
            "--docroot is required; example: phrust-server --docroot public"
        );
        assert_eq!(error.diagnostic().code, "E_PHRUST_SERVER_CONFIG");
    }

    #[test]
    fn rejects_invalid_listen_address_with_expected_format() {
        let error =
            ServerConfig::parse_from(["--listen", "no-port", "--docroot", "public"]).unwrap_err();

        assert!(error.to_string().contains("invalid --listen `no-port`"));
        assert!(error.to_string().contains("expected host:port"));
    }

    #[test]
    fn rejects_unknown_flag() {
        let error = ServerConfig::parse_from(["--docroot", "public", "--wat"]).unwrap_err();

        assert!(error.to_string().contains("unknown flag `--wat`"));
        assert!(error.to_string().contains("accepted flags"));
    }

    #[test]
    fn parses_debug_env_vars() {
        let env = HashMap::from([
            ("PHRUST_SERVER_DEBUG", "1"),
            ("PHRUST_SERVER_ERROR_FORMAT", "json"),
            ("PHRUST_SERVER_DEBUG_LOG", "server-debug.log"),
        ]);
        let config = ServerConfig::parse_from_with_env(["--docroot", "public"], |name| {
            env.get(name).map(|value| (*value).to_string())
        })
        .unwrap();

        assert!(config.debug);
        assert_eq!(config.error_format, DiagnosticOutputFormat::Json);
        assert_eq!(config.debug_log, Some(PathBuf::from("server-debug.log")));
    }

    #[test]
    fn cli_debug_flags_override_env_vars() {
        let env = HashMap::from([
            ("PHRUST_SERVER_DEBUG", "1"),
            ("PHRUST_SERVER_ERROR_FORMAT", "text"),
            ("PHRUST_SERVER_DEBUG_LOG", "env-debug.log"),
        ]);
        let config = ServerConfig::parse_from_with_env(
            [
                "--docroot",
                "public",
                "--error-format",
                "json",
                "--debug-log",
                "cli-debug.log",
            ],
            |name| env.get(name).map(|value| (*value).to_string()),
        )
        .unwrap();

        assert!(config.debug);
        assert_eq!(config.error_format, DiagnosticOutputFormat::Json);
        assert_eq!(config.debug_log, Some(PathBuf::from("cli-debug.log")));
    }

    #[test]
    fn rejects_invalid_error_format() {
        let error = ServerConfig::parse_from(["--docroot", "public", "--error-format", "yaml"])
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid error format `yaml`; expected text or json"
        );
    }

    #[test]
    fn rejects_invalid_engine_preset() {
        let error = ServerConfig::parse_from(["--docroot", "public", "--engine-preset", "turbo"])
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid --engine-preset: unsupported engine preset `turbo`; expected baseline, default, fast, or experimental-jit"
        );
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

        let error = ServerConfig::parse_from(["--docroot", "public", "--max-execution-ms", "0"])
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--max-execution-ms must be greater than zero"
        );

        let error = ServerConfig::parse_from(["--docroot", "public", "--script-cache-shards", "0"])
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--script-cache-shards must be greater than zero"
        );

        let error =
            ServerConfig::parse_from(["--docroot", "public", "--script-cache-max-entries", "0"])
                .unwrap_err();

        assert_eq!(
            error.to_string(),
            "--script-cache-max-entries must be greater than zero"
        );
    }

    #[test]
    fn rejects_invalid_session_cookie_settings() {
        let error =
            ServerConfig::parse_from(["--docroot", "public", "--session-cookie-name", "bad name"])
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "--session-cookie-name must be a valid cookie name"
        );

        let error =
            ServerConfig::parse_from(["--docroot", "public", "--session-cookie-path", "/;\n"])
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "--session-cookie-path must be a non-empty cookie path without response separators"
        );
    }

    #[test]
    fn rejects_incomplete_tls_pair() {
        let error = ServerConfig::parse_from(["--docroot", "public", "--tls-cert", "cert.pem"])
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "TLS configuration requires both --tls-cert <path> and --tls-key <path>; provide both flags or neither"
        );
    }

    fn temp_config(contents: &str) -> PathBuf {
        let unique = TEMP_CONFIG_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "phrust-server-config-{}-{unique}.toml",
            std::process::id()
        ));
        fs::write(&path, contents).expect("write config");
        path
    }
}
