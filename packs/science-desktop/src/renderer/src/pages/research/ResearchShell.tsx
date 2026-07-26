/**
 * Product shell: Question · Plan · Evidence · Result · Review
 *
 * Hides ACP/SessionActor jargon from the user. Opens workspaces only through
 * window.api.lumen (membership-gated bind + artifact_id preview).
 *
 * Adapted for Lumen Science Desktop — not a copy of Open Science home.
 */

import { useCallback, useEffect, useState } from 'react'

type UiProject = {
  id: string
  name: string
  description?: string
  ownerId: string
  defaultRunId: string
}

type TabId = 'question' | 'plan' | 'evidence' | 'result' | 'review'

const TABS: { id: TabId; label: string }[] = [
  { id: 'question', label: 'Question' },
  { id: 'plan', label: 'Plan' },
  { id: 'evidence', label: 'Evidence' },
  { id: 'result', label: 'Result' },
  { id: 'review', label: 'Review' },
]

export const ResearchShell = (): React.JSX.Element => {
  const [projects, setProjects] = useState<UiProject[]>([])
  const [active, setActive] = useState<UiProject | null>(null)
  const [tab, setTab] = useState<TabId>('question')
  const [name, setName] = useState('')
  const [question, setQuestion] = useState('')
  const [status, setStatus] = useState<string>('')
  const [hash, setHash] = useState<string | null>(null)
  const [error, setError] = useState<string | undefined>()
  const [previewMeta, setPreviewMeta] = useState<string>('')

  const lumen = window.api?.lumen

  const refresh = useCallback(async () => {
    if (!lumen) {
      setError('Lumen bridge unavailable (window.api.lumen)')
      return
    }
    try {
      const res = (await lumen.listUiProjects()) as { projects?: UiProject[] }
      setProjects(res.projects ?? [])
      const h = await lumen.getBinaryHash()
      setHash(h)
      setError(undefined)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    }
  }, [lumen])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const createProject = async (): Promise<void> => {
    if (!lumen || !name.trim()) return
    setStatus('Creating project…')
    const res = (await lumen.createUiProject({ name: name.trim() })) as {
      ok?: boolean
      project?: UiProject
      reason?: string
    }
    if (!res.ok || !res.project) {
      setError(res.reason ?? 'create failed')
      setStatus('')
      return
    }
    setName('')
    await refresh()
    setStatus(`Created ${res.project.name}`)
  }

  const openProject = async (project: UiProject): Promise<void> => {
    if (!lumen) return
    setStatus('Opening (bind + seed)…')
    const res = (await lumen.openUiProject({
      projectId: project.id,
      ownerId: project.ownerId,
      runId: project.defaultRunId,
    })) as {
      ok?: boolean
      reason?: string
      seeded?: number
      seedError?: string
    }
    if (!res.ok) {
      setError(res.reason ?? 'open failed')
      setStatus('')
      return
    }
    setActive(project)
    setTab('question')
    setError(undefined)
    setStatus(
      `Open: seeded ${res.seeded ?? 0} artifacts` +
        (res.seedError ? ` (seed: ${res.seedError})` : ''),
    )
  }

  const tryPreview = async (): Promise<void> => {
    if (!lumen || !active) return
    const artifactId = window.prompt('artifact_id to preview (hash-gated path)')
    if (!artifactId) return
    const res = (await lumen.previewByArtifact({ artifactId })) as {
      access?: { ok?: boolean; reason?: string }
      path?: string
      sha256?: string
    }
    if (!res.access?.ok) {
      setPreviewMeta(res.access?.reason ?? 'denied')
      return
    }
    setPreviewMeta(`ok path=${res.path ?? '?'} sha256=${res.sha256 ?? '?'}`)
  }

  return (
    <div style={styles.root}>
      <header style={styles.header}>
        <div>
          <h1 style={styles.title}>Lumen Science</h1>
          <p style={styles.sub}>
            Question · Plan · Evidence · Result · Review — auditable research desk
          </p>
        </div>
        <div style={styles.meta}>
          <span title="Rust binary identity">
            engine {hash ? hash.slice(0, 12) + '…' : 'offline'}
          </span>
        </div>
      </header>

      {error && <div style={styles.error}>{error}</div>}
      {status && <div style={styles.status}>{status}</div>}

      <div style={styles.body}>
        <aside style={styles.aside}>
          <h2 style={styles.h2}>Projects</h2>
          <div style={styles.row}>
            <input
              style={styles.input}
              placeholder="New project name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void createProject()
              }}
            />
            <button type="button" style={styles.btn} onClick={() => void createProject()}>
              Create
            </button>
          </div>
          <ul style={styles.list}>
            {projects.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  style={{
                    ...styles.projectBtn,
                    ...(active?.id === p.id ? styles.projectBtnActive : {}),
                  }}
                  onClick={() => void openProject(p)}
                >
                  {p.name}
                </button>
              </li>
            ))}
            {projects.length === 0 && (
              <li style={styles.muted}>No projects yet — create one to start.</li>
            )}
          </ul>
          <p style={styles.muted}>
            UI catalog only. Science state stays in Rust SessionActor.
          </p>
        </aside>

        <main style={styles.main}>
          {!active ? (
            <div style={styles.empty}>
              <h2>Open a project</h2>
              <p>
                Membership is asserted before bind. Preview loads by{' '}
                <code>artifact_id</code>, never by arbitrary path.
              </p>
            </div>
          ) : (
            <>
              <div style={styles.tabs}>
                {TABS.map((t) => (
                  <button
                    key={t.id}
                    type="button"
                    style={{
                      ...styles.tab,
                      ...(tab === t.id ? styles.tabActive : {}),
                    }}
                    onClick={() => setTab(t.id)}
                  >
                    {t.label}
                  </button>
                ))}
              </div>

              {tab === 'question' && (
                <section style={styles.panel}>
                  <h2 style={styles.h2}>Research question</h2>
                  <textarea
                    style={styles.textarea}
                    rows={6}
                    placeholder="e.g. Given disease X and target Y, assemble literature, genetic, protein, and compound evidence into a reproducible dossier."
                    value={question}
                    onChange={(e) => setQuestion(e.target.value)}
                  />
                  <p style={styles.muted}>
                    Month-2 golden path: literature → databases → notebook → Motif →
                    reviewer → evidence package. Plan execution routes through Lumen
                    only.
                  </p>
                </section>
              )}

              {tab === 'plan' && (
                <section style={styles.panel}>
                  <h2 style={styles.h2}>Plan</h2>
                  <p style={styles.muted}>
                    Workflow validate / dry-run via Rust WorkflowActor (ACP). No
                    Electron executor.
                  </p>
                  <pre style={styles.pre}>
                    {question
                      ? `1. Literature (PubMed/OpenAlex)\n2. Biological DBs (UniProt/ClinVar/ChEMBL)\n3. Notebook analysis\n4. Reviewer + EvidenceGraph\n5. Export package\n\nQuestion: ${question}`
                      : 'Enter a question first.'}
                  </pre>
                </section>
              )}

              {tab === 'evidence' && (
                <section style={styles.panel}>
                  <h2 style={styles.h2}>Evidence</h2>
                  <p style={styles.muted}>
                    Artifacts are hash-registered. Open preview by artifact_id after
                    session bind.
                  </p>
                  <button type="button" style={styles.btn} onClick={() => void tryPreview()}>
                    Preview by artifact_id
                  </button>
                  {previewMeta && <pre style={styles.pre}>{previewMeta}</pre>}
                </section>
              )}

              {tab === 'result' && (
                <section style={styles.panel}>
                  <h2 style={styles.h2}>Result</h2>
                  <p style={styles.muted}>
                    ResearchResult claims must cite evidence nodes. Export package
                    deferred to 3.0 product path.
                  </p>
                </section>
              )}

              {tab === 'review' && (
                <section style={styles.panel}>
                  <h2 style={styles.h2}>Review</h2>
                  <p style={styles.muted}>
                    Reviewer proposes; SessionActor records; user permission before
                    correction. Verdicts enter EvidenceGraph (stale on re-run).
                  </p>
                </section>
              )}
            </>
          )}
        </main>
      </div>
    </div>
  )
}

