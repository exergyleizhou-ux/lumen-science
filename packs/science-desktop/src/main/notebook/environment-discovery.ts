// Modified from Open Science (Apache-2.0) — statement of changes, §4(b).
// Upstream: https://github.com/aipoch/open-science @ d8f11e34314f,
//           src/main/notebook/environment-discovery.ts
// Per-file digests: docs/provenance/open-science-adoption.json
//
// WHAT THIS IS IN LUMEN
// ---------------------
// A DRIVEN adapter. It enumerates interpreters that already exist on the
// machine and reports what it observed. It does not start a kernel, does not
// choose a runtime, and does not answer whether anything may execute — that is
// the Rust SessionActor's answer, produced by its own probe
// (agent/crates/codegen/xai-grok-science/src/workflow/admission.rs). Nothing
// here is an input to a permission; everything here is an input to a question.
//
// WHAT LUMEN CHANGED, and why (LS5-K4)
// ------------------------------------
// 1. Unpinned candidates are refused, not returned. Upstream fed
//    `manualPaths` (a Settings-catalog string the user typed) straight into the
//    candidate set, so a bare `python3` or a `./python3` could become an
//    "environment". A bare name is not an identity: it resolves through PATH,
//    so the same spelling names a different binary in another process, and a
//    reproducibility record built on one is worthless. `partitionCandidates`
//    now splits them out and `onUnpinnedCandidate` reports each rejection with
//    a reason rather than dropping it silently — a discarded candidate the user
//    explicitly added must be explainable. This mirrors the engine's
//    `interpreter_path_not_absolute` rejection so the two ends agree.
// 2. `DiscoveredInterpreter.runnable` is retained (the Settings cards need it)
//    but is documented at its definition as a READINESS signal for the UI, and
//    is explicitly not an admission verdict. Lumen never routes it into an
//    execution decision; see environment/interpreter-identity.ts, which
//    re-derives facts from disk instead of trusting anything computed here.
//
// The enumeration itself — PATH, framework installs, pyenv, conda roots, the
// Windows launcher, the app's own runtime/envs — is upstream's and is adopted
// as-is. It is careful, targeted (never a disk walk), and it is mechanics.
import { execFile } from 'node:child_process'
import { existsSync, readdirSync, realpathSync } from 'node:fs'
import { access, readdir } from 'node:fs/promises'
import { homedir } from 'node:os'
import { join, win32 } from 'node:path'
import { promisify } from 'node:util'

import type { NotebookLanguage } from '../../shared/notebook'
import type { DiscoveredInterpreter, EnvProvenance } from '../../shared/notebook-runtime'
import { probeInterpreterVersion } from './python-command'
import { parseRVersion, rHasJsonlite } from './r-command'
import {
  condaActivatedPath,
  DEFAULT_PY_ENV,
  DEFAULT_R_ENV,
  envPrefix,
  logicalEnvNameFromDirectory,
  pythonBin,
  rBin
} from './runtime-paths'

export type { DiscoveredInterpreter, EnvProvenance }

const execFileAsync = promisify(execFile)

// Shared options for every discovery subprocess: a hard timeout so a wedged tool (a hung conda, a
// stuck interpreter, an unresponsive `which`) can NEVER hang environment discovery, and windowsHide so
// no console window flashes. NB: discovery never uses shell:true — execFile passes argv directly, so a
// path with spaces (C:\Program Files\…) or shell metacharacters is safe and cannot inject.
const PROBE_TIMEOUT_MS = 10_000
const PROBE_EXEC_OPTS = { timeout: PROBE_TIMEOUT_MS, windowsHide: true } as const
// Upper bound on concurrent interpreter probes so a machine with many envs doesn't spawn a burst of
// subprocesses / exhaust file descriptors; fast enough to keep discovery responsive.
const PROBE_CONCURRENCY = 8
const isWin = (): boolean => process.platform === 'win32'

