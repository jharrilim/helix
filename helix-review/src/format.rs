use crate::{ReviewComment, ReviewData};

/// Format a review as markdown suitable for agent submission.
pub fn format_review_for_llm(review: &ReviewData) -> String {
    let mut out = String::from("# Code Review\n\n");

    if !review.metadata.title.is_empty() {
        out.push_str("## Title\n");
        out.push_str(&review.metadata.title);
        out.push_str("\n\n");
    }

    if !review.metadata.summary.is_empty() {
        out.push_str("## Summary\n");
        out.push_str(&review.metadata.summary);
        out.push_str("\n\n");
    }

    if review.comments.is_empty() {
        out.push_str("_No line comments._\n");
        return out;
    }

    out.push_str("## Comments\n\n");

    let repo_root = &review.metadata.repo_root;
    for comment in &review.comments {
        append_comment(&mut out, comment, repo_root);
    }

    out
}

fn append_comment(out: &mut String, comment: &ReviewComment, repo_root: &std::path::Path) {
    let display_path = comment
        .file
        .strip_prefix(repo_root)
        .unwrap_or(&comment.file)
        .display();
    let line_one_based = comment.line + 1;
    out.push_str(&format!("### {display_path}:{line_one_based}\n"));
    out.push_str(&format!("> {}\n\n", comment.body));

    if let Some(side) = comment.diff_side {
        out.push_str(&format!("Diff: `{side:?}`"));
        if let Some(hunk) = comment.hunk_index {
            out.push_str(&format!(", hunk {hunk}"));
        }
        out.push('\n');
    }

    out.push_str("Context:\n```\n");
    for line in &comment.context_before {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&comment.code_at_comment);
    out.push_str("  // <-- commented line\n");
    for line in &comment.context_after {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("```\n\n");
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{DiffSide, ReviewMetadata, ReviewStatus};

    #[test]
    fn format_review_includes_comments() {
        let review = ReviewData {
            metadata: ReviewMetadata {
                id: "1".into(),
                repo_root: PathBuf::from("/repo"),
                title: "Test review".into(),
                status: ReviewStatus::Draft,
                created_at: "1".into(),
                submitted_at: None,
                summary: "Looks good overall".into(),
            },
            comments: vec![ReviewComment {
                id: "c1".into(),
                file: PathBuf::from("/repo/src/main.rs"),
                line: 9,
                line_end: None,
                char_idx: 0,
                body: "Rename this variable".into(),
                context_before: vec!["fn main() {".into()],
                context_after: vec!["}".into()],
                code_at_comment: "    let x = 1;".into(),
                diff_side: Some(DiffSide::Modified),
                hunk_index: Some(0),
                created_at: "1".into(),
            }],
        };

        let formatted = format_review_for_llm(&review);
        assert!(formatted.contains("# Code Review"));
        assert!(formatted.contains("Rename this variable"));
        assert!(formatted.contains("src/main.rs:10"));
        assert!(formatted.contains("Looks good overall"));
    }
}
