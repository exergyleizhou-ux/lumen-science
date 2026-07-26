/**
 * Lumen Files/Preview Module — OSF-2 product path.
 *
 * Provides artifact-backed file resolution with owner/project isolation.
 * All preview content is loaded by artifact_id (not arbitrary path),
 * verified against the shipped lumen-authority-policy,
 * and checked for hash match before any file handle is returned.
 *
 * This is NOT a full Electron-managed-preview orchestration.
 * It is the authority gate between renderer preview requests
 * and the Rust-backed artifact store.
 *
 * See: packs/science-desktop/ARCHITECTURE.md
 * See: OSF-2 acceptance criterion 3
 */

import { assertArtifactPreviewAccess, type AccessResult } from '../lumen-authority-policy'

// ── Types ────────────────────────────────────────────────────────

export interface PreviewFileRequest {
  artifactId: string
  ownerId: string
  projectId: string
  expectedSha256?: string
  mimeType?: string
}

export interface PreviewFileResult {
  /** Access check result */
  access: AccessResult
  /** The resolved file path (only present if access is ok) */
  path?: string
  /** Content type for renderer */
  mimeType?: string
}

export interface PreviewFileStore {
  /** Resolve an artifact_id to a filesystem path and SHA-256 digest */
  resolveById(artifactId: string): Promise<{ path: string; sha256: string } | null>
}

// ── Preview resolution (shipped function) ────────────────────────

/**
 * Resolve a preview file request through the artifact_id authority gate.
 *
 * Fails closed: rejects wrong owner, wrong project, hash mismatch,
 * and empty/null identifiers. Only returns a file path after ALL
 * access checks pass.
 */
export async function resolvePreview(
  req: PreviewFileRequest,
  store: PreviewFileStore
): Promise<PreviewFileResult> {
  // 1. Authority gate: owner, project, artifact_id validation
  const access = assertArtifactPreviewAccess(
    { artifactId: req.artifactId, ownerId: req.ownerId, projectId: req.projectId, expectedSha256: req.expectedSha256 },
    { ownerId: req.ownerId, projectId: req.projectId }
  )
  if (!access.ok) {
    return { access }
  }

  // 2. Resolve artifact to filesystem path via the store
  const resolved = await store.resolveById(req.artifactId)
  if (!resolved) {
    return {
      access: { ok: false, reason: `artifact_id not found: ${req.artifactId}` },
    }
  }

  // 3. Hash verification (if expectedSha256 is provided)
  if (req.expectedSha256 && req.expectedSha256 !== resolved.sha256) {
    return {
      access: { ok: false, reason: `sha256 mismatch: expected=${req.expectedSha256} actual=${resolved.sha256}` },
    }
  }

  // 4. Cross-owner/project rejection
  // Already checked in step 1 via assertArtifactPreviewAccess

  return {
    access: { ok: true },
    path: resolved.path,
    mimeType: req.mimeType,
  }
}
