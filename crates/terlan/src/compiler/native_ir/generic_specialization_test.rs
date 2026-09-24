//! Bounded generic monomorphization checks for the native application closure.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::lower_syntax_module_output_to_core;

use super::NativeModule;

/// String relational operators must use UTF-8 content, not managed-reference
/// words, and their runtime comparison atoms need no explicit source import.
#[test]
fn native_string_ordering_operators_use_utf8_semantics_without_imports() {
    let syntax = parse_module_as_syntax_output(r#"
module primitive_string_ordering.
less(left: String, right: String): Bool -> left < right.
less_equal(left: String, right: String): Bool -> left <= right.
greater(left: String, right: String): Bool -> left > right.
greater_equal(left: String, right: String): Bool -> left >= right.
pub less_test(): Bool -> less("é", "🙂") and less("", "x") and not less("é", "é") and not less("z", "a").
pub less_equal_test(): Bool -> less_equal("é", "🙂") and less_equal("é", "é") and not less_equal("z", "a").
pub greater_test(): Bool -> greater("🙂", "é") and not greater("é", "é") and not greater("a", "z").
pub greater_equal_test(): Bool -> greater_equal("🙂", "é") and greater_equal("é", "é") and not greater_equal("a", "z").
"#).expect("parse string ordering operators");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower string operators");
    let object = super::emit_native_application_object("string-ordering", &modules)
        .expect("emit string operators");
    use super::native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    };
    let invocations = [
        "less_test",
        "less_equal_test",
        "greater_test",
        "greater_equal_test",
    ]
    .map(|name| {
        let export = modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("string operator export");
        NativeObjectInvocation {
            export_id: export.export_id,
            arguments: vec![],
            expected_status: super::status::OK,
            expected_result: Some(1),
        }
    });
    assert_managed_native_object_invocations("string-ordering", &modules, &object, &invocations);
}

/// Callback receivers retain contextual primitive types and cannot bind to an
/// unrelated local function or a shadowed outer variable with the same name.
#[test]
fn generic_callback_primitive_receivers_keep_type_and_lexical_identity() {
    use crate::terlan_hir::{
        checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
    };
    let syntax = parse_module_as_syntax_output(
        r#"
module callback_primitive_receivers.
import std.vm.{Bytes, BitString}.
apply[T, R](value: T, transform: (T) -> R): R -> transform(value).
byte_size(value: Int): Int -> value + 100.
pub direct(value: String): Int -> value.byte_size().
pub callback(): Int -> apply("éx", (value) -> value.byte_size()).
pub shadowed(value: Int): Int -> apply("éx", (value) -> value.byte_size()).
pub ordinary(value: Int): Int -> byte_size(value).
pub bool_callback(): String -> apply(true, (value) -> value.to_string()).
pub float_callback(): String -> apply(1.5, (value) -> value.to_string()).
pub int_callback(): String -> apply(42, (value) -> value.to_string()).
pub bytes_callback(): Int -> apply(Bytes.from_list([1, 2]), (value) -> value.length()).
pub bits_callback(): Int -> apply(BitString.from_int_be(3, 5), (value) -> value.bit_length()).
"#,
    )
    .expect("parse contextual receiver source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = crate::terlan_typeck::type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules =
        NativeModule::lower_application(&[&core]).expect("lower contextual primitive receivers");
    let object = super::emit_native_application_object("callback-primitive-receivers", &modules)
        .expect("emit contextual receivers");
    use super::native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    };
    let invocations = [
        ("callback", vec![], 3),
        ("shadowed", vec![17], 3),
        ("ordinary", vec![17], 117),
        ("bytes_callback", vec![], 2),
        ("bits_callback", vec![], 5),
    ]
    .map(|(name, arguments, expected)| {
        let export = modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("receiver export");
        NativeObjectInvocation {
            export_id: export.export_id,
            arguments,
            expected_status: super::status::OK,
            expected_result: Some(expected),
        }
    });
    assert_managed_native_object_invocations(
        "callback-primitive-receivers",
        &modules,
        &object,
        &invocations,
    );
}

