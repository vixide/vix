---
name: vix-ship
description: Branch, verify, commit, merge, and push a finished change in vixide/vix (and its sibling site repo vixide.github.io) — the repo's own git workflow across three forges. Use once a change from vix-spec-change is green and ready to land, or whenever asked to commit/merge/push/release in these repos.
---

# vix-ship — land a change the way this repo does

`vix` pushes to **three forges** from one `origin` remote (`git remote -v`
shows three push URLs: GitHub, GitLab, Codeberg). The public site lives at
`vix/vixide.github.io/` — a monorepo subproject (see
`spec/monorepo-github-pages/index.md`), published with `make github-pages`
to a **separate**, GitHub-only, read-only sibling repo (`../vixide.github.io`
next to `vix`) via `git subtree push`. Never edit `../vixide.github.io`
directly — every site change lands in `vix/vixide.github.io/` and gets
published from there.

## In `vix`: one feature branch per change

1. `git checkout -b <short-kebab-name>` off `main`.
2. Implement (see `vix-spec-change`), and confirm `scripts/check` is green
   **before** committing — don't commit red and fix in a follow-up.
3. Commit with a trailer:
   `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
   - **Commits are GPG/SSH-signed** (`commit.gpgsign=true`). The *first*
     commit in a session can hang for up to ~2 minutes waiting on the SSH
     key's passphrase prompt — if `git commit` seems to hang, that's why;
     retry with a longer timeout rather than assuming it's broken. Verify
     with `git log --show-signature -1` if in doubt.
4. `git checkout main && git merge --no-ff <branch> -m "Merge branch '<branch>'"`
   — always `--no-ff`, even for a single commit, so the feature boundary
   stays visible in history.
5. `git branch -d <branch>` — delete it once merged; branches are
   disposable, `main` is the record.
6. Before pushing: this is outward-facing (three public forges) — confirm
   with the user first unless already told to push without asking.
7. `git push origin main` pushes to all three forges in one command. If one
   forge fails, **read the error before retrying blindly**:
   - A GitLab SSH `Connection reset` (e.g. by a Cloudflare-fronted IP) that
     repeats across retries, with `status.gitlab.com` green and
     `git ls-remote https://gitlab.com/...` working over HTTPS, is a local
     network/egress issue, not a GitLab outage or an auth problem — don't
     try to invent HTTPS credentials to route around it. Report which
     forge(s) landed and which didn't, with the exact commits each is
     missing (`git ls-remote <url> HEAD`), and let the user push the
     straggler once connectivity clears.
8. **Verify CI actually went green**, don't assume a push succeeded just
   because the build was green locally — new CI jobs especially can behave
   differently on real infra (network egress, forge-specific images):
   `gh run list --repo vixide/vix --limit 3` to find the run, then
   `gh run watch <id> --repo vixide/vix --exit-status`. A `continue-on-error`
   step failing (shown as an annotation) with the job still `success` is
   working as designed, not a regression — check the job's `conclusion`
   field (`gh run view <id> --json conclusion`), not just the annotation.

## `vixide.github.io/`: edit inside `vix`, publish via `git subtree push`

Changes to the site are ordinary commits under `vix/vixide.github.io/` —
same branch, same `--no-ff` merge to `main`, same `vix-spec-change` process
as any other part of `vix`. Its own rule (`vixide.github.io/AGENTS.md`):
`npm run build` must prerender cleanly before publishing. If the local Node
is too old for a build-tool dependency (has happened: `rolldown` needing
`node:util`'s `styleText`, added in Node 20+), don't block on it — confirm
the failure is pre-existing (reproduces on `main` before your change, e.g.
via `git stash`) and that CI's pinned Node version
(`vixide.github.io/.github/workflows/deploy.yml`, `actions/setup-node`) is
new enough.

Once merged to `vix`'s `main` (and `vix` itself is pushed — subtree push
reads from local history, not the forges), publish the subtree to the
sibling repo:

```
make github-pages
```

(equivalent to `git subtree push --prefix=vixide.github.io github-pages main`
— the `github-pages` remote points at
`git@github.com:vixide/vixide.github.io.git` and is created on first use if
missing). This is a normal (non-force) push derived from the subtree's
history — it should fast-forward `vixide.github.io`'s `main`. If it doesn't,
the sibling repo has a commit that didn't come from a subtree push (i.e.
someone edited it directly, breaking the rule above) — stop and reconcile
rather than forcing.

This is a public GitHub Pages site — confirm before publishing here too,
same as `vix`. After pushing, verify the **deploy** workflow (not your
local build) actually goes green:
`gh run watch <id> --repo vixide/vixide.github.io --exit-status`, then spot
check the live URL(s) with `curl -s -o /dev/null -w '%{http_code}'`.
