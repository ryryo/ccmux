# ccmux-light 実機手動チェックリスト

vte 移行 (Gate A–H) 完了前にリリースの可否を判断するためのチェックリスト。
自動スナップショット (`cargo test --test vt_snapshot_test`) を補完する位置付け。

## 起動確認

- [ ] `cargo build --release` がエラー / 警告ゼロで通る
- [ ] `cargo clippy --all-targets -- -D warnings` がクリーン
- [ ] `cargo test` が全件パス
- [ ] `./target/release/ccmux` で起動し、シェルプロンプトが表示される
- [ ] `CCMUX=1` 環境下で起動を試みると `nested instance not allowed` エラーで終了する

## ターミナルエミュレーション中核

- [ ] **Claude Code 起動 → ログ流し → ホイールで scrollback 遡行できる** ★主目的
  - Claude Code を起動して `ls -la ~ | head -200` 等で長文を流す
  - Ctrl+ホイール上 / Ctrl+Up でスクロールバック方向に遡れる
  - ホイール下 / Ctrl+Down で末尾に戻れる
- [ ] vim 起動 → 退出 → vim 中の表示は scrollback に入らない (alt screen)
- [ ] less でファイルを開く → 退出 → 同上
- [ ] htop / top 起動 → CPU バー / プロセスツリーが描画される
- [ ] `printf '\033[1;20r'` 等で scroll region を設定したあとログを流す → scrollback に履歴が残る (Gate G2 の実機版)

## CJK / Unicode

- [ ] 日本語 ls 出力 (`ls 日本語/`) → カーソル位置が文字境界に整合
- [ ] CJK の auto-wrap 境界が文字を割らない (Gate G4 の実機版)
- [ ] 絵文字を含む出力 (`echo "🎉🚀✅"`) → カーソル進行が破綻しない
- [ ] ambiguous-width 文字 (`echo "→ ← ↑"`) — narrow として扱われ、レイアウトが崩れない

## マウス / 選択

- [ ] マウスドラッグで visible テキストを選択 → 反転表示
- [ ] Ctrl+C / マウスアップで OS クリップボードにコピーされる (`xclip -o` 等で確認)
- [ ] **scrollback まで広がるドラッグ選択** が反転表示される
- [ ] ダブルクリックで単語選択 (空白区切り)
- [ ] トリプルクリックで行選択
- [ ] 同位置で素早く 2 回クリック → 単語選択
- [ ] Esc で選択クリア → 再 Esc で scrollback 解除 → 再 Esc で PTY へ伝搬

## OSC

- [ ] `printf '\033]0;mytitle\007'` → タブのタイトルが `mytitle` に変わる
- [ ] `cd /tmp` 後ファイルツリーが /tmp に切り替わる (OSC 7)
- [ ] OSC 8 ハイパーリンクが青下線で表示される
  - 例: `printf '\e]8;;https://example.com\e\\click here\e]8;;\e\\\n'`
- [ ] 上記リンクを Ctrl+左クリック → 既定ブラウザが開く
- [ ] OSC 52 でクリップボード書き込み: `printf '\e]52;c;%s\e\\' "$(printf 'hello' | base64)"` → `xclip -o` で `hello` が取れる
- [ ] config に `[osc52]\nallow_read = false` (デフォルト) のとき OSC 52 read (`\e]52;c;?\e\\`) が無視される
- [ ] config に `allow_read = true` を入れて再起動すると OSC 52 read 応答が PTY に返る

## レイアウト / pane

- [ ] Ctrl+D で水平分割、Ctrl+E で垂直分割
- [ ] 分割中もそれぞれの scrollback が独立して保持される
- [ ] pane 間のフォーカス切替 (Alt+矢印 / Ctrl+矢印) が機能する
- [ ] pane 境界をドラッグでリサイズできる
- [ ] Alt+T で新タブ、タブダブルクリックでリネーム
- [ ] ターミナルウィンドウリサイズ → cols 変更後も scrollback の文字列が保持されている (reflow 視覚確認)
- [ ] サイドバー (ファイルツリー) と preview の表示切替・幅ドラッグ

## 設定

- [ ] `~/.config/ccmux/config.toml` 不在で起動 → デフォルト動作 (scrollback 10000 行)
- [ ] `[scrollback]\nmax_lines = 50000` を書いて再起動 → 大量出力で 50000 行ぶんまで遡れる
- [ ] `[scrollback]\nmax_lines = 100` で再起動 → 100 行を超えると古い行が落ちる
- [ ] 不正な TOML → デフォルト動作 (起動は失敗しない)

## 統合シナリオ

- [ ] tmux on ccmux で動作 (mouse passthrough、scroll、color)
- [ ] ssh セッション内で動作
- [ ] `claude` (Claude Code CLI) を起動して長セッションを通す
  - DECSTBM 入力中 / リスト表示中 / ストリーミング応答中の描画が破綻しない
  - 過去の応答にホイールで遡れる
- [ ] ペースト (Ctrl+Shift+V / 右クリック) で複数行が崩れずに渡る
  - bracketed paste 対応シェル → 1 回のペーストとして渡る
  - 非対応シェル → エスケープ列がリテラル表示されない (issue #2 の回帰チェック)

## 既知のリスクポイント

- [ ] WSL での Windows pwsh 経由起動 (Windows 限定)
- [ ] iTerm2 / kitty / alacritty の画像プロトコル (preview パネル)
- [ ] 100MB 級の出力を流す耐久試験 (メモリ使用量・パフォーマンス)

## 結果記録テンプレート

```
日付: YYYY-MM-DD
ビルド: <commit hash>
端末: <iTerm2 / WezTerm / Alacritty / etc.>
OS: <Linux / macOS / Windows WSL>
NG 項目:
  - [ ] xxx — 原因 / 対応 Gate

OK 判定: [ ] 全 PASS で master タグ vX.Y.Z に進む
```
