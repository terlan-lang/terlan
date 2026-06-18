use std::collections::BTreeMap;

use terlan_syntax::parse_module_as_syntax_output;

#[test]
fn formal_syntax_output_direct_emit_lowers_record_constructs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_record_construct_emit.

pub make(id: Int, name: Text): Dynamic ->
#User{id = id, name = name}.
"#,
    )
    .expect("parse syntax output record construct fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("record construct should lower directly from syntax output")
    .render();

    assert!(
        output.contains("make(Id, Name) ->\n    #user{id = Id, name = Name}."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_structs_with_defaults() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_emit.

pub struct User {
id: Int,
name: Text,
status: Dynamic = :active
}.

pub make(id: Int, name: Text): User ->
#User{id = id, name = name}.
"#,
    )
    .expect("parse syntax output struct fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("struct subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-export_type([user/0])."));
    assert!(output.contains("-type user() :: #user{}."));
    assert!(
        output.contains("-record(user, {id, name, status = 'active'})."),
        "output:\n{}",
        output
    );
    assert!(output.contains("make(Id, Name) ->\n    #user{id = Id, name = Name}."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_struct_field_access_from_param_type() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_field_emit.

pub struct User {
id: Int,
name: Text
}.

pub username(user: User): Text ->
user.name.
"#,
    )
    .expect("parse syntax output struct field fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("struct field access should lower directly from syntax output")
    .render();

    assert!(output.contains("username(User) ->\n    User#user.name."));
    assert!(
        output.find("-record(user").unwrap_or(usize::MAX)
            < output.find("-type user").unwrap_or(usize::MAX),
        "record declarations must appear before types that reference them:\n{}",
        output
    );
    assert!(!output.contains("User#name.name"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_record_updates() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_record_update_emit.

pub struct User {
id: Int,
name: Text
}.

pub rename(user: User, name: Text): User ->
user#User{name = name}.
"#,
    )
    .expect("parse syntax output record update fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("record update should lower directly from syntax output")
    .render();

    assert!(
        output.contains("rename(User, Name) ->\n    User#user{name = Name}."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_record_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_record_pattern_emit.

pub struct User {
id: Int,
name: Text
}.

pub username(user: User): Text ->
case user {
    #User{name = name} -> name
}.
"#,
    )
    .expect("parse syntax output record pattern fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("record pattern should lower directly from syntax output")
    .render();

    assert!(
        output.contains("#user{name = Name} -> Name"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_struct_headers() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_header_emit.

/// A user account.
pub struct User {
id: Int,
name: Text = <<"guest">>
}.
"#,
    )
    .expect("parse syntax output struct header fixture");

    let output = super::lower_syntax_struct_headers_to_hrl(&module)
        .expect("struct headers should lower directly from syntax output");

    assert!(output.contains("-record(user, {id, name = <<\"guest\">>})."));
}
