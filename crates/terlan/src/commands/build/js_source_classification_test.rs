use super::*;

#[test]
fn delegated_router_is_discovered_without_a_router_provider_import() {
    let source = "module app.Http.\nimport app.Pages.\npub router(): std.http.Router.Router -> Pages.make().\n";
    let syntax = parse_module_as_syntax_output(source).unwrap();
    assert!(is_web_route_source_module(&syntax));
    for source in [
        "module app.Http.\nrouter(): Int -> 0.\n",
        "module app.Http.\npub router(value: Int): Int -> value.\n",
    ] {
        let syntax = parse_module_as_syntax_output(source).unwrap();
        assert!(!is_web_route_source_module(&syntax));
    }
}

#[test]
fn grouped_router_import_marks_a_route_source_module() {
    let source = "module app.Http.\n\nimport std.http.{Response, Router}.\n\nimport type std.http.Router.\n\npub router(): Router -> Router.new().\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse grouped router fixture");

    assert!(is_web_route_source_module(&syntax));
}

#[test]
fn default_router_import_marks_a_route_source_module() {
    let source = "module app.Http.\n\nimport std.http.Router.\nimport type std.http.Router.Router.\n\npub router(): Router -> Router.new().\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse default router fixture");

    assert!(is_web_route_source_module(&syntax));
}

#[test]
fn imported_http_handler_is_a_dependency_not_a_route_entrypoint() {
    let source = "module app.handlers.Pages.\n\nimport type std.http.{Request, Response}.\n\npub index(request: Request): Response -> request.respond().\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse handler provider fixture");

    assert!(!is_web_route_source_module(&syntax));
}

#[test]
fn unrelated_request_and_response_types_do_not_mark_route_sources() {
    for source in [
        "module app.Model.\npub type Request = Int.\npub type Response = String.\npub convert(value: Request): Response -> \"model\".",
        "module app.Pages.\nimport type std.http.Request.{Request as Incoming}.\nimport type std.http.Response.{Response as Outgoing}.\npub home(request: Incoming): Outgoing -> request.respond().",
    ] {
        let syntax = parse_module_as_syntax_output(source).unwrap();
        assert!(!is_web_route_source_module(&syntax));
    }
}

#[test]
fn route_discovery_passes_only_public_exports_to_the_package() {
    for module in ["app.WebSocket", "app.RoomProtocol"] {
        for (declarations, expected) in [
            (
                "pub route(): String -> \"/\".\npub protocol(): String -> \"chat\".",
                true,
            ),
            (
                "route(): String -> \"/\".\npub protocol(): String -> \"chat\".",
                false,
            ),
            (
                "pub route(): String -> \"/\".\nprotocol(): String -> \"chat\".",
                false,
            ),
            (
                "pub route(value: Int): String -> \"/\".\npub protocol(): String -> \"chat\".",
                false,
            ),
            ("pub router(): Int -> 1.", true),
        ] {
            let syntax =
                parse_module_as_syntax_output(&format!("module {module}.\n{declarations}"))
                    .unwrap();
            assert_eq!(
                is_web_route_source_module(&syntax),
                expected,
                "{module}: {declarations}"
            );
        }
    }
}

#[test]
fn ordinary_server_model_does_not_mark_a_route_source_module() {
    let source = "module app.model.Player.\n\npub name(): String -> \"player\".\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse model fixture");

    assert!(!is_web_route_source_module(&syntax));
}