// Injectable so discovery is unit-testable without a real machine. The real defaults enumerate PATH,
// common install dirs, pyenv, conda/mamba envs, and the app's own runtime/envs.
export type DiscoveryDeps = {
  // Absolute candidate interpreter paths for a language, across all sources (may contain dupes/misses;
  // the orchestrator realpath-dedupes and drops non-existent ones).
  candidatePaths: (language: NotebookLanguage) => Promise<string[]>
  // `<interp> --version` → version string (e.g. "3.12.4" / "4.4.1"), or undefined if it doesn't run.
  probeVersion: (interpreterPath: string, language: NotebookLanguage) => Promise<string | undefined>
  // Whether an R interpreter can actually back the kernel loop (jsonlite + protocol). Python
  // runnability is derived from a valid Python-3 version instead.
  rRunnable: (interpreterPath: string) => Promise<boolean>
  // Resolve a path to its canonical form for identity/dedup; tolerate a missing path (return as-is).
  realpath: (p: string) => string
  // Platform the pinned-path rule judges candidates against. Optional; defaults to the running
  // process. Injectable because LS5-K4 re-applies the rule INSIDE discoverInterpreters, and a
  // Windows fixture judged by a macOS process.platform silently loses all its candidates —
  // which is exactly how the Windows CRAN R test broke on every non-Windows machine.
  platform?: NodeJS.Platform
  // App runtime root (<storageRoot>/runtime); used to classify provenance of an interpreter by whether
  // it lives under runtime/envs and whether it is a default (app-managed) vs a named (agent-created) env.
  runtimeRoot: string
  // LS5-K4. Called once per candidate refused for not being a pinned absolute path. Optional because
  // most sources cannot produce one; supplied by callers that surface the reason to a user (a manually
  // added interpreter that vanishes from the list with no explanation is a support ticket).
  onUnpinnedCandidate?: (candidate: string, reason: string) => void
}

// LS5-K4. A candidate is usable as an identity only if it names one file on this machine and keeps
// naming it in another process. PATH-relative and directory-relative spellings do neither: `python3`
// depends on the caller's PATH, `./python3` on the caller's cwd, and both can be re-pointed by
// anything that can write a directory earlier in the search order.
//
// Windows note: win32.isAbsolute accepts `\foo` (root-relative on the current drive), which is not an
// identity either, so the drive/UNC form is required explicitly.
export const isPinnedInterpreterPath = (
  candidate: string,
  platform: NodeJS.Platform = process.platform
): boolean => {
  if (candidate.trim() !== candidate || candidate.length === 0) return false
  if (candidate.includes('\0')) return false
  if (platform === 'win32') {
    return /^([A-Za-z]:[\\/]|\\\\)/.test(candidate)
  }
  return candidate.startsWith('/')
}

export type CandidatePartition = {
  pinned: string[]
  unpinned: { candidate: string; reason: string }[]
}

// Splits a candidate list into the ones that can carry an identity and the ones that cannot, keeping
// a reason for each rejection. Exported so the rejection is testable and so a caller can report it
// rather than discovering an empty list with no explanation.
export const partitionCandidates = (
  candidates: readonly string[],
  platform: NodeJS.Platform = process.platform
): CandidatePartition => {
  const pinned: string[] = []
  const unpinned: { candidate: string; reason: string }[] = []
  for (const candidate of candidates) {
    if (isPinnedInterpreterPath(candidate, platform)) {
      pinned.push(candidate)
      continue
    }
    unpinned.push({
      candidate,
      reason:
        `interpreter '${candidate}' is not an absolute path; a PATH-relative or ` +
        'cwd-relative interpreter is not a pinned identity and cannot be reproduced'
    })
  }
  return { pinned, unpinned }
}

const safeRealpath = (p: string): string => {
  try {
    return realpathSync(p)
  } catch {
    return p
  }
}

// Interpreter basenames per language and platform.
const interpreterNames = (
  language: NotebookLanguage,
  platform: NodeJS.Platform = process.platform
): string[] => {
  const win = platform === 'win32'
  if (language === 'python') return win ? ['python.exe', 'python3.exe'] : ['python3', 'python']
  return win ? ['R.exe', 'Rscript.exe'] : ['R', 'Rscript']
}

