// Modified from Open Science (Apache-2.0).
// Upstream: https://github.com/aipoch/open-science @ d8f11e34314f
// Change: Windows fixtures declare platform 'win32' (the pinned-path rule is platform-dependent), and the mixed-platform CRAN fixture now asserts the refusal it used to contradict.
// Per-file diff and digests: docs/provenance/open-science-adoption.json
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { access, readdir } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { describe, expect, it, vi } from 'vitest'

import {
  collapseRscript,
  createProductionHostEnumeration,
  defaultCandidatePaths,
  defaultDiscoveryDeps,
  discoverInterpreters,
  type DiscoveryDeps,
  type HostPathEnumeration
} from './environment-discovery'

/** Hermetic host enumeration: no which/conda/host PATH/framework scans. */
const fixtureHost = (
  overrides: Partial<HostPathEnumeration> = {}
): HostPathEnumeration =>
  createProductionHostEnumeration({
    whichAll: async () => [],
    listCondaPrefixes: async () => [],
    pyLauncherPaths: async () => [],
    commonBinDirs: [],
    readdirSync: (path) => {
      try {
        return readdirSync(path)
      } catch {
        return []
      }
    },
    readdir: async (path) => readdir(path),
    access: async (path) => {
      await access(path)
    },
    exists: existsSync,
    ...overrides
  })

describe('defaultDiscoveryDeps Windows conda R probes', () => {
  it('activates the interpreter own conda prefix for version and jsonlite probes', async () => {
    const prefix = 'C:\\Users\\HM\\OpenScience\\runtime\\envs\\default-r'
    const interpreter = `${prefix}\\Lib\\R\\bin\\R.exe`
    const exec = vi.fn(
      async (
        _file: string,
        args: readonly string[],
        options: { env?: NodeJS.ProcessEnv }
      ): Promise<{ stdout: string; stderr: string }> => {
        const expectedStart = [
          prefix,
          `${prefix}\\Library\\mingw-w64\\bin`,
          `${prefix}\\Library\\usr\\bin`,
          `${prefix}\\Library\\bin`,
          `${prefix}\\Scripts`,
          `${prefix}\\bin`
        ].join(';')
        expect(options.env?.PATH?.split(';').slice(0, 6).join(';')).toBe(expectedStart)
        return args.includes('--version')
          ? { stdout: '', stderr: 'R version 4.4.3 (2025-02-28 ucrt)' }
          : { stdout: 'TRUE', stderr: '' }
      }
    )
    const deps = defaultDiscoveryDeps('C:\\Users\\HM\\OpenScience\\runtime', undefined, {
      platform: 'win32',
      exec
    })

    await expect(deps.probeVersion(interpreter, 'r')).resolves.toBe('4.4.3')
    await expect(deps.rRunnable(interpreter)).resolves.toBe(true)
    expect(exec.mock.calls.map(([file]) => file)).toEqual([
      interpreter,
      `${prefix}\\Lib\\R\\bin\\Rscript.exe`
    ])
  })

  it('does not inject conda activation into an external Windows R installation', async () => {
    const interpreter = 'C:\\Program Files\\R\\R-4.4.3\\bin\\R.exe'
    const exec = vi.fn(
      async (
        _file: string,
        args: readonly string[],
        options: { env?: NodeJS.ProcessEnv }
      ): Promise<{ stdout: string; stderr: string }> => {
        expect(options.env).toBeUndefined()
        return args.includes('--version')
          ? { stdout: '', stderr: 'R version 4.4.3 (2025-02-28 ucrt)' }
          : { stdout: 'TRUE', stderr: '' }
      }
    )
    const deps = defaultDiscoveryDeps('C:\\Users\\HM\\OpenScience\\runtime', undefined, {
      platform: 'win32',
      exec
    })

    await expect(deps.probeVersion(interpreter, 'r')).resolves.toBe('4.4.3')
    await expect(deps.rRunnable(interpreter)).resolves.toBe(true)
  })
})

