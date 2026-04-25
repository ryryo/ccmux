# WSL+ClaudeCode で全角文字入力時のカーソルがズレるバグ — 調査レポート

調査日: 2026-04-25
対象コミット: `b67fe9d`（本質修正）/ 中間コミット `6400f55`（その場しのぎ修正）
再現動画: [`ccmux_bug.mp4`](./ccmux_bug.mp4)

---

## 1. 症状

- **環境**: WSL2 + Windows Terminal + Claude Code 起動中
- **操作**: 日本語（全角）を入力する
- **症状**: OSカーソル（█）が入力済み文字の上に重なって表示され、文字列の途中の文字に被さる
- **限定条件**:
  - Claude Code 入力中のみ発生
  - bash/zsh など通常シェルでは正常
  - 半角ASCII入力時は正常

## 2. 根本原因

### 2.1 vt100 における全角文字の保持形式

vt100 クレートは全角文字（CJK、East Asian Wide）を **2セル**で保持する：

```
col N    : 文字本体（is_wide() == true）
col N+1  : 継続セル（is_wide_continuation() == true）。contents は空
```

例: `❯ hello  入力テスト` 入力後の状態

| col | 19 | 20 | 21 |
|---|---|---|---|
| 内容 | `'ト'` | （空） | （空） |
| 種別 | `is_wide` | `is_wide_continuation` | 通常セル |

vt100 のカーソル位置は `col=21`（'ト' の論理的な「次の入力位置」）。

### 2.2 旧コードの不具合

`src/ui.rs` には Claude Code 用の `-1` 補正があった：

```rust
let cursor_x = if pane.is_claude_running() {
    area.x + cursor.1.saturating_sub(1)   // -1 補正
} else {
    area.x + cursor.1
};
```

これにより `cursor_x = area.x + 20` となり、OSカーソルが `'ト'` の **継続セル（右半分）** の上に置かれる → 視覚的に「グリフの上にカーソルが乗る」症状が発生。

ASCII の場合は `cursor_x = area.x + 6` で `'o'` の上に乗るが、半角セル幅と一致するため違和感なし → 見逃されていた。

## 3. なぜこのコードが入っていたか（git history）

| 項目 | 値 |
|---|---|
| 導入コミット | `63b67f99` "Add Claude Code JSONL monitoring and npm update notice" |
| 作者 | Shin-sibainu（upstream本家メンテナ） |
| 共著 | `Co-Authored-By: Claude Opus 4.6 (1M context)` |
| 日付 | 2026-04-14 |

コミットメッセージ本体は「JSONL監視機能の追加」がメインで、カーソル補正は副次的に混ぜ込まれていた。コメントの意図：

> Claude draws its own block character at the cursor position, and the PTY cursor would otherwise appear one column after with a visible gap.

### 推測される経緯

1. **当時のClaude Code（旧Ink）**は「`█`を描画 + カーソルを1列進める」挙動だった可能性
2. ccmux 側で「`█[gap]OSカーソル`」のように見えており、メンテナはそのギャップを埋めたかった
3. ASCII入力で動作確認 → 違和感なしで通過
4. **CJKでの破綻に気づかなかった**

注目すべき点: **Claude（LLM）自身が書いたコードに、Claudeの旧描画挙動を前提とした補正が入っていた**。皮肉にも Ink の仕様変更（後述）でこの補正が裏目に出た。

## 4. 業界調査による知見

### 4.1 同種バグの普遍性

「Inkベースの自前カーソル描画 TUI」+「マルチプレクサのカーソル列補正」の組み合わせは業界の鬼門で、近年複数の事例がある：

