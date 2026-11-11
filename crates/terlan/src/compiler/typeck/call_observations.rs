//! Read-only observations of accepted call resolution, never a second resolver.
use super::observation_guard::ObservationGuard;
use super::*;
use serde::Serialize;

const LIMIT: usize = 65536;

#[derive(Default, Serialize)]
pub(crate) struct Observations {
    pub(crate) calls: Vec<serde_json::Value>,
    pub(crate) truncated: bool,
}
#[derive(Clone)]
struct Site {
    module: String,
    path: String,
    span: Span,
}
#[derive(Default)]
struct Index {
    module: String,
    collecting: bool,
    sites: HashMap<usize, Site>,
    declarations: HashMap<String, Vec<serde_json::Value>>,
    declaration_count: usize,
}
struct Frame {
    site: Option<Site>,
    targets: Vec<serde_json::Value>,
}
#[derive(Default)]
struct State {
    index: Index,
    frames: Vec<Frame>,
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
            .expect("active call capture")
            .output
    });
    let mut groups: std::collections::BTreeMap<String, Vec<serde_json::Value>> = Default::default();
    for row in output.calls {
        let key = row["site"].to_string();
        let group = groups.entry(key).or_default();
        if !group.contains(&row) {
            group.push(row);
        }
    }
    output.calls = groups
        .into_values()
        .map(|mut rows| {
            if rows.len() == 1 {
                return rows.remove(0);
            }
            let mut row = rows.remove(0);
            row["status"] = serde_json::json!("unavailable");
            row["target"] = serde_json::Value::Null;
            row["reason"] = serde_json::json!("conflicting_resolution_visits");
            row
        })
        .collect();
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
                    ..Default::default()
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
        analyze_syntax_bindings(module);
        ACTIVE.with(|cell| {
            cell.borrow_mut()
                .as_mut()
                .expect("active module")
                .index
                .collecting = false;
        });
    }
    guard
}

pub(super) fn index_expression(expr: &SyntaxExprOutput, path: &str) {
    let is_call = matches!(
        expr.kind,
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall
    );
    if !is_call {
        return;
    }
    ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        let Some(state) = active.as_mut() else {
            return;
        };
        if !state.index.collecting {
            return;
        }
        if state.index.sites.len() >= LIMIT {
            state.output.truncated = true;
            return;
        }
        state.index.sites.insert(
            expr as *const SyntaxExprOutput as usize,
            Site {
                module: state.index.module.clone(),
                path: path.to_string(),
                span: expr.span.into(),
            },
        );
    });
}

fn scheme_key(module: &str, name: &str, scheme: &FunctionScheme) -> String {
    let bounds = scheme
        .bounds
        .iter()
        .map(|b| serde_json::json!({"trait":b.trait_name,"args":b.trait_args}))
        .collect::<Vec<_>>();
    serde_json::json!([
        module,
        name,
        scheme.params,
        scheme.ret,
        scheme.generic_params,
        bounds
    ])
    .to_string()
}
pub(super) fn register_local(module: &str, name: &str, span: Span, scheme: &FunctionScheme) {
    ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        let Some(state) = active.as_mut() else { return; };
        if state.index.declaration_count >= 4096 { state.output.truncated = true; return; }
        let key = scheme_key(module, name, scheme);
        let origin = serde_json::json!({"identity_kind":"checked_source_declaration","module":module,"name":name,"arity":scheme.params.len(),"span_start":span.start,"span_end":span.end});
        let origins = state.index.declarations.entry(key).or_default();
        if !origins.contains(&origin) { origins.push(origin); state.index.declaration_count += 1; }
    });
}
pub(super) fn local(module: &str, name: &str, scheme: &FunctionScheme) {
    ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        let Some(state) = active.as_mut() else {
            return;
        };
        let Some([origin]) = state
            .index
            .declarations
            .get(&scheme_key(module, name, scheme))
            .map(Vec::as_slice)
        else {
            return;
        };
        if let Some(frame) = state.frames.last_mut().filter(|frame| frame.site.is_some()) {
            if !frame.targets.contains(origin) {
                frame.targets.push(origin.clone());
            }
        }
    });
}

pub(super) fn interface_target(
    module: &str,
    signature: &crate::terlan_hir::FunctionSignature,
) -> Option<serde_json::Value> {
    if !ACTIVE.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|s| s.frames.last())
            .is_some_and(|f| f.site.is_some())
    }) {
        return None;
    }
    Some(
        serde_json::json!({"identity_kind":"checked_interface_signature","signature":{
            "module":module,"name":signature.name,"arity":signature.params.len(),
            "parameter_types":signature.params.iter().map(|p| &p.annotation).collect::<Vec<_>>(),
            "return_type":signature.return_type,"generic_params":signature.generic_params,
            "generic_bounds":signature.generic_bounds,"receiver_method":signature.receiver_method,
            "receiver_mutable":signature.receiver_mutable,"public":signature.public,"compiler_proven_pure":signature.pure
        }}),
    )
}
pub(super) fn accepted(target: Option<serde_json::Value>) {
    let Some(target) = target else {
        return;
    };
    ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        let Some(frame) = active
            .as_mut()
            .and_then(|s| s.frames.last_mut())
            .filter(|frame| frame.site.is_some())
        else {
            return;
        };
        if !frame.targets.contains(&target) {
            frame.targets.push(target);
        }
    });
}

pub(super) struct ExpressionGuard(bool);
pub(super) fn expression(expr: &SyntaxExprOutput) -> ExpressionGuard {
    let active = ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        let Some(state) = active.as_mut() else {
            return false;
        };
        state.frames.push(Frame {
            site: state
                .index
                .sites
                .get(&(expr as *const SyntaxExprOutput as usize))
                .cloned(),
            targets: Vec::new(),
        });
        true
    });
    ExpressionGuard(active)
}
impl Drop for ExpressionGuard {
    fn drop(&mut self) {
        if !self.0 {
            return;
        }
        ACTIVE.with(|cell| {
            let mut active = cell.borrow_mut();
            let Some(state) = active.as_mut() else { return; };
            let Some(Frame { site: Some(site), mut targets }) = state.frames.pop() else { return; };
            if state.output.calls.len() >= LIMIT { state.output.truncated = true; return; }
            let resolved = targets.len() == 1;
            let target = if resolved { targets.remove(0) } else { serde_json::Value::Null };
            state.output.calls.push(serde_json::json!({
                "model":"checked-call-resolution.v1",
                "site":{"module":site.module,"path":site.path,"span_start":site.span.start,"span_end":site.span.end,"span_precision":"enclosing_syntax"},
                "status":if resolved {"resolved"} else {"unavailable"}, "target":target,
                "reason":if resolved {None} else {Some("no_unique_exported_static_target")}
            }));
        });
    }
}

#[cfg(test)]
#[path = "call_observations_test.rs"]
mod tests;
