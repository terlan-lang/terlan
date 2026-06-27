use super::*;

/// Verifies incremental directory checking refreshes dependency manifests only
/// when a public dependency interface changes.
///
/// Inputs:
/// - A temporary directory with a provider module, a dependent module, and an
///   unrelated module.
/// - Two provider edits: one body-only change and one public interface change.
///
/// Output:
/// - Test success when initial dependency manifests exist, body-only changes do
///   not refresh dependent/unrelated manifests, and public interface changes
///   refresh the dependent and provider manifests only.
///
/// Transformation:
/// - Runs `check` in incremental mode across baseline, private-body edit, and
///   public-interface edit states, then compares `.typi.deps` manifest text.
#[test]
fn run_check_dir_incremental_dependency_closure() {
    let dir = make_temp_dir("check_dir_incremental_dependency_closure");
    let cache = dir.join("cache");
    fs::write(
        dir.join("incr_lib.terl"),
        "module incr_lib.\n\npub add(X: Int): Int ->\n    X + 1.\n",
    )
    .expect("write lib");
    fs::write(
        dir.join("incr_user.terl"),
        "module incr_user.\n\nimport incr_lib.{add}.\n\npub compute(X: Int): Int ->\n    add(X).\n",
    )
    .expect("write user");
    fs::write(
        dir.join("incr_other.terl"),
        "module incr_other.\n\npub value(): Int ->\n    1.\n",
    )
    .expect("write unrelated");

    let state = CliState {
        incremental: true,
        cache_dir: Some(cache.clone()),
        ..Default::default()
    };
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into()],
        },
        state.clone(),
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let lib_manifest = cache.join("incr_lib.typi.deps");
    let user_manifest = cache.join("incr_user.typi.deps");
    let other_manifest = cache.join("incr_other.typi.deps");

    assert!(lib_manifest.exists());
    assert!(user_manifest.exists());
    assert!(other_manifest.exists());

    let baseline_user_manifest =
        fs::read_to_string(&user_manifest).expect("read baseline user manifest");
    let baseline_other_manifest =
        fs::read_to_string(&other_manifest).expect("read baseline other manifest");
    let baseline_lib_manifest =
        fs::read_to_string(&lib_manifest).expect("read baseline lib manifest");

    fs::write(
        dir.join("incr_lib.terl"),
        "module incr_lib.\n\npub add(X: Int): Int ->\n    X + 2.\n",
    )
    .expect("edit private-irrelevant lib body");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into()],
        },
        state.clone(),
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    assert_eq!(
        fs::read_to_string(&user_manifest).expect("read user manifest"),
        baseline_user_manifest,
        "user should not be rechecked when dependency interface is unchanged"
    );
    assert_eq!(
        fs::read_to_string(&other_manifest).expect("read other manifest"),
        baseline_other_manifest,
        "unrelated module should not be rechecked"
    );
    assert_ne!(
        fs::read_to_string(&lib_manifest).expect("read lib manifest"),
        baseline_lib_manifest,
        "changed dependency source should refresh its own manifest"
    );

    fs::write(
            dir.join("incr_lib.terl"),
            "module incr_lib.\n\npub add(X: Int): Int ->\n    X + 2.\n\npub neg(X: Int): Int ->\n    0 - X.\n",
        )
        .expect("edit public interface");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into()],
        },
        state,
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    assert_ne!(
        fs::read_to_string(&user_manifest).expect("read user manifest after interface change"),
        baseline_user_manifest,
        "user should be rechecked when dependency interface changes"
    );
    assert_ne!(
        fs::read_to_string(&lib_manifest).expect("read lib manifest after interface change"),
        baseline_lib_manifest,
        "changed dependency interface should refresh its own manifest"
    );
    assert_eq!(
        fs::read_to_string(&other_manifest).expect("read other manifest after interface change"),
        baseline_other_manifest,
        "unrelated module should stay out of dependency closure"
    );
}

/// Verifies incremental directory checking handles trait interfaces and
/// invalidation tracing without failing.
///
/// Inputs:
/// - A temporary trait provider module and a consumer module.
/// - A private helper edit followed by a public trait signature edit.
///
/// Output:
/// - Test success when incremental checking succeeds for the baseline and both
///   edits while producing interface and dependency-cache artifacts.
///
/// Transformation:
/// - Runs `check` in incremental mode with invalidation tracing enabled, then
///   rewrites the provider source through private and public changes.
#[test]
fn run_check_dir_incremental_with_trait_interfaces() {
    let dir = make_temp_dir("check_dir_incremental_trait_interfaces");
    fs::write(
            dir.join("trait_cache_lib.terl"),
            "module trait_cache_lib.\n\npub trait Label[A] {\n    show(value: A): Text.\n}.\n\nhelper(value: Int): Int ->\n    value + 1.\n\npub debug(value: Int): Int ->\n    helper(value).\n",
        )
        .expect("write trait_cache_lib");
    fs::write(
            dir.join("trait_cache_client.terl"),
            "module trait_cache_client.\n\nimport trait_cache_lib.{Label}.\n\npub render(value: Int): Int ->\n    value.\n",
        )
        .expect("write trait_cache_client");

    let cache = dir.join("cache");
    let state = CliState {
        incremental: true,
        trace_invalidation: true,
        cache_dir: Some(cache.clone()),
        ..Default::default()
    };
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into()],
        },
        state.clone(),
    );
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(cache.join("trait_cache_lib.typi").exists());
    assert!(cache.join("trait_cache_lib.typi.deps").exists());
    assert!(cache.join("trait_cache_client.typi").exists());

    fs::write(
            dir.join("trait_cache_lib.terl"),
            "module trait_cache_lib.\n\npub trait Label[A] {\n    show(value: A): Text.\n}.\n\nhelper(value: Int): Int ->\n    value + 2.\n\npub debug(value: Int): Int ->\n    helper(value).\n",
        )
        .expect("edit helper");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into()],
        },
        state.clone(),
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    fs::write(
            dir.join("trait_cache_lib.terl"),
            "module trait_cache_lib.\n\npub trait Label[A] {\n    show(value: A, flag: Int): Text.\n}.\n\nhelper(value: Int): Int ->\n    value + 2.\n\npub debug(value: Int): Int ->\n    helper(value).\n",
        )
        .expect("edit public trait and interface");
    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![dir.to_string_lossy().into()],
        },
        state,
    );
    assert_eq!(exit, ExitCode::SUCCESS);
}
