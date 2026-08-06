// S0-B: typed fail-closed contract for the shipping skill-mutation IPC
// channels. The governed Skill Revision API (book X-M1 / M1-Skill) is not
// shipped yet, so create/update/delete/enable must fail closed at the IPC
// boundary instead of reaching the legacy mutable store. Read-only surfaces
// (list / detail / ZIP preview / quarantine import) stay untouched.
export const SKILL_AUTHORITY_UNAVAILABLE = 'SKILL_AUTHORITY_UNAVAILABLE'

export const SKILL_AUTHORITY_MIGRATION_REQUIRED =
  'Skill mutation is disabled until the governed Skill Revision API ships (X-M1). ' +
  'Read-only browse, preview, and the actor-gated ZIP quarantine import stay available; ' +
  'existing user skills remain readable and exportable.'

/** Detect the typed fail-closed outcome on the renderer side (Electron keeps Error.message). */
export function isSkillAuthorityUnavailable(error: unknown): boolean {
  return (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof (error as { message: unknown }).message === 'string' &&
    (error as { message: string }).message.startsWith(SKILL_AUTHORITY_UNAVAILABLE)
  )
}

/** Build the renderer-visible typed error for a shipping mutation channel. */
export function skillAuthorityError(): Error {
  return new Error(`${SKILL_AUTHORITY_UNAVAILABLE}: ${SKILL_AUTHORITY_MIGRATION_REQUIRED}`)
}
