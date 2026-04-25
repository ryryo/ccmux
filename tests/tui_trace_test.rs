//! Gate H4 — TUI 横断スモークトレーステスト
//!
//! `tests/snapshots/tui_traces/{name}.bin` に置いた実 PTY 採取バイト列を
//! `vt::parser::Terminal` に流し、終了時の cursor pen / cell 状態が
//! 想定どおりであることを確認する。`\e[>4;2m` を SGR と誤解釈していた
//! 回帰 (commit 668b190) の発見プロセスをそのまま CI 化したもの。
//!
//! 採取は `tests/fixtures/capture_tui.py` で人手実行 (claude / vim / less /
//! htop が必要)。CI は採取済みの .bin をリプレイするだけで完結する。

use std::fs;
use std::path::PathBuf;

use ccmux::vt::cell::CellAttrs;
use ccmux::vt::parser::Terminal;

fn traces_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/tui_traces")
}

fn load_trace(name: &str) -> Option<Vec<u8>> {
    let path = traces_dir().join(format!("{name}.bin"));
    fs::read(&path).ok()
}

/// Replay the trace and return the resulting Terminal. Uses 40×120 to match
/// the capture script.
fn replay(bytes: &[u8]) -> Terminal {
    let mut t = Terminal::new(40, 120, 1000);
    t.process(bytes);
    t
}

/// Count cells in the visible buffer that carry `flag`.
fn cells_with_attr(t: &Terminal, flag: u16) -> usize {
    let mut n = 0;
    for line in &t.grid.primary.visible {
        for cell in &line.cells {
            if cell.attrs.contains(flag) {
                n += 1;
            }
        }
    }
    n
}

/// Assert basic invariants that should hold after any well-behaved TUI
/// session — no permanent attribute leak on the cursor pen, no all-cells-
/// underlined catastrophe, scrollback is non-negative, etc.
fn assert_no_pen_leak(name: &str, t: &Terminal) {
    let bits = t.grid.cursor.style.bits;
    assert_eq!(
        bits, 0,
        "[{name}] cursor.style.bits leaked at end of trace: {bits:#06x}"
    );

    // None of the visible cells should be underlined unless the TUI was
    // actively rendering an underlined span at exit. That's vanishingly
    // unlikely for the simple "open + scroll + quit" scripts we run, so
    // gate at <= 5% to allow for legitimate transient styling.
    let total_non_blank: usize = t
        .grid
        .primary
        .visible
        .iter()
        .flat_map(|l| &l.cells)
        .filter(|c| c.ch != ' ' && c.ch != '\0')
        .count();
    let underlined = cells_with_attr(t, CellAttrs::UNDERLINE);
    if total_non_blank > 0 {
        let ratio = underlined as f64 / total_non_blank as f64;
        assert!(
            ratio < 0.05,
            "[{name}] {underlined}/{total_non_blank} cells underlined ({:.1}%) — \
             likely a regression of the `\\e[>4;2m` SGR-misinterpretation bug",
            ratio * 100.0
        );
    }
}

#[test]
fn claude_trace_does_not_leak_underline() {
    let Some(bytes) = load_trace("claude") else {
        eprintln!("skipping: claude.bin not present (run capture_tui.py)");
        return;
    };
    let t = replay(&bytes);
    assert_no_pen_leak("claude", &t);
}

#[test]
fn vim_trace_does_not_leak_attrs() {
    let Some(bytes) = load_trace("vim") else {
        eprintln!("skipping: vim.bin not present");
        return;
    };
    let t = replay(&bytes);
    // vim leaves an alt-screen + DECSTBM session and should restore on :q!
    assert_no_pen_leak("vim", &t);
    // After :q!, alt screen must be exited.
    assert!(
        !t.grid.use_alternate,
        "[vim] alt screen should have been exited"
    );
}

#[test]
fn less_trace_does_not_leak_attrs() {
    let Some(bytes) = load_trace("less") else {
        eprintln!("skipping: less.bin not present");
        return;
    };
    let t = replay(&bytes);
    assert_no_pen_leak("less", &t);
    assert!(
        !t.grid.use_alternate,
        "[less] alt screen should have been exited"
    );
}

#[test]
fn htop_trace_does_not_leak_attrs() {
    let Some(bytes) = load_trace("htop") else {
        eprintln!("skipping: htop.bin not present");
        return;
    };
    let t = replay(&bytes);
    assert_no_pen_leak("htop", &t);
}

/// Specific regression: the smoking-gun sequence from commit 668b190.
/// We embed the literal bytes here in addition to the captured trace so the
/// test still catches the bug even if the trace file is rotated.
#[test]
fn xterm_modify_other_keys_in_isolation() {
    let mut t = Terminal::new(2, 10, 100);
    t.process(b"\x1b[>4;2m");
    assert_eq!(t.grid.cursor.style.bits, 0);
}
