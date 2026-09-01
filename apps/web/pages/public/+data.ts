import type { PageContextServer } from 'vike/types'
import { resolvePublicRoute } from '@/content/routes'
import { loadPublishedProjection } from '@/lib/server/publicProjection'
import { normalizePublicOrigin } from '@/lib/publicOrigin'

export async function data(pageContext: PageContextServer) {
  const route = resolvePublicRoute(pageContext.urlPathname)
  return {
    page: await loadPublishedProjection(route, {
      productSlug: pageContext.urlParsed.search.product,
    }),
    publicOrigin: normalizePublicOrigin(process.env.PUBLIC_ORIGIN || import.meta.env.VITE_PUBLIC_ORIGIN),
  }
}

export type Data = Awaited<ReturnType<typeof data>>
