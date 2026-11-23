# Tutorial 13: Scripting with Rhai

Vix compiles every built-in command into the binary. When you want one it
doesn't have, you can write it yourself as a small [Rhai](https://rhai.rs/)
script: drop a `.rhai` file on disk, reload, and you have a new command
palette entry and, if you want one, a key binding. No rebuild.

This tutorial builds three small scripts: a line sorter with a key
binding, a dated-TODO inserter that prompts for text, and a project-only
ticket linker. Along the way it covers project scripts and workspace trust,
plugins, the sandbox, and reading a script error.

Open the demo workspace so you have files to try the scripts on:

```sh
cd tui/examples/demo-workspace
vix .
```

## Where scripts live

Vix loads `*.rhai` files directly inside two directories (not their
subdirectories): global scripts from `<config dir>/scripts/`, for every
workspace, and project scripts from `<workspace root>/.vix/scripts/`.
`<config dir>` is the directory that holds `config.toml`:

| Platform | Global scripts directory |
| -------- | ------------------------ |
| Linux | `~/.config/vix/scripts/` |
| macOS | `~/Library/Application Support/rs.vix/scripts/` |
| Windows | `%APPDATA%\vix\config\scripts\` |

The directory doesn't exist until you create it. A script is known by its
file stem: `sort-lines.rhai` is the script `sort-lines`.

## Step 1: a first script

Create the global scripts directory and a file named `sort-lines.rhai` in
it (in Vix, `Ctrl+N` opens a new buffer and `Ctrl+Shift+S` saves it under a
name). Start with this:

```rhai
// Sort the selected lines, A to Z.

register_command("sort_lines", "Sort Selected Lines", "on_sort_lines");

fn on_sort_lines() {
    let text = selection_text();
    let lines = text.split("\n");
    lines.sort();
    let result = "";
    for (line, i) in lines {
        if i > 0 {
            result += "\n";
        }
        result += line;
    }
    set_selection_text(result);
}
```

Two parts:

- The **top level** runs once, when the script loads. It should only
  register things. `register_command(id, label, handler)` adds a command:
  `id` only has to be unique inside this file, `label` is what you see in
  menus and the palette, and `handler` names a `fn` in the same file.
- The **handler** runs each time you invoke the command, as a fresh call
  with no arguments. `selection_text()` returns the selected text (`""` if
  nothing is selected) and `set_selection_text(text)` replaces it. The
  rest is plain Rhai: `split`, `sort`, and a loop that joins the lines
  back together.

Now load it: **Tools → Scripts → Reload**. The message drawer (the right
dock; **View → Layout → Right Dock** shows it) reports
`Reloaded scripts: 1 command(s) available`.

Try it. Open `notes.md`, type three lines such as `pear`, `apple`, `fig`,
select them, then run the command one of two ways:

- **Tools → Scripts → Run…** opens a chooser listing every script
  command. `↑`/`↓` to pick, `Enter` to run, `Esc` to close.
- The command palette: `Ctrl+P`, type `>sort`, and pick
  **Sort Selected Lines**. Script commands sit alongside the built-in ones.

The three lines come back as `apple`, `fig`, `pear`.

## Step 2: a key binding, a guard, and a message

The first version has rough edges. With nothing selected it quietly does
nothing, and if your selection ends with a newline, the empty last "line"
sorts to the top. Replace the file with this:

```rhai
// Sort the selected lines, A to Z or Z to A.

register_command("sort_lines", "Sort Selected Lines", "on_sort_lines");
register_command("sort_lines_desc", "Sort Selected Lines (Descending)", "on_sort_lines_desc");
bind_key("C-S-u", "sort_lines");

fn on_sort_lines() {
    sort_selection(false);
}

fn on_sort_lines_desc() {
    sort_selection(true);
}

