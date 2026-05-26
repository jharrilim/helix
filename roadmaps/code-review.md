# Code Review Roadmap

Developer-facing roadmap for local, line-anchored code reviews on source files and
git diffs, with submission to the Helix agent panel.

Related: [`helix-acp/ROADMAP.md`](../helix-acp/ROADMAP.md) (agent prompt submission),
[`roadmaps/focused-leaf-refactor.md`](focused-leaf-refactor.md) (panel focus plumbing).

Work top-down by tier. Mark items complete (`[x]`) when merged.

## Data schema

Reviews are stored under `~/.local/share/helix/reviews/<repo-slug>/<review-id>/`:

```
metadata.json   # id, repo_root, title, status, timestamps, summary
review.json     # comments array with line snapshots for LLM use
```

Example `review.json` comment entry:

```json
{
  "id": "c1",
  "file": "helix-term/src/ui/agent.rs",
  "line": 41,
  "line_end": null,
  "body": "Extract this into a helper",
  "context_before": ["fn foo() {", "    let x = 1;"],
  "context_after": ["    bar(x);", "}"],
  "code_at_comment": "    let x = compute();",
  "diff_side": "modified",
  "hunk_index": 2,
  "created_at": "1748187082"
}
```

Conventions:

- **Domain crate:** portable review types, persistence, and formatting live in [`helix-review/`](../helix-review/)
- **repo-slug:** basename of workspace root from `helix_loader::find_workspace()`
- **review-id:** Unix timestamp string (seconds since epoch)
- **line numbers:** 0-based in storage; displayed as 1-based in prompts to the agent
- **snapshots:** each comment stores surrounding lines so reviews stay useful after edits

## User commands

| Command | Keybind (normal) | Description |
|---------|------------------|-------------|
| `:review-toggle` | `Space R t` | Enter/exit review mode |
| `:review-comment` | `Space R c` | Comment on cursor line (review mode) |
| `:review-submit` | `Space R s` | Format review and send to agent |
| `:review-new` | `Space R n` | Start a new review session |
| `:review-list` | `Space R l` | List saved reviews for this repo |
| `:review-summary` | `Space R S` | Set overall review summary text |
| `:review-next` | `Space R ]` | Jump to next comment |
| `:review-prev` | `Space R [` | Jump to previous comment |
| `:review-resume` | `:review-resume <id>` | Resume a saved review |

## Tier 1 — Core review on source files

### State and persistence

- [x] **`ReviewState` on `Editor`** — active flag, current review payload, repo slug
  - Files: [`helix-view/src/review/mod.rs`](../helix-view/src/review/mod.rs),
    [`helix-view/src/editor.rs`](../helix-view/src/editor.rs)
- [x] **`ReviewComment` + serde types** — line snapshots, diff metadata fields
  - Files: [`helix-review/src/types.rs`](../helix-review/src/types.rs)
- [x] **`Document.review_comments`** — per-buffer display cache synced from review session
  - Files: [`helix-view/src/document.rs`](../helix-view/src/document.rs)
- [x] **Disk persistence** — `data_dir/reviews/<repo>/<id>/metadata.json` + `review.json`
  - Files: [`helix-review/src/storage.rs`](../helix-review/src/storage.rs)
- **Acceptance:** Comments persist across Helix restarts under `data_dir/reviews/`

### Virtual-line rendering

- [x] **`ReviewLineAnnotation`** — reserve space below commented lines
  - Files: [`helix-view/src/annotations/review.rs`](../helix-view/src/annotations/review.rs),
    [`helix-view/src/view.rs`](../helix-view/src/view.rs)
- [x] **`ReviewDecoration`** — draw soft-wrapped comment text in virtual lines
  - Files: [`helix-term/src/ui/text_decorations/review.rs`](../helix-term/src/ui/text_decorations/review.rs),
    [`helix-term/src/ui/editor.rs`](../helix-term/src/ui/editor.rs)
- [x] **Position mapping on edit** — update `char_idx` / `line` via `ChangeSet`
  - Files: [`helix-view/src/document/edit.rs`](../helix-view/src/document/edit.rs)
- **Acceptance:** Comments render below their anchor lines and survive buffer edits

### Commands

- [x] **`:review-toggle` / `:review-comment` / `:review-submit`**
- [x] **`:review-new` / `:review-list`**
  - Files: [`helix-term/src/commands/review.rs`](../helix-term/src/commands/review.rs),
    [`helix-term/src/commands/typed/review.rs`](../helix-term/src/commands/typed/review.rs),
    [`helix-term/src/commands/typed/command_list/review.rs`](../helix-term/src/commands/typed/command_list/review.rs)
- [x] **Keymap** — `Space R` prefix in normal mode
  - Files: [`helix-term/src/keymap/default.rs`](../helix-term/src/keymap/default.rs)
- **Acceptance:** Full workflow: toggle mode → comment on lines → submit to agent

## Tier 2 — Diff-aware review UX

- [x] **Hunk metadata on comment** — `hunk_index`, `diff_side` from `DiffHandle::hunk_at`
  - Files: [`helix-view/src/review.rs`](../helix-view/src/review.rs),
    [`helix-term/src/commands/review.rs`](../helix-term/src/commands/review.rs)
- [x] **Gutter marker** — `◆` on lines with review comments
  - Files: [`helix-view/src/gutter.rs`](../helix-view/src/gutter.rs)
- [x] **`:review-next` / `:review-prev`** — cross-file comment navigation
- [x] **`:review-summary`** — optional summary before submit
- [x] **Theme scopes** — `ui.review.comment`, `ui.review.gutter` (fallback to `hint`)
- **Acceptance:** Diff hunks appear in stored JSON; gutter shows comment markers; navigation works

## Tier 3 — Git diff buffer + agent polish

- [x] **Diff buffer line map** — parse `@@` headers when opening git diff buffer
  - Files: [`helix-view/src/review/diff_map.rs`](../helix-view/src/review/diff_map.rs),
    [`helix-term/src/commands/git.rs`](../helix-term/src/commands/git.rs)
- [x] **Comment on diff lines** — map diff buffer line → source coordinates
- [x] **`:review-resume <id>`** — reload saved review session
- [x] **`format_review_for_llm`** — structured markdown with file/line links and code context
  - Files: [`helix-view/src/review.rs`](../helix-view/src/review.rs)
- [x] **Tests** — persistence, formatting, comment sync
  - Files: [`helix-term/tests/test/review.rs`](../helix-term/tests/test/review.rs)
- **Acceptance:** Git diff buffer supports comments; agent receives actionable review text

## Tier 4 — Future (not v1)

- [ ] Threaded replies / resolve status
- [ ] Review export (standalone markdown/JSON)
- [ ] Side-by-side diff virtual lines
- [ ] Multi-reviewer / shared reviews
