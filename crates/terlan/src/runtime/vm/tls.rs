use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Cursor, ErrorKind, Read, Write};
use std::path::Path;
use std::sync::Arc;

use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection};

use super::tcp::{VmTcpListener, VmTcpRuntime, VmTcpStream};

#[cfg(test)]
#[path = "tls_test.rs"]
#[cfg(test)]
mod tls_test;

/// VM-owned TLS mode.
///
/// Inputs: selected server TLS policy. Output: stable runtime mode.
/// Transformation: keeps TLS policy explicit before a concrete rustls transport
/// is attached to VM TCP streams.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTlsMode {
    Plain,
    Manual,
    Internal,
    Auto,
}

/// VM-owned ACME provider identity.
///
/// Inputs: configured public certificate provider. Output: normalized provider
/// value for runtime inspection. Transformation: avoids leaking provider
/// spelling into scheduler and transport code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTlsProvider {
    LetsEncrypt,
    ZeroSsl,
}

/// VM-owned listener transport mode derived from TLS policy.
///
/// Inputs: validated TLS plan for a VM TCP listener. Output: transport mode
/// used by future protocol handoff. Transformation: keeps production HTTP from
/// matching directly on certificate policy details when it only needs to know
/// whether a TLS engine is required.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTlsTransportMode {
    Plaintext,
    Tls,
}

/// VM-owned TLS runtime plan.
///
/// Inputs: project TLS configuration translated by the compiler/CLI boundary.
/// Output: validated TLS metadata for later rustls transport setup.
/// Transformation: separates VM stream scheduling from certificate policy and
/// ACME cache policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmTlsPlan {
    pub(crate) mode: VmTlsMode,
    pub(crate) domains: Vec<String>,
    pub(crate) email: Option<String>,
    pub(crate) primary_provider: Option<VmTlsProvider>,
    pub(crate) fallback_provider: Option<VmTlsProvider>,
    pub(crate) cert_path: Option<String>,
    pub(crate) key_path: Option<String>,
    pub(crate) passphrase_env: Option<String>,
    pub(crate) ca_path: Option<String>,
    pub(crate) server_name: Option<String>,
    pub(crate) trust_local: Option<bool>,
}

/// VM-owned concrete TLS server configuration.
///
/// Inputs: a validated listener-bound TLS plan. Output: maintained `rustls`
/// server configuration plus the originating TLS mode. Transformation: gives
/// future VM stream scheduling a concrete TLS engine boundary without
/// hand-rolling TLS records or depending on host async runtime state.
#[derive(Clone, Debug)]
pub(crate) struct VmTlsServerConfig {
    pub(crate) mode: VmTlsMode,
    pub(crate) server_config: Arc<ServerConfig>,
}

/// VM-owned TLS rotation overlap window.
///
/// Inputs: a listener TLS plan replacement. Output: inspectable old/new mode
/// overlap and retirement deadline. Transformation: lets accepted
/// connections keep their previous TLS config while new accepts observe the
/// replacement, without relying on host runtime state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmTlsRotationWindow {
    pub(crate) listener: VmTcpListener,
    pub(crate) previous_mode: VmTlsMode,
    pub(crate) replacement_mode: VmTlsMode,
    pub(crate) started_at_tick: u64,
    pub(crate) retire_after_tick: u64,
}

/// VM-owned TLS server connection state.
///
/// Inputs: a concrete rustls server configuration. Output: a stateful rustls
/// server connection plus source TLS mode. Transformation: gives the VM a
/// concrete TLS stream state object whose read/write readiness can be driven by
/// VM scheduling without exposing rustls internals to HTTP handlers.
#[derive(Debug)]
pub(crate) struct VmTlsServerConnection {
    mode: VmTlsMode,
    connection: ServerConnection,
}

/// Runtime-visible TLS server connection readiness.
///
/// Inputs: one VM TLS server connection. Output: handshake and IO readiness
/// flags. Transformation: converts rustls connection state into the vocabulary
/// the VM scheduler will use to park or wake stream actors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmTlsServerConnectionInfo {
    pub(crate) mode: VmTlsMode,
    pub(crate) handshaking: bool,
    pub(crate) wants_read: bool,
    pub(crate) wants_write: bool,
}

