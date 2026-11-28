//! Private, workspace-scoped conversation transcripts.
use crate::Provider;
use serde::{Deserialize, Serialize};

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// A visible message. Tool approval payloads are transient.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    /// Text submitted by the user.
    User(String),
    /// Text returned by the provider.
    Reply(String),
}

/// The locally stored part of a conversation.
#[derive(Debug, Serialize, Deserialize)]
pub struct Transcript {
    /// Provider that understands the resume token.
    pub provider: Provider,
    /// Provider-issued token, once a session has been established.
    pub session: Option<String>,
    /// Messages in chronological order.
    pub messages: Vec<Message>,
}

/// The filename is also the durable checkout binding. Older Vix versions
/// only recognize plain UUID stems, so cannot resume an isolated thread
/// in the original workspace.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Id {
    uuid: Uuid,
    isolated: bool,
}

impl Id {
    fn parse(stem: &str) -> Option<Self> {
        let uuid = stem.strip_suffix(".worktree");
        Some(Self {
            uuid: Uuid::parse_str(uuid.unwrap_or(stem)).ok()?,
            isolated: uuid.is_some(),
        })
    }

    fn stem(self) -> String {
        if self.isolated {
            format!("{}.worktree", self.uuid)
        } else {
            self.uuid.to_string()
        }
    }

    fn branch(self) -> Option<String> {
        self.isolated.then(|| format!("vix-agent-{}", self.uuid))
    }
}

/// A saved conversation's opaque local identifier and first user prompt.
#[derive(Debug, Clone)]
pub struct Entry {
    id: Id,
    /// Provider that owns the conversation.
    pub provider: Provider,
    /// First user prompt, abbreviated to one line.
    pub title: String,
}

impl Entry {
    /// The dedicated branch of an isolated conversation.
    #[must_use]
    pub fn worktree_branch(&self) -> Option<String> {
        self.id.branch()
    }
}

/// The private directory of one workspace.
pub struct Store(PathBuf);

impl Store {
    /// Derive the workspace directory from the host's settings path and canonical workspace name.
    #[must_use]
    pub fn for_workspace(session_file: &Path, workspace: &str) -> Self {
        let key = Uuid::new_v5(&Uuid::NAMESPACE_URL, workspace.as_bytes()).to_string();
        Self(session_file.with_added_extension("agents").join(key))
    }

    /// The workspace's private directory (it may not exist yet).
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.0
    }

    /// List saved conversations by most recent update. Each title comes from
    /// the conversation's small `<id>.meta` file; only a conversation without
    /// one (written by an older Vix) is read whole. A transcript that cannot
    /// be read or parsed is left out, so one damaged file never hides the rest.
    ///
    /// # Errors
    /// The directory could not be read.
    pub fn entries(&self) -> io::Result<Vec<Entry>> {
        let dir = match fs::read_dir(&self.0) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            other => other?,
        };
        let mut entries = Vec::new();
        for file in dir.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(Id::parse)
            else {
                continue;
            };
            let Some(meta) = read_meta(&path.with_extension("meta"))
                .or_else(|| read(&path).ok().map(|t| Meta::of(&t)))
            else {
                continue;
            };
            let Ok(modified) = file.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            entries.push((
                modified,
                Entry {
                    id,
                    provider: meta.provider,
                    title: meta.title,
                },
            ));
        }
        entries.sort_by_key(|(time, entry)| std::cmp::Reverse((*time, entry.id)));
        Ok(entries.into_iter().map(|(_, e)| e).collect())
    }

    /// Open a new conversation. It is first written when a message is sent.
    ///
    /// # Errors
    /// Its private directory or lock could not be opened.
    pub fn create(&self, provider: Provider) -> io::Result<Conversation> {
        self.create_with(provider, false)
    }

    /// Reserve a new ID for a conversation in a dedicated Git worktree.
    /// The host provisions the checkout before launching a provider.
    ///
    /// # Errors
    /// Its private directory or lock could not be opened.
    pub fn create_worktree(&self, provider: Provider) -> io::Result<Conversation> {
        self.create_with(provider, true)
    }

    fn create_with(&self, provider: Provider, isolated: bool) -> io::Result<Conversation> {
        self.lock(
            Id {
                uuid: Uuid::new_v4(),
                isolated,
            },
            Transcript {
                provider,
                session: None,
                messages: Vec::new(),
            },
        )
    }

    /// Reopen the exact saved conversation selected by the host.
    ///
    /// # Errors
    /// The transcript cannot be read, or another editor is using it: then the
    /// error's kind is [`io::ErrorKind::WouldBlock`].
    pub fn resume(&self, entry: &Entry) -> io::Result<Conversation> {
        let mut conversation = self.lock(
            entry.id,
            Transcript {
                provider: entry.provider,
                session: None,
                messages: Vec::new(),
            },
        )?;
        conversation.transcript = read(&conversation.path)?;
        conversation.meta = Some(Meta::of(&conversation.transcript));
        Ok(conversation)
    }

    /// Take the conversation's writer lock. The directories are owner-only
    /// (0700) and the lock file 0600 on Unix.
    fn lock(&self, id: Id, transcript: Transcript) -> io::Result<Conversation> {
        private_dir(&self.0)?;
        let stem = id.stem();
        let lock_path = self.0.join(format!("{stem}.lock"));
        let mut options = File::options();
        options.write(true).create(true).truncate(false);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let lock = options.open(&lock_path)?;
        lock.try_lock().map_err(|e| match e {
            fs::TryLockError::WouldBlock => io::Error::new(
                io::ErrorKind::WouldBlock,
                "the conversation is open in another Vix",
            ),
            fs::TryLockError::Error(e) => e,
        })?;
        Ok(Conversation {
            id,
            path: self.0.join(format!("{stem}.json")),
            lock_path,
            transcript,
            meta: None,
            _lock: lock,
        })
    }
}

