/**
 * Product entry for OSF-2 Files/Preview.
 *
 * Loads preview by artifact_id using trusted main-process session identity
 * (passed from the IPC boundary — never read from a process-global bag)
 * and a durable PreviewFileStore (ACP-backed or fixture).
 */

import {
  resolvePreview,
  type PreviewFileRequest,
  type PreviewFileResult,
  type PreviewFileStore,
} from './preview-resolver'
import type { TrustedPreviewContext } from './session-identity'

export type LoadArtifactPreviewDeps = {
  store: PreviewFileStore
}

/**
 * Product path: session identity → policy → store-owned handle → verified
 * bytes. No path is returned or reopened after the digest check.
 *
 * `trusted` must come from requireSenderTrustedContext / trySenderTrustedContext
 * at the IPC boundary. Services never self-load identity.
 */
export async function loadArtifactPreview(
  req: PreviewFileRequest,
  deps: LoadArtifactPreviewDeps,
  trusted: TrustedPreviewContext | null,
): Promise<PreviewFileResult> {
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
