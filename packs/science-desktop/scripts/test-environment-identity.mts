#!/usr/bin/env npx tsx
/**
 * LS5-K4 — environment identity adapter, executed against the real machine.
 *
 * This drives the SHIPPED modules, not a re-implementation of them:
 *   src/main/notebook/environment-discovery.ts   (adopted, adapted)
 *   src/main/notebook/bundle-manifest.ts         (adopted, adapted)
 *   src/main/notebook/runtime-paths.ts           (adopted, adapted)
 *   src/main/environment/*.ts                    (Lumen adapter)
 *   src/shared/runtime-origin-policy.ts          (Lumen policy)
 *
 * What it is trying to prove, in order of how badly getting it wrong would hurt:
 *
 *  1. The adapter has no execution authority. It cannot say "admitted", and
 *     with no engine reachable it says it could not ask rather than guessing.
 *  2. The facts are real. The sha256 is the sha256 of the bytes on disk, the
 *     version is what the interpreter printed, and both are stable across runs.
 *  3. An unpinned interpreter is refused, with a reason, at every layer.
 *  4. No third-party download origin is reachable, by configuration or by URL
 *     composition.
 *
 * Requires a python3 on the machine — that is the point of the test. A run
 * that "passed" on a host with no interpreter would prove nothing.
 *
 * Run: npx tsx scripts/test-environment-identity.mts
 */
import { deepStrictEqual, ok, strictEqual } from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

import {
  discoverInterpreters,
  isPinnedInterpreterPath,
  partitionCandidates,
  type DiscoveryDeps,
} from '../src/main/notebook/environment-discovery.js'
import { manifestUrl, packUrl } from '../src/main/notebook/bundle-manifest.js'
import { resolveRuntimeCdnBase } from '../src/main/notebook/runtime-paths.js'
import {
  classifyRuntimeOrigin,
  resolveRuntimeOriginPolicy,
} from '../src/shared/runtime-origin-policy.js'
import {
  VERSION_PROBE_ARGV,
  identifyInterpreter,
} from '../src/main/environment/interpreter-identity.js'
import { buildKernelAdmissionRequest } from '../src/main/environment/admission-request.js'
import { createEnvironmentService } from '../src/main/environment/service.js'
import {
  registerScienceIpcHandlers,
  resolveNotebookInterpreter,
  type IpcMainLike,
  type SafeHandleFn,
} from '../src/main/files/science-ipc.js'
import { validateIpcChannel } from '../src/main/lumen-authority-policy.js'

let failures = 0
const test = (name: string, fn: () => void | Promise<void>): Promise<void> =>
  Promise.resolve()
    .then(() => fn())
    .then(() => console.log(`OK  ${name}`))
    .catch((e: unknown) => {
      failures++
      console.log(`FAIL ${name}: ${(e as Error).message}`)
    })

const PACK = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..')

// ── the machine we are testing against ───────────────────────────

const realPython = ((): string => {
  const out = execFileSync('sh', ['-c', 'command -v python3 || true'], {
    encoding: 'utf8',
  }).trim()
  if (!out) {
    console.error(
      'FAIL setup: no python3 on this machine. This suite exists to prove the adapter reports ' +
        'REAL interpreter facts; without an interpreter there is nothing to observe and a pass ' +
        'would be meaningless.',
    )
    process.exit(1)
  }
  return fs.realpathSync(out)
})()

const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'lumen-k4-'))
const runtimeRoot = path.join(scratch, 'runtime')
fs.mkdirSync(path.join(runtimeRoot, 'envs'), { recursive: true })

