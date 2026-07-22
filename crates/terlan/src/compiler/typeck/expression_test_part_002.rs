
#[test]
fn syntax_output_accepts_pure_helper_list_comprehension_filter_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_helper_filter.\n\
\n\
is_visible(value: Int): Bool ->\n\
    value > 0 and value < 10.\n\
\n\
pub values(items: List[Int]): List[Int] ->\n\
    [x | x <- items, is_visible(x)].\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies an imported effect execution cannot masquerade as a pure
/// comprehension filter even when it returns `Bool`.
#[test]
fn syntax_output_rejects_effectful_imported_list_comprehension_filter() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module syntax_list_effectful_import_filter.\n\
\n\
import std.core.Effect.{Effect, run, succeed}.\n\
\n\
pub existing(paths: List[String]): List[String] ->\n\
    [path | path <- paths, run(succeed(path == path))].\n\
",
        "std/core/Effect.terl",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "list comprehension filter must be pure; found effectful imported function call"
            )),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies local helper inference propagates effect execution into a
/// comprehension filter.
#[test]
fn syntax_output_rejects_transitively_effectful_list_comprehension_filter() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module syntax_list_transitive_effect_filter.\n\
\n\
import std.core.Effect.{Effect, run, succeed}.\n\
\n\
is_existing(path: String): Bool ->\n\
    run(succeed(path == path)).\n\
\n\
pub existing(paths: List[String]): List[String] ->\n\
    [path | path <- paths, is_existing(path)].\n\
",
        "std/core/Effect.terl",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "list comprehension filter must be pure; found effectful local function call"
            )),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn syntax_output_accepts_iterable_list_comprehension_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_iterable_comprehension.
pub type Iterator[T] = List[T].

pub trait Iterable[C, T] {
    iterator(collection: C): Iterator[T].
}.

pub struct IntCollection implements Iterable[IntCollection, Int] {
    values: List[Int]
}.

pub (collection: IntCollection) iterator(): Iterator[Int] ->
    collection.values.

pub values(items: IntCollection): List[Int] ->
    [value | value <- items, value > 0].
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_non_bool_list_comprehension_filter_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_filter_type.\n\
pub values(items: List[Int]): List[Int] ->\n\
    [x | x <- items, x].\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("list comprehension filter")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_list_comprehension_non_list_source_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_list_source.\n\
pub inc_all(value: Int): List[Int] ->\n\
    [x + 1 | x <- value].\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message
                .contains("list comprehension source must be List or Iterable")
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_range_source_in_list_comprehension_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_range_comprehension_source.\n\
pub inc_all(): List[Int] ->\n\
    [x + 1 | x <- 1..3].\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_local_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_call_inference.\n\
add_one(x: Int): Int ->\n\
    x + 1.\n\
pub inc_all(values: List[Int]): List[Int] ->\n\
    [add_one(x) | x <- values].\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported overloads resolve by argument type.
///
/// Inputs:
/// - A provider interface declaring two `pick/1` signatures with different
///   parameter and return types.
/// - A consumer module calling both overloads through a qualified import.
///
/// Output:
/// - Test passes when both calls typecheck against their declared return types.
///
/// Transformation:
/// - Parses the provider as an interface module so duplicate same-name
///   same-arity signatures are preserved in `ModuleInterface.function_overloads`,
///   then typechecks the consumer through ordinary remote-call inference.
#[test]
fn syntax_output_selects_imported_overloads_by_argument_type_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module overload.Consumer.\n\
\n\
import overload.Provider.\n\
\n\
pub int_value(): Int ->\n\
    Provider.pick(1).\n\
