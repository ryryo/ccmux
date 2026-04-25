use std::collections::VecDeque;

use super::line::LogicalLine;

#[derive(Clone, Debug)]
pub struct Scrollback {
    lines: VecDeque<LogicalLine>,
    max_lines: usize,
}

impl Scrollback {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max_lines,
        }
    }

    pub fn push(&mut self, line: LogicalLine) {
        if self.max_lines == 0 {
            return;
        }
        self.lines.push_back(line);
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &LogicalLine> {
        self.lines.iter()
    }

    pub fn get(&self, index: usize) -> Option<&LogicalLine> {
        self.lines.get(index)
    }

    pub fn max_lines(&self) -> usize {
        self.max_lines
    }

    pub fn set_max_lines(&mut self, n: usize) {
        self.max_lines = n;
        while self.lines.len() > n {
            self.lines.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_with_marker(marker: char) -> LogicalLine {
        let mut l = LogicalLine::empty();
        l.push_cell(super::super::cell::Cell {
            ch: marker,
            width: 1,
            ..Default::default()
        });
        l
    }

    #[test]
    fn empty_scrollback_len_zero() {
        let s = Scrollback::new(10);
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
    }

    #[test]
    fn push_evicts_oldest_at_capacity() {
        let mut s = Scrollback::new(3);
        for ch in ['1', '2', '3', '4', '5'] {
            s.push(line_with_marker(ch));
        }
        assert_eq!(s.len(), 3);
        let chars: Vec<char> = s.iter().map(|l| l.cells[0].ch).collect();
        assert_eq!(chars, vec!['3', '4', '5']);
    }

    #[test]
    fn iter_in_chronological_order() {
        let mut s = Scrollback::new(10);
        s.push(line_with_marker('a'));
        s.push(line_with_marker('b'));
        s.push(line_with_marker('c'));
        let chars: Vec<char> = s.iter().map(|l| l.cells[0].ch).collect();
        assert_eq!(chars, vec!['a', 'b', 'c']);
    }

    #[test]
    fn set_max_lines_truncates() {
        let mut s = Scrollback::new(10);
        for ch in ['1', '2', '3', '4', '5'] {
            s.push(line_with_marker(ch));
        }
        s.set_max_lines(2);
        assert_eq!(s.len(), 2);
        let chars: Vec<char> = s.iter().map(|l| l.cells[0].ch).collect();
        assert_eq!(chars, vec!['4', '5']);
    }

    #[test]
    fn get_out_of_range_returns_none() {
        let s = Scrollback::new(10);
        assert!(s.get(0).is_none());
        let mut s = Scrollback::new(10);
        s.push(line_with_marker('x'));
        assert!(s.get(0).is_some());
        assert!(s.get(1).is_none());
    }
}
