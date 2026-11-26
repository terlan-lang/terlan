use serde_json::json;

use crate::checked::{CheckedType, GeneratedBody};
use crate::{CaseManifest, GeneratedCase, Oracle, TypedValue, CASE_SCHEMA, VALUE_SCHEMA};

const GENERATOR: &str = "terlan.type-fuzz-generator.v1";

/// Stable inputs shared by every case in one deterministic generation run.
pub struct GenerationConfig<'a> {
    pub seed: u64,
    pub max_depth: u8,
    pub oracle: Oracle,
    pub timeout_ms: u64,
    pub rss_limit_bytes: u64,
    pub cpu_limit_ms: u64,
    pub compiler_revision: &'a str,
}

/// Generates one deterministic typed program and its replay manifest.
pub fn generate(index: u32, config: &GenerationConfig<'_>) -> GeneratedCase {
    let split_seed = split(config.seed, index);
    let number = (split_seed % 97) as i64 + 1;
    let family = Family::from_index(index, config.max_depth);
    let oracle_name = match config.oracle {
        Oracle::Check => "check",
        Oracle::Native => "native",
        Oracle::None => "none",
    };
    let case_id = format!(
        "v1-s{:016x}-c{index:06}-d{:02}-o{oracle_name}",
        config.seed, config.max_depth
    );
    let module = format!("type_fuzz.Case{index:06}");
    let rendered = family.render(number);
    let expected_type = rendered.checked_type.source_name();
    let source = format!("module {module}.\n\n{}\n", rendered.source());
    let project = format!(
        "[package]\nname = \"type-fuzz-case-{index:06}\"\nversion = \"0.0.0\"\n\n[build]\nsource_roots = [\".\"]\nartifact = \"library\"\n"
    );
    GeneratedCase {
        manifest: CaseManifest {
            schema: CASE_SCHEMA.into(),
            generator: GENERATOR.into(),
            compiler_version: env!("CARGO_PKG_VERSION").into(),
            compiler_revision: config.compiler_revision.into(),
            case_id,
            seed: config.seed,
            case_index: index,
            max_depth: config.max_depth,
            family: family.name().into(),
            expected_type: expected_type.clone(),
            oracle: config.oracle,
            case_timeout_ms: config.timeout_ms,
            rss_limit_bytes: config.rss_limit_bytes,
            cpu_limit_ms: config.cpu_limit_ms,
        },
        project,
        source,
        expected: TypedValue {
            schema: VALUE_SCHEMA,
            type_name: expected_type,
            value: rendered.value,
        },
    }
}

fn split(seed: u64, index: u32) -> u64 {
    let mut value = seed ^ (u64::from(index).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Clone, Copy)]
enum Family {
    UnitScalar,
    BoolScalar,
    IntScalar,
    FloatScalar,
    StringScalar,
    AtomScalar,
    LexicalProduct,
    Record,
    List,
    IfProductJoin,
    CaseProductJoin,
    OptionJoin,
    NominalOptionJoin,
    ResultJoin,
    NestedResultJoin,
    ClosedUnion,
    ClosedUnionEmpty,
}

impl Family {
    fn from_index(index: u32, max_depth: u8) -> Self {
        let family_count = match max_depth {
            1 => 6,
            2 => 10,
            _ => 17,
        };
        match index % family_count {
            0 => Self::UnitScalar,
            1 => Self::BoolScalar,
            2 => Self::IntScalar,
            3 => Self::FloatScalar,
            4 => Self::StringScalar,
            5 => Self::AtomScalar,
            6 => Self::LexicalProduct,
            7 => Self::Record,
            8 => Self::List,
            9 => Self::IfProductJoin,
            10 => Self::CaseProductJoin,
            11 => Self::OptionJoin,
            12 => Self::NominalOptionJoin,
            13 => Self::ResultJoin,
            14 => Self::NestedResultJoin,
            15 => Self::ClosedUnion,
            _ => Self::ClosedUnionEmpty,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::UnitScalar => "scalar_unit",
            Self::BoolScalar => "scalar_bool",
            Self::IntScalar => "scalar_int",
            Self::FloatScalar => "scalar_float",
            Self::StringScalar => "scalar_string",
            Self::AtomScalar => "scalar_atom",
            Self::LexicalProduct => "lexical_product",
            Self::Record => "record",
            Self::List => "list",
            Self::IfProductJoin => "if_product_join",
            Self::CaseProductJoin => "case_product_join",
            Self::OptionJoin => "option_join",
            Self::NominalOptionJoin => "nominal_option_join",
            Self::ResultJoin => "result_join",
            Self::NestedResultJoin => "nested_result_join",
            Self::ClosedUnion => "closed_union",
            Self::ClosedUnionEmpty => "closed_union_empty",
        }
    }