\n\
pub string_value(): String ->\n\
    Provider.pick(\"x\").\n\
",
        "\
module overload.Provider.\n\
\n\
pub pick(value: Int): Int.\n\
pub pick(value: String): String.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies local overload declarations resolve by argument type.
///
/// Inputs:
/// - A source module declaring two public `pick/1` functions with different
///   parameter and return types.
/// - Functions that call each overload through ordinary local call syntax.
///
/// Output:
/// - Test passes when both calls typecheck and HIR does not report a duplicate
///   function diagnostic for distinct overload shapes.
///
/// Transformation:
/// - Exercises parser output, HIR duplicate-shape filtering, type signature
///   candidate collection, and local call overload selection in one formal
///   source path.
#[test]
fn syntax_output_selects_local_overloads_by_argument_type_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module overload_local.\n\
\n\
pub pick(value: Int): Int ->\n\
    value.\n\
\n\
pub pick(value: String): String ->\n\
    value.\n\
\n\
pub int_value(): Int ->\n\
    pick(1).\n\
\n\
pub string_value(): String ->\n\
    pick(\"x\").\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies string concatenation accepts printable scalar operands.
///
/// Inputs:
/// - A source module returning `String` from `"index: " + index`.
///
/// Output:
/// - Test passes when typechecking reports no diagnostics.
///
/// Transformation:
/// - Exercises the user-facing display concatenation rule that keeps numeric
///   `+` numeric while allowing string-plus-scalar print-path expressions.
#[test]
fn syntax_output_accepts_string_concat_with_int_operand_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module string_concat_int_operand.\n\
\n\
pub label(index: Int): String ->\n\
    \"index: \" + index.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported receiver-method overloads resolve by receiver type.
///
/// Inputs:
/// - A provider interface declaring two `length/0` receiver methods on
///   different wrapper types.
/// - A consumer module importing those types and calling `value.length()`.
///
/// Output:
/// - Test passes when receiver-method dispatch selects the candidate whose
///   receiver type matches the call target.
///
/// Transformation:
/// - Exercises generated-style method overloads through imported interface
///   summaries, receiver-method dispatch collection, and method-call
///   typechecking.
#[test]
fn syntax_output_selects_imported_receiver_overloads_by_receiver_type_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module overload.Consumer.\n\
\n\
import type overload.Provider.{JsArray, JsString}.\n\
import overload.Provider.\n\
\n\
pub string_length(value: JsString): Int ->\n\
    value.length().\n\
\n\
pub array_length(value: JsArray): Int ->\n\
    value.length().\n\
",
        "\
module overload.Provider.\n\
\n\
pub type JsString.\n\
pub type JsArray.\n\
\n\
pub (value: JsString) length(): Int.\n\
pub (value: JsArray) length(): Int.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_standalone_expression_on_formal_path() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_expr_query.\n\
pub add_one(value: Int): Int ->\n\
    value + 1.\n\
",
    )
    .expect("parse syntax module");
    let resolved = resolve_syntax_module_output(&module).module;
    let expression = parse_expr_as_syntax_output("add_one(41)").expect("parse syntax expr");

    let (ty, diagnostics) = infer_syntax_expression_type(&expression, &module, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    assert_eq!(pretty_type(&ty), "Int");
}

/// Verifies bracket reads infer through `IndexGet`.
///
/// Inputs:
/// - A syntax-output module declaring `IndexGet[C, I, T]`.
/// - One struct and one explicit `IndexGet[IndexedBox, Int, Int]` impl.
/// - A function body indexing a value parameter.
///
/// Output:
/// - Test passes when the function body is reported as `Int` against an
///   intentionally wrong `String` return annotation.
///
/// Transformation:
/// - Exercises the compiler-owned desugaring contract that treats
///   `collection[index]` as a trait-backed `IndexGet.get_at(collection,
///   index)` lookup while keeping parser and CoreIR index syntax
///   collection-neutral.
#[test]
fn syntax_output_infers_index_read_through_index_get_trait() {
    let diagnostics = check_syntax_output(
        "\
module syntax_index_get_trait.\n\
\n\
pub trait IndexGet[C, I, T] {\n\
    get_at(collection: C, index: I): T.\n\
}.\n\
\n\
pub struct IndexedBox {\n\
    value: Int\n\
}.\n\
\n\
pub impl IndexGet[IndexedBox, Int, Int] for IndexedBox {\n\
    get_at(collection: IndexedBox, index: Int): Int ->\n\
        collection.value.\n\
}.\n\
\n\
pub read(value: IndexedBox): String ->\n\
    value[0].\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Binary found Int")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit type args on imported generic calls constrain results.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
/// - A function returning `Vector[String]` from `Vector.new[Int]()`.
///
/// Output:
/// - Test passes when typechecking reports the explicit `Int` argument as
///   incompatible with the declared `Vector[String]` return type.
///
/// Transformation:
/// - Loads checked-in std summaries and validates that `Call.type_args`
///   participates in generic interface-call inference instead of being parsed
///   only as syntax metadata.
#[test]
fn syntax_output_remote_generic_call_type_args_constrain_return_type() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module native.VectorGenericTypeArgs.\n\
\n\
import std.native.collections.Vector.\n\
\n\
pub wrong(): Vector[String] ->\n\
    Vector.new[Int]().\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Binary found Int")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies public AOT process calls cannot lose concrete type metadata.
#[test]
fn syntax_output_typed_process_operations_require_explicit_specialization() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module typed_process_specialization.\n\
\n\
import std.vm.Process.\n\
import std.vm.Message.\n\
import type std.vm.Process.{Entry, Monitor, Process, Resource, ResourceKind}.\n\
import type std.vm.Message.{Message}.\n\
\n\
pub struct Pair {left: Int, right: Int}.\n\
\n\
pub send(recipient: Process[Pair], payload: Message[Pair]): Unit ->\n\
    Process.send(recipient, payload).\n\
\n\
pub receive(): Message[Pair] ->\n\
    Process.receive().\n\
\n\
pub wrap(payload: Pair): Message[Pair] ->\n\
    Message.wrap(payload).\n\
\n\
pub unwrap(message: Message[Pair]): Pair ->\n\
    Message.unwrap(message).\n\
\n\
pub entry(tag: Int): Entry[Pair] ->\n\
    Process.entry(tag).\n\
\n\
pub spawn(entry: Entry[Pair]): Process[Pair] ->\n\
    Process.spawn(entry).\n\
\n\
pub link(peer: Process[Pair]): Unit ->\n\
    Process.link(peer).\n\
\n\
pub monitor(peer: Process[Pair]): Monitor[Pair] ->\n\
    Process.monitor(peer).\n\
\n\
pub resource_kind(tag: Int): ResourceKind[Pair] ->\n\
    Process.resource_kind(tag).\n\
\n\
pub acquire(kind: ResourceKind[Pair]): Resource[Pair] ->\n\
    Process.acquire(kind).\n\
\n\
pub cancel(target: Process[Pair]): Unit ->\n\
    Process.cancel(target).\n\
",
        "std/vm/Process.terl",
    );

    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic
                .message
                .contains("requires exactly one explicit concrete type argument"))
            .count(),
        11,
        "diagnostics: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|diagnostic| diagnostic
            .message
            .contains("requires exactly one explicit concrete type argument")),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// Verifies imported generic opaque lifecycle handles retain their arity.
