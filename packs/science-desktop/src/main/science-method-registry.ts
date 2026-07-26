/**
 * Science ACP method registry — the allowlist of methods that actually exist.
 *
 * The desktop used to POST arbitrary tool names at a fictional HTTP endpoint,
 * so a name nothing implements was indistinguishable from a name that simply
 * failed to connect. Both produced ECONNREFUSED. That made three invented
 * names (`project_assert_membership`, `artifact_resolve`, `compute_plan`) look
 * like transport problems for as long as the transport was broken.
 *
 * The real science surface is the 24 `x.ai/science/*` ACP extension methods
 * dispatched by
 * agent/crates/codegen/xai-grok-shell/src/extensions/science.rs:109-139.
 * This module is the single place that decides whether a name may go on the
 * wire, and it is a closed allowlist: anything else is rejected here rather
 * than attempted and misreported.
 *
 * WIRE FORM. agent-client-protocol 0.10.4 routes a JSON-RPC method to an
 * agent's `ext_method` only when it starts with `_`, which it then strips
 * (see the crate's `AgentRequest::decode`, the `strip_prefix('_')` arm).
 * So `x.ai/science/project_list` travels as `_x.ai/science/project_list`.
 * Sending the unprefixed name gets -32601 Method not found — verified against
 * the built binary, which is how this prefix was found.
 *
 * Electron-free by construction so it can be executed by the authority tests.
 */

/** ACP extension namespace for every science method. */
export const SCIENCE_METHOD_NAMESPACE = 'x.ai/science/'

/**
 * Prefix agent-client-protocol requires for a method to reach `ext_method`.
 * It is stripped by the agent before dispatch, so it is transport framing —
 * never part of a method's identity.
 */
export const ACP_EXT_WIRE_PREFIX = '_'

/**
 * The 24 methods the Rust engine dispatches. Order and spelling mirror
 * extensions/science.rs; a name absent from that match arm must be absent
 * here, or the desktop would claim a capability the engine does not have.
 */
export const SCIENCE_METHODS = [
  'run_csv',
  'import_preview',
  'connector_fetch',
  'ssh_scp_fixture',
  'goal_host_verify',
  'seq_analyze',
  'project_create',
  'project_get',
  'project_list',
  'project_transition',
  'claim_propose',
  'evidence_attach',
  'evidence_trace',
  'evidence_compare',
  'evidence_consistency',
  'evidence_reproduction',
  'project_migrate',
  'workflow_validate',
  'workflow_dry_run',
  'kernel_admission',
  'multimodal_index',
  'review_record',
  'collaboration_invite',
  'remote_compute_plan',
] as const

export type ScienceMethodName = (typeof SCIENCE_METHODS)[number]

const ALLOWED = new Set<string>(SCIENCE_METHODS)

/**
 * Names the desktop sends that exist in NEITHER engine — not in the Rust ACP
 * dispatch table and not among the Go MCP tools. They were written against an
 * HTTP `/tools/call` endpoint that never existed, so nothing ever rejected
 * them. Each entry records the call site so the rejection names its own fix.
 */
const NONEXISTENT_METHODS: Record<string, string> = {
  project_assert_membership:
    'no such method in either engine; files/acp-membership.ts invented it. ' +
    'Membership has no ACP method yet — the local catalog path in ' +
    'files/hybrid-membership.ts is the only real answer today.',
  artifact_resolve:
    'no such method in either engine; files/acp-preview-store.ts invented it. ' +
    'Previews must be seeded via put() from a real listing.',
  compute_plan:
    'no such method in either engine; files/compute-service.ts invented it. ' +
    'The nearest real method is remote_compute_plan, which takes different ' +
    'params — wiring it is a separate change, not a rename.',
}

/**
 * Names that are real, but belong to the Go MCP tool surface
 * (packs/science/standalone), not the Rust ACP extension surface. Routing
 * them at `_x.ai/science/*` would produce -32601 from the Rust agent and
 * invite the conclusion that the tool does not exist at all. They are
 * rejected here with the distinction spelled out.
 */
