/**
 * Headed Electron E2E config.
 *
 * Serial and single-worker on purpose: these launch a real application, and two
 * instances would race on the same userData directory and on the engine
 * process. A flaky E2E is worse than none — it teaches people to re-run until
 * green.
 *
 * No retries. A retry would hide exactly the intermittent wiring faults this
 * suite exists to catch.
 */
import { defineConfig } from '@playwright/test'

/**
 * The live suite needs a built engine binary; the default suite needs there to
 * be NO engine, to prove the app fails closed. They cannot run together, and
 * the split is explicit rather than a skip: a suite that skips itself when its
 * dependency is missing reports green while testing nothing.
 */
const live = process.env.LUMEN_E2E_LIVE === '1'

export default defineConfig({
  testDir: './e2e',
  testMatch: live ? ['**/live-engine.spec.ts'] : ['**/desktop.spec.ts'],
  fullyParallel: false,
  workers: 1,
  retries: 0,
  // The live suite starts a real engine and waits on a human-facing approval
  // round-trip, so its budget is larger. Raising it for both would slow the
  // engine-less suite's failures down to a crawl.
  timeout: live ? 180_000 : 60_000,
  reporter: [['list']],
  use: {
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
})
