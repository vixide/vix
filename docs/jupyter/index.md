# Jupyter kernels

Run code from the editor in a [Jupyter](https://jupyter.org/) kernel and see
what comes back in the bottom dock, in an Org file's `#+RESULTS:`, or in an
image tab, without leaving Vix. Any kernel works:
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

### Example: a Python file in cells

```python
import math

# %% Set up
radius = 2

# %% Area
math.pi * radius ** 2
```

Put the cursor on the first line and choose **Run and Advance** three
times. The first time, Vix starts the Python kernel and sends `import math`
(the lines above the first marker are a cell of their own), then moves the
cursor to the `# %% Set up` line. The second time it sends `radius = 2`,
and the third time the "Area" cell: the dock shows `12.566370614359172`,
and the same value appears at the end of the cell's last line.

## Output

Output goes to the bottom dock, under a `[*] <kernel>` line that quotes the
code's first line:

- printed text (stdout and stderr) and the value of the last expression, as
  plain text;
- an error as `Name: message` and its traceback, with the colors removed;
- a value with several forms uses the plain-text form. When a richer form
  such as HTML was dropped, the dock says so;
- an image (`image/png` or `image/jpeg`, such as a Matplotlib figure) opens
  in an image tab, and the dock says so and shows the image's text form.
  Without the `image` feature, or in a terminal without graphics, the dock
  says the image cannot be shown here.

When the kernel finishes, the dock adds `[n] <kernel>: done` (or `failed`,
or `not run` when an earlier cell failed), where `n` is the execution count.
The first line of the last output also shows at the end of the line that
ran. **Clear Results** removes it.

## Org source blocks

In an Org file, put the cursor anywhere in a source block and press
`C-c C-c`, or choose **Run → Jupyter → Run**. Vix runs the block in the
kernel for its language and writes the result under it, the way Emacs's
`ob-jupyter` does:

```org
#+begin_src python
total = sum(range(10))
print("adding")
total
#+end_src

#+RESULTS:
: adding
: 45
```

Running the block again replaces the results. The write is one undo step,
so `Ctrl+Z` takes it back. Long results (10 lines or more) go in a
`#+begin_example` block, as in Org. **Clear Results** with the cursor in the
block removes its `#+RESULTS:`. Blocks share their kernel, so `total` is
there for the next Python block.

The block's language picks the kernel: `python` and `jupyter-python` both
mean Python. These header arguments on the `#+begin_src` line are honored:

| Header argument | Effect |
| --------------- | ------ |
| `:results value` | The default: everything the block shows, in order — printed text, values and errors, as in a notebook cell. |
| `:results output` | Only printed text (stdout and stderr) and errors. |
| `:results replace` | The default: new results replace the old ones. |
| `:results append` | New results go after the old ones. |
| `:results silent` | Nothing is written to the file; the output goes to the dock and the end-of-line hint. |
| `:file plot.png` | Save the block's image there, relative to the Org file's folder, and write a link to it. |
| `:eval no` or `:eval never` | Never run the block. |
| `:eval query` | Ask before every run. |

Words can be combined, as in `:results output append`. Other header
arguments (`:session`, `:exports`, `:var`, …) are ignored, and so are
`#+header:` lines and `header-args` properties.

### Example: a plot

```org
#+begin_src python :file sine.png
import matplotlib.pyplot as plt
import numpy as np
x = np.linspace(0, 6.3, 100)
plt.plot(x, np.sin(x))
plt.show()
#+end_src

#+RESULTS:
[[file:sine.png]]
```

The figure is saved as `sine.png` next to the Org file. Without `:file`, an
image is saved as `<org-file-name>-images/<hash>.png` (for `notes.org`,
`notes-images/…`). Vix refuses a `:file` that is an absolute path or climbs
out of the Org file's folder with `..`; then it writes the image's text form
instead.

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
  run of each Org source block in a session asks for confirmation, showing
  its language and first lines, like Emacs's `org-confirm-babel-evaluate`:
  `y` runs it, `n` cancels, and `a` runs it and trusts the workspace for
  Org blocks from now on ("always in this workspace", remembered across
  sessions). A block in a file outside the workspace always asks, and does
  not offer `a`. A selection, cell or line you send yourself is never
  prompted. The `jupyter_confirm_evaluate` setting changes this (below).
- **Kernels in the project need trust.** A kernelspec inside the workspace
  (such as one in its `.venv`) could be a program the repository's author
  chose, so Vix uses it only once you trust the workspace (scripts trust,
  or `a` above). Then `.venv/share/jupyter/kernels` and
  `venv/share/jupyter/kernels` are searched too.
- **Images are checked.** Vix refuses an image over 16 MB, over 16,384
  pixels on a side, over 16 megapixels, or that is not the type it claims,
  before decoding it.
- **Remote workspaces are refused.** A kernel runs on this computer, so Vix
  does not start one while the workspace is on another machine.
- **A private connection.** Each kernel gets its own `0700` directory and a
  `0600` connection file with a random key; on Unix the kernel listens on
  Unix sockets in that directory, so no TCP port is opened. Every message is
  signed and checked. Kernels cannot ask for input (`input()` raises an
  error instead of waiting).

## Settings

```toml
# When an Org block asks before it runs: "untrusted" (the default) asks
# for a block's first run unless the workspace is trusted, "always" asks
# every time, "never" never asks. `:eval query` and `:eval never` in a
# block still win.
jupyter_confirm_evaluate = "untrusted"

# Which kernelspec runs each language.
[jupyter_kernels]
python = "python3"
rust = "rust"
```

## Not yet

Remote kernels, `input()`, widgets, HTML and SVG output, updating a display
in place, kernel completion, `.ipynb` editing, `#+header:` lines and
`header-args` properties, and Org's table and raw result formats are out of
scope for now. The design is in [`spec/jupyter`](../../spec/jupyter/index.md).