/**
 * Host enumeration boundary for defaultCandidatePaths.
 *
 * Unit tests inject fixed which/conda/PATH/filesystem fixtures so discovery
 * never waits on a real host conda/which (CI 5s timeout root cause).
 * Production uses createProductionHostEnumeration().
 */
export type HostPathEnumeration = {
  platform: NodeJS.Platform
  env: NodeJS.ProcessEnv
  homedir: () => string
  exists: (path: string) => boolean
  readdirSync: (path: string) => string[]
  readdir: (path: string) => Promise<string[]>
  access: (path: string) => Promise<void>
  whichAll: (name: string) => Promise<string[]>
  listCondaPrefixes: () => Promise<string[]>
  pyLauncherPaths: () => Promise<string[]>
  commonBinDirs: string[]
}

// `which -a <name>` (POSIX) / `where <name>` (Windows) → existing absolute paths, best-effort.
const productionWhichAll = async (name: string): Promise<string[]> => {
  try {
    const { stdout } = isWin()
      ? await execFileAsync('where', [name], PROBE_EXEC_OPTS)
      : await execFileAsync('which', ['-a', name], PROBE_EXEC_OPTS)
    return stdout
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && existsSync(line))
  } catch {
    return []
  }
}

// conda/mamba env prefixes, best-effort: `conda env list --json`, else scan common install roots.
const productionListCondaPrefixes = async (): Promise<string[]> => {
  const prefixes = new Set<string>()
  for (const bin of ['conda', 'mamba', 'micromamba']) {
    try {
      const { stdout } = await execFileAsync(bin, ['env', 'list', '--json'], PROBE_EXEC_OPTS)
      const parsed = JSON.parse(stdout) as { envs?: unknown }
      if (Array.isArray(parsed.envs)) {
        for (const env of parsed.envs) if (typeof env === 'string') prefixes.add(env)
      }
      break
    } catch {
      // try the next tool
    }
  }
  // Scan known conda/mamba/micromamba install ROOTS directly — essential in a packaged GUI app where
  // conda itself isn't on PATH (so `env list` above found nothing). Each root's base prefix + every
  // dir under its envs/ is a candidate; a non-existent root is simply skipped.
  const home = homedir()
  // Home-based roots exist on every platform (installers default to the user profile).
  const homeRoots = ['miniconda3', 'anaconda3', 'miniforge3', 'mambaforge', 'micromamba'].map((d) =>
    join(home, d)
  )
  // Plus per-platform SYSTEM install locations.
  const systemRoots = isWin()
    ? [
        'C:\\ProgramData\\miniconda3',
        'C:\\ProgramData\\anaconda3',
        'C:\\ProgramData\\miniforge3',
        'C:\\miniconda3',
        'C:\\anaconda3',
        join(process.env.LOCALAPPDATA ?? join(home, 'AppData', 'Local'), 'miniconda3'),
        join(process.env.LOCALAPPDATA ?? join(home, 'AppData', 'Local'), 'anaconda3')
      ]
    : process.platform === 'darwin'
      ? [
          '/opt/miniconda3',
          '/opt/anaconda3',
          '/opt/homebrew/anaconda3',
          '/opt/homebrew/Caskroom/miniconda/base',
          '/opt/homebrew/Caskroom/miniforge/base'
        ]
      : [
          '/opt/conda', // common in Linux/Docker images
          '/opt/miniconda3',
          '/opt/anaconda3',
          '/usr/local/miniconda3',
          '/usr/local/anaconda3'
        ]
  const roots = [...homeRoots, ...systemRoots]
  for (const root of roots) {
    if (existsSync(root)) prefixes.add(root)
    const envsDir = join(root, 'envs')
    try {
      for (const name of readdirSync(envsDir)) prefixes.add(join(envsDir, name))
    } catch {
      // no envs dir
    }
  }
  return [...prefixes]
}

