# AI

The **AI** menu runs the `claude` command-line tool on text from the editor. It
requires the `claude` CLI to be installed and on `PATH`.

## Commands

| Item      | Prompt                | Input                                  | Output                  |
| --------- | --------------------- | -------------------------------------- | ----------------------- |
| Summarize | `Summarize this text.`| selection, else the whole file         | **new editor tab**      |
| Explain   | `Explain this text.`  | selection, else the whole file         | **new editor tab**      |
| Define    | `Define this text.`   | selection, else the word at/after cursor | **new editor tab**    |
| Annotate  | `Annotate this text.` | selection, else the whole file         | **replaces** the text   |
| Improve   | `Improve this text.`  | selection, else the whole file         | **replaces** the text   |

A separator groups the new-tab commands (Summarize / Explain / Define) from the
replace commands (Annotate / Improve).

## Input selection

- Summarize, Explain, Annotate, and Improve act on the current **selection**, or
  the **whole file** when nothing is selected.
- **Define** is word-scoped: the selection, else the word under the cursor, else
  the next word when the cursor sits between words — never the whole buffer.

## Output modes

All five commands run `claude` in the **background** and capture its full output;
only one AI task runs at a time (a second is declined until the first finishes,
and empty output is treated as a failure).

- **New-tab commands** open the captured output in a new untitled editor tab
  (marked dirty so you are reminded to save it).
- **Replace commands** replace the selected range (or the whole buffer) with the
  output as a single undoable edit.

## As implemented in Vix

The host pipes the input text to `claude -p "<prompt>"` (via a temp file) with the
shared `spawn_ai` helper, tracked as an async `AiReplace` task drained by
`poll_ai_replace`. Its `AiDest` decides the result: `NewTab` calls
`new_tab_with_content`; `Replace` calls `apply_ai_replace`. Status keys
(`status.ai_running`/`ai_done`/`ai_failed`/`ai_busy`/`ai_no_input`) report
progress. Actions: `ai.summarize`, `ai.explain`, `ai.define`, `ai.annotate`,
`ai.improve`.
