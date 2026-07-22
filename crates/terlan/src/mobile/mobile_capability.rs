//! Typed mobile native capability resources.
#![allow(dead_code)]
//!
//! Inputs:
//! - Mobile bridge capabilities required by native shell declarations.
//!
//! Outputs:
//! - Native-boundary resource metadata for mobile capabilities.
//!
//! Transformation:
//! - Treats mobile platform capabilities as typed resources instead of raw
//!   strings before target-specific permission enforcement exists.

use std::collections::BTreeSet;

use super::mobile_bridge::MobileBridgeCapability;

/// Boundary kind used by mobile native capability resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MobileNativeCapabilityBoundary {
    NativeBoundary,
}

impl MobileNativeCapabilityBoundary {
    /// Returns the stable metadata spelling for the boundary kind.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NativeBoundary => "native_boundary",
        }
    }
}

/// Generated mobile native capability resource manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileNativeCapabilityResourceManifest {
    pub(crate) schema_version: u32,
    pub(crate) resources: Vec<MobileNativeCapabilityResource>,
}

/// Explicit mobile native service declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileNativeServiceDeclaration {
    pub(crate) name: String,
    pub(crate) capabilities: Vec<MobileBridgeCapability>,
}

/// One typed native-boundary resource required by a mobile capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileNativeCapabilityResource {
    pub(crate) name: String,
    pub(crate) capability: &'static str,
    pub(crate) boundary: &'static str,
}

/// Validation diagnostic for mobile native capability resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MobileNativeCapabilityDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Generates native-boundary resource metadata for mobile capabilities.
///
/// Inputs:
/// - `capabilities`: typed mobile bridge capabilities.
///
/// Output:
/// - Schema-versioned resource manifest.
/// - Stable diagnostics for duplicate capabilities.
///
/// Transformation:
/// - Converts each capability into a deterministic `mobile.<capability>`
///   resource with the `native_boundary` boundary kind.
pub(crate) fn generate_mobile_native_capability_resources(
    capabilities: &[MobileBridgeCapability],
) -> Result<MobileNativeCapabilityResourceManifest, Vec<MobileNativeCapabilityDiagnostic>> {
    validate_mobile_native_capabilities(capabilities)?;
    Ok(MobileNativeCapabilityResourceManifest {
        schema_version: 1,
        resources: capabilities
            .iter()
            .map(|capability| {
                let name = format!("mobile.{}", capability.as_str());
                MobileNativeCapabilityResource {
                    name,
                    capability: capability.as_str(),
                    boundary: MobileNativeCapabilityBoundary::NativeBoundary.as_str(),
                }
            })
            .collect(),
    })
}

/// Generates native-boundary resource metadata from explicit service declarations.
///
/// Inputs:
/// - `services`: mobile native services with explicitly declared capabilities.
///
/// Output:
/// - Schema-versioned resource manifest.
/// - Stable diagnostics for blank names, duplicate services, missing
///   capabilities, and duplicate service-local capabilities.
///
/// Transformation:
/// - Converts each service capability into a deterministic
///   `mobile.<service>.<capability>` resource with the `native_boundary`
///   boundary kind.
pub(crate) fn generate_mobile_native_service_capability_resources(
    services: &[MobileNativeServiceDeclaration],
) -> Result<MobileNativeCapabilityResourceManifest, Vec<MobileNativeCapabilityDiagnostic>> {
    validate_mobile_native_service_declarations(services)?;
    Ok(MobileNativeCapabilityResourceManifest {
        schema_version: 1,
        resources: services
            .iter()
            .flat_map(|service| {
                service.capabilities.iter().map(|capability| {
                    let name = format!("mobile.{}.{}", service.name, capability.as_str());
                    MobileNativeCapabilityResource {
                        name,
                        capability: capability.as_str(),
                        boundary: MobileNativeCapabilityBoundary::NativeBoundary.as_str(),
                    }
                })
            })
            .collect(),
    })
}

/// Validates mobile native capability resource inputs.
///
/// Inputs:
/// - `capabilities`: typed mobile bridge capabilities.
///
/// Output:
/// - `Ok(())` when each capability appears at most once.
/// - Stable diagnostics for duplicate capability resources.
///
/// Transformation:
/// - Uses capability enum spellings as resource keys so validation remains
///   independent from target-specific platform services.
fn validate_mobile_native_capabilities(
    capabilities: &[MobileBridgeCapability],
) -> Result<(), Vec<MobileNativeCapabilityDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    for capability in capabilities {
        if !seen.insert(capability.as_str()) {
            diagnostics.push(MobileNativeCapabilityDiagnostic {
                code: "mobile_native_capability_duplicate",
                message: format!(
                    "mobile native capability `{}` is declared more than once",
                    capability.as_str()
                ),
            });
        }
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Validates explicit mobile native service declarations.
///
/// Inputs:
/// - `services`: typed native service declarations.
///
/// Output:
/// - `Ok(())` when every service has a unique non-empty name and at least one
///   explicitly declared capability.
/// - Stable diagnostics otherwise.
///
/// Transformation:
/// - Keeps capability discovery explicit so native service resources cannot be
///   inferred from implementation details or bridge payloads.
fn validate_mobile_native_service_declarations(
    services: &[MobileNativeServiceDeclaration],
) -> Result<(), Vec<MobileNativeCapabilityDiagnostic>> {
    let mut diagnostics = Vec::new();
    let mut service_names = BTreeSet::new();
    for service in services {
        if service.name.trim().is_empty() {
            diagnostics.push(MobileNativeCapabilityDiagnostic {
                code: "mobile_native_service_empty_name",
                message: "mobile native service name must not be empty".to_string(),
            });
        } else if !service_names.insert(service.name.as_str()) {
            diagnostics.push(MobileNativeCapabilityDiagnostic {
                code: "mobile_native_service_duplicate",
                message: format!(
                    "mobile native service `{}` is declared more than once",
                    service.name
                ),
            });
        }

        if service.capabilities.is_empty() {
            diagnostics.push(MobileNativeCapabilityDiagnostic {
                code: "mobile_native_service_missing_capability",
                message: format!(
                    "mobile native service `{}` must declare at least one capability",
                    service.name
                ),
            });
            continue;
        }

        let mut seen_capabilities = BTreeSet::new();
        for capability in &service.capabilities {
            if !seen_capabilities.insert(capability.as_str()) {
                diagnostics.push(MobileNativeCapabilityDiagnostic {
                    code: "mobile_native_service_duplicate_capability",
                    message: format!(
                        "mobile native service `{}` declares capability `{}` more than once",
                        service.name,
                        capability.as_str()
                    ),
                });
            }
        }
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

#[cfg(test)]
#[path = "mobile_capability_test.rs"]
mod mobile_capability_test;
