//! Scalar and string primitive lowering for CoreIR Erlang emission.
//!
//! Inputs:
//! - Already-lowered Erlang argument expressions for primitive CoreIR
//!   intrinsics.
//!
//! Outputs:
//! - Erlang expressions implementing Bool, Atom, Int, Float, and String
//!   operations.
//!
//! Transformations:
//! - Maps backend-neutral primitive operations onto BEAM-safe expression
//!   builders while keeping Terlan option, ordering, and string contracts
//!   stable at the source boundary.

use super::super::erl::{ErlBinaryOp, ErlCaseClause, ErlExpr, ErlIfClause, ErlPattern};
use super::{
    erl_exact_eq, erl_none, erl_remote_call, erl_some, exact_args, exact_array_args,
    nomatch_case_to_bool,
};

/// Lowers `core.bool.equal` to Erlang exact equality.
///
/// Inputs:
/// - `args`: two lowered Erlang boolean expressions.
///
/// Output:
/// - `Some(left =:= right)` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses BEAM exact equality as the implementation of the closed Terlan Bool
///   equality hook.
pub(super) fn lower_core_bool_equal(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [left, right] = exact_array_args(args)?;
    Some(erl_exact_eq(left, right))
}

/// Lowers `core.bool.compare` to the ordering comparison domain.
///
/// Inputs:
/// - `args`: two lowered Erlang boolean expressions.
///
/// Output:
/// - Erlang case expression returning `lt`, `eq`, or `gt`.
///
/// Transformation:
/// - Encodes Terlan's canonical `false < true` ordering behind the
///   backend-neutral Bool comparison intrinsic.
pub(super) fn lower_core_bool_compare(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [left, right] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(ErlExpr::Tuple(vec![left, right])),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Tuple(vec![
                    ErlPattern::Atom("false".to_string()),
                    ErlPattern::Atom("true".to_string()),
                ]),
                guard: None,
                body: ErlExpr::Atom("lt".to_string()),
            },
            ErlCaseClause {
                pattern: ErlPattern::Tuple(vec![
                    ErlPattern::Atom("true".to_string()),
                    ErlPattern::Atom("false".to_string()),
                ]),
                guard: None,
                body: ErlExpr::Atom("gt".to_string()),
            },
            ErlCaseClause {
                pattern: ErlPattern::Wildcard,
                guard: None,
                body: ErlExpr::Atom("eq".to_string()),
            },
        ],
    })
}

/// Lowers `core.bool.to_string` to canonical Bool text.
///
/// Inputs:
/// - `args`: one lowered Erlang boolean expression.
///
/// Output:
/// - Erlang case expression returning `"true"` or `"false"`.
///
/// Transformation:
/// - Converts the closed Terlan Bool runtime values into their canonical
///   source-level string spellings.
pub(super) fn lower_core_bool_to_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(value),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Atom("true".to_string()),
                guard: None,
                body: ErlExpr::Binary("\"true\"".to_string()),
            },
            ErlCaseClause {
                pattern: ErlPattern::Atom("false".to_string()),
                guard: None,
                body: ErlExpr::Binary("\"false\"".to_string()),
            },
        ],
    })
}

/// Lowers `core.bool.from_string` to a Terlan option shape.
///
/// Inputs:
/// - `args`: one lowered Erlang string expression.
///
/// Output:
/// - Erlang case expression returning `some(true)`, `some(false)`, or `none`.
///
/// Transformation:
/// - Recognizes only the canonical Bool strings admitted by
///   `std.core.String.Parse[Bool]`.
pub(super) fn lower_core_bool_from_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(value),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Var("Value".to_string()),
                guard: Some(erl_exact_eq(
                    ErlExpr::Var("Value".to_string()),
                    ErlExpr::Binary("\"true\"".to_string()),
                )),
                body: erl_some(ErlExpr::Atom("true".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::Var("Value".to_string()),
                guard: Some(erl_exact_eq(
                    ErlExpr::Var("Value".to_string()),
                    ErlExpr::Binary("\"false\"".to_string()),
                )),
                body: erl_some(ErlExpr::Atom("false".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::Wildcard,
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.atom.to_string` to canonical Atom text.
///
/// Inputs:
/// - `args`: one lowered Erlang atom expression.
///
/// Output:
/// - `Some(erlang:atom_to_list(value))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Converts Terlan's language-neutral singleton atom runtime value into its
///   canonical `String` spelling without exposing backend atom syntax in user
///   source.
pub(super) fn lower_core_atom_to_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call(
        "erlang",
        "atom_to_list",
        exact_args(args, 1)?,
    ))
}

/// Lowers `core.int.to_string` to Erlang integer display.
///
/// Inputs:
/// - `args`: one lowered Erlang integer expression.
///
/// Output:
/// - `Some(erlang:integer_to_list(value))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses Erlang's integer rendering as the current BEAM implementation of the
///   Terlan canonical integer display contract.
pub(super) fn lower_core_int_to_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call(
        "erlang",
        "integer_to_list",
        exact_args(args, 1)?,
    ))
}

