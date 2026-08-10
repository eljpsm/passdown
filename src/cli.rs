use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// A really, really simple Markdown formatter with as few knobs as possible.
#[derive(Debug, Parser)]
#[command(name = "passdown", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Format files in place, fixing any issues
    Fix {
        /// Files or directories to format
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,
    },
    /// Check formatting; exit nonzero if there are any issues (for CI)
    Check {
        /// Files or directories to check
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcommands_default_to_the_current_directory() {
        for subcommand in ["check", "fix"] {
            let cli = Cli::try_parse_from(["passdown", subcommand]).unwrap();
            let paths = match cli.command {
                Command::Fix { paths } | Command::Check { paths } => paths,
            };
            assert_eq!(paths, [PathBuf::from(".")]);
        }
    }

    #[test]
    fn explicit_paths_replace_the_default() {
        let cli = Cli::try_parse_from(["passdown", "check", "docs", "README.md"]).unwrap();
        let Command::Check { paths } = cli.command else {
            panic!("expected check command");
        };
        assert_eq!(paths, [PathBuf::from("docs"), PathBuf::from("README.md")]);
    }
}
