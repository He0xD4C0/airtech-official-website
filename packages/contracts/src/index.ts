export type * from './generated/openapi'
export * from './client'

import type {
  DiscoveryDocument,
  DiscoveryEntry,
} from './generated/openapi'

export type Locale = 'en'

export type JsonPrimitive = string | number | boolean | null
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue }

/** Compatibility alias used by the public sitemap client. */
export type PublicDiscoveryEntry = DiscoveryEntry
/** Compatibility alias used by the public sitemap client. */
export type PublicDiscoveryDocument = DiscoveryDocument

/** Generic convenience shape for consumers outside a concrete generated path. */
export interface CursorPage<T> {
  items: T[]
  nextCursor: string | null
}
