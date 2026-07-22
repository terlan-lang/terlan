//! Tests for target-specific Clang canonical type metadata.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::generate_cpp_bindings;

/// Returns one collision-resistant temporary test directory.
fn temp_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("terlan_cpp_extracted_{name}_{nonce}"))
}

/// Rewrites one type object to the canonical spelling emitted by Linux Clang.
fn canonicalize_type(ty: &mut Value) {
    let Some(spelling) = ty.get("spelling").and_then(Value::as_str) else {
        return;
    };
    let canonical = match spelling {
        "std::int64_t" => Some("long"),
        "std::unique_ptr<NativeBoundary>" => {
            Some("std::unique_ptr<terlan_fixture::NativeBoundary>")
        }
        "std::unique_ptr<NativeGauge>" => Some("std::unique_ptr<terlan_fixture::NativeGauge>"),
        "std::unique_ptr<std::vector<std::uint8_t>>" => {
            Some("std::unique_ptr<std::vector<unsigned char>>")
        }
        "std::unique_ptr<std::vector<std::int64_t>>" => Some("std::unique_ptr<std::vector<long>>"),
        _ => None,
    };
    if let Some(canonical) = canonical {
        ty["canonical"] = Value::String(canonical.to_string());
    }
}

/// Applies target canonicalization to every extracted declaration type.
fn canonicalize_symbols(manifest: &mut Value) {
    for symbol in manifest["cpp_metadata"]["symbols"]
        .as_array_mut()
        .expect("symbols")
    {
        if let Some(receiver) = symbol.get_mut("receiver") {
            if let Some(name) = receiver.as_str() {
                *receiver = Value::String(format!("terlan_fixture::{name}"));
            }
        }
        if let Some(returns) = symbol.get_mut("returns") {
            canonicalize_type(returns);
        }
        if let Some(parameters) = symbol.get_mut("parameters").and_then(Value::as_array_mut) {
            for parameter in parameters {
                canonicalize_type(&mut parameter["ty"]);
            }
        }
        if let Some(fields) = symbol.get_mut("fields").and_then(Value::as_array_mut) {
            for field in fields {
                canonicalize_type(&mut field["ty"]);
            }
        }
    }
}

/// Proves generated bridges consume maintained-Clang aliases and qualified names.
#[test]
fn clang_canonical_aliases_generate_the_same_cxx_surface() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cpp_native_boundary");
    let root = temp_dir("aliases");
    fs::create_dir_all(&root).expect("create fixture root");
    for file in ["native_boundary.hpp", "native_boundary.cc"] {
        fs::copy(fixture.join(file), root.join(file)).expect("copy fixture input");
    }
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(fixture.join("native-binding.json")).expect("read manifest"),
    )
    .expect("parse manifest");
    canonicalize_symbols(&mut manifest);
    let manifest_path = root.join("native-binding.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("render manifest"),
    )
    .expect("write manifest");
    let out = temp_dir("aliases_out");

    generate_cpp_bindings(&manifest_path, &out).expect("generate canonicalized fixture");
    let bridge = fs::read_to_string(out.join("native/rust/src/lib.rs")).expect("read bridge");
    assert!(bridge.contains("self: &NativeBoundary"));
    assert!(bridge.contains("-> UniquePtr<NativeBoundary>"));
    assert!(bridge.contains("-> UniquePtr<CxxVector<u8>>"));
    assert!(bridge.contains("-> UniquePtr<CxxVector<i64>>"));

    fs::remove_dir_all(root).expect("remove fixture root");
    fs::remove_dir_all(out).expect("remove output root");
}
