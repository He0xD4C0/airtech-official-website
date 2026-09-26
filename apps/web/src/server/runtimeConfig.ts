import { normalizePublicOrigin } from '@/shared/lib/publicOrigin'
import type { PublicRuntimeConfig } from '@/shared/lib/runtimeConfig'

function requiredOrigin(name: 'PUBLIC_API_BROWSER_ORIGIN' | 'PUBLIC_ORIGIN', fallback: string): string {
  const value = process.env[name] || fallback
  try {
    return normalizePublicOrigin(value)
  } catch {
    throw new Error(`${name} must be a bare HTTP(S) origin.`)
  }
}

export function serverPublicRuntimeConfig(): PublicRuntimeConfig {
  const apiOrigin = requiredOrigin(
    'PUBLIC_API_BROWSER_ORIGIN',
    import.meta.env.VITE_PUBLIC_API_BASE_URL
      ? new URL(import.meta.env.VITE_PUBLIC_API_BASE_URL).origin
      : 'http://localhost:8080',
  )
  return {
    apiBaseUrl: `${apiOrigin}/api/public/v1`,
    publicOrigin: requiredOrigin('PUBLIC_ORIGIN', import.meta.env.VITE_PUBLIC_ORIGIN || 'http://localhost:3000'),
  }
}
