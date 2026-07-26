/**
 * Environment identity service (LS5-K4) — the driven adapter.
 *
 * Three verbs, in the order a caller needs them:
 *
 *   discover()          what interpreters exist on this machine
 *   identify()          what exactly one of them IS
 *   requestAdmission()  ask the engine whether it may back a kernel
 *
 * The first two answer questions about the machine and return observations.
 * The third answers nothing: it builds a request, hands it to the SessionActor
 * over ACP, and returns whatever the actor said. If no transport is wired it
 * reports that it could not ask — it does not fall back to an opinion. A
 * desktop that answers "admitted" because it could not reach the engine is the
 * exact failure this whole task exists to remove.
 *
 * Electron-free by construction (`runtimeRoot` is injected rather than read
 * from `storage-root`, which imports `electron`) so the authority scripts can
 * execute the shipped module rather than a re-implementation of it.
 *
 * NOTHING HERE SELF-DIRECTS. There is no timer, no watcher, no startup hook,
 * and no code path that provisions or launches anything. Every function runs
 * because a caller called it.
 */

import type { NotebookLanguage } from '../../shared/notebook'
import type { DiscoveredInterpreter } from '../../shared/notebook-runtime'
import {
  defaultDiscoveryDeps,
  discoverInterpreters,
  type DiscoveryDeps,
} from '../notebook/environment-discovery'
import { micromambaCacheLockKey } from '../notebook/micromamba-cache'
import { envPrefix, pkgsCache, resolveEnvName } from '../notebook/runtime-paths'
import {
  buildKernelAdmissionRequest,
  type BuildAdmissionRequest,
  type KernelAdmissionParams,
} from './admission-request'
import {
  identifyInterpreter,
  type IdentificationResult,
  type IdentifyDeps,
  type IdentifyRequest,
  type KernelKindName,
} from './interpreter-identity'

export type AcpScienceCall = (
  method: string,
  args: Record<string, unknown>,
) => Promise<unknown>

/** A candidate refused before it was probed, with the reason it was refused. */
export type UnpinnedCandidate = { candidate: string; reason: string }

export type DiscoveryReport = {
  language: NotebookLanguage
  interpreters: DiscoveredInterpreter[]
  /**
   * Candidates dropped for not being absolute paths. Reported rather than
   * discarded: a user who typed `python3` into the interpreter catalog and then
   * cannot find it in the list is owed the reason.
   */
  unpinned: UnpinnedCandidate[]
}

/**
 * Where this installation's toolchain lives. Paths and a cache key — no claim
 * that anything in them is trusted, installed, or usable.
 */
export type ToolchainLocations = {
  runtimeRoot: string
  packageCacheDir: string
  /**
   * Canonical, platform-normalised identity of the package cache. Upstream uses
   * it as a cross-process lock key; here it doubles as the stable name of the
   * cache an environment was materialised from, which belongs in a
   * reproducibility record.
   */
  packageCacheKey: string
  environmentPrefix: string
}

export type AdmissionOutcome =
  | { asked: true; params: KernelAdmissionParams; response: unknown }
  | { asked: false; reason: string }

export type EnvironmentService = {
  discover: (language: NotebookLanguage) => Promise<DiscoveryReport>
  identify: (request: IdentifyRequest) => Promise<IdentificationResult>
  /**
   * Identify and then ask the engine, in one driven step.
   *
   * Returns `asked: false` when there is nothing to ask about (the interpreter
   * could not be identified) or nobody to ask (no transport). Neither is a
   * verdict; see the module note.
   */
  requestAdmission: (request: AdmissionAsk) => Promise<AdmissionOutcome>
  toolchain: (language: NotebookLanguage, environment?: string) => ToolchainLocations
}

export type AdmissionAsk = {
  sessionId: string
  storeRoot: string
  kernelId: string
  kind: KernelKindName
  interpreterPath: string
  packageLockPath?: string
  allowedRoot?: string
  probeTimeoutMs?: number
}

export type EnvironmentServiceOptions = {
  /** `<storageRoot>/runtime`. Injected: resolving it needs Electron. */
  runtimeRoot: string
  /** Absent means "cannot ask the engine", never "answer locally". */
  acpCall?: AcpScienceCall
  /** Interpreter paths the user added by hand, per language. */
  manualPaths?: (language: NotebookLanguage) => string[]
  /** Test seams. */
  discoveryDeps?: DiscoveryDeps
  identifyDeps?: IdentifyDeps
}

export const createEnvironmentService = (
  options: EnvironmentServiceOptions,
): EnvironmentService => {
  const { runtimeRoot } = options

  return {
    async discover(language) {
      const unpinned: UnpinnedCandidate[] = []
      const record = (candidate: string, reason: string): void => {
        unpinned.push({ candidate, reason })
      }
      const deps: DiscoveryDeps = options.discoveryDeps
        ? { ...options.discoveryDeps, onUnpinnedCandidate: record }
        : defaultDiscoveryDeps(runtimeRoot, options.manualPaths, {
            onUnpinnedCandidate: record,
          })
      const interpreters = await discoverInterpreters(language, deps)
      return { language, interpreters, unpinned }
    },

    identify(request) {
      return identifyInterpreter(request, options.identifyDeps ?? {})
    },

    async requestAdmission(request) {
      const identification = await identifyInterpreter(
        {
          kind: request.kind,
          interpreterPath: request.interpreterPath,
          packageLockPath: request.packageLockPath,
          probeTimeoutMs: request.probeTimeoutMs,
        },
        options.identifyDeps ?? {},
      )
      if (!identification.identified) {
        return {
          asked: false,
          reason:
            `interpreter could not be identified (${identification.failure.code}): ` +
            identification.failure.detail,
        }
      }

      const built = buildKernelAdmissionRequest({
        sessionId: request.sessionId,
        storeRoot: request.storeRoot,
        kernelId: request.kernelId,
        identity: identification.identity,
        allowedRoot: request.allowedRoot,
        probeTimeoutMs: request.probeTimeoutMs,
      } satisfies BuildAdmissionRequest)
      if (!built.ok) return { asked: false, reason: built.reason }

      if (!options.acpCall) {
        return {
          asked: false,
          reason:
            'no science engine transport wired: kernel admission is decided by the Lumen ' +
            'SessionActor and this desktop does not decide it locally',
        }
      }
      const response = await options.acpCall(built.method, built.params)
      // Returned verbatim. Interpreting the engine's verdict here — mapping it
      // to a boolean, defaulting it when absent — would recreate a second
      // authority one convenience function at a time.
      return { asked: true, params: built.params, response }
    },

    toolchain(language, environment) {
      const packageCacheDir = pkgsCache(runtimeRoot)
      return {
        runtimeRoot,
        packageCacheDir,
        packageCacheKey: micromambaCacheLockKey(packageCacheDir),
        environmentPrefix: envPrefix(runtimeRoot, resolveEnvName(language, environment)),
      }
    },
  }
}
