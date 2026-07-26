use std::collections::HashMap;

#[test]
fn hidden_continuation_slots_keep_new_source_locals_disjoint() {
    let sparse = HashMap::from([("capture".to_string(), 1), ("result".to_string(), 2)]);
    assert_eq!(super::control::next_local_index(&sparse), 3);
}
