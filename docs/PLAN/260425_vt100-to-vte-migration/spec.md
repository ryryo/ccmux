# vt100 → vte パーサ + 自前 Grid への Big-bang 置換

## Gate 0: 準備 **必須工程(スキップ不可)**

この仕様書の実行には `/dev:spec-run` スキルを使用すること。

**Gate 0 通過条件**: `/dev:spec-run` の実行プロトコルに従い、実行モード(従来 / Codex)を選択済みであること。

---

## 概要

ccmux の VT エミュレーション層を、`vt100` crate(scroll region 有効時に scrollback を保存しない構造的制約あり)から、`vte` パーサ + 自前 Grid/Scrollback/Selection モデルに全面置換する。これにより Claude Code 等 DECSTBM を使う TUI でもスクロールバックが効き、scrollback 含む範囲選択/コピー、reflow、CJK 幅、OSC 8/52 などのターミナル機能を本質的に獲得する。

## 背景

### 直接の動機

ccmux 上で Claude Code を使うと、ログが下に流れていくのに対しマウスホイールで遡れない。原因は `vt100` crate の以下の実装(`grid.rs:566`):

```rust
if self.scrollback_len > 0 && !self.scroll_region_active() {
    self.scrollback.push_back(removed);
}
```

Claude Code は入力欄を最下行に固定するため DECSTBM(`ESC[<top>;<bottom>r`)でスクロール領域を設定する。これにより `scroll_region_active()` が常に true となり、画面外に流れた行は scrollback に**一切保存されない**。その結果 `set_scrollback(N)` を呼んでも 0 にクランプされ、ホイール操作が一切動かない。

### 本質的な対応の意義

業界標準仕様(xterm)では、`scroll_top == 0` であればスクロール領域内であっても画面外に流れた行は scrollback に保存される。これは vim/less/htop/Claude Code など「最下部に固定 UI を持つ TUI」を扱うために必須の挙動。`vt100` はこの仕様を満たしておらず、構造的に修正できない (= scrollback 含む統一セルアクセス API がそもそも貧弱なため、将来予定の「scrollback 含む範囲選択/コピー」要件にも応えられない)。

### 業界ベストプラクティスの選定

本格マルチプレクサ(tmux / Zellij / WezTerm)はいずれも他人の Term 構造体を使わず、低レベル VTE パーサ + 自前グリッドを採用している(理由: 選択・検索・reflow など固有要件が必ず出るため、グリッド層を他者に任せると後で破綻する)。alacritty_terminal は公式に「外部利用非推奨」と明示されており差し替え先としても不適格。よって本リファクタは Zellij と同じ路線、すなわち `vte` crate(状態機械のみ・安定 API)+ 自前 Grid/Scrollback/Selection を採る。

### スコープ拡張の根拠

「スクロール問題の修正」と「scrollback 含む範囲選択/コピー」は、いずれも統一的なセルアクセスを必要とする。vt100 を維持したまま選択機能を後から積み上げるのは二重実装になるため、ここで一度に解く方がベストプラクティスに沿う。

## 設計決定事項

