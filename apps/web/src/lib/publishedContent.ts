import type { ContentEntry, RichTextDocument } from '@airtek/contracts'
import { asRichTextNode, type RichTextNode } from '@/lib/richText'

const controlCharacters = /[\u0000-\u001f\u007f]/u
const collapsibleWhitespace = /\s+/gu

export interface PublishedFaqItem {
  id: string
  question: string
  answer: string
  answerDocument: RichTextDocument
}

export interface PublishedFaqContent {
  items: PublishedFaqItem[]
  /** True only when at least one question exists and every question has a textual answer. */
  complete: boolean
  editorialDocument?: RichTextDocument
}

export interface ArticleOutlineItem {
  id: string
  title: string
  level: 2 | 3 | 4
}

export interface PublishedArticleMetadata {
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
  // Inline nodes within a paragraph or heading concatenate exactly as authored;
  // containers that separate blocks add whitespace for plain-text schema output.
  const separator = ['doc', 'bulletList', 'orderedList', 'listItem', 'blockquote', 'table', 'tableRow'].includes(node.type) ? ' ' : ''
  return (node.content ?? []).map(nodeText).join(separator)
}

function normalizedNodeText(nodes: RichTextNode[], maximumLength: number): string | undefined {
  return normalizedText(nodes.map(nodeText).join(' '), maximumLength)
}

function headingLevel(node: RichTextNode): 2 | 3 | 4 | undefined {
  if (node.type !== 'heading') return undefined
  const requested = Number(node.attrs?.level)
  return requested === 2 || requested === 3 || requested === 4 ? requested : 2
}

function documentFor(nodes: RichTextNode[]): RichTextDocument {
  return { schemaVersion: 1, doc: { type: 'doc', content: nodes } }
}

/**
 * FAQ records use the existing versioned document rather than a parallel source.
 * A top-level H2-H4 ending in "?" starts a question; following non-heading blocks
 * form its answer until the next heading. Incomplete pairs stay out of the public
 * accordion and make the document ineligible for FAQPage structured data.
 */
export function extractPublishedFaqContent(document: RichTextDocument | undefined): PublishedFaqContent {
  if (!document || document.schemaVersion !== 1) return { items: [], complete: false }
  const root = asRichTextNode(document.doc)
  if (!root || root.type !== 'doc') return { items: [], complete: false }

  const nodes = root.content ?? []
  const consumed = new Set<number>()
  const items: PublishedFaqItem[] = []
  let questionCount = 0

  for (let index = 0; index < nodes.length; index += 1) {
    const node = nodes[index]
    if (!headingLevel(node)) continue
    const rawQuestion = normalizedNodeText([node], 2_000)
    if (!rawQuestion?.endsWith('?')) continue

    questionCount += 1
    let end = index + 1
    while (end < nodes.length && !headingLevel(nodes[end])) end += 1
    for (let consumedIndex = index; consumedIndex < end; consumedIndex += 1) consumed.add(consumedIndex)

    const question = normalizedText(rawQuestion, 300)
    const answerNodes = nodes.slice(index + 1, end)
    const answer = normalizedNodeText(answerNodes, 10_000)
    if (question && answer) {
      items.push({
        id: `faq-${questionCount}`,
        question,
        answer,
        answerDocument: documentFor(answerNodes),
      })
    }
    index = end - 1
  }

  const editorialNodes = nodes.filter((_, index) => !consumed.has(index))
  return {
    items,
    complete: questionCount > 0 && items.length === questionCount,
    ...(editorialNodes.length ? { editorialDocument: documentFor(editorialNodes) } : {}),
  }
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
  return headingLevel(node) ? normalizedNodeText([node], 200) : undefined
}

function collectHeadings(node: RichTextNode, output: Array<{ title: string; level: 2 | 3 | 4 }>): void {
  const level = headingLevel(node)
  const title = articleOutlineHeadingTitle(node)
  if (level && title) output.push({ title, level })
  for (const child of node.content ?? []) collectHeadings(child, output)
}

export function extractArticleOutline(document: RichTextDocument | undefined): ArticleOutlineItem[] {
  if (!document || document.schemaVersion !== 1) return []
  const root = asRichTextNode(document.doc)
  if (!root || root.type !== 'doc') return []
  const headings: Array<{ title: string; level: 2 | 3 | 4 }> = []
  collectHeadings(root, headings)
  const counts = new Map<string, number>()
  return headings.map((heading) => {
    const base = headingSlug(heading.title)
    const occurrence = (counts.get(base) ?? 0) + 1
    counts.set(base, occurrence)
    return {
      ...heading,
      id: occurrence === 1 ? base : `${base}-${occurrence}`,
    }
  })
}

function validDate(value: unknown): { value: string; date: string } | undefined {
  const text = normalizedText(value, 40)
  if (!text || !/^\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2}))?$/u.test(text)) return undefined
  const [year, month, day] = text.slice(0, 10).split('-').map(Number)
  const calendarDate = new Date(Date.UTC(year, month - 1, day))
  if (calendarDate.getUTCFullYear() !== year || calendarDate.getUTCMonth() !== month - 1 || calendarDate.getUTCDate() !== day) return undefined
  const parsed = new Date(text)
  const date = text.slice(0, 10)
  if (!Number.isFinite(parsed.valueOf())) return undefined
  return { value: text, date }
}

/**
 * Article presentation metadata is optional and must be explicit. The current
 * public contract stores it in the published document root attributes:
 * `{ author, authorType, publishedAt, category }`. No route label or update timestamp is
 * reinterpreted as one of those editorial facts.
 */
export function extractPublishedArticleMetadata(content: ContentEntry | undefined): PublishedArticleMetadata {
  if (!content || content.kind !== 'article' || content.status !== 'published' || content.body.schemaVersion !== 1) {
    return { outline: [] }
  }
  const root = asRichTextNode(content.body.doc)
  if (!root || root.type !== 'doc') return { outline: [] }
  const attrs = root.attrs ?? {}
  const author = normalizedText(attrs.author, 160)
  const authorType = attrs.authorType === 'Person' || attrs.authorType === 'Organization' ? attrs.authorType : undefined
  const published = validDate(attrs.publishedAt)
  const category = normalizedText(attrs.category, 120)
  const updated = validDate(content.updatedAt)
  return {
    ...(author ? { author } : {}),
    ...(author && authorType ? { authorType } : {}),
    ...(published ? { publishedAt: published.value, publishedDate: published.date } : {}),
    ...(category ? { category } : {}),
    ...(updated ? { updatedAt: updated.value, updatedDate: updated.date } : {}),
    outline: extractArticleOutline(content.body),
  }
}
