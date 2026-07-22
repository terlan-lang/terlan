use super::template_response::{
    render_http_template_response, VmAccountedHttpOutputError, VmAccountedHttpTemplateResponse,
    VmHttpTemplateResponse,
};
use crate::runtime::vm::{
    memory::{VmMemoryAccountant, VmMemoryLimits},
    process::{VmProcessSource, VmProcessTable},
    scheduler::VmScheduler,
};

#[test]
fn typed_template_response_uses_source_target_content_type() {
    let fixtures = [
        ("page.terl.html", "text/html; charset=utf-8"),
        ("readme.terl.md", "text/html; charset=utf-8"),
        ("data.terl.json", "application/json; charset=utf-8"),
        ("config.terl.toml", "application/toml; charset=utf-8"),
        ("data.terl.yaml", "application/yaml; charset=utf-8"),
        ("data.terl.yml", "application/yaml; charset=utf-8"),
        ("feed.terl.xml", "application/xml; charset=utf-8"),
        ("message.terl.txt", "text/plain; charset=utf-8"),
    ];

    for (source_file, expected) in fixtures {
        let template = VmHttpTemplateResponse::typed("Artifact", source_file, "rendered")
            .expect("supported target");
        let response =
            render_http_template_response(template, http::StatusCode::OK).expect("typed response");
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            expected,
            "source {source_file}"
        );
    }
}

#[test]
fn unsupported_template_target_fails_before_vm_accounting() {
    let direct = VmHttpTemplateResponse::typed("Artifact", "template.html", "rendered")
        .expect_err("non-Terlan suffix must fail");
    assert!(direct.starts_with("template_runtime_unsupported_target:"));

    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Http", "template", 0));
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(32, 64).expect("limits"));
    let mut scheduler = VmScheduler::default();
    let error = VmAccountedHttpTemplateResponse::typed(
        &mut memory,
        &mut scheduler,
        &mut processes,
        owner,
        "Artifact",
        "template.unsupported",
        "rendered",
    )
    .expect_err("unsupported target must fail before accounting");

    assert!(matches!(
        error,
        VmAccountedHttpOutputError::Template(message)
            if message.starts_with("template_runtime_unsupported_target:")
    ));
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(scheduler.memory_reductions(owner), 0);
}

#[test]
fn html_helper_rejects_structured_non_html_target() {
    let error = VmHttpTemplateResponse::html("Data", "data.terl.json", "{}")
        .expect_err("HTML helper must reject JSON");

    assert_eq!(
        error,
        "template_runtime_unsupported_target: VM HTTP HTML response cannot render json template `data.terl.json`"
    );
}
