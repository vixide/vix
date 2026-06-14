# Modelines

Modelines are special comments at the beginning or end of a file that configure editor settings for that specific file. Vix supports both Vim and Emacs modeline formats, allowing you to specify settings like tab size, indentation style, and file type directly within your files.

Configuration

Use the modeline_lines setting to control how many lines Vix searches for modelines:

```json
{
  "modeline_lines": 5
}
```

Set to 0 to disable modeline parsing entirely.

## Emacs

Vix has some compatibility support for Emacs file variables.

Example:

```txt
# -*- mode: python; tab-width: 4; indent-tabs-mode: nil; -*-
```

Supported Emacs Variables

| Variable | Description | Vix Setting
|-|-|-|
| mode | Major mode/language | Language detection |
| tab-width | Tab display width | tab_size |
| fill-column | Line wrap column | preferred_line_length |
| indent-tabs-mode | nil for spaces, t for tabs | hard_tabs |
| electric-indent-mode | Auto-indentation | auto_indent |
| require-final-newline | Ensure final newline | ensure_final_newline_on_save |
| show-trailing-whitespace | Show trailing whitespace | show_whitespaces |

## Vim

Vix has some compatibility support for Vim modeline.

Example:

```txt
# vim: set ft=python ts=4 sw=4 et:
```

Supported Vim Options

| Option | Aliases | Description | Vix Setting
|-|-|-|-|
| filetype | ft | File type/language | Language detection
| tabstop | ts | Number of spaces a tab counts for | tab_size
| textwidth | tw | Maximum line width | preferred_line_length
| expandtab | et | Use spaces instead of tabs | hard_tabs
| noexpandtab | noet | Use tabs instead of spaces | hard_tabs
| autoindent | ai | Enable auto-indentation | auto_indent
| noautoindent | noai | Disable auto-indentation | auto_indent
| endofline | eol | Ensure final newline | ensure_final_newline_on_save
| noendofline | noeol | Disable final newline | ensure_final_newline_on_save

## Notes

The first kilobyte of a file is searched for modelines.

Emacs modelines take precedence over Vim modelines when both are present.

Modelines in the first few lines take precedence over those at the end of the file.
