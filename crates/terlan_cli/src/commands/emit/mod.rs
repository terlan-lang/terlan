use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use terlan_erlang::{
    emit_html_runtime_to_erlang, try_emit_core_module_to_erlang_with_syntax_bridge,
    try_emit_syntax_struct_headers_to_hrl,
};

use crate::commands::artifacts::{
    collect_syntax_dependency_hashes, collect_syntax_file_import_bytes,
    collect_syntax_markdown_inputs, collect_syntax_template_inputs, fingerprint, read_manifest,
    DependencyManifest,
};
use crate::validation::native_policy::source_uses_native;
use crate::validation::phase_manifest::current_syntax_contract_identity;
use crate::validation::target_profile::TargetProfileCheckOptions;
use crate::{CliCommand, CliState};

/// Executes the `emit` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing exactly one Terlan source path.
/// - `state`: parsed global CLI state, including output/cache directories,
///   incremental-write mode, diagnostic format, no-emit mode, and native policy.
///
/// Output:
/// - `ExitCode::SUCCESS` when compilation and requested output writes complete.
/// - `ExitCode::from(2)` for malformed command arguments.
/// - `ExitCode::from(1)` for read, compile, native artifact, dependency input,
///   emit, directory, or write failures.
///
/// Transformation:
/// - Compiles one source module through the formal compiler phases, emits
///   Erlang/header/interface/dependency outputs, and writes optional native and
///   HTML runtime artifacts.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    if cmd.args.len() != 1 {
        eprintln!("missing or extra path argument");
        crate::print_usage();
        return ExitCode::from(2);
    }

    let path = &cmd.args[0];
    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let compiled =
        match crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            state.target_profile,
            TargetProfileCheckOptions {
                allow_asset_imports: true,
            },
        ) {
            Ok(compiled) => compiled,
            Err(exit_code) => return exit_code,
        };

    if state.no_emit {
        return ExitCode::SUCCESS;
    }
    if let Err(err) = fs::create_dir_all(&state.out_dir) {
        eprintln!("cannot create output directory: {}", err);
        return ExitCode::from(1);
    }
    if let Some(cache_dir) = &state.cache_dir {
        if let Err(err) = fs::create_dir_all(cache_dir) {
            eprintln!("cannot create cache directory: {}", err);
            return ExitCode::from(1);
        }
    }
    if source_uses_native(&source) {
        if let Err(message) = crate::commands::emit_native_metadata::emit_native_artifacts(
            &source,
            &state.out_dir,
            state.native_policy,
            state.incremental,
        ) {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    }

    let interface_text = compiled.core.interface.to_terlan_interface_text();
    let source_hash = fingerprint(source.as_bytes());
    let interface_hash = fingerprint(
        compiled
            .core
            .interface
            .to_terlan_interface_type_text()
            .as_bytes(),
    );
    let interface_doc_hash = fingerprint(
        compiled
            .core
            .interface
            .to_terlan_interface_doc_text()
            .as_bytes(),
    );
    let file_imports =
        match collect_syntax_file_import_bytes(&compiled.syntax_output, Path::new(path)) {
            Ok(file_imports) => file_imports,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
    let templates = match collect_syntax_template_inputs(&compiled.syntax_output, Path::new(path)) {
        Ok(templates) => templates,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let markdown_imports =
        match collect_syntax_markdown_inputs(&compiled.syntax_output, Path::new(path)) {
            Ok(markdown_imports) => markdown_imports,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
    let dependency_hashes = collect_syntax_dependency_hashes(
        &compiled.syntax_output,
        &compiled.interfaces,
        Some(Path::new(path)),
        Some(&file_imports),
    );
    let syntax_contract_identity = match current_syntax_contract_identity() {
        Ok(identity) => identity,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let manifest = DependencyManifest {
        module: compiled.syntax_output.module_name.clone(),
        syntax_contract_identity,
        source_hash,
        interface_hash,
        interface_doc_hash,
        dependencies: dependency_hashes,
    };
    let output_stem = crate::support::erlang_output_stem(&compiled.syntax_output.module_name);
    let _ = read_manifest(
        &state
            .out_dir
            .join(format!("{}.typi.deps", compiled.syntax_output.module_name)),
    )
    .map(|previous| manifest.should_recheck_dependents(&previous));

    let code = match try_emit_core_module_to_erlang_with_syntax_bridge(
        &compiled.core,
        &compiled.syntax_output,
        &compiled.interfaces.into_iter().collect::<BTreeMap<_, _>>(),
        &file_imports,
        &templates,
        &markdown_imports,
    ) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let target = state.out_dir.join(format!("{}.erl", output_stem));
    if let Err(err) =
        crate::support::write_if_changed_or_forced(&target, code.as_bytes(), state.incremental)
    {
        eprintln!("failed to write output: {}", err);
        return ExitCode::from(1);
    }

    if crate::commands::static_site::syntax_module_uses_html(&compiled.syntax_output) {
        let runtime_target = state.out_dir.join("typer_html.erl");
        if let Err(err) = crate::support::write_if_changed_or_forced(
            &runtime_target,
            emit_html_runtime_to_erlang().as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write html runtime: {}", err);
            return ExitCode::from(1);
        }
    }

    let hrl = match try_emit_syntax_struct_headers_to_hrl(&compiled.syntax_output) {
        Ok(hrl) => hrl,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    if !hrl.is_empty() {
        let hrl_target = state.out_dir.join(format!("{}.hrl", output_stem));
        if let Err(err) = crate::support::write_if_changed_or_forced(
            &hrl_target,
            hrl.as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write header output: {}", err);
            return ExitCode::from(1);
        }
    }

    let interface_file = format!("{}.typi", compiled.syntax_output.module_name);
    let deps_file = format!("{}.typi.deps", compiled.syntax_output.module_name);
    let mut interface_targets = vec![state.out_dir.join(&interface_file)];
    let mut deps_targets = vec![state.out_dir.join(&deps_file)];
    if let Some(cache_dir) = &state.cache_dir {
        if cache_dir != &state.out_dir {
            interface_targets.push(cache_dir.join(&interface_file));
            deps_targets.push(cache_dir.join(&deps_file));
        }
    }

    for target in interface_targets {
        if let Err(err) = crate::support::write_if_changed_or_forced(
            &target,
            interface_text.as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write interface output: {}", err);
            return ExitCode::from(1);
        }
    }
    for target in deps_targets {
        if let Err(err) = crate::support::write_if_changed_or_forced(
            &target,
            manifest.encode().as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write dependency manifest: {}", err);
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates a unique temporary directory for command tests.
    ///
    /// Inputs:
    /// - `name`: stable test label included in the directory name.
    ///
    /// Output:
    /// - Filesystem path that does not exist before the test uses it.
    ///
    /// Transformation:
    /// - Combines process id and current timestamp so parallel test execution
    ///   does not reuse output directories.
    fn temp_output_dir(name: &str) -> std::path::PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "terlan_emit_command_{name}_{}_{}",
            std::process::id(),
            now
        ))
    }

    /// Verifies `emit` auto-emits SafeNative artifacts for compiler-native std.
    ///
    /// Inputs:
    /// - Real `std/data/json.terl` source path.
    ///
    /// Output:
    /// - Exit-code and filesystem assertions.
    ///
    /// Transformation:
    /// - Runs the normal `emit` command and checks that compiler outputs,
    ///   derived SafeNative artifacts, and the stable BEAM not-loaded worker
    ///   reply surface are written together.
    #[test]
    fn run_emit_writes_compiler_native_std_artifacts() {
        let out_dir = temp_output_dir("std_json");
        let source_path = format!("{}/../../std/data/json.terl", env!("CARGO_MANIFEST_DIR"));
        let exit = run(
            CliCommand {
                verb: Some("emit".to_string()),
                args: vec![source_path],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..CliState::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(out_dir.join("std.data.Json.typi").exists());
        assert!(out_dir.join("std.data.Json.safe_native.json").exists());
        assert!(out_dir.join("std_data_json.erl").exists());
        let erl_loader_path = out_dir.join("std_data_json_safe_native.erl");
        assert!(erl_loader_path.exists());
        let erl_loader =
            fs::read_to_string(&erl_loader_path).expect("read generated safe native erl loader");
        assert!(erl_loader.contains("safe_native.not_loaded"));
        assert!(erl_loader.contains(
            "{safe_native_reply, RequestId, {error, safe_native_not_loaded_error()}, 0}"
        ));
        assert!(out_dir
            .join("std_data_json_safe_native.safe_native.rs")
            .exists());

        fs::remove_dir_all(out_dir).expect("remove emit command output");
    }
}
