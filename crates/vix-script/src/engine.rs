//! The Rhai `Engine`: resource limits (§ Execution model) and the v1
//! native-function API (§ API v1), plus the types that carry a call's
//! request and result across the host boundary.

#![warn(clippy::pedantic)]

use std::cell::RefCell;
use std::rc::Rc;

use rhai::{AST, Dynamic, Engine, Scope};

/// One command a script registered via `register_command` at load time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The script's own identifier for this command (unique within the
    /// script, not globally — the host namespaces it, e.g.
    /// `script:<stem>:<id>`, tasks.md T103).
    pub id: String,
    /// Palette label, shown **verbatim**: script-authored text is not routed
    /// through `t!`/`locales/app.yml`, the same as a saved macro's name.
    pub label: String,
    /// The script's `fn` name this command calls with no arguments when run.
    pub handler: String,
}

/// One key binding a script requested via `bind_key` at load time, naming a
/// command this same script already registered by [`Command::id`]. Whether
/// it actually wins the key — conflict handling against the real keymap —
/// is host wiring (tasks.md T104), not this crate's job; this only records
/// what the script asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    /// Key token in `vix-macros`' grammar (`C-`/`A-`/`S-` prefixes, e.g.
    /// `C-c`, `S-Tab`, `Enter`, `a`) — validated at registration time by
    /// [`vix_macros::decode_key`], so a malformed token is a load error
    /// (§ Error handling), not a binding that silently never fires.
    pub key_token: String,
    /// The [`Command::id`] this key should run.
    pub command_id: String,
}

/// A script that finished loading: its registrations (in registration
/// order) plus the compiled AST needed to invoke a handler later.
#[derive(Debug)]
pub struct LoadedScript {
    /// The script's file stem — its identity.
    pub stem: String,
    /// Commands registered at load time, in registration order.
    pub commands: Vec<Command>,
    /// Key bindings requested at load time, in registration order.
    pub bindings: Vec<KeyBinding>,
    ast: AST,
}

/// A script that failed to load — parsing, or the top-level
/// `register_command`/`bind_key` calls, raised an error. The script is
/// skipped; every *other* script still loads normally (§ Error handling,
/// "at load").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    /// The script's file stem.
    pub stem: String,
    /// The Rhai parse/eval error, as shown to the user.
    pub message: String,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "script '{}' failed to load: {}", self.stem, self.message)
    }
}

impl std::error::Error for LoadError {}

/// A message a handler raised via `message`/`error` (§ API v1, "Messages").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostMessage {
    /// `message(text)` — informational.
    Info(String),
    /// `error(text)` — error-styled. Still just a message: raising it does
    /// not stop the script (§ Error handling).
    Error(String),
}

/// A `prompt(message, on_submit)` request raised by a handler
/// (§ "Prompting for input"). By the time this is read back, the handler
/// call that raised it has already returned — answering the prompt is a
/// **fresh** [`Runtime::invoke`] of `on_submit`, never a resumed call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRequest {
    /// Text shown above the input field.
    pub message: String,
    /// The script `fn` to call with the entered text when the user submits.
    /// Not called at all if the prompt is cancelled.
    pub on_submit: String,
}

/// The editor state one handler call sees and can change — a snapshot in,
/// effects out. The host fills `buffer_text`/`selection_text`/
/// `current_line`/`cursor_offset` from the real editor before calling
/// [`Runtime::invoke`], then reads the `*_written` flags, `messages`, and
/// `prompt` back afterward to apply whatever the handler asked for. Every
/// text position is a **character offset** (`vix-find-panel`'s convention),
/// never a byte offset or `(line, col)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostState {
    /// Whole active-buffer text (`buffer_text()`). `set_buffer_text`
    /// replaces this field in place, so a handler that reads it back after
    /// writing sees its own write.
    pub buffer_text: String,
    /// `true` once a handler has called `set_buffer_text` — the host should
    /// apply `buffer_text` as the new whole buffer.
    pub buffer_text_written: bool,
    /// Selected text (`selection_text()`, `""` with no selection). Replaced
    /// in place by `set_selection_text`.
    pub selection_text: String,
    /// `true` once a handler has called `set_selection_text` — the host
    /// should apply `selection_text` as a selection replacement, or an
    /// insert at the cursor if there was no selection.
    pub selection_text_written: bool,
    /// The line the cursor is on (`current_line()`) — read-only in v1, no
    /// `set_current_line`.
    pub current_line: String,
    /// Cursor position as a character offset (`cursor_offset()`).
    /// `set_cursor_offset` updates this in place, clamped to
    /// `buffer_text`'s character length.
    pub cursor_offset: usize,
    /// `true` once a handler has called `set_cursor_offset`.
    pub cursor_offset_written: bool,
    /// Messages raised via `message`/`error`, in call order.
    pub messages: Vec<HostMessage>,
    /// The last `prompt` request, if the handler ended by asking for input.
    pub prompt: Option<PromptRequest>,
}

