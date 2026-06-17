//! Collection and core task primitive lowering for CoreIR Erlang emission.
//!
//! Inputs:
//! - Already-lowered Erlang argument expressions for collection and task
//!   primitive intrinsics.
//!
//! Outputs:
//! - Erlang expressions implementing List, Iterator, Map, Set, and simple
//!   completed Task operations.
//!
//! Transformations:
//! - Maps backend-neutral collection APIs onto compiler-owned BEAM backing
//!   shapes while preserving Terlan option, result, and mutable-receiver
//!   contracts at the source boundary.

use super::super::erl::{ErlCaseClause, ErlExpr, ErlPattern};
use super::{
    erl_exact_eq, erl_none, erl_remote_call, erl_result_ok, erl_some, exact_args, exact_array_args,
};

/// Lowers `core.list.new` to an empty Erlang list.
///
/// Inputs:
/// - `args`: intrinsic arguments, expected to be empty.
///
/// Output:
/// - Empty Erlang list expression.
///
/// Transformation:
/// - Hides the BEAM list representation behind the backend-neutral collection
///   intrinsic boundary.
pub(super) fn lower_core_list_new(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 0)?;
    Some(ErlExpr::List(vec![]))
}

/// Lowers `core.list.is_empty` to an empty-list comparison.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Compares the receiver with the canonical empty Erlang list while keeping
///   Terlan source independent from that representation.
pub(super) fn lower_core_list_is_empty(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list] = exact_array_args(args)?;
    Some(erl_exact_eq(list, ErlExpr::List(vec![])))
}

/// Lowers `core.list.length` to `length/1`.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - Integer Erlang expression for the number of list values.
///
/// Transformation:
/// - Delegates to the BEAM list runtime while preserving the portable Terlan
///   `List.length()` API.
pub(super) fn lower_core_list_length(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list] = exact_array_args(args)?;
    Some(erl_remote_call("erlang", "length", vec![list]))
}

/// Lowers `core.list.first` to a Terlan `Option` shape.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - `{some, Head}` when the list is non-empty, otherwise `none`.
///
/// Transformation:
/// - Converts BEAM list pattern matching into Terlan's option runtime shape.
pub(super) fn lower_core_list_first(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(list),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::ListCons(
                    Box::new(ErlPattern::Var("Head".to_string())),
                    Box::new(ErlPattern::Wildcard),
                ),
                guard: None,
                body: erl_some(ErlExpr::Var("Head".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::List(vec![]),
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.list.iterator` to the selected BEAM iterator state.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - The same Erlang list expression used as immutable traversal state.
///
/// Transformation:
/// - Starts portable traversal by reusing the BEAM list representation behind
///   the opaque `Iterator[T]` abstraction.
pub(super) fn lower_core_list_iterator(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list] = exact_array_args(args)?;
    Some(list)
}

/// Lowers `core.iterator.next` to one immutable state-passing traversal step.
///
/// Inputs:
/// - `args`: one iterator state expression.
///
/// Output:
/// - Terlan option runtime shape: `none` for exhausted traversal, or
///   `{some, {CompilerValue, CompilerNextIterator}}` for one yielded value and
///   the next state.
///
/// Transformation:
/// - Pattern matches the backend iterator representation and returns the next
///   state explicitly instead of mutating the current iterator.
pub(super) fn lower_core_iterator_next(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [iterator] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(iterator),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::ListCons(
                    Box::new(ErlPattern::Var("_TerlanIteratorValue".to_string())),
                    Box::new(ErlPattern::Var("_TerlanNextIterator".to_string())),
                ),
                guard: None,
                body: erl_some(ErlExpr::Tuple(vec![
                    ErlExpr::Var("_TerlanIteratorValue".to_string()),
                    ErlExpr::Var("_TerlanNextIterator".to_string()),
                ])),
            },
            ErlCaseClause {
                pattern: ErlPattern::List(vec![]),
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.list.push` to an append-at-end list update.
///
/// Inputs:
/// - `args`: list and value expressions.
///
/// Output:
/// - Updated list expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
pub(super) fn lower_core_list_push(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [list, value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "lists",
        "append",
        vec![list, ErlExpr::List(vec![value])],
    ))
}

/// Lowers `core.list.clear` to an empty Erlang list.
///
/// Inputs:
/// - `args`: one list expression.
///
/// Output:
/// - Empty list expression.
///
/// Transformation:
/// - Ignores the old receiver and returns the canonical empty collection
///   representation for the BEAM backend.
pub(super) fn lower_core_list_clear(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 1)?;
    Some(ErlExpr::List(vec![]))
}

/// Lowers `core.map.new` to an empty Erlang map.
///
/// Inputs:
/// - `args`: intrinsic arguments, expected to be empty.
///
/// Output:
/// - Empty Erlang map expression.
///
/// Transformation:
/// - Hides the BEAM map representation behind the backend-neutral collection
///   intrinsic boundary.
pub(super) fn lower_core_map_new(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 0)?;
    Some(ErlExpr::Map(vec![]))
}

