//! L3: block writes to persistence / credential / system paths.

use crate::hidden::strip_hidden_chars;
use crate::unsafe_mode;
use crate::CheckResult;

const SENSITIVE_SEGMENTS: &[(&str, &str)] = &[
    ("/.ssh/", "write into ~/.ssh (SSH key / authorized_keys injection)"),
    (
        "/.git/hooks/",
        "write a git hook (executes on git operations — RCE)",
    ),
    ("/.aws/", "write AWS credentials/config"),
    ("/.kube/", "write kube config (cluster hijack)"),
    ("/.gnupg/", "write into GnuPG keyring"),
    ("/.config/fish/", "write fish shell config (persistence)"),
    ("/.config/autostart/", "write an autostart entry (persistence)"),
    ("/.docker/config.json", "write Docker config (credential store)"),
];

const SENSITIVE_PREFIXES: &[(&str, &str)] = &[
    ("/etc/", "write to a system config under /etc"),
    ("/usr/", "write under /usr (system binaries/libs)"),
    ("/bin/", "write under /bin"),
    ("/sbin/", "write under /sbin"),
    ("/boot/", "write under /boot"),
    ("/lib/", "write under /lib"),
    ("/lib64/", "write under /lib64"),
    ("/var/spool/cron", "write a cron job"),
    ("/system/", "write under /System (macOS)"),
    // ── Windows system directories (both raw c:/ and normalized /c/ forms) ──
    ("c:/windows/", "write under Windows system directory"),
    ("/c/windows/", "write under Windows system directory"),
    ("c:/program files/", "write under Program Files"),
    ("/c/program files/", "write under Program Files"),
    ("c:/program files (x86)/", "write under Program Files (x86)"),
    ("/c/program files (x86)/", "write under Program Files (x86)"),
];

const SHELL_RC: &[&str] = &[
    ".bashrc",
    ".bash_profile",
    ".bash_login",
    ".profile",
    ".zshrc",
    ".zprofile",
    ".zshenv",
    ".zlogin",
    ".kshrc",
    ".netrc",
    ".bash_aliases",
    ".gitconfig",
    ".npmrc",
];

/// Block write/edit targets that plant persistence or clobber system paths.
pub fn check_write_path(path: &str) -> CheckResult {
    if unsafe_mode() {
        return CheckResult::ok();
    }
    check_write_path_strict(path)
}

/// Rule-table evaluation that ignores `LUMEN_UNSAFE` (see `check_bash_strict`).
pub fn check_write_path_strict(path: &str) -> CheckResult {
    let p = strip_hidden_chars(path.trim());
    if p.is_empty() {
        return CheckResult::ok();
    }
    let p = p.replace("${HOME}", "~").replace("$HOME", "~");
    let clean = normalize_path_slashes(&p);
    let lower = clean.to_ascii_lowercase();

    // Normalise Windows drive-letter paths: "c:/..." → "/c/..." so that
    // segment and prefix matching behaves the same as Git Bash paths.
    let lower = normalize_drive_letter(&lower);

    let mut probe = lower.clone();
    if !probe.starts_with('/') && !probe.starts_with('~') {
        probe = format!("/{probe}");
    } else if probe.starts_with('~') && !probe.starts_with("~/") && probe != "~" {
        // "~foo" oddity — still probe with leading slash form for segments
        probe = format!("/{probe}");
    }

    // Segment matches need "/.ssh/" style; ensure leading slash for relative ".ssh/..."
    let segment_probe = if probe.starts_with("~/") {
        format!("/{}", &probe[1..]) // "/.ssh/..."
    } else if !probe.starts_with('/') {
        format!("/{probe}")
    } else {
        probe.clone()
    };

    for (seg, reason) in SENSITIVE_SEGMENTS {
        if segment_probe.contains(seg) || lower.contains(seg) {
            return CheckResult::deny(*reason);
        }
    }
    for (prefix, reason) in SENSITIVE_PREFIXES {
        if lower.starts_with(prefix) {
            return CheckResult::deny(*reason);
        }
    }

    // Home-anchored shell rc / login files.
    let home_anchored = lower.starts_with("~/")
        || lower.starts_with("/root/")
        || lower.starts_with("/home/")
        || lower.starts_with("/users/")
        || is_windows_user_dir(&lower);
    if home_anchored {
        let base = file_name(&clean).to_ascii_lowercase();
        if SHELL_RC.iter().any(|n| *n == base) {
            return CheckResult::deny("write to a shell startup file (persistence)");
        }
        // ~/.ssh/config, authorized_keys by basename under .ssh already caught by segment
    }

    CheckResult::ok()
}

