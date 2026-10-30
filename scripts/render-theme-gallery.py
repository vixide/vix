#!/usr/bin/env python3
"""Render one real screenshot of Vix per bundled theme, plus the site's
1200x630 social card (tasks.md T740 and T741).

Every image is a capture of the real `vix` release binary running in a real
pseudo-terminal -- nothing is drawn by hand:

    pexpect (a real pty, 120x32) runs `vix rust-app/src/main.rs` in a
    fresh copy of tui/examples/demo-workspace under a throwaway HOME whose
    config.toml sets `theme = "<name>"`  ->  every byte Vix writes is fed
    to pyte (a VT100/xterm screen emulator, 24-bit colour aware)  ->  the
    emulated screen's cells (character, foreground, background, bold,
    reverse) are painted cell by cell with Pillow in a Nerd Font (Vix's
    file and panel icons are Nerd Font glyphs)  ->  PNG.

This is the same idea as scripts/vhs_lite.py + agg (a real pty, then a
terminal emulator that renders the byte stream without a browser), reduced
to one still frame, so it needs neither VHS/ttyd/Chromium nor agg. The only
things this script adds on top of the emulated screen are the light
box-drawing lines (drawn as lines rather than font glyphs so borders join
up across rows) and, for the social card, a title band above the
screenshot.

Prerequisites: Python 3 with `pexpect`, `pyte` and `Pillow`
(`pip install --user pexpect pyte pillow`), JetBrains Mono Nerd Font Mono
installed (or pass `--font`/`--bold-font`), and a release build
(`cargo build --release -p vix-tui`).

Usage:
    scripts/render-theme-gallery.py [--vix target/release/vix]
        [--out docs/demos/themes] [--card docs/demos/social-card.png]

Not part of CI or scripts/check, same as scripts/render-demos.sh: run it by
hand after a change that affects what Vix looks like, review the PNGs, copy
them into vixide.github.io/static/assets/demos/ and commit them.
"""
import argparse
import json
import os
import pathlib
import re
import shutil
import tempfile
import time

import pexpect
import pyte
from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
COLS, ROWS = 120, 32
FONT_SIZE = 16

# xterm's default 16-colour palette, for any cell Vix colours by name
# rather than by RGB (pyte reports those as names).
NAMED = {
    "black": "000000", "red": "cd0000", "green": "00cd00", "brown": "cdcd00",
    "blue": "0000ee", "magenta": "cd00cd", "cyan": "00cdcd", "white": "e5e5e5",
    "brightblack": "7f7f7f", "brightred": "ff0000", "brightgreen": "00ff00",
    "brightbrown": "ffff00", "brightblue": "5c5cff", "brightmagenta": "ff00ff",
    "brightcyan": "00ffff", "brightwhite": "ffffff",
}

# Light box-drawing characters -> which arms (up, down, left, right) to draw.
BOX = {
    "─": "lr", "│": "ud", "┌": "dr", "┐": "dl", "└": "ur", "┘": "ul",
    "├": "udr", "┤": "udl", "┬": "dlr", "┴": "ulr", "┼": "udlr",
    "╭": "dr", "╮": "dl", "╰": "ur", "╯": "ul",
}


def rgb(hex6: str) -> tuple:
    return tuple(int(hex6[i:i + 2], 16) for i in (0, 2, 4))


def colour(value: str, fallback: str) -> tuple:
    if value == "default":
        return rgb(fallback)
    if re.fullmatch(r"[0-9a-fA-F]{6}", value):
        return rgb(value)
    return rgb(NAMED.get(value, fallback))


def find_font(name: str) -> str:
    for folder in ("/Library/Fonts", "~/Library/Fonts", "~/.local/share/fonts",
                   "/usr/share/fonts/truetype"):
        path = pathlib.Path(folder).expanduser() / name
        if path.exists():
            return str(path)
    return name  # let Pillow search its own font path, or fail loudly


def config_path(home: pathlib.Path) -> pathlib.Path:
    """Where Vix (confy) wrote config.toml under a given HOME."""
    found = sorted(home.rglob("config.toml"))
    found = [p for p in found if "demo-workspace" not in p.parts]
    if not found:
        raise SystemExit(f"vix wrote no config.toml under {home}")
    return found[0]


def run_vix(vix: str, home: pathlib.Path, workspace: pathlib.Path, file: str,
            settle: float) -> pyte.Screen:
    """Run Vix in a real pty, return the emulated screen once it settles."""
    screen = pyte.Screen(COLS, ROWS)
    stream = pyte.ByteStream(screen)
    env = {
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_DATA_HOME": str(home / ".local" / "share"),
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "TERM": "xterm-256color",
        "COLORTERM": "truecolor",
        "LANG": "en_US.UTF-8",
        "LC_ALL": "en_US.UTF-8",
    }
    child = pexpect.spawn(vix, ["--locale", "en", file], cwd=str(workspace),
                          env=env, dimensions=(ROWS, COLS), timeout=None)
    deadline = time.time() + settle
    while time.time() < deadline:
        try:
            stream.feed(child.read_nonblocking(65536, timeout=0.2))
        except pexpect.TIMEOUT:
            continue
        except pexpect.EOF:
            break
    child.terminate(force=True)
    return screen


