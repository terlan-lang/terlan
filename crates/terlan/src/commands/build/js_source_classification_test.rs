use super::*;

#[test]
fn grouped_router_import_marks_a_route_source_module() {
    let source = "module app.Http.\n\nimport std.http.{Response, Router}.\n\nimport type std.http.Router.\n\npub router(): Router -> Router.new().\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse grouped router fixture");

    assert!(is_web_route_source_module(&syntax));
}
