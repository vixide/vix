# org-babel

Documentation: <https://orgmode.org/manual/Working-with-Source-Code.html>

Tutorial: [An Ode to Org Babel](https://donaldh.wtf/2025/11/an-ode-to-org-babel/)

**Status: capture is a minimal version; evaluation in a Jupyter kernel is
built behind the opt-in `jupyter` feature (T734e, see "Evaluation" below);
the capture select box and the rest of the capture proposal are unbuilt.**
`org.capture.babel` ships as a built-in `org_capture_templates` entry (key
`"b"`): a single `%^{Language}` prompt, then a multiline review buffer around
`#+begin_src <language>` / `#+end_src` (`vix-org-capture`'s generalized
capture engine, `crates/vix-org-capture/spec/index.md` § Decisions #7) — no header-args
select box, no RESULTS-line handling, no remembered last-used language. The
rest of this page is the original, fuller proposal below, kept as a design
record for anyone who wants to build that out; none of the "Capture options"
select box or `PromptKind::OrgCaptureBabel` it describes exists.

## Evaluation (T734e, `jupyter` capability)

In a build with the opt-in `jupyter` feature, Vix evaluates a source block in
a Jupyter kernel when the user asks: **Run → Jupyter → Run** with the cursor
in the block, or `C-c C-c` (`org.ctrl_c_ctrl_c`) anywhere in it (fences
included; it takes precedence over the table and checkbox meanings there).
Without the feature, `C-c C-c` in a block does what it does elsewhere and
nothing is ever evaluated. The block's language picks the kernel
(`jupyter-python` means `python`, as in `ob-jupyter`). When the kernel
replies, Vix writes a `#+RESULTS:` block after `#+end_src` in Org's format,
as one undo step, honoring `:results value`/`output`,
`replace`/`append`/`silent`, `:file` (where an image is saved) and `:eval
no`/`never`/`query`. **Run → Jupyter → Clear Results** in the block removes
its results. Nothing runs on open, save, export or `#+CALL`, and the trust
confirmation follows the `jupyter_confirm_evaluate` setting. The full rules
are in [`spec/jupyter`](../../../../spec/jupyter/index.md#org-babel-t734e);
the formatter is `vix-jupyter-core`'s `org` and `babel` modules.

## Capture

[Org Babel](https://orgmode.org/manual/Working-with-Source-Code.html) is Org
mode's convention for embedding source code in a document as a `#+begin_src` /
`#+end_src` block, optionally followed by a `#+RESULTS:` block holding the
last-evaluated output. Without the `jupyter` capability Vix does not execute
code (there is no sandboxed evaluator), but it can still help a user
**capture** a Babel block in the
right shape — the block header, the language tag, and the header arguments
that control how results are shown — the same way `org.capture.task` helps
capture a `* TODO` headline.

## Menu (as shipped)

**Org → Capture → Babel…** (`org.capture.babel`), after **Task…**, runs the
built-in `"b"` template. See `crates/vix-org/spec/index.md` § Menu.

## Menu (original fuller proposal, unbuilt)

| Item             | Action              | Effect                                                                                                                                                                                            |
| ---------------- | ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Capture → Babel… | `org.capture_babel` | Open a multiline editing area (Alt+Enter = newline) pre-filled from the `org_babel_capture_template` setting; the text is inserted verbatim at the cursor as a `#+begin_src` / `#+end_src` block. |

This followed the shape of the old `org.capture_todo` (predating the capture
engine): a dedicated `PromptKind::OrgCaptureBabel` prompt, multiline input, a
settings-backed template, and verbatim insertion at the cursor on confirm —
superseded by the generic engine instead.

## Capture options (unbuilt)

Before the multiline editor opens, the capture flow asks for two things that
shape the block header:

1. **Language** — free-text, defaults to the language of the last Babel
   capture (`sh`, `python`, `rustic`, `emacs-lisp`, …). Written as the token
   right after `#+begin_src`.
2. **Output mode** — a select box (fixed choices, not free-text) of the
   Babel header-argument combinations Org itself recognizes:

   | Label                    | Header args                        | Meaning                                       |
   | ------------------------ | ---------------------------------- | --------------------------------------------- |
   | Standard Output          | `:results output`                  | Show the standard output.                     |
   | Return Value             | `:results value`                   | Show the return value.                        |
   | Source Code              | `:exports code`                    | Show the source code.                         |
   | Standard Output (export) | `:exports results :results output` | Show the standard output.                     |
   | Return Value (export)    | `:exports results :results value`  | Show the return value.                        |
   | Code and Standard Output | `:exports both :results output`    | Show the source code and the standard output. |
   | Code and Return Value    | `:exports both :results value`     | Show the source code and the return value.    |

The selected label's header args string is appended verbatim to
`#+begin_src <language>`, matching Org's own header-argument syntax — e.g.
choosing **Code and Return Value** for Python produces
`#+begin_src python :exports both :results value`.

`:results output` collects everything the block prints to standard output;
`:results value` instead evaluates the block and captures the value its last
expression returns. `:exports` controls what an _export_ (Markdown/HTML/…)
shows, independent of `:results` — `code` keeps only the source code block,
`results` keeps only the `#+RESULTS:` block, `both` keeps both. Org's default
export behavior (when `:exports` is omitted) is `code`, so plain
`:results output`/`:results value` still show only the source code on
export even though the result is available interactively; the
`:exports results …` entries exist specifically to override that default and
export the result instead of the source code. There is no "neither" option
in this select box: every combination that shows nothing at all would make
the capture pointless.

## RESULTS line (unbuilt)

`#+RESULTS:` is Org's marker for the last-evaluated output of the preceding
block, formatted as a fixed-width drawer:

- A single-line result is written as `: <output>` (a colon-space prefix on
  one line).
- A multi-line result is wrapped in `#+begin_example` / `#+end_example`.

The capture flow never evaluates code, so it never fabricates a
`#+RESULTS:` block on its own — it only inserts one when the user explicitly
types example output into the capture buffer (see examples below). Whether a
`#+RESULTS:` block ends up in the file is otherwise entirely up to the
selected header args: any combination that includes `:results` implies one
will follow once the block is evaluated in Emacs or another Babel-aware
tool; **Source Code** (`:exports code`) implies there won't be one shown on
export even if the block was evaluated.

## Examples (unbuilt)

Shell script, no header args, no results:

```
#+begin_src sh
ls
#+end_src
```

Python, **Standard Output** (`:results output`):

```
#+begin_src python :results output
print("Hello")
#+end_src

#+RESULTS:
: Hello
```

Python, **Return Value** (`:results value`) — the block's last expression is
the result, not anything it prints:

```
#+begin_src python :results value
"Hello"
#+end_src

#+RESULTS:
: Hello
```

Shell, **Standard Output (export)** (`:exports results :results output`) —
only the `#+RESULTS:` block survives export, the source is dropped:

```
#+begin_src sh :exports results :results output
echo Hello
#+end_src

#+RESULTS:
: Hello
```

Rust, **Code and Standard Output** (`:exports both :results output`):

```
#+begin_src rustic :exports both :results output
fn main() {
    println!("Hello");
}
#+end_src

#+RESULTS:
: Hello
```

## Relationship to existing Org insertion

`Tools → Insert → Org / Markers / Begin-End` (`crates/vix-org/spec/insert-org.md`)
already covers generic `#+BEGIN_…` / `#+END_…` block insertion (Comment,
Center, Quote, Verse) via `App::insert_block`. Org Babel capture is
deliberately a separate, Capture-menu feature rather than a fifth Begin-End
entry: a source block additionally needs a language tag and header
arguments, and — unlike the other blocks — is meant to be filled with real
content up front (via the multiline capture editor), not toggled empty
around a selection.
