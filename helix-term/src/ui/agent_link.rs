//! Link resolution and opening for the agent transcript.

use std::path::{Path, PathBuf};

use helix_core::{pos_at_coords, Position, Selection};
use helix_view::{align_view, editor::Action, Align, Editor};
use url::Url;

use crate::args::parse_file;
use crate::job;
use crate::ui::overlay::overlaid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentLinkTarget {
    External(Url),
    File { path: PathBuf, pos: Position },
}

pub fn resolve_agent_link(href: &str, workspace: &Path) -> Result<AgentLinkTarget, String> {
    if href.starts_with("https://") || href.starts_with("http://") {
        return Url::parse(href)
            .map(AgentLinkTarget::External)
            .map_err(|err| format!("invalid URL: {err}"));
    }

    let (path, pos) = parse_link_href(href);
    let resolved = if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    };

    let canonical = helix_stdx::path::canonicalize(&resolved);
    let workspace_canonical = helix_stdx::path::canonicalize(workspace);

    if !canonical.starts_with(&workspace_canonical) {
        return Err("link is outside project root".to_string());
    }

    if !canonical.exists() {
        return Err(format!("path not found: {}", resolved.display()));
    }

    Ok(AgentLinkTarget::File { path: canonical, pos })
}

fn parse_link_href(href: &str) -> (PathBuf, Position) {
    let (path_part, fragment_pos) = match href.split_once('#') {
        None => (href, Position::default()),
        Some((path, fragment)) => {
            let line = fragment
                .strip_prefix('L')
                .or(Some(fragment))
                .and_then(|value| value.parse::<usize>().ok())
                .map(|line| Position::new(line.saturating_sub(1), 0))
                .unwrap_or_default();
            (path, line)
        }
    };

    let (path, colon_pos) = parse_file(path_part);
    let pos = if colon_pos != Position::default() {
        colon_pos
    } else {
        fragment_pos
    };
    (path, pos)
}

pub fn open_agent_link(editor: &mut Editor, href: &str) {
    let (workspace, _) = helix_loader::find_workspace();
    let target = match resolve_agent_link(href, &workspace) {
        Ok(target) => target,
        Err(message) => {
            editor.set_error(message);
            helix_event::request_redraw();
            return;
        }
    };

    match target {
        AgentLinkTarget::External(url) => {
            let url_display = url.to_string();
            job::dispatch_blocking(move |editor, _compositor| {
                tokio::spawn(async move {
                    match crate::open_external_url_callback(url).await {
                        Ok(callback) => job::dispatch_callback(callback).await,
                        Err(err) => helix_event::status::report(err).await,
                    }
                });
                editor.set_status(format!("opened {url_display}"));
            });
        }
        AgentLinkTarget::File { path, pos } => {
            let display = path.display().to_string();
            if path.is_dir() {
                job::dispatch_blocking(move |editor, compositor| {
                    editor.focus_editor_from_agent();
                    let picker = crate::ui::file_picker(editor, path);
                    compositor.push(Box::new(overlaid(picker)));
                    editor.set_status(format!("opened {display}"));
                });
            } else {
                editor.focus_editor_from_agent();
                if let Err(err) = editor.open(&path, Action::Replace) {
                    editor.set_error(format!("open file failed: {err:?}"));
                    helix_event::request_redraw();
                    return;
                }
                if let Some((view, doc)) = try_current!(editor) {
                    if pos != Position::default() {
                        let selection =
                            Selection::point(pos_at_coords(doc.text().slice(..), pos, true));
                        doc.set_selection(view.id, selection);
                        align_view(doc, view, Align::Center);
                    }
                }
                editor.set_status(format!("opened {display}"));
            }
        }
    }
    helix_event::request_redraw();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn workspace_with_file() -> (TempDir, PathBuf, PathBuf) {
        let dir = TempDir::new().unwrap();
        let root = dir.path().to_path_buf();
        let nested = root.join("src");
        fs::create_dir_all(&nested).unwrap();
        let file = nested.join("foo.rs");
        fs::write(&file, "fn main() {}\n").unwrap();
        (dir, root, file)
    }

    #[test]
    fn resolve_relative_file_within_workspace() {
        let (_dir, root, file) = workspace_with_file();
        let target = resolve_agent_link("src/foo.rs", &root).unwrap();
        assert_eq!(
            target,
            AgentLinkTarget::File {
                path: file,
                pos: Position::default()
            }
        );
    }

    #[test]
    fn resolve_https_url() {
        let (_dir, root, _) = workspace_with_file();
        let target = resolve_agent_link("https://example.com/docs", &root).unwrap();
        assert!(matches!(target, AgentLinkTarget::External(_)));
    }

    #[test]
    fn reject_path_outside_workspace() {
        let (_dir, root, _) = workspace_with_file();
        assert!(resolve_agent_link("../outside.rs", &root).is_err());
    }

    #[test]
    fn resolve_line_fragment() {
        let (_dir, root, file) = workspace_with_file();
        let target = resolve_agent_link("src/foo.rs#L2", &root).unwrap();
        assert_eq!(
            target,
            AgentLinkTarget::File {
                path: file,
                pos: Position::new(1, 0)
            }
        );
    }
}