| #  | トピック | 決定 | 根拠 |
|----|---------|-----|-----|
| 1  | パーサ | `vte` crate (parser only) | Zellij と同じ業界ベストプラクティス。状態機械のみ・安定 API |
| 2  | グリッド/scrollback/selection | 自前実装 | 他人の Term 依存は将来要件で破綻する |
| 3  | Reflow | Soft wrap + reflow (WezTerm/iTerm 流) | UX 優先。論理行 vs 表示行の二層モデルを採る |
| 4  | 移行戦略 | Big-bang 置換 | 中途半端な二重メンテを避ける |
| 5  | 描画統合 | ハイブリッド: 自前 Grid → ratatui::Buffer (Widget 実装) | UI chrome は ratatui の差分描画を活かす。PTY セルのみ Buffer に注入 |
| 6  | scrollback 上限 | 設定可能、デフォルト 10000 行 | Claude Code 長セッション対応 |
| 7  | マウスパススルー | 非スクロール時は PTY 転送、scrollback 表示中は ccmux が奪う | WezTerm/Alacritty 同等 |
| 8  | OSC 対応 | 7 / 0 / 2 / 8 / 52 を実装、未対応は外側ターミナルに通過させない(廃棄) | 実用セット。応答系 OSC は要件外のため非対応 |
| 9  | CJK 幅 | `unicode-width` crate、East Asian Ambiguous = narrow | Unicode 公式デフォルト、Claude Code 想定値 |
| 10 | クリップボード | `arboard` + OSC 52 write 許可・read デフォルト off | Alacritty 同等のセキュリティポリシー |
| 11 | 選択座標系 | 論理行座標(reflow 安定) | リサイズ後も選択範囲が崩れない |
| 12 | テスト戦略 | スナップショットテスト + 実アプリ手動 | Big-bang 規模に対する現実解 |

## アーキテクチャ詳細

### モジュール構成 (新規)

```
src/
├── vt/                      ← 新規モジュール (旧 vt100 依存に置換)
│   ├── mod.rs               ← Terminal 構造体 (旧 vt100::Parser に相当する公開 API)
│   ├── cell.rs              ← Cell 型: char + Style (fg/bg/attrs)
│   ├── grid.rs              ← Grid 型: 表示領域 + カーソル + scroll region + alternate screen
│   ├── scrollback.rs        ← Scrollback ring buffer (論理行単位)
│   ├── line.rs              ← LogicalLine 型(soft-wrap対応): セル列 + 折返しメタ
│   ├── parser.rs            ← vte::Parser::Performer 実装
│   ├── csi.rs               ← CSI ハンドラ (SGR/カーソル/DECSTBM/消去/スクロール)
│   ├── osc.rs               ← OSC 7/0/2/8/52 ディスパッチ
│   ├── reflow.rs            ← 論理行 ↔ 表示行の変換、リサイズ時 reflow
│   ├── width.rs             ← CJK 幅計算 (unicode-width ラッパー)
│   ├── selection.rs         ← 選択モデル(論理座標)、ヒットテスト、テキスト抽出
│   └── widget.rs            ← ratatui::Widget 実装 (Grid → Buffer 注入)
├── pane.rs                  ← Pane が vt::Terminal を保持するよう書き換え
├── app.rs                   ← マウス/キー処理を新 API に向けて再配線
├── ui.rs                    ← PtyPaneWidget を呼び出す
└── config.rs                ← 新規: 設定ファイル読み込み
```

### データ構造

```rust
// cell.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,           // 主文字 (combining は別保持)
    pub width: u8,          // 1 or 2 (CJK wide), 0 はワイド文字の続きセル
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttrs,   // bold/italic/underline/reverse/dim/strikethrough/hyperlink_id
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellAttrs {
    pub bits: u16,          // bitfield
    pub hyperlink: u32,     // 0 = なし, 非 0 = HyperlinkRegistry のインデックス
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

// line.rs
pub struct LogicalLine {
    pub cells: Vec<Cell>,    // 論理上の 1 行 (折返しは表示時に計算)
    pub continued: bool,     // 直前の論理行から続いているか (改行で切れていない場合)
}

// grid.rs
pub struct Grid {
    pub rows: u16,
    pub cols: u16,
    pub cursor: Cursor,
    pub saved_cursor: Option<Cursor>,
    pub scroll_top: u16,         // DECSTBM
    pub scroll_bottom: u16,      // DECSTBM
    pub primary: Buffer,         // メイン画面
    pub alternate: Buffer,       // 代替画面
    pub use_alternate: bool,
    pub modes: TerminalModes,    // bracketed paste, mouse modes 等
    pub hyperlinks: HyperlinkRegistry,
}

pub struct Buffer {
    pub visible: Vec<LogicalLine>,    // 現在画面内の論理行 (rows 行ぶん)
    pub scrollback: Scrollback,        // 履歴
}

// scrollback.rs
pub struct Scrollback {
    pub lines: VecDeque<LogicalLine>,
    pub max_lines: usize,
}

// selection.rs
pub struct Selection {
    pub anchor: LogicalPos,
    pub head: LogicalPos,
    pub kind: SelectionKind,   // Linear / Word / Line
}

pub struct LogicalPos {
    /// scrollback 内の場合は scrollback.lines のインデックス、可視領域内なら scrollback.len + visible_index
    pub line: usize,
    pub col: usize,
}
```

