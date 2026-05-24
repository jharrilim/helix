# View / Document ECS Roadmap

Developer-facing roadmap for decoupling `View` and `Document` in Helix using an
entity-component-style storage model. This is **long-term editor architecture**,
not required to finish the focused-leaf panel work.

Related: [`focused-leaf-refactor.md`](focused-leaf-refactor.md) (panel focus),
[`try-current-command-guards.md`](try-current-command-guards.md) (command safety).

## Problem

`View` and `Document` are tightly coupled today:

- Each [`View`](../helix-view/src/view.rs) points at a `DocumentId` and owns
  view-local state (jumplist, scroll sync, gutters, diagnostics handler).
- Each [`Document`](../helix-view/src/document.rs) stores **per-view** state keyed
  by `ViewId`: selections, `ViewData`, inlay hints, document highlights, jump
  labels, savepoints.

This split causes recurring pain:

1. **Bidirectional identity** — view knows doc; doc maps hold view ids; easy to
   desync on close/split.
2. **Borrowing friction** — `current!` macros exist because `Editor` cannot be
   mutably borrowed as a whole; partial field access is manual.
3. **Per-view hacks** — every view gets its own `DiagnosticsHandler` because
   positioning code needs editor context:

```152:158:helix-view/src/view.rs
    // HACKS: there should really only be a global diagnostics handler ...
    // That is a huge refactor left to future work. For now we treat all views
    // as focused and give them each their own handler.
    pub diagnostics_handler: DiagnosticsHandler,
```

4. **Lazy sync complexity** — `doc_revisions`, `sync_changes`, and
   `changes_to_sync` exist because view and document histories are separate
   stores that must be merged on focus switch.

## Vision

**Not** a full game-engine ECS (no archetype queries, no systems scheduler).
Helix needs a **storage refactor**:

```text
EntityId (ViewId | DocumentId — or unified EntityId)
  ├── components in typed maps (Selection, ViewOffset, JumpList, …)
  └── resources on Editor (config, theme, LSP pool, …)
```

Goals:

- **Single source of truth** for each piece of state (no selections on Document
  keyed by ViewId *and* view pointing at doc).
- **Global services** (diagnostics positioning, inlay hints) keyed by focused
  view + document, not duplicated per view.
- **Cleaner borrows** — fetch `(ViewComponents, DocComponents)` for a focused
  entity without macro workarounds.
- **Panels stay separate** — agent/git/terminal state remains on `Editor`; ECS
  applies to the document-editing core only.

Non-goals:

- Rewriting helix-core rope/transaction model
- Making auxiliary panels into document entities
- Runtime plugin component registration

## Current state inventory

Before coding, document what lives where:

| State | Today | Target owner |
| --- | --- | --- |
| Buffer text, history | `Document` | `Document` entity (unchanged) |
| Selection | `Document.selections[view_id]` | `ViewSelection` component on view entity |
| Scroll offset | `Document.view_data[view_id]` | `ViewOffset` component on view entity |
| Jumplist | `View.jumps` | `JumpList` component on view entity |
| Gutter config | `View.gutters` | `GutterConfig` component on view entity |
| Diagnostics UI | `View.diagnostics_handler` | Global handler + focus id |
| Inlay hints | `Document.inlay_hints[view_id]` | `InlayHints` component or global cache |
| LSP state | `Document` | Stays on document entity |
| Mode | `Editor.mode` | Stays editor resource (document views only) |

Deliverable: architecture note in `helix-view/docs/` or this roadmap updated
with the final table.

## Tier 1 — Design and scaffolding

### Entity model

- [ ] Decide: reuse `ViewId` / `DocumentId` or introduce unified `EntityId`
- [ ] Define `ComponentStore` pattern — likely `HashMap<ViewId, T>` per component
  type inside `Editor` (Helix style: closed types, no trait objects)
- [ ] Sketch `World` or extend `Editor` with component accessors:

```rust
editor.components::<Selection>(view_id)
editor.components_mut::<ViewOffset>(view_id)
```

- **Acceptance:** design doc reviewed; no production migration yet

### Migration strategy

- [ ] **Strangler pattern** — move one component at a time; dual-write during
  transition if needed
