//! Built-in language servers: the well-known servers Vix starts without any
//! `lsp_servers` entry, when they are installed and the workspace is trusted.
//!
//! [`available`] picks, for each built-in language your own `lsp_servers`
//! does not cover, the first candidate command whose program is on `PATH`.
//! Your entries always win: a built-in is never added for an extension any
//! of them handles. Spec: `crates/vix-lsp/spec/index.md`.

use vix_settings::LspServer;

/// One built-in language: its LSP language id, the extensions it handles,
/// and the commands to try, in order of preference.
#[derive(Debug, Clone, Copy)]
pub struct Builtin {
    /// LSP `languageId`.
    pub language_id: &'static str,
    /// File extensions, without the dot.
    pub extensions: &'static [&'static str],
    /// Candidate launch commands (program then arguments), best first.
    pub candidates: &'static [&'static [&'static str]],
}

/// The built-in servers.
pub const BUILTINS: &[Builtin] = &[
    Builtin {
        language_id: "rust",
        extensions: &["rs"],
        candidates: &[&["rust-analyzer"]],
    },
    Builtin {
        language_id: "go",
        extensions: &["go"],
        candidates: &[&["gopls"]],
    },
    Builtin {
        language_id: "python",
        extensions: &["py", "pyi"],
        candidates: &[
            &["pyright-langserver", "--stdio"],
            &["basedpyright-langserver", "--stdio"],
            &["pylsp"],
        ],
    },
    Builtin {
        language_id: "typescript",
        extensions: &["ts", "tsx", "js", "jsx", "mjs", "cjs"],
        candidates: &[&["typescript-language-server", "--stdio"]],
    },
    Builtin {
        language_id: "c",
        extensions: &["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"],
        candidates: &[&["clangd"]],
    },
    Builtin {
        language_id: "zig",
        extensions: &["zig"],
        candidates: &[&["zls"]],
    },
    Builtin {
        language_id: "lua",
        extensions: &["lua"],
        candidates: &[&["lua-language-server"]],
    },
    Builtin {
        language_id: "shellscript",
        extensions: &["sh", "bash"],
        candidates: &[&["bash-language-server", "start"]],
    },
    Builtin {
        language_id: "toml",
        extensions: &["toml"],
        candidates: &[&["taplo", "lsp", "stdio"]],
    },
    Builtin {
        language_id: "yaml",
        extensions: &["yaml", "yml"],
        candidates: &[&["yaml-language-server", "--stdio"]],
    },
];

/// The built-in servers to add to `user`: one per built-in language whose
/// extensions none of `user` handles, using its first candidate whose
/// program `on_path` finds.
#[must_use]
pub fn available(user: &[LspServer], on_path: impl Fn(&str) -> bool) -> Vec<LspServer> {
    let covered = |ext: &str| {
        user.iter()
            .any(|s| s.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
    };
    BUILTINS
        .iter()
        .filter(|b| !b.extensions.iter().any(|e| covered(e)))
        .filter_map(|b| {
            let command = b.candidates.iter().find(|c| on_path(c[0]))?;
            Some(LspServer {
                language_id: b.language_id.to_string(),
                extensions: b.extensions.iter().map(|e| (*e).to_string()).collect(),
                command: command.iter().map(|a| (*a).to_string()).collect(),
            })
        })
        .collect()
}

/// The built-in server [`available`] would start for a file with extension
/// `ext`, by program name, if any.
#[must_use]
pub fn program_for(
    user: &[LspServer],
    ext: &str,
    on_path: impl Fn(&str) -> bool,
) -> Option<String> {
    available(user, on_path)
        .into_iter()
        .find(|s| s.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
        .and_then(|s| s.command.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(lang: &str, exts: &[&str], cmd: &[&str]) -> LspServer {
        LspServer {
            language_id: lang.to_string(),
            extensions: exts.iter().map(|e| (*e).to_string()).collect(),
            command: cmd.iter().map(|c| (*c).to_string()).collect(),
        }
    }

    #[test]
    fn only_installed_servers_are_offered() {
        let found = available(&[], |p| p == "rust-analyzer" || p == "gopls");
        let langs: Vec<&str> = found.iter().map(|s| s.language_id.as_str()).collect();
        assert_eq!(langs, ["rust", "go"]);
        assert_eq!(found[0].command, ["rust-analyzer"]);
        assert_eq!(found[0].extensions, ["rs"]);
        assert_eq!(available(&[], |_| false), vec![]);
    }

    #[test]
    fn the_first_installed_candidate_wins() {
        let found = available(&[], |p| p == "pylsp" || p == "basedpyright-langserver");
        assert_eq!(found[0].command, ["basedpyright-langserver", "--stdio"]);
        let found = available(&[], |p| p == "pylsp");
        assert_eq!(found[0].command, ["pylsp"]);
    }

    #[test]
    fn your_own_servers_win() {
        let user = [server("rust", &["rs"], &["my-ra"])];
        let found = available(&user, |_| true);
        assert!(found.iter().all(|s| s.language_id != "rust"), "{found:?}");
        // One shared extension is enough to drop the whole built-in.
        let user = [server("js", &["JS"], &["my-js"])];
        assert!(
            available(&user, |_| true)
                .iter()
                .all(|s| s.language_id != "typescript")
        );
    }

    #[test]
    fn program_for_an_extension() {
        assert_eq!(
            program_for(&[], "rs", |p| p == "rust-analyzer"),
            Some("rust-analyzer".into())
        );
        assert_eq!(
            program_for(&[], "RS", |p| p == "rust-analyzer"),
            Some("rust-analyzer".into())
        );
        assert_eq!(program_for(&[], "rs", |_| false), None);
        assert_eq!(program_for(&[], "txt", |_| true), None);
        let user = [server("rust", &["rs"], &["mine"])];
        assert_eq!(program_for(&user, "rs", |_| true), None);
    }

    #[test]
    fn builtins_do_not_overlap() {
        let mut seen: Vec<&str> = Vec::new();
        for b in BUILTINS {
            assert!(!b.candidates.is_empty(), "{}", b.language_id);
            for e in b.extensions {
                assert!(!seen.contains(e), "{e} is in two built-ins");
                seen.push(e);
            }
        }
    }
}
