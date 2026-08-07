/**
 * Lumen ACP Bridge — Electron main → Rust Lumen binary, over ACP stdio.
 *
 * All science operations go through the Rust SessionActor via ACP. The Electron
 * main process is ONLY responsible for window/tray/updater. This file is not an
 * execution path for science tools, notebooks, or reviewers.
 *
 * WHAT THIS FILE USED TO DO, AND WHY IT NEVER WORKED
 *
 * Despite its name it spoke HTTP. It spawned
 * `lumen-science serve --interface loopback --port 17000`, polled
 * `GET /health`, and POSTed to `/tools/call` — and never wrote a byte to the
 * child's stdin. No such subcommand and no such port exist in either engine:
 * the Go CLI (packs/science/standalone/cmd/science/main.go) has no `serve`, and
 * the Rust binary exposes the 24 `x.ai/science/*` methods only over
 * `lumen agent stdio`. So the child died at once, `startLumen()` always
 * rejected, index.ts logged and swallowed it, and every `acpCall` returned
 * ECONNREFUSED. The desktop had never talked to an engine.
 *
 * It now runs the real protocol — spawn → initialize → authenticate →
 * session/new → `_x.ai/science/*` — split across four modules:
 *
 *   lumen-process-manager.ts    spawn, hash-pin, stderr capture, SIGTERM/KILL
 *   acp-stdio-transport.ts      NDJSON framing, id correlation, bounded frames
 *   acp-session-manager.ts      handshake, session lifecycle, engine state
 *   science-method-registry.ts  the allowlist of methods that actually exist
 *
 * Apache-2.0. Adapted from Open Science (d8f11e34) and modified for
 * Lumen Science Desktop authority model.
 */

import { type IpcMain, app } from 'electron'
import { validateIpcChannel } from './lumen-authority-policy'
// Type-only import: erased at build time, so this does NOT make the bridge depend on the science
// IPC surface at runtime. The contract is owned by the consumer because that module must stay
// Electron-free to remain testable; the bridge conforms to it.
import type { IpcMainLike } from './files/science-ipc'
import { AcpSessionManager, type EngineState } from './acp-session-manager'
import { PermissionBroker, type AskHuman } from './permission-broker'
import { ENGINE_APPROVAL_TIMEOUT_MS } from './files/science-ipc'
import {
  isGenericRendererScienceMethod,
  listScienceMethods,
} from './science-method-registry'

// ── Engine singleton ─────────────────────────────────────────────

let manager: AcpSessionManager | null = null
let broker: PermissionBroker | null = null

/**
 * How a permission ask reaches a person. Installed by the IPC layer once a
 * window exists.
 *
 * Absent until then, and absence DENIES rather than allows: an engine request
 * arriving before the UI is ready must not be auto-approved just because no
 * one could be asked yet.
 */
let askHuman: AskHuman | null = null

export function setPermissionPrompt(ask: AskHuman | null): void {
  askHuman = ask
}

/** Deny anything still waiting, for shutdown. Returns how many were denied. */
export function denyPendingPermissions(reason?: string): number {
  return broker?.denyAllPending(reason) ?? 0
}
let lastState: EngineState = { status: 'stopped' }

/**
 * Session workspace for the engine.
 *
 * Science store paths are pinned inside the session cwd by the Rust adapter
 * (`canonical_dir_within`), so this is the root every project store must live
 * under. userData keeps it per-install and writable in a packaged app.
 */
function sessionWorkspace(): string {
  return app.getPath('userData')
}

let permissionSeq = 0

function ensureBroker(): PermissionBroker {
  if (broker) return broker
  broker = new PermissionBroker({
    // Strictly shorter than the engine's approval window. If the prompt
    // outlived it, a user could click Allow, watch the dialog close, and have
    // the engine already have abandoned the run — the worst kind of failure,
    // because it looks like success.
    timeoutMs: ENGINE_APPROVAL_TIMEOUT_MS - 10_000,
    ask: async (request) => {
      if (!askHuman) {
        // No UI is listening. Refusing is the only honest answer: nobody
        // declined, but nobody approved either, and only an approval may
        // proceed.
        throw new Error('no permission UI is available to ask')
      }
      return askHuman(request)
    },
    onDenied: (request, reason) => {
      console.warn(`[lumen] permission denied for ${request.operation}: ${reason}`)
    },
  })
  return broker
}

