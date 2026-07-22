use std::path::PathBuf;

use crate::CliState;

/// Default host for `terlc serve`.
pub(crate) const DEFAULT_SERVE_HOST: &str = "127.0.0.1";

/// Default port for `terlc serve`.
pub(crate) const DEFAULT_SERVE_PORT: u16 = 3000;

/// Default live-reload polling interval in milliseconds.
pub(crate) const DEFAULT_POLL_MS: u64 = 500;

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
    pub(crate) handler_runtime: ServeHandlerRuntime,
    pub(crate) check_only: bool,
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
pub(crate) fn parse_serve_args(args: &[String], state: &CliState) -> Result<ServeArgs, String> {
    let mut web_root = None;
    let mut host = DEFAULT_SERVE_HOST.to_string();
    let mut port = DEFAULT_SERVE_PORT;
    let mut poll_ms = DEFAULT_POLL_MS;
    let mut handler_runtime = ServeHandlerRuntime::Static;
    let mut check_only = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc serve --host requires a value".to_string());
                };
                host = value.clone();
            }
            "--port" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("terlc serve --port requires a value".to_string());
                };
                port = value.parse::<u16>().map_err(|_| {
                    format!("terlc serve --port expects a u16 value, got `{value}`")
                })?;
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
        web_root: web_root.unwrap_or_else(|| state.out_dir.join("web")),
        host,
        port,
        poll_ms,
        handler_runtime,
        check_only,
    })
}
