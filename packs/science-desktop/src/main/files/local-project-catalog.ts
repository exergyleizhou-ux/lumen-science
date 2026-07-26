/**
 * UI-only project catalog (Electron local state).
 *
 * NOT science authority. ResearchProject / ArtifactRegistry / EvidenceGraph
 * remain Rust SessionActor. This catalog exists so the desktop can open a
 * workspace offline and bind a trusted preview identity for projects the
 * user created in this app.
 */

import { randomUUID } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'

export type UiProject = {
  id: string
  name: string
  description?: string
  ownerId: string
  createdAt: number
  updatedAt: number
  /** Default run id used for artifact_list seed */
  defaultRunId: string
}

export type CreateUiProjectRequest = {
  name: string
  description?: string
  ownerId: string
}

export class LocalProjectCatalog {
  private projects = new Map<string, UiProject>()

  constructor(private readonly persistPath?: string) {
    if (persistPath) this.load()
  }

  list(): UiProject[] {
    return [...this.projects.values()].sort((a, b) => b.updatedAt - a.updatedAt)
  }

  get(id: string): UiProject | undefined {
    return this.projects.get(id)
  }

  create(req: CreateUiProjectRequest): UiProject {
    const name = req.name.trim()
    if (!name) throw new Error('project name required')
    if (!req.ownerId) throw new Error('ownerId required')
    const now = Date.now()
    const project: UiProject = {
      id: randomUUID(),
      name,
      description: req.description?.trim() || undefined,
      ownerId: req.ownerId,
      createdAt: now,
      updatedAt: now,
      defaultRunId: 'default',
    }
    this.projects.set(project.id, project)
    this.save()
    return project
  }

  delete(id: string): boolean {
    const ok = this.projects.delete(id)
    if (ok) this.save()
    return ok
  }

  /** Membership for hybrid asserter: owner must match catalog row. */
  hasMembership(ownerId: string, projectId: string): boolean {
    const p = this.projects.get(projectId)
    return Boolean(p && p.ownerId === ownerId)
  }

  private load(): void {
    if (!this.persistPath) return
    try {
      if (!fs.existsSync(this.persistPath)) return
      const raw = JSON.parse(fs.readFileSync(this.persistPath, 'utf-8')) as {
        projects?: UiProject[]
      }
      for (const p of raw.projects ?? []) {
        if (p?.id && p.ownerId && p.name) this.projects.set(p.id, p)
      }
    } catch {
      // corrupt file — start empty
    }
  }

  private save(): void {
    if (!this.persistPath) return
    try {
      fs.mkdirSync(path.dirname(this.persistPath), { recursive: true })
      fs.writeFileSync(
        this.persistPath,
        JSON.stringify({ projects: this.list() }, null, 2),
        'utf-8',
      )
    } catch {
      // best-effort UI persistence
    }
  }
}

/** Process-wide catalog for science IPC (tests replace via deps). */
let defaultCatalog: LocalProjectCatalog | null = null

export function getDefaultLocalProjectCatalog(persistPath?: string): LocalProjectCatalog {
  if (!defaultCatalog) {
    defaultCatalog = new LocalProjectCatalog(persistPath)
  }
  return defaultCatalog
}

export function resetDefaultLocalProjectCatalogForTests(): void {
  defaultCatalog = null
}
