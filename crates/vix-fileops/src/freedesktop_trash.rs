//! Move to trash on Linux and the BSDs, following the
//! [freedesktop.org Trash specification](https://specifications.freedesktop.org/trash-spec/latest/).
//!
//! Vix writes the trash itself here rather than through the `trash` crate, whose
//! freedesktop backend needs `chrono` to write the spec's required
//! `DeletionDate`; this one uses `jiff`.
//!
//! A path goes to the **home trash** (`$XDG_DATA_HOME/Trash`, by default
//! `~/.local/share/Trash`) when it is on the same device, and otherwise to the
//! trash at the top of its own mount: `$topdir/.Trash/$uid` when the
//! administrator made `$topdir/.Trash` (a real directory with the sticky bit),
//! else `$topdir/.Trash-$uid`. Either way the item is renamed, never copied, so
//! trashing is as quick as a move and never crosses devices.
//!
//! Each item gets `info/<name>.trashinfo`, created exclusively *before* the move
//! so two trashings can never claim the same name, and is then renamed to
//! `files/<name>`. If the move fails, the info file is removed again.

use std::fs;
use std::io::{self, Write as _};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, PermissionsExt as _};
use std::path::{Component, Path, PathBuf};

/// How many names (`a`, `a.2`, `a.3`, …) to try before giving up.
const MAX_NAME_TRIES: u32 = 10_000;

/// Move `path` (a file, directory or symlink, which is moved itself, not its
/// target) to the trash.
#[cfg(not(target_os = "macos"))]
pub(crate) fn trash(path: &Path) -> io::Result<()> {
    let date = jiff::ZonedDateTime::now()
        .strftime("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let uid = nix::unistd::getuid().as_raw();
    trash_with(path, home_trash_dir().as_deref(), uid, &date).map(drop)
}

/// [`trash`] with its inputs explicit: the home trash (if any), the user id
/// and the `DeletionDate`. Returns where the item landed.
fn trash_with(path: &Path, home: Option<&Path>, uid: u32, date: &str) -> io::Result<PathBuf> {
    let src = absolute_unresolved(path)?;
    // `symlink_metadata`, so a symlink is trashed rather than followed.
    let src_dev = fs::symlink_metadata(&src)?.dev();
    if let Some(home) = home
        && device_of_nearest(home)? == src_dev
    {
        make_private_dir(home)?;
        return put(home, &src, &encode_path(&src), date);
    }
    let topdir = mount_point(&src)?;
    let trash_dir = topdir_trash(&topdir, uid)?;
    // A topdir trash records the path relative to the topdir, so the entry
    // still restores if the volume is mounted somewhere else.
    let relative = src.strip_prefix(&topdir).unwrap_or(&src);
    put(&trash_dir, &src, &encode_path(relative), date)
}

/// `path` made absolute and normalized *without* resolving its last
/// component: the parent is canonicalized, the file name is kept.
fn absolute_unresolved(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .filter(|_| !matches!(path.components().next_back(), Some(Component::ParentDir)))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "nothing to trash"))?;
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => fs::canonicalize(p)?,
        _ => std::env::current_dir()?,
    };
    Ok(parent.join(name))
}

/// The home trash: `$XDG_DATA_HOME/Trash`, or `$HOME/.local/share/Trash` when
/// `XDG_DATA_HOME` is unset, empty or relative (the XDG rule).
#[cfg(not(target_os = "macos"))]
fn home_trash_dir() -> Option<PathBuf> {
    home_trash_dir_from(
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

fn home_trash_dir_from(xdg_data_home: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    let data = xdg_data_home.filter(|p| p.is_absolute()).or_else(|| {
        home.filter(|h| h.is_absolute())
            .map(|h| h.join(".local/share"))
    })?;
    Some(data.join("Trash"))
}

/// The device of `path`, or of its nearest existing ancestor (the trash may
/// not have been made yet).
fn device_of_nearest(path: &Path) -> io::Result<u64> {
    path.ancestors()
        .find_map(|p| fs::metadata(p).ok())
        .map(|m| m.dev())
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no existing ancestor"))
}

/// The top directory of the mount holding `path`: its highest ancestor still
/// on the same device.
fn mount_point(path: &Path) -> io::Result<PathBuf> {
    let dev = fs::symlink_metadata(path)?.dev();
    let mut top = path.parent().unwrap_or(path);
    for dir in path.ancestors().skip(1) {
        if fs::metadata(dir)?.dev() != dev {
            break;
        }
        top = dir;
    }
    Ok(top.to_path_buf())
}

/// The trash for a mount's `topdir`: `$topdir/.Trash/$uid` when `.Trash` is a
/// real (not symlinked) directory with the sticky bit set, else
/// `$topdir/.Trash-$uid`, which must be ours and not a symlink.
fn topdir_trash(topdir: &Path, uid: u32) -> io::Result<PathBuf> {
    let admin = topdir.join(".Trash");
    if let Ok(m) = fs::symlink_metadata(&admin)
        && m.is_dir()
        && m.permissions().mode() & 0o1000 != 0
    {
        let mine = admin.join(uid.to_string());
        if make_private_dir(&mine).is_ok() && is_own_real_dir(&mine, uid) {
            return Ok(mine);
        }
    }
    let mine = topdir.join(format!(".Trash-{uid}"));
    make_private_dir(&mine)?;
    if is_own_real_dir(&mine, uid) {
        Ok(mine)
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "{} is not a trash directory owned by this user",
                mine.display()
            ),
        ))
    }
}

