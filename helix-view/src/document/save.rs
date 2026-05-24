use anyhow::{bail};
use helix_stdx::faccess::{copy_metadata, readonly};
use std::future::Future;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::SystemTime;

use super::{Document, DocumentSavedEvent};
use super::encoding_io::to_writer;

impl Document {
    pub fn save<P: Into<PathBuf>>(
        &mut self,
        path: Option<P>,
        force: bool,
    ) -> Result<
        impl Future<Output = Result<DocumentSavedEvent, anyhow::Error>> + 'static + Send,
        anyhow::Error,
    > {
        let path = path.map(|path| path.into());
        self.save_impl(path, force)

        // futures_util::future::Ready<_>,
    }

    /// The `Document`'s text is encoded according to its encoding and written to the file located
    /// at its `path()`.
    fn save_impl(
        &mut self,
        path: Option<PathBuf>,
        force: bool,
    ) -> Result<
        impl Future<Output = Result<DocumentSavedEvent, anyhow::Error>> + 'static + Send,
        anyhow::Error,
    > {
        log::debug!(
            "submitting save of doc '{:?}'",
            self.path().map(|path| path.to_string_lossy())
        );

        // we clone and move text + path into the future so that we asynchronously save the current
        // state without blocking any further edits.
        let text = self.text().clone();

        let path = match path {
            Some(path) => helix_stdx::path::canonicalize(path),
            None => {
                if self.path.is_none() {
                    bail!("Can't save with no path set!");
                }
                self.path.as_ref().unwrap().clone()
            }
        };

        let identifier = self.path().map(|_| self.identifier());
        let language_servers: Vec<_> = self.language_servers.values().cloned().collect();

        // mark changes up to now as saved
        let current_rev = self.get_current_revision();
        let doc_id = self.id();
        let atomic_save = self.config.load().atomic_save;

        let encoding_with_bom_info = (self.encoding, self.has_bom);
        let last_saved_time = self.last_saved_time;

        // We encode the file according to the `Document`'s encoding.
        let future = async move {
            use tokio::fs;
            if let Some(parent) = path.parent() {
                // TODO: display a prompt asking the user if the directories should be created
                if !parent.exists() {
                    if force {
                        std::fs::DirBuilder::new().recursive(true).create(parent)?;
                    } else {
                        bail!("can't save file, parent directory does not exist (use :w! to create it)");
                    }
                }
            }

            // Protect against overwriting changes made externally
            if !force {
                if let Ok(metadata) = fs::metadata(&path).await {
                    if let Ok(mtime) = metadata.modified() {
                        if last_saved_time < mtime {
                            bail!("file modified by an external process, use :w! to overwrite");
                        }
                    }
                }
            }
            let write_path = tokio::fs::read_link(&path)
                .await
                .ok()
                .and_then(|p| {
                    if p.is_relative() {
                        path.parent().map(|parent| parent.join(p))
                    } else {
                        Some(p)
                    }
                })
                .unwrap_or_else(|| path.clone());

            if readonly(&write_path) {
                bail!(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Path is read only"
                ));
            }

            // Assume it is a hardlink to prevent data loss if the metadata cant be read (e.g. on certain Windows configurations)
            let is_hardlink = helix_stdx::faccess::hardlink_count(&write_path).unwrap_or(2) > 1;
            let is_symlink = match tokio::fs::symlink_metadata(&write_path).await {
                Ok(meta) => meta.is_symlink(),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
                Err(err) => return Err(err.into()),
            };
            let must_copy = is_hardlink || is_symlink;
            let backup = if path.exists() && atomic_save {
                let path_ = write_path.clone();
                // hacks: we use tempfile to handle the complex task of creating
                // non clobbered temporary path for us we don't want
                // the whole automatically delete path on drop thing
                // since the path doesn't exist yet, we just want
                // the path
                tokio::task::spawn_blocking(move || -> Option<PathBuf> {
                    let mut builder = tempfile::Builder::new();
                    builder.prefix(path_.file_name()?).suffix(".bck");

                    let backup_path = if must_copy {
                        builder
                            .make_in(path_.parent()?, |backup| std::fs::copy(&path_, backup))
                            .ok()?
                            .into_temp_path()
                    } else {
                        builder
                            .make_in(path_.parent()?, |backup| std::fs::rename(&path_, backup))
                            .ok()?
                            .into_temp_path()
                    };

                    backup_path.keep().ok()
                })
                .await
                .ok()
                .flatten()
            } else {
                None
            };

            let write_result: anyhow::Result<_> = async {
                let mut dst = tokio::fs::File::create(&write_path).await?;
                to_writer(&mut dst, encoding_with_bom_info, &text).await?;
                // Ignore ENOTSUP/EOPNOTSUPP (Operation not supported) errors from sync_all()
                // This is known to occur on SMB filesystems on macOS where fsync is not supported
                match dst.sync_all().await {
                    Ok(_) => (),
                    Err(err) if err.kind() == ErrorKind::Unsupported => (),
                    // Some extra OS errors are thrown on macOS for example if fsync is not
                    // available for this filesystem. NOTE: on macOS, ENOTSUP and EOPNOTSUPP are
                    // not the same code, so we need to suppress the unreachable_patterns lint on
                    // Unix generally.
                    #[allow(unreachable_patterns)]
                    #[cfg(unix)]
                    Err(err)
                        if matches!(err.raw_os_error(), Some(libc::ENOTSUP | libc::EOPNOTSUPP)) => {
                    }
                    Err(err) => return Err(err.into()),
                }
                Ok(())
            }
            .await;

            let save_time = match fs::metadata(&write_path).await {
                Ok(metadata) => metadata.modified().map_or(SystemTime::now(), |mtime| mtime),
                Err(_) => SystemTime::now(),
            };

            if let Some(backup) = backup {
                if must_copy {
                    let mut delete = true;
                    if write_result.is_err() {
                        // Restore backup
                        let _ = tokio::fs::copy(&backup, &write_path).await.map_err(|e| {
                            delete = false;
                            log::error!("Failed to restore backup on write failure: {e}")
                        });
                    }

                    if delete {
                        // Delete backup
                        let _ = tokio::fs::remove_file(backup)
                            .await
                            .map_err(|e| log::error!("Failed to remove backup file on write: {e}"));
                    }
                } else if write_result.is_err() {
                    // restore backup
                    let _ = tokio::fs::rename(&backup, &write_path)
                        .await
                        .map_err(|e| log::error!("Failed to restore backup on write failure: {e}"));
                } else {
                    // copy metadata and delete backup
                    let _ = tokio::task::spawn_blocking(move || {
                        let _ = copy_metadata(&backup, &write_path)
                            .map_err(|e| log::error!("Failed to copy metadata on write: {e}"));
                        let _ = std::fs::remove_file(backup)
                            .map_err(|e| log::error!("Failed to remove backup file on write: {e}"));
                    })
                    .await;
                }
            }

            write_result?;

            let event = DocumentSavedEvent {
                revision: current_rev,
                save_time,
                doc_id,
                path,
                text: text.clone(),
            };

            for language_server in language_servers {
                if !language_server.is_initialized() {
                    continue;
                }
                if let Some(id) = identifier.clone() {
                    language_server.text_document_did_save(id, &text);
                }
            }

            Ok(event)
        };

        Ok(future)
    }

}
