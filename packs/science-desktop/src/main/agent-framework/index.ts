/**
 * STUB: Open Science multi-agent framework — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Supports Claude Code, OpenCode, and Codex as switchable agent backends.
 *
 * Lumen Science Desktop: this file is a NO-OP stub.
 *   Lumen uses a SINGLE execution authority: the Rust SessionActor.
 *   No Claude Code / OpenCode / Codex backend is admitted as a peer authority.
 *
 * These agents may be used as controlled provider/consultant adapters
 * through Lumen's ACP bridge, but NEVER as equal-to-SessionActor runtimes.
 *
 * See: packs/science-desktop/ARCHITECTURE.md
 * See: third_party/open-science/NOTICE
 */

// ── Re-export types for UI compatibility ─────────────────────────

export * from './types'

// ── No-op agent framework stubs ──────────────────────────────────

/** Replaced by Lumen SessionActor. */
export const claudeCodeFramework = Object.freeze({
  id: 'claude-code-stubbed' as const,
  name: 'Claude Code (STUBBED — use Lumen bridge)',
  description: 'EXECUTION AUTHORITY REMOVED. Route via lumen-acp-bridge.ts.',
})

/** Replaced by Lumen SessionActor. */
export const codexFramework = Object.freeze({
  id: 'codex-stubbed' as const,
  name: 'Codex (STUBBED — use Lumen bridge)',
  description: 'EXECUTION AUTHORITY REMOVED. Route via lumen-acp-bridge.ts.',
})

/** Replaced by Lumen SessionActor. */
export const opencodeFramework = Object.freeze({
  id: 'opencode-stubbed' as const,
  name: 'OpenCode (STUBBED — use Lumen bridge)',
  description: 'EXECUTION AUTHORITY REMOVED. Route via lumen-acp-bridge.ts.',
})

export const DEFAULT_AGENT_FRAMEWORK_ID = 'lumen-stubbed' as const

export function getAgentFramework() {
  return null
}

export function listAgentFrameworks() {
  return []
}
