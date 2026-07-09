use crate::engine::{CliIniOptions, EngineInput, execute_php, lint_php, read_script};
use php_diagnostics::DiagnosticOutputFormat;
use php_runtime::api::{
    PHP_E_DEPRECATED, PHP_E_ERROR, PHP_E_NOTICE, PHP_E_USER_DEPRECATED, PHP_E_USER_ERROR,
    PHP_E_USER_NOTICE, PHP_E_USER_WARNING, PHP_E_WARNING, PhpDiagnosticChannel,
    PhpDiagnosticLocation, RuntimeInputFilter, error_reporting_allows_level,
    format_php_diagnostic_line,
};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const EXIT_SUCCESS: i32 = 0;
const EXIT_PHP_ERROR: i32 = 255;
const PHP_E_PARSE: i64 = 4;
const PHP_E_CORE_ERROR: i64 = 16;
const PHP_E_CORE_WARNING: i64 = 32;
const PHP_E_COMPILE_ERROR: i64 = 64;
const PHP_E_COMPILE_WARNING: i64 = 128;
const PHP_E_STRICT: i64 = 2048;
const PHP_E_RECOVERABLE_ERROR: i64 = 4096;
const PHP_E_ALL: i64 = 30719;

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedCli {
    action: CliAction,
    no_ini: bool,
    defines: Vec<(String, String)>,
    config_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CliAction {
    Help,
    Version,
    ShowIni,
    ListModules,
    PhpInfo,
    UnsupportedIntrospection {
        flag: String,
    },
    LintFile {
        path: PathBuf,
    },
    RunCode {
        code: String,
        args: Vec<String>,
    },
    RunFile {
        path: PathBuf,
        args: Vec<String>,
    },
    RunStdin {
        args: Vec<String>,
    },
    Serve {
        listen: String,
        docroot: Option<PathBuf>,
        router: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LoadedIni {
    path: Option<PathBuf>,
    directives: Vec<(String, String)>,
    disabled: bool,
}

pub fn run<I, R, W, E>(args: I, stdin: &mut R, stdout: &mut W, stderr: &mut E) -> i32
where
    I: IntoIterator<Item = String>,
    R: Read,
    W: Write,
    E: Write,
{
    run_with_terminal(args, stdin, false, stdout, stderr)
}

pub fn run_with_terminal<I, R, W, E>(
    args: I,
    stdin: &mut R,
    stdin_is_terminal: bool,
    stdout: &mut W,
    stderr: &mut E,
) -> i32
where
    I: IntoIterator<Item = String>,
    R: Read,
    W: Write,
    E: Write,
{
    match run_inner(
        args.into_iter().collect(),
        stdin,
        stdin_is_terminal,
        stdout,
        stderr,
    ) {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            EXIT_PHP_ERROR
        }
    }
}

fn run_inner<R, W, E>(
    args: Vec<String>,
    stdin: &mut R,
    stdin_is_terminal: bool,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String>
where
    R: Read,
    W: Write,
    E: Write,
{
    let parsed = ParsedCli::parse(&args)?;
    let loaded_ini = load_ini(&parsed)?;
    let merged_defines = merged_ini_defines(&loaded_ini, &parsed.defines);
    let ini = ini_options(&merged_defines);
    let debug = debug_enabled_from_env();
    let debug_log = env::var("PHRUST_DEBUG_LOG")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let debug_format = error_format_from_env();
    let php_binary = php_binary_path();
    match parsed.action {
        CliAction::Help => {
            print_usage(stdout)?;
            Ok(EXIT_SUCCESS)
        }
        CliAction::Version => {
            writeln!(
                stdout,
                "PHP {} (phrust-php)",
                php_source::reference_php_version()
            )
            .map_err(|error| error.to_string())?;
            Ok(EXIT_SUCCESS)
        }
        CliAction::ShowIni => {
            print_ini(stdout, &loaded_ini)?;
            Ok(EXIT_SUCCESS)
        }
        CliAction::ListModules => {
            print_modules(stdout)?;
            Ok(EXIT_SUCCESS)
        }
        CliAction::PhpInfo => {
            print_php_info(stdout, &loaded_ini, &php_binary)?;
            Ok(EXIT_SUCCESS)
        }
        CliAction::UnsupportedIntrospection { flag } => Err(format!(
            "E_PHRUST_CLI_UNSUPPORTED_OPTION: {flag} is recognized, but reflection introspection is not implemented"
        )),
        CliAction::LintFile { path } => {
            let (source, _real_path, source_path) = read_script(&path)?;
            lint_php(&source, &source_path, stdout, stderr)
        }
        CliAction::RunCode { code, args } => {
            let source = normalize_command_line_code(&code);
            let input = EngineInput {
                source,
                source_path: "Command line code".to_string(),
                real_path: None,
                script_name: "Command line code".to_string(),
                script_args: args,
                cwd: current_dir()?,
                env: collect_env(),
                ini: ini.clone(),
                stdin: read_stdin_if_piped(stdin, stdin_is_terminal)?,
                php_binary: php_binary.clone(),
                debug,
                debug_log,
                debug_format,
            };
            execute_php(input, stdout, stderr)
        }
        CliAction::RunFile { path, args } => {
            let (source, real_path, source_path) = read_script(&path)?;
            emit_startup_ini_deprecations(stdout, &ini)?;
            let input = EngineInput {
                source,
                source_path,
                real_path: Some(real_path.clone()),
                script_name: real_path.to_string_lossy().into_owned(),
                script_args: args,
                cwd: current_dir()?,
                env: collect_env(),
                ini: ini.clone(),
                stdin: read_stdin_if_piped(stdin, stdin_is_terminal)?,
                php_binary: php_binary.clone(),
                debug,
                debug_log,
                debug_format,
            };
            execute_php(input, stdout, stderr)
        }
        CliAction::RunStdin { args } => {
            let mut source = String::new();
            stdin
                .read_to_string(&mut source)
                .map_err(|error| format!("stdin: {error}"))?;
            let input = EngineInput {
                source,
                source_path: "php://stdin".to_string(),
                real_path: None,
                script_name: "Standard input code".to_string(),
                script_args: args,
                cwd: current_dir()?,
                env: collect_env(),
                ini: ini.clone(),
                stdin: Vec::new(),
                php_binary: php_binary.clone(),
                debug,
                debug_log,
                debug_format,
            };
            execute_php(input, stdout, stderr)
        }
        CliAction::Serve {
            listen,
            docroot,
            router,
        } => run_builtin_server(listen, docroot, router),
    }
}

impl ParsedCli {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut no_ini = false;
        let mut defines = Vec::new();
        let mut config_path = None;
        let mut server_listen: Option<String> = None;
        let mut server_docroot: Option<PathBuf> = None;
        let mut server_router: Option<PathBuf> = None;
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            match arg.as_str() {
                "-h" | "--help" => {
                    return Ok(Self {
                        action: CliAction::Help,
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                "-v" | "--version" => {
                    return Ok(Self {
                        action: CliAction::Version,
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                "--ini" => {
                    return Ok(Self {
                        action: CliAction::ShowIni,
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                "-m" => {
                    reject_server_mix(&server_listen, "-m")?;
                    return Ok(Self {
                        action: CliAction::ListModules,
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                "-i" => {
                    reject_server_mix(&server_listen, "-i")?;
                    return Ok(Self {
                        action: CliAction::PhpInfo,
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                "--ri" | "--rf" | "--rc" => {
                    let flag = arg.clone();
                    if args
                        .get(index + 1)
                        .is_some_and(|next| !next.starts_with('-'))
                    {
                        index += 2;
                    } else {
                        index += 1;
                    }
                    let _ = index;
                    return Ok(Self {
                        action: CliAction::UnsupportedIntrospection { flag },
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                "-n" | "-q" => {
                    no_ini = true;
                    index += 1;
                }
                "-d" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "-d requires name=value".to_string())?;
                    defines.push(parse_define(value));
                    index += 1;
                }
                _ if arg.starts_with("-d") && arg.len() > 2 => {
                    defines.push(parse_define(&arg[2..]));
                    index += 1;
                }
                "-c" => {
                    let path = args
                        .get(index + 1)
                        .ok_or_else(|| "-c requires a path".to_string())?;
                    config_path = Some(PathBuf::from(path));
                    index += 2;
                }
                "-l" => {
                    reject_server_mix(&server_listen, "-l")?;
                    let path = args
                        .get(index + 1)
                        .ok_or_else(|| "-l requires a file path".to_string())?;
                    return Ok(Self {
                        action: CliAction::LintFile {
                            path: PathBuf::from(path),
                        },
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                "-S" => {
                    if server_listen.is_some() {
                        return Err("-S may only be specified once".to_string());
                    }
                    let listen = args
                        .get(index + 1)
                        .ok_or_else(|| "-S requires a listen address".to_string())?;
                    server_listen = Some(listen.clone());
                    index += 2;
                }
                "-t" => {
                    let docroot = args
                        .get(index + 1)
                        .ok_or_else(|| "-t requires a document root".to_string())?;
                    server_docroot = Some(PathBuf::from(docroot));
                    index += 2;
                }
                "--repeat" => {
                    if args.get(index + 1).is_none() {
                        return Err("--repeat requires a count".to_string());
                    }
                    index += 2;
                }
                "-r" => {
                    reject_server_mix(&server_listen, "-r")?;
                    index += 1;
                    let code = args
                        .get(index)
                        .ok_or_else(|| "-r requires code".to_string())?
                        .clone();
                    index += 1;
                    let rest = parse_script_args(&args[index..])?;
                    return Ok(Self {
                        action: CliAction::RunCode { code, args: rest },
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                "-f" => {
                    reject_server_mix(&server_listen, "-f")?;
                    index += 1;
                    let path = args
                        .get(index)
                        .ok_or_else(|| "-f requires a file path".to_string())?;
                    index += 1;
                    let rest = parse_script_args(&args[index..])?;
                    return Ok(Self {
                        action: CliAction::RunFile {
                            path: PathBuf::from(path),
                            args: rest,
                        },
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                "--" => {
                    reject_server_mix(&server_listen, "--")?;
                    return Ok(Self {
                        action: CliAction::RunStdin {
                            args: args[index + 1..].to_vec(),
                        },
                        no_ini,
                        defines,
                        config_path,
                    });
                }
                _ if arg.starts_with('-') => {
                    return Err(format!("unknown option `{arg}`"));
                }
                _ => {
                    if server_listen.is_some() {
                        if server_router.is_some() {
                            return Err(
                                "phrust-php -S accepts at most one router script".to_string()
                            );
                        }
                        server_router = Some(PathBuf::from(arg));
                        index += 1;
                        continue;
                    }
                    let path = PathBuf::from(arg);
                    let rest = parse_script_args(&args[index + 1..])?;
                    return Ok(Self {
                        action: CliAction::RunFile { path, args: rest },
                        no_ini,
                        defines,
                        config_path,
                    });
                }
            }
        }
        if let Some(listen) = server_listen {
            return Ok(Self {
                action: CliAction::Serve {
                    listen,
                    docroot: server_docroot,
                    router: server_router,
                },
                no_ini,
                defines,
                config_path,
            });
        }
        if server_docroot.is_some() {
            return Err("-t requires -S".to_string());
        }
        Ok(Self {
            action: CliAction::RunStdin { args: Vec::new() },
            no_ini,
            defines,
            config_path,
        })
    }
}

fn reject_server_mix(server_listen: &Option<String>, flag: &str) -> Result<(), String> {
    if server_listen.is_some() {
        Err(format!("{flag} cannot be combined with -S"))
    } else {
        Ok(())
    }
}

fn parse_script_args(args: &[String]) -> Result<Vec<String>, String> {
    if args.first().is_some_and(|arg| arg == "--") {
        Ok(args[1..].to_vec())
    } else {
        Ok(args.to_vec())
    }
}

fn parse_define(value: &str) -> (String, String) {
    value
        .split_once('=')
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .unwrap_or_else(|| (value.to_string(), "1".to_string()))
}

fn load_ini(parsed: &ParsedCli) -> Result<LoadedIni, String> {
    if parsed.no_ini {
        return Ok(LoadedIni {
            path: parsed.config_path.clone(),
            directives: Vec::new(),
            disabled: true,
        });
    }
    let Some(path) = &parsed.config_path else {
        return Ok(LoadedIni {
            path: None,
            directives: Vec::new(),
            disabled: false,
        });
    };
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("configuration file `{}`: {error}", path.display()))?;
    Ok(LoadedIni {
        path: Some(path.clone()),
        directives: parse_ini_directives(&contents),
        disabled: false,
    })
}

fn parse_ini_directives(contents: &str) -> Vec<(String, String)> {
    let mut directives = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with('#')
            || (line.starts_with('[') && line.ends_with(']'))
        {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if !matches!(
            name,
            "include_path"
                | "display_errors"
                | "display_startup_errors"
                | "error_reporting"
                | "filter.default"
        ) {
            continue;
        }
        directives.push((name.to_string(), unquote_ini_value(value.trim())));
    }
    directives
}

fn unquote_ini_value(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn merged_ini_defines(
    loaded_ini: &LoadedIni,
    defines: &[(String, String)],
) -> Vec<(String, String)> {
    let mut merged = loaded_ini.directives.clone();
    merged.extend_from_slice(defines);
    merged
}

fn print_ini<W: Write>(stdout: &mut W, loaded_ini: &LoadedIni) -> Result<(), String> {
    writeln!(
        stdout,
        "Configuration File (php.ini) Path: {}",
        ini_path_display(loaded_ini)
    )
    .map_err(|error| error.to_string())?;
    let loaded = if loaded_ini.disabled {
        "(none)".to_string()
    } else {
        ini_path_display(loaded_ini)
    };
    writeln!(stdout, "Loaded Configuration File: {}", loaded).map_err(|error| error.to_string())?;
    writeln!(stdout, "Scan for additional .ini files in: (none)").map_err(|error| error.to_string())
}

fn ini_path_display(loaded_ini: &LoadedIni) -> String {
    loaded_ini
        .path
        .as_deref()
        .map(Path::display)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "(none)".to_string())
}

fn print_modules<W: Write>(stdout: &mut W) -> Result<(), String> {
    writeln!(stdout, "[PHP Modules]").map_err(|error| error.to_string())?;
    for name in php_std::introspection::get_loaded_extensions(
        php_std::ExtensionRegistry::standard_library(),
    ) {
        writeln!(stdout, "{name}").map_err(|error| error.to_string())?;
    }
    writeln!(stdout, "\n[Zend Modules]").map_err(|error| error.to_string())
}

fn print_php_info<W: Write>(
    stdout: &mut W,
    loaded_ini: &LoadedIni,
    php_binary: &str,
) -> Result<(), String> {
    writeln!(stdout, "phpinfo()").map_err(|error| error.to_string())?;
    writeln!(
        stdout,
        "PHP Version => {}",
        php_source::reference_php_version()
    )
    .map_err(|error| error.to_string())?;
    writeln!(stdout, "System => phrust").map_err(|error| error.to_string())?;
    writeln!(stdout, "Server API => Command Line Interface").map_err(|error| error.to_string())?;
    writeln!(stdout, "PHP Binary => {php_binary}").map_err(|error| error.to_string())?;
    writeln!(
        stdout,
        "Loaded Configuration File => {}",
        if loaded_ini.disabled {
            "(none)".to_string()
        } else {
            ini_path_display(loaded_ini)
        }
    )
    .map_err(|error| error.to_string())?;
    writeln!(stdout, "\nPHP Modules").map_err(|error| error.to_string())?;
    for name in php_std::introspection::get_loaded_extensions(
        php_std::ExtensionRegistry::standard_library(),
    ) {
        writeln!(stdout, "{name}").map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn ini_options(defines: &[(String, String)]) -> CliIniOptions {
    let mut options = CliIniOptions {
        overrides: defines
            .iter()
            .map(|(name, value)| {
                if name == "error_reporting"
                    && let Some(mask) = parse_error_reporting_ini(value)
                {
                    return (name.clone(), mask.to_string());
                }
                (name.clone(), value.clone())
            })
            .collect(),
        ..CliIniOptions::default()
    };
    for (name, value) in defines {
        match name.as_str() {
            "include_path" => {
                options.include_path = Some(split_include_path(value).collect());
            }
            "display_errors" => {
                options.display_errors = Some(parse_bool_ini(value));
            }
            "display_startup_errors" => {
                options.display_startup_errors = Some(parse_bool_ini(value));
            }
            "error_reporting" => {
                if let Some(mask) = parse_error_reporting_ini(value) {
                    options.error_reporting = Some(mask);
                }
            }
            "filter.default" => {
                options.default_input_filter = RuntimeInputFilter::from_ini_value(value);
            }
            "filter.default_flags" => {
                options.default_input_filter_flags = value.trim().parse::<i64>().ok();
            }
            _ if name.starts_with("opcache.") => {}
            _ => {}
        }
    }
    options
}

fn emit_startup_ini_deprecations<W: Write>(
    stdout: &mut W,
    options: &CliIniOptions,
) -> Result<(), String> {
    if !options.display_startup_errors.unwrap_or(false) {
        return Ok(());
    }
    if !options.display_errors.unwrap_or(true) {
        return Ok(());
    }
    if let Some(mask) = options.error_reporting
        && !error_reporting_allows_level(mask, PHP_E_DEPRECATED)
    {
        return Ok(());
    }
    if options
        .overrides
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("filter.default"))
    {
        write!(
            stdout,
            "{}",
            format_php_diagnostic_line(
                PhpDiagnosticChannel::Deprecated,
                "The filter.default ini setting is deprecated",
                &PhpDiagnosticLocation::new("Unknown", 0),
            )
            .trim_start_matches('\n')
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn parse_error_reporting_ini(value: &str) -> Option<i64> {
    parse_error_reporting_or(value.trim())
}

fn parse_error_reporting_or(value: &str) -> Option<i64> {
    let mut parts = value.split('|');
    let mut mask = parse_error_reporting_and(parts.next()?.trim())?;
    for part in parts {
        mask |= parse_error_reporting_and(part.trim())?;
    }
    Some(mask)
}

fn parse_error_reporting_and(value: &str) -> Option<i64> {
    let mut parts = value.split('&');
    let mut mask = parse_error_reporting_factor(parts.next()?.trim())?;
    for part in parts {
        mask &= parse_error_reporting_factor(part.trim())?;
    }
    Some(mask)
}

fn parse_error_reporting_factor(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(rest) = value.strip_prefix('~') {
        return parse_error_reporting_factor(rest).map(|mask| !mask);
    }
    if value.starts_with('(') && value.ends_with(')') {
        return parse_error_reporting_ini(&value[1..value.len() - 1]);
    }
    if let Ok(mask) = value.parse::<i64>() {
        return Some(mask);
    }
    match value {
        "E_ALL" => Some(PHP_E_ALL),
        "E_ERROR" => Some(PHP_E_ERROR),
        "E_WARNING" => Some(PHP_E_WARNING),
        "E_PARSE" => Some(PHP_E_PARSE),
        "E_NOTICE" => Some(PHP_E_NOTICE),
        "E_CORE_ERROR" => Some(PHP_E_CORE_ERROR),
        "E_CORE_WARNING" => Some(PHP_E_CORE_WARNING),
        "E_COMPILE_ERROR" => Some(PHP_E_COMPILE_ERROR),
        "E_COMPILE_WARNING" => Some(PHP_E_COMPILE_WARNING),
        "E_USER_ERROR" => Some(PHP_E_USER_ERROR),
        "E_USER_WARNING" => Some(PHP_E_USER_WARNING),
        "E_USER_NOTICE" => Some(PHP_E_USER_NOTICE),
        "E_STRICT" => Some(PHP_E_STRICT),
        "E_RECOVERABLE_ERROR" => Some(PHP_E_RECOVERABLE_ERROR),
        "E_DEPRECATED" => Some(PHP_E_DEPRECATED),
        "E_USER_DEPRECATED" => Some(PHP_E_USER_DEPRECATED),
        _ => None,
    }
}

fn split_include_path(value: &str) -> impl Iterator<Item = PathBuf> + '_ {
    value
        .split(':')
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
}

fn parse_bool_ini(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "off" | "false" | "no"
    )
}

fn normalize_command_line_code(code: &str) -> String {
    if code.trim_start().starts_with("<?") {
        code.to_string()
    } else {
        format!("<?php {code}")
    }
}

fn read_stdin_if_piped<R>(stdin: &mut R, stdin_is_terminal: bool) -> Result<Vec<u8>, String>
where
    R: Read,
{
    if stdin_is_terminal {
        return Ok(Vec::new());
    }
    let mut bytes = Vec::new();
    stdin
        .read_to_end(&mut bytes)
        .map_err(|error| format!("stdin: {error}"))?;
    Ok(bytes)
}

fn debug_enabled_from_env() -> bool {
    env::var("PHRUST_DEBUG")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "on"))
}

fn error_format_from_env() -> DiagnosticOutputFormat {
    match env::var("PHRUST_ERROR_FORMAT").ok().as_deref() {
        Some("json" | "jsonl") => DiagnosticOutputFormat::Json,
        _ => DiagnosticOutputFormat::Text,
    }
}

fn current_dir() -> Result<PathBuf, String> {
    env::current_dir().map_err(|error| format!("current directory: {error}"))
}

fn collect_env() -> Vec<(String, String)> {
    env::vars_os()
        .filter_map(|(name, value)| Some((os_to_string(name)?, os_to_string(value)?)))
        .collect()
}

fn os_to_string(value: OsString) -> Option<String> {
    value.into_string().ok()
}

fn php_binary_path() -> String {
    env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| "phrust-php".to_string())
}

fn run_builtin_server(
    listen: String,
    docroot: Option<PathBuf>,
    router: Option<PathBuf>,
) -> Result<i32, String> {
    let docroot = docroot.unwrap_or(current_dir()?);
    let config = php_server::config::ServerConfig::builtin_cli_server(&listen, docroot, router)
        .map_err(|error| error.to_string())?;
    php_server::server::run_blocking(config).map_err(|error| error.to_string())?;
    Ok(EXIT_SUCCESS)
}

fn print_usage<W: Write>(stdout: &mut W) -> Result<(), String> {
    writeln!(
        stdout,
        "Usage: phrust-php [options] [-f] <file> [--] [args...]\n       phrust-php [options] -r <code> [--] [args...]\n       phrust-php -S <addr> [-t <docroot>] [router]\n\nOptions:\n  -v, --version        show PHP-compatible version\n  --ini                show loaded configuration file information\n  -c <path>            load minimal php.ini directives from path\n  -n                   ignore configuration files\n  -d name=value        set an INI directive; overrides -c\n  -l <file>            lint only; do not execute code\n  -m                   list loaded modules\n  -i                   show minimal phpinfo output\n  -S <addr>            start PHP-compatible built-in web server\n  -t <docroot>         document root for -S\n  -h, --help           show this help"
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct TestInput(Cursor<Vec<u8>>);

    impl Read for TestInput {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(buf)
        }
    }

    #[test]
    fn parser_accepts_runner_flags_and_repeated_defines() {
        let parsed = ParsedCli::parse(&[
            "-n".to_string(),
            "-d".to_string(),
            "display_errors=1".to_string(),
            "-dinclude_path=fixtures".to_string(),
            "--repeat".to_string(),
            "2".to_string(),
            "-f".to_string(),
            "test.php".to_string(),
            "--".to_string(),
            "a".to_string(),
            "b".to_string(),
        ])
        .expect("parse");

        assert!(parsed.no_ini);
        assert_eq!(
            parsed.defines,
            vec![
                ("display_errors".to_string(), "1".to_string()),
                ("include_path".to_string(), "fixtures".to_string())
            ]
        );
        assert_eq!(
            parsed.action,
            CliAction::RunFile {
                path: PathBuf::from("test.php"),
                args: vec!["a".to_string(), "b".to_string()]
            }
        );
    }

    #[test]
    fn parser_accepts_run_code_and_bare_file() {
        let run_code =
            ParsedCli::parse(&["-r".to_string(), "echo 1;".to_string()]).expect("parse -r");
        assert_eq!(
            run_code.action,
            CliAction::RunCode {
                code: "echo 1;".to_string(),
                args: Vec::new()
            }
        );

        let file =
            ParsedCli::parse(&["script.php".to_string(), "arg".to_string()]).expect("parse file");
        assert_eq!(
            file.action,
            CliAction::RunFile {
                path: PathBuf::from("script.php"),
                args: vec!["arg".to_string()]
            }
        );
    }

    #[test]
    fn parser_rejects_unknown_options() {
        let error = ParsedCli::parse(&["--not-php".to_string()]).expect_err("error");
        assert!(error.contains("unknown option"));
    }

    #[test]
    fn parser_rejects_missing_option_values() {
        let error = ParsedCli::parse(&["-c".to_string()]).expect_err("error");
        assert!(error.contains("-c requires"));
        let error = ParsedCli::parse(&["--repeat".to_string()]).expect_err("error");
        assert!(error.contains("--repeat requires"));
    }

    #[test]
    fn ini_options_parse_error_reporting_expressions() {
        let options = ini_options(&[(
            "error_reporting".to_string(),
            "E_ALL&~E_DEPRECATED".to_string(),
        )]);
        assert_eq!(options.error_reporting, Some(PHP_E_ALL & !PHP_E_DEPRECATED));
        assert_eq!(
            options.overrides,
            vec![(
                "error_reporting".to_string(),
                (PHP_E_ALL & !PHP_E_DEPRECATED).to_string()
            )]
        );

        let options = ini_options(&[(
            "error_reporting".to_string(),
            "E_WARNING | E_USER_WARNING".to_string(),
        )]);
        assert_eq!(
            options.error_reporting,
            Some(PHP_E_WARNING | PHP_E_USER_WARNING)
        );

        let options = ini_options(&[("error_reporting".to_string(), "24575".to_string())]);
        assert_eq!(options.error_reporting, Some(24575));
    }

    #[test]
    fn ini_options_parse_filter_default() {
        let options = ini_options(&[
            ("filter.default".to_string(), "special_chars".to_string()),
            ("filter.default_flags".to_string(), "4".to_string()),
        ]);

        assert_eq!(
            options.default_input_filter,
            Some(RuntimeInputFilter::SpecialChars)
        );
        assert_eq!(options.default_input_filter_flags, Some(4));
    }

    #[test]
    fn run_code_prints_php_version() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            ["-r".to_string(), "echo PHP_VERSION;".to_string()],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(
            String::from_utf8(stdout).expect("utf8"),
            php_source::reference_php_version()
        );
    }

    #[test]
    fn run_code_handles_recursive_autoload_callbacks() {
        let source = r#"
        spl_autoload_register(function ($name) {
            echo "IN:  autoload($name)\n";

            static $i = 0;
            if ($i++ > 10) {
                echo "-> Recursion detected - as expected.\n";
                return;
            }

            class_exists('UndefinedClass' . $i);

            echo "OUT: autoload($name)\n";
        });

        var_dump(class_exists('UndefinedClass0'));
        "#;
        let source = source.to_owned();

        let handle = std::thread::Builder::new()
            .name("recursive-autoload-cli-test".to_owned())
            .stack_size(128 * 1024 * 1024)
            .spawn(move || {
                let mut stdin = TestInput(Cursor::new(Vec::new()));
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                let status = run(
                    ["-r".to_owned(), source],
                    &mut stdin,
                    &mut stdout,
                    &mut stderr,
                );
                (status, stdout, stderr)
            })
            .expect("spawn recursive autoload test thread");
        let (status, stdout, stderr) = handle.join().expect("recursive autoload test finished");

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(
            String::from_utf8(stdout).expect("utf8"),
            concat!(
                "IN:  autoload(UndefinedClass0)\n",
                "IN:  autoload(UndefinedClass1)\n",
                "IN:  autoload(UndefinedClass2)\n",
                "IN:  autoload(UndefinedClass3)\n",
                "IN:  autoload(UndefinedClass4)\n",
                "IN:  autoload(UndefinedClass5)\n",
                "IN:  autoload(UndefinedClass6)\n",
                "IN:  autoload(UndefinedClass7)\n",
                "IN:  autoload(UndefinedClass8)\n",
                "IN:  autoload(UndefinedClass9)\n",
                "IN:  autoload(UndefinedClass10)\n",
                "IN:  autoload(UndefinedClass11)\n",
                "-> Recursion detected - as expected.\n",
                "OUT: autoload(UndefinedClass10)\n",
                "OUT: autoload(UndefinedClass9)\n",
                "OUT: autoload(UndefinedClass8)\n",
                "OUT: autoload(UndefinedClass7)\n",
                "OUT: autoload(UndefinedClass6)\n",
                "OUT: autoload(UndefinedClass5)\n",
                "OUT: autoload(UndefinedClass4)\n",
                "OUT: autoload(UndefinedClass3)\n",
                "OUT: autoload(UndefinedClass2)\n",
                "OUT: autoload(UndefinedClass1)\n",
                "OUT: autoload(UndefinedClass0)\n",
                "bool(false)\n"
            )
        );
    }

    #[test]
    fn run_file_seeds_argv_and_argc() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = temp_root("argv");
        fs::create_dir_all(&root).expect("mkdir");
        let script = root.join("fixture.php");
        fs::write(
            &script,
            "<?php echo $argc, '|', $argv[1], '|', $_SERVER['argv'][2];",
        )
        .expect("write script");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-n".to_string(),
                "-d".to_string(),
                "display_errors=1".to_string(),
                "-f".to_string(),
                script.to_string_lossy().into_owned(),
                "--".to_string(),
                "a".to_string(),
                "b".to_string(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).expect("utf8"), "3|a|b");
    }

    #[test]
    fn run_code_exposes_stdin_resource() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let mut stdin = TestInput(Cursor::new(b"payload".to_vec()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-r".to_string(),
                "echo stream_get_contents(STDIN);".to_string(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).expect("utf8"), "payload");
    }

    #[test]
    fn successful_warning_output_does_not_emit_internal_stderr_diagnostics() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-r".to_string(),
                "class Test {} $o = new Test; var_dump((int) $o);".to_string(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        let stdout = String::from_utf8(stdout).expect("utf8");
        assert!(
            stdout.contains("Warning: Object of class Test could not be converted to int"),
            "{stdout}"
        );
        assert!(stdout.contains("int(1)"), "{stdout}");
        assert_eq!(stderr, b"");
    }

    #[test]
    fn env_debug_writes_timeline_without_changing_stdout() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous_debug = env::var("PHRUST_DEBUG").ok();
        let previous_format = env::var("PHRUST_ERROR_FORMAT").ok();
        unsafe {
            env::set_var("PHRUST_DEBUG", "1");
            env::remove_var("PHRUST_ERROR_FORMAT");
        }
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            ["-r".to_string(), "echo 'ok';".to_string()],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        restore_env("PHRUST_DEBUG", previous_debug);
        restore_env("PHRUST_ERROR_FORMAT", previous_format);
        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).expect("utf8"), "ok");
        let stderr = String::from_utf8(stderr).expect("utf8");
        assert!(stderr.contains("D_PHRUST_FRONTEND_ANALYZE_START"));
        assert!(stderr.contains("D_PHRUST_VM_EXECUTE_END"));
        assert!(stderr.contains("D_PHRUST_VM_TRACE"));
    }

    #[test]
    fn include_path_define_affects_include_resolution() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = temp_root("include-path");
        let lib = root.join("lib");
        fs::create_dir_all(&lib).expect("mkdir");
        let script = root.join("fixture.php");
        fs::write(lib.join("dep.php"), "<?php echo 'dep';").expect("write dep");
        fs::write(&script, "<?php include 'dep.php';").expect("write script");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-d".to_string(),
                format!("include_path={}", lib.display()),
                script.to_string_lossy().into_owned(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).expect("utf8"), "dep");
    }

    #[test]
    fn run_file_emits_filter_default_startup_deprecation() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = temp_root("filter-default-startup-deprecation");
        fs::create_dir_all(&root).expect("mkdir");
        let script = root.join("fixture.php");
        fs::write(&script, "<?php echo \"Done\\n\";").expect("write script");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-n".to_string(),
                "-d".to_string(),
                "display_startup_errors=1".to_string(),
                "-d".to_string(),
                "filter.default=special_chars".to_string(),
                script.to_string_lossy().into_owned(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(
            String::from_utf8(stdout).expect("utf8"),
            concat!(
                "Deprecated: The filter.default ini setting is deprecated in Unknown on line 0\n",
                "Done\n"
            )
        );
        assert_eq!(stderr, b"");
    }

    #[test]
    fn run_file_suppresses_filter_default_startup_deprecation_by_default() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = temp_root("filter-default-startup-deprecation-off");
        fs::create_dir_all(&root).expect("mkdir");
        let script = root.join("fixture.php");
        fs::write(&script, "<?php echo \"Done\\n\";").expect("write script");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-n".to_string(),
                "-d".to_string(),
                "filter.default=special_chars".to_string(),
                script.to_string_lossy().into_owned(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).expect("utf8"), "Done\n");
        assert_eq!(stderr, b"");
    }

    #[test]
    fn run_file_honors_error_reporting_for_filter_default_startup_deprecation() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = temp_root("filter-default-startup-deprecation-masked");
        fs::create_dir_all(&root).expect("mkdir");
        let script = root.join("fixture.php");
        fs::write(&script, "<?php echo \"Done\\n\";").expect("write script");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-n".to_string(),
                "-d".to_string(),
                "display_startup_errors=1".to_string(),
                "-d".to_string(),
                "error_reporting=E_ALL&~E_DEPRECATED".to_string(),
                "-d".to_string(),
                "filter.default=special_chars".to_string(),
                script.to_string_lossy().into_owned(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).expect("utf8"), "Done\n");
        assert_eq!(stderr, b"");
    }

    #[test]
    fn run_file_honors_display_errors_for_filter_default_startup_deprecation() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = temp_root("filter-default-startup-deprecation-display-off");
        fs::create_dir_all(&root).expect("mkdir");
        let script = root.join("fixture.php");
        fs::write(&script, "<?php echo \"Done\\n\";").expect("write script");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-n".to_string(),
                "-d".to_string(),
                "display_startup_errors=1".to_string(),
                "-d".to_string(),
                "display_errors=0".to_string(),
                "-d".to_string(),
                "filter.default=special_chars".to_string(),
                script.to_string_lossy().into_owned(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).expect("utf8"), "Done\n");
        assert_eq!(stderr, b"");
    }

    #[test]
    fn config_file_loads_minimal_ini_and_d_overrides() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = temp_root("ini-config");
        let lib_from_ini = root.join("ini-lib");
        let lib_from_define = root.join("define-lib");
        fs::create_dir_all(&lib_from_ini).expect("mkdir ini lib");
        fs::create_dir_all(&lib_from_define).expect("mkdir define lib");
        fs::write(lib_from_ini.join("dep.php"), "<?php echo 'ini';").expect("write ini dep");
        fs::write(lib_from_define.join("dep.php"), "<?php echo 'define';")
            .expect("write define dep");
        let ini = root.join("php.ini");
        fs::write(
            &ini,
            format!(
                "; comment\n[PHP]\ninclude_path = \"{}\"\ndisplay_errors = 1\n",
                lib_from_ini.display()
            ),
        )
        .expect("write ini");
        let script = root.join("fixture.php");
        fs::write(&script, "<?php include 'dep.php';").expect("write script");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-c".to_string(),
                ini.to_string_lossy().into_owned(),
                "-d".to_string(),
                format!("include_path={}", lib_from_define.display()),
                script.to_string_lossy().into_owned(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).expect("utf8"), "define");
    }

    #[test]
    fn lint_file_does_not_execute_side_effects() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = temp_root("lint");
        fs::create_dir_all(&root).expect("mkdir");
        let marker = root.join("marker.txt");
        let script = root.join("fixture.php");
        fs::write(
            &script,
            format!(
                "<?php file_put_contents('{}', 'ran');",
                marker.to_string_lossy()
            ),
        )
        .expect("write script");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            ["-l".to_string(), script.to_string_lossy().into_owned()],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert!(!marker.exists());
        assert!(
            String::from_utf8(stdout)
                .expect("utf8")
                .contains("No syntax errors detected")
        );
    }

    #[test]
    fn exposes_cli_sapi_and_non_empty_php_binary() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let status = run(
            [
                "-r".to_string(),
                "echo PHP_SAPI, '|', php_sapi_name(), '|', PHP_BINARY === '' ? 'empty' : 'set';"
                    .to_string(),
            ],
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(status, 0, "{}", String::from_utf8_lossy(&stderr));
        assert_eq!(String::from_utf8(stdout).expect("utf8"), "cli|cli|set");
    }

    #[test]
    fn prints_modules_phpinfo_and_ini_report() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        assert_eq!(
            run(["-m".to_string()], &mut stdin, &mut stdout, &mut stderr),
            0
        );
        assert!(
            String::from_utf8(stdout)
                .expect("utf8")
                .contains("[PHP Modules]")
        );

        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(["-i".to_string()], &mut stdin, &mut stdout, &mut stderr),
            0
        );
        assert!(
            String::from_utf8(stdout)
                .expect("utf8")
                .contains("PHP Version =>")
        );

        let mut stdin = TestInput(Cursor::new(Vec::new()));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            run(["--ini".to_string()], &mut stdin, &mut stdout, &mut stderr),
            0
        );
        assert!(
            String::from_utf8(stdout)
                .expect("utf8")
                .contains("Loaded Configuration File")
        );
    }

    #[test]
    fn parser_accepts_server_flags_and_rejects_invalid_mixes() {
        let parsed = ParsedCli::parse(&[
            "-S".to_string(),
            "127.0.0.1:0".to_string(),
            "-t".to_string(),
            "public".to_string(),
            "router.php".to_string(),
        ])
        .expect("parse server");
        assert_eq!(
            parsed.action,
            CliAction::Serve {
                listen: "127.0.0.1:0".to_string(),
                docroot: Some(PathBuf::from("public")),
                router: Some(PathBuf::from("router.php")),
            }
        );

        let error =
            ParsedCli::parse(&["-t".to_string(), "public".to_string()]).expect_err("missing -S");
        assert!(error.contains("-t requires -S"));
        let error = ParsedCli::parse(&[
            "-S".to_string(),
            "127.0.0.1:0".to_string(),
            "-r".to_string(),
            "echo 1;".to_string(),
        ])
        .expect_err("invalid mix");
        assert!(error.contains("cannot be combined with -S"));
    }

    fn temp_root(name: &str) -> PathBuf {
        let index = NEXT_TEMP.fetch_add(1, Ordering::SeqCst);
        let path = env::temp_dir().join(format!(
            "phrust-php-cli-{}-{name}-{index}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    fn restore_env(name: &str, previous: Option<String>) {
        unsafe {
            if let Some(value) = previous {
                env::set_var(name, value);
            } else {
                env::remove_var(name);
            }
        }
    }
}
