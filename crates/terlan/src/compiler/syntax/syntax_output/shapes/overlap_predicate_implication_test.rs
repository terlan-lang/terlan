use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn rejects_later_case_guard_repeating_predicate_with_stronger_constraint() {
    let error = parse_module_as_syntax_output(
        "module predicate_guard_implication.\n\
         pub accepted(value: Int): Bool -> value > 0.\n\
         shape Accepted(value) = value where accepted(value).\n\
         shape AcceptedSmall(item) = item where accepted(item) and item < 10.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Accepted(value) -> value;\n\
                 AcceptedSmall(item) -> item\n\
             }.\n",
    )
    .expect_err("repeated predicate evidence makes the stronger later guard unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `AcceptedSmall` implies the earlier guard for subsuming alias `Accepted`"
    ));
}

#[test]
fn rejects_later_function_guard_implying_predicate_disjunction() {
    let error = parse_module_as_syntax_output(
        "module predicate_disjunction_implication.\n\
         pub selected(value: Int): Bool -> value > 0.\n\
         shape Selected(value) = value where selected(value) or value >= 100.\n\
         shape SelectedPositive(item) = item where selected(item) and item > 0.\n\
         pub classify(Selected(value)) -> value;\n\
         classify(SelectedPositive(item)) -> item.\n",
    )
    .expect_err("one retained disjunction branch makes the later function guard unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `SelectedPositive` implies the earlier guard for subsuming alias `Selected`"
    ));
}

#[test]
fn rejects_distinct_local_predicates_with_equivalent_visible_bodies() {
    let error = parse_module_as_syntax_output(
        "module distinct_predicate_guards.\n\
         pub accepted(value: Int): Bool -> value > 0.\n\
         pub reviewed(value: Int): Bool -> value > 0.\n\
         shape Accepted(value) = value where accepted(value).\n\
         shape ReviewedPositive(item) = item where reviewed(item) and item > 0.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Accepted(value) -> value;\n\
                 ReviewedPositive(item) -> item;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect_err("visible equivalent predicate bodies make the later shape unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `ReviewedPositive` implies the earlier guard for subsuming alias `Accepted`"
    ));
}

#[test]
fn rejects_distinct_local_predicate_body_that_implies_earlier_body() {
    let error = parse_module_as_syntax_output(
        "module distinct_predicate_body_implication.\n\
         pub positive(value: Int): Bool -> value > 0.\n\
         pub above_ten(value: Int): Bool -> value > 10.\n\
         shape Positive(value) = value where positive(value).\n\
         shape AboveTen(item) = item where above_ten(item).\n\
         pub classify(Positive(value)) -> value;\n\
         classify(AboveTen(item)) -> item.\n",
    )
    .expect_err("the stronger visible helper body makes the later shape unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `AboveTen` implies the earlier guard for subsuming alias `Positive`"
    ));
}

