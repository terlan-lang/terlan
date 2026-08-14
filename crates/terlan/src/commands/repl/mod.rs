mod bindings;
mod evaluation;
mod event;
mod help;
mod interactive;
mod source;

#[cfg(test)]
#[path = "repl_aot_test.rs"]
#[cfg(test)]
mod repl_aot_test;
#[cfg(test)]
#[path = "repl_test.rs"]
#[cfg(test)]
mod repl_test;

#[cfg(test)]
use bindings::ReplValueBinding;
pub(crate) use evaluation::evaluate_repl_prompt_inputs;
#[cfg(test)]
use evaluation::{
    repl_generation_run_name, run_repl_expression_in_session_with_output, ReplCompilerService,
    ReplExpressionRequest,
};
#[cfg(test)]
use interactive::parse_repl_command_args;
pub(crate) use interactive::run;
#[cfg(test)]
pub(crate) use interactive::run_with_input;
