use super::*;

pub(super) fn validate_c_inputs(metadata: &CMetadata, input_dir: &Path) -> Result<(), String> {
    if let Some(link) = &metadata.external_link {
        match (&link.root_env, &link.pkg_config) {
            (Some(root_env), None) => {
                if !is_environment_variable(root_env) {
                    return Err(format!(
                        "external C root environment variable `{root_env}` is invalid"
                    ));
                }
                if link.library_dirs.is_empty() || link.libraries.is_empty() {
                    return Err(
                        "environment-rooted external C metadata requires library directories and libraries"
                            .into(),
                    );
                }
            }
            (None, Some(pkg_config)) => {
                if !is_link_library_name(&pkg_config.package) {
                    return Err(format!(
                        "external C pkg-config package `{}` is invalid",
                        pkg_config.package
                    ));
                }
                if pkg_config
                    .min_version
                    .as_deref()
                    .is_some_and(|version| !is_version_requirement(version))
                {
                    return Err("external C pkg-config min_version is invalid".into());
                }
                if !link.library_dirs.is_empty()
                    || !link.libraries.is_empty()
                    || !link.runtime_library_dirs.is_empty()
                {
                    return Err(
                        "pkg-config external C metadata must obtain library paths and names from pkg-config"
                            .into(),
                    );
                }
            }
            (Some(_), Some(_)) => {
                return Err(
                    "external C metadata must choose exactly one of root_env or pkg_config".into(),
                );
            }
            (None, None) => {
                return Err(
                    "external C metadata requires exactly one of root_env or pkg_config".into(),
                );
            }
        }
        validate_relative_metadata_path(&metadata.header)?;
        for path in link
            .include_dirs
            .iter()
            .chain(&link.library_dirs)
            .chain(&link.runtime_library_dirs)
        {
            validate_relative_metadata_path(path)?;
        }
        for library in &link.libraries {
            if !is_link_library_name(library) {
                return Err(format!("external C library name `{library}` is invalid"));
            }
        }
        for source in &metadata.sources {
            validate_input_path(input_dir, source)?;
        }
        for header in &metadata.headers {
            validate_input_path(input_dir, header)?;
        }
        return Ok(());
    }

    validate_input_path(input_dir, &metadata.header)?;
    if metadata.sources.is_empty() {
        return Err("structured C metadata must declare sources or external_link".into());
    }
    for source in &metadata.sources {
        validate_input_path(input_dir, source)?;
    }
    for header in &metadata.headers {
        validate_input_path(input_dir, header)?;
    }
    Ok(())
}

pub(super) fn validate_rust_extension(
    package: &CAbiBindingPackage,
    input_dir: &Path,
) -> Result<(), String> {
    let Some(extension) = &package.rust_extension else {
        return Ok(());
    };
    validate_input_path(input_dir, &extension.source)?;
    if Path::new(&extension.source)
        .extension()
        .and_then(|value| value.to_str())
        != Some("rs")
    {
        return Err("C ABI package rust_extension source must be a Rust source file".into());
    }
    let mut output_names = BTreeSet::new();
    for source in &extension.support_sources {
        validate_input_path(input_dir, source)?;
        let path = Path::new(source);
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            return Err(format!(
                "C ABI package rust_extension support source `{source}` must be a Rust source file"
            ));
        }
        let name = file_name(source)?;
        if name == "package_extension.rs" || !output_names.insert(name.clone()) {
            return Err(format!(
                "C ABI package rust_extension support source `{source}` has conflicting output name `{name}`"
            ));
        }
    }
    for (name, dependency) in &extension.dependencies {
        validate_cargo_package_name(name)?;
        let (version, features) = match dependency {
            CAbiRustDependency::Version(version) => (version, &[][..]),
            CAbiRustDependency::Detailed {
                version, features, ..
            } => (version, features.as_slice()),
        };
        if !is_pinned_cargo_version(version) {
            return Err(format!(
                "C ABI package Rust dependency `{name}` must use an exact stable x.y.z version; found `{version}`"
            ));
        }
        if features
            .iter()
            .any(|feature| feature.is_empty() || feature.chars().any(char::is_whitespace))
        {
            return Err(format!(
                "C ABI package Rust dependency `{name}` contains an invalid Cargo feature"
            ));
        }
    }
    Ok(())
}

