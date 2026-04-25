#!/usr/bin/env python3
"""TUI バイト列採取スクリプト (Gate H4 / 0_docs/pty-byte-trace-debugging.md 参照)

各 TUI を擬似 TTY で起動し、決まった操作を送って終了させ、
受け取った全バイトを tests/snapshots/tui_traces/{name}.bin に保存する。

ローカル開発でのみ実行する手作業ツールで、CI からは呼ばない
(ホストに対象 CLI が無いと採取できないため)。CI は採取済みの .bin を
リプレイするだけ。

使い方:
    python3 tests/fixtures/capture_tui.py             # 全 TUI を順に採取
    python3 tests/fixtures/capture_tui.py claude vim  # 名指しで採取
"""
import os
import pty
import select
import sys
import time
import fcntl
import termios
import struct
import shutil
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
OUT_DIR = REPO / "tests" / "snapshots" / "tui_traces"
ROWS, COLS = 40, 120


def capture(name: str, cmd: list[str], script: list[tuple[str, bytes, float]]) -> bool:
    """`cmd` を PTY で起動 → script の (label, bytes, wait_sec) を順に流す → 全バイトを保存。

    返り値: 採取成功なら True。コマンドが見つからない場合は False。
    """
    if not shutil.which(cmd[0]):
        print(f"[skip] {name}: {cmd[0]} not found in PATH")
        return False

    out_path = OUT_DIR / f"{name}.bin"
    pid, fd = pty.fork()
    if pid == 0:
        env = os.environ.copy()
        env["TERM"] = "xterm-256color"
        # 多くの TUI は LANG/LC_ALL を見て幅判定するので明示
        env.setdefault("LC_ALL", "C.UTF-8")
        os.execvpe(cmd[0], cmd, env)

    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    out = open(out_path, "wb")

    def drain(timeout: float) -> None:
        deadline = time.time() + timeout
        while True:
            remaining = deadline - time.time()
            if remaining <= 0:
                break
            r, _, _ = select.select([fd], [], [], remaining)
            if not r:
                break
            try:
                chunk = os.read(fd, 4096)
            except OSError:
                break
            if not chunk:
                break
            out.write(chunk)
            out.flush()

    for label, payload, wait_sec in script:
        drain(wait_sec)
        if payload:
            os.write(fd, payload)

    # Final drain: keep reading until the child actually exits OR a hard
    # ceiling elapses. Without this we routinely miss the trailing
    # alt-screen-exit (\e[?1049l) that TUIs emit on shutdown.
    final_deadline = time.time() + 5.0
    while time.time() < final_deadline:
        try:
            wpid, _ = os.waitpid(pid, os.WNOHANG)
        except ChildProcessError:
            wpid = pid
        # Drain whatever is currently available regardless of child state.
        while True:
            r, _, _ = select.select([fd], [], [], 0.1)
            if not r:
                break
            try:
                chunk = os.read(fd, 4096)
            except OSError:
                chunk = b""
            if not chunk:
                break
            out.write(chunk)
            out.flush()
        if wpid != 0:
            # Child reaped. One more grace read and we're done.
            try:
                while True:
                    r, _, _ = select.select([fd], [], [], 0.2)
                    if not r:
                        break
                    chunk = os.read(fd, 4096)
                    if not chunk:
                        break
                    out.write(chunk)
                    out.flush()
            except OSError:
                pass
            break

    out.close()
    size = out_path.stat().st_size
    print(f"[ok] {name}: saved → {out_path.relative_to(REPO)} ({size} bytes)")
    return True


# ─── 各 TUI の採取スクリプト ────────────────────────────────

def capture_claude() -> bool:
    return capture(
        "claude",
        ["claude"],
        [
            ("settle", b"", 6.0),
            ("prompt", b"list 3 things about this project\r", 35.0),
            ("exit_esc", b"\x1b\x1b", 1.0),
            ("exit_eof", b"\x04", 2.0),
        ],
    )


def capture_vim() -> bool:
    # 一時ファイルを作って vim で開き :q! で抜ける
    tmp = tempfile.NamedTemporaryFile(
        mode="w", suffix=".txt", delete=False, prefix="ccmux_vim_"
    )
    tmp.write("hello\nworld\n日本語\n")
    tmp.close()
    ok = capture(
        "vim",
        ["vim", "-N", "-u", "NONE", "-i", "NONE", tmp.name],
        [
            ("settle", b"", 2.0),
            ("scroll", b"jjkk", 0.5),
            ("quit", b":q!\r", 1.5),
        ],
    )
    os.unlink(tmp.name)
    return ok


def capture_less() -> bool:
    # 数十行ある一時ファイルを less で開いて q で抜ける
    tmp = tempfile.NamedTemporaryFile(
        mode="w", suffix=".txt", delete=False, prefix="ccmux_less_"
    )
    for i in range(80):
        tmp.write(f"line {i:03d}: hello world 日本語混じり\n")
    tmp.close()
    ok = capture(
        "less",
        ["less", "-R", tmp.name],
        [
            ("settle", b"", 1.5),
            ("page_down", b" ", 0.5),
            ("page_down", b" ", 0.5),
            ("quit", b"q", 1.0),
        ],
    )
    os.unlink(tmp.name)
    return ok


def capture_htop() -> bool:
    return capture(
        "htop",
        ["htop", "-d", "5"],  # 0.5s refresh
        [
            ("settle", b"", 2.5),
            ("quit", b"q", 1.5),
        ],
    )


REGISTRY = {
    "claude": capture_claude,
    "vim": capture_vim,
    "less": capture_less,
    "htop": capture_htop,
}


def main() -> int:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    targets = sys.argv[1:] or list(REGISTRY.keys())
    captured = 0
    skipped = 0
    for name in targets:
        fn = REGISTRY.get(name)
        if fn is None:
            print(f"[err] unknown target: {name}", file=sys.stderr)
            continue
        if fn():
            captured += 1
        else:
            skipped += 1
    print(f"\nsummary: captured={captured} skipped={skipped}")
    return 0 if captured else 1


if __name__ == "__main__":
    sys.exit(main())
