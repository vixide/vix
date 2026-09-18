# Themes demo (T406, tasks.md): the bundled default theme's real syntax
# highlighting across a variety of Rust constructs (keywords, strings,
# numbers, comments, a doc comment, an attribute). Regenerate with
# scripts/render-demos.sh.
#
# Two interactive ways to reach further theme UI were tried and dropped,
# both for the same newly-found reason (see tasks.md's T406 entry for the
# full writeup): a real, reproducible bug where certain `t!()`-translated
# labels render as their raw i18n key (`ui.theme_editor_title`, …) instead
# of translated text when opened through the live keyboard-driven app --
# confirmed NOT a translation-lookup bug (a direct `TestBackend` render of
# the exact same code path translates correctly), reproducible via the real
# `vix` binary consistently (release and debug builds alike, after a full
# rebuild) -- root cause not yet found; whether real VHS's own pipeline
# would also show it is untested (this session never got a working VHS
# install).
#   - Command Palette ">Theme" -- also not viable on its own merits: no
#     single "open the theme chooser" action exists, each theme is its own
#     dynamically-generated `view.theme:<name>` menu leaf, none palette-
#     searchable either.
#   - Command Palette ">Edit Theme" (`view.theme_edit`, palette entry added
#     this session): reaches the interactive Theme Editor, but its 15
#     color-slot labels are exactly what hits the bug above.

Output docs/demos/themes.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms

Hide
Type "cd examples/demo-workspace && vix rust-app/src/main.rs"
Enter
Sleep 1s
Show

# Scroll down through the file's mix of doc comments, an attribute, string
# and numeric literals, keywords, and a nested test module -- a representative
# sample of what the active theme's syntax palette actually colors.
Down 10
Sleep 500ms
Down 10
Sleep 500ms
Down 10
Sleep 1s
