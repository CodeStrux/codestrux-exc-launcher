use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::Result;
use crossterm::{cursor, execute, terminal};

use crate::config::{expand_template, CommandEntry};

fn announce(id: usize, cmd: &CommandEntry, expanded: &str) -> Result<()> {
    let mut stdout = io::stdout();
    let _ = execute!(stdout, terminal::Clear(terminal::ClearType::All), cursor::MoveTo(0, 0));
    println!("Running [{id}] {}", cmd.name);
    println!("$ {expanded}");
    stdout.flush()?;
    Ok(())
}

/// Clear the screen, print what's about to run (with its global id, e.g.
/// `Running [12] disk-usage`), expand `{{param}}` placeholders, and hand off
/// to `sh -c`.
///
/// On Unix this replaces the `exc` process image outright (`execve`, like
/// the shell `exec` builtin) instead of forking and waiting on a child: the
/// command becomes the foreground process directly, `exc` no longer exists
/// as its parent, and the command's own exit code becomes the exit code of
/// the whole `exc` invocation. This matters for anything long-lived or
/// interactive (an `ssh` session, `docker attach`, etc.), which would
/// otherwise stay wrapped under a lingering `exc` parent for as long as it
/// runs.
#[cfg(unix)]
pub fn run_command(id: usize, cmd: &CommandEntry, values: &HashMap<String, String>) -> Result<i32> {
    use std::os::unix::process::CommandExt;

    let expanded = expand_template(&cmd.command, values);
    announce(id, cmd, &expanded)?;

    // `exec` only returns if it failed to replace the process at all (e.g.
    // no `sh` on $PATH); on success control never comes back here.
    let err = std::process::Command::new("sh").arg("-c").arg(&expanded).exec();
    Err(err.into())
}

#[cfg(not(unix))]
pub fn run_command(id: usize, cmd: &CommandEntry, values: &HashMap<String, String>) -> Result<i32> {
    let expanded = expand_template(&cmd.command, values);
    announce(id, cmd, &expanded)?;

    let status = std::process::Command::new("sh").arg("-c").arg(&expanded).status()?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_template_used_by_run_command_matches_config_module() {
        let mut values = HashMap::new();
        values.insert("x".to_string(), "42".to_string());
        assert_eq!(expand_template("echo {{x}}", &values), "echo 42");
    }
}
