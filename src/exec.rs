use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::Result;
use crossterm::{cursor, execute, terminal};

use crate::config::{expand_template, CommandEntry};

/// Clear the screen, print what's about to run, expand `{{param}}`
/// placeholders, execute via `sh -c` with fully inherited stdio, and
/// return the child's exit code.
pub fn run_command(cmd: &CommandEntry, values: &HashMap<String, String>) -> Result<i32> {
    let expanded = expand_template(&cmd.command, values);

    let mut stdout = io::stdout();
    let _ = execute!(stdout, terminal::Clear(terminal::ClearType::All), cursor::MoveTo(0, 0));
    println!("Running: {}", cmd.name);
    println!("$ {expanded}");
    stdout.flush()?;

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
