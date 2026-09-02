<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { FitAddon } from '@xterm/addon-fit'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { Circle, Code2, Plus, ShieldAlert, SquareTerminal, Trash2 } from 'lucide-vue-next'
import PageHeader from '@/components/PageHeader.vue'
import StatusBadge from '@/components/StatusBadge.vue'
import { createDevtoolsTerminalToken, devtoolsTerminalUrl } from './devtoolsApi'

interface SessionStartedEvent { type: 'sessionStarted'; sessionId: string; idleTimeoutSeconds: number; absoluteTimeoutSeconds: number; maxOutputBytes: number }
interface SessionEndedEvent { type: 'sessionEnded'; sessionId: string; reason: string; exitCode: number | null }
interface TerminalErrorEvent { type: 'error'; code: string; message: string }
type ServerControlEvent = SessionStartedEvent | SessionEndedEvent | TerminalErrorEvent

const terminalHost = ref<HTMLElement>()
const socket = ref<WebSocket>()
const sessionId = ref<string>()
const connectionState = ref<'disconnected' | 'connecting' | 'connected'>('disconnected')
const connectionLabel = computed(() => ({ disconnected: '未连接', connecting: '连接中', connected: '已连接' })[connectionState.value])

let emulator: Terminal | undefined
let fitAddon: FitAddon | undefined
let resizeObserver: ResizeObserver | undefined
let resizeFrame: number | undefined
let dataDisposable: { dispose: () => void } | undefined
let binaryDisposable: { dispose: () => void } | undefined

function writeLine(value: string, kind: 'info' | 'error' = 'info'): void {
  if (!emulator) return
  emulator.writeln(`${kind === 'error' ? '\u001b[31m' : '\u001b[90m'}${value}\u001b[0m`)
}

function parseControlEvent(value: string): ServerControlEvent | null {
  try {
    const event = JSON.parse(value) as Partial<ServerControlEvent>
    return event.type === 'sessionStarted' || event.type === 'sessionEnded' || event.type === 'error'
      ? event as ServerControlEvent
      : null
  } catch {
    return null
  }
}

function sendControl(value: object): void {
  if (socket.value?.readyState === WebSocket.OPEN) socket.value.send(JSON.stringify(value))
}

function resizePty(): void {
  if (!emulator || !fitAddon) return
  if (resizeFrame !== undefined) cancelAnimationFrame(resizeFrame)
  resizeFrame = requestAnimationFrame(() => {
    resizeFrame = undefined
    try {
      fitAddon?.fit()
      if (emulator) sendControl({ type: 'resize', rows: emulator.rows, cols: emulator.cols })
    } catch {
      writeLine('终端尺寸更新失败；当前会话仍可继续。', 'error')
    }
  })
}

function handleControlEvent(event: ServerControlEvent): void {
  if (event.type === 'sessionStarted') {
    sessionId.value = event.sessionId
    connectionState.value = 'connected'
    writeLine(`PTY ready (${event.sessionId.slice(0, 8)}). Idle timeout ${Math.round(event.idleTimeoutSeconds / 60)} min; absolute timeout ${Math.round(event.absoluteTimeoutSeconds / 3600)} h; output cap ${Math.round(event.maxOutputBytes / 1024 / 1024)} MiB.`)
    resizePty()
    emulator?.focus()
    return
  }
  if (event.type === 'sessionEnded') {
    const exit = event.exitCode === null ? '' : `, exit code ${event.exitCode}`
    writeLine(`PTY ended: ${event.reason}${exit}.`)
    sessionId.value = undefined
    return
  }
  writeLine(`${event.code}: ${event.message}`, 'error')
}