/// Generic clones retain executable same-module helpers after qualification.
#[test]
fn generic_specialization_can_call_private_local_helpers() {
    let syntax = parse_module_as_syntax_output(
        "module generic_local_calls.\n\
         offset(): Int -> 7.\n\
         choose[T](value: T): Int -> offset() + 1.\n\
         pub run(): Int -> choose(true).\n",
    )
    .expect("parse generic local call");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower qualified local call");
    let object = super::emit_native_application_object("generic-local-calls", &modules)
        .expect("emit qualified local call");
    let export = modules[0]
        .functions
        .iter()
        .find(|function| function.name == "run")
        .expect("run export");
    use super::native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    };
    assert_managed_native_object_invocations(
        "generic-local-calls",
        &modules,
        &object,
        &[NativeObjectInvocation {
            export_id: export.export_id,
            arguments: vec![],
            expected_status: super::status::OK,
            expected_result: Some(8),
        }],
    );
}

#[test]
fn private_generic_helper_is_replaced_by_concrete_native_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module generic_native.\n\n\
         identity[T](value: T): T -> value.\n\n\
         pub run(value: Int): Int -> identity(value).\n",
    )
    .expect("parse generic source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("generic NativeIR");
    let names = modules[0]
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();
    assert!(!names.contains(&"identity"));
    assert!(names
        .iter()
        .any(|name| name.starts_with("$aot_generic_") && name.contains("identity")));
    assert!(names.contains(&"run"));
}

/// Verifies operators retain their typed result during generic specialization.
///
/// Inputs:
/// - Comparison and arithmetic expressions passed directly to a generic helper.
///
/// Output:
/// - Concrete Bool and Int native specializations.
///
/// Transformation:
/// - Uses operator result semantics instead of requiring a literal or variable
///   at the generic call boundary.
#[test]
fn operator_results_drive_generic_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module generic_operator_native.\n\n\
         identity[T](value: T): T -> value.\n\n\
         pub compare(value: Int): Bool -> identity(value > 1).\n\n\
         pub increment(value: Int): Int -> identity(value + 1).\n",
    )
    .expect("parse generic operator source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("generic operator NativeIR");
    let specializations = modules[0]
        .functions
        .iter()
        .filter(|function| {
            function.name.starts_with("$aot_generic_") && function.name.contains("identity")
        })
        .collect::<Vec<_>>();

    assert!(specializations.iter().any(|function| {
        function.params == vec![super::NativeType::Bool]
            && function.return_type == super::NativeType::Bool
    }));
    assert!(specializations.iter().any(|function| {
        function.params == vec![super::NativeType::Int]
            && function.return_type == super::NativeType::Int
    }));
}

/// Verifies singleton atoms can drive concrete generic specialization.
///
/// Inputs:
/// - A singleton atom alias passed to one private generic helper.
///
/// Output:
/// - A concrete specialization whose argument retains the atom-literal type.
///
/// Transformation:
/// - Prevents singleton values from becoming uninferable merely because their
///   runtime representation is the shared atom word.
#[test]
fn singleton_atom_argument_drives_generic_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module generic_atom_native.\n\n\
         pub type Ready = Atom[\"ready\"].\n\n\
         identity[T](value: T): T -> value.\n\n\
         pub run(): Ready -> identity(Ready).\n",
    )
    .expect("parse generic atom source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("generic atom NativeIR");
    let names = modules[0]
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();
    let specialization = modules[0]
        .functions
        .iter()
        .find(|function| {
            function.name.starts_with("$aot_generic_") && function.name.contains("identity")
        })
        .unwrap_or_else(|| panic!("concrete atom specialization; functions: {names:?}"));

    assert_eq!(specialization.params, vec![super::NativeType::Atom]);
    assert_eq!(specialization.return_type, super::NativeType::Atom);
}

