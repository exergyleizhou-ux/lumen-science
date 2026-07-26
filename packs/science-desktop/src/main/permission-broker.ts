/**
 * Carries an engine permission request to a human, and the answer back.
 *
 * The Rust SessionActor asks `session/request_permission` before anything
 * consequential. The desktop passed no handler, so the transport answered
 * `-32601` and every approval-requiring mutation failed. The seam was left
 * deliberately unused: auto-approving in the main process would grant
 * execution authority with nobody in the loop, which is worse than refusing.
 *
 * So this brokers to a real person. The design constraints all follow from one
 * rule — **silence is never consent**:
 *
 *   - no window to ask        → deny
 *   - the user closes the UI  → deny
 *   - the request times out   → deny
 *   - a malformed request     → deny
 *   - the renderer answers an id we never issued → ignored, and the real
 *     request still times out rather than being resolved by an unrelated reply
 *
 * Nothing here can produce an allow that a human did not click.
 *
 * Electron-free on purpose: `ask` is injected, so the authority scripts can
 * execute this module and the decision logic is testable without a UI.
 */

/** What the engine asked, reduced to what a person needs to decide. */
export type PermissionAsk = {
  requestId: string
  /** e.g. `x.ai/science/workflow_execute` */
  operation: string
  /** Human-readable target: a project title, a file, a command summary. */
  target: string
  /** Set when the engine described the concrete effect. */
  detail?: string
  /**
   * The option ids the ENGINE offered, by kind.
   *
   * The answer must name one of these. Returning an id the engine never
   * offered is not an approval it can act on — it was read as a denial, so a
   * user clicking Allow got their operation refused while everything looked
   * like it worked.
   */
  allowOptionId?: string
  rejectOptionId?: string
}

export type PermissionDecision = 'allow_once' | 'reject'

/** Presents the ask and resolves with what the human chose. */
export type AskHuman = (ask: PermissionAsk) => Promise<PermissionDecision>

export type PermissionBrokerOptions = {
  ask: AskHuman
  /**
   * How long to wait for a human. Generous by default: a person reading a
   * dialog is not a stalled process, and a short timeout would train the
   * product to deny things the user was about to allow.
   */
  timeoutMs?: number
  onDenied?: (ask: PermissionAsk, reason: string) => void
}

const DEFAULT_TIMEOUT_MS = 5 * 60_000

/**
 * The user's decision, before it is put in the envelope ACP expects.
 *
 * This is NOT the wire shape — see `PermissionResponse`. Keeping the two named
 * separately is the point: they differ by one level of nesting, they serialise
 * to almost the same JSON, and confusing them costs nothing at compile time
 * and everything at runtime.
 */
type PermissionOutcome =
  | { outcome: 'selected'; optionId: string }
  | { outcome: 'cancelled' }

/**
 * What actually goes back on the wire.
 *
 * `RequestPermissionResponse { outcome: RequestPermissionOutcome }` — the
 * decision nested inside a field of the same name. The schema calls this out
 * itself ("This extra-level is unfortunately needed because the output must be
 * an object", client.rs:669).
 *
 * We were returning the INNER object alone. That deserialises as a response
 * whose `outcome` is the string "selected" where an object is required, so the
 * engine could not read the answer and recorded the run as Denied — a user
 * clicked Allow and their project was refused, with the dialog, the click and
 * the timing all looking exactly like success.
 *
 * Nothing in an engine-less test can catch this: it is a fact about what the
 * engine accepts, and only e2e/live-engine.spec.ts talks to one.
 */
export type PermissionResponse = { outcome: PermissionOutcome }

/** Put a decision in the envelope ACP requires. */
const envelope = (outcome: PermissionOutcome): PermissionResponse => ({ outcome })


/**
 * Read the engine's request into the shape a person can act on.
 *
 * Returns null when the request is not recognisable. That is deliberately not
 * an error to display: an unparseable permission request must be denied, and
 * showing a dialog whose text we could not read would invite a click on
 * something nobody understood.
 */
