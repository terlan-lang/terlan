use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::callee_scalar_replacement::specialize_projection_callees;
use super::{NativeBinaryOperator, NativeExpr, NativeModule};

/// Produces checked CoreIR for one callee-specialization fixture.
fn checked_core(source: &str) -> crate::terlan_typeck::CoreModule {
    let syntax = parse_module_as_syntax_output(source).expect("callee-specialization source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    lower_syntax_module_output_to_core(&syntax, &resolved)
}

/// Returns a module with one private projection helper and two call shapes.
fn projection_source() -> &'static str {
    "\
module callee_projection.\n\
\n\
pub struct Pair {\n\
    left: Int,\n\
    right: Int\n\
}.\n\
\n\
pub constructor Pair {\n\
    (left: Int, right: Int): Pair ->\n\
        Pair {left: left, right: right}\n\
}.\n\
\n\
sum_pair(pair: Pair): Int ->\n\
    pair.left + pair.right.\n\
\n\
pub direct(): Int ->\n\
    sum_pair(Pair(20, 22)).\n\
\n\
pub local(): Int ->\n\
    let pair = Pair(30, 12);\n\
    sum_pair(pair).\n"
}

/// Removes a projection-only private helper after rewriting all direct calls.
#[test]
fn private_projection_callee_is_removed_from_backend_core() {
    let mut cores = vec![checked_core(projection_source())];
    specialize_projection_callees(&mut cores).expect("specialize projection callee");

    assert!(!cores[0]
        .functions
        .iter()
        .any(|function| function.name == "sum_pair"));
    assert!(cores[0]
        .functions
        .iter()
        .any(|function| function.name == "direct"));
    assert!(cores[0]
        .functions
        .iter()
        .any(|function| function.name == "local"));
}

/// Lowers constructor and local call arguments without managed allocation.
#[test]
fn private_projection_calls_lower_to_scalar_native_ir() {
    let core = checked_core(projection_source());
    let modules = NativeModule::lower_application(&[&core]).expect("native application");
    let direct = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "direct")
        .expect("direct native function");
    let local = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "local")
        .expect("local native function");

    for function in [direct, local] {
        assert!(matches!(
            function.body,
            NativeExpr::Let { ref bindings, ref body }
                if bindings.len() == 2
                    && matches!(body.as_ref(), NativeExpr::Binary {
                        operator: NativeBinaryOperator::Add,
                        ..
                    })
        ));
        assert_eq!(function.params, Vec::new());
    }
}

/// Keeps public projection helpers on their declared managed ABI.
#[test]
fn public_projection_callee_is_not_specialized() {
    let source = projection_source().replace("sum_pair(pair: Pair)", "pub sum_pair(pair: Pair)");
    let mut cores = vec![checked_core(&source)];
    specialize_projection_callees(&mut cores).expect("specialize projection callee");

    assert!(cores[0]
        .functions
        .iter()
        .any(|function| function.name == "sum_pair" && function.public));
}

/// Keeps helpers that expose the complete aggregate identity.
#[test]
fn aggregate_identity_callee_is_not_specialized() {
    let mut cores = vec![checked_core(
        "\
module callee_identity.\n\
\n\
pub struct Pair { left: Int, right: Int }.\n\
pub constructor Pair {\n\
    (left: Int, right: Int): Pair -> Pair {left: left, right: right}\n\
}.\n\
identity(pair: Pair): Pair -> pair.\n\
pub direct(): Pair -> identity(Pair(20, 22)).\n",
    )];
    specialize_projection_callees(&mut cores).expect("specialize projection callee");

    assert!(cores[0]
        .functions
        .iter()
        .any(|function| function.name == "identity"));
}

/// Skips an unused candidate and still specializes a later callable helper.
#[test]
fn unused_projection_callee_does_not_block_later_candidate() {
    let source = projection_source().replace(
        "sum_pair(pair: Pair): Int ->",
        "unused(pair: Pair): Int -> pair.left.\n\nsum_pair(pair: Pair): Int ->",
    );
    let mut cores = vec![checked_core(&source)];
    specialize_projection_callees(&mut cores).expect("specialize projection callee");

    assert!(cores[0]
        .functions
        .iter()
        .any(|function| function.name == "unused"));
    assert!(!cores[0]
        .functions
        .iter()
        .any(|function| function.name == "sum_pair"));
}

/// Keeps a recursive projection helper when inlining leaves a recursive use.
#[test]
fn recursive_projection_callee_is_not_removed() {
    let mut cores = vec![checked_core(
        "\
module callee_recursive.\n\
\n\
pub struct Pair { left: Int, right: Int }.\n\
pub constructor Pair {\n\
    (left: Int, right: Int): Pair -> Pair {left: left, right: right}\n\
}.\n\
recursive(pair: Pair): Int -> pair.left + recursive(pair).\n\
pub direct(): Int -> recursive(Pair(20, 22)).\n",
    )];
    specialize_projection_callees(&mut cores).expect("specialize projection callee");

    assert!(cores[0]
        .functions
        .iter()
        .any(|function| function.name == "recursive"));
}
