/**
 * electron-builder afterPack hook.
 *
 * On macOS: apply deep ad-hoc codesign so Gatekeeper does not mark the
 * unpacked .app as damaged when no Developer ID is available.
 * On other platforms: no-op (safe).
 *
 * This does NOT perform Developer ID signing or notarization.
 */
'use strict'

const { execFileSync } = require('node:child_process')
const fs = require('node:fs')
const path = require('node:path')

exports.default = async function afterPack(context) {
  if (context.electronPlatformName !== 'darwin') {
    return
  }
  const appName = context.packager.appInfo.productFilename
  const appPath = path.join(context.appOutDir, `${appName}.app`)
  if (!fs.existsSync(appPath)) {
    console.warn(`[adhoc-sign] skip: missing ${appPath}`)
    return
  }
  try {
    execFileSync(
      'codesign',
      ['--force', '--deep', '--sign', '-', appPath],
      { stdio: 'inherit' },
    )
    console.log(`[adhoc-sign] ad-hoc signed ${appPath}`)
  } catch (err) {
    console.warn(`[adhoc-sign] codesign failed (non-fatal for unsigned CI): ${err}`)
  }
}
