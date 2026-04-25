use std::path::PathBuf;

use vte::{Params, Perform};

use super::cell::Cell;
use super::grid::Grid;
use super::width::char_width;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalEvent {
    TitleChanged(String),
    CwdChanged(PathBuf),
    Bell,
    ClipboardWrite(String),
    ClipboardReadRequested,
}

pub struct Terminal {
    pub grid: Grid,
    parser: vte::Parser,
    pending_events: Vec<TerminalEvent>,
}

impl Terminal {
    pub fn new(rows: u16, cols: u16, scrollback_max: usize) -> Self {
        Self {
            grid: Grid::new(rows, cols, scrollback_max),
            parser: vte::Parser::new(),
            pending_events: Vec::new(),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        let Self { grid, parser, pending_events } = self;
        let mut performer = Performer { grid, events: pending_events };
        for &b in bytes {
            parser.advance(&mut performer, b);
        }
    }

    pub fn drain_events(&mut self) -> Vec<TerminalEvent> {
        std::mem::take(&mut self.pending_events)
    }
}

struct Performer<'a> {
    grid: &'a mut Grid,
    events: &'a mut Vec<TerminalEvent>,
}

impl Performer<'_> {
    /// Wrap to the next line. Marks the *destination* line as `continued`
    /// (per `LogicalLine` semantics: a line flags itself as continuing from
    /// the previous logical line). Scrolls when at the bottom of the region.
    fn wrap_to_next_line(&mut self) {
        if self.grid.cursor.row >= self.grid.scroll_bottom {
            self.grid.scroll_up_in_region(1);
        } else {
            self.grid.cursor.row += 1;
        }
        self.grid.cursor.col = 0;
        let row = self.grid.cursor.row as usize;
        let buf = self.grid.current_buffer_mut();
        if let Some(line) = buf.visible.get_mut(row) {
            line.continued = true;
        }
    }

    /// LF / VT / FF: advance row; scroll if at the bottom of the scroll region.
    /// Column is unchanged (CR is a separate control).
    fn line_feed(&mut self) {
        if self.grid.cursor.row >= self.grid.scroll_bottom {
            self.grid.scroll_up_in_region(1);
        } else {
            self.grid.cursor.row += 1;
        }
    }
}

