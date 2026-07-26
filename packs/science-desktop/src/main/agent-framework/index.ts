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

import type { AgentFramework, AgentFrameworkId } from './types'

// ── No-op agent framework stubs ──────────────────────────────────

/**
 * Human names for every framework id (LS5-D1-02).
 *
 * The registry used to be the only source of a display name, via `AgentFramework.displayName`.
 * It no longer registers anything, but the app still has to NAME these runtimes: the onboarding
 * environment check probes the host for the `claude` / `opencode` / `codex` binaries and reports
 * what it found, and that report is about the binary on disk, not about a registered peer runtime.
 * So the labels live here, independent of registration, and `agentFrameworkLabel()` is the one way
 * to ask for one.
 */
export const AGENT_FRAMEWORK_LABELS: Record<AgentFrameworkId, string> = {
  'claude-code': 'Claude Code',
  opencode: 'OpenCode',
  codex: 'Codex',
  'lumen-stubbed': 'Lumen SessionActor',
  'claude-code-stubbed': 'Claude Code (STUBBED — use Lumen bridge)',
  'opencode-stubbed': 'OpenCode (STUBBED — use Lumen bridge)',
  'codex-stubbed': 'Codex (STUBBED — use Lumen bridge)'
}

/** Replaced by Lumen SessionActor. */
export const claudeCodeFramework = Object.freeze({
  id: 'claude-code-stubbed' as const,
  name: AGENT_FRAMEWORK_LABELS['claude-code-stubbed'],
  description: 'EXECUTION AUTHORITY REMOVED. Route via lumen-acp-bridge.ts.',
})

/** Replaced by Lumen SessionActor. */
export const codexFramework = Object.freeze({
  id: 'codex-stubbed' as const,
  name: AGENT_FRAMEWORK_LABELS['codex-stubbed'],
  description: 'EXECUTION AUTHORITY REMOVED. Route via lumen-acp-bridge.ts.',
})

/** Replaced by Lumen SessionActor. */
export const opencodeFramework = Object.freeze({
  id: 'opencode-stubbed' as const,
  name: AGENT_FRAMEWORK_LABELS['opencode-stubbed'],
  description: 'EXECUTION AUTHORITY REMOVED. Route via lumen-acp-bridge.ts.',
})

export const DEFAULT_AGENT_FRAMEWORK_ID = 'lumen-stubbed' as const

/**
 * The stub contract (LS5-D1-02).
 *
 * These two functions are the module's whole public behaviour, and the *types* are the contract —
 * `null` / `[]` are not placeholders to be tightened later, they are the permanent answer: this
 * pack registers no peer agent runtime. Annotating them keeps that promise checkable at every call
 * site. Before this, `getAgentFramework()` was inferred as `() => null` (so passing an id was a
 * compile error) and `listAgentFrameworks()` as `() => never[]` (so reading any field off an
 * element was a compile error) — both symptoms of an unstated contract, not of bad callers.
 *
 * Callers MUST handle the null/empty case. Several did not, and would have thrown a TypeError at
 * runtime; those are fixed in settings/service.ts alongside this change.
 */

/** Always null: no framework is registered as a peer execution authority. */
export function getAgentFramework(_id: AgentFrameworkId): AgentFramework | null {
  return null
}

/** Always empty: the settings selector has no frameworks to offer. */
export function listAgentFrameworks(): readonly AgentFramework[] {
  return []
}

/**
 * The display name for a framework id, whether or not one is registered.
 * Prefers a registered framework's own `displayName` so this keeps working if a real registry ever
 * returns; falls back to the static label above, which is what actually happens today.
 */
export function agentFrameworkLabel(id: AgentFrameworkId): string {
  return getAgentFramework(id)?.displayName ?? AGENT_FRAMEWORK_LABELS[id]
}
