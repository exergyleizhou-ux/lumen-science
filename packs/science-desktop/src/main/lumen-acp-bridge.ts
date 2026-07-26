/**
 * Lumen ACP Bridge — Electron main → Rust Lumen binary.
 *
 * Replaces the Open Science agent-framework execution authority.
 * All science operations go through the Rust SessionActor via ACP.
 * Electron main process is ONLY responsible for window/tray/updater.
 *
 * NOT an execution path for science tools, notebooks, or reviewers.
 *
 * Apache-2.0. Adapted from Open Science (d8f11e34) and modified for
 * Lumen Science Desktop authority model.
 */

import { ChildProcess, spawn } from 'child_process';
import { app } from 'electron';
import path from 'path';
import fs from 'fs';
import crypto from 'crypto';

// ── Binary discovery ─────────────────────────────────────────────

function lumenBinaryPath(): string {
  // 1. BUNDLED_LUMEN env override (dev/testing)
  if (process.env.LUMEN_BINARY) {
    return process.env.LUMEN_BINARY;
  }
  // 2. App resources (production packaging)
  const resourcesDir = path.join(process.resourcesPath || app.getAppPath(), 'bin');
  const platform = process.platform;
  const ext = platform === 'win32' ? '.exe' : '';
  const candidate = path.join(resourcesDir, `lumen-science${ext}`);
  if (fs.existsSync(candidate)) {
    return candidate;
  }
  // 3. PATH fallback (development)
  return `lumen-science${ext}`;
}

// ── Lumen process lifecycle ──────────────────────────────────────

let lumenProcess: ChildProcess | null = null;
let binaryHash: string | null = null;

export function getLumenBinaryHash(): string | null {
  return binaryHash;
}

export async function startLumen(): Promise<void> {
  const bin = lumenBinaryPath();

  // Compute binary hash for attestation
  if (fs.existsSync(bin)) {
    const buf = fs.readFileSync(bin);
    binaryHash = crypto.createHash('sha256').update(buf).digest('hex');
  }

  lumenProcess = spawn(bin, ['serve', '--interface', 'loopback', '--port', '17000'], {
    stdio: ['pipe', 'pipe', 'pipe'],
    env: {
      ...process.env,
      LUMEN_DESKTOP: '1',
      LUMEN_NO_BROWSER: '1',
    },
  });

  lumenProcess.on('exit', (code) => {
    console.log(`Lumen binary exited with code ${code}`);
    lumenProcess = null;
  });

  lumenProcess.stderr?.on('data', (data: Buffer) => {
    console.error(`[lumen] ${data.toString().trim()}`);
  });

  // Wait for ACP handshake readiness
  await waitForAcpReady();
}

export function stopLumen(): void {
  if (lumenProcess) {
    lumenProcess.kill('SIGTERM');
    setTimeout(() => {
      if (lumenProcess) lumenProcess.kill('SIGKILL');
    }, 5000);
  }
}

async function waitForAcpReady(timeoutMs = 30000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const resp = await fetch('http://127.0.0.1:17000/health');
      if (resp.ok) return;
    } catch {
      // Not ready yet
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error('Lumen binary did not become ready within timeout');
}

// ── Authority boundary enforcement ───────────────────────────────

/**
 * Allowed IPC channels from renderer.
 * Any science-state channel (project create, artifact write, etc.)
 * MUST route through ACP to the Rust binary, NOT through Electron main.
 */
const ALLOWED_RENDERER_CHANNELS = new Set([
  // UI-only channels
  'window:minimize',
  'window:maximize',
  'window:close',
  'window:toggle-fullscreen',
  'app:quit',
  'app:get-version',
  'app:get-lumen-hash',
  'tray:update',
  'updater:check',
  'updater:install',
  'settings:get',
  'settings:set',
  'dialog:open-file',
  'dialog:save-file',
  'notification:show',
  'clipboard:write',
  'session:restore-layout',
  'session:save-layout',

  // ACP-proxied science channels (validated by Rust)
  'acp:call',
  'acp:list-tools',
  'acp:health',
]);

/**
 * BANNED channels — these would bypass SessionActor.
 * Electron main MUST reject any IPC on these paths.
 */
const BANNED_CHANNELS = new Set([
  'project:create',
  'project:delete',
  'artifact:write',
  'artifact:read',
  'notebook:execute',
  'reviewer:accept',
  'connector:fetch',
  'skill:approve',
  'compute:submit',
  'evidence:attach',
  'device:command',
]);

export function validateIpcChannel(channel: string): boolean {
  if (BANNED_CHANNELS.has(channel)) {
    console.error(`[security] BANNED IPC channel rejected: ${channel}`);
    return false;
  }
  return ALLOWED_RENDERER_CHANNELS.has(channel);
}

// ── ACP proxy (renderer -> Electron main -> Rust Lumen) ──────────

export async function acpCall(toolName: string, args: Record<string, unknown>): Promise<unknown> {
  const resp = await fetch('http://127.0.0.1:17000/tools/call', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: toolName, arguments: args }),
  });
  return resp.json();
}
