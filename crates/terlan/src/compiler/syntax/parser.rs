mod annotations;
mod binary_layout;
mod callables;
mod constants;
mod expressions;
mod field_keys;
mod helpers;
mod html;
mod imports;
mod modules;
mod nesting;
mod patterns;
mod repeated_lets;
mod type_decls;
mod type_params;
mod types;

#[cfg(test)]
#[path = "parser_decl_surface_test.rs"]
mod parser_decl_surface_test;
#[cfg(test)]
#[path = "parser_decl_test.rs"]
mod parser_decl_test;
#[cfg(test)]
#[path = "parser_expr_test.rs"]
mod parser_expr_test;
#[cfg(test)]
#[path = "parser_repeated_let_test.rs"]
mod parser_repeated_let_test;
#[cfg(test)]
#[path = "parser_trait_purity_test.rs"]
mod parser_trait_purity_test;

#[cfg(test)]
#[path = "parser_adversarial_test.rs"]
mod parser_adversarial_test;
#[cfg(test)]
#[path = "parser_html_test.rs"]
mod parser_html_test;

#[cfg(test)]
#[path = "parser_pattern_test.rs"]
mod parser_pattern_test;
#[cfg(test)]
mod type_params_test;
include!("parser_part_001.rs");
include!("parser_part_002.rs");
include!("parser_part_003.rs");
