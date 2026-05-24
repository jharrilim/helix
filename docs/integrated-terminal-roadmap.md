# Helix Integrated Terminal Roadmap

Developer-facing roadmap for an in-editor terminal emulator. User documentation
should live in `book/src/terminal.md` once Tier 1 ships.

Shared split-tree focus infrastructure for agent, terminal, and git panels is
tracked in [`roadmaps/focused-leaf-refactor.md`](../roadmaps/focused-leaf-refactor.md).

Work top-down by tier. Mark items complete (`[x]`) when merged.

## Vision

Helix should host one or more interactive shell sessions inside the split tree,
with **modal editing** consistent with the rest of the editor (Normal / Insert /
Select), full **scrollback**, **multiple terminals**, and first-class hooks for
**ACP agent terminal spawning** and **DAP `runInTerminal`**.

Today, shell integration is limited to one-shot commands (`!`, `|`, `$`) and DAP
opens an **external** terminal via `[editor.terminal]`. This roadmap adds a
real PTY-backed terminal surface inside Helix.

## Architecture

Follow the same three-layer split used by agent/ACP:

| Layer | Crate (proposed) | Responsibility |
| --- | --- | --- |
| Emulator + PTY | **`helix-pty`** (new) | Spawn processes, pump I/O, parse ANSI, maintain grid + scrollback, resize. UI-agnostic. |
| Editor state | **`helix-view`** | `TerminalSettings`, `TerminalState`, `TerminalPanel` tree node, focus/selection. |
| TUI + commands | **`helix-term`** | Render, input routing, keymap, typed commands, job/event integration. |

### Tree integration

Extend [`helix-view/src/tree.rs`](../helix-view/src/tree.rs) the same way agent
panels work today:

- Add `Content::TerminalPanel(TerminalPanel)` alongside `AgentPanel`.
- `split_terminal_panel`, `is_terminal_panel`, `terminal_panels()` iterators.
- Terminal leaves participate in focus navigation (`Ctrl-w h/j/k/l`, `:focus`).

### Event model (`helix-pty`)

Reactive, no UI polling:

```text
TerminalCommand  →  spawn, write, resize, signal, kill, focus
TerminalEvent    →  output, exit, title change, bell, error
```

The runtime owns PTY file descriptors on a dedicated thread or async task; the UI
thread receives byte chunks and applies them to the emulator grid.

### Emulator backend

Prefer a maintained Rust VT parser + grid crate rather than rolling our own:

- **Primary candidate:** `alacritty_terminal` (grid, scrollback, ANSI, resize).
- **PTY spawning:** `portable-pty` (cross-platform).
- **Fallback parser:** `vte` + custom grid if `alacritty_terminal` API friction
  is too high.

Acceptance for backend choice: pass a small integration test suite (basic colors,
cursor motion, alternate screen, window title OSC).

## Modal behavior

Terminals must feel like Helix, not a raw TUI passthrough with one mode.

### Focus modes (`TerminalFocus`)

Mirror [`AgentFocus`](../helix-view/src/agent.rs):

| Mode | Purpose | Keys |
| --- | --- | --- |
| **Normal** | Navigate scrollback, yank, switch terminals, run commands | `j`/`k`, `Ctrl-u`/`Ctrl-d`, `y`, `i`/`a`, `:` |
| **Insert** | Send keys to PTY | printable keys, Enter, Backspace, arrows (moded through curses) |
| **Select** (optional Tier 2) | Character/line selection in scrollback | `v`, `V`, `y` |

### Key routing rules

- `<esc>` in Insert → Normal (does **not** send ESC to PTY by default; use
  `<C-[>` or a dedicated `terminal-send-esc` binding if needed).
- Prefix maps (`Space t …`) work in Normal only, like `Space A` for agent.
- Mouse wheel scrolls scrollback in Normal; click focuses terminal leaf.
- When terminal leaf is focused, editor status line shows `[terminal]`
  + shell name + cwd + pid.

### Multiline input

Three complementary paths:

1. **Insert mode (Tier 1)** — Enter sends `\n` to the PTY; the shell/readline
   handles line editing. This is the default for interactive shells.
2. **Bracketed paste (Tier 3)** — Pasted text is wrapped in `\e[200~…\e[201~` so
   shells treat multiline paste as a single unit.
3. **Command buffer (Tier 2, optional)** — A dedicated input strip (like the
   agent panel prompt) for composing a multiline snippet before sending. Useful
   for pasting into non-readline programs. Toggle via config
   `command-buffer = false` (default off).

## Configuration sketch

