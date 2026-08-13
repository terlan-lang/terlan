pub(super) use super::manifest::{write_browser_manifest, WebAssetArtifact};
pub(super) use super::routes::discover_web_handlers_from_modules;
pub(super) use super::routes::WebRouteManifestRows;
pub(super) use super::*;
pub(super) use crate::commands::emit_js::target_contract::js_target_contract;
pub(super) use crate::validation::target_profile::TargetProfile;

#[cfg(test)]
#[path = "js_browser_test/asset_and_response_manifests.rs"]
mod asset_and_response_manifests;
#[cfg(test)]
#[path = "js_browser_test/route_fixtures.rs"]
mod route_fixtures;
use route_fixtures::*;