### スクロールバックへの保存ルール (xterm 仕様準拠)

```text
新しい行が画面外に流れる際、以下を満たすと scrollback に push する:
  ① alt screen が非アクティブ
  ② scroll_top == 0 (上端が画面最上行)

①②のいずれか一つでも満たさない場合は破棄する。
これは xterm のデフォルト挙動と一致し、vt100 crate の制約を解消する。
```

### Reflow のアルゴリズム

```text
入力: LogicalLine の列 (scrollback + visible)、新しい cols 値
出力: 同じ LogicalLine 列のまま (論理行は不変)、表示行は描画時に都度計算

描画時:
  for each LogicalLine in (scrollback ++ visible):
      total_width = sum of cell widths
      visual_rows = ceil(total_width / cols)
      各 visual row を切り出して描画用バッファへ

論理行は不変なので、リサイズが何度起きても情報損失しない。
カーソルは「最後の論理行の末尾」として論理座標で保持する。
```

### 描画パイプライン

```text
PTY bytes
  → vte::Parser → Performer (parser.rs)
  → Grid 状態更新
  → app.dirty = true
  → render frame
      → ui::render
          → PtyPaneWidget::render(area, buf)
              → Grid から visible 行を計算 (scroll offset 適用)
              → reflow して visual rows を生成
              → ratatui::buffer::Buffer のセルに書き込み
              → 選択範囲があるセルは fg/bg を反転
```

### マウス制御の状態遷移

```text
状態 A (live view): scrollback offset == 0
  - PTY が SGR/X10 マウスモードを有効化していれば、Down/Drag/Up を SGR エンコードして PTY 送信
  - そうでなければ ccmux の選択モデルで処理

状態 B (scrollback view): scrollback offset > 0
  - PTY マウスモードを問わず、ccmux が完全に奪う
  - 選択範囲計算はすべて論理座標で
  - スクロールが 0 に戻ったら状態 A に遷移

scrollback offset が 0 から増えるトリガ:
  - マウスホイール上方向
  - キーバインド (PageUp 等、現状の app.rs を参照)

scrollback offset が 0 に戻るトリガ:
  - キー入力(`pane.scroll_reset()` 既存呼び出し点を維持)
  - PageDown 連打で末端到達
  - 手動の reset ショートカット
```

### 設定ファイル

```toml
# ~/.config/ccmux/config.toml (省略時はデフォルト)
[scrollback]
max_lines = 10000

[osc52]
# clipboard write は常に許可。read のみ設定可
allow_read = false

[mouse]
# 将来の拡張余地。現状はデフォルト挙動のみ
```

ファイル不在時はデフォルト値で動く。設定読込ミスは warning ログで継続。

## 変更対象ファイルと影響範囲

### 変更するファイル

| ファイル | 変更内容 | 影響 |
|---------|---------|-----|
| `Cargo.toml` | `vt100` 削除、`vte`/`unicode-width`/`arboard`/`toml`/`serde` 追加 | 依存関係更新 |
| `src/pane.rs` | `vt100::Parser` → `vt::Terminal` に置換。`scroll_up/down/reset/scrollbar_info/is_scrolled_back` を新 API に再配線 | 中規模リファクタ |
| `src/app.rs` | マウスイベント処理: PTY パススルー判定追加。選択モデルとの統合。`scroll_pane_to_click` を新 API に対応 | 中規模 |
| `src/ui.rs` | PTY セル描画を `PtyPaneWidget` 呼び出しに置換。スクロールバー描画ロジックは新 API を参照 | 中規模 |
| `src/main.rs` | 起動時の設定読み込み追加 | 軽微 |
| `CLAUDE.md` | 「vt100 crate」言及を「自前 vt モジュール (vte ベース)」に書き換え | ドキュメント |
| `docs/cjk-cursor-bug-report.md` | 末尾に「本対応で恒久的に自前グリッドへ移行」と追記 | ドキュメント |

