/**
 * Headed Electron end-to-end tests.
 *
 * Every other suite in this pack checks a module in isolation. None of them has
 * ever launched the application, so nothing proved the pieces are wired
 * together — a product can pass 400 unit assertions and still fail to open a
 * window, and this pack shipped a branding shell for months while its tests
 * were green.
 *
 * These launch the REAL built app from `out/`, drive the actual renderer, and
 * assert on what a user would see.
 *
 * Deliberately NOT mocked: no fake IPC, no stubbed bridge. Where the engine is
 * absent the tests assert the app FAILS CLOSED and says so, because that is the
 * honest behaviour to guarantee — an app that silently degrades to a working
 * -looking state without an engine is the defect, not the test's problem.
 *
 *   npm run test:e2e
 */
import { test, expect, _electron as electron, type ElectronApplication, type Page } from '@playwright/test'
import path from 'node:path'

// Playwright resolves testDir against the config's directory and runs from the
// package root, so cwd IS the pack. import.meta is unavailable under its
// transpile target.
const PACK = process.cwd()

let app: ElectronApplication
let page: Page

test.beforeAll(async () => {
  app = await electron.launch({
    args: [path.join(PACK, 'out/main/index.js')],
    cwd: PACK,
    env: {
      ...process.env,
      // Deterministic, and isolated from a developer's real data.
      LUMEN_DESKTOP_OWNER_ID: 'e2e-owner',
      // No engine binary on purpose for the fail-closed assertions below.
      LUMEN_BINARY: '/nonexistent/lumen',
    },
  })
  page = await app.firstWindow()
  await page.waitForLoadState('load')
  // Wait for the CONDITION, not a fixed sleep: domcontentloaded fires before
  // React mounts, and a sleep long enough today is a flake on a loaded runner.
  // If the app never renders this throws with a clear timeout, which is the
  // failure worth reporting — it is exactly the bug this suite found.
  await page.waitForFunction(
    () => (document.getElementById('root')?.childElementCount ?? 0) > 0,
    undefined,
    { timeout: 30_000 },
  )
})

test.afterAll(async () => {
  await app?.close()
})

test('the app opens a window and actually renders something', async () => {
  // Asserting the branding shell's ABSENCE is not enough: a BLANK page passes
  // that too, and did. The app mounted #root and rendered zero children because
  // four unregistered IPC channels rejected into unhandled page errors. So
  // assert on what is THERE.
  const html = await page.content()
  expect(html).not.toContain('Pack proof shell')

  const rendered = await page.evaluate(() => ({
    rootChildren: document.getElementById('root')?.childElementCount ?? -1,
    textLength: document.body.innerText.trim().length,
  }))
  expect(rendered.rootChildren).toBeGreaterThan(0)
  expect(rendered.textLength).toBeGreaterThan(0)

  const title = await page.title()
  expect(title.length).toBeGreaterThan(0)
})

test('the renderer mounts without an unhandled page error', async () => {
  // The regression that hid for months: a rejected invoke escapes as a page
  // error and React stops rendering. Every unit suite stayed green throughout.
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.reload({ waitUntil: 'load' })
  await page.waitForTimeout(2500)
  expect(errors).toEqual([])
})

test('the first screen is the research desk, not upstream onboarding', async () => {
  // The upstream first-run wizard has five steps, and four configure subsystems
  // this build does not have: environment provisioning, agent-framework choice,
  // notebook runtime, data-root location. It blocked entry until the user
  // configured them, so the first screen was a setup flow that could not
  // succeed — which reads as broken rather than deliberately narrower.
  const text = await page.evaluate(() => document.body.innerText)
  expect(text).not.toContain('FIRST-TIME SETUP')
  expect(text).toContain('Question')
  expect(text).toContain('Evidence')
})

test('the desk says the engine is offline rather than looking healthy', async () => {
  // LUMEN_BINARY points at nothing. Stating it is the point: an app that looks
  // ready without an engine is the failure mode this whole branch removed, and
  // a user who cannot see the engine is down cannot act on it.
  const text = await page.evaluate(() => document.body.innerText)
  expect(text.toLowerCase()).toContain('engine offline')
})

test('an engine refusal is readable without losing the technical reason', async () => {
  // Opening a project with no engine hits the fail-closed path. The banner used
  // to be a wall of internal text — file paths and method names across the top
  // of the window — which is honest and unusable.
  const project = page.locator('aside button').filter({ hasText: /./ }).nth(1)
  if ((await project.count()) > 0) {
    await project.click()
    await page.waitForTimeout(1200)
  }

  const banner = await page.evaluate(() => {
    const el = document.querySelector('[role="status"], [role="alert"]')
    if (!el) return null
    const paragraphs = [...el.querySelectorAll('p')].map((p) => p.textContent ?? '')
    return { headline: paragraphs[0] ?? '', detail: paragraphs[1] ?? '' }
  })

  if (banner) {
    // A plain sentence leads.
    expect(banner.headline.length).toBeGreaterThan(10)
    expect(banner.headline).not.toContain('.ts')
    // And the original reason is still there, not swapped out for it.
    expect(banner.detail.length).toBeGreaterThan(banner.headline.length)
  }
})