#[test]
fn syntax_output_accepts_explicit_typed_process_lifecycle_operations() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module typed_process_lifecycle.\n\
\n\
import std.vm.Process.\n\
import type std.vm.Process.{Entry, ExitReason, Monitor, Process, Resource, ResourceKind, SchedulingClass, Timer}.\n\
\n\
pub spawn(entry: Entry[Int]): Process[Int] -> Process.spawn[Int](entry).\n\
pub sleep(timer: Timer): Unit -> Process.sleep(timer).\n\
pub link(peer: Process[Int]): Unit -> Process.link[Int](peer).\n\
pub monitor(peer: Process[Int]): Monitor[Int] -> Process.monitor[Int](peer).\n\
pub acquire(kind: ResourceKind[Int]): Resource[Int] -> Process.acquire[Int](kind).\n\
pub cancel(target: Process[Int]): Unit -> Process.cancel[Int](target).\n\
pub fail(reason: ExitReason): Unit -> Process.fail(reason).\n\
pub schedule(class: SchedulingClass): Unit -> Process.schedule(class).\n\
",
        "std/vm/Process.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

/// Verifies raw integers cannot cross typed process lifecycle boundaries.
#[test]
fn syntax_output_rejects_scalar_process_lifecycle_arguments() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module scalar_process_lifecycle.\n\
\n\
import std.vm.Process.\n\
\n\
pub spawn(): Unit -> let value = Process.spawn[Int](1); Unit.\n\
pub sleep(): Unit -> Process.sleep(1).\n\
pub link(): Unit -> Process.link[Int](1).\n\
pub monitor(): Unit -> let value = Process.monitor[Int](1); Unit.\n\
pub acquire(): Unit -> let value = Process.acquire[Int](1); Unit.\n\
pub cancel(): Unit -> Process.cancel[Int](1).\n\
pub fail(): Unit -> Process.fail(1).\n\
pub schedule(): Unit -> Process.schedule(1).\n\
",
        "std/vm/Process.terl",
    );

    for expected in [
        "expected std.vm.Process.Entry[Int] found 1",
        "expected std.vm.Process.Timer found 1",
        "expected std.vm.Process.Process[Int] found 1",
        "expected std.vm.Process.ResourceKind[Int] found 1",
        "expected std.vm.Process.ExitReason found 1",
        "expected std.vm.Process.SchedulingClass found 1",
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "missing `{expected}` in {diagnostics:?}"
        );
    }
}

