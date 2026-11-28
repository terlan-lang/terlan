//! Callable and conformance templates used during monomorphization.

use super::*;

/// Every callable candidate grouped by its qualified name and arity.
#[derive(Default)]
pub(in crate::compiler::native_ir) struct CallableTemplates {
    pub(in crate::compiler::native_ir) functions: BTreeMap<(String, usize), Vec<CoreFunction>>,
    receivers: super::super::application::mutable_receivers::ReceiverTargets,
    conformances: HashMap<String, Vec<nominal_parameters::GenericConformance>>,
    nominal_types: HashMap<String, NominalTypeTemplate>,
}

#[derive(Clone)]
struct NominalTypeTemplate {
    params: Vec<String>,
    body: CoreType,
}

impl CallableTemplates {
    /// Starts a callable catalog with the application's shared receiver index.
    pub(in crate::compiler::native_ir) fn new(cores: &[CoreModule]) -> Self {
        Self {
            functions: BTreeMap::new(),
            receivers: super::super::application::mutable_receivers::receiver_targets(cores),
            conformances: collect_conformances(cores),
            nominal_types: nominal_type_templates(cores),
        }
    }

    /// Projects one checked field through a nominal generic application.
    pub(super) fn field_type(&self, ty: &CoreType, field: &str, module: &str) -> Option<CoreType> {
        self.field_type_inner(ty, field, module, &mut HashSet::new())
    }

    fn field_type_inner(
        &self,
        ty: &CoreType,
        field: &str,
        module: &str,
        active: &mut HashSet<String>,
    ) -> Option<CoreType> {
        if let Some(field_type) = structural_field_type(ty, field) {
            return Some(field_type.clone());
        }
        let (constructor, args) = match ty {
            CoreType::Named(name) => (name.as_str(), &[][..]),
            CoreType::Apply { constructor, args } => (constructor.as_str(), args.as_slice()),
            _ => return None,
        };
        let qualified = (!constructor.contains('.')).then(|| format!("{module}.{constructor}"));
        let identity = qualified.as_deref().unwrap_or(constructor);
        let declaration = self
            .nominal_types
            .get(identity)
            .or_else(|| self.nominal_types.get(constructor))?;
        if declaration.params.len() != args.len() || !active.insert(identity.to_string()) {
            return None;
        }
        let values = declaration
            .params
            .iter()
            .cloned()
            .zip(args.iter().cloned())
            .collect();
        let body = substitute(&declaration.body, &declaration.params, &values);
        let projected = self.field_type_inner(&body, field, module, active);
        active.remove(identity);
        projected
    }

    pub(super) fn receiver_callable(
        &self,
        method: &str,
        arity: usize,
        receiver: &CoreType,
        caller: &str,
    ) -> Option<String> {
        super::super::application::mutable_receivers::receiver_callable(
            &self.receivers,
            method,
            arity,
            receiver,
            caller,
        )
    }

    pub(super) fn refine_generic_bounds(
        &self,
        template: &CoreFunction,
        substitution: &mut HashMap<String, CoreType>,
    ) {
        for bound in &template.generic_bounds {
            let CoreType::Apply { constructor, .. } = bound else {
                continue;
            };
            let Some(candidates) = self
                .conformances
                .get(constructor)
                .or_else(|| self.conformances.get(constructor.rsplit('.').next()?))
            else {
                continue;
            };
            let required = substitute(bound, &template.generic_params, substitution);
            let unresolved = template
                .generic_params
                .iter()
                .filter(|parameter| !substitution.contains_key(*parameter))
                .cloned()
                .map(|parameter| (parameter, CoreType::Never))
                .collect::<HashMap<_, _>>();
            let candidate_required = substitute(&required, &template.generic_params, &unresolved);
            let mut refinements = Vec::new();
            for candidate in candidates {
                let instantiated = if candidate.generic_params.is_empty() {
                    candidate.instance.clone()
                } else {
                    let mut candidate_values = HashMap::new();
                    if unify(
                        &candidate.instance,
                        &candidate_required,
                        &candidate.generic_params,
                        &mut candidate_values,
                    )
                    .is_err()
                        || candidate.generic_params.iter().any(|parameter| {
                            candidate_values
                                .get(parameter)
                                .is_none_or(|value| value == &CoreType::Never)
                        })
                    {
                        continue;
                    }
                    substitute(
                        &candidate.instance,
                        &candidate.generic_params,
                        &candidate_values,
                    )
                };
                let mut trial = substitution.clone();
                if unify(
                    &required,
                    &instantiated,
                    &template.generic_params,
                    &mut trial,
                )
                .is_ok()
                    && !refinements.iter().any(|existing| existing == &trial)
                {
                    refinements.push(trial);
                }
            }
            if let [refinement] = refinements.as_slice() {
                *substitution = refinement.clone();
            }
        }
    }
}

fn nominal_type_templates(cores: &[CoreModule]) -> HashMap<String, NominalTypeTemplate> {
    let mut templates = HashMap::new();
    for core in cores {
        for declaration in &core.types {
            let Some(body) = &declaration.core_body else {
                continue;
            };
            let template = NominalTypeTemplate {
                params: declaration.params.clone(),
                body: body.clone(),
            };
            templates.insert(
                format!("{}.{}", core.module, declaration.name),
                template.clone(),
            );
            templates.insert(
                crate::terlan_hir::public_type_path(&core.module, &declaration.name),
                template,
            );
        }
    }
    templates
}

fn structural_field_type<'a>(ty: &'a CoreType, name: &str) -> Option<&'a CoreType> {
    match ty {
        CoreType::Tuple(elements) => elements.iter().find_map(|element| match element {
            crate::terlan_typeck::CoreTupleTypeElem::Field { name: field, ty } if field == name => {
                Some(ty)
            }
            _ => None,
        }),
        CoreType::Struct { fields, .. } => fields
            .iter()
            .find(|candidate| candidate.name == name)
            .map(|candidate| &candidate.ty),
        CoreType::Map(fields) => fields
            .iter()
            .find(|candidate| candidate.key == name)
            .map(|candidate| &candidate.value),
        _ => None,
    }
}

#[cfg(test)]
#[path = "templates_test.rs"]
mod tests;
