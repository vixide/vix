# Community

Give Vix users one place to talk, one place to report bugs, and a clear way
in for new contributors (T747).

## Channels

- **GitHub Discussions** on `vixide/vix` is the community home. Its six
  categories each have one job: Announcements (maintainers' news), General
  (introductions and anything else), Ideas (features talked through before
  code), Polls (votes on open choices), Q&A (questions, with an accepted
  answer), and Show and tell (what people have made).
- **Issues** are for bugs and accepted work only. Questions go to Q&A, and
  undecided ideas go to Ideas.
- **Security problems** go through [`SECURITY.md`](../../SECURITY.md), never a
  public post.
- No Matrix room or chat server for now. Discussions is searchable and keeps
  answers, which a chat room doesn't. A Matrix room can be added later if
  people ask for one.

## As implemented

- **Discussions** is enabled on GitHub with the six default categories. The
  README links to it from a badge, its header links, and the Contributing
  section, and the site links to it from its footer.
- **First posts.** A ready-to-paste first post for each category lives in
  [`locales/en-us/communications/github-discussions/`](../../locales/en-us/communications/github-discussions/),
  one `.txt` file per category: the title on the first line, then a blank
  line, then the Markdown body. Placeholders marked "to fill in" are for the
  maintainer.
- **Issue forms** in [`.github/ISSUE_TEMPLATE/`](../../.github/ISSUE_TEMPLATE/):
  - `bug_report.yml` asks for what happened, steps to reproduce, the
    expected result, `vix --version`, the operating system and terminal, and
    optionally the keymap and `vix --doctor` output. It adds the `bug` label.
  - `feature_request.yml` asks for the problem first, then the proposal,
    alternatives, and trade-offs. It adds the `enhancement` label.
  - `config.yml` keeps blank issues allowed and adds three links on the
    new-issue page: Q&A for questions, Ideas for undecided features, and
    `SECURITY.md` for security problems.
- **GitHub only.** GitLab and Codeberg mirror the repository but take no issue
  traffic, so they get no templates.
- **`make test-github-discussions`** (`bin/test-github-discussions`) checks
  that Discussions stays enabled with at least one category. `make test` runs
  it, and it's skipped without a signed-in `gh`.
