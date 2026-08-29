use ratatui::layout::Rect;
use ratatui::style::{Color as RColor, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::config::CommandEntry;
use crate::hostinfo::{self, SystemInfo};
use crate::layout;
use crate::theme::ThemeColors;

pub fn conv_color(c: crossterm::style::Color) -> RColor {
    use crossterm::style::Color as CC;
    match c {
        CC::Black => RColor::Black,
        CC::DarkGrey => RColor::DarkGray,
        CC::Red => RColor::LightRed,
        CC::DarkRed => RColor::Red,
        CC::Green => RColor::LightGreen,
        CC::DarkGreen => RColor::Green,
        CC::Yellow => RColor::LightYellow,
        CC::DarkYellow => RColor::Yellow,
        CC::Blue => RColor::LightBlue,
        CC::DarkBlue => RColor::Blue,
        CC::Magenta => RColor::LightMagenta,
        CC::DarkMagenta => RColor::Magenta,
        CC::Cyan => RColor::LightCyan,
        CC::DarkCyan => RColor::Cyan,
        CC::White => RColor::White,
        CC::Grey => RColor::Gray,
        CC::Rgb { r, g, b } => RColor::Rgb(r, g, b),
        _ => RColor::White,
    }
}

pub struct DrawContext<'a> {
    pub profile_label: &'a str,
    pub profile_index: usize,
    pub profile_count: usize,
    /// 1-based global id of this profile's first command; added to a
    /// command's position within the profile to label it with its true,
    /// cross-profile-unique id (see `crate::config::global_id_of`).
    pub profile_base: usize,
    pub query: &'a str,
    pub items: &'a [&'a CommandEntry],
    pub filtered: &'a [usize],
    pub selected: usize,
    pub scroll_offset: usize,
    pub theme: &'a ThemeColors,
    /// `None` when the panel is hidden via `--no-sysinfo`.
    pub info: Option<&'a SystemInfo>,
}

pub fn draw(frame: &mut Frame, ctx: &DrawContext) {
    let area = frame.area();
    let border = conv_color(ctx.theme.border);
    let accent = conv_color(ctx.theme.accent);
    let text_c = conv_color(ctx.theme.text);
    let muted = conv_color(ctx.theme.muted);
    let selected_bg = conv_color(ctx.theme.selected_bg);
    let selected_fg = conv_color(ctx.theme.selected_fg);

    let info_layout = ctx.info.map(|info| hostinfo::panel::layout(info, area.width));
    let info_height = info_layout.as_ref().map(|l| l.len() as u16).unwrap_or(0);

    let filter_height = 1u16;
    let footer_height = 1u16;
    let grid_height = area
        .height
        .saturating_sub(info_height + filter_height + footer_height + 1);

    // Info box (skipped entirely when the panel is hidden). Each segment
    // keeps the color its role (border/label/value) maps to, so labels and
    // values read as visually distinct instead of one flat color.
    let info_area = Rect { x: area.x, y: area.y, width: area.width, height: info_height.min(area.height) };
    if let Some(info_layout) = info_layout {
        let info_lines: Vec<Line> = info_layout
            .into_iter()
            .map(|segments| {
                let spans: Vec<Span> = segments
                    .into_iter()
                    .map(|seg| {
                        let color = match seg.role {
                            hostinfo::panel::Role::Border => border,
                            hostinfo::panel::Role::Accent => accent,
                            hostinfo::panel::Role::Text => text_c,
                        };
                        Span::styled(seg.text, Style::default().fg(color))
                    })
                    .collect();
                Line::from(spans)
            })
            .collect();
        frame.render_widget(Paragraph::new(info_lines), info_area);
    }

    // Filter/profile line.
    let filter_y = info_area.y + info_area.height;
    let filter_area = Rect { x: area.x, y: filter_y, width: area.width, height: filter_height };
    let profile_tab = format!(
        "[{}/{}] {}",
        ctx.profile_index + 1,
        ctx.profile_count,
        ctx.profile_label
    );
    let is_id_query = !ctx.query.is_empty() && ctx.query.bytes().all(|b| b.is_ascii_digit());
    let prompt_glyph = if is_id_query { "#" } else { "/" };
    let filter_line = Line::from(vec![
        Span::styled(profile_tab, Style::default().fg(accent).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(prompt_glyph, Style::default().fg(muted)),
        Span::styled(ctx.query.to_string(), Style::default().fg(text_c)),
        Span::styled("_", Style::default().fg(muted)),
    ]);
    frame.render_widget(Paragraph::new(filter_line), filter_area);

    // Grid.
    let grid_y = filter_area.y + filter_area.height + 1;
    let grid_area = Rect { x: area.x, y: grid_y, width: area.width, height: grid_height };
    let grid_block = Block::default().borders(Borders::NONE);
    frame.render_widget(&grid_block, grid_area);

    if ctx.filtered.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("(no matches)", Style::default().fg(muted))),
            grid_area,
        );
    } else {
        let names: Vec<&str> = ctx.filtered.iter().map(|&i| ctx.items[i].name.as_str()).collect();
        let widest = names.iter().map(|n| layout::display_width(n)).max().unwrap_or(1);
        let columns = layout::grid_columns(grid_area.width as usize, widest + 5, names.len());
        let rows_total = layout::rows_needed(names.len(), columns);
        let visible_rows = grid_area.height as usize;

        let (sel_row, _) = layout::grid_position(ctx.selected, rows_total);
        let scroll = if sel_row < ctx.scroll_offset {
            sel_row
        } else if sel_row >= ctx.scroll_offset + visible_rows {
            sel_row + 1 - visible_rows.max(1)
        } else {
            ctx.scroll_offset
        };

        let mut lines = Vec::new();
        for row in scroll..(scroll + visible_rows).min(rows_total) {
            let mut spans = Vec::new();
            for col in 0..columns {
                let idx = layout::index_from_position(row, col, rows_total);
                if idx >= names.len() {
                    break;
                }
                // `idx` is a position within the filtered/displayed subset;
                // the label shows the item's true global id (stable across
                // profiles and filtering), not that display-relative position.
                let global_id = ctx.profile_base + ctx.filtered[idx] + 1;
                let label = format!("[{:>2}] {:<width$}", global_id, names[idx], width = widest);
                let style = if idx == ctx.selected {
                    Style::default().fg(selected_fg).bg(selected_bg).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(text_c)
                };
                spans.push(Span::styled(label, style));
                spans.push(Span::raw("  "));
            }
            lines.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(lines), grid_area);
    }

    // Footer.
    let footer_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    let footer = "type # id or text  Ctrl-U clear  ↑↓←→/Ctrl-jkhl move  Enter run  Tab profile  Esc/Ctrl-C quit";
    frame.render_widget(
        Paragraph::new(Span::styled(footer, Style::default().fg(border))),
        footer_area,
    );
}
