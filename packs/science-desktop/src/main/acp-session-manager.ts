/**
 * ACP lifecycle: spawn → initialize → authenticate → session/new → ext method.
 *
 * That ordering is not decoration. `x.ai/science/*` handlers resolve the
 * session by id (`agent.get_session_handle`) and pin every store path inside
 * that session's cwd, so a science call before `session/new` cannot be
 * answered at all. The reference is
 * agent/crates/codegen/xai-grok-test-support/src/acp_client.rs — `initialize`
 * (~:239), `create_session` (~:289), `ext_method` (~:444).
 *
 * The engine is either ready or explicitly unavailable. There is no third
 * state, no cached last-good answer, and no mock: a desktop that cannot tell
 * "the engine said this" from "there is no engine" is what this replaces.
 *
 * Electron-free so the authority tests can drive the whole lifecycle against a
 * scripted child.
 */

import {
  AcpStdioTransport,
  type ServerRequestHandler,
  DEFAULT_MAX_FRAME_BYTES,
  DEFAULT_REQUEST_TIMEOUT_MS,
} from './acp-stdio-transport'
import {
  LumenProcessManager,
  type LumenProcessOptions,
  type ResolvedLumenBinary,
} from './lumen-process-manager'
import { resolveScienceMethod } from './science-method-registry'

/** ACP protocol version this client speaks (agent-client-protocol V1). */
export const ACP_PROTOCOL_VERSION = 1

/** Auth method the Rust agent advertises for a key-based headless client. */
export const PREFERRED_AUTH_METHOD_ID = 'xai.api_key'

export const DEFAULT_HANDSHAKE_TIMEOUT_MS = 60_000

export type EngineStatus = 'stopped' | 'starting' | 'ready' | 'unavailable'

export type EngineState = {
  status: EngineStatus
  /** Populated for `unavailable`; never empty when the status is unavailable. */
  reason?: string
  binaryPath?: string
  binaryHash?: string
  sessionId?: string
  stderrTail?: string
}

/** Thrown for every call made while the engine is not ready. */
export class LumenEngineUnavailableError extends Error {
  readonly code = 'LUMEN_ENGINE_UNAVAILABLE'
  readonly state: EngineState

  constructor(state: EngineState) {
    super(`lumen engine unavailable: ${state.reason ?? state.status}`)
    this.name = 'LumenEngineUnavailableError'
    this.state = state
  }
}

export type AcpSessionManagerOptions = {
  /** Session workspace. Science store paths must live inside it. */
  cwd: string
  process?: Partial<LumenProcessOptions>
  handshakeTimeoutMs?: number
  requestTimeoutMs?: number
  maxFrameBytes?: number
  /** Identifies this client in `initialize._meta`. */
  clientType?: string
  clientVersion?: string
  /**
   * Answers agent→client requests (permission prompts, fs reads). Omitted
   * means every one is refused with -32601 — fail-closed, and visible to the
   * agent, rather than a hang.
   */
  onServerRequest?: ServerRequestHandler
  onNotification?: (method: string, params: unknown) => void
  onStateChange?: (state: EngineState) => void
  log?: {
    info: (message: string, meta?: unknown) => void
    warn: (message: string, meta?: unknown) => void
    error: (message: string, meta?: unknown) => void
  }
}

type Ready = {
  transport: AcpStdioTransport
  sessionId: string
  binary: ResolvedLumenBinary
}

export class AcpSessionManager {
  private readonly opts: AcpSessionManagerOptions
  private processManager: LumenProcessManager | null = null
  private transport: AcpStdioTransport | null = null
  private sessionId: string | null = null
  private binary: ResolvedLumenBinary | null = null
  private status: EngineStatus = 'stopped'
  private reason: string | undefined
  private starting: Promise<Ready> | null = null

  constructor(opts: AcpSessionManagerOptions) {
    this.opts = opts
  }

