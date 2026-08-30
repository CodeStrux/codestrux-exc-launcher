pub mod filter;
pub mod render;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::config::{CommandEntry, Config, Profile};
use crate::hostinfo::SystemInfo;
use crate::layout;
use crate::theme::ThemeColors;

/// RAII guard: enables raw mode + alt screen on construction, and always
/// restores the terminal on drop (including on panic, via the panic hook
/// installed in `main.rs`), so the picker never leaves the user's shell in a
/// broken state.
struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(RawModeGuard)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

struct App<'a> {
    profiles: &'a [Profile],
    profile_idx: usize,
    query: String,
    filtered: Vec<usize>,
    selected: usize,
    scroll_offset: usize,
}

impl<'a> App<'a> {
    fn new(profiles: &'a [Profile], start_profile: usize) -> Self {
        let mut app = App {
            profiles,
            profile_idx: start_profile.min(profiles.len().saturating_sub(1)),
            query: String::new(),
            filtered: Vec::new(),
            selected: 0,
            scroll_offset: 0,
        };
        app.recompute_filter();
        app
    }

    fn current_profile(&self) -> &Profile {
        &self.profiles[self.profile_idx]
    }

    /// 1-based global id of this profile's first command, minus one (0 for
    /// the first profile, then continuing right after the previous
    /// profile's last command).
    fn profile_base(&self) -> usize {
        crate::config::global_id_of(self.profiles, self.profile_idx, 0) - 1
    }

    fn haystacks(&self) -> Vec<String> {
        self.current_profile()
            .commands
            .iter()
            .map(|c| format!("{} {}", c.name, c.description))
            .collect()
    }

