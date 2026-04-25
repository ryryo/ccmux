use super::cell::Cell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogicalLine {
    pub cells: Vec<Cell>,
    pub continued: bool,
}

impl LogicalLine {
    pub fn new(cols: usize) -> Self {
        Self {
            cells: vec![Cell::default(); cols],
            continued: false,
        }
    }

    pub fn empty() -> Self {
        Self {
            cells: Vec::new(),
            continued: false,
        }
    }

    pub fn cell_width(&self) -> usize {
        self.cells.iter().map(|c| c.width as usize).sum()
    }

    pub fn push_cell(&mut self, cell: Cell) {
        self.cells.push(cell);
    }

    pub fn set_cell(&mut self, col: usize, cell: Cell) {
        if col >= self.cells.len() {
            self.cells.resize(col + 1, Cell::default());
        }

        let prev = self.cells[col];

        if prev.width == 2 {
            if let Some(c) = self.cells.get_mut(col + 1) {
                if c.is_continuation() {
                    *c = Cell::default();
                }
            }
        } else if prev.is_continuation() && col > 0 {
            if let Some(prev_main) = self.cells.get_mut(col - 1) {
                if prev_main.width == 2 {
                    *prev_main = Cell::default();
                }
            }
        }

        self.cells[col] = cell;

        if cell.width == 2 {
            let cont_col = col + 1;
            if cont_col >= self.cells.len() {
                self.cells.resize(cont_col + 1, Cell::default());
            }
            let after = self.cells[cont_col];
            if after.width == 2 {
                if let Some(after_cont) = self.cells.get_mut(cont_col + 1) {
                    if after_cont.is_continuation() {
                        *after_cont = Cell::default();
                    }
                }
            }
            self.cells[cont_col] = Cell {
                ch: '\0',
                width: 0,
                fg: cell.fg,
                bg: cell.bg,
                attrs: cell.attrs,
            };
        }
    }

    /// Truncate so total visible width <= `width`, preserving wide-char boundaries.
    pub fn truncate_to_width(&mut self, width: usize) {
        let mut total = 0usize;
        let mut cut_at = self.cells.len();
        for (i, c) in self.cells.iter().enumerate() {
            let cw = c.width as usize;
            if total + cw > width {
                cut_at = i;
                break;
            }
            total += cw;
        }
        self.cells.truncate(cut_at);
    }
}

#[cfg(test)]
mod tests {
    use super::super::cell::{Cell, Color};
    use super::*;

    fn ascii_cell(ch: char) -> Cell {
        Cell { ch, width: 1, fg: Color::Default, bg: Color::Default, attrs: Default::default() }
    }

    fn wide_cell(ch: char) -> Cell {
        Cell { ch, width: 2, fg: Color::Default, bg: Color::Default, attrs: Default::default() }
    }

    #[test]
    fn empty_line_zero_width() {
        let l = LogicalLine::empty();
        assert_eq!(l.cell_width(), 0);
    }

    #[test]
    fn ascii_five_chars() {
        let mut l = LogicalLine::empty();
        for ch in "hello".chars() {
            l.push_cell(ascii_cell(ch));
        }
        assert_eq!(l.cell_width(), 5);
    }

    #[test]
    fn cjk_wide_has_continuation() {
        let mut l = LogicalLine::new(4);
        l.set_cell(0, wide_cell('あ'));
        assert_eq!(l.cells[0].width, 2);
        assert_eq!(l.cells[1].width, 0);
        assert!(l.cells[1].is_continuation());
    }

    #[test]
    fn truncate_preserves_wide_boundary() {
        let mut l = LogicalLine::empty();
        l.push_cell(ascii_cell('a'));
        l.push_cell(wide_cell('あ'));
        l.push_cell(Cell { ch: '\0', width: 0, ..Default::default() });
        l.push_cell(ascii_cell('b'));
        l.truncate_to_width(3);
        assert_eq!(l.cell_width(), 3);
        assert_eq!(l.cells.last().unwrap().is_continuation(), true);
    }

    #[test]
    fn truncate_does_not_split_wide_char() {
        let mut l = LogicalLine::empty();
        l.push_cell(ascii_cell('a'));
        l.push_cell(wide_cell('あ'));
        l.push_cell(Cell { ch: '\0', width: 0, ..Default::default() });
        l.truncate_to_width(2);
        assert_eq!(l.cell_width(), 1);
        assert_eq!(l.cells.len(), 1);
    }

    #[test]
    fn overwriting_wide_with_ascii_clears_continuation() {
        let mut l = LogicalLine::new(4);
        l.set_cell(0, wide_cell('あ'));
        l.set_cell(0, ascii_cell('x'));
        assert_eq!(l.cells[0].width, 1);
        assert_eq!(l.cells[1].width, 1);
        assert_eq!(l.cells[1].ch, ' ');
    }
}
