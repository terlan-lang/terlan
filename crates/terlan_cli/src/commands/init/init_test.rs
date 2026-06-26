use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates a unique temporary test directory.
///
/// Inputs:
/// - `name`: readable test stem.
///
/// Output:
/// - Path to a not-yet-existing directory under the system temp directory.
///
/// Transformation:
/// - Combines process id and current nanoseconds to avoid collisions.
fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("timestamp")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan_init_{name}_{}_{}",
        std::process::id(),
        nanos
    ))
}

#[test]
fn parse_init_args_accepts_named_project() {
    let parsed = parse_init_args(&["hello".to_string()]).expect("parse init args");

    assert_eq!(parsed.target_dir, PathBuf::from("hello"));
    assert_eq!(parsed.package_name, "hello");
    assert_eq!(parsed.profile, InitProfile::Default);
}

#[test]
fn parse_init_args_accepts_web_profile_before_project() {
    let parsed = parse_init_args(&[
        "--profile".to_string(),
        "web".to_string(),
        "hello-web".to_string(),
    ])
    .expect("parse init args");

    assert_eq!(parsed.target_dir, PathBuf::from("hello-web"));
    assert_eq!(parsed.package_name, "hello-web");
    assert_eq!(parsed.profile, InitProfile::Web);
}

#[test]
fn parse_init_args_accepts_web_profile_after_project() {
    let parsed = parse_init_args(&[
        "hello-web".to_string(),
        "--profile".to_string(),
        "web".to_string(),
    ])
    .expect("parse init args");

    assert_eq!(parsed.target_dir, PathBuf::from("hello-web"));
    assert_eq!(parsed.package_name, "hello-web");
    assert_eq!(parsed.profile, InitProfile::Web);
}

/// Verifies `terlc init --profile static` selects the static-site scaffold.
///
/// Inputs:
/// - Command-local init args with `--profile static`.
///
/// Output:
/// - Parsed init args with `InitProfile::Static`.
///
/// Transformation:
/// - Exercises profile parsing without touching the filesystem.
#[test]
fn parse_init_args_accepts_static_profile() {
    let parsed = parse_init_args(&[
        "--profile".to_string(),
        "static".to_string(),
        "docs-site".to_string(),
    ])
    .expect("parse init args");

    assert_eq!(parsed.target_dir, PathBuf::from("docs-site"));
    assert_eq!(parsed.package_name, "docs-site");
    assert_eq!(parsed.profile, InitProfile::Static);
}

#[test]
fn parse_init_args_rejects_missing_project_name() {
    let err = parse_init_args(&[]).expect_err("missing project name");

    assert!(err.contains("requires one new project name"));
}

#[test]
fn parse_init_args_rejects_invalid_package_name() {
    let err = parse_init_args(&["Hello".to_string()]).expect_err("invalid package");

    assert!(err.contains("must start with a lowercase ASCII letter"));
}

#[test]
fn parse_init_args_rejects_unknown_profile() {
    let err = parse_init_args(&[
        "--profile".to_string(),
        "desktop".to_string(),
        "hello".to_string(),
    ])
    .expect_err("unknown profile");

    assert!(err.contains("unsupported init profile `desktop`"));
}

#[test]
fn render_manifest_uses_release_project_contract() {
    assert_eq!(
            render_manifest("hello", InitProfile::Default),
            "[package]\nname = \"hello\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n"
        );
}

#[test]
fn render_manifest_web_profile_includes_asset_contract() {
    assert_eq!(
            render_manifest("hello-web", InitProfile::Web),
            "[package]\nname = \"hello-web\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n\n[web.assets]\ndirectory = \"assets\"\n"
        );
}

/// Verifies static projects reuse the supported web asset manifest section.
///
/// Inputs:
/// - Static profile manifest rendering request.
///
/// Output:
/// - Manifest text with `[web.assets] directory = "assets"`.
///
/// Transformation:
/// - Keeps static scaffolds compatible with the current project manifest parser
///   instead of introducing unsupported static-only TOML sections.
#[test]
fn render_manifest_static_profile_includes_asset_contract() {
    assert_eq!(
        render_manifest("docs-site", InitProfile::Static),
        "[package]\nname = \"docs-site\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"beam-thin\"\n\n[web.assets]\ndirectory = \"assets\"\n"
    );
}

