use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::commands::artifacts::collect_syntax_asset_imports;
use crate::commands::emit_js::target_contract::{
    js_declaration_artifact_relative_path, js_metadata_relative_path,
    js_module_artifact_relative_path, js_target_contract, JsTargetContract, JS_DIAGNOSTICS_FILE,
    JS_TARGET_PROFILE_FILE,
};
use crate::commands::emit_js::{
    emit_core_module_to_typescript_declarations, emit_core_module_with_direct_oxc_ast,
    validate_js_module_with_oxc,
};
use crate::formal_pipeline::CheckedSyntaxModuleArtifacts;
use crate::support::read_file;
use crate::validation::target_profile::{TargetProfile, TargetProfileCheckOptions};
use crate::CliState;

use super::js_assets::browser_static_assets_from_manifest;
use super::js_browser::{
    write_browser_package_with_route_sources, BrowserStaticAssetConfig, WebRouteSourceArtifact,
};
pub(super) use super::js_model::JsModuleArtifact;
use super::js_model::{
    JsBuildManifest, JsDeclarationArtifact, JsDiagnosticsMetadata, JsTargetProfileMetadata,
};
use super::js_source_classification::{
    should_skip_browser_backend_source, web_route_source_artifact_from_file,
};
use super::source_roots::{prepare_source_root_interfaces, SourceRootBuildUnit};
use super::{
    project_manifest_path, reject_unsupported_external_dependencies,
    reject_unsupported_target_std_source, resolve_project_build_roots, write_build_file, BuildArgs,
    BuildOneError, BuildTimings, ProjectSourceRoot,
};

/// Runs the JavaScript build path.
///
/// Inputs:
/// - `args`: parsed build command arguments.
/// - `state`: global CLI state used for diagnostics, output directory, cache,
///   native policy, and incremental writes.
/// - `profile`: normalized JavaScript target profile selected by `--target`.
///
/// Output:
/// - CLI exit code representing build success or failure.
///
/// Transformation:
/// - Dispatches source files and source directories through the existing
///   formal compiler path, then writes JS modules and JS target metadata under
///   the 0.0.4 `_build/js` artifact contract.
pub(super) fn run_js_build(args: &BuildArgs, state: &CliState, profile: TargetProfile) -> ExitCode {
    let mut js_state = state.clone();
    js_state.target_profile = profile;
    let Some(contract) = js_target_contract(profile) else {
        eprintln!(
            "internal build error: `{}` is not a JS target",
            profile.as_str()
        );
        return ExitCode::from(1);
    };

    let source_path = Path::new(&args.path);
    if source_path.is_dir() {
        return run_js_directory_build(source_path, &js_state, contract, args.declarations);
    }

    match build_one_js_source_artifact(&args.path, &js_state, contract, args.declarations) {
        Ok(Some(artifact)) => write_js_manifest_or_exit(
            &js_build_root(&js_state),
            contract,
            vec![artifact],
            Vec::new(),
            js_state.incremental,
            None,
        ),
        Ok(None) => ExitCode::SUCCESS,
        Err(err) => err.into_exit_code(),
    }
}

