use super::{
    vm_tls_plan_from_project_tls, ProjectServerTls, ProjectServerTlsMode, ProjectServerTlsProvider,
};
use crate::runtime::vm::tls::{VmTlsMode, VmTlsProvider};

fn manual_tls() -> ProjectServerTls {
    ProjectServerTls {
        mode: ProjectServerTlsMode::Manual,
        domains: Vec::new(),
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert: Some("cert.pem".to_string()),
        key: Some("key.pem".to_string()),
        passphrase_env: Some("TERLAN_TLS_PASSPHRASE".to_string()),
        ca: Some("ca.pem".to_string()),
        server_name: Some("localhost".to_string()),
        trust_local: None,
    }
}

fn auto_tls() -> ProjectServerTls {
    ProjectServerTls {
        mode: ProjectServerTlsMode::Auto,
        domains: vec!["example.com".to_string()],
        email: Some("admin@example.com".to_string()),
        primary_provider: Some(ProjectServerTlsProvider::LetsEncrypt),
        fallback_provider: Some(ProjectServerTlsProvider::ZeroSsl),
        cert: None,
        key: None,
        passphrase_env: None,
        ca: None,
        server_name: None,
        trust_local: None,
    }
}

fn internal_tls() -> ProjectServerTls {
    ProjectServerTls {
        mode: ProjectServerTlsMode::Internal,
        domains: Vec::new(),
        email: None,
        primary_provider: None,
        fallback_provider: None,
        cert: None,
        key: None,
        passphrase_env: None,
        ca: None,
        server_name: Some("localhost".to_string()),
        trust_local: Some(true),
    }
}

#[test]
fn project_tls_manual_converts_to_vm_tls_plan_without_dropping_fields() {
    let plan = vm_tls_plan_from_project_tls(&manual_tls()).expect("manual tls plan");

    assert_eq!(plan.mode, VmTlsMode::Manual);
    assert_eq!(plan.cert_path.as_deref(), Some("cert.pem"));
    assert_eq!(plan.key_path.as_deref(), Some("key.pem"));
    assert_eq!(
        plan.passphrase_env.as_deref(),
        Some("TERLAN_TLS_PASSPHRASE")
    );
    assert_eq!(plan.ca_path.as_deref(), Some("ca.pem"));
    assert_eq!(plan.server_name.as_deref(), Some("localhost"));
}

#[test]
fn project_tls_auto_converts_to_vm_tls_plan_with_acme_providers() {
    let plan = vm_tls_plan_from_project_tls(&auto_tls()).expect("auto tls plan");

    assert_eq!(plan.mode, VmTlsMode::Auto);
    assert_eq!(plan.domains, vec!["example.com".to_string()]);
    assert_eq!(plan.email.as_deref(), Some("admin@example.com"));
    assert_eq!(plan.primary_provider, Some(VmTlsProvider::LetsEncrypt));
    assert_eq!(plan.fallback_provider, Some(VmTlsProvider::ZeroSsl));
}

#[test]
fn project_tls_internal_converts_to_vm_tls_plan_with_local_trust_metadata() {
    let plan = vm_tls_plan_from_project_tls(&internal_tls()).expect("internal tls plan");

    assert_eq!(plan.mode, VmTlsMode::Internal);
    assert_eq!(plan.server_name.as_deref(), Some("localhost"));
    assert_eq!(plan.trust_local, Some(true));
}

#[test]
fn project_tls_bridge_rejects_mode_mismatches_before_transport_start() {
    let mut tls = auto_tls();
    tls.cert = Some("cert.pem".to_string());

    assert_eq!(
        vm_tls_plan_from_project_tls(&tls).expect_err("mixed auto/manual tls should fail"),
        "VM TLS auto mode cannot include manual certificate fields"
    );
}
