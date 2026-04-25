# ccmux v0.7.0 — vt100 → vte 移行リリース

## ハイライト

- **Claude Code (DECSTBM 利用 TUI) でホイールスクロールバック遡行が動くようになった** (主目的)
- VT エミュレーション層を `vt100` crate から `vte` パーサ + 自前 Grid/Scrollback に big-bang 置換
- 結果として: scrollback 含むテキスト選択 / OSC 8 ハイパーリンク Ctrl+クリック / OSC 52 クリップボード write / `~/.config/ccmux/config.toml` 設定 / マウス SGR 1006 パススルー が新たに動作

## 変更点 (Breaking なし、内部実装のみ)

### 新機能

- **scrollback 含む範囲選択** + Ctrl+C / Cmd+C コピー (論理座標で reflow しても崩れない)
- **マウス SGR 1006 パススルー** — vim / htop など PTY 側がマウスを欲しがる時のみ ccmux が譲る
- **OSC 8 ハイパーリンク** — Ctrl+左クリックで OS デフォルトブラウザに渡す
- **OSC 52 クリップボード書き込み** — TUI からホストクリップボードへ反映 (read は default off, `osc52.allow_read=true` で opt-in)
- **`~/.config/ccmux/config.toml`** — `scrollback.max_lines` (default 10000), `osc52.allow_read` (default false)

### バグ修正

- **DECSTBM 領域内で scrollback に履歴が積まれない問題を解消** (vt100 crate の構造的制約を撤去) — Claude Code でホイール遡行不可だったやつ
- **Claude Code 画面全体に下線が乗るバグを修正** — `\e[>4;2m` (xterm modifyOtherKeys, intermediate `>`) を SGR と誤解釈していた根本原因を特定 (ECMA-48 §5.4 準拠で intermediate byte を分岐)
- CJK 幅処理を `unicode-width` で再実装、Ambiguous=narrow ポリシーで Claude Code とアラインメント

### 開発者向け

- **TUI 描画バグ診断手順を整備**: [`0_docs/pty-byte-trace-debugging.md`](../../../0_docs/pty-byte-trace-debugging.md)
  - PTY 経由で実バイト列を採取する Python スクリプト
  - SGR / CSI 集計 + 怪しい intermediate を浮かび上がらせる手順
  - in-process opt-in トレース (`CCMUX_TRACE_CSI` / `CCMUX_TRACE_OSC` / `CCMUX_TRACE_ESC`)
- **TUI 横断スモークトレーステスト** (`tests/tui_trace_test.rs`) — claude / vim / less の実バイト列を CI でリプレイ
- スナップショットテスト基盤 (`tests/vt_snapshot_test.rs`)
- 採取スクリプト (`tests/fixtures/capture_tui.py`)

## メトリクス

- 166 tests passing (118 lib + 39 bin + 5 trace + 4 snapshot)
- clippy `-D warnings` clean
- 旧 vt100 crate 依存ゼロ (`cargo tree` / Cargo.lock / src grep いずれも空)

## ロードマップに残るもの

- Windows / WSL での実機テスト (現状 Linux native でのみ確認)
- Wayland / X11 での arboard クリップボード挙動差の確認
- htop など更なる TUI のスモークトレース追加 (依存が増える順次)

## アップグレード手順

```bash
# crates.io 経由
cargo install ccmux

# npm 経由 (Trusted Publishing 経由で自動公開される)
npm install -g ccmux-cli
```

設定ファイル `~/.config/ccmux/config.toml` は不要 (デフォルト動作)。書く場合は:

```toml
[scrollback]
max_lines = 10000

[osc52]
allow_read = false  # true にすると PTY からホストクリップボード読みを許可
```

## 関連リファレンス

- 仕様書: [`docs/PLAN/260425_vt100-to-vte-migration/spec.md`](./spec.md)
- CJK バグ後日談: [`0_docs/cjk-cursor-bug-report.md`](../../../0_docs/cjk-cursor-bug-report.md) §10
- 主要 fix commits: `1487a7c` (Gate A) → `ec3fa23` (Gate B) → `2678464` (Gate C) → `7e9b00b` (Gate D) → `bad05f2` (Gate E) → `6216d2c` (Gate F) → `e386707` (Gate G) → `668b190` (`\e[>4;2m` 根本修正)
