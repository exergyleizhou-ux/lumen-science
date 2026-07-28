/**
 * Product entry for OSF-2 Files/Preview.
 *
 * Loads preview by artifact_id using trusted main-process session identity
 * and a durable PreviewFileStore (ACP-backed or fixture).
 */

import {
  resolvePreview,
  type PreviewFileRequest,
  type PreviewFileResult,
  type PreviewFileStore,
} from './preview-resolver'
import { getTrustedPreviewContext } from './session-identity'

export type LoadArtifactPreviewDeps = {
  store: PreviewFileStore
}

/**
 * Product path: session identity → policy → store-owned handle → verified
 * bytes. No path is returned or reopened after the digest check.
 */
export async function loadArtifactPreview(
  req: PreviewFileRequest,
  deps: LoadArtifactPreviewDeps,
): Promise<PreviewFileResult> {
  const trusted = getTrustedPreviewContext()
  if (!trusted) {
    return {
      access: {
        ok: false,
        reason: 'no trusted session identity — open a project/session first',
      },
    }
  }

  return resolvePreview(req, deps.store, trusted)
}