/// VM TLS poll result for one TCP-backed TLS server stream.
///
/// Inputs: encrypted bytes currently available through VM TCP and rustls
/// connection state. Output: scheduler-facing readiness state. Transformation:
/// tells protocol drivers whether they need more encrypted bytes, are still in
/// handshake, or may read/write plaintext.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTlsTcpPoll {
    NeedRead,
    Handshaking,
    Ready,
}

/// VM-owned TLS transport over one VM TCP stream.
///
/// Inputs: an accepted VM TCP stream and a rustls server connection. Output:
/// encrypted byte movement through VM TCP plus plaintext access for protocol
/// layers. Transformation: keeps TLS mechanics in rustls while the VM owns
/// stream scheduling, byte movement, and resource handles.
pub(crate) struct VmTlsTcpServerStream {
    stream: VmTcpStream,
    connection: VmTlsServerConnection,
    read_limit: usize,
}

impl VmTlsServerConnection {
    /// Inspects handshake and IO readiness for scheduler integration.
    pub(crate) fn inspect(&self) -> VmTlsServerConnectionInfo {
        VmTlsServerConnectionInfo {
            mode: self.mode,
            handshaking: self.connection.is_handshaking(),
            wants_read: self.connection.wants_read(),
            wants_write: self.connection.wants_write(),
        }
    }

    /// Reads TLS wire bytes into the rustls server connection.
    ///
    /// Inputs:
    /// - `bytes`: encrypted TLS records received from VM TCP.
    ///
    /// Output:
    /// - Number of bytes consumed by rustls.
    ///
    /// Transformation:
    /// - Keeps TLS record parsing inside rustls while giving the VM scheduler a
    ///   byte-oriented handoff point for TCP readiness.
    pub(crate) fn read_tls_bytes(&mut self, bytes: &[u8]) -> Result<usize, String> {
        Ok(self
            .connection
            .read_tls(&mut Cursor::new(bytes))
            .expect("VM TLS in-memory cursor reads cannot fail"))
    }

    /// Processes pending TLS packets inside rustls.
    ///
    /// Inputs:
    /// - Bytes previously provided by `read_tls_bytes`.
    ///
    /// Output:
    /// - Success when rustls accepts the pending records, or a stable
    ///   diagnostic for TLS protocol failures.
    ///
    /// Transformation:
    /// - Advances the rustls state machine without embedding TLS semantics in
    ///   the VM.
    pub(crate) fn process_new_packets(&mut self) -> Result<(), String> {
        self.connection
            .process_new_packets()
            .map(|_| ())
            .map_err(|err| format!("VM TLS failed to process TLS packets: {err}"))
    }

    /// Writes pending TLS wire bytes from rustls.
    ///
    /// Inputs:
    /// - Current rustls connection state.
    ///
    /// Output:
    /// - Encrypted TLS bytes ready to send through VM TCP.
    ///
    /// Transformation:
    /// - Keeps TLS record serialization inside rustls while exposing a stable
    ///   VM byte buffer boundary.
    pub(crate) fn write_tls_bytes(&mut self) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        self.connection
            .write_tls(&mut bytes)
            .expect("VM TLS in-memory byte buffer writes cannot fail");
        Ok(bytes)
    }

    /// Writes plaintext application bytes into the TLS connection.
    ///
    /// Inputs:
    /// - Plain application payload produced by a Terlan handler.
    ///
    /// Output:
    /// - Number of plaintext bytes accepted by rustls.
    ///
    /// Transformation:
    /// - Lets HTTP or another protocol write plaintext while rustls owns
    ///   encryption and framing.
    pub(crate) fn write_plaintext(&mut self, bytes: &[u8]) -> Result<usize, String> {
        let mut writer = self.connection.writer();
        write_plaintext_to(&mut writer, bytes)
    }

    /// Reads decrypted plaintext application bytes from the TLS connection.
    ///
    /// Inputs:
    /// - Current rustls connection state after packet processing.
    ///
    /// Output:
    /// - Decrypted plaintext currently available to the VM protocol layer.
    ///
    /// Transformation:
    /// - Lets HTTP or another protocol consume plaintext without seeing TLS
    ///   records or cryptographic state.
    pub(crate) fn read_plaintext(&mut self) -> Result<Vec<u8>, String> {
        let mut reader = self.connection.reader();
        read_plaintext_from(&mut reader)
    }
}