/// Verifies a text literal retains the checked concrete parameter type while
/// the remaining arguments drive generic specialization.
///
/// Inputs:
/// - A concrete Binary label followed by a generic String value.
///
/// Output:
/// - NativeIR lowers the call with a String specialization instead of
///   rejecting the label as an inferred String.
///
/// Transformation:
/// - Applies the concrete parameter context before the literal's default text
///   type participates in generic unification.
#[test]
fn concrete_binary_parameter_contextualizes_literal_during_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module generic_binary_label.\n\n\
         keep[T](label: Binary, value: T): T -> value.\n\n\
         pub run(): String -> keep(\"label\", \"value\").\n",
    )
    .expect("parse contextual Binary literal source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules =
        NativeModule::lower_application(&[&core]).expect("contextual Binary literal NativeIR");

    assert!(modules[0].functions.iter().any(|function| {
        function.name.starts_with("$aot_generic_")
            && function.name.contains("keep")
            && function.params == vec![super::NativeType::BinaryRef, super::NativeType::StringRef]
            && function.return_type == super::NativeType::StringRef
    }));
}

/// Verifies overload inventories retain every same-name callable.
///
/// Inputs:
/// - Two concrete overloads with one shared return type.
/// - A let-bound overloaded call passed to a generic helper.
///
/// Output:
/// - NativeIR contains the concrete String specialization of the helper.
///
/// Transformation:
/// - Resolves the overloaded call by its argument type before using its return
///   type to specialize the generic consumer.
#[test]
fn overloaded_call_return_drives_let_bound_generic_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module overloaded_generic_native.\n\n\
         render(value: Int): String -> \"int\".\n\
         render(value: String): String -> value.\n\n\
         identity[T](value: T): T -> value.\n\n\
         pub run(): String ->\n\
             let rendered = render(7);\n\
             identity(rendered).\n",
    )
    .expect("parse overloaded generic source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("overloaded generic NativeIR");
    let specialization = modules[0]
        .functions
        .iter()
        .find(|function| {
            function.name.starts_with("$aot_generic_") && function.name.contains("identity")
        })
        .expect("String specialization");

    assert_eq!(specialization.params, vec![super::NativeType::StringRef]);
    assert_eq!(specialization.return_type, super::NativeType::StringRef);
}

/// Verifies common overload results survive nominal argument aliases.
///
/// Inputs:
/// - Two constructor overloads whose argument aliases are intentionally opaque
///   to CoreIR generic inference.
/// - The let-bound constructor result passed to a generic native function.
///
/// Output:
/// - NativeIR contains the concrete String specialization.
///
/// Transformation:
/// - Uses the common concrete return type after normal type checking has
///   already selected a valid overload.
#[test]
fn common_overload_return_drives_let_bound_generic_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module common_overload_return.\n\n\
         pub opaque type Scalar = String.\n\
         pub opaque type Nested = String.\n\n\
         wrap(value: Scalar): Nested -> value.\n\
         wrap(value: Nested): Nested -> value.\n\n\
         identity[T](value: T): T -> value.\n\n\
         pub run(value: Scalar): Nested ->\n\
             let nested = wrap(value);\n\
             identity(nested).\n",
    )
    .expect("parse common overload return source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules =
        NativeModule::lower_application(&[&core]).expect("common overload return NativeIR");
    let specialization = modules[0]
        .functions
        .iter()
        .find(|function| {
            function.name.starts_with("$aot_generic_") && function.name.contains("identity")
        })
        .expect("String specialization");

    assert_eq!(specialization.params, vec![super::NativeType::StringRef]);
    assert_eq!(specialization.return_type, super::NativeType::StringRef);
}

/// Verifies a specialized generic result remains available to later calls.
///
/// Inputs:
/// - A generic converter returning a concrete nominal type.
/// - Its let-bound result passed to a second generic helper.
///
/// Output:
/// - Both generic calls specialize with a String native ABI.
///
/// Transformation:
/// - Captures the binding type before the first call is renamed to its generated
///   specialization symbol.
#[test]
fn let_bound_generic_result_drives_later_generic_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module chained_generic_native.\n\n\
         pub opaque type Nested = String.\n\n\
         convert[T](value: T): Nested -> \"nested\".\n\
         identity[T](value: T): T -> value.\n\n\
         pub run(): Nested ->\n\
             let nested = convert(7);\n\
             identity(nested).\n",
    )
    .expect("parse chained generic source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("chained generic NativeIR");
    let specializations = modules[0]
        .functions
        .iter()
        .filter(|function| function.name.starts_with("$aot_generic_"))
        .collect::<Vec<_>>();

    assert_eq!(specializations.len(), 2);
    assert!(specializations.iter().any(|function| {
        function.name.contains("identity")
            && function.params == vec![super::NativeType::StringRef]
            && function.return_type == super::NativeType::StringRef
    }));
}

