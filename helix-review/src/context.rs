use helix_core::RopeSlice;

/// Capture line context around `line` for LLM snapshots.
pub fn capture_line_context(
    text: RopeSlice<'_>,
    line: usize,
    context_lines: usize,
) -> (Vec<String>, String, Vec<String>) {
    let line_count = text.len_lines();
    let before_start = line.saturating_sub(context_lines);
    let after_end = (line + context_lines + 1).min(line_count);

    let context_before: Vec<String> = (before_start..line)
        .map(|l| text.line(l).to_string())
        .collect();
    let code_at_comment = text.line(line.min(line_count.saturating_sub(1))).to_string();
    let context_after: Vec<String> = ((line + 1)..after_end)
        .map(|l| text.line(l).to_string())
        .collect();

    (context_before, code_at_comment, context_after)
}
