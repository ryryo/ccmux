use vte::Params;

use super::cell::{Cell, CellAttrs, Color};
use super::grid::Grid;
use super::line::LogicalLine;

fn first_param(params: &Params, default: u16) -> u16 {
    params
        .iter()
        .next()
        .and_then(|g| g.first().copied())
        .filter(|&v| v != 0)
        .unwrap_or(default)
}

fn nth_param(params: &Params, n: usize, default: u16) -> u16 {
    params
        .iter()
        .nth(n)
        .and_then(|g| g.first().copied())
        .filter(|&v| v != 0)
        .unwrap_or(default)
}

/// Dispatch DECSET (`?...h`) or DECRST (`?...l`) by setting `set` accordingly.
pub fn dispatch_private_mode(grid: &mut Grid, params: &Params, set: bool) {
    for grp in params.iter() {
        let p = match grp.first() {
            Some(&v) => v,
            None => continue,
        };
        match p {
            7 => grid.modes.auto_wrap = set,
            25 => grid.cursor.visible = set,
            47 | 1047 => {
                if set {
                    grid.enter_alternate();
                } else {
                    grid.exit_alternate();
                }
            }
            1049 => {
                if set {
                    grid.saved_cursor = Some(grid.cursor);
                    grid.enter_alternate();
                } else {
                    grid.exit_alternate();
                    if let Some(c) = grid.saved_cursor.take() {
                        grid.cursor = c;
                    }
                }
            }
            1000 => grid.modes.mouse_x10 = set,
            1002 => grid.modes.mouse_button = set,
            1003 => grid.modes.mouse_motion = set,
            1006 => grid.modes.mouse_sgr_encoding = set,
            2004 => grid.modes.bracketed_paste = set,
            _ => {}
        }
    }
}

/// Dispatch a CSI sequence (private modes excluded — those are handled in
/// `dispatch_private_mode`). Returns silently for unhandled actions.
pub fn dispatch(grid: &mut Grid, params: &Params, action: char) {
    match action {
        'A' => {
            let n = first_param(params, 1);
            grid.cursor.row = grid.cursor.row.saturating_sub(n);
        }
        'B' | 'e' => {
            let n = first_param(params, 1);
            grid.cursor.row = (grid.cursor.row + n).min(grid.rows.saturating_sub(1));
        }
        'C' | 'a' => {
            let n = first_param(params, 1);
            grid.cursor.col = (grid.cursor.col + n).min(grid.cols.saturating_sub(1));
        }
        'D' => {
            let n = first_param(params, 1);
            grid.cursor.col = grid.cursor.col.saturating_sub(n);
        }
        'G' | '`' => {
            let n = first_param(params, 1);
            grid.cursor.col = n.saturating_sub(1).min(grid.cols.saturating_sub(1));
        }
        'd' => {
            let n = first_param(params, 1);
            grid.cursor.row = n.saturating_sub(1).min(grid.rows.saturating_sub(1));
        }
        'H' | 'f' => {
            let row = first_param(params, 1).saturating_sub(1);
            let col = nth_param(params, 1, 1).saturating_sub(1);
            grid.cursor.row = row.min(grid.rows.saturating_sub(1));
            grid.cursor.col = col.min(grid.cols.saturating_sub(1));
        }
        'J' => erase_in_display(grid, first_param(params, 0)),
        'K' => erase_in_line(grid, first_param(params, 0)),
        'r' => {
            let top = first_param(params, 1).saturating_sub(1);
            let bottom = nth_param(params, 1, grid.rows).saturating_sub(1);
            grid.set_scroll_region(top, bottom);
            grid.cursor.row = 0;
            grid.cursor.col = 0;
        }
        's' => grid.saved_cursor = Some(grid.cursor),
        'u' => {
            if let Some(c) = grid.saved_cursor {
                grid.cursor = c;
            }
        }
        'S' => grid.scroll_up_in_region(first_param(params, 1)),
        'T' => scroll_down(grid, first_param(params, 1)),
        'L' => insert_lines(grid, first_param(params, 1)),
        'M' => delete_lines(grid, first_param(params, 1)),
        '@' => insert_chars(grid, first_param(params, 1)),
        'P' => delete_chars(grid, first_param(params, 1)),
        'X' => erase_chars(grid, first_param(params, 1)),
        'm' => handle_sgr(grid, params),
        _ => {}
    }
}

