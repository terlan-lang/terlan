
/// Verifies generic type aliases share canonical implication spacing.
#[test]
fn formatter_preserves_structural_generic_type_alias_implication() {
    let output = format_source_module(
        r#"
module alias_implication_fmt.

pub type Named[T=>{name:String}] = T.
"#,
    )
    .expect("format generic alias implication");

    assert!(output.contains("pub type Named[T => {name: String}] ="));
}

/// Verifies generic implementation implications reconstruct their source form
/// after parsing separates semantic trait arguments from implication evidence.
#[test]
fn formatter_preserves_generic_trait_impl_structural_implication() {
    let output = format_source_module(
        r#"
module impl_implication_fmt.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[T=>{title:String}] for T {
    render(value: T): String -> value.title.
}.
"#,
    )
    .expect("format generic trait impl implication");

    assert!(output.contains("pub impl Render[T => {title: String}] for T"));
}

/// Verifies bodyless negative trait facts round-trip canonically.
#[test]
fn formatter_preserves_negative_trait_impl_declarations() {
    let output = format_source_module(
        r#"
module negative_impl_fmt.

pub opaque type SecretKey.

pub impl not JsonEncode[SecretKey].
impl not Copy[Box[SecretKey]].
"#,
    )
    .expect("format negative trait impl declarations");

    assert!(output.contains("pub impl not JsonEncode[SecretKey]."));
    assert!(output.contains("impl not Copy[Box[SecretKey]]."));
    assert!(!output.contains("impl not JsonEncode for"));
}

/// Verifies long single-clause function bodies wrap at the default width.
///
/// Inputs:
/// - A test-shaped function body with multiple semicolon-separated
///   expressions that would exceed the formatter's default line length if kept
///   on one line.
///
/// Output:
/// - Formatted source with each expression on its own indented line.
///
/// Transformation:
/// - Parses and formats the function body through the normal formatter so
///   standard-library tests do not retain unreadable one-line bodies.
#[test]
fn formatter_wraps_long_function_body_sequences() {
    let output = format_source_module(
        r#"
module long_body_fmt.

pub value_of(entry: {Binary, Int}): Int ->
    1.

@test
pub map_fold_accumulates_entries(): Bool ->
    let users = Map({"alice", 1}, {"bob", 2}); assert_equal(3, KeyedEnumerable.fold(users, 0, (sum, entry) -> sum + value_of(entry))).
"#,
    )
    .expect("format long function body");

    assert!(output.contains(
        "pub map_fold_accumulates_entries(): Bool ->\n    let users = Map({\"alice\", 1}, {\"bob\", 2});\n    assert_equal(3, KeyedEnumerable.fold(users, 0, (sum, entry) -> sum + value_of(entry)))."
    ));
    for line in output.lines() {
        assert!(
            line.chars().count() <= 100,
            "line exceeds default formatter width: {line}"
        );
    }
}

/// Verifies short function-body semicolon chains still split by statement.
///
/// Inputs:
/// - A function body whose semicolon-separated calls would fit on one line.
///
/// Output:
/// - Formatted source with one expression per line after each semicolon.
///
/// Transformation:
/// - Keeps `terlc fmt` as a layout normalizer for statement boundaries without
///   depending on the maximum line-length fallback.
#[test]
fn formatter_splits_short_function_body_semicolon_sequences() {
    let output = format_source_module(
        r#"
module short_sequence_fmt.

pub main(): Unit ->
    a(); b(); c().
"#,
    )
    .expect("format short function body sequence");

    assert!(output.contains("pub main(): Unit ->\n    a();\n    b();\n    c()."));
    assert!(!output.contains("a(); b(); c()."));
}

/// Verifies let bindings in case clauses remain one statement per line.
///
/// Inputs:
/// - A case arm with a let binding followed by its result expression.
///
/// Output:
/// - A multiline arm whose statements retain body indentation.
///
/// Transformation:
/// - Prevents the formatter from emitting semicolon chains rejected by lint.
#[test]
fn formatter_splits_case_clause_let_sequence() {
    let output = format_source_module(
        r#"
module case_let_sequence_fmt.

pub apply(value: Option[Int], cb: (Int) -> Unit): Unit ->
    case value {
        None -> Unit;
        Some(found) -> let _done = cb(found); Unit
    }.
"#,
    )
    .expect("format case let sequence");

    assert!(output.contains("Some(found) ->\n            let _done = cb(found);\n            Unit"));
    assert!(!output.contains("let _done = cb(found); Unit"));
}

