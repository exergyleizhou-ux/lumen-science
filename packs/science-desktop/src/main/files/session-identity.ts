/**
 * Trusted preview/session identity for Lumen Science Desktop.
 *
 * Owner/project for artifact access MUST come from main-process session
 * state set at project/session open — never from renderer self-attestation.
 *
 * Sender-scoped map is the authority path for ZIP/.skill quarantine.
 * A process-global bag remains only for legacy consumers that have not yet
 * migrated (notebook/review/compute/skill/preview). Those remain P0.
 *
 * State lives on globalThis so ESM/CJS dual-graph (tsx .js vs .ts imports)
 * still share one identity — OSF-2 isolation must not silently diverge.
 */

export interface TrustedPreviewContext {
  ownerId: string
  projectId: string
}

const GLOBAL_KEY = '__lumenTrustedPreviewContext__'
const SENDER_MAP_KEY = '__lumenTrustedPreviewContextBySender__'
const SENDER_CLEANUP_KEY = '__lumenTrustedPreviewSenderCleanupAttached__'
const SENDER_EPOCH_KEY = '__lumenTrustedPreviewContextEpochBySender__'

type GlobalBag = typeof globalThis & {
  [GLOBAL_KEY]?: TrustedPreviewContext | null
  [SENDER_MAP_KEY]?: Map<number, TrustedPreviewContext>
  [SENDER_CLEANUP_KEY]?: Set<number>
  [SENDER_EPOCH_KEY]?: Map<number, number>
}

function bag(): GlobalBag {
  return globalThis as GlobalBag
}

function senderMap(): Map<number, TrustedPreviewContext> {
  const g = bag()
  if (!g[SENDER_MAP_KEY]) {
    g[SENDER_MAP_KEY] = new Map()
  }
  return g[SENDER_MAP_KEY]!
}

function cleanupAttached(): Set<number> {
  const g = bag()
  if (!g[SENDER_CLEANUP_KEY]) {
    g[SENDER_CLEANUP_KEY] = new Set()
  }
  return g[SENDER_CLEANUP_KEY]!
}

function senderEpochs(): Map<number, number> {
  const g = bag()
  if (!g[SENDER_EPOCH_KEY]) {
    g[SENDER_EPOCH_KEY] = new Map()
  }
  return g[SENDER_EPOCH_KEY]!
}

function assertSenderId(senderId: number): void {
  if (!Number.isInteger(senderId) || senderId < 0) {
    throw new Error('trusted preview senderId must be a non-negative integer')
  }
}

/**
 * Revoke the current capability and open one compare-and-set generation for an
 * asynchronous membership assertion.
 */
export function beginTrustedPreviewContextBinding(senderId: number): number {
  assertSenderId(senderId)
  const epochs = senderEpochs()
  const epoch = (epochs.get(senderId) ?? 0) + 1
  epochs.set(senderId, epoch)
  senderMap().delete(senderId)
  return epoch
}

/** Publish membership only if no navigation/unbind/rebind happened meanwhile. */
export function commitTrustedPreviewContextForSender(
  senderId: number,
  epoch: number,
  ctx: TrustedPreviewContext,
): boolean {
  assertSenderId(senderId)
  if (!ctx.ownerId || !ctx.projectId) {
    throw new Error('trusted preview context requires ownerId and projectId')
  }
  if (senderEpochs().get(senderId) !== epoch) {
    return false
  }
  senderMap().set(senderId, { ownerId: ctx.ownerId, projectId: ctx.projectId })
  return true
}

/** @deprecated Process-global identity — residual P0 for non-ZIP consumers. */
export function setTrustedPreviewContext(ctx: TrustedPreviewContext): void {
  if (!ctx.ownerId || !ctx.projectId) {
    throw new Error('trusted preview context requires ownerId and projectId')
  }
  bag()[GLOBAL_KEY] = { ownerId: ctx.ownerId, projectId: ctx.projectId }
}

/** @deprecated Process-global identity — residual P0 for non-ZIP consumers. */
export function getTrustedPreviewContext(): TrustedPreviewContext | null {
  return bag()[GLOBAL_KEY] ?? null
}

/** @deprecated Process-global identity — residual P0 for non-ZIP consumers. */
export function clearTrustedPreviewContext(): void {
  bag()[GLOBAL_KEY] = null
}

export function setTrustedPreviewContextForSender(
  senderId: number,
  ctx: TrustedPreviewContext,
): void {
  const epoch = beginTrustedPreviewContextBinding(senderId)
  commitTrustedPreviewContextForSender(senderId, epoch, ctx)
}

export function getTrustedPreviewContextForSender(senderId: number): TrustedPreviewContext | null {
  if (!Number.isInteger(senderId) || senderId < 0) {
    return null
  }
  return senderMap().get(senderId) ?? null
}

export function clearTrustedPreviewContextForSender(senderId: number): void {
  if (!Number.isInteger(senderId) || senderId < 0) {
    return
  }
  const epochs = senderEpochs()
  epochs.set(senderId, (epochs.get(senderId) ?? 0) + 1)
  senderMap().delete(senderId)
}

/** Engine stop/restart or full identity invalidation. */
export function clearAllTrustedPreviewContexts(): void {
  bag()[GLOBAL_KEY] = null
  const epochs = senderEpochs()
  for (const senderId of epochs.keys()) {
    epochs.set(senderId, (epochs.get(senderId) ?? 0) + 1)
  }
  senderMap().clear()
}

export function listTrustedPreviewSenderIds(): number[] {
  return [...senderMap().keys()].sort((a, b) => a - b)
}

/**
 * Electron WebContents-like surface used to clear identity when the renderer
 * dies, navigates away, or the window is destroyed.
 */
export type TrustedIdentitySender = {
  id: number
  on: (event: string, listener: (...args: unknown[]) => void) => void
}

/**
 * Attach one-shot cleanup for a sender. Safe to call repeatedly; only the
 * first registration per sender id attaches listeners.
 */
export function attachTrustedIdentitySenderCleanup(sender: TrustedIdentitySender): void {
  const senderId = sender.id
  if (!Number.isInteger(senderId) || senderId < 0) {
    return
  }
  const attached = cleanupAttached()
  if (attached.has(senderId)) {
    return
  }
  attached.add(senderId)
  const revoke = (): void => {
    clearTrustedPreviewContextForSender(senderId)
  }
  const release = (): void => {
    revoke()
    cleanupAttached().delete(senderId)
  }
  // destroyed / render-process-gone cover crash and window close.
  // did-navigate-main-frame / did-frame-navigate cover main-frame navigation.
  const on = sender.on.bind(sender)
  // Keep one listener set for the lifetime of this WebContents. Unbind,
  // navigation and engine restart revoke identity but must not cause a later
  // rebind to attach duplicate stale one-shot listeners that can clear the
  // new capability.
  on('destroyed', release)
  on('render-process-gone', revoke)
  on('did-navigate', revoke)
  on('did-navigate-in-page', revoke)
}
