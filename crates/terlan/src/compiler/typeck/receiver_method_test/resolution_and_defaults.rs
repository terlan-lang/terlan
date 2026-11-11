use super::*;

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
pub(super) fn syntax_output_checks_release_traversal_contracts_on_formal_path() {
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
pub(super) fn syntax_output_rejects_release_traversal_contracts_missing_receiver_method() {
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
pub(super) fn syntax_output_resolves_local_receiver_method_calls_on_formal_path() {
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
pub(super) fn syntax_output_accepts_local_receiver_named_call_arguments() {
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
pub(super) fn syntax_output_accepts_omitted_receiver_method_default_argument() {
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
pub(super) fn syntax_output_rejects_omitted_required_receiver_method_argument() {
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
pub(super) fn syntax_output_rejects_unknown_local_receiver_named_call_argument() {
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
pub(super) fn syntax_output_rejects_local_receiver_named_call_argument_supplied_positionally() {
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

#[test]
pub(super) fn syntax_output_typechecks_inherited_receiver_method_call() {
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
pub(super) fn syntax_output_typechecks_imported_inherited_receiver_method_call() {
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
/// - A consumer module importing the type and using a direct receiver call.
///
/// Output:
/// - Test passes when the direct call types as `Unit`.
///
/// Transformation:
/// - Exercises generated-summary receiver metadata for JS-style wrappers
///   without requiring a local `includes` edge or hand-written adapter method.
#[test]
pub(super) fn syntax_output_typechecks_imported_mutable_receiver_method_call() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module imported_mutable_receiver_method_ok.\n\
\n\
import type std.js.Dom.{Element}.\n\
import std.js.Dom.\n\
\n\
pub set_once(element: Element): Unit ->\n\
    element.set_text(\"hello\").\n\
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
pub(super) fn syntax_output_preserves_imported_receiver_return_type_dependencies() {
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
pub(super) fn syntax_output_accepts_command_style_mutable_receiver_body_returning_receiver() {
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

#[test]
pub(super) fn syntax_output_rejects_result_producing_mutable_receiver_methods() {
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
pub(super) fn syntax_output_accepts_receiver_returning_mutable_receiver_methods() {
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
