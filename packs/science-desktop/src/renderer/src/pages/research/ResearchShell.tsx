/**
 * Product shell: Question · Plan · Evidence · Result · Review
 *
 * Hides ACP/SessionActor jargon from the user. Opens workspaces only through
 * window.api.lumen (membership-gated bind + artifact_id preview).
 *
 * Adapted for Lumen Science Desktop — not a copy of Open Science home.
 */

import { useCallback, useEffect, useState } from 'react'
import { PermissionPrompt } from '@/components/PermissionPrompt'
import { describeError } from './describe-error'
import { describeOpen, type OpenOutcome } from './describe-open'

type UiProject = {
  id: string
  name: string
  description?: string
  ownerId: string
  defaultRunId: string
}

type TabId =
  | 'question'
  | 'plan'
  | 'notebook'
  | 'evidence'
  | 'result'
  | 'review'
  | 'skills'
  | 'compute'
  | 'connectors'

const TABS: { id: TabId; label: string }[] = [
  { id: 'question', label: 'Question' },
  { id: 'plan', label: 'Plan' },
  { id: 'notebook', label: 'Notebook' },
  { id: 'evidence', label: 'Evidence' },
  { id: 'result', label: 'Result' },
  { id: 'review', label: 'Review' },
  { id: 'skills', label: 'Skills' },
  { id: 'compute', label: 'Compute' },
  { id: 'connectors', label: 'Connectors' },
]

