use super::test_support::*;
use super::*;
use terlan_hir::{
    load_interfaces_from_file_set, parse_interface_file,
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};
use terlan_syntax::{parse_interface_module_as_syntax_output, parse_module_as_syntax_output};

/// Verifies remote public signatures preserve fully qualified alias returns.
///
/// Inputs:
/// - A provider interface exposing `compare` with a generic contained type
///   and a fully qualified `ordering.Comparison` callback/return type.
/// - A consumer module that calls `option.compare` with `Option[Int]`
///   shapes and compares the result with `:lt`.
///
/// Output:
/// - Test passes when the remote call result remains `Comparison` instead
///   of collapsing to the option contained type.
///
/// Transformation:
/// - Builds provider interfaces through the syntax-output interface path,
///   resolves the consumer against those interfaces, and checks that
///   generic argument inference at the interface boundary does not leak the
///   `T` substitution into the declared callback return.
#[test]
fn syntax_output_remote_comparator_signature_preserves_qualified_alias_return_type() {
    let ordering = parse_interface_module_as_syntax_output(
        "\
module ordering.\n\
pub type Comparison = :lt | :eq | :gt.\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse ordering fixture: {:?}", err));
    let option = parse_interface_module_as_syntax_output(
        "\
module option.\n\
pub type Option[T] = :none | {:some, T}.\n\
pub constructor None {\n\
    (): Option[T] -> :none\n\
}.\n\
pub constructor Some[T] {\n\
    (value: T): Option[T] -> {:some, value}\n\
}.\n\
pub compare(\n\
    left: Option[A],\n\
    right: Option[A],\n\
    value_compare: (A, A) -> ordering.Comparison\n\
): ordering.Comparison.\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse option interface fixture: {:?}", err));

    let mut interfaces = HashMap::new();
    interfaces.insert(
        ordering.module_name.clone(),
        syntax_module_output_to_interface(&ordering),
    );
    interfaces.insert(
        option.module_name.clone(),
        syntax_module_output_to_interface(&option),
    );

    let consumer = parse_module_as_syntax_output(
        "\
module option_consumer.\n\
import option.{None, Some}.\n\
import type ordering.Comparison.\n\
pub compare_int(left: Int, right: Int): Comparison -> :lt.\n\
pub demo(): Bool ->\n\
    std.test.Test.assert_equal(:lt, option.compare(None, Some(1), compare_int)).\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer fixture: {:?}", err));

    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies checked std summaries expose `std.core.Option.compare` correctly.
