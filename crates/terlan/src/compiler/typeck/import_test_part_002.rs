
/// Verifies aliased imported list aliases do not become constructor calls.
///
/// Inputs:
/// - A provider interface exporting non-eligible alias `Items[T] = List[T]`.
/// - A consumer module importing `Items as Bag` and calling `Bag(values)`.
///
/// Output:
/// - Test passes when syntax-output typechecking reports `unknown
///   constructor Bag / 1`.
///
/// Transformation:
/// - Loads provider interface metadata into the syntax-output typechecker,
///   resolves the local import alias, and confirms non-single-shape aliases
///   never produce constructor-call identity metadata under aliased names.
#[test]
fn syntax_output_aliased_imported_list_aliases_do_not_generate_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module aliased_imported_list_alias_constructor_calls.\n\
import items.{Items as Bag}.\n\
pub make(values: List[Int]): Bag[Int] ->\n\
    Bag(values).\n\
",
        "\
module items.\n\
pub type Items[T] = List[T].\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Bag / 1"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_alias_constructor_calls_report_arity_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_alias_constructor_call_arity.\n\
import result.{Ok}.\n\
pub make(): Dynamic ->\n\
    Ok().\n\
",
        "\
module result.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.message == "constructor Ok has arity mismatch: expected 1..1 args, found 0"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies aliased imported eligible type-alias constructor calls with
/// wrong arity fail as constructor arity errors on the source alias name.
///
/// Inputs:
/// - A provider interface exporting `Ok[T] = {Atom["ok"], value: T}`.
/// - A consumer module importing `Ok as Success` and calling `Success()`.
///
/// Output:
/// - Test passes when syntax-output typechecking reports the constructor
///   arity mismatch against `Success`.
///
/// Transformation:
/// - Loads provider interface metadata into the syntax-output typechecker,
///   resolves the local import alias, and confirms eligible imported
///   aliases preserve arity diagnostics for source-visible call heads.
#[test]
fn syntax_output_aliased_imported_alias_constructor_calls_report_arity_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module aliased_imported_alias_constructor_call_arity.\n\
import result.{Ok as Success}.\n\
pub make(): Dynamic ->\n\
    Success().\n\
",
        "\
module result.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.message == "constructor Success has arity mismatch: expected 1..1 args, found 0"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_list_aliases_do_not_generate_constructor_chains_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_list_alias_constructor_chains.\n\
import items.{Items}.\n\
pub make(values: List[Int]): Dynamic ->\n\
    Items(values) with Wrapped { values: values }.\n\
",
        "\
module items.\n\
pub type Items[T] = List[T].\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Items / 1"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies aliased imported list aliases do not become constructor-chain
/// bases.
///
/// Inputs:
/// - A provider interface exporting non-eligible alias `Items[T] = List[T]`.
/// - A consumer module importing `Items as Bag` and using `Bag(values)` as
///   a constructor-chain base.
///
/// Output:
/// - Test passes when syntax-output typechecking reports `unknown
///   constructor Bag / 1`.
///
/// Transformation:
/// - Loads provider interface metadata into the syntax-output typechecker,
///   resolves the local import alias, and confirms non-single-shape aliases
///   never produce constructor-chain identity metadata under aliased names.
#[test]
fn syntax_output_aliased_imported_list_aliases_do_not_generate_constructor_chains_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module aliased_imported_list_alias_constructor_chains.\n\
import items.{Items as Bag}.\n\
pub make(values: List[Int]): Dynamic ->\n\
    Bag(values) with Wrapped { values: values }.\n\
",
        "\
module items.\n\
pub type Items[T] = List[T].\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Bag / 1"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies directly imported eligible type-alias constructor chains with
/// wrong arity fail as constructor arity errors.
///
/// Inputs:
/// - A provider interface exporting `User = {Atom["user"], id: Int, name: Binary}`.
/// - A consumer module importing `User` directly and using `User(id)` as a
///   constructor-chain base.
///
/// Output:
/// - Test passes when syntax-output typechecking reports the imported
///   constructor arity mismatch.
///
/// Transformation:
/// - Loads provider interface metadata into the syntax-output typechecker
///   and confirms imported single-shape aliases keep arity diagnostics for
///   constructor-chain bases instead of becoming unresolved chain metadata.
#[test]
fn syntax_output_imported_alias_constructor_chains_report_arity_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_alias_constructor_chain_arity.\n\
import result.{User}.\n\
pub make(id: Int): Dynamic ->\n\
    User(id) with Wrapped { id: id }.\n\
