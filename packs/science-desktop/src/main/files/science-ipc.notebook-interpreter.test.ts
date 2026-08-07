import { describe, expect, it } from 'vitest'

import { selectNotebookInterpreter } from './science-ipc'

describe('selectNotebookInterpreter', () => {
  it('prefers an OS-owned Python request over a PATH-first Homebrew candidate on macOS', () => {
    const selected = selectNotebookInterpreter(
      [
        { interpreterPath: '/opt/homebrew/bin/python3' },
        { interpreterPath: '/usr/bin/python3' },
        {
          interpreterPath:
            '/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/Resources/Python.app/Contents/MacOS/Python',
        },
      ],
      'darwin',
    )

    expect(selected?.interpreterPath).toBe(
      '/Library/Developer/CommandLineTools/Library/Frameworks/Python3.framework/Versions/3.9/Resources/Python.app/Contents/MacOS/Python',
    )
  })

  it('does not invent a protected path when discovery did not observe one', () => {
    const selected = selectNotebookInterpreter(
      [{ interpreterPath: '/custom/python3' }],
      'linux',
    )

    expect(selected?.interpreterPath).toBe('/custom/python3')
  })

  it('keeps the discovery order on platforms without an OS protected-path contract', () => {
    const selected = selectNotebookInterpreter(
      [
        { interpreterPath: 'C:\\Python312\\python.exe' },
        { interpreterPath: 'D:\\Python311\\python.exe' },
      ],
      'win32',
    )

    expect(selected?.interpreterPath).toBe('C:\\Python312\\python.exe')
  })
})
