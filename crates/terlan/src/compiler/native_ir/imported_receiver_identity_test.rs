use super::source_constructor_test::check_sources;

#[test]
fn selected_nominal_imports_survive_unrelated_receiver_dependencies() {
    for order in [false, true] {
        let left = r#"
module app.Left.
pub struct Endpoint { value: Int }.
pub new(): Endpoint -> Endpoint(value = 21).
pub (endpoint: Endpoint) read(): Int -> endpoint.value.
"#;
        let right = r#"
module app.Right.
pub struct Endpoint { value: Int }.
pub new(): Endpoint -> Endpoint(value = 7).
pub (endpoint: Endpoint) read(): Int -> endpoint.value * 5.
"#;
        let providers = if order { [left, right] } else { [right, left] };
        check_sources(&[
            r#"
module selected_receivers.
import app.{Left, Right}.
import type app.Left.{Endpoint}.
import type app.Right.{Endpoint as Other}.
left(): Endpoint -> Left.new().
right(): Other -> Right.new().
pub check(): Bool -> left().read() == 21 and right().read() == 35.
"#,
            providers[0],
            providers[1],
        ]);
    }
}
