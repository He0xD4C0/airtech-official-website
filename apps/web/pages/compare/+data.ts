import type { PageContextServer } from 'vike/types'
import { render } from 'vike/abort'
import { loadPublicPageData, PublicPageDataError } from '@/lib/server/publicPageData'
import { normalizePublicOrigin } from '@/lib/publicOrigin'

export async function data(pageContext: PageContextServer) {
  try {
    return {
      ...await loadPublicPageData(pageContext.urlPathname),
      publicOrigin: normalizePublicOrigin(process.env.PUBLIC_ORIGIN || import.meta.env.VITE_PUBLIC_ORIGIN),
    }
  } catch (cause) {
    if (cause instanceof PublicPageDataError) throw render(cause.status, cause.message)
    throw render(503, 'The public site projection is temporarily unavailable.')
  }
}

export type Data = Awaited<ReturnType<typeof data>>