New section in `config.toml` (name TBD):

```toml
[editor.integrated-terminal]
enable = true
shell = "default"              # or explicit path + args
scrollback-lines = 10000
cwd = "current"                # "current" | "workspace-root" | explicit path
focus-on-open = true
auto-close-on-exit = false
command-buffer = false
```

Keep existing `[editor.terminal]` for **external** terminal launch (DAP fallback).

---

## Tier 0 — Foundation

### Crate scaffolding

- [x] **Create `helix-pty`** — Workspace member with `TerminalRuntime`,
  `TerminalCommand`, `TerminalEvent`, and a single `TerminalSession` type.
  - Files: `helix-pty/Cargo.toml`, `src/lib.rs`, `src/runtime.rs`, `src/session.rs`
- [x] **Emulator smoke test** — Feed ANSI fixture bytes, assert grid + scrollback.
  - Files: `helix-pty/tests/grid.rs`

### View state

- [x] **`TerminalSettings` + `TerminalState`** — Panel id, focus mode, session id,
  title, cwd, exit status, scroll pin.
  - Files: [`helix-view/src/terminal.rs`](../helix-view/src/terminal.rs) (new),
    [`helix-view/src/editor.rs`](../helix-view/src/editor.rs)
- [x] **Tree node** — `TerminalPanel` in split tree; open/close/focus helpers.
  - Files: [`helix-view/src/tree.rs`](../helix-view/src/tree.rs),
    [`helix-view/src/editor.rs`](../helix-view/src/editor.rs)

**Acceptance:** `cargo test -p helix-pty` passes; editor can hold terminal state
without rendering.

---

## Tier 1 — Single usable terminal

### PTY lifecycle

- [x] **Spawn default shell** — Inherit env, set `TERM` appropriately (e.g.
  `xterm-256color`), optional cwd from config.
  - Files: `helix-pty/src/live.rs`, `helix-pty/src/runtime.rs`
- [x] **I/O pump** — EventLoop thread reads PTY → grid updates →
  `TerminalEvent::Updated`; write stdin from `TerminalCommand::Write`.
  - Files: `helix-pty/src/live.rs`, `helix-pty/src/runtime.rs`
- [x] **Resize** — On panel area change, send winsize + `TerminalCommand::Resize
  { rows, cols }`.
  - Files: `helix-pty/src/live.rs`, [`helix-term/src/ui/terminal.rs`](../helix-term/src/ui/terminal.rs)
- [x] **Process exit** — Emit `TerminalEvent::Exited { code, signal }`; show
  status in panel header.
  - Files: `helix-pty/src/live.rs`, [`helix-term/src/ui/terminal.rs`](../helix-term/src/ui/terminal.rs)

### Rendering + input

- [x] **Grid render** — Draw emulator contents into `helix-tui` surface with
  theme-aware default fg/bg; respect alternate screen.
  - Files: [`helix-term/src/ui/terminal.rs`](../helix-term/src/ui/terminal.rs),
    [`helix-term/src/ui/editor.rs`](../helix-term/src/ui/editor.rs)
- [x] **Scrollback** — Normal-mode `j`/`k`, `Ctrl-u`/`Ctrl-d`, `G`/`gg` move
  viewport through history; auto-pin to bottom on new output unless user scrolled
  up.
  - Files: [`helix-term/src/ui/terminal.rs`](../helix-term/src/ui/terminal.rs)
- [x] **Insert mode** — `i`/`a` enter Insert; keys encode to PTY except Helix
  prefix escapes.
  - Files: [`helix-term/src/ui/terminal.rs`](../helix-term/src/ui/terminal.rs)
- [x] **Basic commands** — `:terminal-open`, `:terminal-close`, `:terminal-toggle`
  (aliases `:term-open`, etc.).
  - Files: [`helix-term/src/commands/terminal.rs`](../helix-term/src/commands/terminal.rs),
    [`helix-term/src/commands/typed.rs`](../helix-term/src/commands/typed.rs)
- [x] **Keymap** — `Space t o` open, `Space t c` close, `Space t i` insert (when
  Normal).
  - Files: [`helix-term/src/keymap/default.rs`](../helix-term/src/keymap/default.rs)

### ANSI support (Tier 1 minimum)

- [x] SGR colors (16 + 256 + truecolor if `$COLORTERM` allows)
- [x] Cursor show/hide, move, clear line/screen
- [x] Alternate screen (`smcup`/`rmcup`) for `vim`, `less`, `htop`

