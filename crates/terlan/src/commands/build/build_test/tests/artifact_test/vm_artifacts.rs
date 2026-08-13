use super::*;

use object::{Object, ObjectSymbol};

/// Asserts a JS runtime smoke status accepted by the J0.4 contract.
///
/// Inputs:
/// - `value`: manifest JSON value stored on one JS module artifact entry.
///
/// Output:
/// - Test assertion only; panics when the status is not a known runtime-smoke
///   result.
///
/// Transformation:
/// - Accepts successful runtime smoke when Node is available and explicit skip
///   status when the local runtime is unavailable.
pub(super) fn assert_runtime_smoke_status(value: &serde_json::Value) {
    let status = value.as_str().expect("runtime smoke status");
    assert!(
        status == "passed" || status == "skipped:node_unavailable",
        "unexpected runtime smoke status: {status}"
    );
}

pub(super) fn inspect_native_descriptor(
    path: &Path,
) -> crate::runtime::native_image::TvmExecutableDescriptor {
    let image = fs::read(path).expect("read native TVM image");
    let target = crate::runtime::native_image::host_tvm_target().expect("host TVM target");
    crate::runtime::native_image::inspect_tvm_image(&image, &target.triple)
        .expect("inspect native TVM image")
        .descriptor
}

/// Verifies single-file VM builds accept compiler-known atom aliases.
///
/// Inputs:
/// - A standalone Terlan source file with an `Atom["ready"]` alias and value.
///
/// Output:
/// - Test passes when the checked native image exposes the atom-valued export
///   without emitting Erlang source or BEAM bytecode.
///
/// Transformation:
/// - Runs normal VM build emission for an atom alias source and inspects the
///   admitted native descriptor.
#[test]
pub(super) fn build_command_builds_atom_alias_for_vm() {
    let dir = make_temp_dir("single_file_atom_manifest");
    let source_path = dir.join("atom_manifest_single_file.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module atom_manifest_single_file.\n\npub type Ready = Atom[\"ready\"].\n\npub value(): Ready ->\n    Atom[\"ready\"].\n",
    )
    .expect("failed to write atom manifest source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let image = out_dir.join("vm/atom_manifest_single_file.tvm");
    let descriptor = inspect_native_descriptor(&image);
    assert_eq!(descriptor.identity.module, "atom_manifest_single_file");
    assert_eq!(descriptor.exports.len(), 1);
    assert_eq!(
        descriptor.exports[0].name,
        "atom_manifest_single_file.value/0"
    );
    assert!(image.is_file(), "missing {}", image.display());
    assert!(
        !out_dir.join("src").exists(),
        "VM atom alias build must not emit Vm source"
    );
    assert!(
        !out_dir.join("ebin").exists(),
        "VM atom alias build must not emit VM bytecode"
    );
}

/// Verifies a project-owned script, not the package `Main`, roots direct script AOT builds.
#[test]
pub(super) fn build_command_roots_project_script_at_synthetic_main() {
    let dir = make_temp_dir("project_script_entry");
    let project = dir.join("script_host");
    let source_root = project.join("src/script_host");
    let scripts = project.join("scripts");
    let out_dir = project.join("_build");
    fs::create_dir_all(&source_root).expect("create source root");
    fs::create_dir_all(&scripts).expect("create scripts root");
    fs::write(
        project.join("terlan.toml"),
        "[package]\nname = \"script_host\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\n",
    )
    .expect("write manifest");
    fs::write(
        source_root.join("Main.terl"),
        "module script_host.Main.\n\npub main(): Unit -> Unit.\n",
    )
    .expect("write package main");
    let script = scripts.join("Smoke.terls");
    fs::write(
        &script,
        "answer = 40 + 2;\nassert_equal(answer, 42);\nanswer.\n",
    )
    .expect("write script");

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                script.display().to_string(),
                "--target".to_string(),
                "terlan-vm".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    let descriptor = inspect_native_descriptor(&out_dir.join("vm/scripts_Smoke.tvm"));
    assert_eq!(descriptor.identity.module, "scripts.Smoke");
    assert!(descriptor
        .exports
        .iter()
        .any(|export| export.name == "scripts.Smoke.main/0"));
    let main = descriptor
        .exports
        .iter()
        .find(|export| export.name == "scripts.Smoke.main/0")
        .expect("script main export");
    assert_eq!(
        main.results,
        vec![crate::runtime::native_image::TvmBoundaryType::Int]
    );
}

/// Verifies a project script remains executable when its source closure has a
/// test-named module but no conventional `Main.terl` module of its own.
#[test]
pub(super) fn build_command_roots_script_only_project_at_synthetic_main() {
    let dir = make_temp_dir("script_only_project_entry");
    let project = dir.join("script_host");
    let source_root = project.join("src/script_host");
    let scripts = project.join("scripts");
    let out_dir = project.join("_build");
    fs::create_dir_all(&source_root).expect("create source root");
    fs::create_dir_all(&scripts).expect("create scripts root");
    fs::write(
        project.join("terlan.toml"),
        "[package]\nname = \"script_host\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\n",
    )
    .expect("write manifest");
    fs::write(
        source_root.join("SelfTest.terl"),
        "module script_host.SelfTest.\n\npub answer(): Int -> 42.\n",
    )
    .expect("write package self-test module");
    let script = scripts.join("Smoke.terls");
    fs::write(
        &script,
        "answer = 40 + 2;\nassert_equal(answer, 42);\nanswer.\n",
    )
    .expect("write script");

    let status = run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                script.display().to_string(),
                "--target".to_string(),
                "terlan-vm".to_string(),
            ],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);
    let descriptor = inspect_native_descriptor(&out_dir.join("vm/scripts_Smoke.tvm"));
    assert_eq!(descriptor.identity.module, "scripts.Smoke");
    assert!(descriptor
        .exports
        .iter()
        .any(|export| export.name == "scripts.Smoke.main/0"));
}