/// Create `dir` and its missing parents owner-only, and make `dir` itself
/// owner-only if it already existed.
fn private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        builder.mode(0o700);
        builder.create(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    builder.create(dir)
}

fn read(path: &Path) -> io::Result<Transcript> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// What the chooser shows for a conversation, kept beside its transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Meta {
    provider: Provider,
    title: String,
}

impl Meta {
    /// The provider and the first user prompt's first line (64 characters).
    fn of(transcript: &Transcript) -> Self {
        let title = transcript
            .messages
            .iter()
            .find_map(|m| match m {
                Message::User(s) => Some(
                    s.lines()
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .take(64)
                        .collect(),
                ),
                Message::Reply(_) => None,
            })
            .unwrap_or_default();
        Self {
            provider: transcript.provider,
            title,
        }
    }
}

fn read_meta(path: &Path) -> Option<Meta> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

/// A transcript and its exclusive writer lock. Dropping it releases the lock,
/// and removes the lock file of a conversation that was never saved.
pub struct Conversation {
    id: Id,
    path: PathBuf,
    lock_path: PathBuf,
    /// The chooser entry last written, to rewrite it only when it changes.
    meta: Option<Meta>,
    _lock: File,
    /// The current state of the conversation.
    pub transcript: Transcript,
}

impl Conversation {
    /// The isolated checkout beside the transcript, or `None` for an
    /// ordinary conversation. Its final component is also its branch name.
    #[must_use]
    pub fn worktree(&self) -> Option<PathBuf> {
        self.id
            .branch()
            .map(|branch| self.path.with_file_name(branch))
    }

    /// Atomically persist this conversation with private file permissions,
    /// and its chooser entry when that changed.
    ///
    /// # Errors
    /// Serialization or writing the transcript failed.
    pub fn save(&mut self) -> io::Result<()> {
        let meta = Meta::of(&self.transcript);
        if self.meta.as_ref() != Some(&meta) {
            vix_fileops::write_atomic_private(
                &self.path.with_extension("meta"),
                &serde_json::to_vec(&meta)?,
            )?;
            self.meta = Some(meta);
        }
        vix_fileops::write_atomic_private(&self.path, &serde_json::to_vec(&self.transcript)?)
    }
}

