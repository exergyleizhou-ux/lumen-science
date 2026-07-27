// Update policy — fail closed (LS5-R1-02).
//
// This desktop was adapted from Open Science, which shipped an update feed on
// `statics.aipoch.com`. Inheriting that feed would mean a third party could
// serve code to Lumen users, so the feed is not merely re-pointed — updating is
// off unless Lumen-owned signing material is explicitly configured.
//
// Two independent requirements, both mandatory:
//   1. a feed URL on a Lumen-owned host, over https
//   2. a public key to verify what that feed serves
//
// Neither implies the other: a URL without a key is unverified code, and a key
// without a URL updates nothing. Missing either => Disabled, and the main
// process never constructs an updater or opens a socket.
//
// No Electron/Node imports: main and renderer both consume this.

export type UpdatePolicy =
  | { enabled: false; reason: string }
  | { enabled: true; feedUrl: string; publicKey: string }

// Hosts that must never serve updates to Lumen, regardless of configuration.
// This is a backstop against a re-inherited or mistyped upstream value, not the
// primary control — the primary control is that enabling requires explicit
// Lumen-owned config.
export const FORBIDDEN_UPDATE_HOSTS: readonly string[] = [
  'aipoch.com',
  'statics.aipoch.com',
  'www.aipoch.com'
]

// Only these hosts may serve updates. Kept explicit rather than "any https"
// so a compromised or mistaken env var cannot redirect the updater anywhere.
export const ALLOWED_UPDATE_HOSTS: readonly string[] = [
  'github.com',
  'objects.githubusercontent.com',
  'releases.lumen.science'
]

const isForbidden = (hostname: string): boolean =>
  FORBIDDEN_UPDATE_HOSTS.some((h) => hostname === h || hostname.endsWith(`.${h}`))

const isAllowed = (hostname: string): boolean =>
  ALLOWED_UPDATE_HOSTS.some((h) => hostname === h || hostname.endsWith(`.${h}`))

/**
 * The environment this policy is read from. Only the two LUMEN_* variables are meaningful; they are
 * named explicitly so the contract is self-documenting and a typo in a test fixture is caught.
 *
 * The index signature is what makes `resolveUpdatePolicy(process.env)` — how all three real callers
 * invoke it — legal. Without it this is a "weak type" (every property optional), and NodeJS.ProcessEnv
 * declares none of its properties, so TypeScript rejects the call outright even though the value is
 * exactly what the function is designed to read. The signature states the truth: any other variable
 * may be present and is ignored.
 */
export type UpdatePolicyEnv = {
  LUMEN_UPDATE_FEED_URL?: string
  LUMEN_UPDATE_PUBLIC_KEY?: string
  [key: string]: string | undefined
}

/**
 * Resolve the update policy from environment configuration.
 *
 * Every rejection path returns a reason, because "updates are off" is a state
 * a user is entitled to see explained rather than a silent no-op.
 */
export const resolveUpdatePolicy = (env: UpdatePolicyEnv = {}): UpdatePolicy => {
  const feedUrl = env.LUMEN_UPDATE_FEED_URL?.trim()
  const publicKey = env.LUMEN_UPDATE_PUBLIC_KEY?.trim()

  if (!feedUrl && !publicKey) {
    return {
      enabled: false,
      reason:
        'no Lumen-owned update feed configured (set LUMEN_UPDATE_FEED_URL and LUMEN_UPDATE_PUBLIC_KEY)'
    }
  }
  if (!feedUrl) {
    return { enabled: false, reason: 'LUMEN_UPDATE_PUBLIC_KEY set without LUMEN_UPDATE_FEED_URL' }
  }
  if (!publicKey) {
    return {
      enabled: false,
      reason:
        'LUMEN_UPDATE_FEED_URL set without LUMEN_UPDATE_PUBLIC_KEY — refusing to fetch updates that cannot be verified'
    }
  }

  let parsed: URL
  try {
    parsed = new URL(feedUrl)
  } catch {
    return { enabled: false, reason: `LUMEN_UPDATE_FEED_URL is not a valid URL: ${feedUrl}` }
  }

  if (parsed.protocol !== 'https:') {
    return { enabled: false, reason: `update feed must use https, got ${parsed.protocol}` }
  }
  if (isForbidden(parsed.hostname)) {
    return {
      enabled: false,
      reason: `refusing third-party update host ${parsed.hostname} — Lumen does not accept updates from upstream infrastructure`
    }
  }
  if (!isAllowed(parsed.hostname)) {
    return {
      enabled: false,
      reason: `update host ${parsed.hostname} is not a Lumen-owned release host`
    }
  }

  return { enabled: true, feedUrl, publicKey }
}

/**
 * Feed URL for a strategy that is about to do network I/O.
 *
 * Throws when updating is disabled. Previously these call sites fell back to a
 * hardcoded third-party URL, so a missing configuration silently produced a
 * working updater pointed at someone else's infrastructure. Now a networked
 * strategy simply cannot be constructed without an explicit Lumen-owned feed.
 */
export const requireUpdateFeedUrl = (env: UpdatePolicyEnv = {}): string => {
  const policy = resolveUpdatePolicy(env)
  if (!policy.enabled) {
    throw new Error(
      `refusing to construct a networked update strategy: ${policy.reason}`
    )
  }
  return policy.feedUrl
}
