//! Adversarial validation for generated C++ exception containment.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::super::generate_cpp_bindings;

/// Returns the package-neutral C++ fixture root.
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cpp_native_boundary")
}

/// Creates one unique temporary test directory.
fn temp_dir(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "terlan_cxx_exception_{name}_{}_{}",
        std::process::id(),
        suffix
    ))
}

/// Copies the native fixture and applies one manifest mutation.
fn fixture_variant(name: &str, mutate: impl FnOnce(&mut Value)) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("create exception variant");
    for file in ["native_boundary.hpp", "native_boundary.cc"] {
        fs::copy(fixture_dir().join(file), root.join(file)).expect("copy exception fixture");
    }
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(fixture_dir().join("native-binding.json"))
            .expect("read exception fixture"),
    )
    .expect("parse exception fixture");
    mutate(&mut manifest);
    let path = root.join("native-binding.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&manifest).expect("render exception variant"),
    )
    .expect("write exception variant");
    path
}

/// Returns one mutable package mapping policy by symbol ID.
fn policy_mut<'a>(manifest: &'a mut Value, symbol: &str) -> &'a mut Value {
    manifest["mapping"]["symbols"]
        .as_array_mut()
        .expect("mapping policies")
        .iter_mut()
        .find(|policy| policy["symbol"] == symbol)
        .expect("exception policy")
}

/// Returns one mutable generated function by public name.
fn function_mut<'a>(manifest: &'a mut Value, name: &str) -> &'a mut Value {
    manifest["modules"][0]["functions"]
        .as_array_mut()
        .expect("module functions")
        .iter_mut()
        .find(|function| function["name"] == name)
        .expect("exception function")
}

#[test]
fn throwing_symbols_require_explicit_stable_containment_policy() {
    let cases: &[(&str, &str, fn(&mut Value))] = &[
        ("missing", "cpp.exception.crossing", |manifest| {
            policy_mut(manifest, "method.native_boundary.tripled_or_throw")
                .as_object_mut()
                .expect("policy object")
                .remove("exception");
        }),
        ("code", "exception error code", |manifest| {
            policy_mut(manifest, "method.native_boundary.tripled_or_throw")["exception"]
                ["error_code"] = Value::String("Not Stable".into());
        }),
        ("message", "stable one-line message", |manifest| {
            policy_mut(manifest, "method.native_boundary.tripled_or_throw")["exception"]
                ["message"] = Value::String("line one\nline two".into());
        }),
        ("fallible", "requires typed fallible result", |manifest| {
            function_mut(manifest, "tripled_or_error")
                .as_object_mut()
                .expect("function object")
                .remove("fallible");
        }),
        ("return", "return type must be", |manifest| {
            function_mut(manifest, "tripled_or_error")["returns"] = Value::String("Int".into());
        }),
    ];
    for (name, expected, mutate) in cases {
        let manifest = fixture_variant(name, *mutate);
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("unsafe exception mapping must fail");
        assert!(
            error.contains(expected),
            "unexpected exception diagnostic: {error}"
        );
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
fn exception_policy_is_rejected_for_noexcept_or_rejected_symbols() {
    let noexcept = fixture_variant("noexcept", |manifest| {
        let policy = policy_mut(manifest, "method.native_boundary.value");
        policy["exception"] = serde_json::json!({
            "error_code": "impossible",
            "message": "This cannot throw."
        });
    });
    let error = generate_cpp_bindings(&noexcept, &temp_dir("noexcept_out"))
        .expect_err("noexcept policy must fail");
    assert!(error.contains("noexcept C++ symbol"));
    fs::remove_dir_all(noexcept.parent().expect("noexcept root")).expect("remove noexcept");

    let rejected = fixture_variant("rejected", |manifest| {
        policy_mut(manifest, "unsupported.exception")["exception"] = serde_json::json!({
            "error_code": "rejected",
            "message": "Rejected symbol."
        });
    });
    let error = generate_cpp_bindings(&rejected, &temp_dir("rejected_out"))
        .expect_err("rejected exception policy must fail");
    assert!(error.contains("cannot carry binding policy"));
    fs::remove_dir_all(rejected.parent().expect("rejected root")).expect("remove rejected");
}