/// Normalize Windows drive-letter paths so "c:/path" and "d:/path" become
/// "/c/path" and "/d/path", matching Git Bash conventions. Unix paths and
/// relative paths pass through unchanged.
fn normalize_drive_letter(p: &str) -> String {
    let bytes = p.as_bytes();
    if bytes.len() >= 3
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
        && bytes[0].is_ascii_alphabetic()
    {
        format!("/{}/{}", bytes[0] as char, &p[3..])
    } else {
        p.to_string()
    }
}

fn normalize_path_slashes(p: &str) -> String {
    // Collapse .. and . without requiring absolute host paths.
    let mut stack: Vec<&str> = Vec::new();
    let starts_tilde = p.starts_with('~');
    let starts_slash = p.starts_with('/');
    for part in p.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if stack.last().is_some_and(|s| *s != ".." && *s != "~") {
                stack.pop();
            } else if !starts_slash && !starts_tilde {
                stack.push("..");
            }
            continue;
        }
        stack.push(part);
    }
    let mut out = stack.join("/");
    if starts_tilde {
        if out.is_empty() {
            out = "~".into();
        } else if !out.starts_with('~') {
            out = format!("~/{out}");
        }
    } else if starts_slash {
        out = format!("/{out}");
    }
    out
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Returns true if `p` is a Windows user profile directory like
/// "/c/users/lei/" or "/d/users/admin/".
fn is_windows_user_dir(p: &str) -> bool {
    // Expected form after normalize_drive_letter: /X/users/username/...
    if p.len() < 10 {
        return false;
    }
    let bytes = p.as_bytes();
    // "/c/users/" = 9 chars minimum
    bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b'/'
        && bytes[3..9].eq_ignore_ascii_case(b"users/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_ssh_and_rc() {
        for p in [
            "~/.ssh/authorized_keys",
            "~/.ssh/id_rsa",
            "/home/dev/.ssh/authorized_keys",
            ".git/hooks/pre-commit",
            "repo/sub/.git/hooks/post-checkout",
            "~/.bashrc",
            "~/.zshrc",
            "$HOME/.bash_profile",
            "/root/.profile",
            "/etc/cron.d/evil",
            "/etc/sudoers",
            "/usr/local/bin/x",
            "~/.aws/credentials",
            "~/.config/fish/config.fish",
            "foo/../../../.ssh/authorized_keys",
            // Windows paths
            "C:\\Users\\dev\\.ssh\\authorized_keys",
            "c:/users/dev/.bashrc",
            "/c/users/dev/.zshrc",
            "C:\\Windows\\System32\\evil.dll",
        ] {
            assert!(!check_write_path(p).safe, "should block {p}");
        }
    }

    #[test]
    fn allows_project_writes() {
        for p in [
            "src/main.go",
            "./internal/foo/bar.go",
            "README.md",
            "docs/design.md",
            "config/app.yaml",
            "pkg/ssh/client.go",
            "templates/bashrc.tmpl",
            // Windows project paths should be allowed
            "C:\\Users\\dev\\code\\myproject\\src\\main.go",
            "c:/users/dev/code/myproject/README.md",
        ] {
            let r = check_write_path(p);
            assert!(r.safe, "should allow {p}: {}", r.reason);
        }
    }
}
