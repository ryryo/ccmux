use super::cell::Cell;
use super::line::LogicalLine;
use super::scrollback::Scrollback;
use super::selection::LogicalPos;

/// A single visual (screen) row produced by chunking a logical line at `cols`.
/// `line_index` is sequential across scrollback then visible.
/// `start_col` is the cumulative visual-width offset within the logical line at
/// which this chunk begins.
#[derive(Debug, Clone, Copy)]
pub struct VisualRow<'a> {
    pub line_index: usize,
    /// Cumulative visual-width offset within the logical line (used for
    /// cursor placement and visual-column math).
    pub start_col: usize,
    /// Cumulative cell-index offset within the logical line (used for
    /// selection logical-position math, which addresses cells).
    pub start_cell_idx: usize,
    pub cells: &'a [Cell],
}

pub fn to_visual_rows<'a>(
    scrollback: &'a Scrollback,
    visible: &'a [LogicalLine],
    cols: u16,
) -> Vec<VisualRow<'a>> {
    let cols = (cols as usize).max(1);
    let mut out = Vec::new();
    for (idx, line) in scrollback.iter().chain(visible.iter()).enumerate() {
        push_chunks(line, idx, cols, &mut out);
    }
    out
}

fn push_chunks<'a>(
    line: &'a LogicalLine,
    line_index: usize,
    cols: usize,
    out: &mut Vec<VisualRow<'a>>,
) {
    if line.cells.is_empty() {
        out.push(VisualRow {
            line_index,
            start_col: 0,
            start_cell_idx: 0,
            cells: &[],
        });
        return;
    }
    let mut start = 0usize;
    let mut start_col = 0usize;
    let mut width = 0usize;
    let mut i = 0usize;
    while i < line.cells.len() {
        let cw = line.cells[i].width as usize;
        if cw > 0 && width + cw > cols {
            out.push(VisualRow {
                line_index,
                start_col,
                start_cell_idx: start,
                cells: &line.cells[start..i],
            });
            start = i;
            start_col += width;
            width = 0;
            continue;
        }
        width += cw;
        i += 1;
    }
    out.push(VisualRow {
        line_index,
        start_col,
        start_cell_idx: start,
        cells: &line.cells[start..],
    });
}