#[test]
fn write_project_creates_manifest_and_main_module() {
    let dir = temp_dir("writes_project");
    let args = InitArgs {
        target_dir: dir.clone(),
        package_name: "hello".to_string(),
        profile: InitProfile::Default,
    };

    write_project(&args).expect("write project");

    assert_eq!(
        fs::read_to_string(dir.join("terlan.toml")).expect("manifest"),
        render_manifest("hello", InitProfile::Default)
    );
    assert_eq!(
        fs::read_to_string(dir.join(".gitignore")).expect("gitignore"),
        render_gitignore()
    );
    assert_eq!(
        fs::read_to_string(dir.join("Makefile")).expect("makefile"),
        render_makefile()
    );
    assert_eq!(
        fs::read_to_string(dir.join("src/hello/Main.terl")).expect("main module"),
        render_main_module("hello")
    );
    assert_eq!(
        fs::read_to_string(dir.join("tests/hello/MainTest.terl")).expect("test module"),
        render_test_module("hello")
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn write_project_normalizes_hyphenated_source_root() {
    let dir = temp_dir("hyphen_project");
    let args = InitArgs {
        target_dir: dir.clone(),
        package_name: "hello-app".to_string(),
        profile: InitProfile::Default,
    };

    write_project(&args).expect("write project");

    assert!(dir.join("src/hello_app/Main.terl").exists());
    assert!(dir.join("tests/hello_app/MainTest.terl").exists());
    assert!(fs::read_to_string(dir.join("src/hello_app/Main.terl"))
        .expect("main module")
        .contains("module hello_app.Main."));
    assert!(
        fs::read_to_string(dir.join("tests/hello_app/MainTest.terl"))
            .expect("test module")
            .contains("module hello_app.MainTest.")
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn write_project_web_profile_creates_browser_and_http_modules() {
    let dir = temp_dir("web_project");
    let args = InitArgs {
        target_dir: dir.clone(),
        package_name: "hello-web".to_string(),
        profile: InitProfile::Web,
    };

    write_project(&args).expect("write web project");

    assert_eq!(
        fs::read_to_string(dir.join("terlan.toml")).expect("manifest"),
        render_manifest("hello-web", InitProfile::Web)
    );
    assert!(dir.join("assets").is_dir());
    assert!(dir.join("templates").is_dir());
    assert_eq!(
        fs::read_to_string(dir.join("src/hello_web/Web.terl")).expect("web module"),
        render_web_module("hello_web")
    );
    assert_eq!(
        fs::read_to_string(dir.join("src/hello_web/Http.terl")).expect("http module"),
        render_http_handler_module("hello_web")
    );
    let http_source = fs::read_to_string(dir.join("src/hello_web/Http.terl")).expect("http module");
    assert!(http_source.contains("Page(title = \"Hello from Terlan\")"));
    assert!(http_source.contains("Response.html(page())"));
    assert!(http_source.contains("Router.new()"));
    assert!(http_source.contains(".get(\"/\", home)"));
    assert!(http_source.contains(".get(\"/users/:id\", show_user)"));
    assert!(http_source.contains(".fallback(not_found)"));
    assert!(!http_source.contains("Router.get(Router.new()"));
    assert!(!http_source.contains("let router ="));
    assert!(!http_source.contains("Dynamic"));
    assert!(!http_source.contains("terlan_response"));
    assert_eq!(
        fs::read_to_string(dir.join("templates/page.terl.html")).expect("page template"),
        render_web_page_template()
    );
    let compose = fs::read_to_string(dir.join("docker-compose.yml")).expect("docker compose");
    assert_eq!(compose, render_web_docker_compose());
    assert!(compose.contains("image: postgres:16-alpine"));
    assert!(compose.contains("\"127.0.0.1:5432:5432\""));
    assert!(compose.contains("pg_isready -U terlan -d terlan_dev"));
    crate::commands::serve::compose_check::validate_project_compose(&dir)
        .expect("generated web compose validates");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies the web scaffold's generated template declaration resolves.
///
/// Inputs:
/// - Web init args targeting an empty temporary directory.
///
/// Output:
/// - Parsed `Http.terl` syntax output with one successfully collected
///   template frontend input.
///
/// Transformation:
/// - Exercises the generated `../../templates/page.terl.html` path through the
///   same artifact collector used by command and validation paths.
#[test]
fn write_project_web_profile_template_declaration_resolves() {
    let dir = temp_dir("web_project_template_contract");
    let args = InitArgs {
        target_dir: dir.clone(),
        package_name: "hello-web".to_string(),
        profile: InitProfile::Web,
    };

    write_project(&args).expect("write web project");

    let http_path = dir.join("src/hello_web/Http.terl");
    let http_source = fs::read_to_string(&http_path).expect("read generated http module");
    let module =
        terlan_syntax::parse_module_as_syntax_output(&http_source).expect("parse http module");
    let collected =
        crate::commands::artifacts::collect_syntax_template_frontend_inputs(&module, &http_path);

    assert!(collected.errors.is_empty(), "{:?}", collected.errors);
    assert_eq!(collected.inputs.len(), 1);
    assert_eq!(collected.inputs[0].name, "Page");
    assert_eq!(collected.inputs[0].props[0].name, "title");
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies the generated web scaffold builds through the browser target.
///
/// Inputs:
/// - Web init args targeting an empty temporary project directory.
///
/// Output:
/// - Successful `terlc build --target js.browser` result and a web manifest
///   containing the scaffolded typed route contract.
///
/// Transformation:
/// - Runs the generated `Http.terl` through the formal build path so missing
///   std summaries or stale bridge-shaped handler signatures fail this test.
#[test]
fn write_project_web_profile_builds_js_browser_manifest() {
    let dir = temp_dir("web_project_build_contract");
    let args = InitArgs {
        target_dir: dir.clone(),
        package_name: "hello-web".to_string(),
        profile: InitProfile::Web,
    };

    write_project(&args).expect("write web project");

    let output_dir = dir.join("_build");
    let status = crate::commands::build::run(
        crate::CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                dir.display().to_string(),
                "--target".to_string(),
                "js.browser".to_string(),
            ],
        },
        crate::CliState {
            out_dir: output_dir.clone(),
            ..crate::CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);

    let manifest_path = output_dir.join("web/manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).expect("web manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_text).expect("parse web manifest");

    assert_eq!(manifest["target_profile"], "js.browser");
    assert!(manifest["handlers"]
        .as_array()
        .expect("handlers")
        .iter()
        .any(|handler| handler["route"] == "/" && handler["function"] == "home"));
    assert!(manifest["static_responses"]
        .as_array()
        .expect("static responses")
        .iter()
        .any(|response| response["route"] == "/users/:id" && response["body"] == "user route"));
    assert!(manifest["static_responses"]
        .as_array()
        .expect("static responses")
        .iter()
        .any(|response| response["route"] == "*" && response["body"] == "not found"));
    assert!(manifest["static_responses"]
        .as_array()
        .expect("static responses")
        .iter()
        .any(|response| response["method"] == "OPTIONS" && response["route"] == "*"));
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies static profile scaffolding writes source, content, and templates.
///
/// Inputs:
/// - Static init args targeting an empty temporary directory.
///
/// Output:
/// - Filesystem scaffold containing `Site.terl`, content, templates, assets,
///   manifest, and the ordinary test module.
///
/// Transformation:
/// - Exercises the release-owned static profile scaffold without invoking
///   build or serve commands.
#[test]
fn write_project_static_profile_creates_static_site_files() {
    let dir = temp_dir("static_project");
    let args = InitArgs {
        target_dir: dir.clone(),
        package_name: "docs-site".to_string(),
        profile: InitProfile::Static,
    };

    write_project(&args).expect("write static project");

    assert_eq!(
        fs::read_to_string(dir.join("terlan.toml")).expect("manifest"),
        render_manifest("docs-site", InitProfile::Static)
    );
    assert!(dir.join("assets").is_dir());
    assert_eq!(
        fs::read_to_string(dir.join("src/docs_site/Site.terl")).expect("site module"),
        render_static_site_module("docs_site")
    );
    assert_eq!(
        fs::read_to_string(dir.join("templates/layout.terl.html")).expect("layout template"),
        render_static_layout_template()
    );
    assert_eq!(
        fs::read_to_string(dir.join("content/index.terl.md")).expect("index content"),
        render_static_index_content()
    );
    assert!(dir.join("tests/docs_site/MainTest.terl").exists());
    fs::remove_dir_all(dir).expect("cleanup");
}

/// Verifies static profile scaffolding emits a validated static site.
///
/// Inputs:
/// - Static init args targeting an empty temporary project directory.
///
/// Output:
/// - Successful `terlc static emit` result and an emitted `index.html` page.
///
/// Transformation:
/// - Runs the generated `Site.terl`, imported Markdown content, and layout
///   template through the release static-site pipeline so scaffold drift fails
///   before users hit it locally.
#[test]
fn write_project_static_profile_emits_valid_static_site() {
    let dir = temp_dir("static_project_emit_contract");
    let args = InitArgs {
        target_dir: dir.clone(),
        package_name: "docs-site".to_string(),
        profile: InitProfile::Static,
    };

    write_project(&args).expect("write static project");

    let output_dir = dir.join("_build/web");
    let status = crate::commands::static_site::run(
        crate::CliCommand {
            verb: Some("static".to_string()),
            args: vec![
                "emit".to_string(),
                dir.join("src/docs_site/Site.terl").display().to_string(),
                "--validate-output".to_string(),
            ],
        },
        crate::CliState {
            out_dir: output_dir.clone(),
            ..crate::CliState::default()
        },
    );

    assert_eq!(status, ExitCode::SUCCESS);

    let index = fs::read_to_string(output_dir.join("index.html")).expect("generated index");
    assert!(index.contains("<h1>Home</h1>"));
    assert!(index.contains("<h1>Welcome</h1>"));
    assert!(index.contains("This page was generated by"));

    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn write_project_refuses_existing_directory() {
    let dir = temp_dir("refuse_directory");
    fs::create_dir_all(&dir).expect("create dir");
    let args = InitArgs {
        target_dir: dir.clone(),
        package_name: "hello".to_string(),
        profile: InitProfile::Default,
    };

    let err = write_project(&args).expect_err("should refuse overwrite");

    assert!(err.contains("refuses to write into existing directory"));
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn next_steps_for_web_profile_build_both_targets_and_serve() {
    assert_eq!(
        next_steps(InitProfile::Web, "hello-web"),
        vec![
            "make".to_string(),
            "terlc build --target js.browser".to_string(),
            "terlc serve".to_string(),
            "make test".to_string(),
        ]
    );
}

/// Verifies static profile next steps use the static emit workflow.
///
/// Inputs:
/// - Static profile and hyphenated package name.
///
/// Output:
/// - Commands pointing at the normalized source root and `_build/web`.
///
/// Transformation:
/// - Keeps generated user guidance aligned with the public `terlc static`
///   command group while the implementation delegates to the static runners.
#[test]
fn next_steps_for_static_profile_emit_and_serve_static_site() {
    assert_eq!(
        next_steps(InitProfile::Static, "docs-site"),
        vec![
            "terlc static emit src/docs_site/Site.terl --out-dir _build/web --validate-output"
                .to_string(),
            "terlc static serve src/docs_site/Site.terl --out-dir _build/web --validate-output"
                .to_string(),
            "make test".to_string(),
        ]
    );
}