async function connect(): Promise<void> {
  if (connectionState.value !== 'disconnected') return
  connectionState.value = 'connecting'
  writeLine('Requesting a one-time PTY authorization…')
  try {
    const { token } = await createDevtoolsTerminalToken()
    const client = new WebSocket(devtoolsTerminalUrl(token))
    client.binaryType = 'arraybuffer'
    client.addEventListener('open', () => {
      socket.value = client
      writeLine('WebSocket connected. Session and input metadata are audited; command text and terminal output are excluded from audit records.')
      resizePty()
    })
    client.addEventListener('message', async (event) => {
      if (typeof event.data === 'string') {
        const control = parseControlEvent(event.data)
        if (control) handleControlEvent(control)
        else writeLine('The terminal returned an invalid control message.', 'error')
        return
      }
      const bytes = event.data instanceof Blob
        ? new Uint8Array(await event.data.arrayBuffer())
        : new Uint8Array(event.data as ArrayBuffer)
      emulator?.write(bytes)
    })
    client.addEventListener('error', () => {
      writeLine('PTY connection failed. Check the development API session and devtools.shell permission.', 'error')
    })
    client.addEventListener('close', () => {
      if (socket.value === client) socket.value = undefined
      connectionState.value = 'disconnected'
      sessionId.value = undefined
      writeLine('PTY disconnected.')
    })
  } catch (error) {
    connectionState.value = 'disconnected'
    writeLine(error instanceof Error ? error.message : 'Unable to connect the PTY.', 'error')
  }
}

function disconnect(): void {
  socket.value?.close(1000, 'Session closed from Admin Web')
  socket.value = undefined
  sessionId.value = undefined
  connectionState.value = 'disconnected'
}

function initializeTerminal(): void {
  if (!terminalHost.value) return
  emulator = new Terminal({
    allowProposedApi: false,
    convertEol: false,
    cursorBlink: true,
    cursorStyle: 'bar',
    fontFamily: '"SFMono-Regular", Menlo, Consolas, monospace',
    fontSize: 12,
    lineHeight: 1.3,
    scrollback: 5_000,
    theme: {
      background: '#071b22',
      foreground: '#d7e6e7',
      cursor: '#65cb90',
      selectionBackground: '#275563',
      black: '#071b22',
      red: '#ff9f91',
      green: '#65cb90',
      yellow: '#e6c66f',
      blue: '#79b7d1',
      magenta: '#c7a1d6',
      cyan: '#7ac9c8',
      white: '#d7e6e7',
    },
  })
  fitAddon = new FitAddon()
  emulator.loadAddon(fitAddon)
  emulator.open(terminalHost.value)
  emulator.writeln('\u001b[1;36mAIRTEKPOWER development console\u001b[0m')
  writeLine('PTY session: not connected')
  writeLine('The shell inherits the non-root service process UID. Click 新建会话 to connect.')
  dataDisposable = emulator.onData((data) => {
    if (connectionState.value !== 'connected') return
    sendControl({
      type: 'input',
      data,
      command: data.includes('\r') || data.includes('\n'),
    })
  })
  binaryDisposable = emulator.onBinary((data) => {
    if (socket.value?.readyState !== WebSocket.OPEN) return
    socket.value.send(Uint8Array.from(data, (character) => character.charCodeAt(0)))
  })
  resizeObserver = new ResizeObserver(resizePty)
  resizeObserver.observe(terminalHost.value)
  resizePty()
}

onMounted(initializeTerminal)
onBeforeUnmount(() => {
  disconnect()
  resizeObserver?.disconnect()
  if (resizeFrame !== undefined) cancelAnimationFrame(resizeFrame)
  dataDisposable?.dispose()
  binaryDisposable?.dispose()
  emulator?.dispose()
})
</script>

