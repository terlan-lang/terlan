use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

fn main() {
    emit_native_codegen_revision();
    println!("cargo:rerun-if-changed=src/compiler/syntax/terlan_lalrpop.lalrpop");
    lalrpop::Configuration::new()
        .use_cargo_dir_conventions()
        .process()
        .expect("generate the Terlan LALRPOP parser");
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
    println!("cargo:rustc-env=TERLAN_NATIVE_CODEGEN_REVISION_SHA256={revision}");
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
