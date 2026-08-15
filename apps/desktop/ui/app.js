function requiredElement(document, selector) {
  const element = document.querySelector(selector)
  if (!element) throw new Error(`missing launcher element: ${selector}`)
  return element
}

export function createLauncher({ document, invoke, navigate }) {
  const title = requiredElement(document, '#title')
  const message = requiredElement(document, '#message')
  const detail = requiredElement(document, '#detail')
  const spinner = requiredElement(document, '#spinner')
  const actions = requiredElement(document, '#actions')

  function setStatus(status) {
    const ready = status.state === 'ready'
    const failed = status.state === 'failed'
    const stopped = status.state === 'stopped'
    title.textContent = ready
        ? '正在打开 Harness'
      : failed
        ? 'Harness 启动失败'
        : stopped
          ? 'Harness 已停止'
          : '正在启动 DeepSeek Harness'
    message.textContent = ready
      ? '本地服务已就绪，正在加载 Web UI。'
      : failed
        ? '请查看启动日志中的详细错误，然后点击重试。'
        : stopped
          ? '本地服务当前未运行。'
          : '正在准备本地 Agent 服务，请稍候。'
    detail.textContent = status.error || (status.port ? `本地端口：${status.port}` : '')
    spinner.classList.toggle('hidden', failed || stopped)
    actions.classList.toggle('hidden', !failed && !stopped)
    if (ready && status.port) navigate(`http://127.0.0.1:${status.port}`)
  }

  async function start() {
    try {
      setStatus(await invoke('start_dsh'))
    } catch (error) {
      setStatus({ state: 'failed', error: String(error) })
    }
  }

  requiredElement(document, '#retry').addEventListener('click', start)
  requiredElement(document, '#workspace').addEventListener('click', async () => {
    try {
      const path = await invoke('select_workspace')
      if (path) detail.textContent = `已选择：${path}`
    } catch (error) {
      detail.textContent = `workspace 选择失败：${String(error)}`
    }
  })
  requiredElement(document, '#data-dir').addEventListener('click', async () => {
    try {
      detail.textContent = `数据目录：${await invoke('open_app_data_dir')}`
    } catch (error) {
      detail.textContent = `打开数据目录失败：${String(error)}`
    }
  })

  return { setStatus, start }
}

if (typeof window !== 'undefined' && window.__TAURI__) {
  const launcher = createLauncher({
    document: window.document,
    invoke: (command, args) => window.__TAURI__.core.invoke(command, args),
    navigate: url => window.location.replace(url),
  })
  launcher.start()
}
