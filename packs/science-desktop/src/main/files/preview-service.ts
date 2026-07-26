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
  /**
   * Optional post-access content fetch via ACP (e.g. artifact_preview tool).
   * Not required for isolation tests; path metadata alone is the gate.
   */
  fetchContent?: (record: {
    path: string
    artifactId: string
    sha256: string
  }) => Promise<unknown>
}

/**
 * Product path: session identity → policy → store → optional ACP content.
 */
export async function loadArtifactPreview(
  req: PreviewFileRequest,
  deps: LoadArtifactPreviewDeps,
): Promise<PreviewFileResult & { content?: unknown }> {
  const trusted = getTrustedPreviewContext()
  if (!trusted) {
    return {
      access: {
        ok: false,
        reason: 'no trusted session identity — open a project/session first',
      },
    }
  }

  const result = await resolvePreview(req, deps.store, trusted)
  if (!result.access.ok || !result.path || !deps.fetchContent) {
    return result
  }

  try {
    const content = await deps.fetchContent({
      path: result.path,
      artifactId: req.artifactId,
      sha256: result.sha256 ?? '',
    })
    return { ...result, content }
  } catch (e: unknown) {
    return {
      access: {
        ok: false,
        reason: `content fetch failed: ${(e as Error).message || String(e)}`,
      },
    }
  }
}