function ensureManager(): AcpSessionManager {
  if (manager) return manager
  manager = new AcpSessionManager({
    cwd: sessionWorkspace(),
    clientVersion: app.getVersion(),
    process: {
      // process.resourcesPath is only defined in a packaged app; undefined
      // here simply means the bundled candidate is skipped.
      resourcesPath: process.resourcesPath,
      childEnv: {
        LUMEN_DESKTOP: '1',
        LUMEN_NO_BROWSER: '1',
      },
    },
    // The engine asks before anything consequential. Without this handler the
    // transport answered -32601 and every approval-requiring mutation failed.
    onServerRequest: async (method, params) => {
      if (method !== 'session/request_permission') {
        // Unknown server-initiated methods are refused, not guessed at.
        throw new Error(`unsupported server request: ${method}`)
      }
      const requestId = `perm-${++permissionSeq}`
      return ensureBroker().handle(requestId, params)
    },
    onStateChange: (state) => {
      lastState = state
    },
    log: {
      info: (message, meta) => console.log(`[lumen] ${message}`, meta ?? ''),
      warn: (message, meta) => console.warn(`[lumen] ${message}`, meta ?? ''),
      error: (message, meta) => console.error(`[lumen] ${message}`, meta ?? ''),
    },
  })
  return manager
}

/** SHA-256 of the binary actually executed, or null before it is resolved. */
export function getLumenBinaryHash(): string | null {
  return manager?.getBinaryHash() ?? null
}

/**
 * Engine state for diagnostics surfaces. `unavailable` always carries a reason;
 * callers must render it rather than substituting a placeholder.
 */
export function getLumenEngineState(): EngineState {
  return manager?.getState() ?? lastState
}

/**
 * Spawn the engine and complete the ACP handshake. Rejects on failure.
 *
 * Unlike the manager (which deliberately does not self-retry so a crash loop
 * stays visible), the bridge retries transient failures a bounded number of
 * times with backoff. The failure mode this guards against is real: on
 * 2026-08-07 a loaded machine (load ~5, three stale TUI processes spinning)
 * made `initialize`+`authenticate` exceed the old shared 60s budget, so
 * `session/new` got ~1ms, the engine went `unavailable`, and stayed that way
 * until the app was manually restarted. A retry after a few seconds lets the
 * cold start finish. A genuinely broken binary still surfaces after
 * START_RETRY_ATTEMPTS — the failure is bounded, logged, and visible.
 */
const START_RETRY_ATTEMPTS = 3
const START_RETRY_BACKOFF_MS = [2_000, 8_000, 24_000] as const

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

export async function startLumen(): Promise<void> {
  let lastError: unknown
  for (let attempt = 1; attempt <= START_RETRY_ATTEMPTS; attempt++) {
    try {
      await ensureManager().start()
      return
    } catch (error: unknown) {
      lastError = error
      if (attempt < START_RETRY_ATTEMPTS) {
        const backoff = START_RETRY_BACKOFF_MS[attempt - 1]
        console.warn(
          `[lumen] engine handshake failed (attempt ${attempt}/${START_RETRY_ATTEMPTS}), ` +
            `retrying in ${backoff}ms: ${error instanceof Error ? error.message : String(error)}`,
        )
        await sleep(backoff)
      }
    }
  }
  throw lastError
}

/** Graceful shutdown: close the transport, SIGTERM, then SIGKILL. */
export async function stopLumen(): Promise<void> {
  const current = manager
  manager = null
  // Invalidate identity before awaiting process shutdown. A stopped, crashed,
  // or already-absent engine may never preserve a capability from its session.
  const { clearAllTrustedSessions } = await import('./files/session-binding')
  clearAllTrustedSessions()
  lastState = { status: 'stopped' }
  if (!current) return
  await current.stop()
}

