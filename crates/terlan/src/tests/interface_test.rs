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
    let interface_path = success_dir.join("fixture.terli");
    fs::write(
        &interface_path,
        "//! Cache contract interface.\nmodule cache_contract.\n\n/// User ID alias.\npub type UserId = Int.\n\n/// User ID box alias.\npub type UserBox[T] = {box, T}.\n\n/// Cache handle.\npub opaque type Cache.\n\n/// Reads a value from the cache.\npub get(Cache: Cache, Key: Binary): Result[Binary, not_found].\n\n/// Trait for logging values.\npub trait Logger[A] {\n    log(V: A): Dynamic.\n}.\n",
    )
    .expect("write interface fixture");
    let path = interface_path.to_string_lossy().to_string();
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
    let emitted_deps =
        fs::read_to_string(out_dir.join("cache_contract.typi.deps")).expect("read typi deps");
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
    assert!(emitted_deps.contains("module=cache_contract"));
    assert!(emitted_deps.contains("syntax_contract_schema=terlan-syntax-contract-v1"));
    assert!(emitted_deps.contains("deps=0"));

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

/// Verifies `interface` can summarize implementation source files.
///
/// Inputs:
/// - A `.terl` source module containing public implementations.
///
/// Output:
/// - Test passes when public signatures and dependency metadata are emitted.
///
/// Transformation:
/// - Exercises the stdlib regeneration path now that the removed `emit`
///   command is no longer available.
#[test]
fn run_interface_accepts_source_modules() {
    let dir = make_temp_dir("interface_source_module");
    let source_dir = dir.join("src");
    fs::create_dir_all(&source_dir).expect("create source dir");
    let source = fixture(
        &source_dir,
        "//! Source interface.\nmodule source_contract.\n\n/// Box alias.\npub type Boxed[T] = {box, T}.\n\n/// Adds two ints.\npub add(left: Int, right: Int): Int ->\n    left + right.\n",
    );
    let out_dir = dir.join("out");

    let exit = commands::interface::run(
        &[source],
        &CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let emitted = fs::read_to_string(out_dir.join("source_contract.typi")).expect("read typi");
    let emitted_deps =
        fs::read_to_string(out_dir.join("source_contract.typi.deps")).expect("read typi deps");
    assert!(emitted.contains("//! Source interface."));
    assert!(emitted.contains("pub type Boxed[T] =\n    {box, T}."));
    assert!(emitted.contains("pub add(left: Int, right: Int): Int."));
    assert!(emitted_deps.contains("module=source_contract"));
    assert!(emitted_deps.contains("deps=0"));
}

/// Verifies batch interface generation resolves dependencies from the complete
/// generated source set rather than output order.
#[test]
fn run_interface_batch_hashes_generated_dependencies() {
    let dir = make_temp_dir("interface_batch_dependencies");
    let provider_dir = dir.join("provider");
    let consumer_dir = dir.join("consumer");
    fs::create_dir_all(&provider_dir).expect("create provider dir");
    fs::create_dir_all(&consumer_dir).expect("create consumer dir");
    let provider = fixture(
        &provider_dir,
        "module batch.Provider.\n\npub type Value = Int.\n\npub value(): Value -> 1.\n",
    );
    let consumer = fixture(
        &consumer_dir,
        "module batch.Consumer.\n\nimport batch.Provider.{value}.\nimport type batch.Provider.{Value}.\n\npub consume(): Value -> value().\n",
    );
    let out_dir = dir.join("out");

    let exit = commands::interface::run(
        &[consumer, provider],
        &CliState {
            out_dir: out_dir.clone(),
            ..Default::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let deps = fs::read_to_string(out_dir.join("batch.Consumer.typi.deps"))
        .expect("read batch consumer deps");
    assert!(deps.contains("deps=1"));
    assert!(deps.contains("batch.Provider="));
    let summary = fs::read_to_string(out_dir.join("batch.Consumer.typi"))
        .expect("read batch consumer summary");
    assert!(summary.contains("pub consume(): batch.Provider.Value."));
    assert!(!summary.contains("pub consume(): Value."));
}

/// Verifies batch interface generation rejects duplicate module identities.
#[test]
fn run_interface_batch_rejects_duplicate_modules() {
    let dir = make_temp_dir("interface_batch_duplicate_modules");
    let first_dir = dir.join("first");
    let second_dir = dir.join("second");
    fs::create_dir_all(&first_dir).expect("create first dir");
    fs::create_dir_all(&second_dir).expect("create second dir");
    let first = fixture(
        &first_dir,
        "module duplicate.Module.\n\npub one(): Int -> 1.\n",
    );
    let second = fixture(
        &second_dir,
        "module duplicate.Module.\n\npub two(): Int -> 2.\n",
    );

    let exit = commands::interface::run(&[first, second], &CliState::default());

    assert_eq!(exit, ExitCode::from(1));
}
