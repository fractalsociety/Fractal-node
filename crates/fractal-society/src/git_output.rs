//! Git output adapter — commit a packaged paper into the research-packages repo.
//!
//! The package directory must already be written inside `repo_path` (typically
//! at `packages/<package_id>/`) by [`crate::research_package::assemble_package`].
//! This adapter runs `git add -A` + `git commit` so the package, its signed
//! manifest, and the concept index land in one atomic commit.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};

/// Reference to a completed commit.
#[derive(Debug, Clone)]
pub struct CommitRef {
    /// Repository root.
    pub repo: PathBuf,
    /// Commit SHA.
    pub commit_hash: String,
    /// Commit message.
    pub message: String,
}

/// Commit all changes in `repo_path` (the package dir + concept index) as a
/// single commit. `committer` is `(name, email)` for the commit identity.
pub fn commit_package_to_repo(
    repo_path: &Path,
    message: &str,
    committer: (&str, &str),
) -> Result<CommitRef> {
    run_git(repo_path, &["add", "-A"], committer)?;
    // `git commit` returns non-zero if there is nothing to commit; treat that
    // as an error so callers know the package was already committed.
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["commit", "-m", message])
        .env("GIT_AUTHOR_NAME", committer.0)
        .env("GIT_AUTHOR_EMAIL", committer.1)
        .env("GIT_COMMITTER_NAME", committer.0)
        .env("GIT_COMMITTER_EMAIL", committer.1)
        .output()
        .map_err(|e| Error::ProtocolViolation(format!("failed to run git commit: {e}")))?;
    if !output.status.success() {
        return Err(Error::ProtocolViolation(format!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let head = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| Error::ProtocolViolation(format!("failed to run git rev-parse: {e}")))?;
    if !head.status.success() {
        return Err(Error::ProtocolViolation(format!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&head.stderr).trim()
        )));
    }
    let commit_hash = String::from_utf8_lossy(&head.stdout).trim().to_string();

    Ok(CommitRef {
        repo: repo_path.to_path_buf(),
        commit_hash,
        message: message.to_string(),
    })
}

/// Ensure `repo_path` is a git repo (`git init` if needed), creating the
/// directory tree first if it does not yet exist.
pub fn ensure_repo(repo_path: &Path, committer: (&str, &str)) -> Result<()> {
    if !repo_path.join(".git").exists() {
        std::fs::create_dir_all(repo_path)?;
        run_git(repo_path, &["init", "-q"], committer)?;
        // Initial identity default (harmless if the repo has its own config later).
        run_git(repo_path, &["config", "user.name", committer.0], committer)?;
        run_git(repo_path, &["config", "user.email", committer.1], committer)?;
    }
    Ok(())
}

fn run_git(repo_path: &Path, args: &[&str], committer: (&str, &str)) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .env("GIT_AUTHOR_NAME", committer.0)
        .env("GIT_AUTHOR_EMAIL", committer.1)
        .env("GIT_COMMITTER_NAME", committer.0)
        .env("GIT_COMMITTER_EMAIL", committer.1)
        .output()
        .map_err(|e| {
            Error::ProtocolViolation(format!("failed to run git {}: {e}", args.join(" ")))
        })?;
    if !output.status.success() {
        // `git init` with no repo yet runs from cwd; -C may warn but still init.
        return Err(Error::ProtocolViolation(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("fractal-git-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn commits_files_to_a_repo() {
        let repo = repo_root("basic");
        ensure_repo(&repo, ("Test", "test@example.com")).unwrap();
        std::fs::write(repo.join("hello.txt"), b"hi").unwrap();

        let c = commit_package_to_repo(&repo, "test commit", ("Test", "test@example.com")).unwrap();
        assert!(!c.commit_hash.is_empty());

        // A second commit with new content produces a different HEAD.
        std::fs::write(repo.join("more.txt"), b"more").unwrap();
        let c2 = commit_package_to_repo(&repo, "second", ("Test", "test@example.com")).unwrap();
        assert_ne!(c.commit_hash, c2.commit_hash);

        let _ = std::fs::remove_dir_all(&repo);
    }
}