/// Verifies single-file builds can emit a Terlan VM artifact without Vm.
///
/// Inputs:
/// - A standalone Terlan source file with one public pure function.
/// - An explicit `terlan-vm` build target and isolated output directory.
///
/// Output:
/// - Test passes when a descriptor-bearing native image is written and no
///   `.erl` or `.beam` artifact directories are created.
///
/// Transformation:
/// - Runs the formal compiler path through the VM artifact target, proving the
///   post-OTP runtime artifact can be emitted directly from CoreIR.
#[test]
pub(super) fn build_command_emits_terlan_vm_artifact_without_erlang_or_beam() {
    let dir = make_temp_dir("single_file_terlan_vm");
    let source_path = dir.join("vm_single_file.terl");
    let out_dir = dir.join("build");
    fs::create_dir_all(&out_dir).expect("create VM build directory");
    fs::write(
        out_dir.join(BUILD_PACKAGE_METADATA_FILE),
        r#"{"native":{"rust_dependencies":[{"rust":{"helper":"stale-helper"}}]}}"#,
    )
    .expect("write stale package metadata");
    fs::write(
        &source_path,
        "module vm_single_file.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n",
    )
    .expect("failed to write VM source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(
        !out_dir.join("src").exists(),
        "terlan-vm target must not emit Vm source"
    );
    assert!(
        !out_dir.join("ebin").exists(),
        "terlan-vm target must not emit VM bytecode"
    );

    let pure_path = out_dir.join("vm/vm_single_file.tvm");
    let descriptor = inspect_native_descriptor(&pure_path);
    assert_eq!(descriptor.identity.module, "vm_single_file");
    assert_eq!(descriptor.exports.len(), 1);
    assert_eq!(descriptor.exports[0].name, "vm_single_file.add/2");
    assert_eq!(
        descriptor.exports[0].parameters,
        vec![
            crate::runtime::native_image::TvmBoundaryType::Int,
            crate::runtime::native_image::TvmBoundaryType::Int
        ]
    );
    assert!(pure_path.is_file(), "missing {}", pure_path.display());
    let image_bytes = fs::read(&pure_path).expect("read native TVM image");
    let image = object::File::parse(&*image_bytes).expect("parse native TVM image");
    assert!(image.symbols().any(|symbol| {
        symbol.name().is_ok_and(|name| {
            name == "terlan_native_dispatch_v3" || name == "_terlan_native_dispatch_v3"
        })
    }));
    assert!(out_dir.join(".terlan/native-aot").is_dir());
    assert!(!out_dir.join("vm/native").exists());
    assert!(
        !out_dir.join(BUILD_PACKAGE_METADATA_FILE).exists(),
        "standalone VM builds must not retain package-native metadata from an earlier build"
    );
}

