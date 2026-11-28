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
    # A Nerd Font first, so Vix's file and status icons draw.
    home = os.path.expanduser("~")
    for path in (
        "/Library/Fonts/JetBrainsMonoNerdFontMono-Regular.ttf",
        f"{home}/Library/Fonts/JetBrainsMonoNerdFontMono-Regular.ttf",
        "/Library/Fonts/HackNerdFontMono-Regular.ttf",
        f"{home}/.local/share/fonts/JetBrainsMonoNerdFontMono-Regular.ttf",
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
    ap.add_argument("--max-frame", type=float, default=3.0)
    args = ap.parse_args()

    with open(args.cast) as f:
        header = json.loads(f.readline())
        events = [json.loads(line) for line in f if line.strip()]
    cols, rows = header["width"], header["height"]
    screen = pyte.Screen(cols, rows)
    stream = pyte.Stream(screen)

    font_path = args.font or default_font()
    font = ImageFont.truetype(font_path, args.font_size)
    bold_path = font_path.replace("-Regular.", "-Bold.")
    try:
        if bold_path != font_path and os.path.exists(bold_path):
            bold_font = ImageFont.truetype(bold_path, args.font_size)
        else:
            bold_font = ImageFont.truetype(font_path, args.font_size, index=1)
    except OSError:
        bold_font = font
    cw = round(font.getlength("M"))
    ch = round(args.font_size * 1.25)
    pad = 10

    # Timeline with idle gaps capped; a frame is taken whenever the screen
    # has settled for one frame step (the next output is that far away) or
    # a frame step has passed since the last frame.
    out = [e for e in events if e[1] == "o"]
    times, clock, last_t = [], 0.0, 0.0
    for t, _, _ in out:
        clock += min(max(t - last_t, 0.0), args.idle_time_limit)
        last_t = t
        times.append(clock)
    step = 1.0 / args.fps
    frames, stamps = [], []
    last_key = None
    in_sync = False  # inside a synchronized update (DEC mode 2026): half drawn
    for i, (_, _, data) in enumerate(out):
        stream.feed(data)
        begin, end = data.rfind("\x1b[?2026h"), data.rfind("\x1b[?2026l")
        if begin != -1 or end != -1:
            in_sync = begin > end
        if in_sync:
            continue
        now = times[i]
        nxt = times[i + 1] if i + 1 < len(out) else now + step
        if nxt - now < step and stamps and now - stamps[-1] < step:
            continue
        key = tuple((c.data, c.fg, c.bg, c.reverse, c.bold) for r in range(rows) for c in (screen.buffer[r][x] for x in range(cols)))
        if key == last_key:
            continue
        last_key = key
        frames.append(render(screen, font, bold_font, cw, ch, pad))
        stamps.append(now)
    if not frames:
        raise SystemExit("the cast has no output")
    # A frame that only waits (an agent thinking, say) is shown at most
    # --max-frame seconds, like agg's idle limit but per frame.
    durations = [min(max(int((b - a) * 1000), 20), int(args.max_frame * 1000)) for a, b in zip(stamps, stamps[1:])]
    durations.append(3000)  # hold the last frame before looping
    # One shared palette (no dithering) keeps unchanged pixels identical
    # from frame to frame, so the GIF encoder stores only what changed.
    sheet = Image.new("RGB", (frames[0].width, frames[0].height * min(len(frames), 4)))
    for i, f in enumerate(frames[:: max(1, len(frames) // 4)][:4]):
        sheet.paste(f, (0, i * f.height))
    palette = sheet.quantize(colors=64, method=Image.Quantize.MEDIANCUT)
    pal = [f.quantize(palette=palette, dither=Image.Dither.NONE) for f in frames]
    pal[0].save(args.gif, save_all=True, append_images=pal[1:], duration=durations, loop=0, optimize=True, disposal=1)
    print(f"  {len(frames)} frames, {sum(durations) / 1000:.1f}s, {os.path.getsize(args.gif) // 1024} KiB")


if __name__ == "__main__":
    main()
