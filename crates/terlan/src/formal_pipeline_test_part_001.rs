use super::*;

use crate::validation::target_profile::TargetProfile;

const UNICODE_SOURCE_PATH: &str = "/tmp/terlan/☠☠☠/erl_544.terl";

/// Verifies compiler-owned CoreIR retains an exact printable-Unicode source
/// path for downstream VM diagnostics.
#[test]
fn formal_pipeline_retains_unicode_source_path_in_core_ir() {
    let artifacts = compile_syntax_module_through_phases_with_profile(
        UNICODE_SOURCE_PATH,
        "module erl_544.\n\npub err(): Int -> 544.\n",
        DiagnosticFormat::Text {
            color: crate::ColorChoice::Never,
        },
        None,
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("compile Unicode-path fixture");

    assert_eq!(
        artifacts.core.source.source_path.as_deref(),
        Some(UNICODE_SOURCE_PATH)
    );
}

/// Verifies the formal pipeline loads external package interfaces containing
/// bodyless receiver methods from the configured cache directory.
///
/// Inputs:
/// - One consumer source file and a generated-style `polars.DataFrame.typi`
///   cache entry.
///
/// Output:
/// - The loaded interface exposes both a free function and receiver method.
///
/// Transformation:
/// - Covers the interface-cache path used when a manifest-backed project builds
///   local path dependencies before its own source roots.
#[test]
fn formal_interface_loading_accepts_cached_package_receiver_methods() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_formal_package_interface_{}_{}",
        std::process::id(),
        nanos
    ));
    let source_dir = root.join("src/app");
    let cache_dir = root.join("cache");
    std::fs::create_dir_all(&source_dir).expect("create source fixture");
    std::fs::create_dir_all(&cache_dir).expect("create cache fixture");
    let source_path = source_dir.join("Main.terl");
    std::fs::write(&source_path, "module app.Main.\n").expect("write source fixture");
    std::fs::write(
        cache_dir.join("polars.DataFrame.typi"),
        r#"module polars.DataFrame.

pub opaque type DataFrame.

pub read_csv(path: String): Result[DataFrame, Error].
pub (df: DataFrame) height(): Int.
"#,
    )
    .expect("write package interface fixture");

    let interfaces = load_external_interfaces(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
        Some(&cache_dir),
    );
    let _ = std::fs::remove_dir_all(&root);

    let interface = interfaces
        .get("polars.DataFrame")
        .expect("cached Polars interface");
    assert!(interface.functions.contains_key(&("read_csv".into(), 1)));
    assert!(interface.functions.contains_key(&("height".into(), 1)));
}

/// Verifies formal interface loading ignores local generated std inventories.
///
/// Inputs:
/// - Temporary project root containing a forged `std/summaries/std.core.Bool`
///   summary with the wrong type surface.
///
/// Output:
/// - Test passes when `load_external_interfaces` resolves the embedded release
///   `std.core.Bool` contract instead of the generated-inventory fixture.
///
/// Transformation:
/// - Builds a throwaway project layout, asks the formal compiler path to load
///   external interfaces for one source file, and confirms std contracts come
///   from compiler-embedded summaries unless explicitly supplied through a
///   cache directory.
#[test]
fn formal_interface_loading_does_not_scan_generated_std_inventory() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_formal_no_std_scan_{}_{}",
        std::process::id(),
        nanos
    ));
    let source_dir = root.join("src/app");
    let summaries = root.join("std/summaries");
    std::fs::create_dir_all(&source_dir).expect("create source fixture");
    std::fs::create_dir_all(&summaries).expect("create summaries fixture");
    let source_path = source_dir.join("Main.terl");
    std::fs::write(&source_path, "module app.Main.\n").expect("write source fixture");
    std::fs::write(
        summaries.join("std.core.Bool.typi"),
        "\
module std.core.Bool.\n\
pub type Imposter = Atom[\"imposter\"].\n",
    )
    .expect("write forged std summary fixture");

    let interfaces = load_external_interfaces(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
        None,
    );
    let _ = std::fs::remove_dir_all(&root);

    let bool_interface = interfaces
        .get("std.core.Bool")
        .expect("embedded Bool interface");
    assert!(bool_interface
        .functions
        .contains_key(&("compare".into(), 2)));
    assert!(!bool_interface.public_types.contains("Imposter"));
}