/// The outcome of one [`Runtime::invoke`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvokeOutcome {
    /// The handler ran to completion; `state` carries whatever it changed.
    Ran(HostState),
    /// The handler raised a runtime error (a Rhai type error, an unhandled
    /// script exception, a resource limit hit) — aborts just this call.
    /// **Not transactional** (§ Error handling): whatever `HostState`
    /// effects the handler made *before* the error are still in `state`.
    Error {
        /// The Rhai error message.
        message: String,
        /// Whatever the handler had already changed before the error.
        state: HostState,
    },
}

/// Load-time-only accumulator for `register_command`/`bind_key` calls,
/// reset before each [`Runtime::load`].
#[derive(Default)]
struct Registry {
    commands: Vec<Command>,
    bindings: Vec<KeyBinding>,
}

/// Register `register_command`/`bind_key` (§ API v1, "Registering
/// commands"/"Key bindings") — the only two functions that write into
/// `registry` rather than `host`.
fn register_registration_fns(engine: &mut Engine, registry: &Rc<RefCell<Registry>>) {
    {
        let r = registry.clone();
        engine.register_fn(
            "register_command",
            move |id: String, label: String, handler: String| {
                r.borrow_mut().commands.push(Command { id, label, handler });
            },
        );
    }
    let r = registry.clone();
    engine.register_fn(
        "bind_key",
        move |key_token: String, command_id: String| -> Result<(), Box<rhai::EvalAltResult>> {
            if vix_macros::decode_key(&key_token).is_none() {
                return Err(format!(
                    "bind_key: '{key_token}' is not a valid key token \
                     (expected e.g. 'C-c', 'S-Tab', 'Enter', 'a')"
                )
                .into());
            }
            r.borrow_mut().bindings.push(KeyBinding {
                key_token,
                command_id,
            });
            Ok(())
        },
    );
}

/// Register the buffer/selection/cursor functions (§ API v1, "Buffer &
/// selection") — every one reads and/or writes `host`.
fn register_buffer_fns(engine: &mut Engine, host: &Rc<RefCell<HostState>>) {
    {
        let h = host.clone();
        engine.register_fn("buffer_text", move || h.borrow().buffer_text.clone());
    }
    {
        let h = host.clone();
        engine.register_fn("set_buffer_text", move |text: String| {
            let mut s = h.borrow_mut();
            s.buffer_text = text;
            s.buffer_text_written = true;
        });
    }
    {
        let h = host.clone();
        engine.register_fn("selection_text", move || h.borrow().selection_text.clone());
    }
    {
        let h = host.clone();
        engine.register_fn("set_selection_text", move |text: String| {
            let mut s = h.borrow_mut();
            s.selection_text = text;
            s.selection_text_written = true;
        });
    }
    {
        let h = host.clone();
        engine.register_fn("current_line", move || h.borrow().current_line.clone());
    }
    {
        let h = host.clone();
        engine.register_fn("cursor_offset", move || {
            i64::try_from(h.borrow().cursor_offset).unwrap_or(i64::MAX)
        });
    }
    let h = host.clone();
    engine.register_fn("set_cursor_offset", move |n: i64| {
        let mut s = h.borrow_mut();
        let len = i64::try_from(s.buffer_text.chars().count()).unwrap_or(i64::MAX);
        s.cursor_offset = n.clamp(0, len).try_into().unwrap_or(0);
        s.cursor_offset_written = true;
    });
}

