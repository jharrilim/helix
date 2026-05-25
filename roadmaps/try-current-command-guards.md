# Try-Current Command Guards Roadmap

Developer-facing roadmap for hardening Helix commands against auxiliary panel
focus (agent, git, terminal). Builds on the focused-leaf infrastructure from
[`focused-leaf-refactor.md`](focused-leaf-refactor.md).

## Problem

`tree.focus` may point at a document `View` or an auxiliary panel leaf. Most
commands still use `current!` / `view!`, which **panic** when a panel has focus.

The focused-leaf refactor fixed the highest-risk paths (quit, wclose, split,
space passthrough) and added safe primitives:

- [`try_current!`](../helix-view/src/macros.rs), [`try_view!`](../helix-view/src/macros.rs)
- [`Editor::is_document_view_focused()`](../helix-view/src/editor/focus.rs)
- [`require_document_view()`](../helix-term/src/commands/helpers.rs)

Roughly **150+** call sites still use `current!` across `helix-term` and
`helix-view`. A full rewrite is unnecessary; **guarding entry points** covers
most real-world exposure.

## Exposure model

| Path | Panel can reach? | Priority |
| --- | --- | --- |
| Typed commands via `:` passthrough | Yes | **High** |
| Mappable commands via normal keymap | No (space passthrough removed) | Low |
| Mappable commands via pending/sticky keymap | Rare | Medium |
| Handlers / jobs / LSP callbacks | Usually view-focused | Medium |
| UI helpers (prompt, completion, picker) | When opened from panel | Medium |

**Rule:** panel normal mode only escapes to global commands via `:` (and
in-progress keymap sequences). Typed commands are the main remaining panic
surface.

## Principles

- **Incremental** — guard commands in small PRs by category, not one mega-diff.
- **Fail gracefully** — use `require_document_view_or_error()`; never panic.
- **Do not rewrite working paths** — keep `current!` where focus is guaranteed
  (e.g. after an explicit guard, or in code only reachable from document views).
- **Panel-aware commands stay panel-aware** — `:agent-*`, `:terminal-*`, `:git-*`
  must not require a document view.
- **Test from panel focus** — each guarded tier gets a smoke test with git/agent
  panel focused.

## Tier 1 — Typed command registry audit

Classify every command in
[`helix-term/src/commands/typed/command_list/`](../helix-term/src/commands/typed/command_list/)
into one of:

| Class | Action |
| --- | --- |
| **Document** | Requires `require_document_view_or_error()` at start |
| **Panel** | Safe with panel focus; may call `editor.agent` / `terminal` / `git` |
| **Global** | Safe anywhere (config, quit, theme, etc.) |

### Lifecycle and buffer (high traffic)

- [x] Audit [`lifecycle.rs`](../helix-term/src/commands/typed/lifecycle.rs) — dispatch guard + quit ordering fix; exit uses `try_current_ref!`
- [x] Audit [`buffer.rs`](../helix-term/src/commands/typed/buffer.rs) — document focus via registry; helpers use `try_current!`
- [x] Audit [`edit.rs`](../helix-term/src/commands/typed/edit.rs) — all commands classified `Document`
- **Acceptance:** running `:write` or `:yank-join` with git panel focused shows an
  error, does not panic

### Window and workspace

- [x] Audit [`window.rs`](../helix-term/src/commands/typed/window.rs) — all commands classified `Document`; dispatch guard covers `:vsplit`
- [x] Audit [`workspace.rs`](../helix-term/src/commands/typed/workspace.rs) — all commands classified `Global`
- **Acceptance:** `:vsplit` from agent panel returns error message

### LSP, treesitter, diff, clipboard

- [x] Audit [`lsp.rs`](../helix-term/src/commands/typed/lsp.rs)
- [x] Audit [`treesitter.rs`](../helix-term/src/commands/typed/treesitter.rs)
- [x] Audit [`diff.rs`](../helix-term/src/commands/typed/diff.rs)
- [x] Audit [`clipboard.rs`](../helix-term/src/commands/typed/clipboard.rs)
- **Acceptance:** each module documented in registry comments with focus class

