/**
 * Interpreter identification (LS5-K4) — facts, never a permission.
 *
 * ## Why this exists
 *
 * `check_kernel_admission` used to fabricate its answer: `exact_version:
 * "unknown"`, a hardcoded `admitted_at`, the caller's own hashes echoed back,
 * and `Admitted` unconditionally. The Rust side now probes for real
 * (agent/crates/codegen/xai-grok-science/src/workflow/admission.rs). It can
 * only do that if something names an interpreter worth probing, and a
 * reproducibility record is only as good as the identity that went into it.
 *
 * This module produces that identity from the machine: the resolved absolute
 * path, the sha256 of the executable's own bytes, the exact version string the
 * interpreter printed, the host os/arch, and the digest of the lock file that
 * pins its package set.
 *
 * ## What it is NOT
 *
 * It is not an authority and it must never become one.
 *
 *  * It returns no `admitted` field, and no boolean that a caller could read as
 *    one. A failure here is `identified: false` — "I could not observe this" —
 *    and never "this may not run". The two are different claims: an interpreter
 *    this desktop cannot read may still be admissible to an engine running as
 *    another user, and an interpreter this desktop identified perfectly may
 *    still be refused by policy.
 *  * It starts no kernel. The only process it spawns is the interpreter with a
 *    FIXED version argv, which exits immediately. The argv is deliberately not
 *    caller-supplied — the engine's request type allows an override, this
 *    adapter does not, because an adapter that accepts arbitrary argv for a
 *    binary it is about to execute is an arbitrary-execution surface wearing a
 *    version probe's name.
 *  * It does not decide which interpreter to use. It identifies the one it was
 *    handed.
 *
 * The engine re-derives every one of these facts itself before admitting
 * anything; what this module produces is a *claim*, which the engine verifies
 * and rejects on mismatch. That redundancy is the design: two independent
 * observations that must agree, rather than one observation trusted twice.
 */

import { execFile } from 'node:child_process'
import { constants as fsConstants } from 'node:fs'
import { access, lstat, realpath, stat } from 'node:fs/promises'
import { isAbsolute } from 'node:path'

import { sha256File } from '../notebook/bundle-manifest'
import { isPinnedInterpreterPath } from '../notebook/environment-discovery'

/** Kernel kinds the engine dispatches (`KernelKind` in workflow/kernel.rs). */
export type KernelKindName = 'python' | 'r' | 'julia'

/**
 * The version argv per kind. A constant, not a parameter.
 *
 * `-VV` rather than `-V` for Python because the short form prints only
 * `Python 3.13.1`, which two different builds of the same release share. The
 * long form carries the build date and compiler, so the string distinguishes
 * binaries that the version number alone does not.
 */
export const VERSION_PROBE_ARGV: Readonly<Record<KernelKindName, readonly string[]>> = {
  python: ['-VV'],
  r: ['--version'],
  julia: ['--version'],
}

/** Wall-clock budget for the version probe. Matches the engine's default. */
export const DEFAULT_PROBE_TIMEOUT_MS = 10_000

/** Longest version string retained, so a hostile binary cannot flood a record. */
export const MAX_VERSION_CHARS = 256

/** Largest probe output read before truncation. */
export const MAX_PROBE_OUTPUT_BYTES = 64 * 1024

/**
 * Everything observed about one interpreter. Every field is a measurement.
 *
 * `observedAt` is the only clock reading, and it records when the observation
 * happened — unlike the fabricated `admitted_at` this work exists to replace,
 * it is not a claim that anything was approved at that moment.
 */
export type InterpreterIdentity = {
  kind: KernelKindName
  /** Exactly what the caller asked about, before symlink resolution. */
  requestedPath: string
  /** The same file with symlinks followed: the identity that was hashed. */
  interpreterPath: string
  /** Lowercase hex sha256 of the executable's own bytes. */
  executableSha256: string
  executableSizeBytes: number
  /** What the interpreter printed, normalised to one line and capped. */
  exactVersion: string
  /** The argv used to obtain it, recorded so the reading is reproducible. */
  versionProbeArgv: readonly string[]
  /** `process.platform` / `process.arch` of the host that observed this. */
  os: string
  architecture: string
  /** The lock file pinning this interpreter's package set, when one was named. */
  packageLock: { path: string; sha256: string } | null
  observedAt: string
}

/**
 * Why an identity could not be produced.
 *
 * The codes are the engine's `RejectionReason::code()` spellings on purpose. An
 * interpreter this adapter could not read and one the engine refuses report the
 * same code, so a log line means one thing across the two languages. They stay
 * *descriptions of an observation*: `interpreter_not_executable` says the mode
 * bits lacked +x, not that permission was withheld.
 */
