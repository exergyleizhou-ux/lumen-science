/**
 * Kernel admission request construction (LS5-K4) — pure, no Electron, no spawn.
 *
 * Turns an observed [`InterpreterIdentity`] into the parameters of the engine's
 * `x.ai/science/kernel_admission` method. That is the whole of this module's
 * job: it asks. The answer — admitted, rejected, and why — is produced by the
 * Rust SessionActor, which re-probes the interpreter itself and compares what
 * it finds against the digests asserted here.
 *
 * The asymmetry is deliberate and is the point of the adapter split. The
 * desktop is well placed to *find* an interpreter: it can enumerate PATH, conda
 * roots, framework installs and the app's own envs, which the engine has no
 * business doing. The desktop is not the right place to *bless* one. So it
 * sends facts and a claim, and receives a verdict.
 *
 * The digests travel as `execHash` / `lockHash`, which the engine treats as
 * caller assertions to be VERIFIED against its own probe and rejected on
 * mismatch — never copied into the admission record. Echoing a caller's hash
 * back into a record as though it had been checked was precisely the defect
 * this work exists to close, so nothing here should read as if the desktop's
 * measurement is the authoritative one. It is a second opinion that must agree.
 */

import type { InterpreterIdentity, KernelKindName } from './interpreter-identity'

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
  storeRoot: string
  kernelId: string
  kind: KernelKindName
  interpreterPath: string
  allowedRoot?: string
  execHash?: string
  packageLockPath?: string
  lockHash?: string
  probeTimeoutMs: number
}

export type BuildAdmissionRequest = {
  sessionId: string
  storeRoot: string
  /** Stable name for this kernel within the project. */
  kernelId: string
  identity: InterpreterIdentity
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
}

export type BuildAdmissionResult =
  | { ok: true; method: 'kernel_admission'; params: KernelAdmissionParams }
  | { ok: false; reason: string }

/** The engine rejects anything outside this range before it probes. */
export const MIN_PROBE_TIMEOUT_MS = 1
export const MAX_PROBE_TIMEOUT_MS = 120_000
export const DEFAULT_ADMISSION_PROBE_TIMEOUT_MS = 10_000

const SHA256_HEX = /^[0-9a-f]{64}$/

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
  const { identity } = request
  const sessionId = request.sessionId?.trim()
  const storeRoot = request.storeRoot?.trim()
  const kernelId = request.kernelId?.trim()

  if (!sessionId) return { ok: false, reason: 'sessionId is required' }
  if (!storeRoot) return { ok: false, reason: 'storeRoot is required' }
  if (!kernelId) {
    return { ok: false, reason: 'kernelId is required: an admission record needs a name' }
  }
  if (!identity?.interpreterPath) {
    return { ok: false, reason: 'identity.interpreterPath is required' }
  }
  // Re-asserted rather than assumed. An identity always carries a resolved
  // absolute path, but this function is reachable from IPC with a
  // caller-constructed object, and a relative path here would be probed by the
  // engine against ITS working directory — a different file, silently.
  if (!identity.interpreterPath.startsWith('/') && !/^([A-Za-z]:[\\/]|\\\\)/.test(identity.interpreterPath)) {
    return {
      ok: false,
      reason: `interpreterPath '${identity.interpreterPath}' is not absolute`,
    }
  }
  if (!SHA256_HEX.test(identity.executableSha256)) {
    return {
      ok: false,
      reason: `executableSha256 '${identity.executableSha256}' is not a lowercase sha256 hex digest`,
    }
  }
  if (identity.packageLock !== null) {
    if (!SHA256_HEX.test(identity.packageLock.sha256)) {
      return {
        ok: false,
        reason: `packageLock.sha256 '${identity.packageLock.sha256}' is not a lowercase sha256 hex digest`,
      }
    }
    if (!identity.packageLock.path) {
      return { ok: false, reason: 'packageLock.path is required when a lock digest is asserted' }
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

  const params: KernelAdmissionParams = {
    sessionId,
    storeRoot,
    kernelId,
    kind: identity.kind,
    interpreterPath: identity.interpreterPath,
    execHash: identity.executableSha256,
    probeTimeoutMs,
  }
  if (request.allowedRoot) params.allowedRoot = request.allowedRoot
  if (identity.packageLock !== null) {
    params.packageLockPath = identity.packageLock.path
    params.lockHash = identity.packageLock.sha256
  }
  return { ok: true, method: 'kernel_admission', params }
}
