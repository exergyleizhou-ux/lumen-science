/**
 * JSON-RPC 2.0 over newline-delimited JSON on a child process's stdio.
 *
 * This is the framing layer the old `lumen-acp-bridge.ts` claimed to have and
 * did not: it spawned a child with piped stdio, then talked to a loopback HTTP
 * port instead, never writing a byte to stdin. ACP is a stdio protocol —
 * requests go out on the child's stdin, responses come back on its stdout, one
 * JSON object per line.
 *
 * Rules this layer enforces, all of them fail-closed:
 *
 *   - stdout carries protocol ONLY. A line that is not JSON is a protocol
 *     violation, not something to skip: skipping it means a child that has
 *     started printing to the wrong stream keeps looking healthy.
 *   - frames are bounded. An unterminated or oversized line kills the
 *     transport rather than growing a buffer until the process dies.
 *   - every request is bounded in time and settles exactly once.
 *   - a response for an id that was never issued is a violation; a response
 *     for an id that already settled (timed out, cancelled) is dropped,
 *     because that one is expected and harmless.
 *   - agent→client requests are answered. A client that ignores them lets the
 *     agent block forever; without a responder we answer -32601, which is a
 *     refusal the agent can act on.
 *
 * Electron-free: it takes streams, so the authority tests drive it over a pair
 * of in-process pipes with no child and no Electron.
 */

/** Default ceiling for one NDJSON frame. A science response is far smaller. */
export const DEFAULT_MAX_FRAME_BYTES = 8 * 1024 * 1024

/** Default per-request deadline. */
export const DEFAULT_REQUEST_TIMEOUT_MS = 30_000

export type JsonRpcErrorBody = {
  code: number
  message: string
  data?: unknown
}

/** A JSON-RPC error returned by the peer. Carries the wire code. */
export class AcpRemoteError extends Error {
  readonly code = 'LUMEN_ACP_REMOTE_ERROR'
  readonly rpcCode: number
  readonly data: unknown

  constructor(method: string, body: JsonRpcErrorBody) {
    // `data` carries the ACTUAL reason. JSON-RPC's `message` for -32603 is the
    // generic "Internal error", so a message-only error told the user nothing:
    // "project_create failed: [-32603] Internal error" while the engine had
    // said "science run … finished TimedOut" in the field we discarded.
    const detail =
      typeof body.data === 'string'
        ? body.data
        : body.data === undefined
          ? ''
          : JSON.stringify(body.data)
    super(
      detail
        ? `${method} failed: [${body.code}] ${body.message}: ${detail}`
        : `${method} failed: [${body.code}] ${body.message}`,
    )
    this.name = 'AcpRemoteError'
    this.rpcCode = body.code
    this.data = body.data
  }
}

/** The peer broke the framing contract. The transport is dead after this. */
export class AcpProtocolViolationError extends Error {
  readonly code = 'LUMEN_ACP_PROTOCOL_VIOLATION'

  constructor(detail: string) {
    super(`ACP protocol violation: ${detail}`)
    this.name = 'AcpProtocolViolationError'
  }
}

/** The transport was closed (child exited, shutdown, or an earlier violation). */
export class AcpTransportClosedError extends Error {
  readonly code = 'LUMEN_ACP_TRANSPORT_CLOSED'

  constructor(reason: string) {
    super(`ACP transport closed: ${reason}`)
    this.name = 'AcpTransportClosedError'
  }
}

/** A request passed its deadline without a response. */
export class AcpRequestTimeoutError extends Error {
  readonly code = 'LUMEN_ACP_REQUEST_TIMEOUT'

  constructor(method: string, timeoutMs: number) {
    super(`${method} timed out after ${timeoutMs}ms`)
    this.name = 'AcpRequestTimeoutError'
  }
}

/** A request was cancelled by its caller. */
export class AcpRequestCancelledError extends Error {
  readonly code = 'LUMEN_ACP_REQUEST_CANCELLED'

  constructor(method: string) {
    super(`${method} cancelled`)
    this.name = 'AcpRequestCancelledError'
  }
}

export type ServerRequestHandler = (
  method: string,
  params: unknown,
) => Promise<unknown>

