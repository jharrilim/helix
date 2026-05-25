//! `helix_vcs` provides types for working with diffs from a Version Control System (VCS).
//! Currently `git` is the only supported provider for diffs, but this architecture allows
//! for other providers to be added in the future.

use anyhow::{anyhow, bail, Result};
use arc_swap::ArcSwap;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(feature = "git")]
mod git;

#[cfg(feature = "git")]
pub use git::{commit, file_diff, list_status, stage_all, stage_file, unstage_file};

mod diff;

pub use diff::{DiffHandle, Hunk};

mod status;

pub use status::{FileChange, GitStatusEntry, StagingSection};

/// Contains all active diff providers. Diff providers are compiled in via features. Currently
/// only `git` is supported.
#[derive(Clone)]
pub struct DiffProviderRegistry {
    providers: Vec<DiffProvider>,
}

impl DiffProviderRegistry {
    /// Get the given file from the VCS. This provides the unedited document as a "base"
    /// for a diff to be created.
    pub fn get_diff_base(&self, file: &Path) -> Option<Vec<u8>> {
        self.providers
            .iter()
            .find_map(|provider| match provider.get_diff_base(file) {
                Ok(res) => Some(res),
                Err(err) => {
                    log::debug!("{err:#?}");
                    log::debug!("failed to open diff base for {}", file.display());
                    None
                }
            })
    }

    /// Get the current name of the current [HEAD](https://stackoverflow.com/questions/2304087/what-is-head-in-git).
    pub fn get_current_head_name(&self, file: &Path) -> Option<Arc<ArcSwap<Box<str>>>> {
        self.providers
            .iter()
            .find_map(|provider| match provider.get_current_head_name(file) {
                Ok(res) => Some(res),
                Err(err) => {
                    log::debug!("{err:#?}");
                    log::debug!("failed to obtain current head name for {}", file.display());
                    None
                }
            })
    }

    /// Fire-and-forget changed file iteration. Runs everything in a background task. Keeps
    /// iteration until `on_change` returns `false`.
    pub fn for_each_changed_file(
        self,
        cwd: PathBuf,
        f: impl Fn(Result<FileChange>) -> bool + Send + 'static,
    ) {
        tokio::task::spawn_blocking(move || {
            if self
                .providers
                .iter()
                .find_map(|provider| provider.for_each_changed_file(&cwd, &f).ok())
                .is_none()
            {
                f(Err(anyhow!("no diff provider returns success")));
            }
        });
    }

    /// List staged and unstaged changes in a background task.
    pub fn list_status(
        self,
        cwd: PathBuf,
        f: impl FnOnce(Result<Vec<GitStatusEntry>>) + Send + 'static,
    ) {
        tokio::task::spawn_blocking(move || {
            let result = self
                .providers
                .iter()
                .find_map(|provider| provider.list_status(&cwd).ok())
                .ok_or_else(|| anyhow!("no diff provider returns success"));
            f(result);
        });
    }

    /// Stage a single file in a background task.
    pub fn stage_file(
        self,
        cwd: PathBuf,
        path: PathBuf,
        f: impl FnOnce(Result<()>) + Send + 'static,
    ) {
        tokio::task::spawn_blocking(move || {
            f(self.stage_file_sync(&cwd, &path));
        });
    }

    /// Stage all changes in a background task.
    pub fn stage_all(self, cwd: PathBuf, f: impl FnOnce(Result<()>) + Send + 'static) {
        tokio::task::spawn_blocking(move || {
            f(self.stage_all_sync(&cwd));
        });
    }

    /// Unstage a single file in a background task.
    pub fn unstage_file(
        self,
        cwd: PathBuf,
        path: PathBuf,
        f: impl FnOnce(Result<()>) + Send + 'static,
    ) {
        tokio::task::spawn_blocking(move || {
            f(self.unstage_file_sync(&cwd, &path));
        });
    }

    /// Commit staged changes in a background task.
    pub fn commit(
        self,
        cwd: PathBuf,
        message: String,
        f: impl FnOnce(Result<()>) + Send + 'static,
    ) {
        tokio::task::spawn_blocking(move || {
            f(self.commit_sync(&cwd, &message));
        });
    }

    /// Get unified diff for a file in a background task.
    pub fn file_diff(
        self,
        cwd: PathBuf,
        path: PathBuf,
        section: StagingSection,
        untracked: bool,
        f: impl FnOnce(Result<String>) + Send + 'static,
    ) {
        tokio::task::spawn_blocking(move || {
            f(self.file_diff_sync(&cwd, &path, section, untracked));
        });
    }