/// Verifies hostile generated std summaries cannot poison embedded std loading.
///
/// Inputs:
/// - Temporary project root containing an invalid generated
///   `std/summaries/std.core.Bool.typi` file.
/// - A source file outside that generated summaries directory.
///
/// Output:
/// - Test passes when `load_external_interfaces` still returns the embedded
///   release `std.core.Bool` interface.
///
/// Transformation:
/// - Exercises the formal interface loader against an adversarial generated
///   std inventory. The compiler must not recursively scan generated summary
///   files from a project tree when release std summaries are embedded.
#[test]
fn adversarial_std_summary_loading_ignores_malformed_generated_inventory() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_formal_adversarial_std_summary_{}_{}",
        std::process::id(),
        nanos
    ));
    let source_dir = root.join("src/app");
    let summaries = root.join("std/summaries");
    std::fs::create_dir_all(&source_dir).expect("create source fixture");
    std::fs::create_dir_all(&summaries).expect("create generated summaries fixture");
    let source_path = source_dir.join("Main.terl");
    std::fs::write(&source_path, "module app.Main.\n").expect("write source fixture");
    std::fs::write(
        summaries.join("std.core.Bool.typi"),
        "module std.core.Bool.\npub type Broken = \n",
    )
    .expect("write malformed generated std summary fixture");

    let interfaces = load_external_interfaces(
        source_path
            .to_str()
            .expect("temporary source path should be utf-8"),
        None,
    );
    let _ = std::fs::remove_dir_all(&root);

    let bool_interface = interfaces
        .get("std.core.Bool")
        .expect("embedded Bool interface");
    assert!(bool_interface
        .functions
        .contains_key(&("compare".into(), 2)));
    assert!(!bool_interface.public_types.contains("Broken"));
}

/// Verifies source discovery does not cross nested project boundaries.
///
/// Inputs:
/// - A parent scratch directory with one normal `.terl` source.
/// - A nested child directory containing its own `terlan.toml` and source root.
///
/// Output:
/// - Test passes when discovery returns only the parent-owned source file.
///
/// Transformation:
/// - Builds a throwaway nested project layout and asks the formal source
///   discovery helper to scan the parent directory, proving nested manifests
///   act as project boundaries instead of contributing source paths to the
///   parent module layout.
#[test]
fn terlan_source_discovery_skips_nested_project_roots() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_formal_nested_project_scan_{}_{}",
        std::process::id(),
        nanos
    ));
    let parent_source_dir = root.join("app");
    let nested_source_dir = root.join("test/src/test");
    std::fs::create_dir_all(&parent_source_dir).expect("create parent source dir");
    std::fs::create_dir_all(&nested_source_dir).expect("create nested source dir");
    let parent_source = parent_source_dir.join("Main.terl");
    let nested_source = nested_source_dir.join("Main.terl");
    std::fs::write(&parent_source, "module app.Main.\n").expect("write parent source fixture");
    std::fs::write(
        root.join("test/terlan.toml"),
        "[package]\nname = \"test\"\n",
    )
    .expect("write nested manifest fixture");
    std::fs::write(&nested_source, "module test.Main.\n").expect("write nested source fixture");

    let files = terlan_sources_in_dir(&root).expect("scan parent source root");
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(files, vec![parent_source]);
    assert!(
        !files.contains(&nested_source),
        "nested project source must not leak into parent directory builds"
    );
}

#[test]
fn compile_syntax_module_with_erlang_profile_accepts_float() {
    let source = "\
module target_profile_accept.

pub f(): Float ->
  1.0.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/target_profile_accept.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::Vm,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}

