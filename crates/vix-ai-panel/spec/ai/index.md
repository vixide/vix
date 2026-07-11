# AI

The **AI** menu runs a configurable command-line assistant on text from the
editor. It defaults to the `claude` CLI, which must then be installed and on
`PATH`, but the command is **not hardcoded**: the `ai_command` setting lets you
point the menu at any assistant CLI (Claude Code, Codex, Mistral, a local
`ollama` model, …).

## Configuration

The `ai_command` setting (see `crates/vix-i18n/spec`/configuration) is a command
template, default `claude -p "{prompt}"`:

- `{prompt}` is replaced with the action's instruction (e.g.
  `Summarize this text.`).
- The input text is supplied on **stdin**. If the template instead contains
  `{file}`, that placeholder is replaced with the path of a temp file holding the
  input text; otherwise the temp file is redirected to stdin.
- An empty template falls back to the default `claude` invocation.

Examples:

| Assistant | `ai_command`                    |
| --------- | ------------------------------- |
| Claude    | `claude -p "{prompt}"` (default)|
| Codex     | `codex exec "{prompt}"`         |
| Mistral   | `mistral chat -m "{prompt}"`    |
| Ollama    | `ollama run llama3 "{prompt}"`  |

## Commands

| Item      | Prompt                | Input                                  | Output                  |
| --------- | --------------------- | -------------------------------------- | ----------------------- |
| Chat…     | (typed by the user)   | a running conversation + selection seed | **chat panel**         |
| Summarize | `Summarize this text.`| selection, else the whole file         | **new editor tab**      |
| Explain   | `Explain this text.`  | selection, else the whole file         | **new editor tab**      |
| Define    | `Define this text.`   | selection, else the word at/after cursor | **new editor tab**    |
| Annotate  | `Annotate this text.` | selection, else the whole file         | **replaces** the text   |
| Improve   | `Improve this text.`  | selection, else the whole file         | **replaces** the text   |

A separator groups the new-tab commands (Summarize / Explain / Define) from the
replace commands (Annotate / Improve). **Chat…** sits above them and opens the
interactive [AI chat panel](../vix-agent-panel/index.md) instead of running a
single canned prompt.

## Input selection

- Summarize, Explain, Annotate, and Improve act on the current **selection**, or
  the **whole file** when nothing is selected.
- **Define** is word-scoped: the selection, else the word under the cursor, else
  the next word when the cursor sits between words — never the whole buffer.

## Output modes

All five commands run the configured assistant in the **background** and capture
its full output; only one AI task runs at a time (a second is declined until the
first finishes, and empty output is treated as a failure).

- **New-tab commands** open the captured output in a new untitled editor tab
  (marked dirty so you are reminded to save it).
- **Replace commands** replace the selected range (or the whole buffer) with the
  output as a single undoable edit.

## Reviewing AI edits

By default the replace commands (Annotate / Improve) do **not** overwrite the
text immediately. Instead they open a **diff review** of the proposed change and
let you accept or reject it hunk by hunk:

| Key       | Action                                  |
| --------- | --------------------------------------- |
| `↑` / `↓` | Move between change hunks                |
| `Space`   | Toggle the highlighted hunk             |
| `a` / `r` | Accept all / reject all hunks           |
| `Enter`   | Apply the accepted hunks (one undo)     |
| `Esc`     | Discard the whole proposal              |

The diff is computed line-by-line with `similar`; accepted hunks use the new
text, rejected hunks keep the old. Set `ai_diff_review = false` to skip the
review and apply directly (the prior behavior). The review is also skipped, with a
status note, when the assistant proposes no change.

## As implemented in Vix

Pure diff state lives in the `ai_diff` module (`Review`/`Seg`, with unit-tested
hunk toggling and exact reconstruction). The host routes Annotate/Improve through
`AiDest::Diff`, opens the review in `open_ai_diff`, handles keys in `ai_diff_key`,
applies via `ai_diff_apply`, and draws it with `ui::draw_ai_diff`.

## As implemented in Vix

The host expands the `ai_command` template via `Settings::ai_command_line` and
pipes the input text to it (via a temp file) with the shared `spawn_ai` helper,
tracked as an async `AiReplace` task drained by
`poll_ai_replace`. Its `AiDest` decides the result: `NewTab` calls
`new_tab_with_content`; `Replace` calls `apply_ai_replace`. Status keys
(`status.ai_running`/`ai_done`/`ai_failed`/`ai_busy`/`ai_no_input`) report
progress. Actions: `ai.summarize`, `ai.explain`, `ai.define`, `ai.annotate`,
`ai.improve`.