/// Verifies trivial constant function bodies are kept on one line.
///
/// Inputs:
/// - Multiline zero-argument functions returning literal constants.
///
/// Output:
/// - Canonical one-line function declarations for each constant body.
///
/// Transformation:
/// - Runs the source through `terlc fmt`'s declaration formatter so trivial
///   constant helpers do not spread across three lines by default.
#[test]
fn formatter_collapses_trivial_constant_function_bodies() {
    let output = format_source_module(
        r#"
module constant_function_fmt.

pub type Cell =
    Int.

pub type Active =
    Atom["active"].

pub type Finished =
    Atom["finished"].

pub type Phase =
    Active
  | Finished.

pub empty(): Int ->
    -1.

pub blocked(): Int ->
    -2.

pub hit(): Int ->
    -3.

pub miss(): Int ->
    -4.

pub label(): String ->
    "occupied".

pub ok(): Bool ->
    true.

pub tuple_value(): {Int, String} ->
    {1, "ready"}.

pub values(): List[Int] ->
    [1, 2, 3].
"#,
    )
    .expect("format trivial constant functions");

    assert!(output.contains("pub type Cell = Int."));
    assert!(output.contains("pub type Active."));
    assert!(output.contains("pub type Finished."));
    assert!(output.contains("pub type Phase = Active | Finished."));
    assert!(output.contains("pub empty(): Int -> -1."));
    assert!(output.contains("pub blocked(): Int -> -2."));
    assert!(output.contains("pub hit(): Int -> -3."));
    assert!(output.contains("pub miss(): Int -> -4."));
    assert!(output.contains("pub label(): String -> \"occupied\"."));
    assert!(output.contains("pub ok(): Bool -> true."));
    assert!(output.contains("pub tuple_value(): {Int, String} -> {1, \"ready\"}."));
    assert!(output.contains("pub values(): List[Int] -> [1, 2, 3]."));
    assert!(!output.contains("pub type Cell =\n      Int."));
    assert!(!output.contains("pub type Active = Atom[\"active\"]."));
    assert!(!output.contains("pub empty(): Int ->\n    -1."));
}

/// Verifies singleton atom aliases use the canonical bodyless spelling.
///
/// Inputs:
/// - Explicit aliases whose payloads do and do not match the deterministic
///   type-name conversion.
///
/// Output:
/// - Matching aliases collapse to shorthand while custom wire names remain
///   explicit.
///
/// Transformation:
/// - Proves formatting is semantics-preserving and does not erase an explicit
///   atom payload merely because the alias is a singleton.
#[test]
fn formatter_canonicalizes_only_matching_singleton_atom_aliases() {
    let output = format_source_module(
        r#"
module atom_alias_fmt.

pub type InvalidMove = Atom["invalid_move"].
pub type HTTPError = Atom["http_error"].
pub type ExternalCode = Atom["wire_error"].
"#,
    )
    .expect("format singleton atom aliases");

    assert!(output.contains("pub type InvalidMove."));
    assert!(output.contains("pub type HTTPError."));
    assert!(output.contains("pub type ExternalCode = Atom[\"wire_error\"]."));
}

/// Verifies long structural type aliases are split into vertical fields.
///
/// Inputs:
/// - A single structural type alias wider than the alias readability limit.
///
/// Output:
/// - Canonical source with the structural type body formatted one field per
///   line.
///
/// Transformation:
/// - Keeps short aliases compact while avoiding very wide record/tuple-shaped
///   type declarations.
#[test]
fn formatter_wraps_long_structural_type_alias_fields() {
    let output = format_source_module(
        r#"
module long_type_alias_fmt.

pub type Match = {Atom["match"], player_one: Dynamic, player_two: Dynamic, current_turn: String, winner: String, strikes: List[Dynamic], phase: Dynamic}.
"#,
    )
    .expect("format long structural type alias");

    assert!(output.contains(
        r#"pub type Match =
      {
          Atom["match"],
          player_one: Dynamic,
          player_two: Dynamic,
          current_turn: String,
          winner: String,
          strikes: List[Dynamic],
          phase: Dynamic
      }."#
    ));
    assert!(!output.contains("pub type Match = {Atom[\"match\"], player_one"));
}