/// Verifies imported-style `Result` constructors bind generic payload types.
///
/// Inputs:
/// - A concrete `Result[String, Int]` value.
/// - `Ok` and `Err` constructor patterns whose payloads feed generic helpers.
///
/// Output:
/// - String and Int specializations are both emitted.
///
/// Transformation:
/// - Propagates each `Result` type argument through its matching constructor.
#[test]
fn result_constructor_patterns_drive_generic_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module result_pattern_generic.\n\n\
         import std.core.Result.{Err, Ok, Result}.\n\n\
         identity[T](value: T): T -> value.\n\n\
         pub run(value: Result[String, Int]): String ->\n\
             case value {\n\
                 Ok(text) -> identity(text);\n\
                 Err(code) ->\n\
                     let _copied = identity(code);\n\
                     \"error\"\n\
             }.\n",
    )
    .expect("parse Result pattern source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("Result pattern NativeIR");
    let specializations = modules[0]
        .functions
        .iter()
        .filter(|function| {
            function.name.starts_with("$aot_generic_") && function.name.contains("identity")
        })
        .collect::<Vec<_>>();

    assert!(specializations
        .iter()
        .any(|function| function.params == vec![super::NativeType::StringRef]));
    assert!(specializations
        .iter()
        .any(|function| function.params == vec![super::NativeType::Int]));
}

/// Verifies a generic call can supply a typed `Result` case scrutinee.
///
/// Inputs:
/// - A generic producer returning `Result[T, String]`.
/// - Its direct result matched before the payload enters another generic call.
///
/// Output:
/// - The consumer receives a concrete Int specialization.
///
/// Transformation:
/// - Captures the scrutinee type before rewriting the producer call to a
///   generated specialization symbol.
#[test]
fn generic_result_scrutinee_drives_payload_specialization() {
    let syntax = parse_module_as_syntax_output(
        "module generic_result_scrutinee.\n\n\
         import std.core.Result.{Err, Ok, Result}.\n\n\
         produce[T](value: T): Result[T, String] -> Ok(value).\n\
         identity[T](value: T): T -> value.\n\n\
         pub run(): Int ->\n\
             case produce(7) {\n\
                 Ok(value) -> identity(value);\n\
                 Err(_error) -> 0\n\
             }.\n",
    )
    .expect("parse generic Result scrutinee source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut specialized_core = core.clone();
    super::generic_specialization::specialize_application_generics_with_budget(
        std::slice::from_mut(&mut specialized_core),
        &mut super::specialization_budget::SpecializationBudget::default(),
    )
    .expect("specialize generic Result scrutinee");
    let identity_parameter = specialized_core
        .functions
        .iter()
        .find(|function| {
            function.name.starts_with("$aot_generic_") && function.name.contains("identity")
        })
        .and_then(|function| function.params.first())
        .and_then(|parameter| parameter.core_ty.as_ref());
    assert_eq!(
        identity_parameter,
        Some(&crate::terlan_typeck::CoreType::Int),
        "specialized core={}",
        specialized_core.contract_text()
    );
    let modules =
        NativeModule::lower_application(&[&core]).expect("generic Result scrutinee NativeIR");

    assert!(
        modules[0].functions.iter().any(|function| {
            function.name.starts_with("$aot_generic_")
                && function.name.contains("identity")
                && function.params == vec![super::NativeType::Int]
        }),
        "core={}; native={:?}",
        core.contract_text(),
        modules[0]
            .functions
            .iter()
            .map(|function| (&function.name, &function.params))
            .collect::<Vec<_>>()
    );
}

/// Verifies concrete call parameters contextualize aggregate arguments.
///
/// Inputs:
/// - A function accepting `List[Option[Int]]`.
/// - A mixed `Some`/`None` list literal passed directly at the call site.
///
/// Output:
/// - The CoreIR argument is cast to its checked aggregate parameter type.
///
/// Transformation:
/// - Preserves the type context needed by NativeIR before eager argument
///   lowering.
#[test]
fn concrete_parameter_context_is_attached_to_option_list_argument() {
    let syntax = parse_module_as_syntax_output(
        "module contextual_option_list.\n\n\
         import std.core.Option.{None, Option, Some}.\n\n\
         consume(values: List[Option[Int]]): Int -> 1.\n\n\
         pub run(): Int -> consume([Some(1), None]).\n",
    )
    .expect("parse contextual Option list source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    super::generic_specialization::specialize_application_generics_with_budget(
        std::slice::from_mut(&mut core),
        &mut super::specialization_budget::SpecializationBudget::default(),
    )
    .expect("contextualize Option list");
    let argument = core
        .functions
        .iter()
        .find(|function| function.name == "run")
        .and_then(|function| function.clauses.first())
        .and_then(|clause| clause.body.core_expr.as_ref())
        .and_then(|body| match body {
            crate::terlan_typeck::CoreExpr::Call { args, .. } => args.first(),
            _ => None,
        })
        .expect("run call argument");

    assert!(matches!(
        argument,
        crate::terlan_typeck::CoreExpr::Cast {
            target_type: crate::terlan_typeck::CoreType::List(_),
            ..
        }
    ));
}

/// Verifies public generic declarations remain compile-time templates.
#[test]
fn unused_public_generic_export_has_no_open_native_abi() {
    let syntax = parse_module_as_syntax_output(
        "module generic_export.\n\npub identity[T](value: T): T -> value.\n",
    )
    .expect("parse generic export");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("generic template NativeIR");

    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .all(|function| function.name != "identity"));
}