// Orchestration-only test (real enumerators are injected): dedup by realpath, provenance
// classification, and python-vs-r runnability. Uses fake paths so no real machine is touched.
const makeDeps = (
  paths: string[],
  opts: {
    versions?: Record<string, string>
    rRunnable?: Record<string, boolean>
    realpath?: Record<string, string>
    // The pinned-path rule is platform-dependent; a Windows-path fixture must
    // say so, or a non-Windows host refuses every candidate as unpinned.
    platform?: NodeJS.Platform
  } = {}
): DiscoveryDeps => ({
  candidatePaths: async () => paths,
  probeVersion: async (p) => opts.versions?.[p],
  rRunnable: async (p) => opts.rRunnable?.[p] ?? false,
  realpath: (p) => opts.realpath?.[p] ?? p,
  runtimeRoot: '/rt',
  platform: opts.platform
})

describe('discoverInterpreters', () => {
  it('classifies provenance and dedupes python interpreters by real path', async () => {
    const deps = makeDeps(
      ['/usr/bin/python3', '/rt/envs/default-python/bin/python', '/a/python', '/b/python'],
      {
        versions: {
          '/usr/bin/python3': '3.9.6',
          '/rt/envs/default-python/bin/python': '3.12.4',
          '/a/python': '3.11.0'
        },
        // /a and /b are the same interpreter via symlink → one entry.
        realpath: { '/a/python': '/canon/python', '/b/python': '/canon/python' }
      }
    )
    const found = await discoverInterpreters('python', deps)

    expect(found).toHaveLength(3) // /a and /b collapsed
    const byId = Object.fromEntries(found.map((f) => [f.envId, f]))
    expect(byId['/usr/bin/python3'].provenance).toBe('user-own')
    expect(byId['/rt/envs/default-python/bin/python'].provenance).toBe('app-managed')
    expect(byId['/canon/python'].runnable).toBe(true)
    // A python that does not report a version is not runnable and carries an actionable detail.
    const notPy3 = await discoverInterpreters('python', makeDeps(['/x/python2'], {}))
    expect(notPy3[0].runnable).toBe(false)
    expect(notPy3[0].detail).toMatch(/Python 3/)
  })

  it('keeps the logical default name when discovery sees a short physical directory', async () => {
    const path = '/rt/envs/.p/bin/python'
    const [found] = await discoverInterpreters(
      'python',
      makeDeps([path], { versions: { [path]: '3.12.4' } })
    )

    expect(found.provenance).toBe('app-managed')
    expect(found.condaEnv).toBe('default-python')
    expect(found.label).toBe('conda: default-python')
  })

  it('flags an agent-created named env and marks a conda R needing jsonlite', async () => {
    const deps = makeDeps(['/rt/envs/my-analysis/bin/R', '/opt/miniconda3/envs/bio/bin/R'], {
      versions: {
        '/rt/envs/my-analysis/bin/R': '4.4.1',
        '/opt/miniconda3/envs/bio/bin/R': '4.3.2'
      },
      rRunnable: { '/rt/envs/my-analysis/bin/R': true } // the conda one lacks jsonlite
    })
    const found = await discoverInterpreters('r', deps)
    const byId = Object.fromEntries(found.map((f) => [f.envId, f]))

    expect(byId['/rt/envs/my-analysis/bin/R'].provenance).toBe('agent-created')
    expect(byId['/rt/envs/my-analysis/bin/R'].runnable).toBe(true)
    const condaR = byId['/opt/miniconda3/envs/bio/bin/R']
    expect(condaR.provenance).toBe('user-own')
    expect(condaR.condaEnv).toBe('bio')
    expect(condaR.runnable).toBe(false)
    expect(condaR.detail).toMatch(/jsonlite/)
  })

  it('bounds probe concurrency with a worker pool yet discovers every env in input order', async () => {
    // Many distinct interpreters — enough to overflow the pool and make a spawn storm observable.
    const paths = Array.from({ length: 30 }, (_, i) => `/envs/py-${i}/bin/python`)
    let inFlight = 0
    let maxInFlight = 0
    // A probeVersion that tracks concurrent in-flight calls: overlap is only observable if each call
    // stays open across a real (tiny) delay, so the pool's cap can actually bite.
    const deps: DiscoveryDeps = {
      candidatePaths: async () => paths,
      probeVersion: async (p) => {
        inFlight += 1
        maxInFlight = Math.max(maxInFlight, inFlight)
        await new Promise((r) => setTimeout(r, 5))
        inFlight -= 1
        return `3.${p.match(/py-(\d+)/)![1]}.0`
      },
      rRunnable: async () => false,
      realpath: (p) => p,
      runtimeRoot: '/rt'
    }

    const found = await discoverInterpreters('python', deps)

    // The delay must actually produce overlap, else the cap assertion is vacuous.
    expect(maxInFlight).toBeGreaterThan(1)
    // PROBE_CONCURRENCY is a non-exported module const (currently 8); the worker pool must never let
    // more than that many probes run at once, no matter how many envs are present.
    expect(maxInFlight).toBeLessThanOrEqual(8)
    // No candidate is dropped, and results map 1:1 to the input candidate order (written back by index).
    expect(found).toHaveLength(30)
    expect(found.map((f) => f.interpreterPath)).toEqual(paths)
    expect(found.map((f) => f.version)).toEqual(paths.map((_, i) => `3.${i}.0`))
  })

  it('classifies Windows CRAN R installations as user-own', async () => {
    // Windows CRAN R paths discovered via Program Files should be classified as 'user-own',
    // not 'app-managed' or 'agent-created', since they're user-installed global interpreters.
    //
    // The fixture used to slip a POSIX path in alongside the Windows ones and
    // assert all three survived. Under the LS5-K4 pinned-path rule that is a
    // contradiction: on win32 a `/rt/...` candidate is not an absolute Windows
    // path and MUST be refused. The app-managed classification of runtime-root
    // envs is covered by the python provenance test above on its own platform.
    const unpinned: string[] = []
    const deps = {
      ...makeDeps(
        [
          'C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe',
          'C:\\Program Files (x86)\\R\\R-4.2.0\\bin\\R.exe',
          '/rt/envs/default-r/bin/R'
        ],
        {
          versions: {
            'C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe': '4.4.3',
            'C:\\Program Files (x86)\\R\\R-4.2.0\\bin\\R.exe': '4.2.0'
          },
          rRunnable: {
            'C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe': true,
            'C:\\Program Files (x86)\\R\\R-4.2.0\\bin\\R.exe': true
          },
          platform: 'win32'
        }
      ),
      onUnpinnedCandidate: (candidate: string) => {
        unpinned.push(candidate)
      }
    }
    const found = await discoverInterpreters('r', deps)
    const byId = Object.fromEntries(found.map((f) => [f.envId, f]))

    expect(byId['C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe'].provenance).toBe('user-own')
    expect(byId['C:\\Program Files (x86)\\R\\R-4.2.0\\bin\\R.exe'].provenance).toBe('user-own')
    // The POSIX path is refused on win32, with the refusal REPORTED, not dropped.
    expect(found.map((f) => f.interpreterPath)).not.toContain('/rt/envs/default-r/bin/R')
    expect(unpinned).toContain('/rt/envs/default-r/bin/R')
  })

  it('deduplicates when PATH and CRAN paths resolve to the same R installation', async () => {
    // If a user adds "C:\Program Files\R\R-4.4.3\bin\x64" to PATH, both the PATH scan
    // and the CRAN Program Files scan will find the same R.exe. Realpath dedup should
    // collapse them into a single entry.
    const deps = makeDeps(
      [
        '/usr/bin/R', // from PATH (symlink to CRAN install)
        'C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe', // from CRAN scan
        '/rt/envs/default-r/bin/R' // app-managed
      ],
      {
        versions: {
          '/usr/bin/R': '4.4.3',
          'C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe': '4.4.3',
          '/rt/envs/default-r/bin/R': '4.3.1'
        },
        rRunnable: {
          '/usr/bin/R': true,
          'C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe': true,
          '/rt/envs/default-r/bin/R': true
        },
        // PATH entry is a symlink to the CRAN install
        realpath: {
          '/usr/bin/R': 'C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe'
        }
      }
    )
    const found = await discoverInterpreters('r', deps)

    // Should have exactly 2 entries: one for the deduplicated CRAN R, one for app-managed
    expect(found).toHaveLength(2)
    const byId = Object.fromEntries(found.map((f) => [f.envId, f]))
    // The deduplicated entry should use the canonical path
    expect(byId['C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe']).toBeDefined()
    expect(byId['C:\\Program Files\\R\\R-4.4.3\\bin\\x64\\R.exe'].provenance).toBe('user-own')
    expect(byId['/rt/envs/default-r/bin/R'].provenance).toBe('app-managed')
  })
})

