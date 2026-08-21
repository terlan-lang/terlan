//! Transactional project-manifest commands for Registry dependencies.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use super::package_registry_error::RegistryResult;
use super::package_registry_transport::atomic_bytes;

pub(super) fn run_add(args: &[String], output_root: &Path) -> ExitCode {
    let Some((name, requirement, registry, resolver_args)) = parse_add(args) else {
        eprintln!("usage: terlc package add <name> <requirement> --registry <url> --trust-root <pin.json> [--offline] --out-dir <project-dir>");
        return ExitCode::from(2);
    };
    if !crate::package_registry::admission::canonical_package_name(&name)
        || crate::package_registry::parse_requirement(&requirement).is_err()
    {
        eprintln!("error[registry_add_dependency]: package name or requirement is invalid");
        return ExitCode::from(2);
    }
    let origin = match super::package_publish_live::registry_origin(&registry) {
        Ok(origin) => origin,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    mutate_and_resolve(
        output_root,
        &resolver_args,
        |source| add_dependency(source, &name, &requirement, &origin),
        "added",
        &name,
    )
}

pub(super) fn run_remove(args: &[String], output_root: &Path) -> ExitCode {
    let Some((name, resolver_args)) = parse_remove(args) else {
        eprintln!("usage: terlc package remove <name> --registry <url> --trust-root <pin.json> [--offline] --out-dir <project-dir>");
        return ExitCode::from(2);
    };
    if !crate::package_registry::admission::canonical_package_name(&name) {
        eprintln!("error[registry_remove_dependency]: package name is invalid");
        return ExitCode::from(2);
    }
    mutate_and_resolve(
        output_root,
        &resolver_args,
        |source| remove_dependency(source, &name),
        "removed",
        &name,
    )
}

fn mutate_and_resolve<F>(
    output_root: &Path,
    resolver_args: &[String],
    edit: F,
    action: &str,
    name: &str,
) -> ExitCode
where
    F: FnOnce(&str) -> RegistryResult<String>,
{
    let manifest_path = output_root.join(super::TERLAN_PROJECT_MANIFEST_FILE);
    let original = match fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("cannot read {}: {error}", manifest_path.display());
            return ExitCode::from(1);
        }
    };
    let source = match std::str::from_utf8(&original) {
        Ok(source) => source,
        Err(_) => {
            eprintln!("error[registry_manifest_encoding]: terlan.toml is not UTF-8");
            return ExitCode::from(1);
        }
    };
    if let Err(message) = super::project_manifest::read_project_manifest(&manifest_path) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    let updated = match edit(source) {
        Ok(updated) => updated,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    if let Err(message) = atomic_bytes(&manifest_path, updated.as_bytes()) {
        eprintln!("{message}");
        return ExitCode::from(1);
    }
    let manifest = match super::project_manifest::read_project_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(message) => {
            let _ = atomic_bytes(&manifest_path, &original);
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let has_registry = manifest.dependencies.iter().any(|dependency| {
        matches!(
            dependency.source,
            super::project_manifest::ProjectDependencySource::Registry { .. }
        )
    });
    let status = if has_registry {
        super::package_registry_resolver::run(resolver_args, output_root)
    } else {
        match super::package_registry_resolver::write_empty_lock(output_root) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        }
    };
    if status != ExitCode::SUCCESS {
        if let Err(error) = atomic_bytes(&manifest_path, &original) {
            eprintln!("error[registry_manifest_restore]: {error}");
        }
        return status;
    }
    println!("{action} Registry dependency `{name}`");
    status
}

fn parse_add(args: &[String]) -> Option<(String, String, String, Vec<String>)> {
    if args.first().map(String::as_str) != Some("add") || args.len() < 5 {
        return None;
    }
    let name = args.get(1)?.clone();
    let requirement = args.get(2)?.clone();
    if name.starts_with("--") || requirement.starts_with("--") {
        return None;
    }
    let tail = &args[3..];
    let registry = option_value(tail, "--registry")?.to_string();
    let mut resolver = vec!["resolve".into()];
    resolver.extend_from_slice(tail);
    Some((name, requirement, registry, resolver))
}

fn parse_remove(args: &[String]) -> Option<(String, Vec<String>)> {
    if args.first().map(String::as_str) != Some("remove") || args.len() < 4 {
        return None;
    }
    let name = args.get(1)?.clone();
    if name.starts_with("--") {
        return None;
    }
    let mut resolver = vec!["resolve".into()];
    resolver.extend_from_slice(&args[2..]);
    Some((name, resolver))
}

fn option_value<'a>(args: &'a [String], option: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].as_str())
}

fn add_dependency(
    source: &str,
    name: &str,
    requirement: &str,
    registry: &str,
) -> RegistryResult<String> {
    let line = format!("{name} = {{ registry = \"{registry}\", version = \"{requirement}\" }}\n");
    edit_dependency_section(source, name, Some(&line))
}

fn remove_dependency(source: &str, name: &str) -> RegistryResult<String> {
    edit_dependency_section(source, name, None)
}

fn edit_dependency_section(
    source: &str,
    name: &str,
    replacement: Option<&str>,
) -> RegistryResult<String> {
    let mut output = String::new();
    let mut in_dependencies = false;
    let mut section_found = false;
    let mut dependency_found = false;
    let mut inserted = false;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if in_dependencies && !inserted {
                if let Some(replacement) = replacement {
                    output.push_str(replacement);
                }
                inserted = true;
            }
            in_dependencies = trimmed == "[dependencies]";
            section_found |= in_dependencies;
            output.push_str(line);
            continue;
        }
        if in_dependencies
            && line
                .split_once('=')
                .is_some_and(|(key, _)| key.trim() == name)
        {
            dependency_found = true;
            if replacement.is_none() {
                continue;
            }
        }
        output.push_str(line);
    }
    if replacement.is_some() && dependency_found {
        return Err(
            format!("error[registry_dependency_exists]: `{name}` is already declared").into(),
        );
    }
    if replacement.is_none() && !dependency_found {
        return Err(format!(
            "error[registry_dependency_missing]: `{name}` is not declared in [dependencies]"
        )
        .into());
    }
    if let Some(replacement) = replacement {
        if section_found {
            if !inserted {
                if !output.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str(replacement);
            }
        } else {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("\n[dependencies]\n");
            output.push_str(replacement);
        }
    }
    Ok(output)
}

#[cfg(test)]
#[path = "package_registry_commands_test.rs"]
mod tests;
