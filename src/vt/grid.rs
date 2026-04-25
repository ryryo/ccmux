use std::path::PathBuf;

use super::cell::{Cell, CellAttrs, Color};
use super::line::LogicalLine;
use super::scrollback::Scrollback;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub row: u16,
    pub col: u16,
    pub style: CellAttrs,
    pub fg: Color,
    pub bg: Color,
    pub visible: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            style: CellAttrs::default(),
            fg: Color::Default,
            bg: Color::Default,
            visible: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Buffer {
    pub visible: Vec<LogicalLine>,
    pub scrollback: Scrollback,
    pub cols: u16,
}

impl Buffer {
    pub fn new(rows: u16, cols: u16, scrollback_max: usize) -> Self {
        let visible = (0..rows).map(|_| LogicalLine::new(cols as usize)).collect();
        Self {
            visible,
            scrollback: Scrollback::new(scrollback_max),
            cols,
        }
    }

    pub fn rows(&self) -> u16 {
        self.visible.len() as u16
    }

    pub fn resize_visible(&mut self, rows: u16, cols: u16) {
        self.cols = cols;
        let target = rows as usize;
        if self.visible.len() < target {
            let extra = target - self.visible.len();
            for _ in 0..extra {
                self.visible.push(LogicalLine::new(cols as usize));
            }
        } else if self.visible.len() > target {
            self.visible.truncate(target);
        }
    }

