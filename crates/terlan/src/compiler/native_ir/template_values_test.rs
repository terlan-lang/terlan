//! Tests for compiler-owned typed HTML fragment lowering.

use std::sync::Arc;

use crate::runtime::native_image::managed::encode_string_list_join_operation;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, CoreExpr, CoreImport, CoreImportKind, CoreModule,
    CoreRecordExprField, CoreTemplateExpression, CoreTemplateProp, CoreTemplateRenderPlan,
    CoreType,
};

use super::template_values::{
    lower_managed_template_operation, lower_template_values, managed_template_operation_type,
};
use super::{native_type, NativeExpr, NativeType};

/// Creates one checked module importing the public template facade.
fn template_core() -> CoreModule {
    let syntax = parse_module_as_syntax_output("module app.View.\n\npub render(): Int -> 1.\n")
        .expect("parse template lowering fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    core.imports.push(CoreImport {
        module: "std.template.Template".to_string(),
        kind: CoreImportKind::Module,
    });
    core
}

/// Returns the mutable body of the fixture's only function.
fn body(core: &mut CoreModule) -> &mut CoreExpr {
    core.functions[0].clauses[0]
        .body
        .core_expr
        .as_mut()
        .expect("typed body")
}

/// Creates one public template module call.
fn template_call(function: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::RemoteCall {
        module: "std.template.Template".to_string(),
        function: function.to_string(),
        args,
    }
}

/// Creates one unnamed checked render plan for a focused lowering test.
fn render_plan(
    name: &str,
    props: Vec<CoreTemplateProp>,
    expressions: Vec<CoreTemplateExpression>,
    nodes: Vec<crate::terlan_html::HtmlNode>,
) -> CoreTemplateRenderPlan {
    CoreTemplateRenderPlan {
        name: name.to_string(),
        source_path: format!("./{}.terl.html", name.to_ascii_lowercase()),
        props,
        expressions,
        template: crate::terlan_html::HtmlTemplate::new(nodes),
    }
}

/// Installs one template instantiation as the fixture function body.
fn instantiate(core: &mut CoreModule, name: &str, fields: Vec<CoreRecordExprField>) {
    *body(core) = CoreExpr::TemplateInstantiate {
        name: name.to_string(),
        fields,
    };
}

#[test]
fn public_html_types_erase_to_managed_strings_only_after_typechecking() {
    for name in ["Html", "Template.Html", "std.template.Template.Html"] {
        let ty = CoreType::Named(name.to_string());
        assert_eq!(native_type(Some(&ty), name), Some(NativeType::StringRef));
    }
    let unrelated = CoreType::Named("app.Html".to_string());
    assert!(matches!(
        native_type(Some(&unrelated), "app.Html"),
        Some(NativeType::ManagedRef(_))
    ));
}

#[test]
fn trusted_and_empty_fragments_lower_to_the_string_representation() {
    let mut core = template_core();
    *body(&mut core) = template_call("trusted", vec![template_call("empty", Vec::new())]);
    lower_template_values(&mut core).expect("lower fragments");
    assert_eq!(body(&mut core), &CoreExpr::Binary("\"\"".to_string()));
}

#[test]
fn literal_join_lowers_to_ordered_managed_appends_without_a_list_literal() {
    let mut core = template_core();
    *body(&mut core) = template_call(
        "join",
        vec![CoreExpr::List(vec![
            template_call("trusted", vec![CoreExpr::Binary("\"<p>\"".to_string())]),
            template_call("trusted", vec![CoreExpr::Binary("\"Ada\"".to_string())]),
            template_call("trusted", vec![CoreExpr::Binary("\"</p>\"".to_string())]),
        ])],
    );
    lower_template_values(&mut core).expect("lower literal join");

    assert_eq!(
        body(&mut core),
        &CoreExpr::RemoteCall {
            module: "$terlan.managed.template".to_string(),
            function: "append".to_string(),
            args: vec![
                CoreExpr::RemoteCall {
                    module: "$terlan.managed.template".to_string(),
                    function: "append".to_string(),
                    args: vec![
                        CoreExpr::Binary("\"<p>\"".to_string()),
                        CoreExpr::Binary("\"Ada\"".to_string()),
                    ],
                },
                CoreExpr::Binary("\"</p>\"".to_string()),
            ],
        }
    );
}

