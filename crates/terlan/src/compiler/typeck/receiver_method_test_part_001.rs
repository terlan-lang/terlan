use super::test_support::*;
use super::*;
use crate::terlan_syntax::parse_module_as_syntax_output;

/// Verifies the release Iterable contract typechecks as receiver methods.
///
/// Inputs:
/// - A source module containing the release-shaped `Iterator[T]` and
///   `Iterable[C, T]` contracts.
/// - A struct declaring `implements Iterable[IntCollection, Int]`.
/// - A matching receiver method `iterator(): Iterator[Int]`.
///
/// Output:
/// - Test passes when declaration-site conformance reports no diagnostics.
///
/// Transformation:
/// - Exercises formal typecheck conformance without traversal lowering:
///   the trait method's first parameter maps to the receiver and the return
///   type is specialized from `Iterator[T]` to `Iterator[Int]`.
#[test]
fn syntax_output_checks_release_traversal_contracts_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module iterable_contract_ok.\n\
\n\
pub trait Iterable[C, T] {\n\
    iterator(collection: C): Iterator[T].\n\
}.\n\
\n\
pub struct IntCollection implements Iterable[IntCollection, Int] {\n\
    size: Int\n\
}.\n\
\n\
pub type Iterator[T] = IntCollection.\n\
\n\
pub (collection: IntCollection) iterator(): Iterator[Int] ->\n\
    collection.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies Iterable conformance requires the receiver method.
