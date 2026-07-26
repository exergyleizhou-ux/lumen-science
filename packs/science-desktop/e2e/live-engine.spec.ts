/**
 * Headed end-to-end against a REAL engine binary.
 *
 * `desktop.spec.ts` deliberately runs with no engine, to prove the app fails
 * closed. That is a necessary test and an insufficient one: every assertion in
 * it is satisfied by an app that can do nothing at all. It cannot see whether a
 * user who clicks Allow actually gets their project.
 *
 * They did not. The broker answered the engine's permission request with a
 * hardcoded `optionId: 'allow_once'`, but ACP has the ENGINE offer the options
 * (`client.rs:653` — each carries its own `optionId` and a `kind`). An id the
 * engine never issued is not a choice it can act on, so it recorded the run as
 * `Denied`. The dialog appeared, the user clicked Allow, the click was logged,
 * and the operation was refused — every visible signal said success.
 *
 * No amount of engine-less testing reaches that. The bug lives precisely in the
 * exchange the engine-less suite removes, which is why this file exists.
 *
 * Requires a built binary. It does NOT skip when one is missing: a skipped test
 * reads as a passing one in CI summaries, and this is the only suite covering
 * the approval path.
 *
 *   npm run test:e2e:live
 */
import { test, expect, _electron as electron, type ElectronApplication, type Page } from '@playwright/test'
import { existsSync } from 'node:fs'
import { mkdtemp } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'

const PACK = process.cwd()
const BINARY = process.env.LUMEN_BINARY ?? path.resolve(PACK, '../../agent/target/debug/lumen')

let app: ElectronApplication
let page: Page

test.beforeAll(async () => {
  // Fail loudly rather than skip. "0 passed, 1 skipped" is how an untested
  // approval path looks green.
  if (!existsSync(BINARY)) {
    throw new Error(
      `No engine binary at ${BINARY}. Build it (cargo build --bin lumen) or set LUMEN_BINARY. ` +
        `This suite must not be skipped: it is the only coverage of the permission handshake.`,
    )
  }

  // A scratch home AND a scratch userData: the project catalog lives in
  // Electron's userData, so LUMEN_HOME alone left a developer's real projects
  // (and every previous run's) in the sidebar.
  const home = await mkdtemp(path.join(tmpdir(), 'lumen-live-'))
  const userData = await mkdtemp(path.join(tmpdir(), 'lumen-live-ud-'))

  app = await electron.launch({
    args: [path.join(PACK, 'out/main/index.js'), `--user-data-dir=${userData}`],
    cwd: PACK,
    env: {
      ...process.env,
      LUMEN_DESKTOP_OWNER_ID: 'live-e2e-owner',
      LUMEN_BINARY: BINARY,
      LUMEN_HOME: home,
    },
  })
  page = await app.firstWindow()
  await page.waitForLoadState('load')
  await page.waitForFunction(
    () => (document.getElementById('root')?.childElementCount ?? 0) > 0,
    undefined,
    { timeout: 30_000 },
  )
})

test.afterAll(async () => {
  await app?.close()
})

test('the desk reports the engine as reachable, not offline', async () => {
  // The engine-less suite asserts the opposite. If this says offline with a
  // real binary, nothing below is meaningful and the failure belongs here.
  await expect
    .poll(async () => (await page.evaluate(() => document.body.innerText)).toLowerCase(), {
      timeout: 30_000,
    })
    .not.toContain('engine offline')
})

test('clicking Allow creates the project the user asked for', async () => {
  const NAME = `Live approval ${Date.now()}`

  // By placeholder, not by `input[type="text"]`: the input has no `type`
  // attribute, so the attribute selector matched nothing while the field was
  // plainly on screen.
  const nameField = page.getByPlaceholder('New project name')
  await nameField.waitFor({ timeout: 15_000 })
  await nameField.fill(NAME)

  const createButton = page.getByRole('button', { name: /create/i }).first()
  await expect(createButton).toBeEnabled({ timeout: 10_000 })
  await createButton.click()

  // The engine asks before it mutates. Waiting for the prompt is itself an
  // assertion: a build that creates the project without asking never shows it.
  const allow = page.getByRole('button', { name: /allow/i }).first()
  await allow.waitFor({ timeout: 60_000 })
  await allow.click()

  // The claim under test: an approval produces the project. Previously the
  // click was recorded, the dialog closed, and the run finished Denied.
  await expect
    .poll(async () => page.evaluate(() => document.body.innerText), { timeout: 60_000 })
    .toContain(NAME)

  // And no refusal banner is left behind. Without this, a screen showing both
  // the project name and "the engine refused this operation" would pass.
  const refusal = await page.evaluate(() => {
    const el = document.querySelector('[role="alert"]')
    return el ? (el.textContent ?? '') : ''
  })
  expect(refusal.toLowerCase()).not.toContain('refused')
  expect(refusal.toLowerCase()).not.toContain('denied')
})