// Windows `py -0p`: the launcher lists installed pythons; each line ends with the interpreter path.
// Best-effort (parsing is loose; on-device verification pending).
const productionPyLauncherPaths = async (): Promise<string[]> => {
  try {
    const { stdout } = await execFileAsync('py', ['-0p'], PROBE_EXEC_OPTS)
    return stdout
      .split('\n')
      .map((line) => {
        const match = line.match(/([A-Za-z]:\\[^\s*]+python\.exe)\s*$/i)
        return match ? match[1] : undefined
      })
      .filter((p): p is string => p !== undefined && existsSync(p))
  } catch {
    return []
  }
}

/** Production host enumeration — real which/conda/PATH/filesystem. */
export const createProductionHostEnumeration = (
  overrides: Partial<HostPathEnumeration> = {}
): HostPathEnumeration => ({
  platform: process.platform,
  env: process.env,
  homedir,
  exists: existsSync,
  readdirSync,
  readdir: (path) => readdir(path),
  access: (path) => access(path).then(() => undefined),
  whichAll: productionWhichAll,
  listCondaPrefixes: productionListCondaPrefixes,
  pyLauncherPaths: productionPyLauncherPaths,
  commonBinDirs: ['/usr/bin', '/usr/local/bin', '/opt/homebrew/bin'],
  ...overrides
})

// The interpreter path for a language inside a conda-style env prefix.
const prefixInterpreter = (
  prefix: string,
  language: NotebookLanguage,
  platformIsWin: boolean = isWin()
): string =>
  language === 'python'
    ? platformIsWin
      ? join(prefix, 'python.exe')
      : join(prefix, 'bin', 'python')
    : rBin(prefix)

// A conda-forge Windows R interpreter lives at <prefix>\Lib\R\bin\R[script].exe and depends on
// DLLs in <prefix>\Library\bin. Return only that interpreter's own prefix: external CRAN R paths do
// not match this layout, and an external conda R must never receive the app-managed prefix.
export const windowsCondaPrefixForR = (
  interpreterPath: string,
  platform: NodeJS.Platform = process.platform
): string | undefined => {
  if (platform !== 'win32') return undefined
  const normalized = win32.normalize(interpreterPath)
  const match = normalized.match(/^(.*)\\Lib\\R\\bin\\R(?:script)?\.exe$/i)
  return match?.[1]
}

