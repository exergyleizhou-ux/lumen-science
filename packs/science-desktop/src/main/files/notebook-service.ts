/**
 * OSF-3 Notebook product service.
 *
 * Plan / dry-run locally; live execute via the engine's `workflow_execute`.
 * Never imports or constructs KernelExecutor.
 *
 * ## Why workflow_execute and not notebook_execute
 *
 * This service originally called `notebook_execute` — which is on the method
 * registry's REJECTED list ("Go MCP tool, not a Rust ACP extension method").
 * The registry refused it before it ever reached the wire, so the "Run in
 * engine" button could not succeed against any engine, ever. Meanwhile LS5-K8
 * had already given the Rust engine a real cell-execution route:
 * `workflow_execute` runs a NotebookCell step through the SessionActor —
 * permission prompt, kernel admission from a pinned interpreter path, sandboxed
 * exec-loop, run record. A cell is exactly a one-step workflow, so that is what
 * this sends.
 */

import {
  planNotebookCell,
  assertNotebookExecuteAccess,
  exportHistoryToIpynb,
  type NotebookCellRequest,
  type NotebookCellPlan,
  type NotebookHistoryCell,
  type NotebookLanguage,
} from './notebook-plan'
import { getTrustedPreviewContext } from './session-identity'
import { randomUUID } from 'node:crypto'

export type AcpNotebookCall = (
  toolName: string,
  args: Record<string, unknown>,
) => Promise<unknown>

/**
 * Resolves the interpreter a kernel step runs on.
 *
 * The engine requires an ABSOLUTE path — which binary ran is part of the
 * evidence, so nothing may be resolved through PATH on the far side. Failure is
 * a value, not an exception: "no interpreter" is an expected state on a fresh
 * machine and the caller needs the reason to show.
 */
export type ResolveInterpreter = () => Promise<
  { ok: true; interpreterPath: string } | { ok: false; reason: string }
>

export type NotebookService = {
  plan: (req: NotebookCellRequest) => ReturnType<typeof planNotebookCell>
  dryRun: (req: NotebookCellRequest) => {
    ok: true
    plan: NotebookCellPlan
    wouldCall: { tool: string; args: { code: string } }
  } | { ok: false; reason: string }
  execute: (req: NotebookCellRequest) => Promise<unknown>
  history: () => NotebookHistoryCell[]
  clearHistory: () => void
  exportIpynb: () => Record<string, unknown> | { ok: false; reason: string }
}

