# ccmux

Claude Code Multiplexer — TUI 分割ペインで複数の Claude Code インスタンスを管理するターミナルマルチプレクサ。

> **このリポジトリは [Shin-sibainu/ccmux](https://github.com/Shin-sibainu/ccmux) のフォーク**です。
> フォーク元からの主な変更点は [フォーク元との差分](#フォーク元との差分) を参照してください。

![ccmux screenshot](screenshot.png)

## 機能

- **マルチペイン** — 上下・左右に分割して独立した PTY シェルを実行
- **タブワークスペース** — 複数プロジェクトタブ、クリックで切り替え
- **ファイルツリーサイドバー** — アイコン付きディレクトリ表示、展開/折りたたみ
- **シンタックスハイライトプレビュー** — 言語を認識した色付きファイル内容表示
- **Claude Code 検出** — Claude Code 起動中はペインのボーダーがオレンジに変わる
- **cd トラッキング** — ディレクトリ移動に合わせてファイルツリーとタブ名が自動更新
- **マウス対応** — クリックでフォーカス、ドラッグでリサイズ、スクロールで履歴遡行
- **スクロールバック** — ペインごとに最大 10,000 行の端末履歴
- **ライト / ダークテーマ** — `config.toml` で切り替え可能なカラースキーム
- **キーバインドカスタマイズ** — `config.toml` の `[keybindings]` で全操作を再定義可能
- **クロスプラットフォーム** — Windows / macOS / Linux
- **シングルバイナリ** — 約 1MB、ランタイム依存なし

## インストール

### ソースからビルド

```bash
git clone https://github.com/ryryo/ccmux.git
cd ccmux
cargo build --release
# バイナリ: target/release/ccmux (Windows では ccmux.exe)
```

[Rust](https://rustup.rs/) ツールチェーンが必要です。

## 使い方

```bash
ccmux
```

任意のディレクトリから起動するとファイルツリーにそのディレクトリが表示されます。

## キーバインド

### ペインモード（デフォルト）

| キー | 操作 |
|-----|--------|
| `Ctrl+D` | 垂直分割 |
| `Ctrl+E` | 水平分割 |
| `Ctrl+W` | ペイン / タブを閉じる |
| `Alt+T` / `Ctrl+T` | 新しいタブ |
| `Alt+1..9` | タブ N に移動 |
| `Alt+Left/Right` | 前 / 次のタブ |
| `Alt+R` | タブ名を変更（セッション中のみ） |
| `Alt+S` | ステータスバーの表示切り替え |
| `Ctrl+F` | ファイルツリーの表示切り替え |
| `Ctrl+P` | プレビュー / ターミナルレイアウト切り替え |
| `Ctrl+Right/Left` | フォーカス循環（サイドバー・プレビュー・ペイン） |
| `Ctrl+Q` | 終了 |

### ファイルツリーモード（`Ctrl+F` 後）

| キー | 操作 |
|-----|--------|
| `j` / `k` | 選択を移動 |
| `Enter` | ファイルを開く / ディレクトリを展開 |
| `y` | 相対パスをクリップボードにコピー |
| `Ctrl+Y` | 絶対パスをクリップボードにコピー |
| `.` | 隠しファイルの表示切り替え |
| `Esc` | ペインに戻る |

### プレビューモード（プレビューにフォーカス後）

| キー | 操作 |
|-----|--------|
| `j` / `k` | 縦スクロール |
| `h` / `l` | 横スクロール |
| `Ctrl+W` | プレビューを閉じる |
| `Esc` | ペインに戻る |

### マウス操作

| 操作 | 効果 |
|--------|--------|
| ペインをクリック | フォーカスを移動 |
| タブをクリック | タブを切り替え |
| タブをダブルクリック | タブ名を変更 |
| `+` をクリック | 新しいタブを作成 |
| ボーダーをドラッグ | パネルをリサイズ |
| スクロールホイール | ファイルツリー / プレビュー / 端末履歴をスクロール |

## フォーク元との差分

このフォークは [Shin-sibainu/ccmux](https://github.com/Shin-sibainu/ccmux) をベースに、VT エミュレータを**フルスクラッチで再実装**しています。

### VT エミュレータを vt100 crate → 自前実装 (vte ベース) に置き換え

フォーク元は `vt100` crate を使用していましたが、以下の構造的制約がありました:

- **スクロール領域 (DECSTBM) 有効時にスクロールバックを保存しない** — Claude Code は DECSTBM ベースの UI を使用するため、ホイールで過去の出力を遡れなかった

この問題を根本解決するために `src/vt/` に自前エミュレータを実装しました (Zellij と同じアーキテクチャ):

| コンポーネント | 説明 |
|--------------|------|
| `src/vt/parser.rs` | vte クレートによる VT シーケンス解析 |
| `src/vt/grid.rs` | 端末グリッド（論理行管理） |
| `src/vt/scrollback.rs` | スクロールバック履歴 |
| `src/vt/reflow.rs` | リサイズ時の論理行 reflow |
| `src/vt/selection.rs` | マウスによるテキスト選択 |
| `src/vt/cell.rs` | セル（文字・色・属性） |
| `src/vt/csi.rs` | CSI シーケンス処理 |
| `src/vt/osc.rs` | OSC シーケンス処理（OSC 52 クリップボード、OSC 8 ハイパーリンク） |
| `src/vt/widget.rs` | ratatui への描画ブリッジ |

### 論理行 + 描画時 reflow

- リサイズしてもスクロールバックや選択範囲が崩れない
- 長い行が折り返されても論理的に 1 行として扱われる

### opt-in トレース機能

描画バグの診断用に CSI/OSC/ESC のトレースを環境変数で有効化できます:

```bash
CCMUX_TRACE_CSI=/tmp/csi.log ccmux
CCMUX_TRACE_OSC=/tmp/osc.log ccmux
CCMUX_TRACE_ESC=/tmp/esc.log ccmux
```

### 設定ファイル

`~/.config/ccmux/config.toml` で動作をカスタマイズできます:

```toml
[theme]
mode = "light"  # "light" | "dark"  (デフォルト: "light")

[scrollback]
max_lines = 10000

[osc52]
allow_read = false

# キーバインドをスコープごとに上書き
# スコープ: global / pane / file_tree / preview
# 値に "none" を指定すると既定の束縛を無効化できる
[keybindings.global]
quit = "ctrl+q"
new_tab = "ctrl+t"

[keybindings.pane]
split_vertical = "ctrl+d"
split_horizontal = "ctrl+e"

[keybindings.file_tree]
copy_rel_path = "y"
copy_abs_path = "ctrl+y"
```

`[theme] mode` で UI 全体の配色（ファイルツリー / タブ / ボーダー / プレビュー）と
syntect のシンタックスハイライトテーマが連動して切り替わります:

| mode | UI 配色 | syntect テーマ |
|------|---------|---------------|
| `light` | GitHub Light 風（白背景・黒文字） | `InspiredGitHub` |
| `dark`  | GitHub Dark 風（黒背景・白文字） | `base16-eighties.dark` |

カラースキーム本体は `src/theme.rs` の `Theme::light()` / `Theme::dark()` で
定義されており、ここを書き換えれば独自の配色も作れます。

## アーキテクチャ

```
src/
├── main.rs       # エントリポイント、イベントループ、パニックフック
├── app.rs        # ワークスペース/タブ状態、レイアウトツリー、キー/マウスハンドリング
├── pane.rs       # PTY 管理、vt::Terminal 保持、スクロールオフセット
├── ui.rs         # ratatui 描画、テーマ、レイアウト
├── filetree.rs   # ファイルツリーのスキャン・ナビゲーション
├── preview.rs    # シンタックスハイライト付きファイルプレビュー
├── theme.rs      # ライト/ダークのカラースキーム定義
├── keymap.rs     # キーバインド管理（Action enum、KeyChord パース、スコープ別ルックアップ）
└── vt/           # 自前 VT エミュレータ
```

**設計の要点:**
- バイナリツリーレイアウトで可変比率の再帰ペイン分割
- ペインごとの PTY リーダースレッド + mpsc チャンネルでメインイベントループに送信
- OSC 7 検出による自動 cd トラッキング
- ダーティフラグによる描画最適化（アイドル時の CPU 使用を最小化）
- ステータスバーのショートカット表示は `KeyMap::find_chord` で実行時に解決するため、`config.toml` で再束縛すると表示も自動追従する

## 技術スタック

- [ratatui](https://ratatui.rs/) + [crossterm](https://github.com/crossterm-rs/crossterm) — TUI フレームワーク
- [portable-pty](https://github.com/nickelc/portable-pty) — PTY 抽象化（Windows では ConPTY）
- [vte](https://crates.io/crates/vte) — VT シーケンスパーサ
- [syntect](https://github.com/trishume/syntect) — シンタックスハイライト

## ライセンス

MIT