#[test]
fn accepts_distinct_predicates_with_call_bearing_bodies_conservatively() {
    let output = parse_module_as_syntax_output(
        "module opaque_predicate_guards.\n\
         pub normalize(value: Int): Int -> value.\n\
         pub accepted(value: Int): Bool -> normalize(value) > 0.\n\
         pub reviewed(value: Int): Bool -> normalize(value) > 10.\n\
         shape Accepted(value) = value where accepted(value).\n\
         shape Reviewed(item) = item where reviewed(item).\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Accepted(value) -> value;\n\
                 Reviewed(item) -> item;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("call-bearing helper bodies remain outside compile-time implication proof");

    assert_eq!(output.declarations.len(), 6);
}

#[test]
fn does_not_use_non_bool_function_bodies_as_predicate_proofs() {
    let output = parse_module_as_syntax_output(
        "module non_bool_predicate_proof.\n\
         pub accepted(value: Int): Int -> value > 0.\n\
         pub reviewed(value: Int): Int -> value > 10.\n\
         shape Accepted(value) = value where accepted(value).\n\
         shape Reviewed(item) = item where reviewed(item).\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Accepted(value) -> value;\n\
                 Reviewed(item) -> item;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("non-Bool helper declarations remain typechecker errors, not overlap proofs");

    assert_eq!(output.declarations.len(), 5);
}

#[test]
fn accepts_later_predicate_when_earlier_guard_requires_extra_evidence() {
    let output = parse_module_as_syntax_output(
        "module missing_predicate_evidence.\n\
         pub normalize(value: Int): Int -> value.\n\
         pub accepted(value: Int): Bool -> normalize(value) > 0.\n\
         shape AcceptedPositive(value) = value where accepted(value) and value > 0.\n\
         shape Accepted(item) = item where accepted(item).\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 AcceptedPositive(value) -> value;\n\
                 Accepted(item) -> item;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("a later predicate alone does not imply an earlier conjunction");

    assert_eq!(output.declarations.len(), 5);
}

#[test]
fn accepts_same_predicate_with_distinct_arguments() {
    let output = parse_module_as_syntax_output(
        "module distinct_predicate_arguments.\n\
         pub within(value: Int, lower: Int, upper: Int): Bool ->\n\
             value > lower and value < upper.\n\
         shape LowWindow(value) = value where within(value, 0, 10).\n\
         shape HighWindow(item) = item where within(item, 5, 15).\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 LowWindow(value) -> value;\n\
                 HighWindow(item) -> item;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("crossing predicate arguments do not imply one another");

    assert_eq!(output.declarations.len(), 4);
}

#[test]
fn rejects_later_guard_repeating_negated_predicate() {
    let error = parse_module_as_syntax_output(
        "module negated_predicate_implication.\n\
         pub accepted(value: Int): Bool -> value > 0.\n\
         shape Rejected(value) = value where not accepted(value).\n\
         shape RejectedNegative(item) = item where not accepted(item) and item < 0.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Rejected(value) -> value;\n\
                 RejectedNegative(item) -> item\n\
             }.\n",
    )
    .expect_err("repeated negated predicate evidence makes the later guard unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `RejectedNegative` implies the earlier guard for subsuming alias `Rejected`"
    ));
}

#[test]
fn rejects_contradictory_positive_and_negated_predicate_guard() {
    let error = parse_module_as_syntax_output(
        "module contradictory_predicate_guard.\n\
         pub accepted(value: Int): Bool -> value > 0.\n\
         shape Positive(value) = value where value > 0.\n\
         shape Impossible(item) = item where accepted(item) and not accepted(item).\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Positive(value) -> value;\n\
                 Impossible(item) -> item\n\
             }.\n",
    )
    .expect_err("a predicate and its negation make the later guard impossible");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `Impossible` implies the earlier guard for subsuming alias `Positive`"
    ));
}

#[test]
fn rejects_double_negation_as_positive_predicate_evidence() {
    let error = parse_module_as_syntax_output(
        "module double_negated_predicate_guard.\n\
         pub accepted(value: Int): Bool -> value > 0.\n\
         shape Accepted(value) = value where accepted(value).\n\
         shape DoubleAccepted(item) = item where not not accepted(item) and item < 10.\n\
         pub classify(Accepted(value)) -> value;\n\
         classify(DoubleAccepted(item)) -> item.\n",
    )
    .expect_err("double negation preserves positive predicate evidence");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `DoubleAccepted` implies the earlier guard for subsuming alias `Accepted`"
    ));
}

#[test]
fn accepts_opposing_predicate_polarities_as_distinct_guards() {
    let output = parse_module_as_syntax_output(
        "module opposing_predicate_polarities.\n\
         pub accepted(value: Int): Bool -> value > 0.\n\
         shape Accepted(value) = value where accepted(value).\n\
         shape Rejected(item) = item where not accepted(item).\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Accepted(value) -> value;\n\
                 Rejected(item) -> item\n\
             }.\n",
    )
    .expect("positive and negated predicates match disjoint guard domains");

    assert_eq!(output.declarations.len(), 4);
}

#[test]
fn rejects_compound_negation_equivalent_to_explicit_de_morgan_guard() {
    let error = parse_module_as_syntax_output(
        "module compound_predicate_negation.\n\
         pub accepted(value: Int): Bool -> value > 0.\n\
         pub reviewed(value: Int): Bool -> value > 0.\n\
         shape Neither(value) = value where not (accepted(value) or reviewed(value)).\n\
         shape ExplicitNeither(item) = item where not accepted(item) and not reviewed(item).\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Neither(value) -> value;\n\
                 ExplicitNeither(item) -> item;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect_err("De Morgan normalization makes the later guard unreachable");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `ExplicitNeither` implies the earlier guard for subsuming alias `Neither`"
    ));
}

#[test]
fn rejects_negated_conjunction_equivalent_to_negative_disjunction() {
    let error = parse_module_as_syntax_output(
        "module negated_predicate_conjunction.\n\
         pub accepted(value: Int): Bool -> value > 0.\n\
         pub reviewed(value: Int): Bool -> value > 0.\n\
         shape NotBoth(value) = value where not (accepted(value) and reviewed(value)).\n\
         shape MissingOne(item) = item where not accepted(item) or not reviewed(item).\n\
         pub classify(NotBoth(value)) -> value;\n\
         classify(MissingOne(item)) -> item.\n",
    )
    .expect_err("negated conjunction normalizes to a disjunction of negated predicates");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `MissingOne` implies the earlier guard for subsuming alias `NotBoth`"
    ));
}

#[test]
fn rejects_compound_negation_contradicted_by_positive_predicate() {
    let error = parse_module_as_syntax_output(
        "module compound_predicate_contradiction.\n\
         pub accepted(value: Int): Bool -> value > 0.\n\
         pub reviewed(value: Int): Bool -> value > 0.\n\
         shape Positive(value) = value where value > 0.\n\
         shape Impossible(item) = item where not (accepted(item) or reviewed(item)) and accepted(item).\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Positive(value) -> value;\n\
                 Impossible(item) -> item\n\
             }.\n",
    )
    .expect_err("De Morgan normalization exposes the contradictory predicate branch");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `Impossible` implies the earlier guard for subsuming alias `Positive`"
    ));
}

