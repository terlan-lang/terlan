use std::path::PathBuf;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use crate::CliState;

/// Default host for `terlc serve`.
pub(crate) const DEFAULT_SERVE_HOST: &str = "127.0.0.1";

/// Default port for `terlc serve`.
pub(crate) const DEFAULT_SERVE_PORT: u16 = 3000;

/// Default live-reload polling interval in milliseconds.
pub(crate) const DEFAULT_POLL_MS: u64 = 500;

/// Default maximum request-body size accepted by the production HTTP adapter.
pub(crate) const DEFAULT_MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;

/// Dynamic handler runtime selected for `terlc serve`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServeHandlerRuntime {
    Static,
}

/// Parsed `terlc serve` arguments.
///
/// Inputs:
/// - Produced from command-local CLI arguments and global CLI state.
///
/// Output:
/// - Normalized web package root, host, port, and validation-only mode.
///
/// Transformation:
/// - Keeps path and network settings explicit so command execution can validate
///   the package before binding a socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServeArgs {
    pub(crate) web_root: PathBuf,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) poll_ms: u64,
    pub(crate) max_body_bytes: u64,
    pub(crate) handler_runtime: ServeHandlerRuntime,
    pub(crate) check_only: bool,
    pub(crate) overrides: ServeCliOverrides,
}

/// Explicit command-line overrides retained for deterministic config merging.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ServeCliOverrides {
    pub(crate) host: bool,
    pub(crate) port: bool,
    pub(crate) poll_ms: bool,
    pub(crate) protocol: Option<String>,
    pub(crate) allow_public: bool,
    pub(crate) max_connections: Option<u64>,
    pub(crate) max_request_bytes: Option<u64>,
    pub(crate) max_body_bytes: Option<u64>,
    pub(crate) max_header_bytes: Option<u64>,
    pub(crate) request_timeout_ms: Option<u64>,
    pub(crate) idle_timeout_ms: Option<u64>,
    pub(crate) queue_capacity: Option<u64>,
    pub(crate) handler_pool_size: Option<u64>,
    pub(crate) shutdown_grace_ms: Option<u64>,
    pub(crate) telemetry: Option<String>,
    pub(crate) log_format: Option<String>,
}

/// Parses command-local `terlc serve` arguments.
///
/// Inputs:
/// - `args`: arguments after the `serve` verb.
/// - `state`: global CLI state used for the default `_build/web` directory.
///
/// Output:
/// - Parsed serve arguments or a user-facing error string.
///
/// Transformation:
/// - Accepts at most one package directory, parses `--host`, `--port`,
///   `--poll-ms`, `--handler-runtime`, and `--check`, and preserves unknown
///   option failures as stable CLI errors.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn parse_serve_args(args: &[String], state: &CliState) -> Result<ServeArgs, String> {
    parse_serve_args_with_default(args, state.out_dir.join("web"))
}

/// Parses the compiler-free runtime command with the release package default.
pub(crate) fn parse_serve_runtime_args(args: &[String]) -> Result<ServeArgs, String> {
    parse_serve_args_with_default(args, PathBuf::from("_build/web"))
}

fn parse_serve_args_with_default(
    args: &[String],
    default_web_root: PathBuf,
) -> Result<ServeArgs, String> {
    let mut web_root = None;
    let mut host = DEFAULT_SERVE_HOST.to_string();
    let mut port = DEFAULT_SERVE_PORT;
    let mut poll_ms = DEFAULT_POLL_MS;
    let mut handler_runtime = ServeHandlerRuntime::Static;
    let mut check_only = false;
    let mut overrides = ServeCliOverrides::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc serve --host requires a value".to_string());
                };
                host = value.clone();
                overrides.host = true;
            }
            "--port" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc serve --port requires a value".to_string());
                };
                port = value.parse::<u16>().map_err(|_| {
                    format!("terlc serve --port expects a u16 value, got `{value}`")
                })?;
                overrides.port = true;
            }
            "--poll-ms" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc serve --poll-ms requires a value".to_string());
                };
                poll_ms = value.parse::<u64>().map_err(|_| {
                    format!("terlc serve --poll-ms expects a u64 value, got `{value}`")
                })?;
                if poll_ms == 0 {
                    return Err("terlc serve --poll-ms must be greater than 0".to_string());
                }
                overrides.poll_ms = true;
            }
            "--protocol" => {
                overrides.protocol = Some(parse_text_value(args, &mut index, "--protocol")?)
            }
            "--allow-public" => overrides.allow_public = true,
            "--max-connections" => {
                overrides.max_connections =
                    Some(parse_u64_value(args, &mut index, "--max-connections")?)
            }
            "--max-request-bytes" => {
                overrides.max_request_bytes =
                    Some(parse_u64_value(args, &mut index, "--max-request-bytes")?)
            }
            "--max-body-bytes" => {
                overrides.max_body_bytes =
                    Some(parse_u64_value(args, &mut index, "--max-body-bytes")?)
            }
            "--max-header-bytes" => {
                overrides.max_header_bytes =
                    Some(parse_u64_value(args, &mut index, "--max-header-bytes")?)
            }
            "--request-timeout-ms" => {
                overrides.request_timeout_ms =
                    Some(parse_u64_value(args, &mut index, "--request-timeout-ms")?)
            }
            "--idle-timeout-ms" => {
                overrides.idle_timeout_ms =
                    Some(parse_u64_value(args, &mut index, "--idle-timeout-ms")?)
            }
            "--queue-capacity" => {
                overrides.queue_capacity =
                    Some(parse_u64_value(args, &mut index, "--queue-capacity")?)
            }
            "--handler-pool-size" => {
                overrides.handler_pool_size =
                    Some(parse_u64_value(args, &mut index, "--handler-pool-size")?)
            }
            "--shutdown-grace-ms" => {
                overrides.shutdown_grace_ms =
                    Some(parse_u64_value(args, &mut index, "--shutdown-grace-ms")?)
            }
            "--telemetry" => {
                overrides.telemetry = Some(parse_text_value(args, &mut index, "--telemetry")?)
            }
            "--log-format" => {
                overrides.log_format = Some(parse_text_value(args, &mut index, "--log-format")?)
            }
            "--handler-runtime" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc serve --handler-runtime requires a value".to_string());
                };
                handler_runtime = match value.as_str() {
                    "static" => ServeHandlerRuntime::Static,
                    "beam" => {
                        return Err(
                            "handler runtime `beam` was removed from the public CLI; use `static`"
                                .to_string(),
                        );
                    }
                    _ => {
                        return Err(format!(
                            "terlc serve --handler-runtime expects static, got `{value}`"
                        ));
                    }
                };
            }
            "--check" | "--check-config" => {
                check_only = true;
            }
            option if option.starts_with('-') => {
                return Err(format!("unsupported serve option: {option}"));
            }
            path => {
                if web_root.is_some() {
                    return Err("terlc serve expects at most one web package directory".to_string());
                }
                web_root = Some(PathBuf::from(path));
            }
        }
        index += 1;
    }

    Ok(ServeArgs {
        web_root: web_root.unwrap_or(default_web_root),
        host,
        port,
        poll_ms,
        max_body_bytes: DEFAULT_MAX_BODY_BYTES,
        handler_runtime,
        check_only,
        overrides,
    })
}

fn parse_text_value(args: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("terlc serve {option} requires a value"))
}

fn parse_u64_value(args: &[String], index: &mut usize, option: &str) -> Result<u64, String> {
    let value = parse_text_value(args, index, option)?;
    value
        .parse::<u64>()
        .map_err(|_| format!("terlc serve {option} expects a u64 value, got `{value}`"))
}
