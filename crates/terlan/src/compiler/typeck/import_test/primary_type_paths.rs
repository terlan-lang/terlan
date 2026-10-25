use super::*;

/// Qualified primary paths, aliases, and legacy paths share nominal identity.
#[test]
fn primary_type_paths_preserve_opaque_generic_identity() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module primary_consumer.
import type sample.Box as Container.
pub direct(value: Container[Int]): sample.Box[Int] -> value.
pub legacy(value: sample.Box[Int]): sample.Box.Box[Int] -> value.
pub nested(value: List[Container[Int]]): List[sample.Box[Int]] -> value.
pub secondary(value: sample.Box.Other): sample.Box.Other -> value.
"#,
        r#"
module sample.Box.
pub opaque type Box[T].
pub opaque type Other.
"#,
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// A short public type in an imported signature resolves back to its provider.
#[test]
fn primary_type_paths_round_trip_imported_signatures() {
    let diagnostics = super::super::test_support::check_syntax_output_with_interfaces(
        r#"
module primary_signature_consumer.
import sample.Factory.
import type sample.Box.
pub make(value: Int): Box[Int] -> Factory.make(value).
"#,
        &[
            "module sample.Box. pub opaque type Box[T].",
            "module sample.Factory. pub make(value: Int): sample.Box[Int].",
        ],
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// The convention also applies to package facades, without hiding secondary types.
#[test]
fn primary_type_paths_support_external_package_facades() {
    let diagnostics = super::super::test_support::check_syntax_output_with_interfaces(
        r#"
module package_consumer.
import type polars.DataFrame as Frame.
import type pytorch.Tensor.
pub frame(value: Frame): polars.DataFrame -> value.
pub tensor(value: Tensor): pytorch.Tensor -> value.
pub series(value: polars.DataFrame.Series): polars.DataFrame.Series -> value.
pub scalar(value: pytorch.Tensor.Scalar): pytorch.Tensor.Scalar -> value.
"#,
        &[
            "module polars.DataFrame. pub opaque type DataFrame. pub opaque type Series.",
            "module pytorch.Tensor. pub opaque type Tensor. pub opaque type Scalar.",
        ],
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Same-named primary types retain their distinct owning modules.
#[test]
fn primary_type_paths_reject_cross_provider_values() {
    let diagnostics = super::super::test_support::check_syntax_output_with_interfaces(
        r#"
module distinct_primary_consumer.
import type left.Box as Left.
import type right.Box as Right.
pub invalid(value: Left): right.Box -> value.
"#,
        &[
            "module left.Box. pub opaque type Box.",
            "module right.Box. pub opaque type Box.",
        ],
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("left.Box")
                && diagnostic.message.contains("right.Box")),
        "diagnostics: {diagnostics:?}"
    );
}

/// Primary paths work recursively inside their own module's struct fields.
#[test]
fn primary_type_paths_support_own_module_fields() {
    let diagnostics = check_syntax_output(
        r#"
module sample.Node.
pub struct Node { value: Int }.
pub struct Envelope { node: sample.Node }.
pub unwrap(value: Envelope): Node -> value.node.
"#,
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Interface and native signature admission accepts primary paths at their declared arity.
#[test]
fn primary_type_paths_validate_interface_signature_arities() {
    use crate::terlan_hir::{
        resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
    };
    use crate::terlan_syntax::parse_interface_module_as_syntax_output;
    let provider =
        parse_interface_module_as_syntax_output("module sample.Box. pub opaque type Box[T].")
            .expect("provider");
    let interfaces = std::collections::HashMap::from([(
        "sample.Box".to_string(),
        syntax_module_output_to_interface(&provider),
    )]);
    for (arguments, valid) in [("Int", true), ("Int, Int", false)] {
        let module = parse_interface_module_as_syntax_output(&format!("module sample.Api. import type sample.Box. pub echo(value: sample.Box[{arguments}]): sample.Box[{arguments}].")).expect("interface signature");
        let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&module, &resolved);
        if valid {
            assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
        } else {
            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic
                    .message
                    .contains("expects 1 argument(s), found 2")),
                "diagnostics: {diagnostics:?}"
            );
        }
    }
}

/// Explicit primary type arguments retain the local opaque identity.
#[test]
fn primary_type_paths_preserve_own_opaque_type_arguments() {
    let diagnostics = check_syntax_output(
        r#"
module sample.Box.
pub opaque type Box = Int.
identity[T](value: T): T -> value.
pub echo(value: Box): Box -> identity[sample.Box](value).
"#,
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Primary paths retain their constructor kind in higher-kinded trait implementations.
#[test]
fn primary_type_paths_preserve_higher_kinded_constructor_metadata() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module primary_hkt.
import type sample.Box.
pub trait Keep[F[_]] { keep[T](value: F[T]): F[T]. }.
pub impl Keep[sample.Box] for sample.Box {
    keep[T](value: sample.Box[T]): sample.Box[T] -> value.
}.
"#,
        "module sample.Box. pub opaque type Box[T].",
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Primary constructor metadata must still reject a mismatched higher kind.
#[test]
fn primary_type_paths_reject_wrong_higher_kinded_constructor_arity() {
    let diagnostics = check_syntax_output_with_interface(
        r#"
module primary_hkt_mismatch.
import type sample.Box.
pub trait Keep[F[_, _]] { keep[T, U](value: F[T, U]): F[T, U]. }.
pub impl Keep[sample.Box] for sample.Box {
    keep[T](value: sample.Box[T]): sample.Box[T] -> value.
}.
"#,
        "module sample.Box. pub opaque type Box[T].",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("kind mismatch")
                && diagnostic
                    .message
                    .contains("found sample.Box of kind Type -> Type")),
        "diagnostics: {diagnostics:?}"
    );
}
