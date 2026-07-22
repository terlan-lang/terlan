//! Bounded compiler expansion for pure single-generator list comprehensions.

use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_list_empty_operation, encode_list_first_operation, encode_list_is_empty_operation,
    encode_list_prepend_operation, encode_list_rest_operation, SemanticTypeId,
};
use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreIfClause, CoreLetBinding, CoreModule, CoreParam, CorePattern,
    CoreProofCoverage, CoreType,
};

use super::{expression::free_variables, native_type, NativeExpr, NativeType};

const MODULE: &str = "$terlan.managed.comprehension";
const MAX_COMPREHENSIONS_PER_MODULE: usize = 128;
const MAX_CAPTURES: usize = 64;

pub(super) fn is_managed_comprehension_module(module: &str) -> bool {
    module == MODULE
}

/// Replaces every admitted whole-result comprehension with one private,
/// closed recursive native helper before application admission.
pub(super) fn lower_list_comprehensions(core: &mut CoreModule) -> Result<(), String> {
    let original_len = core.functions.len();
    let mut helpers = Vec::new();
    for index in 0..original_len {
        let owner = core.functions[index].clone();
        let Some(clause) = core.functions[index].clauses.first_mut() else {
            continue;
        };
        let Some(body) = clause.body.core_expr.as_mut() else {
            continue;
        };
        let CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            lift,
        } = body
        else {
            continue;
        };
        if helpers.len() >= MAX_COMPREHENSIONS_PER_MODULE {
            return Err(format!(
                "error[native_ir.comprehension_budget]: module `{}` exceeds {MAX_COMPREHENSIONS_PER_MODULE} comprehensions",
                core.module
            ));
        }
        if lift.is_some() || generators.len() != 1 {
            return Err(
                "error[native_ir.comprehension_shape]: AOT comprehensions require one generator and no lifted result"
                    .to_string(),
            );
        }
        let generator = &generators[0];
        let CorePattern::Var(element_name) = &generator.pattern else {
            return Err(
                "error[native_ir.comprehension_pattern]: AOT comprehensions require a variable generator pattern"
                    .to_string(),
            );
        };
        if element_name.starts_with('_') {
            return Err(
                "error[native_ir.comprehension_pattern]: generator value must have a usable binding"
                    .to_string(),
            );
        }
        let element_name_owned = element_name.clone();
        let parameter_types = owner
            .params
            .iter()
            .filter_map(|parameter| {
                parameter
                    .core_ty
                    .as_ref()
                    .map(|ty| (parameter.name.clone(), ty.clone()))
            })
            .collect::<HashMap<_, _>>();
        let source_type = core_expr_type(&generator.source, &parameter_types).ok_or_else(|| {
            "error[native_ir.comprehension_source]: generator source needs a concrete List type"
                .to_string()
        })?;
        let input_element = list_element(&source_type)?.clone();
        let output_type = owner.core_return_type.clone().ok_or_else(|| {
            "error[native_ir.comprehension_result]: comprehension result type is unavailable"
                .to_string()
        })?;
        let output_element = list_element(&output_type)?.clone();
        let source_native = native_type(Some(&source_type), &source_type.contract_text())
            .and_then(managed_semantic)
            .ok_or_else(|| {
                "error[native_ir.comprehension_source]: generator List has no managed identity"
                    .to_string()
            })?;
        let output_native = native_type(Some(&output_type), &output_type.contract_text())
            .and_then(managed_semantic)
            .ok_or_else(|| {
                "error[native_ir.comprehension_result]: result List has no managed identity"
                    .to_string()
            })?;
        let input_element_native = native_type(
            Some(&input_element),
            &input_element.contract_text(),
        )
        .ok_or_else(|| {
            "error[native_ir.comprehension_element]: generator element has no native representation"
                .to_string()
        })?;
        let output_element_native = native_type(
            Some(&output_element),
            &output_element.contract_text(),
        )
        .ok_or_else(|| {
            "error[native_ir.comprehension_element]: result element has no native representation"
                .to_string()
        })?;

        let mut captures = free_variables(expr);
        for guard in guards.iter() {
            captures.extend(free_variables(guard));
        }
        captures.remove(element_name);
        let mut captures = captures.into_iter().collect::<Vec<_>>();
        captures.sort();
        if captures.len() > MAX_CAPTURES {
            return Err(format!(
                "error[native_ir.comprehension_captures]: comprehension captures more than {MAX_CAPTURES} values"
            ));
        }
        let capture_params = captures
            .iter()
            .map(|name| {
                let ty = parameter_types.get(name).cloned().ok_or_else(|| {
                    format!(
                        "error[native_ir.comprehension_capture]: `{name}` is not a typed function parameter"
                    )
                })?;
                Ok(CoreParam {
                    name: name.clone(),
                    ty: ty.contract_text(),
                    core_ty: Some(ty),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let helper_name = format!(
            "$aot_comprehension_{}_{}_{}",
            owner.name,
            owner.arity,
            helpers.len()
        );
        let yielded = expr.as_ref().clone();
        let helper_guards = guards.clone();
        let source_argument = generator.source.clone();
        let outer_args = std::iter::once(source_argument)
            .chain(captures.iter().cloned().map(CoreExpr::Var))
            .collect::<Vec<_>>();
        *body = CoreExpr::Call {
            function: helper_name.clone(),
            args: outer_args,
        };
        helpers.push(build_helper(
            &owner,
            helper_name,
            &element_name_owned,
            yielded,
            helper_guards,
            source_type,
            input_element,
            output_type,
            capture_params,
            source_native,
            output_native,
            input_element_native,
            output_element_native,
        )?);
    }
    core.functions.extend(helpers);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_helper(
    owner: &CoreFunction,
    name: String,
    element_name: &str,
    yielded: CoreExpr,
    guards: Vec<CoreExpr>,
    source_type: CoreType,
    input_element: CoreType,
    output_type: CoreType,
    capture_params: Vec<CoreParam>,
    source_semantic: SemanticTypeId,
    output_semantic: SemanticTypeId,
    input_native: NativeType,
    output_native: NativeType,
) -> Result<CoreFunction, String> {
    let mut helper = owner.clone();
    helper.name = name.clone();
    helper.public = false;
    helper.native_operation = None;
    helper.params = std::iter::once(CoreParam {
        name: "$items".to_string(),
        ty: source_type.contract_text(),
        core_ty: Some(source_type),
    })
    .chain(capture_params)
    .collect();
    helper.arity = helper.params.len();
    helper.return_type = output_type.contract_text();
    helper.core_return_type = Some(output_type);
    helper.clauses.truncate(1);
    let clause = helper
        .clauses
        .first_mut()
        .ok_or_else(|| "error[native_ir.comprehension_helper]: owner has no clause".to_string())?;
    clause.patterns = helper
        .params
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect();
    clause.core_patterns = helper
        .params
        .iter()
        .map(|parameter| Some(CorePattern::Var(parameter.name.clone())))
        .collect();
    clause.pattern_proof_coverage = vec![CoreProofCoverage::RuntimeBoundary; helper.params.len()];
    clause.pattern_checked_preservation_evidence = vec![None; helper.params.len()];
    clause.guard = None;
    let recursive_args = std::iter::once(operation(
        &format!("rest:{}", semantic_text(source_semantic)),
        vec![CoreExpr::Var("$items".to_string())],
    ))
    .chain(
        helper
            .params
            .iter()
            .skip(1)
            .map(|parameter| CoreExpr::Var(parameter.name.clone())),
    )
    .collect::<Vec<_>>();
    let recurse = CoreExpr::Call {
        function: name,
        args: recursive_args,
    };
    let guard = guards
        .into_iter()
        .reduce(|left, right| CoreExpr::BinaryOp {
            operator: "and".to_string(),
            left: Box::new(left),
            right: Box::new(right),
        })
        .unwrap_or(CoreExpr::Atom("true".to_string()));
    let prepend = operation(
        &format!("prepend:{}", semantic_text(output_semantic)),
        vec![yielded, recurse.clone()],
    );
    let nonempty = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var(element_name.to_string()),
            value: operation(
                &format!(
                    "first:{}:{}",
                    semantic_text(source_semantic),
                    native_type_text(input_native)
                ),
                vec![CoreExpr::Var("$items".to_string())],
            ),
        }],
        body: Box::new(CoreExpr::If {
            clauses: vec![
                CoreIfClause {
                    condition: guard,
                    body: prepend,
                },
                CoreIfClause {
                    condition: CoreExpr::Atom("true".to_string()),
                    body: recurse,
                },
            ],
        }),
    };
    clause.body.kind = "if".to_string();
    clause.body.core_expr = Some(CoreExpr::If {
        clauses: vec![
            CoreIfClause {
                condition: operation(
                    &format!("is_empty:{}", semantic_text(source_semantic)),
                    vec![CoreExpr::Var("$items".to_string())],
                ),
                body: operation(
                    &format!("empty:{}", semantic_text(output_semantic)),
                    Vec::new(),
                ),
            },
            CoreIfClause {
                condition: CoreExpr::Atom("true".to_string()),
                body: nonempty,
            },
        ],
    });
    clause.body.proof_coverage = CoreProofCoverage::RuntimeBoundary;
    let _ = input_element;
    let _ = output_native;
    Ok(helper)
}

pub(super) fn managed_comprehension_operation_type(expr: &CoreExpr) -> Option<NativeType> {
    let CoreExpr::RemoteCall {
        module, function, ..
    } = expr
    else {
        return None;
    };
    if module != MODULE {
        return None;
    }
    parse_operation(function)
        .ok()
        .map(|operation| operation.result())
}

pub(super) fn lower_managed_comprehension_operation(
    expr: &CoreExpr,
    mut lower: impl FnMut(&CoreExpr) -> Result<NativeExpr, String>,
) -> Result<Option<NativeExpr>, String> {
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = expr
    else {
        return Ok(None);
    };
    if module != MODULE {
        return Ok(None);
    }
    let operation = parse_operation(function)?;
    let expected_arity = match operation {
        Operation::Empty(_) => 0,
        Operation::IsEmpty(_) | Operation::First(_, _) | Operation::Rest(_) => 1,
        Operation::Prepend(_) => 2,
    };
    if args.len() != expected_arity {
        return Err(format!(
            "error[native_ir.comprehension_operation]: `{function}` expects {expected_arity} arguments"
        ));
    }
    let encoded = match operation {
        Operation::Empty(semantic) => encode_list_empty_operation(semantic),
        Operation::IsEmpty(semantic) => encode_list_is_empty_operation(semantic),
        Operation::First(semantic, ty) => {
            encode_list_first_operation(semantic, ty.is_managed_reference())
        }
        Operation::Rest(semantic) => encode_list_rest_operation(semantic),
        Operation::Prepend(semantic) => encode_list_prepend_operation(semantic),
    };
    Ok(Some(NativeExpr::ManagedOperation {
        encoded: Arc::from(encoded),
        args: args.iter().map(&mut lower).collect::<Result<Vec<_>, _>>()?,
    }))
}

enum Operation {
    Empty(SemanticTypeId),
    IsEmpty(SemanticTypeId),
    First(SemanticTypeId, NativeType),
    Rest(SemanticTypeId),
    Prepend(SemanticTypeId),
}

impl Operation {
    fn result(&self) -> NativeType {
        match self {
            Self::IsEmpty(_) => NativeType::Bool,
            Self::First(_, ty) => *ty,
            Self::Empty(semantic) | Self::Rest(semantic) | Self::Prepend(semantic) => {
                NativeType::ManagedRef(*semantic)
            }
        }
    }
}

fn parse_operation(function: &str) -> Result<Operation, String> {
    let parts = function.split(':').collect::<Vec<_>>();
    let semantic = parts
        .get(1)
        .ok_or_else(|| {
            "error[native_ir.comprehension_operation]: missing semantic identity".to_string()
        })
        .and_then(|value| parse_semantic(value))?;
    match parts.as_slice() {
        ["empty", _] => Ok(Operation::Empty(semantic)),
        ["is_empty", _] => Ok(Operation::IsEmpty(semantic)),
        ["rest", _] => Ok(Operation::Rest(semantic)),
        ["prepend", _] => Ok(Operation::Prepend(semantic)),
        ["first", _, ty] => Ok(Operation::First(semantic, parse_native_type(ty)?)),
        _ => Err(format!(
            "error[native_ir.comprehension_operation]: malformed `{function}`"
        )),
    }
}

fn operation(function: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::RemoteCall {
        module: MODULE.to_string(),
        function: function.to_string(),
        args,
    }
}

fn core_expr_type(expr: &CoreExpr, types: &HashMap<String, CoreType>) -> Option<CoreType> {
    match expr {
        CoreExpr::Var(name) => types.get(name).cloned(),
        CoreExpr::List(_) => None,
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        _ => None,
    }
}

fn list_element(ty: &CoreType) -> Result<&CoreType, String> {
    match ty {
        CoreType::List(element) => Ok(element),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Ok(&args[0])
        }
        _ => Err(format!(
            "error[native_ir.comprehension_type]: `{}` is not List[T]",
            ty.contract_text()
        )),
    }
}

