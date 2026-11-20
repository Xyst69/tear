//! `tearw` runs the same commands as `tear` without opening a console window.
//! File Explorer's context menu launches it. Successful extraction is silent;
//! a password is asked for when needed, and anything else that needs the
//! user's attention is shown in a single message box.
#![windows_subsystem = "windows"]

mod cli;
mod password;

use std::io;
use std::path::Path;
use std::process::ExitCode;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MESSAGEBOX_STYLE,
    MessageBoxW,
};

fn show(text: &str, icon: MESSAGEBOX_STYLE) {
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide("Tear"));
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_SETFOREGROUND | icon,
        );
    }
}

fn file_name(path: Option<&Path>) -> String {
    path.and_then(Path::file_name).map_or_else(
        || "the archive".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn main() -> ExitCode {
    let matches = match cli::command().try_get_matches() {
        Ok(m) => m,
        Err(e) => {
            show(&e.to_string(), MB_ICONERROR);
            return ExitCode::from(cli::EXIT_BAD_INPUT);
        }
    };
    let action = matches.subcommand_name().unwrap_or_default();
    let name = file_name(cli::archive_path(&matches));
    let button = if action == "t" { "Test" } else { "Extract" };

    let mut out = Vec::new();
    let mut entered: Option<String> = None;
    let result = loop {
        out.clear();
        match cli::run(&matches, entered.as_deref(), &mut out) {
            Err(e) if tear_core::is_password_error(&e) => {
                let notice = entered
                    .is_some()
                    .then_some("That password didn't work. Try again.");
                match password::prompt(&name, button, notice) {
                    Some(p) => entered = Some(p),
                    // Cancelled: the user already knows why nothing happened.
                    None => return ExitCode::from(cli::EXIT_PASSWORD),
                }
            }
            other => break other,
        }
    };

    match result {
        Ok(outcome) if outcome.damaged.is_empty() => {
            if action == "t" {
                let summary = String::from_utf8_lossy(&out);
                show(
                    &format!("{name} is OK.\n\n{}", summary.trim()),
                    MB_ICONINFORMATION,
                );
            }
            ExitCode::SUCCESS
        }
        Ok(outcome) => {
            let shown: Vec<_> = outcome
                .damaged
                .iter()
                .take(10)
                .map(|d| format!("• {d}"))
                .collect();
            let more = outcome.damaged.len().saturating_sub(shown.len());
            let mut text = format!(
                "{} {} in {name} could not be recovered:\n\n{}",
                outcome.damaged.len(),
                if outcome.damaged.len() == 1 {
                    "file"
                } else {
                    "files"
                },
                shown.join("\n")
            );
            if more > 0 {
                text.push_str(&format!("\n…and {more} more."));
            }
            if action == "x" {
                text.push_str("\n\nEverything else was extracted.");
            }
            show(&text, MB_ICONWARNING);
            ExitCode::from(outcome.code)
        }
        Err(e) => {
            let verb = if action == "t" { "test" } else { "extract" };
            show(
                &format!("Couldn't {verb} {name}.\n\n{}", describe(&e)),
                MB_ICONERROR,
            );
            ExitCode::from(cli::error_code(&e))
        }
    }
}

fn describe(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::AlreadyExists => {
            "A file with the same name already exists in the destination, so nothing was replaced."
                .to_owned()
        }
        io::ErrorKind::StorageFull => format!("Not enough free disk space. {e}."),
        io::ErrorKind::NotFound => "The archive was moved or deleted.".to_owned(),
        _ => e.to_string(),
    }
}