/// Runs the JavaScript build path for a source directory or project root.
///
/// Inputs:
/// - `dir`: source directory or project directory.
/// - `state`: JS-profile-adjusted CLI state.
/// - `contract`: selected JS artifact contract.
/// - `emit_declarations`: whether `.d.ts` artifacts were requested.
///
/// Output:
/// - CLI exit code representing directory build success or failure.
///
/// Transformation:
/// - Reuses project manifest detection and local path dependency resolution,
///   then delegates to the shared JS source-root build path.
fn run_js_directory_build(
    dir: &Path,
    state: &CliState,
    contract: JsTargetContract,
    emit_declarations: bool,
) -> ExitCode {
    let manifest_path = project_manifest_path(dir);
    if manifest_path.exists() {
        let manifest = match super::project_manifest::read_project_manifest(&manifest_path) {
            Ok(manifest) => manifest,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        if let Err(message) = reject_unsupported_external_dependencies(&manifest) {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
        let browser_static_assets = match manifest
            .web_assets
            .as_ref()
            .map(|assets| browser_static_assets_from_manifest(dir, assets))
            .transpose()
        {
            Ok(assets) => assets,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        let roots = match resolve_project_build_roots(dir, &manifest) {
            Ok(roots) => roots,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        return run_js_project_source_roots_build(
            &roots.source_roots,
            state,
            contract,
            emit_declarations,
            browser_static_assets,
        );
    }

    run_js_plain_source_roots_build(&[dir.to_path_buf()], state, contract, emit_declarations)
}

/// Runs the JavaScript build path for plain source roots.
///
/// Inputs:
/// - `source_roots`: source roots to scan for Terlan files.
/// - `state`: JS-profile-adjusted CLI state.
/// - `contract`: selected JS artifact contract.
/// - `emit_declarations`: whether `.d.ts` artifacts were requested.
///
/// Output:
/// - CLI exit code representing source-root build success or failure.
///
/// Transformation:
/// - Wraps plain source roots as build units, then delegates to the shared JS
///   source-root build path.
fn run_js_plain_source_roots_build(
    source_roots: &[PathBuf],
    state: &CliState,
    contract: JsTargetContract,
    emit_declarations: bool,
) -> ExitCode {
    let roots = source_roots
        .iter()
        .map(|path| SourceRootBuildUnit {
            path: path.clone(),
            package_path: None,
        })
        .collect::<Vec<_>>();
    run_js_source_roots_build(&roots, state, contract, emit_declarations, None)
}

/// Runs the JavaScript build path for manifest-backed project roots.
///
/// Inputs:
/// - `source_roots`: manifest source roots with package namespace metadata.
/// - `state`: JS-profile-adjusted CLI state.
/// - `contract`: selected JS artifact contract.
/// - `emit_declarations`: whether `.d.ts` artifacts were requested.
///
/// Output:
/// - CLI exit code representing source-root build success or failure.
///
/// Transformation:
/// - Converts manifest roots into build units that preserve package-root
///   validation, then delegates to the shared JS source-root build path.
fn run_js_project_source_roots_build(
    source_roots: &[ProjectSourceRoot],
    state: &CliState,
    contract: JsTargetContract,
    emit_declarations: bool,
    browser_static_assets: Option<BrowserStaticAssetConfig>,
) -> ExitCode {
    let roots = source_roots
        .iter()
        .map(|root| SourceRootBuildUnit {
            path: root.path.clone(),
            package_path: Some(root.package_path.clone()),
        })
        .collect::<Vec<_>>();
    run_js_source_roots_build(
        &roots,
        state,
        contract,
        emit_declarations,
        browser_static_assets,
    )
}

/// Runs JavaScript build emission for one or more source roots.
///
/// Inputs:
/// - `source_roots`: roots to scan and validate.
/// - `state`: JS-profile-adjusted CLI state.
/// - `contract`: selected JS artifact contract.
/// - `emit_declarations`: whether `.d.ts` artifacts were requested.
///
/// Output:
/// - CLI exit code representing build success or failure.
///
/// Transformation:
/// - Discovers `.terl` files, validates each root with the formal check path,
///   compiles every source file to CoreIR, emits deterministic JS modules, and
///   writes one JS target manifest.
fn run_js_source_roots_build(
    source_roots: &[SourceRootBuildUnit],
    state: &CliState,
    contract: JsTargetContract,
    emit_declarations: bool,
    browser_static_assets: Option<BrowserStaticAssetConfig>,
) -> ExitCode {
    let mut timings = BuildTimings::new(state.timings);
    let mut files = Vec::new();
    for root in source_roots {
        let root_files = match crate::formal_pipeline::terlan_sources_in_dir(&root.path) {
            Ok(root_files) => root_files,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        if root_files.is_empty() {
            eprintln!(
                "terlc build found no .terl files in {}",
                root.path.display()
            );
            return ExitCode::from(1);
        }
        if let Some(package_path) = root.package_path.as_deref() {
            for file in &root_files {
                if let Err(message) =
                    super::validate_project_source_package_root(&root.path, file, package_path)
                {
                    eprintln!("{}", message);
                    return ExitCode::from(1);
                }
            }
        }
        files.extend(root_files);
    }
    timings.mark("js.scan");

    let mut directory_state = state.clone();
    if directory_state.cache_dir.is_none() {
        directory_state.cache_dir = Some(js_build_root(state).join(".terlan"));
    }

    if directory_state.incremental {
        for root in source_roots {
            if let Err(message) = prepare_source_root_interfaces(&root.path, &directory_state) {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        }
        timings.mark("js.interface-prepass");
    } else {
        let check_status = run_full_js_source_root_checks(source_roots, &directory_state);
        if check_status != ExitCode::SUCCESS {
            return check_status;
        }
        timings.mark("js.full-check");
    }

    let (artifacts, route_sources) = match build_js_source_artifacts(
        &files,
        &directory_state,
        contract,
        emit_declarations,
        browser_static_assets.is_some(),
    ) {
        Ok(artifacts) => artifacts,
        Err(err) => {
            if !directory_state.incremental {
                return err.into_exit_code();
            }
            let check_status = run_full_js_source_root_checks(source_roots, &directory_state);
            if check_status != ExitCode::SUCCESS {
                return check_status;
            }
            match build_js_source_artifacts(
                &files,
                &directory_state,
                contract,
                emit_declarations,
                browser_static_assets.is_some(),
            ) {
                Ok(artifacts) => artifacts,
                Err(err) => return err.into_exit_code(),
            }
        }
    };
    timings.mark("js.compile");

    if state.no_emit {
        return ExitCode::SUCCESS;
    }

    write_js_manifest_or_exit(
        &js_build_root(state),
        contract,
        artifacts,
        route_sources,
        state.incremental,
        browser_static_assets.as_ref(),
    )
}

/// Runs formal source-root checks before JavaScript artifact generation.
///
/// Inputs:
/// - `source_roots`: source roots selected for the JS build.
/// - `state`: CLI state reused by the check command.
///
/// Output:
/// - Success when all roots pass, or the first failing check exit code.
///
/// Transformation:
/// - Reuses `terlc check` so JS builds do not bypass syntax/type validation.
fn run_full_js_source_root_checks(
    source_roots: &[SourceRootBuildUnit],
    state: &CliState,
) -> ExitCode {
    for root in source_roots {
        let check_status = crate::commands::check::run_check_dir(
            &root.path.to_string_lossy(),
            state.clone(),
            None,
        );
        if check_status != ExitCode::SUCCESS {
            return check_status;
        }
    }
    ExitCode::SUCCESS
}

/// Builds JavaScript artifacts and route-source metadata from Terlan files.
///
/// Inputs:
/// - `files`: source files selected for the JS target.
/// - `state`: CLI state with output/cache paths.
/// - `contract`: target profile and backend contract.
/// - `emit_declarations`: whether `.d.ts` artifacts should be emitted.
/// - `has_browser_static_assets`: whether the package has browser assets.
///
/// Output:
/// - JavaScript module artifacts plus route-source artifacts.
///
/// Transformation:
/// - Splits server route metadata from browser-emittable modules and compiles
///   only the files appropriate for the selected JS profile.
fn build_js_source_artifacts(
    files: &[PathBuf],
    state: &CliState,
    contract: JsTargetContract,
    emit_declarations: bool,
    has_browser_static_assets: bool,
) -> Result<(Vec<JsModuleArtifact>, Vec<WebRouteSourceArtifact>), BuildOneError> {
    let mut artifacts = Vec::new();
    let mut route_sources = Vec::new();
    for file in files {
        match web_route_source_artifact_from_file(file) {
            Ok(Some(route_source)) => {
                route_sources.push(route_source);
                continue;
            }
            Ok(None) => {}
            Err(message) => return Err(BuildOneError::Message(message)),
        }
        match should_skip_browser_backend_source(file, contract.profile, has_browser_static_assets)
        {
            Ok(true) => continue,
            Ok(false) => {}
            Err(message) => return Err(BuildOneError::Message(message)),
        }
        match build_one_js_source_artifact(
            &file.to_string_lossy(),
            state,
            contract,
            emit_declarations,
        ) {
            Ok(Some(artifact)) => artifacts.push(artifact),
            Ok(None) => {}
            Err(BuildOneError::Message(message))
                if has_browser_static_assets && message.contains("error[js_emit_unsupported]") =>
            {
                continue;
            }
            Err(err) => return Err(err),
        }
    }
    Ok((artifacts, route_sources))
}

/// Builds one Terlan source file into one JavaScript artifact.
///
/// Inputs:
/// - `path`: Terlan source path.
/// - `state`: JS-profile-adjusted CLI state.
/// - `contract`: selected JS artifact contract.
/// - `emit_declarations`: whether `.d.ts` artifacts were requested.
///
/// Output:
/// - `Ok(Some(JsModuleArtifact))` when a JS module was emitted.
/// - `Ok(None)` when `--no-emit` suppresses output.
/// - `Err(BuildOneError)` for source, compile, emit, or write failures.
///
/// Transformation:
/// - Reads source, runs formal compilation with the JS target profile, emits
///   JavaScript through the Oxc-backed JS backend, and writes the module under
///   the target contract's module directory.
fn build_one_js_source_artifact(
    path: &str,
    state: &CliState,
    contract: JsTargetContract,
    emit_declarations: bool,
) -> Result<Option<JsModuleArtifact>, BuildOneError> {
    let source = read_file(path).map_err(BuildOneError::Message)?;
    let target_profile_options = js_target_profile_check_options(state.target_profile);
    if let Err(message) = reject_unsupported_target_std_source(
        path,
        &source,
        state.target_profile,
        target_profile_options,
    ) {
        return Err(BuildOneError::Message(message));
    }

    let compiled =
        match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            state.target_profile,
            target_profile_options,
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return Err(BuildOneError::Exit(exit_code)),
        };

    if state.no_emit {
        return Ok(None);
    }

    write_js_module_artifact(path, &compiled, state, contract, emit_declarations)
        .map(Some)
        .map_err(BuildOneError::Message)
}

/// Builds target-profile validation options for JavaScript commands.
///
/// Inputs:
/// - `profile`: selected JavaScript target profile.
///
/// Output:
/// - Target-profile validation switches for the formal compile path.
///
/// Transformation:
/// - Allows asset imports only for `js.browser`, where this command resolves
///   and packages assets into `_build/web`; shared library JS keeps default
///   strict validation.
fn js_target_profile_check_options(profile: TargetProfile) -> TargetProfileCheckOptions {
    TargetProfileCheckOptions {
        allow_asset_imports: profile == TargetProfile::JsBrowser,
        allow_rust_backed_std_modules: false,
    }
}

/// Writes one compiled CoreIR module as JavaScript.
///
/// Inputs:
/// - `source_path`: original Terlan source path.
/// - `compiled`: checked formal pipeline artifacts.
/// - `state`: JS-profile-adjusted CLI state.
/// - `contract`: selected JS artifact contract.
/// - `emit_declarations`: whether `.d.ts` artifacts were requested.
///
/// Output:
/// - `Ok(JsModuleArtifact)` with manifest metadata.
/// - `Err(String)` for JS emission or artifact writes.
///
/// Transformation:
/// - Emits JS through the direct CoreIR-to-Oxc-AST backend, converts module
///   identity into the JS artifact path, validates through Oxc, writes the
///   file, runs the optional runtime smoke command, optionally writes a `.d.ts`
///   declaration artifact, and returns manifest metadata. Unsupported
///   direct-backend shapes fail before any artifact path is written.
fn write_js_module_artifact(
    source_path: &str,
    compiled: &CheckedSyntaxModuleArtifacts,
    state: &CliState,
    contract: JsTargetContract,
    emit_declarations: bool,
) -> Result<JsModuleArtifact, String> {
    let js = emit_release_js_module(source_path, compiled, contract)?;
    validate_release_js_module(source_path, compiled, contract, &js)?;
    let asset_imports =
        collect_syntax_asset_imports(&compiled.syntax_output, Path::new(source_path))?;
    let js_root = js_build_root(state);
    let relative_path = js_module_artifact_relative_path(&compiled.core.module);
    let artifact_path = js_root.join(&relative_path);
    if let Some(parent) = artifact_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "cannot create JS module artifact directory {}: {err}",
                parent.display()
            )
        })?;
    }
    write_build_file(&artifact_path, js.as_bytes(), state.incremental)?;
    let runtime_smoke_status = run_js_runtime_smoke(&artifact_path)?;
    let declaration_artifact = write_js_declaration_artifact(compiled, state, emit_declarations)?;

    Ok(JsModuleArtifact {
        module: compiled.core.module.clone(),
        source_path: source_path.to_string(),
        artifact_path: super::path_to_manifest_string(&artifact_path),
        relative_path: super::path_to_manifest_string(&relative_path),
        core_ir_hash: super::fingerprint(compiled.core.contract_text().as_bytes()),
        target_profile: contract.profile_name.to_string(),
        validation_status: "ok".to_string(),
        runtime_smoke_status,
        declaration_path: declaration_artifact
            .as_ref()
            .map(|artifact| artifact.artifact_path.clone()),
        declaration_relative_path: declaration_artifact
            .as_ref()
            .map(|artifact| artifact.relative_path.clone()),
        asset_imports,
    })
}

/// Emits one release-owned JavaScript module.
///
/// Inputs:
/// - `source_path`: original Terlan source path used in diagnostics.
/// - `compiled`: checked formal pipeline artifacts with CoreIR.
/// - `contract`: selected JS artifact contract.
///
/// Output:
/// - `Ok(String)` containing direct Oxc-generated JavaScript.
/// - `Err(String)` with a stable unsupported-feature diagnostic.
///
/// Transformation:
/// - Calls only the direct CoreIR-to-Oxc-AST path for release builds. The
///   bootstrap/fallback JS text emitter remains available to lower-level
///   backend probes, but build artifacts must come from the direct Oxc path.
fn emit_release_js_module(
    source_path: &str,
    compiled: &CheckedSyntaxModuleArtifacts,
    contract: JsTargetContract,
) -> Result<String, String> {
    emit_core_module_with_direct_oxc_ast(&compiled.core).ok_or_else(|| {
        format!(
            "error[{}]: JavaScript target `{}` does not support every public body in module `{}` for gate J0.3; no JS artifact was written for {}",
            contract.unsupported_feature_code,
            contract.profile_name,
            compiled.core.module,
            source_path
        )
    })
}

/// Validates release JavaScript source through Oxc before artifact writes.
///
/// Inputs:
/// - `source_path`: original Terlan source path used in diagnostics.
/// - `compiled`: checked formal pipeline artifacts with CoreIR.
/// - `contract`: selected JS artifact contract.
/// - `js`: emitted JavaScript source to validate.
///
/// Output:
/// - `Ok(())` when Oxc accepts the emitted JavaScript module.
/// - `Err(String)` with a stable `js_validate` diagnostic when Oxc rejects it.
///
/// Transformation:
/// - Runs the backend-independent validation hook before filesystem writes so
///   invalid JS never becomes a release artifact.
fn validate_release_js_module(
    source_path: &str,
    compiled: &CheckedSyntaxModuleArtifacts,
    contract: JsTargetContract,
    js: &str,
) -> Result<(), String> {
    validate_js_module_with_oxc(js).map_err(|message| {
        format!(
            "error[js_validate_oxc]: Oxc rejected JavaScript for module `{}` target `{}` before artifact write for {}: {}",
            compiled.core.module, contract.profile_name, source_path, message
        )
    })
}

/// Runs the optional JavaScript runtime smoke command for one artifact.
///
/// Inputs:
/// - `artifact_path`: emitted `.js` module path.
///
/// Output:
/// - `Ok("passed")` when a local Node runtime accepts `node --check`.
/// - `Ok("skipped:node_unavailable")` when Node is not installed.
/// - `Err(String)` when Node is available but rejects the module.
///
/// Transformation:
/// - Uses Node only as a runtime syntax smoke check. Oxc validation remains the
///   mandatory correctness gate, so missing Node does not make builds fail.
fn run_js_runtime_smoke(artifact_path: &Path) -> Result<String, String> {
    match Command::new("node")
        .arg("--check")
        .arg(artifact_path)
        .output()
    {
        Ok(output) if output.status.success() => Ok("passed".to_string()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!(
                "error[js_validate_runtime]: JavaScript runtime smoke failed for {}: {}",
                artifact_path.display(),
                stderr.trim()
            ))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok("skipped:node_unavailable".to_string())
        }
        Err(err) => Err(format!(
            "error[js_validate_runtime]: cannot run JavaScript runtime smoke for {}: {}",
            artifact_path.display(),
            err
        )),
    }
}