export type AcpStdioTransportOptions = {
  /** Child stdout — protocol in. */
  input: NodeJS.ReadableStream
  /** Child stdin — protocol out. */
  output: NodeJS.WritableStream
  maxFrameBytes?: number
  defaultRequestTimeoutMs?: number
  /** Peer notifications (no id). Never affects request correlation. */
  onNotification?: (method: string, params: unknown) => void
  /** Diagnostics for frames that are dropped rather than acted on. */
  onDropped?: (reason: string, detail: Record<string, unknown>) => void
  /**
   * Answers agent→client requests. Omitted means "this client offers nothing",
   * and every such request is refused with -32601 rather than left hanging.
   */
  onServerRequest?: ServerRequestHandler
  /** Called once, when the transport dies. */
  onClose?: (error: Error) => void
}

type Pending = {
  method: string
  resolve: (value: unknown) => void
  reject: (error: Error) => void
  timer: ReturnType<typeof setTimeout> | null
  onAbort: (() => void) | null
  signal: AbortSignal | null
}

export type RequestOptions = {
  timeoutMs?: number
  signal?: AbortSignal
}

export class AcpStdioTransport {
  private readonly input: NodeJS.ReadableStream
  private readonly output: NodeJS.WritableStream
  private readonly maxFrameBytes: number
  private readonly defaultTimeoutMs: number
  private readonly onNotification?: (method: string, params: unknown) => void
  private readonly onDropped?: (reason: string, detail: Record<string, unknown>) => void
  private readonly onServerRequest?: ServerRequestHandler
  private readonly onCloseCallback?: (error: Error) => void

  private buffer = ''
  private bufferBytes = 0
  private nextId = 1
  private readonly pending = new Map<number, Pending>()
  /** Ids already settled locally — a late response for these is expected. */
  private readonly settled = new Set<number>()
  private closedWith: Error | null = null

  constructor(opts: AcpStdioTransportOptions) {
    this.input = opts.input
    this.output = opts.output
    this.maxFrameBytes = opts.maxFrameBytes ?? DEFAULT_MAX_FRAME_BYTES
    this.defaultTimeoutMs = opts.defaultRequestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS
    this.onNotification = opts.onNotification
    this.onDropped = opts.onDropped
    this.onServerRequest = opts.onServerRequest
    this.onCloseCallback = opts.onClose

    this.input.setEncoding?.('utf8')
    this.input.on('data', (chunk: string | Buffer) => this.ingest(chunk))
    this.input.on('end', () => this.close('peer closed stdout'))
    this.input.on('error', (error: Error) =>
      this.close(`stdout stream error: ${error.message}`),
    )
    this.output.on('error', (error: Error) =>
      this.close(`stdin stream error: ${error.message}`),
    )
  }

  /** Non-null once the transport is dead; every later call rejects with it. */
  get failure(): Error | null {
    return this.closedWith
  }

  get isOpen(): boolean {
    return this.closedWith === null
  }

  /** Requests still awaiting a response. Exposed for shutdown diagnostics. */
  get inFlight(): number {
    return this.pending.size
  }

  /**
   * Send a request and resolve with its `result`.
   *
   * Rejects with AcpRemoteError (peer said no), AcpRequestTimeoutError,
   * AcpRequestCancelledError, or AcpTransportClosedError. It never resolves
   * with a placeholder: a caller that cannot tell "engine said X" from "engine
   * is gone" is exactly the failure this file replaces.
   */
  request(method: string, params: unknown, opts: RequestOptions = {}): Promise<unknown> {
    if (this.closedWith) return Promise.reject(this.closedWith)
    if (opts.signal?.aborted) {
      return Promise.reject(new AcpRequestCancelledError(method))
    }

    const id = this.nextId++
    const timeoutMs = opts.timeoutMs ?? this.defaultTimeoutMs

    return new Promise<unknown>((resolve, reject) => {
      const entry: Pending = {
        method,
        resolve,
        reject,
        timer: null,
        onAbort: null,
        signal: opts.signal ?? null,
      }

      entry.timer = setTimeout(() => {
        this.settle(id, () => reject(new AcpRequestTimeoutError(method, timeoutMs)))
      }, timeoutMs)

      if (opts.signal) {
        entry.onAbort = () => {
          this.settle(id, () => reject(new AcpRequestCancelledError(method)))
        }
        opts.signal.addEventListener('abort', entry.onAbort, { once: true })
      }

      this.pending.set(id, entry)

      try {
        this.writeLine({ jsonrpc: '2.0', id, method, params })
      } catch (error: unknown) {
        this.settle(id, () =>
          reject(
            error instanceof Error ? error : new Error(String(error)),
          ),
        )
      }
    })
  }

