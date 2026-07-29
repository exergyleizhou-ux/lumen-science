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

test('a notebook cell actually runs in the engine after approval', async () => {
  // The chain this proves: Run in engine → workflow_execute → SessionActor
  // permission prompt → human Allow → kernel admitted from a pinned
  // interpreter → sandboxed exec-loop → run recorded → result on screen.
  //
  // Until LS5-K24 the button sent `notebook_execute`, which the desktop's own
  // registry REJECTS — so this path had never once succeeded against an
  // engine, while the notebook suite stayed green on a mock that accepted the
  // rejected name. This is the only test anywhere that runs a cell for real.
  await page.getByRole('tab', { name: 'Notebook', exact: true }).click()
  await page.locator('#panel-notebook').waitFor({ timeout: 10_000 })

  const code = page.locator('#panel-notebook textarea')
  await code.fill('print(6 * 7)\n')
  await page.getByRole('button', { name: 'Run in engine', exact: true }).click()

  // The engine must ask before running arbitrary code. If no prompt ever
  // appears AND no result arrives, the timeout below reports it.
  const allow = page.getByRole('button', { name: /allow/i }).first()
  try {
    await allow.waitFor({ timeout: 30_000 })
    await allow.click()
  } catch {
    // Already-approved sessions may not re-prompt; the output assertion below
    // is the real gate either way.
  }

  // The run's terminal state must be visible, and it must be success. The
  // output pane prints the engine's report verbatim.
  await expect
    .poll(async () => page.locator('#panel-notebook pre').textContent(), { timeout: 120_000 })
    .toContain('"state": "succeeded"')

  const report = (await page.locator('#panel-notebook pre').textContent()) ?? ''
  // An artifact was committed — the cell really produced recorded output, not
  // just a state flag.
  expect(report).toContain('artifactsCommitted')
  // Empty MEANS empty: asserting the absence of '"refusedSteps": [' bans the
  // empty array too, since '[]' contains '['. Assert the empty list itself.
  expect(report).toContain('"refusedSteps": []')
})

test('the run\'s artifacts are previewable and reviewable — the evidence chain holds', async () => {
  // End of the chain the product exists for: a cell ran in the engine, the
  // engine committed hashed outputs, and now (a) the Evidence tab resolves one
  // of those hashes to a real local file and (b) a review of those exact bytes
  // is recorded under actor authority. Workflow outputs are seeded from the
  // actor-owned commit report; durable ScienceStore runs are seeded through
  // the Rust artifact_list query.
  const report = (await page.locator('#panel-notebook pre').textContent()) ?? ''
  // The manifest maps relative path → content hash; stdout.txt always exists
  // for a cell that printed.
  const sha = /"stdout\.txt":\s*"([0-9a-f]{64})"/.exec(report)?.[1]
  expect(sha, 'the run report must carry a hashed stdout artifact').toBeTruthy()

  // (a) Preview by content hash.
  await page.getByRole('tab', { name: 'Evidence', exact: true }).click()
  await page.locator('#panel-evidence').waitFor({ timeout: 10_000 })
  await page.getByLabel('Artifact id to preview').fill(sha!)
  await page.getByRole('button', { name: 'Preview', exact: true }).click()
  await expect
    .poll(async () => page.locator('#panel-evidence pre').textContent(), { timeout: 15_000 })
    .toContain('ok bytes=')
  const meta = (await page.locator('#panel-evidence pre').textContent()) ?? ''
  // The record's hash is the id itself — content addressing, visibly.
  expect(meta).toContain(sha!)

  // (b) Review those bytes.
  await page.getByRole('tab', { name: 'Review', exact: true }).click()
  await page.locator('#panel-review').waitFor({ timeout: 10_000 })
  await expect(page.locator('#panel-review')).toContainText(/Source run:\s*[A-Za-z0-9_-]+/)
  await page
    .getByLabel('Artifacts to review, one per line as id:expected-sha256')
    .fill(`${sha}:${sha}`)
  await page.getByLabel('Review verdict').selectOption('pass')
  await page
    .getByLabel('Review rationale')
    .fill('The recorded stdout bytes match the expected deterministic result.')
  await page.getByRole('button', { name: /submit review/i }).click()

  // Recording a verdict is a new durable mutation, not a side effect of the
  // notebook approval. It must ask independently; otherwise the UI either
  // bypassed SessionActor or waits forever on a prompt this test never answers.
  const allowReview = page.getByRole('button', { name: /allow/i }).first()
  await allowReview.waitFor({ timeout: 30_000 })
  await allowReview.click()

  await expect
    .poll(async () => page.locator('#panel-review pre').textContent(), { timeout: 30_000 })
    .toContain('"ok": true')
  const review = (await page.locator('#panel-review pre').textContent()) ?? ''
  expect(review).toContain('"outcome": "pass"')
  // The engine's record came back — this was not a local projection alone.
  expect(review).toContain('SessionActor')
})

