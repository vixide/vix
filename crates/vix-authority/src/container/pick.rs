//! Pickers for **File → Open Remote…** (T718c): the arguments that list what
//! can be opened, and parsers that turn their output into
//! [`ContainerTarget`]s, so a Kubernetes container or a WSL distribution
//! need not be typed.
//!
//! Pure: running the program is the App's job. Every name goes through the
//! same strict checks as a typed one ([`ContainerTarget::kubernetes`],
//! [`ContainerTarget::new`]), and a row that fails them is dropped, so
//! nothing a cluster or `wsl.exe` prints can reach an engine command line
//! unchecked. Owning spec: `crates/vix-authority/spec/container/index.md`.

use super::target::{ContainerTarget, ExecKind};

/// The `kubectl` template: one line per pod, `namespace TAB pod TAB phase TAB
/// container container …`. Kubernetes names are DNS labels, so they hold no
/// tab, space or newline.
const PODS_TEMPLATE: &str = "{range .items[*]}{.metadata.namespace}{\"\\t\"}\
{.metadata.name}{\"\\t\"}{.status.phase}{\"\\t\"}\
{range .spec.containers[*]}{.name}{\" \"}{end}{\"\\n\"}{end}";

/// How long `kubectl` may wait for the API server.
const KUBECTL_REQUEST_TIMEOUT: &str = "--request-timeout=5s";

/// The arguments for `kubectl`: the pods of the current context's namespace,
/// one line each (see [`parse_kubectl_pods`]).
#[must_use]
pub fn kubectl_pods_args() -> Vec<String> {
    vec![
        "get".into(),
        "pods".into(),
        KUBECTL_REQUEST_TIMEOUT.into(),
        "-o".into(),
        format!("jsonpath={PODS_TEMPLATE}"),
    ]
}

/// The containers of every running pod in `kubectl`'s output for
/// [`kubectl_pods_args`], in the order printed, each at its home directory.
/// Pods in any other phase (`Pending`, `Succeeded`, `Failed`) cannot be
/// entered and are left out, as are lines that are not four fields and
/// names that are not valid Kubernetes names.
#[must_use]
pub fn parse_kubectl_pods(output: &str) -> Vec<ContainerTarget> {
    let mut targets = Vec::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [namespace, pod, phase, containers] = fields[..] else {
            continue;
        };
        if phase != "Running" {
            continue;
        }
        for container in containers.split_whitespace() {
            if let Ok(target) = ContainerTarget::kubernetes(namespace, pod, container) {
                targets.push(target);
            }
        }
    }
    targets
}

/// The arguments for `wsl.exe`: the installed distributions' names, one per
/// line (see [`parse_wsl_list`]).
#[must_use]
pub fn wsl_list_args() -> Vec<String> {
    vec!["--list".into(), "--quiet".into()]
}

/// The distributions in `wsl.exe`'s output for [`wsl_list_args`], in the
/// order printed, each at its home directory.
///
/// `wsl.exe` writes UTF-16LE (with or without a byte-order mark) to a pipe;
/// that is decoded, and UTF-8 is accepted too (`WSL_UTF8=1`). Blank lines and
/// names that are not valid distribution names are left out, as are repeats.
#[must_use]
pub fn parse_wsl_list(output: &[u8]) -> Vec<ContainerTarget> {
    let text = decode_wsl(output);
    let mut targets: Vec<ContainerTarget> = Vec::new();
    for line in text.lines() {
        let name = line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0');
        if name.is_empty() || targets.iter().any(|t| t.name == name) {
            continue;
        }
        if let Ok(target) = ContainerTarget::new(ExecKind::Wsl, name) {
            targets.push(target);
        }
    }
    targets
}

/// UTF-16LE when the bytes look like it (a byte-order mark, or an even
/// length with a zero in the second byte of a pair); otherwise UTF-8.
fn decode_wsl(bytes: &[u8]) -> String {
    let bom = bytes.starts_with(&[0xFF, 0xFE]);
    let (pairs, rest) = bytes.as_chunks::<2>();
    let wide = bom || (rest.is_empty() && pairs.iter().any(|&[lo, hi]| hi == 0 && lo != 0));
    if !wide {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let units: Vec<u16> = pairs
        .iter()
        .skip(usize::from(bom))
        .map(|&pair| u16::from_le_bytes(pair))
        .collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(targets: &[ContainerTarget]) -> Vec<String> {
        targets.iter().map(ContainerTarget::to_url).collect()
    }

    fn utf16le(text: &str, bom: bool) -> Vec<u8> {
        let mut out = if bom { vec![0xFF, 0xFE] } else { Vec::new() };
        for unit in text.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out
    }

    #[test]
    fn kubectl_args_ask_for_one_line_per_pod_with_a_timeout() {
        let args = kubectl_pods_args();
        assert_eq!(&args[..4], ["get", "pods", "--request-timeout=5s", "-o"]);
        assert!(args[4].starts_with("jsonpath={range .items[*]}"));
        assert!(args[4].contains("{.status.phase}"));
    }

    #[test]
    fn running_pods_give_one_target_per_container() {
        let out = "dev\tweb-7f9\tRunning\tapp sidecar \n\
                   dev\tjob-1\tSucceeded\tmain \n\
                   kube-system\tdns\tRunning\tcoredns \n";
        assert_eq!(
            urls(&parse_kubectl_pods(out)),
            [
                "k8s://dev/web-7f9/app/~",
                "k8s://dev/web-7f9/sidecar/~",
                "k8s://kube-system/dns/coredns/~",
            ]
        );
    }

    #[test]
    fn malformed_pod_lines_and_hostile_names_are_dropped() {
        let out = "only\ttwo\n\
                   dev\t-it\tRunning\tapp \n\
                   Dev\tweb\tRunning\tapp \n\
                   dev\tweb\tRunning\t--privileged ok \n\
                   \n";
        assert_eq!(urls(&parse_kubectl_pods(out)), ["k8s://dev/web/ok/~"]);
        assert_eq!(parse_kubectl_pods(""), []);
    }

    #[test]
    fn wsl_list_reads_utf16_with_or_without_a_bom() {
        for bom in [true, false] {
            let out = utf16le("Ubuntu-24.04\r\nDebian\r\n\r\n", bom);
            assert_eq!(
                urls(&parse_wsl_list(&out)),
                ["wsl://Ubuntu-24.04/~", "wsl://Debian/~"],
                "bom {bom}"
            );
        }
    }

    #[test]
    fn wsl_list_reads_utf8_and_drops_repeats_and_bad_names() {
        let out = b"Ubuntu\n  \nUbuntu\n-d\nkali-linux\n";
        assert_eq!(
            urls(&parse_wsl_list(out)),
            ["wsl://Ubuntu/~", "wsl://kali-linux/~"]
        );
        assert_eq!(parse_wsl_list(b""), []);
    }
}
