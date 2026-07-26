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

import { assertArtifactPreviewAccess, type AccessResult } from '../lumen-authority-policy'
import type { TrustedPreviewContext } from './session-identity'

// ── Types ────────────────────────────────────────────────────────

export interface PreviewFileRequest {
  /** Client-supplied artifact id (never a filesystem path) */
  artifactId: string
  expectedSha256?: string
  mimeType?: string
}

export interface PreviewFileResult {
  access: AccessResult
  path?: string
  mimeType?: string
  sha256?: string
}

export interface PreviewFileRecord {
  path: string
  sha256: string
  ownerId: string
  projectId: string
}

export interface PreviewFileStore {
  /** Resolve artifact_id to path + digest + ownership from durable store */
  resolveById(artifactId: string): Promise<PreviewFileRecord | null>
}

// ── Preview resolution (shipped function) ────────────────────────

/**
 * Resolve a preview file request through the artifact_id authority gate.
 *
 * Fails closed: rejects wrong owner, wrong project, hash mismatch,
 * empty identifiers, and unknown artifact ids. Only returns a file path
 * after ALL access checks pass against trusted session context.
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

  return {
    access: { ok: true },
    path: resolved.path,
    mimeType: req.mimeType,
    sha256: resolved.sha256,
  }
}
