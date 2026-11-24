//! Specializes type-level conditionals when the declaration graph proves the test.

use super::super::types::tuple_parameters;
use super::graph_types::{join, substitute, transform_member};
use super::{TsCallKind, TsDeclaration, TsInterfaceMember, TsTypeAliasDeclaration, TsTypeRef};
use crate::commands::bind::ts_dom_module_mapping::terlan_type_name;
use crate::commands::bind::ts_type_mapping::{
    TsConditionalType, TsConstructorType, TsConstructorUtility, TsPrimitiveType, TsTupleElement,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn normalize_declarations(declarations: &mut [TsDeclaration]) {
    let aliases: BTreeMap<_, _> = declarations
        .iter()
        .filter_map(|declaration| {
            let TsDeclaration::TypeAlias(alias) = declaration else {
                return None;
            };
            (!alias.imported).then(|| {
                (
                    join(&alias.namespace, &terlan_type_name(&alias.name)),
                    alias.clone(),
                )
            })
        })
        .collect();
    let mut computed = BTreeSet::new();
    loop {
        let before = computed.len();
        let forwarded: Vec<_> = aliases
            .iter()
            .filter(|(_, alias)| has_computation(&alias.ty, &computed))
            .map(|(name, _)| name.clone())
            .collect();
        computed.extend(forwarded);
        if computed.len() == before {
            break;
        }
    }
    let constructors = declarations
        .iter()
        .filter_map(|declaration| {
            let TsDeclaration::Interface(interface) = declaration else {
                return None;
            };
            let [TsInterfaceMember::Method(method)] = interface.members.as_slice() else {
                return None;
            };
            if method.call_kind != TsCallKind::Construct
                || !interface.parents.is_empty()
                || !method.type_params.is_empty()
            {
                return None;
            }
            Some((
                join(&interface.namespace, &terlan_type_name(&interface.name)),
                (
                    interface.type_params.clone(),
                    TsTypeRef::Constructor(Box::new(TsConstructorType {
                        is_abstract: false,
                        params: tuple_parameters(method.params.clone()),
                        instance: method.return_type.clone(),
                    })),
                ),
            ))
        })
        .collect();
    let evaluator = Evaluator {
        aliases,
        computed,
        constructors,
    };
    let normalize = |ty: &mut TsTypeRef| {
        let mut active = BTreeSet::new();
        evaluator.normalize(ty, &mut active);
        evaluator.project_variadic_references(ty, &mut active);
    };
    for declaration in declarations {
        match declaration {
            TsDeclaration::TypeAlias(alias) => {
                normalize(&mut alias.ty);
                for constraint in alias.type_constraints.iter_mut().flatten() {
                    if evaluator.proves_callable_domain(constraint) {
                        *constraint = TsTypeRef::CallableBound;
                    }
                }
            }
            TsDeclaration::Interface(interface) => {
                for member in &mut interface.members {
                    transform_member(member, &interface.type_params, |_, ty| normalize(ty));
                }
            }
            TsDeclaration::Unsupported(_) => {}
        }
    }
}

struct Evaluator {
    aliases: BTreeMap<String, TsTypeAliasDeclaration>,
    computed: BTreeSet<String>,
    constructors: BTreeMap<String, (Vec<String>, TsTypeRef)>,
}

impl Evaluator {
    fn proves_callable_domain(&self, ty: &TsTypeRef) -> bool {
        let mut active = BTreeSet::new();
        let TsTypeRef::RestCallback { rest, return_type } = self.operand(ty, &mut active) else {
            return false;
        };
        self.unrestricted_signature(&rest, &return_type, &mut active)
    }

    fn unrestricted_signature(
        &self,
        rest: &TsTypeRef,
        return_type: &TsTypeRef,
        active: &mut BTreeSet<String>,
    ) -> bool {
        self.rest_domain(rest, active).is_some()
            && matches!(
                self.operand(return_type, active),
                TsTypeRef::Any | TsTypeRef::Unknown
            )
    }

    fn rest_domain(&self, rest: &TsTypeRef, active: &mut BTreeSet<String>) -> Option<&'static str> {
        let array = self.operand(rest, active);
        let array = match array {
            TsTypeRef::Tuple(tuple) => {
                let [TsTupleElement { rest: true, ty, .. }] = tuple.as_slice() else {
                    return None;
                };
                self.operand(ty, active)
            }
            other => other,
        };
        let element = match &array {
            TsTypeRef::Array(element) => element.as_ref(),
            TsTypeRef::Generic { name, args }
                if (name == "Array" || name == "ReadonlyArray") && args.len() == 1 =>
            {
                &args[0]
            }
            _ => return None,
        };
        match self.operand(element, active) {
            TsTypeRef::Any => Some("std.js.Callable.AnyArguments"),
            TsTypeRef::Never => Some("std.js.Callable.NeverArguments"),
            _ => None,
        }
    }

    fn project_variadic_references(&self, ty: &mut TsTypeRef, active: &mut BTreeSet<String>) {
        // Keep type-level operands intact until their conditions and utilities have resolved.
        if matches!(
            ty,
            TsTypeRef::Conditional(_) | TsTypeRef::ConstructorUtility(..)
        ) {
            return;
        }
        ty.for_each_child_mut(|child| self.project_variadic_references(child, active));
        if let TsTypeRef::RestCallback { rest, return_type } = ty {
            if matches!(self.operand(return_type, active), TsTypeRef::Unknown) {
                if let Some(domain) = self.rest_domain(rest, active) {
                    *ty = variadic_reference("std.js.Callable.VariadicCallable", domain);
                }
            }
            return;
        }
        if !matches!(ty, TsTypeRef::Constructor(_))
            && !reference(ty).is_some_and(|(name, _)| self.constructors.contains_key(name))
        {
            return;
        }
        let TsTypeRef::Constructor(constructor) = self.operand(ty, active) else {
            return;
        };
        if constructor.is_abstract
            || !matches!(
                self.operand(&constructor.instance, active),
                TsTypeRef::Unknown
            )
        {
            return;
        }
        if let [TsTupleElement {
            rest: true,
            ty: rest,
            ..
        }] = constructor.params.as_slice()
        {
            if let Some(domain) = self.rest_domain(rest, active) {
                *ty = variadic_reference("std.js.Callable.VariadicConstructor", domain);
            }
        }
    }

    fn normalize(&self, ty: &mut TsTypeRef, active: &mut BTreeSet<String>) {
        if let TsTypeRef::Conditional(conditional) = ty {
            // Evaluate only the selected branch. The other one can use a type
            // operator outside the binding language.
            if let Some(mut selected) = self.select(conditional, active) {
                self.normalize(&mut selected, active);
                *ty = selected;
            }
            return;
        }
        ty.for_each_child_mut(|child| self.normalize(child, active));
        if let TsTypeRef::ConstructorUtility(utility, arg) = ty {
            if let TsTypeRef::Constructor(constructor) = self.operand(arg, active) {
                *ty = match utility {
                    TsConstructorUtility::Instance => constructor.instance,
                    TsConstructorUtility::Parameters => TsTypeRef::Tuple(
                        constructor
                            .params
                            .into_iter()
                            .flat_map(|parameter| match parameter {
                                TsTupleElement {
                                    rest: true,
                                    ty: TsTypeRef::Tuple(elements),
                                    ..
                                } => elements,
                                parameter => vec![parameter],
                            })
                            .collect(),
                    ),
                };
            }
            return;
        }
        if let TsTypeRef::RestCallback { rest, return_type } = ty {
            if let TsTypeRef::Tuple(parameters) = rest.as_ref() {
                if parameters
                    .iter()
                    .all(|parameter| !parameter.optional && !parameter.rest)
                {
                    *ty = TsTypeRef::Callback {
                        params: parameters
                            .iter()
                            .map(|parameter| parameter.ty.clone())
                            .collect(),
                        return_type: return_type.clone(),
                    };
                    return;
                }
            }
        }
        if let Some((name, _)) = reference(ty) {
            if self.computed.contains(name) {
                if let Some(resolved) = self.expand(ty, active, false) {
                    if !has_computation(&resolved, &BTreeSet::new()) {
                        *ty = resolved;
                    }
                }
            }
        }
        if let TsTypeRef::Union(members) = ty {
            let mut flat = Vec::new();
            for member in std::mem::take(members) {
                match member {
                    TsTypeRef::Union(nested) => flat.extend(nested),
                    other => flat.push(other),
                }
            }
            flat.retain(|member| *member != TsTypeRef::Never);
            *ty = match flat.len() {
                0 => TsTypeRef::Never,
                1 => flat.pop().unwrap(),
                _ => TsTypeRef::Union(flat),
            };
        }
    }

    fn select(
        &self,
        conditional: &TsConditionalType,
        active: &mut BTreeSet<String>,
    ) -> Option<TsTypeRef> {
        let check = self.operand(&conditional.check, active);
        if conditional.distribute_over.is_some() {
            match check {
                TsTypeRef::Never => return Some(TsTypeRef::Never),
                TsTypeRef::Union(_) => return None,
                _ => {}
            }
        }
        let constraint = self.operand(&conditional.constraint, active);
        let passes = self.extends(&check, &constraint, active)?;
        Some(if passes {
            conditional.then_type.clone()
        } else {
            conditional.else_type.clone()
        })
    }

    fn constructor_extends(
        &self,
        check: &TsConstructorType,
        constraint: &TsConstructorType,
        active: &mut BTreeSet<String>,
    ) -> Option<bool> {
        if check.is_abstract && !constraint.is_abstract {
            return Some(false);
        }
        let [rest] = constraint.params.as_slice() else {
            return None;
        };
        (rest.rest && self.unrestricted_signature(&rest.ty, &constraint.instance, active))
            .then_some(true)
    }

    fn extends(
        &self,
        check: &TsTypeRef,
        constraint: &TsTypeRef,
        active: &mut BTreeSet<String>,
    ) -> Option<bool> {
        use TsTypeRef::*;
        match (check, constraint) {
            (Never, _) => Some(true),
            (Union(items), _) => combine(
                items
                    .iter()
                    .map(|item| self.extends(item, constraint, active)),
                true,
            ),
            (_, Union(items)) => combine(
                items.iter().map(|item| self.extends(check, item, active)),
                false,
            ),
            (Constructor(check), Constructor(constraint)) => {
                self.constructor_extends(check, constraint, active)
            }
            (Callback { .. } | RestCallback { .. }, ConstructorType | Constructor(_))
            | (ConstructorType | Constructor(_), Callback { .. } | RestCallback { .. }) => {
                Some(false)
            }
            (Callback { .. } | RestCallback { .. }, RestCallback { rest, return_type }) => self
                .unrestricted_signature(rest, return_type, active)
                .then_some(true),
            _ => scalar_extends(check, constraint),
        }
    }

    fn operand(&self, ty: &TsTypeRef, active: &mut BTreeSet<String>) -> TsTypeRef {
        if let Some(expanded) = self.expand(ty, active, true) {
            return expanded;
        }
        if let Some((name, args)) = reference(ty) {
            if let Some((params, signature)) = self.constructors.get(name) {
                if params.len() == args.len() {
                    let mut signature = signature.clone();
                    substitute(
                        &mut signature,
                        &params.iter().cloned().zip(args.iter().cloned()).collect(),
                    );
                    return signature;
                }
            }
        }
        let mut result = ty.clone();
        if let TsTypeRef::Union(members) = &mut result {
            for member in members {
                *member = self.operand(member, active);
            }
        }
        self.normalize(&mut result, active);
        result
    }

    fn expand(
        &self,
        ty: &TsTypeRef,
        active: &mut BTreeSet<String>,
        operand: bool,
    ) -> Option<TsTypeRef> {
        let (name, args) = reference(ty)?;
        let alias = self.aliases.get(name)?;
        if alias.type_params.len() != args.len() || !active.insert(name.into()) {
            return None;
        }
        let replacements: BTreeMap<_, _> = alias
            .type_params
            .iter()
            .cloned()
            .zip(args.iter().map(|arg| {
                // A union named through a transparent alias still distributes.
                match self.operand(arg, active) {
                    shape @ (TsTypeRef::Union(_) | TsTypeRef::Never) => shape,
                    _ => arg.clone(),
                }
            }))
            .collect();
        for (parameter, constraint) in alias.type_params.iter().zip(&alias.type_constraints) {
            if let Some(constraint) = constraint {
                let mut constraint = constraint.clone();
                substitute(&mut constraint, &replacements);
                let bound = self.operand(&constraint, active);
                let argument = self.operand(&replacements[parameter], active);
                if self.extends(&argument, &bound, active) != Some(true) {
                    active.remove(name);
                    return None;
                }
            }
        }
        let mut body = alias.ty.clone();
        substitute(&mut body, &replacements);
        if operand {
            body = self.operand(&body, active);
        } else {
            self.normalize(&mut body, active);
        }
        active.remove(name);
        Some(body)
    }
}

