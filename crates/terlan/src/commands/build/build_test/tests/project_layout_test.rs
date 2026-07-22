use super::*;

/// Verifies project manifests are rejected before silent source-root builds.
///
/// Inputs:
/// - A directory containing `terlan.toml` and one otherwise buildable
///   source module.
///
/// Output:
/// - Test passes when the VM build fails and emits no VM artifact, Vm
///   source, VM artifact, or build debug map.
///
/// Transformation:
/// - Runs the build command against a manifest-bearing directory and proves
///   A0.37 package/project manifest semantics are not silently skipped by
///   the plain recursive source-root build path.
#[test]
fn build_command_rejects_project_manifest_before_silent_directory_scan() {
    let dir = make_temp_dir("directory_project_manifest_rejected");
    let source_dir = dir.join("project");
    let out_dir = dir.join("build");
    fs::create_dir_all(&source_dir).expect("failed to create source dir");
    fs::write(
        source_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        source_dir.join("main.terl"),
        "module main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write manifest-bearing source fixture");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            source_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("vm/main.tvm").exists());
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
}

/// Verifies project manifests build from the parsed source root.
///
/// Inputs:
/// - A project root containing `terlan.toml`.
/// - A single manifest-declared `src` source root containing one nested
///   package-rooted module.
///
/// Output:
/// - Test passes when the VM build emits a VM artifact for the module under
///   the manifest source root without producing Vm or VM artifacts.
///
/// Transformation:
/// - Parses `terlan.toml`, resolves `[build] source_roots`, delegates the
///   selected source root to the VM artifact build path,
///   and proves the project root itself is not used as the module layout
///   root.
#[test]
fn build_command_compiles_project_manifest_source_root() {
    let dir = make_temp_dir("directory_project_manifest_source_root");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
        )
        .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Main.terl"),
        "module app.Main.\n\npub main(): Int ->\n    1 + 2.\n",
    )
    .expect("failed to write manifest source-root module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    let image_path = out_dir.join("vm/app_Main.tvm");
    assert_eq!(
        native_image_export_names(&image_path),
        vec!["app.Main.main/0"]
    );
}

/// Verifies manifest-backed VM builds resolve sibling source-root modules.
///
/// Inputs:
/// - A project manifest with `artifact = "library"`.
/// - `app.Auth` importing and calling `app.Account`.
/// - `app.Main` importing and calling `app.Auth`.
///
/// Output:
/// - Test passes when the import closure typechecks and its scalar leaf enters
///   the native application image without unresolved-module diagnostics.
///
/// Transformation:
/// - Runs the package source-root interface prepass before per-file VM
///   artifact emission, matching real applications whose modules depend on
///   siblings discovered from the same `terlan.toml` source root.
#[test]
fn build_command_compiles_project_manifest_sibling_module_imports() {
    let dir = make_temp_dir("directory_project_manifest_sibling_imports");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Account.terl"),
        "module app.Account.\n\npub label(): Int ->\n    1.\n",
    )
    .expect("failed to write account module");
    fs::write(
        app_dir.join("Auth.terl"),
        "module app.Auth.\n\nimport app.Account.\n\npub label(): Int ->\n    Account.label().\n",
    )
    .expect("failed to write auth module");
    fs::write(
        app_dir.join("Main.terl"),
        "module app.Main.\n\nimport app.Auth.\n\npub main(): Int ->\n    Auth.label().\n",
    )
    .expect("failed to write main module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    let image_path = out_dir.join("vm/app_Main.tvm");
    assert_eq!(
        native_image_export_names(&image_path),
        vec!["app.Account.label/0", "app.Auth.label/0", "app.Main.main/0"]
    );
    assert!(!out_dir.join("vm/app_Account.tvm").exists());
    assert!(!out_dir.join("vm/app_Auth.tvm").exists());
    assert!(out_dir.join(".terlan/app.Account.typi").exists());
    assert!(out_dir.join(".terlan/app.Auth.typi").exists());
    assert!(out_dir.join(".terlan/app.Main.typi").exists());
}