#[test]
fn accepts_partial_negative_evidence_for_earlier_negative_conjunction() {
    let output = parse_module_as_syntax_output(
        "module incomplete_negative_predicate_evidence.\n\
         pub normalize(value: Int): Int -> value.\n\
         pub accepted(value: Int): Bool -> normalize(value) > 0.\n\
         pub reviewed(value: Int): Bool -> normalize(value) > 0.\n\
         shape Neither(value) = value where not (accepted(value) or reviewed(value)).\n\
         shape NotAccepted(item) = item where not accepted(item).\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Neither(value) -> value;\n\
                 NotAccepted(item) -> item;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("one negated predicate does not imply a conjunction of two");

    assert_eq!(output.declarations.len(), 6);
}

#[test]
fn rejects_negated_comparison_equivalent_to_inverted_operator() {
    let error = parse_module_as_syntax_output(
        "module negated_comparison_boundary.\n\
         shape NonNegative(value) = value where not (value < 0).\n\
         shape ExplicitNonNegative(item) = item where item >= 0.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 NonNegative(value) -> value;\n\
                 ExplicitNonNegative(item) -> item;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect_err("negated less-than normalizes to greater-than-or-equal");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `ExplicitNonNegative` implies the earlier guard for subsuming alias `NonNegative`"
    ));
}

#[test]
fn rejects_negated_integer_equality_equivalent_to_inequality() {
    let error = parse_module_as_syntax_output(
        "module negated_integer_equality.\n\
         shape NotFive(value) = value where not (value == 5).\n\
         shape ExplicitNotFive(item) = item where item != 5.\n\
         pub classify(NotFive(value)) -> value;\n\
         classify(ExplicitNotFive(item)) -> item.\n",
    )
    .expect_err("integer inequality normalizes to ranges below and above the excluded value");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `ExplicitNotFive` implies the earlier guard for subsuming alias `NotFive`"
    ));
}

#[test]
fn rejects_negated_integer_inequality_equivalent_to_equality() {
    let error = parse_module_as_syntax_output(
        "module negated_integer_inequality.\n\
         shape Five(value) = value where not (value != 5).\n\
         shape ExplicitFive(item) = item where item == 5.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Five(value) -> value;\n\
                 ExplicitFive(item) -> item\n\
             }.\n",
    )
    .expect_err("negated integer inequality normalizes to equality");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `ExplicitFive` implies the earlier guard for subsuming alias `Five`"
    ));
}

#[test]
fn rejects_negated_variable_relation_equivalent_to_inverse_relation() {
    let error = parse_module_as_syntax_output(
        "module negated_variable_relation.\n\
         shape NotOrdered(left, right) = {left, right} where not (left < right).\n\
         shape DescendingOrEqual(first, second) = {first, second} where first >= second.\n\
         pub classify(NotOrdered(left, right)) -> left;\n\
         classify(DescendingOrEqual(first, second)) -> first.\n",
    )
    .expect_err("negated variable ordering normalizes to its inverse relation");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `DescendingOrEqual` implies the earlier guard for subsuming alias `NotOrdered`"
    ));
}

#[test]
fn rejects_negated_variable_equality_equivalent_to_inequality() {
    let error = parse_module_as_syntax_output(
        "module negated_variable_equality.\n\
         shape Distinct(left, right) = {left, right} where not (left == right).\n\
         shape ExplicitDistinct(first, second) = {first, second} where first != second.\n\
         pub classify(Distinct(left, right)) -> left;\n\
         classify(ExplicitDistinct(first, second)) -> first.\n",
    )
    .expect_err("negated variable equality normalizes to inequality");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `ExplicitDistinct` implies the earlier guard for subsuming alias `Distinct`"
    ));
}

#[test]
fn rejects_negated_reversed_comparison_after_operator_inversion() {
    let error = parse_module_as_syntax_output(
        "module negated_reversed_comparison.\n\
         shape Positive(value) = value where not (0 >= value).\n\
         shape ExplicitPositive(item) = item where item > 0.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 Positive(value) -> value;\n\
                 ExplicitPositive(item) -> item\n\
             }.\n",
    )
    .expect_err("negation precedes literal-on-left comparison reversal");

    assert!(format!("{error:?}").contains(
        "unreachable shape expansion: later guard for alias `ExplicitPositive` implies the earlier guard for subsuming alias `Positive`"
    ));
}

#[test]
fn accepts_inverted_comparison_that_does_not_imply_earlier_range() {
    let output = parse_module_as_syntax_output(
        "module useful_inverted_comparison.\n\
         shape NonNegative(value) = value where not (value < 0).\n\
         shape AtLeastNegativeOne(item) = item where item >= -1.\n\
         pub classify(input: Dynamic): Dynamic ->\n\
             case input {\n\
                 NonNegative(value) -> value;\n\
                 AtLeastNegativeOne(item) -> item;\n\
                 _ -> 0\n\
             }.\n",
    )
    .expect("a broader later range retains the value minus one");

    assert_eq!(output.declarations.len(), 3);
}
