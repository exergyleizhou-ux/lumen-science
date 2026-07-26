#!/usr/bin/env node
/**
 * Scripted stand-in for `lumen agent stdio`, used by
 * scripts/test-acp-stdio-client.mts.
 *
 * It speaks the same NDJSON JSON-RPC wire as the real binary so the transport,
 * process manager and session manager can be driven through their failure
 * modes — crash, garbage on stdout, oversized frame, silence — without needing
 * a 400MB Rust build present. The live exchange against the real binary is
 * proved separately by scripts/test-acp-live-handshake.mts; this fixture
 * exists for the paths a healthy binary will not produce on demand.
 *
 * Mode comes from FAKE_LUMEN_MODE:
 *   good      full handshake, project_list -> []
 *   crash     exit 3 immediately (never handshakes)
 *   crash-mid handshake, then exit 9 on the first science call
 *   garbage   handshake, then a non-JSON line on stdout
 *   huge      handshake, then an unterminated oversized frame
 *   silent    accepts input, never answers
 *   ask       handshake, then an agent->client request before answering
 *
 * Records argv to FAKE_LUMEN_ARGV_FILE so the caller can assert the child was
 * launched as `agent stdio` and not some invented subcommand.
 */
import fs from 'node:fs'

const mode = process.env.FAKE_LUMEN_MODE ?? 'good'
const argvFile = process.env.FAKE_LUMEN_ARGV_FILE
if (argvFile) fs.writeFileSync(argvFile, JSON.stringify(process.argv.slice(2)))

if (mode === 'crash') {
  process.stderr.write('fake-lumen: refusing to start (scripted crash)\n')
  process.exit(3)
}

const send = (msg) => process.stdout.write(`${JSON.stringify(msg)}\n`)

let buffer = ''
process.stdin.setEncoding('utf8')
process.stdin.on('data', (chunk) => {
  buffer += chunk
  let index = buffer.indexOf('\n')
  while (index >= 0) {
    const line = buffer.slice(0, index)
    buffer = buffer.slice(index + 1)
    if (line.trim()) handle(JSON.parse(line))
    index = buffer.indexOf('\n')
  }
})

function handle(msg) {
  if (mode === 'silent') return
  // Responses to our own agent->client request: ignore.
  if (msg.method === undefined) return

  switch (msg.method) {
    case 'initialize':
      send({
        jsonrpc: '2.0',
        id: msg.id,
        result: {
          protocolVersion: 1,
          authMethods: [
            { id: 'grok.com', name: 'Grok' },
            { id: 'xai.api_key', name: 'xai.api_key' },
          ],
          _meta: { defaultAuthMethodId: 'xai.api_key' },
        },
      })
      return
    case 'authenticate':
      send({ jsonrpc: '2.0', id: msg.id, result: {} })
      return
    case 'session/new':
      send({ jsonrpc: '2.0', id: msg.id, result: { sessionId: 'fake-session-1' } })
      return
    default:
      break
  }

  if (!msg.method.startsWith('_x.ai/science/')) {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      error: { code: -32601, message: 'Method not found' },
    })
    return
  }

  if (mode === 'crash-mid') {
    process.stderr.write('fake-lumen: dying mid-call\n')
    process.exit(9)
  }
  if (mode === 'garbage') {
    process.stdout.write('this line is not JSON\n')
    return
  }
  if (mode === 'huge') {
    process.stdout.write('x'.repeat(4096))
    return
  }
  if (mode === 'ask') {
    send({ jsonrpc: '2.0', id: 9001, method: 'session/request_permission', params: {} })
    send({ jsonrpc: '2.0', id: msg.id, result: { asked: true } })
    return
  }

  // `good`: echo the params back so the caller can assert sessionId injection.
  send({ jsonrpc: '2.0', id: msg.id, result: { method: msg.method, params: msg.params } })
}
