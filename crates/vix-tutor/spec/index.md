# Interactive tutorial (`vix-tutor`)

An interactive, in-editor tutorial — the `vimtutor` idea, but its lessons are
real Vix buffers the learner edits with their own hands, not a scripted
walkthrough. This spec is the v1 design (improvement plan T401); T402
implements the engine and chapter 1, T403 fills in chapters 2–6.

**Status**: T402 and T403 both done — all six chapters are real. Launch
(`vix --tutor` / **Help → Tutorial**), navigation, restart, and the live
status-bar indicator all work end to end. Chapters 2–6's checks lean more
on `some_line_has_more_than_prefix`-style "report back in prose" tasks
than chapter 1's — see § Chapters and lessons for why, and a real class of
bug (a check's target word/phrase also appearing in that same chapter's
own instructions, so a bare `text.contains(...)` check could never pass)
that every chapter's tests now guard against.

## Launch

Two entry points, both calling the same host method (`App::open_tutor`,
T402):

- `vix --tutor` (a new `Cli` flag, `src/cli.rs`) — opens straight into the
  tutorial on startup, in place of whatever files were also passed.
- **Help → Tutorial** (a new `HELP` leaf in `crates/vix-menu/src/lib.rs`,
  action id `help.tutorial`) — opens or resumes it from a running session.

Either way lands on chapter 1 the first time; resuming via the menu during
the same session returns to whichever chapter was last active (§ Session
state). There is no separate "exit tutorial" action — a chapter's buffer is
an ordinary `Tab`, so leaving is just closing it like any other file.

## Working copy

`vix-tutor` bundles each chapter's starter text as a plain file under
`crates/vix-tutor/lessons/<NN>-<slug>.txt`, pulled into the crate via
`include_str!` (not `t!` — see § Content and localization) and exposed as
`Chapter::body`. On first launch this session, the host copies every
chapter's body into a fresh temp directory — `vix-tutor-<pid>`, matching
the `vix-<feature>-<pid>` convention every other temp-dir feature already
uses (session/roam/contacts/… tests in `src/app.rs`) — one file per
chapter, then opens the active chapter's copy as a normal, freely-editable
`Tab`. The bundled originals under `crates/vix-tutor/lessons/` are never
touched; **Restart Chapter** (§ Navigation) re-copies one chapter's
original body over its working copy, discarding whatever the learner did
to it.

## Chapters and lessons

A chapter *is* a lesson: one self-contained file, matching T403's own
phrasing ("each chapter is a small self-contained lesson file"). Six for
v1, exactly `tasks.md`'s T402/T403 list:

```rust
pub struct Chapter {
    pub id: &'static str,       // "moving-around", stable across releases
    pub title: &'static str,    // i18n key, e.g. "tutor.chapter.moving_around"
    pub ordinal: usize,         // 0-based, also the CHAPTERS index
}

pub const CHAPTERS: &[Chapter] = &[
    // 0: Moving around            (T402)
    // 1: Editing basics           (T403)
    // 2: Find & replace           (T403)
    // 3: Multi-cursor & selection (T403)
    // 4: Files, tabs & palette    (T403)
    // 5: Git basics               (T403)
];
```

A chapter's prose *is* its instructions — the lesson file itself explains
what to do, the same way a real Vim tutor buffer does, not a separate
instruction pane alongside the buffer. Where a step needs the learner to
do something checkable, the lesson text says so in place (e.g. "Task 1:
delete this line.") and the buffer's own content is the thing the check
below inspects.

**This has one sharp edge, found the hard way in T403's own tests**: since
the instructions and the content share one buffer, a task that says
"delete the word REDUNDANT" necessarily puts the word `REDUNDANT` in the
buffer *twice* — once in the instruction, once in the content to edit — so
a check written as a bare `!text.contains("REDUNDANT")` can never pass; the
instruction's own copy keeps it true forever. Every check below either
matches the *exact* original content line/block (so only that line's edit
counts) or, for "type something here" tasks, checks a specific line
*starts with* or *extends past* a label rather than searching the whole
buffer for a word the instructions also happen to use.

## Progress checks

Cheap and textual, per T401's own framing ("verified against the buffer,"
not a semantic understanding of what the learner did):

```rust
pub struct Progress {
    pub total: usize,  // tasks in this chapter
    pub done: usize,   // how many currently verify as complete
}

pub fn progress(chapter_id: &str, text: &str, cursor: usize) -> Progress
```

