#[cfg(test)]
mod tests {
    use crate::terlan_syntax::parse_module;
    use crate::terlan_syntax::parse_tree::Decl;

    #[test]
    fn parses_pure_trait_method_contract() {
        let module = parse_module(
            "module trait_pure.\n\n\
pub trait Normalize[T] {\n\
    @pure\n\
    normalize(value: T): T.\n\
}.\n",
        )
        .expect("parse pure trait method");

        let Decl::Trait(trait_decl) = &module.declarations[0] else {
            panic!("expected trait declaration");
        };
        assert!(trait_decl.methods[0].is_pure);
    }

    #[test]
    fn rejects_pure_trait_method_metadata() {
        let error = parse_module(
            "module trait_pure_metadata.\n\n\
pub trait Normalize[T] {\n\
    @pure { enabled: true }\n\
    normalize(value: T): T.\n\
}.\n",
        )
        .expect_err("reject @pure metadata on trait method");

        assert_eq!(error.message, "@pure does not accept metadata");
    }

    #[test]
    fn rejects_unsupported_trait_method_annotation() {
        let error = parse_module(
            "module trait_annotation_unsupported.\n\n\
pub trait Normalize[T] {\n\
    @memoize\n\
    normalize(value: T): T.\n\
}.\n",
        )
        .expect_err("reject unsupported trait annotation");

        assert_eq!(
            error.message,
            "annotation @memoize is not supported on trait methods"
        );
    }

    #[test]
    fn rejects_duplicate_pure_trait_method_annotation() {
        let error = parse_module(
            "module trait_pure_duplicate.\n\n\
pub trait Normalize[T] {\n\
    @pure\n\
    @pure\n\
    normalize(value: T): T.\n\
}.\n",
        )
        .expect_err("reject duplicate trait purity contract");

        assert_eq!(error.message, "duplicate @pure trait method annotation");
    }
}
