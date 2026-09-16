import type { NewsEntryResponse } from './publicApiTypes'
import type { CardEntry } from '@/types/content'

const slugPattern = /^[a-z0-9]+(?:-[a-z0-9]+)*$/u

export function publishedNewsCard(entry: NewsEntryResponse): CardEntry | undefined {
  const content = entry.content
  const developmentFixture = entry.dataClass === 'developmentFixture'
  if ((content.isPlaceholder && !developmentFixture)
    || !content.publishedRevision
    || !content.slug
    || !slugPattern.test(content.slug)) return undefined
  return {
    slug: content.slug,
    title: content.title,
    summary: content.summary ?? '',
    eyebrow: entry.category ?? undefined,
    href: `/en/resources/news/${content.slug}`,
    category: entry.category ?? undefined,
    author: entry.authorDisplayName ?? undefined,
    publishedAt: entry.publishedAt ?? undefined,
    featured: entry.featured,
  }
}
