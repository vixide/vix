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

/// A saved conversation's opaque local identifier and first user prompt.
#[derive(Debug, Clone)]
pub struct Entry {
    id: Uuid,
    /// Provider that owns the conversation.
    pub provider: Provider,
    /// First user prompt, abbreviated to one line.
    pub title: String,
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

    /// List saved conversations by most recent update.
    ///
    /// # Errors
    /// The directory or a saved transcript could not be read.
    pub fn entries(&self) -> io::Result<Vec<Entry>> {
        let dir = match fs::read_dir(&self.0) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            other => other?,
        };
        let mut entries = Vec::new();
        for file in dir {
            let file = file?;
            let path = file.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| Uuid::parse_str(s).ok())
            else {
                continue;
            };
            let transcript = read(&path)?;
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
            entries.push((
                file.metadata()?.modified()?,
                Entry {
                    id,
                    provider: transcript.provider,
                    title,
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
        self.lock(
            Uuid::new_v4(),
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
    /// The transcript cannot be read or another editor is using it.
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
        Ok(conversation)
    }

    fn lock(&self, id: Uuid, transcript: Transcript) -> io::Result<Conversation> {
        fs::create_dir_all(&self.0)?;
        let lock = File::options()
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.0.join(format!("{id}.lock")))?;
        lock.try_lock()?;
        Ok(Conversation {
            path: self.0.join(format!("{id}.json")),
            transcript,
            _lock: lock,
        })
    }
}

fn read(path: &Path) -> io::Result<Transcript> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// A transcript and its exclusive writer lock. Dropping it releases the lock.
pub struct Conversation {
    path: PathBuf,
    _lock: File,
    /// The current state of the conversation.
    pub transcript: Transcript,
}

impl Conversation {
    /// Atomically persist this conversation with private file permissions.
    ///
    /// # Errors
    /// Serialization or writing the transcript failed.
    pub fn save(&self) -> io::Result<()> {
        vix_fileops::write_atomic_private(&self.path, &serde_json::to_vec(&self.transcript)?)
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
        assert!(a.resume(&entry).is_err());
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
}
