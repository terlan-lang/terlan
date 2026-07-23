//! Dependency-free EPMD command and runtime configuration types.

use std::string::{String, ToString};
use std::vec;
use std::vec::Vec;
use core::net::{AddrParseError, IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use core::num::NonZeroU64;
use core::time::Duration;

/// Default EPMD port used by OTP.
pub const DEFAULT_PORT: u16 = 4369;

/// Default inactivity timeout in seconds.
pub const DEFAULT_PACKET_TIMEOUT: u64 = 60;

/// Environment-derived defaults that affect EPMD command planning.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EpmdEnvironment {
    /// Optional comma-separated address list from `ERL_EPMD_ADDRESS`.
    pub addresses: Option<String>,
    /// Optional port string from `ERL_EPMD_PORT`.
    pub port: Option<String>,
    /// Whether `ERL_EPMD_RELAXED_COMMAND_CHECK` is present.
    pub relaxed_command_check: bool,
}

impl EpmdEnvironment {
    /// Return the parsed port value or the OTP default.
    pub fn parsed_port(&self) -> u16 {
        self.port
            .as_deref()
            .and_then(|port| port.parse::<u16>().ok())
            .filter(|port| *port != 0)
            .unwrap_or(DEFAULT_PORT)
    }
}

/// Parsed EPMD command line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpmdParsedCommand {
    /// Runtime configuration shared by server and client commands.
    pub config: EpmdRuntimeConfig,
    /// Action selected by the command line.
    pub command: EpmdCommand,
}

/// Runtime configuration selected by flags and environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpmdRuntimeConfig {
    /// Optional comma-separated listen address list.
    pub addresses: Option<String>,
    /// EPMD port.
    pub port: u16,
    /// Number of debug flags supplied.
    pub debug_level: u8,
    /// Whether the server should detach.
    pub daemon: bool,
    /// Whether relaxed command checks are enabled.
    pub relaxed_command_check: bool,
    /// Connection inactivity timeout in seconds.
    pub packet_timeout: NonZeroU64,
    /// Optional debug-only accept delay in seconds.
    pub delay_accept: Option<NonZeroU64>,
    /// Optional debug-only write delay in seconds.
    pub delay_write: Option<NonZeroU64>,
    /// Whether systemd socket activation mode is requested.
    pub systemd: bool,
}

/// Parsed runtime configuration inputs before environment defaults are merged.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EpmdRuntimeConfigParts {
    /// Optional command-line listen address list.
    pub addresses: Option<String>,
    /// Optional command-line port.
    pub port: Option<u16>,
    /// Number of debug flags supplied.
    pub debug_level: u8,
    /// Whether the server should detach.
    pub daemon: bool,
    /// Whether relaxed command checks are enabled by explicit flag.
    pub relaxed_command_check: bool,
    /// Optional command-line connection inactivity timeout in seconds.
    pub packet_timeout: Option<NonZeroU64>,
    /// Optional debug-only accept delay in seconds.
    pub delay_accept: Option<NonZeroU64>,
    /// Optional debug-only write delay in seconds.
    pub delay_write: Option<NonZeroU64>,
    /// Whether systemd socket activation mode is requested.
    pub systemd: bool,
}

/// Merge parsed command-line parts with environment defaults.
pub fn build_runtime_config(
    parts: EpmdRuntimeConfigParts,
    environment: EpmdEnvironment,
) -> EpmdRuntimeConfig {
    let port = parts.port.unwrap_or_else(|| environment.parsed_port());
    EpmdRuntimeConfig {
        addresses: parts.addresses.or(environment.addresses),
        port,
        debug_level: parts.debug_level,
        daemon: parts.daemon,
        relaxed_command_check: parts.relaxed_command_check || environment.relaxed_command_check,
        packet_timeout: parts.packet_timeout.unwrap_or_else(default_packet_timeout),
        delay_accept: parts.delay_accept,
        delay_write: parts.delay_write,
        systemd: parts.systemd,
    }
}

/// Top-level EPMD action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EpmdCommand {
    /// Run an EPMD server.
    Server,
    /// Query names from a running EPMD instance.
    Names {
        /// Suppress the leading status line for `-started`.
        silent: bool,
    },
    /// Dump active and old node registrations.
    Dump,
    /// Kill a running EPMD instance.
    Kill,
    /// Stop one node registration by name.
    Stop {
        /// Node name to stop.
        name: String,
    },
}

/// Raw interactive EPMD command flags after argument parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EpmdCommandFlags<'a> {
    /// Whether the names query flag was supplied.
    pub names: bool,
    /// Whether the started query flag was supplied.
    pub started: bool,
    /// Whether the dump query flag was supplied.
    pub dump: bool,
    /// Whether the kill command flag was supplied.
    pub kill: bool,
    /// Optional node name supplied to the stop command.
    pub stop_name: Option<&'a str>,
}

/// Failures while selecting one EPMD command from parsed flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EpmdCommandSelectionError {
    /// More than one interactive command flag was supplied.
    ConflictingInteractiveCommands,
}

