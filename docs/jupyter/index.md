# Jupyter kernels

Run code from the editor in a [Jupyter](https://jupyter.org/) kernel and see
what comes back in the bottom dock, without leaving Vix. Any kernel works:
IPython (`ipykernel`), IRkernel, `evcxr` for Rust, Deno, and others. Vix
talks the kernel protocol itself, so it does not need Jupyter Notebook,
JupyterLab or `jupyter console`, only the kernel.

## Get a build with Jupyter

Jupyter support is off by default, so the standard build never starts a
kernel. Build Vix with the `jupyter` feature:

```sh
cargo install vix-tui --features jupyter
# or, from a checkout:
cargo run --features jupyter
```

The actions live under **Run → Jupyter** and in the command palette
(`>jupyter`).

## Install a kernel

Vix finds kernels the way Jupyter does: each kernel is a `kernel.json` in a
`kernels/<name>/` directory under `$JUPYTER_PATH`, your Jupyter data
directory (`~/Library/Jupyter` on macOS, `~/.local/share/jupyter` on Linux,
`%APPDATA%\jupyter` on Windows), or the `share/jupyter` directory of the
active virtual environment, Conda environment or the Python on `PATH`. For
Python:

```sh
pip install ipykernel
python -m ipykernel install --user
```

`jupyter kernelspec list` shows what is installed. Vix never runs `pip`
itself; with no kernel installed it says how to install one.

## Run code

Put the cursor in the code and choose **Run → Jupyter → Run**
(`Ctrl+Shift+Enter`; terminals send that key only with the kitty keyboard
protocol). Vix runs, in this order:

1. the **selection**, if there is one;
2. the **Org source block** around the cursor (`#+begin_src python` …
   `#+end_src`; `jupyter-python` also means Python);
3. the **cell** around the cursor, in a file split into cells with the
   language's comment followed by `%%` (`# %%` in Python, `// %%` in
   JavaScript or Rust), the format VS Code, Spyder and Jupytext use;
4. the **current line**.

**Run and Advance** does the same, then moves the cursor past the code it
ran, so you can step through a file cell by cell.

The first run for a language starts that language's kernel. Vix picks the
kernelspec named for the language in the `[jupyter_kernels]` setting, else
the first installed kernel for the language; with none, it shows the kernel
picker. One kernel serves each language for the whole workspace, so a
variable set from one file or block is there for the next, as in a notebook.

**Start Kernel…** opens the picker yourself. Each row shows the kernel's
name, language and the exact command Vix will run.

## Output

Output goes to the bottom dock, under a `[*] <kernel>` line that quotes the
code's first line:

- printed text (stdout and stderr) and the value of the last expression, as
  plain text;
- an error as `Name: message` and its traceback, with the colors removed;
- a value with several forms uses the plain-text form. When a richer form
  such as HTML was dropped, the dock says so;
- images are not shown yet: the dock shows a placeholder line and the
  image's text form.

When the kernel finishes, the dock adds `[n] <kernel>: done` (or `failed`,
or `not run` when an earlier cell failed), where `n` is the execution count.
The first line of the last output also shows at the end of the line that
ran. **Clear Results** removes it.

The status bar shows the kernel for the buffer's language: its name, its
state (`starting`, `idle`, `busy`, `restarting`) and the execution count.

## Control the kernel

- **Interrupt Kernel** stops the code that is running. The kernel and its
  variables stay.
- **Restart Kernel** starts the kernel again; its variables are gone.
- **Shut Down Kernel** stops it. Vix also stops every kernel when it exits.

Each acts on the kernel for the language at the cursor, else the kernel
that ran code last.

## Safety

A kernel runs code as you, with your files and your network. So:

- **Nothing runs on its own.** Vix starts a kernel and sends code only when
  you choose an action. Opening a file, restoring a session or saving never
  runs anything.
- **Org blocks ask first.** In a workspace you have not trusted, the first
  run of each Org source block asks for confirmation (`y` runs it, `n`
  cancels), showing its language and first lines, like Emacs's
  `org-confirm-babel-evaluate`. A selection, cell or line you send yourself
  is never prompted.
- **Remote workspaces are refused.** A kernel runs on this computer, so Vix
  does not start one while the workspace is on another machine.
- **A private connection.** Each kernel gets its own `0700` directory and a
  `0600` connection file with a random key; on Unix the kernel listens on
  Unix sockets in that directory, so no TCP port is opened. Every message is
  signed and checked. Kernels cannot ask for input (`input()` raises an
  error instead of waiting).

## Settings

```toml
# Which kernelspec runs each language.
[jupyter_kernels]
python = "python3"
rust = "rust"
```

## Not yet

Writing results back into an Org buffer as `#+RESULTS:`, showing images,
and `C-c C-c` in an Org block are coming. Remote kernels, widgets, kernel
completion and `.ipynb` editing are out of scope for now. The design is in
[`spec/jupyter`](../../spec/jupyter/index.md).
