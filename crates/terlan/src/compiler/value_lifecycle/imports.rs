//! Imported compile-time values and valued-union arms.

use super::*;

impl Evaluator {
    pub(super) fn load_imported_values(
        &mut self,
        module: &SyntaxModuleOutput,
        interfaces: &HashMap<String, ModuleInterface>,
    ) {
        for (module_name, interface) in interfaces {
            for (name, signature) in &interface.constants {
                if let Some(value) = literal_expr_value(&signature.value) {
                    self.values.insert(format!("{module_name}.{name}"), value);
                }
            }
            for ((name, arity), signature) in &interface.const_functions {
                self.functions.insert(
                    (format!("{module_name}.{name}"), *arity),
                    ConstFunction {
                        params: signature
                            .params
                            .iter()
                            .map(|param| param.name.clone())
                            .collect(),
                        return_type: signature.return_type.clone(),
                        body: signature.body.clone(),
                    },
                );
            }
            for (type_name, union) in &interface.valued_unions {
                self.load_imported_union(module_name, &format!("{module_name}.{type_name}"), union);
            }
            for (name, signature) in &interface.associated_constants {
                if let Some(value) = literal_expr_value(&signature.value) {
                    self.values.insert(name.clone(), value.clone());
                    self.values.insert(format!("{module_name}.{name}"), value);
                }
            }
        }

        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Import {
                module_name, items, ..
            } = &declaration.payload
            else {
                continue;
            };
            let Some(interface) = interfaces.get(module_name) else {
                continue;
            };
            for item in items {
                if item.name == "*" {
                    for (name, signature) in &interface.constants {
                        if let Some(value) = literal_expr_value(&signature.value) {
                            self.values.insert(name.clone(), value);
                        }
                    }
                    for ((name, arity), signature) in &interface.const_functions {
                        self.functions.insert(
                            (name.clone(), *arity),
                            const_function_from_signature(signature),
                        );
                    }
                    for (type_name, union) in &interface.valued_unions {
                        self.load_imported_union(module_name, type_name, union);
                    }
                    continue;
                }
                let local_name = item.as_alias.as_ref().unwrap_or(&item.name);
                if let Some(signature) = interface.constants.get(&item.name) {
                    if let Some(value) = literal_expr_value(&signature.value) {
                        self.values.insert(local_name.clone(), value);
                    }
                }
                for ((name, arity), signature) in &interface.const_functions {
                    if name == &item.name {
                        self.functions.insert(
                            (local_name.clone(), *arity),
                            const_function_from_signature(signature),
                        );
                    }
                }
                if let Some(union) = interface.valued_unions.get(&item.name) {
                    self.load_imported_union(module_name, local_name, union);
                }
                if let Some((union, arm)) = interface.valued_unions.values().find_map(|union| {
                    union
                        .arms
                        .iter()
                        .find(|arm| arm.name == item.name)
                        .map(|arm| (union, arm))
                }) {
                    if let Some(representation) = literal_expr_value(&arm.value) {
                        self.values.insert(
                            local_name.clone(),
                            ConstValue::Union {
                                name: format!("{module_name}.{}", union.name),
                                arm: arm.name.clone(),
                                representation: Box::new(representation),
                            },
                        );
                    }
                }
            }
        }
    }

    fn load_imported_union(
        &mut self,
        module_name: &str,
        local_type_name: &str,
        union: &crate::terlan_hir::ValuedUnionSignature,
    ) {
        for arm in &union.arms {
            if let Some(representation) = literal_expr_value(&arm.value) {
                self.values.insert(
                    format!("{local_type_name}.{}", arm.name),
                    ConstValue::Union {
                        name: format!("{module_name}.{}", union.name),
                        arm: arm.name.clone(),
                        representation: Box::new(representation),
                    },
                );
            }
        }
    }
}
