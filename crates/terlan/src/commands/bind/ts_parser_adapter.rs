#[path = "ts_parser_adapter/classes.rs"]
mod classes;
#[path = "ts_parser_adapter/declarations.rs"]
mod declarations;
#[path = "ts_parser_adapter/graph.rs"]
mod graph;
#[path = "ts_parser_adapter/members.rs"]
mod members;
#[path = "ts_parser_adapter/types.rs"]
mod types;

pub(in crate::commands::bind) use declarations::*;
pub(in crate::commands::bind) use graph::{load_declaration_graph, specialize_this_type};
