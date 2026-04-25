#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Default for Color {
    fn default() -> Self {
        Color::Default
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellAttrs {
    pub bits: u16,
    pub hyperlink: u32,
}

impl CellAttrs {
    pub const BOLD: u16 = 1 << 0;
    pub const ITALIC: u16 = 1 << 1;
    pub const UNDERLINE: u16 = 1 << 2;
    pub const REVERSE: u16 = 1 << 3;
    pub const DIM: u16 = 1 << 4;
    pub const STRIKETHROUGH: u16 = 1 << 5;
    pub const BLINK: u16 = 1 << 6;

    pub fn set(&mut self, flag: u16) {
        self.bits |= flag;
    }

    pub fn clear(&mut self, flag: u16) {
        self.bits &= !flag;
    }

    pub fn contains(&self, flag: u16) -> bool {
        (self.bits & flag) == flag
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub width: u8,
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttrs,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            width: 1,
            fg: Color::Default,
            bg: Color::Default,
            attrs: CellAttrs::default(),
        }
    }
}

impl Cell {
    pub fn is_continuation(&self) -> bool {
        self.width == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_default_is_blank() {
        let c = Cell::default();
        assert_eq!(c.ch, ' ');
        assert_eq!(c.width, 1);
        assert_eq!(c.fg, Color::Default);
        assert_eq!(c.bg, Color::Default);
        assert_eq!(c.attrs, CellAttrs::default());
    }

    #[test]
    fn color_variants_are_distinct() {
        assert_ne!(Color::Default, Color::Indexed(0));
        assert_ne!(Color::Indexed(1), Color::Indexed(2));
        assert_ne!(Color::Rgb(1, 2, 3), Color::Rgb(1, 2, 4));
    }

    #[test]
    fn cell_attrs_set_clear_contains() {
        let mut a = CellAttrs::default();
        assert!(!a.contains(CellAttrs::BOLD));
        a.set(CellAttrs::BOLD);
        a.set(CellAttrs::ITALIC);
        assert!(a.contains(CellAttrs::BOLD));
        assert!(a.contains(CellAttrs::ITALIC));
        assert!(!a.contains(CellAttrs::UNDERLINE));
        a.clear(CellAttrs::BOLD);
        assert!(!a.contains(CellAttrs::BOLD));
        assert!(a.contains(CellAttrs::ITALIC));
    }

    #[test]
    fn is_continuation_when_width_zero() {
        let mut c = Cell::default();
        assert!(!c.is_continuation());
        c.width = 0;
        assert!(c.is_continuation());
    }
}