// ── Authority boundary enforcement ───────────────────────────────

/**
 * safeHandle — single choke-point that refuses to register handlers
 * for banned science-execution channels. Every science-adjacent IPC
 * registration must go through this function.
 *
 * Returns the original handler if allowed, or a rejection handler.
 *
 * Takes IpcMainLike, not Electron's IpcMain. files/science-ipc.ts publishes SafeHandleFn over that
 * minimal shape so its registration site can be exercised without booting Electron (see
 * scripts/test-register-ipc-mock.mts), and this function is the ONLY production value passed for
 * it — so demanding the full IpcMain here made the real implementation unassignable to the very
 * contract it exists to satisfy. `handle` is the entire surface used below, so the narrow type is
 * also the accurate one; the real ipcMain still satisfies it at the call site in ipc.ts.
 */
export function safeHandle(
  ipcMain: IpcMainLike,
  channel: string,
  handler: (_event: unknown, ...args: unknown[]) => Promise<unknown>,
): void {
  if (!validateIpcChannel(channel)) {
    console.error(`[lumen-security] BANNED IPC channel: ${channel} — handler NOT registered`)
    // Still register a rejection so renderer calls fail fast
    ipcMain.handle(channel, async () => ({
      _lumenBanned: true,
      channel,
      reason: 'EXECUTION AUTHORITY REMOVED — use Lumen bridge (acp:call)',
    }))
    return
  }
  ipcMain.handle(channel, handler)
}

// ── ACP proxy (renderer -> Electron main -> Rust Lumen) ──────────

/**
 * Call one science method on the Rust engine.
 *
 * `toolName` is a science method name — the registry decides whether it may go
 * on the wire. Three names this pack has been sending exist in neither engine
 * (`artifact_resolve`, `compute_plan`) and two more are Go MCP tools rather
 * than Rust ACP methods (`notebook_execute`, `start_review`); all four are
 * rejected here, by name, with the reason. That rejection is deliberate and load-bearing: while the
 * transport was fictional those calls failed with ECONNREFUSED, which read as
 * "engine down" instead of "this method does not exist".
 *
 * Throws when the engine is unavailable. It never resolves with a mock or a
 * stale value — the caller must surface the failure.
 */
export async function acpCall(
  toolName: string,
  args: Record<string, unknown>,
): Promise<unknown> {
  return ensureManager().callScience(toolName, args ?? {})
}

/**
 * The science methods this engine can serve.
 *
 * Replaces `acpToolsFetch`, an adapter that accepted a fake `/tools/call`
 * Request and unpacked it back into a method name and arguments. That shim
 * existed only because science-ipc.ts modelled the transport as HTTP; with that
 * signature reworked to a typed call, nothing needs to build or parse a fake
 * Request, and the last place the desktop pretended to speak HTTP is gone.
 */
export async function listScienceTools(): Promise<unknown> {
  return {
    tools: listScienceMethods().map((m) => ({
      name: m.name,
      method: m.qualified,
      transport: 'acp-stdio',
      genericRendererCallable: isGenericRendererScienceMethod(m.name),
      route: isGenericRendererScienceMethod(m.name)
        ? 'generic-acp-call'
        : 'sender-bound-settings-ipc',
    })),
    authority: 'rust-acp-extension-methods',
  }
}

// ── Wire into Electron IPC ───────────────────────────────────────

let _guardInstalled = false

// The IpcMain handle is deliberately unused: this guard must NOT raw-register any channel (asserted
// by scripts/test-ipc-handlers.mts). It stays in the signature because callers pass it and because
// re-acquiring the authority to register would be a one-line change here — keeping the parameter
// makes that boundary explicit rather than implying the guard has no access to IPC at all.
export function installIpcGuard(_ipcMain: IpcMain): void {
  if (_guardInstalled) return
  _guardInstalled = true
  // Channel registration is done by registerIpcHandlers via safeHandle.
  // This guard only marks installation complete and logs the hash.
  console.log('[lumen-security] IPC guard installed — hash:', getLumenBinaryHash() ?? 'unavailable')
}
