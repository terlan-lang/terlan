use std::collections::HashMap;

use crate::terlan_hir::{
    resolve_syntax_module_output, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface,
};
use crate::terlan_syntax::{
    formatter::format_source_module, parse_interface_module_as_syntax_output,
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprKind,
};

use super::{
    lower_syntax_module_output_to_core, prepare_syntax_constants, type_check_syntax_module_output,
};

const COMPLETE_FIXTURE: &str = r#"
module lifecycle.demo.

const BASE: Int = 40.
const add_two(value: Int): Int -> value + 2.
pub const ANSWER: Int = add_two(BASE).

pub type Status: Int = OK = 200 | MISSING = 404.

pub trait HasCode {
    const CODE: Int.
}.

pub impl HasCode for Int {
    const CODE = ANSWER.
}.

pub trait DefaultCode {
    const CODE: Int = ANSWER.
}.

pub impl DefaultCode for Bool {
}.

pub answer(): {Int, Int, Int, Status} ->
    {ANSWER, HasCode[Int].CODE, DefaultCode[Bool].CODE, Status.OK}.
"#;

#[test]
fn constants_are_evaluated_substituted_and_absent_from_runtime_core() {
    let syntax = parse_module_as_syntax_output(COMPLETE_FIXTURE).expect("parse constants");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");

    let (prepared, const_diagnostics) = prepare_syntax_constants(&syntax);
    assert!(const_diagnostics.is_empty(), "{const_diagnostics:#?}");
    let body = prepared
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, clauses, .. } if name == "answer" => {
                clauses.first().map(|clause| &clause.body)
            }
            _ => None,
        })
        .expect("answer body");
    assert_eq!(body.kind, SyntaxExprKind::Tuple);
    assert_eq!(body.children[0].text.as_deref(), Some("42"));
    assert_eq!(body.children[1].text.as_deref(), Some("42"));
    assert_eq!(body.children[2].text.as_deref(), Some("42"));
    assert_eq!(body.children[3].text.as_deref(), Some("200"));

    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let answer = core
        .functions
        .iter()
        .find(|function| function.name == "answer")
        .expect("core answer");
    assert!(!format!("{answer:#?}").contains("ANSWER"));
    assert!(!format!("{answer:#?}").contains("HasCode"));
    assert!(!format!("{answer:#?}").contains("DefaultCode"));
    assert!(!format!("{answer:#?}").contains("Status.OK"));
}