fn erase_in_display(grid: &mut Grid, mode: u16) {
    let cols = grid.cols as usize;
    let rows = grid.rows as usize;
    let cur_row = grid.cursor.row as usize;
    let cur_col = grid.cursor.col as usize;
    let blank = grid.blank_cell();
    let buf = grid.current_buffer_mut();
    match mode {
        0 => {
            // cursor → end of screen
            if let Some(line) = buf.visible.get_mut(cur_row) {
                fill_from(line, cur_col, cols, blank);
            }
            for r in (cur_row + 1)..rows.min(buf.visible.len()) {
                buf.visible[r] = LogicalLine::new(cols);
            }
        }
        1 => {
            // start of screen → cursor
            for r in 0..cur_row.min(buf.visible.len()) {
                buf.visible[r] = LogicalLine::new(cols);
            }
            if let Some(line) = buf.visible.get_mut(cur_row) {
                fill_to(line, cur_col + 1, cols, blank);
            }
        }
        2 => {
            for r in 0..rows.min(buf.visible.len()) {
                buf.visible[r] = LogicalLine::new(cols);
            }
        }
        3 => {
            buf.scrollback = super::scrollback::Scrollback::new(buf.scrollback.max_lines());
        }
        _ => {}
    }
}

fn erase_in_line(grid: &mut Grid, mode: u16) {
    let cols = grid.cols as usize;
    let cur_row = grid.cursor.row as usize;
    let cur_col = grid.cursor.col as usize;
    let blank = grid.blank_cell();
    let buf = grid.current_buffer_mut();
    let Some(line) = buf.visible.get_mut(cur_row) else { return };
    match mode {
        0 => fill_from(line, cur_col, cols, blank),
        1 => fill_to(line, cur_col + 1, cols, blank),
        2 => *line = LogicalLine::new(cols),
        _ => {}
    }
}

fn fill_from(line: &mut LogicalLine, start: usize, cols: usize, blank: Cell) {
    if line.cells.len() < cols {
        line.cells.resize(cols, Cell::default());
    }
    for c in line.cells.iter_mut().skip(start) {
        *c = blank;
    }
}

fn fill_to(line: &mut LogicalLine, end: usize, cols: usize, blank: Cell) {
    if line.cells.len() < cols {
        line.cells.resize(cols, Cell::default());
    }
    let end = end.min(line.cells.len());
    for c in line.cells.iter_mut().take(end) {
        *c = blank;
    }
}

fn scroll_down(grid: &mut Grid, count: u16) {
    let top = grid.scroll_top as usize;
    let bottom = grid.scroll_bottom as usize;
    if top >= bottom {
        return;
    }
    let cols = grid.cols as usize;
    let count = (count as usize).min(bottom - top + 1);
    let buf = grid.current_buffer_mut();
    for _ in 0..count {
        if bottom >= buf.visible.len() {
            break;
        }
        buf.visible.remove(bottom);
        buf.visible.insert(top, LogicalLine::new(cols));
    }
}

fn insert_lines(grid: &mut Grid, count: u16) {
    let cur_row = grid.cursor.row;
    if cur_row < grid.scroll_top || cur_row > grid.scroll_bottom {
        return;
    }
    let bottom = grid.scroll_bottom as usize;
    let row = cur_row as usize;
    let cols = grid.cols as usize;
    let count = (count as usize).min(bottom - row + 1);
    let buf = grid.current_buffer_mut();
    for _ in 0..count {
        if bottom >= buf.visible.len() {
            break;
        }
        buf.visible.remove(bottom);
        buf.visible.insert(row, LogicalLine::new(cols));
    }
    grid.cursor.col = 0;
}

fn delete_lines(grid: &mut Grid, count: u16) {
    let cur_row = grid.cursor.row;
    if cur_row < grid.scroll_top || cur_row > grid.scroll_bottom {
        return;
    }
    let bottom = grid.scroll_bottom as usize;
    let row = cur_row as usize;
    let cols = grid.cols as usize;
    let count = (count as usize).min(bottom - row + 1);
    let buf = grid.current_buffer_mut();
    for _ in 0..count {
        if row >= buf.visible.len() {
            break;
        }
        buf.visible.remove(row);
        buf.visible.insert(bottom, LogicalLine::new(cols));
    }
    grid.cursor.col = 0;
}

