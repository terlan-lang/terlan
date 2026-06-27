use super::test_support::*;
use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;

/// Verifies `Unit = Atom["unit"]` aliases can satisfy trait impl methods.
///
/// Inputs:
/// - A syntax-output module defining `Unit = Atom["unit"]`.
/// - A trait implementation for `Show[Unit]`.
/// - An impl method that passes its `Unit` parameter into a function whose
///   parameter is also annotated as `Unit`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Typechecks explicit trait impl method bodies and confirms the named
///   `Unit` alias unifies with its singleton `Atom["unit"]` representation
///   without admitting lowercase `unit` as a source-level value.
#[test]
fn syntax_output_accepts_unit_alias_in_explicit_trait_impl_methods() {
    let diagnostics = check_syntax_output(
        "\
module unit_trait_impl_alias_bridge.\n\
pub type Unit = Atom[\"unit\"].\n\
\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub unit_to_string(value: Unit): String ->\n\
    \"unit\".\n\
\n\
pub impl Show[Unit] for Unit {\n\
    to_string(value: Unit): String ->\n\
        unit_to_string(value).\n\
}.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_trait_decls_on_formal_path() {
    let module = parse_module_as_syntax_output(
        "\
module trait_extends_bad.\n\
pub trait Derived[A] extends NoSuch[A] {\n\
    derived(value: A): A.\n\
}.\n\
",
    )
    .expect("parse syntax output trait diagnostic fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let trait_signatures = collect_syntax_trait_signatures(&module, &resolved);
    let diagnostics = check_syntax_trait_decls(&module, &trait_signatures);

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("unknown super trait")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_declared_implements_receiver_methods_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module declared_implements_ok.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) to_string(): String ->\n\
    user.name.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_inherited_receiver_method_satisfies_declared_implements() {
    let diagnostics = check_syntax_output(
        "\
module derived_receiver_trait_ok.\n\
\n\
pub trait Display[T] {\n\
    display(value: T): String.\n\
}.\n\
\n\
pub struct Error {\n\
    message: String\n\
}.\n\
\n\
pub (error: Error) display(): String ->\n\
    error.message.\n\
\n\
pub struct FileError includes Error implements Display[FileError] {\n\
    path: String\n\
}.\n\
\n\
pub show(error: FileError): String ->\n\
    Display.display(error).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_declared_implements_missing_required_method_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module declared_implements_missing_method.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("missing receiver method `to_string` for `User` implementing `Show`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_declared_implements_trait_default_methods_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module declared_implements_default_method.\n\
pub trait Show[T] {\n\
    to_string(value: T): String -> \"<value>\".\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_declared_implements_receiver_signature_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module declared_implements_signature_mismatch.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) to_string(): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "receiver method `to_string` return type for `User` expects String, found Int"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_resolves_declared_implements_trait_method_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module declared_implements_dispatch.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) to_string(): String ->\n\
    user.name.\n\
\n\
pub stringify(user: User): String ->\n\
    Show.to_string(user).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies mutable trait receiver requirements are enforced.
///
/// Inputs:
/// - A trait whose first method parameter is declared `mut`.
/// - One struct declaring `implements` with a matching mutable receiver
///   method.
///
/// Output:
/// - Test passes when declaration-site conformance accepts the mutable
///   receiver method.
///
/// Transformation:
/// - Checks that `mut` in trait parameter metadata participates in
///   receiver-method conformance validation instead of being treated as
///   documentation-only syntax.
#[test]
fn syntax_output_accepts_declared_implements_mutable_receiver_requirement() {
    let diagnostics = check_syntax_output(
        "\
module declared_implements_mut_receiver.\n\
\n\
pub trait IndexSet[C, I, T] {\n\
    set_at(mut collection: C, index: I, value: T): Unit.\n\
}.\n\
\n\
pub struct Bag implements IndexSet[Bag, Int, Int] {\n\
    value: Int\n\
}.\n\
\n\
pub (mut bag: Bag) set_at(index: Int, value: Int): Unit ->\n\
    bag.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies immutable receivers do not satisfy mutable trait requirements.
///
/// Inputs:
/// - A trait whose first method parameter is declared `mut`.
/// - A struct declaring `implements` with an immutable receiver method.
///
/// Output:
/// - Test passes when a diagnostic reports the missing mutable receiver.
///
/// Transformation:
/// - Prevents source contracts such as `IndexSet` from being implemented
///   by read-only receiver methods when bracket assignment later relies on
///   mutable receiver rebinding semantics.
#[test]
fn syntax_output_rejects_declared_implements_missing_mutable_receiver() {
    let diagnostics = check_syntax_output(
        "\
module declared_implements_missing_mut_receiver.\n\
\n\
pub trait IndexSet[C, I, T] {\n\
    set_at(mut collection: C, index: I, value: T): Unit.\n\
}.\n\
\n\
pub struct Bag implements IndexSet[Bag, Int, Int] {\n\
    value: Int\n\
}.\n\
\n\
pub (bag: Bag) set_at(index: Int, value: Int): Unit ->\n\
    Unit.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("must use a mutable receiver")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generic trait bounds supply local trait-method evidence.
///
/// Inputs:
/// - Two source modules: one generic function with an `Eq` bound and one
///   generic function without that bound.
///
/// Output:
/// - The bounded module produces no diagnostics.
/// - The unbounded module reports a missing trait implementation at the
///   trait-method call site.
///
/// Transformation:
/// - Exercises syntax-output typechecking so `Eq.equal(Left, Right)` can be
///   checked from the active function bound without synthesizing a global
///   implementation candidate.
#[test]
fn syntax_output_uses_generic_bounds_for_trait_method_dispatch_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module generic_trait_bound_dispatch.\n\
pub trait Eq[A] {\n\
    equal(left: A, right: A): Bool.\n\
}.\n\
\n\
pub is_same[A](left: A, right: A)[Eq[A]]: Bool ->\n\
    Eq.equal(left, right).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );

    let missing_bound = check_syntax_output(
        "\
module generic_trait_bound_dispatch_missing.\n\
pub trait Eq[A] {\n\
    equal(left: A, right: A): Bool.\n\
}.\n\
\n\
pub is_same[A](left: A, right: A): Bool ->\n\
    Eq.equal(left, right).\n\
",
    );

    assert!(
        missing_bound
            .iter()
            .any(|diag| diag.message.contains("no impl for trait method Eq.equal")),
        "diagnostics: {:?}",
        missing_bound
    );
}

#[test]
fn syntax_output_checks_explicit_trait_impl_methods_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_trait_impl_ok.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
    to_string(value: ExternalUser): String ->\n\
        value.name.\n\
}.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_explicit_trait_impl_missing_required_method_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_trait_impl_missing_method.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("missing method `to_string` in impl of trait `Show`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_explicit_trait_impl_default_methods_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_trait_impl_default_method.\n\
pub trait Show[T] {\n\
    to_string(value: T): String -> \"<value>\".\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
}.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_imported_trait_impl_default_methods_on_formal_path() {
    let interface_source = "\
module lifecycle.\n\
pub trait Lifecycle[T] {\n\
    start(value: T): T.\n\
    stop(value: T): Unit -> terlan_interface_default.\n\
}.\n";
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_trait_default_method.\n\
import lifecycle.{Lifecycle}.\n\
\n\
pub struct Worker implements Lifecycle[Worker] {\n\
    value: Int\n\
}.\n\
\n\
pub (worker: Worker) start(): Worker ->\n\
    worker.\n\
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
fn syntax_output_accepts_std_beam_gen_server_without_default_terminate() {
    let diagnostics = check_syntax_output_with_std_interfaces(
            "\
module beam_gen_server_default_terminate.\n\
\n\
import std.beam.GenServer.{GenServer, CallReply}.\n\
import std.core.Result.{Result, Ok}.\n\
import std.core.Error.{Error}.\n\
\n\
pub struct CounterServer implements GenServer[CounterServer, Int, Int, Int, Int] {\n\
    seed: Int\n\
}.\n\
\n\
pub (server: CounterServer) init(): Result[Int, Error] ->\n\
    Ok(server.seed).\n\
\n\
pub (server: CounterServer) handle_call(state: Int, request: Int): Result[CallReply[Int, Int], Error] ->\n\
    Ok({state, request}).\n\
\n\
pub (server: CounterServer) handle_cast(state: Int, event: Int): Result[Int, Error] ->\n\
    Ok(state + event).\n\
",
            "std/beam/gen_server.terl",
        );

    assert!(
        diagnostics.is_empty(),
        "unexpected std.beam.GenServer diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_explicit_trait_impl_signature_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_trait_impl_signature_mismatch.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
    to_string(value: ExternalUser): Int ->\n\
        1.\n\
}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("method `to_string` return type in trait `Show` expects String, found Int")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_explicit_trait_impl_body_return_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_trait_impl_body_mismatch.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Show[ExternalUser] for ExternalUser {\n\
    to_string(value: ExternalUser): String ->\n\
        1.\n\
}.\n\
",
    );

    assert!(
        diagnostics.iter().any(
            |diag| diag.message.contains("expected Binary") && diag.message.contains("found 1")
        ),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_duplicate_declared_and_explicit_trait_impl_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_trait_impl_duplicate_pair.\n\
pub trait Show[T] {\n\
    to_string(value: T): String.\n\
}.\n\
\n\
pub struct User implements Show[User] {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) to_string(): String ->\n\
    user.name.\n\
\n\
pub impl Show[User] for User {\n\
    to_string(value: User): String ->\n\
        value.name.\n\
}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("coherent impl conflict for `Show[User] for User`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_resolves_explicit_trait_impl_method_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_trait_impl_dispatch.\n\
pub trait Identity[T] {\n\
    id(value: T): T.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub impl Identity[ExternalUser] for ExternalUser {\n\
    id(value: ExternalUser): ExternalUser ->\n\
        value.\n\
}.\n\
\n\
pub roundtrip(value: ExternalUser): ExternalUser ->\n\
    Identity.id(value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HKT trait implementations specialize constructor parameters.
///
/// Inputs:
/// - A unary higher-kinded trait `Functor[F[_]]`.
/// - A concrete `Option` type constructor implementing that trait.
/// - A trait method call returning `Option[String]`.
///
/// Output:
/// - No diagnostics; `F[A]` in the trait method specializes to
///   `Option[Int]`, and `F[B]` specializes to `Option[String]`.
///
/// Transformation:
/// - Exercises explicit trait impl dispatch with a higher-kinded constructor
///   argument so raw `F[_]` parameter text cannot leak into method parsing.
#[test]
fn syntax_output_resolves_explicit_hkt_trait_impl_method_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_trait_impl_dispatch.\n\
\n\
pub type None = Atom[\"none\"].\n\
pub type Some[T] = {Atom[\"some\"], value: T}.\n\
pub type Option[T] = None | Some[T].\n\
\n\
pub trait Functor[F[_]] {\n\
    map[A, B](value: F[A], f: (A) -> B): F[B].\n\
}.\n\
\n\
pub impl Functor[Option] for Option {\n\
    map(value: Option[A], f: (A) -> B): Option[B] ->\n\
        case value {\n\
            None -> None;\n\
            Some(x) -> Some(f(x))\n\
        }.\n\
}.\n\
\n\
pub render(value: Int): String ->\n\
    \"value\".\n\
\n\
pub demo(value: Option[Int]): Option[String] ->\n\
    Functor.map(value, render).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported HKT trait implementations specialize provider types.
///
/// Inputs:
/// - A provider interface exposing `Functor[F[_]]`, provider-owned `Option`,
///   and a public `Functor[Option] for Option` conformance.
/// - A consumer module importing the trait, provider option types, and a
///   provider function that returns `Option[Int]`.
///
/// Output:
/// - No diagnostics; `Functor.map` sees the imported conformance and returns
///   the provider-owned `Option[String]`.
///
/// Transformation:
/// - Exercises interface-summary conformance import, provider-local type
///   qualification, HKT constructor substitution, and trait-method dispatch in
///   one cross-module call.
#[test]
fn syntax_output_resolves_imported_hkt_trait_impl_method_calls_on_formal_path() {
    let interface_source = "\
module hkt_functor_provider.\n\
\n\
pub type None = Atom[\"none\"].\n\
pub type Some[T] = {Atom[\"some\"], value: T}.\n\
pub type Option[T] = None | Some[T].\n\
\n\
pub trait Functor[F[_]] {\n\
    map[A, B](value: F[A], f: (A) -> B): F[B].\n\
}.\n\
\n\
pub impl Functor[Option] for Option {\n\
    map(value: Option[A], f: (A) -> B): Option[B].\n\
}.\n\
\n\
pub sample(): Option[Int].\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_hkt_trait_impl_dispatch.\n\
\n\
import hkt_functor_provider.{Functor, Option, sample}.\n\
\n\
pub render(value: Int): String ->\n\
    \"value\".\n\
\n\
pub demo(): Option[String] ->\n\
    Functor.map(sample(), render).\n\
",
        interface_source,
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HKT trait inheritance supports monadic abstractions.
///
/// Inputs:
/// - `Functor`, `Applicative`, and `Monad` traits parameterized by a unary
///   type constructor.
/// - An `Option` implementation of `Monad`.
/// - A `Monad.flat_map` call from source.
///
/// Output:
/// - No diagnostics; inherited HKT traits and the concrete `Option`
///   implementation remain type-correct.
///
/// Transformation:
/// - Locks the standard advanced-FP hierarchy shape before the same contracts
///   are exposed from std.
#[test]
fn syntax_output_resolves_hkt_monad_trait_hierarchy_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_monad_trait_hierarchy.\n\
\n\
pub type None = Atom[\"none\"].\n\
pub type Some[T] = {Atom[\"some\"], value: T}.\n\
pub type Option[T] = None | Some[T].\n\
\n\
pub trait Functor[F[_]] {\n\
    map[A, B](value: F[A], f: (A) -> B): F[B].\n\
}.\n\
\n\
pub trait Applicative[F[_]] extends Functor[F] {\n\
    pure[A](value: A): F[A].\n\
    apply[A, B](f: F[(A) -> B], value: F[A]): F[B].\n\
}.\n\
\n\
pub trait Monad[F[_]] extends Applicative[F] {\n\
    flat_map[A, B](value: F[A], f: (A) -> F[B]): F[B].\n\
}.\n\
\n\
pub impl Monad[Option] for Option {\n\
    map(value: Option[A], f: (A) -> B): Option[B] ->\n\
        case value {\n\
            None -> None;\n\
            Some(x) -> Some(f(x))\n\
        }.\n\
\n\
    pure(value: A): Option[A] ->\n\
        Some(value).\n\
\n\
    apply(f: Option[(A) -> B], value: Option[A]): Option[B] ->\n\
        case f {\n\
            None -> None;\n\
            Some(unwrapped) ->\n\
                case value {\n\
                    None -> None;\n\
                    Some(x) -> Some(unwrapped(x))\n\
                }\n\
        }.\n\
\n\
    flat_map(value: Option[A], f: (A) -> Option[B]): Option[B] ->\n\
        case value {\n\
            None -> None;\n\
            Some(x) -> f(x)\n\
        }.\n\
}.\n\
\n\
pub positive(value: Int): Option[Int] ->\n\
    Some(value).\n\
\n\
pub demo(value: Option[Int]): Option[Int] ->\n\
    Monad.flat_map(value, positive).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_explicit_trait_impl_method_call_without_impl_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_trait_impl_dispatch_missing.\n\
pub trait Identity[T] {\n\
    id(value: T): T.\n\
}.\n\
\n\
pub struct ExternalUser {\n\
    name: String\n\
}.\n\
\n\
pub roundtrip(value: ExternalUser): ExternalUser ->\n\
    Identity.id(value).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("no impl for trait method Identity.id")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_implements_trait_methods_are_synthesized_for_calls() {
    let diagnostics = check_syntax_output(
        "\
module implements_trait_calls.
pub trait Show[A] {
    show(value: A): Binary.
}.

pub struct User implements Show[User] {
    id: Int
}.

pub (user: User) show(): Binary ->
    \"user\".

pub describe(value: User): Binary ->
    Show.show(value).
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}