    fn stage_file_sync(&self, cwd: &Path, path: &Path) -> Result<()> {
        self.providers
            .iter()
            .find_map(|provider| provider.stage_file(cwd, path).ok())
            .ok_or_else(|| anyhow!("no diff provider returns success"))
    }

    fn stage_all_sync(&self, cwd: &Path) -> Result<()> {
        self.providers
            .iter()
            .find_map(|provider| provider.stage_all(cwd).ok())
            .ok_or_else(|| anyhow!("no diff provider returns success"))
    }

    fn unstage_file_sync(&self, cwd: &Path, path: &Path) -> Result<()> {
        self.providers
            .iter()
            .find_map(|provider| provider.unstage_file(cwd, path).ok())
            .ok_or_else(|| anyhow!("no diff provider returns success"))
    }

    fn commit_sync(&self, cwd: &Path, message: &str) -> Result<()> {
        self.providers
            .iter()
            .find_map(|provider| provider.commit(cwd, message).ok())
            .ok_or_else(|| anyhow!("no diff provider returns success"))
    }

    fn file_diff_sync(
        &self,
        cwd: &Path,
        path: &Path,
        section: StagingSection,
        untracked: bool,
    ) -> Result<String> {
        self.providers
            .iter()
            .find_map(|provider| provider.file_diff(cwd, path, section, untracked).ok())
            .ok_or_else(|| anyhow!("no diff provider returns success"))
    }
}

impl Default for DiffProviderRegistry {
    fn default() -> Self {
        // currently only git is supported
        // TODO make this configurable when more providers are added
        let providers = vec![
            #[cfg(feature = "git")]
            DiffProvider::Git,
            DiffProvider::None,
        ];
        DiffProviderRegistry { providers }
    }
}

/// A union type that includes all types that implement [DiffProvider]. We need this type to allow
/// cloning [DiffProviderRegistry] as `Clone` cannot be used in trait objects.
///
/// `Copy` is simply to ensure the `clone()` call is the simplest it can be.
#[derive(Copy, Clone)]
enum DiffProvider {
    #[cfg(feature = "git")]
    Git,
    None,
}

impl DiffProvider {
    fn get_diff_base(&self, file: &Path) -> Result<Vec<u8>> {
        match self {
            #[cfg(feature = "git")]
            Self::Git => git::get_diff_base(file),
            Self::None => bail!("No diff support compiled in"),
        }
    }

    fn get_current_head_name(&self, file: &Path) -> Result<Arc<ArcSwap<Box<str>>>> {
        match self {
            #[cfg(feature = "git")]
            Self::Git => git::get_current_head_name(file),
            Self::None => bail!("No diff support compiled in"),
        }
    }

    fn for_each_changed_file(
        &self,
        cwd: &Path,
        f: impl Fn(Result<FileChange>) -> bool,
    ) -> Result<()> {
        match self {
            #[cfg(feature = "git")]
            Self::Git => git::for_each_changed_file(cwd, f),
            Self::None => bail!("No diff support compiled in"),
        }
    }

    fn list_status(&self, cwd: &Path) -> Result<Vec<GitStatusEntry>> {
        match self {
            #[cfg(feature = "git")]
            Self::Git => git::list_status(cwd),
            Self::None => bail!("No diff support compiled in"),
        }
    }

    fn stage_file(&self, cwd: &Path, path: &Path) -> Result<()> {
        match self {
            #[cfg(feature = "git")]
            Self::Git => git::stage_file(cwd, path),
            Self::None => bail!("No diff support compiled in"),
        }
    }

    fn stage_all(&self, cwd: &Path) -> Result<()> {
        match self {
            #[cfg(feature = "git")]
            Self::Git => git::stage_all(cwd),
            Self::None => bail!("No diff support compiled in"),
        }
    }

    fn unstage_file(&self, cwd: &Path, path: &Path) -> Result<()> {
        match self {
            #[cfg(feature = "git")]
            Self::Git => git::unstage_file(cwd, path),
            Self::None => bail!("No diff support compiled in"),
        }
    }

    fn commit(&self, cwd: &Path, message: &str) -> Result<()> {
        match self {
            #[cfg(feature = "git")]
            Self::Git => git::commit(cwd, message),
            Self::None => bail!("No diff support compiled in"),
        }
    }

    fn file_diff(
        &self,
        cwd: &Path,
        path: &Path,
        section: StagingSection,
        untracked: bool,
    ) -> Result<String> {
        match self {
            #[cfg(feature = "git")]
            Self::Git => git::file_diff(cwd, path, section, untracked),
            Self::None => bail!("No diff support compiled in"),
        }
    }
}