/// Verifies project builds support template-backed web handlers.
///
/// Inputs:
/// - A manifest-backed project with `src/app/Http.terl`.
/// - A `template Page from "../../templates/page.terl.html"` declaration.
/// - A public HTTP handler returning `Response.html(Page(title = ...))`.
///
/// Output:
/// - Test passes when the frontend emits the handler interface; its managed
///   values remain outside the scalar native ABI for now.
///
/// Transformation:
/// - Exercises external template loading plus generated template-call lowering
///   in an ordinary project build, matching the web-profile frontend shape.
#[test]
fn build_command_compiles_project_template_backed_http_handler() {
    let dir = make_temp_dir("directory_project_template_http_handler");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let template_dir = project_dir.join("templates");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::create_dir_all(&template_dir).expect("failed to create project templates dir");
    fs::write(
        project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n\n[web.assets]\ndirectory = \"assets\"\npublic_path = \"/assets\"\n",
    )
    .expect("failed to write project manifest fixture");
    fs::write(
        template_dir.join("page.terl.html"),
        "<main><h1>{title}</h1></main>",
    )
    .expect("failed to write page template fixture");
    fs::write(
        app_dir.join("Http.terl"),
        "module app.Http.\n\nimport std.http.Response.\nimport std.template.Template.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\ntemplate Page from \"../../templates/page.terl.html\" {\n    title: String\n}.\n\npub page(): Template.Html ->\n    Page(title = \"Terlan Cloud\").\n\npub dashboard(_request: Request): Response ->\n    Response.html(page()).\n",
    )
    .expect("failed to write template-backed http handler module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    assert!(out_dir.join(".terlan/app.Http.typi").exists());
}

/// Verifies manifest-backed library packages do not require an executable
/// entrypoint.
///
/// Inputs:
/// - A project manifest with `[build] artifact = "library"`.
/// - A package-rooted source module that does not define `Main.main`.
///
/// Output:
/// - Test passes when the VM build emits module artifacts without writing a
///   launcher.
///
/// Transformation:
/// - Parses the library artifact mode, validates the source root, lowers
///   the module, skips executable entrypoint validation, and records package
///   metadata with no executable entry.
#[test]
fn build_command_compiles_project_manifest_library_without_entrypoint() {
    let dir = make_temp_dir("directory_project_manifest_library");
    let project_dir = dir.join("project");
    let app_dir = project_dir.join("src/app");
    let out_dir = dir.join("build");
    fs::create_dir_all(&app_dir).expect("failed to create project src dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
        )
        .expect("failed to write project manifest fixture");
    fs::write(
        app_dir.join("Util.terl"),
        "module app.Util.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write library source module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    assert_eq!(
        native_image_export_names(&out_dir.join("vm/app_Util.tvm")),
        vec!["app.Util.value/0"]
    );
    assert!(!out_dir.join("bin/app").exists());
}

/// Verifies manifest package namespaces control source layout.
///
/// Inputs:
/// - A library package named `std-sample-polars` with namespace
///   `std.sample.polars`.
/// - A source file under `src/std/sample/polars`.
///
/// Output:
/// - Test passes when the build accepts the namespace path and emits the
///   namespaced VM artifact.
///
/// Transformation:
/// - Parses `[package] namespace`, validates source files against that
///   namespace path instead of the package-name-derived root, and preserves
///   the namespace in build metadata.
#[test]
fn build_command_compiles_project_manifest_namespace_layout() {
    let dir = make_temp_dir("directory_project_manifest_namespace_layout");
    let project_dir = dir.join("project");
    let module_dir = project_dir.join("src/std/sample/polars");
    let out_dir = dir.join("build");
    fs::create_dir_all(&module_dir).expect("failed to create namespace source dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"std-sample-polars\"\nversion = \"0.0.4\"\nnamespace = \"std.sample.polars\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n",
        )
        .expect("failed to write project manifest fixture");
    fs::write(
            module_dir.join("DataFrame.terl"),
            "module std.sample.polars.DataFrame.\n\npub opaque type DataFrame.\n\npub height(df: DataFrame): Int ->\n    0.\n\npub version(): Int ->\n    4.\n",
        )
        .expect("failed to write namespaced module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    assert_eq!(
        native_image_export_names(&out_dir.join("vm/std_sample_polars_DataFrame.tvm")),
        vec!["std.sample.polars.DataFrame.version/0"]
    );
    assert!(!out_dir.join("bin/std-sample-polars").exists());
}

/// Verifies manifest-backed builds reject source files outside the package root.
///
/// Inputs:
/// - A project manifest whose package name is `app`.
/// - A source file under `src/other` declaring `module other.Main`.
///
/// Output:
/// - Test passes when build fails before writing VM artifacts, Vm source,
///   VM artifacts, debug maps, package metadata, or executable launchers.
///
/// Transformation:
/// - Runs the project build path and proves manifest package identity is
///   enforced before the existing source-root layout and backend gates.
#[test]
fn build_command_rejects_project_source_outside_package_root() {
    let dir = make_temp_dir("directory_project_manifest_package_root_mismatch");
    let project_dir = dir.join("project");
    let other_dir = project_dir.join("src/other");
    let out_dir = dir.join("build");
    fs::create_dir_all(&other_dir).expect("failed to create project src dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"terlan-vm\"\n",
        )
        .expect("failed to write project manifest fixture");
    fs::write(
        other_dir.join("Main.terl"),
        "module other.Main.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("failed to write mismatched package-root module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::from(1));
    assert!(!out_dir.join("vm/other_Main.tvm").exists());
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    assert!(!out_dir.join(BUILD_DEBUG_MAP_FILE).exists());
    assert!(!out_dir.join(BUILD_PACKAGE_METADATA_FILE).exists());
    assert!(!out_dir.join("bin/app").exists());
}

/// Verifies project manifests build multiple declared source roots.
///
/// Inputs:
/// - A project root containing `terlan.toml`.
/// - Two manifest-declared source roots where the second imports a value
///   from the first.
///
/// Output:
/// - Test passes when the VM build typechecks both roots and emits their
///   independently native scalar leaf in one application image.
///
/// Transformation:
/// - Parses `terlan.toml`, resolves all `[build] source_roots`, validates
///   each root with a shared interface cache, lowers both roots through
///   CoreIR, and writes one source-to-artifact map across the project.
#[test]
fn build_command_accepts_project_manifest_multiple_source_roots_vm_import_closure() {
    let dir = make_temp_dir("directory_project_manifest_multiple_source_roots");
    let project_dir = dir.join("project");
    let lib_dir = project_dir.join("lib/demo");
    let app_dir = project_dir.join("app/demo");
    let out_dir = dir.join("build");
    fs::create_dir_all(&lib_dir).expect("failed to create project lib dir");
    fs::create_dir_all(&app_dir).expect("failed to create project app dir");
    fs::write(
            project_dir.join(TERLAN_PROJECT_MANIFEST_FILE),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"lib\", \"app\"]\nartifact = \"library\"\n",
        )
        .expect("failed to write multi-root project manifest fixture");
    fs::write(
        lib_dir.join("Util.terl"),
        "module demo.Util.\n\npub one(): Int ->\n    1.\n",
    )
    .expect("failed to write multi-root provider module");
    fs::write(
            app_dir.join("Main.terl"),
            "module demo.Main.\n\nimport demo.Util.{one}.\nimport std.io.Console.{println}.\n\npub main(): Unit ->\n    println(\"ok\");\n    Unit.\n\npub value(): Int ->\n    one().\n",
        )
        .expect("failed to write multi-root consumer module");

    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![
            project_dir.display().to_string(),
            "--target".to_string(),
            "terlan-vm".to_string(),
        ],
    };

    let status = run(cmd, state);

    assert_eq!(status, ExitCode::SUCCESS);
    assert!(!out_dir.join("src").exists());
    assert!(!out_dir.join("ebin").exists());
    assert_eq!(
        native_image_export_names(&out_dir.join("vm/demo_Main.tvm")),
        vec!["demo.Main.value/0", "demo.Util.one/0"]
    );
    assert!(!out_dir.join("vm/demo_Util.tvm").exists());
}

fn native_image_export_names(path: &Path) -> Vec<String> {
    let image = fs::read(path).expect("read native application image");
    let target = crate::runtime::native_image::host_tvm_target().expect("host TVM target");
    let mut names = crate::runtime::native_image::inspect_tvm_image(&image, &target.triple)
        .expect("inspect native application image")
        .descriptor
        .exports
        .into_iter()
        .map(|export| export.name)
        .collect::<Vec<_>>();
    names.sort();
    names
}
