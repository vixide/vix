# Roam

Org-roam: networked, Zettelkasten-style note-taking over a directory of Org
files (<https://www.orgroam.com/>).

A *node* is an `.org` file carrying an `:ID:` property and a `#+title:`. Nodes
link to one another with `[[id:<id>][Title]]` links; the set of nodes and
links forms a graph. This module is the pure, testable core: parsing a node's
title/id, building new node and daily-note skeletons, editing the file-level
property drawer and `#+filetags:`, and compiling cross-node views (backlinks,
a Mermaid graph, and a node index). The host (`app`) wires these to the
Org → Roam menu, prompting for input and reading/writing the files.

All functions are pure so they can be unit-tested without a live editor or a
filesystem.

## Sub-specs

- [dailies-calendar](dailies-calendar/index.md)
- [live-backlinks](live-backlinks/index.md)
- [wiki-link-completion](wiki-link-completion/index.md)
