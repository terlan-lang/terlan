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

#[test]
fn renamed_imports_keep_receiver_identity_inside_captured_lambdas() {
    check_sources(&[
        r#"
module captured_receiver.
import app.{Left, Right}.
import type app.Left.{Endpoint as Value, Outcome as Output}.
factory(offset: Int): (Value) -> Output ->
    (value: Value) -> value.read() + offset.
pub check(): Bool ->
    let callback = factory(4);
    callback(Left.new()) == 25 and Right.new().read() == 35.
"#,
        r#"
module app.Left.
pub struct Endpoint { value: Int }.
pub type Outcome = Int.
pub new(): Endpoint -> Endpoint(value = 21).
pub (endpoint: Endpoint) read(): Outcome -> endpoint.value.
"#,
        r#"
module app.Right.
pub struct Endpoint { value: Int }.
pub new(): Endpoint -> Endpoint(value = 7).
pub (endpoint: Endpoint) read(): Int -> endpoint.value * 5.
"#,
    ]);
}
