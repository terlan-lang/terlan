#[cfg(test)]
mod tests {
    include!("parser_decl_test_part_001.rs");
    include!("parser_decl_test_part_002.rs");
    include!("parser_decl_test_part_003.rs");
    include!("parser_decl_test_part_004.rs");
}

#[path = "parser_type_alias_shorthand_test.rs"]
mod parser_type_alias_shorthand_test;
