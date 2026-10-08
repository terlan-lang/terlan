pub(super) use super::manifest::{write_browser_manifest, WebAssetArtifact};
pub(super) use super::routes::discover_web_handlers_from_modules;
pub(super) use super::routes::WebRouteManifestRows;
pub(super) use super::*;
pub(super) use crate::commands::emit_js::target_contract::js_target_contract;
pub(super) use crate::validation::target_profile::TargetProfile;

#[cfg(test)]
#[path = "js_browser_test/asset_and_response_manifests.rs"]
mod asset_and_response_manifests;
#[path = "js_browser_test/callback_source_execution.rs"]
mod callback_source_execution;
#[path = "js_browser_test/response_source_execution.rs"]
mod response_source_execution;
#[cfg(test)]
#[path = "js_browser_test/route_fixtures.rs"]
mod route_fixtures;
use route_fixtures::*;

/// Verifies compiler-free VM service staging retains declared templates.
///
/// A source-relative template outside the declared source root is copied to
/// the same project-relative path in staging, while unrelated project files
/// are not admitted into the service package.
#[test]
fn vm_service_staging_copies_only_declared_project_templates() {
    let project = temp_dir("vm_service_template_dependencies");
    let source_dir = project.join("src/app");
    let templates_dir = project.join("templates");
    let staging = project.join("staging");
    fs::create_dir_all(&source_dir).expect("create source directory");
    fs::create_dir_all(&templates_dir).expect("create templates directory");
    fs::create_dir_all(&staging).expect("create staging directory");
    fs::write(
        source_dir.join("Http.terl"),
        "module app.Http.\n\ntemplate Page from \"../../templates/page.terl.html\" { title: String }.\n",
    )
    .expect("write template source declaration");
    fs::write(templates_dir.join("page.terl.html"), "<main>{title}</main>")
        .expect("write declared template");
    fs::write(project.join("operator-secret.txt"), "do-not-package")
        .expect("write unrelated project file");

    copy_vm_service_template_dependencies(&project, &staging, &["src".to_string()])
        .expect("copy declared template dependencies");

    assert_eq!(
        fs::read_to_string(staging.join("templates/page.terl.html")).expect("read staged template"),
        "<main>{title}</main>"
    );
    assert!(!staging.join("operator-secret.txt").exists());
}