/// Lowers `core.int.from_string` to Erlang integer parsing.
///
/// Inputs:
/// - `args`: one lowered Erlang string expression.
///
/// Output:
/// - Erlang case expression returning `some(parsed)` only when parsing consumes
///   the whole string, otherwise `none`.
///
/// Transformation:
/// - Calls `string:to_integer/1`, checks for an empty rest string, and converts
///   Erlang parser output into the Terlan option runtime shape.
pub(super) fn lower_core_int_from_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    lower_core_parse_from_string("to_integer", args)
}

/// Lowers `core.float.to_string` to Erlang finite-float display.
///
/// Inputs:
/// - `args`: one lowered Erlang float expression.
///
/// Output:
/// - `Some(erlang:float_to_list(value, Options))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses compact decimal formatting with sixteen decimals to preserve the
///   current 0.0.1 BEAM behavior behind a CoreIR intrinsic boundary.
pub(super) fn lower_core_float_to_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "erlang",
        "float_to_list",
        vec![
            value,
            ErlExpr::List(vec![
                ErlExpr::Tuple(vec![
                    ErlExpr::Atom("decimals".to_string()),
                    ErlExpr::Int(16),
                ]),
                ErlExpr::Atom("compact".to_string()),
            ]),
        ],
    ))
}

/// Lowers `core.float.from_string` to Erlang float parsing.
///
/// Inputs:
/// - `args`: one lowered Erlang string expression.
///
/// Output:
/// - Erlang case expression returning `some(parsed)` only when parsing consumes
///   the whole string, otherwise `none`.
///
/// Transformation:
/// - Calls `string:to_float/1`, checks for an empty rest string, and converts
///   Erlang parser output into the Terlan option runtime shape.
pub(super) fn lower_core_float_from_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    lower_core_parse_from_string("to_float", args)
}

