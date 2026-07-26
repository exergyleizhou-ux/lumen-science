/**
 * OSF-6 Remote Compute product service.
 *
 * Plans only (dry-run). Optional ACP call for plan registration.
 * Never constructs SystemSshRunner / SystemScpRunner / JobDispatcher.
 */

import {
  planRemoteCompute,
  assertComputePlanAccess,
  rejectDesktopLiveExecute,
  type ComputePlanRequest,
  type ComputePlan,
} from './compute-plan'
import { getTrustedPreviewContext } from './session-identity'

export type AcpComputeCall = (
  toolName: string,
  args: Record<string, unknown>,
) => Promise<unknown>

export type ComputeService = {
  plan: (req: ComputePlanRequest) => unknown
  /** Register plan hash via ACP (still dry-run); never live SSH */
  submitPlan: (req: ComputePlanRequest) => Promise<unknown>
  /** Always denied at desktop */
  executeLive: (planId: string) => unknown
  history: () => ComputePlan[]
  clear: () => void
}

export function createComputeService(opts: {
  acpCall?: AcpComputeCall
}): ComputeService {
  const history: ComputePlan[] = []

  return {
    plan(req) {
      const trusted = getTrustedPreviewContext()
      if (!trusted) {
        return { ok: false, reason: 'no trusted session — open a project before compute plan' }
      }
      const planned = planRemoteCompute(req)
      if ('ok' in planned && planned.ok === false) return planned
      const plan = planned as ComputePlan
      const access = assertComputePlanAccess(plan, trusted)
      if (!access.ok) return { ok: false, reason: access.reason, plan }
      history.push(plan)
      return {
        ok: true,
        plan,
        authority: 'SessionActor/ToolAdapter',
        note: 'dry-run only',
      }
    },

    async submitPlan(req) {
      const plannedResult = this.plan(req)
      if (!(plannedResult as { ok?: boolean }).ok) return plannedResult
      const plan = (plannedResult as { plan: ComputePlan }).plan

      if (!opts.acpCall) {
        return {
          ok: true,
          plan,
          registered: false,
          note: 'plan held locally — no ACP caller for registration',
        }
      }

      try {
        const trusted = getTrustedPreviewContext()!
        const raw = await opts.acpCall('compute_plan', {
          project_id: trusted.projectId,
          hostname: plan.hostname,
          plan_id: plan.planId,
          plan_hash: plan.planHash,
          target_kind: plan.targetKind,
          jobs: plan.jobs,
          dry_run: true,
        })
        return {
          ok: true,
          plan,
          registered: true,
          acpResult: raw,
          authority: 'SessionActor/ToolAdapter',
        }
      } catch (e: unknown) {
        return {
          ok: false,
          reason: (e as Error).message || String(e),
          plan,
        }
      }
    },

    executeLive(_planId: string) {
      return rejectDesktopLiveExecute()
    },

    history() {
      return [...history]
    },

    clear() {
      history.length = 0
    },
  }
}
