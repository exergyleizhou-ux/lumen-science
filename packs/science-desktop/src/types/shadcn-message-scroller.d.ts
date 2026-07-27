/**
 * Local type surface for `@shadcn/react/message-scroller` (LS5-D1-02).
 *
 * READ THIS BEFORE TREATING IT AS "FIXED": this is a REAL RUNTIME DEPENDENCY of reachable code and
 * it is NOT installed. components/ui/message-scroller.tsx wraps this primitive and
 * WorkspaceMessageScroller renders the wrapper, so the transcript will fail at bundle/run time even
 * though it typechecks. This file documents a known gap; it does not close one.
 *
 * WHY IT IS NOT INSTALLED
 * `@shadcn/react@0.2.1` exists on the public registry and does export `./message-scroller`, but its
 * peer range is `react: ">=19"` while this pack pins react ^18.3.1. Installing it needs
 * --legacy-peer-deps — recording a resolution npm says is wrong — which would trade a visible
 * failure for a hidden one. Same blocker as @lobehub/icons; see renderer-ui-deps.d.ts for the two
 * real options (move the pack to React 19, or vendor the scroller).
 *
 * SCOPE RULE: only the parts and props the wrapper forwards. Prop names are read off the wrapper's
 * own call sites (scrollAnchor, direction, render, autoScroll, defaultScrollPosition,
 * scrollPreviousItemPeek), which is the whole contract this codebase depends on.
 */
declare module '@shadcn/react/message-scroller' {
  import type * as React from 'react'

  type DivPart<P = object> = React.ForwardRefExoticComponent<
    React.PropsWithoutRef<React.HTMLAttributes<HTMLDivElement>> & P & React.RefAttributes<HTMLDivElement>
  >

  export const MessageScroller: {
    /** Owns the scroll state; every other part must be rendered inside it. */
    Provider: React.FC<{
      children?: React.ReactNode
      /** Stick to the newest item while the user is at the bottom. */
      autoScroll?: boolean
      defaultScrollPosition?: 'last-anchor' | 'start' | 'end'
      /** Pixels of the previous item left visible when jumping to an anchor. */
      scrollPreviousItemPeek?: number
    }>
    Root: DivPart
    Viewport: DivPart
    Content: DivPart
    /**
     * `scrollAnchor` marks the item auto-scroll should settle on; `messageId` is the stable
     * identity the scroller keys restored positions by (every reachable call site passes it).
     */
    Item: DivPart<{ scrollAnchor?: boolean; messageId?: string }>
    /** Jump-to-end/start affordance; `render` supplies the element to clone the behaviour onto. */
    Button: React.ForwardRefExoticComponent<
      React.PropsWithoutRef<React.ButtonHTMLAttributes<HTMLButtonElement>> & {
        direction?: 'start' | 'end'
        render?: React.ReactElement
      } & React.RefAttributes<HTMLButtonElement>
    >
  }

  export function useMessageScroller(): {
    scrollToEnd: () => void
    scrollToStart: () => void
    isAtEnd: boolean
    isAtStart: boolean
  }

  /** Whether the viewport currently overflows (i.e. scrolling is possible at all). */
  export function useMessageScrollerScrollable(): boolean

  /** Whether the jump affordance for a direction should be shown. */
  export function useMessageScrollerVisibility(direction?: 'start' | 'end'): boolean
}