#[test]
fn compile_syntax_module_with_profile_argument_accepts_float() {
    let source = "\
module target_profile_reject.

pub f(): Float ->
  1.0.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/target_profile_reject.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::Vm,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}

/// Verifies the strict formal compile path accepts the portable CoreIR v0
/// target subset for a Lean-covered body.
///
/// Inputs:
/// - Source text whose function body lowers to typed integer subtraction.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are written.
///
/// Transformation:
/// - Runs the full syntax-output parse/resolve/typecheck/CoreIR path with
///   `TargetProfile::CoreV0` and asserts no profile diagnostics are emitted.
#[test]
fn compile_syntax_module_with_core_v0_profile_accepts_covered_subset() {
    let source = "\
module target_profile_core_v0_accept.

pub f(x: Int, y: Int): Int ->
  x - y.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/target_profile_core_v0_accept.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::CoreV0,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}

/// Verifies the strict formal compile path rejects CoreIR outside the
/// portable CoreIR v0 target subset.
///
/// Inputs:
/// - Source text whose function body lowers to a typed map expression.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are written.
///
/// Transformation:
/// - Runs the full syntax-output parse/resolve/typecheck/CoreIR path with
///   `TargetProfile::CoreV0` and asserts target-profile diagnostics abort
///   compilation before artifacts are returned.
#[test]
fn compile_syntax_module_with_core_v0_profile_rejects_broad_coreir() {
    let source = "\
module target_profile_core_v0_reject.

pub f(): Map ->
  {a: 1}.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/target_profile_core_v0_reject.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::CoreV0,
    );

    assert_ne!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_none());
    assert!(
        result
            .core_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "target_profile_unsupported"),
        "Core v0 profile should report target-profile violations"
    );
}

/// Verifies native vector interface summaries are embedded with stdlib.
///
/// Inputs:
/// - Empty interface map.
///
/// Output:
/// - Test passes when `std.native.collections.Vector` is loaded from the
///   compiler-embedded std summary list.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so native std modules are
///   available for import resolution before target-capability diagnostics
///   decide whether the active backend may compile them.
#[test]
fn embedded_std_interfaces_include_native_vector_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let interface = interfaces
        .get("std.native.collections.Vector")
        .expect("embedded native vector interface");
    assert!(interface.opaque_types.contains("Vector"));
    assert!(interface.functions.contains_key(&("new".to_string(), 0)));
    let length = interface
        .functions
        .get(&("length".to_string(), 1))
        .expect("Vector.length receiver method");
    assert!(length.receiver_method);
    assert!(!length.receiver_mutable);
    let set_at = interface
        .functions
        .get(&("set_at".to_string(), 3))
        .expect("Vector.set_at mutable receiver method");
    assert!(set_at.receiver_method);
    assert!(set_at.receiver_mutable);
}

/// Verifies embedded std summaries include keyed enumerable contracts.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.collections.KeyedEnumerable` is loaded from the
///   embedded summary list with its binary higher-kinded trait.
///
/// Transformation:
/// - Keeps newly added std collection traits visible to installed compilers
///   that resolve std imports from embedded summaries.
#[test]
fn embedded_std_interfaces_include_keyed_enumerable_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let interface = interfaces
        .get("std.collections.KeyedEnumerable")
        .expect("embedded keyed enumerable interface");
    let trait_decl = interface
        .traits
        .get("KeyedEnumerable")
        .expect("KeyedEnumerable trait");
    assert_eq!(trait_decl.type_params, vec!["C[_, _]"]);
    assert!(trait_decl.methods.contains_key("map"));
    assert!(interface
        .trait_conformances
        .iter()
        .any(|conformance| conformance.trait_ref == "KeyedEnumerable[std.collections.Map.Map]"));
}

