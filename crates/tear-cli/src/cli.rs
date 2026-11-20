//! Command definitions shared by `tear` (console) and `tearw` (no window).
use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};
use tear_core::{Archive, ArchiveOptions, ExtractMode};

pub const EXIT_PARTIAL: u8 = 1;
pub const EXIT_BAD_INPUT: u8 = 2;
pub const EXIT_PASSWORD: u8 = 3;

fn archive_command(name: &'static str, description: &'static str) -> Command {
    Command::new(name)
        .about(description)
        .arg(
            Arg::new("archive")
                .value_parser(value_parser!(PathBuf))
                .required(true),
        )
        .arg(
            Arg::new("password")
                .short('p')
                .long("password")
                .value_name("PASSWORD"),
        )
        .arg(
            Arg::new("large")
                .long("allow-large")
                .action(ArgAction::SetTrue)
                .help("Allow reviewed archives larger than 10 GiB with an expansion ratio above 1000:1"),
        )
}

pub fn command() -> Command {
    let json = Arg::new("json").long("json").action(ArgAction::SetTrue);
    Command::new("tear")
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            archive_command("x", "Extract an archive (smart folder by default)")
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_parser(value_parser!(PathBuf))
                        .value_name("DIR")
                        .help("Destination folder (default: the archive's folder)"),
                )
                .arg(
                    Arg::new("smart")
                        .long("smart")
                        .action(ArgAction::SetTrue)
                        .help("Keep a single top-level folder, otherwise create one (default)"),
                )
                .arg(
                    Arg::new("folder")
                        .long("folder")
                        .action(ArgAction::SetTrue)
                        .help("Always create a new folder named after the archive"),
                )
                .arg(
                    Arg::new("here")
                        .long("here")
                        .action(ArgAction::SetTrue)
                        .help("Write directly in the destination; existing files are protected"),
                )
                .group(clap::ArgGroup::new("mode").args(["smart", "folder", "here"])),
        )
        .subcommand(archive_command("l", "List archive members").arg(json.clone()))
        .subcommand(archive_command("t", "Decode and verify the archive").arg(json))
}

/// What a finished command produced, for callers that present results themselves.
pub struct Outcome {
    pub code: u8,
    /// Entries that could not be recovered, one line each.
    pub damaged: Vec<String>,
}

pub fn archive_path(args: &ArgMatches) -> Option<&Path> {
    args.subcommand()
        .and_then(|(_, sub)| sub.get_one::<PathBuf>("archive"))
        .map(PathBuf::as_path)
}

/// Runs the parsed command. `password` overrides `--password`, for callers that
/// ask the user interactively.
pub fn run(
    matches: &ArgMatches,
    password: Option<&str>,
    out: &mut dyn Write,
) -> io::Result<Outcome> {
    let Some((action, args)) = matches.subcommand() else {
        return Ok(Outcome {
            code: 0,
            damaged: Vec::new(),
        });
    };
    let path = args
        .get_one::<PathBuf>("archive")
        .ok_or_else(|| io::Error::other("Missing archive path"))?;
    let options = ArchiveOptions {
        password: password
            .map(str::to_owned)
            .or_else(|| args.get_one::<String>("password").cloned()),
        allow_large: args.get_flag("large"),
    };
    let archive = Archive::open(path, options)?;
    let mut damaged = Vec::new();
    match action {
        "l" if args.get_flag("json") => {
            let entries: Vec<_> = archive
                .entries()
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "path": e.path,
                        "size": e.size,
                        "directory": e.directory,
                        "encrypted": e.encrypted,
                    })
                })
                .collect();
            serde_json::to_writer(
                &mut *out,
                &serde_json::json!({"format": format!("{:?}", archive.format()), "entries": entries}),
            )?;
            writeln!(out)?;
        }
        "l" => {
            for e in archive.entries() {
                let size = e.size.map_or_else(|| "?".to_owned(), |n| n.to_string());
                let slash = if e.directory { "/" } else { "" };
                writeln!(out, "{size:>12}  {}{slash}", e.path.display())?;
            }
        }
        "t" => {
            let r = archive.test()?;
            damaged = r.damaged;
            if args.get_flag("json") {
                let report = serde_json::json!({"files": r.files, "bytes": r.bytes, "ok": damaged.is_empty()});
                writeln!(out, "{report}")?;
            } else {
                writeln!(
                    out,
                    "Verified {} ({}).",
                    count(r.files, "file"),
                    human_size(r.bytes)
                )?;
            }
        }
        "x" => {
            let dest = args
                .get_one::<PathBuf>("output")
                .map(PathBuf::as_path)
                .or_else(|| path.parent().filter(|p| !p.as_os_str().is_empty()))
                .unwrap_or_else(|| Path::new("."));
            let mode = if args.get_flag("here") {
                ExtractMode::Here
            } else if args.get_flag("folder") {
                ExtractMode::Folder
            } else {
                ExtractMode::Smart
            };
            let r = archive.extract(dest, mode)?;
            damaged = r.damaged;
            if let Some(root) = r.destination {
                writeln!(
                    out,
                    "Extracted {} ({}) to {}",
                    count(r.files, "file"),
                    human_size(r.bytes),
                    display_path(&root)
                )?;
            }
        }
        _ => return Err(io::Error::other("Unrecognized command")),
    }
    Ok(Outcome {
        code: if damaged.is_empty() { 0 } else { EXIT_PARTIAL },
        damaged,
    })
}

pub fn error_code(e: &io::Error) -> u8 {
    if tear_core::is_password_error(e) {
        EXIT_PASSWORD
    } else {
        EXIT_BAD_INPUT
    }
}

fn count(n: u64, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Shows paths the way Explorer does, without the `\\?\` long-path prefix.
fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} bytes");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
