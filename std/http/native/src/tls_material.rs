//! Package-owned HTTPS certificate loading and engine configuration.
//! Host discovery and ACME issuance are not part of this material boundary.

use std::{fs, path::Path, sync::Arc};

use rcgen::generate_simple_self_signed;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;

use crate::tls_config::Config;

#[derive(Clone)]
pub struct RuntimeConfig {
    pub server_config: Arc<ServerConfig>,
}

/// Loads project-relative certificate material for manual HTTPS.
pub fn manual(project_root: &Path, tls: &Config) -> Result<RuntimeConfig, crate::ServiceError> {
    if tls.passphrase_env.is_some() {
        return Err(
            "error[serve_tls]: encrypted manual TLS keys are not supported by the local runtime yet"
                .to_string().into(),
        );
    }
    let cert = tls.cert.as_deref().ok_or_else(|| {
        "error[serve_tls]: manual TLS runtime requires a certificate path".to_string()
    })?;
    let key = tls
        .key
        .as_deref()
        .ok_or_else(|| "error[serve_tls]: manual TLS runtime requires a key path".to_string())?;
    let certificates = load_certificate_chain(&project_root.join(cert))?;
    let private_key = load_private_key(&project_root.join(key))?;
    let server_config = rustls_server_config(certificates, private_key)?;
    Ok(RuntimeConfig {
        server_config: Arc::new(server_config),
    })
}

/// Generates an in-memory local certificate through maintained rcgen.
pub fn internal(tls: &Config) -> Result<RuntimeConfig, crate::ServiceError> {
    let server_name = tls.server_name.as_deref().unwrap_or("localhost");
    let subject_alt_names = vec![server_name.to_string()];
    let generated = generate_simple_self_signed(subject_alt_names).map_err(|err| {
        format!("error[serve_tls]: failed to generate internal certificate: {err}")
    })?;
    let cert_der = generated.cert.der().as_ref().to_vec();
    let key_der = generated.key_pair.serialize_der();
    let server_config = rustls_server_config(
        vec![CertificateDer::from(cert_der)],
        PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key_der)),
    )?;
    Ok(RuntimeConfig {
        server_config: Arc::new(server_config),
    })
}

/// Loads a nonempty PEM chain using the shared maintained TLS parser.
pub fn load_certificate_chain(
    path: &Path,
) -> Result<Vec<CertificateDer<'static>>, crate::ServiceError> {
    let pem = fs::read(path).map_err(|err| {
        format!(
            "error[serve_tls]: failed to open TLS certificate `{}`: {err}",
            path.display()
        )
    })?;
    Ok(
        terlan_net_native::tls::parse_certificate_chain(&pem).map_err(|err| match err {
            rustls::pki_types::pem::Error::NoItemsFound => format!(
                "error[serve_tls]: TLS certificate `{}` did not contain any PEM certificates",
                path.display()
            ),
            err => {
                format!(
                    "error[serve_tls]: failed to parse TLS certificate `{}`: {err}",
                    path.display()
                )
            }
        })?,
    )
}

/// Loads the first supported unencrypted PEM key using the shared TLS parser.
pub fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, crate::ServiceError> {
    let pem = fs::read(path).map_err(|err| {
        format!(
            "error[serve_tls]: failed to open TLS private key `{}`: {err}",
            path.display()
        )
    })?;
    Ok(terlan_net_native::tls::parse_private_key(&pem).map_err(|err| match err {
        rustls::pki_types::pem::Error::NoItemsFound => format!(
            "error[serve_tls]: TLS private key `{}` did not contain a supported unencrypted PEM key",
            path.display()
        ),
        err => format!(
            "error[serve_tls]: failed to parse TLS private key `{}`: {err}",
            path.display()
        ),
    })?)
}

/// Applies HTTP ALPN preference to the shared safe rustls server defaults.
pub fn rustls_server_config(
    certificates: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
) -> Result<ServerConfig, crate::ServiceError> {
    let mut config = terlan_net_native::tls::server_config(certificates, private_key)
        .map_err(|err| format!("error[serve_tls]: failed to build TLS server config: {err}"))?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(config)
}

#[cfg(test)]
#[path = "tls_material_test.rs"]
mod tests;