///
/// Inputs:
/// - A consumer fixture resolved with `load_interfaces_from_file_set` from
///   the std test tree, matching the `terlc test` dependency-loading path.
///
/// Output:
/// - Test passes when `std.core.Option.compare` typechecks as returning an
///   ordering atom domain instead of the contained option value type.
///
/// Transformation:
/// - Loads checked-in std `.typi` summaries, resolves a consumer module
///   against them, and typechecks a release-style assertion using
///   `Option.compare(None, Some(1), compare_int)`.
#[test]
fn syntax_output_std_option_compare_summary_preserves_comparison_return_type() {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("std/core/option_test.terl");
    let option_summary_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("std/summaries/std.core.Option.typi");
    let direct_option_function_keys = parse_interface_file(&option_summary_path)
        .map(|(_module_name, interface)| {
            let mut keys = interface.functions.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys
        })
        .unwrap_or_default();
    let interfaces = load_interfaces_from_file_set(&fixture_path.to_string_lossy());
    let option_compare_return = interfaces
        .get("std.core.Option")
        .and_then(|interface| interface.functions.get(&("compare".to_string(), 3)))
        .map(|signature| signature.return_type.as_str());
    let option_function_keys = interfaces
        .get("std.core.Option")
        .map(|interface| {
            let mut keys = interface.functions.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            keys
        })
        .unwrap_or_default();
    let mut interface_keys = interfaces.keys().cloned().collect::<Vec<_>>();
    interface_keys.sort();
    assert_eq!(
            option_compare_return,
            Some("std.core.Ordering.Comparison"),
            "loaded interfaces: {:?}; loaded std.core.Option function keys: {:?}; direct std.core.Option function keys: {:?}",
            interface_keys,
            option_function_keys,
            direct_option_function_keys
        );
    let option_interface = interfaces.get("std.core.Option").expect("option interface");
    let compare_signature = option_interface
        .functions
        .get(&("compare".to_string(), 3))
        .expect("compare signature");
    let mut global_aliases = HashMap::new();
    for interface in interfaces.values() {
        for (name, alias) in interface_type_aliases(interface) {
            global_aliases.insert(format!("{}.{}", interface.module, name), alias);
        }
    }
    let compare_scheme =
        parse_interface_signature(compare_signature, option_interface, &global_aliases)
            .expect("parse compare scheme");
    assert!(
        matches!(compare_scheme.ret, Type::Union(ref items) if items.len() == 3),
        "compare scheme: {:?}",
        compare_scheme
    );
    let mut trial_subst = HashMap::new();
    let empty_fns = HashMap::new();
    let empty_signatures = HashMap::new();
    let empty_module_aliases = HashMap::new();
    let empty_file_imports = HashMap::new();
    let empty_markdown_imports = HashMap::new();
    let empty_function_imports = HashMap::new();
    let empty_imported_type_names = HashMap::new();
    let empty_constructor_aliases = HashMap::new();
    let empty_constructors = HashMap::new();
    let empty_templates = HashMap::new();
    let empty_struct_fields = HashMap::new();
    let empty_receiver_methods = HashMap::new();
    let empty_trait_method_calls = HashMap::new();
    let empty_trait_bound_impls = HashMap::new();
    let empty_trait_signatures = HashMap::new();
    let empty_alias_names = HashSet::new();
    let trial_trait_cache = RefCell::new(TraitLookupCache::default());
    let trial_ctx = ExprInferContext {
        local_fns: &empty_fns,
        signatures: &empty_signatures,
        interface_map: &interfaces,
        module_aliases: &empty_module_aliases,
        file_imports: &empty_file_imports,
        markdown_imports: &empty_markdown_imports,
        function_imports: &empty_function_imports,
        imported_type_names: &empty_imported_type_names,
        constructor_aliases: &empty_constructor_aliases,
        constructors: &empty_constructors,
        templates: &empty_templates,
        aliases: &global_aliases,
        struct_fields: &empty_struct_fields,
        receiver_methods: &empty_receiver_methods,
        trait_method_calls: &empty_trait_method_calls,
        trait_bound_impl_type_args: &empty_trait_bound_impls,
        trait_signatures: &empty_trait_signatures,
        alias_names: &empty_alias_names,
        current_bounds: &[],
        trait_lookup_cache: &trial_trait_cache,
    };
    let trial_result = infer_function_with_bounds(
        &compare_scheme,
        Some("compare"),
        &[
            Type::LiteralAtom("none".to_string()),
            Type::Tuple(vec![
                Type::LiteralAtom("some".to_string()),
                Type::LiteralInt(1),
            ]),
            Type::Function {
                params: vec![Type::Int, Type::Int],
                ret: Box::new(compare_scheme.ret.clone()),
            },
        ],
        &trial_ctx,
        &mut trial_subst,
    )
    .expect("trial compare inference");
    assert!(
        matches!(trial_result, Type::Union(ref items) if items.len() == 3),
        "trial result: {:?}",
        trial_result
    );
    let module = parse_module_as_syntax_output(
        "\
module option_summary_consumer.\n\
import std.core.Option.{None, Some}.\n\
import std.core.Ordering.{Lt}.\n\
import type std.core.Ordering.Comparison.\n\
pub compare_int(left: Int, right: Int): Comparison ->\n\
    std.core.Int.compare(left, right).\n\
pub direct(): Comparison ->\n\
    std.core.Option.compare(None, Some(1), compare_int).\n\
pub demo(): Bool ->\n\
    std.test.Test.assert_equal(Lt, std.core.Option.compare(None, Some(1), compare_int)).\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse summary consumer fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_imported_constructor_alias_calls_are_valid_on_formal_path() {
    let interface_source = "\
module option.\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {:some, value}\n\
}.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module option_consumer.\n\
import option.{Some}.\n\
pub make(value: Dynamic): Dynamic ->\n\
    Some(value).\n\
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
fn syntax_output_colon_remote_calls_are_checked_against_interfaces_on_formal_path() {
    let interface_source = "\
module math.\n\
pub inc(value: Int): Int.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module math_consumer.\n\
pub demo(): Int ->\n\
    math:inc(1).\n\
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
fn syntax_output_colon_remote_calls_report_argument_mismatches_on_formal_path() {
    let interface_source = "\
module math.\n\
pub inc(value: Int): Int.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module math_consumer.\n\
pub demo(): Int ->\n\
    math:inc(\"bad\").\n\
",
        interface_source,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Int found Binary")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies selected function imports are checked against provider signatures.
///
/// Inputs:
/// - A provider interface declaring `println(value: String): Unit`.
/// - A consumer module importing `println` by local name and calling it
///   with an `Int`.
///
/// Output:
/// - Test passes when the syntax-output typechecker reports an argument
///   mismatch for the selected import.
///
/// Transformation:
/// - Resolves the selected import through the provider interface and reuses
///   ordinary function scheme inference for the local call.
#[test]
fn syntax_output_selected_function_imports_report_argument_mismatches() {
    let interface_source = "\
module console.\n\
pub println(value: String): Unit.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module console_consumer.\n\
import console.{println}.\n\
pub demo(): Unit ->\n\
    println(1).\n\
",
        interface_source,
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Binary found 1")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies selected import diagnostics suggest the loaded primitive module.
///
/// Inputs:
/// - A loaded `std.core.Int` interface exporting `to_string`.
/// - A consumer that mistakenly imports `std.io.Int.{to_string}`.
///
/// Output:
/// - Test passes when the diagnostic names the missing module and suggests
///   the available core import path.
///
/// Transformation:
/// - Resolves a selected import whose provider interface is absent, searches
///   loaded interfaces for the selected function, and emits a deterministic
///   import suggestion.
#[test]
fn syntax_output_selected_function_imports_suggest_loaded_provider_module() {
    let interface_source = "\
module std.core.Int.\n\
pub to_string(value: Int): String.\n\
";
    let source = "\
module int_import_consumer.\n\
import std.io.Int.{to_string}.\n\
pub demo(): String ->\n\
    to_string(2).\n\
";
    let diagnostics = check_syntax_output_with_interface(source, interface_source);
    let diagnostic = diagnostics
        .iter()
        .find(|diag| {
            diag.message
                .contains("cannot find module `std.io.Int` for imported function `to_string`")
        })
        .unwrap_or_else(|| panic!("diagnostics: {:?}", diagnostics));
    assert!(
        diagnostic
            .message
            .contains("did you mean `std.core.Int.{to_string}`?"),
        "diagnostics: {:?}",
        diagnostics
    );
    assert_eq!(
        &source[diagnostic.span.start..diagnostic.span.end],
        "to_string",
        "diagnostic should point at selected import item"
    );
}

#[test]
fn syntax_output_imported_list_aliases_do_not_generate_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_list_alias_constructor_calls.\n\
import items.{Items}.\n\
pub make(values: List[Int]): Items[Int] ->\n\
    Items(values).\n\
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
pub type Ok[T] = {:ok, value: T}.\n\
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
/// - A provider interface exporting `Ok[T] = {:ok, value: T}`.
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
pub type Ok[T] = {:ok, value: T}.\n\
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
    Items(values) with Wrapped { values = values }.\n\
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
    Bag(values) with Wrapped { values = values }.\n\
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
/// - A provider interface exporting `User = {:user, id: Int, name: Binary}`.
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
    User(id) with Wrapped { id = id }.\n\
",
        "\
module result.\n\
pub type User = {:user, id: Int, name: Binary}.\n\
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
/// - A provider interface exporting `User = {:user, id: Int, name: Binary}`.
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
    Member(id) with Wrapped { id = id }.\n\
",
        "\
module result.\n\
pub type User = {:user, id: Int, name: Binary}.\n\
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
    Props(#{name = name}).\n\
",
        "\
module props.\n\
pub type Props = #{name := Binary}.\n\
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
pub type Props = #{name := Binary}.\n\
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
        None -> :ok\n\
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
        None -> :ok\n\
    }.\n\
",
        "\
module options.\n\
pub type None = :none | :empty.\n\
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
pub type None = :none | :empty.\n\
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
pub type Ok[T] = {:ok, value: T}.\n\
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
pub type Ok[T] = {:ok, value: T}.\n\
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
pub type Ok[T] = {:ok, value: T}.\n\
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
/// - A provider interface exporting `Ok[T] = {:ok, value: T}`.
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
pub type Ok[T] = {:ok, value: T}.\n\
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
fn expands_syntax_derives_copies_imported_parent_struct_fields() {
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
pub struct FileError derives Error {\n\
    path: String\n\
}.\n",
    )
    .expect("parse consumer derive source fixture");
    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;

    let (expanded, diagnostics) = expand_syntax_derives(consumer, &resolved);

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

    let maps = collect_syntax_import_maps(&module);

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

/// Verifies imported public struct type identity does not allow raw
/// construction outside the defining module.
///
/// Inputs:
/// - A provider interface declaring public struct `Point`.
/// - A consumer module importing that type and attempting `#Point { ... }`.
///
/// Output:
/// - Test passes when typechecking rejects the raw imported struct literal
///   before CoreIR/backend emission.
///
/// Transformation:
/// - Resolves a consumer against an explicit interface map and checks that
///   record construction visibility is enforced semantically, independent
///   of syntax acceptance.
#[test]
fn syntax_output_rejects_raw_imported_struct_construction_without_constructor() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub struct Point {\n\
    x: Int\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module raw_imported_struct_construction_boundary.\n\
\n\
import type provider.Point.\n\
\n\
pub make(): Dynamic ->\n\
    #Point { x = 1 }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("cannot raw-construct imported struct provider.Point")),
        "diagnostics: {:?}",
        diagnostics
    );
}
