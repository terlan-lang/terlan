use super::*;
use crate::manifest::{validate_route_namespace, SourceSpan};

#[derive(Default)]
struct Fixture {
    handlers: Vec<HandlerRoute>,
    sse: Vec<SseRoute>,
    static_responses: Vec<StaticResponse>,
    file_responses: Vec<FileResponse>,
    websockets: Vec<WebSocketRoute>,
}

impl Fixture {
    fn table(&self) -> RouteTable<'_> {
        RouteTable {
            handlers: &self.handlers,
            sse: &self.sse,
            static_responses: &self.static_responses,
            file_responses: &self.file_responses,
            websockets: &self.websockets,
        }
    }

    fn add(&mut self, kind: usize, method: &str, path: &str) {
        let source = SourceSpan {
            path: "src/app/Web.terl".into(),
            line: 4,
            column: 1,
        };
        match kind {
            0 => self.handlers.push(HandlerRoute {
                method: method.into(),
                route: path.into(),
                module: "app.Web".into(),
                function: "handle".into(),
                arity: 1,
                source: Some(source),
            }),
            1 => {
                assert_eq!(method, "GET");
                self.sse.push(SseRoute {
                    module: "app.Events".into(),
                    route: path.into(),
                    source,
                });
            }
            2 => self.static_responses.push(StaticResponse {
                module: "app.Static".into(),
                function: "text".into(),
                arity: 0,
                method: method.into(),
                route: path.into(),
                status: 201,
                content_type: "text/plain".into(),
                headers: vec![],
                body: "body".into(),
                source: Some(source),
            }),
            3 => self.file_responses.push(FileResponse {
                module: "app.Files".into(),
                function: "download".into(),
                arity: 0,
                method: method.into(),
                route: path.into(),
                path: "downloads/report.txt".into(),
                status: 200,
                content_type: None,
                source: Some(source),
            }),
            _ => unreachable!(),
        }
    }

    fn validate(&self) {
        validate_route_namespace(
            &self.handlers,
            &self.websockets,
            &self.sse,
            &self.static_responses,
            &self.file_responses,
        )
        .expect("unambiguous namespace");
    }
}

fn kind(endpoint: Endpoint<'_>) -> usize {
    match endpoint {
        Endpoint::Handler(_) => 0,
        Endpoint::Sse(_) => 1,
        Endpoint::StaticResponse(_) => 2,
        Endpoint::FileResponse(_) => 3,
    }
}

#[test]
fn every_endpoint_kind_competes_by_specificity_not_storage_order() {
    for winner in 0..4 {
        for loser in 0..4 {
            for (specific, general, request, params) in [
                ("/items/exact", "/items/:id", "/items/exact", vec![]),
                (
                    "/items/{id:Int}",
                    "/items/*",
                    "/items/42",
                    vec![("id".into(), "42".into())],
                ),
                (
                    "/items/*",
                    "/*",
                    "/items/a/b",
                    vec![("*".into(), "a/b".into())],
                ),
            ] {
                let mut fixture = Fixture::default();
                fixture.add(winner, "GET", specific);
                fixture.add(loser, "GET", general);
                fixture.validate();
                for method in ["GET", "HEAD"] {
                    let selected = fixture.table().select(method, request).unwrap();
                    assert_eq!(kind(selected.endpoint), winner);
                    assert_eq!(selected.params, params);
                    assert_eq!(selected.endpoint.describe(), ("GET", specific));
                }
                assert!(fixture.table().select("POST", request).is_none());
            }
        }
    }
}

#[test]
fn explicit_head_precedes_more_specific_get_across_endpoint_kinds() {
    for get_kind in 0..4 {
        for head_kind in [0, 2, 3] {
            let mut fixture = Fixture::default();
            fixture.add(get_kind, "GET", "/items/exact");
            fixture.add(head_kind, "HEAD", "/items/*");
            fixture.validate();
            let selected = fixture.table().select("HEAD", "/items/exact").unwrap();
            assert_eq!(kind(selected.endpoint), head_kind);
            assert_eq!(selected.endpoint.describe(), ("HEAD", "/items/*"));
            let selected = fixture.table().select("GET", "/items/exact").unwrap();
            assert_eq!(kind(selected.endpoint), get_kind);
        }
    }
}

#[test]
fn selected_rows_are_the_original_manifest_objects() {
    let mut fixture = Fixture::default();
    for endpoint in 0..4 {
        fixture.add(endpoint, "GET", &format!("/kind/{endpoint}"));
    }
    fixture.validate();
    for endpoint in 0..4 {
        let selected = fixture
            .table()
            .select("GET", &format!("/kind/{endpoint}"))
            .unwrap();
        assert!(match selected.endpoint {
            Endpoint::Handler(row) => std::ptr::eq(row, &fixture.handlers[0]),
            Endpoint::Sse(row) => std::ptr::eq(row, &fixture.sse[0]),
            Endpoint::StaticResponse(row) => std::ptr::eq(row, &fixture.static_responses[0]),
            Endpoint::FileResponse(row) => std::ptr::eq(row, &fixture.file_responses[0]),
        });
    }
}

#[test]
fn typed_capture_rejection_and_empty_tables_never_synthesize_endpoints() {
    assert!(RouteTable::default().select("GET", "/").is_none());
    assert!(RouteTable::default().is_handler_only());
    for endpoint in 0..4 {
        let mut fixture = Fixture::default();
        fixture.add(endpoint, "GET", "/items/{id:Int}");
        for path in [
            "/items/no",
            "/items/9223372036854775808",
            "/",
            "/items/1/extra",
        ] {
            assert!(fixture.table().select("GET", path).is_none(), "{path}");
        }
        assert!(fixture.table().select("get", "/items/1").is_none());
        assert_eq!(fixture.table().is_handler_only(), endpoint == 0);
    }
}

#[test]
fn websocket_reservation_is_exact_and_separate_from_regular_dispatch() {
    let mut fixture = Fixture::default();
    fixture.websockets.push(WebSocketRoute {
        module: "app.Socket".into(),
        route: "/socket".into(),
        protocol: "chat".into(),
        source: None,
    });
    assert!(!fixture.table().is_handler_only());
    assert!(std::ptr::eq(
        fixture.table().websocket("/socket").unwrap(),
        &fixture.websockets[0]
    ));
    for path in ["/", "/socket/", "/socket/child"] {
        assert!(fixture.table().websocket(path).is_none());
    }
    assert!(fixture.table().select("GET", "/socket").is_none());
}
