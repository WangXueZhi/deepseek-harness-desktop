import { JSDOM } from 'jsdom'
import { describe, expect, it, vi } from 'vitest'

import { createLauncher } from '../ui/app.js'

function createDocument(): Document {
  return new JSDOM(`
    <main>
      <h1 id="title"></h1>
      <p id="message"></p>
      <div id="detail"></div>
      <div id="spinner"></div>
      <div id="actions" class="hidden"></div>
      <button id="retry"></button>
      <button id="workspace"></button>
      <button id="data-dir"></button>
    </main>
  `).window.document
}

describe('desktop launcher', () => {
  it('navigates to the ready Harness service', () => {
    const document = createDocument()
    const navigate = vi.fn()
    const launcher = createLauncher({ document, invoke: vi.fn(), navigate })

    launcher.setStatus({ state: 'ready', port: 3182 })

    expect(navigate).toHaveBeenCalledWith('http://127.0.0.1:3182')
    expect(document.querySelector('#title')?.textContent).toBe('正在打开 Harness')
  })

  it('shows the retry actions for a failed start', async () => {
    const document = createDocument()
    const launcher = createLauncher({
      document,
      invoke: vi.fn().mockRejectedValue(new Error('runtime missing')),
      navigate: vi.fn(),
    })

    await launcher.start()

    expect(document.querySelector('#title')?.textContent).toBe('Harness 启动失败')
    expect(document.querySelector('#detail')?.textContent).toContain('runtime missing')
    expect(document.querySelector('#actions')?.classList.contains('hidden')).toBe(false)
  })

  it('reports the selected workspace', async () => {
    const document = createDocument()
    const invoke = vi.fn().mockResolvedValue('/tmp/harness-workspace')
    createLauncher({ document, invoke, navigate: vi.fn() })

    ;(document.querySelector('#workspace') as HTMLButtonElement).click()
    await vi.waitFor(() => {
      expect(document.querySelector('#detail')?.textContent).toBe('已选择：/tmp/harness-workspace')
    })
  })

  it('reports the application data directory', async () => {
    const document = createDocument()
    const invoke = vi.fn().mockResolvedValue('/tmp/harness-data')
    createLauncher({ document, invoke, navigate: vi.fn() })

    ;(document.querySelector('#data-dir') as HTMLButtonElement).click()
    await vi.waitFor(() => {
      expect(document.querySelector('#detail')?.textContent).toBe('数据目录：/tmp/harness-data')
    })
  })
})
