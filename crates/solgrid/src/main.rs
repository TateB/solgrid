mod commands;
mod output;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

fn long_version() -> &'static str {
    concat!(
        env!("CARGO_PKG_VERSION"),
        " (",
        env!("SOLGRID_GIT_HASH"),
        " ",
        env!("SOLGRID_BUILD_DATE"),
        ")"
    )
}

#[derive(Parser)]
#[command(
    name = "solgrid",
    version,
    long_version = long_version(),
    about = "The Solidity linter and formatter"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Files or directories to lint
    paths: Vec<PathBuf>,

    /// Path to solgrid.toml
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Output format: text, json, github, sarif
    #[arg(long, default_value = "text", global = true)]
    pub output_format: String,

    /// Apply safe auto-fixes
    #[arg(long, global = true)]
    pub fix: bool,

    /// Also apply suggestion-level fixes (requires --fix)
    #[arg(long, global = true)]
    pub unsafe_fixes: bool,

    /// Show diff instead of writing files
    #[arg(long, global = true)]
    pub diff: bool,

    /// Only show errors (suppress warnings and info)
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Read from stdin instead of files
    #[arg(long, global = true)]
    pub stdin: bool,

    /// Disable caching
    #[arg(long, global = true)]
    pub no_cache: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Lint files and report diagnostics (default)
    Check {
        /// Files or directories to lint
        paths: Vec<PathBuf>,
    },
    /// Lint files and apply safe auto-fixes
    Fix {
        /// Files or directories to fix
        paths: Vec<PathBuf>,
    },
    /// Format files
    Fmt {
        /// Files or directories to format
        paths: Vec<PathBuf>,
    },
    /// List all available rules
    ListRules,
    /// Show detailed documentation for a rule
    Explain {
        /// Rule ID (e.g. "security/tx-origin")
        rule: String,
    },
    /// Migrate configuration from another tool
    Migrate {
        /// Source tool to migrate from (e.g. "solhint")
        #[arg(long)]
        from: String,
    },
    /// Start the LSP server (for editor integration)
    Server,
}

fn main() {
    let cli = Cli::parse();

    let exit_code = match &cli.command {
        Some(Commands::Check { paths }) => {
            let paths = if paths.is_empty() { &cli.paths } else { paths };
            commands::check::run(paths, &cli)
        }
        Some(Commands::Fix { paths }) => {
            let paths = if paths.is_empty() { &cli.paths } else { paths };
            commands::fix::run(paths, &cli)
        }
        Some(Commands::Fmt { paths }) => {
            let paths = if paths.is_empty() { &cli.paths } else { paths };
            commands::fmt::run(paths, &cli)
        }
        Some(Commands::ListRules) => commands::list_rules::run(),
        Some(Commands::Explain { rule }) => commands::explain::run(rule),
        Some(Commands::Migrate { from }) => commands::migrate::run(from),
        Some(Commands::Server) => {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(solgrid_server::run_server());
            0
        }
        None => {
            // Default: check
            commands::check::run(&cli.paths, &cli)
        }
    };

    process::exit(exit_code);
}
