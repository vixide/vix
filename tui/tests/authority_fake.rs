//! T710/T711: the App's explorer, open and save work against a
//! [`FakeAuthority`] whose paths don't exist on this machine.
//!
//! The authority slot is process-wide, so this lives in its own test binary
//! and runs as one `#[test]`: installing the fake here can't leak into any
//! other test.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ratatui::{Terminal, backend::TestBackend};
use vix::app::App;
use vix::settings::Settings;
use vix_authority::{Authority, FakeAuthority};

fn screen(app: &mut App) -> String {
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    let buf = term.backend().buffer();
    (0..30)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn explorer_open_and_save_go_through_an_installed_fake_authority() {
    let scratch = std::env::temp_dir().join(format!("vix-authority-app-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let fake = Arc::new(FakeAuthority::new(scratch.join("remote")).unwrap());
    let backing = fake.backing().to_path_buf();
    std::fs::create_dir_all(backing.join("proj/src")).unwrap();
    std::fs::write(backing.join("proj/src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(backing.join("proj/zz-remote-only.txt"), "x\n").unwrap();
    let previous = vix_authority::install(Arc::<FakeAuthority>::clone(&fake));

    // `/proj` exists only inside the fake: everything below must reach it
    // through the authority.
    let root = PathBuf::from("/proj");
    assert!(!root.join("zz-remote-only.txt").exists());
    let mut app =
        App::new(root.clone(), Settings::default()).with_session_path(scratch.join("session.toml"));

    // Explorer: the tree lists the fake's directory.
    let shown = screen(&mut app);
    assert!(shown.contains("zz-remote-only.txt"), "{shown}");
    assert!(
        fake.calls()
            .iter()
            .any(|c| c.op == "read_dir" && c.path == root)
    );

    // Open: the buffer holds the fake file's text.
    let main = root.join("src/main.rs");
    app.open_initial(&main);
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.path.as_deref(), Some(main.as_path()));
    assert_eq!(tab.text(), "fn main() {}\n");

    // Save: the edit lands in the backing file, atomically.
    fake.clear_calls();
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    assert!(app.editor.insert_str("// remote\n", area));
    app.run_action("file.save");
    assert_eq!(
        std::fs::read_to_string(backing.join("proj/src/main.rs")).unwrap(),
        "// remote\nfn main() {}\n"
    );
    assert!(
        fake.calls()
            .iter()
            .any(|c| c.op == "write_atomic" && c.path == main)
    );
    assert!(fake.exists(Path::new("/proj/src/main.rs")));

    vix_authority::install(previous);
    let _ = std::fs::remove_dir_all(&scratch);
}
