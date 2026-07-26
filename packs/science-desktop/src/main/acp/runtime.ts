/**
 * STUB: Open Science ACP runtime — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Fully-featured ACP agent runtime with Claude Code / OpenCode / Codex backend.
 *
 * Lumen Science Desktop: this file is a NO-OP stub.
 *   All science execution is routed through the Rust Lumen SessionActor
 *   via the Lumen ACP bridge (lumen-acp-bridge.ts).
 *
 * NEVER re-enable this module's execution paths. They would create a second
 * agent kernel authority, violating the Lumen single-authority architecture.
 *
 * See: packs/science-desktop/ARCHITECTURE.md
 * See: third_party/open-science/NOTICE
 */

import type {
  ActiveSession,
  ClientConnection,
  ContentBlock,
  McpServer,
  PromptResponse,
  SessionConfigOption,
  SessionModeState,
  SessionNotification,
} from '@agentclientprotocol/sdk'

// ── Re-export types for compatibility with existing UI code ──────
// These types are preserved so React components that reference them
// still compile. No functions from this module should be called.

export type AcpRuntimeCallbacks = Record<string, never>

export type ReviewerSessionDisposition = Record<string, never>

import type {
  AcpCancelPromptRequest,
  AcpConnectRequest,
  AcpCreateSessionRequest,
  AcpCreateSessionResponse,
  AcpRuntimeEvent,
  AcpDeleteSessionRequest,
  AcpPermissionRequest,
  AcpPermissionResponse,
  AcpPromptRequest,
  AcpResumeSessionRequest,
  AcpRevokePermissionGrantRequest,
  AcpContextUsage,
  AcpSetPermissionProfileRequest,
  AcpStateSnapshot,
} from '../../shared/acp'

export type { ActiveSession, ContentBlock, McpServer, PromptResponse, SessionModeState }
export type {
  AcpCancelPromptRequest,
  AcpConnectRequest,
  AcpCreateSessionRequest,
  AcpCreateSessionResponse,
  AcpRuntimeEvent,
  AcpDeleteSessionRequest,
  AcpPermissionRequest,
  AcpPermissionResponse,
  AcpPromptRequest,
  AcpResumeSessionRequest,
  AcpRevokePermissionGrantRequest,
  AcpContextUsage,
  AcpSetPermissionProfileRequest,
  AcpStateSnapshot,
}

// ── Stub class — all methods throw ───────────────────────────────

export class AcpRuntime {
  constructor() {
    console.error(
      '[lumen-stub] AcpRuntime instantiated — EXECUTION AUTHORITY REMOVED.\n' +
        'This module is a NO-OP stub. Science operations must go through:\n' +
        '  Rust Lumen SessionActor → ACP bridge (lumen-acp-bridge.ts)\n' +
        'See: packs/science-desktop/ARCHITECTURE.md'
    )
  }

  get connectedAgents(): readonly never[] {
    return []
  }

  connect(_request: AcpConnectRequest): Promise<ClientConnection> {
    return Promise.reject(
      new Error(
        'AcpRuntime.connect() — EXECUTION AUTHORITY REMOVED.\n' +
          'Use acpCall() from lumen-acp-bridge.ts to reach Rust Lumen.'
      )
    )
  }

  listAgentSessions(_connection: ClientConnection): Promise<ActiveSession[]> {
    return Promise.reject(new Error('AcpRuntime.listAgentSessions() — STUBBED'))
  }

  createSession(
    _connection: ClientConnection,
    _request: AcpCreateSessionRequest
  ): Promise<AcpCreateSessionResponse> {
    return Promise.reject(new Error('AcpRuntime.createSession() — STUBBED'))
  }

  prompt(
    _connection: ClientConnection,
    _request: AcpPromptRequest
  ): Promise<PromptResponse> {
    return Promise.reject(new Error('AcpRuntime.prompt() — STUBBED'))
  }

  cancelPrompt(_request: AcpCancelPromptRequest): Promise<void> {
    return Promise.reject(new Error('AcpRuntime.cancelPrompt() — STUBBED'))
  }

  requestPermission(
    _request: AcpPermissionRequest
  ): Promise<AcpPermissionResponse> {
    // Permissions are managed by Rust SessionActor via Lumen bridge.
    // This stub preserves the API shape so UI code compiles, but
    // runtime permission decisions are NEVER made here.
    return Promise.resolve({
      outcome: 'cancelled',
      reason: 'Permission broker stubbed — use Lumen bridge',
    } as AcpPermissionResponse)
  }

  deleteSession(_request: AcpDeleteSessionRequest): Promise<void> {
    return Promise.reject(new Error('AcpRuntime.deleteSession() — STUBBED'))
  }

  resumeSession(
    _connection: ClientConnection,
    _request: AcpResumeSessionRequest
  ): Promise<AcpCreateSessionResponse> {
    return Promise.reject(new Error('AcpRuntime.resumeSession() — STUBBED'))
  }

  revokePermissionGrant(_request: AcpRevokePermissionGrantRequest): Promise<void> {
    return Promise.reject(new Error('AcpRuntime.revokePermissionGrant() — STUBBED'))
  }

  listMcpServers(): Promise<McpServer[]> {
    return Promise.resolve([])
  }

  setPermissionProfile(_request: AcpSetPermissionProfileRequest): Promise<void> {
    return Promise.reject(new Error('AcpRuntime.setPermissionProfile() — STUBBED'))
  }

  disconnect(_connection: ClientConnection): Promise<void> {
    return Promise.reject(new Error('AcpRuntime.disconnect() — STUBBED'))
  }

  // Keep event emitter-ish API shape for UI compatibility
  on(_event: string, _callback: (...args: unknown[]) => void): void {
    // no-op
  }

  off(_event: string, _callback: (...args: unknown[]) => void): void {
    // no-op
  }

  destroy(): void {
    // no-op — no resources to clean up
  }
}
