use super::*;
use crate::commands::build::package_artifact::tests::write_named_test_artifact;
use crate::commands::build::project_roots::resolve_project_build_roots;
use crate::commands::build::resolve_project_test_dependencies;
use crate::support::test_fs;

#[test]
fn package_fetch_arguments_accept_target_and_repeated_artifacts() {
    let parsed = parse_fetch_args(&[
        "fetch".to_string(),
        "project".to_string(),
        "--target".to_string(),
        "x86_64-unknown-linux-gnu".to_string(),
        "--artifact".to_string(),
        "one.tar.zst".to_string(),
        "--artifact".to_string(),
        "two.tar.zst".to_string(),
    ])
    .expect("parse package fetch arguments");

    assert_eq!(parsed.project_dir, PathBuf::from("project"));
    assert_eq!(parsed.target.as_deref(), Some("x86_64-unknown-linux-gnu"));
    assert_eq!(
        parsed.artifacts,
        vec![PathBuf::from("one.tar.zst"), PathBuf::from("two.tar.zst")]
    );
    assert!(parse_fetch_args(&["fetch".to_string(), "--target".to_string()]).is_err());
    assert!(
        parse_fetch_args(&["fetch".to_string(), "one".to_string(), "two".to_string()]).is_err()
    );
}

#[test]
fn package_fetch_locks_artifact_and_resolves_prebuilt_source_and_runtime() {
    let root = test_fs::temp_dir("package_git", "artifact_resolution");
    let repository = root.join("remote_utils");
    write_git_package(&repository, "remote_utils", "pub one(): Int ->\n    1.\n");
    let revision = commit_repository(&repository);
    let app = root.join("app");
    write_consumer(&app, &repository, &revision);
    let archive =
        write_named_test_artifact(&root, "x86_64-unknown-linux-gnu", "remote_utils", "0.0.1");

    let (packages, artifacts, _) = fetch_project_dependencies(&PackageFetchArgs {
        project_dir: app.clone(),
        target: Some("x86_64-unknown-linux-gnu".to_string()),
        artifacts: vec![archive],
    })
    .expect("fetch package source and artifact");

    assert_eq!((packages, artifacts), (1, 1));
    let lock = fs::read_to_string(app.join(LOCKFILE_NAME)).expect("read lockfile");
    assert!(lock.contains("[[artifact]]"));
    assert!(lock.contains("package = \"remote_utils\""));
    assert!(lock.contains("TERLAN_NATIVE_BOUNDARY_HELPER_PATH"));

    let manifest = project_manifest::read_project_manifest(&project_manifest_path(&app))
        .expect("read consumer manifest");
    let roots = resolve_project_build_roots(&app, &manifest).expect("resolve artifact dependency");
    assert_eq!(roots.native_artifact_environment.len(), 1);
    assert_eq!(
        roots.native_artifact_environment[0].0,
        "TERLAN_NATIVE_BOUNDARY_HELPER_PATH"
    );
    assert!(roots.native_artifact_environment[0].1.is_file());
    let dependency_root = roots
        .source_roots
        .iter()
        .find(|source| source.package_path == ["remote_utils"])
        .expect("artifact source root");
    assert!(dependency_root
        .path
        .to_string_lossy()
        .contains("/artifacts/"));
    let test_dependencies = resolve_project_test_dependencies(&app, &manifest)
        .expect("resolve artifact dependency for tests");
    assert_eq!(test_dependencies.native_helper_environment.len(), 1);
    assert_eq!(
        test_dependencies.native_helper_environment[0].0,
        "TERLAN_NATIVE_BOUNDARY_HELPER_PATH"
    );

    assert_eq!(
        fetch_project_git_dependencies(&app),
        Ok(1),
        "source-only refresh must preserve verified target artifacts"
    );
    let refreshed = read_lockfile(&app.join(LOCKFILE_NAME)).expect("read refreshed lockfile");
    assert_eq!(refreshed.artifacts.len(), 1);
}