fn variadic_reference(name: &str, domain: &str) -> TsTypeRef {
    TsTypeRef::Generic {
        name: name.into(),
        args: vec![TsTypeRef::Named(domain.into())],
    }
}

fn reference(ty: &TsTypeRef) -> Option<(&str, &[TsTypeRef])> {
    match ty {
        TsTypeRef::Named(name) => Some((name, &[])),
        TsTypeRef::Generic { name, args } => Some((name, args)),
        _ => None,
    }
}

fn has_computation(ty: &TsTypeRef, computed: &BTreeSet<String>) -> bool {
    let child = |ty| has_computation(ty, computed);
    match ty {
        TsTypeRef::Conditional(_) | TsTypeRef::ConstructorUtility(..) => true,
        TsTypeRef::Named(name) => computed.contains(name),
        TsTypeRef::Generic { name, args } => computed.contains(name) || args.iter().any(child),
        TsTypeRef::Union(items) => items.iter().any(child),
        TsTypeRef::RestCallback { rest, return_type } => child(rest) || child(return_type),
        TsTypeRef::Callback {
            params,
            return_type,
        } => params.iter().any(child) || child(return_type),
        _ => false,
    }
}

fn scalar_extends(check: &TsTypeRef, constraint: &TsTypeRef) -> Option<bool> {
    use TsTypeRef::*;
    match (check, constraint) {
        (Primitive(left), Primitive(right))
            if !matches!(left, TsPrimitiveType::Void)
                && !matches!(right, TsPrimitiveType::Void) =>
        {
            Some(left == right)
        }
        (StringLiteral(_), Primitive(right)) => Some(*right == TsPrimitiveType::String),
        (NumberLiteral(_), Primitive(right)) => Some(*right == TsPrimitiveType::Number),
        (BooleanLiteral(_), Primitive(right)) => Some(*right == TsPrimitiveType::Boolean),
        (StringLiteral(left), StringLiteral(right)) => Some(left == right),
        (BooleanLiteral(left), BooleanLiteral(right)) => Some(left == right),
        _ => None,
    }
}

fn combine(items: impl Iterator<Item = Option<bool>>, all: bool) -> Option<bool> {
    let mut resolved = true;
    for result in items {
        match result {
            Some(result) if result != all => return Some(result),
            Some(_) => {}
            None => resolved = false,
        }
    }
    resolved.then_some(all)
}