fn read_plaintext_from(reader: &mut dyn Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => bytes.extend_from_slice(&buffer[..read]),
            Err(err) if err.kind() == ErrorKind::WouldBlock => break,
            Err(err) => return Err(format!("VM TLS failed to read plaintext: {err}")),
        }
    }
    Ok(bytes)
}

fn write_plaintext_to(writer: &mut dyn Write, bytes: &[u8]) -> Result<usize, String> {
    match writer.write(bytes) {
        Ok(written) => Ok(written),
        Err(err) => Err(format!("VM TLS failed to write plaintext: {err}")),
    }
}

impl VmTlsTcpServerStream {
    /// Creates a TLS transport wrapper for one accepted VM TCP stream.
    pub(crate) fn new(stream: VmTcpStream, connection: VmTlsServerConnection) -> Self {
        Self {
            stream,
            connection,
            read_limit: 16 * 1024,
        }
    }

    /// Returns the underlying VM TCP stream handle.
    pub(crate) fn stream(&self) -> VmTcpStream {
        self.stream
    }

    /// Inspects the underlying TLS server connection.
    pub(crate) fn inspect(&self) -> VmTlsServerConnectionInfo {
        self.connection.inspect()
    }

    /// Polls encrypted VM TCP bytes through rustls.
    ///
    /// Inputs:
    /// - `tcp`: VM TCP runtime that owns encrypted stream bytes.
    ///
    /// Output:
    /// - TLS readiness state after processing available encrypted bytes and
    ///   flushing any pending TLS records back through VM TCP.
    ///
    /// Transformation:
    /// - Receives encrypted bytes from VM TCP, lets rustls process them, emits
    ///   encrypted response records back to the peer, and reports whether
    ///   protocol layers may consume plaintext.
    pub(crate) fn poll(&mut self, tcp: &mut VmTcpRuntime) -> Result<VmTlsTcpPoll, String> {
        let received = tcp.receive(self.stream, self.read_limit)?;
        if let Some(bytes) = received {
            self.connection.read_tls_bytes(&bytes)?;
            self.connection.process_new_packets()?;
            self.flush_tls_to_tcp(tcp)?;
            return Ok(self.poll_state());
        }

        if self.connection.inspect().wants_write {
            self.flush_tls_to_tcp(tcp)?;
        }

        Ok(self.poll_state())
    }

    /// Reads currently available decrypted plaintext bytes.
    pub(crate) fn read_plaintext(&mut self) -> Result<Vec<u8>, String> {
        self.connection.read_plaintext()
    }

    /// Writes plaintext and flushes encrypted TLS records through VM TCP.
    pub(crate) fn write_plaintext(
        &mut self,
        tcp: &mut VmTcpRuntime,
        bytes: &[u8],
    ) -> Result<usize, String> {
        let written = self.connection.write_plaintext(bytes)?;
        self.flush_tls_to_tcp(tcp)?;
        Ok(written)
    }

    fn flush_tls_to_tcp(&mut self, tcp: &mut VmTcpRuntime) -> Result<usize, String> {
        let bytes = self.connection.write_tls_bytes()?;
        if bytes.is_empty() {
            return Ok(0);
        }
        tcp.send(self.stream, bytes)
    }

    fn poll_state(&self) -> VmTlsTcpPoll {
        let info = self.connection.inspect();
        if !info.handshaking {
            return VmTlsTcpPoll::Ready;
        }
        if info.wants_read {
            VmTlsTcpPoll::NeedRead
        } else {
            VmTlsTcpPoll::Handshaking
        }
    }
}

