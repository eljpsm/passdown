//! passdown: a really, really simple Markdown formatter with as few knobs as
//! possible.
//!
//! Pipeline: `cli` parses arguments and `app::run` drives the rest. It walks
//! up for config (`config`), discovers files (`discover`), formats each one
//! (`format`), and commits fixes atomically (`file_io`).

use std::process::ExitCode;

use clap::Parser;

mod app;
mod cli;
mod config;
mod diagnostics;
mod discover;
mod file_io;
mod format;
mod normalize;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    app::run(cli).into()
}