fn insert_chars(grid: &mut Grid, count: u16) {
    let cols = grid.cols as usize;
    let row = grid.cursor.row as usize;
    let col = grid.cursor.col as usize;
    let blank = grid.blank_cell();
    let buf = grid.current_buffer_mut();
    let Some(line) = buf.visible.get_mut(row) else { return };
    if line.cells.len() < cols {
        line.cells.resize(cols, Cell::default());
    }
    let count = (count as usize).min(cols.saturating_sub(col));
    for _ in 0..count {
        line.cells.insert(col, blank);
    }
    line.cells.truncate(cols);
}

fn delete_chars(grid: &mut Grid, count: u16) {
    let cols = grid.cols as usize;
    let row = grid.cursor.row as usize;
    let col = grid.cursor.col as usize;
    let blank = grid.blank_cell();
    let buf = grid.current_buffer_mut();
    let Some(line) = buf.visible.get_mut(row) else { return };
    if line.cells.len() < cols {
        line.cells.resize(cols, Cell::default());
    }
    let count = (count as usize).min(cols.saturating_sub(col));
    for _ in 0..count {
        if col < line.cells.len() {
            line.cells.remove(col);
        }
    }
    while line.cells.len() < cols {
        line.cells.push(blank);
    }
}

fn erase_chars(grid: &mut Grid, count: u16) {
    let cols = grid.cols as usize;
    let row = grid.cursor.row as usize;
    let col = grid.cursor.col as usize;
    let blank = grid.blank_cell();
    let buf = grid.current_buffer_mut();
    let Some(line) = buf.visible.get_mut(row) else { return };
    if line.cells.len() < cols {
        line.cells.resize(cols, Cell::default());
    }
    let count = (count as usize).min(cols.saturating_sub(col));
    for c in line.cells.iter_mut().skip(col).take(count) {
        *c = blank;
    }
}

/// Apply SGR (Select Graphic Rendition) parameters to the cursor's pen.
pub fn handle_sgr(grid: &mut Grid, params: &Params) {
    // Flatten into a single list of u16; vte gives us groups (sub-params),
    // but for SGR the conventional encoding puts 38;5;n etc. at the top level
    // OR as a single group with sub-params. Handle both.
    let flat: Vec<u16> = if params.iter().any(|grp| grp.len() > 1) {
        // Sub-params present: treat each group as a sequence.
        let mut v = Vec::new();
        for grp in params.iter() {
            for &p in grp {
                v.push(p);
            }
        }
        v
    } else {
        params.iter().map(|grp| grp[0]).collect()
    };

    if flat.is_empty() {
        reset(grid);
        return;
    }

    let mut i = 0;
    while i < flat.len() {
        let p = flat[i];
        match p {
            0 => reset(grid),
            1 => grid.cursor.style.set(CellAttrs::BOLD),
            2 => grid.cursor.style.set(CellAttrs::DIM),
            3 => grid.cursor.style.set(CellAttrs::ITALIC),
            4 => grid.cursor.style.set(CellAttrs::UNDERLINE),
            5 | 6 => grid.cursor.style.set(CellAttrs::BLINK),
            7 => grid.cursor.style.set(CellAttrs::REVERSE),
            9 => grid.cursor.style.set(CellAttrs::STRIKETHROUGH),
            22 => grid.cursor.style.clear(CellAttrs::BOLD | CellAttrs::DIM),
            23 => grid.cursor.style.clear(CellAttrs::ITALIC),
            24 => grid.cursor.style.clear(CellAttrs::UNDERLINE),
            25 => grid.cursor.style.clear(CellAttrs::BLINK),
            27 => grid.cursor.style.clear(CellAttrs::REVERSE),
            29 => grid.cursor.style.clear(CellAttrs::STRIKETHROUGH),
            30..=37 => grid.cursor.fg = Color::Indexed((p - 30) as u8),
            38 => {
                if let Some((color, consumed)) = parse_extended_color(&flat[i + 1..]) {
                    grid.cursor.fg = color;
                    i += consumed;
                }
            }
            39 => grid.cursor.fg = Color::Default,
            40..=47 => grid.cursor.bg = Color::Indexed((p - 40) as u8),
            48 => {
                if let Some((color, consumed)) = parse_extended_color(&flat[i + 1..]) {
                    grid.cursor.bg = color;
                    i += consumed;
                }
            }
            49 => grid.cursor.bg = Color::Default,
            90..=97 => grid.cursor.fg = Color::Indexed((p - 90 + 8) as u8),
            100..=107 => grid.cursor.bg = Color::Indexed((p - 100 + 8) as u8),
            _ => {}
        }
        i += 1;
    }
}