/// Verifies long structural aliases split only at top-level commas.
///
/// Inputs:
/// - A long structural alias containing nested generic and tuple-shaped type
///   expressions.
///
/// Output:
/// - Canonical source preserving nested comma groups inside a vertical alias.
///
/// Transformation:
/// - Protects nested type expressions from the long-alias column splitter.
#[test]
fn formatter_wraps_long_structural_type_alias_without_splitting_nested_types() {
    let output = format_source_module(
        r#"
module nested_long_type_alias_fmt.

pub type EventEnvelope = {Atom["event"], meta: Map[String, {source: String, retry: Bool}], payload: Result[List[{id: Int, label: String}], String], received_at: String}.
"#,
    )
    .expect("format nested long structural type alias");

    assert!(output.contains(
        r#"pub type EventEnvelope =
      {
          Atom["event"],
          meta: Map[String, {source: String, retry: Bool}],
          payload: Result[List[{id: Int, label: String}], String],
          received_at: String
      }."#
    ));
}

/// Verifies lambda-valued let bindings remain parseable after formatting.
///
/// Inputs:
/// - A function body that binds a lambda, then calls it in the let body.
///
/// Output:
/// - Formatted source with an extra grouping boundary around the lambda value.
///
/// Transformation:
/// - Formats and reparses the source so fmt cannot consume the let-body
///   separator as part of the lambda body.
#[test]
fn formatter_keeps_lambda_let_binding_parseable() {
    let output = format_source_module(
        r#"
module lambda_let_fmt.

pub run(): Bool ->
    let double = ((value) -> value * 2); double(4) == 8.
"#,
    )
    .expect("format lambda let binding");

    assert!(output.contains("let double = ((value) -> value * 2);"));
    parse_module(&output).expect("formatted lambda let binding should parse");
}

/// Verifies list-comprehension filters survive formatting.
///
/// Inputs:
/// - A list comprehension with multiple boolean filters.
///
/// Output:
/// - Formatted source that keeps both guards after the generator.
///
/// Transformation:
/// - Formats and reparses the source so fmt cannot drop comprehension filters.
#[test]
fn formatter_preserves_list_comprehension_guards() {
    let output = format_source_module(
        r#"
module list_comprehension_guard_fmt.

pub values(): List[Int] ->
    let input = [-1, 0, 1, 2, 10];
    [value | value <- input, value > 0, value < 10].
"#,
    )
    .expect("format guarded list comprehension");

    assert!(output.contains("[value | value <- input, value > 0, value < 10]."));
    parse_module(&output).expect("formatted guarded list comprehension should parse");
}

/// Verifies ordered generators survive formatting and reparsing.
#[test]
fn formatter_preserves_ordered_list_comprehension_generators() {
    let output = format_source_module(
        r#"
module list_comprehension_generator_fmt.

pub flatten(rows: List[List[Int]]): List[Int] ->
    [value | row <- rows, value <- row, value > 0].
"#,
    )
    .expect("format multi-generator list comprehension");

    assert!(output.contains("[value | row <- rows, value <- row, value > 0]."));
    parse_module(&output).expect("formatted multi-generator comprehension should parse");
}

/// Verifies multiline let-binding values are indented under the binding.
///
/// Inputs:
/// - A let expression whose binding value is a case expression.
///
/// Output:
/// - Formatted source with the case expression on the line after `=`.
///
/// Transformation:
/// - Exercises formatter-owned nested expression layout for let bindings so
///   std and test sources do not rely on manual indentation fixes.
#[test]
fn formatter_indents_multiline_let_binding_values() {
    let output = format_source_module(
        r#"
module multiline_let_value_fmt.

pub run(): Bool ->
    let left = case true { true -> 1; false -> 0 }; left == 1.
"#,
    )
    .expect("format multiline let binding");

    assert!(output.contains(
        "let left =\n        case true {\n            true -> 1;\n            false -> 0\n        };"
    ));
    parse_module(&output).expect("formatted multiline let binding should parse");
}