// Default real enumeration: PATH + common dirs + pyenv + conda envs + the app's own runtime/envs, plus
// any manually-added interpreter paths from the Settings catalog (so a picked interpreter that is not
// on PATH / in a conda root still surfaces as a card). `manualPaths` is a sync getter over a settings
// snapshot; a missing/failed lookup contributes nothing.
//
// Host enumeration (which/conda/filesystem roots) is injectable via `host` so unit tests never touch
// the developer's real machine. Production omits `host` and uses createProductionHostEnumeration().
export const defaultCandidatePaths =
  (
    runtimeRoot: string,
    manualPaths?: (language: NotebookLanguage) => string[],
    // LS5-K4: reports a manually-added interpreter refused for not being absolute.
    onUnpinnedCandidate?: (candidate: string, reason: string) => void,
    host: HostPathEnumeration = createProductionHostEnumeration()
  ) =>
  async (language: NotebookLanguage): Promise<string[]> => {
    const names = interpreterNames(language, host.platform)
    const found = new Set<string>()
    const platformIsWin = host.platform === 'win32'

    // Targeted probes of KNOWN interpreter locations — never a recursive filesystem walk. A packaged
    // GUI app inherits a minimal PATH (not the user's shell), so `which` alone finds almost nothing;
    // we must also check the well-known install dirs, framework versions, and conda roots directly, or
    // a user's real R / Python / conda envs go undetected. Each is an existsSync of a specific path.

    // Manually-added interpreters from the Settings catalog. LS5-K4: this is the only user-authored
    // source, so it is the only one that can contain a bare name; unpinned entries are refused here
    // with a reason instead of becoming an environment nobody can reproduce.
    const manual = partitionCandidates(manualPaths?.(language) ?? [], host.platform)
    for (const p of manual.pinned) found.add(p)
    for (const { candidate, reason } of manual.unpinned) onUnpinnedCandidate?.(candidate, reason)

    // On PATH (`which -a` / `where`) — the happy path when launched from a shell.
    for (const name of names) for (const p of await host.whichAll(name)) found.add(p)

    // Well-known install bin dirs (Homebrew, /usr/local, /usr/bin) — reached even without a shell PATH.
    for (const dir of host.commonBinDirs)
      for (const name of names) {
        const p = join(dir, name)
        if (host.exists(p)) found.add(p)
      }

    // macOS framework installs (python.org Python, CRAN R): one interpreter per versioned Resources dir.
    if (host.platform === 'darwin') {
      const frameworkGlobs =
        language === 'python'
          ? '/Library/Frameworks/Python.framework/Versions'
          : '/Library/Frameworks/R.framework/Versions'
      try {
        for (const ver of host.readdirSync(frameworkGlobs)) {
          const p = prefixInterpreter(
            join(frameworkGlobs, ver, language === 'r' ? 'Resources' : ''),
            language,
            platformIsWin
          )
          if (host.exists(p)) found.add(p)
        }
      } catch {
        // no framework dir
      }
    }

    // pyenv versions (python only): pyenv shims are rarely on a GUI app's PATH.
    if (language === 'python') {
      const versionsDir = join(host.homedir(), '.pyenv', 'versions')
      try {
        for (const ver of host.readdirSync(versionsDir)) {
          const p = join(versionsDir, ver, 'bin', 'python')
          if (host.exists(p)) found.add(p)
        }
      } catch {
        // no pyenv
      }
    }

    // conda / mamba / micromamba envs: `env list --json` when a tool is reachable, else the conda-root
    // scan inside listCondaPrefixes (so envs are found even when conda itself is off the GUI PATH).
    for (const prefix of await host.listCondaPrefixes()) {
      const p = prefixInterpreter(prefix, language, platformIsWin)
      if (host.exists(p)) found.add(p)
    }

    // Windows Python launcher: `py -0p` lists installed interpreters' paths.
    if (language === 'python' && platformIsWin)
      for (const p of await host.pyLauncherPaths()) found.add(p)

    // Windows CRAN R standard installations: check Program Files and user-local directories for versioned
    // R installs (R-x.y.z). CRAN R doesn't register with a launcher like Python's `py`, so we enumerate
    // the standard install roots directly. Each version may have 64-bit (bin/x64/R.exe, most common) or
    // fallback (bin/R.exe) layouts.
    if (language === 'r' && platformIsWin) {
      const home = host.homedir()
      const programFiles = [
        host.env.ProgramFiles ?? 'C:\\Program Files',
        host.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)',
        join(host.env.LOCALAPPDATA ?? join(home, 'AppData', 'Local'), 'Programs')
      ]
      for (const installRoot of programFiles) {
        const rRoot = join(installRoot, 'R')
        let entries: string[]
        try {
          entries = await host.readdir(rRoot)
        } catch (err: unknown) {
          // Discovery is best-effort: absorb all errors and continue. ENOENT/EACCES/EPERM are expected
          // on locked-down corporate machines, but unexpected errors (EIO, ENOTDIR, EMFILE) can also
          // occur (e.g., network-mapped Program Files with dropped shares). Log unexpected codes for
          // observability but do not reject the entire Promise.all that would block Python discovery too.
          const code = (err as NodeJS.ErrnoException).code
          if (code !== 'ENOENT' && code !== 'EACCES' && code !== 'EPERM') {
            console.warn('[cran-r] unexpected error scanning', rRoot, code)
          }
          continue
        }
        for (const ver of entries) {
          // Match R-x.y.z or R-x.y.z-suffix (e.g., R-4.2.0-ucrt for CRAN's UCRT builds)
          if (!/^R-\d+\.\d+\.\d+(-\w+)?$/.test(ver)) continue
          // Try 64-bit first (standard since R 4.2), then fallback to bin/R.exe.
          const candidates = [
            join(rRoot, ver, 'bin', 'x64', 'R.exe'),
            join(rRoot, ver, 'bin', 'R.exe')
          ]
          for (const p of candidates) {
            try {
              await host.access(p)
              found.add(p)
              break
            } catch {
              // Candidate doesn't exist, try next
            }
          }
        }
      }
    }

    // The app's own envs under runtime/envs: the default(s) AND any agent-created named env, so a conda
    // env the agent made with manage_environments is discovered and therefore bindable. Scan every
    // subdir for THIS language's interpreter; classify() labels default -> app-managed, named ->
    // agent-created.
    const appEnvsDir = join(runtimeRoot, 'envs')
    try {
      for (const directory of host.readdirSync(appEnvsDir)) {
        const name = logicalEnvNameFromDirectory(directory)
        const prefix = join(appEnvsDir, directory)
        if (prefix !== envPrefix(runtimeRoot, name)) continue
        const p = language === 'python' ? pythonBin(prefix) : rBin(prefix)
        if (host.exists(p)) found.add(p)
      }
    } catch {
      // No runtime/envs dir yet (first run) — nothing app-owned to add.
    }

    // R and Rscript are two binaries of ONE R install; collapse to a single card (the R binary). The
    // launcher/probe derives the sibling Rscript when needed (see rscriptFor), so nothing is lost.
    //
    // Set preserves insertion order. That order is product behavior, not an
    // implementation detail: an explicit Settings choice must stay ahead of
    // PATH, which must stay ahead of well-known system locations. The notebook
    // executor selects the first runnable candidate. Lexicographic sorting here
    // would therefore silently override the user's explicit interpreter.
    return collapseRscript([...found])
  }