/// Lowers `core.map.is_empty` to a BEAM map-size comparison.
///
/// Inputs:
/// - `args`: one map expression.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Compares `maps:size(Map)` with zero without exposing that implementation
///   choice to Terlan source.
pub(super) fn lower_core_map_is_empty(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map] = exact_array_args(args)?;
    Some(erl_exact_eq(
        erl_remote_call("maps", "size", vec![map]),
        ErlExpr::Int(0),
    ))
}

/// Lowers `core.map.size` to `maps:size/1`.
///
/// Inputs:
/// - `args`: one map expression.
///
/// Output:
/// - Integer Erlang expression for the number of key-value entries.
///
/// Transformation:
/// - Delegates to the BEAM map runtime while preserving the portable Terlan
///   `Map.size()` API.
pub(super) fn lower_core_map_size(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "size", vec![map]))
}

/// Lowers `core.map.get` to a Terlan `Option` shape around `maps:find/2`.
///
/// Inputs:
/// - `args`: map and key expressions.
///
/// Output:
/// - `{some, Value}` when the key exists, otherwise `none`.
///
/// Transformation:
/// - Converts BEAM's `{ok, Value} | error` result into Terlan's option
///   runtime shape.
pub(super) fn lower_core_map_get(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map, key] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(erl_remote_call("maps", "find", vec![key, map])),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Tuple(vec![
                    ErlPattern::Atom("ok".to_string()),
                    ErlPattern::Var("Value".to_string()),
                ]),
                guard: None,
                body: erl_some(ErlExpr::Var("Value".to_string())),
            },
            ErlCaseClause {
                pattern: ErlPattern::Atom("error".to_string()),
                guard: None,
                body: erl_none(),
            },
        ],
    })
}

/// Lowers `core.map.contains_key` to `maps:is_key/2`.
///
/// Inputs:
/// - `args`: map and key expressions.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Delegates key-presence checks to the BEAM map runtime.
pub(super) fn lower_core_map_contains_key(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map, key] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "is_key", vec![key, map]))
}

/// Lowers `core.map.iterator` to a BEAM list of key-value tuple pairs.
///
/// Inputs:
/// - `args`: one map expression.
///
/// Output:
/// - Erlang list expression containing `{Key, Value}` tuples.
///
/// Transformation:
/// - Converts the compiler-owned map backing shape to the common iterator-list
///   state consumed by `core.iterator.next` and std `Enumerable` bridges.
pub(super) fn lower_core_map_iterator(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "to_list", vec![map]))
}

/// Lowers `core.map.put` to `maps:put/3`.
///
/// Inputs:
/// - `args`: map, key, and value expressions.
///
/// Output:
/// - Updated map expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
pub(super) fn lower_core_map_put(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map, key, value] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "put", vec![key, value, map]))
}

/// Lowers `core.map.remove` to `maps:remove/2`.
///
/// Inputs:
/// - `args`: map and key expressions.
///
/// Output:
/// - Updated map expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
pub(super) fn lower_core_map_remove(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [map, key] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "remove", vec![key, map]))
}

/// Lowers `core.map.clear` to an empty Erlang map.
///
/// Inputs:
/// - `args`: one map expression.
///
/// Output:
/// - Empty map expression.
///
/// Transformation:
/// - Ignores the old receiver and returns the canonical empty collection
///   representation for the BEAM backend.
pub(super) fn lower_core_map_clear(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 1)?;
    Some(ErlExpr::Map(vec![]))
}

/// Lowers `core.task.done` to the Erlang completed-task backing shape.
///
/// Inputs:
/// - `args`: one lowered Erlang value expression.
///
/// Output:
/// - Backend-private completed task handle.
///
/// Transformation:
/// - Wraps the value in a tagged tuple owned by the compiler backend. Terlan
///   source observes only the opaque `Task[T]` type and the `result()` method.
pub(super) fn lower_core_task_done(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [value] = exact_array_args(args)?;
    Some(ErlExpr::Tuple(vec![
        ErlExpr::Atom("task_done".to_string()),
        value,
    ]))
}

