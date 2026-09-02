import { fileURLToPath, URL } from 'node:url'
import type { ServerResponse } from 'node:http'
import vue from '@vitejs/plugin-vue'
import { defineConfig, loadEnv, type Connect, type Plugin } from 'vite'

const DEVTOOLS_MODULE = 'virtual:devtools-routes'
const RESOLVED_DEVTOOLS_MODULE = `\0${DEVTOOLS_MODULE}`
const WORKSPACE_ROOT = fileURLToPath(new URL('../..', import.meta.url))

function setIsolationHeaders(
  _req: Connect.IncomingMessage,
  res: ServerResponse,
  connectSources: string,
): void {
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive')
  res.setHeader('X-Content-Type-Options', 'nosniff')
  res.setHeader('Referrer-Policy', 'no-referrer')
  res.setHeader('Permissions-Policy', 'camera=(), geolocation=(), microphone=()')
  res.setHeader(
    'Content-Security-Policy',
    `default-src 'self'; base-uri 'none'; object-src 'none'; frame-ancestors 'none'; img-src 'self' data: blob:; font-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' ${connectSources}; form-action 'self'`,
  )
}

function adminIsolationMiddleware(connectSources: string): Connect.NextHandleFunction {
  return (req, res, next) => {
    setIsolationHeaders(req, res, connectSources)
    const pathname = new URL(req.url ?? '/', 'http://admin.local').pathname
    const isSitemap = /^\/sitemap(?:-[^/]+)?\.xml$/i.test(pathname)
    const isPublicLocaleRoute = pathname === '/en' || pathname.startsWith('/en/')
    const isPublicManifest = /^\/(?:site\.)?manifest(?:\.webmanifest|\.json)$/i.test(pathname)
    const isVerificationFile = /^\/(?:google[a-z0-9_-]+\.html|BingSiteAuth\.xml|yandex_[a-z0-9_-]+\.html)$/i.test(pathname)

    if (isSitemap || isPublicLocaleRoute || isPublicManifest || isVerificationFile) {
      res.statusCode = 404
      res.setHeader('Content-Type', 'text/plain; charset=utf-8')
      res.end('Not Found')
      return
    }

    next()
  }
}

function adminIsolationPlugin(connectSources: string): Plugin {
  return {
    name: 'airtek-admin-isolation',
    configureServer(server) {
      server.middlewares.use(adminIsolationMiddleware(connectSources))
    },
    configurePreviewServer(server) {
      server.middlewares.use(adminIsolationMiddleware(connectSources))
    },
  }
}

function apiConnectSources(value: string | undefined): string {
  try {
    const url = new URL(value || 'http://localhost:8080/api/admin/v1')
    if (url.protocol !== 'http:' && url.protocol !== 'https:') throw new Error('unsupported protocol')
    const websocketProtocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
    return `${url.origin} ${websocketProtocol}//${url.host}`
  } catch {
    throw new Error('VITE_ADMIN_API_BASE_URL must be an absolute HTTP(S) URL.')
  }
}

function devtoolsRoutesPlugin(enabled: boolean): Plugin {
  return {
    name: 'airtek-admin-devtools-boundary',
    resolveId(id) {
      if (id === DEVTOOLS_MODULE) return RESOLVED_DEVTOOLS_MODULE
      return null
    },
    load(id) {
      if (id !== RESOLVED_DEVTOOLS_MODULE) return null
      if (!enabled) return 'export const devtoolsRoutes = []; export const devtoolsNavigation = []; export const devtoolsPermissions = []'

      return `
        import { TerminalSquare } from 'lucide-vue-next'
        export const devtoolsRoutes = [{
          path: '/developer-tools',
          name: 'developer-tools',
          component: () => import('/src/devtools/DevToolsView.vue'),
          meta: { requiresAuth: true, permission: 'devtools.shell', section: 'system' }
        }]
        export const devtoolsNavigation = [{
          label: '开发者模式',
          to: '/developer-tools',
          icon: TerminalSquare,
          permission: 'devtools.shell',
          devOnly: true
        }]
        export const devtoolsPermissions = ['devtools.shell']
      `
    },
  }
}

export default defineConfig(({ command, mode }) => {
  const env = loadEnv(mode, WORKSPACE_ROOT, '')
  const productionBuild = command === 'build' && mode === 'production'
  const requestedDevtools = env.VITE_ENABLE_DEVTOOLS === 'true'
  const devtoolsEnabled = mode === 'development' && requestedDevtools
  const connectSources = apiConnectSources(env.VITE_ADMIN_API_BASE_URL)

  if (productionBuild && env.VITE_ENABLE_DEVTOOLS === 'true') {
    throw new Error('Production configuration cannot compile the Admin DevTools package.')
  }

  return {
    envDir: WORKSPACE_ROOT,
    plugins: [vue(), adminIsolationPlugin(connectSources), devtoolsRoutesPlugin(devtoolsEnabled)],
    resolve: {
      alias: {
        '@': fileURLToPath(new URL('./src', import.meta.url)),
      },
    },
    define: {
      __AIRTEK_DEVTOOLS__: JSON.stringify(devtoolsEnabled),
    },
    server: {
      host: '0.0.0.0',
      port: 3100,
      strictPort: true,
    },
    preview: {
      host: '0.0.0.0',
      port: 3100,
      strictPort: true,
    },
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      sourcemap: false,
      manifest: true,
    },
  }
})
