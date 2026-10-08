use super::*;
use serde_json::json;

const KINDS: [&str; 5] = [
    "handler",
    "websocket",
    "SSE",
    "static response",
    "file response",
];

#[derive(Default)]
struct Routes {
    handlers: Vec<HandlerRoute>,
    websockets: Vec<WebSocketRoute>,
    sse: Vec<SseRoute>,
    responses: Vec<StaticResponse>,
    files: Vec<FileResponse>,
}

impl Routes {
    fn add(&mut self, kind: usize, method: &str, route: &str) {
        let wire = json!({
            "method": method, "route": route, "module": "app.Http",
            "function": "handle", "arity": 1, "protocol": "chat.v1",
            "source": {"path": "app/Http.terl", "line": 1, "column": 1},
            "status": 200, "content_type": "text/plain", "body": "hello",
            "path": "assets/hello.txt"
        });
        match kind {
            0 => self.handlers.push(serde_json::from_value(wire).unwrap()),
            1 => self.websockets.push(serde_json::from_value(wire).unwrap()),
            2 => self.sse.push(serde_json::from_value(wire).unwrap()),
            3 => self.responses.push(serde_json::from_value(wire).unwrap()),
            4 => self.files.push(serde_json::from_value(wire).unwrap()),
            _ => unreachable!(),
        }
    }

    fn validate(&self) -> Result<(), crate::ServiceError> {
        validate_route_namespace(
            &self.handlers,
            &self.websockets,
            &self.sse,
            &self.responses,
            &self.files,
        )
    }
}

#[test]
fn namespace_rejects_every_same_and_cross_section_collision() {
    for (first, first_label) in KINDS.iter().enumerate() {
        for second in 0..KINDS.len() {
            for (left, right) in [
                ("/same", "/same"),
                ("/users/:id", "/users/:name"),
                ("/users/{id:Int}", "/users/{name:String}"),
                ("/users/:id", "/users/{name:Int}"),
                ("/files/:id.json", "/files/:name.json"),
                ("/files/*", "/files/*"),
                ("*", "/*"),
            ] {
                for (left, right) in [(left, right), (right, left)] {
                    let mut routes = Routes::default();
                    routes.add(first, "GET", left);
                    routes.add(second, "GET", right);
                    let expected = if first == second {
                        let method = if first == 1 || first == 2 {
                            ""
                        } else {
                            "`GET` "
                        };
                        format!("error[serve_package]: duplicate or ambiguous {first_label} route {method}`{right}`")
                    } else {
                        let (earlier, earlier_route, later, later_route) = if first < second {
                            (first, left, second, right)
                        } else {
                            (second, right, first, left)
                        };
                        format!("error[serve_package]: {} route `GET` `{later_route}` conflicts with {} route `GET` `{earlier_route}`", KINDS[later], KINDS[earlier])
                    };
                    let error = routes.validate().unwrap_err();
                    assert_eq!(error.code(), "serve_package");
                    assert_eq!(error.to_string(), expected);
                }
            }
        }
    }
}

#[test]
fn namespace_preserves_method_slots_and_valid_route_precedence() {
    Routes::default().validate().unwrap();
    for kind in 0..KINDS.len() {
        let mut routes = Routes::default();
        for route in [
            "/",
            "/users/admin",
            "/users/:id",
            "/users/*",
            "*",
            "/files/:id.json",
            "/files/:id.txt",
        ] {
            routes.add(kind, "GET", route);
        }
        routes.validate().unwrap();
        // Upgrades and event streams occupy GET, not HEAD or POST.
        for method in ["HEAD", "POST", "get"] {
            for other in [0, 3, 4] {
                let mut routes = Routes::default();
                routes.add(kind, "GET", "/users/:id");
                routes.add(other, method, "/users/:name");
                routes.validate().unwrap();
            }
        }
    }
}

#[test]
fn namespace_rejects_malformed_patterns_in_every_section() {
    for kind in 0..KINDS.len() {
        for route in [
            "/../private",
            "/:Upper",
            "/users/{id:}",
            "/files/*/tail",
            "/users//item",
        ] {
            let mut routes = Routes::default();
            routes.add(kind, "GET", route);
            let expected = crate::route_pattern::route_ambiguity_key(route)
                .unwrap_err()
                .to_string();
            assert_eq!(routes.validate().unwrap_err().to_string(), expected);
        }
    }
}

#[test]
fn namespace_reports_first_collision_in_stable_section_and_row_order() {
    let mut routes = Routes::default();
    routes.add(4, "GET", "/conflict");
    routes.add(3, "GET", "/conflict");
    routes.add(2, "GET", "/conflict");
    routes.add(1, "GET", "/conflict");
    routes.add(0, "GET", "/conflict");
    routes.add(0, "GET", "/users/:id");
    routes.add(0, "GET", "/users/:name");
    assert_eq!(
        routes.validate().unwrap_err().to_string(),
        "error[serve_package]: duplicate or ambiguous handler route `GET` `/users/:name`"
    );
    routes.handlers.pop();
    assert_eq!(routes.validate().unwrap_err().to_string(), "error[serve_package]: websocket route `GET` `/conflict` conflicts with handler route `GET` `/conflict`");
}
