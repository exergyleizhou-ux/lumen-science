/**
 * Spawn and supervise the Lumen engine child process.
 *
 * The previous implementation spawned `lumen-science serve --interface
 * loopback --port 17000`. No such subcommand exists: the Go standalone CLI
 * (packs/science/standalone/cmd/science/main.go) switches on
 * version|doctor|gates|brief|seq|artifact|pipeline|project|claim|workflow|help,
 * and the Rust binary's science surface is reachable only through
 * `lumen agent stdio`. So the child died immediately, every call got
 * ECONNREFUSED, and the failure was logged and swallowed at startup.
 *
 * What this manager guarantees:
 *
 *   - the binary is resolved explicitly (LUMEN_BINARY → bundled → PATH) and
 *     its SHA-256 is computed before it runs, so what the desktop attests to
 *     is what it executed;
 *   - an expected hash, when configured, is enforced BEFORE spawn;
 *   - stdout is reserved for protocol and is never logged or parsed here;
 *     stderr is captured separately into a bounded tail for diagnostics;
 *   - exit is observed, so callers can surface an explicit unavailable state
 *     rather than a stale or invented answer;
 *   - shutdown is SIGTERM then SIGKILL, and the escalation timer is unref'd
 *     and cleared, so quitting is never held open by this module.
 *
 * Electron-free: the bundled-resources directory is injected, so the authority
 * tests can drive a fake binary with no Electron and no packaging.
 */

