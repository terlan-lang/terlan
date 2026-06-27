use super::*;

/// Verifies `interface` emits `.typi` summaries and reports stable errors for
/// invalid interface-generation inputs.
///
/// Inputs:
/// - A temporary `.terl` module with module docs, type aliases, an opaque type,
///   a public function signature, and a trait declaration.
/// - Empty arguments, an unparsable source file, a blocked output directory,
///   and a conflicting output file path.
///
/// Output:
/// - Test success when interface generation writes the expected `.typi`
///   content, usage errors return exit code `2`, and read/write/parse failures
///   return exit code `1`.
///
/// Transformation:
/// - Runs the interface command against success and error fixtures, then
///   inspects the emitted interface text and exit codes.
#[test]
fn run_interface_success_and_error_paths() {
    let dir = make_temp_dir("interface_paths");
    let success_dir = dir.join("success");
    fs::create_dir_all(&success_dir).expect("create success dir");
    let path = fixture(
            &success_dir,
            "//! Cache contract interface.\nmodule cache_contract.\n\n/// User ID alias.\npub type UserId = Int.\n\n/// User ID box alias.\npub type UserBox[T] = {box, T}.\n\n/// Cache handle.\npub opaque type Cache.\n\n/// Reads a value from the cache.\npub get(Cache: Cache, Key: Binary): Result[Binary, not_found].\n\n/// Trait for logging values.\npub trait Logger[A] {\n    log(V: A): Dynamic.\n}.\n",
        );
    let out_dir = dir.join("out");
    let exit = commands::interface::run(
        &[path.clone()],
        &CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);
    let emitted = fs::read_to_string(out_dir.join("cache_contract.typi")).expect("read typi");
    assert!(emitted.contains("//! Cache contract interface."));
    assert!(emitted.contains("/// User ID alias."));
    assert!(emitted.contains("pub type UserId =\n    Int."));
    assert!(emitted.contains("/// User ID box alias."));
    assert!(emitted.contains("pub type UserBox[T] =\n    {box, T}."));
    assert!(emitted.contains("/// Cache handle."));
    assert!(emitted.contains("/// Reads a value from the cache."));
    assert!(emitted.contains("pub opaque type Cache."));
    assert!(emitted.contains("pub get(Cache: Cache, Key: Binary): Result[Binary, not_found]."));
    assert!(emitted.contains("/// Trait for logging values."));
    assert!(emitted.contains("pub trait Logger[A]"));
    assert!(emitted.contains("log(V: A): Dynamic."));

    let exit = commands::interface::run(&[], &CliState::default());
    assert_eq!(exit, ExitCode::from(2));

    let bad_dir = dir.join("bad_parse");
    fs::create_dir_all(&bad_dir).expect("create bad dir");
    let bad_parse = fixture(&bad_dir, "module broken\n");
    let exit = commands::interface::run(&[bad_parse], &CliState::default());
    assert_eq!(exit, ExitCode::from(1));

    let blocked_dir = dir.join("blocked_interface_out");
    fs::write(&blocked_dir, "not-a-dir").expect("write blocked out");
    let exit = commands::interface::run(
        &[path.clone()],
        &CliState {
            out_dir: blocked_dir,
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::from(1));

    let out_dir = dir.join("write_fail");
    fs::create_dir_all(&out_dir).expect("create out");
    fs::create_dir_all(out_dir.join("cache_contract.typi")).expect("create conflicting target");
    let exit = commands::interface::run(
        &[path],
        &CliState {
            out_dir,
            incremental: true,
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::from(1));
}