/// Verifies embedded std summaries include the portable task contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.core.Task` is loaded from the embedded summary
///   list with its opaque type and receiver composition methods.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so project imports can
///   resolve the typed async contract before target profiles decide whether
///   a backend can execute it.
#[test]
fn embedded_std_interfaces_include_core_task_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let interface = interfaces
        .get("std.core.Task")
        .expect("embedded core task interface");
    assert!(interface.opaque_types.contains("Task"));
    assert!(interface.functions.contains_key(&("done".to_string(), 1)));
    assert!(interface.functions.contains_key(&("spawn".to_string(), 1)));
    let then = interface
        .functions
        .get(&("then".to_string(), 2))
        .expect("Task.then receiver method");
    assert!(then.receiver_method);
    assert!(!then.receiver_mutable);
    let result = interface
        .functions
        .get(&("result".to_string(), 1))
        .expect("Task.result receiver method");
    assert!(result.receiver_method);
}

/// Verifies embedded std summaries include the portable Float math contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.core.Float` exposes finite rounding and constants.
///
/// Transformation:
/// - Exercises the installed-compiler interface path used before CoreIR
///   intrinsic lowering resolves these public functions.
#[test]
fn embedded_std_interfaces_include_float_math_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let interface = interfaces
        .get("std.core.Float")
        .expect("embedded core float interface");
    for (name, arity) in [("floor", 1), ("ceil", 1), ("log", 1), ("pi", 0), ("tau", 0)] {
        assert!(
            interface.functions.contains_key(&(name.to_string(), arity)),
            "missing Float.{name}/{arity}"
        );
    }
}

/// Verifies embedded std summaries include the portable JSON contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.data.Json` is loaded from the embedded summary
///   list with its opaque type, derived error type, and receiver accessors.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so project imports can
///   resolve the JSON API before target profiles decide whether a backend
///   can execute the Rust/NativeBoundary implementation.
#[test]
fn embedded_std_interfaces_include_data_json_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let interface = interfaces
        .get("std.data.Json")
        .expect("embedded data json interface");
    assert!(interface.opaque_types.contains("Json"));
    assert!(interface.public_types.contains("JsonError"));
    assert!(interface.functions.contains_key(&("parse".to_string(), 1)));
    assert!(interface
        .functions
        .contains_key(&("stringify".to_string(), 1)));
    let get = interface
        .functions
        .get(&("get".to_string(), 2))
        .expect("Json.get receiver method");
    assert!(get.receiver_method);
    assert!(!get.receiver_mutable);
    let is_null = interface
        .functions
        .get(&("is_null".to_string(), 1))
        .expect("Json.is_null receiver method");
    assert!(is_null.receiver_method);
    assert!(!is_null.receiver_mutable);
}

/// Verifies embedded std summaries include the Postgres capability contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.db.Postgres` is loaded from the embedded summary
///   list with its first public pool/query/row contract.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so imports can resolve the
///   Postgres source API before target profiles decide whether a backend can
///   execute the database capability.
#[test]
fn embedded_std_interfaces_include_db_postgres_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let postgres = interfaces
        .get("std.db.Postgres")
        .expect("embedded Postgres interface");
    assert!(postgres.opaque_types.contains("Pool"));
    assert!(postgres.opaque_types.contains("Connection"));
    assert!(postgres.opaque_types.contains("Row"));
    assert!(postgres.public_types.contains("Config"));
    assert!(postgres.functions.contains_key(&("connect".to_string(), 1)));
    assert!(postgres.functions.contains_key(&("query".to_string(), 3)));
    assert!(postgres
        .functions
        .contains_key(&("query_one".to_string(), 3)));
    assert!(postgres.functions.contains_key(&("execute".to_string(), 3)));
    assert!(postgres
        .functions
        .contains_key(&("transaction".to_string(), 2)));

    let string = postgres
        .functions
        .get(&("string".to_string(), 2))
        .expect("Postgres.Row.string receiver method");
    assert!(string.receiver_method);
    assert!(!string.receiver_mutable);
    assert!(postgres.functions.contains_key(&("int".to_string(), 2)));
    assert!(postgres.functions.contains_key(&("bool".to_string(), 2)));
    assert!(postgres.functions.contains_key(&("json".to_string(), 2)));
}

