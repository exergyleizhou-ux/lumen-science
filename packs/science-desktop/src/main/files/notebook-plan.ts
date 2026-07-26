/**
 * OSF-3 Notebook plan — pure module (no Electron, no process spawn).
 *
 * Builds an execution plan that can only be fulfilled by Lumen ACP
 * (SessionActor → KernelAdapter via workflow_execute). TypeScript KernelExecutor
 * remains stubbed and must never receive this plan.
 */

import { createHash, randomUUID } from 'node:crypto'
import type { AccessResult } from '../lumen-authority-policy'
import type { TrustedPreviewContext } from './session-identity'

export type NotebookLanguage = 'python' | 'r'

export type NotebookCellRequest = {
  language: NotebookLanguage
  code: string
  cellId?: string
  /** When true, never invoke live kernel — plan + static checks only */
  dryRun?: boolean
}

export type NotebookCellPlan = {
  planId: string
  cellId: string
  language: NotebookLanguage
  codeHash: string
  codeLength: number
  dryRun: boolean
  tool: 'workflow_execute'
  authority: 'SessionActor/KernelAdapter'
  requiresAdmittedKernel: true
  /** Static warnings (not hard fails) */
  warnings: string[]
  createdAt: number
}

const BANNED_CODE_PATTERNS: { re: RegExp; reason: string }[] = [
  { re: /\bos\.system\s*\(/i, reason: 'os.system is denied in product notebook path' },
  { re: /\bsubprocess\.(?:run|call|Popen|check_output)\s*\(/i, reason: 'subprocess spawn is denied without admission' },
  { re: /\beval\s*\(/i, reason: 'eval is denied in product notebook path' },
  { re: /\bexec\s*\(\s*['"`]/i, reason: 'dynamic exec of string is denied' },
  { re: /rm\s+-rf\s+[\/~]/i, reason: 'destructive shell pattern denied' },
]

export function hashNotebookCode(code: string): string {
  return createHash('sha256').update(code, 'utf8').digest('hex')
}

/**
 * Build a notebook cell plan. Does not execute anything.
 */
export function planNotebookCell(req: NotebookCellRequest): NotebookCellPlan | { ok: false; reason: string } {
  if (req.language !== 'python' && req.language !== 'r') {
    return { ok: false, reason: 'language must be python or r' }
  }
  if (typeof req.code !== 'string' || !req.code.trim()) {
    return { ok: false, reason: 'code is required' }
  }
  if (req.code.length > 512_000) {
    return { ok: false, reason: 'code exceeds 512KB product cap' }
  }

  const warnings: string[] = []
  for (const { re, reason } of BANNED_CODE_PATTERNS) {
    if (re.test(req.code)) {
      return { ok: false, reason }
    }
  }
  if (/https?:\/\//i.test(req.code)) {
    warnings.push('code references URLs — live network requires admitted kernel policy')
  }
  if (/\bopen\s*\(/i.test(req.code) || /\bread_csv\s*\(/i.test(req.code)) {
    warnings.push('file I/O should target registered artifacts only')
  }

  return {
    planId: randomUUID(),
    cellId: req.cellId || randomUUID(),
    language: req.language,
    codeHash: hashNotebookCode(req.code),
    codeLength: req.code.length,
    dryRun: Boolean(req.dryRun),
    tool: 'workflow_execute',
    authority: 'SessionActor/KernelAdapter',
    requiresAdmittedKernel: true,
    warnings,
    createdAt: Date.now(),
  }
}

/**
 * Gate live execute: requires trusted session + non-dry-run plan.
 */
export function assertNotebookExecuteAccess(
  plan: NotebookCellPlan,
  trusted: TrustedPreviewContext | null,
): AccessResult {
  if (plan.dryRun) {
    return { ok: false, reason: 'dry-run plan cannot be executed live' }
  }
  if (!trusted?.ownerId || !trusted?.projectId) {
    return {
      ok: false,
      reason: 'no trusted session — open a project before notebook execute',
    }
  }
  if (plan.tool !== 'workflow_execute') {
    return { ok: false, reason: 'unknown notebook tool' }
  }
  if (plan.authority !== 'SessionActor/KernelAdapter') {
    return { ok: false, reason: 'invalid authority claim' }
  }
  return { ok: true }
}

/**
 * Minimal IPYNB-shaped export from in-memory run records (UI/history only).
 * Does not claim Rust artifact registration.
 */
export type NotebookHistoryCell = {
  cellId: string
  language: NotebookLanguage
  source: string
  stdout?: string
  stderr?: string
  ok?: boolean
  codeHash?: string
  planId?: string
  executedAt?: number
  dryRun?: boolean
}

export function exportHistoryToIpynb(opts: {
  projectId: string
  sessionLabel?: string
  cells: NotebookHistoryCell[]
}): Record<string, unknown> {
  const cells = opts.cells.map((c, i) => {
    const outputs: unknown[] = []
    if (c.stdout) {
      outputs.push({
        output_type: 'stream',
        name: 'stdout',
        text: c.stdout.endsWith('\n') ? c.stdout : c.stdout + '\n',
      })
    }
    if (c.stderr) {
      outputs.push({
        output_type: 'stream',
        name: 'stderr',
        text: c.stderr.endsWith('\n') ? c.stderr : c.stderr + '\n',
      })
    }
    if (c.ok === false && !c.stderr) {
      outputs.push({
        output_type: 'error',
        ename: 'CellError',
        evalue: 'cell failed',
        traceback: [],
      })
    }
    return {
      cell_type: 'code',
      execution_count: c.executedAt ? i + 1 : null,
      id: c.cellId,
      metadata: {
        lumen: {
          planId: c.planId,
          codeHash: c.codeHash,
          dryRun: c.dryRun,
          authority: 'SessionActor/KernelAdapter',
          language: c.language,
        },
      },
      outputs,
      source: c.source.split(/(?<=\n)/),
    }
  })

  return {
    nbformat: 4,
    nbformat_minor: 5,
    metadata: {
      kernelspec: {
        display_name: 'Python 3 (Lumen)',
        language: 'python',
        name: 'python3',
      },
      language_info: { name: 'python' },
      lumen: {
        projectId: opts.projectId,
        sessionLabel: opts.sessionLabel ?? 'research',
        authority: 'export-only-ui-history',
        note: 'Execution authority is Rust SessionActor; this file is a projection',
      },
    },
    cells,
  }
}
