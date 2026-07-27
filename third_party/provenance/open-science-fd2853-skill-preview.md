# Open Science `fd2853` Skill-preview delta

| Field | Value |
|---|---|
| Repository | https://github.com/aipoch/open-science.git |
| Exact commit | `fd2853f0b9bdb6c063ccc1e741687584ab94bf9a` |
| Root license | Apache-2.0 |
| Admission date | 2026-07-27 |
| Lumen authority | Rust `SessionActor`; unchanged |
| Network behavior | read-only GitHub candidate preview through the existing proxy-aware fetch seam |
| Write behavior | preview performs no Skill import, approval, execution, or authoritative state mutation |

## Adapted implementation

The following Lumen files preserve or adapt the upstream module boundaries,
inputs/outputs, resource budgets, failure semantics, and tests. Lumen-specific
changes add attribution, keep current product naming, and retain the existing
authority boundary:

- `packs/science-desktop/src/shared/skill-frontmatter.ts`
- `packs/science-desktop/src/shared/skill-import-limits.ts`
- `packs/science-desktop/src/shared/settings.ts`
- `packs/science-desktop/src/main/skills/frontmatter.ts`
- `packs/science-desktop/src/main/skills/skill-bundle-paths.ts`
- `packs/science-desktop/src/main/skills/github-import.ts`
- `packs/science-desktop/src/main/skills/user-skill-repository.ts`
- `packs/science-desktop/src/main/settings/service.ts`
- `packs/science-desktop/src/main/settings/ipc.ts`
- `packs/science-desktop/src/preload/index.ts`
- `packs/science-desktop/src/preload/index.d.ts`
- `packs/science-desktop/src/renderer/src/components/streamdown/AgentMarkdown.tsx`
- `packs/science-desktop/src/renderer/src/stores/settings-store.ts`
- `packs/science-desktop/src/renderer/src/pages/settings/SkillEditor.tsx`
- `packs/science-desktop/src/renderer/src/pages/settings/SkillDetailView.tsx`
- `packs/science-desktop/src/renderer/src/pages/settings/SkillImportCandidatePreview.tsx`
- `packs/science-desktop/src/renderer/src/pages/settings/useSkillImportCandidatePreview.ts`
- `packs/science-desktop/src/renderer/src/pages/settings/SkillImportView.tsx`
- `packs/science-desktop/src/renderer/src/pages/settings/SkillUploadView.tsx`

The corresponding focused test files were also adapted from the upstream
tests or extended with Lumen-specific negative assertions.

## Exact-source mapping

This slice is a source-level adaptation, not a behavior-only reimplementation.
The direct upstream-to-Lumen mappings are:

| Open Science path at `fd2853` | Lumen path | Adaptation |
|---|---|---|
| `src/shared/skill-frontmatter.ts` | `packs/science-desktop/src/shared/skill-frontmatter.ts` | Direct port plus source attribution |
| `src/shared/skill-import-limits.ts` | `packs/science-desktop/src/shared/skill-import-limits.ts` | Direct limit-field port into the existing Lumen limits object |
| `src/main/skills/skill-bundle-paths.ts` | `packs/science-desktop/src/main/skills/skill-bundle-paths.ts` | Direct port with Lumen type/style integration |
| `src/main/skills/github-import.ts` | `packs/science-desktop/src/main/skills/github-import.ts` | Direct port of bounded preview fetching into Lumen's existing proxy-aware fetch seam |
| `src/main/skills/user-skill-repository.ts` | `packs/science-desktop/src/main/skills/user-skill-repository.ts` | Upstream preview and metadata-preservation flow adapted into Lumen's existing repository |
| `src/main/settings/service.ts` | `packs/science-desktop/src/main/settings/service.ts` | Upstream preview service adapted with a sanitized public source label |
| `src/main/settings/ipc.ts` | `packs/science-desktop/src/main/settings/ipc.ts` | Direct read-only preview IPC port |
| `src/preload/index.ts` and `src/preload/index.d.ts` | matching files below `packs/science-desktop/src/preload/` | Direct typed preload bridge port |
| `src/renderer/src/stores/settings-store.ts` | matching Lumen renderer store | Direct preview action port |
| `src/renderer/src/pages/settings/SkillImportCandidatePreview.tsx` | matching Lumen settings component | Direct port with Lumen styling retained |
| `src/renderer/src/pages/settings/useSkillImportCandidatePreview.ts` | matching Lumen settings hook | Direct async isolation/state-machine port |
| `src/renderer/src/pages/settings/SkillImportView.tsx` | matching Lumen settings view | Upstream click-to-preview flow adapted into the existing Lumen candidate list |
| `src/renderer/src/pages/settings/SkillUploadView.tsx` | matching Lumen settings view | Upstream local preview flow adapted without changing candidate selection |
| `src/renderer/src/components/streamdown/AgentMarkdown.tsx` | matching Lumen Markdown renderer | Direct `allowMedia` safety port |

Tests follow the same source mapping. Lumen-only negative cases additionally
prove that a preview cannot fetch remote media, exceed its byte budget, select
an import candidate, or turn preview into approval or execution.

## Preserved security behavior

- YAML frontmatter uses `FAILSAFE_SCHEMA`; scalar values remain strings.
- `name` and `description` remain authoritative and imported metadata cannot
  override them.
- preview bodies are capped at 4 MiB independently from larger import limits;
- GitHub previews download only root `SKILL.md`, while listing relative files;
- opening untrusted Markdown disables network-fetching media elements;
- late/failed preview responses are isolated by a generation token;
- preview does not select a candidate, import it, or grant runtime authority.

## Explicitly not adopted in this slice

Open Science's conversation-driven agent import MCP, Electron-owned approval
broker, and agent-runtime attachment authority are not wired into Lumen. Those
would compete with the Rust `SessionActor` boundary. A later implementation
must bind attachment bytes, session, turn, owner, project, SHA-256, approval,
terminal state, and durable quarantine evidence through Lumen's existing
authority path.