/// VM-owned TLS plan registry.
///
/// Inputs: named TLS plans. Output: validated plans available to VM-owned
/// listeners. Transformation: gives the VM a deterministic TLS configuration
/// boundary without binding sockets or performing certificate I/O.
#[derive(Debug, Default)]
pub(crate) struct VmTlsRuntime {
    plans: HashMap<String, VmTlsPlan>,
    listener_plans: HashMap<VmTcpListener, VmTlsPlan>,
    rotation_windows: HashMap<VmTcpListener, VmTlsRotationWindow>,
}

impl VmTlsRuntime {
    /// Creates an empty TLS plan registry.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Installs a validated TLS plan for one listener name.
    ///
    /// Inputs:
    /// - `listener`: logical VM listener name.
    /// - `plan`: TLS policy translated at the CLI/compiler boundary.
    ///
    /// Output:
    /// - Success when the plan is internally consistent, or a stable
    ///   diagnostic.
    ///
    /// Transformation:
    /// - Validates mode-specific TLS requirements without reading
    ///   certificates, opening sockets, or depending on a host async runtime.
    pub(crate) fn install_plan(
        &mut self,
        listener: impl Into<String>,
        plan: VmTlsPlan,
    ) -> Result<(), String> {
        let listener = listener.into();
        if listener.trim().is_empty() {
            return Err("VM TLS listener name cannot be empty".to_string());
        }
        validate_tls_plan(&plan)?;
        self.plans.insert(listener, plan);
        Ok(())
    }

    /// Returns the TLS plan for one listener.
    pub(crate) fn inspect_plan(&self, listener: &str) -> Option<&VmTlsPlan> {
        self.plans.get(listener)
    }

    /// Installs a validated TLS plan for one VM TCP listener handle.
    ///
    /// Inputs:
    /// - `listener`: VM-owned TCP listener resource.
    /// - `plan`: TLS policy translated at the CLI/compiler boundary.
    ///
    /// Output:
    /// - Success when the plan is internally consistent, or a stable
    ///   diagnostic.
    ///
    /// Transformation:
    /// - Binds TLS metadata to the same listener resource that production HTTP
    ///   polls, avoiding a separate string-keyed runtime path for VM streams.
    pub(crate) fn install_listener_plan(
        &mut self,
        listener: VmTcpListener,
        plan: VmTlsPlan,
    ) -> Result<(), String> {
        validate_tls_plan(&plan)?;
        self.listener_plans.insert(listener, plan);
        Ok(())
    }

    /// Returns the TLS plan for one VM TCP listener handle.
    pub(crate) fn inspect_listener_plan(&self, listener: VmTcpListener) -> Option<&VmTlsPlan> {
        self.listener_plans.get(&listener)
    }

    /// Rotates the TLS plan for one listener while preserving an overlap window.
    pub(crate) fn rotate_listener_plan(
        &mut self,
        listener: VmTcpListener,
        replacement: VmTlsPlan,
        now_tick: u64,
        overlap_ticks: u64,
    ) -> Result<VmTlsRotationWindow, String> {
        if overlap_ticks == 0 {
            return Err("VM TLS rotation overlap must be positive".to_string());
        }
        validate_tls_plan(&replacement)?;
        let previous = self
            .listener_plans
            .get(&listener)
            .ok_or_else(|| "VM TLS listener handle has no installed transport plan".to_string())?;
        let window = VmTlsRotationWindow {
            listener,
            previous_mode: previous.mode,
            replacement_mode: replacement.mode,
            started_at_tick: now_tick,
            retire_after_tick: now_tick.saturating_add(overlap_ticks),
        };
        self.listener_plans.insert(listener, replacement);
        self.rotation_windows.insert(listener, window);
        Ok(window)
    }

    /// Returns an active TLS rotation overlap window for one listener.
    pub(crate) fn inspect_rotation_window(
        &self,
        listener: VmTcpListener,
    ) -> Option<&VmTlsRotationWindow> {
        self.rotation_windows.get(&listener)
    }

    /// Retires an expired TLS rotation overlap window.
    pub(crate) fn retire_rotation_window(
        &mut self,
        listener: VmTcpListener,
        now_tick: u64,
    ) -> Result<Option<VmTlsRotationWindow>, String> {
        let Some(window) = self.rotation_windows.get(&listener).copied() else {
            return Ok(None);
        };
        if now_tick < window.retire_after_tick {
            return Err(format!(
                "VM TLS rotation overlap cannot retire until {}; now={now_tick}",
                window.retire_after_tick
            ));
        }
        Ok(self.rotation_windows.remove(&listener))
    }

