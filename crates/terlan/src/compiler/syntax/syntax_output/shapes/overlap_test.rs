use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn rejects_distinct_shapes_with_equivalent_case_patterns() {
    let error = parse_module_as_syntax_output(
        "module overlapping_case_shapes.\n\
         shape User(id) = {Atom[\"entity\"], id}.\n\
         shape Account(value) = {Atom[\"entity\"], value}.\n\
         pub classify(input: Dynamic): Int ->\n\
             case input {\n\
                 User(id) -> id;\n\
                 Account(value) -> value;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect_err("equivalent shape case clauses must fail");

    assert!(format!("{error:?}").contains(
        "ambiguous shape expansion: distinct aliases `User` and `Account` produce equivalent unguarded clause patterns"
    ));
}

#[test]
fn rejects_distinct_shapes_with_equivalent_function_patterns() {
    let error = parse_module_as_syntax_output(
        "module overlapping_function_shapes.\n\
         shape User(id) = {Atom[\"entity\"], id}.\n\
         shape Account(value) = {Atom[\"entity\"], value}.\n\
         pub classify(User(id)) -> id;\n\
         classify(Account(value)) -> value.\n",
    )
    .expect_err("equivalent shape function clauses must fail");

    assert!(format!("{error:?}").contains(
        "ambiguous shape expansion: distinct aliases `User` and `Account` produce equivalent unguarded clause patterns"
    ));
}

#[test]
fn accepts_guarded_or_structurally_distinct_shape_clauses() {
    let output = parse_module_as_syntax_output(
        "module distinct_shape_clauses.\n\
         shape Positive(value) = value where value > 0.\n\
         shape Small(value) = value where value < 10.\n\
         shape Left(value) = {Atom[\"left\"], value}.\n\
         shape Right(value) = {Atom[\"right\"], value}.\n\
         pub classify(input: Dynamic): Int ->\n\
             case input {\n\
                 Positive(value) -> value;\n\
                 Small(value) -> value;\n\
                 Left(value) -> value;\n\
                 Right(value) -> value;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("guards and structural constraints distinguish shape clauses");

    assert_eq!(output.declarations.len(), 5);
}

#[test]
fn rejects_later_case_shape_subsumed_by_earlier_shape() {
    let error = parse_module_as_syntax_output(
        "module shadowed_case_shape.\n\
         shape Any(value) = value.\n\
         shape Entity(value) = {Atom[\"entity\"], value}.\n\
         pub classify(input: Dynamic): Int ->\n\
             case input {\n\
                 Any(value) -> value;\n\
                 Entity(value) -> value\n\
             }.\n",
    )
    .expect_err("a broad earlier shape must not shadow a later shape");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: earlier alias `Any` subsumes later alias `Entity`"
    ));
}

#[test]
fn rejects_later_function_shape_subsumed_by_earlier_shape() {
    let error = parse_module_as_syntax_output(
        "module shadowed_function_shape.\n\
         shape Any(value) = value.\n\
         shape Entity(value) = {Atom[\"entity\"], value}.\n\
         pub classify(Any(value)) -> value;\n\
         classify(Entity(value)) -> value.\n",
    )
    .expect_err("a broad earlier function shape must not shadow a later shape");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: earlier alias `Any` subsumes later alias `Entity`"
    ));
}

#[test]
fn accepts_specific_shape_before_broad_fallback_shape() {
    let output = parse_module_as_syntax_output(
        "module ordered_shape_fallback.\n\
         shape Entity(value) = {Atom[\"entity\"], value}.\n\
         shape Any(value) = value.\n\
         pub classify(input: Dynamic): Int ->\n\
             case input {\n\
                 Entity(value) -> value;\n\
                 Any(value) -> value\n\
             }.\n",
    )
    .expect("a specific shape may precede a broad fallback shape");

    assert_eq!(output.declarations.len(), 3);
}

#[test]
fn rejects_later_map_shape_with_stricter_required_fields() {
    let error = parse_module_as_syntax_output(
        "module shadowed_map_shape.\n\
         shape Ok(value) = {kind: Atom[\"ok\"], value: value}.\n\
         shape AdminOk(value) = {kind: Atom[\"ok\"], role: Atom[\"admin\"], value: value}.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Ok(value) -> value;\n\
                 AdminOk(value) -> value\n\
             }.\n",
    )
    .expect_err("fewer required fields must subsume a stricter map shape");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: earlier alias `Ok` subsumes later alias `AdminOk`"
    ));
}

#[test]
fn accepts_partially_overlapping_shapes_when_both_are_useful() {
    let output = parse_module_as_syntax_output(
        "module useful_partial_shape_overlap.\n\
         shape First(value) = {1, value}.\n\
         shape Second(value) = {value, 2}.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 First(value) -> value;\n\
                 Second(value) -> value;\n\
                 _ -> Atom[\"none\"]\n\
             }.\n",
    )
    .expect("crossing overlap is valid when each shape matches distinct values");

    assert_eq!(output.declarations.len(), 3);
}