export function describeAsk(requestId: string, params: unknown): PermissionAsk | null {
  if (typeof params !== 'object' || params === null) return null
  const p = params as Record<string, unknown>

  const update = (typeof p.toolCall === 'object' && p.toolCall !== null
    ? (p.toolCall as Record<string, unknown>)
    : {}) as Record<string, unknown>

  const operation =
    typeof p.method === 'string'
      ? p.method
      : typeof update.title === 'string'
        ? update.title
        : ''
  if (!operation) return null

  const target =
    typeof p.target === 'string'
      ? p.target
      : typeof update.kind === 'string'
        ? update.kind
        : 'unspecified target'

  const detail = typeof p.detail === 'string' ? p.detail : undefined

  // Read the offered options rather than assuming their ids. `allow_once` was
  // hardcoded here and the engine had never offered that id.
  const options = Array.isArray(p.options) ? (p.options as Record<string, unknown>[]) : []
  const idOfKind = (...kinds: string[]): string | undefined => {
    for (const kind of kinds) {
      const hit = options.find((o) => o?.kind === kind)
      const id = hit?.optionId ?? hit?.id
      if (typeof id === 'string' && id.length > 0) return id
    }
    return undefined
  }

  return {
    requestId,
    operation,
    target,
    detail,
    allowOptionId: idOfKind('allow_once', 'allow_always'),
    rejectOptionId: idOfKind('reject_once', 'reject_always'),
  }
}

export class PermissionBroker {
  private readonly ask: AskHuman
  private readonly timeoutMs: number
  private readonly onDenied?: (ask: PermissionAsk, reason: string) => void
  /** Asks currently awaiting a human, so a shutdown can deny them explicitly. */
  private readonly pending = new Map<string, PermissionAsk>()

  constructor(opts: PermissionBrokerOptions) {
    this.ask = opts.ask
    this.timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS
    this.onDenied = opts.onDenied
  }

  pendingCount(): number {
    return this.pending.size
  }

  /**
   * Deny every outstanding ask, for shutdown.
   *
   * The timeout timers are ref'd so an awaited request always settles, which
   * means quit must actively resolve them rather than relying on the loop
   * draining. Denying is the only safe answer: the user is closing the app, not
   * approving anything.
   */
  denyAllPending(reason = 'the application is closing'): number {
    const denied = this.pending.size
    for (const ask of this.pending.values()) {
      this.onDenied?.(ask, reason)
    }
    this.pending.clear()
    return denied
  }

  /**
   * Handle one `session/request_permission`.
   *
   * Never throws: a thrown handler would surface to the engine as a transport
   * fault rather than a decision, and the engine would be right to treat an
   * unanswered permission as a failure. Denying explicitly is the honest reply.
   */
  async handle(requestId: string, params: unknown): Promise<PermissionResponse> {
    const ask = describeAsk(requestId, params)
    if (!ask) {
      this.onDenied?.(
        { requestId, operation: 'unknown', target: 'unparseable request' },
        'the permission request could not be read',
      )
      return envelope({ outcome: 'cancelled' })
    }

    this.pending.set(requestId, ask)
    try {
      const decision = await this.withTimeout(ask)
      if (decision === 'allow_once') {
        if (!ask.allowOptionId) {
          // The engine offered no allow option, so there is nothing to select.
          // Cancelling is the honest reply; inventing an id produced a denial
          // that looked like an approval to everyone except the engine.
          this.onDenied?.(ask, 'the engine offered no allow option')
          return envelope({ outcome: 'cancelled' })
        }
        return envelope({ outcome: 'selected', optionId: ask.allowOptionId })
      }
      // A reject option is selected when offered, so the engine records a
      // decision rather than a cancellation — they are different facts.
      if (ask.rejectOptionId) {
        return envelope({ outcome: 'selected', optionId: ask.rejectOptionId })
      }
      return envelope({ outcome: 'cancelled' })
    } catch (error: unknown) {
      // Includes the timeout. Any failure to obtain an answer is a denial.
      this.onDenied?.(ask, (error as Error)?.message || String(error))
      return envelope({ outcome: 'cancelled' })
    } finally {
      this.pending.delete(requestId)
    }
  }

  private async withTimeout(ask: PermissionAsk): Promise<PermissionDecision> {
    let timer: NodeJS.Timeout | undefined
    try {
      return await Promise.race([
        this.ask(ask),
        new Promise<never>((_, reject) => {
          // Deliberately NOT unref'd. An unref'd timer lets the process exit
          // while this race is still awaited, so the await never settles and
          // the caller silently gets nothing — which is how the first run of
          // the test suite ended after five assertions. Quit is handled by
          // denyAllPending() instead, which resolves every waiter rather than
          // abandoning it.
          timer = setTimeout(
            () => reject(new Error(`no answer within ${this.timeoutMs}ms`)),
            this.timeoutMs,
          )
        }),
      ])
    } finally {
      if (timer) clearTimeout(timer)
    }
  }
}