### Panel commands (explicit allowlist)

- [x] Confirm [`agent.rs`](../helix-term/src/commands/typed/agent.rs) safe with panel focus
- [x] Confirm [`terminal.rs`](../helix-term/src/commands/typed/terminal.rs) safe
- [x] Add git typed commands when present — safe with git panel focused
- **Acceptance:** `:agent-open`, `:terminal-open` work with any panel focused

### Registry metadata (optional enhancement)

- [x] Add `FocusRequirement` enum to
  [`TypableCommand`](../helix-term/src/commands/typed/mod.rs)
- [x] Wrap dispatch in [`infra.rs`](../helix-term/src/commands/typed/infra.rs)
  to auto-guard document commands before `fun` runs
- **Acceptance:** new typed commands declare focus requirement at registration;
  forgotten guards caught by wrapper

## Tier 2 — Completers and prompt paths

Typed-command completers often call `doc!` / `current!` while the prompt is open.

- [x] Audit [`helix-term/src/ui/completers`](../helix-term/src/ui/mod.rs) —
  `buffer`, `configured_language_servers`, `active_language_servers`, etc.
- [x] Audit [`prompt.rs`](../helix-term/src/ui/prompt.rs) and regex prompt paths
  in `ui/mod.rs`
- [x] Return empty completions or use last-focused document when panel focused
- **Acceptance:** `:buffer` tab-completion with agent panel focused does not panic

## Tier 3 — Mappable command spot-check

Normal-mode mappable commands are **not** reachable via space from panels, but
may still run via sticky maps or programmatic invocation.

- [x] Audit commands in [`movement.rs`](../helix-term/src/commands/movement.rs),
  [`edit.rs`](../helix-term/src/commands/edit.rs), [`selection.rs`](../helix-term/src/commands/selection.rs) — no changes needed; not reachable from panel `:`
- [x] Prefer leaving as-is if unreachable; add guards only where handlers call
  them with `tree.focus` directly
- **Acceptance:** grep documents any intentional `current!` without guard

## Tier 4 — Tests and regression prevention

- [x] Extend [`focused_leaf.rs`](../helix-term/tests/test/focused_leaf.rs) —
  typed-command smoke tests per panel kind
- [x] Test matrix: `:write`, `:yank-join`, `:vsplit`, `:buffer-next` with git/agent
  focused → error, no panic
- [x] Test `:agent-open` / `:quit` with panel focused → succeeds
- [ ] (Optional) compile-time or CI grep: flag new `current!` in
  `commands/typed/` without adjacent guard comment
- **Acceptance:** CI blocks reintroduction of unguarded typed-command panics

## Suggested PR sequence

| PR | Scope |
| --- | --- |
| 1 | Registry audit doc + lifecycle/buffer/edit guards |
| 2 | Window/workspace + LSP/treesitter guards |
| 3 | Completer/prompt fixes |
| 4 | Integration test matrix + optional registry metadata |

## Conventions

Prefer registry metadata + centralized dispatch guard for typed commands:

```rust
TypableCommand {
    name: "write",
    // ...
    focus: FocusRequirement::Document,
}
```

For handlers with special panel branches (e.g. `:quit`), use `FocusRequirement::Global`
and handle panel focus explicitly in the handler.

Per-handler guards remain valid where needed:

```rust
if !crate::commands::require_document_view_or_error(cx.editor) {
    return Ok(());
}
let (view, doc) = current!(cx.editor);
```

When focus is guaranteed by control flow (e.g. immediately after
`focus_document`), a brief comment is enough:

```rust
// SAFETY: focus_document ensures a document view is focused.
let (view, doc) = current!(cx.editor);
```

## Out of scope

- Replacing every `current!` in mappable commands (low exposure after passthrough fix)
- Changing `current!` macro semantics (too many valid call sites)
- Making panels use `try_current!` (panels are not documents)

## Related

- [`focused-leaf-refactor.md`](focused-leaf-refactor.md) — infrastructure this builds on
- [`view-document-ecs.md`](view-document-ecs.md) — separate long-term editor architecture work