impl Drop for Conversation {
    fn drop(&mut self) {
        // Only the lock of a conversation nothing was written for: removing a
        // lock file another Vix may be about to lock would let two writers in.
        if !self.path.exists() {
            let _ = fs::remove_file(&self.lock_path);
            let _ = fs::remove_file(self.path.with_extension("meta"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_conversations_survive_restart_with_exclusive_writers_and_separate_workspaces() {
        let root = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let settings = root.join("session.toml");
        let a = Store::for_workspace(&settings, "first");
        let b = Store::for_workspace(&settings, "second");
        let mut codex = a.create(Provider::Codex).unwrap();
        codex.transcript.session = Some("codex-token".into());
        codex
            .transcript
            .messages
            .push(Message::User("first task".into()));
        codex.save().unwrap();
        let entry = a.entries().unwrap().pop().unwrap();
        assert_eq!(entry.title, "first task");
        assert_eq!(
            a.resume(&entry).err().map(|e| e.kind()),
            Some(io::ErrorKind::WouldBlock)
        );
        drop(codex);
        let mut claude = a.create(Provider::Claude).unwrap();
        claude.transcript.session = Some("claude-token".into());
        claude.save().unwrap();
        drop(claude);
        assert_eq!(a.entries().unwrap().len(), 2);
        assert!(b.entries().unwrap().is_empty());
        let resumed = a.resume(&entry).unwrap();
        assert_eq!(resumed.transcript.session.as_deref(), Some("codex-token"));
        drop(resumed);
        fs::remove_dir_all(root).unwrap();
    }

    fn scratch() -> (PathBuf, Store) {
        let root = std::env::temp_dir().join(Uuid::new_v4().to_string());
        let store = Store::for_workspace(&root.join("session.toml"), "w");
        (root, store)
    }

    #[test]
    fn a_corrupt_transcript_is_skipped_and_an_old_one_without_meta_still_lists() {
        let (root, store) = scratch();
        let mut good = store.create(Provider::Codex).unwrap();
        good.transcript
            .messages
            .push(Message::User("kept\nmore".into()));
        good.save().unwrap();
        drop(good);
        let dir = &store.0;
        fs::write(dir.join(format!("{}.json", Uuid::new_v4())), b"{not json").unwrap();
        fs::write(dir.join("notes.json"), b"{}").unwrap();
        // An older Vix wrote no meta file: the transcript itself is read.
        let old = Uuid::new_v4();
        let transcript = Transcript {
            provider: Provider::Claude,
            session: None,
            messages: vec![Message::Reply("r".into()), Message::User("old".into())],
        };
        fs::write(
            dir.join(format!("{old}.json")),
            serde_json::to_vec(&transcript).unwrap(),
        )
        .unwrap();
        let mut titles: Vec<_> = store
            .entries()
            .unwrap()
            .into_iter()
            .map(|e| (e.provider, e.title))
            .collect();
        titles.sort_by_key(|(_, t)| t.clone());
        assert_eq!(
            titles,
            [
                (Provider::Codex, "kept".to_owned()),
                (Provider::Claude, "old".to_owned())
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_title_comes_from_the_meta_file_not_the_transcript() {
        let (root, store) = scratch();
        let mut c = store.create(Provider::Codex).unwrap();
        c.transcript.messages.push(Message::User("first".into()));
        c.save().unwrap();
        c.transcript
            .messages
            .push(Message::Reply("x".repeat(100_000)));
        c.save().unwrap();
        let meta = c.path.with_extension("meta");
        assert!(fs::metadata(&meta).unwrap().len() < 100);
        // The listing does not parse the transcript.
        fs::write(&c.path, b"{damaged").unwrap();
        assert_eq!(store.entries().unwrap()[0].title, "first");
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unsaved_conversation_leaves_no_lock_file_behind() {
        let (root, store) = scratch();
        drop(store.create(Provider::Claude).unwrap());
        assert_eq!(fs::read_dir(&store.0).unwrap().count(), 0);
        let mut saved = store.create(Provider::Claude).unwrap();
        saved.save().unwrap();
        let lock = saved.lock_path.clone();
        drop(saved);
        assert!(lock.exists(), "a saved conversation keeps its lock file");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_store_and_its_locks_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let (root, store) = scratch();
        fs::create_dir_all(&store.0).unwrap();
        fs::set_permissions(&store.0, fs::Permissions::from_mode(0o755)).unwrap();
        let c = store.create(Provider::Codex).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&store.0), 0o700);
        assert_eq!(mode(&c.lock_path), 0o600);
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
}
