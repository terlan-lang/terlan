use super::*;
use crate::terlan_hir::{
    parse_interface_file, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
};

/// Verifies remote public signatures preserve fully qualified alias returns.
///
/// Inputs:
/// - A provider interface exposing `compare` with a generic contained type
///   and a fully qualified `ordering.Comparison` callback/return type.
/// - A consumer module that calls `option.compare` with `Option[Int]`
///   shapes and compares the result with `Atom["lt"]`.
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
pub type Comparison = Atom[\"lt\"] | Atom[\"eq\"] | Atom[\"gt\"].\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse ordering fixture: {:?}", err));
    let option = parse_interface_module_as_syntax_output(
        "\
module option.\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], T}.\n\
pub constructor None {\n\
    (): Option[T] -> Atom[\"none\"]\n\
}.\n\
pub constructor Some[T] {\n\
    (value: T): Option[T] -> {Atom[\"some\"], value}\n\
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
pub compare_int(left: Int, right: Int): Comparison -> Atom[\"lt\"].\n\
pub demo(): Bool ->\n\
    option.compare(None, Some(1), compare_int) == Atom[\"lt\"].\n\
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

/// Verifies imported HKT-generic functions preserve explicit type arguments.
///
/// Inputs:
/// - Provider interface exposing `identity[F[_], A]`.
/// - Consumer module with a local `Option[T]` alias calling
///   `hkt_provider.identity[Option, Int](value)`.
///
/// Output:
/// - Test passes when the imported function accepts the explicit constructor
///   and element type arguments at the module boundary.
///
/// Transformation:
/// - Builds provider interfaces through HIR, resolves a consumer against them,
///   and checks that imported function typechecking seeds callable generic
///   parameters before parsing `F[A]`.
#[test]
fn syntax_output_imported_hkt_generic_function_uses_explicit_type_args() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module hkt_provider.\n\
\n\
pub identity[F[_], A](value: F[A]): F[A].\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse hkt provider fixture: {:?}", err));

    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let provider_interface = interfaces
        .get(&provider.module_name)
        .expect("provider interface");
    let identity_signature = provider_interface
        .functions
        .get(&("identity".to_string(), 1))
        .expect("identity signature");
    assert_eq!(identity_signature.generic_params, vec!["F[_]", "A"]);

    let consumer = parse_module_as_syntax_output(
        "\
module hkt_consumer.\n\
\n\
import hkt_provider.{identity}.\n\
\n\
pub type None = Atom[\"none\"].\n\
pub type Some[T] = {Atom[\"some\"], value: T}.\n\
pub type Option[T] = None | Some[T].\n\
\n\
pub demo(value: Option[Int]): Option[Int] ->\n\
    identity[Option, Int](value).\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse hkt consumer fixture: {:?}", err));

    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported HKT function variance accepts matching constructors.
///
/// Inputs:
/// - Provider interface exposing `keep[F[+_], A]`.
/// - Consumer module supplying a covariant local `Box[+T]` constructor.
///
/// Output:
/// - Test passes when the imported call accepts `Box` as the explicit HKT
///   constructor argument.
///
/// Transformation:
/// - Confirms callable generic parameter metadata survives HIR/interface
///   rendering and is available during imported function call checking.
#[test]
fn syntax_output_imported_hkt_generic_function_accepts_covariant_type_arg() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module hkt_variance_provider.\n\
\n\
pub keep[F[+_], A](value: F[A]): F[A].\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse hkt provider fixture: {:?}", err));

    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let provider_interface = interfaces
        .get(&provider.module_name)
        .expect("provider interface");
    let keep_signature = provider_interface
        .functions
        .get(&("keep".to_string(), 1))
        .expect("keep signature");
    assert_eq!(keep_signature.generic_params, vec!["F[+_]", "A"]);

    let consumer = parse_module_as_syntax_output(
        "\
module hkt_variance_consumer_ok.\n\
\n\
import hkt_variance_provider.{keep}.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub demo(value: Box[Int]): Box[Int] ->\n\
    keep[Box, Int](value).\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse hkt consumer fixture: {:?}", err));

    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported HKT function variance rejects mismatched constructors.
///
/// Inputs:
/// - Provider interface exposing `keep[F[+_], A]`.
/// - Consumer module supplying invariant local `Cell[T]`.
///
/// Output:
/// - Test passes when the imported call reports the explicit type-argument
///   variance mismatch.
///
/// Transformation:
/// - Locks HKT variance enforcement across module boundaries instead of only
///   local function calls.
#[test]
fn syntax_output_imported_hkt_generic_function_rejects_invariant_type_arg() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module hkt_variance_provider_bad.\n\
\n\
pub keep[F[+_], A](value: F[A]): F[A].\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse hkt provider fixture: {:?}", err));

    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let provider_interface = interfaces
        .get(&provider.module_name)
        .expect("provider interface");
    let keep_signature = provider_interface
        .functions
        .get(&("keep".to_string(), 1))
        .expect("keep signature");
    assert_eq!(keep_signature.generic_params, vec!["F[+_]", "A"]);

    let consumer = parse_module_as_syntax_output(
        "\
module hkt_variance_consumer_bad.\n\
\n\
import hkt_variance_provider_bad.{keep}.\n\
\n\
pub opaque type Cell[T] = {value: T}.\n\
\n\
pub demo(value: Cell[Int]): Cell[Int] ->\n\
    keep[Cell, Int](value).\n\
",
    )
    .unwrap_or_else(|err| panic!("failed to parse hkt consumer fixture: {:?}", err));

    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("explicit type argument `Cell` for `F[+_]` requires slot 1 to be covariant")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies checked std summaries expose `std.core.Option.compare` correctly.
