use super::{add_route_parameters, item_route_parameter};

#[test]
fn baseline_routes_parse_dynamic_parameters() {
    assert_eq!(add_route_parameters("/api/add/20/22"), Some((20, 22)));
    assert_eq!(add_route_parameters("/api/add/nope/22"), None);
    assert_eq!(item_route_parameter("/api/items/7"), Some("7"));
    assert_eq!(item_route_parameter("/api/items/7/nested"), None);
}
