const localPublicOrigin = 'http://localhost:3000'

export function normalizePublicOrigin(value: string | undefined): string {
  const url = new URL(value || localPublicOrigin)
  if (!['http:', 'https:'].includes(url.protocol)
    || url.username
    || url.password
    || url.pathname !== '/'
    || url.search
    || url.hash) {
    throw new Error('The canonical public origin must be an absolute http(s) origin without credentials or a path.')
  }
  return url.origin
}

export function buildTimePublicOrigin(): string {
  return normalizePublicOrigin(import.meta.env.VITE_PUBLIC_ORIGIN)
}
