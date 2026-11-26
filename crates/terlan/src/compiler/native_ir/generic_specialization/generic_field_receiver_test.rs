use super::super::source_constructor_test::check_sources;

/// A projected generic field must retain its concrete receiver arguments.
#[test]
fn generic_field_receiver_chain_specializes_and_executes() {
    check_sources(&[
        r#"
module tf4.Check.
import tf4.Batch.{Batch}.
import tf4.Execution.{Execution}.
import tf4.State.{State}.
pub check(): Bool ->
    let execution = Execution(7);
    let state = State(11, execution);
    let batch = Batch(state);
    batch.latest_sequence() == 7.
"#,
        r#"
module tf4.Execution.
pub struct Execution { sequence: Int }.
pub constructor Execution {
    (sequence: Int): Execution -> Execution {sequence: sequence}
}.
pub (execution: Execution) latest_sequence(): Int -> execution.sequence.
"#,
        r#"
module tf4.State.
import type tf4.Execution.{Execution}.
pub struct State[StrategyState] {
    strategy_state: StrategyState,
    execution: Execution,
}.
pub constructor State[StrategyState] {
    (strategy_state: StrategyState, execution: Execution): State[StrategyState] ->
        State {strategy_state: strategy_state, execution: execution}
}.
pub (state: State[StrategyState]) execution_state(): Execution -> state.execution.
"#,
        r#"
module tf4.Batch.
import type tf4.State.{State}.
pub struct Batch[StrategyState] { state: State[StrategyState] }.
pub constructor Batch[StrategyState] {
    (state: State[StrategyState]): Batch[StrategyState] -> Batch {state: state}
}.
pub (batch: Batch[StrategyState]) latest_sequence(): Int ->
    batch.state.execution_state().latest_sequence().
"#,
    ]);
}
