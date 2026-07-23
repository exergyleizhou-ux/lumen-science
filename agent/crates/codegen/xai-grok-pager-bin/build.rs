use std::path::{Path, PathBuf};
use std::process::Command;

fn git_output(manifest_dir: &Path, args: &[&str]) -> Option<String> {
    Command::new("git")
        .arg("-C")
        .arg(manifest_dir)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_owned())
        .filter(|output| !output.is_empty())
}

fn watch_git_path(manifest_dir: &Path, git_path: &str) {
    let path = PathBuf::from(git_path);
    let path = if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    };
    println!("cargo:rerun-if-changed={}", path.display());
}

fn main() {
    println!("cargo:rerun-if-env-changed=GROK_VERSION");
    let manifest_dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );

    if let Some(head_path) = git_output(&manifest_dir, &["rev-parse", "--git-path", "HEAD"]) {
        watch_git_path(&manifest_dir, &head_path);
    }
    if let Some(head_ref) = git_output(&manifest_dir, &["symbolic-ref", "-q", "HEAD"])
        && let Some(ref_path) = git_output(&manifest_dir, &["rev-parse", "--git-path", &head_ref])
    {
        watch_git_path(&manifest_dir, &ref_path);
    }

    let commit = git_output(&manifest_dir, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|| "unknown".to_string());

    let version = std::env::var("GROK_VERSION")
        .or_else(|_| std::env::var("CARGO_PKG_VERSION"))
        .unwrap_or_else(|_| "0.0.0".to_string());

    println!(
        "cargo:rustc-env=VERSION_WITH_COMMIT={} ({})",
        version, commit
    );
}
