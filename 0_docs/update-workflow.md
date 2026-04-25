# ローカル改造版の更新ワークフロー

このドキュメントは、**自分のリポジトリ（このフォーク）に手を入れたあと、
手元の `ccmux` コマンドを最新ビルドに差し替える**手順をまとめたものです。

> 本家 (Shin-sibainu/ccmux) のアップデートを取り込む手順は
> [`fork-maintenance.md`](./fork-maintenance.md) を参照してください。

---

## TL;DR

```bash
cd ~/ccmux-light
cargo install --path . --force
```

これだけで `~/.cargo/bin/ccmux` がリリースビルドに置き換わります。新しいシェルで `ccmux` を起動すれば反映されます。

---

## 前提となる環境

| 項目 | 状態 |
|------|------|
| `~/.cargo/bin` を PATH 先頭に置く | `~/.zshrc:199` で設定済み |
| npm 版 `ccmux-cli` | **アンインストール済み**（`npm uninstall -g ccmux-cli`） |
| `~/.cargo/bin/ccmux` | このフォークの `cargo install` 産物 |

`which ccmux` が `/home/<user>/.cargo/bin/ccmux` を返せば OK です。

npm 版を再インストールしてしまった場合は再度:

```bash
npm uninstall -g ccmux-cli
```

---

## 標準的な更新フロー

### 1. 変更をビルドして検証

```bash
cargo build           # debug ビルドでエラー確認
cargo test            # 全テスト
```

UI 変更を伴う場合は `cargo run` で目視確認（**別シェルで起動すること**。
ccmux 内で `cargo run` すると nested 検出に弾かれます）。

### 2. コミット

```bash
git add <files>
git commit -m "..."
```

CLAUDE.md のルール上、**実装は evaluator agent のレビューを通してから
完了報告**。コミット規約は `dev:simple-add` スキルに従います（emoji + type）。

### 3. ローカルバイナリを更新

```bash
cargo install --path . --force
```

- `--force` を付けないと「同じバージョン」だと判定されてスキップされます
- `~/.cargo/bin/ccmux` がリリースビルドで上書きされます
- 既に起動中の ccmux インスタンスには反映されません（再起動が必要）

### 4. 動作確認

新しいシェル（**ccmux の外側**）で:

```bash
ls -la $(which ccmux)   # タイムスタンプが今のものか
ccmux                   # 起動して新機能を確認
```

---

## バージョン番号を上げる場合（リリース予定があるとき）

`Cargo.toml` と `npm/package.json` のバージョンを**同じ値**に揃えて上げる。
詳細は CLAUDE.md の「Release Process」セクション参照。

```bash
# 例: 0.7.0 → 0.7.1
sed -i 's/^version = "0.7.0"/version = "0.7.1"/' Cargo.toml
sed -i 's/"version": "0.7.0"/"version": "0.7.1"/' npm/package.json
cargo build  # Cargo.lock を更新
git add Cargo.toml Cargo.lock npm/package.json
git commit -m "🔧 chore: bump version to 0.7.1"
```

リリース（GitHub Release + npm publish）を行う場合は CI に任せること。
**手動で `npm publish` や `gh release create` をしない**（バージョン衝突の原因）。

```bash
git tag v0.7.1 && git push origin master v0.7.1
# → .github/workflows/release.yml が自動でビルド & 配布
```

---

## トラブルシューティング

### `cargo install` が「未完成の WIP」のせいでビルド失敗する

`cargo install --path .` は **作業ツリーの現在の状態**をビルドします。
別ブランチでやりかけのコードがあるとビルドが落ちます。

対処: コミット済み HEAD の状態だけをインストールしたい場合は、WIP を一時退避してからインストールします。

```bash
git stash push -u -m "wip-stash-for-install" -- <path1> <path2> ...
cargo install --path . --force
git stash pop
```

`-u` は untracked ファイルも含めるオプション。stash pop でコンフリクトが
出る場合（pop 中にも作業を進めていた場合など）は `git stash drop` で破棄してもよい。

### `which ccmux` が古いパスを返す

PATH のキャッシュが効いていることが多い。新しいシェルを開く、または:

```bash
hash -r       # zsh: コマンドハッシュをクリア
rehash        # 同上
```

### npm 版がしれっと復活している

`npm install -g ccmux-cli` を間違って打ってしまったか、別ツールが入れた可能性。

```bash
npm ls -g ccmux-cli
npm uninstall -g ccmux-cli
```

`~/.cargo/bin` が PATH 先頭にあれば npm 版があっても優先されますが、混乱の元なので消しておくのが無難です。

### 起動中の ccmux に反映されない

`cargo install` はバイナリを置き換えるだけで、すでに起動しているプロセスは
古いバイナリを実行し続けます。`Ctrl+Q` で全タブを閉じて再起動が必要。

---

## 参考: `cargo install --path .` の挙動

- リリースビルド（`--release`）が走ります（debug ビルドではない）
- 出力先: `~/.cargo/bin/ccmux`（`CARGO_HOME` で変わる）
- `--force` なしだと既に同じ pkg の同じバージョンが入っていればスキップ
- `Cargo.lock` を尊重するので、依存関係はリポジトリ内と同じ