#[test]
fn public_constant_round_trips_through_the_interface() {
    let syntax = parse_module_as_syntax_output(COMPLETE_FIXTURE).expect("parse constants");
    let interface = syntax_module_output_to_interface(&syntax);
    let text = interface.to_terlan_interface_text();
    assert!(text.contains("pub const ANSWER: Int = 42."), "{text}");
    assert!(text.contains("pub impl HasCode for Int"), "{text}");
    assert!(text.contains("const CODE = 42."), "{text}");
    let parsed = parse_interface_module_as_syntax_output(&text).expect("parse generated interface");
    assert!(parsed.declarations.iter().any(|declaration| matches!(
        &declaration.payload,
        SyntaxDeclarationPayload::Constant { name, .. } if name == "ANSWER"
    )));
    let reparsed_interface = syntax_module_output_to_interface(&parsed);
    assert_eq!(
        reparsed_interface.associated_constants["HasCode[Int].CODE"].value_text,
        "42"
    );
    assert_eq!(
        reparsed_interface.associated_constants["DefaultCode[Bool].CODE"].value_text,
        "42"
    );

    let interfaces = HashMap::from([("lifecycle.demo".to_string(), reparsed_interface)]);
    let consumer = parse_module_as_syntax_output(
        r#"
module lifecycle.trait_consumer.
import lifecycle.demo.{HasCode}.
pub answer(): Int -> HasCode[Int].CODE.
"#,
    )
    .expect("parse associated constant consumer");
    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn imported_aliased_qualified_and_wildcard_constants_are_substituted() {
    let provider = parse_module_as_syntax_output(
        r#"
module lifecycle.values.
const BASE: Int = 40.
pub const add_two(value: Int): Int -> value + 2.
pub const ANSWER: Int = add_two(BASE).
"#,
    )
    .expect("parse constant provider");
    let interface = syntax_module_output_to_interface(&provider);
    let interfaces = HashMap::from([(provider.module_name.clone(), interface)]);
    let consumer = parse_module_as_syntax_output(
        r#"
module lifecycle.consumer.
import lifecycle.values.{ANSWER as IMPORTED, add_two}.
pub const LOCAL: Int = add_two(IMPORTED).
pub answer(): {Int, Int, Int} -> {IMPORTED, lifecycle.values.ANSWER, LOCAL}.
pub matches(value: Int): Bool -> case value { IMPORTED -> true; _ -> false }.
"#,
    )
    .expect("parse constant consumer");
    let (prepared, diagnostics) =
        super::prepare_syntax_constants_with_interfaces(&consumer, &interfaces);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let resolved = resolve_syntax_module_output_with_interfaces(&prepared, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&prepared, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let body = prepared
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, clauses, .. } if name == "answer" => {
                clauses.first().map(|clause| &clause.body)
            }
            _ => None,
        })
        .expect("answer body");
    assert_eq!(
        body.children
            .iter()
            .map(|child| child.text.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("42"), Some("42"), Some("44")]
    );
    let matches = prepared
        .declarations
        .iter()
        .find(|declaration| {
            matches!(
                &declaration.payload,
                SyntaxDeclarationPayload::Function { name, .. } if name == "matches"
            )
        })
        .expect("constant pattern consumer");
    let rendered = format!("{matches:#?}");
    assert!(!rendered.contains("IMPORTED"), "{rendered}");
    assert!(rendered.contains("42"), "{rendered}");
}

#[test]
fn exported_value_changes_invalidate_interfaces_and_imported_pattern_analysis() {
    fn provider(value: i32) -> crate::terlan_hir::ModuleInterface {
        let syntax = parse_module_as_syntax_output(&format!(
            r#"
module lifecycle.cache_provider.
pub const MATCH_VALUE: Int = {value}.
pub const bump(value: Int): Int -> value + MATCH_VALUE.
pub type Code[const VALUE: Int]: Int = OK = MATCH_VALUE.
"#
        ))
        .expect("parse cache provider");
        syntax_module_output_to_interface(&syntax)
    }

    let old = provider(42);
    let new = provider(43);
    assert_ne!(
        old.constants["MATCH_VALUE"].fingerprint,
        new.constants["MATCH_VALUE"].fingerprint
    );
    assert_ne!(
        old.to_terlan_interface_text(),
        new.to_terlan_interface_text()
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module lifecycle.cache_consumer.
import lifecycle.cache_provider.{MATCH_VALUE as EXPECTED}.
pub classify(value: Int): Int ->
    case value {
        EXPECTED -> 1;
        _ -> 0
    }.
"#,
    )
    .expect("parse imported constant pattern");

    let prepare = |interface| {
        let interfaces = HashMap::from([("lifecycle.cache_provider".to_string(), interface)]);
        let (prepared, diagnostics) =
            super::prepare_syntax_constants_with_interfaces(&consumer, &interfaces);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        format!("{prepared:#?}")
    };
    let old_prepared = prepare(old);
    let new_prepared = prepare(new);
    assert!(old_prepared.contains("42"), "{old_prepared}");
    assert!(new_prepared.contains("43"), "{new_prepared}");
    assert_ne!(old_prepared, new_prepared);
}

