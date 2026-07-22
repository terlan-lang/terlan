use super::*;

/// Builds a template declaration for focused signature tests.
///
/// Inputs:
/// - `name`: template declaration name.
/// - `props`: prop names paired with type text.
///
/// Output:
/// - `TemplateCheckDecl` with deterministic spans and no parsed template body.
///
/// Transformation:
/// - Converts compact test tuples into the validator's normalized declaration
///   shape without reading a template file.
fn template_decl(name: &str, props: &[(&str, &str)]) -> TemplateCheckDecl {
    TemplateCheckDecl {
        name: name.to_string(),
        source_path: "./template.terl.html".to_string(),
        resolved_path: "/tmp/template.terl.html".to_string(),
        metadata: crate::terlan_html::TemplateMetadata::default(),
        props: props
            .iter()
            .enumerate()
            .map(|(index, (name, annotation))| TemplateCheckProp {
                name: (*name).to_string(),
                annotation: (*annotation).to_string(),
                span: Span::new(index, index + 1),
            })
            .collect(),
        span: Span::new(0, 1),
    }
}

/// Returns diagnostic messages from template prop signature validation.
///
/// Inputs:
/// - `template`: normalized template declaration.
///
/// Output:
/// - Diagnostic message strings in validator order.
///
/// Transformation:
/// - Runs the private signature validator and strips spans/severity so tests
///   can assert the user-facing contract text directly.
fn prop_signature_messages(template: &TemplateCheckDecl) -> Vec<String> {
    validate_template_prop_signatures(template)
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Verifies duplicate template props are rejected before render generation.
///
/// Inputs: a declaration with the same prop name twice.
/// Output: one duplicate-prop diagnostic.
/// Transformation: exercises the focused template-signature validator.
#[test]
fn template_prop_signature_rejects_duplicate_props() {
    let template = template_decl("Card", &[("title", "String"), ("title", "String")]);

    assert_eq!(
        prop_signature_messages(&template),
        vec!["duplicate prop `title` in template `Card`"]
    );
}

/// Verifies `children` stays reserved for component body content.
///
/// Inputs: a declaration that exposes `children` as a user prop.
/// Output: one reserved-prop diagnostic.
/// Transformation: exercises the focused template-signature validator.
#[test]
fn template_prop_signature_rejects_reserved_children_prop() {
    let template = template_decl("Shell", &[("children", "Template.Html")]);

    assert_eq!(
        prop_signature_messages(&template),
        vec!["template `Shell` declares reserved prop `children`"]
    );
}

/// Returns template-slot diagnostics for a parsed HTML template body.
///
/// Inputs:
/// - `template`: normalized template declaration.
/// - `html`: external `.terl.html` source body.
/// - `struct_fields`: known struct field type map.
///
/// Output:
/// - Diagnostic message strings.
///
/// Transformation:
/// - Parses the HTML through the real template parser and runs the private
///   slot validator used by the template contract.
fn slot_messages(
    template: &TemplateCheckDecl,
    html: &str,
    struct_fields: &HashMap<String, HashMap<String, String>>,
) -> Vec<String> {
    let parsed = crate::terlan_html::parse_template(html, "template.terl.html")
        .expect("parse template fixture");
    check_template_slots(template, &parsed, struct_fields, None)
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Returns component-use diagnostics for a parsed parent template body.
///
/// Inputs:
/// - `parent`: template declaration using a component tag.
/// - `component_tag`: normalized component tag name.
/// - `component`: component template declaration.
/// - `html`: parent template body.
///
/// Output:
/// - Diagnostic message strings.
///
/// Transformation:
/// - Parses the parent HTML and invokes the component validator with one
///   manually indexed component declaration.
fn component_messages(
    parent: &TemplateCheckDecl,
    component_tag: &str,
    component: &TemplateCheckDecl,
    html: &str,
) -> Vec<String> {
    let parsed = crate::terlan_html::parse_template(html, "template.terl.html")
        .expect("parse template fixture");
    let templates_by_tag = HashMap::from([(component_tag.to_string(), component)]);
    check_template_component_tags(
        parent,
        &parsed,
        &templates_by_tag,
        &BTreeSet::new(),
        &HashMap::new(),
        None,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.message)
    .collect()
}

/// Verifies non-scalar slot roots cannot render directly as text.
///
/// Inputs:
/// - A template prop typed as `User`.
/// - A text interpolation `${user}`.
///
/// Output:
/// - One non-renderable slot diagnostic.
///
/// Transformation:
/// - Exercises the first context-aware expression-island typecheck without
///   requiring runtime rendering.
#[test]
fn template_slot_typecheck_rejects_record_value_in_text_context() {
    let template = template_decl("Card", &[("user", "User")]);

    assert_eq!(
        slot_messages(&template, "<p>${user}</p>", &HashMap::new()),
        vec![
            "template `Card` slot `user` has non-renderable type `User` (template line 1, columns 1-7)"
        ]
    );
}

/// Verifies multiline text-slot diagnostics preserve source line and columns.
///
/// Inputs:
/// - A template prop typed as `User`.
/// - A text interpolation on the second template line.
///
/// Output:
/// - One non-renderable slot diagnostic pointing to template line 2.
///
/// Transformation:
/// - Locks source-map-style location propagation from the HTML parser through
///   template contract validation.
#[test]
fn template_slot_typecheck_reports_multiline_text_slot_location() {
    let template = template_decl("Card", &[("user", "User")]);

    assert_eq!(
        slot_messages(
            &template,
            "<section>\n<p>${user}</p>\n</section>",
            &HashMap::new()
        ),
        vec![
            "template `Card` slot `user` has non-renderable type `User` (template line 2, columns 1-7)"
        ]
    );
}

/// Verifies multiline attribute-slot diagnostics preserve source line and
/// columns.
///
/// Inputs:
/// - A template prop typed as `Int`.
/// - A URL attribute interpolation on the second template line.
///
/// Output:
/// - One URL-specific diagnostic pointing to template line 2.
///
/// Transformation:
/// - Ensures attribute interpolation spans travel with the original attribute
///   line instead of the element start line.
#[test]
fn template_slot_typecheck_reports_multiline_attribute_slot_location() {
    let template = template_decl("ProfileLink", &[("count", "Int")]);

    assert_eq!(
        slot_messages(
            &template,
            "<a\nhref=\"${count}\">count</a>",
            &HashMap::new()
        ),
        vec![
            "template `ProfileLink` URL attribute `href` slot `count` has non-renderable type `Int` (template line 2, columns 1-8)"
        ]
    );
}

/// Verifies scalar struct fields can render as text.
///
/// Inputs:
/// - A template prop typed as `User`.
/// - Known `User.name: String` struct metadata.
/// - A text interpolation `${user.name}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Confirms field-path type resolution feeds the renderability checker.
#[test]
fn template_slot_typecheck_accepts_scalar_struct_field_in_text_context() {
    let template = template_decl("Card", &[("user", "User")]);
    let struct_fields = HashMap::from([(
        "User".to_string(),
        HashMap::from([("name".to_string(), "String".to_string())]),
    )]);

    assert_eq!(
        slot_messages(&template, "<p>${user.name}</p>", &struct_fields),
        Vec::<String>::new()
    );
}

/// Verifies HTML fragments cannot render as attribute values.
///
/// Inputs:
/// - A template prop typed as `Template.Html`.
/// - A whole-attribute interpolation `${body}`.
///
/// Output:
/// - One non-renderable attribute diagnostic.
///
/// Transformation:
/// - Prevents unsafe HTML-fragment interpolation into attribute context while
///   leaving body/text context available for HTML fragments.
#[test]
fn template_slot_typecheck_rejects_html_fragment_in_attribute_context() {
    let template = template_decl("Shell", &[("body", "Template.Html")]);

    assert_eq!(
        slot_messages(
            &template,
            "<main title=\"${body}\"></main>",
            &HashMap::new()
        ),
        vec![
            "template `Shell` attribute slot `body` has non-renderable type `Template.Html` (template line 1, columns 1-7)"
        ]
    );
}

/// Verifies explicit URI values can render into URL-bearing attributes.
///
/// Inputs:
/// - A template prop typed as `std.net.Uri.Uri`.
/// - A URL attribute interpolation `${profile}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Proves URL attribute validation accepts the std URI type instead of
///   treating all attributes as generic scalar text.
#[test]
fn template_slot_typecheck_accepts_uri_in_url_attribute() {
    let template = template_decl("ProfileLink", &[("profile", "std.net.Uri.Uri")]);

    assert_eq!(
        slot_messages(
            &template,
            "<a href=\"${profile}\">profile</a>",
            &HashMap::new()
        ),
        Vec::<String>::new()
    );
}

/// Verifies URL attributes reject non-URL scalar values.
///
/// Inputs:
/// - A template prop typed as `Int`.
/// - A URL attribute interpolation `${count}`.
///
/// Output:
/// - One URL-specific non-renderable attribute diagnostic.
///
/// Transformation:
/// - Keeps URL attributes stricter than generic attributes so accidental
///   numeric values cannot be rendered as links.
#[test]
fn template_slot_typecheck_rejects_int_in_url_attribute() {
    let template = template_decl("ProfileLink", &[("count", "Int")]);

    assert_eq!(
        slot_messages(&template, "<a href=\"${count}\">count</a>", &HashMap::new()),
        vec![
            "template `ProfileLink` URL attribute `href` slot `count` has non-renderable type `Int` (template line 1, columns 1-8)"
        ]
    );
}

/// Verifies boolean values can render into boolean HTML attributes.
///
/// Inputs:
/// - A template prop typed as `Bool`.
/// - A boolean attribute interpolation `${is_disabled}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Proves boolean attributes use a distinct renderability rule instead of
///   generic text rendering.
#[test]
fn template_slot_typecheck_accepts_bool_in_boolean_attribute() {
    let template = template_decl("SubmitButton", &[("is_disabled", "Bool")]);

    assert_eq!(
        slot_messages(
            &template,
            "<button disabled=\"${is_disabled}\">Save</button>",
            &HashMap::new()
        ),
        Vec::<String>::new()
    );
}

/// Verifies boolean attributes reject string-like values.
///
/// Inputs:
/// - A template prop typed as `String`.
/// - A boolean attribute interpolation `${state}`.
///
/// Output:
/// - One boolean-specific non-renderable attribute diagnostic.
///
/// Transformation:
/// - Prevents accidental string truthiness from entering template rendering.
#[test]
fn template_slot_typecheck_rejects_string_in_boolean_attribute() {
    let template = template_decl("SubmitButton", &[("state", "String")]);

    assert_eq!(
        slot_messages(
            &template,
            "<button disabled=\"${state}\">Save</button>",
            &HashMap::new()
        ),
        vec![
            "template `SubmitButton` boolean attribute `disabled` slot `state` has non-renderable type `String` (template line 1, columns 1-8)"
        ]
    );
}

/// Verifies optional scalar values can render as omitted-or-present attributes.
///
/// Inputs:
/// - A template prop typed as `Option[String]`.
/// - A generic attribute interpolation `${maybe_title}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Confirms optional attributes validate the wrapped type instead of
///   rejecting the `Option` container itself.
#[test]
fn template_slot_typecheck_accepts_optional_string_in_attribute() {
    let template = template_decl("Card", &[("maybe_title", "Option[String]")]);

    assert_eq!(
        slot_messages(
            &template,
            "<article title=\"${maybe_title}\"></article>",
            &HashMap::new()
        ),
        Vec::<String>::new()
    );
}

/// Verifies optional URI values can render as omitted-or-present URL
/// attributes.
///
/// Inputs:
/// - A template prop typed as `Option[std.net.Uri.Uri]`.
/// - A URL attribute interpolation `${maybe_profile}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Keeps optional URL attributes aligned with the stricter URL attribute
///   type contract.
#[test]
fn template_slot_typecheck_accepts_optional_uri_in_url_attribute() {
    let template = template_decl(
        "ProfileLink",
        &[("maybe_profile", "Option[std.net.Uri.Uri]")],
    );

    assert_eq!(
        slot_messages(
            &template,
            "<a href=\"${maybe_profile}\">profile</a>",
            &HashMap::new()
        ),
        Vec::<String>::new()
    );
}

/// Verifies optional booleans can render as omitted-or-present boolean
/// attributes.
///
/// Inputs:
/// - A template prop typed as `std.core.Option.Option[Bool]`.
/// - A boolean attribute interpolation `${maybe_disabled}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Confirms fully qualified core Option spellings use the same optional
///   attribute rule as short `Option[T]` annotations.
#[test]
fn template_slot_typecheck_accepts_qualified_optional_bool_in_boolean_attribute() {
    let template = template_decl(
        "SubmitButton",
        &[("maybe_disabled", "std.core.Option.Option[Bool]")],
    );

    assert_eq!(
        slot_messages(
            &template,
            "<button disabled=\"${maybe_disabled}\">Save</button>",
            &HashMap::new()
        ),
        Vec::<String>::new()
    );
}

/// Verifies optional attributes still reject non-renderable wrapped values.
///
/// Inputs:
/// - A template prop typed as `Option[User]`.
/// - A generic attribute interpolation `${maybe_user}`.
///
/// Output:
/// - One optional-attribute diagnostic.
///
/// Transformation:
/// - Prevents `Option[T]` from becoming an escape hatch for rendering complex
///   values directly into HTML attributes.
#[test]
fn template_slot_typecheck_rejects_optional_record_in_attribute() {
    let template = template_decl("Card", &[("maybe_user", "Option[User]")]);

    assert_eq!(
        slot_messages(
            &template,
            "<article title=\"${maybe_user}\"></article>",
            &HashMap::new()
        ),
        vec![
            "template `Card` optional attribute `title` slot `maybe_user` has non-renderable type `Option[User]` (template line 1, columns 1-13)"
        ]
    );
}

/// Verifies token-list attributes accept collections of string-like values.
///
/// Inputs:
/// - A template prop typed as `List[String]`.
/// - A `class` attribute interpolation `${classes}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Confirms token-list attributes can be typed as collections without
///   falling back to generic scalar-only attribute rendering.
#[test]
fn template_slot_typecheck_accepts_string_list_in_token_list_attribute() {
    let template = template_decl("Card", &[("classes", "List[String]")]);

    assert_eq!(
        slot_messages(
            &template,
            "<article class=\"${classes}\"></article>",
            &HashMap::new()
        ),
        Vec::<String>::new()
    );
}

/// Verifies optional token-list attributes accept collections of string-like
/// values.
///
/// Inputs:
/// - A template prop typed as `Option[List[String]]`.
/// - A `class` attribute interpolation `${maybe_classes}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Confirms optional attribute validation delegates to the token-list rule
///   for the wrapped collection type.
#[test]
fn template_slot_typecheck_accepts_optional_string_list_in_token_list_attribute() {
    let template = template_decl("Card", &[("maybe_classes", "Option[List[String]]")]);

    assert_eq!(
        slot_messages(
            &template,
            "<article class=\"${maybe_classes}\"></article>",
            &HashMap::new()
        ),
        Vec::<String>::new()
    );
}

/// Verifies token-list attributes reject collections of non-token values.
///
/// Inputs:
/// - A template prop typed as `List[User]`.
/// - A `class` attribute interpolation `${classes}`.
///
/// Output:
/// - One token-list-specific diagnostic.
///
/// Transformation:
/// - Prevents collection values from bypassing renderability checks for
///   whitespace-separated HTML token attributes.
#[test]
fn template_slot_typecheck_rejects_record_list_in_token_list_attribute() {
    let template = template_decl("Card", &[("classes", "List[User]")]);

    assert_eq!(
        slot_messages(
            &template,
            "<article class=\"${classes}\"></article>",
            &HashMap::new()
        ),
        vec![
            "template `Card` token-list attribute `class` slot `classes` has non-renderable type `List[User]` (template line 1, columns 1-10)"
        ]
    );
}

/// Verifies expression slots can use ordinary Terlan arithmetic.
///
/// Inputs:
/// - A template prop typed as `Int`.
/// - A text interpolation `${count + 1}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Confirms non-path slots are routed through formal expression parsing and
///   typechecking instead of the older dotted-path-only validator.
#[test]
fn template_slot_typecheck_accepts_arithmetic_expression_in_text_context() {
    let template = template_decl("Counter", &[("count", "Int")]);

    assert_eq!(
        slot_messages(&template, "<p>${count + 1}</p>", &HashMap::new()),
        Vec::<String>::new()
    );
}

/// Verifies expression slots can use receiver methods when they typecheck.
///
/// Inputs:
/// - A template prop typed as `Int`.
/// - An attribute interpolation `${count.to_string()}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Exercises the method-call expression island path in an attribute context,
///   where the resulting String is scalar-renderable.
#[test]
fn template_slot_typecheck_accepts_receiver_method_expression_in_attribute_context() {
    let template = template_decl("Counter", &[("count", "Int")]);

    assert_eq!(
        slot_messages(
            &template,
            "<p title=\"${count.to_string()}\">value</p>",
            &HashMap::new()
        ),
        Vec::<String>::new()
    );
}

/// Verifies component props can receive typed expression slots.
///
/// Inputs:
/// - A parent template with `count: Int`.
/// - A component that expects `value: Int`.
/// - A component prop interpolation `${count + 1}`.
///
/// Output:
/// - No diagnostics.
///
/// Transformation:
/// - Confirms component prop compatibility uses formal expression typechecking
///   for non-dotted slot expressions.
#[test]
fn template_component_prop_accepts_expression_slot_matching_expected_type() {
    let parent = template_decl("CounterPage", &[("count", "Int")]);
    let component = template_decl("CounterLabel", &[("value", "Int")]);

    assert_eq!(
        component_messages(
            &parent,
            "counter-label",
            &component,
            "<counter-label value=\"${count + 1}\"></counter-label>"
        ),
        Vec::<String>::new()
    );
}

/// Verifies component props reject expression slots with mismatched types.
///
/// Inputs:
/// - A parent template with `count: Int`.
/// - A component that expects `value: String`.
/// - A component prop interpolation `${count + 1}`.
///
/// Output:
/// - One component prop type diagnostic.
///
/// Transformation:
/// - Exercises the failure branch for expression-backed component prop
///   compatibility.
#[test]
fn template_component_prop_rejects_expression_slot_mismatching_expected_type() {
    let parent = template_decl("CounterPage", &[("count", "Int")]);
    let component = template_decl("CounterLabel", &[("value", "String")]);

    assert_eq!(
        component_messages(
            &parent,
            "counter-label",
            &component,
            "<counter-label value=\"${count + 1}\"></counter-label>"
        ),
        vec![
            "template `CounterPage` component `<counter-label>` prop `value` expects `String`, but expression `count + 1` does not typecheck as `String` (template line 1, columns 1-12)"
        ]
    );
}
