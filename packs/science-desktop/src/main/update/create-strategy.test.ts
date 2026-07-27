// Modified from Open Science (Apache-2.0).
// Upstream: https://github.com/aipoch/open-science @ d8f11e34314f
// Change: Configures a Lumen-owned update feed; the hardened policy refuses to construct a networked strategy without one.
// Per-file diff and digests: docs/provenance/open-science-adoption.json
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest'

// createUpdateStrategy constructs a concrete strategy per platform. Both strategies touch native
// modules at construction (UpdateService reads app.getVersion(); ElectronUpdaterStrategy subscribes to
// autoUpdater), so stub them enough to instantiate without a real Electron runtime.
vi.mock('electron', () => ({
  app: { getVersion: () => '0.0.0', isPackaged: false },
  BrowserWindow: { getAllWindows: () => [] }
}))
vi.mock('electron-updater', () => ({
  autoUpdater: { on: () => {}, autoDownload: true, autoInstallOnAppQuit: true }
}))

import { createUpdateStrategy } from './create-strategy'
import { ElectronUpdaterStrategy } from './electron-updater-strategy'
import { UpdateService } from './service'


// The hardened update policy refuses to construct a networked strategy without
// an explicit Lumen-owned feed (update-policy.ts) — the fallback these tests
// relied on used to be a hardcoded third-party URL, which is exactly what the
// hardening removed. The suite configures a syntactically valid Lumen feed so
// it can test the strategy MECHANICS; the refusal paths have their own
// coverage in scripts/test-update-egress.mts.
beforeAll(() => {
  process.env.LUMEN_UPDATE_FEED_URL = 'https://releases.lumen.science/desktop/manifest.json'
  process.env.LUMEN_UPDATE_PUBLIC_KEY = 'RWTest0000000000000000000000000000000000000000000000000000'
})
afterAll(() => {
  delete process.env.LUMEN_UPDATE_FEED_URL
  delete process.env.LUMEN_UPDATE_PUBLIC_KEY
})

describe('createUpdateStrategy', () => {
  it('uses ElectronUpdaterStrategy on win32', () => {
    expect(createUpdateStrategy('win32')).toBeInstanceOf(ElectronUpdaterStrategy)
  })

  it('uses ElectronUpdaterStrategy on linux', () => {
    expect(createUpdateStrategy('linux')).toBeInstanceOf(ElectronUpdaterStrategy)
  })

  it('uses ElectronUpdaterStrategy on darwin for a packaged stable build', () => {
    expect(createUpdateStrategy('darwin', { isPackaged: true, version: '1.2.3' })).toBeInstanceOf(
      ElectronUpdaterStrategy
    )
  })

  it('falls back to UpdateService on darwin for a nightly (prerelease) build', () => {
    expect(
      createUpdateStrategy('darwin', { isPackaged: true, version: '1.2.3-nightly.abc1234' })
    ).toBeInstanceOf(UpdateService)
  })

  it('falls back to UpdateService on darwin for an unpackaged (dev) build', () => {
    expect(createUpdateStrategy('darwin', { isPackaged: false, version: '1.2.3' })).toBeInstanceOf(
      UpdateService
    )
  })
})
