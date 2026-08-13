use super::*;

/// Reconstructs generic struct schemes from selected imported type metadata.
pub(in super::super) fn collect_imported_struct_schemes(
    resolved: &ResolvedModule,
    alias_names: &HashSet<String>,
) -> HashMap<String, StructScheme> {
    let mut out = HashMap::new();

    for (local_name, imported) in &resolved.imported_types {
        let Some(interface) = resolved.interface_map.get(&imported.source_module) else {
            continue;
        };
        let Some(generic_params) = interface.type_params.get(&imported.source_name) else {
            continue;
        };
        let Some(fields) = interface.struct_fields.get(&imported.source_name) else {
            continue;
        };
        if generic_params.is_empty() {
            continue;
        }

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        let mut params = Vec::with_capacity(generic_params.len());
        for param in generic_params {
            vars.insert(normalize_type_param_name(param), next_var);
            params.push(next_var);
            next_var += 1;
        }
        let fields = fields
            .iter()
            .map(|field| {
                let ty = parse_type_expr(&field.annotation, alias_names, &mut vars, &mut next_var)
                    .unwrap_or(Type::Dynamic);
                (field.name.clone(), ty)
            })
            .collect();
        let bounds = parse_structural_implication_bounds(generic_params, &vars, alias_names);

        out.insert(
            local_name.clone(),
            StructScheme {
                generic_params: generic_params.clone(),
                params,
                fields,
                bounds,
            },
        );
    }

    out
}

/// Collects generic struct fields and implication bounds as one reusable scheme.
pub(in super::super) fn collect_syntax_struct_schemes(
    module: &SyntaxModuleOutput,
    environment: TypeResolutionEnvironment<'_>,
) -> HashMap<String, StructScheme> {
    let mut out = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Struct {
            name,
            generic_params,
            fields,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let mut vars = HashMap::new();
        let mut next_var: TypeVarId = 0;
        let mut params = Vec::new();
        for param in generic_params {
            vars.insert(normalize_type_param_name(param), next_var);
            params.push(next_var);
            next_var += 1;
        }
        let fields = fields
            .iter()
            .map(|field| {
                let ty = parse_type_expr(
                    &field.annotation.text,
                    environment.alias_names,
                    &mut vars,
                    &mut next_var,
                )
                .unwrap_or(Type::Dynamic);
                let ty = expand_imported_aliases_except_named(
                    &ty,
                    environment.imported_type_aliases,
                    environment.imported_type_names,
                    environment.local_aliases,
                );
                let ty = qualify_type_names(&ty, environment.imported_type_names);
                (field.name.clone(), ty)
            })
            .collect();
        let bounds =
            parse_structural_implication_bounds(generic_params, &vars, environment.alias_names);

        out.insert(
            name.clone(),
            StructScheme {
                generic_params: generic_params.clone(),
                params,
                fields,
                bounds,
            },
        );
    }

    out
}
