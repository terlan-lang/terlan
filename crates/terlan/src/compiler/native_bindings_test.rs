use super::*;

#[test]
fn native_declaration_keys_use_module_casing_and_preserve_aliases() {
    for (module, expected) in [
        ("std.regex.Regex", "std.regex.regex.read"),
        ("std.vm.DistributedState", "std.vm.distributed_state.read"),
        ("std.http.HTTPServer", "std.http.http_server.read"),
    ] {
        assert_eq!(inferred_operation(module, "read"), expected);
    }
    let module = crate::terlan_syntax::parse_module_as_syntax_output(
        "module std.native.collections.Vector. pub opaque type Vector. @compiler.native {std.native.collections.vector.length} pub (vector: Vector) len(): Int.",
    ).unwrap();
    assert_eq!(
        declaration_operation(&module.module_name, &module.declarations[1]).as_deref(),
        Some("std.native.collections.vector.length")
    );
}