fn sort_selection(descending) {
    let text = selection_text();
    if text == "" {
        error("Select some lines first, then run Sort Selected Lines.");
        return;
    }
    // Keep a trailing newline out of the sort, then put it back.
    let trailing = text.ends_with("\n");
    if trailing {
        text.pop();
    }
    let lines = text.split("\n");
    lines.sort();
    if descending {
        lines.reverse();
    }
    let result = "";
    for (line, i) in lines {
        if i > 0 {
            result += "\n";
        }
        result += line;
    }
    if trailing {
        result += "\n";
    }
    set_selection_text(result);
    message("Sorted " + lines.len() + " lines.");
}
```

What's new:

- **Two commands, one helper.** Both handlers call an ordinary Rhai
  function, `sort_selection`, with an argument. Only handlers need to be
  registered; other functions in the file are just functions.
- **`bind_key(key_token, command_id)`** binds `Ctrl+Shift+U` to the
  `sort_lines` command from this same file. Key tokens use the macro
  grammar: `C-`, `A-` and `S-` prefixes plus a key name, such as `C-c`,
  `S-Tab`, `C-S-l`, or `Enter`.
- **`error(text)`** and **`message(text)`** write to the message drawer,
  styled as an error or as information. `error` is only a message: it
  doesn't stop the script, which is why the `return` follows it.
- **Write once.** The handler builds the whole result in a local variable
  and calls `set_selection_text` once at the end. If a handler fails
  halfway, whatever it already wrote stays written, so a single write at
  the end is the safe shape.

Run **Tools → Scripts → Reload** again. Select some lines and press
`Ctrl+Shift+U`; the drawer says `Sorted 3 lines.` Run it with nothing
selected and you get the error message instead.

If your key collides with something, Vix tells you in the drawer. A
script binding that takes a built-in's key wins, and is reported once. Two
claims on the same key (two scripts, or a script and your
`keybindings.toml`) are both rejected.

## Step 3: asking for input

The second script prompts for text and stamps it with today's date.
Create `todo-note.rhai` next to `sort-lines.rhai`:

```rhai
// Insert a dated TODO at the cursor: "TODO(2026-11-17): call the printer people".

register_command("todo_note", "Insert Dated TODO", "on_todo_note");

fn on_todo_note() {
    prompt("TODO text:", "on_todo_note_text");
}

fn on_todo_note_text(text) {
    if text == "" {
        message("Cancelled: nothing entered.");
        return;
    }
    set_selection_text("TODO(" + now() + "): " + text);
}
```

- **`prompt(message, on_submit)`** opens a one-line input box. When you
  press `Enter`, Vix calls `on_submit` with what you typed as its only
  argument. That is a **new** call, not a continuation of `on_todo_note`,
  so local variables from the first handler are gone; if the second one
  needs the selection, it calls `selection_text()` again. `Esc` cancels,
  and `on_submit` is never called.
- **`now()`** returns today's local date as `YYYY-MM-DD`. It's the only
  clock function; there is no time of day.
- With no selection, `set_selection_text` inserts at the cursor.

Reload, put the cursor on an empty line, run **Insert Dated TODO** from the
palette, type `call the printer people`, and press `Enter`.

## The script API at a glance

These are all the functions Vix gives a script. Positions are character
offsets, never bytes and never line/column pairs.

| Function | What it does |
| -------- | ------------ |
| `register_command(id, label, handler)` | add a command (top level only) |
| `bind_key(key_token, command_id)` | bind a key to one of this script's commands |
| `buffer_text()` | the whole active buffer |
| `set_buffer_text(text)` | replace the whole active buffer |
| `selection_text()` | the selection, or `""` |
| `set_selection_text(text)` | replace the selection, or insert at the cursor |
| `current_line()` | the cursor's line (read-only) |
| `cursor_offset()` | the cursor position |
| `set_cursor_offset(n)` | move the cursor; an offset outside the buffer is an error |
| `prompt(message, on_submit)` | ask for one line of text |
| `message(text)`, `error(text)` | write to the message drawer |
| `now()` | today's date, `YYYY-MM-DD` |
| `tr(key)`, `tr(key, args)`, `locale()` | plugin translations and the UI language |

On a read-only buffer, the two `set_…_text` functions are blocked and the
drawer reports `Buffer is read-only`; moving the cursor still works.

## Step 4: a project script and workspace trust

Some scripts only make sense for one project. The third script turns a
selected ticket ID such as `ABC-123` into a Markdown link to that
project's tracker, so it belongs in the project, not in your global
directory. In the demo workspace, create `.vix/scripts/ticket-link.rhai`:

```rhai
// Turn a selected ticket ID such as ABC-123 into a Markdown link to this
// project's tracker.