  getState(): EngineState {
    const state: EngineState = { status: this.status }
    if (this.reason !== undefined) state.reason = this.reason
    if (this.binary) {
      state.binaryPath = this.binary.binaryPath
      state.binaryHash = this.binary.sha256
    }
    if (this.sessionId) state.sessionId = this.sessionId
    const tail = this.processManager?.getStderrTail()
    if (tail) state.stderrTail = tail
    return state
  }

  getBinaryHash(): string | null {
    return this.binary?.sha256 ?? null
  }

  /**
   * Bring the engine up and complete the handshake. Concurrent callers share
   * one attempt; a failed attempt leaves the manager explicitly unavailable
   * with the reason attached, and does NOT retry on its own — a crash loop
   * must be visible, not papered over.
   */
  async start(): Promise<EngineState> {
    if (this.status === 'ready') return this.getState()
    if (this.starting) {
      await this.starting.catch(() => undefined)
      return this.getState()
    }
    this.setStatus('starting')
    this.starting = this.handshake()
    try {
      await this.starting
      this.setStatus('ready')
    } catch (error: unknown) {
      this.markUnavailable(error instanceof Error ? error.message : String(error))
      throw error
    } finally {
      this.starting = null
    }
    return this.getState()
  }

  /**
   * Call one `x.ai/science/*` method.
   *
   * The name goes through the registry first: a name no engine implements is
   * rejected here without touching the wire, so its error says which call site
   * invented it instead of blaming the transport.
   */
  async callScience(
    method: unknown,
    params: Record<string, unknown> = {},
    opts: { timeoutMs?: number; signal?: AbortSignal } = {},
  ): Promise<unknown> {
    const resolved = resolveScienceMethod(method)
    const ready = await this.requireReady()
    // Every one of the 24 handlers takes `sessionId` and the param structs are
    // `deny_unknown_fields`, so filling it in when absent is required, and
    // overwriting a caller-supplied one would be wrong.
    const payload: Record<string, unknown> =
      params.sessionId === undefined
        ? { sessionId: ready.sessionId, ...params }
        : { ...params }
    return ready.transport.request(resolved.wireMethod, payload, opts)
  }

  /**
   * Shut the engine down: close the transport, then SIGTERM/SIGKILL.
   *
   * `stopped` is latched BEFORE the transport is closed. Closing it fires
   * onClose, and the child's exit fires onExit; both mean "unavailable" during
   * normal operation, but during a deliberate shutdown they are expected, and
   * letting them win would report a clean quit as an engine failure.
   */
  async stop(): Promise<void> {
    const manager = this.processManager
    const transport = this.transport
    this.status = 'stopped'
    this.reason = undefined
    this.transport = null
    this.sessionId = null
    this.processManager = null
    transport?.close('desktop shutdown')
    this.emitState()
    if (manager) await manager.stop()
  }

  // ── internals ──────────────────────────────────────────────────

  private async requireReady(): Promise<Ready> {
    if (this.status === 'ready' && this.transport && this.sessionId && this.binary) {
      if (this.transport.isOpen) {
        return { transport: this.transport, sessionId: this.sessionId, binary: this.binary }
      }
      this.markUnavailable(
        this.transport.failure?.message ?? 'transport closed without a recorded reason',
      )
    }
    if (this.starting) {
      await this.starting.catch(() => undefined)
      if (this.status === 'ready' && this.transport && this.sessionId && this.binary) {
        return { transport: this.transport, sessionId: this.sessionId, binary: this.binary }
      }
    }
    throw new LumenEngineUnavailableError(this.getState())
  }

