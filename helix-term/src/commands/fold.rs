use helix_core::{
    discover_fold_at_cursor, discover_folds, syntax::Loader, FoldRange, FoldState,
};
use helix_view::{Document, ViewId};

use super::Context;

fn ensure_cursor_not_in_fold(doc: &mut Document, view_id: ViewId) {
    let text = doc.text().slice(..);
    let folds = doc.folds(view_id);
    if folds.is_empty() {
        return;
    }
    let selection = doc.selection(view_id).clone().transform(|range| {
        let cursor = helix_core::fold::char_idx_for_display(Some(folds), text, range.cursor(text));
        range.put_cursor(text, cursor, false)
    });
    doc.set_selection(view_id, selection);
}

fn fold_at_cursor(doc: &Document, view_id: ViewId, loader: &Loader) -> Option<FoldRange> {
    let syntax = doc.syntax()?;
    let text = doc.text().slice(..);
    let cursor = doc.selection(view_id).primary().cursor(text);
    let line = text.char_to_line(cursor);

    let available = discover_folds(syntax, loader, text);
    if let Some(range) = FoldState::innermost_containing(available.iter(), line) {
        return Some(range.clone());
    }

    discover_fold_at_cursor(syntax, loader, text, cursor)
}

pub fn fold_toggle(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let loader = cx.editor.syn_loader.load();
    let Some(range) = fold_at_cursor(doc, view.id, &loader) else {
        cx.editor
            .set_error("No fold at cursor (tree-sitter folds.scm required)");
        return;
    };
    doc.folds_mut(view.id).toggle(range);
    ensure_cursor_not_in_fold(doc, view.id);
}

pub fn fold_open(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let loader = cx.editor.syn_loader.load();
    let Some(range) = fold_at_cursor(doc, view.id, &loader) else {
        cx.editor
            .set_error("No fold at cursor (tree-sitter folds.scm required)");
        return;
    };
    doc.folds_mut(view.id).expand(&range);
}

pub fn fold_close(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let loader = cx.editor.syn_loader.load();
    let Some(range) = fold_at_cursor(doc, view.id, &loader) else {
        cx.editor
            .set_error("No fold at cursor (tree-sitter folds.scm required)");
        return;
    };
    let len_lines = doc.text().len_lines();
    if let Some(range) = FoldState::clamp_range(range, len_lines) {
        doc.folds_mut(view.id).collapse(range);
        ensure_cursor_not_in_fold(doc, view.id);
    }
}

pub fn fold_open_all(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    doc.folds_mut(view.id).clear();
}

pub fn fold_close_all(cx: &mut Context) {
    let (view, doc) = current!(cx.editor);
    let Some(syntax) = doc.syntax() else {
        cx.editor
            .set_error("Syntax tree is not available on this buffer");
        return;
    };
    let text = doc.text().slice(..);
    let loader = cx.editor.syn_loader.load();
    let ranges = discover_folds(syntax, &loader, text);
    if ranges.is_empty() {
        cx.editor
            .set_error("No folds found (tree-sitter folds.scm required)");
        return;
    }
    let len_lines = doc.text().len_lines();
    let ranges: Vec<_> = ranges
        .into_iter()
        .filter_map(|r| FoldState::clamp_range(r, len_lines))
        .collect();
    doc.folds_mut(view.id).collapse_all(ranges);
    ensure_cursor_not_in_fold(doc, view.id);
}
