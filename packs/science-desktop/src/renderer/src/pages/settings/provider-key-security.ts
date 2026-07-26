// Modified from Open Science (Apache-2.0).
// Upstream: https://github.com/aipoch/open-science @ d8f11e34314f
// Change: product name in user-visible strings. The upstream project name is
// retained in this notice, as the licence requires; only text the application
// displays was changed.
// Per-file diff and digests: docs/provenance/open-science-adoption.json
type ApiKeySecurityCopy = {
  title: string
  description: string
}

// Keeps the security promise aligned with the fail-closed safeStorage boundary.
const getApiKeySecurityCopy = (encryptionAvailable: boolean): ApiKeySecurityCopy =>
  encryptionAvailable
    ? {
        title: 'Your key stays private.',
        description:
          'It is stored only on this device and never uploaded to Lumen Science. Your OS secure storage protects it, and it is sent only to the selected provider when you make a request.'
      }
    : {
        title: 'Secure storage is unavailable.',
        description:
          'Lumen Science will not save API keys until the operating-system credential vault is available. Unlock or authorize the system keychain, then retry.'
      }

export { getApiKeySecurityCopy }
