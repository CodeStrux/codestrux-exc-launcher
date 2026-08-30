//! Double-line box-drawing panel showing OS/host/kernel/uptime/memory/disk/
//! process stats, replacing the original toilet/lolcat figlet banner. Field
//! wrapping reuses the shared `layout` grid math. The box border always
//! stretches to the full available terminal width — matching the command
//! grid below it — with the extra space filled as trailing padding on each
//! row rather than left as dead space outside the box.

use super::{format_uptime, SystemInfo};
use crate::layout::display_width;
use crate::theme::ThemeColors;

const MAX_FIELD_COLUMNS: usize = 4;
const MIN_BOX_WIDTH: usize = 30;
/// Below this terminal width, the panel switches to `fields_compact` — a
/// short, single-column-friendly field set — instead of the full list, so a
/// narrow terminal spends most of its height on the command grid rather
/// than the info panel.
const COMPACT_WIDTH_THRESHOLD: u16 = 80;

/// Field order is grouped by category (system identity, compute, memory/
/// storage, power, network, environment, background) rather than by tier or
/// insertion order, so related fields land next to each other in the grid
/// instead of being scattered across rows.
fn fields(info: &SystemInfo) -> Vec<(&'static str, String)> {
    let mut f = vec![
        // System identity.
        ("OS", info.os_display.clone()),
        ("Host", info.host_model.clone()),
        ("Kernel", info.kernel_version.clone()),
        ("Uptime", format_uptime(info.uptime_secs)),
    ];

    // Compute.
    match (&info.cpu_model, info.cpu_cores) {
        (Some(model), Some(cores)) => f.push(("CPU", format!("{model} ({cores} cores)"))),
        (Some(model), None) => f.push(("CPU", model.clone())),
        (None, Some(cores)) => f.push(("CPU", format!("{cores} cores"))),
        (None, None) => {}
    }
    if let Some(load) = info.load_avg_1m {
        f.push(("Load", format!("{load:.2}")));
    } else if let Some(pct) = info.cpu_percent {
        f.push(("CPU%", format!("{pct:.0}%")));
    }
    if let Some(gpu) = &info.gpu_name {
        f.push(("GPU", gpu.clone()));
    }

    // Memory & storage.
    f.push(("Memory", format!("{}M / {}M", info.mem_used_mb, info.mem_total_mb)));
    if let (Some(used), Some(total)) = (info.swap_used_mb, info.swap_total_mb) {
        f.push(("Swap", format!("{used}M / {total}M")));
    }
    f.push((
        "Disk",
        format!("{}G / {}G ({}%)", info.disk_used_gb, info.disk_total_gb, info.disk_pct),
    ));
    f.push(("Procs", format!("{} running", info.process_count)));

    // Power.
    if let Some(pct) = info.battery_pct {
        let charging = info.battery_charging.unwrap_or(false);
        f.push(("Battery", format!("{pct}%{}", if charging { " (charging)" } else { "" })));
    }

    // Network (local, then background-refreshed). Background-refreshed
    // fields are always present, as a placeholder if not resolved yet, so
    // the panel's row/column count never changes once the picker is
    // open — only the cell text updates in place, no layout jump.
    if let Some(ip) = &info.local_ip {
        f.push(("IP", ip.clone()));
    }
    let tier3 = info.tier3.snapshot();
    let net_value = match (tier3.net_rx_bps, tier3.net_tx_bps) {
        (Some(rx), Some(tx)) => Some(format!("\u{2193}{} \u{2191}{}", format_bps(rx), format_bps(tx))),
        _ => None,
    };
    f.push(("Public IP", tier3_placeholder(tier3.ready, tier3.public_ip.clone())));
    f.push(("Net", tier3_placeholder(tier3.ready, net_value)));

    // Environment.
    if let Some(shell) = &info.shell {
        f.push(("Shell", shell.clone()));
    }
    if let Some(term) = &info.terminal {
        f.push(("Term", term.clone()));
    }
    if let Some(time) = super::platform::local_time_string() {
        f.push(("Time", time));
    }

    // Background (package updates only — network fields already grouped above).
    let updates_value = tier3.pending_updates.map(|n| format!("{n} pending"));
    f.push(("Updates", tier3_placeholder(tier3.ready, updates_value)));

    f
}