#[test]
fn package_fetch_writes_lock_and_build_resolves_verified_cache_offline() {
    let root = test_fs::temp_dir("package_git", "fetch_and_resolve_offline");
    let repository = root.join("remote_utils");
    write_git_package(&repository, "remote_utils", "pub one(): Int ->\n    1.\n");
    let revision = commit_repository(&repository);
    let app = root.join("app");
    write_consumer(&app, &repository, &revision);

    assert_eq!(
        crate::run_cli(vec![
            "package".to_string(),
            "fetch".to_string(),
            app.display().to_string(),
        ]),
        ExitCode::SUCCESS
    );
    let lock = fs::read_to_string(app.join(LOCKFILE_NAME)).expect("read lockfile");
    assert!(lock.contains("resolver = \"terlan-0.0.7\""));
    assert!(lock.contains(&format!("rev = \"{revision}\"")));
    assert!(lock.contains("checksum = \"git-tree:"));

    fs::remove_dir_all(&repository).expect("remove source repository");
    assert_eq!(fetch_project_git_dependencies(&app), Ok(1));
    let manifest = project_manifest::read_project_manifest(&project_manifest_path(&app))
        .expect("read consumer manifest");
    let roots = resolve_project_build_roots(&app, &manifest).expect("resolve cached package");
    assert_eq!(roots.source_roots.len(), 2);
    assert!(roots
        .source_roots
        .iter()
        .any(|root| root.package_path == ["remote_utils"]));

    let out_dir = root.join("build");
    let status = crate::commands::build::run(
        CliCommand {
            verb: Some("build".to_string()),
            args: vec![
                app.display().to_string(),
                "--target".to_string(),
                "terlan-vm".to_string(),
            ],
        },
        crate::CliState {
            out_dir: out_dir.clone(),
            ..crate::CliState::default()
        },
    );
    assert_eq!(status, ExitCode::SUCCESS);
    let image_path = out_dir.join("vm/app_Main.tvm");
    assert!(image_path.is_file());
    assert!(!out_dir.join("vm/remote_utils_Util.tvm").exists());
    assert_eq!(
        native_image_export_names(&image_path),
        vec!["app.Main.value/0", "remote_utils.Util.one/0"]
    );
}

#[test]
fn build_rejects_git_dependency_missing_from_lockfile() {
    let root = test_fs::temp_dir("package_git", "missing_lockfile");
    let repository = root.join("remote_utils");
    write_git_package(&repository, "remote_utils", "pub one(): Int ->\n    1.\n");
    let revision = commit_repository(&repository);
    let app = root.join("app");
    write_consumer(&app, &repository, &revision);
    let manifest = project_manifest::read_project_manifest(&project_manifest_path(&app))
        .expect("read consumer manifest");

    let error = resolve_project_build_roots(&app, &manifest)
        .expect_err("build must not fetch an unlocked Git dependency");

    assert!(error.contains("error[package_git_not_locked]"));
    assert!(error.contains("terlc package fetch"));
    assert!(!app.join(".terlan/packages").exists());
}

#[test]
fn build_rejects_dirty_cached_git_checkout() {
    let root = test_fs::temp_dir("package_git", "dirty_cache");
    let repository = root.join("remote_utils");
    write_git_package(&repository, "remote_utils", "pub one(): Int ->\n    1.\n");
    let revision = commit_repository(&repository);
    let app = root.join("app");
    write_consumer(&app, &repository, &revision);
    fetch_project_git_dependencies(&app).expect("fetch dependency");
    fs::write(
        app.join(".terlan/packages/git")
            .join(&revision)
            .join("poisoned.txt"),
        "cache mutation",
    )
    .expect("poison cache");
    let manifest = project_manifest::read_project_manifest(&project_manifest_path(&app))
        .expect("read consumer manifest");

    let error =
        resolve_project_build_roots(&app, &manifest).expect_err("dirty cache must fail closed");

    assert!(error.contains("error[package_cache_dirty]"));
}

