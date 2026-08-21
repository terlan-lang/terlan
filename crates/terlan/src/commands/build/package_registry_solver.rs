//! Deterministic whole-graph solving for trusted Terlan Registry metadata.

use std::collections::{BTreeMap, BTreeSet};

use semver::Version;

use super::package_registry_error::RegistryResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GraphRequirement {
    pub(super) package: String,
    pub(super) requirement: String,
    pub(super) requested_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GraphDependency {
    pub(super) package: String,
    pub(super) requirement: String,
    pub(super) optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GraphCandidate {
    pub(super) package: String,
    pub(super) version: String,
    pub(super) yanked: bool,
    pub(super) dependencies: Vec<GraphDependency>,
}

#[derive(Debug, Clone, Default)]
struct SolverState {
    constraints: BTreeMap<String, Vec<GraphRequirement>>,
    selected: BTreeMap<String, GraphCandidate>,
}

pub(super) fn solve_graph<F>(
    roots: Vec<GraphRequirement>,
    locked: &BTreeMap<String, String>,
    updates: &BTreeSet<String>,
    allow_yanked: bool,
    mut load: F,
) -> RegistryResult<BTreeMap<String, GraphCandidate>>
where
    F: FnMut(&str) -> RegistryResult<Vec<GraphCandidate>>,
{
    let mut state = SolverState::default();
    for requirement in roots {
        add_constraint(&mut state, requirement)?;
    }
    solve_state(state, locked, updates, allow_yanked, &mut load).map(|state| state.selected)
}

fn solve_state<F>(
    state: SolverState,
    locked: &BTreeMap<String, String>,
    updates: &BTreeSet<String>,
    allow_yanked: bool,
    load: &mut F,
) -> RegistryResult<SolverState>
where
    F: FnMut(&str) -> RegistryResult<Vec<GraphCandidate>>,
{
    let Some(package) = state
        .constraints
        .keys()
        .find(|package| !state.selected.contains_key(*package))
        .cloned()
    else {
        return Ok(state);
    };
    let constraints = state.constraints.get(&package).cloned().unwrap_or_default();
    let locked_version = locked
        .get(&package)
        .filter(|_| !updates.contains(&package))
        .map(String::as_str);
    let all_candidates = load(&package)?;
    let mut candidates = all_candidates.clone();
    candidates.retain(|candidate| {
        candidate.package == package
            && (allow_yanked
                || !candidate.yanked
                || locked_version == Some(candidate.version.as_str()))
            && constraints.iter().all(|constraint| {
                crate::package_registry::requirement_matches(
                    &constraint.requirement,
                    &candidate.version,
                )
                .unwrap_or(false)
            })
    });
    candidates.sort_by(|left, right| candidate_order(left, right, locked_version));
    if candidates.is_empty() {
        return Err(conflict(&package, &constraints, all_candidates).into());
    }

    let mut failures = Vec::new();
    for candidate in candidates {
        let mut branch = state.clone();
        branch.selected.insert(package.clone(), candidate.clone());
        let active_dependencies = candidate
            .dependencies
            .iter()
            .filter(|dependency| {
                !dependency.optional || branch.constraints.contains_key(&dependency.package)
            })
            .cloned()
            .collect::<Vec<_>>();
        let result = active_dependencies
            .iter()
            .try_for_each(|dependency| {
                add_constraint(
                    &mut branch,
                    GraphRequirement {
                        package: dependency.package.clone(),
                        requirement: dependency.requirement.clone(),
                        requested_by: format!("{}@{}", candidate.package, candidate.version),
                    },
                )
            })
            .and_then(|_| activate_optional_constraints(&mut branch, &package));
        if let Err(error) = result {
            failures.push(error);
            continue;
        }
        match solve_state(branch, locked, updates, allow_yanked, load) {
            Ok(solved) => return Ok(solved),
            Err(error) => failures.push(error),
        }
    }
    failures.sort();
    failures.dedup();
    Err(failures
        .into_iter()
        .next()
        .unwrap_or_else(|| conflict(&package, &constraints, Vec::new()).into()))
}

fn add_constraint(state: &mut SolverState, requirement: GraphRequirement) -> RegistryResult<()> {
    crate::package_registry::parse_requirement(&requirement.requirement)
        .map_err(|error| error.to_string())?;
    let package = requirement.package.clone();
    let requirements = state.constraints.entry(package.clone()).or_default();
    if !requirements.contains(&requirement) {
        requirements.push(requirement);
        requirements.sort_by(|left, right| {
            (&left.requested_by, &left.requirement).cmp(&(&right.requested_by, &right.requirement))
        });
    }
    if let Some(selected) = state.selected.get(&package) {
        if !matches_all(requirements, &selected.version) {
            return Err(conflict(&package, requirements, vec![selected.clone()]).into());
        }
    }
    activate_optional_constraints(state, &package)
}

fn activate_optional_constraints(state: &mut SolverState, package: &str) -> RegistryResult<()> {
    let optional = state
        .selected
        .values()
        .flat_map(|candidate| {
            candidate
                .dependencies
                .iter()
                .filter(move |dependency| dependency.optional && dependency.package == package)
                .map(move |dependency| GraphRequirement {
                    package: dependency.package.clone(),
                    requirement: dependency.requirement.clone(),
                    requested_by: format!("{}@{}", candidate.package, candidate.version),
                })
        })
        .collect::<Vec<_>>();
    for requirement in optional {
        let requirements = state
            .constraints
            .entry(requirement.package.clone())
            .or_default();
        if !requirements.contains(&requirement) {
            requirements.push(requirement);
            requirements.sort_by(|left, right| {
                (&left.requested_by, &left.requirement)
                    .cmp(&(&right.requested_by, &right.requirement))
            });
        }
    }
    if let Some(selected) = state.selected.get(package) {
        if !matches_all(&state.constraints[package], &selected.version) {
            return Err(
                conflict(package, &state.constraints[package], vec![selected.clone()]).into(),
            );
        }
    }
    Ok(())
}

fn matches_all(constraints: &[GraphRequirement], version: &str) -> bool {
    constraints.iter().all(|constraint| {
        crate::package_registry::requirement_matches(&constraint.requirement, version)
            .unwrap_or(false)
    })
}

fn candidate_order(
    left: &GraphCandidate,
    right: &GraphCandidate,
    locked: Option<&str>,
) -> std::cmp::Ordering {
    let left_locked = locked == Some(left.version.as_str());
    let right_locked = locked == Some(right.version.as_str());
    right_locked.cmp(&left_locked).then_with(|| {
        let left_version = Version::parse(&left.version).ok();
        let right_version = Version::parse(&right.version).ok();
        right_version
            .cmp(&left_version)
            .then_with(|| left.version.cmp(&right.version))
    })
}

fn conflict(
    package: &str,
    constraints: &[GraphRequirement],
    mut candidates: Vec<GraphCandidate>,
) -> String {
    candidates.sort_by(|left, right| candidate_order(left, right, None));
    let requirements = constraints
        .iter()
        .map(|constraint| {
            format!(
                "{} requires `{}`",
                constraint.requested_by, constraint.requirement
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let available = candidates
        .iter()
        .map(|candidate| {
            if candidate.yanked {
                format!("{} (yanked)", candidate.version)
            } else {
                candidate.version.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "error[registry_dependency_conflict]: `{package}` cannot be resolved: {requirements}; available: [{available}]"
    )
}

#[cfg(test)]
#[path = "package_registry_solver_test.rs"]
mod tests;
