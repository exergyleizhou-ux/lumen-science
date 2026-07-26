/**
 * OSF-7 Connector catalog — read-only product surface.
 *
 * Lists Lumen's 42 inventory items from fusion-sources.lock.json.
 * Does NOT execute connectors, does NOT replace Rust adapters.
 * Rejected connectors are visible but not callable from desktop.
 */

import fs from 'node:fs'

export type ConnectorCatalogEntry = {
  connectorId: string
  stableId?: string
  category?: string
  disposition: string
  admission?: string
  callable: boolean
  rustModule?: string
  officialBaseUrl?: string
}

export type ConnectorCatalogSummary = {
  total: number
  implemented: number
  rejected: number
  callable: number
}

export function loadConnectorCatalog(lockPath: string): {
  summary: ConnectorCatalogSummary
  connectors: ConnectorCatalogEntry[]
} {
  const raw = JSON.parse(fs.readFileSync(lockPath, 'utf-8')) as {
    items?: Array<Record<string, unknown>>
    summary?: Record<string, unknown>
  }
  const items = raw.items ?? []
  const connectors: ConnectorCatalogEntry[] = items.map((it) => {
    const disposition = String(it.final_disposition ?? 'unknown')
    const connectorId = String(it.connector_id ?? it.stable_id ?? '')
    const rejected = disposition.startsWith('rejected')
    const implemented = disposition === 'implemented'
    return {
      connectorId,
      stableId: it.stable_id ? String(it.stable_id) : undefined,
      category: it.category ? String(it.category) : undefined,
      disposition,
      admission: it.admission_status ? String(it.admission_status) : undefined,
      // Callable only means "Rust adapter exists" — desktop still cannot fetch
      callable: implemented && !rejected,
      rustModule: it.rust_module ? String(it.rust_module) : undefined,
      officialBaseUrl: it.official_base_url
        ? String(it.official_base_url)
        : undefined,
    }
  })

  const implemented = connectors.filter((c) => c.callable).length
  const rejected = connectors.filter((c) =>
    c.disposition.startsWith('rejected'),
  ).length

  return {
    summary: {
      total: connectors.length,
      implemented,
      rejected,
      callable: implemented,
    },
    connectors,
  }
}

/** Desktop must never claim it can fetch as a second connector runtime */
export function rejectDesktopConnectorFetch(connectorId: string): {
  ok: false
  reason: string
} {
  return {
    ok: false,
    reason: `desktop cannot execute connector ${connectorId} — use SessionActor Rust adapters only`,
  }
}
