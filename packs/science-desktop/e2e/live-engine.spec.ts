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