### 新規作成ファイル

| ファイル | 内容 |
|---------|-----|
| `src/vt/mod.rs` | 公開 API (`Terminal`, `Selection`, `Color` など) |
| `src/vt/cell.rs` | Cell / CellAttrs / Color |
| `src/vt/line.rs` | LogicalLine |
| `src/vt/grid.rs` | Grid + Buffer + Cursor + TerminalModes |
| `src/vt/scrollback.rs` | Scrollback ring buffer |
| `src/vt/parser.rs` | `vte::Parser::Performer` 実装 |
| `src/vt/csi.rs` | CSI ディスパッチ (SGR / カーソル / DECSTBM / 消去 / スクロール) |
| `src/vt/osc.rs` | OSC 7 / 0 / 2 / 8 / 52 ハンドラ |
| `src/vt/reflow.rs` | 論理行 → 表示行の射影 |
| `src/vt/width.rs` | unicode-width ラッパー |
| `src/vt/selection.rs` | 選択モデル + テキスト抽出 |
| `src/vt/widget.rs` | `PtyPaneWidget` (`ratatui::Widget` 実装) |
| `src/config.rs` | 設定ファイル読み込み |
| `tests/snapshots/*.bin` | PTY バイト列ゴールデン |
| `tests/snapshots/*.txt` | 期待されるセル状態のテキスト表現 |
| `tests/vt_snapshot_test.rs` | スナップショット駆動テスト |
| `tests/manual-checklist.md` | 実機手動チェックリスト |

### 変更しないファイル

| ファイル | 理由 |
|---------|-----|
| `src/filetree.rs` | サイドバー機能は ratatui チェーンのまま |
| `src/preview.rs` | プレビューは ratatui チェーンのまま |
| `src/claude_monitor.rs` | OSC 0/2 によるタイトル取得を新 API から取得する形に最小調整(タイトル取得経路は同等) |
| `src/version_check.rs` | 無関係 |
| `.github/workflows/release.yml` | リリースフローは無関係 |

## 参照すべきファイル

実装着手前に必ず読むこと。

### コードベース内

| ファイル | 目的 |
|---------|-----|
| `src/pane.rs` | 既存 vt100 利用箇所の網羅 |
| `src/ui.rs` | 既存 PTY セル描画のロジック把握 |
| `src/app.rs:1093-1591` | マウス/スクロール/選択関連の現行コード |
| `CLAUDE.md` | プロジェクト規約 (バージョニング・リリース手順) |
| `docs/cjk-cursor-bug-report.md` | 過去の CJK 幅問題の経緯 |

### 参照資料

外部参照なし(`vte` および `unicode-width` の API は crates.io ドキュメントを参照)。

## レビューステータス

- [ ] **レビュー完了** — 人間による最終確認

## 残存リスク

| リスク | 影響 | 緩和策 |
|-------|-----|------|
| Big-bang による未知のデグレ | 高 | スナップショットテスト + 実アプリ手動チェックリスト(Gate G) を必ずパスさせる |
| OSC 8 ハイパーリンクの ratatui 描画法 | 中 | Buffer に書く時点で URL クリック領域を別管理(`HyperlinkRegistry`)、ホバー時にステータスバー表示 |
| 長 scrollback の reflow パフォーマンス | 中 | 論理行は不変なので reflow は描画時のみ。表示範囲外は計算しない |
| `vte` のバージョン互換 | 低 | パーサ層は薄いラッパなので、API 変更時の追従コストは限定的 |
| `arboard` の Wayland/WSL での挙動差 | 中 | 失敗時はサイレントに warning ログ、機能フォールバック |
| 既存 `claude_monitor` の title 検出ロジックへの影響 | 中 | OSC 0/2 タイトルは vt::Terminal::title() 経由で取得し既存 API を保つ |