#[test]
fn rejects_guarded_later_case_shape_shadowed_by_unguarded_shape() {
    let error = parse_module_as_syntax_output(
        "module shadowed_guarded_case_shape.\n\
         shape Any(value) = value.\n\
         shape Positive(value) = value where value > 0.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Any(value) -> value;\n\
                 Positive(value) -> value\n\
             }.\n",
    )
    .expect_err("a later guard cannot recover from structural shadowing");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: earlier alias `Any` subsumes later alias `Positive`"
    ));
}

#[test]
fn rejects_guarded_later_function_shape_shadowed_by_unguarded_shape() {
    let error = parse_module_as_syntax_output(
        "module shadowed_guarded_function_shape.\n\
         shape Any(value) = value.\n\
         shape Entity(value) = {Atom[\"entity\"], value}.\n\
         pub classify(Any(value)) -> value;\n\
         classify(Entity(value)) where value > 0 -> value.\n",
    )
    .expect_err("a later function guard cannot recover from structural shadowing");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: earlier alias `Any` subsumes later alias `Entity`"
    ));
}

#[test]
fn accepts_guarded_broad_shape_before_unguarded_fallback() {
    let output = parse_module_as_syntax_output(
        "module guarded_shape_fallback.\n\
         shape Positive(value) = value where value > 0.\n\
         shape Any(value) = value.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Positive(value) -> value;\n\
                 Any(value) -> value\n\
             }.\n",
    )
    .expect("a guarded broad shape may precede an unguarded fallback");

    assert_eq!(output.declarations.len(), 3);
}

#[test]
fn rejects_alpha_equivalent_guarded_case_shapes() {
    let error = parse_module_as_syntax_output(
        "module equivalent_guarded_case_shapes.\n\
         shape Positive(value) = value where value > 0.\n\
         shape AlsoPositive(item) = item where item > 0.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Positive(value) -> value;\n\
                 AlsoPositive(item) -> item\n\
             }.\n",
    )
    .expect_err("alpha-equivalent guarded shapes must not duplicate clauses");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: earlier alias `Positive` subsumes later alias `AlsoPositive` with an equivalent guard"
    ));
}

#[test]
fn rejects_alpha_equivalent_guarded_function_shapes() {
    let error = parse_module_as_syntax_output(
        "module equivalent_guarded_function_shapes.\n\
         shape Entity(value) = {Atom[\"entity\"], value}.\n\
         shape AlsoEntity(item) = {Atom[\"entity\"], item}.\n\
         pub classify(Entity(value)) where value > 0 -> value;\n\
         classify(AlsoEntity(item)) where item > 0 -> item.\n",
    )
    .expect_err("alpha-equivalent guarded function shapes must not duplicate clauses");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: earlier alias `Entity` subsumes later alias `AlsoEntity` with an equivalent guard"
    ));
}

#[test]
fn accepts_equivalent_shape_patterns_with_distinct_guards() {
    let output = parse_module_as_syntax_output(
        "module distinct_guarded_shape_predicates.\n\
         shape Positive(value) = value where value > 0.\n\
         shape Negative(value) = value where value < 0.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Positive(value) -> value;\n\
                 Negative(value) -> value;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("distinct predicates keep equivalent structural patterns useful");

    assert_eq!(output.declarations.len(), 3);
}

#[test]
fn rejects_later_case_shape_with_contained_integer_range() {
    let error = parse_module_as_syntax_output(
        "module contained_guard_range.\n\
         shape Success(body) = {status, body} where status >= 200 and status < 300.\n\
         shape NarrowSuccess(body) = {status, body} where status >= 220 and status < 230.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Success(body) -> body;\n\
                 NarrowSuccess(body) -> body\n\
             }.\n",
    )
    .expect_err("a contained later integer range is unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `NarrowSuccess` implies the earlier guard for subsuming alias `Success`"
    ));
}

#[test]
fn rejects_later_function_shape_with_stricter_integer_bound() {
    let error = parse_module_as_syntax_output(
        "module contained_function_guard.\n\
         shape Positive(value) = value where value > 0.\n\
         shape Large(value) = value where value > 10.\n\
         pub classify(Positive(value)) -> value;\n\
         classify(Large(value)) -> value.\n",
    )
    .expect_err("a stricter later function guard is unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `Large` implies the earlier guard for subsuming alias `Positive`"
    ));
}

