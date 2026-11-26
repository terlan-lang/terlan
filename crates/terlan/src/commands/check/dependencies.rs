//! Source interfaces for the selected package's declared dependency graph.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::commands::build::{project_check_source_roots, project_manifest};
use crate::commands::source_layout::validate_module_layout;
use crate::formal_pipeline::{parse_source_as_syntax_output, terlan_check_sources_in_dir};
use crate::terlan_syntax::SyntaxModuleOutput;

/// Reads dependency APIs using the same package resolution as build and test.
/// Handwritten interfaces take precedence over their implementations.
pub(super) fn source_interfaces(
    checked_root: &Path,
) -> Result<Vec<(PathBuf, SyntaxModuleOutput)>, String> {
    let checked_root = checked_root.canonicalize().map_err(|error| {
        format!(
            "cannot resolve check source root {}: {error}",
            checked_root.display()
        )
    })?;
    let Some(project_dir) = checked_root
        .ancestors()
        .find(|parent| parent.join("terlan.toml").is_file())
    else {
        return Ok(Vec::new());
    };
    let manifest = project_manifest::read_project_manifest(&project_dir.join("terlan.toml"))?;
    if manifest.dependencies.is_empty() {
        return Ok(Vec::new());
    }
    let own_roots = manifest
        .source_roots
        .iter()
        .map(|root| project_dir.join(root).canonicalize())
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| format!("cannot resolve package source roots: {error}"))?;
    let roots = project_check_source_roots(project_dir, &manifest)?;
    interfaces_from_roots(
        &roots
            .into_iter()
            .filter(|root| !own_roots.contains(root))
            .collect::<Vec<_>>(),
    )
}

/// Reads APIs from a dependency graph already resolved by the owning consumer.
/// Current generated summaries avoid reparsing implementation bodies; missing,
/// malformed, or stale summaries fall back to their authoritative source.
pub(super) fn interfaces_from_roots(
    roots: &[PathBuf],
) -> Result<Vec<(PathBuf, SyntaxModuleOutput)>, String> {
    let mut sources = Vec::new();
    for root in roots {
        let cache_dir = root.join(".terlan");
        for path in terlan_check_sources_in_dir(root).map_err(|error| error.to_string())? {
            if path
                .extension()
                .is_some_and(|extension| extension == "terl")
                && path.with_extension("terli").is_file()
            {
                continue;
            }
            let text = crate::support::read_file(&path.to_string_lossy())?;
            let syntax = if let Some(interface) = path
                .extension()
                .is_some_and(|extension| extension == "terl")
                .then(|| {
                    crate::commands::build::load_current_cached_interface_syntax(
                        root, &path, &text, &cache_dir,
                    )
                })
                .flatten()
            {
                interface
            } else {
                parse_source_as_syntax_output(&path.to_string_lossy(), &text).map_err(|error| {
                    format!("cannot parse dependency {}: {error:?}", path.display())
                })?
            };
            validate_module_layout(root, &path, &syntax.module_name)?;
            sources.push((path, syntax));
        }
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::commands::artifacts::{fingerprint, DependencyManifest};
    use crate::commands::build::prepare_source_root_interfaces;
    use crate::support::test_fs::TestDirectory;
    use crate::terlan_hir::parse_interface_file;
    use crate::terlan_syntax::{
        cached_canonical_terlan_syntax_contract_identity, SyntaxSourceKind,
    };
    use crate::CliState;

    use super::interfaces_from_roots;

    #[test]
    fn dependency_roots_reuse_only_current_generated_interfaces() {
        let directory = TestDirectory::new("check", "cached_dependency_interfaces");
        let root = directory.path().join("src");
        let source_path = root.join("app/Dependency.terl");
        let cache_dir = root.join(".terlan");
        fs::create_dir_all(source_path.parent().expect("source parent"))
            .expect("create source directory");
        let source = "module app.Dependency. pub value(): Int -> 1.";
        fs::write(&source_path, source).expect("write dependency source");
        let state = CliState {
            incremental: true,
            cache_dir: Some(cache_dir.clone()),
            ..CliState::default()
        };
        prepare_source_root_interfaces(&root, &state).expect("prepare dependency interface");
        let (_, interface) = parse_interface_file(&cache_dir.join("app.Dependency.typi"))
            .expect("parse generated interface");
        let manifest = DependencyManifest {
            module: "app.Dependency".to_string(),
            syntax_contract_identity: cached_canonical_terlan_syntax_contract_identity()
                .expect("syntax identity"),
            source_hash: fingerprint(source.as_bytes()),
            interface_hash: fingerprint(interface.to_terlan_interface_type_text().as_bytes()),
            interface_doc_hash: fingerprint(interface.to_terlan_interface_doc_text().as_bytes()),
            dependencies: Vec::new(),
        };
        fs::write(
            cache_dir.join("app.Dependency.typi.deps"),
            manifest.encode(),
        )
        .expect("write dependency manifest");

        let cached = interfaces_from_roots(std::slice::from_ref(&root))
            .expect("load current dependency interface");
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].1.source_kind, SyntaxSourceKind::Interface);

        fs::write(
            &source_path,
            "module app.Dependency. pub value(): Int -> 2.",
        )
        .expect("change dependency source");
        let stale = interfaces_from_roots(&[root]).expect("fall back to dependency source");
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].1.source_kind, SyntaxSourceKind::Module);
    }
}
