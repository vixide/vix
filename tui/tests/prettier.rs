//! T683: Format Document and format-on-save through the user's own Prettier,
//! driven by a fake `prettier` shell script in a temp project (no Node).

#![cfg(all(unix, feature = "tasks"))]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use vix::app::App;
use vix::settings::Settings;

/// A temp workspace with a fake project-local prettier that prefixes the
/// buffer with a marker line, and a `.ts` file.
fn project(tag: &str, trusted: bool) -> std::io::Result<(App, PathBuf)> {
    let root = std::env::temp_dir().join(format!("vix-prettier-app-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let bin_dir = root.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin_dir)?;
    let root = root.canonicalize()?;
    let bin = root.join("node_modules").join(".bin").join("prettier");
    std::fs::write(&bin, "#!/bin/sh\necho '// FAKE-PRETTIER'\ncat\n")?;
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))?;
    std::fs::write(root.join("a.ts"), "let a=1\n")?;
    std::fs::write(root.join("b.rs"), "fn b(){}\n")?;

    let session = root.join("session.toml");
    if trusted {
        let mut s = vix_session::Session::default();
        s.set_scripts_trusted(&root.to_string_lossy(), Some(true));
        s.save_to(&session).map_err(std::io::Error::other)?;
    }
    let mut settings = Settings::default();
    settings.save.prettier_languages = vec!["typescript".into()];
    let mut app = App::new(root.clone(), settings).with_session_path(session);
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);
    app.open_initial(&root.join("a.ts"));
    Ok((app, root))
}

fn text(app: &App) -> String {
    app.editor
        .active_tab()
        .map(vix::editor::Tab::text)
        .unwrap_or_default()
}

#[test]
fn format_document_uses_project_prettier_in_a_trusted_workspace() {
    let (mut app, root) = project("fmt", true).unwrap();
    app.run_action("lsp.format");
    assert_eq!(text(&app), "// FAKE-PRETTIER\nlet a=1\n");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn untrusted_workspace_skips_the_project_local_prettier() {
    let (mut app, root) = project("untrusted", false).unwrap();
    app.run_action("lsp.format");
    assert!(
        !text(&app).contains("FAKE-PRETTIER"),
        "the project-local binary must not run"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn format_on_save_formats_and_rewrites_the_file() {
    let (mut app, root) = project("save", true).unwrap();
    app.settings.save.format_on_save = true;
    app.run_action("file.save");
    assert_eq!(
        std::fs::read_to_string(root.join("a.ts")).unwrap(),
        "// FAKE-PRETTIER\nlet a=1\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unlisted_language_is_left_alone() {
    let (mut app, root) = project("other", true).unwrap();
    app.open_initial(&root.join("b.rs"));
    app.run_action("lsp.format");
    assert_eq!(text(&app), "fn b(){}\n");
    let _ = std::fs::remove_dir_all(&root);
}
