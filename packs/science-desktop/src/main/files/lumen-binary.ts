/**
 * Resolve + probe a real lumen-science binary for live smoke / OSF-9.
 *
 * Offline CI stays honest: when no binary is found, callers keep binaryHash=null
 * and skip live probes — never invent a hash.
 */

import { createHash } from 'node:crypto'
import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'

const HEX64 = /^[a-f0-9]{64}$/

export type LumenBinaryProbe = {
  binaryPath: string
  binaryHash: string
  versionOk: boolean
  versionOutput: string
  helpOk: boolean
  helpOutput: string
  ok: boolean
  detail?: string
}

/** Resolve LUMEN_BINARY, then common install paths, then PATH. */
export function resolveLumenScienceBinary(
  env: NodeJS.ProcessEnv = process.env,
): string | null {
  const fromEnv = env.LUMEN_BINARY?.trim()
  if (fromEnv && fs.existsSync(fromEnv) && fs.statSync(fromEnv).isFile()) {
    return path.resolve(fromEnv)
  }

  const home = env.HOME || env.USERPROFILE || ''
  const candidates = [
    path.join(home, '.local/bin/lumen-science'),
    path.join(home, 'bin/lumen-science'),
  ]
  for (const c of candidates) {
    if (c && fs.existsSync(c) && fs.statSync(c).isFile()) return path.resolve(c)
  }

  const which = spawnSync('which', ['lumen-science'], {
    encoding: 'utf-8',
    env: { ...process.env, ...env, PATH: env.PATH ?? process.env.PATH },
  })
  if (which.status === 0) {
    const p = which.stdout.trim().split('\n')[0]?.trim()
    if (p && fs.existsSync(p)) return path.resolve(p)
  }
  return null
}

/** SHA-256 hex of file contents (streamed). Throws if unreadable. */
export function sha256File(filePath: string): string {
  const hash = createHash('sha256')
  const fd = fs.openSync(filePath, 'r')
  try {
    const buf = Buffer.alloc(64 * 1024)
    let n: number
    while ((n = fs.readSync(fd, buf, 0, buf.length, null)) > 0) {
      hash.update(buf.subarray(0, n))
    }
  } finally {
    fs.closeSync(fd)
  }
  return hash.digest('hex')
}

export function isSha256Hex(value: string | null | undefined): boolean {
  return typeof value === 'string' && HEX64.test(value)
}

/**
 * Hash + version + help against a real binary.
 * Fails closed if the binary misbehaves (non-zero exit or missing markers).
 */
export function probeLumenScienceBinary(
  binaryPath: string,
  opts?: { timeoutMs?: number },
): LumenBinaryProbe {
  const timeout = opts?.timeoutMs ?? 30_000
  const binaryHash = sha256File(binaryPath)

  const version = spawnSync(binaryPath, ['version'], {
    encoding: 'utf-8',
    timeout,
  })
  const versionOutput = `${version.stdout || ''}${version.stderr || ''}`
  const versionOk =
    !version.error &&
    version.status === 0 &&
    /1\.\d/.test(versionOutput)

  const help = spawnSync(binaryPath, ['--help'], {
    encoding: 'utf-8',
    timeout,
  })
  const stdioHelp = spawnSync(binaryPath, ['agent', 'stdio', '--help'], {
    encoding: 'utf-8',
    timeout,
  })
  const rootHelpOutput = `${help.stdout || ''}${help.stderr || ''}`
  const stdioHelpOutput = `${stdioHelp.stdout || ''}${stdioHelp.stderr || ''}`
  const helpOutput = `${rootHelpOutput}\n${stdioHelpOutput}`
  const helpOk =
    !help.error &&
    help.status === 0 &&
    /Lumen TUI/i.test(rootHelpOutput) &&
    !stdioHelp.error &&
    stdioHelp.status === 0 &&
    /Run the agent over stdio/i.test(stdioHelpOutput)

  const ok = isSha256Hex(binaryHash) && versionOk && helpOk
  let detail: string | undefined
  if (!ok) {
    const parts: string[] = []
    if (!versionOk) parts.push(`version: status=${version.status} out=${versionOutput.slice(0, 120)}`)
    if (!helpOk) {
      parts.push(
        `help: root_status=${help.status} stdio_status=${stdioHelp.status} ` +
          `out=${helpOutput.slice(0, 120)}`,
      )
    }
    detail = parts.join('; ') || 'probe failed'
  }

  return {
    binaryPath,
    binaryHash,
    versionOk,
    versionOutput: versionOutput.trim(),
    helpOk,
    helpOutput: helpOutput.trim(),
    ok,
    detail,
  }
}

/**
 * Resolve optional live binary and probe it.
 * Returns null when no binary is available (honest offline skip).
 */
export function resolveAndProbeLumenScienceBinary(
  env: NodeJS.ProcessEnv = process.env,
): LumenBinaryProbe | null {
  const bin = resolveLumenScienceBinary(env)
  if (!bin) return null
  return probeLumenScienceBinary(bin)
}