**Acceptance:** User opens a terminal split, runs `ls --color=auto`, `vim`, and
`htop`; scrollback works; closing the panel kills the PTY.

**Manual checklist:** Enable `[editor.integrated-terminal] enable = true`, then
`:terminal-open` or `Space t o`; run `ls --color=auto`, `vim`, `htop`; verify
`j`/`k` scrollback and `Space t c` closes and kills the shell.

---

## Tier 2 — Helix-native UX

### Selection and clipboard

- [x] **Yank scrollback** — Normal/Select mode copy to Helix register + system
  clipboard (where supported).
  - Files: [`helix-term/src/ui/terminal.rs`](../helix-term/src/ui/terminal.rs)
- [x] **Paste** — `"`+`p` / `Shift-Insert` in Insert sends clipboard bytes to PTY.
- [x] **Mouse selection** — Drag to select in scrollback (Normal); release yanks.

### Navigation and layout

- [x] **Focus movement** — Terminals participate in `:focus {direction}` and
  `Ctrl-w` navigation on equal footing with editor buffers and agent panel.
- [x] **Open cwd policy** — `:terminal-open --cwd path`; default follows active
  buffer directory.
- [x] **Send from editor** — `:terminal-send` sends selection or current line to
  focused terminal (append `\n` optionally).
  - Files: [`helix-term/src/commands/terminal.rs`](../helix-term/src/commands/terminal.rs)

### Search and polish

- [x] **Scrollback search** — `/` and `n`/`N` in Normal (picker or incremental).
- [x] **Panel header** — Title (OSC), cwd, mode indicator `[N]`/`[I]`, exit code.
- [x] **Bell** — Optional flash status line on `\a` (respect `[editor.terminal]`
  bell config if added).
- [x] **Resizing** - Ensure that the terminal resizes correctly when the window is resized.

**Acceptance:** Developer workflow: edit code → `Space t o` → run tests → yank
error output back into buffer with terminal selection.

---

## Tier 3 — Multiple terminals

### Session management

- [x] **Terminal registry** — `BTreeMap<TerminalId, TerminalSession>` in
  `TerminalState`; stable ids for ACP/DAP.
  - Files: [`helix-view/src/terminal.rs`](../helix-view/src/terminal.rs),
    `helix-pty/src/runtime.rs`
- [x] **New / list / switch** — `:terminal-new`, `:terminal-list` (picker),
  `:terminal-focus {id}`, `Space t n`, `Space t l`.
  - Files: [`helix-term/src/commands/terminal.rs`](../helix-term/src/commands/terminal.rs)
- [x] **Multiple panels** — Several terminal leaves in the split tree, or tabbed
  container (start with one panel per session; defer tabs if split-only is enough).
- [ ] **Kill / restart** — `:terminal-kill`, `:terminal-restart`; confirm on
  processes with children.

### Split layouts

- [ ] **`:terminal-split`** — Open another terminal in a horizontal/vertical split
  (`:vsplit`-style), sharing or independent cwd.
- [ ] **Broadcast (optional)** — Send input to all visible terminals (off by
  default; useful for fan-out commands).

### Persistence (optional)

- [ ] **Session metadata** — Remember open terminal ids + cwd in a workspace
  scratch file (not full scrollback restore).

**Acceptance:** Three terminals open (test, server, agent shell); picker switches
focus; closing one panel does not kill others unless configured.

---

## Tier 4 — Agent and debugger integration

### ACP terminal capabilities

Unblocks [helix-acp/ROADMAP.md Tier 3](../helix-acp/ROADMAP.md) terminal items.

- [x] **Advertise terminal capability** — On ACP `initialize`, set terminal
  support flags expected by the protocol.
  - Files: [`helix-acp/src/runtime.rs`](../helix-acp/src/runtime.rs)
- [x] **`terminal/create`** — Agent requests a PTY; Helix spawns session, returns
  terminal id; headless shell blocks in the agent pane.
  - Files: `helix-pty`, [`helix-acp/src/runtime.rs`](../helix-acp/src/runtime.rs),
    [`helix-term/src/handlers/agent.rs`](../helix-term/src/handlers/agent.rs)
- [x] **`terminal/output`** — PTY output snapshot for agent polling with byte-limit
  truncation.
- [x] **`terminal/wait_for_exit` / `kill` / `release`** — Lifecycle RPCs wired to
  `TerminalCommand`.
- [x] **Shell blocks in agent transcript** — Stream PTY output into collapsible shell
  blocks (replaces static `terminal: {id}` tool detail text).
  - Files: [`helix-term/src/ui/agent.rs`](../helix-term/src/ui/agent.rs),
    [`helix-view/src/agent.rs`](../helix-view/src/agent.rs)