#[test]
fn formatter_and_const_generic_surface_round_trip() {
    let source = r#"
module lifecycle.format.
pub const LIMIT: Int = 8.
pub type Buffer[const SIZE: Int, T] = {T}.
pub const twice(value: Int): Int -> value * 2.
"#;
    let formatted = format_source_module(source).expect("format constants");
    assert!(formatted.contains("pub const LIMIT: Int = 8."));
    assert!(formatted.contains("const SIZE: Int"));
    parse_module_as_syntax_output(&formatted).expect("parse formatted constants");
}

#[test]
fn const_generic_arguments_are_kind_checked_and_substituted() {
    let syntax = parse_module_as_syntax_output(
        r#"
module lifecycle.const_generics.
const SIZE: Int = 1.
const ENABLED: Bool = true.
const TAG: Atom = Atom["packet"].
const double(value: Int): Int -> value * 2.
type Buffer[const N: Int, T] = FixedArray[N, T].
type Switch[const ON: Bool] = Bool.
type Tagged[const NAME: Atom] = Atom.
pub int_buffer(): Buffer[double(SIZE), Int] -> #[1, 2].
pub switch(): Switch[ENABLED] -> true.
pub tagged(): Tagged[TAG] -> Atom["packet"].
"#,
    )
    .expect("parse const generic fixture");
    let (prepared, diagnostics) = prepare_syntax_constants(&syntax);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let type_diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(type_diagnostics.is_empty(), "{type_diagnostics:#?}");
    let annotations = prepared
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { return_type, .. } => {
                Some(return_type.text.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        annotations,
        vec!["Buffer[2, Int]", "Switch[true]", "Tagged[Atom[\"packet\"]]"]
    );
}

#[test]
fn const_generic_arguments_reject_wrong_kinds_and_inline_arithmetic() {
    let cases = [
        (
            "type Buffer[const SIZE: Int] = Int.\npub bad(): Buffer[true] -> 1.",
            "CONST_GENERIC_KIND_MISMATCH",
        ),
        (
            "type Buffer[const SIZE: Int] = Int.\npub bad(): Buffer[1 + 1] -> 1.",
            "INVALID_CONST_GENERIC_ARGUMENT",
        ),
        (
            "type Flag[const ON: Bool] = Bool.\npub bad(): Flag[\"yes\"] -> true.",
            "CONST_GENERIC_KIND_MISMATCH",
        ),
    ];
    for (body, expected) in cases {
        let syntax = parse_module_as_syntax_output(&format!(
            "module lifecycle.bad_const_generic.\n{body}\n"
        ))
        .expect("negative const generic fixture parses");
        let (_, diagnostics) = prepare_syntax_constants(&syntax);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "expected `{expected}` in {diagnostics:#?}"
        );
    }
}

#[test]
fn const_generic_callable_arguments_and_fixed_array_identity_typecheck() {
    let source = r#"
module lifecycle.const_callable.
const COUNT: Int = 2.
pub first[const SIZE: Int](values: FixedArray[SIZE, Int]): Int -> values[0].
pub run(): Int -> first[COUNT](#[10, 20]).
pub run_inferred(): Int -> first(#[10, 20]).
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse const generic call");
    let (prepared, prepare_diagnostics) = prepare_syntax_constants(&syntax);
    assert!(prepare_diagnostics.is_empty(), "{prepare_diagnostics:#?}");
    let call_type_arg =
        prepared
            .declarations
            .iter()
            .find_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Function { name, clauses, .. } if name == "run" => {
                    clauses
                        .first()
                        .and_then(|clause| clause.body.type_args.first())
                        .map(|argument| argument.text.as_str())
                }
                _ => None,
            });
    assert_eq!(call_type_arg, Some("2"));
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");

    let mismatch = parse_module_as_syntax_output(
        r#"
module lifecycle.const_identity.
type Buffer[const SIZE: Int] = FixedArray[SIZE, Int].
pub bad(): Buffer[2] -> #[1].
"#,
    )
    .expect("parse const identity mismatch");
    let resolved = resolve_syntax_module_output(&mismatch).module;
    let diagnostics = type_check_syntax_module_output(&mismatch, &resolved);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("FixedArray[2, Int]")),
        "{diagnostics:#?}"
    );
}

