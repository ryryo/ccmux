use super::line::LogicalLine;
use super::scrollback::Scrollback;

/// A position within the logical-line stream (scrollback ++ visible).
/// `line` is the global index — `scrollback.iter().chain(visible)`.
/// `col` is a cell-index into `line.cells` (NOT a visual column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogicalPos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Linear,
    Word,
    Line,
    #[allow(dead_code)]
    Rectangle,
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub anchor: LogicalPos,
    pub head: LogicalPos,
    pub kind: SelectionKind,
}

impl Selection {
    pub fn start_linear(pos: LogicalPos) -> Self {
        Self { anchor: pos, head: pos, kind: SelectionKind::Linear }
    }

    pub fn extend(&mut self, pos: LogicalPos) {
        self.head = pos;
    }

    fn line_at<'a>(
        scrollback: &'a Scrollback,
        visible: &'a [LogicalLine],
        idx: usize,
    ) -> Option<&'a LogicalLine> {
        let sb_len = scrollback.len();
        if idx < sb_len {
            scrollback.iter().nth(idx)
        } else {
            visible.get(idx - sb_len)
        }
    }

    pub fn expand_word(&mut self, scrollback: &Scrollback, visible: &[LogicalLine], pos: LogicalPos) {
        let Some(line) = Self::line_at(scrollback, visible, pos.line) else { return };
        let cells = &line.cells;
        if cells.is_empty() {
            self.anchor = pos;
            self.head = pos;
            self.kind = SelectionKind::Word;
            return;
        }
        let is_word = |c: char| !c.is_whitespace() && c != '\0';
        let mut start = pos.col.min(cells.len().saturating_sub(1));
        let mut end = start;
        if !is_word(cells[start].ch) {
            self.anchor = pos;
            self.head = pos;
            self.kind = SelectionKind::Word;
            return;
        }
        while start > 0 && is_word(cells[start - 1].ch) {
            start -= 1;
        }
        while end + 1 < cells.len() && is_word(cells[end + 1].ch) {
            end += 1;
        }
        self.anchor = LogicalPos { line: pos.line, col: start };
        self.head = LogicalPos { line: pos.line, col: end };
        self.kind = SelectionKind::Word;
    }

    pub fn expand_line(&mut self, scrollback: &Scrollback, visible: &[LogicalLine], pos: LogicalPos) {
        let Some(line) = Self::line_at(scrollback, visible, pos.line) else { return };
        self.anchor = LogicalPos { line: pos.line, col: 0 };
        self.head = LogicalPos { line: pos.line, col: line.cells.len().saturating_sub(1) };
        self.kind = SelectionKind::Line;
    }

    /// Normalized (start, end) where start <= end.
    pub fn range(&self) -> (LogicalPos, LogicalPos) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn contains(&self, pos: LogicalPos) -> bool {
        let (s, e) = self.range();
        pos >= s && pos <= e
    }

    pub fn extract_text(&self, scrollback: &Scrollback, visible: &[LogicalLine]) -> String {
        let (start, end) = self.range();
        let mut out = String::new();
        for line_idx in start.line..=end.line {
            let Some(line) = Self::line_at(scrollback, visible, line_idx) else { continue };
            let cells = &line.cells;
            let from = if line_idx == start.line { start.col } else { 0 };
            let to = if line_idx == end.line { (end.col + 1).min(cells.len()) } else { cells.len() };
            let seg_start = out.len();
            for c in &cells[from.min(cells.len())..to] {
                if c.width == 0 { continue; }
                if c.ch == '\0' { continue; }
                out.push(c.ch);
            }
            let trimmed = out[seg_start..].trim_end_matches(' ').len();
            out.truncate(seg_start + trimmed);
            if line_idx < end.line {
                out.push('\n');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vt::cell::Cell;

    fn ascii_line(s: &str) -> LogicalLine {
        let mut l = LogicalLine::empty();
        for ch in s.chars() {
            l.push_cell(Cell { ch, width: 1, ..Cell::default() });
        }
        l
    }

    fn cjk_line(s: &str) -> LogicalLine {
        let mut l = LogicalLine::empty();
        for ch in s.chars() {
            l.push_cell(Cell { ch, width: 2, ..Cell::default() });
            l.push_cell(Cell { ch: '\0', width: 0, ..Cell::default() });
        }
        l
    }

    #[test]
    fn start_extend_grows_range() {
        let mut sel = Selection::start_linear(LogicalPos { line: 0, col: 2 });
        sel.extend(LogicalPos { line: 0, col: 5 });
        let (s, e) = sel.range();
        assert_eq!(s.col, 2);
        assert_eq!(e.col, 5);
    }

    #[test]
    fn contains_endpoints() {
        let mut sel = Selection::start_linear(LogicalPos { line: 1, col: 3 });
        sel.extend(LogicalPos { line: 2, col: 1 });
        assert!(sel.contains(LogicalPos { line: 1, col: 3 }));
        assert!(sel.contains(LogicalPos { line: 2, col: 1 }));
        assert!(sel.contains(LogicalPos { line: 1, col: 9 }));
        assert!(!sel.contains(LogicalPos { line: 0, col: 9 }));
        assert!(!sel.contains(LogicalPos { line: 2, col: 2 }));
    }

    #[test]
    fn extract_text_multiline() {
        let sb = Scrollback::new(0);
        let visible = vec![ascii_line("hello"), ascii_line("world")];
        let mut sel = Selection::start_linear(LogicalPos { line: 0, col: 2 });
        sel.extend(LogicalPos { line: 1, col: 2 });
        let text = sel.extract_text(&sb, &visible);
        assert_eq!(text, "llo\nwor");
    }

    #[test]
    fn extract_text_cjk_no_double_count() {
        let sb = Scrollback::new(0);
        let visible = vec![cjk_line("あいう")];
        let mut sel = Selection::start_linear(LogicalPos { line: 0, col: 0 });
        // include all 6 cells (3 wide chars + 3 continuations)
        sel.extend(LogicalPos { line: 0, col: 5 });
        let text = sel.extract_text(&sb, &visible);
        assert_eq!(text, "あいう");
    }

    #[test]
    fn expand_word_picks_word_boundaries() {
        let sb = Scrollback::new(0);
        let visible = vec![ascii_line("hello world foo")];
        let mut sel = Selection::start_linear(LogicalPos { line: 0, col: 0 });
        sel.expand_word(&sb, &visible, LogicalPos { line: 0, col: 7 }); // 'o' in world
        let (s, e) = sel.range();
        assert_eq!(s.col, 6);
        assert_eq!(e.col, 10);
        assert_eq!(sel.extract_text(&sb, &visible), "world");
    }

    #[test]
    fn expand_line_selects_whole_line() {
        let sb = Scrollback::new(0);
        let visible = vec![ascii_line("hello world")];
        let mut sel = Selection::start_linear(LogicalPos { line: 0, col: 0 });
        sel.expand_line(&sb, &visible, LogicalPos { line: 0, col: 3 });
        let (s, e) = sel.range();
        assert_eq!(s.col, 0);
        assert_eq!(e.col, 10);
    }
}