/// Whether `dir` is a directory (not a symlink to one) owned by `uid`.
fn is_own_real_dir(dir: &Path, uid: u32) -> bool {
    fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && m.uid() == uid)
}

/// Create `dir` (and its parents) as `0700` if it is missing.
fn make_private_dir(dir: &Path) -> io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// Trash `src` into the trash directory `trash` (which holds `files/` and
/// `info/`), recording `path_field` and `date`. Returns where it landed.
fn put(trash: &Path, src: &Path, path_field: &str, date: &str) -> io::Result<PathBuf> {
    let files = trash.join("files");
    let info = trash.join("info");
    make_private_dir(&files)?;
    make_private_dir(&info)?;
    let base = src
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "nothing to trash"))?
        .to_string_lossy()
        .into_owned();
    for n in 1..=MAX_NAME_TRIES {
        let name = if n == 1 {
            base.clone()
        } else {
            format!("{base}.{n}")
        };
        let info_path = info.join(format!("{name}.trashinfo"));
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&info_path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let dest = files.join(&name);
        // A leftover in `files/` with no info file still owns the name.
        if fs::symlink_metadata(&dest).is_ok() {
            drop(file);
            let _ = fs::remove_file(&info_path);
            continue;
        }
        let written =
            write!(file, "{}", trash_info(path_field, date)).and_then(|()| file.sync_all());
        drop(file);
        if let Err(e) = written.and_then(|()| fs::rename(src, &dest)) {
            let _ = fs::remove_file(&info_path);
            return Err(e);
        }
        return Ok(dest);
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free name left in the trash",
    ))
}

/// The `.trashinfo` file's text.
fn trash_info(path_field: &str, date: &str) -> String {
    format!("[Trash Info]\nPath={path_field}\nDeletionDate={date}\n")
}