#[test]
fn valued_unions_lower_checked_parsing_and_closed_patterns() {
    let source = r#"
module lifecycle.valued_union.
pub type Status: Int = OK = 200 | MISSING = 404.
pub parse(value: Int): Status -> Status.parse(value).
pub classify(value: Status): Int ->
    case value {
        Status.OK -> 1;
        Status.MISSING -> 2
    }.
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse valued union fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let (prepared, diagnostics) = prepare_syntax_constants(&syntax);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let parse_body = prepared
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, clauses, .. } if name == "parse" => {
                clauses.first().map(|clause| &clause.body)
            }
            _ => None,
        })
        .expect("parse body");
    assert_eq!(parse_body.kind, SyntaxExprKind::Case);
    assert_eq!(parse_body.clauses.len(), 3);

    let non_exhaustive = parse_module_as_syntax_output(
        r#"
module lifecycle.non_exhaustive.
type Status: Int = OK = 200 | MISSING = 404.
pub classify(value: Status): Int -> case value { Status.OK -> 1 }.
"#,
    )
    .expect("parse non-exhaustive valued union");
    let (_, diagnostics) = prepare_syntax_constants(&non_exhaustive);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("NON_EXHAUSTIVE_VALUED_UNION")),
        "{diagnostics:#?}"
    );

    let implicit = parse_module_as_syntax_output(
        "module lifecycle.implicit_union.\ntype Status: Int = OK = 200.\npub bad(): Status -> 200.\n",
    )
    .expect("parse implicit union conversion");
    let (_, diagnostics) = prepare_syntax_constants(&implicit);
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("IMPLICIT_VALUED_UNION_CONVERSION")),
        "{diagnostics:#?}"
    );
}

#[test]
fn imported_valued_union_arms_and_generic_trait_constants_substitute() {
    let provider = parse_module_as_syntax_output(
        r#"
module lifecycle.nominal_provider.
pub type Status: Int = OK = 200 | NOT_FOUND = 404.
pub trait HasCode[T] { const CODE: Int. }.
pub impl HasCode[Int] for Int { const CODE = 200. }.
"#,
    )
    .expect("parse nominal provider");
    let interface = syntax_module_output_to_interface(&provider);
    let interfaces = HashMap::from([("lifecycle.nominal_provider".to_string(), interface)]);
    let consumer = parse_module_as_syntax_output(
        r#"
module lifecycle.nominal_consumer.
import lifecycle.nominal_provider.{Status, HasCode}.
pub classify(value: Status): Int ->
    case value {
        Status.OK -> HasCode[Int].CODE;
        Status.NOT_FOUND -> 0
    }.
"#,
    )
    .expect("parse nominal consumer");
    let (prepared, diagnostics) =
        super::prepare_syntax_constants_with_interfaces(&consumer, &interfaces);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let resolved = resolve_syntax_module_output_with_interfaces(&prepared, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&prepared, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let text = format!("{prepared:#?}");
    assert!(!text.contains("HasCode[Int].CODE"), "{text}");
    assert!(text.contains("200"), "{text}");
}

#[test]
fn constants_are_rejected_in_non_value_metadata_and_annotations() {
    let annotation = parse_module_as_syntax_output(
        "module lifecycle.annotation_constant.\nconst LIMIT: Int = 2.\npub bad(value: LIMIT): Int -> value.\n",
    )
    .expect("constant-like annotation parses structurally");
    let (_, diagnostics) = prepare_syntax_constants(&annotation);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("CONSTANT_FORBIDDEN_CONTEXT")),
        "{diagnostics:#?}"
    );

    let config = parse_module_as_syntax_output(
        "module lifecycle.config_constant.\nconst LIMIT: Int = 2.\ntarget vm { threads: LIMIT }.\n",
    )
    .expect("config constant fixture parses");
    let (_, diagnostics) = prepare_syntax_constants(&config);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("CONSTANT_FORBIDDEN_CONTEXT")),
        "{diagnostics:#?}"
    );
}

