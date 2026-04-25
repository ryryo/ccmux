use crossterm::event::{MouseButton, MouseEventKind};

/// Encode a mouse event in SGR (DECSET 1006) format:
///   ESC [ < <button> ; <col> ; <row> M  (press / move)
///   ESC [ < <button> ; <col> ; <row> m  (release)
/// `col` and `row` are 1-based, relative to the PTY screen (not the
/// terminal multiplexer's parent area).
pub fn encode_sgr(kind: MouseEventKind, col: u16, row: u16, modifiers: u8) -> Option<Vec<u8>> {
    let (button, suffix) = match kind {
        MouseEventKind::Down(b) => (button_code(b), 'M'),
        MouseEventKind::Up(b) => (button_code(b), 'm'),
        MouseEventKind::Drag(b) => (button_code(b) + 32, 'M'),
        MouseEventKind::ScrollUp => (64, 'M'),
        MouseEventKind::ScrollDown => (65, 'M'),
        MouseEventKind::ScrollLeft => (66, 'M'),
        MouseEventKind::ScrollRight => (67, 'M'),
        MouseEventKind::Moved => return None,
    };
    let code = button + modifiers as u16;
    Some(format!("\x1b[<{};{};{}{}", code, col.max(1), row.max(1), suffix).into_bytes())
}

fn button_code(b: MouseButton) -> u16 {
    match b {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn down_left_press() {
        let bytes = encode_sgr(MouseEventKind::Down(MouseButton::Left), 5, 3, 0).unwrap();
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "\x1b[<0;5;3M");
    }

    #[test]
    fn up_left_release_uses_lowercase_m() {
        let bytes = encode_sgr(MouseEventKind::Up(MouseButton::Left), 5, 3, 0).unwrap();
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "\x1b[<0;5;3m");
    }

    #[test]
    fn drag_adds_motion_bit() {
        let bytes = encode_sgr(MouseEventKind::Drag(MouseButton::Left), 1, 1, 0).unwrap();
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "\x1b[<32;1;1M");
    }

    #[test]
    fn scroll_up_button_64() {
        let bytes = encode_sgr(MouseEventKind::ScrollUp, 10, 20, 0).unwrap();
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "\x1b[<64;10;20M");
    }

    #[test]
    fn modifiers_added() {
        // Ctrl = 16
        let bytes = encode_sgr(MouseEventKind::Down(MouseButton::Right), 1, 1, 16).unwrap();
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "\x1b[<18;1;1M");
    }
}
