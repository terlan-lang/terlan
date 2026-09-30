use super::*;

fn certificate() -> rcgen::CertifiedKey {
    rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap()
}

#[test]
fn certificate_parser_preserves_order_and_rejects_missing_or_partial_chains() {
    let first = certificate();
    let second = certificate();
    let pem = format!("{}{}", first.cert.pem(), second.cert.pem());
    let chain = parse_certificate_chain(pem.as_bytes()).unwrap();
    assert_eq!(chain, [first.cert.der().clone(), second.cert.der().clone()]);
    for pem in [
        b"".as_slice(),
        b"plain text",
        first.key_pair.serialize_pem().as_bytes(),
    ] {
        assert!(matches!(
            parse_certificate_chain(pem),
            Err(PemError::NoItemsFound)
        ));
    }
    for suffix in [
        "-----BEGIN CERTIFICATE-----\n!\n-----END CERTIFICATE-----\n",
        "-----BEGIN CERTIFICATE-----\nAA==\n",
        "-----BEGIN CERTIFICATE\n",
    ] {
        assert!(parse_certificate_chain(suffix.as_bytes()).is_err());
        assert!(
            parse_certificate_chain(format!("{}{suffix}", first.cert.pem()).as_bytes()).is_err()
        );
    }
}

#[test]
fn key_parser_reuses_maintained_supported_formats_and_first_key_selection() {
    let first = certificate();
    let second = certificate();
    let pem = format!(
        "{}{}{}",
        first.cert.pem(),
        first.key_pair.serialize_pem(),
        second.key_pair.serialize_pem()
    );
    assert_eq!(
        parse_private_key(pem.as_bytes()).unwrap().secret_der(),
        first.key_pair.serialize_der()
    );
    for (label, format) in [
        ("PRIVATE KEY", "pkcs8"),
        ("RSA PRIVATE KEY", "pkcs1"),
        ("EC PRIVATE KEY", "sec1"),
    ] {
        let key = parse_private_key(
            format!("-----BEGIN {label}-----\nAA==\n-----END {label}-----\n").as_bytes(),
        )
        .unwrap();
        assert_eq!(key.secret_der(), [0]);
        assert!(matches!(
            (&key, format),
            (PrivateKeyDer::Pkcs8(_), "pkcs8")
                | (PrivateKeyDer::Pkcs1(_), "pkcs1")
                | (PrivateKeyDer::Sec1(_), "sec1")
        ));
        assert!(server_config(vec![first.cert.der().clone()], key).is_err());
    }
}

#[test]
fn invalid_and_encrypted_key_material_fails_closed() {
    let cert = certificate();
    for pem in [
        "",
        "not a key",
        &cert.cert.pem(),
        "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAA==\n-----END ENCRYPTED PRIVATE KEY-----\n",
    ] {
        assert!(matches!(
            parse_private_key(pem.as_bytes()),
            Err(PemError::NoItemsFound)
        ));
    }
    for pem in [
        "-----BEGIN PRIVATE KEY-----\n!\n-----END PRIVATE KEY-----\n",
        "-----BEGIN PRIVATE KEY-----\nAA==\n",
        "-----BEGIN PRIVATE KEY\n",
    ] {
        assert!(parse_private_key(pem.as_bytes()).is_err());
    }
}

#[test]
fn server_config_validates_identity_without_installing_process_global_crypto_or_http_policy() {
    let cert = certificate();
    let config = server_config(
        parse_certificate_chain(cert.cert.pem().as_bytes()).unwrap(),
        parse_private_key(cert.key_pair.serialize_pem().as_bytes()).unwrap(),
    )
    .unwrap();
    assert!(config.alpn_protocols.is_empty());
    assert!(server_config(
        vec![],
        parse_private_key(cert.key_pair.serialize_pem().as_bytes()).unwrap()
    )
    .is_err());
    assert!(server_config(
        vec![CertificateDer::from(vec![0])],
        parse_private_key(cert.key_pair.serialize_pem().as_bytes()).unwrap()
    )
    .is_err());
    let wrong = certificate();
    assert!(server_config(
        vec![cert.cert.der().clone()],
        parse_private_key(wrong.key_pair.serialize_pem().as_bytes()).unwrap()
    )
    .is_err());
}
