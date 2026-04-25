# PTY バイト列トレースで TUI 描画バグを根本診断する手順

ccmux のような **terminal multiplexer** の描画バグは、上流アプリ
(Claude Code / vim / tmux 等) が出すエスケープシーケンスを正しく
解釈できていないことが原因の大半です。スクリーンショットを眺めて
推測で symptom を潰すと、必ず同種のバグが別形で再発します。

このドキュメントは「**実バイト列を採取して原因を特定する**」標準
手順をまとめたものです。`commit 668b190` で fix した「Claude Code を
開くと画面全体に下線」バグはこの手順で 30 分弱で根本原因 (`\e[>4;2m`
を SGR と誤解釈) まで辿れました。

## いつこの手順を使うか

- **「特定のアプリでだけ」起きる描画異常**
- **「旧版 (vt100 era) では起きなかった」回帰**
- スクショから推測しても複数の仮説が並列に立つとき
- すでに 1〜2 回「対症療法的な修正」を試して直っていないとき

逆に、再現が容易な小さな symptom (例: `printf` 1 行で再現) なら
このフローはオーバーキル。直接 `cargo test` で再現させて debugger を
回す方が速い。

## ステップ 1 — PTY 経由で実バイト列を採取する

`script(1)` は PTY を提供してくれるが対話入力が要る。**非対話の
スクリプトで採取するなら Python の `pty.fork()` が最も小回りが効く**。

```python
# /tmp/capture.py
import os, pty, select, sys, time, fcntl, termios, struct

OUT = '/tmp/captured.bin'
CMD = ['claude']                    # 採取したい CLI

pid, fd = pty.fork()
if pid == 0:
    os.environ['TERM'] = 'xterm-256color'
    os.execvp(CMD[0], CMD)

# Master 側: window size を ccmux と同じ条件で固定すると再現性が上がる
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))

out = open(OUT, 'wb')

def drain(timeout):
    deadline = time.time() + timeout
    while True:
        remaining = deadline - time.time()
        if remaining <= 0: break
        r, _, _ = select.select([fd], [], [], remaining)
        if not r: break
        try:
            chunk = os.read(fd, 4096)
        except OSError:
            break
        if not chunk: break
        out.write(chunk); out.flush()

# (1) アプリの初期描画が落ち着くのを待つ
drain(6)
# (2) バグを誘発するプロンプトを送る (\r は CR=Enter)
os.write(fd, b'list 3 things about this project\r')
drain(35)
# (3) 終了させる
os.write(fd, b'\x1b\x1b')   # Esc Esc (claude を抜ける)
drain(1)
os.write(fd, b'\x04')        # Ctrl-D
drain(2)
out.close()
print(f"saved → {OUT} ({os.path.getsize(OUT)} bytes)")
```

実行:

```bash
python3 /tmp/capture.py
```

これで `/tmp/captured.bin` に **ccmux が同じ状況で受け取るのと等価な
バイト列**が落ちる。

> **Tip**: 再現条件 (rows/cols, TERM, env vars, cwd) は ccmux と
> 揃えると診断がブレない。ウィンドウサイズは Claude Code の TUI
> レイアウトに影響する典型的な変数。

## ステップ 2 — エスケープシーケンスをカテゴライズして集計する

raw バイト列を直接眺めても見落とす。`re` で CSI/OSC を抽出して
出現回数で並べると、**異常なシーケンスが頻度の偏りで浮き上がる**。

```python
# /tmp/analyze.py
import re
from collections import Counter
data = open('/tmp/captured.bin','rb').read()

# CSI: ESC [ <params> <final 0x40-0x7E>
csi_re = re.compile(rb'\x1b\[([\d;:?>= ]*)([\x40-\x7E])')

sgrs = Counter()
finals = Counter()
weird = Counter()

for m in csi_re.finditer(data):
    params = m.group(1).decode('latin-1')
    final = chr(m.group(2)[0])
    finals[final] += 1
    if final == 'm':
        sgrs[params] += 1
    # `>` `!` `'` `$` ` ` などの "怪しい" intermediate を別カウント
    if any(ch in params for ch in '>!\'$ '):
        weird[(params, final)] += 1

print("=== SGR by frequency ===")
for params, n in sgrs.most_common(30):
    print(f"  {n:4d} × \\e[{params}m")

print("\n=== CSI finals ===")
for f, n in finals.most_common():
    print(f"  {n:4d} × CSI ...{f}")

