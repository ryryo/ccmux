# フォーク管理

このリポジトリは [Shin-sibainu/ccmux](https://github.com/Shin-sibainu/ccmux) をフォークし、独自改造を加えたものです。

## 独自の改造内容

| 変更 | ファイル |
|------|---------|
| VT エミュレータを vt100 crate → 自前 vte ベース実装に big-bang 置換 | `src/vt/`（新設） |
| ライト/ダーク切替可能なテーマ機構 (`Theme::light()` / `Theme::dark()`) | `src/theme.rs`（新設）, `src/ui.rs` |
| プレビューの syntect テーマがモードに連動 | `src/preview.rs` |
| `~/.config/ccmux/config.toml` に `[theme]` セクション追加 | `src/config.rs` |
| README を全面日本語化、フォーク差分セクション追加 | `README.md` |

> 現在進行中の WIP: 設定可能なキーマップ (`src/keymap.rs` ほか)。完了するまでは
> `cargo install --path .` 時にビルドが落ちるので、stash で退避してからインストールする。
> 詳細は [`update-workflow.md`](./update-workflow.md) のトラブルシューティング参照。

---

## 本家アップデートのマージ手順

### 1. upstream リモートを登録（初回のみ）

```bash
git remote add upstream https://github.com/Shin-sibainu/ccmux.git
```

### 2. 本家の最新を取得

```bash
git fetch upstream
```

### 3. 未取り込みのコミットを確認

```bash
# 本家にあってこちらにないコミット
git log --oneline upstream/master ^master

# こちらにあって本家にないコミット（独自改造）
git log --oneline master ^upstream/master
```

### 4. マージ

```bash
git merge upstream/master --no-edit
```

> コンフリクトが発生しやすい箇所:
> - `src/ui.rs`（テーマ機構：本家は const、こちらは `app.theme.*` 経由）
> - `src/preview.rs`（syntect テーマ名が動的）
> - `src/pane.rs`, `src/app.rs`（vt100 → 自前 `vt::Terminal` への置換が広範）
> - `src/config.rs`（`[theme]` セクション追加分）

### 5. ビルドして確認

```bash
cargo build
```

### 6. 改造版をインストール

```bash
cargo install --path . --force
```

詳細は [`update-workflow.md`](./update-workflow.md) を参照。

---

## 改造版を `ccmux` コマンドとして使う

### インストール

```bash
cargo install --path . --force
```

`~/.cargo/bin/ccmux` にリリースビルドがインストールされます。`--force` を
忘れるとバージョンが同じ場合スキップされるので注意。

### PATH の優先順位に注意

npm 経由で `ccmux-cli` も入っている場合、`~/.nvm/.../bin/ccmux` が見つかって
npm 版（フォーク元 v0.6 系）が優先されることがあります。原則として
**npm 版はアンインストール**しておくのが無難です:

```bash
npm uninstall -g ccmux-cli
```

`~/.cargo/bin` を PATH の先頭に置くことでも改造版を優先できます。

`~/.zshrc` に追記済み：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

確認方法：

```bash
which ccmux
# → /home/<user>/.cargo/bin/ccmux  と表示されればOK
```

### 更新のたびに再インストール

マージ後や独自変更後は毎回以下を実行する：

```bash
cargo install --path . --force
```

日常的な変更 → 再インストールの流れは [`update-workflow.md`](./update-workflow.md)
にまとめてあります。
