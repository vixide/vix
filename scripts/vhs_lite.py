#!/usr/bin/env python3
"""A browser-free player for a useful subset of VHS's `.tape` DSL
(improvement plan T406, tasks.md).

`scripts/render-demos.sh` (real VHS <https://github.com/charmbracelet/vhs>)
is still the documented, primary way to regenerate `docs/demos/*.gif`: it
supports the full tape language and is what most contributors' machines
can run directly. This script exists for the opposite case -- an
environment where VHS's screenshot-based capture pipeline cannot run at
all, because headless Chrome/Chromium (which `ttyd`+VHS drive to
screenshot a real, DOM-rendered terminal) gets killed by the host outright
and has no fallback for that. `scripts/render-demos-lite.sh` drives this
script instead, with no headless browser anywhere in the pipeline:

    pexpect (a real pty)  ->  hand-written asciicast v2 recorder
                          ->  agg <https://github.com/asciinema/agg>
                              (pure-Rust: renders each asciicast frame by
                              directly shaping text with a font library --
                              no browser, no screenshot).

Correctness note on Hide/Show: a terminal-emulator-based player (agg's own
`avt`, and any VT100 emulator) reconstructs screen state purely by
replaying escape codes from the very start of the stream -- unlike VHS's
screenshot approach, it cannot "start recording mid-session", because
cursor position/attributes/scroll region/etc. are all incremental state
carried forward one escape code at a time. So this recorder always feeds
every byte the pty produces; `Hide`/`Show` only change the *timestamp*
attached to those bytes -- a `Hide`d span's bytes get a near-zero time
delta (so they replay instantly, invisible to a human watching the GIF)
while a `Show`n span keeps real elapsed time. This is what actually
matches VHS's own Hide/Show intent (skip the boring setup) without ever
letting the replayed terminal state drift from what really happened.

Supports the directives the current docs/demos/*.tape files use: `Output`,
`Set` (parsed only for Width/Height/FontSize, to size the pty), `Hide`,
`Show`, `Sleep`, `Type "..."`, `Enter`, `Escape`, `Tab`, `Backspace [N]`,
`Down [N]`, `Up [N]`, `Left [N]`, `Right [N]`, and `Ctrl+<key>` /
`Ctrl+Shift+<arrow>` / `Ctrl+Enter` combos. Extend `key_bytes` if a new
tape needs another key.

Usage: vhs_lite.py some.tape out.cast
"""
import json
import re
import sys
import time

import pexpect

CTRL = {chr(i + 96): chr(i) for i in range(1, 27)}  # 'a' -> \x01, etc.


def parse_duration(tok: str) -> float:
    m = re.match(r"^([0-9.]+)(ms|s)$", tok)
    assert m, f"bad duration: {tok}"
    n = float(m.group(1))
    return n / 1000.0 if m.group(2) == "ms" else n


def key_bytes(name: str) -> bytes:
    parts = name.split("+")
    if len(parts) == 1:
        special = {
            "Enter": "\r",
            "Escape": "\x1b",
            "Backspace": "\x7f",
            "Tab": "\t",
            "Down": "\x1b[B",
            "Up": "\x1b[A",
            "Left": "\x1b[D",
            "Right": "\x1b[C",
            "F1": "\x1b[11~",
            "F10": "\x1b[21~",
        }
        if name in special:
            return special[name].encode()
        raise ValueError(f"unknown key: {name}")
    mods = set(parts[:-1])
    base = parts[-1]
    if "Ctrl" in mods and "Shift" not in mods and len(base) == 1 and base.lower() in CTRL:
        return CTRL[base.lower()].encode()
    if base == "Enter" and "Ctrl" in mods:
        # Vix requests the kitty keyboard protocol's disambiguated encoding
        # (src/main.rs, PushKeyboardEnhancementFlags) precisely so Ctrl+Enter
        # is distinguishable from plain Enter -- crossterm's parser
        # recognizes this CSI-u form on sight, independent of whether a real
        # terminal actually negotiated the enhancement.
        return b"\x1b[13;5u"
    if base == "Left" and mods == {"Ctrl", "Shift"}:
        return b"\x1b[1;6D"
    if base == "Right" and mods == {"Ctrl", "Shift"}:
        return b"\x1b[1;6C"
    raise ValueError(f"unknown combo: {name}")


