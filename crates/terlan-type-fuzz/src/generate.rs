use serde_json::json;

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
        Oracle::None => "none",
    };
    let case_id = format!(
        "v1-s{:016x}-c{index:06}-d{:02}-o{oracle_name}",
        config.seed, config.max_depth
    );
    let module = format!("type_fuzz.Case{index:06}");
    let (expected_type, value, body) = family.render(number);
    let source = format!("module {module}.\n\n{body}\n");
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
            expected_type: expected_type.into(),
            oracle: config.oracle,
            case_timeout_ms: config.timeout_ms,
            rss_limit_bytes: config.rss_limit_bytes,
            cpu_limit_ms: config.cpu_limit_ms,
        },
        project,
        source,
        expected: TypedValue {
            schema: VALUE_SCHEMA,
            type_name: expected_type.into(),
            value,
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
    Scalar,
    LexicalProduct,
    Record,
    IfProductJoin,
    OptionJoin,
    NominalOptionJoin,
    ResultJoin,
    ClosedUnion,
}

impl Family {
    fn from_index(index: u32, max_depth: u8) -> Self {
        let family_count = match max_depth {
            1 => 1,
            2 => 3,
            _ => 8,
        };
        match index % family_count {
            0 => Self::Scalar,
            1 => Self::LexicalProduct,
            2 => Self::Record,
            3 => Self::IfProductJoin,
            4 => Self::OptionJoin,
            5 => Self::NominalOptionJoin,
            6 => Self::ResultJoin,
            _ => Self::ClosedUnion,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::LexicalProduct => "lexical_product",
            Self::Record => "record",
            Self::IfProductJoin => "if_product_join",
            Self::OptionJoin => "option_join",
            Self::NominalOptionJoin => "nominal_option_join",
            Self::ResultJoin => "result_join",
            Self::ClosedUnion => "closed_union",
        }
    }

    fn render(self, value: i64) -> (&'static str, serde_json::Value, String) {
        match self {
            Self::Scalar => ("Int", json!(value), format!("pub sample(): Int -> {value}.")),
            Self::LexicalProduct => (
                "{Int, Bool}",
                json!([value, true]),
                format!("pub sample(): {{Int, Bool}} ->\n    let item = {{{value}, true}};\n    item."),
            ),
            Self::Record => (
                "Pair",
                json!({"left": value, "right": value + 1}),
                format!("pub struct Pair {{ left: Int, right: Int }}.\n\npub sample(): Pair -> Pair {{ left: {value}, right: {} }}.", value + 1),
            ),
            Self::IfProductJoin => (
                "{Int, Bool}",
                json!([value, true]),
                format!("pub sample(): {{Int, Bool}} ->\n    let selected = if {{ false -> {{0, false}}; true -> {{{value}, true}} }};\n    selected."),
            ),
            Self::OptionJoin => (
                "Option[Int]",
                json!({"variant": "some", "payload": value}),
                format!("import std.core.Option.{{None, Some}}.\nimport type std.core.Option.{{Option}}.\n\npub sample(): Option[Int] ->\n    let selected = case true {{\n        false -> None;\n        true -> case false {{ false -> Some({value}); true -> None }}\n    }};\n    selected."),
            ),
            Self::NominalOptionJoin => (
                "Option[Evidence]",
                json!({"variant": "some", "payload": {"value": value}}),
                format!("import std.core.Option.{{None, Some}}.\nimport type std.core.Option.{{Option}}.\n\npub struct Evidence {{ value: Int }}.\n\npub sample(): Option[Evidence] ->\n    let evidence = case true {{\n        false -> None;\n        true -> case false {{ false -> Some(Evidence {{ value: {value} }}); true -> None }}\n    }};\n    evidence."),
            ),
            Self::ResultJoin => (
                "Result[Int, String]",
                json!({"variant": "ok", "payload": value}),
                format!("import std.core.Result.{{Err, Ok}}.\nimport type std.core.Result.{{Result}}.\n\npub sample(): Result[Int, String] ->\n    case true {{ false -> Err(\"rejected\"); true -> Ok({value}) }}."),
            ),
            Self::ClosedUnion => (
                "Choice",
                json!({"variant": "value", "payload": value}),
                format!("pub type ValueTag = Atom[\"value\"].\npub type EmptyTag = Atom[\"empty\"].\npub type Choice = {{ValueTag, Int}} | EmptyTag.\npub constructor Value {{ (value: Int): Choice -> {{ValueTag, value}} }}.\n\npub sample(): Choice -> Value({value})."),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(seed: u64, index: u32, depth: u8) -> GeneratedCase {
        generate(
            index,
            &GenerationConfig {
                seed,
                max_depth: depth,
                oracle: Oracle::None,
                timeout_ms: 2_000,
                rss_limit_bytes: 1024,
                cpu_limit_ms: 2_000,
                compiler_revision: "test",
            },
        )
    }

    #[test]
    fn generation_is_stable_and_covers_each_initial_family() {
        assert_eq!(case(17, 3, 8), case(17, 3, 8));
        let families = (0..8)
            .map(|index| case(1, index, 8).manifest.family)
            .collect::<Vec<_>>();
        assert_eq!(
            families,
            [
                "scalar",
                "lexical_product",
                "record",
                "if_product_join",
                "option_join",
                "nominal_option_join",
                "result_join",
                "closed_union"
            ]
        );
    }

    #[test]
    fn seed_and_depth_budgets_change_only_their_owned_dimensions() {
        assert_ne!(case(1, 0, 8).expected, case(2, 0, 8).expected);
        for index in 0..12 {
            assert_eq!(case(1, index, 1).manifest.family, "scalar");
        }
        assert_eq!(case(1, 3, 2).manifest.family, "scalar");
    }

    #[test]
    fn checked_oracle_is_encoded_in_identity_and_manifest() {
        let generated = generate(
            0,
            &GenerationConfig {
                seed: 1,
                max_depth: 8,
                oracle: Oracle::Check,
                timeout_ms: 2_000,
                rss_limit_bytes: 1024,
                cpu_limit_ms: 2_000,
                compiler_revision: "test",
            },
        );
        assert_eq!(generated.manifest.oracle, Oracle::Check);
        assert!(generated.manifest.case_id.ends_with("-ocheck"));
    }
}