/// Register `prompt`/`message`/`error` (§ API v1, "Prompting for input" and
/// "Messages") — all three just append to `host`, never read it.
fn register_prompt_and_message_fns(engine: &mut Engine, host: &Rc<RefCell<HostState>>) {
    {
        let h = host.clone();
        engine.register_fn("prompt", move |message: String, on_submit: String| {
            h.borrow_mut().prompt = Some(PromptRequest { message, on_submit });
        });
    }
    {
        let h = host.clone();
        engine.register_fn("message", move |text: String| {
            h.borrow_mut().messages.push(HostMessage::Info(text));
        });
    }
    let h = host.clone();
    engine.register_fn("error", move |text: String| {
        h.borrow_mut().messages.push(HostMessage::Error(text));
    });
}

/// The Rhai engine, with v1's API (§ API v1) registered and resource limits
/// set (§ Execution model). Building the `Engine` — and registering every
/// native function once — is the expensive part; one `Runtime` loads and
/// invokes every script for the process's lifetime.
pub struct Runtime {
    engine: Engine,
    registry: Rc<RefCell<Registry>>,
    host: Rc<RefCell<HostState>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Build a new runtime: a fresh Rhai `Engine` with resource limits
    /// (§ Execution model — generous enough that no reasonable script
    /// notices them, but deterministic: an infinite loop is caught by
    /// operation count, never a wall-clock timer or a second thread) and
    /// every v1 native function (§ API v1) registered.
    #[must_use]
    pub fn new() -> Self {
        let registry = Rc::new(RefCell::new(Registry::default()));
        let host = Rc::new(RefCell::new(HostState::default()));
        let mut engine = Engine::new();

        engine.set_max_operations(10_000_000);
        engine.set_max_expr_depths(64, 64);
        engine.set_max_string_size(1_000_000);
        engine.set_max_array_size(100_000);
        engine.set_max_map_size(100_000);

        register_registration_fns(&mut engine, &registry);
        register_buffer_fns(&mut engine, &host);
        register_prompt_and_message_fns(&mut engine, &host);

        Self {
            engine,
            registry,
            host,
        }
    }

    /// Compile `source` and run its top level (§ Execution model — "the top
    /// level ... should do nothing but call `register_command`/`bind_key`"),
    /// returning the resulting [`LoadedScript`] or a [`LoadError`] if it
    /// failed to parse or its top level raised an error
    /// (§ Error handling, "at load").
    ///
    /// # Errors
    ///
    /// Returns `Err` if `source` fails to parse, or its top-level statements
    /// raise a Rhai error (including an invalid `bind_key` token).
    pub fn load(&self, stem: &str, source: &str) -> Result<LoadedScript, LoadError> {
        *self.registry.borrow_mut() = Registry::default();
        let ast = self.engine.compile(source).map_err(|e| LoadError {
            stem: stem.to_string(),
            message: e.to_string(),
        })?;
        let _: Dynamic = self.engine.eval_ast(&ast).map_err(|e| LoadError {
            stem: stem.to_string(),
            message: e.to_string(),
        })?;
        let mut registry = self.registry.borrow_mut();
        Ok(LoadedScript {
            stem: stem.to_string(),
            commands: std::mem::take(&mut registry.commands),
            bindings: std::mem::take(&mut registry.bindings),
            ast,
        })
    }

    /// Call `handler` in `script`'s AST with `args`, seeded with `state`
    /// (the host's current buffer/selection/cursor snapshot). A Rhai
    /// runtime error never reaches Rust as a panic (§ Error handling) — it
    /// becomes [`InvokeOutcome::Error`], carrying whatever `state` the
    /// handler had already changed before it failed.
    pub fn invoke(
        &self,
        script: &LoadedScript,
        handler: &str,
        args: Vec<Dynamic>,
        state: HostState,
    ) -> InvokeOutcome {
        *self.host.borrow_mut() = state;
        let mut scope = Scope::new();
        let result = self
            .engine
            .call_fn::<Dynamic>(&mut scope, &script.ast, handler, args);
        let state = self.host.borrow().clone();
        match result {
            Ok(_) => InvokeOutcome::Ran(state),
            Err(e) => InvokeOutcome::Error {
                message: e.to_string(),
                state,
            },
        }
    }
}
