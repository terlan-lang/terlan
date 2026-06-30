use super::pretty_print;

#[test]
fn pretty_print_preserves_backend_output_exactly() {
    let source = "-module(app_main).\n-export([main/0]).\n\nmain() ->\n    ok.\n";

    assert_eq!(pretty_print(source), source);
}

#[test]
fn pretty_print_preserves_empty_output() {
    assert_eq!(pretty_print(""), "");
}
