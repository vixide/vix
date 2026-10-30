//! Finding, uploading and starting the remote agent for [`SshAuthority`]
//! (T717).
//!
//! One shell round trip reports the remote `uname -sm` and every candidate
//! `vix` (the per-user cache, then `PATH`) with its `--version --json`. A
//! candidate of exactly this version is started with `--remote-agent`. With
//! [`AgentMode::Upload`], and only when the remote system and machine match
//! this one, the local binary is uploaded to the cache through a temporary
//! name, its SHA-256 checked *on the remote side* against the local hash,
//! and only then made executable, run once to confirm its version, and
//! renamed into place.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::authority::{SshAuthority, code};
use super::quote::{bytes_path, path_bytes, sh_quote};
use crate::agent::{AgentClient, VERSION};

/// Whether [`SshAuthority::connect`] tries the remote agent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentMode {
    /// Never: every operation is a shell script (T712's behavior).
    #[default]
    Off,
    /// Use a `vix` of exactly this version already on the remote machine
    /// (its `PATH`, or the per-user agent cache); never upload one.
    Installed,
    /// As [`AgentMode::Installed`], and when there is none, upload this
    /// binary to the remote cache (same operating system and machine only,
    /// checksum-verified).
    Upload,
}

impl AgentMode {
    /// The mode a `remote_agent` setting names: `"off"`, `"installed"` or
    /// `"upload"` (case-insensitive). Anything else is `Installed`, which
    /// never writes to the remote machine.
    #[must_use]
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Self::Off,
            "upload" => Self::Upload,
            _ => Self::Installed,
        }
    }
}

/// What became of the remote agent on the last connect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentStatus {
    /// Not tried ([`AgentMode::Off`], or not connected yet).
    #[default]
    Off,
    /// Running: file operations go through it.
    Running {
        /// The remote `vix` that runs it.
        path: PathBuf,
        /// It was uploaded on this connect.
        uploaded: bool,
    },
    /// No `vix` of this version on the remote machine, and uploading was
    /// not allowed.
    NotInstalled,
    /// It could not be used (why, in English, untranslated); shell scripts
    /// are used instead.
    Unavailable(String),
}

/// The remote cache directory for this version, as `sh` text (no quotes
/// needed around `$c`'s use): `${XDG_CACHE_HOME:-$HOME/.cache}/vix/agent/V`,
/// ignoring a relative `XDG_CACHE_HOME` as the XDG spec says to.
fn cache_dir_script() -> String {
    format!(
        "v={}\nc=${{XDG_CACHE_HOME:-}}\ncase $c in /*) ;; *) c=$HOME/.cache ;; esac\n\
         d=$c/vix/agent/$v\n",
        String::from_utf8_lossy(&sh_quote(VERSION.as_bytes()))
    )
}

/// The probe: `uname -sm`, then each runnable candidate's path and
/// `--version --json`, all NUL-separated.
fn probe_script() -> String {
    format!(
        "{}printf '%s\\000' \"$(uname -sm 2>/dev/null)\"\n\
         for x in \"$d/vix\" \"$(command -v vix 2>/dev/null)\"; do\n\
           case $x in /*) ;; *) continue ;; esac\n\
           [ -f \"$x\" ] && [ -x \"$x\" ] || continue\n\
           j=$(\"$x\" --version --json 2>/dev/null </dev/null) || continue\n\
           printf '%s\\000%s\\000' \"$x\" \"$j\"\n\
         done\nexit 0\n",
        cache_dir_script()
    )
}

