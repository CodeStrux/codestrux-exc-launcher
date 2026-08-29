pub mod validate;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::ConfigError;
use crate::theme::CustomTheme;

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Meta {
    #[serde(default)]
    pub title: Option<String>,
    /// Built-in palette name: default | dark | mono.
    #[serde(default)]
    pub theme: Option<String>,
    /// Path to an external file defining a custom `[theme]`-shaped palette
    /// (same fields as the inline `[theme]` table below), resolved relative
    /// to the config file's directory unless absolute or `~`-prefixed.
    /// Ignored when an inline `[theme]` table is also present.
    #[serde(default)]
    pub theme_file: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Param {
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CommandEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub params: Vec<Param>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub commands: Vec<CommandEntry>,
}

impl Profile {
    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Config {
    #[serde(default)]
    pub meta: Meta,
    /// Optional inline custom palette; takes precedence over `[meta]
    /// theme_file` when both are present.
    #[serde(default)]
    pub theme: Option<CustomTheme>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

impl Config {
    /// All commands across all profiles, paired with the owning profile.
    pub fn all_commands(&self) -> Vec<(&Profile, &CommandEntry)> {
        self.profiles
            .iter()
            .flat_map(|p| p.commands.iter().map(move |c| (p, c)))
            .collect()
    }

    /// Resolve a command by exact name, globally unique across profiles.
    pub fn find_command(&self, name: &str) -> Option<(&Profile, &CommandEntry)> {
        self.all_commands().into_iter().find(|(_, c)| c.name == name)
    }

    pub fn find_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// Resolve a 1-based global command number — stable across profiles, so
    /// "#5" always means the same command no matter which profile is
    /// currently shown or listed. Numbering runs continuously across
    /// profiles in config-file order (profile 0's commands are 1..=n0,
    /// profile 1's continue at n0+1, etc.) rather than restarting at 1 for
    /// each profile.
    pub fn resolve_global_id(&self, id: usize) -> Option<(&Profile, &CommandEntry)> {
        let (pi, li) = locate_global_id(&self.profiles, id)?;
        Some((&self.profiles[pi], &self.profiles[pi].commands[li]))
    }
}

/// 1-based global id of the command at `profiles[profile_idx].commands[local_idx]`.
pub fn global_id_of(profiles: &[Profile], profile_idx: usize, local_idx: usize) -> usize {
    let base: usize = profiles[..profile_idx].iter().map(|p| p.commands.len()).sum();
    base + local_idx + 1
}

/// Locate `(profile_idx, local_idx)` for a 1-based global id, searching
/// across all profiles in order. Returns `None` if `id` is 0 or past the
/// last command.
pub fn locate_global_id(profiles: &[Profile], id: usize) -> Option<(usize, usize)> {
    let mut remaining = id.checked_sub(1)?;
    for (pi, p) in profiles.iter().enumerate() {
        if remaining < p.commands.len() {
            return Some((pi, remaining));
        }
        remaining -= p.commands.len();
    }
    None
}

pub fn default_config_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("exc/config.toml");
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/exc/config.toml")
}

pub fn load_config(path: &Path) -> Result<Config, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::NotFound(path.to_path_buf()));
    }
    let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let config: Config = toml::from_str(&raw).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source: Box::new(source),
    })?;
    Ok(config)
}

/// Expand `{{param}}` placeholders in `template` using `values`.
pub fn expand_template(template: &str, values: &HashMap<String, String>) -> String {
    let mut out = template.to_string();
    for (k, v) in values {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_config() {
        let raw = std::fs::read_to_string("tests/fixtures/valid.toml").unwrap();
        let cfg: Config = toml::from_str(&raw).expect("valid config should parse");
        assert_eq!(cfg.profiles.len(), 2);
        assert!(cfg.find_command("cert-check-online").is_some());
    }

    #[test]
    fn broken_syntax_reports_line_col() {
        let raw = std::fs::read_to_string("tests/fixtures/broken_syntax.toml").unwrap();
        let err = toml::from_str::<Config>(&raw).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("line"), "error should mention a line number: {msg}");
    }

    #[test]
    fn expand_template_replaces_placeholders() {
        let mut values = HashMap::new();
        values.insert("domain".to_string(), "example.com".to_string());
        let out = expand_template("openssl s_client -connect {{domain}}:443", &values);
        assert_eq!(out, "openssl s_client -connect example.com:443");
    }

    #[test]
    fn expand_template_leaves_unresolved_placeholders_untouched() {
        let values = HashMap::new();
        let out = expand_template("echo {{missing}}", &values);
        assert_eq!(out, "echo {{missing}}");
    }

    /// Regression coverage for the example config shipped at the repo root:
    /// guards against TOML-escaping mistakes in its trickiest entries
    /// (nested quotes, backslashes, `;`-chains) surviving a round trip.
    #[test]
    fn example_config_preserves_tricky_commands() {
        let raw = std::fs::read_to_string("config.toml").unwrap();
        let cfg: Config = toml::from_str(&raw).expect("example config should parse");

        let cmd = |name: &str| cfg.find_command(name).unwrap().1.command.clone();

        assert_eq!(
            cmd("git-clean-branches"),
            "git fetch --prune; git branch --merged main | grep -v '\\*\\|main'"
        );
        assert_eq!(cmd("gen-password"), "openssl rand -base64 {{length}} | tr -d '\\n'; echo");
        assert_eq!(
            cmd("gpg-decrypt"),
            r#"file=$(fzf --query '.gpg'); [ -z "$file" ] && echo "No file selected" >&2 || gpg --trust-model always --output "${file%.gpg}" --decrypt "$file""#
        );
        assert!(cmd("gen-hex-secret").contains(r#"require('crypto').randomBytes(32).toString('hex')"#));
    }

    #[test]
    fn global_ids_are_continuous_and_unambiguous_across_profiles() {
        let raw = std::fs::read_to_string("config.toml").unwrap();
        let cfg: Config = toml::from_str(&raw).unwrap();

        // #1 is the first command of the first profile.
        let (p1, c1) = cfg.resolve_global_id(1).unwrap();
        assert_eq!(p1.name, "system");
        assert_eq!(c1.name, "disk-usage");

        // The first command of the second profile continues numbering
        // right after the last command of the first, not back at #1.
        let system_len = cfg.profiles[0].commands.len();
        let (p2, c2) = cfg.resolve_global_id(system_len + 1).unwrap();
        assert_eq!(p2.name, "git");
        assert_eq!(c2.name, cfg.profiles[1].commands[0].name);

        // Out of range and zero both resolve to nothing.
        assert!(cfg.resolve_global_id(0).is_none());
        assert!(cfg.resolve_global_id(cfg.all_commands().len() + 1).is_none());
    }

    #[test]
    fn global_id_of_and_locate_global_id_round_trip() {
        let raw = std::fs::read_to_string("config.toml").unwrap();
        let cfg: Config = toml::from_str(&raw).unwrap();

        for (expected_id, (_, _)) in cfg.all_commands().into_iter().enumerate() {
            let expected_id = expected_id + 1;
            let (pi, li) = locate_global_id(&cfg.profiles, expected_id).unwrap();
            assert_eq!(global_id_of(&cfg.profiles, pi, li), expected_id);
        }
    }
}
