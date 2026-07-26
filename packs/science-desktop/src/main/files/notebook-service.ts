/**
 * OSF-3 Notebook product service.
 *
 * Plan / dry-run locally; live execute only via ACP notebook_execute.
 * Never imports or constructs KernelExecutor.
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

export type AcpNotebookCall = (
  toolName: string,
  args: Record<string, unknown>,
) => Promise<unknown>

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
          tool: 'notebook_execute',
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
      const access = assertNotebookExecuteAccess(plan, getTrustedPreviewContext())
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

      try {
        const result = await opts.acpCall('notebook_execute', {
          code: req.code,
          language: req.language,
          plan_id: plan.planId,
          code_hash: plan.codeHash,
          cell_id: plan.cellId,
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
  return false
}