<template>
  <div class="page-stack developer-page">
    <PageHeader eyebrow="DEVELOPMENT BUILD ONLY" title="开发者模式" description="此模块只在 development 构建中编译；production 构建不会包含界面、路由、权限字符串或终端客户端。">
      <template #actions><button class="button button--secondary" type="button" :disabled="connectionState === 'connecting'" @click="connectionState === 'connected' ? disconnect() : connect()"><Plus :size="16" />{{ connectionState === 'connected' ? '断开会话' : connectionState === 'connecting' ? '连接中…' : '新建会话' }}</button></template>
    </PageHeader>
    <div class="dev-warning"><ShieldAlert :size="19" /><div><strong>宿主机 Shell 边界</strong><p>正式连接由一次性授权 WebSocket 建立，继承启动服务的非 root UID，不映射网站用户，不使用 sudo 或 setuid。</p></div><StatusBadge label="开发构建" tone="warning" /></div>

    <section class="terminal-shell">
      <header><div><SquareTerminal :size="17" /><strong>{{ sessionId ? `local-development · ${sessionId.slice(0, 8)}` : 'local-development' }}</strong><span><Circle :size="8" fill="currentColor" />{{ connectionLabel }}</span></div><button type="button" aria-label="清空终端滚动缓冲区" @click="emulator?.clear()"><Trash2 :size="16" /></button></header>
      <div ref="terminalHost" class="terminal-host" role="application" aria-label="AIRTEKPOWER 开发终端" />
    </section>

    <section class="dev-command-grid">
      <article class="panel"><Code2 :size="18" /><h2>平台 CLI</h2><p>开发数据库、同步、校验、索引、缓存、任务与诊断命令。</p><code>airtekctl --help</code></article>
      <article class="panel"><SquareTerminal :size="18" /><h2>完整 PTY</h2><p>支持交互式输入、ANSI 输出、窗口 resize、空闲/绝对超时与退出码；页面断开时立即关闭终端。</p><code>shell: inherited uid</code></article>
    </section>
  </div>
</template>

<style scoped>
.developer-page {
  min-height: calc(100vh - 7rem);
}

.dev-warning {
  display: flex;
  align-items: center;
  gap: 0.7rem;
  padding: 0.72rem 0.8rem;
  border: 1px solid #e6d8a6;
  border-radius: 10px;
  background: #fff9e7;
  color: var(--admin-muted);
  font-size: 0.72rem;
}

.dev-warning > svg {
  color: #936b00;
}

.dev-warning > div {
  flex: 1;
}

.dev-warning strong {
  display: block;
  color: var(--admin-text);
  font-size: 0.72rem;
}

.dev-warning p {
  margin: 0.1rem 0 0;
  font-size: 0.64rem;
}

.terminal-shell {
  overflow: hidden;
  border: 1px solid #173e48;
  border-radius: 12px;
  background: #071b22;
  box-shadow: 0 14px 36px rgb(0 25 32 / 14%);
  color: #d7e6e7;
}

.terminal-shell > header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 2.7rem;
  padding: 0 0.8rem;
  border-bottom: 1px solid rgb(255 255 255 / 8%);
  background: #0b252e;
}

.terminal-shell > header > div {
  display: flex;
  align-items: center;
  gap: 0.45rem;
}

.terminal-shell header strong {
  font-family: monospace;
  font-size: 0.68rem;
}

.terminal-shell header span {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  color: #65cb90;
  font-size: 0.57rem;
}

.terminal-shell header button {
  border: 0;
  background: none;
  color: #86a1a5;
}

.terminal-host {
  height: 26rem;
  padding: 0.75rem 0.4rem 0.6rem 0.75rem;
  text-align: left;
}

.terminal-host :deep(.xterm) {
  height: 100%;
}

.terminal-host :deep(.xterm-viewport) {
  scrollbar-color: #355b64 #071b22;
}

.dev-command-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0.75rem;
}

.dev-command-grid article {
  padding: 0.85rem;
}

.dev-command-grid article > svg {
  color: var(--airtek-blue);
}

.dev-command-grid h2 {
  margin: 0.55rem 0 0.2rem;
  font-size: 0.78rem;
}

.dev-command-grid p {
  color: var(--admin-muted);
  font-size: 0.64rem;
}

.dev-command-grid code {
  display: block;
  padding: 0.45rem;
  border-radius: 6px;
  background: #edf2f2;
  color: #355351;
  font-size: 0.63rem;
}

@media (max-width: 760px) {
  .dev-command-grid {
    grid-template-columns: 1fr;
  }
}
</style>
