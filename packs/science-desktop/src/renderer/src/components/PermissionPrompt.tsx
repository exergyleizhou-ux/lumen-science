/**
 * The dialog a person actually answers when the engine asks permission.
 *
 * Design constraints come from what this gates. Approving here lets the Rust
 * SessionActor run code, so the dialog must not make approving the easy,
 * reflexive action:
 *
 *   - No default-focused approve button. Enter does not approve.
 *   - Escape and the backdrop DENY rather than dismiss. There is no way to
 *     make the dialog go away without answering, because "went away" would
 *     otherwise mean "the engine is still waiting" and the user would think
 *     they had cancelled.
 *   - The operation and its target are shown verbatim. A prompt that says
 *     "allow this action?" teaches people to click yes.
 *
 * Asks queue rather than replace: a second request must not overwrite a first
 * one the user has not answered, or the first silently waits out its timeout
 * while its dialog is gone.
 */

import { useCallback, useEffect, useRef, useState, type ReactElement } from 'react'

export type PermissionAsk = {
  requestId: string
  operation: string
  target: string
  detail?: string
}

type Decision = 'allow_once' | 'reject'

export type PermissionPromptProps = {
  /** Subscribes to asks; returns an unsubscribe. */
  subscribe: (listener: (ask: PermissionAsk) => void) => () => void
  respond: (requestId: string, decision: Decision) => Promise<unknown>
}

export function PermissionPrompt({ subscribe, respond }: PermissionPromptProps): ReactElement | null {
  const [queue, setQueue] = useState<PermissionAsk[]>([])
  const [busy, setBusy] = useState(false)
  const denyRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    return subscribe((ask) => {
      // Append. A newer ask must never displace one still unanswered.
      setQueue((current) =>
        current.some((q) => q.requestId === ask.requestId) ? current : [...current, ask]
      )
    })
  }, [subscribe])

  const current = queue[0]

  const answer = useCallback(
    async (decision: Decision) => {
      if (!current || busy) return
      setBusy(true)
      try {
        await respond(current.requestId, decision)
      } finally {
        setQueue((q) => q.filter((item) => item.requestId !== current.requestId))
        setBusy(false)
      }
    },
    [current, busy, respond]
  )

  // Escape denies. It does not close the dialog without answering: the engine
  // is blocked on a reply, and a dismissed prompt would leave the user
  // believing they had cancelled while the request waits out its timeout.
  useEffect(() => {
    if (!current) return
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.preventDefault()
        void answer('reject')
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [current, answer])

  // Focus lands on Deny, so a stray Enter or Space cannot approve.
  useEffect(() => {
    if (current) denyRef.current?.focus()
  }, [current])

  if (!current) return null

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="permission-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={() => void answer('reject')}
    >
      <div
        className="mx-4 w-full max-w-md rounded-lg border border-border bg-background p-5 shadow-xl"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 id="permission-title" className="text-base font-semibold text-foreground">
          Lumen is asking permission
        </h2>

        <dl className="mt-4 space-y-2 text-sm">
          <div>
            <dt className="text-muted-foreground">Operation</dt>
            <dd className="break-all font-mono text-foreground">{current.operation}</dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Target</dt>
            <dd className="break-all text-foreground">{current.target}</dd>
          </div>
          {current.detail ? (
            <div>
              <dt className="text-muted-foreground">Effect</dt>
              <dd className="break-all text-foreground">{current.detail}</dd>
            </div>
          ) : null}
        </dl>

        <p className="mt-4 text-xs text-muted-foreground">
          Approving allows this one operation. Closing this dialog denies it.
          {queue.length > 1 ? ` ${queue.length - 1} more waiting.` : ''}
        </p>

        <div className="mt-5 flex justify-end gap-2">
          {/* Deny first in DOM order, and focused: the safe choice is the one
              a reflex reaches. */}
          <button
            ref={denyRef}
            type="button"
            disabled={busy}
            onClick={() => void answer('reject')}
            className="rounded-md border border-border px-3 py-1.5 text-sm text-foreground hover:bg-muted disabled:opacity-50"
          >
            Deny
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => void answer('allow_once')}
            className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:opacity-90 disabled:opacity-50"
          >
            Allow once
          </button>
        </div>
      </div>
    </div>
  )
}
