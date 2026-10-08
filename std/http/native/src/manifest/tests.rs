use super::*;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

fn round_trip<T: DeserializeOwned + Serialize + PartialEq + std::fmt::Debug>(wire: Value) -> T {
    let row: T = serde_json::from_value(wire).unwrap();
    let decoded: T = serde_json::from_slice(&serde_json::to_vec(&row).unwrap()).unwrap();
    assert_eq!(row, decoded);
    row
}

fn source() -> Value {
    json!({"path":"src/app/Http.terl", "line":2, "column":3})
}

fn handler() -> Value {
    json!({"method":"GET", "route":"/users/{id:Int}", "module":"app.Http",
        "function":"show", "arity":2, "source":source()})
}

fn static_response() -> Value {
    json!({"method":"GET", "route":"/", "status":200,
        "content_type":"text/plain", "body":"hello"})
}

fn file_response() -> Value {
    json!({"method":"GET", "route":"/file", "path":"assets/file.txt", "status":200})
}

#[test]
fn shared_route_records_round_trip_with_optional_fields_and_build_metadata() {
    for arity in [1, 2] {
        let mut wire = handler();
        wire["arity"] = json!(arity);
        wire["future_build_metadata"] = json!("ignored");
        let row = round_trip::<HandlerRoute>(wire);
        validate_handler(&row).unwrap();
    }
    let mut no_source = handler();
    no_source.as_object_mut().unwrap().remove("source");
    let row = round_trip::<HandlerRoute>(no_source);
    assert!(row.source.is_none());
    assert!(serde_json::to_value(&row).unwrap().get("source").is_none());
    validate_handler(&row).unwrap();
    let websocket = round_trip::<WebSocketRoute>(json!({"route":"/socket", "protocol":"chat.v1"}));
    assert!(websocket.module.is_empty());
    validate_websocket(&websocket).unwrap();
    let websocket = round_trip::<WebSocketRoute>(json!({"module":"app.Chat", "route":"/socket",
        "protocol":"chat.v1", "source":source()}));
    validate_websocket(&websocket).unwrap();
    let sse = round_trip::<SseRoute>(
        json!({"module":"app.Events", "route":"/events", "source":source()}),
    );
    validate_sse(&sse).unwrap();
    let error =
        round_trip::<ErrorHandler>(json!({"module":"app.Http", "function":"recover", "arity":1}));
    validate_error_handler(&error).unwrap();
    let response = round_trip::<StaticResponse>(static_response());
    assert!(response.headers.is_empty());
    assert!(response.module.is_empty());
    assert!(serde_json::to_value(&response)
        .unwrap()
        .get("headers")
        .is_none());
    validate_static_response(&response).unwrap();
    let response = round_trip::<FileResponse>(file_response());
    assert!(response.content_type.is_none());
    validate_file_response(&response).unwrap();
}

#[test]
fn handler_admission_rejects_invalid_identity_routes_arity_and_spans() {
    for (key, invalid) in [
        ("method", json!("CUSTOM")),
        ("route", json!("relative")),
        ("route", json!("/bad\\route")),
        ("route", json!("/bad\0route")),
        ("route", json!("/bad?q=1")),
        ("route", json!("/bad#fragment")),
        ("route", json!("/../bad")),
        ("module", json!("")),
        ("module", json!("app..Http")),
        ("module", json!("app/Http")),
        ("function", json!("Upper")),
        ("function", json!("")),
        ("function", json!("bad-name")),
        ("arity", json!(0)),
        ("arity", json!(3)),
    ] {
        let mut wire = handler();
        wire[key] = invalid;
        let row: HandlerRoute = serde_json::from_value(wire.clone()).unwrap();
        assert!(validate_handler(&row).is_err(), "{wire}");
    }
    for route in ["*", "/", "/users/:id", "/assets/*"] {
        let mut wire = handler();
        wire["method"] = json!("get");
        wire["route"] = json!(route);
        wire["arity"] = json!(1);
        wire["function"] = json!("_handler");
        validate_handler(&round_trip(wire)).unwrap();
    }
    for path in [
        "",
        " ",
        "/absolute.terl",
        "../parent.terl",
        "src/../parent.terl",
        "src\\file.terl",
        "src/\0.terl",
    ] {
        let mut wire = handler();
        wire["source"]["path"] = json!(path);
        assert!(validate_handler(&round_trip(wire)).is_err(), "{path:?}");
    }
    for coordinate in ["line", "column"] {
        let mut wire = handler();
        wire["source"][coordinate] = json!(0);
        assert!(validate_handler(&round_trip(wire)).is_err());
    }
}

