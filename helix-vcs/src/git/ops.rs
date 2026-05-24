use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::StagingSection;

fn exec_git(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_ASKPASS")
        .env_remove("SSH_ASKPASS")
        .env("GIT_TERMINAL_PROMPT", "false")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "commit.gpgsign")
        .env("GIT_CONFIG_VALUE_0", "false")
        .output()
        .with_context(|| format!("failed to spawn git {}", args.join(" ")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        bail!(
            "git {} failed: {}{}",
            args.join(" "),
            stderr,
            stdout
        );
    }
}

pub fn stage_file(cwd: &Path, path: &Path) -> Result<()> {
    let path = path.to_string_lossy();
    exec_git(cwd, &["add", "--", path.as_ref()])?;
    Ok(())
}

pub fn stage_all(cwd: &Path) -> Result<()> {
    exec_git(cwd, &["add", "-A"])?;
    Ok(())
}

pub fn commit(cwd: &Path, message: &str) -> Result<()> {
    exec_git(cwd, &["commit", "-m", message])?;
    Ok(())
}

pub fn file_diff(cwd: &Path, path: &Path, section: StagingSection, untracked: bool) -> Result<String> {
    let path_str = path.to_string_lossy();
    if untracked {
        if !path.exists() {
            bail!("untracked file does not exist: {}", path.display());
        }
        // git diff --no-index exits 1 when diffs exist; treat that as success.
        #[cfg(unix)]
        let empty = "/dev/null";
        #[cfg(windows)]
        let empty = "NUL";

        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(["diff", "--no-index", "--", empty])
            .arg(path)
            .env_remove("GIT_DIR")
            .env("GIT_TERMINAL_PROMPT", "false")
            .output()
            .context("failed to spawn git diff --no-index")?;
        if output.status.success() || output.status.code() == Some(1) {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git diff --no-index failed: {stderr}");
        }
    } else {
        let args = match section {
            StagingSection::Staged => vec!["diff", "--cached", "--", path_str.as_ref()],
            StagingSection::Unstaged => vec!["diff", "--", path_str.as_ref()],
        };
        exec_git(cwd, &args)
    }
}