    /// A query that is entirely digits jumps directly to that 1-based
    /// **global** id (matching the numbers shown by `exc list` and the
    /// picker grid — unique across all profiles, not just the current one)
    /// instead of being treated as regex/substring text. If the id belongs
    /// to a different profile than the one currently shown, the picker
    /// switches to it automatically so the item is ready to run — mirroring
    /// how a shell `select` menu lets you type a number to pick an option
    /// directly, but without numbers colliding across profiles.
    fn recompute_filter(&mut self) {
        if !self.query.is_empty() && self.query.bytes().all(|b| b.is_ascii_digit()) {
            let target = self
                .query
                .parse::<usize>()
                .ok()
                .and_then(|id| crate::config::locate_global_id(self.profiles, id));
            match target {
                Some((profile_idx, local_idx)) => {
                    self.profile_idx = profile_idx;
                    self.filtered = vec![local_idx];
                }
                None => self.filtered = Vec::new(),
            }
        } else {
            let hay = self.haystacks();
            self.filtered = filter::filter_indices(&self.query, &hay);
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
        self.scroll_offset = 0;
    }

    fn next_profile(&mut self) {
        self.profile_idx = (self.profile_idx + 1) % self.profiles.len();
        self.query.clear();
        self.selected = 0;
        self.recompute_filter();
    }

    fn prev_profile(&mut self) {
        self.profile_idx = (self.profile_idx + self.profiles.len() - 1) % self.profiles.len();
        self.query.clear();
        self.selected = 0;
        self.recompute_filter();
    }

    fn columns(&self, width: usize) -> usize {
        let names: Vec<&str> = self
            .filtered
            .iter()
            .map(|&i| self.current_profile().commands[i].name.as_str())
            .collect();
        let widest = names.iter().map(|n| layout::display_width(n)).max().unwrap_or(1);
        layout::grid_columns(width, widest + 5, names.len())
    }

    fn move_selection(&mut self, d_row: isize, d_col: isize, columns: usize) {
        if self.filtered.is_empty() {
            return;
        }
        let rows_total = layout::rows_needed(self.filtered.len(), columns);
        let (row, col) = layout::grid_position(self.selected, rows_total);
        let mut new_row = (row as isize + d_row).rem_euclid(rows_total.max(1) as isize) as usize;
        let new_col = (col as isize + d_col).rem_euclid(columns.max(1) as isize) as usize;

        let mut idx = layout::index_from_position(new_row, new_col, rows_total);
        if idx >= self.filtered.len() {
            // the last column may be shorter than rows_total
            if d_row != 0 {
                // wrapped past the bottom of a short column; snap to its top
                new_row = 0;
            } else {
                let items_in_col = self.filtered.len().saturating_sub(new_col * rows_total);
                new_row = items_in_col.saturating_sub(1);
            }
            idx = layout::index_from_position(new_row, new_col, rows_total).min(self.filtered.len() - 1);
        }
        self.selected = idx;
    }

    fn selected_command(&self) -> Option<CommandEntry> {
        self.filtered
            .get(self.selected)
            .map(|&i| self.current_profile().commands[i].clone())
    }

    /// 1-based global id of the currently selected item, matching the
    /// numbers shown in the grid and by `exc list`.
    fn selected_global_id(&self) -> Option<usize> {
        self.filtered.get(self.selected).map(|&local_idx| self.profile_base() + local_idx + 1)
    }

    /// Typing "exit" and pressing Enter quits, mirroring how you'd leave a
    /// shell — this takes priority over running a matching command, even if
    /// some config happens to declare a command literally named "exit".
    fn wants_exit(&self) -> bool {
        self.query.trim().eq_ignore_ascii_case("exit")
    }
}

pub enum PickerOutcome {
    Run(usize, CommandEntry),
    Quit,
}

pub fn run_picker(
    config: &Config,
    start_profile: usize,
    theme: &ThemeColors,
    info: Option<&SystemInfo>,
) -> Result<PickerOutcome> {
    if config.profiles.is_empty() {
        anyhow::bail!("config defines no profiles to show");
    }

    let _guard = RawModeGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(&config.profiles, start_profile);
    let outcome = event_loop(&mut terminal, &mut app, theme, info)?;
    Ok(outcome)
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    theme: &ThemeColors,
    info: Option<&SystemInfo>,
) -> Result<PickerOutcome> {
    loop {
        let mut columns_for_frame = 1usize;
        terminal.draw(|frame| {
            let area = frame.area();
            columns_for_frame = app.columns(area.width as usize).max(1);
            let items: Vec<&CommandEntry> = app.current_profile().commands.iter().collect();
            let ctx = render::DrawContext {
                profile_label: app.current_profile().display_label(),
                profile_index: app.profile_idx,
                profile_count: app.profiles.len(),
                profile_base: app.profile_base(),
                query: &app.query,
                items: &items,
                filtered: &app.filtered,
                selected: app.selected,
                scroll_offset: app.scroll_offset,
                theme,
                info,
            };
            render::draw(frame, &ctx);
        })?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Esc => return Ok(PickerOutcome::Quit),
                    KeyCode::Char('c') if ctrl => return Ok(PickerOutcome::Quit),
                    KeyCode::Enter => {
                        if app.wants_exit() {
                            return Ok(PickerOutcome::Quit);
                        }
                        if let (Some(id), Some(cmd)) = (app.selected_global_id(), app.selected_command()) {
                            return Ok(PickerOutcome::Run(id, cmd));
                        }
                    }
                    KeyCode::Tab => app.next_profile(),
                    KeyCode::BackTab => app.prev_profile(),
                    KeyCode::Up => app.move_selection(-1, 0, columns_for_frame),
                    KeyCode::Down => app.move_selection(1, 0, columns_for_frame),
                    KeyCode::Left => app.move_selection(0, -1, columns_for_frame),
                    KeyCode::Right => app.move_selection(0, 1, columns_for_frame),
                    KeyCode::Char('k') if ctrl => app.move_selection(-1, 0, columns_for_frame),
                    KeyCode::Char('j') if ctrl => app.move_selection(1, 0, columns_for_frame),
                    KeyCode::Char('h') if ctrl => app.move_selection(0, -1, columns_for_frame),
                    KeyCode::Char('l') if ctrl => app.move_selection(0, 1, columns_for_frame),
                    KeyCode::Char('u') if ctrl => {
                        app.query.clear();
                        app.recompute_filter();
                    }
                    KeyCode::Backspace => {
                        app.query.pop();
                        app.recompute_filter();
                    }
                    KeyCode::Char(c) if !ctrl => {
                        app.query.push(c);
                        app.recompute_filter();
                    }
                    _ => {}
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_with(names: &[&str]) -> Profile {
        Profile {
            name: "p".to_string(),
            label: None,
            description: None,
            commands: names
                .iter()
                .map(|n| CommandEntry {
                    name: n.to_string(),
                    description: String::new(),
                    command: "true".to_string(),
                    params: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn all_digit_query_jumps_to_that_one_based_position() {
        let profiles = vec![profile_with(&["a", "b", "c", "d"])];
        let mut app = App::new(&profiles, 0);
        app.query = "3".to_string();
        app.recompute_filter();
        assert_eq!(app.filtered, vec![2]);
        assert_eq!(app.selected_command().unwrap().name, "c");
    }

    #[test]
    fn out_of_range_digit_query_matches_nothing() {
        let profiles = vec![profile_with(&["a", "b"])];
        let mut app = App::new(&profiles, 0);
        app.query = "9".to_string();
        app.recompute_filter();
        assert!(app.filtered.is_empty());
    }

    #[test]
    fn mixed_alnum_query_falls_back_to_text_filter() {
        let profiles = vec![profile_with(&["gti-pve-00", "pinggy-3000"])];
        let mut app = App::new(&profiles, 0);
        app.query = "3000".to_string();
        app.recompute_filter();
        // pure digits -> id-jump mode, out of range (only 2 items) -> no match
        assert!(app.filtered.is_empty());
        app.query = "pinggy".to_string();
        app.recompute_filter();
        assert_eq!(app.filtered, vec![1]);
    }

    #[test]
    fn digit_query_resolves_across_profiles_and_switches_profile() {
        let profiles = vec![profile_with(&["a", "b"]), profile_with(&["c", "d", "e"])];
        let mut app = App::new(&profiles, 0);

        // #2 is still in the first profile.
        app.query = "2".to_string();
        app.recompute_filter();
        assert_eq!(app.profile_idx, 0);
        assert_eq!(app.selected_command().unwrap().name, "b");

        // #3 is the *first* item of the second profile (continues right
        // after #2), not a reset-to-#1 "c" — and selecting it switches the
        // active profile automatically so it's ready to run.
        app.query = "3".to_string();
        app.recompute_filter();
        assert_eq!(app.profile_idx, 1);
        assert_eq!(app.selected_command().unwrap().name, "c");

        // #5 is the last item of the second profile.
        app.query = "5".to_string();
        app.recompute_filter();
        assert_eq!(app.profile_idx, 1);
        assert_eq!(app.selected_command().unwrap().name, "e");

        // #6 doesn't exist anywhere.
        app.query = "6".to_string();
        app.recompute_filter();
        assert!(app.filtered.is_empty());
    }

    #[test]
    fn profile_base_accounts_for_earlier_profiles() {
        let profiles = vec![profile_with(&["a", "b"]), profile_with(&["c", "d", "e"])];
        let mut app = App::new(&profiles, 0);
        assert_eq!(app.profile_base(), 0);
        app.next_profile();
        assert_eq!(app.profile_base(), 2);
    }

    #[test]
    fn clearing_query_restores_full_list() {
        let profiles = vec![profile_with(&["a", "b", "c"])];
        let mut app = App::new(&profiles, 0);
        app.query = "2".to_string();
        app.recompute_filter();
        assert_eq!(app.filtered, vec![1]);
        app.query.clear();
        app.recompute_filter();
        assert_eq!(app.filtered, vec![0, 1, 2]);
    }

    #[test]
    fn typing_exit_wants_to_quit() {
        let profiles = vec![profile_with(&["a", "b", "c"])];
        let mut app = App::new(&profiles, 0);
        for query in ["exit", "EXIT", "ExIt", " exit "] {
            app.query = query.to_string();
            assert!(app.wants_exit(), "{query:?} should be treated as a quit request");
        }
    }

    #[test]
    fn queries_other_than_exit_do_not_want_to_quit() {
        let profiles = vec![profile_with(&["a", "b", "c"])];
        let mut app = App::new(&profiles, 0);
        for query in ["", "a", "exiting", "exi", "quit"] {
            app.query = query.to_string();
            assert!(!app.wants_exit(), "{query:?} should not be treated as a quit request");
        }
    }
}