test('a refined research question survives leaving the project', async () => {
  // The question was renderer state only: typed, never sent anywhere, gone on
  // the next open — while the engine's durable record said something else
  // entirely. Persistence is the whole claim of this tab, so the test proves
  // it the only way that counts: navigate away, come back, read it again.
  const QUESTION = `Does the refined question persist? ${Date.now()}`

  await page.getByRole('tab', { name: 'Question', exact: true }).click()
  await page.locator('#panel-question').waitFor({ timeout: 10_000 })
  await page.locator('#panel-question textarea').fill(QUESTION)

  const save = page.getByRole('button', { name: 'Save question', exact: true })
  await expect(save).toBeEnabled()
  await save.click()

  // Saving mutates the record, so the engine asks — same gate as any other
  // change to it.
  const allow = page.getByRole('button', { name: /allow/i }).first()
  try {
    await allow.waitFor({ timeout: 30_000 })
    await allow.click()
  } catch {
    // An already-approved session may not re-prompt.
  }
  await expect
    .poll(async () => page.locator('#panel-question').innerText(), { timeout: 60_000 })
    .toContain('Saved to the project record')

  // Leave the project entirely, then re-open it. Re-reading the same mounted
  // textarea would prove nothing — the defect WAS that it looked fine until
  // you came back.
  const project = page.locator('aside button').filter({ hasText: /Live approval/ }).first()
  await project.click()
  await page.waitForTimeout(1500)
  await page.getByRole('tab', { name: 'Question', exact: true }).click()
  await page.locator('#panel-question').waitFor({ timeout: 10_000 })

  await expect
    .poll(async () => page.locator('#panel-question textarea').inputValue(), { timeout: 30_000 })
    .toBe(QUESTION)

  // And it is reported as saved, not as pending work.
  expect(await page.locator('#panel-question').innerText()).not.toContain('Unsaved changes')
})

test('the Skills and Connectors catalogs actually load', async () => {
  // Both read files that were resolved from process.cwd(). Skills swallowed the
  // read error and returned an empty inventory — indistinguishable from "no
  // skills exist" — and Connectors reported a path error. Neither could work in
  // a packaged app at all.
  await page.getByRole('tab', { name: 'Skills', exact: true }).click()
  await page.locator('#panel-skills').waitFor({ timeout: 10_000 })
  await page.getByRole('button', { name: 'List inventory', exact: true }).click()
  await expect
    .poll(async () => page.locator('#panel-skills pre').textContent(), { timeout: 15_000 })
    .toContain('"ok": true')

  const skills = (await page.locator('#panel-skills pre').textContent()) ?? ''
  // Non-empty: a green "ok" over a zero inventory is the exact failure this
  // replaced, so assert the registry actually had contents.
  const total = /"total":\s*(\d+)/.exec(skills)?.[1]
  expect(Number(total ?? 0)).toBeGreaterThan(0)
  expect(skills).not.toContain('unreadable')

  await page.getByRole('tab', { name: 'Connectors', exact: true }).click()
  await page.locator('#panel-connectors').waitFor({ timeout: 10_000 })
  await page.getByRole('button', { name: 'List catalog', exact: true }).click()
  await expect
    .poll(async () => page.locator('#panel-connectors pre').textContent(), { timeout: 15_000 })
    .toContain('"ok": true')
  const conn = (await page.locator('#panel-connectors pre').textContent()) ?? ''
  expect(conn).toContain('pubmed')
})
