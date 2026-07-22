use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn rejects_later_guard_with_transitive_strict_relation() {
    let error = parse_module_as_syntax_output(
        "module transitive_strict_relation.\n\
         shape Ordered(first, middle, last) = {first, middle, last} where first < last.\n\
         shape OrderedChain(a, b, c) = {a, b, c} where a < b and b <= c.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Ordered(first, middle, last) -> middle;\n\
                 OrderedChain(a, b, c) -> b\n\
             }.\n",
    )
    .expect_err("a transitive strict relation makes the later guard unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `OrderedChain` implies the earlier guard for subsuming alias `Ordered`"
    ));
}

#[test]
fn rejects_later_function_guard_with_transitive_equality() {
    let error = parse_module_as_syntax_output(
        "module transitive_relation_equality.\n\
         shape SameEnds(first, middle, last) = {first, middle, last} where first == last.\n\
         shape NonStrictCycle(a, b, c) = {a, b, c} where a <= b and b <= c and c <= a.\n\
         pub classify(SameEnds(first, middle, last)) -> middle;\n\
         classify(NonStrictCycle(a, b, c)) -> b.\n",
    )
    .expect_err("mutual non-strict reachability proves equality");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `NonStrictCycle` implies the earlier guard for subsuming alias `SameEnds`"
    ));
}

#[test]
fn rejects_contradictory_later_relation_guard() {
    let error = parse_module_as_syntax_output(
        "module contradictory_relation_guard.\n\
         shape Distinct(left, right) = {left, right} where left != right.\n\
         shape Impossible(first, second) = {first, second} where first < second and second <= first.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Distinct(left, right) -> left;\n\
                 Impossible(first, second) -> second\n\
             }.\n",
    )
    .expect_err("a strict relation cycle is impossible and therefore unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `Impossible` implies the earlier guard for subsuming alias `Distinct`"
    ));
}

#[test]
fn rejects_later_guard_with_equality_inequality_conflict() {
    let error = parse_module_as_syntax_output(
        "module contradictory_relation_equality.\n\
         shape Ordered(left, right) = {left, right} where left <= right.\n\
         shape Impossible(first, second) = {first, second} where first == second and first != second.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Ordered(left, right) -> left;\n\
                 Impossible(first, second) -> second\n\
             }.\n",
    )
    .expect_err("equality and inequality on the same values are contradictory");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `Impossible` implies the earlier guard for subsuming alias `Ordered`"
    ));
}

#[test]
fn accepts_non_strict_chain_when_earlier_guard_requires_strict_order() {
    let output = parse_module_as_syntax_output(
        "module useful_non_strict_chain.\n\
         shape StrictEnds(first, middle, last) = {first, middle, last} where first < last.\n\
         shape NonStrictChain(a, b, c) = {a, b, c} where a <= b and b <= c.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 StrictEnds(first, middle, last) -> middle;\n\
                 NonStrictChain(a, b, c) -> b;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("a non-strict chain does not prove a strict relation");

    assert_eq!(output.declarations.len(), 3);
}
