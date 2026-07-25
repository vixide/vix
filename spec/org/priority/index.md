# org-priority

Prioritizing can be done by placing a priority cookie into the headline
of a TODO item right after the TODO keyword, like this:

```org
* TODO [#1] Call a friend
```

We prefer prioritizing by priority rank number 0-9 because
these are all single-digits thus align well into columns.

We prefer default priority 0 which is highest which means doing now.
This makes it easy to track what you're currently working on.

The key gotcha: use character literals (?1), not integers (1).
Org priorities are characters, and "highest" means the one that sorts
first (lowest char code). For 0–9, that's 0 = highest, 9 = lowest.

Implementation:

```elisp
(setq org-priority-highest ?0
      org-priority-lowest  ?9
      org-priority-default ?0)
```

If you use a capture template:

```txt
"* TODO [#%^{Priority|0|0|1|2|3|4|5|6|7|8|9}] %^{Task} %^g\n"
```

Example capture template:

```elisp
"* TODO %^{Task}"                                  ; headline, prompts for title
" %^{Priority|0|0|1|2|3|4|5|6|7|8|9}"              ; optional #A/#B/#C via [#…] below
" %^g\n"                                           ; tags (completes against existing)
"SCHEDULED: %^{Scheduled}t DEADLINE: %^{Deadline}t\n"
":properties:\n"
":created:  %U\n"                                  ; inactive timestamp of capture
":effort:   %^{effort|0:10|0:20|0:30|1:00|2:00|3:00}\n" ; estimate for column view/agenda
":category: %^{category|inbox|work|home|out}\n"
":end:\n"
"%a\n"          
```

## Implementation

Shipped: `org::priority`/`set_priority`/`priority_up`/`priority_down`
(`crates/vix-org/src/lib.rs`), reading/writing the `[#X]` cookie right after
the TODO keyword — `org_priority_highest`/`_lowest`/`_default` settings
(defaults `'0'`/`'9'`/`'0'`, matching the preferences above), wired to
**Org → Priority Up/Down** (`org.priority.up`/`.down`; see
`crates/vix-org/spec/index.md` § Priority). Up/down clamp at the bound rather
than the elisp snippet's wraparound-through-`org-priority-highest`/`-lowest`
Emacs default cycle — clamping is simpler and was judged less surprising than
a silent wrap.

The `%^{Priority|0|0|1|2|3|4|5|6|7|8|9}` capture-template idiom above works
via `vix-org-capture`'s `%^{Label|choices}` parsing (`spec/org/capture/index.md`):
the full `|`-delimited list is parsed (`FieldPrompt::choices`), and the first
choice pre-fills the prompt. The rest of that example template — `%^g`
tags, effort/category field prompts, `%a` annotation — all map onto
placeholders `vix-org-capture` already supports, **except** the `t` suffix on
`%^{Scheduled}t`/`%^{Deadline}t` (Org's date-picker variant of the field
prompt, prompting via its calendar rather than free text): `vix-org-capture`
has no notion of that suffix, so it is not consumed as part of the
placeholder — it is emitted as a literal trailing `t` character after the
field's answer. A template using this idiom needs `%^{Scheduled}` (plain text
entry, no calendar) and to drop the trailing `t`, or accept the stray
character. `SCHEDULED:`/`DEADLINE:` planning-line insertion is otherwise
plain template text — `"SCHEDULED: %^{Scheduled}\nDEADLINE: %^{Deadline}\n"`
works today.
