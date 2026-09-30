use super::*;

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
