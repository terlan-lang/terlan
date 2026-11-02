//! Scoped serialization of the very bounds used by declaration checking.

use super::*;
use serde::Serialize;

#[derive(Default, Serialize)]
pub(crate) struct Observations {
    pub(crate) requirements: Vec<serde_json::Value>,
    pub(crate) truncated: bool,
}

thread_local! {
    static CAPTURE: RefCell<Option<Observations>> = const { RefCell::new(None) };
}

struct Guard(Option<Observations>);
impl Drop for Guard {
    fn drop(&mut self) {
        CAPTURE.with(|cell| {
            cell.replace(self.0.take());
        });
    }
}

pub(crate) fn capture<T>(action: impl FnOnce() -> T) -> (T, Observations) {
    let previous = CAPTURE.with(|cell| cell.replace(Some(Observations::default())));
    let _restore = Guard(previous);
    let value = action();
    let mut output = CAPTURE.with(|cell| {
        cell.borrow_mut()
            .take()
            .expect("active requirement capture")
    });
    output
        .requirements
        .sort_by_cached_key(serde_json::Value::to_string);
    output.requirements.dedup();
    (value, output)
}

fn record_kind(
    kind: &str,
    module: &str,
    name: &str,
    arity: usize,
    span: Span,
    scheme: &FunctionScheme,
) {
    CAPTURE.with(|cell| {
        let mut active = cell.borrow_mut();
        let Some(output) = active.as_mut() else { return; };
        if output.requirements.len() >= 4096 { output.truncated = true; return; }
        let variables = scheme.generic_params.iter().enumerate().map(|(id, source)| {
            serde_json::json!({"id":id,"name":normalize_type_param_name(source),"declaration":source})
        }).collect::<Vec<_>>();
        let bounds = scheme.bounds.iter().map(|bound| {
            if bound.trait_name == STRUCTURAL_IMPLICATION_BOUND {
                serde_json::json!({"kind":"structural_implication","subject":bound.trait_args.first(),"required_shape":bound.trait_args.get(1)})
            } else {
                serde_json::json!({"kind":"trait","trait_name":bound.trait_name,"arguments":bound.trait_args})
            }
        }).collect::<Vec<_>>();
        let row = serde_json::json!({
            "model":"checked-declaration-requirements.v1","kind":kind,"module":module,"name":name,"arity":arity,
            "span_start":span.start,"span_end":span.end,"type_variables":variables,
            "parameter_types":scheme.params,"return_type":scheme.ret,"bounds":bounds
        });
        if !output.requirements.contains(&row) { output.requirements.push(row); }
    });
}

pub(super) fn record(module: &str, name: &str, arity: usize, span: Span, scheme: &FunctionScheme) {
    record_kind("function", module, name, arity, span, scheme);
}

pub(super) fn record_alias(
    module: &str,
    name: &str,
    span: Span,
    generic_params: &[String],
    alias: &TypeAlias,
) {
    if !CAPTURE.with(|cell| cell.borrow().is_some()) {
        return;
    }
    let scheme = FunctionScheme {
        params: Vec::new(),
        ret: alias.body.clone(),
        generic_params: generic_params.to_vec(),
        bounds: alias.bounds.clone(),
    };
    record_kind("type", module, name, 0, span, &scheme);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheme() -> FunctionScheme {
        FunctionScheme {
            params: vec![Type::Int],
            ret: Type::Int,
            generic_params: Vec::new(),
            bounds: Vec::new(),
        }
    }

    #[test]
    fn nested_interrupted_capture_does_not_contaminate_the_outer_export() {
        let (_, result) = capture(|| {
            let aborted = std::panic::catch_unwind(|| {
                capture(|| {
                    record("inner", "value", 1, Span { start: 0, end: 1 }, &scheme());
                    panic!("interrupted export");
                })
            });
            assert!(aborted.is_err());
            record("outer", "value", 1, Span { start: 0, end: 1 }, &scheme());
        });
        assert_eq!(result.requirements.len(), 1);
        assert_eq!(result.requirements[0]["module"], "outer");
        let (_, next) = capture(|| ());
        assert!(next.requirements.is_empty());
    }

    #[test]
    fn bounded_requirement_capture_cannot_silently_drop_distinct_contracts() {
        let (_, result) = capture(|| {
            for index in 0..4097 {
                record(
                    "sample",
                    "value",
                    1,
                    Span {
                        start: index,
                        end: index + 1,
                    },
                    &scheme(),
                );
            }
        });
        assert_eq!(result.requirements.len(), 4096);
        assert!(result.truncated);
    }
}
