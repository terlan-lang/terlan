//! Singleton aliases match using the scrutinee's physical representation.

use super::super::NativeIrResult;
use super::{
    bool_or, encode_managed_variant_is_operation, equality, option_constructor_plan,
    option_element_type, Arc, CoreType, NativeConstructorLayouts, NativeExpr, NativeType,
    PatternPlan,
};

pub(super) fn atom_plan(
    atom: &str,
    value: NativeExpr,
    value_type: NativeType,
    core_type: Option<&CoreType>,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> NativeIrResult<PatternPlan> {
    if atom == "none" {
        if let Some(element) = option_element_type(core_type) {
            return option_constructor_plan(
                "None",
                &[],
                value,
                value_type,
                element,
                constructors,
                depth,
            )
            .map_err(Into::into);
        }
    }
    let immediate = equality(
        value.clone(),
        NativeExpr::AtomLiteral(Arc::from(atom)),
        NativeType::Atom,
    );
    if let (NativeType::ManagedRef(semantic), Some(CoreType::Union(variants))) =
        (value_type, core_type)
    {
        if let Some(discriminant) = variants
            .iter()
            .position(|variant| matches!(variant, CoreType::AtomLiteral(name) if name == atom))
            .and_then(|index| u32::try_from(index).ok())
        {
            return Ok(PatternPlan {
                predicate: bool_or(
                    immediate.predicate,
                    NativeExpr::ManagedOperation {
                        encoded: Arc::from(encode_managed_variant_is_operation(
                            semantic,
                            discriminant,
                        )),
                        args: vec![value],
                    },
                ),
                bindings: vec![],
            });
        }
    }
    Ok(immediate)
}
