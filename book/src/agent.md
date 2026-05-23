# Agent

Helix can run a local assistant in an editor pane using the
[Agent Client Protocol](https://agentclientprotocol.com/). The agent opens in a
vertical split, shows the conversation transcript, and accepts prompts from an
input line at the bottom of the pane.

This feature is for local ACP-compatible agents that communicate over stdio.
Remote ACP transports are not supported yet.

## Configuration

Configure the agent command in your `config.toml`:

```toml
[editor.agent]
enable = true
command = "agent acp"
panel-width-percent = 35
auth-method = "cursor_login"   # optional; auto-picked when omitted
skip-authenticate = false      # optional escape hatch
default-mode = "agent"         # optional: agent, plan, or ask
# mcp-config-path = "/path/to/mcp.json"
```

For Cursor, use `agent acp` as the command. Run `agent login` (or set
`CURSOR_API_KEY` / `CURSOR_AUTH_TOKEN`) before starting Helix so the agent can
authenticate.

Other ACP agents may use a different executable or wrapper.

The `command` value should be the command you normally use to start your ACP
agent. It may include arguments if your agent requires them.

`panel-width-percent` controls how much of the editor width the agent split
uses. Values are clamped to a reasonable side-panel width.

## Opening the agent

In normal mode, use the default `Space A` menu:

| Key | Action |
| --- | --- |
| `Space A a` | Open the agent pane |
| `Space A A` | Focus the agent pane |
| `Space A c` | Close the agent pane |
| `Space A s` | Send the current prompt |
| `Space A S` | Cancel in-flight turn, or stop/end session when idle |
| `Space A C` | Clear the visible transcript |
| `Space A h` | List and load agent sessions |
| `Space A m` | Pick agent session mode |

The same actions are available from the command line:

| Command | Action |
| --- | --- |
| `:agent-open` | Open the agent pane |
| `:agent-close` | Close the agent pane |
| `:agent-focus` | Focus the agent pane |
| `:agent-send` | Send the current prompt |
| `:agent-stop` | Cancel in-flight turn, or stop/end session when idle |
| `:agent-history` | List and load agent sessions |
| `:agent-mode` | Pick agent session mode |
| `:agent-mode plan` | Set agent session mode directly |

## Using the agent pane

The agent pane is modal. Focusing it with `:agent-open`, `:agent-focus`, or
normal pane navigation enters agent normal mode. Press `i`, `a`, or `Enter` to
edit the prompt. Press `Esc` from prompt editing to return to agent normal mode.

In agent normal mode:

| Key | Action |
| --- | --- |
| `i` / `a` / `Enter` | Enter prompt insert mode |
| `Ctrl-w w` | Move to the next pane |
| `Ctrl-w h/j/k/l` | Move to the pane left/down/up/right |
| `Ctrl-w q` | Close the agent pane |
| `:` | Open command-line mode |
| `Space` | Use normal `Space` key sequences |
| `Space A h` | Open the agent session history picker |
| `PageUp` / `PageDown` | Scroll the transcript |

In agent insert mode:

| Key | Action |
| --- | --- |
| `Enter` | Send the prompt |
| `Esc` | Return to agent normal mode |
| `Up` / `Down` | Move through prompt history |
| `PageUp` / `PageDown` | Scroll the transcript |
| Mouse wheel over transcript | Scroll the transcript |
| Click and drag over transcript | Select transcript text |
| Release mouse after selecting transcript | Copy selection to the mouse yank register |
| `Ctrl-u` | Clear the prompt input |
| `Ctrl-w` | Delete the previous word in the prompt |

This is useful when the agent is the only remaining pane: press `Esc` to enter
agent normal mode, then use `Ctrl-w q`, `Space A c`, or `:agent-close` to close
it.

The transcript shows user prompts, assistant messages, thought/status updates,
tool calls, plans, and errors when the agent reports them.

Hovering over the agent panel does not change focus. Click the prompt input to
enter agent insert mode, or click and drag in the transcript to select visible
text. Selected text is highlighted and copied to the register configured by
`mouse-yank-register` when you release the mouse button, matching normal editor
mouse selection. Use that register (for example `"` then the register name) to
paste elsewhere.

Use `Space A h` or `:agent-history` to browse sessions reported by the agent
through ACP `session/list`. Selecting a session loads it into the agent pane
and clears the current transcript.

### Session history and Cursor

When loading a session, ACP expects the agent to stream prior messages back to
the client as `session/update` notifications (`user_message_chunk`,
`agent_message_chunk`, and related updates).

Cursor's `agent acp` currently does **not** do this. On `session/load` it
typically sends only metadata (such as `available_commands_update` and session
title), so Helix can list and resume the session but the transcript pane stays
empty until you send a new prompt.

This matches [known Cursor ACP behavior](https://forum.cursor.com/t/acp-no-conversation-history-is-restored-when-loading-an-existing-session/158388).
Session listing still works; only transcript replay is missing on the agent
side.

## Cursor ACP compatibility

Helix follows [Cursor's ACP client flow](https://cursor.com/docs/cli/acp):

- `initialize`, then `authenticate` with `cursor_login` when advertised
- `session/new` or `session/load` with MCP servers from `.cursor/mcp.json`
- `session/prompt` with streaming `session/update` chunks
- `session/cancel` when you press `Space A S` during an in-flight turn
- `session/set_mode` via `:agent-mode` or `Space A m`

Cursor extension methods are supported:

- `cursor/ask_question` — interactive picker in Helix
- `cursor/create_plan` — plan review overlay (accept / reject / cancel)
- `cursor/update_todos`, `cursor/task`, `cursor/generate_image` — shown in the
  transcript

MCP servers are loaded from the project `.cursor/mcp.json`, then
`~/.cursor/mcp.json`, unless overridden by `mcp-config-path`.

## File access and edits

Agents can request file reads and writes through ACP.

When an agent reads a file that is already open in Helix, Helix returns the
current buffer contents rather than reading the file from disk. This means the
agent sees unsaved edits in open buffers.

When an agent writes a file:

- if the file is open and has no unsaved changes, Helix applies the change to
  the buffer and writes it to disk
- if the file is open and has unsaved changes, Helix reports a conflict instead
  of silently overwriting your work
- if the file is not open, Helix writes the file on disk

After an agent changes an open buffer, normal editor systems such as syntax
highlighting, diagnostics, and language-server change notifications are updated
from the buffer change.

## Current limitations

- Only local stdio ACP agents are supported.
- Agent permission requests auto-approve (`allow-once` when available) but are
  not yet interactive in the UI.
- Terminal capabilities from ACP are not exposed to agents yet.
- Transcript replay on `session/load` still depends on agent behavior; Cursor's
  `agent acp` limitation remains unchanged.
- Agents should use ACP file operations for best results; Helix does not
  automatically watch all files for external changes.
