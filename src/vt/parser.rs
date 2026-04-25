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

    /// Resize the terminal to (rows, cols). Logical lines are preserved as-is —
    /// reflow runs at draw time. We only adjust visible row count, clamp the
    /// cursor, and clamp the scroll region.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let cursor_row = self.grid.cursor.row;
        let on_alt = self.grid.use_alternate;

        // Shrink: if the cursor would fall off the bottom, scroll lines off the top
        // (saving to scrollback only for the primary buffer) so cursor content is
        // preserved. Otherwise truncate trailing blank rows. Grow: append blanks.
        if cursor_row >= rows {
            let drop = cursor_row + 1 - rows;
            self.grid.primary.scroll_lines_off_top(drop, !on_alt);
            self.grid.alternate.scroll_lines_off_top(drop, false);
            self.grid.cursor.row = rows - 1;
        }
        self.grid.rows = rows;
        self.grid.cols = cols;
        self.grid.primary.resize_visible(rows, cols);
        self.grid.alternate.resize_visible(rows, cols);

        if self.grid.cursor.row >= rows {
            self.grid.cursor.row = rows - 1;
        }
        if self.grid.cursor.col >= cols {
            self.grid.cursor.col = cols - 1;
        }

        let max_top = rows.saturating_sub(2);
        let max_bottom = rows - 1;
        if self.grid.scroll_top > max_top {
            self.grid.scroll_top = max_top;
        }
        if self.grid.scroll_bottom > max_bottom {
            self.grid.scroll_bottom = max_bottom;
        }
        if self.grid.scroll_top >= self.grid.scroll_bottom {
            self.grid.scroll_top = 0;
            self.grid.scroll_bottom = max_bottom;
        }
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
            0x0A..=0x0C => self.line_feed(),
            0x0D => self.grid.cursor.col = 0,
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // CCMUX_TRACE_OSC=<path> で全 OSC をファイルに追記。
        if let Some(path) = std::env::var_os("CCMUX_TRACE_OSC") {
            use std::io::Write;
            let pretty: Vec<String> = params
                .iter()
                .map(|p| String::from_utf8_lossy(p).into_owned())
                .collect();
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                let _ = writeln!(f, "OSC params={pretty:?}");
            }
        }
        super::osc::dispatch(self.grid, params, self.events);
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        // CCMUX_TRACE_CSI=<path> で全 CSI 'm' をファイルに追記。診断用。
        if action == 'm' {
            if let Some(path) = std::env::var_os("CCMUX_TRACE_CSI") {
                use std::io::Write;
                let pretty: Vec<Vec<u16>> = params.iter().map(|g| g.to_vec()).collect();
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                    let _ = writeln!(
                        f,
                        "CSI m intermediates={intermediates:?} params={pretty:?} underline_before={}",
                        self.grid.cursor.style.contains(crate::vt::cell::CellAttrs::UNDERLINE)
                    );
                }
            }
        }
        // CSI with non-empty intermediates is a private / xterm-extension
        // sequence (ECMA-48 §5.4). It is NOT plain SGR even when the final
        // byte is 'm'. Claude Code emits `\e[>4;2m` (xterm modifyOtherKeys
        // mode 2 set) at startup; routing it through `handle_sgr` previously
        // mis-set UNDERLINE + DIM on the cursor pen, making every subsequent
        // cell render underlined since Claude never emits `\e[24m`/`\e[0m`.
        match intermediates.first() {
            Some(&b'?') => match action {
                'h' => super::csi::dispatch_private_mode(self.grid, params, true),
                'l' => super::csi::dispatch_private_mode(self.grid, params, false),
                _ => {}
            },
            None => super::csi::dispatch(self.grid, params, action),
            // `>`, `!`, ` `, `'`, `$`, etc. — recognised CSI intermediates we
            // don't yet implement. Drop them silently rather than fall back
            // to standard CSI handling.
            Some(_) => {}
        }
        if action == 'm' {
            if let Some(path) = std::env::var_os("CCMUX_TRACE_CSI") {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                    let _ = writeln!(
                        f,
                        "  → underline_after={} bits={:#06x} fg={:?} bg={:?}",
                        self.grid.cursor.style.contains(crate::vt::cell::CellAttrs::UNDERLINE),
                        self.grid.cursor.style.bits,
                        self.grid.cursor.fg,
                        self.grid.cursor.bg,
                    );
                }
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        // CCMUX_TRACE_ESC=<path> で ESC <intermediates> <final> をファイルに追記。
        if let Some(path) = std::env::var_os("CCMUX_TRACE_ESC") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                let _ = writeln!(
                    f,
                    "ESC intermediates={intermediates:?} byte={:#04x} ({})",
                    byte,
                    byte as char
                );
            }
        }
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
    fn resize_grows_visible_rows() {
        let mut t = Terminal::new(3, 5, 100);
        t.resize(5, 5);
        assert_eq!(t.grid.rows, 5);
        assert_eq!(t.grid.primary.visible.len(), 5);
        assert_eq!(t.grid.alternate.visible.len(), 5);
        assert!(t.grid.scroll_bottom <= 4);
    }

    #[test]
    fn resize_shrinks_visible_rows_and_clamps_cursor() {
        let mut t = Terminal::new(5, 5, 100);
        t.process(b"\x1b[5;5H");
        assert_eq!(t.grid.cursor.row, 4);
        t.resize(3, 5);
        assert_eq!(t.grid.rows, 3);
        assert_eq!(t.grid.primary.visible.len(), 3);
        assert!(t.grid.cursor.row < 3);
        assert!(t.grid.cursor.col < 5);
    }

    #[test]
    fn resize_cols_only_preserves_scrollback() {
        let mut t = Terminal::new(3, 10, 100);
        t.process(b"line1\r\n");
        t.process(b"line2\r\n");
        t.process(b"line3\r\n");
        t.process(b"line4\r\n");
        let sb_before = t.grid.primary.scrollback.len();
        t.resize(3, 6);
        assert_eq!(t.grid.primary.scrollback.len(), sb_before);
        assert_eq!(t.grid.cols, 6);
    }

    #[test]
    fn resize_shrink_pushes_overflow_to_scrollback() {
        let mut t = Terminal::new(5, 10, 100);
        t.process(b"a\r\nb\r\nc\r\nd\r\ne");
        let sb_before = t.grid.primary.scrollback.len();
        t.resize(3, 10);
        assert_eq!(t.grid.primary.visible.len(), 3);
        // 'e' was on row 4; after shrink to 3 rows it should be on row 2 (preserved).
        assert_eq!(t.grid.primary.visible[2].cells[0].ch, 'e');
        // Top rows (a, b) were pushed to scrollback.
        assert_eq!(t.grid.primary.scrollback.len(), sb_before + 2);
    }

    #[test]
    fn resize_shrink_alt_screen_does_not_save_to_scrollback() {
        let mut t = Terminal::new(5, 10, 100);
        t.process(b"\x1b[?1049h"); // enter alt screen
        t.process(b"a\r\nb\r\nc\r\nd\r\ne");
        let sb_before = t.grid.alternate.scrollback.len();
        t.resize(3, 10);
        assert_eq!(t.grid.alternate.scrollback.len(), sb_before);
    }

    #[test]
    fn resize_clamps_scroll_region() {
        let mut t = Terminal::new(10, 10, 100);
        t.process(b"\x1b[3;9r"); // top=2, bottom=8
        assert_eq!(t.grid.scroll_top, 2);
        assert_eq!(t.grid.scroll_bottom, 8);
        t.resize(5, 10);
        assert!(t.grid.scroll_bottom <= 4);
        assert!(t.grid.scroll_top < t.grid.scroll_bottom);
    }

    #[test]
    fn combining_char_is_skipped() {
        let mut t = Terminal::new(3, 5, 100);
        // 'a' + combining acute (U+0301)
        t.process("a\u{0301}".as_bytes());
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, 'a');
        assert_eq!(t.grid.cursor.col, 1);
    }

    #[test]
    fn xterm_modify_other_keys_does_not_set_sgr_attrs() {
        // Regression for the "everything underlined" bug: Claude Code emits
        // `\e[>4;2m` (xterm modifyOtherKeys mode 2 set) at startup, which has
        // a `>` intermediate and is NOT plain SGR. We previously routed it
        // through handle_sgr → set UNDERLINE (param 4) + DIM (param 2), and
        // since Claude never emits `\e[24m` / `\e[0m` the bits stuck and
        // every subsequent print() carried them.
        let mut t = Terminal::new(2, 10, 100);
        t.process(b"\x1b[>4;2m");
        assert!(
            !t.grid.cursor.style.contains(crate::vt::cell::CellAttrs::UNDERLINE),
            "CSI > 4 ; 2 m must not set UNDERLINE"
        );
        assert!(
            !t.grid.cursor.style.contains(crate::vt::cell::CellAttrs::DIM),
            "CSI > 4 ; 2 m must not set DIM"
        );
        // Subsequent prints carry no spurious attrs.
        t.process(b"hi");
        let cell = &t.grid.primary.visible[0].cells[0];
        assert_eq!(cell.attrs.bits, 0, "first cell should have no SGR bits");
    }

    #[test]
    fn standard_sgr_4_still_sets_underline() {
        // Make sure the intermediate gating doesn't break legitimate SGR.
        let mut t = Terminal::new(2, 10, 100);
        t.process(b"\x1b[4mU\x1b[24m");
        let cell = &t.grid.primary.visible[0].cells[0];
        assert!(
            cell.attrs.contains(crate::vt::cell::CellAttrs::UNDERLINE),
            "plain CSI 4 m must set UNDERLINE on subsequent cells"
        );
        assert!(!t.grid.cursor.style.contains(crate::vt::cell::CellAttrs::UNDERLINE));
    }

    #[test]
    fn xterm_secondary_da_does_not_pollute_pen() {
        // `\e[>c` queries secondary DA — must be a no-op on the cursor pen.
        let mut t = Terminal::new(2, 10, 100);
        t.process(b"\x1b[>c");
        assert_eq!(t.grid.cursor.style.bits, 0);
        assert_eq!(t.grid.cursor.fg, crate::vt::cell::Color::Default);
    }
}