import { type ChildProcessWithoutNullStreams, spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'

/** Subcommand that speaks ACP over stdio. Not `serve`, which does not exist. */
export const LUMEN_AGENT_STDIO_ARGS = ['agent', 'stdio'] as const

/** Default grace period between SIGTERM and SIGKILL. */
export const DEFAULT_SHUTDOWN_GRACE_MS = 5_000

/** Bytes of stderr retained for diagnostics. */
export const DEFAULT_STDERR_TAIL_BYTES = 64 * 1024

const SHA256_HEX = /^[a-f0-9]{64}$/

export type BinarySource = 'env' | 'bundled' | 'path'

export type ResolvedLumenBinary = {
  binaryPath: string
  source: BinarySource
  sha256: string
}

export class LumenBinaryNotFoundError extends Error {
  readonly code = 'LUMEN_BINARY_NOT_FOUND'

  constructor(detail: string) {
    super(`lumen binary not found: ${detail}`)
    this.name = 'LumenBinaryNotFoundError'
  }
}

export class LumenBinaryHashMismatchError extends Error {
  readonly code = 'LUMEN_BINARY_HASH_MISMATCH'
  readonly expected: string
  readonly actual: string

  constructor(binaryPath: string, expected: string, actual: string) {
    super(
      `lumen binary hash mismatch for ${binaryPath}: expected ${expected}, got ${actual} — refusing to spawn`,
    )
    this.name = 'LumenBinaryHashMismatchError'
    this.expected = expected
    this.actual = actual
  }
}

export class LumenProcessExitedError extends Error {
  readonly code = 'LUMEN_PROCESS_EXITED'
  readonly exitCode: number | null
  readonly signal: NodeJS.Signals | null

  constructor(exitCode: number | null, signal: NodeJS.Signals | null, stderrTail: string) {
    const how = signal ? `signal ${signal}` : `code ${exitCode}`
    super(
      `lumen engine exited with ${how}${stderrTail ? `\nstderr tail:\n${stderrTail}` : ''}`,
    )
    this.name = 'LumenProcessExitedError'
    this.exitCode = exitCode
    this.signal = signal
  }
}

/** SHA-256 hex of a file, streamed so a 400MB debug binary is not buffered. */
export function sha256OfFile(filePath: string): string {
  const hash = createHash('sha256')
  const fd = fs.openSync(filePath, 'r')
  try {
    const chunk = Buffer.alloc(1024 * 1024)
    let read: number
    while ((read = fs.readSync(fd, chunk, 0, chunk.length, null)) > 0) {
      hash.update(chunk.subarray(0, read))
    }
  } finally {
    fs.closeSync(fd)
  }
  return hash.digest('hex')
}

function isFile(candidate: string): boolean {
  try {
    return fs.statSync(candidate).isFile()
  } catch {
    return false
  }
}

export type ResolveOptions = {
  env?: NodeJS.ProcessEnv
  /** Electron's `process.resourcesPath` in production; injected for tests. */
  resourcesPath?: string
  /** Executable base name. The Rust crate ships as `lumen`. */
  binaryName?: string
  platform?: NodeJS.Platform
}

/**
 * Resolve the engine binary: LUMEN_BINARY, then bundled resources, then PATH.
 *
 * PATH is walked here rather than shelled out to `which`, so the answer is the
 * same on every platform and a test can control it with one env var.
 */
export function resolveLumenBinary(opts: ResolveOptions = {}): ResolvedLumenBinary {
  const env = opts.env ?? process.env
  const platform = opts.platform ?? process.platform
  const ext = platform === 'win32' ? '.exe' : ''
  const name = opts.binaryName ?? 'lumen'
  const tried: string[] = []

  const fromEnv = env.LUMEN_BINARY?.trim()
  if (fromEnv) {
    const resolved = path.resolve(fromEnv)
    if (isFile(resolved)) {
      return { binaryPath: resolved, source: 'env', sha256: sha256OfFile(resolved) }
    }
    // An explicit override that does not exist is an error worth naming: it is
    // almost always a stale path, and silently falling through to PATH would
    // run a DIFFERENT binary than the operator asked for.
    throw new LumenBinaryNotFoundError(`LUMEN_BINARY=${fromEnv} is not a file`)
  }

  if (opts.resourcesPath) {
    const bundled = path.join(opts.resourcesPath, 'bin', `${name}${ext}`)
    tried.push(bundled)
    if (isFile(bundled)) {
      return { binaryPath: bundled, source: 'bundled', sha256: sha256OfFile(bundled) }
    }
  }

  const pathEntries = (env.PATH ?? '').split(path.delimiter).filter(Boolean)
  for (const entry of pathEntries) {
    const candidate = path.join(entry, `${name}${ext}`)
    tried.push(candidate)
    if (isFile(candidate)) {
      return { binaryPath: candidate, source: 'path', sha256: sha256OfFile(candidate) }
    }
  }

  throw new LumenBinaryNotFoundError(
    `set LUMEN_BINARY, or install '${name}${ext}'. Tried: ${tried.length > 0 ? tried.join(', ') : '(no candidates)'}`,
  )
}

export type LumenProcessOptions = ResolveOptions & {
  /** Working directory for the agent session. */
  cwd: string
  /** Overridden only by tests; production always speaks `agent stdio`. */
  args?: readonly string[]
  /** Extra environment for the child, merged over the inherited env. */
  childEnv?: NodeJS.ProcessEnv
  /**
   * Required SHA-256. When set (or `LUMEN_BINARY_SHA256` is), a mismatch
   * refuses the spawn — an unpinned engine is exactly what the desktop's
   * attestation claims not to be.
   */
  expectedSha256?: string
  shutdownGraceMs?: number
  stderrTailBytes?: number
  onExit?: (error: LumenProcessExitedError) => void
  onStderr?: (chunk: string) => void
}

export type LumenProcessHandle = {
  child: ChildProcessWithoutNullStreams
  binary: ResolvedLumenBinary
}

export class LumenProcessManager {
  private readonly opts: LumenProcessOptions
  private child: ChildProcessWithoutNullStreams | null = null
  private binary: ResolvedLumenBinary | null = null
  private stderrTail = ''
  private exitError: LumenProcessExitedError | null = null
  private killTimer: ReturnType<typeof setTimeout> | null = null
  private exitPromise: Promise<void> | null = null

  constructor(opts: LumenProcessOptions) {
    this.opts = opts
  }

  get binaryInfo(): ResolvedLumenBinary | null {
    return this.binary
  }

  get running(): boolean {
    return this.child !== null && this.exitError === null
  }

  /** Non-null once the child has exited; the reason callers surface. */
  get exited(): LumenProcessExitedError | null {
    return this.exitError
  }

  /** Bounded tail of the child's stderr. Never parsed as protocol. */
  getStderrTail(): string {
    return this.stderrTail
  }

  /**
   * Resolve + hash-check + spawn. Throws before spawning if the binary is
   * missing or its hash does not match the pin.
   */
  start(): LumenProcessHandle {
    if (this.child) {
      throw new Error('lumen process already started')
    }
    const binary = resolveLumenBinary(this.opts)
    const expected = (
      this.opts.expectedSha256 ?? (this.opts.env ?? process.env).LUMEN_BINARY_SHA256
    )?.trim()
    if (expected) {
      if (!SHA256_HEX.test(expected)) {
        throw new LumenBinaryHashMismatchError(
          binary.binaryPath,
          expected,
          binary.sha256,
        )
      }
      if (expected !== binary.sha256) {
        throw new LumenBinaryHashMismatchError(
          binary.binaryPath,
          expected,
          binary.sha256,
        )
      }
    }

    const args = [...(this.opts.args ?? LUMEN_AGENT_STDIO_ARGS)]
    const child = spawn(binary.binaryPath, args, {
      cwd: this.opts.cwd,
      // Explicit pipes on all three: stdout is protocol, stderr is diagnostics,
      // and they must never be merged.
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, ...(this.opts.childEnv ?? {}) },
    }) as ChildProcessWithoutNullStreams

    this.child = child
    this.binary = binary
    this.exitError = null
    this.stderrTail = ''

    const tailLimit = this.opts.stderrTailBytes ?? DEFAULT_STDERR_TAIL_BYTES
    child.stderr.setEncoding('utf8')
    child.stderr.on('data', (chunk: string) => {
      this.stderrTail = `${this.stderrTail}${chunk}`.slice(-tailLimit)
      this.opts.onStderr?.(chunk)
    })

    this.exitPromise = new Promise<void>((resolve) => {
      const finish = (code: number | null, signal: NodeJS.Signals | null): void => {
        if (this.exitError) return
        this.clearKillTimer()
        this.exitError = new LumenProcessExitedError(code, signal, this.stderrTail)
        this.opts.onExit?.(this.exitError)
        resolve()
      }
      child.on('exit', finish)
      child.on('error', (error: Error) => {
        this.stderrTail = `${this.stderrTail}\nspawn error: ${error.message}`.slice(
          -tailLimit,
        )
        finish(null, null)
      })
    })

    return { child, binary }
  }

  /**
   * SIGTERM, then SIGKILL after the grace period, then await the exit.
   * The escalation timer is unref'd so it can never hold app quit open, and is
   * cleared the moment the child goes away.
   */
  async stop(): Promise<void> {
    const child = this.child
    if (!child) return
    if (this.exitError) {
      this.clearKillTimer()
      return
    }

    try {
      child.kill('SIGTERM')
    } catch {
      // Already gone; the exit handler still resolves exitPromise.
    }

    const grace = this.opts.shutdownGraceMs ?? DEFAULT_SHUTDOWN_GRACE_MS
    this.clearKillTimer()
    this.killTimer = setTimeout(() => {
      try {
        child.kill('SIGKILL')
      } catch {
        // Nothing left to kill.
      }
    }, grace)
    this.killTimer.unref?.()

    try {
      await this.exitPromise
    } finally {
      this.clearKillTimer()
    }
  }

  private clearKillTimer(): void {
    if (this.killTimer) {
      clearTimeout(this.killTimer)
      this.killTimer = null
    }
  }
}
