# The agents directory is named `agents`, lowercase

**Every directory that holds AI-agent material is named `agents`, in lowercase.**
Not `AGENTS`, not `Agents`.

| Path | Holds |
| ---- | ----- |
| `agents/` | This repository's agent guidance: the topic guides and shared reference the entry point links to |
| `agents/share/` | Material shared across guides — the crate map, the glossary |
| `.claude/agents/` | Claude Code subagent definitions, when a repository has them |

## Why lowercase

- **Every other directory in the tree is lowercase** — `crates/`, `docs/`,
  `spec/`, `locales/`, `scripts/`, `langs/`, `themes/`, `benches/`, `fuzz/`.
  A single shouting directory is an exception a reader has to remember, and one
  more thing to get wrong in a path.
- **Case-insensitive filesystems hide mistakes.** macOS and Windows will happily
  open `AGENTS/workflow.md` when the file is at `agents/workflow.md`; Linux and
  CI will not. One spelling, always, removes the class of bug where a link works
  for the author and 404s for everyone else.
- **Tools that scan for agent material look for the lowercase name**, because
  that is what the convention outside this repository uses too.

## The one exception: `AGENTS.md`

The entry-point **file** stays `AGENTS.md`, uppercase. That name is an external
convention — agent tooling looks for `AGENTS.md` (and `CLAUDE.md`) at the
repository root, the way it looks for `README.md` and `LICENSE`. Renaming the
file would hide it from the tools that read it.

So: an uppercase *file* at the root, a lowercase *directory* beside it.

```text
AGENTS.md          <- the entry point; uppercase by outside convention
agents/            <- everything it links to; lowercase like every other directory
  conventions.md
  workflow.md
  share/
    crate-map.md
    glossary.md
```

`CLAUDE.md` follows the same rule for the same reason: it is a root file that
points at `AGENTS.md`, not a directory.

## Keeping it that way

`scripts/check-docs` resolves every documented path, so a link to a directory
that is not there fails the gate — which is what catches a stray `AGENTS/`
reference after the rename. On a case-insensitive filesystem that check passes
either way, so CI (Linux) is the backstop.

See [`AGENTS.md`](../../AGENTS.md) for the guidance itself and
[`spec/index/index.md`](../index/index.md) for the project overview.
