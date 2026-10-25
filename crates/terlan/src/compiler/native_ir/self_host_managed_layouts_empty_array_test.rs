use super::*;

#[test]
fn empty_array_lowers_without_fabricated_fields() {
    let expression = aggregate_expr("put-array", "Array[0, Int]", vec![])
        .expect("empty array lowering")
        .expect("array expression");
    let NativeExpr::Construct {
        descriptor, fields, ..
    } = expression
    else {
        panic!("empty fixed array must use aggregate construction");
    };
    assert!(fields.is_empty());
    assert!(descriptor.fields().is_empty());
    assert_eq!(descriptor.managed().size(), 1);
}

#[test]
fn qualified_array_types_keep_nominal_layout_contracts() {
    let ty = parse_type("std.js.Array.Array[Int]").expect("nominal array type");
    assert_eq!(
        type_contract(&ty, "").expect("nominal array contract"),
        "Apply(std.js.Array.Array;Int)"
    );
    assert!(aggregate_expr("put-array", "std.js.Array.Array[2, Int]", vec![]).is_err());
}