",
        "\
module result.\n\
pub type User = {Atom[\"user\"], id: Int, name: Binary}.\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.message == "constructor User has arity mismatch: expected 2..2 args, found 1"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies aliased imported eligible type-alias constructor chains with
/// wrong arity fail as constructor arity errors on the source alias name.
///
/// Inputs:
/// - A provider interface exporting `User = {Atom["user"], id: Int, name: Binary}`.
/// - A consumer module importing `User as Member` and using `Member(id)` as
///   a constructor-chain base.
///
/// Output:
/// - Test passes when syntax-output typechecking reports the constructor
///   arity mismatch against `Member`.
///
/// Transformation:
/// - Loads provider interface metadata into the syntax-output typechecker,
///   resolves the local import alias, and confirms eligible imported
///   aliases preserve arity diagnostics for source-visible chain bases.
#[test]
fn syntax_output_aliased_imported_alias_constructor_chains_report_arity_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module aliased_imported_alias_constructor_chain_arity.\n\
import result.{User as Member}.\n\
pub make(id: Int): Dynamic ->\n\
    Member(id) with Wrapped { id: id }.\n\
",
        "\
module result.\n\
pub type User = {Atom[\"user\"], id: Int, name: Binary}.\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.message == "constructor Member has arity mismatch: expected 2..2 args, found 1"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_structural_tuple_aliases_do_not_generate_constructor_calls_on_formal_path(
) {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_structural_alias_constructor_calls.\n\
import pairs.{Pair}.\n\
pub make(): Pair ->\n\
    Pair(1, 2).\n\
",
        "\
module pairs.\n\
pub type Pair = {left: Int, right: Int}.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Pair / 2"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_structural_tuple_aliases_do_not_generate_constructor_patterns_on_formal_path(
) {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_structural_alias_constructor_patterns.\n\
import pairs.{Pair}.\n\
pub left(input: Pair): Int ->\n\
    case input {\n\
        Pair(left, _right) -> left\n\
    }.\n\
",
        "\
module pairs.\n\
pub type Pair = {left: Int, right: Int}.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern Pair"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_map_aliases_do_not_generate_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_map_alias_constructor_calls.\n\
import props.{Props}.\n\
pub make(name: Binary): Props ->\n\
    Props({name: name}).\n\
",
        "\
module props.\n\
pub type Props = {name: Binary}.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Props / 1"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_map_aliases_do_not_generate_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_map_alias_constructor_patterns.\n\
import props.{Props}.\n\
pub name(input: Props): Binary ->\n\
    case input {\n\
        Props(values) -> values\n\
    }.\n\
",
        "\
module props.\n\
pub type Props = {name: Binary}.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern Props"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_list_aliases_do_not_generate_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_list_alias_constructor_patterns.\n\
import items.{Items}.\n\
pub unwrap(input: Items[Int]): List[Int] ->\n\
    case input {\n\
        Items(values) -> values\n\
    }.\n\
",
        "\
module items.\n\
pub type Items[T] = List[T].\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern Items"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies aliased imported list aliases do not become constructor
/// patterns.
///
/// Inputs:
/// - A provider interface exporting non-eligible alias `Items[T] = List[T]`.
/// - A consumer module importing `Items as Bag` and matching `Bag(values)`.
///
/// Output:
/// - Test passes when syntax-output typechecking reports `unknown
///   constructor pattern Bag`.
///
/// Transformation:
/// - Loads provider interface metadata into the syntax-output typechecker,
///   resolves the local import alias, and confirms non-single-shape aliases
///   never produce constructor-pattern identity metadata under aliased
///   names.
#[test]
fn syntax_output_aliased_imported_list_aliases_do_not_generate_constructor_patterns_on_formal_path()
{
    let diagnostics = check_syntax_output_with_interface(
        "\
module aliased_imported_list_alias_constructor_patterns.\n\
import items.{Items as Bag}.\n\
pub unwrap(input: Bag[Int]): List[Int] ->\n\
    case input {\n\
        Bag(values) -> values\n\
    }.\n\
",
        "\
module items.\n\
pub type Items[T] = List[T].\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern Bag"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_literal_alias_constructor_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_alias_literal_patterns.\n\
import literals.{None}.\n\
pub unwrap(input: None): Dynamic ->\n\
    case input {\n\
        None -> Atom[\"ok\"]\n\
    }.\n\
",
        "\
module literals.\n\
pub type None = Atom[\"none\"].\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_literal_alias_values_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_alias_literal_values.\n\
import literals.{None}.\n\
pub none(): None ->\n\
    None.\n\
",
        "\
module literals.\n\
pub type None = Atom[\"none\"].\n\
",
    );
    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies imported std aliases expand provider-local singleton aliases.
///
/// Inputs:
/// - A consumer module using `std.core.Bool.from_string`, whose summary returns
///   `Option[Bool]`.
/// - The real checked-in std summaries discovered from a std source path.
///
/// Output:
/// - Empty diagnostics.
///
/// Transformation:
/// - Resolves `Option[T] = None | Some[T]` through the provider module's alias
///   scope so `None` binds to `std.core.Option.None` even when another loaded
///   std module also exports a type named `None`.
#[test]
fn syntax_output_std_option_alias_expands_provider_local_none_alias() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module std_option_alias_scope.\n\
import std.core.Bool.\n\
import std.core.Option.\n\
pub parsed(): Bool ->\n\
    Option.with_default(Bool.from_string(\"true\"), false).\n\
",
        "std/core/BoolTest.terl",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported higher-kinded impl heads resolve module shorthand.
#[test]
fn syntax_output_std_enumerable_summary_resolves_default_collection_type_constructor() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module std_enumerable_import.\n\
import std.collections.{Enumerable, List, Set}.\n\
import std.core.Unit.\n\
ignore(_value: Int): Unit -> Unit.\n\
pub lists(values: List[Int]): Unit -> Enumerable[List].each(values, ignore).\n\
pub sets(values: Set[Int]): Unit -> Enumerable[Set].each(values, ignore).\n\
",
        "std/collections/EnumerableTest.terl",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_literal_alias_constructor_calls_are_rejected_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_alias_literal_calls.\n\
import literals.{None}.\n\
pub none(): None ->\n\
    None().\n\
",
        "\
module literals.\n\
pub type None = Atom[\"none\"].\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor None / 0"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_union_aliases_do_not_generate_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_alias_union_patterns.\n\
import options.{None}.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        None -> Atom[\"ok\"]\n\
    }.\n\
",
        "\
module options.\n\
pub type None = Atom[\"none\"] | Atom[\"empty\"].\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern None"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_union_aliases_do_not_generate_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_alias_union_calls.\n\
import options.{None}.\n\
pub none(): Dynamic ->\n\
    None().\n\
",
        "\
module options.\n\
pub type None = Atom[\"none\"] | Atom[\"empty\"].\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor None / 0"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_alias_constructor_calls_are_valid_on_formal_path() {
    let interface_source = "\
module result.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module result_consumer.\n\
import result.{Ok}.\n\
pub make(value: Int): Dynamic ->\n\
    Ok(value).\n\
",
        interface_source,
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_alias_constructor_patterns_are_valid_on_formal_path() {
    let interface_source = "\
module result.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module result_consumer.\n\
import result.{Ok}.\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value) -> value\n\
    }.\n\
",
        interface_source,
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_alias_constructor_patterns_report_arity_mismatch_on_formal_path() {
    let interface_source = "\
module result.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module result_consumer.\n\
import result.{Ok}.\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value, extra) -> value\n\
    }.\n\
",
        interface_source,
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.message == "constructor Ok has arity mismatch: expected 1..1 args, found 2"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies aliased imported eligible type-alias constructor patterns with
/// wrong arity fail as constructor arity errors on the source alias name.
///
/// Inputs:
/// - A provider interface exporting `Ok[T] = {Atom["ok"], value: T}`.
/// - A consumer module importing `Ok as Success` and matching
///   `Success(value, extra)`.
///
/// Output:
/// - Test passes when syntax-output typechecking reports the constructor
///   arity mismatch against `Success`.
///
/// Transformation:
/// - Loads provider interface metadata into the syntax-output typechecker,
///   resolves the local import alias, and confirms eligible imported
///   aliases preserve arity diagnostics for source-visible pattern heads.
#[test]
fn syntax_output_aliased_imported_alias_constructor_patterns_report_arity_mismatch_on_formal_path()
{
    let interface_source = "\
module result.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module result_consumer.\n\
import result.{Ok as Success}.\n\
pub unwrap(input: Success[Int]): Int ->\n\
    case input {\n\
        Success(value, extra) -> value\n\
    }.\n\
",
        interface_source,
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.message == "constructor Success has arity mismatch: expected 1..1 args, found 2"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn expands_syntax_includes_copies_imported_parent_struct_fields() {
    let provider = parse_module_as_syntax_output(
        "\
module std.core.\n\
\n\
pub struct Error {\n\
    code: Atom,\n\
    message: String\n\
}.\n",
    )
    .expect("parse provider struct source fixture");
    let provider_interface_text =
        syntax_module_output_to_interface(&provider).to_terlan_interface_text();
    let provider_summary = parse_interface_module_as_syntax_output(&provider_interface_text)
        .expect("parse rendered provider interface fixture");
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider_summary.module_name.clone(),
        syntax_module_output_to_interface(&provider_summary),
    );
    let consumer = parse_module_as_syntax_output(
        "\
module std.io.File.\n\
\n\
import std.core.{Error}.\n\
\n\
pub struct FileError includes Error {\n\
    path: String\n\
}.\n",
    )
    .expect("parse consumer include source fixture");
    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;

    let (expanded, diagnostics) = expand_syntax_includes(consumer, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    let file_error_fields = expanded
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, fields, .. } if name == "FileError" => Some(
                fields
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .expect("expanded imported FileError struct");
    assert_eq!(file_error_fields, vec!["code", "message", "path"]);
}

#[test]
fn syntax_output_rejects_imported_opaque_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module syntax_imported_opaque_calls.\n\
import users.{UserId}.\n\
pub make(value: Int): UserId ->\n\
    UserId(value).\n\
",
        "\
module users.\n\
pub opaque type UserId = Int.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message
            == "cannot construct opaque type users.UserId outside defining module"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_imported_opaque_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module syntax_imported_opaque_patterns.\n\
import users.{UserId}.\n\
pub unwrap(input: UserId): Int ->\n\
    case input {\n\
        UserId(value) -> value\n\
    }.\n\
",
        "\
module users.\n\
pub opaque type UserId = Int.\n\
",
    );

    assert!(
            diagnostics.iter().any(|diag| diag.message
                == "cannot match opaque type users.UserId as constructor pattern outside defining module"),
            "diagnostics: {:?}",
            diagnostics
        );
}

#[test]
fn syntax_output_collects_import_maps_on_formal_path() {
    let module = parse_module_as_syntax_output(
        r#"
module imports.

import std.text.{format as format_alias}.
import std.collections.Set.
import file "./view.html" as ViewHtml.
import css "./site.css" as SiteCss.
import markdown "./post.md" as Post.

pub view(): Binary ->
    ViewHtml.
"#,
    )
    .expect("parse syntax output import map fixture");

    let maps = collect_syntax_import_maps(&module, &HashMap::new());

    assert_eq!(
        maps.module_aliases.get("format_alias").map(String::as_str),
        Some("std.text.format")
    );
    assert_eq!(
        maps.module_aliases.get("Set").map(String::as_str),
        Some("std.collections.Set")
    );
    assert_eq!(
        maps.file_imports.get("ViewHtml").map(String::as_str),
        Some("./view.html")
    );
    assert_eq!(
        maps.file_imports.get("SiteCss").map(String::as_str),
        Some("./site.css")
    );
    assert_eq!(
        maps.markdown_imports.get("Post").map(String::as_str),
        Some("./post.md")
    );
}