/// Minimal field set shown below `COMPACT_WIDTH_THRESHOLD`: just the
/// "vitals" — uptime, memory, disk, and battery if present — so a narrow
/// terminal doesn't spend most of its height on the info panel instead of
/// the command grid.
fn fields_compact(info: &SystemInfo) -> Vec<(&'static str, String)> {
    let mut f = vec![
        ("Uptime", format_uptime(info.uptime_secs)),
        ("Memory", format!("{}M / {}M", info.mem_used_mb, info.mem_total_mb)),
        (
            "Disk",
            format!("{}G / {}G ({}%)", info.disk_used_gb, info.disk_total_gb, info.disk_pct),
        ),
    ];
    if let Some(pct) = info.battery_pct {
        let charging = info.battery_charging.unwrap_or(false);
        f.push(("Battery", format!("{pct}%{}", if charging { " (charging)" } else { "" })));
    }
    f
}

/// `…` while the background fetch hasn't completed its first pass yet, `n/a`
/// if it completed but couldn't determine this particular value (offline,
/// no supported package manager, etc.), or the real value once known.
fn tier3_placeholder(ready: bool, value: Option<String>) -> String {
    if !ready {
        "\u{2026}".to_string()
    } else {
        value.unwrap_or_else(|| "n/a".to_string())
    }
}

fn format_bps(bytes_per_sec: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes_per_sec >= MB {
        format!("{:.1}MB/s", bytes_per_sec as f64 / MB as f64)
    } else if bytes_per_sec >= KB {
        format!("{:.0}KB/s", bytes_per_sec as f64 / KB as f64)
    } else {
        format!("{bytes_per_sec}B/s")
    }
}

/// Which theme color a rendered chunk of text should use. Kept separate
/// from any particular output format (ANSI string vs. ratatui spans) so the
/// row/padding math is computed exactly once and both the CLI renderer and
/// the picker's ratatui renderer can each color it their own way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Border,
    /// Field labels and the header title — `theme.accent`.
    Accent,
    /// Field values — `theme.text`.
    Text,
}

pub struct Segment {
    pub role: Role,
    pub text: String,
}