    /// Returns the transport mode for one VM TCP listener handle.
    ///
    /// Inputs:
    /// - `listener`: VM-owned TCP listener resource.
    ///
    /// Output:
    /// - `Plaintext` for explicit plain mode.
    /// - `Tls` for manual, internal, or automatic certificate modes.
    /// - Stable diagnostic when no TLS plan is attached to the listener.
    ///
    /// Transformation:
    /// - Gives production HTTP a typed transport decision without matching on
    ///   certificate provisioning policy.
    pub(crate) fn listener_transport_mode(
        &self,
        listener: VmTcpListener,
    ) -> Result<VmTlsTransportMode, String> {
        let plan = self
            .listener_plans
            .get(&listener)
            .ok_or_else(|| "VM TLS listener handle has no installed transport plan".to_string())?;
        Ok(transport_mode_for_plan(plan))
    }

    /// Requires a listener to be plaintext before a raw protocol poll.
    ///
    /// Inputs:
    /// - `listener`: VM-owned TCP listener resource.
    ///
    /// Output:
    /// - Success for explicit plaintext listeners.
    /// - Stable diagnostic for TLS listeners because raw protocol polls must
    ///   use the TLS stream adapter instead of consuming encrypted bytes as
    ///   plaintext.
    ///
    /// Transformation:
    /// - Keeps the raw-protocol plaintext guard in the TLS runtime instead of
    ///   every protocol server inventing its own encrypted-listener diagnostic.
    pub(crate) fn require_plaintext_listener(&self, listener: VmTcpListener) -> Result<(), String> {
        match self.listener_transport_mode(listener)? {
            VmTlsTransportMode::Plaintext => Ok(()),
            VmTlsTransportMode::Tls => Err(
                "VM TLS listener requires TLS stream handling before protocol polling".to_string(),
            ),
        }
    }

    /// Builds a concrete rustls server configuration for one listener.
    ///
    /// Inputs:
    /// - `listener`: VM-owned TCP listener resource with an installed TLS plan.
    ///
    /// Output:
    /// - Maintained `rustls::ServerConfig` for manual and internal TLS modes.
    /// - Stable diagnostics for plaintext listeners, missing plans, unsupported
    ///   encrypted keys, and ACME plans that do not yet have cache-backed
    ///   certificate material.
    ///
    /// Transformation:
    /// - Moves the VM TLS boundary from metadata-only planning to concrete
    ///   TLS engine configuration without binding sockets, reading from host
    ///   async state, or implementing TLS parsing by hand.
    pub(crate) fn build_listener_server_config(
        &self,
        listener: VmTcpListener,
    ) -> Result<VmTlsServerConfig, String> {
        let plan = self
            .listener_plans
            .get(&listener)
            .ok_or_else(|| "VM TLS listener handle has no installed transport plan".to_string())?;
        match plan.mode {
            VmTlsMode::Plain => {
                Err("VM TLS plaintext listener does not require a server config".to_string())
            }
            VmTlsMode::Manual => build_manual_server_config(plan),
            VmTlsMode::Internal => build_internal_server_config(plan),
            VmTlsMode::Auto => Err(
                "VM TLS auto mode requires ACME certificate cache before server config".to_string(),
            ),
        }
    }

    /// Starts a rustls server connection for one listener.
    ///
    /// Inputs:
    /// - `listener`: VM-owned TCP listener resource with an installed TLS plan.
    ///
    /// Output:
    /// - Stateful rustls server connection for manual and internal TLS modes.
    /// - Stable diagnostics for plaintext, missing, or cache-incomplete modes.
    ///
    /// Transformation:
    /// - Converts listener TLS metadata into an inspectable TLS state machine
    ///   that future VM stream scheduling can drive with VM TCP bytes.
    pub(crate) fn start_listener_server_connection(
        &self,
        listener: VmTcpListener,
    ) -> Result<VmTlsServerConnection, String> {
        let config = self.build_listener_server_config(listener)?;
        let mode = config.mode;
        let connection = ServerConnection::new(config.server_config)
            .expect("validated VM TLS server config should start a server connection");
        Ok(VmTlsServerConnection { mode, connection })
    }

