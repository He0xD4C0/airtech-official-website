import type { PageContextServer } from 'vike/types'
import { render } from 'vike/abort'
import { ContentPreviewLoadError, loadContentPreview } from '@/lib/server/contentPreview'

export async function data(pageContext: PageContextServer) {
  const token = pageContext.urlParsed.search.token
  if (typeof token !== 'string') throw render(404, 'Content preview was not found.')
  try {
    return await loadContentPreview(token)
  } catch (error) {
    if (error instanceof ContentPreviewLoadError && error.statusCode === 410) {
      throw render(410, 'Content preview has expired.')
    }
    throw render(404, 'Content preview was not found.')
  }
}

export type Data = Awaited<ReturnType<typeof data>>
