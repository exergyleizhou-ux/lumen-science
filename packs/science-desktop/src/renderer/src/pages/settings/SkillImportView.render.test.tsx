// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { SkillImportView } from './SkillImportView'
import { createInitialSettingsState, useSettingsStore } from '@/stores/settings-store'

vi.mock('@/components/streamdown/AgentMarkdown', () => ({
  AgentMarkdown: ({ content, allowMedia }: { content: string; allowMedia?: boolean }) => (
    <div data-testid="agent-markdown" data-allow-media={String(allowMedia)}>
      {content}
    </div>
  )
}))

let container: HTMLDivElement
let root: Root

const flush = async (): Promise<void> => {
  for (let index = 0; index < 4; index += 1) {
    await act(async () => {
      await Promise.resolve()
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
  }
}

beforeEach(() => {
  useSettingsStore.setState({
    ...createInitialSettingsState(),
    scanRepoSkills: vi.fn().mockResolvedValue({
      skills: [
        {
          name: 'Alpha',
          path: 'skills/alpha',
          url: 'https://github.com/acme/skills/tree/main/skills/alpha',
          alreadyImported: false
        }
      ]
    }),
    previewGitHubSkill: vi.fn().mockResolvedValue({
      name: 'Alpha',
      description: 'Remote preview',
      sourceLabel: 'github.com/acme/skills@main/skills/alpha',
      metadata: { license: 'MIT' },
      body: '# Alpha instructions',
      files: ['SKILL.md', 'references/guide.md']
    }),
    importSkill: vi.fn()
  })
  container = document.createElement('div')
  document.body.appendChild(container)
  root = createRoot(container)
})

afterEach(() => {
  act(() => root.unmount())
  container.remove()
  document.body.innerHTML = ''
})

describe('SkillImportView', () => {
  it('lazily previews one GitHub candidate without selecting or importing it', async () => {
    act(() => root.render(<SkillImportView onImported={vi.fn()} />))
    const input = document.body.querySelector<HTMLInputElement>(
      '[aria-label="GitHub skill URL or repo"]'
    )!
    act(() => {
      const setter = Object.getOwnPropertyDescriptor(
        window.HTMLInputElement.prototype,
        'value'
      )?.set
      setter?.call(input, 'acme/skills')
      input.dispatchEvent(new Event('input', { bubbles: true }))
    })
    act(() => {
      Array.from(document.body.querySelectorAll('button'))
        .find((button) => button.textContent === 'Preview')
        ?.click()
    })
    await flush()

    const checkbox = document.body.querySelector<HTMLInputElement>('[aria-label="Select Alpha"]')
    expect(checkbox?.checked).toBe(true)
    act(() => {
      document.body.querySelector<HTMLButtonElement>('[aria-label="Preview Alpha"]')?.click()
    })
    await flush()

    expect(useSettingsStore.getState().previewGitHubSkill).toHaveBeenCalledWith(
      'https://github.com/acme/skills/tree/main/skills/alpha'
    )
    expect(document.body.querySelector('[role="dialog"]')?.textContent).toContain(
      'Alpha instructions'
    )
    expect(
      document.body
        .querySelector('[role="dialog"]')
        ?.querySelector('[data-testid="agent-markdown"]')
        ?.getAttribute('data-allow-media')
    ).toBe('false')
    expect(useSettingsStore.getState().importSkill).not.toHaveBeenCalled()
    expect(checkbox?.checked).toBe(true)
  })
})