#[test]
fn list_constructor_join_uses_the_same_literal_fragment_lowering() {
    let mut core = template_core();
    *body(&mut core) = template_call(
        "join",
        vec![CoreExpr::ConstructorCall {
            constructor: "std.collections.List.List".to_string(),
            constructor_identity: Some("std.collections.List.List/2".to_string()),
            args: vec![
                CoreExpr::Binary("\"left\"".to_string()),
                CoreExpr::Binary("\"right\"".to_string()),
            ],
        }],
    );
    lower_template_values(&mut core).expect("lower list constructor join");
    assert!(matches!(
        body(&mut core),
        CoreExpr::RemoteCall { module, function, args }
            if module == "$terlan.managed.template" && function == "append" && args.len() == 2
    ));
}

#[test]
fn dynamic_join_lowers_to_the_checked_managed_list_operation() {
    let mut core = template_core();
    *body(&mut core) = template_call("join", vec![CoreExpr::Var("fragments".to_string())]);
    lower_template_values(&mut core).expect("lower dynamic join");
    assert_eq!(
        managed_template_operation_type(body(&mut core)),
        Some(NativeType::StringRef)
    );
    let lowered = lower_managed_template_operation(body(&mut core), |_| Ok(NativeExpr::Param(0)))
        .expect("lower operation")
        .expect("managed operation");
    assert_eq!(
        lowered,
        NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_string_list_join_operation()),
            args: vec![NativeExpr::Param(0)],
        }
    );
}

#[test]
fn malformed_public_template_calls_fail_with_typed_diagnostics() {
    for (function, args, diagnostic) in [
        (
            "trusted",
            Vec::new(),
            "error[native_ir.template_arity]: Template.trusted does not accept 0 argument(s)",
        ),
        (
            "unknown",
            Vec::new(),
            "error[native_ir.template_function]: Template.unknown/0 is not in the managed template profile",
        ),
    ] {
        let mut core = template_core();
        *body(&mut core) = template_call(function, args);
        assert_eq!(lower_template_values(&mut core).unwrap_err(), diagnostic);
    }
}

#[test]
fn checked_template_instantiation_lowers_to_context_escape_operations() {
    let mut core = template_core();
    core.templates.push(CoreTemplateRenderPlan {
        name: "Page".to_string(),
        source_path: "./page.terl.html".to_string(),
        props: vec![CoreTemplateProp {
            name: "title".to_string(),
            ty: CoreType::String,
            default: None,
        }],
        expressions: Vec::new(),
        template: crate::terlan_html::HtmlTemplate::new(vec![
            crate::terlan_html::HtmlNode::Element(crate::terlan_html::HtmlElement {
                name: "h1".to_string(),
                attrs: Vec::new(),
                children: vec![crate::terlan_html::HtmlNode::Slot(
                    crate::terlan_html::HtmlSlot::dotted("title"),
                )],
            }),
        ]),
    });
    *body(&mut core) = CoreExpr::TemplateInstantiate {
        name: "Page".to_string(),
        fields: vec![CoreRecordExprField {
            key: "title".to_string(),
            required: true,
            value: CoreExpr::Binary("\"<unsafe>\"".to_string()),
        }],
    };

    lower_template_values(&mut core).expect("lower render plan");
    let rendered = body(&mut core).contract_text();
    assert!(rendered.contains("RemoteCall($terlan.managed.template:render_text_string;"));
    assert!(!rendered.contains("TemplateInstantiate"));
}

#[test]
fn template_instantiation_without_a_checked_plan_fails_closed() {
    let mut core = template_core();
    *body(&mut core) = CoreExpr::TemplateInstantiate {
        name: "Missing".to_string(),
        fields: Vec::new(),
    };
    assert_eq!(
        lower_template_values(&mut core).unwrap_err(),
        "error[native_ir.template_plan_missing]: template `Missing` has no checked render plan"
    );
}