/// Build the panel as role-tagged segments, one inner `Vec` per line.
/// `render`/`render_plain` turn this into an ANSI/plain string; the picker
/// turns it into colored ratatui spans directly, so labels and values (and
/// the border) can each get their own color without re-deriving the
/// column/padding layout a second time.
pub fn layout(info: &SystemInfo, terminal_width: u16) -> Vec<Vec<Segment>> {
    let field_list = if terminal_width < COMPACT_WIDTH_THRESHOLD { fields_compact(info) } else { fields(info) };
    let label_width = field_list.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    let value_width = field_list.iter().map(|(_, v)| display_width(v)).max().unwrap_or(0);
    // "Label   value" -> label + 3 spaces + value
    let cell_width = label_width + 3 + value_width;

    let available = terminal_width.saturating_sub(4).max(MIN_BOX_WIDTH as u16) as usize;
    let columns = crate::layout::grid_columns(available, cell_width, field_list.len()).min(MAX_FIELD_COLUMNS);
    let columns = columns.max(1);

    let content_width = (cell_width + 2) * columns + (columns.saturating_sub(1) * 2);
    // Stretch to fill the full available width (matching the grid below);
    // only fall back to the content's own width if the terminal is too
    // narrow to fit the fields at all.
    let box_width = available.max(content_width).max(MIN_BOX_WIDTH);
    let inner_width = box_width.saturating_sub(2);

    let border = |text: String| Segment { role: Role::Border, text };
    let accent = |text: String| Segment { role: Role::Accent, text };
    let value_seg = |text: String| Segment { role: Role::Text, text };

    let mut lines: Vec<Vec<Segment>> = Vec::new();

    // Top border with title inset.
    let title = &info.user_at_host;
    if title.len() + 4 <= inner_width {
        let fill = inner_width - title.len() - 3;
        lines.push(vec![
            border("\u{2554}\u{2550} ".to_string()),
            accent(title.clone()),
            border(format!(" {}\u{2557}", "\u{2550}".repeat(fill))),
        ]);
    } else {
        lines.push(vec![border(format!("\u{2554}{}\u{2557}", "\u{2550}".repeat(inner_width)))]);
        lines.push(vec![
            border("\u{2551} ".to_string()),
            accent(title.clone()),
            border(format!("{}\u{2551}", " ".repeat(inner_width.saturating_sub(title.len() + 1)))),
        ]);
    }

    for chunk in field_list.chunks(columns) {
        let mut row_segments: Vec<Segment> = Vec::new();
        let mut row_width = 0usize;
        for (i, (label, value)) in chunk.iter().enumerate() {
            row_segments.push(accent(format!("{:<label_width$}", label)));
            row_segments.push(border("  ".to_string()));
            row_segments.push(value_seg(format!("{:<value_width$}", value)));
            row_width += label_width + 2 + value_width;
            if i + 1 < chunk.len() {
                row_segments.push(border("  ".to_string()));
                row_width += 2;
            }
        }
        // "║ " (2) + row + pad + "║" (1) must equal box_width (inner_width + 2).
        let pad = inner_width.saturating_sub(row_width + 1);
        let mut line = vec![border("\u{2551} ".to_string())];
        line.extend(row_segments);
        line.push(border(format!("{}\u{2551}", " ".repeat(pad))));
        lines.push(line);
    }

    lines.push(vec![border(format!("\u{255a}{}\u{255d}", "\u{2550}".repeat(inner_width)))]);
    lines
}

/// Render the box as a plain string (no color codes) — used for width
/// calculations and for `--no-color` output.
pub fn render_plain(info: &SystemInfo, terminal_width: u16) -> String {
    render_from_layout(layout(info, terminal_width), None)
}

pub fn render(info: &SystemInfo, terminal_width: u16, theme: &ThemeColors) -> String {
    render_from_layout(layout(info, terminal_width), Some(theme))
}

fn render_from_layout(lines: Vec<Vec<Segment>>, theme: Option<&ThemeColors>) -> String {
    let color_for = |role: Role, theme: &ThemeColors| -> crossterm::style::Color {
        match role {
            Role::Border => theme.border,
            Role::Accent => theme.accent,
            Role::Text => theme.text,
        }
    };

    let rendered_lines: Vec<String> = lines
        .into_iter()
        .map(|segments| {
            let mut line = String::new();
            for seg in segments {
                match theme {
                    Some(t) => {
                        line.push_str(&format!("\x1b[38;5;{}m{}", color_code(color_for(seg.role, t)), seg.text));
                    }
                    None => line.push_str(&seg.text),
                }
            }
            if theme.is_some() {
                line.push_str("\x1b[0m");
            }
            line
        })
        .collect();

    rendered_lines.join("\n")
}

