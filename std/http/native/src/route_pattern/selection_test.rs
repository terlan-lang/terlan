use super::*;

#[derive(Debug, PartialEq, Eq)]
struct Route {
    method: &'static str,
    pattern: &'static str,
}

fn select<'a>(routes: &'a [Route], method: &str, path: &str) -> Option<SelectedRoute<'a, Route>> {
    select_route(routes, method, path, |route| (route.method, route.pattern))
}

#[test]
fn precedence_is_independent_of_declaration_order_and_borrows_the_selected_route() {
    let patterns = ["*", "/api/*", "/api/:id", "/api/status", "/api/nested/*"];
    for reverse in [false, true] {
        let mut routes: Vec<_> = patterns
            .iter()
            .map(|pattern| Route {
                method: "GET",
                pattern,
            })
            .collect();
        if reverse {
            routes.reverse();
        }
        for (path, expected) in [
            ("/api/status", "/api/status"),
            ("/api/user", "/api/:id"),
            ("/api/deep/path", "/api/*"),
            ("/api/nested/file", "/api/nested/*"),
            ("/different", "*"),
        ] {
            let selected = select(&routes, "GET", path).unwrap();
            assert_eq!(selected.route.pattern, expected);
            assert!(routes
                .iter()
                .any(|route| std::ptr::eq(route, selected.route)));
        }
    }
}

#[test]
fn head_prefers_any_explicit_match_before_get_fallback() {
    let routes = [
        Route {
            method: "GET",
            pattern: "/api/exact",
        },
        Route {
            method: "HEAD",
            pattern: "/api/*",
        },
        Route {
            method: "GET",
            pattern: "/other/:id",
        },
        Route {
            method: "POST",
            pattern: "/post",
        },
    ];
    assert_eq!(
        select(&routes, "HEAD", "/api/exact").unwrap().route,
        &routes[1]
    );
    let selected = select(&routes, "HEAD", "/other/%34%32").unwrap();
    assert_eq!(selected.route, &routes[2]);
    assert_eq!(selected.params, [("id".into(), "42".into())]);
    assert_eq!(
        select(&routes, "GET", "/api/exact").unwrap().route,
        &routes[0]
    );
    assert_eq!(select(&routes, "POST", "/post").unwrap().route, &routes[3]);
    for (method, path) in [
        ("HEAD", "/missing"),
        ("HEAD", "/post"),
        ("POST", "/api/exact"),
        ("get", "/api/exact"),
        ("OPTIONS", "/other/42"),
    ] {
        assert!(select(&routes, method, path).is_none());
    }
    assert!(select(&[], "GET", "/").is_none());
}

#[test]
fn equal_scores_keep_the_last_candidate_without_reordering_captures() {
    let routes = [
        Route {
            method: "GET",
            pattern: "/api/:first/:second",
        },
        Route {
            method: "GET",
            pattern: "/api/:left/:right",
        },
    ];
    let selected = select(&routes, "GET", "/api/a%252Fb/%E2%9C%93").unwrap();
    assert!(std::ptr::eq(selected.route, &routes[1]));
    assert_eq!(
        selected.params,
        [
            ("left".into(), "a%2Fb".into()),
            ("right".into(), "\u{2713}".into())
        ]
    );
}

#[test]
fn typed_route_rejection_falls_back_without_reinterpreting_invalid_values() {
    let routes = [
        Route {
            method: "GET",
            pattern: "/api/{id:Int}/{enabled:Bool}",
        },
        Route {
            method: "GET",
            pattern: "*",
        },
    ];
    let selected = select(&routes, "GET", "/api/-42/false").unwrap();
    assert_eq!(selected.route, &routes[0]);
    let values: Vec<_> = selected
        .params
        .iter()
        .map(|(name, value)| route_param_argument(selected.route.pattern, name, value).unwrap())
        .collect();
    assert_eq!(values, [NativeValue::Int(-42), NativeValue::Bool(false)]);
    for path in [
        "/api/9223372036854775808/true",
        "/api/42/TRUE",
        "/api/1.5/false",
    ] {
        assert_eq!(select(&routes, "GET", path).unwrap().route, &routes[1]);
    }
}

#[test]
fn materialization_preserves_strings_and_scalar_boundaries() {
    for (pattern, name, value) in [
        ("/api/:id", "id", "001"),
        ("/api/{id:String}", "id", "false"),
        ("/api/*", "*", "a/b"),
        ("/api/{other:Int}/:id", "id", "42"),
    ] {
        assert_eq!(
            route_param_argument(pattern, name, value).unwrap(),
            NativeValue::String(value.into())
        );
    }
    for (value, expected) in [
        ("-9223372036854775808", i64::MIN),
        ("9223372036854775807", i64::MAX),
        ("+42", 42),
        ("0", 0),
    ] {
        assert_eq!(
            route_param_argument("/{id:Int}", "id", value).unwrap(),
            NativeValue::Int(expected)
        );
    }
    for (value, expected) in [("true", true), ("false", false)] {
        assert_eq!(
            route_param_argument("/{value:Bool}", "value", value).unwrap(),
            NativeValue::Bool(expected)
        );
    }
}

#[test]
fn invalid_scalar_values_return_stable_boundary_errors() {
    for value in [
        "",
        "no",
        "1.5",
        " 1",
        "9223372036854775808",
        "-9223372036854775809",
    ] {
        let error = route_param_argument("/{id:Int}", "id", value).unwrap_err();
        assert!(
            error.to_string().contains(&format!(
                "typed route capture `id:Int` could not materialize `{value}`"
            )),
            "{error}"
        );
    }
    for value in ["", "TRUE", "False", "0", " true"] {
        let error = route_param_argument("/{flag:Bool}", "flag", value).unwrap_err();
        assert!(
            error.to_string().contains(&format!(
                "typed route capture `flag:Bool` could not materialize `{value}`"
            )),
            "{error}"
        );
    }
    assert!(route_param_argument("/{id:Float}", "id", "1")
        .unwrap_err()
        .to_string()
        .contains("unsupported typed route capture `id:Float`"));
}