/// Verifies embedded std summaries include Rust-backed web/data utilities.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.encoding.Base64`, `std.encoding.Md5`, `std.io.Path`, `std.net.Uri`,
///   and HTTP utility modules are loaded from the embedded summary list with
///   their public contract surfaces.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so project imports can
///   resolve portable web/data utility APIs before target profiles decide
///   whether a backend can execute their Rust/NativeBoundary implementations.
#[test]
fn embedded_std_interfaces_include_web_data_utility_contracts() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let base64 = interfaces
        .get("std.encoding.Base64")
        .expect("embedded Base64 interface");
    assert!(base64.public_types.contains("Base64Error"));
    assert!(base64.functions.contains_key(&("encode".to_string(), 1)));
    assert!(base64.functions.contains_key(&("decode".to_string(), 1)));

    let md5 = interfaces
        .get("std.encoding.Md5")
        .expect("embedded MD5 interface");
    assert!(md5.functions.contains_key(&("digest".to_string(), 1)));

    let path = interfaces
        .get("std.io.Path")
        .expect("embedded Path interface");
    assert!(path.opaque_types.contains("Path"));
    assert!(path.public_types.contains("PathError"));
    assert!(path.functions.contains_key(&("from_string".to_string(), 1)));
    let join = path
        .functions
        .get(&("join".to_string(), 2))
        .expect("Path.join receiver method");
    assert!(join.receiver_method);

    let uri = interfaces
        .get("std.net.Uri")
        .expect("embedded Uri interface");
    assert!(uri.opaque_types.contains("Uri"));
    assert!(uri.public_types.contains("UriError"));
    assert!(uri.functions.contains_key(&("parse".to_string(), 1)));
    let host = uri
        .functions
        .get(&("host".to_string(), 1))
        .expect("Uri.host receiver method");
    assert!(host.receiver_method);

    let request = interfaces
        .get("std.http.Request")
        .expect("embedded Request interface");
    assert!(request.opaque_types.contains("Request"));
    assert!(request.functions.contains_key(&("method".to_string(), 1)));
    assert!(request.functions.contains_key(&("path".to_string(), 1)));
    assert!(request.functions.contains_key(&("param".to_string(), 2)));
    assert!(request.functions.contains_key(&("query".to_string(), 2)));
    assert!(request.functions.contains_key(&("cookie".to_string(), 2)));
    assert!(request.functions.contains_key(&("cookies".to_string(), 1)));
    assert!(request
        .functions
        .contains_key(&("body_text".to_string(), 1)));

    let response = interfaces
        .get("std.http.Response")
        .expect("embedded Response interface");
    assert!(response.opaque_types.contains("Response"));
    assert!(response.functions.contains_key(&("text".to_string(), 2)));
    assert!(response.functions.contains_key(&("html".to_string(), 2)));
    assert!(response
        .functions
        .contains_key(&("redirect".to_string(), 2)));
    assert!(response
        .functions
        .contains_key(&("set_cookie_header".to_string(), 2)));

    let cookies = interfaces
        .get("std.http.Cookies")
        .expect("embedded Cookies interface");
    assert!(cookies.opaque_types.contains("Jar"));
    assert!(cookies.public_types.contains("Options"));
    assert!(cookies.public_types.contains("SameSite"));
    assert!(cookies.functions.contains_key(&("get".to_string(), 2)));
    assert!(cookies.functions.contains_key(&("set".to_string(), 6)));
    assert!(cookies.functions.contains_key(&("delete".to_string(), 3)));

    let router = interfaces
        .get("std.http.Router")
        .expect("embedded Router interface");
    assert!(router.opaque_types.contains("Router"));
    assert!(router.public_types.contains("Handler"));
    assert!(router.functions.contains_key(&("new".to_string(), 0)));
    assert!(router.functions.contains_key(&("get".to_string(), 3)));
    assert!(router.functions.contains_key(&("post".to_string(), 3)));
    assert!(router.functions.contains_key(&("put".to_string(), 3)));
    assert!(router.functions.contains_key(&("patch".to_string(), 3)));
    assert!(router.functions.contains_key(&("delete".to_string(), 3)));
    assert!(router.functions.contains_key(&("head".to_string(), 3)));
    assert!(router.functions.contains_key(&("fallback".to_string(), 2)));

    let tls = interfaces
        .get("std.http.Tls")
        .expect("embedded Tls interface");
    assert!(tls.public_types.contains("Config"));
    assert!(tls.public_types.contains("Mode"));
    assert!(tls.public_types.contains("Provider"));
}