/// The upload: stdin into an exclusive temp file in a private cache
/// directory, SHA-256 compared with `hash` remotely, then made executable,
/// run once (`--version --json` must name this version) and renamed to
/// `$d/vix`, whose path it prints.
fn upload_script(hash: &str, token: &str) -> String {
    format!(
        "{}umask 077\nmkdir -p -- \"$d\" || exit 1\n\
         [ -d \"$d\" ] && ! [ -h \"$d\" ] && [ -O \"$d\" ] || exit 101\n\
         chmod 700 \"$d\" || exit 1\n\
         t=\"$d\"/{token}\n\
         (set -C; : > \"$t\") || exit 1\n\
         cat > \"$t\" || {{ rm -f -- \"$t\"; exit 1; }}\n\
         if command -v sha256sum >/dev/null 2>&1; then s=$(sha256sum < \"$t\"); s=${{s%% *}}\n\
         elif command -v shasum >/dev/null 2>&1; then s=$(shasum -a 256 < \"$t\"); s=${{s%% *}}\n\
         elif command -v openssl >/dev/null 2>&1; then s=$(openssl dgst -sha256 < \"$t\"); s=${{s##* }}\n\
         else rm -f -- \"$t\"; exit {no_tool}; fi\n\
         [ \"$s\" = {hash} ] || {{ rm -f -- \"$t\"; exit {mismatch}; }}\n\
         chmod 700 \"$t\" || {{ rm -f -- \"$t\"; exit 1; }}\n\
         j=$(\"$t\" --version --json 2>/dev/null </dev/null) || {{ rm -f -- \"$t\"; exit {broken}; }}\n\
         case $j in *\"\\\"version\\\":\\\"$v\\\"\"*) ;; *) rm -f -- \"$t\"; exit {broken} ;; esac\n\
         mv -f -- \"$t\" \"$d/vix\" || {{ rm -f -- \"$t\"; exit 1; }}\n\
         printf %s \"$d/vix\"\n",
        cache_dir_script(),
        hash = String::from_utf8_lossy(&sh_quote(hash.as_bytes())),
        no_tool = code::NO_HASH_TOOL,
        mismatch = code::CHECKSUM,
        broken = code::NOT_RUNNABLE,
    )
}

/// The `"version"` value of a `vix --version --json` reply.
pub(crate) fn json_version(json: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(json).ok()?;
    let rest = &text[text.find("\"version\":\"")? + "\"version\":\"".len()..];
    Some(&rest[..rest.find('"')?])
}

/// The probe's reply: the remote `uname -sm`, and each candidate's path
/// and version.
pub(crate) fn parse_probe(reply: &[u8]) -> (String, Vec<(PathBuf, String)>) {
    let mut fields = reply.split(|&b| b == 0);
    let uname = String::from_utf8_lossy(fields.next().unwrap_or_default())
        .trim()
        .to_string();
    let mut candidates = Vec::new();
    while let (Some(path), Some(json)) = (fields.next(), fields.next()) {
        if let Some(v) = json_version(json) {
            candidates.push((bytes_path(path), v.to_string()));
        }
    }
    (uname, candidates)
}

/// Whether a remote `uname -sm` names this machine's operating system and
/// architecture, so this binary can run there. Unknown pairs never match.
#[must_use]
pub fn same_platform(uname: &str) -> bool {
    let mut words = uname.split_whitespace();
    let (Some(os), Some(arch)) = (words.next(), words.next()) else {
        return false;
    };
    let os_ok = match std::env::consts::OS {
        "linux" => os == "Linux",
        "macos" => os == "Darwin",
        "freebsd" => os == "FreeBSD",
        "netbsd" => os == "NetBSD",
        "openbsd" => os == "OpenBSD",
        "dragonfly" => os == "DragonFly",
        "illumos" | "solaris" => os == "SunOS",
        _ => false,
    };
    let arch_ok = match std::env::consts::ARCH {
        "x86_64" => matches!(arch, "x86_64" | "amd64"),
        "aarch64" => matches!(arch, "aarch64" | "arm64"),
        "riscv64" => arch == "riscv64",
        "s390x" => arch == "s390x",
        "loongarch64" => arch == "loongarch64",
        _ => false,
    };
    os_ok && arch_ok
}

/// SHA-256 of `bytes`, in lowercase hex.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

impl SshAuthority {
    /// Find, upload (as the options allow) and start the remote agent, and
    /// route file operations through it from now on. Never fails: when no
    /// agent can run, the shell scripts stay in use, and the returned (and
    /// [`SshAuthority::agent_status`]) status says why.
    pub fn start_agent(&self) -> AgentStatus {
        let status = self.try_start_agent();
        self.set_agent_status(status.clone());
        status
    }