/// Convert parsed command flags into the canonical EPMD command.
pub fn select_epmd_command(
    flags: EpmdCommandFlags<'_>,
) -> Result<EpmdCommand, EpmdCommandSelectionError> {
    let command_count = usize::from(flags.names)
        + usize::from(flags.started)
        + usize::from(flags.dump)
        + usize::from(flags.kill)
        + usize::from(flags.stop_name.is_some());
    if command_count > 1 {
        return Err(EpmdCommandSelectionError::ConflictingInteractiveCommands);
    }
    if flags.names {
        Ok(EpmdCommand::Names { silent: false })
    } else if flags.started {
        Ok(EpmdCommand::Names { silent: true })
    } else if flags.dump {
        Ok(EpmdCommand::Dump)
    } else if flags.kill {
        Ok(EpmdCommand::Kill)
    } else if let Some(name) = flags.stop_name {
        Ok(EpmdCommand::Stop {
            name: name.to_string(),
        })
    } else {
        Ok(EpmdCommand::Server)
    }
}

/// Return the default packet timeout as a non-zero value.
pub fn default_packet_timeout() -> NonZeroU64 {
    NonZeroU64::new(DEFAULT_PACKET_TIMEOUT).expect("default packet timeout is non-zero")
}

/// Failures from dependency-free EPMD listen-address planning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EpmdListenAddressError {
    /// No valid listen addresses were selected.
    NoListenAddress,
    /// One configured listen address could not be parsed.
    AddressParse {
        /// Address text that failed to parse.
        address: String,
        /// Parser error returned by the standard library.
        source: AddrParseError,
    },
}

/// Parse configured listen addresses into socket addresses.
pub fn parse_listen_addresses(
    config: &EpmdRuntimeConfig,
) -> Result<Vec<SocketAddr>, EpmdListenAddressError> {
    let addresses = config
        .addresses
        .as_deref()
        .map(parse_configured_addresses)
        .unwrap_or_else(|| {
            Ok(vec![
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            ])
        })?;
    if addresses.is_empty() {
        return Err(EpmdListenAddressError::NoListenAddress);
    }
    Ok(addresses
        .into_iter()
        .map(|address| SocketAddr::new(address, config.port))
        .collect())
}

/// Parse the comma-separated `ERL_EPMD_ADDRESS` or `-address` list.
pub fn parse_configured_addresses(addresses: &str) -> Result<Vec<IpAddr>, EpmdListenAddressError> {
    addresses
        .split(',')
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .map(|address| {
            address
                .parse::<IpAddr>()
                .map_err(|source| EpmdListenAddressError::AddressParse {
                    address: address.to_string(),
                    source,
                })
        })
        .collect()
}

/// Runtime behavior for one accepted EPMD connection.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct EpmdConnectionOptions {
    /// Maximum time allowed for a client to send the initial request frame.
    pub packet_timeout: Duration,
    /// Optional test delay before writing a reply to the client.
    pub delay_write: Option<Duration>,
}

impl EpmdConnectionOptions {
    /// Return connection options from explicit timeout and delay values.
    pub fn new(packet_timeout: Duration, delay_write: Option<Duration>) -> Self {
        Self {
            packet_timeout,
            delay_write,
        }
    }
}

impl Default for EpmdConnectionOptions {
    /// Return connection options matching the normal OTP epmd defaults.
    fn default() -> Self {
        Self {
            packet_timeout: Duration::from_secs(DEFAULT_PACKET_TIMEOUT),
            delay_write: None,
        }
    }
}

/// Convert parsed CLI runtime settings into per-connection behavior.
pub fn connection_options_from_config(config: &EpmdRuntimeConfig) -> EpmdConnectionOptions {
    EpmdConnectionOptions::new(
        Duration::from_secs(config.packet_timeout.get()),
        config
            .delay_write
            .map(|delay| Duration::from_secs(delay.get())),
    )
}

/// Return true when an address is a local loopback peer.
pub fn is_local_peer(addr: SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(ip) => ip == Ipv4Addr::LOCALHOST,
        IpAddr::V6(ip) => ip == Ipv6Addr::LOCALHOST,
    }
}

/// Return local EPMD client connection targets in OTP-compatible order.
pub fn client_loopback_addresses(port: u16) -> [SocketAddr; 2] {
    [
        SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
    ]
}

/// Return the clap-compatible spelling for an OTP-style single-dash option.
pub fn normalize_legacy_option_text(option: &str) -> Option<&'static str> {
    match option {
        "-debug" => Some("--debug"),
        "-packet_timeout" => Some("--packet_timeout"),
        "-delay_accept" => Some("--delay_accept"),
        "-delay_write" => Some("--delay_write"),
        "-daemon" => Some("--daemon"),
        "-relaxed_command_check" => Some("--relaxed_command_check"),
        "-kill" => Some("--kill"),
        "-address" => Some("--address"),
        "-port" => Some("--port"),
        "-names" => Some("--names"),
        "-started" => Some("--started"),
        "-dump" => Some("--dump"),
        "-stop" => Some("--stop"),
        "-systemd" => Some("--systemd"),
        _ => None,
    }
}