const styles: Record<string, React.CSSProperties> = {
  root: {
    minHeight: '100vh',
    background: '#0b0f14',
    color: '#e8eef5',
    fontFamily:
      'ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif',
  },
  header: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'flex-start',
    padding: '20px 24px',
    borderBottom: '1px solid #1e2a38',
  },
  title: { margin: 0, fontSize: 22, fontWeight: 650 },
  sub: { margin: '6px 0 0', color: '#8fa3b8', fontSize: 13 },
  meta: { fontSize: 12, color: '#6b7f93', fontFamily: 'ui-monospace, monospace' },
  body: { display: 'flex', minHeight: 'calc(100vh - 88px)' },
  aside: {
    width: 280,
    borderRight: '1px solid #1e2a38',
    padding: 16,
    boxSizing: 'border-box',
  },
  main: { flex: 1, padding: 20 },
  h2: { margin: '0 0 12px', fontSize: 14, letterSpacing: 0.4, textTransform: 'uppercase' as const, color: '#9db0c4' },
  row: { display: 'flex', gap: 8, marginBottom: 12 },
  input: {
    flex: 1,
    background: '#121a24',
    border: '1px solid #2a3a4d',
    color: '#e8eef5',
    borderRadius: 8,
    padding: '8px 10px',
  },
  btn: {
    background: '#1b6ef3',
    color: '#fff',
    border: 'none',
    borderRadius: 8,
    padding: '8px 12px',
    cursor: 'pointer',
    fontWeight: 600,
  },
  list: { listStyle: 'none', padding: 0, margin: '0 0 12px' },
  projectBtn: {
    width: '100%',
    textAlign: 'left' as const,
    background: 'transparent',
    border: '1px solid transparent',
    color: '#e8eef5',
    padding: '8px 10px',
    borderRadius: 8,
    cursor: 'pointer',
    marginBottom: 4,
  },
  projectBtnActive: {
    background: '#152033',
    borderColor: '#2d4f7c',
  },
  muted: { color: '#6b7f93', fontSize: 12, lineHeight: 1.45 },
  tabs: { display: 'flex', gap: 6, marginBottom: 16, flexWrap: 'wrap' as const },
  tab: {
    background: '#121a24',
    border: '1px solid #2a3a4d',
    color: '#b7c7d8',
    borderRadius: 999,
    padding: '6px 14px',
    cursor: 'pointer',
  },
  tabActive: {
    background: '#1b6ef3',
    borderColor: '#1b6ef3',
    color: '#fff',
  },
  panel: {
    background: '#101820',
    border: '1px solid #1e2a38',
    borderRadius: 12,
    padding: 16,
  },
  textarea: {
    width: '100%',
    boxSizing: 'border-box' as const,
    background: '#0b0f14',
    border: '1px solid #2a3a4d',
    color: '#e8eef5',
    borderRadius: 8,
    padding: 12,
    resize: 'vertical' as const,
  },
  pre: {
    background: '#0b0f14',
    border: '1px solid #1e2a38',
    borderRadius: 8,
    padding: 12,
    fontSize: 12,
    overflow: 'auto',
  },
  empty: { color: '#8fa3b8', maxWidth: 480 },
  error: {
    background: '#3a1515',
    color: '#ffb4b4',
    padding: '8px 16px',
    fontSize: 13,
  },
  status: {
    background: '#122318',
    color: '#9de0b3',
    padding: '8px 16px',
    fontSize: 13,
  },
}
