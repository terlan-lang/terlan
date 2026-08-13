use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::lower_syntax_module_output_to_core;

fn checked(source: &str) -> CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("parse termination fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

fn evidence<'a>(
    module: &'a CoreModule,
    function: &str,
    arity: usize,
) -> &'a CoreFunctionTerminationEvidence {
    module
        .termination
        .function(function, arity)
        .expect("function termination evidence")
}

#[test]
fn coreir_attaches_deterministic_nonrecursive_evidence() {
    let source = "module termination_leaf.\n\npub answer(value: Int): Int -> value + 1.\n";
    let first = checked(source);
    let second = checked(source);

    assert_eq!(first.termination, second.termination);
    assert_eq!(
        evidence(&first, "answer", 1).state,
        CoreTerminationState::Proven
    );
    assert_eq!(
        evidence(&first, "answer", 1).reason,
        CoreTerminationReason::NonRecursive
    );
    validate_core_termination_evidence(&first).expect("attached evidence validates");
}

#[test]
fn structural_list_recursion_proves_well_founded_descent() {
    let module = checked(
        "module termination_list.\n\n\
         pub length(values: List[Int]): Int ->\n\
             case values {\n\
                 [] -> 0;\n\
                 [_head | tail] -> 1 + length(tail)\n\
             }.\n",
    );
    let proof = evidence(&module, "length", 1);

    assert_eq!(proof.state, CoreTerminationState::Proven);
    assert_eq!(proof.reason, CoreTerminationReason::StructuralDescent);
    assert_eq!(proof.measure, vec![0]);
    assert!(proof
        .recursive_calls
        .iter()
        .all(|edge| { edge.argument_relations == vec![CoreDecreaseKind::Structural] }));
}

#[test]
fn guarded_integer_countdown_proves_but_unguarded_descent_does_not() {
    let guarded = checked(
        "module termination_countdown.\n\n\
         pub countdown(n: Int): Int ->\n\
             if {\n\
                 n > 0 -> countdown(n - 1);\n\
                 true -> 0\n\
             }.\n",
    );
    let unguarded = checked(
        "module termination_unguarded.\n\n\
         pub countdown(n: Int): Int -> countdown(n - 1).\n",
    );

    assert_eq!(
        evidence(&guarded, "countdown", 1).reason,
        CoreTerminationReason::GuardedIntegerDescent
    );
    assert_eq!(
        evidence(&unguarded, "countdown", 1).state,
        CoreTerminationState::Unproven
    );
    assert_eq!(
        evidence(&guarded, "countdown", 1).state,
        CoreTerminationState::Proven
    );
    guarded
        .termination
        .require_total("countdown", 1)
        .expect("proven countdown is admitted to a total context");
    assert!(unguarded
        .termination
        .require_total("countdown", 1)
        .unwrap_err()
        .context()
        .contains("termination.total_required"));
}

#[test]
fn high_arity_recursion_finds_an_independent_singleton_measure() {
    let module = checked(
        "module termination_high_arity.\n\n\
         pub descend(a: Int, b: Int, c: Int, d: Int, e: Int, f: Int, g: Int, h: Int, n: Int): Int ->\n\
             if {\n\
                 n > 0 -> descend(a + 1, b + 1, c + 1, d + 1, e + 1, f + 1, g + 1, h + 1, n - 1);\n\
                 true -> 0\n\
             }.\n",
    );
    let proof = evidence(&module, "descend", 9);

    assert_eq!(proof.state, CoreTerminationState::Proven);
    assert_eq!(proof.reason, CoreTerminationReason::GuardedIntegerDescent);
    assert_eq!(proof.measure, vec![8]);
}

#[test]
fn short_circuit_right_operands_preserve_tail_position() {
    let module = checked(
        "module termination_short_circuit.\n\n\
         pub all(n: Int): Bool ->\n\
             if { n > 0 -> true and all(n - 1); true -> true }.\n\
         pub any(n: Int): Bool ->\n\
             if { n > 0 -> false or any(n - 1); true -> false }.\n",
    );

    for name in ["all", "any"] {
        let proof = evidence(&module, name, 1);
        assert_eq!(proof.state, CoreTerminationState::Proven);
        assert!(proof.recursive_calls.iter().all(|edge| edge.tail_position));
    }
}

#[test]
fn lexicographic_and_mutual_size_change_components_prove() {
    let lexicographic = checked(
        "module termination_lexicographic.\n\n\
         pub descend(outer: Int, inner: Int): Int ->\n\
             if {\n\
                 outer > 0 -> descend(outer - 1, inner);\n\
                 inner > 0 -> descend(outer, inner - 1);\n\
                 true -> 0\n\
             }.\n",
    );
    let mutual = checked(
        "module termination_mutual.\n\n\
         pub even(n: Int): Bool ->\n\
             if { n > 0 -> odd(n - 1); true -> true }.\n\
         pub odd(n: Int): Bool ->\n\
             if { n > 0 -> even(n - 1); true -> false }.\n",
    );

    assert_eq!(
        evidence(&lexicographic, "descend", 2).reason,
        CoreTerminationReason::LexicographicDescent
    );
    for name in ["even", "odd"] {
        let proof = evidence(&mutual, name, 1);
        assert_eq!(proof.state, CoreTerminationState::Proven);
        assert_eq!(proof.reason, CoreTerminationReason::MutualSizeChange);
        assert_eq!(proof.component, vec!["even/1", "odd/1"]);
    }
}

#[test]
fn forged_or_stale_termination_evidence_is_a_loud_error() {
    let mut module =
        checked("module termination_forged.\n\npub loop(value: Int): Int -> loop(value).\n");
    let proof = module
        .termination
        .functions
        .iter_mut()
        .find(|proof| proof.function == "loop")
        .expect("loop evidence");
    proof.state = CoreTerminationState::Proven;
    proof.reason = CoreTerminationReason::StructuralDescent;
    proof.measure = vec![0];

    let error = validate_core_termination_evidence(&module).unwrap_err();
    assert_eq!(
        error.domain(),
        terlan_runtime_abi::ErrorDomain::CompilerPhase
    );
    assert_eq!(error.code(), "termination.evidence_invalid");
    assert_eq!(
        error.context(),
        "error[termination.evidence_invalid]: attached evidence does not match checked CoreIR"
    );
}

#[test]
fn typed_process_cycles_distinguish_productive_and_unproductive_persistence() {
    let finite = checked(
        "module termination_actor_finite.\n\n\
         import std.vm.Process.\n\n\
         pub worker(): Int -> Process.receive_int().\n",
    );
    let productive = checked(
        "module termination_actor_productive.\n\n\
         import std.vm.Process.\n\n\
         pub mailbox_loop(): Unit ->\n\
             (let _message = Process.receive_int(); mailbox_loop()).\n",
    );
    let unproductive = checked(
        "module termination_actor_unproductive.\n\n\
         import std.vm.Process.\n\n\
         pub busy_loop(): Unit ->\n\
             (busy_loop(); Process.send_int(1, 1)).\n",
    );

    assert_eq!(
        evidence(&productive, "mailbox_loop", 0).state,
        CoreTerminationState::ProductivePersistent
    );
    assert_eq!(
        evidence(&finite, "worker", 0).actor_behavior,
        CoreActorBehavior::FiniteWorker
    );
    assert_eq!(
        evidence(&productive, "mailbox_loop", 0).actor_behavior,
        CoreActorBehavior::Persistent
    );
    assert_eq!(
        evidence(&unproductive, "busy_loop", 0).state,
        CoreTerminationState::IntentionalPersistent
    );
}