export const ResearchShell = (): React.JSX.Element => {
  const [projects, setProjects] = useState<UiProject[]>([])
  const [active, setActive] = useState<UiProject | null>(null)
  const [tab, setTab] = useState<TabId>('question')
  const [name, setName] = useState('')
  const [question, setQuestion] = useState('')
  /**
   * The question as the ENGINE has it recorded. Kept beside the editable text
   * so the tab can say whether what is on screen has been saved — a textarea
   * that looks identical whether or not its contents survived a tab switch is
   * how an hour of work disappears silently.
   */
  const [savedQuestion, setSavedQuestion] = useState('')
  const [questionStatus, setQuestionStatus] = useState('')
  const [status, setStatus] = useState<string>('')
  /**
   * The outcome of the last open, as a headline plus the engine's own words.
   * Previously the raw internals went straight into the status bar, so the
   * first thing a user saw inside a project was a paragraph about which Rust
   * module dispatches which method.
   */
  const [opened, setOpened] = useState<OpenOutcome | null>(null)
  const [hash, setHash] = useState<string | null>(null)
  const [error, setError] = useState<string | undefined>()
  const [previewMeta, setPreviewMeta] = useState<string>('')
  const [nbCode, setNbCode] = useState('print("hello from lumen notebook plan")\n')
  const [nbOut, setNbOut] = useState<string>('')

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
    // Clear the previous outcome BEFORE asking. Otherwise a failed open leaves
    // the last project's "Opened." line on screen next to the error explaining
    // that nothing opened — two contradictory claims, one of them stale.
    setOpened(null)
    setQuestionStatus('')
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
    setStatus('')
    setOpened(describeOpen(res))
    // Show what is RECORDED, not whatever the previous project left in the box.
    const recorded = (res as { researchQuestion?: string }).researchQuestion ?? ''
    setQuestion(recorded)
    setSavedQuestion(recorded)
    setQuestionStatus('')
  }

  const [previewId, setPreviewId] = useState('')
  const saveQuestion = async (): Promise<void> => {
    if (!lumen || !active) return
    setQuestionStatus('Saving…')
    try {
      const res = (await lumen.updateQuestion({ researchQuestion: question })) as {
        ok?: boolean
        reason?: string
      }
      if (!res.ok) {
        // Includes a denied permission. The box keeps the user's text — losing
        // it because the engine said no would punish them for our refusal.
        setQuestionStatus(res.reason ?? 'the engine refused this change')
        return
      }
      setSavedQuestion(question)
      setQuestionStatus('Saved to the project record.')
    } catch (e: unknown) {
      setQuestionStatus((e as Error)?.message || String(e))
    }
  }

  const tryPreview = async (): Promise<void> => {
    if (!lumen || !active) return
    // An inline field, not window.prompt(): Electron renderers THROW on
    // prompt() ("prompt() is and will not be supported"), so the tab's only
    // control died on first click with an unhandled rejection and no feedback.
    const artifactId = previewId.trim()
    if (!artifactId) {
      setPreviewMeta('Enter an artifact_id first.')
      return
    }
    try {
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
    } catch (e: unknown) {
      // A failed preview is a result to show, never an unhandled rejection.
      setPreviewMeta((e as Error)?.message || String(e))
    }
  }

  const notebookDryRun = async (): Promise<void> => {
    if (!lumen) return
    const res = await lumen.notebookDryRunCell({ language: 'python', code: nbCode })
    setNbOut(JSON.stringify(res, null, 2))
  }

  const notebookExecute = async (): Promise<void> => {
    if (!lumen || !active) {
      setNbOut('Open a project first (trusted session required for live execute).')
      return
    }
    const res = await lumen.notebookExecuteCell({ language: 'python', code: nbCode })
    setNbOut(JSON.stringify(res, null, 2))
  }

  const notebookExport = async (): Promise<void> => {
    if (!lumen) return
    const res = await lumen.notebookExportIpynb()
    setNbOut(JSON.stringify(res, null, 2))
  }

  const [reviewArtifacts, setReviewArtifacts] = useState('art-1:abc\nart-2:xyz')
  const [reviewOut, setReviewOut] = useState('')
  const reviewPlan = async (): Promise<void> => {
    if (!lumen) return
    const artifacts = reviewArtifacts
      .split('\n')
      .filter(Boolean)
      .map((line) => {
        const [artifactId, expectedSha256] = line.split(':')
        return { artifactId: artifactId?.trim() ?? '', expectedSha256: expectedSha256?.trim() ?? '' }
      })
      .filter((a) => a.artifactId && a.expectedSha256)
    const res = await lumen.reviewPlan({ artifacts })
    setReviewOut(JSON.stringify(res, null, 2))
  }
  const reviewSubmit = async (): Promise<void> => {
    if (!lumen) return
    const artifacts = reviewArtifacts
      .split('\n')
      .filter(Boolean)
      .map((line) => {
        const [artifactId, expectedSha256] = line.split(':')
        return { artifactId: artifactId?.trim() ?? '', expectedSha256: expectedSha256?.trim() ?? '' }
      })
      .filter((a) => a.artifactId && a.expectedSha256)
    const res = await lumen.reviewSubmit({ artifacts })
    setReviewOut(JSON.stringify(res, null, 2))
  }
  const reviewExport = async (): Promise<void> => {
    if (!lumen) return
    const res = await lumen.reviewExportDossier()
    setReviewOut(JSON.stringify(res, null, 2))
  }

  const [skillsOut, setSkillsOut] = useState('')
  const skillsList = async (): Promise<void> => {
    if (!lumen) return
    setSkillsOut(JSON.stringify(await lumen.skillsList(), null, 2))
  }
  const skillsBulkDeny = async (): Promise<void> => {
    if (!lumen) return
    setSkillsOut(
      JSON.stringify(
        await lumen.skillsBulkAdmit({ skillIds: ['a', 'b', 'c'] }),
        null,
        2,
      ),
    )
  }

  const [computeHost, setComputeHost] = useState('hpc.example.com')
  const [computeOut, setComputeOut] = useState('')
  const computePlan = async (): Promise<void> => {
    if (!lumen) return
    setComputeOut(
      JSON.stringify(
        await lumen.computePlan({
          hostname: computeHost,
          targetKind: 'ssh_fixture',
          command: 'lumen-science pipeline offline ...',
        }),
        null,
        2,
      ),
    )
  }
  const computeLiveDeny = async (): Promise<void> => {
    if (!lumen) return
    setComputeOut(JSON.stringify(await lumen.computeExecuteLive({ planId: 'x' }), null, 2))
  }

  const [connOut, setConnOut] = useState('')
  const connectorsList = async (): Promise<void> => {
    if (!lumen) return
    setConnOut(JSON.stringify(await lumen.connectorsList(), null, 2))
  }
  const connectorsFetchDeny = async (): Promise<void> => {
    if (!lumen) return
    setConnOut(
      JSON.stringify(await lumen.connectorsFetch({ connectorId: 'pubmed' }), null, 2),
    )
  }

  return (
    <div className={cx.root}>
      {/* Mounted unconditionally. The engine can ask at any point, and a prompt
          that only exists on some screen would leave those asks to time out
          into a denial the user never saw. */}
      {lumen ? (
        <PermissionPrompt
          subscribe={lumen.onPermissionAsk}
          respond={lumen.respondToPermission}
        />
      ) : null}
      <header className={cx.header}>
        <div>
          <h1 className={cx.title}>Lumen Science</h1>
          <p className={cx.sub}>
            Question · Plan · Evidence · Result · Review — auditable research desk
          </p>
        </div>
        <div className={hash ? cx.engineOnline : cx.engineOffline}>
          {/* A dot plus a word, not colour alone: the state has to survive
              being read by someone who cannot separate green from amber. */}
          <span aria-hidden className={hash ? cx.engineDotOn : cx.engineDotOff} />
          {hash ? (
            <span title="Rust engine binary identity (SHA-256)">
              engine <span className="font-mono">{hash.slice(0, 12)}…</span>
            </span>
          ) : (
            <span title="No Lumen engine binary was resolved">engine offline</span>
          )}
        </div>
      </header>

      {error &&
        (() => {
          const described = describeError(error)
          return (
            <div
              className={described.expected ? cx.notice : cx.error}
              role={described.expected ? 'status' : 'alert'}
            >
              <p className={cx.noticeHeadline}>{described.headline}</p>
              {/* The original text, verbatim and never truncated. Shortening an
                  engine error is how a product ends up saying "something went
                  wrong" while the cause sits in a log nobody reads. */}
              <p className={cx.noticeDetail}>{described.detail}</p>
            </div>
          )
        })()}
      {opened && (
        <div className={cx.notice} role="status">
          <p className={cx.noticeHeadline}>{opened.headline}</p>
          {/* Present, not shouted. The engine's exact words stay one click
              away rather than dominating the screen — dropping them would be
              how a build silently stops seeding evidence it claims to have. */}
          {opened.detail && (
            <details className={cx.details}>
              <summary className={cx.summary}>
                {opened.expected ? 'Why' : 'Technical detail'}
              </summary>
              <p className={cx.noticeDetail}>{opened.detail}</p>
            </details>
          )}
        </div>
      )}
      {status && <div className={cx.status}>{status}</div>}

      <div className={cx.body}>
        <aside className={cx.aside}>
          <h2 className={cx.h2}>Projects</h2>
          <div className={cx.row}>
            <input
              className={cx.input}
              placeholder="New project name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void createProject()
              }}
            />
            <button
              type="button"
              className={cx.btn}
              // createProject() returns early on an empty name, so the button
              // did nothing and said nothing. A dead control teaches people the
              // app is broken; disabling it states the requirement instead.
              disabled={!name.trim()}
              onClick={() => void createProject()}
            >
              Create
            </button>
          </div>
          <ul className={cx.list}>
            {projects.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  className={active?.id === p.id ? cx.projectBtnActive : cx.projectBtn}
                  // Selection is announced, not only drawn: a sighted user sees
                  // the filled row, everyone else needs this.
                  aria-current={active?.id === p.id ? 'true' : undefined}
                  onClick={() => void openProject(p)}
                >
                  <span className={cx.projectName}>{p.name}</span>
                  {/* Two projects may share a name; the id is what identifies
                      one to the engine, so show enough of it to tell them
                      apart without dominating the row. */}
                  <span className={cx.projectId}>{p.id.slice(0, 8)}</span>
                </button>
              </li>
            ))}
            {projects.length === 0 && (
              <li className={cx.sidebarEmpty}>No projects yet.</li>
            )}
          </ul>
          <p className={cx.sidebarNote}>
            This list is just an index. The projects themselves live in the
            engine, which is what any of these results can be checked against.
          </p>
        </aside>

        <main className={cx.main}>
          {!active ? (
            <div className={cx.empty}>
              <div className={cx.emptyInner}>
                <h2 className={cx.emptyTitle}>No project open</h2>
                <p className={cx.emptyBody}>
                  Create one on the left, or pick an existing project to open its
                  question, evidence and results.
                </p>
                {/* The invariant belongs here, but quieter than the
                    instruction: a first-time reader needs to know what to DO
                    before they need to know what is guaranteed. */}
                <p className={cx.emptyNote}>
                  Membership is asserted before bind. Previews load by{' '}
                  <code className={cx.code}>artifact_id</code>, never by an
                  arbitrary path.
                </p>
              </div>
            </div>
          ) : (
            <>
              {/* role="tab" is only meaningful inside a tablist: on its own it
                  tells a screen reader "this is a tab" without ever saying what
                  set it belongs to or how many there are. The panel below is
                  labelled by its tab for the same reason. */}
              <div className={cx.tabs} role="tablist" aria-label="Project workspace">
                {TABS.map((t) => (
                  <button
                    key={t.id}
                    type="button"
                    id={`tab-${t.id}`}
                    className={tab === t.id ? cx.tabActive : cx.tab}
                    role="tab"
                    aria-selected={tab === t.id}
                    aria-controls={`panel-${t.id}`}
                    onClick={() => setTab(t.id)}
                  >
                    {t.label}
                  </button>
                ))}
              </div>

              {tab === 'question' && (
                <section
                  className={cx.panel}
                  role="tabpanel"
                  id="panel-question"
                  aria-labelledby="tab-question"
                >
                  <h2 className={cx.h2}>Research question</h2>
                  <textarea
                    className={cx.textarea}
                    rows={6}
                    placeholder="e.g. Given disease X and target Y, assemble literature, genetic, protein, and compound evidence into a reproducible dossier."
                    value={question}
                    onChange={(e) => setQuestion(e.target.value)}
                  />
                  <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
                    <button
                      type="button"
                      className={cx.btn}
                      // Nothing to save is not an error to explain after the
                      // click; the control says so by being unavailable.
                      disabled={!question.trim() || question === savedQuestion}
                      onClick={() => void saveQuestion()}
                    >
                      Save question
                    </button>
                    {question !== savedQuestion && (
                      <span className={cx.muted}>Unsaved changes</span>
                    )}
                    {questionStatus && <span className={cx.muted}>{questionStatus}</span>}
                  </div>
                  <p className={cx.muted}>
                    The question is part of the project record, so saving it goes
                    through the engine and asks for your approval like any other
                    change to that record.
                  </p>
                </section>
              )}

              {tab === 'plan' && (
                <section
                  className={cx.panel}
                  role="tabpanel"
                  id="panel-plan"
                  aria-labelledby="tab-plan"
                >
                  <h2 className={cx.h2}>Plan</h2>
                  <p className={cx.muted}>
                    An outline of the intended route, written here — <strong>not</strong>
                    validated by the engine. Nothing on this tab has been checked
                    against anything; the Notebook tab is where a step actually runs,
                    and only the engine runs it.
                  </p>
                  <pre className={cx.pre}>
                    {question
                      ? `1. Literature (PubMed/OpenAlex)\n2. Biological DBs (UniProt/ClinVar/ChEMBL)\n3. Notebook analysis\n4. Reviewer + EvidenceGraph\n5. Export package\n\nQuestion: ${question}`
                      : 'Enter a question first.'}
                  </pre>
                </section>
              )}

              {tab === 'notebook' && (
                <section
                  className={cx.panel}
                  role="tabpanel"
                  id="panel-notebook"
                  aria-labelledby="tab-notebook"
                >
                  <h2 className={cx.h2}>Notebook</h2>
                  <p className={cx.muted}>
                    Write and dry-run cells here; every real execution happens in the
                    engine, which records what ran. No code runs inside this window.
                  </p>
                  <textarea
                    className={cx.textarea}
                    rows={8}
                    value={nbCode}
                    onChange={(e) => setNbCode(e.target.value)}
                    spellCheck={false}
                  />
                  <div style={{ display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap' }}>
                    <button type="button" className={cx.btn} onClick={() => void notebookDryRun()}>
                      Dry-run plan
                    </button>
                    <button type="button" className={cx.btn} onClick={() => void notebookExecute()}>
                      Run in engine
                    </button>
                    <button type="button" className={cx.btn} onClick={() => void notebookExport()}>
                      Export .ipynb
                    </button>
                  </div>
                  {nbOut && <pre className={cx.pre}>{nbOut}</pre>}
                </section>
              )}

              {tab === 'evidence' && (
                <section
                  className={cx.panel}
                  role="tabpanel"
                  id="panel-evidence"
                  aria-labelledby="tab-evidence"
                >
                  <h2 className={cx.h2}>Evidence</h2>
                  <p className={cx.muted}>
                    Artifacts are hash-registered. Open preview by artifact_id after
                    session bind.
                  </p>
                  <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                    <input
                      className={cx.input}
                      value={previewId}
                      onChange={(e) => setPreviewId(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') void tryPreview()
                      }}
                      placeholder="artifact_id"
                      aria-label="Artifact id to preview"
                    />
                    <button type="button" className={cx.btn} onClick={() => void tryPreview()}>
                      Preview
                    </button>
                  </div>
                  {previewMeta && <pre className={cx.pre}>{previewMeta}</pre>}
                </section>
              )}

              {tab === 'result' && (
                <section
                  className={cx.panel}
                  role="tabpanel"
                  id="panel-result"
                  aria-labelledby="tab-result"
                >
                  <h2 className={cx.h2}>Result</h2>
                  <p className={cx.muted}>
                    ResearchResult claims must cite evidence nodes. Export package
                    deferred to 3.0 product path.
                  </p>
                </section>
              )}

              {tab === 'connectors' && (
                <section
                  className={cx.panel}
                  role="tabpanel"
                  id="panel-connectors"
                  aria-labelledby="tab-connectors"
                >
                  <h2 className={cx.h2}>Connectors</h2>
                  <p className={cx.muted}>
                    A read-only catalog of the data sources this build knows about. All
                    fetching is done by the engine — ask this window to fetch one and it
                    will tell you it has no way to.
                  </p>
                  <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                    <button type="button" className={cx.btn} onClick={() => void connectorsList()}>
                      List catalog
                    </button>
                    <button
                      type="button"
                      className={cx.btnQuiet}
                      onClick={() => void connectorsFetchDeny()}
                    >
                      Ask this window to fetch
                    </button>
                  </div>
                  {connOut && <pre className={cx.pre}>{connOut}</pre>}
                </section>
              )}

              {tab === 'compute' && (
                <section
                  className={cx.panel}
                  role="tabpanel"
                  id="panel-compute"
                  aria-labelledby="tab-compute"
                >
                  <h2 className={cx.h2}>Remote Compute</h2>
                  <p className={cx.muted}>
                    Plans a remote run and shows you exactly what it would do. Scheduling
                    it for real needs your approval of that specific plan, and only the
                    engine can carry it out — this window runs nothing remotely, and an
                    open-ended shell command is refused outright.
                  </p>
                  <input
                    className={cx.input}
                    value={computeHost}
                    onChange={(e) => setComputeHost(e.target.value)}
                    placeholder="hostname"
                  />
                  <div style={{ display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap' }}>
                    <button type="button" className={cx.btn} onClick={() => void computePlan()}>
                      Plan (dry-run)
                    </button>
                    <button type="button" className={cx.btnQuiet} onClick={() => void computeLiveDeny()}>
                      Ask this window to run it live
                    </button>
                  </div>
                  {computeOut && <pre className={cx.pre}>{computeOut}</pre>}
                </section>
              )}

              {tab === 'skills' && (
                <section
                  className={cx.panel}
                  role="tabpanel"
                  id="panel-skills"
                  aria-labelledby="tab-skills"
                >
                  <h2 className={cx.h2}>Skills</h2>
                  <p className={cx.muted}>
                    Imported skills arrive quarantined and stay pending until someone
                    admits them file by file — <strong>nothing is approved in bulk</strong>,
                    and that includes anything asking for a GPU. Ask this window to admit
                    them all at once and it will refuse.
                  </p>
                  <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                    <button type="button" className={cx.btn} onClick={() => void skillsList()}>
                      List inventory
                    </button>
                    <button type="button" className={cx.btnQuiet} onClick={() => void skillsBulkDeny()}>
                      Ask this window to admit all
                    </button>
                  </div>
                  {skillsOut && <pre className={cx.pre}>{skillsOut}</pre>}
                </section>
              )}

              {tab === 'review' && (
                <section
                  className={cx.panel}
                  role="tabpanel"
                  id="panel-review"
                  aria-labelledby="tab-review"
                >
                  <h2 className={cx.h2}>Review</h2>
                  <p className={cx.muted}>
                    Reviews are bound to specific artifacts, and each verdict — pass, warn
                    or fail — is recorded as supporting or contradicting evidence. A review
                    can propose corrections but can never apply them: proposals are plans,
                    not actions.
                  </p>
                  <p className={cx.muted}>
                    Evidence (one per line): <code>artifactId:expectedSha256</code>
                  </p>
                  <textarea
                    className={cx.textarea}
                    rows={4}
                    value={reviewArtifacts}
                    onChange={(e) => setReviewArtifacts(e.target.value)}
                    spellCheck={false}
                  />
                  <div style={{ display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap' }}>
                    <button type="button" className={cx.btn} onClick={() => void reviewPlan()}>
                      Plan review
                    </button>
                    <button
                      type="button"
                      className={cx.btn}
                      onClick={() => void reviewSubmit()}
                    >
                      Submit review
                    </button>
                    <button type="button" className={cx.btn} onClick={() => void reviewExport()}>
                      Export dossier
                    </button>
                  </div>
                  {reviewOut && <pre className={cx.pre}>{reviewOut}</pre>}
                </section>
              )}
            </>
          )}
        </main>
      </div>
    </div>
  )
}

/**
 * Presentation classes for the research desk.
 *
 * Replaces a `Record<string, React.CSSProperties>` of hardcoded colours
 * (`#0b0f14`, `#e8eef5`, `#1b6ef3`). Those bypassed the design system entirely,
 * so this screen ignored the app's theme and looked like a different product
 * bolted on — which is what it was.
 *
 * Everything here is a design token: `bg-background`, `text-foreground`,
 * `border-border`, `bg-primary`. The desk now follows light and dark with the
 * rest of the app, and a theme change reaches it without touching this file.
 */
const cx = {
  // h-screen + overflow-hidden, not min-h-screen: the panes scroll, the shell
  // does not. The previous min-h-[calc(100vh-5.5rem)] body hardcoded a guess at
  // the header height, so a status line pushed the sidebar's own footnote below
  // the fold where nobody could read it.
  root: 'flex h-screen flex-col overflow-hidden bg-background text-foreground',
  header:
    'flex shrink-0 items-start justify-between gap-4 border-b border-border px-6 py-5',
  title: 'text-xl font-semibold tracking-tight',
  sub: 'mt-1 text-sm text-muted-foreground',
  meta: 'font-mono text-xs text-muted-foreground',
  body: 'flex min-h-0 flex-1',
  aside: 'flex w-72 shrink-0 flex-col overflow-y-auto border-r border-border p-4',
  main: 'flex min-w-0 flex-1 flex-col overflow-y-auto p-6',
  h2: 'mb-3 text-xs font-medium uppercase tracking-wider text-muted-foreground',
  row: 'mb-3 flex gap-2',
  input:
    'flex-1 rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-xs outline-none transition-[color,box-shadow] placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50',
  btn:
    'inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:pointer-events-none disabled:opacity-50',
  // A secondary weight for the "prove the boundary holds" controls. They are
  // worth having — this product's claim is containment, and a claim you can
  // press a button to check is stronger than one in a doc — but they are not
  // the action a researcher came to this panel to take.
  btnQuiet:
    'inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md border border-border bg-transparent px-3 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-50',
  list: 'flex flex-col gap-1',
  projectBtn:
    'flex w-full flex-col items-start gap-0.5 rounded-md px-3 py-2 text-left transition-colors hover:bg-muted',
  // Selection is carried by background AND weight, not colour alone: a
  // colour-only cue disappears for anyone who cannot separate those hues.
  projectBtnActive:
    'flex w-full flex-col items-start gap-0.5 rounded-md bg-muted px-3 py-2 text-left',
  muted: 'text-sm text-muted-foreground',
  tabs: 'mb-4 flex gap-1 border-b border-border',
  tab:
    'rounded-t-md px-3 py-2 text-sm text-muted-foreground transition-colors hover:text-foreground',
  tabActive:
    'rounded-t-md border-b-2 border-primary px-3 py-2 text-sm font-medium text-foreground',
  panel: 'mb-4 rounded-lg border border-border bg-card p-4 text-card-foreground shadow-sm',
  textarea:
    'min-h-28 w-full rounded-md border border-input bg-transparent px-3 py-2 font-mono text-sm shadow-xs outline-none focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50',
  // Long output must scroll inside its panel rather than widening the page.
  pre:
    'max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-md bg-muted p-3 font-mono text-xs text-foreground',
  // Anchored rather than floated at the top: an empty screen with one line
  // pinned under the header reads as a page that failed to load.
  empty: 'flex flex-1 items-center justify-center p-6',
  emptyInner: 'max-w-md text-center',
  emptyTitle: 'text-base font-medium text-foreground',
  emptyBody: 'mt-2 text-sm text-muted-foreground',
  emptyNote: 'mt-4 border-t border-border pt-4 text-xs text-muted-foreground',
  code: 'rounded bg-muted px-1 py-0.5 font-mono text-[0.9em]',
  sidebarEmpty: 'px-3 py-2 text-sm text-muted-foreground',
  projectName: 'w-full truncate text-sm text-foreground',
  projectId: 'font-mono text-[11px] text-muted-foreground',
  // mt-auto pins this to the bottom of the flex column, so it stops reading as
  // the next item in the project list.
  sidebarNote: 'mt-auto border-t border-border pt-3 text-xs text-muted-foreground',
  error:
    'shrink-0 border-b border-destructive/40 bg-destructive/10 px-6 py-3 text-destructive',
  // A refusal by design is not a fault. Showing both in the same alarmed red
  // teaches people to ignore the colour, so an expected refusal gets a neutral
  // notice and only the unrecognised failure gets the alarm.
  notice: 'shrink-0 border-b border-border bg-muted/50 px-6 py-3 text-foreground',
  noticeHeadline: 'text-sm font-medium',
  noticeDetail: 'mt-1 font-mono text-xs leading-relaxed text-muted-foreground',
  // A disclosure, so the engine's exact words are one click away instead of
  // being the largest thing on the screen.
  details: 'mt-1',
  summary:
    'cursor-pointer text-xs text-muted-foreground underline-offset-2 hover:underline ' +
    'marker:text-muted-foreground',
  status:
    'shrink-0 border-b border-border bg-muted/40 px-6 py-2 font-mono text-xs text-muted-foreground',
  // Engine identity reads as a status pill rather than stray monospace: it is
  // the single most load-bearing fact on this screen — every claim the desk
  // makes is only as good as the binary that produced it.
  engineOnline:
    'inline-flex shrink-0 items-center gap-2 rounded-full border border-border bg-muted/60 px-3 py-1 text-xs text-foreground',
  engineOffline:
    'inline-flex shrink-0 items-center gap-2 rounded-full border border-destructive/40 bg-destructive/10 px-3 py-1 text-xs text-destructive',
  engineDotOn: 'size-1.5 rounded-full bg-emerald-500',
  engineDotOff: 'size-1.5 rounded-full bg-destructive'
} as const
