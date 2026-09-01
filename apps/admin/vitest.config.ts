import { fileURLToPath, URL } from 'node:url'
import vue from '@vitejs/plugin-vue'
import { defineConfig, type Plugin } from 'vitest/config'

const DEVTOOLS_MODULE = 'virtual:devtools-routes'
const RESOLVED_DEVTOOLS_MODULE = `\0${DEVTOOLS_MODULE}`

function testDevtoolsBoundary(): Plugin {
  return {
    name: 'airtek-admin-test-devtools-boundary',
    resolveId(id) {
      return id === DEVTOOLS_MODULE ? RESOLVED_DEVTOOLS_MODULE : null
    },
    load(id) {
      return id === RESOLVED_DEVTOOLS_MODULE
        ? 'export const devtoolsRoutes = []; export const devtoolsNavigation = []; export const devtoolsPermissions = []'
        : null
    },
  }
}

export default defineConfig({
  plugins: [vue(), testDevtoolsBoundary()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  define: {
    __AIRTEK_DEVTOOLS__: 'false',
  },
})
