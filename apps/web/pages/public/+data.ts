import type { ProductFamily, PublicSearchType } from '@airtek/contracts'
import type { PageContextServer } from 'vike/types'
import { render } from 'vike/abort'
import { loadPublicPageData, PublicPageDataError } from '@/server/publicPageData'
import { normalizePublicOrigin } from '@/shared/lib/publicOrigin'

export async function data(pageContext: PageContextServer) {
  try {
    const productFamily = pageContext.urlParsed.search.family
    const supportedFamilies: ProductFamily[] = ['centrifugal', 'axial', 'crossFlow', 'inlineDuct', 'motors']
    const searchType = pageContext.urlParsed.search.type
    const supportedSearchTypes: PublicSearchType[] = [
      'product', 'solution', 'technology', 'article', 'news', 'faq', 'caseStudy', 'download', 'company', 'page',
    ]
    const q = pageContext.urlParsed.search.q?.slice(0, 200)
    const cursor = pageContext.urlParsed.search.cursor?.slice(0, 4096)
    const motorTechnology = pageContext.urlParsed.search.motorTechnology?.slice(0, 120)
    const view = pageContext.urlParsed.search.view === 'table' ? 'table' : 'cards'
    const projection = await loadPublicPageData(pageContext.urlPathname, {
      productSlug: pageContext.urlParsed.search.product,
      productFamily: supportedFamilies.find((family) => family === productFamily),
      catalogQuery: {
        ...(cursor ? { cursor } : {}),
        ...(q ? { q } : {}),
        ...(supportedFamilies.includes(productFamily as ProductFamily) ? { family: productFamily as ProductFamily } : {}),
        ...(motorTechnology ? { motorTechnology } : {}),
        view,
      },
      searchQuery: {
        ...(cursor ? { cursor } : {}),
        ...(q ? { q } : {}),
        ...(supportedSearchTypes.includes(searchType as PublicSearchType) ? { type: searchType as PublicSearchType } : {}),
      },
    })
    return {
      ...projection,
      publicOrigin: normalizePublicOrigin(process.env.PUBLIC_ORIGIN || import.meta.env.VITE_PUBLIC_ORIGIN),
    }
  } catch (cause) {
    if (cause instanceof PublicPageDataError) {
      throw render(cause.status, cause.message)
    }
    throw render(503, 'The public site projection is temporarily unavailable.')
  }
}

export type Data = Awaited<ReturnType<typeof data>>