///
/// Inputs:
/// - A consumer fixture resolved against the exact checked-in std interface
///   closure used by its qualified calls.
///
/// Output:
/// - Test passes when `std.core.Option.compare` typechecks as returning an
///   ordering atom domain instead of the contained option value type.
///
/// Transformation:
/// - Loads the checked-in Option dependency closure, resolves a consumer
///   module against it, and typechecks a release-style assertion using
///   `Option.compare(None, Some(1), compare_int)`.
#[test]
fn syntax_output_std_option_compare_summary_preserves_comparison_return_type() {
    let interface_scope = parse_module_as_syntax_output(
        "module option_summary_scope.\n\nimport std.core.Option.\nimport std.core.Ordering.\nimport std.core.Int.\nimport std.test.Test.\n\npub main(): Int -> 1.\n",
    )
    .expect("parse option summary interface scope");
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
    let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&interface_scope);
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
    let empty_struct_schemes = HashMap::new();
    let empty_struct_field_visibility = HashMap::new();
    let empty_receiver_methods = HashMap::new();
    let empty_trait_method_calls = HashMap::new();
    let empty_trait_bound_impls = HashMap::new();
    let empty_negative_trait_impls = HashMap::new();
    let empty_trait_signatures = HashMap::new();
    let empty_alias_names = HashSet::new();
    let trial_trait_cache = RefCell::new(TraitLookupCache::default());
    let trial_ctx = ExprInferContext {
        database_schema: None,
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
        struct_schemes: &empty_struct_schemes,
        struct_field_visibility: &empty_struct_field_visibility,
        receiver_methods: &empty_receiver_methods,
        trait_method_calls: &empty_trait_method_calls,
        trait_bound_impl_type_args: &empty_trait_bound_impls,
        negative_trait_impl_type_args: &empty_negative_trait_impls,
        trait_signatures: &empty_trait_signatures,
        alias_names: &empty_alias_names,
        current_bounds: &[],
        current_constructor_target: None,
        trait_lookup_cache: &trial_trait_cache,
        effectful_calls: EffectfulCallFacts::default(),
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
    (value: Dynamic): Dynamic -> {Atom[\"some\"], value}\n\
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

/// Verifies wildcard imports expose public provider functions as local calls.
///
/// Inputs:
/// - A provider interface with one public module-level function.
/// - A consumer using `import math.Tools.{*}.` and calling the function by
///   local name.
///
/// Output:
/// - Test passes when the local call typechecks through the provider
///   interface.
///
/// Transformation:
/// - Expands the wildcard into selected function imports during import-map
///   construction and reuses normal imported-function inference.
#[test]
fn syntax_output_wildcard_function_imports_typecheck_local_calls() {
    let interface_source = "\
module math.Tools.\n\
pub inc(value: Int): Int.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module wildcard_function_consumer.\n\
import math.Tools.{*}.\n\
pub demo(): Int ->\n\
    inc(1).\n\
",
        interface_source,
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected wildcard function import diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies collapsed uppercase imports expose module aliases.
///
/// Inputs:
/// - A provider interface under `std.collections.List`.
/// - A consumer using `import std.collections.{List}.` and calling
///   `List.new()`.
///
/// Output:
/// - Test passes when the selected uppercase import binds `List` as a module
///   alias for remote-call typechecking.
///
/// Transformation:
/// - Expands braced uppercase import items into the same module alias map used
///   by bare imports such as `import std.collections.List.`.
#[test]
fn syntax_output_collapsed_uppercase_imports_typecheck_module_alias_calls() {
    let interface_source = "\
module std.collections.List.\n\
pub type List[T] = Dynamic.\n\
pub new[T](): List[T].\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module collapsed_module_alias_consumer.\n\
import std.collections.{List}.\n\
pub demo(): List[Int] ->\n\
    List.new().\n\
",
        interface_source,
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected collapsed module alias diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies collapsed uppercase trait imports do not become module aliases.
///
/// Inputs:
/// - A provider interface under `std.collections.Enumerable` exposing a trait.
/// - A consumer using `import std.collections.{Enumerable}.` and calling
///   `Enumerable.each`.
///
/// Output:
/// - Test passes when the selected uppercase import keeps trait-call
///   resolution instead of binding `Enumerable` as a remote module alias.
///
/// Transformation:
/// - Expands collapsed default imports far enough to detect that
///   `std.collections.Enumerable.Enumerable` is a trait import, so module alias
///   collection skips it.
#[test]
fn syntax_output_collapsed_uppercase_trait_imports_keep_trait_calls() {
    let interface_source = "\
module std.collections.Enumerable.\n\
pub type List[T] = Dynamic.\n\
pub trait Enumerable[C[_]] {\n\
    each[T](collection: C[T], cb: (T) -> Unit): Unit.\n\
}.\n\
pub impl Enumerable[List] for List {\n\
    each(collection: List[T], cb: (T) -> Unit): Unit.\n\
}.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module collapsed_trait_import_consumer.\n\
import std.collections.{Enumerable}.\n\
pub type List[T] = Dynamic.\n\
pub value(collection: List[Int]): Unit ->\n\
    Enumerable.each(collection, (_value) -> Unit).\n\
",
        interface_source,
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected collapsed trait import diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies selected imported function defaults participate in typechecking.
///
/// Inputs:
/// - A provider interface declaring a function with two trailing defaults.
/// - A consumer importing that function and omitting the middle defaulted
///   parameter while supplying the final parameter by name.
///
/// Output:
/// - Test passes when the selected import typechecks as a full-arity call
///   after defaulted parameter completion.
///
/// Transformation:
/// - Resolves the selected import through the provider interface, validates
///   named arguments against provider parameter names, and completes omitted
///   defaulted slots before ordinary overload inference.
#[test]
fn syntax_output_selected_function_imports_accept_omitted_defaults() {
    let interface_source = "\
module text_tools.\n\
pub decorate(first: String, middle: String = \".\", last: String = \"!\"): String.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module text_tools_consumer.\n\
import text_tools.{decorate}.\n\
pub demo(): String ->\n\
    decorate(first = \"A\", last = \"?\").\n\
",
        interface_source,
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported valued-union defaults preserve one nominal identity.
///
/// Inputs:
/// - A provider interface exporting a represented valued union and a function
///   whose trailing parameter defaults to one of its arms.
/// - A consumer importing both declarations and calling the function with the
///   omitted default and with another qualified arm.
///
/// Output:
/// - Both calls typecheck without a same-name nominal mismatch.
///
/// Transformation:
/// - Resolves parameter annotations, default metadata, and qualified arm
///   constants through the same provider-owned type identity.
#[test]
fn syntax_output_imported_valued_union_default_preserves_nominal_identity() {
    let interface_source = "\
module layout_provider.\n\
pub type MemoryOrder: Int = C = 0 | FORTRAN = 1.\n\
pub eye(size: Int, order: MemoryOrder = MemoryOrder.C): Int.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module layout_consumer.\n\
import layout_provider.{MemoryOrder, eye}.\n\
pub defaults(): Int -> eye(3).\n\
pub explicit(): Int -> eye(3, order = MemoryOrder.FORTRAN).\n\
",
        interface_source,
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies selected imported defaults do not hide missing required params.
///
/// Inputs:
/// - A provider interface declaring one required parameter and one default.
/// - A consumer call that supplies only the defaulted parameter by name.
///
/// Output:
/// - Test passes when typechecking reports the missing required argument.
///
/// Transformation:
/// - Computes supplied provider parameter slots and rejects required slots that
///   do not have defaults in the imported interface signature.
#[test]
fn syntax_output_selected_function_imports_reject_omitted_required_argument() {
    let interface_source = "\
module text_tools.\n\
pub decorate(first: String, suffix: String = \"!\"): String.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module text_tools_consumer.\n\
import text_tools.{decorate}.\n\
pub demo(): String ->\n\
    decorate(suffix = \"?\").\n\
",
        interface_source,
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("missing required argument `first` for call to `decorate`")),
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

/// Verifies imported singleton aliases inhabit transparent unions in calls.
///
/// Inputs:
/// - A provider exposing singleton atom aliases, a scalar union, an opaque
///   nested type, and functions accepting the combined outer union.
/// - A consumer importing the singleton values and selected functions.
///
/// Output:
/// - Test passes when singleton values select scalar overloads and satisfy the
///   combined union without exposing the opaque type representation.
///
/// Transformation:
/// - Exercises interface alias qualification and nested transparent-union
///   expansion at selected imported call boundaries.
#[test]
fn syntax_output_imported_singletons_satisfy_nested_transparent_unions() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module data_type_consumer.\n\
import data_types.{Int64, is_integer, list_type}.\n\
pub scalar(): Bool -> is_integer(Int64).\n\
pub nested(): data_types.Nested -> list_type(Int64).\n\
",
        "\
module data_types.\n\
pub type Int64 = Atom[\"int64\"].\n\
pub type Float64 = Atom[\"float64\"].\n\
pub type Scalar = Int64 | Float64.\n\
pub opaque type Nested = String.\n\
pub type AnyDataType = Scalar | Nested.\n\
pub (_value: AnyDataType) is_integer(): Bool.\n\
pub list_type(_value: Scalar): Nested.\n\
pub list_type(_value: Nested): Nested.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