describe('collapseRscript', () => {
  it('drops a Rscript whose sibling R is also present (one R install = one env)', () => {
    expect(collapseRscript(['/usr/local/bin/R', '/usr/local/bin/Rscript']).sort()).toEqual([
      '/usr/local/bin/R'
    ])
  })

  it('keeps a lone Rscript with no sibling R, and never touches python/R entries', () => {
    expect(collapseRscript(['/opt/only/bin/Rscript'])).toEqual(['/opt/only/bin/Rscript'])
    expect(
      collapseRscript(['/usr/bin/python3', '/usr/local/bin/R', '/usr/local/bin/Rscript']).sort()
    ).toEqual(['/usr/bin/python3', '/usr/local/bin/R'])
  })

  it('collapses the Windows R.exe / Rscript.exe pair too', () => {
    expect(collapseRscript(['C:\\R\\bin\\R.exe', 'C:\\R\\bin\\Rscript.exe'])).toEqual([
      'C:\\R\\bin\\R.exe'
    ])
  })
})

describe('defaultCandidatePaths (targeted enumeration)', () => {
  it('includes manually-added interpreters and the app-managed default, and collapses R/Rscript', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-disc-'))
    // App-managed default-r on disk under runtime/envs.
    const rPrefix = join(root, 'envs', 'default-r', 'bin')
    mkdirSync(rPrefix, { recursive: true })
    writeFileSync(join(rPrefix, 'R'), 'x')
    writeFileSync(join(rPrefix, 'Rscript'), 'x') // sibling — must be collapsed away
    // A manually-added R that is not on PATH.
    const manualR = join(root, 'custom', 'R')
    mkdirSync(join(root, 'custom'), { recursive: true })
    writeFileSync(manualR, 'x')

    // Hermetic host: no real which/conda (CI used to hang here for 5s).
    const paths = await defaultCandidatePaths(
      root,
      () => [manualR],
      undefined,
      fixtureHost()
    )('r')

    expect(paths).toContain(join(rPrefix, 'R'))
    expect(paths).toContain(manualR)
    // The app default's Rscript sibling is collapsed (only its R remains).
    expect(paths).not.toContain(join(rPrefix, 'Rscript'))
    rmSync(root, { recursive: true, force: true })
  })

  it('returns no candidates when host enumeration and manual paths are empty', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-empty-'))
    const paths = await defaultCandidatePaths(
      root,
      () => [],
      undefined,
      fixtureHost()
    )('python')
    expect(paths).toEqual([])
    rmSync(root, { recursive: true, force: true })
  })

  it('dedupes repeated which/manual candidates and keeps order stable', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-dedupe-'))
    const py = join(root, 'bin', 'python3')
    mkdirSync(join(root, 'bin'), { recursive: true })
    writeFileSync(py, 'x')
    const paths = await defaultCandidatePaths(
      root,
      () => [py, py],
      undefined,
      fixtureHost({
        whichAll: async () => [py, py],
        commonBinDirs: [join(root, 'bin')]
      })
    )('python')
    expect(paths.filter((p) => p === py)).toHaveLength(1)
    const again = await defaultCandidatePaths(
      root,
      () => [py],
      undefined,
      fixtureHost({ whichAll: async () => [py] })
    )('python')
    expect(again).toEqual(paths)
    rmSync(root, { recursive: true, force: true })
  })

  it('preserves manual then PATH then known-location priority instead of sorting paths', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-priority-'))
    // Deliberately choose names whose lexicographic order is the reverse of
    // product priority. A trailing `.sort()` would make this test fail.
    const manual = join(root, 'z-manual', 'python3')
    const fromPath = join(root, 'm-path', 'python3')
    const knownDir = join(root, 'a-known')
    const known = join(knownDir, 'python3')
    for (const candidate of [manual, fromPath, known]) {
      mkdirSync(join(candidate, '..'), { recursive: true })
      writeFileSync(candidate, 'x')
    }

    const paths = await defaultCandidatePaths(
      root,
      () => [manual],
      undefined,
      fixtureHost({
        whichAll: async (name) => (name === 'python3' ? [fromPath] : []),
        commonBinDirs: [knownDir]
      })
    )('python')

    expect(paths).toEqual([manual, fromPath, known])
    rmSync(root, { recursive: true, force: true })
  })

  it('refuses illegal relative manual paths without host enumeration', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-illegal-'))
    const refused: string[] = []
    const paths = await defaultCandidatePaths(
      root,
      () => ['python3', './local-python'],
      (c) => refused.push(c),
      fixtureHost()
    )('python')
    expect(paths).toEqual([])
    expect(refused).toEqual(['python3', './local-python'])
    rmSync(root, { recursive: true, force: true })
  })

  it('treats failed conda as empty without timing out', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-conda-'))
    let condaCalls = 0
    const paths = await defaultCandidatePaths(
      root,
      () => [],
      undefined,
      fixtureHost({
        listCondaPrefixes: async () => {
          condaCalls += 1
          // Production enumerator catches conda failures and returns [].
          // Fixtures inject the same empty outcome — never a hang.
          return []
        }
      })
    )('python')
    expect(condaCalls).toBe(1)
    expect(paths).toEqual([])
    rmSync(root, { recursive: true, force: true })
  })

  it('which failure yields empty contribution (best-effort)', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-which-'))
    const paths = await defaultCandidatePaths(
      root,
      () => [],
      undefined,
      fixtureHost({
        whichAll: async () => [] // production returns [] on which failure
      })
    )('r')
    expect(paths).toEqual([])
    rmSync(root, { recursive: true, force: true })
  })
})