/// Verifies every binding remains explicitly marked after formatting.
#[test]
fn formatter_emits_and_preserves_repeated_let_keywords() {
    let source = r#"
module repeated_let_format.

pub total(value: Int): Int ->
    let first = value;
    let second = first + 1;
    second.
"#;

    let output = format_source_module(source).expect("repeated lets should format");
    assert!(output.contains("let first = value;\n    let second = first + 1;"));
    assert_eq!(
        format_source_module(&output).expect("formatted output should parse"),
        output
    );
}

/// Verifies case-clause guards preserve the canonical `where` spelling.
///
/// Inputs:
/// - A case expression using the canonical `where` guard spelling.
///
/// Output:
/// - Formatted source using `where` while preserving guard semantics.
///
/// Transformation:
/// - Keeps formatter output on the single Terlan guard introducer.
#[test]
fn formatter_preserves_case_guards_with_where() {
    let output = format_source_module(
        r#"
module case_guard_where_fmt.

pub run(value: Int): Int ->
    case value {
        item where item > 0 -> item;
        _ -> 0
    }.
"#,
    )
    .expect("format case guard");

    assert!(output.contains("item where item > 0 -> item;"));
    parse_module(&output).expect("formatted case guard should parse");
}

/// Verifies formatter output preserves `${...}` string capture patterns.
///
/// Inputs:
/// - A module using typed and untyped string captures in `case` and `let`
///   pattern positions.
///
/// Output:
/// - Formatted source preserving capture braces, annotation text, and literal
///   separators.
///
/// Transformation:
/// - Runs capture-bearing strings through parser and formatter output so the
///   CLI formatting path cannot collapse them into ordinary string literals or
///   Elixir-style interpolation syntax.
#[test]
fn formatter_preserves_string_capture_patterns() {
    let output = format_source_module(
        r#"
module string_capture_fmt.

pub route(path: String): String ->
    case path {
        "users/${id: Int}/${name}.json" where id > 0 -> name;
        _ -> "missing"
    }.

pub bind(path: String): String ->
    let "users/${id: Int}/${name}.json" = path;
    name.
"#,
    )
    .expect("format string capture patterns");

    assert!(output.contains(r#""users/${id: Int}/${name}.json" where id > 0 -> name;"#));
    assert!(output.contains(r#"let "users/${id: Int}/${name}.json" = path;"#));
    assert!(!output.contains("#{id"));
    parse_module(&output).expect("formatted string capture patterns should parse");
}

/// Verifies formatter output canonicalizes parse-preserved shape declarations.
///
/// Inputs:
/// - Public and guarded shape declarations preserved as raw syntax before
///   expansion exists.
///
/// Output:
/// - Formatted source with canonical parameter and pattern punctuation while
///   keeping string capture literal contents intact.
///
/// Transformation:
/// - Exercises the shape-specific raw formatter so `terlc fmt` can support the
///   public syntax foothold without enabling shape expansion semantics.
#[test]
fn formatter_preserves_shape_synonym_raw_declarations() {
    let output = format_source_module(
        r#"
module shape_synonym_fmt.

pub shape UserAsset(id, file) =
    "users/${id: Int}/assets/${file}".

shape OkResponse(body) =
    {status, body} where status in 200..299.
"#,
    )
    .expect("format shape synonym raw declarations");

    assert!(
        output.contains(r#"pub shape UserAsset(id, file) = "users/${id: Int}/assets/${file}"."#)
    );
    assert!(output.contains("shape OkResponse(body) = {status, body} where status in 200 .. 299."));
    parse_module(&output).expect("formatted shape synonym raw declarations should parse");
}

/// Verifies formatter output preserves descriptor-backed binary layouts.
///
/// Inputs:
/// - A module using `Binary[big] { ... }` as an expression and
///   `Binary[little] { ... }` as a function-head pattern.
///
/// Output:
/// - Formatted source retaining endian policy, field order, and descriptor
///   type text.
///
/// Transformation:
/// - Locks the parser scaffold to a stable public source shape before runtime
///   construction/matching support is enabled.
#[test]
fn formatter_preserves_binary_layout_scaffold() {
    let output = format_source_module(
        r#"
module binary_layout_fmt.

pub packet(): Dynamic ->
    Binary[big] { source_port: UInt[16], payload: Rest }.

decode(Binary[little] { opcode: UInt[8], payload: Rest }): Int ->
    1.
"#,
    )
    .expect("format binary layout scaffold");

    assert!(output.contains("Binary[big] {source_port: UInt[16], payload: Rest}."));
    assert!(output.contains("Binary[little] {opcode: UInt[8], payload: Rest}"));
    assert!(output.contains("): Int -> 1."));
    parse_module(&output).expect("formatted binary layout scaffold should parse");
}

/// Verifies long descriptor-backed binary layouts are split vertically.
///
/// Inputs:
/// - A binary layout whose inline spelling exceeds the binary layout
///   formatter threshold.
///
/// Output:
/// - Formatted source places each descriptor on its own line while preserving
///   order and descriptor text.
///
/// Transformation:
/// - Keeps protocol-layout source readable without changing the parser surface
///   or runtime staging rules.
#[test]
fn formatter_splits_long_binary_layout_scaffold() {
    let output = format_source_module(
        r#"
module binary_layout_fmt_long.

pub packet(): Dynamic ->
    Binary[big] { source_port: UInt[16], destination_port: UInt[16], sequence_number: UInt[32], acknowledgement_number: UInt[32], data_offset: UInt[4], flags: UInt[8], payload: Rest }.
"#,
    )
    .expect("format long binary layout scaffold");

    assert!(output.contains("Binary[big] {\n"));
    assert!(output.contains("    source_port: UInt[16],\n"));
    assert!(output.contains("    destination_port: UInt[16],\n"));
    assert!(output.contains("    payload: Rest\n}"));
    parse_module(&output).expect("formatted long binary layout scaffold should parse");
}

/// Verifies function-head guards format to the canonical `where` spelling.
///
/// Inputs:
/// - A multi-clause function using the canonical `where` guard spelling.
///
/// Output:
/// - Formatted source using `where` while preserving function-clause order.
///
/// Transformation:
/// - Keeps function-head guards consistent with case/try clause guard
///   formatting so public source converges on the Terlan spelling.
#[test]
fn formatter_canonicalizes_function_head_guards_to_where() {
    let output = format_source_module(
        r#"
module function_guard_where_fmt.

pub classify(value) where value < 0 ->
    "negative";
classify(_value) ->
    "positive".
"#,
    )
    .expect("format function guard");

    assert!(output.contains("classify(value) where value < 0 ->"));
    parse_module(&output).expect("formatted function guard should parse");
}

/// Verifies over-width case branch bodies are split below the arrow.
///
/// Inputs:
/// - A case expression with a long list-cons branch body.
///
/// Output:
/// - Formatted source where the long body is placed on the next line.
///
/// Transformation:
/// - Applies the default line-length rule at case-clause level instead of
///   leaving long pattern-heavy branches for manual cleanup.
#[test]
fn formatter_wraps_long_case_clause_bodies() {
    let output = format_source_module(
        r#"
module long_case_clause_fmt.

pub run(iterator: Iterator[{K, V}], cb: (V) -> U): List[{K, U}] ->
    case Iterator.next(iterator) {
        Some({value: {key, value}, next: next}) -> [{key, cb(value)} | map_keyed_iterator(next, cb)];
        None -> []
    }.
"#,
    )
    .expect("format long case clause");

    assert!(output.contains(
        "Some({value: {key, value}, next: next}) ->\n            [{key, cb(value)} | map_keyed_iterator(next, cb)];"
    ));
    for line in output.lines() {
        assert!(
            line.chars().count() <= 100,
            "line exceeds default formatter width: {line}"
        );
    }
    parse_module(&output).expect("formatted long case clause should parse");
}

/// Verifies over-width function signatures split their parameter list.
///
/// Inputs:
/// - A function signature whose inline form would exceed the formatter width.
///
/// Output:
/// - Formatted source with one parameter per line and a parseable declaration.
///
/// Transformation:
/// - Applies the default line-length rule to declaration signatures so std
///   source files can be kept under the formatter-owned width.
#[test]
fn formatter_wraps_long_function_signatures() {
    let output = format_source_module(
        r#"
module long_signature_fmt.

pub filter_keyed_iterator[K, V](iterator: Iterator[{K, V}], predicate: ({K, V}) -> Bool): List[{K, V}] ->
    [].
"#,
    )
    .expect("format long signature");

    assert!(output.contains(
        "pub filter_keyed_iterator[K, V](\n    iterator: Iterator[{K, V}],\n    predicate: ({K, V}) -> Bool\n): List[{K, V}] ->"
    ));
    for line in output.lines() {
        assert!(
            line.chars().count() <= 100,
            "line exceeds default formatter width: {line}"
        );
    }
    parse_module(&output).expect("formatted long signature should parse");
}

/// Verifies constructor-shaped case patterns remain valid source.
///
/// Inputs:
/// - A case expression matching nullary and payload constructor patterns.
///
/// Output:
/// - Formatted source with `None` and `Some(value)`, not tuple spellings such
///   as `{None}`.
///
/// Transformation:
/// - Parses and formats constructor pattern cases so directory fmt output
///   remains parseable.
#[test]
fn formatter_preserves_constructor_patterns() {
    let output = format_source_module(
        r#"
module constructor_pattern_fmt.

pub value(option: Dynamic): Int ->
    case option {
        None ->
            0;
        Some(value) ->
            value
    }.
"#,
    )
    .expect("format constructor patterns");

    assert!(output.contains("None ->"));
    assert!(output.contains("Some(value) ->"));
    assert!(!output.contains("{None}"));
    assert!(!output.contains("{Some, value}"));
}

/// Verifies function-head pattern parameters survive formatter round-trips.
///
/// Inputs:
/// - A single-clause function using tuple destructuring in the typed parameter
///   head.
///
/// Output:
/// - Formatted source keeps the destructuring signature instead of replacing
///   it with the generated `_ArgN` parameter name.
///
/// Transformation:
/// - Uses the formatter's single-clause canonicalization path so pattern-head
///   functions remain readable after `terlc fmt`.
#[test]
fn formatter_preserves_function_head_pattern_parameters() {
    let output = format_source_module(
        r#"
module function_head_pattern_fmt.

pub add({left, right}: Dynamic): Int ->
    left + right.
"#,
    )
    .expect("format function-head pattern parameter");

    assert!(output.contains("pub add({left, right}: Dynamic): Int ->"));
    assert!(!output.contains("_Arg1"));
    parse_module(&output).expect("formatted function-head pattern should parse");
}

/// Verifies canonical atom literals format with portable escapes.
///
/// Inputs:
/// - A source module returning `Atom["..."]` with escaped quote, backslash,
///   newline, carriage return, and tab payloads.
///
/// Output:
/// - Formatted source preserving the atom as a single-line canonical literal
///   with escaped control characters.
///
/// Transformation:
/// - Parses escaped atom payloads into their semantic value and renders them
///   back through the formatter's shared string literal escaping path.
#[test]
fn formatter_escapes_canonical_atom_literals_portably() {
    let output = format_source_module(
        r#"
module atom_literal_format.

ready(): Atom ->
    Atom["quote \" slash \\ newline \n carriage \r tab \t"].
"#,
    )
    .expect("format escaped canonical atom");

    assert!(output.contains(r#"Atom["quote \" slash \\ newline \n carriage \r tab \t"]."#));
    assert!(!output.contains("newline \n carriage"));
    assert!(!output.contains("carriage \r tab"));
    assert!(!output.contains("tab \t\""));
}

/// Verifies source modules cannot reach formatter export-list normalization.
///
/// Inputs:
/// - A canonical `.terl` module string containing removed source `export`
///   syntax.
///
/// Output:
/// - Parse diagnostic from the source parser.
///
/// Transformation:
/// - Attempts source parsing before formatting, proving formatter export
///   support is not a source-module escape hatch.
#[test]
fn formatter_source_parser_rejects_export_declarations() {
    let error = parse_module(
        r#"
module formatter_source_export.

export ghost/1.
"#,
    )
    .expect_err("source parser must reject export declarations before formatting");

    assert!(error
        .message
        .contains("source export declarations are not part of canonical Terlan"));
}
