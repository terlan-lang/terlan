use super::*;

#[test]
fn none_cast_infers_its_option_layout_without_constructor_inventory() {
    use crate::compiler::native_ir::expression::infer_native_type_with_constructors;
    use crate::compiler::native_ir::{native_type, NativeType};
    use crate::terlan_typeck::core_type_from_text;
    use std::collections::HashMap;

    for target in [
        CoreType::Apply {
            constructor: "std.core.Option.Option".into(),
            args: vec![CoreType::String],
        },
        core_type_from_text("Atom[\"none\"] | {Atom[\"some\"], value: String}").unwrap(),
    ] {
        let expected = native_type(Some(&target), &target.contract_text());
        assert!(matches!(expected, Some(NativeType::ManagedRef(_))));
        for (value, result) in [
            (CoreExpr::Atom("none".into()), expected),
            (CoreExpr::Atom("missing".into()), None),
            (CoreExpr::Var("none".into()), None),
        ] {
            assert_eq!(
                infer_native_type_with_constructors(
                    &CoreExpr::Cast {
                        expr: Box::new(value),
                        target_type: target.clone(),
                    },
                    &HashMap::from([("none".into(), NativeType::Int)]),
                    &HashMap::new(),
                    &HashMap::new(),
                ),
                result,
            );
        }
    }
}

#[test]
fn source_union_nullary_arguments_keep_managed_storage() {
    crate::compiler::native_ir::source_constructor_test::check_sources(&[r#"
module union_arguments.
import std.collections.List.
type Done.
type Value = {Atom["value"], item: Int}.
type Entry = Value | Done.
type State = Atom["open"] | Atom["closed"].
append(values: List[Entry], entry: Entry): List[Entry] -> List.concat(values, [entry]).
state(value: State): State -> value.
preserve(done: Entry): Entry -> done.
not_done(done: Entry): Bool -> done != Done.
pub check(): Bool ->
    let values = append(append([], Value(42)), Done);
    let matched = case values { [Value(42), Done] -> true; _ -> false };
    matched and values[0] != Done and values[1] == Done
        and Done == values[1] and Done != values[0]
        and not_done(preserve(Value(42))) and not not_done(preserve(Done))
        and state(Atom["open"]) == Atom["open"]
        and state(Atom["closed"]) == Atom["closed"].
"#]);
}

#[test]
fn none_in_an_atom_union_keeps_its_scalar_representation() {
    let value = CoreExpr::Atom("none".into());
    let same_site = CoreType::Union(
        ["lax", "strict", "none"]
            .into_iter()
            .map(|name| CoreType::AtomLiteral(name.into()))
            .collect(),
    );
    assert!(!is_none_option_value(&value, &same_site));
    assert!(is_none_option_value(
        &value,
        &CoreType::Apply {
            constructor: "Option".into(),
            args: vec![same_site],
        }
    ));
}
