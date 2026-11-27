# Incident Response

This is what happens from the moment a vulnerability report arrives until
users have a fixed release and know about it. [`SECURITY.md`](../SECURITY.md)
is the promise to reporters (channels, response targets, disclosure); this page
is how the maintainer keeps it. It sits next to the
[threat model](threat-model.md), which is what a report is judged against.

Steps marked **Maintainer only** are outward-facing or hard to undo: deciding
about a report, publishing an advisory, tagging, creating a release, yanking.
Under [`AI_STATEMENT.md`](../AI_STATEMENT.md) an AI agent may help write and
test a fix, but does not take those steps.

## Who decides

The maintainer (the repository owner, the contact in `SECURITY.md`) decides
whether a report is accepted, its severity, the disclosure date, when the fix
is released, and whether anything is yanked. Vix has one maintainer, so there
is no separate security team or rotation; the response targets in
`SECURITY.md` are written for that.

## 1. A report arrives

Reports come in two ways (`SECURITY.md`, "Reporting a vulnerability"):

- **A GitHub private vulnerability report**, from the Security tab of
  `vixide/vix`. It opens as a draft security advisory that only the
  maintainer and the reporter see. Private reporting has been on since
  2026-11-17 (`gh api repos/vixide/vix/private-vulnerability-reporting`
  shows it).
- **An email** to the maintainer. GitLab and Codeberg have no private report
  feature, so email is the channel from those forges. **Maintainer only:**
  open a draft advisory for it yourself (Security → Advisories → New draft
  security advisory) so the fix has the same private place to live, and add
  the reporter to it if they have a GitHub account.

If a report arrives in public (an issue, pull request or discussion on any
forge), do not discuss the details there. Ask the reporter to send it
privately, and hide or delete the public copy. If the details are already
public, treat it as actively exploited: fix and disclose as soon as ready.

Acknowledge the report within 7 days.

## 2. Triage

**Maintainer only.** Within 14 days:

1. Reproduce it on the latest release and on `main`. Only the latest release
   is supported.
2. Judge it against [Scope](../SECURITY.md#scope), the
   [Threat model](../SECURITY.md#threat-model) and its
   [accepted risks](../SECURITY.md#accepted-risks). A declined report gets
   the reason, usually a pointer to one of them.
3. Rate an accepted report with a CVSS v3.1 base score (critical, high,
   medium or low, as in `SECURITY.md`). That sets the target: 30 days for
   critical or high, 90 days for medium or low.
4. Fill in the draft advisory: the affected crates (ecosystem **Rust**,
   package names as on crates.io, such as `vix-tui` and the `vix-*` crate
   with the bug), the affected versions (`< ` the version that will fix it),
   the severity, and a weakness (CWE) if one fits.
5. Agree the disclosure date with the reporter: the fixed release, or 90
   days after the report, whichever comes first.

## 3. Fix it privately

Until the fixed release is out, nothing about the fix goes anywhere public:
no public branch, pull request, issue or commit message, and no line in
`tasks.md` or `CHANGELOG.md` on `main`.

1. From the draft advisory, **Start a temporary private fork**. Clone it and
   branch there, exactly as for any other change
   ([workflow](contributing-workflow.md)): the owning spec first, a test that
   fails before the fix, then the fix.
2. In the same branch, write what the release needs: a **Security** entry
   under `[Unreleased]` in `CHANGELOG.md` (what was wrong, who it affected,
   what to do, credit), and the matching changes to `SECURITY.md` and the
   [threat model](threat-model.md) if a trust boundary moved.
3. Run the whole gate locally (`scripts/check`, or `scripts/attest`). Do not
   count on CI: the fix is not on any public branch, so the forges' CI never
   sees it before the merge.
4. Open the pull request inside the private fork, and offer the reporter the
   chance to check the fix there (`SECURITY.md`, "Fix").

A vulnerability in a dependency is out of scope for a report here, but if
Vix ships an affected version, update it the same way (`cargo update -p
<crate> --precise <fixed version>`) and say so in the **Security** entry.

## 4. Cut the fixed release

**Maintainer only.** A patch release, made as in
[Releasing](release/index.md); none of its steps change for a security fix.
Keep the time between merging and tagging short: the fix is public the moment
it is merged.

1. In the private branch, bump the version and cut the changelog: the
   workspace version and its internal pins (`AI_STATEMENT.md` §1, checked
   with `cargo semver-checks --workspace`), and `[Unreleased]` becomes a
   `## [X.Y.Z]` section with its **Security** entry.
2. Merge the private fork's pull request from the advisory page. That merges
   it into `main` on GitHub. Then bring the other two forges up to date
   (`origin` pushes to GitHub, GitLab and Codeberg at once):

   ```sh
   git switch main
   git pull --ff-only
   git push origin main
   ```

3. Wait for CI to be green on `main`. Tag only a commit that is already on
   `main` ([Releasing](release/index.md), step 3). Push a signed tag:

   ```sh
   git tag -s X.Y.Z -m X.Y.Z
   git push origin X.Y.Z
   gh run watch
   ```

   The tag starts each forge's release: `dist` on GitHub (archives,
   installers, the CycloneDX SBOMs, the Homebrew tap, npm, the Linux packages),
   the `release` stage on GitLab and the release workflow on Codeberg.
   `scripts/release-local` is the local alternative, described on the same
   page.
4. Publish the crates to crates.io, in dependency order, with
   `cargo publish -p <crate>` for each (`AI_STATEMENT.md` §4–§5). The
   crates hold the fixed source, so their publication is part of the
   disclosure: it happens now, not before the merge.

## 5. Tell users

**Maintainer only.**

- **GitHub Security Advisory.** On the draft advisory, request a CVE if the
  issue warrants one, credit the reporter (unless they declined), set the
  patched version, and **Publish advisory**. GitHub adds it to its advisory
  database once it has reviewed it, so Dependabot alerts the people who
  depend on the crate.
- **RustSec.** For a published crate, open a pull request to
  [`rustsec/advisory-db`](https://github.com/rustsec/advisory-db) adding the
  advisory under `crates/<crate>/` (the database assigns its RUSTSEC id), so
  `cargo audit` and `cargo deny` warn downstream users.
- **`CHANGELOG.md`.** The **Security** entry is already in the release
  section. `dist` uses that section as the GitHub Release's notes; link the
  advisory from the GitLab and Codeberg release pages too, since neither forge
  has an advisory database of its own.
- **The reporter** hears that it is out, with the links.

## 6. Yank a crate version

**Maintainer only.** Yanking stops new projects from resolving to a version.
It deletes nothing: a project whose `Cargo.lock` already names the version
keeps building. Yank only when
using the version is itself harmful (a fix that cannot simply be upgraded to
is not a reason), and only after the fixed version is on crates.io, so a plain
`cargo install vix-tui` still resolves:

```sh
cargo yank --version X.Y.Z vix-tui
cargo yank --version X.Y.Z <crate>      # each affected vix-* crate
cargo yank --version X.Y.Z --undo vix-tui
```

The other channels have no yank. A GitHub, GitLab or Codeberg Release can be
edited to say which version replaces it, and its assets deleted if they are
themselves dangerous; the npm package can be deprecated with
`npm deprecate @vixide/vix@X.Y.Z "<message>"`; the Homebrew formulas move to
the fixed version with the next release.

## 7. Afterwards

- Record the work in `tasks.md` and close anything it left open.
- If the bug was a kind the [new-feature checklist](threat-model.md#checklist-for-a-new-feature)
  or the [Security Checklist](security-checklist.md) would have caught, add
  the item that would have.
- Delete the temporary private fork once the advisory is published.
