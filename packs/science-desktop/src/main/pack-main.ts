/**
 * Minimal main process used for electron-builder pack proof (1.1.0-dev).
 *
 * Full product entry remains `src/main/index.ts` (dev / full typecheck debt).
 * This entry only proves: Lumen branding, appId surfaces, window title, and
 * that electron-builder can package a real `out/main/index.js` without fake
 * notarization claims.
 */
import { app, BrowserWindow } from 'electron'
import path from 'node:path'

const APP_NAME = 'Lumen Science Desktop'
const APP_USER_MODEL_ID = 'com.exergyleizhou-ux.lumen-science-desktop'

// esbuild CJS pack output: __dirname is the real pack-main directory (out/main).
declare const __dirname: string

app.setName(app.isPackaged ? APP_NAME : `${APP_NAME} (DEV)`)
if (process.platform === 'win32') {
  app.setAppUserModelId(APP_USER_MODEL_ID)
}

function createWindow(): void {
  const win = new BrowserWindow({
    width: 960,
    height: 640,
    title: APP_NAME,
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  })
  const html = path.join(__dirname, '../renderer/index.html')
  void win.loadFile(html)
}

app.whenReady().then(() => {
  createWindow()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit()
})
