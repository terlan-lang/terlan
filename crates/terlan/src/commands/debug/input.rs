//! Interactive command input for the native VM debugger.

use std::io::{IsTerminal, Read, Write};

use crossterm::cursor::MoveToColumn;
use crossterm::event::{self as terminal_event, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{Clear, ClearType};
use crossterm::ExecutableCommand;

use crate::commands::terminal::RawModeGuard;

use super::script::{parse_debug_script_line, DebugScriptCommand};
use super::DebugCliError;

const DEBUG_PROMPT: &str = "debug> ";
const DEBUG_COMMANDS: &[&str] = &[
    "abort",
    "args",
    "break",
    "bt",
    "continue",
    "disable",
    "enable",
    "eval",
    "finish",
    "frame",
    "help",
    "list",
    "locals",
    "mailbox",
    "next",
    "pause",
    "print",
    "process",
    "processes",
    "quit",
    "remove",
    "resources",
    "restart",
    "restarts",
    "run",
    "step",
    "trace",
    "untrace",
    "use",
];

/// Stateful debugger command reader with terminal history and completion.
pub(super) struct DebugCommandReader {
    terminal: bool,
    history: Vec<String>,
    redirected: std::vec::IntoIter<(usize, String)>,
    next_line: usize,
    completions: Vec<String>,
}

impl DebugCommandReader {
    pub(super) fn open(completions: Vec<String>) -> Result<Self, DebugCliError> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let terminal = stdin.is_terminal() && stdout.is_terminal();
        let mut redirected = Vec::new();
        if !terminal {
            let mut input = String::new();
            stdin
                .lock()
                .read_to_string(&mut input)
                .map_err(|error| input_error(format!("failed to read debugger input: {error}")))?;
            redirected = input
                .lines()
                .enumerate()
                .map(|(index, line)| (index + 1, line.to_string()))
                .collect();
        }
        Ok(Self {
            terminal,
            history: Vec::new(),
            redirected: redirected.into_iter(),
            next_line: 1,
            completions,
        })
    }

    pub(super) fn next_command(&mut self) -> Result<Option<DebugScriptCommand>, DebugCliError> {
        loop {
            let (line_number, line) = if self.terminal {
                let Some(line) = read_terminal_line(&mut self.history, &self.completions)
                    .map_err(input_error)?
                else {
                    return Ok(None);
                };
                let line_number = self.next_line;
                self.next_line += 1;
                (line_number, line)
            } else {
                let Some(line) = self.redirected.next() else {
                    return Ok(None);
                };
                line
            };
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            return parse_debug_script_line(line_number, trimmed).map(Some);
        }
    }
}

