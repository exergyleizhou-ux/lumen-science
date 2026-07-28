/**
 * Lumen Files/Preview Module — OSF-2 product path.
 *
 * Provides artifact-backed file resolution with owner/project isolation.
 * All preview content is loaded by artifact_id (not arbitrary path),
 * verified against the shipped lumen-authority-policy,
 * and checked for hash match before any file handle is returned.
 *
 * Trust model:
 *   - Client supplies artifactId (+ optional expectedSha256 / mimeType)
 *   - Trusted owner/project comes from main-process session identity
 *   - Store returns durable ownership metadata for the artifact
 *   - Policy compares trusted identity to store metadata (fail-closed)
 *
 * See: packs/science-desktop/ARCHITECTURE.md
 * See: OSF-2 acceptance criterion 3
 */

import { createHash } from 'node:crypto'
import fs from 'node:fs/promises'
import { assertArtifactPreviewAccess, type AccessResult } from '../lumen-authority-policy'
import type { TrustedPreviewContext } from './session-identity'

// ── Types ────────────────────────────────────────────────────────

const MAX_PREVIEW_BYTES = 16 * 1024 * 1024

export interface PreviewFileRequest {
  /** Client-supplied artifact id (never a filesystem path) */
  artifactId: string
  expectedSha256?: string
  mimeType?: string
}

export interface PreviewFileResult {
  access: AccessResult
  mimeType?: string
  sha256?: string
  /** Exact verified bytes from the same file-handle read that was hashed. */
  contentBase64?: string
  byteLength?: number
}

export interface PreviewFileRecord {
  path: string
  sha256: string
  ownerId: string
  projectId: string
  /** Durable run that registered these bytes. Required for review evidence. */
  runId?: string
}

export interface PreviewFileStore {
  /** Resolve artifact_id to path + digest + ownership from durable store */
  resolveById(artifactId: string): Promise<PreviewFileRecord | null>
  /**
   * Seed or update a record. Optional: read-only stores may not support it,
   * and callers that seed (e.g. registering a workflow run's committed
   * artifacts) must check before calling rather than assume.
   */
  put?(artifactId: string, record: PreviewFileRecord): void
}

// ── Preview resolution (shipped function) ────────────────────────

/**
 * Resolve a preview file request through the artifact_id authority gate.
 *
 * Fails closed: rejects wrong owner, wrong project, hash mismatch,
 * empty identifiers, and unknown artifact ids. It never returns the backing
 * path: the result contains the exact bytes hashed on the already-open handle,
 * so the renderer cannot reopen a swapped path after verification.
 */
export async function resolvePreview(
  req: PreviewFileRequest,
  store: PreviewFileStore,
  trusted: TrustedPreviewContext,
): Promise<PreviewFileResult> {
  if (!req.artifactId) {
    return {
      access: { ok: false, reason: 'artifact_id, owner_id, and project_id are required' },
    }
  }
  if (!trusted.ownerId || !trusted.projectId) {
    return {
      access: { ok: false, reason: 'trusted session owner_id and project_id are required' },
    }
  }

  // 1. Resolve durable metadata first (ownership lives in the store, not the client)
  const resolved = await store.resolveById(req.artifactId)
  if (!resolved) {
    return {
      access: { ok: false, reason: `artifact_id not found: ${req.artifactId}` },
    }
  }

  // 2. Authority gate: trusted session identity vs store-owned metadata + optional hash
  const access = assertArtifactPreviewAccess(
    {
      artifactId: req.artifactId,
      ownerId: trusted.ownerId,
      projectId: trusted.projectId,
      expectedSha256: req.expectedSha256,
    },
    {
      ownerId: resolved.ownerId,
      projectId: resolved.projectId,
      digest: resolved.sha256,
    },
  )
  if (!access.ok) {
    return { access }
  }

  // 3. Hash re-check when client supplied expectedSha256 (store digest is authoritative)
  if (req.expectedSha256 && req.expectedSha256 !== resolved.sha256) {
    return {
      access: {
        ok: false,
        reason: `sha256 mismatch: expected=${req.expectedSha256} actual=${resolved.sha256}`,
      },
    }
  }

  // 4. The bytes themselves. The record CLAIMS these bytes exist at this path
  // with this digest; a preview that never reads the file would present a
  // missing or silently-modified artifact as previewable, and the claim would
  // only fail later, somewhere the reason is gone. Re-hashing here makes the
  // content address mean what it says every time it is used.
  let handle: fs.FileHandle | undefined
  try {
    handle = await fs.open(resolved.path, 'r')
    const stat = await handle.stat()
    if (!stat.isFile() || stat.size > MAX_PREVIEW_BYTES) {
      return {
        access: {
          ok: false,
          reason: `artifact preview must be a regular file no larger than ${MAX_PREVIEW_BYTES} bytes`,
        },
      }
    }
    const bytes = await handle.readFile()
    if (bytes.length > MAX_PREVIEW_BYTES) {
      return {
        access: {
          ok: false,
          reason: `artifact preview exceeds ${MAX_PREVIEW_BYTES} bytes`,
        },
      }
    }
    const actual = createHash('sha256').update(bytes).digest('hex')
    if (actual !== resolved.sha256) {
      return {
        access: {
          ok: false,
          reason: `artifact bytes do not match their record: expected=${resolved.sha256} actual=${actual}`,
        },
      }
    }
    return {
      access: { ok: true },
      mimeType: req.mimeType,
      sha256: actual,
      contentBase64: bytes.toString('base64'),
      byteLength: bytes.length,
    }
  } catch {
    return {
      access: {
        ok: false,
        reason: 'artifact bytes are unavailable',
      },
    }
  } finally {
    await handle?.close().catch(() => undefined)
  }
}