register_command("ticket_link", "Link Selected Ticket", "on_ticket_link");

fn on_ticket_link() {
    let id = selection_text();
    id.trim();
    if id == "" {
        error("Select a ticket ID such as ABC-123 first.");
        return;
    }
    let label = "[" + id + "]";
    let url = "https://tracker.example.com/browse/" + id;
    set_selection_text(label + "(" + url + ")");
}
```

(Rhai's `trim()` changes the string in place, so it's called on its own
line rather than assigned.)

Now run **Tools → Scripts → Reload**. Instead of loading the new script
right away, Vix shows a **Trust Workspace Scripts?** dialog (unless you
trusted this workspace before, in which case the script just loads):

> This workspace has 1 script(s) under .vix/scripts/ that will run with
> read/write access to the active buffer (no file or network access
> otherwise). Trust this workspace and run them?

This exists because anyone can put a `.vix/scripts/` directory in a
repository. Without the prompt, cloning a stranger's repo and opening it
would run their scripts. Your global scripts never ask; they're yours.

- Press `y` to trust the workspace. The project scripts load and the
  status bar says `Workspace scripts trusted`.
- Press `n` or `Esc` to decline. Nothing from `.vix/scripts/` loads.
- Every other key, including `Enter`, is ignored, and so is anything you
  press in the first quarter-second after the dialog opens, so a key
  typed ahead can't answer it by accident.

The answer is saved for this workspace. Vix asks at startup only if you
have never answered. A "no" is not permanent: **Tools → Scripts →
Reload** asks again. The same decision also covers the workspace's
`.vix/settings.toml` and project environment, if it has them; when it
does, the dialog says so.

After trusting, select `ABC-123` in `notes.md` and run **Link Selected
Ticket**. In any other workspace, the command doesn't exist.

If a project script and a global script have the same file stem, the
project one wins and the global one doesn't load at all.

## Step 5: share a script as a plugin

A plugin is a git repository with `.rhai` scripts at its top level. To try
it, turn `sort-lines.rhai` into one. **Move** the file out of the global
scripts directory rather than copying it; otherwise the loose copy and the
plugin both claim `Ctrl+Shift+U` and both bindings are rejected.

```sh
mkdir -p ~/src/vix-sort
mv "<config dir>/scripts/sort-lines.rhai" ~/src/vix-sort/
cd ~/src/vix-sort
git init
git add sort-lines.rhai
git commit -m "Sort selected lines"
```

Install it. Plugin commands print a report and exit without opening the
editor:

```sh
vix --plugin install file://$HOME/src/vix-sort
```

Vix clones the repository with your own `git` (so your credentials and SSH
agent work as usual) into `<config dir>/scripts/vix-sort/`. The name is
the URL's last path segment, minus any `.git`. Append `@<rev>` to pin a
branch, tag or commit, as in `https://example.com/me/vix-sort.git@v1`.
The report ends with: `It stays disabled until you enable it in Vix →
Plugins.`

**A new plugin is disabled.** Check with `vix --plugin list`, which prints
each plugin's name, state, short commit and URL:

```text
vix-sort  disabled  3f1c2ab9e0d4  file:///home/you/src/vix-sort
```

Start Vix, open **Vix → Plugins…**, highlight `vix-sort`, and press
`Enter`. The status bar says `Plugin vix-sort enabled` and the scripts
reload. The command now appears in the palette as before; its id is
`script:vix-sort/sort-lines:sort_lines`, so a plugin's commands can never
collide with your loose scripts.

### The SHA-256 trust check

Enabling a plugin trusts its files exactly as they are at that moment.
Vix records a SHA-256 digest of every file under the plugin's directory
(except `.git`) in `<config dir>/plugins.lock`:

```toml
[[plugin]]
name = "vix-sort"
url = "file:///home/you/src/vix-sort"
commit = "3f1c2ab9e0d4…"
trusted = "9a0e…"
```

Every time scripts load, Vix recomputes the digest and loads the plugin
only if it still matches:

