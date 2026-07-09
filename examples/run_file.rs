use std::env;
use std::path::PathBuf;

use rustscript_embedded::{RunOutcome, render_value, run_source_file};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os();
    let binary = args.next().unwrap_or_default();
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: {} <program.rss>", PathBuf::from(binary).display());
        std::process::exit(64);
    };

    let RunOutcome::Halted { stack } = run_source_file(&path)?;
    if let Some(value) = stack.last() {
        eprintln!("final: {}", render_value(value));
    }

    Ok(())
}
