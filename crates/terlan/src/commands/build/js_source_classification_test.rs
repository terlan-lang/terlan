use super::*;

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
fn imported_http_handler_marks_a_route_source_module() {
    let source = "module app.handlers.Pages.\n\nimport type std.http.{Request, Response}.\n\npub index(request: Request): Response -> request.respond().\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse handler provider fixture");

    assert!(is_web_route_source_module(&syntax));
}

#[test]
fn ordinary_server_model_does_not_mark_a_route_source_module() {
    let source = "module app.model.Player.\n\npub name(): String -> \"player\".\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse model fixture");

    assert!(!is_web_route_source_module(&syntax));
}
