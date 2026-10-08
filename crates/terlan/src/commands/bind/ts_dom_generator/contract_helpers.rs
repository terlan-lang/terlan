//! Renders API-shape test helpers for generated DOM bindings.

use super::*;

/// Renders generated Terlan test text for one DOM module.
///
/// Inputs:
/// - `module`: planned DOM module.
/// - `manifest`: validated input manifest.
/// - `manifest_path`: user-supplied manifest path used for provenance.
///
/// Output:
/// - Generated `.terl` test source text.
///
/// Transformation:
/// - Emits deterministic executable API-shape contract tests once the
///   generated `std.js` support types are available. These prove binding
///   surface availability without claiming browser behavior coverage.
pub(super) fn render_module_test(
    module: &DomModulePlan,
    manifest: &TsInputManifest,
    manifest_path: &Path,
) -> String {
    let mut output = render_module_header(module, manifest, manifest_path, "test");
    output.push_str(&format!("module {}Test.\n\n", module.module_path));
    let type_name = render_type_name(&module.type_name, &[]);
    if module.module_path.rsplit('.').next() == Some(type_name.as_str()) {
        output.push_str(&format!("import type {}.\n\n", module.module_path));
    } else {
        output.push_str(&format!(
            "import type {}.{{{type_name}}}.\n\n",
            module.module_path,
        ));
    }
    output.push_str("pub generated_binding_surface_contract(): Bool ->\n    true.\n");
    for member in &module.members {
        output.push('\n');
        output.push_str(&render_member_test(module, member));
    }
    output
}

/// Renders one generated test helper for a DOM member.
///
/// Inputs:
/// - `module`: containing DOM module plan.
/// - `member`: planned DOM member.
///
/// Output:
/// - Terlan function that typechecks one generated receiver API shape.
///
/// Transformation:
/// - Converts properties and methods into parameterized helper functions so
///   generated tests cover signatures without constructing DOM runtime values.
fn render_member_test(module: &DomModulePlan, member: &DomMemberPlan) -> String {
    match member {
        DomMemberPlan::Property(property) => render_property_test(module, property),
        DomMemberPlan::Method(method) => render_method_test(module, method),
    }
}

/// Renders a generated property-shape test helper.
///
/// Inputs:
/// - `module`: containing DOM module plan.
/// - `property`: planned DOM property.
///
/// Output:
/// - Parameterized Terlan function returning the property getter type.
///
/// Transformation:
/// - Calls the generated receiver getter so source review can see the exact
///   property method and mapped return type.
fn render_property_test(module: &DomModulePlan, property: &DomPropertyPlan) -> String {
    format!(
        "pub {}_typechecks(receiver: {}): {} ->\n    receiver.{}().\n",
        property.terlan_name,
        render_type_reference_name(module),
        property.terlan_type,
        property.terlan_name
    )
}

/// Renders a generated method-shape test helper.
///
/// Inputs:
/// - `module`: containing DOM module plan.
/// - `method`: planned DOM method.
///
/// Output:
/// - Parameterized Terlan function returning the method call type.
///
/// Transformation:
/// - Reuses generated parameter names where possible while reserving the
///   receiver helper name so source JavaScript parameters named `value` cannot
///   shadow the method-call receiver in generated tests.
fn render_method_test(module: &DomModulePlan, method: &DomMethodPlan) -> String {
    let receiver_param = format!("receiver: {}", render_type_reference_name(module));
    let mut params = vec![receiver_param];
    let argument_names = collision_free_test_argument_names(&method.params);
    params.extend(
        method
            .params
            .iter()
            .zip(argument_names.iter())
            .map(|(param, name)| format!("{}: {}", name, param.terlan_type)),
    );
    format!(
        "pub {}_typechecks({}): {} ->\n    receiver.{}({}).\n",
        method.terlan_name,
        params.join(", "),
        method.return_type,
        method.terlan_name,
        argument_names.join(", ")
    )
}

/// Builds collision-free generated test argument names.
///
/// Inputs:
/// - `params`: planned DOM method parameters.
///
/// Output:
/// - Parameter names safe to use beside the generated `receiver` binding.
///
/// Transformation:
/// - Preserves each generated parameter name unless it collides with the
///   receiver binding or an earlier parameter, appending a stable numeric
///   suffix for collisions.
fn collision_free_test_argument_names(params: &[DomParamPlan]) -> Vec<String> {
    let mut used = vec!["receiver".to_string()];
    params
        .iter()
        .map(|param| unique_test_argument_name(&param.terlan_name, &mut used))
        .collect()
}

/// Selects one generated helper argument name.
///
/// Inputs:
/// - `base`: preferred generated parameter name.
/// - `used`: names already reserved in the helper declaration.
///
/// Output:
/// - Unique helper parameter name.
///
/// Transformation:
/// - Returns `base` when unused, otherwise appends `_2`, `_3`, and so on until
///   the name is unique, then records that name in `used`.
fn unique_test_argument_name(base: &str, used: &mut Vec<String>) -> String {
    if !used.iter().any(|name| name == base) {
        used.push(base.to_string());
        return base.to_string();
    }

    for suffix in 2.. {
        let candidate = format!("{base}_{suffix}");
        if !used.iter().any(|name| name == &candidate) {
            used.push(candidate.clone());
            return candidate;
        }
    }

    unreachable!("unbounded suffix search should always find a unique name")
}