const GO_MCP_TOOLS: Record<string, string> = {
  artifact_list: 'Go MCP tool, not a Rust ACP extension method',
  notebook_execute: 'Go MCP tool, not a Rust ACP extension method',
  start_review: 'Go MCP tool, not a Rust ACP extension method',
}

/** Thrown for any name the registry refuses to put on the wire. */
export class UnknownScienceMethodError extends Error {
  readonly code = 'LUMEN_METHOD_NOT_ALLOWED'
  readonly method: string

  constructor(method: string, detail: string) {
    super(`science method '${method}' rejected by registry: ${detail}`)
    this.name = 'UnknownScienceMethodError'
    this.method = method
  }
}

/**
 * Strip the namespace and any wire prefix so `project_list`,
 * `x.ai/science/project_list` and `_x.ai/science/project_list` are one name.
 * Callers in this pack use the bare form; the ledger and logs use the full one.
 */
function normalize(name: string): string {
  let out = name.trim()
  while (out.startsWith(ACP_EXT_WIRE_PREFIX)) out = out.slice(ACP_EXT_WIRE_PREFIX.length)
  if (out.startsWith(SCIENCE_METHOD_NAMESPACE)) {
    out = out.slice(SCIENCE_METHOD_NAMESPACE.length)
  }
  return out
}

export function isScienceMethod(name: string): name is ScienceMethodName {
  return ALLOWED.has(normalize(name))
}

export type ResolvedScienceMethod = {
  /** Bare name, e.g. `project_list`. */
  name: ScienceMethodName
  /** Fully-qualified ACP method, e.g. `x.ai/science/project_list`. */
  qualified: string
  /** What actually goes in the JSON-RPC `method` field. */
  wireMethod: string
}

/**
 * Resolve a caller-supplied name to its wire form, or throw.
 *
 * Fail-closed is the entire point: an unknown name must not reach the child
 * process, because a -32601 from the engine and a name this pack made up are
 * different bugs with different fixes.
 */
export function resolveScienceMethod(name: unknown): ResolvedScienceMethod {
  if (typeof name !== 'string' || name.trim() === '') {
    return failWith(String(name), 'method name must be a non-empty string')
  }
  const bare = normalize(name)
  if (ALLOWED.has(bare)) {
    const qualified = `${SCIENCE_METHOD_NAMESPACE}${bare}`
    return {
      name: bare as ScienceMethodName,
      qualified,
      wireMethod: `${ACP_EXT_WIRE_PREFIX}${qualified}`,
    }
  }
  const invented = NONEXISTENT_METHODS[bare]
  if (invented) return failWith(bare, invented)
  const goTool = GO_MCP_TOOLS[bare]
  if (goTool) {
    return failWith(
      bare,
      `${goTool}. The Rust engine dispatches only ${SCIENCE_METHOD_NAMESPACE}* ` +
        '(extensions/science.rs); this call site needs the Go MCP client, not this bridge.',
    )
  }
  return failWith(
    bare,
    `not one of the ${SCIENCE_METHODS.length} methods dispatched by the Rust engine`,
  )
}

function failWith(name: string, detail: string): never {
  throw new UnknownScienceMethodError(name, detail)
}

/** Every allowed method with its wire form — used to answer tool listings. */
export function listScienceMethods(): ResolvedScienceMethod[] {
  return SCIENCE_METHODS.map((name) => {
    const qualified = `${SCIENCE_METHOD_NAMESPACE}${name}`
    return { name, qualified, wireMethod: `${ACP_EXT_WIRE_PREFIX}${qualified}` }
  })
}

/**
 * Why a name is refused, without throwing. For diagnostics surfaces that want
 * to explain the whole rejected set rather than fail on the first one.
 */
export function explainRejection(name: string): string | null {
  try {
    resolveScienceMethod(name)
    return null
  } catch (error: unknown) {
    return error instanceof UnknownScienceMethodError ? error.message : String(error)
  }
}
