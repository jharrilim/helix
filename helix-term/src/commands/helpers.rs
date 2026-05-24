use std::{future::Future, io::Read, num::NonZeroUsize, path::Path};

use helix_core::{movement::Movement, Range, RopeSlice};
use helix_event::status;
use helix_vcs::Hunk;
use helix_view::{
    document::Mode,
    editor::Action,
    view::View,
    Document, Editor,
};
use url::Url;

use crate::{
    compositor::Compositor,
    job::{self, Callback},
    ui::overlay::overlaid,
};

use super::Context;

#[inline]
pub(crate) fn make_job_callback<T, F>(
    call: impl Future<Output = helix_lsp::Result<T>> + 'static + Send,
    callback: F,
) -> std::pin::Pin<Box<impl Future<Output = Result<Callback, anyhow::Error>>>>
where
    T: Send + 'static,
    F: FnOnce(&mut Editor, &mut Compositor, T) + Send + 'static,
{
    Box::pin(async move {
        let response = call.await?;
        let call: job::Callback = Callback::EditorCompositor(Box::new(
            move |editor: &mut Editor, compositor: &mut Compositor| {
                callback(editor, compositor, response)
            },
        ));
        Ok(call)
    })
}

/// Opens the given url. If the URL points to a valid textual file it is open in helix.
/// Otherwise, the file is open using external program.
pub(crate) fn open_url(cx: &mut Context, url: Url, action: Action) {
    let doc = doc!(cx.editor);
    let rel_path = doc
        .relative_path()
        .map(|path| path.parent().unwrap().to_path_buf())
        .unwrap_or_default();

    if should_open_url_externally(&url) {
        return cx.jobs.callback(crate::open_external_url_callback(url));
    }

    let path = &rel_path.join(url.path());
    if path.is_dir() {
        let picker = crate::ui::file_picker(cx.editor, path.into());
        cx.push_layer(Box::new(overlaid(picker)));
    } else if let Err(e) = cx.editor.open(path, action) {
        cx.editor.set_error(format!("Open file failed: {:?}", e));
    }
}

/// Open a URL from an editor/compositor callback.
pub(crate) fn open_url_in_callback(
    editor: &mut Editor,
    compositor: &mut Compositor,
    url: Url,
    action: Action,
    rel_path: &Path,
) {
    if should_open_url_externally(&url) {
        tokio::spawn(async move {
            match crate::open_external_url_callback(url).await {
                Ok(callback) => job::dispatch_callback(callback).await,
                Err(err) => status::report(err).await,
            }
        });
        return;
    }

    let path = &rel_path.join(url.path());
    if path.is_dir() {
        let picker = crate::ui::file_picker(editor, path.into());
        compositor.push(Box::new(overlaid(picker)));
    } else if let Err(e) = editor.open(path, action) {
        editor.set_error(format!("Open file failed: {:?}", e));
    }
}

pub(crate) fn should_open_url_externally(url: &Url) -> bool {
    if url.scheme() != "file" {
        return true;
    }

    let content_type = std::fs::File::open(url.path()).and_then(|file| {
        let mut read_buffer = Vec::new();
        let n = file.take(1024).read_to_end(&mut read_buffer)?;
        Ok(content_inspector::inspect(&read_buffer[..n]))
    });

    matches!(content_type, Ok(content_inspector::ContentType::BINARY))
}

pub(crate) fn enter_insert_mode(cx: &mut Context) {
    cx.editor.mode = Mode::Insert;
}

pub(crate) fn push_jump(view: &mut View, doc: &mut Document) {
    doc.append_changes_to_history(view);
    let jump = (doc.id(), doc.selection(view.id).clone());
    view.jumps.push(jump);
}

pub(crate) fn goto_line_without_jumplist(
    editor: &mut Editor,
    count: Option<NonZeroUsize>,
    movement: Movement,
) {
    if let Some(count) = count {
        let (view, doc) = current!(editor);
        let text = doc.text().slice(..);
        let max_line = if text.line(text.len_lines() - 1).len_chars() == 0 {
            text.len_lines().saturating_sub(2)
        } else {
            text.len_lines() - 1
        };
        let line_idx = std::cmp::min(count.get() - 1, max_line);
        let pos = text.line_to_char(line_idx);
        let selection = doc
            .selection(view.id)
            .clone()
            .transform(|range| range.put_cursor(text, pos, movement == Movement::Extend));

        doc.set_selection(view.id, selection);
    }
}

pub(crate) fn exit_select_mode(cx: &mut Context) {
    if cx.editor.mode == Mode::Select {
        cx.editor.mode = Mode::Normal;
    }
}

pub(crate) fn hunk_range(hunk: Hunk, text: RopeSlice) -> Range {
    let anchor = text.line_to_char(hunk.after.start as usize);
    let head = if hunk.after.is_empty() {
        anchor + 1
    } else {
        text.line_to_char(hunk.after.end as usize)
    };

    Range::new(anchor, head)
}