    /// Scroll `count` lines off the top. If `save_to_scrollback`, push removed lines to
    /// scrollback (oldest first); otherwise discard. Append blank lines at bottom to
    /// preserve row count.
    pub fn scroll_lines_off_top(&mut self, count: u16, save_to_scrollback: bool) {
        let count = (count as usize).min(self.visible.len());
        for _ in 0..count {
            let removed = self.visible.remove(0);
            if save_to_scrollback {
                self.scrollback.push(removed);
            }
            self.visible.push(LogicalLine::new(self.cols as usize));
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalModes {
    pub bracketed_paste: bool,
    pub mouse_x10: bool,
    pub mouse_button: bool,
    pub mouse_motion: bool,
    pub mouse_sgr_encoding: bool,
    pub auto_wrap: bool,
}

impl TerminalModes {
    pub fn defaults() -> Self {
        Self {
            auto_wrap: true,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HyperlinkRegistry {
    pub entries: Vec<String>,
    pub current: u32,
}

impl HyperlinkRegistry {
    pub fn register(&mut self, uri: &str) -> u32 {
        if uri.is_empty() {
            return 0;
        }
        if let Some(pos) = self.entries.iter().position(|e| e == uri) {
            return (pos + 1) as u32;
        }
        self.entries.push(uri.to_string());
        self.entries.len() as u32
    }

    pub fn get(&self, id: u32) -> Option<&str> {
        if id == 0 {
            return None;
        }
        self.entries.get((id - 1) as usize).map(|s| s.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct Grid {
    pub rows: u16,
    pub cols: u16,
    pub cursor: Cursor,
    pub saved_cursor: Option<Cursor>,
    pub scroll_top: u16,
    pub scroll_bottom: u16,
    pub primary: Buffer,
    pub alternate: Buffer,
    pub use_alternate: bool,
    pub modes: TerminalModes,
    pub hyperlinks: HyperlinkRegistry,
    pub title: String,
    pub cwd: Option<PathBuf>,
}

impl Grid {
    pub fn new(rows: u16, cols: u16, scrollback_max: usize) -> Self {
        Self {
            rows,
            cols,
            cursor: Cursor::default(),
            saved_cursor: None,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            primary: Buffer::new(rows, cols, scrollback_max),
            alternate: Buffer::new(rows, cols, 0),
            use_alternate: false,
            modes: TerminalModes::defaults(),
            hyperlinks: HyperlinkRegistry::default(),
            title: String::new(),
            cwd: None,
        }
    }

    pub fn current_buffer(&self) -> &Buffer {
        if self.use_alternate { &self.alternate } else { &self.primary }
    }

    pub fn current_buffer_mut(&mut self) -> &mut Buffer {
        if self.use_alternate { &mut self.alternate } else { &mut self.primary }
    }

    pub fn enter_alternate(&mut self) {
        if self.use_alternate {
            return;
        }
        self.saved_cursor = Some(self.cursor);
        self.alternate = Buffer::new(self.rows, self.cols, 0);
        self.use_alternate = true;
        self.cursor = Cursor::default();
    }

    pub fn exit_alternate(&mut self) {
        if !self.use_alternate {
            return;
        }
        self.use_alternate = false;
        if let Some(saved) = self.saved_cursor.take() {
            self.cursor = saved;
        }
    }

    pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let bottom = bottom.min(self.rows.saturating_sub(1));
        if top >= bottom {
            return;
        }
        self.scroll_top = top;
        self.scroll_bottom = bottom;
    }

    pub fn reset_scroll_region(&mut self) {
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
    }

    /// xterm rule: scrollback save iff alt screen inactive AND scroll_top == 0.
    pub fn should_save_to_scrollback(&self) -> bool {
        !self.use_alternate && self.scroll_top == 0
    }

    /// Scroll the active scroll region up by `count` lines. Lines pushed off
    /// the top of the region are saved to scrollback iff `should_save_to_scrollback`
    /// (xterm rule: alt screen inactive AND scroll_top == 0). Blank lines are
    /// inserted at the bottom of the region.
    pub fn scroll_up_in_region(&mut self, count: u16) {
        if count == 0 {
            return;
        }
        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom as usize;
        if top >= bottom {
            return;
        }
        let save = self.should_save_to_scrollback();
        let cols = self.cols as usize;
        let count = (count as usize).min(bottom - top + 1);
        let buf = self.current_buffer_mut();
        for _ in 0..count {
            if top >= buf.visible.len() || bottom >= buf.visible.len() {
                break;
            }
            let removed = buf.visible.remove(top);
            if save {
                buf.scrollback.push(removed);
            }
            buf.visible.insert(bottom, LogicalLine::new(cols));
        }
    }

    pub fn blank_cell(&self) -> Cell {
        Cell {
            ch: ' ',
            width: 1,
            fg: self.cursor.fg,
            bg: self.cursor.bg,
            attrs: CellAttrs::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_new_has_rows() {
        let b = Buffer::new(5, 80, 100);
        assert_eq!(b.visible.len(), 5);
        assert_eq!(b.cols, 80);
    }

    #[test]
    fn resize_visible_grows_and_shrinks() {
        let mut b = Buffer::new(3, 80, 100);
        b.resize_visible(5, 80);
        assert_eq!(b.visible.len(), 5);
        b.resize_visible(2, 80);
        assert_eq!(b.visible.len(), 2);
    }

    #[test]
    fn scroll_lines_off_top_to_scrollback() {
        let mut b = Buffer::new(5, 10, 100);
        // mark first two lines so we can detect them in scrollback
        b.visible[0].cells[0].ch = 'A';
        b.visible[1].cells[0].ch = 'B';
        b.scroll_lines_off_top(2, true);
        assert_eq!(b.visible.len(), 5);
        assert_eq!(b.scrollback.len(), 2);
        assert_eq!(b.scrollback.get(0).unwrap().cells[0].ch, 'A');
        assert_eq!(b.scrollback.get(1).unwrap().cells[0].ch, 'B');
    }

    #[test]
    fn scroll_lines_off_top_discard() {
        let mut b = Buffer::new(5, 10, 100);
        b.scroll_lines_off_top(2, false);
        assert_eq!(b.scrollback.len(), 0);
        assert_eq!(b.visible.len(), 5);
    }

    #[test]
    fn grid_initial_scroll_region() {
        let g = Grid::new(24, 80, 1000);
        assert_eq!(g.scroll_top, 0);
        assert_eq!(g.scroll_bottom, 23);
        assert!(!g.use_alternate);
        assert!(g.modes.auto_wrap);
    }

    #[test]
    fn enter_exit_alternate_switches_buffer() {
        let mut g = Grid::new(24, 80, 1000);
        g.cursor.row = 5;
        g.cursor.col = 10;
        g.enter_alternate();
        assert!(g.use_alternate);
        assert_eq!(g.cursor.row, 0);
        assert_eq!(g.cursor.col, 0);
        g.exit_alternate();
        assert!(!g.use_alternate);
        assert_eq!(g.cursor.row, 5);
        assert_eq!(g.cursor.col, 10);
    }

    #[test]
    fn set_scroll_region_clamps() {
        let mut g = Grid::new(24, 80, 1000);
        g.set_scroll_region(2, 10);
        assert_eq!(g.scroll_top, 2);
        assert_eq!(g.scroll_bottom, 10);
        g.set_scroll_region(2, 100);
        assert_eq!(g.scroll_bottom, 23);
    }

    #[test]
    fn invalid_scroll_region_ignored() {
        let mut g = Grid::new(24, 80, 1000);
        g.set_scroll_region(2, 10);
        g.set_scroll_region(10, 5);
        assert_eq!(g.scroll_top, 2);
        assert_eq!(g.scroll_bottom, 10);
    }

    #[test]
    fn should_save_to_scrollback_xterm_rule() {
        let mut g = Grid::new(24, 80, 1000);
        assert!(g.should_save_to_scrollback());
        g.set_scroll_region(2, 23);
        assert!(!g.should_save_to_scrollback());
        g.reset_scroll_region();
        assert!(g.should_save_to_scrollback());
        g.enter_alternate();
        assert!(!g.should_save_to_scrollback());
    }

    #[test]
    fn scroll_up_in_region_default_saves_scrollback() {
        let mut g = Grid::new(4, 5, 100);
        g.primary.visible[0].cells[0].ch = 'A';
        g.scroll_up_in_region(1);
        assert_eq!(g.primary.scrollback.len(), 1);
        assert_eq!(g.primary.scrollback.get(0).unwrap().cells[0].ch, 'A');
        assert_eq!(g.primary.visible.len(), 4);
    }

    #[test]
    fn scroll_up_in_region_with_decstbm_top_zero_still_saves() {
        // Mimics Claude Code: scroll_top=0, scroll_bottom < rows-1.
        let mut g = Grid::new(5, 5, 100);
        g.set_scroll_region(0, 3);
        g.primary.visible[0].cells[0].ch = 'A';
        g.primary.visible[4].cells[0].ch = 'X'; // out-of-region; must not move
        g.scroll_up_in_region(1);
        assert_eq!(g.primary.scrollback.len(), 1);
        assert_eq!(g.primary.scrollback.get(0).unwrap().cells[0].ch, 'A');
        assert_eq!(g.primary.visible[4].cells[0].ch, 'X');
    }

    #[test]
    fn scroll_up_in_region_top_nonzero_does_not_save() {
        let mut g = Grid::new(5, 5, 100);
        g.set_scroll_region(1, 4);
        g.primary.visible[1].cells[0].ch = 'A';
        g.scroll_up_in_region(1);
        assert_eq!(g.primary.scrollback.len(), 0);
    }

    #[test]
    fn scroll_up_in_region_alt_screen_does_not_save() {
        let mut g = Grid::new(4, 5, 100);
        g.enter_alternate();
        g.alternate.visible[0].cells[0].ch = 'A';
        g.scroll_up_in_region(1);
        assert_eq!(g.primary.scrollback.len(), 0);
        assert_eq!(g.alternate.scrollback.len(), 0);
    }

    #[test]
    fn hyperlink_registry_dedups() {
        let mut h = HyperlinkRegistry::default();
        let a = h.register("https://a");
        let b = h.register("https://b");
        let a2 = h.register("https://a");
        assert_eq!(a, a2);
        assert_ne!(a, b);
        assert_eq!(h.get(a), Some("https://a"));
        assert_eq!(h.get(0), None);
    }
}