#[test]
fn evaluator_limits_and_runtime_capabilities_fail_closed() {
    let cases = [
        (
            "const recurse(value: Int): Int -> recurse(value).\nconst BAD: Int = recurse(1).",
            "CONST_EVALUATOR_EXHAUSTED",
        ),
        (
            "const SECRET: String = env(\"TERLAN_SECRET\").",
            "CONST_FORBIDDEN_EFFECT",
        ),
        (
            "const HANDLE: Int = NativeBoundary.open().",
            "CONST_FORBIDDEN_EFFECT",
        ),
    ];
    for (body, expected) in cases {
        let syntax = parse_module_as_syntax_output(&format!(
            "module lifecycle.capability_reject.\n{body}\n"
        ))
        .expect("capability rejection fixture parses");
        let (_, diagnostics) = prepare_syntax_constants(&syntax);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "expected `{expected}` in {diagnostics:#?}"
        );
    }
}

#[test]
fn parameter_and_constructor_defaults_use_the_typed_const_evaluator() {
    let source = r#"
module lifecycle.const_defaults.
const BASE: Int = 20.
const twice(value: Int): Int -> value * 2.
pub choose(value: Int = twice(BASE)): Int -> value.
pub constructor Box {
    (value: Int = twice(BASE)): Int -> value
}.
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse const defaults");
    let (prepared, prepare_diagnostics) = prepare_syntax_constants(&syntax);
    assert!(prepare_diagnostics.is_empty(), "{prepare_diagnostics:#?}");
    let defaults = prepared
        .declarations
        .iter()
        .flat_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { params, .. } => params
                .iter()
                .filter_map(|param| param.default.as_ref())
                .collect::<Vec<_>>(),
            SyntaxDeclarationPayload::Constructor { clauses, .. } => clauses
                .iter()
                .flat_map(|clause| clause.params.iter())
                .filter_map(|param| param.default.as_ref())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    assert_eq!(defaults.len(), 2);
    assert!(defaults.iter().all(
        |default| default.kind == SyntaxExprKind::Int && default.text.as_deref() == Some("40")
    ));
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn immutable_nominal_and_aggregate_constants_are_semantically_substituted() {
    let syntax = parse_module_as_syntax_output(
        r#"
module lifecycle.aggregate_constants.
pub struct Settings { port: Int }.
const BASE: Int = 40.
pub const SETTINGS: Settings = Settings {port: BASE + 2}.
pub const VALUES: {Int, List[Int]} = {BASE, [1, 2]}.
pub const LOOKUP: Map[String, Int] = {answer: 42}.
pub const PORT: Int = SETTINGS.port.
pub const FIRST: Int = (VALUES)[0].
"#,
    )
    .expect("parse nominal aggregate constants");
    let (prepared, diagnostics) = prepare_syntax_constants(&syntax);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let rendered = format!("{prepared:#?}");
    assert!(rendered.contains("RecordConstruct"), "{rendered}");
    assert!(rendered.contains("42"), "{rendered}");

    let interface = syntax_module_output_to_interface(&syntax).to_terlan_interface_text();
    assert!(
        interface.contains("pub const SETTINGS: Settings = Settings {port: 42}."),
        "{interface}"
    );
    parse_interface_module_as_syntax_output(&interface).expect("aggregate interface round trip");
}

#[test]
fn valued_union_and_trait_associated_constants_are_compile_time_values() {
    let simple = parse_module_as_syntax_output(
        "module lifecycle.Simple.\npub type Status: Int = OK = 200 | NOT_FOUND = 404.\npub status(): Status -> Status.OK.\n",
    )
    .expect("parse simple valued union");
    let (_, simple_diagnostics) = prepare_syntax_constants(&simple);
    assert!(
        simple_diagnostics.is_empty(),
        "{simple_diagnostics:#?}\nsimple={simple:#?}"
    );
    let source = r#"
module lifecycle.Nominal.
pub type Status: Int = OK = 200 | NOT_FOUND = 404.
trait HasCode[T] { const CODE: Int = 500. }.
impl HasCode[Int] for Int { const CODE = 200. }.
const ASSOCIATED: Int = HasCode[Int].CODE.
pub status(): Status -> Status.OK.
pub code(): Int -> ASSOCIATED.
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse nominal constants");
    let (expanded, macro_diagnostics) = super::expand_syntax_raw_macros(syntax.clone());
    assert!(macro_diagnostics.is_empty(), "{macro_diagnostics:#?}");
    let (_, expanded_diagnostics) = prepare_syntax_constants(&expanded);
    assert!(
        expanded_diagnostics.is_empty(),
        "{expanded_diagnostics:#?}\nexpanded={expanded:#?}"
    );
    let (prepared, diagnostics) = prepare_syntax_constants(&syntax);
    assert!(
        diagnostics.is_empty(),
        "{diagnostics:#?}\nsyntax={syntax:#?}\nprepared={prepared:#?}"
    );
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}

#[test]
fn constant_failures_have_stable_specific_diagnostics() {
    let cases = [
        ("const A: Int = B.\nconst B: Int = A.", "CONST_CYCLE"),
        (
            "const BAD: Int = runtime_value().",
            "CONST_FORBIDDEN_EFFECT",
        ),
        (
            "type Code: Int = OK = 1 | ALSO_OK = 1.",
            "DUPLICATE_VALUED_UNION_VALUE",
        ),
        (
            "trait Required { const VALUE: Int. }.\nimpl Required for Int { value(): Int -> 1. }.",
            "MISSING_TRAIT_CONSTANT",
        ),
    ];
    for (body, expected) in cases {
        let source = format!("module lifecycle.bad.\n{body}\n");
        let syntax = parse_module_as_syntax_output(&source).expect("negative fixture parses");
        let (_, diagnostics) = prepare_syntax_constants(&syntax);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "expected `{expected}` in {diagnostics:#?}"
        );
    }
}