`cursor` is a character offset, not a `(line, col)` pair — matching
`vix-modal`'s own motion-function convention (`fn(text: &str, pos: usize,
…)`), not editor-core's `(line, col)` cursor display. T402 settled this in
the implementation; the host passes `Tab::editor::get_cursor()` straight
through.

One hand-written check function per chapter (`check::moving_around`, …,
matching `CHAPTERS`), each a plain `fn(&str, usize) -> Progress` — there
is no generic checker DSL; six bespoke functions is simpler than a
framework for six cases and keeps every check auditable at a glance. A
check may use `text` alone ("does the string `DELETE ME` still appear?"),
`cursor` alone ("is the cursor past character 200?"), or both — though
T402's own chapter 1 uses `text` alone throughout: a check that is only
true while the cursor happens to sit in one place would stop being true
the instant the learner moves to the next task, undoing its own progress,
so a persistently-true text mutation is the safer default whenever a task
can be phrased that way (T402's chapter 1 tasks all can — see § Chapters
and lessons). `progress` re-runs
on every keystroke inside a tutor tab (cheap: pure string scans over a
lesson-sized buffer, well under editor-core's existing per-keystroke
budget) and the host renders `done/total` in the status bar (`status.yml`)
— nothing blocks on it. The learner can move to the next chapter, or leave
entirely, with an incomplete chapter; this is a tutorial, not a gate.

## Navigation

Three actions, `App::run_action` arms in a new `tutor.rs` slice under
`src/app/` (the one-file-per-App-concern convention `src/app/*.rs` already
follows):

| Action id | Effect |
| --- | --- |
| `tutor.next_chapter` | Open (or switch to) the next chapter's working copy. Clamps at the last chapter — no wraparound, matching `vix-list-state`'s clamp convention. |
| `tutor.prev_chapter` | Same, backward; clamps at chapter 1. |
| `tutor.restart_chapter` | Overwrite the active chapter's working copy with its pristine bundled body (§ Working copy). Prompts for confirmation first — matching every other destructive-overwrite action's own pattern (e.g. `file.revert`) — since it discards unsaved edits unconditionally. |

None of the three has a menu leaf (chapter navigation only makes sense
while already inside a tutor tab, the same reasoning `vix-org`/roam's
context-only actions already follow); they get default keybindings in
`vix-keybindings` and, since they have no menu leaf, an entry in
`vix-action-catalog::CATALOG` so F1/the palette still title them (exact
keys are T402's call, not fixed here).

## Session state

`App` gains one field, `tutor: Option<TutorSession>` (`{ dir: PathBuf,
active: usize }`), created by `open_tutor` and read/written by the three
navigation actions. **Not persisted across restarts** — an explicit v1
simplification, the same call `vix-modal`'s named registers made: a
learner who quits mid-tutorial starts over at chapter 1 next time, a
small, natural follow-on (a `tutor_progress` confy-backed file, mirroring
`session.toml`) once v1 ships and turns out to be worth it.

## Content and localization

Lesson bodies are **English-only for v1**, plain files, not routed through
`t!` — a deliberate choice, not an oversight. Six chapters of instructional
prose translated into all 14 locales `locales/*.yml` already carries is a
translation project in its own right (per-string `t!` keys would also
fragment each lesson's prose across dozens of tiny keys, unlike the
short, self-contained UI strings `t!` is built for), and it blocks nothing
else in T402/T403: the check functions operate on the buffer's raw text
regardless of what language it's in. **UI chrome around the lessons is
translated as normal** — chapter titles (`tutor.chapter.*`), the Help menu
leaf, the progress indicator, and the restart-confirmation prompt are all
`t!` keys in the usual namespace files, same as every other feature.
Per-locale lesson files (`lessons/<locale>/<NN>-<slug>.txt`, falling back
to English) are the natural follow-on structure if this becomes worth
doing later; nothing in `Chapter`/`progress`'s shape above forecloses it.

## v1 cut line

- **No ex-command-style scripted walkthroughs** — every "task" is a plain
  textual/positional check against a real buffer, not a recorded macro.
- **No progress persistence across restarts** (§ Session state).
- **No per-locale lesson content** — chrome only (§ Content and
  localization).
- **No branching/adaptive lessons** — chapters are a fixed, linear
  sequence; `next`/`prev` only, no "skip ahead if you already know this."
- **No in-tutor hint/answer reveal** — the lesson prose itself is the only
  hint; getting stuck means re-reading it or `tutor.restart_chapter`.

Each is a small, natural follow-on once v1's shapes (`Chapter`, `progress`,
the three navigation actions) exist, not a missing piece v1 is incomplete
without.

## Planned crate shape (T402+)

Not binding — T402's call — but a starting shape:

- `chapter.rs` — `Chapter`, `CHAPTERS`, `Chapter::body()` (the
  `include_str!` lookup).
- `check.rs` — `Progress`, `progress()`, and one bespoke check function per
  chapter.
- `lessons/` — the bundled starter text, one file per chapter, not `src/`
  (content, not code).
- `lib.rs` — the public surface the App shell's new `tutor.rs` slice calls.

Unit tests drive `progress()` directly against literal buffer strings (no
host needed) — one test per chapter covering "task not yet done," "task
done," and, where a check is position-sensitive, "text right but cursor
wrong." The `tutor.rs` slice's own tests (working-copy creation, `next`/
`prev` clamping, restart) follow the terminal-independent pattern
(`spec/test/index.md`): build an `App`, drive it with `KeyEvent`s, assert
on `App::tutor` and the temp dir's file contents.