## タスクリスト

<!-- generated:begin -->
<!-- このセクションは sync-spec-md が tasks.json から自動生成します。-->
<!-- 手動編集は反映されません。変更は tasks.json に対して行ってください。-->

### 依存関係図

```
Gate A: 基盤データ型
Gate B: vte パーサ統合（Gate A 完了後）
Gate C: Reflow & リサイズ（Gate B 完了後）
Gate D: 描画統合（Gate C 完了後）
Gate E: マウス/選択モデル（Gate D 完了後）
Gate F: 設定・OSC 8/52・既存 API 再配線（Gate E 完了後）
Gate G: テスト・回帰検証（Gate F 完了後）
Gate H: クリーンアップ・ドキュメント（Gate G 完了後）
```

### Gate A: 基盤データ型

> Cell/Line/Buffer/Grid/Scrollback の基本型を定義し、TDD で固める

- [x] **A1**: Cargo.toml 依存差し替え
  > **Review A1**: ⏭️ SKIPPED — 設定のみ(vt100→vte/serde/toml)。cargo build で deps 解決確認、vt100 参照箇所の compile error は仕様通り後続 Todo で解消
- [x] **A2**: [TDD] Cell/Color/CellAttrs 型
  > **Review A2**: ✅ PASSED — 3 レビュアー (correctness/quality/conventions) 全員 PASS。31 unit tests green。設計決定事項 #1/#3/#6/#9 と整合
- [x] **A3**: [TDD] LogicalLine 型 + soft-wrap メタ
  > **Review A3**: ✅ PASSED — 3 レビュアー (correctness/quality/conventions) 全員 PASS。31 unit tests green。設計決定事項 #1/#3/#6/#9 と整合
- [x] **A4**: [TDD] CJK 幅計算 width.rs
  > **Review A4**: ✅ PASSED — 3 レビュアー (correctness/quality/conventions) 全員 PASS。31 unit tests green。設計決定事項 #1/#3/#6/#9 と整合
- [x] **A5**: [TDD] Scrollback ring buffer
  > **Review A5**: ✅ PASSED — 3 レビュアー (correctness/quality/conventions) 全員 PASS。31 unit tests green。設計決定事項 #1/#3/#6/#9 と整合
- [x] **A6**: [TDD] Cursor + Buffer 構造体
  > **Review A6**: ✅ PASSED — 3 レビュアー (correctness/quality/conventions) 全員 PASS。31 unit tests green。設計決定事項 #1/#3/#6/#9 と整合
- [x] **A7**: [TDD] Grid + alternate screen 切替 + DECSTBM 状態
  > **Review A7**: ✅ PASSED — 3 レビュアー (correctness/quality/conventions) 全員 PASS。31 unit tests green。設計決定事項 #1/#3/#6/#9 と整合

**Gate A 通過条件**: 全 Review 結果記入欄が埋まり、総合判定が PASS であること

### Gate B: vte パーサ統合

> vte::Parser::Performer を実装し、CSI/OSC/SGR/DECSTBM を Grid に反映

- [x] **B1**: vte::Performer 実装の骨格
  > **Review B1**: ✅ PASSED (FIX 1回) — Quality/Correctness/Conventions 3レビュアー全員PASS。Performer骨格はvte 0.13仕様準拠 (parser.rs:82-235)、命名/可視性Gate Aと整合。
- [x] **B2**: [TDD] print: 文字書き込み + auto-wrap + CJK ワイド処理
  > **Review B2**: ✅ PASSED (FIX 1回) — 3レビュアー全員PASS。auto-wrap/DECAWM-off/CJK/combining/pending-wrap 全分岐テスト済み。Correctnessから LogicalLine.continued の方向性指摘 (信頼度80-85) → wrap_to_next_line で destination 側にマークするよう修正済み (FIX 1回)。