/// Percent-encode `path`'s bytes as the spec's `Path` key requires: every
/// byte but the URI unreserved characters and `/`.
fn encode_path(path: &Path) -> String {
    let mut out = String::new();
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(char::from(b));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xF)]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// A fresh, canonical scratch directory.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vix-fdo-trash-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::canonicalize(&dir).unwrap()
    }

    #[test]
    fn trash_with_uses_the_home_trash_on_the_same_device() {
        let dir = scratch("home");
        let home = dir.join("data/Trash");
        let src = dir.join("my file");
        fs::write(&src, b"x").unwrap();
        let uid = nix::unistd::getuid().as_raw();
        let dest = trash_with(&src, Some(&home), uid, "2026-01-02T03:04:05").unwrap();
        assert_eq!(dest, home.join("files/my file"));
        assert!(!src.exists());
        let info = fs::read_to_string(home.join("info/my file.trashinfo")).unwrap();
        let want = format!("Path={}\n", encode_path(&src));
        assert!(info.contains(&want), "{info}");
        assert!(
            info.ends_with("DeletionDate=2026-01-02T03:04:05\n"),
            "{info}"
        );
    }

    #[test]
    fn trash_with_refuses_a_missing_path() {
        let dir = scratch("missing");
        let uid = nix::unistd::getuid().as_raw();
        let err = trash_with(&dir.join("nope"), Some(&dir.join("T")), uid, "d").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn put_moves_the_item_and_writes_its_info_first() {
        let dir = scratch("put");
        let trash = dir.join("Trash");
        let src = dir.join("a.txt");
        fs::write(&src, b"x").unwrap();
        let dest = put(&trash, &src, "/p/a.txt", "2026-11-17T09:05:00").unwrap();
        assert_eq!(dest, trash.join("files/a.txt"));
        assert!(!src.exists());
        assert_eq!(fs::read(&dest).unwrap(), b"x");
        assert_eq!(
            fs::read_to_string(trash.join("info/a.txt.trashinfo")).unwrap(),
            "[Trash Info]\nPath=/p/a.txt\nDeletionDate=2026-11-17T09:05:00\n"
        );
        let mode = fs::metadata(trash.join("files"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn put_picks_a_free_name_on_collision() {
        let dir = scratch("collide");
        let trash = dir.join("Trash");
        for _ in 0..2 {
            fs::write(dir.join("a"), b"x").unwrap();
            put(&trash, &dir.join("a"), "a", "d").unwrap();
        }
        // A stray file with no info file also blocks its name.
        fs::write(trash.join("files/a.3"), b"stray").unwrap();
        fs::write(dir.join("a"), b"x").unwrap();
        let dest = put(&trash, &dir.join("a"), "a", "d").unwrap();
        assert_eq!(dest, trash.join("files/a.4"));
        assert!(trash.join("info/a.2.trashinfo").exists());
        assert!(!trash.join("info/a.3.trashinfo").exists());
        assert!(trash.join("info/a.4.trashinfo").exists());
    }

    #[test]
    fn put_removes_the_info_file_when_the_move_fails() {
        let dir = scratch("fail");
        let trash = dir.join("Trash");
        let err = put(&trash, &dir.join("missing"), "missing", "d").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert_eq!(fs::read_dir(trash.join("info")).unwrap().count(), 0);
    }

    #[test]
    fn put_moves_a_symlink_not_its_target() {
        let dir = scratch("link");
        let target = dir.join("target");
        fs::write(&target, b"keep").unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let dest = put(&dir.join("Trash"), &link, "link", "d").unwrap();
        assert!(
            fs::symlink_metadata(&dest)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).unwrap(), b"keep");
    }

    #[test]
    fn encode_path_escapes_all_but_unreserved_and_slash() {
        assert_eq!(encode_path(Path::new("/a-b_c.d~/e")), "/a-b_c.d~/e");
        assert_eq!(encode_path(Path::new("/a b%é")), "/a%20b%25%C3%A9");
    }

    #[test]
    fn home_trash_follows_xdg_data_home_then_home() {
        let p = |s: &str| Some(PathBuf::from(s));
        assert_eq!(home_trash_dir_from(p("/x"), p("/h")), p("/x/Trash"));
        assert_eq!(
            home_trash_dir_from(p("rel"), p("/h")),
            p("/h/.local/share/Trash")
        );
        assert_eq!(
            home_trash_dir_from(None, p("/h")),
            p("/h/.local/share/Trash")
        );
        assert_eq!(home_trash_dir_from(None, p("rel")), None);
        assert_eq!(home_trash_dir_from(None, None), None);
    }

    #[test]
    fn topdir_trash_uses_a_sticky_admin_trash_else_its_own() {
        let uid = nix::unistd::getuid().as_raw();
        let top = scratch("top");
        assert_eq!(
            topdir_trash(&top, uid).unwrap(),
            top.join(format!(".Trash-{uid}"))
        );
        let mode = fs::metadata(top.join(format!(".Trash-{uid}")))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);

        // An admin `.Trash` without the sticky bit is not trusted.
        let top = scratch("top-nosticky");
        fs::create_dir(top.join(".Trash")).unwrap();
        assert_eq!(
            topdir_trash(&top, uid).unwrap(),
            top.join(format!(".Trash-{uid}"))
        );

        let top = scratch("top-sticky");
        fs::create_dir(top.join(".Trash")).unwrap();
        fs::set_permissions(top.join(".Trash"), fs::Permissions::from_mode(0o1777)).unwrap();
        assert_eq!(
            topdir_trash(&top, uid).unwrap(),
            top.join(format!(".Trash/{uid}"))
        );
    }

    #[test]
    fn topdir_trash_refuses_a_symlinked_trash() {
        let uid = nix::unistd::getuid().as_raw();
        let top = scratch("top-link");
        let elsewhere = scratch("elsewhere");
        std::os::unix::fs::symlink(&elsewhere, top.join(format!(".Trash-{uid}"))).unwrap();
        let err = topdir_trash(&top, uid).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn mount_point_is_the_highest_same_device_ancestor() {
        let dir = scratch("mount");
        let file = dir.join("f");
        fs::write(&file, b"x").unwrap();
        let top = mount_point(&file).unwrap();
        assert!(file.starts_with(&top));
        let dev = fs::metadata(&file).unwrap().dev();
        assert_eq!(fs::metadata(&top).unwrap().dev(), dev);
        if let Some(above) = top.parent() {
            assert_ne!(fs::metadata(above).unwrap().dev(), dev);
        }
    }

    #[test]
    fn absolute_unresolved_keeps_the_last_component() {
        let dir = scratch("abs");
        let link = dir.join("link");
        std::os::unix::fs::symlink("/nowhere", &link).unwrap();
        assert_eq!(absolute_unresolved(&link).unwrap(), link);
        assert!(absolute_unresolved(Path::new("/")).is_err());
        assert!(absolute_unresolved(&dir.join("..")).is_err());
    }
}
