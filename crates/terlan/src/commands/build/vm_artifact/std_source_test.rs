//! Adjacent behavioral tests; module identity and assertions are preserved.

use super::*;

#[test]
fn type_only_error_provider_retains_its_canonical_layout() {
    let state = CliState::default();
    let root = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "task_type_closure.terl",
        "module task_type_closure. import std.core.Task. \
         import type std.core.{Error, Result, Task}. \
         pub observe(task: Task[Int]): Result[Int, Error] -> task.result().",
        state.diagnostic_format,
        None,
        state.native_policy,
        crate::validation::target_profile::TargetProfile::Vm,
    )
    .expect("compile typed Task observer")
    .core;
    let modules =
        compile_imported_std_source_modules(&[&root], Path::new("task_type_closure.terl"), &state)
            .expect("load value and type dependency closure");
    let error = modules
        .iter()
        .find(|module| module.compiled.core.module == "std.core.Error")
        .expect("type-only Error provider is required for managed identity");
    assert!(error.compiled.core.types.iter().any(|ty| {
        ty.name == "Error"
            && matches!(&ty.core_body,
            Some(crate::terlan_typeck::CoreType::Struct { fields, .. }) if fields.len() == 2)
    }));
    let cores = std::iter::once(&root)
        .chain(modules.iter().map(|module| &module.compiled.core))
        .collect::<Vec<_>>();
    crate::compiler::native_ir::NativeModule::lower_application(&cores)
        .expect("admit the declared Task Result with its canonical Error schema");
}

#[test]
fn qualified_option_payload_keeps_buffer_receivers_in_source_closure() {
    let state = CliState::default();
    let root = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "qualified_option_bytes.terl",
        r#"module qualified_option_bytes.
import std.core.Option.{Some, None}.
import std.vm.Bytes.
import std.vm.BitString.
make(): std.core.Option.Option[std.vm.Bytes.Bytes] -> Some(Bytes.from_list([1, 2])).
bits(): std.core.Option.Option[std.vm.BitString.BitString] -> Some(BitString.from_int_be(3, 5)).
pub check(): Bool -> (case make() { Some(value) -> value.length() == 2; None -> false })
and (case bits() { Some(value) -> value.bit_length() == 5; None -> false }).
"#,
        state.diagnostic_format,
        None,
        state.native_policy,
        crate::validation::target_profile::TargetProfile::Vm,
    )
    .expect("compile qualified Option consumer")
    .core;
    let modules = compile_imported_std_source_modules(
        &[&root],
        Path::new("qualified_option_bytes.terl"),
        &state,
    )
    .expect("load production source closure");
    let mut cores = vec![root];
    cores.extend(modules.into_iter().map(|module| module.compiled.core));
    crate::compiler::native_ir::prune_application_to_function_roots(
        &mut cores,
        &[("qualified_option_bytes".into(), "check".into(), 0)],
    )
    .expect("retain check closure");
    crate::compiler::native_ir::NativeModule::lower_application(&cores.iter().collect::<Vec<_>>())
        .expect("lower qualified managed-buffer receivers through the production closure");
}

#[test]
fn intrinsic_only_std_modules_keep_their_type_declarations() {
    let state = CliState::default();
    let root = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "alias_closure.terl",
        "module alias_closure. import std.core.Object. pub value(): Int -> 7.",
        state.diagnostic_format,
        None,
        state.native_policy,
        crate::validation::target_profile::TargetProfile::Vm,
    )
    .expect("compile alias consumer")
    .core;
    let modules =
        compile_imported_std_source_modules(&[&root], Path::new("alias_closure.terl"), &state)
            .expect("load std implementation closure");
    let object = &modules
        .iter()
        .find(|module| module.compiled.core.module == "std.core.Object")
        .expect("intrinsic-only provider remains part of the typed closure")
        .compiled
        .core;
    assert!(
        !object.functions.is_empty()
            && object
                .functions
                .iter()
                .all(|function| object.constructors.iter().any(|constructor| {
                    constructor
                        .implementation
                        .as_ref()
                        .is_some_and(|implementation| implementation.function == function.name)
                })),
        "source constructor bodies must survive while intrinsic placeholders remain filtered"
    );
    assert!(object
        .types
        .iter()
        .any(|ty| ty.name == "Object" && ty.core_body.is_some()));
    assert!(object
        .constructors
        .iter()
        .any(|constructor| constructor.name == "Object"));
}
