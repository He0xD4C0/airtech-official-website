import { fileURLToPath, URL } from 'node:url'
import vue from '@vitejs/plugin-vue'
import vike from 'vike/plugin'
import { defineConfig, loadEnv } from 'vite'

const WORKSPACE_ROOT = fileURLToPath(new URL('../..', import.meta.url))

export default defineConfig(({ mode }) => {
  const workspaceEnv = loadEnv(mode, WORKSPACE_ROOT, '')
  // Vite only exposes VITE_* keys to browser code. Copy the three explicit SSR
  // runtime keys into this development process so `pnpm dev:web` also honors
  // the root .env; production containers still inject them at runtime.
  for (const key of ['PUBLIC_API_INTERNAL_URL', 'PUBLIC_API_BROWSER_ORIGIN', 'PUBLIC_ORIGIN']) {
    if (!process.env[key] && workspaceEnv[key]) process.env[key] = workspaceEnv[key]
  }

  return {
    envDir: WORKSPACE_ROOT,
    plugins: [vue(), vike()],
    resolve: {
      alias: {
        '@': fileURLToPath(new URL('./src', import.meta.url)),
      },
    },
    ssr: {
      // vue-i18n's production build contains Vue feature flags that Vite must
      // replace at bundle time; externalizing it leaves those globals undefined.
      noExternal: ['vue-i18n'],
    },
    server: {
      host: '0.0.0.0',
      port: 3000,
      strictPort: true,
    },
    preview: {
      host: '0.0.0.0',
      port: 3000,
      strictPort: true,
    },
  }
})
