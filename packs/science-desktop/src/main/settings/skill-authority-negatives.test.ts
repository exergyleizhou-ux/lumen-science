import { mkdtemp, readdir, readFile, writeFile, mkdir, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { execPath } from 'node:process'
import { createHash } from 'node:crypto'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// Capture every ipcMain.handle registration so the REAL registered handlers can
// be invoked directly — same mechanism as ipc.test.ts.
const handlers = new Map<string, (event: unknown, payload: unknown) => unknown>()

vi.mock('electron', () => ({
  ipcMain: {
    handle: (channel: string, handler: (event: unknown, payload: unknown) => unknown) => {
      handlers.set(channel, handler)
    }
  },
  BrowserWindow: { getAllWindows: () => [] },
  safeStorage: {
    isEncryptionAvailable: () => true,
    encryptString: (plaintext: string) => Buffer.from(`cipher:${plaintext}`, 'utf8'),
    decryptString: (buffer: Buffer) => {
      const decoded = buffer.toString('utf8')
      if (!decoded.startsWith('cipher:')) throw new Error('bad ciphertext')
      return decoded.slice('cipher:'.length)
    }
  },
  app: { getPath: () => '/home', getAppPath: () => '/home/no-such-app-root', isPackaged: false },
  net: { fetch: vi.fn((...args: Parameters<typeof fetch>) => globalThis.fetch(...args)) }
}))

const { registerSettingsIpcHandlers } = await import('./ipc')
const { SettingsService } = await import('./service')
const { SettingsRepository } = await import('./repository')
const { SkillRegistry } = await import('../skills/registry')

let storageRoot: string
let repository: InstanceType<typeof SettingsRepository>

const seedBundle = async (): Promise<string> => {
  const bundle = await mkdtemp(join(tmpdir(), 'os-skills-bundle-'))
  await mkdir(join(bundle, 'demo'), { recursive: true })
  await writeFile(
    join(bundle, 'demo', 'SKILL.md'),
    ['---', 'name: demo', 'description: A demo skill.', '---', '', 'demo body'].join('\n'),
    'utf8'
  )
  await writeFile(
    join(bundle, 'manifest.json'),
    JSON.stringify({
      version: 1,
      skills: [
        { id: 'demo', name: 'Demo', source: 'featured', updatedAt: '2026-01-01T00:00:00.000Z' }
      ]
    }),
    'utf8'
  )
  return bundle
}

const createRealService = async (): Promise<InstanceType<typeof SettingsService>> =>
  new SettingsService({
    repository,
    storageRoot,
    skillRegistry: new SkillRegistry(await seedBundle())
  })

const invoke = async (channel: string, payload?: unknown): Promise<unknown> => {
  const handler = handlers.get(channel)
  if (!handler) throw new Error(`no handler registered for ${channel}`)
  return handler({}, payload)
}

/** Recursive byte-hash of a directory tree (files only, paths relative). */
const hashTree = async (root: string): Promise<string> => {
  const digest = createHash('sha256')
  const walk = async (dir: string, prefix: string): Promise<void> => {
    const entries = await readdir(dir, { withFileTypes: true })
    for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
      const path = join(dir, entry.name)
      const rel = prefix ? `${prefix}/${entry.name}` : entry.name
      if (entry.isDirectory()) {
        await walk(path, rel)
      } else if (entry.isFile()) {
        digest.update(rel)
        digest.update('\0')
        digest.update(await readFile(path))
        digest.update('\0')
      }
    }
  }
  await walk(root, '')
  return digest.digest('hex')
}

beforeEach(async () => {
  handlers.clear()
  storageRoot = await mkdtemp(join(tmpdir(), 's0b-neg-'))
  repository = new SettingsRepository(storageRoot)
})

afterEach(async () => {
  await rm(storageRoot, { recursive: true, force: true })
})

describe('S0-B skill-mutation fail-close (real registered handlers + real service)', () => {
  it('four shipping channels reject typed; persisted store bytes are untouched; reload never fires', async () => {
    const service = await createRealService()
    const onSkillsChanged = vi.fn()
    registerSettingsIpcHandlers({ service, onSkillsChanged })

    // Seed a real personal skill through the (still available) service method —
    // used only to prove the fail-close leaves those bytes alone.
    await service.createSkill({ name: 'Mine', description: 'd', body: '# Mine' })
    await service.setSkillEnabled({ id: 'demo', enabled: false })
    await repository.setClaudeInfo({ resolvedPath: execPath, version: '2.1.0' })

    const before = await hashTree(storageRoot)

    // Each shipping mutation channel must reject with the typed outcome.
    for (const [channel, payload] of [
      ['settings:set-skill-enabled', { id: 'demo', enabled: true }],
      ['settings:create-skill', { name: 'Other', description: 'd', body: '# Other' }],
      ['settings:update-skill', { id: 'personal-mine', name: 'Mine2', description: 'd', body: '# x' }],
      ['settings:delete-skill', { id: 'personal-mine' }]
    ] as const) {
      await expect(invoke(channel, payload)).rejects.toThrow(/SKILL_AUTHORITY_UNAVAILABLE/)
    }

    // Zero side effects: byte-identical store, zero reload callbacks.
    expect(await hashTree(storageRoot)).toBe(before)
    expect(onSkillsChanged).not.toHaveBeenCalled()

    // Read-only surface still works against the real service.
    const skills = await service.listSkills()
    expect(skills.find((skill) => skill.id === 'demo')?.enabled).toBe(false)
    expect(skills.find((skill) => skill.name === 'Mine')).toBeDefined()
    const detail = await service.getSkillDetail('personal-mine')
    expect(detail?.body).toContain('# Mine')
  })

  it('runtime provisioning stays fail-closed: forced ids cannot respawn a disabled skill, and invoke leaves runtime dir unchanged', async () => {
    const service = await createRealService()
    const onSkillsChanged = vi.fn()
    registerSettingsIpcHandlers({ service, onSkillsChanged })

    await service.setSkillEnabled({ id: 'demo', enabled: false })
    await repository.setClaudeInfo({ resolvedPath: execPath, version: '2.1.0' })
    const created = (
      await service.upsertProvider({
        type: 'custom',
        name: 'Local',
        baseUrl: 'https://g/v1',
        model: 'm',
        key: 'k'
      })
    ).providers[0]
    await service.setActiveProvider(created.id)

    // A task-forced spawn must NOT materialize the disabled skill.
    await service.resolveActiveSpawnConfig({ forcedSkillIds: ['demo'] })

    // The runtime dir must not contain the disabled skill's materialized copy.
    const claudeConfigDir = join(
      storageRoot,
      '.claude-lumen',
      'skills',
      'demo',
      'SKILL.md'
    ).replace('/.claude-lumen/', '/.claude/')
    const claudeSkill = join(storageRoot, '.claude', 'skills', 'demo', 'SKILL.md')
    const exists = await readFile(claudeSkill, 'utf8').then(
      () => true,
      () => false
    )
    expect(exists).toBe(false)

    // And invoking any shipping mutation channel changes nothing at runtime level.
    const before = await hashTree(storageRoot)
    await expect(invoke('settings:set-skill-enabled', { id: 'demo', enabled: true })).rejects.toThrow(
      /SKILL_AUTHORITY_UNAVAILABLE/
    )
    expect(await hashTree(storageRoot)).toBe(before)
    expect(onSkillsChanged).not.toHaveBeenCalled()
  })
})
