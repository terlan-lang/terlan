use std::path::PathBuf;

use serde_json::json;

use crate::checked::{CheckedType, GeneratedBody};
use crate::GeneratedSource;

pub(super) fn generic_field_receiver(
    index: u32,
    sequence: i64,
) -> (GeneratedBody, Vec<GeneratedSource>) {
    let module = format!("type_fuzz.case{index:06}");
    let path = format!("src/type_fuzz/case{index:06}");
    let body = GeneratedBody::expression(
        CheckedType::Int,
        json!(sequence),
        format!(
            "let execution = Execution({sequence});\n\
             let state = State(Numbers.next({sequence}), execution);\n\
             let batch = Batch(state);\n\
             batch.latest_sequence()"
        ),
        sequence.to_string(),
    )
    .with_prelude(format!(
        "import {module}.{{Batch, Execution, Numbers, State}}."
    ));
    let sources = vec![
        source(
            &path,
            "Execution",
            format!(
                concat!(
                    "module {module}.Execution.\n\n",
                    "pub struct Execution {{\n",
                    "    sequence: Int,\n",
                    "}}.\n\n",
                    "pub constructor Execution {{\n",
                    "    (sequence: Int): Execution ->\n",
                    "        Execution {{sequence: sequence}}\n",
                    "}}.\n\n",
                    "pub (execution: Execution) latest_sequence(): Int ->\n",
                    "    execution.sequence.\n",
                ),
                module = module,
            ),
        ),
        source(
            &path,
            "Numbers",
            format!(
                concat!(
                    "module {module}.Numbers.\n\n",
                    "pub next(value: Int): Int ->\n",
                    "    value + 1.\n",
                ),
                module = module,
            ),
        ),
        source(
            &path,
            "State",
            format!(
                concat!(
                    "module {module}.State.\n\n",
                    "import type {module}.Execution.{{Execution}}.\n\n",
                    "pub struct State[StrategyState] {{\n",
                    "    strategy_state: StrategyState,\n",
                    "    execution: Execution,\n",
                    "}}.\n\n",
                    "pub constructor State[StrategyState] {{\n",
                    "    (strategy_state: StrategyState, execution: Execution): State[StrategyState] ->\n",
                    "        State {{strategy_state: strategy_state, execution: execution}}\n",
                    "}}.\n\n",
                    "pub (state: State[StrategyState]) execution_state(): Execution ->\n",
                    "    state.execution.\n",
                ),
                module = module,
            ),
        ),
        source(
            &path,
            "Batch",
            format!(
                concat!(
                    "module {module}.Batch.\n\n",
                    "import type {module}.State.{{State}}.\n\n",
                    "pub struct Batch[StrategyState] {{\n",
                    "    state: State[StrategyState],\n",
                    "}}.\n\n",
                    "pub constructor Batch[StrategyState] {{\n",
                    "    (state: State[StrategyState]): Batch[StrategyState] ->\n",
                    "        Batch {{state: state}}\n",
                    "}}.\n\n",
                    "pub (batch: Batch[StrategyState]) latest_sequence(): Int ->\n",
                    "    batch.state.execution_state().latest_sequence().\n",
                ),
                module = module,
            ),
        ),
    ];
    (body, sources)
}

fn source(root: &str, name: &str, source: String) -> GeneratedSource {
    GeneratedSource {
        path: PathBuf::from(format!("{root}/{name}.terl")),
        source,
    }
}