  /** Fire-and-forget notification (no id, no response). */
  notify(method: string, params: unknown): void {
    if (this.closedWith) throw this.closedWith
    this.writeLine({ jsonrpc: '2.0', method, params })
  }

  /**
   * Tear the transport down and reject everything outstanding. Idempotent —
   * the first cause wins, so a violation is not overwritten by the child-exit
   * that follows from it.
   */
  close(reason: string | Error): void {
    if (this.closedWith) return
    const error =
      reason instanceof Error ? reason : new AcpTransportClosedError(reason)
    this.closedWith = error

    for (const entry of [...this.pending.values()]) {
      this.clearPending(entry)
      entry.reject(error)
    }
    this.pending.clear()
    this.settled.clear()
    this.onCloseCallback?.(error)
  }

  // ── framing ────────────────────────────────────────────────────

  private ingest(chunk: string | Buffer): void {
    if (this.closedWith) return
    const text = typeof chunk === 'string' ? chunk : chunk.toString('utf8')
    this.bufferBytes += Buffer.byteLength(text, 'utf8')
    this.buffer += text

    // Bound BEFORE splitting: a peer that never sends a newline must not be
    // able to grow this buffer without limit.
    if (this.bufferBytes > this.maxFrameBytes && !this.buffer.includes('\n')) {
      this.close(
        new AcpProtocolViolationError(
          `unterminated frame exceeded ${this.maxFrameBytes} bytes`,
        ),
      )
      return
    }

    let index = this.buffer.indexOf('\n')
    while (index >= 0) {
      const line = this.buffer.slice(0, index)
      this.buffer = this.buffer.slice(index + 1)
      this.bufferBytes = Buffer.byteLength(this.buffer, 'utf8')
      if (Buffer.byteLength(line, 'utf8') > this.maxFrameBytes) {
        this.close(
          new AcpProtocolViolationError(
            `frame of ${Buffer.byteLength(line, 'utf8')} bytes exceeds ${this.maxFrameBytes}`,
          ),
        )
        return
      }
      this.handleLine(line)
      if (this.closedWith) return
      index = this.buffer.indexOf('\n')
    }
  }

  private handleLine(rawLine: string): void {
    const line = rawLine.trim()
    // A blank line carries nothing and breaks nothing; NDJSON writers emit
    // them at flush boundaries. Anything non-blank must parse.
    if (line === '') return

    let message: unknown
    try {
      message = JSON.parse(line)
    } catch {
      this.close(
        new AcpProtocolViolationError(
          `stdout line is not JSON (${truncate(line)}) — stdout must carry protocol only`,
        ),
      )
      return
    }
    if (message === null || typeof message !== 'object' || Array.isArray(message)) {
      this.close(
        new AcpProtocolViolationError(
          `stdout line is not a JSON-RPC object (${truncate(line)})`,
        ),
      )
      return
    }

    const msg = message as Record<string, unknown>
    const hasMethod = typeof msg.method === 'string'
    const hasId = msg.id !== undefined && msg.id !== null

    if (hasMethod && hasId) {
      void this.handleServerRequest(msg)
      return
    }
    if (hasMethod) {
      this.onNotification?.(msg.method as string, msg.params)
      return
    }
    if (!hasId) {
      this.close(
        new AcpProtocolViolationError(`message has neither method nor id (${truncate(line)})`),
      )
      return
    }
    this.handleResponse(msg, line)
  }

