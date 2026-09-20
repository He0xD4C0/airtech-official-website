import { spawn } from 'node:child_process'

export function testProcess(environment) {
  let current
  let interrupted = false
  function interrupt() {
    interrupted = true
    if (current) process.kill(-current.pid, 'SIGTERM')
  }
  process.on('SIGINT', interrupt)
  process.on('SIGTERM', interrupt)
  async function run(command, args, { capture = false, cleanup = false } = {}) {
    if (interrupted && !cleanup) throw new Error('Test interrupted; cleaning owned resources.')
    return await new Promise((resolve, reject) => {
      const child = spawn(command, args, {
        env: environment, stdio: capture ? ['ignore', 'pipe', 'pipe'] : 'inherit', detached: true,
      })
      current = child
      let stdout = ''
      let stderr = ''
      child.stdout?.on('data', (chunk) => { stdout += chunk })
      child.stderr?.on('data', (chunk) => { stderr += chunk })
      child.on('error', reject)
      child.on('close', (status) => {
        current = undefined
        resolve(capture ? { status: status ?? 1, stdout, stderr } : status ?? 1)
      })
    })
  }
  return { run, capture: (command, args) => run(command, args, { capture: true }) }
}
