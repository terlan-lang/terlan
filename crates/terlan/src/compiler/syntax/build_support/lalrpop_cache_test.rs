use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

fn temporary_directory() -> PathBuf {
    std::env::temp_dir().join(format!(
        "terlan-lalrpop-cache-{}-{}",
        std::process::id(),
        NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
    ))
}

#[test]
fn cache_identity_covers_grammar_and_generator_lockfile() {
    let baseline = identity(b"grammar", b"lockfile");
    assert_eq!(baseline, identity(b"grammar", b"lockfile"));
    assert_ne!(baseline, identity(b"changed", b"lockfile"));
    assert_ne!(baseline, identity(b"grammar", b"changed"));
}

#[test]
fn generated_parser_cache_restores_exact_bytes() {
    let root = temporary_directory();
    let generated = root.join("profile-a/compiler/syntax/terlan_lalrpop.rs");
    let restored = root.join("profile-b/compiler/syntax/terlan_lalrpop.rs");
    fs::create_dir_all(generated.parent().expect("generated parent")).expect("create output");
    fs::write(&generated, b"generated parser").expect("write generated parser");

    assert!(!restore(&root.join("cache"), "identity", &restored).expect("missing cache"));
    record(&root.join("cache"), "identity", &generated).expect("record cache");
    assert!(restore(&root.join("cache"), "identity", &restored).expect("restore cache"));
    assert_eq!(
        fs::read(restored).expect("read restored parser"),
        b"generated parser"
    );

    fs::remove_dir_all(root).expect("remove temporary cache");
}

#[test]
fn cache_replaces_an_abandoned_temporary_file() {
    let root = temporary_directory();
    let cache = root.join("cache");
    let generated = root.join("generated.rs");
    fs::create_dir_all(&cache).expect("create cache");
    fs::write(&generated, b"complete").expect("write generated parser");
    fs::write(
        cache.join(format!(".identity.{}.tmp", std::process::id())),
        b"partial",
    )
    .expect("write abandoned temporary file");

    record(&cache, "identity", &generated).expect("replace temporary file");
    let restored = root.join("restored.rs");
    assert!(restore(&cache, "identity", &restored).expect("restore parser"));
    assert_eq!(fs::read(restored).expect("read parser"), b"complete");

    fs::remove_dir_all(root).expect("remove temporary cache");
}