### DAP `runInTerminal`

- [ ] **Integrated path** — When `[editor.integrated-terminal].enable`, handle
  `RunInTerminal` by spawning an integrated session instead of
  [`TerminalConfig`](../helix-view/src/editor.rs) external launch.
  - Files: [`helix-view/src/handlers/dap.rs`](../helix-view/src/handlers/dap.rs),
    [`helix-term/src/handlers/terminal.rs`](../helix-term/src/handlers/terminal.rs)
- [ ] **Fallback** — Keep external terminal when integrated terminal is disabled.

**Acceptance:** Cursor agent tool call opens a visible terminal, runs a command,
streams output back; debugger `runInTerminal` lands in a Helix split.

---

## Tier 5 — Advanced / parity

- [ ] **Bracketed paste** — `\e[200~` / `\e[201~` on large pastes.
- [ ] **Mouse reporting** — SGR mouse in alternate screen apps that expect it.
- [ ] **Hyperlinks (OSC 8)** — Click opens via `open_external_url_callback`.
- [ ] **Unicode width** — Correct wide-character layout (East Asian, emoji); reuse
  `helix-core` width helpers where possible.
- [ ] **Process groups** — Clean teardown on `:quit` / SIGWINCH storms; no zombie
  PTYs.
- [ ] **Remote / SSH (out of scope for v1)** — Document as future work; do not
  block local PTY design.
- [ ] **Performance** — Cap redraw rate on flood output; coalesce `Output` events
  per frame (same pattern as agent debug redraw gating).

---

## Commands and keymap (target)

| Command | Key (proposal) | Description |
| --- | --- | --- |
| `terminal-open` | `Space t o` | Open/focus terminal in split |
| `terminal-close` | `Space t c` | Close panel; kill PTY |
| `terminal-new` | `Space t n` | New session |
| `terminal-list` | `Space t l` | Picker of sessions |
| `terminal-focus` | `Space t f` | Cycle terminals |
| `terminal-insert-mode` | `i` (in terminal Normal) | Send keys to shell |
| `terminal-normal-mode` | `Esc` | Stop sending to shell |
| `terminal-send` | (typed) | Send editor selection to PTY |

Typed commands should mirror agent naming: `:terminal-open`, `:terminal-new`,
`:terminal-list`, `:terminal-kill`, `:terminal-send`.

---

## Testing strategy

| Layer | Tests |
| --- | --- |
| `helix-pty` | ANSI fixtures, resize reflow, scrollback limits, exit codes |
| `helix-view` | Tree insertion/removal, focus helpers, settings deserialize |
| `helix-term` | Integration tests (feature `integration`): open terminal, type bytes, assert grid snapshot; mock PTY with scripted shell |

Use **`script(1)`** or a fake shell that echoes fixed bytes for deterministic
integration tests.

---

## Conventions

- **Reactive event loop only** — PTY reads feed `TerminalEvent`; no polling UI state.
- **Kill on close** — Closing a terminal panel sends SIGHUP/SIGTERM (configurable);
  mirror agent panel shutdown behavior.
- **Config in `helix-view`** — Protocol/session ids in `helix-pty`; keymaps and
  rendering in `helix-term`.
- **Do not block the UI thread** on PTY I/O or `wait_for_exit`; use channels.
- **Prefer explicit commands** over magic auto-spawn, except ACP/DAP protocol paths.

---

## Related work

- Agent panel split tree: [`helix-view/src/tree.rs`](../helix-view/src/tree.rs),
  [`helix-term/src/ui/agent.rs`](../helix-term/src/ui/agent.rs)
- ACP terminal Tier 3: [`helix-acp/ROADMAP.md`](../helix-acp/ROADMAP.md)
- External terminal today: [`helix-view/src/editor.rs`](../helix-view/src/editor.rs)
  (`TerminalConfig`), DAP handler
- One-shot shell commands: [`helix-term/src/commands.rs`](../helix-term/src/commands.rs)
  (`shell_*`)

---

## Open questions

1. **Tabs vs splits only** — Start with tree splits; add tab bar only if users
   hit panel clutter.
2. **Single global vs per-workspace terminal state** — Likely per `Editor` instance.
3. **Default `$TERM`** — Must balance feature detection with compatibility;
   consider `helix-terminal` string and document limitations.
4. **Neovim `:terminal` parity** — Helix uses dedicated panel leaves, not a
   special buffer type; avoids mixing rope semantics with PTY binary I/O.
