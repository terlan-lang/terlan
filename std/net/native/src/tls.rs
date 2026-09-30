//! Maintained TLS material parsing and engine construction. No file I/O,
//! certificate policy, HTTP ALPN selection, scheduler, or host executor.

use std::sync::Arc;

use rustls::pki_types::{
    pem::{Error as PemError, PemObject},
    CertificateDer, PrivateKeyDer,
};
use rustls::ServerConfig;

/// Reads every certificate PEM block in order, rejecting empty or malformed chains.
/// DER certificate validity and key matching are checked by `server_config`.
pub fn parse_certificate_chain(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, PemError> {
    let certificates = CertificateDer::pem_slice_iter(pem).collect::<Result<Vec<_>, _>>()?;
    if certificates.is_empty() {
        return Err(PemError::NoItemsFound);
    }
    Ok(certificates)
}

/// Reads the first supported unencrypted PKCS#8, PKCS#1, or SEC1 PEM key.
pub fn parse_private_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>, PemError> {
    PrivateKeyDer::from_pem_slice(pem)
}

/// Builds a ring-backed TLS server with maintained safe protocol defaults and
/// no client authentication. Callers select their application protocols explicitly.
pub fn server_config(
    certificates: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
) -> Result<ServerConfig, rustls::Error> {
    ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
}

#[cfg(test)]
#[path = "tls_test.rs"]
mod tests;
