import type { PageContextServer } from 'vike/types'
import { render } from 'vike/abort'
import { loadPublicPageData, PublicPageDataError } from '@/server/publicPageData'
import { serverPublicRuntimeConfig } from '@/server/runtimeConfig'

export async function data(pageContext: PageContextServer) {
  try {
    const runtimeConfig = serverPublicRuntimeConfig()
    return {
      ...await loadPublicPageData(pageContext.urlPathname),
      publicOrigin: runtimeConfig.publicOrigin,
      runtimeConfig,
    }
  } catch (cause) {
    if (cause instanceof PublicPageDataError) throw render(cause.status, cause.message)
    throw render(503, 'The public site projection is temporarily unavailable.')
  }
}

export type Data = Awaited<ReturnType<typeof data>>
