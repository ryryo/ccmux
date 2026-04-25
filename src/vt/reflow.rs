use super::cell::Cell;
use super::line::LogicalLine;
use super::scrollback::Scrollback;

/// A single visual (screen) row produced by chunking a logical line at `cols`.
/// `line_index` is sequential across scrollback then visible.
/// `start_col` is the cumulative visual-width offset within the logical line at
/// which this chunk begins.
#[derive(Debug, Clone, Copy)]
pub struct VisualRow<'a> {
    pub line_index: usize,
    pub start_col: usize,
    pub cells: &'a [Cell],
}

pub fn to_visual_rows<'a>(
    scrollback: &'a Scrollback,
    visible: &'a [LogicalLine],
    cols: u16,
) -> Vec<VisualRow<'a>> {
    let cols = (cols as usize).max(1);
    let mut out = Vec::new();
    let mut idx = 0usize;
    for line in scrollback.iter().chain(visible.iter()) {
        push_chunks(line, idx, cols, &mut out);
        idx += 1;
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
        cells: &line.cells[start..],
    });
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
