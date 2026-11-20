// Shared with `tearw`, which uses a few helpers this binary doesn't.
#[allow(dead_code)]
mod cli;

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let matches = cli::command().get_matches();
    let mut out = io::BufWriter::new(io::stdout().lock());
    let result = cli::run(&matches, None, &mut out).and_then(|outcome| {
        out.flush()?;
        Ok(outcome)
    });
    match result {
        Ok(outcome) => {
            for reason in &outcome.damaged {
                eprintln!("{reason}");
            }
            ExitCode::from(outcome.code)
        }
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tear: {e}");
            ExitCode::from(cli::error_code(&e))
        }
    }
}
