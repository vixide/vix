# DRAFT, not posted: Reddit

> Drafts. Not posted. Read each subreddit's self-promotion rules first, and post
> only with the maintainer's approval.

## r/rust

**Title:** Vix 2.0: a ratatui terminal editor, now with remote editing over SSH and real-time co-editing

Vix is a terminal editor built on ratatui, organized as a Cargo workspace of
146 small `vix-*` crates behind a thin app shell, with `unsafe_code = "forbid"`,
`clippy::pedantic` for every crate and a spec for each crate.

2.0 highlights:

- Remote editing over SSH and into containers (experimental). A single
  authority abstraction (`vix-authority`) sits behind every file access and
  process spawn, so search, Git, language servers and the terminal follow the
  workspace to the other machine.
- Cowork: encrypted real-time co-editing with conflict-free merging (opt-in
  `cowork` feature).
- A Large File Viewer that opens multi-gigabyte files read-only without loading them.
- Local History, an undo tree, multiple-caret depth, an interactive Git graph and rebase.

Because it is a major version, the library crates made breaking changes: typed
errors in place of `Result<_, String>`, `#[non_exhaustive]` on types that keep
growing, and a much smaller public surface on `App`. The changelog's Breaking
section lists each one. MSRV is Rust 1.97.

Notes: [LINK]. Source: [LINK]. Questions and criticism of the architecture are
welcome.

## r/commandline

**Title:** Vix 2.0, a terminal editor: open a folder over SSH, edit with others, view 4 GiB logs

Short version of the above. Lead with a one-line install (Homebrew, npm or
Cargo), a screenshot or the overview demo from `docs/demos/`, and the Large File
Viewer. Say plainly that Cowork needs a custom build and remote editing is
experimental.
