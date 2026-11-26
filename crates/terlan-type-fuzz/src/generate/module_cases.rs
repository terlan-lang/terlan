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

pub(super) fn generic_trait_package(
    index: u32,
    sequence: i64,
) -> (GeneratedBody, Vec<GeneratedSource>) {
    let module = format!("type_fuzz.case{index:06}");
    let path = format!("src/type_fuzz/case{index:06}");
    let expected = sequence * 2 + 119;
    let body = GeneratedBody::expression(
        CheckedType::Int,
        json!(expected),
        format!(
            concat!(
                "inspect_left(LeftReady(LeftValue({sequence}))) + ",
                "inspect_left(LeftStopped()) + ",
                "inspect_right(RightReady(RightValue({right}))) + ",
                "inspect_right(RightStopped())"
            ),
            sequence = sequence,
            right = sequence + 1,
        ),
        expected.to_string(),
    )
    .with_prelude(format!(
        concat!(
            "import {module}.Contract.{{score_with}}.\n",
            "import {module}.Left.{{Ready as LeftReady, Stopped as LeftStopped, Value as LeftValue}}.\n",
            "import {module}.Right.{{Ready as RightReady, Stopped as RightStopped, Value as RightValue}}.\n\n",
            "import type {module}.Left.{{Event as LeftEvent}}.\n",
            "import type {module}.Right.{{Event as RightEvent}}.\n\n",
            "inspect_left(event: LeftEvent): Int ->\n",
            "    case event {{\n",
            "        LeftReady(value) -> score_with(value);\n",
            "        LeftStopped() -> 7\n",
            "    }}.\n\n",
            "inspect_right(event: RightEvent): Int ->\n",
            "    case event {{\n",
            "        RightReady(value) -> score_with(value);\n",
            "        RightStopped() -> 11\n",
            "    }}."
        ),
        module = module,
    ));
    let sources = vec![
        source(
            &path,
            "Contract",
            format!(
                concat!(
                    "module {module}.Contract.\n\n",
                    "pub trait Score[T] {{\n",
                    "    score(value: T): Int.\n",
                    "}}.\n\n",
                    "pub score_with[T](value: T)[Score[T]]: Int ->\n",
                    "    Score.score(value).\n",
                ),
                module = module,
            ),
        ),
        scored_provider(&path, &module, "Left", "left", 0),
        scored_provider(&path, &module, "Right", "right", 100),
    ];
    (body, sources)
}

fn scored_provider(
    path: &str,
    module: &str,
    provider: &str,
    tag_prefix: &str,
    increment: i64,
) -> GeneratedSource {
    source(
        path,
        provider,
        format!(
            concat!(
                "module {module}.{provider}.\n\n",
                "import type {module}.Contract.{{Score}}.\n\n",
                "pub type ReadyTag = Atom[\"{tag_prefix}_ready\"].\n\n",
                "pub type StoppedTag = Atom[\"{tag_prefix}_stopped\"].\n\n",
                "pub struct Value implements Score[Value] {{\n",
                "    amount: Int,\n",
                "}}.\n\n",
                "pub type Payload = Value.\n\n",
                "pub type Event = {{ReadyTag, Payload}} | StoppedTag.\n\n",
                "pub constructor Value {{\n",
                "    (amount: Int): Value ->\n",
                "        Value {{amount: amount}}\n",
                "}}.\n\n",
                "pub constructor Ready {{\n",
                "    (value: Payload): Event ->\n",
                "        {{ReadyTag, value}}\n",
                "}}.\n\n",
                "pub constructor Stopped {{\n",
                "    (): Event ->\n",
                "        StoppedTag\n",
                "}}.\n\n",
                "pub (value: Value) score(): Int ->\n",
                "    value.amount + {increment}.\n",
            ),
            module = module,
            provider = provider,
            tag_prefix = tag_prefix,
            increment = increment,
        ),
    )
}

fn source(root: &str, name: &str, source: String) -> GeneratedSource {
    GeneratedSource {
        path: PathBuf::from(format!("{root}/{name}.terl")),
        source,
    }
}
