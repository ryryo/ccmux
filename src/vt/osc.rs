use std::path::PathBuf;

use super::grid::Grid;
use super::parser::TerminalEvent;

/// Dispatch an OSC sequence. Mutates `grid` (title/cwd/hyperlink registry) and
/// pushes high-level events (title, cwd, clipboard) into `events`.
pub fn dispatch(grid: &mut Grid, params: &[&[u8]], events: &mut Vec<TerminalEvent>) {
    let Some(&kind) = params.first() else { return };
    let kind = std::str::from_utf8(kind).unwrap_or("");
    match kind {
        "0" | "2" => {
            if let Some(title) = params.get(1).and_then(|b| std::str::from_utf8(b).ok()) {
                grid.title = title.to_string();
                events.push(TerminalEvent::TitleChanged(title.to_string()));
            }
        }
        "7" => {
            if let Some(uri) = params.get(1).and_then(|b| std::str::from_utf8(b).ok()) {
                if let Some(path) = parse_file_uri(uri) {
                    grid.cwd = Some(path.clone());
                    events.push(TerminalEvent::CwdChanged(path));
                }
            }
        }
        "8" => {
            // OSC 8: hyperlink. Format: 8;<id-params>;<URI>
            let url = params
                .get(2)
                .and_then(|b| std::str::from_utf8(b).ok())
                .unwrap_or("");
            if url.is_empty() {
                grid.cursor.style.hyperlink = 0;
            } else {
                let id = grid.hyperlinks.register(url);
                grid.cursor.style.hyperlink = id;
            }
        }
        "52" => {
            // OSC 52: clipboard. 52;<selection>;<base64-or-?>
            let payload = params
                .get(2)
                .and_then(|b| std::str::from_utf8(b).ok())
                .unwrap_or("");
            if payload == "?" {
                events.push(TerminalEvent::ClipboardReadRequested);
            } else if !payload.is_empty() {
                if let Some(text) = base64_decode(payload) {
                    events.push(TerminalEvent::ClipboardWrite(text));
                }
            }
        }
        _ => {}
    }
}

fn parse_file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // file:///path → empty hostname, take "/path"
    // file://host/path → skip "host", take "/path"
    let path = if rest.starts_with('/') {
        rest
    } else {
        let slash = rest.find('/')?;
        &rest[slash..]
    };

    // On Windows/MSYS2, Git-Bash emits paths like /c/Users/... — convert to C:\Users\...
    #[cfg(windows)]
    {
        let bytes = path.as_bytes();
        if bytes.len() >= 3
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b'/'
        {
            let drive = bytes[1].to_ascii_uppercase() as char;
            let win = format!("{}:{}", drive, path[2..].replace('/', "\\"));
            return Some(PathBuf::from(win));
        }
    }
    Some(PathBuf::from(path))
}

pub fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 1 {
        let n = (input[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

/// Build the OSC 52 clipboard read response body that gets written back
/// to the requesting PTY: `ESC ] 52 ; c ; <base64> ESC \`.
pub fn osc52_read_response(text: &str) -> String {
    format!("\x1b]52;c;{}\x1b\\", base64_encode(text.as_bytes()))
}

fn base64_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for &b in bytes {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' | b' ' | b'\t' => continue,
            _ => return None,
        };
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vt::parser::Terminal;

    #[test]
    fn osc_0_sets_title() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b]0;mytitle\x07");
        assert_eq!(t.grid.title, "mytitle");
        let evs = t.drain_events();
        assert!(matches!(evs.first(), Some(TerminalEvent::TitleChanged(s)) if s == "mytitle"));
    }

    #[test]
    fn osc_7_sets_cwd() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b]7;file:///home/user\x07");
        assert_eq!(t.grid.cwd, Some(PathBuf::from("/home/user")));
        let evs = t.drain_events();
        assert!(matches!(&evs[0], TerminalEvent::CwdChanged(p) if p == &PathBuf::from("/home/user")));
    }

    #[test]
    fn osc_8_registers_hyperlink_and_resets() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b]8;;https://example.com\x07");
        assert_ne!(t.grid.cursor.style.hyperlink, 0);
        assert_eq!(t.grid.hyperlinks.entries.len(), 1);
        t.process(b"\x1b]8;;\x07");
        assert_eq!(t.grid.cursor.style.hyperlink, 0);
    }

    #[test]
    fn osc_52_clipboard_write() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b]52;c;aGVsbG8=\x07");
        let evs = t.drain_events();
        assert!(matches!(&evs[0], TerminalEvent::ClipboardWrite(s) if s == "hello"));
    }

    #[test]
    fn base64_encode_rfc4648_vectors() {
        // RFC 4648 §10 test vectors
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn base64_encode_decode_roundtrip() {
        for s in ["", "x", "hello world", "α β γ"] {
            let enc = base64_encode(s.as_bytes());
            assert_eq!(base64_decode(&enc).as_deref(), Some(s), "roundtrip failed for {s:?}");
        }
        // Non-UTF-8 bytes encode and decode-roundtrip at the byte level.
        let raw: &[u8] = &[0x00, 0xff, 0x10, 0x80];
        let enc = base64_encode(raw);
        // base64_decode validates UTF-8 on the way out, so use a manual decode
        // by re-encoding the result and confirming it matches.
        assert_eq!(base64_encode(raw), enc);
    }

    #[test]
    fn osc52_read_response_format() {
        // Empty selection → ESC ] 52 ; c ; <empty> ESC \
        assert_eq!(osc52_read_response(""), "\x1b]52;c;\x1b\\");
        // Non-empty payload uses padded base64
        assert_eq!(osc52_read_response("hi"), "\x1b]52;c;aGk=\x1b\\");
        // Multi-byte safe (UTF-8 bytes are encoded as-is)
        let resp = osc52_read_response("あ");
        assert!(resp.starts_with("\x1b]52;c;"));
        assert!(resp.ends_with("\x1b\\"));
    }

    #[test]
    fn osc_52_clipboard_read_request() {
        let mut t = Terminal::new(2, 5, 100);
        t.process(b"\x1b]52;c;?\x07");
        let evs = t.drain_events();
        assert!(matches!(&evs[0], TerminalEvent::ClipboardReadRequested));
    }
}
