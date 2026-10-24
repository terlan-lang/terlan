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

#[test]
fn primary_type_paths_keep_native_receiver_and_lambda_identity() {
    check_sources(&[
        r#"
module primary_receiver_paths.
import polars.DataFrame as Frame.
import pytorch.Tensor.
import type polars.DataFrame as Table.
identity(value: Table): polars.DataFrame.DataFrame -> value.
callback(): (polars.DataFrame) -> Int ->
    (value: polars.DataFrame) -> value.read().
pub check(): Bool ->
    let frame = Frame.new();
    let consume = callback();
    consume(identity(frame)) == 21 and Tensor.new().read() == 35.
"#,
        r#"
module polars.DataFrame.
pub struct DataFrame { value: Int }.
pub new(): DataFrame -> DataFrame(value = 21).
pub (frame: DataFrame) read(): Int -> frame.value.
"#,
        r#"
module pytorch.Tensor.
pub struct Tensor { value: Int }.
pub new(): Tensor -> Tensor(value = 7).
pub (tensor: Tensor) read(): Int -> tensor.value * 5.
"#,
    ]);
}
