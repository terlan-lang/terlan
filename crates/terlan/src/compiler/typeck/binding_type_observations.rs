//! Exact lexical binding/type observations from the authoritative checker.
//!
//! Pointer keys are used only while the borrowed prepared syntax is alive.
//! They never escape into artifacts; exported keys are compiler binding IDs.
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
struct CaptureGuard(Option<State>);
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        ACTIVE.with(|cell| {
            cell.replace(self.0.take());
        });
    }
}

pub(crate) fn capture<T>(action: impl FnOnce() -> T) -> (T, Observations) {
    let _restore = CaptureGuard(ACTIVE.with(|cell| cell.replace(Some(State::default()))));
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

pub(super) struct ModuleGuard(Option<Index>);
impl Drop for ModuleGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            ACTIVE.with(|cell| {
                if let Some(state) = cell.borrow_mut().as_mut() {
                    state.index = previous;
                }
            });
        }
    }
}
pub(super) fn module(module: &SyntaxModuleOutput) -> ModuleGuard {
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
    let guard = ModuleGuard(previous);
    if guard.0.is_some() {
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
mod tests {
    use super::*;

    fn pattern() -> SyntaxPatternOutput {
        SyntaxPatternOutput {
            kind: SyntaxPatternKind::Var,
            arity: 0,
            text: Some("local".to_string()),
            children: Vec::new(),
            fields: Vec::new(),
            expanded_from: Vec::new(),
        }
    }

    fn identity() -> CoreBindingIdentity {
        CoreBindingIdentity {
            id: CoreBindingId(u64::MAX),
            region: CoreBindingRegionId(u64::MAX),
            name: "local".to_string(),
            kind: CoreBindingKind::Pattern,
            path: "function:value/0:binding:0".to_string(),
            span_start: 1,
            span_end: 2,
        }
    }

    fn prepare(pattern: &SyntaxPatternOutput) {
        ACTIVE.with(|cell| {
            let mut active = cell.borrow_mut();
            let index = &mut active.as_mut().unwrap().index;
            index.module = "sample".to_string();
            index.collecting = true;
        });
        index_pattern(pattern, &[identity()]);
    }

    #[test]
    fn abandoned_clause_and_nested_capture_do_not_leak_observations() {
        let pattern = pattern();
        let (_, observations) = capture(|| {
            prepare(&pattern);
            let _abandoned = clause();
            record(
                std::slice::from_ref(&pattern),
                &HashMap::from([("local".to_string(), Type::Int)]),
                &HashMap::new(),
            );
            let (_, nested) = capture(|| {});
            assert!(nested.bindings.is_empty());
        });
        assert!(observations.bindings.is_empty());
        assert!(ACTIVE.with(|cell| cell.borrow().is_none()));
    }

    #[test]
    fn index_bound_reports_incomplete_capture_without_adding_pointer_keys() {
        let pattern = pattern();
        let (_, observations) = capture(|| {
            ACTIVE.with(|cell| {
                let mut active = cell.borrow_mut();
                let index = &mut active.as_mut().unwrap().index;
                index.collecting = true;
                index.binding_count = 65536;
            });
            index_pattern(&pattern, &[identity()]);
            assert!(ACTIVE.with(|cell| cell.borrow().as_ref().unwrap().index.patterns.is_empty()));
        });
        assert!(observations.truncated);
        assert!(observations.bindings.is_empty());
    }

    #[test]
    fn final_clause_substitution_precedes_observation_variable_canonicalization() {
        let pattern = pattern();
        let (_, observations) = capture(|| {
            prepare(&pattern);
            let completed = clause();
            record(
                std::slice::from_ref(&pattern),
                &HashMap::from([("local".to_string(), Type::Var(47))]),
                &HashMap::new(),
            );
            completed.finish(&HashMap::from([(47, Type::Int)]));
        });
        assert!(!observations.truncated);
        assert_eq!(observations.bindings.len(), 1);
        assert_eq!(observations.bindings[0]["compiler_type"], "Int");
        assert_eq!(observations.bindings[0]["binding"]["id"], u64::MAX);
    }

    #[test]
    fn canonical_variables_preserve_array_and_existential_relationships() {
        fn ty(free: usize, bound: usize) -> Type {
            Type::Function {
                params: vec![Type::Var(free)],
                ret: Box::new(Type::Existential {
                    params: vec![bound],
                    body: Box::new(Type::Array {
                        size: super::super::types::ArraySize::Param(bound),
                        elem: Box::new(Type::Tuple(vec![Type::Var(free), Type::Var(bound)])),
                    }),
                }),
            }
        }
        assert_eq!(
            super::super::type_system::canonical_observation_type(&ty(42, 99)),
            ty(0, 1)
        );
        assert_eq!(
            super::super::type_system::canonical_observation_type(&ty(700, 3)),
            ty(0, 1)
        );
    }
}
