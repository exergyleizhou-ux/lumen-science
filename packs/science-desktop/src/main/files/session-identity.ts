/**
 * Trusted preview/session identity for Lumen Science Desktop.
 *
 * Owner/project for artifact access MUST come from main-process session
 * state set at project/session open — never from renderer self-attestation.
 *
 * State lives on globalThis so ESM/CJS dual-graph (tsx .js vs .ts imports)
 * still share one identity — OSF-2 isolation must not silently diverge.
 */

export interface TrustedPreviewContext {
  ownerId: string
  projectId: string
}

const GLOBAL_KEY = '__lumenTrustedPreviewContext__'

type GlobalBag = typeof globalThis & {
  [GLOBAL_KEY]?: TrustedPreviewContext | null
}

function bag(): GlobalBag {
  return globalThis as GlobalBag
}

export function setTrustedPreviewContext(ctx: TrustedPreviewContext): void {
  if (!ctx.ownerId || !ctx.projectId) {
    throw new Error('trusted preview context requires ownerId and projectId')
  }
  bag()[GLOBAL_KEY] = { ownerId: ctx.ownerId, projectId: ctx.projectId }
}

export function getTrustedPreviewContext(): TrustedPreviewContext | null {
  return bag()[GLOBAL_KEY] ?? null
}

export function clearTrustedPreviewContext(): void {
  bag()[GLOBAL_KEY] = null
}