#[test]
fn package_fetch_rejects_revision_absent_from_repository() {
    let root = test_fs::temp_dir("package_git", "missing_revision");
    let repository = root.join("remote_utils");
    write_git_package(&repository, "remote_utils", "pub one(): Int ->\n    1.\n");
    commit_repository(&repository);
    let app = root.join("app");
    write_consumer(&app, &repository, &"a".repeat(40));

    let error = fetch_project_git_dependencies(&app)
        .expect_err("unknown immutable revision must be rejected");

    assert!(error.contains("error[package_git_revision_missing]"));
    assert!(!app.join(LOCKFILE_NAME).exists());
}

#[test]
fn package_fetch_and_build_follow_path_to_transitive_git_dependency() {
    let root = test_fs::temp_dir("package_git", "transitive_git");
    let repository = root.join("remote_utils");
    write_git_package(&repository, "remote_utils", "pub one(): Int ->\n    1.\n");
    let revision = commit_repository(&repository);
    let bridge = root.join("bridge");
    fs::create_dir_all(bridge.join("src/bridge")).expect("create bridge source");
    fs::write(
        bridge.join(TERLAN_PROJECT_MANIFEST_FILE),
        format!(
            "[package]\nname = \"bridge\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n\n[dependencies]\nremote_utils = {{ git = \"{}\", rev = \"{revision}\" }}\n",
            repository.display()
        ),
    )
    .expect("write bridge manifest");
    fs::write(
        bridge.join("src/bridge/Bridge.terl"),
        "module bridge.Bridge.\n\npub value(): Int ->\n    2.\n",
    )
    .expect("write bridge source");
    let app = root.join("app");
    fs::create_dir_all(app.join("src/app")).expect("create app source");
    fs::write(
        app.join(TERLAN_PROJECT_MANIFEST_FILE),
        "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n\n[dependencies]\nbridge = { path = \"../bridge\" }\n",
    )
    .expect("write app manifest");
    fs::write(
        app.join("src/app/Main.terl"),
        "module app.Main.\n\npub value(): Int ->\n    3.\n",
    )
    .expect("write app source");

    assert_eq!(fetch_project_git_dependencies(&app), Ok(1));
    let manifest = project_manifest::read_project_manifest(&project_manifest_path(&app))
        .expect("read app manifest");
    let roots = resolve_project_build_roots(&app, &manifest).expect("resolve transitive Git");

    assert_eq!(roots.source_roots.len(), 3);
    assert!(roots
        .source_roots
        .iter()
        .any(|root| root.package_path == ["remote_utils"]));
}

fn write_git_package(repository: &Path, package: &str, body: &str) {
    let source = repository.join("src").join(package);
    fs::create_dir_all(&source).expect("create package source");
    fs::write(
        repository.join(TERLAN_PROJECT_MANIFEST_FILE),
        format!(
            "[package]\nname = \"{package}\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n"
        ),
    )
    .expect("write package manifest");
    fs::write(
        source.join("Util.terl"),
        format!("module {package}.Util.\n\n{body}"),
    )
    .expect("write package source");
}

fn write_consumer(app: &Path, repository: &Path, revision: &str) {
    fs::create_dir_all(app.join("src/app")).expect("create app source");
    fs::write(
        app.join(TERLAN_PROJECT_MANIFEST_FILE),
        format!(
            "[package]\nname = \"app\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n\n[dependencies]\nremote_utils = {{ git = \"{}\", rev = \"{revision}\" }}\n",
            repository.display()
        ),
    )
    .expect("write consumer manifest");
    fs::write(
        app.join("src/app/Main.terl"),
        "module app.Main.\n\nimport remote_utils.Util.{one}.\n\npub value(): Int ->\n    one().\n",
    )
    .expect("write consumer source");
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

fn commit_repository(repository: &Path) -> String {
    run_git(repository, &["init", "--quiet"]);
    run_git(
        repository,
        &["config", "user.email", "terlan-tests@example.invalid"],
    );
    run_git(repository, &["config", "user.name", "Terlan Tests"]);
    run_git(repository, &["add", "."]);
    run_git(repository, &["commit", "--quiet", "-m", "fixture"]);
    git_output(repository, &["rev-parse", "HEAD"]).expect("resolve fixture revision")
}

fn run_git(repository: &Path, args: &[&str]) {
    git_status(repository, args).unwrap_or_else(|error| panic!("git fixture failed: {error}"));
}
