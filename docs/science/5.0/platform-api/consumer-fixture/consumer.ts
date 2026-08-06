/**
 * X-C1 consumer compile fixture — typed construction of every v1-baseline
 * ACP science request against the pinned canonical tuple. Compiling this file
 * with --strict proves the consumer-side contract typechecks; it is never
 * executed against a live engine (the live seam negatives live in
 * packs/science-desktop/scripts/test-platform-api-live.mts).
 */

/** Pinned canonical lumen tuple (X-U current pin, 2026-08-06). */
export const LUMEN_PIN = {
  sourceCommitA: '098f7cd424c1015bfe0d1cbd88c96570b36064ca',
  evidenceCommitB: 'af2857a21ec4b1c5655d308231b89ba81596c4fd',
  tag: 'v2.2.0',
  binarySha256: 'f1aa406131f9db4b30f636770386738b289a5bcd65a695e7aac912f699073564',
} as const

type SessionOwner = { sessionId: string; ownerId: string }

/** run_csv: actor-gated deterministic CSV analysis. */
export type RunCsvRequest = SessionOwner & {
  projectId: string
  storeRoot: string
  artifactRoot: string
  fixturePath: string
  approvalTimeoutMs?: number
}

/** import_preview: read-only bounded source preview. */
export type ImportPreviewRequest = SessionOwner & {
  projectId: string
  storeRoot: string
  artifactRoot: string
  sourcePath: string
  approvalTimeoutMs?: number
}

/** connector_fetch: actor-gated fixture connector query. */
export type ConnectorFetchRequest = SessionOwner & {
  projectId: string
  storeRoot: string
  artifactRoot: string
  connectorId: string
  query: string
  fixturePaths: string[]
  maxResults?: number
  approvalTimeoutMs?: number
}

/** ssh_scp_fixture: actor-gated fixture-only transport (live denied). */
export type SshScpFixtureRequest = SessionOwner & {
  projectId: string
  storeRoot: string
  artifactRoot: string
  port: number
  hostKeySha256: string
  user: string
  identityFile: string
  knownHostsFile: string
  sshConfigFile: string
  direction: 'upload' | 'download'
  localPath: string
  remotePath: string
  approvalTimeoutMs?: number
  transportTimeoutMs?: number
  cancelAfterMs?: number
}

/** goal_host_verify: read-only host verification projection. */
export type GoalHostVerifyRequest = {
  sessionId: string
  storeRoot: string
  runId: string
}

/** The seven v1-baseline method names (catalog v1). */
export const V1_METHODS = [
  'x.ai/science/run_csv',
  'x.ai/science/import_preview',
  'x.ai/science/connector_fetch',
  'x.ai/science/ssh_scp_fixture',
  'x.ai/science/goal_host_verify',
  'x.ai/governedTree/status',
  'x.ai/governedTree/assignmentRecommendation',
] as const

export type V1MethodName = (typeof V1_METHODS)[number]

/**
 * Typed wire envelope: consumer never sends a method absent from V1_METHODS
 * and never adds a field absent from the per-method request type.
 */
export type V1Request =
  | { method: 'x.ai/science/run_csv'; params: RunCsvRequest }
  | { method: 'x.ai/science/import_preview'; params: ImportPreviewRequest }
  | { method: 'x.ai/science/connector_fetch'; params: ConnectorFetchRequest }
  | { method: 'x.ai/science/ssh_scp_fixture'; params: SshScpFixtureRequest }
  | { method: 'x.ai/science/goal_host_verify'; params: GoalHostVerifyRequest }
  | { method: 'x.ai/governedTree/status'; params: Record<string, never> }
  | { method: 'x.ai/governedTree/assignmentRecommendation'; params: Record<string, never> }

/** Sample requests for every v1 method — compile-time contract witnesses. */
export const SAMPLE_REQUESTS: V1Request[] = [
  {
    method: 'x.ai/science/run_csv',
    params: {
      sessionId: 's',
      ownerId: 'o',
      projectId: 'p',
      storeRoot: '/store',
      artifactRoot: '/store/runs',
      fixturePath: '/fixtures/a.csv',
    },
  },
  {
    method: 'x.ai/science/import_preview',
    params: {
      sessionId: 's',
      ownerId: 'o',
      projectId: 'p',
      storeRoot: '/store',
      artifactRoot: '/store/runs',
      sourcePath: '/sources/paper.pdf',
    },
  },
  {
    method: 'x.ai/science/connector_fetch',
    params: {
      sessionId: 's',
      ownerId: 'o',
      projectId: 'p',
      storeRoot: '/store',
      artifactRoot: '/store/runs',
      connectorId: 'uniprot',
      query: 'P12345',
      fixturePaths: ['/fixtures/uniprot.json'],
    },
  },
  {
    method: 'x.ai/science/ssh_scp_fixture',
    params: {
      sessionId: 's',
      ownerId: 'o',
      projectId: 'p',
      storeRoot: '/store',
      artifactRoot: '/store/runs',
      port: 2222,
      hostKeySha256: 'aa',
      user: 'u',
      identityFile: '/keys/id',
      knownHostsFile: '/keys/known',
      sshConfigFile: '/keys/conf',
      direction: 'upload',
      localPath: '/data/x.txt',
      remotePath: '/remote/x.txt',
    },
  },
  {
    method: 'x.ai/science/goal_host_verify',
    params: { sessionId: 's', storeRoot: '/store', runId: 'r1' },
  },
  { method: 'x.ai/governedTree/status', params: {} },
  { method: 'x.ai/governedTree/assignmentRecommendation', params: {} },
]

/** A consumer must reject a request whose method is not in V1_METHODS. */
export function isV1Method(method: string): method is V1MethodName {
  return (V1_METHODS as readonly string[]).includes(method)
}
