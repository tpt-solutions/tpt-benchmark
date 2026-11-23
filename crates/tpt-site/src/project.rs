//! Zero-config defaults: the site title from `Cargo.toml` and the repository
//! URL from the git remote.

use std::path::Path;
use std::process::Command;

/// First `name = "..."` after `[package]` (or `[workspace.package]`).
pub fn title_from_cargo_toml(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]" || line == "[workspace.package]";
        } else if in_package {
            if let Some(rest) = line.strip_prefix("name") {
                let value = rest.trim_start().strip_prefix('=')?.trim();
                return Some(value.trim_matches('"').to_owned());
            }
        }
    }
    None
}

pub fn default_title(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("Cargo.toml"))
        .ok()
        .and_then(|t| title_from_cargo_toml(&t))
        .or_else(|| {
            std::fs::canonicalize(dir)
                .ok()?
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "Benchmarks".to_owned())
}

/// Normalises a git remote to an `https://host/owner/repo` URL.
pub fn normalise_remote(remote: &str) -> Option<String> {
    let remote = remote.trim().trim_end_matches('/');
    let remote = remote.strip_suffix(".git").unwrap_or(remote);
    if let Some(rest) = remote.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return Some(format!("https://{host}/{path}"));
    }
    if let Some(rest) = remote.strip_prefix("ssh://git@") {
        return Some(format!("https://{rest}"));
    }
    remote.starts_with("https://").then(|| remote.to_owned())
}

pub fn default_repo_url() -> Option<String> {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    normalise_remote(&String::from_utf8(out.stdout).ok()?)
}

/// GitHub Pages address for a `https://github.com/owner/repo` URL.
pub fn pages_url(repo: &str) -> Option<String> {
    let rest = repo.strip_prefix("https://github.com/")?;
    let (owner, name) = rest.split_once('/')?;
    let name = name.split('/').next()?;
    Some(format!(
        "https://{}.github.io/{name}/",
        owner.to_lowercase()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_package_name() {
        let toml = "[workspace]\nmembers = []\n\n[package]\nversion = \"1\"\nname = \"demo\"\n";
        assert_eq!(title_from_cargo_toml(toml).as_deref(), Some("demo"));
        assert_eq!(title_from_cargo_toml("[workspace]\nname = \"x\"\n"), None);
    }

    #[test]
    fn normalises_remotes() {
        for (input, want) in [
            ("git@github.com:o/r.git", "https://github.com/o/r"),
            ("https://github.com/o/r.git\n", "https://github.com/o/r"),
            ("ssh://git@github.com/o/r", "https://github.com/o/r"),
        ] {
            assert_eq!(normalise_remote(input).as_deref(), Some(want));
        }
        assert_eq!(normalise_remote("/local/path"), None);
    }

    #[test]
    fn derives_pages_url() {
        assert_eq!(
            pages_url("https://github.com/Owner/repo").as_deref(),
            Some("https://owner.github.io/repo/")
        );
        assert_eq!(pages_url("https://gitlab.com/o/r"), None);
    }
}