/// Verifies local nominal types are not inferred as undeclared type variables.
#[test]
fn public_nominal_export_is_not_misclassified_as_generic() {
    let syntax = parse_module_as_syntax_output(
        "module nominal_export.\n\n\
         pub struct Pair { left: Int, right: Int }.\n\n\
         pub identity(value: Pair): Pair -> value.\n",
    )
    .expect("parse nominal export");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower nominal export");

    assert!(modules[0]
        .functions
        .iter()
        .any(|function| function.name == "identity"));
}

/// Verifies a concrete named callback specializes a generic union before a
/// nullary constructor argument is checked against that union.
#[test]
fn named_callback_specializes_nullary_generic_union_arguments() {
    let syntax = parse_module_as_syntax_output(
        "module generic_option_compare.\n\n\
         pub type None.\n\n\
         pub type Some[T] = {Atom[\"some\"], value: T}.\n\n\
         pub type Option[T] = None | Some[T].\n\n\
         compare[T](left: Option[T], right: Option[T], callback: (T, T) -> Int): Int ->\n\
             case left { None -> 0; Some(value) -> callback(value, value) }.\n\n\
         compare_int(left: Int, right: Int): Int -> left - right.\n\n\
         pub run(): Int -> compare(None, None, compare_int).\n",
    )
    .expect("parse generic option comparison");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core])
        .expect("named callback should specialize the option payload type");

    assert!(modules[0]
        .functions
        .iter()
        .any(|function| function.name == "run"));
}

#[test]
fn generic_specialization_budget_fails_before_native_linking() {
    let mut source =
        String::from("module generic_budget.\n\nidentity[T](value: T): T -> value.\n\n");
    for index in 0..=128 {
        source.push_str(&format!(
            "pub struct Value{index} {{ value: Int }}.\n\n\
             pub use_{index}(value: Value{index}): Value{index} -> identity(value).\n\n"
        ));
    }
    let syntax = parse_module_as_syntax_output(&source).expect("parse generic budget source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let error = NativeModule::lower_application(&[&core]).expect_err("reject generic explosion");

    assert!(
        error.starts_with("error[native_ir.generic_budget]"),
        "{error}"
    );
}
