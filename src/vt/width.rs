use unicode_width::UnicodeWidthChar;

pub fn char_width(c: char) -> u8 {
    UnicodeWidthChar::width(c).unwrap_or(0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_one() {
        assert_eq!(char_width('a'), 1);
        assert_eq!(char_width('Z'), 1);
        assert_eq!(char_width('0'), 1);
    }

    #[test]
    fn cjk_is_two() {
        assert_eq!(char_width('あ'), 2);
        assert_eq!(char_width('漢'), 2);
    }

    #[test]
    fn control_is_zero() {
        assert_eq!(char_width('\t'), 0);
        assert_eq!(char_width('\n'), 0);
    }

    #[test]
    fn combining_is_zero() {
        assert_eq!(char_width('\u{0301}'), 0);
    }

    #[test]
    fn ea_ambiguous_narrow() {
        assert_eq!(char_width('±'), 1);
    }

    #[test]
    fn emoji_is_two() {
        assert_eq!(char_width('🦀'), 2);
    }
}