/// Writes a TypeScript declaration artifact for one JS module when requested.
///
/// Inputs:
/// - `compiled`: checked formal pipeline artifacts with CoreIR.
/// - `state`: JS-profile-adjusted CLI state with output directory and
///   incremental-write behavior.
/// - `emit_declarations`: whether the user requested `.d.ts` output.
///
/// Output:
/// - `Ok(Some(JsDeclarationArtifact))` when a declaration file was written.
/// - `Ok(None)` when declaration output was not requested.
/// - `Err(String)` for declaration artifact write failures.
///
/// Transformation:
/// - Renders TypeScript declarations from CoreIR public type/function metadata
///   and writes them beside the emitted `.js` module using the same module path
///   mapping.
fn write_js_declaration_artifact(
    compiled: &CheckedSyntaxModuleArtifacts,
    state: &CliState,
    emit_declarations: bool,
) -> Result<Option<JsDeclarationArtifact>, String> {
    if !emit_declarations {
        return Ok(None);
    }

    let js_root = js_build_root(state);
    let relative_path = js_declaration_artifact_relative_path(&compiled.core.module);
    let artifact_path = js_root.join(&relative_path);
    if let Some(parent) = artifact_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "cannot create JS declaration artifact directory {}: {err}",
                parent.display()
            )
        })?;
    }

    let declarations = emit_core_module_to_typescript_declarations(&compiled.core);
    write_build_file(&artifact_path, declarations.as_bytes(), state.incremental)?;

    Ok(Some(JsDeclarationArtifact {
        artifact_path: super::path_to_manifest_string(&artifact_path),
        relative_path: super::path_to_manifest_string(&relative_path),
    }))
}

