
/// Verifies local trait impl dispatch works for imported HKT constructors.
///
/// Inputs:
/// - A provider interface exporting a binary opaque `Map[K, V]` constructor.
/// - A consumer module defining and implementing `SecondMap[F[_, _]]` for
///   that imported constructor.
/// - A `SecondMap[Map].map` call over `Map[Binary, Int]`.
///
/// Output:
/// - No diagnostics; candidate filtering and final method inference both use
///   the provider-qualified `provider_map.Map` type.
///
/// Transformation:
/// - Prevents trait dispatch from accepting an imported constructor candidate
///   during matching and then rejecting the same candidate during function
///   unification because its scheme still contains the local import name.
#[test]
fn syntax_output_resolves_local_hkt_trait_impl_for_imported_constructor() {
    let interface_source = "\
module provider_map.\n\
\n\
pub opaque type Map[K, V].\n\
\n\
pub new[K, V](): Map[K, V].\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_constructor_hkt_trait_impl_dispatch.\n\
\n\
import provider_map.{Map, new}.\n\
\n\
import type provider_map.Map.\n\
\n\
pub trait SecondMap[F[_, _]] {\n\
    map[K, V, U](value: F[K, V], f: (V) -> U): F[K, U].\n\
}.\n\
\n\
pub impl SecondMap[Map] for Map {\n\
    map(value: Map[K, V], f: (V) -> U): Map[K, U] ->\n\
        new().\n\
}.\n\
\n\
pub inc(value: Int): Int ->\n\
    value + 1.\n\
\n\
pub demo(value: Map[Binary, Int]): Map[Binary, Int] ->\n\
    SecondMap[Map].map(value, inc).\n\
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

/// Verifies imported type aliases are canonicalized on both sides of an impl.
#[test]
fn syntax_output_accepts_short_imported_types_in_qualified_trait_impl() {
    let interface_source = "\
module provider_box.\n\
\n\
pub opaque type Box[T].\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_trait_impl_aliases.\n\
\n\
import type provider_box.{Box}.\n\
\n\
pub trait Visit[C, T] {\n\
    visit(collection: C, callback: (T) -> Box[T]): Box[T].\n\
}.\n\
\n\
pub impl Visit[provider_box.Box[T], T] for provider_box.Box[T] {\n\
    visit(collection: Box[T], callback: (T) -> Box[T]): Box[T] ->\n\
        collection.\n\
}.\n\
",
        interface_source,
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies a resolved negative fact is accepted without granting conformance.
#[test]
fn syntax_output_accepts_resolved_negative_trait_impl() {
    let diagnostics = check_syntax_output(
        "\
module negative_trait_impl_resolved.\n\
\n\
pub opaque type SecretKey.\n\
\n\
pub trait JsonEncode[T] {\n\
    encode(value: T): String.\n\
}.\n\
\n\
pub impl not JsonEncode[SecretKey].\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies visible names nested inside a generic denial target resolve.
#[test]
fn syntax_output_accepts_nested_negative_trait_impl_target() {
    let diagnostics = check_syntax_output(
        "module nested_negative_target.\n\
pub opaque type SecretKey.\n\
pub opaque type Box[T].\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl not JsonEncode[Box[SecretKey]].\n",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies negative facts resolve only declared traits.
#[test]
fn syntax_output_rejects_unknown_negative_trait_impl() {
    let diagnostics = check_syntax_output(
        "module negative_unknown_trait.\n\
pub opaque type SecretKey.\n\
pub impl not JsonEncode[SecretKey].\n",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == "unknown trait `JsonEncode` in negative impl"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies unknown names nested inside a negative target are rejected.
#[test]
fn syntax_output_rejects_unknown_negative_trait_impl_target() {
    let diagnostics = check_syntax_output(
        "module negative_unknown_target.\n\
pub opaque type Box[T].\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl not JsonEncode[Box[Missing]].\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "unknown type `Missing` in negative impl target `Box[Missing]`"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported trait and type names resolve through ordinary imports.
#[test]
fn syntax_output_accepts_imported_negative_trait_impl_target() {
    let diagnostics = check_syntax_output_with_interface(
        "module imported_negative_target.\n\
import negative_provider.{SecretKey}.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl not JsonEncode[SecretKey].\n",
        "module negative_provider.\n\
pub opaque type SecretKey.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies a public provider denial blocks a consumer-side positive impl.
#[test]
fn syntax_output_rejects_positive_impl_conflicting_with_imported_negative_impl() {
    let diagnostics = check_syntax_output_with_interface(
        "module imported_negative_conflict.\n\
import negative_provider.{JsonEncode, SecretKey}.\n\
pub impl JsonEncode[SecretKey] for SecretKey {\n\
    encode(value: SecretKey): String -> \"secret\".\n\
}.\n",
        "module negative_provider.\n\
pub opaque type SecretKey.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl not JsonEncode[SecretKey].\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "conflicting positive and imported negative trait impls for `JsonEncode[negative_provider.SecretKey] for negative_provider.SecretKey`"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies private provider denials remain isolated to their defining module.
#[test]
fn syntax_output_accepts_positive_impl_when_imported_negative_impl_is_private() {
    let diagnostics = check_syntax_output_with_interface(
        "module private_negative_isolation.\n\
import negative_provider.{JsonEncode, SecretKey}.\n\
pub impl JsonEncode[SecretKey] for SecretKey {\n\
    encode(value: SecretKey): String -> \"secret\".\n\
}.\n",
        "module negative_provider.\n\
pub opaque type SecretKey.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
impl not JsonEncode[SecretKey].\n",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies an imported positive conformance blocks a local negative fact.
#[test]
fn syntax_output_rejects_negative_impl_conflicting_with_imported_positive_impl() {
    let diagnostics = check_syntax_output_with_interface(
        "module imported_positive_conflict.\n\
import positive_provider.{JsonEncode, SecretKey}.\n\
pub impl not JsonEncode[SecretKey].\n",
        "module positive_provider.\n\
pub opaque type SecretKey.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl JsonEncode[SecretKey] for SecretKey {\n\
    encode(value: SecretKey): String.\n\
}.\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "conflicting imported positive and negative trait impls for `JsonEncode[positive_provider.SecretKey] for positive_provider.SecretKey`"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported denial metadata never grants trait method dispatch.
#[test]
fn syntax_output_rejects_method_dispatch_from_imported_negative_impl() {
    let diagnostics = check_syntax_output_with_interface(
        "module imported_negative_dispatch.\n\
import negative_provider.{JsonEncode, SecretKey}.\n\
pub encode(value: SecretKey): String -> JsonEncode.encode(value).\n",
        "module negative_provider.\n\
pub opaque type SecretKey.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl not JsonEncode[SecretKey].\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("no impl for trait method JsonEncode.encode")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies the compact negative syntax is limited to unary trait facts.
#[test]
fn syntax_output_rejects_non_unary_negative_trait_impl() {
    let diagnostics = check_syntax_output(
        "module negative_trait_arity.\n\
pub opaque type SecretKey.\n\
pub trait Convert[From, To] { convert(value: From): To. }.\n\
pub impl not Convert[SecretKey].\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "negative impl trait `Convert` expects 2 type parameter(s), found 1"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies repeated denial facts are rejected deterministically.
#[test]
fn syntax_output_rejects_duplicate_negative_trait_impl() {
    let diagnostics = check_syntax_output(
        "module duplicate_negative_trait.\n\
pub opaque type SecretKey.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl not JsonEncode[SecretKey].\n\
pub impl not JsonEncode[SecretKey].\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "duplicate negative trait impl for `JsonEncode[SecretKey] for SecretKey`"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies a type cannot both implement and deny the same trait instance.
#[test]
fn syntax_output_rejects_positive_negative_trait_impl_conflict() {
    let diagnostics = check_syntax_output(
        "module conflicting_negative_trait.\n\
pub opaque type SecretKey.\n\
pub trait JsonEncode[T] { encode(value: T): String. }.\n\
pub impl JsonEncode[SecretKey] for SecretKey {\n\
    encode(value: SecretKey): String -> \"secret\".\n\
}.\n\
pub impl not JsonEncode[SecretKey].\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message
        == "conflicting positive and negative trait impls for `JsonEncode[SecretKey] for SecretKey`"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}
