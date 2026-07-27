/**
 * Local type surface for `@lobehub/icons` deep entry points (LS5-D1-02).
 *
 * READ THIS BEFORE TREATING IT AS "FIXED": unlike the other declarations in this directory, this
 * one covers a package that is a REAL RUNTIME DEPENDENCY of reachable code, and it is NOT
 * installed. `src/renderer/src/pages/settings/AgentPanel.tsx` renders these three brand marks, so
 * the Agent settings panel will fail at bundle/run time even though it typechecks. This file
 * therefore documents a known gap; it does not close one.
 *
 * WHY IT IS NOT INSTALLED
 * Every other renderer package the absorb dropped was installed as a real dependency under
 * LS5-D1-02. These two could not be, and the reason is the same for both:
 *
 *   @lobehub/icons  >= 2.0.0  peer react ^19.0.0     (this pack pins react ^18.3.1)
 *                   1.x       peer react >=18, but drags in antd, @lobehub/ui, antd-style and
 *                             react-layout-kit — an entire component framework for three SVGs
 *   @shadcn/react   0.2.1     peer react >=19        (see shadcn-message-scroller.d.ts)
 *
 * Installing either against React 18 requires --legacy-peer-deps, i.e. knowingly recording a
 * resolution npm says is wrong, which would trade a visible failure for a hidden one. The real fix
 * is a decision this task cannot make: either move the pack to React 19, or replace these three
 * icons with local SVGs (the lucide set is already a dependency). Tracked, not silently green.
 *
 * SCOPE RULE: one default-exported SVG component per deep path, which is all AgentPanel imports.
 */

declare module '@lobehub/icons/es/Claude/components/Color' {
  import type * as React from 'react'
  const Icon: React.FC<React.SVGProps<SVGSVGElement> & { size?: number | string }>
  export default Icon
}

declare module '@lobehub/icons/es/Codex/components/Mono' {
  import type * as React from 'react'
  const Icon: React.FC<React.SVGProps<SVGSVGElement> & { size?: number | string }>
  export default Icon
}

declare module '@lobehub/icons/es/OpenCode/components/Mono' {
  import type * as React from 'react'
  const Icon: React.FC<React.SVGProps<SVGSVGElement> & { size?: number | string }>
  export default Icon
}