#[test]
fn naming_and_const_kind_rejections_are_parser_stable() {
    let bad_name = parse_module_as_syntax_output("module bad.name.\nconst Mixed: Int = 1.\n")
        .expect_err("mixed constant name must fail");
    assert!(format!("{bad_name:?}").contains("SCREAMING_SNAKE_CASE"));

    let bad_kind = parse_module_as_syntax_output(
        "module bad.kind.\ntype Buffer[const SIZE: Float, T] = {T}.\n",
    )
    .expect_err("float const kind must fail");
    assert!(format!("{bad_kind:?}").contains("Int, Bool, or Atom"));
}

#[test]
fn deferred_constant_like_declarations_and_runtime_reflection_are_rejected() {
    let rejected_syntax = [
        "module bad.local.\npub run(): Int -> let const LOCAL: Int = 1; LOCAL.\n",
        "module bad.lazy.\nlazy VALUE: Int = 1.\n",
        "module bad.enum.\nenum Status { OK }.\n",
        "module bad.qualified.\nconst Status.OK: Int = 200.\n",
        "module bad.mut.\npub run(): Int -> let mut value = 1; value.\n",
    ];
    for source in rejected_syntax {
        assert!(
            parse_module_as_syntax_output(source).is_err(),
            "deferred declaration unexpectedly parsed: {source}"
        );
    }

    let static_metadata = parse_module_as_syntax_output(
        "module lifecycle.static_metadata.\nstatic VALUE: Int = 1.\n",
    )
    .expect("static remains tooling configuration metadata");
    assert!(static_metadata.declarations.iter().all(|declaration| {
        matches!(declaration.payload, SyntaxDeclarationPayload::Config { .. })
    }));

    let reflection = parse_module_as_syntax_output(
        "module bad.reflection.\npub run(): Dynamic -> constants().\n",
    )
    .expect("ordinary unknown call parses");
    let (_, diagnostics) = prepare_syntax_constants(&reflection);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("constants")),
        "runtime constant reflection must not resolve: {diagnostics:#?}"
    );
}

