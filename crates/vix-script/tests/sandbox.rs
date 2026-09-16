//! T134 audit: sandbox boundary regression tests.
//!
//! `crates/vix-script/spec/index.md`'s "closed by default" claim covers
//! *registered* functions (there is genuinely no file/network/process API
//! to call) — but Rhai's built-in `import` statement is a separate surface
//! entirely. Empirically verified before the fix landed: `Engine::new()`
//! installs a working `FileModuleResolver` by default, so `import "foo";`
//! actually loaded and ran a real `.rhai` file resolved against the
//! process's current working directory, printing a value the imported file
//! defined. That would let a *reviewed and trusted* project script pull in
//! and execute a second file the user never reviewed at all — undermining
//! the workspace-trust prompt's whole premise (T132: trust is granted to
//! the one script file the user was shown). `Runtime::new` now installs a
//! `DummyModuleResolver`, which makes every `import` fail unconditionally.

use vix_script::Runtime;

#[test]
fn import_is_refused_even_when_no_module_exists_by_that_name() {
    let rt = Runtime::new();
    let result = rt.load("t", "import \"anything\" as m;");
    assert!(result.is_err(), "import should always fail: {result:?}");
}

#[test]
fn no_file_network_or_process_function_is_registered() {
    let rt = Runtime::new();
    for call in [
        "open(\"/etc/passwd\")",
        "read_file(\"x\")",
        "write_file(\"x\", \"y\")",
    ] {
        let result = rt.load("t", &format!("let _ = {call};"));
        assert!(result.is_err(), "{call} should not exist: {result:?}");
    }
}

/// `eval` is intentionally left enabled — it only runs more Rhai code
/// within the same `Engine`, with the same resource limits and the same
/// (empty) set of registered host functions, so it grants no new
/// capability. This documents that expectation, not a gap: `eval` cannot
/// reach anything a normal script statement couldn't already reach.
#[test]
fn eval_runs_more_rhai_but_stays_inside_the_same_sandbox() {
    let rt = Runtime::new();
    let plain = rt.load("t1", "let x = eval(\"1 + 1\"); print(x);");
    assert!(plain.is_ok(), "eval of ordinary code is allowed: {plain:?}");
    let via_eval = rt.load("t2", "eval(\"open(\\\"/etc/passwd\\\")\");");
    assert!(
        via_eval.is_err(),
        "eval cannot reach an unregistered function either: {via_eval:?}"
    );
}
