use super::*;
use crate::terlan_syntax::parse_module_as_syntax_output;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn slot_messages_with_module(source: &str, html: &str, props: &[(&str, &str)]) -> Vec<String> {
    let module = parse_module_as_syntax_output(source).expect("parse template helper module");
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;
    let template = template_decl("HelperCard", props);
    let parsed = crate::terlan_html::parse_template(html, "helper_card.terl.html")
        .expect("parse template fixture");
    check_template_slots(
        &template,
        &parsed,
        &syntax_template_struct_fields(&module),
        Some(TemplateExpressionContext::new(&module, &resolved)),
    )
    .into_iter()
    .map(|diagnostic| diagnostic.message)
    .collect()
}

fn template_decl(name: &str, props: &[(&str, &str)]) -> TemplateCheckDecl {
    TemplateCheckDecl {
        name: name.to_string(),
        source_path: "./helper_card.terl.html".to_string(),
        resolved_path: "/tmp/helper_card.terl.html".to_string(),
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

fn template_entrypoint_messages(source: &str, html: &str) -> Vec<String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after Unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terlan_template_purity_{}_{}",
        std::process::id(),
        nonce
    ));
    let templates = root.join("templates");
    fs::create_dir_all(&templates).expect("create template fixture directory");
    fs::write(templates.join("card.terl.html"), html).expect("write template fixture");
    let source_path = root.join("main.terl");
    fs::write(&source_path, source).expect("write source fixture");

    let module = parse_module_as_syntax_output(source).expect("parse source fixture");
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;
    let messages = type_check_syntax_module_output_with_templates(&module, &resolved, &source_path)
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect();
    fs::remove_dir_all(root).expect("remove template fixture directory");
    messages
}

/// Proves ordinary body-available helpers are inferred pure in template slots.
#[test]
fn typed_template_accepts_inferred_pure_local_helper() {
    assert_eq!(
        slot_messages_with_module(
            "module inferred_template_helper.\n\nnormalize(value: Int): Int -> value + 1.\n",
            "<p>${normalize(value)}</p>",
            &[("value", "Int")],
        ),
        Vec::<String>::new()
    );
}

/// Proves explicit purity contracts remain usable by typed template slots.
#[test]
fn typed_template_accepts_asserted_pure_local_helper() {
    assert_eq!(
        slot_messages_with_module(
            "module asserted_template_helper.\n\n@pure\nnormalize(value: Int): Int -> value + 1.\n",
            "<p>${normalize(value)}</p>",
            &[("value", "Int")],
        ),
        Vec::<String>::new()
    );
}

/// Prevents local mutation from entering rendering through an inferred helper.
#[test]
fn typed_template_rejects_inferred_impure_local_helper() {
    assert_eq!(
        slot_messages_with_module(
            "module impure_template_helper.\n\nreplace_at(items: List[Int]): Unit -> items[0] = 1.\n",
            "<p>${replace_at(items)}</p>",
            &[("items", "List[Int]")],
        ),
        vec![
            "template `HelperCard` slot expression `replace_at(items)` must be pure; found effectful local function call (template line 1, columns 1-20)"
        ]
    );
}

/// Proves the external-template entrypoint preserves module purity context.
#[test]
fn typed_template_entrypoint_rejects_inferred_impure_local_helper() {
    let diagnostics = template_entrypoint_messages(
        "module impure_template_entrypoint.\n\ntemplate Card from \"./templates/card.terl.html\" {\n    items: List[Int]\n}.\n\nreplace_at(items: List[Int]): Unit -> items[0] = 1.\n",
        "<p>${replace_at(items)}</p>",
    );

    assert!(
        diagnostics.iter().any(|message| message ==
            "template `Card` slot expression `replace_at(items)` must be pure; found effectful local function call (template line 1, columns 1-20)"),
        "diagnostics: {diagnostics:?}"
    );
}
