use super::{value_binding, VALUE_BINDINGS};

#[test]
fn resource_contracts_are_exact_unique_and_not_value_bindings() {
    let mut operations = std::collections::BTreeSet::new();
    for contract in super::RESOURCE_OPERATIONS
        .iter()
        .flat_map(|group| group.iter())
    {
        assert!(operations.insert(contract.operation));
        assert!(std::ptr::eq(
            super::resource_operation(contract.operation).unwrap(),
            contract
        ));
        assert!(value_binding(contract.operation).is_none());
        for name in [
            format!("{}suffix", contract.operation),
            format!("prefix{}", contract.operation),
            contract.operation.to_uppercase(),
        ] {
            assert!(super::resource_operation(&name).is_none());
        }
    }
    for name in [
        "",
        "std.data.json.",
        "std.data.json.unknown",
        "std.net.uri.parse_parts",
        "app.data.json.parse",
    ] {
        assert!(super::resource_operation(name).is_none());
    }
}

#[test]
fn registration_is_unique_and_lookup_is_exact() {
    let mut operations = std::collections::BTreeSet::new();
    for binding in VALUE_BINDINGS {
        assert!(operations.insert(binding.operation));
        assert!(std::ptr::eq(
            value_binding(binding.operation).unwrap(),
            binding
        ));
        for name in [
            format!("{}suffix", binding.operation),
            format!("prefix{}", binding.operation),
            binding.operation.to_uppercase(),
        ] {
            assert!(value_binding(&name).is_none());
        }
    }
    assert!(value_binding("").is_none());
    assert!(value_binding("app.uri.parse_parts").is_none());
}