    fn try_start_agent(&self) -> AgentStatus {
        let mode = self.options().agent;
        if mode == AgentMode::Off {
            return AgentStatus::Off;
        }
        let reply = match self.run_script(probe_script().as_bytes(), None) {
            Ok(r) => r,
            Err(e) => return AgentStatus::Unavailable(e.to_string()),
        };
        let (uname, candidates) = parse_probe(&reply);
        let mut why = None;
        for (path, version) in candidates {
            if version != VERSION {
                why = Some(format!(
                    "{} is Vix {version}, not {VERSION}",
                    path.display()
                ));
                continue;
            }
            match self.launch_agent(&path) {
                Ok(()) => {
                    return AgentStatus::Running {
                        path,
                        uploaded: false,
                    };
                }
                Err(e) => why = Some(e.to_string()),
            }
        }
        if mode != AgentMode::Upload {
            return why.map_or(AgentStatus::NotInstalled, AgentStatus::Unavailable);
        }
        if !same_platform(&uname) {
            return AgentStatus::Unavailable(format!(
                "the remote machine is {uname}, not {} {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ));
        }
        let uploaded = self
            .local_agent_binary()
            .and_then(|bytes| self.upload_agent(&bytes, &sha256_hex(&bytes)))
            .and_then(|path| self.launch_agent(&path).map(|()| path));
        match uploaded {
            Ok(path) => AgentStatus::Running {
                path,
                uploaded: true,
            },
            Err(e) => AgentStatus::Unavailable(e.to_string()),
        }
    }

    /// The bytes of the binary to upload: [`super::SshOptions::agent_binary`],
    /// else this process's executable when it is a `vix` (a GUI front end
    /// or a test harness is not an agent).
    fn local_agent_binary(&self) -> io::Result<Vec<u8>> {
        let path = if let Some(p) = &self.options().agent_binary {
            p.clone()
        } else {
            let exe = std::env::current_exe()?;
            if exe.file_stem().and_then(|s| s.to_str()) != Some("vix") {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "this program is not the vix binary, so it cannot be uploaded as the agent",
                ));
            }
            exe
        };
        std::fs::read(path)
    }

    /// Upload `binary` as this version's remote agent and return its remote
    /// path. `sha256` (lowercase hex) is the hash the remote side must
    /// compute for what it received; on a mismatch, or when the result does
    /// not run and report this version, the temporary file is removed and
    /// nothing is installed.
    ///
    /// # Errors
    /// `InvalidData` for a checksum mismatch, `Unsupported` when the remote
    /// machine has no `sha256sum`, `shasum` or `openssl`, or the binary does
    /// not run there; `PermissionDenied` when the cache directory is not
    /// this user's; or the transfer's error.
    pub fn upload_agent(&self, binary: &[u8], sha256: &str) -> io::Result<PathBuf> {
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a SHA-256 hash",
            ));
        }
        let token = format!(".vix-upload-{}", self.save_token());
        let script = upload_script(&sha256.to_ascii_lowercase(), &token);
        let out = self.run_script(script.as_bytes(), Some(binary))?;
        if out.is_empty() {
            return Err(io::Error::other("the upload printed no path"));
        }
        Ok(bytes_path(&out))
    }

    /// Start `path --remote-agent` over `ssh` and complete the handshake.
    fn launch_agent(&self, path: &Path) -> io::Result<()> {
        let mut script = b"exec ".to_vec();
        script.extend(sh_quote(&path_bytes(path)));
        script.extend_from_slice(b" --remote-agent\n");
        let cmd: Command = self.script_command(&script);
        let client = AgentClient::spawn(cmd)?;
        self.set_agent(Some(client));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_names_a_mode() {
        assert_eq!(AgentMode::from_setting("off"), AgentMode::Off);
        assert_eq!(AgentMode::from_setting(" Upload "), AgentMode::Upload);
        assert_eq!(AgentMode::from_setting("installed"), AgentMode::Installed);
        assert_eq!(AgentMode::from_setting("bogus"), AgentMode::Installed);
    }

    #[test]
    fn probe_replies_parse() {
        let reply = b"Linux x86_64\0/home/me/.cache/vix/agent/1.0.0/vix\0{\"name\":\"vix\",\"version\":\"1.0.0\",\"capabilities\":[]}\0/usr/bin/vix\0junk\0";
        let (uname, c) = parse_probe(reply);
        assert_eq!(uname, "Linux x86_64");
        assert_eq!(
            c,
            [(
                PathBuf::from("/home/me/.cache/vix/agent/1.0.0/vix"),
                "1.0.0".to_string()
            )]
        );
        assert_eq!(parse_probe(b"").1, []);
        assert_eq!(json_version(b"{\"version\":\"2"), None);
    }

    #[test]
    fn platforms_match_only_this_one() {
        assert!(!same_platform(""));
        assert!(!same_platform("Plan9 mips"));
        let os = match std::env::consts::OS {
            "linux" => "Linux",
            "macos" => "Darwin",
            _ => return,
        };
        let arch = match std::env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "arm64",
            _ => return,
        };
        assert!(same_platform(&format!("{os} {arch}")));
        assert!(!same_platform(&format!("{os} sparc64")));
    }

    #[test]
    fn hashes_are_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
