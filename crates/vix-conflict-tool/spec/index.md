# Conflict Tool

Parse Git merge-conflict markers and resolve the conflict under the cursor.

A conflict block looks like:
```text
<<<<<<< HEAD
our lines
=======
their lines
>>>>>>> other-branch
```
[`find`] locates the block containing a given line; the host then replaces
that line range with the chosen side (ours / theirs / both) via
[`Resolution`].
