import type { PageContext } from 'vike/types'

export function route(pageContext: PageContext) {
  const { urlPathname } = pageContext
  if (urlPathname === '/en' || urlPathname.startsWith('/en/')) {
    return { routeParams: { publicPath: urlPathname.slice(3) } }
  }
  return false
}