/// Validates package-contained Terlan extension sources and module ownership.
pub(super) fn validate_terlan_module_extensions(
    manifest: &CAbiBindingManifest,
    input_dir: &Path,
) -> Result<(), String> {
    let modules = manifest
        .modules
        .iter()
        .map(|module| module.module.as_str())
        .collect::<BTreeSet<_>>();
    let mut sources = BTreeSet::new();
    for (module, source) in &manifest.package.terlan_module_extensions {
        if !modules.contains(module.as_str()) {
            return Err(format!(
                "C ABI package Terlan extension references unknown module `{module}`"
            ));
        }
        validate_input_path(input_dir, source)?;
        if Path::new(source)
            .extension()
            .and_then(|value| value.to_str())
            != Some("terl")
        {
            return Err(format!(
                "C ABI package Terlan extension source `{source}` must be a Terlan source file"
            ));
        }
        if !sources.insert(source.as_str()) {
            return Err(format!(
                "C ABI package Terlan extension source `{source}` is assigned to multiple modules"
            ));
        }
    }
    Ok(())
}

/// Appends one package-authored declaration fragment to its generated module.
pub(super) fn append_terlan_module_extension(
    module_source: &mut String,
    package: &CAbiBindingPackage,
    module: &str,
    input_dir: &Path,
) -> Result<(), String> {
    let Some(extension) = package.terlan_module_extensions.get(module) else {
        return Ok(());
    };
    let extension_source = fs::read_to_string(input_dir.join(extension)).map_err(|error| {
        format!("failed to read Terlan module extension `{extension}` for `{module}`: {error}")
    })?;
    module_source.push('\n');
    module_source.push_str(&extension_source);
    if !module_source.ends_with('\n') {
        module_source.push('\n');
    }
    Ok(())
}

pub(super) fn validate_c_aliases(metadata: &CMetadata) -> Result<(), String> {
    for (name, target) in &metadata.aliases {
        if !is_c_identifier(name) {
            return Err(format!("C alias name `{name}` is invalid"));
        }
        let resolved = resolve_c_type(target, &metadata.aliases)?;
        let base = c_pointer_base(&resolved);
        if !is_builtin_c_type(base) && !is_c_identifier(base) {
            return Err(format!(
                "C alias `{name}` resolves to unsupported type `{target}`"
            ));
        }
    }
    Ok(())
}

pub(super) fn copy_c_inputs(
    metadata: &CMetadata,
    input_dir: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let package_header = input_dir.join(&metadata.header);
    if metadata.external_link.is_none() || package_header.is_file() {
        let header_name = file_name(&metadata.header)?;
        copy_file(
            &package_header,
            &out_dir.join("native/rust/include").join(header_name),
        )?;
    }
    for header in &metadata.headers {
        copy_file(
            &input_dir.join(header),
            &out_dir.join("native/rust/c").join(file_name(header)?),
        )?;
    }
    for source in &metadata.sources {
        copy_file(
            &input_dir.join(source),
            &out_dir.join("native/rust/c").join(file_name(source)?),
        )?;
    }
    Ok(())
}

pub(super) fn copy_rust_extension(
    package: &CAbiBindingPackage,
    input_dir: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let Some(extension) = &package.rust_extension else {
        return Ok(());
    };
    copy_file(
        &input_dir.join(&extension.source),
        &out_dir.join("native/rust/src/package_extension.rs"),
    )?;
    for source in &extension.support_sources {
        copy_file(
            &input_dir.join(source),
            &out_dir.join("native/rust/src").join(file_name(source)?),
        )?;
    }
    Ok(())
}
