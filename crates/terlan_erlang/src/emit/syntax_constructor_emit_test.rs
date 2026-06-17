use std::collections::BTreeMap;

use terlan_hir::syntax_module_output_to_interface;
use terlan_syntax::parse_module_as_syntax_output;

#[test]
fn formal_syntax_output_direct_emit_rejects_structural_alias_constructor_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module structural_alias_constructor_emit.

pub type Pair =
{left: Int, right: Int}.

pub make(): Pair ->
Pair(1, 2).
"#,
    )
    .expect("parse structural alias constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "structural aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_structural_alias_constructor_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module pairs.

pub type Pair =
{left: Int, right: Int}.
"#,
    )
    .expect("parse structural alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_structural_alias_constructor_emit.

import pairs.{Pair}.

pub make(): Pair ->
Pair(1, 2).
"#,
    )
    .expect("parse imported structural alias constructor consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported structural aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_map_alias_constructor_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module props.

pub type Props =
#{name := Binary}.
"#,
    )
    .expect("parse map alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_map_alias_constructor_emit.

import props.{Props}.

pub make(name: Binary): Props ->
Props(#{name = name}).
"#,
    )
    .expect("parse imported map alias constructor consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported map aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_union_alias_constructor_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module union_alias_constructor_emit.

pub type None =
:none | :empty.

pub none(): Dynamic ->
None().
"#,
    )
    .expect("parse union alias constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "union aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_union_alias_constructor_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module options.

pub type None =
:none | :empty.
"#,
    )
    .expect("parse union alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_union_alias_constructor_emit.

import options.{None}.

pub none(): Dynamic ->
None().
"#,
    )
    .expect("parse imported union alias constructor consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported union aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_union_alias_constructor_pattern_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module union_alias_pattern_emit.

pub type None =
:none | :empty.

pub unwrap(input: Dynamic): Dynamic ->
case input {
    None -> :ok
}.
"#,
    )
    .expect("parse union alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "union alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_union_alias_constructor_pattern_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module options.

pub type None =
:none | :empty.
"#,
    )
    .expect("parse union alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_union_alias_pattern_emit.

import options.{None}.

pub unwrap(input: Dynamic): Dynamic ->
case input {
    None -> :ok
}.
"#,
    )
    .expect("parse imported union alias pattern consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported union alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_list_alias_constructor_pattern_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module list_alias_pattern_emit.

pub type Items[T] =
List[T].

pub unwrap(input: Items[Int]): Dynamic ->
case input {
    Items(values) -> values
}.
"#,
    )
    .expect("parse list alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "list alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_structural_alias_constructor_pattern_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module structural_alias_pattern_emit.

pub type Pair =
{left: Int, right: Int}.

pub left(input: Pair): Int ->
case input {
    Pair(left, _right) -> left
}.
"#,
    )
    .expect("parse structural alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "structural alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_map_alias_constructor_pattern_fallbacks() {
    let module = parse_module_as_syntax_output(
        r#"
module map_alias_pattern_emit.

pub type Props =
#{name := Binary}.

pub name(input: Props): Binary ->
case input {
    Props(values) -> values
}.
"#,
    )
    .expect("parse map alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "map alias patterns should not lower through constructor-pattern fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_list_alias_constructor_fallbacks() {
    let provider = parse_module_as_syntax_output(
        r#"
module items.

pub type Items[T] =
List[T].
"#,
    )
    .expect("parse list alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_list_alias_constructor_emit.

import items.{Items}.

pub make(values: List[Int]): Items[Int] ->
Items(values).
"#,
    )
    .expect("parse imported list alias constructor consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported list aliases should not lower through the plain-call fallback"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_nullary_literal_alias_constructor_calls() {
    let module = parse_module_as_syntax_output(
        r#"
module nullary_literal_alias_constructor_emit.

pub type None =
:none.

pub none(): Dynamic ->
None().
"#,
    )
    .expect("parse nullary literal alias constructor emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "nullary literal alias constructor calls should not lower directly"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_nullary_literal_alias_constructor_calls() {
    let provider = parse_module_as_syntax_output(
        r#"
module literals.

pub type None =
:none.
"#,
    )
    .expect("parse literal alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_nullary_literal_alias_constructor_emit.

import literals.{None}.

pub none(): Dynamic ->
None().
"#,
    )
    .expect("parse imported literal alias consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported nullary literal alias constructor calls should not lower directly"
    );
}
#[test]
fn formal_syntax_output_direct_emit_lowers_quoted_atom_type_aliases() {
    let module = parse_module_as_syntax_output(
        r#"
module quoted_atom_alias_emit.

pub type ModuleAtom =
:'Elixir.Module'.

pub classify(value: ModuleAtom): Dynamic ->
case value {
    ModuleAtom -> :ok
}.
"#,
    )
    .expect("parse quoted atom alias emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("quoted atom alias should lower directly from syntax output")
    .render();

    assert!(
        output.contains("-type module_atom() :: 'Elixir.Module'."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("'Elixir.Module' -> ok"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_literal_alias_constructor_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module alias_pattern_emit.

pub type Ok[T] =
{:ok, value: T}.

pub type None =
:none.

pub unwrap(input: Dynamic): Dynamic ->
case input {
    None -> :none
}.
"#,
    )
    .expect("parse alias pattern emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower directly from syntax output")
    .render();
    assert!(output.contains("-type ok(T) :: {ok, T}."));
    assert!(output.contains("-type none() :: 'none'."));
    assert!(output.contains("'none' -> 'none'"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_literal_alias_values() {
    let module = parse_module_as_syntax_output(
        r#"
module alias_value_emit.

pub type None =
:none.

pub none(): None ->
None.
"#,
    )
    .expect("parse alias value emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("literal alias values should lower directly from syntax output")
    .render();

    assert!(output.contains("-type none() :: 'none'."));
    assert!(
        output.contains("none() ->\n    'none'."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_imported_literal_alias_constructor_patterns() {
    let provider = parse_module_as_syntax_output(
        r#"
module literals.

pub type None =
:none.
"#,
    )
    .expect("parse literal alias provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module imported_alias_pattern_emit.

import literals.{None}.

pub unwrap(input: None): Dynamic ->
case input {
    None -> :ok
}.
"#,
    )
    .expect("parse imported literal alias pattern consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("imported literal alias patterns should lower directly from syntax output")
    .render();

    assert!(output.contains("'none' -> ok"), "output:\n{}", output);
}
