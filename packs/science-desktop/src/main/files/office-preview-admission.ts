/**
 * Office Preview admission gate (OSF product path).
 *
 * UI may open isolated office preview only when converter admission is complete.
 * Fail-closed: missing hostile-doc tests / license / no-network → deny open.
 * Does not claim converters are certified for medical use.
 */

export type OfficeFormat = 'docx' | 'xlsx' | 'pptx' | 'pdf' | 'unknown'

export type ConverterAdmission = {
  format: OfficeFormat
  converterId: string
  dependency: string
  version: string
  license: string
  /** Max input bytes */
  inputCapBytes: number
  timeoutMs: number
  noNetwork: boolean
  /** Hostile document corpus tests recorded */
  hostileDocTestsPass: boolean
  /** Converter provenance recorded */
  provenanceRecorded: boolean
  admitted: boolean
  notes?: string
}

export type OfficePreviewOpenRequest = {
  format: OfficeFormat
  /** Must be artifact-bound when from science path */
  artifactId?: string
  expectedSha256?: string
  /** Path only allowed after admission + artifact gate in product path */
  pathHint?: string
}

const DENIED_LICENSES = new Set(['gpl', 'agpl', 'unknown', ''])

/**
 * Built-in admission table for desktop product path.
 * Defaults are fail-closed until hostile-doc evidence is recorded in-repo.
 */
export const OFFICE_ADMISSION_TABLE: ConverterAdmission[] = [
  {
    format: 'docx',
    converterId: 'office-docx-isolated',
    dependency: 'isolated-office-renderer',
    version: 'pending-audit',
    license: 'Apache-2.0',
    inputCapBytes: 25 * 1024 * 1024,
    timeoutMs: 30_000,
    noNetwork: true,
    hostileDocTestsPass: false, // fail-closed until suite lands
    provenanceRecorded: true,
    admitted: false,
    notes: 'UI scaffold allowed; open denied until hostile-doc suite passes',
  },
  {
    format: 'xlsx',
    converterId: 'office-xlsx-isolated',
    dependency: 'isolated-office-renderer',
    version: 'pending-audit',
    license: 'Apache-2.0',
    inputCapBytes: 25 * 1024 * 1024,
    timeoutMs: 30_000,
    noNetwork: true,
    hostileDocTestsPass: false,
    provenanceRecorded: true,
    admitted: false,
  },
  {
    format: 'pptx',
    converterId: 'office-pptx-isolated',
    dependency: 'isolated-office-renderer',
    version: 'pending-audit',
    license: 'Apache-2.0',
    inputCapBytes: 40 * 1024 * 1024,
    timeoutMs: 45_000,
    noNetwork: true,
    hostileDocTestsPass: false,
    provenanceRecorded: true,
    admitted: false,
  },
  {
    format: 'pdf',
    converterId: 'pdfjs-legacy',
    dependency: 'pdfjs-dist',
    version: 'bundled',
    license: 'Apache-2.0',
    inputCapBytes: 50 * 1024 * 1024,
    timeoutMs: 30_000,
    noNetwork: true,
    hostileDocTestsPass: false,
    provenanceRecorded: true,
    admitted: false,
  },
]

export function getAdmission(format: OfficeFormat): ConverterAdmission | undefined {
  return OFFICE_ADMISSION_TABLE.find((a) => a.format === format)
}

/**
 * Evaluate whether a converter may open content.
 */
export function assertOfficePreviewAdmission(
  req: OfficePreviewOpenRequest,
  table: ConverterAdmission[] = OFFICE_ADMISSION_TABLE,
): { ok: true; admission: ConverterAdmission } | { ok: false; reason: string } {
  if (!req.format || req.format === 'unknown') {
    return { ok: false, reason: 'unknown office format' }
  }
  const admission = table.find((a) => a.format === req.format)
  if (!admission) {
    return { ok: false, reason: `no admission row for format ${req.format}` }
  }
  if (DENIED_LICENSES.has(admission.license.toLowerCase())) {
    return { ok: false, reason: `license denied: ${admission.license}` }
  }
  if (!admission.noNetwork) {
    return { ok: false, reason: 'converter must be no-network' }
  }
  if (!admission.hostileDocTestsPass) {
    return {
      ok: false,
      reason: `hostile document tests not passed for ${admission.converterId}`,
    }
  }
  if (!admission.provenanceRecorded) {
    return { ok: false, reason: 'converter provenance not recorded' }
  }
  if (!admission.admitted) {
    return { ok: false, reason: `converter ${admission.converterId} not admitted` }
  }
  // Science path should be artifact-bound
  if (!req.artifactId || !req.expectedSha256) {
    return {
      ok: false,
      reason: 'office preview requires artifact_id + expected_sha256',
    }
  }
  if (req.expectedSha256.length < 16) {
    return { ok: false, reason: 'expected_sha256 too short' }
  }
  return { ok: true, admission }
}

/**
 * List admission rows for UI honesty (what is blocked and why).
 */
export function listOfficeAdmissions(
  table: ConverterAdmission[] = OFFICE_ADMISSION_TABLE,
): ConverterAdmission[] {
  return table.map((a) => ({ ...a }))
}