// Drops a `Rscript` candidate when its sibling `R` (same dir) is also a candidate, so a detected R
// installation surfaces as one environment, not a duplicate R + Rscript pair. A lone Rscript with no
// sibling R is kept (still a usable R runtime). Exported for unit tests.
export const collapseRscript = (paths: string[]): string[] => {
  const set = new Set(paths)
  return paths.filter((p) => {
    const base = p.split(/[/\\]/).pop() ?? ''
    if (!/^Rscript(\.exe)?$/i.test(base)) return true
    const siblingR = p.replace(
      /Rscript(\.exe)?$/i,
      (_m, ext: string | undefined) => `R${ext ?? ''}`
    )
    return !set.has(siblingR)
  })
}

// Classifies an interpreter by whether it lives under runtime/envs (app-owned) and, if so, whether it
// is a default (app-managed) or a named env (agent-created); anything else is the user's own.
const classify = (interpreterPath: string, runtimeRoot: string): EnvProvenance => {
  const envsRoot = safeRealpath(join(runtimeRoot, 'envs'))
  const real = safeRealpath(interpreterPath)
  if (!real.startsWith(envsRoot + '/') && !real.startsWith(envsRoot + '\\')) return 'user-own'
  const rest = real.slice(envsRoot.length + 1)
  const envName = logicalEnvNameFromDirectory(rest.split(/[/\\]/)[0])
  return envName === DEFAULT_PY_ENV ||
    envName === DEFAULT_R_ENV ||
    envName.startsWith(`${DEFAULT_PY_ENV}-`) ||
    envName.startsWith(`${DEFAULT_R_ENV}-`)
    ? 'app-managed'
    : 'agent-created'
}

const condaEnvName = (interpreterPath: string): string | undefined => {
  const parts = safeRealpath(interpreterPath).split(/[/\\]/)
  const idx = parts.lastIndexOf('envs')
  return idx >= 0 && idx + 1 < parts.length
    ? logicalEnvNameFromDirectory(parts[idx + 1])
    : undefined
}

