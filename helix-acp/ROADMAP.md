# Helix Agent / ACP Roadmap

Developer-facing roadmap for the agent integration. User documentation lives in
[`book/src/agent.md`](../book/src/agent.md).

Work top-down by tier. Mark items complete (`[x]`) when merged.

## Tier 1 — Production ready

### Debug and redraw polish

- [x] **Gate debug logging** — `debug-logging` in `[editor.agent]` (default `false`).
  Only emit `AgentEvent::Debug` when enabled; otherwise use `log::trace!`.
  - Files: [`helix-view/src/agent.rs`](../helix-view/src/agent.rs), [`helix-acp/src/runtime.rs`](src/runtime.rs)
- [x] **Skip debug redraws** — Do not call `request_redraw()` for debug-only events.
  - Files: [`helix-term/src/handlers/agent.rs`](../helix-term/src/handlers/agent.rs)
- **Acceptance:** Default config produces no debug lines in the agent pane during streaming.

### Runtime lifecycle

- [x] **Shutdown on panel close** — Tear down the `agent acp` subprocess when the agent
  pane closes, not only on Helix exit.
  - Files: [`helix-term/src/commands/agent.rs`](../helix-term/src/commands/agent.rs)
- **Acceptance:** Closing the agent pane (`Space A c`, `:agent-close`, `Ctrl-w q`) stops the subprocess.

### Interactive permissions

- [x] **Permission bridge** — Replace silent auto-approve with oneshot UI bridge (mirror
  `cursor/*` in [`helix-term/src/ui/agent_cursor.rs`](../helix-term/src/ui/agent_cursor.rs)).
  - Files: [`helix-acp/src/runtime.rs`](src/runtime.rs), [`helix-acp/src/events.rs`](src/events.rs),
    [`helix-term/src/handlers/agent.rs`](../helix-term/src/handlers/agent.rs),
    [`helix-term/src/ui/agent_permission.rs`](../helix-term/src/ui/agent_permission.rs)
- [x] **Auto-approve fallback** — Optional `auto-approve-permissions = true` for headless use.
  - Files: [`helix-view/src/agent.rs`](../helix-view/src/agent.rs)
- **Acceptance:** Permission prompts show a picker; user choice completes the ACP RPC.

### Editor context in prompts

- [x] **Attach buffer context** — Include current file path (`ResourceLink`) and selection
  text when sending prompts.
  - Files: [`helix-acp/src/events.rs`](src/events.rs), [`helix-acp/src/runtime.rs`](src/runtime.rs),
    [`helix-term/src/handlers/agent.rs`](../helix-term/src/handlers/agent.rs)
- [x] **Config toggle** — `include-editor-context = true` (default on).
  - Files: [`helix-view/src/agent.rs`](../helix-view/src/agent.rs)
- **Acceptance:** Prompts from an open buffer include file URI and selection in the ACP request.

## Tier 2 — UX and lifecycle

- [x] **Tool-call transcript UX** — Collapsible tool rows with `▸/▾` headers; merge
  `ToolCallUpdate` by id; toggle with `z` or click header.
  - Files: [`helix-term/src/ui/agent.rs`](../helix-term/src/ui/agent.rs),
    [`helix-acp/src/runtime.rs`](src/runtime.rs), [`helix-view/src/agent.rs`](../helix-view/src/agent.rs)
- [x] **`:agent-new`** — Explicit new-session command without restarting runtime (`Space A n`).
  - Files: [`helix-term/src/commands/agent.rs`](../helix-term/src/commands/agent.rs)
- [x] **`session/close`** — Send ACP session close before clearing local state when supported.
  - Files: [`helix-acp/src/runtime.rs`](src/runtime.rs)
- [x] **Session picker polish** — Sort by cwd preference and `updated_at`; `:agent-history --cwd`.
  - Files: [`helix-term/src/commands/agent.rs`](../helix-term/src/commands/agent.rs)
- [x] **Cursor extension polish** — Markdown plan review overlay; cancel pending cursor/permission
  requests on panel close.
  - Files: [`helix-term/src/ui/agent_cursor.rs`](../helix-term/src/ui/agent_cursor.rs)

## Tier 3 — Blocked or low priority

- [ ] **Session history replay on `session/load`** — Blocked: Cursor `agent acp` does not
  replay transcript chunks today.
- [x] **Terminal capabilities** — `terminal/create`, output, wait, kill, release.
- [ ] **Remote ACP transports** — HTTP/WebSocket agents.

## Conventions

- Reactive event loop only — no polling loops for agent state.
- Agent→UI blocking RPC uses oneshot bridges in `helix-acp` runtime + picker UI in `helix-term`.
- Config lives in `AgentSettings` ([`helix-view/src/agent.rs`](../helix-view/src/agent.rs));
  protocol types in `helix-acp`.
