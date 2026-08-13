use crate::runtime::vm::tls::{VmTlsMode, VmTlsPlan, VmTlsProvider, VmTlsRuntime};

use super::{ProjectServerTls, ProjectServerTlsMode, ProjectServerTlsProvider};

/// Converts project manifest TLS metadata into a VM TLS plan.
///
/// Inputs:
/// - `tls`: parsed `[server.tls]` metadata from `terlan.toml`.
///
/// Output:
/// - VM-owned TLS plan ready to install into a VM listener registry, or a
///   stable validation diagnostic.
///
/// Transformation:
/// - Maps CLI/project manifest vocabulary to VM runtime vocabulary without
///   loading certificates, contacting ACME providers, binding sockets, or
///   depending on a host async runtime.
pub(crate) fn vm_tls_plan_from_project_tls(tls: &ProjectServerTls) -> Result<VmTlsPlan, String> {
    let plan = VmTlsPlan {
        mode: vm_tls_mode(tls.mode),
        domains: tls.domains.clone(),
        email: tls.email.clone(),
        primary_provider: tls.primary_provider.map(vm_tls_provider),
        fallback_provider: tls.fallback_provider.map(vm_tls_provider),
        cert_path: tls.cert.clone(),
        key_path: tls.key.clone(),
        passphrase_env: tls.passphrase_env.clone(),
        ca_path: tls.ca.clone(),
        server_name: tls.server_name.clone(),
        trust_local: tls.trust_local,
    };
    validate_vm_tls_plan(&plan)?;
    Ok(plan)
}

/// Converts a manifest TLS mode to a VM TLS mode.
fn vm_tls_mode(mode: ProjectServerTlsMode) -> VmTlsMode {
    match mode {
        ProjectServerTlsMode::Auto => VmTlsMode::Auto,
        ProjectServerTlsMode::Manual => VmTlsMode::Manual,
        ProjectServerTlsMode::Internal => VmTlsMode::Internal,
    }
}

/// Converts a manifest ACME provider to a VM ACME provider.
fn vm_tls_provider(provider: ProjectServerTlsProvider) -> VmTlsProvider {
    match provider {
        ProjectServerTlsProvider::LetsEncrypt => VmTlsProvider::LetsEncrypt,
        ProjectServerTlsProvider::ZeroSsl => VmTlsProvider::ZeroSsl,
    }
}

/// Validates a VM TLS plan using the VM-owned registry rules.
fn validate_vm_tls_plan(plan: &VmTlsPlan) -> Result<(), String> {
    let mut runtime = VmTlsRuntime::new();
    runtime.install_plan("manifest", plan.clone())
}
