use anyhow::{bail, Context, Result};
use arc_swap::ArcSwap;
use gix::filter::plumbing::driver::apply::Delay;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use gix::bstr::ByteSlice;
use gix::diff::Rewrites;
use gix::dir::entry::Status;
use gix::objs::tree::EntryKind;
use gix::sec::trust::DefaultForLevel;
use gix::status::{
    index_worktree::Item,
    plumbing::index_as_worktree::{Change, EntryStatus},
    Item as StatusItem, UntrackedFiles,
};
use gix::{Commit, ObjectId, Repository, ThreadSafeRepository};

use crate::{FileChange, GitStatusEntry, StagingSection};

mod ops;

pub use ops::{commit, file_diff, stage_all, stage_file, unstage_file};

#[cfg(test)]
mod test;

#[inline]
fn get_repo_dir(file: &Path) -> Result<&Path> {
    file.parent().context("file has no parent directory")
}

pub fn get_diff_base(file: &Path) -> Result<Vec<u8>> {
    debug_assert!(!file.exists() || file.is_file());
    debug_assert!(file.is_absolute());
    let file = gix::path::realpath(file).context("resolve symlinks")?;

    // TODO cache repository lookup

    let repo_dir = get_repo_dir(&file)?;
    let repo = open_repo(repo_dir)
        .context("failed to open git repo")?
        .to_thread_local();
    let head = repo.head_commit()?;
    let file_oid = find_file_in_commit(&repo, &head, &file)?;

    let file_object = repo.find_object(file_oid)?;
    let data = file_object.detach().data;
    // Get the actual data that git would make out of the git object.
    // This will apply the user's git config or attributes like crlf conversions.
    if let Some(work_dir) = repo.workdir() {
        let rela_path = file.strip_prefix(work_dir)?;
        let rela_path = gix::path::try_into_bstr(rela_path)?;
        let (mut pipeline, _) = repo.filter_pipeline(None)?;
        let mut worktree_outcome =
            pipeline.convert_to_worktree(&data, rela_path.as_ref(), Delay::Forbid)?;
        let mut buf = Vec::with_capacity(data.len());
        worktree_outcome.read_to_end(&mut buf)?;
        Ok(buf)
    } else {
        Ok(data)
    }
}

pub fn get_current_head_name(path: &Path) -> Result<Arc<ArcSwap<Box<str>>>> {
    debug_assert!(path.is_absolute());
    let discover_from = if path.is_dir() {
        path.to_path_buf()
    } else {
        debug_assert!(!path.exists() || path.is_file());
        let file = gix::path::realpath(path).context("resolve symlinks")?;
        get_repo_dir(&file)?.to_path_buf()
    };
    let repo = open_repo(&discover_from)
        .context("failed to open git repo")?
        .to_thread_local();
    let head_ref = repo.head_ref()?;
    let head_commit = repo.head_commit()?;

    let name = match head_ref {
        Some(reference) => reference.name().shorten().to_string(),
        None => head_commit.id.to_hex_with_len(8).to_string(),
    };

    Ok(Arc::new(ArcSwap::from_pointee(name.into_boxed_str())))
}

pub fn for_each_changed_file(cwd: &Path, f: impl Fn(Result<FileChange>) -> bool) -> Result<()> {
    unstaged_status(&open_repo(cwd)?.to_thread_local(), f)
}

pub fn list_status(cwd: &Path) -> Result<Vec<GitStatusEntry>> {
    let repo = open_repo(cwd)?.to_thread_local();
    let mut entries = Vec::new();
    full_status(&repo, |entry| {
        entries.push(entry);
        true
    })?;
    Ok(entries)
}

fn open_repo(path: &Path) -> Result<ThreadSafeRepository> {
    // custom open options
    let mut git_open_opts_map = gix::sec::trust::Mapping::<gix::open::Options>::default();

    // On windows various configuration options are bundled as part of the installations
    // This path depends on the install location of git and therefore requires some overhead to lookup
    // This is basically only used on windows and has some overhead hence it's disabled on other platforms.
    // `gitoxide` doesn't use this as default
    let config = gix::open::permissions::Config {
        system: true,
        git: true,
        user: true,
        env: true,
        includes: true,
        git_binary: cfg!(windows),
    };
    // change options for config permissions without touching anything else
    git_open_opts_map.reduced = git_open_opts_map
        .reduced
        .permissions(gix::open::Permissions {
            config,
            ..gix::open::Permissions::default_for_level(gix::sec::Trust::Reduced)
        });
    git_open_opts_map.full = git_open_opts_map.full.permissions(gix::open::Permissions {
        config,
        ..gix::open::Permissions::default_for_level(gix::sec::Trust::Full)
    });

    let open_options = gix::discover::upwards::Options {
        dot_git_only: true,
        ..Default::default()
    };

    let res = ThreadSafeRepository::discover_with_environment_overrides_opts(
        path,
        open_options,
        git_open_opts_map,
    )?;

    Ok(res)
}

/// Emulates the result of running `git status` from the command line (unstaged only).
fn unstaged_status(repo: &Repository, f: impl Fn(Result<FileChange>) -> bool) -> Result<()> {
    collect_index_worktree(repo, f)
}