class Painter:
    def __init__(self, font_path: str, bold_path: str):
        self.regular = ImageFont.truetype(font_path, FONT_SIZE)
        self.bold = ImageFont.truetype(bold_path, FONT_SIZE)
        self.cw = round(self.regular.getlength("M"))
        self.ch = round(FONT_SIZE * 1.3)

    def paint(self, screen: pyte.Screen, default_fg: str,
              default_bg: str) -> Image.Image:
        img = Image.new("RGB", (COLS * self.cw, ROWS * self.ch), rgb(default_bg))
        draw = ImageDraw.Draw(img)
        top_pad = (self.ch - FONT_SIZE) // 2
        for y in range(ROWS):
            row = screen.buffer[y]
            for x in range(COLS):
                cell = row[x]
                fg = colour(cell.fg, default_fg)
                bg = colour(cell.bg, default_bg)
                if cell.reverse:
                    fg, bg = bg, fg
                if (not screen.cursor.hidden and x == screen.cursor.x
                        and y == screen.cursor.y):
                    fg, bg = bg, fg
                x0, y0 = x * self.cw, y * self.ch
                draw.rectangle([x0, y0, x0 + self.cw - 1, y0 + self.ch - 1], fill=bg)
                ch = cell.data
                if not ch or ch == " ":
                    continue
                arms = BOX.get(ch)
                if arms:
                    cx, cy = x0 + self.cw // 2, y0 + self.ch // 2
                    if "u" in arms:
                        draw.line([cx, y0, cx, cy], fill=fg)
                    if "d" in arms:
                        draw.line([cx, cy, cx, y0 + self.ch - 1], fill=fg)
                    if "l" in arms:
                        draw.line([x0, cy, cx, cy], fill=fg)
                    if "r" in arms:
                        draw.line([cx, cy, x0 + self.cw - 1, cy], fill=fg)
                    continue
                font = self.bold if cell.bold else self.regular
                draw.text((x0, y0 + top_pad), ch, font=font, fill=fg)
        return img


def save_png(img: Image.Image, path: pathlib.Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    # Terminal frames have few distinct colours; a 256-colour palette keeps
    # each PNG small without visible loss.
    img.quantize(colors=256, method=Image.Quantize.MEDIANCUT,
                 dither=Image.Dither.NONE).save(path, optimize=True)


def social_card(shot: Image.Image, font_path: str, bold_path: str, bg: tuple,
                fg: tuple) -> Image.Image:
    """A 1200x630 card: a title band above the real screenshot."""
    card = Image.new("RGB", (1200, 630), bg)
    draw = ImageDraw.Draw(card)
    title = ImageFont.truetype(bold_path, 44)
    sub = ImageFont.truetype(font_path, 22)
    draw.text((48, 30), "Vix", font=title, fill=fg)
    draw.text((48, 88), "A keyboard-friendly terminal text editor and IDE",
              font=sub, fill=fg)
    band = 140
    scale = min((1200 - 96) / shot.width, (630 - band - 24) / shot.height)
    small = shot.resize((round(shot.width * scale), round(shot.height * scale)),
                        Image.Resampling.LANCZOS)
    card.paste(small, ((1200 - small.width) // 2, band))
    return card


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--vix", default=str(ROOT / "target" / "release" / "vix"))
    parser.add_argument("--out", default=str(ROOT / "docs" / "demos" / "themes"))
    parser.add_argument("--card", default=str(ROOT / "docs" / "demos" / "social-card.png"))
    parser.add_argument("--font", default=find_font("JetBrainsMonoNerdFontMono-Regular.ttf"))
    parser.add_argument("--bold-font", default=find_font("JetBrainsMonoNerdFontMono-Bold.ttf"))
    parser.add_argument("--file", default="rust-app/src/main.rs")
    parser.add_argument("--settle", type=float, default=4.0,
                        help="seconds to let Vix draw before capturing")
    args = parser.parse_args()

    demo = ROOT / "tui" / "examples" / "demo-workspace"
    painter = Painter(args.font, args.bold_font)
    out = pathlib.Path(args.out)

    themes = []
    for path in sorted((ROOT / "tui" / "themes").glob("*.json")):
        data = json.loads(path.read_text())
        themes.append((path.stem, data["name"], data["editor"]))

    for slug, name, editor in themes:
        # A fixed, short HOME (the status bar shows the file's absolute
        # path, so a random temp name would differ in every image) with a
        # copy of the demo workspace outside the repository, so no
        # version-control branch shows and the real workspace is never
        # modified.
        home = pathlib.Path(tempfile.gettempdir() if os.name != "posix" else "/tmp") / "vix-gallery"
        shutil.rmtree(home, ignore_errors=True)
        home.mkdir(parents=True)
        workspace = home / "demo-workspace"
        shutil.copytree(demo, workspace)
        try:
            # The first run writes the default config.toml; then set the theme.
            run_vix(args.vix, home, workspace, args.file, 1.5)
            cfg = config_path(home)
            text = re.sub(r'(?m)^theme = ".*"$', f'theme = {json.dumps(name)}',
                          cfg.read_text())
            cfg.write_text(text)
            screen = run_vix(args.vix, home, workspace, args.file, args.settle)
        finally:
            shutil.rmtree(home, ignore_errors=True)
        hex_of = "{:02x}{:02x}{:02x}".format
        img = painter.paint(screen, hex_of(*editor["foreground"]),
                            hex_of(*editor["background"]))
        save_png(img, out / f"{slug}.png")
        print(f"{name:28} -> {out / (slug + '.png')}")
        if slug == "dark":
            card = social_card(img, args.font, args.bold_font,
                               tuple(editor["background"]),
                               tuple(editor["foreground"]))
            save_png(card, pathlib.Path(args.card))
            print(f"{'social card':28} -> {args.card}")


if __name__ == "__main__":
    main()