/// Lowers string-backed numeric parsing into a Terlan option expression.
///
/// Inputs:
/// - `function`: Erlang `string` module parser function name.
/// - `args`: one lowered Erlang string expression.
///
/// Output:
/// - Erlang case expression returning `some(parsed)` for a full parse and
///   `none` otherwise.
///
/// Transformation:
/// - Converts Erlang parser tuples `{Parsed, Rest}` into Terlan option values
///   and requires `Rest =:= ""` to avoid accepting prefixes.
pub(super) fn lower_core_parse_from_string(function: &str, args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(erl_remote_call("string", function, vec![value])),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Tuple(vec![
                    ErlPattern::Var("Parsed".to_string()),
                    ErlPattern::Var("Rest".to_string()),
                ]),
                guard: Some(erl_exact_eq(
                    ErlExpr::Var("Rest".to_string()),
                    ErlExpr::Binary("\"\"".to_string()),
                )),
                body: erl_some(ErlExpr::Var("Parsed".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::Wildcard,
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.string.append` to Erlang string concatenation.
///
/// Inputs:
/// - `args`: two lowered Erlang string expressions.
///
/// Output:
/// - `Some(string:concat(left, right))` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Reuses Erlang's string concat primitive for the backend implementation of
///   Terlan string append.
pub(super) fn lower_core_string_append(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call("string", "concat", exact_args(args, 2)?))
}

/// Lowers `core.string.equal` to Erlang exact equality.
///
/// Inputs:
/// - `args`: two lowered string expressions.
///
/// Output:
/// - `Some(left =:= right)` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses exact equality for the current BEAM representation of Terlan UTF-8
///   strings while keeping the source operation backend neutral.
pub(super) fn lower_core_string_equal(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [left, right] = exact_array_args(args)?;
    Some(erl_exact_eq(left, right))
}

/// Lowers `core.string.compare` to the ordering comparison domain.
///
/// Inputs:
/// - `args`: two lowered string expressions.
///
/// Output:
/// - Erlang conditional returning `lt`, `eq`, or `gt`.
///
/// Transformation:
/// - Encodes Terlan's stable source string ordering behind the backend-neutral
///   String comparison intrinsic.
pub(super) fn lower_core_string_compare(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [left, right] = exact_array_args(args)?;
    Some(ErlExpr::If(vec![
        ErlIfClause {
            condition: erl_exact_eq(left.clone(), right.clone()),
            body: ErlExpr::Atom("eq".to_string()),
        },
        ErlIfClause {
            condition: ErlExpr::BinaryOp {
                op: ErlBinaryOp::Lt,
                left: Box::new(left),
                right: Box::new(right),
            },
            body: ErlExpr::Atom("lt".to_string()),
        },
        ErlIfClause {
            condition: ErlExpr::Atom("true".to_string()),
            body: ErlExpr::Atom("gt".to_string()),
        },
    ]))
}

/// Lowers `core.string.to_string` to an identity expression.
///
/// Inputs:
/// - `args`: one lowered string expression.
///
/// Output:
/// - The same Erlang expression when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Preserves the target string representation because `String` already is
///   its canonical textual form.
pub(super) fn lower_core_string_to_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(value)
}

/// Lowers `core.string.from_string` to a successful Terlan option.
///
/// Inputs:
/// - `args`: one lowered string expression.
///
/// Output:
/// - `some(value)` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Wraps the unchanged input because every Terlan `String` is already a
///   valid parsed `String` value.
pub(super) fn lower_core_string_from_string(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_some(value))
}

/// Lowers `core.string.is_empty` to an empty-string comparison.
///
/// Inputs:
/// - `args`: one lowered string expression.
///
/// Output:
/// - `Some(value =:= "")` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Compares against the canonical empty string literal for the current BEAM
///   representation.
pub(super) fn lower_core_string_is_empty(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_exact_eq(value, ErlExpr::Binary("\"\"".to_string())))
}

/// Lowers `core.string.concat` to a binary-safe Erlang string conversion.
///
/// Inputs:
/// - `args`: one lowered Erlang list expression containing strings.
///
/// Output:
/// - `Some(unicode:characters_to_list(strings))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Treats the input collection as Unicode character data/iolist and returns
///   the canonical Erlang charlist representation used by current BEAM string
///   literals.
pub(super) fn lower_core_string_concat(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call(
        "unicode",
        "characters_to_list",
        exact_args(args, 1)?,
    ))
}

/// Lowers `core.string.contains` to a nomatch case expression.
///
/// Inputs:
/// - `args`: string value and search pattern expressions.
///
/// Output:
/// - Erlang case expression returning booleans.
///
/// Transformation:
/// - Calls `string:find/2` and converts the Erlang `'nomatch'` sentinel into
///   a target-neutral boolean result.
pub(super) fn lower_core_string_contains(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, pattern] = exact_array_args(args)?;
    Some(nomatch_case_to_bool(erl_remote_call(
        "string",
        "find",
        vec![value, pattern],
    )))
}

/// Lowers `core.string.starts_with` to an Erlang prefix check.
///
/// Inputs:
/// - `args`: string value and prefix expressions.
///
/// Output:
/// - Erlang case expression returning booleans.
///
/// Transformation:
/// - Calls `string:prefix/2` and converts the Erlang `'nomatch'` sentinel into
///   a target-neutral boolean result.
pub(super) fn lower_core_string_starts_with(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, pattern] = exact_array_args(args)?;
    Some(nomatch_case_to_bool(erl_remote_call(
        "string",
        "prefix",
        vec![value, pattern],
    )))
}

/// Lowers `core.string.ends_with` to an Erlang trailing search.
///
/// Inputs:
/// - `args`: string value and suffix expressions.
///
/// Output:
/// - Erlang case expression returning booleans.
///
/// Transformation:
/// - Treats the empty suffix as always true, otherwise searches from the
///   trailing end and checks that the found suffix exactly equals the requested
///   suffix.
pub(super) fn lower_core_string_ends_with(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, suffix] = exact_array_args(args)?;
    let search = erl_remote_call(
        "string",
        "find",
        vec![value, suffix.clone(), ErlExpr::Atom("trailing".to_string())],
    );
    Some(ErlExpr::If(vec![
        ErlIfClause {
            condition: erl_exact_eq(suffix.clone(), ErlExpr::Binary("\"\"".to_string())),
            body: ErlExpr::Atom("true".to_string()),
        },
        ErlIfClause {
            condition: ErlExpr::Atom("true".to_string()),
            body: ErlExpr::Case {
                scrutinee: Box::new(search),
                clauses: vec![
                    ErlCaseClause {
                        pattern: ErlPattern::Atom("nomatch".to_string()),
                        guard: None,
                        body: ErlExpr::Atom("false".to_string()),
                    },
                    ErlCaseClause {
                        pattern: ErlPattern::Var("Found".to_string()),
                        guard: None,
                        body: erl_exact_eq(ErlExpr::Var("Found".to_string()), suffix),
                    },
                ],
            },
        },
    ]))
}