async function run(): Promise<void> {
  // ── 1. Pinned-path rule ────────────────────────────────────────

  await test('isPinnedInterpreterPath rejects PATH-relative and cwd-relative names', () => {
    strictEqual(isPinnedInterpreterPath('python3', 'darwin'), false)
    strictEqual(isPinnedInterpreterPath('./python3', 'darwin'), false)
    strictEqual(isPinnedInterpreterPath('../bin/python3', 'darwin'), false)
    strictEqual(isPinnedInterpreterPath('', 'darwin'), false)
    strictEqual(isPinnedInterpreterPath(' /usr/bin/python3', 'darwin'), false)
    strictEqual(isPinnedInterpreterPath('/usr/bin/python3\0evil', 'darwin'), false)
    strictEqual(isPinnedInterpreterPath('/usr/bin/python3', 'darwin'), true)
  })

  await test('isPinnedInterpreterPath rejects Windows root-relative, accepts drive and UNC', () => {
    strictEqual(isPinnedInterpreterPath('\\Python\\python.exe', 'win32'), false)
    strictEqual(isPinnedInterpreterPath('python.exe', 'win32'), false)
    strictEqual(isPinnedInterpreterPath('C:\\Python\\python.exe', 'win32'), true)
    strictEqual(isPinnedInterpreterPath('\\\\host\\share\\python.exe', 'win32'), true)
  })

  await test('partitionCandidates keeps a reason for every rejection', () => {
    const { pinned, unpinned } = partitionCandidates(['python3', '/usr/bin/python3'], 'darwin')
    deepStrictEqual(pinned, ['/usr/bin/python3'])
    strictEqual(unpinned.length, 1)
    strictEqual(unpinned[0].candidate, 'python3')
    ok(unpinned[0].reason.includes('not an absolute path'), unpinned[0].reason)
    ok(unpinned[0].reason.includes('reproduced'), unpinned[0].reason)
  })

  await test('discoverInterpreters never returns an unpinned candidate, and reports each', async () => {
    const seen: string[] = []
    const deps: DiscoveryDeps = {
      candidatePaths: async () => ['python3', './python3', realPython],
      probeVersion: async () => '3.12.0',
      rRunnable: async () => false,
      realpath: (p) => p,
      runtimeRoot,
      onUnpinnedCandidate: (candidate) => seen.push(candidate),
    }
    const found = await discoverInterpreters('python', deps)
    deepStrictEqual(
      found.map((f) => f.interpreterPath),
      [realPython],
    )
    deepStrictEqual(seen.sort(), ['./python3', 'python3'])
  })

  await test("identifyInterpreter rejects a PATH-relative interpreter with the engine's code", async () => {
    for (const bad of ['python3', './python3', '']) {
      const r = await identifyInterpreter({
        kind: 'python',
        interpreterPath: bad,
      })
      ok(!r.identified, `expected rejection for ${JSON.stringify(bad)}`)
      strictEqual(r.failure.code, 'interpreter_path_not_absolute')
      ok(r.failure.detail.includes('not absolute'), r.failure.detail)
    }
  })

  // ── 2. Real identification ─────────────────────────────────────

  const expectedSha = createHash('sha256').update(fs.readFileSync(realPython)).digest('hex')

  await test('identifies a real python3: sha256 matches the bytes on disk', async () => {
    const r = await identifyInterpreter({
      kind: 'python',
      interpreterPath: realPython,
    })
    ok(r.identified, JSON.stringify(r))
    strictEqual(r.identity.executableSha256, expectedSha)
    strictEqual(r.identity.executableSizeBytes, fs.statSync(realPython).size)
    strictEqual(r.identity.interpreterPath, realPython)
    strictEqual(r.identity.os, process.platform)
    strictEqual(r.identity.architecture, process.arch)
    strictEqual(r.identity.packageLock, null)
  })

  await test('identifies a real python3: exact version came from the interpreter', async () => {
    const r = await identifyInterpreter({
      kind: 'python',
      interpreterPath: realPython,
    })
    ok(r.identified)
    ok(/^Python 3\.\d+\.\d+/.test(r.identity.exactVersion), r.identity.exactVersion)
    ok(!r.identity.exactVersion.includes('\n'), 'version must be one line')
    // -VV, not -V: the build string is what distinguishes two builds of one release.
    ok(r.identity.exactVersion.length > 'Python 3.13.1'.length, r.identity.exactVersion)
    deepStrictEqual([...r.identity.versionProbeArgv], ['-VV'])
    deepStrictEqual([...VERSION_PROBE_ARGV.python], ['-VV'])
  })

  await test('identification is stable across runs', async () => {
    const a = await identifyInterpreter({
      kind: 'python',
      interpreterPath: realPython,
    })
    const b = await identifyInterpreter({
      kind: 'python',
      interpreterPath: realPython,
    })
    ok(a.identified && b.identified)
    strictEqual(a.identity.executableSha256, b.identity.executableSha256)
    strictEqual(a.identity.exactVersion, b.identity.exactVersion)
    strictEqual(a.identity.interpreterPath, b.identity.interpreterPath)
  })

  await test('a symlinked interpreter is hashed through to its target', async () => {
    const link = path.join(scratch, 'python3-link')
    fs.symlinkSync(realPython, link)
    const r = await identifyInterpreter({
      kind: 'python',
      interpreterPath: link,
    })
    ok(r.identified, JSON.stringify(r))
    strictEqual(r.identity.requestedPath, link)
    strictEqual(r.identity.interpreterPath, realPython)
    strictEqual(r.identity.executableSha256, expectedSha)
  })

  await test('a pinned package lock is hashed; a missing one fails rather than reporting null', async () => {
    const lock = path.join(scratch, 'requirements.lock')
    fs.writeFileSync(lock, 'numpy==2.1.0\n')
    const withLock = await identifyInterpreter({
      kind: 'python',
      interpreterPath: realPython,
      packageLockPath: lock,
    })
    ok(withLock.identified)
    strictEqual(withLock.identity.packageLock?.path, lock)
    strictEqual(
      withLock.identity.packageLock?.sha256,
      createHash('sha256').update(fs.readFileSync(lock)).digest('hex'),
    )

    const missing = await identifyInterpreter({
      kind: 'python',
      interpreterPath: realPython,
      packageLockPath: path.join(scratch, 'nope.lock'),
    })
    ok(!missing.identified)
    strictEqual(missing.failure.code, 'package_lock_not_a_file')
  })

  await test('non-interpreters are reported by what was observed, not by a verdict', async () => {
    const gone = await identifyInterpreter({
      kind: 'python',
      interpreterPath: path.join(scratch, 'absent'),
    })
    ok(!gone.identified)
    strictEqual(gone.failure.code, 'interpreter_not_found')

    const dir = await identifyInterpreter({
      kind: 'python',
      interpreterPath: scratch,
    })
    ok(!dir.identified)
    strictEqual(dir.failure.code, 'interpreter_not_a_file')

    const plain = path.join(scratch, 'not-executable')
    fs.writeFileSync(plain, 'x', { mode: 0o600 })
    const noExec = await identifyInterpreter({
      kind: 'python',
      interpreterPath: plain,
    })
    ok(!noExec.identified)
    strictEqual(noExec.failure.code, 'interpreter_not_executable')

    const notAnInterpreter = path.join(scratch, 'exits-nonzero')
    fs.writeFileSync(notAnInterpreter, '#!/bin/sh\nexit 3\n', { mode: 0o700 })
    // Generous budget on purpose. This asserts WHICH failure code a probe
    // produces, not how fast it runs, so it only has to outlast scheduling
    // delay. The 10s default was enough on an idle machine and not enough
    // while other builds were running: the whole suite failed here with
    // 'version_probe_timed_out' while passing in isolation. A budget tuned for
    // an idle machine is a flake waiting for a shared CI runner, and a flaky
    // test is worse than none — it teaches people to ignore failures.
    const bad = await identifyInterpreter({
      kind: 'python',
      interpreterPath: notAnInterpreter,
      probeTimeoutMs: 60_000,
    })
    ok(!bad.identified)
    strictEqual(bad.failure.code, 'version_probe_exit_non_zero')
  })

  await test('an identification carries no admission field of any kind', async () => {
    const r = await identifyInterpreter({
      kind: 'python',
      interpreterPath: realPython,
    })
    ok(r.identified)
    const serialised = JSON.stringify(r).toLowerCase()
    for (const forbidden of ['admit', 'allowed', 'permitted', 'approved']) {
      ok(!serialised.includes(forbidden), `identity leaked a permission word: ${forbidden}`)
    }
  })

  // ── 3. Real discovery on this machine ──────────────────────────

  await test('discover("python") enumerates absolute candidates without probing', async () => {
    const service = createEnvironmentService({ runtimeRoot })
    const report = await service.discover('python')
    ok(report.interpreters.length > 0, 'expected at least one python on this machine')
    for (const env of report.interpreters) {
      ok(path.isAbsolute(env.interpreterPath), `not absolute: ${env.interpreterPath}`)
      ok(path.isAbsolute(env.envId), `envId not absolute: ${env.envId}`)
    }
    ok(report.interpreters.every((e) => e.version === undefined))
    ok(report.interpreters.every((e) => e.runnable === false))
    ok(report.interpreters.every((e) => e.detail?.includes('SessionActor')))
  })

  await test('service discovery and identify never invoke injected desktop probes', async () => {
    let probes = 0
    const deps: DiscoveryDeps = {
      runtimeRoot,
      candidatePaths: async () => [realPython],
      realpath: (candidate) => fs.realpathSync(candidate),
      probeVersion: async () => {
        probes++
        return 'should-not-run'
      },
      rRunnable: async () => {
        probes++
        return true
      },
    }
    const service = createEnvironmentService({
      runtimeRoot,
      discoveryDeps: deps,
      identifyDeps: {
        runVersionProbe: async () => {
          probes++
          return { outcome: 'ok', stdout: 'should-not-run', stderr: '' }
        },
        hashFile: async () => {
          probes++
          return expectedSha
        },
      },
    })
    const report = await service.discover('python')
    strictEqual(report.interpreters.length, 1)
    const identified = await service.identify({
      kind: 'python',
      interpreterPath: realPython,
    })
    strictEqual(identified.identified, false)
    ok(
      !identified.identified && identified.failure.code === 'actor_probe_required',
      JSON.stringify(identified),
    )
    strictEqual(probes, 0, 'desktop service crossed the SessionActor execution boundary')
  })

  await test('discover surfaces a hand-added unpinned interpreter instead of dropping it', async () => {
    const service = createEnvironmentService({
      runtimeRoot,
      manualPaths: () => ['python3'],
    })
    const report = await service.discover('python')
    ok(
      report.unpinned.some((u) => u.candidate === 'python3'),
      `expected python3 in unpinned, got ${JSON.stringify(report.unpinned)}`,
    )
    ok(!report.interpreters.some((e) => e.interpreterPath === 'python3'))
  })

  await test('toolchain reports locations, not readiness', () => {
    const service = createEnvironmentService({ runtimeRoot })
    const t = service.toolchain('python')
    strictEqual(t.runtimeRoot, runtimeRoot)
    strictEqual(t.packageCacheDir, path.join(runtimeRoot, 'pkgs'))
    ok(t.packageCacheKey.length > 0)
    ok(t.environmentPrefix.startsWith(path.join(runtimeRoot, 'envs')))
  })

  // ── 4. Admission is the engine's, never the desktop's ──────────

  await test('with no transport, requestAdmission reports it could not ask', async () => {
    const service = createEnvironmentService({ runtimeRoot })
    const outcome = await service.requestAdmission({
      sessionId: 's1',
      ownerId: 'alice',
      projectId: 'project-a',
      storeRoot: scratch,
      kernelId: 'py',
      kind: 'python',
      interpreterPath: realPython,
    })
    strictEqual(outcome.asked, false)
    ok(outcome.asked === false && outcome.reason.includes('SessionActor'), JSON.stringify(outcome))
    ok(!JSON.stringify(outcome).toLowerCase().includes('admitted'))
  })

  await test('requestAdmission forwards facts to kernel_admission and returns the verdict verbatim', async () => {
    const calls: { method: string; args: Record<string, unknown> }[] = []
    const engineVerdict = {
      admission_status: 'Rejected',
      rejection_reason: { code: 'executable_hash_mismatch' },
    }
    const service = createEnvironmentService({
      runtimeRoot,
      acpCall: async (method, args) => {
        calls.push({ method, args })
        return engineVerdict
      },
    })
    const outcome = await service.requestAdmission({
      sessionId: 's1',
      ownerId: 'alice',
      projectId: 'project-a',
      storeRoot: scratch,
      kernelId: 'py-3',
      kind: 'python',
      interpreterPath: realPython,
      allowedRoot: '/usr',
    })
    strictEqual(outcome.asked, true)
    strictEqual(calls.length, 1)
    strictEqual(calls[0].method, 'kernel_admission')
    strictEqual(calls[0].args.interpreterPath, realPython)
    strictEqual(calls[0].args.execHash, undefined)
    strictEqual(calls[0].args.kind, 'python')
    strictEqual(calls[0].args.allowedRoot, '/usr')
    // deny_unknown_fields on the engine side: an extra key is a parse error.
    const allowed = new Set([
      'sessionId',
      'ownerId',
      'projectId',
      'storeRoot',
      'kernelId',
      'kind',
      'interpreterPath',
      'allowedRoot',
      'execHash',
      'packageLockPath',
      'lockHash',
      'probeTimeoutMs',
      'approvalTimeoutMs',
    ])
    for (const key of Object.keys(calls[0].args)) {
      ok(allowed.has(key), `unknown kernel_admission param would be rejected by the engine: ${key}`)
    }
    // A rejection from the engine must survive unmodified.
    deepStrictEqual(outcome.asked === true ? outcome.response : null, engineVerdict)
  })

  await test('requestAdmission does not ask when there is nothing to ask about', async () => {
    let called = 0
    const service = createEnvironmentService({
      runtimeRoot,
      acpCall: async () => {
        called++
        return {}
      },
    })
    const outcome = await service.requestAdmission({
      sessionId: 's1',
      ownerId: 'alice',
      projectId: 'project-a',
      storeRoot: scratch,
      kernelId: 'py',
      kind: 'python',
      interpreterPath: 'python3',
    })
    strictEqual(outcome.asked, false)
    strictEqual(called, 0, 'an unpinned interpreter must not reach the engine')
    ok(outcome.asked === false && outcome.reason.includes('not absolute'))
  })

  await test('buildKernelAdmissionRequest refuses a fabricated digest', () => {
    const built = buildKernelAdmissionRequest({
      sessionId: 's',
      ownerId: 'alice',
      projectId: 'project-a',
      storeRoot: scratch,
      kernelId: 'k',
      kind: 'python',
      interpreterPath: realPython,
      execHash: 'unknown',
    })
    strictEqual(built.ok, false)
    ok(built.ok === false && built.reason.includes('sha256'), JSON.stringify(built))
  })

  await test('buildKernelAdmissionRequest enforces the engine probe-timeout range', () => {
    for (const bad of [0, 120_001, 1.5]) {
      const built = buildKernelAdmissionRequest({
        sessionId: 's',
        ownerId: 'alice',
        projectId: 'project-a',
        storeRoot: scratch,
        kernelId: 'k',
        kind: 'python',
        interpreterPath: realPython,
        probeTimeoutMs: bad,
      })
      strictEqual(built.ok, false, `expected ${bad} to be refused`)
    }
  })

  // ── 5. No third-party download origin is reachable ─────────────

  await test('runtime origin is disabled by default', () => {
    const policy = resolveRuntimeOriginPolicy({})
    strictEqual(policy.enabled, false)
    ok(policy.enabled === false && policy.reason.includes('LUMEN_RUNTIME_CDN_BASE'))
  })

  await test('the upstream CDN is refused however it is configured', () => {
    for (const host of [
      'https://statics.aipoch.com/open-science',
      'https://aipoch.com/runtime',
      'https://www.aipoch.com/x',
      'https://cdn.statics.aipoch.com/x',
    ]) {
      const viaEnv = resolveRuntimeOriginPolicy({
        LUMEN_RUNTIME_CDN_BASE: host,
      })
      strictEqual(viaEnv.enabled, false, `env-configured ${host} must be refused`)
      const direct = classifyRuntimeOrigin(host)
      strictEqual(direct.enabled, false, `${host} must be refused`)
      ok(
        direct.enabled === false && /third-party|not a Lumen-owned/.test(direct.reason),
        direct.reason,
      )
    }
  })

  await test('http, unknown hosts and junk are refused; a Lumen host is accepted', () => {
    strictEqual(classifyRuntimeOrigin('http://releases.lumen.science').enabled, false)
    strictEqual(classifyRuntimeOrigin('https://evil.example/runtime').enabled, false)
    strictEqual(classifyRuntimeOrigin('not a url').enabled, false)
    strictEqual(classifyRuntimeOrigin('').enabled, false)
    const good = classifyRuntimeOrigin('https://releases.lumen.science/runtime//')
    ok(good.enabled)
    strictEqual(good.enabled === true ? good.baseUrl : '', 'https://releases.lumen.science/runtime')
  })

  await test('resolveRuntimeCdnBase throws unconfigured and throws on a forbidden override', () => {
    let threw = false
    try {
      resolveRuntimeCdnBase()
    } catch (e) {
      threw = true
      ok((e as Error).message.includes('refusing to fetch'), (e as Error).message)
    }
    ok(threw, 'unconfigured runtime CDN must throw')

    threw = false
    try {
      resolveRuntimeCdnBase('https://statics.aipoch.com/open-science')
    } catch {
      threw = true
    }
    ok(threw, 'a forbidden override must throw')
  })

  await test('composed bundle URLs are re-checked against the origin policy', () => {
    strictEqual(
      manifestUrl('https://releases.lumen.science', 1, 'osx-arm64'),
      'https://releases.lumen.science/runtime-bundle/1/osx-arm64/manifest.json',
    )
    for (const call of [
      () => manifestUrl('https://statics.aipoch.com/open-science', 1, 'osx-arm64'),
      () => packUrl('https://statics.aipoch.com', 1, 'osx-arm64', 'python-3.11.tar.zst'),
      // A manifest-supplied `file` that walks the URL onto another host.
      () => packUrl('https:/', 1, '', '/statics.aipoch.com/python-3.11.tar.zst'),
    ]) {
      let threw = false
      try {
        call()
      } catch {
        threw = true
      }
      ok(threw, 'a URL leaving the allowed origin must throw')
    }
  })

  await test('no source file names the forbidden host outside the two denylists', () => {
    // The whole tree, not just the reachable part: an unreachable file that
    // still contains a live third-party URL is one import away from being one.
    // The only files allowed to contain the host as CODE are the denylists —
    // comments about it (which is most of the remaining occurrences) are the
    // record of why it was removed and must survive.
    const denylists = new Set([
      'src/shared/update-policy.ts',
      'src/shared/runtime-origin-policy.ts',
    ])
    const offenders: string[] = []
    const walk = (dir: string): void => {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        const full = path.join(dir, entry.name)
        if (entry.isDirectory()) {
          walk(full)
          continue
        }
        if (!/\.(ts|tsx)$/.test(entry.name) || /\.(test|spec)\.tsx?$/.test(entry.name)) continue
        const rel = path.relative(PACK, full)
        if (denylists.has(rel)) continue
        const code = fs
          .readFileSync(full, 'utf8')
          .split('\n')
          .filter((line) => {
            const t = line.trim()
            return !(t.startsWith('//') || t.startsWith('*') || t.startsWith('/*'))
          })
          .join('\n')
        if (code.includes('aipoch.com')) offenders.push(rel)
      }
    }
    walk(path.join(PACK, 'src'))
    deepStrictEqual(offenders, [], `forbidden host present as code in: ${offenders.join(', ')}`)
  })

  // ── 6. The adapter spawns no kernel ────────────────────────────

  await test('nothing under src/main/environment can start a kernel', () => {
    const dir = path.join(PACK, 'src/main/environment')
    for (const file of fs.readdirSync(dir)) {
      const text = fs.readFileSync(path.join(dir, file), 'utf8')
      for (const banned of ['spawn(', 'lumen_python_loop', 'KernelExecutor', 'exec(']) {
        ok(!text.includes(banned), `${file} must not contain ${banned}`)
      }
      // execFile is permitted, but only for the fixed version argv.
      if (text.includes('execFile')) {
        ok(
          text.includes('VERSION_PROBE_ARGV'),
          `${file} spawns a process without the fixed version argv`,
        )
      }
    }
  })

  await test('the product environment service cannot import the desktop probe implementation', () => {
    const serviceSource = fs.readFileSync(
      path.join(PACK, 'src/main/environment/service.ts'),
      'utf8',
    )
    ok(!serviceSource.includes('identifyInterpreter'))
    ok(!serviceSource.includes('discoverInterpreters'))
  })

  await test('notebook execution forwards an observation-only candidate to SessionActor', async () => {
    const resolved = await resolveNotebookInterpreter({
      discover: async () => ({
        language: 'python',
        interpreters: [
          {
            envId: 'external:/usr/bin/python3',
            language: 'python',
            interpreterPath: '/usr/bin/python3',
            provenance: 'system',
            runnable: false,
            detail: 'not probed before approval',
          },
        ],
        unpinned: [],
      }),
    })
    deepStrictEqual(resolved, {
      ok: true,
      interpreterPath: '/usr/bin/python3',
    })
  })

  await test('notebook execution reports unpinned candidates without probing them', async () => {
    const resolved = await resolveNotebookInterpreter({
      discover: async () => ({
        language: 'python',
        interpreters: [],
        unpinned: [{ candidate: 'python3', reason: 'not absolute' }],
      }),
    })
    strictEqual(resolved.ok, false)
    if (!resolved.ok) {
      ok(resolved.reason.includes('pinned=0 unpinned=1'), resolved.reason)
      ok(!resolved.reason.includes('versionProbed'), resolved.reason)
    }
  })

  // ── 7. IPC surface ─────────────────────────────────────────────

  await test('the three environment channels are allowed by the authority policy', () => {
    for (const channel of [
      'environment:discover',
      'environment:identify',
      'environment:request-admission',
    ]) {
      strictEqual(validateIpcChannel(channel), true, channel)
    }
    strictEqual(validateIpcChannel('environment:provision'), false)
    strictEqual(validateIpcChannel('environment:admit'), false)
  })

  await test('registration wires the channels and fails honestly with no runtime root', async () => {
    const handlers = new Map<string, (event: unknown, ...args: unknown[]) => unknown>()
    const ipc: IpcMainLike = {
      handle(channel, handler) {
        if (handlers.has(channel)) throw new Error(`double registration of ${channel}`)
        handlers.set(channel, handler)
      },
    }
    const safeHandle: SafeHandleFn = (target, channel, handler) => {
      ok(validateIpcChannel(channel), `channel ${channel} must be allowed`)
      target.handle(channel, handler)
    }
    registerScienceIpcHandlers(ipc, {
      safeHandle,
      getLumenBinaryHash: () => 'deadbeef',
      previewStore: {
        async resolveById() {
          return null
        },
      },
      // deliberately no runtimeRoot
    })
    for (const channel of [
      'environment:discover',
      'environment:identify',
      'environment:request-admission',
    ]) {
      ok(handlers.has(channel), `${channel} not registered`)
    }
    const answer = (await handlers.get('environment:request-admission')!({}, {})) as {
      ok: boolean
      reason?: string
    }
    strictEqual(answer.ok, false)
    ok(answer.reason?.includes('not empty'), answer.reason)
  })

  await test('registration with a runtime root keeps identify fail-closed', async () => {
    const handlers = new Map<string, (event: unknown, ...args: unknown[]) => unknown>()
    const ipc: IpcMainLike = {
      handle(channel, handler) {
        handlers.set(channel, handler)
      },
    }
    const safeHandle: SafeHandleFn = (target, channel, handler) => target.handle(channel, handler)
    registerScienceIpcHandlers(ipc, {
      safeHandle,
      getLumenBinaryHash: () => 'deadbeef',
      previewStore: {
        async resolveById() {
          return null
        },
      },
      runtimeRoot,
    })
    const identified = (await handlers.get('environment:identify')!(
      {},
      { kind: 'python', interpreterPath: realPython },
    )) as {
      identified: boolean
      failure?: { code: string }
      authority: string
    }
    strictEqual(identified.identified, false)
    strictEqual(identified.failure?.code, 'actor_probe_required')
    strictEqual(identified.authority, 'SessionActor-required')
  })

  fs.rmSync(scratch, { recursive: true, force: true })
  console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`)
  process.exit(failures > 0 ? 1 : 0)
}

void run()