    /// Removes the TLS plan for one VM TCP listener handle.
    ///
    /// Inputs:
    /// - `listener`: VM-owned TCP listener resource being shut down.
    ///
    /// Output:
    /// - The removed plan when one was installed.
    ///
    /// Transformation:
    /// - Lets production HTTP/TCP listener cleanup release TLS metadata through
    ///   the same listener handle used for serving.
    pub(crate) fn remove_listener_plan(&mut self, listener: VmTcpListener) -> Option<VmTlsPlan> {
        self.rotation_windows.remove(&listener);
        self.listener_plans.remove(&listener)
    }
}

/// Derives the transport mode for one validated TLS plan.
fn transport_mode_for_plan(plan: &VmTlsPlan) -> VmTlsTransportMode {
    match plan.mode {
        VmTlsMode::Plain => VmTlsTransportMode::Plaintext,
        VmTlsMode::Manual | VmTlsMode::Internal | VmTlsMode::Auto => VmTlsTransportMode::Tls,
    }
}

/// Builds a rustls config from manual certificate files.
fn build_manual_server_config(plan: &VmTlsPlan) -> Result<VmTlsServerConfig, String> {
    if plan.passphrase_env.is_some() {
        return Err(
            "VM TLS manual encrypted private keys are not supported by VM runtime yet".to_string(),
        );
    }
    let cert_path = plan
        .cert_path
        .as_deref()
        .ok_or_else(|| "VM TLS manual mode requires cert_path".to_string())?;
    let key_path = plan
        .key_path
        .as_deref()
        .ok_or_else(|| "VM TLS manual mode requires key_path".to_string())?;
    let certificates = load_certificate_chain(Path::new(cert_path))?;
    let private_key = load_private_key(Path::new(key_path))?;
    Ok(VmTlsServerConfig {
        mode: VmTlsMode::Manual,
        server_config: Arc::new(rustls_server_config(certificates, private_key)?),
    })
}

/// Builds a rustls config from an internal self-signed certificate.
fn build_internal_server_config(plan: &VmTlsPlan) -> Result<VmTlsServerConfig, String> {
    let server_name = plan
        .server_name
        .as_deref()
        .ok_or_else(|| "VM TLS internal mode requires server_name".to_string())?;
    let generated = generate_simple_self_signed(vec![server_name.to_string()])
        .expect("VM TLS internal certificate generation should succeed for validated server names");
    let cert_der = generated.cert.der().as_ref().to_vec();
    let key_der = generated.key_pair.serialize_der();
    build_internal_server_config_from_der(cert_der, key_der)
}

fn build_internal_server_config_from_der(
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
) -> Result<VmTlsServerConfig, String> {
    Ok(VmTlsServerConfig {
        mode: VmTlsMode::Internal,
        server_config: Arc::new(rustls_server_config(
            vec![CertificateDer::from(cert_der)],
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key_der)),
        )?),
    })
}

/// Loads a PEM certificate chain through rustls-pemfile.
fn load_certificate_chain(path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
    let file = fs::File::open(path).map_err(|err| {
        format!(
            "VM TLS failed to open certificate `{}`: {err}",
            path.display()
        )
    })?;
    let certificates = rustls_pemfile::certs(&mut BufReader::new(file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            format!(
                "VM TLS failed to parse certificate `{}`: {err}",
                path.display()
            )
        })?;
    if certificates.is_empty() {
        return Err(format!(
            "VM TLS certificate `{}` did not contain any PEM certificates",
            path.display()
        ));
    }
    Ok(certificates)
}

/// Loads the first supported PEM private key through rustls-pemfile.
fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, String> {
    let file = fs::File::open(path).map_err(|err| {
        format!(
            "VM TLS failed to open private key `{}`: {err}",
            path.display()
        )
    })?;
    rustls_pemfile::private_key(&mut BufReader::new(file))
        .map_err(|err| {
            format!(
                "VM TLS failed to parse private key `{}`: {err}",
                path.display()
            )
        })?
        .ok_or_else(|| {
            format!(
                "VM TLS private key `{}` did not contain a supported unencrypted PEM key",
                path.display()
            )
        })
}