test('the design system is actually applied, not just referenced', async () => {
  // Tailwind was silently OFF for the whole app. electron.vite.config.ts
  // require()d @tailwindcss/vite, which is ESM-only, so it always threw
  // ERR_REQUIRE_ESM — and a catch swallowed it as "optional when not
  // installed". The bundle still carried theme variables, so the build looked
  // fine while every screen shipped unstyled.
  //
  // Asserting COMPUTED style, not class names: a className present in the DOM
  // proves nothing about whether a rule was generated for it, which is exactly
  // how this hid.
  const applied = await page.evaluate(() => {
    const heading = document.querySelector('h1')
    const root = document.getElementById('root')?.firstElementChild
    return {
      headingSize: heading ? parseFloat(getComputedStyle(heading).fontSize) : 0,
      headingWeight: heading ? getComputedStyle(heading).fontWeight : '',
      background: root ? getComputedStyle(root).backgroundColor : '',
    }
  })

  // text-xl is 20px; the browser default for h1 is 32px and an unstyled
  // inherited size is 16px. Either would mean no utility was generated.
  expect(applied.headingSize).toBeGreaterThan(16)
  expect(applied.headingSize).toBeLessThan(32)
  expect(Number(applied.headingWeight)).toBeGreaterThanOrEqual(500)
  // A themed surface, not the transparent default.
  expect(applied.background).not.toBe('rgba(0, 0, 0, 0)')
})

test('the UI does not ship under the upstream project name', async () => {
  // app-config.ts already said "Lumen Science", but 38 reachable components
  // carried the upstream name as hardcoded text, so the first screen a user saw
  // said Open Science. Its own header warns against exactly this: shipping
  // under another project's name and copyright.
  //
  // Checks rendered TEXT, not source. Attribution lives in file headers and in
  // third_party/open-science/NOTICE, where the licence requires it — this must
  // not push anyone toward stripping that.
  const visible = await page.evaluate(() => document.body.innerText)
  expect(visible).not.toContain('Open Science')
})

test('the preload exposes the lumen surface, and nothing more', async () => {
  const surface = await page.evaluate(() => {
    const api = (window as unknown as { api?: { lumen?: Record<string, unknown> } }).api
    return api?.lumen ? Object.keys(api.lumen).sort() : null
  })
  expect(surface).not.toBeNull()
  // The permission channel must be reachable from the renderer, or the prompt
  // can never be answered.
  expect(surface).toContain('onPermissionAsk')
  expect(surface).toContain('respondToPermission')
  expect(surface).toContain('bindSession')
})

test('the renderer cannot reach Node, even though main can', async () => {
  // contextIsolation + sandbox. If this ever regresses, a rendered artifact
  // could read the filesystem directly and every containment argument in this
  // repo collapses.
  const escapes = await page.evaluate(() => ({
    require: typeof (window as unknown as { require?: unknown }).require,
    process: typeof (window as unknown as { process?: unknown }).process,
    module: typeof (window as unknown as { module?: unknown }).module,
  }))
  expect(escapes.require).toBe('undefined')
  expect(escapes.process).toBe('undefined')
  expect(escapes.module).toBe('undefined')
})

test('with no engine, the app fails closed instead of pretending', async () => {
  // LUMEN_BINARY points at nothing. A call must report an error, never a
  // plausible empty success — the local-catalog fallback that used to grant
  // membership when the engine was unreachable is exactly what LS5-D2-02
  // removed.
  const result = await page.evaluate(async () => {
    const api = (window as unknown as {
      api?: { lumen?: { bindSession: (r: unknown) => Promise<unknown> } }
    }).api
    if (!api?.lumen) return { unavailable: true }
    return api.lumen.bindSession({ ownerId: 'e2e-owner', projectId: 'nonexistent' })
  })

  const bound = result as { ok?: boolean; reason?: string; unavailable?: boolean }
  expect(bound.unavailable ?? false).toBe(false)
  expect(bound.ok).toBe(false)
  // And it must say WHY. "Denied" and "could not reach the authority" are
  // different facts, and a user who cannot tell them apart cannot fix either.
  expect(bound.reason ?? '').not.toBe('')
})

test('a banned IPC channel is refused from the renderer', async () => {
  // `projects:list` is on the banned list. The renderer has no legitimate way
  // to call it; if it ever succeeds, the authority policy is not enforced at
  // runtime — only in the unit test that checks the list.
  const outcome = await page.evaluate(async () => {
    const invoke = (window as unknown as {
      electron?: { ipcRenderer?: { invoke: (c: string) => Promise<unknown> } }
    }).electron?.ipcRenderer?.invoke
    if (!invoke) return { noAccess: true }
    try {
      const value = await invoke('projects:list')
      return { value }
    } catch (error) {
      return { threw: String(error) }
    }
  })

  const banned = outcome as { noAccess?: boolean; value?: unknown; threw?: string }
  if (banned.noAccess) {
    // Even better: the renderer has no raw ipcRenderer at all.
    expect(banned.noAccess).toBe(true)
  } else {
    // Reached IPC, so it must have been refused rather than served.
    const served = banned.value as { _lumenBanned?: boolean } | undefined
    expect(banned.threw !== undefined || served?._lumenBanned === true).toBe(true)
  }
})

test('the app reports the engine binary hash it is actually running', async () => {
  // Null is the correct answer with no binary present. The failure this guards
  // is a hash reported for a binary that was never resolved, which would put a
  // false identity into an evidence record.
  const hash = await page.evaluate(async () => {
    const api = (window as unknown as {
      api?: { lumen?: { getBinaryHash: () => Promise<string | null> } }
    }).api
    return api?.lumen ? api.lumen.getBinaryHash() : 'no-surface'
  })
  expect(hash === null || /^[0-9a-f]{64}$/.test(hash as string)).toBe(true)
})
