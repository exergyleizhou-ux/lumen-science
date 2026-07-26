/**
 * LUMEN STUB: Notebook IPC handlers — execution authority REMOVED.
 *
 * Original: Open Science v0.7.1, Apache-2.0, commit d8f11e34
 *   Full notebook kernel lifecycle IPC.
 *
 * Lumen Science Desktop: stubbed. Notebook execution is owned by
 * Rust Lumen KernelAdapter. This module exists for IPC graph compatibility.
 */

export type NotebookHandlers = Record<string, (...args: unknown[]) => unknown>

export const registerNotebookIpcHandlers = (
  _runtimeService?: unknown
) => {
  console.warn('[lumen-stub] registerNotebookIpcHandlers — EXECUTION AUTHORITY REMOVED. Route via Lumen bridge.')
}

export const createNotebookHandlers = () => ({} as NotebookHandlers)
