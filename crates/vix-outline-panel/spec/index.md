# Outline Panel

The code-outline list (symbol kind + name + line) and the panel's
selection/scroll state.

Pure data. The host scans the active buffer for declarations, builds an
[`Outline`] of [`Entry`] rows, renders the list, and jumps to a chosen
symbol's line. On open it can select the symbol nearest the cursor with
[`Outline::select_nearest`].

## Sub-specs

- [edit-outline](edit-outline/index.md)
- [outline-sidebar](outline-sidebar/index.md)