- [x] **B3**: [TDD] execute: 制御文字 (LF/CR/BS/HT/BEL)
  > **Review B3**: ✅ PASSED (FIX 1回) — 3レビュアー全員PASS。LF/CR/BS/HT/BEL/VT/FF が scroll_up_in_region 経由でスクロールする統合動作も検証。
- [x] **B4**: [TDD] スクロール処理 + scrollback 保存条件
  > **Review B4**: ✅ PASSED (FIX 1回) — 3レビュアー全員PASS。Correctnessは「移行の核心動機 (Claude Code DECSTBM+top==0 で scrollback 喪失) を正しく解決」と評価。should_save_to_scrollback/scroll_up_in_region の choke-point 設計を高評価。
- [x] **B5**: [TDD] CSI ハンドラ: SGR (色 + 属性)
  > **Review B5**: ✅ PASSED (FIX 1回) — 3レビュアー全員PASS。SGR sub-param 処理 (38;5;n / 38;2;r;g;b の colon/semicolon 両形式) が vte 0.13 の Params API に準拠。
- [x] **B6**: [TDD] CSI ハンドラ: カーソル移動 + 消去
  > **Review B6**: ✅ PASSED (FIX 1回) — 3レビュアー全員PASS。CSI ハンドラ群が vte::Perform 仕様準拠、Claude Code パターン (DECSTBM + LF) の統合テスト含む。Correctnessから set_scroll_region の top>=bottom 早期returnは信頼度<80でinformationalのみ。
- [x] **B7**: [TDD] CSI プライベートモード: alt screen / DECAWM / マウス / bracketed paste
  > **Review B7**: ✅ PASSED (FIX 1回) — 3レビュアー全員PASS。?47/?1047 で saved_cursor を保存する点が厳密xterm仕様と差異あるが信頼度<80でinformational。1049 cursor save/restore 動作確認済み。
- [x] **B8**: [TDD] OSC ディスパッチ: 7 / 0 / 2 / 8 / 52
  > **Review B8**: ✅ PASSED (FIX 1回) — 3レビュアー全員PASS。OSC 8 hyperlink dedup/reset、OSC 52 base64 デコード (=/whitespace 対応、不正文字でNone)、ClipboardReadRequested は read 実行せずイベント発行のみで spec決定#10 (allow_read=false) 準拠。
- [x] **B9**: [TDD] ESC ディスパッチ: RIS / DECSC / DECRC / RI
  > **Review B9**: ✅ PASSED (FIX 1回) — 3レビュアー全員PASS。RIS が scrollback/title/cwd/hyperlinks をクリアしない点は信頼度<80でinformational (Gate H で revisit 候補)。

**Gate B 通過条件**: 全 Review 結果記入欄が埋まり、総合判定が PASS であること

### Gate C: Reflow & リサイズ

> soft wrap + 論理行/表示行モデルを実装、リサイズ時の整合性を保つ

- [x] **C1**: [TDD] Reflow: 論理行 → 表示行への射影
  > **Review C1**: ✅ PASSED — Quality/Conventions/Correctness 3並列レビュー全 PASS。CJK 境界保護 (cw>0 ガード)、scrollback→visible の line_index 通し番号、空行 1 VisualRow、start_col 累積視覚幅などの不変条件を確認。Conv: 既存 vt モジュールの flat 構成・テスト inline 配置に整合。
- [x] **C2**: [TDD] Terminal::resize: visible 行数調整 + scroll region クランプ
  > **Review C2**: ✅ PASSED (FIX 1回) — Correctness reviewer (信頼度 85) が Buffer::resize_visible が末尾切詰めで cursor 行コンテンツを失う問題を指摘。修正: cursor.row >= 新 rows のとき scroll_lines_off_top で先頭側を scrollback (alt screen は破棄) に逃がしてから resize_visible。テスト 2 件追加 (resize_shrink_pushes_overflow_to_scrollback / resize_shrink_alt_screen_does_not_save_to_scrollback)。rows=1 退化 scroll region の指摘 (信頼度 80) は consumer 側 early-return で安全のため deferred。

