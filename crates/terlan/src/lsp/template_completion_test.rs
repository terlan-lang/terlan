use super::*;
use crate::terlan_lsp::document::OpenDocuments;

fn completion_fixture(body: &str, marker: &str) -> Vec<CompletionItem> {
    let uri = Url::parse("file:///tmp/card.terl.html").expect("template URI");
    let documents = OpenDocuments::default();
    documents.open(
        uri.clone(),
        body.to_string(),
        1,
        "terlan-template-html".to_string(),
    );
    let document = documents.snapshot(&uri).expect("open template");
    let offset = body.find(marker).expect("completion marker") + marker.len();
    let prefix = &body[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, tail)| tail)
        .encode_utf16()
        .count() as u32;
    template_completion_items(&uri, &document, Position::new(line, column))
}

fn template_source(body: &str) -> String {
    format!(
        "@template {{\n  params = {{\n    title: String\n    url: Uri\n    disabled: Bool\n    body: Template.Html\n  }}\n}}\n\n{body}"
    )
}

#[test]
fn completion_exposes_declared_params_with_text_and_trusted_contexts() {
    let source = template_source("<main>${ti}</main>");
    let items = completion_fixture(&source, "${ti");

    assert!(items.iter().any(|item| {
        item.label == "title"
            && item
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[TextSlot]"))
    }));
    assert!(items.iter().any(|item| {
        item.label == "body"
            && item
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[TrustedFragmentSlot]"))
    }));
}

#[test]
fn completion_classifies_url_and_boolean_attribute_contexts() {
    let url_source = template_source(r#"<a href="${ur}">link</a>"#);
    let bool_source = template_source(r#"<button disabled="${di}">go</button>"#);
    let attr_source = template_source(r#"<main aria-label="${ti}">go</main>"#);

    assert!(completion_fixture(&url_source, "${ur").iter().any(|item| {
        item.label == "url"
            && item
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[UrlSlot]"))
    }));
    assert!(completion_fixture(&bool_source, "${di").iter().any(|item| {
        item.label == "disabled"
            && item
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[BoolSlot]"))
    }));
    assert!(completion_fixture(&attr_source, "${ti").iter().any(|item| {
        item.label == "title"
            && item
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[AttrSlot]"))
    }));
}

#[test]
fn completion_supports_html_native_text_and_attribute_interpolation() {
    let text_source = template_source("<main>{ti}</main>");
    let url_source = template_source(r#"<a href={ur}>link</a>"#);

    assert!(completion_fixture(&text_source, "{ti").iter().any(|item| {
        item.label == "title"
            && item
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[TextSlot]"))
    }));
    assert!(completion_fixture(&url_source, "{ur").iter().any(|item| {
        item.label == "url"
            && item
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("[UrlSlot]"))
    }));
}

#[test]
fn completion_is_empty_outside_interpolation() {
    let source = template_source("<main>plain text</main>");
    assert!(completion_fixture(&source, "plain").is_empty());
}

#[test]
fn completion_is_empty_without_declared_template_params() {
    let source = "<main>${missing}</main>";
    assert!(completion_fixture(source, "${miss").is_empty());
}

#[test]
fn malformed_interpolation_preserves_exact_opening_span() {
    let uri = Url::parse("file:///tmp/card.terl.html").expect("template URI");
    let documents = OpenDocuments::default();
    let source = "<main>\n  ${title\n</main>";
    documents.open(
        uri.clone(),
        source.to_string(),
        1,
        "terlan-template-html".to_string(),
    );

    let document = documents.snapshot(&uri).expect("open template");
    let diagnostic = document
        .template_diagnostics
        .first()
        .expect("template diagnostic");
    assert_eq!(
        diagnostic.message,
        "unterminated template interpolation slot"
    );
    assert_eq!(
        diagnostic.span,
        Some(crate::terlan_html::HtmlSpan {
            line: 2,
            start: 2,
            end: 4,
        })
    );
}

#[test]
fn malformed_interpolation_span_converts_utf8_columns_to_lsp_utf16() {
    let source = "<main>\n  café ${title";
    let error = crate::terlan_html::scan_template_interpolations(source)
        .expect_err("unterminated interpolation");
    let range = crate::terlan_lsp::document::OpenDocument::range_from_html_span(
        source,
        crate::terlan_html::HtmlSpan {
            line: error.line as u64,
            start: error.start,
            end: error.end,
        },
    );

    assert_eq!(range.start, Position::new(1, 7));
    assert_eq!(range.end, Position::new(1, 9));
}