/// Collect staged and unstaged changes like `git status`.
fn full_status(repo: &Repository, mut f: impl FnMut(GitStatusEntry) -> bool) -> Result<()> {
    let work_dir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("working tree not found"))?
        .to_path_buf();

    let status_platform = repo
        .status(gix::progress::Discard)?
        .untracked_files(UntrackedFiles::Files)
        .index_worktree_rewrites(Some(Rewrites {
            copies: None,
            percentage: Some(0.5),
            limit: 1000,
            ..Default::default()
        }))
        .tree_index_track_renames(gix::status::tree_index::TrackRenames::Given(Rewrites {
            copies: None,
            percentage: Some(0.5),
            limit: 1000,
            ..Default::default()
        }));

    let empty_patterns = vec![];
    let status_iter = status_platform.into_iter(empty_patterns)?;

    for item in status_iter {
        let Ok(item) = item else {
            continue;
        };
        let entry = match item {
            StatusItem::IndexWorktree(item) => {
                map_index_worktree_item(&work_dir, item).map(|change| GitStatusEntry {
                    change,
                    section: StagingSection::Unstaged,
                })
            }
            StatusItem::TreeIndex(change) => {
                map_tree_index_change(&work_dir, change).map(|change| GitStatusEntry {
                    change,
                    section: StagingSection::Staged,
                })
            }
        };
        let Some(entry) = entry else {
            continue;
        };
        if !f(entry) {
            break;
        }
    }

    Ok(())
}

fn collect_index_worktree(
    repo: &Repository,
    mut f: impl FnMut(Result<FileChange>) -> bool,
) -> Result<()> {
    let work_dir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("working tree not found"))?
        .to_path_buf();

    let status_platform = repo
        .status(gix::progress::Discard)?
        .untracked_files(UntrackedFiles::Files)
        .index_worktree_rewrites(Some(Rewrites {
            copies: None,
            percentage: Some(0.5),
            limit: 1000,
            ..Default::default()
        }));

    let empty_patterns = vec![];
    let status_iter = status_platform.into_index_worktree_iter(empty_patterns)?;

    for item in status_iter {
        let Ok(item) = item.map_err(|err| f(Err(err.into()))) else {
            continue;
        };
        let Some(change) = map_index_worktree_item(&work_dir, item) else {
            continue;
        };
        if !f(Ok(change)) {
            break;
        }
    }

    Ok(())
}

fn map_index_worktree_item(work_dir: &Path, item: Item) -> Option<FileChange> {
    match item {
        Item::Modification {
            rela_path, status, ..
        } => {
            let path = work_dir.join(rela_path.to_path().ok()?);
            Some(match status {
                EntryStatus::Conflict { .. } => FileChange::Conflict { path },
                EntryStatus::Change(Change::Removed) => FileChange::Deleted { path },
                EntryStatus::Change(Change::Modification { .. }) => FileChange::Modified { path },
                EntryStatus::IntentToAdd => FileChange::Untracked { path },
                _ => return None,
            })
        }
        Item::DirectoryContents { entry, .. } if entry.status == Status::Untracked => {
            Some(FileChange::Untracked {
                path: work_dir.join(entry.rela_path.to_path().ok()?),
            })
        }
        Item::Rewrite {
            source,
            dirwalk_entry,
            ..
        } => Some(FileChange::Renamed {
            from_path: work_dir.join(source.rela_path().to_path().ok()?),
            to_path: work_dir.join(dirwalk_entry.rela_path.to_path().ok()?),
        }),
        _ => None,
    }
}

fn map_tree_index_change(
    work_dir: &Path,
    change: gix::diff::index::Change,
) -> Option<FileChange> {
    use gix::diff::index::ChangeRef;
    match change {
        ChangeRef::Addition { location, .. } => {
            let path = work_dir.join(location.to_path().ok()?);
            Some(FileChange::Untracked { path })
        }
        ChangeRef::Deletion { location, .. } => {
            let path = work_dir.join(location.to_path().ok()?);
            Some(FileChange::Deleted { path })
        }
        ChangeRef::Modification { location, .. } => {
            let path = work_dir.join(location.to_path().ok()?);
            Some(FileChange::Modified { path })
        }
        ChangeRef::Rewrite {
            source_location,
            location,
            ..
        } => Some(FileChange::Renamed {
            from_path: work_dir.join(source_location.to_path().ok()?),
            to_path: work_dir.join(location.to_path().ok()?),
        }),
    }
}
/// Finds the object that contains the contents of a file at a specific commit.
fn find_file_in_commit(repo: &Repository, commit: &Commit, file: &Path) -> Result<ObjectId> {
    let repo_dir = repo.workdir().context("repo has no worktree")?;
    let rel_path = file.strip_prefix(repo_dir)?;
    let tree = commit.tree()?;
    let tree_entry = tree
        .lookup_entry_by_path(rel_path)?
        .context("file is untracked")?;
    match tree_entry.mode().kind() {
        // not a file, everything is new, do not show diff
        mode @ (EntryKind::Tree | EntryKind::Commit | EntryKind::Link) => {
            bail!("entry at {} is not a file but a {mode:?}", file.display())
        }
        // found a file
        EntryKind::Blob | EntryKind::BlobExecutable => Ok(tree_entry.object_id()),
    }
}