export function createNotebookService(opts: {
  acpCall?: AcpNotebookCall
  resolveInterpreter?: ResolveInterpreter
  /** Owner recorded on the run when the trusted context lacks one. */
  defaultOwnerId?: string
  storeRoot?: string
  approvalTimeoutMs?: number
  /** Injectable for tests; production uses crypto.randomUUID. */
  newOperationId?: () => string
}): NotebookService {
  const history: NotebookHistoryCell[] = []

  return {
    plan(req) {
      return planNotebookCell(req)
    },

    dryRun(req) {
      const planned = planNotebookCell({ ...req, dryRun: true })
      if ('ok' in planned && planned.ok === false) {
        return planned
      }
      const plan = planned as NotebookCellPlan
      return {
        ok: true,
        plan,
        wouldCall: {
          tool: 'workflow_execute',
          args: { code: req.code },
        },
      }
    },

    async execute(req) {
      const planned = planNotebookCell({ ...req, dryRun: false })
      if ('ok' in planned && planned.ok === false) {
        return planned
      }
      const plan = planned as NotebookCellPlan
      const context = getTrustedPreviewContext()
      const access = assertNotebookExecuteAccess(plan, context)
      if (!access.ok) {
        return { ok: false, reason: access.reason, plan }
      }
      if (!opts.acpCall) {
        return {
          ok: false,
          reason: 'no ACP caller configured — cannot execute without Lumen bridge',
          plan,
        }
      }
      if (!opts.resolveInterpreter) {
        // Refuse rather than guess: the engine needs an absolute interpreter
        // path because which binary ran is part of the run's evidence.
        return {
          ok: false,
          reason: 'no interpreter resolver configured — cannot name the binary this cell would run on',
          plan,
        }
      }
      const interpreter = await opts.resolveInterpreter()
      if (!interpreter.ok) {
        return { ok: false, reason: interpreter.reason, plan }
      }

      try {
        // A cell is a one-step workflow. Field casing is split on purpose:
        // the OUTER params are camelCase (WorkflowExecuteParams,
        // rename_all = "camelCase", deny_unknown_fields — a stray field is an
        // invalid_params error, not ignored), while the INNER spec is
        // snake_case (WorkflowSpec derives serde without a rename).
        const result = await opts.acpCall('workflow_execute', {
          ownerId: context?.ownerId ?? opts.defaultOwnerId ?? 'local-user',
          storeRoot: opts.storeRoot ?? 'science-store',
          // Idempotency key: a retried IPC must not run the cell twice.
          operationId: (opts.newOperationId ?? randomUUID)(),
          interpreterPath: interpreter.interpreterPath,
          // Explicit opt-in: the engine's default policy refuses kernel steps,
          // so running arbitrary code is a decision made visibly here, and the
          // engine still asks a human before honouring it.
          allowKernelSteps: true,
          approvalTimeoutMs: opts.approvalTimeoutMs ?? 110_000,
          workflowSpec: {
            workflow_id: `notebook-${plan.cellId}`,
            project_id: context?.projectId ?? '',
            name: 'Notebook cell',
            steps: [
              {
                step_id: plan.cellId,
                kind: 'NotebookCell',
                connector_id: null,
                // The step field carries the SOURCE, not a reference: the
                // executor hashes this value as the cell's identity.
                notebook_cell: req.code,
                inputs: [],
                parameters: {},
                timeout_secs: 120,
                retry_policy: null,
                cache_policy: 'NoCache',
                acceptance_rules: [],
              },
            ],
            parameters: {},
            permissions: [],
            resources: {
              max_concurrent_steps: 1,
              max_total_duration_secs: 300,
              max_memory_mb: 1024,
              max_disk_mb: 512,
            },
            schema_version: 1,
          },
        })
        const cell: NotebookHistoryCell = {
          cellId: plan.cellId,
          language: req.language as NotebookLanguage,
          source: req.code,
          codeHash: plan.codeHash,
          planId: plan.planId,
          executedAt: Date.now(),
          dryRun: false,
          ok: true,
          stdout: extractStdout(result),
          stderr: extractStderr(result),
        }
        if (isFailedResult(result)) {
          cell.ok = false
        }
        history.push(cell)
        return { ok: true, plan, result, authority: 'SessionActor/KernelAdapter' }
      } catch (e: unknown) {
        const cell: NotebookHistoryCell = {
          cellId: plan.cellId,
          language: req.language as NotebookLanguage,
          source: req.code,
          codeHash: plan.codeHash,
          planId: plan.planId,
          executedAt: Date.now(),
          dryRun: false,
          ok: false,
          stderr: (e as Error).message || String(e),
        }
        history.push(cell)
        return {
          ok: false,
          reason: (e as Error).message || String(e),
          plan,
        }
      }
    },

    history() {
      return [...history]
    },

    clearHistory() {
      history.length = 0
    },

    exportIpynb() {
      const trusted = getTrustedPreviewContext()
      if (!trusted) {
        return { ok: false, reason: 'no trusted session for export' }
      }
      return exportHistoryToIpynb({
        projectId: trusted.projectId,
        cells: history,
      })
    },
  }
}

function extractStdout(result: unknown): string | undefined {
  if (!result || typeof result !== 'object') return undefined
  const r = result as Record<string, unknown>
  if (typeof r.Stdout === 'string') return r.Stdout
  if (typeof r.stdout === 'string') return r.stdout
  return undefined
}

function extractStderr(result: unknown): string | undefined {
  if (!result || typeof result !== 'object') return undefined
  const r = result as Record<string, unknown>
  if (typeof r.Stderr === 'string') return r.Stderr
  if (typeof r.stderr === 'string') return r.stderr
  return undefined
}

function isFailedResult(result: unknown): boolean {
  if (!result || typeof result !== 'object') return false
  const r = result as Record<string, unknown>
  if (r.OK === false || r.ok === false) return true
  // workflow_execute reports the run's terminal state rather than a boolean.
  // Anything other than succeeded — failed, denied, timed_out, cancelled,
  // interrupted — is not a success, and recording it as one would put a false
  // claim in the notebook history. RunState serialises snake_case ("succeeded",
  // not "Succeeded") — asserted against a live run, not read off the enum.
  if (typeof r.state === 'string' && r.state !== 'succeeded') return true
  return false
}