fn color_code(color: crossterm::style::Color) -> u8 {
    use crossterm::style::Color;
    match color {
        Color::Black => 0,
        Color::DarkGrey => 8,
        Color::Red => 9,
        Color::DarkRed => 1,
        Color::Green => 10,
        Color::DarkGreen => 2,
        Color::Yellow => 11,
        Color::DarkYellow => 3,
        Color::Blue => 12,
        Color::DarkBlue => 4,
        Color::Magenta => 13,
        Color::DarkMagenta => 5,
        Color::Cyan => 14,
        Color::DarkCyan => 6,
        Color::White => 15,
        Color::Grey => 7,
        _ => 15,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn sample() -> SystemInfo {
        SystemInfo {
            user_at_host: "aao@AAO-MBP".to_string(),
            os_display: "macOS 26.6.2".to_string(),
            host_model: "Mac16,7".to_string(),
            kernel_version: "25.6.0".to_string(),
            uptime_secs: 2 * 86400 + 4 * 3600 + 24 * 60,
            mem_used_mb: 5989,
            mem_total_mb: 49152,
            disk_used_gb: 412,
            disk_total_gb: 926,
            disk_pct: 44,
            process_count: 412,
            ..Default::default()
        }
    }

    /// Every optional/tier field populated — the worst case for row width.
    fn sample_fully_populated() -> SystemInfo {
        let info = SystemInfo {
            cpu_model: Some("Apple M4 Max".to_string()),
            cpu_cores: Some(16),
            load_avg_1m: Some(2.47),
            cpu_percent: None,
            swap_used_mb: Some(512),
            swap_total_mb: Some(2048),
            local_ip: Some("192.168.1.42".to_string()),
            battery_pct: Some(87),
            battery_charging: Some(true),
            shell: Some("/opt/homebrew/bin/zsh".to_string()),
            terminal: Some("iTerm.app".to_string()),
            gpu_name: Some("Apple M4 Max".to_string()),
            ..sample()
        };
        info.tier3.set(crate::hostinfo::background::Tier3Snapshot {
            public_ip: Some("203.0.113.7".to_string()),
            pending_updates: Some(4),
            net_rx_bps: Some(1_500_000),
            net_tx_bps: Some(85_000),
            ready: true,
        });
        info
    }

    #[test]
    fn renders_within_width_at_narrow_terminal() {
        let info = sample();
        let out = render_plain(&info, 60);
        for line in out.lines() {
            assert!(UnicodeWidthStr::width(line) <= 62, "line too wide: {line:?}");
        }
    }

    #[test]
    fn stretches_to_fill_a_wide_terminal_instead_of_staying_content_sized() {
        let info = sample();
        let out = render_plain(&info, 120);
        let widths: Vec<usize> = out.lines().map(UnicodeWidthStr::width).collect();
        let max_width = *widths.iter().max().unwrap();
        // must not overflow the terminal...
        assert!(max_width <= 120, "line too wide: {max_width}");
        // ...but should stretch close to it, not stay pinned to its old
        // fixed 78-column content-driven cap.
        assert!(max_width > 100, "box didn't stretch to fill the wide terminal: {max_width}");
        // every line (border + content rows) should be the same width, so
        // the box reads as a clean rectangle next to the full-width grid.
        assert!(widths.iter().all(|w| *w == max_width), "inconsistent row widths: {widths:?}");
    }

    #[test]
    fn all_rows_share_the_same_width_at_a_narrow_terminal_too() {
        let info = sample();
        let out = render_plain(&info, 60);
        let widths: Vec<usize> = out.lines().map(UnicodeWidthStr::width).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "inconsistent row widths: {widths:?}");
    }

    #[test]
    fn contains_all_fields() {
        let info = sample();
        let out = render_plain(&info, 80);
        for (label, _) in fields(&info) {
            assert!(out.contains(label), "missing field {label} in:\n{out}");
        }
    }

    #[test]
    fn absent_optional_fields_are_simply_omitted() {
        let info = sample();
        let out = render_plain(&info, 80);
        // "IP" is deliberately excluded: it's now a substring of the
        // always-present "Public IP" placeholder row, so it can't be
        // checked this way — local_ip's own absence isn't covered here.
        for label in ["CPU", "Load", "CPU%", "Swap", "Battery", "Shell", "Term", "GPU"] {
            assert!(!out.contains(label), "unexpected field {label} present with no data:\n{out}");
        }
    }

    #[test]
    fn background_fields_show_a_placeholder_before_ready() {
        // `sample()`'s tier3 handle defaults to not-ready.
        let info = sample();
        let out = render_plain(&info, 100);
        assert!(out.contains("Public IP"), "Public IP row should be present even before ready:\n{out}");
        assert!(out.contains("Updates"), "Updates row should be present even before ready:\n{out}");
        assert!(out.contains('\u{2026}'), "expected a loading placeholder glyph in:\n{out}");
    }

    #[test]
    fn background_field_count_is_stable_across_readiness_states() {
        // The picker must never reflow (grow/shrink rows) as background
        // data arrives — only the cell text should change in place. Holds
        // every other field constant (via `sample()`) so tier3 readiness is
        // the only variable across the three snapshots compared.
        let not_ready = sample();

        let ready_but_empty = sample();
        ready_but_empty
            .tier3
            .set(crate::hostinfo::background::Tier3Snapshot { ready: true, ..Default::default() });

        let ready_and_populated = sample();
        ready_and_populated.tier3.set(crate::hostinfo::background::Tier3Snapshot {
            public_ip: Some("203.0.113.7".to_string()),
            pending_updates: Some(4),
            net_rx_bps: Some(1_500_000),
            net_tx_bps: Some(85_000),
            ready: true,
        });

        let not_ready_count = fields(&not_ready).len();
        let ready_but_empty_count = fields(&ready_but_empty).len();
        let ready_and_populated_count = fields(&ready_and_populated).len();

        assert_eq!(
            not_ready_count, ready_but_empty_count,
            "field count changed once the background pass completed with no data"
        );
        assert_eq!(
            ready_but_empty_count, ready_and_populated_count,
            "field count changed once real background values arrived"
        );
    }

    #[test]
    fn below_threshold_uses_the_compact_field_set() {
        let info = sample_fully_populated();
        let out = render_plain(&info, COMPACT_WIDTH_THRESHOLD - 1);
        for label in ["Uptime", "Memory", "Disk", "Battery"] {
            assert!(out.contains(label), "missing compact field {label} in:\n{out}");
        }
        for label in ["OS", "Host", "Kernel", "CPU", "Load", "GPU", "Swap", "Shell", "Term", "Public IP"] {
            assert!(!out.contains(label), "full-only field {label} leaked into compact panel:\n{out}");
        }
    }

    #[test]
    fn at_or_above_threshold_uses_the_full_field_set() {
        let info = sample();
        let out = render_plain(&info, COMPACT_WIDTH_THRESHOLD);
        for (label, _) in fields(&info) {
            assert!(out.contains(label), "missing field {label} at exactly the threshold width:\n{out}");
        }
    }

    #[test]
    fn compact_field_set_is_much_shorter_than_the_full_one() {
        let info = sample_fully_populated();
        let compact_len = fields_compact(&info).len();
        let full_len = fields(&info).len();
        assert!(
            compact_len <= 4,
            "compact field set should stay minimal, got {compact_len} fields"
        );
        assert!(compact_len < full_len, "compact set ({compact_len}) should be shorter than full ({full_len})");
    }

    #[test]
    fn fully_populated_fields_all_appear() {
        let info = sample_fully_populated();
        let out = render_plain(&info, 100);
        for (label, _) in fields(&info) {
            assert!(out.contains(label), "missing field {label} in:\n{out}");
        }
    }

    #[test]
    fn row_width_invariant_holds_with_fully_populated_fields() {
        let info = sample_fully_populated();
        for width in [60u16, 100, 160] {
            let out = render_plain(&info, width);
            let widths: Vec<usize> = out.lines().map(UnicodeWidthStr::width).collect();
            assert!(widths.windows(2).all(|w| w[0] == w[1]), "inconsistent row widths at {width}: {widths:?}");
        }
    }
}
