//! Exact lexical binding/type observations from the authoritative checker.
//!
//! Pointer keys are used only while the borrowed prepared syntax is alive.
//! They never escape into artifacts; exported keys are compiler binding IDs.
use super::observation_guard::ObservationGuard;
use super::*;
use serde::Serialize;

#[derive(Default, Serialize)]
pub(crate) struct Observations {
    pub(crate) bindings: Vec<serde_json::Value>,
    pub(crate) truncated: bool,
}
#[derive(Default)]
struct Index {
    module: String,
    collecting: bool,
    binding_count: usize,
    patterns: HashMap<usize, Vec<CoreBindingIdentity>>,
}
struct Pending {
    module: String,
    binding: CoreBindingIdentity,
    ty: Type,
}
#[derive(Default)]
struct State {
    index: Index,
    frames: Vec<Vec<Pending>>,
    output: Observations,
}
thread_local! { static ACTIVE: RefCell<Option<State>> = const { RefCell::new(None) }; }

pub(crate) fn capture<T>(action: impl FnOnce() -> T) -> (T, Observations) {
    let previous = ACTIVE.with(|cell| cell.replace(Some(State::default())));
    let _restore = ObservationGuard::new(move || {
        ACTIVE.with(|cell| {
            cell.replace(previous);
        });
    });
    let value = action();
    let mut output = ACTIVE.with(|cell| {
        cell.borrow_mut()
            .take()
            .expect("active binding capture")
            .output
    });
    output
        .bindings
        .sort_by_cached_key(serde_json::Value::to_string);
    output.bindings.dedup();
    (value, output)
}

pub(super) fn module(module: &SyntaxModuleOutput) -> ObservationGuard {
    let previous = ACTIVE.with(|cell| {
        cell.borrow_mut().as_mut().map(|state| {
            std::mem::replace(
                &mut state.index,
                Index {
                    module: module.module_name.clone(),
                    collecting: true,
                    binding_count: 0,
                    patterns: HashMap::new(),
                },
            )
        })
    });
    let active = previous.is_some();
    let guard = ObservationGuard::new(move || {
        if let Some(previous) = previous {
            ACTIVE.with(|cell| {
                if let Some(state) = cell.borrow_mut().as_mut() {
                    state.index = previous;
                }
            });
        }
    });
    if active {
        // Reuse the binding analyzer, including its region/shadowing rules.
        analyze_syntax_bindings(module);
        ACTIVE.with(|cell| {
            cell.borrow_mut()
                .as_mut()
                .expect("active module capture")
                .index
                .collecting = false;
        });
    }
    guard
}

pub(super) fn index_pattern(pattern: &SyntaxPatternOutput, bindings: &[CoreBindingIdentity]) {
    ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        let Some(state) = active.as_mut() else {
            return;
        };
        if !state.index.collecting || bindings.is_empty() {
            return;
        }
        if state.index.binding_count.saturating_add(bindings.len()) > 65536 {
            state.output.truncated = true;
            return;
        }
        state.index.binding_count += bindings.len();
        state.index.patterns.insert(
            pattern as *const SyntaxPatternOutput as usize,
            bindings.to_vec(),
        );
    });
}

pub(super) struct ClauseGuard {
    active: bool,
}
pub(super) fn clause() -> ClauseGuard {
    let active = ACTIVE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            state.frames.push(Vec::new());
            true
        } else {
            false
        }
    });
    ClauseGuard { active }
}
impl Drop for ClauseGuard {
    fn drop(&mut self) {
        if self.active {
            ACTIVE.with(|cell| {
                if let Some(state) = cell.borrow_mut().as_mut() {
                    state.frames.pop();
                }
            });
        }
    }
}
impl ClauseGuard {
    pub(super) fn finish(mut self, subst: &HashMap<TypeVarId, Type>) {
        if !self.active {
            return;
        }
        self.active = false;
        ACTIVE.with(|cell| {
            let mut active = cell.borrow_mut();
            let Some(state) = active.as_mut() else {
                return;
            };
            for pending in state.frames.pop().unwrap_or_default() {
                if state.output.bindings.len() >= 65536 {
                    state.output.truncated = true;
                    break;
                }
                let ty = apply_subst(&pending.ty, subst);
                let ty = super::type_system::canonical_observation_type(&ty);
                state.output.bindings.push(serde_json::json!({
                    "model":"checked-binding-type.v1", "module":pending.module,
                    "binding":pending.binding, "compiler_type":ty, "display_type":pretty_type(&ty),
                    "type_encoding":"terlan.typecheck.type.v1",
                    "type_variable_scope":"observation_local", "scope":"checked_binding_definition"
                }));
            }
        });
        self.active = false;
    }
}

/// Called after a checked lexical scope's body has supplied its constraints.
pub(super) fn record(
    patterns: &[SyntaxPatternOutput],
    locals: &HashMap<String, Type>,
    subst: &HashMap<TypeVarId, Type>,
) {
    ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        let Some(state) = active.as_mut() else {
            return;
        };
        let Some(frame) = state.frames.last_mut() else {
            return;
        };
        for pattern in patterns {
            let Some(bindings) = state
                .index
                .patterns
                .get(&(pattern as *const SyntaxPatternOutput as usize))
            else {
                continue;
            };
            for binding in bindings {
                if frame.len() >= 65536 {
                    state.output.truncated = true;
                    return;
                }
                if let Some(ty) = locals.get(&binding.name) {
                    frame.push(Pending {
                        module: state.index.module.clone(),
                        binding: binding.clone(),
                        ty: apply_subst(ty, subst),
                    });
                }
            }
        }
    });
}

#[cfg(test)]
#[path = "binding_type_observations_test.rs"]
mod tests;