print("\n=== CSI with private intermediates (`> ! ' $ ` `) ===")
for (params, final), n in weird.most_common():
    print(f"  {n:4d} × \\e[{params}{final}")
```

```bash
python3 /tmp/analyze.py
```

出力例 (実際に bug を見つけた時の):

```
=== SGR by frequency ===
    96 × \e[39m
    67 × \e[38;5;241m
    ...
     4 × \e[>4;2m       ← ★これ
     4 × \e[3m
```

`\e[>4;2m` は xterm modifyOtherKeys (intermediate=`>`) で SGR では
ない。ccmux の csi_dispatch は `>` を区別せず標準 SGR に流して
`4 → UNDERLINE`, `2 → DIM` と誤解釈、しかも Claude Code は
`\e[24m` を一切出さないので bit が永続 → 全画面下線、というチェーン
が確定した。

> **読み方のコツ**: 出現頻度の low 〜 mid 帯にいる「見慣れない
> シーケンス」が大抵の犯人。`\e[39m` のような典型的なリセット系は
> 多くて当たり前なので無視。intermediate が混ざっているもの、
> `:` でサブパラメタが付いているもの、桁が大きい数値 (58, 59, >100)
> のものを優先して読む。

## ステップ 3 — in-process トレースで仮説を裏取り

バイト列の集計で当たりが付いたら、**実装側に opt-in トレースを
仕込んで cursor 状態の変遷を観察**する。ccmux には既に `CCMUX_TRACE_CSI`
が組み込まれている (commit `b608f74`):

```bash
rm -f /tmp/csi.log
CCMUX_TRACE_CSI=/tmp/csi.log ccmux-next
# (バグ再現)
```

`/tmp/csi.log` に `CSI m` ごとの params + 処理前後の `cursor.style.bits` /
`fg` / `bg` が出る。`underline_before=false` から `underline_after=true`
へ遷移している瞬間がそのまま犯人のシーケンス。

> **追加トレースを足したいとき** は `src/vt/parser.rs::csi_dispatch` の
> 既存ブロックの隣にもう 1 個 env var を生やすのが最小コスト。CSI 以外
> (OSC, DCS, ESC) も同じパターンで `osc_dispatch` / `esc_dispatch` に
> 仕込めば良い。

## ステップ 4 — 回帰テストで固定する

修正の前に、原因シーケンスをそのまま単体テストにする:

```rust
#[test]
fn xterm_modify_other_keys_does_not_set_sgr_attrs() {
    let mut t = Terminal::new(2, 10, 100);
    t.process(b"\x1b[>4;2m");
    assert!(!t.grid.cursor.style.contains(CellAttrs::UNDERLINE));
    assert!(!t.grid.cursor.style.contains(CellAttrs::DIM));
}
```

これがあると:
- 修正コミットの diff だけで「何が起きていたか」が読める
- 同種バグ (別 intermediate, 別 private CSI) を将来踏んだ時も同じ場所
  に追加すれば良い、という指針になる
- リファクタで誤って分岐を消した時に CI で気付ける

## やってはいけないこと

- **「装飾を全部オフにする」「機能を削る」で症状を消す**:
  原因の特定を放棄しているだけ。今回 `OSC 8 視覚装飾を既定 off` という
  対症療法を 2 回 commit してしまったが、両方 revert する羽目になった。
  ユーザーが見えなくなっただけで、内部 state は依然として壊れている。

- **スクショだけ眺めて仮説を立て続ける**: スクショは入力でも内部状態
  でもなく **render 後の見た目** で、情報量が少なすぎる。最大 2 仮説で
  当たりが出ない時点で必ずバイト列採取に切り替える。

- **「old との振る舞いを揃えるために古い実装を真似する」**: 旧実装が
  たまたま無視していた "正しいエスケープ" を新実装も無視するように
  すると、別のアプリでバグる。「なぜ旧版で問題が出ないか」を理解
  してから改修する。今回の場合、vt100 crate は intermediate を ECMA-48
  どおりに区別していたのが正、ccmux 側の `?` 専用分岐が漏れだった。

## 関連コミット / 参考

- `commit 668b190` — `\e[>4;2m` の root-cause 修正
- `commit b608f74` — `CCMUX_TRACE_CSI` opt-in トレース追加
- `src/vt/parser.rs::csi_dispatch` — intermediate byte の分岐
- ECMA-48 §5.4 — Control sequences with intermediate bytes
- xterm Control Sequences — modifyOtherKeys (`CSI > Pp ; Pv m`)
