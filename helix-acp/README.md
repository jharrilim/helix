# helix-acp

`helix-acp` contains Helix's UI-agnostic Agent Client Protocol (ACP)
runtime plumbing. It is responsible for talking to a local ACP agent process
and translating protocol traffic into simple commands, events, and filesystem
callbacks that the editor UI can consume.

## Features Today

- Local ACP agents over stdio, using the `agent-client-protocol` Rust SDK.
- Agent process configuration through `AgentConfig`, including the ACP command
  string, auth settings, default mode, and optional MCP config path.
- Background runtime startup through `AgentRuntime::spawn`, returning:
  - `AgentRuntimeHandle` for sending commands to the agent.
  - an `UnboundedReceiver<AgentEvent>` for streamed session updates.
- Session commands:
  - `StartSession` creates a new ACP session for an optional working directory.
  - `LoadSession` resumes an existing ACP session id.
  - `ListSessions` lists historical sessions, optionally filtered by cwd.
  - `SendPrompt` sends a user prompt to the active session.
  - `Cancel` sends ACP `session/cancel` for the active in-flight turn.
  - `Stop` clears local active session state while keeping the runtime alive.
  - `SetMode` sends ACP `session/set_mode`.
- Runtime events for terminal/editor UI layers:
  - initialize/authenticate, session started, loaded, or listed
  - mode updates, user/assistant/thought/tool-call/plan/system/error messages
  - turn started, finished, or cancelled
  - blocking Cursor extension requests (`CursorRequest`)
  - generic status, error, and debug reporting
- ACP filesystem capability support:
  - `readTextFile` is routed through a caller-provided `FsReadFn`.
  - `writeTextFile` is routed through a caller-provided `FsWriteFn`.
  - writes can report applied, conflict, rejected, or error outcomes.
- Cursor MCP config passthrough from `.cursor/mcp.json`.
- Cursor extension method handling for `cursor/ask_question`,
  `cursor/create_plan`, and notification methods.
- Permission request handling that prefers `allow-once`, then `allow-always`.

## Protocol Compatibility

This table tracks compatibility with the current ACP protocol docs at
<https://agentclientprotocol.com/protocol/schema>. "Supported" means the crate
has runtime wiring for the protocol method or capability. "Partial" means the
crate recognizes the protocol shape but does not yet expose the full expected
behavior to Helix.

| Protocol area | Direction | Status | Notes |
| --- | --- | --- | --- |
| `initialize` | Client -> Agent | Supported | Sends protocol version, client implementation metadata, and advertised client capabilities. |
| `authenticate` | Client -> Agent | Supported | Invoked after initialize when the agent advertises auth methods; prefers `cursor_login`. |
| `logout` | Client -> Agent | Not implemented | No authenticated session lifecycle is tracked. |
| `session/new` | Client -> Agent | Supported | Used by `AgentCommand::StartSession` with cwd and MCP servers. |
| `session/load` | Client -> Agent | Supported | Used by `AgentCommand::LoadSession`; transcript replay depends on agent behavior. |
| `session/list` | Client -> Agent | Supported | Used by the agent history picker; follows `nextCursor` pagination when present. |
| `session/prompt` | Client -> Agent | Supported | Sends text prompts to the active session and reports turn start/finish events. |
| `session/cancel` | Client -> Agent | Supported | Used by `AgentCommand::Cancel` during in-flight turns. |
| `session/close` | Client -> Agent | Not implemented | `AgentCommand::Stop` clears local session state only. |
| `session/resume` | Client -> Agent | Not implemented | Resume-without-history is not modeled separately from `session/load`. |
| `session/set_config_option` | Client -> Agent | Not implemented | Agent config options are not surfaced. |
| `session/set_mode` | Client -> Agent | Supported | Used by `AgentCommand::SetMode`; default mode can be applied after session start/load. |
| `session/update` | Agent -> Client | Partial | Message chunks, thought chunks, tool calls, plans, and session info updates are mapped to `AgentEvent`; unsupported metadata updates are ignored. |
| `session/request_permission` | Agent -> Client | Partial | Requests are accepted and auto-approved with Cursor-friendly option preference; interactive UI is not implemented yet. |
| `fs/read_text_file` | Agent -> Client | Supported | Advertised through `fs.readTextFile` and routed to the caller-provided `FsReadFn`. |
| `fs/write_text_file` | Agent -> Client | Supported | Advertised through `fs.writeTextFile` and routed to the caller-provided `FsWriteFn`; conflicts and errors are returned as JSON-RPC errors. |
| `terminal/create` | Agent -> Client | Not implemented | Terminal capabilities are not advertised. |
| `terminal/output` | Agent -> Client | Not implemented | Terminal capabilities are not advertised. |
| `terminal/wait_for_exit` | Agent -> Client | Not implemented | Terminal capabilities are not advertised. |
| `terminal/kill` | Agent -> Client | Not implemented | Terminal capabilities are not advertised. |
| `terminal/release` | Agent -> Client | Not implemented | Terminal capabilities are not advertised. |
| prompt text content | Client -> Agent | Supported | `SendPrompt` sends `ContentBlock::Text`. |
| prompt image content | Client -> Agent | Not implemented | Image prompt capability is not advertised or modeled. |
| prompt audio content | Client -> Agent | Not implemented | Audio prompt capability is not advertised or modeled. |
| embedded context | Client -> Agent | Not implemented | Embedded context prompt capability is not advertised or modeled. |
| MCP server capabilities | Client -> Agent | Partial | Cursor `.cursor/mcp.json` stdio servers are passed on session/new and session/load. |
| Cursor extensions | Agent -> Client | Partial | Blocking ask/create-plan requests are bridged to Helix UI; notification methods render in transcript. |

## What This Crate Does Not Own

This crate deliberately avoids terminal UI and editor-buffer policy. The
caller is responsible for:

- rendering transcripts and prompt input
- storing editor/session UI state
- deciding how ACP file reads and writes interact with open Helix documents
- surfacing dirty-buffer conflicts or permission prompts to users
- starting and polling the runtime from the editor event loop

In Helix, those responsibilities live in `helix-view` and `helix-term`.

## Current Limitations

- Only local subprocess agents over stdio are supported.
- Remote ACP transports such as HTTP or WebSocket are not implemented here.
- Permission requests are not yet interactive at this layer.
- Transcript replay on `session/load` depends on the agent. Cursor's `agent acp`
  currently restores session metadata without streaming prior messages; see
  `book/src/agent.md` for details.
- Filesystem writes are full-file `writeTextFile` operations; structured
  partial edits are expected to be handled by higher-level editor code.

## Main Types

- `AgentConfig`: runtime configuration for the local ACP command.
- `AgentRuntime`: spawns and owns the background ACP client task.
- `AgentRuntimeHandle`: sends `AgentCommand`s to the runtime.
- `AgentCommand`: commands from UI/editor code to the ACP runtime.
- `AgentEvent`: protocol updates emitted back to UI/editor code.
- `AgentMessage`: transcript-style message items derived from ACP updates.
- `AgentSessionId`: opaque ACP session identifier.
- `FsReadFn` and `FsWriteFn`: callbacks used to route ACP filesystem requests
  through the editor.
