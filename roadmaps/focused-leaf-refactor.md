# Focused Leaf Refactor Roadmap

Developer-facing roadmap for generalizing Helix's split-tree focus and input
plumbing so git/agent/terminal panels are first-class leaves without pretending
to be document `View`s.

This is **shared infrastructure** for agent, terminal, and git panels. Feature
roadmaps: [`helix-acp/ROADMAP.md`](../helix-acp/ROADMAP.md),
[`docs/integrated-terminal-roadmap.md`](../docs/integrated-terminal-roadmap.md).

Work top-down by tier. Mark items complete (`[x]`) when merged.

## Tier 1 — Stop panics (Phase 1)

### Tree + Editor safe accessors

- [x] **`LeafKind` enum** — `View | AgentPanel | GitPanel | TerminalPanel` on
  [`helix-view/src/tree.rs`](../helix-view/src/tree.rs)
- [x] **`Content::leaf_kind` / `leaf_area`** — dedupe area match arms
- [x] **`Tree::focused_kind` / `try_focused_view`** — safe focus queries
- [x] **`Editor::is_document_view_focused` / `focused_leaf_kind`**
  - Files: [`helix-view/src/editor/focus.rs`](../helix-view/src/editor/focus.rs)
- **Acceptance:** `try_focused_view()` returns `None` when a panel has tree focus

### Safe macros

- [x] **`try_view!` / `try_current!` / `try_current_ref!`**
  - Files: [`helix-view/src/macros.rs`](../helix-view/src/macros.rs)
- **Acceptance:** panel-focused code paths compile without calling `current!`

### Lifecycle and window commands

- [x] **`:quit` / `:q!` with git panel focused** — mirror agent/terminal branches
- [x] **`:quit-all` closes git panel**
- [x] **`wclose` handles git panel**
- [x] **`split` guards all panel kinds**
  - Files: [`helix-term/src/commands/typed/lifecycle.rs`](../helix-term/src/commands/typed/lifecycle.rs),
    [`helix-term/src/commands/view.rs`](../helix-term/src/commands/view.rs)
- **Acceptance:** no panics from `view!`/`current!` when any panel is focused

### Panel keymap passthrough

- [x] **Restrict space passthrough** when panel leaf focused; keep `:` passthrough
  - Files: [`helix-term/src/ui/editor.rs`](../helix-term/src/ui/editor.rs)
- **Acceptance:** space in panel normal mode does not reach view-assuming commands

## Tier 2 — Centralize dispatch (Phase 2)

### PanelHandler trait

- [x] **`PanelHandler` trait + dispatch table**
  - Files: [`helix-term/src/ui/panel.rs`](../helix-term/src/ui/panel.rs)
- [x] **Wrap git/agent/terminal modules**
  - Files: [`helix-term/src/ui/git.rs`](../helix-term/src/ui/git.rs),
    [`helix-term/src/ui/agent.rs`](../helix-term/src/ui/agent.rs),
    [`helix-term/src/ui/terminal.rs`](../helix-term/src/ui/terminal.rs)
- **Acceptance:** panel key/mouse/render/cursor logic lives behind one trait

### EditorView dispatch refactor

- [x] **Unified key/mouse/render/cursor dispatch** in `EditorView`
  - Files: [`helix-term/src/ui/editor.rs`](../helix-term/src/ui/editor.rs)
- **Acceptance:** panel branch in `handle_event` reduced to resolve-kind → dispatch

### Sub-focus naming

- [x] **`agent_input_focused` / `terminal_input_focused` / `git_panel_tree_focused`**
  - Files: [`helix-view/src/editor/panels.rs`](../helix-view/src/editor/panels.rs)
- **Acceptance:** old `*_panel_focused` names remain as aliases during transition

## Tier 3 — Focus model + tree cleanup (Phase 3)

### FocusTarget enum

- [x] **`FocusTarget` enum + `Editor::focus_target` / `focus_leaf`**
  - Files: [`helix-view/src/focus.rs`](../helix-view/src/focus.rs),
    [`helix-view/src/editor/focus.rs`](../helix-view/src/editor/focus.rs)
- **Acceptance:** `Editor::focus()` has no per-panel `if` chains

### Tree operations

- [x] **Generalize `swap_split_in_direction`** for mixed leaf kinds
- [x] **`Content::set_area`** — remove duplicated match arms in recalculate/find_child
  - Files: [`helix-view/src/tree.rs`](../helix-view/src/tree.rs)
- **Acceptance:** swap between view and panel siblings does not panic

### Panel lifecycle dedup

- [x] **Generic `prepare_panel_focus` helper** in panels.rs
  - Files: [`helix-view/src/editor/panels.rs`](../helix-view/src/editor/panels.rs)
- [x] **Replace scattered `is_*_panel` checks** with `focused_leaf_kind()` / `is_document_view_focused()` in mode.rs and cursor.rs
  - Files: mode.rs, cursor.rs
- **Acceptance:** grep for `is_*_panel` limited to tree internals + panel dispatch + command guards

## Tier 4 — Hardening (Phase 4)

### Integration tests

- [x] **Panel-focus smoke tests** — quit, wclose, quit-all, focus rotation
  - Files: [`helix-term/tests/test/focused_leaf.rs`](../helix-term/tests/test/focused_leaf.rs)
- **Acceptance:** CI covers each panel kind without panics

### Typed-command guard

- [x] **`require_document_view` helper** for typed commands
  - Files: [`helix-term/src/commands/helpers.rs`](../helix-term/src/commands/helpers.rs),
    [`helix-term/src/commands/typed/lifecycle.rs`](../helix-term/src/commands/typed/lifecycle.rs)
- **Acceptance:** high-traffic typed commands fail gracefully when panel focused

### Developer docs

- [x] **Macro/tree comments** documenting `try_current!` vs `current!`
  - Files: [`helix-view/src/macros.rs`](../helix-view/src/macros.rs),
    [`helix-view/src/tree.rs`](../helix-view/src/tree.rs)

## Conventions

- When `tree.focus` may be a panel, use `try_current!` or `focused_leaf_kind()`.
  Never call `current!` without guarding.
- Panel domain state stays on `Editor` (`agent`, `terminal`, `git`); tree nodes
  remain `{ id, area }` shells.
- Do not move panels into compositor `Component` — they are in-tree leaves.

## Out of scope

- Making panels compositor `Component`s
- Moving panel state onto tree nodes
- Full ECS refactor of `View`/`Document`
- Rewriting all commands to use `try_current!`
