//! HTTPS startup policy. Discovery and execution are supplied by the serving host.

use std::path::Path;

use crate::acme::{
    acme_runtime_plan, load_acme_runtime_tls_cache, validate_acme_provider_supported,
    AcmeRuntimePlan,
};
use crate::tls_config::{Config, Mode, Provider};
use crate::tls_material::{self, RuntimeConfig};

/// Loads manual/internal material or admits a validated automatic cache.
/// The issuer is invoked once, only for an absent cache. Corrupt, stale, or
/// mismatched caches fail closed instead of triggering implicit issuance.
pub fn load(
    project_root: &Path,
    tls: &Config,
    issuer: impl FnOnce(&AcmeRuntimePlan) -> Result<(), String>,
) -> Result<RuntimeConfig, String> {
    match tls.mode {
        Mode::Manual => {
            crate::tls_paths::validate_manual_tls_file_references(project_root, tls)?;
            tls_material::manual(project_root, tls)
        }
        Mode::Internal => tls_material::internal(tls),
        Mode::Auto => {
            let plan = acme_runtime_plan(project_root, tls);
            validate_acme_provider_supported(&plan)?;
            if let Some(config) = load_acme_runtime_tls_cache(&plan)? {
                return Ok(config);
            }
            issuer(&plan)?;
            load_acme_runtime_tls_cache(&plan)?.ok_or_else(|| format!(
                "error[serve_tls]: ACME issuer completed without writing certificate cache `{}` and `{}`",
                plan.certificate_path.display(), plan.private_key_path.display()
            ))
        }
    }
}

/// Loads configured TLS without permission to contact a certificate authority.
pub fn load_cached(project_root: &Path, tls: &Config) -> Result<RuntimeConfig, String> {
    load(project_root, tls, |plan| {
        let provider = match plan.primary_provider {
            Provider::LetsEncrypt => "letsencrypt",
            Provider::ZeroSsl => "zerossl",
        };
        Err(format!(
            "error[serve_tls]: automatic ACME TLS for domains [{}] has no local certificate cache yet; primary provider `{}` uses `{}` and cache `{}`; expected certificate `{}` and key `{}`; project `{}` should use mode `manual` or `internal` until issuance populates the cache",
            plan.domains.join(", "), provider, plan.directory_url, plan.cache_dir.display(),
            plan.certificate_path.display(), plan.private_key_path.display(), project_root.display()
        ))
    })
}

#[cfg(test)]
#[path = "tls_runtime_test.rs"]
mod tests;
