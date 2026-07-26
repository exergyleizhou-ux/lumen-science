/**
 * Local marks for the agent backends listed in settings.
 *
 * Replaces three icon imports from `@lobehub/icons`. That package pulls in
 * `@lobehub/ui`, `antd` and `@ant-design` — 220 MB of dependencies — plus
 * `@emoji-mart/react`, which declares `react ^16.8 || ^17 || ^18` and so
 * conflicts with this pack's React 19. AgentPanel already imported the bare
 * Mono/Color entry points specifically to dodge that tree; npm installs it
 * anyway as a peer of the icons package.
 *
 * Paying 220 MB and a peer-dependency override for three logos is a poor trade
 * on its own. It is a worse one here: `src/main/agent-framework/index.ts`
 * states that no Claude Code / OpenCode / Codex backend is admitted as an
 * execution authority, so these mark rows that cannot run.
 *
 * Deliberately geometric rather than reproductions of the vendors' logos —
 * these identify rows in our own settings list, and shipping someone's
 * trademark to do that is neither necessary nor ours to do.
 *
 * API matches what it replaces (`size`, `className`) so call sites are unchanged.
 */

type BackendIconProps = {
  size?: number
  className?: string
}

const base = (size: number) => ({
  width: size,
  height: size,
  viewBox: '0 0 24 24',
  fill: 'none' as const,
  xmlns: 'http://www.w3.org/2000/svg',
  'aria-hidden': true
})

/** Claude Code — radial burst. */
export const ClaudeMark = ({ size = 24, className }: BackendIconProps) => (
  <svg {...base(size)} className={className}>
    <g stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <path d="M12 3.5v5M12 15.5v5M3.5 12h5M15.5 12h5" />
      <path d="M6.4 6.4l3.5 3.5M14.1 14.1l3.5 3.5M17.6 6.4l-3.5 3.5M9.9 14.1l-3.5 3.5" />
    </g>
  </svg>
)

/** Codex — terminal chevron and caret. */
export const CodexMark = ({ size = 24, className }: BackendIconProps) => (
  <svg {...base(size)} className={className}>
    <rect
      x="2.75"
      y="4.75"
      width="18.5"
      height="14.5"
      rx="2.5"
      stroke="currentColor"
      strokeWidth="1.6"
    />
    <path
      d="M7 9.5l2.75 2.5L7 14.5M12.5 15h4.5"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
)

/** OpenCode — open bracket pair. */
export const OpenCodeMark = ({ size = 24, className }: BackendIconProps) => (
  <svg {...base(size)} className={className}>
    <path
      d="M9 4.5C6 4.5 6.5 10 4 12c2.5 2 2 7.5 5 7.5M15 4.5c3 0 2.5 5.5 5 7.5-2.5 2-2 7.5-5 7.5"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
)
