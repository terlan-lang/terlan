use super::*;
use crate::acme::cache::store_acme_certificate_cache;
use crate::tls_config::Settings;
use rcgen::generate_simple_self_signed;
use std::{cell::Cell, fs};

fn auto() -> Config {
    Settings {
        mode: Some(Mode::Auto),
        domains: Some(vec!["example.test".into()]),
        ..Settings::default()
    }
    .validate()
    .unwrap()
}

fn populate(plan: &AcmeRuntimePlan) -> Result<(), String> {
    let generated = generate_simple_self_signed(plan.domains.clone()).unwrap();
    Ok(store_acme_certificate_cache(
        plan,
        &generated.cert.pem(),
        &generated.key_pair.serialize_pem(),
    )?)
}

#[test]
fn startup_invokes_issuer_once_and_then_uses_the_validated_cache() {
    let directory = tempfile::tempdir().unwrap();
    let tls = auto();
    let calls = Cell::new(0);
    let runtime = load(directory.path(), &tls, |plan| {
        calls.set(calls.get() + 1);
        assert_eq!(plan.domains, tls.domains);
        Ok(populate(plan)?)
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(
        runtime.server_config.alpn_protocols,
        [b"h2".to_vec(), b"http/1.1".to_vec()]
    );
    load(directory.path(), &tls, |_| {
        panic!("fresh cache must not reissue")
    })
    .unwrap();
    load_cached(directory.path(), &tls).unwrap();
}

#[test]
fn cached_only_mode_never_issues_and_issuer_success_requires_valid_material() {
    let directory = tempfile::tempdir().unwrap();
    let tls = auto();
    let error = load_cached(directory.path(), &tls).err().unwrap();
    assert!(error
        .to_string()
        .contains("has no local certificate cache yet"));
    assert!(error.to_string().contains("example.test"));
    assert!(load(directory.path(), &tls, |_| Ok(()))
        .err()
        .unwrap()
        .to_string()
        .contains("without writing certificate cache"));
    assert_eq!(
        load(directory.path(), &tls, |_| Err("cancelled".into()))
            .err()
            .unwrap()
            .to_string(),
        "cancelled"
    );
    let result = load(directory.path(), &tls, |plan| {
        fs::create_dir_all(&plan.cache_dir).unwrap();
        fs::write(&plan.certificate_path, "not a certificate").unwrap();
        Ok(())
    });
    assert!(result.err().unwrap().to_string().contains("incomplete"));
}

#[test]
fn malformed_cache_and_unsupported_provider_never_trigger_issuance() {
    let directory = tempfile::tempdir().unwrap();
    let tls = auto();
    let plan = acme_runtime_plan(directory.path(), &tls);
    populate(&plan).unwrap();
    fs::write(&plan.renewal_metadata_path, "not JSON").unwrap();
    assert!(load(directory.path(), &tls, |_| panic!(
        "invalid cache must fail closed"
    ))
    .is_err());
    let mut unsupported = tls;
    unsupported.primary_provider = Some(Provider::ZeroSsl);
    let error = load(directory.path(), &unsupported, |_| {
        panic!("unsupported provider")
    })
    .err()
    .unwrap();
    assert!(error.to_string().contains("ZeroSSL"), "{error}");
}

#[test]
fn internal_and_manual_startup_never_call_the_acme_issuer() {
    let directory = tempfile::tempdir().unwrap();
    let internal = Settings {
        mode: Some(Mode::Internal),
        ..Settings::default()
    }
    .validate()
    .unwrap();
    load(directory.path(), &internal, |_| {
        panic!("internal mode cannot issue")
    })
    .unwrap();
    let generated = generate_simple_self_signed(vec!["example.test".into()]).unwrap();
    fs::write(directory.path().join("cert.pem"), generated.cert.pem()).unwrap();
    fs::write(
        directory.path().join("key.pem"),
        generated.key_pair.serialize_pem(),
    )
    .unwrap();
    let manual = Settings {
        mode: Some(Mode::Manual),
        cert: Some("cert.pem".into()),
        key: Some("key.pem".into()),
        ..Settings::default()
    }
    .validate()
    .unwrap();
    load(directory.path(), &manual, |_| {
        panic!("manual mode cannot issue")
    })
    .unwrap();
    fs::remove_file(directory.path().join("key.pem")).unwrap();
    assert!(load(directory.path(), &manual, |_| panic!(
        "invalid manual mode cannot issue"
    ))
    .is_err());
}

#[test]
fn issuer_result_is_not_trusted_without_domain_and_key_validation() {
    for tamper in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let tls = auto();
        let result = load(directory.path(), &tls, |plan| {
            populate(plan)?;
            match tamper {
                0 => fs::write(&plan.private_key_path, "not a key").unwrap(),
                1 => fs::remove_file(&plan.renewal_metadata_path).unwrap(),
                _ => {
                    let other = generate_simple_self_signed(vec!["other.test".into()]).unwrap();
                    store_acme_certificate_cache(
                        plan,
                        &other.cert.pem(),
                        &other.key_pair.serialize_pem(),
                    )?;
                }
            }
            Ok(())
        });
        assert!(
            result.is_err(),
            "tampered issuer result {tamper} was accepted"
        );
    }
}
