use serde_json::json;

use crate::checked::{CheckedType, GeneratedBody};

pub(super) fn structured_capture(sequence: i64) -> GeneratedBody {
    let expected = sequence + 4;
    GeneratedBody::expression(
        CheckedType::Int,
        json!(expected),
        format!(
            concat!(
                "let prepared = Ready({sequence});\n",
                "let resumed = suspend_signal(prepared, 2);\n",
                "inspect(resumed)"
            ),
            sequence = sequence,
        ),
        expected.to_string(),
    )
    .with_prelude(concat!(
        "import std.vm.Process.\n\n",
        "pub type ReadyTag = Atom[\"ready\"].\n\n",
        "pub type StoppedTag = Atom[\"stopped\"].\n\n",
        "pub type Signal = {ReadyTag, Int} | StoppedTag.\n\n",
        "pub constructor Ready {\n",
        "    (value: Int): Signal ->\n",
        "        {ReadyTag, value}\n",
        "}.\n\n",
        "pub constructor Stopped {\n",
        "    (): Signal ->\n",
        "        StoppedTag\n",
        "}.\n\n",
        "advance(value: Int): Int ->\n",
        "    value + 1.\n\n",
        "suspend_signal(signal: Signal, offset: Int): Signal ->\n",
        "    let before =\n",
        "        case signal {\n",
        "            Ready(value) -> advance(value);\n",
        "            Stopped() -> 0\n",
        "        };\n",
        "    let _parked = Process.yield_now();\n",
        "    let next =\n",
        "        if {\n",
        "            before > 0 -> advance(before + offset);\n",
        "            true -> offset\n",
        "        };\n",
        "    if {\n",
        "        next > 0 -> Ready(next);\n",
        "        true -> Stopped()\n",
        "    }.\n\n",
        "inspect(signal: Signal): Int ->\n",
        "    case signal {\n",
        "        Ready(value) -> value;\n",
        "        Stopped() -> 0\n",
        "    }."
    ))
}
