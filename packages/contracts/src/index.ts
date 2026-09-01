export type * from './generated/openapi'

import type {
  DiscoveryDocument,
  DiscoveryEntry,
  ProblemDetails,
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

export class ApiError extends Error {
  constructor(public readonly problem: ProblemDetails) {
    super(problem.detail || problem.title)
  }
}

export interface ApiClientOptions {
  baseUrl: string
  fetchImpl?: typeof fetch
  getCsrfToken?: () => string | undefined
}

/**
 * Small transport helper retained for application code. Request and response
 * payload types come from the generated OpenAPI exports above.
 */
export function createApiClient(options: ApiClientOptions) {
  const fetchImpl = options.fetchImpl ?? fetch

  async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const csrfToken = options.getCsrfToken?.()
    const headers = new Headers(init.headers)
    headers.set('Accept', 'application/json')
    if (init.body && !headers.has('Content-Type')) headers.set('Content-Type', 'application/json')
    if (csrfToken) headers.set('X-CSRF-Token', csrfToken)

    const response = await fetchImpl(`${options.baseUrl}${path}`, {
      ...init,
      credentials: 'include',
      headers,
    })

    if (!response.ok) {
      const fallback: ProblemDetails = {
        type: 'about:blank',
        title: response.statusText || 'Request failed',
        status: response.status,
        detail: response.statusText || 'Request failed',
        requestId: '00000000-0000-0000-0000-000000000000',
      }
      throw new ApiError(await response.json().catch(() => fallback) as ProblemDetails)
    }

    if (response.status === 204) return undefined as T
    return await response.json() as T
  }

  return {
    get: <T>(path: string, init?: RequestInit) => request<T>(path, init),
    post: <T>(path: string, body: unknown, init?: RequestInit) => request<T>(path, {
      ...init,
      method: 'POST',
      body: JSON.stringify(body),
    }),
    patch: <T>(path: string, body: unknown, etag: string, init?: RequestInit) => request<T>(path, {
      ...init,
      method: 'PATCH',
      body: JSON.stringify(body),
      headers: { ...Object.fromEntries(new Headers(init?.headers)), 'If-Match': etag },
    }),
  }
}
