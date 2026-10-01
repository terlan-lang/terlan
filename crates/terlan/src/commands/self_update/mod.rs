//! GitHub release discovery and updates of the complete installed toolchain.

use std::io::{self, BufRead, IsTerminal, Write};
use std::process::ExitCode;

mod error;
mod install;
mod releases;
use error::UpdateError;

pub(crate) const USAGE: &str =
    "terlc self-update [<version>|--version <version>|--list|--interactive]";

#[derive(Debug, PartialEq, Eq)]
enum Options {
    Latest,
    Version(String),
    List,
    Interactive,
}

/// Parses command-local arguments before performing any network or disk work.
fn parse_args(args: &[String]) -> Result<Options, UpdateError> {
    match args {
        [] => Ok(Options::Latest),
        [arg] if arg == "latest" => Ok(Options::Latest),
        [arg] if arg == "--list" => Ok(Options::List),
        [arg] if arg == "--interactive" => Ok(Options::Interactive),
        [flag, version] if flag == "--version" => parse_version(version),
        [version] if !version.starts_with('-') => parse_version(version),
        _ => Err(format!("usage: {USAGE}").into()),
    }
}

/// Normalizes a user version to a release tag, rejecting paths and shell input.
fn parse_version(value: &str) -> Result<Options, UpdateError> {
    if value == "latest" {
        return Ok(Options::Latest);
    }
    let tag = format!("v{}", value.strip_prefix('v').unwrap_or(value));
    releases::version(&tag)?;
    Ok(Options::Version(tag))
}

/// Executes release listing, selection, or installation with CLI exit codes.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match execute(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("self-update: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Resolves the requested release and hands installation to the bundled installer.
fn execute(options: Options) -> Result<(), UpdateError> {
    if options == Options::Interactive && !io::stdin().is_terminal() {
        return Err("--interactive requires a terminal; use --list or specify a version".into());
    }
    let artifact = releases::platform_artifact(std::env::consts::OS, std::env::consts::ARCH)?;
    let client = releases::Client::github();
    let selected = match options {
        Options::Latest => client.latest(&artifact)?,
        Options::Version(tag) => client.by_tag(&tag, &artifact)?,
        Options::List | Options::Interactive => {
            let available = client.list(&artifact)?;
            // A repository may publish only prereleases, or omit this platform
            // from its latest release. Listing still works in either case.
            let latest = match client.latest(&artifact) {
                Ok(release) => Some(release),
                Err(error) => {
                    eprintln!("Latest stable release unavailable: {error}");
                    None
                }
            };
            print_releases(
                &available,
                latest.as_ref().map(|release| release.tag_name.as_str()),
            );
            if options == Options::List {
                return Ok(());
            }
            let selected = prompt_selection(
                &available,
                latest.as_ref(),
                &mut io::stdin().lock(),
                &mut io::stdout().lock(),
            )?;
            match selected {
                Some(release) => release,
                None => return Ok(()),
            }
        }
    };
    if selected.tag_name.strip_prefix('v') == Some(env!("CARGO_PKG_VERSION")) {
        println!("terlc {} is already installed.", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    install::run(&selected.tag_name)
}

/// Prints numbered compatible releases with stable, prerelease, and current labels.
fn print_releases(available: &[releases::Release], latest: Option<&str>) {
    println!(
        "Available Terlan releases ({} / {}):",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for (index, release) in available.iter().enumerate() {
        let mut labels = Vec::new();
        if latest == Some(release.tag_name.as_str()) {
            labels.push("latest");
        }
        if release.is_prerelease() {
            labels.push("prerelease");
        }
        if release.tag_name.strip_prefix('v') == Some(env!("CARGO_PKG_VERSION")) {
            labels.push("installed");
        }
        let suffix = if labels.is_empty() {
            String::new()
        } else {
            format!(" ({})", labels.join(", "))
        };
        println!("  {}. {}{}", index + 1, release.tag_name, suffix);
    }
}

/// Reads a number or version, defaulting Enter to latest and cancelling on EOF/q.
fn prompt_selection(
    available: &[releases::Release],
    latest: Option<&releases::Release>,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<Option<releases::Release>, UpdateError> {
    loop {
        write!(
            output,
            "Select a number or version [Enter: latest, q: cancel]: "
        )
        .and_then(|()| output.flush())
        .map_err(|error| error.to_string())?;
        let mut line = String::new();
        if input
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(None);
        }
        let value = line.trim();
        if matches!(value, "q" | "quit") {
            return Ok(None);
        }
        let selected = if value.is_empty() || value == "latest" {
            latest
        } else if let Ok(number) = value.parse::<usize>() {
            number.checked_sub(1).and_then(|index| available.get(index))
        } else {
            let tag = format!("v{}", value.strip_prefix('v').unwrap_or(value));
            available.iter().find(|release| release.tag_name == tag)
        };
        if let Some(release) = selected {
            return Ok(Some(release.clone()));
        }
        writeln!(output, "Choose an available release, or q to cancel.")
            .map_err(|error| error.to_string())?;
    }
}

#[cfg(test)]
#[path = "self_update_test.rs"]
mod tests;