class Recorder:
    """Always feeds every byte (so a replaying terminal emulator's state
    stays correct); Hide/Show only changes how much *timeline* a span of
    bytes consumes in the output cast -- see the module docstring."""

    HIDDEN_STEP = 0.001  # near-zero but strictly increasing, so ordering holds

    def __init__(self, cols: int, rows: int):
        self.cols = cols
        self.rows = rows
        self.events = []  # (t_abs, "o", text)
        self.t = 0.0
        self.visible = False
        self._last_wall = None

    def set_visible(self, visible: bool):
        self.visible = visible
        self._last_wall = time.monotonic()

    def feed(self, data: bytes):
        if not data:
            return
        now = time.monotonic()
        if self._last_wall is None:
            self._last_wall = now
        if self.visible:
            self.t += now - self._last_wall
        else:
            self.t += self.HIDDEN_STEP
        self._last_wall = now
        self.events.append((self.t, "o", data.decode("utf-8", "replace")))

    def write_cast(self, path: str):
        header = {
            "version": 2,
            "width": self.cols,
            "height": self.rows,
            "timestamp": int(time.time()),
            "env": {"SHELL": "/bin/bash", "TERM": "xterm-256color"},
        }
        with open(path, "w") as f:
            f.write(json.dumps(header) + "\n")
            for t, kind, text in self.events:
                f.write(json.dumps([round(t, 6), kind, text]) + "\n")


def cols_rows_from_tape(directives: list[tuple[str, str]]) -> tuple[int, int]:
    """Approximate VHS's pixel Width/Height/FontSize/Padding into terminal
    columns/rows, using a typical monospace cell aspect (~0.6 x 1.2 of the
    font size). Not exact -- VHS's own conversion isn't public API -- but
    close enough that every one of the current tapes (all sharing one
    Width/Height/FontSize/Padding block) renders at a normal, readable
    terminal size."""
    width, height, font_size, padding = 1200, 700, 16, 20
    for cmd, rest in directives:
        if cmd != "Set":
            continue
        parts = rest.split()
        if len(parts) != 2:
            continue
        key, val = parts
        if key not in ("Width", "Height", "FontSize", "Padding"):
            continue  # e.g. "Set Shell bash", "Set TypingSpeed 60ms"
        val = float(val)
        if key == "Width":
            width = val
        elif key == "Height":
            height = val
        elif key == "FontSize":
            font_size = val
        elif key == "Padding":
            padding = val
    cell_w = font_size * 0.6
    cell_h = font_size * 1.2
    cols = max(20, int((width - 2 * padding) / cell_w))
    rows = max(10, int((height - 2 * padding) / cell_h))
    return cols, rows


def parse_tape(tape_path: str) -> list[tuple[str, str]]:
    out = []
    for raw in open(tape_path):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(None, 1)
        out.append((parts[0], parts[1] if len(parts) > 1 else ""))
    return out


# This sandbox's shared-machine contention (documented elsewhere in this
# repo's own session history: build/CI slowdowns from the same cause) means
# a fresh vix process can take noticeably longer than normal to open an
# overlay (e.g. build_file_index() for the command palette) -- confirmed by
# hand: a fixed post-keypress sleep occasionally raced ahead of vix's own
# redraw, most visibly losing the very first character typed right after
# Ctrl+P (it landed before the palette's input handler was registered). The
# tapes' own authored Sleep durations are tuned for a normal machine;
# scaling them up (SLEEP_SCALE) helps but a fixed sleep can never be fully
# race-proof against a host of unknown, varying speed -- so every discrete
# keypress (not just explicit Sleeps) is additionally followed by an active
# "wait until vix's output goes quiet" step (settle()) rather than a blind
# guess: it keeps reading for as long as new bytes keep arriving, up to a
# bounded ceiling, and returns as soon as a short quiet period confirms vix
# has actually finished reacting to that key.
SLEEP_SCALE = 2.0
SETTLE_QUIET = 0.08  # no new output for this long => vix has settled
SETTLE_MAX = 1.5  # ceiling, so a truly idle key (e.g. plain typing) doesn't stall