- Edit `<config dir>/scripts/vix-sort/sort-lines.rhai` by hand and reload:
  **Vix → Plugins…** shows the plugin as `changed since enabled`, and it
  stops loading until you press `Enter` on it again.
- `vix --plugin update` (or `vix --plugin update vix-sort`) fetches new
  commits. If any file changed, the plugin is disabled again, with
  `vix-sort: its code changed, so it is disabled until you enable it again
  in Vix → Plugins.` An update that changes nothing keeps it enabled.
- Disabling a plugin (`Enter` on an enabled one) or
  `vix --plugin remove vix-sort` forgets the trust; reinstalling starts
  disabled.
- A plugin with a symlink pointing outside its own directory is refused at
  install, and an update that adds one is undone.

A plugin gets nothing a loose script doesn't. It runs in the same sandbox.

## The sandbox

The functions in the table above are everything a script can reach.

- **No files, network or processes.** Rhai's standard library has none of
  these, and Vix registers none. `import` is switched off too, so a
  trusted script can't pull in another file you never saw.
- **Only the active buffer.** A script can't list tabs, open a buffer, or
  read any other file. To fill a new buffer, open it first (`Ctrl+N`), then
  run the script.
- **No sharing between scripts.** Each script has its own state, and
  scripts can't call each other.
- **Resource limits.** Each call is capped at 10,000,000 operations, with
  limits on nesting depth and on string, array and map sizes. An infinite
  loop stops with `Too many operations` instead of freezing the editor.

## Step 6: debugging a script error

A broken script never crashes Vix. Errors happen at one of two moments,
and both land in the message drawer.

**At load time.** A syntax error, or an error in the top-level code,
skips that one script; every other script still loads. Break
`todo-note.rhai` by deleting the `)` after `fn on_todo_note(` and reload:

```text
Script 'todo-note' failed to load: Expecting ')' to close the parameters list of function 'on_todo_note' (line 5, position 18)
```

A bad key token is a load error too, not a binding that silently never
fires. `bind_key("Ctrl-u", "sort_lines")` gives:

```text
Script 'sort-lines' failed to load: Runtime error: bind_key: 'Ctrl-u' is not a valid key token (expected e.g. 'C-c', 'S-Tab', 'Enter', 'a') (line 5, position 1)
```

**At run time.** Rhai only looks up function names when the code runs, so
a typo inside a handler loads fine and fails when you invoke the command.
Change `selection_text()` to `selection_txt()` on line 16 of the Step 2
`sort-lines.rhai`, reload, and press `Ctrl+Shift+U`:

```text
Script 'sort-lines' error: Function not found: selection_txt () (line 16, position 16)
in call to function 'sort_selection' (line 8, position 5)
```

Read it from the top: what went wrong and where, then the call chain that
led there (`on_sort_lines` on line 8 called `sort_selection`). A run-time
error stops only that one call. It is not rolled back, which is another
reason to write the buffer once, at the end.

To inspect a value while you debug, show it with `message(...)`. Saving
a script doesn't reload it, so reload after every edit; if a command is
missing afterwards, look in the drawer for a load error first.

## Where to go next

- [`docs/scripting/index.md`](../../scripting/index.md) — the scripting
  reference, including translating a plugin with `tr` and `locales/`.
- [`tui/examples/scripts/`](../../../tui/examples/scripts/) — six working
  sample scripts (Markdown links, a file header, title case, de-duplicating
  lines, a date signature, a scratch template), each tested.
- [`crates/vix-script/spec/index.md`](../../../crates/vix-script/spec/index.md)
  — the full design: discovery, the execution model, plugins and the trust
  digest.
- [`docs/keybindings/index.md`](../../keybindings/index.md) — how script
  bindings are resolved against `keybindings.toml` and the built-in
  keymaps.
- [Tutorial 9: Make Vix Yours](../09-make-vix-yours/index.md) — themes,
  keymaps, snippets and settings, the other half of customizing Vix.

**Previous:** [Tutorial 12 — Remote Editing](../12-remote-editing/index.md)  
**Next:** [Tutorial 14 — AI and Agents](../14-ai-and-agents/index.md)

---

Vix™ and Vix IDE™ are trademarks.
