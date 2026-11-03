# DRAFT, not posted: Show HN

> Draft. Not posted. Replace placeholders; post by hand only after the
> maintainer approves.

## Title options

- Show HN: Vix 2.0, a terminal editor with remote editing and real-time co-editing
- Show HN: Vix, a keyboard-friendly terminal IDE in Rust (2.0)

## First comment

Hi HN. Vix is a terminal text editor in Rust (ratatui) with a menu bar and the
shortcuts you already know, plus the IDE pieces I kept leaving the terminal for:
language servers, a debugger, Git, a database workbench and Org mode.

2.0 adds:

- Remote editing over SSH (`vix ssh://user@host/path`), nothing to install on the
  remote side. Experimental. Containers work the same way; Kubernetes and WSL are
  built but only tested against a stand-in so far.
- Cowork, real-time co-editing, encrypted, no server. It is an opt-in build
  feature, not in the prebuilt releases.
- A code-aware chat menu (local notes, IRC) with threads anchored to lines.
- A Large File Viewer: on an M4 Max with a warm cache, a 4 GiB log shows its first
  screen in under a millisecond, in under 5 MiB.
- Local History, Search Everywhere, a Refactor menu, an undo tree and more.

AI is optional (`ai = "off"` removes it and refuses every request).

It is specification-driven: every crate has a spec and the repo has a docs gate.
There are no lines of `unsafe` (the workspace forbids it).

What I would like feedback on: remote editing on setups I have not tried, and
what is missing from the daily-driver experience for you.

Release notes: [LINK]. Source: [LINK].

## Likely questions, with honest answers

- **Is it modal?** No, it starts non-modal. A Vi keymap is a setting; a deeper
  modal implementation is still on the roadmap, not done.
- **How does it compare with X?** Do not compare. Point to the docs for
  migration guides (for Vim, Emacs, Helix, VS Code, IntelliJ and others users).
- **Is the remote mode safe?** The remote workspace is untrusted until you say
  so; until then language servers, the debugger and tasks do not run there.
- **Windows?** Windows builds exist, including ARM from 2.0. Say which parts are
  less tested if asked, after checking the current state.
- **Why Cowork is not in the default build?** It is new; keeping it opt-in keeps
  the default build and prebuilt releases small and unchanged.
