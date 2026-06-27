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
    check_template_slots(template, &parsed, struct_fields)
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
