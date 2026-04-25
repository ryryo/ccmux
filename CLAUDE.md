# ccmux — Claude Code Multiplexer

## Overview
Rust TUI tool for managing multiple Claude Code instances in split panes.

## Tech Stack
- Rust (stable), ratatui + crossterm, portable-pty
- 自前 VT エミュレータ (`src/vt/`): vte パーサ + 自前 Grid/Scrollback/Selection
  (旧 vt100 crate は v0.7.0 で全削除。経緯は `docs/PLAN/260425_vt100-to-vte-migration/`)

## Build & Run
```bash
cargo build          # Debug build
cargo build --release # Release build
cargo test           # Run tests (snapshot + TUI trace を含む)
cargo run            # Run the app
```

## Architecture
- `main.rs` — Entry point, terminal setup, event loop
- `app.rs` — App state, event dispatching, layout tree, OSC 52 routing
- `pane.rs` — PTY management, vt::Terminal 保持、scroll offset
- `ui.rs` — ratatui rendering, layout calculation, theme
- `vt/` — VT エミュレータ (parser/grid/scrollback/reflow/selection/widget/osc/csi)
- `filetree.rs` — File tree sidebar
- `preview.rs` — File preview panel
- `config.rs` — `~/.config/ccmux/config.toml` 読み込み (scrollback.max_lines, osc52.allow_read)

## Key Design Decisions
- **vte パーサ + 自前 Grid** (Zellij と同じ路線)。vt100 crate は scroll region 有効時に scrollback を保存しない構造的制約があり、Claude Code の DECSTBM ベース UI でホイール遡行が効かなかったため big-bang 置換した。
- **論理行 + 描画時 reflow** — リサイズで選択範囲や scrollback が崩れない。
- **Binary tree layout** for recursive pane splitting
- **Per-PTY reader threads** with mpsc channel to main event loop
- PTY resize via `master_pty.resize()` + `vt::Terminal::resize()`

## Debugging TUI rendering bugs
特定のアプリ (Claude Code 等) でだけ起きる描画異常は、**スクショや推測ではなく実
バイト列を採取**して原因を特定する。標準手順: `0_docs/pty-byte-trace-debugging.md`。
回帰検出は `tests/tui_trace_test.rs` で claude/vim/less の採取済みバイト列を
リプレイ。新たな TUI を追加するときは `tests/fixtures/capture_tui.py` で
`tests/snapshots/tui_traces/{name}.bin` を採取して同テストに追加する。

CSI / OSC / ESC の opt-in トレース:
```bash
CCMUX_TRACE_CSI=/tmp/csi.log ccmux   # CSI 'm' を全部記録
CCMUX_TRACE_OSC=/tmp/osc.log ccmux
CCMUX_TRACE_ESC=/tmp/esc.log ccmux
```

## Shell Detection Priority
- Windows: Git Bash → PowerShell
- Unix: $SHELL → /bin/sh

## Release Process
1. `Cargo.toml` と `npm/package.json` のバージョンを同じ値に揃えて上げる
2. コミット & `git push origin master`
3. `git tag vX.Y.Z && git push origin vX.Y.Z`
4. CI (`.github/workflows/release.yml`) が自動で実行:
   - 4プラットフォーム (Windows x64, macOS x64/arm64, Linux x64) のリリースビルド
   - GitHub Release 作成 + checksums.txt 生成
   - npm publish (Trusted Publishing)
- **手動で `npm publish` や `gh release create` しないこと** — バージョン衝突の原因になる

## Workflow Rules
- **Every implementation must be reviewed by the evaluator agent** before reporting done. This is a Rust TUI app, so Playwright MCP is not available — the evaluator should perform static review (diff analysis, edge cases, logic correctness, key conflict checks, layout math consistency).