impl Perform for Performer<'_> {
    fn print(&mut self, c: char) {
        let cols = self.grid.cols;
        if cols == 0 {
            return;
        }
        let w = char_width(c);
        if w == 0 {
            return;
        }
        let auto_wrap = self.grid.modes.auto_wrap;

        // Pending-wrap: cursor parked past last column from previous full-row write.
        if self.grid.cursor.col >= cols {
            if auto_wrap {
                self.wrap_to_next_line();
            } else {
                self.grid.cursor.col = cols - 1;
            }
        }

        // Wide char that doesn't fit on current row.
        if self.grid.cursor.col + w as u16 > cols {
            if auto_wrap {
                self.wrap_to_next_line();
            } else {
                self.grid.cursor.col = cols.saturating_sub(w as u16);
            }
        }

        let row = self.grid.cursor.row.min(self.grid.rows.saturating_sub(1));
        let col = self.grid.cursor.col;
        let cell = Cell {
            ch: c,
            width: w,
            fg: self.grid.cursor.fg,
            bg: self.grid.cursor.bg,
            attrs: self.grid.cursor.style,
        };
        let cols_usize = cols as usize;
        let buf = self.grid.current_buffer_mut();
        if let Some(line) = buf.visible.get_mut(row as usize) {
            if line.cells.len() < cols_usize {
                line.cells.resize(cols_usize, Cell::default());
            }
            line.set_cell(col as usize, cell);
        }

        let new_col = col + w as u16;
        if auto_wrap {
            // May park at cols (pending-wrap state).
            self.grid.cursor.col = new_col;
        } else {
            self.grid.cursor.col = new_col.min(cols - 1);
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.events.push(TerminalEvent::Bell),
            0x08 => {
                // BS: clamp parked cursor first, then move back.
                let cols = self.grid.cols;
                if self.grid.cursor.col >= cols && cols > 0 {
                    self.grid.cursor.col = cols - 1;
                }
                self.grid.cursor.col = self.grid.cursor.col.saturating_sub(1);
            }
            0x09 => {
                // HT: next multiple-of-8 tabstop, clamped to cols-1.
                let cols = self.grid.cols;
                if cols == 0 {
                    return;
                }
                let next = (self.grid.cursor.col / 8 + 1) * 8;
                self.grid.cursor.col = next.min(cols - 1);
            }
            0x0A | 0x0B | 0x0C => self.line_feed(),
            0x0D => self.grid.cursor.col = 0,
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        super::osc::dispatch(self.grid, params, self.events);
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if intermediates.first() == Some(&b'?') {
            match action {
                'h' => super::csi::dispatch_private_mode(self.grid, params, true),
                'l' => super::csi::dispatch_private_mode(self.grid, params, false),
                _ => {}
            }
            return;
        }
        super::csi::dispatch(self.grid, params, action);
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'c' => {
                // RIS: full reset.
                if self.grid.use_alternate {
                    self.grid.exit_alternate();
                }
                self.grid.cursor = super::grid::Cursor::default();
                self.grid.saved_cursor = None;
                self.grid.reset_scroll_region();
                self.grid.modes = super::grid::TerminalModes::defaults();
                let cols = self.grid.cols as usize;
                let rows = self.grid.rows as usize;
                let buf = self.grid.current_buffer_mut();
                for r in 0..rows.min(buf.visible.len()) {
                    buf.visible[r] = super::line::LogicalLine::new(cols);
                }
            }
            b'7' => self.grid.saved_cursor = Some(self.grid.cursor),
            b'8' => {
                if let Some(c) = self.grid.saved_cursor {
                    self.grid.cursor = c;
                }
            }
            b'M' => {
                // RI: reverse line feed.
                if self.grid.cursor.row == self.grid.scroll_top {
                    let top = self.grid.scroll_top as usize;
                    let bottom = self.grid.scroll_bottom as usize;
                    let cols = self.grid.cols as usize;
                    let buf = self.grid.current_buffer_mut();
                    if bottom < buf.visible.len() {
                        buf.visible.remove(bottom);
                        buf.visible
                            .insert(top, super::line::LogicalLine::new(cols));
                    }
                } else {
                    self.grid.cursor.row = self.grid.cursor.row.saturating_sub(1);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_writes_char_and_advances_cursor() {
        let mut t = Terminal::new(5, 10, 100);
        t.process(b"A");
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, 'A');
        assert_eq!(t.grid.cursor.col, 1);
        assert_eq!(t.grid.cursor.row, 0);
    }

    #[test]
    fn print_multiple_chars() {
        let mut t = Terminal::new(5, 10, 100);
        t.process(b"ABC");
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, 'A');
        assert_eq!(t.grid.primary.visible[0].cells[1].ch, 'B');
        assert_eq!(t.grid.primary.visible[0].cells[2].ch, 'C');
        assert_eq!(t.grid.cursor.col, 3);
    }

    #[test]
    fn drain_events_returns_pending() {
        let mut t = Terminal::new(5, 10, 100);
        let evs = t.drain_events();
        assert!(evs.is_empty());
    }

    #[test]
    fn auto_wrap_to_next_line() {
        // cols=5: ABCDE fills row 0, F lands at (1,0).
        let mut t = Terminal::new(3, 5, 100);
        t.process("ABCDEF".as_bytes());
        assert_eq!(t.grid.primary.visible[0].cells[4].ch, 'E');
        assert_eq!(t.grid.primary.visible[1].cells[0].ch, 'F');
        assert_eq!(t.grid.cursor.row, 1);
        assert_eq!(t.grid.cursor.col, 1);
        assert!(t.grid.primary.visible[1].continued);
        assert!(!t.grid.primary.visible[0].continued);
    }

    #[test]
    fn cjk_wide_writes_continuation() {
        let mut t = Terminal::new(3, 5, 100);
        t.process("あ".as_bytes());
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, 'あ');
        assert_eq!(t.grid.primary.visible[0].cells[0].width, 2);
        assert!(t.grid.primary.visible[0].cells[1].is_continuation());
        assert_eq!(t.grid.cursor.col, 2);
    }

    #[test]
    fn cjk_wide_wraps_when_no_room() {
        // cols=3, "Aあ": A at col 0, あ doesn't fit (col 1 + width 2 = 3, OK)
        // Use cols=2: 'A' at col 0, then 'あ' (w=2) at col 1: 1+2=3>2, wrap.
        let mut t = Terminal::new(3, 2, 100);
        t.process("Aあ".as_bytes());
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, 'A');
        assert_eq!(t.grid.primary.visible[1].cells[0].ch, 'あ');
        assert_eq!(t.grid.cursor.row, 1);
        assert_eq!(t.grid.cursor.col, 2);
    }

    #[test]
    fn auto_wrap_disabled_overwrites_at_last_col() {
        let mut t = Terminal::new(3, 5, 100);
        t.grid.modes.auto_wrap = false;
        t.process(b"ABCDEFG");
        // E,F,G all land on col 4
        assert_eq!(t.grid.primary.visible[0].cells[3].ch, 'D');
        assert_eq!(t.grid.primary.visible[0].cells[4].ch, 'G');
        assert_eq!(t.grid.cursor.row, 0);
        assert_eq!(t.grid.cursor.col, 4);
    }

    #[test]
    fn lf_then_cr_moves_to_next_line_start() {
        let mut t = Terminal::new(3, 5, 100);
        t.process(b"A\r\nB");
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, 'A');
        assert_eq!(t.grid.primary.visible[1].cells[0].ch, 'B');
        assert_eq!(t.grid.cursor.row, 1);
        assert_eq!(t.grid.cursor.col, 1);
    }

    #[test]
    fn lf_at_bottom_scrolls_into_scrollback() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"row0\r\nrow1\r\nrow2");
        assert_eq!(t.grid.primary.scrollback.len(), 1);
        assert_eq!(t.grid.primary.scrollback.get(0).unwrap().cells[0].ch, 'r');
        assert_eq!(t.grid.primary.scrollback.get(0).unwrap().cells[3].ch, '0');
        assert_eq!(t.grid.primary.visible[1].cells[0].ch, 'r');
        assert_eq!(t.grid.primary.visible[1].cells[3].ch, '2');
    }

    #[test]
    fn bs_does_not_underflow() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x08\x08\x08");
        assert_eq!(t.grid.cursor.col, 0);
    }

    #[test]
    fn ht_advances_to_next_tabstop() {
        let mut t = Terminal::new(2, 20, 100);
        t.process(b"A\t");
        assert_eq!(t.grid.cursor.col, 8);
        t.process(b"\t");
        assert_eq!(t.grid.cursor.col, 16);
    }

    #[test]
    fn ht_clamps_to_last_col() {
        let mut t = Terminal::new(2, 10, 100);
        t.process(b"\t\t");
        assert_eq!(t.grid.cursor.col, 9);
    }

    #[test]
    fn bel_emits_event() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x07");
        let evs = t.drain_events();
        assert_eq!(evs, vec![TerminalEvent::Bell]);
    }

    #[test]
    fn ris_resets_grid() {
        let mut t = Terminal::new(5, 10, 100);
        t.process(b"hello\r\nworld");
        t.process(b"\x1b[1;31m");
        t.process(b"\x1bc");
        assert_eq!(t.grid.cursor.row, 0);
        assert_eq!(t.grid.cursor.col, 0);
        assert_eq!(t.grid.cursor.fg, super::super::cell::Color::Default);
        assert_eq!(t.grid.cursor.style.bits, 0);
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, ' ');
        assert_eq!(t.grid.primary.visible[1].cells[0].ch, ' ');
    }

    #[test]
    fn decsc_decrc_round_trip() {
        let mut t = Terminal::new(5, 10, 100);
        t.process(b"\x1b[3;5H");
        t.process(b"\x1b7");
        t.process(b"\x1b[1;1H");
        t.process(b"\x1b8");
        assert_eq!(t.grid.cursor.row, 2);
        assert_eq!(t.grid.cursor.col, 4);
    }

    #[test]
    fn ri_at_scroll_top_inserts_blank() {
        let mut t = Terminal::new(3, 5, 100);
        t.process(b"row0\r\nrow1\r\nrow2");
        t.process(b"\x1b[1;1H"); // back to top
        t.process(b"\x1bM");
        assert_eq!(t.grid.cursor.row, 0);
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, ' ');
        assert_eq!(t.grid.primary.visible[1].cells[0].ch, 'r');
        assert_eq!(t.grid.primary.visible[1].cells[3].ch, '0');
    }

    #[test]
    fn ri_above_scroll_top_decrements_row() {
        let mut t = Terminal::new(3, 5, 100);
        t.process(b"\x1b[3;1H");
        t.process(b"\x1bM");
        assert_eq!(t.grid.cursor.row, 1);
    }

    #[test]
    fn combining_char_is_skipped() {
        let mut t = Terminal::new(3, 5, 100);
        // 'a' + combining acute (U+0301)
        t.process("a\u{0301}".as_bytes());
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, 'a');
        assert_eq!(t.grid.cursor.col, 1);
    }
}
