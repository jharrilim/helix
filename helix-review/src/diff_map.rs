//! Parse unified diff text into a line map for review comments on diff buffers.

use std::path::{Path, PathBuf};

use crate::DiffSide;

/// Mapping from a line in a unified diff buffer to source coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLineMapping {
    pub source_path: PathBuf,
    pub source_line: usize,
    pub side: DiffSide,
}

/// Metadata attached to a git diff scratch buffer.
#[derive(Debug, Clone)]
pub struct DiffReviewSource {
    pub source_path: PathBuf,
    /// Per diff-buffer line (0-based): mapping to source file coordinates.
    pub line_map: Vec<Option<DiffLineMapping>>,
}

/// Parse a unified diff string into per-line source mappings.
pub fn parse_unified_diff_line_map(source_path: &Path, diff: &str) -> DiffReviewSource {
    let mut line_map = Vec::new();
    let mut old_line: isize = 0;
    let mut new_line: isize = 0;
    let mut in_hunk = false;

    for line in diff.lines() {
        if line.starts_with("@@ ") {
            if let Some((old, new)) = parse_hunk_header(line) {
                old_line = old;
                new_line = new;
                in_hunk = true;
            }
            line_map.push(None);
            continue;
        }

        if !in_hunk {
            line_map.push(None);
            continue;
        }

        let Some(first) = line.chars().next() else {
            line_map.push(None);
            continue;
        };

        match first {
            ' ' => {
                let source_line = new_line.max(0) as usize;
                line_map.push(Some(DiffLineMapping {
                    source_path: source_path.to_path_buf(),
                    source_line,
                    side: DiffSide::Unchanged,
                }));
                old_line += 1;
                new_line += 1;
            }
            '+' => {
                let source_line = new_line.max(0) as usize;
                line_map.push(Some(DiffLineMapping {
                    source_path: source_path.to_path_buf(),
                    source_line,
                    side: DiffSide::Added,
                }));
                new_line += 1;
            }
            '-' => {
                let source_line = old_line.max(0) as usize;
                line_map.push(Some(DiffLineMapping {
                    source_path: source_path.to_path_buf(),
                    source_line,
                    side: DiffSide::Removed,
                }));
                old_line += 1;
            }
            '\\' => {
                line_map.push(None);
            }
            _ => {
                line_map.push(None);
            }
        }
    }

    DiffReviewSource {
        source_path: source_path.to_path_buf(),
        line_map,
    }
}

fn parse_hunk_header(line: &str) -> Option<(isize, isize)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let old = parse_hunk_side(parts[1].trim_start_matches('-'))?;
    let new = parse_hunk_side(parts[2].trim_start_matches('+'))?;
    Some((old, new))
}

fn parse_hunk_side(spec: &str) -> Option<isize> {
    let (start, _count) = spec.split_once(',').unwrap_or((spec, "1"));
    start.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_addition_and_context_lines() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,4 @@
 fn main() {
-    let x = 1;
+    let x = 2;
+    let y = 3;
 }
";
        let source = PathBuf::from("/repo/foo.rs");
        let map = parse_unified_diff_line_map(&source, diff);

        assert!(map.line_map.len() >= 5);
        let added = map
            .line_map
            .iter()
            .flatten()
            .find(|entry| entry.side == DiffSide::Added)
            .expect("expected an added line");
        assert_eq!(added.source_path, source);
    }

    #[test]
    fn parses_simple_addition() {
        let diff = "\
--- a/foo.rs
+++ b/foo.rs
@@ -1 +1,2 @@
 line
+added
";
        let map = parse_unified_diff_line_map(Path::new("/repo/foo.rs"), diff);
        let sides: Vec<_> = map.line_map.iter().flatten().map(|entry| entry.side).collect();
        assert!(sides.contains(&DiffSide::Added));
    }
}
