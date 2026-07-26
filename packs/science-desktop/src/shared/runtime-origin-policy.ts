// Runtime download origin policy — fail closed (LS5-K4).
//
// Sibling of update-policy.ts, for the second thing Open Science downloaded and
// then executed: Python/R runtime bundles. Upstream's runtime-paths.ts shipped a
// `DEFAULT_RUNTIME_CDN_BASE` constant naming their own CDN host, so an unset
// environment variable and "configured to fetch a third party's interpreter"
// were the same state at run time. LS5-R1-02 removed the default; this module
// removes the remaining hole, which is that the *replacement* was a bare env
// var — nothing stopped LUMEN_RUNTIME_CDN_BASE from being set to the forbidden
// host, putting it straight back.
//
// The rule is the same one the updater got, for the same reason: code that will
// be executed on a user's machine may only come from a host Lumen owns, and the
// absence of configuration means "off", never "use someone else's".
//
// The forbidden list is IMPORTED from update-policy rather than restated. Two
// copies of a denylist drift, and the host that must not serve us an installer
// is exactly the host that must not serve us an interpreter.
//
// No Electron/Node imports: main, renderer and the authority scripts all
// consume this.

import { FORBIDDEN_UPDATE_HOSTS } from './update-policy'

export type RuntimeOriginPolicy =
  | { enabled: false; reason: string }
  | { enabled: true; baseUrl: string }

/**
 * Hosts that must never serve runtime bundles, shared with the updater.
 *
 * Exported under a download-neutral name so a future third download path picks
 * up the same list instead of inventing a third one.
 */
export const FORBIDDEN_DOWNLOAD_HOSTS: readonly string[] = FORBIDDEN_UPDATE_HOSTS

/**
 * Hosts that may serve runtime bundles.
 *
 * Explicit rather than "any https": a mistyped or injected environment
 * variable must not be able to redirect the bundle fetch anywhere. Kept
 * separate from ALLOWED_UPDATE_HOSTS because the two answer different
 * questions — an installer feed and a conda pack store need not be the same
 * infrastructure, and widening one must not silently widen the other.
 */
export const ALLOWED_RUNTIME_HOSTS: readonly string[] = [
  'github.com',
  'objects.githubusercontent.com',
  'releases.lumen.science',
]

/** Environment variable that configures the runtime bundle origin. */
export const RUNTIME_ORIGIN_ENV_VAR = 'LUMEN_RUNTIME_CDN_BASE'

const matches = (hostname: string, list: readonly string[]): boolean =>
  list.some((h) => hostname === h || hostname.endsWith(`.${h}`))

export type RuntimeOriginEnv = {
  LUMEN_RUNTIME_CDN_BASE?: string
  [key: string]: string | undefined
}

/**
 * Classify a candidate origin without consulting the environment.
 *
 * Separated from `resolveRuntimeOriginPolicy` so a URL composed at call time —
 * a manifest URL, a pack URL — can be checked against exactly the same rule as
 * the configured base. A base that passes and a derived URL that does not is a
 * real failure mode: `https://releases.lumen.science` plus an attacker-supplied
 * relative segment can compose an absolute URL on another host.
 */
export const classifyRuntimeOrigin = (candidate: string): RuntimeOriginPolicy => {
  const trimmed = candidate.trim()
  if (!trimmed) {
    return { enabled: false, reason: 'runtime origin is empty' }
  }

  let parsed: URL
  try {
    parsed = new URL(trimmed)
  } catch {
    return { enabled: false, reason: `runtime origin is not a valid URL: ${trimmed}` }
  }

  if (parsed.protocol !== 'https:') {
    return {
      enabled: false,
      reason: `runtime bundles must be fetched over https, got ${parsed.protocol}`,
    }
  }
  if (matches(parsed.hostname, FORBIDDEN_DOWNLOAD_HOSTS)) {
    return {
      enabled: false,
      reason:
        `refusing third-party runtime host ${parsed.hostname} — Lumen does not execute ` +
        'interpreters served by upstream infrastructure',
    }
  }
  if (!matches(parsed.hostname, ALLOWED_RUNTIME_HOSTS)) {
    return {
      enabled: false,
      reason: `runtime host ${parsed.hostname} is not a Lumen-owned bundle host`,
    }
  }
  return { enabled: true, baseUrl: trimmed.replace(/\/+$/, '') }
}

/**
 * Resolve the runtime origin from configuration.
 *
 * With nothing configured this returns disabled with a reason, and no caller
 * may construct a URL. That is the intended default: the desktop identifies
 * interpreters that already exist on the machine, and needs no download origin
 * to do it.
 */
export const resolveRuntimeOriginPolicy = (
  env: RuntimeOriginEnv = {},
  override?: string,
): RuntimeOriginPolicy => {
  const configured = override?.trim() || env[RUNTIME_ORIGIN_ENV_VAR]?.trim()
  if (!configured) {
    return {
      enabled: false,
      reason:
        `no Lumen-owned runtime bundle origin configured (set ${RUNTIME_ORIGIN_ENV_VAR} to ` +
        `one of: ${ALLOWED_RUNTIME_HOSTS.join(', ')})`,
    }
  }
  return classifyRuntimeOrigin(configured)
}

/**
 * Base URL for a caller that is about to do network I/O, or throw.
 *
 * Throwing is the point. The previous shape returned a string whose absence was
 * papered over by a hardcoded upstream default, so a missing configuration
 * produced a working downloader aimed at someone else's server.
 */
export const requireRuntimeOriginBase = (
  env: RuntimeOriginEnv = {},
  override?: string,
): string => {
  const policy = resolveRuntimeOriginPolicy(env, override)
  if (!policy.enabled) {
    throw new Error(`refusing to fetch a runtime bundle: ${policy.reason}`)
  }
  return policy.baseUrl
}

/**
 * Assert that a fully-composed URL still lands on an allowed host, and return it.
 *
 * Called after string composition, not before: `new URL('https://evil.test/x', base)`
 * and a base segment that starts with `//` both produce a URL on a host the
 * configured base never named.
 */
export const assertRuntimeOriginUrl = (url: string): string => {
  const policy = classifyRuntimeOrigin(url)
  if (!policy.enabled) {
    throw new Error(`refusing runtime bundle URL: ${policy.reason}`)
  }
  return url
}