  private async handshake(): Promise<Ready> {
    const processManager = new LumenProcessManager({
      ...(this.opts.process ?? {}),
      cwd: this.opts.cwd,
      onExit: (error) => {
        // A child that dies takes the engine with it. Recording the reason is
        // what lets every later call fail with "engine exited" instead of a
        // timeout that looks like slowness. An exit during a deliberate stop()
        // is expected and must not be reported as a failure.
        if (this.status === 'stopped') return
        this.transport?.close(error)
        this.markUnavailable(error.message)
      },
    })
    this.processManager = processManager

    const { child, binary } = processManager.start()
    this.binary = binary

    const transport = new AcpStdioTransport({
      input: child.stdout,
      output: child.stdin,
      maxFrameBytes: this.opts.maxFrameBytes ?? DEFAULT_MAX_FRAME_BYTES,
      defaultRequestTimeoutMs: this.opts.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS,
      onNotification: this.opts.onNotification,
      onServerRequest: this.opts.onServerRequest,
      // Surfaced rather than swallowed: a dropped frame is no longer fatal, so
      // it must at least be visible or an unexplained peer message becomes
      // invisible instead of merely non-fatal.
      onDropped: (reason, detail) =>
        this.opts.log?.warn?.(`acp dropped a frame: ${reason}`, detail),
      onClose: (error) => {
        if (this.status !== 'stopped') this.markUnavailable(error.message)
      },
    })
    this.transport = transport

    const timeoutMs = this.opts.handshakeTimeoutMs ?? DEFAULT_HANDSHAKE_TIMEOUT_MS
    const deadline = Date.now() + timeoutMs
    const remaining = (): number => Math.max(1, deadline - Date.now())

    const initResult = (await transport.request(
      'initialize',
      {
        protocolVersion: ACP_PROTOCOL_VERSION,
        clientCapabilities: {
          fs: { readTextFile: false, writeTextFile: false },
          terminal: false,
        },
        _meta: {
          startupHints: {
            nonInteractive: true,
            skipGitStatus: true,
            skipProjectLayout: true,
          },
          clientType: this.opts.clientType ?? 'lumen-science-desktop',
          clientVersion: this.opts.clientVersion ?? '0.0.0-dev',
        },
      },
      { timeoutMs: remaining() },
    )) as { authMethods?: Array<{ id?: unknown }>; _meta?: Record<string, unknown> } | null

    const authMethodId = pickAuthMethod(initResult)
    if (authMethodId) {
      await transport.request(
        'authenticate',
        { methodId: authMethodId, _meta: { headless: true } },
        { timeoutMs: remaining() },
      )
    }

    const sessionResult = (await transport.request(
      'session/new',
      { cwd: this.opts.cwd, mcpServers: [] },
      { timeoutMs: remaining() },
    )) as { sessionId?: unknown } | null

    const sessionId = sessionResult?.sessionId
    if (typeof sessionId !== 'string' || sessionId === '') {
      throw new Error('session/new returned no sessionId')
    }
    this.sessionId = sessionId
    this.opts.log?.info('lumen acp session established', {
      binaryPath: binary.binaryPath,
      binaryHash: binary.sha256,
      source: binary.source,
      sessionId,
    })
    return { transport, sessionId, binary }
  }

  private markUnavailable(reason: string): void {
    if (this.status === 'unavailable' && this.reason === reason) return
    this.status = 'unavailable'
    this.reason = reason
    this.opts.log?.error('lumen engine unavailable', { reason })
    this.emitState()
  }

  private setStatus(status: EngineStatus): void {
    this.status = status
    if (status !== 'unavailable') this.reason = undefined
    this.emitState()
  }

  private emitState(): void {
    this.opts.onStateChange?.(this.getState())
  }
}

/**
 * Prefer the key-based method, then whatever the agent nominates as default,
 * then the first offered. No auth method offered means the agent wants none.
 */
function pickAuthMethod(
  initResult: { authMethods?: Array<{ id?: unknown }>; _meta?: Record<string, unknown> } | null,
): string | null {
  const methods = Array.isArray(initResult?.authMethods) ? initResult.authMethods : []
  const ids = methods
    .map((m) => (typeof m?.id === 'string' ? m.id : null))
    .filter((id): id is string => id !== null)
  if (ids.length === 0) return null
  if (ids.includes(PREFERRED_AUTH_METHOD_ID)) return PREFERRED_AUTH_METHOD_ID
  const fallback = initResult?._meta?.defaultAuthMethodId
  if (typeof fallback === 'string' && ids.includes(fallback)) return fallback
  return ids[0]
}