test('opening the approved project reaches the workspace, not a membership refusal', async () => {
  // `project_assert_membership` gates entry. It is a real engine round-trip, so
  // it can only be exercised here — and the tabs behind it had never rendered.
  const project = page.locator('aside button').filter({ hasText: /Live approval/ }).first()
  await project.waitFor({ timeout: 15_000 })
  await project.click()

  await expect
    .poll(async () => page.evaluate(() => document.body.innerText), { timeout: 30_000 })
    .toContain('Notebook')

  const text = await page.evaluate(() => document.body.innerText)
  // The eight workspace tabs are the product. Assert the set, not a sample:
  // a partial render is the failure mode worth catching.
  for (const tab of ['Question', 'Plan', 'Notebook', 'Evidence', 'Result', 'Review', 'Skills', 'Compute']) {
    expect(text).toContain(tab)
  }

  const banner = await page.evaluate(() => {
    const el = document.querySelector('[role="alert"]')
    return el ? (el.textContent ?? '') : ''
  })
  expect(banner.toLowerCase()).not.toContain('could not confirm you have access')
})

test('the workspace explains itself to a researcher, not to an implementer', async () => {
  // Twice now, honest internal notes shipped as user-facing copy: the first
  // screen inside a project named one of our source files, and the Notebook
  // tab read "Electron KernelExecutor stays stubbed".
  //
  // Neither was a lie — that is what makes this worth a test rather than a
  // review note. The guarantee they encode is the product's whole argument
  // (nothing executes in this window; the engine runs it and records what ran),
  // and stating it in module names buries the one thing a researcher should
  // take away.
  //
  // Requires an OPEN project, so it can only live in the live suite: the panels
  // do not render without one.
  //
  // EVERY tab, not just the visible one. Only the active panel is mounted, so
  // reading document.body once checks Question and nothing else — the first
  // version of this test passed with "Electron KernelExecutor stays stubbed"
  // put back into the Notebook panel, which is how it was caught.
  //
  // The check is on rendered text only. Attribution and architecture belong in
  // file headers and docs/, and this must not push anyone toward stripping them.
  const TABS = ['Question', 'Plan', 'Notebook', 'Evidence', 'Result', 'Review', 'Skills', 'Compute', 'Connectors']
  let text = ''
  for (const tab of TABS) {
    await page.getByRole('tab', { name: tab, exact: true }).click()
    // Wait for the PANEL, not a sleep: click resolves before React re-renders,
    // so reading immediately captured the previous tab's text and the Compute
    // panel never appeared in the sweep at all.
    await page.locator(`#panel-${tab.toLowerCase()}`).waitFor({ timeout: 10_000 })
    text += `\n${await page.evaluate(() => document.body.innerText)}`
  }

  const JARGON = [
    'SessionActor',
    'KernelExecutor',
    'KernelAdapter',
    'WorkflowActor',
    'ToolAdapter',
    'SystemSshRunner',
    'fusion-sources.lock',
    'Electron',
    'extensions/science.rs',
  ]
  // Internal spec codes are as opaque to a researcher as module names, and they
  // arrive the same way — someone documents a rule and pastes its identifier.
  const SPEC_CODE = /\b(DS-\d+|LS5-[A-Z]\d+|OSF\d+)\b/
  const codeHit = text.match(SPEC_CODE)
  expect(codeHit?.[0] ?? null, 'an internal spec code is visible in the UI').toBeNull()
  // Case-insensitively: innerText returns text as CSS renders it, and these
  // headings are `uppercase`, so an exact-case match would silently skip any
  // jargon that happens to sit in a heading.
  const haystack = text.toLowerCase()
  const found = JARGON.filter((term) => haystack.includes(term.toLowerCase()))
  expect(found, `implementation detail visible in the UI: ${found.join(', ')}`).toEqual([])

  // And the sweep must actually have visited the panels, or the assertion above
  // is satisfied by a page that rendered nothing.
  expect(haystack).toContain('remote compute')
  expect(haystack).toContain('research question')
})
