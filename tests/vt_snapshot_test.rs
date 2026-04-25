//! Snapshot tests for the vte-based terminal emulation (Gate G).
//!
//! Each test feeds a deterministic byte stream into `vt::parser::Terminal`,
//! renders the resulting state to a textual form, and compares against the
//! checked-in `tests/snapshots/{name}.txt`. This guards against regressions
//! in CSI handling, scrollback retention, alt-screen behaviour, CJK width,
//! and SGR attribute propagation.
//!
//! Set `CCMUX_UPDATE_SNAPSHOTS=1` to overwrite the expected files.

use std::path::PathBuf;

use ccmux::vt::cell::{Cell, Color};
use ccmux::vt::parser::Terminal;

fn snapshots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots")
}

/// Render the terminal state (scrollback + visible) to a deterministic text
/// representation. Lines from scrollback are prefixed `S | `, visible lines
/// `V | `. Wide-character continuations are skipped (the main cell prints
/// the glyph). Trailing blanks on each line are stripped to keep the
/// snapshot stable across cursor moves into untouched cells.
fn render_grid(t: &Terminal) -> String {
    let buf = t.grid.current_buffer();
    let mut out = String::new();
    for line in buf.scrollback.iter() {
        out.push_str("S | ");
        push_line(&mut out, &line.cells);
        out.push('\n');
    }
    for line in &buf.visible {
        out.push_str("V | ");
        push_line(&mut out, &line.cells);
        out.push('\n');
    }
    out
}

fn push_line(out: &mut String, cells: &[Cell]) {
    let mut text = String::new();
    for cell in cells {
        if cell.width == 0 {
            // Continuation cell of a wide glyph: already represented by the main cell.
            continue;
        }
        if cell.ch == '\0' {
            text.push(' ');
        } else {
            text.push(cell.ch);
        }
    }
    while text.ends_with(' ') {
        text.pop();
    }
    out.push_str(&text);
}

/// Like `render_grid` but also annotates each non-blank cell with its
/// foreground colour and attribute bits. Used by the SGR snapshot.
fn render_grid_styled(t: &Terminal) -> String {
    let buf = t.grid.current_buffer();
    let mut out = String::new();
    for (idx, line) in buf.visible.iter().enumerate() {
        out.push_str(&format!("row {idx}:\n"));
        for cell in &line.cells {
            if cell.width == 0 {
                continue;
            }
            if cell.ch == ' ' && cell.fg == Color::Default && cell.bg == Color::Default && cell.attrs.bits == 0 {
                continue;
            }
            out.push_str(&format!(
                "  {:?} fg={} bits={:#04x} link={}\n",
                cell.ch,
                fmt_color(cell.fg),
                cell.attrs.bits,
                cell.attrs.hyperlink,
            ));
        }
    }
    out
}

fn fmt_color(c: Color) -> String {
    match c {
        Color::Default => "default".into(),
        Color::Indexed(i) => format!("idx{i}"),
        Color::Rgb(r, g, b) => format!("rgb({r},{g},{b})"),
    }
}

/// Compare `actual` to the contents of `tests/snapshots/{name}.txt`. When
/// the env var `CCMUX_UPDATE_SNAPSHOTS=1` is set, the expected file is
/// overwritten instead of asserted against.
fn assert_snapshot(name: &str, actual: &str) {
    let path = snapshots_dir().join(format!("{name}.txt"));
    if std::env::var("CCMUX_UPDATE_SNAPSHOTS").is_ok() {
        std::fs::write(&path, actual).unwrap_or_else(|e| {
            panic!("failed to write snapshot {}: {e}", path.display())
        });
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing snapshot {}: {e}\n--- actual ---\n{actual}",
            path.display()
        )
    });
    if expected != actual {
        panic!(
            "snapshot {} mismatch\n--- expected ---\n{expected}--- actual ---\n{actual}",
            name
        );
    }
}

// ─── G2: scroll-region (DECSTBM) preserves scrollback overflow ───
//
// xterm semantics: when DECSTBM defines a top/bottom margin, line feeds at
// the bottom margin scroll lines OUT of the region. On the primary buffer
// those evicted lines must land in scrollback even though the region does
// not cover the full screen — this is the bug that motivated the vte
// migration (Claude Code uses DECSTBM heavily).

#[test]
fn snapshot_g2_decstbm_preserves_scrollback() {
    let mut t = Terminal::new(24, 20, 1000);
    // Scroll region rows 1..=20 (1-indexed inclusive).
    t.process(b"\x1b[1;20r");
    // Pin cursor inside the region.
    t.process(b"\x1b[1;1H");
    for i in 0..30u32 {
        t.process(format!("L{i}\r\n").as_bytes());
    }
    let actual = render_grid(&t);
    assert_snapshot("g2_decstbm", &actual);
    // Hard invariants beyond the textual snapshot.
    let buf = t.grid.current_buffer();
    assert!(
        buf.scrollback.len() >= 10,
        "DECSTBM overflow must accumulate in scrollback (got {})",
        buf.scrollback.len()
    );
}

// ─── G3: alt-screen entry/exit doesn't leak into scrollback ───

#[test]
fn snapshot_g3_alt_screen_isolates_writes() {
    let mut t = Terminal::new(8, 20, 1000);
    // Primary: write 5 distinct lines.
    for i in 0..5u32 {
        t.process(format!("primary{i}\r\n").as_bytes());
    }
    let scrollback_before = t.grid.current_buffer().scrollback.len();
    // Enter alt screen.
    t.process(b"\x1b[?1049h");
    for i in 0..3u32 {
        t.process(format!("alt{i}\r\n").as_bytes());
    }
    // Exit alt screen — primary should be restored unchanged.
    t.process(b"\x1b[?1049l");
    let actual = render_grid(&t);
    assert_snapshot("g3_alt_screen", &actual);
    let scrollback_after = t.grid.current_buffer().scrollback.len();
    assert_eq!(
        scrollback_before, scrollback_after,
        "alt-screen writes must not bleed into primary scrollback"
    );
}

// ─── G4: CJK + auto-wrap respects character boundaries ───

#[test]
fn snapshot_g4_cjk_wrap_at_boundary() {
    let mut t = Terminal::new(4, 10, 1000);
    // 10 CJK chars × width 2 = 20 cells, must wrap at the 5-char boundary.
    t.process("あいうえおかきくけこ".as_bytes());
    let actual = render_grid(&t);
    assert_snapshot("g4_cjk_wrap", &actual);
    // The first visible line must be exactly 10 visual cells of width-2 chars.
    let buf = t.grid.current_buffer();
    let first = &buf.visible[0];
    let visual_width: usize = first.cells.iter().map(|c| c.width as usize).sum();
    assert!(
        visual_width <= 10,
        "first wrapped line exceeds cols: visual_width={visual_width}"
    );
}

// ─── G5: SGR colour + attribute tracking ───

#[test]
fn snapshot_g5_sgr_colors_and_reset() {
    let mut t = Terminal::new(2, 30, 1000);
    t.process(b"\x1b[31mred\x1b[0mplain\x1b[1;38;2;0;255;0mgreen-bold");
    let actual = render_grid_styled(&t);
    assert_snapshot("g5_sgr_colors", &actual);
}
