use super::*;

/// Detects every supported artifact-template suffix.
///
/// Inputs:
/// - Representative template paths for HTML, Markdown, JSON, TOML, YAML, YML,
///   and text.
///
/// Output:
/// - Test passes when every Terlan artifact-template suffix is recognized and
///   a plain HTML file is rejected.
///
/// Transformation:
/// - Exercises suffix-only discovery without parsing template bodies.
#[test]
fn detects_all_terlan_artifact_template_paths() {
    assert!(is_terlan_artifact_template_path("templates/page.terl.html"));
    assert!(is_terlan_artifact_template_path("templates/readme.terl.md"));
    assert!(is_terlan_artifact_template_path("templates/data.terl.json"));
    assert!(is_terlan_artifact_template_path(
        "templates/config.terl.toml"
    ));
    assert!(is_terlan_artifact_template_path(
        "templates/deploy.terl.yaml"
    ));
    assert!(is_terlan_artifact_template_path(
        "templates/deploy.terl.yml"
    ));
    assert!(is_terlan_artifact_template_path("templates/notes.terl.txt"));
    assert!(!is_terlan_artifact_template_path("templates/page.html"));
}

/// Classifies artifact-template filenames by target.
///
/// Inputs:
/// - Representative filenames for every supported artifact target.
///
/// Output:
/// - Test passes when each suffix maps to the expected target enum.
///
/// Transformation:
/// - Verifies deterministic filename classification, including `.terl.yml`
///   normalization to the YAML target.
#[test]
fn classifies_artifact_template_targets() {
    assert_eq!(
        artifact_template_target_from_filename("page.terl.html"),
        Some(ArtifactTemplateTarget::Html)
    );
    assert_eq!(
        artifact_template_target_from_filename("readme.terl.md"),
        Some(ArtifactTemplateTarget::Markdown)
    );
    assert_eq!(
        artifact_template_target_from_filename("data.terl.json"),
        Some(ArtifactTemplateTarget::Json)
    );
    assert_eq!(
        artifact_template_target_from_filename("config.terl.toml"),
        Some(ArtifactTemplateTarget::Toml)
    );
    assert_eq!(
        artifact_template_target_from_filename("deploy.terl.yaml"),
        Some(ArtifactTemplateTarget::Yaml)
    );
    assert_eq!(
        artifact_template_target_from_filename("deploy.terl.yml"),
        Some(ArtifactTemplateTarget::Yaml)
    );
    assert_eq!(
        artifact_template_target_from_filename("notes.terl.txt"),
        Some(ArtifactTemplateTarget::Text)
    );
    assert_eq!(artifact_template_target_from_filename("notes.txt"), None);
}

/// Distinguishes HTML-tree targets from structured/text targets.
///
/// Inputs:
/// - Artifact target enum variants.
///
/// Output:
/// - Test passes when only HTML and Markdown report HTML-tree parsing, and
///   target metadata remains stable.
///
/// Transformation:
/// - Locks down target metadata used by future validators and diagnostics.
#[test]
fn identifies_targets_that_parse_to_html_tree() {
    assert!(ArtifactTemplateTarget::Html.parses_to_html_tree());
    assert!(ArtifactTemplateTarget::Markdown.parses_to_html_tree());
    assert!(!ArtifactTemplateTarget::Json.parses_to_html_tree());
    assert_eq!(ArtifactTemplateTarget::Yaml.name(), "yaml");
    assert_eq!(ArtifactTemplateTarget::Yaml.suffix(), ".terl.yaml");
}
