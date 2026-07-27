/**
 * Local type surface for `@tailwindcss/vite` (LS5-D1-02).
 *
 * WHY THIS FILE EXISTS
 * `vite.web.config.ts` is in the node tsconfig's `include` list (build configs are typechecked), and
 * it imports the Tailwind v4 Vite plugin statically. The package was dropped during the Open Science
 * absorb. `electron.vite.config.ts` already treats it as optional — it `require`s it inside a
 * try/catch and skips the plugin when absent — so the pack's real build path does not depend on it.
 * This declaration keeps the second config typechecking without reinstating the dependency and
 * without changing either config's runtime behavior.
 *
 * SCOPE RULE: the plugin factory is the entire surface used (`tailwindcss()` in the plugins array).
 */
declare module '@tailwindcss/vite' {
  import type { PluginOption } from 'vite'

  /** Tailwind v4's Vite plugin factory; takes no options in either config here. */
  export default function tailwindcss(): PluginOption
}
