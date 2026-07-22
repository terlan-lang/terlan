
/// Verifies HKT slot covariance accepts a covariant constructor.
///
/// Inputs:
/// - A covariant alias `Box[+T]`.
/// - A trait requiring a unary covariant constructor `Producer[F[+_]]`.
///
/// Output:
/// - Test passes when `Producer[Box]` produces no kind or variance diagnostic.
///
/// Transformation:
/// - Exercises source-level HKT slot variance metadata on trait applications.
#[test]
fn syntax_output_accepts_covariant_hkt_constructor_argument_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_covariant_good.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub trait Producer[F[+_]] {\n\
    produce[A](value: F[A]): F[A].\n\
}.\n\
\n\
pub good(value: Producer[Box]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HKT slot covariance rejects an invariant constructor.
///
/// Inputs:
/// - An invariant alias `Cell[T]`.
/// - A trait requiring a unary covariant constructor `Producer[F[+_]]`.
///
/// Output:
/// - Test passes when `Producer[Cell]` reports a covariance mismatch.
///
/// Transformation:
/// - Makes `F[+_]` semantically meaningful instead of only a parsed marker.
#[test]
fn syntax_output_rejects_invariant_hkt_constructor_for_covariant_slot_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_covariant_bad.\n\
\n\
pub opaque type Cell[T] = {value: T}.\n\
\n\
pub trait Producer[F[+_]] {\n\
    produce[A](value: F[A]): F[A].\n\
}.\n\
\n\
pub bad(value: Producer[Cell]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("Producer expects type argument 1 slot 1 to be covariant")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HKT slot contravariance accepts a contravariant constructor.
///
/// Inputs:
/// - A contravariant alias `Sink[-T]`.
/// - A trait requiring a unary contravariant constructor `Consumer[F[-_]]`.
///
/// Output:
/// - Test passes when `Consumer[Sink]` produces no kind or variance diagnostic.
///
/// Transformation:
/// - Exercises negative HKT slot variance on source trait applications.
#[test]
fn syntax_output_accepts_contravariant_hkt_constructor_argument_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_contravariant_good.\n\
\n\
pub opaque type Sink[-T] = {value: T}.\n\
\n\
pub trait Consumer[F[-_]] {\n\
    consume[A](value: F[A]): Unit.\n\
}.\n\
\n\
pub good(value: Consumer[Sink]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HKT slot contravariance rejects a covariant constructor.
///
/// Inputs:
/// - A covariant alias `Box[+T]`.
/// - A trait requiring a unary contravariant constructor `Consumer[F[-_]]`.
///
/// Output:
/// - Test passes when `Consumer[Box]` reports a contravariance mismatch.
///
/// Transformation:
/// - Prevents `F[-_]` from being parsed as decoration without semantic force.
#[test]
fn syntax_output_rejects_covariant_hkt_constructor_for_contravariant_slot_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_contravariant_bad.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub trait Consumer[F[-_]] {\n\
    consume[A](value: F[A]): Unit.\n\
}.\n\
\n\
pub bad(value: Consumer[Box]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("Consumer expects type argument 1 slot 1 to be contravariant")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_hkt_parameter_application_arity_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_param_arity_bad.\n\
\n\
pub trait Functor[F[_]] {\n\
    bad[A, B](value: F[A, B]): Int.\n\
}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("type constructor `F` expects 1 type argument(s), found 2")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_concrete_type_constructor_application_arity_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_concrete_arity_bad.\n\
\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], value: T}.\n\
\n\
pub bad(value: Option[Int, String]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("type constructor `Option` expects 1 type argument(s), found 2")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_trait_application_arity_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_trait_arity_bad.\n\
\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], value: T}.\n\
\n\
pub trait Functor[F[_]] {\n\
    map[A, B](value: F[A], f: (A) -> B): F[B].\n\
}.\n\
\n\
pub bad(value: Functor[Option, Option]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("type constructor `Functor` expects 1 type argument(s), found 2")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_public_constructor_returning_private_type() {
    let diagnostics = check_syntax_output(
        "\
module public_constructor_private_return.\n\
\n\
struct Secret {\n\
    value: Int\n\
}.\n\
\n\
pub constructor Secret {\n\
    (value: Int): Secret -> value\n\
}.\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("public constructor Secret exposes private return type Secret")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies uppercase boolean spellings are not built-in values.
///
/// Inputs:
/// - A syntax-output module returning undeclared `True` and `False`.
///
/// Output:
/// - Diagnostics explaining that uppercase spellings must be declared or
///   replaced with lowercase literals.
///
/// Transformation:
/// - Runs the formal syntax-output typechecker and rejects unresolved
///   constructor-style boolean names instead of letting them widen to
///   `Dynamic`.
#[test]
fn syntax_output_rejects_undeclared_uppercase_boolean_spellings() {
    let diagnostics = check_syntax_output(
        "\
module uppercase_boolean_spellings.\n\
\n\
pub yes(): Bool ->\n\
    True.\n\
\n\
pub no(): Bool ->\n\
    False.\n\
",
    );

    assert!(
            diagnostics.iter().any(|diag| diag.message
                == "`True` is not a built-in boolean literal; use lowercase `true` or declare `True` explicitly"),
            "diagnostics: {:?}",
            diagnostics
        );
    assert!(
            diagnostics.iter().any(|diag| diag.message
                == "`False` is not a built-in boolean literal; use lowercase `false` or declare `False` explicitly"),
            "diagnostics: {:?}",
            diagnostics
        );
}

/// Verifies lowercase `unit` is not the built-in unit value.
///
/// Inputs:
/// - A syntax-output module returning lowercase `unit` from a `Unit`-typed
///   function.
///
/// Output:
/// - A typecheck diagnostic for the return-type mismatch.
///
/// Transformation:
/// - Treats lowercase `unit` as an ordinary atom-like source expression
///   rather than as the compiler-owned `Unit` singleton.
#[test]
fn syntax_output_rejects_lowercase_unit_as_builtin_value() {
    let diagnostics = check_syntax_output(
        "\
module lowercase_unit_value.\n\
\n\
pub value(): Unit ->\n\
    unit.\n\
",
    );

    assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message
                    == "`unit` is not a built-in unit value; use uppercase `Unit`"),
            "diagnostics: {:?}",
            diagnostics
        );
}