/// Map a screen-relative (row, col) — where `screen_row` 0 is the top of
/// the visible viewport — to a `LogicalPos` referencing
/// `scrollback.iter().chain(visible)`. `area_height` is the number of
/// terminal rows currently rendered; `scroll_offset` is how many rows
/// the user has scrolled UP into history (0 = live tail).
///
/// Returns `None` if the pointer falls above the topmost visual row
/// or in an empty area below the last logical line.
///
/// Continuation cells (CJK width=0) are snapped to their owning main cell
/// so the returned `col` always references a printable cell.
pub fn screen_to_logical(
    visual_rows: &[VisualRow],
    screen_row: u16,
    screen_col: u16,
    scroll_offset: usize,
    area_height: u16,
) -> Option<LogicalPos> {
    let total = visual_rows.len();
    if total == 0 || area_height == 0 {
        return None;
    }
    let bottom = total.saturating_sub(scroll_offset);
    let top = bottom.saturating_sub(area_height as usize);
    let target = top + screen_row as usize;
    if target >= bottom {
        return None;
    }
    let row = visual_rows[target];
    // Walk cells accumulating visual width until we hit screen_col.
    let mut acc = 0usize;
    let mut last_main: usize = 0;
    for (i, cell) in row.cells.iter().enumerate() {
        if cell.width > 0 {
            last_main = i;
        }
        let cw = cell.width as usize;
        if acc + cw > screen_col as usize {
            // Snap continuation cells back to the main cell.
            let cell_idx = if cell.width == 0 { last_main } else { i };
            return Some(LogicalPos {
                line: row.line_index,
                col: row.start_cell_idx + cell_idx,
            });
        }
        acc += cw;
    }
    // Past the last printable cell — clamp to end-of-row.
    Some(LogicalPos {
        line: row.line_index,
        col: row.start_cell_idx + row.cells.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vt::cell::Cell;
    use crate::vt::line::LogicalLine;
    use crate::vt::scrollback::Scrollback;

    fn line_of_ascii(s: &str) -> LogicalLine {
        let mut l = LogicalLine::empty();
        for ch in s.chars() {
            l.push_cell(Cell {
                ch,
                width: 1,
                ..Cell::default()
            });
        }
        l
    }

    fn line_of_cjk(s: &str) -> LogicalLine {
        let mut l = LogicalLine::empty();
        for ch in s.chars() {
            l.push_cell(Cell {
                ch,
                width: 2,
                ..Cell::default()
            });
            l.push_cell(Cell {
                ch: '\0',
                width: 0,
                ..Cell::default()
            });
        }
        l
    }

    #[test]
    fn short_line_one_visual_row() {
        let visible = vec![line_of_ascii("hello")];
        let sb = Scrollback::new(0);
        let rows = to_visual_rows(&sb, &visible, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].start_col, 0);
        assert_eq!(rows[0].cells.len(), 5);
    }

    #[test]
    fn long_line_chunks_at_cols() {
        let visible = vec![line_of_ascii(&"a".repeat(25))];
        let sb = Scrollback::new(0);
        let rows = to_visual_rows(&sb, &visible, 10);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].cells.len(), 10);
        assert_eq!(rows[0].start_col, 0);
        assert_eq!(rows[1].cells.len(), 10);
        assert_eq!(rows[1].start_col, 10);
        assert_eq!(rows[2].cells.len(), 5);
        assert_eq!(rows[2].start_col, 20);
    }

    #[test]
    fn cjk_does_not_split_wide_char() {
        // あいうえお (width 10) into cols=8 → first chunk visual width 8 (4 chars), then 2.
        let visible = vec![line_of_cjk("あいうえお")];
        let sb = Scrollback::new(0);
        let rows = to_visual_rows(&sb, &visible, 8);
        assert_eq!(rows.len(), 2);
        let total_w0: usize = rows[0].cells.iter().map(|c| c.width as usize).sum();
        let total_w1: usize = rows[1].cells.iter().map(|c| c.width as usize).sum();
        assert_eq!(total_w0, 8);
        assert_eq!(total_w1, 2);
        assert_eq!(rows[1].start_col, 8);
    }

    #[test]
    fn empty_visible_yields_empty_vec() {
        let visible: Vec<LogicalLine> = vec![];
        let sb = Scrollback::new(0);
        let rows = to_visual_rows(&sb, &visible, 10);
        assert!(rows.is_empty());
    }

    #[test]
    fn empty_logical_line_emits_one_row() {
        let visible = vec![LogicalLine::empty()];
        let sb = Scrollback::new(0);
        let rows = to_visual_rows(&sb, &visible, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cells.len(), 0);
    }

    #[test]
    fn screen_to_logical_live_tail() {
        let visible = vec![ascii("a"), ascii("b"), ascii("c"), ascii("d")];
        let sb = Scrollback::new(0);
        let rows = to_visual_rows(&sb, &visible, 10);
        // area_height=4 viewport, scroll_offset=0 → top=0, bottom=4
        let pos = screen_to_logical(&rows, 0, 0, 0, 4).unwrap();
        assert_eq!(pos.line, 0);
        let pos = screen_to_logical(&rows, 3, 0, 0, 4).unwrap();
        assert_eq!(pos.line, 3);
    }

    #[test]
    fn screen_to_logical_scrolled_back() {
        let mut sb = Scrollback::new(20);
        sb.push(ascii("old0"));
        sb.push(ascii("old1"));
        let visible = vec![ascii("new0"), ascii("new1")];
        let rows = to_visual_rows(&sb, &visible, 10);
        // 4 rows total, area=2, scroll_offset=2 → see only old0/old1
        let pos = screen_to_logical(&rows, 0, 0, 2, 2).unwrap();
        assert_eq!(pos.line, 0);
        let pos = screen_to_logical(&rows, 1, 0, 2, 2).unwrap();
        assert_eq!(pos.line, 1);
    }

    #[test]
    fn screen_to_logical_cjk_continuation_snaps() {
        let visible = vec![cjk("あいう")];
        let sb = Scrollback::new(0);
        let rows = to_visual_rows(&sb, &visible, 10);
        // screen_col=1 lands on continuation cell of い? actually col 0 is 'あ' main,
        // col 1 is 'あ' continuation → should snap to col 0.
        let pos = screen_to_logical(&rows, 0, 1, 0, 1).unwrap();
        assert_eq!(pos.col, 0);
        // col 2 is 'い' main
        let pos = screen_to_logical(&rows, 0, 2, 0, 1).unwrap();
        assert_eq!(pos.col, 2);
    }

    fn ascii(s: &str) -> LogicalLine { line_of_ascii(s) }
    fn cjk(s: &str) -> LogicalLine { line_of_cjk(s) }

    #[test]
    fn scrollback_then_visible_indexing() {
        let mut sb = Scrollback::new(10);
        sb.push(line_of_ascii("old"));
        let visible = vec![line_of_ascii("new")];
        let rows = to_visual_rows(&sb, &visible, 10);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].line_index, 0);
        assert_eq!(rows[1].line_index, 1);
    }
}
