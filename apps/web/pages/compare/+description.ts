import type { PageContext } from 'vike/types'
import type { Data } from './+data'

export default function description(pageContext: PageContext<Data>) {
  return pageContext.data.page.description
}