**Gate C 通過条件**: 全 Review 結果記入欄が埋まり、総合判定が PASS であること

### Gate D: 描画統合

> PtyPaneWidget で Grid → ratatui::Buffer に注入、選択オーバーレイ/カーソル/scrollbar を扱う

- [x] **D1**: PtyPaneWidget の骨格
  > **Review D1**: ✅ PASSED — PtyPaneWidget が Widget trait を実装、to_visual_rows で reflow 後に scroll_offset で viewport を切り出し描画。CJK 連続セル(width=0)はスキップ、wide cell は x+1 を空シンボルで埋める ratatui 慣用パターン。Quality/Conventions/Correctness 3並列 PASS。
- [x] **D2**: Color/CellAttrs → ratatui::Style 変換
  > **Review D2**: ✅ PASSED — to_ratatui_style と color_to_rat で Color/CellAttrs → ratatui Style 変換。BOLD/ITALIC/UNDERLINE/REVERSE/DIM/STRIKETHROUGH/BLINK 全網羅。テスト 4 件 (色変換 3 + 属性合成 1)。
- [x] **D3**: ui.rs を PtyPaneWidget 経由に切替
  > **Review D3**: ✅ PASSED — render_terminal_content を PtyPaneWidget 呼び出しに置換。selection は (screen_row, screen_col)→bool クロージャ予述で TextSelection を vt 層から疎結合化。cursor 描画は scroll_offset==0 のときのみ frame.set_cursor_position。Correctness が指摘した cursor の reflow 整合 (信頼度 85, MEDIUM) は Gate E (選択/カーソル) で対応予定。
- [x] **D4**: pane.rs: vt::Terminal 保持に置換
  > **Review D4**: ✅ PASSED (FIX 1回) — parser: vt100::Parser → terminal: vt::Terminal に置換。pty_reader_thread が drain_events で TitleChanged/CwdChanged を取り出して title Mutex / AppEvent::CwdChanged に橋渡し。Bell/Clipboard は F-gate で配線。scroll_offset を Pane 側 AtomicUsize に移動 (UI 状態は VT 状態と分離)。extract_osc7/extract_osc_title (~110 行) を削除。Correctness 指摘 (信頼度 95, HIGH): Windows /c/Users/... → C:\Users\... 変換が抜けていたため vt::osc::parse_file_uri に #[cfg(windows)] ブロックを追加して修正。
