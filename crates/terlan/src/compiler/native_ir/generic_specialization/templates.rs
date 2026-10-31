//! Callable and conformance templates used during monomorphization.

use super::*;

/// Every callable candidate grouped by its qualified name and arity.
#[derive(Default)]
pub(in crate::compiler::native_ir) struct CallableTemplates {
    pub(in crate::compiler::native_ir) functions: BTreeMap<(String, usize), Vec<CoreFunction>>,
    receivers: super::super::application::mutable_receivers::ReceiverTargets,
    conformances: HashMap<String, Vec<nominal_parameters::GenericConformance>>,
}

impl CallableTemplates {
    /// Starts a callable catalog with the application's shared receiver index.
    pub(in crate::compiler::native_ir) fn new(cores: &[CoreModule]) -> Self {
        Self {
            functions: BTreeMap::new(),
            receivers: super::super::application::mutable_receivers::receiver_targets(cores),
            conformances: collect_conformances(cores),
        }
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
            let mut refinements = Vec::new();
            for candidate in candidates {
                let instantiated = if candidate.generic_params.is_empty() {
                    candidate.instance.clone()
                } else {
                    let mut candidate_values = HashMap::new();
                    if unify(
                        &candidate.instance,
                        &required,
                        &candidate.generic_params,
                        &mut candidate_values,
                    )
                    .is_err()
                        || candidate
                            .generic_params
                            .iter()
                            .any(|parameter| !candidate_values.contains_key(parameter))
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