def run_tape(tape_path: str, out_cast: str):
    # `Set TypingSpeed` (VHS's per-character delay) has no direct analogue
    # here: settle() (see below) paces every character by actually waiting
    # for vix to react, not by a fixed delay, so it's read for compatibility
    # (parse_tape/cols_rows_from_tape already skip it safely) but unused.
    directives = parse_tape(tape_path)
    cols, rows = cols_rows_from_tape(directives)

    rec = Recorder(cols, rows)
    child = pexpect.spawn(
        "bash", ["--noprofile", "--norc"], dimensions=(rows, cols), encoding=None, timeout=None
    )
    child.setwinsize(rows, cols)

    def reader_tick():
        try:
            data = child.read_nonblocking(size=65536, timeout=0.02)
            rec.feed(data)
            return True
        except pexpect.exceptions.TIMEOUT:
            return False
        except (pexpect.exceptions.EOF, OSError):
            return None

    def drain(seconds: float):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if reader_tick() is None:
                break

    def settle() -> bool:
        """Block until vix's output has gone quiet for SETTLE_QUIET seconds
        (i.e. it has actually finished reacting to the last key), or
        SETTLE_MAX total elapses -- see the module-level comment above.
        Returns whether *any* output was observed at all, so a caller can
        tell "vix reacted and is now idle" apart from "nothing happened"."""
        start = time.monotonic()
        last_activity = start
        saw_any = False
        while True:
            now = time.monotonic()
            if now - start >= SETTLE_MAX:
                return saw_any
            if now - last_activity >= SETTLE_QUIET:
                return saw_any
            if reader_tick():
                last_activity = time.monotonic()
                saw_any = True

    # Two confirmed, empirical findings on this shared, contended sandbox
    # (matching the same shared-machine slowdowns documented elsewhere in
    # this repo's own session history), neither of which is a vix bug --
    # verified directly against App::on_key with no pty involved at all,
    # which applies every key correctly, every time:
    #  1. Sending a keypress and the very next one as two *separate* pty
    #     writes, with any gap between them, occasionally loses the second
    #     one somewhere between this pty and vix's own input parsing --
    #     even with a very long wait in between. So every run of "keys
    #     with nothing but Sleep-free actions between them" (the shape
    #     every current tape actually uses -- e.g. Ctrl+P immediately
    #     followed by `Type ">..."`) is buffered and sent as one write().
    #  2. Even a single combined write can occasionally produce *no*
    #     reaction at all (this host's scheduler starving the vix process
    #     for a stretch, not a lost byte) -- so flush() retries the exact
    #     same bytes, with backoff, whenever settle() reports total
    #     silence.
    #
    # An earlier version of this also retried whenever a batch's typed text
    # didn't turn up in the live decoded screen afterward, on the theory
    # that "some reaction, but not the right one" (e.g. keys landing in an
    # already-open overlay) deserved a retry too -- reverted: it produced
    # *false* negatives (typed text that scrolled out of the visible
    # viewport, still correctly applied, just not visible in the check) and
    # blindly resending on a false negative isn't safe in general -- a
    # `Down 3 / Enter / Type "..."` batch, unlike a fresh Ctrl+P + Type, is
    # not idempotent: a spurious retry replays the cursor move *and* the
    # insert, duplicating the edit (confirmed by hand: an unwanted repeated,
    # increasingly-indented comment in a demo-workspace file). Any actual
    # "right reaction, wrong place" case (e.g. a missing Command Palette
    # entry) needs fixing at the source, not papering over with a retry.
    FLUSH_RETRIES = 5

    pending = bytearray()

    def queue(b: bytes):
        pending.extend(b)

    def flush():
        if not pending:
            return
        data = bytes(pending)
        pending.clear()
        for attempt in range(FLUSH_RETRIES):
            child.send(data)
            if settle():
                return
            drain(0.3 * (attempt + 1))  # backoff before retrying

    for cmd, rest in directives:
        if cmd in ("Output", "Set"):
            continue
        if cmd == "Hide":
            flush()
            rec.set_visible(False)
            continue
        if cmd == "Show":
            flush()
            rec.set_visible(True)
            continue
        if cmd == "Sleep":
            flush()
            drain(parse_duration(rest) * SLEEP_SCALE)
            continue
        if cmd == "Type":
            text = rest.strip()
            if text.startswith('"') and text.endswith('"'):
                text = text[1:-1]
            queue(text.encode())
            continue
        if cmd in ("Down", "Up", "Left", "Right", "Backspace"):
            # Empirically, this is *not* like the Ctrl+P+Type case above:
            # batching N repeats of the *same* byte sequence into one write
            # can also lose some of them (confirmed by hand: 9 batched
            # Right presses moved the menu selection nowhere at all, while
            # the same 9 presses sent and settled one at a time each moved
            # it correctly) -- some coalescing distinct from the
            # different-bytes-in-one-write case. So flush whatever's
            # pending first, then send and settle each repeat individually.
            flush()
            n = int(rest.strip()) if rest.strip() else 1
            kb = key_bytes(cmd)
            for _ in range(n):
                queue(kb)
                flush()
            continue
        queue(key_bytes(cmd))
        if cmd == "Escape":
            # A lone ESC is inherently ambiguous to any VT parser mid-stream
            # (is more of a CSI sequence coming, or was that the whole
            # keypress?) -- confirmed by hand: batching Escape together with
            # a *following* Escape-prefixed key (F1's `\x1b[11~` included)
            # into one write, with nothing but a blank line between their
            # tape directives, garbled into literal text instead of either
            # key registering. So Escape always ends its batch here, never
            # merging with whatever comes next.
            flush()

    flush()
    drain(0.3)
    try:
        child.close(force=True)
    except Exception:
        pass
    rec.write_cast(out_cast)
    print(f"  {len(rec.events)} events, {rec.t:.1f}s visible timeline, {cols}x{rows}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("usage: vhs_lite.py some.tape out.cast", file=sys.stderr)
        sys.exit(2)
    run_tape(sys.argv[1], sys.argv[2])
