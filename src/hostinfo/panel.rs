//! Double-line box-drawing panel showing OS/host/kernel/uptime/memory/disk/
//! process stats, replacing the original toilet/lolcat figlet banner. Field
//! wrapping reuses the shared `layout` grid math, capped to a narrow (2
//! fields per row) layout per the confirmed sketch. The box border always
//! stretches to the full available terminal width — matching the command
//! grid below it — with the extra space filled as trailing padding on each
//! row rather than left as dead space outside the box.

use super::{format_uptime, SystemInfo};
use crate::layout::display_width;
use crate::theme::ThemeColors;

const MAX_FIELD_COLUMNS: usize = 2;
const MIN_BOX_WIDTH: usize = 30;

fn fields(info: &SystemInfo) -> Vec<(&'static str, String)> {
    vec![
        ("OS", info.os_display.clone()),
        ("Host", info.host_model.clone()),
        ("Kernel", info.kernel_version.clone()),
        ("Uptime", format_uptime(info.uptime_secs)),
        ("Memory", format!("{}M / {}M", info.mem_used_mb, info.mem_total_mb)),
        (
            "Disk",
            format!("{}G / {}G ({}%)", info.disk_used_gb, info.disk_total_gb, info.disk_pct),
        ),
        ("Procs", format!("{} running", info.process_count)),
    ]
}

/// Render the box as a plain string (no color codes) — used for width
/// calculations and for `--no-color` output.
pub fn render_plain(info: &SystemInfo, terminal_width: u16) -> String {
    render_inner(info, terminal_width, None)
}

pub fn render(info: &SystemInfo, terminal_width: u16, theme: &ThemeColors) -> String {
    render_inner(info, terminal_width, Some(theme))
}

fn render_inner(info: &SystemInfo, terminal_width: u16, theme: Option<&ThemeColors>) -> String {
    let field_list = fields(info);
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

    let mut out = String::new();
    let (c_border, c_accent, c_text, reset) = match theme {
        Some(t) => (
            format!("\x1b[38;5;{}m", color_code(t.border)),
            format!("\x1b[38;5;{}m", color_code(t.accent)),
            format!("\x1b[38;5;{}m", color_code(t.text)),
            "\x1b[0m".to_string(),
        ),
        None => (String::new(), String::new(), String::new(), String::new()),
    };

    let inner_width = box_width.saturating_sub(2);

    // Top border with title inset.
    let title = &info.user_at_host;
    if title.len() + 4 <= inner_width {
        let fill = inner_width - title.len() - 3;
        out.push_str(&format!(
            "{c_border}╔═ {c_accent}{title}{c_border} {}╗{reset}\n",
            "═".repeat(fill)
        ));
    } else {
        out.push_str(&format!("{c_border}╔{}╗{reset}\n", "═".repeat(inner_width)));
        out.push_str(&format!(
            "{c_border}║{reset} {c_accent}{title}{reset}{}{c_border}║{reset}\n",
            " ".repeat(inner_width.saturating_sub(title.len() + 1))
        ));
    }

    for chunk in field_list.chunks(columns) {
        let mut row = String::new();
        let mut row_width = 0usize;
        for (i, (label, value)) in chunk.iter().enumerate() {
            let cell = format!("{c_accent}{:<label_width$}{c_border}  {c_text}{:<value_width$}{reset}", label, value);
            row.push_str(&cell);
            row_width += label_width + 2 + value_width;
            if i + 1 < chunk.len() {
                row.push_str("  ");
                row_width += 2;
            }
        }
        // "║ " (2) + row + pad + "║" (1) must equal box_width (inner_width + 2).
        let pad = inner_width.saturating_sub(row_width + 1);
        out.push_str(&format!("{c_border}║ {reset}{row}{}{c_border}║{reset}\n", " ".repeat(pad)));
    }

    out.push_str(&format!("{c_border}╚{}╝{reset}", "═".repeat(inner_width)));
    out
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
        }
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
}