// Enumerate pinned candidate paths without executing them. This is the product
// path used by the environment admission UI: version, digest and runnability
// remain unknown until the SessionActor receives an Allow decision and probes.
export const enumerateInterpreterCandidates = async (
  language: NotebookLanguage,
  deps: DiscoveryDeps
): Promise<DiscoveredInterpreter[]> => {
  const seen = new Set<string>()
  const results: DiscoveredInterpreter[] = []
  const { pinned, unpinned } = partitionCandidates(
    await deps.candidatePaths(language),
    deps.platform ?? process.platform
  )
  for (const { candidate, reason } of unpinned) deps.onUnpinnedCandidate?.(candidate, reason)
  for (const candidate of pinned) {
    const envId = deps.realpath(candidate)
    if (seen.has(envId)) continue
    seen.add(envId)
    const conda = condaEnvName(candidate)
    results.push({
      language,
      provenance: classify(candidate, deps.runtimeRoot),
      envId,
      interpreterPath: candidate,
      label: conda ? `conda: ${conda}` : candidate,
      runnable: false,
      condaEnv: conda,
      detail: 'Candidate only; SessionActor admission has not probed this interpreter',
    })
  }
  return results
}

// Legacy readiness discovery for notebook registry callers that explicitly
// need UI readiness. It must not be used by kernel admission: it executes each
// candidate before the user has approved a probe.
export const discoverInterpreters = async (
  language: NotebookLanguage,
  deps: DiscoveryDeps
): Promise<DiscoveredInterpreter[]> => {
  // Dedup by real path FIRST, then probe unique candidates with BOUNDED concurrency. Each probe spawns
  // subprocesses (a `--version` probe, plus a jsonlite probe for R); serial made discovery scale with
  // the number of interpreters (slow with many conda envs), but an unbounded Promise.all over dozens of
  // candidates would fan out too many processes/file descriptors at once. A small worker pool keeps it
  // fast without a spawn storm. Order is preserved (results written back at each candidate's index).
  //
  // LS5-K4: the pinned-path rule is re-applied here rather than trusted from `candidatePaths`. Deps
  // are injectable, so this function must hold the invariant itself: no unpinned path reaches a probe,
  // and none appears in the result.
  const seen = new Set<string>()
  const unique: { path: string; envId: string }[] = []
  const { pinned, unpinned } = partitionCandidates(
    await deps.candidatePaths(language),
    deps.platform ?? process.platform
  )
  for (const { candidate, reason } of unpinned) deps.onUnpinnedCandidate?.(candidate, reason)
  for (const path of pinned) {
    const envId = deps.realpath(path)
    if (seen.has(envId)) continue
    seen.add(envId)
    unique.push({ path, envId })
  }

  const probe = async ({
    path,
    envId
  }: {
    path: string
    envId: string
  }): Promise<DiscoveredInterpreter> => {
    const version = await deps.probeVersion(path, language)
    const provenance = classify(path, deps.runtimeRoot)
    const conda = condaEnvName(path)
    // LS5-K4: `runnable` below is a UI READINESS signal — "this R has jsonlite", "this is a Python 3"
    // — and it is deliberately not routed into any execution decision in Lumen. Admission is decided
    // by the engine's own probe from its own re-derived facts; a boolean computed here would be a
    // second authority's verdict travelling under the name of an observation.
    let runnable: boolean
    let detail: string | undefined
    if (language === 'python') {
      runnable = version !== undefined
      if (!runnable) detail = `version probe failed for ${path} — not a runnable Python 3`
    } else {
      const versioned = version !== undefined
      runnable = versioned && (await deps.rRunnable(path))
      if (!versioned) detail = `version probe failed for ${path} — R did not run`
      else if (!runnable) detail = `R at ${path} lacks jsonlite`
    }
    return {
      language,
      provenance,
      envId,
      interpreterPath: path,
      label: conda ? `conda: ${conda}` : path,
      version,
      runnable,
      condaEnv: conda,
      detail
    }
  }

  const results = new Array<DiscoveredInterpreter>(unique.length)
  let next = 0
  const worker = async (): Promise<void> => {
    for (let i = next++; i < unique.length; i = next++) {
      results[i] = await probe(unique[i])
    }
  }
  await Promise.all(
    Array.from({ length: Math.min(PROBE_CONCURRENCY, unique.length) }, () => worker())
  )
  return results
}

