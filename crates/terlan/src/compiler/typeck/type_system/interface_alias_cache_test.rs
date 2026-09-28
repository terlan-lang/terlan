//! Cached interface parsing preserves edits, caller isolation, and bounded retention.

use super::*;
use crate::terlan_hir::ValuedUnionSignature;
use crate::terlan_typeck::Type;

fn fixture() -> ModuleInterface {
    ModuleInterface {
        module: "cache.Test".to_owned(),
        public_types: HashSet::from(["Value".to_owned()]),
        type_bodies: HashMap::from([("Value".to_owned(), vec!["Int".to_owned()])]),
        ..ModuleInterface::default()
    }
}

fn aliases(interface: &ModuleInterface) -> HashMap<String, TypeAlias> {
    super::super::interface_type_aliases(interface)
}

#[test]
fn edited_interfaces_and_caller_mutations_do_not_reuse_stale_types() {
    CACHE.with_borrow_mut(VecDeque::clear);
    let mut interface = fixture();
    let mut first = aliases(&interface);
    assert_eq!(first["Value"].body, Type::Int);
    first.get_mut("Value").unwrap().body = Type::Bool;
    assert_eq!(aliases(&interface)["Value"].body, Type::Int);
    assert!(
        get_or_parse(&interface, || panic!("unchanged input was reparsed")).contains_key("Value")
    );

    interface
        .type_bodies
        .insert("Value".to_owned(), vec!["Bool".to_owned()]);
    assert_eq!(aliases(&interface)["Value"].body, Type::Bool);
    interface
        .type_bodies
        .insert("Value".to_owned(), vec!["T".to_owned()]);
    interface
        .type_params
        .insert("Value".to_owned(), vec!["T".to_owned()]);
    assert_eq!(aliases(&interface)["Value"].params, vec![0]);
    interface.type_params.clear();
    assert!(aliases(&interface)["Value"].params.is_empty());

    interface.opaque_types.insert("Value".to_owned());
    assert!(!aliases(&interface).contains_key("Value"));
    interface.opaque_types.clear();
    assert!(aliases(&interface).contains_key("Value"));
    interface.valued_unions.insert(
        "Value".to_owned(),
        ValuedUnionSignature {
            name: "Value".to_owned(),
            representation: "Int".to_owned(),
            arms: vec![],
        },
    );
    assert!(!aliases(&interface).contains_key("Value"));
    interface.valued_unions.clear();
    assert!(aliases(&interface).contains_key("Value"));

    interface
        .type_bodies
        .insert("Value".to_owned(), vec!["Other".to_owned()]);
    let before = aliases(&interface)["Value"].body.clone();
    interface.public_types.insert("Other".to_owned());
    let after = aliases(&interface)["Value"].body.clone();
    assert_ne!(before, after);
    assert_eq!(
        after,
        Type::Named {
            module: None,
            name: "Other".to_owned(),
            args: vec![]
        }
    );
}

#[test]
fn eviction_and_oversized_inputs_preserve_parsing_results() {
    CACHE.with_borrow_mut(VecDeque::clear);
    for index in 0..MAX_ENTRIES + 1 {
        let interface = ModuleInterface {
            module: format!("cache.{index}"),
            ..fixture()
        };
        assert_eq!(aliases(&interface)["Value"].body, Type::Int);
    }
    CACHE.with_borrow(|entries| {
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert!(!entries.iter().any(|entry| entry.module == "cache.0"));
    });
    let mut interface = fixture();
    interface.module = "x".repeat(MAX_SOURCE_WEIGHT);
    assert_eq!(aliases(&interface)["Value"].body, Type::Int);
    CACHE.with_borrow(|entries| assert!(!entries.iter().any(|entry| entry.matches(&interface))));
    for index in 0..4 {
        interface.module = format!("{index}{}", "x".repeat(MAX_SOURCE_WEIGHT / 2));
        assert_eq!(aliases(&interface)["Value"].body, Type::Int);
    }
    CACHE.with_borrow(|entries| {
        assert!(
            entries
                .iter()
                .map(|entry| entry.source_weight)
                .sum::<usize>()
                <= MAX_SOURCE_WEIGHT
        );
    });
}