export type IdentificationFailureCode =
  | 'actor_probe_required'
  | 'interpreter_path_not_absolute'
  | 'interpreter_not_found'
  | 'interpreter_not_a_file'
  | 'interpreter_not_executable'
  | 'interpreter_unreadable'
  | 'package_lock_not_a_file'
  | 'version_probe_spawn_failed'
  | 'version_probe_timed_out'
  | 'version_probe_exit_non_zero'
  | 'version_probe_empty_output'

export type IdentificationFailure = {
  code: IdentificationFailureCode
  detail: string
  /** The path under examination, echoed so a log line is self-contained. */
  path: string
}

export type IdentificationResult =
  | { identified: true; identity: InterpreterIdentity }
  | { identified: false; failure: IdentificationFailure }

export type IdentifyRequest = {
  kind: KernelKindName
  /** Must be absolute. A bare name would be resolved through PATH. */
  interpreterPath: string
  /** Lock file whose digest pins the package set (requirements.lock, renv.lock…). */
  packageLockPath?: string
  probeTimeoutMs?: number
}

/** Injected so the whole module is testable without a machine. */
export type IdentifyDeps = {
  runVersionProbe?: (
    file: string,
    args: readonly string[],
    timeoutMs: number,
  ) => Promise<VersionProbeOutcome>
  hashFile?: (path: string) => Promise<string>
  realpath?: (path: string) => Promise<string>
  now?: () => Date
  platform?: NodeJS.Platform
  arch?: string
}

export type VersionProbeOutcome =
  | { outcome: 'ok'; stdout: string; stderr: string }
  | { outcome: 'spawn-failed'; detail: string }
  | { outcome: 'timed-out'; timeoutMs: number }
  | { outcome: 'exit-non-zero'; exitCode: number | null; output: string }

const fail = (
  code: IdentificationFailureCode,
  path: string,
  detail: string,
): IdentificationResult => ({ identified: false, failure: { code, detail, path } })

const errorDetail = (error: unknown): string => {
  if (!(error instanceof Error)) return String(error)
  const code = (error as NodeJS.ErrnoException).code
  return code ? `${code}: ${error.message}` : error.message
}

/**
 * Collapse probe output to a single stable line.
 *
 * `python -VV` prints across lines on some builds; a version recorded with an
 * embedded newline is a different string on Windows and POSIX for the same
 * binary, which would make an otherwise identical environment compare unequal.
 */
export const normaliseVersion = (raw: string): string =>
  raw
    .slice(0, MAX_PROBE_OUTPUT_BYTES)
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, MAX_VERSION_CHARS)

const defaultRunVersionProbe = (
  file: string,
  args: readonly string[],
  timeoutMs: number,
): Promise<VersionProbeOutcome> =>
  new Promise((resolve) => {
    // execFile, never a shell: argv is passed directly, so a path containing
    // spaces or shell metacharacters cannot become a command.
    execFile(
      file,
      [...args],
      {
        timeout: timeoutMs,
        windowsHide: true,
        maxBuffer: MAX_PROBE_OUTPUT_BYTES,
        // An interpreter must not be able to read the desktop's environment
        // just by being asked its version.
        env: { PATH: '', LANG: 'C', LC_ALL: 'C' },
      },
      (error, stdout, stderr) => {
        if (error === null) {
          resolve({ outcome: 'ok', stdout: String(stdout), stderr: String(stderr) })
          return
        }
        const err = error as NodeJS.ErrnoException & { killed?: boolean; code?: number | string }
        if (err.killed === true) {
          resolve({ outcome: 'timed-out', timeoutMs })
          return
        }
        if (typeof err.code === 'string') {
          resolve({ outcome: 'spawn-failed', detail: errorDetail(error) })
          return
        }
        resolve({
          outcome: 'exit-non-zero',
          exitCode: typeof err.code === 'number' ? err.code : null,
          output: `${String(stdout)}${String(stderr)}`,
        })
      },
    )
  })

/**
 * Observe one interpreter and report what is there.
 *
 * Order matters: cheap structural checks first, then the hash, then the spawn.
 * A path that is not a regular executable file is never handed to `execFile`.
 */