/// Verifies scalar if branches and unary operators move into the native image.
#[test]
pub(super) fn build_command_lowers_if_and_unary_expressions_into_native_ir() {
    let dir = make_temp_dir("vm_if_unary_artifact");
    let source_path = dir.join("vm_if_unary.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module vm_if_unary.\n\nabs(n: Int): Int ->\n    if {\n        n < 0 -> -n;\n        true -> n\n    }.\n\npub main(): Int ->\n    abs(-7).\n",
    )
    .expect("write VM if/unary source fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let descriptor = inspect_native_descriptor(&out_dir.join("vm/vm_if_unary.tvm"));
    assert_eq!(descriptor.exports.len(), 1);
    assert_eq!(descriptor.exports[0].name, "vm_if_unary.main/0");
}

/// Proves typed constants are substituted before direct native-AOT lowering.
#[test]
pub(super) fn value_lifecycle_constants_lower_into_native_aot_without_runtime_storage() {
    let dir = make_temp_dir("vm_typed_constant_artifact");
    let source_path = dir.join("vm_typed_constant.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module vm_typed_constant.\n\npub const ANSWER: Int = 40 + 2.\n\npub answer(): Int -> ANSWER.\n",
    )
    .expect("write typed constant fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let descriptor = inspect_native_descriptor(&out_dir.join("vm/vm_typed_constant.tvm"));
    assert_eq!(descriptor.exports.len(), 1);
    assert_eq!(descriptor.exports[0].name, "vm_typed_constant.answer/0");
}

/// Verifies bare single-file builds now emit Terlan VM artifacts by default.
///
/// Inputs:
/// - A standalone Terlan source file with one public pure function.
/// - No explicit `--target` argument.
///
/// Output:
/// - Test passes when the default build writes a native `.tvm` image and no Vm
///   or VM output directories.
///
/// Transformation:
/// - Runs the user-facing `terlc build <file.terl>` path to lock the 0.0.7
///   post-OTP default product lane.
#[test]
pub(super) fn build_command_defaults_to_terlan_vm_artifact_without_erlang_or_beam() {
    let dir = make_temp_dir("single_file_default_terlan_vm");
    let source_path = dir.join("build_default_vm.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module build_default_vm.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n",
    )
    .expect("failed to write default VM source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_path.display().to_string()],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(
        !out_dir.join("src").exists(),
        "default build target must not emit Vm source"
    );
    assert!(
        !out_dir.join("ebin").exists(),
        "default build target must not emit VM bytecode"
    );

    let descriptor = inspect_native_descriptor(&out_dir.join("vm/build_default_vm.tvm"));
    assert_eq!(descriptor.identity.module, "build_default_vm");
    assert_eq!(descriptor.exports[0].name, "build_default_vm.add/2");
}

/// Verifies manifest-backed project builds default to Terlan VM artifacts.
///
/// Inputs:
/// - A project directory with `terlan.toml` and one source under the declared
///   package root.
/// - No explicit `--target` argument.
///
/// Output:
/// - Test passes when the project build writes a native `.tvm` executable
///   package and no Vm or VM output directories.
///
/// Transformation:
/// - Runs `terlc build <project-dir>` through the default path so ordinary
///   package builds remain VM-first after the post-OTP pivot.
#[test]
pub(super) fn build_command_defaults_project_directory_to_terlan_vm_artifacts() {
    let dir = make_temp_dir("project_default_terlan_vm");
    let project_dir = dir.join("app");
    let src_dir = project_dir.join("src/app");
    let out_dir = project_dir.join("_build");
    fs::create_dir_all(&src_dir).expect("create project source directory");
    fs::write(
        project_dir.join("terlan.toml"),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
    )
    .expect("write project manifest");
    fs::write(
        src_dir.join("Main.terl"),
        "module app.Main.\n\npub main(): Int ->\n    40 + 2.\n",
    )
    .expect("write project source");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![project_dir.display().to_string()],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(
        !out_dir.join("src").exists(),
        "default project build target must not emit Vm source"
    );
    assert!(
        !out_dir.join("ebin").exists(),
        "default project build target must not emit VM bytecode"
    );

    let descriptor = inspect_native_descriptor(&out_dir.join("vm/app_Main.tvm"));
    assert_eq!(descriptor.identity.module, "app.Main");
    assert_eq!(descriptor.exports[0].name, "app.Main.main/0");
    assert!(out_dir.join("bin/app").is_file());
}

/// Verifies single-file JavaScript builds emit JS modules and a manifest.
///
/// Inputs:
/// - A standalone Terlan source file with one public arithmetic function.
/// - An explicit JavaScript build target and isolated output directory.
///
/// Output:
/// - Test passes when a `.js` module, target metadata, diagnostics metadata,
///   and JS build manifest are written without Vm artifacts.
///
/// Transformation:
/// - Runs the build command through `--target js`, then inspects the J0.1
///   `_build/js`-style layout under the selected test output directory.
#[test]
pub(super) fn build_command_emits_js_module_and_manifest_for_single_file() {
    let dir = make_temp_dir("single_file_js");
    let source_path = dir.join("build_single_file_js.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module build_single_file_js.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_root = out_dir.join("js");
    let js_module = js_root.join("modules/build_single_file_js.js");
    assert!(js_module.exists(), "expected JS module at {js_module:?}");
    assert!(
        !out_dir.join("src/build_single_file_js.erl").exists(),
        "JS build should not emit Vm source"
    );
    assert!(
        !out_dir.join("ebin/build_single_file_js.beam").exists(),
        "JS build should not emit VM bytecode"
    );

    let js_text = fs::read_to_string(&js_module).expect("read JS module");
    assert!(js_text.contains("export function add(x, y)"));
    assert!(js_text.contains("return x + y;"));

    let manifest_text =
        fs::read_to_string(js_root.join("manifest.json")).expect("read JS manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse JS manifest");
    assert_eq!(manifest["schema"], "terlan-js-build-v1");
    assert_eq!(manifest["target_profile"], "js.shared");
    assert_eq!(manifest["module_format"], "es-module");
    assert_eq!(manifest["module_extension"], "js");
    assert_eq!(manifest["modules"].as_array().expect("modules").len(), 1);
    assert_eq!(manifest["modules"][0]["module"], "build_single_file_js");
    assert_eq!(
        manifest["modules"][0]["relative_path"],
        "modules/build_single_file_js.js"
    );
    assert_runtime_smoke_status(&manifest["modules"][0]["runtime_smoke_status"]);

    let profile_text = fs::read_to_string(js_root.join("metadata/target-profile.json"))
        .expect("read JS target metadata");
    let profile: serde_json::Value =
        serde_json::from_str(&profile_text).expect("parse JS target metadata");
    assert_eq!(profile["target_profile"], "js.shared");

    let diagnostics_text = fs::read_to_string(js_root.join("metadata/diagnostics.json"))
        .expect("read JS diagnostics metadata");
    let diagnostics: serde_json::Value =
        serde_json::from_str(&diagnostics_text).expect("parse JS diagnostics metadata");
    assert_eq!(diagnostics["diagnostic_family"], "js_emit");
    assert_eq!(
        diagnostics["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .len(),
        0
    );
}

/// Verifies JavaScript builds lower selected portable `std.core.String`
/// intrinsics directly into JS operations.
///
/// Inputs:
/// - A standalone Terlan source file using `String` receiver methods selected
///   for J0.7.
/// - An explicit JavaScript build target and isolated output directory.
///
/// Output:
/// - Test passes when the written JS module contains direct JavaScript string
///   operations and the manifest records the artifact.
///
/// Transformation:
/// - Runs the real `terlc build --target js` command so target validation,
///   direct Oxc emission, artifact writing, and manifest generation are all
///   exercised through the release-facing build path.
#[test]
pub(super) fn build_command_emits_js_std_core_string_intrinsics() {
    let dir = make_temp_dir("single_file_js_string_intrinsics");
    let source_path = dir.join("build_single_file_js_string_intrinsics.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "\
module build_single_file_js_string_intrinsics.

pub clean(): String ->
    \"  hello  \".trim().

pub loud(): String ->
    \"hello\".uppercase().

pub has_suffix(): Bool ->
    \"hello\".ends_with(\"lo\").
",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_root = out_dir.join("js");
    let js_module = js_root.join("modules/build_single_file_js_string_intrinsics.js");
    let js_text = fs::read_to_string(&js_module).expect("read JS module");
    assert!(
        js_text.contains(r#"return "  hello  ".trim();"#),
        "{js_text}"
    );
    assert!(
        js_text.contains(r#"return "hello".toUpperCase();"#),
        "{js_text}"
    );
    assert!(
        js_text.contains(r#"return "hello".endsWith("lo");"#),
        "{js_text}"
    );

    let manifest_text =
        fs::read_to_string(js_root.join("manifest.json")).expect("read JS manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse JS manifest");
    assert_eq!(manifest["schema"], "terlan-js-build-v1");
    assert_eq!(
        manifest["modules"][0]["relative_path"],
        "modules/build_single_file_js_string_intrinsics.js"
    );
    assert_runtime_smoke_status(&manifest["modules"][0]["runtime_smoke_status"]);
}

/// Verifies JS builds emit TypeScript declarations when requested.
///
/// Inputs:
/// - A standalone Terlan source file with one public function.
/// - An explicit JavaScript build target, `--declarations`, and isolated
///   output directory.
///
/// Output:
/// - Test passes when `.js` and `.d.ts` artifacts are written side by side and
///   the JS manifest records the declaration path.
///
/// Transformation:
/// - Runs `terlc build --target js --declarations`, then verifies declaration
///   text is derived from CoreIR public function metadata.
#[test]
pub(super) fn build_command_emits_js_declarations_when_requested() {
    let dir = make_temp_dir("single_file_js_declarations");
    let source_path = dir.join("build_single_file_js_declarations.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module build_single_file_js_declarations.\n\npub add(x: Int, y: Int): Int ->\n    x + y.\n",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
            "--declarations".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_root = out_dir.join("js");
    let declaration_path = js_root.join("modules/build_single_file_js_declarations.d.ts");
    assert!(
        declaration_path.exists(),
        "expected TypeScript declaration at {declaration_path:?}"
    );
    let declaration_text =
        fs::read_to_string(&declaration_path).expect("read TypeScript declaration");
    assert!(
        declaration_text.contains("export function add(x: number, y: number): number;"),
        "{declaration_text}"
    );

    let manifest_text =
        fs::read_to_string(js_root.join("manifest.json")).expect("read JS manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse JS manifest");
    assert_eq!(
        manifest["modules"][0]["declaration_relative_path"],
        "modules/build_single_file_js_declarations.d.ts"
    );
    assert_eq!(
        manifest["modules"][0]["declaration_path"],
        declaration_path.to_string_lossy().to_string()
    );
}

/// Verifies JavaScript builds reject unsupported direct-backend shapes before write.
///
/// Inputs:
/// - A standalone Terlan source file with a public body that the release JS
///   backend currently cannot lower through direct Oxc AST emission.
/// - An explicit JavaScript build target and isolated output directory.
///
/// Output:
/// - Test passes when the build fails and no partial JS module or manifest is
///   written.
///
/// Transformation:
/// - Runs `terlc build --target js` through the normal command path and checks
///   J0.3's direct-backend artifact-write boundary.
#[test]
pub(super) fn build_command_rejects_unsupported_js_direct_backend_before_artifact_write() {
    let dir = make_temp_dir("single_file_js_direct_reject");
    let source_path = dir.join("build_js_direct_reject.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "\
module build_js_direct_reject.

pub choose(flag: Bool): Int ->
    if { flag -> 1 }.
",
    )
    .expect("failed to write source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_path.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    let js_root = out_dir.join("js");
    assert!(
        !js_root.join("modules/build_js_direct_reject.js").exists(),
        "unsupported JS bodies must fail before module artifact write"
    );
    assert!(
        !js_root.join("manifest.json").exists(),
        "unsupported JS bodies must fail before manifest write"
    );
}

/// Verifies directory JavaScript builds emit one JS module per source file.
///
/// Inputs:
/// - A source directory containing multiple package-rooted Terlan modules.
/// - An explicit JavaScript build target and isolated output directory.
///
/// Output:
/// - Test passes when all expected `.js` modules are emitted and listed in the
///   JS build manifest without Vm artifacts.
///
/// Transformation:
/// - Runs source-root discovery through `terlc build --target js`, then checks
///   that the J0.1 JS layout receives deterministic module artifacts.
#[test]
pub(super) fn build_command_emits_js_modules_and_manifest_for_directory() {
    let dir = make_temp_dir("directory_js");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
        source_dir.join("a_math.terl"),
        "module a_math.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write first source fixture");
    fs::write(
        source_dir.join("z_math.terl"),
        "module z_math.\n\npub add(x: Int): Int ->\n    x + 1.\n",
    )
    .expect("failed to write second source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "js".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let js_root = out_dir.join("js");
    assert!(js_root.join("modules/a_math.js").exists());
    assert!(js_root.join("modules/z_math.js").exists());
    assert!(
        !out_dir.join("src/a_math.erl").exists(),
        "JS directory builds should not emit Vm source"
    );
    assert!(
        !out_dir.join("ebin/z_math.beam").exists(),
        "JS directory builds should not emit VM bytecode"
    );

    let manifest_text =
        fs::read_to_string(js_root.join("manifest.json")).expect("read JS directory manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse JS directory manifest");
    assert_eq!(manifest["schema"], "terlan-js-build-v1");
    assert_eq!(manifest["target_profile"], "js.shared");
    let modules = manifest["modules"].as_array().expect("modules");
    let module_names = modules
        .iter()
        .map(|entry| entry["module"].as_str().expect("module name"))
        .collect::<Vec<_>>();
    assert_eq!(module_names, vec!["a_math", "z_math"]);
    for module in modules {
        assert_runtime_smoke_status(&module["runtime_smoke_status"]);
    }
}
