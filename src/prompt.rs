use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::Result;

use crate::config::Param;

/// Abstraction over "read a line of visible text", so prompting logic is
/// unit-testable without touching real stdin.
pub trait LineSource {
    fn read_line(&mut self) -> io::Result<String>;
}

pub struct StdinSource;

impl LineSource for StdinSource {
    fn read_line(&mut self) -> io::Result<String> {
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        Ok(line)
    }
}

fn format_prompt(p: &Param) -> String {
    match &p.default {
        Some(d) if !d.is_empty() => format!("{} [{}]: ", p.prompt, d),
        _ => format!("{}: ", p.prompt),
    }
}

/// Prompt for each declared param. Visible params are read via `source`;
/// `secret` params are always read via `rpassword` (masked), regardless of
/// `source`, since masking must talk to the real terminal.
pub fn prompt_for_params(params: &[Param], source: &mut dyn LineSource) -> Result<HashMap<String, String>> {
    let mut values = HashMap::new();
    for p in params {
        let raw = if p.secret {
            rpassword::prompt_password(format!("{}: ", p.prompt))?
        } else {
            print!("{}", format_prompt(p));
            io::stdout().flush()?;
            source.read_line()?.trim().to_string()
        };
        let resolved = if raw.is_empty() {
            p.default.clone().unwrap_or_default()
        } else {
            raw
        };
        values.insert(p.name.clone(), resolved);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSource {
        lines: std::collections::VecDeque<String>,
    }

    impl MockSource {
        fn new(lines: &[&str]) -> Self {
            MockSource {
                lines: lines.iter().map(|s| s.to_string()).collect(),
            }
        }
    }

    impl LineSource for MockSource {
        fn read_line(&mut self) -> io::Result<String> {
            Ok(self.lines.pop_front().unwrap_or_default())
        }
    }

    fn param(name: &str, prompt: &str, default: Option<&str>) -> Param {
        Param {
            name: name.to_string(),
            prompt: prompt.to_string(),
            default: default.map(|s| s.to_string()),
            secret: false,
        }
    }

    #[test]
    fn uses_typed_value_when_present() {
        let params = vec![param("domain", "Domain", Some(""))];
        let mut source = MockSource::new(&["example.com"]);
        let values = prompt_for_params(&params, &mut source).unwrap();
        assert_eq!(values.get("domain").unwrap(), "example.com");
    }

    #[test]
    fn falls_back_to_default_on_empty_input() {
        let params = vec![param("length", "Password length", Some("20"))];
        let mut source = MockSource::new(&[""]);
        let values = prompt_for_params(&params, &mut source).unwrap();
        assert_eq!(values.get("length").unwrap(), "20");
    }

    #[test]
    fn empty_input_with_no_default_yields_empty_string() {
        let params = vec![param("note", "Note", None)];
        let mut source = MockSource::new(&[""]);
        let values = prompt_for_params(&params, &mut source).unwrap();
        assert_eq!(values.get("note").unwrap(), "");
    }
}
