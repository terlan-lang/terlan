use std::collections::BTreeMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use terlan_erlang::try_emit_core_module_to_erlang_with_syntax_bridge;

use super::command_runner::{quote_erlang_atom, run_command_with_no_erl_crash_dump};
use super::manifest::{TestRunReport, TestRunResult, TestRunStatus};
use super::style::TestOutputStyle;
use super::DiscoveredTest;
use crate::commands::artifacts::{
    collect_syntax_file_import_bytes, collect_syntax_markdown_inputs,
    collect_syntax_template_inputs,
};
use crate::validation::native_policy::{source_uses_native, NativePolicy};

/// Temporary BEAM workspace owner.
pub(super) struct TempBeamWorkspace {
    path: PathBuf,
}

impl TempBeamWorkspace {
    /// Creates a temporary workspace for emitted Erlang and BEAM artifacts.
    ///
    /// Inputs:
    /// - `module_name`: Terlan module name used only for a readable path stem.
    ///
    /// Output:
    /// - `Ok(TempBeamWorkspace)` when the directory exists.
    /// - `Err(message)` when the directory cannot be created.
    ///
    /// Transformation:
    /// - Builds a unique path under the host temp directory using process id
    ///   and current nanoseconds, then creates it.
    pub(super) fn create(module_name: &str) -> Result<Self, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| format!("cannot create test workspace timestamp: {err}"))?
            .as_nanos();
        let stem = crate::support::erlang_output_stem(module_name);
        let path = std::env::temp_dir().join(format!(
            "terlan_test_{stem}_{}_{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&path)
            .map_err(|err| format!("cannot create test workspace {}: {err}", path.display()))?;
        Ok(Self { path })
    }

    /// Returns the temporary workspace path.
    ///
    /// Inputs:
    /// - `self`: live workspace owner.
    ///
    /// Output:
    /// - Borrowed path to the workspace directory.
    ///
    /// Transformation:
    /// - Exposes the path without transferring cleanup ownership.
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempBeamWorkspace {
    /// Removes the temporary workspace when the owner goes out of scope.
    ///
    /// Inputs:
    /// - `self`: workspace owner being dropped.
    ///
    /// Output:
    /// - No reported output; cleanup failures are ignored.
    ///
    /// Transformation:
    /// - Attempts recursive removal of the temporary directory.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Emits a compiled module into the temporary workspace and compiles it.
///
/// Inputs:
/// - `path`: source path used to resolve imports.
/// - `workspace`: output directory for `.erl` and `.beam` files.
/// - `compiled`: formal compiler artifacts for the module.
/// - `test_exports`: test-only functions to export in the temporary artifact.
///
/// Output:
/// - `Ok(())` when Erlang source is written and `erlc` succeeds.
/// - `Err(message)` for dependency collection, emission, write, or compile
///   failures.
///
/// Transformation:
/// - Converts CoreIR plus syntax bridge data into Erlang source, writes
///   it to the workspace with optional test-only exports, then runs
///   `erlc -o <workspace>`.
pub(super) fn emit_compiled_module_to_workspace(
    path: &str,
    workspace: &Path,
    compiled: &crate::formal_pipeline::CheckedSyntaxModuleArtifacts,
    test_exports: &[DiscoveredTest],
) -> Result<(), String> {
    let source = crate::support::read_file(path)?;
    emit_compiled_module_source_to_workspace(path, &source, workspace, compiled, test_exports)
}

/// Emits and compiles one checked module into a BEAM test workspace using
/// already-loaded source text.
///
/// Inputs:
/// - `path`: source path used to resolve relative asset imports.
/// - `source`: source text used for SafeNative stub discovery.
/// - `workspace`: temporary directory receiving generated Erlang and BEAM.
/// - `compiled`: checked compiler artifacts to emit.
/// - `test_exports`: discovered source-level tests to export.
///
/// Output:
/// - `Ok(())` after generated Erlang, native stubs, and BEAM artifacts compile.
/// - `Err(String)` for asset collection, emission, write, native stub, or
///   Erlang compiler failures.
///
/// Transformation:
/// - Separates support-source loading from emission so embedded release
///   support modules can generate SafeNative stubs without requiring their
///   original `.terl` files to exist on disk.
pub(super) fn emit_compiled_module_source_to_workspace(
    path: &str,
    source: &str,
    workspace: &Path,
    compiled: &crate::formal_pipeline::CheckedSyntaxModuleArtifacts,
    test_exports: &[DiscoveredTest],
) -> Result<(), String> {
    let file_imports = collect_syntax_file_import_bytes(&compiled.syntax_output, Path::new(path))?;
    let templates = collect_syntax_template_inputs(&compiled.syntax_output, Path::new(path))?;
    let markdown_imports =
        collect_syntax_markdown_inputs(&compiled.syntax_output, Path::new(path))?;
    let code = try_emit_core_module_to_erlang_with_syntax_bridge(
        &compiled.core,
        &compiled.syntax_output,
        &compiled
            .interfaces
            .iter()
            .map(|(name, interface)| (name.clone(), interface.clone()))
            .collect::<BTreeMap<_, _>>(),
        &file_imports,
        &templates,
        &markdown_imports,
    )?;
    let code = add_test_exports_to_erlang_source(code, test_exports);
    let output_stem = crate::support::erlang_output_stem(&compiled.syntax_output.module_name);
    let erl_path = workspace.join(format!("{output_stem}.erl"));
    fs::write(&erl_path, code)
        .map_err(|err| format!("failed to write {}: {err}", erl_path.display()))?;
    emit_and_compile_safe_native_stubs_from_source(source, workspace)?;
    compile_erlang_source(workspace, &erl_path)
}

/// Emits and compiles SafeNative stubs from source text.
///
/// Inputs:
/// - `source`: Terlan source text to scan for compiler-native declarations.
/// - `workspace`: temporary BEAM workspace receiving generated files.
///
/// Output:
/// - `Ok(())` when no stubs are needed or every generated stub compiles.
/// - `Err(String)` for metadata emission or Erlang compile failures.
///
/// Transformation:
/// - Reuses the compiler-native metadata emitter without forcing callers that
///   already own embedded source text to re-open a path from disk.
fn emit_and_compile_safe_native_stubs_from_source(
    source: &str,
    workspace: &Path,
) -> Result<(), String> {
    if !source_uses_native(&source) {
        return Ok(());
    }

    crate::commands::emit_native_metadata::emit_native_artifacts(
        source,
        workspace,
        NativePolicy::SafeNativeOptional,
        false,
    )?;

    for entry in fs::read_dir(workspace).map_err(|err| {
        format!(
            "failed to read test workspace {}: {err}",
            workspace.display()
        )
    })? {
        let path = entry
            .map_err(|err| format!("failed to read test workspace entry: {err}"))?
            .path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.ends_with("_safe_native.erl") {
            compile_erlang_source(workspace, &path)?;
        }
    }

    Ok(())
}

/// Adds test-only exports to generated Erlang source.
///
/// Inputs:
/// - `code`: generated Erlang module source.
/// - `tests`: discovered source-level tests to export in the temporary test
///   artifact.
///
/// Output:
/// - Erlang source with an extra `-export([...]).` attribute when tests are
///   present.
///
/// Transformation:
/// - Inserts one deterministic export attribute after the first line of the
///   module source. This is used only by `terlc test` temporary artifacts so
///   private Terlan tests can execute without widening production emits.
pub(super) fn add_test_exports_to_erlang_source(code: String, tests: &[DiscoveredTest]) -> String {
    if tests.is_empty() {
        return code;
    }
    let exports = tests
        .iter()
        .map(|test| format!("{}/0", quote_erlang_atom(&test.name)))
        .collect::<Vec<_>>()
        .join(", ");
    let export_attr = format!("-export([{}]).\n", exports);
    if let Some(insert_at) = code.find('\n') {
        let mut with_exports = String::with_capacity(code.len() + export_attr.len());
        with_exports.push_str(&code[..=insert_at]);
        with_exports.push_str(&export_attr);
        with_exports.push_str(&code[insert_at + 1..]);
        with_exports
    } else {
        format!("{code}\n{export_attr}")
    }
}

/// Compiles one Erlang source file into BEAM bytecode.
///
/// Inputs:
/// - `workspace`: target directory for compiled `.beam` output.
/// - `erl_path`: path to the generated Erlang source file.
///
/// Output:
/// - `Ok(())` when `erlc` exits successfully.
/// - `Err(message)` when spawning or compilation fails.
///
/// Transformation:
/// - Executes `erlc -o <workspace> <erl_path>` with crash-dump suppression and
///   formats stderr/stdout into command diagnostics on failure.
fn compile_erlang_source(workspace: &Path, erl_path: &Path) -> Result<(), String> {
    let erl_crash_dump = workspace.join("erl_crash.dump");
    let file_name = erl_path
        .file_name()
        .ok_or_else(|| format!("erlc source path {} has no file name", erl_path.display()))?;
    let mut command = Command::new("erlc");
    command
        .current_dir(workspace)
        .arg("-o")
        .arg(".")
        .arg(file_name);
    let output = run_command_with_no_erl_crash_dump(&mut command, "erlc", Some(&erl_crash_dump))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "erlc failed for {}\n{}{}",
        erl_path.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Emits and compiles a backend-owned EUnit wrapper module.
///
/// Inputs:
/// - `workspace`: temporary directory for generated Erlang artifacts.
/// - `module_atom`: compiled Erlang module containing exported Terlan tests.
/// - `tests`: discovered source-level tests to expose as EUnit functions.
///
/// Output:
/// - `Ok(wrapper_module_atom)` when the wrapper source and `.beam` are ready.
/// - `Err(message)` when writing or compiling the wrapper fails.
///
/// Transformation:
/// - Generates an Erlang module with one `<test>_test/0` function per Terlan
///   test. Each function delegates to the emitted Terlan module and maps
///   `true` to `ok`, `false` to an EUnit failure, and other values to an
///   unexpected-result failure.
pub(super) fn emit_and_compile_eunit_wrapper(
    workspace: &Path,
    module_atom: &str,
    tests: &[DiscoveredTest],
) -> Result<String, String> {
    let wrapper_module_atom = format!("{module_atom}_eunit_tests");
    let source = render_eunit_wrapper_source(&wrapper_module_atom, module_atom, tests);
    let erl_path = workspace.join(format!("{wrapper_module_atom}.erl"));
    fs::write(&erl_path, source)
        .map_err(|err| format!("failed to write {}: {err}", erl_path.display()))?;
    compile_erlang_source(workspace, &erl_path)?;
    Ok(wrapper_module_atom)
}

/// Renders the backend-owned EUnit wrapper source.
///
/// Inputs:
/// - `wrapper_module_atom`: Erlang module atom for the generated wrapper.
/// - `target_module_atom`: Erlang module atom for the emitted Terlan module.
/// - `tests`: discovered source-level tests to expose as EUnit functions.
///
/// Output:
/// - Complete Erlang source for the wrapper module.
///
/// Transformation:
/// - Builds deterministic Erlang text without changing Terlan source syntax or
///   standard-library APIs.
pub(super) fn render_eunit_wrapper_source(
    wrapper_module_atom: &str,
    target_module_atom: &str,
    tests: &[DiscoveredTest],
) -> String {
    let exports = tests
        .iter()
        .map(|test| format!("{}/0", quote_erlang_atom(&format!("{}_test", test.name))))
        .collect::<Vec<_>>()
        .join(", ");
    let mut source = format!(
        "-module({}).\n-export([{}]).\n\n",
        quote_erlang_atom(wrapper_module_atom),
        exports
    );
    for test in tests {
        let wrapper_function = quote_erlang_atom(&format!("{}_test", test.name));
        source.push_str(&format!(
            "{}() ->\n    case {}:{}() of\n        true -> ok;\n        false -> erlang:error(assertion_returned_false);\n        Other -> erlang:error({{unexpected_test_result, Other}})\n    end.\n\n",
            wrapper_function,
            quote_erlang_atom(target_module_atom),
            quote_erlang_atom(&test.name)
        ));
    }
    source
}

/// Runs the generated EUnit wrapper without changing normal test output.
///
/// Inputs:
/// - `workspace`: BEAM code path containing the wrapper module.
/// - `wrapper_module_atom`: Erlang module atom for the generated wrapper.
///
/// Output:
/// - `Ok(())` when EUnit reports `ok`.
/// - `Err(message)` when EUnit reports failure or the Erlang runtime cannot be
///   spawned.
///
/// Transformation:
/// - Invokes `eunit:test/2` with `no_tty`, captures output, and converts the
///   EUnit result into a Terlan CLI backend-validation diagnostic.
pub(super) fn run_eunit_wrapper(workspace: &Path, wrapper_module_atom: &str) -> Result<(), String> {
    let eval = format!(
        "Result = eunit:test({}, [no_tty]), case Result of ok -> halt(0); Other -> io:format(\"eunit wrapper result: ~p~n\", [Other]), halt(1) end.",
        quote_erlang_atom(wrapper_module_atom)
    );
    let erl_crash_dump = workspace.join("erl_crash.dump");
    let safe_native_helper = ensure_safe_native_helper_wrapper(workspace)?;
    let mut command = Command::new("erl");
    command
        .arg("-noshell")
        .arg("-pa")
        .arg(workspace)
        .env(
            "TERLAN_SQL_RUNTIME_HELPER",
            std::env::current_exe()
                .map_err(|err| format!("failed to resolve current terlc: {err}"))?,
        )
        .env("TERLAN_SAFE_NATIVE_PATH", safe_native_helper)
        .arg("-eval")
        .arg(eval);
    let output = run_command_with_no_erl_crash_dump(&mut command, "erl", Some(&erl_crash_dump))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "EUnit wrapper validation failed for {}\n{}{}",
        wrapper_module_atom,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Executes all discovered tests against the compiled BEAM module.
///
/// Inputs:
/// - `workspace`: directory containing compiled `.beam` artifacts.
/// - `module_atom`: Erlang module atom to invoke.
/// - `tests`: discovered source-level test functions.
/// - `style`: terminal color policy for pass/fail labels.
///
/// Output:
/// - Structured execution report with pass/fail counts and per-test results.
///
/// Transformation:
/// - Calls each exported zero-argument test function via `erl -noshell`,
///   renders a compact status report, and aggregates pass/fail counts.
pub(super) fn run_discovered_erlang_tests(
    workspace: &Path,
    module_atom: &str,
    tests: &[DiscoveredTest],
    style: TestOutputStyle,
) -> TestRunReport {
    println!("running {} tests", tests.len());
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut results = Vec::new();
    for test in tests {
        match run_single_erlang_test(workspace, module_atom, &test.name) {
            Ok(()) => {
                passed += 1;
                println!("test {} ... {}", test.name, style.success("ok"));
                results.push(TestRunResult {
                    name: test.name.clone(),
                    status: TestRunStatus::Passed,
                    message: None,
                    span_start: test.span_start,
                    span_end: test.span_end,
                });
            }
            Err(message) => {
                failed += 1;
                println!("test {} ... {}", test.name, style.failure("FAILED"));
                println!("  {message}");
                results.push(TestRunResult {
                    name: test.name.clone(),
                    status: TestRunStatus::Failed,
                    message: Some(message),
                    span_start: test.span_start,
                    span_end: test.span_end,
                });
            }
        }
    }

    if failed == 0 {
        println!(
            "test result: {}. {passed} passed; 0 failed",
            style.success("ok")
        );
    } else {
        println!(
            "test result: {}. {passed} passed; {failed} failed",
            style.failure("FAILED")
        );
    }
    TestRunReport {
        passed,
        failed,
        results,
    }
}

/// Executes one exported BEAM test function.
///
/// Inputs:
/// - `workspace`: BEAM code path.
/// - `module_atom`: Erlang module atom to call.
/// - `function_atom`: Erlang function atom to call with arity 0.
///
/// Output:
/// - `Ok(())` when the function returns `true`.
/// - `Err(message)` when the function returns `false`, returns another value,
///   crashes, or the Erlang runtime cannot be spawned.
///
/// Transformation:
/// - Builds an Erlang `-eval` expression that maps `true` to exit code 0 and
///   all other outcomes to non-zero exit codes with diagnostic text.
fn run_single_erlang_test(
    workspace: &Path,
    module_atom: &str,
    function_atom: &str,
) -> Result<(), String> {
    let eval = format!(
        "Result = {}:{}(), case Result of true -> halt(0); false -> io:format(\"assertion returned false~n\", []), halt(1); Other -> io:format(\"unexpected test result: ~p~n\", [Other]), halt(2) end.",
        quote_erlang_atom(module_atom),
        quote_erlang_atom(function_atom)
    );
    let erl_crash_dump = workspace.join("erl_crash.dump");
    let safe_native_helper = ensure_safe_native_helper_wrapper(workspace)?;
    let mut command = Command::new("erl");
    command
        .arg("-noshell")
        .arg("-pa")
        .arg(workspace)
        .env(
            "TERLAN_SQL_RUNTIME_HELPER",
            std::env::current_exe()
                .map_err(|err| format!("failed to resolve current terlc: {err}"))?,
        )
        .env("TERLAN_SAFE_NATIVE_PATH", safe_native_helper)
        .arg("-eval")
        .arg(eval);
    let output = run_command_with_no_erl_crash_dump(&mut command, "erl", Some(&erl_crash_dump))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = format!("{}{}", stdout.trim_end(), stderr.trim_end());
    if message.is_empty() {
        Err(format!("erl exited with status {}", output.status))
    } else {
        Err(message)
    }
}

/// Creates an executable wrapper for the hidden SafeNative runtime.
///
/// Inputs:
/// - `workspace`: temporary BEAM workspace owned by one test run.
///
/// Output:
/// - Path to an executable script that launches `terlc __safe-native-runtime`.
///
/// Transformation:
/// - Bridges the generated Erlang stub contract, which accepts only an
///   executable path via `TERLAN_SAFE_NATIVE_PATH`, to the hidden `terlc`
///   subcommand that owns the generic SafeNative line protocol.
fn ensure_safe_native_helper_wrapper(workspace: &Path) -> Result<PathBuf, String> {
    let wrapper = workspace.join("terlan-safe-native-runtime");
    if wrapper.is_file() {
        return Ok(wrapper);
    }
    let terlc =
        std::env::current_exe().map_err(|err| format!("failed to resolve current terlc: {err}"))?;
    let terlc_text = terlc
        .to_str()
        .ok_or_else(|| format!("current terlc path is not UTF-8: {}", terlc.display()))?;
    let source = format!(
        "#!/bin/sh\nexec '{}' __safe-native-runtime\n",
        shell_quote(terlc_text)
    );
    fs::write(&wrapper, source)
        .map_err(|err| format!("failed to write {}: {err}", wrapper.display()))?;
    make_executable(&wrapper)?;
    Ok(wrapper)
}

#[cfg(unix)]
/// Marks a generated helper script executable on Unix.
///
/// Inputs: filesystem path to the generated script.
/// Output: success or a stable filesystem error string.
/// Transformation: sets mode `755` so Erlang can execute the wrapper.
fn make_executable(path: &Path) -> Result<(), String> {
    let mut permissions = fs::metadata(path)
        .map_err(|err| format!("failed to read {} permissions: {err}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|err| format!("failed to mark {} executable: {err}", path.display()))
}

#[cfg(not(unix))]
/// No-op executable marker for non-Unix platforms.
///
/// Inputs: generated wrapper path.
/// Output: success.
/// Transformation: keeps cross-platform call sites uniform where executable
/// bits are not managed through Unix permissions.
fn make_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Quotes text for a POSIX shell single-quoted string.
///
/// Inputs: unquoted shell argument text.
/// Output: shell-safe fragment.
/// Transformation: escapes embedded single quotes using the standard
/// close-quote/backslash/open-quote sequence.
fn shell_quote(value: &str) -> String {
    value.replace('\'', "'\\''")
}
