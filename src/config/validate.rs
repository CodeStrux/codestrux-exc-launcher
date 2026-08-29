use std::collections::HashMap;
use std::path::Path;

use regex::Regex;
use serde::Serialize;

use super::Config;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: &'static str,
    pub profile: Option<String>,
    pub command: Option<String>,
    pub message: String,
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{level}[{}]: {}", self.code, self.message)
    }
}

fn placeholder_regex() -> Regex {
    Regex::new(r"\{\{(\w+)\}\}").expect("static regex is valid")
}

pub fn validate_config(cfg: &Config, config_dir: &Path) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let placeholder_re = placeholder_regex();

    // duplicate-name (global)
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (_, cmd) in cfg.all_commands() {
        *seen.entry(cmd.name.as_str()).or_insert(0) += 1;
    }
    for (name, count) in &seen {
        if *count > 1 {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                code: "duplicate-name",
                profile: None,
                command: Some(name.to_string()),
                message: format!("command \"{name}\" is defined more than once"),
            });
        }
    }

    if cfg.profiles.is_empty() {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            code: "empty-profile",
            profile: None,
            command: None,
            message: "config defines no profiles".to_string(),
        });
    }

    for profile in &cfg.profiles {
        if profile.commands.is_empty() {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                code: "empty-profile",
                profile: Some(profile.name.clone()),
                command: None,
                message: format!("profile \"{}\" has zero commands", profile.name),
            });
        }

        for cmd in &profile.commands {
            if cmd.command.trim().is_empty() {
                issues.push(ValidationIssue {
                    severity: Severity::Error,
                    code: "empty-command",
                    profile: Some(profile.name.clone()),
                    command: Some(cmd.name.clone()),
                    message: format!(
                        "profile \"{}\", command \"{}\": command string is empty",
                        profile.name, cmd.name
                    ),
                });
            }

            let declared: std::collections::HashSet<&str> =
                cmd.params.iter().map(|p| p.name.as_str()).collect();
            let referenced: std::collections::HashSet<String> = placeholder_re
                .captures_iter(&cmd.command)
                .map(|c| c[1].to_string())
                .collect();

            for placeholder in &referenced {
                if !declared.contains(placeholder.as_str()) {
                    issues.push(ValidationIssue {
                        severity: Severity::Error,
                        code: "unresolved-placeholder",
                        profile: Some(profile.name.clone()),
                        command: Some(cmd.name.clone()),
                        message: format!(
                            "profile \"{}\", command \"{}\": template references {{{{{}}}}} but no param named \"{}\" is declared",
                            profile.name, cmd.name, placeholder, placeholder
                        ),
                    });
                }
            }

            for param in &cmd.params {
                if !referenced.contains(&param.name) {
                    issues.push(ValidationIssue {
                        severity: Severity::Warning,
                        code: "unused-param",
                        profile: Some(profile.name.clone()),
                        command: Some(cmd.name.clone()),
                        message: format!(
                            "profile \"{}\", command \"{}\": param \"{}\" is declared but never referenced in the command template",
                            profile.name, cmd.name, param.name
                        ),
                    });
                }
            }
        }
    }

    if let Some(theme_name) = &cfg.meta.theme
        && !matches!(theme_name.as_str(), "default" | "dark" | "mono")
    {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            code: "unknown-theme",
            profile: None,
            command: None,
            message: format!(
                "[meta] theme = \"{theme_name}\" is not one of default|dark|mono, falling back to default at runtime"
            ),
        });
    }

    if cfg.theme.is_some() && cfg.meta.theme_file.is_some() {
        issues.push(ValidationIssue {
            severity: Severity::Warning,
            code: "theme-file-ignored",
            profile: None,
            command: None,
            message: "both an inline [theme] table and [meta] theme_file are set; the inline table wins and theme_file is ignored".to_string(),
        });
    }

    if let Some(custom) = &cfg.theme {
        for (field, value) in custom.invalid_fields() {
            issues.push(ValidationIssue {
                severity: Severity::Error,
                code: "invalid-theme-color",
                profile: None,
                command: None,
                message: format!("[theme] {field} = \"{value}\" is not a valid color (use #rrggbb or a named color)"),
            });
        }
    } else if let Some(file) = &cfg.meta.theme_file {
        let path = theme::resolve_theme_file_path(file, config_dir);
        if !path.exists() {
            issues.push(ValidationIssue {
                severity: Severity::Warning,
                code: "theme-file-not-found",
                profile: None,
                command: None,
                message: format!("[meta] theme_file = \"{file}\" does not exist at {}", path.display()),
            });
        } else {
            match theme::load_theme_file(&path) {
                None => issues.push(ValidationIssue {
                    severity: Severity::Warning,
                    code: "theme-file-parse-error",
                    profile: None,
                    command: None,
                    message: format!("[meta] theme_file at {} failed to parse as a theme table", path.display()),
                }),
                Some(custom) => {
                    for (field, value) in custom.invalid_fields() {
                        issues.push(ValidationIssue {
                            severity: Severity::Error,
                            code: "invalid-theme-color",
                            profile: None,
                            command: None,
                            message: format!(
                                "{}: {field} = \"{value}\" is not a valid color (use #rrggbb or a named color)",
                                path.display()
                            ),
                        });
                    }
                }
            }
        }
    }

    // Deterministic ordering: errors before warnings, then by message.
    issues.sort_by(|a, b| {
        (a.severity == Severity::Warning, &a.message).cmp(&(b.severity == Severity::Warning, &b.message))
    });
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Config {
        toml::from_str(raw).expect("fixture should parse as valid TOML")
    }

    fn no_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(".")
    }

    #[test]
    fn valid_config_has_no_errors() {
        let raw = std::fs::read_to_string("tests/fixtures/valid.toml").unwrap();
        let cfg = parse(&raw);
        let issues = validate_config(&cfg, &no_dir());
        assert!(
            issues.iter().all(|i| i.severity == Severity::Warning),
            "unexpected errors: {issues:?}"
        );
    }

    #[test]
    fn detects_duplicate_name() {
        let raw = std::fs::read_to_string("tests/fixtures/duplicate_name.toml").unwrap();
        let cfg = parse(&raw);
        let issues = validate_config(&cfg, &no_dir());
        assert!(issues.iter().any(|i| i.code == "duplicate-name"));
    }

    #[test]
    fn detects_unresolved_placeholder() {
        let raw = std::fs::read_to_string("tests/fixtures/unresolved_placeholder.toml").unwrap();
        let cfg = parse(&raw);
        let issues = validate_config(&cfg, &no_dir());
        assert!(issues.iter().any(|i| i.code == "unresolved-placeholder"));
    }

    #[test]
    fn detects_unused_param() {
        let raw = std::fs::read_to_string("tests/fixtures/unused_param.toml").unwrap();
        let cfg = parse(&raw);
        let issues = validate_config(&cfg, &no_dir());
        assert!(issues.iter().any(|i| i.code == "unused-param" && i.severity == Severity::Warning));
    }

    #[test]
    fn detects_empty_command() {
        let raw = std::fs::read_to_string("tests/fixtures/empty_command.toml").unwrap();
        let cfg = parse(&raw);
        let issues = validate_config(&cfg, &no_dir());
        assert!(issues.iter().any(|i| i.code == "empty-command"));
    }

    #[test]
    fn detects_invalid_inline_theme_color() {
        let raw = std::fs::read_to_string("tests/fixtures/invalid_theme_color.toml").unwrap();
        let cfg = parse(&raw);
        let issues = validate_config(&cfg, &no_dir());
        assert!(issues.iter().any(|i| i.code == "invalid-theme-color" && i.severity == Severity::Error));
    }

    #[test]
    fn detects_missing_theme_file() {
        let raw = std::fs::read_to_string("tests/fixtures/missing_theme_file.toml").unwrap();
        let cfg = parse(&raw);
        let issues = validate_config(&cfg, &no_dir());
        assert!(issues.iter().any(|i| i.code == "theme-file-not-found"));
    }

    #[test]
    fn warns_when_both_inline_theme_and_theme_file_set() {
        let raw = r#"
[meta]
theme_file = "somewhere.toml"

[theme]
accent = "cyan"
"#;
        let cfg = parse(raw);
        let issues = validate_config(&cfg, &no_dir());
        assert!(issues.iter().any(|i| i.code == "theme-file-ignored"));
    }
}
