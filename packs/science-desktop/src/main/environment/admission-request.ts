/**
 * Kernel admission request construction (LS5-K4) — pure, no Electron, no spawn.
 *
 * Turns a user-selected, absolute candidate path into the parameters of the
 * engine's `x.ai/science/kernel_admission` method. It does not execute or hash
 * the candidate. The answer — including the executable and package-lock
 * digests — is produced by the Rust SessionActor after permission.
 *
 * The asymmetry is deliberate and is the point of the adapter split. The
 * desktop is well placed to *enumerate candidate paths*: PATH, conda roots,
 * framework installs and the app's own envs. It sends a path and receives an
 * actor-owned verdict. Optional caller digests remain assertion fields for
 * non-desktop clients, but this adapter never manufactures them.
 */

import type { KernelKindName } from './interpreter-identity'

/**
 * Wire parameters for `x.ai/science/kernel_admission`.
 *
 * camelCase, and exactly these keys: the engine's parameter struct is
 * `#[serde(rename_all = "camelCase", deny_unknown_fields)]`, so an extra field
 * is a hard parse error rather than an ignored hint
 * (agent/crates/codegen/xai-grok-shell/src/extensions/science.rs).
 */
export type KernelAdmissionParams = {
  sessionId: string
  ownerId: string
  projectId: string
  storeRoot: string
  kernelId: string
  kind: KernelKindName
  interpreterPath: string
  allowedRoot?: string
  execHash?: string
  packageLockPath?: string
  lockHash?: string
  probeTimeoutMs: number
  approvalTimeoutMs: number
}

export type BuildAdmissionRequest = {
  sessionId: string
  ownerId: string
  projectId: string
  storeRoot: string
  /** Stable name for this kernel within the project. */
  kernelId: string
  kind: KernelKindName
  interpreterPath: string
  packageLockPath?: string
  execHash?: string
  lockHash?: string
  /**
   * Confinement root for the resolved interpreter, when the caller wants one.
   *
   * Passed through unevaluated. The desktop could compare paths itself, but a
   * containment check performed here and trusted there would be a decision
   * made in the adapter; the engine resolves and checks it against the
   * interpreter it probed, which is the only comparison that means anything.
   */
  allowedRoot?: string
  probeTimeoutMs?: number
  approvalTimeoutMs?: number
}

export type BuildAdmissionResult =
  | { ok: true; method: 'kernel_admission'; params: KernelAdmissionParams }
  | { ok: false; reason: string }

/** The engine rejects anything outside this range before it probes. */
export const MIN_PROBE_TIMEOUT_MS = 1
export const MAX_PROBE_TIMEOUT_MS = 120_000
export const DEFAULT_ADMISSION_PROBE_TIMEOUT_MS = 10_000
export const MIN_APPROVAL_TIMEOUT_MS = 1
export const MAX_APPROVAL_TIMEOUT_MS = 300_000
export const DEFAULT_ADMISSION_APPROVAL_TIMEOUT_MS = 60_000

const SHA256_HEX = /^[0-9a-f]{64}$/
const isAbsolutePath = (value: string): boolean =>
  value.startsWith('/') || /^([A-Za-z]:[\\/]|\\\\)/.test(value)

/**
 * Build the request, or say why it cannot be built.
 *
 * Every rejection here is a malformed *request* — a missing session, an
 * unpinned path, a digest that is not a digest. None of them is a statement
 * about whether the kernel is admissible, which is why the failure shape is a
 * `reason` string rather than anything resembling an admission status.
 */
export const buildKernelAdmissionRequest = (
  request: BuildAdmissionRequest,
): BuildAdmissionResult => {
  const sessionId = request.sessionId?.trim()
  const ownerId = request.ownerId?.trim()
  const projectId = request.projectId?.trim()
  const storeRoot = request.storeRoot?.trim()
  const kernelId = request.kernelId?.trim()
  const interpreterPath = request.interpreterPath?.trim()

  if (!sessionId) return { ok: false, reason: 'sessionId is required' }
  if (!ownerId) return { ok: false, reason: 'ownerId is required' }
  if (!projectId) return { ok: false, reason: 'projectId is required' }
  if (!storeRoot) return { ok: false, reason: 'storeRoot is required' }
  if (!kernelId) {
    return {
      ok: false,
      reason: 'kernelId is required: an admission record needs a name',
    }
  }
  if (!interpreterPath) {
    return { ok: false, reason: 'interpreterPath is required' }
  }
  if (!isAbsolutePath(interpreterPath)) {
    return {
      ok: false,
      reason: `interpreterPath '${interpreterPath}' is not absolute`,
    }
  }
  if (request.allowedRoot && !isAbsolutePath(request.allowedRoot)) {
    return {
      ok: false,
      reason: `allowedRoot '${request.allowedRoot}' is not absolute`,
    }
  }
  if (request.packageLockPath && !isAbsolutePath(request.packageLockPath)) {
    return {
      ok: false,
      reason: `packageLockPath '${request.packageLockPath}' is not absolute`,
    }
  }
  if (request.execHash && !SHA256_HEX.test(request.execHash)) {
    return {
      ok: false,
      reason: `execHash '${request.execHash}' is not a lowercase sha256 hex digest`,
    }
  }
  if (request.lockHash && !SHA256_HEX.test(request.lockHash)) {
    return {
      ok: false,
      reason: `lockHash '${request.lockHash}' is not a lowercase sha256 hex digest`,
    }
  }
  if (request.lockHash && !request.packageLockPath) {
    return {
      ok: false,
      reason: 'packageLockPath is required when lockHash is asserted',
    }
  }

  const probeTimeoutMs = request.probeTimeoutMs ?? DEFAULT_ADMISSION_PROBE_TIMEOUT_MS
  if (
    !Number.isInteger(probeTimeoutMs) ||
    probeTimeoutMs < MIN_PROBE_TIMEOUT_MS ||
    probeTimeoutMs > MAX_PROBE_TIMEOUT_MS
  ) {
    return {
      ok: false,
      reason: `probeTimeoutMs must be an integer in ${MIN_PROBE_TIMEOUT_MS}..=${MAX_PROBE_TIMEOUT_MS}`,
    }
  }

  const approvalTimeoutMs = request.approvalTimeoutMs ?? DEFAULT_ADMISSION_APPROVAL_TIMEOUT_MS
  if (
    !Number.isInteger(approvalTimeoutMs) ||
    approvalTimeoutMs < MIN_APPROVAL_TIMEOUT_MS ||
    approvalTimeoutMs > MAX_APPROVAL_TIMEOUT_MS
  ) {
    return {
      ok: false,
      reason:
        `approvalTimeoutMs must be an integer in ` +
        `${MIN_APPROVAL_TIMEOUT_MS}..=${MAX_APPROVAL_TIMEOUT_MS}`,
    }
  }

  const params: KernelAdmissionParams = {
    sessionId,
    ownerId,
    projectId,
    storeRoot,
    kernelId,
    kind: request.kind,
    interpreterPath,
    probeTimeoutMs,
    approvalTimeoutMs,
  }
  if (request.allowedRoot) params.allowedRoot = request.allowedRoot
  if (request.packageLockPath) params.packageLockPath = request.packageLockPath
  if (request.execHash) params.execHash = request.execHash
  if (request.lockHash) params.lockHash = request.lockHash
  return { ok: true, method: 'kernel_admission', params }
}
