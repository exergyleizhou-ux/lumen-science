/**
 * Local type surface for `@agentclientprotocol/sdk` (LS5-D1-02).
 *
 * WHY THIS FILE EXISTS
 * The Lumen absorb removed the Open Science agent-framework execution authority, and with it the
 * `@agentclientprotocol/sdk` dependency: Lumen drives a single execution authority (the Rust
 * SessionActor) and reaches Claude Code / Codex / OpenCode only as controlled adapters behind
 * `src/main/lumen-acp-bridge.ts`. The package is therefore NOT installed and must not be.
 *
 * The ACP *wire types* did not go away, though — persisted sessions, the renderer transcript and
 * `src/shared/acp.ts` still describe tool calls in ACP's shape. Those files are reachable from a
 * production entry point, so the types must resolve. Rather than reinstate the dependency (which
 * would re-admit a peer runtime's SDK into the tree) this declares exactly the surface the pack
 * consumes, and nothing more.
 *
 * SCOPE RULE: only add a member here when a reachable file actually imports it. Everything below is
 * pinned to a real consumer, named in the comment above it. If the real SDK is ever reinstated,
 * delete this file — it is a subset, not a fork.
 *
 * Shapes follow the Agent Client Protocol schema (agentclientprotocol.com) as consumed by this
 * pack; the discriminants are cross-checked against the exhaustive `Record<ToolKind, string>` in
 * `src/renderer/src/pages/workspace/workspace-tool-activity-details.ts`.
 */
declare module '@agentclientprotocol/sdk' {
  // ── tool calls ────────────────────────────────────────────────────────────
  // Consumer: workspace-tool-activity-details.ts TOOL_KIND_LABELS is a total
  // Record<ToolKind, string>, so this union must match it exactly.
  export type ToolKind =
    | 'read'
    | 'edit'
    | 'delete'
    | 'move'
    | 'search'
    | 'execute'
    | 'think'
    | 'fetch'
    | 'switch_mode'
    | 'other'

  // Consumer: session-store.ts re-exports this as ToolActivityStatus.
  export type ToolCallStatus = 'pending' | 'in_progress' | 'completed' | 'failed'

  // Consumer: shared/acp.ts (AcpToolActivity.toolLocations), session-store.ts,
  // PermissionApprovalControls.tsx and workspace-tool-activity-details.ts, which read `.path`.
  export type ToolCallLocation = {
    path: string
    line?: number | null
  }

  // Embedded resource payload of a `resource` content block. Consumers narrow with
  // `'text' in content.resource`, so the text/blob split has to stay a discriminated union.
  export type EmbeddedResource =
    | { uri: string; mimeType?: string | null; text: string }
    | { uri: string; mimeType?: string | null; blob: string }

  // Consumer: workspace-web-search-details.ts and workspace-tool-activity-details.ts switch on
  // `.type` and read text / resource / resource_link fields; other variants fall to `default`.
  export type ContentBlock =
    | { type: 'text'; text: string }
    | { type: 'image'; data: string; mimeType: string; uri?: string | null }
    | { type: 'audio'; data: string; mimeType: string }
    | { type: 'resource'; resource: EmbeddedResource }
    | {
        type: 'resource_link'
        uri: string
        name: string
        title?: string | null
        description?: string | null
        mimeType?: string | null
        size?: number | null
      }

  // Consumer: shared/acp.ts (AcpToolActivity.toolContent). Renderer code narrows with
  // Extract<ToolCallContent, { type: 'content' }> and Extract<…, { type: 'diff' }>, then reads
  // `content`, and `path` / `oldText` / `newText` respectively — so both must stay discriminated.
  export type ToolCallContent =
    | { type: 'content'; content: ContentBlock }
    | { type: 'diff'; path: string; oldText?: string | null; newText: string }
    | { type: 'terminal'; terminalId: string }

  // ── session modes ─────────────────────────────────────────────────────────
  // Consumer: acp/permission-profile-controller.ts reads `modes.availableModes[].id`; the agent
  // framework adapters pass it straight through to mapPermissionProfile().
  export type SessionMode = {
    id: string
    name: string
    description?: string | null
  }

  export type SessionModeState = {
    currentModeId: string
    availableModes: SessionMode[]
  }

  // ── MCP server declarations handed to the agent at session/new ────────────
  // Consumer: artifacts/, notebook/ and activity-groups/ mcp-server.ts build stdio configs;
  // reviewer/mcp-server.ts builds the http variant. env/headers are name/value pairs, not maps.
  export type EnvVariable = { name: string; value: string }
  export type HttpHeader = { name: string; value: string }

  export type McpServerStdio = {
    name: string
    command: string
    args: string[]
    env: EnvVariable[]
  }

  export type McpServerHttp = {
    type: 'http'
    name: string
    url: string
    headers: HttpHeader[]
  }

  export type McpServerSse = {
    type: 'sse'
    name: string
    url: string
    headers: HttpHeader[]
  }

  export type McpServer = McpServerStdio | McpServerHttp | McpServerSse

  // ── client connection (values, not just types) ────────────────────────────
  // Consumer: settings/codex-auth.ts only — it opens a short-lived ndjson ACP connection to
  // codex-acp to read/refresh ChatGPT auth status. Nothing else in the pack constructs a
  // connection; the session runtime goes through lumen-acp-bridge.ts instead.
  export const PROTOCOL_VERSION: number

  export const methods: {
    agent: {
      initialize: string
      authenticate: string
    }
  }

  /** Frames a JSON-RPC stream as newline-delimited JSON over web streams. */
  export function ndJsonStream(
    input: WritableStream<Uint8Array>,
    output: ReadableStream<Uint8Array>
  ): AcpStream

  /** Opaque duplex handle produced by ndJsonStream and consumed by client().connect(). */
  export type AcpStream = {
    readonly __acpStream: unique symbol
  }

  export type AcpRequester = {
    /**
     * Sends one JSON-RPC request. `Result` is supplied by the caller because the response shape is
     * method-specific; codex-auth.ts passes its own CodexAuthenticationStatus for the custom
     * `authentication/status` method.
     */
    request<Result = unknown>(method: string, params: Record<string, unknown>): Promise<Result>
  }

  export type AcpConnection = {
    agent: AcpRequester
    close(): void
  }

  export function client(info: { name: string }): {
    connect(stream: AcpStream): AcpConnection
  }
}