#[test]
fn channel_and_error_handler_admission_require_valid_source_contracts() {
    let wire = json!({"module":"app.Events", "route":"/events", "source":source()});
    for source in [
        Value::Null,
        json!({}),
        json!({"path":"src/a.terl","line":1}),
    ] {
        let mut invalid = wire.clone();
        invalid["source"] = source;
        assert!(serde_json::from_value::<SseRoute>(invalid).is_err());
    }
    let mut missing = wire.clone();
    missing.as_object_mut().unwrap().remove("source");
    assert!(serde_json::from_value::<SseRoute>(missing).is_err());
    for (key, value) in [
        ("module", json!("bad/module")),
        ("route", json!("bad")),
        ("source", json!({"path":"../bad", "line":1,"column":1})),
    ] {
        let mut invalid = wire.clone();
        invalid[key] = value;
        assert!(validate_sse(&round_trip(invalid)).is_err());
    }
    let wire = json!({"module":"app.Chat","route":"/chat", "protocol":"chat.v1","source":source()});
    for (key, value) in [
        ("module", json!("bad/module")),
        ("source", Value::Null),
        ("route", json!("bad")),
        ("protocol", json!("")),
        ("protocol", json!("bad protocol")),
        ("source", json!({"path":"a", "line":0,"column":1})),
    ] {
        let mut invalid = wire.clone();
        invalid[key] = value;
        assert!(validate_websocket(&round_trip(invalid)).is_err());
    }
    for (key, value) in [
        ("module", json!("bad/module")),
        ("function", json!("Bad")),
        ("arity", json!(0)),
        ("arity", json!(2)),
    ] {
        let mut wire = json!({"module":"app.Http", "function":"recover", "arity":1});
        wire[key] = value;
        assert!(validate_error_handler(&round_trip(wire)).is_err());
    }
}

#[test]
fn static_and_file_metadata_preserve_order_and_reject_unsafe_values() {
    let mut wire = static_response();
    wire["headers"] =
        json!([{"name":"Set-Cookie","value":"a=1"},{"name":"Set-Cookie","value":"b=2"}]);
    wire["module"] = json!("app.Http");
    wire["function"] = json!("show");
    wire["arity"] = json!(1);
    wire["source"] = source();
    let response = round_trip::<StaticResponse>(wire.clone());
    validate_static_response(&response).unwrap();
    assert_eq!(response.headers[0].value, "a=1");
    assert_eq!(response.headers[1].value, "b=2");
    for (key, value) in [
        ("module", json!("")),
        ("function", json!("")),
        ("arity", json!(0)),
        ("arity", json!(2)),
        ("headers", json!([{"name":"Bad Header","value":"x"}])),
        ("headers", json!([{"name":"X-Test","value":"x\r\ny"}])),
        ("source", json!({"path":"../bad","line":1,"column":1})),
    ] {
        let mut invalid = wire.clone();
        invalid[key] = value;
        assert!(validate_static_response(&round_trip(invalid)).is_err());
    }
    for (key, value) in [
        ("method", json!("bad")),
        ("route", json!("bad")),
        ("status", json!(99)),
        ("status", json!(600)),
        ("content_type", json!("")),
        ("content_type", json!("text/plain\r\nInjected: yes")),
    ] {
        let mut text = static_response();
        text[key] = value.clone();
        assert!(validate_static_response(&round_trip(text)).is_err());
        let mut file = file_response();
        file[key] = value;
        assert!(validate_file_response(&round_trip(file)).is_err());
    }
    for path in ["", " ", "/absolute", "bad\\path", "bad\0path"] {
        let mut wire = file_response();
        wire["path"] = json!(path);
        assert!(validate_file_response(&round_trip(wire)).is_err());
    }
    let mut file = file_response();
    file["content_type"] = json!("application/octet-stream");
    file["source"] = source();
    validate_file_response(&round_trip(file.clone())).unwrap();
    file["source"]["line"] = json!(0);
    assert!(validate_file_response(&round_trip(file)).is_err());
    for status in [100, 599] {
        let mut wire = static_response();
        wire["status"] = json!(status);
        validate_static_response(&round_trip(wire)).unwrap();
    }
}
