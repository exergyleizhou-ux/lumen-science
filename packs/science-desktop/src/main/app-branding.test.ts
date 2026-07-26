import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import { describe, expect, it } from 'vitest'

const projectRoot = resolve(__dirname, '../..')
const displayName = 'Lumen Science Desktop'
const appId = 'com.exergyleizhou-ux.lumen-science-desktop'

describe('app display branding', () => {
  it('uses the Lumen Science Desktop display name in shell and workspace surfaces', () => {
    const mainSource = readFileSync(resolve(projectRoot, 'src/main/index.ts'), 'utf8')
    const windowsSource = readFileSync(resolve(projectRoot, 'src/main/windows.ts'), 'utf8')
    const rendererHtmlSource = readFileSync(resolve(projectRoot, 'src/renderer/index.html'), 'utf8')
    const builderSource = readFileSync(resolve(projectRoot, 'electron-builder.yml'), 'utf8')
    const packageSource = readFileSync(resolve(projectRoot, 'package.json'), 'utf8')
    // The product name anchors the home screen brand (the workspace sidebar shows the project).
    const homePageSource = readFileSync(
      resolve(projectRoot, 'src/renderer/src/pages/home/HomePage.tsx'),
      'utf8'
    )

    expect(mainSource).toContain(`const APP_NAME = '${displayName}'`)
    expect(windowsSource).toContain(`title: '${displayName}'`)
    expect(rendererHtmlSource).toContain(`<title>${displayName}</title>`)
    expect(builderSource).toContain(`productName: ${displayName}`)
    expect(builderSource).toContain(`CFBundleName: ${displayName}`)
    expect(builderSource).toContain(`CFBundleDisplayName: ${displayName}`)
    expect(packageSource).toContain(`"productName": "${displayName}"`)
    expect(homePageSource).toContain(displayName)
  })

  it('uses the Lumen app identifier for packaged and window integration metadata', () => {
    const mainSource = readFileSync(resolve(projectRoot, 'src/main/index.ts'), 'utf8')
    const builderSource = readFileSync(resolve(projectRoot, 'electron-builder.yml'), 'utf8')

    expect(mainSource).toContain(`const APP_USER_MODEL_ID = '${appId}'`)
    expect(builderSource).toContain(`appId: ${appId}`)
    expect(builderSource).not.toContain('com.aipoch.open-science')
    expect(builderSource).not.toContain('executableName: open-science')
  })

  it('opens tall enough to show the complete first-run environment summary', () => {
    const windowsSource = readFileSync(resolve(projectRoot, 'src/main/windows.ts'), 'utf8')

    expect(windowsSource).toContain('height: 960')
    expect(windowsSource).toContain('minHeight: 720')
  })

  it('awaits Electron readiness inside the startup promise chain', () => {
    const mainSource = readFileSync(resolve(projectRoot, 'src/main/index.ts'), 'utf8')

    expect(mainSource).toContain('await app.whenReady()')
    expect(mainSource).not.toContain('app.whenReady().then')
  })
})
