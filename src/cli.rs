use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "exc",
    version,
    about = "A fast, lightweight terminal menu launcher",
    long_about = "exc is a fast, lightweight menu launcher for your terminal: a searchable, \
keyboard-driven command picker configured entirely from a TOML file. Use it as an everyday \
command hub, or drop a project-local config next to a repo to give it its own menu of tasks \
(deployments, git housekeeping, cleanup, etc.).",
    after_help = "EXAMPLES:\n    \
exc                    Launch the interactive picker\n    \
exc deploy-staging     Run the \"deploy-staging\" command directly\n    \
exc list --plain       List every command name, one per line\n    \
exc init               Write a starter config if you don't have one yet\n    \
exc man > exc.1        Generate a man page (see `exc man --help`)"
)]
pub struct Cli {
    /// Path to the config file (default: $XDG_CONFIG_HOME/exc/config.toml, or ~/.config/exc/config.toml)
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Start the picker on this profile instead of the first one in the config
    #[arg(long, global = true, value_name = "NAME")]
    pub profile: Option<String>,

    /// Built-in color palette to use, overriding [meta] theme in the config
    #[arg(long, global = true, value_name = "NAME")]
    pub theme: Option<String>,

    /// Disable all color output (equivalent to the NO_COLOR env var)
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Hide the system-info panel in the interactive picker
    #[arg(long, global = true)]
    pub no_sysinfo: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a command by name directly (no interactive picker)
    Run {
        /// Command name (or a unique substring), or a numeric id from `exc list`
        name: String,
    },
    /// List commands non-interactively in the same adaptive grid as the picker
    List {
        /// Only list commands from this profile
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,
        /// Print one command name per line instead of the numbered grid
        #[arg(long)]
        plain: bool,
    },
    /// Parse and lint the config file
    Validate {
        /// Treat warnings as failures (nonzero exit code)
        #[arg(long)]
        strict: bool,
        /// Output format for the validation report
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Print OS + hardware info
    Sysinfo,
    /// Write a starter config file at the resolved config path
    Init {
        /// Overwrite the config file if one already exists there
        #[arg(long)]
        force: bool,
    },
    /// Print a roff(7) man page for exc to stdout
    ///
    /// Typically piped straight into a man directory, e.g.:
    ///   exc man | sudo tee /usr/local/share/man/man1/exc.1 > /dev/null
    /// or viewed on the spot without installing it:
    ///   exc man | man -l -
    Man,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

const KNOWN_SUBCOMMANDS: &[&str] = &["run", "list", "validate", "sysinfo", "init", "man", "help"];
/// Global flags that consume a separate following token as their value
/// (space form, e.g. `--config path.toml`) — needed so the shorthand
/// detector below can skip past `--flag value` pairs without mistaking the
/// value for the shorthand's target. `--flag=value` (equals form) and
/// no-value flags like `--no-color` are single self-contained tokens and
/// don't need special-casing.
const VALUE_FLAGS: &[&str] = &["--config", "--profile", "--theme"];

/// Parse argv, supporting the bare `exc <name>` shorthand for `exc run
/// <name>` even when global flags (`--config`, `--profile`, `--theme`,
/// `--no-color`) precede it, e.g. `exc --config path.toml 5`. Tries a
/// normal parse first; if that fails, finds the first token that isn't a
/// global flag (or a global flag's value) or a known subcommand, and
/// retries with `run` inserted right before it. This preserves normal
/// --help/--version/error behavior otherwise.
pub fn parse_args() -> Cli {
    let raw: Vec<String> = std::env::args().collect();
    match Cli::try_parse_from(&raw) {
        Ok(cli) => cli,
        Err(e) => {
            if let Some(insert_at) = implicit_run_insertion_point(&raw) {
                let mut retry = raw.clone();
                retry.insert(insert_at, "run".to_string());
                if let Ok(cli) = Cli::try_parse_from(&retry) {
                    return cli;
                }
            }
            e.exit();
        }
    }
}

/// Scans past any leading global flags (and their values) to find the
/// index of the first remaining token; returns that index if the token
/// looks like an implicit-run target (not itself a flag, not a known
/// subcommand), so `run` can be inserted right before it.
fn implicit_run_insertion_point(raw: &[String]) -> Option<usize> {
    let mut i = 1;
    while i < raw.len() {
        let tok = raw[i].as_str();
        if VALUE_FLAGS.contains(&tok) {
            i += 2; // skip the flag and its separate value token
            continue;
        }
        if tok.starts_with('-') {
            i += 1; // a self-contained flag: --no-color, or --flag=value
            continue;
        }
        return if KNOWN_SUBCOMMANDS.contains(&tok) { None } else { Some(i) };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        std::iter::once("exc".to_string()).chain(v.iter().map(|s| s.to_string())).collect()
    }

    #[test]
    fn bare_id_at_front_gets_run_inserted() {
        assert_eq!(implicit_run_insertion_point(&args(&["73"])), Some(1));
    }

    #[test]
    fn bare_id_after_a_value_flag_still_gets_run_inserted() {
        assert_eq!(implicit_run_insertion_point(&args(&["--config", "c.toml", "73"])), Some(3));
    }

    #[test]
    fn bare_id_after_equals_form_flag_still_gets_run_inserted() {
        assert_eq!(implicit_run_insertion_point(&args(&["--config=c.toml", "73"])), Some(2));
    }

    #[test]
    fn bare_id_after_no_color_still_gets_run_inserted() {
        assert_eq!(implicit_run_insertion_point(&args(&["--no-color", "73"])), Some(2));
    }

    #[test]
    fn known_subcommand_is_left_alone() {
        assert_eq!(implicit_run_insertion_point(&args(&["--config", "c.toml", "list"])), None);
    }

    #[test]
    fn no_positional_token_at_all() {
        assert_eq!(implicit_run_insertion_point(&args(&["--no-color"])), None);
    }
}
