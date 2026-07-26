/**
 * DOM-API gaps in jsdom, filled once, before any test runs.
 *
 * jsdom 26 (the newest line whose dependency chain loads on Node 20 — 27's
 * cssstyle chain needs require(esm), i.e. Node ≥ 22) does not implement
 * several APIs the renderer legitimately uses in browsers:
 *
 *   ResizeObserver          layout-aware panels (ProjectFilesView, previews)
 *   Blob/File .text()       reading an uploaded skill file
 *   URL.createObjectURL     office/pdf preview object URLs
 *
 * Each shim is installed only when missing, so a jsdom upgrade that gains the
 * real API wins automatically. These are deliberately minimal: enough for the
 * components to run, nothing speculative.
 */

if (typeof globalThis.ResizeObserver === 'undefined') {
  class ResizeObserverStub {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }
  globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver
}

// Blob.text()/arrayBuffer() exist in browsers and Node's own Blob, but jsdom's
// File/Blob predate them.
const blobProto = (globalThis.Blob as undefined | typeof Blob)?.prototype as
  | (Blob & { text?: () => Promise<string>; arrayBuffer?: () => Promise<ArrayBuffer> })
  | undefined
if (blobProto && typeof blobProto.text !== 'function') {
  blobProto.text = function text(this: Blob): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader()
      reader.onload = () => resolve(String(reader.result ?? ''))
      reader.onerror = () => reject(reader.error)
      reader.readAsText(this)
    })
  }
}
if (blobProto && typeof blobProto.arrayBuffer !== 'function') {
  blobProto.arrayBuffer = function arrayBuffer(this: Blob): Promise<ArrayBuffer> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader()
      reader.onload = () => resolve(reader.result as ArrayBuffer)
      reader.onerror = () => reject(reader.error)
      reader.readAsArrayBuffer(this)
    })
  }
}

if (typeof URL.createObjectURL !== 'function') {
  let seq = 0
  URL.createObjectURL = () => `blob:vitest-${++seq}`
}
if (typeof URL.revokeObjectURL !== 'function') {
  URL.revokeObjectURL = () => {}
}

// jsdom has no canvas implementation (getContext logs "Not implemented" and
// yields null), and e-virt-table's Paint constructor needs a 2D context. The
// spreadsheet tests exercise CACHE POLICY — window eviction, request
// coalescing — not pixels, so a permissive no-op context is faithful: every
// draw call succeeds and does nothing, measureText reports zero width.
if (typeof HTMLCanvasElement !== 'undefined') {
  const proto = HTMLCanvasElement.prototype as unknown as {
    getContext(kind: string): unknown
  }
  const original = proto.getContext
  proto.getContext = function getContext(this: HTMLCanvasElement, kind: string): unknown {
    if (kind === '2d') {
      const noop = (): void => {}
      return new Proxy(
        { canvas: this, measureText: () => ({ width: 0 }) },
        {
          get(target, prop) {
            if (prop in target) return target[prop as keyof typeof target]
            // Property reads (fillStyle, font …) get undefined via a set/get
            // pair; method calls get a no-op.
            return noop
          },
          set() {
            return true
          },
        },
      )
    }
    try {
      return original.call(this, kind)
    } catch {
      return null
    }
  }
}
