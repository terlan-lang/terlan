use std::collections::HashMap;

use crate::terlan_typeck::CorePattern;

use super::{rewrite, rewrite_pattern, AliasValue};

#[test]
fn bare_type_constructor_pattern_becomes_its_canonical_atom() {
    let mut pattern = CorePattern::Constructor {
        name: "std.binary.Binary.TruncatedPayload".to_string(),
        constructor_identity: Some("std.binary.Binary.TruncatedPayload".to_string()),
        args: Vec::new(),
    };

    rewrite_pattern(
        &mut pattern,
        &HashMap::from([(
            "TruncatedPayload".to_string(),
            AliasValue {
                atom: "truncatedpayload".to_string(),
                managed_variant: false,
            },
        )]),
    );

    assert_eq!(pattern, CorePattern::Atom("truncatedpayload".to_string()));
}

#[test]
fn atom_alias_used_as_managed_union_variant_keeps_constructor_pattern() {
    let aliases = HashMap::from([(
        "None".to_string(),
        AliasValue {
            atom: "none".to_string(),
            managed_variant: true,
        },
    )]);
    let mut expression = crate::terlan_typeck::CoreExpr::Var("None".to_string());
    let mut pattern = CorePattern::Constructor {
        name: "None".to_string(),
        constructor_identity: Some("std.core.Option.None".to_string()),
        args: Vec::new(),
    };

    rewrite(&mut expression, &aliases);
    rewrite_pattern(&mut pattern, &aliases);

    assert_eq!(
        expression,
        crate::terlan_typeck::CoreExpr::Atom("none".to_string())
    );
    assert!(matches!(pattern, CorePattern::Constructor { .. }));
}