fn reset(grid: &mut Grid) {
    grid.cursor.fg = Color::Default;
    grid.cursor.bg = Color::Default;
    grid.cursor.style = CellAttrs::default();
}

/// Parse `5;n` (256-color) or `2;r;g;b` (truecolor) following a 38/48 prefix.
/// Returns (color, params consumed past the prefix).
fn parse_extended_color(rest: &[u16]) -> Option<(Color, usize)> {
    match rest.first().copied()? {
        5 => {
            let n = rest.get(1).copied()?;
            Some((Color::Indexed(n as u8), 2))
        }
        2 => {
            let r = rest.get(1).copied()?;
            let g = rest.get(2).copied()?;
            let b = rest.get(3).copied()?;
            Some((Color::Rgb(r as u8, g as u8, b as u8), 4))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vt::parser::Terminal;

    fn pen(t: &Terminal) -> (Color, Color, u16) {
        (t.grid.cursor.fg, t.grid.cursor.bg, t.grid.cursor.style.bits)
    }

    #[test]
    fn sgr_set_indexed_fg() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b[31m");
        assert_eq!(pen(&t).0, Color::Indexed(1));
    }

    #[test]
    fn sgr_combined_bold_yellow_fg() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b[1;33m");
        assert_eq!(t.grid.cursor.fg, Color::Indexed(3));
        assert!(t.grid.cursor.style.contains(CellAttrs::BOLD));
    }

    #[test]
    fn sgr_truecolor() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b[38;2;255;128;0m");
        assert_eq!(t.grid.cursor.fg, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn sgr_256_color_bg() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b[48;5;200m");
        assert_eq!(t.grid.cursor.bg, Color::Indexed(200));
    }

    #[test]
    fn sgr_reset_returns_to_default() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b[1;31;42m");
        t.process(b"\x1b[0m");
        let (fg, bg, bits) = pen(&t);
        assert_eq!(fg, Color::Default);
        assert_eq!(bg, Color::Default);
        assert_eq!(bits, 0);
    }

    #[test]
    fn sgr_bright_fg() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b[91m");
        assert_eq!(t.grid.cursor.fg, Color::Indexed(9));
    }

    #[test]
    fn cup_moves_cursor_one_origin() {
        let mut t = Terminal::new(10, 20, 100);
        t.process(b"\x1b[3;5H");
        assert_eq!(t.grid.cursor.row, 2);
        assert_eq!(t.grid.cursor.col, 4);
    }

    #[test]
    fn cup_default_is_origin() {
        let mut t = Terminal::new(10, 20, 100);
        t.process(b"AB\x1b[H");
        assert_eq!(t.grid.cursor.row, 0);
        assert_eq!(t.grid.cursor.col, 0);
    }

    #[test]
    fn cuu_cud_cuf_cub() {
        let mut t = Terminal::new(10, 20, 100);
        t.process(b"\x1b[5;5H");
        t.process(b"\x1b[2A");
        assert_eq!(t.grid.cursor.row, 2);
        t.process(b"\x1b[3B");
        assert_eq!(t.grid.cursor.row, 5);
        t.process(b"\x1b[4C");
        assert_eq!(t.grid.cursor.col, 8);
        t.process(b"\x1b[2D");
        assert_eq!(t.grid.cursor.col, 6);
    }

    #[test]
    fn ed_2_clears_screen() {
        let mut t = Terminal::new(3, 5, 100);
        t.process(b"abc\r\ndef");
        t.process(b"\x1b[2J");
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, ' ');
        assert_eq!(t.grid.primary.visible[1].cells[2].ch, ' ');
    }

    #[test]
    fn el_0_clears_to_eol() {
        let mut t = Terminal::new(3, 5, 100);
        t.process(b"hello\x1b[3G\x1b[K");
        // col=2 (G is 1-origin), erase from cursor → cells[2..]=' '
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, 'h');
        assert_eq!(t.grid.primary.visible[0].cells[1].ch, 'e');
        assert_eq!(t.grid.primary.visible[0].cells[2].ch, ' ');
        assert_eq!(t.grid.primary.visible[0].cells[4].ch, ' ');
    }

    #[test]
    fn decstbm_sets_scroll_region() {
        let mut t = Terminal::new(20, 80, 100);
        t.process(b"\x1b[5;15r");
        assert_eq!(t.grid.scroll_top, 4);
        assert_eq!(t.grid.scroll_bottom, 14);
    }

    #[test]
    fn decstbm_with_lf_keeps_scrollback_when_top_is_zero() {
        // Claude Code pattern: top=0, bottom=rows-3.
        let mut t = Terminal::new(5, 5, 100);
        t.process(b"\x1b[1;3r"); // scroll region rows 0..=2
        // Place cursor at row 2, then a newline must scroll within region.
        t.process(b"\x1b[3;1H"); // (row 2, col 0)
        t.process(b"AAA");
        t.process(b"\x1b[3;1H");
        t.process(b"\nBBB"); // LF: at scroll_bottom → scroll_up_in_region
        assert_eq!(t.grid.primary.scrollback.len(), 1);
    }

    #[test]
    fn save_restore_cursor() {
        let mut t = Terminal::new(10, 20, 100);
        t.process(b"\x1b[5;7H\x1b[s");
        t.process(b"\x1b[1;1H");
        t.process(b"\x1b[u");
        assert_eq!(t.grid.cursor.row, 4);
        assert_eq!(t.grid.cursor.col, 6);
    }

    #[test]
    fn ich_inserts_blanks() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"abcde\x1b[1;1H\x1b[2@");
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, ' ');
        assert_eq!(t.grid.primary.visible[0].cells[1].ch, ' ');
        assert_eq!(t.grid.primary.visible[0].cells[2].ch, 'a');
        assert_eq!(t.grid.primary.visible[0].cells[3].ch, 'b');
        assert_eq!(t.grid.primary.visible[0].cells[4].ch, 'c');
    }

    #[test]
    fn dch_deletes_chars() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"abcde\x1b[1;2H\x1b[2P");
        assert_eq!(t.grid.primary.visible[0].cells[0].ch, 'a');
        assert_eq!(t.grid.primary.visible[0].cells[1].ch, 'd');
        assert_eq!(t.grid.primary.visible[0].cells[2].ch, 'e');
    }

    #[test]
    fn ech_erases_chars_without_moving_cursor() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"abcde\x1b[1;2H\x1b[2X");
        assert_eq!(t.grid.primary.visible[0].cells[1].ch, ' ');
        assert_eq!(t.grid.primary.visible[0].cells[2].ch, ' ');
        assert_eq!(t.grid.primary.visible[0].cells[3].ch, 'd');
        assert_eq!(t.grid.cursor.col, 1);
    }

    #[test]
    fn alt_screen_1049_saves_and_restores_cursor() {
        let mut t = Terminal::new(10, 20, 100);
        t.process(b"\x1b[5;7H");
        t.process(b"\x1b[?1049h");
        assert!(t.grid.use_alternate);
        assert_eq!(t.grid.cursor.row, 0);
        t.process(b"\x1b[?1049l");
        assert!(!t.grid.use_alternate);
        assert_eq!(t.grid.cursor.row, 4);
        assert_eq!(t.grid.cursor.col, 6);
    }

    #[test]
    fn bracketed_paste_mode_toggle() {
        let mut t = Terminal::new(2, 5, 100);
        assert!(!t.grid.modes.bracketed_paste);
        t.process(b"\x1b[?2004h");
        assert!(t.grid.modes.bracketed_paste);
        t.process(b"\x1b[?2004l");
        assert!(!t.grid.modes.bracketed_paste);
    }

    #[test]
    fn decawm_disable() {
        let mut t = Terminal::new(2, 5, 100);
        assert!(t.grid.modes.auto_wrap);
        t.process(b"\x1b[?7l");
        assert!(!t.grid.modes.auto_wrap);
    }

    #[test]
    fn mouse_sgr_mode_toggle() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b[?1006h");
        assert!(t.grid.modes.mouse_sgr_encoding);
    }

    #[test]
    fn cursor_hide_show() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b[?25l");
        assert!(!t.grid.cursor.visible);
        t.process(b"\x1b[?25h");
        assert!(t.grid.cursor.visible);
    }

    #[test]
    fn sgr_empty_is_reset() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b[1m\x1b[m");
        assert_eq!(t.grid.cursor.style.bits, 0);
    }
}