/// Lowers `core.string.length` to Erlang string length.
///
/// Inputs:
/// - `args`: one string expression.
///
/// Output:
/// - `Some(string:length(value))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Delegates user-visible character length to Erlang's unicode-aware string
///   length operation.
pub(super) fn lower_core_string_length(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call("string", "length", exact_args(args, 1)?))
}

/// Lowers `core.string.byte_size` to Erlang UTF-8 byte-size logic.
///
/// Inputs:
/// - `args`: one string expression.
///
/// Output:
/// - `Some(erlang:byte_size(unicode:characters_to_binary(value)))` when arity
///   is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Normalizes the string to a binary before measuring bytes so the backend
///   result matches the CoreIR UTF-8 byte-size contract.
pub(super) fn lower_core_string_byte_size(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "erlang",
        "byte_size",
        vec![erl_remote_call(
            "unicode",
            "characters_to_binary",
            vec![value],
        )],
    ))
}

/// Lowers one-argument string intrinsics to Erlang `string:<function>/1`.
///
/// Inputs:
/// - `function`: Erlang string module function name.
/// - `args`: one lowered string expression.
///
/// Output:
/// - `Some(string:function(value))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Centralizes the backend mapping for lowercase and uppercase operations.
pub(super) fn lower_core_string_unary_call(function: &str, args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call("string", function, exact_args(args, 1)?))
}

/// Lowers `core.string.trim` to Erlang string trim.
///
/// Inputs:
/// - `args`: one string expression.
///
/// Output:
/// - `Some(string:trim(value))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses Erlang's default trim mode for the backend implementation.
pub(super) fn lower_core_string_trim(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    Some(erl_remote_call("string", "trim", exact_args(args, 1)?))
}

/// Lowers directional string trim intrinsics to Erlang string trim modes.
///
/// Inputs:
/// - `mode`: Erlang trim mode atom name.
/// - `args`: one string expression.
///
/// Output:
/// - `Some(string:trim(value, mode))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Maps target-neutral trim-start and trim-end intrinsics to Erlang's
///   explicit `leading` and `trailing` mode atoms.
pub(super) fn lower_core_string_trim_mode(mode: &str, args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "string",
        "trim",
        vec![value, ErlExpr::Atom(mode.to_string())],
    ))
}

/// Lowers `core.string.replace` to Erlang global string replacement.
///
/// Inputs:
/// - `args`: value, pattern, and replacement string expressions.
///
/// Output:
/// - Erlang expression flattening the result of `string:replace/4`.
///
/// Transformation:
/// - Calls Erlang with the `all` mode and flattens the iolist result so the
///   backend representation is a string value.
pub(super) fn lower_core_string_replace(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, pattern, replacement] = exact_array_args(args)?;
    Some(erl_remote_call(
        "lists",
        "flatten",
        vec![erl_remote_call(
            "string",
            "replace",
            vec![
                value,
                pattern,
                replacement,
                ErlExpr::Atom("all".to_string()),
            ],
        )],
    ))
}

/// Lowers `core.string.split` to Erlang global string splitting.
///
/// Inputs:
/// - `args`: value and separator string expressions.
///
/// Output:
/// - `Some(string:split(value, separator, all))` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Uses Erlang's `all` split mode to implement the target-neutral list of
///   string fragments.
pub(super) fn lower_core_string_split(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, separator] = exact_array_args(args)?;
    Some(erl_remote_call(
        "string",
        "split",
        vec![value, separator, ErlExpr::Atom("all".to_string())],
    ))
}

/// Lowers `core.string.split_once` to a Terlan option shape.
///
/// Inputs:
/// - `args`: value and separator string expressions.
///
/// Output:
/// - Erlang case expression returning `some({left, right})` or `none`.
///
/// Transformation:
/// - Calls Erlang's leading split operation and translates the result list into
///   the CoreIR option runtime shape for the backend.
pub(super) fn lower_core_string_split_once(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value, separator] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(erl_remote_call(
            "string",
            "split",
            vec![value, separator, ErlExpr::Atom("leading".to_string())],
        )),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::List(vec![
                    ErlPattern::Var("Left".to_string()),
                    ErlPattern::Var("Right".to_string()),
                ]),
                guard: None,
                body: erl_some(ErlExpr::Tuple(vec![
                    ErlExpr::Var("Left".to_string()),
                    ErlExpr::Var("Right".to_string()),
                ])),
            },
            ErlCaseClause {
                pattern: ErlPattern::List(vec![ErlPattern::Wildcard]),
                guard: None,
                body: erl_none(),
            },
        ],
    })
}