// The env's Rscript sits beside its R (…/bin/R → …/bin/Rscript); used to probe jsonlite in THAT env,
// and (exported) to launch an EXTERNAL R binding's kernel loop, which needs Rscript, not the R binary.
export const rscriptFor = (rInterpreterPath: string): string =>
  rInterpreterPath.replace(/R(\.exe)?$/, (_m, ext: string | undefined) => `Rscript${ext ?? ''}`)

// Real dependencies for a live machine: python version via the (python-3-validating) probe, R version
// via parseRVersion, R runnability via jsonlite probed through the env's OWN Rscript, and the standard
// enumerators. Enumeration is standard-location-only (see defaultCandidatePaths) — never a disk walk.
type DiscoveryExec = (
  file: string,
  args: readonly string[],
  options: { timeout: number; windowsHide: boolean; env?: NodeJS.ProcessEnv }
) => Promise<{ stdout: string; stderr: string }>

type DefaultDiscoveryRuntimeDeps = {
  platform?: NodeJS.Platform
  exec?: DiscoveryExec
  // LS5-K4: surface for refused unpinned candidates. Defaults to a warning rather than silence.
  onUnpinnedCandidate?: (candidate: string, reason: string) => void
}

export const defaultDiscoveryDeps = (
  runtimeRoot: string,
  manualPaths?: (language: NotebookLanguage) => string[],
  runtimeDeps: DefaultDiscoveryRuntimeDeps = {}
): DiscoveryDeps => {
  const platform = runtimeDeps.platform ?? process.platform
  const exec: DiscoveryExec =
    runtimeDeps.exec ??
    (async (file, args, options) => {
      const { stdout, stderr } = await execFileAsync(file, [...args], options)
      return { stdout: String(stdout), stderr: String(stderr) }
    })
  const probeOptions = (
    interpreterPath: string,
    timeout = PROBE_TIMEOUT_MS
  ): { timeout: number; windowsHide: boolean; env?: NodeJS.ProcessEnv } => {
    const prefix = windowsCondaPrefixForR(interpreterPath, platform)
    return prefix
      ? {
          timeout,
          windowsHide: true,
          env: {
            ...process.env,
            PATH: condaActivatedPath(prefix, process.env.PATH, platform)
          }
        }
      : { timeout, windowsHide: true }
  }
  const onUnpinnedCandidate =
    runtimeDeps.onUnpinnedCandidate ??
    ((candidate: string, reason: string): void => {
      console.warn('[environment-discovery] refused unpinned interpreter candidate:', reason, {
        candidate
      })
    })
  return {
    candidatePaths: defaultCandidatePaths(runtimeRoot, manualPaths, onUnpinnedCandidate),
    onUnpinnedCandidate,
    probeVersion: async (interpreterPath, language) => {
      if (language === 'python') return probeInterpreterVersion(interpreterPath)
      try {
        // No shell: execFile runs the interpreter directly, so a path with spaces/metacharacters is
        // handled safely (shell:true would break "C:\Program Files\…" and allow injection).
        const { stdout, stderr } = await exec(
          interpreterPath,
          ['--version'],
          probeOptions(interpreterPath)
        )
        return parseRVersion(`${stdout}\n${stderr}`)
      } catch {
        return undefined
      }
    },
    rRunnable: (rInterpreterPath) =>
      rHasJsonlite({
        exec: async (args) => {
          // No shell (see probeVersion): Rscript is run directly with a static arg vector.
          const rscript = rscriptFor(rInterpreterPath)
          return exec(rscript, args, probeOptions(rscript, 15_000))
        }
      }),
    realpath: safeRealpath,
    runtimeRoot
  }
}
