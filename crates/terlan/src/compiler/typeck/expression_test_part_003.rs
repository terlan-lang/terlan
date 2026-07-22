
/// Verifies empty native vector constructors can be locally bound.
///
/// Inputs:
/// - A source module importing `std.native.collections.Vector`.
/// - A let binding that assigns `Vector()` without using it in a constraining
///   position.
///
/// Output:
/// - Test passes when typechecking accepts the empty constructor binding.
///
/// Transformation:
/// - Exercises let-expression inference so empty generic constructor calls can
///   be created first and constrained by later receiver calls or use sites.
#[test]
fn syntax_output_empty_vector_constructor_in_let_is_accepted() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module native.EmptyVectorConstructorLetContext.\n\
\n\
import std.native.collections.Vector.\n\
\n\
pub value(): Unit ->\n\
    let users = Vector();\n\
    Unit.\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies empty portable collection constructors can be locally bound.
///
/// Inputs:
/// - A source module importing portable `List`, `Set`, and `Map` collection
///   constructors.
/// - Let bindings that assign empty constructor calls without constraining
///   their generic arguments.
///
/// Output:
/// - Test passes when typechecking accepts all empty constructor bindings.
///
/// Transformation:
/// - Exercises let-expression inference so empty generic constructor calls for
///   portable collections can be created first and constrained by later use.
#[test]
fn syntax_output_empty_portable_collection_constructors_in_let_are_accepted() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module collections.EmptyConstructorsLetContext.\n\
\n\
import std.collections.List.\n\
import std.collections.Set.\n\
import std.collections.Map.\n\
\n\
pub value(): Unit ->\n\
    let list = List();\n\
    let set = Set();\n\
    let map = Map();\n\
    Unit.\n\
",
        "std/collections/Map.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies bracket assignments infer through `IndexSet`.
