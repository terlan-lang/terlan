#[path = "ts_parser_adapter/declarations.rs"]
mod declarations;
#[path = "ts_parser_adapter/members.rs"]
mod members;
#[path = "ts_parser_adapter/types.rs"]
mod types;

pub(in crate::commands::bind) use declarations::*;
