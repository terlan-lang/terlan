use crate::compiler::native_ir::{NativeExpr, NativeFunction, NativeType};

use super::*;

fn module(name: &str, value: i64) -> NativeModule {
    NativeModule {
        name: name.to_string(),
        functions: vec![NativeFunction {
            export_id: value as u64 + 100,
            name: "value".to_string(),
            public: true,
            arity: 0,
            source_module: name.to_string(),
            source_function: "value".to_string(),
            source_arity: 0,
            callable_captures: Vec::new(),
            params: Vec::new(),
            return_type: NativeType::Int,
            body: NativeExpr::Int(value),
        }],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

#[test]
fn application_fingerprint_invalidates_units_when_a_sibling_body_changes() {
    let original = vec![module("app.First", 1), module("app.Second", 2)];
    let same = vec![module("app.First", 1), module("app.Second", 2)];
    let changed_sibling = vec![module("app.First", 1), module("app.Second", 3)];

    assert_eq!(
        application_implementation_fingerprint(&original),
        application_implementation_fingerprint(&same)
    );
    assert_ne!(
        application_implementation_fingerprint(&original),
        application_implementation_fingerprint(&changed_sibling)
    );
}

/// Application metadata is an object input even when the NativeIR is identical.
#[test]
fn module_cache_separates_application_identity_and_reuses_matching_objects() -> Result<(), String> {
    let root = crate::support::test_fs::TestDirectory::new("native_units", "application_identity");
    let natives = vec![module("fixture.Value", 1)];
    let policy = NativeCodegenPolicy::Development;
    let prepare = |application| {
        prepare_native_object_units(&root, application, &natives, "host-fixture", policy)
            .map_err(|error| format!("{error:?}"))
    };
    let first = prepare("first_application")?;
    let first_paths = first.paths.clone();
    let first_bytes = fs::read(&first_paths[0]).map_err(|error| error.to_string())?;
    drop(first);
    let renamed = prepare("renamed_application")?;
    assert_ne!(first_paths, renamed.paths);
    let expected =
        emit_native_module_object_with_policy("renamed_application", &natives, 0, policy)
            .map_err(|error| error.to_string())?;
    let renamed_bytes = fs::read(&renamed.paths[0]).map_err(|error| error.to_string())?;
    assert_eq!(renamed_bytes, expected);
    // ELF records the object name as a file symbol. Other formats may omit it.
    if cfg!(target_os = "linux") {
        assert_ne!(first_bytes, renamed_bytes);
    }
    let modified = fs::metadata(&renamed.paths[0])
        .and_then(|metadata| metadata.modified())
        .map_err(|error| error.to_string())?;
    let renamed_paths = renamed.paths.clone();
    drop(renamed);
    let warm = prepare("renamed_application")?;
    assert_eq!(warm.paths, renamed_paths);
    assert_eq!(
        fs::metadata(&renamed_paths[0])
            .and_then(|metadata| metadata.modified())
            .map_err(|error| error.to_string())?,
        modified
    );
    drop(warm);
    root.close();
    Ok(())
}
