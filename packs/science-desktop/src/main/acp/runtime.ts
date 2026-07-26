/**
 * LUMEN STUB: ACP runtime — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Fully-featured ACP agent runtime with Claude/Codex/OpenCode backends.
 *
 * Lumen Science Desktop: accepts the same constructor shape for drop-in
 * compatibility with acp/ipc.ts and runtime-coordinator.ts, but all
 * execution paths are no-ops. Science execution is through Rust Lumen.
 *
 * See: packs/science-desktop/ARCHITECTURE.md
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

// Re-export all types needed by callers
export type {
  ActiveSession,
  ClientConnection,
  ContentBlock,
  McpServer,
  PromptResponse,
  SessionConfigOption,
  SessionModeState,
  SessionNotification,
}

import type {
  AcpCancelPromptRequest,
  AcpConnectRequest,
  AcpContextUsage,
  AcpCreateSessionRequest,
  AcpCreateSessionResponse,
  AcpDeleteSessionRequest,
  AcpPermissionRequest,
  AcpPermissionResponse,
  AcpPromptRequest,
  AcpResumeSessionRequest,
  AcpRevokePermissionGrantRequest,
  AcpRuntimeEvent,
  AcpSetPermissionProfileRequest,
  AcpStateSnapshot,
} from '../../shared/acp'

export type {
  AcpCancelPromptRequest,
  AcpConnectRequest,
  AcpContextUsage,
  AcpCreateSessionRequest,
  AcpCreateSessionResponse,
  AcpDeleteSessionRequest,
  AcpPermissionRequest,
  AcpPermissionResponse,
  AcpPromptRequest,
  AcpResumeSessionRequest,
  AcpRevokePermissionGrantRequest,
  AcpRuntimeEvent,
  AcpSetPermissionProfileRequest,
  AcpStateSnapshot,
}

// Re-export shared acp helpers (keep tests compiling)
export { getAcpRuntimeEventImage, MAX_ACP_SESSION_IMAGE_BYTES } from '../../shared/acp'
export { ACP_PROMPT_FAILED_EVENT_TITLE } from '../../shared/acp'

// ── Compatible runtime callbacks type ────────────────────────────

export type AcpRuntimeCallbacks = Record<string, (...args: unknown[]) => void>

export type ReviewerSessionDisposition = Record<string, never>

// ── Stub class — drop-in replacement for original AcpRuntime ─────

export class AcpRuntime {
  constructor(_opts?: Record<string, unknown>) {
    console.warn(
      '[lumen-stub] AcpRuntime constructed — EXECUTION AUTHORITY REMOVED.\n' +
        'All agent backend execution is stubbed. Science ops route via:\n' +
        '  Rust Lumen SessionActor → ACP bridge (lumen-acp-bridge.ts)\n' +
        'See: packs/science-desktop/ARCHITECTURE.md'
    )
  }

  get connectedAgents(): readonly never[] {
    return []
  }

  connect(): Promise<ClientConnection> {
    return Promise.reject(new Error('AcpRuntime.connect() — STUBBED. Use lumen-acp-bridge.ts'))
  }

  listAgentSessions(): Promise<ActiveSession[]> {
    return Promise.resolve([])
  }

  createSession(): Promise<AcpCreateSessionResponse> {
    return Promise.reject(new Error('AcpRuntime.createSession() — STUBBED'))
  }

  prompt(): Promise<PromptResponse> {
    return Promise.reject(new Error('AcpRuntime.prompt() — STUBBED'))
  }

  cancelPrompt(): Promise<void> {
    return Promise.resolve()
  }

  requestPermission(): Promise<AcpPermissionResponse> {
    return Promise.resolve({
      outcome: 'cancelled',
      reason: 'Permission broker stubbed — authority is Rust Lumen',
    } as AcpPermissionResponse)
  }

  deleteSession(): Promise<void> {
    return Promise.resolve()
  }

  resumeSession(): Promise<AcpCreateSessionResponse> {
    return Promise.reject(new Error('AcpRuntime.resumeSession() — STUBBED'))
  }

  revokePermissionGrant(): Promise<void> {
    return Promise.resolve()
  }

  listMcpServers(): Promise<McpServer[]> {
    return Promise.resolve([])
  }

  setPermissionProfile(): Promise<void> {
    return Promise.resolve()
  }

  disconnect(): Promise<void> {
    return Promise.resolve()
  }

  on(_event: string, _callback: (...args: unknown[]) => void): void { /* no-op */ }
  off(_event: string, _callback: (...args: unknown[]) => void): void { /* no-op */ }
  destroy(): void { /* no-op */ }
}
