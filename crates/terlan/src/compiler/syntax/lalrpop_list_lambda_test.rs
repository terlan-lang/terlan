use super::*;
use crate::compiler::syntax::{
    lalrpop_projection::{function_body_arrow_indices, lambda_opening_indices},
    lexer::lex,
};

fn groups(source: &str) -> Vec<(usize, usize)> {
    let tokens = lex(source).expect("lex fixture");
    let function_arrows = function_body_arrow_indices(&tokens);
    let openings = lambda_opening_indices(&tokens, &function_arrows);
    direct_list_lambda_groups(&tokens, &openings)
        .into_iter()
        .map(|(opening, boundary)| (tokens[opening].start, tokens[boundary].start))
        .collect()
}

#[test]
fn direct_list_lambdas_stop_at_owned_delimiters() {
    assert_eq!(
        groups("[() -> pair(1, [2, 3]), (value) -> value]"),
        vec![(1, 22), (24, 40)],
    );
    assert_eq!(groups("[() -> 1 | tail]"), vec![(1, 9)]);
}

#[test]
fn nested_and_unclosed_lambdas_are_not_direct_complete_elements() {
    assert!(groups("[call(() -> 1)]").is_empty());
    assert!(groups("[() -> 1").is_empty());
}
