import { redirect } from 'vike/abort'

export function data() {
  // The custom server returns the required 308 before rendering. This remains
  // as a routing fallback for adapters that invoke Vike directly.
  // Vike's redirect helper intentionally exposes only 301/302; production must
  // enter through apps/web/+server.ts, which returns the required 308.
  throw redirect('/en', 301)
}