  private handleResponse(msg: Record<string, unknown>, line: string): void {
    const id = msg.id
    // An UNCORRELATABLE response is dropped, not fatal.
    //
    // It used to close the transport, which meant one stray frame from the peer
    // took down a working session: the desk showed "the engine is not running"
    // while the engine was fine. A response we cannot match to a request we
    // made cannot affect our state — there is no pending promise for it to
    // resolve, and nothing to corrupt.
    //
    // Malformed FRAMING is different and still fatal: that means the stream
    // itself is no longer trustworthy, so we can no longer rely on any later
    // response either. This is a message we simply did not ask for.
    if (typeof id !== 'number') {
      // Reported in full rather than truncated: an unexplained frame is worth
      // being able to identify later.
      this.onDropped?.('response id is not one this client issued', { frame: line })
      return
    }
    const entry = this.pending.get(id)
    if (!entry) {
      // Two very different facts landed in this branch, and collapsing them
      // into a single "drop" was wrong:
      //
      //   an id we DID issue, already settled by timeout or cancellation
      //     — a late reply. Benign, and killing the session over it is the
      //       bug that made one stray frame read as "the engine is not
      //       running" while the engine was fine.
      //
      //   an id we NEVER issued
      //     — the peer is inventing correlation ids. That is not a stray
      //       message we can ignore: if its ids are unrelated to ours, no
      //       later response can be trusted to answer the request we think
      //       it answers, and a reply could be matched to the wrong call.
      //
      // Ids come from a monotonic counter, so `id >= nextId` PROVES the id was
      // never issued. Deciding this on the `settled` set instead would make
      // correctness depend on retention: settled entries are pruned, and a late
      // reply for a long-finished id would then be misreported as a violation.
      if (id >= this.nextId) {
        this.close(new AcpProtocolViolationError(`response for unknown id ${id} (${truncate(line)})`))
        return
      }
      if (this.settled.has(id)) {
        this.settled.delete(id)
        return
      }
      this.onDropped?.('response for an id with no pending request', {
        id,
        frame: truncate(line),
      })
      return
    }

    this.clearPending(entry)
    this.pending.delete(id)

    const errorBody = msg.error
    if (errorBody !== undefined && errorBody !== null) {
      const body = errorBody as Partial<JsonRpcErrorBody>
      entry.reject(
        new AcpRemoteError(entry.method, {
          code: typeof body.code === 'number' ? body.code : -32603,
          message: typeof body.message === 'string' ? body.message : 'unknown error',
          data: body.data,
        }),
      )
      return
    }
    entry.resolve(msg.result)
  }

  private async handleServerRequest(msg: Record<string, unknown>): Promise<void> {
    const id = msg.id
    const method = msg.method as string
    if (!this.onServerRequest) {
      this.writeSafely({
        jsonrpc: '2.0',
        id,
        error: {
          code: -32601,
          message: `client does not implement '${method}'`,
        },
      })
      return
    }
    try {
      const result = await this.onServerRequest(method, msg.params)
      this.writeSafely({ jsonrpc: '2.0', id, result: result ?? {} })
    } catch (error: unknown) {
      this.writeSafely({
        jsonrpc: '2.0',
        id,
        error: {
          code: -32603,
          message: error instanceof Error ? error.message : String(error),
        },
      })
    }
  }

  // ── plumbing ───────────────────────────────────────────────────

  /**
   * Settle a pending id exactly once, remembering it so a response that
   * arrives afterwards is dropped instead of read as a protocol violation.
   */
  private settle(id: number, finish: () => void): void {
    const entry = this.pending.get(id)
    if (!entry) return
    this.clearPending(entry)
    this.pending.delete(id)
    this.settled.add(id)
    finish()
  }

  /** Release every resource an in-flight request holds. No timer outlives it. */
  private clearPending(entry: Pending): void {
    if (entry.timer) {
      clearTimeout(entry.timer)
      entry.timer = null
    }
    if (entry.signal && entry.onAbort) {
      entry.signal.removeEventListener('abort', entry.onAbort)
      entry.onAbort = null
    }
  }

  private writeLine(message: unknown): void {
    const encoded = JSON.stringify(message)
    if (Buffer.byteLength(encoded, 'utf8') > this.maxFrameBytes) {
      throw new AcpProtocolViolationError(
        `outbound frame of ${Buffer.byteLength(encoded, 'utf8')} bytes exceeds ${this.maxFrameBytes}`,
      )
    }
    this.output.write(`${encoded}\n`)
  }

  /** Best-effort write for replies — a dead pipe closes rather than throws. */
  private writeSafely(message: unknown): void {
    try {
      this.writeLine(message)
    } catch (error: unknown) {
      this.close(error instanceof Error ? error : new Error(String(error)))
    }
  }
}

function truncate(value: string, max = 200): string {
  return value.length <= max ? value : `${value.slice(0, max)}…`
}
