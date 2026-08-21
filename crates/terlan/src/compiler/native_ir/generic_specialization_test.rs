//! Bounded generic monomorphization checks for the native application closure.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::lower_syntax_module_output_to_core;

use super::NativeModule;

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
