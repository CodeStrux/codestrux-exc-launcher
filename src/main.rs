mod cli;
mod config;
mod error;
mod exec;
mod hostinfo;
mod layout;
mod picker;
mod prompt;
mod theme;

use std::io::{self, IsTerminal, Write};
use std::path::Path;

use anyhow::Result;
use clap::CommandFactory;
use config::validate::Severity;
use config::Config;
use error::ConfigError;

use cli::{Cli, Commands, OutputFormat};

const STARTER_CONFIG: &str = include_str!("../config/starter.toml");

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
        default_hook(info);
    }));
}

fn main() {
    install_panic_hook();
    let cli = cli::parse_args();
    let code = run(cli).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        1
    });
    std::process::exit(code);
}

fn run(cli: Cli) -> Result<i32> {
    match &cli.command {
        Some(Commands::Init { force }) => return cmd_init(*force),
        Some(Commands::Validate { strict, format }) => {
            let path = cli.config.clone().unwrap_or_else(config::default_config_path);
            return cmd_validate(&path, *strict, *format);
        }
        // sysinfo works without a config file (config only supplies an optional [meta] title override).
        Some(Commands::Sysinfo) => return cmd_sysinfo(&cli),
        Some(Commands::Man) => return cmd_man(),
        _ => {}
    }

    let path = cli.config.clone().unwrap_or_else(config::default_config_path);
    let cfg = match config::load_config(&path) {
        Ok(cfg) => cfg,
        // Only the bare interactive picker (no subcommand) offers to create a
        // starter config on the spot; `run`/`list` keep failing fast since
        // they're the forms used from scripts and shouldn't have a side effect
        // sprung on them.
        Err(ConfigError::NotFound(_)) if cli.command.is_none() => match prompt_create_config(&path)? {
            ConfigPromptOutcome::Created(cfg) => *cfg,
            ConfigPromptOutcome::DeclinedByUser => return Ok(0),
            ConfigPromptOutcome::NonInteractive => return Ok(2),
        },
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(e.exit_code());
        }
    };

    match cli.command {
        Some(Commands::Run { name }) => cmd_run(&cfg, &name),
        Some(Commands::List { profile, plain }) => cmd_list(&cfg, profile.as_deref(), plain),
        Some(Commands::Init { .. }) | Some(Commands::Validate { .. }) | Some(Commands::Sysinfo) | Some(Commands::Man) => {
            unreachable!()
        }
        None => cmd_picker(&cfg, &cli),
    }
}

/// Renders a roff(7) man page for the whole CLI (top-level flags plus every
/// subcommand) straight from the clap definition in `cli.rs`, so it can never
/// drift out of sync with `--help`. `cargo install` only places the compiled
/// binary on $PATH, not the man page itself, so this is the mechanism package
/// maintainers (or you, locally) use to actually get one installed:
/// `exc man | sudo tee /usr/local/share/man/man1/exc.1 > /dev/null`.
fn cmd_man() -> Result<i32> {
    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buf = Vec::new();
    man.render(&mut buf)?;
    io::stdout().write_all(&buf)?;
    Ok(0)
}

enum ConfigPromptOutcome {
    Created(Box<Config>),
    DeclinedByUser,
    NonInteractive,
}

/// Called the first time `exc` is run with no config at the resolved path:
/// offers to write the starter config right there instead of just erroring.
/// Declines (without prompting) when stdin isn't a terminal, so piping into
/// `exc` or invoking it from a non-interactive context never blocks on input
/// or writes a file as a side effect.
fn prompt_create_config(path: &Path) -> Result<ConfigPromptOutcome> {
    if !io::stdin().is_terminal() {
        eprintln!(
            "error: no config file found at {}\nhint: run `exc init` to create a starter config there, or pass --config <path>",
            path.display()
        );
        return Ok(ConfigPromptOutcome::NonInteractive);
    }

    print!("No config file found at {}.\nCreate a starter config there now? [Y/n] ", path.display());
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_ascii_lowercase();

    if !answer.is_empty() && answer != "y" && answer != "yes" {
        println!("ok, not creating a config. Run `exc init` whenever you're ready.");
        return Ok(ConfigPromptOutcome::DeclinedByUser);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, STARTER_CONFIG)?;
    println!("wrote starter config to {}\n", path.display());
    Ok(ConfigPromptOutcome::Created(Box::new(config::load_config(path)?)))
}

fn cmd_init(force: bool) -> Result<i32> {
    let path = config::default_config_path();
    if path.exists() && !force {
        eprintln!(
            "error: config already exists at {}\nhint: pass --force to overwrite",
            path.display()
        );
        return Ok(1);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, STARTER_CONFIG)?;
    println!("wrote starter config to {}", path.display());
    Ok(0)
}