| プロジェクト | Issue/PR | 内容 |
|---|---|---|
| Claude Code | [#19207](https://github.com/anthropics/claude-code/issues/19207) | Ink TUI が物理カーソルを隠してANSIで描く問題、CJK IME位置ズレ |
| Zellij | PR #4951 | アプリがカーソルを隠してもCJK IME用に物理カーソルを出す |
| gemini-cli | [#13537](https://github.com/google-gemini/gemini-cli/pull/13537) | 全角クリック時のカーソル位置ズレを `string-width` で正規化 |
| ratatui | PR #1764 | 全角文字の wide-cell 進行幅バグ |

### 4.2 ベストプラクティス

1. **UAX #11 (East Asian Width)** に従い `unicode-width` で表示セル数を計算
2. vt100 の規約上「**カーソルがcontinuationセルに来ることは正常では起こり得ない**」
3. **物理カーソルは「アプリが望むスポット（論理位置）」に置く** — IME連携の前提
4. **`-1` 等のヒューリスティック補正は原則避ける**。「正規化」と「見栄え調整」を混ぜない
5. 業界標準（Zellij、wezterm、kitty）はこの方針に統一されている

### 4.3 Modern Ink（Claude Code 現行）の仕様

Issue #19207 で確認できる通り：
- **Ink ≥ 6 は物理カーソルを「アプリが望む位置」に置く**
- IME候補ウィンドウの位置追従のため、これが必須
- 旧バージョンに合わせた `-1` 補正は逆効果

## 5. 修正の経緯

### 5.1 ステップ1: その場しのぎ修正（コミット `6400f55`）

`is_wide_continuation` のセルにカーソルが乗ったら補正をスキップする条件を追加：

```rust
let cursor_x = if pane.is_claude_running() {
    let col = cursor.1;
    let prev_is_wide_cont = col > 0
        && screen.cell(cursor.0, col - 1)
            .map(|c| c.is_wide_continuation())
            .unwrap_or(false);
    if prev_is_wide_cont {
        area.x + col          // 補正なし
    } else {
        area.x + col.saturating_sub(1)   // 従来の -1 補正
    }
} else {
    area.x + cursor.1
};
```

→ 動くが、`-1` 補正自体の妥当性は未検証。Web調査で「業界的にこの補正自体が時代遅れ」と判明。

### 5.2 ステップ2: 本質修正（コミット `b67fe9d`）

`-1` 補正を完全撤去：

```rust
let cursor_x = area.x + cursor.1;
```

- Modern Ink TUI の前提に合致
- 業界標準（Zellij 等）と整合
- CJK問題が副次的に解消
- IME連携も改善される（はず）

### 5.3 最終差分（変更前→変更後）

- **変更行数**: 実質ロジックは6行 → 1行
- **複雑度**: `if/else` 分岐撤去で大幅減
- **依存性**: Claude固有挙動への依存をなくし、業界標準に揃えた

## 6. 副産物

### 6.1 デバッグログ基盤（`src/debug_log.rs`）

`CCMUX_DEBUG_LOG` 環境変数ゲートのファイルログ。

- 環境変数未設定時は完全に no-op
- TUI が stdout/stderr を奪うので、ファイルログは将来の調査でも有用
- 今回の調査では vt100 の cursor 位置・wide判定・周辺セル内容を出力させて根本原因を特定

```bash
CCMUX_DEBUG_LOG=/tmp/ccmux.log ccmux
```

### 6.2 再現動画

`0_docs/ccmux_bug.mp4` に Before の症状を記録。upstream PR時に説得力ある証拠として活用可能。

## 7. 教訓

1. **外部アプリの描画挙動に追従するヒューリスティック補正は、相手のバージョンで壊れる**
   - Ink/Claude Code のような活発に開発されているライブラリ依存だと特に危険
2. **ターミナル系は業界標準（vt100規約 / UAX #11）に従うのが結局安定**
   - 自前で見栄え調整を入れる前に「他のマルチプレクサがどうしているか」を確認する
3. **LLMが書いたコードは「直近の挙動に最適化」しがちで、業界標準を踏み外すリスクがある**
   - 今回まさに該当（Claude Opus が書いた `-1` 補正が CJK で破綻）
   - レビュー時には「広い慣例」「将来の互換性」を意識する必要
4. **CJK等の「主要言語以外」の確認が抜けやすい**
   - 開発者が英語環境メインだとさらに見落とされる
   - WSL+Windows Terminal は日本人開発者の主要環境の一つでもある
5. **TUIアプリのデバッグは stdout/stderr が使えない → ファイルログ基盤を最初から入れておくと調査が早い**

## 8. upstream への貢献提案

このフォーク（`ryryo/ccmux`）で確認した本質修正は、upstream `Shin-sibainu/ccmux` でも同じ問題が発生しているはず。以下を含めた PR を出すと有益：

- 本レポート（または要点）
- 修正コミット（`b67fe9d`）
- 再現動画
- 業界調査リンク（特に Issue #19207、Zellij PR #4951）

## 9. 参考リンク

### 関連プロジェクトのIssue
- [Claude Code #19207 — IME cursor position support for CJK](https://github.com/anthropics/claude-code/issues/19207)
- [Claude Code #14208 — terminal cursor position drift on Windows](https://github.com/anthropics/claude-code/issues/14208)
- [gemini-cli #13537 — wide character cursor positioning fix](https://github.com/google-gemini/gemini-cli/pull/13537)
- [ratatui #1764 — multi-width character rendering](https://github.com/ratatui/ratatui/pull/1764)
- [Zellij #4425 — drain_by_width wide character loss](https://github.com/zellij-org/zellij/issues/4425)
- [Zellij #1034 — unicode double rendering / cursor placement](https://github.com/zellij-org/zellij/issues/1034)

### ドキュメント
- [vt100 crate — Cell::is_wide_continuation](https://docs.rs/vt100/latest/vt100/struct.Cell.html)
- [UAX #11: East Asian Width](http://www.unicode.org/reports/tr11/)
- [Mitchell Hashimoto — Grapheme Clusters and Terminal Emulators](https://mitchellh.com/writing/grapheme-clusters-in-terminals)
- [wezterm — treat_east_asian_ambiguous_width_as_wide](https://wezterm.org/config/lua/config/treat_east_asian_ambiguous_width_as_wide.html)

## 10. 後日談 (2026-04-25)

vt100 crate は本レポートで触れた CJK 幅処理だけでなく、scroll region (DECSTBM)
有効時に scrollback を保存しないという別の構造的制約があり、Claude Code で
ホイール遡行が一切効かない問題を引き起こしていた。これを根本解決するため
`vte` パーサ + 自前 Grid/Scrollback への big-bang 置換を実施
(`docs/PLAN/260425_vt100-to-vte-migration/`)。CJK 幅は `unicode-width` crate で
再実装し、本レポートの -1 補正は撤去 (commit `b67fe9d` を維持)。

移行中、Claude Code が起動時に出す `\e[>4;2m` (xterm modifyOtherKeys) を SGR と
誤解釈して画面全体に下線が乗るバグを別途検出 → 根本原因 (ECMA-48 §5.4 の
intermediate byte 識別漏れ) を [`pty-byte-trace-debugging.md`](./pty-byte-trace-debugging.md)
の手順で特定し commit `668b190` で fix。同種バグの回帰検出は
`tests/tui_trace_test.rs` で claude/vim/less の実バイト列をリプレイして行う。