- [ ] Order: low-risk leaf components first (`ViewData` / scroll offset), then
  selections, then diagnostics
- [ ] Each step keeps all tests green
- **Acceptance:** written migration order with rollback plan per step

## Tier 2 — Extract view-local state from Document

Move per-view fields off `Document` into view-scoped storage.

### ViewOffset / ViewData

- [ ] Move scroll anchor, horizontal/vertical offset out of
  [`Document`](../helix-view/src/document.rs) `view_data` map
- [ ] Update [`view.rs`](../helix-view/src/view.rs) sync/positioning to read new store
- [ ] Remove `Document::view_data` map when empty
- **Acceptance:** split/focus/scroll behavior unchanged; unit tests pass

### Selection

- [ ] Move `Document.selections: HashMap<ViewId, Selection>` to view component store
- [ ] Update `doc.selection(view_id)` API to delegate to component store
- [ ] Keep public `Document::set_selection(view_id, …)` as facade during transition
- **Acceptance:** multi-view same-buffer selections still work

### Inlay hints and highlights

- [ ] Move `inlay_hints`, `document_highlights`, `jump_labels` per-view maps
- [ ] Update LSP hint handlers in `helix-term`
- **Acceptance:** inlay hints render correctly in split views

## Tier 3 — Global diagnostics handler

Address the explicit hack in `view.rs`.

- [ ] Single `DiagnosticsHandler` on `Editor` (or `EditorState` resource)
- [ ] Handler takes `(Editor, focused_view_id, document_id)` for cursor-line logic
- [ ] Non-focused views skip cursor-line diagnostic emphasis (or share one policy)
- [ ] Remove `View.diagnostics_handler`
- **Acceptance:** diagnostics display unchanged in single and multi-view; memory
  not duplicated per view

## Tier 4 — Editor access patterns

Reduce reliance on `current!` macros by providing typed accessors.

- [ ] `Editor::focused_view_components(&mut self) -> Option<FocusedView<'_>>`
  bundling view + doc + selection + offset borrows
- [ ] Migrate hot paths (movement, insert) to accessor; keep macros as thin wrappers
- [ ] Evaluate whether `FocusedView` supersedes some `try_current!` guards
- **Acceptance:** measurable reduction in `current!` usage in `helix-view`; no perf regression

## Tier 5 — Cleanup and documentation

- [ ] Remove dual-write shims and deprecated `Document` maps
- [ ] Update [`macros.rs`](../helix-view/src/macros.rs) docs with component access patterns
- [ ] Document component ownership rules for contributors
- **Acceptance:** `Document` no longer has `HashMap<ViewId, _>` fields except
  documented exceptions

## Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| Large diff hard to review | One component per PR; strangler pattern |
| Subtle selection/sync bugs | Extend integration tests before each migration |
| Perf regression from indirection | Benchmark open/split/scroll; keep hot paths inline |
| Scope creep into panels | Explicit non-goal; panels use focused-leaf API |

## Suggested PR sequence

| PR | Scope | Risk |
| --- | --- | --- |
| 1 | Design doc + `ComponentStore` scaffolding (no migration) | Low |
| 2 | Migrate `ViewData` / scroll offset | Medium |
| 3 | Migrate selections | High |
| 4 | Migrate inlay hints / highlights | Medium |
| 5 | Global diagnostics handler | Medium |
| 6 | `FocusedView` accessor + macro cleanup | Low |

## When to start

Start Tier 1 when **either**:

- Diagnostics or inlay-hint work is blocked by per-view duplication, or
- A feature needs clean multi-view state (e.g. synchronized scrolling, shared
  cursors), or
- Editor borrowing complexity is slowing other refactors.

Do **not** block panel/git/terminal features on this roadmap.

## Out of scope

- Full `bevy_ecs` or generic plugin component system
- Moving agent/terminal/git state into entity components
- Rewriting `helix-core` `Transaction` / rope layer
- Changing tree/split structure (see focused-leaf refactor)

## Related

- [`focused-leaf-refactor.md`](focused-leaf-refactor.md) — panel leaves vs document views
- [`try-current-command-guards.md`](try-current-command-guards.md) — command safety (orthogonal, can proceed in parallel)