fn cmd_validate(path: &Path, strict: bool, format: OutputFormat) -> Result<i32> {
    let cfg = match config::load_config(path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(e.exit_code());
        }
    };

    let config_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let issues = config::validate::validate_config(&cfg, config_dir);
    let errors = issues.iter().filter(|i| i.severity == Severity::Error).count();
    let warnings = issues.iter().filter(|i| i.severity == Severity::Warning).count();

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&issues)?);
        }
        OutputFormat::Text => {
            for issue in &issues {
                println!("{issue}");
            }
            if !issues.is_empty() {
                println!();
            }
            println!("{errors} errors, {warnings} warnings");
        }
    }

    if errors > 0 || (strict && warnings > 0) {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn resolve_theme(cli: &Cli, cfg: &Config, config_dir: &Path) -> theme::ThemeColors {
    let theme_file_path = cfg
        .meta
        .theme_file
        .as_deref()
        .map(|f| theme::resolve_theme_file_path(f, config_dir));
    theme::resolve_theme(
        cli.theme.as_deref(),
        cfg.meta.theme.as_deref(),
        cfg.theme.as_ref(),
        theme_file_path.as_deref(),
        cli.no_color,
    )
}

fn run_selected(cmd: &config::CommandEntry) -> Result<i32> {
    let mut source = prompt::StdinSource;
    let values = prompt::prompt_for_params(&cmd.params, &mut source)?;
    exec::run_command(cmd, &values)
}

/// `name` may be either a command name (exact, or a unique substring match)
/// or a 1-based global id (e.g. `exc run 45`), matching the number shown by
/// `exc list` and the picker grid — these numbers are unique across all
/// profiles, not just within one, so a bare id always resolves unambiguously
/// regardless of which profile it lives in.
fn cmd_run(cfg: &Config, name: &str) -> Result<i32> {
    if let Ok(id) = name.parse::<usize>() {
        return match cfg.resolve_global_id(id) {
            Some((_, cmd)) => run_selected(cmd),
            None => {
                eprintln!("error: no command #{id} (valid range: 1-{})", cfg.all_commands().len());
                Ok(1)
            }
        };
    }

    let target = match cfg.find_command(name) {
        Some(t) => t,
        None => {
            let matches: Vec<_> = cfg
                .all_commands()
                .into_iter()
                .filter(|(_, c)| c.name.contains(name))
                .collect();
            match matches.len() {
                0 => {
                    eprintln!("error: no command named \"{name}\" found");
                    return Ok(1);
                }
                1 => matches[0],
                _ => {
                    eprintln!("error: \"{name}\" is ambiguous, matches:");
                    for (profile, cmd) in matches {
                        eprintln!("  {}/{}", profile.name, cmd.name);
                    }
                    return Ok(1);
                }
            }
        }
    };

    let (_, cmd) = target;
    run_selected(cmd)
}

/// Numbers shown are 1-based **global** ids, continuous across all
/// profiles (profile 0's commands are `1..=n0`, profile 1's continue right
/// after at `n0+1`, etc.) rather than restarting at 1 per profile — so a
/// number printed here always means the same command regardless of which
/// profile it's in, and can be passed directly to `exc run <id>`.
fn cmd_list(cfg: &Config, profile_filter: Option<&str>, plain: bool) -> Result<i32> {
    if let Some(name) = profile_filter
        && cfg.find_profile(name).is_none()
    {
        eprintln!("error: no profile named \"{name}\"");
        return Ok(1);
    }

    let width = crossterm::terminal::size().map(|(w, _)| w as usize).unwrap_or(80);
    let mut base = 0usize;

    for profile in &cfg.profiles {
        let shown = profile_filter.is_none_or(|f| f == profile.name);
        if shown {
            if !plain {
                println!("== {} ==", profile.display_label());
                if let Some(desc) = &profile.description {
                    println!("{desc}");
                }
            }
            if plain {
                for cmd in &profile.commands {
                    println!("{}", cmd.name);
                }
            } else {
                let names: Vec<&str> = profile.commands.iter().map(|c| c.name.as_str()).collect();
                let widest = names.iter().map(|n| layout::display_width(n)).max().unwrap_or(1);
                let columns = layout::grid_columns(width, widest + 6, names.len());
                let rows_total = layout::rows_needed(names.len(), columns);
                for row in 0..rows_total {
                    let mut line = String::new();
                    for col in 0..columns {
                        let idx = layout::index_from_position(row, col, rows_total);
                        let Some(name) = names.get(idx) else { break };
                        line.push_str(&format!("[{:>3}] {name:<widest$}  ", base + idx + 1));
                    }
                    println!("{}", line.trim_end());
                }
                println!();
            }
        }
        base += profile.commands.len();
    }
    Ok(0)
}

fn cmd_sysinfo(cli: &Cli) -> Result<i32> {
    let path = cli.config.clone().unwrap_or_else(config::default_config_path);
    let config_dir = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let cfg = config::load_config(&path).ok().unwrap_or_default();

    let mut info = hostinfo::gather();
    if let Some(title) = &cfg.meta.title {
        info.user_at_host = title.clone();
    }

    let width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
    let no_color = cli.no_color || std::env::var_os("NO_COLOR").is_some();
    if no_color {
        println!("{}", hostinfo::panel::render_plain(&info, width));
    } else {
        let theme = resolve_theme(cli, &cfg, &config_dir);
        println!("{}", hostinfo::panel::render(&info, width, &theme));
    }
    Ok(0)
}

fn cmd_picker(cfg: &Config, cli: &Cli) -> Result<i32> {
    if cfg.profiles.is_empty() {
        eprintln!("error: config defines no profiles\nhint: run `exc init` or edit your config");
        return Ok(1);
    }
    let path = cli.config.clone().unwrap_or_else(config::default_config_path);
    let config_dir = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let theme = resolve_theme(cli, cfg, &config_dir);
    let mut info = hostinfo::gather();
    if let Some(title) = &cfg.meta.title {
        info.user_at_host = title.clone();
    }

    let start_profile = cli
        .profile
        .as_deref()
        .and_then(|name| cfg.profiles.iter().position(|p| p.name == name))
        .unwrap_or(0);

    match picker::run_picker(cfg, start_profile, &theme, &info)? {
        picker::PickerOutcome::Quit => Ok(0),
        picker::PickerOutcome::Run(cmd) => run_selected(&cmd),
    }
}
