use ratatui::buffer::Buffer as RatBuffer;
use ratatui::layout::Rect;
use ratatui::style::{Color as RColor, Modifier, Style};
use ratatui::widgets::Widget;

use super::cell::{CellAttrs, Color};
use super::parser::Terminal;
use super::reflow::to_visual_rows;

/// A ratatui Widget that renders the terminal grid (with scrollback aware
/// reflow) into the destination buffer. Cursor placement is the caller's
/// responsibility (Frame::set_cursor_position is not available on a Widget).
pub struct PtyPaneWidget<'a> {
    pub terminal: &'a Terminal,
    pub scroll_offset: usize,
    /// Selection predicate: `(screen_row, screen_col) -> bool`. The caller is
    /// responsible for translating its higher-level selection model to
    /// screen-relative coords. `None` disables selection highlight.
    pub selection: Option<Box<dyn Fn(u32, u32) -> bool + 'a>>,
    pub focused: bool,
}

impl Widget for PtyPaneWidget<'_> {
    fn render(self, area: Rect, buf: &mut RatBuffer) {
        let _ = self.focused;
        let rows_total = area.height as usize;
        let cols_total = area.width as usize;
        if rows_total == 0 || cols_total == 0 {
            return;
        }
        let grid = &self.terminal.grid;
        let buffer = grid.current_buffer();
        let visual = to_visual_rows(&buffer.scrollback, &buffer.visible, grid.cols);

        let total = visual.len();
        let bottom = total.saturating_sub(self.scroll_offset);
        let top = bottom.saturating_sub(rows_total);

        for (screen_row, vrow) in visual[top..bottom].iter().enumerate() {
            let mut sx = 0u16;
            for cell in vrow.cells.iter() {
                if sx as usize >= cols_total {
                    break;
                }
                if cell.width == 0 {
                    // continuation cell; skip (already painted by main wide cell)
                    continue;
                }
                let x = area.x + sx;
                let y = area.y + screen_row as u16;
                let mut style = to_ratatui_style(cell.fg, cell.bg, cell.attrs);
                if cell.attrs.hyperlink != 0 {
                    // OSC 8: visually mark the cell as a clickable link.
                    style = style
                        .fg(RColor::Rgb(0x4a, 0x9e, 0xff))
                        .add_modifier(Modifier::UNDERLINED);
                }
                let has_selection = self
                    .selection
                    .as_ref()
                    .is_some_and(|f| f(screen_row as u32, sx as u32));
                let final_style = if has_selection {
                    Style::default()
                        .fg(RColor::Rgb(0xff, 0xff, 0xff))
                        .bg(RColor::Rgb(0x4a, 0x9e, 0xff))
                } else {
                    style
                };
                let symbol: String = if cell.ch == '\0' || cell.ch == ' ' {
                    " ".into()
                } else {
                    cell.ch.to_string()
                };
                if let Some(buf_cell) = buf.cell_mut((x, y)) {
                    buf_cell.set_symbol(&symbol);
                    buf_cell.set_style(final_style);
                }
                if cell.width == 2 {
                    let x2 = x + 1;
                    if (x2 as usize) < (area.x as usize) + cols_total {
                        if let Some(buf_cell) = buf.cell_mut((x2, y)) {
                            // ratatui treats the next cell as part of the wide glyph;
                            // clearing prevents rendering ghost characters.
                            buf_cell.set_symbol("");
                            buf_cell.set_style(final_style);
                        }
                    }
                }
                sx = sx.saturating_add(cell.width as u16);
            }
        }
    }
}

pub fn to_ratatui_style(fg: Color, bg: Color, attrs: CellAttrs) -> Style {
    let mut s = Style::default().fg(color_to_rat(fg)).bg(color_to_rat(bg));
    let mut m = Modifier::empty();
    if attrs.contains(CellAttrs::BOLD) {
        m |= Modifier::BOLD;
    }
    if attrs.contains(CellAttrs::ITALIC) {
        m |= Modifier::ITALIC;
    }
    if attrs.contains(CellAttrs::UNDERLINE) {
        m |= Modifier::UNDERLINED;
    }
    if attrs.contains(CellAttrs::REVERSE) {
        m |= Modifier::REVERSED;
    }
    if attrs.contains(CellAttrs::DIM) {
        m |= Modifier::DIM;
    }
    if attrs.contains(CellAttrs::STRIKETHROUGH) {
        m |= Modifier::CROSSED_OUT;
    }
    if attrs.contains(CellAttrs::BLINK) {
        m |= Modifier::SLOW_BLINK;
    }
    s = s.add_modifier(m);
    s
}

pub fn color_to_rat(c: Color) -> RColor {
    match c {
        Color::Default => RColor::Reset,
        Color::Indexed(i) => RColor::Indexed(i),
        Color::Rgb(r, g, b) => RColor::Rgb(r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_color_maps_to_reset() {
        assert_eq!(color_to_rat(Color::Default), RColor::Reset);
    }

    #[test]
    fn indexed_color_maps_through() {
        assert_eq!(color_to_rat(Color::Indexed(33)), RColor::Indexed(33));
    }

    #[test]
    fn rgb_color_maps_through() {
        assert_eq!(color_to_rat(Color::Rgb(1, 2, 3)), RColor::Rgb(1, 2, 3));
    }

    #[test]
    fn render_paints_osc8_hyperlink_with_blue_underline() {
        use ratatui::buffer::Buffer as RatBuffer;
        use ratatui::layout::Rect;
        use ratatui::widgets::Widget;

        let mut term = crate::vt::parser::Terminal::new(2, 20, 0);
        // OSC 8 open + "hi" + OSC 8 close
        term.process(b"\x1b]8;;https://example.com\x1b\\hi\x1b]8;;\x1b\\");

        let area = Rect::new(0, 0, 20, 2);
        let mut buf = RatBuffer::empty(area);
        let widget = PtyPaneWidget {
            terminal: &term,
            scroll_offset: 0,
            selection: None,
            focused: false,
        };
        widget.render(area, &mut buf);

        // 'h' at (0,0), 'i' at (1,0) should both carry the hyperlink style.
        for x in 0..2 {
            let cell = &buf[(x, 0)];
            assert_eq!(
                cell.style().fg,
                Some(RColor::Rgb(0x4a, 0x9e, 0xff)),
                "cell ({x},0) fg should be hyperlink blue"
            );
            assert!(
                cell.style().add_modifier.contains(Modifier::UNDERLINED),
                "cell ({x},0) should be underlined"
            );
        }
        // Cell past the link should not have hyperlink styling.
        let plain = &buf[(2, 0)];
        assert_ne!(plain.style().fg, Some(RColor::Rgb(0x4a, 0x9e, 0xff)));
    }

    #[test]
    fn attrs_combine_into_modifier() {
        let mut a = CellAttrs::default();
        a.set(CellAttrs::BOLD);
        a.set(CellAttrs::UNDERLINE);
        a.set(CellAttrs::REVERSE);
        let s = to_ratatui_style(Color::Default, Color::Default, a);
        let m = s.add_modifier;
        assert!(m.contains(Modifier::BOLD));
        assert!(m.contains(Modifier::UNDERLINED));
        assert!(m.contains(Modifier::REVERSED));
    }
}
