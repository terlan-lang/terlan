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
        fs::read_to_string(dir.join("src/hello/Main.terl")).expect("main module"),
        render_main_module("hello")
    );
    assert_eq!(
        fs::read_to_string(dir.join("tests/hello/main_test.terl")).expect("test module"),
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
    assert!(dir.join("tests/hello_app/main_test.terl").exists());
    assert!(fs::read_to_string(dir.join("src/hello_app/Main.terl"))
        .expect("main module")
        .contains("module hello_app.Main."));
    assert!(
        fs::read_to_string(dir.join("tests/hello_app/main_test.terl"))
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
    assert_eq!(
        fs::read_to_string(dir.join("src/hello_web/Web.terl")).expect("web module"),
        render_web_module("hello_web")
    );
    assert_eq!(
        fs::read_to_string(dir.join("src/hello_web/Http.terl")).expect("http module"),
        render_http_handler_module("hello_web")
    );
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
            "terlc build --target erlang".to_string(),
            "terlc build --target js.browser".to_string(),
            "terlc serve".to_string(),
        ]
    );
}
