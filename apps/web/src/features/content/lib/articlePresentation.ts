import type { TiptapDocument } from '@airtek/contracts'
import { asRichTextNode, type RichTextNode } from '@/shared/lib/richText'

const controlCharacters = /[\u0000-\u001f\u007f]/u
const collapsibleWhitespace = /\s+/gu

export interface ArticleOutlineItem {
  id: string
  title: string
  level: 2 | 3 | 4
}

export interface ArticlePublicationMetadata {
  author?: string
  authorType?: 'Person' | 'Organization'
  publishedAt?: string
  publishedDate?: string
  category?: string
  updatedAt?: string
  updatedDate?: string
  outline: ArticleOutlineItem[]
}

function normalizedText(value: unknown, maximumLength: number): string | undefined {
  if (typeof value !== 'string' || controlCharacters.test(value)) return undefined
  const normalized = value.normalize('NFKC').replace(collapsibleWhitespace, ' ').trim()
  return normalized && normalized.length <= maximumLength ? normalized : undefined
}

function nodeText(node: RichTextNode | undefined): string {
  if (!node) return ''
  if (node.type === 'text') return node.text ?? ''
  const separator = ['doc', 'bulletList', 'orderedList', 'listItem', 'blockquote', 'table', 'tableRow'].includes(node.type) ? ' ' : ''
  return (node.content ?? []).map(nodeText).join(separator)
}

function headingLevel(node: RichTextNode): 2 | 3 | 4 | undefined {
  if (node.type !== 'heading') return undefined
  const requested = Number(node.attrs?.level)
  return requested === 2 || requested === 3 || requested === 4 ? requested : 2
}

function headingSlug(value: string): string {
  const slug = value
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/gu, '')
    .toLowerCase()
    .replace(/[^a-z0-9]+/gu, '-')
    .replace(/^-+|-+$/gu, '')
    .slice(0, 80)
    .replace(/-+$/gu, '')
  return slug || 'section'
}

export function articleOutlineHeadingTitle(node: RichTextNode): string | undefined {
  if (!headingLevel(node)) return undefined
  return normalizedText((node.content ?? []).map(nodeText).join(' '), 200)
}

function collectHeadings(node: RichTextNode, output: Array<{ title: string; level: 2 | 3 | 4 }>): void {
  const level = headingLevel(node)
  const title = articleOutlineHeadingTitle(node)
  if (level && title) output.push({ title, level })
  for (const child of node.content ?? []) collectHeadings(child, output)
}

export function extractArticleOutline(document: TiptapDocument | null | undefined): ArticleOutlineItem[] {
  const root = asRichTextNode(document)
  if (!root || root.type !== 'doc') return []
  const headings: Array<{ title: string; level: 2 | 3 | 4 }> = []
  collectHeadings(root, headings)
  const counts = new Map<string, number>()
  return headings.map((heading) => {
    const base = headingSlug(heading.title)
    const occurrence = (counts.get(base) ?? 0) + 1
    counts.set(base, occurrence)
    return { ...heading, id: occurrence === 1 ? base : `${base}-${occurrence}` }
  })
}
