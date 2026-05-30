use std::path::Path;

use helix_core::{Assoc, ChangeSet, Rope};

use crate::{paths::paths_equal, ReviewComment};

/// Remap comment anchors after a buffer edit.
pub fn map_comments_for_changes(
    comments: &mut [ReviewComment],
    changes: &ChangeSet,
    text: &Rope,
) {
    for comment in comments {
        changes.update_positions([(&mut comment.char_idx, Assoc::After)].into_iter());
        comment.line = text.char_to_line(comment.char_idx);
    }
}

/// Remap session comments anchored to a specific file.
pub fn map_session_comments_for_file(
    comments: &mut [ReviewComment],
    file: &Path,
    repo_root: &Path,
    changes: &ChangeSet,
    text: &Rope,
) {
    for comment in comments.iter_mut() {
        if paths_equal(&comment.file, file, repo_root) {
            changes.update_positions([(&mut comment.char_idx, Assoc::After)].into_iter());
            comment.line = text.char_to_line(comment.char_idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use helix_core::{Rope, Transaction};

    use super::*;
    use crate::ReviewComment;

    #[test]
    fn maps_comment_line_after_insert() {
        let mut text = Rope::from("a\nb\nc\n");
        let mut comments = vec![ReviewComment {
            id: "c1".into(),
            file: PathBuf::from("/repo/a.rs"),
            line: 2,
            line_end: None,
            char_idx: text.line_to_char(2),
            body: "note".into(),
            context_before: Vec::new(),
            context_after: Vec::new(),
            code_at_comment: "c".into(),
            diff_side: None,
            hunk_index: None,
            created_at: "1".into(),
            author: crate::CommentAuthor::User,
        }];

        let transaction = Transaction::change(
            &text,
            vec![(text.line_to_char(0), 0, Some("x\n".into()))].into_iter(),
        );
        let changes = transaction.changes().clone();
        changes.apply(&mut text);

        map_comments_for_changes(&mut comments, &changes, &text);
        assert_eq!(comments[0].line, 3);
    }
}