/// Lowers `core.task.result` to the public `Result` runtime shape.
///
/// Inputs:
/// - `args`: one lowered Erlang task-handle expression.
///
/// Output:
/// - `Ok(value)` for a completed backend task handle.
///
/// Transformation:
/// - Pattern matches the backend-private Task representation and converts it
///   into Terlan's existing `Result` tagged tuple without exposing scheduler or
///   BEAM details to source code.
pub(super) fn lower_core_task_result(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [task] = exact_array_args(args)?;
    Some(ErlExpr::Case {
        scrutinee: Box::new(task),
        clauses: vec![ErlCaseClause {
            pattern: ErlPattern::Tuple(vec![
                ErlPattern::Atom("task_done".to_string()),
                ErlPattern::Var("Value".to_string()),
            ]),
            guard: None,
            body: erl_result_ok(ErlExpr::Var("Value".to_string())),
        }],
    })
}

/// Lowers `core.set.new` to the BEAM set backing shape.
///
/// Inputs:
/// - `args`: intrinsic arguments, expected to be empty.
///
/// Output:
/// - Empty compiler-owned set expression.
///
/// Transformation:
/// - Represents the first BEAM set shape as an Erlang map from value to `true`
///   while preserving the backend-neutral Terlan `Set[T]` contract.
pub(super) fn lower_core_set_new(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 0)?;
    Some(ErlExpr::Map(vec![]))
}

/// Lowers `core.set.is_empty` to a set-size comparison.
///
/// Inputs:
/// - `args`: one set expression.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Observes the compiler-owned map-backed set shape without exposing it to
///   source code.
pub(super) fn lower_core_set_is_empty(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set] = exact_array_args(args)?;
    Some(erl_exact_eq(
        erl_remote_call("maps", "size", vec![set]),
        ErlExpr::Int(0),
    ))
}

/// Lowers `core.set.size` to a map-size call over the backing shape.
///
/// Inputs:
/// - `args`: one set expression.
///
/// Output:
/// - Integer Erlang expression for the number of unique values.
///
/// Transformation:
/// - Uses the BEAM backing map size as the portable set cardinality.
pub(super) fn lower_core_set_size(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "size", vec![set]))
}

/// Lowers `core.set.contains` to a key-presence check.
///
/// Inputs:
/// - `args`: set and value expressions.
///
/// Output:
/// - Boolean Erlang expression.
///
/// Transformation:
/// - Treats set membership as key presence in the compiler-owned BEAM backing
///   shape.
pub(super) fn lower_core_set_contains(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set, value] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "is_key", vec![value, set]))
}

/// Lowers `core.set.iterator` to a BEAM list of set values.
///
/// Inputs:
/// - `args`: one set expression.
///
/// Output:
/// - Erlang list expression containing each unique set value.
///
/// Transformation:
/// - Extracts keys from the compiler-owned map-backed set shape so Set
///   traversal shares the same iterator-list state as List and Map traversal.
pub(super) fn lower_core_set_iterator(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "keys", vec![set]))
}

/// Lowers `core.set.add` to a map insertion into the backing shape.
///
/// Inputs:
/// - `args`: set and value expressions.
///
/// Output:
/// - Updated set expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
pub(super) fn lower_core_set_add(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set, value] = exact_array_args(args)?;
    Some(erl_remote_call(
        "maps",
        "put",
        vec![value, ErlExpr::Atom("true".to_string()), set],
    ))
}

/// Lowers `core.set.remove` to a map removal from the backing shape.
///
/// Inputs:
/// - `args`: set and value expressions.
///
/// Output:
/// - Updated set expression.
///
/// Transformation:
/// - Returns the updated receiver value expected by the command-style mutable
///   receiver ABI.
pub(super) fn lower_core_set_remove(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [set, value] = exact_array_args(args)?;
    Some(erl_remote_call("maps", "remove", vec![value, set]))
}

/// Lowers `core.set.clear` to the empty BEAM set backing shape.
///
/// Inputs:
/// - `args`: one set expression.
///
/// Output:
/// - Empty compiler-owned set expression.
///
/// Transformation:
/// - Ignores the old receiver and returns the canonical empty collection
///   representation for the BEAM backend.
pub(super) fn lower_core_set_clear(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    exact_args(args, 1)?;
    Some(ErlExpr::Map(vec![]))
}
