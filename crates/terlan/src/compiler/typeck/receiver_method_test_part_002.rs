
/// Verifies receiver-method dispatch metadata preserves mutability.
///
/// Inputs:
/// - A syntax-output module containing one mutable receiver method and one
///   immutable receiver method for the same receiver type.
///
/// Output:
/// - Test passes when the dispatch table marks only the command-style
///   mutable method as `receiver_mutable`.
///
/// Transformation:
/// - Builds receiver dispatch signatures from parsed syntax output and
///   checks the compiler-owned metadata that later rebinding lowering will
///   consume.
#[test]
fn syntax_output_receiver_dispatch_signatures_preserve_mutable_marker() {
    let module = parse_module_as_syntax_output(
        "\
module receiver_dispatch_mutability_metadata.\n\
\n\
pub struct Map {\n\
    size: Int\n\
}.\n\
\n\
pub (mut map: Map) put(): Unit ->\n\
    Unit.\n\
\n\
pub (map: Map) size(): Int ->\n\
    map.size.\n\
",
    )
    .expect("parse receiver dispatch mutability fixture");

    let mut alias_names = HashSet::new();
    alias_names.insert("Map".to_string());
    alias_names.insert("Unit".to_string());
    let signatures = collect_syntax_receiver_method_dispatch_signatures(
        &module,
        &alias_names,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );

    let put = signatures
        .get(&("put".to_string(), 0))
        .and_then(|methods| methods.first())
        .expect("mutable put dispatch signature");
    assert!(put.receiver_mutable);
    assert_eq!(pretty_type(&put.receiver_type), "Map");
    assert_eq!(pretty_type(&put.scheme.ret), "Unit");

    let size = signatures
        .get(&("size".to_string(), 0))
        .and_then(|methods| methods.first())
        .expect("immutable size dispatch signature");
    assert!(!size.receiver_mutable);
    assert_eq!(pretty_type(&size.receiver_type), "Map");
    assert_eq!(pretty_type(&size.scheme.ret), "Int");
}

/// Verifies generic set receiver calls preserve non-string element types.
///
/// Inputs:
/// - A source module importing `std.collections.Set` as a bare module.
/// - A `Set.new()` binding followed by `add(1)` and `contains(1)`
///   receiver calls.
///
/// Output:
/// - Test passes when formal syntax-output typechecking accepts `Set[Int]`
///   usage without routing `contains` through string receiver inference.
///
/// Transformation:
/// - Resolves the bare `Set` import through checked-in std summaries,
///   infers the generic `Set[T]` constructor return, then unifies `T` with
///   `Int` through receiver-method dispatch.
#[test]
fn syntax_output_accepts_std_set_int_receiver_methods() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module collection_simple.SetTest.\n\
\n\
import std.collections.Set.\n\
\n\
pub add_int(): Bool ->\n\
    let values = Set.new();\n\
    values.add(1);\n\
    values.contains(1).\n\
",
        "std/collections/Set.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies imported generic map factory results dispatch receiver methods.
///
/// Inputs:
/// - A source module importing `std.collections.List` and `std.collections.Map`.
/// - A list of tuple entries passed through `Map.from_entries`, followed by a
///   `size()` receiver-method call.
///
/// Output:
/// - Test passes when formal syntax-output typechecking accepts the map
///   receiver call.
///
/// Transformation:
/// - Exercises receiver-method dispatch after a prior generic constructor call
///   has populated the substitution table, proving imported receiver
///   candidates freshen their receiver type together with method parameters.
#[test]
fn syntax_output_accepts_std_map_from_entries_receiver_methods() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module collection_simple.MapTest.\n\
\n\
import std.collections.List.\n\
import std.collections.Map.\n\
\n\
pub map_size(): Int ->\n\
    let entries = List({\"alice\", 1});\n\
    let users = Map.from_entries(entries);\n\
    users.size().\n\
",
        "std/collections/Map.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies an imported receiver generic wins over an unrelated global type
/// with the same short name.
///
/// Inputs:
/// - `std.vm.NativeBridge`, whose receiver parameter is named `Resource`.
/// - The complete std interface graph, which also contains the concrete
///   `std.sync.Resource` type.
///
/// Output:
/// - Test passes when `NativeBridge[Binary]` accepts the generic `call`
///   receiver method.
///
/// Transformation:
/// - Exercises interface parsing and receiver dispatch with the colliding
///   `Resource` spelling, then links command and reply inference through
///   `Result.with_default`.
#[test]
fn syntax_output_accepts_imported_receiver_generic_despite_global_type_name_collision() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module native_bridge_receiver_generic.\n\
\n\
import std.vm.NativeBridge.\n\
import std.vm.NativeBridge.{NativeTransfer}.\n\
import std.core.Result.\n\
\n\
pub call_bridge(bridge: NativeBridge[String]): String ->\n\
    Result.with_default(bridge.call(\"ping\"), \"failed\").\n\
",
        "std/vm/NativeBridge.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

#[test]
fn syntax_output_rejects_duplicate_receiver_method_identity_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module receiver_dispatch_duplicate.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n\
\n\
pub (user: User) display_name(): String ->\n\
    user.name.\n\
\n\
pub (user: User) display_name(): String ->\n\
    user.name.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("duplicate receiver method `display_name` for `User` / 0")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_receiver_methods_for_imported_owner_on_formal_path() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module receiver_dispatch_imported_owner.\n\
import users.{User}.\n\
\n\
pub (user: User) display_name(): String ->\n\
    \"external\".\n\
",
        "\
module users.\n\
pub struct User {\n\
    name: String\n\
}.\n\
",
    );

    assert!(
            diagnostics.iter().any(|diag| diag.message.contains(
                "receiver method `display_name` for `User` must be declared in the defining module of `User`"
            )),
            "diagnostics: {:?}",
            diagnostics
        );
}
