//! Ed25519 verification for signed Registry and application payloads.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use rustls::SignatureScheme;

// RFC 8410 PKCS#8 `PrivateKeyInfo` prefix for a raw 32-byte Ed25519 seed.
// Keeping the encoding here lets the runtime use the already-selected rustls
// crypto provider instead of declaring the provider's implementation crate as
// a second public dependency boundary.
const ED25519_PKCS8_SEED_PREFIX: &[u8] = &[
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

#[derive(Debug, Clone, PartialEq, Eq)]
/// Base64-encoded public evidence produced for one Ed25519 signature.
pub struct SignedPayload {
    /// Raw Ed25519 public key encoded with standard base64.
    pub public_key_base64: String,
    /// Signature bytes encoded with standard base64.
    pub signature_base64: String,
}

/// Signs an exact UTF-8 payload with a base64-encoded 32-byte Ed25519 seed.
///
/// Invalid seed material fails closed. Callers are responsible for obtaining
/// the seed from a host secret capability and must never log it.
pub fn sign(seed_base64: &str, payload: &str) -> Option<SignedPayload> {
    let seed = STANDARD.decode(seed_base64).ok()?;
    let seed: [u8; 32] = seed.try_into().ok()?;
    let mut pkcs8 = Vec::with_capacity(ED25519_PKCS8_SEED_PREFIX.len() + seed.len());
    pkcs8.extend_from_slice(ED25519_PKCS8_SEED_PREFIX);
    pkcs8.extend_from_slice(&seed);
    let pkcs8 = rustls::pki_types::PrivatePkcs8KeyDer::from(pkcs8);
    let key = rustls::crypto::ring::sign::any_eddsa_type(&pkcs8).ok()?;
    let signer = key.choose_scheme(&[SignatureScheme::ED25519])?;
    let public_key = key.public_key()?;
    let public_key = public_key
        .as_ref()
        .get(public_key.as_ref().len().checked_sub(32)?..)?;
    let signature = signer.sign(payload.as_bytes()).ok()?;
    Some(SignedPayload {
        public_key_base64: STANDARD.encode(public_key),
        signature_base64: STANDARD.encode(signature),
    })
}

/// Verifies one base64-encoded Ed25519 public-key/signature pair.
///
/// Invalid encodings, key lengths, signatures, and payload mutations all
/// return `false`; verification never falls back to another algorithm.
pub fn verify(public_key_base64: &str, payload: &str, signature_base64: &str) -> bool {
    let Ok(public_key) = STANDARD.decode(public_key_base64) else {
        return false;
    };
    let Ok(signature) = STANDARD.decode(signature_base64) else {
        return false;
    };
    let provider = rustls::crypto::ring::default_provider();
    let Some((_, algorithms)) = provider
        .signature_verification_algorithms
        .mapping
        .iter()
        .find(|(scheme, _)| *scheme == SignatureScheme::ED25519)
    else {
        return false;
    };
    algorithms.iter().any(|algorithm| {
        algorithm
            .verify_signature(&public_key, payload.as_bytes(), &signature)
            .is_ok()
    })
}

#[cfg(test)]
#[path = "ed25519_test.rs"]
mod tests;
