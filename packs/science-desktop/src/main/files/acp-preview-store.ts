/**
 * ACP-backed PreviewFileStore.
 *
 * Resolves artifact_id via an in-process metadata index that is filled from
 * Lumen ACP responses (artifact_list / write acknowledgements). Content
 * authority remains in Rust; this store only holds path + digest + ownership
 * for the Electron isolation gate.
 *
 * When ACP is unavailable, index can be seeded in tests/fixtures.
 */

import type { PreviewFileRecord, PreviewFileStore } from './preview-resolver'

export type AcpCallFn = (
  toolName: string,
  args: Record<string, unknown>,
) => Promise<unknown>

export class AcpPreviewStore implements PreviewFileStore {
  private index = new Map<string, PreviewFileRecord>()

  constructor(private readonly acpCall?: AcpCallFn) {}

  /** Seed or update metadata (from ACP list/write or fixtures). */
  put(artifactId: string, record: PreviewFileRecord): void {
    this.index.set(artifactId, record)
  }

  clear(): void {
    this.index.clear()
  }

  async resolveById(artifactId: string): Promise<PreviewFileRecord | null> {
    const hit = this.index.get(artifactId)
    if (hit) return hit

    // Optional remote refresh: list is project-scoped; without project we cannot
    // scan the whole store. Callers seed via put() after session open / list.
    if (this.acpCall) {
      try {
        const result = await this.acpCall('artifact_resolve', { artifact_id: artifactId })
        const record = normalizeResolveResult(result)
        if (record) {
          this.index.set(artifactId, record)
          return record
        }
      } catch {
        // fail closed — index miss
      }
    }
    return null
  }
}

function normalizeResolveResult(result: unknown): PreviewFileRecord | null {
  if (!result || typeof result !== 'object') return null
  const r = result as Record<string, unknown>
  // Unwrap common ACP/MCP text envelopes
  const body = (r.meta as Record<string, unknown>) ?? r
  const path = String(body.path ?? body.storage_path ?? '')
  const sha256 = String(body.sha256 ?? body.digest ?? '')
  const ownerId = String(body.owner_id ?? body.ownerId ?? '')
  const projectId = String(body.project_id ?? body.projectId ?? '')
  const runId = String(body.run_id ?? body.runId ?? '')
  if (!path || !sha256 || !ownerId || !projectId) return null
  return { path, sha256, ownerId, projectId, ...(runId ? { runId } : {}) }
}

/** Shared singleton used by science IPC registration. */
export const defaultAcpPreviewStore = new AcpPreviewStore()
