use crate::terlan_typeck::CoreFunction;

use super::source_declaration_identity;

fn function(name: &str, arity: usize) -> CoreFunction {
    CoreFunction {
        name: name.to_string(),
        arity,
        public: false,
        generic_params: Vec::new(),
        native_operation: None,
        params: Vec::new(),
        return_type: "Int".to_string(),
        core_return_type: None,
        clauses: Vec::new(),
    }
}

/// Cross-module concrete generic clones retain their defining declaration.
#[test]
fn qualified_generic_symbol_recovers_source_declaration() {
    let generic = function("$aot_generic_rust_quality.SourceInventory.reverse_0", 1);

    assert_eq!(
        source_declaration_identity("rust_quality.FileHeadroom", &generic),
        (
            "rust_quality.SourceInventory".to_string(),
            "reverse".to_string(),
            1,
        )
    );
}

/// Ordinary functions continue to point at their own module and declaration.
#[test]
fn ordinary_symbol_retains_local_source_declaration() {
    let ordinary = function("check", 2);

    assert_eq!(
        source_declaration_identity("rust_quality.FileHeadroom", &ordinary),
        (
            "rust_quality.FileHeadroom".to_string(),
            "check".to_string(),
            2,
        )
    );
}

/// Malformed generated-looking symbols cannot forge a foreign source owner.
#[test]
fn malformed_generic_symbol_falls_back_to_runtime_owner() {
    let malformed = function("$aot_generic_reverse_0", 1);

    assert_eq!(
        source_declaration_identity("rust_quality.FileHeadroom", &malformed),
        (
            "rust_quality.FileHeadroom".to_string(),
            "$aot_generic_reverse_0".to_string(),
            1,
        )
    );
}