/// Verifies embedded std summaries include the JavaScript std seed contracts.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when shared JS wrappers and generated DOM seed modules are
///   loaded from the embedded summary list.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so `std.js.*` imports resolve
///   through the same packaged interface path as hand-authored std modules.
#[test]
fn embedded_std_interfaces_include_js_std_contracts() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let string = interfaces
        .get("std.js.String")
        .expect("embedded JS String interface");
    assert!(string.opaque_types.contains("JsString"));
    assert!(string.functions.contains_key(&("from_core".to_string(), 1)));
    let to_core = string
        .functions
        .get(&("to_core".to_string(), 1))
        .expect("JsString.to_core receiver method");
    assert!(to_core.receiver_method);

    let array = interfaces
        .get("std.js.Array")
        .expect("embedded JS Array interface");
    assert!(array.opaque_types.contains("Array"));
    assert!(array.functions.contains_key(&("from_list".to_string(), 1)));
    let length = array
        .functions
        .get(&("length".to_string(), 1))
        .expect("Array.length receiver method");
    assert!(length.receiver_method);

    let promise = interfaces
        .get("std.js.Promise")
        .expect("embedded JS Promise interface");
    assert!(promise.opaque_types.contains("Promise"));
    assert!(promise
        .functions
        .contains_key(&("from_task".to_string(), 1)));
    let to_task = promise
        .functions
        .get(&("to_task".to_string(), 1))
        .expect("Promise.to_task receiver method");
    assert!(to_task.receiver_method);

    let number = interfaces
        .get("std.js.Number")
        .expect("embedded JS Number interface");
    assert!(number.opaque_types.contains("JsNumber"));
    assert!(number
        .functions
        .contains_key(&("from_float".to_string(), 1)));
    let to_float = number
        .functions
        .get(&("to_float".to_string(), 1))
        .expect("Number.to_float receiver method");
    assert!(to_float.receiver_method);

    let document = interfaces
        .get("std.js.Dom.Document")
        .expect("embedded generated DOM Document interface");
    assert!(document.opaque_types.contains("Document"));
    assert!(document.functions.contains_key(&("title".to_string(), 1)));

    let html_element = interfaces
        .get("std.js.Dom.HTMLElement")
        .expect("embedded generated DOM HTMLElement interface");
    assert!(html_element.opaque_types.contains("HTMLElement"));
    assert!(html_element
        .functions
        .contains_key(&("inner_text".to_string(), 1)));
}

/// Verifies generated JS std summaries resolve during normal JS compilation.
///
/// Inputs:
/// - A source module importing `std.js.String.JsString`.
///
/// Output:
/// - Test assertion only; compilation succeeds under `js.shared`.
///
/// Transformation:
/// - Runs the full parser, embedded-interface resolution, typecheck, CoreIR,
///   and target-profile path without a cache directory so the test proves the
///   compiler binary carries the generated JS std summary.
#[test]
fn compile_syntax_module_with_js_profile_resolves_js_string_summary() {
    let source = "\
module js_summary_accept.

import type std.js.String.JsString.

pub accepts(value: JsString): JsString ->
  value.
";

    let result = compile_syntax_module_through_phases_with_diagnostics_for_profile(
        "src/js_summary_accept.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    );

    assert_eq!(result.exit_code, ExitCode::SUCCESS);
    assert!(result.artifacts.is_some());
    assert!(result.core_diagnostics.is_empty());
}
