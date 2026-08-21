use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn main() {
    emit_checked_frontend_revision();
    emit_native_codegen_revision();
    emit_native_build_policy();
    println!("cargo:rerun-if-changed=src/compiler/syntax/terlan_lalrpop.lalrpop");
    lalrpop::Configuration::new()
        .use_cargo_dir_conventions()
        .process()
        .expect("generate the Terlan LALRPOP parser");
}

/// Binds checked-implementation caches to the compiler phases that produced
/// their syntax, HIR, type evidence, and CoreIR payloads.
fn emit_checked_frontend_revision() {
    let roots = [
        Path::new("src/compiler/syntax"),
        Path::new("src/compiler/hir"),
        Path::new("src/compiler/typeck"),
        Path::new("src/formal_pipeline"),
        Path::new("src/validation"),
    ];
    let standalone_sources = [
        Path::new("src/compiler/purity.rs"),
        Path::new("src/compiler/value_lifecycle.rs"),
        Path::new("src/database_schema.rs"),
        Path::new("src/formal_pipeline.rs"),
        Path::new("src/template_inputs.rs"),
    ];
    let mut sources = standalone_sources
        .iter()
        .map(|path| path.to_path_buf())
        .collect();
    for root in roots {
        collect_rust_sources(root, &mut sources);
    }
    emit_source_revision("TERLAN_CHECKED_FRONTEND_REVISION_SHA256", sources);
}

/// Binds native-cache identities to dependency resolution and compile policy.
///
/// The source revision alone is insufficient: Cargo features can remove or add
/// backend/runtime code without changing a source byte, and a lockfile update
/// can change code emitted by a compiler dependency.
fn emit_native_build_policy() {
    let lockfile = Path::new("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lockfile.display());
    let lockfile_bytes = fs::read(lockfile).unwrap_or_else(|error| {
        panic!("read workspace lockfile `{}`: {error}", lockfile.display())
    });
    let mut features = env::vars()
        .filter_map(|(name, value)| {
            name.strip_prefix("CARGO_FEATURE_")
                .map(|feature| format!("{feature}={value}"))
        })
        .collect::<Vec<_>>();
    features.sort();

    let mut digest = Sha256::new();
    digest.update(b"terlan-native-build-policy-v1\0");
    digest.update(env::var("PROFILE").unwrap_or_default().as_bytes());
    digest.update(b"\0");
    digest.update(env::var("TARGET").unwrap_or_default().as_bytes());
    digest.update(b"\0");
    for feature in features {
        digest.update(feature.as_bytes());
        digest.update(b"\0");
    }
    digest.update(Sha256::digest(lockfile_bytes));
    let identity = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    println!("cargo:rustc-env=TERLAN_NATIVE_BUILD_POLICY_SHA256={identity}");
}

/// Binds native-object cache identities to the compiler implementation that
/// emitted them. Package and Cranelift versions alone cannot distinguish dirty
/// or unversioned code-generator changes during compiler development.
fn emit_native_codegen_revision() {
    let roots = [
        Path::new("src/compiler/native_ir"),
        Path::new("src/commands/build/vm_artifact"),
        Path::new("src/runtime/native_image"),
    ];
    let mut sources = Vec::new();
    for root in roots {
        collect_rust_sources(root, &mut sources);
    }
    emit_source_revision("TERLAN_NATIVE_CODEGEN_REVISION_SHA256", sources);
}

/// Emits a deterministic content identity for one compiler source closure.
fn emit_source_revision(environment_name: &str, mut sources: Vec<PathBuf>) {
    sources.sort();
    let mut digest = Sha256::new();
    for path in sources {
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!("read native codegen source `{}`: {error}", path.display())
        });
        let name = path.to_string_lossy();
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    let revision = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    println!("cargo:rustc-env={environment_name}={revision}");
}

fn collect_rust_sources(root: &Path, output: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(root)
        .unwrap_or_else(|error| {
            panic!(
                "read native codegen directory `{}`: {error}",
                root.display()
            )
        })
        .map(|entry| entry.expect("read native codegen directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rust_sources(&path, output);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            output.push(path);
        }
    }
}
