# AI

The **AI** menu runs the `claude` command-line tool on text from the editor. It
requires the `claude` CLI to be installed and on `PATH`.

## Commands

| Item      | Prompt                | Input                                  | Output                  |
| --------- | --------------------- | -------------------------------------- | ----------------------- |
| Summarize | `Summarize this text.`| selection, else the whole file         | bottom dock (streamed)  |
| Explain   | `Explain this text.`  | selection, else the whole file         | bottom dock (streamed)  |
| Define    | `Define this text.`   | selection, else the word at/after cursor | bottom dock (streamed)|
| Annotate  | `Annotate this text.` | selection, else the whole file         | **replaces** the text   |
| Improve   | `Improve this text.`  | selection, else the whole file         | **replaces** the text   |

A separator groups the dock commands (Summarize / Explain / Define) from the
replace commands (Annotate / Improve).

## Input selection

- Summarize, Explain, Annotate, and Improve act on the current **selection**, or
  the **whole file** when nothing is selected.
- **Define** is word-scoped: the selection, else the word under the cursor, else
  the next word when the cursor sits between words — never the whole buffer.

## Output modes

- **Dock commands** stream `claude`'s response into the bottom dock (read-only),
  via the same background-command machinery as Tools → Run Command.
- **Replace commands** run in the background, capture the full output, and replace
  the selected range (or the whole buffer) with it as a single undoable edit.
  Only one replace task runs at a time; a second is declined until the first
  finishes. Empty output leaves the text unchanged.

## As implemented in Vix

The host pipes the input text to `claude -p "<prompt>"` (via a temp file). Dock
commands use `run_command`; replace commands use an async `AiReplace` task drained
by `poll_ai_replace`, which applies the result with `apply_ai_replace`. Status
keys (`status.ai_running`/`ai_done`/`ai_failed`/`ai_busy`/`ai_no_input`) report
progress. Actions: `ai.summarize`, `ai.explain`, `ai.define`, `ai.annotate`,
`ai.improve`.