    fn render(self, value: i64) -> GeneratedBody {
        match self {
            Self::UnitScalar => generated(
                CheckedType::Unit,
                serde_json::Value::Null,
                "Unit".into(),
            ),
            Self::BoolScalar => generated(
                CheckedType::Bool,
                json!(value % 2 == 0),
                (value % 2 == 0).to_string(),
            ),
            Self::IntScalar => generated(
                CheckedType::Int,
                json!(value),
                value.to_string(),
            ),
            Self::FloatScalar => generated(
                CheckedType::Float,
                json!(value as f64 + 0.5),
                format!("{value}.5"),
            ),
            Self::StringScalar => generated(
                CheckedType::String,
                json!(format!("value-{value}")),
                format!("\"value-{value}\""),
            ),
            Self::AtomScalar => generated(
                CheckedType::Atom("ready"),
                json!({"atom": "ready"}),
                "Atom[\"ready\"]".into(),
            ),
            Self::LexicalProduct => generated_with_expected(
                product_type(),
                json!([value, true]),
                format!("let item = {{{value}, true}};\nitem"),
                format!("{{{value}, true}}"),
            ),
            Self::Record => generated(
                CheckedType::Nominal("Pair"),
                json!({"left": value, "right": value + 1}),
                format!("Pair {{ left: {value}, right: {} }}", value + 1),
            )
            .with_prelude("pub struct Pair { left: Int, right: Int }."),
            Self::List => generated(
                CheckedType::list(CheckedType::Int),
                json!([value, value + 1]),
                format!("[{value}, {}]", value + 1),
            )
            .with_prelude("import type std.collections.List."),
            Self::IfProductJoin => generated_with_expected(
                product_type(),
                json!([value, true]),
                format!("let selected = if {{ false -> {{0, false}}; true -> {{{value}, true}} }};\nselected"),
                format!("{{{value}, true}}"),
            ),
            Self::CaseProductJoin => generated_with_expected(
                product_type(),
                json!([value, true]),
                format!("case {{{value}, true}} {{\n    {{left, right}} -> {{left, right}}\n}}"),
                format!("{{{value}, true}}"),
            ),
            Self::OptionJoin => generated_with_expected(
                CheckedType::option(CheckedType::Int),
                json!({"variant": "some", "payload": value}),
                format!("let selected = case true {{\n    false -> None;\n    true -> case false {{ false -> Some({value}); true -> None }}\n}};\nselected"),
                format!("Some({value})"),
            )
            .with_prelude(option_prelude()),
            Self::NominalOptionJoin => generated_with_expected(
                CheckedType::option(CheckedType::Nominal("Evidence")),
                json!({"variant": "some", "payload": {"value": value}}),
                format!("let evidence = case true {{\n    false -> None;\n    true -> case false {{ false -> Some(Evidence {{ value: {value} }}); true -> None }}\n}};\nevidence"),
                format!("Some(Evidence {{ value: {value} }})"),
            )
            .with_prelude(format!(
                "{}\n\npub struct Evidence {{ value: Int }}.",
                option_prelude()
            )),
            Self::ResultJoin => generated_with_expected(
                CheckedType::result(CheckedType::Int, CheckedType::String),
                json!({"variant": "ok", "payload": value}),
                format!("case true {{ false -> Err(\"rejected\"); true -> Ok({value}) }}"),
                format!("Ok({value})"),
            )
            .with_prelude(result_prelude()),
            Self::NestedResultJoin => generated_with_expected(
                CheckedType::result(CheckedType::Int, CheckedType::String),
                json!({"variant": "ok", "payload": value}),
                format!("let selected = case true {{\n    false -> Err(\"outer\");\n    true -> case false {{ false -> Ok({value}); true -> Err(\"inner\") }}\n}};\nselected"),
                format!("Ok({value})"),
            )
            .with_prelude(result_prelude()),
            Self::ClosedUnion => generated(
                CheckedType::Nominal("Choice"),
                json!({"variant": "value", "payload": value}),
                format!("Value({value})"),
            )
            .with_prelude(choice_declarations()),
            Self::ClosedUnionEmpty => generated(
                CheckedType::Nominal("Choice"),
                json!({"variant": "empty"}),
                "Empty()".into(),
            )
            .with_prelude(choice_declarations()),
        }
    }
}

fn product_type() -> CheckedType {
    CheckedType::Product(vec![CheckedType::Int, CheckedType::Bool])
}

fn generated(checked_type: CheckedType, value: serde_json::Value, source: String) -> GeneratedBody {
    let expected = source.clone();
    GeneratedBody::expression(checked_type, value, source, expected)
}

fn generated_with_expected(
    checked_type: CheckedType,
    value: serde_json::Value,
    source: String,
    expected: String,
) -> GeneratedBody {
    GeneratedBody::expression(checked_type, value, source, expected)
}

fn option_prelude() -> &'static str {
    "import std.core.Option.{None, Some}.\nimport type std.core.Option.{Option}."
}

fn result_prelude() -> &'static str {
    "import std.core.Result.{Err, Ok}.\nimport type std.core.Result.{Result}."
}

fn choice_declarations() -> &'static str {
    concat!(
        "pub type ValueTag = Atom[\"value\"].\n",
        "pub type EmptyTag = Atom[\"empty\"].\n",
        "pub type Choice = {ValueTag, Int} | EmptyTag.\n",
        "pub constructor Value { (value: Int): Choice -> {ValueTag, value} }.\n",
        "pub constructor Empty { (): Choice -> EmptyTag }."
    )
}

#[cfg(test)]
#[path = "generate_test.rs"]
mod tests;
