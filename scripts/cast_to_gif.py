#!/usr/bin/env python3
"""Render an asciicast v2 file to an animated GIF with pyte and Pillow.

A fallback for `scripts/render-demos-lite.sh` on machines without asciinema's
`agg` (which has to be built from source; crates.io's `agg` is another
crate). It replays the cast through pyte's VT emulator, draws each changed
screen with a monospace font, caps idle gaps like `agg --idle-time-limit`,
and writes a looping GIF.

Usage: cast_to_gif.py in.cast out.gif [--font PATH] [--font-size N]
       [--idle-time-limit SECONDS] [--fps N]
"""
import argparse
import json
import os

import pyte
from PIL import Image, ImageDraw, ImageFont

# xterm's 16 colors, then the 6x6x6 cube and the gray ramp.
BASE16 = [
    (0x1E, 0x1E, 0x1E), (0xCD, 0x31, 0x31), (0x0D, 0xBC, 0x79), (0xE5, 0xE5, 0x10),
    (0x24, 0x72, 0xC8), (0xBC, 0x3F, 0xBC), (0x11, 0xA8, 0xCD), (0xE5, 0xE5, 0xE5),
    (0x66, 0x66, 0x66), (0xF1, 0x4C, 0x4C), (0x23, 0xD1, 0x8B), (0xF5, 0xF5, 0x43),
    (0x3B, 0x8E, 0xEA), (0xD6, 0x70, 0xD6), (0x29, 0xB8, 0xDB), (0xFF, 0xFF, 0xFF),
]
NAMES = ["black", "red", "green", "brown", "blue", "magenta", "cyan", "white"]
DEFAULT_FG = (0xD4, 0xD4, 0xD4)
DEFAULT_BG = (0x1E, 0x1E, 0x1E)


def color(value, default, bright=False):
    if value == "default":
        return default
    if value in NAMES:
        return BASE16[NAMES.index(value) + (8 if bright else 0)]
    if value.startswith("bright") and value[6:] in NAMES:
        return BASE16[NAMES.index(value[6:]) + 8]
    if len(value) == 6:
        try:
            return tuple(int(value[i:i + 2], 16) for i in (0, 2, 4))
        except ValueError:
            pass
    return default


def default_font():
    for path in (
        "/System/Library/Fonts/Menlo.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    ):
        if os.path.exists(path):
            return path
    raise SystemExit("no monospace font found: pass --font")


def render(screen, font, bold_font, cw, ch, pad):
    img = Image.new("RGB", (screen.columns * cw + 2 * pad, screen.lines * ch + 2 * pad), DEFAULT_BG)
    draw = ImageDraw.Draw(img)
    for y in range(screen.lines):
        row = screen.buffer[y]
        for x in range(screen.columns):
            c = row[x]
            fg = color(c.fg, DEFAULT_FG, c.bold)
            bg = color(c.bg, DEFAULT_BG)
            if c.reverse:
                fg, bg = bg, fg
            px, py = pad + x * cw, pad + y * ch
            if bg != DEFAULT_BG:
                draw.rectangle([px, py, px + cw - 1, py + ch - 1], fill=bg)
            if c.data.strip():
                draw.text((px, py), c.data, font=bold_font if c.bold else font, fill=fg)
            if c.underscore:
                draw.line([px, py + ch - 2, px + cw - 1, py + ch - 2], fill=fg)
    return img


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cast")
    ap.add_argument("gif")
    ap.add_argument("--font", default=None)
    ap.add_argument("--font-size", type=int, default=16)
    ap.add_argument("--idle-time-limit", type=float, default=2.0)
    ap.add_argument("--fps", type=float, default=10.0)
    args = ap.parse_args()

    with open(args.cast) as f:
        header = json.loads(f.readline())
        events = [json.loads(line) for line in f if line.strip()]
    cols, rows = header["width"], header["height"]
    screen = pyte.Screen(cols, rows)
    stream = pyte.Stream(screen)

    font_path = args.font or default_font()
    font = ImageFont.truetype(font_path, args.font_size)
    try:
        bold_font = ImageFont.truetype(font_path, args.font_size, index=1)
    except OSError:
        bold_font = font
    cw = round(font.getlength("M"))
    ch = round(args.font_size * 1.25)
    pad = 10

    frames, durations = [], []
    step = 1.0 / args.fps
    clock = 0.0  # timeline with idle gaps capped
    last_t = 0.0
    shown_at = None  # clock time the pending frame was taken
    last_display = None
    for t, kind, data in events:
        if kind != "o":
            continue
        gap = min(t - last_t, args.idle_time_limit)
        last_t = t
        clock += max(gap, 0.0)
        if shown_at is not None and clock - shown_at >= step:
            display = "\n".join(screen.display)
            snapshot = (display, tuple((c.fg, c.bg, c.reverse) for r in range(rows) for c in screen.buffer[r].values()))
            if snapshot != last_display:
                if frames:
                    durations.append(max(int((shown_at - durations_start[0]) * 1000), 20))
                frames.append(render(screen, font, bold_font, cw, ch, pad))
                durations_start[0] = shown_at
                last_display = snapshot
            shown_at = clock
        if shown_at is None:
            shown_at = clock
        stream.feed(data)
    frames.append(render(screen, font, bold_font, cw, ch, pad))
    if len(frames) > 1:
        durations.append(max(int((clock - durations_start[0]) * 1000), 20))
    durations.append(3000)  # hold the last frame before looping
    durations = durations[: len(frames)]
    pal = [f.convert("P", palette=Image.ADAPTIVE, colors=128) for f in frames]
    pal[0].save(args.gif, save_all=True, append_images=pal[1:], duration=durations, loop=0, optimize=True, disposal=1)
    print(f"  {len(frames)} frames, {sum(durations) / 1000:.1f}s, {os.path.getsize(args.gif) // 1024} KiB")


durations_start = [0.0]

if __name__ == "__main__":
    main()
