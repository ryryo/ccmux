# フォーク管理

このリポジトリは [Shin-sibainu/ccmux](https://github.com/Shin-sibainu/ccmux) をフォークし、独自改造を加えたものです。

## 独自の改造内容

| 変更 | ファイル |
|------|---------|
| ライトテーマの追加 | `src/ui.rs` |
| JSON/YAML アイコンの色調整（ライトテーマ向け） | `src/ui.rs` |

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

> コンフリクトが発生しやすい箇所: `src/ui.rs`（ライトテーマ関連の変更と競合する可能性があります）

### 5. ビルドして確認

```bash
cargo build
```

### 6. 改造版をインストール

```bash
cargo install --path .
```

---

## 改造版を `ccmux` コマンドとして使う

### インストール

```bash
cargo install --path .
```

`~/.cargo/bin/ccmux` にリリースビルドがインストールされます。

### PATH の優先順位に注意

npm 経由で `ccmux-cli` も入っている場合、`~/.nvm/.../bin/ccmux` が先に見つかり npm 版が優先されることがあります。
`~/.cargo/bin` を PATH の先頭に置くことで改造版が優先されます。

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
cargo install --path .
```