fn read_terminal_line(
    history: &mut Vec<String>,
    completions: &[String],
) -> Result<Option<String>, DebugCliError> {
    let _raw_mode = RawModeGuard::enable()
        .map_err(|error| format!("failed to enable debugger raw mode: {error}"))?;
    let mut stdout = std::io::stdout();
    print!("{DEBUG_PROMPT}");
    stdout
        .flush()
        .map_err(|error| format!("failed to flush debugger prompt: {error}"))?;

    let mut buffer = String::new();
    let mut cursor = 0usize;
    let mut history_index = history.len();
    let mut pending_entry = String::new();
    loop {
        match terminal_event::read()
            .map_err(|error| format!("failed to read debugger input: {error}"))?
        {
            Event::Key(key) => match key.code {
                KeyCode::Enter => {
                    print!("\r\n");
                    if !buffer.trim().is_empty() {
                        history.push(buffer.clone());
                    }
                    return Ok(Some(buffer));
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    print!("\r\n");
                    return Ok(None);
                }
                KeyCode::Char('d')
                    if key.modifiers.contains(KeyModifiers::CONTROL) && buffer.is_empty() =>
                {
                    print!("\r\n");
                    return Ok(None);
                }
                KeyCode::Char(ch) => {
                    buffer.insert(cursor, ch);
                    cursor += ch.len_utf8();
                    redraw_line(&buffer, cursor)?;
                }
                KeyCode::Backspace if cursor > 0 => {
                    let previous = buffer[..cursor]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(index, _)| index);
                    buffer.drain(previous..cursor);
                    cursor = previous;
                    redraw_line(&buffer, cursor)?;
                }
                KeyCode::Delete if cursor < buffer.len() => {
                    let next = buffer[cursor..]
                        .char_indices()
                        .nth(1)
                        .map_or(buffer.len(), |(index, _)| cursor + index);
                    buffer.drain(cursor..next);
                    redraw_line(&buffer, cursor)?;
                }
                KeyCode::Left if cursor > 0 => {
                    cursor = buffer[..cursor]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(index, _)| index);
                    redraw_line(&buffer, cursor)?;
                }
                KeyCode::Right if cursor < buffer.len() => {
                    cursor += buffer[cursor..].chars().next().map_or(0, char::len_utf8);
                    redraw_line(&buffer, cursor)?;
                }
                KeyCode::Home => {
                    cursor = 0;
                    redraw_line(&buffer, cursor)?;
                }
                KeyCode::End => {
                    cursor = buffer.len();
                    redraw_line(&buffer, cursor)?;
                }
                KeyCode::Up if !history.is_empty() => {
                    if history_index == history.len() {
                        pending_entry = buffer.clone();
                    }
                    history_index = history_index.saturating_sub(1);
                    buffer = history[history_index].clone();
                    cursor = buffer.len();
                    redraw_line(&buffer, cursor)?;
                }
                KeyCode::Down if history_index < history.len() => {
                    history_index += 1;
                    buffer = if history_index == history.len() {
                        pending_entry.clone()
                    } else {
                        history[history_index].clone()
                    };
                    cursor = buffer.len();
                    redraw_line(&buffer, cursor)?;
                }
                KeyCode::Tab => {
                    complete_input(&mut buffer, &mut cursor, completions);
                    redraw_line(&buffer, cursor)?;
                }
                _ => {}
            },
            Event::Paste(value) => {
                buffer.insert_str(cursor, &value);
                cursor += value.len();
                redraw_line(&buffer, cursor)?;
            }
            _ => {}
        }
    }
}

fn complete_input(buffer: &mut String, cursor: &mut usize, completions: &[String]) {
    let token_start = buffer[..*cursor]
        .rfind(char::is_whitespace)
        .map_or(0, |index| index + 1);
    let prefix = &buffer[token_start..*cursor];
    let mut matches = if token_start == 0 {
        DEBUG_COMMANDS
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>()
    } else {
        completions.to_vec()
    }
    .into_iter()
    .filter(|candidate| candidate.starts_with(prefix));
    let Some(first) = matches.next() else {
        return;
    };
    if matches.next().is_none() {
        buffer.replace_range(token_start..*cursor, &first);
        *cursor = token_start + first.len();
    }
}

fn redraw_line(buffer: &str, cursor: usize) -> Result<(), DebugCliError> {
    let mut stdout = std::io::stdout();
    stdout
        .execute(MoveToColumn(0))
        .and_then(|stream| stream.execute(Clear(ClearType::CurrentLine)))
        .map_err(|error| format!("failed to redraw debugger line: {error}"))?;
    print!("{DEBUG_PROMPT}{buffer}");
    let column = DEBUG_PROMPT.chars().count() + buffer[..cursor].chars().count();
    stdout
        .execute(MoveToColumn(column as u16))
        .map_err(|error| format!("failed to position debugger cursor: {error}"))?;
    Ok(stdout
        .flush()
        .map_err(|error| format!("failed to flush debugger line: {error}"))?)
}

fn input_error(message: impl ToString) -> DebugCliError {
    DebugCliError {
        code: "debug_input_failed",
        message: message.to_string(),
    }
}