- [x] **D5**: [SIMPLE] claude_monitor のタイトル取得経路を新 API に
  > **Review D5**: ✅ PASSED — Pane::title() アクセサを追加 (#[allow(dead_code)])。claude_monitor は title を直接参照していなかったため呼び出し変更は不要。既存 Arc<Mutex<String>> 経路は is_claude_running() でまだ使用中 (削除は spec も「可能」表現で必須ではない)。

**Gate D 通過条件**: 全 Review 結果記入欄が埋まり、総合判定が PASS であること

### Gate E: マウス/選択モデル

> マウスパススルー・選択範囲モデル・コピー・スクロール統合

- [x] **E1**: [TDD] Selection モデル + テキスト抽出
  > **Review E1**: ✅ PASSED (FIX 1回) — vt::selection module + tests; extract_text per-segment trim fix from review
- [x] **E2**: [TDD] スクリーン座標 → 論理座標 変換
  > **Review E2**: ✅ PASSED — screen_to_logical with CJK continuation snap; start_cell_idx added to VisualRow
- [x] **E3**: app.rs: マウスダウン/ドラッグ/アップで Selection 更新
  > **Review E3**: ✅ PASSED (FIX 1回) — SGR 1006 encoder + try_forward_mouse_to_pty; gated on is_mouse_sgr_enabled per reviewer feedback
- [x] **E4**: Ctrl+C / 自動コピーで arboard 経由クリップボード書き込み
  > **Review E4**: ✅ PASSED — Ctrl+C copies selection (wired in D4); Esc clears selection
- [x] **E5**: ホイール/キースクロールを新 scroll_offset へ統合
  > **Review E5**: ✅ PASSED — Wheel scroll already routes through pane.scroll_up/down(3) since D4
- [x] **E6**: scroll_offset > 0 の間は PTY マウスを抑制
  > **Review E6**: ✅ PASSED — Esc on pane clears selection or exits scrollback view; PTY mouse passthrough requires scroll_offset==0
- [x] **E7**: [TDD] ダブル/トリプルクリック (Word/Line 選択)
  > **Review E7**: ✅ PASSED (FIX 1回) — Double/triple click expand via vt::selection::expand_word; ±1 cell jitter tolerance from review

**Gate E 通過条件**: 全 Review 結果記入欄が埋まり、総合判定が PASS であること

### Gate F: 設定・OSC 8/52・既存 API 再配線

> config.toml 読込、ハイパーリンクとクリップボード、pane.rs の旧 API を新 Grid に向けて再配線

- [x] **F1**: [SIMPLE] config.rs: TOML 読み込み
  > **Review F1**: ✅ PASSED — PASS — defaults match, plumbing OK; minor stacked-derive cleaned up after review
- [x] **F2**: OSC 52 write/read の応答処理
  > **Review F2**: ✅ PASSED — PASS — write unconditional, read gated by allow_read, base64_encode verified
- [x] **F3**: OSC 8 ハイパーリンク描画 + クリック処理
  > **Review F3**: ✅ PASSED — PASS — hyperlink_at uses same visual-walk semantics as widget; non-hit falls through; CellAttrs unused-import suppressor removed after review
- [x] **F4**: [SIMPLE] cursor 描画と auto-scroll-to-bottom 動作
  > **Review F4**: ✅ PASSED — PASS — confirmed existing cursor gate is_focused && scroll_offset==0; PtyOutput no-op intentional
- [x] **F5**: ビルド通過 + warnings ゼロ確認
  > **Review F5**: ✅ PASSED — PASS — clippy -D warnings clean, release build OK, 145 tests passing

**Gate F 通過条件**: 全 Review 結果記入欄が埋まり、総合判定が PASS であること

### Gate G: テスト・回帰検証

> スナップショットテストと実アプリ手動チェックリストの整備

- [ ] **G1**: スナップショットテスト基盤
  > **Review G1**: _未記入_
- [ ] **G2**: スナップショット: Claude Code 模擬セッション
  > **Review G2**: _未記入_
- [ ] **G3**: スナップショット: alt screen / vim 風入退出
  > **Review G3**: _未記入_
- [ ] **G4**: スナップショット: CJK + auto-wrap
  > **Review G4**: _未記入_
- [ ] **G5**: スナップショット: SGR 色 + 属性継続
  > **Review G5**: _未記入_
- [ ] **G6**: 実機手動チェックリスト作成
  > **Review G6**: _未記入_
- [ ] **G7**: 実機手動通しテスト 1 回目
  > **Review G7**: _未記入_

**Gate G 通過条件**: 全 Review 結果記入欄が埋まり、総合判定が PASS であること

### Gate H: クリーンアップ・ドキュメント

> vt100 依存削除確認、CLAUDE.md とバグレポートの追記

- [ ] **H1**: [SIMPLE] vt100 依存の最終削除確認
  > **Review H1**: _未記入_
- [ ] **H2**: [SIMPLE] CLAUDE.md と cjk-cursor-bug-report 追記
  > **Review H2**: _未記入_
- [ ] **H3**: [SIMPLE] バージョン bump とリリースノート下書き
  > **Review H3**: _未記入_

**Gate H 通過条件**: 全 Review 結果記入欄が埋まり、総合判定が PASS であること

<!-- generated:end -->