export const identifyInterpreter = async (
  request: IdentifyRequest,
  deps: IdentifyDeps = {},
): Promise<IdentificationResult> => {
  const platform = deps.platform ?? process.platform
  const requested = request.interpreterPath
  const timeoutMs = request.probeTimeoutMs ?? DEFAULT_PROBE_TIMEOUT_MS

  // 1. Pinned path. This is the first check because everything after it would
  //    otherwise be measuring whichever binary this process's PATH happens to
  //    name — a fact about the caller's environment, not about a kernel.
  if (typeof requested !== 'string' || !isPinnedInterpreterPath(requested, platform)) {
    return fail(
      'interpreter_path_not_absolute',
      String(requested),
      `interpreter path '${String(requested)}' is not absolute; a PATH-relative interpreter ` +
        'is not a pinned identity and cannot be reproduced',
    )
  }

  // 2. It is a regular file. lstat first, so a symlink to a directory is not
  //    mistaken for a file, then realpath for the identity that gets hashed.
  let resolved: string
  let fileStat: Awaited<ReturnType<typeof stat>>
  try {
    const link = await lstat(requested)
    if (!link.isFile() && !link.isSymbolicLink()) {
      return fail(
        'interpreter_not_a_file',
        requested,
        `interpreter '${requested}' is a ${describeType(link)}, not a regular file`,
      )
    }
    resolved = await (deps.realpath ?? realpath)(requested)
    fileStat = await stat(resolved)
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code
    if (code === 'ENOENT') {
      return fail('interpreter_not_found', requested, `interpreter '${requested}' does not exist`)
    }
    return fail(
      'interpreter_unreadable',
      requested,
      `interpreter '${requested}' could not be inspected (${errorDetail(error)})`,
    )
  }
  if (!fileStat.isFile()) {
    return fail(
      'interpreter_not_a_file',
      requested,
      `interpreter '${requested}' resolves to ${resolved}, which is a ${describeType(fileStat)}`,
    )
  }
  if (!isAbsolute(resolved)) {
    return fail(
      'interpreter_path_not_absolute',
      requested,
      `interpreter '${requested}' resolved to a relative path '${resolved}'`,
    )
  }

  // 3. Executable. Windows has no execute bit — access(X_OK) there reports the
  //    read bit, so asserting it would be theatre; the spawn below is the real
  //    test on that platform.
  if (platform !== 'win32') {
    try {
      await access(resolved, fsConstants.X_OK)
    } catch {
      return fail(
        'interpreter_not_executable',
        requested,
        `interpreter '${resolved}' has no execute permission for this process`,
      )
    }
  }

  // 4. The bytes. Hash the resolved file, not the requested one: a symlink has
  //    no contents of its own, and hashing through it would record the link's
  //    identity as the interpreter's.
  let executableSha256: string
  try {
    executableSha256 = await (deps.hashFile ?? sha256File)(resolved)
  } catch (error) {
    return fail(
      'interpreter_unreadable',
      requested,
      `interpreter '${resolved}' could not be read for hashing (${errorDetail(error)})`,
    )
  }

  // 5. The lock file, when one was named. A named lock that is not a file is a
  //    failure rather than a silent `null`: the caller asserted a pin, and
  //    reporting "no lock" would quietly downgrade that assertion.
  let packageLock: InterpreterIdentity['packageLock'] = null
  if (request.packageLockPath !== undefined) {
    const lockPath = request.packageLockPath
    try {
      const lockStat = await stat(lockPath)
      if (!lockStat.isFile()) {
        return fail(
          'package_lock_not_a_file',
          lockPath,
          `package lock '${lockPath}' is a ${describeType(lockStat)}, not a regular file`,
        )
      }
      packageLock = { path: lockPath, sha256: await (deps.hashFile ?? sha256File)(lockPath) }
    } catch (error) {
      return fail(
        'package_lock_not_a_file',
        lockPath,
        `package lock '${lockPath}' could not be read (${errorDetail(error)})`,
      )
    }
  }

  // 6. The version, from the interpreter's own mouth.
  const argv = VERSION_PROBE_ARGV[request.kind]
  const probe = await (deps.runVersionProbe ?? defaultRunVersionProbe)(resolved, argv, timeoutMs)
  if (probe.outcome === 'spawn-failed') {
    return fail(
      'version_probe_spawn_failed',
      resolved,
      `version probe could not be started: ${probe.detail}`,
    )
  }
  if (probe.outcome === 'timed-out') {
    return fail(
      'version_probe_timed_out',
      resolved,
      `version probe did not finish within ${probe.timeoutMs}ms`,
    )
  }
  if (probe.outcome === 'exit-non-zero') {
    return fail(
      'version_probe_exit_non_zero',
      resolved,
      `version probe exited ${probe.exitCode ?? 'by signal'}: ${normaliseVersion(probe.output)}`,
    )
  }
  // R prints its banner on stdout, some Python builds print to stderr; take
  // whichever produced text rather than guessing per platform.
  const exactVersion = normaliseVersion(`${probe.stdout} ${probe.stderr}`)
  if (exactVersion.length === 0) {
    return fail(
      'version_probe_empty_output',
      resolved,
      `version probe produced no output for '${resolved}'`,
    )
  }

  return {
    identified: true,
    identity: {
      kind: request.kind,
      requestedPath: requested,
      interpreterPath: resolved,
      executableSha256,
      executableSizeBytes: fileStat.size,
      exactVersion,
      versionProbeArgv: argv,
      os: platform,
      architecture: deps.arch ?? process.arch,
      packageLock,
      observedAt: (deps.now?.() ?? new Date()).toISOString(),
    },
  }
}

const describeType = (s: { isDirectory: () => boolean; isSymbolicLink: () => boolean }): string =>
  s.isDirectory() ? 'directory' : s.isSymbolicLink() ? 'symbolic link' : 'special file'