fn managed_semantic(ty: NativeType) -> Option<SemanticTypeId> {
    match ty {
        NativeType::ManagedRef(semantic) => Some(semantic),
        _ => None,
    }
}

fn semantic_text(semantic: SemanticTypeId) -> String {
    semantic
        .bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_semantic(text: &str) -> Result<SemanticTypeId, String> {
    if text.len() != 32 {
        return Err(
            "error[native_ir.comprehension_operation]: invalid semantic identity".to_string(),
        );
    }
    let mut bytes = [0u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).map_err(|_| {
            "error[native_ir.comprehension_operation]: invalid semantic identity".to_string()
        })?;
    }
    Ok(SemanticTypeId::from_bytes(bytes))
}

fn native_type_text(ty: NativeType) -> String {
    match ty {
        NativeType::Unit => "u".to_string(),
        NativeType::Int => "i".to_string(),
        NativeType::Float => "f".to_string(),
        NativeType::Bool => "b".to_string(),
        NativeType::Atom => "a".to_string(),
        NativeType::StringRef => "s".to_string(),
        NativeType::BytesRef => "y".to_string(),
        NativeType::BinaryRef => "x".to_string(),
        NativeType::ManagedRef(semantic) => format!("m{}", semantic_text(semantic)),
    }
}

fn parse_native_type(text: &str) -> Result<NativeType, String> {
    match text {
        "u" => Ok(NativeType::Unit),
        "i" => Ok(NativeType::Int),
        "f" => Ok(NativeType::Float),
        "b" => Ok(NativeType::Bool),
        "a" => Ok(NativeType::Atom),
        "s" => Ok(NativeType::StringRef),
        "y" => Ok(NativeType::BytesRef),
        "x" => Ok(NativeType::BinaryRef),
        value if value.starts_with('m') => parse_semantic(&value[1..]).map(NativeType::ManagedRef),
        _ => Err("error[native_ir.comprehension_operation]: invalid native type".to_string()),
    }
}
