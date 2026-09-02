# AI Statement

Vix is developed with substantial AI assistance — most of its code, specs,
docs, and this file itself are written by AI coding assistants (primarily
[Claude Code](https://claude.com/claude-code)), directed by the project's
human maintainer. This file says what that means in practice: what's
disclosed, who's accountable, what bar AI-authored work has to clear, and
what an AI agent is and isn't authorized to do on its own.

## Human accountability

[Joel Parker Henderson](https://joelparkerhenderson.com) (the repository
owner) directs, reviews, and is accountable for everything that ships here,
regardless of which hand typed it. An AI agent does not merge its own work
unreviewed, tag a release, or act on the outward-facing steps below without
the standing authorization (or explicit go-ahead) to do so.

## How to identify AI-authored work

Nothing is hidden. Every AI-authored commit carries a trailer identifying
the model and, where available, a link to the full session that produced it:

```
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_...
```

`git log --grep='Co-Authored-By: Claude'` finds all of them. The exact model
varies by when the work was done (Claude Opus, Sonnet, and others appear
across the project's history); each commit records the one actually used
rather than this file trying to stay current with it.

**On "a tool shouldn't be named as an author, co-author, or signer."** Some
projects hold that line and disclose AI involvement only in the PR/commit
*description*, never in a trailer — reasoning that a `Co-Authored-By` line
puts the tool where only a person belongs. Vix disagrees, deliberately: git's
`Author`/`Committer` identity — the field with actual authorship weight,
what a `git blame` or a signed commit's signature resolves to — is **always
the human maintainer**, never a tool. That never changes here. The
`Co-Authored-By` trailer is a different, lighter thing: a standard git
convention (the same one GitHub uses for pairing) for crediting a
contributor to a change without claiming they hold that formal identity. A
tool named there is credited, not authored-as. That distinction is exactly
why Vix is comfortable using it where a stricter policy would not.

## Same bar, same process

AI-authored changes go through the same spec-driven workflow as any other
change — [`AGENTS.md`](AGENTS.md) is canonical: read or update the owning
spec first, implement, internationalize, document, and test, then pass the
same [`scripts/check`](scripts/check) gate everything else does (pedantic
Clippy with `-D warnings`, the full test suite, a warnings-denied doc build,
documentation-integrity checks) before it merges. Being AI-written earns no
exemption from any of that.

## Governance: standing authorizations

By default, an AI agent working on this repo **confirms before** a
hard-to-reverse or outward-facing action — a force-push, publishing a
release, publishing a package — rather than acting alone. That default holds
except where the maintainer has deliberately granted a standing exception,
each revocable at any time:

| Action | Status | Granted |
| ------ | ------ | ------- |
| `cargo publish` (crates.io) | **Standing authorization** — may run as part of a legitimate release step (version bumped, `CHANGELOG.md` updated, gate green), without asking first each time | 2026-09-02 |

Everything else outward-facing — force-pushing, deleting a branch on a
forge, cutting a tagged release, editing repository settings — is **not**
pre-authorized and still gets confirmed first, change by change.

## Licensing

Vix is offered under your choice of Apache-2.0, BSD-3-Clause, MIT,
GPL-2.0-only, or GPL-3.0-only (see [`LICENSE`](LICENSE) and
[`deny.toml`](deny.toml)). Anthropic does not claim ownership over Claude's
outputs. Whether AI-assisted contributions carry copyright, and under what
terms, is an unsettled question that varies by jurisdiction — if that matters
for how you use this project, check the law that applies to you rather than
treating anything here as legal advice.

## Questions

Open an issue, or reach the maintainer directly at
[joel@joelparkerhenderson.com](mailto:joel@joelparkerhenderson.com).
