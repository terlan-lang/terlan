use super::*;

const TOOLING_FIXTURES: &str =
    include_str!("../../../../tests/template/INTERPOLATION_TOOLING_FIXTURES.tsv");

#[test]
fn scanner_accepts_shared_parser_and_tree_sitter_fixture_inventory() {
    for (line_number, line) in TOOLING_FIXTURES.lines().enumerate() {
        let mut fields = line.splitn(3, '\t');
        let name = fields.next().expect("fixture name");
        let context = fields.next().expect("fixture context");
        let source = fields.next().expect("fixture source");
        let regions = scan_template_interpolations(source)
            .unwrap_or_else(|error| panic!("fixture {name} failed: {error:?}"));
        assert_eq!(regions.len(), 1, "fixture line {}", line_number + 1);
        assert!(
            matches!(
                (&regions[0].context, context),
                (TemplateInterpolationContext::Text, "text")
                    | (TemplateInterpolationContext::Attribute { .. }, "attribute")
            ),
            "fixture {name} context"
        );
    }
}

#[test]
fn scanner_preserves_nested_braces_and_quoted_closing_braces() {
    let source = r#"<p>${render(Map { body = "}" })}</p>"#;
    let regions = scan_template_interpolations(source).expect("nested interpolation");

    assert_eq!(regions.len(), 1);
    assert_eq!(
        &source[regions[0].expression_start..regions[0].expression_end],
        r#"render(Map { body = "}" })"#
    );
    assert_eq!(regions[0].context, TemplateInterpolationContext::Text);
}

#[test]
fn scanner_classifies_whole_attribute_interpolation() {
    let source = r#"<a href="${ url }">link</a>"#;
    let region = scan_template_interpolations(source)
        .expect("attribute interpolation")
        .remove(0);

    assert_eq!(
        region.context,
        TemplateInterpolationContext::Attribute {
            name: "href".to_string()
        }
    );
}

#[test]
fn formatter_normalizes_delimiter_whitespace_without_rewriting_expression() {
    let source = r#"<p>${  render(Map { body = "}" })  }</p>"#;
    assert_eq!(
        format_template_interpolations(source).expect("formatted interpolation"),
        r#"<p>${render(Map { body = "}" })}</p>"#
    );
}

#[test]
fn scanner_and_formatter_preserve_html_native_interpolation() {
    let source = r#"<h1>{ user.name }</h1><a href={ profile_url(user.id) }>profile</a>"#;
    let regions = scan_template_interpolations(source).expect("HTML-native interpolation");

    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0].expression_start - regions[0].open_start, 1);
    assert_eq!(regions[0].context, TemplateInterpolationContext::Text);
    assert_eq!(
        regions[1].context,
        TemplateInterpolationContext::Attribute {
            name: "href".to_string()
        }
    );
    assert_eq!(
        format_template_interpolations(source).expect("format HTML-native interpolation"),
        r#"<h1>{user.name}</h1><a href={profile_url(user.id)}>profile</a>"#
    );
}

#[test]
fn scanner_does_not_treat_metadata_or_raw_text_braces_as_interpolation() {
    let source = "@template { params = { title: String } }\n<style>.card { color: red; }</style>\n<script>const row = { title: 1 };</script>";

    assert_eq!(
        scan_template_interpolations(source).expect("literal brace contexts"),
        Vec::<TemplateInterpolationRegion>::new()
    );
}

#[test]
fn scanner_reports_unterminated_region_at_exact_opening_pair() {
    let error = scan_template_interpolations("<p>\n  ${value\n</p>")
        .expect_err("unterminated interpolation must fail");

    assert_eq!(error.message, "unterminated template interpolation slot");
    assert_eq!((error.line, error.start), (2, 2));
    assert_eq!(error.end, 4);
}

#[test]
fn scanner_preserves_adjacent_interpolation_boundaries() {
    let source = "<p>${first}${second}</p>";
    let regions = scan_template_interpolations(source).expect("adjacent interpolations");

    assert_eq!(regions.len(), 2);
    assert_eq!(
        &source[regions[0].expression_start..regions[0].expression_end],
        "first"
    );
    assert_eq!(
        &source[regions[1].expression_start..regions[1].expression_end],
        "second"
    );
    assert_eq!(regions[0].close_end, regions[1].open_start);
}

#[test]
fn scanner_rejects_empty_interpolation_with_exact_region() {
    let error =
        scan_template_interpolations("<p>${  }</p>").expect_err("empty interpolation must fail");

    assert_eq!(error.message, "template interpolation slot cannot be empty");
    assert_eq!((error.line, error.start, error.end), (1, 3, 8));
}