/// Builds a rustls server config with safe protocol defaults.
fn rustls_server_config(
    certificates: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
) -> Result<ServerConfig, String> {
    let builder =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("rustls ring provider should support safe default protocol versions");
    builder
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|err| format!("VM TLS failed to build server config: {err}"))
}

/// Validates one VM TLS plan.
fn validate_tls_plan(plan: &VmTlsPlan) -> Result<(), String> {
    match plan.mode {
        VmTlsMode::Plain => validate_plain_tls_plan(plan),
        VmTlsMode::Manual => validate_manual_tls_plan(plan),
        VmTlsMode::Internal => validate_internal_tls_plan(plan),
        VmTlsMode::Auto => validate_auto_tls_plan(plan),
    }
}

/// Validates plain HTTP has no TLS-only fields.
fn validate_plain_tls_plan(plan: &VmTlsPlan) -> Result<(), String> {
    if has_any_tls_field(plan) {
        return Err("VM TLS plain mode cannot include TLS configuration fields".to_string());
    }
    Ok(())
}

/// Validates manual TLS has certificate and key paths.
fn validate_manual_tls_plan(plan: &VmTlsPlan) -> Result<(), String> {
    if plan.cert_path.as_deref().unwrap_or("").trim().is_empty() {
        return Err("VM TLS manual mode requires cert_path".to_string());
    }
    if plan.key_path.as_deref().unwrap_or("").trim().is_empty() {
        return Err("VM TLS manual mode requires key_path".to_string());
    }
    if !plan.domains.is_empty()
        || plan.primary_provider.is_some()
        || plan.fallback_provider.is_some()
    {
        return Err("VM TLS manual mode cannot include ACME provider fields".to_string());
    }
    Ok(())
}

/// Validates internal TLS has a server name and no public ACME fields.
fn validate_internal_tls_plan(plan: &VmTlsPlan) -> Result<(), String> {
    if plan.server_name.as_deref().unwrap_or("").trim().is_empty() {
        return Err("VM TLS internal mode requires server_name".to_string());
    }
    if !plan.domains.is_empty()
        || plan.primary_provider.is_some()
        || plan.fallback_provider.is_some()
    {
        return Err("VM TLS internal mode cannot include ACME provider fields".to_string());
    }
    if plan.cert_path.is_some()
        || plan.key_path.is_some()
        || plan.passphrase_env.is_some()
        || plan.ca_path.is_some()
    {
        return Err("VM TLS internal mode cannot include manual certificate fields".to_string());
    }
    Ok(())
}

/// Validates automatic public TLS has domains and a primary provider.
fn validate_auto_tls_plan(plan: &VmTlsPlan) -> Result<(), String> {
    if plan.domains.is_empty() || plan.domains.iter().any(|domain| domain.trim().is_empty()) {
        return Err("VM TLS auto mode requires non-empty domains".to_string());
    }
    if plan.primary_provider.is_none() {
        return Err("VM TLS auto mode requires a primary provider".to_string());
    }
    if plan.cert_path.is_some()
        || plan.key_path.is_some()
        || plan.passphrase_env.is_some()
        || plan.ca_path.is_some()
        || plan.server_name.is_some()
        || plan.trust_local.is_some()
    {
        return Err("VM TLS auto mode cannot include manual certificate fields".to_string());
    }
    Ok(())
}

/// Returns whether a plan contains any TLS-specific field.
fn has_any_tls_field(plan: &VmTlsPlan) -> bool {
    !plan.domains.is_empty()
        || plan.email.is_some()
        || plan.primary_provider.is_some()
        || plan.fallback_provider.is_some()
        || plan.cert_path.is_some()
        || plan.key_path.is_some()
        || plan.passphrase_env.is_some()
        || plan.ca_path.is_some()
        || plan.server_name.is_some()
        || plan.trust_local.is_some()
}