///
/// Inputs:
/// - A source module containing the release-shaped `Iterable[C, T]`
///   contract.
/// - A struct declaring `implements Iterable[IntCollection, Int]` without an
///   `iterator` receiver method.
///
/// Output:
/// - Test passes when typecheck emits the stable missing receiver method
///   diagnostic.
///
/// Transformation:
/// - Runs declaration-site conformance validation and proves traversal
///   contracts are checked before any collection traversal lowering exists.
#[test]
fn syntax_output_rejects_release_traversal_contracts_missing_receiver_method() {
    let diagnostics = check_syntax_output(
        "\
module iterable_contract_missing_method.\n\
\n\
pub opaque type Iterator[T].\n\
\n\
pub trait Iterable[C, T] {\n\
    iterator(collection: C): Iterator[T].\n\
}.\n\
\n\
pub struct IntCollection implements Iterable[IntCollection, Int] {\n\
    size: Int\n\
}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "missing receiver method `iterator` for `IntCollection` implementing `Iterable`"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_resolves_local_receiver_method_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module receiver_dispatch_ok.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) display_name(): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user.display_name().\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_local_receiver_named_call_arguments() {
    let diagnostics = check_syntax_output(
        "\
module receiver_named_args_ok.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) label(prefix: String, suffix: String): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user.label(suffix = \"!\", prefix = \"User\").\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies omitted receiver-method arguments can use declaration defaults.
///
/// Inputs:
/// - A receiver method whose final non-receiver parameter has a default.
/// - A call that supplies only the required positional argument.
///
/// Output:
/// - Test passes when typechecking accepts the shorter call.
///
/// Transformation:
/// - Routes the call through receiver-method dispatch and lets the checker
///   complete the omitted parameter from the method signature metadata.
#[test]
fn syntax_output_accepts_omitted_receiver_method_default_argument() {
    let diagnostics = check_syntax_output(
        "\
module receiver_default_arg_ok.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) label(prefix: String, suffix: String = \"!\"): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user.label(\"User\").\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies named receiver-method calls still require non-default parameters.
///
/// Inputs:
/// - A receiver method with one required parameter and one defaulted parameter.
/// - A named call that supplies only the defaulted parameter.
///
/// Output:
/// - Test passes when typechecking reports the missing required parameter.
///
/// Transformation:
/// - Computes supplied receiver-method slots from named arguments and rejects
///   required slots that do not have a declaration default.
#[test]
fn syntax_output_rejects_omitted_required_receiver_method_argument() {
    let diagnostics = check_syntax_output(
        "\
module receiver_default_arg_missing_required.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) label(prefix: String, suffix: String = \"!\"): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user.label(suffix = \"?\").\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("missing required argument `prefix` for call to `label`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_unknown_local_receiver_named_call_argument() {
    let diagnostics = check_syntax_output(
        "\
module receiver_named_args_unknown.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) label(prefix: String): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user.label(text = \"User\").\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("unknown named argument `text` for call to `label`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_local_receiver_named_call_argument_supplied_positionally() {
    let diagnostics = check_syntax_output(
        "\
module receiver_named_args_positional_duplicate.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) label(prefix: String, suffix: String): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user.label(\"User\", prefix = \"Admin\").\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("argument `prefix` for call to `label` is already supplied positionally")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies pipe-forward prefers receiver-method resolution.
///
/// Inputs:
/// - A module with an immutable receiver method and a function body using
///   `value |> method()`.
///
/// Output:
/// - Test passes when typecheck resolves the pipe as a receiver-method call
///   instead of requiring a separate receiver-first free function.
///
/// Transformation:
/// - Runs the formal syntax-output typecheck path and proves receiver pipes
///   infer through the receiver-method dispatch table.
#[test]
fn syntax_output_typechecks_pipe_into_receiver_method() {
    let diagnostics = check_syntax_output(
        "\
module receiver_pipe_ok.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) display_name(): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user |> display_name().\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies receiver-method pipes accept omitted default arguments.
///
/// Inputs:
/// - A receiver method with a defaulted final non-receiver parameter.
/// - A pipe call that supplies only the required non-receiver parameter.
///
/// Output:
/// - Test passes when pipe inference resolves the receiver method and fills the
///   omitted default parameter for typechecking.
///
/// Transformation:
/// - Uses default-aware receiver candidate lookup for the right side of `|>`.
#[test]
fn syntax_output_typechecks_pipe_into_receiver_method_with_default_argument() {
    let diagnostics = check_syntax_output(
        "\
module receiver_pipe_default_ok.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) label(prefix: String, suffix: String = \"!\"): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user |> label(\"User\").\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_typechecks_inherited_receiver_method_call() {
    let diagnostics = check_syntax_output(
        "\
module derived_receiver_method_ok.\n\
\n\
pub struct Error {\n\
    message: String\n\
}.\n\
\n\
pub (error: Error) message_text(): String ->\n\
    error.message.\n\
\n\
pub struct FileError includes Error {\n\
    path: String\n\
}.\n\
\n\
pub show(error: FileError): String ->\n\
    error.message_text().\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_typechecks_imported_inherited_receiver_method_call() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_derived_receiver_method_ok.\n\
\n\
import std.core.{Error}.\n\
\n\
pub struct FileError includes Error {\n\
    path: String\n\
}.\n\
\n\
pub show(error: FileError): String ->\n\
    error.message_text().\n\
",
        "\
module std.core.\n\
\n\
pub struct Error {\n\
    code: Atom,\n\
    message: String\n\
}.\n\
\n\
pub (error: Error) message_text(): String.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported mutable receiver methods dispatch directly.
///
/// Inputs:
/// - A provider interface exporting a wrapper type and a mutable receiver
///   setter method returning `Unit`.
/// - A consumer module importing the type and using both direct receiver-call
///   and pipe-forward forms.
///
/// Output:
/// - Test passes when direct calls type as `Unit` and mutable receiver pipes
///   continue with the receiver type.
///
/// Transformation:
/// - Exercises generated-summary receiver metadata for JS-style wrappers
///   without requiring a local `includes` edge or hand-written adapter method.
#[test]
fn syntax_output_typechecks_imported_mutable_receiver_method_call_and_pipe() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_mutable_receiver_method_ok.\n\
\n\
import type std.js.Dom.{Element}.\n\
import std.js.Dom.\n\
\n\
pub set_once(element: Element): Unit ->\n\
    element.set_text(\"hello\").\n\
\n\
pub update(element: Element): Element ->\n\
    element |> set_text(\"hello\").\n\
",
        "\
module std.js.Dom.\n\
\n\
pub type Element.\n\
\n\
pub (mut element: Element) set_text(value: String): Unit.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported receiver return types retain dependency identity.
///
/// Inputs:
/// - A request provider whose `cookies()` receiver method returns `Jar`, an
///   opaque type owned by a second provider.
/// - Competing `get/1` receiver methods for `Jar` and `Json` with different
///   return types.
///
/// Output:
/// - Test passes when `request.cookies().get(...)` resolves to the `Jar`
///   method and therefore returns `String`.
///
/// Transformation:
/// - Exercises imported interface parsing with the complete dependency alias
///   graph so `Jar` cannot become a free type variable whose dispatch depends
///   on randomized map iteration order.
#[test]
fn syntax_output_preserves_imported_receiver_return_type_dependencies() {
    let diagnostics = check_syntax_output_with_interfaces(
        "\
module imported_receiver_dependency_identity.\n\
\n\
import deps.Request.\n\
import deps.Cookies.\n\
import deps.Json.\n\
import type deps.Request.{Request}.\n\
\n\
pub read(request: Request): String ->\n\
    let jar = request.cookies();\n\
    jar.get(\"session\").\n\
",
        &[
            "\
module deps.Cookies.\n\
\n\
pub opaque type Jar.\n\
\n\
pub (jar: Jar) get(name: String): String.\n\
",
            "\
module deps.Request.\n\
\n\
import type deps.Cookies.Jar.\n\
\n\
pub opaque type Request.\n\
\n\
pub (request: Request) cookies(): Jar.\n\
",
            "\
module deps.Json.\n\
\n\
pub opaque type Json.\n\
\n\
pub (json: Json) get(name: String): Int.\n\
",
        ],
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_typechecks_inherited_receiver_method_pipe() {
    let diagnostics = check_syntax_output(
        "\
module derived_receiver_pipe_ok.\n\
\n\
pub struct Error {\n\
    message: String\n\
}.\n\
\n\
pub (error: Error) message_text(): String ->\n\
    error.message.\n\
\n\
pub struct FileError includes Error {\n\
    path: String\n\
}.\n\
\n\
pub show(error: FileError): String ->\n\
    error |> message_text().\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies receiver-method pipe ambiguity is diagnosed.
///
/// Inputs:
/// - A module with both a receiver method `label()` for `User` and an
///   ordinary function `label(user: User)`.
/// - A pipe expression written as `user |> label()`.
///
/// Output:
/// - Test passes when typecheck reports the ambiguous pipe target instead
///   of silently choosing the receiver method or ordinary function.
///
/// Transformation:
/// - Runs receiver-method-first pipe inference and checks the explicit
///   ambiguity rule documented for P0.2b.
#[test]
fn syntax_output_rejects_ambiguous_receiver_method_pipe_target() {
    let diagnostics = check_syntax_output(
        "\
module receiver_pipe_ambiguous.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) label(): String ->\n\
    user.name.\n\
\n\
label(user: User): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user |> label().\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "ambiguous pipe target `label` / 0: receiver method and ordinary function both match"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies selected imported functions participate in pipe ambiguity.
///
/// Inputs:
/// - A provider interface exporting `label(value: Dynamic): String`.
/// - A consumer importing `label`, declaring a receiver method named
///   `label`, and using `user |> label()`.
///
/// Output:
/// - Test passes when typecheck reports the same ambiguity diagnostic used
///   for local ordinary functions.
///
/// Transformation:
/// - Resolves the selected import through the loaded interface during the
///   side-effect-free ambiguity check without emitting import diagnostics.
#[test]
fn syntax_output_rejects_imported_ambiguous_receiver_method_pipe_target() {
    let interface_source = "\
module labels.\n\
pub label(value: Dynamic): String.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module receiver_pipe_import_ambiguous.\n\
\n\
import labels.{label}.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) label(): String ->\n\
    user.name.\n\
\n\
pub show(user: User): String ->\n\
    user |> label().\n\
",
        interface_source,
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "ambiguous pipe target `label` / 0: receiver method and ordinary function both match"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies an incompatible receiver method does not shadow an imported pipe target.
#[test]
fn syntax_output_accepts_imported_pipe_target_when_receiver_candidate_does_not_match() {
    let interface_source = "\
module labels.\n\
pub label(value: Int): String.\n\
";
    let diagnostics = check_syntax_output_with_interface(
        "\
module receiver_pipe_import_fallback.\n\
\n\
import labels.{label}.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) label(): String ->\n\
    user.name.\n\
\n\
pub show(value: Int): String ->\n\
    value |> label().\n\
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
fn syntax_output_accepts_command_style_mutable_receiver_body_returning_receiver() {
    let diagnostics = check_syntax_output(
        "\
module receiver_dispatch_mutable_command_style.\n\
\n\
pub struct Map {\n\
    size: Int\n\
}.\n\
\n\
pub (mut map: Map) put(): Unit ->\n\
    map.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies mutable receiver pipe steps continue with the receiver type.
///
/// Inputs:
/// - A module with a mutable receiver method returning `Unit`.
/// - A function whose declared return type is the receiver type and whose
///   body is `receiver |> put()`.
///
/// Output:
/// - Test passes when no return-type mismatch treats the pipe as `Unit`,
///   and no blanket mutable-receiver unsupported diagnostic is emitted.
///
/// Transformation:
/// - Exercises P0.2c command-style mutable receiver checking while keeping
///   pipe continuation typed as the receiver.
#[test]
fn syntax_output_mutable_receiver_pipe_continues_with_receiver_type() {
    let diagnostics = check_syntax_output(
        "\
module receiver_pipe_mutable_continuation.\n\
\n\
pub struct Map {\n\
    size: Int\n\
}.\n\
\n\
pub (mut map: Map) put(): Unit ->\n\
    map.\n\
\n\
pub update(map: Map): Map ->\n\
    map |> put().\n\
",
    );

    assert!(
        !diagnostics.iter().any(|diag| diag
            .message
            .contains("mutable receiver method `put` for `Map` is parsed but not supported")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Map found Unit")),
        "mutable receiver pipe should continue as Map, diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies mutable receiver pipes accept omitted default arguments.
///
/// Inputs:
/// - A mutable receiver method with a defaulted final parameter.
/// - A pipe call that omits that parameter.
///
/// Output:
/// - Test passes when the mutable pipe still continues as the receiver type.
///
/// Transformation:
/// - Runs receiver-method pipe inference with completed defaulted argument
///   types before applying mutable receiver continuation semantics.
#[test]
fn syntax_output_mutable_receiver_pipe_accepts_default_argument() {
    let diagnostics = check_syntax_output(
        "\
module receiver_pipe_mutable_default.\n\
\n\
pub struct Map {\n\
    size: Int\n\
}.\n\
\n\
pub (mut map: Map) put(key: String, value: String = \"default\"): Unit ->\n\
    map.\n\
\n\
pub update(map: Map): Map ->\n\
    map |> put(\"a\").\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_result_producing_mutable_receiver_methods() {
    let diagnostics = check_syntax_output(
        "\
module receiver_dispatch_mutable_arbitrary_return.\n\
\n\
pub struct Map {\n\
    size: Int\n\
}.\n\
\n\
pub (mut map: Map) put(): String ->\n\
    \"map\".\n\
",
    );

    assert!(
            diagnostics.iter().any(|diag| diag.message.contains(
                "mutable receiver method `put` for `Map` may return Unit or `Map`; result type `String` needs the paired mutable receiver result ABI"
            )),
            "diagnostics: {:?}",
            diagnostics
        );
    assert!(
        !diagnostics.iter().any(|diag| diag
            .message
            .contains("mutable receiver method `put` for `Map` must return")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_receiver_returning_mutable_receiver_methods() {
    let diagnostics = check_syntax_output(
        "\
module receiver_dispatch_mutable_receiver_return.\n\
\n\
pub struct Map {\n\
    size: Int\n\
}.\n\
\n\
pub (mut map: Map) put(): Map ->\n\
    map.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}
