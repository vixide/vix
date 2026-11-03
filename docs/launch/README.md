# Vix 2.0 launch kit

Drafts for announcing Vix 2.0. **Nothing here has been posted, sent or
submitted anywhere.** Every file is a draft for the maintainer to edit and post
by hand, or not. Posting, and every listing or submission, needs the
maintainer's explicit say-so (see T745 and T746 in `tasks.md`).

## What is here

- [Blog post](blog-post.md): the longer announcement, for the site or a personal blog.
- [Show HN](show-hn.md): title, first comment and likely questions.
- [Reddit](reddit.md): a post for r/rust and a shorter one for r/commandline.
- [Short posts](short-posts.md): Mastodon, Bluesky and chat-sized text.

The user-facing release notes are [What's new in 2.0](../whats-new-2-0/index.md),
and the site page is `/whats-new/` (SvelteKit subproject `vixide.github.io/`).
The drafts link to those; keep all three in agreement with the
[changelog](../changelog.md).

## Rules for these drafts

1. **Claim only what has landed.** Read `CHANGELOG.md` (`[Unreleased]`) and
   `tasks.md` before editing a claim. If it is experimental, opt-in or partial,
   the draft says so.
2. **Numbers come from the repository**, with their conditions
   ([Performance](../performance/index.md)). No comparisons with other editors.
3. **Never present an unpublished install channel as working.** winget, Scoop,
   AUR, `cargo binstall`, Nix, mise and the container image are not published.
4. **No outward action without approval.** Do not post, submit, email or tag
   anyone from these files.

## Before posting anything

- [ ] 2.0.0 is actually released (tag, GitHub Release, crates, Homebrew tap, npm)
      and the install commands in the drafts were run against it on a real machine.
- [ ] Replace every `[RELEASE DATE]` and `[LINK]` placeholder.
- [ ] The site's `/whats-new/` page has lost its "not released yet" wording, and
      the home and features pages say 2.0, not "coming in 2.0".
- [ ] Re-read each claim against the released changelog.
- [ ] Decide where to post and when; be present to answer questions for the
      first few hours.
