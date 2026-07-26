#!/usr/bin/env npx tsx
/**
 * Authority boundary test — EXECUTES shipped lumen-authority-policy.ts.
 * Run: npx tsx scripts/authority-verify.mts
 */
import { strictEqual, ok } from 'node:assert/strict';
import fs from 'node:fs';
let failures = 0;

function test(name: string, fn: () => void) {
  try { fn(); console.log(`OK  ${name}`); }
  catch (e: unknown) { failures++; console.log(`FAIL ${name}: ${(e as Error).message}`); }
}

// ── EXECUTE shipped policy ──────────────────────────────────────

import { validateIpcChannel, assertArtifactPreviewAccess, getBannedChannels } from '../src/main/lumen-authority-policy.js'

console.log('POLICY-MODULE: executing shipped validateIpcChannel + assertArtifactPreviewAccess')

const banned = getBannedChannels()
test('validate rejects artifacts:finalize-run', () => strictEqual(validateIpcChannel('artifacts:finalize-run'), false));
test('validate rejects artifacts:open-file', () => strictEqual(validateIpcChannel('artifacts:open-file'), false));
test('validate rejects artifacts:read-preview', () => strictEqual(validateIpcChannel('artifacts:read-preview'), false));
test('validate rejects projects:create', () => strictEqual(validateIpcChannel('projects:create'), false));
test('validate rejects projects:delete', () => strictEqual(validateIpcChannel('projects:delete'), false));
test('validate rejects reviewer:run', () => strictEqual(validateIpcChannel('reviewer:run'), false));
test('validate rejects reviewer:abort-fix-loop', () => strictEqual(validateIpcChannel('reviewer:abort-fix-loop'), false));
test('validate rejects compute:job-updated', () => strictEqual(validateIpcChannel('compute:job-updated'), false));
test('validate rejects preview:load', () => strictEqual(validateIpcChannel('preview:load'), false));
test('validate rejects preview:save', () => strictEqual(validateIpcChannel('preview:save'), false));
test('validate rejects preview:delete', () => strictEqual(validateIpcChannel('preview:delete'), false));

test('validate allows acp:call', () => strictEqual(validateIpcChannel('acp:call'), true));
test('validate allows acp:list-tools', () => strictEqual(validateIpcChannel('acp:list-tools'), true));
test('validate allows window:close', () => strictEqual(validateIpcChannel('window:close'), true));
test('validate default-deny unknown', () => strictEqual(validateIpcChannel('random:unknown'), false));

test('getBannedChannels is Set with >=15 entries', () => {
  ok(banned instanceof Set, `type=${typeof banned}`);
  ok(banned.size >= 15, `size=${banned.size}`);
});

// ── Artifact preview access ──────────────────────────────────────

const r1 = assertArtifactPreviewAccess(
  { artifactId: 'a1', ownerId: 'o1', projectId: 'p1', expectedSha256: 'abc' },
  { ownerId: 'o1', projectId: 'p1', digest: 'abc' }
);
test('artifact access: allows valid', () => ok(r1.ok));

const r2 = assertArtifactPreviewAccess(
  { artifactId: '', ownerId: 'o1', projectId: 'p1' },
  { ownerId: 'o1', projectId: 'p1' }
);
test('artifact access: rejects empty artifact_id', () => { ok(!r2.ok); ok(r2.reason!.includes('required')); });

const r3 = assertArtifactPreviewAccess(
  { artifactId: 'a1', ownerId: 'oX', projectId: 'p1' },
  { ownerId: 'o1', projectId: 'p1' }
);
test('artifact access: rejects wrong owner', () => { ok(!r3.ok); ok(r3.reason!.includes('owner')); });

const r4 = assertArtifactPreviewAccess(
  { artifactId: 'a1', ownerId: 'o1', projectId: 'p1', expectedSha256: 'aaa' },
  { ownerId: 'o1', projectId: 'p1', digest: 'bbb' }
);
test('artifact access: rejects hash mismatch', () => { ok(!r4.ok); ok(r4.reason!.includes('sha256')); });

// ── ipc.ts banned imports + safeHandle wiring ───────────────────

const IPC = fs.readFileSync('src/main/ipc.ts', 'utf-8');
for (const sym of ['SystemSshRunner', 'SystemScpRunner', 'JobPoller', 'harvestJob', 'registerComputeIpcHandlers', 'registerNotebookIpcHandlers', 'registerReviewerIpcHandlers']) {
  test(`ipc.ts: no ${sym}`, () => ok(!IPC.includes(sym)));
}
test('ipc.ts imports safeHandle', () => ok(IPC.includes('safeHandle')));
test('ipc.ts imports assertArtifactPreviewAccess', () => ok(IPC.includes('assertArtifactPreviewAccess')));

// ── Skills boundary ──────────────────────────────────────────────

const reg = JSON.parse(fs.readFileSync('../../packs/science/skills/registry.json', 'utf-8'));
const approvedIds = new Set(reg.skills.filter((s: any) => s.final_disposition === 'approved').map((s: any) => s.skill_id));
test('Lumen approved=10', () => strictEqual(approvedIds.size, 10));
for (const id of ['alphafold2', 'boltz', 'evo2', 'diffdock', 'esmfold2', 'proteinmpnn']) {
  test(`OS skill ${id} NOT in Lumen`, () => ok(!approvedIds.has(id)));
}
test('pending unchanged', () => strictEqual(reg.summary.pending, 17));

// ── Branding ─────────────────────────────────────────────────────

test('electron-builder.yml branded Lumen', () => {
  const c = fs.readFileSync('electron-builder.yml', 'utf-8');
  ok(!c.includes('CFBundleName: Open Science'));
  ok(c.includes('Lumen Science Desktop'));
});

// ── Brace balance ─────────────────────────────────────────────────

test('ipc.ts brace balanced 0', () => {
  let b = 0;
  for (const l of IPC.split('\n')) b += (l.match(/\{/g)||[]).length - (l.match(/\}/g)||[]).length;
  strictEqual(b, 0, `imbalance=${b}`);
});

console.log(`\n${failures === 0 ? 'ALL TESTS PASSED' : `${failures} TESTS FAILED`}`);
process.exit(failures > 0 ? 1 : 0);