#[test]
fn malformed_checked_template_surface_has_stable_typed_diagnostics() {
    let string_prop = |name: &str| CoreTemplateProp {
        name: name.to_string(),
        ty: CoreType::String,
        default: None,
    };
    let slot_attribute = |name: &str, slot: &str| {
        crate::terlan_html::HtmlNode::Element(crate::terlan_html::HtmlElement {
            name: "div".to_string(),
            attrs: vec![crate::terlan_html::HtmlAttr {
                name: name.to_string(),
                value: Some(crate::terlan_html::HtmlAttrValue::Slot(
                    crate::terlan_html::HtmlSlot::dotted(slot),
                )),
            }],
            children: Vec::new(),
        })
    };

    let mut unsafe_url = template_core();
    unsafe_url.templates.push(render_plan(
        "Page",
        vec![string_prop("href")],
        Vec::new(),
        vec![slot_attribute("href", "href")],
    ));
    instantiate(
        &mut unsafe_url,
        "Page",
        vec![CoreRecordExprField {
            key: "href".to_string(),
            required: true,
            value: CoreExpr::Binary("\"javascript:alert(1)\"".to_string()),
        }],
    );
    assert_eq!(
        lower_template_values(&mut unsafe_url).unwrap_err(),
        "error[native_ir.template_attribute]: template URL attribute `href` rejects an unsafe URL"
    );

    let mut invalid_tokens = template_core();
    invalid_tokens.templates.push(render_plan(
        "Page",
        vec![CoreTemplateProp {
            name: "classes".to_string(),
            ty: CoreType::List(Box::new(CoreType::String)),
            default: None,
        }],
        Vec::new(),
        vec![slot_attribute("class", "classes")],
    ));
    instantiate(
        &mut invalid_tokens,
        "Page",
        vec![CoreRecordExprField {
            key: "classes".to_string(),
            required: true,
            value: CoreExpr::List(vec![CoreExpr::Binary("\"bad token\"".to_string())]),
        }],
    );
    assert_eq!(
        lower_template_values(&mut invalid_tokens).unwrap_err(),
        "error[native_ir.template_attribute]: template token-list attribute `class` has invalid token at index 0"
    );

    let mut invalid_path = template_core();
    invalid_path.templates.push(render_plan(
        "Page",
        vec![CoreTemplateProp {
            name: "user".to_string(),
            ty: CoreType::Struct {
                name: "User".to_string(),
                fields: vec![crate::terlan_typeck::CoreStructTypeField {
                    name: "name".to_string(),
                    ty: CoreType::String,
                    is_private: false,
                }],
            },
            default: None,
        }],
        Vec::new(),
        vec![crate::terlan_html::HtmlNode::Slot(
            crate::terlan_html::HtmlSlot::dotted("user.missing"),
        )],
    ));
    instantiate(
        &mut invalid_path,
        "Page",
        vec![CoreRecordExprField {
            key: "user".to_string(),
            required: true,
            value: CoreExpr::Var("user_value".to_string()),
        }],
    );
    assert_eq!(
        lower_template_values(&mut invalid_path).unwrap_err(),
        "error[native_ir.template_slot_path]: template `Page` slot `user.missing` has no checked field `missing`"
    );

    let mut invalid_expression = template_core();
    invalid_expression.templates.push(render_plan(
        "Page",
        vec![CoreTemplateProp {
            name: "values".to_string(),
            ty: CoreType::List(Box::new(CoreType::Int)),
            default: None,
        }],
        vec![CoreTemplateExpression {
            source: "values + values".to_string(),
            expr: CoreExpr::Var("values".to_string()),
            ty: CoreType::List(Box::new(CoreType::Int)),
        }],
        vec![crate::terlan_html::HtmlNode::Slot(
            crate::terlan_html::HtmlSlot {
                expression: "values + values".to_string(),
                path: Vec::new(),
                span: None,
            },
        )],
    ));
    instantiate(
        &mut invalid_expression,
        "Page",
        vec![CoreRecordExprField {
            key: "values".to_string(),
            required: true,
            value: CoreExpr::Var("values_value".to_string()),
        }],
    );
    assert_eq!(
        lower_template_values(&mut invalid_expression).unwrap_err(),
        "error[native_ir.template_render_type]: `List(Int)` has no managed template renderer"
    );

    let mut invalid_component = template_core();
    let mut badge = render_plan("Badge", vec![string_prop("label")], Vec::new(), Vec::new());
    badge.template.tag_name = Some("typed-template-badge".to_string());
    invalid_component.templates.push(badge);
    invalid_component.templates.push(render_plan(
        "Page",
        Vec::new(),
        Vec::new(),
        vec![crate::terlan_html::HtmlNode::Element(
            crate::terlan_html::HtmlElement {
                name: "typed-template-badge".to_string(),
                attrs: vec![crate::terlan_html::HtmlAttr {
                    name: "unknown".to_string(),
                    value: Some(crate::terlan_html::HtmlAttrValue::Text("value".to_string())),
                }],
                children: Vec::new(),
            },
        )],
    ));
    instantiate(&mut invalid_component, "Page", Vec::new());
    assert_eq!(
        lower_template_values(&mut invalid_component).unwrap_err(),
        "error[native_ir.template_component_prop_unknown]: component `typed-template-badge` has no prop `unknown`"
    );
}