///
/// Inputs:
/// - A syntax-output module declaring `IndexSet[C, I, T]`.
/// - One struct declaring `implements IndexSet[IndexedBox, Int, Int]`.
/// - One mutable receiver method satisfying the trait contract.
/// - A function body assigning through bracket syntax.
///
/// Output:
/// - Test passes when the function body is reported as `Unit` against an
///   intentionally wrong `String` return annotation.
///
/// Transformation:
/// - Exercises the compiler-owned desugaring contract that treats
///   `collection[index] = value` as a trait-backed
///   `IndexSet.set_at(collection, index, value)` update while preserving
///   target-neutral parser syntax.
#[test]
fn syntax_output_infers_index_assignment_through_index_set_trait() {
    let diagnostics = check_syntax_output(
        "\
module syntax_index_set_trait.\n\
\n\
pub trait IndexSet[C, I, T] {\n\
    set_at(mut collection: C, index: I, value: T): Unit.\n\
}.\n\
\n\
pub struct IndexedBox implements IndexSet[IndexedBox, Int, Int] {\n\
    value: Int\n\
}.\n\
\n\
pub (mut collection: IndexedBox) set_at(index: Int, value: Int): Unit ->\n\
    Unit.\n\
\n\
pub write(value: IndexedBox): String ->\n\
    value[0] = 1.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("expected Binary found Unit")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported wrappers can satisfy bracket read and write contracts.
///
/// Inputs:
/// - A generated-style provider interface exporting an opaque wrapper type.
/// - Public `IndexGet` and `IndexSet` traits plus wrapper conformances.
/// - A consumer module importing the wrapper and trait contracts.
///
/// Output:
/// - Test passes when `values[0]` infers `String` and `values[0] = "x"`
///   infers `Unit` through imported interface metadata.
///
/// Transformation:
/// - Exercises the same trait-backed bracket desugaring that JS DOM wrappers
///   need, without relying on local source impl bodies.
#[test]
fn syntax_output_infers_imported_index_get_and_set_for_generated_wrapper_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module index_generated.Consumer.\n\
\n\
import type std.js.Dom.{ElementList}.\n\
import std.js.Dom.{IndexGet, IndexSet}.\n\
\n\
pub read(values: ElementList): String ->\n\
    values[0].\n\
\n\
pub write(values: ElementList): Unit ->\n\
    values[0] = \"x\".\n\
",
        "\
module std.js.Dom.\n\
\n\
pub type ElementList.\n\
\n\
pub trait IndexGet[C, I, T] {\n\
    get_at(collection: C, index: I): T.\n\
}.\n\
\n\
pub trait IndexSet[C, I, T] {\n\
    set_at(mut collection: C, index: I, value: T): Unit.\n\
}.\n\
\n\
pub impl IndexGet[ElementList, Int, String] for ElementList {\n\
}.\n\
\n\
pub impl IndexSet[ElementList, Int, String] for ElementList {\n\
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
fn syntax_output_infers_pipe_forward_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_pipe_inference.\n\
add_one(x: Int): Int ->\n\
    x + 1.\n\
pub via_pipe(x: Int): Int ->\n\
    x |> add_one().\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies pipe-forward can target imported module-member calls.
///
/// Inputs:
/// - A consumer module importing a provider module alias.
/// - Pipe-forward calls into `Provider.add(...)`, including an omitted
///   defaulted argument.
///
/// Output:
/// - Test passes when the pipe input is inserted as the first remote call
///   argument and default-argument validation still sees that slot as supplied.
///
/// Transformation:
/// - Exercises the syntax-output remote-call representation used by
///   formatter-canonicalized module-member pipe stages.
#[test]
fn syntax_output_infers_pipe_forward_into_imported_module_member_call() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module syntax_module_member_pipe_inference.\n\
\n\
import pipe.Provider.\n\
\n\
pub explicit(value: Int): Int ->\n\
    value |> Provider.add(2).\n\
\n\
pub defaulted(value: Int): Int ->\n\
    value |> Provider.add().\n\
",
        "\
module pipe.Provider.\n\
\n\
pub add(value: Int, delta: Int = 1): Int.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_binary_ops_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_binary_op_inference.\n\
pub add(x: Int, y: Int): Int ->\n\
    x + y.\n\
pub compare(x: Int, y: Int): Bool ->\n\
    x <= y.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_range_membership_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_range_membership_inference.\n\
pub in_success_band(status: Int): Bool ->\n\
    status in 200..299.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_non_integer_range_bounds() {
    let diagnostics = check_syntax_output(
        "\
module syntax_range_bound_diagnostics.\n\
pub invalid_range(): Bool ->\n\
    2 in 1..\"3\".\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("right side expected Int found Binary")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_non_range_membership_target() {
    let diagnostics = check_syntax_output(
        "\
module syntax_range_membership_target_diagnostics.\n\
pub invalid_membership(value: Int): Bool ->\n\
    value in [1, 2, 3].\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| {
            diag.message
                .contains("right side expected std.range.Range.Range found List[Int]")
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_infers_field_access_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_field_inference.\n\
pub struct User {\n\
    id: Int,\n\
    name: Binary\n\
}.\n\
pub get_id(user: User): Int ->\n\
    user.id.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_template_instantiation_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_instantiation.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
pub view(title: Binary): Html[Dynamic] ->\n\
    Page(title = title).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies struct constructor-call field assignment typechecks.
///
/// Inputs:
/// - A module declaring `User`.
/// - A body using canonical `User(name = name)` struct construction.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Exercises the compiler-provided default field constructor for structs
///   that do not declare explicit constructors.
#[test]
fn syntax_output_accepts_default_struct_constructor_call() {
    let diagnostics = check_syntax_output(
        "\
module syntax_struct_constructor_call.\n\
pub struct User {\n\
    name: Binary\n\
}.\n\
pub make(name: Binary): User ->\n\
    User(name = name).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies explicit constructors disable external implicit construction.
///
/// Inputs:
/// - A struct `User`.
/// - An explicit constructor declaration for `User`.
/// - A regular function attempting `User(name = name)`.
///
/// Output:
/// - Diagnostic from explicit-constructor resolution.
///
/// Transformation:
/// - Confirms public construction authority moves to explicit constructor
///   declarations once they exist instead of falling back to the field
///   initializer.
#[test]
fn syntax_output_rejects_default_struct_constructor_when_explicit_constructor_exists() {
    let diagnostics = check_syntax_output(
        "\
module syntax_struct_explicit_constructor_blocks_default.\n\
pub struct User {\n\
    name: Binary\n\
}.\n\
pub constructor User {\n\
    (name: Binary, role: Binary): User ->\n\
        User(name = name)\n\
}.\n\
pub make(name: Binary): User ->\n\
    User(name = name).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("missing required argument `role` for constructor `User`")),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies constructor bodies may use the internal default initializer.
///
/// Inputs:
/// - A struct with an explicit constructor.
/// - The constructor body uses `User(name = name)`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Marks the constructor target as active while checking constructor bodies,
///   allowing the internal initializer without reopening it to other call sites.
#[test]
fn syntax_output_accepts_default_struct_initializer_inside_explicit_constructor() {
    let diagnostics = check_syntax_output(
        "\
module syntax_struct_explicit_constructor_internal_initializer.\n\
pub struct User {\n\
    name: Binary\n\
}.\n\
pub constructor User {\n\
    (display_name: Binary): User ->\n\
        User(name = display_name)\n\
}.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies direct named template calls typecheck as generated functions.
///
/// Inputs:
/// - A template declaration with one required property.
/// - A function returning `Page(title = title)`.
///
/// Output:
/// - Test passes when the call returns the template HTML value type without
///   being treated as an unknown constructor.
///
/// Transformation:
/// - Exercises template-call normalization before ordinary constructor and
///   function resolution.
#[test]
fn syntax_output_checks_named_template_call_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_named_call.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
pub view(title: Binary): Html[Dynamic] ->\n\
    Page(title = title).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies direct positional template calls use declaration prop order.
///
/// Inputs:
/// - A template declaration with one required property and one defaulted
///   property.
/// - A function returning `Page(title)`.
///
/// Output:
/// - Test passes when the positional argument maps to the first property and
///   the omitted trailing property uses its default.
///
/// Transformation:
/// - Confirms generated template functions preserve declaration order in the
///   typechecker.
#[test]
fn syntax_output_checks_positional_template_call_with_default_property() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_positional_call.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary,\n\
    subtitle: String = \"Ready\"\n\
}.\n\
pub view(title: String): Html[Dynamic] ->\n\
    Page(title).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies generated template functions reject missing required props.
///
/// Inputs:
/// - A template declaration with one required property.
/// - A function returning `Page()`.
///
/// Output:
/// - Test passes when the template-instantiation diagnostic names the missing
///   required property.
///
/// Transformation:
/// - Confirms direct template-call normalization still uses the shared required
///   prop checker.
#[test]
fn syntax_output_rejects_template_call_missing_required_prop() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_missing_call_prop.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
pub view(): Html[Dynamic] ->\n\
    Page().\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("template `Page` instantiation is missing required prop `title`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies defaulted template properties may be omitted.
///
/// Inputs:
/// - A template declaration whose only property has a default value.
/// - A template instantiation that supplies no explicit fields.
///
/// Output:
/// - Test passes when typechecking accepts the omitted defaulted property.
///
/// Transformation:
/// - Uses template property default metadata while validating required
///   instantiation fields.
#[test]
fn syntax_output_accepts_omitted_template_default_property() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_default_instantiation.\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: String = \"Untitled\"\n\
}.\n\
pub view(): Html[Dynamic] ->\n\
    Page().\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies omitted template defaults still typecheck against property types.
///
/// Inputs:
/// - A template property declared as `Int` with a binary default.
/// - An instantiation that omits the property and therefore uses the default.
///
/// Output:
/// - Test passes when typechecking reports a default-property mismatch.
///
/// Transformation:
/// - Infers the default expression at instantiation time and unifies it with
///   the declared template property type.
#[test]
fn syntax_output_rejects_mismatched_template_default_property() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_default_bad.\n\
template Page from \"./templates/page.terl.html\" {\n\
    count: Int = \"bad\"\n\
}.\n\
pub view(): Html[Dynamic] ->\n\
    Page().\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("template `Page` default prop `count`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies primitive receiver methods accept named arguments.
///
/// Inputs:
/// - A string receiver call using `pattern = ...`.
///
/// Output:
/// - Test passes when typechecking accepts the compiler-owned primitive method
///   with a named argument.
///
/// Transformation:
/// - Validates primitive receiver method names against the compiler-owned
///   parameter-name table before ordinary primitive unification.
#[test]
fn syntax_output_accepts_primitive_receiver_named_argument() {
    let diagnostics = check_syntax_output(
        "\
module primitive_receiver_named_arg_ok.\n\
pub demo(): Bool ->\n\
    \"hello\".contains(pattern = \"ell\").\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies primitive receiver named arguments reject unknown names.
///
/// Inputs:
/// - A string receiver call with an unsupported named argument.
///
/// Output:
/// - Test passes when typechecking reports the invalid argument name.
///
/// Transformation:
/// - Runs the same named-argument validation used by declared functions
///   against compiler-owned primitive receiver method metadata.
#[test]
fn syntax_output_rejects_unknown_primitive_receiver_named_argument() {
    let diagnostics = check_syntax_output(
        "\
module primitive_receiver_named_arg_bad.\n\
pub demo(): Bool ->\n\
    \"hello\".contains(needle = \"ell\").\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("unknown named argument `needle` for call to `contains`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_html_blocks_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_html_blocks.\n\
pub view(title: Binary): Html[Dynamic] ->\n\
    html {\n\
        <section class={[\"hero\"]}>{title}</section>\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies syntax HTML blocks typecheck against the public template facade.
///
/// Inputs:
/// - A module importing `std.template.Template`.
/// - A function returning `Template.Html` with an `html { ... }` body.
///
/// Output:
/// - Test passes when the internal syntax HTML value type unifies with the
///   public standard-library template fragment type.
///
/// Transformation:
/// - Parses through the formal syntax-output path and exercises return-type
///   unification for `Html[Dynamic]` against `Template.Html`.
#[test]
fn syntax_output_html_blocks_assign_to_template_html_facade() {
    let diagnostics = check_syntax_output(
        "\
module syntax_template_html_blocks.\n\
import std.template.Template.\n\
pub view(title: Binary): Template.Html ->\n\
    html {\n\
        <section>{title}</section>\n\
    }.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