/// Verifies explicit generic call arguments can bind HKT constructor params.
///
/// Inputs:
/// - A generic local function with a unary higher-kinded parameter `F[_]`.
/// - A concrete `Option` type constructor supplied as an explicit type
///   argument.
///
/// Output:
/// - No diagnostics; `F[A]` specializes to `Option[Int]`.
///
/// Transformation:
/// - Protects explicit call type-argument parsing from expanding a bare
///   constructor argument into its structural alias body before HKT
///   substitution can apply it.
#[test]
fn syntax_output_explicit_hkt_call_type_arg_binds_constructor_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_type_arg.\n\
\n\
pub type None = Atom[\"none\"].\n\
pub type Some[T] = {Atom[\"some\"], value: T}.\n\
pub type Option[T] = None | Some[T].\n\
\n\
pub identity[F[_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Option[Int]): Option[Int] ->\n\
    identity[Option, Int](value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit HKT call arguments honor covariant slot requirements.
///
/// Inputs:
/// - A generic function declaring `F[+_]`.
/// - A covariant `Box[+T]` constructor supplied explicitly.
///
/// Output:
/// - Test passes when the call typechecks.
///
/// Transformation:
/// - Exercises explicit call-site type argument validation against retained
///   callable generic parameter metadata.
#[test]
fn syntax_output_explicit_hkt_call_accepts_covariant_constructor_slot() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_covariant_ok.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub keep[F[+_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Box[Int]): Box[Int] ->\n\
    keep[Box, Int](value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit HKT call arguments reject invariant constructors.
///
/// Inputs:
/// - A generic function declaring `F[+_]`.
/// - An invariant `Cell[T]` constructor supplied explicitly.
///
/// Output:
/// - Test passes when the call reports the slot-variance mismatch.
///
/// Transformation:
/// - Prevents explicit type arguments from bypassing the variance contract that
///   trait applications already enforce.
#[test]
fn syntax_output_explicit_hkt_call_rejects_invariant_constructor_for_covariant_slot() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_covariant_bad.\n\
\n\
pub opaque type Cell[T] = {value: T}.\n\
\n\
pub keep[F[+_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Cell[Int]): Cell[Int] ->\n\
    keep[Cell, Int](value).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("explicit type argument `Cell` for `F[+_]` requires slot 1 to be covariant")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit HKT call arguments honor contravariant slot requirements.
///
/// Inputs:
/// - A generic function declaring `F[-_]`.
/// - A contravariant `Sink[-T]` constructor supplied explicitly.
///
/// Output:
/// - Test passes when the call typechecks.
///
/// Transformation:
/// - Covers the negative-variance explicit call path, complementing the
///   covariant slot tests.
#[test]
fn syntax_output_explicit_hkt_call_accepts_contravariant_constructor_slot() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_contravariant_ok.\n\
\n\
pub opaque type Sink[-T] = {value: T}.\n\
\n\
pub keep[F[-_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Sink[Int]): Sink[Int] ->\n\
    keep[Sink, Int](value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit HKT call arguments reject opposite variance.
///
/// Inputs:
/// - A generic function declaring `F[-_]`.
/// - A covariant `Box[+T]` constructor supplied explicitly.
///
/// Output:
/// - Test passes when the call reports the slot-variance mismatch.
///
/// Transformation:
/// - Ensures explicit HKT arguments cannot pass a producer-like constructor
///   into a consumer-like slot.
#[test]
fn syntax_output_explicit_hkt_call_rejects_covariant_constructor_for_contravariant_slot() {
    let diagnostics = check_syntax_output(
        "\
module explicit_hkt_call_contravariant_bad.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub keep[F[-_], A](value: F[A]): F[A] ->\n\
    value.\n\
\n\
pub demo(value: Box[Int]): Box[Int] ->\n\
    keep[Box, Int](value).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "explicit type argument `Box` for `F[-_]` requires slot 1 to be contravariant"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies source-declared covariance affects function-call assignability.
///
/// Inputs:
/// - `Box[+T]` alias.
/// - Function expecting `Box[Number]`.
/// - Caller passing `Box[Int]`.
///
/// Output:
/// - No diagnostics, because `Int <: Number` and `Box` is covariant.
///
/// Transformation:
/// - Exercises variance metadata collected from Terlan source declarations
///   through ordinary local call checking.
#[test]
fn syntax_output_accepts_covariant_alias_argument_widening_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module covariant_alias_call.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub accept(value: Box[Number]): Box[Number] ->\n\
    value.\n\
\n\
pub demo(value: Box[Int]): Box[Number] ->\n\
    accept(value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies unmarked generic aliases stay invariant.
///
/// Inputs:
/// - `Cell[T]` alias without a variance marker.
/// - Function expecting `Cell[Number]`.
/// - Caller passing `Cell[Int]`.
///
/// Output:
/// - Type diagnostic, because invariant parameters reject one-way widening.
///
/// Transformation:
/// - Protects the default generic assignability rule from becoming implicitly
///   covariant.
#[test]
fn syntax_output_rejects_invariant_alias_argument_widening_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module invariant_alias_call.\n\
\n\
pub opaque type Cell[T] = {value: T}.\n\
\n\
pub accept(value: Cell[Number]): Cell[Number] ->\n\
    value.\n\
\n\
pub demo(value: Cell[Int]): Cell[Number] ->\n\
    accept(value).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("expected Number but found Int/Float")),
        "expected invariant widening diagnostic, got: {:?}",
        diagnostics
    );
}

/// Verifies native vector constructor shorthand infers element type.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
/// - A function returning `Vector[String]` from `Vector("Alice", "Bob")`.
/// - A second function reading the first value through bracket indexing.
///
/// Output:
/// - Test passes when typechecking produces no diagnostics.
///
/// Transformation:
/// - Loads checked-in std summaries and validates that the explicit
///   constructor declaration on `Vector[T]` participates in vararg constructor
///   inference and the existing `IndexGet` bridge.
#[test]
fn syntax_output_vector_constructor_shorthand_infers_element_type() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module native.VectorConstructorShorthand.\n\
\n\
import std.native.collections.Vector.\n\
import type std.native.collections.Vector.Vector.\n\
\n\
pub values(): Vector[String] ->\n\
    Vector(\"Alice\", \"Bob\").\n\
\n\
pub first(): String ->\n\
    let users = Vector(\"Alice\", \"Bob\");\n\
    users[0].\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies empty native vector constructors can use return-type context.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
/// - A function returning `Vector[String]` from `Vector()`.
///
/// Output:
/// - Test passes when typechecking accepts the empty constructor because the
///   declared return type supplies the missing element type.
///
/// Transformation:
/// - Loads checked-in std summaries and validates that final return
///   unification can still resolve empty vararg constructor calls.
#[test]
fn syntax_output_empty_vector_constructor_uses_return_context() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module native.EmptyVectorConstructorReturnContext.\n\
\n\
import std.native.collections.Vector.\n\
import type std.native.collections.Vector.Vector.\n\
\n\
pub values(): Vector[String] ->\n\
    Vector().\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies empty portable collection constructors can use return context.
///
/// Inputs:
/// - A source module importing portable `List`, `Set`, and `Map` collection
///   constructors and type aliases.
/// - Functions returning explicit collection types from empty constructor
///   shorthand calls.
///
/// Output:
/// - Test passes when typechecking accepts each empty constructor because the
///   declared return type supplies the otherwise-missing generic arguments.
///
/// Transformation:
/// - Loads checked-in std summaries and validates that final return
///   unification resolves empty portable collection constructor calls without
///   weakening local binding diagnostics.
#[test]
fn syntax_output_empty_portable_collection_constructors_use_return_context() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module collections.EmptyConstructorsReturnContext.\n\
\n\
import std.collections.List.\n\
import std.collections.Set.\n\
import std.collections.Map.\n\
import type std.collections.List.List.\n\
import type std.collections.Set.Set.\n\
import type std.collections.Map.Map.\n\
\n\
pub list_values(): List[String] ->\n\
    List().\n\
\n\
pub set_values(): Set[String] ->\n\
    Set().\n\
\n\
pub map_values(): Map[String, Int] ->\n\
    Map().\n\
",
        "std/collections/Map.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