/// Writes the JavaScript build manifest and metadata files.
///
/// Inputs:
/// - `js_root`: root JS output directory.
/// - `contract`: selected JS artifact contract.
/// - `modules`: emitted module artifacts.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - CLI success or failure code.
///
/// Transformation:
/// - Serializes the manifest and profile/diagnostic metadata as deterministic
///   JSON files under the J0.1 artifact layout.
fn write_js_manifest_or_exit(
    js_root: &Path,
    contract: JsTargetContract,
    modules: Vec<JsModuleArtifact>,
    route_sources: Vec<WebRouteSourceArtifact>,
    incremental: bool,
    browser_static_assets: Option<&BrowserStaticAssetConfig>,
) -> ExitCode {
    match write_js_manifest_and_browser_package(
        js_root,
        contract,
        modules,
        route_sources,
        incremental,
        browser_static_assets,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{}", message);
            ExitCode::from(1)
        }
    }
}

/// Writes JavaScript target manifest and metadata files.
///
/// Inputs:
/// - `js_root`: root JS output directory.
/// - `contract`: selected JS artifact contract.
/// - `modules`: emitted module artifacts.
/// - `route_sources`: Terlan source modules used for web route discovery.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after manifest, metadata files, and any selected browser package
///   files exist.
/// - `Err(String)` for serialization or writes.
///
/// Transformation:
/// - Creates JSON output from typed metadata, writes it through the shared
///   build file writer, and creates a deterministic `_build/web/` artifact for
///   browser-profile builds.
fn write_js_manifest_and_browser_package(
    js_root: &Path,
    contract: JsTargetContract,
    modules: Vec<JsModuleArtifact>,
    route_sources: Vec<WebRouteSourceArtifact>,
    incremental: bool,
    browser_static_assets: Option<&BrowserStaticAssetConfig>,
) -> Result<(), String> {
    fs::create_dir_all(js_root).map_err(|err| {
        format!(
            "cannot create JS build directory {}: {err}",
            js_root.display()
        )
    })?;
    fs::create_dir_all(js_root.join(contract.metadata_dir)).map_err(|err| {
        format!(
            "cannot create JS metadata directory {}: {err}",
            js_root.join(contract.metadata_dir).display()
        )
    })?;
    let manifest = JsBuildManifest {
        schema: "terlan-js-build-v1",
        target_profile: contract.profile_name,
        module_format: contract.module_format,
        module_extension: contract.module_extension,
        modules: &modules,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("cannot serialize JS build manifest: {err}"))?;
    write_build_file(
        &js_root.join(contract.manifest_file),
        manifest_json.as_bytes(),
        incremental,
    )?;

    let profile_json = serde_json::to_string_pretty(&JsTargetProfileMetadata {
        target_profile: contract.profile_name,
        module_format: contract.module_format,
        module_extension: contract.module_extension,
        unsupported_feature_code: contract.unsupported_feature_code,
    })
    .map_err(|err| format!("cannot serialize JS target profile metadata: {err}"))?;
    write_build_file(
        &js_root.join(js_metadata_relative_path(JS_TARGET_PROFILE_FILE)),
        profile_json.as_bytes(),
        incremental,
    )?;

    let diagnostics_json = serde_json::to_string_pretty(&JsDiagnosticsMetadata {
        diagnostic_family: "js_emit",
        unsupported_feature_code: contract.unsupported_feature_code,
        diagnostics: Vec::<String>::new(),
    })
    .map_err(|err| format!("cannot serialize JS diagnostics metadata: {err}"))?;
    write_build_file(
        &js_root.join(js_metadata_relative_path(JS_DIAGNOSTICS_FILE)),
        diagnostics_json.as_bytes(),
        incremental,
    )?;

    if contract.profile == TargetProfile::JsBrowser {
        write_browser_package_with_route_sources(
            js_root,
            contract,
            &modules,
            &route_sources,
            browser_static_assets,
            incremental,
        )?;
    }

    Ok(())
}

/// Returns the JS build output root for the current CLI state.
///
/// Inputs:
/// - `state`: CLI state with the selected output directory.
///
/// Output:
/// - Path to the JavaScript build root.
///
/// Transformation:
/// - Appends the contract's `js` subdirectory to the existing build output root
///   so default builds land in `_build/js`.
fn js_build_root(state: &CliState) -> PathBuf {
    state.out_dir.join("js")
}