describe('defaultCandidatePaths Windows CRAN R detection', () => {
  it('discovers standard CRAN R installations in Program Files', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-cran-'))
    const programFiles = join(root, 'Program Files', 'R')
    const r443Dir = join(programFiles, 'R-4.4.3', 'bin', 'x64')
    mkdirSync(r443Dir, { recursive: true })
    writeFileSync(join(r443Dir, 'R.exe'), 'x')
    writeFileSync(join(r443Dir, 'Rscript.exe'), 'x')

    try {
      const paths = await defaultCandidatePaths(
        root,
        undefined,
        undefined,
        fixtureHost({
          platform: 'win32',
          env: { ProgramFiles: join(root, 'Program Files') },
          homedir: () => root
        })
      )('r')
      expect(paths).toContain(join(r443Dir, 'R.exe'))
      expect(paths).not.toContain(join(r443Dir, 'Rscript.exe'))
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('discovers multiple R versions and prefers 64-bit over fallback layout', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-multi-r-'))
    const programFiles = join(root, 'Program Files', 'R')

    // R 4.4.3 with 64-bit layout
    const r443x64 = join(programFiles, 'R-4.4.3', 'bin', 'x64')
    mkdirSync(r443x64, { recursive: true })
    writeFileSync(join(r443x64, 'R.exe'), 'x')

    // R 4.3.0 with fallback layout only
    const r430bin = join(programFiles, 'R-4.3.0', 'bin')
    mkdirSync(r430bin, { recursive: true })
    writeFileSync(join(r430bin, 'R.exe'), 'x')

    try {
      const paths = await defaultCandidatePaths(
        root,
        undefined,
        undefined,
        fixtureHost({
          platform: 'win32',
          env: { ProgramFiles: join(root, 'Program Files') },
          homedir: () => root
        })
      )('r')
      expect(paths).toContain(join(r443x64, 'R.exe'))
      expect(paths).toContain(join(r430bin, 'R.exe'))
      expect(paths.length).toBeGreaterThanOrEqual(2)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('checks Program Files (x86) and LOCALAPPDATA for R installations', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-r-paths-'))
    const programFilesx86 = join(root, 'Program Files (x86)', 'R')
    const localAppData = join(root, 'AppData', 'Local', 'Programs', 'R')

    // 32-bit R in Program Files (x86)
    const r32bit = join(programFilesx86, 'R-4.2.0', 'bin')
    mkdirSync(r32bit, { recursive: true })
    writeFileSync(join(r32bit, 'R.exe'), 'x')

    // User-local R in LOCALAPPDATA
    const rLocal = join(localAppData, 'R-4.4.0', 'bin', 'x64')
    mkdirSync(rLocal, { recursive: true })
    writeFileSync(join(rLocal, 'R.exe'), 'x')

    try {
      const paths = await defaultCandidatePaths(
        root,
        undefined,
        undefined,
        fixtureHost({
          platform: 'win32',
          env: {
            'ProgramFiles(x86)': join(root, 'Program Files (x86)'),
            LOCALAPPDATA: join(root, 'AppData', 'Local')
          },
          homedir: () => root
        })
      )('r')
      expect(paths).toContain(join(r32bit, 'R.exe'))
      expect(paths).toContain(join(rLocal, 'R.exe'))
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('ignores non-versioned directories in R root', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-r-invalid-'))
    const programFiles = join(root, 'Program Files', 'R')

    // Valid R version
    const rValid = join(programFiles, 'R-4.4.1', 'bin', 'x64')
    mkdirSync(rValid, { recursive: true })
    writeFileSync(join(rValid, 'R.exe'), 'x')

    // Invalid directory names (should be skipped)
    mkdirSync(join(programFiles, 'docs'), { recursive: true })
    mkdirSync(join(programFiles, 'R-alpha'), { recursive: true })
    mkdirSync(join(programFiles, '4.4.1'), { recursive: true })

    try {
      const paths = await defaultCandidatePaths(
        root,
        undefined,
        undefined,
        fixtureHost({
          platform: 'win32',
          env: { ProgramFiles: join(root, 'Program Files') },
          homedir: () => root
        })
      )('r')
      expect(paths).toContain(join(rValid, 'R.exe'))
      expect(paths.filter((p) => p.includes('docs')).length).toBe(0)
      expect(paths.filter((p) => p.includes('R-alpha')).length).toBe(0)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })

  it('does not run CRAN R detection for Python or on non-Windows platforms', async () => {
    const root = mkdtempSync(join(tmpdir(), 'os-no-r-'))
    const programFiles = join(root, 'Program Files', 'R')
    mkdirSync(join(programFiles, 'R-4.4.3', 'bin', 'x64'), { recursive: true })
    writeFileSync(join(programFiles, 'R-4.4.3', 'bin', 'x64', 'R.exe'), 'x')

    try {
      // On Windows: Python should not scan R paths
      const pythonPaths = await defaultCandidatePaths(
        root,
        undefined,
        undefined,
        fixtureHost({
          platform: 'win32',
          env: { ProgramFiles: join(root, 'Program Files') },
          homedir: () => root
        })
      )('python')
      expect(pythonPaths.filter((p) => p.includes('R-4.4.3')).length).toBe(0)

      // On non-Windows: R should not enumerate CRAN Program Files roots
      const rPaths = await defaultCandidatePaths(
        root,
        undefined,
        undefined,
        fixtureHost({
          platform: 'darwin',
          env: { ProgramFiles: join(root, 'Program Files') },
          homedir: () => root
        })
      )('r')
      expect(rPaths.filter((p) => p.includes('R-4.4.3')).length).toBe(0)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })
})
