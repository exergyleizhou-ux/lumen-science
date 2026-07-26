/**
 * Local type surface for `ws` (LS5-D1-02).
 *
 * WHY THIS FILE EXISTS
 * `src/main/web-service/http-server.ts` is reachable from `src/main/index.ts` (it serves the
 * headless/browser client), but `ws` was dropped from package.json during the Open Science absorb
 * and is not reinstated here — adding a runtime dependency is out of scope for a typecheck gate,
 * and the decision of whether this pack ships a websocket server belongs to the web-service task,
 * not to this one. See the LS5-D1-02 report: `npm run build` still fails to resolve `ws` at bundle
 * time; that is a real, separately-tracked gap and this declaration does not paper over it — it
 * only stops an unresolved import from masking the type errors around it.
 *
 * SCOPE RULE: only the members http-server.ts actually touches. Shapes follow @types/ws.
 */
declare module 'ws' {
  import type { IncomingMessage } from 'node:http'
  import type { Duplex } from 'node:stream'

  /** Payload forms http-server.ts sends (it only ever sends pre-serialized JSON strings). */
  type WebSocketData = string | Buffer | ArrayBuffer | Uint8Array

  export class WebSocket {
    /** readyState constants; http-server.ts compares against OPEN before broadcasting. */
    static readonly CONNECTING: 0
    static readonly OPEN: 1
    static readonly CLOSING: 2
    static readonly CLOSED: 3

    readonly readyState: 0 | 1 | 2 | 3

    send(data: WebSocketData): void
    close(code?: number, reason?: string): void

    on(event: 'close', listener: (code: number, reason: Buffer) => void): this
    on(event: 'error', listener: (error: Error) => void): this
    on(event: 'message', listener: (data: WebSocketData, isBinary: boolean) => void): this
  }

  export type WebSocketServerOptions = {
    /** http-server.ts owns the HTTP server and routes `upgrade` itself, so it always passes true. */
    noServer?: boolean
    host?: string
    port?: number
    path?: string
  }

  export class WebSocketServer {
    constructor(options?: WebSocketServerOptions)

    /** Completes a hijacked HTTP upgrade and hands back the established socket. */
    handleUpgrade(
      request: IncomingMessage,
      socket: Duplex,
      head: Buffer,
      callback: (socket: WebSocket, request: IncomingMessage) => void
    ): void

    on(
      event: 'connection',
      listener: (socket: WebSocket, request: IncomingMessage) => void
    ): this
    on(event: 'error', listener: (error: Error) => void): this
    on(event: 'close', listener: () => void): this

    /** Re-dispatches a socket produced by handleUpgrade through the normal connection listeners. */
    emit(event: 'connection', socket: WebSocket, request: IncomingMessage): boolean

    close(callback?: (error?: Error) => void): void
  }
}