#[test]
fn rejects_equality_guard_contained_by_reversed_integer_bound() {
    let error = parse_module_as_syntax_output(
        "module contained_reversed_guard.\n\
         shape Positive(value) = value where 0 < value.\n\
         shape Five(value) = value where value == 5.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Positive(value) -> value;\n\
                 Five(value) -> value\n\
             }.\n",
    )
    .expect_err("equality must be recognized inside a reversed broader bound");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `Five` implies the earlier guard for subsuming alias `Positive`"
    ));
}

#[test]
fn rejects_later_disjunction_when_every_branch_is_contained() {
    let error = parse_module_as_syntax_output(
        "module contained_disjunctive_guard.\n\
         shape Outside(value) = value where value < 0 or value >= 10.\n\
         shape FarOutside(value) = value where value < -10 or value > 20.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Outside(value) -> value;\n\
                 FarOutside(value) -> value\n\
             }.\n",
    )
    .expect_err("every contained branch of a later disjunction is unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `FarOutside` implies the earlier guard for subsuming alias `Outside`"
    ));
}

#[test]
fn rejects_later_range_contained_by_disjunction_of_conjunctions() {
    let error = parse_module_as_syntax_output(
        "module contained_guard_window.\n\
         shape Window(value) = value where value >= 0 and value < 10 or value >= 20 and value < 30.\n\
         shape InnerWindow(value) = value where value >= 22 and value < 25.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Window(value) -> value;\n\
                 InnerWindow(value) -> value\n\
             }.\n",
    )
    .expect_err("a later interval inside one earlier disjunct is unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `InnerWindow` implies the earlier guard for subsuming alias `Window`"
    ));
}

#[test]
fn accepts_disjunctive_guard_when_later_range_crosses_a_gap() {
    let output = parse_module_as_syntax_output(
        "module useful_disjunctive_guard.\n\
         shape Outside(value) = value where value < 0 or value >= 10.\n\
         shape CrossesGap(value) = value where value > -5 and value < 15.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Outside(value) -> value;\n\
                 CrossesGap(value) -> value;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("a later range crossing an earlier disjunction gap remains useful");

    assert_eq!(output.declarations.len(), 3);
}

#[test]
fn accepts_guard_implication_beyond_branch_budget_conservatively() {
    let branches = (0..=64)
        .map(|value| format!("value == {value}"))
        .collect::<Vec<_>>()
        .join(" or ");
    let source = format!(
        "module bounded_guard_proof.\n\
         shape Many(value) = value where {branches}.\n\
         shape Exact(value) = value where value == 64.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {{\n\
                 Many(value) -> value;\n\
                 Exact(value) -> value;\n\
                 _ -> 0\n\
             }}.\n"
    );

    let output = parse_module_as_syntax_output(&source)
        .expect("proof-budget exhaustion must remain conservative");

    assert_eq!(output.declarations.len(), 3);
}

#[test]
fn rejects_later_case_guard_with_implied_variable_relation() {
    let error = parse_module_as_syntax_output(
        "module implied_case_relation.\n\
         shape Ordered(left, right) = {left, right} where left < right.\n\
         shape PositiveOrdered(first, second) = {first, second} where second > first and first >= 0.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Ordered(left, right) -> left;\n\
                 PositiveOrdered(first, second) -> second\n\
             }.\n",
    )
    .expect_err("a normalized later relation implied by an earlier guard is unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `PositiveOrdered` implies the earlier guard for subsuming alias `Ordered`"
    ));
}

#[test]
fn rejects_later_function_guard_with_implied_variable_equality() {
    let error = parse_module_as_syntax_output(
        "module implied_function_relation.\n\
         shape Same(left, right) = {left, right} where left == right.\n\
         shape PositiveSame(first, second) = {first, second} where second == first and first > 0.\n\
         pub classify(Same(left, right)) -> left;\n\
         classify(PositiveSame(first, second)) -> second.\n",
    )
    .expect_err("commuted equality plus a stronger conjunct is unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `PositiveSame` implies the earlier guard for subsuming alias `Same`"
    ));
}

#[test]
fn accepts_distinct_variable_relations_on_equivalent_patterns() {
    let output = parse_module_as_syntax_output(
        "module distinct_guard_relations.\n\
         shape Ascending(left, right) = {left, right} where left < right.\n\
         shape Descending(left, right) = {left, right} where left > right.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Ascending(left, right) -> left;\n\
                 Descending(left, right) -> right;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("opposing variable relations keep both clauses useful");

    assert_eq!(output.declarations.len(), 3);
}

#[test]
fn accepts_narrow_guard_before_broader_guard_fallback() {
    let output = parse_module_as_syntax_output(
        "module ordered_guard_ranges.\n\
         shape Large(value) = value where value > 10.\n\
         shape Positive(value) = value where value > 0.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Large(value) -> value;\n\
                 Positive(value) -> value;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("a narrower range may precede its broader guarded fallback");

    assert_eq!(output.declarations.len(), 3);
}