#[test]
fn public_value_and_evaluator_fingerprints_invalidate_dependent_analysis() {
    fn provider(value: i64, offset: i64) -> crate::terlan_hir::ModuleInterface {
        let source = format!(
            "module lifecycle.cache.\n\
             pub const OFFSET: Int = {offset}.\n\
             pub const add_offset(value: Int): Int -> value + OFFSET.\n\
             pub const ANSWER: Int = add_offset({value}).\n"
        );
        let syntax = parse_module_as_syntax_output(&source).expect("parse cache provider");
        syntax_module_output_to_interface(&syntax)
    }

    let before = provider(40, 2);
    let after_value = provider(41, 2);
    let changed_evaluator = parse_module_as_syntax_output(
        "module lifecycle.cache.\n\
         pub const OFFSET: Int = 2.\n\
         pub const add_offset(value: Int): Int -> value + OFFSET + 0.\n\
         pub const ANSWER: Int = add_offset(40).\n",
    )
    .expect("parse changed evaluator provider");
    let after_evaluator = syntax_module_output_to_interface(&changed_evaluator);
    assert_ne!(
        before.constants["ANSWER"].fingerprint,
        after_value.constants["ANSWER"].fingerprint
    );
    assert_ne!(
        before.const_functions[&("add_offset".to_string(), 1)].fingerprint,
        after_evaluator.const_functions[&("add_offset".to_string(), 1)].fingerprint
    );
    assert_ne!(
        before.to_terlan_interface_text(),
        after_value.to_terlan_interface_text()
    );

    let consumer = parse_module_as_syntax_output(
        "module lifecycle.cache_consumer.\n\
         import lifecycle.cache.{ANSWER}.\n\
         pub matches(value: Int): Bool -> case value { ANSWER -> true; _ -> false }.\n",
    )
    .expect("parse dependent pattern consumer");
    let before_interfaces = HashMap::from([("lifecycle.cache".to_string(), before)]);
    let after_interfaces = HashMap::from([("lifecycle.cache".to_string(), after_value)]);
    let (before_prepared, before_diagnostics) =
        super::prepare_syntax_constants_with_interfaces(&consumer, &before_interfaces);
    let (after_prepared, after_diagnostics) =
        super::prepare_syntax_constants_with_interfaces(&consumer, &after_interfaces);
    assert!(before_diagnostics.is_empty(), "{before_diagnostics:#?}");
    assert!(after_diagnostics.is_empty(), "{after_diagnostics:#?}");
    let before_text = format!("{before_prepared:#?}");
    let after_text = format!("{after_prepared:#?}");
    assert!(before_text.contains("42"), "{before_text}");
    assert!(after_text.contains("43"), "{after_text}");
    assert_ne!(
        before_text, after_text,
        "dependent pattern analysis must be rebuilt"
    );
}

#[test]
fn substituted_constants_retain_use_site_provenance() {
    let source =
        "module lifecycle.provenance.\nconst ANSWER: Int = 42.\npub answer(): Int -> ANSWER.\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse provenance fixture");
    let original_span = syntax
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, clauses, .. } if name == "answer" => {
                clauses.first().map(|clause| clause.body.span.clone())
            }
            _ => None,
        })
        .expect("constant reference span");
    let (prepared, diagnostics) = prepare_syntax_constants(&syntax);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let substituted = prepared
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, clauses, .. } if name == "answer" => {
                clauses.first().map(|clause| &clause.body)
            }
            _ => None,
        })
        .expect("substituted body");
    assert_eq!(substituted.text.as_deref(), Some("42"));
    assert_eq!(substituted.span, original_span);
}
