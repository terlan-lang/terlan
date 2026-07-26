use super::{NativeExpr, NativeFunction, NativeModule, NativeType};

fn module(body: NativeExpr) -> NativeModule {
    NativeModule {
        name: "app.Fingerprint".to_string(),
        functions: vec![NativeFunction {
            export_id: 7,
            name: "value".to_string(),
            public: true,
            arity: 0,
            source_module: "app.Fingerprint".to_string(),
            source_function: "value".to_string(),
            source_arity: 0,
            callable_captures: Vec::new(),
            params: Vec::new(),
            return_type: NativeType::Int,
            body,
        }],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

#[test]
fn native_module_fingerprint_is_deterministic_and_content_sensitive() {
    let first = module(NativeExpr::Int(1));
    let same = module(NativeExpr::Int(1));
    let changed = module(NativeExpr::Int(2));

    assert_eq!(first.fingerprint_sha256(), same.fingerprint_sha256());
    assert_ne!(first.fingerprint_sha256(), changed.fingerprint_sha256());
    assert_eq!(first.fingerprint_sha256().len(), 64);
}
